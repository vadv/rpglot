//! Process collector for gathering per-process metrics from `/proc/[pid]/`.

use crate::collector::procfs::parser::{parse_proc_io, parse_proc_stat, parse_proc_status};
use crate::collector::traits::FileSystem;
use crate::storage::interner::StringInterner;
use crate::storage::model::{ProcessCpuInfo, ProcessDskInfo, ProcessInfo, ProcessMemInfo};
use std::path::Path;

/// Clock ticks per second (USER_HZ). Standard value for Linux.
const CLK_TCK: u64 = 100;

/// Error type for collection failures.
#[derive(Debug)]
pub enum CollectError {
    /// Process disappeared during collection.
    ProcessGone(u32),
    /// I/O error reading process files.
    Io(std::io::Error),
    /// Parse error in process files.
    Parse(String),
}

impl std::fmt::Display for CollectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CollectError::ProcessGone(pid) => write!(f, "process {} disappeared", pid),
            CollectError::Io(e) => write!(f, "I/O error: {}", e),
            CollectError::Parse(msg) => write!(f, "parse error: {}", msg),
        }
    }
}

impl std::error::Error for CollectError {}

impl From<std::io::Error> for CollectError {
    fn from(e: std::io::Error) -> Self {
        CollectError::Io(e)
    }
}

/// Collects process information from `/proc/[pid]/` files.
pub struct ProcessCollector<F: FileSystem> {
    fs: F,
    interner: StringInterner,
    proc_path: String,
    page_size: u64,
    /// System boot time (seconds since epoch), used to calculate process start time.
    boot_time: u64,
}

impl<F: FileSystem> ProcessCollector<F> {
    /// Creates a new process collector.
    ///
    /// # Arguments
    /// * `fs` - Filesystem implementation (real or mock)
    /// * `proc_path` - Base path to proc filesystem (usually "/proc")
    pub fn new(fs: F, proc_path: impl Into<String>) -> Self {
        Self {
            fs,
            interner: StringInterner::new(),
            proc_path: proc_path.into(),
            page_size: 4096, // Default page size, could be detected
            boot_time: 0,
        }
    }

    /// Sets the system boot time for calculating process start times.
    ///
    /// Must be called before `collect_process()` or `collect_all_processes()`
    /// to properly calculate `ProcessInfo.btime`.
    ///
    /// # Arguments
    /// * `boot_time` - System boot time in seconds since epoch (from `/proc/stat` btime)
    pub fn set_boot_time(&mut self, boot_time: u64) {
        self.boot_time = boot_time;
    }

    /// Calculates process start time in seconds since epoch.
    ///
    /// Formula: boot_time + (starttime_jiffies / CLK_TCK)
    ///
    /// Returns 0 if boot_time is not set.
    fn calculate_process_start_time(&self, starttime_jiffies: u64) -> u32 {
        if self.boot_time == 0 {
            return 0;
        }
        (self.boot_time + starttime_jiffies / CLK_TCK) as u32
    }

    /// Returns a reference to the string interner.
    pub fn interner(&self) -> &StringInterner {
        &self.interner
    }

    /// Returns a mutable reference to the string interner.
    pub fn interner_mut(&mut self) -> &mut StringInterner {
        &mut self.interner
    }

    /// Clears the string interner, freeing memory.
    pub fn clear_interner(&mut self) {
        self.interner.clear();
    }

    /// Collects information about a single process.
    pub fn collect_process(&mut self, pid: u32) -> Result<ProcessInfo, CollectError> {
        let proc_dir = format!("{}/{}", self.proc_path, pid);

        // Read /proc/[pid]/stat
        let stat_path = format!("{}/stat", proc_dir);
        let stat_content = self
            .fs
            .read_to_string(Path::new(&stat_path))
            .map_err(|_| CollectError::ProcessGone(pid))?;
        let stat =
            parse_proc_stat(&stat_content).map_err(|e| CollectError::Parse(e.message.clone()))?;

        // Read /proc/[pid]/status
        let status_path = format!("{}/status", proc_dir);
        let status_content = self
            .fs
            .read_to_string(Path::new(&status_path))
            .map_err(|_| CollectError::ProcessGone(pid))?;
        let status = parse_proc_status(&status_content)
            .map_err(|e| CollectError::Parse(e.message.clone()))?;

        // Read /proc/[pid]/io (optional, may fail due to permissions)
        let io_path = format!("{}/io", proc_dir);
        let io = self
            .fs
            .read_to_string(Path::new(&io_path))
            .ok()
            .and_then(|content| parse_proc_io(&content).ok())
            .unwrap_or_default();

        // Read /proc/[pid]/cmdline
        let cmdline_path = format!("{}/cmdline", proc_dir);
        let cmdline = self
            .fs
            .read_to_string(Path::new(&cmdline_path))
            .unwrap_or_default()
            .replace('\0', " ")
            .trim()
            .to_string();

        // Read /proc/[pid]/comm
        let comm_path = format!("{}/comm", proc_dir);
        let comm = self
            .fs
            .read_to_string(Path::new(&comm_path))
            .unwrap_or_else(|_| stat.comm.clone())
            .trim()
            .to_string();

        // Intern strings for deduplication
        let name_hash = self.interner.intern(&comm);
        let cmdline_hash = if cmdline.is_empty() {
            name_hash
        } else {
            self.interner.intern(&cmdline)
        };

        // Convert vsize from bytes to KB
        let vmem = stat.vsize / 1024;
        // Convert rss from pages to KB
        let rmem = (stat.rss.max(0) as u64) * self.page_size / 1024;

        Ok(ProcessInfo {
            pid: stat.pid,
            ppid: stat.ppid,
            uid: status.uid,
            euid: status.euid,
            gid: status.gid,
            egid: status.egid,
            tty: stat.tty_nr as u16,
            state: stat.state,
            num_threads: stat.num_threads as u32,
            exit_signal: stat.exit_signal,
            btime: self.calculate_process_start_time(stat.starttime),
            name_hash,
            cmdline_hash,
            mem: ProcessMemInfo {
                minflt: stat.minflt,
                majflt: stat.majflt,
                vexec: 0, // Would need to parse /proc/[pid]/maps
                vmem,
                rmem,
                pmem: 0, // Would need to parse /proc/[pid]/smaps
                vdata: status.vm_data,
                vstack: status.vm_stk,
                vlibs: status.vm_lib,
                vswap: status.vm_swap,
                vlock: status.vm_lck,
            },
            cpu: ProcessCpuInfo {
                utime: stat.utime,
                stime: stat.stime,
                nice: stat.nice,
                prio: stat.priority,
                rtprio: stat.rt_priority as i32,
                policy: stat.policy as i32,
                curcpu: stat.processor,
                wchan_hash: 0, // Would need to read /proc/[pid]/wchan
                rundelay: 0,   // Would need to read /proc/[pid]/schedstat
                blkdelay: stat.delayacct_blkio_ticks,
                nvcsw: status.voluntary_ctxt_switches,
                nivcsw: status.nonvoluntary_ctxt_switches,
            },
            dsk: ProcessDskInfo {
                rio: io.syscr,
                rsz: io.read_bytes,
                rchar: io.rchar,
                wio: io.syscw,
                wsz: io.write_bytes,
                cwsz: io.cancelled_write_bytes,
            },
        })
    }

