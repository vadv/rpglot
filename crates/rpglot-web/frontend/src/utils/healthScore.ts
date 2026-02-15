import type { ApiSnapshot } from "../api/types";

export interface HealthPenalty {
  label: string;
  value: number; // negative
}

export interface HealthResult {
  score: number; // 0..100
  penalties: HealthPenalty[];
}

/**
 * Compute database health score from snapshot metrics.
 *
 * Starts at 100 and subtracts penalties:
 * - Active PGA sessions: -1 per 2 active sessions
 * - CPU usage above 60%: -1 per percent above 60
 * - Disk IOPS: -5 per 1000 total IOPS
 * - Disk bandwidth: -5 per 50 MB/s total throughput
 */
export function computeHealthScore(snapshot: ApiSnapshot): HealthResult {
  const penalties: HealthPenalty[] = [];
  let score = 100;

  // 1. Active PGA sessions: -1 per 2 active backends
  const activeCount = snapshot.pga.filter(
    (r) => r.state === "active",
  ).length;
  if (activeCount > 0) {
    const penalty = -Math.floor(activeCount / 2);
    if (penalty < 0) {
      penalties.push({
        label: `${activeCount} active sessions`,
        value: penalty,
      });
      score += penalty;
    }
  }

  // 2. CPU usage above 60%: -1 per percent above 60
  const cpu = snapshot.system.cpu;
  if (cpu) {
    const usedPct = cpu.sys_pct + cpu.usr_pct + cpu.irq_pct + cpu.iow_pct + cpu.steal_pct;
    if (usedPct > 60) {
      const penalty = -Math.round(usedPct - 60);
      penalties.push({
        label: `CPU ${Math.round(usedPct)}%`,
        value: penalty,
      });
      score += penalty;
    }
  }

  // 3. Total disk IOPS: -5 per 1000 IOPS
  const disks = snapshot.system.disks;
  if (disks.length > 0) {
    const totalIops = disks.reduce(
      (sum, d) => sum + d.read_iops + d.write_iops,
      0,
    );
    if (totalIops > 0) {
      const penalty = -Math.floor(totalIops / 1000) * 5;
      if (penalty < 0) {
        penalties.push({
          label: `${Math.round(totalIops)} IOPS`,
          value: penalty,
        });
        score += penalty;
      }
    }
  }

  // 4. Total disk bandwidth: -5 per 50 MB/s
  if (disks.length > 0) {
    const totalBwBytesS = disks.reduce(
      (sum, d) => sum + d.read_bytes_s + d.write_bytes_s,
      0,
    );
    const totalBwMBs = totalBwBytesS / (1024 * 1024);
    if (totalBwMBs >= 50) {
      const penalty = -Math.floor(totalBwMBs / 50) * 5;
      if (penalty < 0) {
        penalties.push({
          label: `Disk ${Math.round(totalBwMBs)} MB/s`,
          value: penalty,
        });
        score += penalty;
      }
    }
  }

  return { score: Math.max(0, Math.min(100, score)), penalties };
}

/** Map score to a color CSS variable name. */
export function healthColor(score: number): string {
  if (score >= 80) return "var(--status-success)";
  if (score >= 50) return "var(--status-warning)";
  return "var(--status-critical)";
}

/** Map score to a background CSS variable name. */
export function healthBgColor(score: number): string {
  if (score >= 80) return "var(--status-success-bg)";
  if (score >= 50) return "var(--status-warning-bg)";
  return "var(--status-critical-bg)";
}
