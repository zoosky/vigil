use crate::models::Target;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("Failed to parse config file: {0}")]
    ParseError(#[from] toml::de::Error),
    #[error("Failed to serialize config: {0}")]
    SerializeError(#[from] toml::ser::Error),
    #[error("Could not determine config directory")]
    NoConfigDir,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    /// Interval between pings in milliseconds
    #[serde(default = "default_ping_interval")]
    pub ping_interval_ms: u64,

    /// Ping timeout in milliseconds
    #[serde(default = "default_ping_timeout")]
    pub ping_timeout_ms: u64,

    /// Consecutive failures to enter DEGRADED state
    #[serde(default = "default_degraded_threshold")]
    pub degraded_threshold: u32,

    /// Consecutive failures to enter OFFLINE state
    #[serde(default = "default_offline_threshold")]
    pub offline_threshold: u32,

    /// Consecutive successes to recover to ONLINE
    #[serde(default = "default_recovery_threshold")]
    pub recovery_threshold: u32,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            ping_interval_ms: default_ping_interval(),
            ping_timeout_ms: default_ping_timeout(),
            degraded_threshold: default_degraded_threshold(),
            offline_threshold: default_offline_threshold(),
            recovery_threshold: default_recovery_threshold(),
        }
    }
}

fn default_ping_interval() -> u64 {
    1000
}
fn default_ping_timeout() -> u64 {
    2000
}
fn default_degraded_threshold() -> u32 {
    3
}
fn default_offline_threshold() -> u32 {
    5
}
fn default_recovery_threshold() -> u32 {
    2
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetsConfig {
    /// Gateway IP (auto-detected if not set)
    pub gateway: Option<String>,

    /// List of targets to monitor
    #[serde(default = "default_targets")]
    pub targets: Vec<Target>,
}

impl Default for TargetsConfig {
    fn default() -> Self {
        Self {
            gateway: None,
            targets: default_targets(),
        }
    }
}

fn default_targets() -> Vec<Target> {
    vec![
        Target::new("Google DNS", "8.8.8.8"),
        Target::new("Cloudflare", "1.1.1.1"),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Path to the SQLite database
    pub path: Option<PathBuf>,

    /// Number of days to retain data
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: None,
            retention_days: default_retention_days(),
        }
    }
}

fn default_retention_days() -> u32 {
    90
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub level: String,

    /// Path to log file (optional)
    pub file: Option<PathBuf>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            file: None,
        }
    }
}

fn default_log_level() -> String {
    "info".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub monitor: MonitorConfig,

    #[serde(default)]
    pub targets: TargetsConfig,

    #[serde(default)]
    pub database: DatabaseConfig,

    #[serde(default)]
    pub logging: LoggingConfig,
}

impl Config {
    /// Load configuration from the default location or create default config
    pub fn load() -> Result<Self, ConfigError> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    /// Save configuration to the default location
    pub fn save(&self) -> Result<(), ConfigError> {
        let config_path = Self::config_path()?;

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(&config_path, content)?;
        Ok(())
    }

    /// Get the configuration file path
    pub fn config_path() -> Result<PathBuf, ConfigError> {
        let proj_dirs =
            ProjectDirs::from("ch", "kapptec", "vigil").ok_or(ConfigError::NoConfigDir)?;

        Ok(proj_dirs.config_dir().join("config.toml"))
    }

    /// Get the data directory path
    pub fn data_dir() -> Result<PathBuf, ConfigError> {
        let proj_dirs =
            ProjectDirs::from("ch", "kapptec", "vigil").ok_or(ConfigError::NoConfigDir)?;

        Ok(proj_dirs.data_dir().to_path_buf())
    }

    /// Get the database path (from config or default)
    pub fn database_path(&self) -> Result<PathBuf, ConfigError> {
        if let Some(ref path) = self.database.path {
            Ok(path.clone())
        } else {
            let data_dir = Self::data_dir()?;
            Ok(data_dir.join("monitor.db"))
        }
    }

    /// Get the log file path (from config or default)
    pub fn log_path(&self) -> Result<Option<PathBuf>, ConfigError> {
        if let Some(ref path) = self.logging.file {
            Ok(Some(path.clone()))
        } else {
            let data_dir = Self::data_dir()?;
            Ok(Some(data_dir.join("monitor.log")))
        }
    }

    /// Get all targets to monitor (including gateway if configured)
    pub fn all_targets(&self) -> Vec<Target> {
        let mut targets = Vec::new();

        if let Some(ref gateway) = self.targets.gateway {
            targets.push(Target::new("Gateway", gateway.clone()));
        }

        targets.extend(self.targets.targets.clone());
        targets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.monitor.ping_interval_ms, 1000);
        assert_eq!(config.monitor.degraded_threshold, 3);
        assert_eq!(config.targets.targets.len(), 2);
    }

    #[test]
    fn test_parse_config() {
        let toml_str = r#"
[monitor]
ping_interval_ms = 500
degraded_threshold = 5

[targets]
gateway = "192.168.1.1"
targets = [
    { name = "Custom", ip = "9.9.9.9" }
]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.monitor.ping_interval_ms, 500);
        assert_eq!(config.monitor.degraded_threshold, 5);
        assert_eq!(config.targets.gateway, Some("192.168.1.1".to_string()));
        assert_eq!(config.targets.targets.len(), 1);
    }
}
