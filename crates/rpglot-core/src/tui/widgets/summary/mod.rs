//! Summary widget showing system overview in atop-style format.
//!
//! Displays system metrics in a two-column layout:
//! - Left column: MEM, SWP, DSK, NET (memory/storage/network)
//! - Right column: CPL, CPU, cpu×N (CPU and load)
//!
//! Uses fixed-width metrics with right-aligned values for stable display.

mod extract;
mod render_lines;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::storage::model::{CgroupCpuInfo, CgroupMemoryInfo, CgroupPidsInfo, DataBlock, Snapshot};
use crate::tui::state::Tab;
use crate::tui::style::Styles;

use extract::extract_metrics;
use render_lines::*;

const TOP_CPUS: usize = 5;
/// Number of top disk devices to display.
const TOP_DISKS: usize = 2;
/// Number of top network interfaces to display.
const TOP_NETS: usize = 2;

/// Calculates the required height for the summary panel based on snapshot content.
/// Returns height including help line.
pub fn calculate_summary_height(snapshot: Option<&Snapshot>) -> u16 {
    if let Some(snap) = snapshot {
        let is_container_snapshot = snap
            .blocks
            .iter()
            .any(|b| matches!(b, DataBlock::Cgroup(_)));

        // Count disks (filter same as in extract_top_disks)
        let disk_count = snap
            .blocks
            .iter()
            .filter_map(|b| {
                if let DataBlock::SystemDisk(disks) = b {
                    Some(disks.iter())
                } else {
                    None
                }
            })
            .flatten()
            .filter(|d| {
                // In container snapshots we rely on mountinfo-based IDs.
                // Only disks that were present in mountinfo have non-zero major/minor.
                if is_container_snapshot && d.major == 0 && d.minor == 0 {
                    return false;
                }

                // Skip loop, ram devices and partitions (except nvme)
                !d.device_name.starts_with("loop")
                    && !d.device_name.starts_with("ram")
                    && (is_container_snapshot
                        || d.device_name.starts_with("nvme")
                        || !d
                            .device_name
                            .chars()
                            .last()
                            .is_some_and(|c| c.is_ascii_digit()))
            })
            .count()
            .clamp(1, TOP_DISKS); // At least 1 line for "no disk data"

        // Count network interfaces (filter same as in extract_top_nets)
        let net_count = snap
            .blocks
            .iter()
            .filter_map(|b| {
                if let DataBlock::SystemNet(nets) = b {
                    Some(nets.iter())
                } else {
                    None
                }
            })
            .flatten()
            .filter(|n| {
                if n.name == "lo" {
                    return false;
                }
                if is_container_snapshot {
                    n.name.starts_with("eth") || n.name.starts_with("veth")
                } else {
                    true
                }
            })
            .count()
            .clamp(1, TOP_NETS); // At least 1 line for "no network data"

        // Count CPUs (exclude total CPU with cpu_id == -1)
        let cpu_count = snap
            .blocks
            .iter()
            .filter_map(|b| {
                if let DataBlock::SystemCpu(cpus) = b {
                    Some(cpus.iter())
                } else {
                    None
                }
            })
            .flatten()
            .filter(|c| c.cpu_id != -1)
            .count()
            .min(TOP_CPUS);

        // Detect container-limited cgroup data (works in both live and history mode)
        let (cgroup_mem_limited, cgroup_cpu_limited) = snap
            .blocks
            .iter()
            .find_map(|b| {
                if let DataBlock::Cgroup(cg) = b {
                    let mem_limited = cg.memory.as_ref().is_some_and(|m| m.max != u64::MAX);
                    let cpu_limited = cg.cpu.as_ref().is_some_and(|c| c.quota > 0 && c.period > 0);
                    Some((mem_limited, cpu_limited))
                } else {
                    None
                }
            })
            .unwrap_or((false, false));

        // Left column: MEM(+SWP) + DSK×N + NET×N + PG
        // In container mode with memory limit, SWP is hidden.
        let left_base = if cgroup_mem_limited { 1 } else { 2 };

        let has_pg_database = snap
            .blocks
            .iter()
            .any(|b| matches!(b, DataBlock::PgStatDatabase(_)));
        let has_pg_bgwriter = snap
            .blocks
            .iter()
            .any(|b| matches!(b, DataBlock::PgStatBgwriter(_)));
        let pg_lines =
            (if has_pg_database { 1 } else { 0 }) + (if has_pg_bgwriter { 1 } else { 0 });

        let left_lines = left_base + disk_count + net_count + pg_lines;

        // Right column: CPL + CPU (+ cpu×N) + PSI + VMS
        // In container mode with CPU quota, per-CPU breakdown is not shown.
        let right_cpu_lines = if cgroup_cpu_limited { 0 } else { cpu_count };

        // Check if PSI data is available
        let has_psi = snap
            .blocks
            .iter()
            .any(|b| matches!(b, DataBlock::SystemPsi(psi) if !psi.is_empty()));

        // Check if vmstat data is available (needs previous snapshot for rates)
        let has_vmstat = snap
            .blocks
            .iter()
            .any(|b| matches!(b, DataBlock::SystemVmstat(_)));

        let psi_lines = if has_psi { 1 } else { 0 };
        let vmstat_lines = if has_vmstat { 1 } else { 0 };
        let right_lines = 2 + right_cpu_lines + psi_lines + vmstat_lines;

        // Max of both columns + help line
        (left_lines.max(right_lines) + 1) as u16
    } else {
        // Waiting for data: 2 lines (message + help)
        2
    }
}

