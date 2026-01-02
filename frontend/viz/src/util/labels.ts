// @ts-nocheck

export function nodeDisplayName(n) {
  if (!n) return "";
  if (n.display_name) return String(n.display_name);
  if (n.name) return String(n.name);
  if (n.attrs && n.attrs.name) return String(n.attrs.name);
  return "";
}

export function nodeTitle(n) {
  const display = nodeDisplayName(n);
  if (display && display !== `${n.entity_type}#${n.id}`) {
    return `${n.entity_type}#${n.id} — ${display}`;
  }
  return `${n.entity_type}#${n.id}`;
}

export function effectiveTypeLabel(n) {
  if (!n) return "(unknown)";
  if (n.type_label) return String(n.type_label);
  if ((n.kind === "fact" || n.kind === "morphism" || n.kind === "homotopy") && n.attrs && n.attrs.axi_relation) {
    return String(n.attrs.axi_relation);
  }
  return n.entity_type || "(unknown)";
}

export function nodeColor(n) {
  if (n.kind === "meta") return "#d6d6d6";
  if (n.kind === "morphism") return "#c6f6d5";
  if (n.kind === "homotopy") return "#e9d8fd";
  if (n.kind === "fact") return "#ffe08a";
  return "#9ec5ff";
}

export function planeStrokeColor(n) {
  const plane = n && n.plane ? String(n.plane) : "";
  if (plane === "accepted") return "#1b5e20";
  if (plane === "evidence") return "#b85c00";
  if (plane === "data") return "#2b7fff";
  if (plane === "meta") return "#777";
  return "#666";
}

export function parseRelationSignatureFieldOrder(sig) {
  const parsed = parseRelationSignature(sig);
  if (!parsed || !parsed.fields || !parsed.fields.length) return null;
  return parsed.fields.map(f => f.name);
}

export function parseRelationSignature(sig) {
  if (!sig) return null;
  const s = String(sig);
  const i0 = s.indexOf("(");
  const i1 = s.lastIndexOf(")");
  if (i0 < 0 || i1 <= i0) return null;
  const relation = s.slice(0, i0).trim() || null;
  const inner = s.slice(i0 + 1, i1).trim();
  if (!inner) return { relation, fields: [] };
  const fields = [];
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

export function nodeShortLabel(n) {
  if (!n) return "(unknown)";
  if (n.name) return String(n.name);
  return `${n.entity_type}#${n.id}`;
}

export function isTupleLike(n) {
  if (!n) return false;
  return n.kind === "fact" || n.kind === "morphism" || n.kind === "homotopy";
}

export function runIdForNode(n) {
  if (!n || !n.attrs) return "";
  const raw = n.attrs.meta_axiograph_world_model_trace_id || n.attrs.axiograph_world_model_trace_id;
  return raw ? String(raw) : "";
}
