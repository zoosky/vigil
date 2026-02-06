# Claude Context - Vigil Network Monitor

This file provides context for Claude Code sessions working on this project.

## Project Overview

**Vigil networkmonitor** - A Rust CLI tool to monitor home network connectivity and diagnose intermittent outages by tracking which network hop is failing.

**Problem**: User has a fiber router with connection drops 1-60 seconds, multiple times daily.

**Solution**: Continuous ping monitoring with traceroute-based hop analysis to identify the culprit (local router, ISP equipment, or ISP backbone).

## Current State

| Feature | Status | File(s) |
|---------|--------|---------|
| 001 Core Infrastructure | Done | `config.rs`, `db.rs`, `models.rs`, `lib.rs`, `main.rs` |
| 002 Ping Monitor | Done | `src/monitor/ping.rs` |
| 003 Outage Detection | Done | `src/monitor/state.rs` |
| 004 Hop Analysis | Done | `src/monitor/traceroute.rs` |
| 005 CLI Reporting | Done | `src/cli/helpers.rs`, `status.rs`, `outages.rs`, `stats.rs` |
| 006 Polish & Service | Done | `src/cli/service.rs`, log rotation in `lib.rs` |
| 007 Alerts & Notifications | Pending | Desktop, webhook, command notifications |
| 008 Latency Quality Metrics | Pending | Jitter, packet loss, MOS score |
| 009 HTTP Endpoint Monitoring | Partial | Basic HTTP/TCP in 014; remaining: timing breakdown, cert tracking, `vigil http`/`vigil certs` |
| 010 Enhanced Culprit Tracking | Done | `src/db.rs` (migrate_v2), `src/monitor/state.rs`, `src/cli/outage_detail.rs` |
| 011 Dev Environment | Done | `config.rs` (Environment), `main.rs` (--dev flag) |
| 012 CI Pipeline Fix | Done | `.github/workflows/ci.yml` |
| 013 Gateway-First Diagnosis | Done | `src/monitor/traceroute.rs`, `main.rs`, `cli/outage_detail.rs` |
| 014 TCP/HTTP Connectivity | Done | `src/monitor/tcp.rs`, `src/monitor/http.rs`, `models.rs` |
| 015 Version Information | Done | `build.rs`, `src/lib.rs`, `src/cli/version.rs` |
| 016 Process Timeout | Done | `src/monitor/ping.rs`, `src/config.rs` (ping_process_timeout_ms) |
| 017 Code Audit Fixes | Pending | Bug fixes, security hardening from code audit |
| 018 Diagnostics & Patterns | Pending | Network diagnostics and pattern analysis |

## Development Workflow

```bash
# Initialize dev environment (isolated database)
cargo run -- --dev init

# Run commands in dev mode
cargo run -- --dev status
cargo run -- --dev start --foreground

# Or use cargo aliases
cargo dev status
cargo dev-start
```

Environment variables: `VIGIL_ENV=dev` or `--dev` flag

## Key Design Decisions

1. **Shell-out to macOS tools** - Use `ping` and `traceroute` commands rather than raw sockets (simpler, no elevated privileges needed)

2. **SQLite for persistence** - Simple, no server, works for single-user CLI tool

3. **tokio for async** - Concurrent pinging of multiple targets

4. **State machine with hysteresis** - Prevents flapping on single packet loss

5. **Config via TOML** - Standard Rust config format, human-readable

## File Locations

- **Config**: `~/Library/Application Support/ch.kapptec.vigil/config.toml`
- **Database**: `~/Library/Application Support/ch.kapptec.vigil/monitor.db`
- **Logs**: `~/Library/Application Support/ch.kapptec.vigil/monitor.log`

## macOS Commands Used

```bash
# Ping
ping -c 1 -W 2000 <target>

# Traceroute
traceroute -n -q 1 -w 2 <target>

# Gateway detection
route -n get default | grep gateway
```

## Code Conventions

- Use `thiserror` for error types
- Use `tracing` for logging (not `log` crate)
- Use `chrono` for timestamps (UTC internally)
- Use `clap` derive macros for CLI
- Tests in same file as implementation (`#[cfg(test)] mod tests`)

## Testing

```bash
cargo test                      # Run all tests
cargo run -- --dev init         # Initialize dev config/db
cargo run -- --dev start -f     # Start monitoring (dev mode, foreground)
./scripts/qa.sh                 # Full QA: fmt, clippy, test, doc, build
```

## Dependencies

Core:

- `tokio` - Async runtime (with signal handling)
- `clap` - CLI parsing (derive macros)
- `rusqlite` - SQLite database (bundled)
- `serde` / `serde_json` / `toml` - Configuration and serialization
- `chrono` - Timestamps (UTC internally)
- `tracing` / `tracing-subscriber` / `tracing-appender` - Logging with file rotation
- `thiserror` - Error types
- `directories` / `dirs` - Platform-specific config paths

Networking:

- `reqwest` - HTTP connectivity checks (rustls-tls)
- `futures` - Async utilities

Display:

- `tabled` - Table formatting
- `indicatif` - Progress bars

## Database Schema (v3)

Five tables with versioned migrations (v1: initial, v2: enhanced culprit tracking, v3: gateway-first diagnosis):

```sql
-- Outage events (OFFLINE state)
outages(id, start_time, end_time, duration_secs, affected_targets JSON,
        failing_hop, failing_hop_ip, notes)

-- Individual ping/connectivity check results (sampled)
ping_log(id, timestamp, target, target_name, latency_ms, success)

-- Traceroute snapshots linked to outages or degraded events
traceroutes(id, outage_id, degraded_event_id, trace_trigger,
            gateway_reachable, gateway_latency_ms, diagnosis,
            timestamp, target, hops JSON, success)

-- Degraded state transitions (pre-outage, added in v2)
degraded_events(id, start_time, end_time, duration_secs,
                escalated_to_outage_id, affected_targets JSON, notes)

-- Migration tracking
schema_version(version, applied_at, description)
```

## Installation

```bash
# Install the binary
cargo install --path .

# Initialize config and database
vigil init

# Start as a launchd service (auto-starts on login)
vigil service install

# Or run manually in foreground
vigil start --foreground
```

## Developer Guidelines

### PR Workflow

1. Always run `./scripts/qa.sh` before pushing
2. **Test ALL changes manually before merging** - don't just rely on CI passing
3. Test the actual functionality that was changed (e.g., `cargo run -- --dev start --foreground`)
4. Use `/pr` command to create PRs with proper formatting

### Attribution

Never use any reference to Claude, Claude Code, Anthropic, or AI in any:

- Pull request title or description
- Commit message (no Co-Authored-By: Claude lines)
- Issue
- Specification
- Code snippet or comment

All commits should appear as human-authored. Do not include AI attribution.

## User's Environment

- macOS (Darwin)
- Gateway detected at: 10.0.0.1
- Fiber router (connection drops are the problem being diagnosed)
