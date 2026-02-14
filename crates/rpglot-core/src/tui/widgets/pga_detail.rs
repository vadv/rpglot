//! PostgreSQL session detail popup widget.
//! Shows detailed information about a selected PostgreSQL session from pg_stat_activity.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::storage::StringInterner;
use crate::storage::model::{DataBlock, PgStatActivityInfo, ProcessInfo, Snapshot};
use crate::tui::state::{AppState, PopupState};

use super::detail_common::{
    format_bytes, format_bytes_rate, format_duration, format_duration_or_none, format_kb,
    format_rate, format_ticks, kv, kv_styled, push_help, render_popup_frame, resolve_hash, section,
};

/// Help table for PGA metrics.
const HELP: &[(&str, &str)] = &[
    // Session Identity
    (
        "PID",
        "PostgreSQL backend OS process ID (matches /proc/pid)",
    ),
    (
        "Database",
        "Database this backend is connected to (from pg_stat_activity.datname)",
    ),
    (
        "User",
        "PostgreSQL role name executing queries in this session",
    ),
    (
        "Application",
        "Client-supplied application_name; set via connection string or SET application_name",
    ),
    (
        "Client Address",
        "IP address of the connected client; empty for local Unix socket connections",
    ),
    (
        "Backend Type",
        "Backend category: client backend, autovacuum worker, walwriter, checkpointer, bgwriter, etc.",
    ),
    // Timing
    (
        "Backend Start",
        "Timestamp when this backend process was forked by the postmaster",
    ),
    (
        "Transaction Start",
        "Timestamp when the current transaction began; NULL if no active transaction (idle)",
    ),
    (
        "Query Start",
        "Timestamp when the currently executing or last executed query started",
    ),
    (
        "Query Duration",
        "Wall-clock time since query_start; colored yellow >60s, red >300s for active queries",
    ),
    (
        "Transaction Duration",
        "Wall-clock time since xact_start; long idle-in-transaction sessions block vacuum",
    ),
    (
        "Backend Uptime",
        "Total lifetime of this backend process since it was started",
    ),
    // State & Wait
    (
        "State",
        "Backend state: active (executing), idle (waiting for command), idle in transaction, idle in transaction (aborted), fastpath function call, disabled",
    ),
    (
        "Wait Event Type",
        "Category of wait: Client, IPC, Lock, LWLock, IO, BufferPin, Timeout, Extension, Activity",
    ),
    (
        "Wait Event",
        "Specific wait event within the type; e.g. ClientRead, WALWrite, relation, transactionid",
    ),
    // OS Process
    (
        "OS PID",
        "Operating system process ID (same as PostgreSQL PID for single-host setups)",
    ),
    (
        "Threads",
        "Number of OS threads; PostgreSQL backends are typically single-threaded",
    ),
    (
        "User Time",
        "Cumulative CPU time in user mode (from /proc/pid/stat utime field; 100 ticks = 1 second)",
    ),
    (
        "System Time",
        "Cumulative CPU time in kernel mode (syscalls, I/O handling)",
    ),
    ("Current CPU", "Logical CPU core this backend last ran on"),
    (
        "Nice",
        "Scheduling nice value; PostgreSQL backends typically run at 0 (normal priority)",
    ),
    (
        "Priority",
        "Kernel scheduling priority; for SCHED_NORMAL this equals 20 + nice",
    ),
    (
        "Vol. ctx sw/s",
        "Voluntary context switches/s — backend yielded CPU voluntarily (waiting for I/O, lock, client data)",
    ),
    (
        "Invol. ctx sw/s",
        "Involuntary context switches/s — backend was preempted by the scheduler (CPU contention)",
    ),
    (
        "Vol. ctx switches",
        "Cumulative voluntary context switches since backend start",
    ),
    (
        "Invol. ctx switches",
        "Cumulative involuntary context switches since backend start",
    ),
    // Memory
    (
        "Virtual Memory",
        "Total virtual address space of the backend (includes shared_buffers mapping, libraries, stack)",
    ),
    (
        "Resident (RSS)",
        "Physical pages currently in RAM; for PostgreSQL backends includes shared_buffers pages",
    ),
    (
        "Shared Memory",
        "Shared pages (PSS-based); largely shared_buffers + other shared memory segments",
    ),
    (
        "Swap",
        "Backend address space currently swapped to disk; non-zero indicates memory pressure",
    ),
    (
        "Minor Faults",
        "Page faults resolved from page cache (no disk I/O); normal during buffer access",
    ),
    (
        "Major Faults",
        "Page faults requiring disk read; high values indicate working set exceeds available RAM",
    ),
    // Disk I/O
    (
        "Read bytes/s",
        "Bytes read from storage per second by this backend (from /proc/pid/io)",
    ),
    (
        "Write bytes/s",
        "Bytes written to storage per second (WAL writes, checkpoint writes, temp files)",
    ),
    ("Read ops/s", "Disk read system calls per second"),
    ("Write ops/s", "Disk write system calls per second"),
    (
        "Total Read Bytes",
        "Cumulative bytes read from storage since backend start",
    ),
    (
        "Total Write Bytes",
        "Cumulative bytes written to storage since backend start",
    ),
    (
        "Total Read Ops",
        "Cumulative read syscalls since backend start",
    ),
    (
        "Total Write Ops",
        "Cumulative write syscalls since backend start",
    ),
    (
        "Cancelled Writes",
        "Bytes of writes that were cancelled (e.g. temp file truncated before flush)",
    ),
];

