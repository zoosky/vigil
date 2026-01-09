# 010 - Enhanced Culprit Tracking

**Status:** Proposed

## Problem Statement

The current outage reporting does not clearly identify which network hop is responsible for connectivity issues:

1. **Traceroutes only captured at OFFLINE state** - By the time the system enters OFFLINE (after 5 consecutive failures), the transient cause may have changed. No data is captured during DEGRADED state.
2. **Single traceroute per outage** - Only one snapshot at outage start; no visibility into how the failure evolves or if the culprit changes.
3. **Report truncates IP addresses** - The failing hop IP is truncated to 8 characters (e.g., `10.0.0.1` becomes `10.0.0.`), making it difficult to identify the exact device.
4. **No traceroute details in report** - Users must query the database directly to see full traceroute data.

## Objectives

- Capture traceroutes during DEGRADED state (early warning)
- Run periodic traceroutes during ongoing outages
- Store all traceroutes with proper outage/event linkage
- Display clear culprit information in outage reports
- Provide detailed traceroute view per outage

## Implementation

### 1. New Database Table: `degraded_events`

Track DEGRADED state transitions separately from outages:

```sql
CREATE TABLE degraded_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    start_time TEXT NOT NULL,          -- RFC3339
    end_time TEXT,                      -- RFC3339, NULL if ongoing
    duration_secs REAL,
    escalated_to_outage_id INTEGER,    -- FK to outages.id if became OFFLINE
    affected_targets TEXT NOT NULL,    -- JSON array
    notes TEXT
);

CREATE INDEX idx_degraded_start ON degraded_events(start_time);
```

### 2. Schema Changes: `traceroutes` Table

Add linkage to degraded events and periodic trace markers:

```sql
ALTER TABLE traceroutes ADD COLUMN degraded_event_id INTEGER REFERENCES degraded_events(id);
ALTER TABLE traceroutes ADD COLUMN trace_trigger TEXT DEFAULT 'state_change';
-- trace_trigger values: 'state_change', 'periodic', 'manual'
```

### 3. State Machine Changes

**File: `src/monitor/state.rs`**

Add new state event for DEGRADED with traceroute trigger:

```rust
pub enum StateEvent {
    /// Entered DEGRADED state - trigger initial traceroute
    Degraded {
        failing_targets: Vec<String>,
        event_id: Option<i64>,  // New: link to degraded_events record
    },
    /// Entered OFFLINE state, outage started
    Offline { outage: Outage },
    /// Recovered to ONLINE state, outage ended
    Recovered { outage: Outage },
    /// Recovered from DEGRADED to ONLINE (no outage)
    DegradedRecovered { event_id: i64 },
    /// State unchanged
    NoChange,
}
```

### 4. Traceroute Triggers

**When to run traceroutes:**

| Trigger | Condition | trace_trigger value |
|---------|-----------|---------------------|
| DEGRADED entry | State changes ONLINE → DEGRADED | `state_change` |
| OFFLINE entry | State changes DEGRADED → OFFLINE | `state_change` |
| Periodic during OFFLINE | Every `traceroute_interval` seconds while OFFLINE | `periodic` |
| Manual | User runs `vigil trace` | `manual` |

**Configuration addition:**

```toml
[monitor]
# Existing settings...
traceroute_interval = 60    # Seconds between periodic traceroutes during outage
max_traceroutes_per_outage = 10  # Limit storage
```

### 5. Enhanced Monitor Loop

**File: `src/main.rs`**

```rust
// Track last traceroute time for periodic traces
let mut last_traceroute: Option<Instant> = None;

loop {
    // ... existing ping processing ...

    match event {
        StateEvent::Degraded { failing_targets, event_id } => {
            info!("DEGRADED: {} targets failing", failing_targets.len());

            // Run traceroute immediately
            let trace_result = analyzer.trace(&first_target).await;
            if let Some((hop, ip)) = HopAnalyzer::identify_failing_hop(&trace_result) {
                info!("Failing hop during degraded: {} ({})", hop, ip);
            }

            db.insert_traceroute(None, event_id, &trace_result, "state_change")?;
            last_traceroute = Some(Instant::now());
        }

        StateEvent::Offline { mut outage } => {
            // Run traceroute
            let trace_result = analyzer.trace(&first_target).await;
            if let Some((hop, ip)) = HopAnalyzer::identify_failing_hop(&trace_result) {
                outage.failing_hop = Some(hop);
                outage.failing_hop_ip = Some(ip.clone());
            }

            db.insert_outage(&outage)?;
            db.insert_traceroute(Some(outage.id), None, &trace_result, "state_change")?;
            last_traceroute = Some(Instant::now());
        }

        StateEvent::NoChange if tracker.state() == Offline => {
            // Periodic traceroute during outage
            if let Some(last) = last_traceroute {
                if last.elapsed() >= Duration::from_secs(config.traceroute_interval) {
                    let trace_result = analyzer.trace(&first_target).await;
                    db.insert_traceroute(Some(outage_id), None, &trace_result, "periodic")?;
                    last_traceroute = Some(Instant::now());
                }
            }
        }

        _ => {}
    }
}
```

