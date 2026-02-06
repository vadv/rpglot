//! rpglot - Interactive TUI viewer for system metrics.
//!
//! Supports two modes:
//! - Live mode (default): collect and display metrics in real-time
//! - History mode: view recorded data from rpglotd
//!
//! Usage:
//!   rpglot              # live mode with 1 second interval
//!   rpglot 5            # live mode with 5 second interval
//!   rpglot -r           # history mode (default: /var/log/rpglot)
//!   rpglot -r ./data    # history mode with custom path
//!   rpglot -r -b -1h    # history mode starting from 1 hour ago
//!   rpglot -r -b 07:00  # history mode starting from today 07:00 UTC

use std::time::Duration;

use clap::Parser;

#[cfg(target_os = "linux")]
use rpglot::collector::RealFs;
#[cfg(not(target_os = "linux"))]
use rpglot::collector::mock::MockFs;
use rpglot::collector::{Collector, PostgresCollector};
use rpglot::provider::{HistoryProvider, LiveProvider, SnapshotProvider};
use rpglot::tui::App;
use rpglot::util::parse_time;

/// Default path for history data.
const DEFAULT_HISTORY_PATH: &str = "/var/log/rpglot";

/// Interactive TUI viewer for system metrics.
#[derive(Parser)]
#[command(name = "rpglot", about = "System metrics viewer")]
struct Args {
    /// Update interval in seconds (default: 1).
    /// Only used in live mode.
    #[arg(value_name = "INTERVAL")]
    interval: Option<u64>,

    /// Enable history mode. Optionally specify path to data directory.
    /// Default: /var/log/rpglot
    #[arg(short = 'r', long = "history", value_name = "PATH", num_args = 0..=1, default_missing_value = "")]
    history: Option<Option<String>>,

    /// Start time for history mode. Supported formats:
    /// - ISO 8601: 2026-02-07T17:00:00
    /// - Unix timestamp: 1738944000
    /// - Relative: -1h, -30m, -2d
    /// - Date:time: 2026-02-07:07:00
    /// - Time only (today): 07:00
    #[arg(short = 'b', long = "begin", value_name = "TIME")]
    begin: Option<String>,

    /// Path to /proc filesystem (for live mode).
    #[arg(long, default_value = "/proc")]
    proc_path: String,

    /// Path to cgroup filesystem (for live mode).
    /// Implies --force-cgroup if specified.
    #[arg(long, value_name = "PATH")]
    cgroup_path: Option<String>,

    /// Force cgroup collection even on bare metal.
    /// Useful for testing when container detection fails.
    #[arg(long)]
    force_cgroup: bool,
}

fn main() {
    let args = Args::parse();

    // Validate arguments
    if args.history.is_some() && args.interval.is_some() {
        eprintln!("Error: cannot specify interval in history mode");
        eprintln!("Usage: rpglot -r           # view historical data (default: /var/log/rpglot)");
        eprintln!("       rpglot -r ./data    # view historical data from custom path");
        eprintln!("       rpglot [INTERVAL]   # live monitoring (default: 1s)");
        std::process::exit(1);
    }

    if args.begin.is_some() && args.history.is_none() {
        eprintln!("Error: --begin/-b can only be used with history mode (-r)");
        std::process::exit(1);
    }

    // Parse begin time if provided
    let begin_timestamp = if let Some(ref time_str) = args.begin {
        match parse_time(time_str) {
            Ok(ts) => Some(ts),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        None
    };

    // Create provider based on mode
    let provider: Box<dyn SnapshotProvider> = if let Some(ref path_opt) = args.history {
        // History mode
        // Handle: -r (Some(Some(""))), -r path (Some(Some("path"))), no -r (None)
        let path = path_opt
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_HISTORY_PATH);

        let provider_result = if let Some(since) = begin_timestamp {
            HistoryProvider::from_path_since(path, since)
        } else {
            HistoryProvider::from_path(path)
        };

        match provider_result {
            Ok(p) => Box::new(p),
            Err(e) => {
                eprintln!("Error loading history from '{}': {}", path, e);
                std::process::exit(1);
            }
        }
    } else {
        // Live mode
        #[cfg(target_os = "linux")]
        let collector = {
            let fs = RealFs::new();
            let mut c = Collector::new(fs, &args.proc_path);
            // Enable PostgreSQL collection if PGUSER is set
            // In live mode, disable pg_stat_statements caching for real-time data
            if let Ok(pg_collector) = PostgresCollector::from_env() {
                c = c.with_postgres(pg_collector.with_statements_interval(Duration::ZERO));
            }
            // Handle cgroup options
            if let Some(ref cgroup_path) = args.cgroup_path {
                c = c.with_cgroup(cgroup_path);
            } else if args.force_cgroup {
                c = c.force_cgroup(None);
            }
            c
        };
        #[cfg(not(target_os = "linux"))]
        let collector = {
            let fs = MockFs::typical_system();
            let mut c = Collector::new(fs, &args.proc_path);
            // Enable PostgreSQL collection if PGUSER is set
            // In live mode, disable pg_stat_statements caching for real-time data
            if let Ok(pg_collector) = PostgresCollector::from_env() {
                c = c.with_postgres(pg_collector.with_statements_interval(Duration::ZERO));
            }
            // Handle cgroup options
            if let Some(ref cgroup_path) = args.cgroup_path {
                c = c.with_cgroup(cgroup_path);
            } else if args.force_cgroup {
                c = c.force_cgroup(None);
            }
            c
        };
        Box::new(LiveProvider::new(collector, None))
    };

    // Create and run TUI
    let tick_rate = Duration::from_secs(args.interval.unwrap_or(1));
    let app = App::new(provider);

    if let Err(e) = app.run(tick_rate) {
        eprintln!("Error running TUI: {}", e);
        std::process::exit(1);
    }
}
