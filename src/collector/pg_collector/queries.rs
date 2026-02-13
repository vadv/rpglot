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
            relid,
            COALESCE(schemaname, '') as schemaname,
            COALESCE(relname, '') as relname,
            COALESCE(seq_scan, 0) as seq_scan,
            COALESCE(seq_tup_read, 0) as seq_tup_read,
            COALESCE(idx_scan, 0) as idx_scan,
            COALESCE(idx_tup_fetch, 0) as idx_tup_fetch,
            COALESCE(n_tup_ins, 0) as n_tup_ins,
            COALESCE(n_tup_upd, 0) as n_tup_upd,
            COALESCE(n_tup_del, 0) as n_tup_del,
            COALESCE(n_tup_hot_upd, 0) as n_tup_hot_upd,
            COALESCE(n_live_tup, 0) as n_live_tup,
            COALESCE(n_dead_tup, 0) as n_dead_tup,
            COALESCE(vacuum_count, 0) as vacuum_count,
            COALESCE(autovacuum_count, 0) as autovacuum_count,
            COALESCE(analyze_count, 0) as analyze_count,
            COALESCE(autoanalyze_count, 0) as autoanalyze_count,
            COALESCE(EXTRACT(EPOCH FROM last_vacuum)::bigint, 0) as last_vacuum,
            COALESCE(EXTRACT(EPOCH FROM last_autovacuum)::bigint, 0) as last_autovacuum,
            COALESCE(EXTRACT(EPOCH FROM last_analyze)::bigint, 0) as last_analyze,
            COALESCE(EXTRACT(EPOCH FROM last_autoanalyze)::bigint, 0) as last_autoanalyze
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
            i.indexrelid,
            i.relid,
            COALESCE(i.schemaname, '') as schemaname,
            COALESCE(i.relname, '') as relname,
            COALESCE(i.indexrelname, '') as indexrelname,
            COALESCE(i.idx_scan, 0) as idx_scan,
            COALESCE(i.idx_tup_read, 0) as idx_tup_read,
            COALESCE(i.idx_tup_fetch, 0) as idx_tup_fetch,
            COALESCE(pg_relation_size(i.indexrelid), 0) as size_bytes
        FROM pg_stat_user_indexes i
        ORDER BY COALESCE(i.idx_scan, 0) DESC
        LIMIT 500
    "#
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
}
