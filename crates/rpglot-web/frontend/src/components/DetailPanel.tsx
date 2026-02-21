import { useState, useCallback } from "react";
import {
  X,
  ChevronDown,
  ChevronRight,
  Copy,
  Check,
  ExternalLink,
  Maximize2,
  Filter,
} from "lucide-react";
import type {
  TabKey,
  ColumnSchema,
  DrillDown,
  ColumnOverride,
} from "../api/types";
import { formatValue } from "../utils/formatters";
import { COLUMN_DESCRIPTIONS } from "../utils/columnDescriptions";
import { getThresholdClass } from "../utils/thresholds";
import { Tooltip } from "./Tooltip";
import { HighlightedCode } from "./HighlightedCode";
import { QueryModal } from "./QueryModal";

interface DetailPanelProps {
  tab: TabKey;
  row: Record<string, unknown>;
  columns: ColumnSchema[];
  columnOverrides?: ColumnOverride[];
  drillDown?: DrillDown;
  onClose: () => void;
  onDrillDown: (
    drillDown: DrillDown,
    value: unknown,
    sourceRow: Record<string, unknown>,
  ) => void;
  snapshotTimestamp?: number;
  onFilterField?: (column: string, value: string) => void;
}

interface Section {
  title: string;
  fields: string[];
  type?: "kv" | "query";
  language?: "sql" | "plan" | "text";
}

