import assert from "node:assert/strict";
import test from "node:test";
import { build } from "esbuild";

// Bundle the real modules with the already locked build dependency, so their
// production extensionless imports resolve without a custom loader or copies.
const { outputFiles } = await build({
  stdin: {
    contents: `export { renderNodeList } from './src/render/list';
export { makeDetailRenderer } from './src/render/detail';
export { initStatus } from './src/core/status';
export { initDraft } from './src/core/draft';
export { initAddTab } from './src/tabs/add';
export { initLlmTab } from './src/tabs/llm';
export { selectDraft } from './src/core/draft-selection';
export * from './src/util/labels';`,
    resolveDir: new URL("../", import.meta.url).pathname,
  },
  bundle: true,
  write: false,
  format: "esm",
  platform: "node",
});
const production = await import(
  `data:text/javascript;base64,${Buffer.from(outputFiles[0].text).toString("base64")}`
);
const {
  renderNodeList,
  makeDetailRenderer,
  initStatus,
  initDraft,
  initAddTab,
  initLlmTab,
  selectDraft,
} = production;

// DOM operation spies only: no HTML parsing, browser layout, event bubbling,
// CSS selector engine, accessibility tree, or script execution is simulated.
class ElementSpy {
  constructor(tag = "div") {
    this.tag = tag;
    this.children = [];
    this.style = {};
    this.dataset = {};
    this.className = "";
    this.value = "";
    this.listeners = new Map();
    this.writes = 0;
    this.classList = {
      add: (c) => {
        this.className += ` ${c}`;
      },
      remove: (c) => {
        this.className = this.className
          .split(" ")
          .filter((v) => v !== c)
          .join(" ");
      },
    };
  }
  set innerHTML(_) {
    assert.fail("production rendering must not use an HTML sink");
  }
  set outerHTML(_) {
    assert.fail("production rendering must not use an HTML sink");
  }
  insertAdjacentHTML() {
    assert.fail("production rendering must not use an HTML sink");
  }
  set textContent(value) {
    this.writes++;
    this.children = [String(value ?? "")];
  }
  get textContent() {
    return this.children
      .map((c) => (typeof c === "string" ? c : c.textContent))
      .join("");
  }
  append(...children) {
    this.children.push(...children);
  }
  appendChild(child) {
    this.append(child);
    return child;
  }
  replaceChildren(...children) {
    this.writes++;
    this.children = children;
  }
  addEventListener(name, handler) {
    assert.equal(typeof handler, "function");
    if (!this.listeners.has(name)) this.listeners.set(name, []);
    this.listeners.get(name).push(handler);
  }
  async fire(name, fields = {}) {
    for (const handler of this.listeners.get(name) || [])
      await handler({ preventDefault() {}, ...fields });
  }
}
function descendants(root, predicate) {
  return root.children
    .filter((c) => c instanceof ElementSpy)
    .flatMap((c) => [
      ...(predicate(c) ? [c] : []),
      ...descendants(c, predicate),
    ]);
}
const tagged = (root, tag) => descendants(root, (n) => n.tag === tag);
const hasClass = (n, name) => n.className.split(" ").includes(name);
const byClass = (root, name) => descendants(root, (n) => hasClass(n, name));
function installGlobal(t, key, value) {
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, key);
  Object.defineProperty(globalThis, key, {
    value,
    writable: true,
    configurable: true,
  });
  t.after(() => {
    if (descriptor) Object.defineProperty(globalThis, key, descriptor);
    else delete globalThis[key];
  });
}
function environment(t) {
  t.mock.method(globalThis, "fetch", () => assert.fail("unexpected request"));
  const stored = new Map();
  installGlobal(t, "document", { createElement: (tag) => new ElementSpy(tag) });
  installGlobal(t, "window", {
    location: {
      host: "fixture.invalid",
      protocol: "https:",
      search: "?snapshot=fixture&focus_name=old",
    },
  });
  installGlobal(t, "localStorage", {
    getItem: (k) => stored.get(k) || null,
    setItem: (k, v) => stored.set(k, v),
    removeItem: (k) => stored.delete(k),
  });
  return stored;
}
const hostile =
  '<img src=x onerror="attack()"> & </script><svg onload=attack()> 日本語';
