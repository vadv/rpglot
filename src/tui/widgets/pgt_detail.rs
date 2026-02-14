//! Detail popup widget for pg_stat_user_tables (PGT tab).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;

use crate::storage::StringInterner;
use crate::storage::model::{DataBlock, PgStatUserTablesInfo, Snapshot};
use crate::tui::state::{AppState, PopupState};

use super::detail_common::{
    format_epoch_age, kv, kv_delta_blks, kv_delta_i64, push_help, render_popup_frame, resolve_hash,
    section,
};

const HELP: &[(&str, &str)] = &[
    (
        "relid",
        "Table OID — unique identifier for the table within the database",
    ),
    ("schema", "Schema containing the table"),
    ("table", "Table name"),
    (
        "size",
        "Table relation size on disk (pg_relation_size); excludes indexes and TOAST",
    ),
    (
        "seq_scan",
        "Sequential scans initiated on this table; high count on large tables indicates missing indexes",
    ),
    (
        "seq_tup_read",
        "Rows returned by sequential scans; high values mean full-table reads",
    ),
    (
        "idx_scan",
        "Index scans initiated on this table; indicates healthy index usage",
    ),
    ("idx_tup_fetch", "Rows fetched by index scans"),
    (
        "seq%",
        "Percentage of scans that are sequential: seq_scan / (seq_scan + idx_scan) * 100; >50% on large tables suggests missing indexes",
    ),
    ("n_tup_ins", "Total rows inserted since last stats reset"),
    ("n_tup_upd", "Total rows updated"),
    ("n_tup_del", "Total rows deleted"),
    (
        "n_tup_hot_upd",
        "HOT updates — update that did not require index update; high ratio = good fillfactor tuning",
    ),
    (
        "hot%",
        "HOT update ratio: n_tup_hot_upd / n_tup_upd * 100; higher is better, reduces index bloat",
    ),
    (
        "n_live_tup",
        "Estimated number of live rows (from ANALYZE or autovacuum)",
    ),
    (
        "n_dead_tup",
        "Estimated number of dead rows waiting for VACUUM; high values indicate bloat",
    ),
    (
        "dead%",
        "Dead tuple percentage: n_dead_tup / (n_live_tup + n_dead_tup) * 100; >5% needs attention, >20% is critical",
    ),
    ("vacuum_count", "Number of manual VACUUM operations"),
    (
        "autovacuum_count",
        "Number of autovacuum operations; if 0, table may be excluded from autovacuum",
    ),
    ("analyze_count", "Number of manual ANALYZE operations"),
    ("autoanalyze_count", "Number of autoanalyze operations"),
    ("last_vacuum", "Time of last manual VACUUM"),
    (
        "last_autovacuum",
        "Time of last autovacuum; '-' = never autovacuumed",
    ),
    ("last_analyze", "Time of last manual ANALYZE"),
    (
        "last_autoanalyze",
        "Time of last autoanalyze; '-' = never autoanalyzed, planner stats may be stale",
    ),
    // I/O Statistics (pg_statio_user_tables) — displayed as bytes (blocks × 8 KB)
    (
        "heap_blks_read",
        "Heap data read from disk; displayed as bytes (1 block = 8 KB); in table columns shown as bytes/s rate",
    ),
    (
        "heap_blks_hit",
        "Heap data served from shared_buffers cache; displayed as bytes (1 block = 8 KB); in table columns shown as bytes/s rate",
    ),
    (
        "idx_blks_read",
        "Index data read from disk for all indexes on this table; displayed as bytes (1 block = 8 KB); in table columns shown as bytes/s rate",
    ),
    (
        "idx_blks_hit",
        "Index data served from shared_buffers cache; displayed as bytes (1 block = 8 KB); in table columns shown as bytes/s rate",
    ),
    (
        "toast_blks_read",
        "TOAST data read from disk (large values stored out-of-line); displayed as bytes (1 block = 8 KB)",
    ),
    (
        "toast_blks_hit",
        "TOAST data served from shared_buffers cache; displayed as bytes (1 block = 8 KB)",
    ),
    (
        "tidx_blks_read",
        "TOAST index data read from disk; displayed as bytes (1 block = 8 KB)",
    ),
    (
        "tidx_blks_hit",
        "TOAST index data served from shared_buffers cache; displayed as bytes (1 block = 8 KB)",
    ),
    (
        "io_hit%",
        "Buffer cache hit ratio: total_hits / (total_hits + total_reads) * 100; <90% means significant disk I/O, <70% is critical",
    ),
];

