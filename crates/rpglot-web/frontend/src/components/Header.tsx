import { useState, useEffect, useCallback, useRef } from "react";
import {
  Database,
  Radio,
  History,
  Pause,
  Play,
  Sun,
  Moon,
  Monitor,
  HelpCircle,
  HeartPulse,
  Zap,
  FileText,
  Users,
  Server,
  HardDrive,
  Activity,
  GitBranch,
  ArrowDown,
  ArrowUp,
  ExternalLink,
} from "lucide-react";
import {
  formatTimestamp,
  formatDate,
  getDatePartsInTz,
  dateToEpochInTz,
} from "../utils/formatters";
import type { TimezoneMode } from "../utils/formatters";
import { CalendarPopover, TimeInput } from "./Timeline";
import { RichTooltip } from "./RichTooltip";
import { healthColor, healthBgColor } from "../utils/healthScore";
import type {
  ApiSnapshot,
  InstanceInfo,
  TimelineInfo,
  DateInfo,
} from "../api/types";

export interface ThemeHook {
  theme: "light" | "dark" | "system";
  effective: "light" | "dark";
  cycle: () => void;
}

export interface TimezoneHookType {
  timezone: TimezoneMode;
  cycle: () => void;
}

const TZ_DISPLAY: Record<TimezoneMode, string> = {
  local: "LOCAL",
  utc: "UTC",
  moscow: "MSK",
};

/** Derive the largest database name from PGT rows (by total size_bytes). */
export function deriveLargestDb(
  snapshot: ApiSnapshot | null | undefined,
): string {
  if (!snapshot?.pgt?.length) return "";
  const sizeByDb = new Map<string, number>();
  for (const row of snapshot.pgt) {
    sizeByDb.set(
      row.database,
      (sizeByDb.get(row.database) ?? 0) + row.size_bytes,
    );
  }
  let best = "";
  let bestSize = -1;
  for (const [db, sz] of sizeByDb) {
    if (sz > bestSize) {
      best = db;
      bestSize = sz;
    }
  }
  return best;
}

