import { useState, useEffect, useCallback, useMemo } from "react";
import { createPortal } from "react-dom";
import { X, Copy, Check } from "lucide-react";
import type { AnalysisReport, AnalysisIncident } from "../api/types";
import type { TimezoneMode } from "../utils/formatters";
import { formatTime, formatTimestamp } from "../utils/formatters";
import { reportToText, copyToClipboard } from "../utils/reportExport";
import {
  type AnalysisJump,
  RULE_TARGET,
  CATEGORY_TAB,
  RULE_COLUMN_FILTER,
} from "./analysis/constants";
import { IncidentTimeline } from "./analysis/IncidentTimeline";
import {
  SeverityBadge,
  CollapsibleSection,
  RecommendationCard,
  PersistentSection,
  GroupCard,
} from "./analysis/SubComponents";

export type { AnalysisJump } from "./analysis/constants";

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

  // Split groups into persistent vs transient, then by severity
  const groups = report.groups ?? [];
  const persistentGroups = useMemo(
    () => groups.filter((g) => g.persistent),
    [groups],
  );
  const transientGroups = useMemo(
    () => groups.filter((g) => !g.persistent),
    [groups],
  );
  const criticalGroups = useMemo(
    () => transientGroups.filter((g) => g.severity === "critical"),
    [transientGroups],
  );
  const warningGroups = useMemo(
    () => transientGroups.filter((g) => g.severity === "warning"),
    [transientGroups],
  );
  const infoGroups = useMemo(
    () => transientGroups.filter((g) => g.severity === "info"),
    [transientGroups],
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
      const target = RULE_TARGET[incident.rule_id];
      const tab = target?.tab ?? CATEGORY_TAB[incident.category];

      // Determine filter strategy
      let filter: string | undefined;
      let columnFilter: { column: string; value: string } | undefined;

      // Group A-1: PGT — table name from title (matches display_name column)
      const tableMatch = incident.title.match(/^Table\s+([^:\s]+)/);
      // Group A-2: PGI — index name from title (matches index column, without schema prefix)
      const indexMatch = incident.title.match(/^Index\s+([^:\s]+)/);
      if (tableMatch) {
        filter = tableMatch[1];
      } else if (indexMatch) {
        // PGI has no display_index column; the "index" column stores just the name
        // without schema prefix. Title has "schema.indexname" — strip schema.
        const full = indexMatch[1];
        const dot = full.indexOf(".");
        filter = dot >= 0 ? full.substring(dot + 1) : full;
      }
      // Group B: PGS/PGA/PRC — entity_id as string (queryid, PID)
      else if (incident.entity_id != null) {
        filter = String(incident.entity_id);
      }
      // Group C: PGA aggregate — column filter
      else {
        columnFilter = RULE_COLUMN_FILTER[incident.rule_id];
      }

      onJump({
        timestamp: incident.peak_ts,
        tab,
        view: target?.view,
        entityId: incident.entity_id ?? undefined,
        filter,
        columnFilter,
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

          {/* Incident Timeline (with health lane) or All-clear */}
          {report.incidents.length === 0 &&
          report.health_scores.length === 0 ? (
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
              groups={groups}
              healthScores={report.health_scores}
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

          {/* Persistent incidents */}
          {persistentGroups.length > 0 && (
            <PersistentSection
              groups={persistentGroups}
              timezone={timezone}
              onJump={handleJump}
            />
          )}

          {/* Critical groups */}
          {criticalGroups.length > 0 && (
            <CollapsibleSection
              title={`Critical (${criticalGroups.reduce((n, g) => n + g.incidents.length, 0)})`}
              open={criticalOpen}
              onToggle={() => setCriticalOpen((o) => !o)}
              severity="critical"
            >
              <div className="space-y-2">
                {criticalGroups.map((g) => (
                  <GroupCard
                    key={g.id}
                    group={g}
                    timezone={timezone}
                    onJump={handleJump}
                  />
                ))}
              </div>
            </CollapsibleSection>
          )}

          {/* Warning groups */}
          {warningGroups.length > 0 && (
            <CollapsibleSection
              title={`Warning (${warningGroups.reduce((n, g) => n + g.incidents.length, 0)})`}
              open={warningOpen}
              onToggle={() => setWarningOpen((o) => !o)}
              severity="warning"
            >
              <div className="space-y-2">
                {warningGroups.map((g) => (
                  <GroupCard
                    key={g.id}
                    group={g}
                    timezone={timezone}
                    onJump={handleJump}
                  />
                ))}
              </div>
            </CollapsibleSection>
          )}

          {/* Info groups */}
          {infoGroups.length > 0 && (
            <CollapsibleSection
              title={`Info (${infoGroups.reduce((n, g) => n + g.incidents.length, 0)})`}
              open={infoOpen}
              onToggle={() => setInfoOpen((o) => !o)}
              severity="info"
            >
              <div className="space-y-2">
                {infoGroups.map((g) => (
                  <GroupCard
                    key={g.id}
                    group={g}
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
