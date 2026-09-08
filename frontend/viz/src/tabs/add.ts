// @ts-nocheck

import { UNSUPPORTED } from "../server/read-only-client";

export function initAddTab(ctx) {
  const {
    ui,
    addRelTypeEl,
    addSourceNameEl,
    addTargetNameEl,
    addPairingEl,
    addContextEl,
    addEvidenceTextEl,
    addConfidenceEl,
    addGenerateBtn,
    addCommitBtn,
    addDraftAxiBtn,
    addPromoteAxiBtn,
    addAxiTextEl,
    reviewAxiTextEl,
    currentDraftFiltered,
    setReviewStatus,
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
    isServerMode,
    renderDraftOverlayReview,
  } = ctx;

  const cliGuidance =
    "Review canonical .axi changes with `axiograph check --help` and `axiograph authoring --help`; use the documented AxiStore CLI workflow for accepted state.";
  for (const [buttons, label] of [
    [[addCommitBtn, ctx.reviewCommitBtn], "commit (CLI only)"],
    [[addPromoteAxiBtn, ctx.reviewPromoteAxiBtn], "promote (CLI only)"],
  ]) {
    for (const button of buttons) {
      if (!button) continue;
      button.disabled = true;
      button.textContent = label;
      button.title = `Unavailable through the current read-only server. ${cliGuidance}`;
    }
  }

  for (const button of [addGenerateBtn, addDraftAxiBtn, ctx.reviewDraftAxiBtn]) {
    if (button) { button.disabled = true; button.title = UNSUPPORTED; }
  }
  setAddStatus(UNSUPPORTED);
  async function generateRelationProposals() { setAddStatus(UNSUPPORTED); }

  function selectedDraftForAction(setStatus) {
    try {
      if (typeof currentDraftFiltered !== "function") {
        throw new Error(
          "Draft selection unavailable; reload the visualization and try again.",
        );
      }
      return currentDraftFiltered();
    } catch (error) {
      const message = `Cannot use draft selection: ${String(error)}`;
      setStatus(message);
      ui.reviewActionStatus = message;
      setReviewStatus(message);
      return undefined;
    }
  }

  // The read-only API has no mutation routes. Neither a role string
  // nor a receipt in read-only status is HTTP mutation authority. All outcomes
  // here are blocked; there is deliberately no legacy admin POST fallback.
  async function explainUnavailableMutation(action, setStatus) {
    const reason = isServerMode()
      ? "the read-only server exposes no commit/promote endpoint"
      : "offline visualization";
    const message = `${action} blocked: ${reason}. ${cliGuidance}`;
    setStatus(message);
    ui.reviewActionStatus = message;
    setReviewStatus(message);
  }

  async function commitGeneratedOverlay() {
    const draft = selectedDraftForAction(setAddStatus);
    if (draft === undefined) return;
    if (!draft || !draft.proposals_json.proposals.length) {
      const message = `commit unavailable: no draft overlay selected. ${cliGuidance}`;
      setAddStatus(message);
      setReviewStatus(message);
      return;
    }
    if (ui.draftOverlay.validation?.ok === false) {
      const message = `commit blocked: validation failed; fix proposals first. ${cliGuidance}`;
      setAddStatus(message);
      ui.reviewActionStatus = message;
      setReviewStatus(message);
      return;
    }
    await explainUnavailableMutation("commit", setAddStatus);
  }

  async function draftAxiFromGeneratedOverlay() {
    if (selectedDraftForAction(setAddPromoteStatus) === undefined) return;
    setAddPromoteStatus(UNSUPPORTED);
    setReviewStatus(UNSUPPORTED);
  }

  async function promoteDraftAxiText() {
    await explainUnavailableMutation("promote", setAddPromoteStatus);
  }

  if (addGenerateBtn)
    addGenerateBtn.addEventListener("click", generateRelationProposals);
  if (addCommitBtn)
    addCommitBtn.addEventListener("click", commitGeneratedOverlay);
  if (addDraftAxiBtn)
    addDraftAxiBtn.addEventListener("click", draftAxiFromGeneratedOverlay);
  if (addPromoteAxiBtn)
    addPromoteAxiBtn.addEventListener("click", promoteDraftAxiText);

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
