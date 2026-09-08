// @ts-nocheck
import { UNSUPPORTED } from "../server/read-only-client";

export function initPredictiveProposalTab(ctx) {
  const { setPredictiveProposalStatus, setPredictiveProposalOutput } = ctx;
  for (const name of ["proposalGoalsEl", "proposalMaxNewEl", "proposalSeedEl", "proposalStepsEl", "proposalRolloutsEl", "proposalGuardrailProfileEl", "proposalGuardrailPlaneEl", "proposalIncludeGuardrailEl", "proposalTaskCostsEl", "proposalAutoCommitEl", "proposalCommitStepwiseEl", "proposalProposeBtn", "proposalPlanBtn"]) {
    const control = ctx[name];
    if (control) { control.disabled = true; control.title = UNSUPPORTED; }
  }
  for (const button of [ctx.proposalProposeBtn, ctx.proposalPlanBtn])
    button?.addEventListener("click", () => setPredictiveProposalStatus(UNSUPPORTED));
  setPredictiveProposalStatus(UNSUPPORTED);
function mergePredictiveProposalPlanProposals(report) {
  if (!report || !Array.isArray(report.steps)) return null;
  const proposals = [];
  for (const step of report.steps) {
    const p = step && step.proposals && Array.isArray(step.proposals.proposals)
      ? step.proposals.proposals
      : [];
    proposals.push(...p);
  }
  if (!proposals.length) return null;
  const traceId = report.trace_id || "proposal_rollout_plan";
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