const TAB_SECTIONS: Record<TabKey, Section[]> = {
  prc: [
    {
      title: "Identity",
      fields: [
        "pid",
        "ppid",
        "name",
        "state",
        "tty",
        "btime",
        "num_threads",
        "exit_signal",
      ],
    },
    {
      title: "User/Group",
      fields: ["uid", "euid", "gid", "egid"],
    },
    {
      title: "CPU",
      fields: [
        "cpu_pct",
        "utime",
        "stime",
        "curcpu",
        "rundelay",
        "nice",
        "priority",
        "rtprio",
        "policy",
        "blkdelay",
        "nvcsw_s",
        "nivcsw_s",
      ],
    },
    {
      title: "Memory",
      fields: [
        "mem_pct",
        "vsize_kb",
        "rsize_kb",
        "psize_kb",
        "vgrow_kb",
        "rgrow_kb",
        "vstext_kb",
        "vdata_kb",
        "vstack_kb",
        "vslibs_kb",
        "vlock_kb",
        "vswap_kb",
        "minflt",
        "majflt",
      ],
    },
    {
      title: "Disk I/O",
      fields: [
        "read_bytes_s",
        "write_bytes_s",
        "read_ops_s",
        "write_ops_s",
        "total_read_bytes",
        "total_write_bytes",
        "total_read_ops",
        "total_write_ops",
        "cancelled_write_bytes",
      ],
    },
    {
      title: "PostgreSQL",
      fields: ["pg_backend_type", "pg_query"],
    },
    { title: "Command", fields: ["cmdline"], type: "query", language: "text" },
  ],
  pga: [
    {
      title: "Session",
      fields: [
        "pid",
        "database",
        "user",
        "application_name",
        "client_addr",
        "backend_type",
      ],
    },
    {
      title: "Timing",
      fields: [
        "backend_start",
        "xact_start",
        "query_start",
        "query_duration_s",
        "xact_duration_s",
        "backend_duration_s",
      ],
    },
    {
      title: "State",
      fields: ["state", "wait_event_type", "wait_event"],
    },
    {
      title: "OS Process",
      fields: [
        "cpu_pct",
        "rss_kb",
        "rchar_s",
        "wchar_s",
        "read_bytes_s",
        "write_bytes_s",
      ],
    },
    {
      title: "Statements",
      fields: [
        "stmt_mean_exec_time_ms",
        "stmt_max_exec_time_ms",
        "stmt_calls_s",
        "stmt_hit_pct",
      ],
    },
    { title: "Query", fields: ["query"], type: "query", language: "sql" },
  ],
  pgs: [
    {
      title: "Rates",
      fields: ["calls_s", "rows_s", "exec_time_ms_s", "hit_pct"],
    },
    {
      title: "Identity",
      fields: ["queryid", "database", "user", "calls", "rows", "rows_per_call"],
    },
    {
      title: "Timing",
      fields: [
        "total_exec_time",
        "mean_exec_time_ms",
        "min_exec_time_ms",
        "max_exec_time_ms",
        "stddev_exec_time_ms",
        "total_plan_time",
      ],
    },
    {
      title: "I/O",
      fields: [
        "shared_blks_read_s",
        "shared_blks_hit_s",
        "hit_pct",
        "shared_blks_dirtied_s",
        "shared_blks_written_s",
        "local_blks_read_s",
        "local_blks_written_s",
      ],
    },
    {
      title: "Temp/WAL",
      fields: [
        "temp_blks_read_s",
        "temp_blks_written_s",
        "temp_mb_s",
        "wal_records",
        "wal_bytes",
      ],
    },
    { title: "Query", fields: ["query"], type: "query", language: "sql" },
  ],
  pgt: [
    {
      title: "Identity",
      fields: ["relid", "schema", "table", "display_name", "size_bytes"],
    },
    {
      title: "Scan Activity",
      fields: [
        "seq_scan_s",
        "seq_tup_read_s",
        "idx_scan_s",
        "idx_tup_fetch_s",
        "tot_tup_read_s",
        "seq_pct",
      ],
    },
    {
      title: "Write Activity",
      fields: [
        "n_tup_ins_s",
        "n_tup_upd_s",
        "n_tup_del_s",
        "n_tup_hot_upd_s",
        "hot_pct",
      ],
    },
    {
      title: "Tuples",
      fields: ["n_live_tup", "n_dead_tup", "dead_pct"],
    },
    {
      title: "Maintenance",
      fields: [
        "vacuum_count_s",
        "autovacuum_count_s",
        "analyze_count_s",
        "autoanalyze_count_s",
        "last_vacuum",
        "last_autovacuum",
        "last_analyze",
        "last_autoanalyze",
      ],
    },
    {
      title: "I/O",
      fields: [
        "heap_blks_read_s",
        "heap_blks_hit_s",
        "idx_blks_read_s",
        "idx_blks_hit_s",
        "toast_blks_read_s",
        "toast_blks_hit_s",
        "tidx_blks_read_s",
        "tidx_blks_hit_s",
        "io_hit_pct",
        "disk_blks_read_s",
      ],
    },
  ],
  pgi: [
    {
      title: "Identity",
      fields: [
        "indexrelid",
        "relid",
        "schema",
        "table",
        "index",
        "display_table",
        "size_bytes",
      ],
    },
    {
      title: "Usage",
      fields: ["idx_scan", "idx_scan_s", "idx_tup_read_s", "idx_tup_fetch_s"],
    },
    {
      title: "I/O",
      fields: [
        "idx_blks_read_s",
        "idx_blks_hit_s",
        "io_hit_pct",
        "disk_blks_read_s",
      ],
    },
  ],
  pgp: [
    {
      title: "Plan Info",
      fields: ["planid", "stmt_queryid", "database", "user"],
    },
    {
      title: "Timing",
      fields: [
        "calls",
        "rows",
        "mean_time_ms",
        "min_time_ms",
        "max_time_ms",
        "total_time_ms",
        "first_call",
        "last_call",
      ],
    },
    {
      title: "Rates",
      fields: ["calls_s", "rows_s", "exec_time_ms_s", "rows_per_call"],
    },
    {
      title: "I/O",
      fields: [
        "shared_blks_read_s",
        "shared_blks_hit_s",
        "shared_blks_dirtied_s",
        "shared_blks_written_s",
        "temp_blks_read_s",
        "temp_blks_written_s",
        "hit_pct",
      ],
    },
    { title: "Query", fields: ["query"], type: "query", language: "sql" },
    { title: "Plan", fields: ["plan"], type: "query", language: "plan" },
  ],
  pge: [
    {
      title: "Event Info",
      fields: [
        "event_type",
        "severity",
        "count",
        "table_name",
        "elapsed_s",
        "extra_num1",
        "extra_num2",
      ],
    },
    { title: "Message", fields: ["message"], type: "query", language: "text" },
    { title: "Sample", fields: ["sample"], type: "query", language: "sql" },
    { title: "Statement", fields: ["statement"], type: "query", language: "sql" },
  ],
  pgl: [
    {
      title: "Identity",
      fields: [
        "pid",
        "depth",
        "root_pid",
        "database",
        "user",
        "application_name",
        "backend_type",
      ],
    },
    {
      title: "Lock",
      fields: ["lock_type", "lock_mode", "lock_granted", "lock_target"],
    },
    {
      title: "Timing",
      fields: ["xact_start", "query_start", "state_change"],
    },
    {
      title: "State",
      fields: ["state", "wait_event_type", "wait_event"],
    },
    { title: "Query", fields: ["query"], type: "query", language: "sql" },
  ],
  pgv: [
    {
      title: "Vacuum",
      fields: ["pid", "database", "table_name", "relid", "phase"],
    },
    {
      title: "Progress",
      fields: [
        "progress_pct",
        "heap_blks_total",
        "heap_blks_scanned",
        "heap_blks_vacuumed",
      ],
    },
    {
      title: "Dead Tuples",
      fields: [
        "index_vacuum_count",
        "max_dead_tuples",
        "num_dead_tuples",
        "dead_tuple_bytes",
        "indexes_total",
        "indexes_processed",
      ],
    },
  ],
};

