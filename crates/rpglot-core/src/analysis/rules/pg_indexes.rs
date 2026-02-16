use crate::analysis::rules::AnalysisRule;
use crate::analysis::{AnalysisContext, Anomaly, Category, Severity, find_block};
use crate::storage::interner::StringInterner;
use crate::storage::model::{DataBlock, PgStatUserIndexesInfo};

/// Format index name as "schema.index (on table)" or just "index (on table)".
fn qualified_index_name(
    interner: &StringInterner,
    schema_hash: u64,
    index_hash: u64,
    table_hash: u64,
) -> String {
    let idx = interner.resolve(index_hash).unwrap_or("unknown");
    let tbl = interner.resolve(table_hash).unwrap_or("?");
    match interner.resolve(schema_hash) {
        Some(s) if s != "public" => format!("{s}.{idx} on {s}.{tbl}"),
        _ => format!("{idx} on {tbl}"),
    }
}

fn find_prev_index(
    prev: &[PgStatUserIndexesInfo],
    indexrelid: u32,
) -> Option<&PgStatUserIndexesInfo> {
    prev.iter().find(|i| i.indexrelid == indexrelid)
}

// ============================================================
// IndexReadSpikeRule — index reading heavily from disk
// ============================================================

pub struct IndexReadSpikeRule;

impl AnalysisRule for IndexReadSpikeRule {
    fn id(&self) -> &'static str {
        "index_read_spike"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let prev_snapshot = match ctx.prev_snapshot {
            Some(s) => s,
            None => return Vec::new(),
        };
        if ctx.dt <= 0.0 {
            return Vec::new();
        }

        let Some(indexes) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatUserIndexes(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let Some(prev_indexes) = find_block(prev_snapshot, |b| match b {
            DataBlock::PgStatUserIndexes(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let mut worst_rate = 0.0_f64;
        let mut worst_schema_hash: u64 = 0;
        let mut worst_index_hash: u64 = 0;
        let mut worst_table_hash: u64 = 0;
        let mut worst_delta: i64 = 0;
        let mut worst_dt: f64 = 0.0;

        for idx in indexes {
            let Some(prev) = find_prev_index(prev_indexes, idx.indexrelid) else {
                continue;
            };
            if idx.collected_at == prev.collected_at {
                continue;
            }
            let dt = (idx.collected_at - prev.collected_at) as f64;
            if dt <= 0.0 {
                continue;
            }
            let delta = (idx.idx_blks_read - prev.idx_blks_read).max(0);
            if delta < 100 {
                continue; // noise filter
            }
            let rate = delta as f64 / dt;
            if rate > worst_rate {
                worst_rate = rate;
                worst_schema_hash = idx.schemaname_hash;
                worst_index_hash = idx.indexrelname_hash;
                worst_table_hash = idx.relname_hash;
                worst_delta = delta;
                worst_dt = dt;
            }
        }

        if worst_rate < 50.0 {
            return Vec::new();
        }

        let name = qualified_index_name(
            ctx.interner,
            worst_schema_hash,
            worst_index_hash,
            worst_table_hash,
        );
        let mb_per_s = worst_rate * 8.0 / 1024.0;

        let severity = if worst_rate >= 500.0 {
            Severity::Critical
        } else {
            Severity::Warning
        };

        let detail = format!(
            "Δblks: {worst_delta}, dt: {worst_dt:.0}s, rate: {worst_delta}/{worst_dt:.0} = {worst_rate:.1} blk/s"
        );

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "index_read_spike",
            category: Category::PgTables,
            severity,
            title: format!("Index {name}: {worst_rate:.0} blk/s disk reads ({mb_per_s:.1} MiB/s)"),
            detail: Some(detail),
            value: worst_rate,
        }]
    }
}

// ============================================================
// IndexCacheHitDropRule — index cache hit ratio drops
// ============================================================

pub struct IndexCacheHitDropRule;

impl AnalysisRule for IndexCacheHitDropRule {
    fn id(&self) -> &'static str {
        "index_cache_miss"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let prev_snapshot = match ctx.prev_snapshot {
            Some(s) => s,
            None => return Vec::new(),
        };
        if ctx.dt <= 0.0 {
            return Vec::new();
        }

        let Some(indexes) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatUserIndexes(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let Some(prev_indexes) = find_block(prev_snapshot, |b| match b {
            DataBlock::PgStatUserIndexes(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let mut worst_ratio = 100.0_f64;
        let mut worst_schema_hash: u64 = 0;
        let mut worst_index_hash: u64 = 0;
        let mut worst_table_hash: u64 = 0;
        let mut worst_read_d: i64 = 0;
        let mut worst_hit_d: i64 = 0;

        for idx in indexes {
            let Some(prev) = find_prev_index(prev_indexes, idx.indexrelid) else {
                continue;
            };
            if idx.collected_at == prev.collected_at {
                continue;
            }
            let read_d = (idx.idx_blks_read - prev.idx_blks_read).max(0);
            let hit_d = (idx.idx_blks_hit - prev.idx_blks_hit).max(0);
            let total = read_d + hit_d;
            if total < 100 {
                continue;
            }
            let hit_ratio = hit_d as f64 * 100.0 / total as f64;
            if hit_ratio < worst_ratio {
                worst_ratio = hit_ratio;
                worst_schema_hash = idx.schemaname_hash;
                worst_index_hash = idx.indexrelname_hash;
                worst_table_hash = idx.relname_hash;
                worst_read_d = read_d;
                worst_hit_d = hit_d;
            }
        }

        if worst_ratio >= 90.0 {
            return Vec::new();
        }

        let name = qualified_index_name(
            ctx.interner,
            worst_schema_hash,
            worst_index_hash,
            worst_table_hash,
        );

        let severity = if worst_ratio < 50.0 {
            Severity::Critical
        } else {
            Severity::Warning
        };

        let detail =
            format!("Δhit: {worst_hit_d} blks, Δread: {worst_read_d} blks (delta, not cumulative)");

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "index_cache_miss",
            category: Category::PgTables,
            severity,
            title: format!("Index {name}: cache hit ratio {worst_ratio:.0}%"),
            detail: Some(detail),
            value: 100.0 - worst_ratio,
        }]
    }
}
