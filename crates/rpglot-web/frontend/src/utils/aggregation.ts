import { num } from "./smartFilters";
import type {
  ViewSchema,
  ColumnSchema,
  DataType,
  Unit,
  Format,
} from "../api/types";

// ============================================================
// Column definition helpers — reduce boilerplate for ColumnSchema arrays
// ============================================================

const col = (
  key: string,
  label: string,
  opts?: Partial<ColumnSchema>,
): ColumnSchema => ({
  key,
  label,
  type: "number" as DataType,
  sortable: true,
  ...opts,
});

const strCol = (key: string, label: string): ColumnSchema =>
  col(key, label, { type: "string" as DataType, filterable: true });

const intCol = (key: string, label: string): ColumnSchema =>
  col(key, label, { type: "integer" as DataType });

const pctCol = (key: string, label: string): ColumnSchema =>
  col(key, label, {
    unit: "percent" as Unit,
    format: "percent" as Format,
  });

const rateCol = (key: string, label: string): ColumnSchema =>
  col(key, label, {
    unit: "per_sec" as Unit,
    format: "rate" as Format,
  });

const bytesCol = (key: string, label: string): ColumnSchema =>
  col(key, label, {
    type: "integer" as DataType,
    unit: "bytes" as Unit,
    format: "bytes" as Format,
  });

const blksCol = (key: string, label: string): ColumnSchema =>
  col(key, label, {
    unit: "blks/s" as Unit,
    format: "bytes" as Format,
  });

const durationCol = (key: string, label: string): ColumnSchema =>
  col(key, label, {
    unit: "ms" as Unit,
    format: "duration" as Format,
  });

// ============================================================
// PGP Regression view — detect plan regressions per query
// ============================================================

export const PGP_REGRESSION_COLUMNS: ColumnSchema[] = [
  strCol("stmt_queryid", "Query ID"),
  col("time_ratio", "Ratio"),
  durationCol("mean_time_ms", "Mean Time"),
  rateCol("calls_s", "Calls/s"),
  strCol("first_call", "First Call"),
  strCol("last_call", "Last Call"),
  strCol("database", "Database"),
  col("plan", "Plan", { type: "string" as DataType, sortable: false }),
];

/**
 * For each stmt_queryid with 2+ plans, compute time_ratio = mean_time_ms / min(mean_time_ms).
 * Only include groups where max/min ratio >= 2 (actual regressions).
 */
export function computePgpRegression(
  rows: Record<string, unknown>[],
): Record<string, unknown>[] {
  // Group by stmt_queryid (skip 0 / null)
  const groups = new Map<string, Record<string, unknown>[]>();
  for (const row of rows) {
    const qid = row.stmt_queryid;
    if (qid == null || qid === 0 || qid === "0") continue;
    const key = String(qid);
    let arr = groups.get(key);
    if (!arr) {
      arr = [];
      groups.set(key, arr);
    }
    arr.push(row);
  }

  const result: Record<string, unknown>[] = [];

  for (const plans of groups.values()) {
    if (plans.length < 2) continue;

    // Find min and max mean_time_ms among plans with mean_time_ms > 0
    let minMean = Infinity;
    let maxMean = 0;
    for (const p of plans) {
      const m = num(p.mean_time_ms);
      if (m > 0) {
        if (m < minMean) minMean = m;
        if (m > maxMean) maxMean = m;
      }
    }

    if (minMean === Infinity || minMean === 0) continue;
    if (maxMean / minMean < 2) continue;

    for (const p of plans) {
      const m = num(p.mean_time_ms);
      result.push({
        ...p,
        time_ratio: m > 0 ? m / minMean : null,
      });
    }
  }

  return result;
}

// ============================================================
// PGT Schema view — client-side aggregation by schema
// ============================================================

export const SCHEMA_VIEW: ViewSchema = {
  key: "schema",
  label: "Schema",
  columns: [
    "schema",
    "tables",
    "size_bytes",
    "n_live_tup",
    "n_dead_tup",
    "dead_pct",
    "seq_scan_s",
    "idx_scan_s",
    "seq_pct",
    "tup_read_s",
    "ins_s",
    "upd_s",
    "del_s",
    "blk_rd_s",
    "blk_hit_s",
    "io_hit_pct",
  ],
  default: false,
  default_sort: "blk_rd_s",
  default_sort_desc: true,
};

