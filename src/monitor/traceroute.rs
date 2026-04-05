use crate::models::{
    DiagnosisResult, NetworkDiagnostic, PingResult, TracerouteHop, TracerouteResult,
};
use chrono::Utc;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

/// Hop analyzer for running traceroute and identifying failing hops
pub struct HopAnalyzer {
    timeout_secs: u64,
    max_hops: u8,
    /// Hard process timeout in milliseconds. Kills the subprocess if it exceeds this.
    process_timeout_ms: u64,
    /// Ping timeout in milliseconds (used for gateway ping).
    ping_timeout_ms: u64,
}

impl Default for HopAnalyzer {
    fn default() -> Self {
        Self::new(Duration::from_secs(2), 30)
    }
}

impl HopAnalyzer {
    /// Create a new hop analyzer
    pub fn new(timeout: Duration, max_hops: u8) -> Self {
        let timeout_secs = timeout.as_secs().max(1);
        // Default process timeout: generous upper bound based on max_hops * per-hop timeout
        let process_timeout_ms = (max_hops as u64) * timeout_secs * 1000 + 5000;
        Self {
            timeout_secs,
            max_hops,
            process_timeout_ms,
            ping_timeout_ms: timeout.as_millis() as u64,
        }
    }

    /// Create a hop analyzer with explicit process timeout and ping timeout
    pub fn with_timeouts(
        timeout: Duration,
        max_hops: u8,
        process_timeout_ms: u64,
        ping_timeout_ms: u64,
    ) -> Self {
        Self {
            timeout_secs: timeout.as_secs().max(1),
            max_hops,
            process_timeout_ms,
            ping_timeout_ms,
        }
    }

    /// Run traceroute to a target with process-level timeout
    pub async fn trace(&self, target: &str) -> TracerouteResult {
        let timestamp = Utc::now();

        // macOS traceroute: -n (numeric), -q 1 (1 query per hop), -w timeout, -m max_hops
        let mut child = match Command::new("traceroute")
            .args([
                "-n",
                "-q",
                "1",
                "-w",
                &self.timeout_secs.to_string(),
                "-m",
                &self.max_hops.to_string(),
                target,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                tracing::error!("Failed to spawn traceroute: {}", e);
                return TracerouteResult {
                    target: target.to_string(),
                    timestamp,
                    hops: vec![],
                    success: false,
                };
            }
        };

        let stdout_handle = child.stdout.take();
        let process_timeout = tokio::time::sleep(Duration::from_millis(self.process_timeout_ms));
        tokio::pin!(process_timeout);

        tokio::select! {
            result = child.wait() => {
                match result {
                    Ok(_) => {
                        let stdout = if let Some(mut handle) = stdout_handle {
                            use tokio::io::AsyncReadExt;
                            let mut buf = Vec::new();
                            let _ = handle.read_to_end(&mut buf).await;
                            String::from_utf8_lossy(&buf).to_string()
                        } else {
                            String::new()
                        };
                        let hops = parse_traceroute_output(&stdout);
                        let success = check_reached_target(&hops, target);

                        TracerouteResult {
                            target: target.to_string(),
                            timestamp,
                            hops,
                            success,
                        }
                    }
                    Err(e) => {
                        tracing::error!("Traceroute execution error: {}", e);
                        TracerouteResult {
                            target: target.to_string(),
                            timestamp,
                            hops: vec![],
                            success: false,
                        }
                    }
                }
            }
            _ = &mut process_timeout => {
                tracing::error!(
                    "Traceroute process timeout to {}: subprocess hung for {}ms, killing",
                    target,
                    self.process_timeout_ms
                );
                if let Err(e) = child.kill().await {
                    tracing::warn!("Failed to kill hung traceroute process: {}", e);
                }
                TracerouteResult {
                    target: target.to_string(),
                    timestamp,
                    hops: vec![],
                    success: false,
                }
            }
        }
    }

    /// Identify the failing hop from a traceroute result
    /// Returns the last responding hop (the one before the failure)
    pub fn identify_failing_hop(result: &TracerouteResult) -> Option<(u8, String)> {
        if result.success {
            return None; // No failure - target was reached
        }

        // Find the last hop that responded (not a timeout)
        let last_responding = result
            .hops
            .iter()
            .rev()
            .find(|h| !h.timeout && h.ip.is_some());

        last_responding.map(|h| (h.hop_number, h.ip.clone().unwrap()))
    }

