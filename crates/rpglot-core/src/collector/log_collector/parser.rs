//! PostgreSQL log line parser.
//!
//! Supports stderr format with configurable `log_line_prefix`.
//! Parses ERROR/FATAL/PANIC severity lines and selected LOG-level
//! operational events (checkpoints, autovacuum).

use crate::storage::model::PgLogSeverity;

/// Kind of parsed log event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogEventKind {
    /// Error/Fatal/Panic — existing behavior.
    Error,
    /// Checkpoint starting or complete (LOG level).
    Checkpoint,
    /// Automatic vacuum or analyze completed (LOG level).
    Autovacuum,
    /// STATEMENT: line following an error (contains the SQL that caused it).
    Statement,
    /// DETAIL: or CONTEXT: continuation line (skipped but recognized).
    DetailContext,
}

/// Extracted structured data from checkpoint/autovacuum LOG messages.
#[derive(Debug, Clone, PartialEq)]
pub enum EventData {
    CheckpointStarting {
        reason: String,
    },
    CheckpointComplete {
        buffers_written: i64,
        write_time_ms: f64,
        sync_time_ms: f64,
        total_time_ms: f64,
        distance_kb: i64,
        estimate_kb: i64,
    },
    Autovacuum {
        table_name: String,
        is_analyze: bool,
        tuples_removed: i64,
        pages_removed: i64,
        elapsed_s: f64,
        /// Buffer cache hits.
        buffer_hits: i64,
        /// Buffer cache misses (reads from disk).
        buffer_misses: i64,
        /// Buffers dirtied during operation.
        buffer_dirtied: i64,
        /// Average read rate in MB/s.
        avg_read_rate_mbs: f64,
        /// Average write rate in MB/s.
        avg_write_rate_mbs: f64,
        /// CPU user time in seconds.
        cpu_user_s: f64,
        /// CPU system time in seconds.
        cpu_system_s: f64,
        /// WAL records generated.
        wal_records: i64,
        /// WAL full page images.
        wal_fpi: i64,
        /// WAL bytes written.
        wal_bytes: i64,
    },
}

/// Result of parsing a single log line.
#[derive(Debug, Clone)]
pub struct ParsedLogLine {
    /// Severity level.
    pub severity: PgLogSeverity,
    /// The error message (after severity prefix).
    pub message: String,
    /// Kind of event detected.
    pub event_kind: LogEventKind,
    /// Extracted event data (checkpoint stats, vacuum table name, etc.).
    pub event_data: Option<EventData>,
}

/// Compiled parser for stderr format with a specific `log_line_prefix`.
///
/// Instead of building a regex from the prefix pattern (which would require
/// the `regex` crate), we use a simpler approach: scan for the severity
/// keyword pattern `(ERROR|FATAL|PANIC):  ` in the line. This works because:
///
/// 1. PostgreSQL always emits severity after the prefix, followed by `:  `
/// 2. The severity keywords are distinctive enough to avoid false positives
/// 3. We only care about error-level messages, not LOG/WARNING/NOTICE
///
/// This approach handles any `log_line_prefix` without needing to parse it.
pub struct StderrParser {
    _prefix: String,
}

/// Severity keywords we look for in log lines (English + Russian locale).
const SEVERITIES: &[(&str, PgLogSeverity)] = &[
    ("PANIC:  ", PgLogSeverity::Panic),
    ("FATAL:  ", PgLogSeverity::Fatal),
    ("ERROR:  ", PgLogSeverity::Error),
    // Russian locale (lc_messages = 'ru_RU.UTF-8')
    ("ПАНИКА:  ", PgLogSeverity::Panic),
    ("ВАЖНО:  ", PgLogSeverity::Fatal),
    ("ОШИБКА:  ", PgLogSeverity::Error),
];

/// LOG-level prefixes (English + Russian locale).
const LOG_PREFIXES: &[&str] = &["LOG:  ", "СООБЩЕНИЕ:  "];

/// STATEMENT-level prefixes (English + Russian locale).
const STATEMENT_PREFIXES: &[&str] = &["STATEMENT:  ", "ОПЕРАТОР:  "];

/// DETAIL/CONTEXT prefixes — recognized to keep last_error_key alive.
const DETAIL_CONTEXT_PREFIXES: &[&str] = &[
    "DETAIL:  ",
    "CONTEXT:  ",
    "HINT:  ",
    "ПОДРОБНОСТИ:  ",
    "КОНТЕКСТ:  ",
    "ПОДСКАЗКА:  ",
];

