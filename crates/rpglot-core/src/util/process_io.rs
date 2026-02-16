//! Utilities for correcting process I/O accounting.
//!
//! When a child process exits, Linux adds its cumulative `/proc/[pid]/io`
//! counters (read_bytes, write_bytes, etc.) to the parent via `wait()`.
//! This causes phantom I/O spikes on supervisor processes (postmaster,
//! systemd, etc.) — the same bytes get counted twice: once on the child
//! while it was alive, and again on the parent after the child dies.
//!
//! [`compute_died_children_io`] detects which processes disappeared between
//! two snapshots and returns their cumulative I/O grouped by parent PID,
//! so callers can subtract it from raw deltas.
//!
//! Note: this only corrects for the child's I/O as of the prev snapshot.
//! Any additional I/O the child did between prev and death is unknowable
//! and will remain as residual noise on the parent. For supervisor processes
//! (postmaster, systemd) this residual can be significant — such processes
//! should be excluded from I/O anomaly detection.

use std::collections::{HashMap, HashSet};

use crate::storage::model::ProcessInfo;

/// Cumulative I/O counters inherited from died children.
#[derive(Debug, Default, Clone)]
pub struct DiedChildrenIo {
    pub rsz: u64,
    pub wsz: u64,
    pub rio: u64,
    pub wio: u64,
    pub rchar: u64,
    pub wchar: u64,
}

/// Computes cumulative I/O of processes that existed in `prev` but are
/// absent in `current`, grouped by their parent PID (ppid).
///
/// Returns a map: `ppid → DiedChildrenIo`.  For any parent PID not in
/// the returned map, no children died — no correction needed.
pub fn compute_died_children_io(
    current: &[ProcessInfo],
    prev: &[ProcessInfo],
) -> HashMap<u32, DiedChildrenIo> {
    let current_pids: HashSet<u32> = current.iter().map(|p| p.pid).collect();

    let mut result: HashMap<u32, DiedChildrenIo> = HashMap::new();

    for p in prev {
        if !current_pids.contains(&p.pid) {
            let entry = result.entry(p.ppid).or_default();
            entry.rsz += p.dsk.rsz;
            entry.wsz += p.dsk.wsz;
            entry.rio += p.dsk.rio;
            entry.wio += p.dsk.wio;
            entry.rchar += p.dsk.rchar;
            entry.wchar += p.dsk.wchar;
        }
    }

    result
}

/// Returns the set of PIDs that have at least one child in the given
/// process list. Used to identify supervisor processes whose I/O
/// accounting is unreliable due to Linux wait() inheritance.
pub fn find_parent_pids(processes: &[ProcessInfo]) -> HashSet<u32> {
    let all_pids: HashSet<u32> = processes.iter().map(|p| p.pid).collect();
    processes
        .iter()
        .filter(|p| all_pids.contains(&p.ppid))
        .map(|p| p.ppid)
        .collect()
}
