//! PGA (pg_stat_activity) view model.

use crate::fmt::{self, FmtStyle};
use crate::models::PgActivityViewMode;
use crate::storage::StringInterner;
use crate::storage::model::{
    DataBlock, PgStatActivityInfo, PgStatStatementsInfo, ProcessInfo, Snapshot,
};
use crate::tui::state::{PgActivityTabState, PgStatementsRates, PgStatementsTabState, SortKey};
use crate::view::common::{RowStyleClass, TableViewModel, ViewCell, ViewRow};
use std::collections::HashMap;

const PGA_HEADERS_GENERIC: &[&str] = &[
    "PID", "CPU%", "RSS", "DB", "USER", "STATE", "WAIT", "QDUR", "XDUR", "BDUR", "BTYPE", "QUERY",
];
const PGA_HEADERS_STATS: &[&str] = &[
    "PID", "DB", "USER", "STATE", "QDUR", "MEAN", "MAX", "CALL/s", "HIT%", "QUERY",
];

const PGA_WIDTHS_GENERIC: &[u16] = &[7, 6, 8, 32, 32, 16, 20, 8, 8, 8, 14];
const PGA_WIDTHS_STATS: &[u16] = &[7, 32, 32, 16, 8, 8, 8, 8, 6];

struct PgActivityRowData {
    pid: i32,
    cpu_percent: f64,
    rss_bytes: u64,
    db: String,
    user: String,
    state: String,
    wait: String,
    query: String,
    backend_type: String,
    application_name: String,
    query_duration_secs: Option<i64>,
    xact_duration_secs: Option<i64>,
    backend_duration_secs: Option<i64>,
    query_id: i64,
    pgs_mean_exec_time: Option<f64>,
    pgs_max_exec_time: Option<f64>,
    pgs_calls_s: Option<f64>,
    pgs_hit_pct: Option<f64>,
}

impl PgActivityRowData {
    fn from_pg_activity(
        pg: &PgStatActivityInfo,
        process: Option<&&ProcessInfo>,
        now: i64,
        interner: Option<&StringInterner>,
    ) -> Self {
        let (_cpu_percent, rss_bytes) = process
            .map(|p| (0.0, p.mem.rmem * 1024))
            .unwrap_or((0.0, 0));

        let db = resolve_hash(interner, pg.datname_hash);
        let user = resolve_hash(interner, pg.usename_hash);
        let state = resolve_hash(interner, pg.state_hash);
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
        let query = resolve_hash(interner, pg.query_hash);
        let backend_type = resolve_hash(interner, pg.backend_type_hash);
        let application_name = resolve_hash(interner, pg.application_name_hash);

        let query_duration_secs = if pg.query_start > 0 {
            Some(now.saturating_sub(pg.query_start))
        } else {
            None
        };
        let xact_duration_secs = if pg.xact_start > 0 {
            Some(now.saturating_sub(pg.xact_start))
        } else {
            None
        };
        let backend_duration_secs = if pg.backend_start > 0 {
            Some(now.saturating_sub(pg.backend_start))
        } else {
            None
        };

        Self {
            pid: pg.pid,
            cpu_percent: 0.0,
            rss_bytes,
            db,
            user,
            state,
            wait,
            query,
            backend_type,
            application_name,
            query_duration_secs,
            xact_duration_secs,
            backend_duration_secs,
            query_id: pg.query_id,
            pgs_mean_exec_time: None,
            pgs_max_exec_time: None,
            pgs_calls_s: None,
            pgs_hit_pct: None,
        }
    }

    fn enrich_with_pgs_stats(
        &mut self,
        pgs_info: &PgStatStatementsInfo,
        rates: Option<&PgStatementsRates>,
    ) {
        self.pgs_mean_exec_time = Some(pgs_info.mean_exec_time);
        self.pgs_max_exec_time = Some(pgs_info.max_exec_time);
        self.pgs_calls_s = rates.and_then(|r| r.calls_s);
        let total_blks = pgs_info.shared_blks_hit + pgs_info.shared_blks_read;
        if total_blks > 0 {
            self.pgs_hit_pct = Some(pgs_info.shared_blks_hit as f64 / total_blks as f64 * 100.0);
        }
    }

