//! PgActivityRowData struct and data extraction helpers.

use crate::storage::StringInterner;
use crate::storage::model::{
    DataBlock, PgStatActivityInfo, PgStatStatementsInfo, ProcessInfo, Snapshot,
};
use crate::tui::state::{PgActivityViewMode, PgStatementsRates, SortKey};

/// Intermediate struct for row data.
pub(super) struct PgActivityRowData {
    pub pid: i32,
    pub cpu_percent: f64,
    pub rss_bytes: u64,
    pub db: String,
    pub user: String,
    pub state: String,
    pub wait: String,
    pub query: String,
    pub backend_type: String,
    pub query_duration_secs: i64,
    pub xact_duration_secs: i64,
    pub backend_duration_secs: i64,
    /// Query ID from pg_stat_activity (PostgreSQL 14+). 0 if not available.
    pub query_id: i64,
    /// Stats from pg_stat_statements (linked by query_id).
    /// None if query_id is 0 or not found in pg_stat_statements.
    pub pgs_mean_exec_time: Option<f64>,
    pub pgs_max_exec_time: Option<f64>,
    pub pgs_calls_s: Option<f64>,
    pub pgs_hit_pct: Option<f64>,
}

impl PgActivityRowData {
    pub fn from_pg_activity(
        pg: &PgStatActivityInfo,
        process: Option<&&ProcessInfo>,
        now: i64,
        interner: Option<&StringInterner>,
    ) -> Self {
        // CPU% and RSS from OS process
        let (cpu_percent, rss_bytes) = process
            .map(|p| {
                // CPU% would need delta calculation - for now show 0
                // RSS is in KB, convert to bytes
                (0.0, p.mem.rmem * 1024)
            })
            .unwrap_or((0.0, 0));

        // Resolve hashes using interner
        let db = interner
            .and_then(|i| i.resolve(pg.datname_hash))
            .map(|s| s.to_string())
            .unwrap_or_else(|| "-".to_string());
        let user = interner
            .and_then(|i| i.resolve(pg.usename_hash))
            .map(|s| s.to_string())
            .unwrap_or_else(|| "-".to_string());
        let state = interner
            .and_then(|i| i.resolve(pg.state_hash))
            .map(|s| s.to_string())
            .unwrap_or_else(|| "-".to_string());
        let wait = if pg.wait_event_type_hash != 0 || pg.wait_event_hash != 0 {
            let wait_type = interner
                .and_then(|i| i.resolve(pg.wait_event_type_hash))
                .unwrap_or("");
            let wait_event = interner
                .and_then(|i| i.resolve(pg.wait_event_hash))
                .unwrap_or("");
            if !wait_type.is_empty() && !wait_event.is_empty() {
                format!("{}:{}", wait_type, wait_event)
            } else if !wait_type.is_empty() {
                wait_type.to_string()
            } else if !wait_event.is_empty() {
                wait_event.to_string()
            } else {
                "-".to_string()
            }
        } else {
            "-".to_string()
        };
        let query = interner
            .and_then(|i| i.resolve(pg.query_hash))
            .map(|s| s.to_string())
            .unwrap_or_else(|| "-".to_string());
        let backend_type = interner
            .and_then(|i| i.resolve(pg.backend_type_hash))
            .map(|s| s.to_string())
            .unwrap_or_else(|| "-".to_string());

        // Duration calculations
        let query_duration_secs = if pg.query_start > 0 {
            now.saturating_sub(pg.query_start)
        } else {
            0
        };
        let xact_duration_secs = if pg.xact_start > 0 {
            now.saturating_sub(pg.xact_start)
        } else {
            0
        };
        let backend_duration_secs = if pg.backend_start > 0 {
            now.saturating_sub(pg.backend_start)
        } else {
            0
        };

        Self {
            pid: pg.pid,
            cpu_percent,
            rss_bytes,
            db,
            user,
            state,
            wait,
            query,
            backend_type,
            query_duration_secs,
            xact_duration_secs,
            backend_duration_secs,
            query_id: pg.query_id,
            // PGS stats will be populated later via enrich_with_pgs_stats()
            pgs_mean_exec_time: None,
            pgs_max_exec_time: None,
            pgs_calls_s: None,
            pgs_hit_pct: None,
        }
    }

