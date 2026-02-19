import { num } from "./smartFilters";
import type {
  ViewSchema,
  ColumnSchema,
  DataType,
  Unit,
  Format,
} from "../api/types";

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
  {
    key: "schema",
    label: "Schema",
    type: "string" as DataType,
    sortable: true,
    filterable: true,
  },
  {
    key: "tables",
    label: "Tables",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "size_bytes",
    label: "Size",
    type: "integer" as DataType,
    unit: "bytes" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "n_live_tup",
    label: "Live Tuples",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "n_dead_tup",
    label: "Dead Tuples",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "dead_pct",
    label: "DEAD%",
    type: "number" as DataType,
    unit: "percent" as Unit,
    format: "percent" as Format,
    sortable: true,
  },
  {
    key: "seq_scan_s",
    label: "Seq/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "idx_scan_s",
    label: "Idx/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "seq_pct",
    label: "SEQ%",
    type: "number" as DataType,
    unit: "percent" as Unit,
    format: "percent" as Format,
    sortable: true,
  },
  {
    key: "tup_read_s",
    label: "Tup Rd/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "ins_s",
    label: "Ins/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "upd_s",
    label: "Upd/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "del_s",
    label: "Del/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "blk_rd_s",
    label: "Disk Read/s",
    type: "number" as DataType,
    unit: "blks/s" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "blk_hit_s",
    label: "Buf Hit/s",
    type: "number" as DataType,
    unit: "blks/s" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "io_hit_pct",
    label: "HIT%",
    type: "number" as DataType,
    unit: "percent" as Unit,
    format: "percent" as Format,
    sortable: true,
  },
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
  {
    key: "database",
    label: "Database",
    type: "string" as DataType,
    sortable: true,
    filterable: true,
  },
  {
    key: "tables",
    label: "Tables",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "size_bytes",
    label: "Size",
    type: "integer" as DataType,
    unit: "bytes" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "n_live_tup",
    label: "Live Tuples",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "n_dead_tup",
    label: "Dead Tuples",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "dead_pct",
    label: "DEAD%",
    type: "number" as DataType,
    unit: "percent" as Unit,
    format: "percent" as Format,
    sortable: true,
  },
  {
    key: "seq_scan_s",
    label: "Seq/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "idx_scan_s",
    label: "Idx/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "seq_pct",
    label: "SEQ%",
    type: "number" as DataType,
    unit: "percent" as Unit,
    format: "percent" as Format,
    sortable: true,
  },
  {
    key: "tup_read_s",
    label: "Tup Rd/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "ins_s",
    label: "Ins/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "upd_s",
    label: "Upd/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "del_s",
    label: "Del/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "blk_rd_s",
    label: "Disk Read/s",
    type: "number" as DataType,
    unit: "blks/s" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "blk_hit_s",
    label: "Buf Hit/s",
    type: "number" as DataType,
    unit: "blks/s" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "io_hit_pct",
    label: "HIT%",
    type: "number" as DataType,
    unit: "percent" as Unit,
    format: "percent" as Format,
    sortable: true,
  },
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
  {
    key: "tablespace",
    label: "Tablespace",
    type: "string" as DataType,
    sortable: true,
    filterable: true,
  },
  {
    key: "tables",
    label: "Tables",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "size_bytes",
    label: "Size",
    type: "integer" as DataType,
    unit: "bytes" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "n_live_tup",
    label: "Live Tuples",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "n_dead_tup",
    label: "Dead Tuples",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "dead_pct",
    label: "DEAD%",
    type: "number" as DataType,
    unit: "percent" as Unit,
    format: "percent" as Format,
    sortable: true,
  },
  {
    key: "seq_scan_s",
    label: "Seq/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "idx_scan_s",
    label: "Idx/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "seq_pct",
    label: "SEQ%",
    type: "number" as DataType,
    unit: "percent" as Unit,
    format: "percent" as Format,
    sortable: true,
  },
  {
    key: "tup_read_s",
    label: "Tup Rd/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "ins_s",
    label: "Ins/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "upd_s",
    label: "Upd/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "del_s",
    label: "Del/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "blk_rd_s",
    label: "Disk Read/s",
    type: "number" as DataType,
    unit: "blks/s" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "blk_hit_s",
    label: "Buf Hit/s",
    type: "number" as DataType,
    unit: "blks/s" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "io_hit_pct",
    label: "HIT%",
    type: "number" as DataType,
    unit: "percent" as Unit,
    format: "percent" as Format,
    sortable: true,
  },
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
  {
    key: "schema",
    label: "Schema",
    type: "string" as DataType,
    sortable: true,
    filterable: true,
  },
  {
    key: "indexes",
    label: "Indexes",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "tables",
    label: "Tables",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "size_bytes",
    label: "Size",
    type: "integer" as DataType,
    unit: "bytes" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "idx_scan_s",
    label: "Scan/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "idx_tup_read_s",
    label: "Tup Rd/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "idx_tup_fetch_s",
    label: "Tup Ft/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "blk_rd_s",
    label: "Disk Read/s",
    type: "number" as DataType,
    unit: "blks/s" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "blk_hit_s",
    label: "Buf Hit/s",
    type: "number" as DataType,
    unit: "blks/s" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "io_hit_pct",
    label: "HIT%",
    type: "number" as DataType,
    unit: "percent" as Unit,
    format: "percent" as Format,
    sortable: true,
  },
  {
    key: "unused",
    label: "Unused",
    type: "integer" as DataType,
    sortable: true,
  },
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
  {
    key: "database",
    label: "Database",
    type: "string" as DataType,
    sortable: true,
    filterable: true,
  },
  {
    key: "indexes",
    label: "Indexes",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "tables",
    label: "Tables",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "size_bytes",
    label: "Size",
    type: "integer" as DataType,
    unit: "bytes" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "idx_scan_s",
    label: "Scan/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "idx_tup_read_s",
    label: "Tup Rd/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "idx_tup_fetch_s",
    label: "Tup Ft/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "blk_rd_s",
    label: "Disk Read/s",
    type: "number" as DataType,
    unit: "blks/s" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "blk_hit_s",
    label: "Buf Hit/s",
    type: "number" as DataType,
    unit: "blks/s" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "io_hit_pct",
    label: "HIT%",
    type: "number" as DataType,
    unit: "percent" as Unit,
    format: "percent" as Format,
    sortable: true,
  },
  {
    key: "unused",
    label: "Unused",
    type: "integer" as DataType,
    sortable: true,
  },
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
  {
    key: "tablespace",
    label: "Tablespace",
    type: "string" as DataType,
    sortable: true,
    filterable: true,
  },
  {
    key: "indexes",
    label: "Indexes",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "tables",
    label: "Tables",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "size_bytes",
    label: "Size",
    type: "integer" as DataType,
    unit: "bytes" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "idx_scan_s",
    label: "Scan/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "idx_tup_read_s",
    label: "Tup Rd/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "idx_tup_fetch_s",
    label: "Tup Ft/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "blk_rd_s",
    label: "Disk Read/s",
    type: "number" as DataType,
    unit: "blks/s" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "blk_hit_s",
    label: "Buf Hit/s",
    type: "number" as DataType,
    unit: "blks/s" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "io_hit_pct",
    label: "HIT%",
    type: "number" as DataType,
    unit: "percent" as Unit,
    format: "percent" as Format,
    sortable: true,
  },
  {
    key: "unused",
    label: "Unused",
    type: "integer" as DataType,
    sortable: true,
  },
];
