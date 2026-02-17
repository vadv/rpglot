//! SQL query builders for PostgreSQL statistics views.

/// Builds version-aware query for pg_stat_activity.
pub(super) fn build_stat_activity_query(server_version_num: Option<i32>) -> String {
    let query_id_expr = if server_version_num.unwrap_or(0) >= 140000 {
        "COALESCE(query_id, 0)::bigint as query_id"
    } else {
        "0::bigint as query_id"
    };

    format!(
        r#"
            SELECT
                pid,
                COALESCE(datname, '') as datname,
                COALESCE(usename, '') as usename,
                COALESCE(application_name, '') as application_name,
                COALESCE(client_addr::text, '') as client_addr,
                COALESCE(state, '') as state,
                COALESCE(query, '') as query,
                {query_id_expr},
                COALESCE(wait_event_type, '') as wait_event_type,
                COALESCE(wait_event, '') as wait_event,
                COALESCE(backend_type, '') as backend_type,
                COALESCE(EXTRACT(EPOCH FROM backend_start)::bigint, 0) as backend_start,
                COALESCE(EXTRACT(EPOCH FROM xact_start)::bigint, 0) as xact_start,
                COALESCE(EXTRACT(EPOCH FROM query_start)::bigint, 0) as query_start
            FROM pg_stat_activity
        "#
    )
}

/// Builds version-aware query for pg_stat_statements.
pub(super) fn build_stat_statements_query(server_version_num: Option<i32>) -> String {
    let v = server_version_num.unwrap_or(0);
    let (
        total_exec_time_expr,
        mean_exec_time_expr,
        min_exec_time_expr,
        max_exec_time_expr,
        stddev_exec_time_expr,
        total_plan_time_expr,
        wal_records_expr,
        wal_bytes_expr,
    ) = if v >= 130000 {
        (
            "s.total_exec_time",
            "s.mean_exec_time",
            "s.min_exec_time",
            "s.max_exec_time",
            "s.stddev_exec_time",
            "s.total_plan_time",
            "s.wal_records",
            "s.wal_bytes",
        )
    } else {
        (
            "s.total_time",
            "s.mean_time",
            "s.min_time",
            "s.max_time",
            "s.stddev_time",
            "0",
            "0",
            "0",
        )
    };

    format!(
        r#"
            SELECT
                s.userid,
                s.dbid,
                s.queryid,
                COALESCE(d.datname, '') as datname,
                COALESCE(r.rolname, '') as usename,
                COALESCE(s.query, '') as query,
                s.calls,
                {total_exec_time_expr}::double precision as total_exec_time,
                {mean_exec_time_expr}::double precision as mean_exec_time,
                {min_exec_time_expr}::double precision as min_exec_time,
                {max_exec_time_expr}::double precision as max_exec_time,
                {stddev_exec_time_expr}::double precision as stddev_exec_time,
                s.rows,
                s.shared_blks_read,
                s.shared_blks_hit,
                s.shared_blks_written,
                s.shared_blks_dirtied,
                s.local_blks_read,
                s.local_blks_written,
                s.temp_blks_read,
                s.temp_blks_written,
                {wal_records_expr}::bigint as wal_records,
                {wal_bytes_expr}::bigint as wal_bytes,
                {total_plan_time_expr}::double precision as total_plan_time
            FROM pg_stat_statements s
            LEFT JOIN pg_database d ON d.oid = s.dbid
            LEFT JOIN pg_roles r ON r.oid = s.userid
            ORDER BY total_exec_time DESC
            LIMIT 500
        "#
    )
}

