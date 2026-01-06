pub mod config;
pub mod db;
pub mod models;
pub mod monitor;
pub mod cli;

use config::Config;
use std::path::Path;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize the logging framework
pub fn init_logging(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.logging.level));

    let subscriber = tracing_subscriber::registry().with(filter);

    // Console layer
    let console_layer = fmt::layer()
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .compact();

    // File layer (if configured)
    if let Ok(Some(log_path)) = config.log_path() {
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;

        let file_layer = fmt::layer()
            .with_target(true)
            .with_ansi(false)
            .with_writer(file);

        subscriber
            .with(console_layer)
            .with(file_layer)
            .init();
    } else {
        subscriber.with(console_layer).init();
    }

    Ok(())
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
            return line
                .strip_prefix("gateway:")
                .map(|s| s.trim().to_string());
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
