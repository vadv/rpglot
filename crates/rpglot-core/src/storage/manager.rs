use crate::storage::chunk::Chunk;
use crate::storage::interner::StringInterner;
#[allow(unused_imports)]
use crate::storage::model::{
    DataBlock, DataBlockDiff, Delta, PgLockTreeNode, PgStatActivityInfo, PgStatDatabaseInfo,
    PgStatStatementsInfo, PgStatUserIndexesInfo, PgStatUserTablesInfo, ProcessInfo, Snapshot,
    SystemCpuInfo, SystemDiskInfo, SystemFileInfo, SystemInterruptInfo, SystemLoadInfo,
    SystemMemInfo, SystemNetInfo, SystemNetSnmpInfo, SystemPsiInfo, SystemSoftirqInfo,
    SystemStatInfo, SystemVmstatInfo,
};
use chrono::{DateTime, NaiveDate, Timelike, Utc};
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::warn;

/// Configuration for automatic data rotation.
#[derive(Debug, Clone)]
pub struct RotationConfig {
    /// Maximum total size of all data files in bytes. Default: 1GB.
    pub max_total_size: u64,
    /// Maximum retention period in days. Default: 7 days.
    pub max_retention_days: u32,
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            max_total_size: 1_073_741_824, // 1GB
            max_retention_days: 7,
        }
    }
}

impl RotationConfig {
    /// Creates a new RotationConfig with custom values.
    pub fn new(max_total_size: u64, max_retention_days: u32) -> Self {
        Self {
            max_total_size,
            max_retention_days,
        }
    }
}

/// WAL entry containing a snapshot and its string interner.
/// Each WAL entry is self-contained for recovery purposes.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct WalEntry {
    pub(crate) snapshot: Snapshot,
    pub(crate) interner: StringInterner,
}

pub struct StorageManager {
    base_path: PathBuf,
    chunk_size_limit: usize,
    wal_file: File,
    /// Number of entries currently in WAL (for size limit checking)
    wal_entries_count: usize,
    /// Current hour (0-23) for hourly file segmentation
    current_hour: Option<u32>,
    /// Current date for hourly file segmentation
    current_date: Option<NaiveDate>,
}

impl StorageManager {
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        let base_path = base_path.into();
        std::fs::create_dir_all(&base_path).unwrap();

