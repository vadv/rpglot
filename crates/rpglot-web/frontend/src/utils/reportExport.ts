import type { AnalysisReport, IncidentGroup } from "../api/types";
import type { TimezoneMode } from "./formatters";
import { formatTime, formatTimestamp } from "./formatters";
import { type Severity, SEVERITY_ICON } from "../components/analysis/constants";

function severityEmoji(s: Severity): string {
  return SEVERITY_ICON[s] ?? "";
}

/** Copy text to clipboard with fallback for HTTP contexts */
export async function copyToClipboard(text: string): Promise<void> {
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
export function reportToText(report: AnalysisReport, tz: TimezoneMode): string {
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

  const groups = report.groups ?? [];
  const persistentGroups = groups.filter((g) => g.persistent);
  const transientGroups = groups.filter((g) => !g.persistent);

  // Persistent issues
  if (persistentGroups.length > 0) {
    lines.push("Persistent issues:");
    for (const g of persistentGroups) {
      for (const inc of g.incidents) {
        const time =
          inc.first_ts === inc.last_ts
            ? formatTime(inc.first_ts, tz)
            : `${formatTime(inc.first_ts, tz)}\u2014${formatTime(inc.last_ts, tz)}`;
        lines.push(`  \u25CB ${inc.title}  ${time}`);
      }
    }
    lines.push("");
  }

  // Transient groups by severity
  const bySeverity: [Severity, IncidentGroup[]][] = [
    ["critical", transientGroups.filter((g) => g.severity === "critical")],
    ["warning", transientGroups.filter((g) => g.severity === "warning")],
    ["info", transientGroups.filter((g) => g.severity === "info")],
  ];

  for (const [, sGroups] of bySeverity) {
    if (sGroups.length === 0) continue;
    for (const g of sGroups) {
      if (g.incidents.length > 1) {
        const gTime = `${formatTime(g.first_ts, tz)}\u2014${formatTime(g.last_ts, tz)}`;
        lines.push(
          `${severityEmoji(g.severity)} ${g.incidents.length} correlated incidents  ${gTime}`,
        );
        for (const inc of g.incidents) {
          lines.push(`  ${severityEmoji(inc.severity)} ${inc.title}`);
          if (inc.detail) lines.push(`    ${inc.detail}`);
        }
      } else {
        const inc = g.incidents[0];
        const time =
          inc.first_ts === inc.last_ts
            ? formatTime(inc.first_ts, tz)
            : `${formatTime(inc.first_ts, tz)}\u2014${formatTime(inc.last_ts, tz)}`;
        lines.push(`${severityEmoji(inc.severity)} ${inc.title}`);
        lines.push(`  ${time} (${inc.snapshot_count} snaps)`);
        if (inc.detail) lines.push(`  ${inc.detail}`);
      }
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