pub fn render_pgt_detail(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    interner: Option<&StringInterner>,
) {
    let (relid, show_help) = match &state.popup {
        PopupState::PgtDetail {
            relid, show_help, ..
        } => (*relid, *show_help),
        _ => return,
    };

    let snapshot = match &state.current_snapshot {
        Some(s) => s,
        None => return,
    };

    let Some(table) = find_table(snapshot, relid) else {
        let content = vec![Line::raw("Table not found in current snapshot")];
        let scroll = match &mut state.popup {
            PopupState::PgtDetail { scroll, .. } => scroll,
            _ => return,
        };
        render_popup_frame(
            frame,
            area,
            "pg_stat_user_tables detail",
            content,
            scroll,
            false,
        );
        return;
    };

    let prev = state.pgt.delta_base.get(&relid);
    let content = build_content(table, prev, interner, show_help);

    let scroll = match &mut state.popup {
        PopupState::PgtDetail { scroll, .. } => scroll,
        _ => return,
    };

    render_popup_frame(
        frame,
        area,
        "pg_stat_user_tables detail",
        content,
        scroll,
        show_help,
    );
}

fn find_table(snapshot: &Snapshot, relid: u32) -> Option<&PgStatUserTablesInfo> {
    snapshot.blocks.iter().find_map(|b| {
        if let DataBlock::PgStatUserTables(v) = b {
            v.iter().find(|t| t.relid == relid)
        } else {
            None
        }
    })
}

