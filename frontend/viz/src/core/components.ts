// @ts-nocheck

export function initComponents(ctx) {
  const { graph, ui, componentJumpBtn, componentStatusEl, selectedIdRef, selectNode, rerender } = ctx;

  function computeComponents() {
    const nodes = Array.isArray(graph.nodes) ? graph.nodes : [];
    const edges = Array.isArray(graph.edges) ? graph.edges : [];
    if (!nodes.length) return null;
    if (nodes.length > 80000 || edges.length > 250000) return null;
    const adj = new Map();
    for (const n of nodes) adj.set(n.id, []);
    for (const e of edges) {
      if (!adj.has(e.source) || !adj.has(e.target)) continue;
      adj.get(e.source).push(e.target);
      adj.get(e.target).push(e.source);
    }
    const visited = new Set();
    const components = [];
    const componentByNode = new Map();
    for (const n of nodes) {
      if (visited.has(n.id)) continue;
      const comp = [];
      const stack = [n.id];
      visited.add(n.id);
      while (stack.length) {
        const cur = stack.pop();
        comp.push(cur);
        const nbrs = adj.get(cur) || [];
        for (const nb of nbrs) {
          if (visited.has(nb)) continue;
          visited.add(nb);
          stack.push(nb);
        }
      }
      for (const id of comp) componentByNode.set(id, components.length);
      components.push(comp);
    }
    components.sort((a, b) => b.length - a.length);
    return { components, componentByNode };
  }

  function pickRandomComponentNode(excludeNodeId) {
    if (!ui.components || !ui.components.length) return null;
    const excludeIdx = (excludeNodeId != null && ui.componentByNode)
      ? ui.componentByNode.get(excludeNodeId)
      : null;
    const candidates = [];
    for (let i = 0; i < ui.components.length; i++) {
      if (excludeIdx != null && i === excludeIdx) continue;
      candidates.push(i);
    }
    const pickFrom = candidates.length ? candidates : ui.components.map((_, i) => i);
    if (!pickFrom.length) return null;
    const compIdx = pickFrom[Math.floor(Math.random() * pickFrom.length)];
    const comp = ui.components[compIdx];
    if (!comp || !comp.length) return null;
    return comp[Math.floor(Math.random() * comp.length)];
  }

  const comps = computeComponents();
  if (comps) {
    ui.components = comps.components;
    ui.componentByNode = comps.componentByNode;
  }

  function updateComponentButton() {
    if (!componentJumpBtn || !componentStatusEl) return;
    const count = ui.components ? ui.components.length : 0;
    componentJumpBtn.disabled = count < 2;
    componentJumpBtn.textContent = count > 1 ? `other component (${count})` : "other component";
    componentStatusEl.textContent = count > 1 ? "jump to another connected component" : "single component";
  }

  if (componentJumpBtn) {
    componentJumpBtn.addEventListener("click", () => {
      let focus = null;
      const selectedId = selectedIdRef ? selectedIdRef() : null;
      if (selectedId != null) focus = selectedId;
      const nextId = pickRandomComponentNode(focus);
      if (nextId == null) return;
      if (selectNode) selectNode(nextId, false);
      rerender();
    });
    updateComponentButton();
  }

  Object.assign(ctx, { pickRandomComponentNode });
  return { pickRandomComponentNode };
}
