import type { VizUiState } from "../types";

export function initDetailTabs(ctx: { ui: VizUiState; detailEl: HTMLElement }) {
  const { ui, detailEl } = ctx;

  function setActiveDetailTab(name: string): void {
    const want = String(name || "overview");
    ui.detailTab = want;
    const btns = Array.from(
      detailEl.querySelectorAll<HTMLElement>(".detailtabbtn"),
    );
    const panels = Array.from(
      detailEl.querySelectorAll<HTMLElement>(".detailtabpanel"),
    );
    for (const b of btns) b.classList.toggle("active", (b.dataset && b.dataset.dtab) === want);
    for (const p of panels) p.classList.toggle("active", (p.dataset && p.dataset.dtab) === want);
  }

  Object.assign(ctx, { setActiveDetailTab });
  return { setActiveDetailTab };
}
