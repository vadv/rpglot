import { useState, useEffect } from "react";
import { createPortal } from "react-dom";
import {
  X,
  ChevronDown,
  ChevronRight,
  Monitor,
  Activity,
  BarChart3,
  Table2,
  ListTree,
  AlertTriangle,
  Lock,
  Trash2,
  Network,
} from "lucide-react";
import type { TabKey } from "../api/types";
import { TAB_HELP } from "../utils/helpContent";

const TAB_ICONS: Record<TabKey, typeof Monitor> = {
  prc: Monitor,
  pga: Activity,
  pgs: BarChart3,
  pgp: Network,
  pgt: Table2,
  pgi: ListTree,
  pge: AlertTriangle,
  pgl: Lock,
  pgv: Trash2,
};

interface HelpModalProps {
  tab: TabKey;
  view: string;
  onClose: () => void;
}

export function HelpModal({ tab, view, onClose }: HelpModalProps) {
  const help = TAB_HELP[tab];
  const Icon = TAB_ICONS[tab];
  const viewHelp = help.views[view] ?? Object.values(help.views)[0];
  const viewLabel =
    Object.keys(help.views).find((k) => k === view) ??
    Object.keys(help.views)[0];

  // Capture-phase Escape — close modal without propagating to App.tsx
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.stopPropagation();
        e.preventDefault();
        onClose();
      }
    }
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [onClose]);

  return createPortal(
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={onClose}
    >
      <div
        className="relative w-full max-w-2xl max-h-[80vh] flex flex-col rounded-xl border border-[var(--border-default)] bg-[var(--bg-surface)]"
        style={{ boxShadow: "var(--shadow-lg)" }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center gap-3 px-6 py-4 border-b border-[var(--border-default)]">
          <Icon size={20} className="text-[var(--accent-text)]" />
          <div className="flex-1 min-w-0">
            <h2 className="text-sm font-semibold text-[var(--text-primary)]">
              {help.label}
            </h2>
            <span className="text-xs text-[var(--text-tertiary)] font-mono">
              {help.source}
            </span>
          </div>
          <button
            onClick={onClose}
            className="p-1 rounded text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors"
          >
            <X size={16} />
          </button>
        </div>

        {/* Body */}
        <div className="overflow-y-auto p-6 space-y-5">
          <p className="text-sm text-[var(--text-secondary)]">
            {help.description}
          </p>

          <HelpSection title="How to Read This Tab" defaultOpen>
            <p className="text-sm text-[var(--text-secondary)] leading-relaxed">
              {help.howToRead}
            </p>
          </HelpSection>

          {viewHelp && (
            <HelpSection title={`Key Metrics \u2014 ${viewLabel}`} defaultOpen>
              <p className="text-xs text-[var(--text-tertiary)] mb-3">
                {viewHelp.description}
              </p>
              <div className="grid grid-cols-[120px_1fr] gap-x-3 gap-y-2 text-xs">
                {viewHelp.metrics.map((m) => (
                  <MetricRow key={m.label} metric={m} />
                ))}
              </div>
            </HelpSection>
          )}

          <HelpSection title="Color Legend" defaultOpen>
            <div className="space-y-1.5 text-xs">
              <ColorLegendRow
                color="var(--status-critical)"
                label="critical"
                description="needs immediate attention"
              />
              <ColorLegendRow
                color="var(--status-warning)"
                label="warning"
                description="elevated, worth monitoring"
              />
              <ColorLegendRow
                color="var(--status-success)"
                label="good"
                description="healthy / normal"
              />
              <ColorLegendRow
                color="var(--status-inactive)"
                label="inactive"
                description="zero or no activity"
              />
            </div>
          </HelpSection>

          {help.drillDown && (
            <HelpSection title="Drill-down" defaultOpen>
              <p className="text-sm text-[var(--text-secondary)]">
                {help.drillDown}
              </p>
            </HelpSection>
          )}
        </div>
      </div>
    </div>,
    document.body,
  );
}

function HelpSection({
  title,
  children,
  defaultOpen = true,
}: {
  title: string;
  children: React.ReactNode;
  defaultOpen?: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div>
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-1.5 text-sm font-semibold text-[var(--text-primary)] hover:text-[var(--accent-text)] transition-colors"
      >
        {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        {title}
      </button>
      {open && <div className="mt-2 pl-5">{children}</div>}
    </div>
  );
}

function MetricRow({
  metric,
}: {
  metric: { label: string; description: string; thresholds?: string };
}) {
  return (
    <>
      <span className="font-mono font-medium text-[var(--text-primary)]">
        {metric.label}
      </span>
      <div>
        <span className="text-[var(--text-secondary)]">
          {metric.description}
        </span>
        {metric.thresholds && (
          <div className="text-[11px] text-[var(--text-tertiary)] mt-0.5">
            {metric.thresholds}
          </div>
        )}
      </div>
    </>
  );
}

function ColorLegendRow({
  color,
  label,
  description,
}: {
  color: string;
  label: string;
  description: string;
}) {
  return (
    <div className="flex items-center gap-2">
      <span
        className="w-2 h-2 rounded-full flex-shrink-0"
        style={{ backgroundColor: color }}
      />
      <span className="text-[var(--text-secondary)]">
        <strong className="text-[var(--text-primary)]">{label}</strong> &mdash;{" "}
        {description}
      </span>
    </div>
  );
}
