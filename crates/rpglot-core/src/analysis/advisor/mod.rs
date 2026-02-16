pub mod recommendations;

use super::{Incident, Severity};
use serde::Serialize;

#[derive(Serialize)]
pub struct Recommendation {
    pub id: String,
    pub severity: Severity,
    pub title: String,
    pub description: String,
    pub related_incidents: Vec<String>,
}

pub trait Advisor: Send + Sync {
    fn id(&self) -> &'static str;
    fn evaluate(&self, incidents: &[Incident]) -> Vec<Recommendation>;
}

pub fn all_advisors() -> Vec<Box<dyn Advisor>> {
    vec![
        Box::new(recommendations::ReplicationLagAdvisor),
        Box::new(recommendations::LockContentionAdvisor),
        Box::new(recommendations::HighCpuAdvisor),
        Box::new(recommendations::MemoryPressureAdvisor),
        Box::new(recommendations::TableBloatAdvisor),
        Box::new(recommendations::CheckpointStormAdvisor),
        Box::new(recommendations::IoBottleneckAdvisor),
        Box::new(recommendations::ErrorStormAdvisor),
        Box::new(recommendations::CgroupThrottleAdvisor),
    ]
}
