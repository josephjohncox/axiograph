import { getDom } from "./dom";
import { initGraph } from "./graph";
import {
  nodeTitle,
  nodeDisplayName,
  effectiveTypeLabel,
  nodeColor,
  planeStrokeColor,
  parseRelationSignatureFieldOrder,
  parseRelationSignature,
  nodeShortLabel,
  isTupleLike,
  runIdForNode,
} from "./util/labels";
import { escapeHtml, clamp01 } from "./util/helpers";
import { renderNodeList as renderNodeListView } from "./render/list";
import { makeDetailRenderer } from "./render/detail";
import { makeGraphRenderer } from "./render/graph";
import { initServerControls as initServerControlsView } from "./server/controls";
import { initLlmTab } from "./tabs/llm";
import { initQueryTab } from "./tabs/query";
import { initPredictiveProposalTab } from "./tabs/predictive_proposals";
import { initAddTab } from "./tabs/add";
import { initStatus } from "./core/status";
import { initDraft } from "./core/draft";
import { initLayoutControls } from "./core/layout";
import {
  initContextFilter,
  selectedContextFilter,
  currentContextNameFromFilter,
  updateContextBadge,
} from "./core/context";
import {
  initPathUi,
  clearPath,
  updatePathStatus,
  shortestPathEdgeIdxs,
} from "./core/path";
import { initVisibility } from "./core/visibility";
import { initSelection } from "./core/selection";
import { initDetailTabs } from "./core/detail_tabs";
import { makeSummaries } from "./util/summary";
import { initRunFilter } from "./core/run_filter";
import { initComponents } from "./core/components";
import { initDescribe } from "./core/describe";
import { initContextMenu } from "./core/context_menu";
import { makeBfsDepths, makeEdgeColor } from "./util/graph_helpers";
import { UNSUPPORTED } from "./server/read-only-client";
import { isServerMode } from "./util/env";
import { categorizeAttrs } from "./util/attrs";
import type { GraphPayload, VizUiState } from "./types";

function extendContext<T extends object, U extends object>(
  target: T,
  extension: U,
): asserts target is T & U {
  Object.assign(target, extension);
}

