//! End-to-end regulated-shipment usefulness fixture.
//!
//! The crate deliberately uses the production canonical compiler, AxiStore
//! promotion/merge APIs, SQLite materializer, and verified PathDB hydration.
//! It does not define ontology semantics. Rust remains an untrusted producer;
//! the supplied verification-receipt bytes must come from the separately built
//! `Axiograph.VerifyMain` executable when this is run through the documented
//! workflow.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use axiograph_kernel::{
    AnswerIdV2, CanonicalCompiler, CanonicalModuleSource, CertificateIdV2, CommitIdV2,
    CompiledKernelSnapshot, KernelCompilationRequest, ObjectBlobIdV2, QueryIdV2, RepositoryIdV2,
    RevisionDigestV2, SchemaGeneratorKindIr, ScopeAxisIr, SnapshotIdV2, TypeExprIr,
};
use axiograph_llm_sync::grounding::accepted_grounding_context;
use axiograph_llm_sync::AcceptedGroundingPlaneV1;
use axiograph_pathdb::materialization::load_verified_pathdb;
use axiograph_pathdb::{CertificateV3, RuntimeTheoryCheckReportV1, RuntimeTheoryCheckStatusV1};
use axiograph_store::*;
use serde::{Deserialize, Serialize};

pub const REGULATED_SHIPMENT_USEFULNESS_REPORT_VERSION: &str =
    "regulated_shipment_usefulness_report_v4";
const MODULE_NAME: &str = "RegulatedShipment";

