// @ts-nocheck
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
import { initWorldModelTab } from "./tabs/world_model";
import { initAddTab } from "./tabs/add";
import { initStatus } from "./core/status";
import { initDraft } from "./core/draft";
import { initLayoutControls } from "./core/layout";
import { initContextFilter, selectedContextFilter, currentContextNameFromFilter, updateContextBadge } from "./core/context";
import { initPathUi, clearPath, updatePathStatus, shortestPathEdgeIdxs } from "./core/path";
import { initVisibility } from "./core/visibility";
import { initSelection } from "./core/selection";
import { initDetailTabs } from "./core/detail_tabs";
import { makeSummaries } from "./util/summary";
import { initRunFilter } from "./core/run_filter";
import { initComponents } from "./core/components";
import { initDescribe } from "./core/describe";
import { initContextMenu } from "./core/context_menu";
import { makeBfsDepths, makeEdgeColor } from "./util/graph_helpers";
import { isServerMode } from "./util/env";
import { categorizeAttrs } from "./util/attrs";

export function initApp(graph: any) {

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

    wmGoalsEl,
    wmMaxNewEl,
    wmSeedEl,
    wmStepsEl,
    wmRolloutsEl,
    wmGuardrailProfileEl,
    wmGuardrailPlaneEl,
    wmIncludeGuardrailEl,
    wmTaskCostsEl,
    wmAutoCommitEl,
    wmCommitStepwiseEl,
    wmProposeBtn,
    wmPlanBtn,
    wmStatusEl,
    wmOutputEl,

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
let renderDetail = () => {};
let renderGraph = () => {};

const ui = {
  pathStart: null,
  pathEnd: null,
  pathEdgeIdxs: [],
  pathMessage: "",
  highlightIds: new Set(),
  activeRunId: null,
  runMap: new Map(),
  draftOverlay: null,
  draftSelected: new Set(),
  reviewActionStatus: "",
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
  factContexts: new Map(),
  contextNameById: new Map(),
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
  wmGoalsEl,
  wmMaxNewEl,
  wmSeedEl,
  wmStepsEl,
  wmRolloutsEl,
  wmGuardrailProfileEl,
  wmGuardrailPlaneEl,
  wmIncludeGuardrailEl,
  wmTaskCostsEl,
  wmAutoCommitEl,
  wmCommitStepwiseEl,
  wmProposeBtn,
  wmPlanBtn,
  wmStatusEl,
  wmOutputEl,
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

Object.assign(appCtx, makeSummaries(appCtx));
Object.assign(appCtx, initDetailTabs(appCtx));


// Sidebar tabs (keep the UI scannable as tooling grows).
const tabButtons = Array.from(document.querySelectorAll(".tabbtn"));
const tabPanels = {
  explore: document.getElementById("tab_explore"),
  query: document.getElementById("tab_query"),
  llm: document.getElementById("tab_llm"),
  world_model: document.getElementById("tab_world_model"),
  review: document.getElementById("tab_review"),
  add: document.getElementById("tab_add"),
};

function setActiveTab(name) {
  const want = (name && tabPanels[name]) ? name : "explore";
  for (const btn of tabButtons) {
    btn.classList.toggle("active", (btn.dataset && btn.dataset.tab) === want);
  }
  for (const [k, panel] of Object.entries(tabPanels)) {
    if (!panel) continue;
    panel.classList.toggle("active", k === want);
  }
  try { localStorage.setItem("axiograph_viz_sidebar_tab", want); } catch (_e) {}
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
  } catch (_e) {}
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
if (graph.tuple_contexts && typeof graph.tuple_contexts === "object") {
  for (const [k, v] of Object.entries(graph.tuple_contexts)) {
    const tid = Number(k);
    if (!Number.isFinite(tid)) continue;
    const arr = Array.isArray(v) ? v : [];
    for (const cidRaw of arr) {
      const cid = Number(cidRaw);
      if (!Number.isFinite(cid)) continue;
      if (!appCtx.factContexts.has(tid)) appCtx.factContexts.set(tid, new Set());
      appCtx.factContexts.get(tid).add(cid);
    }
  }
} else {
  // Fallback: derive membership from edges in the neighborhood graph.
  for (const e of graph.edges) {
    if (e.label === "axi_fact_in_context") {
      if (!appCtx.factContexts.has(e.source)) appCtx.factContexts.set(e.source, new Set());
      appCtx.factContexts.get(e.source).add(e.target);
    }
  }
}
appCtx.isServerMode = isServerMode;
appCtx.bfsDepths = bfsDepths;


function renderNodeList(filter) {
  return renderNodeListView(appCtx, filter);
}
appCtx.categorizeAttrs = categorizeAttrs;

function rerender() {
  renderNodeList(searchEl.value);
  const selectedId = appCtx.selectedIdRef ? appCtx.selectedIdRef() : null;
  if (selectedId != null) {
    for (const el of nodesEl.querySelectorAll(".node")) {
      el.classList.toggle("selected", el.dataset.id === String(selectedId));
    }
    renderDetail(selectedId);
    renderGraph(selectedId);
  }
}

appCtx.rerender = rerender;
initLayoutControls(appCtx);
initVisibility(appCtx);
Object.assign(appCtx, initContextMenu(appCtx));

appCtx.shortestPathEdgeIdxs = (a, b) => shortestPathEdgeIdxs(appCtx, a, b);
appCtx.updatePathStatus = () => updatePathStatus(appCtx);

const selectionApi = initSelection(appCtx);
Object.assign(appCtx, selectionApi);

const detailRenderer = makeDetailRenderer(appCtx);
renderDetail = detailRenderer.renderDetail;
const graphRenderer = makeGraphRenderer(appCtx);
renderGraph = graphRenderer.renderGraph;
appCtx.renderDetail = renderDetail;
appCtx.renderGraph = renderGraph;
Object.assign(appCtx, initDescribe(appCtx));

// Populate the node list before selecting a focus node so selection highlighting works.
renderNodeList(searchEl.value);

// Auto-select focus node if available.
if (graph.summary && graph.summary.focus_ids && graph.summary.focus_ids.length) {
  appCtx.selectNode(graph.summary.focus_ids[0], false);
} else if (graph.nodes.length) {
  appCtx.selectNode(graph.nodes[0].id, false);
}

initPathUi(appCtx);
initContextFilter(appCtx);
Object.assign(appCtx, initRunFilter(appCtx));
Object.assign(appCtx, initComponents(appCtx));

Object.assign(appCtx, {
  setActiveTab,
  selectedContextFilter: () => selectedContextFilter(appCtx),
  currentContextNameFromFilter: () => currentContextNameFromFilter(appCtx),
  updateContextBadge: () => updateContextBadge(appCtx),
  prefillAddFromToolLoop,
  rerender,
});

if (addCtxFromFilterBtn) addCtxFromFilterBtn.addEventListener("click", () => {
  const name = currentContextNameFromFilter(appCtx);
  if (name && addContextEl) addContextEl.value = name;
});

function updateAddConfidenceLabel() {
  if (!addConfidenceEl || !addConfidenceValEl) return;
  const v = Number(addConfidenceEl.value || "0");
  addConfidenceValEl.textContent = v.toFixed(2);
}
if (addConfidenceEl) addConfidenceEl.addEventListener("input", updateAddConfidenceLabel);
updateAddConfidenceLabel();
appCtx.updateAddConfidenceLabel = updateAddConfidenceLabel;

function prefillAddFromToolLoop(outcome) {
  if (!outcome) return false;

  // Preferred: use the backend-extracted artifact (stable, merged, UI-friendly).
  const artifact = outcome
    && outcome.artifacts
    && outcome.artifacts.generated_overlay
    ? outcome.artifacts.generated_overlay
    : null;
  if (artifact && artifact.proposals_json) {
    return setDraftOverlay(artifact, { notePrefix: "generated from LLM" });
  }

  // Fallback: scan the transcript for proposal generation tool outputs.
  const steps = Array.isArray(outcome.steps) ? outcome.steps : [];
  for (let i = steps.length - 1; i >= 0; i--) {
    const s = steps[i];
    if (!s) continue;
    if (
      s.tool !== "propose_relation_proposals"
      && s.tool !== "propose_relations_proposals"
      && s.tool !== "world_model_propose"
      && s.tool !== "world_model_plan"
    ) continue;
    const r = s.result || null;
    if (!r || !r.proposals_json) continue;
    return setDraftOverlay(r, { notePrefix: "generated from LLM (fallback)" });
  }

  return false;
}


function directedShortestPathEdgeIdxs(startId, endId) {
  const edgeIdxs = appCtx.visibleEdgeIdxsAll();
  const adj = new Map();
  function addAdj(a, b, edgeIdx) {
    if (!adj.has(a)) adj.set(a, []);
    adj.get(a).push({ to: b, edgeIdx });
  }
  for (const idx of edgeIdxs) {
    const e = graph.edges[idx];
    if (e.relation_id == null) continue;
    addAdj(e.source, e.target, idx);
  }
  const q = [];
  const prev = new Map(); // node -> { node: prevNode, edgeIdx }
  q.push(startId);
  prev.set(startId, null);
  while (q.length) {
    const cur = q.shift();
    if (cur === endId) break;
    const nexts = adj.get(cur) || [];
    for (const step of nexts) {
      if (prev.has(step.to)) continue;
      prev.set(step.to, { node: cur, edgeIdx: step.edgeIdx });
      q.push(step.to);
    }
  }
  if (!prev.has(endId)) return [];
  const out = [];
  let cur = endId;
  while (cur !== startId) {
    const p = prev.get(cur);
    if (!p) break;
    out.push(p.edgeIdx);
    cur = p.node;
  }
  out.reverse();
  return out;
}

async function certifySelectedPath(verify) {
  if (ui.pathStart == null || ui.pathEnd == null) {
    ui.pathMessage = "shift-click 2 nodes first";
    updatePathStatus(appCtx);
    return;
  }

  const proto = window.location && window.location.protocol;
  if (!(proto === "http:" || proto === "https:")) {
    ui.pathMessage = "certification requires server mode";
    updatePathStatus(appCtx);
    return;
  }

  const directed = directedShortestPathEdgeIdxs(ui.pathStart, ui.pathEnd);
  if (!directed.length) {
    ui.pathMessage = "no directed relation path (certifiable)";
    updatePathStatus(appCtx);
    return;
  }

  // Update highlighted path to the certifiable directed one.
  ui.pathEdgeIdxs = directed;
  updatePathStatus(appCtx);
  rerender();

  const relationIds = [];
  let cur = ui.pathStart;
  for (const idx of directed) {
    const e = graph.edges[idx];
    if (e.relation_id == null) {
      ui.pathMessage = "path contains non-relation edges";
      updatePathStatus(appCtx);
      return;
    }
    if (e.source !== cur) {
      ui.pathMessage = "path direction mismatch (internal)";
      updatePathStatus(appCtx);
      return;
    }
    relationIds.push(e.relation_id);
    cur = e.target;
  }

  ui.pathMessage = verify ? "certifying+verifying…" : "certifying…";
  updatePathStatus(appCtx);
  if (appCtx.setCertOutput) appCtx.setCertOutput("");

  try {
    const params = new URLSearchParams(window.location.search || "");
    const snapshot = params.get("snapshot");
    const body = {
      start: ui.pathStart,
      relation_ids: relationIds,
      verify: !!verify,
      include_anchor: false,
    };
    if (snapshot) body.snapshot = snapshot;
    const resp = await fetch("/cert/reachability", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
    const data = await resp.json();
    if (appCtx.setCertOutput) appCtx.setCertOutput(data);
    if (!resp.ok) {
      ui.pathMessage = `error (${resp.status})`;
      updatePathStatus(appCtx);
      return;
    }
    ui.pathMessage = verify ? (data.certificate_verified ? "verified" : "not verified") : "ok";
    updatePathStatus(appCtx);
  } catch (e) {
    ui.pathMessage = "error";
    updatePathStatus(appCtx);
    if (appCtx.setCertOutput) appCtx.setCertOutput(String(e));
  }
}

if (certifyPathBtn) certifyPathBtn.addEventListener("click", () => certifySelectedPath(false));
if (verifyPathBtn) verifyPathBtn.addEventListener("click", () => certifySelectedPath(true));

const statusApi = initStatus(appCtx);
Object.assign(appCtx, statusApi);

const draftApi = initDraft(appCtx);
Object.assign(appCtx, draftApi);

const queryApi = initQueryTab(appCtx);
Object.assign(appCtx, queryApi);

const llmApi = initLlmTab(appCtx);
Object.assign(appCtx, llmApi);

const worldModelApi = initWorldModelTab(appCtx);
Object.assign(appCtx, worldModelApi);

const addApi = initAddTab(appCtx);
Object.assign(appCtx, addApi);

initServerControlsView(appCtx);

// Basic pan/zoom (viewBox-based). Pan with Alt+drag (keeps normal click-to-select).
function parseViewBox() {
  const vb = svg.getAttribute("viewBox");
  if (!vb) return { x: 0, y: 0, w: 1000, h: 800 };
  const parts = vb.trim().split(/\s+/).map(Number);
  if (parts.length !== 4 || parts.some(x => !Number.isFinite(x))) return { x: 0, y: 0, w: 1000, h: 800 };
  return { x: parts[0], y: parts[1], w: parts[2], h: parts[3] };
}

const view = parseViewBox();
function setViewBox() {
  svg.setAttribute("viewBox", `${view.x} ${view.y} ${view.w} ${view.h}`);
}
setViewBox();

let panning = false;
let panStart = null;

svg.addEventListener("wheel", (ev) => {
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
}, { passive: false });

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

window.addEventListener("mouseup", () => { panning = false; panStart = null; });

function fitViewToLayoutBounds() {
  if (!ui.layoutBounds) return;
  const pad = 90;
  view.x = ui.layoutBounds.minX - pad;
  view.y = ui.layoutBounds.minY - pad;
  view.w = Math.max(200, (ui.layoutBounds.maxX - ui.layoutBounds.minX) + pad * 2);
  view.h = Math.max(200, (ui.layoutBounds.maxY - ui.layoutBounds.minY) + pad * 2);
  setViewBox();
}

function resetViewToDefault() {
  const w = (ui.layoutBounds && ui.layoutBounds.W) ? ui.layoutBounds.W : 1000;
  const h = (ui.layoutBounds && ui.layoutBounds.H) ? ui.layoutBounds.H : 800;
  view.x = 0;
  view.y = 0;
  view.w = w;
  view.h = h;
  setViewBox();
}

if (layoutFitBtn) layoutFitBtn.addEventListener("click", fitViewToLayoutBounds);
if (layoutResetViewBtn) layoutResetViewBtn.addEventListener("click", resetViewToDefault);

// Keyboard shortcuts (avoid interfering with typing in inputs).
window.addEventListener("keydown", (ev) => {
  const tag = (ev.target && ev.target.tagName) ? String(ev.target.tagName).toLowerCase() : "";
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
  if ((ev.key === "r" || ev.key === "R") && !ev.ctrlKey && !ev.metaKey && !ev.altKey) {
    ui.layoutSeed = (Number(ui.layoutSeed || 0) + 1) >>> 0;
    rerender();
    ev.preventDefault();
    return;
  }
  if ((ev.key === "f" || ev.key === "F") && !ev.ctrlKey && !ev.metaKey && !ev.altKey) {
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
