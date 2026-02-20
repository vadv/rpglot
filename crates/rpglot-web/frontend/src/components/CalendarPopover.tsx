import { useState, useEffect, useMemo, useCallback, useRef } from "react";
import { createPortal } from "react-dom";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type { TimezoneMode } from "../utils/formatters";
import { getDatePartsInTz } from "../utils/formatters";
import type { DateInfo } from "../api/types";

const WEEKDAYS = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

export function CalendarPopover({
  dates,
  currentDate,
  onSelectDate,
  onClose,
  anchorRect,
  currentHour,
  onSelectHour,
  timezone,
}: {
  dates: DateInfo[];
  currentDate: string;
  onSelectDate: (date: DateInfo) => void;
  onClose: () => void;
  anchorRect: DOMRect;
  currentHour?: number;
  onSelectHour?: (hour: number) => void;
  timezone?: TimezoneMode;
}) {
  const popoverRef = useRef<HTMLDivElement>(null);

  const dataDateMap = useMemo(
    () => new Map(dates.map((d) => [d.date, d])),
    [dates],
  );
  const dataDateSet = useMemo(() => new Set(dates.map((d) => d.date)), [dates]);

  // Parse currentDate to initialize view month
  const initYear =
    currentDate !== "-"
      ? Number(currentDate.slice(0, 4))
      : new Date().getFullYear();
  const initMonth =
    currentDate !== "-"
      ? Number(currentDate.slice(5, 7))
      : new Date().getMonth() + 1;

  const [viewYear, setViewYear] = useState(initYear);
  const [viewMonth, setViewMonth] = useState(initMonth);

  const todayStr = useMemo(() => {
    const now = new Date();
    return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
  }, []);

  // Close on click outside
  useEffect(() => {
    function handleMouseDown(e: MouseEvent) {
      if (
        popoverRef.current &&
        !popoverRef.current.contains(e.target as Node)
      ) {
        onClose();
      }
    }
    document.addEventListener("mousedown", handleMouseDown);
    return () => document.removeEventListener("mousedown", handleMouseDown);
  }, [onClose]);

  // Close on Escape
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    }
    document.addEventListener("keydown", handleKeyDown, true);
    return () => document.removeEventListener("keydown", handleKeyDown, true);
  }, [onClose]);

  const prevMonth = useCallback(() => {
    setViewMonth((m) => {
      if (m === 1) {
        setViewYear((y) => y - 1);
        return 12;
      }
      return m - 1;
    });
  }, []);

  const nextMonth = useCallback(() => {
    setViewMonth((m) => {
      if (m === 12) {
        setViewYear((y) => y + 1);
        return 1;
      }
      return m + 1;
    });
  }, []);

  const grid = useMemo(
    () => generateCalendarGrid(viewYear, viewMonth),
    [viewYear, viewMonth],
  );

  const MONTH_NAMES = [
    "Jan",
    "Feb",
    "Mar",
    "Apr",
    "May",
    "Jun",
    "Jul",
    "Aug",
    "Sep",
    "Oct",
    "Nov",
    "Dec",
  ];

  // Determine which hours have data for the currently selected date
  const selectedDateInfo = useMemo(
    () => dataDateMap.get(currentDate) ?? null,
    [dataDateMap, currentDate],
  );

  const hourAvailability = useMemo(() => {
    if (!selectedDateInfo || !timezone) return null;
    const startParts = getDatePartsInTz(
      selectedDateInfo.first_timestamp,
      timezone,
    );
    const endParts = getDatePartsInTz(
      selectedDateInfo.last_timestamp,
      timezone,
    );
    return { startHour: startParts.hour, endHour: endParts.hour };
  }, [selectedDateInfo, timezone]);

  // Position: below anchor, centered
  const style: React.CSSProperties = {
    position: "fixed",
    left: Math.max(8, anchorRect.left + anchorRect.width / 2 - 140),
    top: anchorRect.bottom + 4,
    zIndex: 9999,
  };

  return createPortal(
    <div
      ref={popoverRef}
      className="w-[280px] p-2 rounded-lg shadow-lg bg-[var(--bg-surface)] border border-[var(--border-default)]"
      style={style}
    >
      {/* Month navigation */}
      <div className="flex items-center justify-between mb-1.5">
        <button
          onClick={prevMonth}
          className="p-0.5 rounded text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors"
        >
          <ChevronLeft size={14} />
        </button>
        <span className="text-xs font-semibold text-[var(--text-primary)]">
          {MONTH_NAMES[viewMonth - 1]} {viewYear}
        </span>
        <button
          onClick={nextMonth}
          className="p-0.5 rounded text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors"
        >
          <ChevronRight size={14} />
        </button>
      </div>

      {/* Weekday headers */}
      <div className="grid grid-cols-7 gap-0.5 mb-0.5">
        {WEEKDAYS.map((d) => (
          <div
            key={d}
            className="text-center text-[10px] font-medium text-[var(--text-tertiary)]"
          >
            {d}
          </div>
        ))}
      </div>

      {/* Day cells */}
      <div className="grid grid-cols-7 gap-0.5">
        {grid.map((cell, i) => {
          const dateStr = `${cell.year}-${String(cell.month).padStart(2, "0")}-${String(cell.day).padStart(2, "0")}`;
          const hasData = dataDateSet.has(dateStr);
          const isSelected = dateStr === currentDate;
          const isToday = dateStr === todayStr;
          const inMonth = cell.inMonth;

          return (
            <button
              key={i}
              disabled={!hasData}
              onClick={() => {
                const info = dataDateMap.get(dateStr);
                if (info) onSelectDate(info);
              }}
              className={`
                w-full aspect-square flex items-center justify-center text-[11px] rounded transition-colors
                ${!inMonth ? "opacity-30" : ""}
                ${isSelected ? "bg-[var(--accent)] text-white font-semibold" : ""}
                ${isToday && !isSelected ? "ring-1 ring-[var(--accent)]" : ""}
                ${hasData && !isSelected ? "text-[var(--text-primary)] hover:bg-[var(--bg-hover)] cursor-pointer" : ""}
                ${!hasData ? "text-[var(--text-disabled)] cursor-default" : ""}
              `}
            >
              {cell.day}
            </button>
          );
        })}
      </div>

      {/* Hour grid */}
      {onSelectHour && (
        <>
          <div className="border-t border-[var(--border-default)] mt-1.5 mb-1.5" />
          <div className="grid grid-cols-6 gap-0.5">
            {Array.from({ length: 24 }, (_, h) => {
              const hasHourData =
                hourAvailability != null &&
                h >= hourAvailability.startHour &&
                h <= hourAvailability.endHour;
              const isSelectedHour = h === currentHour;

              return (
                <button
                  key={h}
                  disabled={!hasHourData}
                  onClick={() => {
                    onSelectHour(h);
                    onClose();
                  }}
                  className={`
                    flex items-center justify-center text-[11px] rounded py-0.5 transition-colors
                    ${isSelectedHour ? "bg-[var(--accent)] text-white font-semibold" : ""}
                    ${hasHourData && !isSelectedHour ? "text-[var(--text-primary)] hover:bg-[var(--bg-hover)] cursor-pointer" : ""}
                    ${!hasHourData ? "text-[var(--text-disabled)] cursor-default" : ""}
                  `}
                >
                  {String(h).padStart(2, "0")}
                </button>
              );
            })}
          </div>
        </>
      )}
    </div>,
    document.body,
  );
}

