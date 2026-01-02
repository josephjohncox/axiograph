// @ts-nocheck

export function parseTextList(text) {
  const raw = String(text || "");
  const parts = raw
    .split(/[\n,;]+/g)
    .map(s => s.trim())
    .filter(s => s.length > 0);
  const seen = new Set();
  const out = [];
  for (const p of parts) {
    const key = p.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(p);
  }
  return out;
}
