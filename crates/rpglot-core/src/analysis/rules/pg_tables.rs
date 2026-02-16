use crate::analysis::rules::AnalysisRule;
use crate::analysis::{AnalysisContext, Anomaly, Category, Severity, find_block};
use crate::storage::interner::StringInterner;
use crate::storage::model::{DataBlock, PgStatUserTablesInfo};

/// Format table name as "schema.table" (or just "table" if schema unresolved).
fn qualified_name(interner: &StringInterner, schema_hash: u64, rel_hash: u64) -> String {
    let rel = interner.resolve(rel_hash).unwrap_or("unknown");
    match interner.resolve(schema_hash) {
        Some(s) if s != "public" => format!("{s}.{rel}"),
        _ => rel.to_string(),
    }
}

// ============================================================
// DeadTuplesHighRule
// ============================================================

pub struct DeadTuplesHighRule;

impl AnalysisRule for DeadTuplesHighRule {
    fn id(&self) -> &'static str {
        "dead_tuples_high"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some(tables) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatUserTables(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let mut worst_pct: f64 = 0.0;
        let mut worst_schema_hash: u64 = 0;
        let mut worst_name_hash: u64 = 0;

        for t in tables {
            let total = t.n_live_tup + t.n_dead_tup;
            if total <= 1000 {
                continue;
            }
            let dead_pct = t.n_dead_tup as f64 * 100.0 / total as f64;
            if dead_pct > worst_pct {
                worst_pct = dead_pct;
                worst_schema_hash = t.schemaname_hash;
                worst_name_hash = t.relname_hash;
            }
        }

        if worst_pct < 20.0 {
            return Vec::new();
        }

        let severity = if worst_pct >= 50.0 {
            Severity::Critical
        } else {
            Severity::Warning
        };

        let name = qualified_name(ctx.interner, worst_schema_hash, worst_name_hash);

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "dead_tuples_high",
            category: Category::PgTables,
            severity,
            title: format!("Table {name} has {worst_pct:.0}% dead tuples"),
            detail: None,
            value: worst_pct,
        }]
    }
}

// ============================================================
// SeqScanDominantRule
// ============================================================

pub struct SeqScanDominantRule;

impl AnalysisRule for SeqScanDominantRule {
    fn id(&self) -> &'static str {
        "seq_scan_dominant"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some(tables) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatUserTables(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let mut worst_pct: f64 = 0.0;
        let mut worst_schema_hash: u64 = 0;
        let mut worst_name_hash: u64 = 0;

        for t in tables {
            // Small tables: seq scan is optimal, planner picks it intentionally
            if t.n_live_tup < 10_000 {
                continue;
            }
            let total = t.seq_scan + t.idx_scan;
            if total <= 100 {
                continue;
            }
            let seq_pct = t.seq_scan as f64 * 100.0 / total as f64;
            if seq_pct > worst_pct {
                worst_pct = seq_pct;
                worst_schema_hash = t.schemaname_hash;
                worst_name_hash = t.relname_hash;
            }
        }

        if worst_pct <= 80.0 {
            return Vec::new();
        }

        let name = qualified_name(ctx.interner, worst_schema_hash, worst_name_hash);

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "seq_scan_dominant",
            category: Category::PgTables,
            severity: Severity::Warning,
            title: format!("Table {name}: {worst_pct:.0}% sequential scans"),
            detail: None,
            value: worst_pct,
        }]
    }
}

// ============================================================
// Helper: find previous table by relid
// ============================================================

fn find_prev_table(prev: &[PgStatUserTablesInfo], relid: u32) -> Option<&PgStatUserTablesInfo> {
    prev.iter().find(|t| t.relid == relid)
}

// ============================================================
// HeapReadSpikeRule — table reading heavily from disk
// ============================================================

pub struct HeapReadSpikeRule;

impl AnalysisRule for HeapReadSpikeRule {
    fn id(&self) -> &'static str {
        "heap_read_spike"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let prev_snapshot = match ctx.prev_snapshot {
            Some(s) => s,
            None => return Vec::new(),
        };
        if ctx.dt <= 0.0 {
            return Vec::new();
        }

