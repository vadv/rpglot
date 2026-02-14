import type { TabKey, TabsSchema } from "../api/types";

const TAB_ORDER: TabKey[] = ["prc", "pga", "pgs", "pgt", "pgi", "pgl"];

const TAB_LABELS: Record<TabKey, string> = {
  prc: "Processes",
  pga: "Activity",
  pgs: "Statements",
  pgt: "Tables",
  pgi: "Indexes",
  pgl: "Locks",
};

interface TabBarProps {
  activeTab: TabKey;
  onTabChange: (tab: TabKey) => void;
  tabs?: TabsSchema;
}

export function TabBar({ activeTab, onTabChange }: TabBarProps) {
  return (
    <div className="flex border-b border-slate-700 bg-slate-800/50">
      {TAB_ORDER.map((key) => (
        <button
          key={key}
          onClick={() => onTabChange(key)}
          className={`px-4 py-2 text-sm font-medium transition-colors ${
            activeTab === key
              ? "text-blue-400 border-b-2 border-blue-400 bg-slate-800"
              : "text-slate-400 hover:text-slate-200 hover:bg-slate-800/50"
          }`}
        >
          {TAB_LABELS[key]}
        </button>
      ))}
    </div>
  );
}
