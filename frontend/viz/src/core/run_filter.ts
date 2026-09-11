import type { GraphNode, GraphPayload, VizUiState } from "../types";

interface RunFilterContext {
  graph: GraphPayload;
  ui: VizUiState;
  runFilterEl: HTMLSelectElement;
  runOnlyEl: HTMLInputElement;
  runClearBtn: HTMLButtonElement;
  runIdForNode: (node: GraphNode) => string;
  selectNode: (id: number, shiftKey: boolean) => void;
  rerender: () => void;
}

export function initRunFilter(ctx: RunFilterContext) {
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

  function setActiveRun(runId: string | null): void {
    ui.activeRunId = runId || null;
    if (!ui.activeRunId) {
      ui.highlightIds = new Set<number>();
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
    ui.runMap = new Map<string, number[]>();
    for (const n of (graph.nodes || [])) {
      const rid = runIdForNode(n);
      if (!rid) continue;
      const nodes = ui.runMap.get(rid) ?? [];
      nodes.push(n.id);
      ui.runMap.set(rid, nodes);
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
