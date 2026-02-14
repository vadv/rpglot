//! rpglotd - System metrics collector daemon.
//!
//! Collects system metrics from /proc filesystem and stores them to disk.
//! Supports hourly file segmentation and automatic rotation by size and age.

use tikv_jemallocator::Jemalloc;
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

/// Releases unused memory back to the operating system.
/// Uses jemalloc's arena purge to reduce RSS after memory-intensive operations.
fn release_memory_to_os() {
    // SAFETY: We're calling jemalloc's mallctl with valid arguments.
    // arena.0.purge tells jemalloc to return unused pages to the OS.
    unsafe {
        // Purge all arenas (not just arena 0) for more aggressive memory release
        tikv_jemalloc_sys::mallctl(
            c"arena.0.purge".as_ptr().cast(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        );
    }
}

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use chrono::{Timelike, Utc};
use clap::Parser;
use tracing::{Level, debug, error, info, warn};
use tracing_subscriber::EnvFilter;

#[cfg(target_os = "linux")]
use rpglot_core::collector::RealFs;
#[cfg(not(target_os = "linux"))]
use rpglot_core::collector::mock::MockFs;
use rpglot_core::collector::{Collector, PostgresCollector};
use rpglot_core::storage::model::DataBlock;
use rpglot_core::storage::{RotationConfig, StorageManager};
use rpglot_core::util::is_container;

/// System metrics collector daemon.
#[derive(Parser)]
#[command(name = "rpglotd", about = "System metrics collector daemon", version)]
struct Args {
    /// Collection interval in seconds.
    #[arg(short, long, default_value = "10")]
    interval: u64,

    /// Output directory for storing metrics.
    #[arg(short, long, default_value = "./data")]
    output_dir: String,

    /// Path to /proc filesystem (for testing/mocking).
    #[arg(long, default_value = "/proc")]
    proc_path: String,

    /// Maximum total size of data files (e.g., "1G", "500M", "1073741824").
    /// When exceeded, oldest files are removed.
    #[arg(long, default_value = "1G", value_parser = parse_size)]
    max_size: u64,

    /// Maximum retention period in days. Files older than this are removed.
    #[arg(long, default_value = "7")]
    max_days: u32,

    /// Enable PostgreSQL metrics collection.
    /// Uses PGUSER or $USER for connection. Disable with --postgres=false.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    postgres: bool,

    /// Increase logging verbosity (-v for debug, -vv for trace). Default is info level.
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Quiet mode - only show errors.
    #[arg(short, long)]
    quiet: bool,

    /// Path to cgroup filesystem.
    /// Implies --force-cgroup if specified.
    #[arg(long, value_name = "PATH")]
    cgroup_path: Option<String>,

    /// Force cgroup collection even on bare metal.
    /// Useful for testing when container detection fails.
    #[arg(long)]
    force_cgroup: bool,
}

/// Parses a human-readable size string (e.g., "1G", "500M", "1024K") into bytes.
fn parse_size(s: &str) -> Result<u64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty size string".to_string());
    }

    let (num_str, multiplier) = if let Some(num) = s.strip_suffix('G') {
        (num, 1024 * 1024 * 1024)
    } else if let Some(num) = s.strip_suffix('M') {
        (num, 1024 * 1024)
    } else if let Some(num) = s.strip_suffix('K') {
        (num, 1024)
    } else {
        (s, 1)
    };

    num_str
        .trim()
        .parse::<u64>()
        .map(|n| n * multiplier)
        .map_err(|e| format!("invalid size '{}': {}", s, e))
}