/// Checkpoint starting message prefixes (English + Russian).
const CHECKPOINT_STARTING: &[&str] = &["checkpoint starting:", "начата контрольная точка:"];

/// Checkpoint complete message prefixes (English + Russian).
const CHECKPOINT_COMPLETE: &[&str] = &["checkpoint complete:", "контрольная точка завершена:"];

/// Autovacuum/autoanalyze message prefixes (English + Russian).
const AUTOVACUUM_PREFIXES: &[&str] = &[
    "automatic vacuum of table",
    "automatic analyze of table",
    "автоматическая очистка таблицы",
    "автоматический анализ таблицы",
];

impl StderrParser {
    /// Build a parser. The `log_line_prefix` is stored for future use
    /// but the current implementation uses keyword scanning.
    pub fn new(log_line_prefix: &str) -> Self {
        Self {
            _prefix: log_line_prefix.to_string(),
        }
    }

    /// Try to parse a log line.
    ///
    /// Returns `Some(ParsedLogLine)` if the line contains ERROR/FATAL/PANIC
    /// or a LOG-level operational event (checkpoint, autovacuum).
    /// `None` otherwise (LOG, WARNING, NOTICE, continuation lines, etc.).
    pub fn parse_line(&self, line: &str) -> Option<ParsedLogLine> {
        // Scan for severity keyword in the line.
        // PostgreSQL format: `<prefix>ERROR:  <message>`
        // The double space after colon is a PG convention.
        for &(keyword, severity) in SEVERITIES {
            if let Some(pos) = line.find(keyword) {
                let message_start = pos + keyword.len();
                let message = line[message_start..].trim();

                // Skip if message is empty
                if message.is_empty() {
                    continue;
                }

                // Strip optional SQLSTATE code: "42P01:  " prefix in the message
                let message = strip_sqlstate(message);

                return Some(ParsedLogLine {
                    severity,
                    message: message.to_string(),
                    event_kind: LogEventKind::Error,
                    event_data: None,
                });
            }
        }

        // Check for LOG-level operational events (checkpoint, autovacuum).
        for prefix in LOG_PREFIXES {
            if let Some(pos) = line.find(prefix) {
                let message = &line[pos + prefix.len()..];
                return classify_log_message(message);
            }
        }

        // Check for STATEMENT: line (SQL that caused the preceding error).
        for prefix in STATEMENT_PREFIXES {
            if let Some(pos) = line.find(prefix) {
                let message = &line[pos + prefix.len()..];
                return Some(ParsedLogLine {
                    severity: PgLogSeverity::Error, // placeholder, not used for grouping
                    message: message.trim().to_string(),
                    event_kind: LogEventKind::Statement,
                    event_data: None,
                });
            }
        }

        // Check for DETAIL/CONTEXT/HINT lines — recognized to keep error association alive.
        for prefix in DETAIL_CONTEXT_PREFIXES {
            if line.contains(prefix) {
                return Some(ParsedLogLine {
                    severity: PgLogSeverity::Error, // placeholder
                    message: String::new(),
                    event_kind: LogEventKind::DetailContext,
                    event_data: None,
                });
            }
        }

        None
    }
}

/// Csvlog parser (fixed PostgreSQL CSV format).
///
/// PostgreSQL csvlog has fixed columns (PG 12+):
/// `log_time,user_name,database_name,process_id,connection_from,session_id,
///  session_line_num,command_tag,session_start_time,virtual_transaction_id,
///  transaction_id,error_severity,sql_state_code,message,...`
///
/// Column index for error_severity = 11 (0-based), message = 13.
pub struct CsvlogParser;

impl CsvlogParser {
    /// Try to parse a csvlog line.
    ///
    /// In csvlog format, severity (column 11) is always in English,
    /// but the message (column 13) follows lc_messages locale.
    pub fn parse_line(&self, line: &str) -> Option<ParsedLogLine> {
        // Simple CSV split — PG csvlog uses standard CSV with double-quote escaping.
        let fields = split_csv_line(line);
        if fields.len() < 14 {
            return None;
        }

        let severity_str = &fields[11];
        let severity = match severity_str.as_str() {
            "ERROR" => PgLogSeverity::Error,
            "FATAL" => PgLogSeverity::Fatal,
            "PANIC" => PgLogSeverity::Panic,
            "LOG" => {
                let message = &fields[13];
                return classify_log_message(message);
            }
            _ => return None,
        };

        let message = fields[13].clone();
        if message.is_empty() {
            return None;
        }

        Some(ParsedLogLine {
            severity,
            message,
            event_kind: LogEventKind::Error,
            event_data: None,
        })
    }
}

