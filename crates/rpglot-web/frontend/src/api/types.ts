// Mirror of rpglot-core/src/api/snapshot.rs + schema.rs

// ============================================================
// Snapshot
// ============================================================

export interface SessionCounts {
  active: number;
  idle: number;
  idle_in_transaction: number;
  total: number;
}

export interface ApiSnapshot {
  timestamp: number;
  prev_timestamp?: number;
  next_timestamp?: number;
  system: SystemSummary;
  pg: PgSummary;
  prc: ApiProcessRow[];
  pga: PgActivityRow[];
  pgs: PgStatementsRow[];
  pgt: PgTablesRow[];
  pgi: PgIndexesRow[];
  pge: PgEventsRow[];
  pgl: PgLocksRow[];
  health_score: number;
  health_breakdown: HealthBreakdown;
  session_counts: SessionCounts;
}

export interface HealthBreakdown {
  sessions: number;
  cpu: number;
  disk_iops: number;
  disk_bw: number;
}

export interface SystemSummary {
  cpu: CpuSummary | null;
  load: LoadSummary | null;
  memory: MemorySummary | null;
  swap: SwapSummary | null;
  disks: DiskSummary[];
  networks: NetworkSummary[];
  psi: PsiSummary | null;
  vmstat: VmstatSummary | null;
  cgroup_cpu: CgroupCpuSummary | null;
  cgroup_memory: CgroupMemorySummary | null;
  cgroup_pids: CgroupPidsSummary | null;
}

export interface CpuSummary {
  sys_pct: number;
  usr_pct: number;
  irq_pct: number;
  iow_pct: number;
  idle_pct: number;
  steal_pct: number;
}

export interface LoadSummary {
  avg1: number;
  avg5: number;
  avg15: number;
  nr_threads: number;
  nr_running: number;
}

export interface MemorySummary {
  total_kb: number;
  available_kb: number;
  cached_kb: number;
  buffers_kb: number;
  slab_kb: number;
}

export interface SwapSummary {
  total_kb: number;
  free_kb: number;
  used_kb: number;
  dirty_kb: number;
  writeback_kb: number;
}

export interface DiskSummary {
  name: string;
  read_bytes_s: number;
  write_bytes_s: number;
  read_iops: number;
  write_iops: number;
  util_pct: number;
  r_await_ms: number;
  w_await_ms: number;
}

export interface NetworkSummary {
  name: string;
  rx_bytes_s: number;
  tx_bytes_s: number;
  rx_packets_s: number;
  tx_packets_s: number;
  errors_s: number;
  drops_s: number;
}

export interface PsiSummary {
  cpu_some_pct: number;
  mem_some_pct: number;
  io_some_pct: number;
}

export interface VmstatSummary {
  pgin_s: number;
  pgout_s: number;
  swin_s: number;
  swout_s: number;
  pgfault_s: number;
  ctxsw_s: number;
}

export interface CgroupCpuSummary {
  limit_cores: number;
  used_pct: number;
  usr_pct: number;
  sys_pct: number;
  throttled_ms: number;
  nr_throttled: number;
}

export interface CgroupMemorySummary {
  limit_bytes: number;
  used_bytes: number;
  used_pct: number;
  anon_bytes: number;
  file_bytes: number;
  slab_bytes: number;
  oom_kills: number;
}

export interface CgroupPidsSummary {
  current: number;
  max: number;
}

export interface PgSummary {
  tps: number | null;
  hit_ratio_pct: number | null;
  backend_io_hit_pct: number | null;
  tuples_s: number | null;
  temp_bytes_s: number | null;
  deadlocks: number | null;
  errors: number | null;
  bgwriter: BgwriterSummary | null;
}

export interface BgwriterSummary {
  checkpoints_per_min: number;
  checkpoint_write_time_ms: number;
  buffers_backend_s: number;
  buffers_clean_s: number;
  maxwritten_clean: number;
  buffers_alloc_s: number;
}

export interface ApiProcessRow {
  pid: number;
  ppid: number;
  name: string;
  cmdline: string;
  state: string;
  num_threads: number;
  btime: number;
  cpu_pct: number;
  utime: number;
  stime: number;
  curcpu: number;
  rundelay: number;
  nice: number;
  priority: number;
  rtprio: number;
  policy: number;
  blkdelay: number;
  nvcsw: number;
  nivcsw: number;
  nvcsw_s: number | null;
  nivcsw_s: number | null;
  mem_pct: number;
  vsize_kb: number;
  rsize_kb: number;
  psize_kb: number;
  vgrow_kb: number;
  rgrow_kb: number;
  vswap_kb: number;
  vstext_kb: number;
  vdata_kb: number;
  vstack_kb: number;
  vslibs_kb: number;
  vlock_kb: number;
  minflt: number;
  majflt: number;
  read_bytes_s: number | null;
  write_bytes_s: number | null;
  read_ops_s: number | null;
  write_ops_s: number | null;
  total_read_bytes: number;
  total_write_bytes: number;
  total_read_ops: number;
  total_write_ops: number;
  cancelled_write_bytes: number;
  uid: number;
  euid: number;
  gid: number;
  egid: number;
  tty: number;
  exit_signal: number;
  pg_query: string | null;
  pg_backend_type: string | null;
}

