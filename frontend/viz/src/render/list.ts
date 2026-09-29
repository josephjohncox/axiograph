import { kindDisplayLabel } from "../util/labels";
import { element, muted } from "./dom";
import type { GraphNode, GraphPayload, VizUiState } from "../types";

interface NodeListContext {
  graph: GraphPayload;
  ui: VizUiState;
  nodesEl: HTMLElement;
  isNodeVisible: (node: GraphNode) => boolean;
  nodeDisplayName: (node: GraphNode) => string;
  effectiveTypeLabel: (node: GraphNode) => string;
  selectNode: (id: number, shiftKey: boolean) => void;
  selectedIdRef: () => number | null;
}

interface NodeListItem {
  node: GraphNode;
  disp: string;
  kind: string;
  kindLabel: string;
  entityType: string;
}

function appendNodeLabel(
  parent: HTMLElement,
  type: string,
  id: number,
  kind: string,
  name: string,
  highlighted: boolean,
) {
  parent.append(
    element(
      "div",
      {},
      element("strong", {}, type),
      " ",
      element("span", { className: "muted" }, `#${id} • ${kind}`),
    ),
    name
      ? element("div", {}, `${highlighted ? "★ " : ""}${name}`)
      : muted("(no name)"),
  );
}

export function renderNodeList(ctx: NodeListContext, filter: string): void {
  const {
    graph,
    ui,
    nodesEl,
    isNodeVisible,
    nodeDisplayName,
    effectiveTypeLabel,
    selectNode,
    selectedIdRef,
  } = ctx;

  nodesEl.replaceChildren();
  nodesEl.classList.remove("node-list-virtual");
  const f = (filter || "").trim().toLowerCase();

  const items: NodeListItem[] = [];
  for (const n of graph.nodes) {
    if (!isNodeVisible(n)) continue;
    const disp = nodeDisplayName(n);
    const kind = n.kind || "entity";
    const kindLabel = kindDisplayLabel(kind);
    const entityType = effectiveTypeLabel(n);
    const hay =
      `${n.id} ${n.entity_type} ${kind} ${kindLabel} ${entityType} ${disp}`.toLowerCase();
    if (f && !hay.includes(f)) continue;
    items.push({ node: n, disp, kind, kindLabel, entityType });
  }

  if (!items.length) {
    const empty = muted("(no matching nodes)");
    empty.style.marginTop = "8px";
    nodesEl.appendChild(empty);
    return;
  }

  const VIRTUAL_THRESHOLD = 2000;
  const ITEM_HEIGHT = 64;
  const OVERSCAN = 6;
  const kindOrder = new Map([
    ["entity", 0],
    ["fact", 1],
    ["morphism", 2],
    ["homotopy", 3],
    ["meta", 4],
  ]);

  function compareItems(a: NodeListItem, b: NodeListItem): number {
    const ka = kindOrder.get(a.kind) ?? 99;
    const kb = kindOrder.get(b.kind) ?? 99;
    if (ka !== kb) return ka - kb;
    if (a.entityType !== b.entityType)
      return a.entityType.localeCompare(b.entityType);
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
      const end = Math.min(
        items.length,
        start + Math.ceil(viewH / ITEM_HEIGHT) + OVERSCAN * 2,
      );
      viewport.style.transform = `translateY(${start * ITEM_HEIGHT}px)`;
      viewport.replaceChildren();
      const selectedId = selectedIdRef ? selectedIdRef() : null;

      for (let i = start; i < end; i++) {
        const item = items[i];
        const n = item.node;
        const isHighlighted = ui.highlightIds && ui.highlightIds.has(n.id);
        const div = document.createElement("div");
        div.className = "node";
        div.dataset.id = String(n.id);
        if (isHighlighted) div.classList.add("highlighted");
        if (selectedId != null && selectedId === n.id)
          div.classList.add("selected");
        appendNodeLabel(
          div,
          item.entityType,
          n.id,
          item.kindLabel,
          item.disp,
          isHighlighted,
        );
        div.addEventListener("click", (ev) => selectNode(n.id, ev.shiftKey));
        viewport.appendChild(div);
      }
    }

    let raf: number | null = null;
    scroller.addEventListener("scroll", () => {
      if (raf) cancelAnimationFrame(raf);
      raf = requestAnimationFrame(renderSlice);
    });
    renderSlice();
    return;
  }

  const groups = new Map<
    string,
    { kind: string; entityType: string; nodes: GraphNode[] }
  >(); // key -> { kind, entityType, nodes: [] }
  for (const item of items) {
    const key = `${item.kind}::${item.entityType}`;
    if (!groups.has(key))
      groups.set(key, {
        kind: item.kind,
        entityType: item.entityType,
        nodes: [],
      });
    groups.get(key)?.nodes.push(item.node);
  }

  const sortedGroups = Array.from(groups.values()).sort((a, b) => {
    const ka = kindOrder.get(a.kind) ?? 99;
    const kb = kindOrder.get(b.kind) ?? 99;
    if (ka !== kb) return ka - kb;
    return a.entityType.localeCompare(b.entityType);
  });

  const selectedId = selectedIdRef ? selectedIdRef() : null;

  for (const g of sortedGroups) {
    g.nodes.sort((a, b) =>
      compareItems(
        {
          node: a,
          disp: nodeDisplayName(a),
          kind: a.kind || "entity",
          kindLabel: kindDisplayLabel(a.kind || "entity"),
          entityType: effectiveTypeLabel(a),
        },
        {
          node: b,
          disp: nodeDisplayName(b),
          kind: b.kind || "entity",
          kindLabel: kindDisplayLabel(b.kind || "entity"),
          entityType: effectiveTypeLabel(b),
        },
      ),
    );

    const details = document.createElement("details");
    details.className = "nodegroup";
    details.open = !!f || g.nodes.length <= 20;

    const summary = document.createElement("summary");
    summary.append(
      element("strong", {}, g.entityType),
      element(
        "span",
        { className: "muted" },
        `${kindDisplayLabel(g.kind)} • ${g.nodes.length}`,
      ),
    );
    details.appendChild(summary);

    for (const n of g.nodes) {
      const disp = nodeDisplayName(n);
      const div = document.createElement("div");
      div.className = "node";
      div.dataset.id = String(n.id);
      const isHighlighted = ui.highlightIds && ui.highlightIds.has(n.id);
      if (isHighlighted) div.classList.add("highlighted");
      if (selectedId != null && selectedId === n.id)
        div.classList.add("selected");
      appendNodeLabel(
        div,
        effectiveTypeLabel(n),
        n.id,
        kindDisplayLabel(n.kind || "entity"),
        disp,
        isHighlighted,
      );
      div.addEventListener("click", (ev) => selectNode(n.id, ev.shiftKey));
      details.appendChild(div);
    }

    nodesEl.appendChild(details);
  }
}
