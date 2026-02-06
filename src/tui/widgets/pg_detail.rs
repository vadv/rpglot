//! PostgreSQL session detail popup widget.
//! Shows detailed information about a selected PostgreSQL session from pg_stat_activity.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::storage::StringInterner;
use crate::storage::model::{DataBlock, PgStatActivityInfo, ProcessInfo, Snapshot};
use crate::tui::state::AppState;

/// Renders the PostgreSQL session detail popup.
pub fn render_pg_detail(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    interner: Option<&StringInterner>,
) {
    let pid = match state.pg_detail_pid {
        Some(p) => p,
        None => return,
    };

    let snapshot = match &state.current_snapshot {
        Some(s) => s,
        None => return,
    };

    // Find the PostgreSQL session
    let pg_info = find_pg_activity(snapshot, pid);
    if pg_info.is_none() {
        state.show_pg_detail = false;
        state.pg_detail_pid = None;
        return;
    }
    let pg_info = pg_info.unwrap();

    // Find corresponding OS process info (by PID)
    let process_info = if pid > 0 {
        find_process_info(snapshot, pid as u32)
    } else {
        None
    };

    // Current timestamp for duration calculation
    let now = snapshot.timestamp;

    // Build content
    let content = build_content(pg_info, process_info, now, interner);

    // Calculate popup dimensions (80% width, 80% height, centered)
    let popup_width = (area.width as f32 * 0.8) as u16;
    let popup_height = (area.height as f32 * 0.8) as u16;
    let popup_x = (area.width - popup_width) / 2;
    let popup_y = (area.height - popup_height) / 2;

    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    // Clear the background
    frame.render_widget(Clear, popup_area);

    // Split popup: main content + footer
    let chunks = Layout::vertical([
        Constraint::Min(1),    // Content
        Constraint::Length(1), // Footer
    ])
    .split(popup_area);

    // Clamp scroll offset
    let visible_lines = chunks[0].height.saturating_sub(2) as usize; // -2 for borders
    let max_scroll = content.len().saturating_sub(visible_lines);
    state.pg_detail_scroll = state.pg_detail_scroll.min(max_scroll);

    // Render content with scroll
    let title = format!(" PostgreSQL Session: PID {} ", pid);
    let content_paragraph = Paragraph::new(content)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::Black)),
        )
        .wrap(Wrap { trim: false })
        .scroll((state.pg_detail_scroll as u16, 0));

    frame.render_widget(content_paragraph, chunks[0]);

    // Render footer
    let footer = Paragraph::new(Line::from(vec![
        Span::styled("Press ", Style::default().fg(Color::DarkGray)),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::styled(" or ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::styled(" to close, ", Style::default().fg(Color::DarkGray)),
        Span::styled("↑/↓", Style::default().fg(Color::Yellow)),
        Span::styled(" to scroll", Style::default().fg(Color::DarkGray)),
    ]));
    frame.render_widget(footer, chunks[1]);
}

/// Find PostgreSQL session by PID.
fn find_pg_activity(snapshot: &Snapshot, pid: i32) -> Option<&PgStatActivityInfo> {
    for block in &snapshot.blocks {
        if let DataBlock::PgStatActivity(activities) = block {
            return activities.iter().find(|a| a.pid == pid);
        }
    }
    None
}

/// Find OS process info by PID.
fn find_process_info(snapshot: &Snapshot, pid: u32) -> Option<&ProcessInfo> {
    for block in &snapshot.blocks {
        if let DataBlock::Processes(processes) = block {
            return processes.iter().find(|p| p.pid == pid);
        }
    }
    None
}

