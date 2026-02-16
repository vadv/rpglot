//! Formatting helpers for process table cells.

use std::time::SystemTime;

/// Format CPU ticks as human-readable time.
pub(crate) fn format_ticks(ticks: u64) -> String {
    // Assuming 100 ticks per second (standard Linux)
    let seconds = ticks / 100;
    let minutes = seconds / 60;
    let hours = minutes / 60;
    if hours > 0 {
        format!("{}h{}m", hours, minutes % 60)
    } else if minutes > 0 {
        format!("{}m{}s", minutes, seconds % 60)
    } else {
        format!("{}s", seconds)
    }
}

/// Format nanoseconds delay as human-readable.
pub(crate) fn format_delay(ns: u64) -> String {
    let us = ns / 1000;
    let ms = us / 1000;
    let seconds = ms / 1000;
    if seconds > 0 {
        format!("{}s", seconds)
    } else if ms > 0 {
        format!("{}ms", ms)
    } else if us > 0 {
        format!("{}us", us)
    } else {
        "0".to_string()
    }
}

/// Format memory size (KB) as human-readable.
pub(crate) fn format_memory(kb: u64) -> String {
    if kb >= 1024 * 1024 {
        format!("{}G", kb / (1024 * 1024))
    } else if kb >= 1024 {
        format!("{}M", kb / 1024)
    } else {
        format!("{}K", kb)
    }
}

/// Format size delta (KB) with sign.
pub(crate) fn format_size_delta(delta: i64) -> String {
    if delta == 0 {
        "0".to_string()
    } else if delta > 0 {
        format!(
            "{}+{}",
            if delta >= 1024 {
                format!("{}M", delta / 1024)
            } else {
                format!("{}K", delta)
            },
            ""
        )
        .trim_end_matches('+')
        .to_string()
    } else {
        let abs_delta = delta.unsigned_abs() as i64;
        if abs_delta >= 1024 {
            format!("-{}M", abs_delta / 1024)
        } else {
            format!("-{}K", abs_delta)
        }
    }
}

/// Format bytes rate (bytes per second) as human-readable with auto units.
pub(crate) fn format_bytes_rate(rate: i64) -> String {
    let abs_rate = rate.unsigned_abs();
    let sign = if rate < 0 { "-" } else { "" };
    if abs_rate >= 1024 * 1024 * 1024 {
        format!("{}{}G/s", sign, abs_rate / (1024 * 1024 * 1024))
    } else if abs_rate >= 1024 * 1024 {
        format!("{}{}M/s", sign, abs_rate / (1024 * 1024))
    } else if abs_rate >= 1024 {
        format!("{}{}K/s", sign, abs_rate / 1024)
    } else if abs_rate > 0 {
        format!("{}{}B/s", sign, abs_rate)
    } else {
        "0".to_string()
    }
}

/// Format process start time (unix timestamp) as HH:MM or date.
pub(crate) fn format_start_time(btime: u32) -> String {
    use std::time::{Duration, UNIX_EPOCH};

    if btime == 0 {
        return "--".to_string();
    }

    let start = UNIX_EPOCH + Duration::from_secs(btime as u64);
    let now = SystemTime::now();

    if let Ok(duration) = now.duration_since(start) {
        let secs = duration.as_secs();
        // If started today (less than 24 hours ago), show HH:MM
        if secs < 24 * 3600 {
            // Calculate time of day
            if let Ok(epoch_secs) = start.duration_since(UNIX_EPOCH) {
                let total_secs = epoch_secs.as_secs();
                let hours = (total_secs / 3600) % 24;
                let minutes = (total_secs / 60) % 60;
                return format!("{:02}:{:02}", hours, minutes);
            }
        }
        // Otherwise show date
        let days = secs / (24 * 3600);
        if days < 365 {
            // Show month/day
            if let Ok(epoch_secs) = start.duration_since(UNIX_EPOCH) {
                // Simple approximation: calculate day of year
                let total_days = epoch_secs.as_secs() / (24 * 3600);
                let day_of_year = total_days % 365;
                let month = day_of_year / 30 + 1;
                let day = day_of_year % 30 + 1;
                return format!("{:02}/{:02}", month, day);
            }
        }
    }
    "--".to_string()
}
