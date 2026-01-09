# Vigil

**Keep watch over your network.**

Vigil is a Rust CLI tool that monitors home network connectivity and diagnoses intermittent outages by tracking which network hop is failing. When your connection drops, Vigil identifies the culprit—whether it's your local router, ISP modem, or ISP backbone.

```
$ vigil status

Network Status: ONLINE
═══════════════════════════════════════════════════════════

Target           Status    Latency    Last Check
─────────────────────────────────────────────────────────────
8.8.8.8          UP        12.3 ms    2 seconds ago
1.1.1.1          UP        11.8 ms    2 seconds ago
Gateway          UP         1.2 ms    2 seconds ago

Uptime: 99.7% (last 24h) | Outages today: 2
```

## Features

- **Continuous Monitoring** — Pings multiple targets to detect connectivity issues
- **Outage Detection** — State machine with hysteresis prevents false positives from single packet loss
- **Hop Analysis** — Automatic traceroute on failure identifies exactly where packets stop
- **Historical Stats** — SQLite database tracks all outages and ping history
- **macOS Service** — Runs as a launchd service, starts automatically on login
- **CLI Reports** — View outages, statistics, and run manual diagnostics

## Quick Start

```bash
# Install
cargo install --path .

# Initialize config and database
vigil init

# Start monitoring (foreground)
vigil start --foreground

# Or install as a service (recommended)
vigil service install
vigil service start
```

## Usage

```bash
vigil status              # Current connectivity status
vigil outages -p 7d       # Outages in the last 7 days
vigil stats -p 24h        # Statistics for the last 24 hours
vigil trace 8.8.8.8       # Manual traceroute
vigil service status      # Check if service is running
```

## How It Works

1. **Ping Monitor** — Continuously pings configured targets (default: 8.8.8.8, 1.1.1.1, gateway)
2. **State Machine** — Tracks connectivity state: ONLINE → DEGRADED → OFFLINE
3. **Traceroute** — When entering OFFLINE, runs traceroute to identify failing hop
4. **Culprit ID** — The last responding hop indicates where the failure occurs:
   - Hop 1: Your router/gateway (local network issue)
   - Hop 2: ISP modem/CPE (fiber modem issue)
   - Hop 3+: ISP backbone (ISP infrastructure issue)

## Example Outage Report

```
$ vigil outages -p 7d

Recent Outages (last 7d)
═══════════════════════════════════════════════════════════

Start Time           Duration  Culprit
───────────────────────────────────────────────────────────
2024-01-15 14:23:45     1m 32s  Hop 2 10.0.0.1 (ISP Modem)
2024-01-15 16:45:12       45s   Hop 2 10.0.0.1 (ISP Modem)
2024-01-16 09:12:33     2m 15s  Hop 3 72.14.215.85 (ISP Router)
───────────────────────────────────────────────────────────

Summary: 3 outages, 4m 32s total downtime
Most common culprit: Hop 2 - ISP Modem (2 occurrences)
```

## Configuration

Config file: `~/Library/Application Support/ch.kapptec.vigil/config.toml`

```toml
[monitor]
interval = 5                    # Seconds between pings
timeout = 2000                  # Ping timeout in ms
degraded_threshold = 3          # Failures to enter DEGRADED
offline_threshold = 5           # Failures to enter OFFLINE
recovery_threshold = 2          # Successes to recover

[[targets]]
address = "8.8.8.8"
name = "Google DNS"

[[targets]]
address = "1.1.1.1"
name = "Cloudflare DNS"

[[targets]]
address = "gateway"             # Auto-detected
name = "Gateway"
```

## File Locations

| File | Path |
|------|------|
| Config | `~/Library/Application Support/ch.kapptec.vigil/config.toml` |
| Database | `~/Library/Application Support/ch.kapptec.vigil/monitor.db` |
| Logs | `~/Library/Application Support/ch.kapptec.vigil/monitor.log` |

## Requirements

- macOS (uses native `ping` and `traceroute` commands)
- Rust 1.70+

## Development

```bash
# Run in development mode (isolated database)
cargo run -- --dev start --foreground

# Run tests
cargo test

# Build release
cargo build --release
```

## License

MIT

---

*Vigil — Because your ISP won't tell you when it's their fault.*
