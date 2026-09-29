import type { GraphAttributes } from "../types";

export interface AttributeCategories {
  content: Array<[string, unknown]>;
  overlay: Array<[string, unknown]>;
  axi: Array<[string, unknown]>;
  other: Array<[string, unknown]>;
}

export function categorizeAttrs(attrs: GraphAttributes): AttributeCategories {
  const out: AttributeCategories = { content: [], overlay: [], axi: [], other: [] };
  for (const [k, v] of Object.entries(attrs || {})) {
    if (k === "text" || k === "search_text" || k === "markdown") out.content.push([k, v]);
    else if (k.startsWith("axi_overlay_")) out.overlay.push([k, v]);
    else if (k.startsWith("axi_")) out.axi.push([k, v]);
    else out.other.push([k, v]);
  }
  return out;
}