### 6. Enhanced Outage Report

**File: `src/cli/outages.rs`**

Improve display with full IP and hop interpretation:

```rust
fn print_outage_row(outage: &Outage) {
    let start_time = outage.start_time.format("%Y-%m-%d %H:%M:%S").to_string();

    let duration = outage
        .duration_secs
        .map(format_duration_secs)
        .unwrap_or_else(|| "ongoing".to_string());

    // Enhanced failing hop display - NO truncation
    let failing_hop = match (outage.failing_hop, &outage.failing_hop_ip) {
        (Some(hop), Some(ip)) => {
            let hop_name = interpret_hop(hop);
            format!("Hop {} {} ({})", hop, ip, hop_name)
        }
        (Some(hop), None) => format!("Hop {}", hop),
        (None, _) => "Unknown".to_string(),
    };

    println!(
        "{:<19}  {:>8}  {}",
        start_time,
        duration,
        failing_hop
    );

    // Print affected targets on separate line if present
    if !outage.affected_targets.is_empty() {
        println!("                     Targets: {}", outage.affected_targets.join(", "));
    }
}

fn interpret_hop(hop: u8) -> &'static str {
    match hop {
        1 => "Gateway",
        2 => "ISP Modem",
        3 => "ISP Router",
        _ => "ISP Backbone",
    }
}
```

**New output format:**

```
Recent Outages (last 24h)
═══════════════════════════════════════════════════════════

Start Time           Duration  Culprit
───────────────────────────────────────────────────────────
2024-01-15 14:23:45     1m 32s  Hop 2 10.0.0.1 (ISP Modem)
                               Targets: 8.8.8.8, 1.1.1.1
2024-01-15 16:45:12       45s   Hop 3 72.14.215.85 (ISP Router)
                               Targets: 8.8.8.8
───────────────────────────────────────────────────────────

Summary: 2 outages, 2m 17s total downtime
Most common culprit: Hop 2 - ISP Modem (1 occurrence)
```

### 7. New CLI Command: `outage detail`

Show detailed traceroute information for a specific outage:

```bash
$ vigil outage 42

Outage #42 Details
═══════════════════════════════════════════════════════════

Started:     2024-01-15 14:23:45
Ended:       2024-01-15 14:25:17
Duration:    1m 32s
Culprit:     Hop 2 - 10.0.0.1 (ISP Modem)
Targets:     8.8.8.8, 1.1.1.1

Traceroutes (3 captured)
───────────────────────────────────────────────────────────

[14:23:45] state_change - Target: 8.8.8.8
  1  192.168.1.1     1.2 ms   ✓ Gateway
  2  10.0.0.1        5.6 ms   ✓ ISP Modem
  3  * * *                    ✗ TIMEOUT
  4  * * *                    ✗ TIMEOUT
  → Last responding: Hop 2 (10.0.0.1)

[14:24:45] periodic - Target: 8.8.8.8
  1  192.168.1.1     1.1 ms   ✓ Gateway
  2  10.0.0.1        8.2 ms   ✓ ISP Modem  (latency increased)
  3  * * *                    ✗ TIMEOUT
  → Last responding: Hop 2 (10.0.0.1)

[14:25:15] periodic - Target: 8.8.8.8
  1  192.168.1.1     1.0 ms   ✓ Gateway
  2  10.0.0.1        4.5 ms   ✓ ISP Modem
  3  72.14.215.85   12.3 ms   ✓ ISP Router
  4  8.8.8.8        15.6 ms   ✓ Target reached
  → Connection recovered
```

### 8. Database Query Methods

**File: `src/db.rs`**

