// Text children are always DOM text, never markup. Only code-owned properties
// belong in options; navigation is constructed separately at its call site.
export type DomChild = Node | string | number | null | undefined | false;
export interface DomOptions {
  className?: string;
  style?: Partial<CSSStyleDeclaration>;
  dataset?: Record<string, string>;
}

export function element<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  options: DomOptions = {},
  ...children: DomChild[]
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (options.className) node.className = options.className;
  if (options.style) Object.assign(node.style, options.style);
  if (options.dataset) Object.assign(node.dataset, options.dataset);
  for (const child of children) {
    if (child === null || child === undefined || child === false) continue;
    node.append(typeof child === "number" ? String(child) : child);
  }
  return node;
}

export function muted(text: string): HTMLDivElement {
  return element("div", { className: "muted" }, text);
}