        // Cleanup old .tmp files
        if let Ok(entries) = std::fs::read_dir(&base_path) {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|ext| ext == "tmp") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }

        let wal_path = base_path.join("wal.log");
        let wal_file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&wal_path)
            .unwrap();

        let mut manager = Self {
            base_path,
            chunk_size_limit: 360, // ~1 hour at 10-second intervals
            wal_file,
            wal_entries_count: 0,
            current_hour: None,
            current_date: None,
        };

        manager.recover_from_wal();
        manager
    }

    /// Recovers WAL state on startup.
    /// Counts valid entries and truncates any corrupted data at the end.
    fn recover_from_wal(&mut self) {
        let wal_path = self.base_path.join("wal.log");

        // Migration: remove old strings.bin if exists (no longer needed)
        let strings_path = self.base_path.join("strings.bin");
        if strings_path.exists() {
            let _ = std::fs::remove_file(&strings_path);
        }

        let data = match std::fs::read(&wal_path) {
            Ok(d) if !d.is_empty() => d,
            _ => return,
        };

        let mut cursor = std::io::Cursor::new(&data);
        let mut valid_end_position = 0u64;
        let mut recovered_count = 0usize;

        // Count valid WAL entries and find valid end position
        while let Ok(_entry) = bincode::deserialize_from::<_, WalEntry>(&mut cursor) {
            valid_end_position = cursor.position();
            recovered_count += 1;
        }

        self.wal_entries_count = recovered_count;

        // Check if there's garbage after valid records (corruption detected)
        let file_size = data.len() as u64;
        if valid_end_position < file_size && valid_end_position > 0 {
            let garbage_bytes = file_size - valid_end_position;
            warn!(
                "WAL corruption detected: {} garbage bytes after {} valid records. Truncating WAL.",
                garbage_bytes, recovered_count
            );

            // Truncate WAL to remove corrupted data
            if let Err(e) = OpenOptions::new()
                .write(true)
                .open(&wal_path)
                .and_then(|f| f.set_len(valid_end_position))
            {
                warn!("Failed to truncate WAL: {}", e);
            }
        }
    }

    /// Collects all string hashes used in a single snapshot.
    fn collect_snapshot_hashes(snapshot: &Snapshot) -> HashSet<u64> {
        let mut hashes = HashSet::new();
        for block in &snapshot.blocks {
            match block {
                DataBlock::Processes(procs) => {
                    for p in procs {
                        hashes.insert(p.name_hash);
                        hashes.insert(p.cmdline_hash);
                        hashes.insert(p.cpu.wchan_hash);
                    }
                }
                DataBlock::SystemNet(nets) => {
                    for n in nets {
                        hashes.insert(n.name_hash);
                    }
                }
                DataBlock::SystemDisk(disks) => {
                    for d in disks {
                        hashes.insert(d.device_hash);
                    }
                }
                DataBlock::SystemInterrupts(intrs) => {
                    for i in intrs {
                        hashes.insert(i.irq_hash);
                    }
                }
                DataBlock::SystemSoftirqs(softirqs) => {
                    for s in softirqs {
                        hashes.insert(s.name_hash);
                    }
                }
                DataBlock::PgStatActivity(activities) => {
                    for a in activities {
                        hashes.insert(a.datname_hash);
                        hashes.insert(a.usename_hash);
                        hashes.insert(a.application_name_hash);
                        hashes.insert(a.state_hash);
                        hashes.insert(a.query_hash);
                        hashes.insert(a.wait_event_type_hash);
                        hashes.insert(a.wait_event_hash);
                        hashes.insert(a.backend_type_hash);
                    }
                }
                DataBlock::PgStatStatements(stmts) => {
                    for s in stmts {
                        hashes.insert(s.query_hash);
                    }
                }
                DataBlock::PgStatDatabase(dbs) => {
                    for d in dbs {
                        hashes.insert(d.datname_hash);
                    }
                }
                DataBlock::PgStatUserTables(tables) => {
                    for t in tables {
                        hashes.insert(t.schemaname_hash);
                        hashes.insert(t.relname_hash);
                    }
                }
                DataBlock::PgStatUserIndexes(indexes) => {
                    for i in indexes {
                        hashes.insert(i.schemaname_hash);
                        hashes.insert(i.relname_hash);
                        hashes.insert(i.indexrelname_hash);
                    }
                }
                DataBlock::PgLockTree(nodes) => {
                    for n in nodes {
                        hashes.insert(n.datname_hash);
                        hashes.insert(n.usename_hash);
                        hashes.insert(n.state_hash);
                        hashes.insert(n.wait_event_type_hash);
                        hashes.insert(n.wait_event_hash);
                        hashes.insert(n.query_hash);
                        hashes.insert(n.application_name_hash);
                        hashes.insert(n.backend_type_hash);
                        hashes.insert(n.lock_type_hash);
                        hashes.insert(n.lock_mode_hash);
                        hashes.insert(n.lock_target_hash);
                    }
                }
                _ => {}
            }
        }
        hashes
    }

    /// Adds a snapshot to storage with hourly segmentation.
    /// Returns true if a chunk was flushed (hour boundary crossed or size limit reached).
    pub fn add_snapshot(&mut self, snapshot: Snapshot, interner: &StringInterner) -> bool {
        // Check if hour changed and flush if needed
        let now = Utc::now();
        let current_hour = now.hour();
        let current_date = now.date_naive();
        let mut flushed = false;

        if let (Some(prev_hour), Some(prev_date)) = (self.current_hour, self.current_date)
            && (prev_hour != current_hour || prev_date != current_date)
            && self.wal_entries_count > 0
        {
            // Hour changed, flush the current chunk
            let _ = self.flush_chunk_with_time(prev_date, prev_hour);
            flushed = true;
        }

        // Update current hour/date tracking
        self.current_hour = Some(current_hour);
        self.current_date = Some(current_date);

        // Create minimal interner for this WAL entry (only hashes used in this snapshot)
        let used_hashes = Self::collect_snapshot_hashes(&snapshot);
        let wal_interner = interner.filter(&used_hashes);

        // Write to WAL for SIGKILL resilience
        let wal_entry = WalEntry {
            snapshot,
            interner: wal_interner,
        };
        let encoded = bincode::serialize(&wal_entry).unwrap();
        self.wal_file.write_all(&encoded).unwrap();
        self.wal_file.sync_all().unwrap();
        self.wal_entries_count += 1;

        // Check if size limit reached
        if self.wal_entries_count >= self.chunk_size_limit {
            let _ = self.flush_chunk();
            flushed = true;
        }

        flushed
    }

    /// Adds a snapshot with a specific timestamp (for testing or replay).
    /// Returns true if a chunk was flushed.
    pub fn add_snapshot_at(
        &mut self,
        snapshot: Snapshot,
        time: DateTime<Utc>,
        interner: &StringInterner,
    ) -> bool {
        let hour = time.hour();
        let date = time.date_naive();
        let mut flushed = false;

        if let (Some(prev_hour), Some(prev_date)) = (self.current_hour, self.current_date)
            && (prev_hour != hour || prev_date != date)
            && self.wal_entries_count > 0
        {
            let _ = self.flush_chunk_with_time(prev_date, prev_hour);
            flushed = true;
        }

        self.current_hour = Some(hour);
        self.current_date = Some(date);

        // Create minimal interner for this WAL entry
        let used_hashes = Self::collect_snapshot_hashes(&snapshot);
        let wal_interner = interner.filter(&used_hashes);

        // Write to WAL
        let wal_entry = WalEntry {
            snapshot,
            interner: wal_interner,
        };
        let encoded = bincode::serialize(&wal_entry).unwrap();
        self.wal_file.write_all(&encoded).unwrap();
        self.wal_file.sync_all().unwrap();
        self.wal_entries_count += 1;

        // Check if size limit reached
        if self.wal_entries_count >= self.chunk_size_limit {
            let _ = self.flush_chunk_with_time(date, hour);
            flushed = true;
        }

        flushed
    }

    fn compute_delta(&self, last: &Snapshot, current: &Snapshot) -> Delta {
        let mut diff_blocks = Vec::new();

        for curr_block in &current.blocks {
            match curr_block {
                DataBlock::Processes(curr_procs) => {
                    let last_procs = last.blocks.iter().find_map(|b| {
                        if let DataBlock::Processes(p) = b {
                            Some(p)
                        } else {
                            None
                        }
                    });

                    if let Some(last_procs) = last_procs {
                        diff_blocks.push(self.compute_processes_diff(last_procs, curr_procs));
                    } else {
                        diff_blocks.push(DataBlockDiff::Processes {
                            updates: curr_procs.clone(),
                            removals: Vec::new(),
                        });
                    }
                }
                DataBlock::PgStatActivity(curr_activity) => {
                    let last_activity = last.blocks.iter().find_map(|b| {
                        if let DataBlock::PgStatActivity(a) = b {
                            Some(a)
                        } else {
                            None
                        }
                    });

                    if let Some(last_activity) = last_activity {
                        diff_blocks
                            .push(self.compute_pg_activity_diff(last_activity, curr_activity));
                    } else {
                        diff_blocks.push(DataBlockDiff::PgStatActivity {
                            updates: curr_activity.clone(),
                            removals: Vec::new(),
                        });
                    }
                }
                DataBlock::PgStatStatements(curr_stats) => {
                    let last_stats = last.blocks.iter().find_map(|b| {
                        if let DataBlock::PgStatStatements(s) = b {
                            Some(s)
                        } else {
                            None
                        }
                    });

                    if let Some(last_stats) = last_stats {
                        diff_blocks.push(self.compute_pg_statements_diff(last_stats, curr_stats));
                    } else {
                        diff_blocks.push(DataBlockDiff::PgStatStatements {
                            updates: curr_stats.clone(),
                            removals: Vec::new(),
                        });
                    }
                }
                DataBlock::PgStatDatabase(curr_db) => {
                    let last_db = last.blocks.iter().find_map(|b| {
                        if let DataBlock::PgStatDatabase(d) = b {
                            Some(d)
                        } else {
                            None
                        }
                    });

                    if let Some(last_db) = last_db {
                        diff_blocks.push(self.compute_pg_database_diff(last_db, curr_db));
                    } else {
                        diff_blocks.push(DataBlockDiff::PgStatDatabase {
                            updates: curr_db.clone(),
                            removals: Vec::new(),
                        });
                    }
                }
                DataBlock::PgStatUserTables(curr_tables) => {
                    let last_tables = last.blocks.iter().find_map(|b| {
                        if let DataBlock::PgStatUserTables(t) = b {
                            Some(t)
                        } else {
                            None
                        }
                    });

                    if let Some(last_tables) = last_tables {
                        diff_blocks.push(self.compute_pg_tables_diff(last_tables, curr_tables));
                    } else {
                        diff_blocks.push(DataBlockDiff::PgStatUserTables {
                            updates: curr_tables.clone(),
                            removals: Vec::new(),
                        });
                    }
                }
                DataBlock::PgStatUserIndexes(curr_indexes) => {
                    let last_indexes = last.blocks.iter().find_map(|b| {
                        if let DataBlock::PgStatUserIndexes(i) = b {
                            Some(i)
                        } else {
                            None
                        }
                    });

                    if let Some(last_indexes) = last_indexes {
                        diff_blocks.push(self.compute_pg_indexes_diff(last_indexes, curr_indexes));
                    } else {
                        diff_blocks.push(DataBlockDiff::PgStatUserIndexes {
                            updates: curr_indexes.clone(),
                            removals: Vec::new(),
                        });
                    }
                }
                DataBlock::PgLockTree(curr_locks) => {
                    let last_locks = last.blocks.iter().find_map(|b| {
                        if let DataBlock::PgLockTree(l) = b {
                            Some(l)
                        } else {
                            None
                        }
                    });

                    if let Some(last_locks) = last_locks {
                        diff_blocks.push(self.compute_pg_lock_tree_diff(last_locks, curr_locks));
                    } else {
                        diff_blocks.push(DataBlockDiff::PgLockTree {
                            updates: curr_locks.clone(),
                            removals: Vec::new(),
                        });
                    }
                }
                DataBlock::PgStatBgwriter(curr_bgw) => {
                    diff_blocks.push(DataBlockDiff::PgStatBgwriter(curr_bgw.clone()));
                }
                DataBlock::SystemCpu(curr_cpu) => {
                    let last_cpu = last.blocks.iter().find_map(|b| {
                        if let DataBlock::SystemCpu(c) = b {
                            Some(c)
                        } else {
                            None
                        }
                    });

                    if let Some(last_cpu) = last_cpu {
                        diff_blocks.push(self.compute_system_cpu_diff(last_cpu, curr_cpu));
                    } else {
                        diff_blocks.push(DataBlockDiff::SystemCpu {
                            updates: curr_cpu.clone(),
                            removals: Vec::new(),
                        });
                    }
                }
                DataBlock::SystemLoad(curr_load) => {
                    diff_blocks.push(DataBlockDiff::SystemLoad(curr_load.clone()));
                }
                DataBlock::SystemMem(curr_mem) => {
                    diff_blocks.push(DataBlockDiff::SystemMem(curr_mem.clone()));
                }
                DataBlock::SystemNet(curr_net) => {
                    let last_net = last.blocks.iter().find_map(|b| {
                        if let DataBlock::SystemNet(n) = b {
                            Some(n)
                        } else {
                            None
                        }
                    });

                    if let Some(last_net) = last_net {
                        diff_blocks.push(self.compute_system_net_diff(last_net, curr_net));
                    } else {
                        diff_blocks.push(DataBlockDiff::SystemNet {
                            updates: curr_net.clone(),
                            removals: Vec::new(),
                        });
                    }
                }
                DataBlock::SystemDisk(curr_disk) => {
                    let last_disk = last.blocks.iter().find_map(|b| {
                        if let DataBlock::SystemDisk(d) = b {
                            Some(d)
                        } else {
                            None
                        }
                    });

                    if let Some(last_disk) = last_disk {
                        diff_blocks.push(self.compute_system_disk_diff(last_disk, curr_disk));
                    } else {
                        diff_blocks.push(DataBlockDiff::SystemDisk {
                            updates: curr_disk.clone(),
                            removals: Vec::new(),
                        });
                    }
                }
                DataBlock::SystemPsi(curr_psi) => {
                    diff_blocks.push(DataBlockDiff::SystemPsi(curr_psi.clone()));
                }
                DataBlock::SystemVmstat(curr_vmstat) => {
                    diff_blocks.push(DataBlockDiff::SystemVmstat(curr_vmstat.clone()));
                }
                DataBlock::SystemFile(curr_file) => {
                    diff_blocks.push(DataBlockDiff::SystemFile(curr_file.clone()));
                }
                DataBlock::SystemInterrupts(curr_irq) => {
                    let last_irq = last.blocks.iter().find_map(|b| {
                        if let DataBlock::SystemInterrupts(i) = b {
                            Some(i)
                        } else {
                            None
                        }
                    });

                    if let Some(last_irq) = last_irq {
                        diff_blocks.push(self.compute_system_interrupts_diff(last_irq, curr_irq));
                    } else {
                        diff_blocks.push(DataBlockDiff::SystemInterrupts {
                            updates: curr_irq.clone(),
                            removals: Vec::new(),
                        });
                    }
                }
                DataBlock::SystemSoftirqs(curr_softirq) => {
                    let last_softirq = last.blocks.iter().find_map(|b| {
                        if let DataBlock::SystemSoftirqs(s) = b {
                            Some(s)
                        } else {
                            None
                        }
                    });

                    if let Some(last_softirq) = last_softirq {
                        diff_blocks
                            .push(self.compute_system_softirqs_diff(last_softirq, curr_softirq));
                    } else {
                        diff_blocks.push(DataBlockDiff::SystemSoftirqs {
                            updates: curr_softirq.clone(),
                            removals: Vec::new(),
                        });
                    }
                }
                DataBlock::SystemStat(curr_stat) => {
                    diff_blocks.push(DataBlockDiff::SystemStat(curr_stat.clone()));
                }
                DataBlock::SystemNetSnmp(curr_snmp) => {
                    diff_blocks.push(DataBlockDiff::SystemNetSnmp(curr_snmp.clone()));
                }
                DataBlock::Cgroup(curr_cgroup) => {
                    diff_blocks.push(DataBlockDiff::Cgroup(curr_cgroup.clone()));
                }
            }
        }

        Delta::Diff {
            timestamp: current.timestamp,
            blocks: diff_blocks,
        }
    }

    fn compute_processes_diff(
        &self,
        last: &[ProcessInfo],
        current: &[ProcessInfo],
    ) -> DataBlockDiff {
        let mut updates = Vec::new();
        let mut removals = Vec::new();

        let last_map: HashMap<u32, &ProcessInfo> = last.iter().map(|p| (p.pid, p)).collect();
        let current_map: HashMap<u32, &ProcessInfo> = current.iter().map(|p| (p.pid, p)).collect();

        for (pid, proc) in &current_map {
            if let Some(last_proc) = last_map.get(pid) {
                if *last_proc != *proc {
                    updates.push((*proc).clone());
                }
            } else {
                updates.push((*proc).clone());
            }
        }

        for pid in last_map.keys() {
            if !current_map.contains_key(pid) {
                removals.push(*pid);
            }
        }

        DataBlockDiff::Processes { updates, removals }
    }

    fn compute_pg_activity_diff(
        &self,
        last: &[PgStatActivityInfo],
        current: &[PgStatActivityInfo],
    ) -> DataBlockDiff {
        let mut updates = Vec::new();
        let mut removals = Vec::new();

        let last_map: HashMap<i32, &PgStatActivityInfo> = last.iter().map(|p| (p.pid, p)).collect();
        let current_map: HashMap<i32, &PgStatActivityInfo> =
            current.iter().map(|p| (p.pid, p)).collect();

        for (pid, proc) in &current_map {
            if let Some(last_proc) = last_map.get(pid) {
                if *last_proc != *proc {
                    updates.push((*proc).clone());
                }
            } else {
                updates.push((*proc).clone());
            }
        }

        for pid in last_map.keys() {
            if !current_map.contains_key(pid) {
                removals.push(*pid);
            }
        }

        DataBlockDiff::PgStatActivity { updates, removals }
    }

    fn compute_pg_statements_diff(
        &self,
        last: &[PgStatStatementsInfo],
        current: &[PgStatStatementsInfo],
    ) -> DataBlockDiff {
        let mut updates = Vec::new();
        let mut removals = Vec::new();

        let last_map: HashMap<i64, &PgStatStatementsInfo> =
            last.iter().map(|p| (p.queryid, p)).collect();
        let current_map: HashMap<i64, &PgStatStatementsInfo> =
            current.iter().map(|p| (p.queryid, p)).collect();

        for (id, proc) in &current_map {
            if let Some(last_proc) = last_map.get(id) {
                if *last_proc != *proc {
                    updates.push((*proc).clone());
                }
            } else {
                updates.push((*proc).clone());
            }
        }

        for id in last_map.keys() {
            if !current_map.contains_key(id) {
                removals.push(*id);
            }
        }

        DataBlockDiff::PgStatStatements { updates, removals }
    }

    fn compute_pg_database_diff(
        &self,
        last: &[PgStatDatabaseInfo],
        current: &[PgStatDatabaseInfo],
    ) -> DataBlockDiff {
        let mut updates = Vec::new();
        let mut removals = Vec::new();

        let last_map: HashMap<u32, &PgStatDatabaseInfo> =
            last.iter().map(|d| (d.datid, d)).collect();
        let current_map: HashMap<u32, &PgStatDatabaseInfo> =
            current.iter().map(|d| (d.datid, d)).collect();

        for (id, db) in &current_map {
            if let Some(last_db) = last_map.get(id) {
                if *last_db != *db {
                    updates.push((*db).clone());
                }
            } else {
                updates.push((*db).clone());
            }
        }

        for id in last_map.keys() {
            if !current_map.contains_key(id) {
                removals.push(*id);
            }
        }

        DataBlockDiff::PgStatDatabase { updates, removals }
    }

    fn compute_pg_tables_diff(
        &self,
        last: &[PgStatUserTablesInfo],
        current: &[PgStatUserTablesInfo],
    ) -> DataBlockDiff {
        let mut updates = Vec::new();
        let mut removals = Vec::new();

        let last_map: HashMap<u32, &PgStatUserTablesInfo> =
            last.iter().map(|t| (t.relid, t)).collect();
        let current_map: HashMap<u32, &PgStatUserTablesInfo> =
            current.iter().map(|t| (t.relid, t)).collect();

        for (id, table) in &current_map {
            if let Some(last_table) = last_map.get(id) {
                if *last_table != *table {
                    updates.push((*table).clone());
                }
            } else {
                updates.push((*table).clone());
            }
        }

        for id in last_map.keys() {
            if !current_map.contains_key(id) {
                removals.push(*id);
            }
        }

        DataBlockDiff::PgStatUserTables { updates, removals }
    }

    fn compute_pg_indexes_diff(
        &self,
        last: &[PgStatUserIndexesInfo],
        current: &[PgStatUserIndexesInfo],
    ) -> DataBlockDiff {
        let mut updates = Vec::new();
        let mut removals = Vec::new();

        let last_map: HashMap<u32, &PgStatUserIndexesInfo> =
            last.iter().map(|i| (i.indexrelid, i)).collect();
        let current_map: HashMap<u32, &PgStatUserIndexesInfo> =
            current.iter().map(|i| (i.indexrelid, i)).collect();

        for (id, index) in &current_map {
            if let Some(last_index) = last_map.get(id) {
                if *last_index != *index {
                    updates.push((*index).clone());
                }
            } else {
                updates.push((*index).clone());
            }
        }

        for id in last_map.keys() {
            if !current_map.contains_key(id) {
                removals.push(*id);
            }
        }

        DataBlockDiff::PgStatUserIndexes { updates, removals }
    }

    fn compute_pg_lock_tree_diff(
        &self,
        last: &[PgLockTreeNode],
        current: &[PgLockTreeNode],
    ) -> DataBlockDiff {
        let mut updates = Vec::new();
        let mut removals = Vec::new();

        let last_map: HashMap<i32, &PgLockTreeNode> = last.iter().map(|n| (n.pid, n)).collect();
        let current_map: HashMap<i32, &PgLockTreeNode> =
            current.iter().map(|n| (n.pid, n)).collect();

        for (pid, node) in &current_map {
            if let Some(last_node) = last_map.get(pid) {
                if *last_node != *node {
                    updates.push((*node).clone());
                }
            } else {
                updates.push((*node).clone());
            }
        }

        for pid in last_map.keys() {
            if !current_map.contains_key(pid) {
                removals.push(*pid);
            }
        }

        DataBlockDiff::PgLockTree { updates, removals }
    }

    fn compute_system_cpu_diff(
        &self,
        last: &[SystemCpuInfo],
        current: &[SystemCpuInfo],
    ) -> DataBlockDiff {
        let mut updates = Vec::new();
        let mut removals = Vec::new();

        let last_map: HashMap<i16, &SystemCpuInfo> = last.iter().map(|c| (c.cpu_id, c)).collect();
        let current_map: HashMap<i16, &SystemCpuInfo> =
            current.iter().map(|c| (c.cpu_id, c)).collect();

        for (id, cpu) in &current_map {
            if let Some(last_cpu) = last_map.get(id) {
                if *last_cpu != *cpu {
                    updates.push((*cpu).clone());
                }
            } else {
                updates.push((*cpu).clone());
            }
        }

        for id in last_map.keys() {
            if !current_map.contains_key(id) {
                removals.push(*id);
            }
        }

        DataBlockDiff::SystemCpu { updates, removals }
    }

    fn compute_system_net_diff(
        &self,
        last: &[SystemNetInfo],
        current: &[SystemNetInfo],
    ) -> DataBlockDiff {
        let mut updates = Vec::new();
        let mut removals = Vec::new();

        let last_map: HashMap<u64, &SystemNetInfo> =
            last.iter().map(|n| (n.name_hash, n)).collect();
        let current_map: HashMap<u64, &SystemNetInfo> =
            current.iter().map(|n| (n.name_hash, n)).collect();

        for (hash, net) in &current_map {
            if let Some(last_net) = last_map.get(hash) {
                if *last_net != *net {
                    updates.push((*net).clone());
                }
            } else {
                updates.push((*net).clone());
            }
        }

        for hash in last_map.keys() {
            if !current_map.contains_key(hash) {
                removals.push(*hash);
            }
        }

        DataBlockDiff::SystemNet { updates, removals }
    }

    fn compute_system_disk_diff(
        &self,
        last: &[SystemDiskInfo],
        current: &[SystemDiskInfo],
    ) -> DataBlockDiff {
        let mut updates = Vec::new();
        let mut removals = Vec::new();

        let last_map: HashMap<u64, &SystemDiskInfo> =
            last.iter().map(|d| (d.device_hash, d)).collect();
        let current_map: HashMap<u64, &SystemDiskInfo> =
            current.iter().map(|d| (d.device_hash, d)).collect();

        for (hash, disk) in &current_map {
            if let Some(last_disk) = last_map.get(hash) {
                if *last_disk != *disk {
                    updates.push((*disk).clone());
                }
            } else {
                updates.push((*disk).clone());
            }
        }

        for hash in last_map.keys() {
            if !current_map.contains_key(hash) {
                removals.push(*hash);
            }
        }

        DataBlockDiff::SystemDisk { updates, removals }
    }

    fn compute_system_interrupts_diff(
        &self,
        last: &[SystemInterruptInfo],
        current: &[SystemInterruptInfo],
    ) -> DataBlockDiff {
        let mut updates = Vec::new();
        let mut removals = Vec::new();

        let last_map: HashMap<u64, &SystemInterruptInfo> =
            last.iter().map(|i| (i.irq_hash, i)).collect();
        let current_map: HashMap<u64, &SystemInterruptInfo> =
            current.iter().map(|i| (i.irq_hash, i)).collect();

        for (hash, irq) in &current_map {
            if let Some(last_irq) = last_map.get(hash) {
                if *last_irq != *irq {
                    updates.push((*irq).clone());
                }
            } else {
                updates.push((*irq).clone());
            }
        }

        for hash in last_map.keys() {
            if !current_map.contains_key(hash) {
                removals.push(*hash);
            }
        }

        DataBlockDiff::SystemInterrupts { updates, removals }
    }

    fn compute_system_softirqs_diff(
        &self,
        last: &[SystemSoftirqInfo],
        current: &[SystemSoftirqInfo],
    ) -> DataBlockDiff {
        let mut updates = Vec::new();
        let mut removals = Vec::new();

        let last_map: HashMap<u64, &SystemSoftirqInfo> =
            last.iter().map(|s| (s.name_hash, s)).collect();
        let current_map: HashMap<u64, &SystemSoftirqInfo> =
            current.iter().map(|s| (s.name_hash, s)).collect();

        for (hash, softirq) in &current_map {
            if let Some(last_softirq) = last_map.get(hash) {
                if *last_softirq != *softirq {
                    updates.push((*softirq).clone());
                }
            } else {
                updates.push((*softirq).clone());
            }
        }

        for hash in last_map.keys() {
            if !current_map.contains_key(hash) {
                removals.push(*hash);
            }
        }

        DataBlockDiff::SystemSoftirqs { updates, removals }
    }

    /// Flushes the current chunk using current time for filename.
    pub fn flush_chunk(&mut self) -> std::io::Result<()> {
        let now = Utc::now();
        let date = self.current_date.unwrap_or_else(|| now.date_naive());
        let hour = self.current_hour.unwrap_or_else(|| now.hour());
        self.flush_chunk_with_time(date, hour)
    }

    /// Flushes WAL to a compressed chunk file.
    /// Reads all snapshots from WAL, computes deltas on-the-fly, and writes compressed chunk.
    /// File naming format: rpglot_YYYY-MM-DD_HH.zst
    fn flush_chunk_with_time(&mut self, date: NaiveDate, hour: u32) -> std::io::Result<()> {
        if self.wal_entries_count == 0 {
            return Err(std::io::Error::other("Empty WAL"));
        }

        // Read all snapshots from WAL and build chunk
        let (snapshots, interner) = self.load_wal_snapshots_with_interner()?;
        if snapshots.is_empty() {
            return Err(std::io::Error::other("No snapshots in WAL"));
        }

        // Build chunk with deltas computed on-the-fly
        let mut chunk = Chunk::new();
        let mut last_snapshot: Option<&Snapshot> = None;

        for snapshot in &snapshots {
            let delta = if let Some(last) = last_snapshot {
                self.compute_delta(last, snapshot)
            } else {
                Delta::Full(snapshot.clone())
            };
            chunk.deltas.push(delta);
            last_snapshot = Some(snapshot);
        }

        // Build optimized interner with only used hashes
        let used_hashes = chunk.collect_used_hashes();
        chunk.interner = interner.filter(&used_hashes);

        let compressed = chunk.compress()?;
        let filename = format!("rpglot_{}_{:02}.zst", date.format("%Y-%m-%d"), hour);
        let final_path = self.base_path.join(&filename);

        // If file already exists, append timestamp to make it unique
        let final_path = if final_path.exists() {
            let filename = format!(
                "rpglot_{}_{:02}_{}.zst",
                date.format("%Y-%m-%d"),
                hour,
                Utc::now().timestamp_nanos_opt().unwrap_or(0)
            );
            self.base_path.join(filename)
        } else {
            final_path
        };

        let tmp_path = final_path.with_extension("tmp");

        {
            let mut f = File::create(&tmp_path)?;
            f.write_all(&compressed)?;
            f.sync_all()?;
        }

        std::fs::rename(tmp_path, &final_path)?;

        // Truncate WAL
        self.wal_file.set_len(0)?;
        self.wal_file.sync_all()?;

        // Reset WAL entry count
        self.wal_entries_count = 0;

        Ok(())
    }

    /// Loads unflushed snapshots and their interners from WAL file.
    pub fn load_wal_snapshots(&self) -> std::io::Result<(Vec<Snapshot>, StringInterner)> {
        self.load_wal_snapshots_with_interner()
    }

    /// Loads unflushed snapshots and their interners from WAL file (internal).
    fn load_wal_snapshots_with_interner(&self) -> std::io::Result<(Vec<Snapshot>, StringInterner)> {
        let wal_path = self.base_path.join("wal.log");
        let mut snapshots = Vec::new();
        let mut merged_interner = StringInterner::new();

        if let Ok(data) = std::fs::read(&wal_path)
            && !data.is_empty()
        {
            let mut cursor = std::io::Cursor::new(&data);
            while let Ok(entry) = bincode::deserialize_from::<_, WalEntry>(&mut cursor) {
                merged_interner.merge(&entry.interner);
                snapshots.push(entry.snapshot);
            }
        }

        Ok((snapshots, merged_interner))
    }

    /// Scans WAL file and returns entry metadata (byte_offset, byte_length, timestamp)
    /// for each entry, plus a merged interner. Snapshots are deserialized to extract
    /// timestamps but immediately dropped — peak RAM = one snapshot at a time.
    pub fn scan_wal_metadata(
        wal_path: &Path,
    ) -> std::io::Result<(Vec<(u64, u64, i64)>, StringInterner)> {
        let data = match std::fs::read(wal_path) {
            Ok(d) if !d.is_empty() => d,
            Ok(_) => return Ok((Vec::new(), StringInterner::new())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Vec::new(), StringInterner::new()));
            }
            Err(e) => return Err(e),
        };

        let mut entries = Vec::new();
        let mut merged = StringInterner::new();
        let mut cursor = std::io::Cursor::new(&data);

        while (cursor.position() as usize) < data.len() {
            let start = cursor.position();
            match bincode::deserialize_from::<_, WalEntry>(&mut cursor) {
                Ok(entry) => {
                    let end = cursor.position();
                    let ts = entry.snapshot.timestamp;
                    merged.merge(&entry.interner);
                    entries.push((start, end - start, ts));
                    // entry.snapshot dropped here — RAM freed
                }
                Err(_) => break,
            }
        }

        Ok((entries, merged))
    }

    /// Loads a single snapshot from WAL at the given byte range.
    /// Reads the WAL file, extracts the entry at [offset..offset+length], deserializes it.
    pub fn load_wal_snapshot_at(
        wal_path: &Path,
        offset: u64,
        length: u64,
    ) -> std::io::Result<Snapshot> {
        let data = std::fs::read(wal_path)?;
        let start = offset as usize;
        let end = (offset + length) as usize;
        if end > data.len() {
            return Err(std::io::Error::other("WAL offset out of bounds"));
        }
        let entry: WalEntry =
            bincode::deserialize(&data[start..end]).map_err(std::io::Error::other)?;
        Ok(entry.snapshot)
    }

    /// Loads all chunks from the storage directory and returns reconstructed snapshots
    /// along with a merged StringInterner containing all strings from all chunks.
    ///
    /// Also loads unflushed snapshots from WAL and their interners.
    /// Snapshots are returned in chronological order (oldest first).
    pub fn load_all_snapshots_with_interner(
        &self,
    ) -> std::io::Result<(Vec<Snapshot>, StringInterner)> {
        let mut chunks = self.load_chunks()?;

        // Sort by first timestamp in each chunk
        chunks.sort_by_key(|c| c.deltas.first().map(|d| d.timestamp()).unwrap_or(0));

        let mut snapshots = Vec::new();
        let mut merged_interner = StringInterner::new();

        for chunk in chunks {
            // Merge chunk's interner into the merged one
            merged_interner.merge(&chunk.interner);

            let chunk_snapshots = Self::reconstruct_snapshots_from_chunk(&chunk)?;
            snapshots.extend(chunk_snapshots);
        }

        // Load unflushed snapshots and interners from WAL
        let (wal_snapshots, wal_interner) = self.load_wal_snapshots_with_interner()?;
        merged_interner.merge(&wal_interner);
        snapshots.extend(wal_snapshots);

        // Sort and deduplicate
        snapshots.sort_by_key(|s| s.timestamp);
        snapshots.dedup_by_key(|s| s.timestamp);

        Ok((snapshots, merged_interner))
    }

    /// Loads all chunk files from the storage directory.
    fn load_chunks(&self) -> std::io::Result<Vec<Chunk>> {
        let mut chunks = Vec::new();

        for entry in std::fs::read_dir(&self.base_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "zst") {
                let data = std::fs::read(&path)?;
                let chunk = Chunk::decompress(&data)?;
                chunks.push(chunk);
            }
        }

        Ok(chunks)
    }

    /// Reconstructs full snapshots from a chunk's deltas.
    pub fn reconstruct_snapshots_from_chunk(chunk: &Chunk) -> std::io::Result<Vec<Snapshot>> {
        let mut snapshots = Vec::new();
        let mut last_snapshot: Option<Snapshot> = None;

        for delta in &chunk.deltas {
            match delta {
                Delta::Full(snapshot) => {
                    snapshots.push(snapshot.clone());
                    last_snapshot = Some(snapshot.clone());
                }
                Delta::Diff { timestamp, blocks } => {
                    let base = last_snapshot.as_ref().ok_or_else(|| {
                        std::io::Error::other("Diff without preceding Full snapshot")
                    })?;
                    let snapshot = Self::apply_diff(base, *timestamp, blocks);
                    snapshots.push(snapshot.clone());
                    last_snapshot = Some(snapshot);
                }
            }
        }

        Ok(snapshots)
    }

    /// Applies a diff to a base snapshot to produce a new snapshot.
    fn apply_diff(base: &Snapshot, timestamp: i64, diff_blocks: &[DataBlockDiff]) -> Snapshot {
        let mut new_blocks = base.blocks.clone();

        for diff in diff_blocks {
            match diff {
                DataBlockDiff::Processes { updates, removals } => {
                    if let Some(DataBlock::Processes(procs)) = new_blocks
                        .iter_mut()
                        .find(|b| matches!(b, DataBlock::Processes(_)))
                    {
                        procs.retain(|p| !removals.contains(&p.pid));
                        for update in updates {
                            if let Some(existing) = procs.iter_mut().find(|p| p.pid == update.pid) {
                                *existing = update.clone();
                            } else {
                                procs.push(update.clone());
                            }
                        }
                    }
                }
                DataBlockDiff::SystemLoad(load) => {
                    if let Some(block) = new_blocks
                        .iter_mut()
                        .find(|b| matches!(b, DataBlock::SystemLoad(_)))
                    {
                        *block = DataBlock::SystemLoad(load.clone());
                    }
                }
                DataBlockDiff::SystemMem(mem) => {
                    if let Some(block) = new_blocks
                        .iter_mut()
                        .find(|b| matches!(b, DataBlock::SystemMem(_)))
                    {
                        *block = DataBlock::SystemMem(mem.clone());
                    }
                }
                DataBlockDiff::SystemCpu { updates, removals } => {
                    if let Some(DataBlock::SystemCpu(cpus)) = new_blocks
                        .iter_mut()
                        .find(|b| matches!(b, DataBlock::SystemCpu(_)))
                    {
                        cpus.retain(|c| !removals.contains(&c.cpu_id));
                        for update in updates {
                            if let Some(existing) =
                                cpus.iter_mut().find(|c| c.cpu_id == update.cpu_id)
                            {
                                *existing = update.clone();
                            } else {
                                cpus.push(update.clone());
                            }
                        }
                    }
                }
                DataBlockDiff::SystemNet { updates, removals } => {
                    if let Some(DataBlock::SystemNet(nets)) = new_blocks
                        .iter_mut()
                        .find(|b| matches!(b, DataBlock::SystemNet(_)))
                    {
                        nets.retain(|n| !removals.contains(&n.name_hash));
                        for update in updates {
                            if let Some(existing) =
                                nets.iter_mut().find(|n| n.name_hash == update.name_hash)
                            {
                                *existing = update.clone();
                            } else {
                                nets.push(update.clone());
                            }
                        }
                    }
                }
                DataBlockDiff::SystemDisk { updates, removals } => {
                    if let Some(DataBlock::SystemDisk(disks)) = new_blocks
                        .iter_mut()
                        .find(|b| matches!(b, DataBlock::SystemDisk(_)))
                    {
                        disks.retain(|d| !removals.contains(&d.device_hash));
                        for update in updates {
                            if let Some(existing) = disks
                                .iter_mut()
                                .find(|d| d.device_hash == update.device_hash)
                            {
                                *existing = update.clone();
                            } else {
                                disks.push(update.clone());
                            }
                        }
                    }
                }
                DataBlockDiff::SystemInterrupts { updates, removals } => {
                    if let Some(DataBlock::SystemInterrupts(irqs)) = new_blocks
                        .iter_mut()
                        .find(|b| matches!(b, DataBlock::SystemInterrupts(_)))
                    {
                        irqs.retain(|i| !removals.contains(&i.irq_hash));
                        for update in updates {
                            if let Some(existing) =
                                irqs.iter_mut().find(|i| i.irq_hash == update.irq_hash)
                            {
                                *existing = update.clone();
                            } else {
                                irqs.push(update.clone());
                            }
                        }
                    }
                }
                DataBlockDiff::SystemSoftirqs { updates, removals } => {
                    if let Some(DataBlock::SystemSoftirqs(sirqs)) = new_blocks
                        .iter_mut()
                        .find(|b| matches!(b, DataBlock::SystemSoftirqs(_)))
                    {
                        sirqs.retain(|s| !removals.contains(&s.name_hash));
                        for update in updates {
                            if let Some(existing) =
                                sirqs.iter_mut().find(|s| s.name_hash == update.name_hash)
                            {
                                *existing = update.clone();
                            } else {
                                sirqs.push(update.clone());
                            }
                        }
                    }
                }
                DataBlockDiff::PgStatActivity { updates, removals } => {
                    if let Some(DataBlock::PgStatActivity(activities)) = new_blocks
                        .iter_mut()
                        .find(|b| matches!(b, DataBlock::PgStatActivity(_)))
                    {
                        activities.retain(|a| !removals.contains(&a.pid));
                        for update in updates {
                            if let Some(existing) =
                                activities.iter_mut().find(|a| a.pid == update.pid)
                            {
                                *existing = update.clone();
                            } else {
                                activities.push(update.clone());
                            }
                        }
                    }
                }
                DataBlockDiff::PgStatStatements { updates, removals } => {
                    if let Some(DataBlock::PgStatStatements(stmts)) = new_blocks
                        .iter_mut()
                        .find(|b| matches!(b, DataBlock::PgStatStatements(_)))
                    {
                        stmts.retain(|s| !removals.contains(&s.queryid));
                        for update in updates {
                            if let Some(existing) =
                                stmts.iter_mut().find(|s| s.queryid == update.queryid)
                            {
                                *existing = update.clone();
                            } else {
                                stmts.push(update.clone());
                            }
                        }
                    }
                }
                DataBlockDiff::PgStatDatabase { updates, removals } => {
                    if let Some(DataBlock::PgStatDatabase(dbs)) = new_blocks
                        .iter_mut()
                        .find(|b| matches!(b, DataBlock::PgStatDatabase(_)))
                    {
                        dbs.retain(|d| !removals.contains(&d.datid));
                        for update in updates {
                            if let Some(existing) = dbs.iter_mut().find(|d| d.datid == update.datid)
                            {
                                *existing = update.clone();
                            } else {
                                dbs.push(update.clone());
                            }
                        }
                    }
                }
                DataBlockDiff::PgStatUserTables { updates, removals } => {
                    if let Some(DataBlock::PgStatUserTables(tables)) = new_blocks
                        .iter_mut()
                        .find(|b| matches!(b, DataBlock::PgStatUserTables(_)))
                    {
                        tables.retain(|t| !removals.contains(&t.relid));
                        for update in updates {
                            if let Some(existing) =
                                tables.iter_mut().find(|t| t.relid == update.relid)
                            {
                                *existing = update.clone();
                            } else {
                                tables.push(update.clone());
                            }
                        }
                    }
                }
                DataBlockDiff::PgStatUserIndexes { updates, removals } => {
                    if let Some(DataBlock::PgStatUserIndexes(indexes)) = new_blocks
                        .iter_mut()
                        .find(|b| matches!(b, DataBlock::PgStatUserIndexes(_)))
                    {
                        indexes.retain(|i| !removals.contains(&i.indexrelid));
                        for update in updates {
                            if let Some(existing) = indexes
                                .iter_mut()
                                .find(|i| i.indexrelid == update.indexrelid)
                            {
                                *existing = update.clone();
                            } else {
                                indexes.push(update.clone());
                            }
                        }
                    }
                }
                DataBlockDiff::PgLockTree { updates, removals } => {
                    if let Some(DataBlock::PgLockTree(nodes)) = new_blocks
                        .iter_mut()
                        .find(|b| matches!(b, DataBlock::PgLockTree(_)))
                    {
                        nodes.retain(|n| !removals.contains(&n.pid));
                        for update in updates {
                            if let Some(existing) = nodes.iter_mut().find(|n| n.pid == update.pid) {
                                *existing = update.clone();
                            } else {
                                nodes.push(update.clone());
                            }
                        }
                    } else if !updates.is_empty() {
                        new_blocks.push(DataBlock::PgLockTree(updates.clone()));
                    }
                }
                DataBlockDiff::SystemPsi(psi) => {
                    if let Some(block) = new_blocks
                        .iter_mut()
                        .find(|b| matches!(b, DataBlock::SystemPsi(_)))
                    {
                        *block = DataBlock::SystemPsi(psi.clone());
                    }
                }
                DataBlockDiff::SystemVmstat(vmstat) => {
                    if let Some(block) = new_blocks
                        .iter_mut()
                        .find(|b| matches!(b, DataBlock::SystemVmstat(_)))
                    {
                        *block = DataBlock::SystemVmstat(vmstat.clone());
                    }
                }
                DataBlockDiff::PgStatBgwriter(bgw) => {
                    if let Some(block) = new_blocks
                        .iter_mut()
                        .find(|b| matches!(b, DataBlock::PgStatBgwriter(_)))
                    {
                        *block = DataBlock::PgStatBgwriter(bgw.clone());
                    }
                }
                DataBlockDiff::SystemFile(file) => {
                    if let Some(block) = new_blocks
                        .iter_mut()
                        .find(|b| matches!(b, DataBlock::SystemFile(_)))
                    {
                        *block = DataBlock::SystemFile(file.clone());
                    }
                }
                DataBlockDiff::SystemStat(stat) => {
                    if let Some(block) = new_blocks
                        .iter_mut()
                        .find(|b| matches!(b, DataBlock::SystemStat(_)))
                    {
                        *block = DataBlock::SystemStat(stat.clone());
                    }
                }
                DataBlockDiff::SystemNetSnmp(snmp) => {
                    if let Some(block) = new_blocks
                        .iter_mut()
                        .find(|b| matches!(b, DataBlock::SystemNetSnmp(_)))
                    {
                        *block = DataBlock::SystemNetSnmp(snmp.clone());
                    }
                }
                DataBlockDiff::Cgroup(cgroup) => {
                    if let Some(block) = new_blocks
                        .iter_mut()
                        .find(|b| matches!(b, DataBlock::Cgroup(_)))
                    {
                        *block = DataBlock::Cgroup(cgroup.clone());
                    }
                }
            }
        }

        Snapshot {
            timestamp,
            blocks: new_blocks,
        }
    }

    /// Returns the number of snapshots in the WAL (unflushed).
    pub fn current_chunk_size(&self) -> usize {
        self.wal_entries_count
    }

    /// Rotates data files according to the given configuration.
    ///
    /// Removes files based on two criteria:
    /// 1. Files older than `max_retention_days`
    /// 2. Oldest files if total size exceeds `max_total_size`
    pub fn rotate(&self, config: &RotationConfig) -> std::io::Result<RotationResult> {
        let mut result = RotationResult::default();

        // Collect all .zst files with their metadata
        let mut files: Vec<FileInfo> = Vec::new();

        for entry in std::fs::read_dir(&self.base_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "zst") {
                let metadata = entry.metadata()?;
                let size = metadata.len();

                // Extract date from filename (rpglot_YYYY-MM-DD_HH.zst)
                let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                let date = Self::parse_date_from_filename(filename);

                files.push(FileInfo { path, size, date });
            }
        }

        // Sort by date (oldest first)
        files.sort_by_key(|f| f.date);

        let now = Utc::now().date_naive();
        let retention_limit = now - chrono::Duration::days(config.max_retention_days as i64);

        // Remove files older than retention limit
        let mut remaining_files: Vec<FileInfo> = Vec::new();
        for file in files {
            if let Some(file_date) = file.date
                && file_date < retention_limit
            {
                std::fs::remove_file(&file.path)?;
                result.files_removed_by_age += 1;
                result.bytes_freed += file.size;
                continue;
            }
            remaining_files.push(file);
        }

        // Calculate total size of remaining files
        let mut total_size: u64 = remaining_files.iter().map(|f| f.size).sum();

        // Remove oldest files if total size exceeds limit
        while total_size > config.max_total_size && !remaining_files.is_empty() {
            let file = remaining_files.remove(0);
            std::fs::remove_file(&file.path)?;
            result.files_removed_by_size += 1;
            result.bytes_freed += file.size;
            total_size -= file.size;
        }

        result.total_size_after = total_size;
        result.files_remaining = remaining_files.len();

        Ok(result)
    }

    /// Parses date from filename format: rpglot_YYYY-MM-DD_HH.zst or chunk_*.zst
    fn parse_date_from_filename(filename: &str) -> Option<NaiveDate> {
        // Try new format: rpglot_YYYY-MM-DD_HH.zst
        if filename.starts_with("rpglot_") {
            let parts: Vec<&str> = filename
                .strip_prefix("rpglot_")?
                .strip_suffix(".zst")?
                .split('_')
                .collect();

            if !parts.is_empty() {
                return NaiveDate::parse_from_str(parts[0], "%Y-%m-%d").ok();
            }
        }

        // For old format (chunk_*), use file creation time via metadata
        // This is handled separately in rotate() if needed
        None
    }

    /// Returns the base path of the storage.
    pub fn base_path(&self) -> &PathBuf {
        &self.base_path
    }
}

