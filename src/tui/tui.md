# TUI Architecture

This document describes the Terminal User Interface subsystem for rpglot viewer.

## Overview

The TUI module provides an interactive terminal interface similar to atop/htop for viewing system metrics in real-time or from historical data.

## IMPORTANT: Data Source Rule

**The TUI has a strict separation of data sources:**

| Mode | Data Source | Description |
|------|-------------|-------------|
| **Live** | `Collector` | Real-time data collection from `/proc` filesystem |
| **History** | `Storage` | Historical data from stored chunk files |

This rule is fundamental to the architecture:
- **Live mode** ALWAYS gets data from the `Collector` module
- **History mode** ALWAYS gets data from the `Storage` module
- The TUI itself does NOT collect or store data directly
- The `SnapshotProvider` trait abstracts this difference from the TUI

## Directory Structure

```
src/tui/
├── mod.rs              # Module exports
├── tui.md              # This documentation
├── app.rs              # App struct, main loop
├── state.rs            # AppState, TableState, Tab
├── event.rs            # Event enum, EventHandler
├── render.rs           # Main rendering logic
├── input.rs            # Keybindings
├── style.rs            # Color scheme (atop-style)
└── widgets/
    ├── mod.rs
    ├── header.rs       # Time, mode, tabs
    ├── summary.rs      # CPU, MEM, Load, DSK, NET, PSI, VMS
    ├── processes.rs    # Process table
    ├── postgres.rs     # PostgreSQL sessions table
    └── help.rs         # Context-sensitive help
```

## Core Components

### App (`app.rs`)

Main application struct that orchestrates the TUI:

```rust
pub struct App {
    provider: Box<dyn SnapshotProvider>,
    state: AppState,
    history: SparklineHistory,
    should_quit: bool,
}
```

| Method | Description |
|--------|-------------|
| `new(provider)` | Creates App with given SnapshotProvider |
| `run(tick_rate)` | Main event loop |

### AppState (`state.rs`)

Holds all UI state:

```rust
pub struct AppState {
    pub current_tab: Tab,
    pub input_mode: InputMode,
    pub filter_input: String,
    pub process_table: TableState<ProcessRow>,
    pub current_snapshot: Option<Snapshot>,
    pub previous_snapshot: Option<Snapshot>,
    pub paused: bool,
    pub history_position: Option<(usize, usize)>,
    pub is_live: bool,
    pub process_view_mode: ProcessViewMode,      // g/c/m/d keys
    pub prev_process_mem: HashMap<u32, (u64, u64)>,  // For VGROW/RGROW
    pub prev_process_cpu: HashMap<u32, (u64, u64)>,  // For CPU%
    pub prev_total_cpu_time: Option<u64>,            // For CPU% normalization
    pub horizontal_scroll: usize,                // For wide tables
    pub show_help: bool,                         // Help popup visibility
    pub tab_states: HashMap<Tab, TabState>,      // Per-tab filter/sort/selection state
}
```

### TabState

Per-tab state for filter, sort, and selection. Each tab remembers its own settings when switching between tabs:

```rust
pub struct TabState {
    pub filter: Option<String>,     // Filter string
    pub sort_column: usize,         // Sort column index
    pub sort_ascending: bool,       // Sort direction
    pub selected: usize,            // Selected row index
}
```

When switching tabs, the current tab's state is saved and the target tab's state is restored automatically.

### TableState

Generic table state with sorting, filtering, and diff tracking:

```rust
pub struct TableState<T: TableRow> {
    pub items: Vec<T>,
    pub selected: usize,
    pub sort_column: usize,
    pub sort_ascending: bool,
    pub filter: Option<String>,
    pub diff_status: HashMap<u64, DiffStatus>,
}
```

### TableRow Trait

Defines interface for table items:

```rust
pub trait TableRow: Clone {
    fn id(&self) -> u64;
    fn column_count() -> usize;
    fn headers() -> Vec<&'static str>;
    fn cells(&self) -> Vec<String>;
    fn sort_key(&self, column: usize) -> SortKey;
    fn matches_filter(&self, filter: &str) -> bool;
}
```

### DiffStatus

Tracks changes between snapshots:

```rust
pub enum DiffStatus {
    New,                     // Green - new item
    Modified(Vec<usize>),    // Yellow - changed columns
    Unchanged,
}
```

## Keybindings

### General

