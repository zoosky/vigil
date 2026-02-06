# 014 - TCP Connectivity Monitoring

**Status:** Done

## Problem Statement

Vigil currently uses ICMP ping for connectivity monitoring. However, many routers and ISPs rate-limit or deprioritize ICMP traffic as DoS protection. This causes false outage detection:

```
Observed behavior:
- ICMP ping (concurrent): ~40% success rate
- TCP connections (concurrent): 100% success rate
```

The user experiences no actual connectivity issues (VPN, streaming work fine), but Vigil reports constant outages because ping packets are being dropped while TCP traffic flows normally.

**Root cause:** ICMP rate-limiting is common on:
- Home routers (especially fiber/cable modems)
- ISP edge equipment
- Cloud provider networks
- Corporate firewalls

## Solution

Add TCP-based connectivity monitoring as the primary or alternative method. TCP connections reflect real-world application behavior (HTTP, VPN, streaming) more accurately than ICMP.

### Monitoring Methods

| Method | Protocol | Port | Use Case |
|--------|----------|------|----------|
| `tcp` | TCP SYN | 443/80 | Most accurate for real-world connectivity |
| `ping` | ICMP | - | Traditional, may be rate-limited |
| `http` | HTTP GET | 80/443 | Full HTTP check with response validation |

## Implementation

### 1. Configuration Changes

**File: `src/config.rs`**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub name: String,
    pub ip: String,
    /// Monitoring method: "ping", "tcp", or "http"
    #[serde(default = "default_method")]
    pub method: MonitorMethod,
    /// Port for TCP/HTTP methods (default: 443)
    #[serde(default = "default_port")]
    pub port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MonitorMethod {
    Ping,
    Tcp,
    Http,
}

fn default_method() -> MonitorMethod {
    MonitorMethod::Tcp  // TCP is now the default
}

fn default_port() -> u16 {
    443
}
```

**Example config.toml:**

```toml
[targets]
gateway = "10.0.0.1"

targets = [
    # TCP checks (recommended - not affected by ICMP rate limiting)
    { name = "Google", ip = "8.8.8.8", method = "tcp", port = 443 },
    { name = "Cloudflare", ip = "1.1.1.1", method = "tcp", port = 443 },

    # Traditional ping (may have false positives)
    { name = "Quad9", ip = "9.9.9.9", method = "ping" },

    # HTTP check (validates full response)
    { name = "Google HTTP", ip = "google.com", method = "http", port = 443 },
]
```

### 2. TCP Monitor Implementation

**File: `src/monitor/tcp.rs`**

```rust
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Result of a TCP connectivity check
pub struct TcpCheckResult {
    pub target: String,
    pub port: u16,
    pub success: bool,
    pub latency_ms: Option<f64>,
    pub error: Option<String>,
}

