import { UNSUPPORTED } from "../server/read-only-client";
import { isRecord } from "../types";

type PredictiveControl =
  | HTMLInputElement
  | HTMLTextAreaElement
  | HTMLSelectElement
  | HTMLButtonElement
  | undefined;

interface PredictiveContext {
  setPredictiveProposalStatus: (message: string) => void;
  setPredictiveProposalOutput: (value: unknown) => void;
  proposalGoalsEl?: HTMLTextAreaElement;
  proposalMaxNewEl?: HTMLInputElement;
  proposalSeedEl?: HTMLInputElement;
  proposalStepsEl?: HTMLInputElement;
  proposalRolloutsEl?: HTMLInputElement;
  proposalGuardrailProfileEl?: HTMLSelectElement;
  proposalGuardrailPlaneEl?: HTMLSelectElement;
  proposalIncludeGuardrailEl?: HTMLInputElement;
  proposalTaskCostsEl?: HTMLTextAreaElement;
  proposalAutoCommitEl?: HTMLInputElement;
  proposalCommitStepwiseEl?: HTMLInputElement;
  proposalProposeBtn?: HTMLButtonElement;
  proposalPlanBtn?: HTMLButtonElement;
}

export function initPredictiveProposalTab(ctx: PredictiveContext) {
  const { setPredictiveProposalStatus, setPredictiveProposalOutput } = ctx;
  const controls: PredictiveControl[] = [
    ctx.proposalGoalsEl,
    ctx.proposalMaxNewEl,
    ctx.proposalSeedEl,
    ctx.proposalStepsEl,
    ctx.proposalRolloutsEl,
    ctx.proposalGuardrailProfileEl,
    ctx.proposalGuardrailPlaneEl,
    ctx.proposalIncludeGuardrailEl,
    ctx.proposalTaskCostsEl,
    ctx.proposalAutoCommitEl,
    ctx.proposalCommitStepwiseEl,
    ctx.proposalProposeBtn,
    ctx.proposalPlanBtn,
  ];
  for (const control of controls) {
    if (!control) continue;
    control.disabled = true;
    control.title = UNSUPPORTED;
  }
  for (const button of [ctx.proposalProposeBtn, ctx.proposalPlanBtn])
    button?.addEventListener("click", () => setPredictiveProposalStatus(UNSUPPORTED));
  setPredictiveProposalStatus(UNSUPPORTED);
function mergePredictiveProposalPlanProposals(report: unknown) {
  if (!isRecord(report) || !Array.isArray(report.steps)) return null;
  const proposals: unknown[] = [];
  for (const step of report.steps) {
    const p = isRecord(step) && isRecord(step.proposals) && Array.isArray(step.proposals.proposals)
      ? step.proposals.proposals
      : [];
    proposals.push(...p);
  }
  if (!proposals.length) return null;
  const traceId = String(report.trace_id || "proposal_rollout_plan");
  const generatedAt = String(report.generated_at_unix_secs || Math.floor(Date.now() / 1000));
  return {
    version: 1,
    generated_at: generatedAt,
    source: { source_type: "proposal_rollout_plan", locator: traceId },
    schema_hint: null,
    proposals,
  };
}

  return { setPredictiveProposalStatus, setPredictiveProposalOutput, mergePredictiveProposalPlanProposals };
}