/// Builds version-aware query for pg_stat_database.
pub(super) fn build_stat_database_query(server_version_num: Option<i32>) -> String {
    let v = server_version_num.unwrap_or(0);

    let (
        session_time_expr,
        active_time_expr,
        idle_in_transaction_time_expr,
        sessions_expr,
        sessions_abandoned_expr,
        sessions_fatal_expr,
        sessions_killed_expr,
    ) = if v >= 140000 {
        (
            "COALESCE(session_time, 0)::double precision",
            "COALESCE(active_time, 0)::double precision",
            "COALESCE(idle_in_transaction_time, 0)::double precision",
            "COALESCE(sessions, 0)::bigint",
            "COALESCE(sessions_abandoned, 0)::bigint",
            "COALESCE(sessions_fatal, 0)::bigint",
            "COALESCE(sessions_killed, 0)::bigint",
        )
    } else {
        (
            "0::double precision",
            "0::double precision",
            "0::double precision",
            "0::bigint",
            "0::bigint",
            "0::bigint",
            "0::bigint",
        )
    };

    format!(
        r#"
            SELECT
                datid,
                COALESCE(datname, '') as datname,
                COALESCE(xact_commit, 0) as xact_commit,
                COALESCE(xact_rollback, 0) as xact_rollback,
                COALESCE(blks_read, 0) as blks_read,
                COALESCE(blks_hit, 0) as blks_hit,
                COALESCE(tup_returned, 0) as tup_returned,
                COALESCE(tup_fetched, 0) as tup_fetched,
                COALESCE(tup_inserted, 0) as tup_inserted,
                COALESCE(tup_updated, 0) as tup_updated,
                COALESCE(tup_deleted, 0) as tup_deleted,
                COALESCE(conflicts, 0) as conflicts,
                COALESCE(temp_files, 0) as temp_files,
                COALESCE(temp_bytes, 0) as temp_bytes,
                COALESCE(deadlocks, 0) as deadlocks,
                COALESCE(checksum_failures, 0) as checksum_failures,
                COALESCE(blk_read_time, 0)::double precision as blk_read_time,
                COALESCE(blk_write_time, 0)::double precision as blk_write_time,
                {session_time_expr} as session_time,
                {active_time_expr} as active_time,
                {idle_in_transaction_time_expr} as idle_in_transaction_time,
                {sessions_expr} as sessions,
                {sessions_abandoned_expr} as sessions_abandoned,
                {sessions_fatal_expr} as sessions_fatal,
                {sessions_killed_expr} as sessions_killed
            FROM pg_stat_database
            WHERE datname IS NOT NULL
              AND datname NOT IN ('template0', 'template1')
        "#
    )
}

/// Builds query for pg_stat_user_tables.
///
/// All columns exist since PG 9.1+, no version check needed.
pub(super) fn build_stat_user_tables_query() -> &'static str {
    r#"
        SELECT
            relid::bigint,
            COALESCE(schemaname, '') as schemaname,
            COALESCE(relname, '') as relname,
            COALESCE(seq_scan, 0)::bigint as seq_scan,
            COALESCE(seq_tup_read, 0)::bigint as seq_tup_read,
            COALESCE(idx_scan, 0)::bigint as idx_scan,
            COALESCE(idx_tup_fetch, 0)::bigint as idx_tup_fetch,
            COALESCE(n_tup_ins, 0)::bigint as n_tup_ins,
            COALESCE(n_tup_upd, 0)::bigint as n_tup_upd,
            COALESCE(n_tup_del, 0)::bigint as n_tup_del,
            COALESCE(n_tup_hot_upd, 0)::bigint as n_tup_hot_upd,
            COALESCE(n_live_tup, 0)::bigint as n_live_tup,
            COALESCE(n_dead_tup, 0)::bigint as n_dead_tup,
            COALESCE(vacuum_count, 0)::bigint as vacuum_count,
            COALESCE(autovacuum_count, 0)::bigint as autovacuum_count,
            COALESCE(analyze_count, 0)::bigint as analyze_count,
            COALESCE(autoanalyze_count, 0)::bigint as autoanalyze_count,
            COALESCE(EXTRACT(EPOCH FROM last_vacuum)::bigint, 0) as last_vacuum,
            COALESCE(EXTRACT(EPOCH FROM last_autovacuum)::bigint, 0) as last_autovacuum,
            COALESCE(EXTRACT(EPOCH FROM last_analyze)::bigint, 0) as last_analyze,
            COALESCE(EXTRACT(EPOCH FROM last_autoanalyze)::bigint, 0) as last_autoanalyze,
            COALESCE(pg_relation_size(relid), 0)::bigint as size_bytes
        FROM pg_stat_user_tables
        ORDER BY COALESCE(seq_scan, 0) + COALESCE(idx_scan, 0) DESC
        LIMIT 500
    "#
}

