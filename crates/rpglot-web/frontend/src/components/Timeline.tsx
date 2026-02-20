import { useState, useMemo, useCallback, useRef } from "react";
import { createPortal } from "react-dom";
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
} from "lucide-react";
import type { TimezoneMode } from "../utils/formatters";
import { formatTime } from "../utils/formatters";
import type { TimelineInfo, HeatmapBucket } from "../api/types";
import { ActivityHeatmap } from "./ActivityHeatmap";
import { HeatmapTooltip } from "./HeatmapTooltip";

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

  // Heatmap hover tooltip
  const heatmapContainerRef = useRef<HTMLDivElement>(null);
  const [hoveredBucketIdx, setHoveredBucketIdx] = useState<number | null>(null);
  const [tooltipCoords, setTooltipCoords] = useState({ x: 0, y: 0 });

  const handleHeatmapMouseMove = useCallback(
    (e: React.MouseEvent) => {
      if (!heatmapBuckets || heatmapBuckets.length === 0) return;
      const container = heatmapContainerRef.current;
      if (!container) return;
      const rect = container.getBoundingClientRect();
      const relX = e.clientX - rect.left;
      const idx = Math.floor((relX / rect.width) * heatmapBuckets.length);
      if (idx >= 0 && idx < heatmapBuckets.length) {
        setHoveredBucketIdx(idx);
        setTooltipCoords({ x: e.clientX, y: rect.top });
      } else {
        setHoveredBucketIdx(null);
      }
    },
    [heatmapBuckets],
  );

  const handleHeatmapMouseLeave = useCallback(() => {
    setHoveredBucketIdx(null);
  }, []);

  const hoveredBucket =
    hoveredBucketIdx != null && heatmapBuckets
      ? heatmapBuckets[hoveredBucketIdx] ?? null
      : null;

  const hasCgroupCpu = useMemo(
    () => heatmapBuckets?.some((b) => b.cgroup_cpu > 0) ?? false,
    [heatmapBuckets],
  );

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
      <div
        ref={heatmapContainerRef}
        className="relative flex-1 h-8 flex items-center"
        onMouseMove={handleHeatmapMouseMove}
        onMouseLeave={handleHeatmapMouseLeave}
      >
        {heatmapBuckets && heatmapBuckets.length > 0 && (
          <ActivityHeatmap
            buckets={heatmapBuckets}
            startTs={sliderMin}
            endTs={sliderMax}
            currentTs={ts}
            hoveredIndex={hoveredBucketIdx}
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
        {hoveredBucket &&
          createPortal(
            <HeatmapTooltip
              bucket={hoveredBucket}
              x={tooltipCoords.x}
              y={tooltipCoords.y}
              timezone={timezone}
              hasCgroupCpu={hasCgroupCpu}
            />,
            document.body,
          )}
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
