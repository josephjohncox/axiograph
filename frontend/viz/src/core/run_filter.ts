// @ts-nocheck

export function initRunFilter(ctx) {
  const {
    graph,
    ui,
    runFilterEl,
    runOnlyEl,
    runClearBtn,
    runIdForNode,
    selectNode,
    rerender,
  } = ctx;

  function setActiveRun(runId) {
    ui.activeRunId = runId || null;
    if (!ui.activeRunId) {
      ui.highlightIds = new Set();
    } else {
      const nodes = ui.runMap.get(ui.activeRunId) || [];
      ui.highlightIds = new Set(nodes);
      if (nodes.length && selectNode) selectNode(nodes[0], false);
    }
    rerender();
  }

  function rebuildRunFilter() {
    if (!runFilterEl) return;
    runFilterEl.innerHTML = "";
    ui.runMap = new Map();
    for (const n of (graph.nodes || [])) {
      const rid = runIdForNode(n);
      if (!rid) continue;
      if (!ui.runMap.has(rid)) ui.runMap.set(rid, []);
      ui.runMap.get(rid).push(n.id);
    }
    const runs = Array.from(ui.runMap.keys());
    if (!runs.length) {
      const opt = document.createElement("option");
      opt.value = "";
      opt.textContent = "(no runs)";
      runFilterEl.appendChild(opt);
      runFilterEl.disabled = true;
      if (runOnlyEl) runOnlyEl.disabled = true;
      if (runClearBtn) runClearBtn.disabled = true;
      return;
    }
    runs.sort((a, b) => String(a).localeCompare(String(b)));
    const allOpt = document.createElement("option");
    allOpt.value = "";
    allOpt.textContent = "(all runs)";
    runFilterEl.appendChild(allOpt);
    for (const rid of runs) {
      const opt = document.createElement("option");
      opt.value = rid;
      const count = (ui.runMap.get(rid) || []).length;
      opt.textContent = count ? `${rid} (${count})` : rid;
      runFilterEl.appendChild(opt);
    }
    runFilterEl.disabled = false;
    if (runOnlyEl) runOnlyEl.disabled = false;
    if (runClearBtn) runClearBtn.disabled = false;

    runFilterEl.addEventListener("change", () => {
      const v = String(runFilterEl.value || "");
      setActiveRun(v);
    });
    if (runClearBtn) {
      runClearBtn.addEventListener("click", () => {
        runFilterEl.value = "";
        setActiveRun(null);
      });
    }
  }

  rebuildRunFilter();

  Object.assign(ctx, { setActiveRun });
  return { setActiveRun };
}
