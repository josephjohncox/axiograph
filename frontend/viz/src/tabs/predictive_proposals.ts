// @ts-nocheck

import { parseTextList } from "../util/text";

export function initPredictiveProposalTab(ctx) {
  const {
    ui,
    proposalGoalsEl,
    proposalMaxNewEl,
    proposalSeedEl,
    proposalStepsEl,
    proposalRolloutsEl,
    proposalGuardrailProfileEl,
    proposalGuardrailPlaneEl,
    proposalIncludeGuardrailEl,
    proposalTaskCostsEl,
    proposalAutoCommitEl,
    proposalCommitStepwiseEl,
    proposalProposeBtn,
    proposalPlanBtn,
    reviewAdminTokenEl,
    addAdminTokenEl,
    reviewMessageEl,
    addMessageEl,
    setDraftOverlay,
    setAddCommitOutput,
    setReviewCommitOutput,
    setPredictiveProposalOutput,
    setPredictiveProposalStatus,
    clearDraftOverlay,
    isServerMode,
  } = ctx;
function parseTaskCosts(text) {
  const raw = String(text || "").trim();
  if (!raw) return [];
  const lines = raw.split(/\n+/g).map(s => s.trim()).filter(Boolean);
  const out = [];
  for (const line of lines) {
    const eqIdx = line.indexOf("=");
    if (eqIdx <= 0) continue;
    const name = line.slice(0, eqIdx).trim();
    const rest = line.slice(eqIdx + 1).trim();
    if (!name || !rest) continue;
    const parts = rest.split(":").map(s => s.trim()).filter(Boolean);
    const value = Number(parts[0]);
    if (!Number.isFinite(value)) continue;
    const weight = parts.length > 1 ? Number(parts[1]) : 1.0;
    const unit = parts.length > 2 ? parts.slice(2).join(":") : null;
    out.push({
      name,
      value,
      weight: Number.isFinite(weight) ? weight : 1.0,
      unit: unit || null,
    });
  }
  return out;
}

function buildPredictiveProposalBaseRequest() {
  const goals = parseTextList(proposalGoalsEl && proposalGoalsEl.value || "");
  const maxNewRaw = proposalMaxNewEl ? String(proposalMaxNewEl.value || "").trim() : "";
  const seedRaw = proposalSeedEl ? String(proposalSeedEl.value || "").trim() : "";
  const maxNew = maxNewRaw ? Number(maxNewRaw) : NaN;
  const seed = seedRaw ? Number(seedRaw) : NaN;
  const guardrailProfile = proposalGuardrailProfileEl ? String(proposalGuardrailProfileEl.value || "fast") : "fast";
  const guardrailPlane = proposalGuardrailPlaneEl ? String(proposalGuardrailPlaneEl.value || "both") : "both";
  const includeGuardrail = proposalIncludeGuardrailEl ? !!proposalIncludeGuardrailEl.checked : true;
  const taskCosts = parseTaskCosts(proposalTaskCostsEl && proposalTaskCostsEl.value || "");
  const body = {
    goals,
    guardrail_profile: guardrailProfile,
    guardrail_plane: guardrailPlane,
    include_guardrail: includeGuardrail,
    task_costs: taskCosts,
  };
  if (Number.isFinite(maxNew)) body.max_new_proposals = Math.max(0, Math.floor(maxNew));
  if (Number.isFinite(seed)) body.seed = Math.max(0, Math.floor(seed));
  return body;
}

function predictiveProposalHeaders() {
  const headers = { "content-type": "application/json" };
  const token = (reviewAdminTokenEl && reviewAdminTokenEl.value || addAdminTokenEl && addAdminTokenEl.value || "").trim();
  if (token) headers["authorization"] = `Bearer ${token}`;
  return headers;
}

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

async function runPredictiveProposalPropose() {
  setPredictiveProposalStatus("");
  setPredictiveProposalOutput(null);
  setAddCommitOutput(null);
  setReviewCommitOutput(null);
  ui.reviewActionStatus = "";

  if (!isServerMode()) {
    setPredictiveProposalStatus("requires server mode (`axiograph db serve`)");
    return;
  }

  const body = buildPredictiveProposalBaseRequest();
  const steps = proposalStepsEl ? Number(proposalStepsEl.value || "") : NaN;
  if (Number.isFinite(steps)) body.horizon_steps = Math.max(1, Math.floor(steps));

  const autoCommit = proposalAutoCommitEl ? !!proposalAutoCommitEl.checked : false;
  if (autoCommit) {
    const token = (reviewAdminTokenEl && reviewAdminTokenEl.value || addAdminTokenEl && addAdminTokenEl.value || "").trim();
    if (!token) {
      setPredictiveProposalStatus("auto-commit requires admin token");
      return;
    }
    body.auto_commit = true;
    const message = (reviewMessageEl && reviewMessageEl.value || addMessageEl && addMessageEl.value || "").trim();
    if (message) body.commit_message = message;
  }

  try {
    setPredictiveProposalStatus("running…");
    const resp = await fetch("/evidence/proposals/predict", {
      method: "POST",
      headers: predictiveProposalHeaders(),
      body: JSON.stringify(body),
    });
    const data = await resp.json();
    setPredictiveProposalOutput(data);
    if (!resp.ok) {
      setPredictiveProposalStatus(`error (${resp.status})`);
      return;
    }
    setPredictiveProposalStatus("ok");
    if (data && data.commit) {
      setReviewCommitOutput(data.commit);
    }
    if (data && data.commit_steps) {
      setReviewCommitOutput(data.commit_steps);
    }
    if (data && data.commit_steps) {
      setReviewCommitOutput(data.commit_steps);
    }
    if (data && data.proposals) {
      const overlay = {
        proposals_json: data.proposals,
        chunks: [],
        summary: { source: "predictive_proposal", trace_id: data.trace_id },
      };
      setDraftOverlay(overlay, { notePrefix: "generated from predictive proposals" });
    }
  } catch (e) {
    setPredictiveProposalStatus("error");
    setPredictiveProposalOutput(String(e));
  }
}

async function runPredictiveProposalPlan() {
  setPredictiveProposalStatus("");
  setPredictiveProposalOutput(null);
  setAddCommitOutput(null);
  setReviewCommitOutput(null);
  ui.reviewActionStatus = "";

  if (!isServerMode()) {
    setPredictiveProposalStatus("requires server mode (`axiograph db serve`)");
    return;
  }

  const body = buildPredictiveProposalBaseRequest();
  const steps = proposalStepsEl ? Number(proposalStepsEl.value || "") : NaN;
  const rollouts = proposalRolloutsEl ? Number(proposalRolloutsEl.value || "") : NaN;
  if (Number.isFinite(steps)) body.horizon_steps = Math.max(1, Math.floor(steps));
  if (Number.isFinite(rollouts)) body.rollouts = Math.max(1, Math.floor(rollouts));

  const autoCommit = proposalAutoCommitEl ? !!proposalAutoCommitEl.checked : false;
  if (autoCommit) {
    const token = (reviewAdminTokenEl && reviewAdminTokenEl.value || addAdminTokenEl && addAdminTokenEl.value || "").trim();
    if (!token) {
      setPredictiveProposalStatus("auto-commit requires admin token");
      return;
    }
    body.auto_commit = true;
    body.commit_stepwise = proposalCommitStepwiseEl ? !!proposalCommitStepwiseEl.checked : false;
    const message = (reviewMessageEl && reviewMessageEl.value || addMessageEl && addMessageEl.value || "").trim();
    if (message) body.commit_message = message;
  }

  try {
    setPredictiveProposalStatus("running…");
    const resp = await fetch("/planning/proposal-rollout", {
      method: "POST",
      headers: predictiveProposalHeaders(),
      body: JSON.stringify(body),
    });
    const data = await resp.json();
    setPredictiveProposalOutput(data);
    if (!resp.ok) {
      setPredictiveProposalStatus(`error (${resp.status})`);
      return;
    }
    setPredictiveProposalStatus("ok");
    if (data && data.commit) {
      setReviewCommitOutput(data.commit);
    }
    const merged = data && data.report ? mergePredictiveProposalPlanProposals(data.report) : null;
    if (merged && merged.proposals && merged.proposals.length) {
      const overlay = {
        proposals_json: merged,
        chunks: [],
        summary: { source: "proposal_rollout_plan", trace_id: data.report && data.report.trace_id },
      };
      setDraftOverlay(overlay, { notePrefix: "generated from predictive proposals plan" });
    }
  } catch (e) {
    setPredictiveProposalStatus("error");
    setPredictiveProposalOutput(String(e));
  }
}

if (proposalProposeBtn) proposalProposeBtn.addEventListener("click", runPredictiveProposalPropose);
if (proposalPlanBtn) proposalPlanBtn.addEventListener("click", runPredictiveProposalPlan);


  return { setPredictiveProposalStatus, setPredictiveProposalOutput, mergePredictiveProposalPlanProposals };
}
