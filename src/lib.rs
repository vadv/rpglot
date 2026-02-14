//! rpglot - System monitoring and PostgreSQL agent library.
//!
//! This library provides the core functionality shared between:
//! - `rpglotd` - background daemon for collecting metrics
//! - `rpglot` - interactive TUI viewer

pub mod collector;
pub mod provider;
pub mod storage;
pub mod tui;
pub mod util;
pub mod view;
