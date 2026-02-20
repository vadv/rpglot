import type { ReactNode } from "react";
import type { HeatmapBucket } from "../api/types";
import type { TimezoneMode } from "../utils/formatters";
import { formatTime } from "../utils/formatters";

export function HeatmapTooltip({
  bucket,
  x,
  y,
  timezone,
  hasCgroupCpu,
}: {
  bucket: HeatmapBucket;
  x: number;
  y: number;
  timezone: TimezoneMode;
  hasCgroupCpu: boolean;
}) {
  const cpuVal = hasCgroupCpu ? bucket.cgroup_cpu : bucket.cpu;
  const cpuPct = (cpuVal / 100).toFixed(1);
  const totalErrors =
    bucket.errors_critical + bucket.errors_warning + bucket.errors_info;

  const items: { color: string; shape: ReactNode; label: string }[] = [];

  // CPU / active sessions — always show
  items.push({
    color:
      cpuVal > 600
        ? "var(--status-critical)"
        : cpuVal > 400
          ? "var(--status-warning)"
          : "var(--accent)",
    shape: (
      <span
        className="inline-block w-2.5 h-3 rounded-[1px]"
        style={{
          backgroundColor:
            cpuVal > 600
              ? "var(--status-critical)"
              : cpuVal > 400
                ? "var(--status-warning)"
                : "var(--accent)",
          opacity: cpuVal > 0 ? 0.7 : 0.2,
        }}
      />
    ),
    label: `CPU ${cpuPct}%, ${bucket.active} active`,
  });

  if (totalErrors > 0) {
    const parts: string[] = [];
    if (bucket.errors_critical > 0)
      parts.push(`${bucket.errors_critical} critical`);
    if (bucket.errors_warning > 0)
      parts.push(`${bucket.errors_warning} warning`);
    if (bucket.errors_info > 0) parts.push(`${bucket.errors_info} info`);
    const errorColor =
      bucket.errors_critical > 0
        ? "var(--status-critical)"
        : bucket.errors_warning > 0
          ? "var(--status-warning)"
          : "var(--status-inactive)";
    items.push({
      color: errorColor,
      shape: (
        <span
          className="inline-block w-2.5 h-2.5 rounded-full"
          style={{ backgroundColor: errorColor }}
        />
      ),
      label: `Errors: ${parts.join(", ")}`,
    });
  }

  if (bucket.checkpoints > 0) {
    items.push({
      color: "var(--status-info, #38bdf8)",
      shape: (
        <svg width="10" height="10" viewBox="0 0 10 10" className="inline-block">
          <polygon
            points="5,0.5 9,5 5,9.5 1,5"
            fill="var(--status-info, #38bdf8)"
          />
        </svg>
      ),
      label: `Checkpoints: ${bucket.checkpoints}`,
    });
  }

  if (bucket.autovacuums > 0) {
    items.push({
      color: "var(--status-success, #4ade80)",
      shape: (
        <span
          className="inline-block w-2.5 h-2 rounded-sm"
          style={{ backgroundColor: "var(--status-success, #4ade80)" }}
        />
      ),
      label: `Autovacuums: ${bucket.autovacuums}`,
    });
  }

  if (bucket.slow_queries > 0) {
    items.push({
      color: "var(--status-warning)",
      shape: (
        <svg width="10" height="10" viewBox="0 0 10 10" className="inline-block">
          <polygon points="5,1 9.5,9 0.5,9" fill="var(--status-warning)" />
        </svg>
      ),
      label: `Slow queries: ${bucket.slow_queries}`,
    });
  }

  // Clamp tooltip to viewport
  const tooltipX = Math.max(90, Math.min(x, window.innerWidth - 90));

  return (
    <div
      className="fixed z-[9999] px-2.5 py-1.5 text-[11px] rounded-lg
        bg-[var(--bg-elevated)] text-[var(--text-primary)] border border-[var(--border-default)]
        pointer-events-none"
      style={{
        left: tooltipX,
        top: y - 16,
        transform: "translate(-50%, -100%)",
        boxShadow: "var(--shadow-md)",
      }}
    >
      <div className="font-mono text-[var(--text-tertiary)] mb-0.5">
        {formatTime(bucket.ts, timezone)}
      </div>
      <div className="flex flex-col gap-0.5">
        {items.map((item, i) => (
          <div key={i} className="flex items-center gap-1.5">
            <span className="flex-shrink-0 w-2.5 flex items-center justify-center">
              {item.shape}
            </span>
            <span>{item.label}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
