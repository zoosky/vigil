# 004 - Hop Analysis

**Status:** Done

## Overview

Implement traceroute functionality to identify which network hop is causing connectivity failures.

## Objectives

- Run traceroute when outage detected
- Parse traceroute output to identify failing hop
- Store traceroute results linked to outages
- Provide manual traceroute command

## Implementation

### File: `src/monitor/traceroute.rs`

```rust
pub struct HopAnalyzer {
    timeout: Duration,
}

impl HopAnalyzer {
    pub fn new(timeout: Duration) -> Self;

    /// Run traceroute to target
    pub async fn trace(&self, target: &str) -> TracerouteResult;

    /// Identify the failing hop from traceroute results
    pub fn identify_failing_hop(result: &TracerouteResult) -> Option<(u8, String)>;
}
```

### macOS Traceroute Command

```bash
# Numeric output, 1 query per hop, 2 second timeout
traceroute -n -q 1 -w 2 8.8.8.8
```

**Sample output:**
```
traceroute to 8.8.8.8 (8.8.8.8), 64 hops max, 52 byte packets
 1  192.168.1.1  1.234 ms
 2  10.0.0.1  5.678 ms
 3  * * *
 4  72.14.215.85  12.345 ms
 5  8.8.8.8  15.678 ms
```

### Parsing Strategy

Each line follows pattern:
```
<hop_number>  <ip_or_*>  <latency_ms>
```

Parse into:
```rust
pub struct TracerouteHop {
    pub hop_number: u8,
    pub ip: Option<String>,      // None if timeout (*)
    pub hostname: Option<String>, // Not used with -n flag
    pub latency_ms: Option<f64>, // None if timeout
    pub timeout: bool,
}
```

### Failing Hop Identification

The failing hop is the **last responding hop** when target is unreachable:

```rust
fn identify_failing_hop(result: &TracerouteResult) -> Option<(u8, String)> {
    if result.success {
        return None; // No failure
    }

    // Find last hop that responded
    let last_responding = result.hops
        .iter()
        .rev()
        .find(|h| !h.timeout && h.ip.is_some());

    last_responding.map(|h| (h.hop_number, h.ip.clone().unwrap()))
}
```

**Example:**
```
1  192.168.1.1  1.234 ms   ← Hop 1 responded
2  10.0.0.1    5.678 ms    ← Hop 2 responded (last responding)
3  * * *                    ← Hop 3 timeout
4  * * *                    ← Hop 4 timeout
```
Result: Failing hop = 2 (10.0.0.1), meaning hop 3 is unreachable.

## Tasks

- [ ] Implement `trace()` function with shell-out
- [ ] Parse traceroute output line by line
- [ ] Handle various output formats
- [ ] Implement `identify_failing_hop()`
- [ ] Integrate with outage detection (Feature 003)
- [ ] Implement `networkmonitor trace` CLI command
- [ ] Store traceroute results in database
- [ ] Add unit tests with sample outputs

## Test Plan

### Unit Tests

```rust
#[test]
fn test_parse_traceroute_success() {
    let output = r#"
traceroute to 8.8.8.8 (8.8.8.8), 64 hops max
 1  192.168.1.1  1.234 ms
 2  10.0.0.1  5.678 ms
 3  8.8.8.8  15.678 ms
"#;
    let result = parse_traceroute(output, "8.8.8.8");
    assert!(result.success);
    assert_eq!(result.hops.len(), 3);
}

#[test]
fn test_parse_traceroute_partial_failure() {
    let output = r#"
traceroute to 8.8.8.8 (8.8.8.8), 64 hops max
 1  192.168.1.1  1.234 ms
 2  10.0.0.1  5.678 ms
 3  * * *
 4  * * *
"#;
    let result = parse_traceroute(output, "8.8.8.8");
    assert!(!result.success);

    let (hop, ip) = identify_failing_hop(&result).unwrap();
    assert_eq!(hop, 2);
    assert_eq!(ip, "10.0.0.1");
}

#[test]
fn test_parse_traceroute_all_timeout() {
    let output = r#"
traceroute to 8.8.8.8 (8.8.8.8), 64 hops max
 1  * * *
 2  * * *
"#;
    let result = parse_traceroute(output, "8.8.8.8");
    assert!(!result.success);
    assert!(identify_failing_hop(&result).is_none()); // No responding hop
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_trace_localhost() {
    let analyzer = HopAnalyzer::new(Duration::from_secs(5));
    let result = analyzer.trace("127.0.0.1").await;
    assert!(result.success);
}
```

## CLI Integration

### Manual Traceroute Command

```bash
$ networkmonitor trace 8.8.8.8

Traceroute to 8.8.8.8
═══════════════════════════════════════════════════════════

Hop  IP              Latency
───────────────────────────────────
  1  192.168.1.1     1.23 ms
  2  10.0.0.1        5.67 ms
  3  72.14.215.85   12.34 ms
  4  8.8.8.8        15.67 ms

Target reached in 4 hops.
```

## Integration with Other Features

### From Feature 003 (Outage Detection)

Triggered when entering OFFLINE state:

```rust
StateEvent::Offline { mut outage } => {
    // Run traceroute
    let trace_result = analyzer.trace("8.8.8.8").await;

    // Identify failing hop
    if let Some((hop, ip)) = identify_failing_hop(&trace_result) {
        outage.failing_hop = Some(hop);
        outage.failing_hop_ip = Some(ip);
    }

    // Store results
    db.insert_outage(&outage);
    db.insert_traceroute(outage.id, &trace_result);
}
```

## Hop Interpretation Guide

| Hop # | Typical Device | Failure Meaning |
|-------|----------------|-----------------|
| 1 | Local router/gateway | Local network issue, WiFi problem |
| 2 | ISP CPE/Modem | Fiber modem issue, ISP local problem |
| 3 | ISP first router | ISP infrastructure issue |
| 4+ | ISP backbone | Regional ISP issue |
| Last | Target | Target server down (not network) |

## Acceptance Criteria

1. Successfully runs traceroute via shell-out
2. Correctly parses hop information including timeouts
3. Identifies failing hop when target unreachable
4. Stores traceroute linked to outage record
5. CLI command displays results in readable format
