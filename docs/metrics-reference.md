# rpglot Metrics Reference

This document describes all metrics, tabs, views, and panels in rpglot.
It is intended for UI designers implementing contextual help and tooltips.

---

## Table of Contents

- [Summary Panel](#summary-panel)
- [PRC — OS Processes](#prc--os-processes)
- [PGA — pg_stat_activity](#pga--pg_stat_activity)
- [PGS — pg_stat_statements](#pgs--pg_stat_statements)
- [PGT — pg_stat_user_tables](#pgt--pg_stat_user_tables)
- [PGI — pg_stat_user_indexes](#pgi--pg_stat_user_indexes)
- [PGL — pg_locks](#pgl--pg_locks)
- [Threshold Coloring](#threshold-coloring)
- [Glossary](#glossary)

---

## Summary Panel

The summary panel is always visible at the top of the screen. It shows system-wide and PostgreSQL-wide aggregated metrics.

### System Metrics

#### CPU

| Key | Label | Unit | Description | Interpretation |
|-----|-------|------|-------------|----------------|
| sys_pct | System | % | Time spent executing kernel code | High values (>20%) indicate heavy system call activity or I/O. Investigate with PRC tab disk I/O metrics. |
| usr_pct | User | % | Time spent executing user-space code | Normal for application workloads. High + idle low = CPU-bound queries. |
| irq_pct | IRQ | % | Time spent handling hardware/software interrupts | Usually <5%. High values may indicate network-intensive workloads or hardware issues. |
| iow_pct | I/O Wait | % | Time CPU was idle waiting for disk I/O | >10% suggests disk bottleneck. Check PGT I/O view for tables doing physical reads. |
| idle_pct | Idle | % | Time CPU had nothing to do | Near 0% means CPU saturation. |
| steal_pct | Steal | % | Time stolen by hypervisor (virtualized environments) | >5% means the VM host is overcommitted. Request more CPU allocation or migrate. |

**How to read:** These percentages sum to 100%. A healthy system typically shows high idle_pct during normal operations.

#### Load Average

| Key | Label | Description | Interpretation |
|-----|-------|-------------|----------------|
| avg1 | 1 min | 1-minute load average | Instantaneous pressure. Compare to number of CPU cores. |
| avg5 | 5 min | 5-minute load average | Short-term trend. If >2x cores, investigate. |
| avg15 | 15 min | 15-minute load average | Long-term trend. Useful for capacity planning. |
| nr_threads | Threads | Total kernel threads | Sudden increases may indicate connection storms or fork bombs. |
| nr_running | Running | Currently running/runnable threads | Should be <= number of CPU cores. If consistently higher, CPU-bound. |

**How to read:** Load average = average number of processes waiting for CPU or I/O. Load of 4.0 on a 4-core system means full utilization. On 8 cores, it's 50% utilized.

#### Memory

| Key | Label | Unit | Description | Interpretation |
|-----|-------|------|-------------|----------------|
| total_kb | Total | KB (shown as GiB) | Total physical RAM | Reference value. |
| available_kb | Available | KB (shown as GiB) | Memory available for new allocations (includes reclaimable cache) | Key metric. If <10% of total, memory pressure is high. |
| cached_kb | Cached | KB (shown as GiB) | Page cache (file data cached in RAM) | PostgreSQL relies heavily on OS page cache. Low cache = more disk I/O. |
| buffers_kb | Buffers | KB (shown as MiB) | Kernel buffer cache (metadata, directory entries) | Usually small. |
| slab_kb | Slab | KB (shown as MiB) | Kernel slab allocator (inode cache, dentry cache) | Growth may indicate many open files or directories. |

**How to read:** `available_kb` is the most important metric. It accounts for reclaimable cache. If it drops below 1-2 GiB on a production server, action is needed.

#### Swap

| Key | Label | Unit | Description | Interpretation |
|-----|-------|------|-------------|----------------|
| total_kb | Total | KB (shown as GiB) | Total swap space configured | Reference value. |
| free_kb | Free | KB (shown as GiB) | Available swap space | If much less than total, system has been under memory pressure. |
| used_kb | Used | KB (shown as GiB) | Currently used swap | Any swap usage for PostgreSQL backends is bad for performance. |
| dirty_kb | Dirty | KB (shown as MiB) | Swap pages waiting to be written to disk | Should be low. |
| writeback_kb | Writeback | KB (shown as MiB) | Swap pages currently being written to disk | Transient, should be low. |

**How to read:** PostgreSQL performance degrades severely when shared_buffers or backend memory is swapped out. Ideally used_kb = 0.

#### Pressure (PSI)

| Key | Label | Unit | Description | Interpretation |
|-----|-------|------|-------------|----------------|
| cpu_some_pct | CPU | % | Percentage of time at least one task was waiting for CPU | >10% = CPU contention exists. |
| mem_some_pct | Memory | % | Percentage of time at least one task was stalled on memory | >5% = memory pressure. May cause swapping. |
| io_some_pct | I/O | % | Percentage of time at least one task was waiting for I/O | >10% = I/O bottleneck. Check disk throughput and PGT I/O. |

**How to read:** PSI (Pressure Stall Information) measures actual resource contention. Unlike utilization, PSI directly indicates whether processes are waiting. Available on Linux 4.20+.

#### VMstat

| Key | Label | Unit | Description | Interpretation |
|-----|-------|------|-------------|----------------|
| pgin_s | Page In | /s | Pages read from disk to memory per second | Includes both file reads and swap-ins. |
| pgout_s | Page Out | /s | Pages written from memory to disk per second | Includes both file writes and swap-outs. |
| swin_s | Swap In | /s | Pages read from swap per second | Any swap-in activity for a PostgreSQL server is concerning. |
| swout_s | Swap Out | /s | Pages written to swap per second | Active swapping = severe memory pressure. |
| pgfault_s | Faults | /s | Page faults per second (minor + major) | Minor faults are normal. Major faults mean disk I/O. |
| ctxsw_s | Context Sw | /s | Voluntary + involuntary context switches per second | High values (>100K/s) may indicate lock contention or too many active connections. |

### PostgreSQL Metrics

#### PostgreSQL

| Key | Label | Unit | Description | Interpretation |
|-----|-------|------|-------------|----------------|
| tps | TPS | /s | Transactions committed + rolled back per second | Primary throughput metric. Establish baseline, alert on deviation. |
| hit_ratio_pct | Hit Ratio | % | shared_buffers cache hit ratio across all databases | <99% for OLTP is concerning. Check PGT I/O view for tables missing cache. |
| tuples_s | Tuples | /s | Total tuples returned + fetched + inserted + updated + deleted per second | Overall database activity. |
| temp_bytes_s | Temp | bytes/s | Temporary file bytes written per second | Any temp file usage means work_mem is too small for some queries. Check PGS for culprits. |
| deadlocks | Deadlocks | count | Deadlocks detected (rate per interval) | Any value >0 needs investigation. Check application locking order. |

#### Background Writer

| Key | Label | Unit | Description | Interpretation |
|-----|-------|------|-------------|----------------|
| checkpoints_per_min | Ckpt/min | /min | Checkpoints completed per minute | Too frequent = small checkpoint_timeout or checkpoint_completion_target. Too rare = recovery takes longer. |
| checkpoint_write_time_ms | Ckpt Write | ms | Time spent writing checkpoint data | Long write times = I/O bottleneck during checkpoints. |
| buffers_backend_s | BE Bufs | /s | Buffers written directly by backends (not by bgwriter/checkpointer) | Should be near 0. High values mean bgwriter/checkpointer can't keep up. |
| buffers_clean_s | Clean | /s | Buffers cleaned by background writer per second | Background writer activity. |
| maxwritten_clean | MaxWritten | count | Times bgwriter stopped cleaning because it hit bgwriter_lru_maxpages | High values mean bgwriter_lru_maxpages is too low. |
| buffers_alloc_s | Alloc | /s | New buffer allocations per second | High values mean working set exceeds shared_buffers. |

---

## PRC -- OS Processes

**Source:** Linux `/proc/[pid]/stat`, `/proc/[pid]/status`, `/proc/[pid]/io`
**Entity ID:** `pid` (OS process ID)
**Drill-down:** PRC -> PGA (navigate to PostgreSQL session for this PID)

Shows all OS processes on the system, with special enrichment for PostgreSQL backends (shows current query, backend type).

### Views

| View | Default Sort | Columns | Purpose |
|------|-------------|---------|---------|
| **Generic** | cpu_pct DESC | pid, name, state, cpu_pct, mem_pct, vgrow_kb, rgrow_kb, uid, euid, num_threads, curcpu, cmdline | General overview of process activity |
| **Command** | cpu_pct DESC | pid, name, ppid, state, cpu_pct, mem_pct, cmdline | Process hierarchy and command lines |
| **Memory** | mem_pct DESC | pid, name, mem_pct, vsize_kb, rsize_kb, psize_kb, vgrow_kb, rgrow_kb, vswap_kb, vstext_kb, vdata_kb, vstack_kb, vslibs_kb, vlock_kb, minflt, majflt | Memory usage breakdown |
| **Disk** | read_bytes_s DESC | pid, name, read_bytes_s, write_bytes_s, read_ops_s, write_ops_s, cancelled_write_bytes, cmdline | Disk I/O activity |
| **Scheduler** | cpu_pct DESC | pid, name, cpu_pct, curcpu, nice, priority, rtprio, policy, blkdelay, nvcsw_s, nivcsw_s | CPU scheduling details |

### Columns

| Key | Label | Unit/Format | Description | When to investigate |
|-----|-------|-------------|-------------|---------------------|
| pid | PID | integer | OS process identifier | - |
| ppid | PPID | integer | Parent process ID | Useful for understanding process hierarchy. PostgreSQL backends have postmaster as parent. |
| name | Name | string | Process name from `/proc/pid/comm` | PostgreSQL backends show as "postgres". |
| cmdline | Command | string | Full command line | For PG backends, shows connection details (database, user). |
| state | State | string | R=running, S=sleeping, D=disk wait, Z=zombie, T=stopped | D (uninterruptible sleep) = waiting for disk I/O. Many D-state processes = I/O bottleneck. Z (zombie) = parent not reaping child. |
| cpu_pct | CPU% | percent | CPU utilization since last sample | >100% possible on multi-core (one core = 100%). Single query >100% = parallel query execution. |
| mem_pct | MEM% | percent | Resident memory as percentage of total RAM | High for single PG backend = large work_mem operation or memory leak. |
| vsize_kb | VIRT | KB -> bytes | Total virtual address space | Includes shared libraries, mapped files. Usually not a concern. |
| rsize_kb | RES | KB -> bytes | Resident set size (actual physical memory) | Real memory usage. For PG backends: high values indicate large sorts, hash joins, or maintenance operations. |
| psize_kb | PSS | KB -> bytes | Proportional set size (shared pages divided equally among sharers) | More accurate than RSS for shared memory (shared_buffers). |
| vgrow_kb | VGROW | KB -> bytes | Virtual memory growth since last sample | Sudden growth = memory allocation. |
| rgrow_kb | RGROW | KB -> bytes | Resident memory growth since last sample | Growing RSS = process is actively using more memory. |
| vswap_kb | SWAP | KB -> bytes | Swapped out memory | PostgreSQL backends with swap >0 will have poor performance. |
| vstext_kb | Code | KB -> bytes | Code (text) segment size | Constant per binary. |
| vdata_kb | Data | KB -> bytes | Data + stack segments | Growing = heap allocations. |
| vstack_kb | Stack | KB -> bytes | Stack size | Usually <10MB. Growing = deep recursion. |
| vslibs_kb | Libs | KB -> bytes | Shared libraries memory | Usually constant. |
| vlock_kb | Lock | KB -> bytes | Locked (mlock) memory | Memory that cannot be swapped out. |
| read_bytes_s | Read/s | bytes/s | Disk read throughput | Physical I/O by this process. For PG: seq scans on uncached data, sorts spilling to disk. |
| write_bytes_s | Write/s | bytes/s | Disk write throughput | For PG: WAL writes, checkpoint writes, temp files. |
| read_ops_s | RdOps/s | /s | Read operations per second | Number of I/O operations (not bytes). |
| write_ops_s | WrOps/s | /s | Write operations per second | - |
| total_read_bytes | Total Read | bytes | Cumulative bytes read | Lifetime total since process start. |
| total_write_bytes | Total Write | bytes | Cumulative bytes written | Lifetime total since process start. |
| total_read_ops | Total RdOps | count | Cumulative read operations | - |
| total_write_ops | Total WrOps | count | Cumulative write operations | - |
| cancelled_write_bytes | Cancelled | bytes | Bytes of cancelled writes (truncated files) | Temp files deleted before flushed. |
| uid | UID | integer | Real user ID | PostgreSQL backends run as `postgres` user. |
| euid | EUID | integer | Effective user ID | Usually equals UID. |
| gid | GID | integer | Real group ID | - |
| egid | EGID | integer | Effective group ID | - |
| num_threads | Threads | integer | Thread count | PG backends are single-threaded (1). Parallel queries create workers (separate PIDs). |
| curcpu | CPU# | integer | CPU core currently executing on | Useful for NUMA analysis. |
| nice | Nice | integer | Nice value (-20 to 19) | PG backends usually run at 0 (normal priority). |
| priority | Priority | integer | Kernel scheduling priority | Lower = higher priority. |
| rtprio | RT Prio | integer | Real-time priority | 0 = not real-time. PG should not use RT scheduling. |
| policy | Policy | integer | Scheduling policy: 0=SCHED_OTHER, 1=SCHED_FIFO, 2=SCHED_RR | PG uses 0 (normal). |
| blkdelay | BlkDelay | ticks | Cumulative block I/O delay | High = process spent significant time waiting for disk. |
| nvcsw_s | VCtx/s | /s | Voluntary context switches per second | Process yielded CPU (I/O wait, sleep). High values = I/O-bound. |
| nivcsw_s | ICtx/s | /s | Involuntary context switches per second | Kernel preempted process. High values = CPU contention. |
| minflt | MinFlt | count | Minor page faults (no disk I/O needed) | Normal, caused by first-touch memory access. |
| majflt | MajFlt | count | Major page faults (required disk I/O) | Bad for performance. Indicates swapping or cold start. |
| tty | TTY | integer | Terminal device number | 0 = no terminal (daemon). |
| exit_signal | Exit Sig | integer | Signal sent to parent on exit | Usually SIGCHLD (17). |
| pg_query | PG Query | string | Current SQL query (for PostgreSQL backends) | Enriched from pg_stat_activity join by PID. |
| pg_backend_type | PG Backend | string | PostgreSQL backend type | client backend, autovacuum worker, walwriter, checkpointer, etc. |

### Detail Panel Sections

| Section | Fields | Description |
|---------|--------|-------------|
| Identity | pid, ppid, name, state, tty, btime, num_threads, exit_signal | Basic process identification |
| User/Group | uid, euid, gid, egid | Process ownership |
| CPU | cpu_pct, utime, stime, curcpu, rundelay, nice, priority, rtprio, policy, blkdelay, nvcsw_s, nivcsw_s | CPU scheduling and usage |
| Memory | mem_pct, vsize_kb, rsize_kb, psize_kb, vgrow_kb, rgrow_kb, vstext_kb, vdata_kb, vstack_kb, vslibs_kb, vlock_kb, vswap_kb, minflt, majflt | Full memory breakdown |
| Disk I/O | read_bytes_s, write_bytes_s, read_ops_s, write_ops_s, total_read_bytes, total_write_bytes, total_read_ops, total_write_ops, cancelled_write_bytes | Disk I/O rates and cumulative |
| PostgreSQL | pg_backend_type, pg_query | PG enrichment (only for PG backends) |
| Command | cmdline | Full command line (copyable) |

---

## PGA -- pg_stat_activity

**Source:** `pg_stat_activity` system view
**Entity ID:** `pid` (backend process ID)
**Drill-down:** PGA -> PGS (navigate to statement statistics for this query)

Shows currently active PostgreSQL sessions/backends. This is the primary tab for diagnosing active queries, blocked sessions, and connection usage.

### Views

| View | Default Sort | Columns | Purpose |
|------|-------------|---------|---------|
| **Generic** | query_duration_s DESC | pid, cpu_pct, rss_kb, database, user, state, wait_event_type, wait_event, query_duration_s, xact_duration_s, backend_duration_s, backend_type, query | Comprehensive session overview |
| **Stats** | query_duration_s DESC | pid, database, user, state, query_duration_s, stmt_mean_exec_time_ms, stmt_max_exec_time_ms, stmt_calls_s, stmt_hit_pct, query | Session + statement statistics join |

### Columns

| Key | Label | Unit/Format | Description | When to investigate |
|-----|-------|-------------|-------------|---------------------|
| pid | PID | integer | Backend process ID | Use for `pg_cancel_backend(pid)` or `pg_terminate_backend(pid)`. |
| database | Database | string | Connected database name | Filter by database to focus investigation. |
| user | User | string | Connected user name | Identify application vs admin connections. |
| application_name | App Name | string | Application name set by client (via `application_name` connection parameter) | Helps identify which microservice/tool is connected. |
| client_addr | Client | string | Client IP address | Identify source of connections. NULL = local socket. |
| backend_type | Backend | string | Type: client backend, autovacuum worker, background worker, etc. | Filter out system backends to focus on user queries. |
| state | State | string | idle, active, idle in transaction, idle in transaction (aborted), fastpath function call, disabled | **"idle in transaction"** = holding locks without doing work. Dangerous for long periods. **"active"** = currently executing query. |
| wait_event_type | Wait Type | string | Lock, LWLock, IO, BufferPin, Activity, Client, Extension, IPC, Timeout | Identifies what category of resource the backend is waiting for. |
| wait_event | Wait Event | string | Specific wait event name (e.g., WALWrite, DataFileRead, transactionid) | Combined with wait_event_type, tells you exactly what the backend is stuck on. See PostgreSQL docs for full list. |
| query | Query | string | Current or last executed SQL statement | The actual query text. Truncated in table, full text in detail panel. |
| query_id | Query ID | integer | Hash linking to pg_stat_statements | Use drill-down to see historical statistics for this query pattern. |
| query_duration_s | Q Dur | seconds -> duration | Time since current query started | **>1s** for OLTP = investigate. **>30s** = likely problem. Compare with stmt_mean to see if this execution is abnormal. |
| xact_duration_s | Tx Dur | seconds -> duration | Time since current transaction started | Long transactions hold locks and prevent vacuum. **>60s** = investigate. **>5min** = likely problem. |
| backend_duration_s | BE Dur | seconds -> duration | Time since backend connected | Very old connections may indicate connection pool issues. |
| backend_start | BE Start | epoch -> age | When the backend process was started | - |
| xact_start | Tx Start | epoch -> age | When the current transaction began | - |
| query_start | Q Start | epoch -> age | When the current query began | - |
| cpu_pct | CPU% | percent | OS process CPU utilization | Enriched from /proc. High CPU = heavy computation (hash joins, sorts, aggregation). |
| rss_kb | RSS | KB -> bytes | OS process resident memory | Enriched from /proc. High RSS = large work_mem usage, maintenance_work_mem for VACUUM. |
| stmt_mean_exec_time_ms | Avg Time | ms | Mean execution time from pg_stat_statements | Enriched via query_id join. Compare with current query_duration_s to detect anomalies. |
| stmt_max_exec_time_ms | Max Time | ms | Maximum observed execution time | Historical worst case. If current duration exceeds this, something unusual is happening. |
| stmt_calls_s | Calls/s | /s | Execution rate from pg_stat_statements | High calls/s = hot query. Even small improvements yield big impact. |
| stmt_hit_pct | Hit% | percent | Buffer cache hit ratio from pg_stat_statements | <99% = this query pattern regularly does physical I/O. Consider adding indexes or increasing shared_buffers. |

### Detail Panel Sections

| Section | Fields | Description |
|---------|--------|-------------|
| Session | pid, database, user, application_name, client_addr, backend_type | Connection identification |
| Timing | backend_start, xact_start, query_start, query_duration_s, xact_duration_s, backend_duration_s | Duration breakdown |
| State | state, wait_event_type, wait_event | Current backend state |
| OS Process | cpu_pct, rss_kb | OS-level resource usage |
| Statements | stmt_mean_exec_time_ms, stmt_max_exec_time_ms, stmt_calls_s, stmt_hit_pct | Historical stats for this query pattern |
| Query | query | Full query text (copyable) |

---

## PGS -- pg_stat_statements

**Source:** `pg_stat_statements` extension (must be enabled in `shared_preload_libraries`)
**Entity ID:** `queryid` (statement hash)
**No drill-down.** (Incoming drill-down from PGA.)

Aggregated query statistics. Shows performance metrics per normalized query pattern (with parameters replaced by `$N`). This is the primary tab for query performance analysis and optimization.

### Views

| View | Default Sort | Columns | Purpose |
|------|-------------|---------|---------|
| **Calls** | calls_s DESC | calls_s, rows_s, rows_per_call, mean_exec_time_ms, database, user, query | Find most frequently executed queries |
| **Time** (default) | exec_time_ms_s DESC | calls_s, rows_s, mean_exec_time_ms, min_exec_time_ms, max_exec_time_ms, stddev_exec_time_ms, database, user, query | Find queries consuming most execution time |
| **I/O** | shared_blks_read_s DESC | calls_s, shared_blks_read_s, shared_blks_hit_s, hit_pct, shared_blks_dirtied_s, shared_blks_written_s, database, query | Find queries doing most physical I/O |
| **Temp** | temp_mb_s DESC | calls_s, temp_blks_read_s, temp_blks_written_s, temp_mb_s, local_blks_read_s, local_blks_written_s, database, query | Find queries using temp files (work_mem overflow) |

### Columns

| Key | Label | Unit/Format | Description | When to investigate |
|-----|-------|-------------|-------------|---------------------|
| queryid | Query ID | integer | Hash of the normalized query text | Uniquely identifies a query pattern. |
| database | Database | string | Database where this query executes | - |
| user | User | string | User executing the query | - |
| calls | Calls | integer | Total number of executions (cumulative since stats reset) | Context for rates. |
| rows | Rows | integer | Total rows returned (cumulative) | - |
| calls_s | Calls/s | /s (rate) | Executions per second | High frequency queries have biggest optimization impact. |
| rows_s | Rows/s | /s (rate) | Rows returned per second | Large result sets may indicate missing WHERE clauses. |
| rows_per_call | R/Call | number | Average rows returned per execution (rows/calls) | High values = query returns too many rows per call. Consider pagination. |
| mean_exec_time_ms | Avg | ms | Average execution time per call | Primary optimization target. Compare across time. |
| min_exec_time_ms | Min | ms | Best observed execution time | Indicates best-case performance (data in cache, no contention). |
| max_exec_time_ms | Max | ms | Worst observed execution time | If max >> mean, query has high variance. May be affected by lock contention or cache misses. |
| stddev_exec_time_ms | Stddev | ms | Standard deviation of execution time | High stddev = inconsistent performance. May indicate plan instability. |
| exec_time_ms_s | Time/s | ms/s (rate) | Total execution time consumed per second | Product of calls_s x mean. Queries ranking high here consume the most CPU. |
| total_exec_time | Total Time | ms | Cumulative total execution time | Historical reference. |
| total_plan_time | Plan Time | ms | Cumulative total planning time | If high relative to exec time, consider using prepared statements. |
| shared_blks_read_s | Blk Rd/s | blks/s (rate) | Shared buffer blocks read from disk per second | Physical I/O. Each block = 8 KB. High values = working set exceeds shared_buffers. |
| shared_blks_hit_s | Blk Hit/s | blks/s (rate) | Shared buffer blocks found in cache per second | Cache hits. Good if hit_pct is high. |
| hit_pct | HIT% | percent | shared_blks_hit / (hit + read) * 100 | **>=99%** = good (green). **90-99%** = elevated physical I/O (yellow). **<90%** = major cache misses (red). |
| shared_blks_dirtied_s | Dirty/s | blks/s (rate) | Shared buffer blocks dirtied (modified) per second | High = write-heavy query. Increases checkpoint pressure. |
| shared_blks_written_s | Written/s | blks/s (rate) | Shared buffer blocks written by this statement per second | Backend writes (not bgwriter). Should be 0 in well-tuned systems. |
| local_blks_read_s | Local Rd/s | blks/s (rate) | Local buffer blocks read per second | Temporary tables. High values may indicate opportunity for regular tables. |
| local_blks_written_s | Local Wr/s | blks/s (rate) | Local buffer blocks written per second | - |
| temp_blks_read_s | Temp Rd/s | blks/s (rate) | Temp file blocks read per second | Sorts/hash joins spilling to disk. Increase work_mem for this query. |
| temp_blks_written_s | Temp Wr/s | blks/s (rate) | Temp file blocks written per second | - |
| temp_mb_s | Temp MB/s | MB/s | Temp file throughput in megabytes per second | Overall temp file I/O. Significant performance impact. |
| wal_records | WAL Records | integer | WAL records generated (cumulative) | High values = write-heavy query. |
| wal_bytes | WAL Bytes | integer | WAL bytes generated (cumulative) | Indicates replication and recovery impact. |
| query | Query | string | Normalized query text (parameters replaced with $1, $2, ...) | The actual SQL pattern. |

### Detail Panel Sections

| Section | Fields | Description |
|---------|--------|-------------|
| Rates | calls_s, rows_s, exec_time_ms_s, hit_pct | Key per-second rates |
| Identity | queryid, database, user, calls, rows, rows_per_call | Statement identification and cumulative counts |
| Timing | total_exec_time, mean_exec_time_ms, min_exec_time_ms, max_exec_time_ms, stddev_exec_time_ms, total_plan_time | Execution time breakdown |
| I/O | shared_blks_read_s, shared_blks_hit_s, hit_pct, shared_blks_dirtied_s, shared_blks_written_s, local_blks_read_s, local_blks_written_s | Buffer I/O rates |
| Temp/WAL | temp_blks_read_s, temp_blks_written_s, temp_mb_s, wal_records, wal_bytes | Temp files and WAL generation |
| Query | query | Full normalized query text (copyable) |

---

## PGT -- pg_stat_user_tables

**Source:** `pg_stat_user_tables` + `pg_statio_user_tables` system views
**Entity ID:** `relid` (table OID)
**Drill-down:** PGT -> PGI (navigate to indexes for this table)

Table-level statistics showing scan patterns, write activity, maintenance status, and I/O. Essential for identifying hot tables, missing indexes, and vacuum problems.

### Views

| View | Default Sort | Columns | Purpose |
|------|-------------|---------|---------|
| **Reads** | tot_tup_read_s DESC | seq_tup_read_s, idx_tup_fetch_s, tot_tup_read_s, seq_scan_s, idx_scan_s, io_hit_pct, disk_blks_read_s, size_bytes, display_name | Tables with most read activity |
| **Writes** | n_tup_ins_s DESC | n_tup_ins_s, n_tup_upd_s, n_tup_del_s, n_tup_hot_upd_s, n_live_tup, n_dead_tup, io_hit_pct, disk_blks_read_s, size_bytes, display_name | Tables with most write activity |
| **Scans** | seq_pct DESC | seq_scan_s, seq_tup_read_s, idx_scan_s, idx_tup_fetch_s, seq_pct, io_hit_pct, disk_blks_read_s, size_bytes, display_name | Sequential vs index scan ratio |
| **Maintenance** | dead_pct DESC | n_dead_tup, n_live_tup, dead_pct, vacuum_count_s, autovacuum_count_s, last_autovacuum, last_autoanalyze, display_name | Vacuum and analyze status |
| **I/O** (default) | heap_blks_read_s DESC | heap_blks_read_s, heap_blks_hit_s, idx_blks_read_s, idx_blks_hit_s, io_hit_pct, disk_blks_read_s, size_bytes, display_name | Physical I/O by table |

### Columns

| Key | Label | Unit/Format | Description | When to investigate |
|-----|-------|-------------|-------------|---------------------|
| relid | OID | integer | Table object identifier | Internal PostgreSQL OID. |
| schema | Schema | string | Schema name | Filter by schema to focus on specific application tables. |
| table | Table | string | Table name | - |
| display_name | Table Name | string | Schema-qualified name (e.g., "public.users" or just "users" if public) | Displayed in views. |
| size_bytes | Size | bytes | Total table size including indexes and TOAST | Large tables are more likely to have I/O problems. |
| n_live_tup | Live | integer | Estimated live (visible) rows | - |
| n_dead_tup | Dead | integer | Estimated dead (deleted/updated but not vacuumed) rows | **>100K** = vacuum may be falling behind (red). Dead rows consume space and slow down scans. |
| dead_pct | DEAD% | percent | n_dead_tup / (n_live_tup + n_dead_tup) * 100 | **>20%** = critical, VACUUM urgently needed. **5-20%** = warning. **0** = clean. |
| seq_scan_s | Seq/s | /s (rate) | Sequential scans initiated per second | High rate on large tables = missing index. Each seq scan reads all live rows. |
| seq_tup_read_s | Seq Rd/s | /s (rate) | Rows read by sequential scans per second | Product of seq_scan_s x n_live_tup. The real cost metric. |
| idx_scan_s | Idx/s | /s (rate) | Index scans per second | Index usage rate. |
| idx_tup_fetch_s | Idx Ft/s | /s (rate) | Rows fetched via index scans per second | Effective index reads. |
| tot_tup_read_s | Tot Rd/s | /s (rate) | Total rows read per second (sequential + index) | Overall table read activity. |
| seq_pct | SEQ% | percent | seq_scan / (seq_scan + idx_scan) * 100 | **>80%** = critical, mostly sequential scans (red). **30-80%** = mixed (yellow). Good tables are <10%. Exception: small lookup tables are fine with seq scans. |
| n_tup_ins_s | Ins/s | /s (rate) | Rows inserted per second | Write rate. |
| n_tup_upd_s | Upd/s | /s (rate) | Rows updated per second | High update rate = more dead tuples, more vacuum work. |
| n_tup_del_s | Del/s | /s (rate) | Rows deleted per second | Creates dead tuples. |
| n_tup_hot_upd_s | HOT/s | /s (rate) | HOT (Heap-Only Tuple) updates per second | HOT updates don't require index updates. Higher ratio = better. |
| hot_pct | HOT% | percent | HOT updates / total updates * 100 | High is good. Low HOT% on frequently updated tables suggests fillfactor tuning. |
| io_hit_pct | HIT% | percent | (all cache hits) / (all cache hits + all disk reads) * 100 | **>=99%** = good, table fits in cache (green). **<90%** = most I/O goes to disk (red). |
| disk_blks_read_s | DISK/s | blks/s (rate) | Total disk blocks read per second (heap + index) | Physical I/O rate. Any value >0 shown as warning. |
| heap_blks_read_s | Heap Rd/s | blks/s (rate) | Heap (table data) blocks read from disk per second | Physical reads of table data. |
| heap_blks_hit_s | Heap Hit/s | blks/s (rate) | Heap blocks found in shared_buffers per second | Cache hits for table data. |
| idx_blks_read_s | Idx Rd/s | blks/s (rate) | Index blocks read from disk per second | Physical reads of indexes. |
| idx_blks_hit_s | Idx Hit/s | blks/s (rate) | Index blocks found in shared_buffers per second | Cache hits for indexes. |
| toast_blks_read_s | Toast Rd/s | blks/s (rate) | TOAST blocks read from disk per second | TOAST stores large values (>2KB). Physical TOAST reads = large columns fetched from disk. |
| toast_blks_hit_s | Toast Hit/s | blks/s (rate) | TOAST blocks found in shared_buffers per second | - |
| tidx_blks_read_s | TIdx Rd/s | blks/s (rate) | TOAST index blocks read from disk per second | - |
| tidx_blks_hit_s | TIdx Hit/s | blks/s (rate) | TOAST index blocks found in shared_buffers per second | - |
| vacuum_count_s | Vac/s | /s (rate) | Manual VACUUM operations per second | - |
| autovacuum_count_s | AVac/s | /s (rate) | Automatic VACUUM operations per second | - |
| analyze_count_s | Anl/s | /s (rate) | Manual ANALYZE operations per second | - |
| autoanalyze_count_s | AAnl/s | /s (rate) | Automatic ANALYZE operations per second | - |
| last_vacuum | Last Vacuum | epoch -> age | Time since last manual VACUUM | - |
| last_autovacuum | Last AVac | epoch -> age | Time since last autovacuum | Long time since last autovacuum + high dead_pct = vacuum is stuck or disabled for this table. |
| last_analyze | Last Analyze | epoch -> age | Time since last manual ANALYZE | - |
| last_autoanalyze | Last AAnl | epoch -> age | Time since last autoanalyze | Stale statistics lead to bad query plans. |

### Detail Panel Sections

| Section | Fields | Description |
|---------|--------|-------------|
| Identity | relid, schema, table, display_name, size_bytes | Table identification |
| Scan Activity | seq_scan_s, seq_tup_read_s, idx_scan_s, idx_tup_fetch_s, tot_tup_read_s, seq_pct | Read patterns |
| Write Activity | n_tup_ins_s, n_tup_upd_s, n_tup_del_s, n_tup_hot_upd_s, hot_pct | Write patterns |
| Tuples | n_live_tup, n_dead_tup, dead_pct | Row counts and bloat |
| Maintenance | vacuum_count_s, autovacuum_count_s, analyze_count_s, autoanalyze_count_s, last_vacuum, last_autovacuum, last_analyze, last_autoanalyze | Vacuum and analyze activity |
| I/O | heap_blks_read_s, heap_blks_hit_s, idx_blks_read_s, idx_blks_hit_s, toast_blks_read_s, toast_blks_hit_s, tidx_blks_read_s, tidx_blks_hit_s, io_hit_pct, disk_blks_read_s | Full I/O breakdown |

---

## PGI -- pg_stat_user_indexes

**Source:** `pg_stat_user_indexes` + `pg_statio_user_indexes` system views
**Entity ID:** `indexrelid` (index OID)
**No drill-down.** (Incoming drill-down from PGT.)

Index-level statistics. Shows which indexes are actively used, which are unused (candidates for removal), and I/O patterns.

### Views

| View | Default Sort | Columns | Purpose |
|------|-------------|---------|---------|
| **Usage** | idx_tup_read_s DESC | idx_scan_s, idx_tup_read_s, idx_tup_fetch_s, io_hit_pct, disk_blks_read_s, size_bytes, display_table, index | Active index usage |
| **Unused** | idx_scan ASC | idx_scan, size_bytes, display_table, index | Find unused indexes (candidates for DROP) |
| **I/O** (default) | idx_blks_read_s DESC | idx_blks_read_s, idx_blks_hit_s, io_hit_pct, disk_blks_read_s, size_bytes, display_table, index | Index physical I/O |

### Columns

| Key | Label | Unit/Format | Description | When to investigate |
|-----|-------|-------------|-------------|---------------------|
| indexrelid | OID | integer | Index object identifier | - |
| relid | Table OID | integer | Parent table OID | Links back to PGT. |
| schema | Schema | string | Schema name | - |
| table | Table | string | Parent table name | - |
| index | Index | string | Index name | - |
| display_table | Table Name | string | Schema-qualified table name | - |
| idx_scan | Idx Scans | integer | Total index scans since stats reset (cumulative) | **0 scans** = unused index. Consumes disk space and slows writes. Consider DROP INDEX. |
| size_bytes | Size | bytes | Index size on disk | Large unused indexes waste space and slow VACUUM, INSERT, UPDATE. |
| idx_scan_s | Idx/s | /s (rate) | Index scans per second | Current usage rate. |
| idx_tup_read_s | Tup Rd/s | /s (rate) | Index entries read per second | Entries traversed in the index. |
| idx_tup_fetch_s | Tup Ft/s | /s (rate) | Rows fetched from table via this index per second | Actual rows retrieved. If much less than idx_tup_read_s, index may be suboptimal (many dead tuples in index). |
| idx_blks_read_s | Blk Rd/s | blks/s (rate) | Index blocks read from disk per second | Physical I/O on the index. |
| idx_blks_hit_s | Blk Hit/s | blks/s (rate) | Index blocks found in shared_buffers per second | Cache hits. |
| io_hit_pct | HIT% | percent | Cache hit ratio for this index | Same thresholds as PGT HIT%. |
| disk_blks_read_s | DISK/s | blks/s (rate) | Disk blocks read per second (= idx_blks_read_s) | Alias for consistency with PGT. |

### Detail Panel Sections

| Section | Fields | Description |
|---------|--------|-------------|
| Identity | indexrelid, relid, schema, table, index, display_table, size_bytes | Index identification |
| Usage | idx_scan, idx_scan_s, idx_tup_read_s, idx_tup_fetch_s | Scan counts and rates |
| I/O | idx_blks_read_s, idx_blks_hit_s, io_hit_pct, disk_blks_read_s | Physical I/O |

---

## PGL -- pg_locks

**Source:** `pg_locks` + `pg_stat_activity` joined, organized as a blocking tree
**Entity ID:** `pid` (backend process ID)
**Drill-down:** PGL -> PGA (navigate to session details for this PID)

Lock blocking tree. Visualizes which sessions are blocking which other sessions. Only shows sessions involved in blocking chains (not all locks). Tree structure shown via depth indentation in the PID column.

**Sorting is disabled** for this tab because the tree order (parent -> children) must be preserved.

### Views

| View | Default Sort | Columns | Purpose |
|------|-------------|---------|---------|
| **Lock Tree** | (none, tree order) | pid, depth, state, wait_event_type, wait_event, lock_mode, lock_target, query | Blocking chain visualization |

### Columns

| Key | Label | Unit/Format | Description | When to investigate |
|-----|-------|-------------|-------------|---------------------|
| pid | PID | integer | Backend process ID. Displayed with dot-indentation per depth level. | Root blockers (depth=1) need immediate attention. |
| depth | Depth | integer | Position in blocking tree. 1 = root blocker, 2+ = blocked by parent. | Root blockers cause the most impact. |
| root_pid | Root PID | integer | PID of the ultimate root blocker | All sessions with same root_pid are part of one blocking chain. |
| database | Database | string | Database name | - |
| user | User | string | User name | - |
| application_name | App Name | string | Application name | Helps identify which service is blocking. |
| state | State | string | Backend state (active, idle in transaction, etc.) | "idle in transaction" blockers = application forgot to COMMIT/ROLLBACK. |
| wait_event_type | Wait Type | string | What type of wait: Lock, LWLock, IO, etc. | Blocked sessions show "Lock" here. |
| wait_event | Wait Event | string | Specific lock type being waited on (e.g., transactionid, relation, tuple) | "transactionid" = waiting for another transaction to complete. "relation" = DDL lock. |
| backend_type | Backend | string | Backend type | - |
| lock_type | Lock Type | string | Lock category: relation, transactionid, tuple, virtualxid, etc. | "relation" = table-level lock. "transactionid" = row-level lock conflict. |
| lock_mode | Lock Mode | string | Lock mode: AccessShareLock, RowShareLock, RowExclusiveLock, ShareLock, ShareRowExclusiveLock, ExclusiveLock, AccessExclusiveLock | AccessExclusiveLock = DDL, blocks everything. RowExclusiveLock = normal DML (INSERT/UPDATE/DELETE). |
| lock_target | Target | string | What is locked: table name, transaction ID, tuple position | Identifies the specific object being contended. |
| lock_granted | Granted | boolean | Whether the lock is held (true) or being waited for (false) | **false** (red) = this session is blocked, waiting for the lock. **true** = this session holds the lock and may be blocking others. |
| query | Query | string | Current or last query | For root blockers: shows what operation is holding the lock. |
| xact_start | Xact Start | epoch -> age | Transaction start time | Long-running transactions on root blockers = major issue. |
| query_start | Query Start | epoch -> age | Query start time | - |
| state_change | State Change | epoch -> age | Last state change | - |

### Detail Panel Sections

| Section | Fields | Description |
|---------|--------|-------------|
| Identity | pid, depth, root_pid, database, user, application_name, backend_type | Session and tree position |
| Lock | lock_type, lock_mode, lock_granted, lock_target | Lock details |
| Timing | xact_start, query_start, state_change | Duration information |
| State | state, wait_event_type, wait_event | Backend state |
| Query | query | Full query text (copyable) |

---

## Threshold Coloring

The UI applies color coding to cell values to highlight noteworthy conditions. Colors adapt to the active theme.

| Level | Visual | Meaning |
|-------|--------|---------|
| **critical** | Red | Requires immediate attention |
| **warning** | Yellow/Amber | Elevated, worth monitoring |
| **good** | Green | Healthy/normal |
| **inactive** | Gray (dimmed) | Zero or no activity |
| **default** | Normal text | No special coloring |

### Threshold Rules

| Metric | Inactive | Normal | Warning | Critical |
|--------|----------|--------|---------|----------|
| cpu_pct | =0 | <50% | 50-90% | >90% |
| mem_pct | =0 | <70% | 70-90% | >90% |
| io_hit_pct | - | >=99% (green) | 90-99% | <90% |
| hit_pct | - | >=99% (green) | 90-99% | <90% |
| stmt_hit_pct | - | >=99% (green) | 90-99% | <90% |
| dead_pct | =0 | <5% | 5-20% | >20% |
| seq_pct | =0 | <30% | 30-80% | >80% |
| disk_blks_read_s | =0 | - | >0 (any reads) | - |
| query_duration_s | =0 | <1s | 1-30s | >30s |
| xact_duration_s | =0 | <5s | 5-60s | >60s |
| wait_event_type | empty | - | non-empty (any wait) | - |
| state (PGA) | - | - | "idle in transaction" | "idle in transaction (aborted)" |
| lock_granted | - | true | - | false (waiting) |
| n_dead_tup | =0 | <1K | 1K-100K | >100K |
| All rate metrics (*_s) | =0 (gray) | >0 (normal) | - | - |

---

## Glossary

| Term | Description |
|------|-------------|
| **shared_buffers** | PostgreSQL's main memory cache for table and index data. Configured in `postgresql.conf`. Typically 25% of RAM. |
| **work_mem** | Memory available per query operation (sort, hash join). Queries exceeding this spill to temp files on disk. |
| **WAL (Write-Ahead Log)** | Transaction log ensuring durability. Every data change is first written to WAL before being applied to data files. |
| **VACUUM** | PostgreSQL maintenance process that removes dead tuples (rows from deleted/updated records). Without VACUUM, tables bloat and queries slow down. |
| **autovacuum** | Background process that automatically runs VACUUM and ANALYZE. Should be kept running and properly tuned. |
| **ANALYZE** | Collects table statistics for the query planner. Without fresh statistics, PostgreSQL may choose bad query plans. |
| **HOT update** | Heap-Only Tuple update — an optimization where the update doesn't require modifying any index (the new tuple version stays on the same page). Reduces I/O significantly. |
| **TOAST** | The Oversized-Attribute Storage Technique. PostgreSQL stores large column values (>2KB) in a separate TOAST table. TOAST I/O is tracked separately. |
| **Sequential scan (seq scan)** | Reading all rows of a table in physical order. Efficient for small tables or queries that need most rows. Inefficient for selective queries on large tables — consider adding an index. |
| **Index scan** | Using a B-tree (or other) index to find specific rows. Much faster than seq scan for selective queries. |
| **Buffer cache hit ratio** | Percentage of data pages found in shared_buffers (cache) vs. read from disk. Higher is better. <99% for OLTP workloads suggests shared_buffers may be too small or working set too large. |
| **Dead tuples** | Row versions left behind by UPDATE and DELETE operations. Occupy space until VACUUM removes them. High dead tuple counts slow down sequential scans. |
| **Blocking chain** | When session A holds a lock that session B needs, and session B holds a lock that session C needs, this forms a chain: A -> B -> C. Session A is the "root blocker." |
| **PSI (Pressure Stall Information)** | Linux kernel metric measuring actual resource contention (CPU, memory, I/O). Unlike utilization, PSI tells you whether processes are actually waiting. |
| **TPS (Transactions Per Second)** | Number of completed transactions per second. Primary PostgreSQL throughput metric. |
| **bgwriter** | Background writer process. Writes dirty shared buffers to disk to avoid backends doing it. `buffers_backend_s > 0` means bgwriter isn't keeping up. |
| **Checkpoint** | Periodic process that writes all dirty buffers to disk and creates a recovery point. Configured by `checkpoint_timeout` and `max_wal_size`. |
| **OID** | Object Identifier — internal PostgreSQL integer ID for database objects (tables, indexes, etc.). |
| **Normalized query** | Query text with literal values replaced by parameter placeholders ($1, $2, ...). Used by pg_stat_statements to group similar queries. |
| **Rate metrics** | Values computed as (current - previous) / time_delta. Shown as "/s" (per second). Require at least two samples to compute. |
| **Cumulative metrics** | Values that only increase over time (counters). Reset to 0 on pg_stat_statements_reset() or server restart. |
