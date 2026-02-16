//! File tailer for reading new lines from a growing log file.
//!
//! Supports log rotation detection via inode tracking (Linux)
//! and file size comparison.

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Maximum number of lines to read in a single `read_new_lines()` call.
/// Prevents unbounded memory usage if the log file has a huge backlog.
const MAX_LINES_PER_READ: usize = 10_000;

/// Reads new lines appended to a file since the last read position.
///
/// On creation, seeks to the end of the file (does not read old content).
/// On each `read_new_lines()` call, reads from the last position to current EOF.
/// Detects log rotation via inode change or file truncation.
pub struct FileTailer {
    path: PathBuf,
    offset: u64,
    inode: u64,
}

impl FileTailer {
    /// Create a new tailer, starting from the end of the file.
    ///
    /// Returns `Err` if the file does not exist or cannot be stat'd.
    pub fn new(path: PathBuf) -> io::Result<Self> {
        let metadata = fs::metadata(&path)?;
        let inode = get_inode(&metadata);
        let offset = metadata.len();

        Ok(Self {
            path,
            offset,
            inode,
        })
    }

    /// Read new lines appended since the last call.
    ///
    /// If the file was rotated (inode changed or size decreased),
    /// re-opens from the beginning of the new file.
    ///
    /// Returns at most `MAX_LINES_PER_READ` lines per call.
    pub fn read_new_lines(&mut self) -> io::Result<Vec<String>> {
        let metadata = match fs::metadata(&self.path) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // File gone (rotation in progress) — return empty, try next time
                return Ok(Vec::new());
            }
            Err(e) => return Err(e),
        };

        let current_inode = get_inode(&metadata);
        let current_size = metadata.len();

        // Detect rotation: inode changed or file truncated
        if current_inode != self.inode || current_size < self.offset {
            self.inode = current_inode;
            self.offset = 0; // Read from beginning of new file
        }

        // Nothing new to read
        if current_size <= self.offset {
            return Ok(Vec::new());
        }

        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(self.offset))?;

        let reader = BufReader::new(&file);
        let mut lines = Vec::new();

        for line_result in reader.lines() {
            let line = line_result?;
            lines.push(line);
            if lines.len() >= MAX_LINES_PER_READ {
                break;
            }
        }

        // Update offset to current position
        self.offset = file.stream_position()?;

        Ok(lines)
    }

    /// Switch to a different file (after log rotation detection from PG).
    ///
    /// Starts reading from the beginning of the new file.
    pub fn switch_file(&mut self, new_path: PathBuf) -> io::Result<()> {
        if new_path == self.path {
            return Ok(());
        }

        let metadata = fs::metadata(&new_path)?;
        self.inode = get_inode(&metadata);
        self.offset = 0; // Read new file from the start
        self.path = new_path;

        Ok(())
    }

    /// Returns the current file path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Extract inode from file metadata (Linux-specific).
#[cfg(unix)]
fn get_inode(metadata: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.ino()
}

/// Fallback for non-Unix: always returns 0, relying on size-based rotation detection.
#[cfg(not(unix))]
fn get_inode(_metadata: &std::fs::Metadata) -> u64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_tailer_new_starts_at_end() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.log");

        // Write some initial content
        std::fs::write(&path, "old line 1\nold line 2\n").unwrap();

        let mut tailer = FileTailer::new(path).unwrap();

        // Should not read old content
        let lines = tailer.read_new_lines().unwrap();
        assert!(lines.is_empty());
    }

    #[test]
    fn test_tailer_reads_new_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.log");

        std::fs::write(&path, "old\n").unwrap();
        let mut tailer = FileTailer::new(path.clone()).unwrap();

        // Append new content
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(f, "new line 1").unwrap();
        writeln!(f, "new line 2").unwrap();
        drop(f);

        let lines = tailer.read_new_lines().unwrap();
        assert_eq!(lines, vec!["new line 1", "new line 2"]);

        // Second call: nothing new
        let lines = tailer.read_new_lines().unwrap();
        assert!(lines.is_empty());
    }

    #[test]
    fn test_tailer_detects_truncation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.log");

        // Write long content to establish high offset
        std::fs::write(&path, "a".repeat(1000)).unwrap();
        let mut tailer = FileTailer::new(path.clone()).unwrap();

        // "Rotate" by truncating the file and writing new shorter content
        std::fs::write(&path, "after rotation\n").unwrap();

        let lines = tailer.read_new_lines().unwrap();
        assert_eq!(lines, vec!["after rotation"]);
    }

    #[test]
    fn test_tailer_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.log");

        std::fs::write(&path, "content\n").unwrap();
        let mut tailer = FileTailer::new(path.clone()).unwrap();

        // Remove the file
        std::fs::remove_file(&path).unwrap();

        // Should return empty, not error
        let lines = tailer.read_new_lines().unwrap();
        assert!(lines.is_empty());
    }

    #[test]
    fn test_tailer_switch_file() {
        let dir = tempfile::tempdir().unwrap();
        let path1 = dir.path().join("log1.log");
        let path2 = dir.path().join("log2.log");

        std::fs::write(&path1, "old file\n").unwrap();
        std::fs::write(&path2, "new file line\n").unwrap();

        let mut tailer = FileTailer::new(path1).unwrap();
        tailer.switch_file(path2.clone()).unwrap();

        let lines = tailer.read_new_lines().unwrap();
        assert_eq!(lines, vec!["new file line"]);
        assert_eq!(tailer.path(), path2);
    }
}
