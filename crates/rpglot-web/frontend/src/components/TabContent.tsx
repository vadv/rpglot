import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { DataTable } from "./DataTable";
import { DetailPanel } from "./DetailPanel";
import { getTabData } from "../utils/tabData";
import { isPgProcess, isPgtProblematic, isPgiProblematic } from "../utils/smartFilters";
import {
  SCHEMA_VIEW,
  SCHEMA_COLUMNS,
  DATABASE_VIEW,
  DATABASE_COLUMNS,
  TABLESPACE_VIEW,
  TABLESPACE_COLUMNS,
  PGI_SCHEMA_VIEW,
  PGI_SCHEMA_COLUMNS,
  PGI_DATABASE_VIEW,
  PGI_DATABASE_COLUMNS,
  PGI_TABLESPACE_VIEW,
  PGI_TABLESPACE_COLUMNS,
  aggregateTableRows,
  aggregateIndexRows,
} from "../utils/aggregation";
import type { TabState } from "../hooks/useTabState";
import type { ApiSnapshot, ApiSchema } from "../api/types";

export function TabContent({
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
    activeView,
    initialView,
    initialFilter,
    handleSelectRow,
    handleOpenDetail,
    handleCloseDetail,
    handleDrillDown,
    handleViewChange,
    handleFilterChange,
    columnFilterPreset,
    setColumnFilterPreset,
    flashRowId,
    smartFilterResetKey,
  } = tabState;
  const tabSchema = schema.tabs[activeTab];
  const rawData = getTabData(snapshot, activeTab);

  const isAggregatedView =
    (activeTab === "pgt" || activeTab === "pgi") &&
    (activeView === "schema" ||
      activeView === "database" ||
      activeView === "tablespace");

  // In aggregated view, clicking a row drills down into the default view with a column filter
  const handleAggregatedSelect = useCallback(
    (id: string | number | null) => {
      if (!isAggregatedView || id == null) {
        handleSelectRow(id);
        return;
      }
      // Switch to default view with column filter by schema/database/tablespace
      const groupColumn =
        activeView === "database"
          ? "database"
          : activeView === "tablespace"
            ? "tablespace"
            : "schema";
      const defaultView =
        tabSchema.views.find((v) => v.default) ?? tabSchema.views[0];
      if (defaultView) {
        handleViewChange(defaultView.key);
        setColumnFilterPreset({ column: groupColumn, value: String(id) });
        handleSelectRow(null);
      }
    },
    [
      isAggregatedView,
      activeView,
      tabSchema.views,
      handleViewChange,
      handleSelectRow,
    ],
  );

  // Clear column filter preset when user manually switches view
  const handleViewChangeWithReset = useCallback(
    (view: string) => {
      setColumnFilterPreset(null);
      handleViewChange(view);
    },
    [handleViewChange],
  );

  // Inject Schema + Database + Tablespace views into PGT/PGI views list
  const effectiveViews = useMemo(() => {
    if (activeTab === "pgt") {
      return [...tabSchema.views, SCHEMA_VIEW, DATABASE_VIEW, TABLESPACE_VIEW];
    }
    if (activeTab === "pgi") {
      return [
        ...tabSchema.views,
        PGI_SCHEMA_VIEW,
        PGI_DATABASE_VIEW,
        PGI_TABLESPACE_VIEW,
      ];
    }
    return tabSchema.views;
  }, [activeTab, tabSchema.views]);

  // PGA-specific toggle filters (hide idle / hide system backends)
  const [hideIdle, setHideIdle] = useState(true);
  const [hideSystem, setHideSystem] = useState(true);

  // PRC: only PostgreSQL processes (default ON)
  const [pgOnly, setPgOnly] = useState(true);

  // PGS: hide statements with no calls delta (default ON)
  const [hideInactive, setHideInactive] = useState(true);

  // PGT/PGI: only problematic rows (default OFF — show all)
  const [problemsOnly, setProblemsOnly] = useState(false);

  // Reset problemsOnly when switching tabs (keep it per-tab)
  useEffect(() => {
    setProblemsOnly(false);
  }, [activeTab]);

  // Reset all smart filters on drill-down / analysis jump so the target row is visible
  const prevResetKey = useRef(smartFilterResetKey);
  useEffect(() => {
    if (smartFilterResetKey !== prevResetKey.current) {
      prevResetKey.current = smartFilterResetKey;
      setHideIdle(false);
      setHideSystem(false);
      setPgOnly(false);
      setHideInactive(false);
      setProblemsOnly(false);
    }
  }, [smartFilterResetKey]);

  const data = useMemo(() => {
    let filtered = rawData;

    if (activeTab === "pga") {
      filtered = filtered.filter((row) => {
        if (hideIdle && row.state === "idle") return false;
        if (hideSystem) {
          if (
            row.backend_type !== "client backend" &&
            row.backend_type !== "autovacuum worker"
          )
            return false;
          if (
            typeof row.application_name === "string" &&
            (row.application_name as string).startsWith("rpglot")
          )
            return false;
        }
        return true;
      });
    }

    if (activeTab === "prc" && pgOnly) {
      filtered = filtered.filter(isPgProcess);
    }

    if (activeTab === "pge") {
      if (activeView === "errors") {
        filtered = filtered.filter((row) => {
          const t = row.event_type as string;
          return t === "error" || t === "fatal" || t === "panic";
        });
      } else if (activeView === "checkpoints") {
        filtered = filtered.filter((row) => {
          const t = row.event_type as string;
          return t === "checkpoint_starting" || t === "checkpoint_complete";
        });
      } else if (activeView === "autovacuum") {
        filtered = filtered.filter((row) => {
          const t = row.event_type as string;
          return t === "autovacuum" || t === "autoanalyze";
        });
      } else if (activeView === "slow_queries") {
        filtered = filtered.filter((row) => {
          const t = row.event_type as string;
          return t === "slow_query";
        });
      }
    }

    if (activeTab === "pgs" && hideInactive) {
      filtered = filtered.filter((row) => {
        const c = row.calls_s as number | null;
        return c != null && c > 0;
      });
    }

    if (activeTab === "pgt" && problemsOnly) {
      filtered = filtered.filter((row) => isPgtProblematic(row, activeView));
    }

    if (activeTab === "pgi" && problemsOnly) {
      filtered = filtered.filter((row) => isPgiProblematic(row, activeView));
    }

    return filtered;
  }, [
    rawData,
    activeTab,
    activeView,
    hideIdle,
    hideSystem,
    pgOnly,
    hideInactive,
    problemsOnly,
  ]);

  // Count hidden items for toggle button labels
  const hiddenCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    if (activeTab === "pga") {
      if (hideIdle)
        counts.idle = rawData.filter((r) => r.state === "idle").length;
      if (hideSystem)
        counts.system = rawData.filter(
          (r) =>
            (r.backend_type !== "client backend" &&
              r.backend_type !== "autovacuum worker") ||
            (typeof r.application_name === "string" &&
              (r.application_name as string).startsWith("rpglot")),
        ).length;
    }
    if (activeTab === "prc" && pgOnly) {
      counts.nonpg = rawData.filter((r) => !isPgProcess(r)).length;
    }
    if (activeTab === "pgs" && hideInactive) {
      counts.inactive = rawData.filter((r) => {
        const c = r.calls_s as number | null;
        return c == null || c === 0;
      }).length;
    }
    if ((activeTab === "pgt" || activeTab === "pgi") && problemsOnly) {
      const fn =
        activeTab === "pgt"
          ? (r: Record<string, unknown>) => isPgtProblematic(r, activeView)
          : (r: Record<string, unknown>) => isPgiProblematic(r, activeView);
      counts.healthy = rawData.filter((r) => !fn(r)).length;
    }
    return counts;
  }, [
    rawData,
    activeTab,
    activeView,
    hideIdle,
    hideSystem,
    pgOnly,
    hideInactive,
    problemsOnly,
  ]);

  const toolbarControls = useMemo(() => {
    if (activeTab === "pga") {
      return (
        <>
          <ToggleButton
            active={hideIdle}
            onClick={() => setHideIdle((p) => !p)}
            label="idle"
            count={hiddenCounts.idle}
          />
          <ToggleButton
            active={hideSystem}
            onClick={() => setHideSystem((p) => !p)}
            label="system"
            count={hiddenCounts.system}
          />
        </>
      );
    }
    if (activeTab === "prc") {
      return (
        <ToggleButton
          active={pgOnly}
          onClick={() => setPgOnly((p) => !p)}
          label="non-pg"
          count={hiddenCounts.nonpg}
        />
      );
    }
    if (activeTab === "pgs") {
      return (
        <ToggleButton
          active={hideInactive}
          onClick={() => setHideInactive((p) => !p)}
          label="inactive"
          count={hiddenCounts.inactive}
        />
      );
    }
    if ((activeTab === "pgt" || activeTab === "pgi") && !isAggregatedView) {
      return (
        <ToggleButton
          active={problemsOnly}
          onClick={() => setProblemsOnly((p) => !p)}
          label="healthy"
          count={hiddenCounts.healthy}
          invertLabel
        />
      );
    }
    return undefined;
  }, [
    activeTab,
    hideIdle,
    hideSystem,
    pgOnly,
    hideInactive,
    problemsOnly,
    hiddenCounts,
    isAggregatedView,
  ]);

  // Aggregated views: schema, database, or tablespace grouping
  const aggregatedData = useMemo(() => {
    if (!isAggregatedView) return null;
    if (
      activeView === "schema" ||
      activeView === "database" ||
      activeView === "tablespace"
    ) {
      if (activeTab === "pgt") return aggregateTableRows(rawData, activeView);
      if (activeTab === "pgi") return aggregateIndexRows(rawData, activeView);
    }
    return null;
  }, [isAggregatedView, activeView, activeTab, rawData]);

  const effectiveData = isAggregatedView ? aggregatedData! : data;
  const effectiveColumns = isAggregatedView
    ? activeView === "database"
      ? activeTab === "pgi"
        ? PGI_DATABASE_COLUMNS
        : DATABASE_COLUMNS
      : activeView === "tablespace"
        ? activeTab === "pgi"
          ? PGI_TABLESPACE_COLUMNS
          : TABLESPACE_COLUMNS
        : activeTab === "pgi"
          ? PGI_SCHEMA_COLUMNS
          : SCHEMA_COLUMNS
    : tabSchema.columns;
  const effectiveEntityId = isAggregatedView
    ? activeView === "database"
      ? "database"
      : activeView === "tablespace"
        ? "tablespace"
        : "schema"
    : tabSchema.entity_id;

  const selectedRow =
    selectedId != null
      ? (effectiveData.find((row) => row[effectiveEntityId] === selectedId) ??
        null)
      : null;

  return (
    <div className="flex h-full">
      <div className="flex-1 min-w-0">
        <DataTable
          key={activeTab}
          data={effectiveData}
          columns={effectiveColumns}
          views={effectiveViews}
          entityId={effectiveEntityId}
          selectedId={selectedId}
          onSelectRow={
            isAggregatedView ? handleAggregatedSelect : handleSelectRow
          }
          onOpenDetail={handleOpenDetail}
          isLockTree={activeTab === "pgl"}
          activeTab={activeTab}
          initialView={initialView}
          initialFilter={initialFilter}
          onViewChange={handleViewChangeWithReset}
          onFilterChange={handleFilterChange}
          snapshotTimestamp={snapshot.timestamp}
          toolbarControls={toolbarControls}
          flashId={flashRowId}
          columnFilterPreset={columnFilterPreset}
        />
      </div>
      {detailOpen && selectedRow && !isAggregatedView && (
        <DetailPanel
          tab={activeTab}
          row={selectedRow}
          columns={tabSchema.columns}
          columnOverrides={
            effectiveViews.find((v) => v.key === activeView)?.column_overrides
          }
          drillDown={tabSchema.drill_down}
          onClose={handleCloseDetail}
          onDrillDown={handleDrillDown}
          snapshotTimestamp={snapshot.timestamp}
        />
      )}
    </div>
  );
}

