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

/// pg_stat_user_tables view modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PgTablesViewMode {
    /// Reads view: SEQ_RD/s, IDX_FT/s, TOT_RD/s, SEQ/s, IDX/s, SIZE, TABLE
    Reads,
    /// Writes view: INS/s, UPD/s, DEL/s, HOT/s, LIVE, DEAD, HIT%, DISK/s, SIZE, TABLE
    Writes,
    /// Scans view: SEQ/s, SEQ_TUP/s, IDX/s, IDX_TUP/s, SEQ%, HIT%, DISK/s, SIZE, TABLE
    Scans,
    /// Maintenance view: DEAD, LIVE, DEAD%, VAC/s, AVAC/s, LAST_AVAC, LAST_AANL, TABLE
    Maintenance,
    /// I/O view: HEAP_RD/s, HEAP_HIT/s, IDX_RD/s, IDX_HIT/s, HIT%, MB/s, SIZE, TABLE
    #[default]
    Io,
}

impl PgTablesViewMode {
    /// Default sort column index for this view mode.
    pub fn default_sort_column(&self) -> usize {
        match self {
            Self::Reads => 2,       // TOT_RD/s
            Self::Writes => 0,      // INS/s
            Self::Scans => 4,       // SEQ%
            Self::Maintenance => 2, // DEAD%
            Self::Io => 5,          // MB/s
        }
    }

    /// Number of columns in this view mode.
    pub fn column_count(&self) -> usize {
        match self {
            Self::Reads => 9,   // SEQ_RD/s IDX_FT/s TOT_RD/s SEQ/s IDX/s HIT% MB/s SIZE TABLE
            Self::Writes => 10, // INS/s UPD/s DEL/s HOT/s LIVE DEAD HIT% DISK/s SIZE TABLE
            Self::Scans => 9,   // SEQ/s SEQ_TUP/s IDX/s IDX_TUP/s SEQ% HIT% DISK/s SIZE TABLE
            Self::Maintenance => 8, // DEAD LIVE DEAD% VAC/s AVAC/s LAST_AVAC LAST_AANL TABLE
            Self::Io => 8,      // HEAP_RD/s HEAP_HIT/s IDX_RD/s IDX_HIT/s HIT% MB/s SIZE TABLE
        }
    }
}

/// pg_stat_user_indexes view modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PgIndexesViewMode {
    /// Usage view: IDX/s, TUP_RD/s, TUP_FT/s, HIT%, MB/s, SIZE, TABLE, INDEX
    Usage,
    /// Unused/waste view: IDX_SCAN, SIZE, TABLE, INDEX (sorted ascending)
    Unused,
    /// I/O view: IDX_RD/s, IDX_HIT/s, HIT%, MB/s, SIZE, TABLE, INDEX
    #[default]
    Io,
}

impl PgIndexesViewMode {
    /// Default sort column index for this view mode.
    pub fn default_sort_column(&self) -> usize {
        match self {
            Self::Usage => 1,  // TUP_RD/s
            Self::Unused => 0, // IDX_SCAN
            Self::Io => 3,     // MB/s
        }
    }

    /// Number of columns in this view mode.
    pub fn column_count(&self) -> usize {
        match self {
            Self::Usage => 8,  // IDX/s TUP_RD/s TUP_FT/s HIT% MB/s SIZE TABLE INDEX
            Self::Unused => 4, // IDX_SCAN SIZE TABLE INDEX
            Self::Io => 7,     // IDX_RD/s IDX_HIT/s HIT% MB/s SIZE TABLE INDEX
        }
    }

    /// Whether this view mode defaults to ascending sort.
    pub fn default_sort_ascending(&self) -> bool {
        match self {
            Self::Usage => false, // highest usage first
            Self::Unused => true, // unused (0 scans) first
            Self::Io => false,    // highest I/O first
        }
    }
}

/// pg_store_plans view modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PgStorePlansViewMode {
    /// Time view (timing-focused): CALLS/s, TIME/s, MEAN, MAX, ROWS/s, DB, QID, PLAN
    #[default]
    Time,
    /// I/O view (buffer-focused): CALLS/s, BLK_RD/s, BLK_HIT/s, HIT%, BLK_WR/s, DB, PLAN
    Io,
    /// Regression view: plans with >1 planid per stmt_queryid, max/min mean ratio >2x
    Regression,
}

impl PgStorePlansViewMode {
    /// Default sort column index for this view mode.
    pub fn default_sort_column(&self) -> usize {
        match self {
            Self::Time => 1,       // TIME/s
            Self::Io => 1,         // BLK_RD/s
            Self::Regression => 2, // MEAN (ratio)
        }
    }

    /// Number of columns in this view mode.
    pub fn column_count(&self) -> usize {
        match self {
            Self::Time => 8,       // CALLS/s TIME/s MEAN MAX ROWS/s DB QID PLAN
            Self::Io => 7,         // CALLS/s BLK_RD/s BLK_HIT/s HIT% BLK_WR/s DB PLAN
            Self::Regression => 7, // CALLS/s MEAN MAX MIN RATIO DB PLAN
        }
    }
}

/// Rate metrics for a single `pg_store_plans` entry.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PgStorePlansRates {
    pub dt_secs: f64,
    pub calls_s: Option<f64>,
    pub rows_s: Option<f64>,
    /// Execution time rate in `ms/s`.
    pub exec_time_ms_s: Option<f64>,

    pub shared_blks_read_s: Option<f64>,
    pub shared_blks_hit_s: Option<f64>,
    pub shared_blks_dirtied_s: Option<f64>,
    pub shared_blks_written_s: Option<f64>,

    pub temp_blks_read_s: Option<f64>,
    pub temp_blks_written_s: Option<f64>,
}

/// Rate metrics for a single `pg_stat_user_tables` entry.
///
/// Rates are computed from deltas between two snapshots using `snapshot.timestamp`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PgTablesRates {
    pub dt_secs: f64,
    pub seq_scan_s: Option<f64>,
    pub seq_tup_read_s: Option<f64>,
    pub idx_scan_s: Option<f64>,
    pub idx_tup_fetch_s: Option<f64>,
    pub n_tup_ins_s: Option<f64>,
    pub n_tup_upd_s: Option<f64>,
    pub n_tup_del_s: Option<f64>,
    pub n_tup_hot_upd_s: Option<f64>,
    pub vacuum_count_s: Option<f64>,
    pub autovacuum_count_s: Option<f64>,

    // I/O rates (from pg_statio_user_tables)
    pub heap_blks_read_s: Option<f64>,
    pub heap_blks_hit_s: Option<f64>,
    pub idx_blks_read_s: Option<f64>,
    pub idx_blks_hit_s: Option<f64>,

    // TOAST I/O rates
    pub toast_blks_read_s: Option<f64>,
    pub toast_blks_hit_s: Option<f64>,
    pub tidx_blks_read_s: Option<f64>,
    pub tidx_blks_hit_s: Option<f64>,

    // Analyze rates
    pub analyze_count_s: Option<f64>,
    pub autoanalyze_count_s: Option<f64>,
}

/// Rate metrics for a single `pg_stat_user_indexes` entry.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PgIndexesRates {
    pub dt_secs: f64,
    pub idx_scan_s: Option<f64>,
    pub idx_tup_read_s: Option<f64>,
    pub idx_tup_fetch_s: Option<f64>,

    // I/O rates (from pg_statio_user_indexes)
    pub idx_blks_read_s: Option<f64>,
    pub idx_blks_hit_s: Option<f64>,
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
