//! Shared formatting helpers for TUI widgets.
//!
//! All pure formatting functions (no ratatui styles, no UI layout) live here.
//! Functions that differ between compact table columns and verbose detail popups
//! are parameterized via [`FmtStyle`].

use std::time::{SystemTime, UNIX_EPOCH};

/// Controls compact (table columns) vs verbose (detail popups) output.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FmtStyle {
    /// Compact: no spaces, short suffixes ("1.5G", "3m5s")
    Compact,
    /// Detail: spaces, full suffixes ("1.5 GiB", "3m 5s")
    Detail,
}

// ---------------------------------------------------------------------------
// Style-parameterized functions
// ---------------------------------------------------------------------------

/// Format byte count as human-readable size.
///
/// Compact: `"1.5G"`, `"100.3M"`, `"50.0K"`, `"512B"`
/// Detail:  `"1.5 GiB"`, `"100.3 MiB"`, `"50.0 KiB"`, `"512 B"`
pub fn format_bytes(bytes: u64, style: FmtStyle) -> String {
    let (g, m, k, b) = match style {
        FmtStyle::Compact => ("G", "M", "K", "B"),
        FmtStyle::Detail => (" GiB", " MiB", " KiB", " B"),
    };
    let f = bytes as f64;
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1}{}", f / (1024.0 * 1024.0 * 1024.0), g)
    } else if bytes >= 1024 * 1024 {
        format!("{:.1}{}", f / (1024.0 * 1024.0), m)
    } else if bytes >= 1024 {
        format!("{:.1}{}", f / 1024.0, k)
    } else {
        format!("{}{}", bytes, b)
    }
}

/// Format duration in seconds as human-readable.
///
/// Compact: `"3m5s"` (no spaces, `"-"` for negative)
/// Detail:  `"3m 5s"` (with spaces, `"0s"` for `<= 0`)
pub fn format_duration(secs: i64, style: FmtStyle) -> String {
    match style {
        FmtStyle::Compact => {
            if secs < 0 {
                return "-".to_string();
            }
            if secs < 60 {
                format!("{}s", secs)
            } else if secs < 3600 {
                format!("{}m{}s", secs / 60, secs % 60)
            } else if secs < 86400 {
                format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
            } else {
                format!("{}d{}h", secs / 86400, (secs % 86400) / 3600)
            }
        }
        FmtStyle::Detail => {
            if secs <= 0 {
                return "0s".to_string();
            }
            if secs < 60 {
                format!("{}s", secs)
            } else if secs < 3600 {
                format!("{}m {}s", secs / 60, secs % 60)
            } else if secs < 86400 {
                format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
            } else {
                format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
            }
        }
    }
}

/// Format bytes-per-second rate as human-readable.
///
/// Compact: `"1.5G/s"`, `"100.3M/s"`
/// Detail:  `"1.5 GiB/s"`, `"100.3 MiB/s"`
pub fn format_bytes_rate(rate: f64, style: FmtStyle) -> String {
    if rate < 1.0 {
        return "0".to_string();
    }
    let (g, m, k, b) = match style {
        FmtStyle::Compact => ("G/s", "M/s", "K/s", "B/s"),
        FmtStyle::Detail => (" GiB/s", " MiB/s", " KiB/s", " B/s"),
    };
    if rate >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1}{}", rate / (1024.0 * 1024.0 * 1024.0), g)
    } else if rate >= 1024.0 * 1024.0 {
        format!("{:.1}{}", rate / (1024.0 * 1024.0), m)
    } else if rate >= 1024.0 {
        format!("{:.1}{}", rate / 1024.0, k)
    } else {
        format!("{:.0}{}", rate, b)
    }
}

/// Format ops-per-second rate as human-readable.
///
/// Compact: always `"{:.0}/s"` for rates < 1000
/// Detail:  `"{:.0}/s"` for >= 10, `"{:.1}/s"` for < 10
pub fn format_rate(rate: f64, style: FmtStyle) -> String {
    if rate < 0.01 {
        return "0".to_string();
    }
    if rate >= 1_000_000.0 {
        format!("{:.1}M/s", rate / 1_000_000.0)
    } else if rate >= 1_000.0 {
        format!("{:.1}K/s", rate / 1_000.0)
    } else {
        match style {
            FmtStyle::Compact => format!("{:.0}/s", rate),
            FmtStyle::Detail => {
                if rate >= 10.0 {
                    format!("{:.0}/s", rate)
                } else {
                    format!("{:.1}/s", rate)
                }
            }
        }
    }
}

