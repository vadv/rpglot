//! PGE (pg_log_errors) view model.

use crate::fmt::normalize_query;
use crate::storage::StringInterner;
use crate::storage::model::PgLogSeverity;
use crate::tui::state::{AccumulatedError, PgErrorsTabState};
use crate::view::common::{RowStyleClass, TableViewModel, ViewCell, ViewRow};

const HEADERS: &[&str] = &["SEVERITY", "COUNT", "PATTERN", "SAMPLE"];
const WIDTHS: &[u16] = &[8, 8];

/// Builds a UI-agnostic view model for the PGE (errors) tab.
///
/// Returns `None` if there are no accumulated errors.
pub fn build_errors_view(
    accumulated: &[AccumulatedError],
    state: &PgErrorsTabState,
    interner: Option<&StringInterner>,
) -> Option<TableViewModel<u64>> {
    if accumulated.is_empty() {
        return None;
    }

    let resolve = |hash: u64| -> String {
        if hash == 0 {
            return String::new();
        }
        interner
            .and_then(|i| i.resolve(hash))
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("#{:x}", hash))
    };

    struct ErrorRow {
        pattern_hash: u64,
        severity: PgLogSeverity,
        severity_str: String,
        count: u32,
        pattern: String,
        sample: String,
    }

    let mut rows_data: Vec<ErrorRow> = accumulated
        .iter()
        .map(|a| {
            let severity_str = match a.severity {
                PgLogSeverity::Error => "ERROR",
                PgLogSeverity::Fatal => "FATAL",
                PgLogSeverity::Panic => "PANIC",
            }
            .to_string();
            let pattern = normalize_query(&resolve(a.pattern_hash));
            let sample = normalize_query(&resolve(a.sample_hash));
            ErrorRow {
                pattern_hash: a.pattern_hash,
                severity: a.severity,
                severity_str,
                count: a.count,
                pattern,
                sample,
            }
        })
        .collect();

    // Apply filter
    if let Some(ref filter) = state.filter {
        let f = filter.to_lowercase();
        rows_data.retain(|r| {
            r.pattern.to_lowercase().contains(&f)
                || r.sample.to_lowercase().contains(&f)
                || r.severity_str.to_lowercase().contains(&f)
        });
    }

    if rows_data.is_empty() {
        return None;
    }

    // Sort
    let col = state.sort_column;
    let asc = state.sort_ascending;
    rows_data.sort_by(|a, b| {
        let cmp = match col {
            0 => (a.severity as u8).cmp(&(b.severity as u8)),
            1 => a.count.cmp(&b.count),
            2 => a.pattern.cmp(&b.pattern),
            3 => a.sample.cmp(&b.sample),
            _ => std::cmp::Ordering::Equal,
        };
        if asc { cmp } else { cmp.reverse() }
    });

    // Build view rows
    let rows: Vec<ViewRow<u64>> = rows_data
        .iter()
        .map(|r| {
            let style = match r.severity {
                PgLogSeverity::Panic => RowStyleClass::CriticalBold,
                PgLogSeverity::Fatal => RowStyleClass::Critical,
                PgLogSeverity::Error => RowStyleClass::Warning,
            };

            ViewRow {
                id: r.pattern_hash,
                cells: vec![
                    ViewCell::plain(r.severity_str.clone()),
                    ViewCell::plain(r.count.to_string()),
                    ViewCell::plain(r.pattern.clone()),
                    ViewCell::plain(r.sample.clone()),
                ],
                style,
            }
        })
        .collect();

    let filter_info = state
        .filter
        .as_ref()
        .map(|f| format!(" [filter: {}]", f))
        .unwrap_or_default();

    let sort_indicator = match col {
        0 => "severity",
        1 => "count",
        2 => "pattern",
        3 => "sample",
        _ => "",
    };
    let sort_dir = if asc { "asc" } else { "desc" };

    let title = format!(
        "PGE: Errors ({} patterns, sort: {} {}){filter_info}",
        rows.len(),
        sort_indicator,
        sort_dir,
    );

    Some(TableViewModel {
        title,
        headers: HEADERS.iter().map(|s| s.to_string()).collect(),
        widths: WIDTHS.to_vec(),
        rows,
        sort_column: col,
        sort_ascending: asc,
    })
}
