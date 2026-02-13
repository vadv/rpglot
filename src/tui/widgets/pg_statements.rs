//! PostgreSQL statements table widget for PGS tab.
//! Displays `pg_stat_statements` data.

use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table};

use crate::storage::StringInterner;
use crate::storage::model::{DataBlock, PgStatStatementsInfo, Snapshot};
use crate::tui::state::{AppState, PgStatementsViewMode, SortKey};
use crate::tui::style::Styles;

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

// Width arrays cover all columns except the trailing QUERY column, which uses Fill(1).
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

    // Rates (/s) computed from deltas between two real samples.
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

    fn row_style(&self, mode: PgStatementsViewMode) -> Style {
        match mode {
            PgStatementsViewMode::Time | PgStatementsViewMode::Calls => {
                let time_ms_s = self.exec_time_ms_s.unwrap_or(0.0);
                if time_ms_s >= 1_000.0 {
                    Styles::critical()
                } else if time_ms_s >= 100.0 {
                    Styles::modified_item()
                } else {
                    Styles::default()
                }
            }
            PgStatementsViewMode::Io => {
                let rd_s = self.shared_blks_read_s.unwrap_or(0.0);
                if rd_s >= 10_000.0 {
                    Styles::critical()
                } else if rd_s >= 1_000.0 {
                    Styles::modified_item()
                } else {
                    Styles::default()
                }
            }
            PgStatementsViewMode::Temp => {
                let tmp_mb_s = self.temp_mb_s.unwrap_or(0.0);
                if tmp_mb_s >= 100.0 {
                    Styles::critical()
                } else if tmp_mb_s >= 10.0 {
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

pub fn render_pg_statements(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    interner: Option<&StringInterner>,
) {
    let snapshot = match &state.current_snapshot {
        Some(s) => s,
        None => {
            let block = Block::default()
                .title(" PostgreSQL Statements (PGS) ")
                .borders(Borders::ALL)
                .style(Styles::default());
            frame.render_widget(Paragraph::new("No data available").block(block), area);
            return;
        }
    };

    let statements = extract_pg_statements(snapshot);
    if statements.is_empty() {
        let block = Block::default()
            .title(" PostgreSQL Statements (PGS) ")
            .borders(Borders::ALL)
            .style(Styles::default());
        let message = state
            .pga
            .last_error
            .as_deref()
            .unwrap_or("pg_stat_statements is not available");
        frame.render_widget(Paragraph::new(message).block(block), area);
        return;
    }

    let mode = state.pgs.view_mode;
    let mut rows_data: Vec<PgStatementsRowData> = statements
        .iter()
        .map(|s| {
            let mut row = PgStatementsRowData::from_statement(s, interner);
            if let Some(r) = state.pgs.rates.get(&s.queryid) {
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

                // Derived rate-only metrics.
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

    // Apply filter: queryid, database, username, query
    if let Some(filter) = &state.pgs.filter {
        let f = filter.to_lowercase();
        rows_data.retain(|r| {
            // Check queryid (exact prefix match for numbers)
            r.queryid.to_string().starts_with(&f)
                // Check text fields (substring match)
                || r.db.to_lowercase().contains(&f)
                || r.user.to_lowercase().contains(&f)
                || r.query.to_lowercase().contains(&f)
        });
    }

    // Sort
    let sort_col = state.pgs.sort_column;
    let sort_asc = state.pgs.sort_ascending;
    rows_data.sort_by(|a, b| {
        let cmp = a
            .sort_key(mode, sort_col)
            .partial_cmp(&b.sort_key(mode, sort_col))
            .unwrap_or(std::cmp::Ordering::Equal);
        if sort_asc { cmp } else { cmp.reverse() }
    });

    // Resolve selection: navigate_to, tracking, clamping, ratatui sync
    let row_queryids: Vec<i64> = rows_data.iter().map(|r| r.queryid).collect();
    state.pgs.resolve_selection(&row_queryids);

    let (headers, widths, title_mode) = match mode {
        PgStatementsViewMode::Time => (PGS_HEADERS_TIME, PGS_WIDTHS_TIME, "t:time"),
        PgStatementsViewMode::Calls => (PGS_HEADERS_CALLS, PGS_WIDTHS_CALLS, "c:calls"),
        PgStatementsViewMode::Io => (PGS_HEADERS_IO, PGS_WIDTHS_IO, "i:io"),
        PgStatementsViewMode::Temp => (PGS_HEADERS_TEMP, PGS_WIDTHS_TEMP, "e:temp"),
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
            let is_selected = idx == state.pgs.selected;
            let base_style = if is_selected {
                Styles::selected()
            } else {
                r.row_style(mode)
            };

            let cells: Vec<Span> = match mode {
                PgStatementsViewMode::Time => vec![
                    Span::raw(format_opt_f64(r.calls_s, 9, 1)),
                    Span::raw(format_opt_f64(r.exec_time_ms_s, 9, 1)),
                    Span::raw(format!("{:>7.1}", r.mean_exec_time)),
                    Span::raw(format_opt_f64(r.rows_s, 9, 1)),
                    Span::raw(truncate(&r.db, 20)),
                    Span::raw(truncate(&r.user, 20)),
                    Span::raw(normalize_query(&r.query)),
                ],
                PgStatementsViewMode::Calls => vec![
                    Span::raw(format_opt_f64(r.calls_s, 9, 1)),
                    Span::raw(format_opt_f64(r.rows_s, 9, 1)),
                    Span::raw(format_opt_f64(r.rows_per_call_s, 9, 2)),
                    Span::raw(format!("{:>7.1}", r.mean_exec_time)),
                    Span::raw(truncate(&r.db, 20)),
                    Span::raw(truncate(&r.user, 20)),
                    Span::raw(normalize_query(&r.query)),
                ],
                PgStatementsViewMode::Io => vec![
                    Span::raw(format_opt_f64(r.calls_s, 9, 1)),
                    Span::raw(format_opt_f64(r.shared_blks_read_s, 9, 1)),
                    Span::raw(format_opt_f64(r.shared_blks_hit_s, 9, 1)),
                    Span::styled(
                        format_opt_f64(r.hit_pct_s, 5, 1),
                        hit_pct_style(r.hit_pct_s.unwrap_or(0.0)),
                    ),
                    Span::raw(format_opt_f64(r.shared_blks_dirtied_s, 9, 1)),
                    Span::raw(format_opt_f64(r.shared_blks_written_s, 9, 1)),
                    Span::raw(truncate(&r.db, 20)),
                    Span::raw(normalize_query(&r.query)),
                ],
                PgStatementsViewMode::Temp => vec![
                    Span::raw(format_opt_f64(r.calls_s, 9, 1)),
                    Span::raw(format_opt_f64(r.temp_blks_read_s, 9, 1)),
                    Span::raw(format_opt_f64(r.temp_blks_written_s, 9, 1)),
                    Span::raw(format_opt_f64(r.temp_mb_s, 9, 2)),
                    Span::raw(format_opt_f64(r.local_blks_read_s, 9, 1)),
                    Span::raw(format_opt_f64(r.local_blks_written_s, 9, 1)),
                    Span::raw(truncate(&r.db, 20)),
                    Span::raw(normalize_query(&r.query)),
                ],
            };

            Row::new(cells).style(base_style).height(1)
        })
        .collect();

    // Build sample info string: [dt=Xs, age=Ys]
    // In live mode, age shows time since last collection relative to NOW (increases between ticks).
    // In history mode, age shows staleness relative to snapshot timestamp.
    let sample_info = match (state.pgs.dt_secs, state.pgs.last_real_update_ts) {
        (Some(dt), Some(last_ts)) => {
            let age = if state.is_live {
                // Live mode: age relative to current time
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                now.saturating_sub(last_ts)
            } else {
                // History mode: age relative to snapshot timestamp
                snapshot.timestamp.saturating_sub(last_ts)
            };
            format!("[dt={:.0}s, age={}s]", dt, age)
        }
        (Some(dt), None) => format!("[dt={:.0}s]", dt),
        _ => String::new(),
    };

    let title = if let Some(filter) = &state.pgs.filter {
        format!(
            " PostgreSQL Statements (PGS) [{title_mode}] {sample_info} (filter: {filter}) [{} rows] ",
            rows_data.len()
        )
    } else {
        format!(
            " PostgreSQL Statements (PGS) [{title_mode}] {sample_info} [{} rows] ",
            rows_data.len()
        )
    };

    // Build widths with QUERY taking remaining space
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

    // Clear the area before rendering to avoid artifacts
    frame.render_widget(Clear, area);
    frame.render_stateful_widget(table, area, &mut state.pgs.ratatui_state);
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

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len.saturating_sub(1)])
    }
}

/// Normalize query text for single-line display.
/// Replaces newlines, carriage returns, and tabs with spaces.
fn normalize_query(s: &str) -> String {
    s.replace('\n', " ").replace('\r', "").replace('\t', " ")
}

fn hit_pct_style(hit_pct: f64) -> Style {
    if hit_pct < 90.0 {
        Style::default().fg(Color::Red)
    } else if hit_pct < 98.0 {
        Style::default().fg(Color::Yellow)
    } else {
        Styles::default()
    }
}