        let Some(tables) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatUserTables(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let Some(prev_tables) = find_block(prev_snapshot, |b| match b {
            DataBlock::PgStatUserTables(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        // Find table with highest heap_blks_read rate (blocks/s from disk).
        let mut worst_rate = 0.0_f64;
        let mut worst_schema_hash: u64 = 0;
        let mut worst_name_hash: u64 = 0;

        for t in tables {
            let Some(prev) = find_prev_table(prev_tables, t.relid) else {
                continue;
            };
            // Skip if collected_at didn't change (cached data)
            if t.collected_at == prev.collected_at {
                continue;
            }
            let dt = (t.collected_at - prev.collected_at) as f64;
            if dt <= 0.0 {
                continue;
            }
            let delta = (t.heap_blks_read - prev.heap_blks_read).max(0);
            if delta < 100 {
                continue; // noise filter: < 100 blocks (800 KiB) — not interesting
            }
            let rate = delta as f64 / dt;
            if rate > worst_rate {
                worst_rate = rate;
                worst_schema_hash = t.schemaname_hash;
                worst_name_hash = t.relname_hash;
            }
        }

        // Threshold: > 50 blocks/s (~400 KiB/s sustained disk reads for one table)
        if worst_rate < 50.0 {
            return Vec::new();
        }

        let name = qualified_name(ctx.interner, worst_schema_hash, worst_name_hash);
        let mb_per_s = worst_rate * 8.0 / 1024.0; // blocks are 8 KiB

        let severity = if worst_rate >= 500.0 {
            Severity::Critical
        } else {
            Severity::Warning
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "heap_read_spike",
            category: Category::PgTables,
            severity,
            title: format!("Table {name}: {worst_rate:.0} blk/s disk reads ({mb_per_s:.1} MiB/s)"),
            detail: None,
            value: worst_rate,
        }]
    }
}

// ============================================================
// TableWriteSpikeRule — burst of inserts/updates/deletes
// ============================================================

pub struct TableWriteSpikeRule;

impl AnalysisRule for TableWriteSpikeRule {
    fn id(&self) -> &'static str {
        "table_write_spike"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let prev_snapshot = match ctx.prev_snapshot {
            Some(s) => s,
            None => return Vec::new(),
        };
        if ctx.dt <= 0.0 {
            return Vec::new();
        }

        let Some(tables) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatUserTables(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let Some(prev_tables) = find_block(prev_snapshot, |b| match b {
            DataBlock::PgStatUserTables(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let mut worst_rate = 0.0_f64;
        let mut worst_schema_hash: u64 = 0;
        let mut worst_name_hash: u64 = 0;

        for t in tables {
            let Some(prev) = find_prev_table(prev_tables, t.relid) else {
                continue;
            };
            if t.collected_at == prev.collected_at {
                continue;
            }
            let dt = (t.collected_at - prev.collected_at) as f64;
            if dt <= 0.0 {
                continue;
            }
            let ins = (t.n_tup_ins - prev.n_tup_ins).max(0);
            let upd = (t.n_tup_upd - prev.n_tup_upd).max(0);
            let del = (t.n_tup_del - prev.n_tup_del).max(0);
            let total = ins + upd + del;
            if total < 1000 {
                continue; // noise filter
            }
            let rate = total as f64 / dt;
            if rate > worst_rate {
                worst_rate = rate;
                worst_schema_hash = t.schemaname_hash;
                worst_name_hash = t.relname_hash;
            }
        }

        // Threshold: > 500 ops/s on single table
        if worst_rate < 500.0 {
            return Vec::new();
        }

        let name = qualified_name(ctx.interner, worst_schema_hash, worst_name_hash);

        let severity = if worst_rate >= 5000.0 {
            Severity::Critical
        } else {
            Severity::Warning
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "table_write_spike",
            category: Category::PgTables,
            severity,
            title: format!("Table {name}: {worst_rate:.0} writes/s"),
            detail: None,
            value: worst_rate,
        }]
    }
}

// ============================================================
// CacheHitRatioDropRule — table cache hit ratio drops
// ============================================================

pub struct CacheHitRatioDropRule;

impl AnalysisRule for CacheHitRatioDropRule {
    fn id(&self) -> &'static str {
        "cache_hit_ratio_drop"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let prev_snapshot = match ctx.prev_snapshot {
            Some(s) => s,
            None => return Vec::new(),
        };
        if ctx.dt <= 0.0 {
            return Vec::new();
        }

        let Some(tables) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgStatUserTables(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let Some(prev_tables) = find_block(prev_snapshot, |b| match b {
            DataBlock::PgStatUserTables(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        // Compute delta hit ratio per table, find worst offender.
        let mut worst_ratio = 100.0_f64;
        let mut worst_schema_hash: u64 = 0;
        let mut worst_name_hash: u64 = 0;

        for t in tables {
            let Some(prev) = find_prev_table(prev_tables, t.relid) else {
                continue;
            };
            if t.collected_at == prev.collected_at {
                continue;
            }
            let read_d = (t.heap_blks_read - prev.heap_blks_read).max(0);
            let hit_d = (t.heap_blks_hit - prev.heap_blks_hit).max(0);
            let total = read_d + hit_d;
            if total < 100 {
                continue; // too few block accesses — noise
            }
            let hit_ratio = hit_d as f64 * 100.0 / total as f64;
            if hit_ratio < worst_ratio {
                worst_ratio = hit_ratio;
                worst_schema_hash = t.schemaname_hash;
                worst_name_hash = t.relname_hash;
            }
        }

        // Threshold: hit ratio < 90% for delta window means heavy disk reads
        if worst_ratio >= 90.0 {
            return Vec::new();
        }

        let name = qualified_name(ctx.interner, worst_schema_hash, worst_name_hash);

        let severity = if worst_ratio < 50.0 {
            Severity::Critical
        } else {
            Severity::Warning
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "cache_hit_ratio_drop",
            category: Category::PgTables,
            severity,
            title: format!("Table {name}: cache hit ratio {worst_ratio:.0}%"),
            detail: None,
            value: 100.0 - worst_ratio, // value = miss percentage (higher = worse)
        }]
    }
}
