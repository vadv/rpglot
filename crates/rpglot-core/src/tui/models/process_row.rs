//! ProcessRow struct, view-mode specific columns, and TableRow impl.

use super::ProcessViewMode;
use super::formatting::*;
use crate::tui::table::{ColumnType, SortKey, TableRow};

/// Process row for the process table.
/// Contains all fields needed for different view modes (Generic/Command/Memory).
#[derive(Debug, Clone, Default)]
pub struct ProcessRow {
    // Identity
    pub pid: u32,
    pub tid: u32,
    pub name: String,
    pub cmdline: String,

    // CPU metrics
    pub syscpu: u64, // stime - kernel time (ticks)
    pub usrcpu: u64, // utime - user time (ticks)
    pub cpu_percent: f64,
    pub rdelay: u64, // rundelay - time waiting for CPU (ns)
    pub cpunr: i32,  // current CPU number

    // Memory metrics (all in KB)
    pub minflt: u64,
    pub majflt: u64,
    pub vstext: u64,      // vexec - executable code
    pub vslibs: u64,      // shared libraries
    pub vdata: u64,       // data segment
    pub vstack: u64,      // stack
    pub vlock: u64,       // locked memory (LOCKSZ)
    pub vsize: u64,       // total virtual memory
    pub rsize: u64,       // resident memory
    pub psize: u64,       // proportional set size
    pub vswap: u64,       // swap usage (SWAPSZ)
    pub mem_percent: f64, // MEM%

    // Deltas (require previous snapshot)
    pub vgrow: i64, // delta vsize
    pub rgrow: i64, // delta rsize

    // User identification
    pub ruid: u32,
    pub euid: u32,
    pub ruser: String, // Resolved username from ruid
    pub euser: String, // Resolved username from euid

    // State
    pub state: String,    // S (R/S/D/Z/T)
    pub exit_code: i32,   // EXC
    pub num_threads: u32, // THR
    pub btime: u32,       // Start time (unix timestamp)

    // PostgreSQL integration
    pub query: Option<String>, // Query from pg_stat_activity if PID matches
    pub backend_type: Option<String>, // Backend type from pg_stat_activity if PID matches

    // Disk I/O metrics (rates in bytes per second)
    pub rddsk: i64,       // Read bytes/s (delta from rsz)
    pub wrdsk: i64,       // Write bytes/s (delta from wsz)
    pub wcancl: i64,      // Cancelled write bytes/s (delta from cwsz)
    pub dsk_percent: f64, // % of total system disk I/O
}

