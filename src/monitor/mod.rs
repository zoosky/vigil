pub mod http;
pub mod ping;
pub mod state;
pub mod tcp;
pub mod traceroute;

pub use http::check_http;
pub use ping::PingMonitor;
pub use state::{ConnectivityTracker, StateEvent, TargetState};
pub use tcp::check_tcp;
pub use traceroute::{format_traceroute, HopAnalyzer};

use crate::models::{MonitorMethod, PingResult, Target};

/// Unified connectivity check that supports multiple methods (TCP, Ping, HTTP)
///
/// This is the main entry point for checking connectivity to a target.
/// It dispatches to the appropriate method based on the target's configuration.
///
/// The `process_timeout_ms` is a hard timeout on subprocess execution (for ping).
/// If not specified, defaults to 3x timeout_ms.
pub async fn check_connectivity(target: &Target, timeout_ms: u64) -> PingResult {
    check_connectivity_with_process_timeout(target, timeout_ms, timeout_ms * 3).await
}

/// Unified connectivity check with explicit process timeout
///
/// The `process_timeout_ms` is a hard timeout on subprocess execution.
/// This prevents the monitor from hanging when the network stack is in a bad state.
pub async fn check_connectivity_with_process_timeout(
    target: &Target,
    timeout_ms: u64,
    process_timeout_ms: u64,
) -> PingResult {
    match target.method {
        MonitorMethod::Ping => {
            ping::ping_target(&target.ip, &target.name, timeout_ms, process_timeout_ms).await
        }
        MonitorMethod::Tcp => {
            tcp::check_tcp(&target.ip, &target.name, target.port, timeout_ms).await
        }
        MonitorMethod::Http => {
            http::check_http(&target.ip, &target.name, target.port, timeout_ms).await
        }
    }
}