const node = (id, extra = {}) => ({
  id,
  kind: "entity",
  entity_type: "Thing",
  name: `Node ${id}`,
  attrs: {},
  ...extra,
});
function graphContext(nodes, edges = []) {
  const calls = [],
    tabs = [];
  return {
    ...production,
    graph: { nodes, edges },
    ui: { highlightIds: new Set(), pathEdgeIdxs: [], describeCache: new Map() },
    nodeById: new Map(nodes.map((n) => [n.id, n])),
    nodesEl: new ElementSpy(),
    detailEl: new ElementSpy(),
    outEdgesBySource: new Map(
      nodes.map((n) => [n.id, edges.filter((e) => e.source === n.id)]),
    ),
    inEdgesByTarget: new Map(
      nodes.map((n) => [n.id, edges.filter((e) => e.target === n.id)]),
    ),
    isNodeVisible: () => true,
    isEdgeVisible: (e) => e.visible !== false,
    selectNode: (...args) => calls.push(args),
    selectedIdRef: () => 0,
    setActiveDetailTab: (tab) => tabs.push(tab),
    calls,
    tabs,
    factSummary: (n) => `fact ${n.name}`,
    morphismSummary: (n) => `arrow ${n.name}`,
    homotopySummary: (n) => `equivalence ${n.name}`,
    proposalRunSummary: () => "run summary",
    documentSummary: () => "document summary",
    docChunkSummary: () => "chunk summary",
    firstOutTargetId(id, label) {
      return edges.find((e) => e.source === id && e.label === label)?.target;
    },
    shortenHash: (s) => s.slice(0, 8),
    isServerMode: () => false,
    categorizeAttrs: (attrs) => ({
      content: Object.entries(attrs).filter(([k]) =>
        ["text", "markdown"].includes(k),
      ),
      other: [[hostile, hostile]],
      axi: [["axi", hostile]],
      overlay: [["overlay", hostile]],
    }),
  };
}

test("production list preserves grouped labels, highlighted selection, shift-click, hostile IDs and empty resets", async (t) => {
  environment(t);
  const ctx = graphContext([
    node(0, { name: hostile, entity_type: hostile }),
    node(2, { name: "", kind: "homotopy" }),
    node(hostile),
  ]);
  ctx.ui.highlightIds.add(0);
  renderNodeList(ctx, "");
  assert.equal(tagged(ctx.nodesEl, "details").length, 3);
  assert.ok(tagged(ctx.nodesEl, "details").every((d) => d.open));
  const selected = byClass(ctx.nodesEl, "selected")[0];
  assert.ok(hasClass(selected, "highlighted"));
  assert.ok(selected.textContent.includes(`★ ${hostile}`));
  assert.ok(ctx.nodesEl.textContent.includes(`#${hostile}`));
  assert.ok(ctx.nodesEl.textContent.includes("path equivalence"));
  assert.ok(ctx.nodesEl.textContent.includes("(no name)"));
  await selected.fire("click", { shiftKey: true });
  assert.deepEqual(ctx.calls, [[0, true]]);
  renderNodeList(ctx, "absent");
  assert.equal(ctx.nodesEl.textContent, "(no matching nodes)");
  renderNodeList(ctx, "日本語");
  assert.equal(byClass(ctx.nodesEl, "node").length, 2);
  assert.equal(tagged(ctx.nodesEl, "img").length, 0);
});

test("production list virtualizes and replaces slices without losing IDs or interactions", async (t) => {
  environment(t);
  let scheduled,
    cancelled = 0;
  installGlobal(t, "requestAnimationFrame", (callback) => {
    scheduled = callback;
    return 1;
  });
  installGlobal(t, "cancelAnimationFrame", () => {
    cancelled++;
  });
  const ctx = graphContext(
    Array.from({ length: 2001 }, (_, id) => node(id, { name: hostile })),
  );
  ctx.ui.highlightIds.add(0);
  renderNodeList(ctx, "");
  const scroller = byClass(ctx.nodesEl, "node-list-virtual")[0];
  const viewport = byClass(ctx.nodesEl, "node-list-viewport")[0];
  assert.equal(
    byClass(ctx.nodesEl, "node-list-spacer")[0].style.height,
    "128064px",
  );
  assert.equal(viewport.children.length, 17);
  await viewport.children[0].fire("click", { shiftKey: false });
  assert.deepEqual(ctx.calls, [[0, false]]);
  scroller.scrollTop = 6400;
  await scroller.fire("scroll");
  await scroller.fire("scroll");
  scheduled();
  assert.equal(cancelled, 1);
  assert.equal(viewport.style.transform, "translateY(6016px)");
  assert.equal(viewport.children[0].dataset.id, "94");
  assert.equal(viewport.children.length, 17);
  ctx.graph.nodes = [];
  renderNodeList(ctx, "");
  assert.equal(byClass(ctx.nodesEl, "node-list-virtual").length, 0);
});

