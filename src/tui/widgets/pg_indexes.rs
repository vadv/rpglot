//! PostgreSQL indexes widget for PGI tab.
//! Displays `pg_stat_user_indexes` data with rate computation.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table};

use crate::storage::StringInterner;
use crate::storage::model::{DataBlock, PgStatUserIndexesInfo, Snapshot};
use crate::tui::state::{AppState, PgIndexesViewMode, SortKey};
use crate::tui::style::Styles;
use crate::tui::widgets::detail_common::format_bytes;

const HEADERS_USAGE: &[&str] = &["IDX/s", "TUP_RD/s", "TUP_FT/s", "SIZE", "TABLE", "INDEX"];
const HEADERS_UNUSED: &[&str] = &["IDX_SCAN", "SIZE", "TABLE", "INDEX"];

const WIDTHS_USAGE: &[u16] = &[10, 10, 10, 10, 20];
const WIDTHS_UNUSED: &[u16] = &[12, 10, 20];

#[derive(Debug, Clone)]
struct PgIndexesRowData {
    indexrelid: u32,
    schema: String,
    table_name: String,
    index_name: String,
    display_table: String,

    // Current values
    idx_scan: i64,
    size_bytes: i64,

    // Rates
    idx_scan_s: Option<f64>,
    idx_tup_read_s: Option<f64>,
    idx_tup_fetch_s: Option<f64>,
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
        }
    }

    fn sort_key(&self, mode: PgIndexesViewMode, col: usize) -> SortKey {
        match mode {
            PgIndexesViewMode::Usage => match col {
                0 => SortKey::Float(self.idx_scan_s.unwrap_or(0.0)),
                1 => SortKey::Float(self.idx_tup_read_s.unwrap_or(0.0)),
                2 => SortKey::Float(self.idx_tup_fetch_s.unwrap_or(0.0)),
                3 => SortKey::Integer(self.size_bytes),
                4 => SortKey::String(self.display_table.clone()),
                5 => SortKey::String(self.index_name.clone()),
                _ => SortKey::Integer(0),
            },
            PgIndexesViewMode::Unused => match col {
                0 => SortKey::Integer(self.idx_scan),
                1 => SortKey::Integer(self.size_bytes),
                2 => SortKey::String(self.display_table.clone()),
                3 => SortKey::String(self.index_name.clone()),
                _ => SortKey::Integer(0),
            },
        }
    }

    fn row_style(&self) -> Style {
        if self.idx_scan == 0 {
            Styles::modified_item() // yellow for unused indexes
        } else {
            Styles::default()
        }
    }
}

fn format_opt_f64(v: Option<f64>, width: usize, precision: usize) -> String {
    match v {
        Some(v) => format!("{:>width$.prec$}", v, width = width, prec = precision),
        None => format!("{:>width$}", "--", width = width),
    }
}

