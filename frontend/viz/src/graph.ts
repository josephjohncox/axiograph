import type { EdgeMap, GraphPayload, NodeMap } from "./types";

export function initGraph(graph: GraphPayload) {
  const nodeById: NodeMap = new Map(graph.nodes.map((node) => [node.id, node]));
  const outEdgesBySource: EdgeMap = new Map<number, GraphPayload["edges"]>();
  const inEdgesByTarget: EdgeMap = new Map<number, GraphPayload["edges"]>();
  for (const edge of graph.edges) {
    const outgoing = outEdgesBySource.get(edge.source) ?? [];
    outgoing.push(edge);
    outEdgesBySource.set(edge.source, outgoing);
    const incoming = inEdgesByTarget.get(edge.target) ?? [];
    incoming.push(edge);
    inEdgesByTarget.set(edge.target, incoming);
  }
  return { nodeById, outEdgesBySource, inEdgesByTarget };
}
