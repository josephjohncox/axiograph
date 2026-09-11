import { UNSUPPORTED } from "../server/read-only-client";

import type { DescribeEntry, VizUiState } from "../types";

export function initDescribe(ctx: {
  ui: VizUiState;
  isServerMode: () => boolean;
  renderDetail?: (id: number) => void;
}) {
  const { ui } = ctx;
  const describeCache = ui.describeCache ?? new Map<number, DescribeEntry>();
  ui.describeCache = describeCache;
  async function fetchDescribeEntity(id: number): Promise<void> {
    // Automatic selection is local-only; no unsupported describe request.
    describeCache.set(id, { status: "error", data: { error: UNSUPPORTED } });
  }
  Object.assign(ctx, { fetchDescribeEntity });
  return { fetchDescribeEntity };
}
