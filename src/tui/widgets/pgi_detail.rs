//! Detail popup widget for pg_stat_user_indexes (PGI tab).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;

use crate::storage::StringInterner;
use crate::storage::model::{DataBlock, PgStatUserIndexesInfo, Snapshot};
use crate::tui::state::{AppState, PopupState};

use super::detail_common::{
    format_bytes, kv, kv_delta_blks, kv_delta_i64, push_help, render_popup_frame, resolve_hash,
    section,
};

const HELP: &[(&str, &str)] = &[
    ("indexrelid", "Index OID — unique identifier for the index"),
    ("relid", "Parent table OID"),
    ("schema", "Schema containing the index"),
    ("table", "Parent table name"),
    ("index", "Index name"),
    (
        "idx_scan",
        "Number of index scans initiated; 0 = unused index, candidate for removal",
    ),
    (
        "idx_tup_read",
        "Index entries returned by scans; much larger than idx_tup_fetch suggests dead tuples in index",
    ),
    (
        "idx_tup_fetch",
        "Live table rows fetched via index; this is the actual useful work done by the index",
    ),
    (
        "size",
        "Index size on disk (pg_relation_size); unused large indexes waste disk and slow down writes",
    ),
    // I/O Statistics (pg_statio_user_indexes) — displayed as bytes (blocks × 8 KB)
    (
        "idx_blks_read",
        "Index data read from disk; displayed as bytes (1 block = 8 KB); in table columns shown as bytes/s rate; high values = poor cache efficiency",
    ),
    (
        "idx_blks_hit",
        "Index data served from shared_buffers cache; displayed as bytes (1 block = 8 KB); in table columns shown as bytes/s rate",
    ),
    (
        "io_hit%",
        "Buffer cache hit ratio: idx_blks_hit / (idx_blks_read + idx_blks_hit) * 100; <90% = significant disk I/O, <70% is critical",
    ),
];

pub fn render_pgi_detail(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    interner: Option<&StringInterner>,
) {
    let (indexrelid, show_help) = match &state.popup {
        PopupState::PgiDetail {
            indexrelid,
            show_help,
            ..
        } => (*indexrelid, *show_help),
        _ => return,
    };

    let snapshot = match &state.current_snapshot {
        Some(s) => s,
        None => return,
    };

    let Some(idx) = find_index(snapshot, indexrelid) else {
        let content = vec![Line::raw("Index not found in current snapshot")];
        let scroll = match &mut state.popup {
            PopupState::PgiDetail { scroll, .. } => scroll,
            _ => return,
        };
        render_popup_frame(
            frame,
            area,
            "pg_stat_user_indexes detail",
            content,
            scroll,
            false,
        );
        return;
    };

    let prev = state.pgi.delta_base.get(&indexrelid);
    let content = build_content(idx, prev, interner, show_help);

    let scroll = match &mut state.popup {
        PopupState::PgiDetail { scroll, .. } => scroll,
        _ => return,
    };

    render_popup_frame(
        frame,
        area,
        "pg_stat_user_indexes detail",
        content,
        scroll,
        show_help,
    );
}

fn find_index(snapshot: &Snapshot, indexrelid: u32) -> Option<&PgStatUserIndexesInfo> {
    snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::PgStatUserIndexes(v) = b {
            v.iter().find(|i| i.indexrelid == indexrelid)
        } else {
            None
        }
    })
}

fn build_content(
    idx: &PgStatUserIndexesInfo,
    prev: Option<&PgStatUserIndexesInfo>,
    interner: Option<&StringInterner>,
    show_help: bool,
) -> Vec<Line<'static>> {
    let schema = resolve_hash(interner, idx.schemaname_hash);
    let table = resolve_hash(interner, idx.relname_hash);
    let index = resolve_hash(interner, idx.indexrelname_hash);

    let mut lines = Vec::new();

    // Identity
    lines.push(section("Identity"));
    lines.push(kv("indexrelid", &idx.indexrelid.to_string()));
    push_help(&mut lines, show_help, HELP, "indexrelid");
    lines.push(kv("relid", &idx.relid.to_string()));
    push_help(&mut lines, show_help, HELP, "relid");
    lines.push(kv("schema", &schema));
    push_help(&mut lines, show_help, HELP, "schema");
    lines.push(kv("table", &table));
    push_help(&mut lines, show_help, HELP, "table");
    lines.push(kv("index", &index));
    push_help(&mut lines, show_help, HELP, "index");
    lines.push(Line::raw(""));

    // Usage
    lines.push(section("Usage"));
    lines.push(kv_delta_i64(
        "idx_scan",
        idx.idx_scan,
        prev.map(|p| p.idx_scan),
    ));
    push_help(&mut lines, show_help, HELP, "idx_scan");
    lines.push(kv_delta_i64(
        "idx_tup_read",
        idx.idx_tup_read,
        prev.map(|p| p.idx_tup_read),
    ));
    push_help(&mut lines, show_help, HELP, "idx_tup_read");
    lines.push(kv_delta_i64(
        "idx_tup_fetch",
        idx.idx_tup_fetch,
        prev.map(|p| p.idx_tup_fetch),
    ));
    push_help(&mut lines, show_help, HELP, "idx_tup_fetch");
    lines.push(Line::raw(""));

    // Size
    lines.push(section("Size"));
    lines.push(kv("size", &format_bytes(idx.size_bytes as u64)));
    push_help(&mut lines, show_help, HELP, "size");
    lines.push(Line::raw(""));

    // I/O Statistics — blocks displayed as bytes (× 8 KB)
    lines.push(section("I/O Statistics (pg_statio_user_indexes)"));
    lines.push(kv_delta_blks(
        "idx_blks_read",
        idx.idx_blks_read,
        prev.map(|p| p.idx_blks_read),
    ));
    push_help(&mut lines, show_help, HELP, "idx_blks_read");
    lines.push(kv_delta_blks(
        "idx_blks_hit",
        idx.idx_blks_hit,
        prev.map(|p| p.idx_blks_hit),
    ));
    push_help(&mut lines, show_help, HELP, "idx_blks_hit");

    let total_io = idx.idx_blks_read + idx.idx_blks_hit;
    let io_hit_pct = if total_io > 0 {
        format!(
            "{:.1}%",
            (idx.idx_blks_hit as f64 / total_io as f64) * 100.0
        )
    } else {
        "-".to_string()
    };
    lines.push(kv("io_hit%", &io_hit_pct));
    push_help(&mut lines, show_help, HELP, "io_hit%");

    lines
}
