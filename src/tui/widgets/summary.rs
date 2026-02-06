//! Summary widget showing system overview in atop-style format.
//!
//! Displays system metrics in a two-column layout:
//! - Left column: MEM, SWP, DSK, NET (memory/storage/network)
//! - Right column: CPL, CPU, cpu×N (CPU and load)
//!
//! Uses fixed-width metrics with right-aligned values for stable display.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::storage::model::{
    CgroupCpuInfo, CgroupMemoryInfo, CgroupPidsInfo, DataBlock, Snapshot, SystemCpuInfo,
    SystemDiskInfo, SystemNetInfo, SystemPsiInfo, SystemStatInfo, SystemVmstatInfo,
};
use crate::tui::state::Tab;
use crate::tui::style::Styles;

/// Number of per-CPU cores to display (top by usage).
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

        // Left column: MEM(+SWP) + DSK×N + NET×N
        // In container mode with memory limit, SWP is hidden.
        let left_base = if cgroup_mem_limited { 1 } else { 2 };
        let left_lines = left_base + disk_count + net_count;

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

/// Extracts all metrics from snapshot.
fn extract_metrics(snapshot: &Snapshot, previous: Option<&Snapshot>) -> SummaryMetrics {
    let delta_time = get_delta_time(snapshot, previous);
    let cgroup_cpu_prev = previous.and_then(|p| {
        p.blocks.iter().find_map(|b| {
            if let DataBlock::Cgroup(cg) = b {
                cg.cpu.clone()
            } else {
                None
            }
        })
    });

    let mut metrics = SummaryMetrics {
        load1: 0.0,
        load5: 0.0,
        load15: 0.0,
        nr_procs: 0,
        nr_running: 0,
        delta_time,
        num_cpus: 0,
        cpu_total: CpuMetrics::default(),
        top_cpus: Vec::new(),
        mem_total: 0,
        mem_available: 0,
        mem_cached: 0,
        mem_buffers: 0,
        mem_slab: 0,
        mem_dirty: 0,
        mem_writeback: 0,
        swap_total: 0,
        swap_free: 0,
        cgroup_cpu: None,
        cgroup_cpu_prev,
        cgroup_memory: None,
        cgroup_pids: None,
        top_disks: Vec::new(),
        top_nets: Vec::new(),
        psi: Vec::new(),
        vmstat_rates: None,
    };

    // We compute top disks after the loop because for containers the filtering
    // depends on `DataBlock::Cgroup` (which comes after `SystemDisk` in snapshot).
    let mut current_disks: Option<&[SystemDiskInfo]> = None;
    let mut prev_disks: Option<&[SystemDiskInfo]> = None;

    // Same idea for network interfaces: container filtering depends on cgroup presence.
    let mut current_nets: Option<&[SystemNetInfo]> = None;
    let mut prev_nets: Option<&[SystemNetInfo]> = None;

    for block in &snapshot.blocks {
        match block {
            DataBlock::SystemLoad(load) => {
                metrics.load1 = load.lavg1 as f64;
                metrics.load5 = load.lavg5 as f64;
                metrics.load15 = load.lavg15 as f64;
                metrics.nr_running = load.nr_running;
                metrics.nr_procs = load.nr_threads;
            }
            DataBlock::SystemCpu(cpus) => {
                extract_cpu_metrics(cpus, &mut metrics);
            }
            DataBlock::SystemMem(mem) => {
                metrics.mem_total = mem.total;
                metrics.mem_available = mem.available;
                metrics.mem_cached = mem.cached;
                metrics.mem_buffers = mem.buffers;
                metrics.mem_slab = mem.slab;
                metrics.mem_dirty = mem.dirty;
                metrics.mem_writeback = mem.writeback;
                metrics.swap_total = mem.swap_total;
                metrics.swap_free = mem.swap_free;
            }
            DataBlock::SystemDisk(disks) => {
                prev_disks = previous.and_then(|p| {
                    p.blocks.iter().find_map(|b| {
                        if let DataBlock::SystemDisk(d) = b {
                            Some(d.as_slice())
                        } else {
                            None
                        }
                    })
                });
                current_disks = Some(disks.as_slice());
            }
            DataBlock::SystemNet(nets) => {
                prev_nets = previous.and_then(|p| {
                    p.blocks.iter().find_map(|b| {
                        if let DataBlock::SystemNet(n) = b {
                            Some(n.as_slice())
                        } else {
                            None
                        }
                    })
                });
                current_nets = Some(nets.as_slice());
            }
            DataBlock::Cgroup(cg) => {
                metrics.cgroup_cpu = cg.cpu.clone();
                metrics.cgroup_memory = cg.memory.clone();
                metrics.cgroup_pids = cg.pids.clone();
            }
            DataBlock::SystemPsi(psi_list) => {
                metrics.psi = extract_psi(psi_list);
            }
            DataBlock::SystemVmstat(vmstat) => {
                let prev_vmstat = previous.and_then(|p| {
                    p.blocks.iter().find_map(|b| {
                        if let DataBlock::SystemVmstat(v) = b {
                            Some(v)
                        } else {
                            None
                        }
                    })
                });
                let prev_stat = previous.and_then(|p| {
                    p.blocks.iter().find_map(|b| {
                        if let DataBlock::SystemStat(s) = b {
                            Some(s)
                        } else {
                            None
                        }
                    })
                });
                let curr_stat = snapshot.blocks.iter().find_map(|b| {
                    if let DataBlock::SystemStat(s) = b {
                        Some(s)
                    } else {
                        None
                    }
                });
                metrics.vmstat_rates =
                    extract_vmstat_rates(vmstat, prev_vmstat, curr_stat, prev_stat, delta_time);
            }
            _ => {}
        }
    }

    let is_container_snapshot = snapshot
        .blocks
        .iter()
        .any(|b| matches!(b, DataBlock::Cgroup(_)));

    if let Some(disks) = current_disks {
        metrics.top_disks =
            extract_top_disks(disks, prev_disks, metrics.delta_time, is_container_snapshot);
    }

    if let Some(nets) = current_nets {
        metrics.top_nets =
            extract_top_nets(nets, prev_nets, metrics.delta_time, is_container_snapshot);
    }

    metrics
}