/// Renders the summary panel with two-column layout.
/// Left column: MEM, SWP, DSK, NET | Right column: CPL, CPU, cpu×N
pub fn render_summary(
    frame: &mut Frame,
    area: Rect,
    snapshot: Option<&Snapshot>,
    previous_snapshot: Option<&Snapshot>,
    current_tab: Tab,
) {
    if let Some(snap) = snapshot {
        let metrics = extract_metrics(snap, previous_snapshot);

        // Calculate content widths
        let left_width = calculate_left_column_width();
        let right_width = calculate_right_column_width();

        // Split area: left column | separator | right column | help at bottom
        let main_chunks = Layout::vertical([
            Constraint::Min(1),    // Main content
            Constraint::Length(1), // Help line
        ])
        .split(area);

        // Calculate actual column widths with separator
        let separator_width = 3; // " │ "
        let total_content_width = left_width + separator_width + right_width;
        let available_width = main_chunks[0].width as usize;

        // Adjust proportions if needed
        let (actual_left, actual_right) = if total_content_width <= available_width {
            (left_width, available_width - left_width - separator_width)
        } else {
            let ratio = available_width as f64 / total_content_width as f64;
            let adj_left = (left_width as f64 * ratio) as usize;
            let adj_right = available_width.saturating_sub(adj_left + separator_width);
            (adj_left.max(40), adj_right.max(40))
        };

        let columns = Layout::horizontal([
            Constraint::Length(actual_left as u16),
            Constraint::Length(separator_width as u16),
            Constraint::Min(actual_right as u16),
        ])
        .split(main_chunks[0]);

        // Build left column lines (MEM, SWP, DSK, NET)
        let left_lines = build_left_column(&metrics, actual_left);

        // Build right column lines (CPL, CPU, cpu×N)
        let right_lines = build_right_column(&metrics, actual_right);

        // Build separator lines (match max height)
        let max_lines = left_lines.len().max(right_lines.len());
        let separator_lines: Vec<Line> = (0..max_lines)
            .map(|_| Line::from(Span::styled(" │ ", Styles::dim())))
            .collect();

        // Pad shorter column
        let mut left_padded = left_lines;
        let mut right_padded = right_lines;
        while left_padded.len() < max_lines {
            left_padded.push(Line::from(" ".repeat(actual_left)));
        }
        while right_padded.len() < max_lines {
            right_padded.push(Line::from(" ".repeat(actual_right)));
        }

        // Render columns
        frame.render_widget(Paragraph::new(left_padded), columns[0]);
        frame.render_widget(Paragraph::new(separator_lines), columns[1]);
        frame.render_widget(Paragraph::new(right_padded), columns[2]);

        // Help line
        let help_line = render_help_line(area.width as usize, current_tab);
        frame.render_widget(Paragraph::new(vec![help_line]), main_chunks[1]);
    } else {
        let lines = vec![
            Line::from("Waiting for data..."),
            render_help_line(area.width as usize, current_tab),
        ];
        frame.render_widget(Paragraph::new(lines), area);
    }
}

