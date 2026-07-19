//! Shared formatting helpers for the desktop UI.

/// Binary unit multipliers (powers of 1024), computed once and reused.
const KIB: f64 = 1024.0;
const MIB: f64 = KIB * 1024.0;
const GIB: f64 = MIB * 1024.0;
const TIB: f64 = GIB * 1024.0;

/// Format a byte count into a human-readable binary unit string.
///
/// `suffix` is appended after the unit (e.g. `""` for sizes, `"/s"` for
/// speeds), `decimals` controls the number of fractional digits, and `labels`
/// supplies the four unit prefixes for the TiB/GiB/MiB/KiB tiers (top to
/// bottom). Values below `KIB` are rendered as whole bytes.
fn format_binary(bytes: f64, suffix: &str, decimals: usize, labels: [&str; 4]) -> String {
    let (div, unit) = if bytes >= TIB {
        (TIB, labels[0])
    } else if bytes >= GIB {
        (GIB, labels[1])
    } else if bytes >= MIB {
        (MIB, labels[2])
    } else if bytes >= KIB {
        (KIB, labels[3])
    } else {
        return format!("{:.0} B{}", bytes, suffix);
    };
    format!("{:.1$} {}{}B{}", bytes / div, decimals, unit, suffix)
}

/// Format a byte count in human-readable form (binary units).
pub fn format_bytes(bytes: u64) -> String {
    format_binary(bytes as f64, "", 2, ["Ti", "Gi", "Mi", "Ki"])
}

/// Format a speed (given in Mbps) in human-readable form.
pub fn format_speed(mbps: f64) -> String {
    format_binary(mbps * MIB, "/s", 1, ["G", "G", "M", "K"])
}

/// Parse a human-readable speed string (e.g. `"1.2 MB/s"`, `"N/A"`) into a
/// comparable bytes-per-second value for sorting.
pub fn parse_speed(s: &str) -> f64 {
    let s = s.trim();
    if s == "N/A" {
        return 0.0;
    }
    // Try the longest suffix first so e.g. "MB/s" is not shadowed by "B/s".
    let candidates = [("GB/s", GIB), ("MB/s", MIB), ("KB/s", KIB), ("B/s", 1.0)];
    for (suffix, mult) in candidates {
        if let Some(v) = s.strip_suffix(suffix) {
            return v.trim().parse::<f64>().unwrap_or(0.0) * mult;
        }
    }
    0.0
}
