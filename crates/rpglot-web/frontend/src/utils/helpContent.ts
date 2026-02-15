import type { TabKey } from "../api/types";

interface MetricHelp {
  label: string;
  description: string;
  thresholds?: string;
}

interface ViewHelp {
  description: string;
  metrics: MetricHelp[];
}

interface TabHelp {
  label: string;
  source: string;
  description: string;
  howToRead: string;
  drillDown?: string;
  views: Record<string, ViewHelp>;
}

export const TAB_HELP: Record<TabKey, TabHelp> = {
  prc: {
    label: "Processes",
    source: "/proc",
    description:
      "OS processes on the system. PostgreSQL backends are enriched with current query and backend type.",
    howToRead:
      "Sort by CPU% to find CPU-intensive processes. Check Memory view for backends with high RSS (large sorts/joins). Use Disk view to find processes doing physical I/O. D-state processes indicate I/O bottleneck.",
    drillDown: "Navigate to PGA to see PostgreSQL session details.",
    views: {
      generic: {
        description: "General overview of process activity.",
        metrics: [
          {
            label: "CPU%",
            description: "CPU usage since last sample",
            thresholds: ">90% critical · 50-90% warning",
          },
          {
            label: "MEM%",
            description: "Resident memory as % of total",
            thresholds: ">90% critical · 70-90% warning",
          },
          {
            label: "VGrow",
            description: "Virtual memory growth since last sample",
          },
          {
            label: "RGrow",
            description: "Resident memory growth since last sample",
          },
          { label: "Threads", description: "Number of threads in the process" },
        ],
      },
      command: {
        description: "Process hierarchy and command lines.",
        metrics: [
          { label: "PPID", description: "Parent process ID" },
          {
            label: "State",
            description: "R=running, S=sleeping, D=disk wait, Z=zombie",
          },
          { label: "Command", description: "Full command line of the process" },
        ],
      },
      memory: {
        description: "Detailed memory breakdown per process.",
        metrics: [
          { label: "VSize", description: "Total virtual memory" },
          { label: "RSS", description: "Resident set size (physical memory)" },
          {
            label: "PSS",
            description: "Proportional set size (shared pages divided)",
          },
          { label: "VSwap", description: "Amount swapped to disk" },
          { label: "VLock", description: "Locked (non-swappable) memory" },
        ],
      },
      disk: {
        description: "Disk I/O throughput and operations.",
        metrics: [
          { label: "Read/s", description: "Bytes read per second from disk" },
          { label: "Write/s", description: "Bytes written per second to disk" },
          { label: "R Ops/s", description: "Read operations per second" },
          { label: "W Ops/s", description: "Write operations per second" },
        ],
      },
      scheduler: {
        description: "CPU scheduling details.",
        metrics: [
          {
            label: "Nice",
            description: "Process scheduling priority (-20 to 19)",
          },
          {
            label: "VCtx/s",
            description: "Voluntary context switches (I/O waits)",
          },
          {
            label: "ICtx/s",
            description: "Involuntary context switches (CPU contention)",
          },
          {
            label: "RunDelay",
            description: "Time spent waiting in CPU run queue",
          },
        ],
      },
    },
  },

  pga: {
    label: "Activity",
    source: "pg_stat_activity",
    description:
      "Currently active PostgreSQL sessions. Primary tab for diagnosing active queries, blocked sessions, and connection usage.",
    howToRead:
      "Look for long query durations (>1s for OLTP). 'idle in transaction' sessions hold locks without working \u2014 dangerous for long periods. Wait events show what resources backends are stuck on. Use Stats view to correlate with historical pg_stat_statements data.",
    drillDown:
      "Navigate to PGS to see aggregated statistics for the selected query pattern.",
    views: {
      generic: {
        description: "All active sessions with OS metrics and wait events.",
        metrics: [
          {
            label: "CPU%",
            description: "CPU usage of backend OS process",
            thresholds: ">90% critical",
          },
          { label: "RSS", description: "Physical memory of backend process" },
          {
            label: "State",
            description: "Backend state",
            thresholds: "idle in transaction = warning",
          },
          {
            label: "Wait Type",
            description: "What resource the backend waits for",
            thresholds: "any = warning",
          },
          {
            label: "Query Dur",
            description: "Time since query started",
            thresholds: ">30s critical · 1-30s warning",
          },
          {
            label: "Xact Dur",
            description: "Time since transaction started",
            thresholds: ">60s critical · 5-60s warning",
          },
          {
            label: "Backend Type",
            description: "client backend, autovacuum, etc.",
          },
          {
            label: "Query",
            description: "Currently executing SQL (truncated)",
          },
        ],
      },
      stats: {
        description: "Sessions enriched with pg_stat_statements metrics.",
        metrics: [
          {
            label: "Avg Time",
            description: "Historical average execution time per call",
          },
          {
            label: "Max Time",
            description: "Historical maximum execution time",
          },
          {
            label: "Calls/s",
            description: "Execution frequency of this query pattern",
          },
          {
            label: "Hit%",
            description: "Buffer cache hit ratio for this query",
            thresholds: "\u226599% good · <90% critical",
          },
          { label: "Query", description: "Currently executing SQL" },
        ],
      },
    },
  },

  pgs: {
    label: "Statements",
    source: "pg_stat_statements",
    description:
      "Aggregated query statistics from pg_stat_statements. Shows performance per normalized query pattern.",
    howToRead:
      "Sort by Time/s to find queries consuming most CPU. Sort by Calls/s for high-frequency queries where even small improvements yield big impact. Check I/O view for queries with low HIT% \u2014 they do excessive physical reads. Temp view finds queries spilling to disk (increase work_mem).",
    views: {
      calls: {
        description: "Most frequently executed query patterns.",
        metrics: [
          { label: "Calls/s", description: "Executions per second" },
          { label: "Rows/s", description: "Rows returned per second" },
          { label: "R/Call", description: "Average rows per execution" },
          { label: "Avg Time", description: "Average execution time (ms)" },
          { label: "Query", description: "Normalized query text" },
        ],
      },
      time: {
        description: "Queries consuming the most execution time.",
        metrics: [
          { label: "Exec/s", description: "Total execution time per second" },
          { label: "Calls/s", description: "Executions per second" },
          { label: "Avg Time", description: "Average execution time (ms)" },
          { label: "Max Time", description: "Maximum execution time (ms)" },
          {
            label: "StdDev",
            description: "Standard deviation of execution time",
          },
          { label: "Query", description: "Normalized query text" },
        ],
      },
      io: {
        description: "Queries doing the most physical I/O.",
        metrics: [
          {
            label: "Blk Rd/s",
            description: "Shared blocks read from disk per second",
          },
          {
            label: "Blk Hit/s",
            description: "Shared blocks served from cache per second",
          },
          {
            label: "HIT%",
            description: "Buffer cache hit ratio",
            thresholds: "\u226599% good · 90-99% warning · <90% critical",
          },
          { label: "Dirty/s", description: "Blocks dirtied per second" },
          { label: "Query", description: "Normalized query text" },
        ],
      },
      temp: {
        description: "Queries using temporary files (work_mem overflow).",
        metrics: [
          { label: "Temp MB/s", description: "Temporary file throughput" },
          { label: "Temp Rd/s", description: "Temp blocks read per second" },
          { label: "Temp Wr/s", description: "Temp blocks written per second" },
          { label: "Query", description: "Normalized query text" },
        ],
      },
    },
  },

  pgt: {
    label: "Tables",
    source: "pg_stat_user_tables",
    description:
      "Table-level statistics \u2014 scan patterns, write activity, maintenance status, and I/O. Essential for identifying hot tables, missing indexes, and vacuum problems.",
    howToRead:
      "Scans view: high SEQ% on large tables = missing index. Maintenance view: high DEAD% = vacuum falling behind. I/O view: low HIT% = table doesn't fit in cache. Writes view: watch n_dead_tup growing \u2014 triggers autovacuum pressure.",
    drillDown: "Navigate to PGI to see indexes for the selected table.",
    views: {
      reads: {
        description: "Tables with highest read activity.",
        metrics: [
          {
            label: "Seq Rd/s",
            description: "Rows read via sequential scans per second",
          },
          {
            label: "Idx Fetch/s",
            description: "Rows fetched via index scans per second",
          },
          { label: "Tot Rd/s", description: "Total rows read per second" },
          {
            label: "HIT%",
            description: "Buffer cache hit ratio",
            thresholds: "\u226599% good · <90% critical",
          },
          { label: "Size", description: "Table size on disk" },
        ],
      },
      writes: {
        description: "Tables with highest write activity.",
        metrics: [
          { label: "Ins/s", description: "Rows inserted per second" },
          { label: "Upd/s", description: "Rows updated per second" },
          { label: "Del/s", description: "Rows deleted per second" },
          {
            label: "HOT Upd/s",
            description: "HOT updates (no index change needed)",
          },
          {
            label: "Dead Tup",
            description: "Dead rows waiting for vacuum",
            thresholds: ">100K critical · 1K-100K warning",
          },
        ],
      },
      scans: {
        description: "Sequential vs index scan ratio analysis.",
        metrics: [
          { label: "Seq Scan/s", description: "Sequential scans per second" },
          { label: "Idx Scan/s", description: "Index scans per second" },
          {
            label: "SEQ%",
            description: "Sequential scan ratio",
            thresholds: ">80% critical · 30-80% warning",
          },
          {
            label: "HIT%",
            description: "Buffer cache hit ratio",
            thresholds: "\u226599% good · <90% critical",
          },
          { label: "Size", description: "Table size on disk" },
        ],
      },
      maintenance: {
        description: "Vacuum and analyze status \u2014 dead tuples, bloat.",
        metrics: [
          {
            label: "Dead Tup",
            description: "Dead rows waiting for vacuum",
            thresholds: ">100K critical",
          },
          { label: "Live Tup", description: "Estimated live rows" },
          {
            label: "DEAD%",
            description: "Dead row ratio",
            thresholds: ">20% critical · 5-20% warning",
          },
          { label: "Vac/s", description: "Manual vacuum rate" },
          { label: "AutoVac", description: "Last autovacuum timestamp" },
        ],
      },
      io: {
        description: "Physical I/O by table \u2014 cache hit ratio.",
        metrics: [
          {
            label: "Heap Rd/s",
            description: "Heap blocks read from disk per second",
          },
          { label: "Heap Hit/s", description: "Heap blocks served from cache" },
          { label: "Idx Rd/s", description: "Index blocks read from disk" },
          { label: "Idx Hit/s", description: "Index blocks served from cache" },
          {
            label: "HIT%",
            description: "Overall buffer cache hit ratio",
            thresholds: "\u226599% good · <90% critical",
          },
        ],
      },
    },
  },

  pgi: {
    label: "Indexes",
    source: "pg_stat_user_indexes",
    description:
      "Index statistics \u2014 which indexes are actively used, unused (candidates for DROP), and their I/O patterns.",
    howToRead:
      "Unused view: sort by Idx Scans ascending \u2014 indexes with 0 scans waste space and slow writes. Consider DROP INDEX. Usage view: compare idx_tup_read_s vs idx_tup_fetch_s \u2014 large difference may indicate index bloat.",
    views: {
      usage: {
        description: "Active index usage \u2014 scans and tuple fetches.",
        metrics: [
          { label: "Idx Scan/s", description: "Index scans per second" },
          { label: "Tup Read/s", description: "Index tuples read per second" },
          {
            label: "Tup Fetch/s",
            description: "Index tuples fetched per second",
          },
          { label: "Size", description: "Index size on disk" },
        ],
      },
      unused: {
        description: "Indexes with zero scans \u2014 candidates for DROP.",
        metrics: [
          { label: "Idx Scans", description: "Total index scans (cumulative)" },
          {
            label: "Size",
            description: "Index size on disk (wasted if unused)",
          },
        ],
      },
      io: {
        description: "Index physical I/O \u2014 cache hit ratio.",
        metrics: [
          { label: "Blk Rd/s", description: "Index blocks read from disk" },
          { label: "Blk Hit/s", description: "Index blocks served from cache" },
          {
            label: "HIT%",
            description: "Index buffer cache hit ratio",
            thresholds: "\u226599% good · <90% critical",
          },
          { label: "Size", description: "Index size on disk" },
        ],
      },
    },
  },

  pge: {
    label: "Errors",
    source: "PostgreSQL stderr log parsing",
    description:
      "PostgreSQL log errors (ERROR/FATAL/PANIC) grouped by normalized pattern. Errors are accumulated within the current hour.",
    howToRead:
      "Sort by Count to find most frequent errors. PANIC = database crash, FATAL = connection terminated, ERROR = query failed. Check Sample column for concrete error message.",
    views: {
      default: {
        description: "All error patterns from PostgreSQL logs.",
        metrics: [
          { label: "Severity", description: "ERROR, FATAL, or PANIC" },
          {
            label: "Count",
            description: "Number of occurrences in current hour",
          },
          { label: "Pattern", description: "Normalized error pattern" },
          { label: "Sample", description: "One concrete error message" },
        ],
      },
    },
  },

  pgl: {
    label: "Locks",
    source: "pg_locks + pg_stat_activity",
    description:
      "Lock blocking tree. Shows which sessions block which other sessions. Only sessions involved in blocking chains appear.",
    howToRead:
      "Root blockers (depth=1) cause the most impact \u2014 investigate their queries first. 'idle in transaction' root blockers = application forgot to COMMIT/ROLLBACK. lock_granted=false (red) means the session is waiting. Drill-down to PGA for full session details.",
    drillDown: "Navigate to PGA for full session details of the selected PID.",
    views: {
      tree: {
        description: "Lock blocking tree \u2014 who blocks whom.",
        metrics: [
          { label: "PID", description: "Session PID with depth indentation" },
          {
            label: "Lock Mode",
            description: "Lock type being held or waited for",
          },
          {
            label: "Granted",
            description: "true=held, false=waiting",
            thresholds: "false = critical",
          },
          {
            label: "State",
            description: "Backend state",
            thresholds: "idle in transaction = warning",
          },
          { label: "Query", description: "Currently executing SQL" },
        ],
      },
    },
  },
};

