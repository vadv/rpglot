# Collector Module

The collector module is responsible for gathering system metrics from the Linux `/proc` filesystem and PostgreSQL databases, converting them into `Snapshot` structures for storage.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              Collector                                   │
│  ┌─────────────────────┐   ┌─────────────────────┐   ┌────────────────┐  │
│  │  ProcessCollector   │   │   SystemCollector   │   │PostgresCollector│ │
│  │  - /proc/[pid]/*    │   │  - /proc/meminfo    │   │- pg_stat_activity│ │
│  │  - StringInterner   │   │  - /proc/stat       │   │  (optional)    │  │
│  └──────────┬──────────┘   │  - /proc/loadavg    │   └───────┬────────┘  │
│             │              │  - /proc/diskstats  │           │           │
│             │              │  - /proc/net/dev    │           │           │
│             │              │  - /proc/pressure/* │           │           │
│             │              │  - /proc/vmstat     │           │           │
│             │              └──────────┬──────────┘           │           │
│             └──────────────┬──────────┘                      │           │
│                            │                                 │           │
│                     ┌──────▼──────┐                   ┌──────▼──────┐    │
│                     │  FileSystem │                   │  PostgreSQL │    │
│                     │   (trait)   │                   │   (native)  │    │
│                     └──────┬──────┘                   └──────┬──────┘    │
└────────────────────────────┼─────────────────────────────────┼──────────┘
                             │                                 │
             ┌───────────────┼───────────────┐                 │
             │               │               │                 │
      ┌──────▼──────┐ ┌──────▼──────┐ ┌──────▼──────┐   ┌──────▼──────┐
      │   RealFs    │ │   MockFs    │ │  Scenarios  │   │ PostgreSQL  │
      │ (Linux)     │ │ (Testing)   │ │ (Fixtures)  │   │   Server    │
      └─────────────┘ └─────────────┘ └─────────────┘   └─────────────┘
```

## Components

### Collector (`collector.rs`)

Main entry point that combines all sub-collectors:

| Method | Description |
|--------|-------------|
| `new(fs, proc_path)` | Creates collector with given filesystem and proc path. Automatically enables cgroup collector if running in container. |
| `with_postgres(collector)` | Enables PostgreSQL metrics collection (builder pattern) |
| `with_cgroup(cgroup_path)` | Enables cgroup metrics collection with custom path |
| `force_cgroup(cgroup_path)` | Forces cgroup collection regardless of container detection |
| `cgroup_enabled()` | Returns whether cgroup collector is enabled |
| `collect_snapshot()` | Collects all metrics into a `Snapshot` |
| `interner()` | Returns reference to string interner |
| `user_resolver()` | Returns reference to user resolver (UID → username) |
| `postgres_collector_mut()` | Returns mutable reference to PostgreSQL collector if enabled |

**Automatic Cgroup Detection:**

When `Collector::new()` is called, it automatically checks if the process is running inside a container using `is_container()`. If so, cgroup collector is enabled with the default path `/sys/fs/cgroup`.

To override this behavior:
- Use `with_cgroup(path)` to specify a custom cgroup path
- Use `force_cgroup(None)` to enable on bare metal with default path
- Use `force_cgroup(Some(path))` to enable on bare metal with custom path

### ProcessCollector (`procfs/process.rs`)

Collects per-process information:

| Data Source | Fields Collected |
|-------------|-----------------|
| `/proc/[pid]/stat` | pid, ppid, state, num_threads, exit_signal, tty, utime, stime, nice, priority, vsize, rss, minflt, majflt, curcpu, rt_priority, policy, blkdelay, starttime |
| `/proc/[pid]/status` | uid, euid, gid, egid, vm_data, vm_stk, vm_lib, vm_swap, vm_lck, voluntary/nonvoluntary ctxt switches |
| `/proc/[pid]/cmdline` | Full command line (interned) |
| `/proc/[pid]/comm` | Process name (interned) |
| `/proc/[pid]/io` | syscr, syscw, read_bytes, write_bytes, cancelled_write_bytes |

**Methods:**
| Method | Description |
|--------|-------------|
| `new(fs, proc_path)` | Creates collector with given filesystem and proc path |
| `set_boot_time(boot_time)` | Sets system boot time for calculating process start times |
| `collect_process(pid)` | Collects info for a single process |
| `collect_all_processes()` | Collects info for all processes |
| `interner()` / `interner_mut()` | Access to string interner |

**Process Start Time Calculation:**
The `ProcessInfo.btime` field contains the process start time (seconds since epoch).
It is calculated as: `btime = boot_time + (starttime_jiffies / CLK_TCK)`
where `CLK_TCK = 100` (standard Linux value).

To enable this calculation, `Collector.collect_snapshot()` first collects `SystemStat`
(which contains boot time from `/proc/stat`) and calls `set_boot_time()` on `ProcessCollector`
before collecting processes.

**Output:** `Vec<ProcessInfo>` with:
- Identity: pid, ppid, uid, euid, gid, egid, tty, state, num_threads, exit_signal, btime
- Strings: name_hash, cmdline_hash (via StringInterner)
- Memory: minflt, majflt, vexec, vmem, rmem, pmem, vdata, vstack, vlibs, vswap, vlock
- CPU: utime, stime, nice, prio, rtprio, policy, curcpu, rundelay, blkdelay, nvcsw, nivcsw
- Disk I/O: rio, rsz, wio, wsz, cwsz

### SystemCollector (`procfs/system.rs`)

Collects system-wide metrics:

| Data Source | Description | Method |
|-------------|-------------|--------|
| `/proc/meminfo` | Memory statistics | `collect_meminfo()` |
| `/proc/stat` | CPU statistics, context switches, forks | `collect_cpuinfo()`, `collect_stat()` |
| `/proc/loadavg` | System load average | `collect_loadavg()` |
| `/proc/diskstats` | Per-device I/O counters | `collect_diskstats()` |
| `/proc/net/dev` | Per-interface network counters | `collect_net_dev()` |
| `/proc/net/snmp` + `/proc/net/netstat` | TCP/UDP protocol statistics | `collect_netsnmp()` |
| `/proc/pressure/*` | PSI (Pressure Stall Information) | `collect_psi()` |
| `/proc/vmstat` | VM statistics (faults, swap, OOM) | `collect_vmstat()` |

**Output:** `SystemMemInfo`, `Vec<SystemCpuInfo>`, `SystemLoadInfo`, `Vec<SystemDiskInfo>`, `Vec<SystemNetInfo>`, `Vec<SystemPsiInfo>`, `SystemVmstatInfo`, `SystemStatInfo`, `SystemNetSnmpInfo`

#### SystemDiskInfo Fields

| Field | Source | Description |
|-------|--------|-------------|
| `device_name` | `/proc/diskstats` col 3 | Device name (sda, nvme0n1, etc.) |
| `device_hash` | Interned | Hash for delta encoding |
| `major` | `/proc/diskstats` col 1 | Block device major number. In container snapshots this is kept only for devices present in `/proc/self/mountinfo`, otherwise it is stored as `0`. |
| `minor` | `/proc/diskstats` col 2 | Block device minor number. In container snapshots this is kept only for devices present in `/proc/self/mountinfo`, otherwise it is stored as `0`. |
| `rio` | `/proc/diskstats` col 4 | Read I/O operations completed |
| `r_merged` | `/proc/diskstats` col 5 | Read requests merged |
| `rsz` | `/proc/diskstats` col 6 | Sectors read (512 bytes each) |
| `read_time` | `/proc/diskstats` col 7 | Time spent reading (ms) |
| `wio` | `/proc/diskstats` col 8 | Write I/O operations completed |
| `w_merged` | `/proc/diskstats` col 9 | Write requests merged |
| `wsz` | `/proc/diskstats` col 10 | Sectors written (512 bytes each) |
| `write_time` | `/proc/diskstats` col 11 | Time spent writing (ms) |
| `io_in_progress` | `/proc/diskstats` col 12 | I/Os currently in progress |
| `io_ms` | `/proc/diskstats` col 13 | Time spent doing I/O (ms) |
| `qusz` | `/proc/diskstats` col 14 | Weighted time spent doing I/O (ms) |

#### SystemNetSnmpInfo Fields

| Field | Source | Description |
|-------|--------|-------------|
| `tcp_active_opens` | `/proc/net/snmp` Tcp: ActiveOpens | Active connection openings |
| `tcp_passive_opens` | `/proc/net/snmp` Tcp: PassiveOpens | Passive connection openings |
| `tcp_attempt_fails` | `/proc/net/snmp` Tcp: AttemptFails | Failed connection attempts |
| `tcp_estab_resets` | `/proc/net/snmp` Tcp: EstabResets | Connection resets received |
| `tcp_curr_estab` | `/proc/net/snmp` Tcp: CurrEstab | Currently established connections |
| `tcp_in_segs` | `/proc/net/snmp` Tcp: InSegs | Total TCP segments received |
| `tcp_out_segs` | `/proc/net/snmp` Tcp: OutSegs | Total TCP segments sent |
| `tcp_retrans_segs` | `/proc/net/snmp` Tcp: RetransSegs | TCP segments retransmitted |
| `tcp_in_errs` | `/proc/net/snmp` Tcp: InErrs | TCP segments with errors |
| `tcp_out_rsts` | `/proc/net/snmp` Tcp: OutRsts | TCP RST segments sent |
| `udp_in_datagrams` | `/proc/net/snmp` Udp: InDatagrams | UDP datagrams received |
| `udp_out_datagrams` | `/proc/net/snmp` Udp: OutDatagrams | UDP datagrams sent |
| `udp_in_errors` | `/proc/net/snmp` Udp: InErrors | UDP datagrams with errors |
| `udp_no_ports` | `/proc/net/snmp` Udp: NoPorts | UDP datagrams to unknown port |
| `listen_overflows` | `/proc/net/netstat` TcpExt: ListenOverflows | Listen queue overflows |
| `listen_drops` | `/proc/net/netstat` TcpExt: ListenDrops | Listen queue drops |
| `tcp_timeouts` | `/proc/net/netstat` TcpExt: TCPTimeouts | TCP timeouts |
| `tcp_fast_retrans` | `/proc/net/netstat` TcpExt: TCPFastRetrans | Fast retransmits |
| `tcp_slow_start_retrans` | `/proc/net/netstat` TcpExt: TCPSlowStartRetrans | Slow start retransmits |
| `tcp_ofo_queue` | `/proc/net/netstat` TcpExt: TCPOFOQueue | Out-of-order packets queued |
| `tcp_syn_retrans` | `/proc/net/netstat` TcpExt: TCPSynRetrans | SYN retransmits |

### CgroupCollector (`cgroup/collector.rs`)

Collects cgroup v2 metrics for containers. Only active when `is_container()` returns true.

| Data Source | Fields Collected |
|-------------|-----------------|
| `/sys/fs/cgroup/cpu.max` | quota, period |
| `/sys/fs/cgroup/cpu.stat` | usage_usec, user_usec, system_usec, throttled_usec, nr_throttled |
| `/sys/fs/cgroup/memory.max` | max (limit) |
| `/sys/fs/cgroup/memory.current` | current (usage) |
| `/sys/fs/cgroup/memory.stat` | anon, file, kernel, slab |
| `/sys/fs/cgroup/memory.events` | oom_kill |
| `/sys/fs/cgroup/pids.current` | current (process count) |
| `/sys/fs/cgroup/pids.max` | max (limit) |

**Output:** `CgroupInfo` with optional `CgroupCpuInfo`, `CgroupMemoryInfo`, `CgroupPidsInfo`

### PostgresCollector (`pg_collector/`)

Collects PostgreSQL metrics via native connection. This collector is **optional** and enabled via `with_postgres()` method.

The collector is split into submodules:
- `mod.rs` — struct, connection management, target database auto-detection
- `queries.rs` — SQL query builders
- `activity.rs` — `collect()` for pg_stat_activity
- `statements.rs` — `collect_statements()` with caching
- `database.rs` — `collect_database()` for pg_stat_database
- `tables.rs` — `collect_tables()` for pg_stat_user_tables
- `indexes.rs` — `collect_indexes()` for pg_stat_user_indexes

#### Configuration

PostgreSQL connection is configured via standard environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `PGHOST` | `localhost` | PostgreSQL server hostname |
| `PGPORT` | `5432` | PostgreSQL server port |
| `PGUSER` | `$USER` | Database username |
| `PGPASSWORD` | `""` (empty) | Database password |
| `PGDATABASE` | same as `PGUSER` | Database name. If not set, auto-detection is enabled. |
| `PGSSLMODE` | `prefer` | SSL mode: `disable`, `prefer`, `require` |

#### Target Database Auto-Detection

`pg_stat_user_tables` and `pg_stat_user_indexes` are **per-database views** — they only show data for the currently connected database. To collect meaningful data, the collector automatically detects the largest non-template database.

**Rules:**
- **Single connection** — all views (cluster-wide and per-database) are queried through one connection
- If `PGDATABASE` is explicitly set → connect to that database, auto-detection is **disabled**
- If `PGDATABASE` is not set → auto-detection is **enabled**:
  - On first connect, detect the largest database by `pg_database_size()`
  - Every **10 minutes**, re-check and reconnect if the largest database changed
  - Detection query: `SELECT datname FROM pg_database WHERE NOT datistemplate AND datname NOT IN ('postgres') ORDER BY pg_database_size(datname) DESC LIMIT 1`
  - If the query fails (no permissions), stay on the current database
- On reconnect (database change), all per-connection caches are cleared (statements, tables, indexes)

#### Data Source: pg_stat_activity

| Field | Source | Description |
|-------|--------|-------------|
| `pid` | `pg_stat_activity.pid` | Process ID of the backend |
| `datname_hash` | `pg_stat_activity.datname` | Database name (interned) |
| `usename_hash` | `pg_stat_activity.usename` | Username (interned) |
| `application_name_hash` | `pg_stat_activity.application_name` | Application name (interned) |
| `client_addr` | `pg_stat_activity.client_addr` | Client IP address |
| `state_hash` | `pg_stat_activity.state` | Connection state (interned) |
| `query_hash` | `pg_stat_activity.query` | Current query text (interned) |
| `query_id` | `pg_stat_activity.query_id` | Query identifier (PostgreSQL 14+, otherwise 0) |
| `wait_event_type_hash` | `pg_stat_activity.wait_event_type` | Wait event type (interned) |
| `wait_event_hash` | `pg_stat_activity.wait_event` | Wait event name (interned) |
| `backend_type_hash` | `pg_stat_activity.backend_type` | Backend type (interned) |
| `backend_start` | `pg_stat_activity.backend_start` | Backend process start time (epoch seconds) |
| `xact_start` | `pg_stat_activity.xact_start` | Transaction start time (epoch seconds, 0 if none) |
| `query_start` | `pg_stat_activity.query_start` | Current query start time (epoch seconds) |

**Output:** `Vec<PgStatActivityInfo>`

#### Data Source: pg_stat_statements

This data source is collected only when the `pg_stat_statements` extension is installed.

Collection behavior:
- Extension presence is checked at most once per 5 minutes (cached)
- **Caching is configurable** via `with_statements_interval()`:
  - `rpglot` (TUI live mode): `Duration::ZERO` — no caching, fresh data every tick
  - `rpglotd` (daemon): default 30 seconds — reduces load on PostgreSQL
- Collects **TOP 500** statements by `total_exec_time`
- Column names differ between PostgreSQL versions (legacy `*_time` vs `*_exec_time` in PG 13+)
- Database and user names are collected via joins (`pg_database` by `dbid`, `pg_roles` by `userid`) and interned for UI filtering
- Each `PgStatStatementsInfo` includes `collected_at` timestamp (Unix epoch seconds) indicating when the data was actually collected from PostgreSQL; cached results retain the original `collected_at` value

**Timestamp for rate calculation:**
The `collected_at` field allows TUI to compute accurate per-second rates (`/s`) regardless of the TUI refresh interval:
- TUI calculates `dt = current.collected_at - prev.collected_at`
- In live mode (no caching), rates are computed from actual collection intervals
- In daemon mode (with caching), rates remain stable based on the ~30s interval

**Output:** `Vec<PgStatStatementsInfo>`

#### Data Source: pg_stat_database

Cluster-wide view with one row per database. All numeric fields are cumulative counters.

| Field | Source | Description |
|-------|--------|-------------|
| `datid` | `pg_stat_database.datid` | Database OID (diff key) |
| `datname_hash` | `pg_stat_database.datname` | Database name (interned) |
| `xact_commit` | `pg_stat_database.xact_commit` | Committed transactions |
| `xact_rollback` | `pg_stat_database.xact_rollback` | Rolled back transactions |
| `blks_read` | `pg_stat_database.blks_read` | Disk blocks read |
| `blks_hit` | `pg_stat_database.blks_hit` | Buffer cache hits |
| `tup_returned` | `pg_stat_database.tup_returned` | Rows returned |
| `tup_fetched` | `pg_stat_database.tup_fetched` | Rows fetched |
| `tup_inserted` | `pg_stat_database.tup_inserted` | Rows inserted |
| `tup_updated` | `pg_stat_database.tup_updated` | Rows updated |
| `tup_deleted` | `pg_stat_database.tup_deleted` | Rows deleted |
| `deadlocks` | `pg_stat_database.deadlocks` | Deadlocks detected |
| `temp_files` | `pg_stat_database.temp_files` | Temp files created |
| `temp_bytes` | `pg_stat_database.temp_bytes` | Temp bytes written |
| `session_time` | `pg_stat_database.session_time` | Total session time ms (PG 14+) |
| `active_time` | `pg_stat_database.active_time` | Active time ms (PG 14+) |

**Output:** `Vec<PgStatDatabaseInfo>`

#### Data Source: pg_stat_user_tables

Per-database view showing one row per user table. Only tables in the currently connected database are visible. Uses 30-second caching (same as pg_stat_statements). Collects TOP 500 tables by total scans (seq_scan + idx_scan).

| Field | Source | Description |
|-------|--------|-------------|
| `relid` | `pg_stat_user_tables.relid` | Table OID (diff key) |
| `schemaname_hash` | `pg_stat_user_tables.schemaname` | Schema name (interned) |
| `relname_hash` | `pg_stat_user_tables.relname` | Table name (interned) |
| `seq_scan` | `pg_stat_user_tables.seq_scan` | Sequential scans (cumulative) |
| `seq_tup_read` | `pg_stat_user_tables.seq_tup_read` | Rows from seq scans (cumulative) |
| `idx_scan` | `pg_stat_user_tables.idx_scan` | Index scans (cumulative) |
| `idx_tup_fetch` | `pg_stat_user_tables.idx_tup_fetch` | Rows from idx scans (cumulative) |
| `n_tup_ins` | `pg_stat_user_tables.n_tup_ins` | Rows inserted (cumulative) |
| `n_tup_upd` | `pg_stat_user_tables.n_tup_upd` | Rows updated (cumulative) |
| `n_tup_del` | `pg_stat_user_tables.n_tup_del` | Rows deleted (cumulative) |
| `n_tup_hot_upd` | `pg_stat_user_tables.n_tup_hot_upd` | HOT updates (cumulative) |
| `n_live_tup` | `pg_stat_user_tables.n_live_tup` | Estimated live rows (gauge) |
| `n_dead_tup` | `pg_stat_user_tables.n_dead_tup` | Estimated dead rows (gauge) |
| `vacuum_count` | `pg_stat_user_tables.vacuum_count` | Manual vacuums (cumulative) |
| `autovacuum_count` | `pg_stat_user_tables.autovacuum_count` | Auto vacuums (cumulative) |
| `last_vacuum` | `pg_stat_user_tables.last_vacuum` | Last manual vacuum (epoch secs, 0=never) |
| `last_autovacuum` | `pg_stat_user_tables.last_autovacuum` | Last autovacuum (epoch secs, 0=never) |

All columns exist since PG 9.1+, no version-awareness needed.

**Output:** `Vec<PgStatUserTablesInfo>`

#### Data Source: pg_stat_user_indexes

Per-database view showing one row per user index. Only indexes in the currently connected database are visible. Uses 30-second caching. Collects TOP 500 indexes by idx_scan count.

| Field | Source | Description |
|-------|--------|-------------|
| `indexrelid` | `pg_stat_user_indexes.indexrelid` | Index OID (diff key) |
| `relid` | `pg_stat_user_indexes.relid` | Parent table OID |
| `schemaname_hash` | `pg_stat_user_indexes.schemaname` | Schema name (interned) |
| `relname_hash` | `pg_stat_user_indexes.relname` | Table name (interned) |
| `indexrelname_hash` | `pg_stat_user_indexes.indexrelname` | Index name (interned) |
| `idx_scan` | `pg_stat_user_indexes.idx_scan` | Index scans (cumulative) |
| `idx_tup_read` | `pg_stat_user_indexes.idx_tup_read` | Index entries returned (cumulative) |
| `idx_tup_fetch` | `pg_stat_user_indexes.idx_tup_fetch` | Live rows fetched (cumulative) |
| `size_bytes` | `pg_relation_size(indexrelid)` | Index size in bytes |

All columns exist since PG 9.1+, no version-awareness needed.

**Output:** `Vec<PgStatUserIndexesInfo>`

#### Error Handling

- Connection is established lazily on first metric collection
- Automatic reconnection on connection loss
- If PostgreSQL is unavailable, collection is skipped (no error propagation)
- Does not affect system metrics collection
- Last error message is stored in `Collector.pg_last_error()` for TUI display

**Common error messages:**
| Error | Description |
|-------|-------------|
| `PostgreSQL: PGUSER or USER not set` | Neither PGUSER nor USER environment variable is set |
| `PostgreSQL: connection refused` | Server not running or wrong host/port |
| `PostgreSQL: password authentication failed` | Wrong password or no password provided |
| `PostgreSQL collector not configured` | `with_postgres()` was not called |

### FileSystem Trait (`traits.rs`)

Abstraction for filesystem operations:

```rust
pub trait FileSystem: Send + Sync {
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
    fn exists(&self, path: &Path) -> bool;
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;
}
```

**Implementations:**
- `RealFs` - Real filesystem (production on Linux)
- `MockFs` - In-memory filesystem (testing on macOS)

### MockFs (`mock/`)

Mock filesystem for testing:

| Component | Description |
|-----------|-------------|
| `MockFs` | In-memory filesystem implementation |
| `Scenarios` | Pre-built test fixtures (`typical_system()`, `empty()`, etc.) |

## Data Flow

```
/proc filesystem
       │
       ▼
┌─────────────────┐
│  FileSystem     │  (RealFs or MockFs)
│  trait          │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Parser         │  Raw text → Parsed structs
│  (parser.rs)    │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Collectors     │  ProcessCollector + SystemCollector
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Collector      │  Combines into Snapshot
└────────┬────────┘
         │
         ▼
    Snapshot
    (DataBlocks)
```

## Output Format

The collector produces a `Snapshot` containing `DataBlock` variants:

| DataBlock | Source | Description |
|-----------|--------|-------------|
| `Processes(Vec<ProcessInfo>)` | ProcessCollector | Per-process metrics |
| `SystemMem(SystemMemInfo)` | SystemCollector | Memory statistics |
| `SystemCpu(Vec<SystemCpuInfo>)` | SystemCollector | Per-CPU counters |
| `SystemLoad(SystemLoadInfo)` | SystemCollector | Load averages |
| `SystemDisk(Vec<SystemDiskInfo>)` | SystemCollector | Per-device I/O stats |
| `SystemNet(Vec<SystemNetInfo>)` | SystemCollector | Per-interface network stats |
| `SystemNetSnmp(SystemNetSnmpInfo)` | SystemCollector | TCP/UDP protocol stats |
| `SystemPsi(Vec<SystemPsiInfo>)` | SystemCollector | Pressure Stall Information |
| `SystemVmstat(SystemVmstatInfo)` | SystemCollector | VM statistics |
| `SystemStat(SystemStatInfo)` | SystemCollector | Context switches, forks |
| `PgStatActivity(Vec<PgStatActivityInfo>)` | PostgresCollector | PostgreSQL active sessions |
| `PgStatStatements(Vec<PgStatStatementsInfo>)` | PostgresCollector | PostgreSQL query statistics (pg_stat_statements) |
| `PgStatDatabase(Vec<PgStatDatabaseInfo>)` | PostgresCollector | PostgreSQL database-level statistics |
| `PgStatUserTables(Vec<PgStatUserTablesInfo>)` | PostgresCollector | PostgreSQL per-table statistics (per-database) |
| `PgStatUserIndexes(Vec<PgStatUserIndexesInfo>)` | PostgresCollector | PostgreSQL per-index statistics (per-database) |
| `Cgroup(CgroupInfo)` | CgroupCollector | Container resource limits and usage |

## Usage Examples

### Production (Linux)

```rust
use rpglot::collector::{Collector, RealFs};

let fs = RealFs::new();
let mut collector = Collector::new(fs, "/proc");
let snapshot = collector.collect_snapshot().unwrap();
```

### Testing (with MockFs)

```rust
use rpglot::collector::{Collector, MockFs};

let fs = MockFs::typical_system();
let mut collector = Collector::new(fs, "/proc");
let snapshot = collector.collect_snapshot().unwrap();
```

### With PostgreSQL Metrics

```rust
use rpglot::collector::{Collector, RealFs, PostgresCollector, PostgresConfig};

let fs = RealFs::new();

// Create PostgreSQL collector from environment variables
let pg_collector = PostgresCollector::from_env();

// Enable PostgreSQL metrics collection
let mut collector = Collector::new(fs, "/proc")
    .with_postgres(pg_collector);

let snapshot = collector.collect_snapshot().unwrap();
// snapshot.blocks may now include PgStatActivity if PostgreSQL is available
```

### With Cgroup Metrics (Containers)

Cgroup collector is automatically enabled when running inside a container:

```rust
use rpglot::collector::{Collector, RealFs};

let fs = RealFs::new();

// Automatic: cgroup enabled if is_container() returns true
let mut collector = Collector::new(fs, "/proc");

let snapshot = collector.collect_snapshot().unwrap();
// snapshot.blocks includes Cgroup if running in container
```

### Force Cgroup on Bare Metal (Testing)

```rust
use rpglot::collector::{Collector, RealFs};

let fs = RealFs::new();

// Force cgroup collection even on bare metal
let mut collector = Collector::new(fs, "/proc")
    .force_cgroup(None);  // uses default /sys/fs/cgroup

// Or with custom path:
let mut collector = Collector::new(fs, "/proc")
    .force_cgroup(Some("/custom/cgroup/path"));
```

## String Interning

The collector uses `StringInterner` to deduplicate strings:

- Process names and command lines
- Device names (sda, nvme0n1, etc.)
- Network interface names (eth0, lo, etc.)
- PostgreSQL database names, usernames, queries, wait events
- Strings are stored once and referenced by hash
- Reduces memory usage for repeated snapshots
- Interner persists across collections for consistent hashes

## User Resolution

The collector includes `UserResolver` for mapping UID/EUID to usernames:

- Reads `/etc/passwd` at initialization
- Provides `resolve(uid)` method returning username or UID as string if not found
- Used by TUI to display usernames instead of numeric IDs in RUID/EUID columns

## Error Handling

`CollectError` enum covers all collection failures:

| Variant | Description |
|---------|-------------|
| `IoError` | File read/directory listing failed |
| `ParseError` | Invalid format in proc file |

Individual process failures don't stop collection - the collector continues with other processes.
