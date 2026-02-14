//! System-wide metrics collected from /proc filesystem.
//!
//! These structures store global system statistics including CPU, memory,
//! network, disk, and various kernel counters. Data is collected from
//! various /proc files that provide system-wide (not per-process) information.

use serde::{Deserialize, Serialize};

/// CPU statistics from /proc/stat.
///
/// Source: `/proc/stat`
///
/// Contains cumulative CPU time counters in jiffies (clock ticks).
/// The first line shows aggregate values across all CPUs,
/// subsequent lines show per-CPU statistics.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct SystemCpuInfo {
    /// CPU identifier: -1 for aggregate total, 0+ for individual cores.
    /// Source: line prefix in `/proc/stat` (cpu, cpu0, cpu1, ...)
    pub cpu_id: i16,

    /// Time spent in user mode (jiffies).
    /// Source: `/proc/stat` column 1
    pub user: u64,

    /// Time spent in user mode with low priority (nice) (jiffies).
    /// Source: `/proc/stat` column 2
    pub nice: u64,

    /// Time spent in system/kernel mode (jiffies).
    /// Source: `/proc/stat` column 3
    pub system: u64,

    /// Time spent idle (jiffies).
    /// Source: `/proc/stat` column 4
    pub idle: u64,

    /// Time waiting for I/O to complete (jiffies).
    /// Source: `/proc/stat` column 5
    pub iowait: u64,

    /// Time servicing hardware interrupts (jiffies).
    /// Source: `/proc/stat` column 6
    pub irq: u64,

    /// Time servicing software interrupts (jiffies).
    /// Source: `/proc/stat` column 7
    pub softirq: u64,

    /// Time stolen by hypervisor for other VMs (jiffies).
    /// Source: `/proc/stat` column 8
    pub steal: u64,

    /// Time spent running guest OS (jiffies).
    /// Source: `/proc/stat` column 9
    pub guest: u64,

    /// Time spent running niced guest OS (jiffies).
    /// Source: `/proc/stat` column 10
    pub guest_nice: u64,
}

/// System load averages from /proc/loadavg.
///
/// Source: `/proc/loadavg`
///
/// Load average represents the average number of processes in
/// runnable or uninterruptible state over time periods.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct SystemLoadInfo {
    /// 1-minute load average.
    /// Source: `/proc/loadavg` field 1
    pub lavg1: f32,

    /// 5-minute load average.
    /// Source: `/proc/loadavg` field 2
    pub lavg5: f32,

    /// 15-minute load average.
    /// Source: `/proc/loadavg` field 3
    pub lavg15: f32,

    /// Number of currently runnable kernel scheduling entities.
    /// Source: `/proc/loadavg` field 4 (before '/')
    pub nr_running: u32,

    /// Total number of kernel scheduling entities (threads).
    /// Source: `/proc/loadavg` field 4 (after '/')
    pub nr_threads: u32,
}

/// Memory statistics from /proc/meminfo.
///
/// Source: `/proc/meminfo`
///
/// All values are in kilobytes (Kb).
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct SystemMemInfo {
    /// Total usable RAM (Kb).
    /// Source: `MemTotal` in `/proc/meminfo`
    pub total: u64,

    /// Free memory (Kb).
    /// Source: `MemFree` in `/proc/meminfo`
    pub free: u64,

    /// Available memory for starting new applications (Kb).
    /// Source: `MemAvailable` in `/proc/meminfo`
    /// Note: Better estimate than free alone
    pub available: u64,

    /// Memory used for block device buffers (Kb).
    /// Source: `Buffers` in `/proc/meminfo`
    pub buffers: u64,

    /// Memory used for page cache (Kb).
    /// Source: `Cached` in `/proc/meminfo`
    pub cached: u64,

    /// Total memory used by kernel slab allocator (Kb).
    /// Source: `Slab` in `/proc/meminfo`
    pub slab: u64,

    /// Reclaimable slab memory (Kb).
    /// Source: `SReclaimable` in `/proc/meminfo`
    pub sreclaimable: u64,

    /// Unreclaimable slab memory (Kb).
    /// Source: `SUnreclaim` in `/proc/meminfo`
    pub sunreclaim: u64,

    /// Total swap space (Kb).
    /// Source: `SwapTotal` in `/proc/meminfo`
    pub swap_total: u64,

    /// Free swap space (Kb).
    /// Source: `SwapFree` in `/proc/meminfo`
    pub swap_free: u64,

    /// Memory waiting to be written back to disk (Kb).
    /// Source: `Dirty` in `/proc/meminfo`
    pub dirty: u64,

    /// Memory actively being written back to disk (Kb).
    /// Source: `Writeback` in `/proc/meminfo`
    pub writeback: u64,
}

