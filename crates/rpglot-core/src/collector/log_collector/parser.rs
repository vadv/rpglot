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

/// Severity keywords we look for in log lines.
const SEVERITIES: &[(&str, PgLogSeverity)] = &[
    ("PANIC:  ", PgLogSeverity::Panic),
    ("FATAL:  ", PgLogSeverity::Fatal),
    ("ERROR:  ", PgLogSeverity::Error),
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
    /// Returns `Some(ParsedLogLine)` if the line contains ERROR/FATAL/PANIC,
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
                });
            }
        }

        // Check for LOG-level operational events (checkpoint, autovacuum).
        const LOG_PREFIX: &str = "LOG:  ";
        if let Some(pos) = line.find(LOG_PREFIX) {
            let message = &line[pos + LOG_PREFIX.len()..];
            if message.starts_with("checkpoint starting:")
                || message.starts_with("checkpoint complete:")
            {
                return Some(ParsedLogLine {
                    severity: PgLogSeverity::Error,
                    message: message.to_string(),
                    event_kind: LogEventKind::Checkpoint,
                });
            }
            if message.starts_with("automatic vacuum of table")
                || message.starts_with("automatic analyze of table")
            {
                return Some(ParsedLogLine {
                    severity: PgLogSeverity::Error,
                    message: message.to_string(),
                    event_kind: LogEventKind::Autovacuum,
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
                let message = fields[13].clone();
                if message.starts_with("checkpoint starting:")
                    || message.starts_with("checkpoint complete:")
                {
                    return Some(ParsedLogLine {
                        severity: PgLogSeverity::Error,
                        message,
                        event_kind: LogEventKind::Checkpoint,
                    });
                }
                if message.starts_with("automatic vacuum of table")
                    || message.starts_with("automatic analyze of table")
                {
                    return Some(ParsedLogLine {
                        severity: PgLogSeverity::Error,
                        message,
                        event_kind: LogEventKind::Autovacuum,
                    });
                }
                return None;
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
        })
    }
}

/// Strip optional SQLSTATE code prefix from message.
/// PostgreSQL may include SQLSTATE like `42P01:  relation "foo"...`
fn strip_sqlstate(message: &str) -> &str {
    // SQLSTATE is exactly 5 chars: 2 digits/letters + 3 digits/letters
    if message.len() > 7 {
        let prefix = &message[..5];
        let is_sqlstate = prefix
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
        if is_sqlstate && message[5..].starts_with(":  ") {
            return message[8..].trim();
        }
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