export const SCHEMA_COLUMNS: ColumnSchema[] = [
  strCol("schema", "Schema"),
  intCol("tables", "Tables"),
  bytesCol("size_bytes", "Size"),
  intCol("n_live_tup", "Live Tuples"),
  intCol("n_dead_tup", "Dead Tuples"),
  pctCol("dead_pct", "DEAD%"),
  rateCol("seq_scan_s", "Seq/s"),
  rateCol("idx_scan_s", "Idx/s"),
  pctCol("seq_pct", "SEQ%"),
  rateCol("tup_read_s", "Tup Rd/s"),
  rateCol("ins_s", "Ins/s"),
  rateCol("upd_s", "Upd/s"),
  rateCol("del_s", "Del/s"),
  blksCol("blk_rd_s", "Disk Read/s"),
  blksCol("blk_hit_s", "Buf Hit/s"),
  pctCol("io_hit_pct", "HIT%"),
];

/** Aggregate PGT rows by a grouping field (schema or database) */
export function aggregateTableRows(
  rows: Record<string, unknown>[],
  groupBy: string,
): Record<string, unknown>[] {
  const map = new Map<
    string,
    {
      tables: number;
      size_bytes: number;
      n_live_tup: number;
      n_dead_tup: number;
      seq_scan_s: number;
      idx_scan_s: number;
      seq_tup_read_s: number;
      idx_tup_fetch_s: number;
      ins_s: number;
      upd_s: number;
      del_s: number;
      heap_blks_read_s: number;
      heap_blks_hit_s: number;
      idx_blks_read_s: number;
      idx_blks_hit_s: number;
    }
  >();

  for (const row of rows) {
    const key = String(row[groupBy] ?? "unknown");
    let agg = map.get(key);
    if (!agg) {
      agg = {
        tables: 0,
        size_bytes: 0,
        n_live_tup: 0,
        n_dead_tup: 0,
        seq_scan_s: 0,
        idx_scan_s: 0,
        seq_tup_read_s: 0,
        idx_tup_fetch_s: 0,
        ins_s: 0,
        upd_s: 0,
        del_s: 0,
        heap_blks_read_s: 0,
        heap_blks_hit_s: 0,
        idx_blks_read_s: 0,
        idx_blks_hit_s: 0,
      };
      map.set(key, agg);
    }
    agg.tables += 1;
    agg.size_bytes += num(row.size_bytes);
    agg.n_live_tup += num(row.n_live_tup);
    agg.n_dead_tup += num(row.n_dead_tup);
    agg.seq_scan_s += num(row.seq_scan_s);
    agg.idx_scan_s += num(row.idx_scan_s);
    agg.seq_tup_read_s += num(row.seq_tup_read_s);
    agg.idx_tup_fetch_s += num(row.idx_tup_fetch_s);
    agg.ins_s += num(row.n_tup_ins_s);
    agg.upd_s += num(row.n_tup_upd_s);
    agg.del_s += num(row.n_tup_del_s);
    agg.heap_blks_read_s += num(row.heap_blks_read_s);
    agg.heap_blks_hit_s += num(row.heap_blks_hit_s);
    agg.idx_blks_read_s += num(row.idx_blks_read_s);
    agg.idx_blks_hit_s += num(row.idx_blks_hit_s);
  }

  const result: Record<string, unknown>[] = [];
  for (const [key, agg] of map) {
    const totalTup = agg.n_live_tup + agg.n_dead_tup;
    const totalScans = agg.seq_scan_s + agg.idx_scan_s;
    const totalReads = agg.heap_blks_read_s + agg.idx_blks_read_s;
    const totalHits = agg.heap_blks_hit_s + agg.idx_blks_hit_s;
    const totalIO = totalReads + totalHits;

    result.push({
      [groupBy]: key,
      tables: agg.tables,
      size_bytes: agg.size_bytes,
      n_live_tup: agg.n_live_tup,
      n_dead_tup: agg.n_dead_tup,
      dead_pct: totalTup > 0 ? (agg.n_dead_tup / totalTup) * 100 : null,
      seq_scan_s: agg.seq_scan_s,
      idx_scan_s: agg.idx_scan_s,
      seq_pct: totalScans > 0 ? (agg.seq_scan_s / totalScans) * 100 : null,
      tup_read_s: agg.seq_tup_read_s + agg.idx_tup_fetch_s,
      ins_s: agg.ins_s,
      upd_s: agg.upd_s,
      del_s: agg.del_s,
      blk_rd_s: totalReads,
      blk_hit_s: totalHits,
      io_hit_pct: totalIO > 0 ? (totalHits / totalIO) * 100 : null,
    });
  }
  return result;
}

// ============================================================
// PGT Database view — client-side aggregation by database
// ============================================================

