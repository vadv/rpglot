import { useMemo, useState, useRef, useEffect, useCallback } from "react";
import {
  useReactTable,
  getCoreRowModel,
  getSortedRowModel,
  getFilteredRowModel,
  flexRender,
  type SortingState,
  type ColumnDef,
  type ColumnFiltersState,
} from "@tanstack/react-table";
import { createPortal } from "react-dom";
import { Search, Inbox, ChevronUp, ChevronDown, Filter, X } from "lucide-react";
import { Tooltip } from "./Tooltip";
import { RichTooltip } from "./RichTooltip";
import type {
  ColumnSchema,
  ColumnOverride,
  ViewSchema,
  TabKey,
} from "../api/types";
import { formatValue } from "../utils/formatters";
import { getThresholdClass } from "../utils/thresholds";
import { COLUMN_DESCRIPTIONS } from "../utils/columnDescriptions";
import { buildColumnTooltip, VIEW_DESCRIPTIONS } from "../utils/columnHelp";

interface DataTableProps {
  data: Record<string, unknown>[];
  columns: ColumnSchema[];
  views: ViewSchema[];
  entityId: string;
  selectedId: string | number | null;
  onSelectRow: (id: string | number | null) => void;
  onOpenDetail: () => void;
  isLockTree?: boolean;
  activeTab?: TabKey;
  initialView?: string | null;
  initialFilter?: string | null;
  onViewChange?: (view: string) => void;
  onFilterChange?: (filter: string) => void;
  snapshotTimestamp?: number;
  /** Extra toolbar controls rendered before the filter input */
  toolbarControls?: React.ReactNode;
  /** Global text filter to apply (e.g. from analysis jump). Consumed once on change. */
  globalFilterPreset?: string | null;
  /** Column filter to apply (e.g. from schema view drill-down). Consumed once on change. */
  columnFilterPreset?: { column: string; value: string } | null;
}

