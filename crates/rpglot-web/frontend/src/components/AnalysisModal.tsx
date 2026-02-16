import { useState, useEffect, useCallback, useMemo } from "react";
import { createPortal } from "react-dom";
import { X, ChevronDown, ChevronRight, Copy, Check } from "lucide-react";
import type {
  AnalysisReport,
  AnalysisIncident,
  AnalysisRecommendation,
} from "../api/types";
import type { TimezoneMode } from "../utils/formatters";
import { formatTime, formatTimestamp } from "../utils/formatters";

interface AnalysisModalProps {
  report: AnalysisReport;
  timezone: TimezoneMode;
  onClose: () => void;
  onTimestampJump: (ts: number) => void;
}

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

export function AnalysisModal({
  report,
  timezone,
  onClose,
  onTimestampJump,
}: AnalysisModalProps) {
  const [copied, setCopied] = useState(false);
  const [recsOpen, setRecsOpen] = useState(true);
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
    const md = reportToMarkdown(report, timezone);
    navigator.clipboard.writeText(md).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }, [report, timezone]);

  const handleJump = useCallback(
    (ts: number) => {
      onTimestampJump(ts);
      onClose();
    },
    [onTimestampJump, onClose],
  );

  return createPortal(
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={onClose}
    >
      <div
        className="relative w-[640px] max-h-[80vh] flex flex-col bg-[var(--bg-surface)] border border-[var(--border-default)] rounded-lg shadow-xl"
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
              className="flex items-center gap-1 px-2 py-1 rounded text-xs text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors"
              title="Copy report as Markdown"
            >
              {copied ? (
                <Check size={14} className="text-[var(--status-success)]" />
              ) : (
                <Copy size={14} />
              )}
              {copied ? "Copied" : "Copy MD"}
            </button>
            <button
              onClick={onClose}
              className="p-1 rounded text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors"
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
            {report.summary.total_incidents === 0 && (
              <span className="text-xs text-[var(--status-success)] font-medium">
                No incidents detected — everything looks healthy
              </span>
            )}
          </div>

          {/* Recommendations */}
          {report.recommendations.length > 0 && (
            <CollapsibleSection
              title={`Recommendations (${report.recommendations.length})`}
              open={recsOpen}
              onToggle={() => setRecsOpen((o) => !o)}
            >
              <div className="space-y-2">
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
              <div className="space-y-1.5">
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
              <div className="space-y-1.5">
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
              <div className="space-y-1.5">
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
    critical:
      "bg-[var(--status-critical-bg)] text-[var(--status-critical)]",
    warning:
      "bg-[var(--status-warning-bg)] text-[var(--status-warning)]",
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
        className="flex items-center gap-1 text-xs font-semibold hover:underline"
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
  return (
    <div className="p-2.5 rounded border border-[var(--border-default)] bg-[var(--bg-elevated)]">
      <div className="flex items-start gap-1.5">
        <span className="text-sm leading-none mt-0.5">
          {SEVERITY_ICON[rec.severity]}
        </span>
        <div className="flex-1 min-w-0">
          <div className="text-xs font-semibold text-[var(--text-primary)]">
            {rec.title}
          </div>
          <p className="text-xs text-[var(--text-secondary)] mt-1 whitespace-pre-wrap">
            {rec.description}
          </p>
          {rec.related_incidents.length > 0 && (
            <div className="mt-1 text-[10px] text-[var(--text-tertiary)]">
              Related: {rec.related_incidents.join(", ")}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function IncidentCard({
  incident,
  timezone,
  onJump,
}: {
  incident: AnalysisIncident;
  timezone: TimezoneMode;
  onJump: (ts: number) => void;
}) {
  const timeRange =
    incident.first_ts === incident.last_ts
      ? formatTime(incident.first_ts, timezone)
      : `${formatTime(incident.first_ts, timezone)} \u2014 ${formatTime(incident.last_ts, timezone)}`;

  return (
    <div className="flex items-start justify-between gap-2 p-2 rounded border border-[var(--border-default)] bg-[var(--bg-elevated)]">
      <div className="flex items-start gap-1.5 min-w-0">
        <span className="text-xs leading-none mt-0.5">
          {SEVERITY_ICON[incident.severity]}
        </span>
        <div className="min-w-0">
          <div className="text-xs font-medium text-[var(--text-primary)] truncate">
            {incident.title}
          </div>
          <div className="text-[10px] text-[var(--text-tertiary)] mt-0.5">
            {timeRange} ({incident.snapshot_count} snapshots)
          </div>
          {incident.detail && (
            <div className="text-[10px] text-[var(--text-secondary)] mt-0.5">
              {incident.detail}
            </div>
          )}
        </div>
      </div>
      <button
        onClick={() => onJump(incident.peak_ts)}
        className="shrink-0 text-[10px] px-1.5 py-0.5 rounded text-[var(--accent-text)] hover:bg-[var(--accent-bg)] transition-colors whitespace-nowrap"
        title={`Jump to peak at ${formatTime(incident.peak_ts, timezone)}`}
      >
        &rarr; Jump
      </button>
    </div>
  );
}

// ============================================================
// Markdown export
// ============================================================

function severityEmoji(s: Severity): string {
  return SEVERITY_ICON[s] ?? "";
}

function reportToMarkdown(report: AnalysisReport, tz: TimezoneMode): string {
  let md = `# Hourly Analysis Report\n\n`;
  md += `**Period:** ${formatTimestamp(report.start_ts, tz)} — ${formatTime(report.end_ts, tz)}\n`;
  md += `**Snapshots:** ${report.snapshots_analyzed}\n`;
  md += `**Incidents:** ${report.summary.critical_count} critical, ${report.summary.warning_count} warning, ${report.summary.info_count} info\n\n`;

  if (report.recommendations.length > 0) {
    md += `## Recommendations\n\n`;
    for (const r of report.recommendations) {
      md += `### ${severityEmoji(r.severity)} ${r.title}\n\n`;
      md += `${r.description}\n\n`;
    }
  }

  if (report.incidents.length > 0) {
    md += `## Incidents\n\n`;
    for (const i of report.incidents) {
      md += `- **${severityEmoji(i.severity)} ${i.title}**\n`;
      md += `  ${formatTime(i.first_ts, tz)} — ${formatTime(i.last_ts, tz)} (${i.snapshot_count} snapshots, peak: ${i.peak_value.toFixed(1)})\n`;
      if (i.detail) md += `  ${i.detail}\n`;
      md += `\n`;
    }
  }

  if (
    report.incidents.length === 0 &&
    report.recommendations.length === 0
  ) {
    md += `No incidents detected — everything looks healthy.\n`;
  }

  return md;
}
