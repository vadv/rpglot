import { useState, useEffect, useCallback, useRef } from "react";
import type { TimezoneMode } from "../utils/formatters";
import { formatTime, getDatePartsInTz, dateToEpochInTz } from "../utils/formatters";

export function TimeInput({
  timestamp,
  timezone,
  onSubmit,
}: {
  timestamp: number;
  timezone: TimezoneMode;
  onSubmit: (epoch: number) => void;
}) {
  const displayTime = formatTime(timestamp, timezone);
  const [value, setValue] = useState(displayTime);
  const [editing, setEditing] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  // Sync display when timestamp changes (and not editing)
  useEffect(() => {
    if (!editing) {
      setValue(formatTime(timestamp, timezone));
    }
  }, [timestamp, timezone, editing]);

  const handleFocus = useCallback(() => {
    setEditing(true);
    setValue(formatTime(timestamp, timezone));
  }, [timestamp, timezone]);

  const handleBlur = useCallback(() => {
    setEditing(false);
    setValue(formatTime(timestamp, timezone));
  }, [timestamp, timezone]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        const parsed = parseTimeString(value);
        if (parsed) {
          const parts = getDatePartsInTz(timestamp, timezone);
          const epoch = dateToEpochInTz(
            parts.year,
            parts.month,
            parts.day,
            parsed.hour,
            parsed.minute,
            parsed.second,
            timezone,
          );
          onSubmit(epoch);
        }
        setEditing(false);
        inputRef.current?.blur();
      } else if (e.key === "Escape") {
        e.preventDefault();
        setEditing(false);
        setValue(formatTime(timestamp, timezone));
        inputRef.current?.blur();
      }
    },
    [value, timestamp, timezone, onSubmit],
  );

  return (
    <input
      ref={inputRef}
      type="text"
      value={value}
      onChange={(e) => setValue(e.target.value)}
      onFocus={handleFocus}
      onBlur={handleBlur}
      onKeyDown={handleKeyDown}
      className="w-[68px] font-mono text-xs text-center py-0.5 bg-transparent text-[var(--text-primary)] border-b border-[var(--border-default)] focus:border-[var(--accent)] focus:outline-none transition-colors"
      title="Type HH:MM:SS and press Enter to jump"
    />
  );
}

function parseTimeString(
  s: string,
): { hour: number; minute: number; second: number } | null {
  const m = s.trim().match(/^(\d{1,2}):(\d{2}):(\d{2})$/);
  if (!m) return null;
  const hour = Number(m[1]);
  const minute = Number(m[2]);
  const second = Number(m[3]);
  if (hour > 23 || minute > 59 || second > 59) return null;
  return { hour, minute, second };
}
