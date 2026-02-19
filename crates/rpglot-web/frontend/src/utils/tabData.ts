import type { ApiSnapshot, TabKey } from "../api/types";

export function getTabData(
  snapshot: ApiSnapshot,
  tab: TabKey,
): Record<string, unknown>[] {
  switch (tab) {
    case "prc":
      return snapshot.prc as unknown as Record<string, unknown>[];
    case "pga":
      return snapshot.pga as unknown as Record<string, unknown>[];
    case "pgs":
      return snapshot.pgs as unknown as Record<string, unknown>[];
    case "pgt":
      return snapshot.pgt as unknown as Record<string, unknown>[];
    case "pgi":
      return snapshot.pgi as unknown as Record<string, unknown>[];
    case "pge":
      return snapshot.pge as unknown as Record<string, unknown>[];
    case "pgl":
      return snapshot.pgl as unknown as Record<string, unknown>[];
    case "pgv":
      return snapshot.pgv as unknown as Record<string, unknown>[];
  }
}
