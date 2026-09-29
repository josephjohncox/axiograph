import assert from "node:assert/strict";
import test from "node:test";
import { build } from "esbuild";

const { outputFiles } = await build({
  stdin: { contents: "export * from './src/types';", resolveDir: new URL("../", import.meta.url).pathname },
  bundle: true,
  write: false,
  format: "esm",
  platform: "node",
});
const {
  DRAFT_JSON_BUDGET,
  GRAPH_FORMAT,
  GRAPH_JSON_BUDGET,
  isDraftOverlay,
  isGraphPayload,
  parseDraftOverlayJson,
  parseGraphPayloadJson,
  parseLlmHistoryJson,
  validateDraftOverlay,
  validateGraphPayload,
  validateLlmHistoryEnvelope,
} = await import(`data:text/javascript;base64,${Buffer.from(outputFiles[0].text).toString("base64")}`);

function graph(nodeCount = 1) {
  return {
    format: GRAPH_FORMAT,
    nodes: Array.from({ length: nodeCount }, (_, id) => ({
      id,
      entity_type: "Thing",
      kind: "entity",
      plane: "accepted",
      name: id === 0 ? "<literal>" : `node-${id}`,
      attrs: {},
    })),
    edges: [],
    truncated: false,
    summary: {
      focus_ids: nodeCount ? [0] : [],
      all_nodes: true,
      hops: 2,
      max_nodes: 1000,
      max_edges: 4000,
      include_meta_plane: false,
      include_data_plane: true,
      include_equivalences: true,
      typed_overlay: false,
    },
    contexts: [],
    tuple_contexts: {},
  };
}

function overlay(proposalCount = 1) {
  return {
    proposals_json: {
      version: 1,
      generated_at: "fixture",
      source: { source_type: "conversation", locator: "test" },
      proposals: Array.from({ length: proposalCount }, (_, index) => ({
        kind: "Entity",
        proposal_id: `p${index}`,
        confidence: 0.9,
        evidence: [{ chunk_id: `c${index}`, locator: "page 1" }],
        public_rationale: "fixture",
        metadata: {},
        entity_id: `e${index}`,
        entity_type: "Thing",
        name: `name-${index}`,
        attributes: {},
      })),
    },
    chunks: Array.from({ length: proposalCount }, (_, index) => ({
      chunk_id: `c${index}`,
      document_id: "document",
      page: null,
      span_id: "",
      text: "evidence",
      bbox: null,
      metadata: {},
    })),
    validation: { ok: true, receipt: { opaque: "not authority" } },
  };
}

const clone = (value) => structuredClone(value);

test("closed graph boundary keeps only one finite local graph image", () => {
  const accepted = graph();
  assert.equal(isGraphPayload(accepted), true);
  assert.deepEqual(validateGraphPayload(accepted), accepted);
  assert.deepEqual(parseGraphPayloadJson(JSON.stringify(accepted)), accepted);
  const mutations = [
    (value) => { value.format = "future"; },
    (value) => { value.extra = true; },
    (value) => { value.nodes[0].extra = true; },
    (value) => { value.nodes[0].id = "0"; },
    (value) => { value.nodes[0].id = -1; },
    (value) => { value.nodes.push({ ...value.nodes[0] }); },
    (value) => { value.nodes[0].attrs = []; },
    (value) => { value.nodes[0].attrs = { key: null }; },
    (value) => { value.edges = [{ source: 0, target: 9, label: "foreign", kind: "relation" }]; },
    (value) => { value.summary.focus_ids = [9]; },
    (value) => { value.summary.focus_ids = [0, 0]; },
    (value) => { value.contexts = [{ id: 1, name: "world" }, { id: 1, name: "duplicate" }]; },
    (value) => { value.tuple_contexts = { "00": [] }; },
    (value) => { value.tuple_contexts = { 0: [9] }; },
  ];
  for (const mutate of mutations) {
    const value = clone(accepted);
    mutate(value);
    assert.equal(isGraphPayload(value), false);
    assert.throws(() => validateGraphPayload(value));
  }
});

test("closed draft boundary validates proposals, evidence and chunks without interpreting receipts", () => {
  const accepted = overlay();
  assert.equal(isDraftOverlay(accepted), true);
  const validated = validateDraftOverlay(accepted);
  assert.notEqual(validated, accepted);
  assert.notEqual(validated.proposals_json, accepted.proposals_json);
  assert.notEqual(validated.proposals_json.proposals, accepted.proposals_json.proposals);
  assert.notEqual(validated.proposals_json.proposals[0], accepted.proposals_json.proposals[0]);
  assert.notEqual(validated.proposals_json.proposals[0].metadata, accepted.proposals_json.proposals[0].metadata);
  assert.notEqual(validated.chunks, accepted.chunks);
  assert.notEqual(validated.validation, accepted.validation);
  assert.deepEqual(validated, accepted);
  assert.deepEqual(parseDraftOverlayJson(JSON.stringify(accepted)), accepted);
  const mutations = [
    (value) => { value.extra = true; },
    (value) => { value.proposals_json.version = 2; },
    (value) => { value.proposals_json.extra = true; },
    (value) => { value.proposals_json.source.extra = true; },
    (value) => { value.proposals_json.proposals[0].extra = true; },
    (value) => { value.proposals_json.proposals[0].proposal_id = ""; },
    (value) => { value.proposals_json.proposals[0].confidence = 2; },
    (value) => { value.proposals_json.proposals[0].evidence[0].chunk_id = ""; },
    (value) => { value.proposals_json.proposals.push(clone(value.proposals_json.proposals[0])); },
    (value) => { value.chunks[0].extra = true; },
    (value) => { value.chunks[0].bbox = [0, 1, 2]; },
    (value) => { value.chunks.push(clone(value.chunks[0])); },
    (value) => { value.validation.ok = "yes"; },
  ];
  for (const mutate of mutations) {
    const value = clone(accepted);
    mutate(value);
    assert.equal(isDraftOverlay(value), false);
  }
});

