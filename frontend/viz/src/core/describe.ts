// @ts-nocheck
import { UNSUPPORTED } from "../server/read-only-client";

export function initDescribe(ctx) {
  const { ui } = ctx;
  ui.describeCache = ui.describeCache || new Map();
  async function fetchDescribeEntity(id) {
    // Automatic selection is local-only; no unsupported describe request.
    ui.describeCache.set(id, { status: "error", data: { error: UNSUPPORTED } });
  }
  Object.assign(ctx, { fetchDescribeEntity });
  return { fetchDescribeEntity };
}
