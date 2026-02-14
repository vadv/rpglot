//! Application state management.

pub use crate::models::*;
pub use crate::table::*;

mod app_state;
mod tab_states;

pub use app_state::*;
pub use tab_states::*;

/// Available tabs in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Tab {
    #[default]
    Processes,
    PostgresActive,
    PgStatements,
    PgTables,
    PgIndexes,
    PgLocks,
}

impl Tab {
    pub fn all() -> &'static [Tab] {
        &[
            Tab::Processes,
            Tab::PostgresActive,
            Tab::PgStatements,
            Tab::PgTables,
            Tab::PgIndexes,
            Tab::PgLocks,
        ]
    }
}

/// Per-tab state for filter, sort, and selection.
#[derive(Debug, Clone, Default)]
pub struct TabState {
    /// Filter string.
    pub filter: Option<String>,
    /// Sort column index.
    pub sort_column: usize,
    /// Sort direction (true = ascending).
    pub sort_ascending: bool,
    /// Selected row index.
    pub selected: usize,
}

impl Tab {
    /// Returns the display name of the tab.
    pub fn name(&self) -> &'static str {
        match self {
            Tab::Processes => "PRC",
            Tab::PostgresActive => "PGA",
            Tab::PgStatements => "PGS",
            Tab::PgTables => "PGT",
            Tab::PgIndexes => "PGI",
            Tab::PgLocks => "PGL",
        }
    }

    /// Returns the next tab.
    pub fn next(&self) -> Tab {
        match self {
            Tab::Processes => Tab::PostgresActive,
            Tab::PostgresActive => Tab::PgStatements,
            Tab::PgStatements => Tab::PgTables,
            Tab::PgTables => Tab::PgIndexes,
            Tab::PgIndexes => Tab::PgLocks,
            Tab::PgLocks => Tab::Processes,
        }
    }

    /// Returns the previous tab.
    pub fn prev(&self) -> Tab {
        match self {
            Tab::Processes => Tab::PgLocks,
            Tab::PostgresActive => Tab::Processes,
            Tab::PgStatements => Tab::PostgresActive,
            Tab::PgTables => Tab::PgStatements,
            Tab::PgIndexes => Tab::PgTables,
            Tab::PgLocks => Tab::PgIndexes,
        }
    }
}

/// Input mode for the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Normal,
    Filter,
    TimeJump,
}

/// Active popup state. Only one popup can be open at a time.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum PopupState {
    /// No popup is open.
    #[default]
    None,
    /// Help popup with scroll offset.
    Help { scroll: usize },
    /// Quit confirmation dialog.
    QuitConfirm,
    /// Debug/timing popup (live mode only).
    Debug,
    /// Process detail popup (PRC tab).
    ProcessDetail {
        pid: u32,
        scroll: usize,
        show_help: bool,
    },
    /// PostgreSQL session detail popup (PGA tab).
    PgDetail {
        pid: i32,
        scroll: usize,
        show_help: bool,
    },
    /// pg_stat_statements detail popup (PGS tab).
    PgsDetail {
        queryid: i64,
        scroll: usize,
        show_help: bool,
    },
    /// pg_stat_user_tables detail popup (PGT tab).
    PgtDetail {
        relid: u32,
        scroll: usize,
        show_help: bool,
    },
    /// pg_stat_user_indexes detail popup (PGI tab).
    PgiDetail {
        indexrelid: u32,
        scroll: usize,
        show_help: bool,
    },
    /// pg_locks tree detail popup (PGL tab).
    PglDetail {
        pid: i32,
        scroll: usize,
        show_help: bool,
    },
}

impl PopupState {
    /// Returns true if any popup is open (excluding None).
    pub fn is_open(&self) -> bool {
        !matches!(self, Self::None)
    }

    /// Returns true if a detail popup (ProcessDetail, PgDetail, PgsDetail, PgtDetail, PgiDetail) is open.
    pub fn is_detail_open(&self) -> bool {
        matches!(
            self,
            Self::ProcessDetail { .. }
                | Self::PgDetail { .. }
                | Self::PgsDetail { .. }
                | Self::PgtDetail { .. }
                | Self::PgiDetail { .. }
                | Self::PglDetail { .. }
        )
    }
}
