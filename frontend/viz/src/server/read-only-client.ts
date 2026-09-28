import {
  parseBoundedJson,
  readBoundedJsonResponse,
  utf8Length,
  type JsonBudget,
} from "../json-boundary";
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
  if (selected_vars.length > 64 || new Set(selected_vars).size !== selected_vars.length || selected_vars.some(v => !v)) fail("invalid selected variables");
  if (!Array.isArray(result.rows) || result.rows.length > 200) fail("invalid rows");
  const rows = result.rows.map(row => {
    const r = object(row); keys(r, selected_vars);
    return Object.fromEntries(selected_vars.map(k => [k, integer(r[k], 0xffffffff)]));
  });
  return { family: v.family, result: { selected_vars, rows, truncated: boolean(result.truncated) }, trust: validateTrust(v.trust), non_claims: strings(v.non_claims) };
}

const RESPONSE_JSON_BUDGET: JsonBudget = {
  maxBytes: api.limits.response_bytes,
  maxDepth: 64,
  maxValues: 500_000,
  maxContainerEntries: 4096,
  maxStringBytes: 1024 * 1024,
  maxTotalStringBytes: api.limits.response_bytes,
};
const QUERY_JSON_BUDGET: JsonBudget = {
  maxBytes: api.limits.request_bytes,
  maxDepth: 32,
  maxValues: 8192,
  maxContainerEntries: 256,
  maxStringBytes: 64 * 1024,
  maxTotalStringBytes: api.limits.request_bytes,
};

// Byte and lexical-depth checks precede JSON.parse. Domain validation follows
// the structural budget and treats checker/materialization metadata as opaque.
export async function readBoundedJson(response: Response, maximum = api.limits.response_bytes): Promise<unknown> {
  const value = await readBoundedJsonResponse(
    response,
    { ...RESPONSE_JSON_BUDGET, maxBytes: maximum, maxTotalStringBytes: maximum },
    "Read-only API response",
  );
  if (!response.ok) { const e = object(value); keys(e, ["error"]); fail(`HTTP ${response.status}: ${text(e.error)}`); }
  return value;
}

