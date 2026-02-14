//! System collector for gathering global system metrics from `/proc/`.

use crate::collector::procfs::parser::{
    parse_diskstats, parse_global_stat, parse_loadavg, parse_meminfo, parse_mountinfo_device_ids,
    parse_net_dev, parse_net_snmp, parse_netstat, parse_psi, parse_vmstat,
};
use crate::collector::procfs::process::CollectError;
use crate::collector::traits::FileSystem;
use crate::storage::interner::StringInterner;
use crate::storage::model::{
    SystemCpuInfo, SystemDiskInfo, SystemLoadInfo, SystemMemInfo, SystemNetInfo, SystemNetSnmpInfo,
    SystemPsiInfo, SystemStatInfo, SystemVmstatInfo,
};
use std::collections::HashSet;
use std::path::Path;

/// Collects system-wide metrics from `/proc/`.
pub struct SystemCollector<F: FileSystem> {
    fs: F,
    proc_path: String,
}

impl<F: FileSystem> SystemCollector<F> {
    /// Creates a new system collector.
    ///
    /// # Arguments
    /// * `fs` - Filesystem implementation (real or mock)
    /// * `proc_path` - Base path to proc filesystem (usually "/proc")
    pub fn new(fs: F, proc_path: impl Into<String>) -> Self {
        Self {
            fs,
            proc_path: proc_path.into(),
        }
    }

    /// Collects memory information from `/proc/meminfo`.
    pub fn collect_meminfo(&self) -> Result<SystemMemInfo, CollectError> {
        let path = format!("{}/meminfo", self.proc_path);
        let content = self.fs.read_to_string(Path::new(&path))?;
        let info = parse_meminfo(&content).map_err(|e| CollectError::Parse(e.message))?;

        Ok(SystemMemInfo {
            total: info.mem_total,
            free: info.mem_free,
            available: info.mem_available,
            buffers: info.buffers,
            cached: info.cached,
            slab: info.slab,
            sreclaimable: info.s_reclaimable,
            sunreclaim: 0, // Would need to parse this from meminfo
            swap_total: info.swap_total,
            swap_free: info.swap_free,
            dirty: info.dirty,
            writeback: info.writeback,
        })
    }

    /// Collects load average from `/proc/loadavg`.
    pub fn collect_loadavg(&self) -> Result<SystemLoadInfo, CollectError> {
        let path = format!("{}/loadavg", self.proc_path);
        let content = self.fs.read_to_string(Path::new(&path))?;
        let info = parse_loadavg(&content).map_err(|e| CollectError::Parse(e.message))?;

        Ok(SystemLoadInfo {
            lavg1: info.load1 as f32,
            lavg5: info.load5 as f32,
            lavg15: info.load15 as f32,
            nr_running: info.running,
            nr_threads: info.total,
        })
    }

    /// Collects CPU statistics from `/proc/stat`.
    ///
    /// Returns a vector of CPU stats: first element is aggregate, rest are per-CPU.
    pub fn collect_cpuinfo(&self) -> Result<Vec<SystemCpuInfo>, CollectError> {
        let path = format!("{}/stat", self.proc_path);
        let content = self.fs.read_to_string(Path::new(&path))?;
        let info = parse_global_stat(&content).map_err(|e| CollectError::Parse(e.message))?;

        let cpus: Vec<SystemCpuInfo> = info
            .cpus
            .into_iter()
            .map(|cpu| SystemCpuInfo {
                cpu_id: cpu.cpu_id.map(|id| id as i16).unwrap_or(-1),
                user: cpu.user,
                nice: cpu.nice,
                system: cpu.system,
                idle: cpu.idle,
                iowait: cpu.iowait,
                irq: cpu.irq,
                softirq: cpu.softirq,
                steal: cpu.steal,
                guest: cpu.guest,
                guest_nice: cpu.guest_nice,
            })
            .collect();

        Ok(cpus)
    }

