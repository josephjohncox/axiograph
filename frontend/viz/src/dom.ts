function requiredHtmlElement<K extends keyof HTMLElementTagNameMap>(
  id: string,
  tag: K,
): HTMLElementTagNameMap[K] {
  const element = document.querySelector<HTMLElementTagNameMap[K]>(`${tag}#${id}`);
  if (!element) throw new Error(`Missing required ${tag}#${id} visualization control`);
  return element;
}

function requiredSvg(id: string): SVGSVGElement {
  const element = document.querySelector<SVGSVGElement>(`svg#${id}`);
  if (!element) throw new Error(`Missing required svg#${id} visualization control`);
  return element;
}

export function getDom() {
  const html = requiredHtmlElement;
  return {
    nodesEl: html("nodes", "div"),
    detailEl: html("detail", "div"),
    searchEl: html("search", "input"),
    svg: requiredSvg("svg"),
    serverControlsEl: html("server_controls", "div"),

    show_plane_accepted: html("show_plane_accepted", "input"),
    show_plane_evidence: html("show_plane_evidence", "input"),
    show_plane_data: html("show_plane_data", "input"),
    runFilterEl: html("run_filter", "select"),
    runOnlyEl: html("show_only_run", "input"),
    runClearBtn: html("run_clear", "button"),
    layoutAlgoEl: html("layout_algo", "select"),
    layoutCenterEl: html("layout_center", "select"),
    layoutRefreshBtn: html("layout_refresh", "button"),
    layoutFitBtn: html("layout_fit", "button"),
    layoutResetViewBtn: html("layout_reset_view", "button"),
    labelDensityEl: html("label_density", "select"),
    navHelpEl: html("nav_help", "details"),
    contextFilterEl: html("context_filter", "select"),
    contextBadgeEl: html("context_badge", "span"),
    componentJumpBtn: html("jump_component", "button"),
    componentStatusEl: html("component_status", "span"),

    llmQuestionEl: html("llm_question", "input"),
    llmAutoCommitEl: html("llm_auto_commit", "input"),
    llmCertifyEl: html("llm_certify", "input"),
    llmVerifyEl: html("llm_verify", "input"),
    llmRequireVerifiedEl: html("llm_require_verified", "input"),
    llmAskBtn: html("llm_ask", "button"),
    llmToQueryBtn: html("llm_to_query", "button"),
    llmClearBtn: html("llm_clear", "button"),
    llmStatusEl: html("llm_status", "span"),
    llmChatEl: html("llm_chat", "div"),
    llmCitationsEl: html("llm_citations", "pre"),
    llmDebugEl: html("llm_debug", "pre"),

    proposalGoalsEl: html("proposal_goals", "textarea"),
    proposalMaxNewEl: html("proposal_max_new", "input"),
    proposalSeedEl: html("proposal_seed", "input"),
    proposalStepsEl: html("proposal_steps", "input"),
    proposalRolloutsEl: html("proposal_rollouts", "input"),
    proposalGuardrailProfileEl: html("proposal_guardrail_profile", "select"),
    proposalGuardrailPlaneEl: html("proposal_guardrail_plane", "select"),
    proposalIncludeGuardrailEl: html("proposal_include_guardrail", "input"),
    proposalTaskCostsEl: html("proposal_task_costs", "textarea"),
    proposalAutoCommitEl: html("proposal_auto_commit", "input"),
    proposalCommitStepwiseEl: html("proposal_commit_stepwise", "input"),
    proposalProposeBtn: html("proposal_propose", "button"),
    proposalPlanBtn: html("proposal_plan", "button"),
    proposalStatusEl: html("proposal_status", "span"),
    proposalOutputEl: html("proposal_output", "pre"),

    axqlQueryEl: html("axql_query", "textarea"),
    axqlRunBtn: html("axql_run", "button"),
    axqlCertBtn: html("axql_cert", "button"),
    axqlVerifyBtn: html("axql_verify", "button"),
    axqlStatusEl: html("axql_status", "span"),
    axqlOutputEl: html("axql_output", "pre"),
    certOutputEl: html("cert_output", "pre"),

    addRelTypeEl: html("add_rel_type", "input"),
    addSourceNameEl: html("add_source_name", "textarea"),
    addTargetNameEl: html("add_target_name", "textarea"),
    addPairingEl: html("add_pairing", "select"),
    addContextEl: html("add_context", "input"),
    addCtxFromFilterBtn: html("add_ctx_from_filter", "button"),
    addEvidenceTextEl: html("add_evidence_text", "textarea"),
    addConfidenceEl: html("add_confidence", "input"),
    addConfidenceValEl: html("add_confidence_val", "span"),
    addMessageEl: html("add_message", "input"),
    addAdminTokenEl: html("add_admin_token", "input"),
    addGenerateBtn: html("add_generate", "button"),
    addCommitBtn: html("add_commit", "button"),
    addStatusEl: html("add_status", "span"),
    addOutputEl: html("add_output", "pre"),
    addCommitOutputEl: html("add_commit_output", "pre"),
    addDraftAxiBtn: html("add_draft_axi", "button"),
    addPromoteAxiBtn: html("add_promote_axi", "button"),
    addPromoteStatusEl: html("add_promote_status", "span"),
    addAxiTextEl: html("add_axi_text", "textarea"),
    addPromoteOutputEl: html("add_promote_output", "pre"),

    reviewFilterEl: html("review_filter", "input"),
    reviewSelectAllBtn: html("review_select_all", "button"),
    reviewSelectNoneBtn: html("review_select_none", "button"),
    reviewClearBtn: html("review_clear", "button"),
    reviewStatusEl: html("review_status", "span"),
    reviewMessageEl: html("review_message", "input"),
    reviewAdminTokenEl: html("review_admin_token", "input"),
    reviewCommitBtn: html("review_commit", "button"),
    reviewDraftAxiBtn: html("review_draft_axi", "button"),
    reviewPromoteAxiBtn: html("review_promote_axi", "button"),
    reviewListEl: html("review_list", "div"),
    reviewValidationEl: html("review_validation", "pre"),
    reviewAxiTextEl: html("review_axi_text", "textarea"),
    reviewCommitOutputEl: html("review_commit_output", "pre"),
    reviewPromoteOutputEl: html("review_promote_output", "pre"),
    reviewOverlayRawEl: html("review_overlay_raw", "pre"),

    show_entity: html("show_entity", "input"),
    show_fact: html("show_fact", "input"),
    show_morphism: html("show_morphism", "input"),
    show_homotopy: html("show_homotopy", "input"),
    show_meta: html("show_meta", "input"),
    show_edge_relation: html("show_edge_relation", "input"),
    show_edge_equivalence: html("show_edge_equivalence", "input"),
    show_edge_meta: html("show_edge_meta", "input"),
    clearPathBtn: html("clear_path", "button"),
    certifyPathBtn: html("certify_path", "button"),
    verifyPathBtn: html("verify_path", "button"),
    pathStatusEl: html("path_status", "span"),
    minConfidenceEl: html("min_confidence", "input"),
    minConfidenceValEl: html("min_confidence_val", "span"),
    opacityByConfidenceEl: html("opacity_by_confidence", "input"),
  };
}

export type DomBindings = ReturnType<typeof getDom>;
