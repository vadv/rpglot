import type { Format, Unit } from "../api/types";

export type TimezoneMode = "local" | "utc" | "moscow";

const TZ_LABELS: Record<TimezoneMode, string> = {
  local: "",
  utc: " UTC",
  moscow: " MSK",
};

function tzOption(tz: TimezoneMode): string | undefined {
  switch (tz) {
    case "utc":
      return "UTC";
    case "moscow":
      return "Europe/Moscow";
    default:
      return undefined;
  }
}

export function formatTimestamp(
  epochSeconds: number,
  tz: TimezoneMode,
): string {
  if (epochSeconds <= 0) return "-";
  const date = new Date(epochSeconds * 1000);
  const options: Intl.DateTimeFormatOptions = {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
    timeZone: tzOption(tz),
  };
  return date.toLocaleString("en-GB", options) + TZ_LABELS[tz];
}

export function formatTime(epochSeconds: number, tz: TimezoneMode): string {
  if (epochSeconds <= 0) return "-";
  const date = new Date(epochSeconds * 1000);
  const options: Intl.DateTimeFormatOptions = {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
    timeZone: tzOption(tz),
  };
  return date.toLocaleString("en-GB", options);
}

export function formatValue(
  value: unknown,
  unit?: Unit,
  format?: Format,
  referenceTimestamp?: number,
): string {
  if (value == null) return "-";

  const num = typeof value === "number" ? value : Number(value);
  if (typeof value === "boolean") return value ? "Yes" : "No";
  if (typeof value === "string") return value;

  if (isNaN(num)) return String(value);

  if (format === "bytes") {
    let base: number;
    if (unit === "kb") base = num * 1024;
    else if (unit === "buffers" || unit === "blks/s") base = num * 8192;
    else if (unit === "bytes/s") base = num;
    else base = num;
    const suffix = unit === "blks/s" || unit === "bytes/s" ? "/s" : "";
    return formatBytes(base) + suffix;
  }
  if (format === "duration") {
    const secs = unit === "ms" ? num / 1000 : num;
    return formatDuration(secs);
  }
  if (format === "rate") {
    return formatRate(num);
  }
  if (format === "percent") {
    return `${num.toFixed(1)}%`;
  }
  if (format === "age") {
    if (num === 0) return "-";
    const now = referenceTimestamp ?? Math.floor(Date.now() / 1000);
    const age = now - num;
    if (age < 0) return "-";
    return formatDuration(age);
  }

  // No format — use unit hints
  if (unit === "ms") return `${num.toFixed(1)} ms`;
  if (unit === "percent") return `${num.toFixed(1)}%`;

  if (Number.isInteger(num)) return String(num);
  return num.toFixed(2);
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const abs = Math.abs(bytes);
  if (abs < 1024) return `${bytes.toFixed(0)} B`;
  if (abs < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  if (abs < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GiB`;
}

function formatDuration(totalSeconds: number): string {
  if (totalSeconds < 0) return "-";
  if (totalSeconds === 0) return "0s";
  const ms = totalSeconds * 1000;
  if (ms < 1) return `${ms.toFixed(1)}ms`;
  if (totalSeconds < 1) return `${ms.toFixed(0)}ms`;
  if (totalSeconds < 60) return `${totalSeconds.toFixed(1)}s`;
  if (totalSeconds < 3600) {
    const m = Math.floor(totalSeconds / 60);
    const s = Math.floor(totalSeconds % 60);
    return s > 0 ? `${m}m ${s}s` : `${m}m`;
  }
  if (totalSeconds < 86400) {
    const h = Math.floor(totalSeconds / 3600);
    const m = Math.floor((totalSeconds % 3600) / 60);
    return m > 0 ? `${h}h ${m}m` : `${h}h`;
  }
  const d = Math.floor(totalSeconds / 86400);
  const h = Math.floor((totalSeconds % 86400) / 3600);
  return h > 0 ? `${d}d ${h}h` : `${d}d`;
}

/** Decompose epoch into date/time parts in the given timezone. */
export function getDatePartsInTz(
  epochSeconds: number,
  tz: TimezoneMode,
): {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
  second: number;
} {
  const date = new Date(epochSeconds * 1000);
  const fmt = new Intl.DateTimeFormat("en-US", {
    year: "numeric",
    month: "numeric",
    day: "numeric",
    hour: "numeric",
    minute: "numeric",
    second: "numeric",
    hour12: false,
    timeZone: tzOption(tz),
  });
  const parts = fmt.formatToParts(date);
  const get = (type: string) =>
    Number(parts.find((p) => p.type === type)?.value ?? 0);
  return {
    year: get("year"),
    month: get("month"),
    day: get("day"),
    hour: get("hour") === 24 ? 0 : get("hour"),
    minute: get("minute"),
    second: get("second"),
  };
}

/** Compose date/time parts back to epoch seconds in the given timezone. */
export function dateToEpochInTz(
  year: number,
  month: number,
  day: number,
  hour: number,
  minute: number,
  second: number,
  tz: TimezoneMode,
): number {
  if (tz === "utc") {
    return Math.floor(
      Date.UTC(year, month - 1, day, hour, minute, second) / 1000,
    );
  }
  if (tz === "moscow") {
    // Moscow is UTC+3 (fixed, no DST since 2014)
    return Math.floor(
      (Date.UTC(year, month - 1, day, hour, minute, second) - 3 * 3600 * 1000) /
        1000,
    );
  }
  // local
  return Math.floor(
    new Date(year, month - 1, day, hour, minute, second).getTime() / 1000,
  );
}

/** Format epoch as "YYYY-MM-DD" in the given timezone. */
export function formatDate(epochSeconds: number, tz: TimezoneMode): string {
  if (epochSeconds <= 0) return "-";
  const p = getDatePartsInTz(epochSeconds, tz);
  return `${p.year}-${String(p.month).padStart(2, "0")}-${String(p.day).padStart(2, "0")}`;
}

function formatRate(value: number): string {
  if (value === 0) return "0";
  const abs = Math.abs(value);
  if (abs < 1) return value.toFixed(2);
  if (abs < 10) return value.toFixed(1);
  if (abs < 1000) return value.toFixed(0);
  if (abs < 1_000_000) return `${(value / 1000).toFixed(1)}K`;
  if (abs < 1_000_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  return `${(value / 1_000_000_000).toFixed(1)}G`;
}
