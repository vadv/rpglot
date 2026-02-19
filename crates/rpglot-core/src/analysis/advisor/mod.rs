pub mod recommendations;

use super::{Incident, Severity};
use crate::storage::model::PgSettingEntry;
use crate::storage::model::Snapshot;
use serde::Serialize;

#[derive(Serialize)]
pub struct Recommendation {
    pub id: String,
    pub severity: Severity,
    pub title: String,
    pub description: String,
    pub related_incidents: Vec<String>,
}

/// Context passed to each advisor for evaluation.
pub struct AdvisorContext<'a> {
    pub incidents: &'a [Incident],
    pub settings: Option<PgSettings<'a>>,
    /// Last snapshot in the analyzed range (for data-driven advisors).
    pub snapshot: Option<&'a Snapshot>,
    /// Previous snapshot (for delta computation in advisors).
    pub prev_snapshot: Option<&'a Snapshot>,
}

/// Convenient wrapper around pg_settings entries for typed access.
pub struct PgSettings<'a>(&'a [PgSettingEntry]);

impl<'a> PgSettings<'a> {
    pub fn new(entries: &'a [PgSettingEntry]) -> Self {
        Self(entries)
    }

    /// Get raw setting string by name.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|e| e.name == name)
            .map(|e| e.setting.as_str())
    }

    /// Get setting as i64.
    pub fn get_i64(&self, name: &str) -> Option<i64> {
        self.get(name)?.parse().ok()
    }

    /// Get setting as f64.
    pub fn get_f64(&self, name: &str) -> Option<f64> {
        self.get(name)?.parse().ok()
    }

    /// Get memory setting in bytes, handling pg_settings unit conversion.
    pub fn get_bytes(&self, name: &str) -> Option<i64> {
        let entry = self.0.iter().find(|e| e.name == name)?;
        let val: i64 = entry.setting.parse().ok()?;
        let multiplier = match entry.unit.as_str() {
            "8kB" => 8192,
            "kB" => 1024,
            "MB" => 1024 * 1024,
            _ => 1,
        };
        Some(val * multiplier)
    }

    /// Get time setting in milliseconds, handling pg_settings unit conversion.
    pub fn get_ms(&self, name: &str) -> Option<i64> {
        let entry = self.0.iter().find(|e| e.name == name)?;
        let val: i64 = entry.setting.parse().ok()?;
        let multiplier = match entry.unit.as_str() {
            "ms" => 1,
            "s" => 1000,
            "min" => 60_000,
            _ => 1,
        };
        Some(val * multiplier)
    }
}

pub trait Advisor: Send + Sync {
    fn id(&self) -> &'static str;
    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation>;
}

pub fn all_advisors() -> Vec<Box<dyn Advisor>> {
    vec![
        // Existing advisors
        Box::new(recommendations::ReplicationLagAdvisor),
        Box::new(recommendations::LockContentionAdvisor),
        Box::new(recommendations::HighCpuAdvisor),
        Box::new(recommendations::MemoryPressureAdvisor),
        Box::new(recommendations::TableBloatAdvisor),
        Box::new(recommendations::CheckpointStormAdvisor),
        Box::new(recommendations::IoBottleneckAdvisor),
        Box::new(recommendations::ErrorStormAdvisor),
        Box::new(recommendations::CgroupThrottleAdvisor),
        // New correlation advisors
        Box::new(recommendations::VacuumBlockedAdvisor),
        Box::new(recommendations::LockCascadeAdvisor),
        Box::new(recommendations::ConnectionStormAdvisor),
        Box::new(recommendations::WriteAmplificationAdvisor),
        Box::new(recommendations::CacheMissAdvisor),
        Box::new(recommendations::SeqScanCpuAdvisor),
        Box::new(recommendations::AutovacuumPressureAdvisor),
        Box::new(recommendations::OomRiskAdvisor),
        Box::new(recommendations::QueryRegressionAdvisor),
        Box::new(recommendations::TempFileSpillAdvisor),
        Box::new(recommendations::PlanRegressionAdvisor),
    ]
}
