import type { TimelineInfo } from "../api/types";

interface TimelineProps {
  timeline: TimelineInfo;
  position: number;
  onPositionChange: (position: number) => void;
  timestamp?: number;
}

export function Timeline({
  timeline,
  position,
  onPositionChange,
  timestamp,
}: TimelineProps) {
  const total = timeline.total_snapshots;
  if (total === 0) return null;

  const ts = timestamp ?? 0;
  const date = ts > 0 ? new Date(ts * 1000).toLocaleTimeString() : "-";

  return (
    <div className="flex items-center gap-3 px-4 py-1.5 bg-[var(--bg-surface)] border-t border-[var(--border-default)] text-xs">
      <span className="text-[var(--text-tertiary)]">History</span>
      <input
        type="range"
        min={0}
        max={total - 1}
        value={position}
        onChange={(e) => onPositionChange(Number(e.target.value))}
        className="flex-1 h-1"
        style={{
          accentColor: "var(--accent)",
        }}
      />
      <span className="text-[var(--text-secondary)] font-mono tabular-nums">
        {position + 1}/{total}
      </span>
      <span className="text-[var(--text-tertiary)]">{date}</span>
    </div>
  );
}
