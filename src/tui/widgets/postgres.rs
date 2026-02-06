//! PostgreSQL activity table widget for PGA tab.
//! Displays pg_stat_activity data with CPU/RSS from OS process info.
//! Stats view mode enriches data with pg_stat_statements metrics (linked by query_id).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table};

use crate::storage::StringInterner;
use crate::storage::model::{
    DataBlock, PgStatActivityInfo, PgStatStatementsInfo, ProcessInfo, Snapshot,
};
use crate::tui::state::{AppState, PgActivityViewMode, PgStatementsRates, SortKey};
use crate::tui::style::Styles;

/// Column headers for PGA table (Generic view).
const PGA_HEADERS_GENERIC: &[&str] = &[
    "PID", "CPU%", "RSS", "DB", "USER", "STATE", "WAIT", "QDUR", "XDUR", "BDUR", "BTYPE", "QUERY",
];

/// Column headers for PGA table (Stats view).
/// Shows pg_stat_statements metrics: MEAN, MAX, CALL/s, HIT%
const PGA_HEADERS_STATS: &[&str] = &[
    "PID", "DB", "USER", "STATE", "QDUR", "MEAN", "MAX", "CALL/s", "HIT%", "QUERY",
];

/// Default column widths for Generic view (QUERY uses Fill constraint, so not included here).
/// DB and USER widths accommodate names like "product-service-data-shard-120"
const PGA_WIDTHS_GENERIC: &[u16] = &[7, 6, 8, 32, 32, 16, 20, 8, 8, 8, 14];

/// Column widths for Stats view.
const PGA_WIDTHS_STATS: &[u16] = &[7, 32, 32, 16, 8, 8, 8, 8, 6];