/** Generate a 42-cell calendar grid for the given month. */
function generateCalendarGrid(
  year: number,
  month: number,
): { year: number; month: number; day: number; inMonth: boolean }[] {
  const cells: {
    year: number;
    month: number;
    day: number;
    inMonth: boolean;
  }[] = [];

  // First day of the month (0=Sun, 1=Mon, ..., 6=Sat)
  const firstDow = new Date(year, month - 1, 1).getDay();
  // Convert to Monday-based (0=Mon, ..., 6=Sun)
  const startOffset = firstDow === 0 ? 6 : firstDow - 1;

  // Days in the month
  const daysInMonth = new Date(year, month, 0).getDate();

  // Previous month
  const prevMonthDays = new Date(year, month - 1, 0).getDate();
  const prevYear = month === 1 ? year - 1 : year;
  const prevMon = month === 1 ? 12 : month - 1;

  // Fill leading days from previous month
  for (let i = startOffset - 1; i >= 0; i--) {
    cells.push({
      year: prevYear,
      month: prevMon,
      day: prevMonthDays - i,
      inMonth: false,
    });
  }

  // Current month days
  for (let d = 1; d <= daysInMonth; d++) {
    cells.push({ year, month, day: d, inMonth: true });
  }

  // Fill trailing days from next month
  const nextYear = month === 12 ? year + 1 : year;
  const nextMon = month === 12 ? 1 : month + 1;
  let nextDay = 1;
  while (cells.length < 42) {
    cells.push({
      year: nextYear,
      month: nextMon,
      day: nextDay++,
      inMonth: false,
    });
  }

  return cells;
}
