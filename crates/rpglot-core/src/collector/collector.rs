//! Main collector that combines process and system collectors.
//!
//! The `Collector` struct provides a unified interface for collecting
//! all system metrics into a `Snapshot` for storage.

use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::collector::cgroup::CgroupCollector;
use crate::collector::pg_collector::PostgresCollector;
use crate::collector::procfs::{CollectError, ProcessCollector, SystemCollector, UserResolver};
use crate::collector::traits::FileSystem;
use crate::storage::interner::StringInterner;
use crate::storage::model::{DataBlock, Snapshot};
use crate::util::is_container;

/// Timing information for each collector phase.
///
/// Used for debugging and performance monitoring.
#[derive(Debug, Clone, Default)]
pub struct CollectorTiming {
    /// Total snapshot collection time.
    pub total: Duration,
    /// Time to collect process information.
    pub processes: Duration,
    /// Time to collect system memory info.
    pub meminfo: Duration,
    /// Time to collect CPU info.
    pub cpuinfo: Duration,
    /// Time to collect load average.
    pub loadavg: Duration,
    /// Time to collect disk statistics.
    pub diskstats: Duration,
    /// Time to collect network device statistics.
    pub netdev: Duration,
    /// Time to collect PSI (Pressure Stall Information).
    pub psi: Duration,
    /// Time to collect vmstat.
    pub vmstat: Duration,
    /// Time to collect global stat.
    pub stat: Duration,
    /// Time to collect network SNMP statistics.
    pub netsnmp: Duration,
    /// Time to collect PostgreSQL activity.
    pub pg_activity: Duration,
    /// Time to collect PostgreSQL statements.
    pub pg_statements: Duration,
    /// Time to collect pg_store_plans.
    pub pg_store_plans: Duration,
    /// Time to collect PostgreSQL database stats.
    pub pg_database: Duration,
    /// Time to collect PostgreSQL bgwriter stats.
    pub pg_bgwriter: Duration,
    /// Time to collect PostgreSQL user tables stats.
    pub pg_tables: Duration,
    /// Time to collect PostgreSQL user indexes stats.
    pub pg_indexes: Duration,
    /// Time to collect PostgreSQL lock tree.
    pub pg_locks: Duration,
    /// Time to collect PostgreSQL log errors.
    pub pg_log: Duration,
    /// Time to collect pg_stat_progress_vacuum.
    pub pg_progress_vacuum: Duration,
    /// Time to collect cgroup metrics.
    pub cgroup: Duration,
    /// PostgreSQL statements caching interval (Duration::ZERO = no caching).
    pub pg_stmts_cache_interval: Option<Duration>,
}

/// Main collector that gathers all system metrics.
///
/// Combines process and system collectors into a single interface
/// that produces complete snapshots for storage.
pub struct Collector<F: FileSystem + Clone> {
    fs: F,
    process_collector: ProcessCollector<F>,
    system_collector: SystemCollector<F>,
    user_resolver: UserResolver,
    postgres_collector: Option<PostgresCollector>,
    pg_last_error: Option<String>,
    cgroup_collector: Option<CgroupCollector<F>>,
    /// Timing information from the last collect_snapshot call.
    last_timing: Option<CollectorTiming>,
}

impl<F: FileSystem + Clone> Collector<F> {
    /// Default cgroup path for containers.
    const DEFAULT_CGROUP_PATH: &'static str = "/sys/fs/cgroup";

