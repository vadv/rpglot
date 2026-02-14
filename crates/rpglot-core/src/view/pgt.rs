//! PGT (pg_stat_user_tables) view model.

use crate::fmt::{format_age, format_blks_rate, format_i64, format_opt_f64, format_size};
use crate::models::PgTablesViewMode;
use crate::storage::StringInterner;
use crate::storage::model::{DataBlock, PgStatUserTablesInfo, Snapshot};
use crate::tui::state::{PgTablesTabState, SortKey};
use crate::view::common::{RowStyleClass, TableViewModel, ViewCell, ViewRow};

const HEADERS_READS: &[&str] = &[
    "SEQ_RD/s", "IDX_FT/s", "TOT_RD/s", "SEQ/s", "IDX/s", "HIT%", "DISK/s", "SIZE", "TABLE",
];
const HEADERS_WRITES: &[&str] = &[
    "INS/s", "UPD/s", "DEL/s", "HOT/s", "LIVE", "DEAD", "HIT%", "DISK/s", "SIZE", "TABLE",
];
const HEADERS_SCANS: &[&str] = &[
    "SEQ/s",
    "SEQ_TUP/s",
    "IDX/s",
    "IDX_TUP/s",
    "SEQ%",
    "HIT%",
    "DISK/s",
    "SIZE",
    "TABLE",
];
const HEADERS_MAINTENANCE: &[&str] = &[
    "DEAD",
    "LIVE",
    "DEAD%",
    "VAC/s",
    "AVAC/s",
    "LAST_AVAC",
    "LAST_AANL",
    "TABLE",
];
const HEADERS_IO: &[&str] = &[
    "HEAP_RD/s",
    "HEAP_HIT/s",
    "IDX_RD/s",
    "IDX_HIT/s",
    "HIT%",
    "DISK/s",
    "SIZE",
    "TABLE",
];

const WIDTHS_READS: &[u16] = &[10, 10, 10, 10, 10, 6, 8, 10];
const WIDTHS_WRITES: &[u16] = &[10, 10, 10, 10, 10, 10, 6, 8, 10];
const WIDTHS_SCANS: &[u16] = &[10, 12, 10, 12, 6, 6, 8, 10];
const WIDTHS_MAINTENANCE: &[u16] = &[10, 10, 6, 8, 8, 10, 10];
const WIDTHS_IO: &[u16] = &[10, 10, 10, 10, 6, 8, 10];

#[derive(Debug, Clone)]
struct PgTablesRowData {
    relid: u32,
    schema: String,
    table: String,
    display_name: String,
    n_live_tup: i64,
    n_dead_tup: i64,
    last_autovacuum: i64,
    last_autoanalyze: i64,
    seq_pct: Option<f64>,
    dead_pct: f64,
    size_bytes: i64,
    seq_scan_s: Option<f64>,
    seq_tup_read_s: Option<f64>,
    idx_scan_s: Option<f64>,
    idx_tup_fetch_s: Option<f64>,
    total_read_s: Option<f64>,
    n_tup_ins_s: Option<f64>,
    n_tup_upd_s: Option<f64>,
    n_tup_del_s: Option<f64>,
    n_tup_hot_upd_s: Option<f64>,
    vacuum_count_s: Option<f64>,
    autovacuum_count_s: Option<f64>,
    heap_blks_read_s: Option<f64>,
    heap_blks_hit_s: Option<f64>,
    idx_blks_read_s: Option<f64>,
    idx_blks_hit_s: Option<f64>,
    hit_pct: Option<f64>,
    disk_read_blks_s: Option<f64>,
}

impl PgTablesRowData {
    fn from_table(t: &PgStatUserTablesInfo, interner: Option<&StringInterner>) -> Self {
        let schema = resolve_hash(interner, t.schemaname_hash);
        let table = resolve_hash(interner, t.relname_hash);
        let display_name = if schema.is_empty() || schema == "public" {
            table.clone()
        } else {
            format!("{}.{}", schema, table)
        };

        let total_scans = t.seq_scan + t.idx_scan;
        let seq_pct = if total_scans > 0 {
            Some((t.seq_scan as f64 / total_scans as f64) * 100.0)
        } else {
            None
        };

        let total_tup = t.n_live_tup + t.n_dead_tup;
        let dead_pct = if total_tup > 0 {
            (t.n_dead_tup as f64 / total_tup as f64) * 100.0
        } else {
            0.0
        };

        Self {
            relid: t.relid,
            schema,
            table,
            display_name,
            n_live_tup: t.n_live_tup,
            n_dead_tup: t.n_dead_tup,
            last_autovacuum: t.last_autovacuum,
            last_autoanalyze: t.last_autoanalyze,
            seq_pct,
            dead_pct,
            size_bytes: t.size_bytes,
            seq_scan_s: None,
            seq_tup_read_s: None,
            idx_scan_s: None,
            idx_tup_fetch_s: None,
            total_read_s: None,
            n_tup_ins_s: None,
            n_tup_upd_s: None,
            n_tup_del_s: None,
            n_tup_hot_upd_s: None,
            vacuum_count_s: None,
            autovacuum_count_s: None,
            heap_blks_read_s: None,
            heap_blks_hit_s: None,
            idx_blks_read_s: None,
            idx_blks_hit_s: None,
            hit_pct: None,
            disk_read_blks_s: None,
        }
    }

