import {
  assertSerializedJsonBytes,
  isJsonRecord,
  parseBoundedJson,
  requireExactKeys,
  type JsonBudget,
} from "./json-boundary";

export const GRAPH_FORMAT = "axiograph_viz_graph_v1";
export const GRAPH_JSON_BUDGET: JsonBudget = {
  maxBytes: 4 * 1024 * 1024,
  maxDepth: 16,
  maxValues: 100_000,
  maxContainerEntries: 16_384,
  maxStringBytes: 8 * 1024,
  maxTotalStringBytes: 1024 * 1024,
};
export const DRAFT_JSON_BUDGET: JsonBudget = {
  maxBytes: 1024 * 1024,
  maxDepth: 32,
  maxValues: 100_000,
  maxContainerEntries: 4096,
  maxStringBytes: 256 * 1024,
  maxTotalStringBytes: 1024 * 1024,
};
export const LLM_HISTORY_JSON_BUDGET: JsonBudget = {
  maxBytes: 1024 * 1024,
  maxDepth: 8,
  maxValues: 32_768,
  maxContainerEntries: 1024,
  maxStringBytes: 64 * 1024,
  maxTotalStringBytes: 1024 * 1024,
};

export type GraphAttributes = Record<string, string>;

export interface GraphNode {
  id: number;
  entity_type: string;
  kind: string;
  name?: string;
  display_name?: string;
  type_label?: string;
  plane: string;
  attrs: GraphAttributes;
}

export interface GraphEdge {
  source: number;
  target: number;
  label: string;
  kind: string;
  confidence?: number;
  relation_id?: number;
}

export interface GraphContext {
  id: number;
  name: string;
}

export interface GraphSummary {
  focus_ids: number[];
  all_nodes: boolean;
  hops: number;
  max_nodes: number;
  max_edges: number;
  include_meta_plane: boolean;
  include_data_plane: boolean;
  include_equivalences: boolean;
  typed_overlay: boolean;
}

export interface GraphPayload {
  format: typeof GRAPH_FORMAT;
  nodes: GraphNode[];
  edges: GraphEdge[];
  truncated: boolean;
  summary: GraphSummary;
  contexts: GraphContext[];
  tuple_contexts: Record<string, number[]>;
}

export interface DraftEvidence {
  chunk_id: string;
  locator?: string;
  span_id?: string;
}

interface DraftProposalBase {
  proposal_id: string;
  confidence: number;
  evidence: DraftEvidence[];
  public_rationale: string;
  metadata: Record<string, string>;
  schema_hint?: string;
  attributes: Record<string, string>;
}

export interface DraftEntityProposal extends DraftProposalBase {
  kind: "Entity";
  entity_id: string;
  entity_type: string;
  name: string;
  description?: string | null;
}

export interface DraftRelationProposal extends DraftProposalBase {
  kind: "Relation";
  relation_id: string;
  rel_type: string;
  source: string;
  target: string;
}

export type DraftProposal = DraftEntityProposal | DraftRelationProposal;

export interface DraftChunk {
  chunk_id: string;
  document_id: string;
  page: number | null;
  span_id: string;
  text: string;
  bbox: [number, number, number, number] | null;
  metadata: Record<string, string>;
}

export interface DraftOverlay {
  proposals_json: {
    version: 1;
    generated_at: string;
    source: { source_type: string; locator: string };
    schema_hint?: string;
    proposals: DraftProposal[];
  };
  chunks?: DraftChunk[];
  summary?: Record<string, unknown>;
  /** Opaque runtime report. Only `ok` is used as an evidence-plane display hint. */
  validation?: Record<string, unknown> & { ok?: boolean };
}

export interface LlmHistoryEntry {
  role: "user" | "assistant";
  content: string;
  public_rationale: string;
  citations: string[];
  queries: string[];
  notes: string[];
}

export interface LlmHistoryEnvelope {
  format: "axiograph_llm_history_v2";
  entries: LlmHistoryEntry[];
}

export interface DescribeEntry {
  status: "loading" | "error" | "ready";
  data?: Record<string, unknown>;
}

