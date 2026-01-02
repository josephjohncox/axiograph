// @ts-nocheck

export function initVisibility(ctx) {
  const {
    ui,
    graph,
    nodeById,
    runOnlyEl,
    contextFilterEl,
    show_plane_accepted,
    show_plane_evidence,
    show_plane_data,
    show_entity,
    show_fact,
    show_morphism,
    show_homotopy,
    show_meta,
    show_edge_relation,
    show_edge_equivalence,
    show_edge_meta,
    minConfidenceEl,
    minConfidenceValEl,
    opacityByConfidenceEl,
    rerender,
    factContexts,
    isTupleLike,
    runIdForNode,
    clamp01,
  } = ctx;

  function currentMinConfidence() {
    if (!minConfidenceEl) return 0;
    return clamp01(Number(minConfidenceEl.value));
  }

  function updateMinConfidenceLabel() {
    if (!minConfidenceValEl) return;
    minConfidenceValEl.textContent = currentMinConfidence().toFixed(2);
  }

  function edgeConfidence(e) {
    if (e.confidence == null) return 1.0;
    return clamp01(Number(e.confidence));
  }

  function edgeClass(e) {
    if (!e) return "relation";
    if (e.kind === "equivalence") return "equivalence";
    if (typeof e.kind === "string" && e.kind.startsWith("meta")) return "meta";
    return "relation";
  }

  function isNodeVisible(n) {
    if (!n) return false;

    const plane = n.plane ? String(n.plane) : "";
    if (plane === "accepted" && !show_plane_accepted.checked) return false;
    if (plane === "evidence" && !show_plane_evidence.checked) return false;
    if (plane === "data" && !show_plane_data.checked) return false;

    if (n.kind === "meta" && !show_meta.checked) return false;
    if (n.kind === "fact" && !show_fact.checked) return false;
    if (n.kind === "morphism" && !show_morphism.checked) return false;
    if (n.kind === "homotopy" && !show_homotopy.checked) return false;
    if ((!n.kind || n.kind === "entity") && !show_entity.checked) return false;

    if (runOnlyEl && runOnlyEl.checked && ui.activeRunId) {
      const rid = runIdForNode(n);
      if (!rid || rid !== ui.activeRunId) return false;
    }

    const ctxFilter = contextFilterEl ? String(contextFilterEl.value || "*") : "*";
    if (ctxFilter !== "*") {
      if (n.entity_type === "Context") {
        if (ctxFilter === "__none__") return false;
        return String(n.id) === ctxFilter;
      }
      if (isTupleLike(n)) {
        const ctxs = factContexts.get(n.id);
        if (ctxFilter === "__none__") return !ctxs || !ctxs.size;
        const want = Number(ctxFilter);
        if (!Number.isFinite(want)) return true;
        return ctxs ? ctxs.has(want) : false;
      }
    }

    return true;
  }

  function isEdgeVisible(e) {
    const cls = edgeClass(e);
    if (cls === "equivalence" && !show_edge_equivalence.checked) return false;
    if (cls === "meta" && !show_edge_meta.checked) return false;
    if (cls === "relation" && !show_edge_relation.checked) return false;
    if (!isNodeVisible(nodeById.get(e.source))) return false;
    if (!isNodeVisible(nodeById.get(e.target))) return false;
    const minC = currentMinConfidence();
    if (minC > 0 && edgeConfidence(e) < minC) return false;
    return true;
  }

  function visibleEdgeIdxsAll() {
    const idxs = [];
    for (let i = 0; i < graph.edges.length; i++) {
      if (isEdgeVisible(graph.edges[i])) idxs.push(i);
    }
    return idxs;
  }

  Object.assign(ctx, {
    edgeConfidence,
    isNodeVisible,
    isEdgeVisible,
    visibleEdgeIdxsAll,
    updateMinConfidenceLabel,
  });

  if (minConfidenceEl) {
    minConfidenceEl.addEventListener("input", () => { updateMinConfidenceLabel(); rerender(); });
    updateMinConfidenceLabel();
  }
  if (opacityByConfidenceEl) {
    opacityByConfidenceEl.addEventListener("change", rerender);
  }

  for (const el of [
    show_plane_accepted, show_plane_evidence, show_plane_data,
    show_entity, show_fact, show_morphism, show_homotopy, show_meta,
    runOnlyEl,
    show_edge_relation, show_edge_equivalence, show_edge_meta
  ]) {
    if (!el) continue;
    el.addEventListener("change", rerender);
  }
  if (contextFilterEl) contextFilterEl.addEventListener("change", () => {
    if (ctx.updateContextBadge) ctx.updateContextBadge();
    rerender();
  });
}
