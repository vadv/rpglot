import type { ApiSchema, ApiSnapshot, TimelineInfo } from "./types";
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
  position?: number;
  timestamp?: number;
}): Promise<ApiSnapshot> {
  const url = new URL(`${BASE}/snapshot`, window.location.origin);
  if (params?.position != null)
    url.searchParams.set("position", String(params.position));
  if (params?.timestamp != null)
    url.searchParams.set("timestamp", String(params.timestamp));
  const res = await authFetch(url.toString());
  if (!res.ok) throw new Error(`snapshot: ${res.status}`);
  return res.json();
}

export async function fetchTimeline(): Promise<TimelineInfo> {
  const res = await authFetch(`${BASE}/timeline`);
  if (!res.ok) throw new Error(`timeline: ${res.status}`);
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
