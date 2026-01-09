use clap::{Parser, Subcommand};
use tokio::signal;
use vigil::{
    cli,
    config::{Config, Environment},
    detect_gateway,
    models::ConnectivityState,
    monitor::{format_traceroute, ConnectivityTracker, HopAnalyzer, PingMonitor, StateEvent},
    App, VERSION,
};

#[derive(Parser)]
#[command(name = "vigil")]
#[command(
    author,
    version,
    about = "Keep watch over your network - monitor connectivity and diagnose intermittent outages"
)]
struct Cli {
    /// Use development environment (isolated database)
    #[arg(long, global = true)]
    dev: bool,

    /// Environment: production, development, test
    #[arg(long, short = 'e', global = true, env = "VIGIL_ENV")]
    env: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    /// Determine the environment from CLI flags
    fn environment(&self) -> Environment {
        if self.dev {
            return Environment::Development;
        }
        match self.env.as_deref() {
            Some("dev") | Some("development") => Environment::Development,
            Some("test") => Environment::Test,
            _ => Environment::from_env(),
        }
    }
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

    /// Manage the launchd service
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },

    /// Clean up old data
    Cleanup {
        /// Number of days to retain (default from config)
        #[arg(short, long)]
        days: Option<u32>,
    },

    /// Initialize configuration and database
    Init,

    /// Show version and environment info
    Version {
        /// Show detailed version info
        #[arg(short, long)]
        verbose: bool,
    },

    /// Upgrade database schema
    Upgrade {
        /// Show what would be done without making changes
        #[arg(long)]
        dry_run: bool,

        /// Skip creating a backup
        #[arg(long)]
        no_backup: bool,
    },
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

#[derive(Subcommand)]
enum ServiceAction {
    /// Install the launchd service
    Install,

    /// Uninstall the launchd service
    Uninstall,

    /// Show service status
    Status,

    /// View service logs
    Logs {
        /// Number of lines to show
        #[arg(short, long, default_value = "50")]
        lines: usize,

        /// Follow log output
        #[arg(short, long)]
        follow: bool,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let env = cli.environment();

    match cli.command {
        Commands::Init => cmd_init(&env)?,
        Commands::Config { action } => cmd_config(action, &env)?,
        Commands::Start { foreground } => cmd_start(foreground, &env).await?,
        Commands::Status => cmd_status(&env).await?,
        Commands::Outages { last } => cmd_outages(&last, &env)?,
        Commands::Stats { period } => cmd_stats(&period, &env)?,
        Commands::Trace { target } => cmd_trace(&target).await?,
        Commands::Service { action } => cmd_service(action)?,
        Commands::Cleanup { days } => cmd_cleanup(days, &env)?,
        Commands::Version { verbose } => cmd_version(verbose, &env)?,
        Commands::Upgrade { dry_run, no_backup } => cmd_upgrade(dry_run, no_backup, &env)?,
    }

    Ok(())
}

fn cmd_init(env: &Environment) -> Result<(), Box<dyn std::error::Error>> {
    println!("Initializing Vigil ({})...\n", env);

    // Create data directory
    let data_dir = env.data_dir()?;
    if !data_dir.exists() {
        std::fs::create_dir_all(&data_dir)?;
        println!("Created directory:");
        println!("  {}\n", data_dir.display());
    }

    // Create default config
    let config = Config::default();
    let config_path = env.config_path()?;

    if config_path.exists() {
        println!("Configuration file already exists at:");
        println!("  {}\n", config_path.display());
    } else {
        config.save_for_env(env)?;
        println!("Created configuration file at:");
        println!("  {}\n", config_path.display());
    }

    // Initialize database
    let app = App::with_env(*env)?;
    println!("Database initialized at:");
    println!("  {}\n", app.db_path()?.display());

    // Detect gateway
    if let Some(gateway) = detect_gateway() {
        println!("Detected gateway: {}", gateway);
        println!(
            "  (Add to config with: vigil config set targets.gateway {})\n",
            gateway
        );
    } else {
        println!("Could not auto-detect gateway.");
        println!("  (Set manually with: vigil config set targets.gateway <IP>)\n");
    }

    println!("Targets to monitor:");
    for target in app.config.all_targets() {
        println!("  - {} ({})", target.name, target.ip);
    }

    if env.is_dev() {
        println!("\nDevelopment environment initialized!");
        println!("Run with: vigil --dev <command>");
    } else {
        println!("\nRun 'vigil start' to begin monitoring.");
    }

    Ok(())
}

fn cmd_config(action: ConfigAction, env: &Environment) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        ConfigAction::Show => {
            let config = Config::load_for_env(env)?;
            let toml_str = toml::to_string_pretty(&config)?;
            println!("{}", toml_str);
        }
        ConfigAction::Path => {
            println!("Environment: {}", env);
            println!("Config:      {}", env.config_path()?.display());
            println!("Database:    {}", env.database_path()?.display());
            println!("Logs:        {}", env.log_path()?.display());
        }
        ConfigAction::Set { key, value } => {
            println!("Setting {} = {}", key, value);
            println!("(Configuration editing not yet implemented - edit config file directly)");
            let path = env.config_path()?;
            println!("Config file: {}", path.display());
        }
    }
    Ok(())
}

