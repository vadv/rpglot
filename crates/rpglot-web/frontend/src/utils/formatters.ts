import type { Format, Unit } from '../api/types';

export function formatValue(value: unknown, unit?: Unit, format?: Format): string {
  if (value == null) return '-';

  const num = typeof value === 'number' ? value : Number(value);
  if (typeof value === 'boolean') return value ? 'Yes' : 'No';
  if (typeof value === 'string') return value;

  if (isNaN(num)) return String(value);

  if (format === 'bytes') {
    const base = unit === 'kb' ? num * 1024 : num;
    return formatBytes(base);
  }
  if (format === 'duration') {
    const secs = unit === 'ms' ? num / 1000 : num;
    return formatDuration(secs);
  }
  if (format === 'rate') {
    return formatRate(num);
  }
  if (format === 'percent') {
    return `${num.toFixed(1)}%`;
  }
  if (format === 'age') {
    if (num === 0) return '-';
    const now = Math.floor(Date.now() / 1000);
    const age = now - num;
    if (age < 0) return '-';
    return formatDuration(age);
  }

  // No format — use unit hints
  if (unit === 'ms') return `${num.toFixed(1)} ms`;
  if (unit === 'percent') return `${num.toFixed(1)}%`;

  if (Number.isInteger(num)) return num.toLocaleString();
  return num.toFixed(2);
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const abs = Math.abs(bytes);
  if (abs < 1024) return `${bytes.toFixed(0)} B`;
  if (abs < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  if (abs < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GiB`;
}

function formatDuration(totalSeconds: number): string {
  if (totalSeconds < 0) return '-';
  if (totalSeconds < 1) return `${(totalSeconds * 1000).toFixed(0)}ms`;
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

function formatRate(value: number): string {
  if (value === 0) return '0';
  const abs = Math.abs(value);
  if (abs < 1) return value.toFixed(2);
  if (abs < 10) return value.toFixed(1);
  if (abs < 1000) return value.toFixed(0);
  if (abs < 1_000_000) return `${(value / 1000).toFixed(1)}K`;
  if (abs < 1_000_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  return `${(value / 1_000_000_000).toFixed(1)}G`;
}
