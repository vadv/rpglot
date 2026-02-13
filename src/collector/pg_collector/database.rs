//! pg_stat_database collection.

use crate::storage::interner::StringInterner;
use crate::storage::model::PgStatDatabaseInfo;

use super::PostgresCollector;
use super::queries::build_stat_database_query;

impl PostgresCollector {
    /// Collects pg_stat_database data.
    ///
    /// Returns a vector of per-database statistics. On error, stores the error
    /// message in `last_error` and returns empty vector.
    /// No caching — pg_stat_database is small and changes frequently.
    pub fn collect_database(&mut self, interner: &mut StringInterner) -> Vec<PgStatDatabaseInfo> {
        if let Err(e) = self.ensure_connected() {
            self.last_error = Some(e.to_string());
            return Vec::new();
        }

        let client = self.client.as_mut().unwrap();
        let query = build_stat_database_query(self.server_version_num);

        match client.query(&query, &[]) {
            Ok(rows) => {
                self.last_error = None;
                rows.iter()
                    .map(|row| {
                        let datname: String = row.get("datname");
                        PgStatDatabaseInfo {
                            datid: row.get("datid"),
                            datname_hash: interner.intern(&datname),
                            xact_commit: row.get("xact_commit"),
                            xact_rollback: row.get("xact_rollback"),
                            blks_read: row.get("blks_read"),
                            blks_hit: row.get("blks_hit"),
                            tup_returned: row.get("tup_returned"),
                            tup_fetched: row.get("tup_fetched"),
                            tup_inserted: row.get("tup_inserted"),
                            tup_updated: row.get("tup_updated"),
                            tup_deleted: row.get("tup_deleted"),
                            conflicts: row.get("conflicts"),
                            temp_files: row.get("temp_files"),
                            temp_bytes: row.get("temp_bytes"),
                            deadlocks: row.get("deadlocks"),
                            checksum_failures: row.get("checksum_failures"),
                            blk_read_time: row.get("blk_read_time"),
                            blk_write_time: row.get("blk_write_time"),
                            session_time: row.get("session_time"),
                            active_time: row.get("active_time"),
                            idle_in_transaction_time: row.get("idle_in_transaction_time"),
                            sessions: row.get("sessions"),
                            sessions_abandoned: row.get("sessions_abandoned"),
                            sessions_fatal: row.get("sessions_fatal"),
                            sessions_killed: row.get("sessions_killed"),
                        }
                    })
                    .collect()
            }
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
