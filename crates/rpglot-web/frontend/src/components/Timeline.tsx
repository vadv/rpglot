import { useState, useEffect, useMemo, useCallback, useRef } from "react";
import { createPortal } from "react-dom";
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
} from "lucide-react";
import type { TimezoneMode } from "../utils/formatters";
import {
  formatTime,
  getDatePartsInTz,
  dateToEpochInTz,
} from "../utils/formatters";
import type { TimelineInfo, DateInfo } from "../api/types";

// ============================================================
// Timeline — intra-day slider with time display
// ============================================================

interface TimelineProps {
  timeline: TimelineInfo;
  onTimestampJump: (timestamp: number) => void;
  timestamp?: number;
  prevTimestamp?: number;
  nextTimestamp?: number;
  timezone: TimezoneMode;
}

export function Timeline({
  timeline,
  onTimestampJump,
  timestamp,
  prevTimestamp,
  nextTimestamp,
  timezone,
}: TimelineProps) {
  const total = timeline.total_snapshots;
  if (total === 0) return null;

  const ts = timestamp ?? 0;
  const dates = timeline.dates;

  // Find which date the current timestamp belongs to
  const currentDateInfo = useMemo(() => {
    if (!dates || dates.length === 0 || ts <= 0) return null;
    for (let i = dates.length - 1; i >= 0; i--) {
      if (ts >= dates[i].first_timestamp) return dates[i];
    }
    return dates[0];
  }, [dates, ts]);

  const sliderMin = currentDateInfo
    ? currentDateInfo.first_timestamp
    : timeline.start;
  const sliderMax = currentDateInfo
    ? currentDateInfo.last_timestamp
    : timeline.end;

  // Time labels for slider endpoints
  const startTime = currentDateInfo
    ? formatTime(currentDateInfo.first_timestamp, timezone)
    : "";
  const endTime = currentDateInfo
    ? formatTime(currentDateInfo.last_timestamp, timezone)
    : "";
  const currentTime = ts > 0 ? formatTime(ts, timezone) : "-";

  const handleSliderChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      onTimestampJump(Number(e.target.value));
    },
    [onTimestampJump],
  );

  const handlePrev = useCallback(() => {
    if (prevTimestamp != null) onTimestampJump(prevTimestamp);
  }, [prevTimestamp, onTimestampJump]);

  const handleNext = useCallback(() => {
    if (nextTimestamp != null) onTimestampJump(nextTimestamp);
  }, [nextTimestamp, onTimestampJump]);

  const handlePrevHour = useCallback(() => {
    if (ts > timeline.start) onTimestampJump(ts - 3600);
  }, [ts, timeline.start, onTimestampJump]);

  const handleNextHour = useCallback(() => {
    if (ts < timeline.end) onTimestampJump(ts + 3600);
  }, [ts, timeline.end, onTimestampJump]);

  return (
    <div className="flex items-center gap-2 px-4 py-1.5 bg-[var(--bg-surface)] border-t border-[var(--border-default)] text-xs">
      {/* Prev snapshot */}
      <StepButton
        onClick={handlePrev}
        disabled={prevTimestamp == null}
        title="Previous snapshot"
      >
        <ChevronLeft size={14} />
      </StepButton>

      {/* Prev hour */}
      <StepButton
        onClick={handlePrevHour}
        disabled={ts <= timeline.start}
        title="Back 1 hour"
      >
        <ChevronsLeft size={14} />
      </StepButton>

      {/* Start time label */}
      {startTime && (
        <span className="text-[var(--text-tertiary)] font-mono tabular-nums text-[10px]">
          {startTime}
        </span>
      )}

      {/* Intra-day slider */}
      <input
        type="range"
        min={sliderMin}
        max={sliderMax}
        value={Math.min(Math.max(ts, sliderMin), sliderMax)}
        onChange={handleSliderChange}
        className="flex-1 h-1"
        style={{ accentColor: "var(--accent)" }}
      />

      {/* End time label */}
      {endTime && (
        <span className="text-[var(--text-tertiary)] font-mono tabular-nums text-[10px]">
          {endTime}
        </span>
      )}

      {/* Current time */}
      <span className="text-[var(--text-primary)] font-mono tabular-nums font-medium">
        {currentTime}
      </span>

      {/* Next hour */}
      <StepButton
        onClick={handleNextHour}
        disabled={ts >= timeline.end}
        title="Forward 1 hour"
      >
        <ChevronsRight size={14} />
      </StepButton>

      {/* Next snapshot */}
      <StepButton
        onClick={handleNext}
        disabled={nextTimestamp == null}
        title="Next snapshot"
      >
        <ChevronRight size={14} />
      </StepButton>
    </div>
  );
}