    /// Collects disk I/O statistics from `/proc/diskstats`.
    pub fn collect_diskstats(
        &self,
        interner: &mut StringInterner,
    ) -> Result<Vec<SystemDiskInfo>, CollectError> {
        let path = format!("{}/diskstats", self.proc_path);
        let content = self.fs.read_to_string(Path::new(&path))?;
        let disks = parse_diskstats(&content).map_err(|e| CollectError::Parse(e.message))?;

        Ok(disks
            .into_iter()
            .map(|disk| SystemDiskInfo {
                device_name: disk.device.clone(),
                device_hash: interner.intern(&disk.device),
                major: disk.major,
                minor: disk.minor,
                rio: disk.reads,
                r_merged: disk.r_merged,
                rsz: disk.read_sectors,
                read_time: disk.read_time,
                wio: disk.writes,
                w_merged: disk.w_merged,
                wsz: disk.write_sectors,
                write_time: disk.write_time,
                io_in_progress: disk.io_in_progress,
                io_ms: disk.io_time,
                qusz: disk.io_weighted_time,
            })
            .collect())
    }

    /// Collects block device IDs (major, minor) from `/proc/self/mountinfo`.
    ///
    /// This is used for container-aware disk filtering: we only keep device IDs
    /// for disks that are actually mounted in the current mount namespace.
    pub fn collect_mountinfo_device_ids(&self) -> Result<HashSet<(u32, u32)>, CollectError> {
        let path = format!("{}/self/mountinfo", self.proc_path);
        let content = self.fs.read_to_string(Path::new(&path))?;
        Ok(parse_mountinfo_device_ids(&content))
    }

    /// Collects disk I/O statistics from `/proc/diskstats`, but keeps `major/minor`
    /// only for devices present in the provided mountinfo set.
    ///
    /// For any device not present in `mount_devices`, we set `major=0` and `minor=0`
    /// to avoid recording/storing irrelevant device IDs.
    pub fn collect_diskstats_with_mountinfo_filter(
        &self,
        interner: &mut StringInterner,
        mount_devices: &HashSet<(u32, u32)>,
    ) -> Result<Vec<SystemDiskInfo>, CollectError> {
        let path = format!("{}/diskstats", self.proc_path);
        let content = self.fs.read_to_string(Path::new(&path))?;
        let disks = parse_diskstats(&content).map_err(|e| CollectError::Parse(e.message))?;

        Ok(disks
            .into_iter()
            .map(|disk| {
                let keep_id = mount_devices.contains(&(disk.major, disk.minor));
                SystemDiskInfo {
                    device_name: disk.device.clone(),
                    device_hash: interner.intern(&disk.device),
                    major: if keep_id { disk.major } else { 0 },
                    minor: if keep_id { disk.minor } else { 0 },
                    rio: disk.reads,
                    r_merged: disk.r_merged,
                    rsz: disk.read_sectors,
                    read_time: disk.read_time,
                    wio: disk.writes,
                    w_merged: disk.w_merged,
                    wsz: disk.write_sectors,
                    write_time: disk.write_time,
                    io_in_progress: disk.io_in_progress,
                    io_ms: disk.io_time,
                    qusz: disk.io_weighted_time,
                }
            })
            .collect())
    }

    /// Collects network interface statistics from `/proc/net/dev`.
    pub fn collect_net_dev(
        &self,
        interner: &mut StringInterner,
    ) -> Result<Vec<SystemNetInfo>, CollectError> {
        let path = format!("{}/net/dev", self.proc_path);
        let content = self.fs.read_to_string(Path::new(&path))?;
        let devices = parse_net_dev(&content).map_err(|e| CollectError::Parse(e.message))?;

        Ok(devices
            .into_iter()
            .map(|dev| SystemNetInfo {
                name: dev.interface.clone(),
                name_hash: interner.intern(&dev.interface),
                rx_bytes: dev.rx_bytes,
                rx_packets: dev.rx_packets,
                rx_errs: dev.rx_errs,
                rx_drop: dev.rx_drop,
                tx_bytes: dev.tx_bytes,
                tx_packets: dev.tx_packets,
                tx_errs: dev.tx_errs,
                tx_drop: dev.tx_drop,
            })
            .collect())
    }

