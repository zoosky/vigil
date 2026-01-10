# Manual Testing Guide

This guide covers manual testing for Feature 010 (Enhanced Culprit Tracking).

## 1. Setup Development Environment

```bash
# Initialize dev environment (isolated database)
cargo run -- --dev init

# Verify config path
cargo run -- --dev config path
```

## 2. Configure for Faster Testing

Edit the dev config to use shorter thresholds:

```bash
# Show config location
cargo run -- --dev config path

# Edit config (e.g., vim or nano)
```

Suggested test config:

```toml
[monitor]
ping_interval_ms = 1000
degraded_threshold = 2      # Faster degraded trigger
offline_threshold = 4       # Faster offline trigger
recovery_threshold = 2
traceroute_interval_secs = 30  # More frequent periodic traces
max_traceroutes_per_outage = 5
```

## 3. Test Scenarios

### A. Test DEGRADED -> Recovery (no outage)

```bash
# Start monitoring in dev mode
cargo run -- --dev start --foreground
```

To trigger failures, either:
- Disconnect your network briefly (2-3 failures)
- Or add an unreachable target to config: `{ name = "Bad", ip = "192.0.2.1" }`

Watch for:
- `STATE: DEGRADED` message
- Traceroute running on DEGRADED entry
- `STATE: ONLINE - Recovered from DEGRADED` on recovery

### B. Test DEGRADED -> OFFLINE -> Recovery

Keep network disconnected longer (4+ failures) to hit OFFLINE:
- `STATE: OFFLINE` message
- Traceroute on OFFLINE entry
- Periodic traceroutes every 30s (if configured)
- `STATE: ONLINE - Outage ended` on recovery

### C. Verify CLI Output

```bash
# List outages with enhanced display
cargo run -- --dev outages --last 1h

# Should show:
# - Full IP address (no truncation)
# - Hop interpretation (Gateway, ISP Modem, etc.)
# - Targets on separate line
```

```bash
# Get detailed outage view (use actual ID from outages list)
cargo run -- --dev outage 1

# Should show:
# - Outage summary
# - All traceroutes with timestamps
# - Trigger type (state_change, periodic)
```

## 4. Database Verification

```bash
# Check database directly
sqlite3 ~/Library/Application\ Support/ch.kapptec.vigil/dev/monitor.db

# Verify schema
.schema degraded_events
.schema traceroutes

# Check data
SELECT * FROM degraded_events;
SELECT id, outage_id, degraded_event_id, trace_trigger, timestamp FROM traceroutes;
```

## 5. Migration Test

To test migration on existing data:

```bash
# Use production database copy
cp ~/Library/Application\ Support/ch.kapptec.vigil/monitor.db /tmp/test.db

# Run upgrade
cargo run -- upgrade --dry-run  # Preview
cargo run -- upgrade            # Apply migration
```

## Quick Simulation Method

If you don't want to actually disconnect your network, add an invalid target:

```toml
[targets]
targets = [
    { name = "Google DNS", ip = "8.8.8.8" },
    { name = "Unreachable", ip = "192.0.2.1" }  # TEST-NET, always fails
]
```

This will cause mixed failures and trigger the state machine without affecting real connectivity.

## Expected Results

### Outages List Output

```
Recent Outages (last 1h)
═══════════════════════════════════════════════════════════

  ID  Start Time           Duration  Culprit
───────────────────────────────────────────────────────────────────────────
   1  2024-01-15 14:23:45     1m 32s  Hop 2 10.0.0.1 (ISP Modem)
                                   Targets: 8.8.8.8, 1.1.1.1
───────────────────────────────────────────────────────────────────────────

Summary: 1 outage, 1m 32s total downtime
Most common culprit: Hop 2 - ISP Modem (1 occurrence)
```

Use the ID to get detailed view: `vigil outage 1`

### Outage Detail Output

```
Outage #1 Details
═══════════════════════════════════════════════════════════

Started:     2024-01-15 14:23:45
Ended:       2024-01-15 14:25:17
Duration:    1m 32s
Culprit:     Hop 2 - 10.0.0.1 (ISP Modem)
Targets:     8.8.8.8, 1.1.1.1

Traceroutes (3 captured)
───────────────────────────────────────────────────────────

[14:23:45] state_change - Target: 8.8.8.8
   1  192.168.1.1       1.2 ms   Gateway
   2  10.0.0.1          5.6 ms   ISP Modem
   3  * * *                      TIMEOUT
  -> Last responding: Hop 2 (10.0.0.1)

[14:24:15] periodic - Target: 8.8.8.8
   1  192.168.1.1       1.1 ms   Gateway
   2  10.0.0.1          8.2 ms   ISP Modem
   3  * * *                      TIMEOUT
  -> Last responding: Hop 2 (10.0.0.1)
```

## Checklist

- [ ] Dev environment initializes correctly
- [ ] DEGRADED state triggers traceroute
- [ ] OFFLINE state triggers traceroute
- [ ] Periodic traceroutes run during OFFLINE
- [ ] Recovery from DEGRADED works (DegradedRecovered event)
- [ ] Recovery from OFFLINE works (Recovered event)
- [ ] `vigil outages` shows full IP and hop interpretation
- [ ] `vigil outage <id>` shows detailed traceroute view
- [ ] Database migration works on existing data
- [ ] Config options respected (traceroute_interval_secs, max_traceroutes_per_outage)