/// Extracts CPU metrics from SystemCpu data.
fn extract_cpu_metrics(cpus: &[SystemCpuInfo], metrics: &mut SummaryMetrics) {
    let mut per_cpu: Vec<CpuMetrics> = Vec::new();

    for cpu in cpus {
        let total = cpu.user
            + cpu.nice
            + cpu.system
            + cpu.idle
            + cpu.iowait
            + cpu.irq
            + cpu.softirq
            + cpu.steal;
        if total == 0 {
            continue;
        }

        let cpu_metrics = CpuMetrics {
            cpu_id: cpu.cpu_id,
            sys: (cpu.system as f64 / total as f64) * 100.0,
            usr: ((cpu.user + cpu.nice) as f64 / total as f64) * 100.0,
            irq: ((cpu.irq + cpu.softirq) as f64 / total as f64) * 100.0,
            iow: (cpu.iowait as f64 / total as f64) * 100.0,
            idle: (cpu.idle as f64 / total as f64) * 100.0,
            steal: (cpu.steal as f64 / total as f64) * 100.0,
        };

        if cpu.cpu_id == -1 {
            metrics.cpu_total = cpu_metrics;
        } else {
            per_cpu.push(cpu_metrics);
        }
    }

    // Count CPUs (excluding total)
    metrics.num_cpus = per_cpu.len();

    // Sort by usage (100 - idle) descending, take top N
    per_cpu.sort_by(|a, b| {
        let usage_a = 100.0 - a.idle;
        let usage_b = 100.0 - b.idle;
        usage_b
            .partial_cmp(&usage_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    metrics.top_cpus = per_cpu.into_iter().take(TOP_CPUS).collect();
}

/// Gets delta time between snapshots in seconds.
fn get_delta_time(current: &Snapshot, previous: Option<&Snapshot>) -> f64 {
    if let Some(prev) = previous {
        let delta = current.timestamp - prev.timestamp;
        if delta > 0 {
            return delta as f64;
        }
    }
    1.0 // Default to 1 second if no previous
}

/// Extracts top disks by utilization with extended metrics.
fn extract_top_disks(
    disks: &[SystemDiskInfo],
    prev_disks: Option<&[SystemDiskInfo]>,
    delta_time: f64,
    is_container_snapshot: bool,
) -> Vec<DiskSummary> {
    use std::collections::HashMap;

    let prev_map: HashMap<u64, &SystemDiskInfo> = prev_disks
        .map(|p| p.iter().map(|d| (d.device_hash, d)).collect())
        .unwrap_or_default();

    let mut summaries: Vec<DiskSummary> = disks
        .iter()
        .filter_map(|disk| {
            // In container mode we rely on mountinfo-based IDs.
            // Only devices present in mountinfo have non-zero major/minor.
            if is_container_snapshot && disk.major == 0 && disk.minor == 0 {
                return None;
            }

            // Skip loop/ram devices.
            if disk.device_name.starts_with("loop") || disk.device_name.starts_with("ram") {
                return None;
            }

            // Skip partitions (devices with numbers at end like sda1)
            if !is_container_snapshot
                && disk
                    .device_name
                    .chars()
                    .last()
                    .is_some_and(|c| c.is_ascii_digit())
                && !disk.device_name.starts_with("nvme")
            {
                return None;
            }

            let prev = prev_map.get(&disk.device_hash);

            if let Some(p) = prev {
                let read_sectors = disk.rsz.saturating_sub(p.rsz);
                let write_sectors = disk.wsz.saturating_sub(p.wsz);
                let io_ms = disk.io_ms.saturating_sub(p.io_ms);
                let r_ios = disk.rio.saturating_sub(p.rio);
                let w_ios = disk.wio.saturating_sub(p.wio);
                let r_time = disk.read_time.saturating_sub(p.read_time);
                let w_time = disk.write_time.saturating_sub(p.write_time);

                // Sectors are typically 512 bytes
                let read_mb_s = (read_sectors as f64 * 512.0) / (delta_time * 1024.0 * 1024.0);
                let write_mb_s = (write_sectors as f64 * 512.0) / (delta_time * 1024.0 * 1024.0);
                let r_iops = r_ios as f64 / delta_time;
                let w_iops = w_ios as f64 / delta_time;
                // await = time / operations (in ms)
                let r_await = if r_ios > 0 {
                    r_time as f64 / r_ios as f64
                } else {
                    0.0
                };
                let w_await = if w_ios > 0 {
                    w_time as f64 / w_ios as f64
                } else {
                    0.0
                };
                // io_ms is in ms, delta_time is in seconds
                let util = (io_ms as f64 / (delta_time * 1000.0)) * 100.0;

                // Sanitize values - cap at realistic maximums to handle data issues
                Some(DiskSummary {
                    name: disk.device_name.clone(),
                    read_mb_s: if read_mb_s > MAX_DISK_MB_S {
                        0.0
                    } else {
                        read_mb_s
                    },
                    write_mb_s: if write_mb_s > MAX_DISK_MB_S {
                        0.0
                    } else {
                        write_mb_s
                    },
                    r_iops: if r_iops > MAX_DISK_IOPS { 0.0 } else { r_iops },
                    w_iops: if w_iops > MAX_DISK_IOPS { 0.0 } else { w_iops },
                    r_await,
                    w_await,
                    util: util.min(100.0),
                })
            } else {
                Some(DiskSummary {
                    name: disk.device_name.clone(),
                    read_mb_s: 0.0,
                    write_mb_s: 0.0,
                    r_iops: 0.0,
                    w_iops: 0.0,
                    r_await: 0.0,
                    w_await: 0.0,
                    util: 0.0,
                })
            }
        })
        .collect();

    // Sort by util descending
    summaries.sort_by(|a, b| {
        b.util
            .partial_cmp(&a.util)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    summaries.into_iter().take(TOP_DISKS).collect()
}

/// Extracts top network interfaces by throughput with extended metrics.
fn extract_top_nets(
    nets: &[SystemNetInfo],
    prev_nets: Option<&[SystemNetInfo]>,
    delta_time: f64,
    is_container_snapshot: bool,
) -> Vec<NetSummary> {
    use std::collections::HashMap;

    let prev_map: HashMap<u64, &SystemNetInfo> = prev_nets
        .map(|p| p.iter().map(|n| (n.name_hash, n)).collect())
        .unwrap_or_default();

    let mut summaries: Vec<NetSummary> = nets
        .iter()
        .filter(|net| {
            if net.name == "lo" {
                return false;
            }
            if is_container_snapshot {
                net.name.starts_with("eth") || net.name.starts_with("veth")
            } else {
                true
            }
        })
        .map(|net| {
            let prev = prev_map.get(&net.name_hash);

            if let Some(p) = prev {
                let rx_bytes = net.rx_bytes.saturating_sub(p.rx_bytes);
                let tx_bytes = net.tx_bytes.saturating_sub(p.tx_bytes);
                let rx_pkts = net.rx_packets.saturating_sub(p.rx_packets);
                let tx_pkts = net.tx_packets.saturating_sub(p.tx_packets);
                let rx_errs = net.rx_errs.saturating_sub(p.rx_errs);
                let tx_errs = net.tx_errs.saturating_sub(p.tx_errs);
                let rx_drop = net.rx_drop.saturating_sub(p.rx_drop);
                let tx_drop = net.tx_drop.saturating_sub(p.tx_drop);

                let rx_mb_s = (rx_bytes as f64) / (delta_time * 1024.0 * 1024.0);
                let tx_mb_s = (tx_bytes as f64) / (delta_time * 1024.0 * 1024.0);
                // Sanitize values - cap at realistic maximums
                NetSummary {
                    name: net.name.clone(),
                    rx_mb_s: if rx_mb_s > MAX_NET_MB_S { 0.0 } else { rx_mb_s },
                    tx_mb_s: if tx_mb_s > MAX_NET_MB_S { 0.0 } else { tx_mb_s },
                    rx_pkt_s: rx_pkts as f64 / delta_time,
                    tx_pkt_s: tx_pkts as f64 / delta_time,
                    rx_drp_s: rx_drop as f64 / delta_time,
                    tx_drp_s: tx_drop as f64 / delta_time,
                    errors: rx_errs + tx_errs,
                }
            } else {
                NetSummary {
                    name: net.name.clone(),
                    rx_mb_s: 0.0,
                    tx_mb_s: 0.0,
                    rx_pkt_s: 0.0,
                    tx_pkt_s: 0.0,
                    rx_drp_s: 0.0,
                    tx_drp_s: 0.0,
                    errors: 0,
                }
            }
        })
        .collect();

    // Sort by total throughput descending
    summaries.sort_by(|a, b| {
        let total_a = a.rx_mb_s + a.tx_mb_s;
        let total_b = b.rx_mb_s + b.tx_mb_s;
        total_b
            .partial_cmp(&total_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    summaries.into_iter().take(TOP_NETS).collect()
}

/// Extracts PSI summaries from SystemPsiInfo list.
fn extract_psi(psi_list: &[SystemPsiInfo]) -> Vec<PsiSummary> {
    psi_list
        .iter()
        .map(|psi| {
            let name = match psi.resource {
                0 => "CPU",
                1 => "MEM",
                2 => "I/O",
                _ => "???",
            };
            PsiSummary {
                name,
                some: psi.some_avg10,
                full: psi.full_avg10,
            }
        })
        .collect()
}

/// Extracts vmstat rates from current and previous snapshots.
fn extract_vmstat_rates(
    curr: &SystemVmstatInfo,
    prev: Option<&SystemVmstatInfo>,
    curr_stat: Option<&SystemStatInfo>,
    prev_stat: Option<&SystemStatInfo>,
    delta_time: f64,
) -> Option<VmstatRates> {
    if delta_time <= 0.0 {
        return None;
    }

    let prev = prev?;

    let pgpgin_s = curr.pgpgin.saturating_sub(prev.pgpgin) as f64 / delta_time;
    let pgpgout_s = curr.pgpgout.saturating_sub(prev.pgpgout) as f64 / delta_time;
    let pswpin_s = curr.pswpin.saturating_sub(prev.pswpin) as f64 / delta_time;
    let pswpout_s = curr.pswpout.saturating_sub(prev.pswpout) as f64 / delta_time;
    let pgfault_s = curr.pgfault.saturating_sub(prev.pgfault) as f64 / delta_time;

    // Context switches from SystemStat
    let ctxt_s = match (curr_stat, prev_stat) {
        (Some(c), Some(p)) => c.ctxt.saturating_sub(p.ctxt) as f64 / delta_time,
        _ => 0.0,
    };

    Some(VmstatRates {
        pgpgin_s,
        pgpgout_s,
        pswpin_s,
        pswpout_s,
        pgfault_s,
        ctxt_s,
    })
}

// ============================================================================
// Rendering functions with compact layout (no gaps between content)
// ============================================================================

/// Creates a line from spans with trailing padding to fill width.
fn line_with_padding(spans: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let content_len: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let mut result = spans;
    if content_len < width {
        result.push(Span::raw(" ".repeat(width - content_len)));
    }
    Line::from(result)
}

// ============================================================================
// Fixed-width metric formatting (Variant 2: right-aligned values)
// Each metric has a fixed width: "label:" + right-aligned value
// ============================================================================

/// Fixed widths for metrics (calculated from common sense and typical data)
#[allow(dead_code)]
mod metric_widths {
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
}

use metric_widths::*;

/// Maximum realistic disk throughput (10 GB/s) - values above this indicate data issues
const MAX_DISK_MB_S: f64 = 10000.0;
/// Maximum realistic disk IOPS (1M) - values above this indicate data issues
const MAX_DISK_IOPS: f64 = 1_000_000.0;
/// Maximum realistic network throughput (100 Gbps = 12500 MB/s)
const MAX_NET_MB_S: f64 = 12500.0;

/// Creates spans for a labeled metric: "label:" (dim) + right-aligned styled value
fn metric_spans(label: &str, value: &str, total_width: usize, style: Style) -> Vec<Span<'static>> {
    let label_with_colon = format!("{}:", label);
    let value_width = total_width.saturating_sub(label_with_colon.len());
    let formatted_value = format!("{:>width$}", value, width = value_width);
    vec![
        Span::raw(label_with_colon),
        Span::styled(formatted_value, style),
    ]
}

/// Creates spans for a labeled metric with default style
fn metric_spans_default(label: &str, value: &str, total_width: usize) -> Vec<Span<'static>> {
    metric_spans(label, value, total_width, Styles::default())
}

/// Renders CPL (load) line - fixed-width metrics.
fn render_cpl_line(metrics: &SummaryMetrics, width: usize) -> Line<'static> {
    let num_cpus = metrics.num_cpus.max(1) as f64;

    let mut spans = vec![Span::styled("CPL", Styles::dim()), Span::raw(" │ ")];

    spans.extend(metric_spans(
        "avg1",
        &format!("{:.2}", metrics.load1),
        AVG,
        style_for_load(metrics.load1, num_cpus),
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "avg5",
        &format!("{:.2}", metrics.load5),
        AVG,
        style_for_load(metrics.load5, num_cpus),
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "avg15",
        &format!("{:.2}", metrics.load15),
        AVG15,
        style_for_load(metrics.load15, num_cpus),
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "procs",
        &format!("{}", metrics.nr_procs),
        PROCS,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "run",
        &format!("{}", metrics.nr_running),
        RUN,
        style_for_running(metrics.nr_running, num_cpus),
    ));

    line_with_padding(spans, width)
}

/// Renders CPU line (total or per-core) - fixed-width metrics.
fn render_cpu_line(cpu: &CpuMetrics, is_total: bool, cpu_id: i16, width: usize) -> Line<'static> {
    let label = if is_total {
        Span::styled("CPU", Styles::cpu())
    } else {
        Span::styled("cpu", Styles::dim())
    };

    let mut spans = vec![label, Span::raw(" │ ")];

    spans.extend(metric_spans(
        "sys",
        &format!("{:.1}%", cpu.sys),
        CPU_PCT,
        style_for_cpu_sys(cpu.sys),
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "usr",
        &format!("{:.1}%", cpu.usr),
        CPU_PCT,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "irq",
        &format!("{:.1}%", cpu.irq),
        CPU_PCT,
        style_for_cpu_irq(cpu.irq),
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "iow",
        &format!("{:.1}%", cpu.iow),
        CPU_PCT,
        style_for_cpu_iow(cpu.iow),
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "idle",
        &format!("{:.1}%", cpu.idle),
        IDLE,
        style_for_cpu_idle(cpu.idle),
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "stl",
        &format!("{:.1}%", cpu.steal),
        STL,
        style_for_cpu_steal(cpu.steal),
    ));

    if !is_total {
        spans.push(Span::raw("  "));
        spans.extend(metric_spans(
            "cpu",
            &format!("{}", cpu_id),
            CPUNUM,
            Styles::dim(),
        ));
    }

    line_with_padding(spans, width)
}

/// Renders container CPU line using cgroup v2 metrics.
///
/// Displayed only when `cpu.max` sets a quota (`quota > 0`).
fn render_cgroup_cpu_line(metrics: &SummaryMetrics, width: usize) -> Line<'static> {
    let Some(curr) = metrics.cgroup_cpu.as_ref() else {
        return render_cpu_line(&metrics.cpu_total, true, -1, width);
    };

    // If we got here, quota/period should be valid, but keep it defensive.
    let limit_cores = if curr.quota > 0 && curr.period > 0 {
        curr.quota as f64 / curr.period as f64
    } else {
        0.0
    };

    let prev = metrics.cgroup_cpu_prev.as_ref().unwrap_or(curr);
    let delta_usage = curr.usage_usec.saturating_sub(prev.usage_usec);
    let delta_user = curr.user_usec.saturating_sub(prev.user_usec);
    let delta_system = curr.system_usec.saturating_sub(prev.system_usec);
    let delta_throttled_ms = curr.throttled_usec.saturating_sub(prev.throttled_usec) / 1000;
    let delta_nr_throttled = curr.nr_throttled.saturating_sub(prev.nr_throttled);

    let used_pct = if metrics.delta_time > 0.0 && limit_cores > 0.0 {
        let delta_usage_s = delta_usage as f64 / 1_000_000.0;
        ((delta_usage_s / metrics.delta_time) / limit_cores) * 100.0
    } else {
        0.0
    };

    let usr_pct = if delta_usage > 0 {
        (delta_user as f64 / delta_usage as f64) * 100.0
    } else {
        0.0
    };
    let sys_pct = if delta_usage > 0 {
        (delta_system as f64 / delta_usage as f64) * 100.0
    } else {
        0.0
    };

    let thrtl_style = if delta_throttled_ms > 1000 {
        Styles::critical()
    } else if delta_throttled_ms > 0 {
        Styles::modified_item()
    } else {
        Styles::default()
    };

    let mut spans = vec![Span::styled("CPU", Styles::cpu()), Span::raw(" │ ")];

    spans.extend(metric_spans_default(
        "lim",
        &format!("{:.1}", limit_cores),
        CG_LIM,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "used",
        &format!("{:.0}%", used_pct.min(999.0)),
        CPU_PCT,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "usr",
        &format!("{:.0}%", usr_pct),
        CPU_PCT,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "sys",
        &format!("{:.0}%", sys_pct),
        CPU_PCT,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "thrtl",
        &format!("{}ms", delta_throttled_ms),
        CG_THR,
        thrtl_style,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "nr",
        &format!("{}", delta_nr_throttled),
        CG_NR,
    ));

    line_with_padding(spans, width)
}

/// Renders PSI (Pressure Stall Information) line.
/// Format: PSI │ CPU:  1.2%  MEM:  0.5%  I/O:  2.1%
fn render_psi_line(psi: &[PsiSummary], width: usize) -> Line<'static> {
    let mut spans = vec![Span::styled("PSI", Styles::dim()), Span::raw(" │ ")];

    for (i, p) in psi.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }

        // Color based on some_avg10 value
        let style = if p.some >= 10.0 {
            Styles::critical()
        } else if p.some >= 5.0 {
            Styles::modified_item()
        } else {
            Styles::default()
        };

        spans.extend(metric_spans(p.name, &format!("{:.1}%", p.some), 10, style));
    }

    line_with_padding(spans, width)
}

