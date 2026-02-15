//! Metric extraction from snapshots.

use crate::storage::model::{
    DataBlock, PgStatActivityInfo, PgStatBgwriterInfo, PgStatDatabaseInfo, ProcessInfo, Snapshot,
    SystemCpuInfo, SystemDiskInfo, SystemNetInfo, SystemPsiInfo, SystemStatInfo, SystemVmstatInfo,
};

use super::{
    BgwSummary, CpuMetrics, DiskSummary, NetSummary, PgSummary, PsiSummary, SummaryMetrics,
    TOP_CPUS, TOP_DISKS, TOP_NETS, VmstatRates,
};

/// Maximum realistic disk throughput (10 GB/s) - values above this indicate data issues
const MAX_DISK_MB_S: f64 = 10000.0;
/// Maximum realistic disk IOPS (1M) - values above this indicate data issues
const MAX_DISK_IOPS: f64 = 1_000_000.0;
/// Maximum realistic network throughput (100 Gbps = 12500 MB/s)
const MAX_NET_MB_S: f64 = 12500.0;

pub(super) fn extract_metrics(snapshot: &Snapshot, previous: Option<&Snapshot>) -> SummaryMetrics {
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
        pg_summary: None,
        bgw_summary: None,
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
            DataBlock::PgStatDatabase(dbs) => {
                let prev_dbs = previous.and_then(|p| {
                    p.blocks.iter().find_map(|b| {
                        if let DataBlock::PgStatDatabase(d) = b {
                            Some(d.as_slice())
                        } else {
                            None
                        }
                    })
                });
                metrics.pg_summary = extract_pg_summary(dbs, prev_dbs, delta_time);
            }
            DataBlock::PgStatBgwriter(bgw) => {
                let prev_bgw = previous.and_then(|p| {
                    p.blocks.iter().find_map(|b| {
                        if let DataBlock::PgStatBgwriter(v) = b {
                            Some(v)
                        } else {
                            None
                        }
                    })
                });
                metrics.bgw_summary = extract_bgw_summary(bgw, prev_bgw, delta_time);
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

    // Compute Backend IO Hit Ratio for PG processes if pg_summary exists
    if let Some(ref mut pg) = metrics.pg_summary {
        pg.backend_io_hit = compute_backend_io_hit(snapshot, previous);

        // Count errors from PgLogErrors
        let error_count: u32 = snapshot
            .blocks
            .iter()
            .filter_map(|b| {
                if let DataBlock::PgLogErrors(entries) = b {
                    Some(entries.iter().map(|e| e.count).sum::<u32>())
                } else {
                    None
                }
            })
            .sum();
        pg.errors = error_count;
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

/// Extracts PostgreSQL bgwriter summary from pg_stat_bgwriter.
/// Returns None if previous snapshot is unavailable (rates need two points).
fn extract_bgw_summary(
    curr: &PgStatBgwriterInfo,
    prev: Option<&PgStatBgwriterInfo>,
    delta_time: f64,
) -> Option<BgwSummary> {
    let prev = prev?;
    if delta_time <= 0.0 {
        return None;
    }

    let d_timed = curr
        .checkpoints_timed
        .saturating_sub(prev.checkpoints_timed);
    let d_req = curr.checkpoints_req.saturating_sub(prev.checkpoints_req);
    let d_write_time = curr.checkpoint_write_time - prev.checkpoint_write_time;
    let d_backend = curr.buffers_backend.saturating_sub(prev.buffers_backend);
    let d_clean = curr.buffers_clean.saturating_sub(prev.buffers_clean);
    let d_maxwritten = curr.maxwritten_clean.saturating_sub(prev.maxwritten_clean);
    let d_alloc = curr.buffers_alloc.saturating_sub(prev.buffers_alloc);

    Some(BgwSummary {
        checkpoints_per_min: (d_timed + d_req) as f64 / delta_time * 60.0,
        ckpt_write_time_ms: d_write_time.max(0.0),
        buffers_backend_s: d_backend as f64 / delta_time,
        buffers_clean_s: d_clean as f64 / delta_time,
        maxwritten_clean: d_maxwritten,
        buffers_alloc_s: d_alloc as f64 / delta_time,
    })
}

/// Extracts aggregated PostgreSQL summary from pg_stat_database.
/// Returns None if previous snapshot is unavailable (rates need two points).
fn extract_pg_summary(
    current: &[PgStatDatabaseInfo],
    previous: Option<&[PgStatDatabaseInfo]>,
    delta_time: f64,
) -> Option<PgSummary> {
    use std::collections::HashMap;

    let prev = previous?;
    if delta_time <= 0.0 {
        return None;
    }

    let prev_map: HashMap<u32, &PgStatDatabaseInfo> = prev.iter().map(|d| (d.datid, d)).collect();

    let mut sum_commit: i64 = 0;
    let mut sum_rollback: i64 = 0;
    let mut sum_blks_read: i64 = 0;
    let mut sum_blks_hit: i64 = 0;
    let mut sum_tup: i64 = 0;
    let mut sum_temp_bytes: i64 = 0;
    let mut sum_deadlocks: i64 = 0;

    for db in current {
        let Some(p) = prev_map.get(&db.datid) else {
            continue;
        };

        sum_commit += db.xact_commit.saturating_sub(p.xact_commit);
        sum_rollback += db.xact_rollback.saturating_sub(p.xact_rollback);
        sum_blks_read += db.blks_read.saturating_sub(p.blks_read);
        sum_blks_hit += db.blks_hit.saturating_sub(p.blks_hit);
        sum_tup += db.tup_returned.saturating_sub(p.tup_returned)
            + db.tup_fetched.saturating_sub(p.tup_fetched)
            + db.tup_inserted.saturating_sub(p.tup_inserted)
            + db.tup_updated.saturating_sub(p.tup_updated)
            + db.tup_deleted.saturating_sub(p.tup_deleted);
        sum_temp_bytes += db.temp_bytes.saturating_sub(p.temp_bytes);
        sum_deadlocks += db.deadlocks.saturating_sub(p.deadlocks);
    }

    let total_blks = sum_blks_hit + sum_blks_read;
    let hit_ratio = if total_blks > 0 {
        sum_blks_hit as f64 * 100.0 / total_blks as f64
    } else {
        100.0
    };

    Some(PgSummary {
        tps: (sum_commit + sum_rollback) as f64 / delta_time,
        hit_ratio,
        backend_io_hit: 100.0, // Overwritten after the main loop
        tup_s: sum_tup as f64 / delta_time,
        tmp_bytes_s: sum_temp_bytes as f64 / delta_time,
        deadlocks: sum_deadlocks,
        errors: 0, // Filled in extract_metrics() from PgLogErrors
    })
}

/// Finds PgStatActivity rows in a snapshot.
fn find_pg_activity(snapshot: &Snapshot) -> Option<&[PgStatActivityInfo]> {
    snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::PgStatActivity(rows) = b {
            Some(rows.as_slice())
        } else {
            None
        }
    })
}

/// Finds Process rows in a snapshot.
fn find_processes(snapshot: &Snapshot) -> Option<&[ProcessInfo]> {
    snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::Processes(procs) = b {
            Some(procs.as_slice())
        } else {
            None
        }
    })
}

