import { getThresholdLevel } from "./thresholds";

/** Safe numeric accessor — treats null/undefined/NaN as 0 */
export function num(v: unknown): number {
  if (v == null) return 0;
  const n = Number(v);
  return Number.isFinite(n) ? n : 0;
}

/** Check if a threshold returns a problem level (warning or critical). */
function isExceeded(key: string, row: Record<string, unknown>): boolean {
  const level = getThresholdLevel(key, row[key], row);
  return level === "warning" || level === "critical";
}

/** PRC: returns true if process is a PostgreSQL backend */
export function isPgProcess(row: Record<string, unknown>): boolean {
  const bt = row.pg_backend_type;
  return typeof bt === "string" && bt.length > 0;
}

/** PGT: returns true if table has problems on given view */
export function isPgtProblematic(
  row: Record<string, unknown>,
  view: string,
): boolean {
  switch (view) {
    case "reads":
    case "io":
      return (
        isExceeded("io_hit_pct", row) || isExceeded("disk_blks_read_s", row)
      );
    case "writes":
      return isExceeded("dead_pct", row) || isExceeded("n_dead_tup", row);
    case "scans":
      return (
        isExceeded("seq_pct", row) &&
        num(row.seq_scan_s) > 0 &&
        num(row.n_live_tup) > 10000
      );
    case "maintenance":
      return isExceeded("dead_pct", row) || isExceeded("n_dead_tup", row);
    default:
      return true; // unknown view — don't filter
  }
}

/** PGI: returns true if index has problems on given view */
export function isPgiProblematic(
  row: Record<string, unknown>,
  view: string,
): boolean {
  switch (view) {
    case "usage":
      return num(row.idx_scan) === 0 || num(row.idx_scan_s) === 0;
    case "unused":
      return true; // already shows only unused — don't filter
    case "io":
      return (
        isExceeded("io_hit_pct", row) || isExceeded("disk_blks_read_s", row)
      );
    default:
      return true;
  }
}
