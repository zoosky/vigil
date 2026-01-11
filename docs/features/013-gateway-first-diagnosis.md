# 013 - Gateway-First Diagnosis

**Status:** Done

## Problem Statement

When all traceroute hops timeout, the current output shows only `* * *` entries with no actionable information about where the failure is occurring. Users see:

```
  1  * * *                    ✗ TIMEOUT
  2  * * *                    ✗ TIMEOUT
  3  * * *                    ✗ TIMEOUT
  → All hops timed out
```

This doesn't help identify whether the issue is:
- Local (router/WiFi/cable problem)
- ISP equipment (modem, first router)
- ISP backbone (upstream infrastructure)

## Solution

Before running a full traceroute, first ping the gateway to establish if local connectivity exists. This provides immediate diagnostic value:

1. **Gateway unreachable** → Local network issue (router, WiFi, cable)
2. **Gateway reachable, traceroute fails** → ISP issue (modem or upstream)

## Implementation

### 1. New Diagnostic Struct

**File: `src/models.rs`**

```rust
/// Network diagnostic result with gateway status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkDiagnostic {
    pub gateway_ip: Option<String>,
    pub gateway_reachable: bool,
    pub gateway_latency_ms: Option<f64>,
    pub traceroute: TracerouteResult,
    pub diagnosis: DiagnosisResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosisResult {
    /// All systems operational
    Healthy,
    /// Gateway unreachable - local network issue
    LocalNetworkDown,
    /// Gateway OK but upstream fails - ISP issue
    IspIssue { failing_hop: u8 },
    /// Intermittent - some hops responding
    Intermittent,
    /// Cannot determine
    Unknown,
}

impl std::fmt::Display for DiagnosisResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiagnosisResult::Healthy => write!(f, "Healthy"),
            DiagnosisResult::LocalNetworkDown => write!(f, "Local network down"),
            DiagnosisResult::IspIssue { failing_hop } => {
                write!(f, "ISP issue at hop {}", failing_hop)
            }
            DiagnosisResult::Intermittent => write!(f, "Intermittent connectivity"),
            DiagnosisResult::Unknown => write!(f, "Unknown"),
        }
    }
}
```

### 2. Enhanced HopAnalyzer

**File: `src/monitor/traceroute.rs`**

```rust
impl HopAnalyzer {
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
        let responding_hops: Vec<_> = trace.hops.iter()
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

    /// Simple ping to a host
    async fn ping_host(&self, host: &str) -> PingResult {
        // Use existing ping infrastructure
        use std::process::Command;
        use chrono::Utc;

        let output = Command::new("ping")
            .args(["-c", "1", "-W", "2000", host])
            .output();

        let timestamp = Utc::now();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let success = output.status.success();
                let latency_ms = Self::parse_ping_latency(&stdout);

                PingResult {
                    target: host.to_string(),
                    target_name: "Gateway".to_string(),
                    timestamp,
                    success,
                    latency_ms,
                    error: if success { None } else { Some("timeout".to_string()) },
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

    fn parse_ping_latency(output: &str) -> Option<f64> {
        // Parse "time=X.XX ms" from ping output
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
```

### 3. Updated Monitor Loop

**File: `src/main.rs`**

```rust
// When entering DEGRADED or OFFLINE state:
StateEvent::Degraded { .. } | StateEvent::Offline { .. } => {
    let gateway = app.config.targets.gateway.as_deref();
    let trace_target = targets.first()
        .map(|t| t.ip.as_str())
        .unwrap_or("8.8.8.8");

    // Run full diagnostic instead of just traceroute
    let diagnostic = analyzer.diagnose(trace_target, gateway).await;

    // Log the diagnosis
    match diagnostic.diagnosis {
        DiagnosisResult::LocalNetworkDown => {
            println!("   Diagnosis: LOCAL NETWORK DOWN");
            println!("   Gateway {} is unreachable",
                diagnostic.gateway_ip.as_deref().unwrap_or("unknown"));
            println!("   Check: router, WiFi connection, ethernet cable\n");
        }
        DiagnosisResult::IspIssue { failing_hop } => {
            println!("   Diagnosis: ISP ISSUE at hop {}", failing_hop);
            if diagnostic.gateway_reachable {
                println!("   Gateway OK ({:.1}ms)",
                    diagnostic.gateway_latency_ms.unwrap_or(0.0));
            }
            println!("   Failure point: {}\n", interpret_hop(failing_hop));
        }
        DiagnosisResult::Healthy => {
            println!("   Diagnosis: Connection recovered during trace\n");
        }
        _ => {
            println!("   Diagnosis: Unable to determine failure point\n");
        }
    }

    // Save traceroute as before...
}
```

### 4. Enhanced CLI Output

**Outage detail view:**

```
Outage #1 Details
═══════════════════════════════════════════════════════════

Started:     2024-01-15 14:23:45
Ended:       2024-01-15 14:25:17
Duration:    1m 32s
Diagnosis:   LOCAL NETWORK DOWN
Gateway:     192.168.1.1 (unreachable)

Traceroutes (2 captured)
───────────────────────────────────────────────────────────

[14:23:45] state_change - Target: 8.8.8.8
  Gateway: 192.168.1.1 - UNREACHABLE
  1  * * *                    ✗ TIMEOUT
  2  * * *                    ✗ TIMEOUT
  → Diagnosis: Local network down (check router/WiFi)

[14:25:10] periodic - Target: 8.8.8.8
  Gateway: 192.168.1.1 - OK (1.2ms)
  1  192.168.1.1     1.2 ms   ✓ Gateway
  2  10.0.0.1        5.6 ms   ✓ ISP Modem
  3  8.8.8.8        15.2 ms   ✓ Target reached
  → Connection recovered
```

### 5. Database Changes

Add diagnosis field to traceroutes table:

```sql
ALTER TABLE traceroutes ADD COLUMN gateway_reachable INTEGER;
ALTER TABLE traceroutes ADD COLUMN gateway_latency_ms REAL;
ALTER TABLE traceroutes ADD COLUMN diagnosis TEXT;
```

## Tasks

- [ ] Add `NetworkDiagnostic` and `DiagnosisResult` to models.rs
- [ ] Add `diagnose()` method to HopAnalyzer
- [ ] Add `ping_host()` helper method
- [ ] Update monitor loop to use `diagnose()` instead of `trace()`
- [ ] Update database schema (migration v3)
- [ ] Update outage detail CLI to show diagnosis
- [ ] Update console output during monitoring
- [ ] Add tests for diagnosis logic

## Test Plan

1. **Local network down**: Disconnect WiFi/ethernet, verify "LOCAL NETWORK DOWN" diagnosis
2. **ISP issue**: Gateway reachable but upstream fails (harder to simulate)
3. **Healthy recovery**: Reconnect and verify "Connection recovered" message
4. **No gateway configured**: Verify graceful fallback when gateway not set

## Acceptance Criteria

1. Gateway is pinged before running traceroute
2. "Local network down" shown when gateway unreachable
3. "ISP issue at hop N" shown when gateway OK but upstream fails
4. Diagnosis stored in database with traceroute
5. Diagnosis displayed in outage detail view
6. Clear actionable messages (check router, WiFi, etc.)
