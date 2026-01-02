// @ts-nocheck

import { parseTextList } from "../util/text";

export function initWorldModelTab(ctx) {
  const {
    ui,
    wmGoalsEl,
    wmMaxNewEl,
    wmSeedEl,
    wmStepsEl,
    wmRolloutsEl,
    wmGuardrailProfileEl,
    wmGuardrailPlaneEl,
    wmIncludeGuardrailEl,
    wmTaskCostsEl,
    wmAutoCommitEl,
    wmCommitStepwiseEl,
    wmProposeBtn,
    wmPlanBtn,
    reviewAdminTokenEl,
    addAdminTokenEl,
    reviewMessageEl,
    addMessageEl,
    setDraftOverlay,
    setAddCommitOutput,
    setReviewCommitOutput,
    setWorldModelOutput,
    setWorldModelStatus,
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

function buildWorldModelBaseRequest() {
  const goals = parseTextList(wmGoalsEl && wmGoalsEl.value || "");
  const maxNewRaw = wmMaxNewEl ? String(wmMaxNewEl.value || "").trim() : "";
  const seedRaw = wmSeedEl ? String(wmSeedEl.value || "").trim() : "";
  const maxNew = maxNewRaw ? Number(maxNewRaw) : NaN;
  const seed = seedRaw ? Number(seedRaw) : NaN;
  const guardrailProfile = wmGuardrailProfileEl ? String(wmGuardrailProfileEl.value || "fast") : "fast";
  const guardrailPlane = wmGuardrailPlaneEl ? String(wmGuardrailPlaneEl.value || "both") : "both";
  const includeGuardrail = wmIncludeGuardrailEl ? !!wmIncludeGuardrailEl.checked : true;
  const taskCosts = parseTaskCosts(wmTaskCostsEl && wmTaskCostsEl.value || "");
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

function worldModelHeaders() {
  const headers = { "content-type": "application/json" };
  const token = (reviewAdminTokenEl && reviewAdminTokenEl.value || addAdminTokenEl && addAdminTokenEl.value || "").trim();
  if (token) headers["authorization"] = `Bearer ${token}`;
  return headers;
}

function mergeWorldModelPlanProposals(report) {
  if (!report || !Array.isArray(report.steps)) return null;
  const proposals = [];
  for (const step of report.steps) {
    const p = step && step.proposals && Array.isArray(step.proposals.proposals)
      ? step.proposals.proposals
      : [];
    proposals.push(...p);
  }
  if (!proposals.length) return null;
  const traceId = report.trace_id || "world_model_plan";
  const generatedAt = String(report.generated_at_unix_secs || Math.floor(Date.now() / 1000));
  return {
    version: 1,
    generated_at: generatedAt,
    source: { source_type: "world_model_plan", locator: traceId },
    schema_hint: null,
    proposals,
  };
}

async function runWorldModelPropose() {
  setWorldModelStatus("");
  setWorldModelOutput(null);
  setAddCommitOutput(null);
  setReviewCommitOutput(null);
  ui.reviewActionStatus = "";

  if (!isServerMode()) {
    setWorldModelStatus("requires server mode (`axiograph db serve`)");
    return;
  }

  const body = buildWorldModelBaseRequest();
  const steps = wmStepsEl ? Number(wmStepsEl.value || "") : NaN;
  if (Number.isFinite(steps)) body.horizon_steps = Math.max(1, Math.floor(steps));

  const autoCommit = wmAutoCommitEl ? !!wmAutoCommitEl.checked : false;
  if (autoCommit) {
    const token = (reviewAdminTokenEl && reviewAdminTokenEl.value || addAdminTokenEl && addAdminTokenEl.value || "").trim();
    if (!token) {
      setWorldModelStatus("auto-commit requires admin token");
      return;
    }
    body.auto_commit = true;
    const message = (reviewMessageEl && reviewMessageEl.value || addMessageEl && addMessageEl.value || "").trim();
    if (message) body.commit_message = message;
  }

  try {
    setWorldModelStatus("running…");
    const resp = await fetch("/world_model/propose", {
      method: "POST",
      headers: worldModelHeaders(),
      body: JSON.stringify(body),
    });
    const data = await resp.json();
    setWorldModelOutput(data);
    if (!resp.ok) {
      setWorldModelStatus(`error (${resp.status})`);
      return;
    }
    setWorldModelStatus("ok");
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
        summary: { source: "world_model_propose", trace_id: data.trace_id },
      };
      setDraftOverlay(overlay, { notePrefix: "generated from world model" });
    }
  } catch (e) {
    setWorldModelStatus("error");
    setWorldModelOutput(String(e));
  }
}

async function runWorldModelPlan() {
  setWorldModelStatus("");
  setWorldModelOutput(null);
  setAddCommitOutput(null);
  setReviewCommitOutput(null);
  ui.reviewActionStatus = "";

  if (!isServerMode()) {
    setWorldModelStatus("requires server mode (`axiograph db serve`)");
    return;
  }

  const body = buildWorldModelBaseRequest();
  const steps = wmStepsEl ? Number(wmStepsEl.value || "") : NaN;
  const rollouts = wmRolloutsEl ? Number(wmRolloutsEl.value || "") : NaN;
  if (Number.isFinite(steps)) body.horizon_steps = Math.max(1, Math.floor(steps));
  if (Number.isFinite(rollouts)) body.rollouts = Math.max(1, Math.floor(rollouts));

  const autoCommit = wmAutoCommitEl ? !!wmAutoCommitEl.checked : false;
  if (autoCommit) {
    const token = (reviewAdminTokenEl && reviewAdminTokenEl.value || addAdminTokenEl && addAdminTokenEl.value || "").trim();
    if (!token) {
      setWorldModelStatus("auto-commit requires admin token");
      return;
    }
    body.auto_commit = true;
    body.commit_stepwise = wmCommitStepwiseEl ? !!wmCommitStepwiseEl.checked : false;
    const message = (reviewMessageEl && reviewMessageEl.value || addMessageEl && addMessageEl.value || "").trim();
    if (message) body.commit_message = message;
  }

  try {
    setWorldModelStatus("running…");
    const resp = await fetch("/world_model/plan", {
      method: "POST",
      headers: worldModelHeaders(),
      body: JSON.stringify(body),
    });
    const data = await resp.json();
    setWorldModelOutput(data);
    if (!resp.ok) {
      setWorldModelStatus(`error (${resp.status})`);
      return;
    }
    setWorldModelStatus("ok");
    if (data && data.commit) {
      setReviewCommitOutput(data.commit);
    }
    const merged = data && data.report ? mergeWorldModelPlanProposals(data.report) : null;
    if (merged && merged.proposals && merged.proposals.length) {
      const overlay = {
        proposals_json: merged,
        chunks: [],
        summary: { source: "world_model_plan", trace_id: data.report && data.report.trace_id },
      };
      setDraftOverlay(overlay, { notePrefix: "generated from world model plan" });
    }
  } catch (e) {
    setWorldModelStatus("error");
    setWorldModelOutput(String(e));
  }
}

if (wmProposeBtn) wmProposeBtn.addEventListener("click", runWorldModelPropose);
if (wmPlanBtn) wmPlanBtn.addEventListener("click", runWorldModelPlan);


  return { setWorldModelStatus, setWorldModelOutput, mergeWorldModelPlanProposals };
}
