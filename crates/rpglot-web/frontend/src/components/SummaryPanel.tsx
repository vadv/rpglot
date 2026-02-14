import {
  Cpu,
  MemoryStick,
  HardDrive,
  Network,
  Database,
  Server,
  Gauge,
  BarChart,
} from "lucide-react";
import type { ApiSnapshot, SummarySchema } from "../api/types";
import { formatValue } from "../utils/formatters";

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
};

interface SummaryPanelProps {
  snapshot: ApiSnapshot;
  schema: SummarySchema;
}

export function SummaryPanel({ snapshot, schema }: SummaryPanelProps) {
  const systemSections = schema.system
    .map((section) => ({
      section,
      data: getSystemData(snapshot, section.key),
    }))
    .filter((s) => s.data !== null);

  const pgSections = schema.pg
    .map((section) => ({
      section,
      data: getPgData(snapshot, section.key),
    }))
    .filter((s) => s.data !== null);

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
      </div>
      <div className="grid grid-cols-[auto_1fr] gap-x-2 text-xs leading-[18px]">
        {visibleFields.map((f) => (
          <KV
            key={f.key}
            label={f.label}
            value={formatValue(data[f.key], f.unit as never, f.format as never)}
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
          label="Read"
          value={formatValue(disk.read_bytes_s, "bytes/s", "bytes")}
        />
        <KV
          label="Write"
          value={formatValue(disk.write_bytes_s, "bytes/s", "bytes")}
        />
        <KV label="R IOPS" value={formatValue(disk.read_iops, "/s", "rate")} />
        <KV label="W IOPS" value={formatValue(disk.write_iops, "/s", "rate")} />
        <KV
          label="Util"
          value={formatValue(disk.util_pct, "percent", "percent")}
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
          label="RX"
          value={formatValue(net.rx_bytes_s, "bytes/s", "bytes")}
        />
        <KV
          label="TX"
          value={formatValue(net.tx_bytes_s, "bytes/s", "bytes")}
        />
        <KV
          label="RX pkt"
          value={formatValue(net.rx_packets_s, "/s", "rate")}
        />
        <KV
          label="TX pkt"
          value={formatValue(net.tx_packets_s, "/s", "rate")}
        />
      </div>
    </div>
  );
}

function KV({ label, value }: { label: string; value: string }) {
  return (
    <>
      <span className="text-[var(--text-tertiary)] whitespace-nowrap">
        {label}
      </span>
      <span className="text-[var(--text-primary)] whitespace-nowrap text-right font-mono tabular-nums">
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
