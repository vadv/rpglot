import {
  Cpu,
  MemoryStick,
  HardDrive,
  Network,
  Database,
  Server,
  Gauge,
  BarChart,
  Info,
  Container,
} from "lucide-react";
import { RichTooltip } from "./RichTooltip";
import type { ApiSnapshot, SummarySchema } from "../api/types";
import { formatValue } from "../utils/formatters";
import { getThresholdClass } from "../utils/thresholds";
import { buildColumnTooltip } from "../utils/columnHelp";
import { SUMMARY_SECTION_HELP } from "../utils/helpContent";

const SECTION_ICONS: Record<string, typeof Cpu> = {
  cpu: Cpu,
  load: Gauge,
  memory: MemoryStick,
  swap: MemoryStick,
  psi: BarChart,
  vmstat: Server,
  disk: HardDrive,
  network: Network,
  pg: Database,
  bgwriter: Database,
  cgroup_cpu: Container,
  cgroup_memory: Container,
  cgroup_pids: Container,
};

interface SummaryPanelProps {
  snapshot: ApiSnapshot;
  schema: SummarySchema;
}

export function SummaryPanel({ snapshot, schema }: SummaryPanelProps) {
  const hasCgroupCpu = snapshot.system.cgroup_cpu != null;
  const hasCgroupMemory = snapshot.system.cgroup_memory != null;

  const systemSections = schema.system
    .map((section) => ({
      section,
      data: getSystemData(snapshot, section.key),
    }))
    .filter((s) => s.data != null)
    .filter(({ section }) => {
      // In container mode, replace host CPU/Memory/Swap with cgroup equivalents
      if (hasCgroupCpu && section.key === "cpu") return false;
      if (hasCgroupMemory && section.key === "memory") return false;
      if (hasCgroupMemory && section.key === "swap") return false;
      return true;
    });

  const pgSections = schema.pg
    .map((section) => ({
      section,
      data: getPgData(snapshot, section.key),
    }))
    .filter((s) => s.data != null);

  const disks = snapshot.system.disks;
  const networks = snapshot.system.networks;

  return (
    <div className="px-3 py-2 bg-[var(--bg-surface)] border-b border-[var(--border-default)]">
      <div className="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-x-4 gap-y-2">
        {systemSections.map(({ section, data }) => {
          if (section.key === "disk" || section.key === "network") return null;
          return (
            <SummaryCard
              key={section.key}
              sectionKey={section.key}
              label={section.label}
              fields={section.fields}
              data={data!}
            />
          );
        })}

        {disks.length > 0 &&
          disks.map((disk) => <DiskCard key={disk.name} disk={disk} />)}

        {networks.length > 0 &&
          networks.map((net) => <NetworkCard key={net.name} net={net} />)}

        {pgSections.map(({ section, data }) => (
          <SummaryCard
            key={section.key}
            sectionKey={section.key}
            label={section.label}
            fields={section.fields}
            data={data!}
            accent
          />
        ))}
      </div>
    </div>
  );
}

function SummaryCard({
  sectionKey,
  label,
  fields,
  data,
  accent,
}: {
  sectionKey: string;
  label: string;
  fields: {
    key: string;
    label: string;
    unit?: string;
    format?: string;
  }[];
  data: Record<string, unknown>;
  accent?: boolean;
}) {
  const visibleFields = fields.filter((f) => data[f.key] != null);
  if (visibleFields.length === 0) return null;

  const Icon = SECTION_ICONS[sectionKey] ?? Server;

  return (
    <div
      className="min-w-0 p-2 rounded-lg border border-[var(--border-default)] bg-[var(--bg-base)]"
      style={{ boxShadow: "var(--shadow-sm)" }}
    >
      <div className="flex items-center gap-1.5 mb-1">
        <Icon
          size={12}
          className={
            accent ? "text-[var(--accent-text)]" : "text-[var(--text-tertiary)]"
          }
        />
        <span
          className={`text-[10px] font-semibold uppercase tracking-wider ${
            accent ? "text-[var(--accent-text)]" : "text-[var(--text-tertiary)]"
          }`}
        >
          {label}
        </span>
        {SUMMARY_SECTION_HELP[sectionKey] && (
          <RichTooltip
            content={
              <span className="text-[var(--text-secondary)]">
                {SUMMARY_SECTION_HELP[sectionKey]}
              </span>
            }
            side="bottom"
          >
            <Info
              size={10}
              className="text-[var(--text-tertiary)] opacity-50 hover:opacity-100 cursor-help"
            />
          </RichTooltip>
        )}
      </div>
      <div className="grid grid-cols-[auto_1fr] gap-x-2 text-xs leading-[18px]">
        {visibleFields.map((f) => (
          <KV
            key={f.key}
            sectionKey={sectionKey}
            fieldKey={f.key}
            label={f.label}
            value={formatValue(data[f.key], f.unit as never, f.format as never)}
            rawValue={data[f.key]}
          />
        ))}
      </div>
    </div>
  );
}

