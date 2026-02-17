import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import {
  Database,
  Radio,
  History,
  Pause,
  Play,
  Sun,
  Moon,
  Monitor,
  ShieldX,
  HelpCircle,
  HeartPulse,
  Zap,
  FileText,
  Users,
  Server,
  HardDrive,
  Activity,
} from "lucide-react";
import {
  fetchTimeline,
  fetchTimelineLatest,
  fetchHeatmap,
  fetchAuthConfig,
  fetchAnalysis,
} from "./api/client";
import { useSchema } from "./hooks/useSchema";
import { useLiveSnapshot, useHistorySnapshot } from "./hooks/useSnapshot";
import { readUrlState, useUrlSync } from "./hooks/useUrlState";
import { useTheme } from "./hooks/useTheme";
import { useTimezone } from "./hooks/useTimezone";
import {
  formatTimestamp,
  formatDate,
  getDatePartsInTz,
  dateToEpochInTz,
} from "./utils/formatters";
import type { TimezoneMode } from "./utils/formatters";
import { TabBar } from "./components/TabBar";
import { SummaryPanel } from "./components/SummaryPanel";
import { DataTable } from "./components/DataTable";
import { DetailPanel } from "./components/DetailPanel";
import { Timeline, CalendarPopover, TimeInput } from "./components/Timeline";
import { HelpModal } from "./components/HelpModal";
import { AnalysisModal } from "./components/AnalysisModal";
import type { AnalysisJump } from "./components/AnalysisModal";
import { RichTooltip } from "./components/RichTooltip";
import { healthColor, healthBgColor } from "./utils/healthScore";
import {
  captureTokenFromUrl,
  getToken,
  getTokenUsername,
  redirectToSso,
  setSsoProxyUrl,
  startTokenRefresh,
} from "./auth";
import type {
  AnalysisReport,
  ApiSnapshot,
  ApiSchema,
  InstanceInfo,
  TabKey,
  DrillDown,
  TimelineInfo,
  DateInfo,
  HeatmapBucket,
  ViewSchema,
  ColumnSchema,
  DataType,
  Unit,
  Format,
} from "./api/types";

const TAB_ORDER: TabKey[] = ["prc", "pga", "pgs", "pgt", "pgi", "pge", "pgl"];

// Global auth username — set once during init, read by AppContent
let _authUsername: string | null = null;

export default function App() {
  const [authReady, setAuthReady] = useState(false);
  const [ssoProxyUrl, setSsoProxyUrlState] = useState<string | null>(null);

  useEffect(() => {
    captureTokenFromUrl();

    fetchAuthConfig().then((config) => {
      const proxyUrl = config.sso_proxy_url ?? null;
      setSsoProxyUrlState(proxyUrl);
      setSsoProxyUrl(proxyUrl);

      if (proxyUrl) {
        const token = getToken();
        if (!token) {
          redirectToSso(proxyUrl);
          return;
        }
        // SSO: extract username from JWT
        _authUsername = getTokenUsername();
      } else if (config.auth_user) {
        // Basic Auth: username from server config
        _authUsername = config.auth_user;
      }
      setAuthReady(true);
    });
  }, []);

  // Periodic token refresh
  useEffect(() => {
    if (!ssoProxyUrl) return;
    return startTokenRefresh(ssoProxyUrl);
  }, [ssoProxyUrl]);

  if (!authReady) {
    return (
      <div className="flex items-center justify-center min-h-screen text-[var(--text-tertiary)]">
        Authenticating...
      </div>
    );
  }

  return <AppContent />;
}

function AppContent() {
  const { schema, error: schemaError, forbiddenUser } = useSchema();

  if (forbiddenUser) {
    return (
      <div className="flex items-center justify-center min-h-screen">
        <div className="text-center max-w-md px-6">
          <ShieldX
            className="mx-auto mb-4 text-[var(--status-critical)]"
            size={48}
          />
          <h1 className="text-xl font-semibold text-[var(--text-primary)] mb-2">
            Access Denied
          </h1>
          <p className="text-[var(--text-secondary)] mb-4">
            User{" "}
            <span className="font-mono text-[var(--text-primary)]">
              {forbiddenUser}
            </span>{" "}
            is not authorized to access this instance.
          </p>
          <p className="text-sm text-[var(--text-tertiary)]">
            Contact the administrator to request access.
          </p>
        </div>
      </div>
    );
  }

  if (schemaError) {
    return (
      <div className="flex items-center justify-center min-h-screen text-[var(--status-critical)]">
        Failed to load schema: {schemaError}
      </div>
    );
  }

  if (!schema) {
    return (
      <div className="flex items-center justify-center min-h-screen text-[var(--text-tertiary)]">
        Loading...
      </div>
    );
  }

  if (schema.mode === "history") {
    return <HistoryApp schema={schema} />;
  }

  return <LiveApp schema={schema} />;
}

