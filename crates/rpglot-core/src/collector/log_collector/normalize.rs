//! Normalization of PostgreSQL error messages into patterns for grouping.
//!
//! Inspired by pgbadger's `normalize_error()` function.
//! Replaces concrete values (identifiers, numbers, etc.) with `...`
//! so that identical error types are grouped together.

/// Maximum length of a normalized error pattern.
pub const MAX_LOG_MESSAGE_LEN: usize = 256;

/// Normalize a PostgreSQL error message into a grouping pattern.
///
/// Replaces concrete values with `"..."` or `...` so that messages
/// like `relation "users" does not exist` and `relation "orders" does not exist`
/// both become `relation "..." does not exist`.
pub fn normalize_error(message: &str) -> String {
    let mut s = message.to_string();

    // 1. Remove " at character N..." suffix
    if let Some(pos) = s.find(" at character ") {
        s.truncate(pos);
    }

    // 2. Replace double-quoted identifiers: "something" → "..."
    s = replace_quoted(&s, '"', "\"...\"");

    // 3. Replace single-quoted values: 'something' → '...'
    s = replace_quoted(&s, '\'', "'...'");

    // 4. Replace parenthesized values: (something) → (...)
    s = replace_delimited(&s, '(', ')', "(...)");

    // 5. Replace bracketed values: [something] → [...]
    s = replace_delimited(&s, '[', ']', "[...]");

    // 6. Replace numbers in specific contexts
    s = replace_word_patterns(&s);

    // Truncate to max length
    if s.len() > MAX_LOG_MESSAGE_LEN {
        s.truncate(MAX_LOG_MESSAGE_LEN);
    }

    s
}