export interface LayoutBounds {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
  W: number;
  H: number;
}

export interface NodePosition {
  x: number;
  y: number;
  d: number;
}

export type DraftReviewState =
  | { kind: "empty"; reviewActionStatus: string }
  | {
      kind: "loaded";
      overlay: DraftOverlay;
      selected: Set<string>;
      reviewActionStatus: string;
    };

export interface VizUiState {
  pathStart: number | null;
  pathEnd: number | null;
  pathEdgeIdxs: number[];
  pathMessage: string;
  highlightIds: Set<number>;
  activeRunId: string | null;
  runMap: Map<string, number[]>;
  draft: DraftReviewState;
  layoutAlgo: string;
  layoutCenter: string;
  layoutSeed: number;
  layoutBounds: LayoutBounds | null;
  nodePos?: Map<number, NodePosition>;
  components: number[][] | null;
  componentByNode: Map<number, number> | null;
  detailTab?: string;
  describeCache?: Map<number, DescribeEntry>;
}

export type NodeMap = Map<number, GraphNode>;
export type EdgeMap = Map<number, GraphEdge[]>;

export function isRecord(value: unknown): value is Record<string, unknown> {
  return isJsonRecord(value);
}

function record(value: unknown, label: string): Record<string, unknown> {
  if (!isRecord(value)) throw new Error(`${label}: expected object`);
  return value;
}

function uint32(value: unknown, label: string): number {
  if (typeof value !== "number" || !Number.isInteger(value) || value < 0 || value > 0xffffffff) {
    throw new Error(`${label}: expected u32`);
  }
  return value;
}

function nonnegativeInteger(value: unknown, label: string): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) {
    throw new Error(`${label}: expected nonnegative integer`);
  }
  return value;
}

function text(value: unknown, label: string, nonempty = false): string {
  if (typeof value !== "string" || (nonempty && value.trim().length === 0)) {
    throw new Error(`${label}: expected ${nonempty ? "nonempty " : ""}string`);
  }
  return value;
}

function optionalText(value: unknown, label: string): string | undefined {
  return value === undefined ? undefined : text(value, label);
}

function setOwnDataProperty<T>(target: Record<string, T>, key: string, value: T): void {
  Object.defineProperty(target, key, {
    value,
    enumerable: true,
    configurable: true,
    writable: true,
  });
}

function stringMap(value: unknown, label: string): Record<string, string> {
  const map = record(value, label);
  if (Object.keys(map).length > 4096) throw new Error(`${label}: too many entries`);
  const validated: Record<string, string> = {};
  for (const [key, item] of Object.entries(map)) {
    setOwnDataProperty(validated, key, text(item, `${label}.${key}`));
  }
  return validated;
}

type DetachedJsonValue = string | number | boolean | null | unknown[] | Record<string, unknown>;

function detachJsonRecord(value: Record<string, unknown>): Record<string, DetachedJsonValue> {
  const detached: Record<string, DetachedJsonValue> = {};
  for (const [key, item] of Object.entries(value)) {
    setOwnDataProperty(detached, key, detachJsonValue(item));
  }
  return detached;
}

function detachJsonValue(value: unknown): DetachedJsonValue {
  if (Array.isArray(value)) return value.map(detachJsonValue);
  if (isRecord(value)) return detachJsonRecord(value);
  if (value === null || typeof value === "string" || typeof value === "number" || typeof value === "boolean") return value;
  throw new Error("draft opaque value: expected JSON");
}

function booleanValue(value: unknown, label: string): boolean {
  if (typeof value !== "boolean") throw new Error(`${label}: expected boolean`);
  return value;
}

