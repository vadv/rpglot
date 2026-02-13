//! Styling functions for PostgreSQL activity table cells.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

/// Format bytes to human-readable (K/M/G).
pub(super) fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1}G", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1}K", bytes as f64 / 1024.0)
    } else {
        format!("{}B", bytes)
    }
}

/// Format duration in human-readable format.
fn format_duration(secs: i64) -> String {
    if secs < 0 {
        return "-".to_string();
    }
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d{}h", secs / 86400, (secs % 86400) / 3600)
    }
}

/// Format duration or "-" if no timestamp (None).
/// Some(0) shows "0s" (duration < 1 second), None shows "-" (timestamp was NULL).
pub(super) fn format_duration_or_dash(secs: Option<i64>) -> String {
    match secs {
        Some(s) if s >= 0 => format!("{:>7}", format_duration(s)),
        _ => format!("{:>7}", "-"),
    }
}

/// Truncate string to max length.
pub(super) fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        format!("{:<width$}", s, width = max_len)
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Normalize query text for single-line display.
/// Replaces newlines, carriage returns, and tabs with spaces.
pub(super) fn normalize_query(s: &str) -> String {
    s.replace('\n', " ").replace('\r', "").replace('\t', " ")
}

/// Style CPU% with color coding.
pub(super) fn styled_cpu(cpu: f64) -> Span<'static> {
    let text = format!("{:>5.1}", cpu);
    let style = if cpu > 80.0 {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else if cpu > 50.0 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    Span::styled(text, style)
}

/// Style state with color coding.
pub(super) fn styled_state(state: &str) -> Span<'static> {
    let lower = state.to_lowercase();
    let style = if lower.contains("idle") && lower.contains("trans") {
        Style::default().fg(Color::Yellow)
    } else if lower == "active" {
        Style::default().fg(Color::Green)
    } else if lower.contains("idle") {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };
    Span::styled(truncate(state, 16), style)
}

/// Style wait event with color coding.
/// Don't highlight yellow if state is idle (Client:ClientRead is normal for idle).
pub(super) fn styled_wait(wait: &str, is_idle: bool) -> Span<'static> {
    let style = if wait != "-" && !is_idle {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    Span::styled(truncate(wait, 20), style)
}

/// Returns true if state is plain "idle" (not "idle in transaction").
pub(super) fn is_idle_state(state: &str) -> bool {
    let lower = state.to_lowercase();
    lower.contains("idle") && !lower.contains("trans")
}

/// Style query duration with color coding.
pub(super) fn styled_duration(secs: Option<i64>, state: &str) -> Span<'static> {
    let text = format_duration_or_dash(secs);
    let lower_state = state.to_lowercase();
    let is_active = lower_state == "active" || lower_state.contains("trans");
    let s = secs.unwrap_or(0);

    let style = if is_active && s > 300 {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else if is_active && s > 60 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    Span::styled(text, style)
}

// ========== Stats view styling functions ==========

/// Style QDUR with anomaly detection (Stats view).
/// Compares current duration with historical MEAN and MAX from pg_stat_statements.
/// - Red + Bold: QDUR > MAX (new record!)
/// - Red: QDUR > 5x MEAN
/// - Yellow: QDUR > 2x MEAN
pub(super) fn styled_qdur_with_anomaly(
    secs: Option<i64>,
    state: &str,
    mean_exec_time_ms: Option<f64>,
    max_exec_time_ms: Option<f64>,
) -> Span<'static> {
    let text = format_duration_or_dash(secs);
    let lower_state = state.to_lowercase();
    let is_active = lower_state == "active" || lower_state.contains("trans");

    // Convert seconds to milliseconds for comparison
    let qdur_ms = (secs.unwrap_or(0) * 1000) as f64;

    let style = if !is_active {
        Style::default()
    } else if let Some(max_ms) = max_exec_time_ms {
        if max_ms > 0.0 && qdur_ms > max_ms {
            // New record! Exceeds historical max
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else if let Some(mean_ms) = mean_exec_time_ms {
            if mean_ms > 0.0 && qdur_ms > mean_ms * 5.0 {
                Style::default().fg(Color::Red)
            } else if mean_ms > 0.0 && qdur_ms > mean_ms * 2.0 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            }
        } else {
            Style::default()
        }
    } else if let Some(mean_ms) = mean_exec_time_ms {
        if mean_ms > 0.0 && qdur_ms > mean_ms * 5.0 {
            Style::default().fg(Color::Red)
        } else if mean_ms > 0.0 && qdur_ms > mean_ms * 2.0 {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        }
    } else {
        Style::default()
    };

    Span::styled(text, style)
}

/// Format and style MEAN execution time (milliseconds).
pub(super) fn styled_mean(mean_ms: Option<f64>) -> Span<'static> {
    match mean_ms {
        Some(ms) => {
            let text = format_ms(ms);
            Span::raw(format!("{:>7}", text))
        }
        None => Span::raw(format!("{:>7}", "--")),
    }
}

/// Format and style MAX execution time (milliseconds).
pub(super) fn styled_max(max_ms: Option<f64>) -> Span<'static> {
    match max_ms {
        Some(ms) => {
            let text = format_ms(ms);
            Span::raw(format!("{:>7}", text))
        }
        None => Span::raw(format!("{:>7}", "--")),
    }
}

/// Format and style CALL/s rate.
pub(super) fn styled_calls_s(calls_s: Option<f64>) -> Span<'static> {
    match calls_s {
        Some(rate) => {
            let text = if rate >= 1000.0 {
                format!("{:.1}K", rate / 1000.0)
            } else if rate >= 1.0 {
                format!("{:.1}", rate)
            } else {
                format!("{:.2}", rate)
            };
            Span::raw(format!("{:>7}", text))
        }
        None => Span::raw(format!("{:>7}", "--")),
    }
}

/// Format and style HIT% (buffer cache hit percentage).
/// - Red: < 50%
/// - Yellow: < 80%
pub(super) fn styled_hit_pct(hit_pct: Option<f64>) -> Span<'static> {
    match hit_pct {
        Some(pct) => {
            let text = format!("{:.0}%", pct);
            let style = if pct < 50.0 {
                Style::default().fg(Color::Red)
            } else if pct < 80.0 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };
            Span::styled(format!("{:>5}", text), style)
        }
        None => Span::raw(format!("{:>5}", "--")),
    }
}

/// Format milliseconds to human-readable.
fn format_ms(ms: f64) -> String {
    if ms >= 1000.0 {
        format!("{:.1}s", ms / 1000.0)
    } else if ms >= 1.0 {
        format!("{:.0}ms", ms)
    } else {
        format!("{:.1}ms", ms)
    }
}
