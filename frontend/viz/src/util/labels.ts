import type { GraphNode } from "../types";

export interface RelationSignature {
  relation: string | null;
  fields: Array<{ name: string; ty: string | null }>;
}

export function entityTypeDisplayLabel(entityType: unknown): string {
  const raw = String(entityType || "").trim();
  if (!raw) return "(unknown)";
  if (raw === "Homotopy") return "Path equivalence";
  return raw;
}

export function kindDisplayLabel(kind: unknown): string {
  const raw = String(kind || "").trim();
  if (!raw || raw === "entity") return "entity";
  if (raw === "homotopy") return "path equivalence";
  return raw;
}

export function nodeDisplayName(n: GraphNode | null | undefined): string {
  if (!n) return "";
  if (n.display_name) return String(n.display_name);
  if (n.name) return String(n.name);
  if (n.attrs && n.attrs.name) return String(n.attrs.name);
  return "";
}

export function nodeTitle(n: GraphNode): string {
  const typeLabel = entityTypeDisplayLabel(n && n.entity_type);
  const base = `${typeLabel}#${n.id}`;
  const display = nodeDisplayName(n);
  if (display && display !== base) {
    return `${base} — ${display}`;
  }
  return base;
}

export function effectiveTypeLabel(n: GraphNode | null | undefined): string {
  if (!n) return "(unknown)";
  if (n.type_label) return entityTypeDisplayLabel(n.type_label);
  if ((n.kind === "fact" || n.kind === "morphism" || n.kind === "homotopy") && n.attrs && n.attrs.axi_relation) {
    return String(n.attrs.axi_relation);
  }
  return entityTypeDisplayLabel(n.entity_type);
}

export function nodeColor(n: GraphNode): string {
  if (n.kind === "meta") return "#d6d6d6";
  if (n.kind === "morphism") return "#c6f6d5";
  if (n.kind === "homotopy") return "#e9d8fd";
  if (n.kind === "fact") return "#ffe08a";
  return "#9ec5ff";
}

export function planeStrokeColor(n: GraphNode | null | undefined): string {
  const plane = n && n.plane ? String(n.plane) : "";
  if (plane === "accepted") return "#1b5e20";
  if (plane === "evidence") return "#b85c00";
  if (plane === "data") return "#2b7fff";
  if (plane === "meta") return "#777";
  return "#666";
}

export function parseRelationSignatureFieldOrder(sig: unknown): string[] | null {
  const parsed = parseRelationSignature(sig);
  if (!parsed || !parsed.fields || !parsed.fields.length) return null;
  return parsed.fields.map(f => f.name);
}

export function parseRelationSignature(sig: unknown): RelationSignature | null {
  if (!sig) return null;
  const s = String(sig);
  const i0 = s.indexOf("(");
  const i1 = s.lastIndexOf(")");
  if (i0 < 0 || i1 <= i0) return null;
  const relation = s.slice(0, i0).trim() || null;
  const inner = s.slice(i0 + 1, i1).trim();
  if (!inner) return { relation, fields: [] };
  const fields: RelationSignature["fields"] = [];
  for (const part of inner.split(",")) {
    const p = part.trim();
    if (!p) continue;
    const [nameRaw, tyRaw] = p.split(":");
    const name = (nameRaw || "").trim();
    const ty = (tyRaw || "").trim();
    if (!name) continue;
    fields.push({ name, ty: ty || null });
  }
  return { relation, fields };
}

export function nodeShortLabel(n: GraphNode | null | undefined): string {
  if (!n) return "(unknown)";
  if (n.name) return String(n.name);
  return `${entityTypeDisplayLabel(n.entity_type)}#${n.id}`;
}

export function isTupleLike(n: GraphNode | null | undefined): boolean {
  if (!n) return false;
  return n.kind === "fact" || n.kind === "morphism" || n.kind === "homotopy";
}

export function runIdForNode(n: GraphNode | null | undefined): string {
  if (!n || !n.attrs) return "";
  const raw = n.attrs.meta_axiograph_predictive_proposals_trace_id || n.attrs.axiograph_predictive_proposals_trace_id;
  return raw ? String(raw) : "";
}