    /// Collects PSI (Pressure Stall Information) from `/proc/pressure/{cpu,memory,io}`.
    ///
    /// Returns a vector with PSI for cpu (resource=0), memory (resource=1), io (resource=2).
    pub fn collect_psi(&self) -> Result<Vec<SystemPsiInfo>, CollectError> {
        let mut results = Vec::new();

        let resources = [("cpu", 0u8), ("memory", 1u8), ("io", 2u8)];
        for (name, resource) in resources {
            let path = format!("{}/pressure/{}", self.proc_path, name);
            if let Ok(content) = self.fs.read_to_string(Path::new(&path))
                && let Ok(stats) = parse_psi(&content)
            {
                results.push(SystemPsiInfo {
                    resource,
                    some_avg10: stats.some_avg10,
                    some_avg60: stats.some_avg60,
                    some_avg300: stats.some_avg300,
                    some_total: stats.some_total,
                    full_avg10: stats.full_avg10,
                    full_avg60: stats.full_avg60,
                    full_avg300: stats.full_avg300,
                    full_total: stats.full_total,
                });
            }
        }

        Ok(results)
    }

    /// Collects virtual memory statistics from `/proc/vmstat`.
    pub fn collect_vmstat(&self) -> Result<SystemVmstatInfo, CollectError> {
        let path = format!("{}/vmstat", self.proc_path);
        let content = self.fs.read_to_string(Path::new(&path))?;
        let info = parse_vmstat(&content).map_err(|e| CollectError::Parse(e.message))?;

        Ok(SystemVmstatInfo {
            pgfault: info.pgfault,
            pgmajfault: info.pgmajfault,
            pgpgin: info.pgpgin,
            pgpgout: info.pgpgout,
            pswpin: info.pswpin,
            pswpout: info.pswpout,
            pgsteal_kswapd: info.pgsteal_kswapd,
            pgsteal_direct: info.pgsteal_direct,
            pgscan_kswapd: info.pgscan_kswapd,
            pgscan_direct: info.pgscan_direct,
            oom_kill: info.oom_kill,
        })
    }

    /// Collects global system statistics from `/proc/stat` (context switches, forks, etc.).
    pub fn collect_stat(&self) -> Result<SystemStatInfo, CollectError> {
        let path = format!("{}/stat", self.proc_path);
        let content = self.fs.read_to_string(Path::new(&path))?;
        let info = parse_global_stat(&content).map_err(|e| CollectError::Parse(e.message))?;

        Ok(SystemStatInfo {
            ctxt: info.ctxt,
            processes: info.processes,
            procs_running: info.procs_running,
            procs_blocked: info.procs_blocked,
            btime: info.btime,
        })
    }

