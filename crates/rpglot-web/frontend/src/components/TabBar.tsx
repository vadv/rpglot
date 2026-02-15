import {
  Monitor,
  Activity,
  BarChart3,
  Table2,
  ListTree,
  AlertTriangle,
  Lock,
} from "lucide-react";
import { Tooltip } from "./Tooltip";
import type { TabKey } from "../api/types";

const TAB_ORDER: TabKey[] = ["prc", "pga", "pgs", "pgt", "pgi", "pge", "pgl"];

const TAB_CONFIG: Record<
  TabKey,
  { label: string; fullName: string; icon: typeof Monitor }
> = {
  prc: { label: "Processes", fullName: "OS Processes", icon: Monitor },
  pga: {
    label: "Activity",
    fullName: "pg_stat_activity",
    icon: Activity,
  },
  pgs: {
    label: "Statements",
    fullName: "pg_stat_statements",
    icon: BarChart3,
  },
  pgt: {
    label: "Tables",
    fullName: "pg_stat_user_tables",
    icon: Table2,
  },
  pgi: {
    label: "Indexes",
    fullName: "pg_stat_user_indexes",
    icon: ListTree,
  },
  pge: {
    label: "Events",
    fullName: "PG Log Events",
    icon: AlertTriangle,
  },
  pgl: { label: "Locks", fullName: "pg_locks", icon: Lock },
};

interface TabBarProps {
  activeTab: TabKey;
  onTabChange: (tab: TabKey) => void;
}

export function TabBar({ activeTab, onTabChange }: TabBarProps) {
  return (
    <div className="flex border-b border-[var(--border-default)] bg-[var(--bg-surface)]">
      {TAB_ORDER.map((key) => {
        const config = TAB_CONFIG[key];
        const Icon = config.icon;
        const isActive = activeTab === key;
        return (
          <Tooltip key={key} content={config.fullName} side="bottom">
            <button
              onClick={() => onTabChange(key)}
              className={`flex items-center gap-1.5 px-4 py-2 text-sm font-medium transition-colors ${
                isActive
                  ? "text-[var(--accent-text)] border-b-2 border-[var(--accent)] bg-[var(--accent-subtle)]"
                  : "text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
              }`}
            >
              <Icon size={14} />
              {config.label}
            </button>
          </Tooltip>
        );
      })}
    </div>
  );
}
