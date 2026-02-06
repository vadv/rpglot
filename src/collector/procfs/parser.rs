//! Parsers for `/proc` filesystem files.
//!
//! These are pure functions that parse the content of various `/proc` files
//! into structured data. They are designed to be easily testable with string inputs.

use std::collections::{HashMap, HashSet};

/// Error type for parsing failures.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
}

impl ParseError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Parse error: {}", self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parsed data from `/proc/[pid]/stat`.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct ProcStat {
    pub pid: u32,
    pub comm: String,
    pub state: char,
    pub ppid: u32,
    pub pgrp: i32,
    pub session: i32,
    pub tty_nr: i32,
    pub tpgid: i32,
    pub flags: u32,
    pub minflt: u64,
    pub cminflt: u64,
    pub majflt: u64,
    pub cmajflt: u64,
    pub utime: u64,
    pub stime: u64,
    pub cutime: i64,
    pub cstime: i64,
    pub priority: i32,
    pub nice: i32,
    pub num_threads: i32,
    pub itrealvalue: i64,
    pub starttime: u64,
    pub vsize: u64,
    pub rss: i64,
    pub rsslim: u64,
    pub exit_signal: i32,
    pub processor: i32,
    pub rt_priority: u32,
    pub policy: u32,
    pub delayacct_blkio_ticks: u64,
}

/// Parses `/proc/[pid]/stat` content.
///
/// The format is tricky because the comm field can contain spaces and parentheses.
/// Format: pid (comm) state ppid pgrp session tty_nr ...
pub fn parse_proc_stat(content: &str) -> Result<ProcStat, ParseError> {
    let content = content.trim();

    // Find the comm field boundaries (enclosed in parentheses)
    let open_paren = content
        .find('(')
        .ok_or_else(|| ParseError::new("missing '(' in stat"))?;
    let close_paren = content
        .rfind(')')
        .ok_or_else(|| ParseError::new("missing ')' in stat"))?;

    if close_paren <= open_paren {
        return Err(ParseError::new("invalid parentheses in stat"));
    }

    // Parse PID (before the first '(')
    let pid: u32 = content[..open_paren]
        .trim()
        .parse()
        .map_err(|_| ParseError::new("invalid pid"))?;

    // Extract comm (between parentheses)
    let comm = content[open_paren + 1..close_paren].to_string();

    // Parse remaining fields (after the closing ')')
    let remaining = &content[close_paren + 1..];
    let fields: Vec<&str> = remaining.split_whitespace().collect();

    if fields.len() < 42 {
        return Err(ParseError::new(format!(
            "not enough fields in stat: expected 42+, got {}",
            fields.len()
        )));
    }

    let parse_field = |idx: usize, name: &str| -> Result<i64, ParseError> {
        fields
            .get(idx)
            .ok_or_else(|| ParseError::new(format!("missing field {}", name)))?
            .parse()
            .map_err(|_| ParseError::new(format!("invalid {}", name)))
    };

    let parse_field_u64 = |idx: usize, name: &str| -> Result<u64, ParseError> {
        fields
            .get(idx)
            .ok_or_else(|| ParseError::new(format!("missing field {}", name)))?
            .parse()
            .map_err(|_| ParseError::new(format!("invalid {}", name)))
    };

    Ok(ProcStat {
        pid,
        comm,
        state: fields[0].chars().next().unwrap_or('?'),
        ppid: parse_field(1, "ppid")? as u32,
        pgrp: parse_field(2, "pgrp")? as i32,
        session: parse_field(3, "session")? as i32,
        tty_nr: parse_field(4, "tty_nr")? as i32,
        tpgid: parse_field(5, "tpgid")? as i32,
        flags: parse_field_u64(6, "flags")? as u32,
        minflt: parse_field_u64(7, "minflt")?,
        cminflt: parse_field_u64(8, "cminflt")?,
        majflt: parse_field_u64(9, "majflt")?,
        cmajflt: parse_field_u64(10, "cmajflt")?,
        utime: parse_field_u64(11, "utime")?,
        stime: parse_field_u64(12, "stime")?,
        cutime: parse_field(13, "cutime")?,
        cstime: parse_field(14, "cstime")?,
        priority: parse_field(15, "priority")? as i32,
        nice: parse_field(16, "nice")? as i32,
        num_threads: parse_field(17, "num_threads")? as i32,
        itrealvalue: parse_field(18, "itrealvalue")?,
        starttime: parse_field_u64(19, "starttime")?,
        vsize: parse_field_u64(20, "vsize")?,
        rss: parse_field(21, "rss")?,
        rsslim: parse_field_u64(22, "rsslim")?,
        exit_signal: fields.get(35).and_then(|s| s.parse().ok()).unwrap_or(0),
        processor: fields.get(36).and_then(|s| s.parse().ok()).unwrap_or(0),
        rt_priority: fields.get(37).and_then(|s| s.parse().ok()).unwrap_or(0),
        policy: fields.get(38).and_then(|s| s.parse().ok()).unwrap_or(0),
        delayacct_blkio_ticks: fields.get(39).and_then(|s| s.parse().ok()).unwrap_or(0),
    })
}

/// Parsed data from `/proc/[pid]/status`.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct ProcStatus {
    pub name: String,
    pub pid: u32,
    pub ppid: u32,
    pub uid: u32,
    pub euid: u32,
    pub gid: u32,
    pub egid: u32,
    pub vm_peak: u64,
    pub vm_size: u64,
    pub vm_rss: u64,
    pub vm_data: u64,
    pub vm_stk: u64,
    pub vm_lib: u64,
    pub vm_swap: u64,
    pub vm_lck: u64,
    pub voluntary_ctxt_switches: u64,
    pub nonvoluntary_ctxt_switches: u64,
}

