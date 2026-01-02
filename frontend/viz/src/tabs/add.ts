// @ts-nocheck

import { parseTextList } from "../util/text";

export function initAddTab(ctx) {
  const {
    ui,
    addRelTypeEl,
    addSourceNameEl,
    addTargetNameEl,
    addPairingEl,
    addContextEl,
    addCtxFromFilterBtn,
    addEvidenceTextEl,
    addConfidenceEl,
    addConfidenceValEl,
    addMessageEl,
    addAdminTokenEl,
    addGenerateBtn,
    addCommitBtn,
    addDraftAxiBtn,
    addPromoteAxiBtn,
    addAxiTextEl,
    addPromoteStatusEl,
    addStatusEl,
    reviewCommitOutputEl,
    reviewPromoteOutputEl,
    reviewMessageEl,
    reviewAdminTokenEl,
    addOutputEl,
    addCommitOutputEl,
    addPromoteOutputEl,
    currentContextNameFromFilter,
    setReviewCommitOutput,
    setReviewPromoteOutput,
    setAddCommitOutput,
    setAddPromoteOutput,
    setAddPromoteStatus,
    setAddStatus,
    setAddOutput,
    setDraftOverlay,
    clearDraftOverlay,
    setReviewActionStatus,
    isServerMode,
    updateAddConfidenceLabel,
    renderDraftOverlayReview,
  } = ctx;
async function generateRelationProposals() {
  setAddStatus("");
  setAddOutput(null);
  setAddCommitOutput(null);
  setReviewCommitOutput(null);
  setReviewPromoteOutput(null);
  ui.reviewActionStatus = "";
  clearDraftOverlay();

  if (!isServerMode()) {
    setAddStatus("requires server mode (`axiograph db serve`)");
    return;
  }

  const relType = (addRelTypeEl && addRelTypeEl.value || "").trim();
  const sourceNames = parseTextList(addSourceNameEl && addSourceNameEl.value || "");
  const targetNames = parseTextList(addTargetNameEl && addTargetNameEl.value || "");
  const evidenceText = (addEvidenceTextEl && addEvidenceTextEl.value || "").trim();

  if (!relType || !sourceNames.length || !targetNames.length) {
    setAddStatus("missing rel/source/target");
    return;
  }

  let context = (addContextEl && addContextEl.value || "").trim();
  if (!context) {
    const fromFilter = currentContextNameFromFilter();
    if (fromFilter) {
      context = fromFilter;
      if (addContextEl) addContextEl.value = fromFilter;
    }
  }

  const confidence = addConfidenceEl ? Number(addConfidenceEl.value || "0.9") : 0.9;

  try {
    setAddStatus("generating…");
    const isBatch = sourceNames.length > 1 || targetNames.length > 1;
    const pairing = (addPairingEl && addPairingEl.value || "cartesian").trim() || "cartesian";
    const path = isBatch ? "/proposals/relations" : "/proposals/relation";
    const req = isBatch
      ? {
          rel_type: relType,
          source_names: sourceNames,
          target_names: targetNames,
          pairing: pairing,
          context: context || null,
          confidence: confidence,
          evidence_text: evidenceText || null,
          evidence_locator: "viz_ui",
        }
      : {
          rel_type: relType,
          source_name: sourceNames[0],
          target_name: targetNames[0],
          context: context || null,
          confidence: confidence,
          evidence_text: evidenceText || null,
          evidence_locator: "viz_ui",
        };

    const resp = await fetch(path, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(req),
    });
    const data = await resp.json();
    if (!resp.ok) {
      setAddStatus(`error (${resp.status})`);
      setAddOutput(data);
      return;
    }
    setDraftOverlay(data, { notePrefix: "generated" });
    if (data && data.validation && data.validation.ok === false) {
      const te = (data.validation.axi_typecheck && Array.isArray(data.validation.axi_typecheck.errors))
        ? data.validation.axi_typecheck.errors.length : 0;
      const qe = (data.validation.quality_delta && data.validation.quality_delta.summary)
        ? Number(data.validation.quality_delta.summary.error_count || 0) : 0;
      ui.reviewActionStatus = "validation failed";
      renderDraftOverlayReview();
      setAddStatus(`validation failed (typecheck_errors=${te} quality_errors=${qe})`);
    } else if (data && data.validation && data.validation.ok === true) {
      ui.reviewActionStatus = "validated";
      renderDraftOverlayReview();
      setAddStatus("ok (validated)");
    } else {
      setAddStatus("ok");
    }
  } catch (e) {
    setAddStatus("error");
    setAddOutput(String(e));
  }
}

