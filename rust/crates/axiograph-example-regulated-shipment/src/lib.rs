//! End-to-end regulated-shipment usefulness fixture.
//!
//! The crate deliberately uses the production canonical compiler, AxiStore
//! promotion/merge APIs, SQLite materializer, and verified PathDB hydration.
//! It does not define ontology semantics. Rust remains an untrusted producer;
//! the supplied verification-receipt bytes must come from the separately built
//! `Axiograph.VerifyMain` executable when this is run through the documented
//! workflow.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use axiograph_kernel::{
    CanonicalCompiler, CanonicalModuleSource, CommitIdV2, CompiledKernelSnapshot,
    KernelCompilationRequest, ObjectBlobIdV2, RepositoryIdV2, SchemaGeneratorKindIr, ScopeAxisIr,
    SnapshotIdV2, TypeExprIr,
};
use axiograph_pathdb::materialization::load_verified_pathdb;
use axiograph_store::*;
use serde::{Deserialize, Serialize};

pub const REGULATED_SHIPMENT_USEFULNESS_REPORT_VERSION: &str =
    "regulated_shipment_usefulness_report_v2";
const MODULE_NAME: &str = "RegulatedShipment";

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
    pub context_witnesses: usize,
    pub world_witnesses: usize,
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
pub struct RegulatedShipmentUsefulnessReport {
    pub version: String,
    pub scenario: String,
    pub repository_id: String,
    pub baseline_snapshot_id: String,
    pub accepted_snapshot_id: String,
    pub kernel_ir_digest: String,
    pub canonical_revision_digest: String,
    pub category: RegulatedShipmentCategoryEvidence,
    pub merge: RegulatedShipmentMergeEvidence,
    pub persistence: RegulatedShipmentPersistenceEvidence,
    pub trusted_receipt_inputs: Vec<String>,
    pub checked_runtime_scope: Vec<String>,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone)]
