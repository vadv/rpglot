//! Snapshot → ApiSnapshot conversion.
//!
//! Converts internal `Snapshot` + computed rates into a JSON-serializable `ApiSnapshot`.
//! All interned strings are resolved, all rates are pre-computed.

use std::collections::HashMap;

use crate::analysis::compute_backend_io_hit;
use crate::models::{PgIndexesRates, PgStatementsRates, PgTablesRates};
use crate::storage::StringInterner;
use crate::storage::model::{
    CgroupCpuInfo, DataBlock, PgLogEventType, PgLogSeverity, PgStatBgwriterInfo,
    PgStatDatabaseInfo, ProcessInfo, Snapshot, SystemCpuInfo, SystemDiskInfo, SystemNetInfo,
};

use super::snapshot::*;

/// Sum Option<f64> values: returns Some(sum) if at least one is Some, None if all are None.
fn add_opts(vals: &[Option<f64>]) -> Option<f64> {
    let mut sum = 0.0_f64;
    let mut any = false;
    for x in vals.iter().flatten() {
        sum += x;
        any = true;
    }
    if any { Some(sum) } else { None }
}

/// Input context for snapshot conversion.
pub struct ConvertContext<'a> {
    pub snapshot: &'a Snapshot,
    pub prev_snapshot: Option<&'a Snapshot>,
    pub interner: Option<&'a StringInterner>,
    pub pgs_rates: &'a HashMap<i64, PgStatementsRates>,
    pub pgt_rates: &'a HashMap<u32, PgTablesRates>,
    pub pgi_rates: &'a HashMap<u32, PgIndexesRates>,
}

/// Convert internal snapshot + rates into API snapshot.
pub fn convert(ctx: &ConvertContext<'_>) -> ApiSnapshot {
    let snap = ctx.snapshot;
    let delta_time = get_delta_time(snap, ctx.prev_snapshot);

    ApiSnapshot {
        timestamp: snap.timestamp,
        prev_timestamp: None,
        next_timestamp: None,
        system: extract_system_summary(snap, ctx.prev_snapshot, delta_time),
        pg: extract_pg_summary(snap, ctx.prev_snapshot, delta_time),
        prc: extract_prc(snap, ctx.prev_snapshot, ctx.interner, delta_time),
        pga: extract_pga(
            snap,
            ctx.prev_snapshot,
            ctx.interner,
            ctx.pgs_rates,
            delta_time,
        ),
        pgs: extract_pgs(snap, ctx.interner, ctx.pgs_rates),
        pgt: extract_pgt(snap, ctx.interner, ctx.pgt_rates),
        pgi: extract_pgi(snap, ctx.interner, ctx.pgi_rates),
        pge: extract_pge(snap, ctx.interner),
        pgl: extract_pgl(snap, ctx.interner),
    }
}

// ============================================================
// Helpers
// ============================================================

fn get_delta_time(current: &Snapshot, previous: Option<&Snapshot>) -> f64 {
    previous
        .map(|p| {
            let dt = current.timestamp - p.timestamp;
            if dt > 0 { dt as f64 } else { 1.0 }
        })
        .unwrap_or(1.0)
}

fn resolve(interner: Option<&StringInterner>, hash: u64) -> String {
    if hash == 0 {
        return String::new();
    }
    interner
        .and_then(|i| i.resolve(hash))
        .unwrap_or("")
        .to_string()
}

fn find_block<'a, T>(
    snapshot: &'a Snapshot,
    extract: impl Fn(&'a DataBlock) -> Option<T>,
) -> Option<T> {
    snapshot.blocks.iter().find_map(extract)
}

// ============================================================
// System summary
// ============================================================

fn extract_system_summary(
    snap: &Snapshot,
    prev: Option<&Snapshot>,
    delta_time: f64,
) -> SystemSummary {
    let cpu = find_block(snap, |b| {
        if let DataBlock::SystemCpu(cpus) = b {
            extract_cpu_summary(cpus)
        } else {
            None
        }
    });

    let load = find_block(snap, |b| {
        if let DataBlock::SystemLoad(l) = b {
            Some(LoadSummary {
                avg1: l.lavg1,
                avg5: l.lavg5,
                avg15: l.lavg15,
                nr_threads: l.nr_threads,
                nr_running: l.nr_running,
            })
        } else {
            None
        }
    });

    let memory = find_block(snap, |b| {
        if let DataBlock::SystemMem(m) = b {
            Some(MemorySummary {
                total_kb: m.total,
                available_kb: m.available,
                cached_kb: m.cached,
                buffers_kb: m.buffers,
                slab_kb: m.slab,
            })
        } else {
            None
        }
    });

    let swap = find_block(snap, |b| {
        if let DataBlock::SystemMem(m) = b {
            Some(SwapSummary {
                total_kb: m.swap_total,
                free_kb: m.swap_free,
                used_kb: m.swap_total.saturating_sub(m.swap_free),
                dirty_kb: m.dirty,
                writeback_kb: m.writeback,
            })
        } else {
            None
        }
    });

    let disks = extract_disk_summaries(snap, prev, delta_time);
    let networks = extract_network_summaries(snap, prev, delta_time);

    let psi = find_block(snap, |b| {
        if let DataBlock::SystemPsi(psi_list) = b {
            let mut cpu_some = 0.0_f64;
            let mut mem_some = 0.0_f64;
            let mut io_some = 0.0_f64;
            for p in psi_list {
                match p.resource {
                    0 => cpu_some = p.some_avg10 as f64,
                    1 => mem_some = p.some_avg10 as f64,
                    2 => io_some = p.some_avg10 as f64,
                    _ => {}
                }
            }
            Some(PsiSummary {
                cpu_some_pct: cpu_some,
                mem_some_pct: mem_some,
                io_some_pct: io_some,
            })
        } else {
            None
        }
    });

    let vmstat = extract_vmstat_summary(snap, prev, delta_time);

    let (cgroup_cpu, cgroup_memory, cgroup_pids) = extract_cgroup_summaries(snap, prev, delta_time);

    SystemSummary {
        cpu,
        load,
        memory,
        swap,
        disks,
        networks,
        psi,
        vmstat,
        cgroup_cpu,
        cgroup_memory,
        cgroup_pids,
    }
}

fn extract_cpu_summary(cpus: &[SystemCpuInfo]) -> Option<CpuSummary> {
    let agg = cpus.iter().find(|c| c.cpu_id == -1)?;
    let total = agg.user
        + agg.nice
        + agg.system
        + agg.idle
        + agg.iowait
        + agg.irq
        + agg.softirq
        + agg.steal;
    if total == 0 {
        return None;
    }
    let t = total as f64;
    Some(CpuSummary {
        sys_pct: agg.system as f64 / t * 100.0,
        usr_pct: (agg.user + agg.nice) as f64 / t * 100.0,
        irq_pct: (agg.irq + agg.softirq) as f64 / t * 100.0,
        iow_pct: agg.iowait as f64 / t * 100.0,
        idle_pct: agg.idle as f64 / t * 100.0,
        steal_pct: agg.steal as f64 / t * 100.0,
    })
}

