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
}