export const DATABASE_VIEW: ViewSchema = {
  key: "database",
  label: "Database",
  columns: [
    "database",
    "tables",
    "size_bytes",
    "n_live_tup",
    "n_dead_tup",
    "dead_pct",
    "seq_scan_s",
    "idx_scan_s",
    "seq_pct",
    "tup_read_s",
    "ins_s",
    "upd_s",
    "del_s",
    "blk_rd_s",
    "blk_hit_s",
    "io_hit_pct",
  ],
  default: false,
  default_sort: "blk_rd_s",
  default_sort_desc: true,
};

export const DATABASE_COLUMNS: ColumnSchema[] = [
  strCol("database", "Database"),
  intCol("tables", "Tables"),
  bytesCol("size_bytes", "Size"),
  intCol("n_live_tup", "Live Tuples"),
  intCol("n_dead_tup", "Dead Tuples"),
  pctCol("dead_pct", "DEAD%"),
  rateCol("seq_scan_s", "Seq/s"),
  rateCol("idx_scan_s", "Idx/s"),
  pctCol("seq_pct", "SEQ%"),
  rateCol("tup_read_s", "Tup Rd/s"),
  rateCol("ins_s", "Ins/s"),
  rateCol("upd_s", "Upd/s"),
  rateCol("del_s", "Del/s"),
  blksCol("blk_rd_s", "Disk Read/s"),
  blksCol("blk_hit_s", "Buf Hit/s"),
  pctCol("io_hit_pct", "HIT%"),
];

// ============================================================
// PGT Tablespace view — client-side aggregation by tablespace
// ============================================================

export const TABLESPACE_VIEW: ViewSchema = {
  key: "tablespace",
  label: "Tablespace",
  columns: [
    "tablespace",
    "tables",
    "size_bytes",
    "n_live_tup",
    "n_dead_tup",
    "dead_pct",
    "seq_scan_s",
    "idx_scan_s",
    "seq_pct",
    "tup_read_s",
    "ins_s",
    "upd_s",
    "del_s",
    "blk_rd_s",
    "blk_hit_s",
    "io_hit_pct",
  ],
  default: false,
  default_sort: "blk_rd_s",
  default_sort_desc: true,
};

export const TABLESPACE_COLUMNS: ColumnSchema[] = [
  strCol("tablespace", "Tablespace"),
  intCol("tables", "Tables"),
  bytesCol("size_bytes", "Size"),
  intCol("n_live_tup", "Live Tuples"),
  intCol("n_dead_tup", "Dead Tuples"),
  pctCol("dead_pct", "DEAD%"),
  rateCol("seq_scan_s", "Seq/s"),
  rateCol("idx_scan_s", "Idx/s"),
  pctCol("seq_pct", "SEQ%"),
  rateCol("tup_read_s", "Tup Rd/s"),
  rateCol("ins_s", "Ins/s"),
  rateCol("upd_s", "Upd/s"),
  rateCol("del_s", "Del/s"),
  blksCol("blk_rd_s", "Disk Read/s"),
  blksCol("blk_hit_s", "Buf Hit/s"),
  pctCol("io_hit_pct", "HIT%"),
];

// ============================================================
// PGI Schema view — client-side aggregation by schema
// ============================================================

export const PGI_SCHEMA_VIEW: ViewSchema = {
  key: "schema",
  label: "Schema",
  columns: [
    "schema",
    "indexes",
    "tables",
    "size_bytes",
    "idx_scan_s",
    "idx_tup_read_s",
    "idx_tup_fetch_s",
    "blk_rd_s",
    "blk_hit_s",
    "io_hit_pct",
    "unused",
  ],
  default: false,
  default_sort: "blk_rd_s",
  default_sort_desc: true,
};

export const PGI_SCHEMA_COLUMNS: ColumnSchema[] = [
  strCol("schema", "Schema"),
  intCol("indexes", "Indexes"),
  intCol("tables", "Tables"),
  bytesCol("size_bytes", "Size"),
  rateCol("idx_scan_s", "Scan/s"),
  rateCol("idx_tup_read_s", "Tup Rd/s"),
  rateCol("idx_tup_fetch_s", "Tup Ft/s"),
  blksCol("blk_rd_s", "Disk Read/s"),
  blksCol("blk_hit_s", "Buf Hit/s"),
  pctCol("io_hit_pct", "HIT%"),
  intCol("unused", "Unused"),
];

