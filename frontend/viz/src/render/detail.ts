// @ts-nocheck

import { entityTypeDisplayLabel, kindDisplayLabel } from "../util/labels";
import { element as el, muted } from "./dom";

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
    parseRelationSignature,
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

  function code(text) {
    return el("code", {}, String(text));
  }
  function span(text) {
    return el("span", { className: "muted" }, text);
  }
  function heading(text, marginTop = "") {
    return el("h3", { style: { marginTop } }, text);
  }
  function pre(text, height) {
    return el(
      "pre",
      {
        style: {
          whiteSpace: "pre-wrap",
          maxHeight: height,
          overflow: "auto",
          margin: "0",
        },
      },
      text,
    );
  }
  function table(headers, rows, marginTop = "") {
    return el(
      "table",
      { style: { marginTop } },
      headers.length
        ? el("thead", {}, el("tr", {}, ...headers.map((h) => el("th", {}, h))))
        : null,
      el("tbody", {}, ...rows),
    );
  }
  function row(...cells) {
    return el("tr", {}, ...cells.map((c) => el("td", {}, c)));
  }
  function group(summary, children, open = true, marginTop = "10px") {
    const details = el(
      "details",
      { style: { marginTop } },
      el("summary", {}, ...summary),
      ...children,
    );
    details.open = open;
    return details;
  }

  // These links never accept a model URL. Local selection uses the original ID;
  // server navigation accepts only numeric graph IDs and changes a query field.
  function link(id, text, focus = false) {
    const a = el(
      "a",
      {
        className: "link",
        dataset: { [focus ? "focusId" : "id"]: String(id) },
      },
      text,
    );
    a.href = "#";
    a.addEventListener("click", (ev) => {
      ev.preventDefault();
      if (!Number.isSafeInteger(id) || id < 0) return;
      if (!focus) {
        selectNode(id, false);
        return;
      }
      if (!isServerMode()) return;
      const p = new URLSearchParams(window.location.search || "");
      p.set("focus_id", String(id));
      p.delete("focus_name");
      window.location.search = p.toString();
    });
    return a;
  }

  function renderEdgesGrouped(edges, dir) {
    const byLabel = new Map();
    for (const e of edges) {
      const label = String(e.label || "");
      if (!byLabel.has(label)) byLabel.set(label, []);
      byLabel.get(label).push(e);
    }
    const labels = Array.from(byLabel.keys()).sort((a, b) =>
      a.localeCompare(b),
    );
    if (!labels.length) return [muted("(none)")];
    return labels.map((label) => {
      const edges = byLabel.get(label);
      edges.sort((a, b) =>
        dir === "out" ? a.target - b.target : a.source - b.source,
      );
      const rows = edges.map((e) => {
        const id = dir === "out" ? e.target : e.source;
        const other = nodeById.get(id);
        return row(
          other ? link(id, nodeTitle(other)) : String(id),
          span(String(e.kind || "")),
          span(e.confidence == null ? "" : Number(e.confidence).toFixed(3)),
        );
      });
      const count = span(
        `${edges.length} edge${edges.length === 1 ? "" : "s"}`,
      );
      count.style.marginLeft = "6px";
      return group(
        [code(label), count],
        [table([dir === "out" ? "to" : "from", "kind", "conf"], rows, "8px")],
        edges.length <= 5,
      );
    });
  }

  function renderTargetsGrouped(ids, { limit, title, empty }) {
    const nodes = ids.map((id) => nodeById.get(id)).filter(Boolean);
    if (!nodes.length) return [muted(empty)];
    const groups = new Map();
    for (const n of nodes) {
      const kind = n.kind || "entity";
      const entityType = effectiveTypeLabel(n);
      const key = `${kind}::${entityType}`;
      if (!groups.has(key)) groups.set(key, { kind, entityType, nodes: [] });
      groups.get(key).nodes.push(n);
    }
    const kindOrder = new Map([
      ["entity", 0],
      ["fact", 1],
      ["morphism", 2],
      ["homotopy", 3],
      ["meta", 4],
    ]);
    const sorted = Array.from(groups.values()).sort(
      (a, b) =>
        (kindOrder.get(a.kind) ?? 99) - (kindOrder.get(b.kind) ?? 99) ||
        a.entityType.localeCompare(b.entityType),
    );
    return [
      heading(title),
      ...sorted.map((g) => {
        g.nodes.sort(
          (a, b) =>
            (nodeDisplayName(a) || "").localeCompare(
              nodeDisplayName(b) || "",
            ) || a.id - b.id,
        );
        const rows = g.nodes
          .slice(0, limit)
          .map((n) =>
            row(
              link(n.id, nodeDisplayName(n) || nodeTitle(n)),
              span(kindDisplayLabel(n.kind || "entity")),
            ),
          );
        const count = span(`${kindDisplayLabel(g.kind)} • ${g.nodes.length}`);
        count.style.marginLeft = "6px";
        const children = [table(["node", "kind"], rows, "8px")];
        if (g.nodes.length > limit)
          children.push(
            el(
              "div",
              { className: "muted", style: { marginTop: "6px" } },
              `showing ${limit} of ${g.nodes.length} (increase viz max_nodes/hops for more)`,
            ),
          );
        return group([el("strong", {}, g.entityType), count], children);
      }),
    ];
  }

  function kvTable(pairs) {
    const rows = pairs
      .filter(([, v]) => v != null && String(v).trim())
      .map(([k, v]) => row(code(k), String(v)));
    return rows.length ? table([], rows) : muted("(no details)");
  }

  function renderFactFieldsTable(n) {
    const parsed = parseRelationSignature(
      n.attrs && n.attrs.axi_overlay_relation_signature,
    );
    const fields = parsed?.fields || [];
    const ranks = new Map(fields.map((f, i) => [f.name, i]));
    const types = new Map(fields.map((f) => [f.name, f.ty || ""]));
    const edges = (outEdgesBySource.get(n.id) || []).filter(
      (e) =>
        e &&
        e.kind === "relation" &&
        e.label &&
        !String(e.label).startsWith("axi_"),
    );
    if (!edges.length) return muted("(no fields)");
    edges.sort(
      (a, b) =>
        (ranks.get(a.label) ?? 10000) - (ranks.get(b.label) ?? 10000) ||
        String(a.label).localeCompare(String(b.label)),
    );
    return table(
      ["field", "type", "value"],
      edges.map((e) => {
        const target = nodeById.get(e.target);
        return row(
          code(e.label),
          types.get(e.label) ? span(types.get(e.label)) : "",
          target ? link(e.target, nodeShortLabel(target)) : String(e.target),
        );
      }),
    );
  }

  function renderMentions(n, attrs, outgoing, incoming, tupleSummary) {
    if (n.entity_type === "ProposalRun" || n.entity_type === "Document") {
      const run = n.entity_type === "ProposalRun";
      const ids = outgoing
        .filter(
          (e) =>
            String(e.label || "") ===
            (run ? "run_has_proposal" : "document_has_chunk"),
        )
        .map((e) => e.target);
      return [
        el(
          "div",
          { className: "muted", style: { marginBottom: "10px" } },
          run
            ? "Evidence-plane proposals imported in this run."
            : "Document evidence chunks (extension layer).",
        ),
        ...renderTargetsGrouped(ids, {
          title: run
            ? "Proposals (visible in this view)"
            : "Chunks (visible in this view)",
          empty: run
            ? "(no proposals in this view)"
            : "(no chunks in this view)",
          limit: run ? 200 : 80,
        }),
      ];
    }
    if (n.entity_type === "DocChunk") {
      const parts = [];
      for (const [label, relation, margin] of [
        ["about", "doc_chunk_about", "6px"],
        ["document", "chunk_in_document", "10px"],
      ]) {
        const id = firstOutTargetId(n.id, relation);
        const target = id == null ? null : nodeById.get(id);
        if (target)
          parts.push(
            el(
              "div",
              { className: "muted", style: { marginBottom: margin } },
              `${label}: `,
              link(id, nodeTitle(target)),
            ),
          );
      }
      const text = String(attrs.text || "").trim();
      parts.push(
        text
          ? pre(text.length > 280 ? text.slice(0, 280) + "…" : text, "260px")
          : muted("(no text)"),
      );
      return parts;
    }
    if (isTupleLike(n)) {
      const parts = [
        el(
          "div",
          { style: { marginBottom: "10px" } },
          tupleSummary ? el("div", {}, code(tupleSummary)) : null,
          el(
            "div",
            { className: "muted", style: { marginTop: "6px" } },
            "Tuple nodes are reified n-ary facts. Some are additionally tagged as ",
            code("Morphism"),
            " or as path-equivalence witnesses, so arrows and equivalences can stay first-class objects. This is how we attach context, time, provenance, constraints, and later certificates to the assertion itself.",
          ),
        ),
      ];
      for (const [label, key, margin] of [
        ["signature", "axi_overlay_relation_signature", "6px"],
        ["constraints", "axi_overlay_constraints", "10px"],
      ]) {
        if (attrs[key])
          parts.push(
            el(
              "div",
              { className: "muted", style: { marginBottom: margin } },
              `${label}: `,
              code(attrs[key]),
            ),
          );
      }
      return [...parts, renderFactFieldsTable(n)];
    }
    const groups = new Map();
    for (const e of incoming) {
      const fact = nodeById.get(e.source);
      if (!fact || !isTupleLike(fact)) continue;
      const rel = String(
        fact.attrs?.axi_relation || fact.entity_type || "Fact",
      );
      if (!groups.has(rel)) groups.set(rel, []);
      groups.get(rel).push({ field: String(e.label || ""), fact });
    }
    if (!groups.size) return [muted("(no fact nodes mention this)")];
    return Array.from(groups.keys())
      .sort((a, b) => a.localeCompare(b))
      .map((rel) => {
        const items = groups.get(rel).sort((a, b) => a.fact.id - b.fact.id);
        return group(
          [el("strong", {}, rel), " ", span(`(${items.length})`)],
          [
            table(
              ["as field", "fact"],
              items.map(({ field, fact }) =>
                row(
                  code(field),
                  link(fact.id, factSummary(fact) || nodeTitle(fact)),
                ),
              ),
              "8px",
            ),
          ],
        );
      });
  }

  function renderDbDescribe(id) {
    if (!isServerMode())
      return [
        el(
          "div",
          { className: "muted" },
          "DB describe unavailable through the read-only server; use the CLI (",
          code("axiograph db serve"),
          ").",
        ),
      ];
    const entry = ui.describeCache?.get(id);
    if (!entry || entry.status === "loading") return [muted("Loading…")];
    if (entry.status === "error")
      return [
        pre(
          JSON.stringify(entry.data || { error: "unknown" }, null, 2),
          "340px",
        ),
      ];
    const payload = entry.data?.result || entry.data;
    if (!payload) return [muted("(no data)")];
    function entityLink(ev) {
      if (!ev || ev.id == null) return "";
      const name = String(ev.name || ev.id);
      // Do not turn blank strings, booleans, or injected values into navigation.
      const id = ev.id;
      if (typeof id !== "number" || !Number.isSafeInteger(id) || id < 0)
        return name;
      return link(id, name, !nodeById.has(id));
    }
    function list(values, render) {
      return values.length
        ? el(
            "ul",
            { style: { margin: "0", paddingLeft: "18px" } },
            ...values.map((v) => el("li", {}, ...render(v))),
          )
        : muted("(none)");
    }
    function groups(values) {
      if (!values.length) return [muted("(none)")];
      return values.filter(Boolean).map((g) => {
        const edges = Array.isArray(g.edges) ? g.edges : [];
        const rows = edges.map((e) =>
          el(
            "li",
            {},
            code(
              e?.confidence == null ? "1.000" : Number(e.confidence).toFixed(3),
            ),
            " ",
            entityLink(e?.entity),
          ),
        );
        return group(
          [code(String(g.rel || "")), " ", span(`(${Number(g.count || 0)})`)],
          [
            el(
              "ul",
              { style: { marginTop: "8px", paddingLeft: "18px" } },
              ...(rows.length
                ? rows
                : [el("li", { className: "muted" }, "(no samples)")]),
            ),
          ],
          false,
        );
      });
    }
    return [
      el(
        "div",
        { className: "muted", style: { marginBottom: "10px" } },
        "Full-snapshot details (on-demand; not limited to this neighborhood view).",
      ),
      heading("Contexts"),
      list(Array.isArray(payload.contexts) ? payload.contexts : [], (c) => [
        entityLink(c),
      ]),
      heading("Equivalences", "14px"),
      list(
        Array.isArray(payload.equivalences) ? payload.equivalences : [],
        (e) => [
          entityLink(e?.other),
          " ",
          e?.kind ? span(`(${e.kind})`) : null,
        ],
      ),
      heading("Outgoing", "14px"),
      ...groups(Array.isArray(payload.outgoing) ? payload.outgoing : []),
      heading("Incoming", "14px"),
      ...groups(Array.isArray(payload.incoming) ? payload.incoming : []),
    ];
  }

  function renderAttrs(cats) {
    function rows(pairs) {
      return pairs.map(([k, v]) =>
        row(
          code(k),
          ["text", "search_text", "markdown"].includes(k)
            ? pre(String(v), "240px")
            : String(v),
        ),
      );
    }
    return [
      ...(cats.content.length
        ? [heading("Content"), table([], rows(cats.content))]
        : [muted("(no content fields)")]),
      ...[
        ["Other attributes", cats.other, !!cats.other.length],
        ["Axi metadata", cats.axi, false],
        ["Overlay attributes", cats.overlay, false],
      ].map(([title, pairs, open]) =>
        group(
          [title, " ", span(`(${pairs.length})`)],
          [
            table(
              [],
              pairs.length ? rows(pairs) : [row(span("(none)"), "")],
              "8px",
            ),
          ],
          open,
          "12px",
        ),
      ),
    ];
  }

  function overviewExtra(n, attrs, outgoing) {
    if (n.entity_type === "ProposalRun")
      return kvTable([
        ["schema_hint", attrs.schema_hint],
        ["source_type", attrs.source_type],
        ["source_locator", attrs.source_locator],
        ["generated_at", attrs.generated_at],
        [
          "proposals_digest",
          attrs.proposals_digest ? shortenHash(attrs.proposals_digest) : "",
        ],
        [
          "visible_proposals",
          String(outgoing.filter((e) => e.label === "run_has_proposal").length),
        ],
      ]);
    if (n.entity_type === "Document")
      return kvTable([
        ["document_id", attrs.document_id],
        [
          "visible_chunks",
          String(
            outgoing.filter((e) => e.label === "document_has_chunk").length,
          ),
        ],
      ]);
    if (n.entity_type === "DocChunk")
      return kvTable(
        ["chunk_id", "document_id", "span_id", "page"].map((k) => [
          k,
          attrs[k],
        ]),
      );
    if (n.entity_type !== "AxiMetaTheory") return null;
    const blocks = outgoing
      .filter((e) => e.label === "axi_theory_has_constraint")
      .map((e) => nodeById.get(e.target))
      .filter((n) => n?.attrs?.axi_constraint_kind === "named_block");
    const name = (b) =>
      String(b.attrs?.axi_constraint_name || b.display_name || "");
    blocks.sort((a, b) => name(a).localeCompare(name(b)));
    if (!blocks.length)
      return muted("(no named-block constraints in this view)");
    return el(
      "div",
      {},
      el(
        "div",
        { className: "muted", style: { marginBottom: "8px" } },
        "Named-block constraints are preserved as structured (but opaque) theory content.",
      ),
      table(
        ["named blocks"],
        blocks.map((b) => row(link(b.id, name(b)))),
      ),
    );
  }

  function renderOverview(n, attrs, outgoing, summary) {
    const plane = n.plane ? String(n.plane) : "";
    const parts = [
      el(
        "div",
        { style: { marginBottom: "10px" } },
        el(
          "div",
          { className: "muted" },
          "type: ",
          code(entityTypeDisplayLabel(n.entity_type)),
          " • id: ",
          code(n.id),
          " • kind: ",
          code(kindDisplayLabel(n.kind || "entity")),
          ...(plane ? [" • plane: ", code(plane)] : []),
        ),
        summary
          ? el(
              "div",
              { className: "muted", style: { marginTop: "6px" } },
              "summary: ",
              code(summary),
            )
          : null,
      ),
      overviewExtra(n, attrs, outgoing),
    ];
    if (ui.pathStart == null || ui.pathEnd == null)
      return [
        ...parts,
        muted("(tip: shift-click 2 nodes to highlight a path)"),
      ];
    const rows = ui.pathEdgeIdxs.map((idx, i) => {
      const e = graph.edges[idx];
      if (!e) return row(String(i), span("(edge unavailable)"), "");
      const s = nodeById.get(e.source),
        t = nodeById.get(e.target);
      return row(
        String(i),
        code(e.label),
        `${s ? nodeTitle(s) : e.source} → ${t ? nodeTitle(t) : e.target}`,
      );
    });
    return [
      ...parts,
      heading("Selected path"),
      muted(
        "Shift-click two nodes (in list or graph) to highlight a shortest path (within the current filtered subgraph).",
      ),
      table(
        ["step", "edge", "between"],
        rows.length ? rows : [row(span("(no path found)"), "", "")],
        "8px",
      ),
    ];
  }

  function renderDetail(id) {
    const n = nodeById.get(id);
    if (!n) {
      detailEl.replaceChildren();
      return;
    }
    detailEl.classList.remove("muted");
    const outgoing = (outEdgesBySource.get(id) || []).filter(isEdgeVisible);
    const incoming = (inEdgesByTarget.get(id) || []).filter(isEdgeVisible);
    const tupleSummary =
      n.kind === "fact"
        ? factSummary(n)
        : n.kind === "morphism"
          ? morphismSummary(n)
          : n.kind === "homotopy"
            ? homotopySummary(n)
            : null;
    const summary =
      tupleSummary ||
      (n.entity_type === "ProposalRun"
        ? proposalRunSummary(n)
        : n.entity_type === "Document"
          ? documentSummary(n)
          : n.entity_type === "DocChunk"
            ? docChunkSummary(n)
            : null);
    const attrs = n.attrs || {};
    const tabs = [
      ["overview", "overview"],
      ["facts", isTupleLike(n) ? "tuple" : "facts"],
      ["edges", "edges"],
      ["attrs", "attrs"],
      ["db", "db"],
    ];
    const buttons = tabs.map(([tab, label]) => {
      const button = el(
        "button",
        { className: "detailtabbtn", dataset: { dtab: tab } },
        label,
      );
      button.type = "button";
      button.addEventListener("click", () => setActiveDetailTab(tab));
      return button;
    });
    const panel = (tab, children) =>
      el(
        "div",
        { className: "detailtabpanel", dataset: { dtab: tab } },
        ...children,
      );
    detailEl.replaceChildren(
      el("h2", { style: { margin: "0 0 6px 0" } }, nodeTitle(n)),
      el("div", { className: "detailtabs" }, ...buttons),
      panel("overview", renderOverview(n, attrs, outgoing, summary)),
      panel(
        "facts",
        renderMentions(n, attrs, outgoing, incoming, tupleSummary),
      ),
      panel("edges", [
        heading("Outgoing"),
        ...renderEdgesGrouped(outgoing, "out"),
        heading("Incoming", "16px"),
        ...renderEdgesGrouped(incoming, "in"),
      ]),
      panel("attrs", renderAttrs(categorizeAttrs(attrs))),
      panel("db", renderDbDescribe(id)),
    );
    setActiveDetailTab(ui.detailTab || (isTupleLike(n) ? "facts" : "overview"));
  }

  return { renderDetail };
}