```rust
impl Database {
    /// Insert traceroute with trigger type
    pub fn insert_traceroute(
        &self,
        outage_id: Option<i64>,
        degraded_event_id: Option<i64>,
        result: &TracerouteResult,
        trigger: &str,
    ) -> Result<i64>;

    /// Get all traceroutes for an outage
    pub fn get_traceroutes_for_outage(&self, outage_id: i64) -> Result<Vec<TracerouteResult>>;

    /// Get outage with full details including traceroutes
    pub fn get_outage_detail(&self, outage_id: i64) -> Result<OutageDetail>;

    /// Insert degraded event
    pub fn insert_degraded_event(&self, event: &DegradedEvent) -> Result<i64>;

    /// Update degraded event on recovery or escalation
    pub fn update_degraded_event(&self, event: &DegradedEvent) -> Result<()>;
}

pub struct OutageDetail {
    pub outage: Outage,
    pub traceroutes: Vec<TracerouteWithMeta>,
}

pub struct TracerouteWithMeta {
    pub result: TracerouteResult,
    pub trigger: String,  // state_change, periodic, manual
    pub timestamp: DateTime<Utc>,
}
```

## Tasks

- [ ] Add `degraded_events` table to schema
- [ ] Add columns to `traceroutes` table (`degraded_event_id`, `trace_trigger`)
- [ ] Write database migration for existing installations
- [ ] Implement `DegradedEvent` model and DB methods
- [ ] Update `StateEvent::Degraded` to include event ID
- [ ] Add `DegradedRecovered` state event
- [ ] Implement traceroute on DEGRADED state entry
- [ ] Implement periodic traceroutes during OFFLINE
- [ ] Add configuration options (`traceroute_interval`, `max_traceroutes_per_outage`)
- [ ] Update `outages` command to show full IP (no truncation)
- [ ] Add `outage <id>` subcommand for detailed view
- [ ] Format traceroute output with hop interpretation
- [ ] Add unit tests for new state transitions
- [ ] Add integration tests for periodic traceroutes
- [ ] Update documentation

## Test Plan

### Unit Tests

```rust
#[test]
fn test_degraded_triggers_traceroute_event() {
    let mut tracker = ConnectivityTracker::new(&config, &targets);

    // Send failures to trigger DEGRADED
    for _ in 0..3 {
        tracker.process(failing_result());
    }

    let event = tracker.last_event();
    assert!(matches!(event, StateEvent::Degraded { .. }));
}

#[test]
fn test_degraded_recovery_without_outage() {
    let mut tracker = /* in DEGRADED state */;

    // Recover before hitting OFFLINE threshold
    for _ in 0..2 {
        tracker.process(success_result());
    }

    assert!(matches!(tracker.last_event(), StateEvent::DegradedRecovered { .. }));
    assert_eq!(tracker.state(), ConnectivityState::Online);
}

#[test]
fn test_periodic_traceroute_timing() {
    // Verify traceroutes happen at configured interval
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_traceroute_stored_on_degraded() {
    // Trigger DEGRADED, verify traceroute saved with correct trigger
}

#[tokio::test]
async fn test_multiple_traceroutes_during_outage() {
    // Trigger OFFLINE, wait for periodic traces, verify all stored
}

#[tokio::test]
async fn test_outage_detail_command() {
    // Create outage with multiple traceroutes
    // Run `outage <id>` command
    // Verify all traceroute data displayed
}
```

## Migration Strategy

For existing installations with the old schema:

```sql
-- Add new columns with defaults
ALTER TABLE traceroutes ADD COLUMN degraded_event_id INTEGER;
ALTER TABLE traceroutes ADD COLUMN trace_trigger TEXT DEFAULT 'state_change';

-- Create new table
CREATE TABLE IF NOT EXISTS degraded_events (...);

-- Existing traceroutes are assumed to be 'state_change' triggers
```

## Acceptance Criteria

1. Traceroutes captured when entering DEGRADED state
2. Periodic traceroutes run during OFFLINE state at configured interval
3. Outage report shows full IP address without truncation
4. Outage report shows hop interpretation (Gateway, ISP Modem, etc.)
5. `vigil outage <id>` shows all traceroutes with timestamps
6. Traceroute count limited by `max_traceroutes_per_outage` config
7. DEGRADED events tracked separately and linked to outages if escalated
8. All traceroutes linked to their triggering event (outage or degraded)
9. Database migration works for existing installations

## Future Considerations

- Run traceroutes to multiple targets during outage to triangulate failure
- Compare outage traceroutes to baseline (healthy state) traceroutes
- DNS reverse lookup for hop IPs to show hostnames
- Alert when culprit changes during an outage
