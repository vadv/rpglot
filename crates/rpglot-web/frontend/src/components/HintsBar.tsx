export function HintsBar({
  mode,
  detailOpen,
  hasSelection,
  hasDrillDown,
  paused,
}: {
  mode: "live" | "history";
  detailOpen: boolean;
  hasSelection: boolean;
  hasDrillDown: boolean;
  paused?: boolean;
}) {
  return (
    <div className="flex items-center gap-4 px-4 py-1 bg-[var(--bg-surface)] border-t border-[var(--border-default)] text-[11px] text-[var(--text-tertiary)]">
      <Hint keys="1-7" action="tabs" />
      <Hint keys="j/k" action="navigate" />
      {(detailOpen || hasSelection) && (
        <Hint keys="Esc" action={detailOpen ? "close detail" : "deselect"} />
      )}
      {hasSelection && hasDrillDown && <Hint keys=">" action="drill-down" />}
      <Hint keys="/" action="filter" />
      {mode === "live" && (
        <Hint keys="Space" action={paused ? "resume" : "pause"} />
      )}
      {mode === "history" && <Hint keys="←/→" action="step" />}
      {mode === "history" && <Hint keys="Shift+←/→" action="±1h" />}
      <Hint keys="?" action="help" />
    </div>
  );
}

function Hint({ keys, action }: { keys: string; action: string }) {
  return (
    <span className="flex items-center gap-1">
      <kbd className="inline-flex items-center justify-center min-w-[18px] h-[18px] px-1 bg-[var(--bg-elevated)] border border-[var(--border-default)] rounded text-[10px] font-mono text-[var(--text-secondary)]">
        {keys}
      </kbd>
      <span>{action}</span>
    </span>
  );
}