/// Builds the content lines for the popup.
fn build_content<'a>(
    pg: &PgStatActivityInfo,
    process: Option<&ProcessInfo>,
    now: i64,
    interner: Option<&StringInterner>,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    // Resolve strings from interner
    let db = resolve_hash(interner, pg.datname_hash);
    let user = resolve_hash(interner, pg.usename_hash);
    let app = resolve_hash(interner, pg.application_name_hash);
    let state_str = resolve_hash(interner, pg.state_hash);
    let backend_type = resolve_hash(interner, pg.backend_type_hash);
    let wait_event_type = resolve_hash(interner, pg.wait_event_type_hash);
    let wait_event = resolve_hash(interner, pg.wait_event_hash);
    let query = resolve_hash(interner, pg.query_hash);

    // Section 1: Session Identity
    lines.push(section_header("Session Identity"));
    lines.push(key_value("PID", &pg.pid.to_string()));
    lines.push(key_value("Database", &db));
    lines.push(key_value("User", &user));
    lines.push(key_value("Application", &app));
    lines.push(key_value("Client Address", &pg.client_addr));
    lines.push(key_value("Backend Type", &backend_type));
    lines.push(Line::from(""));

    // Section 2: Timing
    lines.push(section_header("Timing"));
    lines.push(key_value(
        "Backend Start",
        &format_timestamp(pg.backend_start),
    ));
    lines.push(key_value(
        "Transaction Start",
        &format_timestamp_or_none(pg.xact_start),
    ));
    lines.push(key_value(
        "Query Start",
        &format_timestamp_or_none(pg.query_start),
    ));

    let query_duration = if pg.query_start > 0 {
        now - pg.query_start
    } else {
        0
    };
    let xact_duration = if pg.xact_start > 0 {
        now - pg.xact_start
    } else {
        0
    };
    let backend_duration = if pg.backend_start > 0 {
        now - pg.backend_start
    } else {
        0
    };

    lines.push(key_value_styled(
        "Query Duration",
        &format_duration(query_duration),
        duration_style(query_duration, &state_str),
    ));
    lines.push(key_value(
        "Transaction Duration",
        &format_duration_or_none(xact_duration),
    ));
    lines.push(key_value(
        "Backend Uptime",
        &format_duration(backend_duration),
    ));
    lines.push(Line::from(""));

    // Section 3: State & Wait
    lines.push(section_header("State & Wait"));
    lines.push(key_value_styled(
        "State",
        &state_str,
        state_style(&state_str),
    ));
    lines.push(key_value("Wait Event Type", &wait_event_type));
    lines.push(key_value("Wait Event", &wait_event));
    lines.push(Line::from(""));

    // Section 4: OS Process (if available)
    if let Some(p) = process {
        lines.push(section_header("OS Process"));
        lines.push(key_value("OS PID", &p.pid.to_string()));
        lines.push(key_value("Threads", &p.num_threads.to_string()));
        lines.push(key_value("State", &p.state.to_string()));
        lines.push(key_value("User Time", &format_ticks(p.cpu.utime)));
        lines.push(key_value("System Time", &format_ticks(p.cpu.stime)));
        lines.push(key_value("Current CPU", &p.cpu.curcpu.to_string()));
        lines.push(key_value("Nice", &p.cpu.nice.to_string()));
        lines.push(key_value("Priority", &p.cpu.prio.to_string()));
        lines.push(Line::from(""));

        // Memory
        lines.push(key_value("Virtual Memory", &format_kb(p.mem.vmem)));
        lines.push(key_value("Resident (RSS)", &format_kb(p.mem.rmem)));
        lines.push(key_value("Shared Memory", &format_kb(p.mem.pmem)));
        lines.push(key_value("Swap", &format_kb(p.mem.vswap)));
        lines.push(key_value("Minor Faults", &p.mem.minflt.to_string()));
        lines.push(key_value("Major Faults", &p.mem.majflt.to_string()));
        lines.push(Line::from(""));

        // Disk I/O
        lines.push(key_value("Read Bytes", &format_bytes(p.dsk.rsz)));
        lines.push(key_value("Write Bytes", &format_bytes(p.dsk.wsz)));
        lines.push(key_value("Read Ops", &p.dsk.rio.to_string()));
        lines.push(key_value("Write Ops", &p.dsk.wio.to_string()));
        lines.push(Line::from(""));
    } else {
        lines.push(section_header("OS Process"));
        lines.push(Line::from(Span::styled(
            "  OS process not found (PID mismatch or access denied)",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));
    }

    // Section 5: Query
    lines.push(section_header("Query"));
    if query.is_empty() || query == "-" {
        lines.push(Line::from(Span::styled(
            "  (no query)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        // Split query into lines for better display
        for line in query.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {}", line),
                Style::default().fg(Color::White),
            )));
        }
    }

    lines
}

/// Resolve hash to string using interner.
fn resolve_hash(interner: Option<&StringInterner>, hash: u64) -> String {
    if hash == 0 {
        return "-".to_string();
    }
    interner
        .and_then(|i| i.resolve(hash))
        .map(|s| s.to_string())
        .unwrap_or_else(|| "-".to_string())
}

/// Creates a section header line.
fn section_header(name: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("── {} ──", name),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    ))
}

/// Creates a key-value line.
fn key_value(key: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {:20}", key), Style::default().fg(Color::Cyan)),
        Span::styled(value.to_string(), Style::default()),
    ])
}

/// Creates a key-value line with custom value style.
fn key_value_styled(key: &str, value: &str, style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {:20}", key), Style::default().fg(Color::Cyan)),
        Span::styled(value.to_string(), style),
    ])
}

/// Format timestamp (epoch seconds) to human-readable datetime.
fn format_timestamp(ts: i64) -> String {
    if ts <= 0 {
        return "-".to_string();
    }
    // Use chrono for proper formatting
    use chrono::{TimeZone, Utc};
    Utc.timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "-".to_string())
}

/// Format timestamp or "not in transaction" for null values.
fn format_timestamp_or_none(ts: i64) -> String {
    if ts <= 0 {
        "-".to_string()
    } else {
        format_timestamp(ts)
    }
}

/// Format duration in human-readable format.
fn format_duration(secs: i64) -> String {
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
fn format_duration_or_none(secs: i64) -> String {
    if secs <= 0 {
        "-".to_string()
    } else {
        format_duration(secs)
    }
}

/// Style for query duration based on state and duration.
fn duration_style(secs: i64, state: &str) -> Style {
    let lower_state = state.to_lowercase();
    let is_active = lower_state == "active" || lower_state.contains("trans");

    if is_active && secs > 300 {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else if is_active && secs > 60 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    }
}

/// Style for state value.
fn state_style(state: &str) -> Style {
    let lower = state.to_lowercase();
    if lower.contains("idle") && lower.contains("trans") {
        Style::default().fg(Color::Yellow)
    } else if lower == "active" {
        Style::default().fg(Color::Green)
    } else if lower.contains("idle") {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    }
}

/// Format CPU ticks to human-readable time.
fn format_ticks(ticks: u64) -> String {
    // CLK_TCK is typically 100 on Linux
    let secs = ticks / 100;
    let ms = (ticks % 100) * 10;
    if secs >= 3600 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else if secs >= 60 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}.{:02}s", secs, ms / 10)
    }
}

/// Format KB to human-readable size.
fn format_kb(kb: u64) -> String {
    if kb >= 1024 * 1024 {
        format!("{:.1} GiB", kb as f64 / (1024.0 * 1024.0))
    } else if kb >= 1024 {
        format!("{:.1} MiB", kb as f64 / 1024.0)
    } else {
        format!("{} KiB", kb)
    }
}

/// Format bytes to human-readable size.
fn format_bytes(bytes: u64) -> String {
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