/// Renders the PostgreSQL session detail popup.
pub fn render_pg_detail(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    interner: Option<&StringInterner>,
) {
    let (pid, show_help) = match &state.popup {
        PopupState::PgDetail { pid, show_help, .. } => (*pid, *show_help),
        _ => return,
    };

    let snapshot = match &state.current_snapshot {
        Some(s) => s,
        None => return,
    };

    // Find the PostgreSQL session
    let pg_info = match find_pg_activity(snapshot, pid) {
        Some(info) => info,
        None => {
            state.popup = PopupState::None;
            return;
        }
    };

    // Find corresponding OS process info (by PID)
    let process_info = if pid > 0 {
        find_process_info(snapshot, pid as u32)
    } else {
        None
    };

    // Find previous process info for delta calculation
    let prev_process_info = if pid > 0 {
        state
            .previous_snapshot
            .as_ref()
            .and_then(|s| find_process_info(s, pid as u32))
    } else {
        None
    };

    // Calculate time interval between snapshots
    let interval_secs = match (&state.current_snapshot, &state.previous_snapshot) {
        (Some(curr), Some(prev)) => {
            let delta = curr.timestamp - prev.timestamp;
            if delta > 0 { delta as f64 } else { 1.0 }
        }
        _ => 1.0,
    };

    let now = snapshot.timestamp;

    let title = format!("PostgreSQL Session: PID {}", pid);

    let content = build_content(
        pg_info,
        process_info,
        prev_process_info,
        interval_secs,
        now,
        interner,
        show_help,
    );

    let scroll = match &mut state.popup {
        PopupState::PgDetail { scroll, .. } => scroll,
        _ => return,
    };

    render_popup_frame(frame, area, &title, content, scroll, show_help);
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
    prev_process: Option<&ProcessInfo>,
    interval_secs: f64,
    now: i64,
    interner: Option<&StringInterner>,
    show_help: bool,
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
    lines.push(section("Session Identity"));
    lines.push(kv("PID", &pg.pid.to_string()));
    push_help(&mut lines, show_help, HELP, "PID");
    lines.push(kv("Database", &db));
    push_help(&mut lines, show_help, HELP, "Database");
    lines.push(kv("User", &user));
    push_help(&mut lines, show_help, HELP, "User");
    lines.push(kv("Application", &app));
    push_help(&mut lines, show_help, HELP, "Application");
    lines.push(kv("Client Address", &pg.client_addr));
    push_help(&mut lines, show_help, HELP, "Client Address");
    lines.push(kv("Backend Type", &backend_type));
    push_help(&mut lines, show_help, HELP, "Backend Type");
    lines.push(Line::from(""));

    // Section 2: Timing
    lines.push(section("Timing"));
    lines.push(kv("Backend Start", &format_timestamp(pg.backend_start)));
    push_help(&mut lines, show_help, HELP, "Backend Start");
    lines.push(kv(
        "Transaction Start",
        &format_timestamp_or_none(pg.xact_start),
    ));
    push_help(&mut lines, show_help, HELP, "Transaction Start");
    lines.push(kv("Query Start", &format_timestamp_or_none(pg.query_start)));
    push_help(&mut lines, show_help, HELP, "Query Start");

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

    lines.push(kv_styled(
        "Query Duration",
        &format_duration(query_duration),
        duration_style(query_duration, &state_str),
    ));
    push_help(&mut lines, show_help, HELP, "Query Duration");
    lines.push(kv(
        "Transaction Duration",
        &format_duration_or_none(xact_duration),
    ));
    push_help(&mut lines, show_help, HELP, "Transaction Duration");
    lines.push(kv("Backend Uptime", &format_duration(backend_duration)));
    push_help(&mut lines, show_help, HELP, "Backend Uptime");
    lines.push(Line::from(""));

    // Section 3: State & Wait
    lines.push(section("State & Wait"));
    lines.push(kv_styled("State", &state_str, state_style(&state_str)));
    push_help(&mut lines, show_help, HELP, "State");
    lines.push(kv("Wait Event Type", &wait_event_type));
    push_help(&mut lines, show_help, HELP, "Wait Event Type");
    lines.push(kv("Wait Event", &wait_event));
    push_help(&mut lines, show_help, HELP, "Wait Event");
    lines.push(Line::from(""));

    // Section 4: OS Process (if available)
    if let Some(p) = process {
        lines.push(section("OS Process"));
        lines.push(kv("OS PID", &p.pid.to_string()));
        push_help(&mut lines, show_help, HELP, "OS PID");
        lines.push(kv("Threads", &p.num_threads.to_string()));
        push_help(&mut lines, show_help, HELP, "Threads");
        lines.push(kv("State", &p.state.to_string()));
        lines.push(kv("User Time", &format_ticks(p.cpu.utime)));
        push_help(&mut lines, show_help, HELP, "User Time");
        lines.push(kv("System Time", &format_ticks(p.cpu.stime)));
        push_help(&mut lines, show_help, HELP, "System Time");
        lines.push(kv("Current CPU", &p.cpu.curcpu.to_string()));
        push_help(&mut lines, show_help, HELP, "Current CPU");
        lines.push(kv("Nice", &p.cpu.nice.to_string()));
        push_help(&mut lines, show_help, HELP, "Nice");
        lines.push(kv("Priority", &p.cpu.prio.to_string()));
        push_help(&mut lines, show_help, HELP, "Priority");

        // Context switches (rate if prev available, cumulative otherwise)
        if let Some(prev) = prev_process {
            let nvcsw_rate = p.cpu.nvcsw.saturating_sub(prev.cpu.nvcsw) as f64 / interval_secs;
            let nivcsw_rate = p.cpu.nivcsw.saturating_sub(prev.cpu.nivcsw) as f64 / interval_secs;
            lines.push(kv("Vol. ctx sw/s", &format_rate(nvcsw_rate)));
            push_help(&mut lines, show_help, HELP, "Vol. ctx sw/s");
            lines.push(kv("Invol. ctx sw/s", &format_rate(nivcsw_rate)));
            push_help(&mut lines, show_help, HELP, "Invol. ctx sw/s");
        } else {
            lines.push(kv(
                "Vol. ctx switches",
                &format!("{} (cumulative)", p.cpu.nvcsw),
            ));
            push_help(&mut lines, show_help, HELP, "Vol. ctx switches");
            lines.push(kv(
                "Invol. ctx switches",
                &format!("{} (cumulative)", p.cpu.nivcsw),
            ));
            push_help(&mut lines, show_help, HELP, "Invol. ctx switches");
        }
        lines.push(Line::from(""));

        // Memory
        lines.push(kv("Virtual Memory", &format_kb(p.mem.vmem)));
        push_help(&mut lines, show_help, HELP, "Virtual Memory");
        lines.push(kv("Resident (RSS)", &format_kb(p.mem.rmem)));
        push_help(&mut lines, show_help, HELP, "Resident (RSS)");
        lines.push(kv("Shared Memory", &format_kb(p.mem.pmem)));
        push_help(&mut lines, show_help, HELP, "Shared Memory");
        lines.push(kv("Swap", &format_kb(p.mem.vswap)));
        push_help(&mut lines, show_help, HELP, "Swap");
        lines.push(kv("Minor Faults", &p.mem.minflt.to_string()));
        push_help(&mut lines, show_help, HELP, "Minor Faults");
        lines.push(kv("Major Faults", &p.mem.majflt.to_string()));
        push_help(&mut lines, show_help, HELP, "Major Faults");
        lines.push(Line::from(""));

        // Disk I/O
        if let Some(prev) = prev_process {
            let rio_rate = p.dsk.rio.saturating_sub(prev.dsk.rio) as f64 / interval_secs;
            let rsz_rate = p.dsk.rsz.saturating_sub(prev.dsk.rsz) as f64 / interval_secs;
            let wio_rate = p.dsk.wio.saturating_sub(prev.dsk.wio) as f64 / interval_secs;
            let wsz_rate = p.dsk.wsz.saturating_sub(prev.dsk.wsz) as f64 / interval_secs;
            lines.push(kv("Read bytes/s", &format_bytes_rate(rsz_rate)));
            push_help(&mut lines, show_help, HELP, "Read bytes/s");
            lines.push(kv("Write bytes/s", &format_bytes_rate(wsz_rate)));
            push_help(&mut lines, show_help, HELP, "Write bytes/s");
            lines.push(kv("Read ops/s", &format_rate(rio_rate)));
            push_help(&mut lines, show_help, HELP, "Read ops/s");
            lines.push(kv("Write ops/s", &format_rate(wio_rate)));
            push_help(&mut lines, show_help, HELP, "Write ops/s");
            lines.push(Line::from(""));
        }
        lines.push(kv("Total Read Bytes", &format_bytes(p.dsk.rsz)));
        push_help(&mut lines, show_help, HELP, "Total Read Bytes");
        lines.push(kv("Total Write Bytes", &format_bytes(p.dsk.wsz)));
        push_help(&mut lines, show_help, HELP, "Total Write Bytes");
        lines.push(kv("Total Read Ops", &p.dsk.rio.to_string()));
        push_help(&mut lines, show_help, HELP, "Total Read Ops");
        lines.push(kv("Total Write Ops", &p.dsk.wio.to_string()));
        push_help(&mut lines, show_help, HELP, "Total Write Ops");
        if p.dsk.cwsz > 0 {
            lines.push(kv("Cancelled Writes", &format_bytes(p.dsk.cwsz)));
            push_help(&mut lines, show_help, HELP, "Cancelled Writes");
        }
        lines.push(Line::from(""));
    } else {
        lines.push(section("OS Process"));
        lines.push(Line::from(Span::styled(
            "  OS process not found (PID mismatch or access denied)",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));
    }

    // Section 5: Query
    lines.push(section("Query"));
    if query.is_empty() || query == "-" {
        lines.push(Line::from(Span::styled(
            "  (no query)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for line in query.lines() {
            // Replace tabs with spaces to avoid ratatui rendering artifacts
            let sanitized = line.replace('\t', "    ");
            lines.push(Line::from(Span::styled(
                format!("  {}", sanitized),
                Style::default().fg(Color::White),
            )));
        }
    }

    lines
}

/// Format timestamp (epoch seconds) to human-readable datetime.
fn format_timestamp(ts: i64) -> String {
    if ts <= 0 {
        return "-".to_string();
    }
    use chrono::{TimeZone, Utc};
    Utc.timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "-".to_string())
}

/// Format timestamp or "-" for null values.
fn format_timestamp_or_none(ts: i64) -> String {
    if ts <= 0 {
        "-".to_string()
    } else {
        format_timestamp(ts)
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
