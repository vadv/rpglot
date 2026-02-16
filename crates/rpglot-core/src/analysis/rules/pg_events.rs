use crate::analysis::rules::AnalysisRule;
use crate::analysis::{AnalysisContext, Anomaly, Category, Severity, find_block};
use crate::storage::model::{DataBlock, PgLogEventType};

// ============================================================
// AutovacuumImpactRule — heavy autovacuum causing disk + network load
// ============================================================

pub struct AutovacuumImpactRule;

impl AnalysisRule for AutovacuumImpactRule {
    fn id(&self) -> &'static str {
        "autovacuum_impact"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some(events) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgLogDetailedEvents(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        // Find the heaviest autovacuum event in this snapshot by write rate.
        let mut worst_write_rate = 0.0_f64;
        let mut worst_table = "";
        let mut worst_wal_bytes: i64 = 0;
        let mut worst_elapsed = 0.0_f64;
        let mut worst_read_rate = 0.0_f64;

        for ev in events {
            if ev.event_type != PgLogEventType::Autovacuum {
                continue;
            }
            let impact = ev.avg_write_rate_mbs + ev.avg_read_rate_mbs;
            if impact > worst_write_rate + worst_read_rate {
                worst_write_rate = ev.avg_write_rate_mbs;
                worst_read_rate = ev.avg_read_rate_mbs;
                worst_table = &ev.table_name;
                worst_wal_bytes = ev.wal_bytes;
                worst_elapsed = ev.elapsed_s;
            }
        }

        // Threshold: write rate > 10 MB/s or WAL > 100 MB — heavy enough to impact disk/replicas
        let wal_mb = worst_wal_bytes as f64 / 1_048_576.0;
        let total_io = worst_write_rate + worst_read_rate;

        if total_io < 10.0 && wal_mb < 100.0 {
            return Vec::new();
        }

        let severity = if total_io >= 50.0 || wal_mb >= 500.0 {
            Severity::Critical
        } else {
            Severity::Warning
        };

        let detail = format!(
            "Write: {worst_write_rate:.1} MB/s, Read: {worst_read_rate:.1} MB/s, \
             WAL: {wal_mb:.0} MB, Duration: {worst_elapsed:.0}s",
        );

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "autovacuum_impact",
            category: Category::PgEvents,
            severity,
            title: format!(
                "Autovacuum on {worst_table}: {total_io:.1} MB/s I/O, {wal_mb:.0} MB WAL"
            ),
            detail: Some(detail),
            value: total_io,
            merge_key: None,
        }]
    }
}