/// Parses `/proc/[pid]/status` content.
///
/// Format is key:\tvalue pairs, one per line.
pub fn parse_proc_status(content: &str) -> Result<ProcStatus, ParseError> {
    let mut status = ProcStatus::default();
    let mut fields: HashMap<&str, &str> = HashMap::new();

    for line in content.lines() {
        if let Some((key, value)) = line.split_once(':') {
            fields.insert(key.trim(), value.trim());
        }
    }

    status.name = fields.get("Name").unwrap_or(&"").to_string();
    status.pid = fields.get("Pid").and_then(|s| s.parse().ok()).unwrap_or(0);
    status.ppid = fields.get("PPid").and_then(|s| s.parse().ok()).unwrap_or(0);

    // Uid and Gid have format: real effective saved fs
    if let Some(uid_line) = fields.get("Uid") {
        let parts: Vec<&str> = uid_line.split_whitespace().collect();
        if let Some(uid) = parts.first() {
            status.uid = uid.parse().unwrap_or(0);
        }
        if let Some(euid) = parts.get(1) {
            status.euid = euid.parse().unwrap_or(0);
        }
    }
    if let Some(gid_line) = fields.get("Gid") {
        let parts: Vec<&str> = gid_line.split_whitespace().collect();
        if let Some(gid) = parts.first() {
            status.gid = gid.parse().unwrap_or(0);
        }
        if let Some(egid) = parts.get(1) {
            status.egid = egid.parse().unwrap_or(0);
        }
    }

    // Memory fields are in kB format: "12345 kB"
    let parse_kb = |key: &str| -> u64 {
        fields
            .get(key)
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    };

    status.vm_peak = parse_kb("VmPeak");
    status.vm_size = parse_kb("VmSize");
    status.vm_rss = parse_kb("VmRSS");
    status.vm_data = parse_kb("VmData");
    status.vm_stk = parse_kb("VmStk");
    status.vm_lib = parse_kb("VmLib");
    status.vm_swap = parse_kb("VmSwap");
    status.vm_lck = parse_kb("VmLck");

    status.voluntary_ctxt_switches = fields
        .get("voluntary_ctxt_switches")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    status.nonvoluntary_ctxt_switches = fields
        .get("nonvoluntary_ctxt_switches")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    Ok(status)
}

/// Parsed data from `/proc/[pid]/io`.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct ProcIo {
    pub rchar: u64,
    pub wchar: u64,
    pub syscr: u64,
    pub syscw: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub cancelled_write_bytes: u64,
}

/// Parses `/proc/[pid]/io` content.
///
/// Format is key: value pairs, one per line.
pub fn parse_proc_io(content: &str) -> Result<ProcIo, ParseError> {
    let mut io = ProcIo::default();

    for line in content.lines() {
        if let Some((key, value)) = line.split_once(':') {
            let value: u64 = value.trim().parse().unwrap_or(0);
            match key.trim() {
                "rchar" => io.rchar = value,
                "wchar" => io.wchar = value,
                "syscr" => io.syscr = value,
                "syscw" => io.syscw = value,
                "read_bytes" => io.read_bytes = value,
                "write_bytes" => io.write_bytes = value,
                "cancelled_write_bytes" => io.cancelled_write_bytes = value,
                _ => {}
            }
        }
    }

    Ok(io)
}

/// Parsed data from `/proc/meminfo`.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct MemInfo {
    pub mem_total: u64,
    pub mem_free: u64,
    pub mem_available: u64,
    pub buffers: u64,
    pub cached: u64,
    pub swap_cached: u64,
    pub active: u64,
    pub inactive: u64,
    pub swap_total: u64,
    pub swap_free: u64,
    pub dirty: u64,
    pub writeback: u64,
    pub slab: u64,
    pub s_reclaimable: u64,
}

/// Parses `/proc/meminfo` content.
pub fn parse_meminfo(content: &str) -> Result<MemInfo, ParseError> {
    let mut info = MemInfo::default();

    let parse_kb = |line: &str| -> u64 {
        line.split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    };

    for line in content.lines() {
        if line.starts_with("MemTotal:") {
            info.mem_total = parse_kb(line);
        } else if line.starts_with("MemFree:") {
            info.mem_free = parse_kb(line);
        } else if line.starts_with("MemAvailable:") {
            info.mem_available = parse_kb(line);
        } else if line.starts_with("Buffers:") {
            info.buffers = parse_kb(line);
        } else if line.starts_with("Cached:") && !line.starts_with("SwapCached:") {
            info.cached = parse_kb(line);
        } else if line.starts_with("SwapCached:") {
            info.swap_cached = parse_kb(line);
        } else if line.starts_with("Active:") {
            info.active = parse_kb(line);
        } else if line.starts_with("Inactive:") {
            info.inactive = parse_kb(line);
        } else if line.starts_with("SwapTotal:") {
            info.swap_total = parse_kb(line);
        } else if line.starts_with("SwapFree:") {
            info.swap_free = parse_kb(line);
        } else if line.starts_with("Dirty:") {
            info.dirty = parse_kb(line);
        } else if line.starts_with("Writeback:") {
            info.writeback = parse_kb(line);
        } else if line.starts_with("Slab:") {
            info.slab = parse_kb(line);
        } else if line.starts_with("SReclaimable:") {
            info.s_reclaimable = parse_kb(line);
        }
    }

    Ok(info)
}

