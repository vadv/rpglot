import { useCallback, useEffect, useRef, useState } from "react";
import { fetchSnapshot, subscribeSSE } from "../api/client";
import type { ApiSnapshot } from "../api/types";

export function useLiveSnapshot() {
  const [snapshot, setSnapshot] = useState<ApiSnapshot | null>(null);
  const [paused, setPaused] = useState(false);
  const pausedRef = useRef(false);
  const esRef = useRef<EventSource | null>(null);

  useEffect(() => {
    const es = subscribeSSE((snap) => {
      if (!pausedRef.current) {
        setSnapshot(snap);
      }
    });
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
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const jumpTo = useCallback((position: number) => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    setLoading(true);
    debounceRef.current = setTimeout(async () => {
      try {
        const snap = await fetchSnapshot({ position });
        setSnapshot(snap);
      } finally {
        setLoading(false);
      }
    }, 50);
  }, []);

  const jumpToTimestamp = useCallback((timestamp: number) => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    setLoading(true);
    debounceRef.current = setTimeout(async () => {
      try {
        const snap = await fetchSnapshot({ timestamp });
        setSnapshot(snap);
      } finally {
        setLoading(false);
      }
    }, 50);
  }, []);

  // Load first snapshot on mount
  useEffect(() => {
    fetchSnapshot()
      .then(setSnapshot)
      .catch(() => {});
  }, []);

  // Cleanup debounce timer
  useEffect(() => {
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, []);

  return { snapshot, loading, jumpTo, jumpToTimestamp };
}
