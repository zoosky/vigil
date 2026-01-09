use crate::cli::helpers::{format_duration_secs, parse_duration, progress_bar};
use crate::App;
use chrono::{Timelike, Utc};
use std::collections::HashMap;

pub fn run(app: &App, period: &str) -> Result<(), Box<dyn std::error::Error>> {
    let duration = parse_duration(period).map_err(|e| format!("Invalid duration: {}", e))?;
    let since = Utc::now() - duration;
    let until = Utc::now();

    let stats = app.db.get_stats(since, until)?;
    let outages = app.db.get_outages(since, until)?;

    println!("Statistics (last {})", period);
    println!("═══════════════════════════════════════════════════════════\n");

    println!(
        "Period: {} → {}",
        since.format("%Y-%m-%d %H:%M"),
        until.format("%Y-%m-%d %H:%M")
    );

    // Availability bar
    println!("\nAvailability:");
    println!(
        "  {} {:.3}%",
        progress_bar(stats.availability_percent, 40),
        stats.availability_percent
    );

    // Outage statistics
    println!("\nOutages:");
    println!("  Total: {}", stats.total_outages);

    if stats.total_downtime_secs > 0.0 {
        println!(
            "  Total downtime: {}",
            format_duration_secs(stats.total_downtime_secs)
        );
    }

    if let Some(avg) = stats.avg_outage_duration_secs {
        println!("  Average duration: {}", format_duration_secs(avg));
    }

    // Find longest outage
    if let Some(longest) = outages
        .iter()
        .filter_map(|o| o.duration_secs)
        .max_by(|a, b| a.partial_cmp(b).unwrap())
    {
        println!("  Longest: {}", format_duration_secs(longest));
    }

    // Failing hop analysis
    if !outages.is_empty() {
        println!("\nFailing Hop Analysis:");

        let mut hop_stats: HashMap<u8, (u32, f64)> = HashMap::new();
        for outage in &outages {
            if let Some(hop) = outage.failing_hop {
                let entry = hop_stats.entry(hop).or_insert((0, 0.0));
                entry.0 += 1;
                entry.1 += outage.duration_secs.unwrap_or(0.0);
            }
        }

        let mut hop_list: Vec<_> = hop_stats.into_iter().collect();
        hop_list.sort_by(|a, b| b.1 .1.partial_cmp(&a.1 .1).unwrap());

        for (hop, (count, total_time)) in hop_list {
            let hop_name = match hop {
                1 => "Gateway/Router",
                2 => "ISP Modem",
                _ => "ISP Backbone",
            };
            println!(
                "  Hop {}: {} outage{} ({} total)",
                hop,
                count,
                if count == 1 { "" } else { "s" },
                format_duration_secs(total_time)
            );
            println!("    └─ {}", hop_name);
        }
    }

    // Time distribution (by 6-hour blocks)
    if !outages.is_empty() {
        println!("\nTime Distribution:");

        let mut time_blocks = [0u32; 4]; // 00-06, 06-12, 12-18, 18-24
        for outage in &outages {
            let hour = outage.start_time.hour();
            let block = (hour / 6) as usize;
            time_blocks[block] += 1;
        }

        let max_count = *time_blocks.iter().max().unwrap_or(&1);
        let block_names = ["00:00-06:00", "06:00-12:00", "12:00-18:00", "18:00-24:00"];

        for (name, count) in block_names.iter().zip(time_blocks.iter()) {
            let bar_width = if max_count > 0 {
                (*count as f64 / max_count as f64 * 12.0).round() as usize
            } else {
                0
            };
            println!(
                "  {}  {}  {} outage{}",
                name,
                "█".repeat(bar_width) + &"░".repeat(12 - bar_width),
                count,
                if *count == 1 { "" } else { "s" }
            );
        }
    }

    Ok(())
}
