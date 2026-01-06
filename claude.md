# Claude Context - Network Monitor

This file provides context for Claude Code sessions working on this project.

## Project Overview

**networkmonitor** - A Rust CLI tool to monitor home network connectivity and diagnose intermittent outages by tracking which network hop is failing.

**Problem**: User has a Zyxel fiber router with connection drops 1-60 seconds, multiple times daily.

**Solution**: Continuous ping monitoring with traceroute-based hop analysis to identify the culprit (local router, ISP equipment, or ISP backbone).

## Current State

| Feature | Status | File(s) |
|---------|--------|---------|
| 001 Core Infrastructure | Done | `config.rs`, `db.rs`, `models.rs`, `lib.rs`, `main.rs` |
| 002 Ping Monitor | Pending | `src/monitor/ping.rs` |
| 003 Outage Detection | Pending | `src/monitor/state.rs` |
| 004 Hop Analysis | Pending | `src/monitor/traceroute.rs` |
| 005 CLI Reporting | Pending | `src/cli/*.rs` |
| 006 Polish & Service | Pending | `src/cli/service.rs` (new) |

## Implementation Order

Follow feature specs sequentially: `docs/features/001-*.md` through `006-*.md`

Current: **Start Feature 002 (Ping Monitor)**

## Key Design Decisions

1. **Shell-out to macOS tools** - Use `ping` and `traceroute` commands rather than raw sockets (simpler, no elevated privileges needed)

2. **SQLite for persistence** - Simple, no server, works for single-user CLI tool

3. **tokio for async** - Concurrent pinging of multiple targets

4. **State machine with hysteresis** - Prevents flapping on single packet loss

5. **Config via TOML** - Standard Rust config format, human-readable

## File Locations

- **Config**: `~/Library/Application Support/com.kapptec.networkmonitor/config.toml`
- **Database**: `~/Library/Application Support/com.kapptec.networkmonitor/monitor.db`
- **Logs**: `~/Library/Application Support/com.kapptec.networkmonitor/monitor.log`

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
cargo test           # Run all tests
cargo run -- init    # Initialize config/db
cargo run -- start   # Start monitoring (placeholder until 002)
```

## Dependencies

Core:
- `tokio` - Async runtime
- `clap` - CLI parsing
- `rusqlite` - SQLite database
- `serde` / `toml` - Configuration
- `chrono` - Timestamps
- `tracing` - Logging

Display:
- `tabled` - Table formatting
- `indicatif` - Progress bars

## Database Schema

```sql
outages(id, start_time, end_time, duration_secs, affected_targets, failing_hop, failing_hop_ip, notes)
ping_log(id, timestamp, target, target_name, latency_ms, success)
traceroutes(id, outage_id, timestamp, target, hops, success)
```

## Next Steps

When resuming work:
1. Read `docs/features/002-ping-monitor.md` for spec
2. Implement `src/monitor/ping.rs`
3. Update `src/monitor/mod.rs` to export
4. Add tests
5. Mark 002 as Done in docs

## User's Environment

- macOS (Darwin)
- Gateway detected at: 10.0.0.1
- Zyxel fiber router (connection drops are the problem being diagnosed)
