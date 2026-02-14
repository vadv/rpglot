//! UI-agnostic view models.
//!
//! Each sub-module builds a [`common::TableViewModel`] from snapshot data and tab state.
//! The TUI (or a future web frontend) then maps the view model to framework-specific
//! widgets for rendering.

pub mod activity;
pub mod common;
pub mod indexes;
pub mod locks;
pub mod statements;
pub mod tables;

// PRC (processes) is not included here because `ProcessRow` already provides
// a UI-agnostic API via `cells_for_mode()`, `headers_for_mode()`, etc.
// The TUI-specific concerns (DiffStatus per-column highlighting, adaptive widths,
// horizontal scroll) make a separate ViewModel layer unnecessary.
