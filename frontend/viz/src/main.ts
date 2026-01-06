import { initApp } from "./app";

type GraphPayload = { nodes: any[]; edges: any[]; truncated?: boolean };

function setHeaderCounts(graph: GraphPayload) {
  const nodesEl = document.getElementById("graph_nodes");
  const edgesEl = document.getElementById("graph_edges");
  const truncEl = document.getElementById("graph_truncated");
  if (nodesEl) nodesEl.textContent = String(graph.nodes?.length ?? 0);
  if (edgesEl) edgesEl.textContent = String(graph.edges?.length ?? 0);
  if (truncEl) truncEl.textContent = String(!!graph.truncated);
}

function loadGraphFromEmbedded(): GraphPayload | null {
  const win = window as any;
  if (win && win.__AXIOGRAPH_GRAPH) {
    return win.__AXIOGRAPH_GRAPH as GraphPayload;
  }
  const el = document.getElementById("axiograph_graph");
  if (el && el.textContent) {
    try {
      return JSON.parse(el.textContent) as GraphPayload;
    } catch (_e) {
      return null;
    }
  }
  return null;
}

async function loadGraphFromUrl(url: string): Promise<GraphPayload | null> {
  try {
    const resp = await fetch(url, { cache: "no-store" });
    if (!resp.ok) return null;
    return await resp.json();
  } catch (_e) {
    return null;
  }
}

async function boot() {
  const params = new URLSearchParams(window.location.search || "");
  const dataParam = params.get("data");
  const isServer = window.location.protocol === "http:" || window.location.protocol === "https:";

  let graph: GraphPayload | null = loadGraphFromEmbedded();
  if (!graph && dataParam) {
    graph = await loadGraphFromUrl(dataParam);
  } else if (!graph && isServer) {
    graph = await loadGraphFromUrl("/viz.json" + window.location.search);
  }

  if (!graph) {
    console.error("Axiograph viz: missing graph JSON. Use ?data=graph.json or serve from /viz.");
    return;
  }

  setHeaderCounts(graph);
  initApp(graph);
}

function bootWhenReady() {
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", () => boot(), { once: true });
  } else {
    boot();
  }
}

bootWhenReady();
