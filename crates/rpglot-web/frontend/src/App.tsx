import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import { ShieldX } from "lucide-react";
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
import { getDatePartsInTz, dateToEpochInTz } from "./utils/formatters";
import { TabBar } from "./components/TabBar";
import { SummaryPanel } from "./components/SummaryPanel";
import { Timeline } from "./components/Timeline";
import { HelpModal } from "./components/HelpModal";
import { AnalysisModal } from "./components/AnalysisModal";
import type { AnalysisJump } from "./components/AnalysisModal";
import { Header, deriveLargestDb } from "./components/Header";
import { TabContent } from "./components/TabContent";
import { HintsBar } from "./components/HintsBar";
import { useTabState } from "./hooks/useTabState";
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
  ApiSchema,
  ApiSnapshot,
  InstanceInfo,
  HeatmapBucket,
} from "./api/types";

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
        authUsername={_authUsername}
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
        hasDrillDown={!!(schema.tabs[tabState.activeTab].drill_downs?.length)}
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

  // Handle jump from analysis modal — switch tab + apply filters + jump to timestamp
  const handleAnalysisJump = useCallback(
    (jump: AnalysisJump) => {
      if (jump.tab) {
        tabState.handleTabChange(jump.tab);
      }
      if (jump.view) {
        tabState.handleViewChange(jump.view);
      }
      // Reset smart filters so the target row is visible
      tabState.resetSmartFilters();
      // Apply filters: global text filter or column filter (or null for both — clearing)
      tabState.setGlobalFilterPreset(jump.filter ?? null);
      tabState.setColumnFilterPreset(jump.columnFilter ?? null);
      handleManualJump(jump.timestamp);
      if (jump.entityId != null) {
        tabState.handleSelectRow(jump.entityId);
      }
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
        authUsername={_authUsername}
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
        hasDrillDown={!!(schema.tabs[tabState.activeTab].drill_downs?.length)}
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
