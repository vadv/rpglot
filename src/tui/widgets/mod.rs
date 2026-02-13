//! TUI widgets for rpglot.

mod debug_popup;
pub mod detail_common;
mod header;
mod help;
mod pg_indexes;
mod pg_statements;
mod pg_tables;
mod pga_detail;
mod pgi_detail;
mod pgs_detail;
mod pgt_detail;
mod postgres;
mod process_detail;
pub mod processes;
mod quit_confirm;
pub mod summary;
mod time_jump;

pub use debug_popup::render_debug_popup;
pub use header::render_header;
pub use help::render_help;
pub use pg_indexes::render_pg_indexes;
pub use pg_statements::render_pg_statements;
pub use pg_tables::render_pg_tables;
pub use pga_detail::render_pg_detail;
pub use pgi_detail::render_pgi_detail;
pub use pgs_detail::render_pgs_detail;
pub use pgt_detail::render_pgt_detail;
pub use postgres::render_postgres;
pub use process_detail::render_process_detail;
pub use processes::render_processes;
pub use quit_confirm::render_quit_confirm;
pub use summary::{calculate_summary_height, render_summary};
pub use time_jump::render_time_jump;