/// Computes Backend IO Hit Ratio from /proc/[pid]/io data of PG backend processes.
///
/// Formula: (delta_rchar - delta_read_bytes) / delta_rchar * 100
/// This measures how much of the PG backends' read I/O was served from OS page cache.
fn compute_backend_io_hit(snapshot: &Snapshot, previous: Option<&Snapshot>) -> f64 {
    let previous = match previous {
        Some(p) => p,
        None => return 100.0,
    };

    // Collect PG backend PIDs from pg_stat_activity
    let Some(pga) = find_pg_activity(snapshot) else {
        return 100.0;
    };
    let pg_pids: std::collections::HashSet<u32> = pga
        .iter()
        .filter_map(|a| u32::try_from(a.pid).ok())
        .collect();
    if pg_pids.is_empty() {
        return 100.0;
    }

    // Sum rchar and read_bytes for PG processes in current snapshot
    let Some(curr_procs) = find_processes(snapshot) else {
        return 100.0;
    };
    let (curr_rchar, curr_rsz) = sum_pg_io(curr_procs, &pg_pids);

    // Sum rchar and read_bytes for PG processes in previous snapshot
    let Some(prev_procs) = find_processes(previous) else {
        return 100.0;
    };
    let (prev_rchar, prev_rsz) = sum_pg_io(prev_procs, &pg_pids);

    let delta_rchar = curr_rchar.saturating_sub(prev_rchar);
    let delta_rsz = curr_rsz.saturating_sub(prev_rsz);

    if delta_rchar == 0 {
        return 100.0;
    }

    let cache_bytes = delta_rchar.saturating_sub(delta_rsz);
    cache_bytes as f64 * 100.0 / delta_rchar as f64
}

/// Sums rchar and read_bytes for processes whose PID is in pg_pids.
fn sum_pg_io(procs: &[ProcessInfo], pg_pids: &std::collections::HashSet<u32>) -> (u64, u64) {
    let mut rchar = 0u64;
    let mut rsz = 0u64;
    for p in procs {
        if pg_pids.contains(&p.pid) {
            rchar += p.dsk.rchar;
            rsz += p.dsk.rsz;
        }
    }
    (rchar, rsz)
}
