export type GraphAttributes = Record<string, unknown>;

export interface GraphNode {
  id: number;
  entity_type?: string;
  kind?: string;
  name?: string;
  display_name?: string;
  type_label?: string;
  plane?: string;
  attrs?: GraphAttributes;
}

export interface GraphEdge {
  source: number;
  target: number;
  label?: string;
  kind?: string;
  confidence?: number;
}

export interface GraphContext {
  id: number;
  name?: string;
}

export interface GraphSummary {
  focus_ids?: number[];
}

export interface GraphPayload {
  nodes: GraphNode[];
  edges: GraphEdge[];
  truncated?: boolean;
  summary?: GraphSummary;
  contexts?: GraphContext[];
  tuple_contexts?: Record<string, number[]>;
}

export interface DraftEvidence extends Record<string, unknown> {
  chunk_id: string;
  locator?: string;
}

export interface DraftProposal extends Record<string, unknown> {
  proposal_id: string;
  kind?: string;
  confidence?: number;
  schema_hint?: string;
  entity_type?: string;
  name?: string;
  entity_id?: string | number;
  rel_type?: string;
  relation_id?: string | number;
  source?: string;
  target?: string;
  evidence: DraftEvidence[];
}

export interface DraftChunk extends Record<string, unknown> {
  chunk_id: string;
}

export interface DraftOverlay {
  proposals_json: {
    proposals: DraftProposal[];
    [key: string]: unknown;
  };
  chunks?: DraftChunk[];
  validation?: {
    ok?: boolean;
  };
  [key: string]: unknown;
}

export interface LlmHistoryEntry {
  role: string;
  content: string;
  public_rationale: string;
  citations: string[];
  queries: string[];
  notes: string[];
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
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isNonnegativeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function isOptionalString(value: unknown): value is string | undefined {
  return value === undefined || typeof value === "string";
}

function isNonemptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function isGraphNode(value: unknown): value is GraphNode {
  if (!isRecord(value) || !isNonnegativeInteger(value.id)) return false;
  for (const field of ["entity_type", "kind", "name", "display_name", "type_label", "plane"]) {
    if (!isOptionalString(value[field])) return false;
  }
  return value.attrs === undefined || isRecord(value.attrs);
}

function isGraphEdge(value: unknown): value is GraphEdge {
  return (
    isRecord(value) &&
    isNonnegativeInteger(value.source) &&
    isNonnegativeInteger(value.target) &&
    isOptionalString(value.label) &&
    isOptionalString(value.kind) &&
    (value.confidence === undefined ||
      (typeof value.confidence === "number" && Number.isFinite(value.confidence)))
  );
}

export function isGraphPayload(value: unknown): value is GraphPayload {
  if (!isRecord(value) || !Array.isArray(value.nodes) || !Array.isArray(value.edges)) {
    return false;
  }
  if (!value.nodes.every(isGraphNode) || !value.edges.every(isGraphEdge)) return false;
  if (value.truncated !== undefined && typeof value.truncated !== "boolean") return false;
  if (value.summary !== undefined) {
    if (!isRecord(value.summary)) return false;
    if (
      value.summary.focus_ids !== undefined &&
      (!Array.isArray(value.summary.focus_ids) ||
        !value.summary.focus_ids.every(isNonnegativeInteger))
    ) return false;
  }
  if (
    value.contexts !== undefined &&
    (!Array.isArray(value.contexts) ||
      !value.contexts.every(
        (context) =>
          isRecord(context) &&
          isNonnegativeInteger(context.id) &&
          isOptionalString(context.name),
      ))
  ) return false;
  if (value.tuple_contexts !== undefined) {
    if (!isRecord(value.tuple_contexts)) return false;
    for (const contexts of Object.values(value.tuple_contexts)) {
      if (!Array.isArray(contexts) || !contexts.every(isNonnegativeInteger)) return false;
    }
  }
  return true;
}

function isDraftEvidence(value: unknown): value is DraftEvidence {
  return (
    isRecord(value) &&
    isNonemptyString(value.chunk_id) &&
    isOptionalString(value.locator)
  );
}

function isDraftProposal(value: unknown): value is DraftProposal {
  if (!isRecord(value) || !isNonemptyString(value.proposal_id)) return false;
  for (const field of [
    "kind",
    "schema_hint",
    "entity_type",
    "name",
    "rel_type",
    "source",
    "target",
  ]) {
    if (!isOptionalString(value[field])) return false;
  }
  for (const field of ["entity_id", "relation_id"]) {
    if (
      value[field] !== undefined &&
      typeof value[field] !== "string" &&
      typeof value[field] !== "number"
    ) return false;
  }
  if (
    value.confidence !== undefined &&
    (typeof value.confidence !== "number" || !Number.isFinite(value.confidence))
  ) return false;
  return Array.isArray(value.evidence) && value.evidence.every(isDraftEvidence);
}

function isDraftChunk(value: unknown): value is DraftChunk {
  return isRecord(value) && isNonemptyString(value.chunk_id);
}

export function isDraftOverlay(value: unknown): value is DraftOverlay {
  if (!isRecord(value) || !isRecord(value.proposals_json)) return false;
  const proposals = value.proposals_json.proposals;
  if (!Array.isArray(proposals) || !proposals.every(isDraftProposal)) return false;
  const proposalIds = new Set(proposals.map((proposal) => proposal.proposal_id));
  if (proposalIds.size !== proposals.length) return false;
  if (
    value.chunks !== undefined &&
    (!Array.isArray(value.chunks) || !value.chunks.every(isDraftChunk))
  ) return false;
  if (value.validation === undefined) return true;
  return (
    isRecord(value.validation) &&
    (value.validation.ok === undefined || typeof value.validation.ok === "boolean")
  );
}
