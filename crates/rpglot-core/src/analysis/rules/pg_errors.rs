use crate::analysis::rules::AnalysisRule;
use crate::analysis::{AnalysisContext, Anomaly, Category, Severity, find_block};
use crate::storage::model::{DataBlock, PgLogSeverity};

// ============================================================
// ErrorsRule
// ============================================================

pub struct ErrorsRule;

impl AnalysisRule for ErrorsRule {
    fn id(&self) -> &'static str {
        "pg_errors"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some(entries) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgLogErrors(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        let mut total: u64 = 0;
        let mut first_pattern_hash: u64 = 0;

        for e in entries {
            if e.severity == PgLogSeverity::Error {
                total += e.count as u64;
                if first_pattern_hash == 0 {
                    first_pattern_hash = e.pattern_hash;
                }
            }
        }

        if total == 0 {
            return Vec::new();
        }

        let severity = if total >= 10 {
            Severity::Critical
        } else {
            Severity::Warning
        };

        let detail = if first_pattern_hash != 0 {
            ctx.interner
                .resolve(first_pattern_hash)
                .map(|s| s.to_string())
        } else {
            None
        };

        vec![Anomaly {
            timestamp: ctx.timestamp,
            rule_id: "pg_errors",
            category: Category::PgErrors,
            severity,
            title: format!("{total} PostgreSQL error(s)"),
            detail,
            value: total as f64,
            merge_key: None,
        }]
    }
}

// ============================================================
// FatalPanicRule
// ============================================================

pub struct FatalPanicRule;

impl AnalysisRule for FatalPanicRule {
    fn id(&self) -> &'static str {
        "pg_fatal_panic"
    }

    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly> {
        let Some(entries) = find_block(ctx.snapshot, |b| match b {
            DataBlock::PgLogErrors(v) => Some(v.as_slice()),
            _ => None,
        }) else {
            return Vec::new();
        };

        for e in entries {
            match e.severity {
                PgLogSeverity::Fatal | PgLogSeverity::Panic => {
                    let message = ctx
                        .interner
                        .resolve(e.pattern_hash)
                        .unwrap_or("unknown error");

                    return vec![Anomaly {
                        timestamp: ctx.timestamp,
                        rule_id: "pg_fatal_panic",
                        category: Category::PgErrors,
                        severity: Severity::Critical,
                        title: format!("PostgreSQL FATAL/PANIC: {message}"),
                        detail: None,
                        value: e.count as f64,
                        merge_key: None,
                    }];
                }
                PgLogSeverity::Error => {}
            }
        }

        Vec::new()
    }
}
