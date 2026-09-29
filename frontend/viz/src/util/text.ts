
export function parseTextList(text: unknown): string[] {
  const raw = String(text || "");
  const parts = raw
    .split(/[\n,;]+/g)
    .map((item) => item.trim())
    .filter((item) => item.length > 0);
  const seen = new Set<string>();
  const out: string[] = [];
  for (const p of parts) {
    const key = p.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(p);
  }
  return out;
}
