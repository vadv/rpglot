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
