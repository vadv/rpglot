use std::collections::HashMap;

use crate::analysis::rules::AnalysisRule;
use crate::analysis::{AnalysisContext, Anomaly, Category, Severity, find_block};
use crate::storage::model::{DataBlock, ErrorCategory, PgLogSeverity};

/// Local severity mapping for error categories.
/// The backend stores only the category; severity interpretation lives in consumers.
fn category_severity(cat: ErrorCategory) -> Severity {
    match cat {
        ErrorCategory::Lock | ErrorCategory::Constraint | ErrorCategory::Serialization => {
            Severity::Info
        }
        ErrorCategory::Resource | ErrorCategory::DataCorruption | ErrorCategory::System => {
            Severity::Critical
        }
        _ => Severity::Warning,
    }
}

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

        // Group error counts by category.
        let mut by_category: HashMap<ErrorCategory, u64> = HashMap::new();
        for e in entries {
            if e.severity == PgLogSeverity::Error {
                *by_category.entry(e.category).or_default() += e.count as u64;
            }
        }

        let mut anomalies = Vec::new();
        for (cat, count) in &by_category {
            let sev = category_severity(*cat);
            // Info-level categories (lock, constraint, serialization): only report if >= 100.
            // Warning/Critical: always report.
            let threshold = match sev {
                Severity::Info => 100,
                _ => 1,
            };
            if *count < threshold {
                continue;
            }
            let label = cat.label();
            anomalies.push(Anomaly {
                timestamp: ctx.timestamp,
                rule_id: "pg_errors",
                category: Category::PgErrors,
                severity: sev,
                title: format!("{count} PostgreSQL error(s) [{label}]"),
                detail: None,
                value: *count as f64,
                merge_key: Some(label.to_string()),
                entity_id: None,
            });
        }

        anomalies
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

        let mut anomalies = Vec::new();
        for e in entries {
            match e.severity {
                PgLogSeverity::Fatal | PgLogSeverity::Panic => {
                    let message = ctx
                        .interner
                        .resolve(e.pattern_hash)
                        .unwrap_or("unknown error");
                    let sev_str = match e.severity {
                        PgLogSeverity::Fatal => "FATAL",
                        PgLogSeverity::Panic => "PANIC",
                        _ => "ERROR",
                    };
                    let cat_label = e.category.label();

                    anomalies.push(Anomaly {
                        timestamp: ctx.timestamp,
                        rule_id: "pg_fatal_panic",
                        category: Category::PgErrors,
                        severity: Severity::Critical,
                        title: format!("PostgreSQL {sev_str}: {message} ({cat_label})"),
                        detail: None,
                        value: e.count as f64,
                        merge_key: Some(format!("{sev_str}_{cat_label}")),
                        entity_id: None,
                    });
                }
                PgLogSeverity::Error => {}
            }
        }

        anomalies
    }
}
