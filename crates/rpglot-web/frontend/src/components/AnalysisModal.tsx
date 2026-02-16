import { useState, useEffect, useCallback, useMemo } from "react";
import { createPortal } from "react-dom";
import { X, ChevronDown, ChevronRight, Copy, Check } from "lucide-react";
import type {
  AnalysisReport,
  AnalysisIncident,
  AnalysisRecommendation,
  TabKey,
} from "../api/types";
import type { TimezoneMode } from "../utils/formatters";
import { formatTime, formatTimestamp } from "../utils/formatters";

// ============================================================
// Public types
// ============================================================

export interface AnalysisJump {
  timestamp: number;
  tab?: TabKey;
}

// ============================================================
// Constants
// ============================================================

type Severity = "info" | "warning" | "critical";

const SEVERITY_ICON: Record<Severity, string> = {
  critical: "\uD83D\uDD34",
  warning: "\uD83D\uDFE1",
  info: "\uD83D\uDD35",
};

const SEVERITY_LABEL: Record<Severity, string> = {
  critical: "Critical",
  warning: "Warning",
  info: "Info",
};

const SEVERITY_COLOR: Record<Severity, string> = {
  critical: "var(--status-critical)",
  warning: "var(--status-warning)",
  info: "var(--status-info, var(--accent))",
};

const CATEGORY_TAB: Record<string, TabKey> = {
  cpu: "prc",
  memory: "prc",
  disk: "prc",
  network: "prc",
  psi: "prc",
  cgroup: "prc",
  pg_activity: "pga",
  pg_statements: "pgs",
  pg_locks: "pgl",
  pg_tables: "pgt",
  pg_indexes: "pgi",
  pg_bgwriter: "pge",
  pg_errors: "pge",
};

const CATEGORY_LABEL: Record<string, string> = {
  cpu: "CPU",
  memory: "Memory",
  disk: "Disk",
  network: "Network",
  psi: "PSI",
  cgroup: "Cgroup",
  pg_activity: "PG Activity",
  pg_statements: "PG Queries",
  pg_tables: "PG Tables",
  pg_indexes: "PG Indexes",
  pg_bgwriter: "PG BGWriter",
  pg_locks: "PG Locks",
  pg_errors: "PG Errors",
};

/** Human-readable label for each rule_id. Ordered — determines lane order in timeline. */
const RULE_LABEL: Record<string, string> = {
  cpu_high: "CPU high",
  iowait_high: "IO Wait",
  steal_high: "CPU steal",
  load_average_high: "Load avg",
  memory_low: "Memory",
  swap_usage: "Swap",
  disk_util_high: "Disk util",
  disk_io_spike: "Disk I/O",
  autovacuum_impact: "Autovacuum",
  network_spike: "Network",
  cgroup_throttled: "Cgroup thr.",
  cgroup_oom_kill: "OOM kill",
  idle_in_transaction: "Idle in tx",
  long_query: "Long query",
  wait_sync_replica: "Sync repl.",
  wait_lock: "Lock wait",
  high_active_sessions: "Active sess.",
  tps_spike: "TPS spike",
  stmt_call_spike: "Query calls",
  stmt_mean_time_spike: "Query time",
  checkpoint_spike: "Checkpoint",
  backend_buffers_high: "Backend buf.",
  dead_tuples_high: "Dead tuples",
  seq_scan_dominant: "Seq scans",
  heap_read_spike: "Heap reads",
  table_write_spike: "Table writes",
  cache_hit_ratio_drop: "Cache miss",
  index_read_spike: "Idx reads",
  index_cache_miss: "Idx cache miss",
  blocked_sessions: "Blocked",
  pg_errors: "PG errors",
  pg_fatal_panic: "FATAL/PANIC",
};

/** Ordered list of rule_ids — determines lane order in timeline. */
const RULE_ORDER = Object.keys(RULE_LABEL);

// ============================================================
// Main component
// ============================================================

interface AnalysisModalProps {
  report: AnalysisReport;
  timezone: TimezoneMode;
  onClose: () => void;
  onJump: (jump: AnalysisJump) => void;
}

