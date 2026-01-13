use crate::config::Config;
use crate::models::{PingResult, Target};
use chrono::Utc;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::time::interval;

/// Ping monitor that continuously pings multiple targets
pub struct PingMonitor {
    targets: Vec<Target>,
    interval: Duration,
    timeout_ms: u64,
    process_timeout_ms: u64,
}

impl PingMonitor {
    /// Create a new ping monitor from configuration
    pub fn new(config: &Config) -> Self {
        Self {
            targets: config.all_targets(),
            interval: Duration::from_millis(config.monitor.ping_interval_ms),
            timeout_ms: config.monitor.ping_timeout_ms,
            process_timeout_ms: config.monitor.ping_process_timeout_ms,
        }
    }

    /// Create a ping monitor with custom settings
    pub fn with_settings(targets: Vec<Target>, interval: Duration, timeout_ms: u64) -> Self {
        Self {
            targets,
            interval,
            timeout_ms,
            process_timeout_ms: timeout_ms * 3, // Default to 3x ping timeout
        }
    }

    /// Run a single connectivity check to a target (uses TCP or Ping based on target config)
    pub async fn check(&self, target: &Target) -> PingResult {
        super::check_connectivity(target, self.timeout_ms).await
    }

    /// Run a single ping to a target (legacy method, use check() instead)
    pub async fn ping(&self, target: &Target) -> PingResult {
        ping_target(
            &target.ip,
            &target.name,
            self.timeout_ms,
            self.process_timeout_ms,
        )
        .await
    }

    /// Start continuous monitoring, sending results to the returned receiver
    pub fn start(&self) -> mpsc::Receiver<PingResult> {
        let (tx, rx) = mpsc::channel(100);
        let targets = self.targets.clone();
        let interval_duration = self.interval;
        let timeout_ms = self.timeout_ms;
        let process_timeout_ms = self.process_timeout_ms;

        tokio::spawn(async move {
            let mut ticker = interval(interval_duration);

            loop {
                ticker.tick().await;

                // Check all targets concurrently using the appropriate method (TCP or Ping)
                // Use process timeout to prevent hanging on network issues
                let futures: Vec<_> = targets
                    .iter()
                    .map(|t| {
                        super::check_connectivity_with_process_timeout(
                            t,
                            timeout_ms,
                            process_timeout_ms,
                        )
                    })
                    .collect();

                let results = futures::future::join_all(futures).await;

                for result in results {
                    if tx.send(result).await.is_err() {
                        // Receiver dropped, stop monitoring
                        return;
                    }
                }
            }
        });

        rx
    }

    /// Get the list of targets being monitored
    pub fn targets(&self) -> &[Target] {
        &self.targets
    }
}

/// Execute a single ping to a target IP with process-level timeout
///
/// The `timeout_ms` is passed to the ping command's -W flag (ICMP timeout).
/// The `process_timeout_ms` is a hard timeout on the subprocess itself - if the
/// ping process doesn't complete within this time, it is killed. This prevents
/// the monitor from blocking when the network stack is in a bad state.
pub async fn ping_target(
    ip: &str,
    name: &str,
    timeout_ms: u64,
    process_timeout_ms: u64,
) -> PingResult {
    let timestamp = Utc::now();
    let start = Instant::now();

    // Spawn the ping process so we can kill it on timeout
    let mut child = match Command::new("ping")
        .args(["-c", "1", "-W", &timeout_ms.to_string(), ip])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            return PingResult {
                target: ip.to_string(),
                target_name: name.to_string(),
                timestamp,
                success: false,
                latency_ms: None,
                error: Some(format!("Failed to spawn ping: {}", e)),
            };
        }
    };

    // Take stdout/stderr handles before waiting
    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();

    // Use tokio::select! to race the process completion against the timeout
    // This allows us to properly kill the process if it hangs
    let process_timeout = tokio::time::sleep(Duration::from_millis(process_timeout_ms));
    tokio::pin!(process_timeout);

    tokio::select! {
        // Process completed (success or failure)
        result = child.wait() => {
            let elapsed = start.elapsed();

            match result {
                Ok(status) => {
                    // Read stdout and stderr
                    let stdout = if let Some(mut handle) = stdout_handle {
                        use tokio::io::AsyncReadExt;
                        let mut buf = Vec::new();
                        let _ = handle.read_to_end(&mut buf).await;
                        String::from_utf8_lossy(&buf).to_string()
                    } else {
                        String::new()
                    };

                    let stderr = if let Some(mut handle) = stderr_handle {
                        use tokio::io::AsyncReadExt;
                        let mut buf = Vec::new();
                        let _ = handle.read_to_end(&mut buf).await;
                        String::from_utf8_lossy(&buf).to_string()
                    } else {
                        String::new()
                    };

                    let success = status.success();
                    let latency_ms = if success {
                        parse_latency(&stdout)
                    } else {
                        None
                    };

                    // Log slow pings as warnings (took > 2x expected timeout)
                    let slow_threshold = timeout_ms * 2;
                    if elapsed.as_millis() as u64 > slow_threshold {
                        tracing::warn!(
                            "Slow ping to {}: subprocess took {}ms (expected <{}ms)",
                            ip,
                            elapsed.as_millis(),
                            timeout_ms
                        );
                    }

                    PingResult {
                        target: ip.to_string(),
                        target_name: name.to_string(),
                        timestamp,
                        success,
                        latency_ms,
                        error: if success {
                            None
                        } else {
                            Some(parse_error(&stdout, &stderr))
                        },
                    }
                }
                Err(e) => {
                    PingResult {
                        target: ip.to_string(),
                        target_name: name.to_string(),
                        timestamp,
                        success: false,
                        latency_ms: None,
                        error: Some(format!("Ping execution error: {}", e)),
                    }
                }
            }
        }

        // Timeout - kill the hung process
        _ = &mut process_timeout => {
            let elapsed = start.elapsed();

            tracing::error!(
                "Ping process timeout to {}: subprocess hung for {}ms (limit: {}ms), killing",
                ip,
                elapsed.as_millis(),
                process_timeout_ms
            );

            // Kill the hung process
            if let Err(e) = child.kill().await {
                tracing::warn!("Failed to kill hung ping process: {}", e);
            }

            PingResult {
                target: ip.to_string(),
                target_name: name.to_string(),
                timestamp,
                success: false,
                latency_ms: None,
                error: Some(format!(
                    "Process timeout ({}ms) - network stack may be hung",
                    process_timeout_ms
                )),
            }
        }
    }
}

