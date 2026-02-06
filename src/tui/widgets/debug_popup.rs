//! Debug popup widget showing collector timing and rates state.
//!
//! Available only in live mode via `!` key.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::collector::CollectorTiming;
use crate::tui::state::AppState;

/// Renders the debug popup showing collector timing and rates state.
pub fn render_debug_popup(
    frame: &mut Frame,
    area: Rect,
    state: &AppState,
    timing: Option<&CollectorTiming>,
) {
    // Calculate popup size (centered, 60x28)
    let popup_width = 60.min(area.width.saturating_sub(4));
    let popup_height = 28.min(area.height.saturating_sub(4));
    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    // Clear the area
    frame.render_widget(Clear, popup_area);

    // Build content lines
    let mut lines = Vec::new();

    // Section: Collector Timing
    lines.push(Line::from(vec![Span::styled(
        "=== Collector Timing ===",
        Style::default().fg(Color::Yellow),
    )]));

    if let Some(t) = timing {
        lines.push(format_timing_line("Total", t.total));
        lines.push(format_timing_line("  Stat", t.stat));
        lines.push(format_timing_line("  Processes", t.processes));
        lines.push(format_timing_line("  MemInfo", t.meminfo));
        lines.push(format_timing_line("  CPUInfo", t.cpuinfo));
        lines.push(format_timing_line("  LoadAvg", t.loadavg));
        lines.push(format_timing_line("  DiskStats", t.diskstats));
        lines.push(format_timing_line("  NetDev", t.netdev));
        lines.push(format_timing_line("  PSI", t.psi));
        lines.push(format_timing_line("  Vmstat", t.vmstat));
        lines.push(format_timing_line("  NetSNMP", t.netsnmp));
        lines.push(format_timing_line("  PG Activity", t.pg_activity));
        lines.push(format_timing_line("  PG Statements", t.pg_statements));
        lines.push(format_timing_line("  Cgroup", t.cgroup));
        // Show PG statements caching interval
        if let Some(interval) = t.pg_stmts_cache_interval {
            let interval_str = if interval.is_zero() {
                "0 (disabled)".to_string()
            } else {
                format!("{}s", interval.as_secs())
            };
            lines.push(format_info_line("  PGS Cache Intv", interval_str));
        }
    } else {
        lines.push(Line::from("  (no timing data)"));
    }

    lines.push(Line::from(""));

    // Section: Rates State (PGS)
    lines.push(Line::from(vec![Span::styled(
        "=== PGS Rates State ===",
        Style::default().fg(Color::Yellow),
    )]));

    lines.push(format_info_line(
        "prev_sample_ts",
        state
            .pgs_prev_sample_ts
            .map(|ts| format!("{}", ts))
            .unwrap_or_else(|| "--".to_string()),
    ));
    lines.push(format_info_line(
        "last_update_ts",
        state
            .pgs_last_real_update_ts
            .map(|ts| format!("{}", ts))
            .unwrap_or_else(|| "--".to_string()),
    ));
    lines.push(format_info_line(
        "dt_secs",
        state
            .pgs_dt_secs
            .map(|dt| format!("{:.1}s", dt))
            .unwrap_or_else(|| "--".to_string()),
    ));
    lines.push(format_info_line(
        "rates_count",
        format!("{}", state.pgs_rates.len()),
    ));
    // Show current collected_at from PGS data (key for debugging dt issues)
    lines.push(format_info_line(
        "curr_collected_at",
        state
            .pgs_current_collected_at
            .map(|ts| format!("{}", ts))
            .unwrap_or_else(|| "--".to_string()),
    ));
    // Show snapshot timestamp for comparison
    lines.push(format_info_line(
        "snapshot_ts",
        state
            .current_snapshot
            .as_ref()
            .map(|s| format!("{}", s.timestamp))
            .unwrap_or_else(|| "--".to_string()),
    ));

    // Section: PostgreSQL Error
    if let Some(ref err) = state.pg_last_error {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "=== PostgreSQL Error ===",
            Style::default().fg(Color::Red),
        )]));
        lines.push(Line::from(vec![Span::styled(
            err.clone(),
            Style::default().fg(Color::Red),
        )]));
    }

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Debug Info (!) ")
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Cyan)),
        )
        .alignment(Alignment::Left);

    frame.render_widget(paragraph, popup_area);
}

fn format_timing_line(label: &str, duration: std::time::Duration) -> Line<'static> {
    let ms = duration.as_secs_f64() * 1000.0;
    let color = if ms > 100.0 {
        Color::Red
    } else if ms > 10.0 {
        Color::Yellow
    } else {
        Color::White
    };

    Line::from(vec![
        Span::styled(format!("{:16}", label), Style::default().fg(Color::Cyan)),
        Span::styled(format!("{:>8.2} ms", ms), Style::default().fg(color)),
    ])
}

fn format_info_line(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{:16}", label), Style::default().fg(Color::Cyan)),
        Span::raw(value),
    ])
}
