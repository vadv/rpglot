//! PostgreSQL store plans table widget for PGP tab.
//!
//! Renders the pg_store_plans table with Time, IO, and Regression view modes.
//! Unlike PGS, the view model is built inline (no separate view module).

use std::cmp::Ordering;
use std::collections::HashMap;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table};

use crate::fmt::{format_opt_f64, normalize_query, truncate};
use crate::models::PgStorePlansViewMode;
use crate::storage::StringInterner;
use crate::storage::model::{DataBlock, PgStorePlansInfo, Snapshot};
use crate::tui::state::{AppState, PgStorePlansTabState, SortKey};
use crate::tui::style::Styles;
use crate::view::common::{RowStyleClass, TableViewModel, ViewCell, ViewRow};

// Column headers per view mode
const PGP_HEADERS_TIME: &[&str] = &[
    "CALLS/s", "TIME/s", "MEAN", "MAX", "ROWS/s", "DB", "QID", "PLAN",
];
const PGP_HEADERS_IO: &[&str] = &[
    "CALLS/s",
    "BLK_RD/s",
    "BLK_HIT/s",
    "HIT%",
    "BLK_WR/s",
    "DB",
    "PLAN",
];
const PGP_HEADERS_REGRESSION: &[&str] = &["CALLS/s", "MEAN", "MAX", "MIN", "RATIO", "DB", "PLAN"];

// Column widths (last column — PLAN — gets Fill(1), not listed)
const PGP_WIDTHS_TIME: &[u16] = &[10, 10, 8, 8, 10, 20, 19];
const PGP_WIDTHS_IO: &[u16] = &[10, 10, 10, 6, 10, 20];
const PGP_WIDTHS_REGRESSION: &[u16] = &[10, 8, 8, 8, 8, 20];

pub fn render_pg_store_plans(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    interner: Option<&StringInterner>,
) {
    let snapshot = match &state.current_snapshot {
        Some(s) => s,
        None => {
            let block = Block::default()
                .title(" PostgreSQL Store Plans (PGP) ")
                .borders(Borders::ALL)
                .style(Styles::default());
            frame.render_widget(Paragraph::new("No data available").block(block), area);
            return;
        }
    };

    let vm = match build_store_plans_view(snapshot, &state.pgp, interner) {
        Some(vm) => vm,
        None => {
            let block = Block::default()
                .title(" PostgreSQL Store Plans (PGP) ")
                .borders(Borders::ALL)
                .style(Styles::default());
            frame.render_widget(
                Paragraph::new("pg_store_plans is not available").block(block),
                area,
            );
            return;
        }
    };

    // Resolve selection
    let row_planids: Vec<i64> = vm.rows.iter().map(|r| r.id).collect();
    state.pgp.resolve_selection(&row_planids);

    // Header with sort indicator
    let headers: Vec<Span> = vm
        .headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let indicator = if i == vm.sort_column {
                if vm.sort_ascending { "▲" } else { "▼" }
            } else {
                ""
            };
            Span::styled(format!("{}{}", h, indicator), Styles::table_header())
        })
        .collect();
    let header = Row::new(headers).style(Styles::table_header()).height(1);

    // Rows
    let rows: Vec<Row> = vm
        .rows
        .iter()
        .enumerate()
        .map(|(idx, vr)| {
            let is_selected = idx == state.pgp.selected;
            let base_style = if is_selected {
                Styles::selected()
            } else {
                Styles::from_class(vr.style)
            };

            let cells = vr.cells.iter().map(|c| match c.style {
                Some(s) => Span::styled(c.text.clone(), Styles::from_class(s)),
                None => Span::raw(c.text.clone()),
            });
            Row::new(cells).style(base_style).height(1)
        })
        .collect();

    let mut constraints: Vec<ratatui::layout::Constraint> = vm
        .widths
        .iter()
        .map(|&w| ratatui::layout::Constraint::Length(w))
        .collect();
    constraints.push(ratatui::layout::Constraint::Fill(1));

    let table = Table::new(rows, constraints)
        .header(header)
        .block(
            Block::default()
                .title(vm.title)
                .borders(Borders::ALL)
                .style(Styles::default()),
        )
        .column_spacing(1)
        .row_highlight_style(Styles::selected());

    frame.render_widget(Clear, area);
    frame.render_stateful_widget(table, area, &mut state.pgp.ratatui_state);
}

