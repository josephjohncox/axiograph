import { initApp } from "./app";
import { isGraphPayload, type GraphPayload } from "./types";

declare global {
  interface Window {
    __AXIOGRAPH_GRAPH?: unknown;
  }
}

function setHeaderCounts(graph: GraphPayload) {
  const nodesEl = document.getElementById("graph_nodes");
  const edgesEl = document.getElementById("graph_edges");
  const truncEl = document.getElementById("graph_truncated");
  if (nodesEl) nodesEl.textContent = String(graph.nodes?.length ?? 0);
  if (edgesEl) edgesEl.textContent = String(graph.edges?.length ?? 0);
  if (truncEl) truncEl.textContent = String(Boolean(graph.truncated));
}

function loadGraphFromEmbedded(): GraphPayload | null {
  if (isGraphPayload(window.__AXIOGRAPH_GRAPH)) {
    return window.__AXIOGRAPH_GRAPH;
  }
  const el = document.getElementById("axiograph_graph");
  if (el && el.textContent) {
    try {
      const parsed: unknown = JSON.parse(el.textContent);
      return isGraphPayload(parsed) ? parsed : null;
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
    const parsed: unknown = await resp.json();
    return isGraphPayload(parsed) ? parsed : null;
  } catch (_e) {
    return null;
  }
}

async function boot() {
  const params = new URLSearchParams(window.location.search || "");
  const dataParam = params.get("data");

  let graph: GraphPayload | null = loadGraphFromEmbedded();
  if (!graph && dataParam) {
    graph = await loadGraphFromUrl(dataParam);
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