/** Derive the largest database name from PGT rows (by total size_bytes). */
function deriveLargestDb(snapshot: ApiSnapshot | null | undefined): string {
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

/** Updates document.title with health score + instance name. */
function useDocumentTitle(
  snapshot: ApiSnapshot | null | undefined,
  instance?: InstanceInfo,
) {
  useEffect(() => {
    if (!snapshot) {
      document.title = "rpglot";
      return;
    }
    const score = snapshot.health_score;
    const prefix = score <= 50 ? `[!${score}]` : `[${score}]`;
    const dbName = instance?.database ?? deriveLargestDb(snapshot);
    document.title = dbName
      ? `${prefix} ${dbName} \u2014 rpglot`
      : `${prefix} rpglot`;
  }, [snapshot?.health_score, instance?.database, snapshot]);
}

function LiveApp({ schema }: { schema: ApiSchema }) {
  const { snapshot, paused, togglePause } = useLiveSnapshot();
  const tabState = useTabState(schema, snapshot);
  const urlSync = useUrlSync();
  const themeHook = useTheme();
  const timezoneHook = useTimezone();
  useDocumentTitle(snapshot, schema.instance);

  // Sync pause timestamp to URL
  useEffect(() => {
    if (paused && snapshot) {
      urlSync({ timestamp: snapshot.timestamp });
    } else {
      urlSync({ timestamp: null });
    }
  }, [paused, snapshot, urlSync]);

  // Global keyboard: Space to toggle pause
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const target = e.target as HTMLElement;
      if (target.tagName === "INPUT" || target.tagName === "TEXTAREA") return;
      if (e.key === " ") {
        e.preventDefault();
        togglePause();
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [togglePause]);

  return (
    <div className="flex flex-col h-screen">
      <Header
        mode="live"
        timestamp={snapshot?.timestamp}
        paused={paused}
        onTogglePause={togglePause}
        themeHook={themeHook}
        timezoneHook={timezoneHook}
        onHelpOpen={() => tabState.setHelpOpen(true)}
        snapshot={snapshot}
        version={schema.version}
        instance={schema.instance}
      />
      {snapshot && <SummaryPanel snapshot={snapshot} schema={schema.summary} />}
      <TabBar
        activeTab={tabState.activeTab}
        onTabChange={tabState.handleTabChange}
      />
      <div className="flex-1 min-h-0">
        {snapshot ? (
          <TabContent snapshot={snapshot} schema={schema} tabState={tabState} />
        ) : (
          <div className="flex items-center justify-center h-full text-[var(--text-tertiary)]">
            Waiting for data...
          </div>
        )}
      </div>
      <HintsBar
        mode="live"
        detailOpen={tabState.detailOpen}
        hasSelection={tabState.selectedId != null}
        hasDrillDown={!!schema.tabs[tabState.activeTab].drill_down}
        paused={paused}
      />
      {tabState.helpOpen && (
        <HelpModal
          tab={tabState.activeTab}
          view={tabState.activeView}
          onClose={() => tabState.setHelpOpen(false)}
        />
      )}
    </div>
  );
}

function HistoryApp({ schema }: { schema: ApiSchema }) {
  const { snapshot, loading, error, jumpToTimestamp } = useHistorySnapshot();
  const urlSync = useUrlSync();
  const urlState = readUrlState();
  const tabState = useTabState(schema, snapshot);
  const themeHook = useTheme();
  const timezoneHook = useTimezone();
  useDocumentTitle(snapshot, schema.instance);
  const [timeline, setTimeline] = useState(schema.timeline ?? null);
  const [heatmapBuckets, setHeatmapBuckets] = useState<HeatmapBucket[]>([]);
  const snapshotRef = useRef(snapshot);
  snapshotRef.current = snapshot;
  const [playSpeed, setPlaySpeed] = useState<number | null>(null);
  const [liveFollow, setLiveFollow] = useState(false);
  const [analysisReport, setAnalysisReport] = useState<AnalysisReport | null>(
    () => {
      try {
        const cached = sessionStorage.getItem("rpglot_analysis");
        return cached ? JSON.parse(cached) : null;
      } catch {
        return null;
      }
    },
  );
  const [analyzing, setAnalyzing] = useState(false);
  const [analyzeStartedAt, setAnalyzeStartedAt] = useState<number | null>(null);
  const [analysisModalOpen, setAnalysisModalOpen] = useState(false);

  // Persist analysis report to sessionStorage so it survives SSO refreshes
  useEffect(() => {
    if (analysisReport) {
      sessionStorage.setItem("rpglot_analysis", JSON.stringify(analysisReport));
    } else {
      sessionStorage.removeItem("rpglot_analysis");
    }
  }, [analysisReport]);

  // Compute current hour boundaries from timestamp
  const hourRange = useMemo(() => {
    const ts = snapshot?.timestamp ?? 0;
    if (ts <= 0) return null;
    const tz = timezoneHook.timezone;
    const parts = getDatePartsInTz(ts, tz);
    const hourStart = dateToEpochInTz(
      parts.year,
      parts.month,
      parts.day,
      parts.hour,
      0,
      0,
      tz,
    );
    const hourEnd = hourStart + 3599;
    return { start: hourStart, end: hourEnd, hour: parts.hour };
  }, [snapshot?.timestamp, timezoneHook.timezone]);

  // Clear stale analysis report when the hour changes
  const heatmapKey = hourRange ? `${hourRange.start}-${hourRange.end}` : "";
  const prevHeatmapKey = useRef(heatmapKey);
  useEffect(() => {
    if (prevHeatmapKey.current !== heatmapKey) {
      prevHeatmapKey.current = heatmapKey;
      setAnalysisReport(null);
      setAnalysisModalOpen(false);
    }
  }, [heatmapKey]);

  // Load heatmap data for the current hour (and refresh periodically)
  useEffect(() => {
    if (!hourRange) return;
    const { start, end } = hourRange;
    let cancelled = false;
    const load = () => {
      fetchHeatmap(start, end, 400).then((buckets) => {
        if (!cancelled) setHeatmapBuckets(buckets);
      });
    };
    load();
    const interval = setInterval(load, 30_000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [heatmapKey]);

  // On mount: jump to URL timestamp
  useEffect(() => {
    if (urlState.timestamp != null) {
      jumpToTimestamp(urlState.timestamp);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Fetch full timeline (with dates) on mount and periodically
  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      try {
        const tl = await fetchTimeline();
        if (!cancelled) setTimeline(tl);
      } catch {
        // keep schema.timeline as fallback
      }
    };
    load();
    const interval = setInterval(load, 30_000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, []);

  const handleTimestampJump = useCallback(
    (ts: number, direction?: "floor" | "ceil") => {
      jumpToTimestamp(ts, direction);
      urlSync({ timestamp: ts });
    },
    [jumpToTimestamp, urlSync],
  );

  // Manual navigation — disables both Play and Live
  const handleManualJump = useCallback(
    (ts: number, direction?: "floor" | "ceil") => {
      setPlaySpeed(null);
      setLiveFollow(false);
      handleTimestampJump(ts, direction);
    },
    [handleTimestampJump],
  );

  // Play: cycle speed x1 → x2 → x4 → x8 → off
  const handlePlayToggle = useCallback(() => {
    setLiveFollow(false);
    setPlaySpeed((prev) => {
      if (prev == null) return 1;
      if (prev === 1) return 2;
      if (prev === 2) return 4;
      if (prev === 4) return 8;
      return null; // x8 → off
    });
  }, []);

  // Live: toggle follow-latest
  const handleLiveToggle = useCallback(() => {
    setPlaySpeed(null);
    setLiveFollow((prev) => !prev);
  }, []);

  // Handle jump from analysis modal — switch tab + jump to timestamp
  const handleAnalysisJump = useCallback(
    (jump: AnalysisJump) => {
      if (jump.tab) {
        tabState.handleTabChange(jump.tab);
      }
      handleManualJump(jump.timestamp);
    },
    [handleManualJump, tabState],
  );

  // Analyze current hour
  const handleAnalyze = useCallback(async () => {
    if (!hourRange || analyzing) return;
    setAnalyzing(true);
    setAnalyzeStartedAt(Date.now());
    try {
      const report = await fetchAnalysis(hourRange.start, hourRange.end);
      setAnalysisReport(report);
      setAnalysisModalOpen(true);
    } catch (err) {
      console.error("Analysis failed:", err);
    } finally {
      setAnalyzing(false);
      setAnalyzeStartedAt(null);
    }
  }, [hourRange, analyzing]);

  // Play effect — sequential playback using next_timestamp
  useEffect(() => {
    if (playSpeed == null) return;
    const intervalMs = 1000 / playSpeed;

    const interval = setInterval(() => {
      const nextTs = snapshotRef.current?.next_timestamp;
      if (nextTs != null) {
        handleTimestampJump(nextTs);
      } else {
        setPlaySpeed(null);
      }
    }, intervalMs);

    return () => clearInterval(interval);
  }, [playSpeed, handleTimestampJump]);

  // Live effect — aggressive polling + auto-jump to latest
  useEffect(() => {
    if (!liveFollow) return;

    if (timeline) {
      handleTimestampJump(timeline.end);
    }

    const interval = setInterval(async () => {
      try {
        const latest = await fetchTimelineLatest();
        const currentTs = snapshotRef.current?.timestamp ?? 0;
        if (latest.end > currentTs) {
          handleTimestampJump(latest.end);
        }
      } catch {
        /* ignore */
      }
    }, 5_000);

    return () => clearInterval(interval);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [liveFollow, handleTimestampJump]);

  // Keyboard: Left/Right to step, Shift+Left/Right to step ±1 hour, Space to stop
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const target = e.target as HTMLElement;
      const isTextInput =
        target.tagName === "TEXTAREA" ||
        target.tagName === "SELECT" ||
        (target.tagName === "INPUT" &&
          (target as HTMLInputElement).type !== "range");
      if (isTextInput) return;
      // Blur range input so it doesn't capture further keys
      if (
        target.tagName === "INPUT" &&
        (target as HTMLInputElement).type === "range"
      ) {
        (target as HTMLInputElement).blur();
      }
      // Space: stop Play/Live
      if (e.key === " ") {
        e.preventDefault();
        setPlaySpeed(null);
        setLiveFollow(false);
        return;
      }
      // Shift+Arrow: step ±1 hour
      if (e.shiftKey && e.key === "ArrowLeft") {
        e.preventDefault();
        const ts = snapshotRef.current?.timestamp;
        if (ts) handleManualJump(ts - 3600);
        return;
      }
      if (e.shiftKey && e.key === "ArrowRight") {
        e.preventDefault();
        const ts = snapshotRef.current?.timestamp;
        if (ts) handleManualJump(ts + 3600);
        return;
      }
      // Arrow: step ±1 snapshot via prev/next timestamp
      if (e.key === "ArrowLeft") {
        e.preventDefault();
        const prevTs = snapshotRef.current?.prev_timestamp;
        if (prevTs != null) handleManualJump(prevTs);
      } else if (e.key === "ArrowRight") {
        e.preventDefault();
        const nextTs = snapshotRef.current?.next_timestamp;
        if (nextTs != null) handleManualJump(nextTs);
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleManualJump]);

  return (
    <div className="flex flex-col h-screen">
      <Header
        mode="history"
        timestamp={snapshot?.timestamp}
        loading={loading}
        themeHook={themeHook}
        timezoneHook={timezoneHook}
        onHelpOpen={() => tabState.setHelpOpen(true)}
        timeline={timeline ?? undefined}
        onTimestampJump={handleManualJump}
        snapshot={snapshot}
        currentHour={hourRange?.hour}
        version={schema.version}
        instance={schema.instance}
        analyzing={analyzing}
        analyzeStartedAt={analyzeStartedAt}
        onAnalyze={handleAnalyze}
        hasReport={!!analysisReport}
        onShowReport={() => setAnalysisModalOpen(true)}
      />
      {snapshot && <SummaryPanel snapshot={snapshot} schema={schema.summary} />}
      <TabBar
        activeTab={tabState.activeTab}
        onTabChange={tabState.handleTabChange}
      />
      <div className="flex-1 min-h-0">
        {snapshot ? (
          <TabContent snapshot={snapshot} schema={schema} tabState={tabState} />
        ) : error ? (
          <div className="flex items-center justify-center h-full text-[var(--status-critical)]">
            {error}
          </div>
        ) : (
          <div className="flex items-center justify-center h-full text-[var(--text-tertiary)]">
            Loading...
          </div>
        )}
      </div>
      <HintsBar
        mode="history"
        detailOpen={tabState.detailOpen}
        hasSelection={tabState.selectedId != null}
        hasDrillDown={!!schema.tabs[tabState.activeTab].drill_down}
      />
      {tabState.helpOpen && (
        <HelpModal
          tab={tabState.activeTab}
          view={tabState.activeView}
          onClose={() => tabState.setHelpOpen(false)}
        />
      )}
      {timeline && (
        <Timeline
          timeline={timeline}
          onTimestampJump={handleManualJump}
          timestamp={snapshot?.timestamp}
          prevTimestamp={snapshot?.prev_timestamp}
          nextTimestamp={snapshot?.next_timestamp}
          timezone={timezoneHook.timezone}
          heatmapBuckets={heatmapBuckets}
          hourStart={hourRange?.start}
          hourEnd={hourRange?.end}
          playSpeed={playSpeed}
          onPlayToggle={handlePlayToggle}
          liveFollow={liveFollow}
          onLiveToggle={handleLiveToggle}
        />
      )}
      {analysisModalOpen && analysisReport && (
        <AnalysisModal
          report={analysisReport}
          timezone={timezoneHook.timezone}
          onClose={() => setAnalysisModalOpen(false)}
          onJump={handleAnalysisJump}
        />
      )}
    </div>
  );
}

// ============================================================
// Tab state hook — selection, detail, drill-down, URL sync
// ============================================================

interface TabState {
  activeTab: TabKey;
  selectedId: string | number | null;
  detailOpen: boolean;
  helpOpen: boolean;
  activeView: string;
  initialView: string | null;
  initialFilter: string | null;
  handleTabChange: (tab: TabKey) => void;
  handleSelectRow: (id: string | number | null) => void;
  handleOpenDetail: () => void;
  handleCloseDetail: () => void;
  handleDrillDown: (drillDown: DrillDown, value: unknown) => void;
  handleViewChange: (view: string) => void;
  handleFilterChange: (filter: string) => void;
  setHelpOpen: (open: boolean) => void;
}

function useTabState(
  schema: ApiSchema,
  snapshot: ApiSnapshot | null,
): TabState {
  const urlSync = useUrlSync();
  const [urlState] = useState(() => readUrlState());

  const [activeTab, setActiveTab] = useState<TabKey>(urlState.tab);
  const [selectedId, setSelectedId] = useState<string | number | null>(null);
  const [detailOpen, setDetailOpen] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);
  const [activeView, setActiveView] = useState("");
  const [drillDownTarget, setDrillDownTarget] = useState<{
    tab: TabKey;
    targetField?: string;
    value: unknown;
  } | null>(null);

  // Initial view/filter from URL (consumed once by DataTable on mount)
  const [initialView] = useState<string | null>(urlState.view);
  const [initialFilter] = useState<string | null>(urlState.filter);

  // Reset selection on tab change
  const handleTabChange = useCallback(
    (tab: TabKey) => {
      setActiveTab(tab);
      setSelectedId(null);
      setDetailOpen(false);
      urlSync({ tab, view: null, filter: null });
    },
    [urlSync],
  );

  // Validate selection: close detail if entity disappeared
  useEffect(() => {
    if (!snapshot || selectedId == null) return;
    const data = getTabData(snapshot, activeTab);
    const entityId = schema.tabs[activeTab].entity_id;
    const exists = data.some((row) => row[entityId] === selectedId);
    if (!exists) {
      setSelectedId(null);
      setDetailOpen(false);
    }
  }, [snapshot, selectedId, activeTab, schema]);

  // Drill-down: after tab switch, find and select target row
  useEffect(() => {
    if (!drillDownTarget || !snapshot) return;
    if (activeTab !== drillDownTarget.tab) return;

    const data = getTabData(snapshot, drillDownTarget.tab);
    const entityId = schema.tabs[drillDownTarget.tab].entity_id;
    const searchField = drillDownTarget.targetField ?? entityId;
    const targetRow = data.find(
      (row) => row[searchField] === drillDownTarget.value,
    );
    if (targetRow) {
      setSelectedId(targetRow[entityId] as string | number);
      setDetailOpen(true);
    }
    setDrillDownTarget(null);
  }, [drillDownTarget, snapshot, activeTab, schema]);

  const handleSelectRow = useCallback((id: string | number | null) => {
    setSelectedId(id);
    setDetailOpen(id != null);
  }, []);

  const handleOpenDetail = useCallback(() => {
    setDetailOpen(true);
  }, []);

  const handleCloseDetail = useCallback(() => {
    setDetailOpen(false);
  }, []);

  const handleDrillDown = useCallback(
    (drillDown: DrillDown, value: unknown) => {
      const targetTab = drillDown.target as TabKey;
      setDrillDownTarget({
        tab: targetTab,
        targetField: drillDown.target_field,
        value,
      });
      setActiveTab(targetTab);
      setSelectedId(null);
      setDetailOpen(false);
      urlSync({ tab: targetTab, view: null, filter: null });
    },
    [urlSync],
  );

  const handleViewChange = useCallback(
    (view: string) => {
      setActiveView(view);
      urlSync({ view });
    },
    [urlSync],
  );

  const handleFilterChange = useCallback(
    (filter: string) => {
      urlSync({ filter: filter || null });
    },
    [urlSync],
  );

  // Global keyboard shortcuts
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const target = e.target as HTMLElement;
      if (target.tagName === "INPUT" || target.tagName === "TEXTAREA") return;

      // ?: toggle help modal
      if (e.key === "?") {
        e.preventDefault();
        setHelpOpen((prev) => !prev);
        return;
      }

      // 1-6: switch tabs
      const tabIndex = parseInt(e.key) - 1;
      if (tabIndex >= 0 && tabIndex < TAB_ORDER.length) {
        e.preventDefault();
        handleTabChange(TAB_ORDER[tabIndex]);
        return;
      }

      // Escape: close detail first, then deselect
      if (e.key === "Escape") {
        e.preventDefault();
        if (detailOpen) {
          setDetailOpen(false);
        } else if (selectedId != null) {
          setSelectedId(null);
        }
        return;
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [detailOpen, selectedId, handleTabChange]);

  return {
    activeTab,
    selectedId,
    detailOpen,
    helpOpen,
    activeView,
    initialView,
    initialFilter,
    handleTabChange,
    handleSelectRow,
    handleOpenDetail,
    handleCloseDetail,
    handleDrillDown,
    handleViewChange,
    handleFilterChange,
    setHelpOpen,
  };
}

// ============================================================
// Components
// ============================================================

interface ThemeHook {
  theme: "light" | "dark" | "system";
  effective: "light" | "dark";
  cycle: () => void;
}

interface TimezoneHookType {
  timezone: TimezoneMode;
  cycle: () => void;
}

const TZ_DISPLAY: Record<TimezoneMode, string> = {
  local: "LOCAL",
  utc: "UTC",
  moscow: "MSK",
};

function Header({
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
        {_authUsername && (
          <span className="text-xs text-[var(--text-tertiary)] font-mono">
            {_authUsername}
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

// ============================================================
// Smart filters: per-tab, per-view problem detection
// ============================================================

/** PRC: returns true if process is a PostgreSQL backend */
function isPgProcess(row: Record<string, unknown>): boolean {
  const bt = row.pg_backend_type;
  return typeof bt === "string" && bt.length > 0;
}

/** PGT: returns true if table has problems on given view */
function isPgtProblematic(row: Record<string, unknown>, view: string): boolean {
  switch (view) {
    case "reads":
      return (
        (num(row.io_hit_pct) > 0 && num(row.io_hit_pct) < 90) ||
        num(row.disk_blks_read_s) > 0
      );
    case "writes":
      return num(row.dead_pct) > 5 || num(row.n_dead_tup) > 1000;
    case "scans":
      return (
        num(row.seq_pct) > 50 &&
        num(row.seq_scan_s) > 0 &&
        num(row.n_live_tup) > 10000
      );
    case "maintenance":
      return num(row.dead_pct) > 5 || num(row.n_dead_tup) > 10000;
    case "io":
      return (
        (num(row.io_hit_pct) > 0 && num(row.io_hit_pct) < 90) ||
        num(row.disk_blks_read_s) > 0
      );
    default:
      return true; // unknown view — don't filter
  }
}

/** PGI: returns true if index has problems on given view */
function isPgiProblematic(row: Record<string, unknown>, view: string): boolean {
  switch (view) {
    case "usage":
      return num(row.idx_scan) === 0 || num(row.idx_scan_s) === 0;
    case "unused":
      return true; // already shows only unused — don't filter
    case "io":
      return (
        (num(row.io_hit_pct) > 0 && num(row.io_hit_pct) < 90) ||
        num(row.disk_blks_read_s) > 0
      );
    default:
      return true;
  }
}

/** Safe numeric accessor — treats null/undefined/NaN as 0 */
function num(v: unknown): number {
  if (v == null) return 0;
  const n = Number(v);
  return Number.isFinite(n) ? n : 0;
}

// ============================================================
// PGT Schema view — client-side aggregation by schema
// ============================================================

const SCHEMA_VIEW: ViewSchema = {
  key: "schema",
  label: "Schema",
  columns: [
    "schema",
    "tables",
    "size_bytes",
    "n_live_tup",
    "n_dead_tup",
    "dead_pct",
    "seq_scan_s",
    "idx_scan_s",
    "seq_pct",
    "tup_read_s",
    "ins_s",
    "upd_s",
    "del_s",
    "blk_rd_s",
    "blk_hit_s",
    "io_hit_pct",
  ],
  default: false,
  default_sort: "blk_rd_s",
  default_sort_desc: true,
};

const SCHEMA_COLUMNS: ColumnSchema[] = [
  {
    key: "schema",
    label: "Schema",
    type: "string" as DataType,
    sortable: true,
    filterable: true,
  },
  {
    key: "tables",
    label: "Tables",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "size_bytes",
    label: "Size",
    type: "integer" as DataType,
    unit: "bytes" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "n_live_tup",
    label: "Live Tuples",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "n_dead_tup",
    label: "Dead Tuples",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "dead_pct",
    label: "DEAD%",
    type: "number" as DataType,
    unit: "percent" as Unit,
    format: "percent" as Format,
    sortable: true,
  },
  {
    key: "seq_scan_s",
    label: "Seq/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "idx_scan_s",
    label: "Idx/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "seq_pct",
    label: "SEQ%",
    type: "number" as DataType,
    unit: "percent" as Unit,
    format: "percent" as Format,
    sortable: true,
  },
  {
    key: "tup_read_s",
    label: "Tup Rd/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "ins_s",
    label: "Ins/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "upd_s",
    label: "Upd/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "del_s",
    label: "Del/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "blk_rd_s",
    label: "Disk Read/s",
    type: "number" as DataType,
    unit: "blks/s" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "blk_hit_s",
    label: "Buf Hit/s",
    type: "number" as DataType,
    unit: "blks/s" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "io_hit_pct",
    label: "HIT%",
    type: "number" as DataType,
    unit: "percent" as Unit,
    format: "percent" as Format,
    sortable: true,
  },
];

/** Aggregate PGT rows by a grouping field (schema or database) */
function aggregateTableRows(
  rows: Record<string, unknown>[],
  groupBy: string,
): Record<string, unknown>[] {
  const map = new Map<
    string,
    {
      tables: number;
      size_bytes: number;
      n_live_tup: number;
      n_dead_tup: number;
      seq_scan_s: number;
      idx_scan_s: number;
      seq_tup_read_s: number;
      idx_tup_fetch_s: number;
      ins_s: number;
      upd_s: number;
      del_s: number;
      heap_blks_read_s: number;
      heap_blks_hit_s: number;
      idx_blks_read_s: number;
      idx_blks_hit_s: number;
    }
  >();

  for (const row of rows) {
    const key = String(row[groupBy] ?? "unknown");
    let agg = map.get(key);
    if (!agg) {
      agg = {
        tables: 0,
        size_bytes: 0,
        n_live_tup: 0,
        n_dead_tup: 0,
        seq_scan_s: 0,
        idx_scan_s: 0,
        seq_tup_read_s: 0,
        idx_tup_fetch_s: 0,
        ins_s: 0,
        upd_s: 0,
        del_s: 0,
        heap_blks_read_s: 0,
        heap_blks_hit_s: 0,
        idx_blks_read_s: 0,
        idx_blks_hit_s: 0,
      };
      map.set(key, agg);
    }
    agg.tables += 1;
    agg.size_bytes += num(row.size_bytes);
    agg.n_live_tup += num(row.n_live_tup);
    agg.n_dead_tup += num(row.n_dead_tup);
    agg.seq_scan_s += num(row.seq_scan_s);
    agg.idx_scan_s += num(row.idx_scan_s);
    agg.seq_tup_read_s += num(row.seq_tup_read_s);
    agg.idx_tup_fetch_s += num(row.idx_tup_fetch_s);
    agg.ins_s += num(row.n_tup_ins_s);
    agg.upd_s += num(row.n_tup_upd_s);
    agg.del_s += num(row.n_tup_del_s);
    agg.heap_blks_read_s += num(row.heap_blks_read_s);
    agg.heap_blks_hit_s += num(row.heap_blks_hit_s);
    agg.idx_blks_read_s += num(row.idx_blks_read_s);
    agg.idx_blks_hit_s += num(row.idx_blks_hit_s);
  }

  const result: Record<string, unknown>[] = [];
  for (const [key, agg] of map) {
    const totalTup = agg.n_live_tup + agg.n_dead_tup;
    const totalScans = agg.seq_scan_s + agg.idx_scan_s;
    const totalReads = agg.heap_blks_read_s + agg.idx_blks_read_s;
    const totalHits = agg.heap_blks_hit_s + agg.idx_blks_hit_s;
    const totalIO = totalReads + totalHits;

    result.push({
      [groupBy]: key,
      tables: agg.tables,
      size_bytes: agg.size_bytes,
      n_live_tup: agg.n_live_tup,
      n_dead_tup: agg.n_dead_tup,
      dead_pct: totalTup > 0 ? (agg.n_dead_tup / totalTup) * 100 : null,
      seq_scan_s: agg.seq_scan_s,
      idx_scan_s: agg.idx_scan_s,
      seq_pct: totalScans > 0 ? (agg.seq_scan_s / totalScans) * 100 : null,
      tup_read_s: agg.seq_tup_read_s + agg.idx_tup_fetch_s,
      ins_s: agg.ins_s,
      upd_s: agg.upd_s,
      del_s: agg.del_s,
      blk_rd_s: totalReads,
      blk_hit_s: totalHits,
      io_hit_pct: totalIO > 0 ? (totalHits / totalIO) * 100 : null,
    });
  }
  return result;
}

// ============================================================
// PGT Database view — client-side aggregation by database
// ============================================================

const DATABASE_VIEW: ViewSchema = {
  key: "database",
  label: "Database",
  columns: [
    "database",
    "tables",
    "size_bytes",
    "n_live_tup",
    "n_dead_tup",
    "dead_pct",
    "seq_scan_s",
    "idx_scan_s",
    "seq_pct",
    "tup_read_s",
    "ins_s",
    "upd_s",
    "del_s",
    "blk_rd_s",
    "blk_hit_s",
    "io_hit_pct",
  ],
  default: false,
  default_sort: "blk_rd_s",
  default_sort_desc: true,
};

const DATABASE_COLUMNS: ColumnSchema[] = [
  {
    key: "database",
    label: "Database",
    type: "string" as DataType,
    sortable: true,
    filterable: true,
  },
  {
    key: "tables",
    label: "Tables",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "size_bytes",
    label: "Size",
    type: "integer" as DataType,
    unit: "bytes" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "n_live_tup",
    label: "Live Tuples",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "n_dead_tup",
    label: "Dead Tuples",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "dead_pct",
    label: "DEAD%",
    type: "number" as DataType,
    unit: "percent" as Unit,
    format: "percent" as Format,
    sortable: true,
  },
  {
    key: "seq_scan_s",
    label: "Seq/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "idx_scan_s",
    label: "Idx/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "seq_pct",
    label: "SEQ%",
    type: "number" as DataType,
    unit: "percent" as Unit,
    format: "percent" as Format,
    sortable: true,
  },
  {
    key: "tup_read_s",
    label: "Tup Rd/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "ins_s",
    label: "Ins/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "upd_s",
    label: "Upd/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "del_s",
    label: "Del/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "blk_rd_s",
    label: "Disk Read/s",
    type: "number" as DataType,
    unit: "blks/s" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "blk_hit_s",
    label: "Buf Hit/s",
    type: "number" as DataType,
    unit: "blks/s" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "io_hit_pct",
    label: "HIT%",
    type: "number" as DataType,
    unit: "percent" as Unit,
    format: "percent" as Format,
    sortable: true,
  },
];

// ============================================================
// PGI Schema view — client-side aggregation by schema
// ============================================================

const PGI_SCHEMA_VIEW: ViewSchema = {
  key: "schema",
  label: "Schema",
  columns: [
    "schema",
    "indexes",
    "tables",
    "size_bytes",
    "idx_scan_s",
    "idx_tup_read_s",
    "idx_tup_fetch_s",
    "blk_rd_s",
    "blk_hit_s",
    "io_hit_pct",
    "unused",
  ],
  default: false,
  default_sort: "blk_rd_s",
  default_sort_desc: true,
};

const PGI_SCHEMA_COLUMNS: ColumnSchema[] = [
  {
    key: "schema",
    label: "Schema",
    type: "string" as DataType,
    sortable: true,
    filterable: true,
  },
  {
    key: "indexes",
    label: "Indexes",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "tables",
    label: "Tables",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "size_bytes",
    label: "Size",
    type: "integer" as DataType,
    unit: "bytes" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "idx_scan_s",
    label: "Scan/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "idx_tup_read_s",
    label: "Tup Rd/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "idx_tup_fetch_s",
    label: "Tup Ft/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "blk_rd_s",
    label: "Disk Read/s",
    type: "number" as DataType,
    unit: "blks/s" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "blk_hit_s",
    label: "Buf Hit/s",
    type: "number" as DataType,
    unit: "blks/s" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "io_hit_pct",
    label: "HIT%",
    type: "number" as DataType,
    unit: "percent" as Unit,
    format: "percent" as Format,
    sortable: true,
  },
  {
    key: "unused",
    label: "Unused",
    type: "integer" as DataType,
    sortable: true,
  },
];

/** Aggregate PGI rows by a grouping field (schema or database) */
function aggregateIndexRows(
  rows: Record<string, unknown>[],
  groupBy: string,
): Record<string, unknown>[] {
  const map = new Map<
    string,
    {
      indexes: number;
      relids: Set<number>;
      size_bytes: number;
      idx_scan_s: number;
      idx_tup_read_s: number;
      idx_tup_fetch_s: number;
      idx_blks_read_s: number;
      idx_blks_hit_s: number;
      unused: number;
    }
  >();

  for (const row of rows) {
    const key = String(row[groupBy] ?? "unknown");
    let agg = map.get(key);
    if (!agg) {
      agg = {
        indexes: 0,
        relids: new Set(),
        size_bytes: 0,
        idx_scan_s: 0,
        idx_tup_read_s: 0,
        idx_tup_fetch_s: 0,
        idx_blks_read_s: 0,
        idx_blks_hit_s: 0,
        unused: 0,
      };
      map.set(key, agg);
    }
    agg.indexes += 1;
    agg.relids.add(num(row.relid));
    agg.size_bytes += num(row.size_bytes);
    agg.idx_scan_s += num(row.idx_scan_s);
    agg.idx_tup_read_s += num(row.idx_tup_read_s);
    agg.idx_tup_fetch_s += num(row.idx_tup_fetch_s);
    agg.idx_blks_read_s += num(row.idx_blks_read_s);
    agg.idx_blks_hit_s += num(row.idx_blks_hit_s);
    if (num(row.idx_scan) === 0) agg.unused += 1;
  }

  const result: Record<string, unknown>[] = [];
  for (const [key, agg] of map) {
    const totalIO = agg.idx_blks_read_s + agg.idx_blks_hit_s;

    result.push({
      [groupBy]: key,
      indexes: agg.indexes,
      tables: agg.relids.size,
      size_bytes: agg.size_bytes,
      idx_scan_s: agg.idx_scan_s,
      idx_tup_read_s: agg.idx_tup_read_s,
      idx_tup_fetch_s: agg.idx_tup_fetch_s,
      blk_rd_s: agg.idx_blks_read_s,
      blk_hit_s: agg.idx_blks_hit_s,
      io_hit_pct: totalIO > 0 ? (agg.idx_blks_hit_s / totalIO) * 100 : null,
      unused: agg.unused,
    });
  }
  return result;
}

// ============================================================
// PGI Database view — client-side aggregation by database
// ============================================================

const PGI_DATABASE_VIEW: ViewSchema = {
  key: "database",
  label: "Database",
  columns: [
    "database",
    "indexes",
    "tables",
    "size_bytes",
    "idx_scan_s",
    "idx_tup_read_s",
    "idx_tup_fetch_s",
    "blk_rd_s",
    "blk_hit_s",
    "io_hit_pct",
    "unused",
  ],
  default: false,
  default_sort: "blk_rd_s",
  default_sort_desc: true,
};

const PGI_DATABASE_COLUMNS: ColumnSchema[] = [
  {
    key: "database",
    label: "Database",
    type: "string" as DataType,
    sortable: true,
    filterable: true,
  },
  {
    key: "indexes",
    label: "Indexes",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "tables",
    label: "Tables",
    type: "integer" as DataType,
    sortable: true,
  },
  {
    key: "size_bytes",
    label: "Size",
    type: "integer" as DataType,
    unit: "bytes" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "idx_scan_s",
    label: "Scan/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "idx_tup_read_s",
    label: "Tup Rd/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "idx_tup_fetch_s",
    label: "Tup Ft/s",
    type: "number" as DataType,
    unit: "per_sec" as Unit,
    format: "rate" as Format,
    sortable: true,
  },
  {
    key: "blk_rd_s",
    label: "Disk Read/s",
    type: "number" as DataType,
    unit: "blks/s" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "blk_hit_s",
    label: "Buf Hit/s",
    type: "number" as DataType,
    unit: "blks/s" as Unit,
    format: "bytes" as Format,
    sortable: true,
  },
  {
    key: "io_hit_pct",
    label: "HIT%",
    type: "number" as DataType,
    unit: "percent" as Unit,
    format: "percent" as Format,
    sortable: true,
  },
  {
    key: "unused",
    label: "Unused",
    type: "integer" as DataType,
    sortable: true,
  },
];

function TabContent({
  snapshot,
  schema,
  tabState,
}: {
  snapshot: ApiSnapshot;
  schema: ApiSchema;
  tabState: TabState;
}) {
  const {
    activeTab,
    selectedId,
    detailOpen,
    activeView,
    initialView,
    initialFilter,
    handleSelectRow,
    handleOpenDetail,
    handleCloseDetail,
    handleDrillDown,
    handleViewChange,
    handleFilterChange,
  } = tabState;
  const tabSchema = schema.tabs[activeTab];
  const rawData = getTabData(snapshot, activeTab);

  const isAggregatedView =
    (activeTab === "pgt" || activeTab === "pgi") &&
    (activeView === "schema" || activeView === "database");

  // Inject Schema + Database views into PGT/PGI views list
  const effectiveViews = useMemo(() => {
    if (activeTab === "pgt") {
      return [...tabSchema.views, SCHEMA_VIEW, DATABASE_VIEW];
    }
    if (activeTab === "pgi") {
      return [...tabSchema.views, PGI_SCHEMA_VIEW, PGI_DATABASE_VIEW];
    }
    return tabSchema.views;
  }, [activeTab, tabSchema.views]);

  // PGA-specific toggle filters (hide idle / hide system backends)
  const [hideIdle, setHideIdle] = useState(true);
  const [hideSystem, setHideSystem] = useState(true);

  // PRC: only PostgreSQL processes (default ON)
  const [pgOnly, setPgOnly] = useState(true);

  // PGS: hide statements with no calls delta (default ON)
  const [hideInactive, setHideInactive] = useState(true);

  // PGT/PGI: only problematic rows (default OFF — show all)
  const [problemsOnly, setProblemsOnly] = useState(false);

  // Reset problemsOnly when switching tabs (keep it per-tab)
  useEffect(() => {
    setProblemsOnly(false);
  }, [activeTab]);

  const data = useMemo(() => {
    let filtered = rawData;

    if (activeTab === "pga") {
      filtered = filtered.filter((row) => {
        if (hideIdle && row.state === "idle") return false;
        if (hideSystem) {
          if (
            row.backend_type !== "client backend" &&
            row.backend_type !== "autovacuum worker"
          )
            return false;
          if (
            typeof row.application_name === "string" &&
            (row.application_name as string).startsWith("rpglot")
          )
            return false;
        }
        return true;
      });
    }

    if (activeTab === "prc" && pgOnly) {
      filtered = filtered.filter(isPgProcess);
    }

    if (activeTab === "pge") {
      if (activeView === "errors") {
        filtered = filtered.filter((row) => {
          const t = row.event_type as string;
          return t === "error" || t === "fatal" || t === "panic";
        });
      } else if (activeView === "checkpoints") {
        filtered = filtered.filter((row) => {
          const t = row.event_type as string;
          return t === "checkpoint_starting" || t === "checkpoint_complete";
        });
      } else if (activeView === "autovacuum") {
        filtered = filtered.filter((row) => {
          const t = row.event_type as string;
          return t === "autovacuum" || t === "autoanalyze";
        });
      }
    }

    if (activeTab === "pgs" && hideInactive) {
      filtered = filtered.filter((row) => {
        const c = row.calls_s as number | null;
        return c != null && c > 0;
      });
    }

    if (activeTab === "pgt" && problemsOnly) {
      filtered = filtered.filter((row) => isPgtProblematic(row, activeView));
    }

    if (activeTab === "pgi" && problemsOnly) {
      filtered = filtered.filter((row) => isPgiProblematic(row, activeView));
    }

    return filtered;
  }, [
    rawData,
    activeTab,
    activeView,
    hideIdle,
    hideSystem,
    pgOnly,
    hideInactive,
    problemsOnly,
  ]);

  // Count hidden items for toggle button labels
  const hiddenCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    if (activeTab === "pga") {
      if (hideIdle)
        counts.idle = rawData.filter((r) => r.state === "idle").length;
      if (hideSystem)
        counts.system = rawData.filter(
          (r) =>
            (r.backend_type !== "client backend" &&
              r.backend_type !== "autovacuum worker") ||
            (typeof r.application_name === "string" &&
              (r.application_name as string).startsWith("rpglot")),
        ).length;
    }
    if (activeTab === "prc" && pgOnly) {
      counts.nonpg = rawData.filter((r) => !isPgProcess(r)).length;
    }
    if (activeTab === "pgs" && hideInactive) {
      counts.inactive = rawData.filter((r) => {
        const c = r.calls_s as number | null;
        return c == null || c === 0;
      }).length;
    }
    if ((activeTab === "pgt" || activeTab === "pgi") && problemsOnly) {
      const fn =
        activeTab === "pgt"
          ? (r: Record<string, unknown>) => isPgtProblematic(r, activeView)
          : (r: Record<string, unknown>) => isPgiProblematic(r, activeView);
      counts.healthy = rawData.filter((r) => !fn(r)).length;
    }
    return counts;
  }, [
    rawData,
    activeTab,
    activeView,
    hideIdle,
    hideSystem,
    pgOnly,
    hideInactive,
    problemsOnly,
  ]);

  const toolbarControls = useMemo(() => {
    if (activeTab === "pga") {
      return (
        <>
          <ToggleButton
            active={hideIdle}
            onClick={() => setHideIdle((p) => !p)}
            label="idle"
            count={hiddenCounts.idle}
          />
          <ToggleButton
            active={hideSystem}
            onClick={() => setHideSystem((p) => !p)}
            label="system"
            count={hiddenCounts.system}
          />
        </>
      );
    }
    if (activeTab === "prc") {
      return (
        <ToggleButton
          active={pgOnly}
          onClick={() => setPgOnly((p) => !p)}
          label="non-pg"
          count={hiddenCounts.nonpg}
        />
      );
    }
    if (activeTab === "pgs") {
      return (
        <ToggleButton
          active={hideInactive}
          onClick={() => setHideInactive((p) => !p)}
          label="inactive"
          count={hiddenCounts.inactive}
        />
      );
    }
    if ((activeTab === "pgt" || activeTab === "pgi") && !isAggregatedView) {
      return (
        <ToggleButton
          active={problemsOnly}
          onClick={() => setProblemsOnly((p) => !p)}
          label="healthy"
          count={hiddenCounts.healthy}
          invertLabel
        />
      );
    }
    return undefined;
  }, [
    activeTab,
    hideIdle,
    hideSystem,
    pgOnly,
    hideInactive,
    problemsOnly,
    hiddenCounts,
    isAggregatedView,
  ]);

  // Aggregated views: schema or database grouping
  const aggregatedData = useMemo(() => {
    if (!isAggregatedView) return null;
    if (activeView === "schema" || activeView === "database") {
      if (activeTab === "pgt") return aggregateTableRows(rawData, activeView);
      if (activeTab === "pgi") return aggregateIndexRows(rawData, activeView);
    }
    return null;
  }, [isAggregatedView, activeView, activeTab, rawData]);

  const effectiveData = isAggregatedView ? aggregatedData! : data;
  const effectiveColumns = isAggregatedView
    ? activeView === "database"
      ? activeTab === "pgi"
        ? PGI_DATABASE_COLUMNS
        : DATABASE_COLUMNS
      : activeTab === "pgi"
        ? PGI_SCHEMA_COLUMNS
        : SCHEMA_COLUMNS
    : tabSchema.columns;
  const effectiveEntityId = isAggregatedView
    ? activeView === "database"
      ? "database"
      : "schema"
    : tabSchema.entity_id;

  const selectedRow =
    selectedId != null
      ? (effectiveData.find((row) => row[effectiveEntityId] === selectedId) ??
        null)
      : null;

  return (
    <div className="flex h-full">
      <div className="flex-1 min-w-0">
        <DataTable
          key={activeTab}
          data={effectiveData}
          columns={effectiveColumns}
          views={effectiveViews}
          entityId={effectiveEntityId}
          selectedId={selectedId}
          onSelectRow={handleSelectRow}
          onOpenDetail={handleOpenDetail}
          isLockTree={activeTab === "pgl"}
          activeTab={activeTab}
          initialView={initialView}
          initialFilter={initialFilter}
          onViewChange={handleViewChange}
          onFilterChange={handleFilterChange}
          snapshotTimestamp={snapshot.timestamp}
          toolbarControls={toolbarControls}
        />
      </div>
      {detailOpen && selectedRow && !isAggregatedView && (
        <DetailPanel
          tab={activeTab}
          row={selectedRow}
          columns={tabSchema.columns}
          columnOverrides={
            effectiveViews.find((v) => v.key === activeView)?.column_overrides
          }
          drillDown={tabSchema.drill_down}
          onClose={handleCloseDetail}
          onDrillDown={handleDrillDown}
          snapshotTimestamp={snapshot.timestamp}
        />
      )}
    </div>
  );
}

/**
 * Toggle button for filtering.
 * Default: active=true means "hiding items" (shows "+label (N)"), inactive = "showing all" (shows "-label").
 * invertLabel: active=true means "showing filtered" (shows "-label (N)"), inactive = "showing all" (shows "+label").
 */
function ToggleButton({
  active,
  onClick,
  label,
  count,
  invertLabel,
}: {
  active: boolean;
  onClick: () => void;
  label: string;
  count?: number;
  invertLabel?: boolean;
}) {
  // invertLabel: when true, active state means "filter is ON" → highlight the button
  // Default (no invertLabel): active means "hiding items" → dimmed button with count
  const prefix = invertLabel ? (active ? "-" : "+") : active ? "+" : "-";
  const highlighted = invertLabel ? active : !active;

  return (
    <button
      onClick={onClick}
      className={`text-[11px] px-2 py-0.5 rounded-full font-medium transition-colors whitespace-nowrap border ${
        highlighted
          ? "bg-[var(--accent-muted)] text-[var(--accent-text)] border-[var(--accent-text)]/30"
          : "bg-transparent text-[var(--text-secondary)] border-[var(--border-default)] hover:border-[var(--text-tertiary)] hover:text-[var(--text-primary)]"
      }`}
      title={active ? `Show ${label}` : `Hide ${label}`}
    >
      {prefix} {label}
      {active && count != null && count > 0 && (
        <span className="ml-0.5 opacity-70">({count})</span>
      )}
    </button>
  );
}

function HintsBar({
  mode,
  detailOpen,
  hasSelection,
  hasDrillDown,
  paused,
}: {
  mode: "live" | "history";
  detailOpen: boolean;
  hasSelection: boolean;
  hasDrillDown: boolean;
  paused?: boolean;
}) {
  return (
    <div className="flex items-center gap-4 px-4 py-1 bg-[var(--bg-surface)] border-t border-[var(--border-default)] text-[11px] text-[var(--text-tertiary)]">
      <Hint keys="1-7" action="tabs" />
      <Hint keys="j/k" action="navigate" />
      {(detailOpen || hasSelection) && (
        <Hint keys="Esc" action={detailOpen ? "close detail" : "deselect"} />
      )}
      {hasSelection && hasDrillDown && <Hint keys=">" action="drill-down" />}
      <Hint keys="/" action="filter" />
      {mode === "live" && (
        <Hint keys="Space" action={paused ? "resume" : "pause"} />
      )}
      {mode === "history" && <Hint keys="←/→" action="step" />}
      {mode === "history" && <Hint keys="Shift+←/→" action="±1h" />}
      <Hint keys="?" action="help" />
    </div>
  );
}

function Hint({ keys, action }: { keys: string; action: string }) {
  return (
    <span className="flex items-center gap-1">
      <kbd className="inline-flex items-center justify-center min-w-[18px] h-[18px] px-1 bg-[var(--bg-elevated)] border border-[var(--border-default)] rounded text-[10px] font-mono text-[var(--text-secondary)]">
        {keys}
      </kbd>
      <span>{action}</span>
    </span>
  );
}

// ============================================================
// Helpers
// ============================================================

function getTabData(
  snapshot: ApiSnapshot,
  tab: TabKey,
): Record<string, unknown>[] {
  switch (tab) {
    case "prc":
      return snapshot.prc as unknown as Record<string, unknown>[];
    case "pga":
      return snapshot.pga as unknown as Record<string, unknown>[];
    case "pgs":
      return snapshot.pgs as unknown as Record<string, unknown>[];
    case "pgt":
      return snapshot.pgt as unknown as Record<string, unknown>[];
    case "pgi":
      return snapshot.pgi as unknown as Record<string, unknown>[];
    case "pge":
      return snapshot.pge as unknown as Record<string, unknown>[];
    case "pgl":
      return snapshot.pgl as unknown as Record<string, unknown>[];
  }
}