fn extract_disk_summaries(
    snap: &Snapshot,
    prev: Option<&Snapshot>,
    delta_time: f64,
) -> Vec<DiskSummary> {
    let Some(disks) = find_block(snap, |b| {
        if let DataBlock::SystemDisk(d) = b {
            Some(d.as_slice())
        } else {
            None
        }
    }) else {
        return Vec::new();
    };

    let prev_disks: HashMap<u64, &SystemDiskInfo> = prev
        .and_then(|p| {
            find_block(p, |b| {
                if let DataBlock::SystemDisk(d) = b {
                    Some(d.as_slice())
                } else {
                    None
                }
            })
        })
        .map(|ds| ds.iter().map(|d| (d.device_hash, d)).collect())
        .unwrap_or_default();

    let is_container = snap
        .blocks
        .iter()
        .any(|b| matches!(b, DataBlock::Cgroup(_)));

    disks
        .iter()
        .filter(|d| {
            if is_container && d.major == 0 && d.minor == 0 {
                return false;
            }
            if d.device_name.starts_with("loop") || d.device_name.starts_with("ram") {
                return false;
            }
            if !is_container
                && d.device_name
                    .chars()
                    .last()
                    .is_some_and(|c| c.is_ascii_digit())
                && !d.device_name.starts_with("nvme")
            {
                return false;
            }
            true
        })
        .map(|disk| {
            if let Some(p) = prev_disks.get(&disk.device_hash) {
                let read_sectors = disk.rsz.saturating_sub(p.rsz);
                let write_sectors = disk.wsz.saturating_sub(p.wsz);
                let io_ms = disk.io_ms.saturating_sub(p.io_ms);
                let d_rio = disk.rio.saturating_sub(p.rio);
                let d_wio = disk.wio.saturating_sub(p.wio);
                let d_read_time = disk.read_time.saturating_sub(p.read_time);
                let d_write_time = disk.write_time.saturating_sub(p.write_time);

                DiskSummary {
                    name: disk.device_name.clone(),
                    read_bytes_s: (read_sectors as f64 * 512.0) / delta_time,
                    write_bytes_s: (write_sectors as f64 * 512.0) / delta_time,
                    read_iops: d_rio as f64 / delta_time,
                    write_iops: d_wio as f64 / delta_time,
                    util_pct: ((io_ms as f64 / (delta_time * 1000.0)) * 100.0).min(100.0),
                    r_await_ms: if d_rio > 0 {
                        d_read_time as f64 / d_rio as f64
                    } else {
                        0.0
                    },
                    w_await_ms: if d_wio > 0 {
                        d_write_time as f64 / d_wio as f64
                    } else {
                        0.0
                    },
                }
            } else {
                DiskSummary {
                    name: disk.device_name.clone(),
                    read_bytes_s: 0.0,
                    write_bytes_s: 0.0,
                    read_iops: 0.0,
                    write_iops: 0.0,
                    util_pct: 0.0,
                    r_await_ms: 0.0,
                    w_await_ms: 0.0,
                }
            }
        })
        .collect()
}

fn extract_network_summaries(
    snap: &Snapshot,
    prev: Option<&Snapshot>,
    delta_time: f64,
) -> Vec<NetworkSummary> {
    let Some(nets) = find_block(snap, |b| {
        if let DataBlock::SystemNet(n) = b {
            Some(n.as_slice())
        } else {
            None
        }
    }) else {
        return Vec::new();
    };

    let prev_nets: HashMap<u64, &SystemNetInfo> = prev
        .and_then(|p| {
            find_block(p, |b| {
                if let DataBlock::SystemNet(n) = b {
                    Some(n.as_slice())
                } else {
                    None
                }
            })
        })
        .map(|ns| ns.iter().map(|n| (n.name_hash, n)).collect())
        .unwrap_or_default();

    let is_container = snap
        .blocks
        .iter()
        .any(|b| matches!(b, DataBlock::Cgroup(_)));

    nets.iter()
        .filter(|n| {
            if n.name == "lo" {
                return false;
            }
            if is_container {
                n.name.starts_with("eth") || n.name.starts_with("veth")
            } else {
                true
            }
        })
        .map(|net| {
            if let Some(p) = prev_nets.get(&net.name_hash) {
                NetworkSummary {
                    name: net.name.clone(),
                    rx_bytes_s: net.rx_bytes.saturating_sub(p.rx_bytes) as f64 / delta_time,
                    tx_bytes_s: net.tx_bytes.saturating_sub(p.tx_bytes) as f64 / delta_time,
                    rx_packets_s: net.rx_packets.saturating_sub(p.rx_packets) as f64 / delta_time,
                    tx_packets_s: net.tx_packets.saturating_sub(p.tx_packets) as f64 / delta_time,
                    errors_s: (net.rx_errs.saturating_sub(p.rx_errs)
                        + net.tx_errs.saturating_sub(p.tx_errs))
                        as f64
                        / delta_time,
                    drops_s: (net.rx_drop.saturating_sub(p.rx_drop)
                        + net.tx_drop.saturating_sub(p.tx_drop))
                        as f64
                        / delta_time,
                }
            } else {
                NetworkSummary {
                    name: net.name.clone(),
                    rx_bytes_s: 0.0,
                    tx_bytes_s: 0.0,
                    rx_packets_s: 0.0,
                    tx_packets_s: 0.0,
                    errors_s: 0.0,
                    drops_s: 0.0,
                }
            }
        })
        .collect()
}

fn extract_vmstat_summary(
    snap: &Snapshot,
    prev: Option<&Snapshot>,
    delta_time: f64,
) -> Option<VmstatSummary> {
    let prev = prev?;
    if delta_time <= 0.0 {
        return None;
    }

    let curr_vm = find_block(snap, |b| {
        if let DataBlock::SystemVmstat(v) = b {
            Some(v)
        } else {
            None
        }
    })?;
    let prev_vm = find_block(prev, |b| {
        if let DataBlock::SystemVmstat(v) = b {
            Some(v)
        } else {
            None
        }
    })?;

    let curr_stat = find_block(snap, |b| {
        if let DataBlock::SystemStat(s) = b {
            Some(s)
        } else {
            None
        }
    });
    let prev_stat = find_block(prev, |b| {
        if let DataBlock::SystemStat(s) = b {
            Some(s)
        } else {
            None
        }
    });

    let ctxsw_s = match (curr_stat, prev_stat) {
        (Some(c), Some(p)) => c.ctxt.saturating_sub(p.ctxt) as f64 / delta_time,
        _ => 0.0,
    };

    Some(VmstatSummary {
        pgin_s: curr_vm.pgpgin.saturating_sub(prev_vm.pgpgin) as f64 / delta_time,
        pgout_s: curr_vm.pgpgout.saturating_sub(prev_vm.pgpgout) as f64 / delta_time,
        swin_s: curr_vm.pswpin.saturating_sub(prev_vm.pswpin) as f64 / delta_time,
        swout_s: curr_vm.pswpout.saturating_sub(prev_vm.pswpout) as f64 / delta_time,
        pgfault_s: curr_vm.pgfault.saturating_sub(prev_vm.pgfault) as f64 / delta_time,
        ctxsw_s,
    })
}

