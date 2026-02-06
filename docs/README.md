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
| [003](./features/003-outage-detection.md) | Outage Detection | Done | State machine for connectivity tracking |
| [004](./features/004-hop-analysis.md) | Hop Analysis | Done | Traceroute integration for fault isolation |
| [005](./features/005-cli-reporting.md) | CLI & Reporting | Done | Status display, outage history, statistics |
| [006](./features/006-polish-service.md) | Polish & Service | Done | Launchd, graceful shutdown, log rotation |
| [007](./features/007-alerts-notifications.md) | Alerts & Notifications | Pending | Desktop, webhook, command notifications |
| [008](./features/008-latency-quality-metrics.md) | Latency Quality Metrics | Pending | Jitter, packet loss, MOS score |
| [009](./features/009-http-endpoint-monitoring.md) | HTTP Endpoint Monitoring | Pending | Timing breakdown, cert tracking, HTTP CLI |
| [010](./features/010-enhanced-culprit-tracking.md) | Enhanced Culprit Tracking | Done | Periodic traceroutes, degraded events |
| [011](./features/011-dev-environment-upgrade-strategy.md) | Dev Environment & Upgrades | Done | Environment isolation, DB migrations |
| [012](./features/012-ci-pipeline-fix.md) | CI Pipeline Fix | Done | GitHub Actions fix and simplification |
| [013](./features/013-gateway-first-diagnosis.md) | Gateway-First Diagnosis | Done | Gateway ping before traceroute |
| [014](./features/014-tcp-connectivity-monitoring.md) | TCP/HTTP Connectivity | Done | TCP and HTTP monitoring methods |
| [015](./features/015-version-info.md) | Version Information | Done | Build info, schema status, JSON output |
| [016](./features/016-process-timeout.md) | Process Timeout | Done | Subprocess hard timeout, slow ping detection |

## Quick Start

```bash
# Initialize (creates config and database)
vigil init

# Start monitoring
vigil start --foreground

# Check status
vigil status

# View recent outages
vigil outages -l 24h

# View version and build info
vigil version
```

## Problem Statement

Home network with WLAN/ETH connection through a fiber router experiences intermittent outages (1-60 seconds) multiple times daily. This tool monitors connectivity continuously and identifies which network hop is responsible for the failures.