/// Network interface statistics from /proc/net/dev.
///
/// Source: `/proc/net/dev`
///
/// Per-interface network traffic counters.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct SystemNetInfo {
    /// Interface name (eth0, lo, enp0s3, etc.).
    /// Source: interface name from `/proc/net/dev`
    pub name: String,

    /// Hash of interface name for delta encoding.
    /// Source: interface name from `/proc/net/dev` - interned via StringInterner
    pub name_hash: u64,

    /// Total bytes received on this interface.
    /// Source: `/proc/net/dev` receive bytes column
    pub rx_bytes: u64,

    /// Total packets received on this interface.
    /// Source: `/proc/net/dev` receive packets column
    pub rx_packets: u64,

    /// Receive errors count.
    /// Source: `/proc/net/dev` receive errs column
    pub rx_errs: u64,

    /// Receive drops count (packets dropped).
    /// Source: `/proc/net/dev` receive drop column
    pub rx_drop: u64,

    /// Total bytes transmitted on this interface.
    /// Source: `/proc/net/dev` transmit bytes column
    pub tx_bytes: u64,

    /// Total packets transmitted on this interface.
    /// Source: `/proc/net/dev` transmit packets column
    pub tx_packets: u64,

    /// Transmit errors count.
    /// Source: `/proc/net/dev` transmit errs column
    pub tx_errs: u64,

    /// Transmit drops count (packets dropped).
    /// Source: `/proc/net/dev` transmit drop column
    pub tx_drop: u64,
}

/// Block device (disk) statistics from /proc/diskstats.
///
/// Source: `/proc/diskstats`
///
/// Per-device I/O counters for block devices.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct SystemDiskInfo {
    /// Device name (sda, nvme0n1, etc.).
    /// Source: device name from `/proc/diskstats`
    pub device_name: String,

    /// Hash of device name for delta encoding.
    /// Source: device name from `/proc/diskstats` - interned via StringInterner
    pub device_hash: u64,

    /// Block device major number.
    /// Source: `/proc/diskstats` column 1
    #[serde(default)]
    pub major: u32,

    /// Block device minor number.
    /// Source: `/proc/diskstats` column 2
    #[serde(default)]
    pub minor: u32,

    /// Number of read I/O operations completed.
    /// Source: `/proc/diskstats` field 4 (reads completed)
    pub rio: u64,

    /// Number of read requests merged.
    /// Source: `/proc/diskstats` field 5 (reads merged)
    pub r_merged: u64,

    /// Number of sectors read (512 bytes each).
    /// Source: `/proc/diskstats` field 6 (sectors read)
    pub rsz: u64,

    /// Time spent reading (milliseconds).
    /// Source: `/proc/diskstats` field 7 (time spent reading)
    pub read_time: u64,

    /// Number of write I/O operations completed.
    /// Source: `/proc/diskstats` field 8 (writes completed)
    pub wio: u64,

    /// Number of write requests merged.
    /// Source: `/proc/diskstats` field 9 (writes merged)
    pub w_merged: u64,

    /// Number of sectors written (512 bytes each).
    /// Source: `/proc/diskstats` field 10 (sectors written)
    pub wsz: u64,

    /// Time spent writing (milliseconds).
    /// Source: `/proc/diskstats` field 11 (time spent writing)
    pub write_time: u64,

    /// Number of I/Os currently in progress.
    /// Source: `/proc/diskstats` field 12 (I/Os in progress)
    pub io_in_progress: u64,

    /// Total time spent doing I/O (milliseconds).
    /// Source: `/proc/diskstats` field 13 (# of milliseconds spent doing I/O)
    pub io_ms: u64,

    /// Weighted time spent doing I/O (milliseconds).
    /// Source: `/proc/diskstats` field 14 (weighted # of milliseconds)
    pub qusz: u64,
}

