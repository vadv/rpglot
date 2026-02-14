//! PostgreSQL lock tree detail popup widget.
//! Shows detailed information about a selected node in the lock tree.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;

use crate::storage::StringInterner;
use crate::storage::model::{DataBlock, PgLockTreeNode, Snapshot};
use crate::tui::state::{AppState, PopupState};

use super::detail_common::{
    format_epoch_age, kv, push_help, render_popup_frame, resolve_hash, section,
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
    lines.push(kv("Transaction Start", &format_epoch_age(node.xact_start)));
    push_help(&mut lines, show_help, HELP, "Transaction Start");
    lines.push(kv("Query Start", &format_epoch_age(node.query_start)));
    push_help(&mut lines, show_help, HELP, "Query Start");
    lines.push(kv("State Change", &format_epoch_age(node.state_change)));
    push_help(&mut lines, show_help, HELP, "State Change");

    // State section
    lines.push(section("State"));
    lines.push(kv("State", &r(node.state_hash)));
    push_help(&mut lines, show_help, HELP, "State");
    lines.push(kv("Wait Event Type", &r(node.wait_event_type_hash)));
    push_help(&mut lines, show_help, HELP, "Wait Event Type");
    lines.push(kv("Wait Event", &r(node.wait_event_hash)));
    push_help(&mut lines, show_help, HELP, "Wait Event");

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