/// Single CPU stats from `/proc/stat`.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct CpuStat {
    pub cpu_id: Option<u32>, // None for aggregate "cpu" line
    pub user: u64,
    pub nice: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub irq: u64,
    pub softirq: u64,
    pub steal: u64,
    pub guest: u64,
    pub guest_nice: u64,
}

/// Global stats from `/proc/stat`.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct GlobalStat {
    pub cpus: Vec<CpuStat>,
    pub ctxt: u64,
    pub btime: u64,
    pub processes: u64,
    pub procs_running: u32,
    pub procs_blocked: u32,
}

/// Parses `/proc/stat` content.
pub fn parse_global_stat(content: &str) -> Result<GlobalStat, ParseError> {
    let mut stat = GlobalStat::default();

    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        if parts[0].starts_with("cpu") {
            let cpu_id = if parts[0] == "cpu" {
                None
            } else {
                parts[0].strip_prefix("cpu").and_then(|s| s.parse().ok())
            };

            let get_val =
                |idx: usize| -> u64 { parts.get(idx).and_then(|s| s.parse().ok()).unwrap_or(0) };

            stat.cpus.push(CpuStat {
                cpu_id,
                user: get_val(1),
                nice: get_val(2),
                system: get_val(3),
                idle: get_val(4),
                iowait: get_val(5),
                irq: get_val(6),
                softirq: get_val(7),
                steal: get_val(8),
                guest: get_val(9),
                guest_nice: get_val(10),
            });
        } else if parts[0] == "ctxt" {
            stat.ctxt = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        } else if parts[0] == "btime" {
            stat.btime = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        } else if parts[0] == "processes" {
            stat.processes = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        } else if parts[0] == "procs_running" {
            stat.procs_running = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        } else if parts[0] == "procs_blocked" {
            stat.procs_blocked = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        }
    }

    Ok(stat)
}

/// Parsed data from `/proc/loadavg`.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct LoadAvg {
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
    pub running: u32,
    pub total: u32,
    pub last_pid: u32,
}

/// Parses `/proc/loadavg` content.
pub fn parse_loadavg(content: &str) -> Result<LoadAvg, ParseError> {
    let parts: Vec<&str> = content.split_whitespace().collect();
    if parts.len() < 5 {
        return Err(ParseError::new("invalid loadavg format"));
    }

    let load1 = parts[0]
        .parse()
        .map_err(|_| ParseError::new("invalid load1"))?;
    let load5 = parts[1]
        .parse()
        .map_err(|_| ParseError::new("invalid load5"))?;
    let load15 = parts[2]
        .parse()
        .map_err(|_| ParseError::new("invalid load15"))?;

    // Format: running/total
    let (running, total) = if let Some((r, t)) = parts[3].split_once('/') {
        (r.parse().unwrap_or(0), t.parse().unwrap_or(0))
    } else {
        (0, 0)
    };

    let last_pid = parts[4].parse().unwrap_or(0);

    Ok(LoadAvg {
        load1,
        load5,
        load15,
        running,
        total,
        last_pid,
    })
}

/// Parsed entry from `/etc/passwd`.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct PasswdEntry {
    pub username: String,
    pub uid: u32,
    pub gid: u32,
    pub gecos: String,
    pub home: String,
    pub shell: String,
}

/// Parses `/etc/passwd` content and returns a map of UID -> username.
///
/// Format: username:password:uid:gid:gecos:home:shell
pub fn parse_passwd(content: &str) -> HashMap<u32, String> {
    let mut map = HashMap::new();
    for line in content.lines() {
        // Skip comments and empty lines
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() >= 3
            && let Ok(uid) = parts[2].parse::<u32>()
        {
            map.insert(uid, parts[0].to_string());
        }
    }
    map
}

/// Resolver for UID -> username mapping.
///
/// Caches the passwd file contents for efficient lookups.
#[derive(Debug, Clone, Default)]
pub struct UserResolver {
    uid_to_name: HashMap<u32, String>,
}

#[allow(dead_code)]
impl UserResolver {
    /// Creates a new empty resolver.
    pub fn new() -> Self {
        Self {
            uid_to_name: HashMap::new(),
        }
    }

    /// Loads user mappings from /etc/passwd content.
    pub fn load_from_content(&mut self, content: &str) {
        self.uid_to_name = parse_passwd(content);
    }

    /// Resolves UID to username, returns UID as string if not found.
    pub fn resolve(&self, uid: u32) -> String {
        self.uid_to_name
            .get(&uid)
            .cloned()
            .unwrap_or_else(|| uid.to_string())
    }

    /// Returns true if resolver has any mappings.
    pub fn is_loaded(&self) -> bool {
        !self.uid_to_name.is_empty()
    }
}

// ============ Disk Stats Parser ============

/// Parsed data from `/proc/diskstats`.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct DiskStats {
    /// Block device major number.
    pub major: u32,
    /// Block device minor number.
    pub minor: u32,
    /// Device name (sda, nvme0n1, etc.)
    pub device: String,
    /// Number of reads completed
    pub reads: u64,
    /// Number of read requests merged
    pub r_merged: u64,
    /// Number of sectors read
    pub read_sectors: u64,
    /// Time spent reading (ms)
    pub read_time: u64,
    /// Number of writes completed
    pub writes: u64,
    /// Number of write requests merged
    pub w_merged: u64,
    /// Number of sectors written
    pub write_sectors: u64,
    /// Time spent writing (ms)
    pub write_time: u64,
    /// Number of I/Os currently in progress
    pub io_in_progress: u64,
    /// Time spent doing I/Os (ms)
    pub io_time: u64,
    /// Weighted time spent doing I/Os (ms)
    pub io_weighted_time: u64,
}