test("production detail rich panels retain tuples, field order, paths, attrs, tabs and literal text", async (t) => {
  environment(t);
  const nodes = [
    node(0, { name: hostile }),
    node(1, {
      kind: "fact",
      name: hostile,
      plane: "evidence",
      attrs: {
        text: hostile,
        axi_relation: hostile,
        axi_overlay_relation_signature: "R(second: B, first: A)",
        axi_overlay_constraints: hostile,
      },
    }),
  ];
  const edges = [
    { source: 1, target: 0, label: "first", kind: "relation", confidence: 0.9 },
    { source: 1, target: 0, label: "second", kind: "relation" },
    {
      source: 1,
      target: 0,
      label: "axi_fact_of",
      kind: "relation",
      visible: false,
    },
  ];
  const ctx = graphContext(nodes, edges);
  ctx.ui.pathStart = 1;
  ctx.ui.pathEnd = 0;
  ctx.ui.pathEdgeIdxs = [0];
  const { renderDetail } = makeDetailRenderer(ctx);
  renderDetail(1);
  assert.deepEqual(ctx.tabs, ["facts"]);
  assert.deepEqual(
    tagged(ctx.detailEl, "button").map((b) => b.textContent),
    ["overview", "tuple", "edges", "attrs", "db"],
  );
  const panels = byClass(ctx.detailEl, "detailtabpanel");
  assert.equal(panels.length, 5);
  const facts = panels.find((p) => p.dataset.dtab === "facts");
  assert.deepEqual(
    tagged(facts, "tbody")[0].children.map((r) => r.children[0].textContent),
    ["second", "first"],
  );
  for (const text of [
    "Tuple nodes are reified",
    "signature:",
    "constraints:",
    "Selected path",
    "evidence",
    hostile,
    "Content",
    "Other attributes",
    "Axi metadata",
    "Overlay attributes",
    "DB describe unavailable through the read-only server",
  ])
    assert.ok(ctx.detailEl.textContent.includes(text), text);
  await tagged(facts, "a")[0].fire("click");
  assert.deepEqual(ctx.calls, [[0, false]]);
  await tagged(ctx.detailEl, "button")[3].fire("click");
  assert.equal(ctx.tabs.at(-1), "attrs");
  renderDetail(0);
  assert.ok(ctx.detailEl.textContent.includes("as field"));
  assert.equal(byClass(ctx.detailEl, "detailtabpanel").length, 5);
  renderDetail(999);
  assert.equal(ctx.detailEl.textContent, "");
  renderDetail(1);
  assert.equal(tagged(ctx.detailEl, "h2").length, 1);
});

test("production detail run/document/chunk/theory sections and truncation stay discoverable", (t) => {
  environment(t);
  for (const [type, relation, marker, count] of [
    ["ProposalRun", "run_has_proposal", "Evidence-plane proposals", 201],
    ["Document", "document_has_chunk", "Document evidence chunks", 81],
  ]) {
    const targets = Array.from({ length: count }, (_, i) =>
      node(i + 1, { name: hostile }),
    );
    const ctx = graphContext(
      [
        node(0, {
          entity_type: type,
          attrs: {
            source_locator: "javascript:alert(1)",
            document_id: hostile,
          },
        }),
        ...targets,
      ],
      targets.map((n) => ({ source: 0, target: n.id, label: relation })),
    );
    makeDetailRenderer(ctx).renderDetail(0);
    const facts = byClass(ctx.detailEl, "detailtabpanel").find(
      (p) => p.dataset.dtab === "facts",
    );
    assert.ok(facts.textContent.includes(marker));
    assert.ok(facts.textContent.includes(`of ${count}`));
    assert.equal(tagged(facts, "a").length, count - 1);
    assert.ok(tagged(ctx.detailEl, "a").every((a) => a.href === "#"));
  }
  const chunk = graphContext(
    [
      node(0, {
        entity_type: "DocChunk",
        attrs: { text: hostile.repeat(8), chunk_id: hostile, page: null },
      }),
      node(1),
    ],
    [
      { source: 0, target: 1, label: "doc_chunk_about" },
      { source: 0, target: 1, label: "chunk_in_document" },
    ],
  );
  makeDetailRenderer(chunk).renderDetail(0);
  const facts = byClass(chunk.detailEl, "detailtabpanel").find(
    (p) => p.dataset.dtab === "facts",
  );
  assert.ok(facts.textContent.includes("about:"));
  assert.ok(facts.textContent.includes("document:"));
  assert.equal(tagged(facts, "pre")[0].textContent.length, 281);
  const theory = graphContext(
    [
      node(0, { entity_type: "AxiMetaTheory" }),
      node(1, {
        attrs: {
          axi_constraint_kind: "named_block",
          axi_constraint_name: hostile,
        },
      }),
    ],
    [{ source: 0, target: 1, label: "axi_theory_has_constraint" }],
  );
  makeDetailRenderer(theory).renderDetail(0);
  assert.ok(theory.detailEl.textContent.includes("structured (but opaque)"));
  assert.ok(theory.detailEl.textContent.includes(hostile));
});

