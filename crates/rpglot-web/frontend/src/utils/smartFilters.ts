/** Safe numeric accessor — treats null/undefined/NaN as 0 */
export function num(v: unknown): number {
  if (v == null) return 0;
  const n = Number(v);
  return Number.isFinite(n) ? n : 0;
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
      return (
        (num(row.io_hit_pct) > 0 && num(row.io_hit_pct) < 90) ||
        num(row.disk_blks_read_s) > 0
      );
    case "writes":
      return num(row.dead_pct) > 5 || num(row.n_dead_tup) > 1000;
    case "scans":
      return (
        num(row.seq_pct) > 50 &&
        num(row.seq_scan_s) > 0 &&
        num(row.n_live_tup) > 10000
      );
    case "maintenance":
      return num(row.dead_pct) > 5 || num(row.n_dead_tup) > 10000;
    case "io":
      return (
        (num(row.io_hit_pct) > 0 && num(row.io_hit_pct) < 90) ||
        num(row.disk_blks_read_s) > 0
      );
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
        (num(row.io_hit_pct) > 0 && num(row.io_hit_pct) < 90) ||
        num(row.disk_blks_read_s) > 0
      );
    default:
      return true;
  }
}