    fn sort_key_for_mode(&self, col: usize, mode: PgActivityViewMode) -> SortKey {
        match mode {
            PgActivityViewMode::Generic => match col {
                0 => SortKey::Integer(self.pid as i64),
                1 => SortKey::Float(self.cpu_percent),
                2 => SortKey::Integer(self.rss_bytes as i64),
                3 => SortKey::String(self.db.to_lowercase()),
                4 => SortKey::String(self.user.to_lowercase()),
                5 => SortKey::String(self.state.to_lowercase()),
                6 => SortKey::String(self.wait.to_lowercase()),
                7 => SortKey::Integer(self.query_duration_secs.unwrap_or(-1)),
                8 => SortKey::Integer(self.xact_duration_secs.unwrap_or(-1)),
                9 => SortKey::Integer(self.backend_duration_secs.unwrap_or(-1)),
                10 => SortKey::String(self.backend_type.to_lowercase()),
                11 => SortKey::String(self.query.to_lowercase()),
                _ => SortKey::Integer(0),
            },
            PgActivityViewMode::Stats => match col {
                0 => SortKey::Integer(self.pid as i64),
                1 => SortKey::String(self.db.to_lowercase()),
                2 => SortKey::String(self.user.to_lowercase()),
                3 => SortKey::String(self.state.to_lowercase()),
                4 => SortKey::Integer(self.query_duration_secs.unwrap_or(-1)),
                5 => SortKey::Float(self.pgs_mean_exec_time.unwrap_or(0.0)),
                6 => SortKey::Float(self.pgs_max_exec_time.unwrap_or(0.0)),
                7 => SortKey::Float(self.pgs_calls_s.unwrap_or(0.0)),
                8 => SortKey::Float(self.pgs_hit_pct.unwrap_or(0.0)),
                9 => SortKey::String(self.query.to_lowercase()),
                _ => SortKey::Integer(0),
            },
        }
    }

    fn cells_generic(&self) -> Vec<ViewCell> {
        let is_idle = is_idle_state(&self.state);

        vec![
            ViewCell::plain(format!("{:>6}", self.pid)),
            ViewCell::styled(
                format!("{:>5.1}", self.cpu_percent),
                styled_cpu_class(self.cpu_percent),
            ),
            ViewCell::plain(format!(
                "{:>7}",
                fmt::format_bytes(self.rss_bytes, FmtStyle::Compact)
            )),
            ViewCell::plain(truncate(&self.db, 32)),
            ViewCell::plain(truncate(&self.user, 32)),
            ViewCell::styled(truncate(&self.state, 16), styled_state_class(&self.state)),
            ViewCell::styled(
                truncate(&self.wait, 20),
                styled_wait_class(&self.wait, is_idle),
            ),
            ViewCell::styled(
                format_duration_or_dash(self.query_duration_secs),
                styled_duration_class(self.query_duration_secs, &self.state),
            ),
            ViewCell::plain(format_duration_or_dash(self.xact_duration_secs)),
            ViewCell::plain(format_duration_or_dash(self.backend_duration_secs)),
            ViewCell::plain(truncate(&self.backend_type, 14)),
            ViewCell::plain(fmt::normalize_query(&self.query)),
        ]
    }

    fn cells_stats(&self) -> Vec<ViewCell> {
        vec![
            ViewCell::plain(format!("{:>6}", self.pid)),
            ViewCell::plain(truncate(&self.db, 32)),
            ViewCell::plain(truncate(&self.user, 32)),
            ViewCell::styled(truncate(&self.state, 16), styled_state_class(&self.state)),
            ViewCell::styled(
                format_duration_or_dash(self.query_duration_secs),
                styled_qdur_anomaly_class(
                    self.query_duration_secs,
                    &self.state,
                    self.pgs_mean_exec_time,
                    self.pgs_max_exec_time,
                ),
            ),
            ViewCell::plain(format_ms_or_dash(self.pgs_mean_exec_time)),
            ViewCell::plain(format_ms_or_dash(self.pgs_max_exec_time)),
            ViewCell::plain(format_calls_s(self.pgs_calls_s)),
            ViewCell::styled(
                format_hit_pct(self.pgs_hit_pct),
                styled_hit_pct_class(self.pgs_hit_pct),
            ),
            ViewCell::plain(fmt::normalize_query(&self.query)),
        ]
    }

    fn row_style(&self) -> RowStyleClass {
        if is_idle_state(&self.state) {
            RowStyleClass::Dimmed
        } else {
            RowStyleClass::Normal
        }
    }
}

// ========== Style classification helpers ==========

fn is_idle_state(state: &str) -> bool {
    let lower = state.to_lowercase();
    lower.contains("idle") && !lower.contains("trans")
}