async function commitGeneratedOverlay() {
  setAddStatus("");
  setAddCommitOutput(null);
  setReviewCommitOutput(null);
  ui.reviewActionStatus = "";
  renderDraftOverlayReview();

  if (!isServerMode()) {
    setAddStatus("requires server mode (`axiograph db serve`)");
    return;
  }
  const draft = currentDraftFiltered();
  if (!draft || !draft.proposals_json || !Array.isArray(draft.proposals_json.proposals) || !draft.proposals_json.proposals.length) {
    setAddStatus("no draft overlay selected (generate proposals first)");
    return;
  }
  if (ui.draftOverlay && ui.draftOverlay.validation && ui.draftOverlay.validation.ok === false) {
    setAddStatus("refusing to commit: validation failed (fix proposals first)");
    ui.reviewActionStatus = "refusing to commit: invalid";
    renderDraftOverlayReview();
    return;
  }

  const message = (reviewMessageEl && reviewMessageEl.value || addMessageEl && addMessageEl.value || "").trim();

  let acceptedSnapshot = null;
  try {
    const statusResp = await fetch("/status", { cache: "no-store" });
    if (statusResp.ok) {
      const status = await statusResp.json();
      if (status && status.snapshot && status.snapshot.accepted_snapshot_id) {
        acceptedSnapshot = status.snapshot.accepted_snapshot_id;
      }
      if (status && status.role && String(status.role).toLowerCase() !== "master") {
        setAddStatus("commit requires master server (role != master)");
        ui.reviewActionStatus = "commit blocked (role != master)";
        renderDraftOverlayReview();
        return;
      }
    }
  } catch (_e) {}

  const req = {
    accepted_snapshot: acceptedSnapshot,
    chunks: Array.isArray(draft.chunks) ? draft.chunks : [],
    proposals: draft.proposals_json,
    message: message || null,
  };

  const headers = { "content-type": "application/json" };
  const token = (reviewAdminTokenEl && reviewAdminTokenEl.value || addAdminTokenEl && addAdminTokenEl.value || "").trim();
  if (token) headers["authorization"] = `Bearer ${token}`;

  try {
    setAddStatus("committing…");
    ui.reviewActionStatus = "committing…";
    renderDraftOverlayReview();
    const resp = await fetch("/admin/accept/pathdb-commit", {
      method: "POST",
      headers,
      body: JSON.stringify(req),
    });
    const data = await resp.json();
    setAddCommitOutput(data);
    setReviewCommitOutput(data);
    if (!resp.ok) {
      setAddStatus(`error (${resp.status})`);
      ui.reviewActionStatus = `commit error (${resp.status})`;
      renderDraftOverlayReview();
      return;
    }
    setAddStatus("ok");
    ui.reviewActionStatus = "committed";
    renderDraftOverlayReview();

    if (data && data.snapshot_id) {
      clearDraftOverlay();
      const p = new URLSearchParams(window.location.search || "");
      p.set("snapshot", data.snapshot_id);
      window.location.search = p.toString();
    }
  } catch (e) {
    setAddStatus("error");
    setAddCommitOutput(String(e));
    setReviewCommitOutput(String(e));
    ui.reviewActionStatus = "commit error";
    renderDraftOverlayReview();
  }
}

if (addGenerateBtn) addGenerateBtn.addEventListener("click", generateRelationProposals);
if (addCommitBtn) addCommitBtn.addEventListener("click", commitGeneratedOverlay);

async function draftAxiFromGeneratedOverlay() {
  setAddPromoteStatus("");
  setAddPromoteOutput(null);
  if (addAxiTextEl) addAxiTextEl.value = "";
  if (reviewAxiTextEl) reviewAxiTextEl.value = "";
  ui.reviewActionStatus = "";
  renderDraftOverlayReview();

  if (!isServerMode()) {
    setAddPromoteStatus("requires server mode (`axiograph db serve`)");
    return;
  }
  const draft = currentDraftFiltered();
  if (!draft || !draft.proposals_json || !Array.isArray(draft.proposals_json.proposals) || !draft.proposals_json.proposals.length) {
    setAddPromoteStatus("no draft overlay selected (generate proposals first)");
    ui.reviewActionStatus = "no draft overlay";
    renderDraftOverlayReview();
    return;
  }

  try {
    setAddPromoteStatus("drafting…");
    ui.reviewActionStatus = "drafting .axi…";
    renderDraftOverlayReview();
    const req = {
      proposals: draft.proposals_json,
      infer_constraints: true,
    };
    const resp = await fetch("/discover/draft-axi", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(req),
    });
    const data = await resp.json();
    setAddPromoteOutput(data);
    setReviewPromoteOutput(data);
    if (!resp.ok) {
      setAddPromoteStatus(`error (${resp.status})`);
      ui.reviewActionStatus = `draft error (${resp.status})`;
      renderDraftOverlayReview();
      return;
    }
    if (addAxiTextEl && data && typeof data.axi_text === "string") {
      addAxiTextEl.value = data.axi_text;
    }
    if (reviewAxiTextEl && data && typeof data.axi_text === "string") {
      reviewAxiTextEl.value = data.axi_text;
    }
    setAddPromoteStatus("ok");
    ui.reviewActionStatus = "drafted .axi";
    renderDraftOverlayReview();
  } catch (e) {
    setAddPromoteStatus("error");
    setAddPromoteOutput(String(e));
    setReviewPromoteOutput(String(e));
    ui.reviewActionStatus = "draft error";
    renderDraftOverlayReview();
  }
}

