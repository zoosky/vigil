pub mod cli;
pub mod config;
pub mod db;
pub mod models;
pub mod monitor;

use config::Config;
use std::path::Path;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize the logging framework with daily log rotation
pub fn init_logging(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.logging.level));

    let subscriber = tracing_subscriber::registry().with(filter);

    // Console layer
    let console_layer = fmt::layer()
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .compact();

    // File layer with daily rotation (if configured)
    if let Ok(Some(log_path)) = config.log_path() {
        if let Some(log_dir) = log_path.parent() {
            std::fs::create_dir_all(log_dir)?;

            // Use daily rotation - creates files like monitor.2024-01-15.log
            let file_appender = RollingFileAppender::new(Rotation::DAILY, log_dir, "monitor.log");

            let file_layer = fmt::layer()
                .with_target(true)
                .with_ansi(false)
                .with_writer(file_appender);

            subscriber.with(console_layer).with(file_layer).init();
        } else {
            subscriber.with(console_layer).init();
        }
    } else {
        subscriber.with(console_layer).init();
    }

    Ok(())
}

/// Clean up old log files older than max_age_days
pub fn cleanup_old_logs(
    log_dir: &Path,
    max_age_days: u32,
) -> Result<usize, Box<dyn std::error::Error>> {
    use std::time::Duration;

    let mut deleted = 0;
    let max_age = Duration::from_secs(max_age_days as u64 * 86400);

    for entry in std::fs::read_dir(log_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Only process log files
        if path.extension().is_none_or(|ext| ext != "log") {
            continue;
        }

        // Check file age
        if let Ok(metadata) = entry.metadata() {
            if let Ok(modified) = metadata.modified() {
                if let Ok(age) = modified.elapsed() {
                    if age > max_age && std::fs::remove_file(&path).is_ok() {
                        tracing::info!("Removed old log file: {:?}", path);
                        deleted += 1;
                    }
                }
            }
        }
    }

    Ok(deleted)
}

/// Initialize the application (config, logging, database)
pub struct App {
    pub config: Config,
    pub db: db::Database,
}

impl App {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let config = Config::load()?;
        init_logging(&config)?;

        let db_path = config.database_path()?;
        let db = db::Database::open(&db_path)?;

        tracing::info!("Database opened at {:?}", db_path);

        Ok(App { config, db })
    }

    /// Create app with a custom database path (for testing)
    #[allow(dead_code)]
    pub fn with_db_path(db_path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let config = Config::load()?;
        init_logging(&config)?;

        let db = db::Database::open(db_path)?;

        Ok(App { config, db })
    }
}

/// Detect the default gateway IP on macOS
pub fn detect_gateway() -> Option<String> {
    use std::process::Command;

    let output = Command::new("route")
        .args(["-n", "get", "default"])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        let line = line.trim();
        if line.starts_with("gateway:") {
            return line.strip_prefix("gateway:").map(|s| s.trim().to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_gateway() {
        // This test only works on macOS with a network connection
        if cfg!(target_os = "macos") {
            let gateway = detect_gateway();
            // Gateway should be detected on most systems
            println!("Detected gateway: {:?}", gateway);
        }
    }
}