// ============================================================
// Step Button
// ============================================================

function StepButton({
  onClick,
  disabled,
  title,
  children,
}: {
  onClick: () => void;
  disabled: boolean;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      title={title}
      className="p-0.5 rounded text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] disabled:opacity-30 disabled:cursor-default transition-colors"
    >
      {children}
    </button>
  );
}

// ============================================================
// Time Input (used by Header for time editing)
// ============================================================

export function TimeInput({
  timestamp,
  timezone,
  onSubmit,
}: {
  timestamp: number;
  timezone: TimezoneMode;
  onSubmit: (epoch: number) => void;
}) {
  const displayTime = formatTime(timestamp, timezone);
  const [value, setValue] = useState(displayTime);
  const [editing, setEditing] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  // Sync display when timestamp changes (and not editing)
  useEffect(() => {
    if (!editing) {
      setValue(formatTime(timestamp, timezone));
    }
  }, [timestamp, timezone, editing]);

  const handleFocus = useCallback(() => {
    setEditing(true);
    setValue(formatTime(timestamp, timezone));
  }, [timestamp, timezone]);

  const handleBlur = useCallback(() => {
    setEditing(false);
    setValue(formatTime(timestamp, timezone));
  }, [timestamp, timezone]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        const parsed = parseTimeString(value);
        if (parsed) {
          const parts = getDatePartsInTz(timestamp, timezone);
          const epoch = dateToEpochInTz(
            parts.year,
            parts.month,
            parts.day,
            parsed.hour,
            parsed.minute,
            parsed.second,
            timezone,
          );
          onSubmit(epoch);
        }
        setEditing(false);
        inputRef.current?.blur();
      } else if (e.key === "Escape") {
        e.preventDefault();
        setEditing(false);
        setValue(formatTime(timestamp, timezone));
        inputRef.current?.blur();
      }
    },
    [value, timestamp, timezone, onSubmit],
  );

  return (
    <input
      ref={inputRef}
      type="text"
      value={value}
      onChange={(e) => setValue(e.target.value)}
      onFocus={handleFocus}
      onBlur={handleBlur}
      onKeyDown={handleKeyDown}
      className="w-[68px] font-mono text-xs text-center py-0.5 bg-transparent text-[var(--text-primary)] border-b border-[var(--border-default)] focus:border-[var(--accent)] focus:outline-none transition-colors"
      title="Type HH:MM:SS and press Enter to jump"
    />
  );
}

function parseTimeString(
  s: string,
): { hour: number; minute: number; second: number } | null {
  const m = s.trim().match(/^(\d{1,2}):(\d{2}):(\d{2})$/);
  if (!m) return null;
  const hour = Number(m[1]);
  const minute = Number(m[2]);
  const second = Number(m[3]);
  if (hour > 23 || minute > 59 || second > 59) return null;
  return { hour, minute, second };
}

// ============================================================
// Calendar Popover (exported for use in Header)
// ============================================================

const WEEKDAYS = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

export function CalendarPopover({
  dates,
  currentDate,
  onSelectDate,
  onClose,
  anchorRect,
}: {
  dates: DateInfo[];
  currentDate: string;
  onSelectDate: (date: DateInfo) => void;
  onClose: () => void;
  anchorRect: DOMRect;
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

  // Position: below anchor, centered
  const style: React.CSSProperties = {
    position: "fixed",
    left: Math.max(8, anchorRect.left + anchorRect.width / 2 - 120),
    top: anchorRect.bottom + 4,
    zIndex: 9999,
  };

  return createPortal(
    <div
      ref={popoverRef}
      className="w-[240px] p-2 rounded-lg shadow-lg bg-[var(--bg-surface)] border border-[var(--border-default)]"
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
  const prevMonth = month === 1 ? 12 : month - 1;

  // Fill leading days from previous month
  for (let i = startOffset - 1; i >= 0; i--) {
    cells.push({
      year: prevYear,
      month: prevMonth,
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
  const nextMonth = month === 12 ? 1 : month + 1;
  let nextDay = 1;
  while (cells.length < 42) {
    cells.push({
      year: nextYear,
      month: nextMonth,
      day: nextDay++,
      inMonth: false,
    });
  }

  return cells;
}