impl ProcessRow {
    /// Returns headers for Generic view mode.
    pub fn headers_generic() -> Vec<&'static str> {
        vec![
            "PID", "SYSCPU", "USRCPU", "RDELAY", "VGROW", "RGROW", "RUID", "EUID", "ST", "EXC",
            "THR", "S", "CPUNR", "CPU", "CMD",
        ]
    }

    /// Returns headers for Command view mode.
    pub fn headers_command() -> Vec<&'static str> {
        vec!["PID", "TID", "S", "CPU", "MEM", "COMMAND-LINE"]
    }

    /// Returns headers for Memory view mode.
    pub fn headers_memory() -> Vec<&'static str> {
        vec![
            "PID", "TID", "MINFLT", "MAJFLT", "VSTEXT", "VSLIBS", "VDATA", "VSTACK", "LOCKSZ",
            "VSIZE", "RSIZE", "PSIZE", "VGROW", "RGROW", "SWAPSZ", "RUID", "EUID", "MEM", "CMD",
        ]
    }

    /// Returns column widths for Generic view mode.
    pub fn widths_generic() -> Vec<u16> {
        vec![8, 8, 8, 8, 8, 8, 6, 6, 3, 4, 4, 2, 5, 5, 20]
    }

    /// Returns column widths for Command view mode.
    pub fn widths_command() -> Vec<u16> {
        vec![8, 8, 2, 6, 8, 60]
    }

    /// Returns column widths for Memory view mode.
    pub fn widths_memory() -> Vec<u16> {
        vec![8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 6, 6, 5, 20]
    }

    /// Returns headers for Disk view mode.
    pub fn headers_disk() -> Vec<&'static str> {
        vec!["PID", "RDDSK", "WRDSK", "WCANCL", "DSK", "CMD"]
    }

    /// Returns column widths for Disk view mode.
    pub fn widths_disk() -> Vec<u16> {
        vec![8, 10, 10, 10, 6, 20]
    }

    /// Returns cells for Generic view mode.
    pub fn cells_generic(&self) -> Vec<String> {
        vec![
            self.pid.to_string(),
            format_ticks(self.syscpu),
            format_ticks(self.usrcpu),
            format_delay(self.rdelay),
            format_size_delta(self.vgrow),
            format_size_delta(self.rgrow),
            self.ruser.clone(),
            self.euser.clone(),
            format_start_time(self.btime),
            self.exit_code.to_string(),
            self.num_threads.to_string(),
            self.state.clone(),
            self.cpunr.to_string(),
            format!("{:.1}%", self.cpu_percent),
            self.format_cmd_with_query(),
        ]
    }

    /// Returns cells for Command view mode.
    pub fn cells_command(&self) -> Vec<String> {
        vec![
            self.pid.to_string(),
            self.tid.to_string(),
            self.state.clone(),
            format!("{:.1}%", self.cpu_percent),
            format_memory(self.rsize),
            self.format_cmdline_with_query(),
        ]
    }

    /// Returns cells for Memory view mode.
    pub fn cells_memory(&self) -> Vec<String> {
        vec![
            self.pid.to_string(),
            self.tid.to_string(),
            self.minflt.to_string(),
            self.majflt.to_string(),
            format_memory(self.vstext),
            format_memory(self.vslibs),
            format_memory(self.vdata),
            format_memory(self.vstack),
            format_memory(self.vlock),
            format_memory(self.vsize),
            format_memory(self.rsize),
            format_memory(self.psize),
            format_size_delta(self.vgrow),
            format_size_delta(self.rgrow),
            format_memory(self.vswap),
            self.ruser.clone(),
            self.euser.clone(),
            format!("{:.1}%", self.mem_percent),
            self.format_cmd_with_query(),
        ]
    }

    /// Returns cells for Disk view mode.
    pub fn cells_disk(&self) -> Vec<String> {
        vec![
            self.pid.to_string(),
            format_bytes_rate(self.rddsk),
            format_bytes_rate(self.wrdsk),
            format_bytes_rate(self.wcancl),
            format!("{:.1}%", self.dsk_percent),
            self.format_cmd_with_query(),
        ]
    }

    /// Formats CMD column with optional query or backend_type from pg_stat_activity.
    /// Returns "name [query]" if query is present and non-empty,
    /// "name [backend_type]" if only backend_type is present,
    /// otherwise just "name".
    fn format_cmd_with_query(&self) -> String {
        match &self.query {
            Some(q) if !q.is_empty() => format!("{} [{}]", self.name, q),
            _ => match &self.backend_type {
                Some(bt) if !bt.is_empty() => format!("{} [{}]", self.name, bt),
                _ => self.name.clone(),
            },
        }
    }

    /// Formats COMMAND-LINE column with optional query or backend_type from pg_stat_activity.
    /// Returns "cmdline [query]" if query is present and non-empty,
    /// "cmdline [backend_type]" if only backend_type is present,
    /// otherwise just cmdline (or name if cmdline is empty).
    fn format_cmdline_with_query(&self) -> String {
        let base = if self.cmdline.is_empty() {
            &self.name
        } else {
            &self.cmdline
        };
        match &self.query {
            Some(q) if !q.is_empty() => format!("{} [{}]", base, q),
            _ => match &self.backend_type {
                Some(bt) if !bt.is_empty() => format!("{} [{}]", base, bt),
                _ => base.clone(),
            },
        }
    }

    /// Returns sort key for the specified column and view mode.
    pub fn sort_key_for_mode(&self, column: usize, mode: ProcessViewMode) -> SortKey {
        match mode {
            ProcessViewMode::Generic => {
                // PID SYSCPU USRCPU RDELAY VGROW RGROW RUID EUID ST EXC THR S CPUNR CPU CMD
                match column {
                    0 => SortKey::Integer(self.pid as i64),
                    1 => SortKey::Integer(self.syscpu as i64),
                    2 => SortKey::Integer(self.usrcpu as i64),
                    3 => SortKey::Integer(self.rdelay as i64),
                    4 => SortKey::Integer(self.vgrow),
                    5 => SortKey::Integer(self.rgrow),
                    6 => SortKey::Integer(self.ruid as i64),
                    7 => SortKey::Integer(self.euid as i64),
                    8 => SortKey::String(String::new()), // ST placeholder
                    9 => SortKey::Integer(self.exit_code as i64),
                    10 => SortKey::Integer(self.num_threads as i64),
                    11 => SortKey::String(self.state.clone()),
                    12 => SortKey::Integer(self.cpunr as i64),
                    13 => SortKey::Float(self.cpu_percent),
                    14 => SortKey::String(self.name.clone()),
                    _ => SortKey::Integer(0),
                }
            }
            ProcessViewMode::Command => {
                // PID TID S CPU MEM COMMAND-LINE
                match column {
                    0 => SortKey::Integer(self.pid as i64),
                    1 => SortKey::Integer(self.tid as i64),
                    2 => SortKey::String(self.state.clone()),
                    3 => SortKey::Float(self.cpu_percent),
                    4 => SortKey::Integer(self.rsize as i64), // MEM = rsize
                    5 => SortKey::String(if self.cmdline.is_empty() {
                        self.name.clone()
                    } else {
                        self.cmdline.clone()
                    }),
                    _ => SortKey::Integer(0),
                }
            }
            ProcessViewMode::Memory => {
                // PID TID MINFLT MAJFLT VSTEXT VSLIBS VDATA VSTACK LOCKSZ VSIZE RSIZE PSIZE VGROW RGROW SWAPSZ RUID EUID MEM CMD
                match column {
                    0 => SortKey::Integer(self.pid as i64),
                    1 => SortKey::Integer(self.tid as i64),
                    2 => SortKey::Integer(self.minflt as i64),
                    3 => SortKey::Integer(self.majflt as i64),
                    4 => SortKey::Integer(self.vstext as i64),
                    5 => SortKey::Integer(self.vslibs as i64),
                    6 => SortKey::Integer(self.vdata as i64),
                    7 => SortKey::Integer(self.vstack as i64),
                    8 => SortKey::Integer(self.vlock as i64),
                    9 => SortKey::Integer(self.vsize as i64),
                    10 => SortKey::Integer(self.rsize as i64),
                    11 => SortKey::Integer(self.psize as i64),
                    12 => SortKey::Integer(self.vgrow),
                    13 => SortKey::Integer(self.rgrow),
                    14 => SortKey::Integer(self.vswap as i64),
                    15 => SortKey::Integer(self.ruid as i64),
                    16 => SortKey::Integer(self.euid as i64),
                    17 => SortKey::Float(self.mem_percent),
                    18 => SortKey::String(self.name.clone()),
                    _ => SortKey::Integer(0),
                }
            }
            ProcessViewMode::Disk => {
                // PID RDDSK WRDSK WCANCL DSK CMD
                match column {
                    0 => SortKey::Integer(self.pid as i64),
                    1 => SortKey::Integer(self.rddsk),
                    2 => SortKey::Integer(self.wrdsk),
                    3 => SortKey::Integer(self.wcancl),
                    4 => SortKey::Float(self.dsk_percent),
                    5 => SortKey::String(self.name.clone()),
                    _ => SortKey::Integer(0),
                }
            }
        }
    }

    /// Returns headers for the specified view mode.
    pub fn headers_for_mode(mode: ProcessViewMode) -> Vec<&'static str> {
        match mode {
            ProcessViewMode::Generic => Self::headers_generic(),
            ProcessViewMode::Command => Self::headers_command(),
            ProcessViewMode::Memory => Self::headers_memory(),
            ProcessViewMode::Disk => Self::headers_disk(),
        }
    }

    /// Returns cells for the specified view mode.
    pub fn cells_for_mode(&self, mode: ProcessViewMode) -> Vec<String> {
        match mode {
            ProcessViewMode::Generic => self.cells_generic(),
            ProcessViewMode::Command => self.cells_command(),
            ProcessViewMode::Memory => self.cells_memory(),
            ProcessViewMode::Disk => self.cells_disk(),
        }
    }

    /// Returns column widths for the specified view mode.
    pub fn widths_for_mode(mode: ProcessViewMode) -> Vec<u16> {
        match mode {
            ProcessViewMode::Generic => Self::widths_generic(),
            ProcessViewMode::Command => Self::widths_command(),
            ProcessViewMode::Memory => Self::widths_memory(),
            ProcessViewMode::Disk => Self::widths_disk(),
        }
    }

    /// Returns minimum column widths for Generic view mode.
    pub fn min_widths_generic() -> Vec<u16> {
        // PID SYSCPU USRCPU RDELAY VGROW RGROW RUID EUID ST EXC THR S CPUNR CPU CMD
        vec![5, 7, 7, 7, 7, 7, 4, 4, 2, 3, 3, 1, 3, 6, 8]
    }

    /// Returns minimum column widths for Command view mode.
    pub fn min_widths_command() -> Vec<u16> {
        // PID TID S CPU MEM COMMAND-LINE
        vec![5, 5, 1, 4, 4, 10]
    }

    /// Returns minimum column widths for Memory view mode.
    pub fn min_widths_memory() -> Vec<u16> {
        // PID TID MINFLT MAJFLT VSTEXT VSLIBS VDATA VSTACK LOCKSZ VSIZE RSIZE PSIZE VGROW RGROW SWAPSZ RUID EUID MEM CMD
        vec![5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 4, 4, 4, 8]
    }

    /// Returns minimum column widths for Disk view mode.
    pub fn min_widths_disk() -> Vec<u16> {
        // PID RDDSK WRDSK WCANCL DSK CMD
        vec![5, 6, 6, 6, 4, 8]
    }

    /// Returns minimum column widths for the specified view mode.
    pub fn min_widths_for_mode(mode: ProcessViewMode) -> Vec<u16> {
        match mode {
            ProcessViewMode::Generic => Self::min_widths_generic(),
            ProcessViewMode::Command => Self::min_widths_command(),
            ProcessViewMode::Memory => Self::min_widths_memory(),
            ProcessViewMode::Disk => Self::min_widths_disk(),
        }
    }

    /// Returns column types for Generic view mode.
    pub fn column_types_generic() -> Vec<ColumnType> {
        use ColumnType::*;
        // PID SYSCPU USRCPU RDELAY VGROW RGROW RUID EUID ST EXC THR S CPUNR CPU CMD
        vec![
            Fixed,      // PID
            Flexible,   // SYSCPU
            Flexible,   // USRCPU
            Flexible,   // RDELAY
            Flexible,   // VGROW
            Flexible,   // RGROW
            Flexible,   // RUID
            Flexible,   // EUID
            Fixed,      // ST
            Fixed,      // EXC
            Fixed,      // THR
            Fixed,      // S
            Fixed,      // CPUNR
            Flexible,   // CPU
            Expandable, // CMD
        ]
    }

    /// Returns column types for Command view mode.
    pub fn column_types_command() -> Vec<ColumnType> {
        use ColumnType::*;
        // PID TID S CPU MEM COMMAND-LINE
        vec![
            Fixed,      // PID
            Fixed,      // TID
            Fixed,      // S
            Flexible,   // CPU
            Flexible,   // MEM
            Expandable, // COMMAND-LINE
        ]
    }

    /// Returns column types for Memory view mode.
    pub fn column_types_memory() -> Vec<ColumnType> {
        use ColumnType::*;
        // PID TID MINFLT MAJFLT VSTEXT VSLIBS VDATA VSTACK LOCKSZ VSIZE RSIZE PSIZE VGROW RGROW SWAPSZ RUID EUID MEM CMD
        vec![
            Fixed,      // PID
            Fixed,      // TID
            Flexible,   // MINFLT
            Flexible,   // MAJFLT
            Flexible,   // VSTEXT
            Flexible,   // VSLIBS
            Flexible,   // VDATA
            Flexible,   // VSTACK
            Flexible,   // LOCKSZ
            Flexible,   // VSIZE
            Flexible,   // RSIZE
            Flexible,   // PSIZE
            Flexible,   // VGROW
            Flexible,   // RGROW
            Flexible,   // SWAPSZ
            Flexible,   // RUID
            Flexible,   // EUID
            Flexible,   // MEM
            Expandable, // CMD
        ]
    }

    /// Returns column types for Disk view mode.
    pub fn column_types_disk() -> Vec<ColumnType> {
        use ColumnType::*;
        // PID RDDSK WRDSK WCANCL DSK CMD
        vec![
            Fixed,      // PID
            Flexible,   // RDDSK
            Flexible,   // WRDSK
            Flexible,   // WCANCL
            Flexible,   // DSK
            Expandable, // CMD
        ]
    }

    /// Returns column types for the specified view mode.
    pub fn column_types_for_mode(mode: ProcessViewMode) -> Vec<ColumnType> {
        match mode {
            ProcessViewMode::Generic => Self::column_types_generic(),
            ProcessViewMode::Command => Self::column_types_command(),
            ProcessViewMode::Memory => Self::column_types_memory(),
            ProcessViewMode::Disk => Self::column_types_disk(),
        }
    }
}

