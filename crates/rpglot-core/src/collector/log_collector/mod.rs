//! PostgreSQL log file collector.
//!
//! Reads PostgreSQL log files (stderr or csvlog), parses ERROR/FATAL/PANIC
//! entries, normalizes messages into patterns, and groups them for storage
//! in snapshots.

pub mod normalize;
pub mod parser;
pub mod tailer;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use postgres::Client;

use crate::storage::interner::StringInterner;
use crate::storage::model::PgLogSeverity;

use normalize::{MAX_LOG_MESSAGE_LEN, normalize_error};
use parser::{CsvlogParser, ParsedLogLine, StderrParser};
use tailer::FileTailer;

/// Maximum number of unique error patterns kept per snapshot interval.
const MAX_LOG_PATTERNS_PER_SNAPSHOT: usize = 32;

/// How often to re-check pg_current_logfile() for rotation (seconds).
const LOG_ROTATION_CHECK_SECS: u64 = 60;

/// How often to re-read PG log settings (seconds).
const SETTINGS_REFRESH_SECS: u64 = 600;

/// Accumulated error during collection interval.
struct PendingError {
    count: u32,
    sample: String,
}

/// Log format detected from `log_destination` setting.
#[derive(Debug, Clone, Copy, PartialEq)]
enum LogFormat {
    Stderr,
    Csvlog,
}

/// Collects PostgreSQL ERROR/FATAL/PANIC log entries.
///
/// Reads the current log file via tail, parses error lines, normalizes
/// messages into patterns, and returns grouped entries for each snapshot.
pub struct LogCollector {
    tailer: Option<FileTailer>,
    stderr_parser: Option<StderrParser>,
    csvlog_parser: Option<CsvlogParser>,
    log_format: Option<LogFormat>,
    /// Cached PG settings
    data_directory: Option<String>,
    log_directory: Option<String>,
    log_line_prefix: Option<String>,
    /// Last time we refreshed settings from PG
    settings_last_check: Option<Instant>,
    /// Last time we checked pg_current_logfile()
    rotation_last_check: Option<Instant>,
    /// Accumulated errors between snapshot collections
    pending_errors: HashMap<(String, PgLogSeverity), PendingError>,
    /// Last initialization error (for diagnostics)
    last_error: Option<String>,
}

impl Default for LogCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl LogCollector {
    /// Create a new uninitialized log collector.
    pub fn new() -> Self {
        Self {
            tailer: None,
            stderr_parser: None,
            csvlog_parser: None,
            log_format: None,
            data_directory: None,
            log_directory: None,
            log_line_prefix: None,
            settings_last_check: None,
            rotation_last_check: None,
            pending_errors: HashMap::new(),
            last_error: None,
        }
    }

    /// Initialize the log collector by querying PG settings and locating the log file.
    ///
    /// Should be called after PostgreSQL connection is established.
    /// On failure, sets `last_error` and returns — collection will be skipped.
    pub fn init(&mut self, client: &mut Client) {
        self.last_error = None;

        // Read PG settings
        let data_directory = show_setting(client, "data_directory");
        let log_directory = show_setting(client, "log_directory");
        let log_line_prefix = show_setting(client, "log_line_prefix");
        let log_destination = show_setting(client, "log_destination");

        self.data_directory = data_directory;
        self.log_directory = log_directory;
        self.log_line_prefix = log_line_prefix.clone();
        self.settings_last_check = Some(Instant::now());

        // Determine log format
        let dest = log_destination.as_deref().unwrap_or("stderr");
        self.log_format = if dest.contains("csvlog") {
            Some(LogFormat::Csvlog)
        } else {
            Some(LogFormat::Stderr)
        };

        // Build parser
        match self.log_format {
            Some(LogFormat::Stderr) => {
                let prefix = log_line_prefix.as_deref().unwrap_or("");
                self.stderr_parser = Some(StderrParser::new(prefix));
                self.csvlog_parser = None;
            }
            Some(LogFormat::Csvlog) => {
                self.csvlog_parser = Some(CsvlogParser);
                self.stderr_parser = None;
            }
            None => {}
        }

        // Locate current log file
        if let Err(e) = self.locate_log_file(client) {
            self.last_error = Some(e);
        }

        self.rotation_last_check = Some(Instant::now());
    }

