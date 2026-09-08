import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { build } from "esbuild";
const { outputFiles } = await build({ stdin: { contents: `export * from './src/server/read-only-client'; export * from './src/tabs/query'; export * from './src/tabs/predictive_proposals'; export * from './src/core/describe';`, resolveDir: new URL("../", import.meta.url).pathname }, bundle: true, write: false, format: "esm", platform: "node" });
const production = await import(`data:text/javascript;base64,${Buffer.from(outputFiles[0].text).toString("base64")}`);
const { validateCapabilities, validateStatus, validateQueryResponse, serializeFiniteQuery, readBoundedJson, ReadOnlyClient, LatestQuery, QUERY_EXAMPLE, initQueryTab, initPredictiveProposalTab, initDescribe } = production;
const api = JSON.parse(await readFile(new URL("../src/server/read-only-api.json", import.meta.url)));
const caps = { api, query_schema: { type: "object", properties: { version: { const: 1 } } }, ui_available: true };
// Opaque metadata here deliberately is NOT a usable receipt. Real receipt
// validation is exercised by db_server_e2e + read-only-client-live.mjs.
const status = { format: "axiograph_authenticated_pathdb_status_v2", loaded_at_unix_secs: 1, entities: 2, relations: 0, receipt: { test_only: "opaque, not authority" } };
const response = { family: "compiled_finite_query", result: { selected_vars: ["entity"], rows: [{ entity: 0 }], truncated: false }, trust: { trust_class: "certifiable", soundness: "certificate_available_but_not_emitted", coverage: "full_query", scope: { anchor: "snapshot_scoped", context: "unscoped" }, claim_scope: "finite_query_denotation_within_exact_accepted_module", completeness_claim: "not_claimed", ontology_closure_claim: "not_claimed" }, non_claims: ["no_certificate_without_exact_accepted_axi_bytes", "no_ontology_closure_claim"] };
const clone = value => structuredClone(value);
const json = (value, status = 200) => new Response(JSON.stringify(value), { status, headers: { "content-type": "application/json" } });

