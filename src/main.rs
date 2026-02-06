use tikv_jemallocator::Jemalloc;
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

mod collector;
mod provider;
mod storage;

use collector::Collector;
use provider::{HistoryProvider, LiveProvider, SnapshotProvider};
use storage::model::{
    DataBlock, PgStatActivityInfo, ProcessCpuInfo, ProcessInfo, ProcessMemInfo, SystemCpuInfo,
    SystemDiskInfo, SystemLoadInfo, SystemMemInfo, SystemNetInfo, SystemNetSnmpInfo, SystemPsiInfo,
    SystemStatInfo, SystemVmstatInfo,
};
use storage::{Snapshot, StorageManager};

fn main() {
    // Demo 1: Original storage demo
    println!("=== Demo 1: Storage with manual snapshot ===");
    demo_storage();

    // Demo 2: LiveProvider usage
    println!("\n=== Demo 2: LiveProvider (mock data) ===");
    demo_live_provider();

    // Demo 3: HistoryProvider usage
    println!("\n=== Demo 3: HistoryProvider ===");
    demo_history_provider();
}

fn demo_storage() {
    let mut manager = StorageManager::new("data");

    let mut interner = storage::StringInterner::new();
    let name_hash = interner.intern("systemd");
    let cmd_hash = interner.intern("/lib/systemd/systemd --system --deserialize 31");

    let pg_query_hash = interner.intern("SELECT * FROM users WHERE id = 1");
    let pg_db_hash = interner.intern("production_db");

    let snapshot = Snapshot {
        timestamp: chrono::Utc::now().timestamp(),
        blocks: vec![
            DataBlock::Processes(vec![ProcessInfo {
                pid: 1,
                name_hash,
                cmdline_hash: cmd_hash,
                mem: ProcessMemInfo {
                    vmem: 1024 * 1024,
                    rmem: 512 * 1024,
                    ..ProcessMemInfo::default()
                },
                cpu: ProcessCpuInfo {
                    utime: 100,
                    stime: 50,
                    ..ProcessCpuInfo::default()
                },
                ..ProcessInfo::default()
            }]),
            DataBlock::PgStatActivity(vec![PgStatActivityInfo {
                pid: 1234,
                datname_hash: pg_db_hash,
                query_hash: pg_query_hash,
                client_addr: "127.0.0.1".to_string(),
                ..PgStatActivityInfo::default()
            }]),
            DataBlock::SystemCpu(vec![SystemCpuInfo {
                cpu_id: -1,
                user: 1000,
                system: 500,
                idle: 8500,
                ..SystemCpuInfo::default()
            }]),
            DataBlock::SystemLoad(SystemLoadInfo {
                lavg1: 0.5,
                lavg5: 0.7,
                lavg15: 1.0,
                nr_running: 2,
                nr_threads: 250,
            }),
            DataBlock::SystemMem(SystemMemInfo {
                total: 16 * 1024 * 1024,
                free: 8 * 1024 * 1024,
                available: 12 * 1024 * 1024,
                ..SystemMemInfo::default()
            }),
            DataBlock::SystemNet(vec![SystemNetInfo {
                name_hash: interner.intern("eth0"),
                rx_bytes: 1_000_000,
                tx_bytes: 500_000,
                rx_packets: 10_000,
                tx_packets: 5_000,
                ..SystemNetInfo::default()
            }]),
            DataBlock::SystemDisk(vec![SystemDiskInfo {
                device_hash: interner.intern("nvme0n1"),
                rio: 10000,
                rsz: 50000,
                wio: 5000,
                wsz: 25000,
                ..SystemDiskInfo::default()
            }]),
            DataBlock::SystemPsi(vec![
                SystemPsiInfo {
                    resource: 0, // CPU
                    some_avg10: 0.5,
                    some_avg60: 0.3,
                    some_avg300: 0.2,
                    ..SystemPsiInfo::default()
                },
                SystemPsiInfo {
                    resource: 1, // Memory
                    some_avg10: 0.1,
                    full_avg10: 0.05,
                    ..SystemPsiInfo::default()
                },
            ]),
            DataBlock::SystemVmstat(SystemVmstatInfo {
                pgfault: 100_000,
                pgmajfault: 50,
                pswpin: 100,
                pswpout: 200,
                ..SystemVmstatInfo::default()
            }),
            DataBlock::SystemStat(SystemStatInfo {
                ctxt: 1_000_000,
                processes: 5000,
                procs_running: 3,
                procs_blocked: 1,
                btime: 1700000000,
            }),
            DataBlock::SystemNetSnmp(SystemNetSnmpInfo {
                tcp_active_opens: 1000,
                tcp_passive_opens: 500,
                tcp_curr_estab: 50,
                tcp_in_segs: 100_000,
                tcp_out_segs: 90_000,
                ..SystemNetSnmpInfo::default()
            }),
        ],
    };

    manager.add_snapshot(snapshot.clone());
    manager.add_snapshot(snapshot);
    manager.flush_chunk().unwrap();

    println!("Data saved to 'data' directory.");
}

