import { useState, useEffect, useCallback } from "react";
import { useSchema } from "./hooks/useSchema";
import { useLiveSnapshot, useHistorySnapshot } from "./hooks/useSnapshot";
import { readUrlState, useUrlSync } from "./hooks/useUrlState";
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
      <div className="flex items-center justify-center min-h-screen text-red-400">
        Failed to load schema: {schemaError}
      </div>
    );
  }

  if (!schema) {
    return (
      <div className="flex items-center justify-center min-h-screen text-slate-400">
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
  const snapshot = useLiveSnapshot();
  const tabState = useTabState(schema, snapshot);

  return (
    <div className="flex flex-col h-screen">
      <Header mode="live" timestamp={snapshot?.timestamp} />
      {snapshot && <SummaryPanel snapshot={snapshot} schema={schema.summary} />}
      <TabBar
        activeTab={tabState.activeTab}
        onTabChange={tabState.handleTabChange}
      />
      <div className="flex-1 min-h-0">
        {snapshot ? (
          <TabContent snapshot={snapshot} schema={schema} tabState={tabState} />
        ) : (
          <div className="flex items-center justify-center h-full text-slate-500">
            Waiting for data...
          </div>
        )}
      </div>
      <HintsBar
        detailOpen={tabState.detailOpen}
        hasSelection={tabState.selectedId != null}
        hasDrillDown={!!schema.tabs[tabState.activeTab].drill_down}
      />
    </div>
  );
}

function HistoryApp({ schema }: { schema: ApiSchema }) {
  const { snapshot, loading, jumpTo } = useHistorySnapshot();
  const urlSync = useUrlSync();
  const urlState = readUrlState();
  const tabState = useTabState(schema, snapshot);
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
          <div className="flex items-center justify-center h-full text-slate-500">
            Loading...
          </div>
        )}
      </div>
      <HintsBar
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
    field: string;
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
    const targetRow = data.find(
      (row) => row[drillDownTarget.field] === drillDownTarget.value,
    );
    if (targetRow) {
      setSelectedId(targetRow[entityId] as string | number);
      setDetailOpen(true);
    }
    setDrillDownTarget(null);
  }, [drillDownTarget, snapshot, activeTab, schema]);

  const handleSelectRow = useCallback((id: string | number | null) => {
    setSelectedId(id);
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
      setDrillDownTarget({ tab: targetTab, field: drillDown.via, value });
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

      // 1-5: switch tabs
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

function Header({
  mode,
  timestamp,
  loading,
}: {
  mode: string;
  timestamp?: number;
  loading?: boolean;
}) {
  const ts = timestamp ? new Date(timestamp * 1000).toLocaleString() : "-";
  return (
    <div className="flex items-center justify-between px-4 py-1.5 bg-slate-900 border-b border-slate-700">
      <div className="flex items-center gap-3">
        <span className="text-sm font-bold text-slate-200">rpglot</span>
        <span
          className={`text-xs px-1.5 py-0.5 rounded ${
            mode === "live"
              ? "bg-green-900/50 text-green-400"
              : "bg-yellow-900/50 text-yellow-400"
          }`}
        >
          {mode}
        </span>
      </div>
      <div className="flex items-center gap-2 text-xs text-slate-400">
        {loading && <span className="text-yellow-400">loading...</span>}
        <span>{ts}</span>
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
  detailOpen,
  hasSelection,
  hasDrillDown,
}: {
  detailOpen: boolean;
  hasSelection: boolean;
  hasDrillDown: boolean;
}) {
  return (
    <div className="flex items-center gap-4 px-4 py-1 bg-slate-900 border-t border-slate-700 text-[10px] text-slate-500">
      <Hint keys="1-6" action="tabs" />
      <Hint keys="j/k" action="navigate" />
      {hasSelection && <Hint keys="Enter" action="details" />}
      {(detailOpen || hasSelection) && (
        <Hint keys="Esc" action={detailOpen ? "close detail" : "deselect"} />
      )}
      {hasSelection && hasDrillDown && <Hint keys=">" action="drill-down" />}
      <Hint keys="/" action="filter" />
    </div>
  );
}

function Hint({ keys, action }: { keys: string; action: string }) {
  return (
    <span>
      <span className="text-slate-400">{keys}</span> <span>{action}</span>
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