| Key | Action |
|-----|--------|
| `q` | Quit (confirmation: press `q` again or `Enter`, cancel: `Esc`) |
| `Ctrl+C` | Quit |
| `Tab`, `Shift+Tab` | Switch tabs |
| `1-3` | Jump to specific tab (1=PRC, 2=PGA, 3=PGS) |
| `Space` | Pause/resume (live mode only) |
| `→` | Next snapshot (history mode) |
| `t` | Next snapshot (history mode; **except PGS tab**, where `t` is a view mode key) |
| `T` | Previous snapshot (history mode) |
| `←/→` | Navigate history (history mode, same as t/T) |
| `b` | Jump to a specific time (history mode; supports `-1h`, `16:00`, ISO 8601, Unix timestamp) |
| `?`, `H` | Toggle help popup |
| `!` | Toggle debug popup (live mode only) |
| `Enter` | Toggle detail popup (PRC: process detail, PGA: session detail, PGS: statement detail) |
| `Esc` | Close popup |
| `↑/↓`, `j/k` | Scroll help/detail popups |
| `PgUp/PgDn` | Page scroll in popups |

### Process Table Navigation

| Key | Action |
|-----|--------|
| `↑/↓`, `j/k` | Navigate rows |
| `PgUp/PgDn` | Page navigation |
| `Home/End` | Jump to first/last |
| `h/l` | Horizontal scroll (for wide tables) |

### Process Table Sorting & Filtering

| Key | Action |
|-----|--------|
| `s/S` | Cycle sort column (works in PRC, PGA, PGS tabs) |
| `r/R` | Reverse sort direction (works in PRC, PGA, PGS tabs) |
| `/` | Enter filter mode |
| `p/P` | Enter filter mode (atop-style process name filter) |
| `i/I` | Context-sensitive: toggle hide/show idle sessions (PGA tab) or switch to I/O view (PGS tab) |
| `Esc` | Cancel filter and clear |
| `Enter` | Confirm filter and exit filter mode |

**Filter mode**: Filter is applied in real-time as you type. Matches substring in process name, cmdline, or PID (case-insensitive).

**Sorting**: Sorting is available in PRC (Processes), PGA (PostgreSQL Activity) and PGS (PostgreSQL Statements) tabs. The current sort column is indicated by ▲ (ascending) or ▼ (descending) in the column header.

### Process View Modes (atop-style)

| Key | Mode | Columns |
|-----|------|--------|
| `g/G` | Generic | PID, SYSCPU, USRCPU, RDELAY, VGROW, RGROW, RUID, EUID, ST, EXC, THR, S, CPUNR, CPU, CMD |
| `c/C` | Command | PID, TID, S, CPU, MEM, COMMAND-LINE |
| `m/M` | Memory | PID, TID, MINFLT, MAJFLT, VSTEXT, VSLIBS, VDATA, VSTACK, LOCKSZ, VSIZE, RSIZE, PSIZE, VGROW, RGROW, SWAPSZ, RUID, EUID, MEM, CMD |
| `d/D` | Disk | PID, RDDSK, WRDSK, WCANCL, DSK, CMD |

### PGA View Modes

| Key | Mode | Columns |
|-----|------|--------|
| `g/G` | Generic | PID, CPU%, RSS, DB, USER, STATE, WAIT, QDUR, XDUR, BDUR, BTYPE, QUERY |
| `v/V` | Stats | PID, DB, USER, STATE, QDUR, MEAN, MAX, CALL/s, HIT%, QUERY |

**Stats view** enriches pg_stat_activity sessions with pg_stat_statements metrics linked by `query_id` (PostgreSQL 14+). Shows anomaly highlighting when QDUR exceeds MEAN or MAX.

### Drill-down Navigation

| Key | From Tab | To Tab | Link |
|-----|----------|--------|------|
| `>` or `J` | PRC | PGA | Navigate to PostgreSQL session by PID |
| `>` or `J` | PGA | PGS | Navigate to statement stats by query_id |

Drill-down navigation allows you to follow the data flow: from OS process (PRC) to PostgreSQL session (PGA) to query statistics (PGS).

**Popup restrictions:** Tab switching (`Tab`, `Shift+Tab`, `1`/`2`/`3`) and drill-down (`>`/`J`) are blocked while a detail popup is open. A yellow status message appears in the header explaining to close the popup first (`Esc`). The message is cleared when the popup is closed.