/// Renders vmstat rates line.
/// Format: VMS │ pgin:  123/s  pgout:  456/s  swin:    0/s  swout:    0/s  flt:  1.2K/s  ctx:  5.6K/s
fn render_vmstat_line(rates: &VmstatRates, width: usize) -> Line<'static> {
    let mut spans = vec![Span::styled("VMS", Styles::dim()), Span::raw(" │ ")];

    // Swap in/out - critical if any swapping is happening
    let swin_style = if rates.pswpin_s > 0.0 {
        Styles::critical()
    } else {
        Styles::default()
    };
    let swout_style = if rates.pswpout_s > 0.0 {
        Styles::critical()
    } else {
        Styles::default()
    };

    spans.extend(metric_spans_default(
        "pgin",
        &format_rate(rates.pgpgin_s),
        12,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default(
        "pgout",
        &format_rate(rates.pgpgout_s),
        13,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans(
        "swin",
        &format_rate(rates.pswpin_s),
        11,
        swin_style,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans(
        "swout",
        &format_rate(rates.pswpout_s),
        12,
        swout_style,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default(
        "flt",
        &format_rate(rates.pgfault_s),
        11,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default("ctx", &format_rate(rates.ctxt_s), 11));

    line_with_padding(spans, width)
}

/// Formats a rate value with K/M suffix if needed.
fn format_rate(value: f64) -> String {
    if value >= 1_000_000.0 {
        format!("{:.1}M/s", value / 1_000_000.0)
    } else if value >= 1000.0 {
        format!("{:.1}K/s", value / 1000.0)
    } else {
        format!("{:.0}/s", value)
    }
}

/// Renders MEM line - fixed-width metrics.
fn render_cgroup_mem_line(metrics: &SummaryMetrics, width: usize) -> Line<'static> {
    let Some(mem) = metrics.cgroup_memory.as_ref() else {
        return render_mem_line(metrics, width);
    };

    let used_style = if mem.max > 0 {
        let used_percent = (mem.current as f64 / mem.max as f64) * 100.0;
        if used_percent > 95.0 {
            Styles::critical()
        } else if used_percent > 80.0 {
            Styles::modified_item()
        } else {
            Styles::default()
        }
    } else {
        Styles::default()
    };

    let oom_style = if mem.oom_kill > 0 {
        Styles::critical()
    } else {
        Styles::default()
    };

    let mut spans = vec![Span::styled("MEM", Styles::mem()), Span::raw(" │ ")];

    spans.extend(metric_spans_default(
        "lim",
        &format_size_bytes(mem.max),
        MEM_TOT,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "used",
        &format_size_bytes(mem.current),
        MEM_AVAIL,
        used_style,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "anon",
        &format_size_bytes(mem.anon),
        MEM_CACHE,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "file",
        &format_size_bytes(mem.file),
        MEM_CACHE,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "slab",
        &format_size_bytes(mem.slab),
        MEM_SLAB,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "oom",
        &format!("{}", mem.oom_kill),
        CG_OOM,
        oom_style,
    ));

    line_with_padding(spans, width)
}

fn render_mem_line(metrics: &SummaryMetrics, width: usize) -> Line<'static> {
    let mut spans = vec![Span::styled("MEM", Styles::mem()), Span::raw(" │ ")];

    spans.extend(metric_spans_default(
        "tot",
        &format_size_gib(metrics.mem_total),
        MEM_TOT,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "avail",
        &format_size_gib(metrics.mem_available),
        MEM_AVAIL,
        style_for_mem_free(metrics.mem_available, metrics.mem_total),
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "cache",
        &format_size_gib(metrics.mem_cached),
        MEM_CACHE,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "buf",
        &format_size_gib(metrics.mem_buffers),
        MEM_BUF,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "slab",
        &format_size_gib(metrics.mem_slab),
        MEM_SLAB,
        style_for_slab(metrics.mem_slab),
    ));

    line_with_padding(spans, width)
}

/// Renders SWP line - fixed-width metrics.
fn render_swp_line(metrics: &SummaryMetrics, width: usize) -> Line<'static> {
    let swap_used = metrics.swap_total.saturating_sub(metrics.swap_free);

    let mut spans = vec![Span::styled("SWP", Styles::mem()), Span::raw(" │ ")];

    spans.extend(metric_spans_default(
        "tot",
        &format_size_gib(metrics.swap_total),
        MEM_TOT,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "free",
        &format_size_gib(metrics.swap_free),
        SWP_FREE,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "swpd",
        &format_size_gib(swap_used),
        SWP_SWPD,
        style_for_swap(swap_used),
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "dirty",
        &format_size_gib(metrics.mem_dirty),
        SWP_DIRTY,
        style_for_dirty(metrics.mem_dirty),
    ));
    spans.push(Span::raw("  "));
    let wback_style = if metrics.mem_writeback > 0 {
        Styles::modified_item()
    } else {
        Styles::default()
    };
    spans.extend(metric_spans(
        "wback",
        &format_size_gib(metrics.mem_writeback),
        SWP_WBACK,
        wback_style,
    ));

    line_with_padding(spans, width)
}

/// Formats disk name with fixed width padding (right-padded name + colon)
fn fmt_disk_name(name: &str) -> String {
    let truncated = if name.len() > DSK_NAME {
        &name[..DSK_NAME]
    } else {
        name
    };
    format!("{:width$}:", truncated, width = DSK_NAME)
}

/// Renders empty DSK line when no disk data.
fn render_dsk_empty_line(width: usize) -> Line<'static> {
    let spans = vec![
        Span::styled("DSK", Styles::disk()),
        Span::raw(" │ "),
        Span::styled("(no disk data)", Styles::dim()),
    ];
    line_with_padding(spans, width)
}

