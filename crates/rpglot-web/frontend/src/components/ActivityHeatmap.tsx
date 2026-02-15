import { useMemo } from "react";
import type { HeatmapBucket } from "../api/types";

interface ActivityHeatmapProps {
  buckets: HeatmapBucket[];
  startTs: number;
  endTs: number;
  currentTs: number;
}

export function ActivityHeatmap({
  buckets,
  startTs,
  endTs,
  currentTs,
}: ActivityHeatmapProps) {
  const maxActive = useMemo(
    () => Math.max(1, ...buckets.map((b) => b.active)),
    [buckets],
  );

  const range = endTs - startTs;
  if (buckets.length === 0 || range <= 0) return null;

  const n = buckets.length;

  return (
    <svg
      className="absolute inset-0 w-full h-full pointer-events-none"
      viewBox={`0 0 ${n} 24`}
      preserveAspectRatio="none"
    >
      {buckets.map((b, i) => {
        const height = (b.active / maxActive) * 24;
        const color =
          b.cpu > 600
            ? "var(--status-critical)"
            : b.cpu > 400
              ? "var(--status-warning)"
              : "var(--accent)";
        const opacity = b.cpu > 600 ? 0.6 : b.cpu > 400 ? 0.5 : 0.3;
        return (
          <rect
            key={i}
            x={i}
            y={24 - height}
            width={1}
            height={height}
            fill={color}
            opacity={opacity}
          />
        );
      })}
      {currentTs > startTs && currentTs < endTs && (
        <line
          x1={((currentTs - startTs) / range) * n}
          y1={0}
          x2={((currentTs - startTs) / range) * n}
          y2={24}
          stroke="var(--text-primary)"
          strokeWidth={0.5}
          opacity={0.7}
        />
      )}
    </svg>
  );
}
