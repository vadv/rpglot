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
import type { ColumnSchema, ViewSchema } from "../api/types";
import { formatValue } from "../utils/formatters";

interface DataTableProps {
  data: Record<string, unknown>[];
  columns: ColumnSchema[];
  views: ViewSchema[];
  entityId: string;
  selectedId: string | number | null;
  onSelectRow: (id: string | number | null) => void;
  onOpenDetail: () => void;
  isLockTree?: boolean;
  initialView?: string | null;
  initialFilter?: string | null;
  onViewChange?: (view: string) => void;
  onFilterChange?: (filter: string) => void;
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
  initialView,
  initialFilter,
  onViewChange,
  onFilterChange,
}: DataTableProps) {
  const [activeView, setActiveView] = useState(() => {
    // Use URL-provided view if it matches available views
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

      // PGL lock tree: indent PID column based on depth
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
              return prefix + String(info.getValue() ?? "-");
            }
          : (info) => formatValue(info.getValue(), schema.unit, schema.format),
        enableSorting: isLockTree ? false : schema.sortable,
        sortingFn: schema.type === "string" ? "alphanumeric" : "auto",
        sortUndefined: "last" as const,
      });
    }
    return defs;
  }, [allColumns, visibleKeys, isLockTree]);

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
      // Don't capture when filter input is focused
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
          if (selectedId != null) {
            onOpenDetail();
          } else if (rows.length > 0) {
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
        onOpenDetail();
      } else {
        onSelectRow(id);
      }
    },
    [entityId, selectedId, onSelectRow, onOpenDetail],
  );

  const handleViewSwitch = useCallback(
    (v: ViewSchema) => {
      setActiveView(v.key);
      onViewChange?.(v.key);
      if (v.default_sort) {
        setSorting([
          { id: v.default_sort, desc: v.default_sort_desc ?? false },
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
      <div className="flex items-center gap-2 px-3 py-1.5 bg-slate-800/20 border-b border-slate-700/50">
        {views.length > 1 && (
          <div className="flex gap-1">
            {views.map((v) => (
              <button
                key={v.key}
                onClick={() => handleViewSwitch(v)}
                className={`px-2 py-0.5 text-xs rounded transition-colors ${
                  activeView === v.key
                    ? "bg-blue-600 text-white"
                    : "bg-slate-700 text-slate-400 hover:text-white"
                }`}
              >
                {v.label}
              </button>
            ))}
          </div>
        )}
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
          className="ml-auto px-2 py-0.5 text-xs bg-slate-800 border border-slate-600 rounded text-slate-300 placeholder-slate-500 focus:outline-none focus:border-blue-500 w-48"
        />
        <span className="text-xs text-slate-500">
          {table.getFilteredRowModel().rows.length} rows
        </span>
      </div>

      {/* Table */}
      <div className="flex-1 overflow-auto">
        <table className="w-full text-xs">
          <thead className="sticky top-0 bg-slate-800 z-10">
            {table.getHeaderGroups().map((hg) => (
              <tr key={hg.id}>
                {hg.headers.map((header) => {
                  const isSorted = header.column.getIsSorted();
                  return (
                    <th
                      key={header.id}
                      onClick={
                        header.column.getCanSort()
                          ? header.column.getToggleSortingHandler()
                          : undefined
                      }
                      className={`px-2 py-1.5 text-left font-medium border-b border-slate-700 whitespace-nowrap ${
                        header.column.getCanSort()
                          ? "cursor-pointer select-none hover:text-slate-200"
                          : ""
                      } ${isSorted ? "text-blue-400" : "text-slate-400"}`}
                    >
                      <span className="flex items-center gap-1">
                        {flexRender(
                          header.column.columnDef.header,
                          header.getContext(),
                        )}
                        {isSorted === "asc" && (
                          <span className="text-blue-400">^</span>
                        )}
                        {isSorted === "desc" && (
                          <span className="text-blue-400">v</span>
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
                  className={`cursor-pointer border-b border-slate-800/30 transition-colors duration-100 ${
                    isSelected
                      ? "bg-blue-900/40 text-white"
                      : `${idx % 2 === 0 ? "bg-transparent" : "bg-slate-800/15"} hover:bg-slate-700/30 text-slate-300`
                  }`}
                >
                  {row.getVisibleCells().map((cell) => (
                    <td
                      key={cell.id}
                      className="px-2 py-1 whitespace-nowrap max-w-md truncate"
                    >
                      {flexRender(
                        cell.column.columnDef.cell,
                        cell.getContext(),
                      )}
                    </td>
                  ))}
                </tr>
              );
            })}
          </tbody>
        </table>
        {rows.length === 0 && (
          <div className="flex items-center justify-center py-12 text-slate-500 text-sm">
            <div className="text-center">
              <div className="text-lg mb-1">No data</div>
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
