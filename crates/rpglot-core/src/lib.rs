//! rpglot-core — shared library for rpglot ecosystem.
//!
//! Provides:
//! - `collector` — system and PostgreSQL metrics collection
//! - `storage` — data persistence, models, string interner
//! - `util` — helper utilities
//! - `fmt` — shared formatting helpers (bytes, duration, rate, etc.)
//! - `models` — shared data models (view modes, rates, rows)
//! - `table` — generic table state (sorting, selection tracking)
//!
//! With `provider` feature (default):
//! - `provider` — snapshot source abstraction (live, history)
//!
//! With `api` feature:
//! - `api` — JSON-serializable API types (snapshot, schema)
//!
//! With `tui` feature (default):
//! - `tui` — TUI rendering (ratatui/crossterm), state, input, widgets
//! - `view` — view models (depends on tui state)

pub mod collector;
pub mod fmt;
pub mod models;
pub mod storage;
pub mod table;
pub mod util;

#[cfg(feature = "provider")]
pub mod provider;

#[cfg(feature = "api")]
pub mod api;

#[cfg(feature = "tui")]
pub mod tui;

#[cfg(feature = "tui")]
pub mod view;