/** Aggregate PGI rows by a grouping field (schema or database) */
export function aggregateIndexRows(
  rows: Record<string, unknown>[],
  groupBy: string,
): Record<string, unknown>[] {
  const map = new Map<
    string,
    {
      indexes: number;
      relids: Set<number>;
      size_bytes: number;
      idx_scan_s: number;
      idx_tup_read_s: number;
      idx_tup_fetch_s: number;
      idx_blks_read_s: number;
      idx_blks_hit_s: number;
      unused: number;
    }
  >();

  for (const row of rows) {
    const key = String(row[groupBy] ?? "unknown");
    let agg = map.get(key);
    if (!agg) {
      agg = {
        indexes: 0,
        relids: new Set(),
        size_bytes: 0,
        idx_scan_s: 0,
        idx_tup_read_s: 0,
        idx_tup_fetch_s: 0,
        idx_blks_read_s: 0,
        idx_blks_hit_s: 0,
        unused: 0,
      };
      map.set(key, agg);
    }
    agg.indexes += 1;
    agg.relids.add(num(row.relid));
    agg.size_bytes += num(row.size_bytes);
    agg.idx_scan_s += num(row.idx_scan_s);
    agg.idx_tup_read_s += num(row.idx_tup_read_s);
    agg.idx_tup_fetch_s += num(row.idx_tup_fetch_s);
    agg.idx_blks_read_s += num(row.idx_blks_read_s);
    agg.idx_blks_hit_s += num(row.idx_blks_hit_s);
    if (num(row.idx_scan) === 0) agg.unused += 1;
  }

  const result: Record<string, unknown>[] = [];
  for (const [key, agg] of map) {
    const totalIO = agg.idx_blks_read_s + agg.idx_blks_hit_s;

    result.push({
      [groupBy]: key,
      indexes: agg.indexes,
      tables: agg.relids.size,
      size_bytes: agg.size_bytes,
      idx_scan_s: agg.idx_scan_s,
      idx_tup_read_s: agg.idx_tup_read_s,
      idx_tup_fetch_s: agg.idx_tup_fetch_s,
      blk_rd_s: agg.idx_blks_read_s,
      blk_hit_s: agg.idx_blks_hit_s,
      io_hit_pct: totalIO > 0 ? (agg.idx_blks_hit_s / totalIO) * 100 : null,
      unused: agg.unused,
    });
  }
  return result;
}

// ============================================================
// PGI Database view — client-side aggregation by database
// ============================================================

export const PGI_DATABASE_VIEW: ViewSchema = {
  key: "database",
  label: "Database",
  columns: [
    "database",
    "indexes",
    "tables",
    "size_bytes",
    "idx_scan_s",
    "idx_tup_read_s",
    "idx_tup_fetch_s",
    "blk_rd_s",
    "blk_hit_s",
    "io_hit_pct",
    "unused",
  ],
  default: false,
  default_sort: "blk_rd_s",
  default_sort_desc: true,
};

export const PGI_DATABASE_COLUMNS: ColumnSchema[] = [
  strCol("database", "Database"),
  intCol("indexes", "Indexes"),
  intCol("tables", "Tables"),
  bytesCol("size_bytes", "Size"),
  rateCol("idx_scan_s", "Scan/s"),
  rateCol("idx_tup_read_s", "Tup Rd/s"),
  rateCol("idx_tup_fetch_s", "Tup Ft/s"),
  blksCol("blk_rd_s", "Disk Read/s"),
  blksCol("blk_hit_s", "Buf Hit/s"),
  pctCol("io_hit_pct", "HIT%"),
  intCol("unused", "Unused"),
];

// ============================================================
// PGI Tablespace view — client-side aggregation by tablespace
// ============================================================

export const PGI_TABLESPACE_VIEW: ViewSchema = {
  key: "tablespace",
  label: "Tablespace",
  columns: [
    "tablespace",
    "indexes",
    "tables",
    "size_bytes",
    "idx_scan_s",
    "idx_tup_read_s",
    "idx_tup_fetch_s",
    "blk_rd_s",
    "blk_hit_s",
    "io_hit_pct",
    "unused",
  ],
  default: false,
  default_sort: "blk_rd_s",
  default_sort_desc: true,
};

export const PGI_TABLESPACE_COLUMNS: ColumnSchema[] = [
  strCol("tablespace", "Tablespace"),
  intCol("indexes", "Indexes"),
  intCol("tables", "Tables"),
  bytesCol("size_bytes", "Size"),
  rateCol("idx_scan_s", "Scan/s"),
  rateCol("idx_tup_read_s", "Tup Rd/s"),
  rateCol("idx_tup_fetch_s", "Tup Ft/s"),
  blksCol("blk_rd_s", "Disk Read/s"),
  blksCol("blk_hit_s", "Buf Hit/s"),
  pctCol("io_hit_pct", "HIT%"),
  intCol("unused", "Unused"),
];
