//! pg_stat_activity collection.

use crate::storage::interner::StringInterner;
use crate::storage::model::PgStatActivityInfo;

use super::PostgresCollector;
use super::queries::build_stat_activity_query;

impl PostgresCollector {
    /// Collects pg_stat_activity data.
    ///
    /// Returns a vector of active sessions. On error, stores the error
    /// message in `last_error` and returns empty vector (collection
    /// continues for other metrics).
    pub fn collect(&mut self, interner: &mut StringInterner) -> Vec<PgStatActivityInfo> {
        if let Err(e) = self.ensure_connected() {
            self.last_error = Some(e.to_string());
            return Vec::new();
        }

        let client = self.client.as_mut().unwrap();
        let query = build_stat_activity_query(self.server_version_num);

        match client.query(&query, &[]) {
            Ok(rows) => {
                self.last_error = None;
                rows.iter()
                    .map(|row| {
                        let datname: String = row.get("datname");
                        let usename: String = row.get("usename");
                        let application_name: String = row.get("application_name");
                        let state: String = row.get("state");
                        let query_text: String = row.get("query");
                        let wait_event_type: String = row.get("wait_event_type");
                        let wait_event: String = row.get("wait_event");
                        let backend_type: String = row.get("backend_type");

                        PgStatActivityInfo {
                            pid: row.get("pid"),
                            datname_hash: interner.intern(&datname),
                            usename_hash: interner.intern(&usename),
                            application_name_hash: interner.intern(&application_name),
                            client_addr: row.get("client_addr"),
                            state_hash: interner.intern(&state),
                            query_hash: interner.intern(&query_text),
                            query_id: row.get("query_id"),
                            wait_event_type_hash: interner.intern(&wait_event_type),
                            wait_event_hash: interner.intern(&wait_event),
                            backend_type_hash: interner.intern(&backend_type),
                            backend_start: row.get("backend_start"),
                            xact_start: row.get("xact_start"),
                            query_start: row.get("query_start"),
                            collected_at: row.get("collected_at"),
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
