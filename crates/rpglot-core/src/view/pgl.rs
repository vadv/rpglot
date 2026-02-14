//! PGL (pg_locks tree) view model.

use crate::fmt::{format_epoch_age, normalize_query};
use crate::storage::StringInterner;
use crate::storage::model::{DataBlock, PgLockTreeNode, Snapshot};
use crate::tui::state::PgLocksTabState;
use crate::view::common::{RowStyleClass, TableViewModel, ViewCell, ViewRow};

const HEADERS: &[&str] = &[
    "PID",
    "STATE",
    "WAIT",
    "DURATION",
    "LOCK_MODE",
    "TARGET",
    "QUERY",
];
const WIDTHS: &[u16] = &[12, 20, 20, 10, 12, 24];

/// Shortens PostgreSQL lock mode names for table display.
fn short_lock_mode(mode: &str) -> &str {
    match mode {
        "AccessShareLock" => "AccShr",
        "RowShareLock" => "RowShr",
        "RowExclusiveLock" => "RowExcl",
        "ShareUpdateExclusiveLock" => "ShrUpdExcl",
        "ShareLock" => "Share",
        "ShareRowExclusiveLock" => "ShrRowExcl",
        "ExclusiveLock" => "Excl",
        "AccessExclusiveLock" => "AccExcl",
        "" => "-",
        other => other,
    }
}

/// Formats PID with depth-based dot indentation.
fn format_pid_with_depth(pid: i32, depth: i32) -> String {
    if depth <= 1 {
        format!("{}", pid)
    } else {
        let dots: String = ".".repeat((depth - 1) as usize);
        format!("{}{}", dots, pid)
    }
}

struct LockRow {
    pid: i32,
    depth: i32,
    pid_display: String,
    state: String,
    wait: String,
    duration: String,
    lock_mode: String,
    target: String,
    query: String,
    lock_granted: bool,
}

/// Builds a UI-agnostic view model for the PGL (lock tree) tab.
///
/// Returns `None` if there is no snapshot data or no blocking chains.
pub fn build_locks_view(
    snapshot: &Snapshot,
    state: &PgLocksTabState,
    interner: Option<&StringInterner>,
) -> Option<TableViewModel<i32>> {
    let nodes: &[PgLockTreeNode] = snapshot
        .blocks
        .iter()
        .find_map(|b| {
            if let DataBlock::PgLockTree(v) = b {
                Some(v.as_slice())
            } else {
                None
            }
        })
        .unwrap_or(&[]);

    if nodes.is_empty() {
        return None;
    }

    let resolve = |hash: u64| -> String {
        if hash == 0 {
            return "-".to_string();
        }
        interner
            .and_then(|i| i.resolve(hash))
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("#{:x}", hash))
    };

    let mut rows_data: Vec<LockRow> = nodes
        .iter()
        .map(|n| {
            let state_str = resolve(n.state_hash);
            let wet = resolve(n.wait_event_type_hash);
            let we = resolve(n.wait_event_hash);
            let wait = if wet == "-" && we == "-" {
                "-".to_string()
            } else {
                format!("{}:{}", wet, we)
            };

            let duration = if n.depth <= 1 {
                format_epoch_age(n.xact_start)
            } else {
                format_epoch_age(n.state_change)
            };

            let lock_mode_full = resolve(n.lock_mode_hash);
            let lock_mode = short_lock_mode(&lock_mode_full).to_string();

            let lock_target = resolve(n.lock_target_hash);
            let lock_type = resolve(n.lock_type_hash);
            let target = if lock_target == "-" || lock_target.is_empty() {
                lock_type
            } else {
                lock_target
            };

            let query_raw = resolve(n.query_hash);
            let query = normalize_query(&query_raw);

            LockRow {
                pid: n.pid,
                depth: n.depth,
                pid_display: format_pid_with_depth(n.pid, n.depth),
                state: state_str,
                wait,
                duration,
                lock_mode,
                target,
                query,
                lock_granted: n.lock_granted,
            }
        })
        .collect();

    // Apply filter
    if let Some(ref filter) = state.filter {
        let f = filter.to_lowercase();
        rows_data.retain(|r| {
            r.query.to_lowercase().contains(&f)
                || r.target.to_lowercase().contains(&f)
                || r.pid.to_string().contains(&f)
                || r.state.to_lowercase().contains(&f)
        });
    }

    if rows_data.is_empty() {
        return None;
    }

    // Build view rows
    let rows: Vec<ViewRow<i32>> = rows_data
        .iter()
        .map(|r| {
            let style = if r.depth <= 1 {
                RowStyleClass::Critical
            } else if !r.lock_granted {
                RowStyleClass::Warning
            } else {
                RowStyleClass::Normal
            };

            ViewRow {
                id: r.pid,
                cells: vec![
                    ViewCell::plain(r.pid_display.clone()),
                    ViewCell::plain(r.state.clone()),
                    ViewCell::plain(r.wait.clone()),
                    ViewCell::plain(r.duration.clone()),
                    ViewCell::plain(r.lock_mode.clone()),
                    ViewCell::plain(r.target.clone()),
                    ViewCell::plain(r.query.clone()),
                ],
                style,
            }
        })
        .collect();

    let filter_info = state
        .filter
        .as_ref()
        .map(|f| format!(" [filter: {}]", f))
        .unwrap_or_default();

    let title = format!("PGL: Lock Tree ({} rows){}", rows.len(), filter_info);

    Some(TableViewModel {
        title,
        headers: HEADERS.iter().map(|s| s.to_string()).collect(),
        widths: WIDTHS.to_vec(),
        rows,
        sort_column: 0,
        sort_ascending: false,
    })
}
