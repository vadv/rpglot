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
    /// SQL statement from STATEMENT: line (first seen).
    statement: String,
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
    /// Index of the last autovacuum/checkpoint event in pending_events.
    /// Used to patch-in metrics from continuation lines (stderr multiline messages).
    /// Reset to None when a non-continuation line arrives.
    last_event_idx: Option<usize>,
    /// Key of the last error in pending_errors.
    /// Used to attach STATEMENT: lines to the preceding error.
    /// Reset to None when a non-DETAIL/CONTEXT/STATEMENT line arrives.
    last_error_key: Option<(String, PgLogSeverity)>,
    /// True if `drain_pending` already held back the last error once.
    /// Prevents holding back the same error indefinitely.
    error_held_back: bool,
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
            last_event_idx: None,
            last_error_key: None,
            error_held_back: false,
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
            // Continuation line (starts with whitespace): try to patch last event in-place
            if is_continuation_line(line) {
                if let Some(idx) = self.last_event_idx
                    && let Some(entry) = self.pending_events.get_mut(idx)
                {
                    patch_event_from_continuation(entry, line);
                }
                continue;
            }

            // New primary line — reset continuation tracking
            self.last_event_idx = None;

            let parsed = self.parse_line(line);
            let Some(parsed) = parsed else {
                // Unrecognized line (WARNING, NOTICE, other LOG, etc.) — reset error tracking
                self.last_error_key = None;
                continue;
            };

            // Remember index for events that have multiline continuations
            let will_have_continuations = matches!(
                parsed.event_kind,
                LogEventKind::Autovacuum | LogEventKind::Checkpoint
            );

            self.accumulate(parsed);

            if will_have_continuations {
                self.last_event_idx = Some(self.pending_events.len() - 1);
            }
        }

        // Drain accumulated data
        let errors = self.drain_pending(interner);
        let checkpoint_count = self.pending_checkpoints;
        let autovacuum_count = self.pending_autovacuums;
        let events = std::mem::take(&mut self.pending_events);
        self.pending_checkpoints = 0;
        self.pending_autovacuums = 0;
        self.last_event_idx = None;
        // NOTE: last_error_key is NOT reset here — if the last error is
        // held back (no STATEMENT yet), the key stays alive so that a
        // STATEMENT line in the next batch can attach to it.
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

                let entry = self
                    .pending_errors
                    .entry(key.clone())
                    .or_insert(PendingError {
                        count: 0,
                        sample: String::new(),
                        statement: String::new(),
                    });
                entry.count += 1;
                // Keep first sample only
                if entry.sample.is_empty() {
                    let mut sample = parsed.message;
                    sample.truncate(MAX_LOG_MESSAGE_LEN);
                    entry.sample = sample;
                }
                self.last_error_key = Some(key);
                self.error_held_back = false;
            }
            LogEventKind::Statement => {
                // Attach SQL statement to the preceding error
                if let Some(ref key) = self.last_error_key
                    && let Some(entry) = self.pending_errors.get_mut(key)
                    && entry.statement.is_empty()
                {
                    let mut stmt = parsed.message;
                    stmt.truncate(MAX_LOG_MESSAGE_LEN);
                    entry.statement = stmt;
                }
                self.last_error_key = None;
            }
            LogEventKind::DetailContext => {
                // Keep last_error_key alive — STATEMENT may follow after DETAIL/CONTEXT
            }
            LogEventKind::Checkpoint => {
                self.last_error_key = None;
                self.pending_checkpoints = self.pending_checkpoints.saturating_add(1);
                if let Some(event_data) = parsed.event_data {
                    self.pending_events
                        .push(event_data_to_entry(event_data, &parsed.message));
                }
            }
            LogEventKind::Autovacuum => {
                self.last_error_key = None;
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

        // If the last error has no STATEMENT yet and we haven't already
        // held it back once, keep it in pending_errors so that a STATEMENT
        // line arriving in the next collect() batch can still attach to it.
        let held_back = if !self.error_held_back {
            self.last_error_key.as_ref().and_then(|key| {
                let entry = self.pending_errors.get(key)?;
                if entry.statement.is_empty() {
                    Some(key.clone())
                } else {
                    None
                }
            })
        } else {
            None
        };
        let held_entry = held_back
            .as_ref()
            .and_then(|key| self.pending_errors.remove(key))
            .map(|entry| (held_back.unwrap(), entry));

        let mut entries: Vec<_> = self.pending_errors.drain().collect();

        // Put the held-back entry back for one more cycle.
        if let Some((key, entry)) = held_entry {
            self.pending_errors.insert(key, entry);
            self.error_held_back = true;
        } else {
            self.error_held_back = false;
        }

        // Sort by count descending — keep top patterns
        entries.sort_by(|a, b| b.1.count.cmp(&a.1.count));
        entries.truncate(MAX_LOG_PATTERNS_PER_SNAPSHOT);

        entries
            .into_iter()
            .map(|((pattern, severity), pending)| {
                let mut pattern_str = pattern;
                pattern_str.truncate(MAX_LOG_MESSAGE_LEN);

                let statement_hash = if pending.statement.is_empty() {
                    0
                } else {
                    interner.intern(&pending.statement)
                };
                crate::storage::model::PgLogEntry {
                    pattern_hash: interner.intern(&pattern_str),
                    severity,
                    count: pending.count,
                    sample_hash: interner.intern(&pending.sample),
                    statement_hash,
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

/// Check if a line is a continuation of a previous multiline LOG message.
/// PostgreSQL continuation lines start with whitespace (tab or spaces).
fn is_continuation_line(line: &str) -> bool {
    line.starts_with(['\t', ' '])
}

/// Patch an existing event entry in-place from a continuation line.
///
/// Checks for known markers (buffer usage, avg rate, CPU, pages, tuples, WAL)
/// and updates the corresponding fields. Unknown lines are silently ignored
/// (zero allocations for garbage like DETAIL, CONTEXT, STATEMENT, long queries).
fn patch_event_from_continuation(entry: &mut PgLogEventEntry, line: &str) {
    let trimmed = line.trim_start();

    // "buffer usage: 78 hits, 5 misses, 0 dirtied"
    // "буферов: 78 попаданий, 5 промахов, 0 загрязнено"
    if trimmed.starts_with("buffer usage:") || trimmed.starts_with("буферов:") {
        entry.buffer_hits = parser::extract_i64_after(trimmed, "buffer usage: ")
            .or_else(|| parser::extract_i64_after(trimmed, "буферов: "))
            .unwrap_or(0);
        entry.buffer_misses = parser::extract_i64_after(trimmed, " hits, ")
            .or_else(|| parser::extract_i64_after(trimmed, " попаданий, "))
            .unwrap_or(0);
        entry.buffer_dirtied = parser::extract_i64_after(trimmed, " misses, ")
            .or_else(|| parser::extract_i64_after(trimmed, " промахов, "))
            .unwrap_or(0);
        return;
    }

    // "avg read rate: 0.653 MB/s, avg write rate: 0.000 MB/s"
    // "средняя скорость чтения: 0.653 МБ/с, средняя скорость записи: 0.000 МБ/с"
    if trimmed.starts_with("avg read rate:") || trimmed.starts_with("средняя скорость чтения:")
    {
        entry.avg_read_rate_mbs = parser::extract_f64_after(trimmed, "avg read rate: ")
            .or_else(|| parser::extract_f64_after(trimmed, "средняя скорость чтения: "))
            .unwrap_or(0.0);
        entry.avg_write_rate_mbs = parser::extract_f64_after(trimmed, "avg write rate: ")
            .or_else(|| parser::extract_f64_after(trimmed, "средняя скорость записи: "))
            .unwrap_or(0.0);
        return;
    }

    // "system usage: CPU: user: 0.00 s, system: 0.00 s, elapsed: 0.05 s"
    // "системное использование: CPU: user: 0.12 s, system: 0.34 s, elapsed: 5.67 s"
    if trimmed.starts_with("system usage:") || trimmed.starts_with("системное") {
        entry.cpu_user_s = parser::extract_cpu_field(trimmed, "user: ");
        entry.cpu_system_s = parser::extract_cpu_field(trimmed, "system: ");
        entry.elapsed_s = parser::extract_f64_after(trimmed, "elapsed: ")
            .or_else(|| parser::extract_f64_after(trimmed, "прошло: "))
            .unwrap_or(entry.elapsed_s);
        return;
    }

    // "tuples: 50 removed, ..."
    // "кортежей: 50 удалено, ..."
    if trimmed.starts_with("tuples:") || trimmed.starts_with("кортежей:") {
        entry.extra_num1 = parser::extract_i64_after(trimmed, "tuples: ")
            .or_else(|| parser::extract_i64_after(trimmed, "кортежей: "))
            .unwrap_or(0);
        return;
    }

    // "pages: 1 removed, ..."
    // "страниц: 1 удалено, ..."
    if trimmed.starts_with("pages:") || trimmed.starts_with("страниц:") {
        entry.extra_num2 = parser::extract_i64_after(trimmed, "pages: ")
            .or_else(|| parser::extract_i64_after(trimmed, "страниц: "))
            .unwrap_or(0);
        return;
    }

    // "WAL usage: 15 records, 2 full page images, 1617 bytes"
    if trimmed.starts_with("WAL usage:") {
        entry.wal_records = parser::extract_i64_after(trimmed, "WAL usage: ").unwrap_or(0);
        entry.wal_fpi = parser::extract_i64_after(trimmed, " records, ").unwrap_or(0);
        entry.wal_bytes = parser::extract_i64_after(trimmed, " images, ").unwrap_or(0);
    }

    // Anything else — ignore (DETAIL, CONTEXT, STATEMENT, etc.)
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
            extra_num3: 0,
            buffer_hits: 0,
            buffer_misses: 0,
            buffer_dirtied: 0,
            avg_read_rate_mbs: 0.0,
            avg_write_rate_mbs: 0.0,
            cpu_user_s: 0.0,
            cpu_system_s: 0.0,
            wal_records: 0,
            wal_fpi: 0,
            wal_bytes: 0,
        },
        EventData::CheckpointComplete {
            buffers_written,
            write_time_ms,
            sync_time_ms,
            total_time_ms,
            distance_kb,
            estimate_kb,
            wal_added,
            wal_removed,
            wal_recycled,
            sync_files,
            longest_sync_s,
            average_sync_s,
        } => PgLogEventEntry {
            event_type: PgLogEventType::CheckpointComplete,
            message: message.to_string(),
            table_name: String::new(),
            elapsed_s: total_time_ms / 1000.0,
            extra_num1: buffers_written,
            extra_num2: distance_kb,
            extra_num3: estimate_kb,
            // Reuse autovacuum fields for checkpoint-specific metrics:
            cpu_user_s: write_time_ms / 1000.0,
            cpu_system_s: sync_time_ms / 1000.0,
            buffer_hits: sync_files,
            buffer_misses: 0,
            buffer_dirtied: 0,
            avg_read_rate_mbs: longest_sync_s,
            avg_write_rate_mbs: average_sync_s,
            wal_records: wal_added,
            wal_fpi: wal_removed,
            wal_bytes: wal_recycled,
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
            wal_records,
            wal_fpi,
            wal_bytes,
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
            extra_num3: 0,
            buffer_hits,
            buffer_misses,
            buffer_dirtied,
            avg_read_rate_mbs,
            avg_write_rate_mbs,
            cpu_user_s,
            cpu_system_s,
            wal_records,
            wal_fpi,
            wal_bytes,
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

        // First drain may hold back the last error (waiting for STATEMENT).
        // Second drain releases it.
        let mut entries = collector.drain_pending(&mut interner);
        entries.extend(collector.drain_pending(&mut interner));

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

        // First drain holds back (waiting for STATEMENT), second releases
        let mut entries = collector.drain_pending(&mut interner);
        entries.extend(collector.drain_pending(&mut interner));
        assert_eq!(entries.len(), 1);

        // Third drain should be empty
        let entries = collector.drain_pending(&mut interner);
        assert!(entries.is_empty());
    }

    #[test]
    fn test_statement_same_batch() {
        let mut collector = LogCollector::new();
        let mut interner = StringInterner::new();

        // ERROR followed by STATEMENT in the same batch
        collector.accumulate(ParsedLogLine {
            severity: PgLogSeverity::Error,
            message: "canceling statement due to statement timeout".to_string(),
            event_kind: LogEventKind::Error,
            event_data: None,
        });
        collector.accumulate(ParsedLogLine {
            severity: PgLogSeverity::Error,
            message: "select pg_sleep(2);".to_string(),
            event_kind: LogEventKind::Statement,
            event_data: None,
        });

        let entries = collector.drain_pending(&mut interner);
        assert_eq!(entries.len(), 1);
        assert!(
            entries[0].statement_hash != 0,
            "statement should be attached"
        );
    }

    #[test]
    fn test_statement_cross_batch() {
        let mut collector = LogCollector::new();
        let mut interner = StringInterner::new();

        // Batch 1: ERROR only
        collector.accumulate(ParsedLogLine {
            severity: PgLogSeverity::Error,
            message: "canceling statement due to statement timeout".to_string(),
            event_kind: LogEventKind::Error,
            event_data: None,
        });

        // First drain — error held back (no STATEMENT yet)
        let entries = collector.drain_pending(&mut interner);
        assert_eq!(
            entries.len(),
            0,
            "error should be held back waiting for STATEMENT"
        );

        // Batch 2: STATEMENT arrives
        collector.accumulate(ParsedLogLine {
            severity: PgLogSeverity::Error,
            message: "select pg_sleep(2);".to_string(),
            event_kind: LogEventKind::Statement,
            event_data: None,
        });

        // Second drain — error with STATEMENT attached
        let entries = collector.drain_pending(&mut interner);
        assert_eq!(entries.len(), 1);
        assert!(
            entries[0].statement_hash != 0,
            "statement should be attached"
        );
    }

    #[test]
    fn test_statement_not_held_forever() {
        let mut collector = LogCollector::new();
        let mut interner = StringInterner::new();

        // ERROR without STATEMENT
        collector.accumulate(ParsedLogLine {
            severity: PgLogSeverity::Error,
            message: "division by zero".to_string(),
            event_kind: LogEventKind::Error,
            event_data: None,
        });

        // First drain — held back
        let entries = collector.drain_pending(&mut interner);
        assert_eq!(entries.len(), 0);

        // Second drain — no STATEMENT came, error should be released
        let entries = collector.drain_pending(&mut interner);
        assert_eq!(
            entries.len(),
            1,
            "error should be released after one hold-back"
        );
        assert_eq!(entries[0].statement_hash, 0, "no statement attached");

        // Third drain — should be empty
        let entries = collector.drain_pending(&mut interner);
        assert!(entries.is_empty());
    }
}
