use crate::cli::helpers::{format_duration_secs, progress_bar};
use crate::monitor::PingMonitor;
use crate::App;
use chrono::{Duration, Utc};

pub async fn run(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    println!("Network Monitor Status");
    println!("═══════════════════════════════════════════════════════════\n");

    // Check current connectivity by pinging targets
    let targets = app.config.all_targets();
    let monitor = PingMonitor::new(&app.config);

    println!("Target Health:");
    for target in &targets {
        let result = monitor.ping(target).await;
        let status = if result.success { "✓" } else { "✗" };
        let latency = result
            .latency_ms
            .map(|l| format!("{:.1}ms", l))
            .unwrap_or_else(|| "timeout".to_string());

        println!("  {} {} ({}) - {}", status, target.name, target.ip, latency);
    }

    // Get today's statistics
    let now = Utc::now();
    let today_start = now - Duration::hours(24);
    let stats = app.db.get_stats(today_start, now)?;

    println!("\nLast 24 Hours:");
    println!(
        "  Availability: {} {:.2}%",
        progress_bar(stats.availability_percent, 20),
        stats.availability_percent
    );
    println!("  Outages: {}", stats.total_outages);

    if stats.total_downtime_secs > 0.0 {
        println!(
            "  Total downtime: {}",
            format_duration_secs(stats.total_downtime_secs)
        );
    }

    if let Some(avg) = stats.avg_outage_duration_secs {
        println!("  Avg outage duration: {}", format_duration_secs(avg));
    }

    // Check for ongoing outage
    if let Some(outage) = app.db.get_ongoing_outage()? {
        let duration = (Utc::now() - outage.start_time).num_seconds() as f64;
        println!("\n⚠️  ONGOING OUTAGE:");
        println!(
            "  Started: {}",
            outage.start_time.format("%Y-%m-%d %H:%M:%S")
        );
        println!("  Duration: {}", format_duration_secs(duration));
        if let Some(hop) = outage.failing_hop {
            let hop_ip = outage.failing_hop_ip.as_deref().unwrap_or("unknown");
            println!("  Failing hop: {} ({})", hop, hop_ip);
        }
    }

    Ok(())
}
