import { useState, useCallback } from "react";
import type { TimezoneMode } from "../utils/formatters";

const STORAGE_KEY = "rpglot-timezone";
const MODES: TimezoneMode[] = ["local", "utc", "moscow"];

export function useTimezone() {
  const [timezone, setTimezone] = useState<TimezoneMode>(() => {
    const stored = localStorage.getItem(STORAGE_KEY);
    return MODES.includes(stored as TimezoneMode)
      ? (stored as TimezoneMode)
      : "local";
  });

  const cycle = useCallback(() => {
    setTimezone((prev) => {
      const idx = MODES.indexOf(prev);
      const next = MODES[(idx + 1) % MODES.length];
      localStorage.setItem(STORAGE_KEY, next);
      return next;
    });
  }, []);

  return { timezone, cycle };
}
