//! Application state management.

// Re-export table and models types so existing `use super::state::*` paths keep working.
pub use super::models::*;
pub use super::table::*;

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
}

impl Tab {
    pub fn all() -> &'static [Tab] {
        &[Tab::Processes, Tab::PostgresActive, Tab::PgStatements]
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
        }
    }

    /// Returns the next tab.
    pub fn next(&self) -> Tab {
        match self {
            Tab::Processes => Tab::PostgresActive,
            Tab::PostgresActive => Tab::PgStatements,
            Tab::PgStatements => Tab::Processes,
        }
    }

    /// Returns the previous tab.
    pub fn prev(&self) -> Tab {
        match self {
            Tab::Processes => Tab::PgStatements,
            Tab::PostgresActive => Tab::Processes,
            Tab::PgStatements => Tab::PostgresActive,
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
#[derive(Debug, Clone, PartialEq)]
pub enum PopupState {
    /// No popup is open.
    None,
    /// Help popup with scroll offset.
    Help { scroll: usize },
    /// Quit confirmation dialog.
    QuitConfirm,
    /// Debug/timing popup (live mode only).
    Debug,
    /// Process detail popup (PRC tab).
    ProcessDetail { pid: u32, scroll: usize },
    /// PostgreSQL session detail popup (PGA tab).
    PgDetail { pid: i32, scroll: usize },
    /// pg_stat_statements detail popup (PGS tab).
    PgsDetail { queryid: i64, scroll: usize },
}

impl Default for PopupState {
    fn default() -> Self {
        Self::None
    }
}

impl PopupState {
    /// Returns true if any popup is open (excluding None).
    pub fn is_open(&self) -> bool {
        !matches!(self, Self::None)
    }

    /// Returns true if a detail popup (ProcessDetail, PgDetail, PgsDetail) is open.
    pub fn is_detail_open(&self) -> bool {
        matches!(
            self,
            Self::ProcessDetail { .. } | Self::PgDetail { .. } | Self::PgsDetail { .. }
        )
    }
}

