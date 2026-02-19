import { useMemo } from "react";
import type { HeatmapBucket } from "../api/types";

interface ActivityHeatmapProps {
  buckets: HeatmapBucket[];
  startTs: number;
  endTs: number;
  currentTs: number;
  hoveredIndex?: number | null;
}

export function ActivityHeatmap({
  buckets,
  startTs,
  endTs,
  currentTs,
  hoveredIndex,
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
      {/* Error indicators — tri-color dots by severity (highest wins) */}
      {buckets.map((b, i) =>
        b.errors_critical > 0 ? (
          <circle
            key={`err-${i}`}
            cx={i + 0.5}
            cy={2}
            r={0.9}
            fill="var(--status-critical)"
            opacity={0.95}
          />
        ) : b.errors_warning > 0 ? (
          <circle
            key={`err-${i}`}
            cx={i + 0.5}
            cy={2}
            r={0.6}
            fill="var(--status-warning)"
            opacity={0.8}
          />
        ) : b.errors_info > 0 ? (
          <circle
            key={`err-${i}`}
            cx={i + 0.5}
            cy={2}
            r={0.4}
            fill="var(--status-inactive)"
            opacity={0.5}
          />
        ) : null,
      )}
      {/* Checkpoint indicators — blue diamonds near top */}
      {buckets.map((b, i) =>
        b.checkpoints > 0 ? (
          <polygon
            key={`ckpt-${i}`}
            points={`${i + 0.5},0.5 ${i + 1},2 ${i + 0.5},3.5 ${i},2`}
            fill="var(--status-info, #38bdf8)"
            opacity={0.8}
          />
        ) : null,
      )}
      {/* Autovacuum indicators — green rectangles at bottom */}
      {buckets.map((b, i) =>
        b.autovacuums > 0 ? (
          <rect
            key={`av-${i}`}
            x={i + 0.15}
            y={22}
            width={0.7}
            height={1.5}
            rx={0.3}
            fill="var(--status-success, #4ade80)"
            opacity={0.7}
          />
        ) : null,
      )}
      {/* Slow query indicators — amber triangles near top */}
      {buckets.map((b, i) =>
        b.slow_queries > 0 ? (
          <polygon
            key={`sq-${i}`}
            points={`${i + 0.5},0.5 ${i + 1},3 ${i},3`}
            fill="var(--status-warning)"
            opacity={0.8}
          />
        ) : null,
      )}
      {hoveredIndex != null && hoveredIndex >= 0 && hoveredIndex < n && (
        <rect
          x={hoveredIndex}
          y={0}
          width={1}
          height={24}
          fill="var(--text-primary)"
          opacity={0.15}
        />
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