    /// Collects information about all processes.
    ///
    /// Processes that disappear during collection are silently skipped.
    pub fn collect_all_processes(&mut self) -> Result<Vec<ProcessInfo>, CollectError> {
        let proc_path = Path::new(&self.proc_path);
        let entries = self.fs.read_dir(proc_path)?;

        let mut processes = Vec::new();

        for entry in entries {
            // Check if entry is a PID directory (numeric name)
            if let Some(name) = entry.file_name().and_then(|n| n.to_str())
                && let Ok(pid) = name.parse::<u32>()
            {
                match self.collect_process(pid) {
                    Ok(info) => processes.push(info),
                    Err(CollectError::ProcessGone(_)) => {
                        // Process disappeared, skip it
                        continue;
                    }
                    Err(e) => {
                        // Log error but continue with other processes
                        eprintln!("Warning: failed to collect process {}: {}", pid, e);
                    }
                }
            }
        }

        Ok(processes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::mock::MockFs;

    #[test]
    fn test_collect_single_process() {
        let fs = MockFs::typical_system();
        let mut collector = ProcessCollector::new(fs, "/proc");

        let info = collector.collect_process(1).unwrap();

        assert_eq!(info.pid, 1);
        assert_eq!(info.ppid, 0);
        assert_eq!(info.uid, 0);
        assert_eq!(info.gid, 0);
    }

    #[test]
    fn test_collect_process_with_special_name() {
        let fs = MockFs::with_special_names();
        let mut collector = ProcessCollector::new(fs, "/proc");

        // Process with spaces in name
        let info = collector.collect_process(5000).unwrap();
        assert_eq!(info.pid, 5000);

        // Verify name was interned
        let name = collector.interner().resolve(info.name_hash);
        assert_eq!(name, Some("Web Content"));
    }

    #[test]
    fn test_collect_all_processes() {
        let fs = MockFs::typical_system();
        let mut collector = ProcessCollector::new(fs, "/proc");

        let processes = collector.collect_all_processes().unwrap();

        // typical_system has 3 processes: 1, 1000, 1001
        assert_eq!(processes.len(), 3);

        // Verify PIDs
        let pids: Vec<u32> = processes.iter().map(|p| p.pid).collect();
        assert!(pids.contains(&1));
        assert!(pids.contains(&1000));
        assert!(pids.contains(&1001));
    }

    #[test]
    fn test_collect_process_gone() {
        let mut fs = MockFs::new();
        fs.add_dir("/proc/9999"); // Directory exists but no files

        let mut collector = ProcessCollector::new(fs, "/proc");
        let result = collector.collect_process(9999);

        assert!(matches!(result, Err(CollectError::ProcessGone(9999))));
    }

    #[test]
    fn test_collect_zombie_process() {
        let fs = MockFs::with_zombie_process();
        let mut collector = ProcessCollector::new(fs, "/proc");

        let info = collector.collect_process(4000).unwrap();
        assert_eq!(info.pid, 4000);
        // Zombie processes have minimal info
        assert_eq!(info.mem.vmem, 0);
    }

    #[test]
    fn test_process_btime_without_boot_time() {
        let fs = MockFs::typical_system();
        let mut collector = ProcessCollector::new(fs, "/proc");

        // Without setting boot_time, btime should be 0
        let info = collector.collect_process(1).unwrap();
        assert_eq!(info.btime, 0);
    }

    #[test]
    fn test_process_btime_with_boot_time() {
        let fs = MockFs::typical_system();
        let mut collector = ProcessCollector::new(fs, "/proc");

        // Set boot time (from /proc/stat btime in typical_system mock)
        collector.set_boot_time(1700000000);

        // PID 1 has starttime = 1 jiffy, so btime = 1700000000 + 1/100 = 1700000000
        let info1 = collector.collect_process(1).unwrap();
        assert_eq!(info1.btime, 1700000000);

        // PID 1000 has starttime = 100000 jiffies, so btime = 1700000000 + 100000/100 = 1700001000
        let info1000 = collector.collect_process(1000).unwrap();
        assert_eq!(info1000.btime, 1700001000);
    }
}
