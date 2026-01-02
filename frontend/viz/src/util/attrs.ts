// @ts-nocheck

export function categorizeAttrs(attrs) {
  const out = { content: [], overlay: [], axi: [], other: [] };
  for (const [k, v] of Object.entries(attrs || {})) {
    if (k === "text" || k === "search_text" || k === "markdown") out.content.push([k, v]);
    else if (k.startsWith("axi_overlay_")) out.overlay.push([k, v]);
    else if (k.startsWith("axi_")) out.axi.push([k, v]);
    else out.other.push([k, v]);
  }
  return out;
}