    fn sort_key(&self, mode: PgTablesViewMode, col: usize) -> SortKey {
        match mode {
            PgTablesViewMode::Reads => match col {
                0 => SortKey::Float(self.seq_tup_read_s.unwrap_or(0.0)),
                1 => SortKey::Float(self.idx_tup_fetch_s.unwrap_or(0.0)),
                2 => SortKey::Float(self.total_read_s.unwrap_or(0.0)),
                3 => SortKey::Float(self.seq_scan_s.unwrap_or(0.0)),
                4 => SortKey::Float(self.idx_scan_s.unwrap_or(0.0)),
                5 => SortKey::Float(self.hit_pct.unwrap_or(0.0)),
                6 => SortKey::Float(self.disk_read_blks_s.unwrap_or(0.0)),
                7 => SortKey::Integer(self.size_bytes),
                8 => SortKey::String(self.display_name.clone()),
                _ => SortKey::Integer(0),
            },
            PgTablesViewMode::Writes => match col {
                0 => SortKey::Float(self.n_tup_ins_s.unwrap_or(0.0)),
                1 => SortKey::Float(self.n_tup_upd_s.unwrap_or(0.0)),
                2 => SortKey::Float(self.n_tup_del_s.unwrap_or(0.0)),
                3 => SortKey::Float(self.n_tup_hot_upd_s.unwrap_or(0.0)),
                4 => SortKey::Integer(self.n_live_tup),
                5 => SortKey::Integer(self.n_dead_tup),
                6 => SortKey::Float(self.hit_pct.unwrap_or(0.0)),
                7 => SortKey::Float(self.disk_read_blks_s.unwrap_or(0.0)),
                8 => SortKey::Integer(self.size_bytes),
                9 => SortKey::String(self.display_name.clone()),
                _ => SortKey::Integer(0),
            },
            PgTablesViewMode::Scans => match col {
                0 => SortKey::Float(self.seq_scan_s.unwrap_or(0.0)),
                1 => SortKey::Float(self.seq_tup_read_s.unwrap_or(0.0)),
                2 => SortKey::Float(self.idx_scan_s.unwrap_or(0.0)),
                3 => SortKey::Float(self.idx_tup_fetch_s.unwrap_or(0.0)),
                4 => SortKey::Float(self.seq_pct.unwrap_or(0.0)),
                5 => SortKey::Float(self.hit_pct.unwrap_or(0.0)),
                6 => SortKey::Float(self.disk_read_blks_s.unwrap_or(0.0)),
                7 => SortKey::Integer(self.size_bytes),
                8 => SortKey::String(self.display_name.clone()),
                _ => SortKey::Integer(0),
            },
            PgTablesViewMode::Maintenance => match col {
                0 => SortKey::Integer(self.n_dead_tup),
                1 => SortKey::Integer(self.n_live_tup),
                2 => SortKey::Float(self.dead_pct),
                3 => SortKey::Float(self.vacuum_count_s.unwrap_or(0.0)),
                4 => SortKey::Float(self.autovacuum_count_s.unwrap_or(0.0)),
                5 => SortKey::Integer(self.last_autovacuum),
                6 => SortKey::Integer(self.last_autoanalyze),
                7 => SortKey::String(self.display_name.clone()),
                _ => SortKey::Integer(0),
            },
            PgTablesViewMode::Io => match col {
                0 => SortKey::Float(self.heap_blks_read_s.unwrap_or(0.0)),
                1 => SortKey::Float(self.heap_blks_hit_s.unwrap_or(0.0)),
                2 => SortKey::Float(self.idx_blks_read_s.unwrap_or(0.0)),
                3 => SortKey::Float(self.idx_blks_hit_s.unwrap_or(0.0)),
                4 => SortKey::Float(self.hit_pct.unwrap_or(0.0)),
                5 => SortKey::Float(self.disk_read_blks_s.unwrap_or(0.0)),
                6 => SortKey::Integer(self.size_bytes),
                7 => SortKey::String(self.display_name.clone()),
                _ => SortKey::Integer(0),
            },
        }
    }