    /// Run full network diagnostic: gateway ping + traceroute
    pub async fn diagnose(&self, target: &str, gateway: Option<&str>) -> NetworkDiagnostic {
        // Step 1: Ping gateway if known
        let (gateway_reachable, gateway_latency_ms) = if let Some(gw) = gateway {
            let ping_result = self.ping_host(gw).await;
            (ping_result.success, ping_result.latency_ms)
        } else {
            (true, None) // Assume reachable if no gateway configured
        };

        // Step 2: Run traceroute
        let traceroute = self.trace(target).await;

        // Step 3: Analyze and diagnose
        let diagnosis = self.analyze_diagnosis(gateway_reachable, &traceroute);

        NetworkDiagnostic {
            gateway_ip: gateway.map(String::from),
            gateway_reachable,
            gateway_latency_ms,
            traceroute,
            diagnosis,
        }
    }

    /// Analyze traceroute results and gateway status to determine diagnosis
    fn analyze_diagnosis(
        &self,
        gateway_reachable: bool,
        trace: &TracerouteResult,
    ) -> DiagnosisResult {
        // If traceroute succeeded, we're healthy
        if trace.success {
            return DiagnosisResult::Healthy;
        }

        // Count responding vs timeout hops
        let responding_hops: Vec<_> = trace
            .hops
            .iter()
            .filter(|h| !h.timeout && h.latency_ms.is_some())
            .collect();

        // All hops timeout
        if responding_hops.is_empty() {
            if !gateway_reachable {
                return DiagnosisResult::LocalNetworkDown;
            } else {
                // Gateway responds but traceroute all timeouts
                // This means ISP is blocking ICMP or issue at hop 2+
                return DiagnosisResult::IspIssue { failing_hop: 2 };
            }
        }

        // Some hops respond - find the last one
        if let Some(last_good) = responding_hops.last() {
            let failing_hop = last_good.hop_number + 1;
            return DiagnosisResult::IspIssue { failing_hop };
        }

        DiagnosisResult::Unknown
    }

    /// Ping a host asynchronously with process-level timeout
    pub async fn ping_host(&self, host: &str) -> PingResult {
        let timestamp = Utc::now();
        let timeout_str = self.ping_timeout_ms.to_string();

        // macOS ping: -c 1 (count), -W timeout_ms
        let mut child = match Command::new("ping")
            .args(["-c", "1", "-W", &timeout_str, host])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                return PingResult {
                    target: host.to_string(),
                    target_name: "Gateway".to_string(),
                    timestamp,
                    success: false,
                    latency_ms: None,
                    error: Some(format!("Failed to spawn ping: {}", e)),
                };
            }
        };

        let stdout_handle = child.stdout.take();
        let process_timeout =
            tokio::time::sleep(Duration::from_millis(self.process_timeout_ms));
        tokio::pin!(process_timeout);

        tokio::select! {
            result = child.wait() => {
                match result {
                    Ok(status) => {
                        let stdout = if let Some(mut handle) = stdout_handle {
                            use tokio::io::AsyncReadExt;
                            let mut buf = Vec::new();
                            let _ = handle.read_to_end(&mut buf).await;
                            String::from_utf8_lossy(&buf).to_string()
                        } else {
                            String::new()
                        };
                        let success = status.success();
                        let latency_ms = Self::parse_ping_latency(&stdout);

                        PingResult {
                            target: host.to_string(),
                            target_name: "Gateway".to_string(),
                            timestamp,
                            success,
                            latency_ms,
                            error: if success {
                                None
                            } else {
                                Some("timeout".to_string())
                            },
                        }
                    }
                    Err(e) => PingResult {
                        target: host.to_string(),
                        target_name: "Gateway".to_string(),
                        timestamp,
                        success: false,
                        latency_ms: None,
                        error: Some(e.to_string()),
                    },
                }
            }
            _ = &mut process_timeout => {
                tracing::error!(
                    "Gateway ping process timeout to {}: hung for {}ms, killing",
                    host,
                    self.process_timeout_ms
                );
                if let Err(e) = child.kill().await {
                    tracing::warn!("Failed to kill hung ping process: {}", e);
                }
                PingResult {
                    target: host.to_string(),
                    target_name: "Gateway".to_string(),
                    timestamp,
                    success: false,
                    latency_ms: None,
                    error: Some(format!(
                        "Process timeout ({}ms) - network stack may be hung",
                        self.process_timeout_ms
                    )),
                }
            }
        }
    }

    /// Parse latency from ping output (e.g., "time=1.234 ms")
    fn parse_ping_latency(output: &str) -> Option<f64> {
        for line in output.lines() {
            if let Some(pos) = line.find("time=") {
                let rest = &line[pos + 5..];
                if let Some(end) = rest.find(" ms") {
                    if let Ok(ms) = rest[..end].parse::<f64>() {
                        return Some(ms);
                    }
                }
            }
        }
        None
    }
}

