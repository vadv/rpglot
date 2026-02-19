import {
  Monitor,
  Activity,
  BarChart3,
  Table2,
  ListTree,
  AlertTriangle,
  Lock,
  Trash2,
  Network,
} from "lucide-react";
import { Tooltip } from "./Tooltip";
import type { TabKey } from "../api/types";

const TAB_ORDER: TabKey[] = [
  "prc",
  "pga",
  "pgs",
  "pgp",
  "pgt",
  "pgi",
  "pge",
  "pgl",
  "pgv",
];

const TAB_CONFIG: Record<
  TabKey,
  { label: string; description: string; icon: typeof Monitor }
> = {
  prc: {
    label: "Processes",
    icon: Monitor,
    description:
      "OS-level view of all processes on the server.\nFind who is eating CPU or memory \u2014 is it PostgreSQL, a backup job, or something unexpected?\nSpot runaway processes, zombie workers, or OOM candidates before the kernel does.",
  },
  pga: {
    label: "Activity",
    icon: Activity,
    description:
      "Live sessions inside PostgreSQL (pg_stat_activity).\nSee which queries are running right now, who is waiting on locks, and who has been idle in transaction for too long.\nThis is your first stop when the database feels slow.",
  },
  pgs: {
    label: "Statements",
    icon: BarChart3,
    description:
      "Cumulative query statistics (pg_stat_statements).\nFind the heaviest queries by total time, calls/sec, or rows returned.\nSpot cache misses, temp file spills, and queries that suddenly got slower.",
  },
  pgp: {
    label: "Plans",
    icon: Network,
    description:
      "Plan statistics from pg_store_plans extension.\nSee execution plans grouped by plan structure, track timing and I/O per plan.\nDetect plan regressions when the optimizer picks a worse plan for the same query.",
  },
  pgt: {
    label: "Tables",
    icon: Table2,
    description:
      "Per-table I/O and maintenance stats.\nCheck if autovacuum is keeping up, find tables with excessive sequential scans, and see dead tuple bloat.\nSwitch between Reads / Writes / Scans / Maintenance views.",
  },
  pgi: {
    label: "Indexes",
    icon: ListTree,
    description:
      "Per-index usage and I/O stats.\nFind unused indexes that waste disk and slow down writes.\nSpot missing indexes \u2014 tables with high seq scans but no index activity.",
  },
  pge: {
    label: "Events",
    icon: AlertTriangle,
    description:
      "PostgreSQL log events: errors, checkpoints, autovacuums, slow queries.\nSee error spikes, checkpoint frequency, vacuum duration, and slowest queries.\nSwitch views to focus on what matters right now.",
  },
  pgl: {
    label: "Locks",
    icon: Lock,
    description:
      "Lock dependency tree (pg_locks).\nVisualize who blocks whom \u2014 find the root blocker and the full cascade of waiting sessions.\nCritical when transactions pile up and throughput drops to zero.",
  },
  pgv: {
    label: "Vacuum",
    icon: Trash2,
    description:
      "Live VACUUM progress (pg_stat_progress_vacuum).\nSee which tables are being vacuumed right now, current phase, scan progress, and dead tuple collection.\nEmpty when no vacuums are running.",
  },
};

interface TabBarProps {
  activeTab: TabKey;
  onTabChange: (tab: TabKey) => void;
}

export function TabBar({ activeTab, onTabChange }: TabBarProps) {
  return (
    <div className="flex gap-1 px-2 pt-1.5 pb-0 bg-[var(--bg-surface)] border-b border-[var(--border-default)]">
      {TAB_ORDER.map((key) => {
        const config = TAB_CONFIG[key];
        const Icon = config.icon;
        const isActive = activeTab === key;
        return (
          <Tooltip key={key} content={config.description} side="bottom" wide>
            <button
              onClick={() => onTabChange(key)}
              className={`relative flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium rounded-t transition-colors ${
                isActive
                  ? "text-[var(--accent-text)]"
                  : "text-[var(--text-tertiary)] hover:text-[var(--text-primary)]"
              }`}
            >
              <Icon size={14} />
              {config.label}
              {isActive && (
                <span className="absolute bottom-0 left-2 right-2 h-0.5 rounded-full bg-[var(--accent)]" />
              )}
            </button>
          </Tooltip>
        );
      })}
    </div>
  );
}