**Persistent selection tracking:**
- After drill-down, the selection is tracked by PID (PGA) or queryid (PGS), not by row index
- When data updates (new snapshot, re-sort), the same session/statement stays selected
- If the tracked session/statement disappears from data, tracking is reset
- Manual navigation (↑/↓) switches tracking to the newly selected row
- Tracking is reset when switching to a different tab

### PostgreSQL Integration in PRC Tab

When a process PID matches a PostgreSQL backend PID from `pg_stat_activity`, the CMD/COMMAND-LINE column displays the current query or backend type:

**Display format**: 
- `name [query]` (Generic/Memory) or `cmdline [query]` (Command view) — when query is present and non-empty
- `name [backend_type]` or `cmdline [backend_type]` — when query is empty but backend_type is available (e.g., `autovacuum worker`, `walwriter`, `checkpointer`)

**Color coding**: Processes with PostgreSQL query or backend_type are highlighted in **cyan** for easy identification.

**Example**:
```
PID     ... CMD
12345   ... postgres [SELECT * FROM users WHERE id = $1]
12346   ... postgres [UPDATE orders SET status = 'shipped']
12347   ... postgres [autovacuum worker]
12348   ... postgres [walwriter]
12349   ... bash
```

This integration allows you to see which PostgreSQL backends are running and what queries they're executing (or their backend type if idle) directly from the OS process list, without switching to the PGA tab.

### Debug Popup (Live Mode)

Press `!` to open a debug popup showing collector timing and rates state. Available only in live mode.

**Sections:**

| Section | Fields |
|---------|--------|
| Collector Timing | Total, Stat, Processes, MemInfo, CPUInfo, LoadAvg, DiskStats, NetDev, PSI, Vmstat, NetSNMP, PG Activity, PG Statements, Cgroup, PGS Cache Intv |
| PGS Rates State | prev_sample_ts, last_update_ts, dt_secs, rates_count |
| PostgreSQL Error | Last error message (if any) |

**PGS Cache Intv** shows the `pg_stat_statements` caching interval:
- `0 (disabled)`: Fresh data on every tick (live mode default)
- `30s`: Cached for 30 seconds (daemon mode default)

If you see `dt` significantly larger than your tick interval in live mode, check that `PGS Cache Intv` shows `0 (disabled)`. If it shows `30s`, you may be running an older binary.

**Color coding:**
- Yellow: Timing > 10ms
- Red: Timing > 100ms

This popup is useful for diagnosing performance issues and understanding why `dt` in PGS tab might differ from the TUI update interval.

### Process Detail Popup

Press `Enter` on a selected process in the PRC tab to open a detailed popup with comprehensive process information.

**Sections:**

| Section | Fields |
|---------|--------|
| Identity | PID, PPID, Name, Command, State, TTY, Start time, Threads |
| User/Group | Real UID/User, Effective UID/User, Real GID, Effective GID |
| CPU | CPU%, User time, System time, Current CPU, Run delay, Nice, Priority, RT Priority, Policy, I/O wait time, Vol. ctx sw/s, Invol. ctx sw/s |
| Memory | MEM%, Virtual, Resident, PSS, VGROW, RGROW, Code, Data, Stack, Libraries, Locked, Swap, Minor/Major faults |
| Disk I/O | Read ops, Read bytes, Write ops, Write bytes, Cancelled writes |

**Keybindings:**
- `↑/↓`, `j/k` to scroll content
- `PgUp/PgDn` to scroll by page
- `Enter` or `Esc` to close the popup

**Color coding:**
- Cyan: Key labels
- Yellow: Section headers
- White: Values

## Tabs

| Tab | Shortcut | Content |
|-----|----------|---------|
| PRC | 1 | Process table |
| PGA | 2 | PostgreSQL activity (pg_stat_activity) |
| PGS | 3 | PostgreSQL statements (pg_stat_statements) |

### PGA Tab (PostgreSQL Activity)

The PGA tab displays PostgreSQL active sessions from `pg_stat_activity` with OS-level metrics. It supports two view modes with different column sets.

**View modes:**

| Key | Mode | Columns |
|-----|------|---------|
| `g` | Generic | PID, CPU%, RSS, DB, USER, STATE, WAIT, QDUR, XDUR, BDUR, BTYPE, QUERY |
| `v` | Stats | PID, DB, USER, STATE, QDUR, MEAN, MAX, CALL/s, HIT%, QUERY |

Press `v` to toggle between Generic and Stats view. Stats view enriches sessions with `pg_stat_statements` metrics linked by `query_id` (PostgreSQL 14+).

