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
    <div className="flex gap-1 px-2 pt-1.5 pb-0 bg-[var(--bg-surface)] border-b border-[var(--border-default)]">
      {TAB_ORDER.map((key) => {
        const config = TAB_CONFIG[key];
        const Icon = config.icon;
        const isActive = activeTab === key;
        return (
          <Tooltip key={key} content={config.fullName} side="bottom">
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
