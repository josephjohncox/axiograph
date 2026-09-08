import { LatestQuery, QUERY_EXAMPLE, ReadOnlyClient, UNSUPPORTED } from "../server/read-only-client";

export interface QueryContext {
  axqlStatusEl?: HTMLElement | null;
  axqlOutputEl?: HTMLElement | null;
  certOutputEl?: HTMLElement | null;
  axqlQueryEl?: HTMLTextAreaElement | null;
  axqlRunBtn?: HTMLButtonElement | null;
  axqlCertBtn?: HTMLButtonElement | null;
  axqlVerifyBtn?: HTMLButtonElement | null;
}
export function initQueryTab(ctx: QueryContext) {
  const { axqlStatusEl, axqlOutputEl, certOutputEl, axqlQueryEl, axqlRunBtn, axqlCertBtn, axqlVerifyBtn } = ctx;
  let client: ReadOnlyClient | null = null;
  const latest = new LatestQuery();
  function setAxqlStatus(text: string) { if (axqlStatusEl) axqlStatusEl.textContent = text; }
  function setAxqlOutput(value: unknown) { if (axqlOutputEl) axqlOutputEl.textContent = typeof value === "string" ? value : JSON.stringify(value, null, 2); }
  function setCertOutput(value: unknown) { if (certOutputEl) certOutputEl.textContent = typeof value === "string" ? value : JSON.stringify(value, null, 2); }
  function setQueryClient(next: ReadOnlyClient | null) { latest.invalidate(); client = next; if (axqlRunBtn) axqlRunBtn.disabled = !next; }
  setQueryClient(null);
  setAxqlStatus("Finite queries unavailable until supported same-origin capabilities/status are established.");
  setCertOutput("Runtime trust only. No certificate is emitted; certifiable means eligibility, not Lean verification. Exact accepted .axi bytes and the approved checker CLI are required.");
  if (axqlQueryEl) { axqlQueryEl.value = JSON.stringify(QUERY_EXAMPLE, null, 2); axqlQueryEl.placeholder = "QueryIrV1 JSON (Rust compiles and typechecks)"; }
  for (const button of [axqlCertBtn, axqlVerifyBtn]) if (button) { button.disabled = true; button.title = UNSUPPORTED; }
  async function run() {
    if (!client || !axqlQueryEl) { setAxqlStatus("Query unavailable: supported capabilities/status not established."); return; }
    setAxqlStatus("Running against the connected server image. Previous output, if any, is from the previous successful query.");
    await latest.run(client, axqlQueryEl.value, value => {
      setAxqlOutput(value);
      setAxqlStatus(`${value.result.truncated ? "TRUNCATED" : "Returned"}: ${value.result.rows.length} rows; ${String(value.trust.trust_class)}. IDs are server-image-local. Graph highlighting unavailable (no image binding). No certificate or ontology-closure claim.`);
    }, error => setAxqlStatus(`Query failed: ${String(error)}. Previous successful output retained; not a result for this query.`));
  }
  axqlRunBtn?.addEventListener("click", run);
  axqlQueryEl?.addEventListener("input", () => { latest.invalidate(); setAxqlStatus("Editor changed; any previous output is stale. Run to evaluate this input."); });
  axqlQueryEl?.addEventListener("keydown", event => { if ((event.ctrlKey || event.metaKey) && event.key === "Enter") { event.preventDefault(); void run(); } });
  return { setAxqlStatus, setAxqlOutput, setCertOutput, setQueryClient };
}
