//! Flexible time parser for CLI arguments.
//!
//! Supports multiple formats:
//! - ISO 8601: `2026-02-07T17:00:00`
//! - Unix timestamp: `1738944000`
//! - Relative: `-1h`, `-30m`, `-2d`
//! - Date+time (UTC): `2026-02-07:07:00` or `2026-02-07:07:00:00`
//! - Time only (current day, UTC): `07:00`

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};

/// Error type for time parsing failures.
#[derive(Debug, Clone)]
pub struct TimeParseError {
    pub input: String,
    pub message: String,
}

impl std::fmt::Display for TimeParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Failed to parse time '{}': {}", self.input, self.message)
    }
}

impl std::error::Error for TimeParseError {}

/// Parse a flexible time string into a Unix timestamp (seconds since epoch).
///
/// # Supported formats
///
/// | Format | Example | Description |
/// |--------|---------|-------------|
/// | ISO 8601 | `2026-02-07T17:00:00` | Full datetime |
/// | Unix timestamp | `1738944000` | Seconds since epoch |
/// | Relative | `-1h`, `-30m`, `-2d` | Relative to now |
/// | Date+time | `2026-02-07:07:00` | UTC, colon separator |
/// | Date+time+sec | `2026-02-07:07:00:00` | UTC, with seconds |
/// | Time only | `07:00` | Current day, UTC |
///
/// # Examples
///
/// ```
/// use rpglot::util::parse_time;
///
/// // Relative time
/// let ts = parse_time("-1h").unwrap();
///
/// // Time only (today)
/// let ts = parse_time("07:00").unwrap();
/// ```
pub fn parse_time(input: &str) -> Result<i64, TimeParseError> {
    let input = input.trim();

    // Try each format in order
    if let Some(ts) = try_parse_unix_timestamp(input) {
        return Ok(ts);
    }

    if let Some(ts) = try_parse_relative(input) {
        return Ok(ts);
    }

    if let Some(ts) = try_parse_iso8601(input) {
        return Ok(ts);
    }

    if let Some(ts) = try_parse_date_colon_time(input) {
        return Ok(ts);
    }

    if let Some(ts) = try_parse_time_only(input) {
        return Ok(ts);
    }

    Err(TimeParseError {
        input: input.to_string(),
        message: "Unrecognized format. Use: ISO 8601 (2026-02-07T17:00:00), \
                  Unix timestamp (1738944000), relative (-1h, -30m, -2d), \
                  date:time (2026-02-07:07:00), or time only (07:00)"
            .to_string(),
    })
}

/// Parses a time expression in UTC using `base_ts` as a reference.
///
/// Supported formats are the same as `parse_time()`, with different semantics for:
/// - Relative time (`-1h`, `-30m`, ...): relative to `base_ts`.
/// - Time only (`HH:MM`): interpreted as that time on the day of `base_ts`.
pub fn parse_time_with_base(input: &str, base_ts: i64) -> Result<i64, TimeParseError> {
    let input = input.trim();

    if let Some(ts) = try_parse_unix_timestamp(input) {
        return Ok(ts);
    }

    if let Some(delta_secs) = try_parse_relative_delta_seconds(input) {
        return base_ts.checked_add(delta_secs).ok_or(TimeParseError {
            input: input.to_string(),
            message: "Relative time overflow".to_string(),
        });
    }

    if let Some(ts) = try_parse_iso8601(input) {
        return Ok(ts);
    }

    if let Some(ts) = try_parse_date_colon_time(input) {
        return Ok(ts);
    }

    if let Some(ts) = try_parse_time_only_on_base_date(input, base_ts) {
        return Ok(ts);
    }

    Err(TimeParseError {
        input: input.to_string(),
        message: "Unrecognized format. Use: ISO 8601 (2026-02-07T17:00:00), \
                  Unix timestamp (1738944000), relative (-1h, -30m, -2d), \
                  date:time (2026-02-07:07:00), or time only (07:00)"
            .to_string(),
    })
}

