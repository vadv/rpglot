//! PGI (pg_stat_user_indexes) view model.

use crate::storage::StringInterner;
use crate::storage::model::{DataBlock, PgStatUserIndexesInfo, Snapshot};
use crate::tui::fmt::{self, FmtStyle, format_blks_rate, format_opt_f64, truncate};
use crate::tui::models::PgIndexesViewMode;
use crate::tui::state::{PgIndexesTabState, SortKey};
use crate::view::common::{RowStyleClass, TableViewModel, ViewCell, ViewRow};

const HEADERS_USAGE: &[&str] = &[
    "IDX/s", "TUP_RD/s", "TUP_FT/s", "HIT%", "DISK/s", "SIZE", "TABLE", "INDEX",
];
const HEADERS_UNUSED: &[&str] = &["IDX_SCAN", "SIZE", "TABLE", "INDEX"];
const HEADERS_IO: &[&str] = &[
    "IDX_RD/s",
    "IDX_HIT/s",
    "HIT%",
    "DISK/s",
    "SIZE",
    "TABLE",
    "INDEX",
];

const WIDTHS_USAGE: &[u16] = &[10, 10, 10, 6, 8, 10, 20];
const WIDTHS_UNUSED: &[u16] = &[12, 10, 20];
const WIDTHS_IO: &[u16] = &[10, 10, 6, 8, 10, 20];

#[derive(Debug, Clone)]
struct PgIndexesRowData {
    indexrelid: u32,
    schema: String,
    table_name: String,
    index_name: String,
    display_table: String,
    idx_scan: i64,
    size_bytes: i64,
    idx_scan_s: Option<f64>,
    idx_tup_read_s: Option<f64>,
    idx_tup_fetch_s: Option<f64>,
    idx_blks_read_s: Option<f64>,
    idx_blks_hit_s: Option<f64>,
    hit_pct: Option<f64>,
    disk_read_blks_s: Option<f64>,
}

impl PgIndexesRowData {
    fn from_index(i: &PgStatUserIndexesInfo, interner: Option<&StringInterner>) -> Self {
        let schema = resolve_hash(interner, i.schemaname_hash);
        let table_name = resolve_hash(interner, i.relname_hash);
        let index_name = resolve_hash(interner, i.indexrelname_hash);
        let display_table = if schema.is_empty() || schema == "public" {
            table_name.clone()
        } else {
            format!("{}.{}", schema, table_name)
        };

        Self {
            indexrelid: i.indexrelid,
            schema,
            table_name,
            index_name,
            display_table,
            idx_scan: i.idx_scan,
            size_bytes: i.size_bytes,
            idx_scan_s: None,
            idx_tup_read_s: None,
            idx_tup_fetch_s: None,
            idx_blks_read_s: None,
            idx_blks_hit_s: None,
            hit_pct: None,
            disk_read_blks_s: None,
        }
    }

    fn sort_key(&self, mode: PgIndexesViewMode, col: usize) -> SortKey {
        match mode {
            PgIndexesViewMode::Usage => match col {
                0 => SortKey::Float(self.idx_scan_s.unwrap_or(0.0)),
                1 => SortKey::Float(self.idx_tup_read_s.unwrap_or(0.0)),
                2 => SortKey::Float(self.idx_tup_fetch_s.unwrap_or(0.0)),
                3 => SortKey::Float(self.hit_pct.unwrap_or(0.0)),
                4 => SortKey::Float(self.disk_read_blks_s.unwrap_or(0.0)),
                5 => SortKey::Integer(self.size_bytes),
                6 => SortKey::String(self.display_table.clone()),
                7 => SortKey::String(self.index_name.clone()),
                _ => SortKey::Integer(0),
            },
            PgIndexesViewMode::Unused => match col {
                0 => SortKey::Integer(self.idx_scan),
                1 => SortKey::Integer(self.size_bytes),
                2 => SortKey::String(self.display_table.clone()),
                3 => SortKey::String(self.index_name.clone()),
                _ => SortKey::Integer(0),
            },
            PgIndexesViewMode::Io => match col {
                0 => SortKey::Float(self.idx_blks_read_s.unwrap_or(0.0)),
                1 => SortKey::Float(self.idx_blks_hit_s.unwrap_or(0.0)),
                2 => SortKey::Float(self.hit_pct.unwrap_or(0.0)),
                3 => SortKey::Float(self.disk_read_blks_s.unwrap_or(0.0)),
                4 => SortKey::Integer(self.size_bytes),
                5 => SortKey::String(self.display_table.clone()),
                6 => SortKey::String(self.index_name.clone()),
                _ => SortKey::Integer(0),
            },
        }
    }

    fn row_style(&self, mode: PgIndexesViewMode) -> RowStyleClass {
        match mode {
            PgIndexesViewMode::Usage | PgIndexesViewMode::Unused => {
                if self.idx_scan == 0 {
                    RowStyleClass::Warning
                } else {
                    RowStyleClass::Normal
                }
            }
            PgIndexesViewMode::Io => {
                if let Some(hit) = self.hit_pct {
                    if hit < 70.0 {
                        RowStyleClass::Critical
                    } else if hit < 90.0 {
                        RowStyleClass::Warning
                    } else {
                        RowStyleClass::Normal
                    }
                } else if self.idx_scan == 0 {
                    RowStyleClass::Warning
                } else {
                    RowStyleClass::Normal
                }
            }
        }
    }

