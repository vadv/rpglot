// Threshold-based cell coloring for DataTable.
//
// Colors use CSS custom properties for theme support:
//   critical  — var(--status-critical)  (needs attention)
//   warning   — var(--status-warning)   (elevated)
//   good      — var(--status-success)   (healthy)
//   inactive  — var(--status-inactive)  (zero / no activity)
//   default   — ""                      (inherit from row)

const LEVEL_CLASS: Record<string, string> = {
  critical: "text-[var(--status-critical)]",
  warning: "text-[var(--status-warning)]",
  good: "text-[var(--status-success)]",
  inactive: "text-[var(--status-inactive)]",
};

type Classifier = (
  value: unknown,
  row: Record<string, unknown>,
) => string | undefined;

// --- Numeric helpers ---

function pctHigh(
  v: unknown,
  goodBelow: number,
  warnBelow: number,
): string | undefined {
  if (v == null) return undefined;
  const n = Number(v);
  if (isNaN(n)) return undefined;
  if (n === 0) return "inactive";
  if (n < goodBelow) return undefined;
  if (n < warnBelow) return "warning";
  return "critical";
}

function pctHit(v: unknown): string | undefined {
  if (v == null) return undefined;
  const n = Number(v);
  if (isNaN(n)) return undefined;
  if (n >= 99) return "good";
  if (n >= 90) return "warning";
  return "critical";
}

function durationThreshold(
  v: unknown,
  warnSec: number,
  critSec: number,
): string | undefined {
  if (v == null) return undefined;
  const n = Number(v);
  if (isNaN(n)) return undefined;
  if (n === 0) return "inactive";
  if (n < warnSec) return undefined;
  if (n < critSec) return "warning";
  return "critical";
}

function rateInactive(v: unknown): string | undefined {
  if (v == null) return undefined;
  const n = Number(v);
  if (isNaN(n)) return undefined;
  if (n === 0) return "inactive";
  return undefined;
}

// --- Rules map ---

const RULES: Record<string, Classifier> = {
  // CPU / Memory percentage
  cpu_pct: (v) => pctHigh(v, 50, 90),
  mem_pct: (v) => pctHigh(v, 70, 90),

  // Hit ratios (inverted — higher is better)
  io_hit_pct: (v) => pctHit(v),
  hit_pct: (v) => pctHit(v),
  stmt_hit_pct: (v) => pctHit(v),

  // Dead tuples percentage
  dead_pct: (v) => {
    if (v == null) return undefined;
    const n = Number(v);
    if (isNaN(n)) return undefined;
    if (n === 0) return "inactive";
    if (n < 5) return undefined;
    if (n < 20) return "warning";
    return "critical";
  },

  // Sequential scan percentage (high = bad)
  seq_pct: (v) => {
    if (v == null) return undefined;
    const n = Number(v);
    if (isNaN(n)) return undefined;
    if (n === 0) return "inactive";
    if (n < 30) return undefined;
    if (n < 80) return "warning";
    return "critical";
  },

  // Disk reads (any physical reads = warning)
  disk_blks_read_s: (v) => {
    if (v == null) return undefined;
    const n = Number(v);
    if (isNaN(n)) return undefined;
    if (n === 0) return "inactive";
    return "warning";
  },

  // Query / transaction duration
  query_duration_s: (v) => durationThreshold(v, 1, 30),
  xact_duration_s: (v) => durationThreshold(v, 5, 60),

  // Wait event type (any wait = warning)
  wait_event_type: (v) => {
    if (v == null || v === "") return undefined;
    return "warning";
  },

  // PGA state
  state: (v) => {
    if (v === "idle in transaction") return "warning";
    if (v === "idle in transaction (aborted)") return "critical";
    return undefined;
  },

  // Lock granted
  lock_granted: (v) => {
    if (v === false) return "critical";
    return undefined;
  },

  // Dead tuples absolute
  n_dead_tup: (v) => {
    if (v == null) return undefined;
    const n = Number(v);
    if (isNaN(n)) return undefined;
    if (n === 0) return "inactive";
    if (n < 1000) return undefined;
    if (n < 100000) return "warning";
    return "critical";
  },

  // Rates — zero = inactive
  calls_s: rateInactive,
  rows_s: rateInactive,
  exec_time_ms_s: rateInactive,
  seq_scan_s: rateInactive,
  seq_tup_read_s: rateInactive,
  idx_scan_s: rateInactive,
  idx_tup_fetch_s: rateInactive,
  n_tup_ins_s: rateInactive,
  n_tup_upd_s: rateInactive,
  n_tup_del_s: rateInactive,
  n_tup_hot_upd_s: rateInactive,
  vacuum_count_s: rateInactive,
  autovacuum_count_s: rateInactive,
  shared_blks_read_s: rateInactive,
  shared_blks_hit_s: rateInactive,
  shared_blks_dirtied_s: rateInactive,
  shared_blks_written_s: rateInactive,
  heap_blks_read_s: rateInactive,
  heap_blks_hit_s: rateInactive,
  idx_blks_read_s: rateInactive,
  idx_blks_hit_s: rateInactive,
  idx_tup_read_s: rateInactive,
  read_bytes_s: rateInactive,
  write_bytes_s: rateInactive,
  read_ops_s: rateInactive,
  write_ops_s: rateInactive,
  nvcsw_s: rateInactive,
  nivcsw_s: rateInactive,
  tot_tup_read_s: rateInactive,
  toast_blks_read_s: rateInactive,
  toast_blks_hit_s: rateInactive,
  tidx_blks_read_s: rateInactive,
  tidx_blks_hit_s: rateInactive,
  analyze_count_s: rateInactive,
  autoanalyze_count_s: rateInactive,
  temp_blks_read_s: rateInactive,
  temp_blks_written_s: rateInactive,
  temp_mb_s: rateInactive,
  local_blks_read_s: rateInactive,
  local_blks_written_s: rateInactive,
  stmt_calls_s: rateInactive,
};

/**
 * Return a CSS color class for a cell value based on threshold rules.
 * Returns empty string if no special coloring applies.
 */
export function getThresholdClass(
  key: string,
  value: unknown,
  row: Record<string, unknown>,
): string {
  const rule = RULES[key];
  if (!rule) return "";
  const level = rule(value, row);
  if (!level) return "";
  return LEVEL_CLASS[level] ?? "";
}