/// Try to parse as Unix timestamp (plain integer).
fn try_parse_unix_timestamp(input: &str) -> Option<i64> {
    // Must be all digits (possibly with leading minus for negative, but we don't expect that)
    if input.chars().all(|c| c.is_ascii_digit()) && !input.is_empty() {
        input.parse::<i64>().ok()
    } else {
        None
    }
}

/// Try to parse as relative time (-1h, -30m, -2d, -1w).
fn try_parse_relative(input: &str) -> Option<i64> {
    if !input.starts_with('-') {
        return None;
    }

    let rest = &input[1..];
    if rest.is_empty() {
        return None;
    }

    // Extract number and unit
    let unit = rest.chars().last()?;
    let number_str = &rest[..rest.len() - 1];

    if number_str.is_empty() {
        return None;
    }

    let number: i64 = number_str.parse().ok()?;

    let seconds = match unit {
        's' => number,
        'm' => number * 60,
        'h' => number * 3600,
        'd' => number * 86400,
        'w' => number * 604800,
        _ => return None,
    };

    let now = Utc::now().timestamp();
    Some(now - seconds)
}

/// Parses relative expression and returns delta seconds (negative value).
fn try_parse_relative_delta_seconds(input: &str) -> Option<i64> {
    if !input.starts_with('-') {
        return None;
    }

    let rest = &input[1..];
    if rest.is_empty() {
        return None;
    }

    let unit = rest.chars().last()?;
    let number_str = &rest[..rest.len() - 1];
    if number_str.is_empty() {
        return None;
    }
    let number: i64 = number_str.parse().ok()?;

    let seconds = match unit {
        's' => number,
        'm' => number * 60,
        'h' => number * 3600,
        'd' => number * 86400,
        'w' => number * 604800,
        _ => return None,
    };

    Some(-seconds)
}

/// Try to parse as ISO 8601 datetime.
fn try_parse_iso8601(input: &str) -> Option<i64> {
    // Try with 'T' separator
    if input.contains('T') {
        // Try parsing as DateTime<Utc> first (with timezone)
        if let Ok(dt) = DateTime::parse_from_rfc3339(input) {
            return Some(dt.with_timezone(&Utc).timestamp());
        }

        // Try as NaiveDateTime (no timezone, assume UTC)
        if let Ok(ndt) = NaiveDateTime::parse_from_str(input, "%Y-%m-%dT%H:%M:%S") {
            return Some(Utc.from_utc_datetime(&ndt).timestamp());
        }

        // Try without seconds
        if let Ok(ndt) = NaiveDateTime::parse_from_str(input, "%Y-%m-%dT%H:%M") {
            return Some(Utc.from_utc_datetime(&ndt).timestamp());
        }
    }

    None
}

/// Try to parse as date:time format (2026-02-07:07:00 or 2026-02-07:07:00:00).
fn try_parse_date_colon_time(input: &str) -> Option<i64> {
    // Format: YYYY-MM-DD:HH:MM or YYYY-MM-DD:HH:MM:SS
    // The date part has hyphens, then colon separates date from time

    // Check if it looks like a date:time format
    if !input.contains('-') {
        return None;
    }

    // Split on first colon that comes after the date part
    // Date format: YYYY-MM-DD (10 chars)
    if input.len() < 11 {
        return None;
    }

    let date_part = &input[..10];
    if !input[10..].starts_with(':') {
        return None;
    }

    let time_part = &input[11..];

    // Parse date
    let date = NaiveDate::parse_from_str(date_part, "%Y-%m-%d").ok()?;

    // Parse time (HH:MM or HH:MM:SS)
    let time = if time_part.len() == 5 {
        NaiveTime::parse_from_str(time_part, "%H:%M").ok()?
    } else if time_part.len() == 8 {
        NaiveTime::parse_from_str(time_part, "%H:%M:%S").ok()?
    } else {
        return None;
    };

    let datetime = NaiveDateTime::new(date, time);
    Some(Utc.from_utc_datetime(&datetime).timestamp())
}

/// Try to parse as time only (07:00 = today at that time, UTC).
fn try_parse_time_only(input: &str) -> Option<i64> {
    // Format: HH:MM
    if input.len() != 5 {
        return None;
    }

    if input.chars().nth(2) != Some(':') {
        return None;
    }

    let time = NaiveTime::parse_from_str(input, "%H:%M").ok()?;
    let today = Utc::now().date_naive();
    let datetime = NaiveDateTime::new(today, time);

    Some(Utc.from_utc_datetime(&datetime).timestamp())
}

