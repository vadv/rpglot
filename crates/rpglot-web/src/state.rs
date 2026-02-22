//! Shared application state, global statics, and memory management.

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

/// Releases unused memory back to the operating system.
/// Uses jemalloc's arena purge to reduce RSS after memory-intensive operations.
#[cfg(not(target_env = "msvc"))]
pub(crate) fn release_memory_to_os() {
    unsafe {
        // MALLCTL_ARENAS_ALL = 4096: purge dirty pages from ALL jemalloc arenas.
        // Using arena.0 only purges one arena, missing allocations from tokio worker threads.
        tikv_jemalloc_sys::mallctl(
            c"arena.4096.purge".as_ptr().cast(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            0,
        );
    }
}

#[cfg(target_env = "msvc")]
pub(crate) fn release_memory_to_os() {}

use std::collections::HashMap;
use std::ptr;
use std::sync::atomic::{AtomicI64, AtomicUsize};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use tokio::sync::broadcast;

use rpglot_core::api::snapshot::ApiSnapshot;
use rpglot_core::provider::SnapshotProvider;
use rpglot_core::rates::{PgiRateState, PgpRateState, PgsRateState, PgtRateState};
use rpglot_core::storage::heatmap::HeatmapBucket;
use rpglot_core::storage::model::Snapshot;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Mode {
    Live,
    History,
}

pub(crate) struct WebAppInner {
    pub(crate) provider: Box<dyn SnapshotProvider + Send>,
    pub(crate) mode: Mode,
    // Current API snapshot
    pub(crate) current_snapshot: Option<Arc<ApiSnapshot>>,
    // Raw snapshots for delta/rates computation
    pub(crate) raw_snapshot: Option<Snapshot>,
    pub(crate) prev_snapshot: Option<Snapshot>,
    // Rates
    pub(crate) pgs_rate: PgsRateState,
    pub(crate) pgp_rate: PgpRateState,
    pub(crate) pgt_rate: PgtRateState,
    pub(crate) pgi_rate: PgiRateState,
    // History metadata (updated by refresh task)
    pub(crate) total_snapshots: Option<usize>,
    pub(crate) history_start: Option<i64>,
    pub(crate) history_end: Option<i64>,
    // Heatmap cache: per-date ("YYYY-MM-DD" → bucketed data).
    // Past dates are immutable — cached forever. Today invalidated on refresh.
    pub(crate) heatmap_cache: HashMap<String, Vec<HeatmapBucket>>,
    // Instance metadata (database name + PG version + is_standby), cached from provider.
    pub(crate) instance_info: Option<(String, String, Option<bool>)>,
    // Machine hostname, obtained at startup.
    pub(crate) hostname: String,
}

pub(crate) type SharedState = Arc<Mutex<WebAppInner>>;

pub(crate) type AppState = State<(SharedState, broadcast::Sender<Arc<ApiSnapshot>>)>;

pub(crate) static LAST_CLIENT_ACTIVITY: AtomicI64 = AtomicI64::new(0);

pub(crate) static SSE_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
