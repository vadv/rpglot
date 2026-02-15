//! Process-level metrics collected from /proc/[pid]/ filesystem.
//!
//! These structures store per-process information similar to what `atop` collects.
//! Data is gathered from various /proc/[pid]/ files including stat, status, io, etc.

use serde::{Deserialize, Serialize};

/// Memory statistics for a single process.
///
/// Source: `/proc/[pid]/stat`, `/proc/[pid]/status`, `/proc/[pid]/smaps`
///
/// All memory values are in kilobytes (Kb) unless otherwise noted.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct ProcessMemInfo {
    /// Number of minor page faults (page reclaims without I/O).
    /// Source: `/proc/[pid]/stat` field 10 (minflt)
    pub minflt: u64,

    /// Number of major page faults (required disk I/O).
    /// Source: `/proc/[pid]/stat` field 12 (majflt)
    pub majflt: u64,

    /// Virtual memory size of executable code (Kb).
    /// Source: calculated from `/proc/[pid]/maps`
    pub vexec: u64,

    /// Total virtual memory size (Kb).
    /// Source: `/proc/[pid]/stat` field 23 (vsize) / 1024
    pub vmem: u64,

    /// Resident set size - physical memory in use (Kb).
    /// Source: `/proc/[pid]/stat` field 24 (rss) * page_size / 1024
    pub rmem: u64,

    /// Proportional set size - memory accounting for shared pages (Kb).
    /// Source: `/proc/[pid]/smaps` (Pss)
    pub pmem: u64,

    /// Virtual memory used for data segment (Kb).
    /// Source: `/proc/[pid]/status` (VmData)
    pub vdata: u64,

    /// Virtual memory used for stack (Kb).
    /// Source: `/proc/[pid]/status` (VmStk)
    pub vstack: u64,

    /// Virtual memory used for shared libraries (Kb).
    /// Source: `/proc/[pid]/status` (VmLib)
    pub vlibs: u64,

    /// Swap space used by process (Kb).
    /// Source: `/proc/[pid]/status` (VmSwap)
    pub vswap: u64,

    /// Locked virtual memory that cannot be swapped (Kb).
    /// Source: `/proc/[pid]/status` (VmLck)
    pub vlock: u64,
}

/// CPU statistics for a single process.
///
/// Source: `/proc/[pid]/stat`, `/proc/[pid]/schedstat`, `/proc/[pid]/wchan`
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct ProcessCpuInfo {
    /// Time spent in user mode (clock ticks / jiffies).
    /// Source: `/proc/[pid]/stat` field 14 (utime)
    pub utime: u64,

    /// Time spent in kernel/system mode (clock ticks / jiffies).
    /// Source: `/proc/[pid]/stat` field 15 (stime)
    pub stime: u64,

    /// Nice value ranging from -20 (high priority) to 19 (low priority).
    /// Source: `/proc/[pid]/stat` field 19 (nice)
    pub nice: i32,

    /// Scheduling priority.
    /// Source: `/proc/[pid]/stat` field 18 (priority)
    pub prio: i32,

    /// Real-time scheduling priority (0 for non-RT processes).
    /// Source: `/proc/[pid]/stat` field 40 (rt_priority)
    pub rtprio: i32,

    /// Scheduling policy (0=SCHED_NORMAL, 1=SCHED_FIFO, 2=SCHED_RR, etc.).
    /// Source: `/proc/[pid]/stat` field 41 (policy)
    pub policy: i32,

    /// CPU number the process is currently running on.
    /// Source: `/proc/[pid]/stat` field 39 (processor)
    pub curcpu: i32,

    /// Hash of wait channel name (kernel function where process sleeps).
    /// Source: `/proc/[pid]/wchan` - string is interned via StringInterner
    pub wchan_hash: u64,

    /// Total time spent waiting to run on CPU (nanoseconds).
    /// Source: `/proc/[pid]/schedstat` field 2 (run_delay)
    pub rundelay: u64,

    /// Time waiting for block I/O to complete (clock ticks).
    /// Source: `/proc/[pid]/stat` field 42 (delayacct_blkio_ticks)
    pub blkdelay: u64,

    /// Number of voluntary context switches.
    /// Source: `/proc/[pid]/status` (voluntary_ctxt_switches)
    pub nvcsw: u64,

    /// Number of involuntary context switches.
    /// Source: `/proc/[pid]/status` (nonvoluntary_ctxt_switches)
    pub nivcsw: u64,
}