// ============================================================
// LOG message classification and data extraction
// ============================================================

/// Classify a LOG-level message as checkpoint or autovacuum event.
/// Returns `None` if the message is not a known operational event.
fn classify_log_message(message: &str) -> Option<ParsedLogLine> {
    // Checkpoint starting
    for prefix in CHECKPOINT_STARTING {
        if let Some(rest) = message.strip_prefix(prefix) {
            let reason = rest.trim().to_string();
            return Some(ParsedLogLine {
                severity: PgLogSeverity::Error,
                message: message.to_string(),
                event_kind: LogEventKind::Checkpoint,
                event_data: Some(EventData::CheckpointStarting { reason }),
            });
        }
    }

    // Checkpoint complete
    for prefix in CHECKPOINT_COMPLETE {
        if message.starts_with(prefix) {
            let event_data = parse_checkpoint_complete(message);
            return Some(ParsedLogLine {
                severity: PgLogSeverity::Error,
                message: message.to_string(),
                event_kind: LogEventKind::Checkpoint,
                event_data: Some(event_data),
            });
        }
    }

    // Autovacuum / autoanalyze
    for prefix in AUTOVACUUM_PREFIXES {
        if message.starts_with(prefix) {
            let is_analyze = prefix.contains("analyze") || prefix.contains("анализ");
            let event_data = parse_autovacuum(message, is_analyze);
            return Some(ParsedLogLine {
                severity: PgLogSeverity::Error,
                message: message.to_string(),
                event_kind: LogEventKind::Autovacuum,
                event_data: Some(event_data),
            });
        }
    }

    None
}

/// Parse checkpoint complete message fields.
///
/// EN: `checkpoint complete: wrote 123 buffers (0.1%); ... write=1.234 s, sync=0.567 s, total=2.345 s; ... distance=12345 kB, estimate=67890 kB`
/// RU: `контрольная точка завершена: записано буферов: 123 (0.1%); ... запись=1.234 с, синхронизация=0.567 с, всего=2.345 с; ... расстояние=12345 КБ, ожидалось=67890 КБ`
fn parse_checkpoint_complete(message: &str) -> EventData {
    let buffers_written = extract_i64_after(message, "wrote ")
        .or_else(|| extract_i64_after(message, "записано буферов: "))
        .unwrap_or(0);

    let write_time_s = extract_f64_after(message, "write=")
        .or_else(|| extract_f64_after(message, "запись="))
        .unwrap_or(0.0);

    let sync_time_s = extract_f64_after(message, "sync=")
        .or_else(|| extract_f64_after(message, "синхронизация="))
        .unwrap_or(0.0);

    let total_time_s = extract_f64_after(message, "total=")
        .or_else(|| extract_f64_after(message, "всего="))
        .unwrap_or(0.0);

    let distance_kb = extract_i64_after(message, "distance=")
        .or_else(|| extract_i64_after(message, "расстояние="))
        .unwrap_or(0);

    let estimate_kb = extract_i64_after(message, "estimate=")
        .or_else(|| extract_i64_after(message, "ожидалось="))
        .unwrap_or(0);

    EventData::CheckpointComplete {
        buffers_written,
        write_time_ms: write_time_s * 1000.0,
        sync_time_ms: sync_time_s * 1000.0,
        total_time_ms: total_time_s * 1000.0,
        distance_kb,
        estimate_kb,
    }
}