/// Builds query for pg_stat_user_indexes.
///
/// All columns exist since PG 9.1+, no version check needed.
/// Includes pg_relation_size() for index size.
pub(super) fn build_stat_user_indexes_query() -> &'static str {
    r#"
        SELECT
            i.indexrelid::bigint,
            i.relid::bigint,
            COALESCE(i.schemaname, '') as schemaname,
            COALESCE(i.relname, '') as relname,
            COALESCE(i.indexrelname, '') as indexrelname,
            COALESCE(i.idx_scan, 0)::bigint as idx_scan,
            COALESCE(i.idx_tup_read, 0)::bigint as idx_tup_read,
            COALESCE(i.idx_tup_fetch, 0)::bigint as idx_tup_fetch,
            COALESCE(pg_relation_size(i.indexrelid), 0)::bigint as size_bytes
        FROM pg_stat_user_indexes i
        ORDER BY COALESCE(i.idx_scan, 0) DESC
        LIMIT 500
    "#
}

/// Builds a recursive CTE query for the PostgreSQL lock tree.
///
/// Returns blocking chains as a flat tree in DFS order.
/// Each row represents a session participating in a blocking relationship.
/// Compatible with PostgreSQL 10+ (uses `pg_blocking_pids()` from PG 9.6+).
///
/// Returns 0 rows when there are no blocking chains (fast path).
pub(super) fn build_lock_tree_query() -> &'static str {
    r#"
        WITH RECURSIVE activity AS (
            SELECT
                a.pid,
                pg_blocking_pids(a.pid) AS blocked_by,
                COALESCE(a.datname, '') AS datname,
                COALESCE(a.usename, '') AS usename,
                COALESCE(a.state, '') AS state,
                COALESCE(a.wait_event_type, '') AS wait_event_type,
                COALESCE(a.wait_event, '') AS wait_event,
                COALESCE(a.query, '') AS query,
                COALESCE(a.application_name, '') AS application_name,
                COALESCE(a.backend_type, '') AS backend_type,
                COALESCE(EXTRACT(EPOCH FROM a.xact_start)::bigint, 0) AS xact_start,
                COALESCE(EXTRACT(EPOCH FROM a.query_start)::bigint, 0) AS query_start,
                COALESCE(EXTRACT(EPOCH FROM a.state_change)::bigint, 0) AS state_change
            FROM pg_stat_activity a
            WHERE a.state IS DISTINCT FROM 'idle'
        ),
        blockers AS (
            SELECT array_agg(DISTINCT c ORDER BY c) AS pids
            FROM (SELECT unnest(blocked_by) AS c FROM activity) dt
        ),
        tree AS (
            SELECT
                activity.*,
                1 AS depth,
                activity.pid AS root_pid,
                ARRAY[activity.pid] AS path,
                ARRAY[activity.pid]::int[] AS all_above
            FROM activity, blockers
            WHERE ARRAY[activity.pid] <@ blockers.pids
              AND activity.blocked_by = '{}'::int[]
            UNION ALL
            SELECT
                activity.*,
                tree.depth + 1,
                tree.root_pid,
                tree.path || activity.pid,
                tree.all_above || array_agg(activity.pid) OVER ()
            FROM activity, tree
            WHERE activity.blocked_by <> '{}'::int[]
              AND activity.blocked_by <@ tree.all_above
              AND NOT ARRAY[activity.pid] <@ tree.all_above
        ),
        lock_info AS (
            SELECT DISTINCT ON (l.pid)
                l.pid,
                COALESCE(l.locktype, '') AS lock_type,
                COALESCE(l.mode, '') AS lock_mode,
                l.granted AS lock_granted,
                COALESCE(n.nspname || '.' || c.relname, l.relation::text, '') AS lock_target
            FROM pg_locks l
            LEFT JOIN pg_class c ON c.oid = l.relation
            LEFT JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE l.pid IN (SELECT pid FROM tree)
            ORDER BY l.pid, l.granted ASC, l.relation NULLS LAST
        )
        SELECT
            t.pid,
            t.depth,
            t.root_pid,
            t.datname,
            t.usename,
            t.state,
            t.wait_event_type,
            t.wait_event,
            t.query,
            t.application_name,
            t.backend_type,
            t.xact_start,
            t.query_start,
            t.state_change,
            COALESCE(li.lock_type, '') AS lock_type,
            COALESCE(li.lock_mode, '') AS lock_mode,
            COALESCE(li.lock_granted, true) AS lock_granted,
            COALESCE(li.lock_target, '') AS lock_target
        FROM tree t
        LEFT JOIN lock_info li ON li.pid = t.pid
        ORDER BY t.root_pid, t.path
    "#
}