/// Parses `/proc/diskstats` content.
///
/// Format: major minor name reads r_merged r_sectors r_time writes w_merged w_sectors w_time io_pending io_time w_io_time [discards ...]
pub fn parse_diskstats(content: &str) -> Result<Vec<DiskStats>, ParseError> {
    let mut disks = Vec::new();

    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 14 {
            continue; // Skip malformed lines
        }

        let major: u32 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
        let minor: u32 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);

        let get_val =
            |idx: usize| -> u64 { parts.get(idx).and_then(|s| s.parse().ok()).unwrap_or(0) };

        disks.push(DiskStats {
            major,
            minor,
            device: parts[2].to_string(),
            reads: get_val(3),
            r_merged: get_val(4),
            read_sectors: get_val(5),
            read_time: get_val(6),
            writes: get_val(7),
            w_merged: get_val(8),
            write_sectors: get_val(9),
            write_time: get_val(10),
            io_in_progress: get_val(11),
            io_time: get_val(12),
            io_weighted_time: get_val(13),
        });
    }

    Ok(disks)
}

// ============ Mount Info Parser ============

/// Extracts a set of block device IDs (major, minor) from `/proc/self/mountinfo`.
///
/// We use this to identify which block devices are actually mounted inside the
/// current mount namespace (container), so we can avoid storing device IDs for
/// unrelated host devices.
///
/// Format (man 5 proc):
/// `mount_id parent_id major:minor root mount_point options ... - fstype source superoptions`
///
/// We only need the `major:minor` field (3rd column).
pub fn parse_mountinfo_device_ids(content: &str) -> HashSet<(u32, u32)> {
    let mut devices = HashSet::new();

    for line in content.lines() {
        let mut parts = line.split_whitespace();

        // Skip mount_id, parent_id
        let _ = parts.next();
        let _ = parts.next();

        let Some(dev) = parts.next() else {
            continue;
        };

        let Some((major_s, minor_s)) = dev.split_once(':') else {
            continue;
        };

        let Ok(major) = major_s.parse::<u32>() else {
            continue;
        };
        let Ok(minor) = minor_s.parse::<u32>() else {
            continue;
        };

        // Ignore pseudo devices (e.g. overlay often shows 0:XXX).
        if major == 0 {
            continue;
        }

        devices.insert((major, minor));
    }

    devices
}

// ============ Network Device Stats Parser ============

/// Parsed data from `/proc/net/dev`.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct NetDevStats {
    /// Interface name (eth0, lo, etc.)
    pub interface: String,
    /// Bytes received
    pub rx_bytes: u64,
    /// Packets received
    pub rx_packets: u64,
    /// Receive errors
    pub rx_errs: u64,
    /// Receive drops
    pub rx_drop: u64,
    /// Bytes transmitted
    pub tx_bytes: u64,
    /// Packets transmitted
    pub tx_packets: u64,
    /// Transmit errors
    pub tx_errs: u64,
    /// Transmit drops
    pub tx_drop: u64,
}

/// Parses `/proc/net/dev` content.
///
/// Format:
/// Inter-|   Receive                                                |  Transmit
///  face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
///    lo: 1234567     1234    0    0    0     0          0         0  1234567     1234    0    0    0     0       0          0
pub fn parse_net_dev(content: &str) -> Result<Vec<NetDevStats>, ParseError> {
    let mut devices = Vec::new();

    for line in content.lines() {
        // Skip header lines
        if line.contains('|') || line.trim().is_empty() {
            continue;
        }

        // Format: "interface: rx_bytes rx_packets rx_errs rx_drop rx_fifo rx_frame rx_compressed rx_multicast tx_bytes tx_packets tx_errs tx_drop tx_fifo tx_colls tx_carrier tx_compressed"
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() != 2 {
            continue;
        }

        let interface = parts[0].trim().to_string();
        let values: Vec<&str> = parts[1].split_whitespace().collect();
        if values.len() < 16 {
            continue;
        }

        let get_val =
            |idx: usize| -> u64 { values.get(idx).and_then(|s| s.parse().ok()).unwrap_or(0) };

        devices.push(NetDevStats {
            interface,
            rx_bytes: get_val(0),
            rx_packets: get_val(1),
            rx_errs: get_val(2),
            rx_drop: get_val(3),
            tx_bytes: get_val(8),
            tx_packets: get_val(9),
            tx_errs: get_val(10),
            tx_drop: get_val(11),
        });
    }

    Ok(devices)
}

// ============ PSI (Pressure Stall Information) Parser ============

/// Parsed data from `/proc/pressure/{cpu,memory,io}`.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct PsiStats {
    /// "some" line averages and total
    pub some_avg10: f32,
    pub some_avg60: f32,
    pub some_avg300: f32,
    pub some_total: u64,
    /// "full" line averages and total (not available for CPU)
    pub full_avg10: f32,
    pub full_avg60: f32,
    pub full_avg300: f32,
    pub full_total: u64,
}

