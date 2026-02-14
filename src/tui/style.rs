//! Color scheme and styles (atop-style).

use ratatui::style::{Color, Modifier, Style};

/// Atop-style color palette.
pub struct Theme;

impl Theme {
    // Background colors
    pub const BG: Color = Color::Reset;
    pub const HEADER_BG: Color = Color::Blue;
    pub const SELECTED_BG: Color = Color::DarkGray;

    // Foreground colors
    pub const FG: Color = Color::White;
    pub const FG_DIM: Color = Color::DarkGray;
    pub const HEADER_FG: Color = Color::White;

    // Highlight colors
    pub const HIGHLIGHT_NEW: Color = Color::Green;
    pub const HIGHLIGHT_MODIFIED: Color = Color::Yellow;
    pub const HIGHLIGHT_CRITICAL: Color = Color::Red;

    // Tab colors
    pub const TAB_ACTIVE: Color = Color::Cyan;
    pub const TAB_INACTIVE: Color = Color::DarkGray;

    // Metrics colors
    pub const CPU_COLOR: Color = Color::Cyan;
    pub const MEM_COLOR: Color = Color::Magenta;
    pub const DISK_COLOR: Color = Color::Yellow;
    #[allow(dead_code)]
    pub const NET_COLOR: Color = Color::Green;

    // Sparkline colors
    #[allow(dead_code)]
    pub const SPARKLINE_COLOR: Color = Color::Cyan;
}

/// Pre-defined styles.
pub struct Styles;

impl Styles {
    /// Default text style.
    pub fn default() -> Style {
        Style::default().fg(Theme::FG).bg(Theme::BG)
    }

    /// Header bar style.
    pub fn header() -> Style {
        Style::default()
            .fg(Theme::HEADER_FG)
            .bg(Theme::HEADER_BG)
            .add_modifier(Modifier::BOLD)
    }

    /// Selected row style.
    pub fn selected() -> Style {
        Style::default()
            .bg(Theme::SELECTED_BG)
            .add_modifier(Modifier::BOLD)
    }

    /// Table header style.
    pub fn table_header() -> Style {
        Style::default()
            .fg(Theme::HEADER_FG)
            .bg(Theme::HEADER_BG)
            .add_modifier(Modifier::BOLD)
    }

    /// New item style (green).
    pub fn new_item() -> Style {
        Style::default().fg(Theme::HIGHLIGHT_NEW)
    }

    /// Modified item style (yellow).
    pub fn modified_item() -> Style {
        Style::default().fg(Theme::HIGHLIGHT_MODIFIED)
    }

    /// Critical value style (red).
    pub fn critical() -> Style {
        Style::default()
            .fg(Theme::HIGHLIGHT_CRITICAL)
            .add_modifier(Modifier::BOLD)
    }

    /// Active tab style.
    pub fn tab_active() -> Style {
        Style::default()
            .fg(Theme::TAB_ACTIVE)
            .add_modifier(Modifier::BOLD)
    }

    /// Inactive tab style.
    pub fn tab_inactive() -> Style {
        Style::default().fg(Theme::TAB_INACTIVE)
    }

    /// Dimmed text style.
    pub fn dim() -> Style {
        Style::default().fg(Theme::FG_DIM)
    }

    /// CPU metric style.
    pub fn cpu() -> Style {
        Style::default().fg(Theme::CPU_COLOR)
    }

    /// Memory metric style.
    pub fn mem() -> Style {
        Style::default().fg(Theme::MEM_COLOR)
    }

    /// Disk metric style.
    pub fn disk() -> Style {
        Style::default().fg(Theme::DISK_COLOR)
    }

    /// Network metric style.
    #[allow(dead_code)]
    pub fn net() -> Style {
        Style::default().fg(Theme::NET_COLOR)
    }

    /// Filter input style.
    pub fn filter_input() -> Style {
        Style::default()
            .fg(Theme::FG)
            .add_modifier(Modifier::UNDERLINED)
    }

    /// Section header style for detail popups.
    pub fn section_header() -> Style {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    }

    /// Help text style.
    pub fn help() -> Style {
        Style::default().fg(Theme::FG_DIM)
    }

    /// Help key style (highlighted keys in help line).
    pub fn help_key() -> Style {
        Style::default().fg(Theme::FG).add_modifier(Modifier::BOLD)
    }

    /// Maps a UI-agnostic [`RowStyleClass`] to a ratatui [`Style`].
    pub fn from_class(class: crate::view::common::RowStyleClass) -> Style {
        use crate::view::common::RowStyleClass;
        match class {
            RowStyleClass::Normal => Self::default(),
            RowStyleClass::Warning => Self::modified_item(),
            RowStyleClass::Critical => Self::critical(),
            RowStyleClass::CriticalBold => {
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
            }
            RowStyleClass::Active => Style::default().fg(Color::Green),
            RowStyleClass::Dimmed => Style::default().fg(Color::DarkGray),
            RowStyleClass::Accent => Style::default().fg(Color::Cyan),
        }
    }
}
