import { isRecord, type VizUiState } from "../types";

interface SelectionContext {
  nodesEl: HTMLElement;
  ui: VizUiState;
  shortestPathEdgeIdxs?: (startId: number, endId: number) => number[];
  updatePathStatus?: () => void;
  renderDetail?: (id: number) => void;
  renderGraph?: (id: number) => void;
  fetchDescribeEntity?: (id: number) => void;
}

export function initSelection(ctx: SelectionContext) {
  let selectedId: number | null = null;

  function selectedIdRef() {
    return selectedId;
  }

  function syncSelectedClass(id: number): void {
    const nodesEl = ctx.nodesEl;
    if (!nodesEl) return;
    for (const el of nodesEl.querySelectorAll<HTMLElement>(".node")) {
      el.classList.toggle("selected", el.dataset.id === String(id));
    }
  }

  function selectNode(id: number, shiftKey: boolean): void {
    selectedId = id;
    syncSelectedClass(id);
    if (shiftKey) {
      if (ctx.ui.pathStart == null || (ctx.ui.pathStart != null && ctx.ui.pathEnd != null)) {
        ctx.ui.pathStart = id;
        ctx.ui.pathEnd = null;
        ctx.ui.pathEdgeIdxs = [];
        ctx.ui.pathMessage = "";
      } else if (ctx.ui.pathEnd == null) {
        ctx.ui.pathEnd = id;
        if (ctx.shortestPathEdgeIdxs) {
          ctx.ui.pathEdgeIdxs = ctx.shortestPathEdgeIdxs(ctx.ui.pathStart, ctx.ui.pathEnd);
        }
        ctx.ui.pathMessage = "";
      }
      if (ctx.updatePathStatus) ctx.updatePathStatus();
    }
    if (ctx.renderDetail) ctx.renderDetail(id);
    if (ctx.renderGraph) ctx.renderGraph(id);
    if (ctx.fetchDescribeEntity) ctx.fetchDescribeEntity(id);
  }

  function clearHighlights(): void {
    ctx.ui.highlightIds = new Set<number>();
  }

  function highlightFromQueryResponse(resp: unknown): void {
    const ids = new Set<number>();
    const rows = isRecord(resp) && Array.isArray(resp.rows) ? resp.rows : [];
    for (const row of rows) {
      if (!isRecord(row)) continue;
      for (const value of Object.values(row)) {
        if (isRecord(value) && typeof value.id === "number") ids.add(value.id);
      }
    }
    ctx.ui.highlightIds = ids;
  }

  function highlightFromToolLoop(outcome: unknown): void {
    if (!isRecord(outcome)) return;
    if (outcome.query_result) {
      highlightFromQueryResponse(outcome.query_result);
      return;
    }
    if (outcome.query) {
      highlightFromQueryResponse(outcome.query);
      return;
    }
    if (Array.isArray(outcome.rows)) {
      highlightFromQueryResponse({ rows: outcome.rows });
    }
  }

  Object.assign(ctx, {
    selectNode,
    selectedIdRef,
    clearHighlights,
    highlightFromQueryResponse,
    highlightFromToolLoop,
  });

  return {
    selectNode,
    selectedIdRef,
    clearHighlights,
    highlightFromQueryResponse,
    highlightFromToolLoop,
  };
}
