# 006 - Polish & Service

**Status:** Pending

## Overview

Final polish including launchd service integration, graceful shutdown, log rotation, and performance optimization.

## Objectives

- macOS launchd service for automatic startup
- Graceful shutdown with Ctrl+C handling
- Log rotation to prevent disk fill
- Memory optimization for long-running daemon
- Database cleanup based on retention policy

## launchd Service

### Plist File

Create `~/Library/LaunchAgents/ch.kapptec.vigil.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>ch.kapptec.vigil</string>

    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/vigil</string>
        <string>start</string>
        <string>--foreground</string>
    </array>

    <key>RunAtLoad</key>
    <true/>

    <key>KeepAlive</key>
    <true/>

    <key>StandardOutPath</key>
    <string>/tmp/vigil.out.log</string>

    <key>StandardErrorPath</key>
    <string>/tmp/vigil.err.log</string>

    <key>EnvironmentVariables</key>
    <dict>
        <key>RUST_LOG</key>
        <string>info</string>
    </dict>
</dict>
</plist>
```

### Service Management Commands

```bash
# Install service
vigil service install

# Uninstall service
vigil service uninstall

# Check service status
vigil service status

# View service logs
vigil service logs
```

## Implementation

### File: `src/cli/service.rs`

```rust
pub fn install() -> Result<()> {
    let plist_path = dirs::home_dir()
        .unwrap()
        .join("Library/LaunchAgents/ch.kapptec.vigil.plist");

    // Generate plist content
    let plist = generate_plist()?;

    // Write plist file
    std::fs::write(&plist_path, plist)?;

    // Load service
    Command::new("launchctl")
        .args(["load", plist_path.to_str().unwrap()])
        .status()?;

    println!("Service installed and started.");
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let plist_path = /* ... */;

    // Unload service
    Command::new("launchctl")
        .args(["unload", plist_path.to_str().unwrap()])
        .status()?;

    // Remove plist file
    std::fs::remove_file(&plist_path)?;

    println!("Service uninstalled.");
    Ok(())
}
```

## Graceful Shutdown

### Signal Handling

```rust
use tokio::signal;

async fn run_monitor(app: App) -> Result<()> {
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel(1);

    // Spawn signal handler
    tokio::spawn(async move {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
        tracing::info!("Shutdown signal received");
        let _ = shutdown_tx.send(());
    });

    // Main monitoring loop
    loop {
        tokio::select! {
            _ = shutdown_rx.recv() => {
                tracing::info!("Shutting down gracefully...");
                break;
            }
            result = ping_monitor.next() => {
                // Process ping result
            }
        }
    }

    // Cleanup
    if let Some(outage) = tracker.current_outage() {
        outage.notes = Some("Monitor shutdown during outage".to_string());
        app.db.update_outage(outage)?;
    }

    tracing::info!("Shutdown complete");
    Ok(())
}
```

## Log Rotation

### Built-in Rotation

Using `tracing-appender`:

```rust
use tracing_appender::rolling::{RollingFileAppender, Rotation};

let file_appender = RollingFileAppender::new(
    Rotation::DAILY,
    log_dir,
    "monitor.log",
);

// Keep last 7 days of logs
```

### Manual Cleanup

```rust
fn cleanup_old_logs(log_dir: &Path, max_age_days: u32) -> Result<()> {
    for entry in std::fs::read_dir(log_dir)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        let age = metadata.modified()?.elapsed()?;

        if age > Duration::from_secs(max_age_days as u64 * 86400) {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}
```

## Database Cleanup

Run periodically to enforce retention policy:

```rust
async fn periodic_cleanup(app: &App) {
    let mut interval = tokio::time::interval(Duration::from_secs(86400)); // Daily

    loop {
        interval.tick().await;

        match app.db.cleanup(app.config.database.retention_days) {
            Ok(deleted) => {
                tracing::info!("Cleaned up {} old records", deleted);
            }
            Err(e) => {
                tracing::error!("Database cleanup failed: {}", e);
            }
        }
    }
}
```

## Performance Optimization

### Memory Management

1. **Ping result sampling**: Don't store every ping, aggregate per minute
2. **Bounded channels**: Use bounded mpsc channels to prevent memory growth
3. **String interning**: Reuse target strings instead of cloning

```rust
// Aggregate pings per minute instead of storing each one
struct PingAggregator {
    minute_start: DateTime<Utc>,
    success_count: u32,
    failure_count: u32,
    latency_sum: f64,
    latency_count: u32,
}
```

### CPU Efficiency

1. **Sleep between pings**: Don't busy-wait
2. **Batch database writes**: Buffer writes, flush periodically

## Tasks

- [ ] Implement service install/uninstall commands
- [ ] Generate launchd plist dynamically
- [ ] Add graceful shutdown with signal handling
- [ ] Implement log rotation
- [ ] Add periodic database cleanup
- [ ] Implement ping aggregation for storage efficiency
- [ ] Profile memory usage in long-running tests
- [ ] Add health check endpoint (optional)

## Test Plan

### Manual Testing

```bash
# Test service installation
vigil service install
launchctl list | grep vigil

# Test graceful shutdown
vigil start --foreground
# Press Ctrl+C, verify clean shutdown message

# Test log rotation
# Run for multiple days, verify old logs removed

# Test database cleanup
# Insert old records, run cleanup, verify removal
```

### Performance Testing

```bash
# Monitor memory over 24 hours
while true; do
    ps aux | grep vigil | grep -v grep
    sleep 3600
done
```

## Acceptance Criteria

1. Service can be installed/uninstalled via CLI
2. Service starts automatically on login
3. Ctrl+C results in clean shutdown (no data loss)
4. Logs rotate daily, old logs deleted after 7 days
5. Database records cleaned per retention policy
6. Memory usage stable over 24+ hour runs (<50MB)
7. CPU usage minimal when idle (<1% average)
