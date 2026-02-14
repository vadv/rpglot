//! Parsers for cgroup v2 files.

use crate::storage::model::{CgroupCpuInfo, CgroupIoInfo, CgroupMemoryInfo};

/// Parses cpu.max file.
/// Format: "quota period" or "max period"
/// Example: "100000 100000" or "max 100000"
pub fn parse_cpu_max(content: &str) -> (i64, u64) {
    let parts: Vec<&str> = content.split_whitespace().collect();
    if parts.len() < 2 {
        return (-1, 100_000);
    }

    let quota = if parts[0] == "max" {
        -1
    } else {
        parts[0].parse().unwrap_or(-1)
    };

    let period = parts[1].parse().unwrap_or(100_000);

    (quota, period)
}

/// Parses cpu.stat file into CgroupCpuInfo (partial, needs cpu.max data).
/// Format: key value pairs, one per line
pub fn parse_cpu_stat(content: &str) -> CgroupCpuInfo {
    let mut info = CgroupCpuInfo::default();

    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let value: u64 = parts[1].parse().unwrap_or(0);

        match parts[0] {
            "usage_usec" => info.usage_usec = value,
            "user_usec" => info.user_usec = value,
            "system_usec" => info.system_usec = value,
            "throttled_usec" => info.throttled_usec = value,
            "nr_throttled" => info.nr_throttled = value,
            _ => {}
        }
    }

    info
}

/// Parses memory.max file.
/// Format: number or "max"
pub fn parse_memory_max(content: &str) -> u64 {
    let trimmed = content.trim();
    if trimmed == "max" {
        u64::MAX
    } else {
        trimmed.parse().unwrap_or(u64::MAX)
    }
}

/// Parses memory.current file.
/// Format: number (bytes)
pub fn parse_memory_current(content: &str) -> u64 {
    content.trim().parse().unwrap_or(0)
}

/// Parses memory.stat file (partial fields).
/// Format: key value pairs, one per line
pub fn parse_memory_stat(content: &str, info: &mut CgroupMemoryInfo) {
    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let value: u64 = parts[1].parse().unwrap_or(0);

        match parts[0] {
            "anon" => info.anon = value,
            "file" => info.file = value,
            "kernel" => info.kernel = value,
            "slab" => info.slab = value,
            _ => {}
        }
    }
}

/// Parses memory.events file.
/// Format: key value pairs, one per line
pub fn parse_memory_events(content: &str, info: &mut CgroupMemoryInfo) {
    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        if parts[0] == "oom_kill" {
            info.oom_kill = parts[1].parse().unwrap_or(0);
        }
    }
}

/// Parses pids.current file.
/// Format: number
pub fn parse_pids_current(content: &str) -> u64 {
    content.trim().parse().unwrap_or(0)
}

/// Parses pids.max file.
/// Format: number or "max"
pub fn parse_pids_max(content: &str) -> u64 {
    let trimmed = content.trim();
    if trimmed == "max" {
        u64::MAX
    } else {
        trimmed.parse().unwrap_or(u64::MAX)
    }
}

/// Parses io.stat file.
///
/// Format: one device per line:
/// `MAJOR:MINOR rbytes=.. wbytes=.. rios=.. wios=.. [other fields...]`
pub fn parse_io_stat(content: &str) -> Vec<CgroupIoInfo> {
    let mut devices = Vec::new();

    for line in content.lines() {
        let mut parts = line.split_whitespace();
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

        let mut info = CgroupIoInfo {
            major,
            minor,
            ..Default::default()
        };

        for kv in parts {
            let Some((k, v)) = kv.split_once('=') else {
                continue;
            };
            let value: u64 = v.parse().unwrap_or(0);
            match k {
                "rbytes" => info.rbytes = value,
                "wbytes" => info.wbytes = value,
                "rios" => info.rios = value,
                "wios" => info.wios = value,
                _ => {}
            }
        }

        devices.push(info);
    }

    devices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cpu_max_with_quota() {
        let (quota, period) = parse_cpu_max("100000 100000\n");
        assert_eq!(quota, 100_000);
        assert_eq!(period, 100_000);
    }

    #[test]
    fn test_parse_cpu_max_unlimited() {
        let (quota, period) = parse_cpu_max("max 100000\n");
        assert_eq!(quota, -1);
        assert_eq!(period, 100_000);
    }

    #[test]
    fn test_parse_cpu_stat() {
        let content = "usage_usec 123456\nuser_usec 100000\nsystem_usec 23456\nthrottled_usec 500\nnr_throttled 2\n";
        let info = parse_cpu_stat(content);
        assert_eq!(info.usage_usec, 123456);
        assert_eq!(info.user_usec, 100000);
        assert_eq!(info.system_usec, 23456);
        assert_eq!(info.throttled_usec, 500);
        assert_eq!(info.nr_throttled, 2);
    }

    #[test]
    fn test_parse_memory_max() {
        assert_eq!(parse_memory_max("1073741824\n"), 1073741824);
        assert_eq!(parse_memory_max("max\n"), u64::MAX);
    }

    #[test]
    fn test_parse_memory_current() {
        assert_eq!(parse_memory_current("536870912\n"), 536870912);
    }

    #[test]
    fn test_parse_memory_stat() {
        let content = "anon 100000\nfile 200000\nkernel 50000\nslab 25000\n";
        let mut info = CgroupMemoryInfo::default();
        parse_memory_stat(content, &mut info);
        assert_eq!(info.anon, 100000);
        assert_eq!(info.file, 200000);
        assert_eq!(info.kernel, 50000);
        assert_eq!(info.slab, 25000);
    }

    #[test]
    fn test_parse_memory_events() {
        let content = "low 0\nhigh 0\nmax 0\noom 0\noom_kill 3\n";
        let mut info = CgroupMemoryInfo::default();
        parse_memory_events(content, &mut info);
        assert_eq!(info.oom_kill, 3);
    }

    #[test]
    fn test_parse_pids() {
        assert_eq!(parse_pids_current("42\n"), 42);
        assert_eq!(parse_pids_max("100\n"), 100);
        assert_eq!(parse_pids_max("max\n"), u64::MAX);
    }

    #[test]
    fn test_parse_io_stat() {
        let content = "8:0 rbytes=123 wbytes=456 rios=7 wios=8 dbytes=0 dios=0\n8:16 rbytes=0 wbytes=1 rios=0 wios=2\n";
        let parsed = parse_io_stat(content);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].major, 8);
        assert_eq!(parsed[0].minor, 0);
        assert_eq!(parsed[0].rbytes, 123);
        assert_eq!(parsed[0].wbytes, 456);
        assert_eq!(parsed[0].rios, 7);
        assert_eq!(parsed[0].wios, 8);

        assert_eq!(parsed[1].major, 8);
        assert_eq!(parsed[1].minor, 16);
        assert_eq!(parsed[1].wbytes, 1);
        assert_eq!(parsed[1].wios, 2);
    }
}
