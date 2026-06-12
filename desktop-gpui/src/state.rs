// Created shared state abstraction
use std::sync::Arc;
use crate::config::RqbitDesktopConfig;
use crate::ipc::IpcService;

/// Holds the current configuration and a handle to the IPC RPC service.
/// The struct is `Clone` so it can be stored in GPUI’s context and passed
/// to any component.
#[derive(Clone)]
pub struct SharedState {
    /// Current configuration loaded from disk or edited in the UI.
    pub config: RqbitDesktopConfig,
    /// Shared reference to the IPC service that talks to the librqbit session.
    pub ipc: Arc<ipc::IpcService>,
}

/// Helper trait that allows any `gpui::Context` to quickly obtain the IPC service.
pub trait IpcExt {
    fn ipc(&self) -> Arc<IpcService>;
}

impl<T> IpcExt for gpui::Context<T>
where
    T: gpui::Render,
{
    fn ipc(&self) -> Arc<IpcService> {
        self.context::<SharedState>().ipc.clone()
    }
}
