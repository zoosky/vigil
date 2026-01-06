use clap::{Parser, Subcommand};
use networkmonitor::{config::Config, detect_gateway, monitor::PingMonitor, App};
use tokio::signal;

#[derive(Parser)]
#[command(name = "networkmonitor")]
#[command(author, version, about = "Network connectivity monitor for diagnosing intermittent outages")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the network monitor daemon
    Start {
        /// Run in foreground (don't daemonize)
        #[arg(short, long)]
        foreground: bool,
    },

    /// Show current network status
    Status,

    /// List recent outages
    Outages {
        /// Time period (e.g., "24h", "7d", "30d")
        #[arg(short, long, default_value = "24h")]
        last: String,
    },

    /// Show statistics
    Stats {
        /// Time period (e.g., "24h", "7d", "30d")
        #[arg(short, long, default_value = "24h")]
        period: String,
    },

    /// Run a manual traceroute
    Trace {
        /// Target IP or hostname
        #[arg(default_value = "8.8.8.8")]
        target: String,
    },

    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Initialize configuration and database
    Init,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show current configuration
    Show,

    /// Show configuration file path
    Path,

    /// Set a configuration value
    Set {
        /// Key to set (e.g., "monitor.ping_interval_ms")
        key: String,
        /// Value to set
        value: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => cmd_init()?,
        Commands::Config { action } => cmd_config(action)?,
        Commands::Start { foreground } => cmd_start(foreground).await?,
        Commands::Status => cmd_status()?,
        Commands::Outages { last } => cmd_outages(&last)?,
        Commands::Stats { period } => cmd_stats(&period)?,
        Commands::Trace { target } => cmd_trace(&target)?,
    }

    Ok(())
}

fn cmd_init() -> Result<(), Box<dyn std::error::Error>> {
    println!("Initializing network monitor...\n");

    // Create default config
    let config = Config::default();
    let config_path = Config::config_path()?;

    if config_path.exists() {
        println!("Configuration file already exists at:");
        println!("  {:?}\n", config_path);
    } else {
        config.save()?;
        println!("Created configuration file at:");
        println!("  {:?}\n", config_path);
    }

    // Initialize database
    let app = App::new()?;
    println!("Database initialized at:");
    println!("  {:?}\n", app.config.database_path()?);

    // Detect gateway
    if let Some(gateway) = detect_gateway() {
        println!("Detected gateway: {}", gateway);
        println!(
            "  (Add to config with: networkmonitor config set targets.gateway {})\n",
            gateway
        );
    } else {
        println!("Could not auto-detect gateway.");
        println!("  (Set manually with: networkmonitor config set targets.gateway <IP>)\n");
    }

    println!("Targets to monitor:");
    for target in app.config.all_targets() {
        println!("  - {} ({})", target.name, target.ip);
    }

    println!("\nRun 'networkmonitor start' to begin monitoring.");

    Ok(())
}

fn cmd_config(action: ConfigAction) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        ConfigAction::Show => {
            let config = Config::load()?;
            let toml_str = toml::to_string_pretty(&config)?;
            println!("{}", toml_str);
        }
        ConfigAction::Path => {
            let path = Config::config_path()?;
            println!("{}", path.display());
        }
        ConfigAction::Set { key, value } => {
            println!("Setting {} = {}", key, value);
            println!("(Configuration editing not yet implemented - edit config file directly)");
            let path = Config::config_path()?;
            println!("Config file: {}", path.display());
        }
    }
    Ok(())
}

async fn cmd_start(_foreground: bool) -> Result<(), Box<dyn std::error::Error>> {
    let app = App::new()?;

    println!("Network Monitor");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("Monitoring targets:");
    for target in app.config.all_targets() {
        println!("  • {} ({})", target.name, target.ip);
    }

    println!("\nSettings:");
    println!(
        "  Ping interval: {}ms",
        app.config.monitor.ping_interval_ms
    );
    println!("  Ping timeout: {}ms", app.config.monitor.ping_timeout_ms);
    println!(
        "  Degraded threshold: {} failures",
        app.config.monitor.degraded_threshold
    );
    println!(
        "  Offline threshold: {} failures",
        app.config.monitor.offline_threshold
    );

    println!("\nStarting monitoring... Press Ctrl+C to stop.\n");

    // Create ping monitor
    let monitor = PingMonitor::new(&app.config);
    let mut rx = monitor.start();

    // Track consecutive results per target for display
    let mut last_status: std::collections::HashMap<String, (bool, Option<f64>)> =
        std::collections::HashMap::new();

    loop {
        tokio::select! {
            // Handle Ctrl+C
            _ = signal::ctrl_c() => {
                println!("\n\nShutting down...");
                break;
            }

            // Handle ping results
            result = rx.recv() => {
                match result {
                    Some(ping_result) => {
                        let status_char = if ping_result.success { "✓" } else { "✗" };
                        let latency_str = ping_result
                            .latency_ms
                            .map(|l| format!("{:.1}ms", l))
                            .unwrap_or_else(|| ping_result.error.clone().unwrap_or_else(|| "timeout".to_string()));

                        // Only print if status changed or first result
                        let key = ping_result.target.clone();
                        let current = (ping_result.success, ping_result.latency_ms.map(|l| l.round()));
                        let should_print = last_status.get(&key) != Some(&current);

                        if should_print {
                            let timestamp = ping_result.timestamp.format("%H:%M:%S");
                            println!(
                                "[{}] {} {} ({}) - {}",
                                timestamp,
                                status_char,
                                ping_result.target_name,
                                ping_result.target,
                                latency_str
                            );

                            // Log to database
                            if let Err(e) = app.db.insert_ping(&ping_result) {
                                tracing::error!("Failed to log ping: {}", e);
                            }

                            last_status.insert(key, current);
                        }
                    }
                    None => {
                        // Channel closed, monitor stopped
                        break;
                    }
                }
            }
        }
    }

    println!("Monitor stopped.");
    Ok(())
}

fn cmd_status() -> Result<(), Box<dyn std::error::Error>> {
    let _app = App::new()?;

    println!("Network Monitor Status");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("[Status display not yet implemented - Phase 5]");

    Ok(())
}

fn cmd_outages(last: &str) -> Result<(), Box<dyn std::error::Error>> {
    let _app = App::new()?;

    println!("Recent Outages (last {})", last);
    println!("═══════════════════════════════════════════════════════════\n");

    println!("[Outages display not yet implemented - Phase 5]");

    Ok(())
}

fn cmd_stats(period: &str) -> Result<(), Box<dyn std::error::Error>> {
    let _app = App::new()?;

    println!("Statistics ({})", period);
    println!("═══════════════════════════════════════════════════════════\n");

    println!("[Statistics display not yet implemented - Phase 5]");

    Ok(())
}

fn cmd_trace(target: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("Traceroute to {}", target);
    println!("═══════════════════════════════════════════════════════════\n");

    println!("[Traceroute not yet implemented - Phase 4]");

    Ok(())
}
