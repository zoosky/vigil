# Architecture

## System Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                     Network Monitor                              │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │ Ping Monitor │  │ Hop Analyzer │  │ Outage Detector      │  │
│  │ (continuous) │  │ (on-demand)  │  │ (state machine)      │  │
│  └──────┬───────┘  └──────┬───────┘  └──────────┬───────────┘  │
│         │                 │                      │              │
│         └─────────────────┼──────────────────────┘              │
│                           ▼                                      │
│                   ┌───────────────┐                              │
│                   │ Event Logger  │                              │
│                   │ (SQLite)      │                              │
│                   └───────────────┘                              │
└─────────────────────────────────────────────────────────────────┘
```

## Network Topology

The tool monitors connectivity across multiple network hops:

```
[This Machine] → [Local Gateway] → [Home Router] → [ISP] → [Internet]
     hop 0           hop 1            hop 2         hop 3+    target
```

By monitoring multiple targets and running traceroute during outages, we can identify which hop is failing.

## Components

### 1. Ping Monitor (`src/monitor/ping.rs`)

- Continuously pings multiple targets at configurable intervals
- Uses macOS `ping` command via shell-out
- Parses output for latency and success/failure
- Runs concurrently using tokio tasks

**Targets monitored:**
- Local gateway (auto-detected or configured)
- External DNS servers (8.8.8.8, 1.1.1.1)
- Custom targets (user-configured)

### 2. State Machine (`src/monitor/state.rs`)

Tracks connectivity state with hysteresis to avoid flapping:

```
                    ┌─────────────────┐
                    │     ONLINE      │
                    └────────┬────────┘
                             │ N consecutive failures
                             ▼
                    ┌─────────────────┐
                    │    DEGRADED     │──── success ────┐
                    └────────┬────────┘                 │
                             │ M more failures          │
                             ▼                          │
                    ┌─────────────────┐                 │
                    │    OFFLINE      │─────────────────┘
                    └─────────────────┘   K consecutive successes
```

**Thresholds (configurable):**
- `degraded_threshold`: 3 consecutive failures → DEGRADED
- `offline_threshold`: 5 consecutive failures → OFFLINE
- `recovery_threshold`: 2 consecutive successes → ONLINE

### 3. Hop Analyzer (`src/monitor/traceroute.rs`)

- Triggered when entering OFFLINE state
- Runs macOS `traceroute` command
- Parses output to identify failing hop
- Stores results linked to outage events

### 4. Database (`src/db.rs`)

SQLite database with three tables:

**outages** - Outage events
```sql
- id, start_time, end_time, duration_secs
- affected_targets (JSON array)
- failing_hop, failing_hop_ip
- notes
```

**ping_log** - Individual ping results (sampled)
```sql
- id, timestamp, target, target_name
- latency_ms, success
```

**traceroutes** - Traceroute snapshots
```sql
- id, outage_id, timestamp, target
- hops (JSON array), success
```

### 5. Configuration (`src/config.rs`)

TOML-based configuration at:
- macOS: `~/Library/Application Support/com.kapptec.networkmonitor/config.toml`

Supports:
- Monitor settings (intervals, thresholds)
- Target list (gateway, DNS servers, custom)
- Database path and retention
- Logging level and file path

## Data Flow

```
1. Ping Monitor sends pings every 1 second
                    │
                    ▼
2. Results fed to State Machine
                    │
                    ├── State unchanged → Log ping result
                    │
                    ├── State → OFFLINE
                    │       │
                    │       ▼
                    │   Trigger Hop Analyzer
                    │       │
                    │       ▼
                    │   Create Outage record
                    │
                    └── State → ONLINE (from OFFLINE)
                            │
                            ▼
                        End Outage record
                        (set end_time, duration)
```

## File Structure

```
src/
├── main.rs              # CLI entry point (clap)
├── lib.rs               # Library root, logging init
├── config.rs            # Configuration management
├── db.rs                # SQLite operations
├── models.rs            # Data structures
├── monitor/
│   ├── mod.rs
│   ├── ping.rs          # Ping implementation
│   ├── state.rs         # State machine
│   └── traceroute.rs    # Traceroute implementation
└── cli/
    ├── mod.rs
    ├── start.rs         # Start command
    ├── status.rs        # Status command
    ├── outages.rs       # Outages command
    └── stats.rs         # Stats command
```

## macOS Integration

The tool shells out to standard macOS commands:

```bash
# Ping (single packet, timeout in ms)
ping -c 1 -W 2000 8.8.8.8

# Traceroute (numeric, 1 query, 2s wait)
traceroute -n -q 1 -w 2 8.8.8.8

# Gateway detection
route -n get default | grep gateway
```

## Performance Considerations

- **Memory**: Target <50MB RAM for long-running daemon
- **CPU**: Target <1% CPU average
- **Disk**: Ping logs sampled/aggregated to limit growth
- **Network**: ~1 ping/second per target (minimal overhead)
