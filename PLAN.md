# Network Monitor - Implementation Plan

## Problem Statement

Home network with WLAN/ETH connection through a fiber router experiences intermittent outages (1-60 seconds) multiple times daily. Need to identify the culprit by monitoring connectivity and analyzing network hops.

## Architecture Overview

```
┌──────────────────────────────────────────────────────────────────┐
│                      Vigil Network Monitor                        │
├──────────────────────────────────────────────────────────────────┤
│  ┌───────────────────────┐  ┌──────────────────────────────────┐ │
│  │ Connectivity Checker  │  │ Outage Detector (state machine)  │ │
│  │ ┌───────┐ ┌─────┐    │  │ ONLINE → DEGRADED → OFFLINE      │ │
│  │ │ Ping  │ │ TCP │    │  └──────────────┬───────────────────┘ │
│  │ └───────┘ └─────┘    │                 │                     │
│  │ ┌───────┐             │                 ▼                     │
│  │ │ HTTP  │             │  ┌──────────────────────────────────┐ │
│  │ └───────┘             │  │ Hop Analyzer (gateway-first)     │ │
│  └───────────┬───────────┘  │ gateway ping → traceroute        │ │
│              │               └──────────────┬───────────────────┘ │
│              └──────────────────────────────┘                     │
│                              │                                    │
│                              ▼                                    │
│                      ┌───────────────┐                            │
│                      │ Event Logger  │                            │
│                      │ (SQLite v3)   │                            │
│                      └───────────────┘                            │
└──────────────────────────────────────────────────────────────────┘
```

## Network Topology to Monitor

```
[This Machine] → [Local Gateway/Router] → [Fiber Router] → [ISP] → [Internet]
     hop 0            hop 1                   hop 2          hop 3+    target
```

## Core Components

### 1. Multi-Target Connectivity Checker

Monitor multiple targets using configurable methods (TCP, ping, HTTP):

| Target | Purpose | Method | Interval |
|--------|---------|--------|----------|
| Gateway (auto-detected) | Local network health | Ping | 2s |
| 8.8.8.8 (Google DNS) | Internet connectivity | TCP:443 | 2s |
| 1.1.1.1 (Cloudflare DNS) | Redundant internet check | TCP:443 | 2s |
| Custom target (configurable) | User-defined | Any | 2s |

### 2. Outage Detection State Machine

```
                    ┌─────────────────┐
                    │     ONLINE      │
                    └────────┬────────┘
                             │ N consecutive failures (default: 2)
                             ▼
                    ┌─────────────────┐
                    │    DEGRADED     │──── K successes ────┐
                    └────────┬────────┘   (default: 3)      │
                             │ M more failures (default: 3)  │
                             ▼                               │
                    ┌─────────────────┐                      │
              ┌─────│    OFFLINE      │──────────────────────┘
              │     └─────────────────┘   K consecutive successes
              │
              └──► Trigger gateway-first diagnosis (traceroute)
```

### 3. Hop Analyzer (Gateway-First)

When outage detected:
1. Ping gateway to determine local network health
2. Run traceroute to identify failing hop
3. Classify diagnosis: `LocalNetworkDown`, `IspIssue`, `Healthy`, `Intermittent`, `Unknown`

```bash
# Shell out to macOS traceroute
traceroute -n -q 1 -w 2 8.8.8.8
```

### 4. Event Logger (SQLite v3)

**Tables:**

