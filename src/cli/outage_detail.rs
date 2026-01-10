use crate::cli::helpers::format_duration_secs;
use crate::db::TracerouteWithMeta;
use crate::models::{interpret_hop, Outage, TraceTrigger};
use crate::App;

pub fn run(app: &App, outage_id: i64) -> Result<(), Box<dyn std::error::Error>> {
    // Get the outage
    let outage = match app.db.get_outage(outage_id)? {
        Some(o) => o,
        None => {
            println!("Outage #{} not found.", outage_id);
            return Ok(());
        }
    };

    // Get associated traceroutes
    let traceroutes = app.db.get_traceroutes_for_outage(outage_id)?;

    println!("Outage #{} Details", outage_id);
    println!("═══════════════════════════════════════════════════════════\n");

    // Basic info
    println!(
        "Started:     {}",
        outage.start_time.format("%Y-%m-%d %H:%M:%S")
    );

    if let Some(end_time) = outage.end_time {
        println!("Ended:       {}", end_time.format("%Y-%m-%d %H:%M:%S"));
    } else {
        println!("Ended:       (ongoing)");
    }

    if let Some(duration) = outage.duration_secs {
        println!("Duration:    {}", format_duration_secs(duration));
    }

    // Culprit
    print_culprit(&outage);

    // Affected targets
    if !outage.affected_targets.is_empty() {
        println!("Targets:     {}", outage.affected_targets.join(", "));
    }

    // Notes
    if let Some(ref notes) = outage.notes {
        println!("Notes:       {}", notes);
    }

    // Traceroutes section
    println!("\nTraceroutes ({} captured)", traceroutes.len());
    println!("───────────────────────────────────────────────────────────");

    if traceroutes.is_empty() {
        println!("\nNo traceroutes recorded for this outage.");
    } else {
        for trace in &traceroutes {
            print_traceroute(trace);
        }
    }

    Ok(())
}

fn print_culprit(outage: &Outage) {
    let culprit = match (outage.failing_hop, &outage.failing_hop_ip) {
        (Some(hop), Some(ip)) => {
            format!("Hop {} - {} ({})", hop, ip, interpret_hop(hop))
        }
        (Some(hop), None) => format!("Hop {} ({})", hop, interpret_hop(hop)),
        (None, _) => "Unknown".to_string(),
    };
    println!("Culprit:     {}", culprit);
}

fn print_traceroute(trace: &TracerouteWithMeta) {
    let trigger_str = match trace.trigger {
        TraceTrigger::StateChange => "state_change",
        TraceTrigger::Periodic => "periodic",
        TraceTrigger::Manual => "manual",
    };

    println!(
        "\n[{}] {} - Target: {}",
        trace.result.timestamp.format("%H:%M:%S"),
        trigger_str,
        trace.result.target
    );

    let mut last_responding_hop: Option<(u8, &str)> = None;

    for hop in &trace.result.hops {
        let status = if hop.timeout {
            "✗ TIMEOUT".to_string()
        } else if hop.latency_ms.is_some() {
            let ip = hop.ip.as_deref().unwrap_or("?");
            last_responding_hop = Some((hop.hop_number, ip));
            format!("✓ {}", interpret_hop(hop.hop_number))
        } else {
            "?".to_string()
        };

        let ip_str = hop.ip.as_deref().unwrap_or("* * *");
        let latency_str = hop
            .latency_ms
            .map(|l| format!("{:.1} ms", l))
            .unwrap_or_default();

        println!(
            "  {:>2}  {:<15}  {:>8}   {}",
            hop.hop_number, ip_str, latency_str, status
        );
    }

    // Summary line
    if trace.result.success {
        println!("  → Connection recovered");
    } else if let Some((hop, ip)) = last_responding_hop {
        println!("  → Last responding: Hop {} ({})", hop, ip);
    } else {
        println!("  → All hops timed out");
    }
}