// ---------------------------------------------------------------------------
// View model builder
// ---------------------------------------------------------------------------

/// Internal row data for sorting/filtering before rendering.
#[derive(Debug, Clone)]
struct PgStorePlansRowData {
    planid: i64,
    stmt_queryid: i64,
    db: String,
    plan: String,
    mean_time: f64,
    max_time: f64,
    min_time: f64,
    calls_s: Option<f64>,
    rows_s: Option<f64>,
    exec_time_ms_s: Option<f64>,
    shared_blks_read_s: Option<f64>,
    shared_blks_hit_s: Option<f64>,
    shared_blks_written_s: Option<f64>,
    hit_pct_s: Option<f64>,
    /// Regression ratio: max_mean / min_mean across all plans for same stmt_queryid.
    regression_ratio: Option<f64>,
}

impl PgStorePlansRowData {
    fn from_plan(p: &PgStorePlansInfo, interner: Option<&StringInterner>) -> Self {
        let db = resolve_hash(interner, p.datname_hash);
        let plan = resolve_hash(interner, p.plan_hash);

        Self {
            planid: p.planid,
            stmt_queryid: p.stmt_queryid,
            db,
            plan,
            mean_time: p.mean_time,
            max_time: p.max_time,
            min_time: p.min_time,
            calls_s: None,
            rows_s: None,
            exec_time_ms_s: None,
            shared_blks_read_s: None,
            shared_blks_hit_s: None,
            shared_blks_written_s: None,
            hit_pct_s: None,
            regression_ratio: None,
        }
    }

    fn sort_key(&self, mode: PgStorePlansViewMode, col: usize) -> SortKey {
        match mode {
            PgStorePlansViewMode::Time => match col {
                0 => SortKey::Float(self.calls_s.unwrap_or(0.0)),
                1 => SortKey::Float(self.exec_time_ms_s.unwrap_or(0.0)),
                2 => SortKey::Float(self.mean_time),
                3 => SortKey::Float(self.max_time),
                4 => SortKey::Float(self.rows_s.unwrap_or(0.0)),
                5 => SortKey::String(self.db.clone()),
                6 => SortKey::Integer(self.stmt_queryid),
                7 => SortKey::String(self.plan.clone()),
                _ => SortKey::Integer(0),
            },
            PgStorePlansViewMode::Io => match col {
                0 => SortKey::Float(self.calls_s.unwrap_or(0.0)),
                1 => SortKey::Float(self.shared_blks_read_s.unwrap_or(0.0)),
                2 => SortKey::Float(self.shared_blks_hit_s.unwrap_or(0.0)),
                3 => SortKey::Float(self.hit_pct_s.unwrap_or(0.0)),
                4 => SortKey::Float(self.shared_blks_written_s.unwrap_or(0.0)),
                5 => SortKey::String(self.db.clone()),
                6 => SortKey::String(self.plan.clone()),
                _ => SortKey::Integer(0),
            },
            PgStorePlansViewMode::Regression => match col {
                0 => SortKey::Float(self.calls_s.unwrap_or(0.0)),
                1 => SortKey::Float(self.mean_time),
                2 => SortKey::Float(self.max_time),
                3 => SortKey::Float(self.min_time),
                4 => SortKey::Float(self.regression_ratio.unwrap_or(0.0)),
                5 => SortKey::String(self.db.clone()),
                6 => SortKey::String(self.plan.clone()),
                _ => SortKey::Integer(0),
            },
        }
    }

