//! UI-agnostic view model types.
//!
//! These types represent presentation data without any dependency on a specific
//! rendering framework (ratatui, HTML, etc). TUI maps them to ratatui Styles,
//! a future web frontend would map them to CSS classes.

/// Row-level style classification.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RowStyleClass {
    #[default]
    Normal,
    /// Warning level (TUI: yellow).
    Warning,
    /// Critical level (TUI: red).
    Critical,
    /// Critical + bold (TUI: red + bold). Used for anomalies.
    CriticalBold,
    /// Positive/active (TUI: green). E.g. "active" state.
    Active,
    /// Dimmed (TUI: dark gray). E.g. idle sessions.
    Dimmed,
    /// Accent (TUI: cyan). E.g. PostgreSQL-related CMD.
    Accent,
}

/// A single table cell with optional per-cell style override.
#[derive(Debug, Clone, Default)]
pub struct ViewCell {
    pub text: String,
    /// `None` = inherit row style.
    pub style: Option<RowStyleClass>,
}

impl ViewCell {
    pub fn plain(text: String) -> Self {
        Self { text, style: None }
    }

    pub fn styled(text: String, style: RowStyleClass) -> Self {
        Self {
            text,
            style: Some(style),
        }
    }
}

/// One table row, parameterized by entity ID type.
pub struct ViewRow<Id> {
    pub id: Id,
    pub cells: Vec<ViewCell>,
    pub style: RowStyleClass,
}

/// Complete table ready to be rendered by any frontend.
pub struct TableViewModel<Id> {
    pub title: String,
    pub headers: Vec<String>,
    pub widths: Vec<u16>,
    pub rows: Vec<ViewRow<Id>>,
    pub sort_column: usize,
    pub sort_ascending: bool,
}