    /// Collect new error entries from the log file.
    ///
    /// Reads new lines, parses them, normalizes messages, groups by pattern,
    /// and returns entries for the current snapshot.
    ///
    /// `client` is needed for periodic log rotation checks.
    pub fn collect(
        &mut self,
        client: &mut Client,
        interner: &mut StringInterner,
    ) -> Vec<crate::storage::model::PgLogEntry> {
        // Periodic rotation check
        if let Some(last) = self.rotation_last_check
            && last.elapsed().as_secs() >= LOG_ROTATION_CHECK_SECS
        {
            let _ = self.check_log_rotation(client);
            self.rotation_last_check = Some(Instant::now());
        }

        // Periodic settings refresh
        if let Some(last) = self.settings_last_check
            && last.elapsed().as_secs() >= SETTINGS_REFRESH_SECS
        {
            self.init(client);
        }

        // Read new lines from log file
        let lines = match &mut self.tailer {
            Some(tailer) => match tailer.read_new_lines() {
                Ok(lines) => lines,
                Err(_) => return Vec::new(),
            },
            None => return Vec::new(),
        };

        // Parse and accumulate errors
        for line in &lines {
            let parsed = self.parse_line(line);
            if let Some(parsed) = parsed {
                self.accumulate(parsed);
            }
        }

        // Drain accumulated errors into PgLogEntry vec
        self.drain_pending(interner)
    }

    /// Parse a single line using the appropriate parser.
    fn parse_line(&self, line: &str) -> Option<ParsedLogLine> {
        match self.log_format {
            Some(LogFormat::Stderr) => self.stderr_parser.as_ref()?.parse_line(line),
            Some(LogFormat::Csvlog) => self.csvlog_parser.as_ref()?.parse_line(line),
            None => None,
        }
    }

    /// Accumulate a parsed error into pending_errors.
    fn accumulate(&mut self, parsed: ParsedLogLine) {
        let normalized = normalize_error(&parsed.message);
        let key = (normalized, parsed.severity);

        let entry = self.pending_errors.entry(key).or_insert(PendingError {
            count: 0,
            sample: String::new(),
        });
        entry.count += 1;
        // Keep first sample only
        if entry.sample.is_empty() {
            let mut sample = parsed.message;
            sample.truncate(MAX_LOG_MESSAGE_LEN);
            entry.sample = sample;
        }
    }

    /// Drain pending errors into PgLogEntry vec, applying limits.
    fn drain_pending(
        &mut self,
        interner: &mut StringInterner,
    ) -> Vec<crate::storage::model::PgLogEntry> {
        if self.pending_errors.is_empty() {
            return Vec::new();
        }

        let mut entries: Vec<_> = self.pending_errors.drain().collect();

        // Sort by count descending — keep top patterns
        entries.sort_by(|a, b| b.1.count.cmp(&a.1.count));
        entries.truncate(MAX_LOG_PATTERNS_PER_SNAPSHOT);

        entries
            .into_iter()
            .map(|((pattern, severity), pending)| {
                let mut pattern_str = pattern;
                pattern_str.truncate(MAX_LOG_MESSAGE_LEN);

                crate::storage::model::PgLogEntry {
                    pattern_hash: interner.intern(&pattern_str),
                    severity,
                    count: pending.count,
                    sample_hash: interner.intern(&pending.sample),
                }
            })
            .collect()
    }

