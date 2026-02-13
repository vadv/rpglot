//! Shared primitives for detail popup widgets (PRC, PGA, PGS).

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::storage::StringInterner;
use crate::tui::style::Styles;

// ---------------------------------------------------------------------------
// Popup chrome
// ---------------------------------------------------------------------------

/// Returns a centered rect of given percentage within `area`.
pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

/// Renders a detail popup with unified chrome: border, scroll, footer.
///
/// `content` is the pre-built `Vec<Line>` from `build_content()`.
/// `scroll` is mutably borrowed to clamp it within bounds.
/// Returns nothing; renders directly to `frame`.
pub fn render_popup_frame(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    content: Vec<Line<'static>>,
    scroll: &mut usize,
    show_help: bool,
) {
    let popup_area = centered_rect(80, 85, area);
    frame.render_widget(Clear, popup_area);

    // Render the outer block (border + background) on the entire popup area
    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().fg(Color::White).bg(Color::Black));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Split inner area into content + footer
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);

    // Estimate visual lines after wrapping
    let inner_width = chunks[0].width as usize;
    let visual_lines: usize = if inner_width > 0 {
        content
            .iter()
            .map(|line| {
                let line_width: usize = line.spans.iter().map(|s| s.content.len()).sum();
                if line_width == 0 {
                    1
                } else {
                    line_width.div_ceil(inner_width)
                }
            })
            .sum()
    } else {
        content.len()
    };
    let visible_height = chunks[0].height as usize;
    let max_scroll = visual_lines.saturating_sub(visible_height);
    if *scroll > max_scroll {
        *scroll = max_scroll;
    }

    let bg = Style::default().fg(Color::White).bg(Color::Black);

    let paragraph = Paragraph::new(content)
        .style(bg)
        .wrap(Wrap { trim: false })
        .scroll((*scroll as u16, 0));
    frame.render_widget(paragraph, chunks[0]);

    // Footer
    let help_hint = if show_help { "? hide help" } else { "? help" };
    let footer = Line::from(vec![
        Span::styled("↑/↓", Styles::help_key()),
        Span::styled(" scroll  ", Styles::help()),
        Span::styled("PgUp/PgDn", Styles::help_key()),
        Span::styled(" page  ", Styles::help()),
        Span::styled(help_hint, Styles::help_key()),
        Span::styled("  ", Styles::help()),
        Span::styled("Esc", Styles::help_key()),
        Span::styled(" close", Styles::help()),
    ]);
    frame.render_widget(Paragraph::new(footer).style(bg), chunks[1]);
}

// ---------------------------------------------------------------------------
// Content formatting
// ---------------------------------------------------------------------------

/// Section header: `── {name} ──`
pub fn section(name: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("── {} ──", name),
        Styles::section_header(),
    ))
}

/// Simple key-value line. Key is right-aligned 20 chars with colon, Cyan.
pub fn kv(key: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{:>20}: ", key), Styles::cpu()),
        Span::raw(value.to_string()),
    ])
}

/// Key-value with custom value style.
pub fn kv_styled(key: &str, value: &str, style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{:>20}: ", key), Styles::cpu()),
        Span::styled(value.to_string(), style),
    ])
}

/// Inline help line (dim, indented under value column).
pub fn kv_help(text: &str) -> Line<'static> {
    Line::from(Span::styled(format!("{:>22} {}", "", text), Styles::help()))
}

/// Lookup help text for a key in a help table.
pub fn help_text<'a>(table: &'a [(&str, &str)], key: &str) -> Option<&'a str> {
    table.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

/// Conditionally push a help line after a metric.
pub fn push_help(
    lines: &mut Vec<Line<'static>>,
    show_help: bool,
    table: &[(&str, &str)],
    key: &str,
) {
    if show_help {
        if let Some(text) = help_text(table, key) {
            lines.push(kv_help(text));
        }
    }
}

// ---------------------------------------------------------------------------
// Delta styles
// ---------------------------------------------------------------------------

