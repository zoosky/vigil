# 018 - Network Diagnostics and Pattern Analysis

**Status:** Pending

## Problem Statement

Vigil currently detects and records outages with culprit identification (which hop is failing). However, when users experience recurring issues, they lack the diagnostic data needed to:

1. Identify **patterns** - Is it time-of-day related? Certain days of the week?
2. Understand **trends** - Are outages increasing? Getting longer?
3. Provide **evidence to ISP** - Concrete data showing the problem is on their side
4. Compare **before/after** - Did ISP maintenance actually fix the issue?

Real-world example: User experiences outages at Hop 2 (ISP modem/OLT) multiple times daily. BGP data shows the ISP backbone is healthy, suggesting the issue is in the access network layer. The user needs pattern analysis to correlate outages with potential causes (peak usage times, weather, ISP maintenance windows).

### User Impact

Without diagnostics:
- User can only see individual outages, not the bigger picture
- ISP support dismisses complaints without concrete pattern evidence
- No way to verify if ISP fixes actually improved stability

## Solution

Add a `vigil diagnostics` command that analyzes historical outage data and produces actionable insights.

### Key Features

1. **Pattern Analysis**: Time-of-day and day-of-week distribution
2. **Trend Analysis**: Outage frequency and duration over time periods
3. **Culprit Summary**: Which hops fail most often with IPs
4. **Stability Score**: Single metric summarizing network health
5. **Export Format**: Machine-readable output for ISP support tickets

## Implementation

### CLI Commands

#### `vigil diagnostics`

Main diagnostics command with subcommands:

```bash
# Full diagnostic report (default: last 7 days)
vigil diagnostics

# Specify time range
vigil diagnostics --last 30d
vigil diagnostics --since 2026-01-01

# Specific analysis
vigil diagnostics patterns     # Time patterns only
vigil diagnostics trends       # Trend analysis only
vigil diagnostics culprits     # Culprit breakdown only

# Export for ISP support
vigil diagnostics --export json > network-report.json
vigil diagnostics --export csv > outages.csv
```

### File: `src/cli/diagnostics.rs`

```rust
use crate::db::Database;
use crate::models::Outage;
use chrono::{DateTime, Datelike, Timelike, Utc};
use std::collections::HashMap;

/// Diagnostic analysis results
pub struct DiagnosticReport {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_outages: usize,
    pub total_downtime_secs: f64,
    pub stability_score: f64,
    pub patterns: TimePatterns,
    pub trends: TrendAnalysis,
    pub culprits: CulpritAnalysis,
}

/// Time-based pattern analysis
pub struct TimePatterns {
    /// Outages per hour of day (0-23)
    pub hourly_distribution: [u32; 24],
    /// Outages per day of week (0=Mon, 6=Sun)
    pub daily_distribution: [u32; 7],
    /// Peak outage hours (sorted by frequency)
    pub peak_hours: Vec<(u8, u32)>,
    /// Average outage duration by hour
    pub avg_duration_by_hour: [f64; 24],
}

/// Trend analysis over time
pub struct TrendAnalysis {
    /// Outages per day for the period
    pub daily_counts: Vec<(String, u32)>,  // (date, count)
    /// Rolling 7-day average
    pub weekly_moving_avg: Vec<f64>,
    /// Trend direction: -1 (improving), 0 (stable), 1 (worsening)
    pub trend_direction: i8,
    /// Percentage change vs previous period
    pub change_vs_previous: Option<f64>,
}

/// Culprit analysis with hop details
pub struct CulpritAnalysis {
    /// Breakdown by hop number
    pub by_hop: HashMap<u8, HopStats>,
    /// Most common failing hop
    pub primary_culprit: Option<(u8, String, u32)>,  // (hop, interpretation, count)
    /// IPs seen at each hop
    pub hop_ips: HashMap<u8, Vec<String>>,
}

pub struct HopStats {
    pub count: u32,
    pub total_duration_secs: f64,
    pub avg_duration_secs: f64,
    pub last_occurrence: DateTime<Utc>,
    pub ips_seen: Vec<String>,
}

impl DiagnosticReport {
    /// Calculate stability score (0-100, higher is better)
    pub fn calculate_stability_score(
        total_secs_in_period: f64,
        downtime_secs: f64,
        outage_count: usize,
    ) -> f64 {
        // Uptime percentage (0-100)
        let uptime_pct = ((total_secs_in_period - downtime_secs) / total_secs_in_period) * 100.0;

        // Penalty for frequency (many short outages worse than few long ones for user experience)
        let frequency_penalty = (outage_count as f64 * 0.5).min(20.0);

        (uptime_pct - frequency_penalty).max(0.0).min(100.0)
    }
}
```

### Pattern Analysis Logic