    /// Enrich row data with pg_stat_statements metrics.
    pub fn enrich_with_pgs_stats(
        &mut self,
        pgs_info: &PgStatStatementsInfo,
        rates: Option<&PgStatementsRates>,
    ) {
        self.pgs_mean_exec_time = Some(pgs_info.mean_exec_time);
        self.pgs_max_exec_time = Some(pgs_info.max_exec_time);
        self.pgs_calls_s = rates.and_then(|r| r.calls_s);
        // Calculate hit percentage
        let total_blks = pgs_info.shared_blks_hit + pgs_info.shared_blks_read;
        if total_blks > 0 {
            self.pgs_hit_pct = Some(pgs_info.shared_blks_hit as f64 / total_blks as f64 * 100.0);
        }
    }

    /// Returns sort key for the given column index and view mode.
    pub fn sort_key_for_mode(&self, col: usize, mode: PgActivityViewMode) -> SortKey {
        match mode {
            PgActivityViewMode::Generic => {
                // Columns: 0=PID, 1=CPU%, 2=RSS, 3=DB, 4=USER, 5=STATE, 6=WAIT, 7=QDUR, 8=XDUR, 9=BDUR, 10=BTYPE, 11=QUERY
                match col {
                    0 => SortKey::Integer(self.pid as i64),
                    1 => SortKey::Float(self.cpu_percent),
                    2 => SortKey::Integer(self.rss_bytes as i64),
                    3 => SortKey::String(self.db.to_lowercase()),
                    4 => SortKey::String(self.user.to_lowercase()),
                    5 => SortKey::String(self.state.to_lowercase()),
                    6 => SortKey::String(self.wait.to_lowercase()),
                    7 => SortKey::Integer(self.query_duration_secs),
                    8 => SortKey::Integer(self.xact_duration_secs),
                    9 => SortKey::Integer(self.backend_duration_secs),
                    10 => SortKey::String(self.backend_type.to_lowercase()),
                    11 => SortKey::String(self.query.to_lowercase()),
                    _ => SortKey::Integer(0),
                }
            }
            PgActivityViewMode::Stats => {
                // Columns: 0=PID, 1=DB, 2=USER, 3=STATE, 4=QDUR, 5=MEAN, 6=MAX, 7=CALL/s, 8=HIT%, 9=QUERY
                match col {
                    0 => SortKey::Integer(self.pid as i64),
                    1 => SortKey::String(self.db.to_lowercase()),
                    2 => SortKey::String(self.user.to_lowercase()),
                    3 => SortKey::String(self.state.to_lowercase()),
                    4 => SortKey::Integer(self.query_duration_secs),
                    5 => SortKey::Float(self.pgs_mean_exec_time.unwrap_or(0.0)),
                    6 => SortKey::Float(self.pgs_max_exec_time.unwrap_or(0.0)),
                    7 => SortKey::Float(self.pgs_calls_s.unwrap_or(0.0)),
                    8 => SortKey::Float(self.pgs_hit_pct.unwrap_or(0.0)),
                    9 => SortKey::String(self.query.to_lowercase()),
                    _ => SortKey::Integer(0),
                }
            }
        }
    }
}

/// Extract PgStatActivity from snapshot.
pub(super) fn extract_pg_activities(snapshot: &Snapshot) -> Vec<&PgStatActivityInfo> {
    snapshot
        .blocks
        .iter()
        .filter_map(|block| {
            if let DataBlock::PgStatActivity(activities) = block {
                Some(activities.iter().collect::<Vec<_>>())
            } else {
                None
            }
        })
        .flatten()
        .collect()
}

/// Extract ProcessInfo from snapshot.
pub(super) fn extract_processes(snapshot: &Snapshot) -> Vec<&ProcessInfo> {
    snapshot
        .blocks
        .iter()
        .filter_map(|block| {
            if let DataBlock::Processes(processes) = block {
                Some(processes.iter().collect::<Vec<_>>())
            } else {
                None
            }
        })
        .flatten()
        .collect()
}

/// Extract pg_stat_statements as a map keyed by queryid.
pub(super) fn extract_pg_statements_map(
    snapshot: &Snapshot,
) -> std::collections::HashMap<i64, &PgStatStatementsInfo> {
    snapshot
        .blocks
        .iter()
        .filter_map(|block| {
            if let DataBlock::PgStatStatements(statements) = block {
                Some(statements.iter().map(|s| (s.queryid, s)))
            } else {
                None
            }
        })
        .flatten()
        .collect()
}