/// Replace content between matching quote characters.
/// `"foo bar"` → `"..."`
fn replace_quoted(s: &str, quote: char, replacement: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        if c == quote {
            // Find closing quote
            let mut found_close = false;
            for (j, c2) in chars.by_ref() {
                if c2 == quote {
                    // Don't replace empty quotes
                    if j > i + 1 {
                        result.push_str(replacement);
                    } else {
                        result.push(quote);
                        result.push(quote);
                    }
                    found_close = true;
                    break;
                }
            }
            if !found_close {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Replace content between matching delimiters (non-nested).
fn replace_delimited(s: &str, open: char, close: char, replacement: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();

    while let Some(c) = chars.next() {
        if c == open {
            // Find closing delimiter
            let mut found_close = false;
            for c2 in chars.by_ref() {
                if c2 == close {
                    result.push_str(replacement);
                    found_close = true;
                    break;
                }
            }
            if !found_close {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Replace specific numeric/identifier patterns common in PG error messages.
fn replace_word_patterns(s: &str) -> String {
    let mut result = s.to_string();

    // "transaction 12345" → "transaction ..."
    result = replace_word_number(&result, "transaction ");
    // "relation 12345" → "relation ..."
    result = replace_word_number(&result, "relation ");
    // "process 12345" → "process ..."
    result = replace_word_number(&result, "process ");
    // "database 12345" → "database ..."
    result = replace_word_number(&result, "database ");

    // "PID 12345" → "PID ..."
    result = replace_word_number(&result, "PID ");

    // "after 30.500" → "after ..."
    result = replace_word_float(&result, "after ");

    // "on page 42 of" → "on page ... of"
    result = replace_word_number(&result, "on page ");

    // "TIMELINE 3" → "TIMELINE ..."
    result = replace_word_number(&result, "TIMELINE ");

    // WAL addresses: "0/1A2B3C4D" → "x/x"
    result = replace_wal_address(&result);

    // "invalid input syntax for TYPE: VALUE" → "invalid input syntax for TYPE: ..."
    if let Some(pos) = result.find("invalid input syntax for ")
        && let Some(colon) = result[pos..].find(": ")
    {
        let truncate_at = pos + colon + 2;
        result.truncate(truncate_at);
        result.push_str("...");
    }

    // "permission denied for TABLE name" → "permission denied for TABLE ..."
    for obj_type in &[
        "permission denied for table ",
        "permission denied for schema ",
        "permission denied for sequence ",
        "permission denied for function ",
        "permission denied for database ",
    ] {
        if let Some(pos) = result.to_lowercase().find(*obj_type) {
            let truncate_at = pos + obj_type.len();
            result.truncate(truncate_at);
            result.push_str("...");
            break;
        }
    }

    // "byte sequence for encoding..." → "byte sequence for encoding ..."
    if let Some(pos) = result.find("byte sequence for encoding") {
        result.truncate(pos + "byte sequence for encoding".len());
        result.push_str(" ...");
    }

    result
}

/// Replace a number (digits) immediately after the given prefix word.
fn replace_word_number(s: &str, prefix: &str) -> String {
    if let Some(pos) = s.find(prefix) {
        let after = pos + prefix.len();
        if after < s.len() {
            let rest = &s[after..];
            if rest.starts_with(|c: char| c.is_ascii_digit()) {
                // Find end of number
                let num_end = rest
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(rest.len());
                let mut result = String::with_capacity(s.len());
                result.push_str(&s[..after]);
                result.push_str("...");
                result.push_str(&rest[num_end..]);
                return result;
            }
        }
    }
    s.to_string()
}

/// Replace a float number immediately after the given prefix word.
fn replace_word_float(s: &str, prefix: &str) -> String {
    if let Some(pos) = s.find(prefix) {
        let after = pos + prefix.len();
        if after < s.len() {
            let rest = &s[after..];
            if rest.starts_with(|c: char| c.is_ascii_digit()) {
                let num_end = rest
                    .find(|c: char| !c.is_ascii_digit() && c != '.')
                    .unwrap_or(rest.len());
                let mut result = String::with_capacity(s.len());
                result.push_str(&s[..after]);
                result.push_str("...");
                result.push_str(&rest[num_end..]);
                return result;
            }
        }
    }
    s.to_string()
}

/// Replace WAL-style hex addresses: "0/1A2B3C4D" → "x/x"
fn replace_wal_address(s: &str) -> String {
    // Look for pattern: hex/hex (e.g. "0/1A2B3C4D" or "3E/FF000028")
    let bytes = s.as_bytes();
    let mut result = String::with_capacity(s.len());
    let mut i = 0;

    while i < bytes.len() {
        // Check if we're at the start of a hex/hex pattern
        if bytes[i].is_ascii_hexdigit() {
            let hex_start = i;
            // Scan first hex part
            while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                i += 1;
            }
            let first_len = i - hex_start;
            // Check for /
            if i < bytes.len() && bytes[i] == b'/' && (1..=8).contains(&first_len) {
                i += 1; // skip /
                let second_start = i;
                while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                    i += 1;
                }
                let second_len = i - second_start;
                // Valid WAL address: both parts are hex, reasonable lengths
                if (1..=8).contains(&second_len) {
                    // Check it's bounded by non-alnum or start/end
                    let before_ok = hex_start == 0 || !bytes[hex_start - 1].is_ascii_alphanumeric();
                    let after_ok = i >= bytes.len() || !bytes[i].is_ascii_alphanumeric();
                    if before_ok && after_ok {
                        result.push_str("x/x");
                        continue;
                    }
                }
                // Not a WAL address — push what we consumed
                result.push_str(&s[hex_start..i]);
            } else {
                result.push_str(&s[hex_start..i]);
            }
        } else {
            result.push(s.as_bytes()[i] as char);
            i += 1;
        }
    }

    result
}

// ============================================================
// Error classification
// ============================================================

use crate::storage::model::{ErrorCategory, PgLogSeverity};

/// Classify a normalized error pattern into an [`ErrorCategory`].
///
/// Takes the **normalized** pattern (after `normalize_error()`) and the log severity.
/// Order of checks matters — first match wins.
pub fn classify_error(pattern: &str, severity: PgLogSeverity) -> ErrorCategory {
    // PANIC is always data corruption / catastrophic
    if severity == PgLogSeverity::Panic {
        return ErrorCategory::DataCorruption;
    }

    // --- Lock ---
    if pattern.starts_with("deadlock detected") {
        return ErrorCategory::Lock;
    }
    if pattern.contains("could not obtain lock") {
        return ErrorCategory::Lock;
    }
    if pattern.contains("lock timeout")
        || pattern.starts_with("canceling statement due to lock timeout")
    {
        return ErrorCategory::Lock;
    }
    if pattern.contains("still waiting for") && pattern.contains("Lock") {
        return ErrorCategory::Lock;
    }

    // --- Constraint ---
    if pattern.contains("duplicate key") {
        return ErrorCategory::Constraint;
    }
    if pattern.contains("violates foreign key") {
        return ErrorCategory::Constraint;
    }
    if pattern.contains("violates not-null") {
        return ErrorCategory::Constraint;
    }
    if pattern.contains("violates check constraint") {
        return ErrorCategory::Constraint;
    }
    if pattern.contains("violates exclusion constraint") {
        return ErrorCategory::Constraint;
    }
    if pattern.starts_with("null value in column") {
        return ErrorCategory::Constraint;
    }

    // --- Serialization ---
    if pattern.contains("could not serialize access") {
        return ErrorCategory::Serialization;
    }

    // --- Timeout ---
    if pattern.contains("statement timeout") {
        return ErrorCategory::Timeout;
    }
    if pattern.contains("idle-in-transaction session timeout") {
        return ErrorCategory::Timeout;
    }
    if pattern.contains("transaction timeout") {
        return ErrorCategory::Timeout;
    }
    if pattern.starts_with("canceling statement due to user request") {
        return ErrorCategory::Timeout;
    }
    if pattern.contains("idle session timeout") {
        return ErrorCategory::Timeout;
    }

    // --- Resource ---
    if pattern.contains("out of memory") || pattern.contains("out of shared memory") {
        return ErrorCategory::Resource;
    }
    if pattern.contains("too many connections") {
        return ErrorCategory::Resource;
    }
    if pattern.contains("disk full") {
        return ErrorCategory::Resource;
    }
    if pattern.contains("could not extend") {
        return ErrorCategory::Resource;
    }
    if pattern.contains("could not resize shared memory") {
        return ErrorCategory::Resource;
    }
    if pattern.contains("remaining connection slots") {
        return ErrorCategory::Resource;
    }

    // --- Data Corruption ---
    if pattern.contains("invalid page") {
        return ErrorCategory::DataCorruption;
    }
    if pattern.contains("could not read block") {
        return ErrorCategory::DataCorruption;
    }
    if pattern.contains("data corrupted") || pattern.contains("index corrupted") {
        return ErrorCategory::DataCorruption;
    }
    if pattern.contains("unexpected zero page") {
        return ErrorCategory::DataCorruption;
    }
    if pattern.contains("could not access status of transaction") {
        return ErrorCategory::DataCorruption;
    }
    if pattern.contains("invalid checkpoint record") {
        return ErrorCategory::DataCorruption;
    }
    if pattern.contains("invalid memory alloc") {
        return ErrorCategory::DataCorruption;
    }

    // --- System ---
    if pattern.contains("could not open file") {
        return ErrorCategory::System;
    }
    if pattern.contains("could not write") && pattern.contains("file") {
        return ErrorCategory::System;
    }
    if pattern.contains("I/O error") {
        return ErrorCategory::System;
    }
    if pattern.contains("crash shutdown") {
        return ErrorCategory::System;
    }
    if pattern.contains("server process") && pattern.contains("terminated") {
        return ErrorCategory::System;
    }
    if pattern.contains("shutting down") {
        return ErrorCategory::System;
    }
    if pattern.contains("not accepting commands") {
        return ErrorCategory::System;
    }

    // --- Connection ---
    if pattern.contains("connection reset by peer") {
        return ErrorCategory::Connection;
    }
    if pattern.contains("unexpected EOF") {
        return ErrorCategory::Connection;
    }
    if pattern.contains("broken pipe") {
        return ErrorCategory::Connection;
    }
    if pattern.contains("could not receive data from client") {
        return ErrorCategory::Connection;
    }
    if pattern.contains("could not send data to client") {
        return ErrorCategory::Connection;
    }
    if pattern.contains("terminating connection") {
        return ErrorCategory::Connection;
    }
    if pattern.contains("connection lost") {
        return ErrorCategory::Connection;
    }

    // --- Auth ---
    if pattern.contains("password authentication failed") {
        return ErrorCategory::Auth;
    }
    if pattern.contains("no pg_hba.conf entry") {
        return ErrorCategory::Auth;
    }
    if pattern.starts_with("role") && pattern.contains("does not exist") {
        return ErrorCategory::Auth;
    }
    if pattern.starts_with("permission denied") {
        return ErrorCategory::Auth;
    }
    if pattern.contains("SSL connection is required") {
        return ErrorCategory::Auth;
    }

    // --- Syntax (broad catch-all for SQL errors) ---
    if pattern.starts_with("syntax error") {
        return ErrorCategory::Syntax;
    }
    if pattern.contains("does not exist") {
        return ErrorCategory::Syntax;
    }
    if pattern.contains("invalid input syntax") {
        return ErrorCategory::Syntax;
    }
    if pattern.contains("division by zero") {
        return ErrorCategory::Syntax;
    }
    if pattern.contains("value too long") {
        return ErrorCategory::Syntax;
    }
    if pattern.contains("numeric value out of range") {
        return ErrorCategory::Syntax;
    }
    if pattern.contains("cannot coerce") {
        return ErrorCategory::Syntax;
    }

    // FATAL without specific classification → System
    if severity == PgLogSeverity::Fatal {
        return ErrorCategory::System;
    }

    ErrorCategory::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_double_quoted() {
        assert_eq!(
            normalize_error(r#"relation "users" does not exist"#),
            r#"relation "..." does not exist"#
        );
    }

    #[test]
    fn test_normalize_at_character() {
        assert_eq!(
            normalize_error(r#"column "..." does not exist at character 42"#),
            r#"column "..." does not exist"#
        );
    }

    #[test]
    fn test_normalize_single_quoted() {
        assert_eq!(
            normalize_error("unterminated quoted string at or near 'hello world'"),
            "unterminated quoted string at or near '...'"
        );
    }

    #[test]
    fn test_normalize_parenthesized() {
        assert_eq!(
            normalize_error("duplicate key value violates unique constraint (some detail)"),
            "duplicate key value violates unique constraint (...)"
        );
    }

    #[test]
    fn test_normalize_transaction_number() {
        assert_eq!(
            normalize_error("transaction 12345 was already aborted"),
            "transaction ... was already aborted"
        );
    }

    #[test]
    fn test_normalize_pid() {
        assert_eq!(
            normalize_error("canceling statement due to lock timeout PID 9876"),
            "canceling statement due to lock timeout PID ..."
        );
    }

    #[test]
    fn test_normalize_invalid_input_syntax() {
        assert_eq!(
            normalize_error("invalid input syntax for integer: abc123"),
            "invalid input syntax for integer: ..."
        );
    }

    #[test]
    fn test_normalize_wal_address() {
        assert_eq!(
            normalize_error("recovery target x/x is not reached"),
            normalize_error("recovery target 0/1A2B3C4D is not reached")
        );
    }

    #[test]
    fn test_normalize_truncation() {
        let long_msg = "a".repeat(500);
        let normalized = normalize_error(&long_msg);
        assert!(normalized.len() <= MAX_LOG_MESSAGE_LEN);
    }

    #[test]
    fn test_normalize_empty() {
        assert_eq!(normalize_error(""), "");
    }

    #[test]
    fn test_normalize_no_changes_needed() {
        assert_eq!(normalize_error("deadlock detected"), "deadlock detected");
    }

    #[test]
    fn test_normalize_multiple_quoted() {
        assert_eq!(
            normalize_error(r#"column "col1" of relation "tbl1" does not exist"#),
            r#"column "..." of relation "..." does not exist"#
        );
    }

    #[test]
    fn test_normalize_after_timeout() {
        assert_eq!(
            normalize_error("canceling statement due to statement timeout after 30000.123"),
            "canceling statement due to statement timeout after ..."
        );
    }

    // ---- classify_error tests ----

    use crate::storage::model::{ErrorCategory, PgLogSeverity};

    #[test]
    fn test_classify_lock() {
        assert_eq!(
            classify_error("deadlock detected", PgLogSeverity::Error),
            ErrorCategory::Lock
        );
        assert_eq!(
            classify_error(
                "could not obtain lock on row in relation \"...\"",
                PgLogSeverity::Error
            ),
            ErrorCategory::Lock
        );
        assert_eq!(
            classify_error(
                "canceling statement due to lock timeout",
                PgLogSeverity::Error
            ),
            ErrorCategory::Lock
        );
    }

    #[test]
    fn test_classify_constraint() {
        assert_eq!(
            classify_error(
                "duplicate key value violates unique constraint \"...\"",
                PgLogSeverity::Error
            ),
            ErrorCategory::Constraint
        );
        assert_eq!(
            classify_error(
                "insert or update on table \"...\" violates foreign key constraint \"...\"",
                PgLogSeverity::Error
            ),
            ErrorCategory::Constraint
        );
        assert_eq!(
            classify_error(
                "null value in column \"...\" violates not-null constraint",
                PgLogSeverity::Error
            ),
            ErrorCategory::Constraint
        );
        assert_eq!(
            classify_error(
                "new row for relation \"...\" violates check constraint \"...\"",
                PgLogSeverity::Error
            ),
            ErrorCategory::Constraint
        );
    }

    #[test]
    fn test_classify_serialization() {
        assert_eq!(
            classify_error(
                "could not serialize access due to concurrent update",
                PgLogSeverity::Error
            ),
            ErrorCategory::Serialization
        );
        assert_eq!(
            classify_error(
                "could not serialize access due to read/write dependencies among transactions",
                PgLogSeverity::Error
            ),
            ErrorCategory::Serialization
        );
    }

    #[test]
    fn test_classify_timeout() {
        assert_eq!(
            classify_error(
                "canceling statement due to statement timeout after ...",
                PgLogSeverity::Error
            ),
            ErrorCategory::Timeout
        );
        assert_eq!(
            classify_error(
                "canceling statement due to user request",
                PgLogSeverity::Error
            ),
            ErrorCategory::Timeout
        );
        assert_eq!(
            classify_error(
                "terminating connection due to idle-in-transaction session timeout",
                PgLogSeverity::Fatal
            ),
            ErrorCategory::Timeout
        );
    }

    #[test]
    fn test_classify_connection() {
        assert_eq!(
            classify_error("connection reset by peer", PgLogSeverity::Fatal),
            ErrorCategory::Connection
        );
        assert_eq!(
            classify_error("unexpected EOF on client connection", PgLogSeverity::Fatal),
            ErrorCategory::Connection
        );
        assert_eq!(
            classify_error("broken pipe", PgLogSeverity::Error),
            ErrorCategory::Connection
        );
    }

    #[test]
    fn test_classify_auth() {
        assert_eq!(
            classify_error(
                "password authentication failed for user \"...\"",
                PgLogSeverity::Fatal
            ),
            ErrorCategory::Auth
        );
        assert_eq!(
            classify_error(
                "no pg_hba.conf entry for host \"...\", user \"...\", database \"...\"",
                PgLogSeverity::Fatal
            ),
            ErrorCategory::Auth
        );
        assert_eq!(
            classify_error("permission denied for table ...", PgLogSeverity::Error),
            ErrorCategory::Auth
        );
    }

    #[test]
    fn test_classify_syntax() {
        assert_eq!(
            classify_error("syntax error at or near \"...\"", PgLogSeverity::Error),
            ErrorCategory::Syntax
        );
        assert_eq!(
            classify_error(
                "column \"...\" of relation \"...\" does not exist",
                PgLogSeverity::Error
            ),
            ErrorCategory::Syntax
        );
        assert_eq!(
            classify_error(
                "invalid input syntax for integer: ...",
                PgLogSeverity::Error
            ),
            ErrorCategory::Syntax
        );
        assert_eq!(
            classify_error("division by zero", PgLogSeverity::Error),
            ErrorCategory::Syntax
        );
    }

    #[test]
    fn test_classify_resource() {
        assert_eq!(
            classify_error("out of memory", PgLogSeverity::Error),
            ErrorCategory::Resource
        );
        assert_eq!(
            classify_error("out of shared memory", PgLogSeverity::Error),
            ErrorCategory::Resource
        );
        assert_eq!(
            classify_error(
                "too many connections for role \"...\"",
                PgLogSeverity::Fatal
            ),
            ErrorCategory::Resource
        );
        assert_eq!(
            classify_error("disk full", PgLogSeverity::Error),
            ErrorCategory::Resource
        );
        assert_eq!(
            classify_error(
                "could not extend file \"...\": No space left on device",
                PgLogSeverity::Error
            ),
            ErrorCategory::Resource
        );
        assert_eq!(
            classify_error(
                "remaining connection slots are reserved for non-replication superuser connections",
                PgLogSeverity::Fatal
            ),
            ErrorCategory::Resource
        );
    }

    #[test]
    fn test_classify_data_corruption() {
        assert_eq!(
            classify_error(
                "invalid page in block ... of relation \"...\"",
                PgLogSeverity::Error
            ),
            ErrorCategory::DataCorruption
        );
        assert_eq!(
            classify_error(
                "index \"...\" contains unexpected zero page at block ...",
                PgLogSeverity::Error
            ),
            ErrorCategory::DataCorruption
        );
        assert_eq!(
            classify_error("something went wrong", PgLogSeverity::Panic),
            ErrorCategory::DataCorruption
        );
    }

    #[test]
    fn test_classify_system() {
        assert_eq!(
            classify_error(
                "could not open file \"...\": No such file or directory",
                PgLogSeverity::Error
            ),
            ErrorCategory::System
        );
        assert_eq!(
            classify_error("I/O error on read", PgLogSeverity::Error),
            ErrorCategory::System
        );
        assert_eq!(
            classify_error("the database system is shutting down", PgLogSeverity::Fatal),
            ErrorCategory::System
        );
    }

    #[test]
    fn test_classify_fatal_fallback_to_system() {
        assert_eq!(
            classify_error("some unknown fatal error", PgLogSeverity::Fatal),
            ErrorCategory::System
        );
    }

    #[test]
    fn test_classify_other() {
        assert_eq!(
            classify_error("some completely unknown error", PgLogSeverity::Error),
            ErrorCategory::Other
        );
    }

    #[test]
    fn test_classify_role_does_not_exist() {
        assert_eq!(
            classify_error("role \"...\" does not exist", PgLogSeverity::Fatal),
            ErrorCategory::Auth
        );
    }
}
