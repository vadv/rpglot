//! pg_stat_progress_vacuum collection (PG 9.6+).

use crate::storage::interner::StringInterner;
use crate::storage::model::PgStatProgressVacuumInfo;

use super::PostgresCollector;
use super::queries::build_stat_progress_vacuum_query;

impl PostgresCollector {
    /// Collects pg_stat_progress_vacuum data.
    ///
    /// Returns a vector of currently running VACUUM operations.
    /// Empty vector when no vacuums are running or on error.
    pub fn collect_progress_vacuum(
        &mut self,
        interner: &mut StringInterner,
    ) -> Vec<PgStatProgressVacuumInfo> {
        if let Err(e) = self.ensure_connected() {
            self.last_error = Some(e.to_string());
            return Vec::new();
        }

        let client = self.client.as_mut().unwrap();
        let query = build_stat_progress_vacuum_query(self.server_version_num);

        match client.query(&query, &[]) {
            Ok(rows) => {
                self.last_error = None;
                rows.iter()
                    .map(|row| {
                        let datname: String = row.get("datname");
                        let phase: String = row.get("phase");

                        PgStatProgressVacuumInfo {
                            pid: row.get("pid"),
                            datname_hash: interner.intern(&datname),
                            relid: row.get("relid"),
                            phase_hash: interner.intern(&phase),
                            heap_blks_total: row.get("heap_blks_total"),
                            heap_blks_scanned: row.get("heap_blks_scanned"),
                            heap_blks_vacuumed: row.get("heap_blks_vacuumed"),
                            index_vacuum_count: row.get("index_vacuum_count"),
                            max_dead_tuples: row.get("max_dead_tuples"),
                            num_dead_tuples: row.get("num_dead_tuples"),
                            dead_tuple_bytes: row.get("dead_tuple_bytes"),
                            indexes_total: row.get("indexes_total"),
                            indexes_processed: row.get("indexes_processed"),
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