test("production DB detail handles errors/malformed boundaries and only builds numeric same-page navigation", async (t) => {
  environment(t);
  const ctx = graphContext([node(0), node(1)]);
  ctx.isServerMode = () => true;
  const { renderDetail } = makeDetailRenderer(ctx);
  renderDetail(0);
  assert.ok(ctx.detailEl.textContent.includes("Loading…"));
  ctx.ui.describeCache.set(0, { status: "error", data: { error: hostile } });
  renderDetail(0);
  assert.ok(tagged(ctx.detailEl, "pre")[0].textContent.includes("<img"));
  ctx.ui.describeCache.set(0, { status: "ready", data: null });
  renderDetail(0);
  assert.ok(ctx.detailEl.textContent.includes("(no data)"));
  ctx.ui.describeCache.set(0, {
    status: "ready",
    data: {
      contexts: [
        null,
        { id: 0, name: hostile },
        { id: 42, name: "outside" },
        ...[hostile, "javascript:alert(1)", "", "2", true, -1, Infinity].map(
          (id) => ({ id, name: hostile }),
        ),
      ],
      equivalences: [{ other: { id: 1, name: hostile }, kind: hostile }],
      outgoing: [
        {
          rel: hostile,
          count: 2,
          edges: [{ entity: { id: 1, name: hostile }, confidence: 0.7 }, null],
        },
        null,
      ],
      incoming: "malformed",
    },
  });
  renderDetail(0);
  const db = byClass(ctx.detailEl, "detailtabpanel").find(
    (p) => p.dataset.dtab === "db",
  );
  const links = tagged(db, "a");
  assert.equal(links.length, 4);
  assert.ok(links.every((a) => a.href === "#"));
  await links[0].fire("click");
  assert.deepEqual(ctx.calls, [[0, false]]);
  await links[1].fire("click");
  const params = new URLSearchParams(window.location.search);
  assert.equal(params.get("focus_id"), "42");
  assert.equal(params.get("snapshot"), "fixture");
  assert.equal(params.has("focus_name"), false);
  assert.ok(db.textContent.includes(hostile));
});

function overlay(count = 2) {
  return {
    proposals_json: {
      version: 1,
      generated_at: "fixture",
      source: { source_type: "conversation", locator: "viz_ui" },
      proposals: Array.from({ length: count }, (_, i) => ({
        kind: "Entity",
        proposal_id: `p${i}`,
        entity_id: `e${i}`,
        entity_type: hostile,
        name: hostile,
        confidence: 0.9,
        public_rationale: "fixture",
        metadata: {},
        attributes: {},
        evidence: [{ chunk_id: `c${i}`, locator: hostile }],
      })),
    },
    chunks: Array.from({ length: count }, (_, i) => ({
      chunk_id: `c${i}`,
      document_id: "doc",
      page: null,
      bbox: null,
      span_id: "",
      text: hostile,
      metadata: {},
    })),
    validation: { ok: true },
  };
}
function freeze(value) {
  if (value && typeof value === "object") {
    Object.freeze(value);
    for (const child of Object.values(value)) freeze(child);
  }
  return value;
}
function reviewContext() {
  const ctx = {
    ui: { draftSelected: new Set() },
    isServerMode: () => true,
    setActiveTab: (tab) => ctx.tabs.push(tab),
    tabs: [],
    currentContextNameFromFilter: () => null,
    rerender: () => {},
    updateAddConfidenceLabel: () => {},
  };
  for (const key of [
    "reviewFilterEl",
    "reviewSelectAllBtn",
    "reviewSelectNoneBtn",
    "reviewClearBtn",
    "reviewCommitBtn",
    "reviewDraftAxiBtn",
    "reviewPromoteAxiBtn",
    "reviewListEl",
    "reviewStatusEl",
    "reviewValidationEl",
    "reviewOverlayRawEl",
    "reviewCommitOutputEl",
    "reviewPromoteOutputEl",
    "reviewAxiTextEl",
    "reviewMessageEl",
    "reviewAdminTokenEl",
    "addStatusEl",
    "addOutputEl",
    "addCommitOutputEl",
    "addPromoteOutputEl",
    "addPromoteStatusEl",
    "addAxiTextEl",
    "llmCitationsEl",
  ])
    ctx[key] = new ElementSpy();
  Object.assign(ctx, initStatus(ctx));
  Object.assign(ctx, initDraft(ctx)); // real app order, before LLM/add
  return ctx;
}