/// Renders the PostgreSQL activity table.
pub fn render_postgres(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    interner: Option<&StringInterner>,
) {
    let snapshot = match &state.current_snapshot {
        Some(s) => s,
        None => {
            let block = Block::default()
                .title(" PostgreSQL Activity (PGA) ")
                .borders(Borders::ALL)
                .style(Styles::default());
            let paragraph = Paragraph::new("No data available").block(block);
            frame.render_widget(paragraph, area);
            return;
        }
    };

    // Extract PgStatActivity data
    let pg_activities = extract_pg_activities(snapshot);

    if pg_activities.is_empty() {
        let block = Block::default()
            .title(" PostgreSQL Activity (PGA) ")
            .borders(Borders::ALL)
            .style(Styles::default());
        let message = state
            .pg_last_error
            .as_deref()
            .unwrap_or("No active PostgreSQL sessions");
        let paragraph = Paragraph::new(message).block(block);
        frame.render_widget(paragraph, area);
        return;
    }

    // Get process info for CPU/RSS lookup (by PID)
    let processes = extract_processes(snapshot);
    let process_map: std::collections::HashMap<u32, &ProcessInfo> =
        processes.iter().map(|p| (p.pid, *p)).collect();

    // Current timestamp for duration calculation
    let now = snapshot.timestamp;

    // Build rows with sorting (idle at bottom, then by QDUR desc)
    let mut rows_data: Vec<PgActivityRowData> = pg_activities
        .iter()
        .map(|pg| {
            // Convert i32 pid to u32 for lookup (should always be positive)
            let process = if pg.pid > 0 {
                process_map.get(&(pg.pid as u32))
            } else {
                None
            };
            PgActivityRowData::from_pg_activity(pg, process, now, interner)
        })
        .collect();

    // Apply filter if set (matches PID, query_id, DB, USER, QUERY)
    if let Some(filter) = &state.pg_filter {
        let filter_lower = filter.to_lowercase();
        rows_data.retain(|row| {
            // Check PID (exact prefix match for numbers)
            row.pid.to_string().starts_with(&filter_lower)
                // Check query_id (exact prefix match for numbers)
                || (row.query_id != 0 && row.query_id.to_string().starts_with(&filter_lower))
                // Check text fields (substring match)
                || row.db.to_lowercase().contains(&filter_lower)
                || row.user.to_lowercase().contains(&filter_lower)
                || row.query.to_lowercase().contains(&filter_lower)
        });
    }

    // Hide idle sessions if flag is set
    if state.pg_hide_idle {
        rows_data.retain(|row| !is_idle_state(&row.state));
    }

    // Get view mode early as it affects sorting
    let view_mode = state.pga_view_mode;

    // For Stats view, enrich rows with pg_stat_statements data before sorting
    if view_mode == PgActivityViewMode::Stats {
        // Build map of queryid -> PgStatStatementsInfo
        let pgs_map = extract_pg_statements_map(snapshot);
        for row in &mut rows_data {
            if row.query_id != 0
                && let Some(pgs_info) = pgs_map.get(&row.query_id)
            {
                let rates = state.pgs_rates.get(&row.query_id);
                row.enrich_with_pgs_stats(pgs_info, rates);
            }
        }
    }

    // Sort by selected column, with idle sessions always at bottom
    let sort_col = state.pg_sort_column;
    let sort_asc = state.pg_sort_ascending;
    rows_data.sort_by(|a, b| {
        let a_idle = is_idle_state(&a.state);
        let b_idle = is_idle_state(&b.state);

        // Idle sessions always at bottom
        match (a_idle, b_idle) {
            (true, false) => return std::cmp::Ordering::Greater,
            (false, true) => return std::cmp::Ordering::Less,
            _ => {}
        }

        // Sort by selected column (using view mode-specific sort key)
        let cmp = a
            .sort_key_for_mode(sort_col, view_mode)
            .partial_cmp(&b.sort_key_for_mode(sort_col, view_mode))
            .unwrap_or(std::cmp::Ordering::Equal);
        if sort_asc { cmp } else { cmp.reverse() }
    });

    // Handle navigate_to_pid: set tracked PID for persistent selection
    if let Some(target_pid) = state.pga_navigate_to_pid.take() {
        state.pg_tracked_pid = Some(target_pid);
    }

    // If tracked PID is set, find and select the row with that PID
    if let Some(tracked_pid) = state.pg_tracked_pid {
        if let Some(idx) = rows_data.iter().position(|row| row.pid == tracked_pid) {
            state.pg_selected = idx;
        } else {
            // PID no longer exists in data — reset tracking
            state.pg_tracked_pid = None;
        }
    }

    // Clamp selected index and update tracked PID
    if !rows_data.is_empty() {
        state.pg_selected = state.pg_selected.min(rows_data.len() - 1);
        // Update tracked PID based on current selection (for popup and next render)
        state.pg_tracked_pid = Some(rows_data[state.pg_selected].pid);
    } else {
        state.pg_selected = 0;
        state.pg_tracked_pid = None;
    }

    // Sync ratatui TableState for auto-scrolling
    state.pga_ratatui_state.select(Some(state.pg_selected));

    // Select headers and widths based on view mode
    let (headers_arr, widths_arr, view_indicator) = match view_mode {
        PgActivityViewMode::Generic => (PGA_HEADERS_GENERIC, PGA_WIDTHS_GENERIC, "g:generic"),
        PgActivityViewMode::Stats => (PGA_HEADERS_STATS, PGA_WIDTHS_STATS, "v:stats"),
    };

    // Build header row with sort indicator
    let sort_col = state.pg_sort_column;
    let sort_asc = state.pg_sort_ascending;
    let headers: Vec<Span> = headers_arr
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

    // Build data rows based on view mode
    let rows: Vec<Row> = rows_data
        .iter()
        .enumerate()
        .map(|(idx, row)| {
            let is_selected = idx == state.pg_selected;
            let is_idle = is_idle_state(&row.state);

            let cells: Vec<Span> = match view_mode {
                PgActivityViewMode::Generic => vec![
                    Span::raw(format!("{:>6}", row.pid)),
                    styled_cpu(row.cpu_percent),
                    Span::raw(format!("{:>7}", format_bytes(row.rss_bytes))),
                    Span::raw(truncate(&row.db, 32)),
                    Span::raw(truncate(&row.user, 32)),
                    styled_state(&row.state),
                    styled_wait(&row.wait, is_idle),
                    styled_duration(row.query_duration_secs, &row.state),
                    Span::raw(format_duration_or_dash(row.xact_duration_secs)),
                    Span::raw(format_duration_or_dash(row.backend_duration_secs)),
                    Span::raw(truncate(&row.backend_type, 14)),
                    Span::raw(normalize_query(&row.query)),
                ],
                PgActivityViewMode::Stats => vec![
                    Span::raw(format!("{:>6}", row.pid)),
                    Span::raw(truncate(&row.db, 32)),
                    Span::raw(truncate(&row.user, 32)),
                    styled_state(&row.state),
                    styled_qdur_with_anomaly(
                        row.query_duration_secs,
                        &row.state,
                        row.pgs_mean_exec_time,
                        row.pgs_max_exec_time,
                    ),
                    styled_mean(row.pgs_mean_exec_time),
                    styled_max(row.pgs_max_exec_time),
                    styled_calls_s(row.pgs_calls_s),
                    styled_hit_pct(row.pgs_hit_pct),
                    Span::raw(normalize_query(&row.query)),
                ],
            };

            let row_style = if is_selected {
                Styles::selected()
            } else if is_idle {
                Style::default().fg(Color::DarkGray)
            } else {
                Styles::default()
            };

            Row::new(cells).style(row_style).height(1)
        })
        .collect();

    let title = {
        let idle_marker = if state.pg_hide_idle {
            " [hide idle]"
        } else {
            ""
        };
        if let Some(filter) = &state.pg_filter {
            format!(
                " PostgreSQL Activity (PGA) [{}] (filter: {}){} [{} sessions] ",
                view_indicator,
                filter,
                idle_marker,
                rows_data.len()
            )
        } else {
            format!(
                " PostgreSQL Activity (PGA) [{}]{} [{} sessions] ",
                view_indicator,
                idle_marker,
                rows_data.len()
            )
        }
    };

    // Build widths with QUERY taking remaining space
    let mut widths: Vec<ratatui::layout::Constraint> = widths_arr
        .iter()
        .map(|&w| ratatui::layout::Constraint::Length(w))
        .collect();
    widths.push(ratatui::layout::Constraint::Fill(1)); // QUERY column fills remaining space

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .style(Styles::default()),
        )
        .row_highlight_style(Styles::selected());

    // Clear the area before rendering to avoid artifacts
    frame.render_widget(Clear, area);
    frame.render_stateful_widget(table, area, &mut state.pga_ratatui_state);
}