struct EvidenceBytes {
    authoring: Vec<u8>,
    theory: Vec<u8>,
    verification: Vec<u8>,
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

fn evidence(
    authoring: &Path,
    theory: &Path,
    verification: &Path,
    label: &str,
) -> Result<EvidenceBytes> {
    let authoring = read(authoring, &format!("{label} authoring report"))?;
    let theory = read(theory, &format!("{label} theory report"))?;
    let verification = read(verification, &format!("{label} VerifyMain receipt"))?;
    if authoring.is_empty() || theory.is_empty() || verification.is_empty() {
        return Err(anyhow!(
            "{label} evidence inputs must be non-empty authoring, theory, and VerifyMain outputs"
        ));
    }
    let authoring_json: serde_json::Value = axiograph_security::parse_json_bounded(
        &authoring,
        16 * 1024 * 1024,
        "regulated-shipment authoring report",
    )
    .with_context(|| format!("parse {label} authoring report"))?;
    if authoring_json
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
    {
        return Err(anyhow!("{label} authoring report is not successful"));
    }
    let evaluation = &authoring_json["competency_questions"]["evaluation"];
    let satisfied = evaluation
        .get("satisfied")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| anyhow!("{label} authoring report omits CQ satisfied count"))?;
    let total = evaluation
        .get("total")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| anyhow!("{label} authoring report omits CQ total"))?;
    if total == 0 || satisfied != total {
        return Err(anyhow!(
            "{label} authoring report does not satisfy every declared CQ ({satisfied}/{total})"
        ));
    }
    let theory_json: serde_json::Value = axiograph_security::parse_json_bounded(
        &theory,
        16 * 1024 * 1024,
        "regulated-shipment runtime-theory report",
    )
    .with_context(|| format!("parse {label} runtime-theory report"))?;
    if theory_json
        .get("blocking_errors")
        .and_then(serde_json::Value::as_u64)
        != Some(0)
    {
        return Err(anyhow!("{label} runtime-theory report has blocking errors"));
    }
    if theory_json
        .get("completeness_claim")
        .and_then(serde_json::Value::as_str)
        != Some("not_claimed_runtime_admissibility_only")
    {
        return Err(anyhow!(
            "{label} runtime-theory report omitted the required completeness non-claim"
        ));
    }
    Ok(EvidenceBytes {
        authoring,
        theory,
        verification,
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

fn blob(objects: &[ImmutableBlob], kind: ImmutableObjectKind) -> ObjectBlobIdV2 {
    objects
        .iter()
        .find(|object| object.kind == kind)
        .expect("required supporting object")
        .digest
        .clone()
}

fn typed_candidate(plan: &ScenarioPlan) -> Result<TypedCandidatePayloadV2> {
    Ok(TypedCandidatePayloadV2::checked(
        plan.promotion.snapshot.snapshot_id.clone(),
        plan.promotion.tree.tree_id.clone(),
        plan.compiled.ir().root_module_id().clone(),
        plan.promotion.manifest.kernel_ir_digest.clone(),
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
            evidence.verification.clone(),
        )?,
    ];
    let validation = blob(&objects, ImmutableObjectKind::ValidationReport);
    let competency = blob(&objects, ImmutableObjectKind::CompetencyQuestionReport);
    let theory = blob(&objects, ImmutableObjectKind::TheoryReport);
    let verification = blob(&objects, ImmutableObjectKind::VerificationReceipt);
    let manifest = AcceptedBuildManifest {
        format: BUILD_MANIFEST_FORMAT.to_string(),
        version: FORMAT_VERSION,
        repository_id: repository_id.clone(),
        accepted_tree_id: tree.tree_id.clone(),
        accepted_snapshot_id: snapshot.snapshot_id.clone(),
        ordered_module_closure: vec![publication.module.revision_digest.clone()],
        compiler_version: "axiograph-kernel-canonical-compiler".to_string(),
        ir_version: "kernel-snapshot-ir-v2".to_string(),
        kernel_ir_digest: blob(&objects, ImmutableObjectKind::KernelIr),
        canonical_fact_log_digest: blob(&objects, ImmutableObjectKind::CanonicalFactLog),
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

fn category_evidence(compiled: &CompiledKernelSnapshot) -> RegulatedShipmentCategoryEvidence {
    let ir = compiled.ir();
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
    RegulatedShipmentCategoryEvidence {
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
        context_witnesses,
        world_witnesses,
    }
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
        "baseline",
    )?;
    let candidate_evidence = evidence(
        &inputs.candidate_authoring_report,
        &inputs.candidate_theory_report,
        &inputs.candidate_verification_receipt,
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
    let shipment_present = image
        .entities
        .iter()
        .any(|entity| entity.value == "Shipment_RX_1007");
    if !shipment_present {
        return Err(anyhow!("verified restart image omitted Shipment_RX_1007"));
    }
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
        category: category_evidence(&merge.compiled),
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
            shipment_rx_1007_present_after_restart: shipment_present,
        },
        trusted_receipt_inputs: vec![
            inputs
                .baseline_verification_receipt
                .display()
                .to_string(),
            inputs
                .candidate_verification_receipt
                .display()
                .to_string(),
        ],
        checked_runtime_scope: vec![
            "canonical compiler over exact baseline and candidate bytes".to_string(),
            "finite relation objects, ordered role projections, indexed/refined role witnesses, checked formal groupoid equations, and explanation-certified saturation".to_string(),
            "AxiStore exact-two-parent reviewed merge and authenticated SQLite materialization".to_string(),
            "receipt-checked PathDB hydration after process-level reopen".to_string(),
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
    use axiograph_projections::{
        check_readback_v1, manifest_readback_fixture_v1, project_snapshot_v1, ProjectionBackendV1,
        ReadbackTransportStatusV1,
    };
    use std::fs;
    use std::io::Write;
    use tempfile::tempdir;

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
        let authoring_report = serde_json::json!({
            "ok": true,
            "competency_questions": {
                "evaluation": {"satisfied": 3, "total": 3}
            },
            "scope": "test fixture"
        });
        let baseline_authoring = write_fixture("baseline-authoring.json", authoring_report.clone());
        let candidate_authoring = write_fixture("candidate-authoring.json", authoring_report);
        let theory_report = serde_json::json!({
            "blocking_errors": 0,
            "completeness_claim": "not_claimed_runtime_admissibility_only",
            "scope": "runtime only"
        });
        let baseline_theory = write_fixture("baseline-theory.json", theory_report.clone());
        let candidate_theory = write_fixture("candidate-theory.json", theory_report);
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
                     ok: category_kernel_v3 schema=RegulatedShipment objects=1 arrows=1 equations=0 congruence=0 reachability=1 lifecycle=explanationVerified\n"
                ),
            )
            .unwrap();
            path
        };
        let baseline_verification = write_receipt("baseline-verification.txt", &baseline);
        let candidate_verification = write_receipt("candidate-verification.txt", &candidate);
        RegulatedShipmentWorkflowInputs {
            baseline_axi: baseline,
            candidate_axi: candidate,
            baseline_authoring_report: baseline_authoring,
            candidate_authoring_report: candidate_authoring,
            baseline_theory_report: baseline_theory,
            candidate_theory_report: candidate_theory,
            baseline_verification_receipt: baseline_verification,
            candidate_verification_receipt: candidate_verification,
            store_dir,
        }
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
        assert!(report.category.context_witnesses > 0);
        assert!(report.persistence.entity_rows > 0);
        assert!(report.persistence.relation_fact_rows > 0);
        assert!(report.persistence.shipment_rx_1007_present_after_restart);
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
    fn existing_store_directory_is_never_overwritten() {
        let temp = tempdir().unwrap();
        let store = temp.path().join("store");
        fs::create_dir(&store).unwrap();
        let inputs = fixture_inputs(temp.path(), store);
        let error = run_workflow(&inputs).expect_err("existing store must fail closed");
        assert!(error.to_string().contains("must not already exist"));
    }
}
