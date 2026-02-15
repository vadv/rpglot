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

  // Use cgroup CPU when available (container environment)
  const hasCgroup = useMemo(
    () => buckets.some((b) => b.cgroup_cpu > 0),
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
        const cpuVal = hasCgroup ? b.cgroup_cpu : b.cpu;
        const color =
          cpuVal > 600
            ? "var(--status-critical)"
            : cpuVal > 400
              ? "var(--status-warning)"
              : "var(--accent)";
        const opacity = cpuVal > 600 ? 0.6 : cpuVal > 400 ? 0.5 : 0.3;
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
      {/* Error indicators — red dots at top of bars with errors */}
      {buckets.map((b, i) =>
        b.errors > 0 ? (
          <circle
            key={`err-${i}`}
            cx={i + 0.5}
            cy={2}
            r={0.8}
            fill="var(--status-critical)"
            opacity={0.9}
          />
        ) : null,
      )}
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
