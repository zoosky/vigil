# 008 - Latency Quality Metrics

**Status:** Pending

## Overview

Extend the ping monitoring to capture comprehensive quality metrics beyond simple success/failure, including jitter, packet loss percentage, latency percentiles, and trend detection. This enables identification of network degradation before full outages occur.

## Objectives

- Calculate jitter (latency variation) per target
- Track packet loss percentage over rolling windows
- Compute latency percentiles (p50, p95, p99)
- Detect latency anomalies and trends
- Store aggregated quality metrics for historical analysis
- Alert on quality degradation thresholds

## Key Metrics

### 1. Jitter (Latency Variation)

Jitter measures the inconsistency of latency, critical for real-time applications (VoIP, gaming, video calls).

```
Jitter = Average of |latency[n] - latency[n-1]|
```

**Quality Thresholds:**
- Excellent: < 5ms
- Good: 5-15ms
- Fair: 15-30ms
- Poor: > 30ms

### 2. Packet Loss Percentage

Rolling window packet loss calculation:

```
Loss % = (failed_pings / total_pings) × 100
```

**Quality Thresholds:**
- Excellent: 0%
- Good: < 1%
- Fair: 1-2.5%
- Poor: > 2.5%

### 3. Latency Percentiles

More informative than averages for understanding tail latency:

- **p50 (median)**: Typical user experience
- **p95**: Experience during congestion
- **p99**: Worst-case (excluding outliers)

### 4. Mean Opinion Score (MOS) Estimate

Derived metric estimating voice call quality (1-5 scale):

```rust
fn calculate_mos(latency_ms: f64, jitter_ms: f64, loss_percent: f64) -> f64 {
    // Simplified E-model calculation
    let effective_latency = latency_ms + jitter_ms * 2.0 + 10.0;
    let r_factor = 93.2 - (effective_latency / 40.0) - (loss_percent * 2.5);

    // Convert R-factor to MOS
    if r_factor < 0.0 {
        1.0
    } else if r_factor > 100.0 {
        4.5
    } else {
        1.0 + 0.035 * r_factor + r_factor * (r_factor - 60.0) * (100.0 - r_factor) * 7e-6
    }
}
```

## Implementation

### File: `src/monitor/quality.rs`

```rust
pub struct QualityMetrics {
    target: String,
    window_size: usize,
    latencies: VecDeque<f64>,
    successes: VecDeque<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QualitySnapshot {
    pub target: String,
    pub timestamp: DateTime<Utc>,

    // Latency metrics
    pub latency_avg_ms: f64,
    pub latency_min_ms: f64,
    pub latency_max_ms: f64,
    pub latency_p50_ms: f64,
    pub latency_p95_ms: f64,
    pub latency_p99_ms: f64,

    // Quality metrics
    pub jitter_ms: f64,
    pub packet_loss_percent: f64,
    pub mos_score: f64,

    // Classification
    pub quality_grade: QualityGrade,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum QualityGrade {
    Excellent,
    Good,
    Fair,
    Poor,
}

impl QualityMetrics {
    pub fn new(target: String, window_size: usize) -> Self;

    /// Record a new ping result
    pub fn record(&mut self, result: &PingResult);

    /// Calculate current quality snapshot
    pub fn snapshot(&self) -> QualitySnapshot;

    /// Check if quality has degraded below threshold
    pub fn is_degraded(&self, config: &QualityConfig) -> bool;
}
```

### Jitter Calculation

```rust
fn calculate_jitter(latencies: &[f64]) -> f64 {
    if latencies.len() < 2 {
        return 0.0;
    }

    let variations: Vec<f64> = latencies
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .collect();

    variations.iter().sum::<f64>() / variations.len() as f64
}
```

### Percentile Calculation

```rust
fn percentile(sorted_values: &[f64], p: f64) -> f64 {
    if sorted_values.is_empty() {
        return 0.0;
    }

    let index = (p / 100.0 * (sorted_values.len() - 1) as f64).round() as usize;
    sorted_values[index.min(sorted_values.len() - 1)]
}
```

## Database Schema Extension

```sql
CREATE TABLE quality_metrics (
    id INTEGER PRIMARY KEY,
    timestamp TEXT NOT NULL,
    target TEXT NOT NULL,

    -- Latency
    latency_avg_ms REAL,
    latency_min_ms REAL,
    latency_max_ms REAL,
    latency_p50_ms REAL,
    latency_p95_ms REAL,
    latency_p99_ms REAL,

    -- Quality
    jitter_ms REAL,
    packet_loss_percent REAL,
    mos_score REAL,
    quality_grade TEXT
);

CREATE INDEX idx_quality_timestamp ON quality_metrics(timestamp);
CREATE INDEX idx_quality_target ON quality_metrics(target);
```