/// Formats bytes as human-readable size string.
fn format_size(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;
    const KB: u64 = 1024;

    if bytes >= GB {
        format!("{:.1}G", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}M", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}K", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
}

/// Initializes the tracing subscriber with the appropriate log level.
/// Default level is INFO (equivalent to -v). Use -q for quiet mode (errors only).
fn init_logging(verbose: u8, quiet: bool) {
    let level = if quiet {
        Level::ERROR
    } else {
        match verbose {
            0 => Level::INFO, // Default is INFO (verbose)
            1 => Level::DEBUG,
            _ => Level::TRACE,
        }
    };

    let filter = EnvFilter::from_default_env()
        .add_directive(format!("rpglotd={}", level).parse().unwrap())
        .add_directive(format!("rpglot={}", level).parse().unwrap());

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

/// Describes the contents of a snapshot for logging.
fn describe_snapshot(snapshot: &rpglot_core::storage::Snapshot) -> String {
    let mut parts: Vec<String> = Vec::new();

    for block in &snapshot.blocks {
        match block {
            DataBlock::Processes(p) => parts.push(format!("{} processes", p.len())),
            DataBlock::SystemDisk(d) => parts.push(format!("{} disks", d.len())),
            DataBlock::SystemNet(n) => parts.push(format!("{} interfaces", n.len())),
            DataBlock::PgStatActivity(a) => parts.push(format!("{} pg_sessions", a.len())),
            DataBlock::PgStatStatements(s) => parts.push(format!("{} pg_stat_statements", s.len())),
            DataBlock::PgStatDatabase(d) => parts.push(format!("{} pg_databases", d.len())),
            DataBlock::PgStatUserTables(t) => parts.push(format!("{} pg_tables", t.len())),
            DataBlock::PgStatUserIndexes(i) => parts.push(format!("{} pg_indexes", i.len())),
            DataBlock::PgStatBgwriter(_) => parts.push("pg_bgwriter".to_string()),
            _ => {}
        }
    }

    parts.join(", ")
}

fn main() {
    let args = Args::parse();

    // Initialize logging
    init_logging(args.verbose, args.quiet);

    let rotation_config = RotationConfig::new(args.max_size, args.max_days);

    info!("rpglotd {} starting", env!("CARGO_PKG_VERSION"));
    info!(
        "Config: interval={}s, output={}, proc={}",
        args.interval, args.output_dir, args.proc_path
    );
    info!(
        "Rotation policy: max_size={}, max_days={}",
        format_size(args.max_size),
        args.max_days
    );

    // Create collector
    #[cfg(target_os = "linux")]
    let mut collector = {
        let fs = RealFs::new();
        let mut c = Collector::new(fs, &args.proc_path);
        // Handle cgroup options
        if let Some(ref cgroup_path) = args.cgroup_path {
            c = c.with_cgroup(cgroup_path);
        } else if args.force_cgroup {
            c = c.force_cgroup(None);
        }
        c
    };
    #[cfg(not(target_os = "linux"))]
    let mut collector = {
        let fs = MockFs::new();
        let mut c = Collector::new(fs, &args.proc_path);
        // Handle cgroup options
        if let Some(ref cgroup_path) = args.cgroup_path {
            c = c.with_cgroup(cgroup_path);
        } else if args.force_cgroup {
            c = c.force_cgroup(None);
        }
        c
    };

    // Log cgroup collector status
    if collector.cgroup_enabled() {
        if let Some(ref path) = args.cgroup_path {
            info!("Cgroup collector: enabled (custom path: {})", path);
        } else if args.force_cgroup {
            info!("Cgroup collector: enabled (forced)");
        } else if is_container() {
            info!("Cgroup collector: enabled (container detected)");
        }
    } else {
        debug!("Cgroup collector: disabled (bare metal)");
    }

    // Enable PostgreSQL collector if requested
    if args.postgres {
        let pg_host = std::env::var("PGHOST").unwrap_or_else(|_| "localhost".to_string());
        let pg_port = std::env::var("PGPORT").unwrap_or_else(|_| "5432".to_string());

        match PostgresCollector::from_env() {
            Ok(mut pg_collector) => {
                match pg_collector.try_connect() {
                    Ok(()) => {
                        info!(
                            "PostgreSQL collector: enabled, connected to {}:{}",
                            pg_host, pg_port
                        );
                    }
                    Err(e) => {
                        warn!("PostgreSQL collector: connection failed ({})", e);
                        print_pg_warning(&e.to_string());
                    }
                }
                collector = collector.with_postgres(pg_collector);
            }
            Err(e) => {
                warn!("PostgreSQL collector: disabled ({})", e);
                print_pg_warning(&e.to_string());
            }
        }
    } else {
        debug!("PostgreSQL collector: disabled");
    }

    // Initialize storage
    let mut storage = StorageManager::new(&args.output_dir);
    info!("Storage initialized at {}", args.output_dir);

    let interval = Duration::from_secs(args.interval);

    // Setup graceful shutdown
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    if let Err(e) = ctrlc::set_handler(move || {
        info!("Received shutdown signal");
        r.store(false, Ordering::SeqCst);
    }) {
        warn!("Failed to set Ctrl-C handler: {}", e);
    }

    // Track the last hour when rotation was performed
    let mut last_rotation_hour: Option<u32> = None;
    let mut snapshot_count: u64 = 0;

    // Run initial rotation on startup
    match storage.rotate(&rotation_config) {
        Ok(result) => {
            if result.files_removed_by_age > 0 || result.files_removed_by_size > 0 {
                info!(
                    "Initial rotation: removed {} by age, {} by size, freed {}",
                    result.files_removed_by_age,
                    result.files_removed_by_size,
                    format_size(result.bytes_freed)
                );
            }
            info!(
                "Storage status: {} files, {}",
                result.files_remaining,
                format_size(result.total_size_after)
            );
        }
        Err(e) => {
            error!("Initial rotation failed: {}", e);
        }
    }

    info!("Starting collection loop");

    while running.load(Ordering::SeqCst) {
        let current_hour = Utc::now().hour();

        match collector.collect_snapshot() {
            Ok(snapshot) => {
                snapshot_count += 1;
                let description = describe_snapshot(&snapshot);
                let serialized_size = bincode::serialize(&snapshot).map(|s| s.len()).unwrap_or(0);

                info!(
                    "Snapshot #{}: {} ({})",
                    snapshot_count,
                    description,
                    format_size(serialized_size as u64)
                );

                // Log PostgreSQL error if any
                if args.postgres
                    && let Some(error) = collector.pg_last_error()
                {
                    warn!("PostgreSQL: {}", error);
                }

                let chunk_flushed = storage.add_snapshot(snapshot, collector.interner());
                debug!("WAL: {} snapshots pending", storage.current_chunk_size());

                // Clear interner after each snapshot to prevent memory accumulation
                collector.clear_interner();

                // Release memory to OS after chunk flush
                if chunk_flushed {
                    release_memory_to_os();
                    debug!("Memory released after chunk flush");
                }

                // Log memory metrics every 60 snapshots (~10 minutes)
                if snapshot_count.is_multiple_of(60) {
                    info!(
                        "Memory stats: collector_interner={} strings, wal_entries={}",
                        collector.interner().len(),
                        storage.current_chunk_size(),
                    );
                }
            }
            Err(e) => {
                error!("Failed to collect snapshot: {}", e);
            }
        }

        // Run rotation when hour changes
        if last_rotation_hour != Some(current_hour) {
            last_rotation_hour = Some(current_hour);

            match storage.rotate(&rotation_config) {
                Ok(result) => {
                    if result.files_removed_by_age > 0 || result.files_removed_by_size > 0 {
                        info!(
                            "Rotation: removed {} by age, {} by size, freed {}, {} files remaining ({})",
                            result.files_removed_by_age,
                            result.files_removed_by_size,
                            format_size(result.bytes_freed),
                            result.files_remaining,
                            format_size(result.total_size_after)
                        );
                    }
                }
                Err(e) => {
                    error!("Rotation failed: {}", e);
                }
            }
        }

        // Sleep with periodic checks for shutdown signal
        let sleep_interval = Duration::from_millis(100);
        let mut remaining = interval;
        while remaining > Duration::ZERO && running.load(Ordering::SeqCst) {
            let sleep_time = remaining.min(sleep_interval);
            std::thread::sleep(sleep_time);
            remaining = remaining.saturating_sub(sleep_time);
        }
    }

    // Graceful shutdown
    info!("Shutting down...");

    let pending = storage.current_chunk_size();
    if pending > 0 {
        info!("Flushing {} pending snapshots...", pending);
        if let Err(e) = storage.flush_chunk() {
            error!("Failed to flush chunk on shutdown: {}", e);
        } else {
            info!("Chunk flushed successfully");
        }
    }

    info!("Shutdown complete");
}

/// Prints a colored PostgreSQL warning with configuration hints.
fn print_pg_warning(error: &str) {
    // ANSI colors: red for error, yellow for hints, reset after
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

#[cfg(test)]
mod tests {
    use super::describe_snapshot;
    use rpglot_core::storage::Snapshot;
    use rpglot_core::storage::model::{DataBlock, PgStatActivityInfo, PgStatStatementsInfo};

    #[test]
    fn describe_snapshot_lists_all_blocks() {
        let snapshot = Snapshot {
            timestamp: 0,
            blocks: vec![
                DataBlock::Processes(Vec::new()),
                DataBlock::SystemDisk(Vec::new()),
                DataBlock::SystemNet(Vec::new()),
                DataBlock::PgStatActivity(vec![PgStatActivityInfo::default()]),
                DataBlock::PgStatStatements(vec![
                    PgStatStatementsInfo::default(),
                    PgStatStatementsInfo::default(),
                ]),
            ],
        };

        let desc = describe_snapshot(&snapshot);
        assert!(desc.contains("0 processes"));
        assert!(desc.contains("1 pg_sessions"));
        assert!(desc.contains("2 pg_stat_statements"));
    }
}
