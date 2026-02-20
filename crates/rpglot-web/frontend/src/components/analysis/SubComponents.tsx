import { useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import type {
  AnalysisIncident,
  AnalysisRecommendation,
  IncidentGroup,
} from "../../api/types";
import type { TimezoneMode } from "../../utils/formatters";
import { formatTime } from "../../utils/formatters";
import {
  type Severity,
  SEVERITY_ICON,
  SEVERITY_LABEL,
  SEVERITY_COLOR,
  PERSISTENT_COLOR,
  CATEGORY_LABEL,
  RULE_LABEL,
} from "./constants";

export function SeverityBadge({
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

export function CollapsibleSection({
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

export function RecommendationCard({ rec }: { rec: AnalysisRecommendation }) {
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

export function PersistentSection({
  groups,
  timezone,
  onJump,
}: {
  groups: IncidentGroup[];
  timezone: TimezoneMode;
  onJump: (incident: AnalysisIncident) => void;
}) {
  return (
    <div>
      <div
        className="flex items-center gap-1 text-xs font-semibold"
        style={{ color: PERSISTENT_COLOR }}
      >
        Persistent issues ({groups.length})
      </div>
      <div className="mt-1.5 ml-4 space-y-1">
        {groups.map((g) =>
          g.incidents.length === 1 ? (
            <PersistentRow
              key={g.id}
              incident={g.incidents[0]}
              timezone={timezone}
              onJump={onJump}
            />
          ) : (
            <PersistentGroupRow
              key={g.id}
              group={g}
              timezone={timezone}
              onJump={onJump}
            />
          ),
        )}
      </div>
    </div>
  );
}

function PersistentRow({
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
      className="flex items-center gap-2 px-2 py-1 rounded border border-[var(--border-default)] bg-[var(--bg-elevated)] hover:bg-[var(--bg-hover)] cursor-pointer transition-colors"
      style={{ borderLeft: `3px solid ${PERSISTENT_COLOR}` }}
      onClick={() => onJump(incident)}
      title={`Jump to peak at ${formatTime(incident.peak_ts, timezone)}`}
    >
      <span
        className="w-2 h-2 rounded-full shrink-0"
        style={{ backgroundColor: PERSISTENT_COLOR }}
      />
      <span className="text-xs font-medium text-[var(--text-primary)] truncate flex-1">
        {incident.title}
      </span>
      <span className="text-[10px] text-[var(--text-tertiary)] font-mono shrink-0">
        {timeRange}
      </span>
    </div>
  );
}

function PersistentGroupRow({
  group,
  timezone,
  onJump,
}: {
  group: IncidentGroup;
  timezone: TimezoneMode;
  onJump: (incident: AnalysisIncident) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const ruleLabel =
    RULE_LABEL[group.incidents[0]?.rule_id] ?? group.incidents[0]?.rule_id;
  const timeRange = `${formatTime(group.first_ts, timezone)} \u2014 ${formatTime(group.last_ts, timezone)}`;

  return (
    <div
      className="rounded border border-[var(--border-default)] bg-[var(--bg-elevated)] overflow-hidden"
      style={{ borderLeft: `3px solid ${PERSISTENT_COLOR}` }}
    >
      <div
        className="flex items-center gap-2 px-2 py-1 cursor-pointer hover:bg-[var(--bg-hover)] transition-colors"
        onClick={() => setExpanded((v) => !v)}
      >
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
        <span
          className="w-2 h-2 rounded-full shrink-0"
          style={{ backgroundColor: PERSISTENT_COLOR }}
        />
        <span className="text-xs font-medium text-[var(--text-primary)] truncate flex-1">
          {ruleLabel}: {group.incidents.length} affected
        </span>
        <span className="text-[10px] text-[var(--text-tertiary)] font-mono shrink-0">
          {timeRange}
        </span>
      </div>
      {expanded && (
        <div className="px-2 pb-1.5 space-y-1">
          {group.incidents.map((inc, i) => (
            <PersistentRow
              key={i}
              incident={inc}
              timezone={timezone}
              onJump={onJump}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export function GroupCard({
  group,
  timezone,
  onJump,
}: {
  group: IncidentGroup;
  timezone: TimezoneMode;
  onJump: (incident: AnalysisIncident) => void;
}) {
  const [expanded, setExpanded] = useState(false);

  // Single-incident group — render plain IncidentCard
  if (group.incidents.length === 1) {
    return (
      <IncidentCard
        incident={group.incidents[0]}
        timezone={timezone}
        onJump={onJump}
      />
    );
  }

  // Multi-incident group — collapsible wrapper
  const timeRange = `${formatTime(group.first_ts, timezone)} \u2014 ${formatTime(group.last_ts, timezone)}`;

  return (
    <div
      className="rounded border border-[var(--border-default)] bg-[var(--bg-elevated)] overflow-hidden"
      style={{ borderLeft: `3px solid ${SEVERITY_COLOR[group.severity]}` }}
    >
      <div
        className="flex items-center gap-1.5 px-2 py-1.5 cursor-pointer hover:bg-[var(--bg-hover)] transition-colors"
        onClick={() => setExpanded((v) => !v)}
      >
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
          {SEVERITY_ICON[group.severity]}
        </span>
        <span className="text-xs font-semibold text-[var(--text-primary)]">
          {group.incidents.length} correlated incidents
        </span>
        <span className="text-[10px] text-[var(--text-tertiary)] font-mono ml-auto shrink-0">
          {timeRange}
        </span>
      </div>
      {expanded && (
        <div className="px-2 pb-2 space-y-1.5">
          {group.incidents.map((inc, i) => (
            <IncidentCard
              key={i}
              incident={inc}
              timezone={timezone}
              onJump={onJump}
            />
          ))}
        </div>
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
