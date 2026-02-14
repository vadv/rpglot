//! In-memory mock filesystem for testing collectors without real `/proc`.
//!
//! This module provides `MockFs` which simulates a filesystem in memory,
//! allowing tests to run on macOS and in CI environments without Linux.

use crate::collector::traits::FileSystem;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

/// In-memory filesystem for testing.
///
/// Stores files and directories in memory, allowing tests to simulate
/// various `/proc` filesystem states without needing actual Linux access.
#[derive(Debug, Clone, Default)]
pub struct MockFs {
    /// Map from path to file contents.
    files: HashMap<PathBuf, String>,
    /// Set of directories (for read_dir support).
    directories: HashSet<PathBuf>,
}

impl MockFs {
    /// Creates a new empty mock filesystem.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a file with the given content.
    ///
    /// Parent directories are automatically created.
    pub fn add_file(&mut self, path: impl AsRef<Path>, content: impl Into<String>) {
        let path = path.as_ref().to_path_buf();

        // Add parent directories
        let mut parent = path.parent();
        while let Some(p) = parent {
            if !p.as_os_str().is_empty() {
                self.directories.insert(p.to_path_buf());
            }
            parent = p.parent();
        }

        self.files.insert(path, content.into());
    }

    /// Adds an empty directory.
    pub fn add_dir(&mut self, path: impl AsRef<Path>) {
        let path = path.as_ref().to_path_buf();
        self.directories.insert(path.clone());

        // Add parent directories
        let mut parent = path.parent();
        while let Some(p) = parent {
            if !p.as_os_str().is_empty() {
                self.directories.insert(p.to_path_buf());
            }
            parent = p.parent();
        }
    }

    /// Adds a process with all its typical `/proc/[pid]/` files.
    ///
    /// # Arguments
    /// * `pid` - Process ID
    /// * `stat` - Content of `/proc/[pid]/stat`
    /// * `status` - Content of `/proc/[pid]/status`
    /// * `io` - Content of `/proc/[pid]/io` (can be empty if not accessible)
    /// * `cmdline` - Content of `/proc/[pid]/cmdline`
    /// * `comm` - Content of `/proc/[pid]/comm`
    pub fn add_process(
        &mut self,
        pid: u32,
        stat: &str,
        status: &str,
        io: &str,
        cmdline: &str,
        comm: &str,
    ) {
        let base = PathBuf::from(format!("/proc/{}", pid));
        self.add_dir(&base);
        self.add_file(base.join("stat"), stat);
        self.add_file(base.join("status"), status);
        if !io.is_empty() {
            self.add_file(base.join("io"), io);
        }
        self.add_file(base.join("cmdline"), cmdline);
        self.add_file(base.join("comm"), comm);
    }

    /// Loads a mock filesystem from a directory snapshot.
    ///
    /// This is useful for regression tests with real `/proc` snapshots.
    pub fn from_snapshot(dir: &Path) -> io::Result<Self> {
        let mut fs = Self::new();
        load_directory_recursive(&mut fs, dir, Path::new("/proc"))?;
        Ok(fs)
    }
}

fn load_directory_recursive(
    fs: &mut MockFs,
    real_path: &Path,
    virtual_path: &Path,
) -> io::Result<()> {
    fs.add_dir(virtual_path);

    for entry in std::fs::read_dir(real_path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name();
        let real_child = entry.path();
        let virtual_child = virtual_path.join(&name);

        if file_type.is_dir() {
            load_directory_recursive(fs, &real_child, &virtual_child)?;
        } else if file_type.is_file() {
            // Try to read as string, skip binary files
            if let Ok(content) = std::fs::read_to_string(&real_child) {
                fs.add_file(&virtual_child, content);
            }
        }
    }
    Ok(())
}

impl FileSystem for MockFs {
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        self.files.get(path).cloned().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("file not found: {:?}", path),
            )
        })
    }

    fn exists(&self, path: &Path) -> bool {
        self.files.contains_key(path) || self.directories.contains(path)
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        if !self.directories.contains(path) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("directory not found: {:?}", path),
            ));
        }

        let mut entries = HashSet::new();

        // Find all files and directories that are direct children
        for file_path in self.files.keys() {
            if file_path.parent().is_some_and(|parent| parent == path) {
                entries.insert(file_path.clone());
            }
        }

        for dir_path in &self.directories {
            if dir_path.parent().is_some_and(|parent| parent == path) && dir_path != path {
                entries.insert(dir_path.clone());
            }
        }

        Ok(entries.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_fs_add_file() {
        let mut fs = MockFs::new();
        fs.add_file("/proc/meminfo", "MemTotal: 16384 kB\n");

        assert!(fs.exists(Path::new("/proc/meminfo")));
        assert!(fs.exists(Path::new("/proc")));

        let content = fs.read_to_string(Path::new("/proc/meminfo")).unwrap();
        assert_eq!(content, "MemTotal: 16384 kB\n");
    }

    #[test]
    fn test_mock_fs_read_dir() {
        let mut fs = MockFs::new();
        fs.add_file("/proc/1/stat", "stat content");
        fs.add_file("/proc/1/status", "status content");
        fs.add_file("/proc/2/stat", "stat content 2");

        let proc_entries = fs.read_dir(Path::new("/proc")).unwrap();
        assert_eq!(proc_entries.len(), 2); // /proc/1 and /proc/2

        let proc1_entries = fs.read_dir(Path::new("/proc/1")).unwrap();
        assert_eq!(proc1_entries.len(), 2); // stat and status
    }

    #[test]
    fn test_mock_fs_add_process() {
        let mut fs = MockFs::new();
        fs.add_process(
            1234,
            "1234 (bash) S 1233 1234 1234 0 -1 4194304 100 0 0 0 10 5 0 0 20 0 1 0 12345 12345678 100 18446744073709551615 0 0 0 0 0 0 0 0 0 0 0 0 17 0 0 0 0 0 0 0 0 0 0 0 0 0 0",
            "Name:\tbash\nPid:\t1234\nPPid:\t1233\nUid:\t1000\t1000\t1000\t1000\nGid:\t1000\t1000\t1000\t1000\n",
            "rchar: 1000\nwchar: 500\nsyscr: 100\nsyscw: 50\nread_bytes: 4096\nwrite_bytes: 2048\ncancelled_write_bytes: 0\n",
            "/bin/bash\0--login\0",
            "bash\n",
        );

        assert!(fs.exists(Path::new("/proc/1234")));
        assert!(fs.exists(Path::new("/proc/1234/stat")));
        assert!(fs.exists(Path::new("/proc/1234/status")));
        assert!(fs.exists(Path::new("/proc/1234/io")));
        assert!(fs.exists(Path::new("/proc/1234/cmdline")));
        assert!(fs.exists(Path::new("/proc/1234/comm")));
    }

    #[test]
    fn test_mock_fs_not_found() {
        let fs = MockFs::new();
        let result = fs.read_to_string(Path::new("/nonexistent"));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
    }
}
