//! Process detail popup widget showing comprehensive process information.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;

use crate::storage::model::{ProcessInfo, Snapshot};
use crate::tui::state::{AppState, PopupState};

use super::detail_common::{
    format_bytes, format_bytes_rate, format_delta_kb, format_kb, format_ns, format_rate,
    format_ticks, kv, push_help, render_popup_frame, section,
};

/// Help table for process metrics.
const HELP: &[(&str, &str)] = &[
    // Identity
    ("PID", "Linux process ID assigned by the kernel"),
    ("PPID", "Parent process ID; 1 = adopted by init/systemd"),
    (
        "Name",
        "Executable name from /proc/pid/stat (comm field, max 16 chars)",
    ),
    ("Command", "Full command line from /proc/pid/cmdline"),
    (
        "State",
        "Process state: R=running, S=interruptible sleep, D=uninterruptible (disk I/O), Z=zombie, T=stopped, I=idle kernel thread",
    ),
    (
        "TTY",
        "Controlling terminal device number; 0 = no terminal (daemon)",
    ),
    (
        "Start time",
        "Wall-clock time when the process was created (from /proc/pid/stat start_time + boot time)",
    ),
    ("Threads", "Number of threads (tasks) in this thread group"),
    // User/Group
    (
        "Real UID",
        "Real user ID — the user who launched the process",
    ),
    (
        "Effective UID",
        "Effective user ID — determines actual permissions (may differ due to setuid)",
    ),
    ("Real GID", "Real group ID of the process owner"),
    (
        "Effective GID",
        "Effective group ID — determines group-level permissions",
    ),
    // CPU
    (
        "CPU %",
        "Percentage of one CPU core consumed: (Δutime + Δstime) / Δtotal_cpu * 100",
    ),
    (
        "User time",
        "Cumulative CPU time in user mode (clock ticks, 100 ticks = 1 second)",
    ),
    (
        "System time",
        "Cumulative CPU time in kernel mode (syscalls, page faults, I/O)",
    ),
    ("Current CPU", "Logical CPU core this process last ran on"),
    (
        "Run delay",
        "Time spent waiting in the CPU run queue (schedstat, nanoseconds); high values indicate CPU contention",
    ),
    (
        "Nice",
        "Scheduling nice value: −20 (highest priority) to +19 (lowest); affects CFS weight",
    ),
    (
        "Priority",
        "Kernel scheduling priority; for SCHED_NORMAL this is 20 + nice",
    ),
    (
        "RT Priority",
        "Real-time priority (1–99); only set for SCHED_FIFO/SCHED_RR policies",
    ),
    (
        "Policy",
        "Scheduling policy: NORMAL (CFS), FIFO/RR (real-time), BATCH, IDLE, DEADLINE",
    ),
    (
        "I/O wait time",
        "Cumulative ticks spent waiting for block I/O (blkio_delay from /proc/pid/stat)",
    ),
    (
        "Vol. ctx sw/s",
        "Voluntary context switches/s — process yielded CPU (e.g. waiting for I/O, mutex)",
    ),
    (
        "Invol. ctx sw/s",
        "Involuntary context switches/s — preempted by scheduler (CPU-bound process)",
    ),
    (
        "Vol. ctx switches",
        "Cumulative voluntary context switches since process start",
    ),
    (
        "Invol. ctx switches",
        "Cumulative involuntary context switches since process start",
    ),
    // Memory
    (
        "MEM %",
        "Resident memory as percentage of total physical RAM",
    ),
    (
        "Virtual (VSIZE)",
        "Total virtual address space (includes all mapped regions, even unused)",
    ),
    (
        "Resident (RSIZE)",
        "Physical pages currently in RAM (RSS); excludes swapped-out pages",
    ),
    (
        "PSS (PSIZE)",
        "Proportional Set Size — shared pages divided equally among all processes sharing them; more accurate than RSS for shared libs",
    ),
    (
        "VGROW",
        "Change in virtual memory size since previous snapshot (positive = allocation, negative = deallocation)",
    ),
    (
        "RGROW",
        "Change in resident memory since previous snapshot (positive = pages loaded, negative = pages evicted or freed)",
    ),
    (
        "Code (VSTEXT)",
        "Size of executable code segment (.text) mapped in memory",
    ),
    (
        "Data (VDATA)",
        "Size of initialized + uninitialized data segments (heap, BSS)",
    ),
    ("Stack (VSTACK)", "Size of the main thread stack"),
    (
        "Libraries (VSLIBS)",
        "Total size of shared library mappings (libc, libpthread, etc.)",
    ),
    (
        "Locked (LOCKSZ)",
        "Memory locked into RAM via mlock(); cannot be swapped out",
    ),
    (
        "Swap (SWAPSZ)",
        "Amount of address space currently swapped to disk",
    ),
    (
        "Minor faults",
        "Page faults resolved without disk I/O (page already in page cache)",
    ),
    (
        "Major faults",
        "Page faults requiring disk read (page not in memory); high values indicate memory pressure or cold startup",
    ),
    // Disk I/O
    (
        "Read ops/s",
        "Disk read system calls per second (from /proc/pid/io read_bytes delta)",
    ),
    ("Read bytes/s", "Bytes read from storage per second"),
    ("Write ops/s", "Disk write system calls per second"),
    ("Write bytes/s", "Bytes written to storage per second"),
    (
        "Read ops",
        "Cumulative disk read operations since process start",
    ),
    (
        "Read bytes",
        "Cumulative bytes read from storage since process start",
    ),
    (
        "Write ops",
        "Cumulative disk write operations since process start",
    ),
    (
        "Write bytes",
        "Cumulative bytes written to storage since process start",
    ),
    (
        "Total read ops",
        "Cumulative read syscalls (from /proc/pid/io)",
    ),
    ("Total read bytes", "Cumulative bytes read from storage"),
    ("Total write ops", "Cumulative write syscalls"),
    ("Total write bytes", "Cumulative bytes written to storage"),
    (
        "Cancelled writes",
        "Bytes whose write was started but then cancelled (e.g. file truncated before flush)",
    ),
];

