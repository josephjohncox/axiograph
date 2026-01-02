// @ts-nocheck
export function renderNodeList(ctx: any, filter: string) {
  const {
    graph,
    ui,
    nodesEl,
    isNodeVisible,
    nodeDisplayName,
    effectiveTypeLabel,
    escapeHtml,
    selectNode,
    selectedIdRef,
  } = ctx;

  nodesEl.innerHTML = "";
  nodesEl.classList.remove("node-list-virtual");
  const f = (filter || "").trim().toLowerCase();

  const items = [];
  for (const n of graph.nodes) {
    if (!isNodeVisible(n)) continue;
    const disp = nodeDisplayName(n);
    const hay = `${n.id} ${n.entity_type} ${n.kind || ""} ${disp}`.toLowerCase();
    if (f && !hay.includes(f)) continue;
    const kind = n.kind || "entity";
    const entityType = effectiveTypeLabel(n);
    items.push({ node: n, disp, kind, entityType });
  }

  if (!items.length) {
    nodesEl.innerHTML = `<div class="muted" style="margin-top:8px;">(no matching nodes)</div>`;
    return;
  }

  const VIRTUAL_THRESHOLD = 2000;
  const ITEM_HEIGHT = 64;
  const OVERSCAN = 6;
  const kindOrder = new Map([["entity", 0], ["fact", 1], ["morphism", 2], ["homotopy", 3], ["meta", 4]]);

  function compareItems(a, b) {
    const ka = kindOrder.has(a.kind) ? kindOrder.get(a.kind) : 99;
    const kb = kindOrder.has(b.kind) ? kindOrder.get(b.kind) : 99;
    if (ka !== kb) return ka - kb;
    if (a.entityType !== b.entityType) return a.entityType.localeCompare(b.entityType);
    const na = a.disp || "";
    const nb = b.disp || "";
    if (na !== nb) return na.localeCompare(nb);
    return a.node.id - b.node.id;
  }

  if (items.length > VIRTUAL_THRESHOLD) {
    items.sort(compareItems);
    const note = document.createElement("div");
    note.className = "muted";
    note.style.marginTop = "6px";
    note.textContent = `showing ${items.length} nodes (virtualized)`;
    nodesEl.appendChild(note);

    const scroller = document.createElement("div");
    scroller.className = "node-list-virtual";
    nodesEl.appendChild(scroller);

    const spacer = document.createElement("div");
    spacer.className = "node-list-spacer";
    spacer.style.height = `${items.length * ITEM_HEIGHT}px`;
    scroller.appendChild(spacer);

    const viewport = document.createElement("div");
    viewport.className = "node-list-viewport";
    scroller.appendChild(viewport);

    function renderSlice() {
      const scrollTop = scroller.scrollTop || 0;
      const viewH = scroller.clientHeight || 320;
      const start = Math.max(0, Math.floor(scrollTop / ITEM_HEIGHT) - OVERSCAN);
      const end = Math.min(items.length, start + Math.ceil(viewH / ITEM_HEIGHT) + OVERSCAN * 2);
      viewport.style.transform = `translateY(${start * ITEM_HEIGHT}px)`;
      viewport.innerHTML = "";
      const selectedId = selectedIdRef ? selectedIdRef() : null;

      for (let i = start; i < end; i++) {
        const item = items[i];
        const n = item.node;
        const isHighlighted = ui.highlightIds && ui.highlightIds.has(n.id);
        const div = document.createElement("div");
        div.className = "node";
        div.dataset.id = String(n.id);
        if (isHighlighted) div.classList.add("highlighted");
        if (selectedId != null && selectedId === n.id) div.classList.add("selected");
        div.innerHTML = `<div><strong>${escapeHtml(item.entityType)}</strong> <span class="muted">#${n.id} • ${escapeHtml(item.kind)}</span></div>`
          + (item.disp ? `<div>${isHighlighted ? "★ " : ""}${escapeHtml(item.disp)}</div>` : `<div class="muted">(no name)</div>`);
        div.addEventListener("click", (ev) => selectNode(n.id, ev.shiftKey));
        viewport.appendChild(div);
      }
    }

    let raf = null;
    scroller.addEventListener("scroll", () => {
      if (raf) cancelAnimationFrame(raf);
      raf = requestAnimationFrame(renderSlice);
    });
    renderSlice();
    return;
  }

  const groups = new Map(); // key -> { kind, entityType, nodes: [] }
  for (const item of items) {
    const key = `${item.kind}::${item.entityType}`;
    if (!groups.has(key)) groups.set(key, { kind: item.kind, entityType: item.entityType, nodes: [] });
    groups.get(key).nodes.push(item.node);
  }

  const sortedGroups = Array.from(groups.values()).sort((a, b) => {
    const ka = kindOrder.has(a.kind) ? kindOrder.get(a.kind) : 99;
    const kb = kindOrder.has(b.kind) ? kindOrder.get(b.kind) : 99;
    if (ka !== kb) return ka - kb;
    return a.entityType.localeCompare(b.entityType);
  });

  const selectedId = selectedIdRef ? selectedIdRef() : null;

  for (const g of sortedGroups) {
    g.nodes.sort((a, b) => compareItems({ node: a, disp: nodeDisplayName(a), kind: a.kind || "entity", entityType: effectiveTypeLabel(a) }, { node: b, disp: nodeDisplayName(b), kind: b.kind || "entity", entityType: effectiveTypeLabel(b) }));

    const details = document.createElement("details");
    details.className = "nodegroup";
    details.open = !!f || g.nodes.length <= 20;

    const summary = document.createElement("summary");
    summary.innerHTML = `<strong>${escapeHtml(g.entityType)}</strong><span class="muted">${escapeHtml(g.kind)} • ${g.nodes.length}</span>`;
    details.appendChild(summary);

    for (const n of g.nodes) {
      const disp = nodeDisplayName(n);
      const div = document.createElement("div");
      div.className = "node";
      div.dataset.id = String(n.id);
      const isHighlighted = ui.highlightIds && ui.highlightIds.has(n.id);
      if (isHighlighted) div.classList.add("highlighted");
      if (selectedId != null && selectedId === n.id) div.classList.add("selected");
      div.innerHTML = `<div><strong>${escapeHtml(effectiveTypeLabel(n))}</strong> <span class="muted">#${n.id} • ${escapeHtml(n.kind || "entity")}</span></div>`
        + (disp ? `<div>${isHighlighted ? "★ " : ""}${escapeHtml(disp)}</div>` : `<div class="muted">(no name)</div>`);
      div.addEventListener("click", (ev) => selectNode(n.id, ev.shiftKey));
      details.appendChild(div);
    }

    nodesEl.appendChild(details);
  }
}
