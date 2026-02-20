//! PGS (pg_stat_statements) view model.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::fmt::{format_opt_f64, normalize_query, truncate};
use crate::models::PgStatementsViewMode;
use crate::storage::StringInterner;
use crate::storage::model::{DataBlock, PgStatStatementsInfo, Snapshot};
use crate::tui::state::{PgStatementsTabState, SortKey};
use crate::view::common::{RowStyleClass, TableViewModel, ViewCell, ViewRow};

const PGS_HEADERS_TIME: &[&str] = &["CALLS/s", "TIME/s", "MEAN", "ROWS/s", "DB", "USER", "QUERY"];
const PGS_HEADERS_CALLS: &[&str] = &["CALLS/s", "ROWS/s", "R/CALL", "MEAN", "DB", "USER", "QUERY"];
const PGS_HEADERS_IO: &[&str] = &[
    "CALLS/s",
    "BLK_RD/s",
    "BLK_HIT/s",
    "HIT%",
    "BLK_DIRT/s",
    "BLK_WR/s",
    "DB",
    "QUERY",
];
const PGS_HEADERS_TEMP: &[&str] = &[
    "CALLS/s", "TMP_RD/s", "TMP_WR/s", "TMP_MB/s", "LOC_RD/s", "LOC_WR/s", "DB", "QUERY",
];

const PGS_WIDTHS_TIME: &[u16] = &[10, 10, 8, 10, 20, 20];
const PGS_WIDTHS_CALLS: &[u16] = &[10, 10, 10, 8, 20, 20];
const PGS_WIDTHS_IO: &[u16] = &[10, 10, 10, 6, 10, 10, 20];
const PGS_WIDTHS_TEMP: &[u16] = &[10, 10, 10, 10, 10, 10, 20];

#[derive(Debug, Clone)]
struct PgStatementsRowData {
    queryid: i64,
    db: String,
    user: String,
    query: String,
    mean_exec_time: f64,
    calls_s: Option<f64>,
    rows_s: Option<f64>,
    exec_time_ms_s: Option<f64>,
    rows_per_call_s: Option<f64>,
    shared_blks_read_s: Option<f64>,
    shared_blks_hit_s: Option<f64>,
    shared_blks_dirtied_s: Option<f64>,
    shared_blks_written_s: Option<f64>,
    local_blks_read_s: Option<f64>,
    local_blks_written_s: Option<f64>,
    temp_blks_read_s: Option<f64>,
    temp_blks_written_s: Option<f64>,
    temp_mb_s: Option<f64>,
    hit_pct_s: Option<f64>,
}

impl PgStatementsRowData {
    fn from_statement(s: &PgStatStatementsInfo, interner: Option<&StringInterner>) -> Self {
        let db = resolve_hash(interner, s.datname_hash);
        let user = resolve_hash(interner, s.usename_hash);
        let query = resolve_hash(interner, s.query_hash);

        Self {
            queryid: s.queryid,
            db,
            user,
            query,
            mean_exec_time: s.mean_exec_time,
            calls_s: None,
            rows_s: None,
            exec_time_ms_s: None,
            rows_per_call_s: None,
            shared_blks_read_s: None,
            shared_blks_hit_s: None,
            shared_blks_dirtied_s: None,
            shared_blks_written_s: None,
            local_blks_read_s: None,
            local_blks_written_s: None,
            temp_blks_read_s: None,
            temp_blks_written_s: None,
            temp_mb_s: None,
            hit_pct_s: None,
        }
    }

    fn sort_key(&self, mode: PgStatementsViewMode, col: usize) -> SortKey {
        match mode {
            PgStatementsViewMode::Time => match col {
                0 => SortKey::Float(self.calls_s.unwrap_or(0.0)),
                1 => SortKey::Float(self.exec_time_ms_s.unwrap_or(0.0)),
                2 => SortKey::Float(self.mean_exec_time),
                3 => SortKey::Float(self.rows_s.unwrap_or(0.0)),
                4 => SortKey::String(self.db.clone()),
                5 => SortKey::String(self.user.clone()),
                6 => SortKey::String(self.query.clone()),
                _ => SortKey::Integer(0),
            },
            PgStatementsViewMode::Calls => match col {
                0 => SortKey::Float(self.calls_s.unwrap_or(0.0)),
                1 => SortKey::Float(self.rows_s.unwrap_or(0.0)),
                2 => SortKey::Float(self.rows_per_call_s.unwrap_or(0.0)),
                3 => SortKey::Float(self.mean_exec_time),
                4 => SortKey::String(self.db.clone()),
                5 => SortKey::String(self.user.clone()),
                6 => SortKey::String(self.query.clone()),
                _ => SortKey::Integer(0),
            },
            PgStatementsViewMode::Io => match col {
                0 => SortKey::Float(self.calls_s.unwrap_or(0.0)),
                1 => SortKey::Float(self.shared_blks_read_s.unwrap_or(0.0)),
                2 => SortKey::Float(self.shared_blks_hit_s.unwrap_or(0.0)),
                3 => SortKey::Float(self.hit_pct_s.unwrap_or(0.0)),
                4 => SortKey::Float(self.shared_blks_dirtied_s.unwrap_or(0.0)),
                5 => SortKey::Float(self.shared_blks_written_s.unwrap_or(0.0)),
                6 => SortKey::String(self.db.clone()),
                7 => SortKey::String(self.query.clone()),
                _ => SortKey::Integer(0),
            },
            PgStatementsViewMode::Temp => match col {
                0 => SortKey::Float(self.calls_s.unwrap_or(0.0)),
                1 => SortKey::Float(self.temp_blks_read_s.unwrap_or(0.0)),
                2 => SortKey::Float(self.temp_blks_written_s.unwrap_or(0.0)),
                3 => SortKey::Float(self.temp_mb_s.unwrap_or(0.0)),
                4 => SortKey::Float(self.local_blks_read_s.unwrap_or(0.0)),
                5 => SortKey::Float(self.local_blks_written_s.unwrap_or(0.0)),
                6 => SortKey::String(self.db.clone()),
                7 => SortKey::String(self.query.clone()),
                _ => SortKey::Integer(0),
            },
        }
    }