/// Renders the process detail popup centered on screen.
pub fn render_process_detail(frame: &mut Frame, area: Rect, state: &mut AppState) {
    // Extract fields from popup state
    let (pid, show_help) = match &state.popup {
        PopupState::ProcessDetail { pid, show_help, .. } => (*pid, *show_help),
        _ => return,
    };

    // Find process row by PID
    let selected_row = match state.process_table.items.iter().find(|r| r.pid == pid) {
        Some(row) => row,
        None => {
            state.popup = PopupState::None;
            return;
        }
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

    let title = format!(
        "Process Details: {} (PID {})",
        selected_row.name, selected_row.pid
    );

    // Build content
    let content = build_content(
        selected_row,
        process_info,
        prev_process_info,
        interval_secs,
        show_help,
    );

    // Get mutable scroll reference and render
    let scroll = match &mut state.popup {
        PopupState::ProcessDetail { scroll, .. } => scroll,
        _ => return,
    };

    render_popup_frame(frame, area, &title, content, scroll, show_help);
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
    show_help: bool,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    // Section: Identity
    lines.push(section("Identity"));
    lines.push(kv("PID", &row.pid.to_string()));
    push_help(&mut lines, show_help, HELP, "PID");
    if let Some(p) = info {
        lines.push(kv("PPID", &p.ppid.to_string()));
        push_help(&mut lines, show_help, HELP, "PPID");
    }
    lines.push(kv("Name", &row.name));
    push_help(&mut lines, show_help, HELP, "Name");
    if !row.cmdline.is_empty() {
        lines.push(kv("Command", &truncate_cmdline(&row.cmdline, 60)));
        push_help(&mut lines, show_help, HELP, "Command");
    }
    lines.push(kv("State", &format_state(&row.state)));
    push_help(&mut lines, show_help, HELP, "State");
    if let Some(p) = info {
        if p.tty != 0 {
            lines.push(kv("TTY", &format!("{}", p.tty)));
            push_help(&mut lines, show_help, HELP, "TTY");
        }
        lines.push(kv("Start time", &format_btime(p.btime)));
        push_help(&mut lines, show_help, HELP, "Start time");
        lines.push(kv("Threads", &p.num_threads.to_string()));
        push_help(&mut lines, show_help, HELP, "Threads");
    } else {
        lines.push(kv("Threads", &row.num_threads.to_string()));
        push_help(&mut lines, show_help, HELP, "Threads");
    }
    lines.push(Line::from(""));

    // Section: User/Group
    lines.push(section("User/Group"));
    lines.push(kv("Real UID", &format!("{} ({})", row.ruid, row.ruser)));
    push_help(&mut lines, show_help, HELP, "Real UID");
    lines.push(kv(
        "Effective UID",
        &format!("{} ({})", row.euid, row.euser),
    ));
    push_help(&mut lines, show_help, HELP, "Effective UID");
    if let Some(p) = info {
        lines.push(kv("Real GID", &p.gid.to_string()));
        push_help(&mut lines, show_help, HELP, "Real GID");
        lines.push(kv("Effective GID", &p.egid.to_string()));
        push_help(&mut lines, show_help, HELP, "Effective GID");
    }
    lines.push(Line::from(""));

    // Section: CPU Details
    lines.push(section("CPU"));
    lines.push(kv("CPU %", &format!("{:.1}%", row.cpu_percent)));
    push_help(&mut lines, show_help, HELP, "CPU %");
    lines.push(kv("User time", &format_ticks(row.usrcpu)));
    push_help(&mut lines, show_help, HELP, "User time");
    lines.push(kv("System time", &format_ticks(row.syscpu)));
    push_help(&mut lines, show_help, HELP, "System time");
    lines.push(kv("Current CPU", &row.cpunr.to_string()));
    push_help(&mut lines, show_help, HELP, "Current CPU");
    lines.push(kv("Run delay", &format_ns(row.rdelay)));
    push_help(&mut lines, show_help, HELP, "Run delay");
    if let Some(p) = info {
        lines.push(kv("Nice", &p.cpu.nice.to_string()));
        push_help(&mut lines, show_help, HELP, "Nice");
        lines.push(kv("Priority", &p.cpu.prio.to_string()));
        push_help(&mut lines, show_help, HELP, "Priority");
        if p.cpu.rtprio > 0 {
            lines.push(kv("RT Priority", &p.cpu.rtprio.to_string()));
            push_help(&mut lines, show_help, HELP, "RT Priority");
        }
        lines.push(kv("Policy", &format_policy(p.cpu.policy)));
        push_help(&mut lines, show_help, HELP, "Policy");
        lines.push(kv("I/O wait time", &format_ticks(p.cpu.blkdelay)));
        push_help(&mut lines, show_help, HELP, "I/O wait time");
        // Context switches per second (if we have previous data)
        if let Some(prev) = prev_info {
            let nvcsw_delta = p.cpu.nvcsw.saturating_sub(prev.cpu.nvcsw);
            let nivcsw_delta = p.cpu.nivcsw.saturating_sub(prev.cpu.nivcsw);
            let nvcsw_rate = nvcsw_delta as f64 / interval_secs;
            let nivcsw_rate = nivcsw_delta as f64 / interval_secs;
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
    }
    lines.push(Line::from(""));

    // Section: Memory
    lines.push(section("Memory"));
    lines.push(kv("MEM %", &format!("{:.1}%", row.mem_percent)));
    push_help(&mut lines, show_help, HELP, "MEM %");
    lines.push(kv("Virtual (VSIZE)", &format_kb(row.vsize)));
    push_help(&mut lines, show_help, HELP, "Virtual (VSIZE)");
    lines.push(kv("Resident (RSIZE)", &format_kb(row.rsize)));
    push_help(&mut lines, show_help, HELP, "Resident (RSIZE)");
    lines.push(kv("PSS (PSIZE)", &format_kb(row.psize)));
    push_help(&mut lines, show_help, HELP, "PSS (PSIZE)");
    lines.push(kv("VGROW", &format_delta_kb(row.vgrow)));
    push_help(&mut lines, show_help, HELP, "VGROW");
    lines.push(kv("RGROW", &format_delta_kb(row.rgrow)));
    push_help(&mut lines, show_help, HELP, "RGROW");
    lines.push(kv("Code (VSTEXT)", &format_kb(row.vstext)));
    push_help(&mut lines, show_help, HELP, "Code (VSTEXT)");
    lines.push(kv("Data (VDATA)", &format_kb(row.vdata)));
    push_help(&mut lines, show_help, HELP, "Data (VDATA)");
    lines.push(kv("Stack (VSTACK)", &format_kb(row.vstack)));
    push_help(&mut lines, show_help, HELP, "Stack (VSTACK)");
    lines.push(kv("Libraries (VSLIBS)", &format_kb(row.vslibs)));
    push_help(&mut lines, show_help, HELP, "Libraries (VSLIBS)");
    lines.push(kv("Locked (LOCKSZ)", &format_kb(row.vlock)));
    push_help(&mut lines, show_help, HELP, "Locked (LOCKSZ)");
    lines.push(kv("Swap (SWAPSZ)", &format_kb(row.vswap)));
    push_help(&mut lines, show_help, HELP, "Swap (SWAPSZ)");
    lines.push(kv("Minor faults", &row.minflt.to_string()));
    push_help(&mut lines, show_help, HELP, "Minor faults");
    lines.push(kv("Major faults", &row.majflt.to_string()));
    push_help(&mut lines, show_help, HELP, "Major faults");
    lines.push(Line::from(""));

    // Section: Disk I/O (from ProcessInfo only)
    if let Some(p) = info {
        lines.push(section("Disk I/O"));

        // Calculate rates if we have previous data
        if let Some(prev) = prev_info {
            // Read ops/s
            let rio_delta = p.dsk.rio.saturating_sub(prev.dsk.rio);
            let rio_rate = rio_delta as f64 / interval_secs;
            lines.push(kv("Read ops/s", &format_rate(rio_rate)));
            push_help(&mut lines, show_help, HELP, "Read ops/s");

            // Read bytes/s
            let rsz_delta = p.dsk.rsz.saturating_sub(prev.dsk.rsz);
            let rsz_rate = rsz_delta as f64 / interval_secs;
            lines.push(kv("Read bytes/s", &format_bytes_rate(rsz_rate)));
            push_help(&mut lines, show_help, HELP, "Read bytes/s");

            // Write ops/s
            let wio_delta = p.dsk.wio.saturating_sub(prev.dsk.wio);
            let wio_rate = wio_delta as f64 / interval_secs;
            lines.push(kv("Write ops/s", &format_rate(wio_rate)));
            push_help(&mut lines, show_help, HELP, "Write ops/s");

            // Write bytes/s
            let wsz_delta = p.dsk.wsz.saturating_sub(prev.dsk.wsz);
            let wsz_rate = wsz_delta as f64 / interval_secs;
            lines.push(kv("Write bytes/s", &format_bytes_rate(wsz_rate)));
            push_help(&mut lines, show_help, HELP, "Write bytes/s");
        } else {
            // No previous data, show cumulative with note
            lines.push(kv("Read ops", &format!("{} (cumulative)", p.dsk.rio)));
            push_help(&mut lines, show_help, HELP, "Read ops");
            lines.push(kv(
                "Read bytes",
                &format!("{} (cumulative)", format_bytes(p.dsk.rsz)),
            ));
            push_help(&mut lines, show_help, HELP, "Read bytes");
            lines.push(kv("Write ops", &format!("{} (cumulative)", p.dsk.wio)));
            push_help(&mut lines, show_help, HELP, "Write ops");
            lines.push(kv(
                "Write bytes",
                &format!("{} (cumulative)", format_bytes(p.dsk.wsz)),
            ));
            push_help(&mut lines, show_help, HELP, "Write bytes");
        }

        // Always show cumulative totals
        lines.push(Line::from(""));
        lines.push(kv("Total read ops", &p.dsk.rio.to_string()));
        push_help(&mut lines, show_help, HELP, "Total read ops");
        lines.push(kv("Total read bytes", &format_bytes(p.dsk.rsz)));
        push_help(&mut lines, show_help, HELP, "Total read bytes");
        lines.push(kv("Total write ops", &p.dsk.wio.to_string()));
        push_help(&mut lines, show_help, HELP, "Total write ops");
        lines.push(kv("Total write bytes", &format_bytes(p.dsk.wsz)));
        push_help(&mut lines, show_help, HELP, "Total write bytes");

        if p.dsk.cwsz > 0 {
            lines.push(kv("Cancelled writes", &format_bytes(p.dsk.cwsz)));
            push_help(&mut lines, show_help, HELP, "Cancelled writes");
        }
    }

    lines
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

    let secs = btime as i64;
    let days = secs / 86400;
    let time_of_day = secs % 86400;

    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let (year, month, day) = days_to_ymd(days);

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year, month, day, hours, minutes, seconds
    )
}

/// Converts days since Unix epoch to (year, month, day).
fn days_to_ymd(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

/// Truncates command line if too long.
fn truncate_cmdline(cmdline: &str, max_len: usize) -> String {
    if cmdline.len() <= max_len {
        cmdline.to_string()
    } else {
        format!("{}...", &cmdline[..max_len - 3])
    }
}
