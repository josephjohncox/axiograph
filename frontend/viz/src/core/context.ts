export function initContextFilter(ctx) {
  const { factContexts, contextNameById, contextFilterEl } = ctx;
  if (!contextFilterEl) return;
  contextFilterEl.innerHTML = "";

  const hasAny = factContexts.size > 0;
  if (!hasAny) {
    const opt = document.createElement("option");
    opt.value = "*";
    opt.textContent = "(no contexts)";
    contextFilterEl.appendChild(opt);
    contextFilterEl.disabled = true;
    return;
  }

  function addOpt(value, text) {
    const opt = document.createElement("option");
    opt.value = value;
    opt.textContent = text;
    contextFilterEl.appendChild(opt);
  }

  addOpt("*", "(all)");
  addOpt("__none__", "(no context)");

  const ids = new Set();
  for (const ctxs of factContexts.values()) {
    for (const id of ctxs.values()) ids.add(id);
  }
  const sorted = Array.from(ids).sort((a,b) => a-b);
  for (const id of sorted) {
    const name = contextNameById.get(id) || `Context#${id}`;
    addOpt(String(id), name);
  }

  if (ctx.updateContextBadge) ctx.updateContextBadge();
}

export function selectedContextFilter(ctx) {
  const { contextFilterEl } = ctx;
  if (!contextFilterEl || contextFilterEl.disabled) return "*";
  return String(contextFilterEl.value || "*");
}

export function currentContextNameFromFilter(ctx) {
  const { contextNameById, contextFilterEl } = ctx;
  if (!contextFilterEl) return null;
  const v = contextFilterEl.value || "*";
  if (v === "*" || v === "__none__") return null;
  const id = Number(v);
  if (!Number.isFinite(id)) return null;
  return contextNameById.get(id) || null;
}

export function updateContextBadge(ctx) {
  const { contextBadgeEl, contextFilterEl, contextNameById } = ctx;
  if (!contextBadgeEl || !contextFilterEl) return;
  const v = String(contextFilterEl.value || "*");
  if (v === "*") {
    contextBadgeEl.style.display = "none";
    contextBadgeEl.textContent = "";
    return;
  }
  let label = "context";
  if (v === "__none__") label = "context: none";
  else {
    const id = Number(v);
    const name = Number.isFinite(id) ? (contextNameById.get(id) || `Context#${id}`) : String(v);
    label = `context: ${name}`;
  }
  contextBadgeEl.textContent = label;
  contextBadgeEl.style.display = "inline-flex";
}