export function validateGraphPayload(value: unknown): GraphPayload {
  assertSerializedJsonBytes(value, GRAPH_JSON_BUDGET, "graph boundary");
  const graph = record(value, "graph");
  requireExactKeys(graph, ["format", "nodes", "edges", "truncated", "summary", "contexts", "tuple_contexts"], [], "graph");
  if (graph.format !== GRAPH_FORMAT) throw new Error("graph: unsupported format");
  if (!Array.isArray(graph.nodes) || graph.nodes.length > 1000) throw new Error("graph.nodes: count exceeds 1000 or is not an array");
  if (!Array.isArray(graph.edges) || graph.edges.length > 4000) throw new Error("graph.edges: count exceeds 4000 or is not an array");
  const nodeIds = new Set<number>();
  const nodes = graph.nodes.map((item, index): GraphNode => {
    const node = record(item, `graph.nodes[${index}]`);
    requireExactKeys(node, ["id", "entity_type", "kind", "plane", "attrs"], ["name", "display_name", "type_label"], `graph.nodes[${index}]`);
    const id = uint32(node.id, `graph.nodes[${index}].id`);
    if (nodeIds.has(id)) throw new Error(`graph.nodes[${index}]: duplicate id`);
    nodeIds.add(id);
    return {
      id,
      entity_type: text(node.entity_type, `graph.nodes[${index}].entity_type`),
      kind: text(node.kind, `graph.nodes[${index}].kind`),
      plane: text(node.plane, `graph.nodes[${index}].plane`),
      attrs: stringMap(node.attrs, `graph.nodes[${index}].attrs`),
      ...(node.name === undefined ? {} : { name: optionalText(node.name, `graph.nodes[${index}].name`) }),
      ...(node.display_name === undefined ? {} : { display_name: optionalText(node.display_name, `graph.nodes[${index}].display_name`) }),
      ...(node.type_label === undefined ? {} : { type_label: optionalText(node.type_label, `graph.nodes[${index}].type_label`) }),
    };
  });
  let attributeEntries = 0;
  for (const node of nodes) attributeEntries += Object.keys(node.attrs).length;
  if (attributeEntries > 16_384) throw new Error("graph: attribute entry count exceeds 16384");
  const edges = graph.edges.map((item, index): GraphEdge => {
    const edge = record(item, `graph.edges[${index}]`);
    requireExactKeys(edge, ["source", "target", "label", "kind"], ["confidence", "relation_id"], `graph.edges[${index}]`);
    const source = uint32(edge.source, `graph.edges[${index}].source`);
    const target = uint32(edge.target, `graph.edges[${index}].target`);
    if (!nodeIds.has(source) || !nodeIds.has(target)) throw new Error(`graph.edges[${index}]: endpoint is outside graph image`);
    const confidence = edge.confidence;
    if (confidence !== undefined && (typeof confidence !== "number" || !Number.isFinite(confidence) || confidence < 0 || confidence > 1)) {
      throw new Error(`graph.edges[${index}].confidence: expected finite value in [0,1]`);
    }
    return {
      source,
      target,
      label: text(edge.label, `graph.edges[${index}].label`),
      kind: text(edge.kind, `graph.edges[${index}].kind`),
      ...(confidence === undefined ? {} : { confidence }),
      ...(edge.relation_id === undefined ? {} : { relation_id: uint32(edge.relation_id, `graph.edges[${index}].relation_id`) }),
    };
  });
  const summaryValue = record(graph.summary, "graph.summary");
  requireExactKeys(summaryValue, ["focus_ids", "all_nodes", "hops", "max_nodes", "max_edges", "include_meta_plane", "include_data_plane", "include_equivalences", "typed_overlay"], [], "graph.summary");
  if (!Array.isArray(summaryValue.focus_ids) || summaryValue.focus_ids.length > 1000) throw new Error("graph.summary.focus_ids: invalid count");
  const focusIds = summaryValue.focus_ids.map((id, index) => uint32(id, `graph.summary.focus_ids[${index}]`));
  if (new Set(focusIds).size !== focusIds.length || focusIds.some((id) => !nodeIds.has(id))) {
    throw new Error("graph.summary.focus_ids: duplicate or outside graph image");
  }
  const summary: GraphSummary = {
    focus_ids: focusIds,
    all_nodes: booleanValue(summaryValue.all_nodes, "graph.summary.all_nodes"),
    hops: nonnegativeInteger(summaryValue.hops, "graph.summary.hops"),
    max_nodes: nonnegativeInteger(summaryValue.max_nodes, "graph.summary.max_nodes"),
    max_edges: nonnegativeInteger(summaryValue.max_edges, "graph.summary.max_edges"),
    include_meta_plane: booleanValue(summaryValue.include_meta_plane, "graph.summary.include_meta_plane"),
    include_data_plane: booleanValue(summaryValue.include_data_plane, "graph.summary.include_data_plane"),
    include_equivalences: booleanValue(summaryValue.include_equivalences, "graph.summary.include_equivalences"),
    typed_overlay: booleanValue(summaryValue.typed_overlay, "graph.summary.typed_overlay"),
  };
  if (!Array.isArray(graph.contexts) || graph.contexts.length > 1000) throw new Error("graph.contexts: invalid count");
  const contextIds = new Set<number>();
  const contexts = graph.contexts.map((item, index): GraphContext => {
    const context = record(item, `graph.contexts[${index}]`);
    requireExactKeys(context, ["id", "name"], [], `graph.contexts[${index}]`);
    const id = uint32(context.id, `graph.contexts[${index}].id`);
    if (contextIds.has(id)) throw new Error(`graph.contexts[${index}]: duplicate id`);
    contextIds.add(id);
    return { id, name: text(context.name, `graph.contexts[${index}].name`) };
  });
  const tupleContextsValue = record(graph.tuple_contexts, "graph.tuple_contexts");
  const tupleContexts: Record<string, number[]> = {};
  for (const [key, item] of Object.entries(tupleContextsValue)) {
    if (!/^(0|[1-9]\d*)$/.test(key)) throw new Error(`graph.tuple_contexts.${key}: invalid u32 key`);
    const tupleId = uint32(Number(key), `graph.tuple_contexts.${key}`);
    if (!nodeIds.has(tupleId)) throw new Error(`graph.tuple_contexts.${key}: tuple is outside graph image`);
    if (!Array.isArray(item) || item.length > 1000) throw new Error(`graph.tuple_contexts.${key}: invalid context array`);
    const ids = item.map((id, index) => uint32(id, `graph.tuple_contexts.${key}[${index}]`));
    if (new Set(ids).size !== ids.length || ids.some((id) => !contextIds.has(id))) {
      throw new Error(`graph.tuple_contexts.${key}: duplicate or unknown context id`);
    }
    tupleContexts[key] = ids;
  }
  return { format: GRAPH_FORMAT, nodes, edges, truncated: booleanValue(graph.truncated, "graph.truncated"), summary, contexts, tuple_contexts: tupleContexts };
}

