use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use axiograph_pathdb::{AcceptedSnapshotId, AxiDigest, ProposalDigest};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TypedChangeBucketV1 {
    #[serde(default)]
    pub added: usize,
    #[serde(default)]
    pub reused: usize,
    #[serde(default)]
    pub removed: usize,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TypedChangeSummaryV1 {
    pub kind: String,
    #[serde(default)]
    pub subjects: Vec<String>,
    #[serde(default)]
    pub counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub schema: TypedChangeBucketV1,
    #[serde(default)]
    pub theory: TypedChangeBucketV1,
    #[serde(default)]
    pub instance: TypedChangeBucketV1,
    #[serde(default)]
    pub context: TypedChangeBucketV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrustDeltaQuestionV1 {
    pub name: String,
    pub before_trust_class: String,
    pub after_trust_class: String,
    #[serde(default)]
    pub trust_changed: bool,
    #[serde(default)]
    pub before_trust_reasons: Vec<String>,
    #[serde(default)]
    pub after_trust_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemTrustSummaryV1 {
    pub trust_class: String,
    pub soundness: String,
    pub coverage: String,
    pub scope: String,
    pub completeness_claim: String,
    pub ontology_closure_claim: String,
    #[serde(default)]
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemCompetencySummaryV1 {
    pub total: usize,
    pub satisfied_before: usize,
    pub satisfied_after: usize,
    pub regressions: usize,
    pub improvements: usize,
    pub changed_questions: usize,
    pub gate_passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemRuleSummaryV1 {
    pub constraint_count: u32,
    pub instance_count: u32,
    pub check_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemGateSummaryV1 {
    pub kind: String,
    pub candidate_label: String,
    pub ok: bool,
    pub trust: SemTrustSummaryV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub competency: Option<SemCompetencySummaryV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_semantics: Option<crate::semantic_claim::RuntimeSemanticSummaryV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<SemRuleSummaryV1>,
    #[serde(default)]
    pub residual_obligation_count: usize,
}

impl SemGateSummaryV1 {
    pub fn with_rule_summary(mut self, rule: SemRuleSummaryV1) -> Self {
        self.rule = Some(rule);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrustDeltaV1 {
    pub preview_trust_class: String,
    pub preview_soundness: String,
    pub preview_coverage: String,
    pub preview_scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub competency_coverage_before: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub competency_coverage_after: Option<f64>,
    #[serde(default)]
    pub regressions: usize,
    #[serde(default)]
    pub improvements: usize,
    #[serde(default)]
    pub changed_questions: usize,
    #[serde(default)]
    pub questions: Vec<TrustDeltaQuestionV1>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionPreviewV1 {
    pub version: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_snapshot_id: Option<AcceptedSnapshotId>,
    pub candidate_label: String,
    pub typed_change: TypedChangeSummaryV1,
    pub quality_delta: crate::quality::QualityReportV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub competency_gate: Option<crate::proposals_validate::CompetencyGateReportV1>,
    pub trust: crate::proposals_validate::ProposalValidationTrustContractV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_semantics: Option<crate::semantic_claim::RuntimeSemanticSummaryV1>,
    #[serde(default)]
    pub trust_delta: TrustDeltaV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_obligations: Vec<String>,
    pub ok: bool,
}

pub const EVOLUTION_PREVIEW_VERSION_V1: &str = "evolution_preview_v1";

fn trust_delta_from_preview(
    trust: &crate::proposals_validate::ProposalValidationTrustContractV1,
    competency_gate: Option<&crate::proposals_validate::CompetencyGateReportV1>,
) -> TrustDeltaV1 {
    let mut notes = Vec::new();
    let mut questions = Vec::new();
    let mut changed_questions = 0usize;
    let (coverage_before, coverage_after, regressions, improvements) = if let Some(gate) =
        competency_gate
    {
        notes.push(format!(
            "competency coverage changed from {:.3} to {:.3} across {} question(s)",
            gate.coverage_before, gate.coverage_after, gate.total
        ));
        for question in &gate.questions {
            if question.trust_changed {
                changed_questions += 1;
            }
            questions.push(TrustDeltaQuestionV1 {
                name: question.name.clone(),
                before_trust_class: question.before_trust_class.clone(),
                after_trust_class: question.after_trust_class.clone(),
                trust_changed: question.trust_changed,
                before_trust_reasons: question.before_trust_reasons.clone(),
                after_trust_reasons: question.after_trust_reasons.clone(),
            });
        }
        (
            Some(gate.coverage_before),
            Some(gate.coverage_after),
            gate.regressions,
            gate.improvements,
        )
    } else {
        notes.push(
            "no competency questions were configured, so trust delta is limited to the preview trust contract"
                .to_string(),
        );
        (None, None, 0, 0)
    };

    TrustDeltaV1 {
        preview_trust_class: trust.trust_class.clone(),
        preview_soundness: trust.soundness.clone(),
        preview_coverage: trust.coverage.clone(),
        preview_scope: trust.scope.clone(),
        competency_coverage_before: coverage_before,
        competency_coverage_after: coverage_after,
        regressions,
        improvements,
        changed_questions,
        questions,
        notes,
    }
}

pub fn sem_gate_summary_from_evolution_preview(preview: &EvolutionPreviewV1) -> SemGateSummaryV1 {
    SemGateSummaryV1 {
        kind: preview.kind.clone(),
        candidate_label: preview.candidate_label.clone(),
        ok: preview.ok,
        trust: SemTrustSummaryV1 {
            trust_class: preview.trust.trust_class.clone(),
            soundness: preview.trust.soundness.clone(),
            coverage: preview.trust.coverage.clone(),
            scope: preview.trust.scope.clone(),
            completeness_claim: preview
                .runtime_semantics
                .as_ref()
                .map(|summary| summary.completeness_claim.clone())
                .unwrap_or_else(|| "not_claimed".to_string()),
            ontology_closure_claim: preview
                .runtime_semantics
                .as_ref()
                .map(|summary| summary.ontology_closure_claim.clone())
                .unwrap_or_else(|| "not_claimed".to_string()),
            reasons: preview.trust.reasons.clone(),
        },
        competency: preview
            .competency_gate
            .as_ref()
            .map(|gate| SemCompetencySummaryV1 {
                total: gate.total,
                satisfied_before: gate.satisfied_before,
                satisfied_after: gate.satisfied_after,
                regressions: gate.regressions,
                improvements: gate.improvements,
                changed_questions: preview.trust_delta.changed_questions,
                gate_passed: gate.gate_passed,
            }),
        runtime_semantics: preview.runtime_semantics.clone(),
        rule: None,
        residual_obligation_count: preview.residual_obligations.len(),
    }
}

pub fn collect_residual_obligations<I, S>(
    quality_error_count: usize,
    competency_gate: Option<&crate::proposals_validate::CompetencyGateReportV1>,
    extra: I,
) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut obligations = extra.into_iter().map(Into::into).collect::<Vec<_>>();
    if quality_error_count > 0 {
        obligations.push(format!(
            "preview introduced {} quality error(s) that still require author review",
            quality_error_count
        ));
    }
    if let Some(gate) = competency_gate {
        if !gate.gate_passed {
            obligations.push(format!(
                "competency gate remains unsatisfied after preview (regressions={}, satisfied_after={}/{})",
                gate.regressions, gate.satisfied_after, gate.total
            ));
        }
    }
    obligations
}

pub fn build_evolution_preview_v1<I, S>(
    kind: &str,
    base_snapshot_id: Option<AcceptedSnapshotId>,
    candidate_label: String,
    typed_change: TypedChangeSummaryV1,
    quality_delta: &crate::quality::QualityReportV1,
    competency_gate: Option<&crate::proposals_validate::CompetencyGateReportV1>,
    trust: &crate::proposals_validate::ProposalValidationTrustContractV1,
    runtime_semantics: Option<crate::semantic_claim::RuntimeSemanticSummaryV1>,
    extra_residual_obligations: I,
    ok: bool,
) -> EvolutionPreviewV1
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    EvolutionPreviewV1 {
        version: EVOLUTION_PREVIEW_VERSION_V1.to_string(),
        kind: kind.to_string(),
        base_snapshot_id,
        candidate_label,
        typed_change,
        quality_delta: quality_delta.clone(),
        competency_gate: competency_gate.cloned(),
        trust: trust.clone(),
        runtime_semantics,
        trust_delta: trust_delta_from_preview(trust, competency_gate),
        residual_obligations: collect_residual_obligations(
            quality_delta.summary.error_count,
            competency_gate,
            extra_residual_obligations,
        ),
        ok,
    }
}

pub fn proposal_typed_change_summary(
    digest: &ProposalDigest,
    import_summary: &crate::proposals_import::ImportProposalsSummary,
) -> TypedChangeSummaryV1 {
    let mut counts = BTreeMap::new();
    counts.insert(
        "proposals_total".to_string(),
        import_summary.proposals_total,
    );
    counts.insert("entities_added".to_string(), import_summary.entities_added);
    counts.insert(
        "entities_reused".to_string(),
        import_summary.entities_reused,
    );
    counts.insert(
        "relation_facts_added".to_string(),
        import_summary.relation_facts_added,
    );
    counts.insert(
        "relation_facts_reused".to_string(),
        import_summary.relation_facts_reused,
    );
    counts.insert(
        "derived_edges_added".to_string(),
        import_summary.derived_edges_added,
    );
    counts.insert(
        "contexts_created".to_string(),
        import_summary.contexts_created,
    );
    counts.insert(
        "evidence_links_added".to_string(),
        import_summary.evidence_links_added,
    );

    TypedChangeSummaryV1 {
        kind: "proposal_delta".to_string(),
        subjects: vec![digest.to_string()],
        counts,
        notes: vec![
            "delta is still evidence-plane only until review/promotion".to_string(),
            "counts describe proposal import effects, not a full semantic diff".to_string(),
        ],
        schema: TypedChangeBucketV1 {
            notes: vec![
                "proposal review keeps canonical schema artifacts fixed during preview".to_string(),
            ],
            ..TypedChangeBucketV1::default()
        },
        theory: TypedChangeBucketV1 {
            notes: vec![
                "proposal review does not mutate canonical theory artifacts during preview"
                    .to_string(),
            ],
            ..TypedChangeBucketV1::default()
        },
        instance: TypedChangeBucketV1 {
            added: import_summary.entities_added
                + import_summary.relation_facts_added
                + import_summary.derived_edges_added,
            reused: import_summary.entities_reused + import_summary.relation_facts_reused,
            notes: vec![
                "instance delta aggregates proposal entities, fact nodes, and derived traversal edges"
                    .to_string(),
            ],
            ..TypedChangeBucketV1::default()
        },
        context: TypedChangeBucketV1 {
            added: import_summary.contexts_created,
            notes: vec![
                "context delta tracks preview-only context nodes introduced by proposals"
                    .to_string(),
            ],
            ..TypedChangeBucketV1::default()
        },
    }
}

pub fn promotion_typed_change_summary(
    module_name: &str,
    module_digest: &AxiDigest,
    import_summary: &crate::accepted_plane::PromotionImportSummaryV1,
) -> TypedChangeSummaryV1 {
    let mut counts = BTreeMap::new();
    counts.insert(
        "meta_entities_added".to_string(),
        import_summary.meta_entities_added,
    );
    counts.insert(
        "meta_relations_added".to_string(),
        import_summary.meta_relations_added,
    );
    counts.insert(
        "instances_imported".to_string(),
        import_summary.instances_imported,
    );
    counts.insert("entities_added".to_string(), import_summary.entities_added);
    counts.insert(
        "tuple_entities_added".to_string(),
        import_summary.tuple_entities_added,
    );
    counts.insert(
        "relations_added".to_string(),
        import_summary.relations_added,
    );
    counts.insert(
        "derived_edges_added".to_string(),
        import_summary.derived_edges_added,
    );
    counts.insert(
        "entity_type_upgrades".to_string(),
        import_summary.entity_type_upgrades,
    );

    TypedChangeSummaryV1 {
        kind: "accepted_module_delta".to_string(),
        subjects: vec![module_name.to_string(), module_digest.to_string()],
        counts,
        notes: vec![
            "delta compares accepted-snapshot state before and after candidate promotion"
                .to_string(),
            "counts describe import/build effects, not yet a categorical migration witness"
                .to_string(),
        ],
        schema: TypedChangeBucketV1 {
            added: import_summary.meta_entities_added + import_summary.meta_relations_added,
            notes: vec![
                "schema delta is approximated from imported meta-plane entities and relations"
                    .to_string(),
            ],
            ..TypedChangeBucketV1::default()
        },
        theory: TypedChangeBucketV1 {
            notes: vec![
                "theory/context deltas are not yet separated in the current promotion import summary"
                    .to_string(),
            ],
            ..TypedChangeBucketV1::default()
        },
        instance: TypedChangeBucketV1 {
            added: import_summary.entities_added
                + import_summary.tuple_entities_added
                + import_summary.relations_added
                + import_summary.derived_edges_added,
            reused: import_summary.entity_type_upgrades,
            notes: vec![
                "instance delta aggregates imported entities, tuple facts, relation edges, and type upgrades"
                    .to_string(),
            ],
            ..TypedChangeBucketV1::default()
        },
        context: TypedChangeBucketV1 {
            notes: vec![
                "context changes remain folded into the imported instance layer for accepted-module previews"
                    .to_string(),
            ],
            ..TypedChangeBucketV1::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sem_gate_summary_from_evolution_preview_is_compact_and_eq_safe() {
        let preview = EvolutionPreviewV1 {
            version: EVOLUTION_PREVIEW_VERSION_V1.to_string(),
            kind: "proposal_review".to_string(),
            base_snapshot_id: None,
            candidate_label: "fnv1a64:proposal-set".to_string(),
            typed_change: TypedChangeSummaryV1::default(),
            quality_delta: crate::quality::QualityReportV1 {
                version: "quality_report_v1".to_string(),
                generated_at_unix_secs: 1,
                input: "proposal".to_string(),
                profile: "fast".to_string(),
                plane: "both".to_string(),
                summary: crate::quality::QualitySummaryV1 {
                    error_count: 0,
                    warning_count: 1,
                    info_count: 0,
                },
                findings: Vec::new(),
            },
            competency_gate: Some(crate::proposals_validate::CompetencyGateReportV1 {
                total: 2,
                satisfied_before: 1,
                satisfied_after: 2,
                coverage_before: 0.5,
                coverage_after: 1.0,
                cost_before: 1.0,
                cost_after: 0.0,
                regressions: 0,
                improvements: 1,
                gate_passed: true,
                policy: crate::proposals_validate::CompetencyGatePolicyV1 {
                    fail_on_regression: true,
                    fail_on_unsatisfied_after: true,
                },
                questions: Vec::new(),
            }),
            trust: crate::proposals_validate::ProposalValidationTrustContractV1 {
                trust_class: "preview_validated".to_string(),
                soundness: "preview_typechecked_and_quality_gated".to_string(),
                coverage: "proposal_delta_plus_competency_questions".to_string(),
                scope: "snapshot_scoped_preview".to_string(),
                reasons: vec!["preview is scoped to the proposal delta".to_string()],
            },
            runtime_semantics: Some(crate::semantic_claim::RuntimeSemanticSummaryV1 {
                version: crate::semantic_claim::RUNTIME_SEMANTIC_SUMMARY_VERSION_V1.to_string(),
                trust_class: "preview_validated".to_string(),
                soundness: "preview_typechecked_and_quality_gated".to_string(),
                coverage: "proposal_delta_plus_competency_questions".to_string(),
                scope: "snapshot_scoped_preview".to_string(),
                completeness_claim: "not_claimed".to_string(),
                ontology_closure_claim: "not_claimed".to_string(),
                rule_inventory: crate::semantic_claim::SemanticRuleInventoryV1::default(),
                semantic_coverage: crate::semantic_claim::SemanticCoverageSummaryV1 {
                    typed_fact_surface: "schema_scoped_fact_typing_present".to_string(),
                    structured_constraint_surface: "none_detected".to_string(),
                    rewrite_surface: "not_declared".to_string(),
                    named_block_surface: "none_declared".to_string(),
                    competency_surface: "cq_gated_preview".to_string(),
                    quality_surface: "preview_quality_delta".to_string(),
                    gaps: Vec::new(),
                },
                notes: Vec::new(),
            }),
            trust_delta: TrustDeltaV1 {
                preview_trust_class: "preview_validated".to_string(),
                preview_soundness: "preview_typechecked_and_quality_gated".to_string(),
                preview_coverage: "proposal_delta_plus_competency_questions".to_string(),
                preview_scope: "snapshot_scoped_preview".to_string(),
                competency_coverage_before: Some(0.5),
                competency_coverage_after: Some(1.0),
                regressions: 0,
                improvements: 1,
                changed_questions: 0,
                questions: Vec::new(),
                notes: Vec::new(),
            },
            residual_obligations: vec!["author review".to_string()],
            ok: true,
        };

        let summary = sem_gate_summary_from_evolution_preview(&preview);
        assert_eq!(summary.kind, "proposal_review");
        assert_eq!(summary.trust.trust_class, "preview_validated");
        assert_eq!(summary.trust.completeness_claim, "not_claimed");
        assert_eq!(
            summary
                .competency
                .as_ref()
                .expect("competency")
                .improvements,
            1
        );
        assert_eq!(summary.residual_obligation_count, 1);
        assert!(summary.rule.is_none());
        assert_eq!(
            summary
                .runtime_semantics
                .as_ref()
                .expect("runtime semantics")
                .ontology_closure_claim,
            "not_claimed"
        );
    }
}