export function Header({
  mode,
  timestamp,
  loading,
  paused,
  onTogglePause,
  themeHook,
  timezoneHook,
  timeline,
  onTimestampJump,
  onHelpOpen,
  snapshot,
  currentHour,
  version,
  instance,
  analyzing,
  analyzeStartedAt,
  onAnalyze,
  hasReport,
  onShowReport,
  authUsername,
}: {
  mode: string;
  timestamp?: number;
  loading?: boolean;
  paused?: boolean;
  onTogglePause?: () => void;
  themeHook: ThemeHook;
  timezoneHook: TimezoneHookType;
  timeline?: TimelineInfo;
  onTimestampJump?: (ts: number, direction?: "floor" | "ceil") => void;
  onHelpOpen?: () => void;
  snapshot?: ApiSnapshot | null;
  currentHour?: number;
  version?: string;
  instance?: InstanceInfo;
  analyzing?: boolean;
  analyzeStartedAt?: number | null;
  onAnalyze?: () => void;
  hasReport?: boolean;
  onShowReport?: () => void;
  authUsername?: string | null;
}) {
  const [calendarOpen, setCalendarOpen] = useState(false);
  const [anchorRect, setAnchorRect] = useState<DOMRect | null>(null);
  const dateButtonRef = useRef<HTMLButtonElement>(null);
  const [analyzeElapsed, setAnalyzeElapsed] = useState(0);

  useEffect(() => {
    if (!analyzeStartedAt) {
      setAnalyzeElapsed(0);
      return;
    }
    setAnalyzeElapsed(Math.floor((Date.now() - analyzeStartedAt) / 1000));
    const id = setInterval(() => {
      setAnalyzeElapsed(Math.floor((Date.now() - analyzeStartedAt) / 1000));
    }, 1000);
    return () => clearInterval(id);
  }, [analyzeStartedAt]);

  const ts = timestamp ?? 0;
  const tz = timezoneHook.timezone;
  const dates = timeline?.dates;
  const currentDateStr = ts > 0 ? formatDate(ts, tz) : "-";

  const toggleCalendar = useCallback(() => {
    if (!calendarOpen && dateButtonRef.current) {
      setAnchorRect(dateButtonRef.current.getBoundingClientRect());
    }
    setCalendarOpen((prev) => !prev);
  }, [calendarOpen]);

  const handleSelectDate = useCallback(
    (dateInfo: DateInfo) => {
      onTimestampJump?.(dateInfo.first_timestamp);
      setCalendarOpen(false);
    },
    [onTimestampJump],
  );

  const handleTimeSubmit = useCallback(
    (epoch: number) => {
      onTimestampJump?.(epoch);
    },
    [onTimestampJump],
  );

  const handleHourChange = useCallback(
    (hour: number) => {
      if (ts <= 0 || !onTimestampJump) return;
      const parts = getDatePartsInTz(ts, tz);
      const epoch = dateToEpochInTz(
        parts.year,
        parts.month,
        parts.day,
        hour,
        0,
        0,
        tz,
      );
      onTimestampJump(epoch, "ceil");
    },
    [ts, tz, onTimestampJump],
  );

  const ThemeIcon =
    themeHook.theme === "light"
      ? Sun
      : themeHook.theme === "dark"
        ? Moon
        : Monitor;

  const isHistory = mode === "history";

  return (
    <div className="flex items-center justify-between px-4 py-2 bg-[var(--bg-surface)] border-b border-[var(--border-default)]">
      <div className="flex items-center gap-3">
        <div className="flex items-center gap-1.5">
          <Database size={16} className="text-[var(--accent-text)]" />
          <span className="text-sm font-semibold text-[var(--text-primary)]">
            rpglot
          </span>
          {version && (
            <span className="text-[10px] font-mono text-[var(--text-tertiary)]">
              {version}
            </span>
          )}
        </div>
        {(() => {
          const dbName = instance?.database ?? deriveLargestDb(snapshot);
          const pgVer = instance?.pg_version;
          if (!dbName) return null;
          return (
            <span
              className="flex items-center gap-1 text-xs px-2 py-0.5 rounded-full font-medium bg-[var(--bg-elevated)] text-[var(--text-secondary)] border border-[var(--border-default)]"
              title={
                pgVer
                  ? `Database: ${dbName}\nPostgreSQL ${pgVer}`
                  : `Database: ${dbName}`
              }
            >
              <Server size={10} />
              {dbName}
              {pgVer ? <>&nbsp;&middot; PG {pgVer}</> : null}
            </span>
          );
        })()}
        <span
          className={`flex items-center gap-1 text-xs px-2 py-0.5 rounded-full font-medium ${
            mode === "live"
              ? "bg-[var(--status-success-bg)] text-[var(--status-success)]"
              : "bg-[var(--status-warning-bg)] text-[var(--status-warning)]"
          }`}
        >
          {mode === "live" ? <Radio size={10} /> : <History size={10} />}
          {mode}
        </span>
        {mode === "live" && onTogglePause && (
          <button
            onClick={onTogglePause}
            className={`flex items-center gap-1 text-xs px-2 py-0.5 rounded transition-colors ${
              paused
                ? "bg-[var(--status-warning-bg)] text-[var(--status-warning)]"
                : "bg-[var(--bg-elevated)] text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
            }`}
          >
            {paused ? <Play size={12} /> : <Pause size={12} />}
            {paused ? "resume" : "pause"}
          </button>
        )}
        {snapshot && <SessionBadge snapshot={snapshot} />}
        {snapshot && <ReplicationBadge snapshot={snapshot} />}
        {snapshot && (
          <div className="flex items-center gap-1">
            <HealthBadge snapshot={snapshot} />
            {onAnalyze && (
              <button
                onClick={onAnalyze}
                disabled={analyzing}
                className={`flex items-center gap-1.5 text-xs px-2.5 py-0.5 rounded-full font-semibold transition-all ${
                  analyzing
                    ? "bg-[var(--accent)] text-white animate-pulse-btn"
                    : "bg-[var(--accent)] text-white hover:brightness-110 shadow-sm"
                }`}
                title="Analyze current hour for anomalies and recommendations"
              >
                <Zap size={11} />
                {analyzing ? `${analyzeElapsed}s\u2026` : "Analyze"}
              </button>
            )}
          </div>
        )}
        {hasReport && !analyzing && onShowReport && (
          <button
            onClick={onShowReport}
            className="flex items-center gap-1 text-xs px-2 py-0.5 rounded-full font-medium bg-[var(--bg-elevated)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors border border-[var(--border-default)]"
            title="Show analysis report"
          >
            <FileText size={10} />
            Report
          </button>
        )}
      </div>
      <div className="flex items-center gap-3">
        {loading && (
          <span className="text-xs text-[var(--status-warning)]">
            loading...
          </span>
        )}
        {isHistory && dates && dates.length > 0 ? (
          /* History mode: clickable date+hour button + editable time */
          <div className="flex items-center gap-1.5">
            <button
              ref={dateButtonRef}
              onClick={toggleCalendar}
              className="font-mono text-xs px-1.5 py-0.5 rounded bg-[var(--bg-elevated)] text-[var(--text-primary)] border border-[var(--border-default)] hover:bg-[var(--bg-hover)] focus:outline-none focus:ring-1 focus:ring-[var(--accent)] cursor-pointer transition-colors"
            >
              {currentDateStr} {String(currentHour ?? 0).padStart(2, "0")}:00
            </button>
            {ts > 0 && (
              <TimeInput
                timestamp={ts}
                timezone={tz}
                onSubmit={handleTimeSubmit}
              />
            )}
          </div>
        ) : (
          /* Live mode or no dates: static timestamp */
          <span className="text-xs text-[var(--text-tertiary)] font-mono tabular-nums">
            {ts > 0 ? formatTimestamp(ts, tz) : "-"}
          </span>
        )}
        {authUsername && (
          <span className="text-xs text-[var(--text-tertiary)] font-mono">
            {authUsername}
          </span>
        )}
        <button
          onClick={timezoneHook.cycle}
          className="text-[10px] font-mono font-semibold px-1.5 py-0.5 rounded bg-[var(--bg-elevated)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors"
          title="Cycle timezone: Local → UTC → Moscow"
        >
          {TZ_DISPLAY[timezoneHook.timezone]}
        </button>
        {onHelpOpen && (
          <button
            onClick={onHelpOpen}
            className="p-1 rounded text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors"
            title="Help (?)"
          >
            <HelpCircle size={16} />
          </button>
        )}
        <button
          onClick={themeHook.cycle}
          className="p-1 rounded text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors"
          title={`Theme: ${themeHook.theme}`}
        >
          <ThemeIcon size={16} />
        </button>
      </div>

      {/* Calendar popover */}
      {calendarOpen && anchorRect && dates && dates.length > 0 && (
        <CalendarPopover
          dates={dates}
          currentDate={currentDateStr}
          onSelectDate={handleSelectDate}
          onClose={() => setCalendarOpen(false)}
          anchorRect={anchorRect}
          currentHour={currentHour}
          onSelectHour={handleHourChange}
          timezone={tz}
        />
      )}
    </div>
  );
}