/// Builds version-aware query for pg_stat_bgwriter (+ pg_stat_checkpointer on PG 17+).
///
/// PG < 17: all fields in pg_stat_bgwriter (single view).
/// PG 17+:  bgwriter fields from pg_stat_bgwriter,
///          checkpoint fields from pg_stat_checkpointer.
///          buffers_backend/buffers_backend_fsync default to 0 (moved to pg_stat_io).
pub(super) fn build_stat_bgwriter_query(server_version_num: Option<i32>) -> String {
    let v = server_version_num.unwrap_or(0);

    if v >= 170000 {
        r#"
            SELECT
                COALESCE(c.num_timed, 0)::bigint AS checkpoints_timed,
                COALESCE(c.num_requested, 0)::bigint AS checkpoints_req,
                COALESCE(c.write_time, 0)::double precision AS checkpoint_write_time,
                COALESCE(c.sync_time, 0)::double precision AS checkpoint_sync_time,
                COALESCE(c.buffers_written, 0)::bigint AS buffers_checkpoint,
                COALESCE(b.buffers_clean, 0)::bigint AS buffers_clean,
                COALESCE(b.maxwritten_clean, 0)::bigint AS maxwritten_clean,
                0::bigint AS buffers_backend,
                0::bigint AS buffers_backend_fsync,
                COALESCE(b.buffers_alloc, 0)::bigint AS buffers_alloc
            FROM pg_stat_bgwriter b
            CROSS JOIN pg_stat_checkpointer c
        "#
        .to_string()
    } else {
        r#"
            SELECT
                COALESCE(checkpoints_timed, 0)::bigint AS checkpoints_timed,
                COALESCE(checkpoints_req, 0)::bigint AS checkpoints_req,
                COALESCE(checkpoint_write_time, 0)::double precision AS checkpoint_write_time,
                COALESCE(checkpoint_sync_time, 0)::double precision AS checkpoint_sync_time,
                COALESCE(buffers_checkpoint, 0)::bigint AS buffers_checkpoint,
                COALESCE(buffers_clean, 0)::bigint AS buffers_clean,
                COALESCE(maxwritten_clean, 0)::bigint AS maxwritten_clean,
                COALESCE(buffers_backend, 0)::bigint AS buffers_backend,
                COALESCE(buffers_backend_fsync, 0)::bigint AS buffers_backend_fsync,
                COALESCE(buffers_alloc, 0)::bigint AS buffers_alloc
            FROM pg_stat_bgwriter
        "#
        .to_string()
    }
}