    /// Collects network SNMP statistics from `/proc/net/snmp` and `/proc/net/netstat`.
    pub fn collect_netsnmp(&self) -> Result<SystemNetSnmpInfo, CollectError> {
        let mut info = SystemNetSnmpInfo::default();

        // Parse /proc/net/snmp for TCP/UDP stats
        let snmp_path = format!("{}/net/snmp", self.proc_path);
        if let Ok(content) = self.fs.read_to_string(Path::new(&snmp_path))
            && let Ok(snmp) = parse_net_snmp(&content)
        {
            info.tcp_active_opens = snmp.tcp_active_opens;
            info.tcp_passive_opens = snmp.tcp_passive_opens;
            info.tcp_attempt_fails = snmp.tcp_attempt_fails;
            info.tcp_estab_resets = snmp.tcp_estab_resets;
            info.tcp_curr_estab = snmp.tcp_curr_estab;
            info.tcp_in_segs = snmp.tcp_in_segs;
            info.tcp_out_segs = snmp.tcp_out_segs;
            info.tcp_retrans_segs = snmp.tcp_retrans_segs;
            info.tcp_in_errs = snmp.tcp_in_errs;
            info.tcp_out_rsts = snmp.tcp_out_rsts;
            info.udp_in_datagrams = snmp.udp_in_datagrams;
            info.udp_out_datagrams = snmp.udp_out_datagrams;
            info.udp_in_errors = snmp.udp_in_errors;
            info.udp_no_ports = snmp.udp_no_ports;
        }

        // Parse /proc/net/netstat for TcpExt stats
        let netstat_path = format!("{}/net/netstat", self.proc_path);
        if let Ok(content) = self.fs.read_to_string(Path::new(&netstat_path))
            && let Ok(netstat) = parse_netstat(&content)
        {
            info.listen_overflows = netstat.listen_overflows;
            info.listen_drops = netstat.listen_drops;
            info.tcp_timeouts = netstat.tcp_timeouts;
            info.tcp_fast_retrans = netstat.tcp_fast_retrans;
            info.tcp_slow_start_retrans = netstat.tcp_slow_start_retrans;
            info.tcp_ofo_queue = netstat.tcp_ofo_queue;
            info.tcp_syn_retrans = netstat.tcp_syn_retrans;
        }

        Ok(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::mock::MockFs;

    #[test]
    fn test_collect_meminfo() {
        let fs = MockFs::typical_system();
        let collector = SystemCollector::new(fs, "/proc");

        let info = collector.collect_meminfo().unwrap();

        assert_eq!(info.total, 16384000);
        assert_eq!(info.free, 8192000);
        assert_eq!(info.available, 12000000);
    }

    #[test]
    fn test_collect_meminfo_pressure() {
        let fs = MockFs::memory_pressure();
        let collector = SystemCollector::new(fs, "/proc");

        let info = collector.collect_meminfo().unwrap();

        assert_eq!(info.free, 256000); // Low free memory
        assert!(info.swap_free < info.swap_total); // Swap in use
    }

    #[test]
    fn test_collect_loadavg() {
        let fs = MockFs::typical_system();
        let collector = SystemCollector::new(fs, "/proc");

        let info = collector.collect_loadavg().unwrap();

        assert!((info.lavg1 - 0.15).abs() < 0.01);
        assert!((info.lavg5 - 0.10).abs() < 0.01);
        assert!((info.lavg15 - 0.05).abs() < 0.01);
        assert_eq!(info.nr_running, 1);
        assert_eq!(info.nr_threads, 150);
    }

    #[test]
    fn test_collect_loadavg_high_load() {
        let fs = MockFs::high_cpu_load();
        let collector = SystemCollector::new(fs, "/proc");

        let info = collector.collect_loadavg().unwrap();

        assert!((info.lavg1 - 4.50).abs() < 0.01);
        assert_eq!(info.nr_running, 8);
    }

    #[test]
    fn test_collect_cpuinfo() {
        let fs = MockFs::typical_system();
        let collector = SystemCollector::new(fs, "/proc");

        let cpus = collector.collect_cpuinfo().unwrap();

        // typical_system has aggregate + 4 CPUs
        assert_eq!(cpus.len(), 5);

        // First is aggregate (cpu_id = -1)
        assert_eq!(cpus[0].cpu_id, -1);
        assert_eq!(cpus[0].user, 10000);

        // Rest are individual CPUs
        assert_eq!(cpus[1].cpu_id, 0);
        assert_eq!(cpus[2].cpu_id, 1);
    }

    #[test]
    fn test_collect_diskstats() {
        let fs = MockFs::typical_system();
        let collector = SystemCollector::new(fs, "/proc");
        let mut interner = StringInterner::new();

        let disks = collector.collect_diskstats(&mut interner).unwrap();

        assert_eq!(disks.len(), 3);

        // Check sda
        assert_eq!(disks[0].major, 8);
        assert_eq!(disks[0].minor, 0);
        assert_eq!(disks[0].rio, 12345);
        assert_eq!(disks[0].rsz, 987654);
        assert_eq!(disks[0].wio, 6789);
        assert_eq!(disks[0].wsz, 456789);
        assert_eq!(disks[0].io_ms, 4000);
        assert_eq!(disks[0].qusz, 8000);

        // Check nvme0n1
        assert_eq!(disks[2].major, 259);
        assert_eq!(disks[2].minor, 0);
        assert_eq!(disks[2].rio, 50000);
        assert_eq!(disks[2].rsz, 2000000);
    }

    #[test]
    fn test_collect_diskstats_with_mountinfo_filter_sets_ids_only_for_mounts() {
        let mut fs = MockFs::new();
        fs.add_file(
            "/proc/diskstats",
            "\
   8       0 sda 1234 0 56789 100 5678 0 98765 200 0 150 300 0 0 0 0\n\
   8       1 sda1 1000 0 50000 80 5000 0 90000 180 0 130 260 0 0 0 0\n\
 259       0 nvme0n1 9999 0 123456 500 8888 0 654321 400 5 1000 2000 0 0 0 0\n",
        );
        fs.add_file(
            "/proc/self/mountinfo",
            "\
36 35 8:1 / / rw,relatime - ext4 /dev/sda1 rw\n\
37 35 0:123 / /proc rw,nosuid,nodev,noexec,relatime - proc proc rw\n",
        );

        let collector = SystemCollector::new(fs, "/proc");
        let mut interner = StringInterner::new();

        let mount_devices = collector.collect_mountinfo_device_ids().unwrap();
        let disks = collector
            .collect_diskstats_with_mountinfo_filter(&mut interner, &mount_devices)
            .unwrap();

        assert_eq!(disks.len(), 3);

        // Only sda1 is in mountinfo, so only it should keep major/minor.
        assert_eq!(disks[0].device_name, "sda");
        assert_eq!(disks[0].major, 0);
        assert_eq!(disks[0].minor, 0);

        assert_eq!(disks[1].device_name, "sda1");
        assert_eq!(disks[1].major, 8);
        assert_eq!(disks[1].minor, 1);

        assert_eq!(disks[2].device_name, "nvme0n1");
        assert_eq!(disks[2].major, 0);
        assert_eq!(disks[2].minor, 0);
    }

    #[test]
    fn test_collect_net_dev() {
        let fs = MockFs::typical_system();
        let collector = SystemCollector::new(fs, "/proc");
        let mut interner = StringInterner::new();

        let devices = collector.collect_net_dev(&mut interner).unwrap();

        assert_eq!(devices.len(), 2);

        // Check lo
        assert_eq!(devices[0].rx_bytes, 12345678);
        assert_eq!(devices[0].rx_packets, 9876);
        assert_eq!(devices[0].tx_bytes, 12345678);
        assert_eq!(devices[0].rx_errs, 0);

        // Check eth0
        assert_eq!(devices[1].rx_bytes, 987654321);
        assert_eq!(devices[1].rx_errs, 5);
        assert_eq!(devices[1].rx_drop, 10);
        assert_eq!(devices[1].tx_bytes, 123456789);
        assert_eq!(devices[1].tx_errs, 2);
        assert_eq!(devices[1].tx_drop, 5);
    }

    #[test]
    fn test_collect_psi() {
        let fs = MockFs::typical_system();
        let collector = SystemCollector::new(fs, "/proc");

        let psi = collector.collect_psi().unwrap();

        // Should have 3 entries: cpu, memory, io
        assert_eq!(psi.len(), 3);

        // CPU (resource=0)
        assert_eq!(psi[0].resource, 0);
        assert!((psi[0].some_avg10 - 0.50).abs() < 0.01);
        assert!((psi[0].some_avg60 - 0.30).abs() < 0.01);

        // Memory (resource=1)
        assert_eq!(psi[1].resource, 1);
        assert!((psi[1].some_avg10 - 0.10).abs() < 0.01);
        assert!((psi[1].full_avg10 - 0.02).abs() < 0.01);

        // IO (resource=2)
        assert_eq!(psi[2].resource, 2);
        assert!((psi[2].some_avg10 - 1.50).abs() < 0.01);
        assert!((psi[2].full_avg10 - 0.50).abs() < 0.01);
    }

    #[test]
    fn test_collect_vmstat() {
        let fs = MockFs::typical_system();
        let collector = SystemCollector::new(fs, "/proc");

        let vmstat = collector.collect_vmstat().unwrap();

        assert_eq!(vmstat.pgpgin, 123456);
        assert_eq!(vmstat.pgpgout, 654321);
        assert_eq!(vmstat.pswpin, 100);
        assert_eq!(vmstat.pswpout, 200);
        assert_eq!(vmstat.pgfault, 999999);
        assert_eq!(vmstat.pgmajfault, 1234);
        assert_eq!(vmstat.pgsteal_kswapd, 5000);
        assert_eq!(vmstat.pgsteal_direct, 1000);
        assert_eq!(vmstat.oom_kill, 0);
    }

    #[test]
    fn test_collect_stat() {
        let fs = MockFs::typical_system();
        let collector = SystemCollector::new(fs, "/proc");

        let stat = collector.collect_stat().unwrap();

        assert_eq!(stat.ctxt, 500000);
        assert_eq!(stat.processes, 10000);
        assert_eq!(stat.procs_running, 2);
        assert_eq!(stat.procs_blocked, 0);
        assert_eq!(stat.btime, 1700000000);
    }
}