/// Intermediate struct for row data.
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
    query_duration_secs: i64,
    xact_duration_secs: i64,
    backend_duration_secs: i64,
    /// Query ID from pg_stat_activity (PostgreSQL 14+). 0 if not available.
    query_id: i64,
    /// Stats from pg_stat_statements (linked by query_id).
    /// None if query_id is 0 or not found in pg_stat_statements.
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
    fn enrich_with_pgs_stats(
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
    fn sort_key_for_mode(&self, col: usize, mode: PgActivityViewMode) -> SortKey {
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

/// Extract ProcessInfo from snapshot.
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

/// Extract pg_stat_statements as a map keyed by queryid.
fn extract_pg_statements_map(
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

/// Format bytes to human-readable (K/M/G).
fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1}G", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1}K", bytes as f64 / 1024.0)
    } else {
        format!("{}B", bytes)
    }
}

/// Format duration in human-readable format.
fn format_duration(secs: i64) -> String {
    if secs < 0 {
        return "-".to_string();
    }
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d{}h", secs / 86400, (secs % 86400) / 3600)
    }
}

/// Format duration or "-" for zero/invalid.
fn format_duration_or_dash(secs: i64) -> String {
    if secs <= 0 {
        "-".to_string()
    } else {
        format!("{:>7}", format_duration(secs))
    }
}

/// Truncate string to max length.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        format!("{:<width$}", s, width = max_len)
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Normalize query text for single-line display.
/// Replaces newlines, carriage returns, and tabs with spaces.
fn normalize_query(s: &str) -> String {
    s.replace('\n', " ").replace('\r', "").replace('\t', " ")
}

/// Style CPU% with color coding.
fn styled_cpu(cpu: f64) -> Span<'static> {
    let text = format!("{:>5.1}", cpu);
    let style = if cpu > 80.0 {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else if cpu > 50.0 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    Span::styled(text, style)
}

/// Style state with color coding.
fn styled_state(state: &str) -> Span<'static> {
    let lower = state.to_lowercase();
    let style = if lower.contains("idle") && lower.contains("trans") {
        Style::default().fg(Color::Yellow)
    } else if lower == "active" {
        Style::default().fg(Color::Green)
    } else if lower.contains("idle") {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };
    Span::styled(truncate(state, 16), style)
}

/// Style wait event with color coding.
/// Don't highlight yellow if state is idle (Client:ClientRead is normal for idle).
fn styled_wait(wait: &str, is_idle: bool) -> Span<'static> {
    let style = if wait != "-" && !is_idle {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    Span::styled(truncate(wait, 20), style)
}

