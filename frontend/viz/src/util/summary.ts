// @ts-nocheck

export function makeSummaries(ctx) {
  const {
    nodeById,
    outEdgesBySource,
    nodeShortLabel,
    parseRelationSignatureFieldOrder,
    isTupleLike,
  } = ctx;

  function firstOutTargetId(srcId, edgeLabel) {
    const edges = outEdgesBySource.get(srcId) || [];
    for (const e of edges) {
      if (!e || e.kind !== "relation") continue;
      if (String(e.label || "") !== edgeLabel) continue;
      return e.target;
    }
    return null;
  }

  function tupleFallbackSummary(n) {
    if (!n || !isTupleLike(n)) return null;

    const rel = (n.attrs && n.attrs.axi_relation) ? String(n.attrs.axi_relation) : String(n.entity_type || "Tuple");
    const order = parseRelationSignatureFieldOrder(n.attrs && n.attrs.axi_overlay_relation_signature);
    const rank = new Map();
    if (order) {
      for (let i = 0; i < order.length; i++) rank.set(order[i], i);
    }

    const edges = outEdgesBySource.get(n.id) || [];
    const fields = [];
    for (const e of edges) {
      if (!e || e.kind !== "relation") continue;
      const label = String(e.label || "");
      if (!label) continue;
      // Runtime/meta affordances: keep tuple summaries readable.
      if (label === "axi_fact_of") continue;
      if (label === "axi_fact_in_context") continue;
      if (label.startsWith("axi_")) continue;
      const t = nodeById.get(e.target);
      const value = t ? nodeShortLabel(t) : String(e.target);
      fields.push({ label, value });
    }

    if (!fields.length) return rel;

    fields.sort((a, b) => {
      const ra = rank.has(a.label) ? rank.get(a.label) : 10_000;
      const rb = rank.has(b.label) ? rank.get(b.label) : 10_000;
      if (ra !== rb) return ra - rb;
      return a.label.localeCompare(b.label);
    });

    const parts = fields.map(f => `${f.label}=${f.value}`);
    return `${rel}(${parts.join(", ")})`;
  }

  function factSummary(n) {
    if (!n || n.kind !== "fact") return null;

    const rel = (n.attrs && n.attrs.axi_relation) ? String(n.attrs.axi_relation) : String(n.entity_type || "Fact");
    const order = parseRelationSignatureFieldOrder(n.attrs && n.attrs.axi_overlay_relation_signature);
    const rank = new Map();
    if (order) {
      for (let i = 0; i < order.length; i++) rank.set(order[i], i);
    }

    const edges = outEdgesBySource.get(n.id) || [];
    const fields = [];
    for (const e of edges) {
      if (!e || e.kind !== "relation") continue;
      const label = String(e.label || "");
      if (!label) continue;
      // Runtime/meta affordances: keep the "typed record" fields readable.
      if (label === "axi_fact_of") continue;
      if (label === "axi_fact_in_context") continue;
      if (label.startsWith("axi_")) continue;
      const t = nodeById.get(e.target);
      const value = t ? nodeShortLabel(t) : String(e.target);
      fields.push({ label, value });
    }

    if (!fields.length) return rel;

    fields.sort((a, b) => {
      const ra = rank.has(a.label) ? rank.get(a.label) : 10_000;
      const rb = rank.has(b.label) ? rank.get(b.label) : 10_000;
      if (ra !== rb) return ra - rb;
      return a.label.localeCompare(b.label);
    });

    const parts = fields.map(f => `${f.label}=${f.value}`);
    return `${rel}(${parts.join(", ")})`;
  }

  function morphismSummary(n) {
    if (!n || n.kind !== "morphism") return null;
    const rel = (n.attrs && n.attrs.axi_relation) ? String(n.attrs.axi_relation) : String(n.entity_type || "Morphism");
    const fromId = firstOutTargetId(n.id, "from");
    const toId = firstOutTargetId(n.id, "to");
    const from = fromId != null ? nodeShortLabel(nodeById.get(fromId)) : "";
    const to = toId != null ? nodeShortLabel(nodeById.get(toId)) : "";
    if (from && to) return `${rel}: ${from} → ${to}`;
    return tupleFallbackSummary(n) || rel;
  }

  function homotopySummary(n) {
    if (!n || n.kind !== "homotopy") return null;
    const rel = (n.attrs && n.attrs.axi_relation) ? String(n.attrs.axi_relation) : String(n.entity_type || "Homotopy");
    const lhsId = firstOutTargetId(n.id, "lhs");
    const rhsId = firstOutTargetId(n.id, "rhs");
    const lhs = lhsId != null ? nodeShortLabel(nodeById.get(lhsId)) : "";
    const rhs = rhsId != null ? nodeShortLabel(nodeById.get(rhsId)) : "";
    if (lhs && rhs) return `${rel}: ${lhs} ≃ ${rhs}`;
    return tupleFallbackSummary(n) || rel;
  }

  function shortenHash(h) {
    const s = String(h || "").trim();
    if (!s) return "";
    if (s.length <= 10) return s;
    return s.slice(0, 10) + "…";
  }

  function shortLocator(loc) {
    const s0 = String(loc || "").trim();
    if (!s0) return "";
    const s = s0.replace(/[?#].*$/, "");
    const parts = s.split(/[\\/]/g).filter(p => p.length > 0);
    if (parts.length >= 1) return parts[parts.length - 1];
    return s0.length > 36 ? s0.slice(0, 36) + "…" : s0;
  }

  function proposalRunSummary(n) {
    if (!n || n.entity_type !== "ProposalRun") return null;
    const attrs = n.attrs || {};
    const digest = shortenHash(attrs.proposals_digest);
    const hint = String(attrs.schema_hint || "").trim();
    const stype = String(attrs.source_type || "").trim();
    const loc = shortLocator(attrs.source_locator || attrs.source || "");
    const tag = hint || stype || "run";
    if (digest && loc) return `${tag} @ ${loc} (${digest})`;
    if (digest) return `${tag} (${digest})`;
    if (loc) return `${tag} @ ${loc}`;
    return tag;
  }

  function documentSummary(n) {
    if (!n || n.entity_type !== "Document") return null;
    const attrs = n.attrs || {};
    const doc = shortLocator(attrs.document_id || n.name || "");
    if (!doc) return "Document";
    return `doc ${doc}`;
  }

  function docChunkSummary(n) {
    if (!n || n.entity_type !== "DocChunk") return null;
    const attrs = n.attrs || {};
    const chunk = shortLocator(attrs.chunk_id || n.name || "");
    const aboutId = firstOutTargetId(n.id, "doc_chunk_about");
    const about = aboutId != null ? nodeShortLabel(nodeById.get(aboutId)) : "";
    if (chunk && about) return `chunk ${chunk} (about ${about})`;
    if (chunk) return `chunk ${chunk}`;
    if (about) return `chunk (about ${about})`;
    return "chunk";
  }

  return {
    firstOutTargetId,
    factSummary,
    tupleFallbackSummary,
    morphismSummary,
    homotopySummary,
    shortenHash,
    shortLocator,
    proposalRunSummary,
    documentSummary,
    docChunkSummary,
  };
}
