//! Collectors for Linux `/proc` filesystem.
//!
//! This module provides parsers and collectors for reading system and process
//! information from the `/proc` virtual filesystem.

pub mod parser;
pub mod process;
pub mod system;

pub use parser::UserResolver;
pub use process::{CollectError, ProcessCollector};
pub use system::SystemCollector;