/// Style for i64 delta: green (+), red (-), dark gray (0).
pub fn delta_style(delta: i64) -> Style {
    if delta > 0 {
        Style::default().fg(Color::Green)
    } else if delta < 0 {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

/// Style for f64 delta: green (+), red (-), dark gray (~0).
pub fn delta_style_f64(delta: f64) -> Style {
    if delta > 0.0005 {
        Style::default().fg(Color::Green)
    } else if delta < -0.0005 {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

/// Key-value with i64 delta shown as colored span.
pub fn kv_delta_i64(key: &str, current: i64, prev: Option<i64>) -> Line<'static> {
    let mut spans = vec![
        Span::styled(format!("{:>20}: ", key), Styles::cpu()),
        Span::raw(current.to_string()),
    ];
    if let Some(p) = prev {
        let d = current - p;
        spans.push(Span::styled(format!("  {:+}", d), delta_style(d)));
    }
    Line::from(spans)
}

/// Key-value for PG block counters: displays blocks as human-readable bytes
/// (blocks * 8192), delta also in bytes.
pub fn kv_delta_blks(key: &str, blocks: i64, prev_blocks: Option<i64>) -> Line<'static> {
    let bytes = blocks as f64 * 8192.0;
    let mut spans = vec![
        Span::styled(format!("{:>20}: ", key), Styles::cpu()),
        Span::raw(format_blks_as_bytes(bytes)),
    ];
    if let Some(p) = prev_blocks {
        let d = blocks - p;
        let d_bytes = d as f64 * 8192.0;
        spans.push(Span::styled(
            format!(
                "  {}{}",
                if d >= 0 { "+" } else { "" },
                format_blks_as_bytes(d_bytes.abs())
            ),
            delta_style(d),
        ));
    }
    Line::from(spans)
}

/// Format bytes count (from blocks * 8192) to human-readable.
fn format_blks_as_bytes(bytes: f64) -> String {
    let abs = bytes.abs();
    if abs >= 1_099_511_627_776.0 {
        format!("{:.1} TB", bytes / 1_099_511_627_776.0)
    } else if abs >= 1_073_741_824.0 {
        format!("{:.1} GB", bytes / 1_073_741_824.0)
    } else if abs >= 1_048_576.0 {
        format!("{:.1} MB", bytes / 1_048_576.0)
    } else if abs >= 1024.0 {
        format!("{:.1} KB", bytes / 1024.0)
    } else if abs >= 1.0 {
        format!("{:.0} B", bytes)
    } else {
        "0".to_string()
    }
}

/// Key-value with f64 delta shown as colored span.
pub fn kv_delta_f64(key: &str, current: f64, prev: Option<f64>, precision: usize) -> Line<'static> {
    let mut spans = vec![
        Span::styled(format!("{:>20}: ", key), Styles::cpu()),
        Span::raw(format!("{:.prec$}", current, prec = precision)),
    ];
    if let Some(p) = prev {
        let d = current - p;
        spans.push(Span::styled(
            format!("  {:+.prec$}", d, prec = precision),
            delta_style_f64(d),
        ));
    }
    Line::from(spans)
}

// ---------------------------------------------------------------------------
// Value formatting
// ---------------------------------------------------------------------------

/// Format bytes to human-readable size.
pub fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GiB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

/// Format signed bytes with +/- prefix.
pub fn format_bytes_signed(bytes: i64) -> String {
    let sign = if bytes >= 0 { "+" } else { "-" };
    let abs = bytes.unsigned_abs();
    if abs >= 1024 * 1024 * 1024 {
        format!("{}{:.1} GiB", sign, abs as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if abs >= 1024 * 1024 {
        format!("{}{:.1} MiB", sign, abs as f64 / (1024.0 * 1024.0))
    } else if abs >= 1024 {
        format!("{}{:.1} KiB", sign, abs as f64 / 1024.0)
    } else {
        format!("{}{} B", sign, abs)
    }
}

/// Format KiB to human-readable size.
pub fn format_kb(kb: u64) -> String {
    if kb == 0 {
        return "0".to_string();
    }
    if kb >= 1024 * 1024 {
        format!("{:.1} GiB", kb as f64 / (1024.0 * 1024.0))
    } else if kb >= 1024 {
        format!("{:.1} MiB", kb as f64 / 1024.0)
    } else {
        format!("{} KiB", kb)
    }
}

/// Format rate (ops/s) to human-readable.
pub fn format_rate(rate: f64) -> String {
    if rate < 0.01 {
        "0".to_string()
    } else if rate >= 1_000_000.0 {
        format!("{:.1}M/s", rate / 1_000_000.0)
    } else if rate >= 1_000.0 {
        format!("{:.1}K/s", rate / 1_000.0)
    } else if rate >= 10.0 {
        format!("{:.0}/s", rate)
    } else {
        format!("{:.1}/s", rate)
    }
}

/// Format bytes per second rate to human-readable.
pub fn format_bytes_rate(rate: f64) -> String {
    if rate < 1.0 {
        "0".to_string()
    } else if rate >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} GiB/s", rate / (1024.0 * 1024.0 * 1024.0))
    } else if rate >= 1024.0 * 1024.0 {
        format!("{:.1} MiB/s", rate / (1024.0 * 1024.0))
    } else if rate >= 1024.0 {
        format!("{:.1} KiB/s", rate / 1024.0)
    } else {
        format!("{:.0} B/s", rate)
    }
}

/// Format CPU ticks to human-readable time.
pub fn format_ticks(ticks: u64) -> String {
    if ticks == 0 {
        return "0".to_string();
    }
    let secs = ticks / 100;
    let ms = (ticks % 100) * 10;
    if secs > 3600 {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    } else if secs > 60 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else if secs > 0 {
        format!("{}.{}s", secs, ms / 100)
    } else {
        format!("{}ms", ms)
    }
}

/// Format duration in seconds to human-readable.
pub fn format_duration(secs: i64) -> String {
    if secs <= 0 {
        return "0s".to_string();
    }
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}

/// Format duration or "-" for zero/invalid.
pub fn format_duration_or_none(secs: i64) -> String {
    if secs <= 0 {
        "-".to_string()
    } else {
        format_duration(secs)
    }
}

/// Format epoch timestamp as age from now, or "-" for zero/invalid.
/// Input is UNIX epoch seconds (from EXTRACT(EPOCH FROM timestamp)).
pub fn format_epoch_age(epoch_secs: i64) -> String {
    if epoch_secs <= 0 {
        return "-".to_string();
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let age = now.saturating_sub(epoch_secs);
    if age < 0 {
        return "-".to_string();
    }
    format_duration(age)
}

/// Resolve hash to string using interner.
pub fn resolve_hash(interner: Option<&StringInterner>, hash: u64) -> String {
    if hash == 0 {
        return "-".to_string();
    }
    interner
        .and_then(|i| i.resolve(hash))
        .map(|s| s.to_string())
        .unwrap_or_else(|| "-".to_string())
}

/// Format nanoseconds to human-readable.
pub fn format_ns(ns: u64) -> String {
    if ns == 0 {
        return "0".to_string();
    }
    let ms = ns / 1_000_000;
    if ms > 1000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else if ms > 0 {
        format!("{}ms", ms)
    } else {
        format!("{}us", ns / 1000)
    }
}

/// Format memory delta (can be negative) in KiB.
pub fn format_delta_kb(delta: i64) -> String {
    if delta == 0 {
        return "0".to_string();
    }
    let abs = delta.unsigned_abs();
    let sign = if delta < 0 { "-" } else { "+" };
    if abs >= 1024 * 1024 {
        format!("{}{:.1} GiB", sign, abs as f64 / (1024.0 * 1024.0))
    } else if abs >= 1024 {
        format!("{}{:.1} MiB", sign, abs as f64 / 1024.0)
    } else {
        format!("{}{} KiB", sign, abs)
    }
}