/**
 * Toggle button for filtering.
 * Default: active=true means "hiding items" (shows "+label (N)"), inactive = "showing all" (shows "-label").
 * invertLabel: active=true means "showing filtered" (shows "-label (N)"), inactive = "showing all" (shows "+label").
 */
function ToggleButton({
  active,
  onClick,
  label,
  count,
  invertLabel,
}: {
  active: boolean;
  onClick: () => void;
  label: string;
  count?: number;
  invertLabel?: boolean;
}) {
  // invertLabel: when true, active state means "filter is ON" → highlight the button
  // Default (no invertLabel): active means "hiding items" → dimmed button with count
  const prefix = invertLabel ? (active ? "-" : "+") : active ? "+" : "-";
  const highlighted = invertLabel ? active : !active;

  return (
    <button
      onClick={onClick}
      className={`text-[11px] px-2 py-0.5 rounded-full font-medium transition-colors whitespace-nowrap border ${
        highlighted
          ? "bg-[var(--accent-muted)] text-[var(--accent-text)] border-[var(--accent-text)]/30"
          : "bg-transparent text-[var(--text-secondary)] border-[var(--border-default)] hover:border-[var(--text-tertiary)] hover:text-[var(--text-primary)]"
      }`}
      title={active ? `Show ${label}` : `Hide ${label}`}
    >
      {prefix} {label}
      {active && count != null && count > 0 && (
        <span className="ml-0.5 opacity-70">({count})</span>
      )}
    </button>
  );
}