    fn row_style(&self, mode: PgTablesViewMode) -> RowStyleClass {
        match mode {
            PgTablesViewMode::Reads | PgTablesViewMode::Writes | PgTablesViewMode::Scans => {
                if self.dead_pct > 20.0 {
                    RowStyleClass::Critical
                } else if self.dead_pct > 5.0 || self.seq_pct.unwrap_or(0.0) > 80.0 {
                    RowStyleClass::Warning
                } else {
                    RowStyleClass::Normal
                }
            }
            PgTablesViewMode::Maintenance => {
                if self.dead_pct > 20.0 {
                    RowStyleClass::Critical
                } else if self.dead_pct > 5.0 {
                    RowStyleClass::Warning
                } else {
                    RowStyleClass::Normal
                }
            }
            PgTablesViewMode::Io => {
                if let Some(hit) = self.hit_pct {
                    if hit < 70.0 {
                        RowStyleClass::Critical
                    } else if hit < 90.0 {
                        RowStyleClass::Warning
                    } else {
                        RowStyleClass::Normal
                    }
                } else {
                    RowStyleClass::Normal
                }
            }
        }
    }

    fn cells(&self, mode: PgTablesViewMode) -> Vec<ViewCell> {
        match mode {
            PgTablesViewMode::Reads => vec![
                ViewCell::plain(format_opt_f64(self.seq_tup_read_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.idx_tup_fetch_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.total_read_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.seq_scan_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.idx_scan_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.hit_pct, 5, 1)),
                ViewCell::plain(format_blks_rate(self.disk_read_blks_s, 7)),
                ViewCell::plain(format_size(self.size_bytes)),
                ViewCell::plain(self.display_name.clone()),
            ],
            PgTablesViewMode::Writes => vec![
                ViewCell::plain(format_opt_f64(self.n_tup_ins_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.n_tup_upd_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.n_tup_del_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.n_tup_hot_upd_s, 9, 1)),
                ViewCell::plain(format_i64(self.n_live_tup, 9)),
                ViewCell::plain(format_i64(self.n_dead_tup, 9)),
                ViewCell::plain(format_opt_f64(self.hit_pct, 5, 1)),
                ViewCell::plain(format_blks_rate(self.disk_read_blks_s, 7)),
                ViewCell::plain(format_size(self.size_bytes)),
                ViewCell::plain(self.display_name.clone()),
            ],
            PgTablesViewMode::Scans => vec![
                ViewCell::plain(format_opt_f64(self.seq_scan_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.seq_tup_read_s, 11, 1)),
                ViewCell::plain(format_opt_f64(self.idx_scan_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.idx_tup_fetch_s, 11, 1)),
                ViewCell::plain(match self.seq_pct {
                    Some(v) => format!("{:>5.1}", v),
                    None => format!("{:>5}", "--"),
                }),
                ViewCell::plain(format_opt_f64(self.hit_pct, 5, 1)),
                ViewCell::plain(format_blks_rate(self.disk_read_blks_s, 7)),
                ViewCell::plain(format_size(self.size_bytes)),
                ViewCell::plain(self.display_name.clone()),
            ],
            PgTablesViewMode::Maintenance => vec![
                ViewCell::plain(format_i64(self.n_dead_tup, 9)),
                ViewCell::plain(format_i64(self.n_live_tup, 9)),
                ViewCell::plain(format!("{:>5.1}", self.dead_pct)),
                ViewCell::plain(format_opt_f64(self.vacuum_count_s, 7, 2)),
                ViewCell::plain(format_opt_f64(self.autovacuum_count_s, 7, 2)),
                ViewCell::plain(format!("{:>9}", format_age(self.last_autovacuum))),
                ViewCell::plain(format!("{:>9}", format_age(self.last_autoanalyze))),
                ViewCell::plain(self.display_name.clone()),
            ],
            PgTablesViewMode::Io => vec![
                ViewCell::plain(format_blks_rate(self.heap_blks_read_s, 9)),
                ViewCell::plain(format_blks_rate(self.heap_blks_hit_s, 9)),
                ViewCell::plain(format_blks_rate(self.idx_blks_read_s, 9)),
                ViewCell::plain(format_blks_rate(self.idx_blks_hit_s, 9)),
                ViewCell::plain(format_opt_f64(self.hit_pct, 5, 1)),
                ViewCell::plain(format_blks_rate(self.disk_read_blks_s, 7)),
                ViewCell::plain(format_size(self.size_bytes)),
                ViewCell::plain(self.display_name.clone()),
            ],
        }
    }
}

