type DraftRecord = Readonly<Record<string, unknown>>;

function isRecord(value: unknown): value is DraftRecord {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function record(value: unknown, location: string): DraftRecord {
  if (!isRecord(value)) {
    throw new Error(
      `Invalid draft ${location}: expected an object; regenerate or repair the draft.`,
    );
  }
  return value;
}

function identifier(value: unknown, location: string): string {
  if (typeof value !== "string" || !value.trim()) {
    throw new Error(
      `Invalid draft ${location}: expected a nonempty string; regenerate or repair the draft.`,
    );
  }
  return value;
}

export interface DraftSelection {
  readonly proposals_json: DraftRecord & {
    readonly proposals: readonly DraftRecord[];
  };
  readonly chunks: readonly DraftRecord[];
}

// Consumers only serialize this view. Copy the changed envelope/arrays, not the
// entire JSON tree; retained proposal/chunk records are read-only shared values.
// Null means no draft, never malformed data or an invalid successful selection.
export function selectDraft(
  overlay: unknown,
  selected: ReadonlySet<string>,
): DraftSelection | null {
  if (overlay === null || overlay === undefined) return null;
  const draft = record(overlay, "overlay");
  const file = record(draft.proposals_json, "proposals_json");
  if (!Array.isArray(file.proposals)) {
    throw new Error(
      "Invalid draft proposals: expected an array; regenerate or repair the draft.",
    );
  }
  const needed = new Set<string>();
  const seen = new Set<string>();
  const proposals: DraftRecord[] = [];
  for (const value of file.proposals) {
    const proposal = record(value, "proposal");
    const id = identifier(proposal.proposal_id, "proposal_id");
    if (seen.has(id))
      throw new Error(
        `Invalid draft: duplicate proposal_id ${id}; regenerate or repair the draft.`,
      );
    seen.add(id);
    if (!Array.isArray(proposal.evidence)) {
      throw new Error(
        "Invalid draft evidence: expected an array; regenerate or repair the draft.",
      );
    }
    for (const value of proposal.evidence) {
      const evidence = record(value, "evidence entry");
      const chunkId = identifier(evidence.chunk_id, "evidence chunk_id");
      if (selected.has(id)) needed.add(chunkId);
    }
    if (selected.has(id)) proposals.push(proposal);
  }
  for (const id of selected) {
    if (!seen.has(id))
      throw new Error(
        `Invalid draft selection: unknown proposal_id ${id}; select proposals again.`,
      );
  }
  if (draft.chunks !== undefined && !Array.isArray(draft.chunks)) {
    throw new Error(
      "Invalid draft chunks: expected an array; regenerate or repair the draft.",
    );
  }
  const chunks: DraftRecord[] = [];
  for (const value of Array.isArray(draft.chunks) ? draft.chunks : []) {
    const chunk = record(value, "chunk");
    const id = identifier(chunk.chunk_id, "chunk_id");
    if (!needed.size || needed.has(id)) chunks.push(chunk);
  }
  return { proposals_json: { ...file, proposals }, chunks };
}