/// Calculate minimum width for left column (MEM, SWP, DSK, NET).
fn calculate_left_column_width() -> usize {
    // MEM line is widest: "MEM │ " + metrics
    // MEM │ tot:  15.7 GiB  avail:   13.0 GiB  cache:    1.3 GiB  buf:    92 KiB  slab: 742 MiB
    6 + MEM_TOT + 2 + MEM_AVAIL + 2 + MEM_CACHE + 2 + MEM_BUF + 2 + MEM_SLAB
}

/// Calculate minimum width for right column (CPL, CPU, cpu×N).
fn calculate_right_column_width() -> usize {
    // CPU line: "CPU │ " + metrics + cpu number
    6 + CPU_PCT * 4 + IDLE + STL + CPUNUM + 10 // spacing
}

/// Build left column lines: MEM, SWP, DSK×N, NET×N.
fn build_left_column(metrics: &SummaryMetrics, width: usize) -> Vec<Line<'static>> {
    let mut lines = if metrics
        .cgroup_memory
        .as_ref()
        .is_some_and(|m| m.max != u64::MAX)
    {
        vec![render_cgroup_mem_line(metrics, width)]
    } else {
        vec![
            render_mem_line(metrics, width),
            render_swp_line(metrics, width),
        ]
    };

    // Add separate line for each disk
    if metrics.top_disks.is_empty() {
        lines.push(render_dsk_empty_line(width));
    } else {
        for (i, disk) in metrics.top_disks.iter().enumerate() {
            lines.push(render_single_dsk_line(disk, i == 0, width));
        }
    }

    // Add separate line for each network interface
    if metrics.top_nets.is_empty() {
        lines.push(render_net_empty_line(width));
    } else {
        for (i, net) in metrics.top_nets.iter().enumerate() {
            lines.push(render_single_net_line(net, i == 0, width));
        }
    }

    // Add PostgreSQL summary line if data is available
    if let Some(ref pg) = metrics.pg_summary {
        lines.push(render_pg_line(pg, width));
    }

    // Add PostgreSQL bgwriter summary line if data is available
    if let Some(ref bgw) = metrics.bgw_summary {
        lines.push(render_bgw_line(bgw, width));
    }

    lines
}

/// Build right column lines: CPL, CPU, cpu×N, PSI, VMS.
fn build_right_column(metrics: &SummaryMetrics, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(render_cpl_line(metrics, width));

    if metrics
        .cgroup_cpu
        .as_ref()
        .is_some_and(|c| c.quota > 0 && c.period > 0)
    {
        lines.push(render_cgroup_cpu_line(metrics, width));
    } else {
        lines.push(render_cpu_line(&metrics.cpu_total, true, -1, width));
        for cpu in &metrics.top_cpus {
            lines.push(render_cpu_line(cpu, false, cpu.cpu_id, width));
        }
    }

    // Add PSI line if data is available
    if !metrics.psi.is_empty() {
        lines.push(render_psi_line(&metrics.psi, width));
    }

    // Add vmstat rates line if data is available
    if let Some(ref rates) = metrics.vmstat_rates {
        lines.push(render_vmstat_line(rates, width));
    }

    lines
}

/// Extracted metrics for rendering.
struct SummaryMetrics {
    // Load
    load1: f64,
    load5: f64,
    load15: f64,
    nr_procs: u32,
    nr_running: u32,

    // Delta time between current and previous snapshot (seconds).
    delta_time: f64,

    // CPU
    num_cpus: usize,
    cpu_total: CpuMetrics,
    top_cpus: Vec<CpuMetrics>,

    // Memory (in KB)
    mem_total: u64,
    mem_available: u64,
    mem_cached: u64,
    mem_buffers: u64,
    mem_slab: u64,
    mem_dirty: u64,
    mem_writeback: u64,

    // Swap (in KB)
    swap_total: u64,
    swap_free: u64,

    // Cgroup v2 (container) - present when snapshot contains DataBlock::Cgroup
    cgroup_cpu: Option<CgroupCpuInfo>,
    cgroup_cpu_prev: Option<CgroupCpuInfo>,
    cgroup_memory: Option<CgroupMemoryInfo>,
    cgroup_pids: Option<CgroupPidsInfo>,

    // Disk (top by util)
    top_disks: Vec<DiskSummary>,

    // Network (top by throughput)
    top_nets: Vec<NetSummary>,

    // PSI (Pressure Stall Information)
    psi: Vec<PsiSummary>,

    // Vmstat rates (from current and previous vmstat/stat)
    vmstat_rates: Option<VmstatRates>,