export interface PgActivityRow {
  pid: number;
  database: string;
  user: string;
  application_name: string;
  client_addr: string;
  state: string;
  wait_event_type: string;
  wait_event: string;
  backend_type: string;
  query: string;
  query_id: number;
  query_duration_s: number | null;
  xact_duration_s: number | null;
  backend_duration_s: number | null;
  backend_start: number;
  xact_start: number;
  query_start: number;
  cpu_pct: number | null;
  rss_kb: number | null;
  rchar_s: number | null;
  wchar_s: number | null;
  read_bytes_s: number | null;
  write_bytes_s: number | null;
  stmt_mean_exec_time_ms: number | null;
  stmt_max_exec_time_ms: number | null;
  stmt_calls_s: number | null;
  stmt_hit_pct: number | null;
}

export interface PgStatementsRow {
  queryid: number;
  database: string;
  user: string;
  query: string;
  calls: number;
  rows: number;
  mean_exec_time_ms: number;
  min_exec_time_ms: number;
  max_exec_time_ms: number;
  stddev_exec_time_ms: number;
  calls_s: number | null;
  rows_s: number | null;
  exec_time_ms_s: number | null;
  shared_blks_read_s: number | null;
  shared_blks_hit_s: number | null;
  shared_blks_dirtied_s: number | null;
  shared_blks_written_s: number | null;
  local_blks_read_s: number | null;
  local_blks_written_s: number | null;
  temp_blks_read_s: number | null;
  temp_blks_written_s: number | null;
  temp_mb_s: number | null;
  rows_per_call: number | null;
  hit_pct: number | null;
  total_plan_time: number;
  wal_records: number;
  wal_bytes: number;
  total_exec_time: number;
}

export interface PgTablesRow {
  relid: number;
  database: string;
  schema: string;
  table: string;
  display_name: string;
  n_live_tup: number;
  n_dead_tup: number;
  size_bytes: number;
  last_autovacuum: number;
  last_autoanalyze: number;
  seq_scan_s: number | null;
  seq_tup_read_s: number | null;
  idx_scan_s: number | null;
  idx_tup_fetch_s: number | null;
  n_tup_ins_s: number | null;
  n_tup_upd_s: number | null;
  n_tup_del_s: number | null;
  n_tup_hot_upd_s: number | null;
  vacuum_count_s: number | null;
  autovacuum_count_s: number | null;
  heap_blks_read_s: number | null;
  heap_blks_hit_s: number | null;
  idx_blks_read_s: number | null;
  idx_blks_hit_s: number | null;
  tot_tup_read_s: number | null;
  disk_blks_read_s: number | null;
  io_hit_pct: number | null;
  seq_pct: number | null;
  dead_pct: number | null;
  hot_pct: number | null;
  analyze_count_s: number | null;
  autoanalyze_count_s: number | null;
  last_vacuum: number;
  last_analyze: number;
  toast_blks_read_s: number | null;
  toast_blks_hit_s: number | null;
  tidx_blks_read_s: number | null;
  tidx_blks_hit_s: number | null;
}

export interface PgIndexesRow {
  indexrelid: number;
  relid: number;
  database: string;
  schema: string;
  table: string;
  index: string;
  display_table: string;
  idx_scan: number;
  size_bytes: number;
  idx_scan_s: number | null;
  idx_tup_read_s: number | null;
  idx_tup_fetch_s: number | null;
  idx_blks_read_s: number | null;
  idx_blks_hit_s: number | null;
  io_hit_pct: number | null;
  disk_blks_read_s: number | null;
}

export interface PgEventsRow {
  event_id: number;
  event_type: string;
  severity: string;
  count: number;
  table_name: string;
  elapsed_s: number;
  extra_num1: number;
  extra_num2: number;
  extra_num3: number;
  buffer_hits: number;
  buffer_misses: number;
  buffer_dirtied: number;
  avg_read_rate_mbs: number;
  avg_write_rate_mbs: number;
  cpu_user_s: number;
  cpu_system_s: number;
  wal_records: number;
  wal_fpi: number;
  wal_bytes: number;
  message: string;
  sample: string;
  statement: string;
}