function HealthBadge({ snapshot }: { snapshot: ApiSnapshot }) {
  const score = snapshot.health_score;
  const bd = snapshot.health_breakdown;
  const color = healthColor(score);
  const bgColor = healthBgColor(score);

  const penalties: { icon: typeof Users; label: string; value: number }[] = [
    { icon: Users, label: "Sessions", value: bd.sessions },
    { icon: Zap, label: "CPU", value: bd.cpu },
    { icon: HardDrive, label: "Disk IOPS", value: bd.disk_iops },
    { icon: Activity, label: "Disk BW", value: bd.disk_bw },
  ];

  return (
    <RichTooltip
      content={
        <div className="w-44">
          <div className="flex items-center justify-between mb-2">
            <span className="font-semibold text-[var(--text-primary)]">
              Health
            </span>
            <span className="font-bold text-sm" style={{ color }}>
              {score}
              <span className="text-[var(--text-tertiary)] font-normal">
                /100
              </span>
            </span>
          </div>
          <div className="space-y-1">
            {penalties.map((p) => {
              const active = p.value > 0;
              const pColor =
                p.value >= 20
                  ? "var(--status-critical)"
                  : "var(--status-warning)";
              return (
                <div
                  key={p.label}
                  className="flex items-center gap-2"
                  style={{ opacity: active ? 1 : 0.35 }}
                >
                  <p.icon
                    size={10}
                    className="shrink-0"
                    style={{ color: active ? pColor : "var(--text-tertiary)" }}
                  />
                  <span className="text-[var(--text-secondary)] flex-1">
                    {p.label}
                  </span>
                  <span
                    className="font-mono w-6 text-right"
                    style={{ color: active ? pColor : "var(--text-tertiary)" }}
                  >
                    {active ? `\u2212${p.value}` : "0"}
                  </span>
                </div>
              );
            })}
          </div>
        </div>
      }
      side="bottom"
    >
      <span
        className="flex items-center gap-1 text-xs px-2 py-0.5 rounded-full font-medium cursor-default"
        style={{ backgroundColor: bgColor, color }}
      >
        <HeartPulse size={12} />
        {score}
      </span>
    </RichTooltip>
  );
}

