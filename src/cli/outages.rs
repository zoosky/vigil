use crate::cli::helpers::{format_duration_secs, parse_duration, truncate};
use crate::models::Outage;
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
        "{:<19}  {:>8}  {:>12}  Affected Targets",
        "Start Time", "Duration", "Failing Hop"
    );
    println!("{}", "─".repeat(65));

    // Print each outage
    for outage in &outages {
        print_outage_row(outage);
    }

    println!("{}", "─".repeat(65));

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
        let hop_name = match hop {
            1 => "Gateway/Router",
            2 => "ISP Modem",
            _ => "ISP Backbone",
        };
        println!(
            "Most common failing hop: {} ({}) - {} occurrence{}",
            hop,
            hop_name,
            count,
            if count == 1 { "" } else { "s" }
        );
    }

    Ok(())
}

fn print_outage_row(outage: &Outage) {
    let start_time = outage.start_time.format("%Y-%m-%d %H:%M:%S").to_string();

    let duration = outage
        .duration_secs
        .map(format_duration_secs)
        .unwrap_or_else(|| "ongoing".to_string());

    let failing_hop = match (outage.failing_hop, &outage.failing_hop_ip) {
        (Some(hop), Some(ip)) => format!("{} ({})", hop, truncate(ip, 8)),
        (Some(hop), None) => format!("{}", hop),
        (None, _) => "-".to_string(),
    };

    let affected = if outage.affected_targets.is_empty() {
        "-".to_string()
    } else if outage.affected_targets.len() <= 2 {
        outage.affected_targets.join(", ")
    } else {
        format!(
            "{}, +{} more",
            outage.affected_targets[0],
            outage.affected_targets.len() - 1
        )
    };

    println!(
        "{:<19}  {:>8}  {:>12}  {}",
        start_time,
        duration,
        failing_hop,
        truncate(&affected, 20)
    );
}
