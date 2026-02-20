import { useState, useEffect, useCallback } from "react";
import { readUrlState, useUrlSync } from "./useUrlState";
import { getTabData } from "../utils/tabData";
import type { ApiSchema, ApiSnapshot, TabKey, DrillDown } from "../api/types";

export const TAB_ORDER: TabKey[] = [
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

export interface TabState {
  activeTab: TabKey;
  selectedId: string | number | null;
  detailOpen: boolean;
  helpOpen: boolean;
  activeView: string;
  initialView: string | null;
  initialFilter: string | null;
  flashRowId: string | number | null;
  smartFilterResetKey: number;
  columnFilterPreset: { column: string; value: string } | null;
  handleTabChange: (tab: TabKey) => void;
  handleSelectRow: (id: string | number | null) => void;
  handleOpenDetail: () => void;
  handleCloseDetail: () => void;
  handleDrillDown: (
    drillDown: DrillDown,
    value: unknown,
    sourceRow: Record<string, unknown>,
  ) => void;
  handleViewChange: (view: string) => void;
  handleFilterChange: (filter: string) => void;
  setHelpOpen: (open: boolean) => void;
  setColumnFilterPreset: (
    preset: { column: string; value: string } | null,
  ) => void;
  triggerFlash: (id: string | number) => void;
  resetSmartFilters: () => void;
}

export function useTabState(
  schema: ApiSchema,
  snapshot: ApiSnapshot | null,
): TabState {
  const urlSync = useUrlSync();
  const [urlState] = useState(() => readUrlState());

  const [activeTab, setActiveTab] = useState<TabKey>(urlState.tab);
  const [selectedId, setSelectedId] = useState<string | number | null>(null);
  const [detailOpen, setDetailOpen] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);
  const [activeView, setActiveView] = useState("");
  const [drillDownTarget, setDrillDownTarget] = useState<{
    tab: TabKey;
    targetField?: string;
    value: unknown;
    filterColumn?: string;
    filterValue?: string;
  } | null>(null);
  const [columnFilterPreset, setColumnFilterPreset] = useState<{
    column: string;
    value: string;
  } | null>(null);

  // Initial view/filter from URL (consumed once by DataTable on mount)
  const [initialView, setInitialView] = useState<string | null>(urlState.view);
  const [initialFilter] = useState<string | null>(urlState.filter);
  const [flashRowId, setFlashRowId] = useState<string | number | null>(null);
  // Incremented on drill-down / analysis jump to reset smart filters in TabContent
  const [smartFilterResetKey, setSmartFilterResetKey] = useState(0);

  const triggerFlash = useCallback((id: string | number) => {
    setFlashRowId(id);
    // Scroll the target row into view within the table's scroll container.
    // We can't use scrollIntoView({ block: "center" }) because nested
    // overflow containers cause incorrect positioning.
    requestAnimationFrame(() => {
      const rowEl = document.getElementById(`row-${id}`);
      if (!rowEl) return;
      // Find the closest scrollable ancestor (the overflow-auto div)
      const container = rowEl.closest(".overflow-auto") as HTMLElement | null;
      if (container) {
        const rowTop = rowEl.offsetTop;
        const rowHeight = rowEl.offsetHeight;
        const containerHeight = container.clientHeight;
        container.scrollTo({
          top: rowTop - containerHeight / 2 + rowHeight / 2,
          behavior: "smooth",
        });
      } else {
        rowEl.scrollIntoView({ block: "center", behavior: "smooth" });
      }
    });
    // No auto-clear: highlight stays until user navigates (arrow keys / mouse click)
  }, []);

  // Reset selection on tab change
  const handleTabChange = useCallback(
    (tab: TabKey) => {
      setActiveTab(tab);
      setSelectedId(null);
      setDetailOpen(false);
      setColumnFilterPreset(null);
      urlSync({ tab, view: null, filter: null });
    },
    [urlSync],
  );

  // Validate selection: close detail if entity disappeared.
  // Skip validation while flashRowId is active — we're waiting for a new snapshot
  // to load after an analysis jump / drill-down navigation.
  useEffect(() => {
    if (!snapshot || selectedId == null || flashRowId != null) return;
    const data = getTabData(snapshot, activeTab);
    const entityId = schema.tabs[activeTab].entity_id;
    const exists = data.some((row) => row[entityId] === selectedId);
    if (!exists) {
      setSelectedId(null);
      setDetailOpen(false);
    }
  }, [snapshot, selectedId, activeTab, schema, flashRowId]);

  // Drill-down: after tab switch, apply column filter and/or find and select target row
  useEffect(() => {
    if (!drillDownTarget || !snapshot) return;
    if (activeTab !== drillDownTarget.tab) return;

    // 1. Column filter (if specified) — switch to default view and set filter
    if (drillDownTarget.filterColumn && drillDownTarget.filterValue) {
      const tabSch = schema.tabs[drillDownTarget.tab];
      const defaultView =
        tabSch.views.find((v) => v.default) ?? tabSch.views[0];
      if (defaultView) {
        setActiveView(defaultView.key);
        setInitialView(defaultView.key);
      }
      setColumnFilterPreset({
        column: drillDownTarget.filterColumn,
        value: drillDownTarget.filterValue,
      });
    }

    // 2. Find and select target row (if via was specified)
    const data = getTabData(snapshot, drillDownTarget.tab);
    const entityId = schema.tabs[drillDownTarget.tab].entity_id;
    const searchField = drillDownTarget.targetField ?? entityId;
    const targetRow = data.find(
      (row) => row[searchField] === drillDownTarget.value,
    );
    if (targetRow) {
      const id = targetRow[entityId] as string | number;
      setSelectedId(id);
      setDetailOpen(true);
      triggerFlash(id);
    }
    setDrillDownTarget(null);
  }, [drillDownTarget, snapshot, activeTab, schema, triggerFlash]);

  const handleSelectRow = useCallback((id: string | number | null) => {
    setSelectedId(id);
    setDetailOpen(id != null);
    setFlashRowId(null); // Clear persistent highlight on user interaction
  }, []);

  const handleOpenDetail = useCallback(() => {
    setDetailOpen(true);
  }, []);

  const handleCloseDetail = useCallback(() => {
    setDetailOpen(false);
  }, []);

  const handleDrillDown = useCallback(
    (
      drillDown: DrillDown,
      value: unknown,
      sourceRow: Record<string, unknown>,
    ) => {
      const targetTab = drillDown.target as TabKey;
      const filterValue =
        drillDown.filter_via && sourceRow[drillDown.filter_via] != null
          ? String(sourceRow[drillDown.filter_via])
          : undefined;
      setDrillDownTarget({
        tab: targetTab,
        targetField: drillDown.target_field,
        value,
        filterColumn: drillDown.filter_target ?? undefined,
        filterValue: filterValue || undefined,
      });
      setActiveTab(targetTab);
      setSelectedId(null);
      setDetailOpen(false);
      setSmartFilterResetKey((k) => k + 1);
      urlSync({ tab: targetTab, view: null, filter: null });
    },
    [urlSync],
  );

  const handleViewChange = useCallback(
    (view: string) => {
      setActiveView(view);
      setInitialView(view);
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

      // ?: toggle help modal
      if (e.key === "?") {
        e.preventDefault();
        setHelpOpen((prev) => !prev);
        return;
      }

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
    helpOpen,
    activeView,
    initialView,
    initialFilter,
    handleTabChange,
    handleSelectRow,
    handleOpenDetail,
    handleCloseDetail,
    handleDrillDown,
    handleViewChange,
    handleFilterChange,
    setHelpOpen,
    columnFilterPreset,
    setColumnFilterPreset,
    flashRowId,
    triggerFlash,
    smartFilterResetKey,
    resetSmartFilters: useCallback(
      () => setSmartFilterResetKey((k) => k + 1),
      [],
    ),
  };
}
