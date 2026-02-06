//! Utility modules for rpglot.

mod container;
mod time_parser;

pub use container::is_container;
pub use time_parser::{TimeParseError, parse_time, parse_time_with_base};
