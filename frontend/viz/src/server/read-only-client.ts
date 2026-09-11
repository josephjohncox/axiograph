import api from "./read-only-api.json";

export const UNSUPPORTED = "Unavailable: the read-only database API supports status and finite queries, not mutation, certificates, LLM, proposals, evidence lookup or describe. Use the canonical check/authoring and AxiStore CLI workflow.";
export const QUERY_EXAMPLE = { version: 1, select_vars: ["entity"], where_atoms: [{ kind: "attr_eq", term: "?entity", key: "axiograph.value", value: "Alice" }], limit: 10 };
export type JsonObject = Record<string, unknown>;
export interface Status { format: string; loaded_at_unix_secs: number; entities: number; relations: number; receipt: JsonObject }
export interface Capabilities { api: typeof api; query_schema: JsonObject; ui_available: boolean }
export interface QueryResponse {
  family: "compiled_finite_query";
  result: { selected_vars: string[]; rows: Record<string, number>[]; truncated: boolean };
  trust: JsonObject;
  non_claims: string[];
}
export type Transport = (path: string, init?: RequestInit) => Promise<Response>;

function fail(message: string): never { throw new Error(`Read-only API: ${message}`); }
function isJsonObject(value: unknown): value is JsonObject {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}
export function object(value: unknown): JsonObject {
  if (!isJsonObject(value)) fail("expected object");
  return value;
}
function keys(value: JsonObject, required: string[], optional: string[] = []) {
  const present = new Set(Object.keys(value));
  const allowed = new Set([...required, ...optional]);
  if (required.some(k => !present.has(k)) || [...present].some(k => !allowed.has(k))) fail("missing or unknown fields");
}
function text(value: unknown): string { if (typeof value !== "string") fail("expected string"); return value; }
function integer(value: unknown, max = Number.MAX_SAFE_INTEGER): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0 || value > max) fail("invalid integer");
  return value;
}
function boolean(value: unknown): boolean { if (typeof value !== "boolean") fail("expected boolean"); return value; }
function strings(value: unknown): string[] { if (!Array.isArray(value)) fail("expected string array"); return value.map(text); }
function equal(actual: unknown, expected: unknown): boolean {
  if (Array.isArray(expected)) return Array.isArray(actual) && actual.length === expected.length && expected.every((v, i) => equal(actual[i], v));
  if (expected && typeof expected === "object") {
    const e = object(expected);
    if (!actual || typeof actual !== "object" || Array.isArray(actual)) return false;
    const a = object(actual);
    return Object.keys(a).length === Object.keys(e).length && Object.keys(e).every(k => equal(a[k], e[k]));
  }
  return actual === expected;
}

export function validateCapabilities(value: unknown): Capabilities {
  const v = object(value); keys(v, ["api", "query_schema", "ui_available"]);
  if (!equal(v.api, api)) fail("unknown or unsupported capabilities/version");
  const schema = object(v.query_schema);
  if (schema.type !== "object" || object(object(schema.properties).version).const !== 1) fail("unknown query schema version");
  return { api, query_schema: schema, ui_available: boolean(v.ui_available) };
}
export function validateStatus(value: unknown): Status {
  const v = object(value); keys(v, ["format", "loaded_at_unix_secs", "entities", "relations", "receipt"]);
  if (v.format !== "axiograph_authenticated_pathdb_status_v2") fail("unknown status version");
  return { format: v.format, loaded_at_unix_secs: integer(v.loaded_at_unix_secs), entities: integer(v.entities), relations: integer(v.relations), receipt: object(v.receipt) };
}
function validateTrust(value: unknown): JsonObject {
  const v = object(value);
  keys(v, ["trust_class", "soundness", "coverage", "scope", "claim_scope", "completeness_claim", "ontology_closure_claim"], ["reasons", "notes", "certifiable_disjuncts", "execution_only_disjuncts", "semantic_coverage", "semantic_claims", "gaps"]);
  const expected: Record<string, readonly [string, string]> = {
    certifiable: ["certificate_available_but_not_emitted", "full_query"],
    execution_only: ["runtime_only_no_certificate_claim", "runtime_only"],
    mixed: ["runtime_only_no_certificate_claim", "mixed_union_of_branches"],
  };
  const classification = text(v.trust_class);
  const expectation = expected[classification];
  if (!expectation) fail("unknown trust class");
  const [soundness, coverage] = expectation;
  if (v.soundness !== soundness || v.coverage !== coverage || v.claim_scope !== "finite_query_denotation_within_exact_accepted_module" || v.completeness_claim !== "not_claimed" || v.ontology_closure_claim !== "not_claimed") fail("unsupported trust claims");
  const scope = object(v.scope); keys(scope, ["anchor", "context"]);
  if (scope.anchor !== "snapshot_scoped" || !["unscoped", "single_context", "multi_context"].includes(text(scope.context))) fail("unknown trust scope");
  for (const k of ["reasons", "notes"]) if (v[k] !== undefined) strings(v[k]);
  for (const k of ["certifiable_disjuncts", "execution_only_disjuncts"]) if (v[k] !== undefined) integer(v[k]);
  if (classification === "mixed" && (!integer(v.certifiable_disjuncts) || !integer(v.execution_only_disjuncts))) fail("empty mixed branches");
  if (v.semantic_coverage !== undefined) {
    const c = object(v.semantic_coverage);
    const counts = ["in_scope_claims", "runtime_visible_claims", "answer_relevant_claims", "review_only_claims", "unsupported_claims"];
    keys(c, ["coverage_scope", ...counts]); text(c.coverage_scope); counts.forEach(k => integer(c[k]));
  }
  const reportCollections: Array<
    [string, readonly string[], readonly string[]]
  > = [
    ["semantic_claims", ["subject", "kind", "runtime_support", "certification", "status"], []],
    ["gaps", ["code", "detail"], ["subject"]],
  ];
  for (const [field, required, optional] of reportCollections) {
    if (v[field] === undefined) continue;
    if (!Array.isArray(v[field])) fail(`invalid ${field}`);
    for (const entry of v[field]) { const e = object(entry); keys(e, [...required], [...optional]); Object.values(e).forEach(text); }
  }
  return v;
}
export function validateQueryResponse(value: unknown): QueryResponse {
  const v = object(value); keys(v, ["family", "result", "trust", "non_claims"]);
  if (v.family !== "compiled_finite_query") fail("unknown result family");
  if (!equal(v.non_claims, ["no_certificate_without_exact_accepted_axi_bytes", "no_ontology_closure_claim"])) fail("missing non-claims");
  const result = object(v.result); keys(result, ["selected_vars", "rows", "truncated"]);
  const selected_vars = strings(result.selected_vars);
  if (new Set(selected_vars).size !== selected_vars.length || selected_vars.some(v => !v)) fail("invalid selected variables");
  if (!Array.isArray(result.rows)) fail("invalid rows");
  const rows = result.rows.map(row => {
    const r = object(row); keys(r, selected_vars);
    return Object.fromEntries(selected_vars.map(k => [k, integer(r[k], 0xffffffff)]));
  });
  return { family: v.family, result: { selected_vars, rows, truncated: boolean(result.truncated) }, trust: validateTrust(v.trust), non_claims: strings(v.non_claims) };
}

