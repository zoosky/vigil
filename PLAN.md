# Network Monitor - Implementation Plan

## Problem Statement

Home network with WLAN/ETH connection through a fiber router experiences intermittent outages (1-60 seconds) multiple times daily. Need to identify the culprit by monitoring connectivity and analyzing network hops.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                     Network Monitor                             │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐   │
│  │ Ping Monitor │  │ Hop Analyzer │  │ Outage Detector      │   │
│  │ (continuous) │  │ (on-demand)  │  │ (state machine)      │   │
│  └──────┬───────┘  └──────┬───────┘  └──────────┬───────────┘   │
│         │                 │                      │              │
│         └─────────────────┼──────────────────────┘              │
│                           ▼                                     │
│                   ┌───────────────┐                             │
│                   │ Event Logger  │                             │
│                   │ (SQLite)      │                             │
│                   └───────────────┘                             │
└─────────────────────────────────────────────────────────────────┘
```

## Network Topology to Monitor

```
[This Machine] → [Local Gateway/Router] → [ Fiber Router] → [ISP] → [Internet]
     hop 0            hop 1                    hop 2              hop 3+    target
```

## Core Components

### 1. Multi-Target Ping Monitor

Monitor multiple targets to isolate failure points:

| Target | Purpose | Interval |
|--------|---------|----------|
| Gateway (e.g., 192.168.1.1) | Local network health | 1s |
| Fiber router (if separate IP) | Router health | 1s |
| 8.8.8.8 (Google DNS) | Internet connectivity | 1s |
| 1.1.1.1 (Cloudflare DNS) | Redundant internet check | 1s |
| Custom target (configurable) | User-defined | 1s |

### 2. Outage Detection State Machine

```
                    ┌─────────────────┐
                    │     ONLINE      │
                    └────────┬────────┘
                             │ ping fails (threshold: 3 consecutive)
                             ▼
                    ┌─────────────────┐
                    │    DEGRADED     │──── ping succeeds ────┐
                    └────────┬────────┘                       │
                             │ ping fails (threshold: 5 consecutive)
                             ▼                                │
                    ┌─────────────────┐                       │
              ┌─────│    OFFLINE      │───────────────────────┘
              │     └─────────────────┘   ping succeeds (2 consecutive)
              │
              └──► Trigger hop analysis (traceroute)
```

### 3. Hop Analyzer

When outage detected, run traceroute to identify failing hop:

```rust
// Shell out to macOS traceroute
// traceroute -n -q 1 -w 2 8.8.8.8

// Parse output to identify:
// - Last responding hop (likely culprit is next hop)
// - Response times per hop
// - Packet loss per hop
```

### 4. Event Logger (SQLite)

**Tables:**

```sql
-- Outage events
CREATE TABLE outages (
    id INTEGER PRIMARY KEY,
    start_time TEXT NOT NULL,      -- ISO 8601
    end_time TEXT,                 -- NULL if ongoing
    duration_secs REAL,
    affected_targets TEXT,         -- JSON array
    failing_hop INTEGER,           -- Hop number where failure occurs
    failing_hop_ip TEXT,
    notes TEXT
);

-- Continuous ping log (sampled/aggregated)
CREATE TABLE ping_log (
    id INTEGER PRIMARY KEY,
    timestamp TEXT NOT NULL,
    target TEXT NOT NULL,
    latency_ms REAL,               -- NULL if timeout
    success INTEGER NOT NULL       -- 0 or 1
);

-- Traceroute snapshots
CREATE TABLE traceroutes (
    id INTEGER PRIMARY KEY,
    outage_id INTEGER REFERENCES outages(id),
    timestamp TEXT NOT NULL,
    hops TEXT NOT NULL             -- JSON array of hop data
);
```

### 5. CLI Interface

```bash
# Start monitoring daemon
networkmonitor start

# Check current status
networkmonitor status

# View recent outages
networkmonitor outages [--last 24h | --last 7d | --since "2024-01-01"]

# View statistics
networkmonitor stats

# Run manual traceroute
networkmonitor trace [target]

# Export logs
networkmonitor export --format csv --output outages.csv