test("production status and draft review render rich literal text, filter/select/clear and reinitialize", async (t) => {
  environment(t);
  const ctx = reviewContext();
  assert.ok(ctx.reviewListEl.textContent.includes("No local draft overlay"));
  assert.match(ctx.reviewListEl.textContent, /generation and accepted changes require the CLI/);
  assert.equal(ctx.setDraftOverlay(overlay(), { notePrefix: hostile }), true);
  assert.deepEqual(ctx.tabs, ["review"]);
  assert.ok(ctx.addOutputEl.textContent.includes("proposals_json"));
  ctx.setReviewActionStatus(hostile);
  assert.ok(ctx.reviewStatusEl.textContent.includes(hostile));
  assert.equal(byClass(ctx.reviewStatusEl, "chip")[0].textContent, "validated");
  assert.ok(!ctx.reviewStatusEl.textContent.includes("accepted"));
  const checkbox = tagged(ctx.reviewListEl, "input")[0];
  checkbox.checked = false;
  await checkbox.fire("change");
  assert.equal(ctx.reviewStatusEl.textContent.startsWith("1/2 selected"), true);
  assert.deepEqual(
    ctx
      .currentDraftFiltered()
      .proposals_json.proposals.map((p) => p.proposal_id),
    ["p1"],
  );
  await ctx.reviewSelectNoneBtn.fire("click");
  assert.equal(ctx.currentDraftFiltered().proposals_json.proposals.length, 0);
  await ctx.reviewSelectAllBtn.fire("click");
  assert.equal(ctx.ui.draftSelected.size, 2);
  ctx.reviewFilterEl.value = "absent";
  await ctx.reviewFilterEl.fire("input");
  assert.ok(ctx.reviewListEl.textContent.includes("No proposals match"));
  assert.equal(ctx.ui.draftSelected.size, 2);
  ctx.reviewFilterEl.value = "";
  ctx.setDraftOverlay(overlay(251));
  assert.equal(tagged(ctx.reviewListEl, "input").length, 250);
  assert.ok(ctx.reviewListEl.textContent.includes("Showing 250 of 251"));
  for (const [validation, label] of [
    [{ ok: false }, "invalid"],
    [undefined, "unvalidated"],
  ]) {
    ctx.setDraftOverlay({ ...overlay(), validation });
    assert.equal(byClass(ctx.reviewStatusEl, "chip")[0].textContent, label);
  }
  await ctx.reviewClearBtn.fire("click");
  assert.equal(ctx.currentDraftFiltered(), null);
  assert.equal(ctx.reviewStatusEl.textContent, "(no draft overlay)");
  ctx.setDraftOverlay(overlay());
  assert.equal(tagged(ctx.reviewListEl, "input").length, 2);
  ctx.setReviewStatus(hostile);
  assert.equal(ctx.reviewStatusEl.textContent, hostile);
  ctx.setReviewStatus();
  assert.equal(ctx.reviewStatusEl.textContent, "");
  initStatus({}).setReviewStatus(hostile);
});

test("production tool-loop prefill uses the initialized draft callback without auto-commit or requests", (t) => {
  environment(t);
  const ctx = reviewContext();
  const setOverlay = t.mock.method(ctx, "setDraftOverlay");
  const draft = overlay();
  assert.equal(
    ctx.prefillAddFromToolLoop({ artifacts: { generated_overlay: draft } }),
    true,
  );
  assert.equal(setOverlay.mock.calls.length, 1);
  assert.equal(ctx.ui.draftOverlay, draft);
  assert.equal(ctx.ui.draftSelected.size, 2);
  assert.deepEqual(ctx.tabs, ["review"]);
  assert.equal(ctx.reviewCommitOutputEl.textContent, "");
  assert.equal(ctx.reviewPromoteOutputEl.textContent, "");
  const latest = overlay(1);
  assert.equal(
    ctx.prefillAddFromToolLoop({
      steps: [
        { tool: "predictive_proposal", result: draft },
        { tool: "proposal_rollout_plan", result: latest },
      ],
    }),
    true,
  );
  assert.equal(ctx.ui.draftOverlay, latest);
  assert.equal(setOverlay.mock.calls.length, 2);
  assert.equal(ctx.prefillAddFromToolLoop(null), false);
  assert.equal(
    ctx.prefillAddFromToolLoop({
      steps: [null, { tool: "unknown", result: draft }],
    }),
    false,
  );
  const before = ctx.addOutputEl.textContent;
  ctx.setDraftOverlay = undefined;
  assert.equal(
    ctx.prefillAddFromToolLoop({ artifacts: { generated_overlay: draft } }),
    false,
  );
  assert.match(ctx.reviewStatusEl.textContent, /unavailable.*reload/);
  assert.equal(ctx.addOutputEl.textContent, before);
  ctx.setDraftOverlay = () => {
    throw new Error("fixture failure");
  };
  assert.equal(
    ctx.prefillAddFromToolLoop({ artifacts: { generated_overlay: draft } }),
    false,
  );
  assert.match(ctx.reviewStatusEl.textContent, /fixture failure/);
});