#[derive(Debug, Clone)]
pub struct ApprovedQueryVerifierConfig {
    pub verifier_bin: PathBuf,
    pub approved_checker_sha256: String,
    pub approved_checker_build_id: String,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct RegulatedShipmentWorkflowInputs {
    pub baseline_axi: PathBuf,
    pub candidate_axi: PathBuf,
    pub baseline_authoring_report: PathBuf,
    pub candidate_authoring_report: PathBuf,
    pub baseline_theory_report: PathBuf,
    pub candidate_theory_report: PathBuf,
    pub baseline_verification_receipt: PathBuf,
    pub candidate_verification_receipt: PathBuf,
    pub baseline_query_verification: PathBuf,
    pub candidate_query_verification: PathBuf,
    pub query_verifier: ApprovedQueryVerifierConfig,
    pub store_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegulatedShipmentCategoryEvidence {
    pub object_types: usize,
    pub relation_objects: usize,
    pub role_projections: usize,
    pub explicit_generators: usize,
    pub path_equations: usize,
    pub formal_groupoid_equations: usize,
    pub rewrite_rules: usize,
    pub indexed_role_types: usize,
    pub refined_role_types: usize,
    pub finite_reachability_entries: usize,
    pub role_indexed_witnesses: usize,
    pub finite_refinement_predicates: usize,
    pub context_witnesses: usize,
    pub world_witnesses: usize,
    pub identity_scope_transports: usize,
    pub non_identity_scope_transports_certified: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegulatedShipmentQueryEvidence {
    pub claim_kind: String,
    pub decision: String,
    pub revision_digest_v2: String,
    pub prepared_query_digest_v1: String,
    pub answer_digest_v1: String,
    pub certificate_digest_v2: String,
    pub verified_rows: usize,
    pub path_witnesses: usize,
    pub receipt_bound_to_exact_answer: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegulatedShipmentMergeEvidence {
    pub operation: String,
    pub ordered_parent_count: usize,
    pub typed_decisions: usize,
    pub source_ref: String,
    pub materialized_commit_id: String,
    pub finite_contract: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegulatedShipmentPersistenceEvidence {
    pub store_generation_after_restart: u64,
    pub accepted_commit_id_after_restart: String,
    pub materialization_id: String,
    pub exact_image_digest: String,
    pub entity_rows: usize,
    pub relation_fact_rows: usize,
    pub hydrated_entities: usize,
    pub hydrated_relations: usize,
    pub shipment_rx_1007_present_after_restart: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegulatedShipmentAcceptedGroundingEvidence {
    pub plane: String,
    pub accepted_snapshot_id: String,
    pub materialization_id: String,
    pub query_digest: String,
    pub selection_digest: String,
    pub stable_ids: Vec<String>,
    pub truncated: bool,
    pub truncation_reasons: Vec<String>,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegulatedShipmentUsefulnessReport {
    pub version: String,
    pub scenario: String,
    pub repository_id: String,
    pub baseline_snapshot_id: String,
    pub accepted_snapshot_id: String,
    pub kernel_ir_digest: String,
    pub canonical_revision_digest: String,
    pub category: RegulatedShipmentCategoryEvidence,
    pub finite_query: RegulatedShipmentQueryEvidence,
    pub merge: RegulatedShipmentMergeEvidence,
    pub persistence: RegulatedShipmentPersistenceEvidence,
    pub accepted_grounding: RegulatedShipmentAcceptedGroundingEvidence,
    pub trusted_receipt_inputs: Vec<String>,
    pub checked_runtime_scope: Vec<String>,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone)]
struct EvidenceBytes {
    authoring: Vec<u8>,
    theory: Vec<u8>,
    verification: Vec<u8>,
    query_verification: Vec<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredFiniteQueryScopeV1 {
    revision_digest_v2: axiograph_kernel::RevisionDigestV2,
    accepted_snapshot_id: axiograph_kernel::SnapshotIdV2,
    kernel_ir_digest: axiograph_kernel::ObjectBlobIdV2,
    prepared_query_digest_v1: axiograph_kernel::QueryIdV2,
    answer_digest_v1: axiograph_kernel::AnswerIdV2,
    certificate_digest_v2: axiograph_kernel::CertificateIdV2,
    claim_kind: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredFiniteQueryCoverageV1 {
    selected_row_count: usize,
    row_witness_count: usize,
    runtime_truncated: bool,
    query_shape_certifiable: bool,
    certificate_emitted: bool,
    accepted_receipt_bound_to_exact_answer: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StoredVerifierReceiptV2 {
    version: String,
    nonce: String,
    checker_sha256: String,
    checker_build_id: String,
    revision_digest_v2: RevisionDigestV2,
    certificate_digest_v2: CertificateIdV2,
    prepared_query_digest_v1: QueryIdV2,
    answer_digest_v1: AnswerIdV2,
    certificate_kind: String,
    claim_kind: String,
    decision: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct QueryVerifierRequestV2<'a> {
    version: &'static str,
    nonce: &'a str,
    checker_sha256: &'a str,
    module_axi: &'a str,
    certificate_json: &'a str,
    expected_prepared_query_digest: &'a QueryIdV2,
    expected_answer_digest: &'a AnswerIdV2,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredFiniteQueryVerificationReportV1 {
    version: String,
    decision: String,
    scope: StoredFiniteQueryScopeV1,
    coverage: StoredFiniteQueryCoverageV1,
    finite_theory_gate: axiograph_kernel::FiniteTheoryGateReceiptIr,
    prepared_query: serde_json::Value,
    certificate: serde_json::Value,
    certificate_text: String,
    verifier_receipt_v2: StoredVerifierReceiptV2,
    verified_rows: Vec<serde_json::Value>,
    residual_obligations: Vec<String>,
    non_claims: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct StoredRuntimeTheoryModuleReportV1 {
    version: String,
    module_digest: String,
    summary: axiograph_tooling_overlays::RuntimeTheoryCheckSummaryV1,
    reports: Vec<RuntimeTheoryCheckReportV1>,
    blocking_errors: usize,
    trust_boundary: String,
    #[serde(default)]
    non_claims: Vec<axiograph_tooling_overlays::RuntimeTheoryNonClaimSummaryV1>,
    #[serde(default)]
    notes: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredAuthoringModuleAnchorV1 {
    module_name: String,
    module_id: String,
    revision_digest: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredAuthoringSourceAnchorV1 {
    workspace_relative_path: String,
    root_module: String,
    repository_id: String,
    compiled_snapshot_id: String,
    kernel_ir_digest: String,
    exact_root_axi_digest: String,
    ordered_module_closure: Vec<StoredAuthoringModuleAnchorV1>,
    runtime_ir_ref_count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredAuthoringValidationV1 {
    canonical_axi_valid: bool,
    compiled_kernel_ir_valid: bool,
    finite_category_fragment_valid: bool,
    finite_theory_gate: Option<axiograph_kernel::FiniteTheoryGateReceiptIr>,
    runtime_theory_gate: String,
    runtime_theory: Option<StoredRuntimeTheoryModuleReportV1>,
    scope: String,
    non_claims: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredAuthoringCompetencyV1 {
    #[serde(default)]
    questions: Vec<serde_json::Value>,
    evaluation: Option<serde_json::Value>,
    #[serde(default)]
    unresolved_question_names: Vec<String>,
    promotion_gate: String,
    #[serde(default)]
    non_claims: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredAuthoringPromotionGateV1 {
    gate: String,
    decision: String,
    detail: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredAuthoringPromotionV1 {
    candidate_reviewable: bool,
    protected_main_eligible: bool,
    gates: Vec<StoredAuthoringPromotionGateV1>,
    blockers: Vec<String>,
    required_write_authority: String,
    scope: String,
    non_claims: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct StoredAuthoringWorkspaceReportV1 {
    version: String,
    operation: String,
    #[serde(default)]
    workspace_root: String,
    ok: bool,
    source: Option<StoredAuthoringSourceAnchorV1>,
    #[serde(default)]
    diagnostics: Vec<serde_json::Value>,
    validation: StoredAuthoringValidationV1,
    #[serde(default)]
    typed_holes: serde_json::Value,
    #[serde(default)]
    dependent_refinements: Vec<serde_json::Value>,
    #[serde(default)]
    repairs: Vec<serde_json::Value>,
    competency_questions: Option<StoredAuthoringCompetencyV1>,
    #[serde(default)]
    prepared_query: Option<serde_json::Value>,
    #[serde(default)]
    query_explanation: Option<serde_json::Value>,
    #[serde(default)]
    applied_query_repair: Option<serde_json::Value>,
    #[serde(default)]
    checked_olog: Option<serde_json::Value>,
    #[serde(default)]
    applied_olog_repair: Option<serde_json::Value>,
    #[serde(default)]
    evolution_previews: Vec<serde_json::Value>,
    promotion: StoredAuthoringPromotionV1,
    #[serde(default)]
    stable_runtime_refs: Vec<serde_json::Value>,
    #[serde(default)]
    next_actions: Vec<String>,
    #[serde(default)]
    trust: serde_json::Value,
}

#[derive(Debug)]
struct ScenarioPlan {
    promotion: PromotionPlan,
    compiled: CompiledKernelSnapshot,
}

fn read(path: &Path, label: &str) -> Result<Vec<u8>> {
    const MAX_EVIDENCE_BYTES: usize = 16 * 1024 * 1024;
    axiograph_security::read_file_bounded(path, MAX_EVIDENCE_BYTES, label)
}

fn validate_runtime_theory_report(
    report: &StoredRuntimeTheoryModuleReportV1,
    expected_revision: &axiograph_kernel::RevisionDigestV2,
    label: &str,
) -> Result<()> {
    if report.version != "runtime_theory_check_module_report_v1"
        || report.module_digest != expected_revision.as_str()
        || report.summary.module_digest != expected_revision.as_str()
        || report.summary.theory_count != report.reports.len()
        || report.blocking_errors != 0
        || report.trust_boundary.trim().is_empty()
        || report.non_claims.is_empty()
    {
        return Err(anyhow!(
            "{label} runtime-theory module report has invalid version, anchor, counts, or trust boundary"
        ));
    }
    let blockers = report.summary.gate_blockers();
    if !blockers.is_empty() {
        return Err(anyhow!(
            "{label} runtime-theory report is not fully checked: {}",
            blockers.join("; ")
        ));
    }
    let checked = report
        .reports
        .iter()
        .map(|theory| theory.checked_obligations)
        .sum::<usize>();
    let review_only = report
        .reports
        .iter()
        .map(|theory| theory.review_only_obligations)
        .sum::<usize>();
    let residual = report
        .reports
        .iter()
        .map(|theory| theory.residual_obligations)
        .sum::<usize>();
    let blocked = report
        .reports
        .iter()
        .map(|theory| theory.blocked_obligations)
        .sum::<usize>();
    let excluded = report
        .reports
        .iter()
        .map(|theory| theory.excluded_by_evidence)
        .sum::<usize>();
    if checked != report.summary.checked_obligations
        || review_only != report.summary.review_only_obligations
        || residual != report.summary.residual_obligations
        || blocked != report.summary.blocked_obligations
        || excluded != report.summary.excluded_by_evidence
        || report
            .summary
            .admissibility_trace
            .admissibility_scan_complete_steps
            != report.reports.len()
    {
        return Err(anyhow!(
            "{label} runtime-theory summary does not equal its typed report judgments"
        ));
    }
    for theory in &report.reports {
        if theory.version != "runtime_theory_check_report_v1"
            || theory.total_obligations != theory.judgments.len()
            || theory.checked_obligations != theory.total_obligations
            || theory.review_only_obligations != 0
            || theory.residual_obligations != 0
            || theory.blocked_obligations != 0
            || theory.excluded_by_evidence != 0
            || theory.admissibility_scan.blocked_obligations != 0
            || !theory.admissibility_scan.residual_obligations.is_empty()
            || theory.judgments.iter().any(|judgment| {
                judgment.status != RuntimeTheoryCheckStatusV1::Checked
                    || !judgment.admissible
                    || !judgment.residual_obligations.is_empty()
            })
        {
            return Err(anyhow!(
                "{label} runtime-theory report contains a non-checked judgment"
            ));
        }
    }
    Ok(())
}

fn validate_authoring_report(
    report: &StoredAuthoringWorkspaceReportV1,
    expected_revision: &axiograph_kernel::RevisionDigestV2,
    label: &str,
) -> Result<()> {
    let source = report
        .source
        .as_ref()
        .ok_or_else(|| anyhow!("{label} authoring report omits its source anchor"))?;
    if report.version != "authoring_workspace_report_v1"
        || report.operation != "promotion_review"
        || !report.ok
        || source.root_module != MODULE_NAME
        || source.exact_root_axi_digest != expected_revision.as_str()
        || source.ordered_module_closure.len() != 1
        || source.ordered_module_closure[0].module_name != MODULE_NAME
        || source.ordered_module_closure[0].revision_digest != expected_revision.as_str()
        || source.ordered_module_closure[0].module_id.trim().is_empty()
        || source.repository_id.trim().is_empty()
        || source.compiled_snapshot_id.trim().is_empty()
        || source.kernel_ir_digest.trim().is_empty()
        || source.workspace_relative_path.trim().is_empty()
        || source.runtime_ir_ref_count == 0
    {
        return Err(anyhow!(
            "{label} authoring report has an invalid version, operation, or exact-module anchor"
        ));
    }
    if report.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .get("severity")
            .and_then(serde_json::Value::as_str)
            == Some("error")
    }) {
        return Err(anyhow!(
            "{label} authoring report contains an error diagnostic"
        ));
    }
    let validation = &report.validation;
    let finite_gate = validation
        .finite_theory_gate
        .as_ref()
        .ok_or_else(|| anyhow!("{label} authoring report omits its finite-theory receipt"))?;
    if !validation.canonical_axi_valid
        || !validation.compiled_kernel_ir_valid
        || !validation.finite_category_fragment_valid
        || validation.runtime_theory_gate != "passed"
        || validation.scope.trim().is_empty()
        || validation.non_claims.is_empty()
        || !finite_gate.passed
        || finite_gate.consumer != axiograph_kernel::FiniteTheoryGateConsumerIr::Authoring
        || !finite_gate.residual_obligations.is_empty()
        || finite_gate.accepted_snapshot_id.as_str() != source.compiled_snapshot_id
        || finite_gate.kernel_ir_digest.as_str() != source.kernel_ir_digest
    {
        return Err(anyhow!(
            "{label} authoring validation did not pass every canonical and runtime-theory gate"
        ));
    }
    let theory = validation
        .runtime_theory
        .as_ref()
        .ok_or_else(|| anyhow!("{label} authoring report omits its runtime-theory report"))?;
    validate_runtime_theory_report(theory, expected_revision, label)?;
    if report
        .typed_holes
        .get("theory")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|holes| !holes.is_empty())
    {
        return Err(anyhow!(
            "{label} authoring report retains unresolved runtime-theory holes"
        ));
    }
    let competency = report
        .competency_questions
        .as_ref()
        .ok_or_else(|| anyhow!("{label} authoring report omits competency questions"))?;
    let evaluation = competency
        .evaluation
        .as_ref()
        .ok_or_else(|| anyhow!("{label} authoring report omits CQ evaluation"))?;
    let satisfied = evaluation
        .get("satisfied")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| anyhow!("{label} authoring report omits CQ satisfied count"))?;
    let total = evaluation
        .get("total")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| anyhow!("{label} authoring report omits CQ total"))?;
    if total == 0
        || satisfied != total
        || competency.promotion_gate != "passed"
        || !competency.unresolved_question_names.is_empty()
        || competency.questions.is_empty()
        || competency.non_claims.is_empty()
    {
        return Err(anyhow!(
            "{label} authoring report does not pass every declared competency question"
        ));
    }
    let promotion = &report.promotion;
    let mut gates = BTreeMap::new();
    for gate in &promotion.gates {
        if gate.detail.trim().is_empty()
            || gates
                .insert(gate.gate.as_str(), gate.decision.as_str())
                .is_some()
        {
            return Err(anyhow!("{label} authoring promotion gate set is malformed"));
        }
    }
    let expected_gates = BTreeMap::from([
        ("canonical_validation", "passed"),
        ("competency_questions", "passed"),
        ("runtime_theory", "passed"),
        ("trusted_checker", "blocked"),
    ]);
    if !promotion.candidate_reviewable
        || promotion.protected_main_eligible
        || gates != expected_gates
        || promotion.required_write_authority.trim().is_empty()
        || promotion.scope.trim().is_empty()
        || promotion.non_claims.is_empty()
        || promotion.blockers.iter().any(|blocker| {
            blocker.contains("canonical validation")
                || blocker.contains("competency")
                || blocker.contains("runtime finite-fragment theory")
        })
    {
        return Err(anyhow!(
            "{label} authoring promotion review did not preserve the fail-closed gate decisions"
        ));
    }
    Ok(())
}

fn rerun_approved_query_verifier(
    config: &ApprovedQueryVerifierConfig,
    module_axi: &str,
    certificate_json: &str,
    expected_prepared_query_digest: &QueryIdV2,
    expected_answer_digest: &AnswerIdV2,
) -> Result<StoredVerifierReceiptV2> {
    const MAX_VERIFIER_EXECUTABLE_BYTES: usize = 256 * 1024 * 1024;
    const MAX_VERIFIER_INPUT_BYTES: usize = 16 * 1024 * 1024;
    const MAX_VERIFIER_OUTPUT_BYTES: usize = 1024 * 1024;
    const VERIFIER_PROTOCOL_V2: &str = "axiograph-verifier-stdio-v2";

    if config.approved_checker_build_id.trim().is_empty() {
        return Err(anyhow!(
            "approved query verifier build id must be non-empty"
        ));
    }
    let staged = axiograph_security::stage_approved_executable(
        &config.verifier_bin,
        &config.approved_checker_sha256,
        MAX_VERIFIER_EXECUTABLE_BYTES,
        if cfg!(windows) {
            "axiograph_verify.exe"
        } else {
            "axiograph_verify"
        },
        "regulated-shipment query verifier",
    )?;
    let nonce = uuid::Uuid::new_v4().to_string();
    let request = serde_json::to_vec(&QueryVerifierRequestV2 {
        version: VERIFIER_PROTOCOL_V2,
        nonce: &nonce,
        checker_sha256: &config.approved_checker_sha256,
        module_axi,
        certificate_json,
        expected_prepared_query_digest,
        expected_answer_digest,
    })?;
    let limits = axiograph_security::ProcessLimits::new(
        config.timeout,
        MAX_VERIFIER_INPUT_BYTES,
        MAX_VERIFIER_OUTPUT_BYTES,
        MAX_VERIFIER_OUTPUT_BYTES,
    )?;
    let mut command = Command::new(staged.executable());
    command.arg("--stdio-v2");
    let output = axiograph_security::run_command_bounded(
        command,
        &request,
        limits,
        "regulated-shipment approved query verifier",
    )?;
    let receipt: StoredVerifierReceiptV2 = axiograph_security::parse_json_bounded(
        &output.stdout,
        MAX_VERIFIER_OUTPUT_BYTES,
        "regulated-shipment verifier receipt",
    )
    .with_context(|| {
        format!(
            "approved query verifier did not return a strict receipt (stderr: {})",
            String::from_utf8_lossy(&output.stderr).trim()
        )
    })?;
    let expected_revision = RevisionDigestV2::from_accepted_text(module_axi);
    let expected_certificate =
        CertificateIdV2::from_canonical_fields(&[certificate_json.as_bytes()]);
    if !output.status.success()
        || receipt.version != VERIFIER_PROTOCOL_V2
        || receipt.nonce != nonce
        || receipt.checker_sha256 != config.approved_checker_sha256
        || receipt.checker_build_id != config.approved_checker_build_id
        || receipt.revision_digest_v2 != expected_revision
        || receipt.certificate_digest_v2 != expected_certificate
        || receipt.prepared_query_digest_v1 != *expected_prepared_query_digest
        || receipt.answer_digest_v1 != *expected_answer_digest
        || receipt.certificate_kind != "query_result_v4"
        || receipt.claim_kind != "finite_exact_complete"
        || receipt.decision != "accepted"
        || receipt.message.trim().is_empty()
    {
        return Err(anyhow!(
            "approved query verifier did not accept the exact anchored certificate and answer"
        ));
    }
    Ok(receipt)
}

fn evidence(
    authoring: &Path,
    theory: &Path,
    verification: &Path,
    query_verification: &Path,
    verifier: &ApprovedQueryVerifierConfig,
    exact_axi: &[u8],
    label: &str,
) -> Result<EvidenceBytes> {
    let authoring = read(authoring, &format!("{label} authoring report"))?;
    let theory = read(theory, &format!("{label} theory report"))?;
    let verification = read(
        verification,
        &format!("{label} VerifyMain category receipt"),
    )?;
    let query_verification = read(
        query_verification,
        &format!("{label} bound finite-query verification report"),
    )?;
    if authoring.is_empty()
        || theory.is_empty()
        || verification.is_empty()
        || query_verification.is_empty()
    {
        return Err(anyhow!(
            "{label} evidence inputs must be non-empty authoring, theory, category, and finite-query verification outputs"
        ));
    }
    let module_axi = std::str::from_utf8(exact_axi).context("canonical .axi is not UTF-8")?;
    let expected_revision = RevisionDigestV2::from_accepted_text(module_axi);
    let authoring_report: StoredAuthoringWorkspaceReportV1 =
        axiograph_security::parse_json_bounded(
            &authoring,
            16 * 1024 * 1024,
            "regulated-shipment authoring report",
        )
        .with_context(|| format!("parse {label} authoring report"))?;
    validate_authoring_report(&authoring_report, &expected_revision, label)?;
    let theory_report: StoredRuntimeTheoryModuleReportV1 = axiograph_security::parse_json_bounded(
        &theory,
        16 * 1024 * 1024,
        "regulated-shipment runtime-theory report",
    )
    .with_context(|| format!("parse {label} runtime-theory report"))?;
    validate_runtime_theory_report(&theory_report, &expected_revision, label)?;
    let authoring_theory = authoring_report
        .validation
        .runtime_theory
        .as_ref()
        .ok_or_else(|| anyhow!("{label} authoring report omitted runtime theory"))?;
    if authoring_theory.summary != theory_report.summary {
        return Err(anyhow!(
            "{label} standalone and authoring runtime-theory summaries do not match"
        ));
    }

    let query_report: StoredFiniteQueryVerificationReportV1 =
        axiograph_security::parse_json_bounded(
            &query_verification,
            16 * 1024 * 1024,
            "regulated-shipment finite-query verification report",
        )
        .with_context(|| format!("parse {label} finite-query verification report"))?;
    let historical_receipt = &query_report.verifier_receipt_v2;
    let certificate: CertificateV3 = axiograph_security::parse_json_bounded(
        query_report.certificate_text.as_bytes(),
        16 * 1024 * 1024,
        "regulated-shipment strict query_result_v4 certificate",
    )
    .context("finite-query report certificate_text is not a strict CertificateV3")?;
    let certificate_from_text = serde_json::to_value(&certificate)?;
    let recomputed_certificate_digest =
        CertificateIdV2::from_canonical_fields(&[query_report.certificate_text.as_bytes()]);
    let certificate_witness_count = certificate
        .proof
        .rows
        .iter()
        .map(|row| row.witnesses.len())
        .sum::<usize>();
    let actual_receipt = rerun_approved_query_verifier(
        verifier,
        module_axi,
        &query_report.certificate_text,
        &query_report.scope.prepared_query_digest_v1,
        &query_report.scope.answer_digest_v1,
    )?;
    if query_report.version != "finite_query_verification_report_v1"
        || query_report.decision != "accepted"
        || query_report.scope.revision_digest_v2 != expected_revision
        || query_report.scope.claim_kind != "finite_exact_complete"
        || query_report.coverage.selected_row_count != query_report.verified_rows.len()
        || query_report.coverage.selected_row_count != certificate.proof.rows.len()
        || query_report.coverage.selected_row_count == 0
        || query_report.coverage.row_witness_count != certificate_witness_count
        || query_report.coverage.row_witness_count == 0
        || query_report.coverage.runtime_truncated
        || !query_report.coverage.query_shape_certifiable
        || !query_report.coverage.certificate_emitted
        || !query_report.coverage.accepted_receipt_bound_to_exact_answer
        || !query_report.residual_obligations.is_empty()
        || query_report.non_claims.is_empty()
        || certificate_from_text != query_report.certificate
        || recomputed_certificate_digest != query_report.scope.certificate_digest_v2
        || !query_report.finite_theory_gate.passed
        || query_report.finite_theory_gate.consumer
            != axiograph_kernel::FiniteTheoryGateConsumerIr::Query
        || query_report.finite_theory_gate.accepted_snapshot_id
            != query_report.scope.accepted_snapshot_id
        || query_report.finite_theory_gate.kernel_ir_digest != query_report.scope.kernel_ir_digest
        || !query_report
            .finite_theory_gate
            .residual_obligations
            .is_empty()
        || query_report.prepared_query["certified_prepared_query_digest_v1"].as_str()
            != Some(query_report.scope.prepared_query_digest_v1.as_str())
        || certificate.anchor.revision_digest_v2 != expected_revision
        || certificate.proof.prepared_query_digest_v1 != query_report.scope.prepared_query_digest_v1
        || certificate.proof.answer_digest_v1 != query_report.scope.answer_digest_v1
        || historical_receipt.version != "axiograph-verifier-stdio-v2"
        || uuid::Uuid::parse_str(&historical_receipt.nonce).is_err()
        || historical_receipt.checker_sha256 != verifier.approved_checker_sha256
        || historical_receipt.checker_build_id != verifier.approved_checker_build_id
        || historical_receipt.revision_digest_v2 != expected_revision
        || historical_receipt.certificate_digest_v2 != query_report.scope.certificate_digest_v2
        || historical_receipt.prepared_query_digest_v1
            != query_report.scope.prepared_query_digest_v1
        || historical_receipt.answer_digest_v1 != query_report.scope.answer_digest_v1
        || historical_receipt.certificate_kind != "query_result_v4"
        || historical_receipt.claim_kind != "finite_exact_complete"
        || historical_receipt.decision != "accepted"
        || historical_receipt.message.trim().is_empty()
        || actual_receipt.checker_sha256 != historical_receipt.checker_sha256
        || actual_receipt.checker_build_id != historical_receipt.checker_build_id
        || actual_receipt.revision_digest_v2 != historical_receipt.revision_digest_v2
        || actual_receipt.certificate_digest_v2 != historical_receipt.certificate_digest_v2
        || actual_receipt.prepared_query_digest_v1 != historical_receipt.prepared_query_digest_v1
        || actual_receipt.answer_digest_v1 != historical_receipt.answer_digest_v1
        || actual_receipt.certificate_kind != historical_receipt.certificate_kind
        || actual_receipt.claim_kind != historical_receipt.claim_kind
        || actual_receipt.decision != historical_receipt.decision
    {
        return Err(anyhow!(
            "{label} finite-query trust gate is not an accepted exact-answer-bound query_result_v4 receipt"
        ));
    }
    Ok(EvidenceBytes {
        authoring,
        theory,
        verification,
        query_verification,
    })
}

fn compile(
    repository_id: &RepositoryIdV2,
    snapshot_id: &SnapshotIdV2,
    exact_axi: &[u8],
) -> Result<CompiledKernelSnapshot> {
    let source = CanonicalModuleSource::parse(exact_axi.to_vec())?;
    CanonicalCompiler::compile(KernelCompilationRequest {
        repository_id: repository_id.clone(),
        accepted_snapshot_id: snapshot_id.clone(),
        root_module: MODULE_NAME.to_string(),
        modules: vec![source],
    })
    .map_err(anyhow::Error::from)
}

fn blob(objects: &[ImmutableBlob], kind: ImmutableObjectKind) -> Result<ObjectBlobIdV2> {
    objects
        .iter()
        .find(|object| object.kind == kind)
        .map(|object| object.digest.clone())
        .ok_or_else(|| anyhow!("regulated-shipment plan omitted required {kind:?} object"))
}

fn typed_candidate(plan: &ScenarioPlan) -> Result<TypedCandidatePayloadV2> {
    Ok(TypedCandidatePayloadV2::checked(
        plan.promotion.snapshot.snapshot_id.clone(),
        plan.promotion.tree.tree_id.clone(),
        plan.compiled.ir().root_module_id().clone(),
        plan.promotion.manifest.kernel_ir_digest.clone(),
        plan.compiled
            .require_finite_theory_gate(axiograph_kernel::FiniteTheoryGateConsumerIr::Merge)?,
        plan.compiled.payload_fingerprints()?,
    )?)
}

fn gate_reports(plan: &ScenarioPlan) -> CandidateGateReportsV2 {
    CandidateGateReportsV2 {
        canonical_validation: plan.promotion.manifest.validation_report_digest.clone(),
        competency_questions: plan
            .promotion
            .manifest
            .competency_question_report_digest
            .clone(),
        trust: plan
            .promotion
            .manifest
            .trusted_checker_receipt_digest
            .clone(),
        runtime_theory: plan.promotion.manifest.runtime_theory_report_digest.clone(),
    }
}

fn reviewed_parent(plan: &ScenarioPlan, reviewer: &str) -> Result<ReviewedParentCandidateV2> {
    Ok(ReviewedParentCandidateV2 {
        commit_id: plan.promotion.commit.commit_id.clone(),
        candidate: typed_candidate(plan)?,
        gates: gate_reports(plan),
        reviewer: reviewer.to_string(),
        reviewed_at_unix_secs: 1_735_689_600,
    })
}

fn union_decisions(
    left: &ReviewedParentCandidateV2,
    right: &ReviewedParentCandidateV2,
    merged: &ReviewedMergedCandidateV2,
) -> Vec<TypedReconciliationDecisionV2> {
    let left_set = left
        .candidate
        .payloads
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let right_set = right
        .candidate
        .payloads
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let merged_set = merged
        .candidate
        .payloads
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let parent_union = left_set.union(&right_set).cloned().collect::<BTreeSet<_>>();
    let mut decisions = Vec::new();

    for source in &left_set {
        let origin = if right_set.contains(source) {
            CandidateOriginV2::Both
        } else {
            CandidateOriginV2::Left
        };
        if merged_set.contains(source) {
            decisions.push(TypedReconciliationDecisionV2::Keep {
                origin,
                source: source.clone(),
                target: source.clone(),
                rationale: "reviewed typed payload retained".to_string(),
            });
        } else {
            decisions.push(TypedReconciliationDecisionV2::Drop {
                origin,
                source: source.clone(),
                rationale: "baseline payload superseded by reviewed candidate".to_string(),
            });
        }
    }
    for source in right_set.difference(&left_set) {
        if merged_set.contains(source) {
            decisions.push(TypedReconciliationDecisionV2::Keep {
                origin: CandidateOriginV2::Right,
                source: source.clone(),
                target: source.clone(),
                rationale: "review-branch payload retained".to_string(),
            });
        } else {
            decisions.push(TypedReconciliationDecisionV2::Drop {
                origin: CandidateOriginV2::Right,
                source: source.clone(),
                rationale: "review-branch payload explicitly omitted".to_string(),
            });
        }
    }
    for target in merged_set.difference(&parent_union) {
        decisions.push(TypedReconciliationDecisionV2::Introduce {
            target: target.clone(),
            rationale: "reviewed merge result introduces this typed payload".to_string(),
        });
    }
    decisions
}

fn build_plan(
    repository_id: &RepositoryIdV2,
    exact_axi: Vec<u8>,
    evidence: &EvidenceBytes,
    ordered_parents: &[(CommitIdV2, SnapshotIdV2)],
    reconciliation_parents: Option<(CommitIdV2, &ScenarioPlan, &ScenarioPlan)>,
    action: &str,
) -> Result<ScenarioPlan> {
    let publication = ModulePublication::new(repository_id, MODULE_NAME, exact_axi.clone())?;
    let verification_text =
        std::str::from_utf8(&evidence.verification).context("VerifyMain receipt is not UTF-8")?;
    for required in [
        publication.module.revision_digest.as_str(),
        "ok: axi_well_typed module=RegulatedShipment",
        "ok: axi_constraints_ok module=RegulatedShipment",
        "ok: category_kernel_v3 schema=RegulatedShipment",
    ] {
        if !verification_text.contains(required) {
            return Err(anyhow!(
                "VerifyMain receipt for {} is missing `{required}`",
                publication.module.revision_digest
            ));
        }
    }
    let tree = AcceptedTree::new(repository_id.clone(), vec![publication.module.clone()])?;
    let snapshot = AcceptedSnapshot::new(
        repository_id.clone(),
        tree.tree_id.clone(),
        ordered_parents
            .iter()
            .map(|(_, snapshot)| snapshot.clone())
            .collect(),
    )?;
    let compiled = compile(repository_id, &snapshot.snapshot_id, &exact_axi)?;
    let mut objects = vec![
        ImmutableBlob::new(
            ImmutableObjectKind::KernelIr,
            serde_json::to_vec(compiled.ir())?,
        )?,
        ImmutableBlob::new(ImmutableObjectKind::CanonicalFactLog, exact_axi)?,
        ImmutableBlob::new(
            ImmutableObjectKind::ValidationReport,
            evidence.authoring.clone(),
        )?,
        ImmutableBlob::new(
            ImmutableObjectKind::CompetencyQuestionReport,
            evidence.authoring.clone(),
        )?,
        ImmutableBlob::new(ImmutableObjectKind::TheoryReport, evidence.theory.clone())?,
        ImmutableBlob::new(
            ImmutableObjectKind::VerificationReceipt,
            evidence.query_verification.clone(),
        )?,
    ];
    let validation = blob(&objects, ImmutableObjectKind::ValidationReport)?;
    let competency = blob(&objects, ImmutableObjectKind::CompetencyQuestionReport)?;
    let theory = blob(&objects, ImmutableObjectKind::TheoryReport)?;
    let verification = blob(&objects, ImmutableObjectKind::VerificationReceipt)?;
    let manifest = AcceptedBuildManifest {
        format: BUILD_MANIFEST_FORMAT.to_string(),
        version: FORMAT_VERSION,
        repository_id: repository_id.clone(),
        accepted_tree_id: tree.tree_id.clone(),
        accepted_snapshot_id: snapshot.snapshot_id.clone(),
        ordered_module_closure: vec![publication.module.revision_digest.clone()],
        compiler_version: "axiograph-kernel-canonical-compiler".to_string(),
        ir_version: "kernel-snapshot-ir-v2".to_string(),
        kernel_ir_digest: blob(&objects, ImmutableObjectKind::KernelIr)?,
        canonical_fact_log_digest: blob(&objects, ImmutableObjectKind::CanonicalFactLog)?,
        validation_report_digest: validation.clone(),
        competency_question_report_digest: competency.clone(),
        runtime_theory_report_digest: theory.clone(),
        trusted_checker_receipt_digest: verification.clone(),
        non_claims: required_non_claims(),
    };
    let current_candidate = TypedCandidatePayloadV2::checked(
        snapshot.snapshot_id.clone(),
        tree.tree_id.clone(),
        compiled.ir().root_module_id().clone(),
        manifest.kernel_ir_digest.clone(),
        compiled.require_finite_theory_gate(axiograph_kernel::FiniteTheoryGateConsumerIr::Merge)?,
        compiled.payload_fingerprints()?,
    )?;
    let reconciliation = if let Some((base, left_plan, right_plan)) = reconciliation_parents {
        let preview = ImmutableBlob::new(
            ImmutableObjectKind::Evidence,
            serde_json::to_vec(&serde_json::json!({
                "kind": "regulated_shipment_finite_reconciliation",
                "base": base,
                "scope": "exact compiled payload accounting; no arbitrary pushout claim"
            }))?,
        )?;
        let left = reviewed_parent(left_plan, "quality@example.test")?;
        let right = reviewed_parent(right_plan, "customs@example.test")?;
        let merged = ReviewedMergedCandidateV2 {
            candidate: current_candidate,
            gates: CandidateGateReportsV2 {
                canonical_validation: validation.clone(),
                competency_questions: competency.clone(),
                trust: verification.clone(),
                runtime_theory: theory.clone(),
            },
            reviewer: "release-authority@example.test".to_string(),
            reviewed_at_unix_secs: 1_735_689_700,
        };
        let decisions = union_decisions(&left, &right, &merged);
        let reconciliation = SemReconciliationV2::new(
            repository_id.clone(),
            base,
            left,
            right,
            merged,
            decisions,
            preview.digest.clone(),
            ReconciliationOutcomeV2::Materialized,
        )?;
        objects.push(preview);
        Some(reconciliation)
    } else {
        None
    };
    let commit_kind = if reconciliation.is_some() {
        CommitKind::Merge
    } else {
        CommitKind::Normal
    };
    let commit = SemCommitV2::new(
        repository_id.clone(),
        commit_kind,
        ordered_parents
            .iter()
            .map(|(commit, _)| commit.clone())
            .collect(),
        tree.tree_id.clone(),
        snapshot.snapshot_id.clone(),
        manifest.digest()?,
        reconciliation
            .as_ref()
            .map(|value| value.reconciliation_id.clone()),
        "operator@example.test",
        1_735_689_800,
        Some(format!("{action} regulated shipment")),
        action,
        "protected-main",
        CommitProvenance {
            source: "examples/regulated_shipment".to_string(),
            command: Some("make verify-regulated-shipment".to_string()),
            origin_trust: OriginTrust::Native,
        },
        SemanticDelta {
            changes: vec![SemanticChange {
                operation: ReindexOperation::Add,
                sources: Vec::new(),
                targets: vec![SemanticId::Revision(
                    publication.module.revision_digest.clone(),
                )],
            }],
        },
        vec![
            PromotionGate {
                kind: GateKind::CanonicalValidation,
                decision: GateDecision::Passed,
                report_digest: validation,
            },
            PromotionGate {
                kind: GateKind::CompetencyQuestions,
                decision: GateDecision::Passed,
                report_digest: competency,
            },
            PromotionGate {
                kind: GateKind::RuntimeTheory,
                decision: GateDecision::Passed,
                report_digest: theory,
            },
            PromotionGate {
                kind: GateKind::Trust,
                decision: GateDecision::Passed,
                report_digest: verification,
            },
        ],
        vec![CommitAttachment {
            kind: ImmutableObjectKind::KernelIr,
            digest: manifest.kernel_ir_digest.clone(),
        }],
        vec![LifecycleEvent {
            artifact: SemanticId::Revision(publication.module.revision_digest.clone()),
            from: Some(LifecycleStage::Reviewed),
            to: LifecycleStage::Accepted,
            reason: "reviewed regulated-shipment workflow".to_string(),
        }],
    )?;
    Ok(ScenarioPlan {
        promotion: PromotionPlan {
            modules: vec![publication],
            objects,
            tree,
            snapshot,
            manifest,
            reconciliation,
            commit,
            ref_updates: Vec::new(),
        },
        compiled,
    })
}

fn count_type_wrappers(type_expr: &TypeExprIr) -> (usize, usize) {
    match type_expr {
        TypeExprIr::Indexed { base, .. } => {
            let (indexed, refined) = count_type_wrappers(base);
            (indexed + 1, refined)
        }
        TypeExprIr::Refined { base, .. } => {
            let (indexed, refined) = count_type_wrappers(base);
            (indexed, refined + 1)
        }
        TypeExprIr::Object { .. } | TypeExprIr::RelationObject { .. } => (0, 0),
    }
}

fn category_evidence(
    compiled: &CompiledKernelSnapshot,
) -> Result<RegulatedShipmentCategoryEvidence> {
    let ir = compiled.ir();
    let gate = compiled
        .require_finite_theory_gate(axiograph_kernel::FiniteTheoryGateConsumerIr::Merge)
        .context("regulated-shipment candidate failed the typed merge gate")?;
    let object_types = ir.schemas().iter().map(|schema| schema.objects.len()).sum();
    let relation_objects = ir
        .schemas()
        .iter()
        .map(|schema| schema.relations.len())
        .sum();
    let role_projections = ir
        .schemas()
        .iter()
        .flat_map(|schema| &schema.relations)
        .map(|relation| relation.roles.len())
        .sum();
    let explicit_generators = ir
        .schemas()
        .iter()
        .flat_map(|schema| &schema.generators)
        .filter(|generator| {
            matches!(
                generator.kind,
                SchemaGeneratorKindIr::Aspect | SchemaGeneratorKindIr::Function
            )
        })
        .count();
    let (indexed_role_types, refined_role_types) = ir
        .schemas()
        .iter()
        .flat_map(|schema| &schema.relations)
        .flat_map(|relation| &relation.roles)
        .map(|role| count_type_wrappers(&role.type_expr))
        .fold((0, 0), |(ai, ar), (i, r)| (ai + i, ar + r));
    let path_equations = ir
        .theories()
        .iter()
        .flat_map(|theory| &theory.equations)
        .filter(|equation| equation.schema_equation.is_some())
        .count();
    let formal_groupoid_equations = ir
        .schemas()
        .iter()
        .map(|schema| schema.formal_groupoid_equations.len())
        .sum();
    let rewrite_rules = ir
        .theories()
        .iter()
        .map(|theory| theory.rewrite_rules.len())
        .sum();
    let finite_reachability_entries = ir
        .schemas()
        .iter()
        .filter_map(|schema| schema.category_formation.saturation.as_ref())
        .map(|certificate| certificate.entries.len())
        .sum();
    let role_indexed_witnesses = ir
        .instances()
        .iter()
        .map(|instance| instance.role_witnesses.len())
        .sum();
    let context_witnesses = ir
        .instances()
        .iter()
        .flat_map(|instance| &instance.scope_witnesses)
        .filter(|witness| witness.axis == ScopeAxisIr::Context)
        .count();
    let world_witnesses = ir
        .instances()
        .iter()
        .flat_map(|instance| &instance.scope_witnesses)
        .filter(|witness| witness.axis == ScopeAxisIr::World)
        .count();
    Ok(RegulatedShipmentCategoryEvidence {
        object_types,
        relation_objects,
        role_projections,
        explicit_generators,
        path_equations,
        formal_groupoid_equations,
        rewrite_rules,
        indexed_role_types,
        refined_role_types,
        finite_reachability_entries,
        role_indexed_witnesses,
        finite_refinement_predicates: gate.coverage.finite_refinement_predicates_replayed as usize,
        context_witnesses,
        world_witnesses,
        identity_scope_transports: gate.coverage.identity_scope_transports_replayed as usize,
        non_identity_scope_transports_certified: gate
            .coverage
            .non_identity_scope_transports_certified
            as usize,
    })
}

fn finite_query_evidence(bytes: &[u8]) -> Result<RegulatedShipmentQueryEvidence> {
    let report: StoredFiniteQueryVerificationReportV1 = axiograph_security::parse_json_bounded(
        bytes,
        16 * 1024 * 1024,
        "regulated-shipment finite-query verification report",
    )?;
    Ok(RegulatedShipmentQueryEvidence {
        claim_kind: report.scope.claim_kind,
        decision: report.decision,
        revision_digest_v2: report.scope.revision_digest_v2.to_string(),
        prepared_query_digest_v1: report.scope.prepared_query_digest_v1.to_string(),
        answer_digest_v1: report.scope.answer_digest_v1.to_string(),
        certificate_digest_v2: report.scope.certificate_digest_v2.to_string(),
        verified_rows: report.coverage.selected_row_count,
        path_witnesses: report.coverage.row_witness_count,
        receipt_bound_to_exact_answer: report.coverage.accepted_receipt_bound_to_exact_answer,
    })
}

pub fn run_workflow(
    inputs: &RegulatedShipmentWorkflowInputs,
) -> Result<RegulatedShipmentUsefulnessReport> {
    if inputs.store_dir.exists() {
        return Err(anyhow!(
            "store directory must not already exist: {}",
            inputs.store_dir.display()
        ));
    }
    let baseline_axi = read(&inputs.baseline_axi, "baseline canonical .axi")?;
    let candidate_axi = read(&inputs.candidate_axi, "candidate canonical .axi")?;
    let baseline_evidence = evidence(
        &inputs.baseline_authoring_report,
        &inputs.baseline_theory_report,
        &inputs.baseline_verification_receipt,
        &inputs.baseline_query_verification,
        &inputs.query_verifier,
        &baseline_axi,
        "baseline",
    )?;
    let candidate_evidence = evidence(
        &inputs.candidate_authoring_report,
        &inputs.candidate_theory_report,
        &inputs.candidate_verification_receipt,
        &inputs.candidate_query_verification,
        &inputs.query_verifier,
        &candidate_axi,
        "candidate",
    )?;

    let descriptor = RepositoryDescriptor::new(
        "regulated-shipment-usefulness",
        "regulated-shipment-proof-carrying-genesis",
    )?;
    let repository_id = descriptor.repository_id()?;
    let store = AxiStore::init(&inputs.store_dir, &descriptor)?;

    let baseline = build_plan(
        &repository_id,
        baseline_axi,
        &baseline_evidence,
        &[],
        None,
        "promote-baseline",
    )?;
    let mut status = store.promote(0, &baseline.promotion)?;

    let baseline_parent = [(
        baseline.promotion.commit.commit_id.clone(),
        baseline.promotion.snapshot.snapshot_id.clone(),
    )];
    let candidate = build_plan(
        &repository_id,
        candidate_axi.clone(),
        &candidate_evidence,
        &baseline_parent,
        None,
        "publish-review-candidate",
    )?;
    let source_ref = "heads/review/regulated-shipment";
    status = store.publish_candidate(
        status.state.generation,
        source_ref,
        None,
        &candidate.promotion,
    )?;

    let merge_parents = [
        (
            baseline.promotion.commit.commit_id.clone(),
            baseline.promotion.snapshot.snapshot_id.clone(),
        ),
        (
            candidate.promotion.commit.commit_id.clone(),
            candidate.promotion.snapshot.snapshot_id.clone(),
        ),
    ];
    let merge = build_plan(
        &repository_id,
        candidate_axi,
        &candidate_evidence,
        &merge_parents,
        Some((
            baseline.promotion.commit.commit_id.clone(),
            &baseline,
            &candidate,
        )),
        "materialize-reviewed-merge",
    )?;
    store.materialize_merge(
        status.state.generation,
        source_ref,
        &candidate.promotion.commit.commit_id,
        &merge.promotion,
    )?;

    let spec = AxpdBuildSpec::from_accepted_kernel(
        &merge.promotion.manifest,
        merge.compiled.ir(),
        &[],
        AxpdConfiguration::default(),
    )?;
    let receipt = store.publish_axpd(spec, &AxpdLimits::default())?;
    let materialization_id = receipt.materialization_id.clone();
    drop(store);

    let reopened = AxiStore::open(&inputs.store_dir)?;
    let restarted_status = reopened.status()?;
    let verified = reopened.open_axpd(&materialization_id, &AxpdLimits::default())?;
    let loaded = load_verified_pathdb(
        &inputs.store_dir,
        &materialization_id,
        &AxpdLimits::default(),
    )?;
    let image = verified.image();
    let expected_shipment_key = image
        .entities
        .iter()
        .find(|entity| entity.value == "Shipment_RX_1007")
        .map(|entity| entity.entity_key.to_string())
        .ok_or_else(|| anyhow!("verified restart image omitted Shipment_RX_1007"))?;
    let accepted_commit_id = restarted_status
        .state
        .accepted_commit_id
        .as_ref()
        .ok_or_else(|| anyhow!("restarted AxiStore has no accepted commit"))?;
    if accepted_commit_id != &merge.promotion.commit.commit_id {
        return Err(anyhow!(
            "restarted AxiStore accepted a different merge commit"
        ));
    }
    let reconciliation = merge
        .promotion
        .reconciliation
        .as_ref()
        .ok_or_else(|| anyhow!("merge plan omitted typed reconciliation"))?;
    let accepted_grounding = accepted_grounding_context(&loaded, "Shipment_RX_1007", 16)
        .context("build accepted-derived grounding from verified materialization")?;
    if accepted_grounding.provenance().plane() != AcceptedGroundingPlaneV1::AcceptedDerived {
        return Err(anyhow!(
            "verified grounding did not retain accepted-derived plane"
        ));
    }
    if accepted_grounding.provenance().accepted_snapshot_id()
        != merge.promotion.snapshot.snapshot_id.as_str()
    {
        return Err(anyhow!(
            "verified grounding accepted snapshot differs from reviewed merge"
        ));
    }
    if !accepted_grounding
        .facts()
        .iter()
        .any(|fact| fact.stable_id() == expected_shipment_key.as_str())
    {
        return Err(anyhow!(
            "verified grounding omitted exact accepted Shipment_RX_1007 key {expected_shipment_key}"
        ));
    }

    Ok(RegulatedShipmentUsefulnessReport {
        version: REGULATED_SHIPMENT_USEFULNESS_REPORT_VERSION.to_string(),
        scenario: "regulated_pharmaceutical_shipment_release".to_string(),
        repository_id: repository_id.to_string(),
        baseline_snapshot_id: baseline.promotion.snapshot.snapshot_id.to_string(),
        accepted_snapshot_id: merge.promotion.snapshot.snapshot_id.to_string(),
        kernel_ir_digest: merge.promotion.manifest.kernel_ir_digest.to_string(),
        canonical_revision_digest: merge.promotion.modules[0]
            .module
            .revision_digest
            .to_string(),
        category: category_evidence(&merge.compiled)?,
        finite_query: finite_query_evidence(&candidate_evidence.query_verification)?,
        merge: RegulatedShipmentMergeEvidence {
            operation: "reviewed_finite_typed_replacement_merge".to_string(),
            ordered_parent_count: merge.promotion.commit.ordered_parents.len(),
            typed_decisions: reconciliation.decisions.len(),
            source_ref: source_ref.to_string(),
            materialized_commit_id: merge.promotion.commit.commit_id.to_string(),
            finite_contract:
                "exact compiled payload keep/drop/introduce/transport accounting".to_string(),
        },
        persistence: RegulatedShipmentPersistenceEvidence {
            store_generation_after_restart: restarted_status.state.generation,
            accepted_commit_id_after_restart: accepted_commit_id.to_string(),
            materialization_id: materialization_id.to_string(),
            exact_image_digest: receipt.exact_image_digest.to_string(),
            entity_rows: image.entities.len(),
            relation_fact_rows: image.relation_facts.len(),
            hydrated_entities: loaded.db().entities.len(),
            hydrated_relations: loaded.db().relations.len(),
            shipment_rx_1007_present_after_restart: true,
        },
        accepted_grounding: RegulatedShipmentAcceptedGroundingEvidence {
            plane: "accepted_derived".to_string(),
            accepted_snapshot_id: accepted_grounding
                .provenance()
                .accepted_snapshot_id()
                .to_string(),
            materialization_id: accepted_grounding
                .provenance()
                .materialization_id()
                .to_string(),
            query_digest: accepted_grounding.provenance().query_digest().to_string(),
            selection_digest: accepted_grounding
                .provenance()
                .selection_digest()
                .to_string(),
            stable_ids: accepted_grounding
                .facts()
                .iter()
                .map(|fact| fact.stable_id().to_string())
                .collect(),
            truncated: accepted_grounding.truncated(),
            truncation_reasons: accepted_grounding.truncation_reasons().to_vec(),
            non_claims: accepted_grounding.non_claims().to_vec(),
        },
        trusted_receipt_inputs: vec![
            inputs.baseline_query_verification.display().to_string(),
            inputs.candidate_query_verification.display().to_string(),
        ],
        checked_runtime_scope: vec![
            "canonical compiler over exact baseline and candidate bytes".to_string(),
            "finite relation objects, ordered role projections, indexed/refined role witnesses, checked formal groupoid equations, and explanation-certified saturation".to_string(),
            "AxiStore exact-two-parent reviewed merge and authenticated SQLite materialization".to_string(),
            "receipt-checked PathDB hydration after process-level reopen".to_string(),
            "accepted-derived grounding bound to the reopened materialization, accepted snapshot, and exact query digest".to_string(),
        ],
        non_claims: vec![
            "no arbitrary categorical pushout, colimit, or complete-lattice merge claim".to_string(),
            "no general dependent type theory, univalence, higher inductive type, or unrestricted HoTT claim".to_string(),
            "no open-world, regulatory, evidence, backend-query, or ontology-closure completeness claim".to_string(),
            "AxiStore and PathDB checks are Rust operational checks; only claims accepted by the VerifyMain import closure are trusted".to_string(),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiograph_pathdb::certificate::{
        answer_digest_v1, CertificateAnchorV2, FiniteQueryAtomV4, FiniteQueryAtomWitnessV4,
        FiniteQueryBindingV4, FiniteQueryRowV4, FiniteQueryTermV4, FiniteQueryV4,
        PreparedQueryBindingV1, QueryResultProofV4,
    };
    use axiograph_projections::{
        check_readback_v1, manifest_readback_fixture_v1, project_snapshot_v1, ProjectionBackendV1,
        ReadbackTransportStatusV1,
    };
    use std::fs;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn workflow_cli_authoring_artifacts_satisfy_canonical_consumer_contract() {
        let repository_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap();
        let output_dir = tempdir().unwrap();
        for (phase, request, axi) in [
            (
                "baseline",
                "authoring_baseline_request.json",
                "RegulatedShipmentBaseline.axi",
            ),
            (
                "candidate",
                "authoring_request.json",
                "RegulatedShipment.axi",
            ),
        ] {
            let fixtures = repository_root.join("examples/regulated_shipment");
            let artifact = output_dir.path().join(format!("{phase}_authoring.json"));
            // Use the script's actual requests without a --detail override. Cargo
            // builds the current CLI rather than relying on a stale local binary.
            let output = Command::new(env!("CARGO"))
                .args(["run", "--quiet", "--locked", "--offline", "--manifest-path"])
                .arg(repository_root.join("rust/Cargo.toml"))
                .args(["-p", "axiograph-cli", "--bin", "axiograph", "--"])
                .args(["authoring", "workspace", "--workspace"])
                .arg(&repository_root)
                .arg("--request")
                .arg(fixtures.join(request))
                .arg("--out")
                .arg(&artifact)
                .output()
                .expect("run actual authoring CLI");
            assert!(
                output.status.success(),
                "{phase} authoring CLI failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let bytes = read(&artifact, "workflow authoring artifact").unwrap();
            let report: StoredAuthoringWorkspaceReportV1 = axiograph_security::parse_json_bounded(
                &bytes,
                16 * 1024 * 1024,
                "regulated-shipment authoring report",
            )
            .unwrap_or_else(|error| panic!("parse {phase} authoring report: {error}"));
            let exact_axi = fs::read_to_string(fixtures.join(axi)).unwrap();
            let revision = RevisionDigestV2::from_accepted_text(&exact_axi);
            validate_authoring_report(&report, &revision, phase)
                .unwrap_or_else(|error| panic!("validate {phase} authoring report: {error}"));
        }
    }

    #[cfg(unix)]
    fn fixture_query_verifier(root: &Path) -> ApprovedQueryVerifierConfig {
        use std::os::unix::fs::PermissionsExt;

        let path = root.join("approved-query-verifier.py");
        let script = r#"#!/usr/bin/env python3
import hashlib
import json
import struct
import sys


def identity(domain, field):
    domain_bytes = domain.encode("utf-8")
    preimage = (
        b"AXIOGRAPH-ID"
        + struct.pack(">H", 2)
        + struct.pack(">H", len(domain_bytes))
        + domain_bytes
        + struct.pack(">I", 1)
        + struct.pack(">Q", len(field))
        + field
    )
    return "axi:" + domain + ":v2:sha256:" + hashlib.sha256(preimage).hexdigest()


request = json.load(sys.stdin)
certificate_bytes = request["certificate_json"].encode("utf-8")
module_bytes = request["module_axi"].encode("utf-8")
print(json.dumps({
    "version": "axiograph-verifier-stdio-v2",
    "nonce": request["nonce"],
    "checker_sha256": request["checker_sha256"],
    "checker_build_id": "axiograph-test-verifier-v1",
    "revision_digest_v2": identity("revision", module_bytes),
    "certificate_digest_v2": identity("certificate", certificate_bytes),
    "prepared_query_digest_v1": request["expected_prepared_query_digest"],
    "answer_digest_v1": request["expected_answer_digest"],
    "certificate_kind": "query_result_v4",
    "claim_kind": "finite_exact_complete",
    "decision": "accepted",
    "message": "test verifier accepted exact request"
}))
"#;
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o500)).unwrap();
        let approved_checker_sha256 =
            axiograph_security::sha256_file_bounded(&path, 1024 * 1024, "test query verifier")
                .unwrap();
        ApprovedQueryVerifierConfig {
            verifier_bin: path,
            approved_checker_sha256,
            approved_checker_build_id: "axiograph-test-verifier-v1".to_string(),
            timeout: Duration::from_secs(10),
        }
    }

    #[cfg(not(unix))]
    fn fixture_query_verifier(_root: &Path) -> ApprovedQueryVerifierConfig {
        panic!("regulated-shipment approved-executable fixture currently requires Unix")
    }

    fn fixture_inputs(root: &Path, store_dir: PathBuf) -> RegulatedShipmentWorkflowInputs {
        let repository_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let baseline =
            repository_root.join("examples/regulated_shipment/RegulatedShipmentBaseline.axi");
        let candidate = repository_root.join("examples/regulated_shipment/RegulatedShipment.axi");
        let write_fixture = |name: &str, value: serde_json::Value| {
            let path = root.join(name);
            fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
            path
        };
        let write_runtime_evidence = |prefix: &str, axi_path: &Path| {
            let exact_axi = fs::read(axi_path).unwrap();
            let source = CanonicalModuleSource::parse(exact_axi.clone()).unwrap();
            let revision = source.revision().clone();
            let repository_id = RepositoryIdV2::from_descriptor_bytes(prefix.as_bytes());
            let snapshot_id = SnapshotIdV2::from_canonical_fields(&[prefix.as_bytes()]);
            let compiled = CanonicalCompiler::compile(KernelCompilationRequest {
                repository_id: repository_id.clone(),
                accepted_snapshot_id: snapshot_id.clone(),
                root_module: MODULE_NAME.to_string(),
                modules: vec![source.clone()],
            })
            .unwrap();
            let runtime =
                axiograph_pathdb::derive_runtime_package_index(&compiled, &[source]).unwrap();
            let mut reports = runtime
                .theories
                .iter()
                .map(|theory| {
                    let schema = runtime
                        .schemas
                        .iter()
                        .find(|schema| schema.schema_id == theory.schema_id)
                        .unwrap();
                    axiograph_pathdb::check_runtime_theory_v1(schema, theory)
                })
                .collect::<Vec<_>>();
            reports.sort_by_key(|report| report.theory_ref.stable_id());
            let blocking_errors = reports
                .iter()
                .flat_map(|report| report.judgments.iter())
                .filter(|judgment| judgment.status == RuntimeTheoryCheckStatusV1::Blocked)
                .count();
            let notes = vec![
                "runtime theory check reports are typed operational artifacts, not Lean certificates"
                    .to_string(),
                "blocking judgments should fail promotion/check gates; review-only judgments remain explicit weak claims"
                    .to_string(),
            ];
            let summary = axiograph_tooling_overlays::runtime_theory_check_summary_v1(
                revision.as_str(),
                &reports,
                blocking_errors,
                notes.clone(),
            );
            let theory_report = serde_json::json!({
                "version": "runtime_theory_check_module_report_v1",
                "module_digest": revision,
                "summary": summary,
                "reports": reports,
                "blocking_errors": blocking_errors,
                "trust_boundary": "Runtime checked in Rust; not Lean verified.",
                "non_claims": summary.non_claims,
                "notes": notes
            });
            let finite_gate = compiled
                .require_finite_theory_gate(axiograph_kernel::FiniteTheoryGateConsumerIr::Authoring)
                .unwrap();
            let authoring_report = serde_json::json!({
                "version": "authoring_workspace_report_v1",
                "operation": "promotion_review",
                "workspace_root": ".",
                "ok": true,
                "source": {
                    "workspace_relative_path": axi_path.file_name().unwrap().to_string_lossy(),
                    "root_module": MODULE_NAME,
                    "repository_id": repository_id,
                    "compiled_snapshot_id": snapshot_id,
                    "kernel_ir_digest": compiled.ir().ir_digest(),
                    "exact_root_axi_digest": revision,
                    "ordered_module_closure": [{
                        "module_name": MODULE_NAME,
                        "module_id": compiled.ir().root_module_id(),
                        "revision_digest": revision
                    }],
                    "runtime_ir_ref_count": compiled.ir().refs().len()
                },
                "diagnostics": [],
                "validation": {
                    "canonical_axi_valid": true,
                    "compiled_kernel_ir_valid": true,
                    "finite_category_fragment_valid": true,
                    "finite_theory_gate": finite_gate,
                    "runtime_theory_gate": "passed",
                    "runtime_theory": theory_report,
                    "scope": "exact finite canonical fixture",
                    "non_claims": ["Rust authoring validation is not Lean certification"]
                },
                "typed_holes": {"theory": []},
                "dependent_refinements": [],
                "repairs": [],
                "competency_questions": {
                    "questions": [{"name":"one"},{"name":"two"},{"name":"three"}],
                    "evaluation": {"satisfied": 3, "total": 3},
                    "unresolved_question_names": [],
                    "promotion_gate": "passed",
                    "non_claims": ["finite competency fixture"]
                },
                "evolution_previews": [],
                "promotion": {
                    "candidate_reviewable": true,
                    "protected_main_eligible": false,
                    "gates": [
                        {"gate":"canonical_validation","decision":"passed","detail":"compiled"},
                        {"gate":"competency_questions","decision":"passed","detail":"checked"},
                        {"gate":"runtime_theory","decision":"passed","detail":"checked"},
                        {"gate":"trusted_checker","decision":"blocked","detail":"separate receipt required"}
                    ],
                    "blockers": [
                        "no trusted-checker receipt from the VerifyMain import closure is attached",
                        "read-only authoring adapters do not construct or execute an AxiStore PromotionPlan"
                    ],
                    "required_write_authority": "AxiStore::promote",
                    "scope": "read-only authoring review",
                    "non_claims": ["not a protected-main mutation"]
                },
                "stable_runtime_refs": [],
                "next_actions": [],
                "trust": {}
            });
            (
                write_fixture(&format!("{prefix}-authoring.json"), authoring_report),
                write_fixture(&format!("{prefix}-theory.json"), theory_report),
            )
        };
        let (baseline_authoring, baseline_theory) = write_runtime_evidence("baseline", &baseline);
        let (candidate_authoring, candidate_theory) =
            write_runtime_evidence("candidate", &candidate);
        let write_receipt = |name: &str, axi_path: &Path| {
            let axi_text = fs::read_to_string(axi_path).unwrap();
            let revision = axiograph_kernel::RevisionDigestV2::from_accepted_text(&axi_text);
            let path = root.join(name);
            fs::write(
                &path,
                format!(
                    "ok: loaded axi module revision={revision}\n\
                     ok: axi_well_typed module=RegulatedShipment schemas=1 instances=1\n\
                     ok: axi_constraints_ok module=RegulatedShipment constraints=1 checks=1\n\
                     ok: category_kernel_v3 schema=RegulatedShipment objects=1 arrows=1 equations=0 congruence=0 groupoid_normalizations=2 reachability=1 lifecycle=explanationVerified\n"
                ),
            )
            .unwrap();
            path
        };
        let baseline_verification = write_receipt("baseline-verification.txt", &baseline);
        let candidate_verification = write_receipt("candidate-verification.txt", &candidate);
        let query_verifier = fixture_query_verifier(root);
        let write_query_verification = |name: &str, axi_path: &Path| {
            let exact_axi = fs::read(axi_path).unwrap();
            let revision = axiograph_kernel::RevisionDigestV2::from_accepted_text(
                std::str::from_utf8(&exact_axi).unwrap(),
            );
            let repository_id = RepositoryIdV2::from_descriptor_bytes(name.as_bytes());
            let snapshot_id = SnapshotIdV2::from_canonical_fields(&[name.as_bytes()]);
            let compiled = compile(&repository_id, &snapshot_id, &exact_axi).unwrap();
            let finite_gate = compiled
                .require_finite_theory_gate(axiograph_kernel::FiniteTheoryGateConsumerIr::Query)
                .unwrap();
            let binding = PreparedQueryBindingV1::new(
                FiniteQueryV4 {
                    select_vars: vec!["?shipment".to_string()],
                    disjuncts: vec![vec![FiniteQueryAtomV4::Type {
                        term: FiniteQueryTermV4::Var {
                            name: "?shipment".to_string(),
                        },
                        type_name: "Shipment".to_string(),
                    }]],
                    max_hops: None,
                    min_confidence_fp: None,
                },
                1,
            );
            let rows = vec![FiniteQueryRowV4 {
                disjunct: 0,
                bindings: vec![FiniteQueryBindingV4 {
                    var: "?shipment".to_string(),
                    entity: "Shipment_RX_1007".to_string(),
                }],
                witnesses: vec![FiniteQueryAtomWitnessV4::Type {
                    entity: "Shipment_RX_1007".to_string(),
                    type_name: "Shipment".to_string(),
                }],
            }];
            let prepared = binding.digest_v1().unwrap();
            let answer = answer_digest_v1(&binding, &prepared, &rows, false).unwrap();
            let proof = QueryResultProofV4 {
                binding,
                prepared_query_digest_v1: prepared.clone(),
                rows,
                runtime_truncated: false,
                answer_digest_v1: answer.clone(),
            };
            let strict_certificate =
                CertificateV3::query_result_v4(CertificateAnchorV2::new(revision.clone()), proof)
                    .unwrap();
            let certificate_payload = serde_json::to_value(&strict_certificate).unwrap();
            let certificate_text = serde_json::to_string_pretty(&strict_certificate).unwrap();
            let certificate = axiograph_kernel::CertificateIdV2::from_canonical_fields(&[
                certificate_text.as_bytes(),
            ]);
            write_fixture(
                name,
                serde_json::json!({
                    "version": "finite_query_verification_report_v1",
                    "decision": "accepted",
                    "scope": {
                        "revision_digest_v2": revision,
                        "accepted_snapshot_id": snapshot_id,
                        "kernel_ir_digest": compiled.ir().ir_digest(),
                        "prepared_query_digest_v1": prepared,
                        "answer_digest_v1": answer,
                        "certificate_digest_v2": certificate,
                        "claim_kind": "finite_exact_complete"
                    },
                    "coverage": {
                        "selected_row_count": 1,
                        "row_witness_count": 1,
                        "runtime_truncated": false,
                        "query_shape_certifiable": true,
                        "certificate_emitted": true,
                        "accepted_receipt_bound_to_exact_answer": true
                    },
                    "finite_theory_gate": finite_gate,
                    "prepared_query": {
                        "certified_prepared_query_digest_v1": prepared
                    },
                    "certificate": certificate_payload,
                    "certificate_text": certificate_text,
                    "verifier_receipt_v2": {
                        "version": "axiograph-verifier-stdio-v2",
                        "nonce": "00000000-0000-4000-8000-000000000001",
                        "checker_sha256": query_verifier.approved_checker_sha256,
                        "checker_build_id": query_verifier.approved_checker_build_id,
                        "revision_digest_v2": revision,
                        "certificate_digest_v2": certificate,
                        "prepared_query_digest_v1": prepared,
                        "answer_digest_v1": answer,
                        "certificate_kind": "query_result_v4",
                        "claim_kind": "finite_exact_complete",
                        "decision": "accepted",
                        "message": "exact finite query answer verified"
                    },
                    "verified_rows": [{"certificate": "CoA_RX_42"}],
                    "residual_obligations": [],
                    "non_claims": ["finite fixture only"]
                }),
            )
        };
        let baseline_query_verification =
            write_query_verification("baseline-query-verification.json", &baseline);
        let candidate_query_verification =
            write_query_verification("candidate-query-verification.json", &candidate);
        RegulatedShipmentWorkflowInputs {
            baseline_axi: baseline,
            candidate_axi: candidate,
            baseline_authoring_report: baseline_authoring,
            candidate_authoring_report: candidate_authoring,
            baseline_theory_report: baseline_theory,
            candidate_theory_report: candidate_theory,
            baseline_verification_receipt: baseline_verification,
            candidate_verification_receipt: candidate_verification,
            baseline_query_verification,
            candidate_query_verification,
            query_verifier,
            store_dir,
        }
    }

    #[test]
    fn regulated_shipment_v4_grounding_wire_schema_is_exact() {
        assert_eq!(
            REGULATED_SHIPMENT_USEFULNESS_REPORT_VERSION,
            "regulated_shipment_usefulness_report_v4"
        );
        let evidence = RegulatedShipmentAcceptedGroundingEvidence {
            plane: "accepted_derived".to_string(),
            accepted_snapshot_id: "snapshot".to_string(),
            materialization_id: "materialization".to_string(),
            query_digest: "query".to_string(),
            selection_digest: "selection".to_string(),
            stable_ids: vec!["entity:key".to_string()],
            truncated: true,
            truncation_reasons: vec!["output_byte_limit".to_string()],
            non_claims: vec!["lexical selection only".to_string()],
        };
        assert_eq!(
            serde_json::to_value(evidence).unwrap(),
            serde_json::json!({
                "plane": "accepted_derived",
                "accepted_snapshot_id": "snapshot",
                "materialization_id": "materialization",
                "query_digest": "query",
                "selection_digest": "selection",
                "stable_ids": ["entity:key"],
                "truncated": true,
                "truncation_reasons": ["output_byte_limit"],
                "non_claims": ["lexical selection only"]
            })
        );
    }

    #[test]
    fn regulated_shipment_survives_typed_merge_materialization_and_restart() {
        let temp = tempdir().unwrap();
        let inputs = fixture_inputs(temp.path(), temp.path().join("store"));
        let report = run_workflow(&inputs).unwrap();
        assert_eq!(report.version, REGULATED_SHIPMENT_USEFULNESS_REPORT_VERSION);
        assert_eq!(report.merge.ordered_parent_count, 2);
        assert!(report.merge.typed_decisions > 0);
        assert!(report.category.relation_objects >= 9);
        assert!(report.category.role_projections >= 30);
        assert_eq!(report.category.path_equations, 1);
        assert_eq!(report.category.formal_groupoid_equations, 1);
        assert_eq!(report.category.rewrite_rules, 1);
        assert_eq!(report.category.indexed_role_types, 1);
        assert_eq!(report.category.refined_role_types, 1);
        assert!(report.category.finite_reachability_entries > 0);
        assert!(report.category.role_indexed_witnesses > 0);
        assert!(report.category.finite_refinement_predicates > 0);
        assert!(report.category.context_witnesses > 0);
        assert!(report.category.identity_scope_transports > 0);
        assert_eq!(report.category.non_identity_scope_transports_certified, 0);
        assert_eq!(report.finite_query.claim_kind, "finite_exact_complete");
        assert_eq!(report.finite_query.decision, "accepted");
        assert_eq!(report.finite_query.verified_rows, 1);
        assert!(report.finite_query.path_witnesses > 0);
        assert!(report.finite_query.receipt_bound_to_exact_answer);
        assert!(report.persistence.entity_rows > 0);
        assert!(report.persistence.relation_fact_rows > 0);
        assert!(report.persistence.shipment_rx_1007_present_after_restart);
        assert_eq!(report.accepted_grounding.plane, "accepted_derived");
        assert_eq!(
            report.accepted_grounding.accepted_snapshot_id,
            report.accepted_snapshot_id
        );
        assert_eq!(
            report.accepted_grounding.materialization_id,
            report.persistence.materialization_id
        );
        let materialization_id = report
            .persistence
            .materialization_id
            .parse()
            .expect("typed materialization identity");
        let store = AxiStore::open(&inputs.store_dir).unwrap();
        let verified = store
            .open_axpd(&materialization_id, &AxpdLimits::default())
            .unwrap();
        let expected_shipment_key = verified
            .image()
            .entities
            .iter()
            .find(|entity| entity.value == "Shipment_RX_1007")
            .map(|entity| entity.entity_key.to_string())
            .expect("verified image must contain Shipment_RX_1007");
        assert!(report
            .accepted_grounding
            .stable_ids
            .contains(&expected_shipment_key));
        assert_eq!(report.accepted_grounding.non_claims.len(), 4);
        assert!(report
            .accepted_grounding
            .non_claims
            .iter()
            .any(|non_claim| {
                non_claim
                    == "lexical grounding selection is not an entailment or completeness proof"
            }));
        assert!(report
            .accepted_grounding
            .non_claims
            .iter()
            .any(|non_claim| {
                non_claim == "accepted-derived source rows do not certify downstream LLM output"
            }));
        let grounding_limit = 16_u64.to_be_bytes();
        let expected_query_digest = ObjectBlobIdV2::from_canonical_fields(&[
            b"axiograph_accepted_grounding_query_v1",
            b"Shipment_RX_1007",
            &grounding_limit,
        ]);
        assert_eq!(
            report.accepted_grounding.query_digest,
            expected_query_digest.to_string()
        );
        let truncated_bytes = [u8::from(report.accepted_grounding.truncated)];
        let mut selection_fields =
            Vec::with_capacity(report.accepted_grounding.stable_ids.len() + 4);
        selection_fields.push(b"axiograph_accepted_grounding_selection_v1".as_slice());
        selection_fields.push(expected_query_digest.as_str().as_bytes());
        selection_fields.push(report.accepted_grounding.materialization_id.as_bytes());
        selection_fields.push(truncated_bytes.as_slice());
        selection_fields.extend(
            report
                .accepted_grounding
                .stable_ids
                .iter()
                .map(String::as_bytes),
        );
        assert_eq!(
            report.accepted_grounding.selection_digest,
            ObjectBlobIdV2::from_canonical_fields(&selection_fields).to_string()
        );
        assert!(!report.accepted_grounding.truncated);
        assert!(report.accepted_grounding.truncation_reasons.is_empty());
    }

    #[test]
    fn tampered_materialization_is_rejected_after_restart() {
        let temp = tempdir().unwrap();
        let inputs = fixture_inputs(temp.path(), temp.path().join("store"));
        let report = run_workflow(&inputs).unwrap();
        let materialization_id = report
            .persistence
            .materialization_id
            .parse()
            .expect("typed materialization id");
        let store = AxiStore::open(&inputs.store_dir).unwrap();
        let image_path = store.axpd_image_path(&materialization_id);
        drop(store);
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(image_path)
            .unwrap();
        file.write_all(b"tampered").unwrap();
        file.sync_all().unwrap();
        let reopened = AxiStore::open(&inputs.store_dir).unwrap();
        assert!(reopened
            .open_axpd(&materialization_id, &AxpdLimits::default())
            .is_err());
    }

    #[test]
    fn regulated_shipment_projection_preserves_typed_records_and_reports_drift() {
        let repository_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let exact_axi =
            fs::read(repository_root.join("examples/regulated_shipment/RegulatedShipment.axi"))
                .unwrap();
        let repository_id =
            RepositoryIdV2::from_descriptor_bytes(b"regulated-shipment-projection-test");
        let snapshot_id =
            SnapshotIdV2::from_canonical_fields(&[b"regulated-shipment-projection-snapshot"]);
        let compiled = compile(&repository_id, &snapshot_id, &exact_axi).unwrap();
        let manifest = project_snapshot_v1(&compiled, ProjectionBackendV1::TypeDb).unwrap();
        assert!(manifest.coverage.relation_objects >= 9);
        assert!(manifest.coverage.role_projections >= 30);
        assert_eq!(manifest.coverage.path_equations, 1);
        assert!(!manifest.semantic_loss.lossless_semantic_projection_claim);

        let exact = manifest_readback_fixture_v1(&manifest);
        let exact_report = check_readback_v1(&manifest, &exact).unwrap();
        assert_eq!(
            exact_report.transport_status,
            ReadbackTransportStatusV1::ExactFiniteRecordMatch
        );
        assert!(!exact_report.semantic_equivalence_claim);

        let mut drifted = exact;
        drifted.records[0].payload_fingerprint =
            ObjectBlobIdV2::from_canonical_fields(&[b"regulated-shipment-drift"]);
        let drift_report = check_readback_v1(&manifest, &drifted).unwrap();
        assert_eq!(
            drift_report.transport_status,
            ReadbackTransportStatusV1::DriftDetected
        );
        assert_eq!(drift_report.drifted_records.len(), 1);
        assert!(!drift_report.evidence.accepted_state_change);
    }

    #[test]
    fn forged_verification_receipt_is_rejected_before_candidate_publication() {
        let temp = tempdir().unwrap();
        let inputs = fixture_inputs(temp.path(), temp.path().join("store"));
        fs::write(
            &inputs.candidate_verification_receipt,
            "ok: unbound self-assertion\n",
        )
        .unwrap();
        let error = run_workflow(&inputs).expect_err("forged receipt must fail closed");
        assert!(error
            .to_string()
            .contains("VerifyMain receipt for axi:revision:v2:sha256:"));
    }

    #[test]
    fn review_only_runtime_theory_gate_is_rejected_before_protected_main_advances() {
        let temp = tempdir().unwrap();
        let inputs = fixture_inputs(temp.path(), temp.path().join("store"));
        let mut report: serde_json::Value =
            serde_json::from_slice(&fs::read(&inputs.candidate_authoring_report).unwrap()).unwrap();
        report["validation"]["runtime_theory_gate"] =
            serde_json::Value::String("blocked".to_string());
        report["validation"]["runtime_theory"]["summary"]["review_only_obligations"] =
            serde_json::json!(1);
        report["validation"]["runtime_theory"]["summary"]["residual_obligation_ids"] =
            serde_json::json!(["equation:review-only"]);
        report["promotion"]["gates"][2]["decision"] =
            serde_json::Value::String("blocked".to_string());
        fs::write(
            &inputs.candidate_authoring_report,
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        let error = run_workflow(&inputs).expect_err("review-only theory must fail closed");
        assert!(error.to_string().contains("authoring validation"));
        assert!(!inputs.store_dir.exists());
    }

    #[test]
    fn placeholder_query_receipt_is_rejected_before_protected_main_advances() {
        let temp = tempdir().unwrap();
        let inputs = fixture_inputs(temp.path(), temp.path().join("store"));
        let mut report: serde_json::Value =
            serde_json::from_slice(&fs::read(&inputs.candidate_query_verification).unwrap())
                .unwrap();
        report["verifier_receipt_v2"]["certificate_digest_v2"] =
            serde_json::Value::String("placeholder-witness".to_string());
        fs::write(
            &inputs.candidate_query_verification,
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        let error = run_workflow(&inputs).expect_err("placeholder receipt must fail closed");
        assert!(error
            .to_string()
            .contains("finite-query verification report"));
        assert!(!inputs.store_dir.exists());
    }

    #[test]
    fn echoed_checker_hash_cannot_forge_approved_verifier_provenance() {
        let temp = tempdir().unwrap();
        let inputs = fixture_inputs(temp.path(), temp.path().join("store"));
        for path in [
            &inputs.baseline_query_verification,
            &inputs.candidate_query_verification,
        ] {
            let mut report: serde_json::Value =
                serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
            report["verifier_receipt_v2"]["checker_sha256"] = serde_json::Value::String(
                "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            );
            fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        }
        let error = run_workflow(&inputs).expect_err("echoed checker identity must fail closed");
        assert!(error.to_string().contains("finite-query trust gate"));
        assert!(!inputs.store_dir.exists());
    }

    #[test]
    fn existing_store_directory_is_never_overwritten() {
        let temp = tempdir().unwrap();
        let store = temp.path().join("store");
        fs::create_dir(&store).unwrap();
        let inputs = fixture_inputs(temp.path(), store);
        let error = run_workflow(&inputs).expect_err("existing store must fail closed");
        assert!(error.to_string().contains("must not already exist"));
    }
}