export const SUMMARY_SECTION_HELP: Record<string, string> = {
  cpu: "CPU time breakdown across all cores. iow_pct >10% = disk bottleneck. steal >5% = hypervisor overcommit.",
  load: "Load average = processes waiting for CPU/IO. Compare to core count. >2x cores = overloaded.",
  memory:
    "available_kb is the key metric \u2014 includes reclaimable cache. <10% of total = memory pressure.",
  swap: "Any swap usage for PostgreSQL backends degrades performance severely. Ideally used_kb = 0.",
  psi: "Pressure Stall Information \u2014 measures actual resource contention, not just utilization.",
  vmstat:
    "Page faults and context switches. swin_s/swout_s >0 = active swapping (bad).",
  pg: "Database-wide throughput. hit_ratio <99% for OLTP is concerning. temp_bytes_s >0 = work_mem overflow.",
  bgwriter:
    "buffers_backend_s >0 means bgwriter can't keep up. maxwritten_clean high = increase bgwriter_lru_maxpages.",
  cgroup_cpu:
    "Container CPU usage vs limit. throttled_ms >0 = container exceeded quota, queries slowed down. Increase CPU limit or optimize queries.",
  cgroup_memory:
    "Container memory usage vs limit. used_pct >95% risks OOM kills. oom_kills >0 = processes were terminated. Increase memory limit.",
  cgroup_pids:
    "Container process/thread count vs limit. Approaching max prevents new connections and fork().",
};
