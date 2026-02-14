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
  ShieldX,
} from "lucide-react";
import { fetchTimeline, fetchAuthConfig } from "./api/client";
import { useSchema } from "./hooks/useSchema";
import { useLiveSnapshot, useHistorySnapshot } from "./hooks/useSnapshot";
import { readUrlState, useUrlSync } from "./hooks/useUrlState";
import { useTheme } from "./hooks/useTheme";
import { useTimezone } from "./hooks/useTimezone";
import { formatTimestamp, formatDate } from "./utils/formatters";
import type { TimezoneMode } from "./utils/formatters";
import { TabBar } from "./components/TabBar";
import { SummaryPanel } from "./components/SummaryPanel";
import { DataTable } from "./components/DataTable";
import { DetailPanel } from "./components/DetailPanel";
import { Timeline, CalendarPopover, TimeInput } from "./components/Timeline";
import {
  captureTokenFromUrl,
  getToken,
  getTokenUsername,
  redirectToSso,
  setSsoProxyUrl,
  startTokenRefresh,
} from "./auth";
import type {
  ApiSnapshot,
  ApiSchema,
  TabKey,
  DrillDown,
  TimelineInfo,
  DateInfo,
} from "./api/types";

const TAB_ORDER: TabKey[] = ["prc", "pga", "pgs", "pgt", "pgi", "pgl"];

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

function LiveApp({ schema }: { schema: ApiSchema }) {
  const { snapshot, paused, togglePause } = useLiveSnapshot();
  const tabState = useTabState(schema, snapshot);
  const urlSync = useUrlSync();
  const themeHook = useTheme();
  const timezoneHook = useTimezone();

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
    </div>
  );
}