    /// Creates a new collector.
    ///
    /// # Arguments
    /// * `fs` - Filesystem implementation (real or mock)
    /// * `proc_path` - Base path to proc filesystem (usually "/proc")
    ///
    /// If running inside a container (detected via `is_container()`),
    /// cgroup collector is automatically enabled with default path `/sys/fs/cgroup`.
    pub fn new(fs: F, proc_path: impl Into<String>) -> Self {
        let proc_path = proc_path.into();

        // Load user resolver from /etc/passwd
        let mut user_resolver = UserResolver::new();
        if let Ok(passwd_content) = fs.read_to_string(Path::new("/etc/passwd")) {
            user_resolver.load_from_content(&passwd_content);
        }

        // Automatically enable cgroup collector in container environment
        let cgroup_collector = if is_container() {
            Some(CgroupCollector::new(fs.clone(), Self::DEFAULT_CGROUP_PATH))
        } else {
            None
        };

        Self {
            fs: fs.clone(),
            process_collector: ProcessCollector::new(fs.clone(), &proc_path),
            system_collector: SystemCollector::new(fs.clone(), &proc_path),
            user_resolver,
            postgres_collector: None,
            pg_last_error: None,
            cgroup_collector,
            last_timing: None,
        }
    }

    /// Enables cgroup metrics collection with custom path.
    ///
    /// This overrides the automatic cgroup detection.
    ///
    /// # Arguments
    /// * `cgroup_path` - Path to cgroup directory (e.g., "/sys/fs/cgroup")
    pub fn with_cgroup(mut self, cgroup_path: &str) -> Self {
        self.cgroup_collector = Some(CgroupCollector::new(self.fs.clone(), cgroup_path));
        self
    }

    /// Forces cgroup metrics collection regardless of container detection.
    ///
    /// Useful for testing on bare metal or when automatic detection fails.
    ///
    /// # Arguments
    /// * `cgroup_path` - Optional custom path. If None, uses default `/sys/fs/cgroup`
    pub fn force_cgroup(mut self, cgroup_path: Option<&str>) -> Self {
        let path = cgroup_path.unwrap_or(Self::DEFAULT_CGROUP_PATH);
        self.cgroup_collector = Some(CgroupCollector::new(self.fs.clone(), path));
        self
    }

    /// Returns whether cgroup collector is enabled.
    pub fn cgroup_enabled(&self) -> bool {
        self.cgroup_collector.is_some()
    }

    /// Enables PostgreSQL metrics collection.
    ///
    /// # Arguments
    /// * `pg_collector` - PostgreSQL collector instance
    pub fn with_postgres(mut self, pg_collector: PostgresCollector) -> Self {
        self.postgres_collector = Some(pg_collector);
        self
    }

    /// Returns the last PostgreSQL error message, if any.
    pub fn pg_last_error(&self) -> Option<&str> {
        self.pg_last_error.as_deref()
    }

    /// Returns a reference to the string interner used for deduplication.
    pub fn interner(&self) -> &StringInterner {
        self.process_collector.interner()
    }

    /// Returns a mutable reference to the string interner.
    pub fn interner_mut(&mut self) -> &mut StringInterner {
        self.process_collector.interner_mut()
    }

    /// Clears the string interner, freeing memory.
    /// Should be called after chunk flush to prevent unbounded growth.
    pub fn clear_interner(&mut self) {
        self.process_collector.clear_interner();
    }

    /// Returns a reference to the user resolver for UID -> username mapping.
    pub fn user_resolver(&self) -> &UserResolver {
        &self.user_resolver
    }

    /// Returns timing information from the last collect_snapshot call.
    pub fn last_timing(&self) -> Option<&CollectorTiming> {
        self.last_timing.as_ref()
    }

    /// Returns instance metadata: (database_name, pg_version).
    pub fn instance_info(&self) -> Option<(String, String)> {
        self.postgres_collector
            .as_ref()
            .and_then(|pg| pg.instance_info())
    }

    /// Returns whether the PostgreSQL instance is in recovery mode (standby).
    pub fn is_in_recovery(&self) -> Option<bool> {
        self.postgres_collector
            .as_ref()
            .and_then(|pg| pg.is_in_recovery())
    }