## Configuration

```toml
[quality]
enabled = true
window_size = 60           # Samples for rolling calculations
snapshot_interval_secs = 60 # How often to store snapshots

[quality.thresholds]
# Alert when quality drops below these
jitter_warning_ms = 20
jitter_critical_ms = 50
loss_warning_percent = 1.0
loss_critical_percent = 3.0
latency_warning_ms = 100
latency_critical_ms = 200
```

## CLI Integration

### `networkmonitor quality`

```
Network Quality Report
═══════════════════════════════════════════════════════════

Target: Gateway (192.168.1.1)
  Grade: EXCELLENT
  Latency: 2.1ms avg (p95: 3.2ms, p99: 4.1ms)
  Jitter: 0.8ms
  Packet Loss: 0.0%
  MOS Score: 4.4/5.0

Target: Google DNS (8.8.8.8)
  Grade: GOOD
  Latency: 18.3ms avg (p95: 24.1ms, p99: 31.2ms)
  Jitter: 3.2ms
  Packet Loss: 0.2%
  MOS Score: 4.2/5.0

Target: Cloudflare (1.1.1.1)
  Grade: FAIR
  Latency: 15.1ms avg (p95: 45.2ms, p99: 89.3ms)
  Jitter: 12.4ms
  Packet Loss: 1.5%
  MOS Score: 3.6/5.0
  ⚠ High jitter detected

Overall Network Grade: GOOD
```

### `networkmonitor stats` Enhancement

Add quality metrics to existing stats output:

```
Quality Trends (last 24 hours)
───────────────────────────────
Average Jitter: 4.2ms
Peak Jitter: 28.3ms (at 14:23)
Average Packet Loss: 0.3%
Latency Trend: ↗ +2.1ms (slight increase)
```

## Integration with Notifications (Feature 007)

Quality degradation can trigger notifications:

```rust
if quality.is_degraded(&config) {
    notifier.notify(&NotificationEvent::QualityDegraded {
        target: target.clone(),
        quality: quality.snapshot(),
    }).await?;
}
```

## Tasks

- [ ] Define `QualityMetrics` struct with rolling window
- [ ] Implement jitter calculation
- [ ] Implement percentile calculations
- [ ] Implement MOS score estimation
- [ ] Add database table for quality metrics
- [ ] Implement periodic snapshot storage
- [ ] Add `networkmonitor quality` CLI command
- [ ] Enhance stats command with quality trends
- [ ] Add quality degradation to notifications
- [ ] Add unit tests for all calculations
- [ ] Add configuration for thresholds

## Test Plan

### Unit Tests

```rust
#[test]
fn test_jitter_calculation() {
    let latencies = vec![10.0, 12.0, 11.0, 15.0, 13.0];
    let jitter = calculate_jitter(&latencies);
    // Expected: (2 + 1 + 4 + 2) / 4 = 2.25
    assert!((jitter - 2.25).abs() < 0.01);
}

#[test]
fn test_percentile_calculation() {
    let values: Vec<f64> = (1..=100).map(|x| x as f64).collect();
    assert_eq!(percentile(&values, 50.0), 50.0);
    assert_eq!(percentile(&values, 95.0), 95.0);
}

#[test]
fn test_mos_calculation() {
    // Perfect conditions
    let mos = calculate_mos(20.0, 1.0, 0.0);
    assert!(mos > 4.0);

    // Poor conditions
    let mos = calculate_mos(200.0, 50.0, 5.0);
    assert!(mos < 3.0);
}

#[test]
fn test_quality_grade() {
    let snapshot = QualitySnapshot {
        jitter_ms: 3.0,
        packet_loss_percent: 0.0,
        latency_avg_ms: 15.0,
        ..Default::default()
    };
    assert_eq!(snapshot.quality_grade, QualityGrade::Excellent);
}
```

## Acceptance Criteria

1. Jitter calculated correctly from rolling window of pings
2. Packet loss percentage tracked accurately
3. Latency percentiles (p50, p95, p99) computed correctly
4. MOS score provides meaningful quality indication
5. Quality metrics stored periodically to database
6. CLI displays quality report with grades
7. Quality degradation triggers notifications (if enabled)
8. Historical quality trends visible in stats command
