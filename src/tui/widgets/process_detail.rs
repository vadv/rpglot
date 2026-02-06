//! Process detail popup widget showing comprehensive process information.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::storage::model::{ProcessInfo, Snapshot};
use crate::tui::state::AppState;

/// Renders the process detail popup centered on screen.
pub fn render_process_detail(frame: &mut Frame, area: Rect, state: &mut AppState) {
    // Get process by stored PID (to keep tracking it after sort changes)
    let pid = match state.process_detail_pid {
        Some(pid) => pid,
        None => return,
    };

    // Find process row by PID
    let selected_row = match state.process_table.items.iter().find(|r| r.pid == pid) {
        Some(row) => row,
        None => return, // Process no longer exists
    };

    // Get full ProcessInfo from current and previous snapshots
    let process_info = state
        .current_snapshot
        .as_ref()
        .and_then(|s| find_process_info(s, selected_row.pid));

    let prev_process_info = state
        .previous_snapshot
        .as_ref()
        .and_then(|s| find_process_info(s, selected_row.pid));

    // Calculate time interval between snapshots (seconds)
    let interval_secs = match (&state.current_snapshot, &state.previous_snapshot) {
        (Some(curr), Some(prev)) => {
            let delta = curr.timestamp - prev.timestamp;
            if delta > 0 { delta as f64 } else { 1.0 }
        }
        _ => 1.0,
    };

    // Calculate popup size (70% width, 85% height)
    let popup_width = (area.width * 70 / 100).clamp(60, 100);
    let popup_height = (area.height * 85 / 100).clamp(20, 40);

    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;

    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    // Clear the area behind popup
    frame.render_widget(Clear, popup_area);

    let title = format!(
        " Process Details: {} (PID {}) ",
        selected_row.name, selected_row.pid
    );
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Split inner area: content + footer
    let chunks = Layout::vertical([
        Constraint::Min(1),    // Content
        Constraint::Length(1), // Footer
    ])
    .split(inner);

    // Build content
    let content = build_content(selected_row, process_info, prev_process_info, interval_secs);
    let total_lines = content.len();

    // Apply scroll offset (clamp to valid range and update state)
    let max_scroll = total_lines.saturating_sub(chunks[0].height as usize);
    if state.process_detail_scroll > max_scroll {
        state.process_detail_scroll = max_scroll;
    }
    let scroll = state.process_detail_scroll;

    let paragraph = Paragraph::new(content)
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0))
        .style(Style::default().fg(Color::White));
    frame.render_widget(paragraph, chunks[0]);

    // Render footer with scroll hint
    let footer = Paragraph::new(Line::from(vec![
        Span::styled("↑/↓", Style::default().fg(Color::Yellow)),
        Span::styled(" scroll  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::styled("/", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::styled(" close", Style::default().fg(Color::DarkGray)),
    ]));
    frame.render_widget(footer, chunks[1]);
}

/// Finds ProcessInfo by PID in the snapshot.
fn find_process_info(snapshot: &Snapshot, pid: u32) -> Option<&ProcessInfo> {
    use crate::storage::model::DataBlock;
    for block in &snapshot.blocks {
        if let DataBlock::Processes(processes) = block {
            return processes.iter().find(|p| p.pid == pid);
        }
    }
    None
}

