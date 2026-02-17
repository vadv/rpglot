import { useCallback, useEffect, useRef, useState } from "react";
import { fetchSnapshot, subscribeSSE } from "../api/client";
import { getToken, getSsoProxyUrl, redirectToSso } from "../auth";
import type { ApiSnapshot } from "../api/types";

export function useLiveSnapshot() {
  const [snapshot, setSnapshot] = useState<ApiSnapshot | null>(null);
  const [paused, setPaused] = useState(false);
  const pausedRef = useRef(false);
  const esRef = useRef<EventSource | null>(null);

  useEffect(() => {
    const es = subscribeSSE(
      (snap) => {
        if (!pausedRef.current) {
          setSnapshot(snap);
        }
      },
      () => {
        // SSE error — may be auth failure (401).
        // EventSource doesn't expose status codes, so check token validity.
        const token = getToken();
        const proxyUrl = getSsoProxyUrl();
        if (!token && proxyUrl) {
          es.close();
          redirectToSso(proxyUrl);
        }
        // Otherwise: network error — EventSource auto-reconnects.
      },
    );
    esRef.current = es;
    return () => {
      es.close();
      esRef.current = null;
    };
  }, []);

  const togglePause = useCallback(() => {
    setPaused((prev) => {
      pausedRef.current = !prev;
      return !prev;
    });
  }, []);

  return { snapshot, paused, togglePause };
}

export function useHistorySnapshot() {
  const [snapshot, setSnapshot] = useState<ApiSnapshot | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const jumpToTimestamp = useCallback(
    (timestamp: number, direction?: "floor" | "ceil") => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
      setLoading(true);
      setError(null);
      debounceRef.current = setTimeout(async () => {
        try {
          const snap = await fetchSnapshot({ timestamp, direction });
          setSnapshot(snap);
        } catch {
          // Fallback: fetch current snapshot without params
          try {
            const snap = await fetchSnapshot();
            setSnapshot(snap);
          } catch (e) {
            setError(
              e instanceof Error ? e.message : "Failed to load snapshot",
            );
          }
        } finally {
          setLoading(false);
        }
      }, 50);
    },
    [],
  );

  // Load first snapshot on mount
  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      try {
        const snap = await fetchSnapshot();
        if (!cancelled) setSnapshot(snap);
      } catch {
        // Retry once after 2s
        setTimeout(async () => {
          try {
            const snap = await fetchSnapshot();
            if (!cancelled) setSnapshot(snap);
          } catch {
            if (!cancelled) setError("Failed to load initial snapshot");
          }
        }, 2000);
      }
    };
    load();
    return () => {
      cancelled = true;
    };
  }, []);

  // Cleanup debounce timer
  useEffect(() => {
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, []);

  return { snapshot, loading, error, jumpToTimestamp };
}