/// Parses `/proc/pressure/*` content.
///
/// Format:
/// some avg10=0.00 avg60=0.00 avg300=0.00 total=0
/// full avg10=0.00 avg60=0.00 avg300=0.00 total=0
pub fn parse_psi(content: &str) -> Result<PsiStats, ParseError> {
    let mut stats = PsiStats::default();

    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let is_some = parts[0] == "some";
        let is_full = parts[0] == "full";
        if !is_some && !is_full {
            continue;
        }

        for part in &parts[1..] {
            if let Some((key, value)) = part.split_once('=') {
                match (key, is_some) {
                    ("avg10", true) => stats.some_avg10 = value.parse().unwrap_or(0.0),
                    ("avg60", true) => stats.some_avg60 = value.parse().unwrap_or(0.0),
                    ("avg300", true) => stats.some_avg300 = value.parse().unwrap_or(0.0),
                    ("total", true) => stats.some_total = value.parse().unwrap_or(0),
                    ("avg10", false) => stats.full_avg10 = value.parse().unwrap_or(0.0),
                    ("avg60", false) => stats.full_avg60 = value.parse().unwrap_or(0.0),
                    ("avg300", false) => stats.full_avg300 = value.parse().unwrap_or(0.0),
                    ("total", false) => stats.full_total = value.parse().unwrap_or(0),
                    _ => {}
                }
            }
        }
    }

    Ok(stats)
}

// ============ Vmstat Parser ============

/// Parsed data from `/proc/vmstat`.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct VmstatInfo {
    pub pgpgin: u64,
    pub pgpgout: u64,
    pub pswpin: u64,
    pub pswpout: u64,
    pub pgfault: u64,
    pub pgmajfault: u64,
    pub pgsteal_kswapd: u64,
    pub pgsteal_direct: u64,
    pub pgscan_kswapd: u64,
    pub pgscan_direct: u64,
    pub oom_kill: u64,
}

/// Parses `/proc/vmstat` content.
///
/// Format: key value (one per line)
pub fn parse_vmstat(content: &str) -> Result<VmstatInfo, ParseError> {
    let mut info = VmstatInfo::default();

    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let value: u64 = parts[1].parse().unwrap_or(0);
        match parts[0] {
            "pgpgin" => info.pgpgin = value,
            "pgpgout" => info.pgpgout = value,
            "pswpin" => info.pswpin = value,
            "pswpout" => info.pswpout = value,
            "pgfault" => info.pgfault = value,
            "pgmajfault" => info.pgmajfault = value,
            "pgsteal_kswapd" => info.pgsteal_kswapd = value,
            "pgsteal_direct" => info.pgsteal_direct = value,
            "pgscan_kswapd" => info.pgscan_kswapd = value,
            "pgscan_direct" => info.pgscan_direct = value,
            "oom_kill" => info.oom_kill = value,
            _ => {}
        }
    }

    Ok(info)
}

// ============ Network SNMP Parser ============

/// Parsed data from `/proc/net/snmp`.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct NetSnmpStats {
    // TCP statistics
    pub tcp_active_opens: u64,
    pub tcp_passive_opens: u64,
    pub tcp_attempt_fails: u64,
    pub tcp_estab_resets: u64,
    pub tcp_curr_estab: u64,
    pub tcp_in_segs: u64,
    pub tcp_out_segs: u64,
    pub tcp_retrans_segs: u64,
    pub tcp_in_errs: u64,
    pub tcp_out_rsts: u64,
    // UDP statistics
    pub udp_in_datagrams: u64,
    pub udp_out_datagrams: u64,
    pub udp_in_errors: u64,
    pub udp_no_ports: u64,
}

/// Parses `/proc/net/snmp` content.
///
/// Format: Each protocol has two lines - keys and values
/// Tcp: key1 key2 key3...
/// Tcp: val1 val2 val3...
pub fn parse_net_snmp(content: &str) -> Result<NetSnmpStats, ParseError> {
    let mut stats = NetSnmpStats::default();
    let lines: Vec<&str> = content.lines().collect();

    let mut i = 0;
    while i + 1 < lines.len() {
        let key_line = lines[i];
        let val_line = lines[i + 1];

        // Both lines should start with same prefix
        let key_parts: Vec<&str> = key_line.split_whitespace().collect();
        let val_parts: Vec<&str> = val_line.split_whitespace().collect();

        if key_parts.is_empty() || val_parts.is_empty() {
            i += 1;
            continue;
        }

        // Check if both lines have same prefix (e.g., "Tcp:")
        if key_parts[0] != val_parts[0] {
            i += 1;
            continue;
        }

        let prefix = key_parts[0].trim_end_matches(':');
        let keys = &key_parts[1..];
        let vals = &val_parts[1..];

        for (idx, key) in keys.iter().enumerate() {
            let value: u64 = vals.get(idx).and_then(|v| v.parse().ok()).unwrap_or(0);
            match (prefix, *key) {
                ("Tcp", "ActiveOpens") => stats.tcp_active_opens = value,
                ("Tcp", "PassiveOpens") => stats.tcp_passive_opens = value,
                ("Tcp", "AttemptFails") => stats.tcp_attempt_fails = value,
                ("Tcp", "EstabResets") => stats.tcp_estab_resets = value,
                ("Tcp", "CurrEstab") => stats.tcp_curr_estab = value,
                ("Tcp", "InSegs") => stats.tcp_in_segs = value,
                ("Tcp", "OutSegs") => stats.tcp_out_segs = value,
                ("Tcp", "RetransSegs") => stats.tcp_retrans_segs = value,
                ("Tcp", "InErrs") => stats.tcp_in_errs = value,
                ("Tcp", "OutRsts") => stats.tcp_out_rsts = value,
                ("Udp", "InDatagrams") => stats.udp_in_datagrams = value,
                ("Udp", "OutDatagrams") => stats.udp_out_datagrams = value,
                ("Udp", "InErrors") => stats.udp_in_errors = value,
                ("Udp", "NoPorts") => stats.udp_no_ports = value,
                _ => {}
            }
        }
        i += 2;
    }

    Ok(stats)
}

