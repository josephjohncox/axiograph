export interface StatusContext {
  addStatusEl?: HTMLElement | null;
  proposalStatusEl?: HTMLElement | null;
  proposalOutputEl?: HTMLElement | null;
  reviewStatusEl?: HTMLElement | null;
  reviewValidationEl?: HTMLElement | null;
  reviewOverlayRawEl?: HTMLElement | null;
  reviewCommitOutputEl?: HTMLElement | null;
  reviewPromoteOutputEl?: HTMLElement | null;
  addOutputEl?: HTMLElement | null;
  addCommitOutputEl?: HTMLElement | null;
  addPromoteOutputEl?: HTMLElement | null;
  addPromoteStatusEl?: HTMLElement | null;
}

export function initStatus(ctx: StatusContext) {
  const {
    addStatusEl,
    proposalStatusEl,
    proposalOutputEl,
    reviewStatusEl,
    reviewValidationEl,
    reviewOverlayRawEl,
    reviewCommitOutputEl,
    reviewPromoteOutputEl,
    addOutputEl,
    addCommitOutputEl,
    addPromoteOutputEl,
    addPromoteStatusEl,
  } = ctx;
  // ---------------------------------------------------------------------------
  // Add data (WAL overlays)
  // ---------------------------------------------------------------------------

  function setAddStatus(msg: string) {
    if (!addStatusEl) return;
    addStatusEl.textContent = msg || "";
  }

  function setPredictiveProposalStatus(msg: string) {
    if (!proposalStatusEl) return;
    proposalStatusEl.textContent = msg || "";
  }

  function setPredictiveProposalOutput(obj: unknown) {
    if (!proposalOutputEl) return;
    if (obj === null || obj === undefined) proposalOutputEl.textContent = "";
    else
      proposalOutputEl.textContent =
        typeof obj === "string" ? obj : JSON.stringify(obj, null, 2);
  }

  function setReviewStatus(...content: (Node | string)[]) {
    reviewStatusEl?.replaceChildren(...content);
  }

  function setReviewValidation(obj: unknown) {
    if (!reviewValidationEl) return;
    if (obj === null || obj === undefined) reviewValidationEl.textContent = "";
    else
      reviewValidationEl.textContent =
        typeof obj === "string" ? obj : JSON.stringify(obj, null, 2);
  }

  function setReviewOverlayRaw(obj: unknown) {
    if (!reviewOverlayRawEl) return;
    if (obj === null || obj === undefined) reviewOverlayRawEl.textContent = "";
    else
      reviewOverlayRawEl.textContent =
        typeof obj === "string" ? obj : JSON.stringify(obj, null, 2);
  }

  function setReviewCommitOutput(obj: unknown) {
    if (!reviewCommitOutputEl) return;
    if (obj === null || obj === undefined)
      reviewCommitOutputEl.textContent = "";
    else
      reviewCommitOutputEl.textContent =
        typeof obj === "string" ? obj : JSON.stringify(obj, null, 2);
  }

  function setReviewPromoteOutput(obj: unknown) {
    if (!reviewPromoteOutputEl) return;
    if (obj === null || obj === undefined)
      reviewPromoteOutputEl.textContent = "";
    else
      reviewPromoteOutputEl.textContent =
        typeof obj === "string" ? obj : JSON.stringify(obj, null, 2);
  }

  function setAddOutput(obj: unknown) {
    if (!addOutputEl) return;
    if (obj === null || obj === undefined) addOutputEl.textContent = "";
    else addOutputEl.textContent = JSON.stringify(obj, null, 2);
  }

  function setAddCommitOutput(obj: unknown) {
    if (!addCommitOutputEl) return;
    if (obj === null || obj === undefined) addCommitOutputEl.textContent = "";
    else addCommitOutputEl.textContent = JSON.stringify(obj, null, 2);
  }

  function setAddPromoteStatus(msg: string) {
    if (!addPromoteStatusEl) return;
    addPromoteStatusEl.textContent = msg || "";
  }

  function setAddPromoteOutput(obj: unknown) {
    if (!addPromoteOutputEl) return;
    if (obj === null || obj === undefined) addPromoteOutputEl.textContent = "";
    else addPromoteOutputEl.textContent = JSON.stringify(obj, null, 2);
  }

  return {
    setAddStatus,
    setAddOutput,
    setAddCommitOutput,
    setAddPromoteStatus,
    setAddPromoteOutput,
    setPredictiveProposalStatus,
    setPredictiveProposalOutput,
    setReviewStatus,
    setReviewValidation,
    setReviewOverlayRaw,
    setReviewCommitOutput,
    setReviewPromoteOutput,
  };
}
