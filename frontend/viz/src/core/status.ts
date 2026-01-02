// @ts-nocheck

export function initStatus(ctx) {
  const {
    addStatusEl,
    wmStatusEl,
    wmOutputEl,
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

function setAddStatus(msg) {
  if (!addStatusEl) return;
  addStatusEl.textContent = msg || "";
}

function setWorldModelStatus(msg) {
  if (!wmStatusEl) return;
  wmStatusEl.textContent = msg || "";
}

function setWorldModelOutput(obj) {
  if (!wmOutputEl) return;
  if (obj === null || obj === undefined) wmOutputEl.textContent = "";
  else wmOutputEl.textContent = (typeof obj === "string") ? obj : JSON.stringify(obj, null, 2);
}

function setReviewStatusHtml(html) {
  if (!reviewStatusEl) return;
  reviewStatusEl.innerHTML = html || "";
}

function setReviewValidation(obj) {
  if (!reviewValidationEl) return;
  if (obj === null || obj === undefined) reviewValidationEl.textContent = "";
  else reviewValidationEl.textContent = (typeof obj === "string") ? obj : JSON.stringify(obj, null, 2);
}

function setReviewOverlayRaw(obj) {
  if (!reviewOverlayRawEl) return;
  if (obj === null || obj === undefined) reviewOverlayRawEl.textContent = "";
  else reviewOverlayRawEl.textContent = (typeof obj === "string") ? obj : JSON.stringify(obj, null, 2);
}

function setReviewCommitOutput(obj) {
  if (!reviewCommitOutputEl) return;
  if (obj === null || obj === undefined) reviewCommitOutputEl.textContent = "";
  else reviewCommitOutputEl.textContent = (typeof obj === "string") ? obj : JSON.stringify(obj, null, 2);
}

function setReviewPromoteOutput(obj) {
  if (!reviewPromoteOutputEl) return;
  if (obj === null || obj === undefined) reviewPromoteOutputEl.textContent = "";
  else reviewPromoteOutputEl.textContent = (typeof obj === "string") ? obj : JSON.stringify(obj, null, 2);
}

function setAddOutput(obj) {
  if (!addOutputEl) return;
  if (obj === null || obj === undefined) addOutputEl.textContent = "";
  else addOutputEl.textContent = JSON.stringify(obj, null, 2);
}

function setAddCommitOutput(obj) {
  if (!addCommitOutputEl) return;
  if (obj === null || obj === undefined) addCommitOutputEl.textContent = "";
  else addCommitOutputEl.textContent = JSON.stringify(obj, null, 2);
}

function setAddPromoteStatus(msg) {
  if (!addPromoteStatusEl) return;
  addPromoteStatusEl.textContent = msg || "";
}

function setAddPromoteOutput(obj) {
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
    setWorldModelStatus,
    setWorldModelOutput,
    setReviewStatusHtml,
    setReviewValidation,
    setReviewOverlayRaw,
    setReviewCommitOutput,
    setReviewPromoteOutput,
  };
}