```sql
-- Outage events (OFFLINE state)
CREATE TABLE outages (
    id INTEGER PRIMARY KEY,
    start_time TEXT NOT NULL,
    end_time TEXT,
    duration_secs REAL,
    affected_targets TEXT,       -- JSON array
    failing_hop INTEGER,
    failing_hop_ip TEXT,
    notes TEXT
);

-- Continuous ping/connectivity log (sampled)
CREATE TABLE ping_log (
    id INTEGER PRIMARY KEY,
    timestamp TEXT NOT NULL,
    target TEXT NOT NULL,
    target_name TEXT NOT NULL,
    latency_ms REAL,
    success INTEGER NOT NULL
);

-- Traceroute snapshots with diagnosis metadata
CREATE TABLE traceroutes (
    id INTEGER PRIMARY KEY,
    outage_id INTEGER REFERENCES outages(id),
    degraded_event_id INTEGER REFERENCES degraded_events(id),
    trace_trigger TEXT DEFAULT 'state_change',
    gateway_reachable INTEGER,
    gateway_latency_ms REAL,
    diagnosis TEXT,
    timestamp TEXT NOT NULL,
    target TEXT NOT NULL,
    hops TEXT NOT NULL,          -- JSON array of hop data
    success INTEGER NOT NULL
);

-- Degraded state events (pre-outage)
CREATE TABLE degraded_events (
    id INTEGER PRIMARY KEY,
    start_time TEXT NOT NULL,
    end_time TEXT,
    duration_secs REAL,
    escalated_to_outage_id INTEGER REFERENCES outages(id),
    affected_targets TEXT NOT NULL,
    notes TEXT
);

-- Migration tracking
CREATE TABLE schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT (datetime('now')),
    description TEXT
);
```

### 5. CLI Interface

```bash
# Start monitoring daemon
vigil start [--foreground]

# Check current status
vigil status

# View recent outages
vigil outages [--last 24h | --last 7d]

# View specific outage with traceroute history
vigil outage <id>

# View statistics
vigil stats [-p 7d]

# Run manual traceroute
vigil trace [target]

# Configuration management
vigil config show
vigil config path
vigil config set <key> <value>

# Service management (macOS launchd)
vigil service install|uninstall|status|logs

# Database management
vigil init [--dev]
vigil upgrade [--dry-run] [--no-backup]
vigil cleanup [--days N]

# Version and build info
vigil version [--json]
```

## Implementation Steps

### Phase 1: Core Infrastructure (Features 001)

- [x] Project setup with Cargo
- [x] Configuration management (TOML file)
- [x] SQLite database setup with migrations
- [x] Logging framework (tracing crate)

### Phase 2: Ping Monitor (Feature 002)

- [x] Implement ping using `ping` shell command (macOS)
- [x] Parse ping output for latency/success
- [x] Multi-target concurrent pinging with tokio
- [x] Configurable intervals and timeouts

### Phase 3: Outage Detection (Feature 003)

- [x] State machine implementation (ONLINE/DEGRADED/OFFLINE)
- [x] Threshold configuration
- [x] Event emission on state transitions
- [x] Outage duration tracking

### Phase 4: Hop Analysis (Feature 004)

- [x] Implement traceroute shell-out
- [x] Parse traceroute output
- [x] Identify failing hop logic
- [x] Store traceroute snapshots

### Phase 5: CLI & Reporting (Feature 005)

- [x] CLI argument parsing (clap)
- [x] Status display
- [x] Outage history with filtering
- [x] Statistics calculation
- [ ] CSV/JSON export (deferred)

### Phase 6: Polish (Feature 006)

- [x] Graceful shutdown handling
- [x] Launchd service file for macOS
- [x] Log rotation
- [x] Memory-efficient long-running operation

### Phase 7: Extended Features (Features 010-016)

- [x] Enhanced culprit tracking with degraded events (Feature 010)
- [x] Development environment isolation and upgrade strategy (Feature 011)
- [x] CI pipeline fix (Feature 012)
- [x] Gateway-first diagnosis (Feature 013)
- [x] TCP/HTTP connectivity monitoring (Feature 014)
- [x] Version and build information (Feature 015)
- [x] Process timeout for hung subprocesses (Feature 016)

### Phase 8: Future (Features 007-009)

- [ ] Alerts and notifications — desktop, webhook, command (Feature 007)
- [ ] Latency quality metrics — jitter, packet loss, MOS score (Feature 008)
- [ ] Advanced HTTP monitoring — timing breakdown, cert tracking, CLI (Feature 009)

## Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["full", "signal"] }
clap = { version = "4", features = ["derive", "env"] }
rusqlite = { version = "0.31", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-appender = "0.2"
chrono = { version = "0.4", features = ["serde"] }
thiserror = "1"
directories = "5"
dirs = "5"
tabled = "0.15"
indicatif = "0.17"
futures = "0.3"
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls"] }
```

## Configuration File

Location: `~/Library/Application Support/ch.kapptec.vigil/config.toml`

```toml
[monitor]
ping_interval_ms = 2000           # How often to check (ms)
ping_timeout_ms = 2000            # Ping/connection timeout (ms)
degraded_threshold = 2            # Failures before DEGRADED state
offline_threshold = 3             # Failures before OFFLINE state
recovery_threshold = 3            # Successes to recover
traceroute_interval_secs = 60     # Periodic traceroute during outage
max_traceroutes_per_outage = 10   # Limit stored per outage
ping_process_timeout_ms = 6000    # Hard subprocess timeout
degraded_ping_interval_ms = 500   # Faster polling during DEGRADED

[targets]
gateway = "10.0.0.1"             # Auto-detected if not set
targets = [
    { name = "Google DNS", ip = "8.8.8.8", method = "tcp", port = 443 },
    { name = "Cloudflare", ip = "1.1.1.1", method = "tcp", port = 443 },
]
# method: "tcp" (default), "ping" (ICMP), or "http"

[database]
retention_days = 90

[logging]
level = "info"
```

## macOS Shell Commands Used

```bash
# Ping (single packet, timeout in milliseconds)
# Note: macOS -W flag takes milliseconds, not seconds
ping -c 1 -W 2000 8.8.8.8

# Traceroute (numeric, 1 query per hop, 2 second wait)
traceroute -n -q 1 -w 2 8.8.8.8

# Get default gateway
route -n get default | grep gateway
```

## File Structure

```
vigil/
├── Cargo.toml
├── build.rs                 # Build-time info (git hash, timestamp)
├── PLAN.md
├── README.md
├── claude.md                # Development context
├── scripts/
│   └── qa.sh                # QA: fmt, clippy, test, doc, build
├── src/
│   ├── main.rs              # CLI entry point (clap)
│   ├── lib.rs               # Library root, logging, version constants
│   ├── config.rs            # Configuration, environment support
│   ├── db.rs                # SQLite operations, migrations (v1→v3)
│   ├── models.rs            # Data structures (Target, Outage, PingResult, etc.)
│   ├── monitor/
│   │   ├── mod.rs           # Unified connectivity dispatcher
│   │   ├── ping.rs          # ICMP ping with process timeout
│   │   ├── tcp.rs           # TCP connectivity checks
│   │   ├── http.rs          # HTTP endpoint checks
│   │   ├── state.rs         # State machine (ONLINE/DEGRADED/OFFLINE)
│   │   └── traceroute.rs    # Traceroute + gateway-first diagnosis
│   └── cli/
│       ├── mod.rs
│       ├── start.rs         # Start monitor daemon
│       ├── status.rs        # Current status display
│       ├── outages.rs       # List outages
│       ├── outage_detail.rs # Detailed outage view with traceroutes
│       ├── stats.rs         # Statistics reporting
│       ├── service.rs       # macOS launchd service management
│       ├── version.rs       # Version and build info
│       └── helpers.rs       # Shared CLI utilities
└── docs/
    ├── README.md            # Documentation index
    ├── architecture.md      # System design
    ├── usage.md             # User guide
    └── features/            # Feature specifications (001-016)
```

## Success Criteria

1. **Reliability**: Runs continuously without crashes or memory leaks
2. **Accuracy**: Detects outages within seconds of occurrence
3. **Insight**: Correctly identifies failing network hop via gateway-first diagnosis
4. **Usability**: Clear CLI output helps diagnose network issues
5. **Performance**: Minimal CPU/memory footprint (<1% CPU, <50MB RAM)