/// Parse autovacuum/autoanalyze message fields.
///
/// EN: `automatic vacuum of table "db.schema.table": index scans: 1\n  pages: 0 removed, ...`
/// RU: `автоматическая очистка таблицы "db.schema.table": ...`
fn parse_autovacuum(message: &str, is_analyze: bool) -> EventData {
    // Extract table name from first quoted string
    let table_name = extract_quoted_string(message).unwrap_or_default();

    let tuples_removed = if !is_analyze {
        // EN: "tuples: 1234 removed"
        extract_i64_after(message, "tuples: ")
            .or_else(|| extract_i64_after(message, "кортежей: "))
            .unwrap_or(0)
    } else {
        0
    };

    let pages_removed = if !is_analyze {
        // EN: "pages: 0 removed"
        extract_i64_after(message, "pages: ")
            .or_else(|| extract_i64_after(message, "страниц: "))
            .unwrap_or(0)
    } else {
        0
    };

    // EN: "elapsed: 5.67 s"
    let elapsed_s = extract_f64_after(message, "elapsed: ")
        .or_else(|| extract_f64_after(message, "прошло: "))
        .unwrap_or(0.0);

    // Buffer usage: "456 hits, 78 misses, 9 dirtied"
    // RU: "буферов: 456 попаданий, 78 промахов, 9 загрязнено"
    let buffer_hits = extract_i64_after(message, "buffer usage: ")
        .or_else(|| extract_i64_after(message, "буферов: "))
        .unwrap_or(0);

    let buffer_misses = extract_i64_after(message, " hits, ")
        .or_else(|| extract_i64_after(message, " попаданий, "))
        .unwrap_or(0);

    let buffer_dirtied = extract_i64_after(message, " misses, ")
        .or_else(|| extract_i64_after(message, " промахов, "))
        .unwrap_or(0);

    // Avg read/write rate: "avg read rate: 1.234 MB/s, avg write rate: 5.678 MB/s"
    // RU: "средняя скорость чтения: 1.234 МБ/с, средняя скорость записи: 5.678 МБ/с"
    let avg_read_rate_mbs = extract_f64_after(message, "avg read rate: ")
        .or_else(|| extract_f64_after(message, "средняя скорость чтения: "))
        .unwrap_or(0.0);

    let avg_write_rate_mbs = extract_f64_after(message, "avg write rate: ")
        .or_else(|| extract_f64_after(message, "средняя скорость записи: "))
        .unwrap_or(0.0);

    // CPU: "user: 0.12 s, system: 0.34 s"
    // Note: for CPU we look for "user: " after "CPU:" to avoid ambiguity
    let cpu_user_s = extract_cpu_field(message, "user: ");
    let cpu_system_s = extract_cpu_field(message, "system: ");

    // WAL usage: "15 records, 2 full page images, 1617 bytes"
    let wal_records = extract_i64_after(message, "WAL usage: ").unwrap_or(0);
    let wal_fpi = if message.contains("WAL usage:") {
        extract_i64_after(message, " records, ").unwrap_or(0)
    } else {
        0
    };
    let wal_bytes = if message.contains("WAL usage:") {
        extract_i64_after(message, " images, ").unwrap_or(0)
    } else {
        0
    };

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
    }
}

// ============================================================
// Numeric extraction helpers
// ============================================================