export function initApp(graph: GraphPayload) {
  const dom = getDom();
  const {
    nodesEl,
    detailEl,
    searchEl,
    svg,
    serverControlsEl,

    show_plane_accepted,
    show_plane_evidence,
    show_plane_data,
    runFilterEl,
    runOnlyEl,
    runClearBtn,
    layoutAlgoEl,
    layoutCenterEl,
    layoutRefreshBtn,
    layoutFitBtn,
    layoutResetViewBtn,
    labelDensityEl,
    navHelpEl,
    contextFilterEl,
    contextBadgeEl,
    componentJumpBtn,
    componentStatusEl,

    llmQuestionEl,
    llmAutoCommitEl,
    llmCertifyEl,
    llmVerifyEl,
    llmRequireVerifiedEl,
    llmAskBtn,
    llmToQueryBtn,
    llmClearBtn,
    llmStatusEl,
    llmChatEl,
    llmCitationsEl,
    llmDebugEl,

    proposalGoalsEl,
    proposalMaxNewEl,
    proposalSeedEl,
    proposalStepsEl,
    proposalRolloutsEl,
    proposalGuardrailProfileEl,
    proposalGuardrailPlaneEl,
    proposalIncludeGuardrailEl,
    proposalTaskCostsEl,
    proposalAutoCommitEl,
    proposalCommitStepwiseEl,
    proposalProposeBtn,
    proposalPlanBtn,
    proposalStatusEl,
    proposalOutputEl,

    axqlQueryEl,
    axqlRunBtn,
    axqlCertBtn,
    axqlVerifyBtn,
    axqlStatusEl,
    axqlOutputEl,
    certOutputEl,

    addRelTypeEl,
    addSourceNameEl,
    addTargetNameEl,
    addPairingEl,
    addContextEl,
    addCtxFromFilterBtn,
    addEvidenceTextEl,
    addConfidenceEl,
    addConfidenceValEl,
    addMessageEl,
    addAdminTokenEl,
    addGenerateBtn,
    addCommitBtn,
    addStatusEl,
    addOutputEl,
    addCommitOutputEl,
    addDraftAxiBtn,
    addPromoteAxiBtn,
    addPromoteStatusEl,
    addAxiTextEl,
    addPromoteOutputEl,

    reviewFilterEl,
    reviewSelectAllBtn,
    reviewSelectNoneBtn,
    reviewClearBtn,
    reviewStatusEl,
    reviewMessageEl,
    reviewAdminTokenEl,
    reviewCommitBtn,
    reviewDraftAxiBtn,
    reviewPromoteAxiBtn,
    reviewListEl,
    reviewValidationEl,
    reviewAxiTextEl,
    reviewCommitOutputEl,
    reviewPromoteOutputEl,
    reviewOverlayRawEl,

    show_entity,
    show_fact,
    show_morphism,
    show_homotopy,
    show_meta,
    show_edge_relation,
    show_edge_equivalence,
    show_edge_meta,
    clearPathBtn,
    certifyPathBtn,
    verifyPathBtn,
    pathStatusEl,
    minConfidenceEl,
    minConfidenceValEl,
    opacityByConfidenceEl,
  } = dom;

  const { nodeById, outEdgesBySource, inEdgesByTarget } = initGraph(graph);
  const edgeColor = makeEdgeColor(nodeById);
  const bfsDepths = makeBfsDepths(graph);
  let renderDetail: (id: number) => void = () => {};
  let renderGraph: (id: number) => void = () => {};
  let selectedIdRef: () => number | null = () => null;

  const ui: VizUiState = {
    pathStart: null,
    pathEnd: null,
    pathEdgeIdxs: [],
    pathMessage: "",
    highlightIds: new Set<number>(),
    activeRunId: null,
    runMap: new Map<string, number[]>(),
    draft: { kind: "empty", reviewActionStatus: "" },
    layoutAlgo: "radial",
    layoutCenter: "focus",
    layoutSeed: 0,
    layoutBounds: null,
    components: null,
    componentByNode: null,
  };

  const appCtx = {
    graph,
    ui,
    nodesEl,
    detailEl,
    searchEl,
    svg,
    serverControlsEl,
    nodeById,
    outEdgesBySource,
    inEdgesByTarget,
    factContexts: new Map<number, Set<number>>(),
    contextNameById: new Map<number, string>(),
    show_plane_accepted,
    show_plane_evidence,
    show_plane_data,
    runFilterEl,
    runOnlyEl,
    runClearBtn,
    layoutAlgoEl,
    layoutCenterEl,
    layoutRefreshBtn,
    layoutFitBtn,
    layoutResetViewBtn,
    labelDensityEl,
    navHelpEl,
    contextFilterEl,
    contextBadgeEl,
    componentJumpBtn,
    componentStatusEl,
    llmQuestionEl,
    llmAutoCommitEl,
    llmCertifyEl,
    llmVerifyEl,
    llmRequireVerifiedEl,
    llmAskBtn,
    llmToQueryBtn,
    llmClearBtn,
    llmStatusEl,
    llmChatEl,
    llmCitationsEl,
    llmDebugEl,
    proposalGoalsEl,
    proposalMaxNewEl,
    proposalSeedEl,
    proposalStepsEl,
    proposalRolloutsEl,
    proposalGuardrailProfileEl,
    proposalGuardrailPlaneEl,
    proposalIncludeGuardrailEl,
    proposalTaskCostsEl,
    proposalAutoCommitEl,
    proposalCommitStepwiseEl,
    proposalProposeBtn,
    proposalPlanBtn,
    proposalStatusEl,
    proposalOutputEl,
    axqlQueryEl,
    axqlRunBtn,
    axqlCertBtn,
    axqlVerifyBtn,
    axqlStatusEl,
    axqlOutputEl,
    certOutputEl,
    addRelTypeEl,
    addSourceNameEl,
    addTargetNameEl,
    addPairingEl,
    addContextEl,
    addCtxFromFilterBtn,
    addEvidenceTextEl,
    addConfidenceEl,
    addConfidenceValEl,
    addMessageEl,
    addAdminTokenEl,
    addGenerateBtn,
    addCommitBtn,
    addStatusEl,
    addOutputEl,
    addCommitOutputEl,
    addDraftAxiBtn,
    addPromoteAxiBtn,
    addPromoteStatusEl,
    addAxiTextEl,
    addPromoteOutputEl,
    reviewFilterEl,
    reviewSelectAllBtn,
    reviewSelectNoneBtn,
    reviewClearBtn,
    reviewStatusEl,
    reviewMessageEl,
    reviewAdminTokenEl,
    reviewCommitBtn,
    reviewDraftAxiBtn,
    reviewPromoteAxiBtn,
    reviewListEl,
    reviewValidationEl,
    reviewAxiTextEl,
    reviewCommitOutputEl,
    reviewPromoteOutputEl,
    reviewOverlayRawEl,
    show_entity,
    show_fact,
    show_morphism,
    show_homotopy,
    show_meta,
    show_edge_relation,
    show_edge_equivalence,
    show_edge_meta,
    clearPathBtn,
    certifyPathBtn,
    verifyPathBtn,
    pathStatusEl,
    minConfidenceEl,
    minConfidenceValEl,
    opacityByConfidenceEl,
    isTupleLike,
    nodeShortLabel,
    nodeTitle,
    nodeDisplayName,
    effectiveTypeLabel,
    nodeColor,
    edgeColor,
    planeStrokeColor,
    parseRelationSignatureFieldOrder,
    parseRelationSignature,
    runIdForNode,
    escapeHtml,
    clamp01,
  };

  extendContext(appCtx, makeSummaries(appCtx));
  extendContext(appCtx, initDetailTabs(appCtx));

  // Sidebar tabs (keep the UI scannable as tooling grows).
  const tabButtons = Array.from(
    document.querySelectorAll<HTMLButtonElement>(".tabbtn"),
  );
  const tabPanels: Record<string, HTMLElement | null> = {
    explore: document.getElementById("tab_explore"),
    query: document.getElementById("tab_query"),
    llm: document.getElementById("tab_llm"),
    predictive_proposals: document.getElementById("tab_predictive_proposals"),
    review: document.getElementById("tab_review"),
    add: document.getElementById("tab_add"),
  };

  function setActiveTab(name: string) {
    const want = name && tabPanels[name] ? name : "explore";
    for (const btn of tabButtons) {
      btn.classList.toggle("active", (btn.dataset && btn.dataset.tab) === want);
    }
    for (const [k, panel] of Object.entries(tabPanels)) {
      if (!panel) continue;
      panel.classList.toggle("active", k === want);
    }
    try {
      localStorage.setItem("axiograph_viz_sidebar_tab", want);
    } catch {
      return;
    }
  }

  function initTabs() {
    if (!tabButtons.length) return;
    for (const btn of tabButtons) {
      btn.addEventListener("click", () => {
        const tab = (btn.dataset && btn.dataset.tab) || "explore";
        setActiveTab(tab);
      });
    }
    let initial = "explore";
    try {
      const v = localStorage.getItem("axiograph_viz_sidebar_tab");
      if (v) initial = v;
    } catch {
      initial = "explore";
    }
    setActiveTab(initial);
  }
  initTabs();

  // Persisted layout selection (graph view).

  // Context scoping (worlds) UI:
  // - tuple-like nodes are reified facts/morphisms/homotopies
  // - context scoping is represented as: tuple -axi_fact_in_context-> Context
  //
  // In server mode we prefer server-provided:
  // - `graph.contexts` (id -> name),
  // - `graph.tuple_contexts` (tupleId -> [contextId...]),
  // because context edges/nodes may be truncated from the neighborhood graph.
  if (Array.isArray(graph.contexts)) {
    for (const c of graph.contexts) {
      if (!c) continue;
      const id = Number(c.id);
      if (!Number.isFinite(id)) continue;
      const name = String(c.name || `Context#${id}`);
      appCtx.contextNameById.set(id, name);
    }
  }
  for (const n of graph.nodes) {
    if (n.entity_type === "Context") {
      appCtx.contextNameById.set(n.id, n.name || `Context#${n.id}`);
    }
  }
  function addFactContext(tupleId: number, contextId: number): void {
    const contexts = appCtx.factContexts.get(tupleId) ?? new Set<number>();
    contexts.add(contextId);
    appCtx.factContexts.set(tupleId, contexts);
  }
  if (graph.tuple_contexts && typeof graph.tuple_contexts === "object") {
    for (const [k, v] of Object.entries(graph.tuple_contexts)) {
      const tid = Number(k);
      if (!Number.isFinite(tid)) continue;
      const arr = Array.isArray(v) ? v : [];
      for (const cidRaw of arr) {
        const cid = Number(cidRaw);
        if (!Number.isFinite(cid)) continue;
        addFactContext(tid, cid);
      }
    }
  } else {
    // Fallback: derive membership from edges in the neighborhood graph.
    for (const e of graph.edges) {
      if (e.label === "axi_fact_in_context") {
        addFactContext(e.source, e.target);
      }
    }
  }
  extendContext(appCtx, { isServerMode, bfsDepths, categorizeAttrs });

  function rerender() {
    renderNodeList(searchEl.value);
    const selectedId = selectedIdRef();
    if (selectedId != null) {
      for (const el of nodesEl.querySelectorAll<HTMLElement>(".node")) {
        el.classList.toggle("selected", el.dataset.id === String(selectedId));
      }
      renderDetail(selectedId);
      renderGraph(selectedId);
    }
  }

  extendContext(appCtx, { rerender });
  initLayoutControls(appCtx);
  const visibilityApi = initVisibility(appCtx);
  extendContext(appCtx, visibilityApi);
  extendContext(appCtx, initContextMenu(appCtx));

  extendContext(appCtx, {
    shortestPathEdgeIdxs: (a: number, b: number) => shortestPathEdgeIdxs(appCtx, a, b),
    updatePathStatus: () => updatePathStatus(appCtx),
  });

  const selectionApi = initSelection(appCtx);
  extendContext(appCtx, selectionApi);
  selectedIdRef = selectionApi.selectedIdRef;

  function renderNodeList(filter: string): void {
    renderNodeListView(
      { ...appCtx, ...visibilityApi, ...selectionApi },
      filter,
    );
  }

  const detailRenderer = makeDetailRenderer(appCtx);
  renderDetail = detailRenderer.renderDetail;
  const graphRenderer = makeGraphRenderer(appCtx);
  renderGraph = graphRenderer.renderGraph;
  extendContext(appCtx, { renderDetail, renderGraph });
  extendContext(appCtx, initDescribe(appCtx));

  // Populate the node list before selecting a focus node so selection highlighting works.
  renderNodeList(searchEl.value);

  // Auto-select focus node if available.
  if (
    graph.summary &&
    graph.summary.focus_ids &&
    graph.summary.focus_ids.length
  ) {
    appCtx.selectNode(graph.summary.focus_ids[0], false);
  } else if (graph.nodes.length) {
    appCtx.selectNode(graph.nodes[0].id, false);
  }

  initPathUi(appCtx);
  initContextFilter(appCtx);
  extendContext(appCtx, initRunFilter(appCtx));
  extendContext(appCtx, initComponents(appCtx));
  extendContext(appCtx, { clearPath });

  extendContext(appCtx, {
    setActiveTab,
    selectedContextFilter: () => selectedContextFilter(appCtx),
    currentContextNameFromFilter: () => currentContextNameFromFilter(appCtx),
    updateContextBadge: () => updateContextBadge(appCtx),
    rerender,
  });

  if (addCtxFromFilterBtn)
    addCtxFromFilterBtn.addEventListener("click", () => {
      const name = currentContextNameFromFilter(appCtx);
      if (name && addContextEl) addContextEl.value = name;
    });

  function updateAddConfidenceLabel() {
    if (!addConfidenceEl || !addConfidenceValEl) return;
    const v = Number(addConfidenceEl.value || "0");
    addConfidenceValEl.textContent = v.toFixed(2);
  }
  if (addConfidenceEl)
    addConfidenceEl.addEventListener("input", updateAddConfidenceLabel);
  updateAddConfidenceLabel();
  extendContext(appCtx, { updateAddConfidenceLabel });

  function certifySelectedPath(): void {
    ui.pathMessage = UNSUPPORTED;
    updatePathStatus(appCtx);
  }
  for (const button of [certifyPathBtn, verifyPathBtn]) {
    if (button) { button.disabled = true; button.title = UNSUPPORTED; }
  }

  extendContext(appCtx, { certifySelectedPath });

  if (certifyPathBtn)
    certifyPathBtn.addEventListener("click", certifySelectedPath);
  if (verifyPathBtn)
    verifyPathBtn.addEventListener("click", certifySelectedPath);

  const statusApi = initStatus(appCtx);
  extendContext(appCtx, statusApi);

  const draftApi = initDraft(appCtx);
  extendContext(appCtx, draftApi);

  const queryApi = initQueryTab(appCtx);
  extendContext(appCtx, queryApi);

  const llmApi = initLlmTab(appCtx);
  extendContext(appCtx, llmApi);

  const predictiveProposalApi = initPredictiveProposalTab(appCtx);
  extendContext(appCtx, predictiveProposalApi);

  const addApi = initAddTab(appCtx);
  extendContext(appCtx, addApi);

  initServerControlsView(appCtx);

  // Basic pan/zoom (viewBox-based). Pan with Alt+drag (keeps normal click-to-select).
  function parseViewBox() {
    const vb = svg.getAttribute("viewBox");
    if (!vb) return { x: 0, y: 0, w: 1000, h: 800 };
    const parts = vb.trim().split(/\s+/).map(Number);
    if (parts.length !== 4 || parts.some((x) => !Number.isFinite(x)))
      return { x: 0, y: 0, w: 1000, h: 800 };
    return { x: parts[0], y: parts[1], w: parts[2], h: parts[3] };
  }

  const view = parseViewBox();
  function setViewBox() {
    svg.setAttribute("viewBox", `${view.x} ${view.y} ${view.w} ${view.h}`);
  }
  setViewBox();

  let panning = false;
  let panStart: { x: number; y: number; vx: number; vy: number } | null = null;

  svg.addEventListener(
    "wheel",
    (ev) => {
      ev.preventDefault();
      const rect = svg.getBoundingClientRect();
      const mx = view.x + (ev.clientX - rect.left) * (view.w / rect.width);
      const my = view.y + (ev.clientY - rect.top) * (view.h / rect.height);
      const zoom = ev.deltaY < 0 ? 0.9 : 1.1;
      const newW = Math.min(8000, Math.max(200, view.w * zoom));
      const newH = Math.min(8000, Math.max(200, view.h * zoom));
      const relX = (mx - view.x) / view.w;
      const relY = (my - view.y) / view.h;
      view.x = mx - relX * newW;
      view.y = my - relY * newH;
      view.w = newW;
      view.h = newH;
      setViewBox();
    },
    { passive: false },
  );

  svg.addEventListener("mousedown", (ev) => {
    if (!ev.altKey) return;
    panning = true;
    panStart = { x: ev.clientX, y: ev.clientY, vx: view.x, vy: view.y };
    ev.preventDefault();
  });

  window.addEventListener("mousemove", (ev) => {
    if (!panning || !panStart) return;
    const rect = svg.getBoundingClientRect();
    const dx = (ev.clientX - panStart.x) * (view.w / rect.width);
    const dy = (ev.clientY - panStart.y) * (view.h / rect.height);
    view.x = panStart.vx - dx;
    view.y = panStart.vy - dy;
    setViewBox();
  });

  window.addEventListener("mouseup", () => {
    panning = false;
    panStart = null;
  });

  function fitViewToLayoutBounds() {
    if (!ui.layoutBounds) return;
    const pad = 90;
    view.x = ui.layoutBounds.minX - pad;
    view.y = ui.layoutBounds.minY - pad;
    view.w = Math.max(
      200,
      ui.layoutBounds.maxX - ui.layoutBounds.minX + pad * 2,
    );
    view.h = Math.max(
      200,
      ui.layoutBounds.maxY - ui.layoutBounds.minY + pad * 2,
    );
    setViewBox();
  }

  function resetViewToDefault() {
    const w = ui.layoutBounds && ui.layoutBounds.W ? ui.layoutBounds.W : 1000;
    const h = ui.layoutBounds && ui.layoutBounds.H ? ui.layoutBounds.H : 800;
    view.x = 0;
    view.y = 0;
    view.w = w;
    view.h = h;
    setViewBox();
  }

  function centerViewOnNode(nodeId: number) {
    const p = ui.nodePos?.get(nodeId);
    if (!p) return;
    view.x = p.x - view.w / 2;
    view.y = p.y - view.h / 2;
    setViewBox();
  }

  if (layoutFitBtn)
    layoutFitBtn.addEventListener("click", fitViewToLayoutBounds);
  if (layoutResetViewBtn)
    layoutResetViewBtn.addEventListener("click", resetViewToDefault);

  extendContext(appCtx, {
    fitViewToLayoutBounds,
    resetViewToDefault,
    centerViewOnNode,
  });

  // Keyboard shortcuts (avoid interfering with typing in inputs).
  window.addEventListener("keydown", (ev) => {
    const target = ev.target;
    const tag =
      target instanceof Element ? target.tagName.toLowerCase() : "";
    if (tag === "input" || tag === "textarea" || ev.isComposing) return;

    if (ev.key === "/" && !ev.ctrlKey && !ev.metaKey && !ev.altKey) {
      if (searchEl) searchEl.focus();
      ev.preventDefault();
      return;
    }
    if (ev.key === "?" && !ev.ctrlKey && !ev.metaKey && !ev.altKey) {
      if (navHelpEl) navHelpEl.open = !navHelpEl.open;
      ev.preventDefault();
      return;
    }
    if (
      (ev.key === "r" || ev.key === "R") &&
      !ev.ctrlKey &&
      !ev.metaKey &&
      !ev.altKey
    ) {
      ui.layoutSeed = (Number(ui.layoutSeed || 0) + 1) >>> 0;
      rerender();
      ev.preventDefault();
      return;
    }
    if (
      (ev.key === "f" || ev.key === "F") &&
      !ev.ctrlKey &&
      !ev.metaKey &&
      !ev.altKey
    ) {
      fitViewToLayoutBounds();
      ev.preventDefault();
      return;
    }
    if (ev.key === "0" && !ev.ctrlKey && !ev.metaKey && !ev.altKey) {
      resetViewToDefault();
      ev.preventDefault();
      return;
    }
    if (ev.key === "Escape") {
      clearPath(appCtx);
      rerender();
    }
  });
}