/// Builds a UI-agnostic view model for the PGT (tables) tab.
pub fn build_tables_view(
    snapshot: &Snapshot,
    state: &PgTablesTabState,
    interner: Option<&StringInterner>,
) -> Option<TableViewModel<u32>> {
    let tables = extract_pg_tables(snapshot);
    if tables.is_empty() {
        return None;
    }

    let mode = state.view_mode;
    let mut rows_data: Vec<PgTablesRowData> = tables
        .iter()
        .map(|t| {
            let mut row = PgTablesRowData::from_table(t, interner);
            if let Some(r) = state.rates.get(&t.relid) {
                row.seq_scan_s = r.seq_scan_s;
                row.seq_tup_read_s = r.seq_tup_read_s;
                row.idx_scan_s = r.idx_scan_s;
                row.idx_tup_fetch_s = r.idx_tup_fetch_s;
                row.n_tup_ins_s = r.n_tup_ins_s;
                row.n_tup_upd_s = r.n_tup_upd_s;
                row.n_tup_del_s = r.n_tup_del_s;
                row.n_tup_hot_upd_s = r.n_tup_hot_upd_s;
                row.vacuum_count_s = r.vacuum_count_s;
                row.autovacuum_count_s = r.autovacuum_count_s;
                row.total_read_s = match (r.seq_tup_read_s, r.idx_tup_fetch_s) {
                    (Some(a), Some(b)) => Some(a + b),
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    (None, None) => None,
                };
                row.heap_blks_read_s = r.heap_blks_read_s;
                row.heap_blks_hit_s = r.heap_blks_hit_s;
                row.idx_blks_read_s = r.idx_blks_read_s;
                row.idx_blks_hit_s = r.idx_blks_hit_s;

                if let (Some(hr), Some(hh), Some(ir), Some(ih)) = (
                    r.heap_blks_read_s,
                    r.heap_blks_hit_s,
                    r.idx_blks_read_s,
                    r.idx_blks_hit_s,
                ) {
                    let total_io = hr + hh + ir + ih;
                    if total_io > 0.0 {
                        row.hit_pct = Some((hh + ih) / total_io * 100.0);
                    }
                    row.disk_read_blks_s = Some(hr + ir);
                }
            }
            row
        })
        .collect();

    // Filter
    if let Some(filter) = &state.filter {
        let f = filter.to_lowercase();
        rows_data.retain(|r| {
            r.schema.to_lowercase().contains(&f)
                || r.table.to_lowercase().contains(&f)
                || r.display_name.to_lowercase().contains(&f)
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
        PgTablesViewMode::Reads => (HEADERS_READS, WIDTHS_READS, "a:reads"),
        PgTablesViewMode::Writes => (HEADERS_WRITES, WIDTHS_WRITES, "w:writes"),
        PgTablesViewMode::Scans => (HEADERS_SCANS, WIDTHS_SCANS, "x:scans"),
        PgTablesViewMode::Maintenance => (HEADERS_MAINTENANCE, WIDTHS_MAINTENANCE, "n:maint"),
        PgTablesViewMode::Io => (HEADERS_IO, WIDTHS_IO, "i:io"),
    };

    let rows: Vec<ViewRow<u32>> = rows_data
        .iter()
        .map(|r| ViewRow {
            id: r.relid,
            cells: r.cells(mode),
            style: r.row_style(mode),
        })
        .collect();

    let sample_info = match state.dt_secs {
        Some(dt) => format!("[dt={:.0}s]", dt),
        None => String::new(),
    };

    let title = if let Some(filter) = &state.filter {
        format!(
            " PostgreSQL Tables (PGT) [{title_mode}] {sample_info} (filter: {filter}) [{} rows] ",
            rows.len()
        )
    } else {
        format!(
            " PostgreSQL Tables (PGT) [{title_mode}] {sample_info} [{} rows] ",
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

fn extract_pg_tables(snapshot: &Snapshot) -> Vec<&PgStatUserTablesInfo> {
    snapshot
        .blocks
        .iter()
        .find_map(|b| {
            if let DataBlock::PgStatUserTables(v) = b {
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