export function AnalysisModal({
  report,
  timezone,
  onClose,
  onJump,
}: AnalysisModalProps) {
  const [copied, setCopied] = useState(false);
  const [recsOpen, setRecsOpen] = useState(false);
  const [criticalOpen, setCriticalOpen] = useState(true);
  const [warningOpen, setWarningOpen] = useState(true);
  const [infoOpen, setInfoOpen] = useState(false);

  // Capture-phase Escape
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.stopPropagation();
        e.preventDefault();
        onClose();
      }
    }
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [onClose]);

  const criticalIncidents = useMemo(
    () => report.incidents.filter((i) => i.severity === "critical"),
    [report],
  );
  const warningIncidents = useMemo(
    () => report.incidents.filter((i) => i.severity === "warning"),
    [report],
  );
  const infoIncidents = useMemo(
    () => report.incidents.filter((i) => i.severity === "info"),
    [report],
  );

  const handleCopyMarkdown = useCallback(() => {
    const text = reportToText(report, timezone);
    copyToClipboard(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }, [report, timezone]);

  const handleJump = useCallback(
    (incident: AnalysisIncident) => {
      onJump({
        timestamp: incident.peak_ts,
        tab: CATEGORY_TAB[incident.category],
      });
      onClose();
    },
    [onJump, onClose],
  );

  return createPortal(
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={onClose}
    >
      <div
        className="relative w-[90vw] max-w-[1100px] min-w-[600px] max-h-[85vh] flex flex-col bg-[var(--bg-surface)] border border-[var(--border-default)] rounded-lg shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-[var(--border-default)]">
          <div>
            <h2 className="text-sm font-semibold text-[var(--text-primary)]">
              Hourly Analysis Report
            </h2>
            <p className="text-xs text-[var(--text-tertiary)] mt-0.5">
              {formatTimestamp(report.start_ts, timezone)} &mdash;{" "}
              {formatTime(report.end_ts, timezone)} &middot;{" "}
              {report.snapshots_analyzed} snapshots &middot;{" "}
              {report.summary.total_incidents} incidents
            </p>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={handleCopyMarkdown}
              className="flex items-center gap-1 px-2 py-1 rounded text-xs text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] cursor-pointer transition-colors"
              title="Copy report to clipboard"
            >
              {copied ? (
                <Check size={14} className="text-[var(--status-success)]" />
              ) : (
                <Copy size={14} />
              )}
              {copied ? "Copied" : "Copy"}
            </button>
            <button
              onClick={onClose}
              className="p-1 rounded text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] cursor-pointer transition-colors"
            >
              <X size={16} />
            </button>
          </div>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto px-4 py-3 space-y-3">
          {/* Summary badges */}
          <div className="flex items-center gap-2 flex-wrap">
            {report.summary.critical_count > 0 && (
              <SeverityBadge
                severity="critical"
                count={report.summary.critical_count}
              />
            )}
            {report.summary.warning_count > 0 && (
              <SeverityBadge
                severity="warning"
                count={report.summary.warning_count}
              />
            )}
            {report.summary.info_count > 0 && (
              <SeverityBadge
                severity="info"
                count={report.summary.info_count}
              />
            )}
          </div>

          {/* Incident Timeline or All-clear */}
          {report.incidents.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-6 rounded-lg border border-[var(--border-default)] bg-[var(--bg-elevated)]">
              <Check size={28} className="text-[var(--status-success)] mb-2" />
              <span className="text-sm font-medium text-[var(--status-success)]">
                All clear for this hour
              </span>
              <span className="text-xs text-[var(--text-tertiary)] mt-1">
                No incidents detected across {report.snapshots_analyzed}{" "}
                snapshots
              </span>
            </div>
          ) : (
            <IncidentTimeline
              incidents={report.incidents}
              startTs={report.start_ts}
              endTs={report.end_ts}
              timezone={timezone}
              onJump={handleJump}
            />
          )}

          {/* Recommendations */}
          {report.recommendations.length > 0 && (
            <CollapsibleSection
              title={`Recommendations (${report.recommendations.length})`}
              open={recsOpen}
              onToggle={() => setRecsOpen((o) => !o)}
            >
              <div className="space-y-1">
                {report.recommendations.map((rec, i) => (
                  <RecommendationCard key={i} rec={rec} />
                ))}
              </div>
            </CollapsibleSection>
          )}

          {/* Critical incidents */}
          {criticalIncidents.length > 0 && (
            <CollapsibleSection
              title={`Critical (${criticalIncidents.length})`}
              open={criticalOpen}
              onToggle={() => setCriticalOpen((o) => !o)}
              severity="critical"
            >
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-2">
                {criticalIncidents.map((inc, i) => (
                  <IncidentCard
                    key={i}
                    incident={inc}
                    timezone={timezone}
                    onJump={handleJump}
                  />
                ))}
              </div>
            </CollapsibleSection>
          )}

          {/* Warning incidents */}
          {warningIncidents.length > 0 && (
            <CollapsibleSection
              title={`Warning (${warningIncidents.length})`}
              open={warningOpen}
              onToggle={() => setWarningOpen((o) => !o)}
              severity="warning"
            >
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-2">
                {warningIncidents.map((inc, i) => (
                  <IncidentCard
                    key={i}
                    incident={inc}
                    timezone={timezone}
                    onJump={handleJump}
                  />
                ))}
              </div>
            </CollapsibleSection>
          )}

          {/* Info incidents */}
          {infoIncidents.length > 0 && (
            <CollapsibleSection
              title={`Info (${infoIncidents.length})`}
              open={infoOpen}
              onToggle={() => setInfoOpen((o) => !o)}
              severity="info"
            >
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-2">
                {infoIncidents.map((inc, i) => (
                  <IncidentCard
                    key={i}
                    incident={inc}
                    timezone={timezone}
                    onJump={handleJump}
                  />
                ))}
              </div>
            </CollapsibleSection>
          )}
        </div>
      </div>
    </div>,
    document.body,
  );
}