// ============================================================
// Cgroup summaries (container mode)
// ============================================================

fn find_cgroup_cpu(snap: &Snapshot) -> Option<&CgroupCpuInfo> {
    find_block(snap, |b| {
        if let DataBlock::Cgroup(cg) = b {
            cg.cpu.as_ref()
        } else {
            None
        }
    })
}

fn extract_cgroup_summaries(
    snap: &Snapshot,
    prev: Option<&Snapshot>,
    delta_time: f64,
) -> (
    Option<CgroupCpuSummary>,
    Option<CgroupMemorySummary>,
    Option<CgroupPidsSummary>,
) {
    let cg = find_block(snap, |b| {
        if let DataBlock::Cgroup(cg) = b {
            Some(cg)
        } else {
            None
        }
    });
    let Some(cg) = cg else {
        return (None, None, None);
    };

    // --- Cgroup CPU ---
    let cgroup_cpu = cg
        .cpu
        .as_ref()
        .filter(|c| c.quota > 0 && c.period > 0)
        .map(|cpu| {
            let limit_cores = cpu.quota as f64 / cpu.period as f64;

            let prev_cpu = prev.and_then(find_cgroup_cpu);
            let (used_pct, usr_pct, sys_pct, throttled_ms, nr_throttled) = if let Some(pc) =
                prev_cpu
            {
                if delta_time > 0.0 {
                    let d_usage = cpu.usage_usec.saturating_sub(pc.usage_usec) as f64 / 1_000_000.0;
                    let d_user = cpu.user_usec.saturating_sub(pc.user_usec) as f64 / 1_000_000.0;
                    let d_system =
                        cpu.system_usec.saturating_sub(pc.system_usec) as f64 / 1_000_000.0;
                    let d_throttled =
                        cpu.throttled_usec.saturating_sub(pc.throttled_usec) as f64 / 1000.0;
                    let d_nr = cpu.nr_throttled.saturating_sub(pc.nr_throttled) as f64;

                    let used = if limit_cores > 0.0 {
                        (d_usage / delta_time / limit_cores) * 100.0
                    } else {
                        0.0
                    };
                    let usr = if d_usage > 0.0 {
                        (d_user / d_usage) * 100.0
                    } else {
                        0.0
                    };
                    let sys = if d_usage > 0.0 {
                        (d_system / d_usage) * 100.0
                    } else {
                        0.0
                    };
                    (used, usr, sys, d_throttled, d_nr)
                } else {
                    (0.0, 0.0, 0.0, 0.0, 0.0)
                }
            } else {
                (0.0, 0.0, 0.0, 0.0, 0.0)
            };

            CgroupCpuSummary {
                limit_cores,
                used_pct,
                usr_pct,
                sys_pct,
                throttled_ms,
                nr_throttled,
            }
        });

    // --- Cgroup Memory ---
    let cgroup_memory = cg.memory.as_ref().filter(|m| m.max != u64::MAX).map(|mem| {
        let used_pct = if mem.max > 0 {
            mem.current as f64 * 100.0 / mem.max as f64
        } else {
            0.0
        };
        let anon_pct = if mem.max > 0 {
            (mem.anon + mem.slab) as f64 * 100.0 / mem.max as f64
        } else {
            0.0
        };
        CgroupMemorySummary {
            limit_bytes: mem.max,
            used_bytes: mem.current,
            used_pct,
            anon_pct,
            anon_bytes: mem.anon,
            file_bytes: mem.file,
            slab_bytes: mem.slab,
            oom_kills: mem.oom_kill,
        }
    });

    // --- Cgroup PIDs ---
    let cgroup_pids =
        cg.pids
            .as_ref()
            .filter(|p| p.max != u64::MAX)
            .map(|pids| CgroupPidsSummary {
                current: pids.current,
                max: pids.max,
            });

    (cgroup_cpu, cgroup_memory, cgroup_pids)
}

// ============================================================
// PG summary
// ============================================================

fn extract_pg_summary(snap: &Snapshot, prev: Option<&Snapshot>, delta_time: f64) -> PgSummary {
    let db_rates = compute_pg_db_rates(snap, prev, delta_time);
    let bgw = compute_bgw_rates(snap, prev, delta_time);
    let backend_io_hit = compute_backend_io_hit(snap, prev);

    let errors: u32 = find_block(snap, |b| {
        if let DataBlock::PgLogErrors(entries) = b {
            Some(entries.iter().map(|e| e.count).sum())
        } else {
            None
        }
    })
    .unwrap_or(0);

    PgSummary {
        tps: db_rates.as_ref().map(|r| r.0),
        hit_ratio_pct: db_rates.as_ref().map(|r| r.1),
        backend_io_hit_pct: backend_io_hit,
        tuples_s: db_rates.as_ref().map(|r| r.2),
        temp_bytes_s: db_rates.as_ref().map(|r| r.3),
        deadlocks: db_rates.as_ref().map(|r| r.4),
        bgwriter: bgw,
        errors: if errors > 0 { Some(errors) } else { None },
    }
}

