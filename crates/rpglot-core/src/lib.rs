//! rpglot-core — shared library for rpglot ecosystem.
//!
//! Provides:
//! - `collector` — system and PostgreSQL metrics collection
//! - `storage` — data persistence, models, string interner
//! - `util` — helper utilities
//!
//! With `provider` feature (default):
//! - `provider` — snapshot source abstraction (live, history)
//!
//! With `view` feature (default):
//! - `tui` — formatting, models, state, and TUI rendering
//! - `view` — UI-agnostic view models

pub mod collector;
pub mod storage;
pub mod util;

#[cfg(feature = "provider")]
pub mod provider;

#[cfg(feature = "view")]
pub mod tui;

#[cfg(feature = "view")]
pub mod view;
