use crate::cli::helpers::{format_duration_secs, parse_duration};
use crate::models::{interpret_hop, Outage};
use crate::App;
use chrono::Utc;
use std::collections::HashMap;

pub fn run(app: &App, last: &str) -> Result<(), Box<dyn std::error::Error>> {
    let duration = parse_duration(last).map_err(|e| format!("Invalid duration: {}", e))?;
    let since = Utc::now() - duration;
    let until = Utc::now();

    let outages = app.db.get_outages(since, until)?;

    println!("Recent Outages (last {})", last);
    println!("═══════════════════════════════════════════════════════════\n");

    if outages.is_empty() {
        println!("No outages recorded in this period.");
        return Ok(());
    }

    // Print table header
    println!(
        "{:>4}  {:<19}  {:>8}  Culprit",
        "ID", "Start Time", "Duration"
    );
    println!("{}", "─".repeat(75));

    // Print each outage
    for outage in &outages {
        print_outage_row(outage);
    }

    println!("{}", "─".repeat(75));

    // Summary
    let total_downtime: f64 = outages.iter().filter_map(|o| o.duration_secs).sum();
    println!(
        "\nSummary: {} outage{}, {} total downtime",
        outages.len(),
        if outages.len() == 1 { "" } else { "s" },
        format_duration_secs(total_downtime)
    );

    // Most common failing hop
    let mut hop_counts: HashMap<u8, u32> = HashMap::new();
    for outage in &outages {
        if let Some(hop) = outage.failing_hop {
            *hop_counts.entry(hop).or_insert(0) += 1;
        }
    }

    if let Some((hop, count)) = hop_counts.into_iter().max_by_key(|(_, count)| *count) {
        println!(
            "Most common culprit: Hop {} - {} ({} occurrence{})",
            hop,
            interpret_hop(hop),
            count,
            if count == 1 { "" } else { "s" }
        );
    }

    Ok(())
}

fn print_outage_row(outage: &Outage) {
    let id = outage
        .id
        .map(|i| i.to_string())
        .unwrap_or_else(|| "-".to_string());
    let start_time = outage.start_time.format("%Y-%m-%d %H:%M:%S").to_string();

    let duration = outage
        .duration_secs
        .map(format_duration_secs)
        .unwrap_or_else(|| "ongoing".to_string());

    // Enhanced failing hop display with full IP and interpretation
    let culprit = match (outage.failing_hop, &outage.failing_hop_ip) {
        (Some(hop), Some(ip)) => {
            format!("Hop {} {} ({})", hop, ip, interpret_hop(hop))
        }
        (Some(hop), None) => format!("Hop {} ({})", hop, interpret_hop(hop)),
        (None, _) => "Unknown".to_string(),
    };

    println!(
        "{:>4}  {:<19}  {:>8}  {}",
        id, start_time, duration, culprit
    );

    // Print affected targets on separate line if present
    if !outage.affected_targets.is_empty() {
        println!("{:35}Targets: {}", "", outage.affected_targets.join(", "));
    }
}
