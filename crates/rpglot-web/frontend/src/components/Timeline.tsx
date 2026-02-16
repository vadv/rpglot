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
import type { TimelineInfo, DateInfo, HeatmapBucket } from "../api/types";
import { ActivityHeatmap } from "./ActivityHeatmap";

// ============================================================
// Timeline — intra-day slider with time display
// ============================================================

interface TimelineProps {
  timeline: TimelineInfo;
  onTimestampJump: (timestamp: number, direction?: "floor" | "ceil") => void;
  timestamp?: number;
  prevTimestamp?: number;
  nextTimestamp?: number;
  timezone: TimezoneMode;
  heatmapBuckets?: HeatmapBucket[];
  hourStart?: number;
  hourEnd?: number;
  playSpeed?: number | null;
  onPlayToggle?: () => void;
  liveFollow?: boolean;
  onLiveToggle?: () => void;
  analyzing?: boolean;
  onAnalyze?: () => void;
}

export function Timeline({
  timeline,
  onTimestampJump,
  timestamp,
  prevTimestamp,
  nextTimestamp,
  timezone,
  heatmapBuckets,
  hourStart,
  hourEnd,
  playSpeed,
  onPlayToggle,
  liveFollow,
  onLiveToggle,
  analyzing,
  onAnalyze,
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

  // Use hour boundaries if available, otherwise fall back to day/global range
  const sliderMin =
    hourStart != null
      ? hourStart
      : currentDateInfo
        ? currentDateInfo.first_timestamp
        : timeline.start;
  const sliderMax =
    hourEnd != null
      ? hourEnd
      : currentDateInfo
        ? currentDateInfo.last_timestamp
        : timeline.end;

  // Time labels for slider endpoints
  const startTime = sliderMin > 0 ? formatTime(sliderMin, timezone) : "";
  const endTime = sliderMax > 0 ? formatTime(sliderMax, timezone) : "";

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

      {/* Intra-day slider with heatmap overlay */}
      <div className="relative flex-1 h-6 flex items-center">
        {heatmapBuckets && heatmapBuckets.length > 0 && (
          <ActivityHeatmap
            buckets={heatmapBuckets}
            startTs={sliderMin}
            endTs={sliderMax}
            currentTs={ts}
          />
        )}
        <input
          type="range"
          min={sliderMin}
          max={sliderMax}
          value={Math.min(Math.max(ts, sliderMin), sliderMax)}
          onChange={handleSliderChange}
          className="relative z-10 w-full h-1 opacity-80"
          style={{ accentColor: "var(--accent)" }}
        />
      </div>

      {/* End time label */}
      {endTime && (
        <span className="text-[var(--text-tertiary)] font-mono tabular-nums text-[10px]">
          {endTime}
        </span>
      )}

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
        disabled={nextTimestamp == null || !!liveFollow}
        title="Next snapshot"
      >
        <ChevronRight size={14} />
      </StepButton>

      {/* Play button */}
      {onPlayToggle && (
        <button
          onClick={onPlayToggle}
          className={`px-2 py-0.5 rounded text-xs font-medium transition-colors ${
            playSpeed != null
              ? "bg-[var(--accent)] text-white animate-pulse-btn"
              : "bg-[var(--bg-elevated)] text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
          }`}
          title={
            playSpeed != null
              ? `Playing x${playSpeed} — click to speed up, Space to stop`
              : "Play forward (1 snap/sec)"
          }
        >
          {playSpeed == null && "\u25B6 Play"}
          {playSpeed === 1 && "\u25B6"}
          {playSpeed === 2 && "\u25B6\u25B6"}
          {playSpeed === 4 && "\u25B6\u25B6\u25B6"}
          {playSpeed === 8 && "\u25B6\u25B6\u25B6\u25B6"}
        </button>
      )}

      {/* Live button */}
      {onLiveToggle && (
        <button
          onClick={onLiveToggle}
          className={`px-2 py-0.5 rounded text-xs font-medium transition-colors ${
            liveFollow
              ? "bg-[var(--status-success-bg)] text-[var(--status-success)] animate-pulse-btn"
              : "bg-[var(--bg-elevated)] text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
          }`}
          title={
            liveFollow
              ? "Following latest \u2014 click or Space to stop"
              : "Follow latest snapshots"
          }
        >
          {liveFollow ? "\u25CF Live" : "\u25C9 Live"}
        </button>
      )}

      {/* Analyze button */}
      {onAnalyze && (
        <button
          onClick={onAnalyze}
          disabled={analyzing}
          className={`px-2 py-0.5 rounded text-xs font-medium transition-colors ${
            analyzing
              ? "bg-[var(--accent)] text-white animate-pulse-btn"
              : "bg-[var(--bg-elevated)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
          }`}
          title="Analyze current hour for anomalies"
        >
          {analyzing ? "Analyzing\u2026" : "\u26A1 Analyze"}
        </button>
      )}
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