/// Information about a data file for rotation.
struct FileInfo {
    path: PathBuf,
    size: u64,
    date: Option<NaiveDate>,
}

/// Result of a rotation operation.
#[derive(Debug, Default)]
pub struct RotationResult {
    /// Number of files removed due to age (older than max_retention_days).
    pub files_removed_by_age: usize,
    /// Number of files removed due to size limit.
    pub files_removed_by_size: usize,
    /// Total bytes freed by rotation.
    pub bytes_freed: u64,
    /// Total size of remaining files after rotation.
    pub total_size_after: u64,
    /// Number of files remaining after rotation.
    pub files_remaining: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::model::ProcessInfo;
    use tempfile::tempdir;

    #[test]
    fn test_storage_manager_delta_efficiency() {
        let dir = tempdir().unwrap();
        let mut manager = StorageManager::new(dir.path());
        manager.chunk_size_limit = 2;

        let s1 = Snapshot {
            timestamp: 100,
            blocks: vec![DataBlock::Processes(vec![ProcessInfo {
                pid: 1,
                name_hash: 1,
                cmdline_hash: 1,
                ..ProcessInfo::default()
            }])],
        };

        // Same snapshot, should result in a small Delta::Diff
        let s2 = Snapshot {
            timestamp: 110,
            blocks: s1.blocks.clone(),
        };

        manager.add_snapshot(s1, &StringInterner::new());
        manager.add_snapshot(s2, &StringInterner::new());

        // At this point it should have flushed
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "zst"))
            .collect();
        assert!(!entries.is_empty());

        // Check content of the chunk
        let chunk_path = entries[0].path();
        let data = std::fs::read(chunk_path).unwrap();
        let chunk = Chunk::decompress(&data).unwrap();

        assert_eq!(chunk.deltas.len(), 2);
        assert!(matches!(chunk.deltas[0], Delta::Full(_)));
        assert!(matches!(chunk.deltas[1], Delta::Diff { .. }));

        if let Delta::Diff { blocks, .. } = &chunk.deltas[1] {
            if let DataBlockDiff::Processes { updates, removals } = &blocks[0] {
                assert!(updates.is_empty());
                assert!(removals.is_empty());
            } else {
                panic!("Expected DataBlockDiff::Processes");
            }
        }
    }

    #[test]
    fn test_storage_manager_wal_recovery() {
        let dir = tempdir().unwrap();
        let s1 = Snapshot {
            timestamp: 100,
            blocks: vec![DataBlock::Processes(vec![ProcessInfo {
                pid: 1,
                name_hash: 1,
                cmdline_hash: 1,
                ..ProcessInfo::default()
            }])],
        };

        {
            let mut manager = StorageManager::new(dir.path());
            manager.add_snapshot(s1.clone(), &StringInterner::new());
            // Drop manager without flushing chunk to disk (simulated crash)
        }

        // New manager should recover WAL entry count
        let manager = StorageManager::new(dir.path());
        assert_eq!(manager.current_chunk_size(), 1);

        // Verify snapshot can be loaded from WAL
        let (snapshots, _) = manager.load_all_snapshots_with_interner().unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].timestamp, 100);
    }

    #[test]
    fn test_storage_manager_pg_delta_efficiency() {
        let dir = tempdir().unwrap();
        let mut manager = StorageManager::new(dir.path());
        manager.chunk_size_limit = 2;

        let s1 = Snapshot {
            timestamp: 100,
            blocks: vec![DataBlock::PgStatActivity(vec![PgStatActivityInfo {
                pid: 1234,
                query_hash: 555,
                ..PgStatActivityInfo::default()
            }])],
        };

        // Snapshot with query changed
        let s2 = Snapshot {
            timestamp: 110,
            blocks: vec![DataBlock::PgStatActivity(vec![PgStatActivityInfo {
                pid: 1234,
                query_hash: 666,
                ..PgStatActivityInfo::default()
            }])],
        };

        manager.add_snapshot(s1, &StringInterner::new());
        manager.add_snapshot(s2, &StringInterner::new());

        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "zst"))
            .collect();
        assert!(!entries.is_empty());

        let data = std::fs::read(entries[0].path()).unwrap();
        let chunk = Chunk::decompress(&data).unwrap();

        assert_eq!(chunk.deltas.len(), 2);
        if let Delta::Diff { blocks, .. } = &chunk.deltas[1] {
            if let DataBlockDiff::PgStatActivity { updates, removals } = &blocks[0] {
                assert_eq!(updates.len(), 1);
                assert_eq!(updates[0].query_hash, 666);
                assert!(removals.is_empty());
            } else {
                panic!("Expected DataBlockDiff::PgStatActivity");
            }
        } else {
            panic!("Expected Delta::Diff");
        }
    }

    #[test]
    fn test_storage_manager_system_delta_efficiency() {
        let dir = tempdir().unwrap();
        let mut manager = StorageManager::new(dir.path());
        manager.chunk_size_limit = 2;

        let s1 = Snapshot {
            timestamp: 100,
            blocks: vec![
                DataBlock::SystemCpu(vec![SystemCpuInfo {
                    cpu_id: -1,
                    user: 100,
                    ..SystemCpuInfo::default()
                }]),
                DataBlock::SystemLoad(SystemLoadInfo {
                    lavg1: 0.1,
                    ..SystemLoadInfo::default()
                }),
            ],
        };

        let s2 = Snapshot {
            timestamp: 110,
            blocks: vec![
                DataBlock::SystemCpu(vec![SystemCpuInfo {
                    cpu_id: -1,
                    user: 110, // changed
                    ..SystemCpuInfo::default()
                }]),
                DataBlock::SystemLoad(SystemLoadInfo {
                    lavg1: 0.2, // changed
                    ..SystemLoadInfo::default()
                }),
            ],
        };

        manager.add_snapshot(s1, &StringInterner::new());
        manager.add_snapshot(s2, &StringInterner::new());

        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "zst"))
            .collect();
        let data = std::fs::read(entries[0].path()).unwrap();
        let chunk = Chunk::decompress(&data).unwrap();

        assert_eq!(chunk.deltas.len(), 2);
        if let Delta::Diff { blocks, .. } = &chunk.deltas[1] {
            assert_eq!(blocks.len(), 2);
            if let DataBlockDiff::SystemCpu { updates, .. } = &blocks[0] {
                assert_eq!(updates.len(), 1);
                assert_eq!(updates[0].user, 110);
            } else {
                panic!("Expected SystemCpu diff");
            }
            if let DataBlockDiff::SystemLoad(load) = &blocks[1] {
                assert_eq!(load.lavg1, 0.2);
            } else {
                panic!("Expected SystemLoad diff");
            }
        } else {
            panic!("Expected Delta::Diff");
        }
    }

    #[test]
    fn test_rotation_by_days() {
        let dir = tempdir().unwrap();
        let manager = StorageManager::new(dir.path());

        // Create test files with dates
        let now = Utc::now().date_naive();
        let old_date = now - chrono::Duration::days(10);
        let recent_date = now - chrono::Duration::days(3);

        // Create old file (should be deleted)
        let old_file = dir
            .path()
            .join(format!("rpglot_{}_12.zst", old_date.format("%Y-%m-%d")));
        std::fs::write(&old_file, b"old data").unwrap();

        // Create recent file (should be kept)
        let recent_file = dir
            .path()
            .join(format!("rpglot_{}_12.zst", recent_date.format("%Y-%m-%d")));
        std::fs::write(&recent_file, b"recent data").unwrap();

        let config = RotationConfig::new(1_000_000_000, 7); // 1GB, 7 days
        let result = manager.rotate(&config).unwrap();

        assert_eq!(result.files_removed_by_age, 1);
        assert_eq!(result.files_remaining, 1);
        assert!(!old_file.exists());
        assert!(recent_file.exists());
    }

    #[test]
    fn test_rotation_by_size() {
        let dir = tempdir().unwrap();
        let manager = StorageManager::new(dir.path());

        let now = Utc::now().date_naive();
        let day1 = now - chrono::Duration::days(3);
        let day2 = now - chrono::Duration::days(2);
        let day3 = now - chrono::Duration::days(1);

        // Create files totaling more than the limit (different dates for predictable sort order)
        let file1 = dir
            .path()
            .join(format!("rpglot_{}_10.zst", day1.format("%Y-%m-%d")));
        let file2 = dir
            .path()
            .join(format!("rpglot_{}_10.zst", day2.format("%Y-%m-%d")));
        let file3 = dir
            .path()
            .join(format!("rpglot_{}_10.zst", day3.format("%Y-%m-%d")));

        std::fs::write(&file1, vec![0u8; 500]).unwrap(); // 500 bytes, oldest
        std::fs::write(&file2, vec![0u8; 500]).unwrap(); // 500 bytes
        std::fs::write(&file3, vec![0u8; 500]).unwrap(); // 500 bytes, newest

        // Set max size to 1000 bytes (should keep only 2 files)
        let config = RotationConfig::new(1000, 365);
        let result = manager.rotate(&config).unwrap();

        assert_eq!(result.files_removed_by_size, 1);
        assert_eq!(result.files_remaining, 2);
        assert!(!file1.exists()); // oldest removed
        assert!(file2.exists());
        assert!(file3.exists());
    }

    #[test]
    fn test_hourly_file_naming() {
        let dir = tempdir().unwrap();
        let mut manager = StorageManager::new(dir.path());
        manager.chunk_size_limit = 100; // Large limit to prevent auto-flush by count

        let s1 = Snapshot {
            timestamp: 100,
            blocks: vec![DataBlock::Processes(vec![ProcessInfo {
                pid: 1,
                name_hash: 1,
                cmdline_hash: 1,
                ..ProcessInfo::default()
            }])],
        };

        manager.add_snapshot(s1, &StringInterner::new());
        manager.flush_chunk().unwrap();

        // Check that file was created with the new naming format
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.starts_with("rpglot_") && name.ends_with(".zst")
            })
            .collect();

        assert_eq!(entries.len(), 1);
        let filename = entries[0].file_name().to_string_lossy().to_string();
        assert!(filename.starts_with("rpglot_"));
        assert!(filename.contains("_")); // Contains date and hour separator
    }

    #[test]
    fn test_parse_date_from_filename() {
        // Test new format
        let date = StorageManager::parse_date_from_filename("rpglot_2026-02-07_17.zst");
        assert!(date.is_some());
        assert_eq!(date.unwrap().to_string(), "2026-02-07");

        // Test with collision suffix
        let date = StorageManager::parse_date_from_filename("rpglot_2026-02-07_17_123456789.zst");
        assert!(date.is_some());
        assert_eq!(date.unwrap().to_string(), "2026-02-07");

        // Test old format (returns None)
        let date = StorageManager::parse_date_from_filename("chunk_1234567890.zst");
        assert!(date.is_none());
    }

    #[test]
    fn test_rotation_config_default() {
        let config = RotationConfig::default();
        assert_eq!(config.max_total_size, 1_073_741_824); // 1GB
        assert_eq!(config.max_retention_days, 7);
    }

    #[test]
    fn test_load_all_snapshots_includes_wal() {
        let dir = tempdir().unwrap();
        let mut manager = StorageManager::new(dir.path());
        manager.chunk_size_limit = 100; // Large limit to prevent auto-flush

        // Add snapshots that will stay in WAL (not flushed)
        let s1 = Snapshot {
            timestamp: 100,
            blocks: vec![DataBlock::SystemLoad(SystemLoadInfo {
                lavg1: 1.0,
                ..SystemLoadInfo::default()
            })],
        };
        let s2 = Snapshot {
            timestamp: 200,
            blocks: vec![DataBlock::SystemLoad(SystemLoadInfo {
                lavg1: 2.0,
                ..SystemLoadInfo::default()
            })],
        };

        manager.add_snapshot(s1, &StringInterner::new());
        manager.add_snapshot(s2, &StringInterner::new());

        // DO NOT flush - snapshots should be in WAL only
        // Verify WAL file exists and has content
        let wal_path = dir.path().join("wal.log");
        assert!(wal_path.exists());
        let wal_size = std::fs::metadata(&wal_path).unwrap().len();
        assert!(wal_size > 0);

        // Create a new manager to read (simulates rpglot -r)
        let reader = StorageManager::new(dir.path());
        let (snapshots, _) = reader.load_all_snapshots_with_interner().unwrap();

        // Should read 2 snapshots from WAL
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].timestamp, 100);
        assert_eq!(snapshots[1].timestamp, 200);
    }

    #[test]
    fn test_load_all_snapshots_combines_chunks_and_wal() {
        let dir = tempdir().unwrap();
        let mut manager = StorageManager::new(dir.path());
        manager.chunk_size_limit = 2; // Small limit to trigger flush

        // Add 2 snapshots (will be flushed to chunk)
        let s1 = Snapshot {
            timestamp: 100,
            blocks: vec![DataBlock::SystemLoad(SystemLoadInfo {
                lavg1: 1.0,
                ..SystemLoadInfo::default()
            })],
        };
        let s2 = Snapshot {
            timestamp: 200,
            blocks: vec![DataBlock::SystemLoad(SystemLoadInfo {
                lavg1: 2.0,
                ..SystemLoadInfo::default()
            })],
        };

        manager.add_snapshot(s1, &StringInterner::new());
        manager.add_snapshot(s2, &StringInterner::new());
        // After 2 snapshots, chunk_size_limit=2 triggers flush

        // Add one more snapshot (stays in WAL)
        let s3 = Snapshot {
            timestamp: 300,
            blocks: vec![DataBlock::SystemLoad(SystemLoadInfo {
                lavg1: 3.0,
                ..SystemLoadInfo::default()
            })],
        };
        manager.add_snapshot(s3, &StringInterner::new());

        // Verify: should have 1 chunk file + WAL with 1 snapshot
        let chunk_files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "zst"))
            .collect();
        assert_eq!(chunk_files.len(), 1);

        // Create a new manager to read
        let reader = StorageManager::new(dir.path());
        let (snapshots, _) = reader.load_all_snapshots_with_interner().unwrap();

        // Should read all 3 snapshots (2 from chunk + 1 from WAL)
        assert_eq!(snapshots.len(), 3);
        assert_eq!(snapshots[0].timestamp, 100);
        assert_eq!(snapshots[1].timestamp, 200);
        assert_eq!(snapshots[2].timestamp, 300);
    }
}