const TAB_NAMES: Record<TabKey, string> = {
  prc: "Process",
  pga: "Activity",
  pgs: "Statement",
  pgp: "Plan",
  pgt: "Table",
  pgi: "Index",
  pge: "Event",
  pgl: "Lock",
  pgv: "Vacuum",
};

export function DetailPanel({
  tab,
  row,
  columns,
  columnOverrides,
  drillDown,
  onClose,
  onDrillDown,
  snapshotTimestamp,
  onFilterField,
}: DetailPanelProps) {
  const sections = TAB_SECTIONS[tab];
  const colMap = new Map(columns.map((c) => [c.key, c]));
  const overrideMap = new Map((columnOverrides ?? []).map((o) => [o.key, o]));

  const drillDownValue = drillDown ? row[drillDown.via] : undefined;
  const hasDrillDown =
    drillDown && drillDownValue != null && drillDownValue !== 0;

  return (
    <div className="w-[480px] min-w-[480px] border-l border-[var(--border-default)] bg-[var(--bg-surface)] flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-2 border-b border-[var(--border-default)] bg-[var(--bg-elevated)]">
        <span className="text-sm font-medium text-[var(--text-primary)]">
          {TAB_NAMES[tab]} Detail
        </span>
        <button
          onClick={onClose}
          className="p-0.5 rounded text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors"
        >
          <X size={16} />
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-4 py-3 space-y-4">
        {sections.map((section) => (
          <DetailSection
            key={section.title}
            section={section}
            row={row}
            colMap={colMap}
            overrideMap={overrideMap}
            snapshotTimestamp={snapshotTimestamp}
            onFilterField={onFilterField}
          />
        ))}
      </div>

      {/* Drill-down footer */}
      {hasDrillDown && (
        <div className="px-4 py-2 border-t border-[var(--border-default)]">
          <button
            onClick={() => onDrillDown(drillDown!, drillDownValue, row)}
            className="flex items-center justify-center gap-1.5 w-full px-3 py-1.5 text-xs rounded bg-[var(--accent-subtle)] text-[var(--accent-text)] hover:bg-[var(--accent)] hover:text-[var(--text-inverse)] transition-colors font-medium"
          >
            <ExternalLink size={12} />
            {drillDown!.description}
          </button>
        </div>
      )}
    </div>
  );
}

const FILTER_EXCLUDED = new Set([
  "query", "plan", "cmdline", "pg_query",
  "queryid", "planid", "pid", "ppid", "relid", "indexrelid",
  "root_pid", "event_id", "stmt_queryid", "query_id",
]);

