// Context IDs are numeric graph entity IDs, not display names or string keys.
export interface ContextData {
  factContexts: ReadonlyMap<number, ReadonlySet<number>>;
  contextNameById: ReadonlyMap<number, string>;
}

export interface ContextFilterState extends ContextData {
  contextFilterEl?: HTMLSelectElement | null;
  contextBadgeEl?: HTMLElement | null;
}

export interface ContextOption {
  value: string;
  text: string;
}

export function contextFilterOptions(ctx: ContextData): ContextOption[] {
  const ids = new Set<number>();
  for (const contexts of ctx.factContexts.values()) {
    for (const id of contexts) ids.add(id);
  }
  if (ids.size === 0) return [{ value: "*", text: "(no contexts)" }];

  return [
    { value: "*", text: "(all)" },
    { value: "__none__", text: "(no context)" },
    ...Array.from(ids)
      .sort((a, b) => a - b)
      .map((id) => ({
        value: String(id),
        text: ctx.contextNameById.get(id) || `Context#${id}`,
      })),
  ];
}

export function initContextFilter(ctx: ContextFilterState): void {
  const { contextFilterEl } = ctx;
  if (!contextFilterEl) return;
  const options = contextFilterOptions(ctx);
  contextFilterEl.replaceChildren();
  for (const { value, text } of options) {
    const option = contextFilterEl.ownerDocument.createElement("option");
    option.value = value;
    option.textContent = text;
    contextFilterEl.appendChild(option);
  }
  contextFilterEl.disabled = options.length === 1;
  updateContextBadge(ctx);
}

export function selectedContextFilter(ctx: ContextFilterState): string {
  const { contextFilterEl } = ctx;
  if (!contextFilterEl || contextFilterEl.disabled) return "*";
  return contextFilterEl.value || "*";
}

export function currentContextNameFromFilter(
  ctx: ContextFilterState,
): string | null {
  const v = selectedContextFilter(ctx);
  if (v === "*" || v === "__none__") return null;
  const id = Number(v);
  if (!Number.isFinite(id)) return null;
  return ctx.contextNameById.get(id) || null;
}

export function updateContextBadge(ctx: ContextFilterState): void {
  const { contextBadgeEl, contextNameById } = ctx;
  if (!contextBadgeEl) return;
  const v = selectedContextFilter(ctx);
  if (v === "*") {
    contextBadgeEl.style.display = "none";
    contextBadgeEl.textContent = "";
    return;
  }
  let label = "context";
  if (v === "__none__") label = "context: none";
  else {
    const id = Number(v);
    const name = Number.isFinite(id)
      ? contextNameById.get(id) || `Context#${id}`
      : v;
    label = `context: ${name}`;
  }
  contextBadgeEl.textContent = label;
  contextBadgeEl.style.display = "inline-flex";
}