async fn cmd_start(_foreground: bool, env: &Environment) -> Result<(), Box<dyn std::error::Error>> {
    let app = App::with_env(*env)?;

    println!("Vigil Network Monitor ({})", env);
    println!("═══════════════════════════════════════════════════════════\n");

    let targets = app.config.all_targets();
    println!("Monitoring targets:");
    for target in &targets {
        println!("  • {} ({})", target.name, target.ip);
    }

    println!("\nSettings:");
    println!("  Ping interval: {}ms", app.config.monitor.ping_interval_ms);
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

    // Create ping monitor and state tracker
    let monitor = PingMonitor::new(&app.config);
    let mut tracker = ConnectivityTracker::new(&app.config.monitor, &targets);
    let mut rx = monitor.start();

    // Track for display (only print on changes)
    let mut last_status: std::collections::HashMap<String, (bool, Option<f64>)> =
        std::collections::HashMap::new();
    let mut current_outage_id: Option<i64> = None;

    loop {
        tokio::select! {
            // Handle Ctrl+C
            _ = signal::ctrl_c() => {
                println!("\n\nShutting down...");

                // End any ongoing outage
                if let Some(outage) = tracker.current_outage_mut() {
                    outage.end();
                    outage.notes = Some("Monitor shutdown during outage".to_string());
                    if let Some(id) = current_outage_id {
                        outage.id = Some(id);
                        if let Err(e) = app.db.update_outage(outage) {
                            tracing::error!("Failed to update outage on shutdown: {}", e);
                        }
                    }
                }
                break;
            }

            // Handle ping results
            result = rx.recv() => {
                match result {
                    Some(ping_result) => {
                        // Process through state machine
                        let event = tracker.process(&ping_result);

                        // Handle state events
                        match event {
                            StateEvent::Degraded { ref failing_targets } => {
                                println!(
                                    "\n⚠️  STATE: DEGRADED - Failing targets: {}\n",
                                    failing_targets.join(", ")
                                );
                            }
                            StateEvent::Offline { ref outage } => {
                                println!(
                                    "\n🔴 STATE: OFFLINE - Outage started at {}",
                                    outage.start_time.format("%H:%M:%S")
                                );

                                // Run traceroute to identify failing hop
                                let analyzer = HopAnalyzer::default();
                                let trace_target = targets.first()
                                    .map(|t| t.ip.as_str())
                                    .unwrap_or("8.8.8.8");

                                println!("   Running traceroute to {}...", trace_target);
                                let trace_result = analyzer.trace(trace_target).await;

                                let mut outage_to_save = outage.clone();

                                // Identify and record failing hop
                                if let Some((hop, ip)) = HopAnalyzer::identify_failing_hop(&trace_result) {
                                    println!("   Failing hop identified: {} ({})\n", hop, ip);
                                    outage_to_save.failing_hop = Some(hop);
                                    outage_to_save.failing_hop_ip = Some(ip);
                                } else if !trace_result.success {
                                    println!("   Could not identify failing hop (all timeouts)\n");
                                } else {
                                    println!("   Traceroute succeeded (intermittent issue)\n");
                                }

                                // Save outage to database
                                match app.db.insert_outage(&outage_to_save) {
                                    Ok(id) => {
                                        current_outage_id = Some(id);
                                        tracing::info!("Outage recorded with ID {}", id);

                                        // Also save traceroute
                                        if let Err(e) = app.db.insert_traceroute(Some(id), &trace_result) {
                                            tracing::error!("Failed to save traceroute: {}", e);
                                        }

                                        // Update tracker's outage with failing hop info
                                        if let Some(current) = tracker.current_outage_mut() {
                                            current.id = Some(id);
                                            current.failing_hop = outage_to_save.failing_hop;
                                            current.failing_hop_ip = outage_to_save.failing_hop_ip.clone();
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!("Failed to record outage: {}", e);
                                    }
                                }
                            }
                            StateEvent::Recovered { ref outage } => {
                                println!(
                                    "\n🟢 STATE: ONLINE - Outage ended, duration: {:.1}s\n",
                                    outage.duration_secs.unwrap_or(0.0)
                                );
                                // Update outage in database
                                if let Some(id) = current_outage_id.take() {
                                    let mut updated_outage = outage.clone();
                                    updated_outage.id = Some(id);
                                    if let Err(e) = app.db.update_outage(&updated_outage) {
                                        tracing::error!("Failed to update outage: {}", e);
                                    }
                                }
                            }
                            StateEvent::NoChange => {}
                        }

                        // Display ping result
                        let status_char = match tracker.state() {
                            ConnectivityState::Online => if ping_result.success { "✓" } else { "!" },
                            ConnectivityState::Degraded => if ping_result.success { "~" } else { "✗" },
                            ConnectivityState::Offline => if ping_result.success { "?" } else { "✗" },
                        };

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

                            // Log to database (sample - only on changes)
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

async fn cmd_status(env: &Environment) -> Result<(), Box<dyn std::error::Error>> {
    let app = App::with_env(*env)?;
    cli::status::run(&app).await
}

fn cmd_outages(last: &str, env: &Environment) -> Result<(), Box<dyn std::error::Error>> {
    let app = App::with_env(*env)?;
    cli::outages::run(&app, last)
}

fn cmd_stats(period: &str, env: &Environment) -> Result<(), Box<dyn std::error::Error>> {
    let app = App::with_env(*env)?;
    cli::stats::run(&app, period)
}

async fn cmd_trace(target: &str) -> Result<(), Box<dyn std::error::Error>> {
    let analyzer = HopAnalyzer::default();
    let result = analyzer.trace(target).await;

    print!("{}", format_traceroute(&result));

    Ok(())
}

fn cmd_service(action: ServiceAction) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        ServiceAction::Install => cli::service::install()?,
        ServiceAction::Uninstall => cli::service::uninstall()?,
        ServiceAction::Status => cli::service::status()?,
        ServiceAction::Logs { lines, follow } => cli::service::logs(lines, follow)?,
    }
    Ok(())
}

fn cmd_cleanup(days: Option<u32>, env: &Environment) -> Result<(), Box<dyn std::error::Error>> {
    let app = App::with_env(*env)?;

    let retention_days = days.unwrap_or(app.config.database.retention_days);

    println!(
        "Cleaning up data older than {} days ({})...\n",
        retention_days, env
    );

    // Clean up database
    let deleted = app.db.cleanup(retention_days)?;
    println!("Database: Deleted {} old records", deleted);

    // Clean up old log files
    if let Ok(Some(log_path)) = app.config.log_path_for_env(env) {
        if let Some(log_dir) = log_path.parent() {
            match vigil::cleanup_old_logs(log_dir, retention_days) {
                Ok(deleted_logs) => {
                    if deleted_logs > 0 {
                        println!("Logs: Deleted {} old log files", deleted_logs);
                    } else {
                        println!("Logs: No old log files to clean up");
                    }
                }
                Err(e) => {
                    println!("Logs: Failed to clean up - {}", e);
                }
            }
        }
    }

    println!("\nCleanup complete.");
    Ok(())
}

fn cmd_version(verbose: bool, env: &Environment) -> Result<(), Box<dyn std::error::Error>> {
    println!("vigil {}", VERSION);

    if verbose {
        println!();
        println!("Environment:     {}", env);
        println!("Config:          {}", env.config_path()?.display());
        println!("Database:        {}", env.database_path()?.display());
        println!();
        println!("Schema version:  {} (current)", vigil::DB_SCHEMA_VERSION);
    }

    Ok(())
}

fn cmd_upgrade(
    dry_run: bool,
    no_backup: bool,
    env: &Environment,
) -> Result<(), Box<dyn std::error::Error>> {
    use chrono::Utc;

    let db_path = env.database_path()?;

    if !db_path.exists() {
        println!("Database does not exist. Run 'vigil init' first.");
        return Ok(());
    }

    println!("Database: {}", db_path.display());
    println!("Current schema version: {}", vigil::DB_SCHEMA_VERSION);

    if dry_run {
        println!("\n[Dry run] No changes will be made.");
        println!("Database is at the latest schema version.");
        return Ok(());
    }

    // Create backup if requested
    if !no_backup {
        let backup_name = format!("monitor.db.backup_{}", Utc::now().format("%Y%m%d_%H%M%S"));
        let backup_path = db_path.parent().unwrap().join(&backup_name);
        std::fs::copy(&db_path, &backup_path)?;
        println!("\nBackup created: {}", backup_path.display());
    }

    // Open database (this will run any pending migrations)
    let _app = App::with_env(*env)?;

    println!("\nDatabase is up to date.");
    Ok(())
}