/// Format milliseconds as human-readable.
///
/// Compact: has minute case (`>= 60_000` -> `"1.5m"`)
/// Detail:  no minute case, has sub-ms case (`< 1.0` -> `"0.5ms"`)
pub fn format_ms(ms: f64, style: FmtStyle) -> String {
    match style {
        FmtStyle::Compact => {
            if ms >= 60_000.0 {
                format!("{:.1}m", ms / 60_000.0)
            } else if ms >= 1_000.0 {
                format!("{:.1}s", ms / 1_000.0)
            } else {
                format!("{:.0}ms", ms)
            }
        }
        FmtStyle::Detail => {
            if ms >= 1000.0 {
                format!("{:.1}s", ms / 1000.0)
            } else if ms >= 1.0 {
                format!("{:.0}ms", ms)
            } else {
                format!("{:.1}ms", ms)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Style-independent functions
// ---------------------------------------------------------------------------

/// Format duration or `"-"` for zero/invalid.
pub fn format_duration_or_none(secs: i64) -> String {
    if secs <= 0 {
        "-".to_string()
    } else {
        format_duration(secs, FmtStyle::Detail)
    }
}

/// Format epoch timestamp as age from now, or `"-"` for zero/invalid.
/// Uses Detail style (with spaces): `"3m 5s"`.
pub fn format_epoch_age(epoch_secs: i64) -> String {
    if epoch_secs <= 0 {
        return "-".to_string();
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let age = now.saturating_sub(epoch_secs);
    if age < 0 {
        return "-".to_string();
    }
    format_duration(age, FmtStyle::Detail)
}

/// Format epoch timestamp as compact age: `"3s"`, `"5m"`, `"2h"`, `"7d"`.
/// For table columns (single unit, no spaces).
pub fn format_age(epoch_secs: i64) -> String {
    if epoch_secs == 0 {
        return "-".to_string();
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let age = now.saturating_sub(epoch_secs);
    if age < 0 {
        return "-".to_string();
    }
    if age < 60 {
        format!("{}s", age)
    } else if age < 3600 {
        format!("{}m", age / 60)
    } else if age < 86400 {
        format!("{}h", age / 3600)
    } else {
        format!("{}d", age / 86400)
    }
}

/// Format signed bytes with +/- prefix (Detail style).
pub fn format_bytes_signed(bytes: i64) -> String {
    let sign = if bytes >= 0 { "+" } else { "-" };
    let abs = bytes.unsigned_abs();
    if abs >= 1024 * 1024 * 1024 {
        format!("{}{:.1} GiB", sign, abs as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if abs >= 1024 * 1024 {
        format!("{}{:.1} MiB", sign, abs as f64 / (1024.0 * 1024.0))
    } else if abs >= 1024 {
        format!("{}{:.1} KiB", sign, abs as f64 / 1024.0)
    } else {
        format!("{}{} B", sign, abs)
    }
}

/// Format KiB to human-readable size.
pub fn format_kb(kb: u64) -> String {
    if kb == 0 {
        return "0".to_string();
    }
    if kb >= 1024 * 1024 {
        format!("{:.1} GiB", kb as f64 / (1024.0 * 1024.0))
    } else if kb >= 1024 {
        format!("{:.1} MiB", kb as f64 / 1024.0)
    } else {
        format!("{} KiB", kb)
    }
}

/// Format CPU ticks to human-readable time.
pub fn format_ticks(ticks: u64) -> String {
    if ticks == 0 {
        return "0".to_string();
    }
    let secs = ticks / 100;
    let ms = (ticks % 100) * 10;
    if secs > 3600 {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    } else if secs > 60 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else if secs > 0 {
        format!("{}.{}s", secs, ms / 100)
    } else {
        format!("{}ms", ms)
    }
}

/// Format nanoseconds to human-readable.
pub fn format_ns(ns: u64) -> String {
    if ns == 0 {
        return "0".to_string();
    }
    let ms = ns / 1_000_000;
    if ms > 1000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else if ms > 0 {
        format!("{}ms", ms)
    } else {
        format!("{}us", ns / 1000)
    }
}

/// Format memory delta (can be negative) in KiB.
pub fn format_delta_kb(delta: i64) -> String {
    if delta == 0 {
        return "0".to_string();
    }
    let abs = delta.unsigned_abs();
    let sign = if delta < 0 { "-" } else { "+" };
    if abs >= 1024 * 1024 {
        format!("{}{:.1} GiB", sign, abs as f64 / (1024.0 * 1024.0))
    } else if abs >= 1024 {
        format!("{}{:.1} MiB", sign, abs as f64 / 1024.0)
    } else {
        format!("{}{} KiB", sign, abs)
    }
}

// ---------------------------------------------------------------------------
// Table column formatting (fixed-width, right-aligned)
// ---------------------------------------------------------------------------

/// Format i64 with K/M/G suffix for table columns.
pub fn format_i64(v: i64, width: usize) -> String {
    if v >= 1_000_000_000 {
        format!("{:>width$.1}G", v as f64 / 1e9, width = width - 1)
    } else if v >= 1_000_000 {
        format!("{:>width$.1}M", v as f64 / 1e6, width = width - 1)
    } else if v >= 10_000 {
        format!("{:>width$.1}K", v as f64 / 1e3, width = width - 1)
    } else {
        format!("{:>width$}", v, width = width)
    }
}

/// Format byte size for table columns (compact, right-aligned, 9 chars).
pub fn format_size(bytes: i64) -> String {
    if bytes <= 0 {
        return format!("{:>9}", "-");
    }
    let b = bytes as f64;
    if b >= 1_099_511_627_776.0 {
        format!("{:>8.1}T", b / 1_099_511_627_776.0)
    } else if b >= 1_073_741_824.0 {
        format!("{:>8.1}G", b / 1_073_741_824.0)
    } else if b >= 1_048_576.0 {
        format!("{:>8.1}M", b / 1_048_576.0)
    } else if b >= 1024.0 {
        format!("{:>8.1}K", b / 1024.0)
    } else {
        format!("{:>8}B", bytes)
    }
}

/// Format `Option<f64>` with width and precision, `"--"` for `None`.
pub fn format_opt_f64(v: Option<f64>, width: usize, precision: usize) -> String {
    match v {
        Some(v) => format!("{:>width$.prec$}", v, width = width, prec = precision),
        None => format!("{:>width$}", "--", width = width),
    }
}

/// Format block rate (blocks/s) as human-readable bytes/s.
/// Each PostgreSQL block is 8192 bytes (8 KB).
pub fn format_blks_rate(blks_per_sec: Option<f64>, width: usize) -> String {
    match blks_per_sec {
        None => format!("{:>width$}", "--", width = width),
        Some(v) => {
            let bytes = v * 8192.0;
            if bytes >= 1_073_741_824.0 {
                format!("{:>width$.1}G", bytes / 1_073_741_824.0, width = width - 1)
            } else if bytes >= 1_048_576.0 {
                format!("{:>width$.1}M", bytes / 1_048_576.0, width = width - 1)
            } else if bytes >= 1024.0 {
                format!("{:>width$.1}K", bytes / 1024.0, width = width - 1)
            } else if bytes >= 1.0 {
                format!("{:>width$.0}B", bytes, width = width - 1)
            } else {
                format!("{:>width$}", "0", width = width)
            }
        }
    }
}

/// Format bytes count (from blocks * 8192) to human-readable.
#[allow(dead_code)]
pub(crate) fn format_blks_as_bytes(bytes: f64) -> String {
    let abs = bytes.abs();
    if abs >= 1_099_511_627_776.0 {
        format!("{:.1} TB", bytes / 1_099_511_627_776.0)
    } else if abs >= 1_073_741_824.0 {
        format!("{:.1} GB", bytes / 1_073_741_824.0)
    } else if abs >= 1_048_576.0 {
        format!("{:.1} MB", bytes / 1_048_576.0)
    } else if abs >= 1024.0 {
        format!("{:.1} KB", bytes / 1024.0)
    } else if abs >= 1.0 {
        format!("{:.0} B", bytes)
    } else {
        "0".to_string()
    }
}

// ---------------------------------------------------------------------------
// Text normalization
// ---------------------------------------------------------------------------

/// Truncate string to max length with unicode ellipsis (`…`).
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len.saturating_sub(1)])
    }
}

/// Normalize query text for single-line display.
/// Replaces newlines, carriage returns, and tabs with spaces.
pub fn normalize_query(s: &str) -> String {
    s.replace('\n', " ").replace('\r', "").replace('\t', " ")
}

/// Normalize text for single-line display with space collapsing.
/// Like [`normalize_query`] but also collapses multiple consecutive spaces into one.
pub fn normalize_for_display(s: &str) -> String {
    let s = s.replace('\n', " ").replace('\r', "").replace('\t', " ");
    let mut result = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch == ' ' {
            if !prev_space {
                result.push(ch);
            }
            prev_space = true;
        } else {
            result.push(ch);
            prev_space = false;
        }
    }
    result
}
