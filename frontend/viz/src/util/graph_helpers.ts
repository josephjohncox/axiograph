// @ts-nocheck

export function makeBfsDepths(graph) {
  return function bfsDepths(anchorIds, edgeIdxs) {
    const depth = new Map();
    const q = [];
    for (const id of (anchorIds || [])) {
      depth.set(id, 0);
      q.push(id);
    }
    const adj = new Map();
    for (const n of graph.nodes) adj.set(n.id, []);
    for (const idx of edgeIdxs || []) {
      const e = graph.edges[idx];
      if (!e) continue;
      if (!adj.has(e.source) || !adj.has(e.target)) continue;
      adj.get(e.source).push(e.target);
      adj.get(e.target).push(e.source);
    }
    while (q.length) {
      const cur = q.shift();
      const d = depth.get(cur) || 0;
      const nexts = adj.get(cur) || [];
      for (const nb of nexts) {
        if (depth.has(nb)) continue;
        depth.set(nb, d + 1);
        q.push(nb);
      }
    }
    return depth;
  };
}

export function makeEdgeColor(nodeById) {
  return function edgeColor(e) {
    if (e.kind === "equivalence") return "#999";
    if (e.kind && String(e.kind).startsWith("meta")) return "#bbb";
    const src = nodeById.get(e.source);
    if (src && src.kind === "morphism") return "#2f855a";
    if (src && src.kind === "homotopy") return "#6b46c1";
    if (src && src.kind === "fact") return "#8a5a00";
    return "#666";
  };
}