async function promoteDraftAxiText() {
  setAddPromoteStatus("");
  setAddPromoteOutput(null);
  setReviewPromoteOutput(null);
  ui.reviewActionStatus = "";
  renderDraftOverlayReview();

  if (!isServerMode()) {
    setAddPromoteStatus("requires server mode (`axiograph db serve`)");
    ui.reviewActionStatus = "requires server mode";
    renderDraftOverlayReview();
    return;
  }

  const axiText = (reviewAxiTextEl && String(reviewAxiTextEl.value || "").trim())
    ? String(reviewAxiTextEl.value || "")
    : (addAxiTextEl ? String(addAxiTextEl.value || "") : "");
  if (!String(axiText || "").trim()) {
    setAddPromoteStatus("draft .axi text is empty (click 'draft .axi' or paste)");
    ui.reviewActionStatus = "promote blocked: empty .axi";
    renderDraftOverlayReview();
    return;
  }

  const token = (reviewAdminTokenEl && reviewAdminTokenEl.value || addAdminTokenEl && addAdminTokenEl.value || "").trim();
  if (!token) {
    setAddPromoteStatus("promote requires admin token");
    ui.reviewActionStatus = "promote blocked: missing token";
    renderDraftOverlayReview();
    return;
  }

  // Verify role is master (same check as commit).
  try {
    const statusResp = await fetch("/status", { cache: "no-store" });
    if (statusResp.ok) {
      const status = await statusResp.json();
      if (status && status.role && String(status.role).toLowerCase() !== "master") {
        setAddPromoteStatus("promote requires master server (role != master)");
        ui.reviewActionStatus = "promote blocked (role != master)";
        renderDraftOverlayReview();
        return;
      }
    }
  } catch (_e) {}

  const message = (reviewMessageEl && reviewMessageEl.value || addMessageEl && addMessageEl.value || "").trim();
  const req = {
    axi_text: String(axiText || ""),
    message: message || null,
    quality: "fast",
  };

  const headers = { "content-type": "application/json", "authorization": `Bearer ${token}` };

  try {
    setAddPromoteStatus("promoting…");
    ui.reviewActionStatus = "promoting…";
    renderDraftOverlayReview();
    const resp = await fetch("/admin/accept/promote", {
      method: "POST",
      headers,
      body: JSON.stringify(req),
    });
    const data = await resp.json();
    setAddPromoteOutput(data);
    setReviewPromoteOutput(data);
    if (!resp.ok) {
      setAddPromoteStatus(`error (${resp.status})`);
      ui.reviewActionStatus = `promote error (${resp.status})`;
      renderDraftOverlayReview();
      return;
    }
    setAddPromoteStatus("ok");
    ui.reviewActionStatus = "promoted";
    renderDraftOverlayReview();
    // NOTE: If the server is currently serving the accepted plane (accepted/head),
    // it will auto-reload. If serving pathdb/head, you'll still need to build/commit
    // a new PathDB snapshot derived from the new accepted snapshot.
  } catch (e) {
    setAddPromoteStatus("error");
    setAddPromoteOutput(String(e));
    setReviewPromoteOutput(String(e));
    ui.reviewActionStatus = "promote error";
    renderDraftOverlayReview();
  }
}

if (addDraftAxiBtn) addDraftAxiBtn.addEventListener("click", draftAxiFromGeneratedOverlay);
if (addPromoteAxiBtn) addPromoteAxiBtn.addEventListener("click", promoteDraftAxiText);


  return {
    setAddStatus,
    setAddOutput,
    setAddCommitOutput,
    setAddPromoteOutput,
    setAddPromoteStatus,
    setDraftOverlay,
    clearDraftOverlay,
    commitGeneratedOverlay,
    draftAxiFromGeneratedOverlay,
    promoteDraftAxiText,
  };
}
