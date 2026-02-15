import type { ReactNode } from "react";

interface ColumnHelpEntry {
  label: string;
  description: string;
  thresholds?: string;
  tip?: string;
  /** Link to PostgreSQL documentation */
  docUrl?: string;
}

// PostgreSQL documentation base URLs
const PG_DOCS = "https://www.postgresql.org/docs/current";
const PG_STAT_ACTIVITY = `${PG_DOCS}/monitoring-stats.html#MONITORING-PG-STAT-ACTIVITY-VIEW`;
const PG_STAT_STATEMENTS = `${PG_DOCS}/pgstatstatements.html`;
const PG_STAT_USER_TABLES = `${PG_DOCS}/monitoring-stats.html#MONITORING-PG-STAT-ALL-TABLES-VIEW`;
const PG_STAT_USER_INDEXES = `${PG_DOCS}/monitoring-stats.html#MONITORING-PG-STAT-ALL-INDEXES-VIEW`;
const PG_LOCKS = `${PG_DOCS}/view-pg-locks.html`;
const PG_WAIT_EVENTS = `${PG_DOCS}/monitoring-stats.html#WAIT-EVENT-TABLE`;
const PG_VACUUM = `${PG_DOCS}/routine-vacuuming.html`;

export const COLUMN_HELP: Record<string, ColumnHelpEntry> = {
  // =====================================================
  // PRC (OS Processes)
  // =====================================================
  cpu_pct: {
    label: "CPU%",
    description: "CPU usage percentage since last sample.",
    thresholds: ">90% critical \u00b7 50-90% warning \u00b7 <50% normal",
    tip: "Check active queries in PGA tab",
  },
  mem_pct: {
    label: "MEM%",
    description: "Resident memory as percentage of total RAM.",
    thresholds: ">90% critical \u00b7 70-90% warning \u00b7 <70% normal",
    tip: "Check work_mem and maintenance_work_mem settings",
  },
  pid: {
    label: "PID",
    description:
      "OS process ID. For PostgreSQL backends, matches pg_stat_activity.pid.",
    tip: "Use drill-down to navigate to PGA for backend details",
  },
  state: {
    label: "State",
    description:
      "Backend/process state. For PGA: active, idle, idle in transaction.",
    thresholds: "idle in transaction = warning \u00b7 aborted = critical",
    tip: "Idle-in-transaction sessions hold locks without working",
    docUrl: PG_STAT_ACTIVITY,
  },
  cmdline: {
    label: "Command",
    description:
      "Full command line of the process. PostgreSQL backends show process title.",
    tip: "Useful for identifying application processes",
  },
  nvcsw_s: {
    label: "VCtx/s",
    description:
      "Voluntary context switches per second. Process yielded CPU willingly (I/O wait).",
    tip: "High values indicate I/O-bound process",
  },
  nivcsw_s: {
    label: "ICtx/s",
    description:
      "Involuntary context switches per second. OS preempted the process.",
    tip: "High values indicate CPU contention or overloaded system",
  },
  rss_kb: {
    label: "RSS",
    description:
      "Resident set size \u2014 physical memory actually used by the process.",
    tip: "High RSS on backends = large work_mem sorts/joins or maintenance_work_mem",
  },
  vsize_kb: {
    label: "VSIZE",
    description:
      "Virtual memory size \u2014 total address space (includes mapped but unused pages).",
    tip: "VSIZE is often misleading. Focus on RSS for actual memory usage",
  },
  vswap_kb: {
    label: "SWAP",
    description:
      "Memory swapped to disk. Any swap on PostgreSQL backends degrades performance.",
    thresholds: ">0 critical",
    tip: "Disable swap for PostgreSQL or increase vm.swappiness=0",
  },

  // =====================================================
  // Summary: Host CPU
  // =====================================================
  iow_pct: {
    label: "IO Wait%",
    description: "Percentage of CPU time waiting for I/O completion.",
    thresholds: ">15% critical \u00b7 5-15% warning",
    tip: "High iowait indicates disk I/O bottleneck. Check disk utilization",
  },
  steal_pct: {
    label: "Steal%",
    description: "CPU time stolen by hypervisor for other VMs.",
    thresholds: ">10% critical \u00b7 3-10% warning",
    tip: "High steal = hypervisor overcommit. Consider dedicated resources",
  },
  idle_pct: {
    label: "Idle%",
    description: "Percentage of CPU time idle (not doing any work).",
    thresholds: "<10% critical \u00b7 <30% warning",
    tip: "Very low idle = CPU saturated. Check top consumers in PRC tab",
  },

  // =====================================================
  // Summary: Host Swap
  // =====================================================
  "swap.used_kb": {
    label: "Used",
    description: "Amount of swap space currently in use.",
    thresholds: ">1 GB critical \u00b7 >0 warning \u00b7 =0 good",
    tip: "Any swap usage for PostgreSQL backends degrades performance severely",
  },

  // =====================================================
  // Summary: PSI
  // =====================================================
  cpu_some_pct: {
    label: "CPU Some%",
    description: "Percentage of time at least some tasks were stalled on CPU.",
    thresholds: ">25% critical \u00b7 5-25% warning",
    tip: "Measures actual CPU contention, not just utilization",
  },
  mem_some_pct: {
    label: "Mem Some%",
    description:
      "Percentage of time at least some tasks were stalled on memory.",
    thresholds: ">25% critical \u00b7 5-25% warning",
    tip: "Memory pressure causing page reclaim or swap",
  },
  io_some_pct: {
    label: "IO Some%",
    description: "Percentage of time at least some tasks were stalled on I/O.",
    thresholds: ">40% critical \u00b7 10-40% warning",
    tip: "I/O pressure from disk bottleneck or insufficient buffer cache",
  },

  // =====================================================
  // Summary: VMstat
  // =====================================================
  swin_s: {
    label: "Swap In/s",
    description: "Pages swapped in from disk per second.",
    thresholds: ">0 critical",
    tip: "Active swap-in = memory pressure. Increase RAM or reduce shared_buffers",
  },
  swout_s: {
    label: "Swap Out/s",
    description: "Pages swapped out to disk per second.",
    thresholds: ">0 critical",
    tip: "Active swap-out = severe memory pressure",
  },

  // =====================================================
  // Summary: PostgreSQL
  // =====================================================
  hit_ratio_pct: {
    label: "Hit Ratio",
    description: "Buffer cache hit ratio across all databases.",
    thresholds: "\u226599% good \u00b7 90-99% warning \u00b7 <90% critical",
    tip: "For OLTP, hit ratio should be \u226599%. Low values = increase shared_buffers",
  },
  temp_bytes_s: {
    label: "Temp/s",
    description: "Temporary file bytes written per second across all backends.",
    thresholds: ">0 warning",
    tip: "Temp files indicate work_mem overflow. Increase work_mem",
  },

  // =====================================================
  // Summary: BGWriter
  // =====================================================
  buffers_backend_s: {
    label: "Backend Buf/s",
    description: "Buffers written directly by backends (not bgwriter).",
    thresholds: ">0 warning",
    tip: "Should be 0 normally. High values = bgwriter can't keep up",
  },
  maxwritten_clean: {
    label: "Max Written",
    description:
      "Number of times bgwriter stopped cleaning because it wrote too many buffers.",
    thresholds: ">0 warning",
    tip: "Increase bgwriter_lru_maxpages if this is non-zero",
  },

  // =====================================================
  // Summary: Disk
  // =====================================================
  "disk.util_pct": {
    label: "Util%",
    description: "Disk utilization percentage (time spent doing I/O).",
    thresholds: ">90% critical \u00b7 60-90% warning",
    tip: "100% utilization = disk saturated. Consider faster storage or I/O optimization",
  },

  // =====================================================
  // Summary: Cgroup CPU
  // =====================================================
  "cgroup_cpu.limit_cores": {
    label: "Limit",
    description: "CPU core limit assigned to this container (quota/period).",
    tip: "Container can use up to this many CPU cores",
  },
  "cgroup_cpu.used_pct": {
    label: "Used%",
    description: "CPU utilization as percentage of container limit.",
    thresholds: ">90% critical \u00b7 70-90% warning",
    tip: "Approaching limit will cause throttling. Request more CPU or optimize",
  },
  "cgroup_cpu.throttled_ms": {
    label: "Throttled",
    description:
      "CPU throttling time per sample interval (ms). Container exceeded its CPU quota.",
    thresholds: ">1000ms critical \u00b7 >0 warning",
    tip: "Non-zero throttling degrades query performance. Increase CPU limit",
  },
  "cgroup_cpu.nr_throttled": {
    label: "Nr Throttled",
    description:
      "Number of times this container's CPU usage was throttled per sample.",
    thresholds: ">0 warning",
    tip: "Frequent throttling = CPU limit too low for the workload",
  },

  // =====================================================
  // Summary: Cgroup Memory
  // =====================================================
  "cgroup_memory.used_pct": {
    label: "Used%",
    description: "Memory utilization as percentage of container limit.",
    thresholds: ">95% critical \u00b7 80-95% warning",
    tip: "Approaching limit triggers OOM kills. Increase memory limit or optimize",
  },
  "cgroup_memory.oom_kills": {
    label: "OOM Kills",
    description: "Cumulative number of OOM kills in this container.",
    thresholds: ">0 critical",
    tip: "OOM kills terminate processes. Increase memory limit immediately",
  },
  "cgroup_memory.limit_bytes": {
    label: "Limit",
    description: "Memory limit assigned to this container.",
    tip: "Container will be OOM-killed if it exceeds this limit",
  },

  // =====================================================
  // Summary: Cgroup PIDs
  // =====================================================
  "cgroup_pids.current": {
    label: "Current",
    description: "Current number of processes/threads in the container.",
    tip: "Approaching max may prevent fork() and new connections",
  },
  "cgroup_pids.max": {
    label: "Max",
    description:
      "Maximum number of processes/threads allowed in the container.",
  },

  // =====================================================
  // PGA (pg_stat_activity)
  // =====================================================
  database: {
    label: "Database",
    description:
      "Name of the PostgreSQL database. For PGA: database the backend is connected to. For PGT/PGI: database containing the table/index.",
    docUrl: PG_STAT_ACTIVITY,
  },
  user: {
    label: "User",
    description: "Database user name of this backend.",
    docUrl: PG_STAT_ACTIVITY,
  },
  application_name: {
    label: "App Name",
    description:
      "Application name set by the client via application_name parameter.",
    tip: "Configure your app to set this for easy identification",
    docUrl: PG_STAT_ACTIVITY,
  },
  client_addr: {
    label: "Client",
    description:
      "IP address of the connected client. Null for local Unix socket connections.",
    docUrl: PG_STAT_ACTIVITY,
  },
  backend_type: {
    label: "Backend Type",
    description:
      "Type of backend: client backend, autovacuum worker, walwriter, bgwriter, etc.",
    tip: "Filter by backend_type to focus on client connections",
    docUrl: PG_STAT_ACTIVITY,
  },
  wait_event_type: {
    label: "Wait Type",
    description:
      "Category of the wait event. Main types:\n" +
      "\u2022 Lock \u2014 heavyweight lock (row, table, advisory)\n" +
      "\u2022 LWLock \u2014 lightweight lock (shared memory structures)\n" +
      "\u2022 IO \u2014 waiting for I/O completion\n" +
      "\u2022 BufferPin \u2014 waiting for a buffer pin\n" +
      "\u2022 Activity \u2014 idle server process (normal)\n" +
      "\u2022 Client \u2014 waiting for client data\n" +
      "\u2022 IPC \u2014 inter-process communication",
    thresholds: "Lock/LWLock/IO = warning \u00b7 Activity/Client = normal",
    tip: "Lock waits indicate contention. Check PGL tab for blocking tree",
    docUrl: PG_WAIT_EVENTS,
  },
  wait_event: {
    label: "Wait Event",
    description:
      "Specific wait event name within the wait type. Common events:\n" +
      "\u2022 ClientRead \u2014 waiting for client to send query\n" +
      "\u2022 DataFileRead \u2014 reading data from disk\n" +
      "\u2022 WALWrite \u2014 writing to WAL\n" +
      "\u2022 transactionid \u2014 waiting for transaction to finish\n" +
      "\u2022 tuple \u2014 waiting for row-level lock\n" +
      "\u2022 relation \u2014 waiting for table-level lock",
    tip: "DataFileRead = cold cache. transactionid/tuple/relation = lock contention",
    docUrl: PG_WAIT_EVENTS,
  },
  query: {
    label: "Query",
    description: "Current or most recently executed SQL query text.",
    tip: "Truncated by track_activity_query_size (default 1024 bytes)",
    docUrl: PG_STAT_ACTIVITY,
  },
  query_id: {
    label: "Query ID",
    description:
      "Hash of the normalized query from pg_stat_statements. Allows drill-down to PGS.",
    tip: "Use drill-down (>) to see aggregated statistics for this query pattern",
    docUrl: PG_STAT_STATEMENTS,
  },
  query_duration_s: {
    label: "Query Duration",
    description: "Time elapsed since the current query started executing.",
    thresholds: ">30s critical \u00b7 1-30s warning \u00b7 <1s normal",
    tip: "Only meaningful for active sessions. Compare with mean_exec_time from PGS",
    docUrl: PG_STAT_ACTIVITY,
  },
  xact_duration_s: {
    label: "Transaction Duration",
    description:
      "Time elapsed since the current transaction started (xact_start).",
    thresholds: ">60s critical \u00b7 5-60s warning \u00b7 <5s normal",
    tip: "Long transactions prevent VACUUM from reclaiming dead rows and hold locks",
    docUrl: PG_STAT_ACTIVITY,
  },
  backend_duration_s: {
    label: "Backend Duration",
    description: "Time since this backend process connected (backend_start).",
    tip: "Very old backends may indicate connection pooling issues",
    docUrl: PG_STAT_ACTIVITY,
  },
  backend_start: {
    label: "Backend Start",
    description: "Timestamp when this backend process was started.",
    docUrl: PG_STAT_ACTIVITY,
  },
  xact_start: {
    label: "Xact Start",
    description:
      "Timestamp when the current transaction started. Null if no active transaction.",
    docUrl: PG_STAT_ACTIVITY,
  },
  query_start: {
    label: "Query Start",
    description: "Timestamp when the currently active query started execution.",
    docUrl: PG_STAT_ACTIVITY,
  },
  stmt_mean_exec_time_ms: {
    label: "Stmt Avg",
    description:
      "Mean execution time from pg_stat_statements for this query pattern.",
    tip: "Compare with current query_duration to detect anomalies",
    docUrl: PG_STAT_STATEMENTS,
  },
  stmt_max_exec_time_ms: {
    label: "Stmt Max",
    description: "Maximum execution time ever recorded for this query pattern.",
    docUrl: PG_STAT_STATEMENTS,
  },
  stmt_calls_s: {
    label: "Stmt Calls/s",
    description:
      "Execution rate from pg_stat_statements for this query pattern.",
    tip: "High-frequency queries have biggest optimization impact",
    docUrl: PG_STAT_STATEMENTS,
  },
  stmt_hit_pct: {
    label: "Stmt Hit%",
    description:
      "Buffer cache hit ratio from pg_stat_statements for this query pattern.",
    thresholds: "\u226599% good \u00b7 90-99% warning \u00b7 <90% critical",
    tip: "Low hit% means this query pattern does excessive disk I/O",
    docUrl: PG_STAT_STATEMENTS,
  },

  // =====================================================
  // PGS (pg_stat_statements)
  // =====================================================
  queryid: {
    label: "Query ID",
    description: "Internal hash of the normalized (parameterized) query text.",
    docUrl: PG_STAT_STATEMENTS,
  },
  calls: {
    label: "Calls",
    description:
      "Total number of times this statement was executed (cumulative since stats reset).",
    docUrl: PG_STAT_STATEMENTS,
  },
  rows: {
    label: "Rows",
    description:
      "Total rows retrieved or affected by this statement (cumulative).",
    docUrl: PG_STAT_STATEMENTS,
  },
  calls_s: {
    label: "Calls/s",
    description: "Execution rate \u2014 number of calls per second.",
    tip: "High-frequency queries benefit most from optimization",
    docUrl: PG_STAT_STATEMENTS,
  },
  rows_s: {
    label: "Rows/s",
    description: "Rate of rows returned or affected per second.",
    docUrl: PG_STAT_STATEMENTS,
  },
  exec_time_ms_s: {
    label: "Time/s",
    description:
      "Total execution time consumed per second (ms/s). Measures CPU pressure from this query.",
    tip: "Top queries by Time/s are consuming the most database resources",
    docUrl: PG_STAT_STATEMENTS,
  },
  mean_exec_time_ms: {
    label: "Avg Time",
    description:
      "Average execution time per call (ms). Primary optimization target.",
    tip: "Compare across time periods to detect regressions",
    docUrl: PG_STAT_STATEMENTS,
  },
  min_exec_time_ms: {
    label: "Min Time",
    description: "Minimum execution time ever recorded for this query (ms).",
    tip: "Best-case performance. Large gap to mean suggests plan instability",
    docUrl: PG_STAT_STATEMENTS,
  },
  max_exec_time_ms: {
    label: "Max Time",
    description: "Maximum execution time ever recorded for this query (ms).",
    tip: "Worst-case latency. Large gap to mean may indicate lock contention or cache misses",
    docUrl: PG_STAT_STATEMENTS,
  },
  stddev_exec_time_ms: {
    label: "StdDev",
    description:
      "Standard deviation of execution time. High values mean inconsistent performance.",
    tip: "High stddev = plan instability, lock contention, or varying data volumes",
    docUrl: PG_STAT_STATEMENTS,
  },
  rows_per_call: {
    label: "R/Call",
    description: "Average rows returned per execution (rows / calls).",
    tip: "Very high values may indicate missing LIMIT or overly broad WHERE clause",
    docUrl: PG_STAT_STATEMENTS,
  },
  hit_pct: {
    label: "HIT%",
    description:
      "Buffer cache hit ratio: shared_blks_hit / (hit + read) \u00d7 100.",
    thresholds: "\u226599% good \u00b7 90-99% warning \u00b7 <90% critical",
    tip: "Low HIT% = query reads cold data from disk. Consider increasing shared_buffers",
    docUrl: PG_STAT_STATEMENTS,
  },
  shared_blks_read_s: {
    label: "Blk Rd/s",
    description:
      "Shared buffer blocks read from disk per second (physical I/O).",
    tip: "Physical reads are 100x slower than cache hits",
    docUrl: PG_STAT_STATEMENTS,
  },
  shared_blks_hit_s: {
    label: "Blk Hit/s",
    description:
      "Shared buffer blocks found in PostgreSQL buffer cache per second.",
    docUrl: PG_STAT_STATEMENTS,
  },
  shared_blks_dirtied_s: {
    label: "Blk Dirty/s",
    description: "Shared buffer blocks dirtied per second (modified in cache).",
    docUrl: PG_STAT_STATEMENTS,
  },
  shared_blks_written_s: {
    label: "Blk Wr/s",
    description:
      "Shared buffer blocks written to disk per second by this query.",
    tip: "High values indicate backend doing direct writes (bgwriter can't keep up)",
    docUrl: PG_STAT_STATEMENTS,
  },
  temp_blks_read_s: {
    label: "Tmp Rd/s",
    description:
      "Temp file blocks read per second. Sorts/hashes spilling to disk.",
    tip: "Increase work_mem to avoid temp files",
    docUrl: PG_STAT_STATEMENTS,
  },
  temp_blks_written_s: {
    label: "Tmp Wr/s",
    description: "Temp file blocks written per second.",
    docUrl: PG_STAT_STATEMENTS,
  },
  temp_mb_s: {
    label: "Temp MB/s",
    description:
      "Temp file throughput in MB/s. Sorts and hash joins spilling to disk.",
    tip: "Increase work_mem for this query to avoid temp files",
    docUrl: PG_STAT_STATEMENTS,
  },
  local_blks_read_s: {
    label: "Local Rd/s",
    description: "Local buffer blocks read per second (temporary tables).",
    docUrl: PG_STAT_STATEMENTS,
  },
  local_blks_written_s: {
    label: "Local Wr/s",
    description: "Local buffer blocks written per second (temporary tables).",
    docUrl: PG_STAT_STATEMENTS,
  },
  total_exec_time: {
    label: "Total Exec",
    description:
      "Total execution time for all calls (cumulative, ms). Resets on pg_stat_statements_reset().",
    docUrl: PG_STAT_STATEMENTS,
  },
  total_plan_time: {
    label: "Total Plan",
    description:
      "Total planning time for all calls (cumulative, ms). Requires track_planning=on.",
    tip: "High planning time may indicate complex queries or excessive partitions",
    docUrl: PG_STAT_STATEMENTS,
  },
  wal_records: {
    label: "WAL Records",
    description: "Total WAL records generated by this statement (cumulative).",
    docUrl: PG_STAT_STATEMENTS,
  },
  wal_bytes: {
    label: "WAL Bytes",
    description: "Total WAL bytes generated by this statement (cumulative).",
    tip: "High WAL volume affects replication lag and backup size",
    docUrl: PG_STAT_STATEMENTS,
  },

  // =====================================================
  // PGT (pg_stat_user_tables)
  // =====================================================
  display_name: {
    label: "Table",
    description: "Schema-qualified table name (schema.table).",
    docUrl: PG_STAT_USER_TABLES,
  },
  size_bytes: {
    label: "Size",
    description:
      "Total on-disk size including table, indexes, TOAST, and free space map.",
    docUrl: PG_STAT_USER_TABLES,
  },
  n_live_tup: {
    label: "Live Tuples",
    description: "Estimated number of live (visible) rows. Updated by ANALYZE.",
    tip: "Inaccurate if ANALYZE hasn't run recently",
    docUrl: PG_STAT_USER_TABLES,
  },
  n_dead_tup: {
    label: "Dead Tuples",
    description:
      "Estimated number of dead (deleted/updated) rows not yet reclaimed by VACUUM.",
    thresholds: ">100K critical \u00b7 1K-100K warning \u00b7 <1K normal",
    tip: "Dead rows consume disk space and slow sequential scans. Run VACUUM",
    docUrl: PG_VACUUM,
  },
  dead_pct: {
    label: "DEAD%",
    description:
      "Dead rows as percentage of total tuples: n_dead_tup / (live + dead) \u00d7 100.",
    thresholds: ">20% critical \u00b7 5-20% warning \u00b7 <5% normal",
    tip: "High DEAD% = VACUUM falling behind. Check autovacuum settings",
    docUrl: PG_VACUUM,
  },
  seq_scan_s: {
    label: "Seq Scan/s",
    description: "Sequential (full table) scans per second.",
    tip: "Sequential scans on large tables are expensive. Consider adding an index",
    docUrl: PG_STAT_USER_TABLES,
  },
  seq_tup_read_s: {
    label: "Seq Read/s",
    description: "Rows read via sequential scans per second.",
    docUrl: PG_STAT_USER_TABLES,
  },
  idx_scan_s: {
    label: "Idx Scan/s",
    description: "Index scans per second on this table.",
    docUrl: PG_STAT_USER_TABLES,
  },
  idx_tup_fetch_s: {
    label: "Idx Fetch/s",
    description: "Rows fetched via index scans per second.",
    docUrl: PG_STAT_USER_TABLES,
  },
  tot_tup_read_s: {
    label: "Total Read/s",
    description: "Total rows read per second (sequential + index).",
    docUrl: PG_STAT_USER_TABLES,
  },
  seq_pct: {
    label: "SEQ%",
    description:
      "Sequential scan ratio: seq_scan / (seq_scan + idx_scan) \u00d7 100.",
    thresholds: ">80% critical \u00b7 30-80% warning \u00b7 <30% normal",
    tip: "High SEQ% on large tables = missing index. Check query plans",
    docUrl: PG_STAT_USER_TABLES,
  },
  n_tup_ins_s: {
    label: "Insert/s",
    description: "Rows inserted per second.",
    docUrl: PG_STAT_USER_TABLES,
  },
  n_tup_upd_s: {
    label: "Update/s",
    description: "Rows updated per second (creates dead tuples).",
    docUrl: PG_STAT_USER_TABLES,
  },
  n_tup_del_s: {
    label: "Delete/s",
    description: "Rows deleted per second (creates dead tuples).",
    docUrl: PG_STAT_USER_TABLES,
  },
  n_tup_hot_upd_s: {
    label: "HOT Upd/s",
    description:
      "HOT (Heap-Only Tuple) updates per second. Updates that don't require index changes.",
    tip: "Higher HOT ratio = better. Adjust fillfactor to increase HOT updates",
    docUrl: PG_STAT_USER_TABLES,
  },
  hot_pct: {
    label: "HOT%",
    description:
      "HOT update ratio: hot_updates / total_updates \u00d7 100. Higher is better.",
    tip: "Low HOT% = every update re-indexes. Consider fillfactor < 100",
    docUrl: PG_STAT_USER_TABLES,
  },
  io_hit_pct: {
    label: "HIT%",
    description:
      "Buffer cache hit ratio for all I/O on this table (heap + index + toast).",
    thresholds: "\u226599% good \u00b7 90-99% warning \u00b7 <90% critical",
    tip: "Low HIT% = table doesn't fit in shared_buffers. Consider increasing shared_buffers or optimizing queries",
    docUrl: PG_STAT_USER_TABLES,
  },
  disk_blks_read_s: {
    label: "DISK/s",
    description: "Total physical disk blocks read per second (heap + index).",
    thresholds: ">0 warning \u00b7 =0 inactive",
    tip: "Physical reads are slow. Improve HIT% or reduce table size",
  },
  heap_blks_read_s: {
    label: "Heap Rd/s",
    description: "Heap (table data) blocks read from disk per second.",
    docUrl: PG_STAT_USER_TABLES,
  },
  heap_blks_hit_s: {
    label: "Heap Hit/s",
    description: "Heap blocks found in buffer cache per second.",
    docUrl: PG_STAT_USER_TABLES,
  },
  idx_blks_read_s: {
    label: "Idx Rd/s",
    description: "Index blocks read from disk per second.",
    docUrl: PG_STAT_USER_TABLES,
  },
  idx_blks_hit_s: {
    label: "Idx Hit/s",
    description: "Index blocks found in buffer cache per second.",
    docUrl: PG_STAT_USER_TABLES,
  },
  toast_blks_read_s: {
    label: "Toast Rd/s",
    description: "TOAST table blocks read from disk per second.",
    tip: "High TOAST reads = large columns (text, json, bytea) causing I/O",
    docUrl: PG_STAT_USER_TABLES,
  },
  toast_blks_hit_s: {
    label: "Toast Hit/s",
    description: "TOAST table blocks found in buffer cache per second.",
    docUrl: PG_STAT_USER_TABLES,
  },
  tidx_blks_read_s: {
    label: "TIdx Rd/s",
    description: "TOAST index blocks read from disk per second.",
    docUrl: PG_STAT_USER_TABLES,
  },
  tidx_blks_hit_s: {
    label: "TIdx Hit/s",
    description: "TOAST index blocks found in buffer cache per second.",
    docUrl: PG_STAT_USER_TABLES,
  },
  vacuum_count_s: {
    label: "Vacuum/s",
    description: "Manual VACUUM executions per second.",
    docUrl: PG_VACUUM,
  },
  autovacuum_count_s: {
    label: "AutoVac/s",
    description: "Autovacuum executions per second.",
    tip: "Frequent autovacuums = high write rate or aggressive autovacuum settings",
    docUrl: PG_VACUUM,
  },
  analyze_count_s: {
    label: "Analyze/s",
    description: "Manual ANALYZE executions per second.",
    docUrl: PG_STAT_USER_TABLES,
  },
  autoanalyze_count_s: {
    label: "AutoAnalyze/s",
    description: "Auto-analyze executions per second.",
    docUrl: PG_STAT_USER_TABLES,
  },
  last_autovacuum: {
    label: "Last AutoVac",
    description: "Time since last autovacuum completed on this table.",
    tip: "Very old autovacuum + high dead_pct = autovacuum can't keep up",
    docUrl: PG_VACUUM,
  },
  last_autoanalyze: {
    label: "Last AutoAnalyze",
    description:
      "Time since last auto-analyze completed. Stale stats cause bad query plans.",
    docUrl: PG_STAT_USER_TABLES,
  },
  last_vacuum: {
    label: "Last Vacuum",
    description: "Time since last manual VACUUM on this table.",
    docUrl: PG_VACUUM,
  },
  last_analyze: {
    label: "Last Analyze",
    description: "Time since last manual ANALYZE on this table.",
    docUrl: PG_STAT_USER_TABLES,
  },

  // =====================================================
  // PGI (pg_stat_user_indexes)
  // =====================================================
  indexrelid: {
    label: "Index OID",
    description: "OID of the index in pg_class.",
    docUrl: PG_STAT_USER_INDEXES,
  },
  index: {
    label: "Index",
    description: "Index name.",
    docUrl: PG_STAT_USER_INDEXES,
  },
  display_table: {
    label: "Table",
    description: "Schema-qualified name of the table this index belongs to.",
    docUrl: PG_STAT_USER_INDEXES,
  },
  idx_scan: {
    label: "Idx Scans",
    description: "Total index scans initiated on this index (cumulative).",
    tip: "Indexes with 0 scans are unused \u2014 candidates for DROP INDEX",
    docUrl: PG_STAT_USER_INDEXES,
  },
  idx_tup_read_s: {
    label: "Idx Read/s",
    description: "Index entries returned by scans per second.",
    tip: "Large gap between read and fetch indicates index bloat",
    docUrl: PG_STAT_USER_INDEXES,
  },

  // =====================================================
  // PGL (pg_locks)
  // =====================================================
  depth: {
    label: "Depth",
    description:
      "Position in the lock tree. Depth 1 = root blocker causing the chain.",
    tip: "Investigate root blockers (depth=1) first \u2014 they impact the most sessions",
    docUrl: PG_LOCKS,
  },
  root_pid: {
    label: "Root PID",
    description:
      "PID of the root blocking process at the top of the lock chain.",
    docUrl: PG_LOCKS,
  },
  lock_type: {
    label: "Lock Type",
    description:
      "Type of lockable object:\n" +
      "\u2022 relation \u2014 table/index lock\n" +
      "\u2022 transactionid \u2014 waiting for transaction to finish\n" +
      "\u2022 tuple \u2014 row-level lock\n" +
      "\u2022 advisory \u2014 application advisory lock\n" +
      "\u2022 virtualxid \u2014 virtual transaction ID lock",
    docUrl: PG_LOCKS,
  },
  lock_mode: {
    label: "Lock Mode",
    description:
      "Lock mode strength (weakest to strongest):\n" +
      "\u2022 AccessShareLock \u2014 SELECT\n" +
      "\u2022 RowShareLock \u2014 SELECT FOR UPDATE/SHARE\n" +
      "\u2022 RowExclusiveLock \u2014 INSERT/UPDATE/DELETE\n" +
      "\u2022 ShareLock \u2014 CREATE INDEX\n" +
      "\u2022 ExclusiveLock \u2014 certain ALTER TABLE\n" +
      "\u2022 AccessExclusiveLock \u2014 DROP TABLE, VACUUM FULL, etc.",
    tip: "AccessExclusiveLock blocks ALL other access including SELECT",
    docUrl: `${PG_DOCS}/explicit-locking.html`,
  },
  lock_target: {
    label: "Lock Target",
    description:
      "The object being locked \u2014 table name, transaction ID, or advisory lock key.",
    docUrl: PG_LOCKS,
  },
  lock_granted: {
    label: "Granted",
    description: "true = lock is held. false = session is waiting (blocked).",
    thresholds: "false = critical (session blocked)",
    tip: "Find root blocker (depth=1) and investigate their query",
    docUrl: PG_LOCKS,
  },
  state_change: {
    label: "State Change",
    description: "Time since the last state change in pg_stat_activity.",
    docUrl: PG_STAT_ACTIVITY,
  },

  // =====================================================
  // PGE (Events) — checkpoint-specific dual-use fields
  // =====================================================
  severity: {
    label: "Severity",
    description:
      "Log severity level: ERROR, FATAL, or PANIC for errors; LOG for events.",
    thresholds: "PANIC/FATAL = critical · ERROR = warning",
  },
  count: {
    label: "Count",
    description:
      "Number of occurrences in the snapshot interval (grouped errors). Always 1 for events.",
    thresholds: ">100 critical · 10-100 warning",
  },
  event_type: {
    label: "Type",
    description:
      "Event type: checkpoint_starting, checkpoint_complete, autovacuum, autoanalyze, or error severity.",
  },
  table_name: {
    label: "Table",
    description:
      "Target table for autovacuum/autoanalyze. Empty for checkpoints and errors.",
    docUrl: PG_VACUUM,
  },
  elapsed_s: {
    label: "Elapsed",
    description:
      "Total duration in seconds.\n• Checkpoint: total checkpoint time\n• Autovacuum: elapsed time reported by PostgreSQL",
    thresholds: ">5min critical · 30s-5min warning",
  },
  extra_num1: {
    label: "Buffers/Tuples",
    description:
      "Dual-use field:\n• Checkpoint: buffers written to disk\n• Autovacuum: tuples removed",
  },
  extra_num2: {
    label: "Distance/Pages",
    description:
      "Dual-use field:\n• Checkpoint: WAL distance in kB between this and previous checkpoint\n• Autovacuum: pages removed",
  },
  extra_num3: {
    label: "Estimate",
    description:
      "Checkpoint only: PostgreSQL's estimate of optimal checkpoint distance (kB). Used to plan next checkpoint spacing.",
    tip: "If distance >> estimate, checkpoint_completion_target may need tuning",
  },
  cpu_user_s: {
    label: "CPU User / Write Time",
    description:
      "Dual-use field:\n• Checkpoint: write phase duration (seconds)\n• Autovacuum: CPU user time (seconds)",
    thresholds: ">30s critical · 5-30s warning",
  },
  cpu_system_s: {
    label: "CPU Sys / Sync Time",
    description:
      "Dual-use field:\n• Checkpoint: sync (fsync) phase duration (seconds)\n• Autovacuum: CPU system time (seconds)",
    thresholds: ">30s critical · 5-30s warning",
    tip: "Checkpoint sync time should be near zero with modern filesystems and effective_io_concurrency",
  },
  buffer_hits: {
    label: "Buf Hits / Sync Files",
    description:
      "Dual-use field:\n• Checkpoint: number of files synchronized (fsync'd)\n• Autovacuum: buffer cache hits",
  },
  buffer_misses: {
    label: "Buf Misses",
    description:
      "Buffer cache misses (physical reads from disk). Autovacuum only.",
    thresholds: ">10K critical · 1K-10K warning",
  },
  buffer_dirtied: {
    label: "Buf Dirtied",
    description: "Buffers dirtied during operation. Autovacuum only.",
    thresholds: ">1K critical · 100-1K warning",
  },
  avg_read_rate_mbs: {
    label: "Avg Read / Longest Sync",
    description:
      "Dual-use field:\n• Checkpoint: longest individual file sync duration (seconds)\n• Autovacuum: average read rate (MB/s)",
    thresholds: "Autovacuum: >100 MB/s critical · 20-100 MB/s warning",
    tip: "Checkpoint: high longest sync indicates slow storage for specific files",
  },
  avg_write_rate_mbs: {
    label: "Avg Write / Avg Sync",
    description:
      "Dual-use field:\n• Checkpoint: average file sync duration (seconds)\n• Autovacuum: average write rate (MB/s)",
    thresholds: "Autovacuum: >50 MB/s critical · 10-50 MB/s warning",
  },
};