fn styled_cpu_class(cpu: f64) -> RowStyleClass {
    if cpu > 80.0 {
        RowStyleClass::CriticalBold
    } else if cpu > 50.0 {
        RowStyleClass::Warning
    } else {
        RowStyleClass::Normal
    }
}

fn styled_state_class(state: &str) -> RowStyleClass {
    let lower = state.to_lowercase();
    if lower.contains("idle") && lower.contains("trans") {
        RowStyleClass::Warning
    } else if lower == "active" {
        RowStyleClass::Active
    } else if lower.contains("idle") {
        RowStyleClass::Dimmed
    } else {
        RowStyleClass::Normal
    }
}

fn styled_wait_class(wait: &str, is_idle: bool) -> RowStyleClass {
    if wait != "-" && !is_idle {
        RowStyleClass::Warning
    } else {
        RowStyleClass::Normal
    }
}

fn styled_duration_class(secs: Option<i64>, state: &str) -> RowStyleClass {
    let lower_state = state.to_lowercase();
    let is_active = lower_state == "active" || lower_state.contains("trans");
    let s = secs.unwrap_or(0);

    if is_active && s > 300 {
        RowStyleClass::CriticalBold
    } else if is_active && s > 60 {
        RowStyleClass::Warning
    } else {
        RowStyleClass::Normal
    }
}

fn styled_qdur_anomaly_class(
    secs: Option<i64>,
    state: &str,
    mean_exec_time_ms: Option<f64>,
    max_exec_time_ms: Option<f64>,
) -> RowStyleClass {
    let lower_state = state.to_lowercase();
    let is_active = lower_state == "active" || lower_state.contains("trans");
    if !is_active {
        return RowStyleClass::Normal;
    }

    let qdur_ms = (secs.unwrap_or(0) * 1000) as f64;

    if let Some(max_ms) = max_exec_time_ms
        && max_ms > 0.0
        && qdur_ms > max_ms
    {
        return RowStyleClass::CriticalBold;
    }
    if let Some(mean_ms) = mean_exec_time_ms {
        if mean_ms > 0.0 && qdur_ms > mean_ms * 5.0 {
            return RowStyleClass::Critical;
        }
        if mean_ms > 0.0 && qdur_ms > mean_ms * 2.0 {
            return RowStyleClass::Warning;
        }
    }
    RowStyleClass::Normal
}

fn styled_hit_pct_class(hit_pct: Option<f64>) -> RowStyleClass {
    match hit_pct {
        Some(pct) if pct < 50.0 => RowStyleClass::Critical,
        Some(pct) if pct < 80.0 => RowStyleClass::Warning,
        _ => RowStyleClass::Normal,
    }
}

// ========== Formatting helpers ==========

fn format_duration_compact(secs: i64) -> String {
    fmt::format_duration(secs, FmtStyle::Compact)
}