/// Disk I/O statistics for a single process.
///
/// Source: `/proc/[pid]/io`
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct ProcessDskInfo {
    /// Number of read operations (read syscalls).
    /// Source: `/proc/[pid]/io` (syscr)
    pub rio: u64,

    /// Total bytes read (cumulative sectors read).
    /// Source: `/proc/[pid]/io` (read_bytes)
    pub rsz: u64,

    /// Total bytes read through read() syscalls (includes page cache hits).
    /// Source: `/proc/[pid]/io` (rchar)
    #[serde(default)]
    pub rchar: u64,

    /// Number of write operations (write syscalls).
    /// Source: `/proc/[pid]/io` (syscw)
    pub wio: u64,

    /// Total bytes written through write() syscalls (includes page cache).
    /// Source: `/proc/[pid]/io` (wchar)
    #[serde(default)]
    pub wchar: u64,

    /// Total bytes written (cumulative sectors written).
    /// Source: `/proc/[pid]/io` (write_bytes)
    pub wsz: u64,

    /// Cancelled write bytes (writes that were truncated/cancelled).
    /// Source: `/proc/[pid]/io` (cancelled_write_bytes)
    pub cwsz: u64,
}

/// Complete process information combining identity, memory, CPU, disk, and network stats.
///
/// This is the main structure for storing per-process metrics, similar to
/// the `tstat` structure in atop's source code.
///
/// Sources: Various files under `/proc/[pid]/`
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct ProcessInfo {
    /// Process ID.
    /// Source: directory name under `/proc/`
    pub pid: u32,

    /// Parent process ID.
    /// Source: `/proc/[pid]/stat` field 4 (ppid)
    pub ppid: u32,

    /// Real user ID of the process owner.
    /// Source: `/proc/[pid]/status` (Uid line, first value)
    pub uid: u32,

    /// Effective user ID of the process.
    /// Source: `/proc/[pid]/status` (Uid line, second value)
    pub euid: u32,

    /// Real group ID of the process.
    /// Source: `/proc/[pid]/status` (Gid line, first value)
    pub gid: u32,

    /// Effective group ID of the process.
    /// Source: `/proc/[pid]/status` (Gid line, second value)
    pub egid: u32,

    /// Controlling terminal (tty).
    /// Source: `/proc/[pid]/stat` field 7 (tty_nr)
    pub tty: u16,

    /// Process state (R=running, S=sleeping, D=disk sleep, Z=zombie, T=stopped, etc.).
    /// Source: `/proc/[pid]/stat` field 3 (state)
    pub state: char,

    /// Number of threads in this process.
    /// Source: `/proc/[pid]/stat` field 20 (num_threads)
    pub num_threads: u32,

    /// Exit signal to be sent to parent when process dies.
    /// Source: `/proc/[pid]/stat` field 38 (exit_signal)
    pub exit_signal: i32,

    /// Process start time (seconds since epoch).
    /// Source: calculated from `/proc/[pid]/stat` field 22 (starttime) + boot time
    pub btime: u32,

    /// Hash of process name (comm).
    /// Source: `/proc/[pid]/comm` or `/proc/[pid]/stat` field 2
    /// String is interned via StringInterner for deduplication
    pub name_hash: u64,

    /// Hash of full command line.
    /// Source: `/proc/[pid]/cmdline`
    /// String is interned via StringInterner for deduplication
    pub cmdline_hash: u64,

    /// Memory statistics (see ProcessMemInfo).
    pub mem: ProcessMemInfo,

    /// CPU statistics (see ProcessCpuInfo).
    pub cpu: ProcessCpuInfo,

    /// Disk I/O statistics (see ProcessDskInfo).
    pub dsk: ProcessDskInfo,
}
