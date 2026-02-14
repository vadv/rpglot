import { useEffect, useRef, useState } from 'react';
import { fetchSnapshot, subscribeSSE } from '../api/client';
import type { ApiSnapshot } from '../api/types';

export function useLiveSnapshot() {
  const [snapshot, setSnapshot] = useState<ApiSnapshot | null>(null);
  const esRef = useRef<EventSource | null>(null);

  useEffect(() => {
    const es = subscribeSSE(setSnapshot);
    esRef.current = es;
    return () => {
      es.close();
      esRef.current = null;
    };
  }, []);

  return snapshot;
}

export function useHistorySnapshot() {
  const [snapshot, setSnapshot] = useState<ApiSnapshot | null>(null);
  const [loading, setLoading] = useState(false);

  const jumpTo = async (position: number) => {
    setLoading(true);
    try {
      const snap = await fetchSnapshot({ position });
      setSnapshot(snap);
    } finally {
      setLoading(false);
    }
  };

  const jumpToTimestamp = async (timestamp: number) => {
    setLoading(true);
    try {
      const snap = await fetchSnapshot({ timestamp });
      setSnapshot(snap);
    } finally {
      setLoading(false);
    }
  };

  // Load first snapshot on mount
  useEffect(() => {
    fetchSnapshot().then(setSnapshot).catch(() => {});
  }, []);

  return { snapshot, loading, jumpTo, jumpToTimestamp };
}
