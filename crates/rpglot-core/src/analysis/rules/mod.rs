pub mod cgroup;
pub mod cpu;
pub mod disk;
pub mod load;
pub mod memory;
pub mod network;
pub mod pg_activity;
pub mod pg_bgwriter;
pub mod pg_errors;
pub mod pg_events;
pub mod pg_indexes;
pub mod pg_locks;
pub mod pg_statements;
pub mod pg_tables;
pub mod process_blkdelay;
pub mod process_io;

use super::{AnalysisContext, Anomaly};

pub trait AnalysisRule: Send + Sync {
    fn id(&self) -> &'static str;
    fn evaluate(&self, ctx: &AnalysisContext) -> Vec<Anomaly>;
}

pub fn all_rules() -> Vec<Box<dyn AnalysisRule>> {
    vec![
        // CPU / Load
        Box::new(cpu::CpuHighRule),
        Box::new(cpu::IowaitHighRule),
        Box::new(cpu::StealHighRule),
        Box::new(load::LoadAverageHighRule),
        // Memory
        Box::new(memory::MemoryLowRule),
        Box::new(memory::SwapUsageRule),
        // Disk
        Box::new(disk::DiskUtilHighRule),
        Box::new(disk::DiskIoSpikeRule),
        Box::new(disk::DiskLatencyHighRule),
        // Network
        Box::new(network::NetworkSpikeRule),
        // PG Activity
        Box::new(pg_activity::IdleInTransactionRule),
        Box::new(pg_activity::LongQueryRule),
        Box::new(pg_activity::WaitSyncReplicaRule),
        Box::new(pg_activity::WaitLockRule),
        Box::new(pg_activity::HighActiveSessionsRule),
        Box::new(pg_activity::TpsSpikeRule),
        // PG Statements
        Box::new(pg_statements::MeanTimeSpikeRule),
        Box::new(pg_statements::QueryCallSpikeRule),
        // PG Locks
        Box::new(pg_locks::BlockedSessionsRule),
        // PG Tables
        Box::new(pg_tables::DeadTuplesHighRule),
        Box::new(pg_tables::SeqScanDominantRule),
        Box::new(pg_tables::HeapReadSpikeRule),
        Box::new(pg_tables::TableWriteSpikeRule),
        Box::new(pg_tables::CacheHitRatioDropRule),
        // PG Indexes
        Box::new(pg_indexes::IndexReadSpikeRule),
        Box::new(pg_indexes::IndexCacheHitDropRule),
        // PG Events (log-based)
        Box::new(pg_events::AutovacuumImpactRule),
        // PG BGWriter
        Box::new(pg_bgwriter::CheckpointSpikeRule),
        Box::new(pg_bgwriter::BackendBuffersRule),
        // PG Errors
        Box::new(pg_errors::ErrorsRule),
        Box::new(pg_errors::FatalPanicRule),
        // Cgroup
        Box::new(cgroup::ThrottledRule),
        Box::new(cgroup::OomKillRule),
        // Process-level
        Box::new(process_io::ProcessIoHogRule),
        Box::new(process_blkdelay::HighBlkDelayRule),
    ]
}
