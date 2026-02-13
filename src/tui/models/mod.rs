//! View mode enums, ProcessRow, and formatting helpers.

mod formatting;
mod process_row;

pub use process_row::*;

/// Process table view mode (similar to atop).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProcessViewMode {
    /// Generic view: PID SYSCPU USRCPU RDELAY VGROW RGROW RUID EUID ST EXC THR S CPUNR CPU CMD
    #[default]
    Generic,
    /// Command view: PID TID S MEM COMMAND-LINE
    Command,
    /// Memory view: PID TID MINFLT MAJFLT VSTEXT VSLIBS VDATA VSTACK LOCKSZ VSIZE RSIZE PSIZE VGROW RGROW SWAPSZ RUID EUID MEM CMD
    Memory,
    /// Disk I/O view: PID RDDSK WRDSK WCANCL DSK CMD
    Disk,
}

/// pg_stat_statements table view modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PgStatementsViewMode {
    /// Time view (timing-focused).
    #[default]
    Time,
    /// Calls view (frequency-focused).
    Calls,
    /// I/O view (buffer/cache focused).
    Io,
    /// Temp view (temp blocks / spill focused).
    Temp,
}

impl PgStatementsViewMode {
    /// Default sort column index for this view mode.
    pub fn default_sort_column(&self) -> usize {
        match self {
            Self::Time => 1,  // TIME/s
            Self::Calls => 0, // CALLS/s
            Self::Io => 1,    // BLK_RD/s
            Self::Temp => 3,  // TMP_MB/s
        }
    }

    /// Number of columns in this view mode.
    pub fn column_count(&self) -> usize {
        match self {
            Self::Time => 7,  // CALLS/s TIME/s MEAN ROWS/s DB USER QUERY
            Self::Calls => 7, // CALLS/s ROWS/s R/CALL MEAN DB USER QUERY
            Self::Io => 8,    // CALLS/s BLK_RD/s BLK_HIT/s HIT% BLK_DIRT/s BLK_WR/s DB QUERY
            Self::Temp => 8,  // CALLS/s TMP_RD/s TMP_WR/s TMP_MB/s LOC_RD/s LOC_WR/s DB QUERY
        }
    }
}

/// pg_stat_activity table view modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PgActivityViewMode {
    /// Generic view: PID, CPU%, RSS, DB, USER, STATE, WAIT, QDUR, XDUR, BDUR, BTYPE, QUERY
    #[default]
    Generic,
    /// Stats view: PID, DB, USER, STATE, QDUR, MEAN, MAX, CALL/s, HIT%, QUERY
    /// Shows pg_stat_statements metrics for the current query (linked by query_id).
    Stats,
}

impl PgActivityViewMode {
    /// Default sort column index for this view mode.
    pub fn default_sort_column(&self) -> usize {
        match self {
            Self::Generic => 7, // QDUR
            Self::Stats => 4,   // QDUR
        }
    }

    /// Number of columns in this view mode.
    pub fn column_count(&self) -> usize {
        match self {
            Self::Generic => 12, // PID CPU% RSS DB USER STATE WAIT QDUR XDUR BDUR BTYPE QUERY
            Self::Stats => 10,   // PID DB USER STATE QDUR MEAN MAX CALL/s HIT% QUERY
        }
    }
}

/// Rate metrics for a single `pg_stat_statements` entry.
///
/// Rates are computed from deltas between two **real samples** of statement counters,
/// not between every TUI tick (collector may cache `pg_stat_statements` for ~30s).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PgStatementsRates {
    pub dt_secs: f64,
    pub calls_s: Option<f64>,
    pub rows_s: Option<f64>,
    /// Execution time rate in `ms/s`.
    pub exec_time_ms_s: Option<f64>,

    pub shared_blks_read_s: Option<f64>,
    pub shared_blks_hit_s: Option<f64>,
    pub shared_blks_dirtied_s: Option<f64>,
    pub shared_blks_written_s: Option<f64>,

    pub local_blks_read_s: Option<f64>,
    pub local_blks_written_s: Option<f64>,

    pub temp_blks_read_s: Option<f64>,
    pub temp_blks_written_s: Option<f64>,
    /// Temp I/O rate in `MB/s` (assumes 8 KiB blocks).
    pub temp_mb_s: Option<f64>,
}