/// Builds query for pg_statio_user_tables (I/O block counters).
///
/// All columns exist since PG 7.2+, no version check needed.
/// Returns relid + 8 I/O counter columns for merge with pg_stat_user_tables by relid.
pub(super) fn build_statio_user_tables_query() -> &'static str {
    r#"
        SELECT
            relid::bigint,
            COALESCE(heap_blks_read, 0)::bigint as heap_blks_read,
            COALESCE(heap_blks_hit, 0)::bigint as heap_blks_hit,
            COALESCE(idx_blks_read, 0)::bigint as idx_blks_read,
            COALESCE(idx_blks_hit, 0)::bigint as idx_blks_hit,
            COALESCE(toast_blks_read, 0)::bigint as toast_blks_read,
            COALESCE(toast_blks_hit, 0)::bigint as toast_blks_hit,
            COALESCE(tidx_blks_read, 0)::bigint as tidx_blks_read,
            COALESCE(tidx_blks_hit, 0)::bigint as tidx_blks_hit
        FROM pg_statio_user_tables
        ORDER BY COALESCE(heap_blks_read, 0) + COALESCE(idx_blks_read, 0) DESC
        LIMIT 500
    "#
}

pub(super) fn build_statio_user_indexes_query() -> &'static str {
    r#"
        SELECT
            indexrelid::bigint,
            COALESCE(idx_blks_read, 0)::bigint as idx_blks_read,
            COALESCE(idx_blks_hit, 0)::bigint as idx_blks_hit
        FROM pg_statio_user_indexes
        ORDER BY COALESCE(idx_blks_read, 0) DESC
        LIMIT 500
    "#
}

