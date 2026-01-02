// @ts-nocheck

export function initDraft(ctx) {
  const {
    ui,
    reviewFilterEl,
    reviewSelectAllBtn,
    reviewSelectNoneBtn,
    reviewClearBtn,
    reviewMessageEl,
    reviewAdminTokenEl,
    reviewCommitBtn,
    reviewDraftAxiBtn,
    reviewPromoteAxiBtn,
    reviewListEl,
    reviewValidationEl,
    reviewAxiTextEl,
    reviewCommitOutputEl,
    reviewPromoteOutputEl,
    reviewOverlayRawEl,
    addAxiTextEl,
    addMessageEl,
    addAdminTokenEl,
    setReviewStatusHtml,
    setReviewValidation,
    setReviewOverlayRaw,
    setReviewCommitOutput,
    setReviewPromoteOutput,
    setAddPromoteOutput,
    setAddCommitOutput,
    setAddPromoteStatus,
    setAddStatus,
    isServerMode,
    commitGeneratedOverlay,
    draftAxiFromGeneratedOverlay,
    promoteDraftAxiText,
  } = ctx;
function draftOverlayStorageKey() {
  // Persist draft overlay per *accepted snapshot* when available (stable across WAL
  // commits), falling back to the current snapshot param.
  const host = (window.location && window.location.host) ? window.location.host : "offline";
  let key = "";
  try {
    const accepted = (localStorage.getItem("axiograph_server_accepted_snapshot_id") || "").trim();
    if (accepted) key = accepted;
  } catch (_e) {}
  if (!key) {
    const params = new URLSearchParams(window.location.search || "");
    key = (params.get("snapshot") || "").trim();
  }
  if (!key) key = "default";
  return `axiograph_draft_overlay_v1:${host}:${key}`;
}

let draftOverlayKey = draftOverlayStorageKey();

function getDraftOverlayKey() {
  return draftOverlayKey;
}

function setDraftOverlayKey(next) {
  draftOverlayKey = next;
}

function loadDraftOverlayForKey(key) {
  try {
    const raw = localStorage.getItem(key) || "";
    if (!raw.trim()) return null;
    const v = JSON.parse(raw);
    if (!v || typeof v !== "object") return null;
    if (!v.proposals_json) return null;
    return v;
  } catch (_e) {
    return null;
  }
}

function saveDraftOverlay() {
  try {
    if (!ui.draftOverlay) {
      localStorage.removeItem(draftOverlayKey);
      return;
    }
    localStorage.setItem(draftOverlayKey, JSON.stringify(ui.draftOverlay));
  } catch (_e) {}
}

function clearDraftOverlay() {
  ui.draftOverlay = null;
  ui.draftSelected = new Set();
  saveDraftOverlay();
  setReviewCommitOutput(null);
  setReviewPromoteOutput(null);
  renderDraftOverlayReview();
}

function setDraftOverlay(overlay, opts) {
  const r = overlay || null;
  if (!r || !r.proposals_json) return false;

  ui.draftOverlay = r;
  const props = (r.proposals_json && Array.isArray(r.proposals_json.proposals))
    ? r.proposals_json.proposals : [];
  ui.draftSelected = new Set(props.map(p => String(p && p.proposal_id || "")).filter(Boolean));

  saveDraftOverlay();
  renderDraftOverlayReview();

  // Keep the raw JSON visible in the "Add" tab (debug), but make the Review tab
  // the primary workflow surface.
  setAddOutput(r);
  setAddCommitOutput(null);
  setReviewCommitOutput(null);
  setReviewPromoteOutput(null);
  setAddPromoteOutput(null);
  setAddPromoteStatus("");
  if (addAxiTextEl) addAxiTextEl.value = "";
  if (reviewAxiTextEl) reviewAxiTextEl.value = "";

  const notePrefix = (opts && opts.notePrefix) ? String(opts.notePrefix) : "draft overlay ready";
  const ok = r.validation && r.validation.ok === true;
  const bad = r.validation && r.validation.ok === false;
  if (bad) setAddStatus(`${notePrefix} (validation failed; review before commit)`);
  else if (ok) setAddStatus(`${notePrefix} (validated; review before commit)`);
  else setAddStatus(`${notePrefix} (review before commit)`);

  setActiveTab("review");
  return true;
}

function currentDraftFiltered() {
  const r = ui.draftOverlay;
  if (!r || !r.proposals_json || !Array.isArray(r.proposals_json.proposals)) return null;
  const selected = ui.draftSelected || new Set();
  const file = JSON.parse(JSON.stringify(r.proposals_json));
  file.proposals = file.proposals.filter(p => selected.has(String(p && p.proposal_id || "")));

  // Filter chunks to those referenced by selected proposals (keep all if none).
  let chunks = Array.isArray(r.chunks) ? r.chunks : [];
  const needed = new Set();
  for (const p of file.proposals) {
    const evs = Array.isArray(p && p.evidence) ? p.evidence : [];
    for (const ev of evs) {
      const cid = ev && ev.chunk_id ? String(ev.chunk_id) : "";
      if (cid) needed.add(cid);
    }
  }
  if (needed.size && chunks.length) {
    chunks = chunks.filter(c => c && needed.has(String(c.chunk_id || "")));
  }

  return { proposals_json: file, chunks };
}

function renderDraftOverlayReview() {
  if (!reviewListEl) return;

  reviewListEl.innerHTML = "";

  const r = ui.draftOverlay;
  if (!r || !r.proposals_json) {
    setReviewStatusHtml("<span class=\"muted\">(no draft overlay)</span>");
    setReviewOverlayRaw("");
    setReviewValidation("");
    if (reviewAxiTextEl) reviewAxiTextEl.value = "";
    reviewListEl.innerHTML = `<div class="proprow"><div class="main"><div class="muted">No draft overlay. Generate one in <strong>add</strong> or via <strong>llm</strong>, then review/commit here.</div></div></div>`;
    return;
  }

  const props = (r.proposals_json && Array.isArray(r.proposals_json.proposals))
    ? r.proposals_json.proposals : [];
  const selected = ui.draftSelected || new Set();

  const ok = r.validation && r.validation.ok === true;
  const bad = r.validation && r.validation.ok === false;
  const chip = ok
    ? `<span class="chip ok">validated</span>`
    : bad
      ? `<span class="chip bad">invalid</span>`
      : `<span class="chip">unvalidated</span>`;

  function escapeHtml(text) {
    return String(text || "")
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#39;");
  }

  function setSelectionStatus() {
    const action = ui.reviewActionStatus ? ` <span class="muted">— ${escapeHtml(ui.reviewActionStatus)}</span>` : "";
    setReviewStatusHtml(`${selected.size}/${props.length} selected ${chip}${action}`);
  }

  setSelectionStatus();

  setReviewOverlayRaw(r);
  setReviewValidation(r.validation || "");

  const filter = (reviewFilterEl && reviewFilterEl.value) ? String(reviewFilterEl.value).trim().toLowerCase() : "";

  function proposalLine(p) {
    const kind = String(p && p.kind || "");
    const conf = (p && p.confidence != null) ? Number(p.confidence) : null;
    const confText = (conf != null && Number.isFinite(conf)) ? ` conf=${conf.toFixed(2)}` : "";
    if (kind.toLowerCase() === "entity") {
      const ty = p.entity_type || "Entity";
      const name = p.name || "";
      return `Entity ${ty} "${name}"${confText}`;
    }
    if (kind.toLowerCase() === "relation") {
      const rt = p.rel_type || "Relation";
      const src = p.source || "?";
      const dst = p.target || "?";
      return `Relation ${rt}(${src} -> ${dst})${confText}`;
    }
    return `${kind || "Proposal"}${confText}`;
  }

  function proposalMatches(p) {
    if (!filter) return true;
    const parts = [];
    for (const k of ["kind","proposal_id","schema_hint","entity_type","name","entity_id","rel_type","relation_id","source","target"]) {
      if (p && p[k] != null) parts.push(String(p[k]));
    }
    const evs = Array.isArray(p && p.evidence) ? p.evidence : [];
    for (const ev of evs.slice(0, 4)) {
      if (ev && ev.chunk_id) parts.push(String(ev.chunk_id));
      if (ev && ev.locator) parts.push(String(ev.locator));
    }
    return parts.join(" ").toLowerCase().includes(filter);
  }

  const filtered = props.filter(proposalMatches);
  if (!filtered.length) {
    reviewListEl.innerHTML = `<div class="proprow"><div class="main"><div class="muted">No proposals match the current filter.</div></div></div>`;
    return;
  }

  const maxRows = 250;
  const toShow = filtered.slice(0, maxRows);

  for (const p of toShow) {
    const pid = String(p && p.proposal_id || "");
    const row = document.createElement("div");
    row.className = "proprow";

    const cb = document.createElement("input");
    cb.type = "checkbox";
    cb.checked = pid ? selected.has(pid) : false;
    cb.addEventListener("change", () => {
      if (!pid) return;
      if (cb.checked) selected.add(pid);
      else selected.delete(pid);
      ui.draftSelected = selected;
      setSelectionStatus();
    });

    const main = document.createElement("div");
    main.className = "main";

    const line = document.createElement("div");
    line.className = "line";
    line.textContent = proposalLine(p);

    const sub = document.createElement("div");
    sub.className = "sub";
    const evs = Array.isArray(p && p.evidence) ? p.evidence : [];
    const evCount = evs.length;
    sub.textContent = `proposal_id=${pid || "?"}${evCount ? ` evidence=${evCount}` : ""}`;

    const det = document.createElement("details");
    const sum = document.createElement("summary");
    sum.className = "muted";
    sum.textContent = "details";
    const pre = document.createElement("pre");
    pre.style.whiteSpace = "pre-wrap";
    pre.style.maxHeight = "220px";
    pre.style.overflow = "auto";
    pre.style.margin = "8px 0 0 0";
    pre.textContent = JSON.stringify(p, null, 2);
    det.appendChild(sum);
    det.appendChild(pre);

    // Evidence quick-open buttons.
    if (evs.length) {
      const citeBox = document.createElement("div");
      citeBox.className = "muted";
      citeBox.style.marginTop = "6px";
      citeBox.textContent = "evidence:";
      for (const ev of evs.slice(0, 6)) {
        const cid = ev && ev.chunk_id ? String(ev.chunk_id) : "";
        if (!cid) continue;
        const openBtn = document.createElement("button");
        openBtn.type = "button";
        openBtn.className = "btn";
        openBtn.style.padding = "2px 8px";
        openBtn.style.marginLeft = "6px";
        openBtn.textContent = cid;
        openBtn.title = "Open DocChunk";
        openBtn.addEventListener("click", () => openDocChunk(cid));
        citeBox.appendChild(openBtn);
      }
      main.appendChild(citeBox);
    }

    main.appendChild(line);
    main.appendChild(sub);
    main.appendChild(det);
    row.appendChild(cb);
    row.appendChild(main);
    reviewListEl.appendChild(row);
  }

  if (filtered.length > maxRows) {
    const more = document.createElement("div");
    more.className = "proprow";
    more.innerHTML = `<div class="main"><div class="muted">Showing ${maxRows} of ${filtered.length} matching proposals. Narrow the filter to review more precisely.</div></div>`;
    reviewListEl.appendChild(more);
  }
}

function setReviewActionStatus(msg) {
  ui.reviewActionStatus = msg || "";
  renderDraftOverlayReview();
}

// Restore any persisted draft overlay (durable across reload).
ui.draftOverlay = loadDraftOverlayForKey(draftOverlayKey);
if (ui.draftOverlay && ui.draftOverlay.proposals_json && Array.isArray(ui.draftOverlay.proposals_json.proposals)) {
  ui.draftSelected = new Set(ui.draftOverlay.proposals_json.proposals.map(p => String(p && p.proposal_id || "")).filter(Boolean));
}
renderDraftOverlayReview();

function loadAdminToken() {
  try {
    const v = localStorage.getItem("axiograph_admin_token") || "";
    if (addAdminTokenEl && !addAdminTokenEl.value) addAdminTokenEl.value = v;
    if (reviewAdminTokenEl && !reviewAdminTokenEl.value) reviewAdminTokenEl.value = v;
  } catch (_e) {}
}

function saveAdminToken() {
  try {
    const src = (reviewAdminTokenEl && reviewAdminTokenEl.value)
      ? reviewAdminTokenEl
      : addAdminTokenEl;
    if (!src) return;
    const v = (src.value || "").trim();
    localStorage.setItem("axiograph_admin_token", v);
    if (addAdminTokenEl && addAdminTokenEl.value !== v) addAdminTokenEl.value = v;
    if (reviewAdminTokenEl && reviewAdminTokenEl.value !== v) reviewAdminTokenEl.value = v;
  } catch (_e) {}
}

loadAdminToken();
if (addAdminTokenEl) addAdminTokenEl.addEventListener("change", saveAdminToken);
if (reviewAdminTokenEl) reviewAdminTokenEl.addEventListener("change", saveAdminToken);

function loadCommitMessage() {
  try {
    const v = localStorage.getItem("axiograph_commit_message") || "";
    if (addMessageEl && !addMessageEl.value) addMessageEl.value = v;
    if (reviewMessageEl && !reviewMessageEl.value) reviewMessageEl.value = v;
  } catch (_e) {}
}

function saveCommitMessage() {
  try {
    const src = (reviewMessageEl && reviewMessageEl.value)
      ? reviewMessageEl
      : addMessageEl;
    if (!src) return;
    const v = (src.value || "").trim();
    localStorage.setItem("axiograph_commit_message", v);
    if (addMessageEl && addMessageEl.value !== v) addMessageEl.value = v;
    if (reviewMessageEl && reviewMessageEl.value !== v) reviewMessageEl.value = v;
  } catch (_e) {}
}

loadCommitMessage();
if (addMessageEl) addMessageEl.addEventListener("change", saveCommitMessage);
if (reviewMessageEl) reviewMessageEl.addEventListener("change", saveCommitMessage);

if (reviewFilterEl) reviewFilterEl.addEventListener("input", renderDraftOverlayReview);
if (reviewSelectAllBtn) reviewSelectAllBtn.addEventListener("click", () => {
  if (!ui.draftOverlay || !ui.draftOverlay.proposals_json || !Array.isArray(ui.draftOverlay.proposals_json.proposals)) return;
  ui.draftSelected = new Set(ui.draftOverlay.proposals_json.proposals.map(p => String(p && p.proposal_id || "")).filter(Boolean));
  renderDraftOverlayReview();
});
if (reviewSelectNoneBtn) reviewSelectNoneBtn.addEventListener("click", () => {
  ui.draftSelected = new Set();
  renderDraftOverlayReview();
});
if (reviewClearBtn) reviewClearBtn.addEventListener("click", clearDraftOverlay);
if (reviewCommitBtn) reviewCommitBtn.addEventListener("click", commitGeneratedOverlay);
if (reviewDraftAxiBtn) reviewDraftAxiBtn.addEventListener("click", draftAxiFromGeneratedOverlay);
if (reviewPromoteAxiBtn) reviewPromoteAxiBtn.addEventListener("click", promoteDraftAxiText);


  return {
    draftOverlayStorageKey,
    loadDraftOverlayForKey,
    saveDraftOverlay,
    getDraftOverlayKey,
    setDraftOverlayKey,
    clearDraftOverlay,
    setDraftOverlay,
    setReviewActionStatus,
    renderDraftOverlayReview,
  };
}