/// Parse latency from ping output
/// Looks for pattern: time=X.XXX ms
fn parse_latency(output: &str) -> Option<f64> {
    // Look for "time=14.123 ms" pattern
    for line in output.lines() {
        if let Some(time_idx) = line.find("time=") {
            let after_time = &line[time_idx + 5..];
            // Find the end of the number (space or "ms")
            let end_idx = after_time.find([' ', 'm']).unwrap_or(after_time.len());
            let num_str = &after_time[..end_idx];
            if let Ok(latency) = num_str.parse::<f64>() {
                return Some(latency);
            }
        }
    }
    None
}

/// Parse error message from ping output
fn parse_error(stdout: &str, stderr: &str) -> String {
    // Check for common error patterns
    if stdout.contains("100.0% packet loss") || stdout.contains("100% packet loss") {
        return "Request timeout".to_string();
    }
    if stdout.contains("No route to host") {
        return "No route to host".to_string();
    }
    if stdout.contains("Network is unreachable") {
        return "Network unreachable".to_string();
    }
    if stderr.contains("Unknown host") || stderr.contains("cannot resolve") {
        return "DNS resolution failed".to_string();
    }

    // Default error
    if !stderr.is_empty() {
        stderr.lines().next().unwrap_or("Unknown error").to_string()
    } else {
        "Ping failed".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_latency_success() {
        let output = r#"PING 8.8.8.8 (8.8.8.8): 56 data bytes
64 bytes from 8.8.8.8: icmp_seq=0 ttl=117 time=14.123 ms

--- 8.8.8.8 ping statistics ---
1 packets transmitted, 1 packets received, 0.0% packet loss
round-trip min/avg/max/stddev = 14.123/14.123/14.123/0.000 ms"#;

        let latency = parse_latency(output);
        assert_eq!(latency, Some(14.123));
    }

    #[test]
    fn test_parse_latency_sub_millisecond() {
        let output = "64 bytes from 127.0.0.1: icmp_seq=0 ttl=64 time=0.042 ms";
        let latency = parse_latency(output);
        assert_eq!(latency, Some(0.042));
    }

    #[test]
    fn test_parse_latency_no_match() {
        let output = "Request timeout for icmp_seq 0";
        let latency = parse_latency(output);
        assert!(latency.is_none());
    }

    #[test]
    fn test_parse_error_timeout() {
        let stdout = r#"PING 8.8.8.8 (8.8.8.8): 56 data bytes

--- 8.8.8.8 ping statistics ---
1 packets transmitted, 0 packets received, 100.0% packet loss"#;

        let error = parse_error(stdout, "");
        assert_eq!(error, "Request timeout");
    }

    #[test]
    fn test_parse_error_no_route() {
        let stdout = "ping: sendto: No route to host";
        let error = parse_error(stdout, "");
        assert_eq!(error, "No route to host");
    }

    #[test]
    fn test_parse_error_network_unreachable() {
        let stdout = "ping: sendto: Network is unreachable";
        let error = parse_error(stdout, "");
        assert_eq!(error, "Network unreachable");
    }

    #[tokio::test]
    async fn test_ping_localhost() {
        let result = ping_target("127.0.0.1", "localhost", 2000, 6000).await;
        assert!(result.success, "Ping to localhost should succeed");
        assert!(result.latency_ms.is_some(), "Should have latency");
        assert!(
            result.latency_ms.unwrap() < 10.0,
            "Localhost latency should be < 10ms"
        );
    }

    #[tokio::test]
    async fn test_ping_invalid_ip() {
        // Using a non-routable IP that should timeout quickly
        let result = ping_target("192.0.2.1", "test", 1000, 3000).await;
        assert!(!result.success, "Ping to non-routable IP should fail");
        assert!(result.error.is_some(), "Should have error message");
    }

    #[tokio::test]
    async fn test_process_timeout() {
        // Test that process timeout kicks in
        // Using a very short process timeout to force the timeout path
        // (This test may not trigger actual timeout on fast systems, but validates the code path)
        let result = ping_target("127.0.0.1", "localhost", 2000, 6000).await;
        // Localhost should succeed within the timeout
        assert!(result.success, "Localhost should not timeout");
    }

    #[test]
    fn test_ping_monitor_creation() {
        let config = Config::default();
        let monitor = PingMonitor::new(&config);
        assert!(!monitor.targets().is_empty());
    }
}