# Configuration
networkmonitor config --set ping_interval=1000
networkmonitor config --add-target 8.8.4.4
```

## Implementation Steps

### Phase 1: Core Infrastructure
- [ ] Project setup with Cargo workspace structure
- [ ] Configuration management (TOML file + CLI overrides)
- [ ] SQLite database setup with migrations
- [ ] Logging framework (tracing crate)

### Phase 2: Ping Monitor
- [ ] Implement ping using `ping` shell command (macOS)
- [ ] Parse ping output for latency/success
- [ ] Multi-target concurrent pinging with tokio
- [ ] Configurable intervals and timeouts

### Phase 3: Outage Detection
- [ ] State machine implementation
- [ ] Threshold configuration
- [ ] Event emission on state transitions
- [ ] Outage duration tracking

### Phase 4: Hop Analysis
- [ ] Implement traceroute shell-out
- [ ] Parse traceroute output
- [ ] Identify failing hop logic
- [ ] Store traceroute snapshots

### Phase 5: CLI & Reporting
- [ ] CLI argument parsing (clap)
- [ ] Status display
- [ ] Outage history with filtering
- [ ] Statistics calculation
- [ ] CSV/JSON export

### Phase 6: Polish
- [ ] Graceful shutdown handling
- [ ] Launchd service file for macOS
- [ ] Log rotation
- [ ] Memory-efficient long-running operation

## Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
clap = { version = "4", features = ["derive"] }
rusqlite = { version = "0.31", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
chrono = { version = "0.4", features = ["serde"] }
thiserror = "1"
directories = "5"                  # XDG/macOS config paths
tabled = "0.15"                    # CLI table output
indicatif = "0.17"                 # Progress bars
```

## Configuration File

Location: `~/.config/networkmonitor/config.toml`

```toml
[monitor]
ping_interval_ms = 1000
ping_timeout_ms = 2000
degraded_threshold = 3      # consecutive failures to enter DEGRADED
offline_threshold = 5       # consecutive failures to enter OFFLINE
recovery_threshold = 2      # consecutive successes to recover

[targets]
gateway = "192.168.1.1"     # Auto-detect if not set
targets = [
    { name = "Google DNS", ip = "8.8.8.8" },
    { name = "Cloudflare", ip = "1.1.1.1" },
]

[database]
path = "~/.local/share/networkmonitor/monitor.db"
retention_days = 90

[logging]
level = "info"
file = "~/.local/share/networkmonitor/monitor.log"
```

## macOS Shell Commands Used

```bash
# Ping (single packet, 2 second timeout)
ping -c 1 -W 2000 8.8.8.8

# Traceroute (numeric, 1 query per hop, 2 second wait)
traceroute -n -q 1 -w 2 8.8.8.8

# Get default gateway
route -n get default | grep gateway

# Get network interface info
networksetup -listallhardwareports
ifconfig en0
```

## Output Examples

### Status Command
```
$ networkmonitor status

Network Monitor Status
══════════════════════════════════════════════════════════

Current State: ONLINE ✓
Uptime: 2h 34m 12s (since last outage)

Target Health:
  Gateway (192.168.1.1)     ✓  12ms
  Google DNS (8.8.8.8)      ✓  18ms
  Cloudflare (1.1.1.1)      ✓  15ms

Today's Statistics:
  Outages: 3
  Total downtime: 1m 45s
  Availability: 99.88%
```

### Outages Command
```
$ networkmonitor outages --last 24h

Recent Outages (last 24 hours)
══════════════════════════════════════════════════════════

┌─────────────────────┬──────────┬─────────────┬───────────────────┐
│ Start Time          │ Duration │ Failing Hop │ Affected Targets  │
├─────────────────────┼──────────┼─────────────┼───────────────────┤
│ 2024-01-15 14:23:05 │ 12s      │ 3 (ISP)     │ 8.8.8.8, 1.1.1.1  │
│ 2024-01-15 09:45:32 │ 45s      │ 2 (Home)   │ All targets       │
│ 2024-01-15 03:12:18 │ 8s       │ 3 (ISP)     │ 8.8.8.8, 1.1.1.1  │
└─────────────────────┴──────────┴─────────────┴───────────────────┘

Summary: 3 outages, 1m 5s total downtime
Most common failing hop: Hop 3 (ISP gateway) - 2 occurrences
```

## File Structure

```
my-networkmonitor/
├── Cargo.toml
├── PLAN.md
├── src/
│   ├── main.rs              # CLI entry point
│   ├── lib.rs               # Library root
│   ├── config.rs            # Configuration management
│   ├── db.rs                # SQLite operations
│   ├── monitor/
│   │   ├── mod.rs
│   │   ├── ping.rs          # Ping implementation
│   │   ├── state.rs         # State machine
│   │   └── traceroute.rs    # Traceroute implementation
│   ├── cli/
│   │   ├── mod.rs
│   │   ├── start.rs         # Start command
│   │   ├── status.rs        # Status command
│   │   ├── outages.rs       # Outages command
│   │   └── stats.rs         # Stats command
│   └── models.rs            # Data structures
└── tests/
    └── integration.rs
```

## Success Criteria

1. **Reliability**: Runs continuously without crashes or memory leaks
2. **Accuracy**: Detects outages within 3 seconds of occurrence
3. **Insight**: Correctly identifies failing network hop 90%+ of the time
4. **Usability**: Clear CLI output helps diagnose network issues
5. **Performance**: Minimal CPU/memory footprint (<1% CPU, <50MB RAM)