class Control {
  constructor() { this.value = ""; this.textContent = ""; this.listeners = new Map(); }
  addEventListener(event, fn) { this.listeners.set(event, fn); }
  fire(event, fields = {}) { return this.listeners.get(event)?.({ preventDefault() {}, ...fields }); }
}
test("closed versioned capabilities/status reject malformed and authority-looking metadata envelopes", () => {
  assert.equal(validateCapabilities(caps).ui_available, true);
  assert.deepEqual(validateStatus(status), status);
  for (const change of [v => v.api.format = "future", v => v.api.unsupported = [], v => v.api.endpoints[3].path = "https://evil.invalid", v => v.ui_available = "yes", v => v.extra = 1, v => v.query_schema.properties.version.const = 2]) { const v = clone(caps); change(v); assert.throws(() => validateCapabilities(v)); }
  for (const patch of [{ role: "master" }, { format: "future" }, { entities: -1 }, { entities: "2" }, { relations: Infinity }, { loaded_at_unix_secs: 1.5 }, { receipt: null }, { receipt: [] }]) assert.throws(() => validateStatus({ ...status, ...patch }));
});
test("complete finite result/trust shape validates before any presentation, no verified relabeling", () => {
  assert.deepEqual(validateQueryResponse(response), response);
  for (const change of [v => v.family = "legacy", v => v.source_identity = "other", v => v.result.rows[0].entity = 0x100000000, v => v.result.rows[0].entity = -1, v => v.result.rows[0].entity = "0", v => v.result.rows[0].extra = 1, v => v.result.rows = [{}], v => v.result.selected_vars = ["entity", "entity"], v => v.result.truncated = "false", v => v.trust.soundness = "lean_verified_finite_exact_complete", v => v.trust.completeness_claim = "exact", v => v.trust.scope.anchor = "unrelated_image", v => v.trust.trust_class = "unknown", v => v.trust.gaps = [null], v => v.trust.notes = [2], v => delete v.non_claims, v => v.non_claims = []]) { const v = clone(response); change(v); assert.throws(() => validateQueryResponse(v)); }
});
test("production serializer only wraps IR; Rust owns atom parsing/typing and semantic rejection", () => {
  assert.deepEqual(JSON.parse(serializeFiniteQuery(JSON.stringify(QUERY_EXAMPLE))), { query: QUERY_EXAMPLE });
  assert.throws(() => serializeFiniteQuery("select ?x where ..."));
  assert.throws(() => serializeFiniteQuery('{"version":2}'));
  assert.throws(() => serializeFiniteQuery('"legacy"'));
  assert.throws(() => serializeFiniteQuery(" ".repeat(1048577)), /oversized/);
  // Deliberately pass unknown semantic input to Rust, not a JS query parser.
  assert.deepEqual(JSON.parse(serializeFiniteQuery('{"version":1,"where_atoms":[{"kind":"unknown"}]}')).query.where_atoms, [{ kind: "unknown" }]);
});
test("streamed JSON rejects absent/dishonest length, malformed UTF8/JSON, depth, HTTP errors", async () => {
  assert.deepEqual(await readBoundedJson(json({ x: 1 })), { x: 1 });
  for (const r of [new Response("{}"), new Response("{", { headers: { "content-type": "application/json" } }), json({ error: "<img onerror=evil>" }, 400), new Response(new Uint8Array([255]), { headers: { "content-type": "application/json" } }), new Response("[".repeat(66) + "0" + "]".repeat(66), { headers: { "content-type": "application/json" } })]) await assert.rejects(readBoundedJson(r));
  for (const length of [null, "1", "900", "invalid"]) {
    const headers = { "content-type": "application/json" }; if (length !== null) headers["content-length"] = length;
    const stream = new ReadableStream({ start(c) { c.enqueue(new Uint8Array(20)); c.enqueue(new Uint8Array(20)); c.close(); } });
    await assert.rejects(readBoundedJson(new Response(stream, { headers }), 32));
  }
});
test("client discovery fails closed, fixed requests cannot restore unsupported endpoints", async () => {
  const calls = [];
  const client = new ReadOnlyClient(async (path, init) => { calls.push([path, init]); return json(path === "/capabilities" ? caps : path === "/status" ? status : response); });
  await assert.rejects(client.query("{}"), /not established/);
  await client.discover();
  await client.query(JSON.stringify(QUERY_EXAMPLE));
  assert.deepEqual(calls.map(([p]) => p), ["/capabilities", "/status", "/query"]);
  assert.deepEqual(JSON.parse(calls[2][1].body), { query: QUERY_EXAMPLE });
  assert.equal(calls[2][1].redirect, "error");
  const unknown = new ReadOnlyClient(async () => json({ ...caps, ui_available: "false" }));
  await assert.rejects(unknown.discover()); await assert.rejects(unknown.query("{}"));
});
test("stale successes/errors/editor invalidation cannot overwrite latest query", async () => {
  const latest = new LatestQuery(); const pending = []; const outcomes = [];
  const client = { query: () => new Promise((resolve, reject) => pending.push({ resolve, reject })) };
  const run = () => latest.run(client, "fixture", v => outcomes.push(v), e => outcomes.push(String(e)));
  const first = run(); const second = run();
  pending[1].resolve("new"); await second; pending[0].resolve("old"); await first; assert.deepEqual(outcomes, ["new"]);
  const third = run(); latest.invalidate(); pending[2].reject("obsolete error"); await third; assert.deepEqual(outcomes, ["new"]);
});
test("query tab stays typed, visibly rejects malformed replies, never touches colliding offline IDs", async () => {
  const ctx = Object.fromEntries(["axqlQueryEl", "axqlRunBtn", "axqlCertBtn", "axqlVerifyBtn", "axqlStatusEl", "axqlOutputEl", "certOutputEl"].map(k => [k, new Control()]));
  const graph = { nodes: [{ id: 0, name: "unrelated offline node" }], highlights: [99] };
  ctx.clearHighlights = () => assert.fail("no cross-image highlights");
  ctx.highlightFromQueryResponse = ctx.clearHighlights;
  const tab = initQueryTab(ctx);
  assert.equal(ctx.axqlRunBtn.disabled, true); assert.equal(ctx.axqlCertBtn.disabled, true);
  await ctx.axqlRunBtn.fire("click"); assert.match(ctx.axqlStatusEl.textContent, /unavailable/);
  let reply = response;
  const client = new ReadOnlyClient(async p => json(p === "/capabilities" ? caps : p === "/status" ? status : reply));
  await client.discover(); tab.setQueryClient(client);
  await ctx.axqlRunBtn.fire("click");
  const previous = ctx.axqlOutputEl.textContent;
  assert.match(ctx.axqlStatusEl.textContent, /Graph highlighting unavailable/);
  reply = { ...response, source_identity: "foreign" };
  await ctx.axqlRunBtn.fire("click");
  assert.equal(ctx.axqlOutputEl.textContent, previous); assert.match(ctx.axqlStatusEl.textContent, /failed.*Previous successful output retained/);
  assert.deepEqual(graph.highlights, [99]);
  ctx.axqlQueryEl.fire("input"); assert.match(ctx.axqlStatusEl.textContent, /stale/);
});
test("automatic describe and predictive callback/keyboard controls have no remote path", async t => {
  t.mock.method(globalThis, "fetch", () => assert.fail("unsupported request"));
  const ctx = { ui: {}, proposalProposeBtn: new Control(), proposalPlanBtn: new Control(), setPredictiveProposalStatus: v => ctx.status = v, setPredictiveProposalOutput() {} };
  initPredictiveProposalTab(ctx);
  assert.equal(ctx.proposalProposeBtn.disabled, true);
  await ctx.proposalProposeBtn.fire("click"); await ctx.proposalPlanBtn.fire("click"); assert.match(ctx.status, /Unavailable/);
  const describe = initDescribe(ctx); await describe.fetchDescribeEntity(0); assert.match(ctx.ui.describeCache.get(0).data.error, /Unavailable/);
});

test("older discovery cannot enable queries after a newer malformed discovery", async () => {
  const pending = [];
  const client = new ReadOnlyClient(path => new Promise(resolve => pending.push({ path, resolve })));
  const old = client.discover();
  const current = client.discover();
  pending[1].resolve(json({ ...caps, ui_available: "invalid" }));
  await assert.rejects(current);
  pending[0].resolve(json(caps));
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(pending[2].path, "/status");
  pending[2].resolve(json(status));
  await assert.rejects(old, /stale/);
  await assert.rejects(client.query(JSON.stringify(QUERY_EXAMPLE)), /not established/);
});