// ============================================================
// Incident Timeline — swim-lane visualization
// ============================================================

const LANE_HEIGHT = 28;
const AXIS_HEIGHT = 24;
const LABEL_WIDTH = 80;

function IncidentTimeline({
  incidents,
  startTs,
  endTs,
  timezone,
  onJump,
}: {
  incidents: AnalysisIncident[];
  startTs: number;
  endTs: number;
  timezone: TimezoneMode;
  onJump: (incident: AnalysisIncident) => void;
}) {
  const [hovered, setHovered] = useState<{
    incident: AnalysisIncident;
    x: number;
    y: number;
  } | null>(null);

  const range = endTs - startTs;

  const populatedLanes = useMemo(() => {
    const byRule = new Map<string, AnalysisIncident[]>();
    for (const inc of incidents) {
      const list = byRule.get(inc.rule_id) ?? [];
      list.push(inc);
      byRule.set(inc.rule_id, list);
    }
    // Order lanes by RULE_ORDER, then any unknown rule_ids at the end
    const ordered = [
      ...RULE_ORDER.filter((r) => byRule.has(r)),
      ...[...byRule.keys()].filter((r) => !RULE_ORDER.includes(r)),
    ];
    return ordered.map((ruleId) => ({
      label: RULE_LABEL[ruleId] ?? ruleId,
      incidents: byRule.get(ruleId)!,
    }));
  }, [incidents]);

  const timeMarkers = useMemo(() => {
    const markers: { pct: number; label: string }[] = [];
    const step = 600; // 10 minutes
    let t = Math.ceil(startTs / step) * step;
    while (t <= endTs) {
      markers.push({
        pct: ((t - startTs) / range) * 100,
        label: formatTime(t, timezone).slice(0, 5), // HH:MM
      });
      t += step;
    }
    return markers;
  }, [startTs, endTs, range, timezone]);

  if (range <= 0) return null;

  const totalHeight = AXIS_HEIGHT + populatedLanes.length * LANE_HEIGHT + 4;

  return (
    <div
      className="relative w-full rounded-lg border border-[var(--border-default)] bg-[var(--bg-elevated)] overflow-hidden select-none"
      style={{ height: totalHeight }}
    >
      {/* Time axis */}
      <div
        className="absolute top-0 right-0 border-b border-[var(--border-default)]"
        style={{ left: LABEL_WIDTH, height: AXIS_HEIGHT }}
      >
        <div className="relative w-full h-full">
          {timeMarkers.map((m, i) => (
            <span
              key={i}
              className="absolute bottom-1 font-mono text-[9px] text-[var(--text-tertiary)] -translate-x-1/2"
              style={{ left: `${m.pct}%` }}
            >
              {m.label}
            </span>
          ))}
        </div>
      </div>

      {/* Vertical grid lines */}
      {timeMarkers.map((m, i) => (
        <div
          key={i}
          className="absolute top-0 bottom-0 border-l border-[var(--border-default)] opacity-30"
          style={{
            left: `calc(${LABEL_WIDTH}px + (100% - ${LABEL_WIDTH}px) * ${m.pct / 100})`,
          }}
        />
      ))}

      {/* Swim lanes */}
      {populatedLanes.map((lane, laneIdx) => (
        <div
          key={lane.label}
          className="absolute left-0 right-0 flex"
          style={{
            top: AXIS_HEIGHT + laneIdx * LANE_HEIGHT,
            height: LANE_HEIGHT,
            backgroundColor:
              laneIdx % 2 === 1 ? "rgba(255,255,255,0.02)" : undefined,
          }}
        >
          {/* Lane label */}
          <div
            className="flex items-center justify-end pr-2 text-[9px] text-[var(--text-tertiary)] font-medium shrink-0 truncate"
            style={{ width: LABEL_WIDTH }}
          >
            {lane.label}
          </div>
          {/* Bar area */}
          <div className="relative flex-1 border-b border-[var(--border-default)] border-opacity-20">
            {lane.incidents.map((inc, i) => {
              const leftPct = ((inc.first_ts - startTs) / range) * 100;
              const widthPct = Math.max(
                ((inc.last_ts - inc.first_ts) / range) * 100,
                0.5,
              );
              return (
                <div
                  key={i}
                  className="absolute top-[5px] rounded cursor-pointer transition-opacity duration-150 hover:opacity-100"
                  style={{
                    left: `${leftPct}%`,
                    width: `${widthPct}%`,
                    minWidth: 6,
                    height: LANE_HEIGHT - 10,
                    marginRight: 1,
                    backgroundColor: SEVERITY_COLOR[inc.severity],
                    opacity: 0.7,
                  }}
                  onClick={() => onJump(inc)}
                  onMouseEnter={(e) =>
                    setHovered({
                      incident: inc,
                      x: e.clientX,
                      y: e.clientY,
                    })
                  }
                  onMouseMove={(e) =>
                    setHovered((prev) =>
                      prev ? { ...prev, x: e.clientX, y: e.clientY } : null,
                    )
                  }
                  onMouseLeave={() => setHovered(null)}
                />
              );
            })}
          </div>
        </div>
      ))}

      {/* Hover tooltip */}
      {hovered &&
        createPortal(
          <div
            className="fixed z-[9999] px-2.5 py-1.5 rounded-lg text-xs shadow-md pointer-events-none max-w-sm"
            style={{
              left: hovered.x + 14,
              top: hovered.y - 10,
              backgroundColor: "var(--bg-elevated)",
              color: "var(--text-primary)",
              border: "1px solid var(--border-default)",
            }}
          >
            <div className="font-medium">{hovered.incident.title}</div>
            <div className="text-[var(--text-tertiary)] text-[10px] mt-0.5 font-mono">
              {formatTime(hovered.incident.first_ts, timezone)} &mdash;{" "}
              {formatTime(hovered.incident.last_ts, timezone)}
            </div>
            <div className="text-[var(--accent-text)] text-[10px] mt-0.5">
              Click to jump
            </div>
          </div>,
          document.body,
        )}
    </div>
  );
}