pub fn render_pg_indexes(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    interner: Option<&StringInterner>,
) {
    let snapshot = match &state.current_snapshot {
        Some(s) => s,
        None => {
            let block = Block::default()
                .title(" PostgreSQL Indexes (PGI) ")
                .borders(Borders::ALL)
                .style(Styles::default());
            frame.render_widget(Paragraph::new("No data available").block(block), area);
            return;
        }
    };

    let indexes = extract_pg_indexes(snapshot);
    if indexes.is_empty() {
        let block = Block::default()
            .title(" PostgreSQL Indexes (PGI) ")
            .borders(Borders::ALL)
            .style(Styles::default());
        let message = state
            .pga
            .last_error
            .as_deref()
            .unwrap_or("pg_stat_user_indexes is not available");
        frame.render_widget(Paragraph::new(message).block(block), area);
        return;
    }

    let mode = state.pgi.view_mode;
    let mut rows_data: Vec<PgIndexesRowData> = indexes
        .iter()
        .filter(|i| {
            // Apply drill-down filter (from PGT)
            if let Some(filter_relid) = state.pgi.filter_relid {
                i.relid == filter_relid
            } else {
                true
            }
        })
        .map(|i| {
            let mut row = PgIndexesRowData::from_index(i, interner);
            if let Some(r) = state.pgi.rates.get(&i.indexrelid) {
                row.idx_scan_s = r.idx_scan_s;
                row.idx_tup_read_s = r.idx_tup_read_s;
                row.idx_tup_fetch_s = r.idx_tup_fetch_s;
            }
            row
        })
        .collect();

    // Text filter
    if let Some(filter) = &state.pgi.filter {
        let f = filter.to_lowercase();
        rows_data.retain(|r| {
            r.schema.to_lowercase().contains(&f)
                || r.table_name.to_lowercase().contains(&f)
                || r.index_name.to_lowercase().contains(&f)
                || r.display_table.to_lowercase().contains(&f)
        });
    }

    // Sort
    let sort_col = state.pgi.sort_column;
    let sort_asc = state.pgi.sort_ascending;
    rows_data.sort_by(|a, b| {
        let cmp = a
            .sort_key(mode, sort_col)
            .partial_cmp(&b.sort_key(mode, sort_col))
            .unwrap_or(std::cmp::Ordering::Equal);
        if sort_asc { cmp } else { cmp.reverse() }
    });

    // Resolve selection
    let row_indexrelids: Vec<u32> = rows_data.iter().map(|r| r.indexrelid).collect();
    state.pgi.resolve_selection(&row_indexrelids);

    let (headers, widths, title_mode) = match mode {
        PgIndexesViewMode::Usage => (HEADERS_USAGE, WIDTHS_USAGE, "u:usage"),
        PgIndexesViewMode::Unused => (HEADERS_UNUSED, WIDTHS_UNUSED, "w:unused"),
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
            let is_selected = idx == state.pgi.selected;
            let base_style = if is_selected {
                Styles::selected()
            } else {
                r.row_style()
            };

            let cells: Vec<Span> = match mode {
                PgIndexesViewMode::Usage => vec![
                    Span::raw(format_opt_f64(r.idx_scan_s, 9, 1)),
                    Span::raw(format_opt_f64(r.idx_tup_read_s, 9, 1)),
                    Span::raw(format_opt_f64(r.idx_tup_fetch_s, 9, 1)),
                    Span::raw(format!("{:>9}", format_bytes(r.size_bytes as u64))),
                    Span::raw(truncate(&r.display_table, 20)),
                    Span::raw(r.index_name.clone()),
                ],
                PgIndexesViewMode::Unused => vec![
                    Span::raw(format!("{:>11}", r.idx_scan)),
                    Span::raw(format!("{:>9}", format_bytes(r.size_bytes as u64))),
                    Span::raw(truncate(&r.display_table, 20)),
                    Span::raw(r.index_name.clone()),
                ],
            };

            Row::new(cells).style(base_style).height(1)
        })
        .collect();

    // Sample info
    let sample_info = match state.pgi.dt_secs {
        Some(dt) => format!("[dt={:.0}s]", dt),
        None => String::new(),
    };

    let filter_info = if let Some(filter_relid) = state.pgi.filter_relid {
        // Find the table name for the filter
        let table_name = rows_data
            .first()
            .map(|r| r.display_table.as_str())
            .unwrap_or("?");
        format!(" (table: {}, oid={})", table_name, filter_relid)
    } else {
        String::new()
    };

    let title = if let Some(filter) = &state.pgi.filter {
        format!(
            " PostgreSQL Indexes (PGI) [{title_mode}] {sample_info}{filter_info} (filter: {filter}) [{} rows] ",
            rows_data.len()
        )
    } else {
        format!(
            " PostgreSQL Indexes (PGI) [{title_mode}] {sample_info}{filter_info} [{} rows] ",
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
    frame.render_stateful_widget(table, area, &mut state.pgi.ratatui_state);
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

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len.saturating_sub(1)])
    }
}
