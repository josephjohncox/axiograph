
import type { GraphEdge, GraphPayload, NodeMap } from "../types";

export function makeBfsDepths(graph: GraphPayload) {
  return function bfsDepths(anchorIds: number[], edgeIdxs: number[]): Map<number, number> {
    const depth = new Map<number, number>();
    const q: number[] = [];
    for (const id of (anchorIds || [])) {
      depth.set(id, 0);
      q.push(id);
    }
    const adj = new Map<number, number[]>();
    for (const n of graph.nodes) adj.set(n.id, []);
    for (const idx of edgeIdxs || []) {
      const e = graph.edges[idx];
      if (!e) continue;
      if (!adj.has(e.source) || !adj.has(e.target)) continue;
      const outgoing = adj.get(e.source);
      const incoming = adj.get(e.target);
      if (!outgoing || !incoming) continue;
      outgoing.push(e.target);
      incoming.push(e.source);
    }
    while (q.length) {
      const cur = q.shift();
      if (cur === undefined) break;
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

export function makeEdgeColor(nodeById: NodeMap) {
  return function edgeColor(e: GraphEdge): string {
    if (e.kind === "equivalence") return "#999";
    if (e.kind && String(e.kind).startsWith("meta")) return "#bbb";
    const src = nodeById.get(e.source);
    if (src && src.kind === "morphism") return "#2f855a";
    if (src && src.kind === "homotopy") return "#6b46c1";
    if (src && src.kind === "fact") return "#8a5a00";
    return "#666";
  };
}
