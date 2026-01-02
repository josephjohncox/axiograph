// @ts-nocheck

export function initContextMenu(ctx) {
  const { svg, ui, nodeById, nodeDisplayName, nodeTitle, searchEl } = ctx;

  const menu = document.createElement("div");
  menu.id = "context_menu";
  menu.className = "context-menu";
  menu.style.display = "none";
  document.body.appendChild(menu);

  function hideContextMenu() {
    menu.style.display = "none";
    menu.innerHTML = "";
  }

  function copyText(text) {
    if (text == null) return;
    const value = String(text);
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(value).catch(() => {});
      return;
    }
    const ta = document.createElement("textarea");
    ta.value = value;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    try { document.execCommand("copy"); } catch (_e) {}
    document.body.removeChild(ta);
  }

  function addItem(label, action, opts = {}) {
    if (opts.separator) {
      const sep = document.createElement("div");
      sep.className = "context-menu-sep";
      menu.appendChild(sep);
      return;
    }
    const item = document.createElement("button");
    item.type = "button";
    item.className = "context-menu-item";
    item.textContent = label;
    if (opts.disabled) {
      item.disabled = true;
    }
    if (opts.danger) {
      item.classList.add("danger");
    }
    item.addEventListener("click", (ev) => {
      ev.preventDefault();
      ev.stopPropagation();
      hideContextMenu();
      if (!opts.disabled && action) action();
    });
    menu.appendChild(item);
  }

  function showAt(x, y) {
    const pad = 8;
    menu.style.display = "block";
    const rect = menu.getBoundingClientRect();
    const maxX = window.innerWidth - rect.width - pad;
    const maxY = window.innerHeight - rect.height - pad;
    const left = Math.max(pad, Math.min(x, maxX));
    const top = Math.max(pad, Math.min(y, maxY));
    menu.style.left = `${left}px`;
    menu.style.top = `${top}px`;
  }

  function setSelectValue(selectEl, value) {
    if (!selectEl) return;
    selectEl.value = value;
    selectEl.dispatchEvent(new Event("change", { bubbles: true }));
  }

  function showNodeMenu(nodeId, x, y) {
    const n = nodeById ? nodeById.get(nodeId) : null;
    const name = n ? (nodeDisplayName ? nodeDisplayName(n) : (n.name || "")) : "";
    const title = n ? (nodeTitle ? nodeTitle(n) : `${n.entity_type || "Entity"}#${n.id}`) : `#${nodeId}`;

    addItem(title, null, { disabled: true });
    addItem("Select node", () => ctx.selectNode && ctx.selectNode(nodeId, false));
    addItem("Center view", () => ctx.centerViewOnNode && ctx.centerViewOnNode(nodeId), { disabled: !ctx.centerViewOnNode });
    addItem("Set as path start", () => {
      ui.pathStart = nodeId;
      ui.pathMessage = "";
      if (ctx.updatePathStatus) ctx.updatePathStatus();
      if (ctx.rerender) ctx.rerender();
    });
    addItem("Set as path end", () => {
      ui.pathEnd = nodeId;
      ui.pathMessage = "";
      if (ctx.updatePathStatus) ctx.updatePathStatus();
      if (ctx.rerender) ctx.rerender();
    });
    addItem("", null, { separator: true });
    addItem("Clear path", () => {
      if (ctx.clearPath) ctx.clearPath(ctx);
      if (ctx.rerender) ctx.rerender();
    }, { danger: true });
    addItem("Certify path", () => ctx.certifySelectedPath && ctx.certifySelectedPath(false), { disabled: !ctx.certifySelectedPath });
    addItem("Verify path", () => ctx.certifySelectedPath && ctx.certifySelectedPath(true), { disabled: !ctx.certifySelectedPath });
    addItem("", null, { separator: true });
    addItem("Focus search on name", () => {
      if (!searchEl) return;
      searchEl.value = name || "";
      if (ctx.rerender) ctx.rerender();
    }, { disabled: !searchEl });
    addItem("Copy name", () => copyText(name), { disabled: !name });
    addItem("Copy type", () => copyText(n ? n.entity_type : ""), { disabled: !n || !n.entity_type });
    addItem("Copy id", () => copyText(nodeId));

    showAt(x, y);
  }

  function showPanelMenu(x, y) {
    addItem("Fit view", () => ctx.fitViewToLayoutBounds && ctx.fitViewToLayoutBounds(), { disabled: !ctx.fitViewToLayoutBounds });
    addItem("Reset view", () => ctx.resetViewToDefault && ctx.resetViewToDefault(), { disabled: !ctx.resetViewToDefault });
    addItem("Clear path", () => {
      if (ctx.clearPath) ctx.clearPath(ctx);
      if (ctx.rerender) ctx.rerender();
    }, { danger: true });
    addItem("", null, { separator: true });
    addItem("Labels: smart", () => setSelectValue(ctx.labelDensityEl, "smart"), { disabled: !ctx.labelDensityEl });
    addItem("Labels: all", () => setSelectValue(ctx.labelDensityEl, "all"), { disabled: !ctx.labelDensityEl });
    addItem("Labels: none", () => setSelectValue(ctx.labelDensityEl, "none"), { disabled: !ctx.labelDensityEl });
    addItem("", null, { separator: true });
    addItem("Layout: radial", () => setSelectValue(ctx.layoutAlgoEl, "radial"), { disabled: !ctx.layoutAlgoEl });
    addItem("Layout: grid", () => setSelectValue(ctx.layoutAlgoEl, "grid"), { disabled: !ctx.layoutAlgoEl });
    addItem("Layout: type columns", () => setSelectValue(ctx.layoutAlgoEl, "type_columns"), { disabled: !ctx.layoutAlgoEl });
    addItem("Layout: random", () => setSelectValue(ctx.layoutAlgoEl, "random"), { disabled: !ctx.layoutAlgoEl });
    addItem("", null, { separator: true });
    addItem("Other component", () => {
      if (!ctx.pickRandomComponentNode || !ctx.selectNode) return;
      const current = ctx.selectedIdRef ? ctx.selectedIdRef() : null;
      const nextId = ctx.pickRandomComponentNode(current);
      if (nextId != null) ctx.selectNode(nextId, false);
      if (ctx.rerender) ctx.rerender();
    }, { disabled: !ctx.pickRandomComponentNode });

    showAt(x, y);
  }

  function showContextMenu(opts) {
    if (!opts) return;
    menu.innerHTML = "";
    if (opts.kind === "node") {
      showNodeMenu(opts.nodeId, opts.x, opts.y);
    } else {
      showPanelMenu(opts.x, opts.y);
    }
  }

  document.addEventListener("click", hideContextMenu);
  window.addEventListener("blur", hideContextMenu);
  window.addEventListener("resize", hideContextMenu);
  document.addEventListener("keydown", (ev) => { if (ev.key === "Escape") hideContextMenu(); });

  if (svg) {
    svg.addEventListener("contextmenu", (ev) => {
      if (!ev) return;
      // If a node handler fired, it will stopPropagation.
      ev.preventDefault();
      showContextMenu({ kind: "panel", x: ev.clientX, y: ev.clientY });
    });
  }

  Object.assign(ctx, { showContextMenu, hideContextMenu });
  return { showContextMenu, hideContextMenu };
}
