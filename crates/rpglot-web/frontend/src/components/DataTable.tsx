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
import { Search, Inbox, ChevronUp, ChevronDown } from "lucide-react";
import { Tooltip } from "./Tooltip";
import { RichTooltip } from "./RichTooltip";
import type { ColumnSchema, ViewSchema, TabKey } from "../api/types";
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

  const [columnFilters, setColumnFilters] = useState<ColumnFiltersState>([]);
  const [globalFilter, setGlobalFilter] = useState(initialFilter ?? "");

  const containerRef = useRef<HTMLDivElement>(null);
  const filterRef = useRef<HTMLInputElement>(null);

  // Auto-focus table container on mount
  useEffect(() => {
    containerRef.current?.focus();
  }, []);

  const columnDefs = useMemo(() => {
    const colMap = new Map(allColumns.map((c) => [c.key, c]));
    const defs: ColumnDef<Record<string, unknown>>[] = [];
    for (const key of visibleKeys) {
      const schema = colMap.get(key);
      if (!schema) continue;

      const isPglPid = isLockTree && key === "pid";

      defs.push({
        id: key,
        accessorFn: (row) => row[key],
        header: schema.label,
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
                schema.unit,
                schema.format,
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
        sortingFn: schema.type === "string" ? "alphanumeric" : "auto",
        sortUndefined: "last" as const,
      });
    }
    return defs;
  }, [allColumns, visibleKeys, isLockTree, snapshotTimestamp]);

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
                  className={`px-2.5 py-0.5 text-xs rounded font-medium transition-colors ${
                    activeView === v.key
                      ? "bg-[var(--accent)] text-[var(--text-inverse)]"
                      : "bg-[var(--bg-elevated)] text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
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
                      ? "bg-[var(--selection-bg)] border-l-2 border-l-[var(--selection-border)]"
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
    </div>
  );
}