// ============================================================
// Sub-components
// ============================================================

function SeverityBadge({
  severity,
  count,
}: {
  severity: Severity;
  count: number;
}) {
  const colors: Record<Severity, string> = {
    critical: "bg-[var(--status-critical-bg)] text-[var(--status-critical)]",
    warning: "bg-[var(--status-warning-bg)] text-[var(--status-warning)]",
    info: "bg-[var(--status-info-bg,var(--accent-bg))] text-[var(--status-info,var(--accent-text))]",
  };

  return (
    <span
      className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium ${colors[severity]}`}
    >
      {SEVERITY_ICON[severity]} {count} {SEVERITY_LABEL[severity]}
    </span>
  );
}

function CollapsibleSection({
  title,
  open,
  onToggle,
  severity,
  children,
}: {
  title: string;
  open: boolean;
  onToggle: () => void;
  severity?: Severity;
  children: React.ReactNode;
}) {
  const titleColor = severity
    ? severity === "critical"
      ? "text-[var(--status-critical)]"
      : severity === "warning"
        ? "text-[var(--status-warning)]"
        : "text-[var(--accent-text)]"
    : "text-[var(--text-primary)]";

  return (
    <div>
      <button
        onClick={onToggle}
        className="flex items-center gap-1 text-xs font-semibold cursor-pointer hover:underline"
      >
        {open ? (
          <ChevronDown size={14} className="text-[var(--text-tertiary)]" />
        ) : (
          <ChevronRight size={14} className="text-[var(--text-tertiary)]" />
        )}
        <span className={titleColor}>{title}</span>
      </button>
      {open && <div className="mt-1.5 ml-4">{children}</div>}
    </div>
  );
}

function RecommendationCard({ rec }: { rec: AnalysisRecommendation }) {
  const [expanded, setExpanded] = useState(false);
  return (
    <div
      className="px-2 py-1.5 rounded border border-[var(--border-default)] bg-[var(--bg-elevated)] cursor-pointer hover:bg-[var(--bg-hover)] transition-colors"
      onClick={() => setExpanded((v) => !v)}
    >
      <div className="flex items-center gap-1.5">
        {expanded ? (
          <ChevronDown
            size={12}
            className="text-[var(--text-tertiary)] shrink-0"
          />
        ) : (
          <ChevronRight
            size={12}
            className="text-[var(--text-tertiary)] shrink-0"
          />
        )}
        <span className="text-xs leading-none">
          {SEVERITY_ICON[rec.severity]}
        </span>
        <span className="text-xs font-semibold text-[var(--text-primary)] truncate">
          {rec.title}
        </span>
        {rec.related_incidents.length > 0 && (
          <span className="text-[9px] text-[var(--text-tertiary)] shrink-0 ml-auto">
            {rec.related_incidents.join(", ")}
          </span>
        )}
      </div>
      {expanded && (
        <p className="text-xs text-[var(--text-secondary)] mt-1.5 ml-5 whitespace-pre-wrap">
          {rec.description}
        </p>
      )}
    </div>
  );
}

function CategoryBadge({ category }: { category: string }) {
  const label = CATEGORY_LABEL[category] ?? category;
  return (
    <span className="inline-flex items-center px-1.5 py-0 rounded text-[9px] font-medium bg-[var(--bg-hover)] text-[var(--text-secondary)]">
      {label}
    </span>
  );
}

function IncidentCard({
  incident,
  timezone,
  onJump,
}: {
  incident: AnalysisIncident;
  timezone: TimezoneMode;
  onJump: (incident: AnalysisIncident) => void;
}) {
  const timeRange =
    incident.first_ts === incident.last_ts
      ? formatTime(incident.first_ts, timezone)
      : `${formatTime(incident.first_ts, timezone)} \u2014 ${formatTime(incident.last_ts, timezone)}`;

  return (
    <div
      className="flex items-start gap-1.5 p-2 rounded border border-[var(--border-default)] bg-[var(--bg-elevated)] hover:bg-[var(--bg-hover)] cursor-pointer transition-colors"
      style={{
        borderLeft: `3px solid ${SEVERITY_COLOR[incident.severity]}`,
      }}
      onClick={() => onJump(incident)}
      title={`Jump to peak at ${formatTime(incident.peak_ts, timezone)}`}
    >
      <span className="text-xs leading-none mt-0.5">
        {SEVERITY_ICON[incident.severity]}
      </span>
      <div className="min-w-0">
        <div className="text-xs font-medium text-[var(--text-primary)] truncate">
          {incident.title}
        </div>
        <div className="flex items-center gap-1.5 mt-0.5 flex-wrap">
          <CategoryBadge category={incident.category} />
          <span className="text-[10px] text-[var(--text-tertiary)] font-mono">
            {timeRange}
          </span>
          <span className="text-[10px] text-[var(--text-tertiary)]">
            ({incident.snapshot_count} snaps)
          </span>
        </div>
        {incident.detail && (
          <div className="text-[10px] text-[var(--text-secondary)] mt-0.5 truncate">
            {incident.detail}
          </div>
        )}
      </div>
    </div>
  );
}

// ============================================================
// Markdown export
// ============================================================

function severityEmoji(s: Severity): string {
  return SEVERITY_ICON[s] ?? "";
}

/** Copy text to clipboard with fallback for HTTP contexts */
async function copyToClipboard(text: string): Promise<void> {
  if (navigator.clipboard) {
    try {
      await navigator.clipboard.writeText(text);
      return;
    } catch {
      // fallback below
    }
  }
  // Fallback: textarea + execCommand
  const ta = document.createElement("textarea");
  ta.value = text;
  ta.style.position = "fixed";
  ta.style.left = "-9999px";
  document.body.appendChild(ta);
  ta.select();
  document.execCommand("copy");
  document.body.removeChild(ta);
}

/** Messenger-friendly plain text report (Telegram, Slack, etc.) */
function reportToText(report: AnalysisReport, tz: TimezoneMode): string {
  const lines: string[] = [];
  lines.push(
    `rpglot: ${formatTimestamp(report.start_ts, tz)} \u2014 ${formatTime(report.end_ts, tz)}`,
  );

  const counts: string[] = [];
  if (report.summary.critical_count > 0)
    counts.push(`${report.summary.critical_count} critical`);
  if (report.summary.warning_count > 0)
    counts.push(`${report.summary.warning_count} warning`);
  if (report.summary.info_count > 0)
    counts.push(`${report.summary.info_count} info`);
  if (counts.length > 0) lines.push(counts.join(", "));
  lines.push("");

  // Group incidents by severity
  const bySeverity: [Severity, AnalysisIncident[]][] = [
    ["critical", report.incidents.filter((i) => i.severity === "critical")],
    ["warning", report.incidents.filter((i) => i.severity === "warning")],
    ["info", report.incidents.filter((i) => i.severity === "info")],
  ];

  for (const [, incidents] of bySeverity) {
    if (incidents.length === 0) continue;
    for (const inc of incidents) {
      const time =
        inc.first_ts === inc.last_ts
          ? formatTime(inc.first_ts, tz)
          : `${formatTime(inc.first_ts, tz)}\u2014${formatTime(inc.last_ts, tz)}`;
      lines.push(`${severityEmoji(inc.severity)} ${inc.title}`);
      lines.push(`  ${time} (${inc.snapshot_count} snaps)`);
      if (inc.detail) lines.push(`  ${inc.detail}`);
    }
    lines.push("");
  }

  if (report.recommendations.length > 0) {
    lines.push("Recommendations:");
    for (const r of report.recommendations) {
      lines.push(`${severityEmoji(r.severity)} ${r.title}`);
      lines.push(`  ${r.description}`);
    }
    lines.push("");
  }

  if (report.incidents.length === 0 && report.recommendations.length === 0) {
    lines.push("No incidents \u2014 everything looks healthy.");
  }

  return lines.join("\n").trimEnd();
}
