# 016 - Process Timeout and Slow Ping Detection

**Status:** Done

## Problem Statement

The current ping monitoring relies on macOS's `-W` flag to set a timeout for ICMP replies. However, when the network is in a degraded state (the exact condition we're trying to detect), the ping **subprocess itself can hang** for minutes before the `-W` timeout even applies.

Real-world evidence from the dev database shows outages recorded as 76+ seconds (outage #9: 76.23s, #49: 79.35s), but the actual network disruption may have been longer because the monitor was blocked waiting for hung ping processes.

### Root Cause

The macOS ping command's execution can be delayed by:
1. ARP resolution stalls when the network stack is confused
2. Route table lookups blocking when the gateway is unreachable
3. DNS reverse lookups (even for IP addresses) timing out
4. System network stack being in an inconsistent state

When this happens:
- The ping process doesn't even send the ICMP packet for many seconds
- The `-W 2000` timeout (2 seconds) doesn't start until the packet is sent
- The monitor blocks waiting for the subprocess, missing the actual outage window

### User Impact

User reported dropping from a team meeting but the outage wasn't detected promptly because ping processes were hanging. The monitoring data shows gaps that don't match the user's real experience.

## Solution

Add a **hard process-level timeout** using `tokio::time::timeout()` that kills the ping subprocess if it takes too long, independent of the ping command's internal timeout.

### Key Changes

1. **Process Timeout**: Wrap all subprocess execution in `tokio::time::timeout()`
2. **Kill Hung Processes**: When timeout hits, kill the child process and return failure
3. **Slow Ping Detection**: Track pings that complete but took abnormally long
4. **Configuration**: Add new config options for process timeout

## Implementation

### File: `src/monitor/ping.rs`

Add process-level timeout to `ping_target()`:

```rust
use tokio::time::timeout;

/// Execute a single ping to a target IP with process-level timeout
pub async fn ping_target(ip: &str, name: &str, timeout_ms: u64, process_timeout_ms: u64) -> PingResult {
    let timestamp = Utc::now();

    // Hard timeout on the entire subprocess execution
    let result = timeout(
        Duration::from_millis(process_timeout_ms),
        execute_ping(ip, timeout_ms)
    ).await;

    let elapsed = timestamp.elapsed();

    match result {
        Ok(ping_result) => {
            // Check if ping was slow (took > 2x expected timeout)
            let slow_threshold = timeout_ms * 2;
            if elapsed.as_millis() as u64 > slow_threshold {
                tracing::warn!(
                    "Slow ping to {}: took {}ms (expected <{}ms)",
                    ip,
                    elapsed.as_millis(),
                    timeout_ms
                );
            }
            ping_result
        }
        Err(_elapsed) => {
            // Process timeout - subprocess hung
            tracing::error!(
                "Ping process timeout to {}: subprocess hung for {}ms (limit: {}ms)",
                ip,
                elapsed.as_millis(),
                process_timeout_ms
            );
            PingResult {
                target: ip.to_string(),
                target_name: name.to_string(),
                timestamp,
                success: false,
                latency_ms: None,
                error: Some(format!("Process timeout ({}ms)", process_timeout_ms)),
            }
        }
    }
}

/// Internal function to execute ping command
async fn execute_ping(ip: &str, timeout_ms: u64) -> PingResult {
    // Existing ping implementation...
}
```

### Process Termination

When the timeout fires, the subprocess needs to be properly terminated:

```rust
async fn execute_ping_with_kill(ip: &str, timeout_ms: u64, process_timeout_ms: u64) -> PingResult {
    let timestamp = Utc::now();

    let mut child = Command::new("ping")
        .args(["-c", "1", "-W", &timeout_ms.to_string(), ip])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let result = timeout(
        Duration::from_millis(process_timeout_ms),
        child.wait_with_output()
    ).await;

    match result {
        Ok(Ok(output)) => {
            // Normal completion - parse output
            parse_ping_output(&output, ip, timestamp)
        }
        Ok(Err(e)) => {
            // Command execution error
            PingResult::error(ip, timestamp, format!("Execution error: {}", e))
        }
        Err(_) => {
            // Timeout - kill the process
            let _ = child.kill().await;
            PingResult::error(ip, timestamp, format!("Process timeout ({}ms)", process_timeout_ms))
        }
    }
}
```

### File: `src/config.rs`

Add new configuration option:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    // ... existing fields ...

    /// Hard timeout for ping subprocess in milliseconds
    /// If the ping process doesn't complete within this time, it is killed.
    /// Default: 3x ping_timeout_ms (e.g., 6000ms if ping_timeout is 2000ms)
    #[serde(default = "default_process_timeout")]
    pub ping_process_timeout_ms: u64,

    /// Threshold for logging slow pings (as multiplier of ping_timeout_ms)
    /// Pings that complete but take longer than this are logged as warnings.
    /// Default: 2 (i.e., 2x the ping timeout)
    #[serde(default = "default_slow_ping_threshold")]
    pub slow_ping_threshold: u32,
}