export function parseGraphPayloadJson(source: string): GraphPayload {
  return validateGraphPayload(parseBoundedJson(source, GRAPH_JSON_BUDGET, "graph boundary"));
}

function draftEvidence(value: unknown, label: string): DraftEvidence {
  const evidence = record(value, label);
  requireExactKeys(evidence, ["chunk_id"], ["locator", "span_id"], label);
  return {
    chunk_id: text(evidence.chunk_id, `${label}.chunk_id`, true),
    ...(evidence.locator === undefined ? {} : { locator: text(evidence.locator, `${label}.locator`) }),
    ...(evidence.span_id === undefined ? {} : { span_id: text(evidence.span_id, `${label}.span_id`) }),
  };
}

function draftProposal(value: unknown, label: string): DraftProposal {
  const proposal = record(value, label);
  const commonRequired = ["kind", "proposal_id", "confidence", "evidence", "public_rationale", "metadata", "attributes"];
  const commonOptional = ["schema_hint"];
  const kind = text(proposal.kind, `${label}.kind`);
  if (kind === "Entity") requireExactKeys(proposal, [...commonRequired, "entity_id", "entity_type", "name"], [...commonOptional, "description"], label);
  else if (kind === "Relation") requireExactKeys(proposal, [...commonRequired, "relation_id", "rel_type", "source", "target"], commonOptional, label);
  else throw new Error(`${label}.kind: unsupported proposal kind`);
  if (typeof proposal.confidence !== "number" || !Number.isFinite(proposal.confidence) || proposal.confidence < 0 || proposal.confidence > 1) {
    throw new Error(`${label}.confidence: expected finite value in [0,1]`);
  }
  if (!Array.isArray(proposal.evidence) || proposal.evidence.length > 1024) throw new Error(`${label}.evidence: invalid count`);
  const common = {
    proposal_id: text(proposal.proposal_id, `${label}.proposal_id`, true),
    confidence: proposal.confidence,
    evidence: proposal.evidence.map((item, index) => draftEvidence(item, `${label}.evidence[${index}]`)),
    public_rationale: text(proposal.public_rationale, `${label}.public_rationale`),
    metadata: stringMap(proposal.metadata, `${label}.metadata`),
    attributes: stringMap(proposal.attributes, `${label}.attributes`),
    ...(proposal.schema_hint === undefined ? {} : { schema_hint: text(proposal.schema_hint, `${label}.schema_hint`) }),
  };
  if (kind === "Entity") {
    const description = proposal.description;
    const validatedDescription = description === undefined || description === null
      ? description
      : text(description, `${label}.description`);
    return { kind, ...common, entity_id: text(proposal.entity_id, `${label}.entity_id`, true), entity_type: text(proposal.entity_type, `${label}.entity_type`, true), name: text(proposal.name, `${label}.name`, true), ...(validatedDescription === undefined ? {} : { description: validatedDescription }) };
  }
  return { kind, ...common, relation_id: text(proposal.relation_id, `${label}.relation_id`, true), rel_type: text(proposal.rel_type, `${label}.rel_type`, true), source: text(proposal.source, `${label}.source`, true), target: text(proposal.target, `${label}.target`, true) };
}