    fn row_style(&self, mode: PgStatementsViewMode) -> RowStyleClass {
        match mode {
            PgStatementsViewMode::Time | PgStatementsViewMode::Calls => {
                let time_ms_s = self.exec_time_ms_s.unwrap_or(0.0);
                if time_ms_s >= 1_000.0 {
                    RowStyleClass::Critical
                } else if time_ms_s >= 100.0 {
                    RowStyleClass::Warning
                } else {
                    RowStyleClass::Normal
                }
            }
            PgStatementsViewMode::Io => {
                let rd_s = self.shared_blks_read_s.unwrap_or(0.0);
                if rd_s >= 10_000.0 {
                    RowStyleClass::Critical
                } else if rd_s >= 1_000.0 {
                    RowStyleClass::Warning
                } else {
                    RowStyleClass::Normal
                }
            }
            PgStatementsViewMode::Temp => {
                let tmp_mb_s = self.temp_mb_s.unwrap_or(0.0);
                if tmp_mb_s >= 100.0 {
                    RowStyleClass::Critical
                } else if tmp_mb_s >= 10.0 {
                    RowStyleClass::Warning
                } else {
                    RowStyleClass::Normal
                }
            }
        }
    }

    fn cells(&self, mode: PgStatementsViewMode) -> Vec<ViewCell> {
        match mode {
            PgStatementsViewMode::Time => vec![
                ViewCell::plain(format_opt_f64(self.calls_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.exec_time_ms_s, 9, 1)),
                ViewCell::plain(format!("{:>7.1}", self.mean_exec_time)),
                ViewCell::plain(format_opt_f64(self.rows_s, 9, 1)),
                ViewCell::plain(truncate(&self.db, 20)),
                ViewCell::plain(truncate(&self.user, 20)),
                ViewCell::plain(normalize_query(&self.query)),
            ],
            PgStatementsViewMode::Calls => vec![
                ViewCell::plain(format_opt_f64(self.calls_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.rows_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.rows_per_call_s, 9, 2)),
                ViewCell::plain(format!("{:>7.1}", self.mean_exec_time)),
                ViewCell::plain(truncate(&self.db, 20)),
                ViewCell::plain(truncate(&self.user, 20)),
                ViewCell::plain(normalize_query(&self.query)),
            ],
            PgStatementsViewMode::Io => vec![
                ViewCell::plain(format_opt_f64(self.calls_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.shared_blks_read_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.shared_blks_hit_s, 9, 1)),
                ViewCell::styled(
                    format_opt_f64(self.hit_pct_s, 5, 1),
                    hit_pct_style_class(self.hit_pct_s.unwrap_or(0.0)),
                ),
                ViewCell::plain(format_opt_f64(self.shared_blks_dirtied_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.shared_blks_written_s, 9, 1)),
                ViewCell::plain(truncate(&self.db, 20)),
                ViewCell::plain(normalize_query(&self.query)),
            ],
            PgStatementsViewMode::Temp => vec![
                ViewCell::plain(format_opt_f64(self.calls_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.temp_blks_read_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.temp_blks_written_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.temp_mb_s, 9, 2)),
                ViewCell::plain(format_opt_f64(self.local_blks_read_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.local_blks_written_s, 9, 1)),
                ViewCell::plain(truncate(&self.db, 20)),
                ViewCell::plain(normalize_query(&self.query)),
            ],
        }
    }
}

/// HIT% cell style classification.
fn hit_pct_style_class(hit_pct: f64) -> RowStyleClass {
    if hit_pct < 90.0 {
        RowStyleClass::Critical
    } else if hit_pct < 98.0 {
        RowStyleClass::Warning
    } else {
        RowStyleClass::Normal
    }
}

