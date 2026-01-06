# Network Monitor Documentation

A Rust-based network connectivity monitor for diagnosing intermittent outages on home networks.

## Documentation Index

### Core Documentation

- [Architecture](./architecture.md) - System design, components, and data flow
- [Usage Guide](./usage.md) - Installation, configuration, and CLI commands

### Feature Specifications

Implementation is organized into sequentially numbered features:

| # | Feature | Status | Description |
|---|---------|--------|-------------|
| [001](./features/001-core-infrastructure.md) | Core Infrastructure | Done | Config, database, logging, CLI skeleton |
| [002](./features/002-ping-monitor.md) | Ping Monitor | Done | Continuous multi-target ping monitoring |
| [003](./features/003-outage-detection.md) | Outage Detection | Pending | State machine for connectivity tracking |
| [004](./features/004-hop-analysis.md) | Hop Analysis | Pending | Traceroute integration for fault isolation |
| [005](./features/005-cli-reporting.md) | CLI & Reporting | Pending | Status display, outage history, statistics |
| [006](./features/006-polish-service.md) | Polish & Service | Pending | Launchd, graceful shutdown, log rotation |

## Quick Start

```bash
# Initialize (creates config and database)
networkmonitor init

# Start monitoring
networkmonitor start

# Check status
networkmonitor status

# View recent outages
networkmonitor outages --last 24h
```

## Problem Statement

Home network with WLAN/ETH connection through a Zyxel fiber router experiences intermittent outages (1-60 seconds) multiple times daily. This tool monitors connectivity continuously and identifies which network hop is responsible for the failures.
