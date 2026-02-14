import { useCallback } from "react";
import type { TabKey } from "../api/types";

const TAB_KEYS: TabKey[] = ["prc", "pga", "pgs", "pgt", "pgi", "pgl"];

export interface UrlState {
  tab: TabKey;
  view: string | null;
  filter: string | null;
  position: number | null;
}

/** Read initial state from URL search params. */
export function readUrlState(): UrlState {
  const params = new URLSearchParams(window.location.search);

  const rawTab = params.get("tab");
  const tab: TabKey =
    rawTab && TAB_KEYS.includes(rawTab as TabKey) ? (rawTab as TabKey) : "pga";

  const view = params.get("view") || null;
  const filter = params.get("filter") || null;

  const rawPos = params.get("pos");
  const position = rawPos != null ? parseInt(rawPos, 10) : null;

  return {
    tab,
    view,
    filter,
    position: Number.isNaN(position) ? null : position,
  };
}

/** Update URL search params without page reload. */
export function useUrlSync() {
  const sync = useCallback((updates: Partial<UrlState>) => {
    const params = new URLSearchParams(window.location.search);

    if (updates.tab !== undefined) {
      if (updates.tab !== "pga") {
        params.set("tab", updates.tab);
      } else {
        params.delete("tab");
      }
    }

    if (updates.view !== undefined) {
      if (updates.view) {
        params.set("view", updates.view);
      } else {
        params.delete("view");
      }
    }

    if (updates.filter !== undefined) {
      if (updates.filter) {
        params.set("filter", updates.filter);
      } else {
        params.delete("filter");
      }
    }

    if (updates.position !== undefined) {
      if (updates.position != null && updates.position > 0) {
        params.set("pos", String(updates.position));
      } else {
        params.delete("pos");
      }
    }

    const search = params.toString();
    const newUrl = search
      ? `${window.location.pathname}?${search}`
      : window.location.pathname;

    window.history.replaceState(null, "", newUrl);
  }, []);

  return sync;
}