fn format_duration_or_dash(secs: Option<i64>) -> String {
    match secs {
        Some(s) if s >= 0 => format!("{:>7}", format_duration_compact(s)),
        _ => format!("{:>7}", "-"),
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        format!("{:<width$}", s, width = max_len)
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

fn format_ms_or_dash(ms: Option<f64>) -> String {
    match ms {
        Some(ms) => format!("{:>7}", fmt::format_ms(ms, FmtStyle::Compact)),
        None => format!("{:>7}", "--"),
    }
}

fn format_calls_s(calls_s: Option<f64>) -> String {
    match calls_s {
        Some(rate) => {
            let text = if rate >= 1000.0 {
                format!("{:.1}K", rate / 1000.0)
            } else if rate >= 1.0 {
                format!("{:.1}", rate)
            } else {
                format!("{:.2}", rate)
            };
            format!("{:>7}", text)
        }
        None => format!("{:>7}", "--"),
    }
}

fn format_hit_pct(hit_pct: Option<f64>) -> String {
    match hit_pct {
        Some(pct) => format!("{:>5}", format!("{:.0}%", pct)),
        None => format!("{:>5}", "--"),
    }
}

// ========== Build function ==========

/// Builds a UI-agnostic view model for the PGA (activity) tab.
pub fn build_activity_view(
    snapshot: &Snapshot,
    pga_state: &PgActivityTabState,
    pgs_state: &PgStatementsTabState,
    interner: Option<&StringInterner>,
) -> Option<TableViewModel<i32>> {
    let pg_activities = extract_pg_activities(snapshot);
    if pg_activities.is_empty() {
        return None;
    }

    let processes = extract_processes(snapshot);
    let process_map: HashMap<u32, &ProcessInfo> = processes.iter().map(|p| (p.pid, *p)).collect();

    let now = snapshot.timestamp;
    let view_mode = pga_state.view_mode;

    let mut rows_data: Vec<PgActivityRowData> = pg_activities
        .iter()
        .map(|pg| {
            let process = if pg.pid > 0 {
                process_map.get(&(pg.pid as u32))
            } else {
                None
            };
            PgActivityRowData::from_pg_activity(pg, process, now, interner)
        })
        .collect();

    // Filter
    if let Some(filter) = &pga_state.filter {
        let filter_lower = filter.to_lowercase();
        rows_data.retain(|row| {
            row.pid.to_string().starts_with(&filter_lower)
                || (row.query_id != 0 && row.query_id.to_string().starts_with(&filter_lower))
                || row.db.to_lowercase().contains(&filter_lower)
                || row.user.to_lowercase().contains(&filter_lower)
                || row.query.to_lowercase().contains(&filter_lower)
        });
    }

    // Hide idle
    if pga_state.hide_idle {
        rows_data.retain(|row| !is_idle_state(&row.state));
    }

    // Hide system backends (non-client, non-autovacuum) and rpglot's own sessions
    if pga_state.hide_system {
        rows_data.retain(|row| {
            (row.backend_type == "client backend" || row.backend_type == "autovacuum worker")
                && !row.application_name.starts_with("rpglot")
        });
    }

    // Enrich with PGS stats (Stats view)
    if view_mode == PgActivityViewMode::Stats {
        let pgs_map = extract_pg_statements_map(snapshot);
        for row in &mut rows_data {
            if row.query_id != 0
                && let Some(pgs_info) = pgs_map.get(&row.query_id)
            {
                let rates = pgs_state.rates.get(&row.query_id);
                row.enrich_with_pgs_stats(pgs_info, rates);
            }
        }
    }

    // Sort with idle at bottom
    let sort_col = pga_state.sort_column;
    let sort_asc = pga_state.sort_ascending;
    rows_data.sort_by(|a, b| {
        let a_idle = is_idle_state(&a.state);
        let b_idle = is_idle_state(&b.state);

        match (a_idle, b_idle) {
            (true, false) => return std::cmp::Ordering::Greater,
            (false, true) => return std::cmp::Ordering::Less,
            _ => {}
        }

        let cmp = a
            .sort_key_for_mode(sort_col, view_mode)
            .partial_cmp(&b.sort_key_for_mode(sort_col, view_mode))
            .unwrap_or(std::cmp::Ordering::Equal);
        if sort_asc { cmp } else { cmp.reverse() }
    });

    let (headers, widths, view_indicator) = match view_mode {
        PgActivityViewMode::Generic => (PGA_HEADERS_GENERIC, PGA_WIDTHS_GENERIC, "g:generic"),
        PgActivityViewMode::Stats => (PGA_HEADERS_STATS, PGA_WIDTHS_STATS, "v:stats"),
    };

    let rows: Vec<ViewRow<i32>> = rows_data
        .iter()
        .map(|row| {
            let cells = match view_mode {
                PgActivityViewMode::Generic => row.cells_generic(),
                PgActivityViewMode::Stats => row.cells_stats(),
            };
            ViewRow {
                id: row.pid,
                cells,
                style: row.row_style(),
            }
        })
        .collect();

    let title = {
        let mut markers = String::new();
        if pga_state.hide_idle {
            markers.push_str(" [hide idle]");
        }
        if pga_state.hide_system {
            markers.push_str(" [hide sys]");
        }
        if let Some(filter) = &pga_state.filter {
            format!(
                " PostgreSQL Activity (PGA) [{}] (filter: {}){} [{} sessions] ",
                view_indicator,
                filter,
                markers,
                rows.len()
            )
        } else {
            format!(
                " PostgreSQL Activity (PGA) [{}]{} [{} sessions] ",
                view_indicator,
                markers,
                rows.len()
            )
        }
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

// ========== Data extraction ==========

fn extract_pg_activities(snapshot: &Snapshot) -> Vec<&PgStatActivityInfo> {
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

fn extract_processes(snapshot: &Snapshot) -> Vec<&ProcessInfo> {
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

fn extract_pg_statements_map(snapshot: &Snapshot) -> HashMap<i64, &PgStatStatementsInfo> {
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

fn resolve_hash(interner: Option<&StringInterner>, hash: u64) -> String {
    interner
        .and_then(|i| i.resolve(hash))
        .map(|s| s.to_string())
        .unwrap_or_else(|| "-".to_string())
}