/// Pressure Stall Information (PSI) from /proc/pressure/.
///
/// Source: `/proc/pressure/{cpu,memory,io}`
///
/// PSI provides information about resource contention and stalls.
/// Available on kernels 4.20+.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct SystemPsiInfo {
    /// Resource type: 0=cpu, 1=memory, 2=io.
    /// Determines which file this data came from.
    pub resource: u8,

    /// Percentage of time some tasks were stalled (10-second average).
    /// Source: `some avg10` line in `/proc/pressure/*`
    pub some_avg10: f32,

    /// Percentage of time some tasks were stalled (60-second average).
    /// Source: `some avg60` line in `/proc/pressure/*`
    pub some_avg60: f32,

    /// Percentage of time some tasks were stalled (300-second average).
    /// Source: `some avg300` line in `/proc/pressure/*`
    pub some_avg300: f32,

    /// Total stall time for some tasks (microseconds).
    /// Source: `some total` line in `/proc/pressure/*`
    pub some_total: u64,

    /// Percentage of time all tasks were stalled (10-second average).
    /// Source: `full avg10` line in `/proc/pressure/*`
    /// Note: Not available for CPU pressure
    pub full_avg10: f32,

    /// Percentage of time all tasks were stalled (60-second average).
    /// Source: `full avg60` line in `/proc/pressure/*`
    pub full_avg60: f32,

    /// Percentage of time all tasks were stalled (300-second average).
    /// Source: `full avg300` line in `/proc/pressure/*`
    pub full_avg300: f32,

    /// Total stall time when all tasks were blocked (microseconds).
    /// Source: `full total` line in `/proc/pressure/*`
    pub full_total: u64,
}

/// Virtual memory statistics from /proc/vmstat.
///
/// Source: `/proc/vmstat`
///
/// Kernel counters for memory management events.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct SystemVmstatInfo {
    /// Total page faults (minor + major).
    /// Source: `pgfault` in `/proc/vmstat`
    pub pgfault: u64,

    /// Major page faults (required disk I/O).
    /// Source: `pgmajfault` in `/proc/vmstat`
    pub pgmajfault: u64,

    /// Pages read in from block devices.
    /// Source: `pgpgin` in `/proc/vmstat`
    pub pgpgin: u64,

    /// Pages written out to block devices.
    /// Source: `pgpgout` in `/proc/vmstat`
    pub pgpgout: u64,

    /// Pages swapped in from swap space.
    /// Source: `pswpin` in `/proc/vmstat`
    pub pswpin: u64,

    /// Pages swapped out to swap space.
    /// Source: `pswpout` in `/proc/vmstat`
    pub pswpout: u64,

    /// Pages reclaimed by kswapd (background reclaim).
    /// Source: `pgsteal_kswapd` in `/proc/vmstat`
    pub pgsteal_kswapd: u64,

    /// Pages reclaimed directly by process (synchronous).
    /// Source: `pgsteal_direct` in `/proc/vmstat`
    pub pgsteal_direct: u64,

    /// Pages scanned by kswapd.
    /// Source: `pgscan_kswapd` in `/proc/vmstat`
    pub pgscan_kswapd: u64,

    /// Pages scanned directly by process.
    /// Source: `pgscan_direct` in `/proc/vmstat`
    pub pgscan_direct: u64,

    /// Number of OOM killer invocations.
    /// Source: `oom_kill` in `/proc/vmstat`
    pub oom_kill: u64,
}

/// File descriptor and inode statistics from /proc/sys/fs/.
///
/// Sources: `/proc/sys/fs/file-nr`, `/proc/sys/fs/inode-state`
///
/// System-wide limits and usage for file handles and inodes.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct SystemFileInfo {
    /// Number of allocated file handles.
    /// Source: `/proc/sys/fs/file-nr` field 1
    pub nr_file: u64,

    /// Number of free file handles.
    /// Source: `/proc/sys/fs/file-nr` field 2
    pub nr_free_file: u64,

    /// Maximum number of file handles.
    /// Source: `/proc/sys/fs/file-nr` field 3
    pub max_file: u64,

    /// Number of allocated inodes.
    /// Source: `/proc/sys/fs/inode-state` field 1
    pub nr_inode: u64,

    /// Number of free inodes.
    /// Source: `/proc/sys/fs/inode-state` field 2
    pub nr_free_inode: u64,
}

/// Hardware interrupt counters from /proc/interrupts.
///
/// Source: `/proc/interrupts`
///
/// Per-IRQ interrupt counts (aggregated across all CPUs).
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct SystemInterruptInfo {
    /// Hash of IRQ name/number (e.g., "0", "NMI", "LOC", "RES").
    /// Source: first column of `/proc/interrupts` - interned via StringInterner
    pub irq_hash: u64,

    /// Total interrupt count across all CPUs.
    /// Source: sum of per-CPU counts from `/proc/interrupts`
    pub count: u64,
}

/// Software interrupt counters from /proc/softirqs.
///
/// Source: `/proc/softirqs`
///
/// Per-softirq type counts (aggregated across all CPUs).
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct SystemSoftirqInfo {
    /// Hash of softirq name (HI, TIMER, NET_TX, NET_RX, BLOCK, etc.).
    /// Source: first column of `/proc/softirqs` - interned via StringInterner
    pub name_hash: u64,

    /// Total softirq count across all CPUs.
    /// Source: sum of per-CPU counts from `/proc/softirqs`
    pub count: u64,
}

