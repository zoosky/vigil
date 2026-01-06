# 005 - CLI & Reporting

**Status:** Pending

## Overview

Implement the CLI commands for viewing status, outage history, and statistics.

## Objectives

- Status command showing current connectivity
- Outages command listing historical outages
- Stats command showing aggregated statistics
- Formatted tables and progress indicators

## Commands

### `networkmonitor status`

```
Network Monitor Status
═══════════════════════════════════════════════════════════

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

### `networkmonitor outages`

```
Recent Outages (last 24 hours)
═══════════════════════════════════════════════════════════

┌─────────────────────┬──────────┬─────────────┬───────────────────┐
│ Start Time          │ Duration │ Failing Hop │ Affected Targets  │
├─────────────────────┼──────────┼─────────────┼───────────────────┤
│ 2024-01-15 14:23:05 │ 12s      │ 3 (ISP)     │ 8.8.8.8, 1.1.1.1  │
│ 2024-01-15 09:45:32 │ 45s      │ 2 (Zyxel)   │ All targets       │
│ 2024-01-15 03:12:18 │ 8s       │ 3 (ISP)     │ 8.8.8.8, 1.1.1.1  │
└─────────────────────┴──────────┴─────────────┴───────────────────┘

Summary: 3 outages, 1m 5s total downtime
Most common failing hop: Hop 3 (ISP gateway) - 2 occurrences
```

### `networkmonitor stats`

```
Statistics (last 24 hours)
═══════════════════════════════════════════════════════════

Period: 2024-01-14 15:00 → 2024-01-15 15:00

Availability:
  ████████████████████████████████████████░░  99.88%

Outages:
  Total: 3
  Total downtime: 1m 5s
  Average duration: 21.7s
  Longest: 45s

Failing Hop Analysis:
  Hop 2 (Zyxel Router): 1 outage (45s total)
  Hop 3 (ISP):          2 outages (20s total)

Time Distribution:
  00:00-06:00  ░░░░░░░░░░░░  1 outage
  06:00-12:00  ████░░░░░░░░  1 outage
  12:00-18:00  ████░░░░░░░░  1 outage
  18:00-24:00  ░░░░░░░░░░░░  0 outages
```

## Implementation

### File: `src/cli/status.rs`

```rust
pub async fn run(app: &App) -> Result<()> {
    // Get current state from monitoring (if running)
    // or check connectivity now

    // Get today's stats from database
    let since = Utc::now() - Duration::hours(24);
    let stats = app.db.get_stats(since, Utc::now())?;

    // Ping each target once for current latency
    let latencies = ping_all_targets(&app.config).await;

    // Display formatted output
    println!("Network Monitor Status");
    println!("═══════════════════════════════════════════════════════════\n");
    // ...
}
```

### File: `src/cli/outages.rs`

```rust
pub fn run(app: &App, last: &str) -> Result<()> {
    let duration = parse_duration(last)?; // "24h" → Duration
    let since = Utc::now() - duration;

    let outages = app.db.get_outages(since, Utc::now())?;

    // Build table using tabled crate
    let table = Table::new(outages.iter().map(OutageRow::from));
    println!("{}", table);

    // Summary
    print_summary(&outages);
}
```

### File: `src/cli/stats.rs`

```rust
pub fn run(app: &App, period: &str) -> Result<()> {
    let duration = parse_duration(period)?;
    let since = Utc::now() - duration;

    let stats = app.db.get_stats(since, Utc::now())?;
    let outages = app.db.get_outages(since, Utc::now())?;

    // Display availability bar
    print_availability_bar(stats.availability_percent);

    // Display outage statistics
    print_outage_stats(&stats);

    // Analyze failing hops
    print_hop_analysis(&outages);

    // Time distribution
    print_time_distribution(&outages);
}
```

## Tasks

- [ ] Implement `parse_duration()` for "24h", "7d" format
- [ ] Implement status command
- [ ] Implement outages command with table formatting
- [ ] Implement stats command with visualizations
- [ ] Add color coding (green/yellow/red for states)
- [ ] Handle empty results gracefully
- [ ] Add export functionality (CSV/JSON)

## Duration Parsing

```rust
fn parse_duration(s: &str) -> Result<Duration> {
    let len = s.len();
    let (num, unit) = s.split_at(len - 1);
    let num: i64 = num.parse()?;

    match unit {
        "h" => Ok(Duration::hours(num)),
        "d" => Ok(Duration::days(num)),
        "w" => Ok(Duration::weeks(num)),
        _ => Err(anyhow!("Invalid duration format"))
    }
}
```

## Table Formatting

Using `tabled` crate:

```rust
use tabled::{Table, Tabled};

#[derive(Tabled)]
struct OutageRow {
    #[tabled(rename = "Start Time")]
    start_time: String,
    #[tabled(rename = "Duration")]
    duration: String,
    #[tabled(rename = "Failing Hop")]
    failing_hop: String,
    #[tabled(rename = "Affected Targets")]
    affected_targets: String,
}
```

## Progress/Availability Bar

Using `indicatif` crate:

```rust
fn print_availability_bar(percent: f64) {
    let filled = (percent / 100.0 * 40.0) as usize;
    let empty = 40 - filled;

    println!(
        "  {}{}  {:.2}%",
        "█".repeat(filled),
        "░".repeat(empty),
        percent
    );
}
```

## Test Plan

### Unit Tests

```rust
#[test]
fn test_parse_duration() {
    assert_eq!(parse_duration("24h").unwrap(), Duration::hours(24));
    assert_eq!(parse_duration("7d").unwrap(), Duration::days(7));
}

#[test]
fn test_format_duration() {
    assert_eq!(format_duration(65.0), "1m 5s");
    assert_eq!(format_duration(3665.0), "1h 1m 5s");
}
```

## Acceptance Criteria

1. Status shows current state and target latencies
2. Outages displays formatted table with all columns
3. Stats shows availability percentage with visual bar
4. Duration parsing handles hours, days, weeks
5. Empty states handled gracefully ("No outages recorded")
6. Output is readable in standard terminal width (80 chars)