/// Builds the content lines for the popup.
fn build_content<'a>(
    row: &crate::tui::state::ProcessRow,
    info: Option<&ProcessInfo>,
    prev_info: Option<&ProcessInfo>,
    interval_secs: f64,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    // Section: Identity
    lines.push(section_header("Identity"));
    lines.push(key_value("PID", &row.pid.to_string()));
    if let Some(p) = info {
        lines.push(key_value("PPID", &p.ppid.to_string()));
    }
    lines.push(key_value("Name", &row.name));
    if !row.cmdline.is_empty() {
        lines.push(key_value("Command", &truncate_cmdline(&row.cmdline, 60)));
    }
    lines.push(key_value("State", &format_state(&row.state)));
    if let Some(p) = info {
        if p.tty != 0 {
            lines.push(key_value("TTY", &format!("{}", p.tty)));
        }
        lines.push(key_value("Start time", &format_btime(p.btime)));
        lines.push(key_value("Threads", &p.num_threads.to_string()));
    } else {
        lines.push(key_value("Threads", &row.num_threads.to_string()));
    }
    lines.push(Line::from(""));

    // Section: User/Group
    lines.push(section_header("User/Group"));
    lines.push(key_value(
        "Real UID",
        &format!("{} ({})", row.ruid, row.ruser),
    ));
    lines.push(key_value(
        "Effective UID",
        &format!("{} ({})", row.euid, row.euser),
    ));
    if let Some(p) = info {
        lines.push(key_value("Real GID", &p.gid.to_string()));
        lines.push(key_value("Effective GID", &p.egid.to_string()));
    }
    lines.push(Line::from(""));

    // Section: CPU Details
    lines.push(section_header("CPU"));
    lines.push(key_value("CPU %", &format!("{:.1}%", row.cpu_percent)));
    lines.push(key_value("User time", &format_ticks(row.usrcpu)));
    lines.push(key_value("System time", &format_ticks(row.syscpu)));
    lines.push(key_value("Current CPU", &row.cpunr.to_string()));
    lines.push(key_value("Run delay", &format_ns(row.rdelay)));
    if let Some(p) = info {
        lines.push(key_value("Nice", &p.cpu.nice.to_string()));
        lines.push(key_value("Priority", &p.cpu.prio.to_string()));
        if p.cpu.rtprio > 0 {
            lines.push(key_value("RT Priority", &p.cpu.rtprio.to_string()));
        }
        lines.push(key_value("Policy", &format_policy(p.cpu.policy)));
        lines.push(key_value("I/O wait time", &format_ticks(p.cpu.blkdelay)));
        // Context switches per second (if we have previous data)
        if let Some(prev) = prev_info {
            let nvcsw_delta = p.cpu.nvcsw.saturating_sub(prev.cpu.nvcsw);
            let nivcsw_delta = p.cpu.nivcsw.saturating_sub(prev.cpu.nivcsw);
            let nvcsw_rate = nvcsw_delta as f64 / interval_secs;
            let nivcsw_rate = nivcsw_delta as f64 / interval_secs;
            lines.push(key_value("Vol. ctx sw/s", &format_rate(nvcsw_rate)));
            lines.push(key_value("Invol. ctx sw/s", &format_rate(nivcsw_rate)));
        } else {
            lines.push(key_value(
                "Vol. ctx switches",
                &format!("{} (cumulative)", p.cpu.nvcsw),
            ));
            lines.push(key_value(
                "Invol. ctx switches",
                &format!("{} (cumulative)", p.cpu.nivcsw),
            ));
        }
    }
    lines.push(Line::from(""));

    // Section: Memory
    lines.push(section_header("Memory"));
    lines.push(key_value("MEM %", &format!("{:.1}%", row.mem_percent)));
    lines.push(key_value("Virtual (VSIZE)", &format_kb(row.vsize)));
    lines.push(key_value("Resident (RSIZE)", &format_kb(row.rsize)));
    lines.push(key_value("PSS (PSIZE)", &format_kb(row.psize)));
    lines.push(key_value("VGROW", &format_delta(row.vgrow)));
    lines.push(key_value("RGROW", &format_delta(row.rgrow)));
    lines.push(key_value("Code (VSTEXT)", &format_kb(row.vstext)));
    lines.push(key_value("Data (VDATA)", &format_kb(row.vdata)));
    lines.push(key_value("Stack (VSTACK)", &format_kb(row.vstack)));
    lines.push(key_value("Libraries (VSLIBS)", &format_kb(row.vslibs)));
    lines.push(key_value("Locked (LOCKSZ)", &format_kb(row.vlock)));
    lines.push(key_value("Swap (SWAPSZ)", &format_kb(row.vswap)));
    lines.push(key_value("Minor faults", &row.minflt.to_string()));
    lines.push(key_value("Major faults", &row.majflt.to_string()));
    lines.push(Line::from(""));

    // Section: Disk I/O (from ProcessInfo only)
    if let Some(p) = info {
        lines.push(section_header("Disk I/O"));

        // Calculate rates if we have previous data
        if let Some(prev) = prev_info {
            // Read ops/s
            let rio_delta = p.dsk.rio.saturating_sub(prev.dsk.rio);
            let rio_rate = rio_delta as f64 / interval_secs;
            lines.push(key_value("Read ops/s", &format_rate(rio_rate)));

            // Read bytes/s
            let rsz_delta = p.dsk.rsz.saturating_sub(prev.dsk.rsz);
            let rsz_rate = rsz_delta as f64 / interval_secs;
            lines.push(key_value("Read bytes/s", &format_bytes_rate(rsz_rate)));

            // Write ops/s
            let wio_delta = p.dsk.wio.saturating_sub(prev.dsk.wio);
            let wio_rate = wio_delta as f64 / interval_secs;
            lines.push(key_value("Write ops/s", &format_rate(wio_rate)));

            // Write bytes/s
            let wsz_delta = p.dsk.wsz.saturating_sub(prev.dsk.wsz);
            let wsz_rate = wsz_delta as f64 / interval_secs;
            lines.push(key_value("Write bytes/s", &format_bytes_rate(wsz_rate)));
        } else {
            // No previous data, show cumulative with note
            lines.push(key_value(
                "Read ops",
                &format!("{} (cumulative)", p.dsk.rio),
            ));
            lines.push(key_value(
                "Read bytes",
                &format!("{} (cumulative)", format_bytes(p.dsk.rsz)),
            ));
            lines.push(key_value(
                "Write ops",
                &format!("{} (cumulative)", p.dsk.wio),
            ));
            lines.push(key_value(
                "Write bytes",
                &format!("{} (cumulative)", format_bytes(p.dsk.wsz)),
            ));
        }

        // Always show cumulative totals
        lines.push(Line::from(""));
        lines.push(key_value("Total read ops", &p.dsk.rio.to_string()));
        lines.push(key_value("Total read bytes", &format_bytes(p.dsk.rsz)));
        lines.push(key_value("Total write ops", &p.dsk.wio.to_string()));
        lines.push(key_value("Total write bytes", &format_bytes(p.dsk.wsz)));

        if p.dsk.cwsz > 0 {
            lines.push(key_value("Cancelled writes", &format_bytes(p.dsk.cwsz)));
        }
    }

    lines
}

