// @ts-nocheck
export function initGraph(graph: any) {
  const nodeById = new Map(graph.nodes.map((n: any) => [n.id, n]));
  const outEdgesBySource = new Map();
  const inEdgesByTarget = new Map();
  for (const e of graph.edges) {
    if (!outEdgesBySource.has(e.source)) outEdgesBySource.set(e.source, []);
    outEdgesBySource.get(e.source).push(e);
    if (!inEdgesByTarget.has(e.target)) inEdgesByTarget.set(e.target, []);
    inEdgesByTarget.get(e.target).push(e);
  }
  return { nodeById, outEdgesBySource, inEdgesByTarget };
}
