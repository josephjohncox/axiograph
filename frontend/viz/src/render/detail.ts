// @ts-nocheck

export function makeDetailRenderer(ctx) {
  const {
    graph,
    ui,
    nodeById,
    outEdgesBySource,
    inEdgesByTarget,
    isEdgeVisible,
    detailEl,
    nodeDisplayName,
    effectiveTypeLabel,
    nodeTitle,
    nodeShortLabel,
    isTupleLike,
    escapeHtml,
    parseRelationSignature,
    parseRelationSignatureFieldOrder,
    factSummary,
    morphismSummary,
    homotopySummary,
    proposalRunSummary,
    documentSummary,
    docChunkSummary,
    categorizeAttrs,
    setActiveDetailTab,
    firstOutTargetId,
    shortenHash,
    isServerMode,
    selectNode,
  } = ctx;

function renderEdgesGrouped(edges, dir) {
  // dir: "out" | "in"
  const byLabel = new Map();
  for (const e of edges) {
    const label = String(e.label || "");
    if (!byLabel.has(label)) byLabel.set(label, []);
    byLabel.get(label).push(e);
  }
  const labels = Array.from(byLabel.keys()).sort((a, b) => a.localeCompare(b));
  if (!labels.length) return `<div class="muted">(none)</div>`;

  const parts = [];
  for (const label of labels) {
    const es = byLabel.get(label) || [];
    es.sort((a, b) => {
      const ia = dir === "out" ? a.target : a.source;
      const ib = dir === "out" ? b.target : b.source;
      return ia - ib;
    });
    const open = es.length <= 5;
    const rows = es.map(e => {
      const otherId = dir === "out" ? e.target : e.source;
      const other = nodeById.get(otherId);
      const otherTitle = other ? nodeTitle(other) : String(otherId);
      const conf = e.confidence != null ? Number(e.confidence).toFixed(3) : "";
      const link = other ? `<a class="link" href="#" data-id="${otherId}">${escapeHtml(otherTitle)}</a>` : escapeHtml(otherTitle);
      const kind = escapeHtml(e.kind || "");
      return `<tr><td>${link}</td><td class="muted">${kind}</td><td class="muted">${conf}</td></tr>`;
    }).join("");
    parts.push(`
      <details ${open ? "open" : ""} style="margin-top:10px;">
        <summary><code>${escapeHtml(label)}</code><span class="muted" style="margin-left:6px;">${es.length} edge${es.length === 1 ? "" : "s"}</span></summary>
        <table style="margin-top:8px;">
          <thead><tr><th>${dir === "out" ? "to" : "from"}</th><th>kind</th><th>conf</th></tr></thead>
          <tbody>${rows || `<tr><td class="muted">(none)</td><td></td><td></td></tr>`}</tbody>
        </table>
      </details>
    `);
  }
  return parts.join("");
}

function renderTargetsGrouped(targetIds, options) {
  const limit = (options && Number(options.limit)) || 120;
  const title = options && options.title ? String(options.title) : "";
  const empty = options && options.empty ? String(options.empty) : "(none)";

  const nodes = [];
  for (const id of (targetIds || [])) {
    const n = nodeById.get(id);
    if (!n) continue;
    nodes.push(n);
  }
  if (!nodes.length) return `<div class="muted">${escapeHtml(empty)}</div>`;

  const groups = new Map(); // key -> { kind, entityType, ids: [] }
  for (const n of nodes) {
    const kind = n.kind || "entity";
    const entityType = effectiveTypeLabel(n);
    const key = `${kind}::${entityType}`;
    if (!groups.has(key)) groups.set(key, { kind, entityType, nodes: [] });
    groups.get(key).nodes.push(n);
  }

  const kindOrder = new Map([["entity", 0], ["fact", 1], ["morphism", 2], ["homotopy", 3], ["meta", 4]]);
  const sortedGroups = Array.from(groups.values()).sort((a, b) => {
    const ka = kindOrder.has(a.kind) ? kindOrder.get(a.kind) : 99;
    const kb = kindOrder.has(b.kind) ? kindOrder.get(b.kind) : 99;
    if (ka !== kb) return ka - kb;
    return a.entityType.localeCompare(b.entityType);
  });

  const parts = [];
  if (title) parts.push(`<h3>${escapeHtml(title)}</h3>`);

  for (const g of sortedGroups) {
    g.nodes.sort((a, b) => {
      const na = nodeDisplayName(a) || "";
      const nb = nodeDisplayName(b) || "";
      if (na !== nb) return na.localeCompare(nb);
      return a.id - b.id;
    });
    const shown = g.nodes.slice(0, limit);
    const rows = shown.map(n => {
      const disp = nodeDisplayName(n) || nodeTitle(n);
      return `<tr><td><a class="link" href="#" data-id="${n.id}">${escapeHtml(disp)}</a></td><td class="muted">${escapeHtml(n.kind || "entity")}</td></tr>`;
    }).join("");
    const more = g.nodes.length > limit ? `<div class="muted" style="margin-top:6px;">showing ${limit} of ${g.nodes.length} (increase viz max_nodes/hops for more)</div>` : "";
    parts.push(`
      <details open style="margin-top:10px;">
        <summary><strong>${escapeHtml(g.entityType)}</strong><span class="muted" style="margin-left:6px;">${escapeHtml(g.kind)} • ${g.nodes.length}</span></summary>
        <table style="margin-top:8px;"><thead><tr><th>node</th><th>kind</th></tr></thead><tbody>${rows}</tbody></table>
        ${more}
      </details>
    `);
  }

  return parts.join("");
}

function kvTable(pairs) {
  const rows = (pairs || []).filter(p => p && p.length >= 2).map(([k, v]) => {
    if (v === null || v === undefined) return "";
    const vs = String(v);
    if (!vs.trim()) return "";
    return `<tr><td><code>${escapeHtml(k)}</code></td><td>${escapeHtml(vs)}</td></tr>`;
  }).filter(s => s.length > 0).join("");
  if (!rows) return `<div class="muted">(no details)</div>`;
  return `<table><tbody>${rows}</tbody></table>`;
}

function renderFactFieldsTable(factNode) {
  const parsed = parseRelationSignature(factNode.attrs && factNode.attrs.axi_overlay_relation_signature);
  const order = parsed && parsed.fields ? parsed.fields : null;
  const rank = new Map();
  if (order) for (let i = 0; i < order.length; i++) rank.set(order[i].name, i);

  const edges = outEdgesBySource.get(factNode.id) || [];
  const fields = [];
  for (const e of edges) {
    if (!e || e.kind !== "relation") continue;
    const label = String(e.label || "");
    if (!label) continue;
    // Hide runtime/meta affordances from the "statement" view.
    if (label === "axi_fact_of") continue;
    if (label === "axi_fact_in_context") continue;
    if (label.startsWith("axi_")) continue;
    const t = nodeById.get(e.target);
    fields.push({ label, targetId: e.target, targetNode: t || null });
  }
  if (!fields.length) return `<div class="muted">(no fields)</div>`;

  fields.sort((a, b) => {
    const ra = rank.has(a.label) ? rank.get(a.label) : 10_000;
    const rb = rank.has(b.label) ? rank.get(b.label) : 10_000;
    if (ra !== rb) return ra - rb;
    return a.label.localeCompare(b.label);
  });

  const tyByField = new Map();
  if (order) for (const f of order) tyByField.set(f.name, f.ty || "");

  const rows = fields.map(f => {
    const ty = tyByField.get(f.label) || "";
    const valueTitle = f.targetNode ? nodeTitle(f.targetNode) : String(f.targetId);
    const link = f.targetNode
      ? `<a class="link" href="#" data-id="${f.targetId}">${escapeHtml(nodeShortLabel(f.targetNode))}</a>`
      : escapeHtml(valueTitle);
    return `<tr><td><code>${escapeHtml(f.label)}</code></td><td>${ty ? `<span class="muted">${escapeHtml(ty)}</span>` : ""}</td><td>${link}</td></tr>`;
  }).join("");

  return `
    <table>
      <thead><tr><th>field</th><th>type</th><th>value</th></tr></thead>
      <tbody>${rows}</tbody>
    </table>
  `;
}

function renderDetail(id) {
  const n = nodeById.get(id);
  if (!n) return;
  detailEl.classList.remove("muted");
  const outgoing = (outEdgesBySource.get(id) || []).filter(isEdgeVisible);
  const incoming = (inEdgesByTarget.get(id) || []).filter(isEdgeVisible);

  const tupleSummaryText = (n.kind === "fact")
    ? factSummary(n)
    : (n.kind === "morphism")
      ? morphismSummary(n)
      : (n.kind === "homotopy")
        ? homotopySummary(n)
        : null;
  const summaryText = tupleSummaryText
    || (n.entity_type === "ProposalRun" ? proposalRunSummary(n) : null)
    || (n.entity_type === "Document" ? documentSummary(n) : null)
    || (n.entity_type === "DocChunk" ? docChunkSummary(n) : null);
  const attrs = n.attrs || {};
  const cats = categorizeAttrs(attrs);

  // "Facts mentioning this entity": incoming edges from fact nodes (reified tuples).
  const mentionsByRel = new Map(); // relName -> [{ field, factId }]
  if (!isTupleLike(n)) {
    for (const e of incoming) {
      const s = nodeById.get(e.source);
      if (!s || !isTupleLike(s)) continue;
      const rel = (s.attrs && s.attrs.axi_relation) ? String(s.attrs.axi_relation) : String(s.entity_type || "Fact");
      if (!mentionsByRel.has(rel)) mentionsByRel.set(rel, []);
      mentionsByRel.get(rel).push({ field: String(e.label || ""), factId: s.id, factNode: s });
    }
  }

  const mentionsHtml = (() => {
    if (n.entity_type === "ProposalRun") {
      const ids = outgoing.filter(e => String(e.label || "") === "run_has_proposal").map(e => e.target);
      return `
        <div class="muted" style="margin-bottom:10px;">Evidence-plane proposals imported in this run.</div>
        ${renderTargetsGrouped(ids, { title: "Proposals (visible in this view)", empty: "(no proposals in this view)", limit: 200 })}
      `;
    }
    if (n.entity_type === "Document") {
      const ids = outgoing.filter(e => String(e.label || "") === "document_has_chunk").map(e => e.target);
      return `
        <div class="muted" style="margin-bottom:10px;">Document evidence chunks (extension layer).</div>
        ${renderTargetsGrouped(ids, { title: "Chunks (visible in this view)", empty: "(no chunks in this view)", limit: 80 })}
      `;
    }
    if (n.entity_type === "DocChunk") {
      const aboutId = firstOutTargetId(n.id, "doc_chunk_about");
      const inDocId = firstOutTargetId(n.id, "chunk_in_document");
      const aboutN = aboutId != null ? nodeById.get(aboutId) : null;
      const docN = inDocId != null ? nodeById.get(inDocId) : null;
      const preview = String(attrs.text || "").trim();
      const snippet = preview.length > 280 ? preview.slice(0, 280) + "…" : preview;
      return `
        ${aboutN ? `<div class="muted" style="margin-bottom:6px;">about: <a class="link" href="#" data-id="${aboutId}">${escapeHtml(nodeTitle(aboutN))}</a></div>` : ""}
        ${docN ? `<div class="muted" style="margin-bottom:10px;">document: <a class="link" href="#" data-id="${inDocId}">${escapeHtml(nodeTitle(docN))}</a></div>` : ""}
        ${snippet ? `<pre style="white-space:pre-wrap; max-height:260px; overflow:auto; margin:0;">${escapeHtml(snippet)}</pre>` : `<div class="muted">(no text)</div>`}
      `;
    }
    if (isTupleLike(n)) {
      const sig = attrs.axi_overlay_relation_signature ? String(attrs.axi_overlay_relation_signature) : "";
      const constraints = attrs.axi_overlay_constraints ? String(attrs.axi_overlay_constraints) : "";
      return `
        <div style="margin-bottom:10px;">
          ${tupleSummaryText ? `<div><code>${escapeHtml(tupleSummaryText)}</code></div>` : ""}
          <div class="muted" style="margin-top:6px;">Tuple nodes are reified n-ary facts. Some are additionally tagged as <code>Morphism</code> or <code>Homotopy</code> (so arrows/equivalences are first-class objects). This is how we attach context/time/provenance/constraints and later certificates to the *assertion itself*.</div>
        </div>
        ${sig ? `<div class="muted" style="margin-bottom:6px;">signature: <code>${escapeHtml(sig)}</code></div>` : ""}
        ${constraints ? `<div class="muted" style="margin-bottom:10px;">constraints: <code>${escapeHtml(constraints)}</code></div>` : ""}
        ${renderFactFieldsTable(n)}
      `;
    }

    const rels = Array.from(mentionsByRel.keys()).sort((a, b) => a.localeCompare(b));
    if (!rels.length) return `<div class="muted">(no fact nodes mention this)</div>`;
    const parts = [];
    for (const rel of rels) {
      const items = mentionsByRel.get(rel) || [];
      items.sort((a, b) => a.factId - b.factId);
      const rows = items.map(it => {
        const f = it.factNode;
        const title = f ? nodeTitle(f) : `Fact#${it.factId}`;
        const link = f ? `<a class="link" href="#" data-id="${it.factId}">${escapeHtml(factSummary(f) || title)}</a>` : escapeHtml(title);
        return `<tr><td><code>${escapeHtml(it.field)}</code></td><td>${link}</td></tr>`;
      }).join("");
      parts.push(`
        <details open style="margin-top:10px;">
          <summary><strong>${escapeHtml(rel)}</strong> <span class="muted">(${items.length})</span></summary>
          <table style="margin-top:8px;">
            <thead><tr><th>as field</th><th>fact</th></tr></thead>
            <tbody>${rows}</tbody>
          </table>
        </details>
      `);
    }
    return parts.join("");
  })();

  const dbDescribeHtml = (() => {
    if (!isServerMode()) {
      return `<div class="muted">DB describe requires server mode (<code>axiograph db serve</code>).</div>`;
    }

    const entry = ui.describeCache ? ui.describeCache.get(id) : null;
    if (!entry || entry.status === "loading") return `<div class="muted">Loading…</div>`;
    if (entry.status === "error") {
      return `<pre style="white-space:pre-wrap; max-height:340px; overflow:auto; margin:0;">${escapeHtml(JSON.stringify(entry.data || { error: "unknown" }, null, 2))}</pre>`;
    }
    const payload = entry.data && entry.data.result ? entry.data.result : entry.data;
    if (!payload) return `<div class="muted">(no data)</div>`;

    const contexts = Array.isArray(payload.contexts) ? payload.contexts : [];
    const equivs = Array.isArray(payload.equivalences) ? payload.equivalences : [];
    const outGroups = Array.isArray(payload.outgoing) ? payload.outgoing : [];
    const inGroups = Array.isArray(payload.incoming) ? payload.incoming : [];

    function linkForEntityView(ev) {
      if (!ev || ev.id == null) return "";
      const eid = Number(ev.id);
      const name = String(ev.name || ev.id);
      const inView = nodeById.has(eid);
      if (inView) {
        return `<a class="link" href="#" data-id="${eid}">${escapeHtml(name)}</a>`;
      }
      return `<a class="link" href="#" data-focus-id="${eid}">${escapeHtml(name)}</a>`;
    }

    function renderGroups(groups) {
      if (!groups.length) return `<div class="muted">(none)</div>`;
      const parts = [];
      for (const g of groups) {
        if (!g) continue;
        const rel = String(g.rel || "");
        const count = Number(g.count || 0);
        const edges = Array.isArray(g.edges) ? g.edges : [];
        const rows = edges.map(e => {
          const conf = (e && e.confidence != null) ? Number(e.confidence).toFixed(3) : "1.000";
          const ev = e && e.entity ? e.entity : null;
          const link = linkForEntityView(ev);
          return `<li><code>${escapeHtml(conf)}</code> ${link || escapeHtml(String(ev && ev.id || ""))}</li>`;
        }).join("");
        parts.push(`
          <details style="margin-top:10px;">
            <summary><code>${escapeHtml(rel)}</code> <span class="muted">(${count})</span></summary>
            <ul style="margin-top:8px; padding-left:18px;">${rows || `<li class="muted">(no samples)</li>`}</ul>
          </details>
        `);
      }
      return parts.join("");
    }

    const ctxHtml = contexts.length
      ? `<ul style="margin:0; padding-left:18px;">${contexts.map(c => `<li>${linkForEntityView(c)}</li>`).join("")}</ul>`
      : `<div class="muted">(none)</div>`;

    const equivHtml = equivs.length
      ? `<ul style="margin:0; padding-left:18px;">${equivs.map(e => {
          const other = e && e.other ? e.other : null;
          const kind = e && e.kind ? String(e.kind) : "";
          return `<li>${linkForEntityView(other)} ${kind ? `<span class="muted">(${escapeHtml(kind)})</span>` : ""}</li>`;
        }).join("")}</ul>`
      : `<div class="muted">(none)</div>`;

    return `
      <div class="muted" style="margin-bottom:10px;">Full-snapshot details (on-demand; not limited to this neighborhood view).</div>
      <h3>Contexts</h3>
      ${ctxHtml}
      <h3 style="margin-top:14px;">Equivalences</h3>
      ${equivHtml}
      <h3 style="margin-top:14px;">Outgoing</h3>
      ${renderGroups(outGroups)}
      <h3 style="margin-top:14px;">Incoming</h3>
      ${renderGroups(inGroups)}
    `;
  })();

  const attrsPanelHtml = (() => {
    function rowsForPairs(pairs) {
      return pairs.map(([k, v]) => {
        const vs = String(v);
        const isText = (k === "text" || k === "search_text" || k === "markdown");
        const cell = isText
          ? `<pre style="white-space:pre-wrap; max-height:240px; overflow:auto; margin:0;">${escapeHtml(vs)}</pre>`
          : escapeHtml(vs);
        return `<tr><td><code>${escapeHtml(k)}</code></td><td>${cell}</td></tr>`;
      }).join("");
    }

    const other = rowsForPairs(cats.other);
    const content = rowsForPairs(cats.content);
    const overlay = rowsForPairs(cats.overlay);
    const axi = rowsForPairs(cats.axi);

    return `
      ${cats.content.length ? `<h3>Content</h3><table><tbody>${content}</tbody></table>` : `<div class="muted">(no content fields)</div>`}
      <details style="margin-top:12px;" ${cats.other.length ? "open" : ""}>
        <summary>Other attributes <span class="muted">(${cats.other.length})</span></summary>
        <table style="margin-top:8px;"><tbody>${other || `<tr><td class="muted">(none)</td><td></td></tr>`}</tbody></table>
      </details>
      <details style="margin-top:12px;">
        <summary>Axi metadata <span class="muted">(${cats.axi.length})</span></summary>
        <table style="margin-top:8px;"><tbody>${axi || `<tr><td class="muted">(none)</td><td></td></tr>`}</tbody></table>
      </details>
      <details style="margin-top:12px;">
        <summary>Overlay attributes <span class="muted">(${cats.overlay.length})</span></summary>
        <table style="margin-top:8px;"><tbody>${overlay || `<tr><td class="muted">(none)</td><td></td></tr>`}</tbody></table>
      </details>
    `;
  })();

  const overviewHtml = (() => {
    const plane = n.plane ? String(n.plane) : "";
    const kind = n.kind ? String(n.kind) : "entity";
    const overviewExtra = (() => {
      if (n.entity_type === "ProposalRun") {
        const visible = outgoing.filter(e => String(e.label || "") === "run_has_proposal").length;
        return kvTable([
          ["schema_hint", attrs.schema_hint],
          ["source_type", attrs.source_type],
          ["source_locator", attrs.source_locator],
          ["generated_at", attrs.generated_at],
          ["proposals_digest", attrs.proposals_digest ? shortenHash(attrs.proposals_digest) : ""],
          ["visible_proposals", String(visible)],
        ]);
      }
      if (n.entity_type === "Document") {
        const visible = outgoing.filter(e => String(e.label || "") === "document_has_chunk").length;
        return kvTable([
          ["document_id", attrs.document_id],
          ["visible_chunks", String(visible)],
        ]);
      }
      if (n.entity_type === "DocChunk") {
        return kvTable([
          ["chunk_id", attrs.chunk_id],
          ["document_id", attrs.document_id],
          ["span_id", attrs.span_id],
          ["page", attrs.page],
        ]);
      }
      if (n.entity_type === "AxiMetaTheory") {
        const cids = outgoing
          .filter(e => String(e.label || "") === "axi_theory_has_constraint")
          .map(e => e.target);
        const blocks = [];
        for (const cid of cids) {
          const cn = nodeById.get(cid);
          if (!cn) continue;
          const kind = cn.attrs && cn.attrs.axi_constraint_kind ? String(cn.attrs.axi_constraint_kind) : "";
          if (kind !== "named_block") continue;
          blocks.push(cn);
        }
        blocks.sort((a, b) => String((a.attrs && a.attrs.axi_constraint_name) || a.display_name || "").localeCompare(
          String((b.attrs && b.attrs.axi_constraint_name) || b.display_name || "")
        ));
        if (!blocks.length) return `<div class="muted">(no named-block constraints in this view)</div>`;
        const rows = blocks.map(b => {
          const nm = (b.attrs && b.attrs.axi_constraint_name) ? String(b.attrs.axi_constraint_name) : (b.display_name || "");
          return `<tr><td><a class="link" href="#" data-id="${b.id}">${escapeHtml(nm)}</a></td></tr>`;
        }).join("");
        return `
          <div class="muted" style="margin-bottom:8px;">Named-block constraints are preserved as structured (but opaque) theory content.</div>
          <table><thead><tr><th>named blocks</th></tr></thead><tbody>${rows}</tbody></table>
        `;
      }
      return "";
    })();
    return `
      <div style="margin-bottom:10px;">
        <div class="muted">type: <code>${escapeHtml(n.entity_type)}</code> • id: <code>${n.id}</code> • kind: <code>${escapeHtml(kind)}</code>${plane ? ` • plane: <code>${escapeHtml(plane)}</code>` : ""}</div>
        ${summaryText ? `<div class="muted" style="margin-top:6px;">summary: <code>${escapeHtml(summaryText)}</code></div>` : ""}
      </div>
      ${overviewExtra || ""}
      ${(ui.pathStart != null && ui.pathEnd != null)
        ? `<h3>Selected path</h3>
           <div class="muted">Shift-click two nodes (in list or graph) to highlight a shortest path (within the current filtered subgraph).</div>
           <table style="margin-top:8px;"><thead><tr><th>step</th><th>edge</th><th>between</th></tr></thead><tbody>${
              (ui.pathEdgeIdxs.length
                ? ui.pathEdgeIdxs.map((edgeIdx, i) => {
                    const e = graph.edges[edgeIdx];
                    const s = nodeById.get(e.source);
                    const t = nodeById.get(e.target);
                    const src = s ? nodeTitle(s) : String(e.source);
                    const dst = t ? nodeTitle(t) : String(e.target);
                    return `<tr><td>${i}</td><td><code>${escapeHtml(e.label)}</code></td><td>${escapeHtml(src)} → ${escapeHtml(dst)}</td></tr>`;
                  }).join("")
                : `<tr><td class="muted">(no path found)</td><td></td><td></td></tr>`
              )
           }</tbody></table>`
        : `<div class="muted">(tip: shift-click 2 nodes to highlight a path)</div>`
      }
    `;
  })();

  detailEl.innerHTML = `
    <h2 style="margin:0 0 6px 0;">${escapeHtml(nodeTitle(n))}</h2>

    <div class="detailtabs">
      <button class="detailtabbtn" data-dtab="overview" type="button">overview</button>
      <button class="detailtabbtn" data-dtab="facts" type="button">${isTupleLike(n) ? "tuple" : "facts"}</button>
      <button class="detailtabbtn" data-dtab="edges" type="button">edges</button>
      <button class="detailtabbtn" data-dtab="attrs" type="button">attrs</button>
      <button class="detailtabbtn" data-dtab="db" type="button">db</button>
    </div>

    <div class="detailtabpanel" data-dtab="overview">${overviewHtml}</div>
    <div class="detailtabpanel" data-dtab="facts">${mentionsHtml}</div>
    <div class="detailtabpanel" data-dtab="edges">
      <h3>Outgoing</h3>
      ${renderEdgesGrouped(outgoing, "out")}
      <h3 style="margin-top:16px;">Incoming</h3>
      ${renderEdgesGrouped(incoming, "in")}
    </div>
    <div class="detailtabpanel" data-dtab="attrs">${attrsPanelHtml}</div>
    <div class="detailtabpanel" data-dtab="db">${dbDescribeHtml}</div>
  `;

  const defaultTab = isTupleLike(n) ? "facts" : "overview";
  const initialTab = ui.detailTab || defaultTab;
  setActiveDetailTab(initialTab);

  for (const b of detailEl.querySelectorAll(".detailtabbtn")) {
    b.addEventListener("click", () => setActiveDetailTab((b.dataset && b.dataset.dtab) || "overview"));
  }

  for (const a of detailEl.querySelectorAll("a.link")) {
    a.addEventListener("click", (ev) => {
      ev.preventDefault();
      const tid = Number(a.dataset.id || "0");
      const focusId = Number(a.dataset.focusId || "0");
      if (tid) {
        selectNode(tid, false);
        return;
      }
      if (focusId && isServerMode()) {
        const p = new URLSearchParams(window.location.search || "");
        p.set("focus_id", String(focusId));
        p.delete("focus_name");
        window.location.search = p.toString();
      }
    });
  }
}

  return { renderDetail };
}
