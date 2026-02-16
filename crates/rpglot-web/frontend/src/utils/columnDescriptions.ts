// Human-readable descriptions for table column headers.
// Shown as tooltips on hover.
//
// Descriptions are derived from COLUMN_HELP (single source of truth).
// EXTRA_DESCRIPTIONS covers columns that don't have rich help entries.

import { COLUMN_HELP } from "./columnHelp";

const EXTRA_DESCRIPTIONS: Record<string, string> = {
  // === PRC (OS Processes) ===
  ppid: "Parent process ID",
  name: "Process name (comm)",
  vsize_kb: "Virtual memory size (total address space)",
  rsize_kb: "Resident set size (physical memory used)",
  vgrow_kb: "Virtual memory growth since last sample",
  rgrow_kb: "Resident memory growth since last sample",
  psize_kb: "Proportional set size (PSS)",
  vstext_kb: "Code segment size",
  vdata_kb: "Data segment size",
  vstack_kb: "Stack size",
  vslibs_kb: "Shared libraries size",
  vlock_kb: "Locked memory size",
  read_bytes_s: "Disk read throughput (bytes/s)",
  write_bytes_s: "Disk write throughput (bytes/s)",
  read_ops_s: "Disk read operations per second",
  write_ops_s: "Disk write operations per second",
  total_read_bytes: "Total bytes read (cumulative)",
  total_write_bytes: "Total bytes written (cumulative)",
  total_read_ops: "Total read operations (cumulative)",
  total_write_ops: "Total write operations (cumulative)",
  cancelled_write_bytes: "Cancelled write bytes (truncated files)",
  uid: "Real user ID",
  euid: "Effective user ID",
  gid: "Real group ID",
  egid: "Effective group ID",
  num_threads: "Number of threads",
  curcpu: "CPU core currently running on",
  nice: "Nice value (-20 to 19)",
  priority: "Scheduling priority",
  rtprio: "Real-time scheduling priority",
  policy: "Scheduling policy (0=normal, 1=FIFO, 2=RR)",
  blkdelay: "Block I/O delay (ticks)",
  minflt: "Minor page faults (no disk I/O)",
  majflt: "Major page faults (required disk I/O)",
  tty: "Terminal device number",
  exit_signal: "Signal sent to parent on exit",
  pg_query: "PostgreSQL query (if PG backend)",
  pg_backend_type: "PostgreSQL backend type (if PG backend)",

  // === PGA (pg_stat_activity) ===
  rchar_s: "Read syscall bytes/s (includes page cache hits)",
  wchar_s: "Write syscall bytes/s (includes page cache)",

  // === PGT/PGI ===
  relid: "Table OID",
  schema: "Schema name",
  table: "Table name",

  // === Schema views (client-side aggregation) ===
  tables: "Number of tables in schema",
  indexes: "Number of indexes in schema",
  unused: "Number of indexes with zero scans",
  tup_read_s: "Total tuples read per second (seq + idx)",
  ins_s: "Total inserts per second across schema",
  upd_s: "Total updates per second across schema",
  del_s: "Total deletes per second across schema",
  blk_rd_s: "Total disk reads per second (heap + idx)",
  blk_hit_s: "Total buffer hits per second (heap + idx)",
};

export const COLUMN_DESCRIPTIONS: Record<string, string> = {
  ...Object.fromEntries(
    Object.entries(COLUMN_HELP).map(([k, v]) => [k, v.description]),
  ),
  ...EXTRA_DESCRIPTIONS,
};
