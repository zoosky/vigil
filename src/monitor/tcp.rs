//! TCP connectivity monitoring
//!
//! Provides TCP-based connectivity checks as an alternative to ICMP ping.
//! TCP checks are more reliable because they reflect real application behavior
//! and are not affected by ICMP rate-limiting on routers/ISPs.

use crate::models::PingResult;
use chrono::Utc;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Check TCP connectivity to a host:port
///
/// Returns a PingResult for compatibility with the existing monitoring system.
/// The latency includes the full TCP handshake time.
pub async fn check_tcp(host: &str, name: &str, port: u16, timeout_ms: u64) -> PingResult {
    let addr = format!("{}:{}", host, port);
    let timestamp = Utc::now();
    let start = Instant::now();

    let result = timeout(Duration::from_millis(timeout_ms), TcpStream::connect(&addr)).await;

    let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

    match result {
        Ok(Ok(_stream)) => {
            // Connection successful - stream is dropped immediately (we just test connectivity)
            PingResult {
                target: host.to_string(),
                target_name: name.to_string(),
                timestamp,
                success: true,
                latency_ms: Some(latency_ms),
                error: None,
            }
        }
        Ok(Err(e)) => {
            // Connection failed (refused, network unreachable, etc.)
            PingResult {
                target: host.to_string(),
                target_name: name.to_string(),
                timestamp,
                success: false,
                latency_ms: None,
                error: Some(parse_tcp_error(&e)),
            }
        }
        Err(_) => {
            // Timeout
            PingResult {
                target: host.to_string(),
                target_name: name.to_string(),
                timestamp,
                success: false,
                latency_ms: None,
                error: Some("Connection timeout".to_string()),
            }
        }
    }
}

/// Parse TCP connection error into user-friendly message
fn parse_tcp_error(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::ConnectionRefused => "Connection refused".to_string(),
        std::io::ErrorKind::ConnectionReset => "Connection reset".to_string(),
        std::io::ErrorKind::ConnectionAborted => "Connection aborted".to_string(),
        std::io::ErrorKind::NotConnected => "Not connected".to_string(),
        std::io::ErrorKind::AddrNotAvailable => "Address not available".to_string(),
        std::io::ErrorKind::NetworkUnreachable => "Network unreachable".to_string(),
        std::io::ErrorKind::HostUnreachable => "Host unreachable".to_string(),
        std::io::ErrorKind::TimedOut => "Connection timeout".to_string(),
        _ => format!("Connection failed: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tcp_check_google() {
        // Google's DNS should be reachable on port 443
        let result = check_tcp("8.8.8.8", "Google DNS", 443, 5000).await;
        assert!(result.success, "TCP to 8.8.8.8:443 should succeed");
        assert!(result.latency_ms.is_some(), "Should have latency");
        assert!(
            result.latency_ms.unwrap() < 1000.0,
            "Latency should be < 1 second"
        );
    }

    #[tokio::test]
    async fn test_tcp_check_cloudflare() {
        // Cloudflare's DNS should be reachable on port 443
        let result = check_tcp("1.1.1.1", "Cloudflare", 443, 5000).await;
        assert!(result.success, "TCP to 1.1.1.1:443 should succeed");
    }

    #[tokio::test]
    async fn test_tcp_check_timeout() {
        // Non-routable IP should timeout
        let result = check_tcp("192.0.2.1", "Test", 443, 1000).await;
        assert!(!result.success, "TCP to non-routable IP should fail");
        assert!(result.error.is_some(), "Should have error message");
    }

    #[tokio::test]
    async fn test_tcp_check_refused() {
        // Localhost on unused port should be refused
        let result = check_tcp("127.0.0.1", "localhost", 59999, 1000).await;
        assert!(!result.success, "TCP to unused port should fail");
        assert!(result.error.is_some(), "Should have error message");
    }

    #[test]
    fn test_parse_tcp_error() {
        let refused = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        assert_eq!(parse_tcp_error(&refused), "Connection refused");

        let unreachable =
            std::io::Error::new(std::io::ErrorKind::NetworkUnreachable, "unreachable");
        assert_eq!(parse_tcp_error(&unreachable), "Network unreachable");
    }
}