function llmAskContext(t) {
  const ctx = reviewContext();
  for (const key of [
    "llmQuestionEl",
    "llmStatusEl",
    "llmAskBtn",
    "llmAutoCommitEl",
    "llmCertifyEl",
    "llmVerifyEl",
    "llmRequireVerifiedEl",
    "addAdminTokenEl",
    "addMessageEl",
  ])
    ctx[key] = new ElementSpy();
  ctx.llmQuestionEl.value = "Generate an evidence overlay";
  ctx.selectedContextFilter = () => "7";
  ctx.ui.highlightIds = new Set([99]);
  ctx.continuation = [];
  ctx.clearHighlights = t.mock.fn(() => {
    ctx.continuation.push("clear");
    ctx.ui.highlightIds.clear();
  });
  ctx.highlightFromToolLoop = t.mock.fn(() => {
    ctx.continuation.push("highlight");
    ctx.ui.highlightIds.add(7);
  });
  ctx.rerender = t.mock.fn(() => ctx.continuation.push("render"));
  Object.assign(ctx, initLlmTab(ctx));
  return ctx;
}

test("production LLM callbacks and keyboard cannot call unsupported endpoints or mutate local draft/highlights", async (t) => {
  const stored = environment(t);
  stored.set("axiograph_llm_auto_commit", "true");
  stored.set("axiograph_llm_require_verified", "true");
  t.mock.method(globalThis, "fetch", () => assert.fail("no LLM endpoint"));
  const ctx = llmAskContext(t);
  const draft = freeze(overlay());
  ctx.setDraftOverlay(draft);
  assert.equal(ctx.llmAskBtn.disabled, true);
  assert.equal(ctx.llmRequireVerifiedEl.disabled, true);
  assert.equal(ctx.llmAutoCommitEl.checked, false);
  await ctx.llmAskBtn.fire("click");
  await ctx.llmQuestionEl.fire("keydown", { key: "Enter" });
  assert.match(ctx.llmStatusEl.textContent, /Unavailable.*read-only/);
  assert.equal(ctx.ui.draftOverlay, draft);
  assert.deepEqual([...ctx.ui.highlightIds], [99]);
  assert.deepEqual(ctx.continuation, []);
});

test("production selection is nonmutating, retains wire metadata/nulls, and rejects malformed selection boundaries", () => {
  const input = freeze(overlay());
  const before = JSON.stringify(input);
  const selected = new Set(["p1"]);
  const result = selectDraft(input, selected);
  assert.notEqual(result.proposals_json, input.proposals_json);
  assert.notEqual(result.chunks, input.chunks);
  assert.equal(
    result.proposals_json.proposals[0],
    input.proposals_json.proposals[1],
  );
  assert.equal(result.chunks[0], input.chunks[1]);
  assert.equal(result.chunks[0].page, null);
  assert.equal(result.proposals_json.source, input.proposals_json.source);
  assert.equal(JSON.stringify(input), before);
  assert.deepEqual([...selected], ["p1"]);
  assert.equal(selectDraft(input, new Set()).chunks.length, 2); // original keep-all-if-no-references rule
  const noEvidence = overlay();
  noEvidence.proposals_json.proposals[0].evidence = [];
  assert.equal(selectDraft(noEvidence, new Set(["p0"])).chunks.length, 2);
  for (const bad of [
    false,
    {},
    { proposals_json: {} },
    { proposals_json: { proposals: [null] } },
    { ...overlay(), chunks: null },
    { ...overlay(), chunks: [null] },
  ])
    assert.throws(() => selectDraft(bad, new Set()), /Invalid draft/);
  for (const patch of [
    { proposal_id: "" },
    { proposal_id: 1 },
    { evidence: null },
    { evidence: [null] },
    { evidence: [{}] },
  ]) {
    const bad = overlay();
    Object.assign(bad.proposals_json.proposals[0], patch);
    assert.throws(() => selectDraft(bad, new Set()), /Invalid draft/); // even unselected malformed proposals reject
  }
  const duplicate = overlay();
  duplicate.proposals_json.proposals[1].proposal_id = "p0";
  assert.throws(() => selectDraft(duplicate, new Set()), /duplicate/);
  assert.throws(
    () => selectDraft(input, new Set(["stale"])),
    /unknown proposal_id/,
  );
});

