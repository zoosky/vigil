use chrono::Duration;

/// Parse a duration string like "24h", "7d", "30d" into a chrono::Duration
pub fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Empty duration string".to_string());
    }

    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: i64 = num_str
        .parse()
        .map_err(|_| format!("Invalid number in duration: {}", num_str))?;

    match unit {
        "s" => Ok(Duration::seconds(num)),
        "m" => Ok(Duration::minutes(num)),
        "h" => Ok(Duration::hours(num)),
        "d" => Ok(Duration::days(num)),
        "w" => Ok(Duration::weeks(num)),
        _ => Err(format!(
            "Invalid duration unit '{}'. Use s, m, h, d, or w",
            unit
        )),
    }
}

/// Format a duration in seconds to a human-readable string
pub fn format_duration_secs(secs: f64) -> String {
    if secs < 60.0 {
        format!("{:.1}s", secs)
    } else if secs < 3600.0 {
        let mins = (secs / 60.0).floor();
        let remaining_secs = secs % 60.0;
        if remaining_secs < 1.0 {
            format!("{}m", mins as i64)
        } else {
            format!("{}m {}s", mins as i64, remaining_secs as i64)
        }
    } else {
        let hours = (secs / 3600.0).floor();
        let remaining_mins = ((secs % 3600.0) / 60.0).floor();
        if remaining_mins < 1.0 {
            format!("{}h", hours as i64)
        } else {
            format!("{}h {}m", hours as i64, remaining_mins as i64)
        }
    }
}

/// Format a chrono Duration to a human-readable string
pub fn format_duration(duration: Duration) -> String {
    format_duration_secs(duration.num_seconds() as f64)
}

/// Create a simple progress bar
pub fn progress_bar(percent: f64, width: usize) -> String {
    let filled = ((percent / 100.0) * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);

    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

/// Truncate a string to a maximum length, adding "..." if truncated
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len <= 3 {
        ".".repeat(max_len)
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("24h").unwrap(), Duration::hours(24));
        assert_eq!(parse_duration("7d").unwrap(), Duration::days(7));
        assert_eq!(parse_duration("30d").unwrap(), Duration::days(30));
        assert_eq!(parse_duration("1w").unwrap(), Duration::weeks(1));
        assert_eq!(parse_duration("60s").unwrap(), Duration::seconds(60));
        assert_eq!(parse_duration("30m").unwrap(), Duration::minutes(30));
    }

    #[test]
    fn test_parse_duration_errors() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("24x").is_err());
    }

    #[test]
    fn test_format_duration_secs() {
        assert_eq!(format_duration_secs(5.0), "5.0s");
        assert_eq!(format_duration_secs(65.0), "1m 5s");
        assert_eq!(format_duration_secs(3665.0), "1h 1m");
        assert_eq!(format_duration_secs(7200.0), "2h");
    }

    #[test]
    fn test_progress_bar() {
        assert_eq!(progress_bar(100.0, 10), "██████████");
        assert_eq!(progress_bar(50.0, 10), "█████░░░░░");
        assert_eq!(progress_bar(0.0, 10), "░░░░░░░░░░");
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
        assert_eq!(truncate("hi", 2), "hi");
    }
}
