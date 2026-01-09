# 001 - Core Infrastructure

**Status:** Done

## Overview

Foundation layer providing configuration management, database operations, logging, and CLI skeleton.

## Deliverables

- [x] Project setup with Cargo.toml
- [x] Directory structure (`src/`, `src/monitor/`, `src/cli/`)
- [x] Configuration management (`config.rs`)
- [x] SQLite database setup (`db.rs`)
- [x] Data models (`models.rs`)
- [x] Logging framework (`lib.rs`)
- [x] CLI skeleton with clap (`main.rs`)
- [x] Gateway auto-detection

## Files Created

```
Cargo.toml
src/
├── main.rs          # CLI entry point
├── lib.rs           # Library root + init_logging()
├── config.rs        # Config struct + load/save
├── db.rs            # Database struct + CRUD operations
├── models.rs        # Outage, PingResult, TracerouteHop, etc.
├── monitor/
│   ├── mod.rs       # Module declarations
│   ├── ping.rs      # Placeholder
│   ├── state.rs     # Placeholder
│   └── traceroute.rs # Placeholder
└── cli/
    ├── mod.rs       # Module declarations
    ├── start.rs     # Placeholder
    ├── status.rs    # Placeholder
    ├── outages.rs   # Placeholder
    └── stats.rs     # Placeholder
```

## Configuration Schema

```toml
[monitor]
ping_interval_ms = 1000
ping_timeout_ms = 2000
degraded_threshold = 3
offline_threshold = 5
recovery_threshold = 2

[targets]
gateway = "192.168.1.1"  # Optional, auto-detected
targets = [
    { name = "Google DNS", ip = "8.8.8.8" },
    { name = "Cloudflare", ip = "1.1.1.1" },
]

[database]
path = "..."  # Optional, uses default
retention_days = 90

[logging]
level = "info"
file = "..."  # Optional
```

## Database Schema

```sql
CREATE TABLE outages (
    id INTEGER PRIMARY KEY,
    start_time TEXT NOT NULL,
    end_time TEXT,
    duration_secs REAL,
    affected_targets TEXT,  -- JSON array
    failing_hop INTEGER,
    failing_hop_ip TEXT,
    notes TEXT
);

CREATE TABLE ping_log (
    id INTEGER PRIMARY KEY,
    timestamp TEXT NOT NULL,
    target TEXT NOT NULL,
    target_name TEXT NOT NULL,
    latency_ms REAL,
    success INTEGER NOT NULL
);

CREATE TABLE traceroutes (
    id INTEGER PRIMARY KEY,
    outage_id INTEGER REFERENCES outages(id),
    timestamp TEXT NOT NULL,
    target TEXT NOT NULL,
    hops TEXT NOT NULL,  -- JSON array
    success INTEGER NOT NULL
);
```

## CLI Commands (Skeleton)

```bash
vigil init          # Initialize config + DB
vigil start         # Start monitoring (placeholder)
vigil status        # Show status (placeholder)
vigil outages       # List outages (placeholder)
vigil stats         # Show statistics (placeholder)
vigil trace         # Manual traceroute (placeholder)
vigil config show   # Display config
vigil config path   # Show config file path
```

## Tests

7 unit tests passing:

- `config::tests::test_default_config`
- `config::tests::test_parse_config`
- `db::tests::test_create_database`
- `db::tests::test_insert_and_get_outage`
- `db::tests::test_insert_ping`
- `db::tests::test_stats`
- `tests::test_detect_gateway`

## Dependencies

```toml
tokio = { version = "1", features = ["full", "signal"] }
clap = { version = "4", features = ["derive"] }
rusqlite = { version = "0.31", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
chrono = { version = "0.4", features = ["serde"] }
thiserror = "1"
directories = "5"
tabled = "0.15"
indicatif = "0.17"
```

## Next Steps

Proceed to [002-ping-monitor.md](./002-ping-monitor.md) to implement the continuous ping monitoring system.