**Generic view columns:**

| Column | Description |
|--------|-------------|
| PID | PostgreSQL backend process ID |
| CPU% | CPU usage from OS (linked by PID) |
| RSS | Resident Set Size from OS (linked by PID) |
| DB | Database name |
| USER | PostgreSQL user name |
| STATE | Connection state (active/idle/idle in transaction) |
| WAIT | Wait event type:event (e.g., Lock:tuple) |
| QDUR | Query duration (time since query_start) |
| XDUR | Transaction duration (time since xact_start) |
| BDUR | Backend duration (time since backend_start) |
| BTYPE | Backend type (client backend, autovacuum worker, etc.) |
| QUERY | Current/last query text |

**Stats view columns:**

| Column | Description |
|--------|-------------|
| PID | PostgreSQL backend process ID |
| DB | Database name |
| USER | PostgreSQL user name |
| STATE | Connection state |
| QDUR | Query duration (with anomaly highlighting) |
| MEAN | Mean execution time from pg_stat_statements |
| MAX | Max execution time from pg_stat_statements |
| CALL/s | Calls per second from pg_stat_statements |
| HIT% | Buffer cache hit percentage from pg_stat_statements |
| QUERY | Current/last query text |

**Stats view anomaly highlighting:**
- Yellow: QDUR > 2× MEAN (query slower than usual)
- Red: QDUR > 5× MEAN (query much slower than usual)
- Red+Bold: QDUR > MAX (new record — potential issue!)
- Yellow: HIT% < 80% (many disk reads)
- Red: HIT% < 50% (excessive disk reads)
- `--`: No stats available (query_id = 0 or not in pg_stat_statements)

**Drill-down navigation:**
- Press `>` or `J` to navigate from PGA to PGS for the selected query (by query_id)
- Requires query_id (available in PostgreSQL 14+)

**Sorting (default)**: Non-idle sessions first, sorted by QDUR descending (longest running first). Idle sessions are shown at the bottom in gray. Use `s/S` to cycle sort column, `r/R` to reverse direction.

**Filtering**: Press `/` or `p` to filter by PID, query_id, DB, USER, or QUERY. PID and query_id use prefix match; text fields use case-insensitive substring match. Press `i` to toggle hide/show idle sessions.

**Navigation**: `↑/↓`, `j/k` to navigate rows, `PgUp/PgDn` for page navigation, `Home/End` to jump to first/last.

**Color coding (Generic view)**:
- Green: active state
- Yellow: idle in transaction, QDUR > 1 min, WAIT event present (but not for idle sessions)
- Red: QDUR > 5 min (for active queries)
- Gray (dim): idle sessions

**QUERY column**: Expands to fill remaining terminal width for better readability.

### PostgreSQL Session Detail Popup

Press `Enter` on a selected session in the PGA tab to open a detailed popup with comprehensive PostgreSQL session information.

**Sections:**

| Section | Fields |
|---------|--------|
| Session Identity | PID, Database, User, Application, Client Address, Backend Type |
| Timing | Backend Start, Transaction Start, Query Start, Query/Transaction/Backend Duration |
| State & Wait | State, Wait Event Type, Wait Event |
| OS Process | OS PID, Threads, State, User/System Time, CPU, Nice, Priority, Memory (Virtual/RSS/Shared/Swap), Page Faults, Disk I/O |
| Query | Full query text (with scroll support) |

**Keybindings:**
- `↑/↓`, `j/k` to scroll content
- `PgUp/PgDn` to scroll by page
- `Enter` or `Esc` to close the popup

**Color coding:**
- Cyan: Key labels
- Yellow: Section headers, idle in transaction state
- Green: Active state
- Red: Long-running queries (> 5 min)
- Gray: Idle state, missing OS process info

**Popup persistence:** When opened, the popup is locked to the selected session's PID. Data updates (new snapshots, re-sorting) will refresh the popup content but keep showing the same session. The popup only changes when you close it and open another session.

### PGS Tab (PostgreSQL Statements)

The PGS tab displays query statistics from `pg_stat_statements` (TOP 500 by `total_exec_time`).

Most columns in the table are **rate metrics** ("per second", `/s`) computed from deltas between
two real `pg_stat_statements` samples.

