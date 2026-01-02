// @ts-nocheck

export function makeGraphRenderer(ctx) {
  const {
    graph,
    ui,
    svg,
    nodeById,
    outEdgesBySource,
    inEdgesByTarget,
    isNodeVisible,
    isEdgeVisible,
    nodeDisplayName,
    nodeTitle,
    nodeColor,
    planeStrokeColor,
    edgeColor,
    edgeConfidence,
    selectNode,
    visibleEdgeIdxsAll,
    opacityByConfidenceEl,
    labelDensityEl,
    bfsDepths,
    showContextMenu,
  } = ctx;

function renderGraph(selectedId) {
  const focus = (graph.summary && graph.summary.focus_ids && graph.summary.focus_ids.length)
    ? graph.summary.focus_ids
    : (graph.nodes.length ? [graph.nodes[0].id] : []);
  const anchor = (ui.layoutCenter === "selected" && selectedId != null) ? [selectedId] : focus;
  if (!anchor.length) return;

  const visibleIdxs = visibleEdgeIdxsAll();
  const algo = ui.layoutAlgo || "radial";

  // Layout is purely a UI view of the currently visible graph; it is not part
  // of the trusted kernel.
  let pos = new Map(); // id -> {x,y,d}
  let W = 1000, H = 800;

  if (algo === "type_columns") {
    const groups = new Map(); // type -> [nodes]
    for (const n of graph.nodes) {
      if (!isNodeVisible(n)) continue;
      const key = String(n.entity_type || "Entity");
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key).push(n);
    }
    const types = Array.from(groups.keys()).sort((a,b) => a.localeCompare(b));
    for (const t of types) groups.get(t).sort((a,b) => a.id - b.id);

    const margin = 80;
    const colW = 220;
    const rowH = 70;
    const maxRows = Math.max(1, ...types.map(t => groups.get(t).length));
    W = Math.max(1000, margin * 2 + types.length * colW);
    H = Math.max(800, margin * 2 + maxRows * rowH);
    for (let ci = 0; ci < types.length; ci++) {
      const t = types[ci];
      const ns = groups.get(t);
      for (let ri = 0; ri < ns.length; ri++) {
        const x = margin + ci * colW + colW / 2;
        const y = margin + ri * rowH + rowH / 2;
        pos.set(ns[ri].id, { x, y, d: 0 });
      }
    }
  } else if (algo === "random") {
    // Deterministic PRNG based on layoutSeed + node id (no Math.random, so
    // refresh is reproducible).
    const seed = (Number(ui.layoutSeed || 0) ^ 0x9e3779b9) >>> 0;
    function rnd32(x) {
      // xorshift32
      let v = (x ^ seed) >>> 0;
      v ^= v << 13; v >>>= 0;
      v ^= v >>> 17; v >>>= 0;
      v ^= v << 5; v >>>= 0;
      return v >>> 0;
    }
    const margin = 80;
    W = 1000; H = 800;
    for (const n of graph.nodes) {
      if (!isNodeVisible(n)) continue;
      const r1 = rnd32(n.id * 2654435761);
      const r2 = rnd32(n.id * 1597334677 + 1013904223);
      const x = margin + (r1 / 0xffffffff) * (W - 2 * margin);
      const y = margin + (r2 / 0xffffffff) * (H - 2 * margin);
      pos.set(n.id, { x, y, d: 0 });
    }
  } else {
    // BFS-based layouts: compute depths from the selected anchor.
    const depth = bfsDepths(anchor, visibleIdxs);
    const maxDepth = Math.max(...Array.from(depth.values()), 0);
    const rings = [];
    for (let d = 0; d <= maxDepth; d++) rings.push([]);
    for (const n of graph.nodes) {
      if (!isNodeVisible(n)) continue;
      const d = depth.has(n.id) ? depth.get(n.id) : (maxDepth + 1);
      if (!rings[d]) rings[d] = [];
      rings[d].push(n);
    }
    for (const ring of rings) ring.sort((a,b) => a.id - b.id);

    const margin = 80;

    if (algo === "grid") {
      const layerW = 90;
      const layerH = 90;
      const maxLayer = Math.max(1, ...rings.map(r => r ? r.length : 0));
      W = Math.max(1000, margin * 2 + maxLayer * layerW);
      H = Math.max(800, margin * 2 + rings.length * layerH);
      for (let d = 0; d < rings.length; d++) {
        const ring = rings[d];
        if (!ring || !ring.length) continue;
        const count = ring.length;
        const usableW = W - 2 * margin;
        const span = Math.max(0, (count - 1) * layerW);
        const x0 = margin + Math.max(0, (usableW - span) / 2);
        for (let i = 0; i < count; i++) {
          const x = x0 + i * layerW;
          const y = margin + d * layerH + layerH / 2;
          pos.set(ring[i].id, { x, y, d });
        }
      }
    } else {
      // Default: radial.
      W = 1000; H = 800;
      const cx = W / 2, cy = H / 2;
      const ringStep = 120;
      const baseRadius = 40;

      for (let d = 0; d < rings.length; d++) {
        const ring = rings[d];
        if (!ring || !ring.length) continue;
        const radius = baseRadius + d * ringStep;
        const count = ring.length;
        const offset = (Number(ui.layoutSeed || 0) + d * 97) % Math.max(1, count);
        for (let i = 0; i < count; i++) {
          const j = (i + offset) % count;
          const ang = (2 * Math.PI * j) / count - Math.PI / 2;
          const x = cx + radius * Math.cos(ang);
          const y = cy + radius * Math.sin(ang);
          pos.set(ring[i].id, { x, y, d });
        }
      }
    }
  }

  ui.nodePos = pos;

  // Preserve any user pan/zoom state (viewBox is managed by handlers below).
  if (!svg.getAttribute("viewBox")) svg.setAttribute("viewBox", `0 0 ${W} ${H}`);
  svg.innerHTML = "";

  const defs = document.createElementNS("http://www.w3.org/2000/svg", "defs");
  defs.innerHTML = `
    <marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
      <path d="M 0 0 L 10 5 L 0 10 z" fill="#666"></path>
    </marker>
  `;
  svg.appendChild(defs);

  // Bounds for “fit” and other view helpers.
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  let visibleCount = 0;
  const density = labelDensityEl ? String(labelDensityEl.value || "smart") : "smart";
  const labelAll = density === "all" ? true : (density === "none" ? false : (visibleCount <= 900));
  for (const n of graph.nodes) {
    if (!isNodeVisible(n)) continue;
    const p = pos.get(n.id);
    if (!p) continue;
    minX = Math.min(minX, p.x);
    minY = Math.min(minY, p.y);
    maxX = Math.max(maxX, p.x);
    maxY = Math.max(maxY, p.y);
    visibleCount++;
  }
  if (Number.isFinite(minX) && Number.isFinite(minY) && Number.isFinite(maxX) && Number.isFinite(maxY)) {
    ui.layoutBounds = { minX, minY, maxX, maxY, W, H };
  } else {
    ui.layoutBounds = { minX: 0, minY: 0, maxX: W, maxY: H, W, H };
  }

  const maxDrawEdges = 2200;
  const pathEdgeSet = new Set(ui.pathEdgeIdxs || []);
  let drawEdgeIdxs = visibleIdxs.slice();
  if (drawEdgeIdxs.length > maxDrawEdges) {
    const keep = new Set(drawEdgeIdxs.slice(0, maxDrawEdges));
    for (const idx of pathEdgeSet) keep.add(idx);
    drawEdgeIdxs = Array.from(keep).sort((a,b) => a - b);
  }

  const neighborSet = new Set();
  if (selectedId != null) {
    for (const idx of visibleIdxs) {
      const e = graph.edges[idx];
      if (e.source === selectedId) neighborSet.add(e.target);
      if (e.target === selectedId) neighborSet.add(e.source);
    }
  }

  for (const idx of drawEdgeIdxs) {
    const e = graph.edges[idx];
    const a = pos.get(e.source);
    const b = pos.get(e.target);
    if (!a || !b) continue;
    const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
    line.setAttribute("x1", a.x);
    line.setAttribute("y1", a.y);
    line.setAttribute("x2", b.x);
    line.setAttribute("y2", b.y);
    const onPath = pathEdgeSet.has(idx);
    const touchesSelected = (selectedId != null) && (e.source === selectedId || e.target === selectedId);
    const conf01 = edgeConfidence(e);
    line.setAttribute("stroke", onPath ? "#ff006e" : edgeColor(e));
    const baseWidth = onPath ? 2.8 : (touchesSelected ? 2.1 : (e.kind === "equivalence" ? 1.5 : 1.2));
    const width = baseWidth * (0.85 + 0.30 * conf01);
    line.setAttribute("stroke-width", String(width.toFixed(2)));
    const baseOpacity = onPath ? 0.95 : (selectedId != null && !touchesSelected ? 0.45 : 0.85);
    const confOpacity = (opacityByConfidenceEl && opacityByConfidenceEl.checked) ? (0.20 + 0.80 * conf01) : 1.0;
    line.setAttribute("opacity", String((baseOpacity * confOpacity).toFixed(3)));
    if (e.kind === "equivalence") line.setAttribute("stroke-dasharray", "4 4");
    if (e.kind === "meta_relation" || e.kind === "meta_virtual") line.setAttribute("stroke-dasharray", "2 4");
    line.setAttribute("marker-end", "url(#arrow)");
    const title = document.createElementNS("http://www.w3.org/2000/svg", "title");
    const conf = e.confidence != null ? ` (${Number(e.confidence).toFixed(3)})` : "";
    title.textContent = `${e.label}${conf}`;
    line.appendChild(title);
    svg.appendChild(line);
  }

  for (const n of graph.nodes) {
    if (!isNodeVisible(n)) continue;
    const p = pos.get(n.id);
    if (!p) continue;

    const gEl = document.createElementNS("http://www.w3.org/2000/svg", "g");
    gEl.setAttribute("data-id", String(n.id));
    gEl.style.cursor = "pointer";

    const circle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
    circle.setAttribute("cx", p.x);
    circle.setAttribute("cy", p.y);
    const isSelected = n.id === selectedId;
    const isNeighbor = neighborSet.has(n.id);
    const isHighlighted = ui.highlightIds && ui.highlightIds.has(n.id);
    const baseR = isSelected ? 12 : (isNeighbor ? 10 : (isHighlighted ? 10 : 9));
    circle.setAttribute("r", String(baseR));
    circle.setAttribute("fill", nodeColor(n));
    circle.setAttribute("stroke", planeStrokeColor(n));
    circle.setAttribute("stroke-width", "2.0");
    circle.setAttribute("opacity", (selectedId != null && !isSelected && !isNeighbor) ? "0.65" : "1.0");

    if (isSelected || isNeighbor || isHighlighted) {
      const ring = document.createElementNS("http://www.w3.org/2000/svg", "circle");
      ring.setAttribute("cx", p.x);
      ring.setAttribute("cy", p.y);
      ring.setAttribute("r", String(baseR + 3));
      ring.setAttribute("fill", "none");
      ring.setAttribute("stroke", isSelected ? "#2b7fff" : (isNeighbor ? "#ff006e" : "#ff9800"));
      ring.setAttribute("stroke-width", "2.2");
      ring.setAttribute("opacity", (selectedId != null && !isSelected && !isNeighbor) ? "0.55" : "0.95");
      gEl.appendChild(ring);
    }

    gEl.appendChild(circle);
    const showLabel = (density === "none")
      ? (isSelected || isNeighbor || isHighlighted)
      : (labelAll || isSelected || isNeighbor || isHighlighted);
    if (showLabel) {
      const label = document.createElementNS("http://www.w3.org/2000/svg", "text");
      label.setAttribute("x", p.x);
      label.setAttribute("y", p.y - 14);
      label.setAttribute("text-anchor", "middle");
      label.setAttribute("font-size", "10");
      label.setAttribute("fill", "#333");
      label.setAttribute("stroke", "#fff");
      label.setAttribute("stroke-width", "3.5");
      label.setAttribute("paint-order", "stroke");
      label.setAttribute("stroke-linejoin", "round");
      const short = nodeDisplayName(n) || `${n.entity_type}#${n.id}`;
      label.textContent = short.length > 24 ? short.slice(0, 24) + "…" : short;
      label.setAttribute("opacity", (selectedId != null && !isSelected && !isNeighbor) ? "0.65" : "1.0");
      gEl.appendChild(label);
    }
    gEl.addEventListener("click", (ev) => selectNode(n.id, ev.shiftKey));
    if (showContextMenu) {
      gEl.addEventListener("contextmenu", (ev) => {
        ev.preventDefault();
        ev.stopPropagation();
        showContextMenu({ kind: "node", nodeId: n.id, x: ev.clientX, y: ev.clientY });
      });
    }
    const title = document.createElementNS("http://www.w3.org/2000/svg", "title");
    title.textContent = nodeTitle ? nodeTitle(n) : `${n.entity_type}#${n.id}`;
    gEl.appendChild(title);
    svg.appendChild(gEl);
  }
}
  return { renderGraph };
}