/// Builds a UI-agnostic view model for the PGS (statements) tab.
pub fn build_statements_view(
    snapshot: &Snapshot,
    state: &PgStatementsTabState,
    interner: Option<&StringInterner>,
    is_live: bool,
) -> Option<TableViewModel<i64>> {
    let statements = extract_pg_statements(snapshot);
    if statements.is_empty() {
        return None;
    }

    let mode = state.view_mode;
    let mut rows_data: Vec<PgStatementsRowData> = statements
        .iter()
        .map(|s| {
            let mut row = PgStatementsRowData::from_statement(s, interner);
            if let Some(r) = state.rate_state.rates.get(&s.queryid) {
                row.calls_s = r.calls_s;
                row.rows_s = r.rows_s;
                row.exec_time_ms_s = r.exec_time_ms_s;
                row.shared_blks_read_s = r.shared_blks_read_s;
                row.shared_blks_hit_s = r.shared_blks_hit_s;
                row.shared_blks_dirtied_s = r.shared_blks_dirtied_s;
                row.shared_blks_written_s = r.shared_blks_written_s;
                row.local_blks_read_s = r.local_blks_read_s;
                row.local_blks_written_s = r.local_blks_written_s;
                row.temp_blks_read_s = r.temp_blks_read_s;
                row.temp_blks_written_s = r.temp_blks_written_s;
                row.temp_mb_s = r.temp_mb_s;

                row.rows_per_call_s = match (row.rows_s, row.calls_s) {
                    (Some(rows_s), Some(calls_s)) if calls_s > 0.0 => Some(rows_s / calls_s),
                    _ => None,
                };
                row.hit_pct_s = match (row.shared_blks_hit_s, row.shared_blks_read_s) {
                    (Some(hit_s), Some(read_s)) => {
                        let denom = hit_s + read_s;
                        if denom > 0.0 {
                            Some((hit_s / denom) * 100.0)
                        } else {
                            Some(0.0)
                        }
                    }
                    _ => None,
                };
            }
            row
        })
        .collect();

    // Filter
    if let Some(filter) = &state.filter {
        let f = filter.to_lowercase();
        rows_data.retain(|r| {
            r.queryid.to_string().starts_with(&f)
                || r.db.to_lowercase().contains(&f)
                || r.user.to_lowercase().contains(&f)
                || r.query.to_lowercase().contains(&f)
        });
    }

    // Sort
    let sort_col = state.sort_column;
    let sort_asc = state.sort_ascending;
    rows_data.sort_by(|a, b| {
        let cmp = a
            .sort_key(mode, sort_col)
            .partial_cmp(&b.sort_key(mode, sort_col))
            .unwrap_or(std::cmp::Ordering::Equal);
        if sort_asc { cmp } else { cmp.reverse() }
    });

    let (headers, widths, title_mode) = match mode {
        PgStatementsViewMode::Time => (PGS_HEADERS_TIME, PGS_WIDTHS_TIME, "t:time"),
        PgStatementsViewMode::Calls => (PGS_HEADERS_CALLS, PGS_WIDTHS_CALLS, "c:calls"),
        PgStatementsViewMode::Io => (PGS_HEADERS_IO, PGS_WIDTHS_IO, "i:io"),
        PgStatementsViewMode::Temp => (PGS_HEADERS_TEMP, PGS_WIDTHS_TEMP, "e:temp"),
    };

    let rows: Vec<ViewRow<i64>> = rows_data
        .iter()
        .map(|r| ViewRow {
            id: r.queryid,
            cells: r.cells(mode),
            style: r.row_style(mode),
        })
        .collect();

    // Build sample info
    let dt_secs = state.rate_state.rates.values().next().map(|r| r.dt_secs);
    let last_real_update_ts = state.rate_state.prev_ts;
    let sample_info = match (dt_secs, last_real_update_ts) {
        (Some(dt), Some(last_ts)) => {
            let age = if is_live {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                now.saturating_sub(last_ts)
            } else {
                snapshot.timestamp.saturating_sub(last_ts)
            };
            format!("[dt={:.0}s, age={}s]", dt, age)
        }
        (Some(dt), None) => format!("[dt={:.0}s]", dt),
        _ => String::new(),
    };

    let title = if let Some(filter) = &state.filter {
        format!(
            " PostgreSQL Statements (PGS) [{title_mode}] {sample_info} (filter: {filter}) [{} rows] ",
            rows.len()
        )
    } else {
        format!(
            " PostgreSQL Statements (PGS) [{title_mode}] {sample_info} [{} rows] ",
            rows.len()
        )
    };

    Some(TableViewModel {
        title,
        headers: headers.iter().map(|s| s.to_string()).collect(),
        widths: widths.to_vec(),
        rows,
        sort_column: sort_col,
        sort_ascending: sort_asc,
    })
}

fn extract_pg_statements(snapshot: &Snapshot) -> Vec<&PgStatStatementsInfo> {
    snapshot
        .blocks
        .iter()
        .find_map(|b| {
            if let DataBlock::PgStatStatements(v) = b {
                Some(v.iter().collect())
            } else {
                None
            }
        })
        .unwrap_or_default()
}

fn resolve_hash(interner: Option<&StringInterner>, hash: u64) -> String {
    interner
        .and_then(|i| i.resolve(hash))
        .unwrap_or("")
        .to_string()
}
