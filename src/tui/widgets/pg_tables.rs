//! PostgreSQL tables widget for PGT tab.
//! Displays `pg_stat_user_tables` data with rate computation.

use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table};

use crate::storage::StringInterner;
use crate::storage::model::{DataBlock, PgStatUserTablesInfo, Snapshot};
use crate::tui::state::{AppState, PgTablesViewMode, SortKey};
use crate::tui::style::Styles;

const HEADERS_ACTIVITY: &[&str] = &[
    "SEQ/s", "IDX/s", "INS/s", "UPD/s", "DEL/s", "LIVE", "DEAD", "TABLE",
];
const HEADERS_SCANS: &[&str] = &["SEQ/s", "SEQ_TUP/s", "IDX/s", "IDX_TUP/s", "SEQ%", "TABLE"];
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

const WIDTHS_ACTIVITY: &[u16] = &[10, 10, 10, 10, 10, 10, 10];
const WIDTHS_SCANS: &[u16] = &[10, 12, 10, 12, 6];
const WIDTHS_MAINTENANCE: &[u16] = &[10, 10, 6, 8, 8, 10, 10];

#[derive(Debug, Clone)]
struct PgTablesRowData {
    relid: u32,
    schema: String,
    table: String,
    display_name: String,

    // Gauges (current values)
    n_live_tup: i64,
    n_dead_tup: i64,
    last_autovacuum: i64,
    last_autoanalyze: i64,

    // Computed
    seq_pct: Option<f64>,
    dead_pct: f64,

    // Rates
    seq_scan_s: Option<f64>,
    seq_tup_read_s: Option<f64>,
    idx_scan_s: Option<f64>,
    idx_tup_fetch_s: Option<f64>,
    n_tup_ins_s: Option<f64>,
    n_tup_upd_s: Option<f64>,
    n_tup_del_s: Option<f64>,
    vacuum_count_s: Option<f64>,
    autovacuum_count_s: Option<f64>,
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
            seq_scan_s: None,
            seq_tup_read_s: None,
            idx_scan_s: None,
            idx_tup_fetch_s: None,
            n_tup_ins_s: None,
            n_tup_upd_s: None,
            n_tup_del_s: None,
            vacuum_count_s: None,
            autovacuum_count_s: None,
        }
    }

    fn sort_key(&self, mode: PgTablesViewMode, col: usize) -> SortKey {
        match mode {
            PgTablesViewMode::Activity => match col {
                0 => SortKey::Float(self.seq_scan_s.unwrap_or(0.0)),
                1 => SortKey::Float(self.idx_scan_s.unwrap_or(0.0)),
                2 => SortKey::Float(self.n_tup_ins_s.unwrap_or(0.0)),
                3 => SortKey::Float(self.n_tup_upd_s.unwrap_or(0.0)),
                4 => SortKey::Float(self.n_tup_del_s.unwrap_or(0.0)),
                5 => SortKey::Integer(self.n_live_tup),
                6 => SortKey::Integer(self.n_dead_tup),
                7 => SortKey::String(self.display_name.clone()),
                _ => SortKey::Integer(0),
            },
            PgTablesViewMode::Scans => match col {
                0 => SortKey::Float(self.seq_scan_s.unwrap_or(0.0)),
                1 => SortKey::Float(self.seq_tup_read_s.unwrap_or(0.0)),
                2 => SortKey::Float(self.idx_scan_s.unwrap_or(0.0)),
                3 => SortKey::Float(self.idx_tup_fetch_s.unwrap_or(0.0)),
                4 => SortKey::Float(self.seq_pct.unwrap_or(0.0)),
                5 => SortKey::String(self.display_name.clone()),
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
        }
    }

    fn row_style(&self, mode: PgTablesViewMode) -> Style {
        match mode {
            PgTablesViewMode::Activity | PgTablesViewMode::Scans => {
                if self.dead_pct > 20.0 {
                    Styles::critical()
                } else if self.dead_pct > 5.0 || self.seq_pct.unwrap_or(0.0) > 80.0 {
                    Styles::modified_item()
                } else {
                    Styles::default()
                }
            }
            PgTablesViewMode::Maintenance => {
                if self.dead_pct > 20.0 {
                    Styles::critical()
                } else if self.dead_pct > 5.0 {
                    Styles::modified_item()
                } else {
                    Styles::default()
                }
            }
        }
    }
}

