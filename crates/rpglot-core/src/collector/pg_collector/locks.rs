//! PostgreSQL lock tree collection.

use crate::storage::interner::StringInterner;
use crate::storage::model::PgLockTreeNode;

use super::PostgresCollector;
use super::queries::build_lock_tree_query;

impl PostgresCollector {
    /// Collects the PostgreSQL lock tree (blocking chains).
    ///
    /// Uses a recursive CTE on pg_locks + pg_stat_activity + pg_blocking_pids().
    /// Returns an empty vector when there are no blocking chains or on error.
    /// No caching — lock state changes rapidly.
    pub fn collect_lock_tree(&mut self, interner: &mut StringInterner) -> Vec<PgLockTreeNode> {
        if let Err(e) = self.ensure_connected() {
            self.last_error = Some(e.to_string());
            return Vec::new();
        }

        let client = self.client.as_mut().unwrap();
        let query = build_lock_tree_query();

        match client.query(query, &[]) {
            Ok(rows) => rows
                .iter()
                .filter_map(|row| parse_lock_tree_row(row, interner))
                .collect(),
            Err(e) => {
                let msg = super::format_postgres_error(&e);
                self.last_error = Some(msg);
                self.client = None;
                self.server_version_num = None;
                self.statements_ext_version = None;
                self.statements_last_check = None;
                Vec::new()
            }
        }
    }
}

/// Safely parses a single row from the lock tree query.
/// Returns None if critical columns fail to deserialize.
fn parse_lock_tree_row(
    row: &postgres::Row,
    interner: &mut StringInterner,
) -> Option<PgLockTreeNode> {
    let pid: i32 = row.try_get(0).ok()?;
    let depth: i32 = row.try_get(1).unwrap_or(1);
    let root_pid: i32 = row.try_get(2).unwrap_or(pid);

    let datname: String = row.try_get(3).unwrap_or_default();
    let usename: String = row.try_get(4).unwrap_or_default();
    let state: String = row.try_get(5).unwrap_or_default();
    let wait_event_type: String = row.try_get(6).unwrap_or_default();
    let wait_event: String = row.try_get(7).unwrap_or_default();
    let query: String = row.try_get(8).unwrap_or_default();
    let application_name: String = row.try_get(9).unwrap_or_default();
    let backend_type: String = row.try_get(10).unwrap_or_default();

    let xact_start: i64 = row.try_get(11).unwrap_or(0);
    let query_start: i64 = row.try_get(12).unwrap_or(0);
    let state_change: i64 = row.try_get(13).unwrap_or(0);

    let lock_type: String = row.try_get(14).unwrap_or_default();
    let lock_mode: String = row.try_get(15).unwrap_or_default();
    let lock_granted: bool = row.try_get(16).unwrap_or(true);
    let lock_target: String = row.try_get(17).unwrap_or_default();

    Some(PgLockTreeNode {
        pid,
        depth,
        root_pid,
        datname_hash: interner.intern(&datname),
        usename_hash: interner.intern(&usename),
        state_hash: interner.intern(&state),
        wait_event_type_hash: interner.intern(&wait_event_type),
        wait_event_hash: interner.intern(&wait_event),
        query_hash: interner.intern(&query),
        application_name_hash: interner.intern(&application_name),
        backend_type_hash: interner.intern(&backend_type),
        xact_start,
        query_start,
        state_change,
        lock_type_hash: interner.intern(&lock_type),
        lock_mode_hash: interner.intern(&lock_mode),
        lock_granted,
        lock_target_hash: interner.intern(&lock_target),
    })
}
