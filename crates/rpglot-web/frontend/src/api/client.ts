import type { ApiSchema, ApiSnapshot } from './types';

const BASE = '/api/v1';

export async function fetchSchema(): Promise<ApiSchema> {
  const res = await fetch(`${BASE}/schema`);
  if (!res.ok) throw new Error(`schema: ${res.status}`);
  return res.json();
}

export async function fetchSnapshot(params?: {
  position?: number;
  timestamp?: number;
}): Promise<ApiSnapshot> {
  const url = new URL(`${BASE}/snapshot`, window.location.origin);
  if (params?.position != null) url.searchParams.set('position', String(params.position));
  if (params?.timestamp != null) url.searchParams.set('timestamp', String(params.timestamp));
  const res = await fetch(url.toString());
  if (!res.ok) throw new Error(`snapshot: ${res.status}`);
  return res.json();
}

export function subscribeSSE(
  onSnapshot: (snap: ApiSnapshot) => void,
  onError?: (err: Event) => void,
): EventSource {
  const es = new EventSource(`${BASE}/stream`);
  es.addEventListener('snapshot', (ev) => {
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