export function buildColumnTooltip(key: string): ReactNode | null {
  const help = COLUMN_HELP[key];
  if (!help) return null;
  return (
    <div className="space-y-1 max-w-xs">
      <div className="font-semibold text-[var(--text-primary)]">
        {help.label}
      </div>
      <div className="text-[var(--text-secondary)] whitespace-pre-line">
        {help.description}
      </div>
      {help.thresholds && (
        <div className="text-[11px] text-[var(--text-tertiary)] border-t border-[var(--border-subtle)] pt-1 mt-1">
          {help.thresholds}
        </div>
      )}
      {help.tip && (
        <div className="text-[11px] italic text-[var(--accent-text)]">
          {help.tip}
        </div>
      )}
      {help.docUrl && (
        <div className="text-[11px] border-t border-[var(--border-subtle)] pt-1 mt-1">
          <a
            href={help.docUrl}
            target="_blank"
            rel="noopener noreferrer"
            className="text-[var(--accent)] hover:underline"
            onClick={(e) => e.stopPropagation()}
          >
            PostgreSQL docs &rarr;
          </a>
        </div>
      )}
    </div>
  );
}

export const VIEW_DESCRIPTIONS: Record<string, Record<string, string>> = {
  prc: {
    generic: "General overview \u2014 CPU, memory, threads",
    command: "Process hierarchy and command lines",
    memory: "Memory breakdown \u2014 virtual, resident, swap, segments",
    disk: "Disk I/O throughput and operations",
    scheduler: "CPU scheduling \u2014 nice, priority, context switches",
  },
  pga: {
    generic: "Active sessions with OS metrics and wait events",
    stats: "Sessions enriched with pg_stat_statements metrics",
  },
  pgs: {
    calls: "Most frequently executed query patterns",
    time: "Queries consuming the most execution time",
    io: "Queries doing the most physical I/O",
    temp: "Queries using temporary files (work_mem overflow)",
  },
  pgt: {
    reads: "Tables with highest read activity",
    writes: "Tables with highest write activity",
    scans: "Sequential vs index scan ratio analysis",
    maintenance: "Vacuum and analyze status \u2014 dead tuples, bloat",
    io: "Physical I/O by table \u2014 cache hit ratio",
    schema:
      "Tables aggregated by schema \u2014 identify which schema consumes most I/O",
  },
  pgi: {
    usage: "Active index usage \u2014 scans and tuple fetches",
    unused: "Indexes with zero scans \u2014 candidates for DROP",
    io: "Index physical I/O \u2014 cache hit ratio",
    schema:
      "Indexes aggregated by schema \u2014 identify which schema consumes most index I/O",
  },
  pge: {
    errors: "Grouped PostgreSQL errors by pattern",
    checkpoints: "Checkpoint metrics \u2014 timing, buffers, WAL files, sync",
    autovacuum:
      "Autovacuum/autoanalyze metrics \u2014 timing, buffers, CPU, WAL",
  },
  pgl: {
    tree: "Lock blocking tree \u2014 who blocks whom",
  },
};
