import { useState, useEffect, useCallback } from "react";
import {
  Database,
  Radio,
  History,
  Pause,
  Play,
  Sun,
  Moon,
  Monitor,
} from "lucide-react";
import { useSchema } from "./hooks/useSchema";
import { useLiveSnapshot, useHistorySnapshot } from "./hooks/useSnapshot";
import { readUrlState, useUrlSync } from "./hooks/useUrlState";
import { useTheme } from "./hooks/useTheme";
import { TabBar } from "./components/TabBar";
import { SummaryPanel } from "./components/SummaryPanel";
import { DataTable } from "./components/DataTable";
import { DetailPanel } from "./components/DetailPanel";
import { Timeline } from "./components/Timeline";
import type { ApiSnapshot, ApiSchema, TabKey, DrillDown } from "./api/types";

const TAB_ORDER: TabKey[] = ["prc", "pga", "pgs", "pgt", "pgi", "pgl"];

export default function App() {
  const { schema, error: schemaError } = useSchema();

  if (schemaError) {
    return (
      <div className="flex items-center justify-center min-h-screen text-[var(--status-critical)]">
        Failed to load schema: {schemaError}
      </div>
    );
  }

  if (!schema) {
    return (
      <div className="flex items-center justify-center min-h-screen text-[var(--text-tertiary)]">
        Loading...
      </div>
    );
  }

  if (schema.mode === "history") {
    return <HistoryApp schema={schema} />;
  }

  return <LiveApp schema={schema} />;
}

function LiveApp({ schema }: { schema: ApiSchema }) {
  const { snapshot, paused, togglePause } = useLiveSnapshot();
  const tabState = useTabState(schema, snapshot);
  const urlSync = useUrlSync();
  const themeHook = useTheme();

  // Sync pause timestamp to URL
  useEffect(() => {
    if (paused && snapshot) {
      urlSync({ timestamp: snapshot.timestamp });
    } else {
      urlSync({ timestamp: null });
    }
  }, [paused, snapshot, urlSync]);

  // Global keyboard: Space to toggle pause
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const target = e.target as HTMLElement;
      if (target.tagName === "INPUT" || target.tagName === "TEXTAREA") return;
      if (e.key === " ") {
        e.preventDefault();
        togglePause();
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [togglePause]);

  return (
    <div className="flex flex-col h-screen">
      <Header
        mode="live"
        timestamp={snapshot?.timestamp}
        paused={paused}
        onTogglePause={togglePause}
        themeHook={themeHook}
      />
      {snapshot && <SummaryPanel snapshot={snapshot} schema={schema.summary} />}
      <TabBar
        activeTab={tabState.activeTab}
        onTabChange={tabState.handleTabChange}
      />
      <div className="flex-1 min-h-0">
        {snapshot ? (
          <TabContent snapshot={snapshot} schema={schema} tabState={tabState} />
        ) : (
          <div className="flex items-center justify-center h-full text-[var(--text-tertiary)]">
            Waiting for data...
          </div>
        )}
      </div>
      <HintsBar
        mode="live"
        detailOpen={tabState.detailOpen}
        hasSelection={tabState.selectedId != null}
        hasDrillDown={!!schema.tabs[tabState.activeTab].drill_down}
        paused={paused}
      />
    </div>
  );
}

