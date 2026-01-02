// @ts-nocheck

export function initDescribe(ctx) {
  const { ui, isServerMode, renderDetail, selectedIdRef } = ctx;
  ui.describeCache = ui.describeCache || new Map();

  async function fetchDescribeEntity(id) {
    if (!isServerMode()) return;
    if (!Number.isFinite(id) || id <= 0) return;
    const cached = ui.describeCache.get(id);
    if (cached && (cached.status === "ok" || cached.status === "loading")) return;
    ui.describeCache.set(id, { status: "loading", data: null });
    if (selectedIdRef && selectedIdRef() === id) renderDetail(id);
    try {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 8000);
      const resp = await fetch(`/entity/describe?id=${encodeURIComponent(String(id))}`, {
        cache: "no-store",
        signal: controller.signal,
      });
      clearTimeout(timer);
      const data = await resp.json();
      if (!resp.ok) {
        ui.describeCache.set(id, { status: "error", data });
      } else {
        ui.describeCache.set(id, { status: "ok", data });
      }
    } catch (e) {
      ui.describeCache.set(id, { status: "error", data: { error: String(e && e.name === "AbortError" ? "timeout" : e) } });
    }
    if (selectedIdRef && selectedIdRef() === id) renderDetail(id);
  }

  Object.assign(ctx, { fetchDescribeEntity });
  return { fetchDescribeEntity };
}