export function DataTable({
  data,
  columns: allColumns,
  views,
  entityId,
  selectedId,
  onSelectRow,
  onOpenDetail,
  isLockTree,
  activeTab,
  initialView,
  initialFilter,
  onViewChange,
  onFilterChange,
  snapshotTimestamp,
  toolbarControls,
  globalFilterPreset,
  columnFilterPreset,
}: DataTableProps) {
  const [activeView, setActiveView] = useState(() => {
    if (initialView && views.some((v) => v.key === initialView)) {
      return initialView;
    }
    const def = views.find((v) => v.default);
    return def?.key ?? views[0]?.key ?? "";
  });

  const currentView = views.find((v) => v.key === activeView) ?? views[0];
  const visibleKeys = currentView?.columns ?? allColumns.map((c) => c.key);

  const [sorting, setSorting] = useState<SortingState>(() => {
    if (currentView?.default_sort) {
      return [
        {
          id: currentView.default_sort,
          desc: currentView.default_sort_desc ?? false,
        },
      ];
    }
    return [];
  });

  const [columnFilters, setColumnFilters] = useState<ColumnFiltersState>(() => {
    if (columnFilterPreset) {
      return [
        { id: columnFilterPreset.column, value: [columnFilterPreset.value] },
      ];
    }
    return [];
  });
  const [globalFilter, setGlobalFilter] = useState(
    globalFilterPreset ?? initialFilter ?? "",
  );

  // Apply global filter preset changes after mount (e.g. same-tab analysis jump)
  const prevGlobalPreset = useRef<string | null | undefined>(undefined);
  useEffect(() => {
    if (globalFilterPreset !== prevGlobalPreset.current) {
      setGlobalFilter(globalFilterPreset ?? "");
      onFilterChange?.(globalFilterPreset ?? "");
      prevGlobalPreset.current = globalFilterPreset;
    }
  }, [globalFilterPreset, onFilterChange]);

  // Apply column filter preset changes after mount (e.g. drill-down on same tab)
  const prevPreset = useRef<
    { column: string; value: string } | null | undefined
  >(undefined);
  useEffect(() => {
    if (columnFilterPreset !== prevPreset.current) {
      if (columnFilterPreset) {
        setColumnFilters([
          { id: columnFilterPreset.column, value: [columnFilterPreset.value] },
        ]);
      } else {
        setColumnFilters([]);
      }
      prevPreset.current = columnFilterPreset;
    }
  }, [columnFilterPreset]);
  const [filterPopover, setFilterPopover] = useState<{
    columnId: string;
    rect: DOMRect;
  } | null>(null);

  const containerRef = useRef<HTMLDivElement>(null);
  const filterRef = useRef<HTMLInputElement>(null);

  const filterableSet = useMemo(
    () => new Set(allColumns.filter((c) => c.filterable).map((c) => c.key)),
    [allColumns],
  );

  // Notify parent of initial view on mount
  useEffect(() => {
    onViewChange?.(activeView);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Sync view when parent requests a programmatic switch (e.g. analysis jump)
  useEffect(() => {
    if (
      initialView &&
      initialView !== activeView &&
      views.some((v) => v.key === initialView)
    ) {
      setActiveView(initialView);
      const v = views.find((vw) => vw.key === initialView);
      if (v?.default_sort) {
        setSorting([
          { id: v.default_sort, desc: v.default_sort_desc ?? false },
        ]);
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialView]);

  // Auto-focus table container on mount
  useEffect(() => {
    containerRef.current?.focus();
  }, []);

  // Per-view column overrides (label, unit, format)
  const overrideMap = useMemo(() => {
    const m = new Map<string, ColumnOverride>();
    for (const o of currentView?.column_overrides ?? []) m.set(o.key, o);
    return m;
  }, [currentView]);

  const columnDefs = useMemo(() => {
    const colMap = new Map(allColumns.map((c) => [c.key, c]));
    const defs: ColumnDef<Record<string, unknown>>[] = [];
    for (const key of visibleKeys) {
      const schema = colMap.get(key);
      if (!schema) continue;

      const ovr = overrideMap.get(key);
      const effectiveLabel = ovr?.label ?? schema.label;
      const effectiveUnit = ovr?.unit ?? schema.unit;
      const effectiveFormat = ovr?.format ?? schema.format;

      const isPglPid = isLockTree && key === "pid";

      defs.push({
        id: key,
        accessorFn: (row) => row[key],
        header: effectiveLabel,
        cell: isPglPid
          ? (info) => {
              const depth = (info.row.original["depth"] as number) ?? 1;
              const prefix =
                depth > 1 ? "\u00B7".repeat((depth - 1) * 2) + " " : "";
              const text = prefix + String(info.getValue() ?? "-");
              const colorClass = getThresholdClass(
                key,
                info.getValue(),
                info.row.original,
              );
              return colorClass ? (
                <span className={colorClass}>{text}</span>
              ) : (
                text
              );
            }
          : (info) => {
              const formatted = formatValue(
                info.getValue(),
                effectiveUnit,
                effectiveFormat,
                snapshotTimestamp,
              );
              const colorClass = getThresholdClass(
                key,
                info.getValue(),
                info.row.original,
              );
              return colorClass ? (
                <span className={colorClass}>{formatted}</span>
              ) : (
                formatted
              );
            },
        enableSorting: isLockTree ? false : schema.sortable,
        enableColumnFilter: schema.filterable ?? false,
        filterFn: schema.filterable
          ? (row, columnId, filterValue: string[]) => {
              const val = String(row.getValue(columnId) ?? "");
              return filterValue.includes(val);
            }
          : undefined,
        sortingFn: schema.type === "string" ? "alphanumeric" : "auto",
        sortUndefined: "last" as const,
      });
    }
    return defs;
  }, [allColumns, visibleKeys, isLockTree, snapshotTimestamp, overrideMap]);

  const table = useReactTable({
    data,
    columns: columnDefs,
    state: { sorting, columnFilters, globalFilter },
    onSortingChange: setSorting,
    onColumnFiltersChange: setColumnFilters,
    onGlobalFilterChange: setGlobalFilter,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    getRowId: (row, index) => `${String(row[entityId])}_${index}`,
  });

  const rows = table.getRowModel().rows;

  // Keyboard navigation
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (document.activeElement === filterRef.current) return;

      const currentIndex = rows.findIndex(
        (r) => r.original[entityId] === selectedId,
      );

      let newIndex: number | null = null;

      switch (e.key) {
        case "ArrowDown":
        case "j":
          e.preventDefault();
          newIndex =
            currentIndex < 0 ? 0 : Math.min(currentIndex + 1, rows.length - 1);
          break;
        case "ArrowUp":
        case "k":
          e.preventDefault();
          newIndex =
            currentIndex < 0 ? rows.length - 1 : Math.max(currentIndex - 1, 0);
          break;
        case "PageDown":
          e.preventDefault();
          newIndex =
            currentIndex < 0 ? 0 : Math.min(currentIndex + 20, rows.length - 1);
          break;
        case "PageUp":
          e.preventDefault();
          newIndex = currentIndex < 0 ? 0 : Math.max(currentIndex - 20, 0);
          break;
        case "Home":
          e.preventDefault();
          newIndex = 0;
          break;
        case "End":
          e.preventDefault();
          newIndex = rows.length - 1;
          break;
        case "Enter":
          e.preventDefault();
          if (selectedId == null && rows.length > 0) {
            onSelectRow(rows[0].original[entityId] as string | number);
          }
          return;
        case "/":
          e.preventDefault();
          filterRef.current?.focus();
          return;
      }

      if (newIndex != null && newIndex >= 0 && newIndex < rows.length) {
        const newId = rows[newIndex].original[entityId] as string | number;
        onSelectRow(newId);
        const rowEl = document.getElementById(`row-${newId}`);
        rowEl?.scrollIntoView({ block: "nearest" });
      }
    },
    [rows, selectedId, entityId, onSelectRow, onOpenDetail],
  );

  const handleRowClick = useCallback(
    (row: Record<string, unknown>) => {
      const id = row[entityId] as string | number;
      if (id === selectedId) {
        onSelectRow(null);
      } else {
        onSelectRow(id);
      }
    },
    [entityId, selectedId, onSelectRow],
  );

  const handleViewSwitch = useCallback(
    (v: ViewSchema) => {
      setActiveView(v.key);
      onViewChange?.(v.key);
      if (v.default_sort) {
        setSorting([
          {
            id: v.default_sort,
            desc: v.default_sort_desc ?? false,
          },
        ]);
      } else {
        setSorting([]);
      }
    },
    [onViewChange],
  );

  const handleFilterInput = useCallback(
    (value: string) => {
      setGlobalFilter(value);
      onFilterChange?.(value);
    },
    [onFilterChange],
  );

  return (
    <div
      className="flex flex-col flex-1 min-h-0 h-full outline-none"
      tabIndex={0}
      ref={containerRef}
      onKeyDown={handleKeyDown}
    >
      {/* View tabs + filter */}
      <div className="flex items-center gap-2 px-3 py-1.5 bg-[var(--bg-surface)] border-b border-[var(--border-default)]">
        {views.length > 1 && (
          <div className="flex gap-1">
            {views.map((v) => {
              const viewDesc =
                activeTab && VIEW_DESCRIPTIONS[activeTab]?.[v.key];
              const btn = (
                <button
                  onClick={() => handleViewSwitch(v)}
                  className={`px-2 py-0.5 text-xs rounded-full font-medium transition-colors ${
                    activeView === v.key
                      ? "bg-[var(--accent-subtle)] text-[var(--accent-text)]"
                      : "text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
                  }`}
                >
                  {v.label}
                </button>
              );
              return viewDesc ? (
                <Tooltip key={v.key} content={viewDesc} side="bottom">
                  {btn}
                </Tooltip>
              ) : (
                <span key={v.key}>{btn}</span>
              );
            })}
          </div>
        )}
        <div className="ml-auto flex items-center gap-1.5">
          {toolbarControls}
          <div className="relative">
            <Search
              size={13}
              className="absolute left-2 top-1/2 -translate-y-1/2 text-[var(--text-tertiary)] pointer-events-none"
            />
            <input
              ref={filterRef}
              type="text"
              placeholder="Filter... (/)"
              value={globalFilter}
              onChange={(e) => handleFilterInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  e.stopPropagation();
                  handleFilterInput("");
                  containerRef.current?.focus();
                }
              }}
              className="pl-7 pr-2 py-1 text-xs bg-[var(--bg-elevated)] border border-[var(--border-default)] rounded text-[var(--text-primary)] placeholder:text-[var(--text-tertiary)] focus:outline-none focus:border-[var(--accent)] focus:ring-1 focus:ring-[var(--accent)] w-48 transition-colors"
            />
          </div>
          {columnFilters.length > 0 && (
            <button
              onClick={() => setColumnFilters([])}
              className="flex items-center gap-1 px-1.5 py-0.5 text-[10px] font-medium rounded bg-[var(--accent-subtle)] text-[var(--accent-text)] hover:bg-[var(--accent)] hover:text-white cursor-pointer transition-colors"
            >
              <Filter size={10} />
              {columnFilters.length}
              <X size={10} />
            </button>
          )}
          <span className="text-xs text-[var(--text-tertiary)] tabular-nums font-mono">
            {table.getFilteredRowModel().rows.length} rows
          </span>
        </div>
      </div>

      {/* Table */}
      <div className="flex-1 overflow-auto">
        <table className="w-full text-[13px] font-mono">
          <thead className="sticky top-0 bg-[var(--bg-surface)] z-10">
            {table.getHeaderGroups().map((hg) => (
              <tr key={hg.id}>
                {hg.headers.map((header) => {
                  const isSorted = header.column.getIsSorted();
                  const isFilterable = filterableSet.has(header.id);
                  const hasActiveFilter = columnFilters.some(
                    (f) => f.id === header.id,
                  );
                  const richContent = buildColumnTooltip(header.id);
                  const simpleDesc = COLUMN_DESCRIPTIONS[header.id];
                  const headerContent = (
                    <span className="flex items-center gap-0.5">
                      {flexRender(
                        header.column.columnDef.header,
                        header.getContext(),
                      )}
                      {isSorted === "asc" && (
                        <ChevronUp
                          size={12}
                          className="text-[var(--accent-text)]"
                        />
                      )}
                      {isSorted === "desc" && (
                        <ChevronDown
                          size={12}
                          className="text-[var(--accent-text)]"
                        />
                      )}
                      {sorting.length > 1 && isSorted && (
                        <span className="text-[9px] text-[var(--accent-text)] font-bold">
                          {header.column.getSortIndex() + 1}
                        </span>
                      )}
                    </span>
                  );
                  return (
                    <th
                      key={header.id}
                      onClick={
                        header.column.getCanSort()
                          ? header.column.getToggleSortingHandler()
                          : undefined
                      }
                      className={`px-2 py-1.5 text-left text-xs font-medium border-b-2 border-[var(--border-default)] whitespace-nowrap ${
                        header.column.getCanSort()
                          ? "cursor-pointer select-none hover:text-[var(--text-primary)]"
                          : ""
                      } ${isSorted ? "text-[var(--accent-text)]" : "text-[var(--text-secondary)]"}`}
                    >
                      <span className="inline-flex items-center gap-0.5">
                        {richContent ? (
                          <RichTooltip content={richContent} side="bottom">
                            {headerContent}
                          </RichTooltip>
                        ) : simpleDesc ? (
                          <Tooltip content={simpleDesc} side="bottom">
                            {headerContent}
                          </Tooltip>
                        ) : (
                          headerContent
                        )}
                        {isFilterable && (
                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              const rect =
                                e.currentTarget.getBoundingClientRect();
                              setFilterPopover((prev) =>
                                prev?.columnId === header.id
                                  ? null
                                  : { columnId: header.id, rect },
                              );
                            }}
                            className="p-0.5 rounded hover:bg-[var(--bg-hover)] cursor-pointer"
                          >
                            <Filter
                              size={10}
                              className={
                                hasActiveFilter
                                  ? "text-[var(--accent-text)]"
                                  : "text-[var(--text-tertiary)] opacity-50 hover:opacity-100"
                              }
                            />
                          </button>
                        )}
                      </span>
                    </th>
                  );
                })}
              </tr>
            ))}
          </thead>
          <tbody>
            {rows.map((row, idx) => {
              const rowId = row.original[entityId] as string | number;
              const isSelected = rowId === selectedId;
              return (
                <tr
                  key={row.id}
                  id={`row-${rowId}`}
                  onClick={() => handleRowClick(row.original)}
                  className={`cursor-pointer transition-colors duration-100 ${
                    isSelected
                      ? "bg-[var(--selection-bg)] border-l-[3px] border-l-[var(--selection-border)] shadow-[inset_0_0_0_1px_var(--selection-border)]"
                      : `${idx % 2 === 0 ? "bg-[var(--bg-base)]" : "bg-[var(--bg-overlay)]"} hover:bg-[var(--bg-hover)] border-l-2 border-l-transparent`
                  }`}
                >
                  {row.getVisibleCells().map((cell) => {
                    const rawValue = cell.getValue();
                    const titleText = rawValue != null ? String(rawValue) : "";
                    return (
                      <td
                        key={cell.id}
                        className="px-2 py-1 whitespace-nowrap max-w-md truncate tabular-nums"
                        title={titleText}
                      >
                        {flexRender(
                          cell.column.columnDef.cell,
                          cell.getContext(),
                        )}
                      </td>
                    );
                  })}
                </tr>
              );
            })}
          </tbody>
        </table>
        {rows.length === 0 && (
          <div className="flex items-center justify-center py-16 text-[var(--text-tertiary)]">
            <div className="text-center">
              <Inbox
                size={32}
                className="mx-auto mb-2 text-[var(--text-disabled)]"
              />
              <div className="text-sm font-medium mb-0.5">No data</div>
              {globalFilter && (
                <div className="text-xs">
                  Try adjusting the filter or switching views
                </div>
              )}
            </div>
          </div>
        )}
      </div>

      {/* Column filter popover */}
      {filterPopover && (
        <ColumnFilterPopover
          columnId={filterPopover.columnId}
          data={data}
          columnFilters={columnFilters}
          rect={filterPopover.rect}
          onApply={(columnId, values) => {
            setColumnFilters((prev) => {
              const without = prev.filter((f) => f.id !== columnId);
              if (values === null) return without;
              return [...without, { id: columnId, value: values }];
            });
          }}
          onClose={() => setFilterPopover(null)}
        />
      )}
    </div>
  );
}