```rust
impl TimePatterns {
    pub fn analyze(outages: &[Outage]) -> Self {
        let mut hourly = [0u32; 24];
        let mut daily = [0u32; 7];
        let mut duration_sum_by_hour = [0.0f64; 24];
        let mut count_by_hour = [0u32; 24];

        for outage in outages {
            let hour = outage.start_time.hour() as usize;
            let weekday = outage.start_time.weekday().num_days_from_monday() as usize;

            hourly[hour] += 1;
            daily[weekday] += 1;

            if let Some(duration) = outage.duration_secs {
                duration_sum_by_hour[hour] += duration;
                count_by_hour[hour] += 1;
            }
        }

        // Calculate average duration by hour
        let mut avg_duration_by_hour = [0.0f64; 24];
        for i in 0..24 {
            if count_by_hour[i] > 0 {
                avg_duration_by_hour[i] = duration_sum_by_hour[i] / count_by_hour[i] as f64;
            }
        }

        // Find peak hours
        let mut peak_hours: Vec<(u8, u32)> = hourly
            .iter()
            .enumerate()
            .map(|(h, &c)| (h as u8, c))
            .filter(|(_, c)| *c > 0)
            .collect();
        peak_hours.sort_by(|a, b| b.1.cmp(&a.1));

        Self {
            hourly_distribution: hourly,
            daily_distribution: daily,
            peak_hours,
            avg_duration_by_hour,
        }
    }
}
```

### Display Format

```rust
pub fn print_diagnostic_report(report: &DiagnosticReport) {
    println!("Network Diagnostics Report");
    println!("═══════════════════════════════════════════════════════════\n");

    // Summary
    println!("Period: {} to {}",
        report.period_start.format("%Y-%m-%d"),
        report.period_end.format("%Y-%m-%d"));
    println!("Stability Score: {:.1}/100", report.stability_score);
    println!("Total Outages: {}", report.total_outages);
    println!("Total Downtime: {}\n", format_duration(report.total_downtime_secs));

    // Time Patterns
    println!("Time Patterns");
    println!("─────────────────────────────────────────────────────────────");
    print_hourly_histogram(&report.patterns.hourly_distribution);
    println!("\nPeak Hours:");
    for (hour, count) in report.patterns.peak_hours.iter().take(3) {
        println!("  {:02}:00-{:02}:59: {} outages", hour, hour, count);
    }

    // Day of week
    println!("\nDay of Week:");
    let days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    for (i, &count) in report.patterns.daily_distribution.iter().enumerate() {
        if count > 0 {
            println!("  {}: {} outages", days[i], count);
        }
    }

    // Culprit Analysis
    println!("\nCulprit Analysis");
    println!("─────────────────────────────────────────────────────────────");
    if let Some((hop, interp, count)) = &report.culprits.primary_culprit {
        println!("Primary Culprit: Hop {} ({}) - {} occurrences", hop, interp, count);
    }

    for (hop, stats) in &report.culprits.by_hop {
        println!("\n  Hop {}:", hop);
        println!("    Occurrences: {}", stats.count);
        println!("    Avg Duration: {:.1}s", stats.avg_duration_secs);
        if !stats.ips_seen.is_empty() {
            println!("    IPs Seen: {}", stats.ips_seen.join(", "));
        }
    }

    // Trend
    println!("\nTrend Analysis");
    println!("─────────────────────────────────────────────────────────────");
    let trend_str = match report.trends.trend_direction {
        -1 => "📈 Improving",
        0 => "➡️  Stable",
        1 => "📉 Worsening",
        _ => "Unknown",
    };
    println!("Trend: {}", trend_str);
    if let Some(change) = report.trends.change_vs_previous {
        println!("Change vs Previous Period: {:+.1}%", change);
    }
}

fn print_hourly_histogram(hourly: &[u32; 24]) {
    let max = *hourly.iter().max().unwrap_or(&1).max(&1);
    println!("\nHourly Distribution:");
    for (hour, &count) in hourly.iter().enumerate() {
        let bar_len = if max > 0 { (count * 40 / max) as usize } else { 0 };
        let bar = "█".repeat(bar_len);
        if count > 0 {
            println!("  {:02}:00 │{:<40} {}", hour, bar, count);
        }
    }
}
```

### Export Formats

#### JSON Export

```rust
#[derive(Serialize)]
pub struct DiagnosticExport {
    pub generated_at: DateTime<Utc>,
    pub period: PeriodInfo,
    pub summary: SummaryInfo,
    pub patterns: PatternsInfo,
    pub culprits: Vec<CulpritInfo>,
    pub outages: Vec<OutageExport>,
}

pub fn export_json(report: &DiagnosticReport, outages: &[Outage]) -> String {
    let export = DiagnosticExport {
        generated_at: Utc::now(),
        period: PeriodInfo {
            start: report.period_start,
            end: report.period_end,
            days: (report.period_end - report.period_start).num_days(),
        },
        summary: SummaryInfo {
            stability_score: report.stability_score,
            total_outages: report.total_outages,
            total_downtime_secs: report.total_downtime_secs,
            uptime_percentage: calculate_uptime(report),
        },
        // ... rest of export
    };
    serde_json::to_string_pretty(&export).unwrap()
}
```

#### CSV Export

