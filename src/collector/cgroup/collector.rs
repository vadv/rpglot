//! Cgroup v2 metrics collector.

use std::path::PathBuf;

use crate::collector::traits::FileSystem;
use crate::storage::model::{
    CgroupCpuInfo, CgroupInfo, CgroupIoInfo, CgroupMemoryInfo, CgroupPidsInfo,
};

use super::parser;

/// Collector for cgroup v2 metrics.
///
/// Collects CPU, memory, and PIDs metrics from the cgroup v2 filesystem.
/// Only used when running inside a container.
pub struct CgroupCollector<F: FileSystem> {
    fs: F,
    cgroup_path: PathBuf,
}

impl<F: FileSystem> CgroupCollector<F> {
    /// Creates a new CgroupCollector.
    ///
    /// # Arguments
    /// * `fs` - Filesystem implementation
    /// * `cgroup_path` - Path to cgroup directory (e.g., "/sys/fs/cgroup")
    pub fn new(fs: F, cgroup_path: &str) -> Self {
        Self {
            fs,
            cgroup_path: PathBuf::from(cgroup_path),
        }
    }

    /// Collects all available cgroup metrics.
    ///
    /// Returns `None` if no cgroup data is available.
    pub fn collect(&self) -> Option<CgroupInfo> {
        let cpu = self.collect_cpu();
        let memory = self.collect_memory();
        let pids = self.collect_pids();
        let io = self.collect_io();

        if cpu.is_none() && memory.is_none() && pids.is_none() && io.is_empty() {
            return None;
        }

        Some(CgroupInfo {
            cpu,
            memory,
            pids,
            io,
        })
    }

    /// Collects CPU cgroup metrics.
    fn collect_cpu(&self) -> Option<CgroupCpuInfo> {
        let cpu_max_path = self.cgroup_path.join("cpu.max");
        let cpu_stat_path = self.cgroup_path.join("cpu.stat");

        let cpu_stat_content = self.fs.read_to_string(&cpu_stat_path).ok()?;
        let mut info = parser::parse_cpu_stat(&cpu_stat_content);

        if let Ok(cpu_max_content) = self.fs.read_to_string(&cpu_max_path) {
            let (quota, period) = parser::parse_cpu_max(&cpu_max_content);
            info.quota = quota;
            info.period = period;
        }

        Some(info)
    }

    /// Collects memory cgroup metrics.
    fn collect_memory(&self) -> Option<CgroupMemoryInfo> {
        let memory_current_path = self.cgroup_path.join("memory.current");

        let current_content = self.fs.read_to_string(&memory_current_path).ok()?;
        let current = parser::parse_memory_current(&current_content);

        let mut info = CgroupMemoryInfo {
            current,
            ..Default::default()
        };

        if let Ok(max_content) = self.fs.read_to_string(&self.cgroup_path.join("memory.max")) {
            info.max = parser::parse_memory_max(&max_content);
        }

        if let Ok(stat_content) = self
            .fs
            .read_to_string(&self.cgroup_path.join("memory.stat"))
        {
            parser::parse_memory_stat(&stat_content, &mut info);
        }

        if let Ok(events_content) = self
            .fs
            .read_to_string(&self.cgroup_path.join("memory.events"))
        {
            parser::parse_memory_events(&events_content, &mut info);
        }

        Some(info)
    }

    /// Collects PIDs cgroup metrics.
    fn collect_pids(&self) -> Option<CgroupPidsInfo> {
        let pids_current_path = self.cgroup_path.join("pids.current");

        let current_content = self.fs.read_to_string(&pids_current_path).ok()?;
        let current = parser::parse_pids_current(&current_content);

        let mut info = CgroupPidsInfo {
            current,
            ..Default::default()
        };

        if let Ok(max_content) = self.fs.read_to_string(&self.cgroup_path.join("pids.max")) {
            info.max = parser::parse_pids_max(&max_content);
        }

        Some(info)
    }

    /// Collects I/O cgroup metrics.
    fn collect_io(&self) -> Vec<CgroupIoInfo> {
        let io_stat_path = self.cgroup_path.join("io.stat");
        let Ok(content) = self.fs.read_to_string(&io_stat_path) else {
            return Vec::new();
        };

        parser::parse_io_stat(&content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::MockFs;

    fn create_mock_cgroup_fs() -> MockFs {
        let mut fs = MockFs::new();

        fs.add_file("/sys/fs/cgroup/cpu.max", "100000 100000\n");
        fs.add_file(
            "/sys/fs/cgroup/cpu.stat",
            "usage_usec 5000000\nuser_usec 3000000\nsystem_usec 2000000\nthrottled_usec 1000\nnr_throttled 5\n",
        );
        fs.add_file("/sys/fs/cgroup/memory.max", "1073741824\n");
        fs.add_file("/sys/fs/cgroup/memory.current", "536870912\n");
        fs.add_file(
            "/sys/fs/cgroup/memory.stat",
            "anon 100000000\nfile 200000000\nkernel 50000000\nslab 25000000\n",
        );
        fs.add_file(
            "/sys/fs/cgroup/memory.events",
            "low 0\nhigh 0\nmax 0\noom 0\noom_kill 0\n",
        );
        fs.add_file("/sys/fs/cgroup/pids.current", "42\n");
        fs.add_file("/sys/fs/cgroup/pids.max", "1000\n");
        fs.add_file(
            "/sys/fs/cgroup/io.stat",
            "8:0 rbytes=123 wbytes=456 rios=7 wios=8\n",
        );

        fs
    }

    #[test]
    fn test_collect_all() {
        let fs = create_mock_cgroup_fs();
        let collector = CgroupCollector::new(fs, "/sys/fs/cgroup");

        let info = collector.collect().expect("should collect cgroup info");

        let cpu = info.cpu.expect("should have cpu info");
        assert_eq!(cpu.quota, 100_000);
        assert_eq!(cpu.period, 100_000);
        assert_eq!(cpu.usage_usec, 5_000_000);

        let memory = info.memory.expect("should have memory info");
        assert_eq!(memory.max, 1073741824);
        assert_eq!(memory.current, 536870912);

        let pids = info.pids.expect("should have pids info");
        assert_eq!(pids.current, 42);
        assert_eq!(pids.max, 1000);

        assert_eq!(info.io.len(), 1);
        assert_eq!(info.io[0].major, 8);
        assert_eq!(info.io[0].minor, 0);
    }

    #[test]
    fn test_collect_empty_fs() {
        let fs = MockFs::new();
        let collector = CgroupCollector::new(fs, "/sys/fs/cgroup");

        let info = collector.collect();
        assert!(info.is_none());
    }
}