impl TableRow for ProcessRow {
    fn id(&self) -> u64 {
        self.pid as u64
    }

    fn column_count() -> usize {
        // Default to Generic view column count
        15
    }

    fn headers() -> Vec<&'static str> {
        // Default to Generic view
        Self::headers_generic()
    }

    fn cells(&self) -> Vec<String> {
        // Default to Generic view
        self.cells_generic()
    }

    fn sort_key(&self, column: usize) -> SortKey {
        // Generic view columns for sorting
        match column {
            0 => SortKey::Integer(self.pid as i64),
            1 => SortKey::Integer(self.syscpu as i64),
            2 => SortKey::Integer(self.usrcpu as i64),
            3 => SortKey::Integer(self.rdelay as i64),
            4 => SortKey::Integer(self.vgrow),
            5 => SortKey::Integer(self.rgrow),
            6 => SortKey::Integer(self.ruid as i64),
            7 => SortKey::Integer(self.euid as i64),
            10 => SortKey::Integer(self.num_threads as i64),
            11 => SortKey::String(self.state.clone()),
            12 => SortKey::Integer(self.cpunr as i64),
            13 => SortKey::Float(self.cpu_percent),
            14 => SortKey::String(self.name.clone()),
            _ => SortKey::Integer(0),
        }
    }

    fn matches_filter(&self, filter: &str) -> bool {
        let filter_lower = filter.to_lowercase();
        self.name.to_lowercase().contains(&filter_lower)
            || self.cmdline.to_lowercase().contains(&filter_lower)
            || self.pid.to_string().contains(&filter_lower)
    }
}

