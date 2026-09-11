import { UNSUPPORTED } from "../server/read-only-client";
import type { DraftSelection } from "../core/draft-selection";
import type { DraftOverlay, VizUiState } from "../types";

type StatusSetter = (message: string) => void;

interface AddContext {
  ui: VizUiState;
  addGenerateBtn: HTMLButtonElement;
  addCommitBtn: HTMLButtonElement;
  addDraftAxiBtn: HTMLButtonElement;
  addPromoteAxiBtn: HTMLButtonElement;
  reviewCommitBtn: HTMLButtonElement;
  reviewDraftAxiBtn: HTMLButtonElement;
  reviewPromoteAxiBtn: HTMLButtonElement;
  currentDraftFiltered: () => DraftSelection | null;
  setReviewStatus: (...content: Array<Node | string>) => void;
  setReviewCommitOutput: (value: unknown) => void;
  setReviewPromoteOutput: (value: unknown) => void;
  setAddCommitOutput: (value: unknown) => void;
  setAddPromoteOutput: (value: unknown) => void;
  setAddPromoteStatus: StatusSetter;
  setAddStatus: StatusSetter;
  setAddOutput: (value: unknown) => void;
  setDraftOverlay: (overlay: DraftOverlay, options?: { notePrefix?: string }) => boolean;
  clearDraftOverlay: () => void;
  isServerMode: () => boolean;
}

export function initAddTab(ctx: AddContext) {
  const {
    ui,
    addGenerateBtn,
    addCommitBtn,
    addDraftAxiBtn,
    addPromoteAxiBtn,
    currentDraftFiltered,
    setReviewStatus,
    setAddCommitOutput,
    setAddPromoteOutput,
    setAddPromoteStatus,
    setAddStatus,
    setAddOutput,
    setDraftOverlay,
    clearDraftOverlay,
    isServerMode,
  } = ctx;

  const cliGuidance =
    "Review canonical .axi changes with `axiograph check --help` and `axiograph authoring --help`; use the documented AxiStore CLI workflow for accepted state.";
  const mutationButtons: Array<[HTMLButtonElement[], string]> = [
    [[addCommitBtn, ctx.reviewCommitBtn], "commit (CLI only)"],
    [[addPromoteAxiBtn, ctx.reviewPromoteAxiBtn], "promote (CLI only)"],
  ];
  for (const [buttons, label] of mutationButtons) {
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

  function selectedDraftForAction(setStatus: StatusSetter): DraftSelection | null | undefined {
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
      ui.draft.reviewActionStatus = message;
      setReviewStatus(message);
      return undefined;
    }
  }

  // The read-only API has no mutation routes. Neither a role string
  // nor a receipt in read-only status is HTTP mutation authority. All outcomes
  // here are blocked; there is deliberately no legacy admin POST fallback.
  async function explainUnavailableMutation(
    action: string,
    setStatus: StatusSetter,
  ): Promise<void> {
    const reason = isServerMode()
      ? "the read-only server exposes no commit/promote endpoint"
      : "offline visualization";
    const message = `${action} blocked: ${reason}. ${cliGuidance}`;
    setStatus(message);
    ui.draft.reviewActionStatus = message;
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
    if (
      ui.draft.kind === "loaded" &&
      ui.draft.overlay.validation?.ok === false
    ) {
      const message = `commit blocked: validation failed; fix proposals first. ${cliGuidance}`;
      setAddStatus(message);
      ui.draft.reviewActionStatus = message;
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