// V2-shaped mock derived from db_server.rs StatusResponseV2; null is deliberately
// NOT a receipt. This tests denial, not server wire conformance or authority.
const readOnlyStatus = {
  format: "axiograph_authenticated_pathdb_status_v2",
  loaded_at_unix_secs: 1,
  entities: 2,
  relations: 1,
  receipt: null,
};

test("real draft-before-add wiring preserves selection and blocks unsupported mutations", async (t) => {
  environment(t);
  const ctx = reviewContext();
  Object.assign(ctx, initAddTab(ctx));
  ctx.setDraftOverlay(freeze(overlay()));
  ctx.ui.draftSelected = new Set(["p1"]);
  const calls = [];
  t.mock.method(globalThis, "fetch", async (url, options) => {
    calls.push([url, options?.body ? JSON.parse(options.body) : null]);
    return {
      ok: true,
      json: async () =>
        url === "/status"
          ? readOnlyStatus
          : url === "/discover/draft-axi"
            ? { axi_text: "module Fixture" }
            : {},
    };
  });
  const original = JSON.stringify(ctx.ui.draftOverlay);
  await ctx.reviewCommitBtn.fire("click");
  assert.deepEqual(
    calls.map((c) => c[0]),
    [],
  );
  assert.match(ctx.reviewStatusEl.textContent, /commit blocked.*read-only/);
  assert.equal(ctx.reviewCommitBtn.textContent, "commit (CLI only)");
  assert.equal(ctx.reviewPromoteAxiBtn.textContent, "promote (CLI only)");
  calls.length = 0;
  await ctx.reviewDraftAxiBtn.fire("click");
  assert.deepEqual(
    calls.map((c) => c[0]),
    [],
  );
  assert.match(ctx.reviewStatusEl.textContent, /Unavailable.*read-only/);
  assert.equal(ctx.reviewAxiTextEl.value, "");
  assert.equal(ctx.addAxiTextEl.value, "");
  calls.length = 0;
  await ctx.reviewPromoteAxiBtn.fire("click");
  assert.deepEqual(
    calls.map((c) => c[0]),
    [],
  );
  assert.match(ctx.reviewStatusEl.textContent, /promote blocked.*read-only/);
  // Neither status/role metadata nor the selected draft enables HTTP actions.
  assert.equal(JSON.stringify(ctx.ui.draftOverlay), original);
  assert.deepEqual([...ctx.ui.draftSelected], ["p1"]);
});