function HistoryApp({ schema }: { schema: ApiSchema }) {
  const { snapshot, loading, jumpTo } = useHistorySnapshot();
  const urlSync = useUrlSync();
  const urlState = readUrlState();
  const tabState = useTabState(schema, snapshot);
  const themeHook = useTheme();
  const [position, setPosition] = useState(() => urlState.position ?? 0);

  // On mount: jump to URL position
  useEffect(() => {
    if (urlState.position != null && urlState.position > 0) {
      jumpTo(urlState.position);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handlePositionChange = useCallback(
    (pos: number) => {
      setPosition(pos);
      jumpTo(pos);
      urlSync({ position: pos });
    },
    [jumpTo, urlSync],
  );

  return (
    <div className="flex flex-col h-screen">
      <Header
        mode="history"
        timestamp={snapshot?.timestamp}
        loading={loading}
        themeHook={themeHook}
      />
      {snapshot && <SummaryPanel snapshot={snapshot} schema={schema.summary} />}
      <TabBar
        activeTab={tabState.activeTab}
        onTabChange={tabState.handleTabChange}
      />
      <div className="flex-1 min-h-0">
        {snapshot ? (
          <TabContent snapshot={snapshot} schema={schema} tabState={tabState} />
        ) : (
          <div className="flex items-center justify-center h-full text-[var(--text-tertiary)]">
            Loading...
          </div>
        )}
      </div>
      <HintsBar
        mode="history"
        detailOpen={tabState.detailOpen}
        hasSelection={tabState.selectedId != null}
        hasDrillDown={!!schema.tabs[tabState.activeTab].drill_down}
      />
      {schema.timeline && (
        <Timeline
          timeline={schema.timeline}
          position={position}
          onPositionChange={handlePositionChange}
          timestamp={snapshot?.timestamp}
        />
      )}
    </div>
  );
}

// ============================================================
// Tab state hook — selection, detail, drill-down, URL sync
// ============================================================

interface TabState {
  activeTab: TabKey;
  selectedId: string | number | null;
  detailOpen: boolean;
  initialView: string | null;
  initialFilter: string | null;
  handleTabChange: (tab: TabKey) => void;
  handleSelectRow: (id: string | number | null) => void;
  handleOpenDetail: () => void;
  handleCloseDetail: () => void;
  handleDrillDown: (drillDown: DrillDown, value: unknown) => void;
  handleViewChange: (view: string) => void;
  handleFilterChange: (filter: string) => void;
}

function useTabState(
  schema: ApiSchema,
  snapshot: ApiSnapshot | null,
): TabState {
  const urlSync = useUrlSync();
  const [urlState] = useState(() => readUrlState());

  const [activeTab, setActiveTab] = useState<TabKey>(urlState.tab);
  const [selectedId, setSelectedId] = useState<string | number | null>(null);
  const [detailOpen, setDetailOpen] = useState(false);
  const [drillDownTarget, setDrillDownTarget] = useState<{
    tab: TabKey;
    targetField?: string;
    value: unknown;
  } | null>(null);

  // Initial view/filter from URL (consumed once by DataTable on mount)
  const [initialView] = useState<string | null>(urlState.view);
  const [initialFilter] = useState<string | null>(urlState.filter);

  // Reset selection on tab change
  const handleTabChange = useCallback(
    (tab: TabKey) => {
      setActiveTab(tab);
      setSelectedId(null);
      setDetailOpen(false);
      urlSync({ tab, view: null, filter: null });
    },
    [urlSync],
  );

  // Validate selection: close detail if entity disappeared
  useEffect(() => {
    if (!snapshot || selectedId == null) return;
    const data = getTabData(snapshot, activeTab);
    const entityId = schema.tabs[activeTab].entity_id;
    const exists = data.some((row) => row[entityId] === selectedId);
    if (!exists) {
      setSelectedId(null);
      setDetailOpen(false);
    }
  }, [snapshot, selectedId, activeTab, schema]);

  // Drill-down: after tab switch, find and select target row
  useEffect(() => {
    if (!drillDownTarget || !snapshot) return;
    if (activeTab !== drillDownTarget.tab) return;

    const data = getTabData(snapshot, drillDownTarget.tab);
    const entityId = schema.tabs[drillDownTarget.tab].entity_id;
    const searchField = drillDownTarget.targetField ?? entityId;
    const targetRow = data.find(
      (row) => row[searchField] === drillDownTarget.value,
    );
    if (targetRow) {
      setSelectedId(targetRow[entityId] as string | number);
      setDetailOpen(true);
    }
    setDrillDownTarget(null);
  }, [drillDownTarget, snapshot, activeTab, schema]);

  const handleSelectRow = useCallback((id: string | number | null) => {
    setSelectedId(id);
    setDetailOpen(id != null);
  }, []);

  const handleOpenDetail = useCallback(() => {
    setDetailOpen(true);
  }, []);

  const handleCloseDetail = useCallback(() => {
    setDetailOpen(false);
  }, []);

  const handleDrillDown = useCallback(
    (drillDown: DrillDown, value: unknown) => {
      const targetTab = drillDown.target as TabKey;
      setDrillDownTarget({
        tab: targetTab,
        targetField: drillDown.target_field,
        value,
      });
      setActiveTab(targetTab);
      setSelectedId(null);
      setDetailOpen(false);
      urlSync({ tab: targetTab, view: null, filter: null });
    },
    [urlSync],
  );

  const handleViewChange = useCallback(
    (view: string) => {
      urlSync({ view });
    },
    [urlSync],
  );

  const handleFilterChange = useCallback(
    (filter: string) => {
      urlSync({ filter: filter || null });
    },
    [urlSync],
  );

  // Global keyboard shortcuts
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const target = e.target as HTMLElement;
      if (target.tagName === "INPUT" || target.tagName === "TEXTAREA") return;

      // 1-6: switch tabs
      const tabIndex = parseInt(e.key) - 1;
      if (tabIndex >= 0 && tabIndex < TAB_ORDER.length) {
        e.preventDefault();
        handleTabChange(TAB_ORDER[tabIndex]);
        return;
      }

      // Escape: close detail first, then deselect
      if (e.key === "Escape") {
        e.preventDefault();
        if (detailOpen) {
          setDetailOpen(false);
        } else if (selectedId != null) {
          setSelectedId(null);
        }
        return;
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [detailOpen, selectedId, handleTabChange]);

  return {
    activeTab,
    selectedId,
    detailOpen,
    initialView,
    initialFilter,
    handleTabChange,
    handleSelectRow,
    handleOpenDetail,
    handleCloseDetail,
    handleDrillDown,
    handleViewChange,
    handleFilterChange,
  };
}

// ============================================================
// Components
// ============================================================

interface ThemeHook {
  theme: "light" | "dark" | "system";
  effective: "light" | "dark";
  cycle: () => void;
}

function Header({
  mode,
  timestamp,
  loading,
  paused,
  onTogglePause,
  themeHook,
}: {
  mode: string;
  timestamp?: number;
  loading?: boolean;
  paused?: boolean;
  onTogglePause?: () => void;
  themeHook: ThemeHook;
}) {
  const ts = timestamp ? new Date(timestamp * 1000).toLocaleString() : "-";

  const ThemeIcon =
    themeHook.theme === "light"
      ? Sun
      : themeHook.theme === "dark"
        ? Moon
        : Monitor;

  return (
    <div className="flex items-center justify-between px-4 py-2 bg-[var(--bg-surface)] border-b border-[var(--border-default)]">
      <div className="flex items-center gap-3">
        <div className="flex items-center gap-1.5">
          <Database size={16} className="text-[var(--accent-text)]" />
          <span className="text-sm font-semibold text-[var(--text-primary)]">
            rpglot
          </span>
        </div>
        <span
          className={`flex items-center gap-1 text-xs px-2 py-0.5 rounded-full font-medium ${
            mode === "live"
              ? "bg-[var(--status-success-bg)] text-[var(--status-success)]"
              : "bg-[var(--status-warning-bg)] text-[var(--status-warning)]"
          }`}
        >
          {mode === "live" ? <Radio size={10} /> : <History size={10} />}
          {mode}
        </span>
        {mode === "live" && onTogglePause && (
          <button
            onClick={onTogglePause}
            className={`flex items-center gap-1 text-xs px-2 py-0.5 rounded transition-colors ${
              paused
                ? "bg-[var(--status-warning-bg)] text-[var(--status-warning)]"
                : "bg-[var(--bg-elevated)] text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
            }`}
          >
            {paused ? <Play size={12} /> : <Pause size={12} />}
            {paused ? "resume" : "pause"}
          </button>
        )}
      </div>
      <div className="flex items-center gap-3">
        {loading && (
          <span className="text-xs text-[var(--status-warning)]">
            loading...
          </span>
        )}
        <span className="text-xs text-[var(--text-tertiary)] font-mono tabular-nums">
          {ts}
        </span>
        <button
          onClick={themeHook.cycle}
          className="p-1 rounded text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors"
          title={`Theme: ${themeHook.theme}`}
        >
          <ThemeIcon size={16} />
        </button>
      </div>
    </div>
  );
}

function TabContent({
  snapshot,
  schema,
  tabState,
}: {
  snapshot: ApiSnapshot;
  schema: ApiSchema;
  tabState: TabState;
}) {
  const {
    activeTab,
    selectedId,
    detailOpen,
    initialView,
    initialFilter,
    handleSelectRow,
    handleOpenDetail,
    handleCloseDetail,
    handleDrillDown,
    handleViewChange,
    handleFilterChange,
  } = tabState;
  const tabSchema = schema.tabs[activeTab];
  const data = getTabData(snapshot, activeTab);

  const selectedRow =
    selectedId != null
      ? (data.find((row) => row[tabSchema.entity_id] === selectedId) ?? null)
      : null;

  return (
    <div className="flex h-full">
      <div className="flex-1 min-w-0">
        <DataTable
          key={activeTab}
          data={data}
          columns={tabSchema.columns}
          views={tabSchema.views}
          entityId={tabSchema.entity_id}
          selectedId={selectedId}
          onSelectRow={handleSelectRow}
          onOpenDetail={handleOpenDetail}
          isLockTree={activeTab === "pgl"}
          initialView={initialView}
          initialFilter={initialFilter}
          onViewChange={handleViewChange}
          onFilterChange={handleFilterChange}
        />
      </div>
      {detailOpen && selectedRow && (
        <DetailPanel
          tab={activeTab}
          row={selectedRow}
          columns={tabSchema.columns}
          drillDown={tabSchema.drill_down}
          onClose={handleCloseDetail}
          onDrillDown={handleDrillDown}
        />
      )}
    </div>
  );
}

function HintsBar({
  mode,
  detailOpen,
  hasSelection,
  hasDrillDown,
  paused,
}: {
  mode: "live" | "history";
  detailOpen: boolean;
  hasSelection: boolean;
  hasDrillDown: boolean;
  paused?: boolean;
}) {
  return (
    <div className="flex items-center gap-4 px-4 py-1 bg-[var(--bg-surface)] border-t border-[var(--border-default)] text-[11px] text-[var(--text-tertiary)]">
      <Hint keys="1-6" action="tabs" />
      <Hint keys="j/k" action="navigate" />
      {(detailOpen || hasSelection) && (
        <Hint keys="Esc" action={detailOpen ? "close detail" : "deselect"} />
      )}
      {hasSelection && hasDrillDown && <Hint keys=">" action="drill-down" />}
      <Hint keys="/" action="filter" />
      {mode === "live" && (
        <Hint keys="Space" action={paused ? "resume" : "pause"} />
      )}
    </div>
  );
}

function Hint({ keys, action }: { keys: string; action: string }) {
  return (
    <span className="flex items-center gap-1">
      <kbd className="inline-flex items-center justify-center min-w-[18px] h-[18px] px-1 bg-[var(--bg-elevated)] border border-[var(--border-default)] rounded text-[10px] font-mono text-[var(--text-secondary)]">
        {keys}
      </kbd>
      <span>{action}</span>
    </span>
  );
}

// ============================================================
// Helpers
// ============================================================

function getTabData(
  snapshot: ApiSnapshot,
  tab: TabKey,
): Record<string, unknown>[] {
  switch (tab) {
    case "prc":
      return snapshot.prc as unknown as Record<string, unknown>[];
    case "pga":
      return snapshot.pga as unknown as Record<string, unknown>[];
    case "pgs":
      return snapshot.pgs as unknown as Record<string, unknown>[];
    case "pgt":
      return snapshot.pgt as unknown as Record<string, unknown>[];
    case "pgi":
      return snapshot.pgi as unknown as Record<string, unknown>[];
    case "pgl":
      return snapshot.pgl as unknown as Record<string, unknown>[];
  }
}
