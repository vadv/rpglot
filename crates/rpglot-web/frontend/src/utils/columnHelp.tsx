import type { ReactNode } from "react";

interface ColumnHelpEntry {
  label: string;
  description: string;
  thresholds?: string;
  tip?: string;
}

export const COLUMN_HELP: Record<string, ColumnHelpEntry> = {
  // CPU / Memory
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

  // Hit ratios
  io_hit_pct: {
    label: "HIT%",
    description: "Buffer cache hit ratio for all I/O (heap + index + toast).",
    thresholds: "\u226599% good \u00b7 90-99% warning \u00b7 <90% critical",
    tip: "Consider increasing shared_buffers",
  },
  hit_pct: {
    label: "HIT%",
    description: "Buffer cache hit ratio (shared_blks_hit / total).",
    thresholds: "\u226599% good \u00b7 90-99% warning \u00b7 <90% critical",
    tip: "Query does excessive physical I/O",
  },
  stmt_hit_pct: {
    label: "Hit%",
    description: "Statement buffer cache hit ratio from pg_stat_statements.",
    thresholds: "\u226599% good \u00b7 90-99% warning \u00b7 <90% critical",
    tip: "Statement frequently misses buffer cache",
  },

  // Table health
  dead_pct: {
    label: "DEAD%",
    description: "Dead rows as percentage of total (live + dead).",
    thresholds: ">20% critical \u00b7 5-20% warning \u00b7 <5% normal",
    tip: "VACUUM urgently needed when high",
  },
  seq_pct: {
    label: "SEQ%",
    description: "Sequential scan ratio (seq_scan / total scans).",
    thresholds: ">80% critical \u00b7 30-80% warning \u00b7 <30% normal",
    tip: "Missing index on frequently scanned table",
  },
  n_dead_tup: {
    label: "Dead Tuples",
    description: "Estimated number of dead (unvacuumed) rows.",
    thresholds: ">100K critical \u00b7 1K-100K warning \u00b7 <1K normal",
    tip: "Dead rows consume space and slow down scans",
  },
  hot_pct: {
    label: "HOT%",
    description: "HOT update ratio \u2014 updates not requiring index changes.",
    tip: "Higher is better. Low HOT% suggests fillfactor tuning",
  },

  // Durations
  query_duration_s: {
    label: "Query Duration",
    description: "Time since current query started.",
    thresholds: ">30s critical \u00b7 1-30s warning \u00b7 <1s normal",
    tip: "Compare with stmt_mean to detect anomalies",
  },
  xact_duration_s: {
    label: "Transaction Duration",
    description: "Time since current transaction started.",
    thresholds: ">60s critical \u00b7 5-60s warning \u00b7 <5s normal",
    tip: "Long transactions hold locks and prevent vacuum",
  },

  // Disk I/O
  disk_blks_read_s: {
    label: "DISK/s",
    description: "Total disk blocks read per second (physical I/O).",
    thresholds: ">0 warning \u00b7 =0 inactive",
    tip: "Any physical reads reduce performance",
  },

  // Locks
  lock_granted: {
    label: "Granted",
    description: "Whether the lock is held (true) or being waited for (false).",
    thresholds: "false = critical (session blocked)",
    tip: "Find root blocker (depth=1) and investigate their query",
  },
  state: {
    label: "State",
    description: "Backend state: active, idle, idle in transaction, etc.",
    thresholds: "idle in transaction = warning \u00b7 aborted = critical",
    tip: "Idle-in-transaction sessions hold locks without working",
  },
  wait_event_type: {
    label: "Wait Type",
    description: "Type of resource the backend is waiting for.",
    thresholds: "any wait = warning",
    tip: "Lock, IO, LWLock \u2014 check wait_event for details",
  },

  // Statement rates
  calls_s: {
    label: "Calls/s",
    description: "Query executions per second.",
    tip: "High-frequency queries have biggest optimization impact",
  },
  rows_per_call: {
    label: "R/Call",
    description: "Average rows returned per execution.",
    tip: "High values may indicate missing LIMIT or WHERE clause",
  },
  mean_exec_time_ms: {
    label: "Avg Time",
    description: "Average execution time per call (ms).",
    tip: "Primary optimization target. Compare across time",
  },
  temp_mb_s: {
    label: "Temp MB/s",
    description: "Temp file throughput \u2014 sorts/joins spilling to disk.",
    tip: "Increase work_mem for this query",
  },

  // Process scheduling
  nvcsw_s: {
    label: "VCtx/s",
    description: "Voluntary context switches per second.",
    tip: "High values indicate I/O-bound process",
  },
  nivcsw_s: {
    label: "ICtx/s",
    description: "Involuntary context switches per second.",
    tip: "High values indicate CPU contention",
  },

  // OS memory
  rss_kb: {
    label: "RSS",
    description: "Resident set size \u2014 physical memory of backend process.",
    tip: "High RSS = large work_mem operation or maintenance_work_mem",
  },
};

export function buildColumnTooltip(key: string): ReactNode | null {
  const help = COLUMN_HELP[key];
  if (!help) return null;
  return (
    <div className="space-y-1">
      <div className="font-semibold text-[var(--text-primary)]">
        {help.label}
      </div>
      <div className="text-[var(--text-secondary)]">{help.description}</div>
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
  },
  pgi: {
    usage: "Active index usage \u2014 scans and tuple fetches",
    unused: "Indexes with zero scans \u2014 candidates for DROP",
    io: "Index physical I/O \u2014 cache hit ratio",
  },
  pgl: {
    tree: "Lock blocking tree \u2014 who blocks whom",
  },
};
