import { useState, useMemo } from "react";
import { createPortal } from "react-dom";
import type { AnalysisIncident, IncidentGroup, HealthPoint } from "../../api/types";
import type { TimezoneMode } from "../../utils/formatters";
import { formatTime } from "../../utils/formatters";
import {
  SEVERITY_COLOR,
  PERSISTENT_COLOR,
  RULE_LABEL,
  RULE_ORDER,
} from "./constants";

const LANE_HEIGHT = 28;
const AXIS_HEIGHT = 24;
const LABEL_WIDTH = 80;
const HEALTH_LANE_HEIGHT = 24;

export function IncidentTimeline({
  incidents,
  groups,
  healthScores,
  startTs,
  endTs,
  timezone,
  onJump,
}: {
  incidents: AnalysisIncident[];
  groups: IncidentGroup[];
  healthScores: HealthPoint[];
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
  const [hoveredGroupId, setHoveredGroupId] = useState<number | null>(null);

  const range = endTs - startTs;

  // Build incident → group lookup
  const incidentGroupMap = useMemo(() => {
    const m = new Map<string, number>(); // "rule_id:first_ts:last_ts" → group_id
    for (const g of groups) {
      for (const inc of g.incidents) {
        m.set(`${inc.rule_id}:${inc.first_ts}:${inc.last_ts}`, g.id);
      }
    }
    return m;
  }, [groups]);

  // Build persistent incident set
  const persistentSet = useMemo(() => {
    const s = new Set<string>();
    for (const g of groups) {
      if (!g.persistent) continue;
      for (const inc of g.incidents) {
        s.add(`${inc.rule_id}:${inc.first_ts}:${inc.last_ts}`);
      }
    }
    return s;
  }, [groups]);

  // Non-persistent multi-incident groups for vertical stripes
  const stripeGroups = useMemo(
    () => groups.filter((g) => !g.persistent && g.incidents.length >= 2),
    [groups],
  );

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

  const hasHealth = healthScores.length >= 2;
  const healthOffset = hasHealth ? HEALTH_LANE_HEIGHT : 0;
  const totalHeight =
    AXIS_HEIGHT + healthOffset + populatedLanes.length * LANE_HEIGHT + 4;

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

      {/* Health lane */}
      {hasHealth && (
        <div
          className="absolute left-0 right-0 flex"
          style={{
            top: AXIS_HEIGHT,
            height: HEALTH_LANE_HEIGHT,
            borderBottom: "1px solid var(--border-default)",
          }}
        >
          <div
            className="flex items-center justify-end pr-2 text-[9px] text-[var(--text-tertiary)] font-medium shrink-0 truncate"
            style={{ width: LABEL_WIDTH }}
          >
            Health
          </div>
          <div className="relative flex-1">
            {healthScores.map((pt, i) => {
              if (i === healthScores.length - 1) return null;
              const next = healthScores[i + 1];
              const leftPct = ((pt.ts - startTs) / range) * 100;
              const widthPct = Math.max(
                ((next.ts - pt.ts) / range) * 100,
                0.15,
              );
              const score = pt.score;
              const color =
                score >= 80
                  ? "var(--status-success)"
                  : score >= 50
                    ? "var(--status-warning)"
                    : "var(--status-critical)";
              const opacity = ((100 - score) / 100) * 0.8 + 0.2;
              return (
                <div
                  key={i}
                  className="absolute top-[3px]"
                  style={{
                    left: `${leftPct}%`,
                    width: `${widthPct}%`,
                    height: HEALTH_LANE_HEIGHT - 6,
                    backgroundColor: color,
                    opacity,
                  }}
                />
              );
            })}
          </div>
        </div>
      )}

      {/* Vertical stripes behind correlated groups (2+ incidents, not persistent) */}
      {stripeGroups.map((g) => {
        const leftPct = ((g.first_ts - startTs) / range) * 100;
        const widthPct = Math.max(
          ((g.last_ts - g.first_ts) / range) * 100,
          0.3,
        );
        const isHighlighted = hoveredGroupId === g.id;
        return (
          <div
            key={`stripe-${g.id}`}
            className="absolute pointer-events-none"
            style={{
              left: `calc(${LABEL_WIDTH}px + (100% - ${LABEL_WIDTH}px) * ${leftPct / 100})`,
              width: `calc((100% - ${LABEL_WIDTH}px) * ${widthPct / 100})`,
              top: AXIS_HEIGHT + healthOffset,
              bottom: 0,
              backgroundColor: isHighlighted
                ? "rgba(255,255,255,0.08)"
                : "rgba(255,255,255,0.03)",
              transition: "background-color 0.15s",
            }}
          />
        );
      })}

      {/* Swim lanes */}
      {populatedLanes.map((lane, laneIdx) => (
        <div
          key={lane.label}
          className="absolute left-0 right-0 flex"
          style={{
            top: AXIS_HEIGHT + healthOffset + laneIdx * LANE_HEIGHT,
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
              const incKey = `${inc.rule_id}:${inc.first_ts}:${inc.last_ts}`;
              const isPersistent = persistentSet.has(incKey);
              const groupId = incidentGroupMap.get(incKey);
              const isGroupHighlighted =
                hoveredGroupId != null && groupId === hoveredGroupId;

              const barColor = isPersistent
                ? PERSISTENT_COLOR
                : SEVERITY_COLOR[inc.severity];
              const barOpacity = isPersistent
                ? 0.35
                : isGroupHighlighted
                  ? 1.0
                  : 0.7;

              return (
                <div
                  key={i}
                  className="absolute top-[5px] rounded cursor-pointer transition-opacity duration-150"
                  style={{
                    left: `${leftPct}%`,
                    width: `${widthPct}%`,
                    minWidth: 6,
                    height: LANE_HEIGHT - 10,
                    marginRight: 1,
                    backgroundColor: barColor,
                    opacity: barOpacity,
                    outline: isGroupHighlighted
                      ? "1px solid rgba(255,255,255,0.5)"
                      : "none",
                  }}
                  onClick={() => onJump(inc)}
                  onMouseEnter={(e) => {
                    setHovered({
                      incident: inc,
                      x: e.clientX,
                      y: e.clientY,
                    });
                    if (groupId != null) setHoveredGroupId(groupId);
                  }}
                  onMouseMove={(e) =>
                    setHovered((prev) =>
                      prev ? { ...prev, x: e.clientX, y: e.clientY } : null,
                    )
                  }
                  onMouseLeave={() => {
                    setHovered(null);
                    setHoveredGroupId(null);
                  }}
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
            {hovered.incident.detail && (
              <div className="text-[var(--text-secondary)] text-[10px] mt-0.5 truncate max-w-[300px]">
                {hovered.incident.detail}
              </div>
            )}
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