function queryTerm(value: unknown): void {
  if (typeof value === "string") return;
  if (typeof value === "number") { integer(value, 0xffffffff); return; }
  const term = object(value);
  const kind = text(term.kind);
  if (kind === "var") { keys(term, ["kind", "name"]); text(term.name); }
  else if (kind === "name") { keys(term, ["kind", "value"]); text(term.value); }
  else if (kind === "entity") { keys(term, ["kind", "key", "value"]); text(term.key); text(term.value); }
  else if (kind === "wildcard") keys(term, ["kind"]);
  else fail("unknown query term kind");
}
function stringMap(value: unknown): void {
  const map = object(value);
  if (Object.keys(map).length > 256) fail("query map fanout exceeds 256");
  Object.values(map).forEach(text);
}
function queryTermMap(value: unknown): void {
  const map = object(value);
  if (Object.keys(map).length > 256) fail("query field fanout exceeds 256");
  Object.values(map).forEach(queryTerm);
}
function queryAtom(value: unknown): void {
  const atom = object(value);
  switch (text(atom.kind)) {
    case "type": keys(atom, ["kind", "term", "type"]); queryTerm(atom.term); text(atom.type); break;
    case "edge": keys(atom, ["kind", "left", "path", "right"]); queryTerm(atom.left); queryTerm(atom.right); if (utf8Length(text(atom.path)) > 4096) fail("query path exceeds 4096 bytes"); break;
    case "attr_eq": keys(atom, ["kind", "term", "key", "value"]); queryTerm(atom.term); text(atom.key); text(atom.value); break;
    case "attr_contains": keys(atom, ["kind", "term", "key", "needle"]); queryTerm(atom.term); text(atom.key); text(atom.needle); break;
    case "attr_fts": keys(atom, ["kind", "term", "key", "query"]); queryTerm(atom.term); text(atom.key); text(atom.query); break;
    case "attr_fuzzy": keys(atom, ["kind", "term", "key", "needle", "max_dist"]); queryTerm(atom.term); text(atom.key); text(atom.needle); integer(atom.max_dist, 16); break;
    case "fact": keys(atom, ["kind", "relation", "fields"], ["fact"]); text(atom.relation); queryTermMap(atom.fields); if (atom.fact !== undefined) queryTerm(atom.fact); break;
    case "has_out": keys(atom, ["kind", "term", "rels"]); queryTerm(atom.term); if (!Array.isArray(atom.rels) || atom.rels.length > 256) fail("invalid query rels"); atom.rels.forEach(text); break;
    case "attrs": keys(atom, ["kind", "term", "pairs"]); queryTerm(atom.term); stringMap(atom.pairs); break;
    case "shape": keys(atom, ["kind", "term"], ["type_name", "rels", "attrs"]); queryTerm(atom.term); if (atom.type_name !== undefined) text(atom.type_name); if (atom.rels !== undefined) { if (!Array.isArray(atom.rels) || atom.rels.length > 256) fail("invalid query rels"); atom.rels.forEach(text); } if (atom.attrs !== undefined) stringMap(atom.attrs); break;
    default: fail("unknown query atom kind");
  }
}
function queryContext(value: unknown): void {
  if (typeof value === "string") return;
  if (typeof value === "number") { integer(value, 0xffffffff); return; }
  const context = object(value);
  if (context.kind === "name") { keys(context, ["kind", "name"]); text(context.name); }
  else if (context.kind === "entity_id") { keys(context, ["kind", "id"]); integer(context.id, 0xffffffff); }
  else fail("unknown query context kind");
}
export function validateFiniteQuery(value: unknown): JsonObject {
  const query = object(value);
  keys(query, ["version"], ["select_vars", "where_atoms", "disjuncts", "limit", "max_hops", "min_confidence", "contexts"]);
  if (query.version !== 1) fail("query requires explicit version 1");
  const hasWhere = query.where_atoms !== undefined;
  const hasDisjuncts = query.disjuncts !== undefined;
  if (hasWhere === hasDisjuncts) fail("query requires exactly one of where_atoms or disjuncts");
  if (query.select_vars !== undefined) {
    if (!Array.isArray(query.select_vars) || query.select_vars.length > 64) fail("invalid select_vars count");
    query.select_vars.forEach(text);
  }
  let atoms = 0;
  const clause = (value: unknown): void => {
    if (!Array.isArray(value) || value.length > 256) fail("invalid query atom count");
    atoms += value.length;
    if (atoms > 256) fail("aggregate query atom count exceeds 256");
    value.forEach(queryAtom);
  };
  if (hasWhere) clause(query.where_atoms);
  else {
    if (!Array.isArray(query.disjuncts) || query.disjuncts.length > 32) fail("invalid query disjunct count");
    query.disjuncts.forEach(clause);
  }
  if (query.limit !== undefined && (integer(query.limit, 200) < 1)) fail("query limit must be positive");
  if (query.max_hops !== undefined) integer(query.max_hops, 64);
  if (query.min_confidence !== undefined && (typeof query.min_confidence !== "number" || !Number.isFinite(query.min_confidence) || query.min_confidence < 0 || query.min_confidence > 1)) fail("invalid min_confidence");
  if (query.contexts !== undefined) { if (!Array.isArray(query.contexts) || query.contexts.length > 32) fail("invalid context count"); query.contexts.forEach(queryContext); }
  return query;
}

// The browser validates the complete closed transport shape. Rust remains the
// authority for name resolution, path parsing, typing and execution semantics.
export function serializeFiniteQuery(source: string): string {
  const query = validateFiniteQuery(parseBoundedJson(source, QUERY_JSON_BUDGET, "query input"));
  const body = JSON.stringify({ query });
  if (utf8Length(body) > api.limits.request_bytes) fail("oversized query request");
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