function HistoryApp({ schema }: { schema: ApiSchema }) {
  const { snapshot, loading, jumpTo, jumpToTimestamp } = useHistorySnapshot();
  const urlSync = useUrlSync();
  const urlState = readUrlState();
  const tabState = useTabState(schema, snapshot);
  const themeHook = useTheme();
  const timezoneHook = useTimezone();
  const [position, setPosition] = useState(() => urlState.position ?? 0);
  const [timeline, setTimeline] = useState(schema.timeline ?? null);
  const positionRef = useRef(position);
  positionRef.current = position;
  const snapshotRef = useRef(snapshot);
  snapshotRef.current = snapshot;

  // On mount: jump to URL position
  useEffect(() => {
    if (urlState.position != null && urlState.position > 0) {
      jumpTo(urlState.position);
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

  // Sync position from snapshot.position (after backend jump)
  useEffect(() => {
    if (
      snapshot?.position != null &&
      snapshot.position !== positionRef.current
    ) {
      setPosition(snapshot.position);
    }
  }, [snapshot]);

  const totalSnapshots = timeline?.total_snapshots ?? 0;

  const handlePositionChange = useCallback(
    (pos: number) => {
      setPosition(pos);
      jumpTo(pos);
      urlSync({ position: pos });
    },
    [jumpTo, urlSync],
  );

  const handleTimestampJump = useCallback(
    (ts: number) => {
      jumpToTimestamp(ts);
    },
    [jumpToTimestamp],
  );

  // Keyboard: Left/Right to step, Shift+Left/Right to step ±1 hour
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const target = e.target as HTMLElement;
      if (
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.tagName === "SELECT"
      )
        return;
      // Shift+Arrow: step ±1 hour
      if (e.shiftKey && e.key === "ArrowLeft") {
        e.preventDefault();
        const ts = snapshotRef.current?.timestamp;
        if (ts) handleTimestampJump(ts - 3600);
        return;
      }
      if (e.shiftKey && e.key === "ArrowRight") {
        e.preventDefault();
        const ts = snapshotRef.current?.timestamp;
        if (ts) handleTimestampJump(ts + 3600);
        return;
      }
      // Arrow: step ±1 snapshot
      if (e.key === "ArrowLeft") {
        e.preventDefault();
        const cur = positionRef.current;
        if (cur > 0) handlePositionChange(cur - 1);
      } else if (e.key === "ArrowRight") {
        e.preventDefault();
        const cur = positionRef.current;
        if (cur < totalSnapshots - 1) handlePositionChange(cur + 1);
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handlePositionChange, handleTimestampJump, totalSnapshots]);

  return (
    <div className="flex flex-col h-screen">
      <Header
        mode="history"
        timestamp={snapshot?.timestamp}
        loading={loading}
        themeHook={themeHook}
        timezoneHook={timezoneHook}
        timeline={timeline ?? undefined}
        onTimestampJump={handleTimestampJump}
        onDateSelect={handlePositionChange}
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
      {timeline && (
        <Timeline
          timeline={timeline}
          position={position}
          onPositionChange={handlePositionChange}
          onTimestampJump={handleTimestampJump}
          timestamp={snapshot?.timestamp}
          timezone={timezoneHook.timezone}
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
  initialView: string | null;
  initialFilter: string | null;
  handleTabChange: (tab: TabKey) => void;
  handleSelectRow: (id: string | number | null) => void;
  handleOpenDetail: () => void;
  handleCloseDetail: () => void;
  handleDrillDown: (drillDown: DrillDown, value: unknown) => void;
  handleViewChange: (view: string) => void;
  handleFilterChange: (filter: string) => void;
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
    initialView,
    initialFilter,
    handleTabChange,
    handleSelectRow,
    handleOpenDetail,
    handleCloseDetail,
    handleDrillDown,
    handleViewChange,
    handleFilterChange,
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
  onDateSelect,
}: {
  mode: string;
  timestamp?: number;
  loading?: boolean;
  paused?: boolean;
  onTogglePause?: () => void;
  themeHook: ThemeHook;
  timezoneHook: TimezoneHookType;
  timeline?: TimelineInfo;
  onTimestampJump?: (ts: number) => void;
  onDateSelect?: (position: number) => void;
}) {
  const [calendarOpen, setCalendarOpen] = useState(false);
  const [anchorRect, setAnchorRect] = useState<DOMRect | null>(null);
  const dateButtonRef = useRef<HTMLButtonElement>(null);

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
      onDateSelect?.(dateInfo.first_position);
      setCalendarOpen(false);
    },
    [onDateSelect],
  );

  const handleTimeSubmit = useCallback(
    (epoch: number) => {
      onTimestampJump?.(epoch);
    },
    [onTimestampJump],
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
        </div>
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
      </div>
      <div className="flex items-center gap-3">
        {loading && (
          <span className="text-xs text-[var(--status-warning)]">
            loading...
          </span>
        )}
        {isHistory && dates && dates.length > 0 ? (
          /* History mode: clickable date + editable time */
          <div className="flex items-center gap-1.5">
            <button
              ref={dateButtonRef}
              onClick={toggleCalendar}
              className="font-mono text-xs px-1.5 py-0.5 rounded bg-[var(--bg-elevated)] text-[var(--text-primary)] border border-[var(--border-default)] hover:bg-[var(--bg-hover)] focus:outline-none focus:ring-1 focus:ring-[var(--accent)] cursor-pointer transition-colors"
            >
              {currentDateStr}
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
        />
      )}
    </div>
  );
}

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
  const data = getTabData(snapshot, activeTab);

  const selectedRow =
    selectedId != null
      ? (data.find((row) => row[tabSchema.entity_id] === selectedId) ?? null)
      : null;

  return (
    <div className="flex h-full">
      <div className="flex-1 min-w-0">
        <DataTable
          key={activeTab}
          data={data}
          columns={tabSchema.columns}
          views={tabSchema.views}
          entityId={tabSchema.entity_id}
          selectedId={selectedId}
          onSelectRow={handleSelectRow}
          onOpenDetail={handleOpenDetail}
          isLockTree={activeTab === "pgl"}
          initialView={initialView}
          initialFilter={initialFilter}
          onViewChange={handleViewChange}
          onFilterChange={handleFilterChange}
          snapshotTimestamp={snapshot.timestamp}
        />
      </div>
      {detailOpen && selectedRow && (
        <DetailPanel
          tab={activeTab}
          row={selectedRow}
          columns={tabSchema.columns}
          drillDown={tabSchema.drill_down}
          onClose={handleCloseDetail}
          onDrillDown={handleDrillDown}
          snapshotTimestamp={snapshot.timestamp}
        />
      )}
    </div>
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
      <Hint keys="1-6" action="tabs" />
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
    case "pgl":
      return snapshot.pgl as unknown as Record<string, unknown>[];
  }
}