test("V2, legacy roles, malformed and unavailable status never enable an admin POST", async (t) => {
  environment(t);
  const ctx = reviewContext();
  Object.assign(ctx, initAddTab(ctx));
  ctx.setDraftOverlay(freeze(overlay()));
  ctx.reviewAxiTextEl.value = "retain canonical draft";
  ctx.reviewAdminTokenEl.value = "test-only-not-a-credential";
  const outputs = [
    ctx.addOutputEl,
    ctx.addCommitOutputEl,
    ctx.reviewCommitOutputEl,
    ctx.addPromoteOutputEl,
    ctx.reviewPromoteOutputEl,
    ctx.reviewOverlayRawEl,
    ctx.reviewValidationEl,
  ];
  const before = outputs.map((e) => [e.textContent, e.writes]);
  const calls = [];
  let respond;
  t.mock.method(globalThis, "fetch", async (url, options) => {
    calls.push(url);
    assert.equal(url, "/status");
    assert.equal(options.cache, "no-store");
    assert.equal(options.method, undefined);
    assert.equal(options.body, undefined);
    return respond();
  });
  const ok = (value) => async () => ({ ok: true, json: async () => value });
  for (const response of [
    ok(readOnlyStatus),
    ok({
      ...readOnlyStatus,
      receipt: { test_only: "not authority" },
      role: "master",
    }),
    ok({ role: "master" }),
    ok({ role: "replica" }),
    ok({}),
    ok(null),
    ok([]),
    ok("malformed"),
    async () => {
      throw new Error("connection refused");
    },
    async () => ({
      ok: false,
      status: 503,
      json: () => assert.fail("do not parse unavailable status"),
    }),
    async () => ({
      ok: true,
      json: async () => {
        throw new SyntaxError("invalid JSON");
      },
    }),
  ]) {
    respond = response;
    calls.length = 0;
    for (const [button, action] of [
      [ctx.reviewCommitBtn, "commit"],
      [ctx.reviewPromoteAxiBtn, "promote"],
    ]) {
      await button.fire("click");
      assert.ok(
        ctx.reviewStatusEl.textContent.startsWith(`${action} blocked:`),
      );
      assert.match(ctx.reviewStatusEl.textContent, /axiograph check --help/);
      assert.match(ctx.reviewStatusEl.textContent, /AxiStore CLI/);
    }
    assert.deepEqual(calls, []);
    assert.deepEqual(
      outputs.map((e) => [e.textContent, e.writes]),
      before,
    );
    assert.equal(ctx.reviewAxiTextEl.value, "retain canonical draft");
    assert.equal(ctx.ui.draftSelected.size, 2);
  }
  calls.length = 0;
  const offline = reviewContext();
  offline.isServerMode = () => false;
  Object.assign(offline, initAddTab(offline));
  offline.setDraftOverlay(overlay());
  await offline.reviewCommitBtn.fire("click");
  await offline.reviewPromoteAxiBtn.fire("click");
  assert.equal(calls.length, 0);
  assert.match(offline.reviewStatusEl.textContent, /offline visualization/);
});

test("missing handlers and selector errors are actionable, with zero requests or output mutation", async (t) => {
  environment(t);
  const ctx = reviewContext();
  ctx.setDraftOverlay(overlay());
  const outputs = [
    ctx.addOutputEl,
    ctx.addCommitOutputEl,
    ctx.reviewCommitOutputEl,
    ctx.addPromoteOutputEl,
    ctx.reviewPromoteOutputEl,
    ctx.reviewOverlayRawEl,
    ctx.reviewValidationEl,
  ];
  const before = outputs.map((e) => [e.textContent, e.writes]);
  for (const button of [
    ctx.reviewCommitBtn,
    ctx.reviewDraftAxiBtn,
    ctx.reviewPromoteAxiBtn,
  ])
    await button.fire("click");
  assert.match(ctx.reviewStatusEl.textContent, /unavailable.*reload/);
  await tagged(ctx.reviewListEl, "button")[0].fire("click");
  assert.match(ctx.reviewStatusEl.textContent, /Evidence lookup unavailable/);
  assert.deepEqual(
    outputs.map((e) => [e.textContent, e.writes]),
    before,
  );
  Object.assign(ctx, initAddTab(ctx));
  ctx.ui.draftOverlay = { proposals_json: { proposals: [null] } };
  ctx.reviewAxiTextEl.value = "keep draft";
  ctx.addAxiTextEl.value = "keep add";
  await ctx.reviewCommitBtn.fire("click");
  await ctx.reviewDraftAxiBtn.fire("click");
  assert.match(
    ctx.reviewStatusEl.textContent,
    /Cannot use draft selection.*Invalid draft/,
  );
  assert.deepEqual(
    outputs.map((e) => [e.textContent, e.writes]),
    before,
  );
  assert.equal(ctx.reviewAxiTextEl.value, "keep draft");
  assert.equal(ctx.addAxiTextEl.value, "keep add");
  const missing = reviewContext();
  missing.currentDraftFiltered = undefined;
  Object.assign(missing, initAddTab(missing));
  await missing.reviewCommitBtn.fire("click");
  await missing.reviewDraftAxiBtn.fire("click");
  assert.match(
    missing.reviewStatusEl.textContent,
    /Draft selection unavailable/,
  );
});

test("review evidence callback is local-only even with hostile locators and colliding IDs", async (t) => {
  environment(t);
  const ctx = reviewContext();
  Object.assign(ctx, initLlmTab(ctx));
  const draft = overlay();
  draft.proposals_json.proposals[0].evidence[0].chunk_id = hostile;
  ctx.setDraftOverlay(draft);
  t.mock.method(globalThis, "fetch", () => assert.fail("no evidence endpoint"));
  ctx.ui.highlightIds = new Set([99]);
  const before = [...ctx.ui.highlightIds];
  await tagged(ctx.reviewListEl, "button")[0].fire("click");
  assert.deepEqual([...ctx.ui.highlightIds], before);
  assert.match(ctx.llmCitationsEl.textContent, /Unavailable.*evidence lookup/);
});