    fn row_style(&self, mode: PgStorePlansViewMode) -> RowStyleClass {
        match mode {
            PgStorePlansViewMode::Time => {
                let time_ms_s = self.exec_time_ms_s.unwrap_or(0.0);
                if time_ms_s >= 1_000.0 {
                    RowStyleClass::Critical
                } else if time_ms_s >= 100.0 {
                    RowStyleClass::Warning
                } else {
                    RowStyleClass::Normal
                }
            }
            PgStorePlansViewMode::Io => {
                let rd_s = self.shared_blks_read_s.unwrap_or(0.0);
                if rd_s >= 10_000.0 {
                    RowStyleClass::Critical
                } else if rd_s >= 1_000.0 {
                    RowStyleClass::Warning
                } else {
                    RowStyleClass::Normal
                }
            }
            PgStorePlansViewMode::Regression => {
                let ratio = self.regression_ratio.unwrap_or(0.0);
                if ratio >= 10.0 {
                    RowStyleClass::Critical
                } else if ratio >= 5.0 {
                    RowStyleClass::Warning
                } else {
                    RowStyleClass::Normal
                }
            }
        }
    }

    fn cells(&self, mode: PgStorePlansViewMode) -> Vec<ViewCell> {
        match mode {
            PgStorePlansViewMode::Time => vec![
                ViewCell::plain(format_opt_f64(self.calls_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.exec_time_ms_s, 9, 1)),
                ViewCell::plain(format!("{:>7.1}", self.mean_time)),
                ViewCell::plain(format!("{:>7.1}", self.max_time)),
                ViewCell::plain(format_opt_f64(self.rows_s, 9, 1)),
                ViewCell::plain(truncate(&self.db, 20)),
                ViewCell::plain(format!("{:>19}", self.stmt_queryid)),
                ViewCell::plain(normalize_query(&self.plan)),
            ],
            PgStorePlansViewMode::Io => vec![
                ViewCell::plain(format_opt_f64(self.calls_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.shared_blks_read_s, 9, 1)),
                ViewCell::plain(format_opt_f64(self.shared_blks_hit_s, 9, 1)),
                ViewCell::styled(
                    format_opt_f64(self.hit_pct_s, 5, 1),
                    hit_pct_style_class(self.hit_pct_s.unwrap_or(0.0)),
                ),
                ViewCell::plain(format_opt_f64(self.shared_blks_written_s, 9, 1)),
                ViewCell::plain(truncate(&self.db, 20)),
                ViewCell::plain(normalize_query(&self.plan)),
            ],
            PgStorePlansViewMode::Regression => vec![
                ViewCell::plain(format_opt_f64(self.calls_s, 9, 1)),
                ViewCell::plain(format!("{:>7.1}", self.mean_time)),
                ViewCell::plain(format!("{:>7.1}", self.max_time)),
                ViewCell::plain(format!("{:>7.1}", self.min_time)),
                ViewCell::styled(
                    self.regression_ratio
                        .map(|r| format!("{:>7.1}", r))
                        .unwrap_or_else(|| format!("{:>7}", "--")),
                    regression_ratio_style(self.regression_ratio.unwrap_or(0.0)),
                ),
                ViewCell::plain(truncate(&self.db, 20)),
                ViewCell::plain(normalize_query(&self.plan)),
            ],
        }
    }
}

fn hit_pct_style_class(hit_pct: f64) -> RowStyleClass {
    if hit_pct < 90.0 {
        RowStyleClass::Critical
    } else if hit_pct < 98.0 {
        RowStyleClass::Warning
    } else {
        RowStyleClass::Normal
    }
}

fn regression_ratio_style(ratio: f64) -> RowStyleClass {
    if ratio >= 10.0 {
        RowStyleClass::Critical
    } else if ratio >= 5.0 {
        RowStyleClass::Warning
    } else {
        RowStyleClass::Normal
    }
}

/// Compute regression ratios: for each stmt_queryid with >1 planid,
/// ratio = max(mean_time) / min(mean_time) across all plans.
/// Only plans where ratio > 2.0 are included.
fn compute_regression_ratios(plans: &[&PgStorePlansInfo]) -> HashMap<i64, f64> {
    // Group mean_time by stmt_queryid
    let mut groups: HashMap<i64, Vec<f64>> = HashMap::new();
    for p in plans {
        groups.entry(p.stmt_queryid).or_default().push(p.mean_time);
    }

    let mut ratios = HashMap::new();
    for (qid, means) in &groups {
        if means.len() <= 1 {
            continue;
        }
        let min_mean = means.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_mean = means.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        if min_mean > 0.0 {
            let ratio = max_mean / min_mean;
            if ratio > 2.0 {
                ratios.insert(*qid, ratio);
            }
        }
    }
    ratios
}