/// Global system statistics from /proc/stat.
///
/// Source: `/proc/stat` (non-CPU lines)
///
/// System-wide counters for context switches, process creation, etc.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct SystemStatInfo {
    /// Total number of context switches since boot.
    /// Source: `ctxt` line in `/proc/stat`
    pub ctxt: u64,

    /// Total number of processes/threads created (forks) since boot.
    /// Source: `processes` line in `/proc/stat`
    pub processes: u64,

    /// Number of processes currently in runnable state.
    /// Source: `procs_running` line in `/proc/stat`
    pub procs_running: u32,

    /// Number of processes currently blocked waiting for I/O.
    /// Source: `procs_blocked` line in `/proc/stat`
    pub procs_blocked: u32,

    /// System boot time in seconds since Unix epoch.
    /// Source: `btime` line in `/proc/stat`
    pub btime: u64,
}

/// TCP/UDP protocol statistics from /proc/net/snmp.
///
/// Source: `/proc/net/snmp`
///
/// Global network protocol counters from the kernel's SNMP agent.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct SystemNetSnmpInfo {
    // ============ TCP Statistics ============
    // Source: `Tcp:` lines in `/proc/net/snmp`
    /// Number of active connection openings (connect()).
    /// Source: `Tcp: ActiveOpens`
    pub tcp_active_opens: u64,

    /// Number of passive connection openings (accept()).
    /// Source: `Tcp: PassiveOpens`
    pub tcp_passive_opens: u64,

    /// Number of failed connection attempts.
    /// Source: `Tcp: AttemptFails`
    pub tcp_attempt_fails: u64,

    /// Number of connection resets received.
    /// Source: `Tcp: EstabResets`
    pub tcp_estab_resets: u64,

    /// Number of currently established connections.
    /// Source: `Tcp: CurrEstab`
    pub tcp_curr_estab: u64,

    /// Total TCP segments received.
    /// Source: `Tcp: InSegs`
    pub tcp_in_segs: u64,

    /// Total TCP segments sent.
    /// Source: `Tcp: OutSegs`
    pub tcp_out_segs: u64,

    /// Total TCP segments retransmitted.
    /// Source: `Tcp: RetransSegs`
    pub tcp_retrans_segs: u64,

    /// Total TCP segments received with errors.
    /// Source: `Tcp: InErrs`
    pub tcp_in_errs: u64,

    /// Total TCP RST segments sent.
    /// Source: `Tcp: OutRsts`
    pub tcp_out_rsts: u64,

    // ============ UDP Statistics ============
    // Source: `Udp:` lines in `/proc/net/snmp`
    /// Total UDP datagrams received.
    /// Source: `Udp: InDatagrams`
    pub udp_in_datagrams: u64,

    /// Total UDP datagrams sent.
    /// Source: `Udp: OutDatagrams`
    pub udp_out_datagrams: u64,

    /// Total UDP datagrams received with errors.
    /// Source: `Udp: InErrors`
    pub udp_in_errors: u64,

    /// Total UDP datagrams received for unknown port.
    /// Source: `Udp: NoPorts`
    pub udp_no_ports: u64,

    // ============ TcpExt Statistics (from /proc/net/netstat) ============
    /// Listen queue overflows (connection rejected due to full backlog).
    /// Source: `TcpExt: ListenOverflows` in `/proc/net/netstat`
    pub listen_overflows: u64,

    /// Listen queue drops (SYN dropped because accept queue was full).
    /// Source: `TcpExt: ListenDrops` in `/proc/net/netstat`
    pub listen_drops: u64,

    /// TCP connection timeouts.
    /// Source: `TcpExt: TCPTimeouts` in `/proc/net/netstat`
    pub tcp_timeouts: u64,

    /// TCP fast retransmits.
    /// Source: `TcpExt: TCPFastRetrans` in `/proc/net/netstat`
    pub tcp_fast_retrans: u64,

    /// TCP slow start retransmits.
    /// Source: `TcpExt: TCPSlowStartRetrans` in `/proc/net/netstat`
    pub tcp_slow_start_retrans: u64,

    /// Packets received out of order and queued.
    /// Source: `TcpExt: TCPOFOQueue` in `/proc/net/netstat`
    pub tcp_ofo_queue: u64,

    /// TCP SYN retransmits.
    /// Source: `TcpExt: TCPSynRetrans` in `/proc/net/netstat`
    pub tcp_syn_retrans: u64,
}