export interface PgLocksRow {
  pid: number;
  depth: number;
  root_pid: number;
  database: string;
  user: string;
  application_name: string;
  state: string;
  wait_event_type: string;
  wait_event: string;
  backend_type: string;
  lock_type: string;
  lock_mode: string;
  lock_target: string;
  lock_granted: boolean;
  query: string;
  xact_start: number;
  query_start: number;
  state_change: number;
}

// ============================================================
// Schema
// ============================================================

export interface InstanceInfo {
  database: string;
  pg_version: string;
}

export interface ApiSchema {
  version: string;
  mode: "live" | "history";
  timeline?: TimelineInfo;
  instance?: InstanceInfo;
  summary: SummarySchema;
  tabs: TabsSchema;
}

export interface TimelineInfo {
  start: number;
  end: number;
  total_snapshots: number;
  dates?: DateInfo[];
}

export interface DateInfo {
  date: string;
  count: number;
  first_timestamp: number;
  last_timestamp: number;
}

export interface SummarySchema {
  system: SummarySection[];
  pg: SummarySection[];
}

export interface SummarySection {
  key: string;
  label: string;
  fields: FieldSchema[];
}

export interface FieldSchema {
  key: string;
  label: string;
  type: DataType;
  unit?: Unit;
  format?: Format;
}

export interface TabsSchema {
  prc: TabSchema;
  pga: TabSchema;
  pgs: TabSchema;
  pgt: TabSchema;
  pgi: TabSchema;
  pge: TabSchema;
  pgl: TabSchema;
}

export interface TabSchema {
  name: string;
  description: string;
  entity_id: string;
  columns: ColumnSchema[];
  views: ViewSchema[];
  drill_down?: DrillDown;
}

export interface ColumnSchema {
  key: string;
  label: string;
  type: DataType;
  unit?: Unit;
  format?: Format;
  sortable: boolean;
  filterable?: boolean;
}

export interface ViewSchema {
  key: string;
  label: string;
  columns: string[];
  default?: boolean;
  default_sort?: string;
  default_sort_desc?: boolean;
  column_overrides?: ColumnOverride[];
}

export interface ColumnOverride {
  key: string;
  label?: string;
  unit?: Unit;
  format?: Format;
}

export interface DrillDown {
  target: string;
  via: string;
  target_field?: string;
  description: string;
}

export type DataType = "integer" | "number" | "string" | "boolean";
export type Unit =
  | "kb"
  | "bytes"
  | "bytes/s"
  | "ms"
  | "s"
  | "percent"
  | "/s"
  | "/min"
  | "blks/s"
  | "MB/s"
  | "buffers";
export type Format = "bytes" | "duration" | "rate" | "percent" | "age";

// ============================================================
// Heatmap
// ============================================================

export interface HeatmapBucket {
  ts: number;
  active: number;
  cpu: number;
  cgroup_cpu: number;
  cgroup_mem: number;
  errors: number;
  checkpoints: number;
  autovacuums: number;
  slow_queries: number;
}

// Tab key type
export type TabKey = "prc" | "pga" | "pgs" | "pgt" | "pgi" | "pge" | "pgl";

// ============================================================
// Analysis Report
// ============================================================

export interface IncidentGroup {
  id: number;
  first_ts: number;
  last_ts: number;
  severity: "info" | "warning" | "critical";
  persistent: boolean;
  incidents: AnalysisIncident[];
}

export interface HealthPoint {
  ts: number;
  score: number;
}

export interface AnalysisReport {
  start_ts: number;
  end_ts: number;
  snapshots_analyzed: number;
  groups: IncidentGroup[];
  incidents: AnalysisIncident[];
  recommendations: AnalysisRecommendation[];
  summary: AnalysisSummary;
  health_scores: HealthPoint[];
}

export interface AnalysisIncident {
  rule_id: string;
  category: string;
  severity: "info" | "warning" | "critical";
  first_ts: number;
  last_ts: number;
  peak_ts: number;
  peak_value: number;
  title: string;
  detail: string | null;
  snapshot_count: number;
  entity_id: number | null;
}

export interface AnalysisRecommendation {
  id: string;
  severity: "info" | "warning" | "critical";
  title: string;
  description: string;
  related_incidents: string[];
}

export interface AnalysisSummary {
  total_incidents: number;
  critical_count: number;
  warning_count: number;
  info_count: number;
  categories_affected: string[];
}
