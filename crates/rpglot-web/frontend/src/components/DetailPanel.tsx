import { useState } from "react";
import type { TabKey, ColumnSchema, DrillDown } from "../api/types";
import { formatValue } from "../utils/formatters";

interface DetailPanelProps {
  tab: TabKey;
  row: Record<string, unknown>;
  columns: ColumnSchema[];
  drillDown?: DrillDown;
  onClose: () => void;
  onDrillDown: (drillDown: DrillDown, value: unknown) => void;
}

interface Section {
  title: string;
  fields: string[];
  type?: "kv" | "query";
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
    { title: "Command", fields: ["cmdline"], type: "query" },
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
      fields: ["cpu_pct", "rss_kb"],
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
    { title: "Query", fields: ["query"], type: "query" },
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
    { title: "Query", fields: ["query"], type: "query" },
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
    { title: "Query", fields: ["query"], type: "query" },
  ],
};

const TAB_NAMES: Record<TabKey, string> = {
  prc: "Process",
  pga: "Activity",
  pgs: "Statement",
  pgt: "Table",
  pgi: "Index",
  pgl: "Lock",
};

export function DetailPanel({
  tab,
  row,
  columns,
  drillDown,
  onClose,
  onDrillDown,
}: DetailPanelProps) {
  const sections = TAB_SECTIONS[tab];
  const colMap = new Map(columns.map((c) => [c.key, c]));

  const drillDownValue = drillDown ? row[drillDown.via] : undefined;
  const hasDrillDown =
    drillDown && drillDownValue != null && drillDownValue !== 0;

  return (
    <div className="w-[480px] min-w-[480px] border-l border-slate-700 bg-slate-900/95 flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-2 border-b border-slate-700 bg-slate-800/50">
        <span className="text-sm font-medium text-slate-200">
          {TAB_NAMES[tab]} Detail
        </span>
        <button
          onClick={onClose}
          className="text-slate-400 hover:text-white text-lg leading-none px-1"
        >
          x
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
          />
        ))}
      </div>

      {/* Drill-down footer */}
      {hasDrillDown && (
        <div className="px-4 py-2 border-t border-slate-700">
          <button
            onClick={() => onDrillDown(drillDown!, drillDownValue)}
            className="w-full px-3 py-1.5 text-xs rounded bg-blue-900/40 text-blue-300 hover:bg-blue-800/50 transition-colors"
          >
            {">"} {drillDown!.description}
          </button>
        </div>
      )}
    </div>
  );
}

function DetailSection({
  section,
  row,
  colMap,
}: {
  section: Section;
  row: Record<string, unknown>;
  colMap: Map<string, ColumnSchema>;
}) {
  const [collapsed, setCollapsed] = useState(false);

  if (section.type === "query") {
    const queryText = String(row["query"] ?? "");
    if (!queryText) return null;

    return (
      <div>
        <SectionHeader
          title={section.title}
          collapsed={collapsed}
          onToggle={() => setCollapsed(!collapsed)}
        />
        {!collapsed && (
          <pre className="mt-1.5 p-3 bg-slate-950 rounded text-xs text-slate-300 whitespace-pre-wrap break-all max-h-64 overflow-y-auto border border-slate-800">
            {queryText}
          </pre>
        )}
      </div>
    );
  }

  const fields = section.fields.filter((key) => {
    const val = row[key];
    return (val != null && val !== "" && val !== 0) || colMap.has(key);
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
        <div className="mt-1.5 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-xs">
          {fields.map((key) => {
            const col = colMap.get(key);
            const label = col?.label ?? key;
            const val = row[key];
            const formatted =
              val == null
                ? "-"
                : col
                  ? formatValue(val, col.unit, col.format)
                  : String(val);

            return <KV key={key} label={label} value={formatted} />;
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
      className="flex items-center gap-1.5 text-[10px] font-semibold uppercase tracking-wider text-blue-400 hover:text-blue-300 transition-colors"
    >
      <span className="text-[8px]">{collapsed ? ">" : "v"}</span>
      {title}
    </button>
  );
}

function KV({ label, value }: { label: string; value: string }) {
  return (
    <>
      <span className="text-slate-500 whitespace-nowrap leading-[20px]">
        {label}
      </span>
      <span className="text-slate-200 whitespace-nowrap text-right tabular-nums leading-[20px]">
        {value}
      </span>
    </>
  );
}