/// Parse traceroute output into a list of hops
fn parse_traceroute_output(output: &str) -> Vec<TracerouteHop> {
    let mut hops = Vec::new();

    for line in output.lines() {
        let line = line.trim();

        // Skip header line (starts with "traceroute to")
        if line.starts_with("traceroute to") || line.is_empty() {
            continue;
        }

        if let Some(hop) = parse_hop_line(line) {
            hops.push(hop);
        }
    }

    hops
}

/// Parse a single hop line
/// Examples:
///   " 1  192.168.1.1  1.234 ms"
///   " 2  * * *"
///   " 3  10.0.0.1  5.678 ms"
fn parse_hop_line(line: &str) -> Option<TracerouteHop> {
    let parts: Vec<&str> = line.split_whitespace().collect();

    if parts.is_empty() {
        return None;
    }

    // First part should be hop number
    let hop_number: u8 = parts[0].parse().ok()?;

    // Check for timeout (asterisks)
    if parts.len() >= 2 && parts[1] == "*" {
        return Some(TracerouteHop {
            hop_number,
            ip: None,
            hostname: None,
            latency_ms: None,
            timeout: true,
        });
    }

    // Parse IP and latency
    if parts.len() >= 2 {
        let ip = parts[1].to_string();

        // Look for latency (number followed by "ms")
        let latency_ms = parts.iter().enumerate().find_map(|(i, &part)| {
            if part == "ms" && i > 0 {
                parts[i - 1].parse::<f64>().ok()
            } else {
                None
            }
        });

        return Some(TracerouteHop {
            hop_number,
            ip: Some(ip),
            hostname: None, // We use -n flag so no hostname
            latency_ms,
            timeout: false,
        });
    }

    None
}

/// Check if the traceroute reached the target
fn check_reached_target(hops: &[TracerouteHop], target: &str) -> bool {
    if let Some(last_hop) = hops.last() {
        if let Some(ref ip) = last_hop.ip {
            return ip == target;
        }
    }
    false
}

