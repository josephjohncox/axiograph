// @ts-nocheck

export function initQueryTab(ctx) {
  const {
    ui,
    axqlStatusEl,
    axqlOutputEl,
    certOutputEl,
    axqlQueryEl,
    axqlRunBtn,
    axqlCertBtn,
    axqlVerifyBtn,
    selectedContextFilter,
    clearHighlights,
    highlightFromQueryResponse,
    rerender,
  } = ctx;
function setAxqlStatus(text) {
  if (!axqlStatusEl) return;
  axqlStatusEl.textContent = text || "";
}

function setAxqlOutput(obj) {
  if (!axqlOutputEl) return;
  if (obj == null) {
    axqlOutputEl.textContent = "";
    return;
  }
  try {
    if (typeof obj === "string") axqlOutputEl.textContent = obj;
    else axqlOutputEl.textContent = JSON.stringify(obj, null, 2);
  } catch {
    axqlOutputEl.textContent = String(obj);
  }
}

function setCertOutput(obj) {
  if (!certOutputEl) return;
  if (obj == null) {
    certOutputEl.textContent = "";
    return;
  }
  try {
    if (typeof obj === "string") certOutputEl.textContent = obj;
    else certOutputEl.textContent = JSON.stringify(obj, null, 2);
  } catch {
    certOutputEl.textContent = String(obj);
  }
}

async function axqlRequest(certify, verify) {
  if (!axqlQueryEl) return;
  const q = String(axqlQueryEl.value || "").trim();
  if (!q) return;

  const proto = window.location && window.location.protocol;
  if (!(proto === "http:" || proto === "https:")) {
    setAxqlStatus("AxQL requires server mode (open via `axiograph db serve`).");
    return;
  }

  setAxqlStatus(verify ? "certifying+verifying…" : (certify ? "certifying…" : "running…"));
  setAxqlOutput("");
  setCertOutput("");

  const ctx = selectedContextFilter();
  const contexts = [];
  if (ctx !== "*" && ctx !== "__none__") contexts.push(ctx);

  try {
    const params = new URLSearchParams(window.location.search || "");
    const snapshot = params.get("snapshot");
    const body = {
      query: q,
      lang: "axql",
      show_elaboration: true,
      contexts,
      certify: !!certify,
      verify: !!verify,
      include_anchor: false,
    };
    if (snapshot) body.snapshot = snapshot;

    const resp = await fetch("/query", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body)
    });
    const data = await resp.json();
    if (!resp.ok) {
      setAxqlStatus(`error (${resp.status})`);
      setAxqlOutput(data);
      return;
    }
    setAxqlStatus("ok");
    setAxqlOutput(data);
    if (data && data.certificate) setCertOutput(data.certificate);
    clearHighlights();
    highlightFromQueryResponse(data);
    rerender();
  } catch (e) {
    setAxqlStatus("error");
    setAxqlOutput(String(e));
  }
}

if (axqlRunBtn) axqlRunBtn.addEventListener("click", () => axqlRequest(false, false));
if (axqlCertBtn) axqlCertBtn.addEventListener("click", () => axqlRequest(true, false));
if (axqlVerifyBtn) axqlVerifyBtn.addEventListener("click", () => axqlRequest(true, true));
if (axqlQueryEl) axqlQueryEl.addEventListener("keydown", (ev) => {
  if ((ev.ctrlKey || ev.metaKey) && ev.key === "Enter") {
    ev.preventDefault();
    axqlRequest(false, false);
  }
});


  return { setAxqlStatus, setAxqlOutput, setCertOutput };
}
