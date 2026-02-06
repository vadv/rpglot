//! Terminal User Interface for rpglot viewer.
//!
//! This module provides an interactive TUI similar to atop/htop for viewing
//! system metrics in real-time or from historical data.

mod app;
mod event;
mod input;
mod render;
mod state;
mod style;
mod widgets;

pub use app::App;
pub use state::{AppState, Tab};
