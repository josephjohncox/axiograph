// @ts-nocheck

import { UNSUPPORTED } from "../server/read-only-client";
import { element, muted } from "../render/dom";
import { selectDraft } from "./draft-selection";

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
    setReviewStatus,
    setAddOutput,
    setActiveTab,
    setReviewValidation,
    setReviewOverlayRaw,
    setReviewCommitOutput,
    setReviewPromoteOutput,
    setAddPromoteOutput,
    setAddCommitOutput,
    setAddPromoteStatus,
    setAddStatus,
    isServerMode,
  } = ctx;
  function draftOverlayStorageKey() {
    // Retain existing local draft keys for inspection only. Legacy cached
    // snapshot strings are not identity evidence and are never migrated from
    // server status or used in requests.
    const host =
      window.location && window.location.host
        ? window.location.host
        : "offline";
    let key = "";
    try {
      const accepted = (
        localStorage.getItem("axiograph_server_accepted_snapshot_id") || ""
      ).trim();
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
    const props =
      r.proposals_json && Array.isArray(r.proposals_json.proposals)
        ? r.proposals_json.proposals
        : [];
    ui.draftSelected = new Set(
      props.map((p) => String((p && p.proposal_id) || "")).filter(Boolean),
    );

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

    const notePrefix =
      opts && opts.notePrefix ? String(opts.notePrefix) : "draft overlay ready";
    const ok = r.validation && r.validation.ok === true;
    const bad = r.validation && r.validation.ok === false;
    if (bad)
      setAddStatus(`${notePrefix} (validation failed; review before commit)`);
    else if (ok)
      setAddStatus(`${notePrefix} (validated; review before commit)`);
    else setAddStatus(`${notePrefix} (review before commit)`);

    setActiveTab("review");
    return true;
  }

  // Installed on appCtx with the draft API before initLlmTab captures it.
  function prefillAddFromToolLoop(outcome) {
    if (!outcome) return false;
    function useOverlay(overlay, notePrefix) {
      try {
        if (typeof ctx.setDraftOverlay !== "function") {
          throw new Error(
            "Draft review unavailable; reload the visualization and try again.",
          );
        }
        if (!ctx.setDraftOverlay(overlay, { notePrefix })) {
          throw new Error("No usable draft overlay; generate proposals again.");
        }
        return true;
      } catch (error) {
        const message = `Cannot open draft review: ${String(error)}`;
        setAddStatus(message);
        setReviewStatus(message);
        return false;
      }
    }
    const artifact = outcome.artifacts?.generated_overlay;
    if (artifact?.proposals_json)
      return useOverlay(artifact, "generated from LLM");
    // Preserve the existing transcript fallback and latest-result precedence.
    const steps = Array.isArray(outcome.steps) ? outcome.steps : [];
    for (let i = steps.length - 1; i >= 0; i--) {
      const step = steps[i];
      if (
        !step ||
        ![
          "propose_relation_proposals",
          "propose_relations_proposals",
          "predictive_proposal",
          "proposal_rollout_plan",
        ].includes(step.tool)
      )
        continue;
      if (step.result?.proposals_json)
        return useOverlay(step.result, "generated from LLM (fallback)");
    }
    return false;
  }

  function currentDraftFiltered() {
    return selectDraft(ui.draftOverlay, ui.draftSelected || new Set());
  }

  function renderDraftOverlayReview() {
    if (!reviewListEl) return;

    reviewListEl.replaceChildren();

    const r = ui.draftOverlay;
    if (!r || !r.proposals_json) {
      setReviewStatus(
        element("span", { className: "muted" }, "(no draft overlay)"),
      );
      setReviewOverlayRaw("");
      setReviewValidation("");
      if (reviewAxiTextEl) reviewAxiTextEl.value = "";
      reviewListEl.append(
        element(
          "div",
          { className: "proprow" },
          element(
            "div",
            { className: "main" },
            element(
              "div",
              { className: "muted" },
              "No local draft overlay. Inspect existing drafts here; generation and accepted changes require the CLI, not this read-only server.",
            ),
          ),
        ),
      );
      return;
    }

    const props =
      r.proposals_json && Array.isArray(r.proposals_json.proposals)
        ? r.proposals_json.proposals
        : [];
    const selected = ui.draftSelected || new Set();

    const ok = r.validation && r.validation.ok === true;
    const bad = r.validation && r.validation.ok === false;
    function setSelectionStatus() {
      const chip = element(
        "span",
        { className: ok ? "chip ok" : bad ? "chip bad" : "chip" },
        ok ? "validated" : bad ? "invalid" : "unvalidated",
      );
      const action = ui.reviewActionStatus
        ? element("span", { className: "muted" }, `— ${ui.reviewActionStatus}`)
        : "";
      setReviewStatus(
        `${selected.size}/${props.length} selected `,
        chip,
        " ",
        action,
      );
    }

    setSelectionStatus();

    setReviewOverlayRaw(r);
    setReviewValidation(r.validation || "");

    const filter =
      reviewFilterEl && reviewFilterEl.value
        ? String(reviewFilterEl.value).trim().toLowerCase()
        : "";

    function proposalLine(p) {
      const kind = String((p && p.kind) || "");
      const conf = p && p.confidence != null ? Number(p.confidence) : null;
      const confText =
        conf != null && Number.isFinite(conf) ? ` conf=${conf.toFixed(2)}` : "";
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
      for (const k of [
        "kind",
        "proposal_id",
        "schema_hint",
        "entity_type",
        "name",
        "entity_id",
        "rel_type",
        "relation_id",
        "source",
        "target",
      ]) {
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
      reviewListEl.append(
        element(
          "div",
          { className: "proprow" },
          element(
            "div",
            { className: "main" },
            muted("No proposals match the current filter."),
          ),
        ),
      );
      return;
    }

    const maxRows = 250;
    const toShow = filtered.slice(0, maxRows);

    for (const p of toShow) {
      const pid = String((p && p.proposal_id) || "");
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
          openBtn.title = UNSUPPORTED;
          openBtn.disabled = true;
          openBtn.addEventListener("click", () => {
            if (typeof ctx.openDocChunk !== "function") {
              setReviewStatus(
                "Evidence lookup unavailable; reload the visualization and try again.",
              );
              return;
            }
            return ctx.openDocChunk(cid);
          });
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
      more.append(
        element(
          "div",
          { className: "main" },
          muted(
            `Showing ${maxRows} of ${filtered.length} matching proposals. Narrow the filter to review more precisely.`,
          ),
        ),
      );
      reviewListEl.appendChild(more);
    }
  }

  function setReviewActionStatus(msg) {
    ui.reviewActionStatus = msg || "";
    renderDraftOverlayReview();
  }

  // Restore any persisted draft overlay (durable across reload).
  ui.draftOverlay = loadDraftOverlayForKey(draftOverlayKey);
  if (
    ui.draftOverlay &&
    ui.draftOverlay.proposals_json &&
    Array.isArray(ui.draftOverlay.proposals_json.proposals)
  ) {
    ui.draftSelected = new Set(
      ui.draftOverlay.proposals_json.proposals
        .map((p) => String((p && p.proposal_id) || ""))
        .filter(Boolean),
    );
  }
  renderDraftOverlayReview();

  for (const control of [addAdminTokenEl, reviewAdminTokenEl]) {
    if (control) { control.disabled = true; control.value = ""; control.title = UNSUPPORTED; }
  }

  function loadCommitMessage() {
    try {
      const v = localStorage.getItem("axiograph_commit_message") || "";
      if (addMessageEl && !addMessageEl.value) addMessageEl.value = v;
      if (reviewMessageEl && !reviewMessageEl.value) reviewMessageEl.value = v;
    } catch (_e) {}
  }

  function saveCommitMessage() {
    try {
      const src =
        reviewMessageEl && reviewMessageEl.value
          ? reviewMessageEl
          : addMessageEl;
      if (!src) return;
      const v = (src.value || "").trim();
      localStorage.setItem("axiograph_commit_message", v);
      if (addMessageEl && addMessageEl.value !== v) addMessageEl.value = v;
      if (reviewMessageEl && reviewMessageEl.value !== v)
        reviewMessageEl.value = v;
    } catch (_e) {}
  }

  loadCommitMessage();
  if (addMessageEl) addMessageEl.addEventListener("change", saveCommitMessage);
  if (reviewMessageEl)
    reviewMessageEl.addEventListener("change", saveCommitMessage);

  if (reviewFilterEl)
    reviewFilterEl.addEventListener("input", renderDraftOverlayReview);
  if (reviewSelectAllBtn)
    reviewSelectAllBtn.addEventListener("click", () => {
      if (
        !ui.draftOverlay ||
        !ui.draftOverlay.proposals_json ||
        !Array.isArray(ui.draftOverlay.proposals_json.proposals)
      )
        return;
      ui.draftSelected = new Set(
        ui.draftOverlay.proposals_json.proposals
          .map((p) => String((p && p.proposal_id) || ""))
          .filter(Boolean),
      );
      renderDraftOverlayReview();
    });
  if (reviewSelectNoneBtn)
    reviewSelectNoneBtn.addEventListener("click", () => {
      ui.draftSelected = new Set();
      renderDraftOverlayReview();
    });
  if (reviewClearBtn)
    reviewClearBtn.addEventListener("click", clearDraftOverlay);
  // appCtx is populated by initAddTab after initDraft; resolve on user action.
  function reviewAction(name) {
    return () => {
      if (typeof ctx[name] !== "function") {
        const message =
          "Review action unavailable; reload the visualization and try again.";
        ui.reviewActionStatus = message;
        setReviewStatus(message);
        return;
      }
      return ctx[name]();
    };
  }
  if (reviewCommitBtn)
    reviewCommitBtn.addEventListener(
      "click",
      reviewAction("commitGeneratedOverlay"),
    );
  if (reviewDraftAxiBtn)
    reviewDraftAxiBtn.addEventListener(
      "click",
      reviewAction("draftAxiFromGeneratedOverlay"),
    );
  if (reviewPromoteAxiBtn)
    reviewPromoteAxiBtn.addEventListener(
      "click",
      reviewAction("promoteDraftAxiText"),
    );

  return {
    prefillAddFromToolLoop,
    currentDraftFiltered,
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
