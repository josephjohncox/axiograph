// @ts-nocheck

export function initDetailTabs(ctx) {
  const { ui, detailEl } = ctx;

  function setActiveDetailTab(name) {
    const want = String(name || "overview");
    ui.detailTab = want;
    const btns = Array.from(detailEl.querySelectorAll(".detailtabbtn"));
    const panels = Array.from(detailEl.querySelectorAll(".detailtabpanel"));
    for (const b of btns) b.classList.toggle("active", (b.dataset && b.dataset.dtab) === want);
    for (const p of panels) p.classList.toggle("active", (p.dataset && p.dataset.dtab) === want);
  }

  Object.assign(ctx, { setActiveDetailTab });
  return { setActiveDetailTab };
}
