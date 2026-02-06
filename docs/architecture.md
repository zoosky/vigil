# Architecture

## System Overview

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

## Network Topology

The tool monitors connectivity across multiple network hops:

```
[This Machine] → [Local Gateway] → [Home Router] → [ISP] → [Internet]
     hop 0           hop 1            hop 2         hop 3+    target
```

By monitoring multiple targets and running traceroute during outages, we can identify which hop is failing.

## Components

### 1. Connectivity Checker (`src/monitor/`)

Unified dispatcher (`mod.rs`) routes checks to the appropriate method:

| Method | File | Protocol | Use Case |
|--------|------|----------|----------|
| Ping | `ping.rs` | ICMP | Traditional, may be rate-limited |
| TCP | `tcp.rs` | TCP SYN | Default — accurate for real-world connectivity |
| HTTP | `http.rs` | HTTP HEAD | Full HTTP check with response validation |

- Continuously checks multiple targets at configurable intervals (default 2s)
- Hard process timeout (default 6s) kills hung subprocesses
- Adaptive polling: 4x faster during DEGRADED state (500ms)
- Runs concurrently using tokio tasks

**Targets monitored:**

- Local gateway (auto-detected or configured)
- External DNS servers (8.8.8.8, 1.1.1.1) via TCP:443 by default
- Custom targets (user-configured, any method)

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

- `degraded_threshold`: 2 consecutive failures → DEGRADED
- `offline_threshold`: 3 consecutive failures → OFFLINE
- `recovery_threshold`: 3 consecutive successes → ONLINE

### 3. Hop Analyzer (`src/monitor/traceroute.rs`)

- Gateway-first diagnosis: pings gateway before running traceroute
- Triggered when entering DEGRADED or OFFLINE state
- Periodic traceroutes during ongoing outages (configurable interval)
- Runs macOS `traceroute` command
- Parses output to identify failing hop
- Stores results linked to outage or degraded events
- Diagnosis output: `LocalNetworkDown`, `IspIssue`, `Healthy`, `Intermittent`, `Unknown`

### 4. Database (`src/db.rs`)

SQLite database (schema v3) with five tables:

**outages** - Outage events (OFFLINE state)

```
id, start_time, end_time, duration_secs, affected_targets (JSON),
failing_hop, failing_hop_ip, notes
```

**degraded_events** - Degraded state transitions (pre-outage)

```
id, start_time, end_time, duration_secs, escalated_to_outage_id,
affected_targets (JSON), notes
```

**ping_log** - Individual ping results (sampled)

```
id, timestamp, target, target_name, latency_ms, success
```

**traceroutes** - Traceroute snapshots linked to outages or degraded events

```
id, outage_id, degraded_event_id, trace_trigger, gateway_reachable,
gateway_latency_ms, diagnosis, timestamp, target, hops (JSON), success
```

**schema_version** - Migration tracking

```
version, applied_at, description
```

### 5. Configuration (`src/config.rs`)

TOML-based configuration at:

- macOS: `~/Library/Application Support/ch.kapptec.vigil/config.toml`

Supports:

- Monitor settings (intervals, thresholds)
- Target list (gateway, DNS servers, custom)
- Database path and retention
- Logging level and file path

## Data Flow

```
1. Connectivity Checker runs checks every 2 seconds (ping/TCP/HTTP)
                    │
                    ▼
2. Results fed to State Machine
                    │
                    ├── State unchanged → Log ping result
                    │
                    ├── State → DEGRADED
                    │       │
                    │       ▼
                    │   Create DegradedEvent + run gateway-first diagnosis
                    │
                    ├── State → OFFLINE (from DEGRADED)
                    │       │
                    │       ▼
                    │   Create Outage record + run diagnosis
                    │   Periodic traceroutes every N seconds
                    │
                    ├── State → ONLINE (from DEGRADED)
                    │       │
                    │       ▼
                    │   End DegradedEvent (no outage created)
                    │
                    └── State → ONLINE (from OFFLINE)
                            │
                            ▼
                        End Outage record (set end_time, duration)
```

## File Structure

```
src/
├── main.rs              # CLI entry point (clap)
├── lib.rs               # Library root, logging init, version constants
├── config.rs            # Configuration management, environment support
├── db.rs                # SQLite operations, migrations (v1→v3)
├── models.rs            # Data structures (Target, Outage, PingResult, etc.)
├── monitor/
│   ├── mod.rs           # Unified connectivity dispatcher
│   ├── ping.rs          # ICMP ping with process timeout
│   ├── tcp.rs           # TCP connectivity checks
│   ├── http.rs          # HTTP endpoint checks
│   ├── state.rs         # State machine (ONLINE/DEGRADED/OFFLINE)
│   └── traceroute.rs    # Traceroute + gateway-first diagnosis
└── cli/
    ├── mod.rs
    ├── start.rs         # Start monitor daemon
    ├── status.rs        # Current status display
    ├── outages.rs       # List outages
    ├── outage_detail.rs # Detailed outage view with traceroutes
    ├── stats.rs         # Statistics reporting
    ├── service.rs       # macOS launchd service management
    ├── version.rs       # Version and build info
    └── helpers.rs       # Shared CLI utilities
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