fn build_content(
    t: &PgStatUserTablesInfo,
    prev: Option<&PgStatUserTablesInfo>,
    interner: Option<&StringInterner>,
    show_help: bool,
) -> Vec<Line<'static>> {
    let schema = resolve_hash(interner, t.schemaname_hash);
    let table = resolve_hash(interner, t.relname_hash);

    let total_scans = t.seq_scan + t.idx_scan;
    let seq_pct = if total_scans > 0 {
        format!("{:.1}%", (t.seq_scan as f64 / total_scans as f64) * 100.0)
    } else {
        "-".to_string()
    };

    let total_tup = t.n_live_tup + t.n_dead_tup;
    let dead_pct = if total_tup > 0 {
        format!("{:.1}%", (t.n_dead_tup as f64 / total_tup as f64) * 100.0)
    } else {
        "0.0%".to_string()
    };

    let hot_pct = if t.n_tup_upd > 0 {
        format!(
            "{:.1}%",
            (t.n_tup_hot_upd as f64 / t.n_tup_upd as f64) * 100.0
        )
    } else {
        "-".to_string()
    };

    let mut lines = Vec::new();

    // Identity
    lines.push(section("Identity"));
    lines.push(kv("relid", &t.relid.to_string()));
    push_help(&mut lines, show_help, HELP, "relid");
    lines.push(kv("schema", &schema));
    push_help(&mut lines, show_help, HELP, "schema");
    lines.push(kv("table", &table));
    push_help(&mut lines, show_help, HELP, "table");
    lines.push(kv("size", &format_size_detail(t.size_bytes)));
    push_help(&mut lines, show_help, HELP, "size");
    lines.push(Line::raw(""));

    // Scan Activity
    lines.push(section("Scan Activity"));
    lines.push(kv_delta_i64(
        "seq_scan",
        t.seq_scan,
        prev.map(|p| p.seq_scan),
    ));
    push_help(&mut lines, show_help, HELP, "seq_scan");
    lines.push(kv_delta_i64(
        "seq_tup_read",
        t.seq_tup_read,
        prev.map(|p| p.seq_tup_read),
    ));
    push_help(&mut lines, show_help, HELP, "seq_tup_read");
    lines.push(kv_delta_i64(
        "idx_scan",
        t.idx_scan,
        prev.map(|p| p.idx_scan),
    ));
    push_help(&mut lines, show_help, HELP, "idx_scan");
    lines.push(kv_delta_i64(
        "idx_tup_fetch",
        t.idx_tup_fetch,
        prev.map(|p| p.idx_tup_fetch),
    ));
    push_help(&mut lines, show_help, HELP, "idx_tup_fetch");
    lines.push(kv("seq%", &seq_pct));
    push_help(&mut lines, show_help, HELP, "seq%");
    lines.push(Line::raw(""));

    // Write Activity
    lines.push(section("Write Activity"));
    lines.push(kv_delta_i64(
        "n_tup_ins",
        t.n_tup_ins,
        prev.map(|p| p.n_tup_ins),
    ));
    push_help(&mut lines, show_help, HELP, "n_tup_ins");
    lines.push(kv_delta_i64(
        "n_tup_upd",
        t.n_tup_upd,
        prev.map(|p| p.n_tup_upd),
    ));
    push_help(&mut lines, show_help, HELP, "n_tup_upd");
    lines.push(kv_delta_i64(
        "n_tup_del",
        t.n_tup_del,
        prev.map(|p| p.n_tup_del),
    ));
    push_help(&mut lines, show_help, HELP, "n_tup_del");
    lines.push(kv_delta_i64(
        "n_tup_hot_upd",
        t.n_tup_hot_upd,
        prev.map(|p| p.n_tup_hot_upd),
    ));
    push_help(&mut lines, show_help, HELP, "n_tup_hot_upd");
    lines.push(kv("hot%", &hot_pct));
    push_help(&mut lines, show_help, HELP, "hot%");
    lines.push(Line::raw(""));

    // Tuple Estimates
    lines.push(section("Tuple Estimates"));
    lines.push(kv("n_live_tup", &t.n_live_tup.to_string()));
    push_help(&mut lines, show_help, HELP, "n_live_tup");
    lines.push(kv("n_dead_tup", &t.n_dead_tup.to_string()));
    push_help(&mut lines, show_help, HELP, "n_dead_tup");
    lines.push(kv("dead%", &dead_pct));
    push_help(&mut lines, show_help, HELP, "dead%");
    lines.push(Line::raw(""));

    // Maintenance
    lines.push(section("Maintenance"));
    lines.push(kv_delta_i64(
        "vacuum_count",
        t.vacuum_count,
        prev.map(|p| p.vacuum_count),
    ));
    push_help(&mut lines, show_help, HELP, "vacuum_count");
    lines.push(kv_delta_i64(
        "autovacuum_count",
        t.autovacuum_count,
        prev.map(|p| p.autovacuum_count),
    ));
    push_help(&mut lines, show_help, HELP, "autovacuum_count");
    lines.push(kv_delta_i64(
        "analyze_count",
        t.analyze_count,
        prev.map(|p| p.analyze_count),
    ));
    push_help(&mut lines, show_help, HELP, "analyze_count");
    lines.push(kv_delta_i64(
        "autoanalyze_count",
        t.autoanalyze_count,
        prev.map(|p| p.autoanalyze_count),
    ));
    push_help(&mut lines, show_help, HELP, "autoanalyze_count");
    lines.push(kv("last_vacuum", &format_epoch_age(t.last_vacuum)));
    push_help(&mut lines, show_help, HELP, "last_vacuum");
    lines.push(kv("last_autovacuum", &format_epoch_age(t.last_autovacuum)));
    push_help(&mut lines, show_help, HELP, "last_autovacuum");
    lines.push(kv("last_analyze", &format_epoch_age(t.last_analyze)));
    push_help(&mut lines, show_help, HELP, "last_analyze");
    lines.push(kv(
        "last_autoanalyze",
        &format_epoch_age(t.last_autoanalyze),
    ));
    push_help(&mut lines, show_help, HELP, "last_autoanalyze");
    lines.push(Line::raw(""));

    // I/O Statistics — blocks displayed as bytes (× 8 KB)
    lines.push(section("I/O Statistics (pg_statio_user_tables)"));
    lines.push(kv_delta_blks(
        "heap_blks_read",
        t.heap_blks_read,
        prev.map(|p| p.heap_blks_read),
    ));
    push_help(&mut lines, show_help, HELP, "heap_blks_read");
    lines.push(kv_delta_blks(
        "heap_blks_hit",
        t.heap_blks_hit,
        prev.map(|p| p.heap_blks_hit),
    ));
    push_help(&mut lines, show_help, HELP, "heap_blks_hit");
    lines.push(kv_delta_blks(
        "idx_blks_read",
        t.idx_blks_read,
        prev.map(|p| p.idx_blks_read),
    ));
    push_help(&mut lines, show_help, HELP, "idx_blks_read");
    lines.push(kv_delta_blks(
        "idx_blks_hit",
        t.idx_blks_hit,
        prev.map(|p| p.idx_blks_hit),
    ));
    push_help(&mut lines, show_help, HELP, "idx_blks_hit");
    lines.push(kv_delta_blks(
        "toast_blks_read",
        t.toast_blks_read,
        prev.map(|p| p.toast_blks_read),
    ));
    push_help(&mut lines, show_help, HELP, "toast_blks_read");
    lines.push(kv_delta_blks(
        "toast_blks_hit",
        t.toast_blks_hit,
        prev.map(|p| p.toast_blks_hit),
    ));
    push_help(&mut lines, show_help, HELP, "toast_blks_hit");
    lines.push(kv_delta_blks(
        "tidx_blks_read",
        t.tidx_blks_read,
        prev.map(|p| p.tidx_blks_read),
    ));
    push_help(&mut lines, show_help, HELP, "tidx_blks_read");
    lines.push(kv_delta_blks(
        "tidx_blks_hit",
        t.tidx_blks_hit,
        prev.map(|p| p.tidx_blks_hit),
    ));
    push_help(&mut lines, show_help, HELP, "tidx_blks_hit");

    let total_reads = t.heap_blks_read + t.idx_blks_read + t.toast_blks_read + t.tidx_blks_read;
    let total_hits = t.heap_blks_hit + t.idx_blks_hit + t.toast_blks_hit + t.tidx_blks_hit;
    let total_io = total_reads + total_hits;
    let io_hit_pct = if total_io > 0 {
        format!("{:.1}%", (total_hits as f64 / total_io as f64) * 100.0)
    } else {
        "-".to_string()
    };
    lines.push(kv("io_hit%", &io_hit_pct));
    push_help(&mut lines, show_help, HELP, "io_hit%");

    lines
}

fn format_size_detail(bytes: i64) -> String {
    if bytes <= 0 {
        return "-".to_string();
    }
    let b = bytes as f64;
    if b >= 1_099_511_627_776.0 {
        format!("{:.2} TB ({} bytes)", b / 1_099_511_627_776.0, bytes)
    } else if b >= 1_073_741_824.0 {
        format!("{:.2} GB ({} bytes)", b / 1_073_741_824.0, bytes)
    } else if b >= 1_048_576.0 {
        format!("{:.2} MB ({} bytes)", b / 1_048_576.0, bytes)
    } else if b >= 1024.0 {
        format!("{:.1} KB ({} bytes)", b / 1024.0, bytes)
    } else {
        format!("{} bytes", bytes)
    }
}