/// Renders a single DSK line for one disk device.
/// Format: DSK │ vda:   rMB:   4.0  wMB:   1.0  rd/s:   396  wr/s:    74  aw:   0.2  ut:   26%
fn render_single_dsk_line(disk: &DiskSummary, is_first: bool, width: usize) -> Line<'static> {
    let label = if is_first {
        Span::styled("DSK", Styles::disk())
    } else {
        Span::styled("dsk", Styles::dim())
    };

    let mut spans = vec![label, Span::raw(" │ ")];

    spans.push(Span::styled(fmt_disk_name(&disk.name), Styles::dim()));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default(
        "rMB",
        &format!("{:.1}", disk.read_mb_s),
        IO_MB,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default(
        "wMB",
        &format!("{:.1}", disk.write_mb_s),
        IO_MB,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default(
        "rd/s",
        &format!("{:.0}", disk.r_iops),
        IO_OPS,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default(
        "wr/s",
        &format!("{:.0}", disk.w_iops),
        IO_OPS,
    ));
    spans.push(Span::raw(" "));
    let await_max = disk.r_await.max(disk.w_await);
    spans.extend(metric_spans(
        "aw",
        &format!("{:.1}", await_max),
        IO_STAT,
        style_for_await(await_max),
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans(
        "ut",
        &format!("{:.0}%", disk.util),
        IO_STAT,
        style_for_disk_util(disk.util),
    ));

    line_with_padding(spans, width)
}

/// Formats network interface name with fixed width padding (right-padded name + colon)
fn fmt_net_name(name: &str) -> String {
    let truncated = if name.len() > NET_NAME {
        &name[..NET_NAME]
    } else {
        name
    };
    format!("{:width$}:", truncated, width = NET_NAME)
}

/// Renders empty NET line when no network data.
fn render_net_empty_line(width: usize) -> Line<'static> {
    let spans = vec![
        Span::styled("NET", Styles::default()),
        Span::raw(" │ "),
        Span::styled("(no network data)", Styles::dim()),
    ];
    line_with_padding(spans, width)
}

/// Renders a single NET line for one network interface.
/// Format: NET │ ens2:  rxMB:  0.2  txMB:  0.5  rxPk:    1K  txPk:  833  er:     0  dr:     0
fn render_single_net_line(net: &NetSummary, is_first: bool, width: usize) -> Line<'static> {
    let label = if is_first {
        Span::styled("NET", Styles::default())
    } else {
        Span::styled("net", Styles::dim())
    };

    let mut spans = vec![label, Span::raw(" │ ")];

    spans.push(Span::styled(fmt_net_name(&net.name), Styles::dim()));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default(
        "rxMB",
        &format!("{:.1}", net.rx_mb_s),
        IO_MB,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default(
        "txMB",
        &format!("{:.1}", net.tx_mb_s),
        IO_MB,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default(
        "rxPk",
        &format_pkt_rate(net.rx_pkt_s),
        IO_OPS,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default(
        "txPk",
        &format_pkt_rate(net.tx_pkt_s),
        IO_OPS,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans(
        "er",
        &format!("{}", net.errors),
        IO_STAT,
        style_for_net_errors(net.errors),
    ));
    spans.push(Span::raw(" "));
    let total_drops = net.rx_drp_s + net.tx_drp_s;
    let drp_style = if total_drops > 0.0 {
        Styles::critical()
    } else {
        Styles::default()
    };
    spans.extend(metric_spans(
        "dr",
        &format!("{:.0}", total_drops),
        IO_STAT,
        drp_style,
    ));

    line_with_padding(spans, width)
}

/// Renders help line - compact layout with context-sensitive hints.
fn render_help_line(width: usize, tab: Tab) -> Line<'static> {
    let mut spans = vec![
        Span::styled("q", Styles::help_key()),
        Span::styled(":quit(", Styles::help()),
        Span::styled("qq", Styles::help_key()),
        Span::styled("/", Styles::help()),
        Span::styled("Enter", Styles::help_key()),
        Span::styled(") ", Styles::help()),
        Span::styled("s", Styles::help_key()),
        Span::styled(":sort ", Styles::help()),
        Span::styled("r", Styles::help_key()),
        Span::styled(":rev ", Styles::help()),
        Span::styled("/", Styles::help_key()),
        Span::styled(":filter ", Styles::help()),
        Span::styled("b", Styles::help_key()),
        Span::styled(":time ", Styles::help()),
    ];

    // Tab-specific hints
    match tab {
        Tab::Processes => {
            spans.push(Span::styled("g/c/m/d", Styles::help_key()));
            spans.push(Span::styled(":view ", Styles::help()));
        }
        Tab::PostgresActive => {
            spans.push(Span::styled("g/v", Styles::help_key()));
            spans.push(Span::styled(":view ", Styles::help()));
            spans.push(Span::styled("i", Styles::help_key()));
            spans.push(Span::styled(":hide idle ", Styles::help()));
            spans.push(Span::styled(">", Styles::help_key()));
            spans.push(Span::styled(":drill ", Styles::help()));
        }
        Tab::PgStatements => {
            spans.push(Span::styled("t/c/i/e", Styles::help_key()));
            spans.push(Span::styled(":view ", Styles::help()));
        }
    }

    spans.push(Span::styled("?", Styles::help_key()));
    spans.push(Span::styled(":help", Styles::help()));

    line_with_padding(spans, width)
}

// ============================================================================
// Styling functions (anomaly highlighting)
// ============================================================================

/// Style for load average values.
fn style_for_load(load: f64, num_cpus: f64) -> Style {
    if load > num_cpus * 2.0 {
        Styles::critical()
    } else if load > num_cpus {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for running processes count.
fn style_for_running(running: u32, num_cpus: f64) -> Style {
    let running = running as f64;
    if running > num_cpus * 2.0 {
        Styles::critical()
    } else if running > num_cpus {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for CPU sys%.
fn style_for_cpu_sys(sys: f64) -> Style {
    if sys > 40.0 {
        Styles::critical()
    } else if sys > 20.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for CPU irq%.
fn style_for_cpu_irq(irq: f64) -> Style {
    if irq > 15.0 {
        Styles::critical()
    } else if irq > 5.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for CPU iowait%.
fn style_for_cpu_iow(iow: f64) -> Style {
    if iow > 30.0 {
        Styles::critical()
    } else if iow > 10.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for CPU idle%.
fn style_for_cpu_idle(idle: f64) -> Style {
    if idle < 5.0 {
        Styles::critical()
    } else if idle < 20.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for CPU steal%.
fn style_for_cpu_steal(steal: f64) -> Style {
    if steal > 15.0 {
        Styles::critical()
    } else if steal > 5.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for memory available/free.
fn style_for_mem_free(available: u64, total: u64) -> Style {
    if total == 0 {
        return Styles::default();
    }
    let percent = (available as f64 / total as f64) * 100.0;
    if percent < 5.0 {
        Styles::critical()
    } else if percent < 15.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for slab memory (in KB).
fn style_for_slab(slab_kb: u64) -> Style {
    let slab_gib = slab_kb as f64 / (1024.0 * 1024.0);
    if slab_gib > 5.0 {
        Styles::critical()
    } else if slab_gib > 3.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for swap used (in KB).
fn style_for_swap(swap_used_kb: u64) -> Style {
    let swap_gib = swap_used_kb as f64 / (1024.0 * 1024.0);
    if swap_gib > 1.0 {
        Styles::critical()
    } else if swap_used_kb > 0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for dirty pages (in KB).
fn style_for_dirty(dirty_kb: u64) -> Style {
    let dirty_mib = dirty_kb as f64 / 1024.0;
    if dirty_mib > 2048.0 {
        Styles::critical()
    } else if dirty_mib > 500.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for disk utilization%.
fn style_for_disk_util(util: f64) -> Style {
    if util > 80.0 {
        Styles::critical()
    } else if util > 50.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for disk await (ms).
fn style_for_await(await_ms: f64) -> Style {
    if await_ms > 100.0 {
        Styles::critical()
    } else if await_ms > 20.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for network errors.
fn style_for_net_errors(errors: u64) -> Style {
    if errors > 100 {
        Styles::critical()
    } else if errors > 0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

// ============================================================================
// Formatting helpers
// ============================================================================

/// Formats size in KB to human-readable GiB/MiB.
fn format_size_gib(kb: u64) -> String {
    let mib = kb as f64 / 1024.0;
    if mib >= 1024.0 {
        format!("{:.1} GiB", mib / 1024.0)
    } else if mib >= 1.0 {
        format!("{:.0} MiB", mib)
    } else {
        format!("{} KiB", kb)
    }
}

/// Formats size in bytes to human-readable GiB/MiB.
fn format_size_bytes(bytes: u64) -> String {
    // The rest of the summary formatting is KB-based, so keep it consistent.
    format_size_gib(bytes / 1024)
}

/// Formats packet rate to compact form (K/M suffix).
fn format_pkt_rate(pkt_s: f64) -> String {
    if pkt_s >= 1_000_000.0 {
        format!("{:.1}M", pkt_s / 1_000_000.0)
    } else if pkt_s >= 1_000.0 {
        format!("{:.0}K", pkt_s / 1_000.0)
    } else {
        format!("{:.0}", pkt_s)
    }
}
