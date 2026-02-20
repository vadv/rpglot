import type { TabKey } from "../../api/types";

export type Severity = "info" | "warning" | "critical";

export interface AnalysisJump {
  timestamp: number;
  tab?: TabKey;
  view?: string;
  entityId?: number;
  filter?: string;
  columnFilter?: { column: string; value: string };
}

export const SEVERITY_ICON: Record<Severity, string> = {
  critical: "\uD83D\uDD34",
  warning: "\uD83D\uDFE1",
  info: "\uD83D\uDD35",
};

export const SEVERITY_LABEL: Record<Severity, string> = {
  critical: "Critical",
  warning: "Warning",
  info: "Info",
};

export const SEVERITY_COLOR: Record<Severity, string> = {
  critical: "var(--status-critical)",
  warning: "var(--status-warning)",
  info: "var(--status-info, var(--accent))",
};

/** Muted blue for persistent/background incidents */
export const PERSISTENT_COLOR = "var(--accent)";

export const CATEGORY_TAB: Record<string, TabKey> = {
  cpu: "prc",
  memory: "prc",
  disk: "prc",
  network: "prc",
  psi: "prc",
  cgroup: "prc",
  pg_activity: "pga",
  pg_statements: "pgs",
  pg_locks: "pgl",
  pg_tables: "pgt",
  pg_indexes: "pgi",
  pg_bgwriter: "pge",
  pg_events: "pge",
  pg_errors: "pge",
};

/** Per-rule target: which tab + view to open on jump. */
export const RULE_TARGET: Record<string, { tab: TabKey; view?: string }> = {
  // PGS
  stmt_mean_time_spike: { tab: "pgs" },
  stmt_call_spike: { tab: "pgs" },
  // PGT
  dead_tuples_high: { tab: "pgt" },
  seq_scan_dominant: { tab: "pgt", view: "scans" },
  heap_read_spike: { tab: "pgt", view: "io" },
  table_write_spike: { tab: "pgt", view: "writes" },
  cache_hit_ratio_drop: { tab: "pgt", view: "io" },
  // PGI
  index_read_spike: { tab: "pgi" },
  index_cache_miss: { tab: "pgi" },
  // PGA
  idle_in_transaction: { tab: "pga" },
  long_query: { tab: "pga" },
  wait_sync_replica: { tab: "pga" },
  wait_lock: { tab: "pga" },
  high_active_sessions: { tab: "pga" },
  tps_spike: { tab: "pga" },
  // PGL
  blocked_sessions: { tab: "pgl" },
  // PGE
  autovacuum_impact: { tab: "pge", view: "autovacuum" },
  pg_errors: { tab: "pge", view: "errors" },
  pg_fatal_panic: { tab: "pge", view: "errors" },
  checkpoint_spike: { tab: "pge", view: "checkpoints" },
  backend_buffers_high: { tab: "pge", view: "checkpoints" },
  // PRC — disk view for IO rules
  process_io_hog: { tab: "prc", view: "disk" },
  high_blk_delay: { tab: "prc", view: "disk" },
  // PRC — default CPU view for system rules
  cpu_high: { tab: "prc" },
  iowait_high: { tab: "prc" },
  steal_high: { tab: "prc" },
  memory_low: { tab: "prc" },
  swap_usage: { tab: "prc" },
  load_average_high: { tab: "prc" },
  disk_util_high: { tab: "prc" },
  disk_io_spike: { tab: "prc" },
  network_spike: { tab: "prc" },
  cgroup_throttled: { tab: "prc" },
  cgroup_oom_kill: { tab: "prc" },
};

/** Column filter for aggregate PGA rules (Group C). */
export const RULE_COLUMN_FILTER: Record<
  string,
  { column: string; value: string }
> = {
  idle_in_transaction: { column: "state", value: "idle in transaction" },
  wait_lock: { column: "wait_event_type", value: "Lock" },
  wait_sync_replica: { column: "wait_event", value: "SyncRep" },
  high_active_sessions: { column: "state", value: "active" },
};

export const CATEGORY_LABEL: Record<string, string> = {
  cpu: "CPU",
  memory: "Memory",
  disk: "Disk",
  network: "Network",
  psi: "PSI",
  cgroup: "Cgroup",
  pg_activity: "PG Activity",
  pg_statements: "PG Queries",
  pg_tables: "PG Tables",
  pg_indexes: "PG Indexes",
  pg_bgwriter: "PG BGWriter",
  pg_events: "PG Events",
  pg_locks: "PG Locks",
  pg_errors: "PG Errors",
};

/** Human-readable label for each rule_id. Ordered — determines lane order in timeline. */
export const RULE_LABEL: Record<string, string> = {
  cpu_high: "CPU high",
  iowait_high: "IO Wait",
  steal_high: "CPU steal",
  load_average_high: "Load avg",
  memory_low: "Memory",
  swap_usage: "Swap",
  disk_util_high: "Disk util",
  disk_io_spike: "Disk I/O",
  process_io_hog: "I/O hog",
  high_blk_delay: "I/O delay",
  autovacuum_impact: "Autovacuum",
  network_spike: "Network",
  cgroup_throttled: "Cgroup thr.",
  cgroup_oom_kill: "OOM kill",
  idle_in_transaction: "Idle in tx",
  long_query: "Long query",
  wait_sync_replica: "Sync repl.",
  wait_lock: "Lock wait",
  high_active_sessions: "Active sess.",
  tps_spike: "TPS spike",
  stmt_call_spike: "Query calls",
  stmt_mean_time_spike: "Query time",
  checkpoint_spike: "Checkpoint",
  backend_buffers_high: "Backend buf.",
  dead_tuples_high: "Dead tuples",
  seq_scan_dominant: "Seq scans",
  heap_read_spike: "Heap reads",
  table_write_spike: "Table writes",
  cache_hit_ratio_drop: "Cache miss",
  index_read_spike: "Idx reads",
  index_cache_miss: "Idx cache miss",
  blocked_sessions: "Blocked",
  pg_errors: "PG errors",
  pg_fatal_panic: "FATAL/PANIC",
};

/** Ordered list of rule_ids — determines lane order in timeline. */
export const RULE_ORDER = Object.keys(RULE_LABEL);