/// Check TCP connectivity to a host:port
pub async fn check_tcp(
    host: &str,
    port: u16,
    timeout_ms: u64,
) -> TcpCheckResult {
    let addr = format!("{}:{}", host, port);
    let start = Instant::now();

    let result = timeout(
        Duration::from_millis(timeout_ms),
        TcpStream::connect(&addr),
    )
    .await;

    let latency = start.elapsed().as_secs_f64() * 1000.0;

    match result {
        Ok(Ok(_stream)) => TcpCheckResult {
            target: host.to_string(),
            port,
            success: true,
            latency_ms: Some(latency),
            error: None,
        },
        Ok(Err(e)) => TcpCheckResult {
            target: host.to_string(),
            port,
            success: false,
            latency_ms: None,
            error: Some(format!("Connection failed: {}", e)),
        },
        Err(_) => TcpCheckResult {
            target: host.to_string(),
            port,
            success: false,
            latency_ms: None,
            error: Some("Connection timeout".to_string()),
        },
    }
}
```

### 3. Unified Monitor Interface

**File: `src/monitor/mod.rs`**

```rust
/// Unified connectivity check that supports multiple methods
pub async fn check_connectivity(
    target: &Target,
    timeout_ms: u64,
) -> PingResult {
    match target.method {
        MonitorMethod::Ping => ping_target(&target.ip, &target.name, timeout_ms).await,
        MonitorMethod::Tcp => {
            let tcp_result = check_tcp(&target.ip, target.port, timeout_ms).await;
            // Convert to PingResult for compatibility
            PingResult {
                target: tcp_result.target,
                target_name: target.name.clone(),
                timestamp: Utc::now(),
                success: tcp_result.success,
                latency_ms: tcp_result.latency_ms,
                error: tcp_result.error,
            }
        }
        MonitorMethod::Http => {
            let http_result = check_http(&target.ip, target.port, timeout_ms).await;
            PingResult {
                target: http_result.target,
                target_name: target.name.clone(),
                timestamp: Utc::now(),
                success: http_result.success,
                latency_ms: http_result.latency_ms,
                error: http_result.error,
            }
        }
    }
}
```

### 4. HTTP Check (Optional Enhancement)

```rust
/// Full HTTP check with response validation
pub async fn check_http(
    host: &str,
    port: u16,
    timeout_ms: u64,
) -> HttpCheckResult {
    let url = if port == 443 {
        format!("https://{}/", host)
    } else {
        format!("http://{}:{}/", host, port)
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .unwrap();

    let start = Instant::now();
    let result = client.head(&url).send().await;
    let latency = start.elapsed().as_secs_f64() * 1000.0;

    match result {
        Ok(response) => HttpCheckResult {
            target: host.to_string(),
            port,
            success: response.status().is_success() || response.status().is_redirection(),
            latency_ms: Some(latency),
            status_code: Some(response.status().as_u16()),
            error: None,
        },
        Err(e) => HttpCheckResult {
            target: host.to_string(),
            port,
            success: false,
            latency_ms: None,
            status_code: None,
            error: Some(e.to_string()),
        },
    }
}
```

### 5. CLI Output Updates

Show the monitoring method in status output:

```
Vigil Network Monitor
═══════════════════════════════════════════════════════════

Monitoring targets:
  • Google (8.8.8.8:443) [TCP]
  • Cloudflare (1.1.1.1:443) [TCP]
  • Quad9 (9.9.9.9) [PING]

[07:44:16] ✓ Google (8.8.8.8:443) - 4.2ms
[07:44:16] ✓ Cloudflare (1.1.1.1:443) - 3.8ms
[07:44:16] ✓ Quad9 (9.9.9.9) - 5.1ms
```

## Migration Path

1. **Default change:** New installations default to TCP method
2. **Existing configs:** Continue to work (ping is still supported)
3. **Upgrade prompt:** Suggest TCP when ICMP rate-limiting is detected

### Auto-detection of ICMP Rate Limiting

```rust
/// Detect if ICMP is being rate-limited
pub async fn detect_icmp_rate_limiting(target: &str) -> bool {
    // Run concurrent pings
    let ping_success = run_concurrent_pings(target, 6).await;

    // Run concurrent TCP checks
    let tcp_success = run_concurrent_tcp(target, 443, 6).await;

    // If TCP works but ICMP doesn't, suggest switching
    if tcp_success > 5 && ping_success < 4 {
        tracing::warn!(
            "ICMP rate-limiting detected: ping {}/6, tcp {}/6. Consider using method = \"tcp\"",
            ping_success, tcp_success
        );
        return true;
    }
    false
}
```

## Tasks

- [x] Add `MonitorMethod` enum to config
- [x] Add `method` and `port` fields to Target
- [x] Implement `check_tcp()` function
- [x] Implement `check_http()` function
- [x] Create unified `check_connectivity()` dispatcher
- [x] Update `PingMonitor` to use unified interface
- [x] Update CLI output to show method
- [ ] Add ICMP rate-limiting detection (optional, future enhancement)
- [x] Update default targets to use TCP
- [ ] Add migration/upgrade notice for existing users (optional)
- [x] Add tests for TCP monitoring
- [x] Update documentation

## Test Plan

1. **TCP check works:** Verify TCP connections to 8.8.8.8:443 succeed
2. **Timeout handling:** Verify unreachable hosts timeout correctly
3. **Port flexibility:** Test different ports (80, 443, custom)
4. **Mixed methods:** Test config with both ping and TCP targets
5. **Rate-limit detection:** Verify detection works when ICMP is throttled
6. **Latency accuracy:** Compare TCP latency to ping latency

## Acceptance Criteria

1. TCP connectivity checks work reliably
2. No false positives from ICMP rate-limiting
3. Existing ping-based configs continue to work
4. Method shown in CLI output
5. Documentation updated with TCP examples
6. Default new configs use TCP method

## Dependencies

- `tokio` (already present) - for async TCP
- `reqwest` (optional) - for HTTP checks

## Performance Considerations

- TCP checks are slightly heavier than ICMP (full handshake)
- Connection is immediately closed after success
- Latency includes TCP handshake (~1 RTT)
- Consider connection pooling for HTTP checks
