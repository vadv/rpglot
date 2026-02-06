//! TUI widgets for rpglot.

mod debug_popup;
mod header;
mod help;
mod pg_detail;
mod pg_statements;
mod pg_statements_detail;
mod postgres;
mod process_detail;
pub mod processes;
mod quit_confirm;
pub mod summary;
mod time_jump;

pub use debug_popup::render_debug_popup;
pub use header::render_header;
pub use help::render_help;
pub use pg_detail::render_pg_detail;
pub use pg_statements::render_pg_statements;
pub use pg_statements_detail::render_pgs_detail;
pub use postgres::render_postgres;
pub use process_detail::render_process_detail;
pub use processes::render_processes;
pub use quit_confirm::render_quit_confirm;
pub use summary::{calculate_summary_height, render_summary};
pub use time_jump::render_time_jump;