function draftChunk(value: unknown, label: string): DraftChunk {
  const chunk = record(value, label);
  requireExactKeys(chunk, ["chunk_id", "document_id", "page", "span_id", "text", "bbox", "metadata"], [], label);
  const page = chunk.page === null ? null : nonnegativeInteger(chunk.page, `${label}.page`);
  let bbox: DraftChunk["bbox"] = null;
  if (chunk.bbox !== null) {
    if (!Array.isArray(chunk.bbox) || chunk.bbox.length !== 4 || chunk.bbox.some((item) => typeof item !== "number" || !Number.isFinite(item))) {
      throw new Error(`${label}.bbox: expected four finite numbers or null`);
    }
    bbox = [chunk.bbox[0], chunk.bbox[1], chunk.bbox[2], chunk.bbox[3]];
  }
  return { chunk_id: text(chunk.chunk_id, `${label}.chunk_id`, true), document_id: text(chunk.document_id, `${label}.document_id`, true), page, span_id: text(chunk.span_id, `${label}.span_id`), text: text(chunk.text, `${label}.text`), bbox, metadata: stringMap(chunk.metadata, `${label}.metadata`) };
}

export function validateDraftOverlay(value: unknown): DraftOverlay {
  assertSerializedJsonBytes(value, DRAFT_JSON_BUDGET, "draft boundary");
  const overlay = record(value, "draft overlay");
  requireExactKeys(overlay, ["proposals_json"], ["chunks", "summary", "validation"], "draft overlay");
  const file = record(overlay.proposals_json, "draft proposals_json");
  requireExactKeys(file, ["version", "generated_at", "source", "proposals"], ["schema_hint"], "draft proposals_json");
  if (file.version !== 1) throw new Error("draft proposals_json: unsupported version");
  const source = record(file.source, "draft proposals_json.source");
  requireExactKeys(source, ["source_type", "locator"], [], "draft proposals_json.source");
  if (!Array.isArray(file.proposals) || file.proposals.length > 1000) throw new Error("draft proposals_json.proposals: invalid count");
  const proposals = file.proposals.map((item, index) => draftProposal(item, `draft proposals[${index}]`));
  if (new Set(proposals.map((proposal) => proposal.proposal_id)).size !== proposals.length) throw new Error("draft proposals: duplicate proposal_id");
  const chunks = overlay.chunks;
  let validatedChunks: DraftChunk[] | undefined;
  if (chunks !== undefined) {
    if (!Array.isArray(chunks) || chunks.length > 1000) throw new Error("draft chunks: invalid count");
    validatedChunks = chunks.map((item, index) => draftChunk(item, `draft chunks[${index}]`));
    if (new Set(validatedChunks.map((chunk) => chunk.chunk_id)).size !== validatedChunks.length) throw new Error("draft chunks: duplicate chunk_id");
  }
  const summary = overlay.summary === undefined ? undefined : record(overlay.summary, "draft summary (opaque)");
  const validation = overlay.validation === undefined ? undefined : record(overlay.validation, "draft validation (opaque)");
  const validationOk = validation !== undefined && Object.prototype.hasOwnProperty.call(validation, "ok")
    ? validation.ok
    : undefined;
  if (validationOk !== undefined && typeof validationOk !== "boolean") throw new Error("draft validation.ok: expected boolean");
  return {
    proposals_json: {
      version: 1,
      generated_at: text(file.generated_at, "draft proposals_json.generated_at"),
      source: {
        source_type: text(source.source_type, "draft proposals_json.source.source_type"),
        locator: text(source.locator, "draft proposals_json.source.locator"),
      },
      proposals,
      ...(file.schema_hint === undefined ? {} : { schema_hint: text(file.schema_hint, "draft proposals_json.schema_hint") }),
    },
    ...(validatedChunks === undefined ? {} : { chunks: validatedChunks }),
    ...(summary === undefined ? {} : { summary: detachJsonRecord(summary) }),
    ...(validation === undefined ? {} : { validation: detachJsonRecord(validation) }),
  };
}