/// Format a traceroute result for display
pub fn format_traceroute(result: &TracerouteResult) -> String {
    let mut output = String::new();

    output.push_str(&format!("Traceroute to {}\n", result.target));
    output.push_str("═══════════════════════════════════════════════════════════\n\n");
    output.push_str("Hop  IP                  Latency\n");
    output.push_str("───────────────────────────────────────────────────────────\n");

    for hop in &result.hops {
        let ip_str = hop.ip.as_deref().unwrap_or("*");
        let latency_str = hop
            .latency_ms
            .map(|l| format!("{:.2} ms", l))
            .unwrap_or_else(|| "*".to_string());

        output.push_str(&format!(
            "{:3}  {:18}  {}\n",
            hop.hop_number, ip_str, latency_str
        ));
    }

    if result.success {
        output.push_str(&format!(
            "\nTarget reached in {} hops.\n",
            result.hops.len()
        ));
    } else if let Some((hop, ip)) = HopAnalyzer::identify_failing_hop(result) {
        output.push_str(&format!(
            "\nTarget NOT reached. Last responding hop: {} ({})\n",
            hop, ip
        ));
    } else {
        output.push_str("\nTarget NOT reached. No hops responded.\n");
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_traceroute_success() {
        let output = r#"traceroute to 8.8.8.8 (8.8.8.8), 64 hops max, 52 byte packets
 1  192.168.1.1  1.234 ms
 2  10.0.0.1  5.678 ms
 3  72.14.215.85  12.345 ms
 4  8.8.8.8  15.678 ms
"#;

        let hops = parse_traceroute_output(output);
        assert_eq!(hops.len(), 4);

        assert_eq!(hops[0].hop_number, 1);
        assert_eq!(hops[0].ip, Some("192.168.1.1".to_string()));
        assert!((hops[0].latency_ms.unwrap() - 1.234).abs() < 0.001);
        assert!(!hops[0].timeout);

        assert_eq!(hops[3].hop_number, 4);
        assert_eq!(hops[3].ip, Some("8.8.8.8".to_string()));
    }

    #[test]
    fn test_parse_traceroute_with_timeouts() {
        let output = r#"traceroute to 8.8.8.8 (8.8.8.8), 64 hops max
 1  192.168.1.1  1.234 ms
 2  10.0.0.1  5.678 ms
 3  * * *
 4  * * *
"#;

        let hops = parse_traceroute_output(output);
        assert_eq!(hops.len(), 4);

        assert!(!hops[0].timeout);
        assert!(!hops[1].timeout);
        assert!(hops[2].timeout);
        assert!(hops[2].ip.is_none());
        assert!(hops[3].timeout);
    }

    #[test]
    fn test_parse_traceroute_all_timeouts() {
        let output = r#"traceroute to 8.8.8.8 (8.8.8.8), 64 hops max
 1  * * *
 2  * * *
 3  * * *
"#;

        let hops = parse_traceroute_output(output);
        assert_eq!(hops.len(), 3);
        assert!(hops.iter().all(|h| h.timeout));
    }

    #[test]
    fn test_check_reached_target() {
        let hops = vec![
            TracerouteHop {
                hop_number: 1,
                ip: Some("192.168.1.1".to_string()),
                hostname: None,
                latency_ms: Some(1.0),
                timeout: false,
            },
            TracerouteHop {
                hop_number: 2,
                ip: Some("8.8.8.8".to_string()),
                hostname: None,
                latency_ms: Some(10.0),
                timeout: false,
            },
        ];

        assert!(check_reached_target(&hops, "8.8.8.8"));
        assert!(!check_reached_target(&hops, "1.1.1.1"));
    }

    #[test]
    fn test_identify_failing_hop() {
        // Case 1: Partial failure
        let result = TracerouteResult {
            target: "8.8.8.8".to_string(),
            timestamp: Utc::now(),
            hops: vec![
                TracerouteHop {
                    hop_number: 1,
                    ip: Some("192.168.1.1".to_string()),
                    hostname: None,
                    latency_ms: Some(1.0),
                    timeout: false,
                },
                TracerouteHop {
                    hop_number: 2,
                    ip: Some("10.0.0.1".to_string()),
                    hostname: None,
                    latency_ms: Some(5.0),
                    timeout: false,
                },
                TracerouteHop {
                    hop_number: 3,
                    ip: None,
                    hostname: None,
                    latency_ms: None,
                    timeout: true,
                },
            ],
            success: false,
        };

        let (hop, ip) = HopAnalyzer::identify_failing_hop(&result).unwrap();
        assert_eq!(hop, 2);
        assert_eq!(ip, "10.0.0.1");
    }

    #[test]
    fn test_identify_failing_hop_success() {
        let result = TracerouteResult {
            target: "8.8.8.8".to_string(),
            timestamp: Utc::now(),
            hops: vec![TracerouteHop {
                hop_number: 1,
                ip: Some("8.8.8.8".to_string()),
                hostname: None,
                latency_ms: Some(10.0),
                timeout: false,
            }],
            success: true,
        };

        // No failing hop when successful
        assert!(HopAnalyzer::identify_failing_hop(&result).is_none());
    }

    #[test]
    fn test_identify_failing_hop_all_timeout() {
        let result = TracerouteResult {
            target: "8.8.8.8".to_string(),
            timestamp: Utc::now(),
            hops: vec![
                TracerouteHop {
                    hop_number: 1,
                    ip: None,
                    hostname: None,
                    latency_ms: None,
                    timeout: true,
                },
                TracerouteHop {
                    hop_number: 2,
                    ip: None,
                    hostname: None,
                    latency_ms: None,
                    timeout: true,
                },
            ],
            success: false,
        };

        // No responding hop
        assert!(HopAnalyzer::identify_failing_hop(&result).is_none());
    }

    #[tokio::test]
    async fn test_trace_localhost() {
        let analyzer = HopAnalyzer::default();
        let result = analyzer.trace("127.0.0.1").await;

        // Localhost should be reachable in 1 hop
        assert!(result.success || !result.hops.is_empty());
    }

    #[test]
    fn test_format_traceroute() {
        let result = TracerouteResult {
            target: "8.8.8.8".to_string(),
            timestamp: Utc::now(),
            hops: vec![
                TracerouteHop {
                    hop_number: 1,
                    ip: Some("192.168.1.1".to_string()),
                    hostname: None,
                    latency_ms: Some(1.234),
                    timeout: false,
                },
                TracerouteHop {
                    hop_number: 2,
                    ip: Some("8.8.8.8".to_string()),
                    hostname: None,
                    latency_ms: Some(15.678),
                    timeout: false,
                },
            ],
            success: true,
        };

        let output = format_traceroute(&result);
        assert!(output.contains("192.168.1.1"));
        assert!(output.contains("8.8.8.8"));
        assert!(output.contains("Target reached in 2 hops"));
    }

    // Tests for diagnosis logic

    #[test]
    fn test_diagnosis_healthy() {
        let analyzer = HopAnalyzer::default();
        let trace = TracerouteResult {
            target: "8.8.8.8".to_string(),
            timestamp: Utc::now(),
            hops: vec![
                TracerouteHop {
                    hop_number: 1,
                    ip: Some("192.168.1.1".to_string()),
                    hostname: None,
                    latency_ms: Some(1.0),
                    timeout: false,
                },
                TracerouteHop {
                    hop_number: 2,
                    ip: Some("8.8.8.8".to_string()),
                    hostname: None,
                    latency_ms: Some(10.0),
                    timeout: false,
                },
            ],
            success: true,
        };

        let diagnosis = analyzer.analyze_diagnosis(true, &trace);
        assert_eq!(diagnosis, DiagnosisResult::Healthy);
    }

    #[test]
    fn test_diagnosis_local_network_down() {
        let analyzer = HopAnalyzer::default();
        let trace = TracerouteResult {
            target: "8.8.8.8".to_string(),
            timestamp: Utc::now(),
            hops: vec![
                TracerouteHop {
                    hop_number: 1,
                    ip: None,
                    hostname: None,
                    latency_ms: None,
                    timeout: true,
                },
                TracerouteHop {
                    hop_number: 2,
                    ip: None,
                    hostname: None,
                    latency_ms: None,
                    timeout: true,
                },
            ],
            success: false,
        };

        // Gateway unreachable + all hops timeout = local network down
        let diagnosis = analyzer.analyze_diagnosis(false, &trace);
        assert_eq!(diagnosis, DiagnosisResult::LocalNetworkDown);
    }

    #[test]
    fn test_diagnosis_isp_issue_all_timeout() {
        let analyzer = HopAnalyzer::default();
        let trace = TracerouteResult {
            target: "8.8.8.8".to_string(),
            timestamp: Utc::now(),
            hops: vec![
                TracerouteHop {
                    hop_number: 1,
                    ip: None,
                    hostname: None,
                    latency_ms: None,
                    timeout: true,
                },
                TracerouteHop {
                    hop_number: 2,
                    ip: None,
                    hostname: None,
                    latency_ms: None,
                    timeout: true,
                },
            ],
            success: false,
        };

        // Gateway reachable but traceroute all timeout = ISP issue at hop 2
        let diagnosis = analyzer.analyze_diagnosis(true, &trace);
        assert_eq!(diagnosis, DiagnosisResult::IspIssue { failing_hop: 2 });
    }

    #[test]
    fn test_diagnosis_isp_issue_partial() {
        let analyzer = HopAnalyzer::default();
        let trace = TracerouteResult {
            target: "8.8.8.8".to_string(),
            timestamp: Utc::now(),
            hops: vec![
                TracerouteHop {
                    hop_number: 1,
                    ip: Some("192.168.1.1".to_string()),
                    hostname: None,
                    latency_ms: Some(1.0),
                    timeout: false,
                },
                TracerouteHop {
                    hop_number: 2,
                    ip: Some("10.0.0.1".to_string()),
                    hostname: None,
                    latency_ms: Some(5.0),
                    timeout: false,
                },
                TracerouteHop {
                    hop_number: 3,
                    ip: None,
                    hostname: None,
                    latency_ms: None,
                    timeout: true,
                },
            ],
            success: false,
        };

        // Some hops respond, last responding is hop 2 = ISP issue at hop 3
        let diagnosis = analyzer.analyze_diagnosis(true, &trace);
        assert_eq!(diagnosis, DiagnosisResult::IspIssue { failing_hop: 3 });
    }

    #[test]
    fn test_parse_ping_latency() {
        // macOS ping output format
        let output = "PING 8.8.8.8 (8.8.8.8): 56 data bytes\n64 bytes from 8.8.8.8: icmp_seq=0 ttl=117 time=12.345 ms\n";
        let latency = HopAnalyzer::parse_ping_latency(output);
        assert!(latency.is_some());
        assert!((latency.unwrap() - 12.345).abs() < 0.001);
    }

    #[test]
    fn test_parse_ping_latency_no_match() {
        let output = "Request timeout for icmp_seq 0\n";
        let latency = HopAnalyzer::parse_ping_latency(output);
        assert!(latency.is_none());
    }
}
