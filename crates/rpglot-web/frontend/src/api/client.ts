import type {
  AnalysisReport,
  ApiSchema,
  ApiSnapshot,
  HeatmapBucket,
  TimelineInfo,
} from "./types";
import { getToken, clearToken } from "../auth";

const BASE = "/api/v1";

function authHeaders(): HeadersInit {
  const token = getToken();
  return token ? { Authorization: `Bearer ${token}` } : {};
}

export class ForbiddenError extends Error {
  username: string;
  constructor(username: string) {
    super(`Access denied for user: ${username}`);
    this.username = username;
  }
}

async function authFetch(url: string, init?: RequestInit): Promise<Response> {
  const res = await fetch(url, {
    ...init,
    headers: { ...authHeaders(), ...init?.headers },
  });
  if (res.status === 401) {
    clearToken();
    window.location.reload();
    throw new Error("unauthorized");
  }
  if (res.status === 403) {
    const body = await res.json().catch(() => ({}));
    throw new ForbiddenError(body.username ?? "unknown");
  }
  return res;
}

export async function fetchAuthConfig(): Promise<{
  sso_proxy_url: string | null;
  auth_user: string | null;
}> {
  const res = await fetch(`${BASE}/auth/config`);
  if (!res.ok) return { sso_proxy_url: null, auth_user: null };
  return res.json();
}

export async function fetchSchema(): Promise<ApiSchema> {
  const res = await authFetch(`${BASE}/schema`);
  if (!res.ok) throw new Error(`schema: ${res.status}`);
  return res.json();
}

export async function fetchSnapshot(params?: {
  timestamp?: number;
  direction?: "floor" | "ceil";
}): Promise<ApiSnapshot> {
  const url = new URL(`${BASE}/snapshot`, window.location.origin);
  if (params?.timestamp != null)
    url.searchParams.set("timestamp", String(params.timestamp));
  if (params?.direction) url.searchParams.set("direction", params.direction);
  const res = await authFetch(url.toString());
  if (!res.ok) throw new Error(`snapshot: ${res.status}`);
  return res.json();
}

export async function fetchTimeline(): Promise<TimelineInfo> {
  const res = await authFetch(`${BASE}/timeline`);
  if (!res.ok) throw new Error(`timeline: ${res.status}`);
  return res.json();
}

export async function fetchTimelineLatest(): Promise<{
  end: number;
  total_snapshots: number;
}> {
  const res = await authFetch(`${BASE}/timeline/latest`);
  if (!res.ok) throw new Error(`timeline/latest: ${res.status}`);
  return res.json();
}

export async function fetchHeatmap(
  start: number,
  end: number,
  buckets?: number,
): Promise<HeatmapBucket[]> {
  const url = new URL(`${BASE}/timeline/heatmap`, window.location.origin);
  url.searchParams.set("start", String(start));
  url.searchParams.set("end", String(end));
  if (buckets) url.searchParams.set("buckets", String(buckets));
  const res = await authFetch(url.toString());
  if (!res.ok) return [];
  return res.json();
}

export async function fetchAnalysis(
  start: number,
  end: number,
): Promise<AnalysisReport> {
  const url = new URL(`${BASE}/analysis`, window.location.origin);
  url.searchParams.set("start", String(start));
  url.searchParams.set("end", String(end));
  const res = await authFetch(url.toString());
  if (!res.ok) throw new Error(`analysis: ${res.status}`);
  return res.json();
}

export function subscribeSSE(
  onSnapshot: (snap: ApiSnapshot) => void,
  onError?: (err: Event) => void,
): EventSource {
  let url = `${BASE}/stream`;
  const token = getToken();
  if (token) {
    url += `?token=${encodeURIComponent(token)}`;
  }
  const es = new EventSource(url);
  es.addEventListener("snapshot", (ev) => {
    try {
      const snap: ApiSnapshot = JSON.parse(ev.data);
      onSnapshot(snap);
    } catch {
      // ignore parse errors
    }
  });
  if (onError) {
    es.onerror = onError;
  }
  return es;
}
