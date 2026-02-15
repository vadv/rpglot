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
use crate::storage::model::{PgLogEventEntry, PgLogEventType, PgLogSeverity};

use normalize::{MAX_LOG_MESSAGE_LEN, normalize_error};
use parser::{CsvlogParser, EventData, LogEventKind, ParsedLogLine, StderrParser};
use tailer::FileTailer;

/// Result of a log collection cycle.
#[derive(Default)]
pub struct LogCollectResult {
    /// Grouped error entries (ERROR/FATAL/PANIC).
    pub errors: Vec<crate::storage::model::PgLogEntry>,
    /// Number of checkpoint events detected in this interval.
    pub checkpoint_count: u16,
    /// Number of autovacuum/autoanalyze events detected in this interval.
    pub autovacuum_count: u16,
    /// Detailed checkpoint/autovacuum event entries for snapshot storage.
    pub events: Vec<PgLogEventEntry>,
}

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
    /// Accumulated checkpoint event count between snapshots.
    pending_checkpoints: u16,
    /// Accumulated autovacuum/autoanalyze event count between snapshots.
    pending_autovacuums: u16,
    /// Accumulated detailed event entries between snapshots.
    pending_events: Vec<PgLogEventEntry>,
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
            pending_checkpoints: 0,
            pending_autovacuums: 0,
            pending_events: Vec::new(),
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
    ) -> LogCollectResult {
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
                Err(_) => return LogCollectResult::default(),
            },
            None => return LogCollectResult::default(),
        };

        // Parse and accumulate
        for line in &lines {
            let parsed = self.parse_line(line);
            if let Some(parsed) = parsed {
                self.accumulate(parsed);
            }
        }

        // Drain accumulated data
        let errors = self.drain_pending(interner);
        let checkpoint_count = self.pending_checkpoints;
        let autovacuum_count = self.pending_autovacuums;
        let events = std::mem::take(&mut self.pending_events);
        self.pending_checkpoints = 0;
        self.pending_autovacuums = 0;
        LogCollectResult {
            errors,
            checkpoint_count,
            autovacuum_count,
            events,
        }
    }

    /// Parse a single line using the appropriate parser.
    fn parse_line(&self, line: &str) -> Option<ParsedLogLine> {
        match self.log_format {
            Some(LogFormat::Stderr) => self.stderr_parser.as_ref()?.parse_line(line),
            Some(LogFormat::Csvlog) => self.csvlog_parser.as_ref()?.parse_line(line),
            None => None,
        }
    }

    /// Accumulate a parsed log line into the appropriate pending store.
    fn accumulate(&mut self, parsed: ParsedLogLine) {
        match parsed.event_kind {
            LogEventKind::Error => {
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
            LogEventKind::Checkpoint => {
                self.pending_checkpoints = self.pending_checkpoints.saturating_add(1);
                if let Some(event_data) = parsed.event_data {
                    self.pending_events
                        .push(event_data_to_entry(event_data, &parsed.message));
                }
            }
            LogEventKind::Autovacuum => {
                self.pending_autovacuums = self.pending_autovacuums.saturating_add(1);
                if let Some(event_data) = parsed.event_data {
                    self.pending_events
                        .push(event_data_to_entry(event_data, &parsed.message));
                }
            }
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

/// Convert parser `EventData` into storage `PgLogEventEntry`.
fn event_data_to_entry(data: EventData, message: &str) -> PgLogEventEntry {
    match data {
        EventData::CheckpointStarting { .. } => PgLogEventEntry {
            event_type: PgLogEventType::CheckpointStarting,
            message: message.to_string(),
            table_name: String::new(),
            elapsed_s: 0.0,
            extra_num1: 0,
            extra_num2: 0,
            buffer_hits: 0,
            buffer_misses: 0,
            buffer_dirtied: 0,
            avg_read_rate_mbs: 0.0,
            avg_write_rate_mbs: 0.0,
            cpu_user_s: 0.0,
            cpu_system_s: 0.0,
        },
        EventData::CheckpointComplete {
            buffers_written,
            total_time_ms,
            distance_kb,
            ..
        } => PgLogEventEntry {
            event_type: PgLogEventType::CheckpointComplete,
            message: message.to_string(),
            table_name: String::new(),
            elapsed_s: total_time_ms / 1000.0,
            extra_num1: buffers_written,
            extra_num2: distance_kb,
            buffer_hits: 0,
            buffer_misses: 0,
            buffer_dirtied: 0,
            avg_read_rate_mbs: 0.0,
            avg_write_rate_mbs: 0.0,
            cpu_user_s: 0.0,
            cpu_system_s: 0.0,
        },
        EventData::Autovacuum {
            table_name,
            is_analyze,
            tuples_removed,
            pages_removed,
            elapsed_s,
            buffer_hits,
            buffer_misses,
            buffer_dirtied,
            avg_read_rate_mbs,
            avg_write_rate_mbs,
            cpu_user_s,
            cpu_system_s,
        } => PgLogEventEntry {
            event_type: if is_analyze {
                PgLogEventType::Autoanalyze
            } else {
                PgLogEventType::Autovacuum
            },
            message: message.to_string(),
            table_name,
            elapsed_s,
            extra_num1: tuples_removed,
            extra_num2: pages_removed,
            buffer_hits,
            buffer_misses,
            buffer_dirtied,
            avg_read_rate_mbs,
            avg_write_rate_mbs,
            cpu_user_s,
            cpu_system_s,
        },
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
    use parser::LogEventKind;

    #[test]
    fn test_accumulate_and_drain() {
        let mut collector = LogCollector::new();
        let mut interner = StringInterner::new();

        // Accumulate several errors
        collector.accumulate(ParsedLogLine {
            severity: PgLogSeverity::Error,
            message: "relation \"users\" does not exist".to_string(),
            event_kind: LogEventKind::Error,
            event_data: None,
        });
        collector.accumulate(ParsedLogLine {
            severity: PgLogSeverity::Error,
            message: "relation \"orders\" does not exist".to_string(),
            event_kind: LogEventKind::Error,
            event_data: None,
        });
        collector.accumulate(ParsedLogLine {
            severity: PgLogSeverity::Fatal,
            message: "database \"mydb\" does not exist".to_string(),
            event_kind: LogEventKind::Error,
            event_data: None,
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
                event_kind: LogEventKind::Error,
                event_data: None,
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
            event_kind: LogEventKind::Error,
            event_data: None,
        });

        let entries = collector.drain_pending(&mut interner);
        assert_eq!(entries.len(), 1);

        // Second drain should be empty
        let entries = collector.drain_pending(&mut interner);
        assert!(entries.is_empty());
    }
}