// Byte limit applies to decoded streamed HTTP body, even without Content-Length.
// Parsing is followed by a bounded-depth iterative walk before domain validation.
export async function readBoundedJson(response: Response, maximum = api.limits.response_bytes): Promise<unknown> {
  if (!response.headers.get("content-type")?.toLowerCase().startsWith("application/json")) fail(`HTTP ${response.status}: expected JSON`);
  const length = response.headers.get("content-length");
  if (length !== null && (!/^\d+$/.test(length) || Number(length) > maximum)) fail("oversized/invalid Content-Length");
  if (!response.body) fail("missing response body");
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = []; let size = 0;
  try {
    while (true) {
      const { done, value } = await reader.read(); if (done) break;
      size += value.byteLength; if (size > maximum) fail("oversized response");
      chunks.push(value);
    }
  } catch (error) { await reader.cancel().catch(() => {}); throw error; }
  finally { reader.releaseLock(); }
  const bytes = new Uint8Array(size); let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
  let value: unknown;
  try {
    value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch (error) {
    fail(`invalid UTF-8 JSON response: ${String(error)}`);
  }
  const pending: [unknown, number][] = [[value, 0]]; let entries = 0;
  while (pending.length) {
    const next = pending.pop(); if (!next) break;
    const [v, depth] = next;
    if (++entries > 500000 || depth > 64) fail("JSON structure budget exceeded");
    if (v && typeof v === "object") for (const child of Object.values(v)) pending.push([child, depth + 1]);
  }
  if (!response.ok) { const e = object(value); keys(e, ["error"]); fail(`HTTP ${response.status}: ${text(e.error)}`); }
  return value;
}

// This editor accepts an IR JSON object, not an executable handle. Atom shapes,
// names, paths and typing are validated/elaborated ONLY by the Rust compiler.
export function serializeFiniteQuery(source: string): string {
  if (new TextEncoder().encode(source).length > api.limits.request_bytes) fail("oversized query input");
  let decoded: unknown;
  try {
    decoded = JSON.parse(source);
  } catch (error) {
    fail(`invalid query JSON: ${String(error)}`);
  }
  const query = object(decoded);
  if (query.version !== 1) fail("query requires explicit version 1");
  const body = JSON.stringify({ query });
  if (new TextEncoder().encode(body).length > api.limits.request_bytes) fail("oversized query request");
  return body;
}
export class ReadOnlyClient {
  private ready = false;
  private discoveryToken = 0;
  constructor(private readonly transport: Transport = fetch) {}
  private async request(path: "/capabilities" | "/status" | "/query", init: RequestInit = {}) {
    const response = await this.transport(path, { cache: "no-store", redirect: "error", credentials: "same-origin", signal: AbortSignal.timeout(30000), ...init });
    return readBoundedJson(response);
  }
  async discover() {
    const token = ++this.discoveryToken;
    this.ready = false;
    const capabilities = validateCapabilities(await this.request("/capabilities"));
    const status = validateStatus(await this.request("/status"));
    if (token !== this.discoveryToken) fail("stale discovery superseded");
    this.ready = true;
    return { capabilities, status };
  }
  async query(source: string) {
    if (!this.ready) fail("capabilities/status not established");
    const body = serializeFiniteQuery(source);
    return validateQueryResponse(await this.request("/query", { method: "POST", headers: { "content-type": "application/json" }, body }));
  }
}

// A late response never replaces a newer query or an editor change. No graph
// identity is bound in this API, so this boundary exposes no highlight callback.
export class LatestQuery {
  private token = 0;
  invalidate() { this.token++; }
  async run(client: ReadOnlyClient, source: string, success: (v: QueryResponse) => void, failure: (e: unknown) => void) {
    const token = ++this.token;
    try { const result = await client.query(source); if (token === this.token) success(result); }
    catch (error) { if (token === this.token) failure(error); }
  }
}