    fn cells(&self, mode: PgIndexesViewMode) -> Vec<ViewCell> {
        match mode {
            PgIndexesViewMode::Usage => vec![
                ViewCell::plain(format_opt_f64(self.idx_scan_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.idx_tup_read_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.idx_tup_fetch_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.hit_pct, 5, 1)),
                ViewCell::plain(format_blks_rate(self.disk_read_blks_s, 7)),
                ViewCell::plain(format!(
                    "{:>9}",
                    fmt::format_bytes(self.size_bytes as u64, FmtStyle::Detail)
                )),
                ViewCell::plain(truncate(&self.display_table, 20)),
                ViewCell::plain(self.index_name.clone()),
            ],
            PgIndexesViewMode::Unused => vec![
                ViewCell::plain(format!("{:>11}", self.idx_scan)),
                ViewCell::plain(format!(
                    "{:>9}",
                    fmt::format_bytes(self.size_bytes as u64, FmtStyle::Detail)
                )),
                ViewCell::plain(truncate(&self.display_table, 20)),
                ViewCell::plain(self.index_name.clone()),
            ],
            PgIndexesViewMode::Io => vec![
                ViewCell::plain(format_blks_rate(self.idx_blks_read_s, 9)),
                ViewCell::plain(format_blks_rate(self.idx_blks_hit_s, 9)),
                ViewCell::plain(format_opt_f64(self.hit_pct, 5, 1)),
                ViewCell::plain(format_blks_rate(self.disk_read_blks_s, 7)),
                ViewCell::plain(format!(
                    "{:>9}",
                    fmt::format_bytes(self.size_bytes as u64, FmtStyle::Detail)
                )),
                ViewCell::plain(truncate(&self.display_table, 20)),
                ViewCell::plain(self.index_name.clone()),
            ],
        }
    }
}

/// Builds a UI-agnostic view model for the PGI (indexes) tab.
///
/// Returns `None` if there is no snapshot data.
pub fn build_indexes_view(
    snapshot: &Snapshot,
    state: &PgIndexesTabState,
    interner: Option<&StringInterner>,
) -> Option<TableViewModel<u32>> {
    let indexes = extract_pg_indexes(snapshot);
    if indexes.is_empty() {
        return None;
    }

    let mode = state.view_mode;
    let mut rows_data: Vec<PgIndexesRowData> = indexes
        .iter()
        .filter(|i| {
            if let Some(filter_relid) = state.filter_relid {
                i.relid == filter_relid
            } else {
                true
            }
        })
        .map(|i| {
            let mut row = PgIndexesRowData::from_index(i, interner);
            if let Some(r) = state.rates.get(&i.indexrelid) {
                row.idx_scan_s = r.idx_scan_s;
                row.idx_tup_read_s = r.idx_tup_read_s;
                row.idx_tup_fetch_s = r.idx_tup_fetch_s;
                row.idx_blks_read_s = r.idx_blks_read_s;
                row.idx_blks_hit_s = r.idx_blks_hit_s;

                if let (Some(ir), Some(ih)) = (r.idx_blks_read_s, r.idx_blks_hit_s) {
                    let total = ir + ih;
                    if total > 0.0 {
                        row.hit_pct = Some(ih / total * 100.0);
                    }
                    row.disk_read_blks_s = Some(ir);
                }
            }
            row
        })
        .collect();

    // Text filter
    if let Some(filter) = &state.filter {
        let f = filter.to_lowercase();
        rows_data.retain(|r| {
            r.schema.to_lowercase().contains(&f)
                || r.table_name.to_lowercase().contains(&f)
                || r.index_name.to_lowercase().contains(&f)
                || r.display_table.to_lowercase().contains(&f)
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
        PgIndexesViewMode::Usage => (HEADERS_USAGE, WIDTHS_USAGE, "u:usage"),
        PgIndexesViewMode::Unused => (HEADERS_UNUSED, WIDTHS_UNUSED, "w:unused"),
        PgIndexesViewMode::Io => (HEADERS_IO, WIDTHS_IO, "i:io"),
    };

    // Build view rows
    let rows: Vec<ViewRow<u32>> = rows_data
        .iter()
        .map(|r| ViewRow {
            id: r.indexrelid,
            cells: r.cells(mode),
            style: r.row_style(mode),
        })
        .collect();

    // Sample info
    let sample_info = match state.dt_secs {
        Some(dt) => format!("[dt={:.0}s]", dt),
        None => String::new(),
    };

    let filter_info = if let Some(filter_relid) = state.filter_relid {
        let table_name = rows_data
            .first()
            .map(|r| r.display_table.as_str())
            .unwrap_or("?");
        format!(" (table: {}, oid={})", table_name, filter_relid)
    } else {
        String::new()
    };

    let title = if let Some(filter) = &state.filter {
        format!(
            " PostgreSQL Indexes (PGI) [{title_mode}] {sample_info}{filter_info} (filter: {filter}) [{} rows] ",
            rows.len()
        )
    } else {
        format!(
            " PostgreSQL Indexes (PGI) [{title_mode}] {sample_info}{filter_info} [{} rows] ",
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

fn extract_pg_indexes(snapshot: &Snapshot) -> Vec<&PgStatUserIndexesInfo> {
    snapshot
        .blocks
        .iter()
        .find_map(|b| {
            if let DataBlock::PgStatUserIndexes(v) = b {
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
