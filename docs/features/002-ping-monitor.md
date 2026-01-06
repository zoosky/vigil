# 002 - Ping Monitor

**Status:** Done

## Overview

Implement continuous multi-target ping monitoring using macOS `ping` command via shell-out.

## Objectives

- Ping multiple targets concurrently at configurable intervals
- Parse ping output for latency and success/failure
- Report results to the state machine (Feature 003)
- Handle timeouts gracefully

## Implementation

### File: `src/monitor/ping.rs`

```rust
pub struct PingMonitor {
    targets: Vec<Target>,
    interval: Duration,
    timeout: Duration,
}

impl PingMonitor {
    pub fn new(config: &Config) -> Self;

    /// Run a single ping to a target
    pub async fn ping(&self, target: &Target) -> PingResult;

    /// Start continuous monitoring, returns a stream of results
    pub fn start(&self) -> impl Stream<Item = PingResult>;
}
```

### macOS Ping Command

```bash
# Single packet, timeout in milliseconds
ping -c 1 -W 2000 8.8.8.8
```

**Success output:**
```
PING 8.8.8.8 (8.8.8.8): 56 data bytes
64 bytes from 8.8.8.8: icmp_seq=0 ttl=117 time=14.123 ms

--- 8.8.8.8 ping statistics ---
1 packets transmitted, 1 packets received, 0.0% packet loss
round-trip min/avg/max/stddev = 14.123/14.123/14.123/0.000 ms
```

**Timeout output:**
```
PING 8.8.8.8 (8.8.8.8): 56 data bytes

--- 8.8.8.8 ping statistics ---
1 packets transmitted, 0 packets received, 100.0% packet loss
```

### Parsing Strategy

1. Check exit code (0 = success, non-zero = failure)
2. Parse `time=X.XXX ms` from output for latency
3. Handle edge cases: DNS failure, network unreachable

### Concurrency Model

```
┌─────────────────────────────────────────────┐
│              Ping Monitor                    │
│                                             │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐    │
│  │ Task 1  │  │ Task 2  │  │ Task 3  │    │
│  │ Gateway │  │ 8.8.8.8 │  │ 1.1.1.1 │    │
│  └────┬────┘  └────┬────┘  └────┬────┘    │
│       │            │            │          │
│       └────────────┼────────────┘          │
│                    ▼                        │
│            tokio::select!                   │
│                    │                        │
│                    ▼                        │
│           Result Channel                    │
└─────────────────────────────────────────────┘
```

Use `tokio::spawn` for each target, collect results via `mpsc` channel.

## Tasks

- [ ] Implement `ping()` function with shell-out
- [ ] Parse ping output (regex or string parsing)
- [ ] Implement `PingMonitor` struct
- [ ] Add concurrent pinging with tokio
- [ ] Create result channel/stream
- [ ] Add unit tests with mock responses
- [ ] Integration test with real ping

## Test Plan

### Unit Tests

```rust
#[test]
fn test_parse_ping_success() {
    let output = "64 bytes from 8.8.8.8: icmp_seq=0 ttl=117 time=14.123 ms";
    let result = parse_ping_output(output, 0);
    assert!(result.success);
    assert_eq!(result.latency_ms, Some(14.123));
}

#[test]
fn test_parse_ping_timeout() {
    let output = "1 packets transmitted, 0 packets received, 100.0% packet loss";
    let result = parse_ping_output(output, 1);
    assert!(!result.success);
    assert!(result.latency_ms.is_none());
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_ping_localhost() {
    let monitor = PingMonitor::new(...);
    let result = monitor.ping(&Target::new("localhost", "127.0.0.1")).await;
    assert!(result.success);
}
```

## Error Handling

| Error | Handling |
|-------|----------|
| Command not found | Log error, return failure result |
| DNS resolution failure | Return failure with error message |
| Timeout | Return failure, latency = None |
| Network unreachable | Return failure with error message |

## Configuration Used

```toml
[monitor]
ping_interval_ms = 1000   # Interval between ping rounds
ping_timeout_ms = 2000    # Individual ping timeout
```

## Dependencies

No new dependencies required. Uses:
- `tokio::process::Command` for async shell execution
- `tokio::time::interval` for scheduling
- `tokio::sync::mpsc` for result channel

## Acceptance Criteria

1. Pings all configured targets every `ping_interval_ms`
2. Correctly parses latency from successful pings
3. Correctly detects timeouts and failures
4. Does not block on slow/failed pings
5. Results include timestamp, target, success, latency
