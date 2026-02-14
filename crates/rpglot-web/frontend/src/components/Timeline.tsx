import type { TimelineInfo } from '../api/types';

interface TimelineProps {
  timeline: TimelineInfo;
  position: number;
  onPositionChange: (position: number) => void;
  timestamp?: number;
}

export function Timeline({ timeline, position, onPositionChange, timestamp }: TimelineProps) {
  const total = timeline.total_snapshots;
  if (total === 0) return null;

  const ts = timestamp ?? 0;
  const date = ts > 0 ? new Date(ts * 1000).toLocaleTimeString() : '-';

  return (
    <div className="flex items-center gap-3 px-4 py-1.5 bg-slate-800/30 border-t border-slate-700 text-xs">
      <span className="text-slate-500">History</span>
      <input
        type="range"
        min={0}
        max={total - 1}
        value={position}
        onChange={(e) => onPositionChange(Number(e.target.value))}
        className="flex-1 h-1 accent-blue-500"
      />
      <span className="text-slate-400 tabular-nums">
        {position + 1}/{total}
      </span>
      <span className="text-slate-500">{date}</span>
    </div>
  );
}