#[cfg(test)]
mod filter_tests {
    use super::*;
    use crate::tui::table::TableState;

    #[test]
    fn test_process_filter_substring() {
        let row = ProcessRow {
            name: "rpglot".to_string(),
            cmdline: "/usr/bin/rpglot --proc-path ./mock_proc".to_string(),
            pid: 12345,
            ..Default::default()
        };

        // Exact match
        assert!(
            row.matches_filter("rpglot"),
            "Should match exact name 'rpglot'"
        );

        // Substring matches
        assert!(row.matches_filter("rpg"), "Should match substring 'rpg'");
        assert!(row.matches_filter("lot"), "Should match substring 'lot'");
        assert!(row.matches_filter("glot"), "Should match substring 'glot'");

        // Case insensitive
        assert!(
            row.matches_filter("RPG"),
            "Should match case-insensitive 'RPG'"
        );
        assert!(
            row.matches_filter("RPGLOT"),
            "Should match case-insensitive 'RPGLOT'"
        );
    }

    #[test]
    fn test_table_state_filtered_items_substring() {
        let mut table: TableState<ProcessRow> = TableState::new();

        let rows = vec![
            ProcessRow {
                name: "rpglot".to_string(),
                cmdline: "/usr/bin/rpglot".to_string(),
                pid: 1001,
                ..Default::default()
            },
            ProcessRow {
                name: "bash".to_string(),
                cmdline: "/bin/bash".to_string(),
                pid: 1002,
                ..Default::default()
            },
            ProcessRow {
                name: "systemd".to_string(),
                cmdline: "/lib/systemd/systemd".to_string(),
                pid: 1,
                ..Default::default()
            },
        ];

        table.update(rows);

        // No filter - all items
        assert_eq!(table.filtered_items().len(), 3);

        // Exact filter
        table.set_filter(Some("rpglot".to_string()));
        assert_eq!(
            table.filtered_items().len(),
            1,
            "Should find rpglot with exact filter"
        );

        // Substring filter - THIS IS THE BUG CASE
        table.set_filter(Some("rpg".to_string()));
        let filtered = table.filtered_items();
        assert_eq!(
            filtered.len(),
            1,
            "Should find rpglot with substring filter 'rpg'"
        );
        assert_eq!(filtered[0].name, "rpglot");

        // Another substring
        table.set_filter(Some("sys".to_string()));
        assert_eq!(
            table.filtered_items().len(),
            1,
            "Should find systemd with substring filter 'sys'"
        );
    }
}