export function parseDraftOverlayJson(source: string): DraftOverlay {
  return validateDraftOverlay(parseBoundedJson(source, DRAFT_JSON_BUDGET, "draft boundary"));
}

export function isGraphPayload(value: unknown): value is GraphPayload {
  try { validateGraphPayload(value); return true; } catch { return false; }
}

export function isDraftOverlay(value: unknown): value is DraftOverlay {
  try { validateDraftOverlay(value); return true; } catch { return false; }
}

export function validateLlmHistoryEnvelope(value: unknown): LlmHistoryEnvelope {
  assertSerializedJsonBytes(value, LLM_HISTORY_JSON_BUDGET, "LLM history boundary");
  const envelope = record(value, "LLM history");
  requireExactKeys(envelope, ["format", "entries"], [], "LLM history");
  if (envelope.format !== "axiograph_llm_history_v2") throw new Error("LLM history: unsupported format");
  if (!Array.isArray(envelope.entries) || envelope.entries.length > 1024) throw new Error("LLM history.entries: invalid count");
  const entries = envelope.entries.map((item, index): LlmHistoryEntry => {
    const entry = record(item, `LLM history.entries[${index}]`);
    requireExactKeys(entry, ["role", "content", "public_rationale", "citations", "queries", "notes"], [], `LLM history.entries[${index}]`);
    if (entry.role !== "user" && entry.role !== "assistant") throw new Error(`LLM history.entries[${index}].role: unsupported role`);
    const stringArray = (field: "citations" | "queries" | "notes"): string[] => {
      const values = entry[field];
      if (!Array.isArray(values) || values.length > 256) throw new Error(`LLM history.entries[${index}].${field}: invalid count`);
      return values.map((value, itemIndex) => text(value, `LLM history.entries[${index}].${field}[${itemIndex}]`));
    };
    return { role: entry.role, content: text(entry.content, `LLM history.entries[${index}].content`, true), public_rationale: text(entry.public_rationale, `LLM history.entries[${index}].public_rationale`), citations: stringArray("citations"), queries: stringArray("queries"), notes: stringArray("notes") };
  });
  return { format: "axiograph_llm_history_v2", entries };
}

export function parseLlmHistoryJson(source: string): LlmHistoryEnvelope {
  return validateLlmHistoryEnvelope(parseBoundedJson(source, LLM_HISTORY_JSON_BUDGET, "LLM history boundary"));
}