fn build_store_plans_view(
    snapshot: &Snapshot,
    state: &PgStorePlansTabState,
    interner: Option<&StringInterner>,
) -> Option<TableViewModel<i64>> {
    let plans = extract_pg_store_plans(snapshot);
    if plans.is_empty() {
        return None;
    }

    let mode = state.view_mode;

    // For Regression mode, pre-compute ratios
    let regression_ratios = if mode == PgStorePlansViewMode::Regression {
        compute_regression_ratios(&plans)
    } else {
        HashMap::new()
    };

    let mut rows_data: Vec<PgStorePlansRowData> = plans
        .iter()
        .map(|p| {
            let mut row = PgStorePlansRowData::from_plan(p, interner);
            if let Some(r) = state.rate_state.rates.get(&p.planid) {
                row.calls_s = r.calls_s;
                row.rows_s = r.rows_s;
                row.exec_time_ms_s = r.exec_time_ms_s;
                row.shared_blks_read_s = r.shared_blks_read_s;
                row.shared_blks_hit_s = r.shared_blks_hit_s;
                row.shared_blks_written_s = r.shared_blks_written_s;

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
            if let Some(&ratio) = regression_ratios.get(&p.stmt_queryid) {
                row.regression_ratio = Some(ratio);
            }
            row
        })
        .collect();

    // For Regression mode, keep only plans whose stmt_queryid is in regression_ratios
    if mode == PgStorePlansViewMode::Regression {
        rows_data.retain(|r| regression_ratios.contains_key(&r.stmt_queryid));
    }

    // Filter
    if let Some(filter) = &state.filter {
        let f = filter.to_lowercase();
        rows_data.retain(|r| {
            r.planid.to_string().starts_with(&f)
                || r.stmt_queryid.to_string().starts_with(&f)
                || r.db.to_lowercase().contains(&f)
                || r.plan.to_lowercase().contains(&f)
        });
    }

    // Sort
    let sort_col = state.sort_column;
    let sort_asc = state.sort_ascending;
    rows_data.sort_by(|a, b| {
        let cmp = a
            .sort_key(mode, sort_col)
            .partial_cmp(&b.sort_key(mode, sort_col))
            .unwrap_or(Ordering::Equal);
        if sort_asc { cmp } else { cmp.reverse() }
    });

    let (headers, widths, title_mode) = match mode {
        PgStorePlansViewMode::Time => (PGP_HEADERS_TIME, PGP_WIDTHS_TIME, "t:time"),
        PgStorePlansViewMode::Io => (PGP_HEADERS_IO, PGP_WIDTHS_IO, "i:io"),
        PgStorePlansViewMode::Regression => (
            PGP_HEADERS_REGRESSION,
            PGP_WIDTHS_REGRESSION,
            "r:regression",
        ),
    };

    let rows: Vec<ViewRow<i64>> = rows_data
        .iter()
        .map(|r| ViewRow {
            id: r.planid,
            cells: r.cells(mode),
            style: r.row_style(mode),
        })
        .collect();

    // Build sample info
    let sample_info = state
        .rate_state
        .rates
        .values()
        .next()
        .map(|r| format!("[dt={:.0}s]", r.dt_secs))
        .unwrap_or_default();

    let title = if let Some(filter) = &state.filter {
        format!(
            " PostgreSQL Store Plans (PGP) [{title_mode}] {sample_info} (filter: {filter}) [{} rows] ",
            rows.len()
        )
    } else {
        format!(
            " PostgreSQL Store Plans (PGP) [{title_mode}] {sample_info} [{} rows] ",
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

fn extract_pg_store_plans(snapshot: &Snapshot) -> Vec<&PgStorePlansInfo> {
    snapshot
        .blocks
        .iter()
        .find_map(|b| {
            if let DataBlock::PgStorePlans(v) = b {
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