/// Extract first i64 value immediately after `marker` in `text`.
pub(super) fn extract_i64_after(text: &str, marker: &str) -> Option<i64> {
    let pos = text.find(marker)? + marker.len();
    let rest = &text[pos..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit() && c != '-')
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Extract first f64 value immediately after `marker` in `text`.
pub(super) fn extract_f64_after(text: &str, marker: &str) -> Option<f64> {
    let pos = text.find(marker)? + marker.len();
    let rest = &text[pos..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Extract CPU time field (user/system) from autovacuum message.
/// Looks for the field after "CPU:" marker to avoid ambiguity with other "user:" occurrences.
pub(super) fn extract_cpu_field(text: &str, field: &str) -> f64 {
    // Find "CPU:" first, then look for the field after it
    let cpu_pos = text.find("CPU:").or_else(|| text.find("ЦП:")).unwrap_or(0);
    let rest = &text[cpu_pos..];
    extract_f64_after(rest, field).unwrap_or(0.0)
}

/// Extract first double-quoted string from `text`.
fn extract_quoted_string(text: &str) -> Option<String> {
    let start = text.find('"')? + 1;
    let end = start + text[start..].find('"')?;
    Some(text[start..end].to_string())
}

/// Strip optional SQLSTATE code prefix from message.
/// PostgreSQL may include SQLSTATE like `42P01:  relation "foo"...`
fn strip_sqlstate(message: &str) -> &str {
    // SQLSTATE is exactly 5 ASCII chars: 2 digits/letters + 3 digits/letters
    // followed by ":  " (colon + two spaces).
    // Since SQLSTATE is pure ASCII, we can safely index by bytes — but only
    // after checking that bytes 0..5 are all ASCII (single-byte chars).
    if message.len() > 7
        && message.as_bytes()[..5]
            .iter()
            .all(|&b| b.is_ascii_uppercase() || b.is_ascii_digit())
        && message.as_bytes()[5] == b':'
        && message.as_bytes()[6] == b' '
        && message.as_bytes()[7] == b' '
    {
        return message[8..].trim();
    }
    message
}

/// Split a CSV line respecting double-quote escaping.
fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    // Escaped quote
                    chars.next();
                    current.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                current.push(c);
            }
        } else if c == '"' {
            in_quotes = true;
        } else if c == ',' {
            fields.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    fields.push(current);

    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stderr_parser_error() {
        let parser = StderrParser::new("%t [%p]: ");
        let line = "2024-01-15 14:30:00 UTC [12345]: ERROR:  relation \"users\" does not exist";
        let parsed = parser.parse_line(line).unwrap();
        assert_eq!(parsed.severity, PgLogSeverity::Error);
        assert_eq!(parsed.message, "relation \"users\" does not exist");
    }

    #[test]
    fn test_stderr_parser_fatal() {
        let parser = StderrParser::new("%t [%p]: ");
        let line = "2024-01-15 14:30:00 UTC [12345]: FATAL:  database \"mydb\" does not exist";
        let parsed = parser.parse_line(line).unwrap();
        assert_eq!(parsed.severity, PgLogSeverity::Fatal);
    }

    #[test]
    fn test_stderr_parser_panic() {
        let parser = StderrParser::new("%t [%p]: ");
        let line = "2024-01-15 14:30:00 UTC [12345]: PANIC:  could not write to file";
        let parsed = parser.parse_line(line).unwrap();
        assert_eq!(parsed.severity, PgLogSeverity::Panic);
    }

    #[test]
    fn test_stderr_parser_log_other_ignored() {
        let parser = StderrParser::new("%t [%p]: ");
        let line = "2024-01-15 14:30:00 UTC [12345]: LOG:  database system is ready";
        assert!(parser.parse_line(line).is_none());
    }

    #[test]
    fn test_stderr_parser_warning_ignored() {
        let parser = StderrParser::new("%t [%p]: ");
        let line = "2024-01-15 14:30:00 UTC [12345]: WARNING:  some warning message";
        assert!(parser.parse_line(line).is_none());
    }

    #[test]
    fn test_stderr_checkpoint_starting() {
        let parser = StderrParser::new("%t [%p]: ");
        let line = "2024-01-15 14:30:00 UTC [12345]: LOG:  checkpoint starting: time";
        let parsed = parser.parse_line(line).unwrap();
        assert_eq!(parsed.event_kind, LogEventKind::Checkpoint);
        assert!(parsed.message.starts_with("checkpoint starting:"));
        assert!(matches!(
            parsed.event_data,
            Some(EventData::CheckpointStarting { .. })
        ));
    }

    #[test]
    fn test_stderr_checkpoint_complete() {
        let parser = StderrParser::new("%t [%p]: ");
        let line =
            "2024-01-15 14:30:00 UTC [12345]: LOG:  checkpoint complete: wrote 123 buffers (0.1%)";
        let parsed = parser.parse_line(line).unwrap();
        assert_eq!(parsed.event_kind, LogEventKind::Checkpoint);
    }

    #[test]
    fn test_stderr_autovacuum_detected() {
        let parser = StderrParser::new("%t [%p]: ");
        let line = r#"2024-01-15 14:30:00 UTC [12345]: LOG:  automatic vacuum of table "mydb.public.users": index scans: 1"#;
        let parsed = parser.parse_line(line).unwrap();
        assert_eq!(parsed.event_kind, LogEventKind::Autovacuum);
    }

    #[test]
    fn test_stderr_autoanalyze_detected() {
        let parser = StderrParser::new("%t [%p]: ");
        let line = r#"2024-01-15 14:30:00 UTC [12345]: LOG:  automatic analyze of table "mydb.public.users""#;
        let parsed = parser.parse_line(line).unwrap();
        assert_eq!(parsed.event_kind, LogEventKind::Autovacuum);
    }

    #[test]
    fn test_stderr_error_event_kind() {
        let parser = StderrParser::new("%t [%p]: ");
        let line = "2024-01-15 14:30:00 UTC [12345]: ERROR:  relation \"users\" does not exist";
        let parsed = parser.parse_line(line).unwrap();
        assert_eq!(parsed.event_kind, LogEventKind::Error);
        assert!(parsed.event_data.is_none());
    }

    #[test]
    fn test_stderr_parser_with_sqlstate() {
        let parser = StderrParser::new("%t [%p]: ");
        let line =
            "2024-01-15 14:30:00 UTC [12345]: ERROR:  42P01:  relation \"foo\" does not exist";
        let parsed = parser.parse_line(line).unwrap();
        assert_eq!(parsed.message, "relation \"foo\" does not exist");
    }

    #[test]
    fn test_stderr_parser_different_prefix() {
        let parser = StderrParser::new("%m [%p] %d %u ");
        // Different prefix format but still has ERROR:  keyword
        let line = "2024-01-15 14:30:00.123 UTC [99] mydb appuser ERROR:  deadlock detected";
        let parsed = parser.parse_line(line).unwrap();
        assert_eq!(parsed.message, "deadlock detected");
    }

    // ---- Russian locale tests ----

    #[test]
    fn test_stderr_russian_error() {
        let parser = StderrParser::new("%t [%p]: ");
        let line = "2024-01-15 14:30:00 UTC [12345]: ОШИБКА:  отношение \"users\" не существует";
        let parsed = parser.parse_line(line).unwrap();
        assert_eq!(parsed.severity, PgLogSeverity::Error);
        assert_eq!(parsed.event_kind, LogEventKind::Error);
        assert_eq!(parsed.message, "отношение \"users\" не существует");
    }

    #[test]
    fn test_stderr_russian_fatal() {
        let parser = StderrParser::new("%t [%p]: ");
        let line = "2024-01-15 14:30:00 UTC [12345]: ВАЖНО:  база данных \"mydb\" не существует";
        let parsed = parser.parse_line(line).unwrap();
        assert_eq!(parsed.severity, PgLogSeverity::Fatal);
    }

    #[test]
    fn test_stderr_russian_panic() {
        let parser = StderrParser::new("%t [%p]: ");
        let line = "2024-01-15 14:30:00 UTC [12345]: ПАНИКА:  не удалось записать файл";
        let parsed = parser.parse_line(line).unwrap();
        assert_eq!(parsed.severity, PgLogSeverity::Panic);
    }

    #[test]
    fn test_stderr_russian_checkpoint_starting() {
        let parser = StderrParser::new("%t [%p]: ");
        let line = "2024-01-15 14:30:00 UTC [12345]: СООБЩЕНИЕ:  начата контрольная точка: time";
        let parsed = parser.parse_line(line).unwrap();
        assert_eq!(parsed.event_kind, LogEventKind::Checkpoint);
        match parsed.event_data {
            Some(EventData::CheckpointStarting { reason }) => assert_eq!(reason, "time"),
            other => panic!("expected CheckpointStarting, got {:?}", other),
        }
    }

    #[test]
    fn test_stderr_russian_checkpoint_complete() {
        let parser = StderrParser::new("%t [%p]: ");
        let line = "2024-01-15 14:30:00 UTC [12345]: СООБЩЕНИЕ:  контрольная точка завершена: записано буферов: 456 (3.5%); добавлено файлов WAL: 0, удалено: 0, переработано: 1; запись=1.234 с, синхронизация=0.567 с, всего=2.345 с; синхронизировано файлов: 5, самый долгий: 0.123 с, средний: 0.099 с; расстояние=12345 КБ, ожидалось=67890 КБ";
        let parsed = parser.parse_line(line).unwrap();
        assert_eq!(parsed.event_kind, LogEventKind::Checkpoint);
        match parsed.event_data {
            Some(EventData::CheckpointComplete {
                buffers_written,
                write_time_ms,
                sync_time_ms,
                total_time_ms,
                distance_kb,
                estimate_kb,
            }) => {
                assert_eq!(buffers_written, 456);
                assert!((write_time_ms - 1234.0).abs() < 1.0);
                assert!((sync_time_ms - 567.0).abs() < 1.0);
                assert!((total_time_ms - 2345.0).abs() < 1.0);
                assert_eq!(distance_kb, 12345);
                assert_eq!(estimate_kb, 67890);
            }
            other => panic!("expected CheckpointComplete, got {:?}", other),
        }
    }

    #[test]
    fn test_stderr_russian_autovacuum() {
        let parser = StderrParser::new("%t [%p]: ");
        let line = "2024-01-15 14:30:00 UTC [12345]: СООБЩЕНИЕ:  автоматическая очистка таблицы \"mydb.public.users\": просмотров индекса: 1\nстраниц: 0 удалено, 500 осталось\nкортежей: 1234 удалено, 5678 осталось\nсистемное использование: CPU: user: 0.12 s, system: 0.34 s, elapsed: 5.67 s";
        let parsed = parser.parse_line(line).unwrap();
        assert_eq!(parsed.event_kind, LogEventKind::Autovacuum);
        match parsed.event_data {
            Some(EventData::Autovacuum {
                table_name,
                is_analyze,
                tuples_removed,
                elapsed_s,
                cpu_user_s,
                cpu_system_s,
                ..
            }) => {
                assert_eq!(table_name, "mydb.public.users");
                assert!(!is_analyze);
                assert_eq!(tuples_removed, 1234);
                assert!((elapsed_s - 5.67).abs() < 0.01);
                assert!((cpu_user_s - 0.12).abs() < 0.01);
                assert!((cpu_system_s - 0.34).abs() < 0.01);
            }
            other => panic!("expected Autovacuum, got {:?}", other),
        }
    }

    #[test]
    fn test_stderr_russian_autoanalyze() {
        let parser = StderrParser::new("%t [%p]: ");
        let line = "2024-01-15 14:30:00 UTC [12345]: СООБЩЕНИЕ:  автоматический анализ таблицы \"mydb.public.users\"\nсистемное использование: CPU: user: 0.01 s, system: 0.00 s, elapsed: 0.12 s";
        let parsed = parser.parse_line(line).unwrap();
        assert_eq!(parsed.event_kind, LogEventKind::Autovacuum);
        match parsed.event_data {
            Some(EventData::Autovacuum {
                table_name,
                is_analyze,
                elapsed_s,
                cpu_user_s,
                cpu_system_s,
                ..
            }) => {
                assert_eq!(table_name, "mydb.public.users");
                assert!(is_analyze);
                assert!((elapsed_s - 0.12).abs() < 0.01);
                assert!((cpu_user_s - 0.01).abs() < 0.01);
                assert!((cpu_system_s - 0.00).abs() < 0.01);
            }
            other => panic!("expected Autovacuum (analyze), got {:?}", other),
        }
    }

    // ---- Field parsing tests ----

    #[test]
    fn test_parse_checkpoint_complete_fields_en() {
        let msg = "checkpoint complete: wrote 123 buffers (0.1%); 0 WAL file(s) added, 0 removed, 1 recycled; write=1.234 s, sync=0.567 s, total=2.345 s; sync files=5, longest=0.123 s, average=0.099 s; distance=12345 kB, estimate=67890 kB";
        match parse_checkpoint_complete(msg) {
            EventData::CheckpointComplete {
                buffers_written,
                write_time_ms,
                sync_time_ms,
                total_time_ms,
                distance_kb,
                estimate_kb,
            } => {
                assert_eq!(buffers_written, 123);
                assert!((write_time_ms - 1234.0).abs() < 1.0);
                assert!((sync_time_ms - 567.0).abs() < 1.0);
                assert!((total_time_ms - 2345.0).abs() < 1.0);
                assert_eq!(distance_kb, 12345);
                assert_eq!(estimate_kb, 67890);
            }
            other => panic!("expected CheckpointComplete, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_autovacuum_fields_en() {
        let msg = r#"automatic vacuum of table "mydb.public.orders": index scans: 1
pages: 10 removed, 500 remain, 0 skipped due to pins, 0 skipped frozen
tuples: 1234 removed, 5678 remain, 100 are dead but not yet removable
buffer usage: 456 hits, 78 misses, 9 dirtied
avg read rate: 1.234 MB/s, avg write rate: 5.678 MB/s
system usage: CPU: user: 0.12 s, system: 0.34 s, elapsed: 5.67 s"#;
        match parse_autovacuum(msg, false) {
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
            } => {
                assert_eq!(table_name, "mydb.public.orders");
                assert!(!is_analyze);
                assert_eq!(tuples_removed, 1234);
                assert_eq!(pages_removed, 10);
                assert!((elapsed_s - 5.67).abs() < 0.01);
                assert_eq!(buffer_hits, 456);
                assert_eq!(buffer_misses, 78);
                assert_eq!(buffer_dirtied, 9);
                assert!((avg_read_rate_mbs - 1.234).abs() < 0.001);
                assert!((avg_write_rate_mbs - 5.678).abs() < 0.001);
                assert!((cpu_user_s - 0.12).abs() < 0.01);
                assert!((cpu_system_s - 0.34).abs() < 0.01);
                assert_eq!(wal_records, 0);
                assert_eq!(wal_fpi, 0);
                assert_eq!(wal_bytes, 0);
            }
            other => panic!("expected Autovacuum, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_autoanalyze_fields_en() {
        let msg = r#"automatic analyze of table "tpl-service.bucket_90.posting_sender"
avg read rate: 64.717 MB/s, avg write rate: 2.678 MB/s
buffer usage: 1843 hits, 29896 misses, 1237 dirtied
system usage: CPU: user: 1.14 s, system: 0.68 s, elapsed: 3.60 s"#;
        match parse_autovacuum(msg, true) {
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
            } => {
                assert_eq!(table_name, "tpl-service.bucket_90.posting_sender");
                assert!(is_analyze);
                assert_eq!(tuples_removed, 0);
                assert_eq!(pages_removed, 0);
                assert!((elapsed_s - 3.60).abs() < 0.01);
                assert_eq!(buffer_hits, 1843);
                assert_eq!(buffer_misses, 29896);
                assert_eq!(buffer_dirtied, 1237);
                assert!((avg_read_rate_mbs - 64.717).abs() < 0.001);
                assert!((avg_write_rate_mbs - 2.678).abs() < 0.001);
                assert!((cpu_user_s - 1.14).abs() < 0.01);
                assert!((cpu_system_s - 0.68).abs() < 0.01);
                assert_eq!(wal_records, 0);
                assert_eq!(wal_fpi, 0);
                assert_eq!(wal_bytes, 0);
            }
            other => panic!("expected Autovacuum, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_autovacuum_with_wal_usage() {
        let msg = r#"automatic vacuum of table "mydb.public.orders": index scans: 1
pages: 10 removed, 500 remain, 0 skipped due to pins, 0 skipped frozen
tuples: 1234 removed, 5678 remain, 100 are dead but not yet removable
buffer usage: 456 hits, 78 misses, 9 dirtied
avg read rate: 1.234 MB/s, avg write rate: 5.678 MB/s
WAL usage: 15 records, 2 full page images, 1617 bytes
system usage: CPU: user: 0.12 s, system: 0.34 s, elapsed: 5.67 s"#;
        match parse_autovacuum(msg, false) {
            EventData::Autovacuum {
                wal_records,
                wal_fpi,
                wal_bytes,
                ..
            } => {
                assert_eq!(wal_records, 15);
                assert_eq!(wal_fpi, 2);
                assert_eq!(wal_bytes, 1617);
            }
            other => panic!("expected Autovacuum, got {:?}", other),
        }
    }

    #[test]
    fn test_extract_helpers() {
        assert_eq!(extract_i64_after("wrote 123 buffers", "wrote "), Some(123));
        assert_eq!(extract_i64_after("no match", "wrote "), None);
        assert_eq!(extract_f64_after("write=1.234 s", "write="), Some(1.234));
        assert_eq!(
            extract_quoted_string(r#"table "mydb.public.t": done"#),
            Some("mydb.public.t".to_string())
        );
        assert_eq!(extract_quoted_string("no quotes"), None);
    }

    // ---- Existing tests ----

    #[test]
    fn test_csvlog_parser_error() {
        let parser = CsvlogParser;
        let line = r#"2024-01-15 14:30:00.123 UTC,"appuser","mydb",12345,"127.0.0.1:5432","6789",1,"SELECT","2024-01-15 14:00:00 UTC","3/0",0,ERROR,42P01,"relation ""users"" does not exist",,,,,"SELECT * FROM users",,"#;
        let parsed = parser.parse_line(line).unwrap();
        assert_eq!(parsed.severity, PgLogSeverity::Error);
        assert!(parsed.message.contains("relation"));
    }

    #[test]
    fn test_csvlog_parser_log_other_ignored() {
        let parser = CsvlogParser;
        let line = r#"2024-01-15 14:30:00.123 UTC,"","",12345,"","6789",1,"","2024-01-15 14:00:00 UTC","",0,LOG,00000,"database system is ready",,,,,"",,"#;
        assert!(parser.parse_line(line).is_none());
    }

    #[test]
    fn test_csvlog_checkpoint_detected() {
        let parser = CsvlogParser;
        let line = r#"2024-01-15 14:30:00.123 UTC,"","",12345,"","6789",1,"","2024-01-15 14:00:00 UTC","",0,LOG,00000,"checkpoint starting: time",,,,,"",,"#;
        let parsed = parser.parse_line(line).unwrap();
        assert_eq!(parsed.event_kind, LogEventKind::Checkpoint);
    }

    #[test]
    fn test_csvlog_autovacuum_detected() {
        let parser = CsvlogParser;
        let line = r#"2024-01-15 14:30:00.123 UTC,"","",12345,"","6789",1,"","2024-01-15 14:00:00 UTC","",0,LOG,00000,"automatic vacuum of table ""mydb.public.users""",,,,,"",,"#;
        let parsed = parser.parse_line(line).unwrap();
        assert_eq!(parsed.event_kind, LogEventKind::Autovacuum);
    }

    #[test]
    fn test_split_csv_line() {
        let fields = split_csv_line(r#"hello,"world, ""quoted""",123"#);
        assert_eq!(fields, vec!["hello", "world, \"quoted\"", "123"]);
    }

    #[test]
    fn test_strip_sqlstate() {
        assert_eq!(
            strip_sqlstate("42P01:  relation does not exist"),
            "relation does not exist"
        );
        assert_eq!(strip_sqlstate("no sqlstate here"), "no sqlstate here");
        assert_eq!(strip_sqlstate("short"), "short");
    }
}