    // PostgreSQL instance-level summary (from pg_stat_database)
    pg_summary: Option<PgSummary>,

    // PostgreSQL bgwriter summary (from pg_stat_bgwriter)
    bgw_summary: Option<BgwSummary>,
}

/// CPU metrics for a single CPU or total.
#[derive(Clone, Default)]
struct CpuMetrics {
    cpu_id: i16,
    sys: f64,
    usr: f64,
    irq: f64,
    iow: f64,
    idle: f64,
    steal: f64,
}

/// Disk summary for display (extended metrics).
#[derive(Clone)]
struct DiskSummary {
    name: String,
    read_mb_s: f64,
    write_mb_s: f64,
    r_iops: f64,
    w_iops: f64,
    r_await: f64,
    w_await: f64,
    util: f64,
}

/// Network summary for display (extended metrics).
#[derive(Clone)]
struct NetSummary {
    name: String,
    rx_mb_s: f64,
    tx_mb_s: f64,
    rx_pkt_s: f64,
    tx_pkt_s: f64,
    rx_drp_s: f64,
    tx_drp_s: f64,
    errors: u64,
}

/// PSI summary for display.
#[derive(Clone, Default)]
struct PsiSummary {
    /// Resource name (CPU, MEM, I/O)
    name: &'static str,
    /// some avg10 (% of time at least one task was stalled)
    some: f32,
    /// full avg10 (% of time ALL tasks were stalled)
    #[allow(dead_code)]
    full: f32,
}

/// Vmstat rates for display.
#[derive(Clone, Default)]
struct VmstatRates {
    /// Pages read from disk per second
    pgpgin_s: f64,
    /// Pages written to disk per second
    pgpgout_s: f64,
    /// Pages swapped in per second
    pswpin_s: f64,
    /// Pages swapped out per second
    pswpout_s: f64,
    /// Page faults per second
    pgfault_s: f64,
    /// Context switches per second
    ctxt_s: f64,
}

/// PostgreSQL instance-level summary from pg_stat_database.
/// Rates are computed from deltas between consecutive snapshots.
#[derive(Clone, Default)]
struct PgSummary {
    /// Transactions per second (commit + rollback).
    tps: f64,
    /// Buffer cache hit ratio (0..100).
    hit_ratio: f64,
    /// Tuples per second (returned + fetched + inserted + updated + deleted).
    tup_s: f64,
    /// Temp bytes per second.
    tmp_bytes_s: f64,
    /// Deadlocks in this interval.
    deadlocks: i64,
    /// Block read time delta (ms).
    #[allow(dead_code)]
    blk_read_time_ms: f64,
    /// Block write time delta (ms).
    #[allow(dead_code)]
    blk_write_time_ms: f64,
    /// Rollbacks in this interval.
    #[allow(dead_code)]
    rollbacks: i64,
}

/// PostgreSQL background writer summary (rates from pg_stat_bgwriter).
#[derive(Clone, Default)]
struct BgwSummary {
    /// Checkpoints per minute (timed + requested).
    checkpoints_per_min: f64,
    /// Checkpoint write time in ms during this interval.
    ckpt_write_time_ms: f64,
    /// Buffers written by backends per second.
    buffers_backend_s: f64,
    /// Buffers cleaned by bgwriter per second.
    buffers_clean_s: f64,
    /// Times bgwriter hit maxwritten_clean limit in this interval.
    maxwritten_clean: i64,
    /// Buffers allocated per second.
    buffers_alloc_s: f64,
}

// ============================================================================
// Fixed-width metric formatting (Variant 2: right-aligned values)
// Each metric has a fixed width: "label:" + right-aligned value
// ============================================================================

/// Fixed widths for metrics (calculated from common sense and typical data)
#[allow(dead_code)]
pub(super) mod metric_widths {
    // CPL line: avg1:  0.17  avg5:  0.24  avg15:  0.26  procs:  786  run:   1
    pub const AVG: usize = 12; // "avg1:" (5) + value (7) = "avg1:   0.17"
    pub const AVG15: usize = 13; // "avg15:" (6) + value (7) = "avg15:   0.26"
    pub const PROCS: usize = 12; // "procs:" (6) + value (6) = "procs:   786"
    pub const RUN: usize = 8; // "run:" (4) + value (4) = "run:   1"

