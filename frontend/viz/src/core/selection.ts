import type { VizUiState } from "../types";

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

  // Query and evidence IDs are not graph IDs without an authenticated image
  // binding. No response-to-highlight adapter is exposed by this frontend.
  Object.assign(ctx, { selectNode, selectedIdRef, clearHighlights });

  return { selectNode, selectedIdRef, clearHighlights };
}
