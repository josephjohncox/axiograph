//! Proposals validation (preview import + type/constraint checks).
//!
//! This is for "LLM/UI add-data" flows:
//! - proposals are untrusted and do not mutate the DB directly,
//! - but we still want fast feedback when a proposal would be ill-typed or
//!   violate structured theory constraints (key/functional, etc).
//!
//! The strategy is:
//! 1) clone the current snapshot,
//! 2) import the proposals overlay into the clone (evidence plane),
//! 3) run meta-plane typechecking and tooling-level quality checks,
//! 4) return only *delta* findings introduced by the proposals (baseline vs preview).

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use axiograph_ingest_docs::ProposalsFileV1;
use axiograph_pathdb::axi_semantics::{AxiTypeCheckError, MetaPlaneIndex};
use axiograph_pathdb::{PathDB, ProposalDigest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalAxiTypecheckErrorV1 {
    pub fact: u32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalAxiTypecheckReportV1 {
    pub skipped: bool,
    pub checked_facts: usize,
    pub errors: Vec<ProposalAxiTypecheckErrorV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CompetencyGatePolicyV1 {
    #[serde(default)]
    pub fail_on_regression: bool,
    #[serde(default)]
    pub fail_on_unsatisfied_after: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompetencyQuestionDeltaV1 {
    pub name: String,
    pub min_rows: usize,
    pub weight: f64,
    pub before_rows: usize,
    pub after_rows: usize,
    pub before_satisfied: bool,
    pub after_satisfied: bool,
    pub regression: bool,
    pub improvement: bool,
    pub before_trust_class: String,
    pub after_trust_class: String,
    #[serde(default)]
    pub trust_changed: bool,
    #[serde(default)]
    pub before_trust_reasons: Vec<String>,
    #[serde(default)]
    pub after_trust_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompetencyGateReportV1 {
    pub total: usize,
    pub satisfied_before: usize,
    pub satisfied_after: usize,
    pub coverage_before: f64,
    pub coverage_after: f64,
    pub cost_before: f64,
    pub cost_after: f64,
    pub regressions: usize,
    pub improvements: usize,
    pub gate_passed: bool,
    pub policy: CompetencyGatePolicyV1,
    #[serde(default)]
    pub questions: Vec<CompetencyQuestionDeltaV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProposalValidationTrustContractV1 {
    pub trust_class: String,
    pub soundness: String,
    pub coverage: String,
    pub scope: String,
    #[serde(default)]
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProposalsValidationOptionsV1 {
    pub quality_profile: String,
    pub quality_plane: String,
    #[serde(default)]
    pub competency_questions: Vec<crate::predictive_proposals::CompetencyQuestionV1>,
    #[serde(default)]
    pub competency_gate: CompetencyGatePolicyV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalsValidationV1 {
    pub version: String,
    pub import_summary: crate::proposals_import::ImportProposalsSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evolution_preview: Option<crate::evolution_preview::EvolutionPreviewV1>,
    pub axi_typecheck: ProposalAxiTypecheckReportV1,
    pub quality_delta: crate::quality::QualityReportV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub competency_gate: Option<CompetencyGateReportV1>,
    pub trust: ProposalValidationTrustContractV1,
    pub ok: bool,
}

fn proposals_digest(file: &ProposalsFileV1) -> Result<ProposalDigest> {
    let bytes = serde_json::to_vec(file)
        .map_err(|e| anyhow!("failed to serialize proposals for digest: {e}"))?;
    Ok(ProposalDigest::new(
        axiograph_kernel::object_blob_digest_v2(&bytes),
    ))
}

fn clone_db(db: &PathDB) -> Result<PathDB> {
    db.detached_clone()
}

fn typecheck_preview(db: &PathDB, meta: &MetaPlaneIndex) -> ProposalAxiTypecheckReportV1 {
    if meta.schemas.is_empty() {
        return ProposalAxiTypecheckReportV1 {
            skipped: true,
            checked_facts: 0,
            errors: Vec::new(),
        };
    }

    let report = meta.typecheck_axi_facts(db);
    let errors = report
        .errors
        .into_iter()
        .filter_map(|e| {
            let fact = match &e {
                AxiTypeCheckError::MissingSchema { fact }
                | AxiTypeCheckError::UnknownSchema { fact, .. }
                | AxiTypeCheckError::UnknownRelation { fact, .. }
                | AxiTypeCheckError::MissingField { fact, .. }
                | AxiTypeCheckError::MultipleFieldValues { fact, .. }
                | AxiTypeCheckError::MissingEntityType { fact, .. }
                | AxiTypeCheckError::FieldTypeMismatch { fact, .. } => *fact,
            };

            // Only surface errors for evidence-plane fact nodes produced by proposals.
            let is_proposal_fact = db
                .get_entity(fact)
                .map(|v| v.attrs.contains_key("proposal_id"))
                .unwrap_or(false);
            if !is_proposal_fact {
                return None;
            }

            Some(ProposalAxiTypecheckErrorV1 {
                fact,
                message: e.to_string(),
            })
        })
        .collect::<Vec<_>>();

    ProposalAxiTypecheckReportV1 {
        skipped: false,
        checked_facts: report.checked_facts,
        errors,
    }
}

pub fn quality_delta_report_v1(
    base: &PathDB,
    preview: &PathDB,
    profile: &str,
    plane: &str,
) -> Result<crate::quality::QualityReportV1> {
    let profile = profile.trim().to_ascii_lowercase();
    let plane = plane.trim().to_ascii_lowercase();

    let base_report = crate::quality::run_quality_checks(
        base,
        &PathBuf::from("proposals_validate:base"),
        &profile,
        &plane,
    )?;

    let preview_report = crate::quality::run_quality_checks(
        preview,
        &PathBuf::from("proposals_validate:preview"),
        &profile,
        &plane,
    )?;

    fn key(f: &crate::quality::QualityFindingV1) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}",
            f.level,
            f.code,
            f.message,
            f.schema.as_deref().unwrap_or(""),
            f.relation.as_deref().unwrap_or(""),
            f.entity_id.unwrap_or(0),
        )
    }

    let base_keys: HashSet<String> = base_report.findings.iter().map(key).collect();
    let mut delta = preview_report.clone();
    delta.input = "proposals_validate:delta".to_string();
    delta.findings = preview_report
        .findings
        .into_iter()
        .filter(|f| !base_keys.contains(&key(f)))
        .collect();

    // Recompute summary for the delta-only findings.
    let mut summary = crate::quality::QualitySummaryV1::default();
    for f in &delta.findings {
        match f.level.as_str() {
            "error" => summary.error_count += 1,
            "warning" => summary.warning_count += 1,
            "info" => summary.info_count += 1,
            _ => {}
        }
    }
    delta.summary = summary;

    Ok(delta)
}

pub fn competency_gate_report_v1(
    base: &PathDB,
    preview: &PathDB,
    questions: &[crate::predictive_proposals::CompetencyQuestionV1],
    policy: &CompetencyGatePolicyV1,
) -> Result<CompetencyGateReportV1> {
    let before =
        crate::competency_questions::evaluate_competency_questions_with_trust(base, questions)?;
    let after =
        crate::competency_questions::evaluate_competency_questions_with_trust(preview, questions)?;

    let mut regressions = 0usize;
    let mut improvements = 0usize;
    let mut deltas = Vec::with_capacity(after.questions.len());

    for (before_q, after_q) in before.questions.iter().zip(after.questions.iter()) {
        let regression = before_q.satisfied && !after_q.satisfied;
        let improvement = !before_q.satisfied && after_q.satisfied;
        if regression {
            regressions += 1;
        }
        if improvement {
            improvements += 1;
        }
        deltas.push(CompetencyQuestionDeltaV1 {
            name: after_q.name.clone(),
            min_rows: after_q.min_rows,
            weight: after_q.weight,
            before_rows: before_q.rows,
            after_rows: after_q.rows,
            before_satisfied: before_q.satisfied,
            after_satisfied: after_q.satisfied,
            regression,
            improvement,
            before_trust_class: before_q.trust.trust_class.clone(),
            after_trust_class: after_q.trust.trust_class.clone(),
            trust_changed: before_q.trust.trust_class != after_q.trust.trust_class
                || before_q.trust.reasons != after_q.trust.reasons,
            before_trust_reasons: before_q.trust.reasons.clone(),
            after_trust_reasons: after_q.trust.reasons.clone(),
        });
    }

    let unsatisfied_after = after.total.saturating_sub(after.satisfied);
    let gate_passed = (!policy.fail_on_regression || regressions == 0)
        && (!policy.fail_on_unsatisfied_after || unsatisfied_after == 0);

    Ok(CompetencyGateReportV1 {
        total: after.total,
        satisfied_before: before.satisfied,
        satisfied_after: after.satisfied,
        coverage_before: before.coverage,
        coverage_after: after.coverage,
        cost_before: before.cost,
        cost_after: after.cost,
        regressions,
        improvements,
        gate_passed,
        policy: policy.clone(),
        questions: deltas,
    })
}

fn preview_trust_contract(
    competency_gate: Option<&CompetencyGateReportV1>,
) -> ProposalValidationTrustContractV1 {
    let mut reasons = vec![
        "preview is scoped to the current snapshot plus imported proposal delta".to_string(),
        "soundness covers structural/type/quality gates only, not full ontology closure"
            .to_string(),
    ];
    let coverage = if let Some(cq) = competency_gate {
        reasons.push(format!(
            "competency coverage compared {} question(s) before and after the preview delta",
            cq.total
        ));
        "proposal_delta_plus_competency_questions".to_string()
    } else {
        "proposal_delta_only".to_string()
    };

    ProposalValidationTrustContractV1 {
        trust_class: "preview_validated".to_string(),
        soundness: "preview_typechecked_and_quality_gated".to_string(),
        coverage,
        scope: "snapshot_scoped_preview".to_string(),
        reasons,
    }
}

pub fn validate_proposals_v1(
    base: &PathDB,
    proposals: &ProposalsFileV1,
    quality_profile: &str,
    quality_plane: &str,
) -> Result<ProposalsValidationV1> {
    validate_proposals_with_options_v1(
        base,
        proposals,
        &ProposalsValidationOptionsV1 {
            quality_profile: quality_profile.to_string(),
            quality_plane: quality_plane.to_string(),
            ..ProposalsValidationOptionsV1::default()
        },
    )
}

pub fn validate_proposals_with_options_v1(
    base: &PathDB,
    proposals: &ProposalsFileV1,
    options: &ProposalsValidationOptionsV1,
) -> Result<ProposalsValidationV1> {
    let digest = proposals_digest(proposals)?;

    let mut preview = clone_db(base)?;
    let import_summary = crate::proposals_import::import_proposals_file_into_pathdb(
        &mut preview,
        proposals,
        digest.as_str(),
    )?;

    let preview_meta = MetaPlaneIndex::from_db(&preview).unwrap_or_default();
    let axi_typecheck = typecheck_preview(&preview, &preview_meta);
    let quality_profile = if options.quality_profile.trim().is_empty() {
        "fast"
    } else {
        options.quality_profile.as_str()
    };
    let quality_plane = if options.quality_plane.trim().is_empty() {
        "both"
    } else {
        options.quality_plane.as_str()
    };
    let quality_delta = quality_delta_report_v1(base, &preview, quality_profile, quality_plane)?;
    let competency_gate = if options.competency_questions.is_empty() {
        None
    } else {
        Some(competency_gate_report_v1(
            base,
            &preview,
            &options.competency_questions,
            &options.competency_gate,
        )?)
    };

    let ok = quality_delta.summary.error_count == 0
        && (axi_typecheck.skipped || axi_typecheck.errors.is_empty());
    let ok = ok
        && competency_gate
            .as_ref()
            .map(|gate| gate.gate_passed)
            .unwrap_or(true);

    let trust = preview_trust_contract(competency_gate.as_ref());
    let runtime_semantics = crate::semantic_claim::runtime_semantic_summary_for_preview(
        &preview_meta,
        &trust,
        competency_gate.as_ref(),
        quality_delta.summary.error_count,
    );
    let evolution_preview = crate::evolution_preview::build_evolution_preview_v1(
        "proposal_review",
        None,
        digest.to_string(),
        crate::evolution_preview::proposal_typed_change_summary(&digest, &import_summary),
        &quality_delta,
        competency_gate.as_ref(),
        &trust,
        Some(runtime_semantics),
        axi_typecheck
            .errors
            .iter()
            .map(|err| format!("typecheck error on fact {}: {}", err.fact, err.message)),
        ok,
    );

    Ok(ProposalsValidationV1 {
        version: "proposals_validation_v1".to_string(),
        import_summary,
        evolution_preview: Some(evolution_preview),
        axi_typecheck,
        quality_delta,
        competency_gate: competency_gate.clone(),
        trust,
        ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_proposed_parent_fact_is_well_typed_and_has_no_new_errors() -> Result<()> {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
        let base = crate::load_pathdb_for_cli(&repo_root.join("examples/Family.axi"))?;
        let out = crate::proposal_gen::propose_relation_proposals_v1(
            &base,
            &[],
            crate::proposal_gen::ProposeRelationInputV1 {
                rel_type: "Parent".to_string(),
                source_name: "Jamison".to_string(),
                target_name: "Bob".to_string(),
                source_type: None,
                target_type: None,
                source_field: None,
                target_field: None,
                context: Some("FamilyTree".to_string()),
                time: None,
                confidence: Some(0.9),
                schema_hint: Some("Fam".to_string()),
                public_rationale: Some("Jamison is a son of Bob (FamilyTree).".to_string()),
                evidence_text: None,
                evidence_locator: None,
                extra_fields: std::collections::HashMap::new(),
            },
        )?;
        assert_eq!(out.summary.rel_type, "Parent");
        assert!(!out.summary.swapped_endpoints);

        let validation = validate_proposals_v1(&base, &out.proposals, "fast", "both")?;
        assert!(
            !validation.axi_typecheck.skipped,
            "expected meta-plane typecheck"
        );
        assert!(
            validation.axi_typecheck.errors.is_empty(),
            "unexpected typecheck errors: {:?}",
            validation.axi_typecheck.errors
        );
        assert_eq!(
            validation.quality_delta.summary.error_count, 0,
            "unexpected new quality errors: {:?}",
            validation.quality_delta.findings
        );
        Ok(())
    }

    #[test]
    fn propose_fact_proposals_sets_endpoints_by_field_name() -> Result<()> {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
        let base = crate::load_pathdb_for_cli(&repo_root.join("examples/Family.axi"))?;

        let out = crate::proposal_gen::propose_fact_proposals_v1(
            &base,
            &[],
            crate::proposal_gen::ProposeFactInputV1 {
                rel_type: "Fam.Parent".to_string(),
                fields: std::collections::HashMap::from([
                    ("child".to_string(), "Jamison".to_string()),
                    ("parent".to_string(), "Bob".to_string()),
                    ("ctx".to_string(), "FamilyTree".to_string()),
                ]),
                schema_hint: None,
                confidence: Some(0.9),
                public_rationale: Some("Jamison is a son of Bob (FamilyTree).".to_string()),
                evidence_text: None,
                evidence_locator: None,
            },
        )?;

        // Import the overlay into a clone and assert the derived traversal edge direction.
        let mut preview = base.detached_clone()?;
        let digest_bytes = serde_json::to_vec(&out.proposals)?;
        let digest = axiograph_kernel::object_blob_digest_v2(&digest_bytes);
        crate::proposals_import::import_proposals_file_into_pathdb(
            &mut preview,
            &out.proposals,
            &digest,
        )?;

        fn find_named_entity(db: &PathDB, type_name: &str, name: &str) -> Option<u32> {
            let name = name.trim();
            let name_key = db.interner.id_of("name")?;
            let val_id = db.interner.id_of(name)?;
            let ids = db.entities.entities_with_attr_value(name_key, val_id);
            for id in ids.iter() {
                if let Some(v) = db.get_entity(id) {
                    if v.entity_type == type_name {
                        return Some(id);
                    }
                }
            }
            None
        }

        let jamison = find_named_entity(&preview, "Person", "Jamison")
            .ok_or_else(|| anyhow!("expected Jamison Person entity after import"))?;
        let bob = find_named_entity(&preview, "Person", "Bob")
            .ok_or_else(|| anyhow!("expected Bob Person entity"))?;

        let rel_id = preview
            .interner
            .id_of("Parent")
            .ok_or_else(|| anyhow!("expected interned Parent relation id"))?;
        assert!(
            preview.relations.has_edge(jamison, rel_id, bob),
            "expected Jamison -Parent-> Bob derived edge"
        );
        assert!(
            !preview.relations.has_edge(bob, rel_id, jamison),
            "unexpected Bob -Parent-> Jamison derived edge"
        );

        Ok(())
    }

    #[test]
    fn propose_relation_proposals_requires_extra_fields_for_nary_relations() -> Result<()> {
        let mut base = axiograph_pathdb::PathDB::new();
        let axi = r#"
module EconDemo

schema Econ:
  object Person
  object Amount
  object Context
  object Time
  relation Flow(from: Person, to: Person, amount: Amount, ctx: Context, time: Time)

instance EconInst of Econ:
  Person = {Alice, Bob}
  Amount = {USD_100}
  Context = {Observed}
  Time = {T2020}
"#;
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut base, axi)?;
        base.build_indexes();

        // Missing required `amount` should fail in proposal generation.
        let err = crate::proposal_gen::propose_relation_proposals_v1(
            &base,
            &[],
            crate::proposal_gen::ProposeRelationInputV1 {
                rel_type: "Flow".to_string(),
                source_name: "Alice".to_string(),
                target_name: "Bob".to_string(),
                source_type: None,
                target_type: None,
                source_field: None,
                target_field: None,
                context: None,
                time: None,
                confidence: Some(0.9),
                schema_hint: Some("Econ".to_string()),
                public_rationale: None,
                evidence_text: None,
                evidence_locator: None,
                extra_fields: std::collections::HashMap::new(),
            },
        )
        .err()
        .ok_or_else(|| anyhow!("expected propose_relation_proposals_v1 to fail without amount"))?;
        assert!(
            err.to_string()
                .contains("requires additional field `amount`"),
            "unexpected error: {err}"
        );

        // With extra_fields, it should validate and typecheck cleanly.
        let out = crate::proposal_gen::propose_relation_proposals_v1(
            &base,
            &[],
            crate::proposal_gen::ProposeRelationInputV1 {
                rel_type: "Flow".to_string(),
                source_name: "Alice".to_string(),
                target_name: "Bob".to_string(),
                source_type: None,
                target_type: None,
                source_field: None,
                target_field: None,
                context: Some("Observed".to_string()),
                time: Some("T2020".to_string()),
                confidence: Some(0.9),
                schema_hint: Some("Econ".to_string()),
                public_rationale: Some("Alice sends flow to Bob in 2020.".to_string()),
                evidence_text: None,
                evidence_locator: None,
                extra_fields: std::collections::HashMap::from([(
                    "amount".to_string(),
                    "USD_100".to_string(),
                )]),
            },
        )?;

        let validation = validate_proposals_v1(&base, &out.proposals, "fast", "both")?;
        assert!(
            validation.axi_typecheck.errors.is_empty(),
            "unexpected typecheck errors: {:?}",
            validation.axi_typecheck.errors
        );
        assert_eq!(
            validation.quality_delta.summary.error_count, 0,
            "unexpected new quality errors: {:?}",
            validation.quality_delta.findings
        );

        Ok(())
    }

    #[test]
    fn validate_multi_schema_endpoint_resolution_prefers_schema_hint() -> Result<()> {
        // Regression test:
        // When multiple schemas share the same object type + element names,
        // schema-directed proposals import must resolve endpoints *within the
        // hinted schema* (not “first by id”).
        let mut base = axiograph_pathdb::PathDB::new();
        let axi = r#"
module Universe

schema Census:
  object Person
  relation Parent(child: Person, parent: Person)

schema Fam:
  object Person
  relation Parent(child: Person, parent: Person)

instance CensusInst of Census:
  Person = {Alice, Bob, Dan}
  Parent = {(child=Dan, parent=Alice), (child=Dan, parent=Bob)}

instance FamInst of Fam:
  Person = {Alice, Bob, Carol}
  Parent = {(child=Carol, parent=Alice), (child=Carol, parent=Bob)}
"#;
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut base, axi)?;
        base.build_indexes();

        let out = crate::proposal_gen::propose_relation_proposals_v1(
            &base,
            &[],
            crate::proposal_gen::ProposeRelationInputV1 {
                rel_type: "Parent".to_string(),
                source_name: "Carol".to_string(),
                target_name: "Bob".to_string(),
                source_type: None,
                target_type: None,
                source_field: None,
                target_field: None,
                context: None,
                time: None,
                confidence: Some(0.9),
                schema_hint: Some("Fam".to_string()),
                public_rationale: Some("Carol has parent Bob (Fam).".to_string()),
                evidence_text: None,
                evidence_locator: None,
                extra_fields: std::collections::HashMap::new(),
            },
        )?;

        let validation = validate_proposals_v1(&base, &out.proposals, "fast", "both")?;
        assert!(
            validation.axi_typecheck.errors.is_empty(),
            "unexpected typecheck errors: {:?}",
            validation.axi_typecheck.errors
        );
        assert_eq!(
            validation.quality_delta.summary.error_count, 0,
            "unexpected new quality errors: {:?}",
            validation.quality_delta.findings
        );
        Ok(())
    }

    #[test]
    fn validate_proposals_with_competency_gate_reports_trust_and_unsatisfied_after() -> Result<()> {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
        let base = crate::load_pathdb_for_cli(&repo_root.join("examples/Family.axi"))?;
        let out = crate::proposal_gen::propose_relation_proposals_v1(
            &base,
            &[],
            crate::proposal_gen::ProposeRelationInputV1 {
                rel_type: "Parent".to_string(),
                source_name: "Jamison".to_string(),
                target_name: "Bob".to_string(),
                source_type: None,
                target_type: None,
                source_field: None,
                target_field: None,
                context: Some("FamilyTree".to_string()),
                time: Some("T2025".to_string()),
                confidence: Some(0.9),
                schema_hint: Some("Fam".to_string()),
                public_rationale: Some("Jamison is a child of Bob (FamilyTree).".to_string()),
                evidence_text: None,
                evidence_locator: None,
                extra_fields: std::collections::HashMap::new(),
            },
        )?;

        let validation = validate_proposals_with_options_v1(
            &base,
            &out.proposals,
            &ProposalsValidationOptionsV1 {
                quality_profile: "fast".to_string(),
                quality_plane: "both".to_string(),
                competency_questions: vec![
                    crate::predictive_proposals::CompetencyQuestionV1 {
                        name: "jamison_parent".to_string(),
                        question: Some("Jamison should have Bob as a parent".to_string()),
                        authoring: None,
                        query: "select ?f where ?f = Fam.Parent(child=Jamison, parent=Bob, ctx=FamilyTree, time=?t) limit 1".to_string(),
                        min_rows: 1,
                        weight: 1.0,
                        contexts: Vec::new(),
                    },
                    crate::predictive_proposals::CompetencyQuestionV1 {
                        name: "jamison_spouse".to_string(),
                        question: Some("Jamison should not yet have a spouse fact".to_string()),
                        authoring: None,
                        query: "select ?f where ?f = Fam.Spouse(a=Jamison, b=Bob, ctx=FamilyTree) limit 1".to_string(),
                        min_rows: 1,
                        weight: 2.0,
                        contexts: Vec::new(),
                    },
                ],
                competency_gate: CompetencyGatePolicyV1 {
                    fail_on_regression: true,
                    fail_on_unsatisfied_after: true,
                },
            },
        )?;

        assert!(
            !validation.ok,
            "expected unsatisfied-after competency gate to fail preview"
        );
        assert_eq!(validation.trust.trust_class, "preview_validated");
        assert_eq!(
            validation.trust.coverage,
            "proposal_delta_plus_competency_questions"
        );

        let gate = validation
            .competency_gate
            .as_ref()
            .ok_or_else(|| anyhow!("expected competency gate report"))?;
        assert_eq!(gate.total, 2);
        assert_eq!(gate.improvements, 1);
        assert_eq!(gate.regressions, 0);
        assert_eq!(gate.satisfied_after, 1);
        assert!(!gate.gate_passed);
        let evolution = validation
            .evolution_preview
            .as_ref()
            .ok_or_else(|| anyhow!("expected shared evolution preview"))?;
        assert_eq!(evolution.kind, "proposal_review");
        assert_eq!(evolution.typed_change.kind, "proposal_delta");
        assert_eq!(evolution.typed_change.schema.added, 0);
        assert_eq!(evolution.typed_change.theory.added, 0);
        assert_eq!(
            evolution
                .runtime_semantics
                .as_ref()
                .ok_or_else(|| anyhow!("expected runtime semantic summary"))?
                .completeness_claim,
            "not_claimed"
        );
        assert_eq!(evolution.trust_delta.competency_coverage_before, Some(0.0));
        assert_eq!(evolution.trust_delta.competency_coverage_after, Some(0.5));
        assert_eq!(evolution.trust_delta.improvements, 1);
        assert_eq!(evolution.trust_delta.regressions, 0);
        assert_eq!(evolution.trust_delta.changed_questions, 0);
        assert_eq!(evolution.trust_delta.questions.len(), 2);
        assert!(
            evolution
                .residual_obligations
                .iter()
                .any(|item| item.contains("competency gate remains unsatisfied after preview")),
            "expected CQ gate residual obligation: {:?}",
            evolution.residual_obligations
        );
        assert!(
            gate.questions
                .iter()
                .all(|q| q.after_trust_class == "certifiable"),
            "expected competency queries to stay in the certifiable subset: {:?}",
            gate.questions
        );
        assert!(
            gate.questions.iter().all(|q| !q.trust_changed),
            "proposal preview should preserve CQ trust class for this regression-free case: {:?}",
            gate.questions
        );
        Ok(())
    }
}