/// Builds version-aware query for pg_stat_progress_vacuum (PG 9.6+).
///
/// PG < 17: uses original column names (max_dead_tuples, num_dead_tuples).
/// PG 17+:  renamed columns (max_dead_tuple_bytes, num_dead_item_ids) +
///          new columns (dead_tuple_bytes, indexes_total, indexes_processed).
pub(super) fn build_stat_progress_vacuum_query(server_version_num: Option<i32>) -> String {
    let v = server_version_num.unwrap_or(0);

    if v >= 170000 {
        r#"
            SELECT
                pid,
                COALESCE(datname, '') as datname,
                relid::bigint,
                COALESCE(phase, '') as phase,
                heap_blks_total,
                heap_blks_scanned,
                heap_blks_vacuumed,
                index_vacuum_count,
                max_dead_tuple_bytes as max_dead_tuples,
                num_dead_item_ids as num_dead_tuples,
                dead_tuple_bytes,
                indexes_total,
                indexes_processed
            FROM pg_stat_progress_vacuum
        "#
        .to_string()
    } else {
        r#"
            SELECT
                pid,
                COALESCE(datname, '') as datname,
                relid::bigint,
                COALESCE(phase, '') as phase,
                heap_blks_total,
                heap_blks_scanned,
                heap_blks_vacuumed,
                index_vacuum_count,
                max_dead_tuples,
                num_dead_tuples,
                0::bigint as dead_tuple_bytes,
                0::bigint as indexes_total,
                0::bigint as indexes_processed
            FROM pg_stat_progress_vacuum
        "#
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_activity_query_includes_query_id_on_pg14_plus() {
        let q = build_stat_activity_query(Some(140000));
        assert!(q.contains("COALESCE(query_id, 0)::bigint as query_id"));
    }

    #[test]
    fn stat_activity_query_uses_zero_query_id_on_pg13_and_older() {
        let q = build_stat_activity_query(Some(130000));
        assert!(q.contains("0::bigint as query_id"));
        assert!(!q.contains("COALESCE(query_id"));
    }

    #[test]
    fn stat_statements_query_uses_exec_time_columns_on_pg13_plus() {
        let q = build_stat_statements_query(Some(130000));
        assert!(q.contains("s.total_exec_time::double precision as total_exec_time"));
        assert!(q.contains("s.mean_exec_time::double precision as mean_exec_time"));
        assert!(q.contains("s.total_plan_time::double precision as total_plan_time"));
        assert!(q.contains("s.wal_records::bigint as wal_records"));
        assert!(q.contains("s.wal_bytes::bigint as wal_bytes"));
        assert!(q.contains("LEFT JOIN pg_database"));
        assert!(q.contains("LEFT JOIN pg_roles"));
        assert!(q.contains("as datname"));
        assert!(q.contains("as usename"));
    }

    #[test]
    fn stat_statements_query_uses_legacy_time_columns_on_pg12_and_older() {
        let q = build_stat_statements_query(Some(120000));
        assert!(q.contains("s.total_time::double precision as total_exec_time"));
        assert!(q.contains("s.mean_time::double precision as mean_exec_time"));
        assert!(q.contains("0::double precision as total_plan_time"));
        assert!(q.contains("0::bigint as wal_records"));
        assert!(q.contains("0::bigint as wal_bytes"));
        assert!(q.contains("LEFT JOIN pg_database"));
        assert!(q.contains("LEFT JOIN pg_roles"));
        assert!(q.contains("as datname"));
        assert!(q.contains("as usename"));
    }

    #[test]
    fn stat_bgwriter_query_pg16_uses_single_view() {
        let q = build_stat_bgwriter_query(Some(160000));
        assert!(q.contains("FROM pg_stat_bgwriter"));
        assert!(!q.contains("pg_stat_checkpointer"));
        assert!(q.contains("buffers_backend"));
    }

    #[test]
    fn stat_bgwriter_query_pg17_uses_split_views() {
        let q = build_stat_bgwriter_query(Some(170000));
        assert!(q.contains("pg_stat_checkpointer"));
        assert!(q.contains("pg_stat_bgwriter"));
        assert!(q.contains("num_timed"));
        assert!(q.contains("0::bigint AS buffers_backend"));
    }

    #[test]
    fn statio_user_tables_query_selects_io_counters() {
        let q = build_statio_user_tables_query();
        assert!(q.contains("pg_statio_user_tables"));
        assert!(q.contains("heap_blks_read"));
        assert!(q.contains("heap_blks_hit"));
        assert!(q.contains("tidx_blks_hit"));
        assert!(q.contains("LIMIT 500"));
    }

    #[test]
    fn statio_user_indexes_query_selects_io_counters() {
        let q = build_statio_user_indexes_query();
        assert!(q.contains("pg_statio_user_indexes"));
        assert!(q.contains("idx_blks_read"));
        assert!(q.contains("idx_blks_hit"));
        assert!(q.contains("indexrelid"));
        assert!(q.contains("LIMIT 500"));
    }

    #[test]
    fn progress_vacuum_query_pg16_uses_original_columns() {
        let q = build_stat_progress_vacuum_query(Some(160000));
        assert!(q.contains("pg_stat_progress_vacuum"));
        assert!(q.contains("max_dead_tuples"));
        assert!(q.contains("num_dead_tuples"));
        assert!(q.contains("0::bigint as dead_tuple_bytes"));
        assert!(q.contains("0::bigint as indexes_total"));
        assert!(q.contains("0::bigint as indexes_processed"));
        assert!(!q.contains("max_dead_tuple_bytes"));
        assert!(!q.contains("num_dead_item_ids"));
    }

    #[test]
    fn progress_vacuum_query_pg17_uses_renamed_columns() {
        let q = build_stat_progress_vacuum_query(Some(170000));
        assert!(q.contains("pg_stat_progress_vacuum"));
        assert!(q.contains("max_dead_tuple_bytes as max_dead_tuples"));
        assert!(q.contains("num_dead_item_ids as num_dead_tuples"));
        assert!(q.contains("dead_tuple_bytes"));
        assert!(q.contains("indexes_total"));
        assert!(q.contains("indexes_processed"));
    }
}
