//! HTTP connectivity monitoring
//!
//! Provides HTTP-based connectivity checks that validate full HTTP connectivity.
//! HTTP checks are the most thorough since they test the entire network stack
//! including DNS resolution, TCP connection, TLS handshake, and HTTP protocol.

use crate::models::PingResult;
use chrono::Utc;
use reqwest::Client;
use std::time::{Duration, Instant};

/// Check HTTP connectivity to a host
///
/// Performs an HTTP HEAD request to validate full connectivity.
/// Returns a PingResult for compatibility with the existing monitoring system.
/// The latency includes DNS, TCP handshake, TLS handshake, and HTTP response.
pub async fn check_http(host: &str, name: &str, port: u16, timeout_ms: u64) -> PingResult {
    let timestamp = Utc::now();
    let start = Instant::now();

    // Build URL based on port
    let url = if port == 443 {
        format!("https://{}/", host)
    } else if port == 80 {
        format!("http://{}/", host)
    } else {
        // Custom port - assume HTTPS for non-standard ports
        format!("https://{}:{}/", host, port)
    };

    // Create client with timeout
    let client = match Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return PingResult {
                target: host.to_string(),
                target_name: name.to_string(),
                timestamp,
                success: false,
                latency_ms: None,
                error: Some(format!("Failed to create HTTP client: {}", e)),
            };
        }
    };

    // Perform HEAD request (lighter than GET)
    let result = client.head(&url).send().await;
    let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

    match result {
        Ok(response) => {
            let status = response.status();
            // Consider success: 2xx, 3xx redirects, and even some 4xx (server is responding)
            // We're testing connectivity, not content availability
            let success = status.is_success() || status.is_redirection() || status.as_u16() == 405;

            PingResult {
                target: host.to_string(),
                target_name: name.to_string(),
                timestamp,
                success,
                latency_ms: Some(latency_ms),
                error: if success {
                    None
                } else {
                    Some(format!("HTTP {}", status.as_u16()))
                },
            }
        }
        Err(e) => PingResult {
            target: host.to_string(),
            target_name: name.to_string(),
            timestamp,
            success: false,
            latency_ms: None,
            error: Some(parse_http_error(&e)),
        },
    }
}

/// Parse HTTP error into user-friendly message
fn parse_http_error(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "Connection timeout".to_string()
    } else if e.is_connect() {
        "Connection failed".to_string()
    } else if e.is_request() {
        "Request failed".to_string()
    } else if e.is_redirect() {
        "Too many redirects".to_string()
    } else {
        format!("HTTP error: {}", e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_http_check_google() {
        // Google should be reachable via HTTPS
        let result = check_http("www.google.com", "Google", 443, 5000).await;
        assert!(result.success, "HTTP to www.google.com:443 should succeed");
        assert!(result.latency_ms.is_some(), "Should have latency");
        assert!(
            result.latency_ms.unwrap() < 2000.0,
            "Latency should be < 2 seconds"
        );
    }

    #[tokio::test]
    async fn test_http_check_cloudflare() {
        // Cloudflare should be reachable via HTTPS
        let result = check_http("cloudflare.com", "Cloudflare", 443, 5000).await;
        assert!(
            result.success,
            "HTTP to cloudflare.com:443 should succeed: {:?}",
            result.error
        );
    }

    #[tokio::test]
    async fn test_http_check_timeout() {
        // Non-routable IP should timeout
        let result = check_http("192.0.2.1", "Test", 443, 1000).await;
        assert!(!result.success, "HTTP to non-routable IP should fail");
        assert!(result.error.is_some(), "Should have error message");
    }

    #[tokio::test]
    async fn test_http_check_invalid_host() {
        // Invalid hostname should fail
        let result = check_http("invalid.invalid.invalid", "Invalid", 443, 2000).await;
        assert!(!result.success, "HTTP to invalid host should fail");
        assert!(result.error.is_some(), "Should have error message");
    }

    #[tokio::test]
    async fn test_http_check_port_80() {
        // HTTP on port 80
        let result = check_http("www.google.com", "Google HTTP", 80, 5000).await;
        // Google typically redirects HTTP to HTTPS, which is still a success
        assert!(
            result.success,
            "HTTP to www.google.com:80 should succeed (redirect is OK)"
        );
    }
}
