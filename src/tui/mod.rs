//! Terminal User Interface for rpglot viewer.
//!
//! This module provides an interactive TUI similar to atop/htop for viewing
//! system metrics in real-time or from historical data.

mod app;
mod event;
pub mod fmt;
mod input;
mod models;
mod render;
mod state;
mod style;
mod table;
mod widgets;

pub use app::App;
pub use state::{AppState, PopupState, Tab};
