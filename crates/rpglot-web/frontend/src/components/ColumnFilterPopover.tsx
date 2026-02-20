import { useMemo, useState, useRef, useEffect, useCallback } from "react";
import { createPortal } from "react-dom";
import type { ColumnFiltersState } from "@tanstack/react-table";

export function ColumnFilterPopover({
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