```bash
$ vigil diagnostics --export csv
timestamp,duration_secs,culprit_hop,culprit_ip,affected_targets
2026-01-18T06:37:31Z,1.0,2,10.0.0.1,"8.8.8.8,8.8.4.4,10.0.0.1"
2026-01-18T06:28:13Z,10.0,2,10.0.0.1,"8.8.8.8,8.8.4.4,10.0.0.1"
...
```

### Database Queries

```rust
impl Database {
    /// Get outages for diagnostic analysis
    pub fn get_outages_for_diagnostics(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Outage>> {
        let sql = r#"
            SELECT id, start_time, end_time, duration_secs,
                   affected_targets, failing_hop, failing_hop_ip, notes
            FROM outages
            WHERE start_time >= ?1 AND start_time <= ?2
            ORDER BY start_time ASC
        "#;
        // ...
    }

    /// Get hourly outage counts for histogram
    pub fn get_hourly_distribution(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<[u32; 24]> {
        let sql = r#"
            SELECT strftime('%H', start_time) as hour, COUNT(*) as count
            FROM outages
            WHERE start_time >= ?1 AND start_time <= ?2
            GROUP BY hour
        "#;
        // ...
    }
}
```

## Example Output

```
$ vigil diagnostics --last 7d

Network Diagnostics Report
═══════════════════════════════════════════════════════════

Period: 2026-01-11 to 2026-01-18
Stability Score: 94.2/100
Total Outages: 23
Total Downtime: 4m 37s

Time Patterns
─────────────────────────────────────────────────────────────

Hourly Distribution:
  06:00 │████████████████████████████████████████ 8
  07:00 │██████████████████████████               5
  19:00 │██████████████████████████████████       7
  21:00 │███████████████                          3

Peak Hours:
  06:00-06:59: 8 outages
  19:00-19:59: 7 outages
  07:00-07:59: 5 outages

Day of Week:
  Mon: 4 outages
  Wed: 6 outages
  Fri: 5 outages
  Sat: 8 outages

Culprit Analysis
─────────────────────────────────────────────────────────────
Primary Culprit: Hop 2 (ISP Modem) - 21 occurrences

  Hop 1:
    Occurrences: 2
    Avg Duration: 2.3s
    IPs Seen: 10.0.0.1

  Hop 2:
    Occurrences: 21
    Avg Duration: 11.8s
    IPs Seen: 10.0.0.1 (gateway), 81.6.38.129

Trend Analysis
─────────────────────────────────────────────────────────────
Trend: 📉 Worsening
Change vs Previous Period: +35.3%

Recommendation: 91% of outages occur at Hop 2 (ISP equipment).
Consider contacting ISP with this data. Peak times (06:00, 19:00)
suggest possible congestion or scheduled maintenance windows.
```

## Configuration

No new configuration required. Uses existing database.

## Test Plan

### Unit Tests

```rust
#[test]
fn test_stability_score_calculation() {
    // Perfect uptime
    let score = DiagnosticReport::calculate_stability_score(
        86400.0,  // 1 day in seconds
        0.0,      // no downtime
        0,        // no outages
    );
    assert_eq!(score, 100.0);

    // Some downtime
    let score = DiagnosticReport::calculate_stability_score(
        86400.0,
        864.0,    // 1% downtime
        10,       // 10 outages
    );
    assert!(score > 90.0 && score < 100.0);
}

#[test]
fn test_hourly_pattern_analysis() {
    let outages = vec![
        mock_outage("2026-01-18T06:30:00Z"),
        mock_outage("2026-01-18T06:45:00Z"),
        mock_outage("2026-01-18T19:00:00Z"),
    ];

    let patterns = TimePatterns::analyze(&outages);
    assert_eq!(patterns.hourly_distribution[6], 2);
    assert_eq!(patterns.hourly_distribution[19], 1);
    assert_eq!(patterns.peak_hours[0], (6, 2));
}

#[test]
fn test_trend_direction() {
    // More outages in recent period = worsening
    let trends = TrendAnalysis::analyze(/*...*/);
    assert_eq!(trends.trend_direction, 1);
}
```

### Integration Tests

```rust
#[test]
fn test_diagnostics_command() {
    let output = Command::new("vigil")
        .args(["diagnostics", "--last", "1d"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Stability Score:"));
    assert!(stdout.contains("Time Patterns"));
}
```

## Acceptance Criteria

1. `vigil diagnostics` shows comprehensive report with patterns, trends, and culprits
2. Time range can be specified with `--last` or `--since`
3. Export to JSON and CSV formats works correctly
4. Stability score accurately reflects network health (0-100)
5. Hourly and daily patterns are correctly calculated
6. Trend analysis compares to previous period
7. Culprit analysis shows hop breakdown with IPs
8. Output is formatted for easy reading and ISP communication

## Future Enhancements

- **Anomaly Detection**: Alert when outage patterns change significantly
- **Correlation with External Data**: Weather, ISP status pages
- **Comparative Reports**: Compare different time periods side-by-side
- **Automated ISP Report Generation**: PDF export formatted for support tickets

## Dependencies

- `serde_json` - JSON export (already in use)
- `csv` - CSV export (new, optional)