// ============================================================
// ColumnFilterPopover
// ============================================================

function ColumnFilterPopover({
  columnId,
  data,
  columnFilters,
  rect,
  onApply,
  onClose,
}: {
  columnId: string;
  data: Record<string, unknown>[];
  columnFilters: ColumnFiltersState;
  rect: DOMRect;
  onApply: (columnId: string, values: string[] | null) => void;
  onClose: () => void;
}) {
  const popoverRef = useRef<HTMLDivElement>(null);
  const [search, setSearch] = useState("");

  const uniqueValues = useMemo(() => {
    const vals = new Set<string>();
    for (const row of data) {
      const v = row[columnId];
      if (v != null) vals.add(String(v));
    }
    return [...vals].sort();
  }, [data, columnId]);

  const currentFilter = columnFilters.find((f) => f.id === columnId);
  const selectedSet = useMemo(
    () => new Set<string>((currentFilter?.value as string[]) ?? uniqueValues),
    [currentFilter, uniqueValues],
  );

  const filtered = useMemo(() => {
    if (!search) return uniqueValues;
    const q = search.toLowerCase();
    return uniqueValues.filter((v) => v.toLowerCase().includes(q));
  }, [uniqueValues, search]);

  const allSelected = filtered.every((v) => selectedSet.has(v));

  const toggle = useCallback(
    (val: string) => {
      const next = new Set(selectedSet);
      if (next.has(val)) {
        next.delete(val);
      } else {
        next.add(val);
      }
      if (next.size >= uniqueValues.length) {
        onApply(columnId, null); // all selected = clear filter
      } else {
        onApply(columnId, next.size > 0 ? [...next] : []);
      }
    },
    [selectedSet, uniqueValues, columnId, onApply],
  );

  const toggleAll = useCallback(() => {
    if (allSelected) {
      // Deselect all visible — apply empty filter (hides everything in this column)
      const next = new Set(selectedSet);
      for (const v of filtered) next.delete(v);
      onApply(columnId, next.size > 0 ? [...next] : []);
    } else {
      // Select all visible
      const next = new Set(selectedSet);
      for (const v of filtered) next.add(v);
      if (next.size >= uniqueValues.length) {
        onApply(columnId, null); // all selected = clear filter
      } else {
        onApply(columnId, [...next]);
      }
    }
  }, [allSelected, selectedSet, filtered, uniqueValues, columnId, onApply]);

  // Close on click outside or Escape
  useEffect(() => {
    const handleClick = (e: MouseEvent) => {
      if (
        popoverRef.current &&
        !popoverRef.current.contains(e.target as Node)
      ) {
        onClose();
      }
    };
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("mousedown", handleClick);
    document.addEventListener("keydown", handleKey);
    return () => {
      document.removeEventListener("mousedown", handleClick);
      document.removeEventListener("keydown", handleKey);
    };
  }, [onClose]);

  // Position: below the filter icon, clamped to viewport
  const style = useMemo(() => {
    const top = rect.bottom + 4;
    const left = Math.min(rect.left, window.innerWidth - 220);
    return { top, left };
  }, [rect]);

  return createPortal(
    <div
      ref={popoverRef}
      className="fixed z-50 w-52 max-h-72 flex flex-col bg-[var(--bg-elevated)] border border-[var(--border-default)] rounded-lg shadow-lg overflow-hidden"
      style={style}
    >
      {/* Search */}
      {uniqueValues.length > 8 && (
        <div className="p-1.5 border-b border-[var(--border-default)]">
          <input
            autoFocus
            type="text"
            placeholder="Search..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="w-full px-2 py-1 text-xs bg-[var(--bg-base)] border border-[var(--border-default)] rounded text-[var(--text-primary)] placeholder:text-[var(--text-tertiary)] focus:outline-none focus:border-[var(--accent)]"
          />
        </div>
      )}
      {/* Select all */}
      <label className="flex items-center gap-2 px-2.5 py-1.5 text-xs font-medium text-[var(--text-secondary)] border-b border-[var(--border-default)] cursor-pointer hover:bg-[var(--bg-hover)]">
        <input
          type="checkbox"
          checked={allSelected}
          onChange={toggleAll}
          className="accent-[var(--accent)]"
        />
        {allSelected ? "Deselect all" : "Select all"}
        <span className="ml-auto text-[var(--text-tertiary)] text-[10px] tabular-nums">
          {filtered.length}
        </span>
      </label>
      {/* Values */}
      <div className="flex-1 overflow-y-auto">
        {filtered.map((val) => (
          <label
            key={val}
            className="flex items-center gap-2 px-2.5 py-1 text-xs text-[var(--text-primary)] cursor-pointer hover:bg-[var(--bg-hover)]"
          >
            <input
              type="checkbox"
              checked={selectedSet.has(val)}
              onChange={() => toggle(val)}
              className="accent-[var(--accent)]"
            />
            <span className="truncate">{val || "(empty)"}</span>
          </label>
        ))}
        {filtered.length === 0 && (
          <div className="px-2.5 py-3 text-xs text-[var(--text-tertiary)] text-center">
            No matches
          </div>
        )}
      </div>
      {/* Clear filter */}
      {currentFilter && (
        <button
          onClick={() => {
            onApply(columnId, null);
            onClose();
          }}
          className="px-2.5 py-1.5 text-xs text-[var(--accent-text)] font-medium border-t border-[var(--border-default)] hover:bg-[var(--bg-hover)] cursor-pointer text-center"
        >
          Clear filter
        </button>
      )}
    </div>,
    document.body,
  );
}
