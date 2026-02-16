//! Utility modules for rpglot.

mod container;
pub mod process_io;
mod time_parser;

pub use container::is_container;
pub use time_parser::{TimeParseError, parse_time, parse_time_with_base};

/// Prints a PostgreSQL connection warning to stderr with ANSI colors.
pub fn print_pg_warning(error: &str) {
    const RED: &str = "\x1b[1;31m";
    const YELLOW: &str = "\x1b[33m";
    const RESET: &str = "\x1b[0m";

    eprintln!("{RED}PostgreSQL: {error}{RESET}");
    eprintln!();
    eprintln!("{YELLOW}  Configure connection with environment variables:");
    eprintln!("    export PGHOST=localhost");
    eprintln!("    export PGPORT=5432");
    eprintln!("    export PGUSER=postgres");
    eprintln!("    export PGPASSWORD=secret");
    eprintln!("    export PGDATABASE=postgres");
    eprintln!();
    eprintln!("  PostgreSQL metrics will be disabled.{RESET}");
}
