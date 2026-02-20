//! PostgreSQL lock tree detail popup widget.
//! Shows detailed information about a selected node in the lock tree.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::storage::StringInterner;
use crate::storage::model::{DataBlock, PgLockTreeNode, ProcessInfo, Snapshot};
use crate::tui::state::{AppState, PopupState};

use super::detail_common::{
    format_bytes, format_bytes_rate, format_epoch_age, format_kb, format_rate, format_ticks, kv,
    push_help, render_popup_frame, resolve_hash, section,
};

/// Help table for PGL detail fields.
const HELP: &[(&str, &str)] = &[
    ("PID", "PostgreSQL backend OS process ID"),
    (
        "Depth",
        "Nesting level in the blocking chain (1 = root blocker)",
    ),
    (
        "Root PID",
        "PID of the root blocker that started this chain",
    ),
    ("Database", "Database this backend is connected to"),
    ("User", "PostgreSQL role name"),
    ("Application", "Client-supplied application_name"),
    (
        "Backend Type",
        "Backend category: client backend, autovacuum worker, etc.",
    ),
    (
        "State",
        "Backend state: active, idle, idle in transaction, etc.",
    ),
    (
        "Wait Event Type",
        "Category of wait: Client, IPC, Lock, LWLock, IO, etc.",
    ),
    (
        "Wait Event",
        "Specific wait event name within the wait_event_type category",
    ),
    (
        "Lock Type",
        "Type of lockable object: relation, transactionid, tuple, etc.",
    ),
    (
        "Lock Mode",
        "Lock mode requested or held: AccessShareLock, RowExclusiveLock, etc.",
    ),
    (
        "Lock Granted",
        "Whether the lock has been granted (true) or is being waited for (false)",
    ),
    (
        "Lock Target",
        "Object being locked: schema.table for relation locks, or lock type for others",
    ),
    (
        "Transaction Start",
        "Age since the current transaction began",
    ),
    (
        "Query Start",
        "Age since the current query started executing",
    ),
    ("State Change", "Age since the backend state last changed"),
    (
        "Query",
        "Full SQL query text currently being executed or last executed",
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

fn find_node(snapshot: &Snapshot, pid: i32) -> Option<&PgLockTreeNode> {
    snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::PgLockTree(v) = b {
            v.iter().find(|n| n.pid == pid)
        } else {
            None
        }
    })
}

fn find_process_info(snapshot: &Snapshot, pid: u32) -> Option<&ProcessInfo> {
    snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::Processes(processes) = b {
            processes.iter().find(|p| p.pid == pid)
        } else {
            None
        }
    })
}

pub fn render_pgl_detail(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    interner: Option<&StringInterner>,
) {
    let (pid, mut scroll, show_help) = match &state.popup {
        PopupState::PglDetail {
            pid,
            scroll,
            show_help,
        } => (*pid, *scroll, *show_help),
        _ => return,
    };

    let snapshot = match &state.current_snapshot {
        Some(s) => s,
        None => return,
    };

    let Some(node) = find_node(snapshot, pid) else {
        return;
    };

    let process_info = if node.pid > 0 {
        find_process_info(snapshot, node.pid as u32)
    } else {
        None
    };

    let prev_process_info = if node.pid > 0 {
        state
            .previous_snapshot
            .as_ref()
            .and_then(|s| find_process_info(s, node.pid as u32))
    } else {
        None
    };

    let interval_secs = match (&state.current_snapshot, &state.previous_snapshot) {
        (Some(curr), Some(prev)) => {
            let delta = curr.timestamp - prev.timestamp;
            if delta > 0 { delta as f64 } else { 1.0 }
        }
        _ => 1.0,
    };

    let r = |hash: u64| resolve_hash(interner, hash);

    let mut lines: Vec<Line> = Vec::new();

    // Identity section
    lines.push(section("Identity"));
    lines.push(kv("PID", &node.pid.to_string()));
    push_help(&mut lines, show_help, HELP, "PID");
    lines.push(kv("Depth", &node.depth.to_string()));
    push_help(&mut lines, show_help, HELP, "Depth");
    lines.push(kv("Root PID", &node.root_pid.to_string()));
    push_help(&mut lines, show_help, HELP, "Root PID");
    lines.push(kv("Database", &r(node.datname_hash)));
    push_help(&mut lines, show_help, HELP, "Database");
    lines.push(kv("User", &r(node.usename_hash)));
    push_help(&mut lines, show_help, HELP, "User");
    lines.push(kv("Application", &r(node.application_name_hash)));
    push_help(&mut lines, show_help, HELP, "Application");
    lines.push(kv("Backend Type", &r(node.backend_type_hash)));
    push_help(&mut lines, show_help, HELP, "Backend Type");

    // Lock Info section
    lines.push(section("Lock Info"));
    lines.push(kv("Lock Type", &r(node.lock_type_hash)));
    push_help(&mut lines, show_help, HELP, "Lock Type");
    lines.push(kv("Lock Mode", &r(node.lock_mode_hash)));
    push_help(&mut lines, show_help, HELP, "Lock Mode");
    lines.push(kv(
        "Lock Granted",
        if node.lock_granted { "true" } else { "false" },
    ));
    push_help(&mut lines, show_help, HELP, "Lock Granted");
    lines.push(kv("Lock Target", &r(node.lock_target_hash)));
    push_help(&mut lines, show_help, HELP, "Lock Target");

    // Timing section
    lines.push(section("Timing"));
    lines.push(kv(
        "Transaction Start",
        &format_epoch_age(node.xact_start as i64),
    ));
    push_help(&mut lines, show_help, HELP, "Transaction Start");
    lines.push(kv(
        "Query Start",
        &format_epoch_age(node.query_start as i64),
    ));
    push_help(&mut lines, show_help, HELP, "Query Start");
    lines.push(kv(
        "State Change",
        &format_epoch_age(node.state_change as i64),
    ));
    push_help(&mut lines, show_help, HELP, "State Change");

    // State section
    lines.push(section("State"));
    lines.push(kv("State", &r(node.state_hash)));
    push_help(&mut lines, show_help, HELP, "State");
    lines.push(kv("Wait Event Type", &r(node.wait_event_type_hash)));
    push_help(&mut lines, show_help, HELP, "Wait Event Type");
    lines.push(kv("Wait Event", &r(node.wait_event_hash)));
    push_help(&mut lines, show_help, HELP, "Wait Event");

    // OS Process section
    if let Some(p) = process_info {
        lines.push(section("OS Process"));
        lines.push(kv("OS PID", &p.pid.to_string()));
        push_help(&mut lines, show_help, HELP, "OS PID");
        lines.push(kv("Threads", &p.num_threads.to_string()));
        push_help(&mut lines, show_help, HELP, "Threads");
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

        if let Some(prev) = prev_process_info {
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
        if let Some(prev) = prev_process_info {
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

    // Query section
    lines.push(section("Query"));
    let query_text = r(node.query_hash);
    for line in query_text.lines() {
        let sanitized = line.replace('\t', "    ");
        lines.push(Line::raw(format!("  {}", sanitized)));
    }
    push_help(&mut lines, show_help, HELP, "Query");

    render_popup_frame(
        frame,
        area,
        "pg_locks detail",
        lines,
        &mut scroll,
        show_help,
    );

    // Write back scroll
    if let PopupState::PglDetail {
        scroll: ref mut s, ..
    } = state.popup
    {
        *s = scroll;
    }
}