fn demo_live_provider() {
    // Using mock filesystem for demo (works on macOS)
    use collector::mock::MockFs;

    let fs = MockFs::typical_system();
    let collector = Collector::new(fs, "/proc");
    let mut provider = LiveProvider::new(collector, None);

    println!(
        "Provider type: live={}, can_rewind={}",
        provider.is_live(),
        provider.can_rewind()
    );

    // Collect first snapshot
    if let Some(snapshot) = provider.advance() {
        println!("Collected snapshot at timestamp: {}", snapshot.timestamp);
        println!("  Blocks count: {}", snapshot.blocks.len());

        // Show process count
        for block in &snapshot.blocks {
            if let DataBlock::Processes(procs) = block {
                println!("  Processes: {}", procs.len());
            }
        }
    }

    // Collect another snapshot
    if let Some(snapshot) = provider.advance() {
        println!(
            "Collected another snapshot at timestamp: {}",
            snapshot.timestamp
        );
    }

    // Try to rewind (should stay at current)
    println!("Attempting rewind (not supported in live mode)...");
    if provider.rewind().is_some() {
        println!("  Rewind returned current snapshot (expected behavior)");
    }
}

fn demo_history_provider() {
    // Create some test snapshots
    let snapshots = vec![
        Snapshot {
            timestamp: 1000,
            blocks: vec![DataBlock::Processes(vec![ProcessInfo {
                pid: 1,
                name_hash: 111,
                ..ProcessInfo::default()
            }])],
        },
        Snapshot {
            timestamp: 1010,
            blocks: vec![DataBlock::Processes(vec![ProcessInfo {
                pid: 1,
                name_hash: 111,
                ..ProcessInfo::default()
            }])],
        },
        Snapshot {
            timestamp: 1020,
            blocks: vec![DataBlock::Processes(vec![ProcessInfo {
                pid: 2,
                name_hash: 222,
                ..ProcessInfo::default()
            }])],
        },
    ];

    let mut provider = HistoryProvider::from_snapshots(snapshots).unwrap();

    println!(
        "Provider type: live={}, can_rewind={}",
        provider.is_live(),
        provider.can_rewind()
    );
    println!("Total snapshots: {}", provider.len());

    // Navigate through history
    println!("\nNavigating forward:");
    if let Some(s) = provider.current() {
        println!(
            "  Position {}: timestamp={}",
            provider.position(),
            s.timestamp
        );
    }

    while let Some(s) = provider.advance() {
        let ts = s.timestamp;
        let pos = provider.position();
        if pos == provider.len() - 1 {
            println!("  Position {}: timestamp={} (end)", pos, ts);
            break;
        }
        println!("  Position {}: timestamp={}", pos, ts);
    }

    println!("\nNavigating backward:");
    while provider.position() > 0 {
        if let Some(s) = provider.rewind() {
            let ts = s.timestamp;
            println!("  Position {}: timestamp={}", provider.position(), ts);
        }
    }

    // Jump to specific position
    println!("\nJumping to position 2:");
    if let Some(s) = provider.jump_to(2) {
        let ts = s.timestamp;
        println!("  Position {}: timestamp={}", provider.position(), ts);
    }
}

// Example of using Box<dyn SnapshotProvider> for polymorphism
#[allow(dead_code)]
fn process_any_provider(provider: &mut dyn SnapshotProvider) {
    println!("Processing provider (live={})", provider.is_live());

    if let Some(snapshot) = provider.advance() {
        println!("Got snapshot with {} blocks", snapshot.blocks.len());
    }

    if provider.can_rewind() {
        println!("This provider supports rewinding");
    }
}

// Example of creating provider based on args
#[allow(dead_code)]
fn create_provider_from_args(
    history_file: Option<&str>,
) -> Result<Box<dyn SnapshotProvider>, provider::ProviderError> {
    use collector::mock::MockFs;

    if let Some(_path) = history_file {
        // Would use: HistoryProvider::from_path(path)
        // For demo, using mock data:
        let snapshots = vec![Snapshot {
            timestamp: 1000,
            blocks: vec![],
        }];
        Ok(Box::new(HistoryProvider::from_snapshots(snapshots)?))
    } else {
        let fs = MockFs::typical_system();
        let collector = Collector::new(fs, "/proc");
        Ok(Box::new(LiveProvider::new(collector, None)))
    }
}
