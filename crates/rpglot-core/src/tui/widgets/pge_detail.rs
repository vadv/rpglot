//! PostgreSQL log error detail popup widget.
//! Shows detailed information about a selected error pattern.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;

use crate::storage::StringInterner;
use crate::storage::model::PgLogSeverity;
use crate::tui::state::{AppState, PopupState};

use super::detail_common::{kv, push_help, render_popup_frame, section};

/// Help table for PGE detail fields.
const HELP: &[(&str, &str)] = &[
    (
        "Pattern Hash",
        "Unique identifier (xxh3 hash) of the normalized error pattern",
    ),
    (
        "Severity",
        "PostgreSQL log severity level: ERROR, FATAL, or PANIC",
    ),
    (
        "Count",
        "Number of occurrences of this error pattern in the current hour",
    ),
    (
        "Last Seen",
        "Timestamp of the last occurrence of this error",
    ),
    (
        "Pattern",
        "Normalized error message with variable parts replaced by $N placeholders",
    ),
    (
        "Sample",
        "One concrete example of the error message with actual values",
    ),
];

pub fn render_pge_detail(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    interner: Option<&StringInterner>,
) {
    let (pattern_hash, mut scroll, show_help) = match &state.popup {
        PopupState::PgeDetail {
            pattern_hash,
            scroll,
            show_help,
        } => (*pattern_hash, *scroll, *show_help),
        _ => return,
    };

    let acc = state
        .pge
        .accumulated
        .iter()
        .find(|a| a.pattern_hash == pattern_hash);

    let Some(acc) = acc else {
        return;
    };

    let resolve = |hash: u64| -> String {
        if hash == 0 {
            return String::new();
        }
        interner
            .and_then(|i| i.resolve(hash))
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("#{:x}", hash))
    };

    let severity_str = match acc.severity {
        PgLogSeverity::Error => "ERROR",
        PgLogSeverity::Fatal => "FATAL",
        PgLogSeverity::Panic => "PANIC",
    };

    let mut lines: Vec<Line> = Vec::new();

    // Error Info section
    lines.push(section("Error Info"));
    lines.push(kv("Pattern Hash", &format!("{:x}", acc.pattern_hash)));
    push_help(&mut lines, show_help, HELP, "Pattern Hash");
    lines.push(kv("Severity", severity_str));
    push_help(&mut lines, show_help, HELP, "Severity");
    lines.push(kv("Count", &acc.count.to_string()));
    push_help(&mut lines, show_help, HELP, "Count");

    let last_seen = {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let ago = now - acc.last_seen;
        if ago < 60 {
            format!("{}s ago", ago)
        } else if ago < 3600 {
            format!("{}m {}s ago", ago / 60, ago % 60)
        } else {
            format!("{}h {}m ago", ago / 3600, (ago % 3600) / 60)
        }
    };
    lines.push(kv("Last Seen", &last_seen));
    push_help(&mut lines, show_help, HELP, "Last Seen");

    // Pattern section
    lines.push(section("Pattern"));
    let pattern_text = resolve(acc.pattern_hash);
    for line in pattern_text.lines() {
        let sanitized = line.replace('\t', "    ");
        lines.push(Line::raw(format!("  {}", sanitized)));
    }
    push_help(&mut lines, show_help, HELP, "Pattern");

    // Sample section
    lines.push(section("Sample"));
    let sample_text = resolve(acc.sample_hash);
    for line in sample_text.lines() {
        let sanitized = line.replace('\t', "    ");
        lines.push(Line::raw(format!("  {}", sanitized)));
    }
    push_help(&mut lines, show_help, HELP, "Sample");

    render_popup_frame(
        frame,
        area,
        "PG Error detail",
        lines,
        &mut scroll,
        show_help,
    );

    // Write back scroll
    if let PopupState::PgeDetail {
        scroll: ref mut s, ..
    } = state.popup
    {
        *s = scroll;
    }
}
