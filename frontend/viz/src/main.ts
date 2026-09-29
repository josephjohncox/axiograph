import { initApp } from "./app";
import { readBoundedJsonResponse } from "./json-boundary";
import {
  GRAPH_JSON_BUDGET,
  parseGraphPayloadJson,
  validateGraphPayload,
  type GraphPayload,
} from "./types";

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
  try {
    if (window.__AXIOGRAPH_GRAPH !== undefined) {
      return validateGraphPayload(window.__AXIOGRAPH_GRAPH);
    }
    const el = document.getElementById("axiograph_graph");
    return el?.textContent ? parseGraphPayloadJson(el.textContent) : null;
  } catch (error) {
    console.error(`Axiograph viz: rejected embedded graph: ${String(error)}`);
    return null;
  }
}

async function loadGraphFromUrl(url: string): Promise<GraphPayload | null> {
  try {
    const target = new URL(url, window.location.href);
    if (target.origin !== window.location.origin) {
      throw new Error("graph URL must be same-origin");
    }
    const response = await fetch(target, {
      cache: "no-store",
      credentials: "same-origin",
      redirect: "error",
      signal: AbortSignal.timeout(30_000),
    });
    const parsed = await readBoundedJsonResponse(response, GRAPH_JSON_BUDGET, "graph response");
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    return validateGraphPayload(parsed);
  } catch (error) {
    console.error(`Axiograph viz: rejected fetched graph: ${String(error)}`);
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
