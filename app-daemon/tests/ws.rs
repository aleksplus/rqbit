//! Integration tests: request mapping against a quiet session, and a real
//! WebSocket round-trip through the axum server.

use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt as _, StreamExt as _};
use librqbit::{Api, ConnectionOptions, Session, SessionOptions};
use rqbit_protocol as proto;
use rqbit_protocol::codec;
use rqbit_protocol::{ClientMessage, ServerMessage};

/// A minimal real session: no DHT, no listener, no persistence.
async fn quiet_api() -> Arc<Api> {
    let session = Session::new_with_opts(
        std::env::temp_dir(),
        SessionOptions {
            dht: None,
            persistence: None,
            listen: None,
            connect: Some(ConnectionOptions::default()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    Arc::new(Api::new(session, None, None))
}

fn request(id: u64, req: proto::Request) -> ClientMessage {
    ClientMessage::Request { id, req }
}

/// `handle_request` maps the shared UI's API surface 1:1: the happy paths
/// return payloads and the error paths carry the right error kind.
#[tokio::test]
async fn handle_request_maps_api_surface() {
    let api = quiet_api().await;

    // Session stats: an empty session still reports speeds/uptime.
    let resp = server::handle_request(&api, proto::Request::SessionStats)
        .await
        .unwrap();
    assert!(matches!(resp, proto::ResponsePayload::SessionStats(_)));

    // List: empty session → empty list.
    let resp = server::handle_request(&api, proto::Request::List { with_stats: true })
        .await
        .unwrap();
    match resp {
        proto::ResponsePayload::List(list) => assert!(list.torrents.is_empty()),
        other => panic!("unexpected payload: {other:?}"),
    }

    // DHT is disabled in this session → DhtDisabled error kind.
    let err = server::handle_request(&api, proto::Request::DhtStats)
        .await
        .unwrap_err();
    assert_eq!(err.kind, proto::ProtoErrorKind::DhtDisabled);

    // Addressing by a well-formed-but-unknown info hash → TorrentNotFound.
    let bogus = proto::TorrentId::Hash("0".repeat(40));
    let err = server::handle_request(&api, proto::Request::Details { torrent: bogus.clone() })
        .await
        .unwrap_err();
    assert_eq!(err.kind, proto::ProtoErrorKind::TorrentNotFound);

    // Actions on unknown torrents fail, but with a decodable wire error.
    let err = server::handle_request(
        &api,
        proto::Request::Action {
            torrent: bogus,
            action: proto::TorrentAction::Pause,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind, proto::ProtoErrorKind::TorrentNotFound);

    // Malformed ids (bad hash length) → BadRequest.
    let err = server::handle_request(
        &api,
        proto::Request::Stats {
            torrent: proto::TorrentId::Hash("nope".into()),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind, proto::ProtoErrorKind::BadRequest);
}

async fn spawn_server() -> String {
    let api = quiet_api().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = app_daemon::server::router(api, None);
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("ws://{addr}/ws")
}

/// Full binary-protocol round-trip over a real WebSocket: Ping→Pong,
/// request→typed error response, and Subscribe→pushed TorrentList.
#[tokio::test]
async fn websocket_roundtrip() {
    let url = spawn_server().await;
    let (ws, _resp) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let (mut write, mut read) = ws.split();

    // Ping → Pong.
    write
        .send(tokio_tungstenite::tungstenite::Message::binary(
            codec::encode_client(&ClientMessage::Ping).unwrap(),
        ))
        .await
        .unwrap();

    let mut got_pong = false;
    while !got_pong {
        let msg = read.next().await.unwrap().unwrap();
        if let tokio_tungstenite::tungstenite::Message::Binary(bytes) = msg {
            match codec::decode_server(&bytes).unwrap() {
                ServerMessage::Pong => got_pong = true,
                other => panic!("expected pong, got {other:?}"),
            }
        }
    }

    // Subscribe (session level), then expect a pushed TorrentList quickly.
    write
        .send(tokio_tungstenite::tungstenite::Message::binary(
            codec::encode_client(&ClientMessage::Subscribe { torrent: None }).unwrap(),
        ))
        .await
        .unwrap();

    let got_list = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let msg = read.next().await.unwrap().unwrap();
            let tokio_tungstenite::tungstenite::Message::Binary(bytes) = msg else { continue };
            match codec::decode_server(&bytes).unwrap() {
                ServerMessage::Push(proto::PushUpdate::TorrentList(_)) => return true,
                _ => continue,
            }
        }
    })
    .await;
    assert!(got_list.is_ok(), "no TorrentList push within 5s");

    // A request with a correlated id → typed error response.
    write
        .send(tokio_tungstenite::tungstenite::Message::binary(
            codec::encode_client(&request(
                42,
                proto::Request::Details {
                    torrent: proto::TorrentId::Hash("0".repeat(40)),
                },
            ))
            .unwrap(),
        ))
        .await
        .unwrap();

    let mut got_response = false;
    while !got_response {
        let msg = read.next().await.unwrap().unwrap();
        let tokio_tungstenite::tungstenite::Message::Binary(bytes) = msg else { continue };
        match codec::decode_server(&bytes).unwrap() {
            ServerMessage::Response { id, result } => {
                assert_eq!(id, 42);
                assert_eq!(result.unwrap_err().kind, proto::ProtoErrorKind::TorrentNotFound);
                got_response = true;
            }
            _ => continue, // pushes may interleave
        }
    }
}

use app_daemon::server;