/// Returns true if state is plain "idle" (not "idle in transaction").
fn is_idle_state(state: &str) -> bool {
    let lower = state.to_lowercase();
    lower.contains("idle") && !lower.contains("trans")
}

/// Style query duration with color coding.
fn styled_duration(secs: i64, state: &str) -> Span<'static> {
    let text = format_duration_or_dash(secs);
    let lower_state = state.to_lowercase();
    let is_active = lower_state == "active" || lower_state.contains("trans");

    let style = if is_active && secs > 300 {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else if is_active && secs > 60 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    Span::styled(text, style)
}

// ========== Stats view styling functions ==========

/// Style QDUR with anomaly detection (Stats view).
/// Compares current duration with historical MEAN and MAX from pg_stat_statements.
/// - Red + Bold: QDUR > MAX (new record!)
/// - Red: QDUR > 5× MEAN
/// - Yellow: QDUR > 2× MEAN
fn styled_qdur_with_anomaly(
    secs: i64,
    state: &str,
    mean_exec_time_ms: Option<f64>,
    max_exec_time_ms: Option<f64>,
) -> Span<'static> {
    let text = format_duration_or_dash(secs);
    let lower_state = state.to_lowercase();
    let is_active = lower_state == "active" || lower_state.contains("trans");

    // Convert seconds to milliseconds for comparison
    let qdur_ms = (secs * 1000) as f64;

    let style = if !is_active {
        Style::default()
    } else if let Some(max_ms) = max_exec_time_ms {
        if max_ms > 0.0 && qdur_ms > max_ms {
            // New record! Exceeds historical max
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else if let Some(mean_ms) = mean_exec_time_ms {
            if mean_ms > 0.0 && qdur_ms > mean_ms * 5.0 {
                Style::default().fg(Color::Red)
            } else if mean_ms > 0.0 && qdur_ms > mean_ms * 2.0 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            }
        } else {
            Style::default()
        }
    } else if let Some(mean_ms) = mean_exec_time_ms {
        if mean_ms > 0.0 && qdur_ms > mean_ms * 5.0 {
            Style::default().fg(Color::Red)
        } else if mean_ms > 0.0 && qdur_ms > mean_ms * 2.0 {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        }
    } else {
        Style::default()
    };

    Span::styled(text, style)
}

/// Format and style MEAN execution time (milliseconds).
fn styled_mean(mean_ms: Option<f64>) -> Span<'static> {
    match mean_ms {
        Some(ms) => {
            let text = format_ms(ms);
            Span::raw(format!("{:>7}", text))
        }
        None => Span::raw(format!("{:>7}", "--")),
    }
}

/// Format and style MAX execution time (milliseconds).
fn styled_max(max_ms: Option<f64>) -> Span<'static> {
    match max_ms {
        Some(ms) => {
            let text = format_ms(ms);
            Span::raw(format!("{:>7}", text))
        }
        None => Span::raw(format!("{:>7}", "--")),
    }
}

/// Format and style CALL/s rate.
fn styled_calls_s(calls_s: Option<f64>) -> Span<'static> {
    match calls_s {
        Some(rate) => {
            let text = if rate >= 1000.0 {
                format!("{:.1}K", rate / 1000.0)
            } else if rate >= 1.0 {
                format!("{:.1}", rate)
            } else {
                format!("{:.2}", rate)
            };
            Span::raw(format!("{:>7}", text))
        }
        None => Span::raw(format!("{:>7}", "--")),
    }
}

/// Format and style HIT% (buffer cache hit percentage).
/// - Red: < 50%
/// - Yellow: < 80%
fn styled_hit_pct(hit_pct: Option<f64>) -> Span<'static> {
    match hit_pct {
        Some(pct) => {
            let text = format!("{:.0}%", pct);
            let style = if pct < 50.0 {
                Style::default().fg(Color::Red)
            } else if pct < 80.0 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };
            Span::styled(format!("{:>5}", text), style)
        }
        None => Span::raw(format!("{:>5}", "--")),
    }
}

/// Format milliseconds to human-readable.
fn format_ms(ms: f64) -> String {
    if ms >= 1000.0 {
        format!("{:.1}s", ms / 1000.0)
    } else if ms >= 1.0 {
        format!("{:.0}ms", ms)
    } else {
        format!("{:.1}ms", ms)
    }
}
