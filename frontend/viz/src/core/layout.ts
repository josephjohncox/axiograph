export function initLayoutControls(ctx) {
  const { ui, rerender, layoutAlgoEl, layoutCenterEl, layoutRefreshBtn, labelDensityEl } = ctx;
  function loadLayoutPrefs() {
    try {
      const a = localStorage.getItem("axiograph_viz_layout_algo");
      const c = localStorage.getItem("axiograph_viz_layout_center");
      if (a) ui.layoutAlgo = a;
      if (c) ui.layoutCenter = c;
    } catch (_e) {}
    if (layoutAlgoEl) layoutAlgoEl.value = ui.layoutAlgo || "radial";
    if (layoutCenterEl) layoutCenterEl.value = ui.layoutCenter || "focus";
  }

  function saveLayoutPrefs() {
    try {
      localStorage.setItem("axiograph_viz_layout_algo", ui.layoutAlgo || "radial");
      localStorage.setItem("axiograph_viz_layout_center", ui.layoutCenter || "focus");
    } catch (_e) {}
  }

  loadLayoutPrefs();
  if (layoutAlgoEl) layoutAlgoEl.addEventListener("change", () => {
    ui.layoutAlgo = String(layoutAlgoEl.value || "radial");
    saveLayoutPrefs();
    updatePresetButtons();
    rerender();
  });
  if (layoutCenterEl) layoutCenterEl.addEventListener("change", () => {
    ui.layoutCenter = String(layoutCenterEl.value || "focus");
    saveLayoutPrefs();
    rerender();
  });
  if (layoutRefreshBtn) layoutRefreshBtn.addEventListener("click", () => {
    ui.layoutSeed = (Number(ui.layoutSeed || 0) + 1) >>> 0;
    rerender();
  });

  function updatePresetButtons() {
    const presetBtns = Array.from(document.querySelectorAll("[data-layout-preset]"));
    for (const btn of presetBtns) {
      const val = btn.getAttribute("data-layout-preset") || "";
      btn.classList.toggle("active", val === ui.layoutAlgo);
    }
  }

  const presetBtns = Array.from(document.querySelectorAll("[data-layout-preset]"));
  for (const btn of presetBtns) {
    btn.addEventListener("click", () => {
      const val = btn.getAttribute("data-layout-preset") || "";
      if (!val) return;
      if (layoutAlgoEl) layoutAlgoEl.value = val;
      ui.layoutAlgo = val;
      saveLayoutPrefs();
      updatePresetButtons();
      rerender();
    });
  }
  updatePresetButtons();

  if (labelDensityEl) {
    try {
      const v = localStorage.getItem("axiograph_viz_label_density");
      if (v) labelDensityEl.value = v;
    } catch (_e) {}
    labelDensityEl.addEventListener("change", () => {
      try { localStorage.setItem("axiograph_viz_label_density", String(labelDensityEl.value || "")); } catch (_e) {}
      rerender();
    });
  }
}
