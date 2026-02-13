//! pg_stat_bgwriter collection.

use crate::storage::model::PgStatBgwriterInfo;

use super::PostgresCollector;
use super::queries::build_stat_bgwriter_query;

impl PostgresCollector {
    /// Collects pg_stat_bgwriter (+ pg_stat_checkpointer on PG 17+) data.
    ///
    /// Returns singleton bgwriter stats. On error, stores the error
    /// message in `last_error` and returns None.
    pub fn collect_bgwriter(&mut self) -> Option<PgStatBgwriterInfo> {
        if let Err(e) = self.ensure_connected() {
            self.last_error = Some(e.to_string());
            return None;
        }

        let client = self.client.as_mut().unwrap();
        let query = build_stat_bgwriter_query(self.server_version_num);

        match client.query_one(&query, &[]) {
            Ok(row) => {
                self.last_error = None;
                Some(PgStatBgwriterInfo {
                    checkpoints_timed: row.get("checkpoints_timed"),
                    checkpoints_req: row.get("checkpoints_req"),
                    checkpoint_write_time: row.get("checkpoint_write_time"),
                    checkpoint_sync_time: row.get("checkpoint_sync_time"),
                    buffers_checkpoint: row.get("buffers_checkpoint"),
                    buffers_clean: row.get("buffers_clean"),
                    maxwritten_clean: row.get("maxwritten_clean"),
                    buffers_backend: row.get("buffers_backend"),
                    buffers_backend_fsync: row.get("buffers_backend_fsync"),
                    buffers_alloc: row.get("buffers_alloc"),
                })
            }
            Err(e) => {
                let msg = super::format_postgres_error(&e);
                self.last_error = Some(msg);
                self.client = None;
                self.server_version_num = None;
                self.statements_ext_version = None;
                self.statements_last_check = None;
                None
            }
        }
    }
}
