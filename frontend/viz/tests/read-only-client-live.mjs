// Actual HTTP + production TS client, not a browser/layout test. Called by the
// receipt-bound Rust fixture; also runnable against an operator-started server.
import assert from "node:assert/strict";
import { build } from "esbuild";
const { outputFiles } = await build({ entryPoints: ["src/server/read-only-client.ts"], bundle: true, write: false, format: "esm", platform: "node" });
const { ReadOnlyClient, QUERY_EXAMPLE } = await import(`data:text/javascript;base64,${Buffer.from(outputFiles[0].text).toString("base64")}`);
const base = new URL(process.argv[2]);
assert.equal(base.protocol, "http:");
assert.equal(base.hostname, "127.0.0.1");
const available = process.argv[3] === "ui-available";
const oversizedImage = process.argv[3] === "ui-unavailable";
const requests = [];
const client = new ReadOnlyClient(async (path, init) => {
  assert.ok(["/capabilities", "/status", "/query"].includes(path));
  requests.push([init?.method || "GET", path]);
  return fetch(new URL(path, base), init);
});
const discovery = await client.discover();
assert.equal(discovery.status.entities, 2);
assert.equal(discovery.capabilities.ui_available, available);
assert.ok(discovery.status.receipt.materialization_id);
const page = await fetch(new URL("/viz", base));
assert.equal(page.status, available ? 200 : 503);
const html = await page.text();
if (available) {
  assert.match(html, /id="axiograph_graph"/);
  assert.match(html, /axiograph_read_only_api_v1/);
  assert.match(html, /QueryIrV1 JSON/);
  assert.doesNotMatch(html, /<script[^>]+src=/);
  assert.doesNotMatch(html, /<link[^>]+href=/);
} else assert.match(html, /Visualization unavailable/);
const result = await client.query(JSON.stringify(QUERY_EXAMPLE));
assert.equal(result.result.rows.length, oversizedImage ? 0 : 1);
assert.equal(result.trust.completeness_claim, "not_claimed");
const approximate = { version: 1, select_vars: ["entity"], where_atoms: [{ kind: "attr_contains", term: "?entity", key: "axiograph.value", needle: oversizedImage ? "xx" : "o" }], limit: 10 };
const execution = await client.query(JSON.stringify(approximate));
assert.ok(execution.result.rows.length > 0);
assert.equal(execution.trust.trust_class, "execution_only");
assert.equal(execution.trust.soundness, "runtime_only_no_certificate_claim");
const both = { ...QUERY_EXAMPLE, where_atoms: [{ kind: "type", term: "?entity", type: '{"kind":"object_type","id":"fixture"}' }], limit: 1 };
const truncated = await client.query(JSON.stringify(both));
assert.equal(truncated.result.rows.length, 1);
assert.equal(truncated.result.truncated, true);
await assert.rejects(client.query(JSON.stringify({ ...QUERY_EXAMPLE, limit: 0 })), /HTTP 400/);
for (const body of ["{", JSON.stringify({ query: "select ?x where ..." }), JSON.stringify({ query: QUERY_EXAMPLE, certify: true }), JSON.stringify({ query: { ...QUERY_EXAMPLE, version: 999 } })]) {
  const rejected = await fetch(new URL("/query", base), { method: "POST", body });
  assert.equal(rejected.status, 400);
  assert.equal(typeof (await rejected.json()).error, "string");
}
// Unsupported paths are read-probed only; there is no mutation/LLM POST even in tests.
for (const path of ["/snapshots", "/contexts", "/llm/agent", "/entity/describe", "/discover/draft-axi", "/cert/reachability", "/assets/anything.js", "/viz/anything"])
  assert.equal((await fetch(new URL(path, base))).status, 404);
const health = await fetch(new URL("/healthz", base));
assert.equal(await health.text(), "ok\n");
assert.ok(requests.every(([method, path]) => method === "GET" || path === "/query"));
console.log(JSON.stringify({ scope: "real HTTP + production client; synthetic nonempty logical image, genuine repository-bound materialization receipt; no browser or checker claim", ui_available: available, requests, status: discovery.status, result, execution, truncated }, null, 2));
