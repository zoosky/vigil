# Usage Guide

## Installation

```bash
# Build from source
cargo build --release

# Binary will be at target/release/vigil
# Optionally copy to PATH
cp target/release/vigil /usr/local/bin/
```

## Initial Setup

```bash
# Initialize configuration and database
vigil init
```

This will:

1. Create config file at `~/Library/Application Support/ch.kapptec.vigil/config.toml`
2. Create SQLite database at `~/Library/Application Support/ch.kapptec.vigil/monitor.db`
3. Auto-detect your gateway IP
4. Show default monitoring targets

## Configuration

### View Current Config

```bash
vigil config show
```

### Config File Location

```bash
vigil config path
# Output: /Users/you/Library/Application Support/ch.kapptec.vigil/config.toml
```

### Edit Configuration

Edit the TOML file directly:

```toml
[monitor]
ping_interval_ms = 1000      # How often to ping (ms)
ping_timeout_ms = 2000       # Ping timeout (ms)
degraded_threshold = 3       # Failures before DEGRADED state
offline_threshold = 5        # Failures before OFFLINE state
recovery_threshold = 2       # Successes to recover

[targets]
gateway = "192.168.1.1"      # Your router IP (auto-detected if omitted)

[[targets.targets]]
name = "Google DNS"
ip = "8.8.8.8"

[[targets.targets]]
name = "Cloudflare"
ip = "1.1.1.1"

[[targets.targets]]
name = "Quad9"               # Add custom targets
ip = "9.9.9.9"

[database]
retention_days = 90          # How long to keep data

[logging]
level = "info"               # trace, debug, info, warn, error
```

## Commands

### Start Monitoring

```bash
# Run in foreground
vigil start --foreground

# Run as daemon (background)
vigil start
```

Press `Ctrl+C` to stop when running in foreground.

### Check Status

```bash
vigil status
```

Shows:

- Current connectivity state (ONLINE/DEGRADED/OFFLINE)
- Time since last outage
- Current latency to each target
- Today's statistics

### View Outages

```bash
# Last 24 hours (default)
vigil outages

# Last 7 days
vigil outages --last 7d

# Last 30 days
vigil outages --last 30d
```

### View Statistics

```bash
# Last 24 hours
vigil stats

# Last week
vigil stats --period 7d
```

Shows:

- Total outages
- Total downtime
- Availability percentage
- Average outage duration
- Most common failing hop

### Manual Traceroute

```bash
# Default target (8.8.8.8)
vigil trace

# Custom target
vigil trace 1.1.1.1
```

## Understanding Output

### Connectivity States

| State | Meaning |
|-------|---------|
| ONLINE | All targets reachable |
| DEGRADED | Some failures detected, monitoring closely |
| OFFLINE | Connectivity lost, traceroute triggered |

### Failing Hop Identification

When an outage occurs, the tool runs traceroute to identify where packets are being dropped:

| Hop | Typical Device | If This Hop Fails |
|-----|----------------|-------------------|
| 1 | Your router | Local network issue |
| 2 | ISP first hop | Fiber modem or ISP CPE issue |
| 3+ | ISP backbone | ISP infrastructure issue |

### Outage Table Columns

```
┌─────────────────────┬──────────┬─────────────┬───────────────────┐
│ Start Time          │ Duration │ Failing Hop │ Affected Targets  │
├─────────────────────┼──────────┼─────────────┼───────────────────┤
│ 2024-01-15 14:23:05 │ 12s      │ 3 (ISP)     │ 8.8.8.8, 1.1.1.1  │
└─────────────────────┴──────────┴─────────────┴───────────────────┘
```

- **Start Time**: When the outage began
- **Duration**: How long it lasted
- **Failing Hop**: Network hop where packets were dropped
- **Affected Targets**: Which monitored targets were unreachable

## Running as a Service (macOS)

To run automatically at login, create a launchd plist:

```bash
# Create the plist (instructions in 006-polish-service.md)
```

## Troubleshooting

### "Permission denied" on ping

macOS ping should work without elevated privileges. If issues occur:

```bash
# Check ping works directly
ping -c 1 8.8.8.8
```

### Database locked

Only one instance should run at a time. Check for existing processes:

```bash
pgrep vigil
```

### High latency reported

Latency spikes during outages are normal. Persistent high latency when online may indicate:

- Network congestion
- WiFi interference
- Overloaded router

### Log file location

Logs are written to:

```
~/Library/Application Support/ch.kapptec.vigil/monitor.log
```

View with:

```bash
tail -f "~/Library/Application Support/ch.kapptec.vigil/monitor.log"
```