function SessionBadge({ snapshot }: { snapshot: ApiSnapshot }) {
  const sc = snapshot.session_counts;
  const activeColor =
    sc.active > 50
      ? "var(--status-critical)"
      : sc.active > 20
        ? "var(--status-warning)"
        : "var(--text-secondary)";

  return (
    <RichTooltip
      content={
        <div className="space-y-1">
          <div className="font-semibold text-[var(--text-primary)]">
            Sessions: {sc.total}
          </div>
          <div className="text-xs text-[var(--text-secondary)]">
            Active: {sc.active} &middot; Idle: {sc.idle} &middot; IdleTx:{" "}
            {sc.idle_in_transaction}
          </div>
        </div>
      }
      side="bottom"
    >
      <span
        className="flex items-center gap-1 text-xs px-2 py-0.5 rounded-full font-medium cursor-default bg-[var(--bg-elevated)] border border-[var(--border-default)]"
        style={{ color: activeColor }}
      >
        <Users size={10} />
        {sc.active}/{sc.total}
      </span>
    </RichTooltip>
  );
}

function ReplicationBadge({ snapshot }: { snapshot: ApiSnapshot }) {
  const repl = snapshot.replication;
  if (!repl) return null;

  if (repl.is_standby) {
    const lag = repl.replay_lag_s;
    const lagText =
      lag == null ? "?" : lag < 1 ? "<1s" : `${lag}s`;
    const color =
      lag == null
        ? "var(--text-tertiary)"
        : lag <= 5
          ? "var(--status-success)"
          : lag <= 30
            ? "var(--status-warning)"
            : "var(--status-critical)";
    const bgColor =
      lag == null
        ? "var(--bg-elevated)"
        : lag <= 5
          ? "var(--status-success-bg)"
          : lag <= 30
            ? "var(--status-warning-bg)"
            : "var(--status-critical-bg)";

    const senderHost = repl.sender_host;

    return (
      <RichTooltip
        content={
          <div className="w-52">
            <div className="font-semibold text-[var(--text-primary)] mb-1">
              Standby (Replica)
            </div>
            <div className="text-xs text-[var(--text-secondary)]">
              Replay lag: {lagText}
            </div>
            {senderHost && (
              <>
                <div className="border-t border-[var(--border-default)] my-1.5" />
                <a
                  href={`${window.location.protocol}//${senderHost}:${window.location.port}/`}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="flex items-center gap-1.5 text-xs px-1.5 py-1 rounded hover:bg-[var(--bg-hover)] transition-colors text-[var(--text-secondary)] hover:text-[var(--text-primary)] cursor-pointer"
                >
                  <ArrowUp size={10} className="shrink-0 text-[var(--status-success)]" />
                  <span className="flex-1 min-w-0 truncate">Primary: {senderHost}</span>
                  <ExternalLink size={10} className="shrink-0 opacity-50" />
                </a>
              </>
            )}
          </div>
        }
        side="bottom"
      >
        <span
          className="flex items-center gap-1 text-xs px-2 py-0.5 rounded-full font-medium cursor-default"
          style={{ backgroundColor: bgColor, color }}
        >
          <ArrowDown size={10} />
          Standby &middot; {lagText}
        </span>
      </RichTooltip>
    );
  }

  // Primary
  const n = repl.connected_replicas;
  const color = "var(--status-success)";
  const bgColor = "var(--status-success-bg)";

  const formatBytes = (b: number | undefined) => {
    if (b == null) return "?";
    if (b < 1024) return `${b} B`;
    if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KiB`;
    if (b < 1024 * 1024 * 1024)
      return `${(b / 1024 / 1024).toFixed(1)} MiB`;
    return `${(b / 1024 / 1024 / 1024).toFixed(1)} GiB`;
  };

  return (
    <RichTooltip
      content={
        <div className="w-72">
          <div className="font-semibold text-[var(--text-primary)] mb-1">
            Primary (Master)
          </div>
          {repl.replicas.length > 0 ? (
            <div className="space-y-0.5">
              {repl.replicas.map((r, i) => {
                const replicaUrl = r.client_addr
                  ? `${window.location.protocol}//${r.client_addr}:${window.location.port}/`
                  : null;
                const inner = (
                  <>
                    <span
                      className="w-1.5 h-1.5 rounded-full shrink-0"
                      style={{
                        backgroundColor:
                          r.state === "streaming"
                            ? "var(--status-success)"
                            : "var(--status-warning)",
                      }}
                    />
                    <span
                      className="text-[var(--text-secondary)] flex-1 min-w-0 truncate"
                      title={r.application_name || r.client_addr || undefined}
                    >
                      {r.application_name || r.client_addr || "local"}
                    </span>
                    <span className="text-[var(--text-tertiary)] shrink-0">
                      {r.sync_state}
                    </span>
                    <span className="text-[var(--text-tertiary)] font-mono shrink-0">
                      {formatBytes(r.replay_lag_bytes)}
                    </span>
                    {replicaUrl && (
                      <ExternalLink size={10} className="shrink-0 opacity-50" />
                    )}
                  </>
                );
                return replicaUrl ? (
                  <a
                    key={i}
                    href={replicaUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-xs flex items-center gap-1.5 px-1.5 py-1 rounded hover:bg-[var(--bg-hover)] transition-colors cursor-pointer"
                  >
                    {inner}
                  </a>
                ) : (
                  <div
                    key={i}
                    className="text-xs flex items-center gap-1.5 px-1.5 py-1"
                  >
                    {inner}
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="text-xs text-[var(--text-tertiary)]">
              No connected replicas
            </div>
          )}
        </div>
      }
      side="bottom"
    >
      <span
        className="flex items-center gap-1 text-xs px-2 py-0.5 rounded-full font-medium cursor-default"
        style={{ backgroundColor: bgColor, color }}
      >
        <GitBranch size={10} />
        Primary{n > 0 ? ` \u00b7 ${n}R` : ""}
      </span>
    </RichTooltip>
  );
}