**Caching behavior:**
- **Live mode (`rpglot`)**: No caching — fresh data every tick for real-time monitoring
- **History mode (`rpglot -r`)**: Data reflects what `rpglotd` recorded (with daemon's 30s caching)

**Title indicators:** The table title shows sampling information based on `collected_at` timestamps:
- `dt=Xs` — interval between two real pg_stat_statements samples; rates (`/s`) are computed using this period
- `age=Ys` — time since last real data collection (`snapshot.timestamp - collected_at`)

In live mode, `dt` and `age` will match your tick interval (e.g., 1s with default settings). In history mode, they reflect the daemon's collection intervals.

`--` means there is not enough data yet (first sample, or after `pg_stat_statements` reset).

**View modes (best-practice “common views”):**

| Key | Mode | Focus |
|-----|------|-------|
| `t` | Time | Total/mean/min/max timings |
| `c` | Calls | Hot paths (frequency) |
| `i` | I/O | Shared buffer reads/hits/writes |
| `e` | Temp | Temp blocks / spill indicators |

**Sorting (default):** `TIME/s` descending (Time view).

**Filtering:** Press `/` or `p` to filter by **queryid** (prefix match), **DB**, **USER**, or **QUERY** (case-insensitive substring match).

**History navigation note:** in history mode, the key `t` normally advances to the next snapshot, but on the PGS tab it is reserved for switching to Time view. Use `→` for “next snapshot” while on PGS.

### PostgreSQL Statement Detail Popup

Press `Enter` on a selected statement in the PGS tab to open a detailed popup.

**Sections:**
- Rates (/s) (calls/s, time_ms/s, I/O/s)
- Identity (queryid, db, user, calls/rows)
- Timing (total/mean/min/max/stddev, plan time)
- I/O (shared/local/temp blocks, cache hit%)
- Temp/WAL (temp blocks, estimated temp MB, WAL records/bytes)
- Query (full normalized query text)

**Popup persistence:** When opened, the popup is locked to the selected statement's queryid. Data updates will refresh the popup content but keep showing the same statement. The popup only changes when you close it and open another statement.

#### Note on other tabs

This document previously described additional tabs (e.g. `SYS`/`DSK`/`NET`). In the current implementation, system/disk/network metrics are shown in the **summary panel**, while the main content tabs are `PRC`, `PGA`, and `PGS`.

## Event System

Uses a separate thread for non-blocking event polling:

```rust
pub enum Event {
    Tick,           // Timer for data refresh
    Key(KeyEvent),  // Keyboard input
    Resize(w, h),   // Terminal resize
}
```

## Color Scheme (atop-style)

- **Header**: Blue background, white text
- **Selected row**: Dark gray background
- **New items**: Green
- **Modified items**: Yellow
- **Critical values**: Red (bold)
- **CPU metrics**: Cyan
- **Memory metrics**: Magenta
- **Disk metrics**: Yellow

## Summary Panel (atop-style)

The summary panel displays system-wide metrics with anomaly highlighting for quick problem identification.

### Two-Column Layout

The panel is split into two vertical columns with a separator `│`:
- **Left column**: Memory, swap, disk×N, network×N (MEM, SWP, DSK×N, NET×N)
- **Right column**: Load, CPU, pressure stall, vmstat rates (CPL, CPU, cpu×N, PSI, VMS)

#### Container-aware rendering (cgroup)

If the snapshot contains `DataBlock::Cgroup` (works in both **live** and **history** mode), the summary adapts for container workloads:

- **MEM** line switches to cgroup memory **limit/usage** when `memory.max` is set (not `max`), and **SWP** line is hidden (swap is usually not meaningful inside containers).
- **CPU** line switches to cgroup CPU **quota-based** metrics when `cpu.max` is limited (`quota > 0`), and per-core `cpu×N` lines are hidden.
- **DSK** lines are filtered using mountinfo-derived device IDs: in container snapshots we only show disks with non-zero `major/minor` (collector keeps `major/minor` only for devices present in `/proc/self/mountinfo`, and sets it to `0:0` for unrelated host devices).
- **NET** lines are filtered to `eth*` / `veth*` only.

This layout maximizes space utilization and groups related metrics together.

```
┌─────────────────────────────────────────────────┬───┬───────────────────────────────────────────────────────┐
│ LEFT COLUMN (Memory/Storage/Network)            │ │ │ RIGHT COLUMN (CPU/Load)                               │
├─────────────────────────────────────────────────┼───┼───────────────────────────────────────────────────────┤
│ MEM │ tot: 15.7 GiB  avail: 13.0 GiB  cache:... │ │ │ CPL │ avg1:  0.17  avg5:  0.24  avg15:  0.26  procs:...│
│ SWP │ tot: 16.7 GiB  free: 16.7 GiB  swpd:...   │ │ │ CPU │ sys:  0.2%  usr:  0.4%  irq:  0.1%  idle: 99.3% │
│ DSK │ vda:  rMB: 0.0 wMB: 0.0 r/s: 0 w/s: 0 ...  │ │ │ cpu │ sys:  0.3%  usr:  0.5%  ... cpu:  0             │
│ dsk │ sda:  rMB: 0.0 wMB: 0.0 r/s: 0 w/s: 0 ...  │ │ │ cpu │ sys:  0.3%  usr:  0.5%  ... cpu:  1             │
│ NET │ eth0: rxMB: 0.0 txMB: 0.0 rxPkt: 0K err: 0│ │ │ cpu │ ...                                             │
│ net │ ens3: rxMB: 0.0 txMB: 0.0 rxPkt: 0K err: 0│ │ │                                                       │
├─────────────────────────────────────────────────┴───┴───────────────────────────────────────────────────────┤
│ q:quit(qq/Enter) s:sort r:rev /:filter b:time g/c/m/d:view ?:help                                            │
└─────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Column width calculation**:
- Widths are calculated based on content (fixed metric widths from `metric_widths` module)
- If terminal is wide enough, extra space goes to right column
- If terminal is narrow, both columns shrink proportionally (min 40 chars each)

**Adaptive height**: Both columns can have different heights (left: MEM+SWP+DSK×N+NET×N, right: CPL+CPU+cpu×N). The shorter column is padded with empty lines.

### Fixed Metric Widths

Each metric has a predefined width ensuring stable positioning — values don't shift when their content changes.

**Format**: `label:` + right-aligned value = constant width per metric.

| Metric Group | Label | Total Width | Example |
|--------------|-------|-------------|---------|
| CPL | avg1, avg5 | 12 | `avg1:    0.17` |
| CPL | avg15 | 13 | `avg15:    0.26` |
| CPL | procs | 12 | `procs:     786` |
| CPL | run | 8 | `run:    1` |
| CPU | sys, usr, irq, iow | 11 | `sys:    0.2%` |
| CPU | idle | 12 | `idle:   99.3%` |
| CPU | stl | 11 | `stl:    0.0%` |
| CPU | cpu | 8 | `cpu:    0` |
| MEM | tot, buf | 15 | `tot:   15.7 GiB` |
| MEM | avail | 18 | `avail:    13.0 GiB` |
| MEM | cache | 17 | `cache:    1.3 GiB` |
| MEM | slab | 15 | `slab: 742 MiB` |
| DSK/NET | rMB/rxMB, wMB/txMB | 10 | `rMB:   4.0` |
| DSK/NET | rd/s/rxPk, wr/s/txPk | 11 | `rd/s:   396` |
| DSK/NET | aw/er, ut/dr | 9 | `aw:   0.1` |

### Anomaly Highlighting Rules

| Section | Metric | Yellow (warning) | Red (critical) |
|---------|--------|------------------|----------------|
| CPL | avg1 | > num_cpus | > num_cpus × 2 |
| CPL | run | > num_cpus | > num_cpus × 2 |
| CPU | sys% | > 20% | > 40% |
| CPU | irq% | > 5% | > 15% |
| CPU | iow% | > 10% | > 30% |
| CPU | idle% | < 20% | < 5% |
| CPU | stl% | > 5% | > 15% |
| MEM | avail | < 15% total | < 5% total |
| MEM | slab | > 3 GiB | > 5 GiB |
| SWP | swpd | > 0 | > 1 GiB |
| SWP | dirty | > 500 MiB | > 2 GiB |
| SWP | wback | > 0 | - |
| DSK | await | > 20 ms | > 100 ms |
| DSK | util% | > 50% | > 80% |
| NET | err | > 0 | > 100 |
| NET | drp | > 0 | - |
| PSI | some% | > 5% | > 10% |
| VMS | swin/swout | - | > 0 (any swapping) |

### Line Descriptions

**Left Column (Memory/Storage/Network):**
- **MEM**: Memory (total/available/cache/buffers/slab)
- **SWP**: Swap (total/free/used), dirty pages, writeback
- **DSK×N**: Each of top-2 disks on separate line (rMB/wMB MB/s, r/s w/s IOPS, await ms, util%)
- **NET×N**: Each of top-2 interfaces on separate line (rxMB/txMB MB/s, rxPkt/txPkt packets/s, errors, drops)

**Right Column (CPU/Load/Pressure):**
- **CPL**: Load averages (1/5/15 min), process count, running processes
- **CPU**: Total CPU breakdown (sys/usr/irq/iow/idle/steal)
- **cpu**: Top-5 busiest CPU cores (sorted by usage)
- **PSI**: Pressure Stall Information (CPU/MEM/I/O some% avg10)
- **VMS**: Vmstat rates (pgin/pgout/swin/swout/flt/ctx per second)

## Adaptive Column Widths

Column widths are calculated adaptively based on actual data content to ensure values fit without truncation.

### How it works

1. **Calculation timing**: Widths are calculated once on the first snapshot and recalculated on terminal resize
2. **Content-based**: Each column width = max(header_length, max_cell_content_length)
3. **Minimum widths**: Each column has a minimum width that is always respected
4. **Column types**:
   - **Fixed**: Exact width based on content (PID, S, THR)
   - **Flexible**: Adapts to content, can shrink proportionally (RUID, EUID, CPU%)
   - **Expandable**: Takes remaining space (CMD, CMDLINE)

### Width distribution

- If total width ≤ available: extra space goes to Expandable columns
- If total width > available: Flexible/Expandable columns shrink proportionally, Fixed columns keep their width

### Implementation

```rust
pub struct CachedWidths {
    pub generic: Vec<u16>,
    pub command: Vec<u16>,
    pub memory: Vec<u16>,
}

pub enum ColumnType {
    Fixed,      // Width = max(content)
    Flexible,   // min_width..max_content
    Expandable, // Takes remaining space
}
```

Widths are cached in `AppState.cached_widths` and invalidated on terminal resize.

## Integration with Provider

```rust
// Live mode
let provider = LiveProvider::new(collector, None);
let app = App::new(Box::new(provider));

// History mode
let provider = HistoryProvider::from_path("./data")?;
let app = App::new(Box::new(provider));

app.run(Duration::from_secs(10))?;
```

## Screen Layout

The TUI screen is divided into three horizontal sections:

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│ HEADER (1 line)                                                                 │
│ │ Time | Mode (LIVE/HISTORY/PAUSED) | Tabs | PIDs (container) | Position/Filter │
├─────────────────────────────────────────────────────────────────────────────────┤
│ SUMMARY (two-column layout + help line)                                         │
│ ┌─────────────────────────────────┬───┬───────────────────────────────────────┐ │
│ │ MEM │ tot: 16.0 GiB  avail:...  │ │ │ CPL │ avg1: 1.23  avg5: 0.98  ...     │ │
│ │ SWP │ tot:  8.0 GiB  free:...   │ │ │ CPU │ sys: 5.1% usr: 18.2% ...        │ │
│ │ DSK │ vda: rMB: 0.0 wMB: 0.0... │ │ │ cpu │ sys: 0.3% usr: 0.5% ... cpu: 0  │ │
│ │ dsk │ sda: rMB: 0.0 wMB: 0.0... │ │ │ cpu │ ...                             │ │
│ │ NET │ eth0: rxMB: 0.0 txMB: 0.0..│ │ │ cpu │ ...                             │ │
│ │ net │ ens3: rxMB: 0.0 txMB: 0.0..│ │ │                                       │ │
│ └─────────────────────────────────┴───┴───────────────────────────────────────┘ │
│ q:quit(qq/Enter) s:sort r:rev /:filter g/c/m/d:view ?:help                       │
├─────────────────────────────────────────────────────────────────────────────────┤
│ CONTENT (remaining space, min 10 lines)                                         │
│ │ Tab-specific content:                                                        │
│ │ [PRC] Tab: Process table with view modes (g/c/m/d)                           │
│ │ [SYS] Tab: PSI metrics, vmstat counters                                      │
│ │ [DSK] Tab: Disk I/O statistics per device                                    │
│ │ [NET] Tab: Network statistics per interface                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Widget Locations

| Widget | File | Location | Height |
|--------|------|----------|--------|
| Header | `widgets/header.rs` | Top | 1 line |
| Summary | `widgets/summary.rs` | Below header | Dynamic (calculated from content) |
| Process Table | `widgets/processes.rs` | Content area (PRC tab) | Min 10 lines |
| System Metrics | `widgets/system.rs` | Content area (SYS/DSK/NET tabs) | Min 10 lines |

### Layout Code

The layout is defined in `render.rs`:

```rust
// Calculate summary height dynamically based on content
let summary_height = calculate_summary_height(state.current_snapshot.as_ref());

let chunks = Layout::vertical([
    Constraint::Length(1),              // Header
    Constraint::Length(summary_height), // Summary (dynamic: MEM, SWP, DSK×N, NET×N | CPL, CPU, cpu×N, Help)
    Constraint::Min(10),                // Content area
])
.split(area);
```

### Dynamic Summary Height

- The summary panel height is calculated at runtime based on snapshot content.

Bare metal (no cgroup data):
- Left column: MEM + SWP + DSK×N + NET×N (where N = min(actual_devices, TOP_DISKS/TOP_NETS))
- Right column: CPL + CPU + cpu×N (where N = min(actual_cpus, TOP_CPUS))

Container (cgroup present + limits set):
- Left column: MEM + DSK×N + NET×N (SWP hidden when `memory.max` is limited)
- Right column: CPL + CPU (no per-core lines when `cpu.max` is limited)
- Final height = max(left_lines, right_lines) + 1 (help line)

This ensures the summary panel uses exactly as much space as needed, without wasting vertical space.

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                           App                                │
│  ┌─────────────┐  ┌─────────────┐                           │
│  │   AppState  │  │ EventHandler│                           │
│  │  - Tab      │  │  - tick_rate│                           │
│  │  - TableState│ │  - rx/tx    │                           │
│  └──────┬──────┘  └──────┬──────┘                           │
│         │                │                                   │
│         └────────┬───────┘                                   │
│                  │                                           │
│         ┌────────▼────────┐                                  │
│         │ SnapshotProvider│ (LiveProvider or HistoryProvider)│
│         └─────────────────┘                                  │
└─────────────────────────────────────────────────────────────┘
```

## Help System

The TUI provides context-sensitive help via the `?` or `H` keys, showing column descriptions for the current tab.

**Scroll support**: Help popup supports scrolling for long content. Use `↑/↓` or `j/k` to scroll line by line, `PgUp/PgDn` for page scrolling. The footer shows the current scroll position when content exceeds the visible area.

### Help Implementation

Help content is defined in `widgets/help.rs`:

| Function | Tab | Content |
|----------|-----|---------|
| `get_process_help()` | PRC | Column descriptions for Generic (g), Command (c), Memory (m), and Disk (d) view modes |
| `get_system_help()` | SYS | PSI metrics explanation and vmstat/stat counters |
| `get_disk_help()` | DSK | Disk I/O statistics column descriptions |
| `get_network_help()` | NET | Interface statistics and TCP/UDP summary descriptions |

### MANDATORY: Keep Help in Sync

**When modifying any tab's displayed content, you MUST update the corresponding help function in `widgets/help.rs`.**

Checklist for any TUI change:
1. [ ] Adding/removing a column? → Update help descriptions
2. [ ] Changing column meaning/calculation? → Update help descriptions  
3. [ ] Adding new metrics section? → Add help section
4. [ ] Changing color coding rules? → Update help color coding section
5. [ ] Adding new keybinding? → Update keybindings table AND summary help text

The summary widget (`widgets/summary.rs`) shows quick help with context-sensitive hints:
- **PRC tab**: `q:quit(qq/Enter) s:sort r:rev /:filter b:time g/c/m/d:view ?:help`
- **PGA tab**: `q:quit(qq/Enter) s:sort r:rev /:filter b:time i:hide idle ?:help`
- **Other tabs**: `q:quit(qq/Enter) s:sort r:rev /:filter b:time ?:help`

## Testing

Run the TUI:

```bash
# Live mode (default) with 1 second interval
rpglot

# Live mode with 5 second interval
rpglot 5

# History mode (default path: /var/log/rpglot)
rpglot -r

# History mode with custom path
rpglot -r ./data

# History mode starting from specific time
rpglot -r -b 2026-02-07T17:00:00   # ISO 8601
rpglot -r -b 1738944000            # Unix timestamp
rpglot -r -b -1h                   # 1 hour ago
rpglot -r -b 07:00                 # Today at 07:00 UTC
rpglot -r -b 2026-02-07:07:00      # Date with time (UTC)

# History mode with custom path and start time
rpglot -r ./data -b -1h
```