/// Returns (tps, hit_ratio, tuples_s, temp_bytes_s, deadlocks).
fn compute_pg_db_rates(
    snap: &Snapshot,
    prev: Option<&Snapshot>,
    delta_time: f64,
) -> Option<(f64, f64, f64, f64, f64)> {
    let prev = prev?;
    if delta_time <= 0.0 {
        return None;
    }

    let current: &[PgStatDatabaseInfo] = find_block(snap, |b| {
        if let DataBlock::PgStatDatabase(d) = b {
            Some(d.as_slice())
        } else {
            None
        }
    })?;

    let prev_dbs: HashMap<u32, &PgStatDatabaseInfo> = find_block(prev, |b| {
        if let DataBlock::PgStatDatabase(d) = b {
            Some(d.as_slice())
        } else {
            None
        }
    })
    .map(|ds| ds.iter().map(|d| (d.datid, d)).collect())
    .unwrap_or_default();

    let mut sum_commit: i64 = 0;
    let mut sum_rollback: i64 = 0;
    let mut sum_blks_read: i64 = 0;
    let mut sum_blks_hit: i64 = 0;
    let mut sum_tup: i64 = 0;
    let mut sum_temp_bytes: i64 = 0;
    let mut sum_deadlocks: i64 = 0;

    for db in current {
        let Some(p) = prev_dbs.get(&db.datid) else {
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

    Some((
        (sum_commit + sum_rollback) as f64 / delta_time,
        hit_ratio,
        sum_tup as f64 / delta_time,
        sum_temp_bytes as f64 / delta_time,
        sum_deadlocks as f64,
    ))
}

fn compute_bgw_rates(
    snap: &Snapshot,
    prev: Option<&Snapshot>,
    delta_time: f64,
) -> Option<BgwriterSummary> {
    let prev = prev?;
    if delta_time <= 0.0 {
        return None;
    }

    let curr: &PgStatBgwriterInfo = find_block(snap, |b| {
        if let DataBlock::PgStatBgwriter(v) = b {
            Some(v)
        } else {
            None
        }
    })?;

    let prev_bgw: &PgStatBgwriterInfo = find_block(prev, |b| {
        if let DataBlock::PgStatBgwriter(v) = b {
            Some(v)
        } else {
            None
        }
    })?;

    let d_timed = curr
        .checkpoints_timed
        .saturating_sub(prev_bgw.checkpoints_timed);
    let d_req = curr
        .checkpoints_req
        .saturating_sub(prev_bgw.checkpoints_req);
    let d_write_time = curr.checkpoint_write_time - prev_bgw.checkpoint_write_time;

    Some(BgwriterSummary {
        checkpoints_per_min: (d_timed + d_req) as f64 / delta_time * 60.0,
        checkpoint_write_time_ms: d_write_time.max(0.0),
        buffers_backend_s: curr
            .buffers_backend
            .saturating_sub(prev_bgw.buffers_backend) as f64
            / delta_time,
        buffers_clean_s: curr.buffers_clean.saturating_sub(prev_bgw.buffers_clean) as f64
            / delta_time,
        maxwritten_clean: curr
            .maxwritten_clean
            .saturating_sub(prev_bgw.maxwritten_clean) as f64,
        buffers_alloc_s: curr.buffers_alloc.saturating_sub(prev_bgw.buffers_alloc) as f64
            / delta_time,
    })
}

// ============================================================
// PRC (OS processes)
// ============================================================

fn extract_prc(
    snap: &Snapshot,
    prev: Option<&Snapshot>,
    interner: Option<&StringInterner>,
    delta_time: f64,
) -> Vec<ApiProcessRow> {
    let Some(processes) = find_block(snap, |b| {
        if let DataBlock::Processes(v) = b {
            Some(v.as_slice())
        } else {
            None
        }
    }) else {
        return Vec::new();
    };

    // Compute I/O inherited from died children (Linux adds child's
    // cumulative /proc/pid/io counters to parent on wait()).
    let died_io = prev
        .and_then(|ps| {
            find_block(ps, |b| {
                if let DataBlock::Processes(v) = b {
                    Some(v.as_slice())
                } else {
                    None
                }
            })
        })
        .map(|prev_procs| crate::util::process_io::compute_died_children_io(processes, prev_procs))
        .unwrap_or_default();

    // Previous processes by PID for delta computation
    let prev_procs: HashMap<u32, &ProcessInfo> = prev
        .and_then(|p| {
            find_block(p, |b| {
                if let DataBlock::Processes(v) = b {
                    Some(v.as_slice())
                } else {
                    None
                }
            })
        })
        .map(|ps| ps.iter().map(|p| (p.pid, p)).collect())
        .unwrap_or_default();

    // Total CPU time for cpu% calculation
    let total_cpu = get_total_cpu_ticks(snap);
    let prev_total_cpu = prev.and_then(get_total_cpu_ticks);

    // Total memory for mem% calculation
    let total_mem_kb = find_block(snap, |b| {
        if let DataBlock::SystemMem(m) = b {
            Some(m.total)
        } else {
            None
        }
    })
    .unwrap_or(1);

    // PG stat activity by PID for query enrichment
    let pg_by_pid: HashMap<i32, (u64, u64)> = find_block(snap, |b| {
        if let DataBlock::PgStatActivity(v) = b {
            Some(v.as_slice())
        } else {
            None
        }
    })
    .map(|activities| {
        activities
            .iter()
            .map(|a| (a.pid, (a.query_hash, a.backend_type_hash)))
            .collect()
    })
    .unwrap_or_default();

    let has_prev = !prev_procs.is_empty() && delta_time > 0.0;

    processes
        .iter()
        .map(|p| {
            let prev_p = prev_procs.get(&p.pid);

            // CPU %
            let cpu_pct =
                if let (Some(pp), Some(tc), Some(ptc)) = (prev_p, total_cpu, prev_total_cpu) {
                    let d_proc =
                        (p.cpu.utime + p.cpu.stime).saturating_sub(pp.cpu.utime + pp.cpu.stime);
                    let d_total = tc.saturating_sub(ptc);
                    if d_total > 0 {
                        d_proc as f64 / d_total as f64 * 100.0
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };

            // Memory %
            let mem_pct = if total_mem_kb > 0 {
                p.mem.rmem as f64 / total_mem_kb as f64 * 100.0
            } else {
                0.0
            };

            // Memory deltas
            let (vgrow_kb, rgrow_kb) = if let Some(pp) = prev_p {
                (
                    p.mem.vmem as i64 - pp.mem.vmem as i64,
                    p.mem.rmem as i64 - pp.mem.rmem as i64,
                )
            } else {
                (0, 0)
            };

            // Disk I/O rates (subtract inherited I/O from died children)
            let (read_bytes_s, write_bytes_s, read_ops_s, write_ops_s) =
                if let (Some(pp), true) = (prev_p, has_prev) {
                    let inherited = died_io.get(&p.pid);
                    let adj_rsz = inherited.map_or(0, |d| d.rsz);
                    let adj_wsz = inherited.map_or(0, |d| d.wsz);
                    let adj_rio = inherited.map_or(0, |d| d.rio);
                    let adj_wio = inherited.map_or(0, |d| d.wio);
                    (
                        Some(
                            p.dsk.rsz.saturating_sub(pp.dsk.rsz).saturating_sub(adj_rsz) as f64
                                / delta_time,
                        ),
                        Some(
                            p.dsk.wsz.saturating_sub(pp.dsk.wsz).saturating_sub(adj_wsz) as f64
                                / delta_time,
                        ),
                        Some(
                            p.dsk.rio.saturating_sub(pp.dsk.rio).saturating_sub(adj_rio) as f64
                                / delta_time,
                        ),
                        Some(
                            p.dsk.wio.saturating_sub(pp.dsk.wio).saturating_sub(adj_wio) as f64
                                / delta_time,
                        ),
                    )
                } else {
                    (None, None, None, None)
                };

            // PG enrichment
            let (pg_query, pg_backend_type) =
                if let Some(&(query_hash, bt_hash)) = pg_by_pid.get(&(p.pid as i32)) {
                    let q = resolve(interner, query_hash);
                    let bt = resolve(interner, bt_hash);
                    (
                        if q.is_empty() { None } else { Some(q) },
                        if bt.is_empty() { None } else { Some(bt) },
                    )
                } else {
                    (None, None)
                };

            // Context switch rates
            let (nvcsw_s, nivcsw_s) = if let (Some(pp), true) = (prev_p, has_prev) {
                (
                    Some(p.cpu.nvcsw.saturating_sub(pp.cpu.nvcsw) as f64 / delta_time),
                    Some(p.cpu.nivcsw.saturating_sub(pp.cpu.nivcsw) as f64 / delta_time),
                )
            } else {
                (None, None)
            };

            ApiProcessRow {
                pid: p.pid,
                ppid: p.ppid,
                name: resolve(interner, p.name_hash),
                cmdline: resolve(interner, p.cmdline_hash),
                state: p.state.to_string(),
                num_threads: p.num_threads,
                btime: p.btime,
                cpu_pct,
                utime: p.cpu.utime,
                stime: p.cpu.stime,
                curcpu: p.cpu.curcpu,
                rundelay: p.cpu.rundelay,
                nice: p.cpu.nice,
                priority: p.cpu.prio,
                rtprio: p.cpu.rtprio,
                policy: p.cpu.policy,
                blkdelay: p.cpu.blkdelay,
                nvcsw: p.cpu.nvcsw,
                nivcsw: p.cpu.nivcsw,
                nvcsw_s,
                nivcsw_s,
                mem_pct,
                vsize_kb: p.mem.vmem,
                rsize_kb: p.mem.rmem,
                psize_kb: p.mem.pmem,
                vgrow_kb,
                rgrow_kb,
                vswap_kb: p.mem.vswap,
                vstext_kb: p.mem.vexec,
                vdata_kb: p.mem.vdata,
                vstack_kb: p.mem.vstack,
                vslibs_kb: p.mem.vlibs,
                vlock_kb: p.mem.vlock,
                minflt: p.mem.minflt,
                majflt: p.mem.majflt,
                read_bytes_s,
                write_bytes_s,
                read_ops_s,
                write_ops_s,
                total_read_bytes: p.dsk.rsz,
                total_write_bytes: p.dsk.wsz,
                total_read_ops: p.dsk.rio,
                total_write_ops: p.dsk.wio,
                cancelled_write_bytes: p.dsk.cwsz,
                uid: p.uid,
                euid: p.euid,
                gid: p.gid,
                egid: p.egid,
                tty: p.tty,
                exit_signal: p.exit_signal,
                pg_query,
                pg_backend_type,
            }
        })
        .collect()
}

/// Get total CPU ticks (all fields summed from aggregate cpu entry).
fn get_total_cpu_ticks(snap: &Snapshot) -> Option<u64> {
    find_block(snap, |b| {
        if let DataBlock::SystemCpu(cpus) = b {
            cpus.iter().find(|c| c.cpu_id == -1).map(|agg| {
                agg.user
                    + agg.nice
                    + agg.system
                    + agg.idle
                    + agg.iowait
                    + agg.irq
                    + agg.softirq
                    + agg.steal
            })
        } else {
            None
        }
    })
}

// ============================================================
// PGA (pg_stat_activity)
// ============================================================

fn extract_pga(
    snap: &Snapshot,
    prev: Option<&Snapshot>,
    interner: Option<&StringInterner>,
    pgs_rates: &HashMap<i64, PgStatementsRates>,
    delta_time: f64,
) -> Vec<PgActivityRow> {
    let Some(activities) = find_block(snap, |b| {
        if let DataBlock::PgStatActivity(v) = b {
            Some(v.as_slice())
        } else {
            None
        }
    }) else {
        return Vec::new();
    };

    // OS processes for CPU%/RSS/IO enrichment
    let proc_slice = find_block(snap, |b| {
        if let DataBlock::Processes(v) = b {
            Some(v.as_slice())
        } else {
            None
        }
    });
    let prev_proc_slice = prev.and_then(|p| {
        find_block(p, |b| {
            if let DataBlock::Processes(v) = b {
                Some(v.as_slice())
            } else {
                None
            }
        })
    });

    // Subtract inherited I/O from died children
    let died_io = match (proc_slice, prev_proc_slice) {
        (Some(curr), Some(prev_ps)) => {
            crate::util::process_io::compute_died_children_io(curr, prev_ps)
        }
        _ => HashMap::new(),
    };

    let processes: HashMap<u32, &ProcessInfo> = proc_slice
        .map(|ps| ps.iter().map(|p| (p.pid, p)).collect())
        .unwrap_or_default();
    let prev_procs: HashMap<u32, &ProcessInfo> = prev_proc_slice
        .map(|ps| ps.iter().map(|p| (p.pid, p)).collect())
        .unwrap_or_default();

    let total_cpu = get_total_cpu_ticks(snap);
    let prev_total_cpu = prev.and_then(get_total_cpu_ticks);

    // pg_stat_statements by queryid for stmt enrichment
    let stmts_by_qid: HashMap<i64, &crate::storage::model::PgStatStatementsInfo> =
        find_block(snap, |b| {
            if let DataBlock::PgStatStatements(v) = b {
                Some(v.as_slice())
            } else {
                None
            }
        })
        .map(|ss| ss.iter().map(|s| (s.queryid, s)).collect())
        .unwrap_or_default();

    let now = snap.timestamp;
    let has_prev = !prev_procs.is_empty() && delta_time > 0.0;

    activities
        .iter()
        .map(|a| {
            let query_duration_s = if a.query_start > 0 {
                Some(now.saturating_sub(a.query_start))
            } else {
                None
            };
            let xact_duration_s = if a.xact_start > 0 {
                Some(now.saturating_sub(a.xact_start))
            } else {
                None
            };
            let backend_duration_s = if a.backend_start > 0 {
                Some(now.saturating_sub(a.backend_start))
            } else {
                None
            };

            // OS process enrichment
            let pid_u32 = a.pid as u32;
            let (
                cpu_pct,
                rss_kb,
                rchar_s,
                wchar_s,
                read_bytes_s,
                write_bytes_s,
                read_ops_s,
                write_ops_s,
            ) = if let Some(p) = processes.get(&pid_u32) {
                let prev_p = prev_procs.get(&pid_u32);
                let cpu = if let (Some(pp), Some(tc), Some(ptc), true) =
                    (prev_p, total_cpu, prev_total_cpu, has_prev)
                {
                    let d_proc =
                        (p.cpu.utime + p.cpu.stime).saturating_sub(pp.cpu.utime + pp.cpu.stime);
                    let d_total = tc.saturating_sub(ptc);
                    if d_total > 0 {
                        Some(d_proc as f64 / d_total as f64 * 100.0)
                    } else {
                        Some(0.0)
                    }
                } else {
                    None
                };
                let (rc_s, wc_s, rb_s, wb_s, ro_s, wo_s) =
                    if let (Some(pp), true) = (prev_p, has_prev) {
                        let inh = died_io.get(&pid_u32);
                        let a_rchar = inh.map_or(0, |d| d.rchar);
                        let a_wchar = inh.map_or(0, |d| d.wchar);
                        let a_rsz = inh.map_or(0, |d| d.rsz);
                        let a_wsz = inh.map_or(0, |d| d.wsz);
                        let a_rio = inh.map_or(0, |d| d.rio);
                        let a_wio = inh.map_or(0, |d| d.wio);
                        (
                            Some(
                                p.dsk
                                    .rchar
                                    .saturating_sub(pp.dsk.rchar)
                                    .saturating_sub(a_rchar) as f64
                                    / delta_time,
                            ),
                            Some(
                                p.dsk
                                    .wchar
                                    .saturating_sub(pp.dsk.wchar)
                                    .saturating_sub(a_wchar) as f64
                                    / delta_time,
                            ),
                            Some(
                                p.dsk.rsz.saturating_sub(pp.dsk.rsz).saturating_sub(a_rsz) as f64
                                    / delta_time,
                            ),
                            Some(
                                p.dsk.wsz.saturating_sub(pp.dsk.wsz).saturating_sub(a_wsz) as f64
                                    / delta_time,
                            ),
                            Some(
                                p.dsk.rio.saturating_sub(pp.dsk.rio).saturating_sub(a_rio) as f64
                                    / delta_time,
                            ),
                            Some(
                                p.dsk.wio.saturating_sub(pp.dsk.wio).saturating_sub(a_wio) as f64
                                    / delta_time,
                            ),
                        )
                    } else {
                        (None, None, None, None, None, None)
                    };
                (cpu, Some(p.mem.rmem), rc_s, wc_s, rb_s, wb_s, ro_s, wo_s)
            } else {
                (None, None, None, None, None, None, None, None)
            };

            // pg_stat_statements enrichment
            let (stmt_mean_exec_time_ms, stmt_max_exec_time_ms, stmt_calls_s, stmt_hit_pct) =
                if a.query_id != 0 {
                    let mean = stmts_by_qid.get(&a.query_id).map(|s| s.mean_exec_time);
                    let max = stmts_by_qid.get(&a.query_id).map(|s| s.max_exec_time);
                    let calls_s = pgs_rates.get(&a.query_id).and_then(|r| r.calls_s);
                    let hit_pct = {
                        let rate = pgs_rates.get(&a.query_id);
                        let rate_hit = rate.and_then(|r| r.shared_blks_hit_s);
                        let rate_read = rate.and_then(|r| r.shared_blks_read_s);
                        match (rate_hit, rate_read) {
                            (Some(h), Some(rd)) if h + rd > 0.0 => Some(h * 100.0 / (h + rd)),
                            _ => stmts_by_qid.get(&a.query_id).and_then(|s| {
                                let total = s.shared_blks_hit + s.shared_blks_read;
                                if total > 0 {
                                    Some(s.shared_blks_hit as f64 * 100.0 / total as f64)
                                } else {
                                    None
                                }
                            }),
                        }
                    };
                    (mean, max, calls_s, hit_pct)
                } else {
                    (None, None, None, None)
                };

            PgActivityRow {
                pid: a.pid,
                database: resolve(interner, a.datname_hash),
                user: resolve(interner, a.usename_hash),
                application_name: resolve(interner, a.application_name_hash),
                client_addr: a.client_addr.clone(),
                state: resolve(interner, a.state_hash),
                wait_event_type: resolve(interner, a.wait_event_type_hash),
                wait_event: resolve(interner, a.wait_event_hash),
                backend_type: resolve(interner, a.backend_type_hash),
                query: resolve(interner, a.query_hash),
                query_id: a.query_id,
                query_duration_s,
                xact_duration_s,
                backend_duration_s,
                backend_start: a.backend_start,
                xact_start: a.xact_start,
                query_start: a.query_start,
                cpu_pct,
                rss_kb,
                rchar_s,
                wchar_s,
                read_bytes_s,
                write_bytes_s,
                read_ops_s,
                write_ops_s,
                stmt_mean_exec_time_ms,
                stmt_max_exec_time_ms,
                stmt_calls_s,
                stmt_hit_pct,
            }
        })
        .collect()
}

// ============================================================
// PGS (pg_stat_statements)
// ============================================================

fn extract_pgs(
    snap: &Snapshot,
    interner: Option<&StringInterner>,
    rates: &HashMap<i64, PgStatementsRates>,
) -> Vec<PgStatementsRow> {
    let Some(stmts) = find_block(snap, |b| {
        if let DataBlock::PgStatStatements(v) = b {
            Some(v.as_slice())
        } else {
            None
        }
    }) else {
        return Vec::new();
    };

    stmts
        .iter()
        .map(|s| {
            let r = rates.get(&s.queryid);

            let rows_per_call = if s.calls > 0 {
                Some(s.rows as f64 / s.calls as f64)
            } else {
                None
            };

            // Prefer rate-based HIT% (delta over interval) over cumulative
            let hit_pct = {
                let rate_hit = r.and_then(|r| r.shared_blks_hit_s);
                let rate_read = r.and_then(|r| r.shared_blks_read_s);
                match (rate_hit, rate_read) {
                    (Some(h), Some(rd)) if h + rd > 0.0 => Some(h * 100.0 / (h + rd)),
                    (Some(_), Some(_)) => None, // rates available but no activity
                    _ => {
                        // Fallback to cumulative when no rates available
                        let total_blks = s.shared_blks_hit + s.shared_blks_read;
                        if total_blks > 0 {
                            Some(s.shared_blks_hit as f64 * 100.0 / total_blks as f64)
                        } else {
                            None
                        }
                    }
                }
            };

            PgStatementsRow {
                queryid: s.queryid,
                database: resolve(interner, s.datname_hash),
                user: resolve(interner, s.usename_hash),
                query: resolve(interner, s.query_hash),
                calls: s.calls,
                rows: s.rows,
                mean_exec_time_ms: s.mean_exec_time,
                min_exec_time_ms: s.min_exec_time,
                max_exec_time_ms: s.max_exec_time,
                stddev_exec_time_ms: s.stddev_exec_time,
                calls_s: r.and_then(|r| r.calls_s),
                rows_s: r.and_then(|r| r.rows_s),
                exec_time_ms_s: r.and_then(|r| r.exec_time_ms_s),
                shared_blks_read_s: r.and_then(|r| r.shared_blks_read_s),
                shared_blks_hit_s: r.and_then(|r| r.shared_blks_hit_s),
                shared_blks_dirtied_s: r.and_then(|r| r.shared_blks_dirtied_s),
                shared_blks_written_s: r.and_then(|r| r.shared_blks_written_s),
                local_blks_read_s: r.and_then(|r| r.local_blks_read_s),
                local_blks_written_s: r.and_then(|r| r.local_blks_written_s),
                temp_blks_read_s: r.and_then(|r| r.temp_blks_read_s),
                temp_blks_written_s: r.and_then(|r| r.temp_blks_written_s),
                temp_mb_s: r.and_then(|r| r.temp_mb_s),
                rows_per_call,
                hit_pct,
                total_plan_time: s.total_plan_time,
                wal_records: s.wal_records,
                wal_bytes: s.wal_bytes,
                total_exec_time: s.total_exec_time,
            }
        })
        .collect()
}

// ============================================================
// PGT (pg_stat_user_tables)
// ============================================================

fn extract_pgt(
    snap: &Snapshot,
    interner: Option<&StringInterner>,
    rates: &HashMap<u32, PgTablesRates>,
) -> Vec<PgTablesRow> {
    let Some(tables) = find_block(snap, |b| {
        if let DataBlock::PgStatUserTables(v) = b {
            Some(v.as_slice())
        } else {
            None
        }
    }) else {
        return Vec::new();
    };

    tables
        .iter()
        .map(|t| {
            let database = resolve(interner, t.datname_hash);
            let schema = resolve(interner, t.schemaname_hash);
            let table = resolve(interner, t.relname_hash);
            let display_name = if schema.is_empty() || schema == "public" {
                table.clone()
            } else {
                format!("{}.{}", schema, table)
            };

            let r = rates.get(&t.relid);

            let seq_tup_read_s = r.and_then(|r| r.seq_tup_read_s);
            let idx_tup_fetch_s = r.and_then(|r| r.idx_tup_fetch_s);
            let heap_blks_read_s = r.and_then(|r| r.heap_blks_read_s);
            let heap_blks_hit_s = r.and_then(|r| r.heap_blks_hit_s);
            let idx_blks_read_s = r.and_then(|r| r.idx_blks_read_s);
            let idx_blks_hit_s = r.and_then(|r| r.idx_blks_hit_s);
            let toast_blks_read_s = r.and_then(|r| r.toast_blks_read_s);
            let toast_blks_hit_s = r.and_then(|r| r.toast_blks_hit_s);
            let tidx_blks_read_s = r.and_then(|r| r.tidx_blks_read_s);
            let tidx_blks_hit_s = r.and_then(|r| r.tidx_blks_hit_s);

            // Computed: tot_tup_read_s = seq_tup_read_s + idx_tup_fetch_s
            let tot_tup_read_s = match (seq_tup_read_s, idx_tup_fetch_s) {
                (Some(a), Some(b)) => Some(a + b),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                _ => None,
            };

            // Computed: disk_blks_read_s = heap_blks_read_s + idx_blks_read_s
            let disk_blks_read_s = match (heap_blks_read_s, idx_blks_read_s) {
                (Some(a), Some(b)) => Some(a + b),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                _ => None,
            };

            // Computed: io_hit_pct from rates (falls back to cumulative if no rates)
            let io_hit_pct = {
                let rate_hits = add_opts(&[
                    heap_blks_hit_s,
                    idx_blks_hit_s,
                    toast_blks_hit_s,
                    tidx_blks_hit_s,
                ]);
                let rate_reads = add_opts(&[
                    heap_blks_read_s,
                    idx_blks_read_s,
                    toast_blks_read_s,
                    tidx_blks_read_s,
                ]);
                match (rate_hits, rate_reads) {
                    (Some(h), Some(rd)) if h + rd > 0.0 => Some(h * 100.0 / (h + rd)),
                    (Some(_), Some(_)) => None, // rates available but no activity
                    _ => {
                        // Fallback to cumulative when rates unavailable
                        let all_hits =
                            t.heap_blks_hit + t.idx_blks_hit + t.toast_blks_hit + t.tidx_blks_hit;
                        let all_reads = t.heap_blks_read
                            + t.idx_blks_read
                            + t.toast_blks_read
                            + t.tidx_blks_read;
                        let total = all_hits + all_reads;
                        if total > 0 {
                            Some(all_hits as f64 * 100.0 / total as f64)
                        } else {
                            None
                        }
                    }
                }
            };

            // Computed: seq_pct from rates (falls back to cumulative if no rates)
            let seq_scan_s = r.and_then(|r| r.seq_scan_s);
            let idx_scan_s = r.and_then(|r| r.idx_scan_s);
            let seq_pct = match (seq_scan_s, idx_scan_s) {
                (Some(ss), Some(is)) if ss + is > 0.0 => Some(ss * 100.0 / (ss + is)),
                (Some(_), Some(_)) => None, // rates available but no activity
                _ => {
                    let total_scans = t.seq_scan + t.idx_scan;
                    if total_scans > 0 {
                        Some(t.seq_scan as f64 * 100.0 / total_scans as f64)
                    } else {
                        None
                    }
                }
            };

            // Computed: dead_pct
            let dead_pct = {
                let total_tup = t.n_live_tup + t.n_dead_tup;
                if total_tup > 0 {
                    Some(t.n_dead_tup as f64 * 100.0 / total_tup as f64)
                } else {
                    None
                }
            };

            // Computed: hot_pct from cumulative values
            let hot_pct = if t.n_tup_upd > 0 {
                Some(t.n_tup_hot_upd as f64 * 100.0 / t.n_tup_upd as f64)
            } else {
                None
            };

            PgTablesRow {
                relid: t.relid,
                database,
                schema,
                table,
                display_name,
                n_live_tup: t.n_live_tup,
                n_dead_tup: t.n_dead_tup,
                size_bytes: t.size_bytes,
                last_autovacuum: t.last_autovacuum,
                last_autoanalyze: t.last_autoanalyze,
                seq_scan_s,
                seq_tup_read_s,
                idx_scan_s,
                idx_tup_fetch_s,
                n_tup_ins_s: r.and_then(|r| r.n_tup_ins_s),
                n_tup_upd_s: r.and_then(|r| r.n_tup_upd_s),
                n_tup_del_s: r.and_then(|r| r.n_tup_del_s),
                n_tup_hot_upd_s: r.and_then(|r| r.n_tup_hot_upd_s),
                vacuum_count_s: r.and_then(|r| r.vacuum_count_s),
                autovacuum_count_s: r.and_then(|r| r.autovacuum_count_s),
                heap_blks_read_s,
                heap_blks_hit_s,
                idx_blks_read_s,
                idx_blks_hit_s,
                tot_tup_read_s,
                disk_blks_read_s,
                io_hit_pct,
                seq_pct,
                dead_pct,
                hot_pct,
                analyze_count_s: r.and_then(|r| r.analyze_count_s),
                autoanalyze_count_s: r.and_then(|r| r.autoanalyze_count_s),
                last_vacuum: t.last_vacuum,
                last_analyze: t.last_analyze,
                toast_blks_read_s,
                toast_blks_hit_s,
                tidx_blks_read_s,
                tidx_blks_hit_s,
            }
        })
        .collect()
}

// ============================================================
// PGI (pg_stat_user_indexes)
// ============================================================

fn extract_pgi(
    snap: &Snapshot,
    interner: Option<&StringInterner>,
    rates: &HashMap<u32, PgIndexesRates>,
) -> Vec<PgIndexesRow> {
    let Some(indexes) = find_block(snap, |b| {
        if let DataBlock::PgStatUserIndexes(v) = b {
            Some(v.as_slice())
        } else {
            None
        }
    }) else {
        return Vec::new();
    };

    indexes
        .iter()
        .map(|i| {
            let database = resolve(interner, i.datname_hash);
            let schema = resolve(interner, i.schemaname_hash);
            let table = resolve(interner, i.relname_hash);
            let index = resolve(interner, i.indexrelname_hash);
            let display_table = if schema.is_empty() || schema == "public" {
                table.clone()
            } else {
                format!("{}.{}", schema, table)
            };

            let r = rates.get(&i.indexrelid);
            let idx_blks_read_s = r.and_then(|r| r.idx_blks_read_s);
            let idx_blks_hit_s = r.and_then(|r| r.idx_blks_hit_s);

            // Computed: io_hit_pct from rates (falls back to cumulative if no rates)
            let io_hit_pct = match (idx_blks_hit_s, idx_blks_read_s) {
                (Some(h), Some(rd)) if h + rd > 0.0 => Some(h * 100.0 / (h + rd)),
                (Some(_), Some(_)) => None, // rates available but no activity
                _ => {
                    let total = i.idx_blks_hit + i.idx_blks_read;
                    if total > 0 {
                        Some(i.idx_blks_hit as f64 * 100.0 / total as f64)
                    } else {
                        None
                    }
                }
            };

            PgIndexesRow {
                indexrelid: i.indexrelid,
                relid: i.relid,
                database,
                schema,
                table,
                index,
                display_table,
                idx_scan: i.idx_scan,
                size_bytes: i.size_bytes,
                idx_scan_s: r.and_then(|r| r.idx_scan_s),
                idx_tup_read_s: r.and_then(|r| r.idx_tup_read_s),
                idx_tup_fetch_s: r.and_then(|r| r.idx_tup_fetch_s),
                idx_blks_read_s,
                idx_blks_hit_s,
                io_hit_pct,
                disk_blks_read_s: idx_blks_read_s,
            }
        })
        .collect()
}

// ============================================================
// PGE (pg_log_events + errors)
// ============================================================

fn extract_pge(snap: &Snapshot, interner: Option<&StringInterner>) -> Vec<PgEventsRow> {
    let mut rows = Vec::new();
    // Sequential counter for unique event IDs. Must stay within JS Number.MAX_SAFE_INTEGER (2^53-1).
    let mut next_id: u64 = 1;

    // Errors from PgLogErrors
    if let Some(entries) = find_block(snap, |b| {
        if let DataBlock::PgLogErrors(v) = b {
            Some(v.as_slice())
        } else {
            None
        }
    }) {
        for e in entries {
            let severity_str = match e.severity {
                PgLogSeverity::Error => "ERROR",
                PgLogSeverity::Fatal => "FATAL",
                PgLogSeverity::Panic => "PANIC",
            };
            let event_id = next_id;
            next_id += 1;
            rows.push(PgEventsRow {
                event_id,
                event_type: severity_str.to_lowercase(),
                severity: severity_str.to_string(),
                count: e.count,
                table_name: String::new(),
                elapsed_s: 0.0,
                extra_num1: 0,
                extra_num2: 0,
                extra_num3: 0,
                buffer_hits: 0,
                buffer_misses: 0,
                buffer_dirtied: 0,
                avg_read_rate_mbs: 0.0,
                avg_write_rate_mbs: 0.0,
                cpu_user_s: 0.0,
                cpu_system_s: 0.0,
                wal_records: 0,
                wal_fpi: 0,
                wal_bytes: 0,
                message: resolve(interner, e.pattern_hash),
                sample: resolve(interner, e.sample_hash),
                statement: resolve(interner, e.statement_hash),
            });
        }
    }

    // Detailed events from PgLogDetailedEvents
    if let Some(events) = find_block(snap, |b| {
        if let DataBlock::PgLogDetailedEvents(v) = b {
            Some(v.as_slice())
        } else {
            None
        }
    }) {
        for ev in events {
            let event_type_str = match ev.event_type {
                PgLogEventType::CheckpointStarting => "checkpoint_starting",
                PgLogEventType::CheckpointComplete => "checkpoint_complete",
                PgLogEventType::Autovacuum => "autovacuum",
                PgLogEventType::Autoanalyze => "autoanalyze",
            };
            let event_id = next_id;
            next_id += 1;
            rows.push(PgEventsRow {
                event_id,
                event_type: event_type_str.to_string(),
                severity: "LOG".to_string(),
                count: 1,
                table_name: ev.table_name.clone(),
                elapsed_s: ev.elapsed_s,
                extra_num1: ev.extra_num1,
                extra_num2: ev.extra_num2,
                extra_num3: ev.extra_num3,
                buffer_hits: ev.buffer_hits,
                buffer_misses: ev.buffer_misses,
                buffer_dirtied: ev.buffer_dirtied,
                avg_read_rate_mbs: ev.avg_read_rate_mbs,
                avg_write_rate_mbs: ev.avg_write_rate_mbs,
                cpu_user_s: ev.cpu_user_s,
                cpu_system_s: ev.cpu_system_s,
                wal_records: ev.wal_records,
                wal_fpi: ev.wal_fpi,
                wal_bytes: ev.wal_bytes,
                message: ev.message.clone(),
                sample: String::new(),
                statement: String::new(),
            });
        }
    }

    rows
}

// ============================================================
// PGL (pg_locks tree)
// ============================================================

fn extract_pgl(snap: &Snapshot, interner: Option<&StringInterner>) -> Vec<PgLocksRow> {
    let Some(nodes) = find_block(snap, |b| {
        if let DataBlock::PgLockTree(v) = b {
            Some(v.as_slice())
        } else {
            None
        }
    }) else {
        return Vec::new();
    };

    nodes
        .iter()
        .map(|n| PgLocksRow {
            pid: n.pid,
            depth: n.depth,
            root_pid: n.root_pid,
            database: resolve(interner, n.datname_hash),
            user: resolve(interner, n.usename_hash),
            application_name: resolve(interner, n.application_name_hash),
            state: resolve(interner, n.state_hash),
            wait_event_type: resolve(interner, n.wait_event_type_hash),
            wait_event: resolve(interner, n.wait_event_hash),
            backend_type: resolve(interner, n.backend_type_hash),
            lock_type: resolve(interner, n.lock_type_hash),
            lock_mode: resolve(interner, n.lock_mode_hash),
            lock_target: resolve(interner, n.lock_target_hash),
            lock_granted: n.lock_granted,
            query: resolve(interner, n.query_hash),
            xact_start: n.xact_start,
            query_start: n.query_start,
            state_change: n.state_change,
        })
        .collect()
}