    /// Locate the current PG log file using `pg_current_logfile()`.
    fn locate_log_file(&mut self, client: &mut Client) -> Result<(), String> {
        // Try pg_current_logfile() first (PG 10+)
        let log_path = query_current_logfile(client, self.log_format);

        let log_path = match log_path {
            Some(p) => p,
            None => return Err("pg_current_logfile() returned no result".to_string()),
        };

        // Resolve relative paths against data_directory
        let full_path = if PathBuf::from(&log_path).is_absolute() {
            PathBuf::from(log_path)
        } else {
            match &self.data_directory {
                Some(dd) => PathBuf::from(dd).join(log_path),
                None => PathBuf::from(log_path),
            }
        };

        match &mut self.tailer {
            Some(tailer) => {
                if tailer.path() != full_path {
                    tailer
                        .switch_file(full_path)
                        .map_err(|e| format!("switch_file: {}", e))?;
                }
            }
            None => {
                self.tailer = Some(
                    FileTailer::new(full_path).map_err(|e| format!("FileTailer::new: {}", e))?,
                );
            }
        }

        Ok(())
    }

    /// Check if the log file has rotated by re-querying pg_current_logfile().
    fn check_log_rotation(&mut self, client: &mut Client) -> Result<(), String> {
        self.locate_log_file(client)
    }

    /// Returns the last error message, if any.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }
}

/// Execute `SHOW <setting>` and return the value.
fn show_setting(client: &mut Client, name: &str) -> Option<String> {
    let query = format!("SHOW {}", name);
    client
        .query_one(&query as &str, &[])
        .ok()
        .and_then(|row| row.try_get::<_, String>(0).ok())
}

/// Query `pg_current_logfile()` for the active log file path.
fn query_current_logfile(client: &mut Client, format: Option<LogFormat>) -> Option<String> {
    let format_arg = match format {
        Some(LogFormat::Csvlog) => "'csvlog'",
        _ => "'stderr'",
    };
    let query = format!("SELECT pg_current_logfile({})", format_arg);
    client
        .query_one(&query as &str, &[])
        .ok()
        .and_then(|row| row.try_get::<_, Option<String>>(0).ok())
        .flatten()
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accumulate_and_drain() {
        let mut collector = LogCollector::new();
        let mut interner = StringInterner::new();

        // Accumulate several errors
        collector.accumulate(ParsedLogLine {
            severity: PgLogSeverity::Error,
            message: "relation \"users\" does not exist".to_string(),
        });
        collector.accumulate(ParsedLogLine {
            severity: PgLogSeverity::Error,
            message: "relation \"orders\" does not exist".to_string(),
        });
        collector.accumulate(ParsedLogLine {
            severity: PgLogSeverity::Fatal,
            message: "database \"mydb\" does not exist".to_string(),
        });

        let entries = collector.drain_pending(&mut interner);

        // "relation "..." does not exist" should be grouped (count=2)
        assert_eq!(entries.len(), 2);

        // Find the grouped error
        let relation_entry = entries
            .iter()
            .find(|e| e.severity == PgLogSeverity::Error)
            .unwrap();
        assert_eq!(relation_entry.count, 2);

        let fatal_entry = entries
            .iter()
            .find(|e| e.severity == PgLogSeverity::Fatal)
            .unwrap();
        assert_eq!(fatal_entry.count, 1);
    }

    #[test]
    fn test_max_patterns_limit() {
        let mut collector = LogCollector::new();
        let mut interner = StringInterner::new();

        // Add more than MAX_LOG_PATTERNS_PER_SNAPSHOT unique patterns
        for i in 0..50 {
            collector.accumulate(ParsedLogLine {
                severity: PgLogSeverity::Error,
                message: format!("unique error number {}", i),
            });
        }

        let entries = collector.drain_pending(&mut interner);
        assert!(entries.len() <= MAX_LOG_PATTERNS_PER_SNAPSHOT);
    }

    #[test]
    fn test_drain_clears_pending() {
        let mut collector = LogCollector::new();
        let mut interner = StringInterner::new();

        collector.accumulate(ParsedLogLine {
            severity: PgLogSeverity::Error,
            message: "some error".to_string(),
        });

        let entries = collector.drain_pending(&mut interner);
        assert_eq!(entries.len(), 1);

        // Second drain should be empty
        let entries = collector.drain_pending(&mut interner);
        assert!(entries.is_empty());
    }
}