/// Creates a section header line.
fn section_header(name: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("─── {} ───", name),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    ))
}

/// Creates a key-value line with aligned columns.
fn key_value(key: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{:<20}", key), Style::default().fg(Color::Cyan)),
        Span::raw(value.to_string()),
    ])
}

/// Formats process state with description.
fn format_state(state: &str) -> String {
    match state {
        "R" => "R (Running)".to_string(),
        "S" => "S (Sleeping)".to_string(),
        "D" => "D (Disk sleep)".to_string(),
        "Z" => "Z (Zombie)".to_string(),
        "T" => "T (Stopped)".to_string(),
        "t" => "t (Tracing stop)".to_string(),
        "X" => "X (Dead)".to_string(),
        "I" => "I (Idle)".to_string(),
        other => other.to_string(),
    }
}

/// Formats scheduling policy.
fn format_policy(policy: i32) -> String {
    match policy {
        0 => "SCHED_NORMAL".to_string(),
        1 => "SCHED_FIFO".to_string(),
        2 => "SCHED_RR".to_string(),
        3 => "SCHED_BATCH".to_string(),
        5 => "SCHED_IDLE".to_string(),
        6 => "SCHED_DEADLINE".to_string(),
        other => format!("policy={}", other),
    }
}

/// Formats btime (unix timestamp) to human-readable date/time.
fn format_btime(btime: u32) -> String {
    if btime == 0 {
        return "-".to_string();
    }

    // Convert unix timestamp to date/time components
    // Using a simple algorithm without external dependencies
    let secs = btime as i64;

    // Days since Unix epoch (1970-01-01)
    let days = secs / 86400;
    let time_of_day = secs % 86400;

    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate year, month, day from days since epoch
    // Using the algorithm for Gregorian calendar
    let (year, month, day) = days_to_ymd(days);

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year, month, day, hours, minutes, seconds
    )
}

/// Converts days since Unix epoch to (year, month, day).
fn days_to_ymd(days: i64) -> (i32, u32, u32) {
    // Algorithm based on Howard Hinnant's date algorithms
    // https://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // day [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // month [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

/// Formats ticks to human-readable.
fn format_ticks(ticks: u64) -> String {
    if ticks == 0 {
        return "0".to_string();
    }
    // Assuming 100 ticks per second (standard Linux)
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

/// Formats nanoseconds to human-readable.
fn format_ns(ns: u64) -> String {
    if ns == 0 {
        return "0".to_string();
    }
    let ms = ns / 1_000_000;
    if ms > 1000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else if ms > 0 {
        format!("{}ms", ms)
    } else {
        format!("{}µs", ns / 1000)
    }
}

/// Formats KB to human-readable.
fn format_kb(kb: u64) -> String {
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

/// Formats bytes to human-readable.
fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0".to_string();
    }
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

/// Formats memory delta (can be negative).
fn format_delta(delta: i64) -> String {
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

/// Formats rate (ops/s) to human-readable.
fn format_rate(rate: f64) -> String {
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

/// Formats bytes per second rate to human-readable.
fn format_bytes_rate(rate: f64) -> String {
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

/// Truncates command line if too long.
fn truncate_cmdline(cmdline: &str, max_len: usize) -> String {
    if cmdline.len() <= max_len {
        cmdline.to_string()
    } else {
        format!("{}...", &cmdline[..max_len - 3])
    }
}