    // CPU line: sys:  0.2%  usr:  0.4%  irq:  0.1%  iow:  0.0%  idle: 99.3%  stl:  0.0%
    pub const CPU_PCT: usize = 11; // "sys:" (4) + value (7) = "sys:   0.2%"
    pub const IDLE: usize = 12; // "idle:" (5) + value (7) = "idle:  99.3%"
    pub const STL: usize = 11; // "stl:" (4) + value (7) = "stl:   0.0%"
    pub const CPUNUM: usize = 8; // "cpu:" (4) + value (4) = "cpu:   0"

    // MEM/SWP line: tot: 15.7 GiB  avail: 13.0 GiB  cache:  1.3 GiB
    pub const MEM_TOT: usize = 15; // "tot:" (4) + value (11) = "tot:  15.7 GiB"
    pub const MEM_AVAIL: usize = 18; // "avail:" (6) + value (12) = "avail:  13.0 GiB"
    pub const MEM_CACHE: usize = 17; // "cache:" (6) + value (11) = "cache:   1.3 GiB"
    pub const MEM_BUF: usize = 15; // "buf:" (4) + value (11) = "buf:    92 KiB"
    pub const MEM_SLAB: usize = 15; // "slab:" (5) + value (10) = "slab: 742 MiB"
    pub const SWP_FREE: usize = 17; // "free:" (5) + value (12) = "free:  16.7 GiB"
    pub const SWP_SWPD: usize = 16; // "swpd:" (5) + value (11) = "swpd:    0 KiB"
    pub const SWP_DIRTY: usize = 16; // "dirty:" (6) + value (10) = "dirty:  60 KiB"
    pub const SWP_WBACK: usize = 17; // "wback:" (6) + value (11) = "wback:   0 KiB"

    // Cgroup (container) summary extras
    pub const CG_OOM: usize = 8; // "oom:" (4) + value
    pub const CG_LIM: usize = 8; // "lim:" + value
    pub const CG_THR: usize = 12; // "thrtl:" + value (e.g. "234ms")
    pub const CG_NR: usize = 8; // "nr:" + value

    // Unified DSK/NET format with aligned columns:
    // DSK │ vda:   rMB:   4.0  wMB:   1.0  rd/s:   396  wr/s:    74  aw:   0.2  ut:   26%
    // NET │ ens2:  rxMB:  0.2  txMB:  0.5  rxPk:    1K  txPk:  833  er:     0  dr:     0
    pub const DEV_NAME: usize = 5; // device/interface name max width
    pub const IO_MB: usize = 10; // "rMB:/rxMB:" + value = 10 chars
    pub const IO_OPS: usize = 11; // "rd/s:/rxPk:" + value = 11 chars
    pub const IO_STAT: usize = 9; // "aw:/er:" + value = 9 chars

    // Legacy constants for backward compatibility (deprecated)
    pub const DSK_NAME: usize = DEV_NAME;
    pub const DSK_MB: usize = IO_MB;
    pub const DSK_IOS: usize = IO_OPS;
    pub const DSK_AW: usize = IO_STAT;
    pub const DSK_UT: usize = IO_STAT;
    pub const NET_NAME: usize = DEV_NAME;
    pub const NET_MB: usize = IO_MB;
    pub const NET_PKT: usize = IO_OPS;
    pub const NET_ERR: usize = IO_STAT;
    pub const NET_DRP: usize = IO_STAT;

    // PG line: tps:   1234  hit:  99.2%  tup:   5.6K/s  tmp:   0 B/s  dlock:     0
    pub const PG_TPS: usize = 12; // "tps:" + value
    pub const PG_HIT: usize = 12; // "hit:" + value
    pub const PG_TUP: usize = 14; // "tup:" + value
    pub const PG_TMP: usize = 14; // "tmp:" + value
    pub const PG_DLOCK: usize = 11; // "dlock:" + value

    // BGW line: ckpt: 0.1/m  wr: 125ms  be:  45/s  cln: 1.2K/s  mxw:     0  alloc: 5.6K/s
    pub const BGW_CKPT: usize = 12; // "ckpt:" + value
    pub const BGW_WR: usize = 11; // "wr:" + value
    pub const BGW_BE: usize = 11; // "be:" + value
    pub const BGW_CLN: usize = 13; // "cln:" + value
    pub const BGW_MXW: usize = 10; // "mxw:" + value
    pub const BGW_ALLOC: usize = 15; // "alloc:" + value
}

use metric_widths::*;