fn default_process_timeout() -> u64 {
    6000  // 6 seconds - gives ping plenty of time but won't hang forever
}

fn default_slow_ping_threshold() -> u32 {
    2  // Log warning if ping takes > 2x expected timeout
}
```

### Slow Ping Metrics

Track slow pings for diagnostics:

```rust
/// Result of a ping attempt with timing metadata
pub struct PingResult {
    // ... existing fields ...

    /// Whether this ping was slow (took longer than expected)
    pub slow: bool,

    /// Actual time taken for the subprocess to complete (not ICMP latency)
    pub process_time_ms: Option<u64>,
}
```

## Configuration

### Default Values

| Setting | Default | Description |
|---------|---------|-------------|
| `ping_timeout_ms` | 2000 | ICMP reply timeout (existing) |
| `ping_process_timeout_ms` | 6000 | Hard subprocess timeout |
| `slow_ping_threshold` | 2 | Multiplier for slow ping warning |

### Example Config

```toml
[monitor]
ping_interval_ms = 3000
ping_timeout_ms = 2000
ping_process_timeout_ms = 6000    # Kill subprocess after 6 seconds
slow_ping_threshold = 2           # Warn if ping takes > 4 seconds
```

## Behavioral Changes

### Before (Current)
1. Ping subprocess hangs for 77 seconds
2. Monitor blocks, no results recorded during this time
3. Outage detected late, duration inaccurate
4. User experiences dropped calls that don't match monitoring data

### After (With This Feature)
1. Ping subprocess starts
2. After 6 seconds (configurable), subprocess is killed
3. Failure result returned immediately with "Process timeout" error
4. Outage detected within seconds, not minutes
5. Slow pings (4-6 seconds) logged as warnings for diagnostics

## Test Plan

### Unit Tests

```rust
#[tokio::test]
async fn test_process_timeout_kills_hung_ping() {
    // Use a non-routable IP that will cause the ping to hang
    let result = ping_target_with_timeout(
        "192.0.2.1",  // TEST-NET-1, non-routable
        "test",
        2000,   // ping timeout
        1000,   // process timeout (shorter for test)
    ).await;

    assert!(!result.success);
    assert!(result.error.unwrap().contains("Process timeout"));
}

#[tokio::test]
async fn test_slow_ping_detection() {
    // Localhost should be fast
    let result = ping_target("127.0.0.1", "localhost", 2000, 6000).await;
    assert!(result.success);
    assert!(!result.slow);
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_monitor_doesnt_block_on_hung_ping() {
    let config = Config::default();
    let monitor = PingMonitor::new(&config);

    // Even with a hung target, should get results within process_timeout
    let start = Instant::now();
    let result = monitor.ping(&Target::new("test", "192.0.2.1")).await;
    let elapsed = start.elapsed();

    assert!(elapsed.as_millis() < 7000);  // Should not hang
    assert!(!result.success);
}
```

## Migration

This is a backwards-compatible change:
- New config fields have sensible defaults
- Existing behavior is preserved (process timeout is generous)
- No database schema changes required

## Acceptance Criteria

1. Ping subprocess is killed if it doesn't complete within `ping_process_timeout_ms`
2. Process timeout returns a failure result with clear error message
3. Slow pings (> threshold) are logged as warnings
4. Monitor never blocks for more than `ping_process_timeout_ms` on any single ping
5. Default timeout (6s) is reasonable - long enough for normal operation, short enough to detect real issues
6. Configuration allows tuning both timeouts independently

## Dependencies

No new dependencies. Uses existing:
- `tokio::time::timeout` for async timeout
- `tokio::process::Command` with `kill()` for process termination
