// @ts-nocheck

export function initSelection(ctx) {
  let selectedId = null;

  function selectedIdRef() {
    return selectedId;
  }

  function syncSelectedClass(id) {
    const nodesEl = ctx.nodesEl;
    if (!nodesEl) return;
    for (const el of nodesEl.querySelectorAll(".node")) {
      el.classList.toggle("selected", el.dataset.id === String(id));
    }
  }

  function selectNode(id, shiftKey) {
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

  function clearHighlights() {
    ctx.ui.highlightIds = new Set();
  }

  function highlightFromQueryResponse(resp) {
    const ids = new Set();
    const rows = (resp && Array.isArray(resp.rows)) ? resp.rows : [];
    for (const row of rows) {
      if (!row || typeof row !== "object") continue;
      for (const k of Object.keys(row)) {
        const v = row[k];
        if (v && typeof v.id === "number") ids.add(v.id);
      }
    }
    ctx.ui.highlightIds = ids;
  }

  function highlightFromToolLoop(outcome) {
    if (!outcome || typeof outcome !== "object") return;
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