fn format_opt_f64(v: Option<f64>, width: usize, precision: usize) -> String {
    match v {
        Some(v) => format!("{:>width$.prec$}", v, width = width, prec = precision),
        None => format!("{:>width$}", "--", width = width),
    }
}

fn format_i64(v: i64, width: usize) -> String {
    if v >= 1_000_000_000 {
        format!("{:>width$.1}G", v as f64 / 1e9, width = width - 1)
    } else if v >= 1_000_000 {
        format!("{:>width$.1}M", v as f64 / 1e6, width = width - 1)
    } else if v >= 10_000 {
        format!("{:>width$.1}K", v as f64 / 1e3, width = width - 1)
    } else {
        format!("{:>width$}", v, width = width)
    }
}

fn format_age(epoch_secs: i64) -> String {
    if epoch_secs == 0 {
        return "-".to_string();
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let age = now.saturating_sub(epoch_secs);
    if age < 0 {
        return "-".to_string();
    }
    if age < 60 {
        format!("{}s", age)
    } else if age < 3600 {
        format!("{}m", age / 60)
    } else if age < 86400 {
        format!("{}h", age / 3600)
    } else {
        format!("{}d", age / 86400)
    }
}

pub fn render_pg_tables(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    interner: Option<&StringInterner>,
) {
    let snapshot = match &state.current_snapshot {
        Some(s) => s,
        None => {
            let block = Block::default()
                .title(" PostgreSQL Tables (PGT) ")
                .borders(Borders::ALL)
                .style(Styles::default());
            frame.render_widget(Paragraph::new("No data available").block(block), area);
            return;
        }
    };

    let tables = extract_pg_tables(snapshot);
    if tables.is_empty() {
        let block = Block::default()
            .title(" PostgreSQL Tables (PGT) ")
            .borders(Borders::ALL)
            .style(Styles::default());
        let message = state
            .pga
            .last_error
            .as_deref()
            .unwrap_or("pg_stat_user_tables is not available");
        frame.render_widget(Paragraph::new(message).block(block), area);
        return;
    }

    let mode = state.pgt.view_mode;
    let mut rows_data: Vec<PgTablesRowData> = tables
        .iter()
        .map(|t| {
            let mut row = PgTablesRowData::from_table(t, interner);
            if let Some(r) = state.pgt.rates.get(&t.relid) {
                row.seq_scan_s = r.seq_scan_s;
                row.seq_tup_read_s = r.seq_tup_read_s;
                row.idx_scan_s = r.idx_scan_s;
                row.idx_tup_fetch_s = r.idx_tup_fetch_s;
                row.n_tup_ins_s = r.n_tup_ins_s;
                row.n_tup_upd_s = r.n_tup_upd_s;
                row.n_tup_del_s = r.n_tup_del_s;
                row.vacuum_count_s = r.vacuum_count_s;
                row.autovacuum_count_s = r.autovacuum_count_s;
            }
            row
        })
        .collect();

    // Filter
    if let Some(filter) = &state.pgt.filter {
        let f = filter.to_lowercase();
        rows_data.retain(|r| {
            r.schema.to_lowercase().contains(&f)
                || r.table.to_lowercase().contains(&f)
                || r.display_name.to_lowercase().contains(&f)
        });
    }

    // Sort
    let sort_col = state.pgt.sort_column;
    let sort_asc = state.pgt.sort_ascending;
    rows_data.sort_by(|a, b| {
        let cmp = a
            .sort_key(mode, sort_col)
            .partial_cmp(&b.sort_key(mode, sort_col))
            .unwrap_or(std::cmp::Ordering::Equal);
        if sort_asc { cmp } else { cmp.reverse() }
    });

    // Resolve selection
    let row_relids: Vec<u32> = rows_data.iter().map(|r| r.relid).collect();
    state.pgt.resolve_selection(&row_relids);

    let (headers, widths, title_mode) = match mode {
        PgTablesViewMode::Activity => (HEADERS_ACTIVITY, WIDTHS_ACTIVITY, "a:activity"),
        PgTablesViewMode::Scans => (HEADERS_SCANS, WIDTHS_SCANS, "x:scans"),
        PgTablesViewMode::Maintenance => (HEADERS_MAINTENANCE, WIDTHS_MAINTENANCE, "n:maint"),
    };

    // Header with sort indicator
    let headers: Vec<Span> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let indicator = if i == sort_col {
                if sort_asc { "▲" } else { "▼" }
            } else {
                ""
            };
            Span::styled(format!("{}{}", h, indicator), Styles::table_header())
        })
        .collect();
    let header = Row::new(headers).style(Styles::table_header()).height(1);

    let rows: Vec<Row> = rows_data
        .iter()
        .enumerate()
        .map(|(idx, r)| {
            let is_selected = idx == state.pgt.selected;
            let base_style = if is_selected {
                Styles::selected()
            } else {
                r.row_style(mode)
            };

            let cells: Vec<Span> = match mode {
                PgTablesViewMode::Activity => vec![
                    Span::raw(format_opt_f64(r.seq_scan_s, 9, 1)),
                    Span::raw(format_opt_f64(r.idx_scan_s, 9, 1)),
                    Span::raw(format_opt_f64(r.n_tup_ins_s, 9, 1)),
                    Span::raw(format_opt_f64(r.n_tup_upd_s, 9, 1)),
                    Span::raw(format_opt_f64(r.n_tup_del_s, 9, 1)),
                    Span::raw(format_i64(r.n_live_tup, 9)),
                    Span::raw(format_i64(r.n_dead_tup, 9)),
                    Span::raw(r.display_name.clone()),
                ],
                PgTablesViewMode::Scans => vec![
                    Span::raw(format_opt_f64(r.seq_scan_s, 9, 1)),
                    Span::raw(format_opt_f64(r.seq_tup_read_s, 11, 1)),
                    Span::raw(format_opt_f64(r.idx_scan_s, 9, 1)),
                    Span::raw(format_opt_f64(r.idx_tup_fetch_s, 11, 1)),
                    Span::raw(match r.seq_pct {
                        Some(v) => format!("{:>5.1}", v),
                        None => format!("{:>5}", "--"),
                    }),
                    Span::raw(r.display_name.clone()),
                ],
                PgTablesViewMode::Maintenance => vec![
                    Span::raw(format_i64(r.n_dead_tup, 9)),
                    Span::raw(format_i64(r.n_live_tup, 9)),
                    Span::raw(format!("{:>5.1}", r.dead_pct)),
                    Span::raw(format_opt_f64(r.vacuum_count_s, 7, 2)),
                    Span::raw(format_opt_f64(r.autovacuum_count_s, 7, 2)),
                    Span::raw(format!("{:>9}", format_age(r.last_autovacuum))),
                    Span::raw(format!("{:>9}", format_age(r.last_autoanalyze))),
                    Span::raw(r.display_name.clone()),
                ],
            };

            Row::new(cells).style(base_style).height(1)
        })
        .collect();

    // Sample info
    let sample_info = match state.pgt.dt_secs {
        Some(dt) => format!("[dt={:.0}s]", dt),
        None => String::new(),
    };

    let title = if let Some(filter) = &state.pgt.filter {
        format!(
            " PostgreSQL Tables (PGT) [{title_mode}] {sample_info} (filter: {filter}) [{} rows] ",
            rows_data.len()
        )
    } else {
        format!(
            " PostgreSQL Tables (PGT) [{title_mode}] {sample_info} [{} rows] ",
            rows_data.len()
        )
    };

    let mut constraints: Vec<ratatui::layout::Constraint> = widths
        .iter()
        .map(|&w| ratatui::layout::Constraint::Length(w))
        .collect();
    constraints.push(ratatui::layout::Constraint::Fill(1));

    let table = Table::new(rows, constraints)
        .header(header)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .style(Styles::default()),
        )
        .column_spacing(1)
        .row_highlight_style(Styles::selected());

    frame.render_widget(Clear, area);
    frame.render_stateful_widget(table, area, &mut state.pgt.ratatui_state);
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