function DiskCard({ disk }: { disk: ApiSnapshot["system"]["disks"][number] }) {
  return (
    <div
      className="min-w-0 p-2 rounded-lg border border-[var(--border-default)] bg-[var(--bg-base)]"
      style={{ boxShadow: "var(--shadow-sm)" }}
    >
      <div className="flex items-center gap-1.5 mb-1">
        <HardDrive size={12} className="text-[var(--text-tertiary)]" />
        <span className="text-[10px] font-semibold uppercase tracking-wider text-[var(--text-tertiary)]">
          Disk: {disk.name}
        </span>
      </div>
      <div className="grid grid-cols-[auto_1fr] gap-x-2 text-xs leading-[18px]">
        <KV
          sectionKey="disk"
          fieldKey="read_bytes_s"
          label="Read"
          value={formatValue(disk.read_bytes_s, "bytes/s", "bytes")}
          rawValue={disk.read_bytes_s}
        />
        <KV
          sectionKey="disk"
          fieldKey="write_bytes_s"
          label="Write"
          value={formatValue(disk.write_bytes_s, "bytes/s", "bytes")}
          rawValue={disk.write_bytes_s}
        />
        <KV
          sectionKey="disk"
          fieldKey="read_iops"
          label="R IOPS"
          value={formatValue(disk.read_iops, "/s", "rate")}
          rawValue={disk.read_iops}
        />
        <KV
          sectionKey="disk"
          fieldKey="write_iops"
          label="W IOPS"
          value={formatValue(disk.write_iops, "/s", "rate")}
          rawValue={disk.write_iops}
        />
        <KV
          sectionKey="disk"
          fieldKey="util_pct"
          label="Util"
          value={formatValue(disk.util_pct, "percent", "percent")}
          rawValue={disk.util_pct}
        />
      </div>
    </div>
  );
}

function NetworkCard({
  net,
}: {
  net: ApiSnapshot["system"]["networks"][number];
}) {
  return (
    <div
      className="min-w-0 p-2 rounded-lg border border-[var(--border-default)] bg-[var(--bg-base)]"
      style={{ boxShadow: "var(--shadow-sm)" }}
    >
      <div className="flex items-center gap-1.5 mb-1">
        <Network size={12} className="text-[var(--text-tertiary)]" />
        <span className="text-[10px] font-semibold uppercase tracking-wider text-[var(--text-tertiary)]">
          Net: {net.name}
        </span>
      </div>
      <div className="grid grid-cols-[auto_1fr] gap-x-2 text-xs leading-[18px]">
        <KV
          sectionKey="network"
          fieldKey="rx_bytes_s"
          label="RX"
          value={formatValue(net.rx_bytes_s, "bytes/s", "bytes")}
          rawValue={net.rx_bytes_s}
        />
        <KV
          sectionKey="network"
          fieldKey="tx_bytes_s"
          label="TX"
          value={formatValue(net.tx_bytes_s, "bytes/s", "bytes")}
          rawValue={net.tx_bytes_s}
        />
        <KV
          sectionKey="network"
          fieldKey="rx_packets_s"
          label="RX pkt"
          value={formatValue(net.rx_packets_s, "/s", "rate")}
          rawValue={net.rx_packets_s}
        />
        <KV
          sectionKey="network"
          fieldKey="tx_packets_s"
          label="TX pkt"
          value={formatValue(net.tx_packets_s, "/s", "rate")}
          rawValue={net.tx_packets_s}
        />
      </div>
    </div>
  );
}

function KV({
  sectionKey,
  fieldKey,
  label,
  value,
  rawValue,
}: {
  sectionKey?: string;
  fieldKey?: string;
  label: string;
  value: string;
  rawValue?: unknown;
}) {
  // Threshold coloring: try qualified key first, then bare key
  const qKey = sectionKey && fieldKey ? `${sectionKey}.${fieldKey}` : "";
  const colorClass =
    (qKey && getThresholdClass(qKey, rawValue, {})) ||
    (fieldKey && getThresholdClass(fieldKey, rawValue, {})) ||
    "";

  // Rich tooltip on label
  const tooltip =
    (qKey && buildColumnTooltip(qKey)) ||
    (fieldKey && buildColumnTooltip(fieldKey)) ||
    null;

  const labelEl = (
    <span className="text-[var(--text-tertiary)] whitespace-nowrap">
      {label}
    </span>
  );

  return (
    <>
      {tooltip ? (
        <RichTooltip content={tooltip} side="bottom">
          {labelEl}
        </RichTooltip>
      ) : (
        labelEl
      )}
      <span
        className={`whitespace-nowrap text-right font-mono tabular-nums ${colorClass || "text-[var(--text-primary)]"}`}
      >
        {value}
      </span>
    </>
  );
}

function getSystemData(
  snap: ApiSnapshot,
  key: string,
): Record<string, unknown> | null {
  const s = snap.system;
  switch (key) {
    case "cpu":
      return s.cpu as unknown as Record<string, unknown>;
    case "load":
      return s.load as unknown as Record<string, unknown>;
    case "memory":
      return s.memory as unknown as Record<string, unknown>;
    case "swap":
      return s.swap as unknown as Record<string, unknown>;
    case "psi":
      return s.psi as unknown as Record<string, unknown>;
    case "vmstat":
      return s.vmstat as unknown as Record<string, unknown>;
    case "cgroup_cpu":
      return s.cgroup_cpu as unknown as Record<string, unknown>;
    case "cgroup_memory":
      return s.cgroup_memory as unknown as Record<string, unknown>;
    case "cgroup_pids":
      return s.cgroup_pids as unknown as Record<string, unknown>;
    default:
      return null;
  }
}

function getPgData(
  snap: ApiSnapshot,
  key: string,
): Record<string, unknown> | null {
  const pg = snap.pg;
  switch (key) {
    case "pg":
      return pg as unknown as Record<string, unknown>;
    case "bgwriter":
      return pg.bgwriter as unknown as Record<string, unknown>;
    default:
      return null;
  }
}
