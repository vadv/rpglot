//! Abstractions for filesystem access to enable testing and mocking.
//!
//! The `FileSystem` trait allows the collector to work with both real `/proc`
//! filesystem on Linux and mock implementations for testing on macOS or in CI.

use std::io;
use std::path::{Path, PathBuf};

/// Abstraction for filesystem operations.
///
/// This trait allows collectors to read from the real filesystem or from
/// a mock implementation for testing purposes.
#[allow(dead_code)]
pub trait FileSystem: Send + Sync {
    /// Reads the entire contents of a file as a string.
    ///
    /// # Arguments
    /// * `path` - Path to the file to read
    ///
    /// # Returns
    /// The file contents as a string, or an I/O error if the file cannot be read.
    fn read_to_string(&self, path: &Path) -> io::Result<String>;

    /// Checks if a path exists.
    ///
    /// # Arguments
    /// * `path` - Path to check
    ///
    /// # Returns
    /// `true` if the path exists, `false` otherwise.
    fn exists(&self, path: &Path) -> bool;

    /// Lists entries in a directory.
    ///
    /// # Arguments
    /// * `path` - Path to the directory
    ///
    /// # Returns
    /// A vector of paths to entries in the directory, or an I/O error.
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;
}

/// Real filesystem implementation that delegates to `std::fs`.
///
/// Use this in production to read from the actual `/proc` filesystem.
#[allow(dead_code)]
#[derive(Debug, Default, Clone, Copy)]
pub struct RealFs;

#[allow(dead_code)]
impl RealFs {
    /// Creates a new `RealFs` instance.
    pub fn new() -> Self {
        Self
    }
}

impl FileSystem for RealFs {
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let entries = std::fs::read_dir(path)?;
        let mut paths = Vec::new();
        for entry in entries {
            paths.push(entry?.path());
        }
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_real_fs_read_to_string() {
        let fs = RealFs::new();
        // Read Cargo.toml which should exist in project root
        let cargo_toml = env::current_dir().unwrap().join("Cargo.toml");
        let content = fs.read_to_string(&cargo_toml).unwrap();
        assert!(content.contains("[package]"));
    }

    #[test]
    fn test_real_fs_exists() {
        let fs = RealFs::new();
        let cargo_toml = env::current_dir().unwrap().join("Cargo.toml");
        assert!(fs.exists(&cargo_toml));
        assert!(!fs.exists(Path::new("/nonexistent/path/12345")));
    }

    #[test]
    fn test_real_fs_read_dir() {
        let fs = RealFs::new();
        let src_dir = env::current_dir().unwrap().join("src");
        let entries = fs.read_dir(&src_dir).unwrap();
        // Should contain at least main.rs and storage directory
        assert!(!entries.is_empty());
    }
}