function DetailSection({
  section,
  row,
  colMap,
  overrideMap,
  snapshotTimestamp,
  onFilterField,
}: {
  section: Section;
  row: Record<string, unknown>;
  colMap: Map<string, ColumnSchema>;
  overrideMap: Map<string, ColumnOverride>;
  snapshotTimestamp?: number;
  onFilterField?: (column: string, value: string) => void;
}) {
  const [collapsed, setCollapsed] = useState(false);
  const [modalOpen, setModalOpen] = useState(false);

  if (section.type === "query") {
    const fieldKey = section.fields[0] ?? "query";
    const queryText = String(row[fieldKey] ?? "");
    if (!queryText) return null;

    const lang = section.language ?? "text";

    return (
      <div>
        <div className="flex items-center justify-between">
          <SectionHeader
            title={section.title}
            collapsed={collapsed}
            onToggle={() => setCollapsed(!collapsed)}
          />
          {!collapsed && (
            <div className="flex items-center gap-0.5">
              <button
                onClick={() => setModalOpen(true)}
                className="flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded transition-colors text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
              >
                <Maximize2 size={10} />
                <span>expand</span>
              </button>
              <CopyButton text={queryText} />
            </div>
          )}
        </div>
        {!collapsed && (
          <HighlightedCode
            text={queryText}
            language={lang}
            className="mt-1.5 p-3 bg-[var(--bg-base)] border border-[var(--border-default)] rounded-lg text-[13px] font-mono text-[var(--text-primary)] whitespace-pre-wrap break-all max-h-64 overflow-y-auto"
          />
        )}
        {modalOpen && (
          <QueryModal
            text={queryText}
            language={lang}
            title={section.title}
            onClose={() => setModalOpen(false)}
          />
        )}
      </div>
    );
  }

  const fields = section.fields.filter((key) => {
    const val = row[key];
    return val != null && val !== "" && val !== 0;
  });

  if (fields.length === 0) return null;

  return (
    <div>
      <SectionHeader
        title={section.title}
        collapsed={collapsed}
        onToggle={() => setCollapsed(!collapsed)}
      />
      {!collapsed && (
        <div className="mt-1.5 grid grid-cols-[auto_1fr_auto] gap-x-3 gap-y-0.5 text-xs">
          {fields.map((key) => {
            const col = colMap.get(key);
            const ovr = overrideMap.get(key);
            const label = ovr?.label ?? col?.label ?? key;
            const effectiveUnit = ovr?.unit ?? col?.unit;
            const effectiveFormat = ovr?.format ?? col?.format;
            const val = row[key];
            const formatted =
              val == null
                ? "-"
                : col
                  ? formatValue(
                      val,
                      effectiveUnit,
                      effectiveFormat,
                      snapshotTimestamp,
                    )
                  : String(val);
            const isFilterable =
              !!col?.filterable &&
              !FILTER_EXCLUDED.has(key) &&
              !!onFilterField;

            return (
              <KV
                key={key}
                fieldKey={key}
                label={label}
                value={formatted}
                rawValue={val}
                row={row}
                filterable={isFilterable}
                onFilter={
                  isFilterable
                    ? () => onFilterField!(key, String(val))
                    : undefined
                }
              />
            );
          })}
        </div>
      )}
    </div>
  );
}

function SectionHeader({
  title,
  collapsed,
  onToggle,
}: {
  title: string;
  collapsed: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      onClick={onToggle}
      className="flex items-center gap-1 text-[11px] font-semibold uppercase tracking-wider text-[var(--accent-text)] hover:opacity-80 transition-opacity"
    >
      {collapsed ? <ChevronRight size={12} /> : <ChevronDown size={12} />}
      {title}
    </button>
  );
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  }, [text]);

  return (
    <button
      onClick={handleCopy}
      className="flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded transition-colors text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
    >
      {copied ? (
        <>
          <Check size={10} className="text-[var(--status-success)]" />
          <span className="text-[var(--status-success)]">copied</span>
        </>
      ) : (
        <>
          <Copy size={10} />
          <span>copy</span>
        </>
      )}
    </button>
  );
}

function KV({
  fieldKey,
  label,
  value,
  rawValue,
  row,
  filterable,
  onFilter,
}: {
  fieldKey: string;
  label: string;
  value: string;
  rawValue: unknown;
  row: Record<string, unknown>;
  filterable?: boolean;
  onFilter?: () => void;
}) {
  const desc = COLUMN_DESCRIPTIONS[fieldKey];
  const colorClass = getThresholdClass(fieldKey, rawValue, row);
  return (
    <>
      <span className="text-[var(--text-tertiary)] whitespace-nowrap leading-[20px]">
        {desc ? (
          <Tooltip content={desc} side="top">
            <span className="cursor-help border-b border-dotted border-[var(--border-subtle)]">
              {label}
            </span>
          </Tooltip>
        ) : (
          label
        )}
      </span>
      <span
        className={`${colorClass || "text-[var(--text-primary)]"} whitespace-nowrap text-right font-mono tabular-nums leading-[20px]`}
      >
        {value}
      </span>
      {filterable ? (
        <button
          onClick={onFilter}
          className="text-[var(--text-tertiary)] hover:text-[var(--accent-text)] transition-colors p-0.5 leading-[20px]"
          title={`Filter by ${label}`}
        >
          <Filter size={10} />
        </button>
      ) : (
        <span />
      )}
    </>
  );
}
