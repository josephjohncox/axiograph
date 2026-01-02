export function updatePathStatus(ctx) {
  const { ui, pathStatusEl } = ctx;
  if (!pathStatusEl) return;
  const msg = ui.pathMessage ? ` — ${ui.pathMessage}` : "";
  if (ui.pathStart == null) {
    pathStatusEl.textContent = "";
    return;
  }
  if (ui.pathEnd == null) {
    pathStatusEl.textContent = `path: start=${ui.pathStart}${msg}`;
    return;
  }
  if (!ui.pathEdgeIdxs.length) {
    pathStatusEl.textContent = `path: ${ui.pathStart} → ${ui.pathEnd} (no path)${msg}`;
    return;
  }
  pathStatusEl.textContent = `path: ${ui.pathStart} → ${ui.pathEnd} (${ui.pathEdgeIdxs.length} edges)${msg}`;
}

export function clearPath(ctx) {
  const { ui } = ctx;
  ui.pathStart = null;
  ui.pathEnd = null;
  ui.pathEdgeIdxs = [];
  ui.pathMessage = "";
  updatePathStatus(ctx);
}

export function shortestPathEdgeIdxs(ctx, startId, endId) {
  const { graph, isEdgeVisible } = ctx;
  const edgeIdxs = [];
  for (let i = 0; i < graph.edges.length; i++) {
    if (isEdgeVisible(graph.edges[i])) edgeIdxs.push(i);
  }
  const adj = new Map();
  function addAdj(a, b, edgeIdx) {
    if (!adj.has(a)) adj.set(a, []);
    adj.get(a).push({ to: b, edgeIdx });
  }
  for (const idx of edgeIdxs) {
    const e = graph.edges[idx];
    addAdj(e.source, e.target, idx);
    addAdj(e.target, e.source, idx);
  }
  const q = [];
  const prev = new Map();
  q.push(startId);
  prev.set(startId, null);
  while (q.length) {
    const cur = q.shift();
    if (cur === endId) break;
    const nexts = adj.get(cur) || [];
    for (const step of nexts) {
      if (prev.has(step.to)) continue;
      prev.set(step.to, { node: cur, edgeIdx: step.edgeIdx });
      q.push(step.to);
    }
  }
  if (!prev.has(endId)) return [];
  const out = [];
  let cur = endId;
  while (cur !== startId) {
    const p = prev.get(cur);
    if (!p) break;
    out.push(p.edgeIdx);
    cur = p.node;
  }
  out.reverse();
  return out;
}

export function initPathUi(ctx) {
  const { rerender, clearPathBtn } = ctx;
  if (clearPathBtn) {
    clearPathBtn.addEventListener("click", () => { clearPath(ctx); rerender(); });
  }
  updatePathStatus(ctx);
}