// ============ Network Netstat Parser ============

/// Parsed data from `/proc/net/netstat` (TcpExt/IpExt).
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct NetstatStats {
    // TcpExt statistics
    pub listen_overflows: u64,
    pub listen_drops: u64,
    pub tcp_timeouts: u64,
    pub tcp_fast_retrans: u64,
    pub tcp_slow_start_retrans: u64,
    pub tcp_ofo_queue: u64,
    pub tcp_syn_retrans: u64,
}

/// Parses `/proc/net/netstat` content.
///
/// Format: Each protocol has two lines - keys and values
/// TcpExt: key1 key2 key3...
/// TcpExt: val1 val2 val3...
pub fn parse_netstat(content: &str) -> Result<NetstatStats, ParseError> {
    let mut stats = NetstatStats::default();
    let lines: Vec<&str> = content.lines().collect();

    let mut i = 0;
    while i + 1 < lines.len() {
        let key_line = lines[i];
        let val_line = lines[i + 1];

        let key_parts: Vec<&str> = key_line.split_whitespace().collect();
        let val_parts: Vec<&str> = val_line.split_whitespace().collect();

        if key_parts.is_empty() || val_parts.is_empty() {
            i += 1;
            continue;
        }

        if key_parts[0] != val_parts[0] {
            i += 1;
            continue;
        }

        let prefix = key_parts[0].trim_end_matches(':');
        let keys = &key_parts[1..];
        let vals = &val_parts[1..];

        for (idx, key) in keys.iter().enumerate() {
            let value: u64 = vals.get(idx).and_then(|v| v.parse().ok()).unwrap_or(0);
            match (prefix, *key) {
                ("TcpExt", "ListenOverflows") => stats.listen_overflows = value,
                ("TcpExt", "ListenDrops") => stats.listen_drops = value,
                ("TcpExt", "TCPTimeouts") => stats.tcp_timeouts = value,
                ("TcpExt", "TCPFastRetrans") => stats.tcp_fast_retrans = value,
                ("TcpExt", "TCPSlowStartRetrans") => stats.tcp_slow_start_retrans = value,
                ("TcpExt", "TCPOFOQueue") => stats.tcp_ofo_queue = value,
                ("TcpExt", "TCPSynRetrans") => stats.tcp_syn_retrans = value,
                _ => {}
            }
        }
        i += 2;
    }

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_passwd() {
        let content = "\
root:x:0:0:root:/root:/bin/bash
daemon:x:1:1:daemon:/usr/sbin:/usr/sbin/nologin
nobody:x:65534:65534:nobody:/nonexistent:/usr/sbin/nologin
user:x:1000:1000:User Name:/home/user:/bin/bash
";
        let map = parse_passwd(content);
        assert_eq!(map.get(&0), Some(&"root".to_string()));
        assert_eq!(map.get(&1), Some(&"daemon".to_string()));
        assert_eq!(map.get(&1000), Some(&"user".to_string()));
        assert_eq!(map.get(&65534), Some(&"nobody".to_string()));
    }

    #[test]
    fn test_user_resolver() {
        let mut resolver = UserResolver::new();
        resolver.load_from_content(
            "root:x:0:0::/root:/bin/bash\nuser:x:1000:1000::/home/user:/bin/bash",
        );

        assert_eq!(resolver.resolve(0), "root");
        assert_eq!(resolver.resolve(1000), "user");
        assert_eq!(resolver.resolve(9999), "9999"); // Unknown UID returns as string
        assert!(resolver.is_loaded());
    }

    #[test]
    fn test_parse_proc_stat_basic() {
        let content = "1234 (bash) S 1233 1234 1234 34816 1235 4194304 5000 50000 10 20 100 50 200 100 20 0 1 0 100000 25000000 2000 18446744073709551615 0 0 0 0 0 0 65536 3670020 1266777851 0 0 0 17 2 0 0 5 0 0 0 0 0 0 0 0 0 0";
        let stat = parse_proc_stat(content).unwrap();

        assert_eq!(stat.pid, 1234);
        assert_eq!(stat.comm, "bash");
        assert_eq!(stat.state, 'S');
        assert_eq!(stat.ppid, 1233);
        assert_eq!(stat.utime, 100);
        assert_eq!(stat.stime, 50);
        assert_eq!(stat.nice, 0);
        assert_eq!(stat.priority, 20);
        assert_eq!(stat.minflt, 5000);
        assert_eq!(stat.majflt, 10);
    }

    #[test]
    fn test_parse_proc_stat_with_spaces_in_comm() {
        let content = "5000 (Web Content) S 4999 5000 4999 0 -1 4194304 100000 0 500 0 5000 1000 0 0 20 0 20 0 500000 2000000000 50000 18446744073709551615 0 0 0 0 0 0 0 0 0 0 0 0 17 0 0 0 0 0 0 0 0 0 0 0 0 0 0";
        let stat = parse_proc_stat(content).unwrap();

        assert_eq!(stat.pid, 5000);
        assert_eq!(stat.comm, "Web Content");
        assert_eq!(stat.state, 'S');
        assert_eq!(stat.ppid, 4999);
    }

    #[test]
    fn test_parse_proc_stat_with_parentheses_in_comm() {
        let content = "5001 (test(1)) S 1 5001 5001 0 -1 4194304 1000 0 0 0 10 5 0 0 20 0 1 0 500100 10000000 1000 18446744073709551615 0 0 0 0 0 0 0 0 0 0 0 0 17 0 0 0 0 0 0 0 0 0 0 0 0 0 0";
        let stat = parse_proc_stat(content).unwrap();

        assert_eq!(stat.pid, 5001);
        assert_eq!(stat.comm, "test(1)");
    }

    #[test]
    fn test_parse_proc_stat_zombie() {
        let content = "4000 (defunct) Z 1000 4000 1000 0 -1 4194308 0 0 0 0 0 0 0 0 20 0 1 0 400000 0 0 18446744073709551615 0 0 0 0 0 0 0 0 0 0 0 0 -1 0 0 0 0 0 0 0 0 0 0 0 0 0 0";
        let stat = parse_proc_stat(content).unwrap();

        assert_eq!(stat.pid, 4000);
        assert_eq!(stat.state, 'Z');
    }

    #[test]
    fn test_parse_proc_status() {
        let content = "\
Name:\tbash
Pid:\t1234
PPid:\t1233
Uid:\t1000\t1000\t1000\t1000
Gid:\t1000\t1000\t1000\t1000
VmPeak:\t   30000 kB
VmSize:\t   25000 kB
VmRSS:\t    8000 kB
VmData:\t    2000 kB
VmStk:\t      136 kB
VmLib:\t    3000 kB
VmSwap:\t        0 kB
VmLck:\t        0 kB
voluntary_ctxt_switches:\t500
nonvoluntary_ctxt_switches:\t50
";
        let status = parse_proc_status(content).unwrap();

        assert_eq!(status.name, "bash");
        assert_eq!(status.pid, 1234);
        assert_eq!(status.ppid, 1233);
        assert_eq!(status.uid, 1000);
        assert_eq!(status.gid, 1000);
        assert_eq!(status.vm_size, 25000);
        assert_eq!(status.vm_rss, 8000);
        assert_eq!(status.voluntary_ctxt_switches, 500);
        assert_eq!(status.nonvoluntary_ctxt_switches, 50);
    }

    #[test]
    fn test_parse_proc_io() {
        let content = "\
rchar: 1000000
wchar: 500000
syscr: 5000
syscw: 2500
read_bytes: 100000
write_bytes: 50000
cancelled_write_bytes: 1000
";
        let io = parse_proc_io(content).unwrap();

        assert_eq!(io.rchar, 1000000);
        assert_eq!(io.wchar, 500000);
        assert_eq!(io.syscr, 5000);
        assert_eq!(io.syscw, 2500);
        assert_eq!(io.read_bytes, 100000);
        assert_eq!(io.write_bytes, 50000);
        assert_eq!(io.cancelled_write_bytes, 1000);
    }

    #[test]
    fn test_parse_meminfo() {
        let content = "\
MemTotal:       16384000 kB
MemFree:         8192000 kB
MemAvailable:   12000000 kB
Buffers:          512000 kB
Cached:          2048000 kB
SwapTotal:       4096000 kB
SwapFree:        4096000 kB
Dirty:              1024 kB
Slab:             512000 kB
SReclaimable:     256000 kB
";
        let info = parse_meminfo(content).unwrap();

        assert_eq!(info.mem_total, 16384000);
        assert_eq!(info.mem_free, 8192000);
        assert_eq!(info.mem_available, 12000000);
        assert_eq!(info.buffers, 512000);
        assert_eq!(info.cached, 2048000);
        assert_eq!(info.swap_total, 4096000);
        assert_eq!(info.slab, 512000);
    }

    #[test]
    fn test_parse_global_stat() {
        let content = "\
cpu  10000 500 3000 80000 1000 200 100 0 0 0
cpu0 2500 125 750 20000 250 50 25 0 0 0
cpu1 2500 125 750 20000 250 50 25 0 0 0
ctxt 500000
btime 1700000000
processes 10000
procs_running 2
procs_blocked 0
";
        let stat = parse_global_stat(content).unwrap();

        assert_eq!(stat.cpus.len(), 3); // cpu + cpu0 + cpu1
        assert_eq!(stat.cpus[0].cpu_id, None); // aggregate
        assert_eq!(stat.cpus[0].user, 10000);
        assert_eq!(stat.cpus[1].cpu_id, Some(0));
        assert_eq!(stat.cpus[2].cpu_id, Some(1));
        assert_eq!(stat.ctxt, 500000);
        assert_eq!(stat.btime, 1700000000);
        assert_eq!(stat.processes, 10000);
        assert_eq!(stat.procs_running, 2);
    }

    #[test]
    fn test_parse_loadavg() {
        let content = "0.15 0.10 0.05 1/150 1234\n";
        let load = parse_loadavg(content).unwrap();

        assert!((load.load1 - 0.15).abs() < 0.001);
        assert!((load.load5 - 0.10).abs() < 0.001);
        assert!((load.load15 - 0.05).abs() < 0.001);
        assert_eq!(load.running, 1);
        assert_eq!(load.total, 150);
        assert_eq!(load.last_pid, 1234);
    }

    #[test]
    fn test_parse_diskstats() {
        let content = "\
   8       0 sda 1234 0 56789 100 5678 0 98765 200 0 150 300 0 0 0 0
   8       1 sda1 1000 0 50000 80 5000 0 90000 180 0 130 260 0 0 0 0
 259       0 nvme0n1 9999 0 123456 500 8888 0 654321 400 5 1000 2000 0 0 0 0
";
        let disks = parse_diskstats(content).unwrap();

        assert_eq!(disks.len(), 3);

        assert_eq!(disks[0].major, 8);
        assert_eq!(disks[0].minor, 0);
        assert_eq!(disks[0].device, "sda");
        assert_eq!(disks[0].reads, 1234);
        assert_eq!(disks[0].read_sectors, 56789);
        assert_eq!(disks[0].writes, 5678);
        assert_eq!(disks[0].write_sectors, 98765);
        assert_eq!(disks[0].io_time, 150);
        assert_eq!(disks[0].io_weighted_time, 300);

        assert_eq!(disks[2].major, 259);
        assert_eq!(disks[2].minor, 0);
        assert_eq!(disks[2].device, "nvme0n1");
        assert_eq!(disks[2].reads, 9999);
        assert_eq!(disks[2].io_in_progress, 5);
    }

    #[test]
    fn test_parse_mountinfo_device_ids() {
        let content = "\
36 35 8:1 / / rw,relatime - ext4 /dev/sda1 rw\n\
37 35 0:123 / /proc rw,nosuid,nodev,noexec,relatime - proc proc rw\n\
38 35 259:0 / /data rw,relatime - xfs /dev/nvme0n1 rw\n";

        let devices = parse_mountinfo_device_ids(content);

        assert!(devices.contains(&(8, 1)));
        assert!(devices.contains(&(259, 0)));

        // pseudo device ids (major == 0) must be ignored
        assert!(!devices.contains(&(0, 123)));
    }

    #[test]
    fn test_parse_net_dev() {
        let content = "\
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 1234567     1234    0    0    0     0          0         0  1234567     1234    0    0    0     0       0          0
  eth0: 9876543     5678    1    2    0     0          0        10 87654321     4321    3    4    0     0       0          0
";
        let devices = parse_net_dev(content).unwrap();

        assert_eq!(devices.len(), 2);

        assert_eq!(devices[0].interface, "lo");
        assert_eq!(devices[0].rx_bytes, 1234567);
        assert_eq!(devices[0].rx_packets, 1234);
        assert_eq!(devices[0].rx_errs, 0);
        assert_eq!(devices[0].tx_bytes, 1234567);
        assert_eq!(devices[0].tx_packets, 1234);

        assert_eq!(devices[1].interface, "eth0");
        assert_eq!(devices[1].rx_bytes, 9876543);
        assert_eq!(devices[1].rx_errs, 1);
        assert_eq!(devices[1].rx_drop, 2);
        assert_eq!(devices[1].tx_bytes, 87654321);
        assert_eq!(devices[1].tx_errs, 3);
        assert_eq!(devices[1].tx_drop, 4);
    }

    #[test]
    fn test_parse_psi_cpu() {
        // CPU PSI only has "some" line
        let content = "some avg10=1.50 avg60=2.25 avg300=3.00 total=12345678\n";
        let stats = parse_psi(content).unwrap();

        assert!((stats.some_avg10 - 1.50).abs() < 0.01);
        assert!((stats.some_avg60 - 2.25).abs() < 0.01);
        assert!((stats.some_avg300 - 3.00).abs() < 0.01);
        assert_eq!(stats.some_total, 12345678);
        assert!((stats.full_avg10 - 0.0).abs() < 0.01); // No full for CPU
    }

    #[test]
    fn test_parse_psi_memory() {
        // Memory/IO PSI has both "some" and "full" lines
        let content = "\
some avg10=0.50 avg60=1.00 avg300=1.50 total=1000000
full avg10=0.10 avg60=0.20 avg300=0.30 total=500000
";
        let stats = parse_psi(content).unwrap();

        assert!((stats.some_avg10 - 0.50).abs() < 0.01);
        assert!((stats.some_avg60 - 1.00).abs() < 0.01);
        assert_eq!(stats.some_total, 1000000);

        assert!((stats.full_avg10 - 0.10).abs() < 0.01);
        assert!((stats.full_avg60 - 0.20).abs() < 0.01);
        assert_eq!(stats.full_total, 500000);
    }

    #[test]
    fn test_parse_vmstat() {
        let content = "\
pgpgin 123456
pgpgout 654321
pswpin 100
pswpout 200
pgfault 999999
pgmajfault 1234
pgsteal_kswapd 5000
pgsteal_direct 1000
pgscan_kswapd 10000
pgscan_direct 2000
oom_kill 5
other_field 12345
";
        let info = parse_vmstat(content).unwrap();

        assert_eq!(info.pgpgin, 123456);
        assert_eq!(info.pgpgout, 654321);
        assert_eq!(info.pswpin, 100);
        assert_eq!(info.pswpout, 200);
        assert_eq!(info.pgfault, 999999);
        assert_eq!(info.pgmajfault, 1234);
        assert_eq!(info.pgsteal_kswapd, 5000);
        assert_eq!(info.pgsteal_direct, 1000);
        assert_eq!(info.pgscan_kswapd, 10000);
        assert_eq!(info.pgscan_direct, 2000);
        assert_eq!(info.oom_kill, 5);
    }
}