test("parsed prototype keys remain own data in opaque records and string maps", () => {
  const graphInput = graph();
  graphInput.nodes[0].attrs = JSON.parse('{"__proto__":"graph attribute"}');
  const parsedGraph = parseGraphPayloadJson(JSON.stringify(graphInput));
  assert.equal(Object.hasOwn(parsedGraph.nodes[0].attrs, "__proto__"), true);
  assert.equal(parsedGraph.nodes[0].attrs.__proto__, "graph attribute");
  assert.equal(Object.getPrototypeOf(parsedGraph.nodes[0].attrs), Object.prototype);

  const draftInput = overlay();
  draftInput.proposals_json.proposals[0].metadata = JSON.parse('{"__proto__":"proposal metadata"}');
  draftInput.proposals_json.proposals[0].attributes = JSON.parse('{"__proto__":"proposal attribute"}');
  draftInput.chunks[0].metadata = JSON.parse('{"__proto__":"chunk metadata"}');
  draftInput.summary = JSON.parse('{"__proto__":{"note":"opaque summary"}}');
  draftInput.validation = JSON.parse('{"__proto__":{"ok":true}}');
  const parsedDraft = parseDraftOverlayJson(JSON.stringify(draftInput));

  for (const [map, expected] of [
    [parsedDraft.proposals_json.proposals[0].metadata, "proposal metadata"],
    [parsedDraft.proposals_json.proposals[0].attributes, "proposal attribute"],
    [parsedDraft.chunks[0].metadata, "chunk metadata"],
  ]) {
    assert.equal(Object.hasOwn(map, "__proto__"), true);
    assert.equal(map.__proto__, expected);
    assert.equal(Object.getPrototypeOf(map), Object.prototype);
  }
  assert.equal(Object.hasOwn(parsedDraft.summary, "__proto__"), true);
  assert.deepEqual(parsedDraft.summary.__proto__, { note: "opaque summary" });
  assert.equal(Object.hasOwn(parsedDraft.validation, "__proto__"), true);
  assert.deepEqual(parsedDraft.validation.__proto__, { ok: true });
  assert.equal(Object.hasOwn(parsedDraft.validation, "ok"), false);
  assert.equal(parsedDraft.validation.ok, undefined);
});

test("validated draft state is isolated from post-validation caller mutation", () => {
  const callerOwned = overlay();
  callerOwned.summary = { nested: { status: "reviewed" } };
  const validated = validateDraftOverlay(callerOwned);

  callerOwned.proposals_json.proposals = null;
  callerOwned.chunks[0].text = "mutated evidence";
  callerOwned.validation.ok = "mutated authority";
  callerOwned.summary.nested.status = "mutated summary";

  assert.equal(validated.proposals_json.proposals.length, 1);
  assert.equal(validated.chunks[0].text, "evidence");
  assert.equal(validated.validation.ok, true);
  assert.deepEqual(validated.summary, { nested: { status: "reviewed" } });
});

test("persisted LLM history uses one closed versioned envelope", () => {
  const accepted = {
    format: "axiograph_llm_history_v2",
    entries: [{ role: "assistant", content: "literal <img>", public_rationale: "", citations: [], queries: [], notes: [] }],
  };
  assert.deepEqual(validateLlmHistoryEnvelope(accepted), accepted);
  assert.deepEqual(parseLlmHistoryJson(JSON.stringify(accepted)), accepted);
  for (const value of [
    accepted.entries,
    { ...accepted, format: "axiograph_llm_history_v1" },
    { ...accepted, format: "future" },
    { ...accepted, extra: true },
    { ...accepted, entries: [{ ...accepted.entries[0], extra: true }] },
    { ...accepted, entries: [{ ...accepted.entries[0], role: "tool" }] },
    { ...accepted, entries: [{ ...accepted.entries[0], citations: [1] }] },
  ]) assert.throws(() => validateLlmHistoryEnvelope(value));
});

test("graph and draft byte limits accept N and reject N+1", () => {
  for (const [value, maximum, parse] of [
    [graph(), GRAPH_JSON_BUDGET.maxBytes, parseGraphPayloadJson],
    [overlay(), DRAFT_JSON_BUDGET.maxBytes, parseDraftOverlayJson],
  ]) {
    const json = JSON.stringify(value);
    const exact = `${json}${" ".repeat(maximum - Buffer.byteLength(json))}`;
    assert.deepEqual(parse(exact), value);
    assert.throws(() => parse(`${exact} `), /exceeds/);
  }
});

test("graph and draft count limits accept N and reject N+1", () => {
  const graphAtLimit = graph(1000);
  assert.equal(isGraphPayload(graphAtLimit), true);
  graphAtLimit.nodes.push({ ...graphAtLimit.nodes[0], id: 1000 });
  assert.equal(isGraphPayload(graphAtLimit), false);
  const draftAtLimit = overlay(1000);
  assert.equal(isDraftOverlay(draftAtLimit), true);
  draftAtLimit.proposals_json.proposals.push({ ...draftAtLimit.proposals_json.proposals[0], proposal_id: "overflow", entity_id: "overflow" });
  assert.equal(isDraftOverlay(draftAtLimit), false);
});