    /// Collects a complete system snapshot.
    ///
    /// This gathers:
    /// - All process information
    /// - Memory statistics
    /// - CPU statistics
    /// - Load average
    ///
    /// Also records timing information accessible via `last_timing()`.
    pub fn collect_snapshot(&mut self) -> Result<Snapshot, CollectError> {
        let total_start = Instant::now();
        let mut timing = CollectorTiming::default();

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let mut blocks = Vec::new();

        // Collect global stat first to get boot time for process start time calculation
        let start = Instant::now();
        let stat = self.system_collector.collect_stat().ok();
        timing.stat = start.elapsed();
        if let Some(ref stat) = stat {
            self.process_collector.set_boot_time(stat.btime);
        }

        // Collect process information (now with correct boot time)
        let start = Instant::now();
        let processes = self.process_collector.collect_all_processes()?;
        timing.processes = start.elapsed();
        blocks.push(DataBlock::Processes(processes));

        // Collect system memory info
        let start = Instant::now();
        if let Ok(meminfo) = self.system_collector.collect_meminfo() {
            blocks.push(DataBlock::SystemMem(meminfo));
        }
        timing.meminfo = start.elapsed();

        // Collect CPU info
        let start = Instant::now();
        if let Ok(cpuinfo) = self.system_collector.collect_cpuinfo() {
            blocks.push(DataBlock::SystemCpu(cpuinfo));
        }
        timing.cpuinfo = start.elapsed();

        // Collect load average
        let start = Instant::now();
        if let Ok(loadavg) = self.system_collector.collect_loadavg() {
            blocks.push(DataBlock::SystemLoad(loadavg));
        }
        timing.loadavg = start.elapsed();

        // Collect disk statistics
        let start = Instant::now();
        {
            let diskstats_result = if is_container() {
                let mount_devices = self
                    .system_collector
                    .collect_mountinfo_device_ids()
                    .unwrap_or_default();
                self.system_collector
                    .collect_diskstats_with_mountinfo_filter(
                        self.process_collector.interner_mut(),
                        &mount_devices,
                    )
            } else {
                self.system_collector
                    .collect_diskstats(self.process_collector.interner_mut())
            };

            if let Ok(diskstats) = diskstats_result {
                blocks.push(DataBlock::SystemDisk(diskstats));
            }
        }
        timing.diskstats = start.elapsed();

        // Collect network device statistics
        let start = Instant::now();
        if let Ok(netdev) = self
            .system_collector
            .collect_net_dev(self.process_collector.interner_mut())
        {
            blocks.push(DataBlock::SystemNet(netdev));
        }
        timing.netdev = start.elapsed();

        // Collect PSI (Pressure Stall Information)
        let start = Instant::now();
        if let Ok(psi) = self.system_collector.collect_psi()
            && !psi.is_empty()
        {
            blocks.push(DataBlock::SystemPsi(psi));
        }
        timing.psi = start.elapsed();

        // Collect vmstat
        let start = Instant::now();
        if let Ok(vmstat) = self.system_collector.collect_vmstat() {
            blocks.push(DataBlock::SystemVmstat(vmstat));
        }
        timing.vmstat = start.elapsed();

        // Add global stat (already collected at the start for boot time)
        if let Some(stat) = stat {
            blocks.push(DataBlock::SystemStat(stat));
        }

        // Collect network SNMP statistics
        let start = Instant::now();
        if let Ok(netsnmp) = self.system_collector.collect_netsnmp() {
            blocks.push(DataBlock::SystemNetSnmp(netsnmp));
        }
        timing.netsnmp = start.elapsed();

        // Collect PostgreSQL activity (if configured)
        if let Some(ref mut pg_collector) = self.postgres_collector {
            let start = Instant::now();
            let activities = pg_collector.collect(self.process_collector.interner_mut());
            timing.pg_activity = start.elapsed();
            if !activities.is_empty() {
                blocks.push(DataBlock::PgStatActivity(activities));
            }

            let start = Instant::now();
            let statements = pg_collector.collect_statements(self.process_collector.interner_mut());
            timing.pg_statements = start.elapsed();
            if !statements.is_empty() {
                blocks.push(DataBlock::PgStatStatements(statements));
            }

            let start = Instant::now();
            let store_plans =
                pg_collector.collect_store_plans(self.process_collector.interner_mut());
            timing.pg_store_plans = start.elapsed();
            if !store_plans.is_empty() {
                blocks.push(DataBlock::PgStorePlans(store_plans));
            }

            let start = Instant::now();
            let databases = pg_collector.collect_database(self.process_collector.interner_mut());
            timing.pg_database = start.elapsed();
            if !databases.is_empty() {
                blocks.push(DataBlock::PgStatDatabase(databases));
            }

            let start = Instant::now();
            if let Some(bgwriter) = pg_collector.collect_bgwriter() {
                blocks.push(DataBlock::PgStatBgwriter(bgwriter));
            }
            timing.pg_bgwriter = start.elapsed();

            let start = Instant::now();
            let progress_vacuum =
                pg_collector.collect_progress_vacuum(self.process_collector.interner_mut());
            timing.pg_progress_vacuum = start.elapsed();
            if !progress_vacuum.is_empty() {
                blocks.push(DataBlock::PgStatProgressVacuum(progress_vacuum));
            }

            // Ensure per-database connections are established for tables/indexes.
            pg_collector.ensure_db_clients();

            let start = Instant::now();
            match pg_collector.collect_tables(self.process_collector.interner_mut()) {
                Ok(tables) if !tables.is_empty() => {
                    blocks.push(DataBlock::PgStatUserTables(tables));
                }
                _ => {}
            }
            timing.pg_tables = start.elapsed();

            let start = Instant::now();
            match pg_collector.collect_indexes(self.process_collector.interner_mut()) {
                Ok(indexes) if !indexes.is_empty() => {
                    blocks.push(DataBlock::PgStatUserIndexes(indexes));
                }
                _ => {}
            }
            timing.pg_indexes = start.elapsed();

            let start = Instant::now();
            let lock_tree = pg_collector.collect_lock_tree(self.process_collector.interner_mut());
            timing.pg_locks = start.elapsed();
            if !lock_tree.is_empty() {
                blocks.push(DataBlock::PgLockTree(lock_tree));
            }

            let start = Instant::now();
            let log_result = pg_collector.collect_log_data(self.process_collector.interner_mut());
            timing.pg_log = start.elapsed();
            if !log_result.errors.is_empty() {
                blocks.push(DataBlock::PgLogErrors(log_result.errors));
            }
            if log_result.checkpoint_count > 0
                || log_result.autovacuum_count > 0
                || log_result.slow_query_count > 0
            {
                blocks.push(DataBlock::PgLogEvents(
                    crate::storage::model::PgLogEventsInfo {
                        checkpoint_count: log_result.checkpoint_count,
                        autovacuum_count: log_result.autovacuum_count,
                        slow_query_count: log_result.slow_query_count,
                    },
                ));
            }
            if !log_result.events.is_empty() {
                blocks.push(DataBlock::PgLogDetailedEvents(log_result.events));
            }

            let settings = pg_collector.collect_settings();
            if !settings.is_empty() {
                blocks.push(DataBlock::PgSettings(settings));
            }

            if let Some(repl_status) = pg_collector.collect_replication_status() {
                blocks.push(DataBlock::ReplicationStatus(repl_status));
            }

            // Store last error for TUI display
            self.pg_last_error = pg_collector.last_error().map(|s| s.to_string());

            // Store caching interval for debugging
            timing.pg_stmts_cache_interval = Some(pg_collector.statements_cache_interval());
        } else {
            self.pg_last_error = Some("PostgreSQL collector not configured".to_string());
        }

        // Collect cgroup metrics (if collector is configured)
        let start = Instant::now();
        if let Some(ref cgroup_collector) = self.cgroup_collector
            && let Some(cgroup_info) = cgroup_collector.collect()
        {
            blocks.push(DataBlock::Cgroup(cgroup_info));
        }
        timing.cgroup = start.elapsed();

        timing.total = total_start.elapsed();
        self.last_timing = Some(timing);

        Ok(Snapshot { timestamp, blocks })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::mock::MockFs;

    #[test]
    fn test_collect_snapshot() {
        let fs = MockFs::typical_system();
        let mut collector = Collector::new(fs, "/proc");

        let snapshot = collector.collect_snapshot().unwrap();

        // Should have all block types (processes, mem, cpu, load, disk, net, psi, vmstat, stat)
        assert!(snapshot.blocks.len() >= 9);

        // Check for processes
        let has_processes = snapshot
            .blocks
            .iter()
            .any(|b| matches!(b, DataBlock::Processes(_)));
        assert!(has_processes);

        // Check for memory info
        let has_meminfo = snapshot
            .blocks
            .iter()
            .any(|b| matches!(b, DataBlock::SystemMem(_)));
        assert!(has_meminfo);

        // Check for CPU info
        let has_cpuinfo = snapshot
            .blocks
            .iter()
            .any(|b| matches!(b, DataBlock::SystemCpu(_)));
        assert!(has_cpuinfo);

        // Check for load average
        let has_loadavg = snapshot
            .blocks
            .iter()
            .any(|b| matches!(b, DataBlock::SystemLoad(_)));
        assert!(has_loadavg);

        // Check for disk stats
        let has_diskstats = snapshot
            .blocks
            .iter()
            .any(|b| matches!(b, DataBlock::SystemDisk(_)));
        assert!(has_diskstats);

        // Check for network stats
        let has_netdev = snapshot
            .blocks
            .iter()
            .any(|b| matches!(b, DataBlock::SystemNet(_)));
        assert!(has_netdev);

        // Check for PSI
        let has_psi = snapshot
            .blocks
            .iter()
            .any(|b| matches!(b, DataBlock::SystemPsi(_)));
        assert!(has_psi);

        // Check for vmstat
        let has_vmstat = snapshot
            .blocks
            .iter()
            .any(|b| matches!(b, DataBlock::SystemVmstat(_)));
        assert!(has_vmstat);

        // Check for stat
        let has_stat = snapshot
            .blocks
            .iter()
            .any(|b| matches!(b, DataBlock::SystemStat(_)));
        assert!(has_stat);
    }

    #[test]
    fn test_collect_snapshot_processes() {
        let fs = MockFs::typical_system();
        let mut collector = Collector::new(fs, "/proc");

        let snapshot = collector.collect_snapshot().unwrap();

        // Find processes block
        let processes = snapshot.blocks.iter().find_map(|b| {
            if let DataBlock::Processes(p) = b {
                Some(p)
            } else {
                None
            }
        });

        assert!(processes.is_some());
        let processes = processes.unwrap();
        assert_eq!(processes.len(), 3); // typical_system has 3 processes
    }

    #[test]
    fn test_interner_persistence() {
        let fs = MockFs::typical_system();
        let mut collector = Collector::new(fs, "/proc");

        // Collect first snapshot
        let snapshot1 = collector.collect_snapshot().unwrap();

        // Collect second snapshot
        let snapshot2 = collector.collect_snapshot().unwrap();

        // Same process names should have same hash (interner persists)
        let get_first_process = |snapshot: &Snapshot| {
            snapshot.blocks.iter().find_map(|b| {
                if let DataBlock::Processes(p) = b {
                    p.iter().find(|proc| proc.pid == 1).cloned()
                } else {
                    None
                }
            })
        };

        let proc1 = get_first_process(&snapshot1).unwrap();
        let proc2 = get_first_process(&snapshot2).unwrap();

        // Same process should have same name hash
        assert_eq!(proc1.name_hash, proc2.name_hash);
    }
}