fn try_parse_time_only_on_base_date(input: &str, base_ts: i64) -> Option<i64> {
    if input.len() != 5 {
        return None;
    }
    if input.chars().nth(2) != Some(':') {
        return None;
    }

    let time = NaiveTime::parse_from_str(input, "%H:%M").ok()?;
    let base_dt = Utc.timestamp_opt(base_ts, 0).single()?;
    let base_date = base_dt.date_naive();

    let datetime = NaiveDateTime::new(base_date, time);
    Some(Utc.from_utc_datetime(&datetime).timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unix_timestamp() {
        assert_eq!(parse_time("1738944000").unwrap(), 1738944000);
        assert_eq!(parse_time("0").unwrap(), 0);
        assert_eq!(parse_time("1234567890").unwrap(), 1234567890);
    }

    #[test]
    fn test_relative_time() {
        let now = Utc::now().timestamp();

        let ts = parse_time("-1h").unwrap();
        assert!((ts - (now - 3600)).abs() < 2);

        let ts = parse_time("-30m").unwrap();
        assert!((ts - (now - 1800)).abs() < 2);

        let ts = parse_time("-2d").unwrap();
        assert!((ts - (now - 172800)).abs() < 2);

        let ts = parse_time("-1w").unwrap();
        assert!((ts - (now - 604800)).abs() < 2);

        let ts = parse_time("-60s").unwrap();
        assert!((ts - (now - 60)).abs() < 2);
    }

    #[test]
    fn test_iso8601() {
        // Calculate expected timestamp for 2026-02-07T17:00:00 UTC
        let expected = NaiveDateTime::new(
            NaiveDate::from_ymd_opt(2026, 2, 7).unwrap(),
            NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
        );
        let expected_ts = Utc.from_utc_datetime(&expected).timestamp();

        let ts = parse_time("2026-02-07T17:00:00").unwrap();
        assert_eq!(ts, expected_ts);

        let ts = parse_time("2026-02-07T17:00").unwrap();
        assert_eq!(ts, expected_ts);
    }

    #[test]
    fn test_date_colon_time() {
        // Calculate expected timestamp for 2026-02-07T17:00:00 UTC
        let expected = NaiveDateTime::new(
            NaiveDate::from_ymd_opt(2026, 2, 7).unwrap(),
            NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
        );
        let expected_ts = Utc.from_utc_datetime(&expected).timestamp();

        let ts = parse_time("2026-02-07:17:00").unwrap();
        assert_eq!(ts, expected_ts);

        let ts = parse_time("2026-02-07:17:00:00").unwrap();
        assert_eq!(ts, expected_ts);
    }

    #[test]
    fn test_time_only() {
        let ts = parse_time("07:00").unwrap();
        let today = Utc::now().date_naive();
        let expected = NaiveDateTime::new(today, NaiveTime::from_hms_opt(7, 0, 0).unwrap());
        let expected_ts = Utc.from_utc_datetime(&expected).timestamp();
        assert_eq!(ts, expected_ts);
    }

    #[test]
    fn test_invalid_formats() {
        assert!(parse_time("").is_err());
        assert!(parse_time("invalid").is_err());
        assert!(parse_time("2026-02-07").is_err()); // date only, no time
        assert!(parse_time("-abc").is_err());
        assert!(parse_time("12:34:56:78").is_err());
    }

    #[test]
    fn test_parse_time_with_base_relative_and_time_only() {
        // base: 2026-02-08 10:00:00 UTC
        let base = Utc
            .with_ymd_and_hms(2026, 2, 8, 10, 0, 0)
            .single()
            .unwrap()
            .timestamp();

        // Relative should be based on `base`
        assert_eq!(parse_time_with_base("-1h", base).unwrap(), base - 3600);

        // Time-only should keep base date
        let expected_16 = Utc
            .with_ymd_and_hms(2026, 2, 8, 16, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(parse_time_with_base("16:00", base).unwrap(), expected_16);
    }
}
