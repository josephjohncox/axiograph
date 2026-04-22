use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

use axiograph_pathdb::{
    kernel_ir::{
        CompiledSchemaIr, RelationSemanticsIr, RoleKind, TheoryIr, TheoryObligationKindIr,
        TheoryObligationRefIr, TheorySubjectRefIr, WitnessViewIr,
    },
    migration::{MigrationFunctorKindV1, SchemaMorphismV1, SchemaV1},
    AcceptedSnapshotId, AxiDigest, ProposalDigest,
};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EvolutionPrimitiveV1 {
    ReifyRelationObject {
        relation: String,
        tuple_type: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        roles: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    IntroduceDependentRelationFamily {
        relation: String,
        tuple_type: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        index_roles: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        fiber_roles: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    TransportAlongSchemaMorphism {
        operator: MigrationFunctorKindV1,
        morphism_id: String,
        source_schema: String,
        target_schema: String,
        object_mappings: usize,
        arrow_mappings: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    IntroduceSubtype {
        sub: String,
        sup: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        affected_subject_refs: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    GeneralizeToSupertype {
        from: String,
        to: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        affected_subject_refs: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    SpecializeToSubtype {
        from: String,
        to: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        affected_subject_refs: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    PushRelationRoleToSubtype {
        relation: String,
        role: String,
        from_supertype: String,
        to_subtype: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        affected_subject_refs: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    PullRelationRoleToSupertype {
        relation: String,
        role: String,
        from_subtype: String,
        to_supertype: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        affected_subject_refs: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    FactorCommonStructureToSupertype {
        supertype: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        source_types: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        relations: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        fields: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    SplitTypeIntoSubtypes {
        source: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        subtypes: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        discriminator: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        affected_subject_refs: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    MergeTypesUnderSupertype {
        supertype: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        merged_types: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        affected_subject_refs: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    LiftRelationToCarrier {
        relation: String,
        carrier_type: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        roles: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    AddPathEquation {
        equation_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_box: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end_box: Option<String>,
        lhs_steps: usize,
        rhs_steps: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    AddRewriteRule {
        rule_id: String,
        scope: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    ResolveConflictByDecision {
        artifact_kind: String,
        artifact_id: String,
        resolution: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
}

#[allow(dead_code)]
impl EvolutionPrimitiveV1 {
    pub fn reify_relation_object<I, T>(
        relation: impl Into<String>,
        tuple_type: impl Into<String>,
        roles: I,
    ) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        Self::ReifyRelationObject {
            relation: relation.into(),
            tuple_type: tuple_type.into(),
            roles: roles.into_iter().map(Into::into).collect(),
            rationale: None,
        }
    }

    pub fn introduce_dependent_relation_family<I, T, J, U>(
        relation: impl Into<String>,
        tuple_type: impl Into<String>,
        index_roles: I,
        fiber_roles: J,
    ) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
        J: IntoIterator<Item = U>,
        U: Into<String>,
    {
        Self::IntroduceDependentRelationFamily {
            relation: relation.into(),
            tuple_type: tuple_type.into(),
            index_roles: index_roles.into_iter().map(Into::into).collect(),
            fiber_roles: fiber_roles.into_iter().map(Into::into).collect(),
            rationale: None,
        }
    }

    pub fn transport_along_schema_morphism(
        operator: MigrationFunctorKindV1,
        morphism_id: impl Into<String>,
        source_schema: impl Into<String>,
        target_schema: impl Into<String>,
        object_mappings: usize,
        arrow_mappings: usize,
    ) -> Self {
        Self::TransportAlongSchemaMorphism {
            operator,
            morphism_id: morphism_id.into(),
            source_schema: source_schema.into(),
            target_schema: target_schema.into(),
            object_mappings,
            arrow_mappings,
            rationale: None,
        }
    }

    pub fn introduce_subtype(sub: impl Into<String>, sup: impl Into<String>) -> Self {
        Self::IntroduceSubtype {
            sub: sub.into(),
            sup: sup.into(),
            affected_subject_refs: Vec::new(),
            rationale: None,
        }
    }

    pub fn generalize_to_supertype(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self::GeneralizeToSupertype {
            from: from.into(),
            to: to.into(),
            affected_subject_refs: Vec::new(),
            rationale: None,
        }
    }

    pub fn specialize_to_subtype(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self::SpecializeToSubtype {
            from: from.into(),
            to: to.into(),
            affected_subject_refs: Vec::new(),
            rationale: None,
        }
    }

    pub fn push_relation_role_to_subtype(
        relation: impl Into<String>,
        role: impl Into<String>,
        from_supertype: impl Into<String>,
        to_subtype: impl Into<String>,
    ) -> Self {
        Self::PushRelationRoleToSubtype {
            relation: relation.into(),
            role: role.into(),
            from_supertype: from_supertype.into(),
            to_subtype: to_subtype.into(),
            affected_subject_refs: Vec::new(),
            rationale: None,
        }
    }

    pub fn pull_relation_role_to_supertype(
        relation: impl Into<String>,
        role: impl Into<String>,
        from_subtype: impl Into<String>,
        to_supertype: impl Into<String>,
    ) -> Self {
        Self::PullRelationRoleToSupertype {
            relation: relation.into(),
            role: role.into(),
            from_subtype: from_subtype.into(),
            to_supertype: to_supertype.into(),
            affected_subject_refs: Vec::new(),
            rationale: None,
        }
    }

    pub fn factor_common_structure_to_supertype<I, T>(
        supertype: impl Into<String>,
        source_types: I,
    ) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        Self::FactorCommonStructureToSupertype {
            supertype: supertype.into(),
            source_types: source_types.into_iter().map(Into::into).collect(),
            relations: Vec::new(),
            fields: Vec::new(),
            rationale: None,
        }
    }

    pub fn split_type_into_subtypes<I, T>(source: impl Into<String>, subtypes: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        Self::SplitTypeIntoSubtypes {
            source: source.into(),
            subtypes: subtypes.into_iter().map(Into::into).collect(),
            discriminator: None,
            affected_subject_refs: Vec::new(),
            rationale: None,
        }
    }

    pub fn merge_types_under_supertype<I, T>(supertype: impl Into<String>, merged_types: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        Self::MergeTypesUnderSupertype {
            supertype: supertype.into(),
            merged_types: merged_types.into_iter().map(Into::into).collect(),
            affected_subject_refs: Vec::new(),
            rationale: None,
        }
    }

    pub fn lift_relation_to_carrier<I, T>(
        relation: impl Into<String>,
        carrier_type: impl Into<String>,
        roles: I,
    ) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        Self::LiftRelationToCarrier {
            relation: relation.into(),
            carrier_type: carrier_type.into(),
            roles: roles.into_iter().map(Into::into).collect(),
            rationale: None,
        }
    }

    pub fn add_path_equation(
        equation_id: impl Into<String>,
        lhs_steps: usize,
        rhs_steps: usize,
    ) -> Self {
        Self::AddPathEquation {
            equation_id: equation_id.into(),
            start_box: None,
            end_box: None,
            lhs_steps,
            rhs_steps,
            rationale: None,
        }
    }

    pub fn add_rewrite_rule(rule_id: impl Into<String>, scope: impl Into<String>) -> Self {
        Self::AddRewriteRule {
            rule_id: rule_id.into(),
            scope: scope.into(),
            rationale: None,
        }
    }

    pub fn resolve_conflict_by_decision(
        artifact_kind: impl Into<String>,
        artifact_id: impl Into<String>,
        resolution: impl Into<String>,
    ) -> Self {
        Self::ResolveConflictByDecision {
            artifact_kind: artifact_kind.into(),
            artifact_id: artifact_id.into(),
            resolution: resolution.into(),
            rationale: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TypedChangeSummaryV1 {
    pub kind: String,
    #[serde(default)]
    pub subjects: Vec<String>,
    #[serde(default)]
    pub primitives: Vec<EvolutionPrimitiveV1>,
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

impl TypedChangeSummaryV1 {
    #[allow(dead_code)]
    pub fn push_primitive(&mut self, primitive: EvolutionPrimitiveV1) {
        self.primitives.push(primitive);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct EvolutionSemanticDeltaV1 {
    pub delta_kind: String,
    #[serde(default)]
    pub subject_refs: Vec<String>,
    #[serde(default)]
    pub primitives: Vec<EvolutionPrimitiveV1>,
    #[serde(default)]
    pub changed_layers: Vec<String>,
    #[serde(default)]
    pub schema: TypedChangeBucketV1,
    #[serde(default)]
    pub theory: TypedChangeBucketV1,
    #[serde(default)]
    pub instance: TypedChangeBucketV1,
    #[serde(default)]
    pub context: TypedChangeBucketV1,
    #[serde(default)]
    pub total_added: usize,
    #[serde(default)]
    pub total_reused: usize,
    #[serde(default)]
    pub total_removed: usize,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
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
    #[serde(default)]
    pub constraint_count: u32,
    #[serde(default)]
    pub instance_count: u32,
    #[serde(default)]
    pub check_count: u32,
    #[serde(default)]
    pub total_rules: usize,
    #[serde(default)]
    pub relation_constraints: usize,
    #[serde(default)]
    pub rewrite_rules: usize,
    #[serde(default)]
    pub named_block_constraints: usize,
    #[serde(default)]
    pub runtime_checkable_rules: usize,
    #[serde(default)]
    pub runtime_visible_rules: usize,
    #[serde(default)]
    pub review_only_rules: usize,
    #[serde(default)]
    pub relations_with_rules: usize,
    #[serde(default)]
    pub theories_with_rules: usize,
    #[serde(default)]
    pub notes: Vec<String>,
}

impl Default for SemRuleSummaryV1 {
    fn default() -> Self {
        Self {
            constraint_count: 0,
            instance_count: 0,
            check_count: 0,
            total_rules: 0,
            relation_constraints: 0,
            rewrite_rules: 0,
            named_block_constraints: 0,
            runtime_checkable_rules: 0,
            runtime_visible_rules: 0,
            review_only_rules: 0,
            relations_with_rules: 0,
            theories_with_rules: 0,
            notes: Vec::new(),
        }
    }
}

impl SemRuleSummaryV1 {
    fn merged_with(self, overlay: SemRuleSummaryV1) -> SemRuleSummaryV1 {
        fn choose_u32(base: u32, overlay: u32) -> u32 {
            if overlay == 0 {
                base
            } else {
                overlay
            }
        }

        fn choose_usize(base: usize, overlay: usize) -> usize {
            if overlay == 0 {
                base
            } else {
                overlay
            }
        }

        let mut notes = self.notes;
        for note in overlay.notes {
            if !notes.iter().any(|existing| existing == &note) {
                notes.push(note);
            }
        }

        SemRuleSummaryV1 {
            constraint_count: choose_u32(self.constraint_count, overlay.constraint_count),
            instance_count: choose_u32(self.instance_count, overlay.instance_count),
            check_count: choose_u32(self.check_count, overlay.check_count),
            total_rules: choose_usize(self.total_rules, overlay.total_rules),
            relation_constraints: choose_usize(
                self.relation_constraints,
                overlay.relation_constraints,
            ),
            rewrite_rules: choose_usize(self.rewrite_rules, overlay.rewrite_rules),
            named_block_constraints: choose_usize(
                self.named_block_constraints,
                overlay.named_block_constraints,
            ),
            runtime_checkable_rules: choose_usize(
                self.runtime_checkable_rules,
                overlay.runtime_checkable_rules,
            ),
            runtime_visible_rules: choose_usize(
                self.runtime_visible_rules,
                overlay.runtime_visible_rules,
            ),
            review_only_rules: choose_usize(self.review_only_rules, overlay.review_only_rules),
            relations_with_rules: choose_usize(
                self.relations_with_rules,
                overlay.relations_with_rules,
            ),
            theories_with_rules: choose_usize(
                self.theories_with_rules,
                overlay.theories_with_rules,
            ),
            notes,
        }
    }
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
        self.rule = Some(match self.rule.take() {
            Some(existing) => existing.merged_with(rule),
            None => rule,
        });
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct EvolutionCoverageSummaryV1 {
    pub typed_fact_surface: String,
    pub structured_constraint_surface: String,
    pub rewrite_surface: String,
    pub named_block_surface: String,
    pub competency_surface: String,
    pub quality_surface: String,
    #[serde(default)]
    pub competency_gate_configured: bool,
    #[serde(default)]
    pub competency_questions_total: usize,
    #[serde(default)]
    pub competency_questions_changed: usize,
    #[serde(default)]
    pub regressions: usize,
    #[serde(default)]
    pub improvements: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub competency_coverage_before: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub competency_coverage_after: Option<f64>,
    #[serde(default)]
    pub gaps: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvolutionPreviewV1 {
    pub version: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_snapshot_id: Option<AcceptedSnapshotId>,
    pub candidate_label: String,
    pub typed_change: TypedChangeSummaryV1,
    pub semantic_delta: EvolutionSemanticDeltaV1,
    pub quality_delta: crate::quality::QualityReportV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub competency_gate: Option<crate::proposals_validate::CompetencyGateReportV1>,
    pub trust: crate::proposals_validate::ProposalValidationTrustContractV1,
    pub trust_summary: SemTrustSummaryV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_semantics: Option<crate::semantic_claim::RuntimeSemanticSummaryV1>,
    pub rule_summary: SemRuleSummaryV1,
    pub coverage_summary: EvolutionCoverageSummaryV1,
    #[serde(default)]
    pub trust_delta: TrustDeltaV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exploration_next_actions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_obligations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refinement_candidates: Vec<crate::typed_refinement::RuntimeRefinementCandidateV1>,
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[allow(dead_code)]
pub struct MigrationTransportObligationV1 {
    pub operator: MigrationFunctorKindV1,
    pub obligation_id: String,
    pub obligation_kind: String,
    pub subject_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theory_obligation_ref: Option<TheoryObligationRefIr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theory_subject_ref: Option<TheorySubjectRefIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub theory_subject_refs: Vec<TheorySubjectRefIr>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[allow(dead_code)]
pub struct MigrationTransportRefinementApplyResultV1 {
    pub handle: crate::typed_refinement::RuntimeRefinementHandleV1,
    pub base_transport_obligations: Vec<MigrationTransportObligationV1>,
    pub resolved_transport_obligation: MigrationTransportObligationV1,
    pub remaining_transport_obligations: Vec<MigrationTransportObligationV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct MigrationAuthoringApplyResultV1 {
    pub transport_apply: MigrationTransportRefinementApplyResultV1,
    pub evolution_preview: EvolutionPreviewV1,
}

pub const EVOLUTION_PREVIEW_VERSION_V1: &str = "evolution_preview_v1";

fn bucket_has_structural_change(bucket: &TypedChangeBucketV1) -> bool {
    bucket.added > 0 || bucket.reused > 0 || bucket.removed > 0
}

fn push_unique_layer(changed_layers: &mut Vec<String>, layer: &str) {
    if !changed_layers.iter().any(|existing| existing == layer) {
        changed_layers.push(layer.to_string());
    }
}

fn artifact_kind_layer(artifact_kind: &str) -> &'static str {
    let lowered = artifact_kind.trim().to_ascii_lowercase();
    if lowered.contains("context") || lowered.contains("world") || lowered.contains("time") {
        "context"
    } else if lowered.contains("theory")
        || lowered.contains("constraint")
        || lowered.contains("rewrite")
        || lowered.contains("equation")
        || lowered.contains("rule")
    {
        "theory"
    } else if lowered.contains("schema")
        || lowered.contains("relation")
        || lowered.contains("type")
        || lowered.contains("object")
        || lowered.contains("arrow")
    {
        "schema"
    } else {
        "instance"
    }
}

fn primitive_changed_layers(primitive: &EvolutionPrimitiveV1) -> Vec<&'static str> {
    match primitive {
        EvolutionPrimitiveV1::ReifyRelationObject { .. }
        | EvolutionPrimitiveV1::IntroduceDependentRelationFamily { .. }
        | EvolutionPrimitiveV1::TransportAlongSchemaMorphism { .. }
        | EvolutionPrimitiveV1::IntroduceSubtype { .. }
        | EvolutionPrimitiveV1::GeneralizeToSupertype { .. }
        | EvolutionPrimitiveV1::SpecializeToSubtype { .. }
        | EvolutionPrimitiveV1::PushRelationRoleToSubtype { .. }
        | EvolutionPrimitiveV1::PullRelationRoleToSupertype { .. }
        | EvolutionPrimitiveV1::FactorCommonStructureToSupertype { .. }
        | EvolutionPrimitiveV1::LiftRelationToCarrier { .. } => vec!["schema"],
        EvolutionPrimitiveV1::AddPathEquation { .. }
        | EvolutionPrimitiveV1::AddRewriteRule { .. } => vec!["theory"],
        EvolutionPrimitiveV1::ResolveConflictByDecision { artifact_kind, .. } => {
            vec![artifact_kind_layer(artifact_kind)]
        }
        EvolutionPrimitiveV1::SplitTypeIntoSubtypes {
            affected_subject_refs,
            ..
        }
        | EvolutionPrimitiveV1::MergeTypesUnderSupertype {
            affected_subject_refs,
            ..
        } => {
            if affected_subject_refs.is_empty() {
                vec!["schema"]
            } else {
                vec!["schema", "instance"]
            }
        }
    }
}

fn semantic_delta_from_typed_change(
    typed_change: &TypedChangeSummaryV1,
) -> EvolutionSemanticDeltaV1 {
    let mut changed_layers = Vec::new();
    if bucket_has_structural_change(&typed_change.schema) {
        push_unique_layer(&mut changed_layers, "schema");
    }
    if bucket_has_structural_change(&typed_change.theory) {
        push_unique_layer(&mut changed_layers, "theory");
    }
    if bucket_has_structural_change(&typed_change.instance) {
        push_unique_layer(&mut changed_layers, "instance");
    }
    if bucket_has_structural_change(&typed_change.context) {
        push_unique_layer(&mut changed_layers, "context");
    }
    for primitive in &typed_change.primitives {
        for layer in primitive_changed_layers(primitive) {
            push_unique_layer(&mut changed_layers, layer);
        }
    }

    EvolutionSemanticDeltaV1 {
        delta_kind: typed_change.kind.clone(),
        subject_refs: typed_change.subjects.clone(),
        primitives: typed_change.primitives.clone(),
        changed_layers,
        schema: typed_change.schema.clone(),
        theory: typed_change.theory.clone(),
        instance: typed_change.instance.clone(),
        context: typed_change.context.clone(),
        total_added: typed_change.schema.added
            + typed_change.theory.added
            + typed_change.instance.added
            + typed_change.context.added,
        total_reused: typed_change.schema.reused
            + typed_change.theory.reused
            + typed_change.instance.reused
            + typed_change.context.reused,
        total_removed: typed_change.schema.removed
            + typed_change.theory.removed
            + typed_change.instance.removed
            + typed_change.context.removed,
        notes: typed_change.notes.clone(),
    }
}

fn trust_summary_from_preview(
    trust: &crate::proposals_validate::ProposalValidationTrustContractV1,
    runtime_semantics: Option<&crate::semantic_claim::RuntimeSemanticSummaryV1>,
) -> SemTrustSummaryV1 {
    SemTrustSummaryV1 {
        trust_class: trust.trust_class.clone(),
        soundness: trust.soundness.clone(),
        coverage: trust.coverage.clone(),
        scope: trust.scope.clone(),
        completeness_claim: runtime_semantics
            .map(|summary| summary.completeness_claim.clone())
            .unwrap_or_else(|| "not_claimed".to_string()),
        ontology_closure_claim: runtime_semantics
            .map(|summary| summary.ontology_closure_claim.clone())
            .unwrap_or_else(|| "not_claimed".to_string()),
        reasons: trust.reasons.clone(),
    }
}

fn push_unique_note(notes: &mut Vec<String>, note: impl Into<String>) {
    let note = note.into();
    if !notes.iter().any(|existing| existing == &note) {
        notes.push(note);
    }
}

fn exploration_next_actions_from_primitives(primitives: &[EvolutionPrimitiveV1]) -> Vec<String> {
    let mut next_actions = Vec::new();
    for primitive in primitives {
        match primitive {
            EvolutionPrimitiveV1::ReifyRelationObject {
                relation,
                tuple_type,
                ..
            } => {
                push_unique_note(
                    &mut next_actions,
                    format!(
                        "review whether `{relation}` should be treated primarily as the explicit relation-object `{tuple_type}` across queries, docs, and code mappings"
                    ),
                );
            }
            EvolutionPrimitiveV1::IntroduceDependentRelationFamily {
                relation,
                index_roles,
                fiber_roles,
                ..
            } => {
                push_unique_note(
                    &mut next_actions,
                    format!(
                        "check CQ/query coverage for indexed relation family `{relation}` over index roles `{}` and fiber roles `{}`",
                        index_roles.join("`, `"),
                        fiber_roles.join("`, `")
                    ),
                );
            }
            EvolutionPrimitiveV1::TransportAlongSchemaMorphism {
                morphism_id,
                source_schema,
                target_schema,
                ..
            } => {
                push_unique_note(
                    &mut next_actions,
                    format!(
                        "review transport obligations and CQ drift along schema morphism `{morphism_id}` (`{source_schema}` -> `{target_schema}`)"
                    ),
                );
            }
            EvolutionPrimitiveV1::IntroduceSubtype { sub, sup, .. } => {
                push_unique_note(
                    &mut next_actions,
                    format!(
                        "rerun subtype-sensitive CQs and rule applicability for `{sub} <: {sup}`"
                    ),
                );
                push_unique_note(
                    &mut next_actions,
                    format!(
                        "inspect relations and fields currently attached to `{sup}` for candidate push-down into `{sub}`"
                    ),
                );
            }
            EvolutionPrimitiveV1::GeneralizeToSupertype { from, to, .. } => {
                push_unique_note(
                    &mut next_actions,
                    format!(
                        "review sibling subtypes of `{to}` to see which rules and roles should generalize alongside `{from}`"
                    ),
                );
            }
            EvolutionPrimitiveV1::SpecializeToSubtype { from, to, .. } => {
                push_unique_note(
                    &mut next_actions,
                    format!(
                        "check which rules, CQs, and implementation mappings should move from `{from}` to the narrower subtype `{to}`"
                    ),
                );
            }
            EvolutionPrimitiveV1::PushRelationRoleToSubtype {
                relation,
                role,
                from_supertype,
                to_subtype,
                ..
            } => {
                push_unique_note(
                    &mut next_actions,
                    format!(
                        "review `{relation}.{role}` queries and CQs for callers that still assume the broader `{from_supertype}` scope after pushing it to `{to_subtype}`"
                    ),
                );
            }
            EvolutionPrimitiveV1::PullRelationRoleToSupertype {
                relation,
                role,
                from_subtype,
                to_supertype,
                ..
            } => {
                push_unique_note(
                    &mut next_actions,
                    format!(
                        "inspect sibling types of `{from_subtype}` and shared rules for `{relation}.{role}` now that it is being pulled to `{to_supertype}`"
                    ),
                );
            }
            EvolutionPrimitiveV1::FactorCommonStructureToSupertype {
                supertype,
                source_types,
                ..
            } => {
                push_unique_note(
                    &mut next_actions,
                    format!(
                        "compare CQ and implementation coverage across `{}` before factoring shared structure into `{supertype}`",
                        source_types.join("`, `")
                    ),
                );
            }
            EvolutionPrimitiveV1::SplitTypeIntoSubtypes {
                source,
                subtypes,
                discriminator,
                ..
            } => {
                let discriminator_note = discriminator
                    .as_ref()
                    .map(|name| format!(" using discriminator `{name}`"))
                    .unwrap_or_default();
                push_unique_note(
                    &mut next_actions,
                    format!(
                        "prepare migration/CQ updates for splitting `{source}` into `{}`{discriminator_note}",
                        subtypes.join("`, `")
                    ),
                );
            }
            EvolutionPrimitiveV1::MergeTypesUnderSupertype {
                supertype,
                merged_types,
                ..
            } => {
                push_unique_note(
                    &mut next_actions,
                    format!(
                        "check which distinctions, rules, and docs would be lost or weakened when merging `{}` under `{supertype}`",
                        merged_types.join("`, `")
                    ),
                );
            }
            EvolutionPrimitiveV1::LiftRelationToCarrier {
                relation,
                carrier_type,
                ..
            } => {
                push_unique_note(
                    &mut next_actions,
                    format!(
                        "move provenance, temporal/context fields, and business rules for `{relation}` onto the explicit carrier `{carrier_type}`"
                    ),
                );
            }
            EvolutionPrimitiveV1::AddPathEquation {
                equation_id,
                start_box,
                end_box,
                ..
            } => {
                let endpoints = match (start_box.as_ref(), end_box.as_ref()) {
                    (Some(start), Some(end)) => format!(" from `{start}` to `{end}`"),
                    _ => String::new(),
                };
                push_unique_note(
                    &mut next_actions,
                    format!(
                        "rerun CQ/query elaboration and normalization review for path equation `{equation_id}`{endpoints}"
                    ),
                );
            }
            EvolutionPrimitiveV1::AddRewriteRule { rule_id, scope, .. } => {
                push_unique_note(
                    &mut next_actions,
                    format!(
                        "check soundness scope, certifiability, and query impact for rewrite rule `{rule_id}` in `{scope}`"
                    ),
                );
            }
            EvolutionPrimitiveV1::ResolveConflictByDecision {
                artifact_kind,
                artifact_id,
                resolution,
                ..
            } => {
                push_unique_note(
                    &mut next_actions,
                    format!(
                        "inspect reconciliation decision `{resolution}` for {artifact_kind} `{artifact_id}` and rerun affected CQs/queries"
                    ),
                );
            }
        }
    }
    next_actions
}

fn synthetic_quality_report(
    input: &str,
    profile: &str,
    error_count: usize,
    warning_count: usize,
) -> crate::quality::QualityReportV1 {
    crate::quality::QualityReportV1 {
        version: "quality_report_v1".to_string(),
        generated_at_unix_secs: 0,
        input: input.to_string(),
        profile: profile.to_string(),
        plane: "review".to_string(),
        summary: crate::quality::QualitySummaryV1 {
            error_count,
            warning_count,
            info_count: 0,
        },
        findings: Vec::new(),
    }
}

fn rule_summary_from_preview(
    runtime_semantics: Option<&crate::semantic_claim::RuntimeSemanticSummaryV1>,
) -> SemRuleSummaryV1 {
    let Some(runtime_semantics) = runtime_semantics else {
        return SemRuleSummaryV1 {
            notes: vec![
                "runtime semantic rule inventory was not materialized for this preview".to_string(),
            ],
            ..SemRuleSummaryV1::default()
        };
    };

    let inventory = &runtime_semantics.rule_inventory;
    SemRuleSummaryV1 {
        constraint_count: 0,
        instance_count: 0,
        check_count: 0,
        total_rules: inventory.total_rules,
        relation_constraints: inventory.relation_constraints,
        rewrite_rules: inventory.rewrite_rules,
        named_block_constraints: inventory.named_block_constraints,
        runtime_checkable_rules: inventory.runtime_checkable_rules,
        runtime_visible_rules: inventory.runtime_visible_rules,
        review_only_rules: inventory.review_only_rules,
        relations_with_rules: inventory.relations_with_rules,
        theories_with_rules: inventory.theories_with_rules,
        notes: inventory.notes.clone(),
    }
}

fn coverage_summary_from_preview(
    trust_delta: &TrustDeltaV1,
    competency_gate: Option<&crate::proposals_validate::CompetencyGateReportV1>,
    runtime_semantics: Option<&crate::semantic_claim::RuntimeSemanticSummaryV1>,
) -> EvolutionCoverageSummaryV1 {
    let competency_gate_configured = competency_gate.is_some();
    let competency_questions_total = competency_gate.map(|gate| gate.total).unwrap_or(0);
    let competency_surface_fallback = if competency_gate_configured {
        "cq_gated_preview".to_string()
    } else {
        "not_configured".to_string()
    };

    let (
        typed_fact_surface,
        structured_constraint_surface,
        rewrite_surface,
        named_block_surface,
        competency_surface,
        quality_surface,
        mut gaps,
    ) = if let Some(runtime_semantics) = runtime_semantics {
        let coverage = &runtime_semantics.semantic_coverage;
        (
            coverage.typed_fact_surface.clone(),
            coverage.structured_constraint_surface.clone(),
            coverage.rewrite_surface.clone(),
            coverage.named_block_surface.clone(),
            coverage.competency_surface.clone(),
            coverage.quality_surface.clone(),
            coverage.gaps.clone(),
        )
    } else {
        (
            "not_reported".to_string(),
            "not_reported".to_string(),
            "not_reported".to_string(),
            "not_reported".to_string(),
            competency_surface_fallback,
            "preview_quality_delta".to_string(),
            vec!["runtime semantic coverage was not materialized for this preview".to_string()],
        )
    };

    let mut notes = trust_delta.notes.clone();
    if competency_gate_configured {
        notes.push(format!(
            "competency gate evaluated {} question(s) for this preview",
            competency_questions_total
        ));
    } else {
        notes.push(
            "coverage summary is limited to preview trust/runtime surfaces because no competency gate was configured"
                .to_string(),
        );
    }
    if !competency_gate_configured {
        gaps.push(
            "coverage hooks do not include CQ before/after data because no competency gate was configured"
                .to_string(),
        );
    }

    EvolutionCoverageSummaryV1 {
        typed_fact_surface,
        structured_constraint_surface,
        rewrite_surface,
        named_block_surface,
        competency_surface,
        quality_surface,
        competency_gate_configured,
        competency_questions_total,
        competency_questions_changed: trust_delta.changed_questions,
        regressions: trust_delta.regressions,
        improvements: trust_delta.improvements,
        competency_coverage_before: trust_delta.competency_coverage_before,
        competency_coverage_after: trust_delta.competency_coverage_after,
        gaps,
        notes,
    }
}

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
        trust: preview.trust_summary.clone(),
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
        rule: Some(preview.rule_summary.clone()),
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

fn migration_refinement_candidates(
    transport_obligations: &[MigrationTransportObligationV1],
) -> Vec<crate::typed_refinement::RuntimeRefinementCandidateV1> {
    transport_obligations
        .iter()
        .map(|obligation| {
            let summary = obligation
                .theory_obligation_ref
                .as_ref()
                .map(|theory_obligation| {
                    format!(
                        "address migration obligation `{}` over theory obligation `{}`",
                        obligation.obligation_id,
                        theory_obligation.display_name()
                    )
                })
                .unwrap_or_else(|| {
                    format!(
                        "address migration obligation `{}` for `{}`",
                        obligation.obligation_id, obligation.subject_ref
                    )
                });
            crate::typed_refinement::RuntimeRefinementCandidateV1::new_migration(
                summary,
                crate::typed_refinement::MigrationRefinementOpV1::AddressTransportObligation {
                    operator: obligation.operator.clone(),
                    obligation_id: obligation.obligation_id.clone(),
                    obligation_kind: obligation.obligation_kind.clone(),
                    subject_ref: obligation.subject_ref.clone(),
                    theory_obligation_ref: obligation.theory_obligation_ref.clone(),
                    theory_subject_ref: obligation.theory_subject_ref.clone(),
                    theory_subject_refs: obligation.theory_subject_refs.clone(),
                },
                obligation.obligation_id.clone(),
                obligation.theory_obligation_ref.clone(),
                obligation.theory_subject_refs.clone(),
            )
        })
        .collect()
}

#[allow(dead_code)]
pub fn apply_runtime_refinement_handle_to_migration_transport_obligations(
    transport_obligations: &[MigrationTransportObligationV1],
    handle: &crate::typed_refinement::RuntimeRefinementHandleV1,
) -> anyhow::Result<MigrationTransportRefinementApplyResultV1> {
    handle.validate()?;
    let crate::typed_refinement::RuntimeRefinementPayloadV1::MigrationAuthoring { op } =
        &handle.payload
    else {
        return Err(anyhow!(
            "runtime refinement handle `{}` is not a migration refinement",
            handle.id
        ));
    };
    let crate::typed_refinement::MigrationRefinementOpV1::AddressTransportObligation {
        operator,
        obligation_id,
        obligation_kind,
        subject_ref,
        theory_obligation_ref,
        theory_subject_ref,
        theory_subject_refs,
    } = op;

    let mut matches = transport_obligations
        .iter()
        .filter(|candidate| candidate.obligation_id == *obligation_id);
    let Some(existing) = matches.next() else {
        return Err(anyhow!(
            "migration refinement handle `{}` targets unknown obligation `{}`",
            handle.id,
            obligation_id
        ));
    };
    if matches.next().is_some() {
        return Err(anyhow!(
            "migration refinement handle `{}` matched multiple transport obligations named `{}`; obligation ids must stay unique",
            handle.id,
            obligation_id
        ));
    }

    if existing.obligation_kind != *obligation_kind {
        return Err(anyhow!(
            "migration refinement handle `{}` expects obligation kind `{}` but current obligation `{}` has kind `{}`",
            handle.id,
            obligation_kind,
            obligation_id,
            existing.obligation_kind
        ));
    }
    if existing.operator != *operator {
        return Err(anyhow!(
            "migration refinement handle `{}` expects operator `{:?}` but current obligation `{}` has operator `{:?}`",
            handle.id,
            operator,
            obligation_id,
            existing.operator
        ));
    }
    if existing.subject_ref != *subject_ref {
        return Err(anyhow!(
            "migration refinement handle `{}` expects subject `{}` but current obligation `{}` targets `{}`",
            handle.id,
            subject_ref,
            obligation_id,
            existing.subject_ref
        ));
    }
    if existing.theory_subject_ref != *theory_subject_ref {
        return Err(anyhow!(
            "migration refinement handle `{}` carries a mismatched primary theory subject for `{}`",
            handle.id,
            obligation_id
        ));
    }
    if existing.theory_obligation_ref != *theory_obligation_ref {
        return Err(anyhow!(
            "migration refinement handle `{}` carries a mismatched theory obligation for `{}`",
            handle.id,
            obligation_id
        ));
    }
    if existing.theory_subject_refs != *theory_subject_refs {
        return Err(anyhow!(
            "migration refinement handle `{}` carries mismatched theory subjects for `{}`",
            handle.id,
            obligation_id
        ));
    }

    let resolved_transport_obligation = existing.clone();
    let remaining_transport_obligations = transport_obligations
        .iter()
        .filter(|obligation| obligation.obligation_id != *obligation_id)
        .cloned()
        .collect();

    Ok(MigrationTransportRefinementApplyResultV1 {
        handle: handle.clone(),
        base_transport_obligations: transport_obligations.to_vec(),
        resolved_transport_obligation,
        remaining_transport_obligations,
    })
}

#[allow(dead_code)]
pub fn apply_runtime_refinement_by_id_to_migration_transport_obligations_from_compiled_theory_v1(
    compiled_schema: &CompiledSchemaIr,
    theories: &[TheoryIr],
    morphism: &SchemaMorphismV1,
    handle_id: &str,
) -> anyhow::Result<MigrationTransportRefinementApplyResultV1> {
    let transport_obligations = build_migration_transport_obligations_from_compiled_theory_v1(
        compiled_schema,
        theories,
        morphism,
    );
    let handle = migration_refinement_candidates(&transport_obligations)
        .into_iter()
        .find(|candidate| candidate.handle.id == handle_id)
        .map(|candidate| candidate.handle)
        .ok_or_else(|| anyhow!("unknown migration refinement handle `{handle_id}`"))?;
    apply_runtime_refinement_handle_to_migration_transport_obligations(
        &transport_obligations,
        &handle,
    )
}

#[allow(dead_code)]
pub fn apply_runtime_refinement_handle_to_migration_preview_v1(
    base_snapshot_id: Option<AcceptedSnapshotId>,
    candidate_label: String,
    morphism: &SchemaMorphismV1,
    source_schema: &SchemaV1,
    transport_obligations: &[MigrationTransportObligationV1],
    handle: &crate::typed_refinement::RuntimeRefinementHandleV1,
) -> anyhow::Result<MigrationAuthoringApplyResultV1> {
    let transport_apply = apply_runtime_refinement_handle_to_migration_transport_obligations(
        transport_obligations,
        handle,
    )?;
    let evolution_preview = build_migration_evolution_preview_v1(
        base_snapshot_id,
        candidate_label,
        morphism,
        source_schema,
        &transport_apply.remaining_transport_obligations,
    );
    Ok(MigrationAuthoringApplyResultV1 {
        transport_apply,
        evolution_preview,
    })
}

#[allow(dead_code)]
pub fn apply_runtime_refinement_by_id_to_migration_preview_from_compiled_theory_v1(
    base_snapshot_id: Option<AcceptedSnapshotId>,
    candidate_label: String,
    compiled_schema: &CompiledSchemaIr,
    theories: &[TheoryIr],
    morphism: &SchemaMorphismV1,
    source_schema: &SchemaV1,
    handle_id: &str,
) -> anyhow::Result<MigrationAuthoringApplyResultV1> {
    let transport_apply =
        apply_runtime_refinement_by_id_to_migration_transport_obligations_from_compiled_theory_v1(
            compiled_schema,
            theories,
            morphism,
            handle_id,
        )?;
    let evolution_preview = build_migration_evolution_preview_v1(
        base_snapshot_id,
        candidate_label,
        morphism,
        source_schema,
        &transport_apply.remaining_transport_obligations,
    );
    Ok(MigrationAuthoringApplyResultV1 {
        transport_apply,
        evolution_preview,
    })
}

fn local_name(raw: &str) -> &str {
    raw.rsplit('.').next().unwrap_or(raw)
}

fn relation_semantics_for_name<'a>(
    compiled_schema: &'a CompiledSchemaIr,
    relation_name: &str,
) -> Option<&'a RelationSemanticsIr> {
    compiled_schema
        .relations
        .get(relation_name)
        .or_else(|| compiled_schema.relations.get(local_name(relation_name)))
}

fn contextual_transport_prefix(relation: Option<&RelationSemanticsIr>) -> &'static str {
    if relation.is_some_and(|relation| {
        relation
            .roles
            .iter()
            .any(|role| matches!(role.kind, RoleKind::Context | RoleKind::Temporal))
    }) {
        "context_"
    } else {
        ""
    }
}

fn transport_kind_for_obligation(
    relation: Option<&RelationSemanticsIr>,
    obligation_kind: Option<TheoryObligationKindIr>,
) -> String {
    let prefix = contextual_transport_prefix(relation);
    let kind = match obligation_kind {
        Some(TheoryObligationKindIr::Constraint) => "constraint",
        Some(TheoryObligationKindIr::PathEquation) => "path_equation",
        Some(TheoryObligationKindIr::OpaqueEquation) => "opaque_equation",
        Some(TheoryObligationKindIr::RewriteRule) => "rewrite_rule",
        None => "relation",
    };
    format!("{prefix}{kind}_transport")
}

fn typed_subject_refs_for_relation(
    compiled_schema: &CompiledSchemaIr,
    relation_name: &str,
) -> Vec<TheorySubjectRefIr> {
    let Some(relation) = relation_semantics_for_name(compiled_schema, relation_name) else {
        return Vec::new();
    };
    let mut refs = vec![TheorySubjectRefIr::Relation {
        relation_id: relation.relation_id.clone(),
        relation_name: relation.name.clone(),
    }];
    refs.extend(relation.roles.iter().map(|role| TheorySubjectRefIr::Role {
        relation_id: relation.relation_id.clone(),
        relation_name: relation.name.clone(),
        role_id: role.role_id.clone(),
        role_name: role.name.clone(),
    }));
    refs
}

fn primary_theory_subject_ref(subject_refs: &[TheorySubjectRefIr]) -> Option<TheorySubjectRefIr> {
    subject_refs
        .iter()
        .find(|subject| !matches!(subject, TheorySubjectRefIr::Theory { .. }))
        .or_else(|| subject_refs.first())
        .cloned()
}

fn obligation_matches_artifact(
    artifact_kind: &str,
    artifact_id: &str,
    obligation: &TheoryObligationRefIr,
) -> bool {
    let lowered_kind = artifact_kind.trim().to_ascii_lowercase();
    match obligation {
        TheoryObligationRefIr::Constraint { .. } => {
            lowered_kind.contains("constraint") && obligation.matches_artifact_id(artifact_id)
        }
        TheoryObligationRefIr::PathEquation { .. }
        | TheoryObligationRefIr::OpaqueEquation { .. } => {
            (lowered_kind.contains("equation")
                || lowered_kind.contains("path")
                || lowered_kind.contains("rewrite"))
                && obligation.matches_artifact_id(artifact_id)
        }
        TheoryObligationRefIr::RewriteRule { .. } => {
            lowered_kind.contains("rewrite") && obligation.matches_artifact_id(artifact_id)
        }
    }
}

fn subject_matches_artifact(
    artifact_kind: &str,
    artifact_id: &str,
    subject: &TheorySubjectRefIr,
) -> bool {
    let lowered_kind = artifact_kind.trim().to_ascii_lowercase();
    match subject {
        TheorySubjectRefIr::Theory { .. } => {
            lowered_kind.contains("theory") && subject.matches_artifact_id(artifact_id)
        }
        TheorySubjectRefIr::Relation { .. } => {
            (lowered_kind.contains("relation")
                || lowered_kind.contains("schema")
                || lowered_kind.contains("fact"))
                && subject.matches_artifact_id(artifact_id)
        }
        TheorySubjectRefIr::Role { .. } => {
            lowered_kind.contains("role") && subject.matches_artifact_id(artifact_id)
        }
    }
}

fn theory_handles_for_artifact(
    compiled_schema: &CompiledSchemaIr,
    theories: &[TheoryIr],
    artifact_kind: &str,
    artifact_id: &str,
) -> (Option<TheoryObligationRefIr>, Vec<TheorySubjectRefIr>) {
    let mut matched_obligations: BTreeSet<TheoryObligationRefIr> = BTreeSet::new();
    let mut matched_subjects: BTreeSet<TheorySubjectRefIr> = BTreeSet::new();

    for theory in theories {
        for obligation in theory.obligation_refs() {
            if obligation_matches_artifact(artifact_kind, artifact_id, &obligation) {
                matched_obligations.insert(obligation.clone());
                for subject in theory.subject_refs_for_obligation(&obligation) {
                    matched_subjects.insert(subject);
                }
            }
        }
        for subject in theory.subject_refs() {
            if subject_matches_artifact(artifact_kind, artifact_id, &subject) {
                matched_subjects.insert(subject);
            }
        }
    }

    if matched_subjects.is_empty()
        && (artifact_kind.contains("relation") || artifact_kind.contains("schema"))
    {
        for subject in typed_subject_refs_for_relation(compiled_schema, artifact_id) {
            matched_subjects.insert(subject);
        }
    }

    if matched_obligations.is_empty() {
        for subject in &matched_subjects {
            for theory in theories {
                for obligation in theory.obligation_refs_for_subject(subject) {
                    matched_obligations.insert(obligation);
                }
            }
        }
    }

    let theory_obligation_ref = if matched_obligations.len() == 1 {
        matched_obligations.into_iter().next()
    } else {
        None
    };

    (
        theory_obligation_ref,
        matched_subjects.into_iter().collect(),
    )
}

#[allow(dead_code)]
pub fn build_migration_transport_obligations_from_compiled_theory_v1(
    compiled_schema: &CompiledSchemaIr,
    theories: &[TheoryIr],
    morphism: &SchemaMorphismV1,
) -> Vec<MigrationTransportObligationV1> {
    let mut obligations = Vec::new();
    let mut seen_ids: BTreeSet<String> = BTreeSet::new();

    for mapping in morphism.arrows.iter().filter(|mapping| {
        !(mapping.target_path.len() == 1 && mapping.target_path[0] == mapping.source_arrow)
    }) {
        let relation = relation_semantics_for_name(compiled_schema, &mapping.source_arrow);
        let target_path = if mapping.target_path.is_empty() {
            "identity".to_string()
        } else {
            mapping.target_path.join(" ; ")
        };
        let relation_subject = relation.and_then(|relation| {
            Some(TheorySubjectRefIr::Relation {
                relation_id: relation.relation_id.clone(),
                relation_name: relation.name.clone(),
            })
        });

        let mut matched = Vec::new();
        if let Some(subject) = relation_subject.as_ref() {
            for theory in theories {
                for obligation in theory.obligation_refs_for_subject(subject) {
                    matched.push((
                        obligation.clone(),
                        theory.subject_refs_for_obligation(&obligation),
                    ));
                }
            }
        }

        if matched.is_empty() {
            let obligation_id = format!("transport:{}:relation", local_name(&mapping.source_arrow));
            if seen_ids.insert(obligation_id.clone()) {
                let theory_subject_refs =
                    typed_subject_refs_for_relation(compiled_schema, &mapping.source_arrow);
                obligations.push(MigrationTransportObligationV1 {
                    operator: MigrationFunctorKindV1::DeltaF,
                    obligation_id,
                    obligation_kind: transport_kind_for_obligation(relation, None),
                    subject_ref: mapping.source_arrow.clone(),
                    theory_obligation_ref: None,
                    theory_subject_ref: primary_theory_subject_ref(&theory_subject_refs),
                    theory_subject_refs,
                    detail: format!(
                        "transport `{}` along target path `{}` and review relation/role attachments explicitly",
                        mapping.source_arrow, target_path
                    ),
                });
            }
            continue;
        }

        for (obligation, subject_refs) in matched {
            let obligation_id = format!(
                "transport:{}:{}",
                local_name(&mapping.source_arrow),
                obligation.stable_id()
            );
            if !seen_ids.insert(obligation_id.clone()) {
                continue;
            }
            obligations.push(MigrationTransportObligationV1 {
                operator: MigrationFunctorKindV1::DeltaF,
                obligation_id,
                obligation_kind: transport_kind_for_obligation(
                    relation,
                    Some(obligation.obligation_kind()),
                ),
                subject_ref: mapping.source_arrow.clone(),
                theory_obligation_ref: Some(obligation.clone()),
                theory_subject_ref: primary_theory_subject_ref(&subject_refs),
                theory_subject_refs: subject_refs,
                detail: format!(
                    "transport `{}` along target path `{}` while preserving theory obligation `{}`",
                    mapping.source_arrow,
                    target_path,
                    obligation.display_name()
                ),
            });
        }
    }

    obligations.sort_by(|left, right| left.obligation_id.cmp(&right.obligation_id));
    obligations
}

#[allow(dead_code)]
pub fn enrich_reconciliation_with_compiled_theory_v1(
    compiled_schema: &CompiledSchemaIr,
    theories: &[TheoryIr],
    reconciliation: &crate::accepted_plane::SemReconciliationV1,
) -> crate::accepted_plane::SemReconciliationV1 {
    let mut enriched = reconciliation.clone();
    for conflict in &mut enriched.conflicts {
        if conflict.artifact.theory_obligation_ref.is_some()
            || !conflict.artifact.theory_subject_refs.is_empty()
        {
            continue;
        }
        let (theory_obligation_ref, theory_subject_refs) = theory_handles_for_artifact(
            compiled_schema,
            theories,
            &conflict.artifact.artifact_kind,
            &conflict.artifact.artifact_id,
        );
        conflict.artifact.theory_obligation_ref = theory_obligation_ref;
        conflict.artifact.theory_subject_ref = primary_theory_subject_ref(&theory_subject_refs);
        conflict.artifact.theory_subject_refs = theory_subject_refs;
    }
    for decision in &mut enriched.decisions {
        if decision.artifact.theory_obligation_ref.is_some()
            || !decision.artifact.theory_subject_refs.is_empty()
        {
            continue;
        }
        let (theory_obligation_ref, theory_subject_refs) = theory_handles_for_artifact(
            compiled_schema,
            theories,
            &decision.artifact.artifact_kind,
            &decision.artifact.artifact_id,
        );
        decision.artifact.theory_obligation_ref = theory_obligation_ref;
        decision.artifact.theory_subject_ref = primary_theory_subject_ref(&theory_subject_refs);
        decision.artifact.theory_subject_refs = theory_subject_refs;
    }
    enriched
}

#[allow(dead_code)]
pub fn build_migration_evolution_preview_from_compiled_theory_v1(
    base_snapshot_id: Option<AcceptedSnapshotId>,
    candidate_label: String,
    morphism: &SchemaMorphismV1,
    source_schema: &SchemaV1,
    compiled_schema: &CompiledSchemaIr,
    theories: &[TheoryIr],
) -> EvolutionPreviewV1 {
    let transport_obligations = build_migration_transport_obligations_from_compiled_theory_v1(
        compiled_schema,
        theories,
        morphism,
    );
    build_migration_evolution_preview_v1(
        base_snapshot_id,
        candidate_label,
        morphism,
        source_schema,
        &transport_obligations,
    )
}

#[allow(dead_code)]
pub fn build_reconciliation_evolution_preview_from_compiled_theory_v1(
    base_snapshot_id: Option<AcceptedSnapshotId>,
    compiled_schema: &CompiledSchemaIr,
    theories: &[TheoryIr],
    reconciliation: &crate::accepted_plane::SemReconciliationV1,
) -> EvolutionPreviewV1 {
    let enriched =
        enrich_reconciliation_with_compiled_theory_v1(compiled_schema, theories, reconciliation);
    build_reconciliation_evolution_preview_v1(base_snapshot_id, &enriched)
}

fn reconciliation_refinement_candidates(
    reconciliation: &crate::accepted_plane::SemReconciliationV1,
) -> Vec<crate::typed_refinement::RuntimeRefinementCandidateV1> {
    let mut candidates = Vec::new();
    for conflict in &reconciliation.conflicts {
        let decided = reconciliation.decisions.iter().any(|decision| {
            decision.artifact.artifact_kind == conflict.artifact.artifact_kind
                && decision.artifact.artifact_id == conflict.artifact.artifact_id
        });
        if decided {
            continue;
        }
        for resolution in ["prefer_left", "prefer_right", "manual_review"] {
            candidates.push(
                crate::typed_refinement::RuntimeRefinementCandidateV1::new_reconciliation(
                    format!(
                        "resolve {} `{}` via `{}`",
                        conflict.artifact.artifact_kind, conflict.artifact.artifact_id, resolution
                    ),
                    crate::typed_refinement::ReconciliationRefinementOpV1::ResolveConflictByDecision {
                        reconciliation_id: reconciliation.reconciliation_id.to_string(),
                        artifact_kind: conflict.artifact.artifact_kind.clone(),
                        artifact_id: conflict.artifact.artifact_id.clone(),
                        resolution: resolution.to_string(),
                        theory_obligation_ref: conflict.artifact.theory_obligation_ref.clone(),
                        theory_subject_ref: primary_theory_subject_ref(
                            &conflict.artifact.theory_subject_refs,
                        ),
                        theory_subject_refs: conflict.artifact.theory_subject_refs.clone(),
                    },
                    conflict.artifact.artifact_id.clone(),
                    resolution.to_string(),
                    conflict.artifact.theory_obligation_ref.clone(),
                    conflict.artifact.theory_subject_refs.clone(),
                ),
            );
        }
    }
    candidates
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
    let trust_delta = trust_delta_from_preview(trust, competency_gate);
    let trust_summary = trust_summary_from_preview(trust, runtime_semantics.as_ref());
    let rule_summary = rule_summary_from_preview(runtime_semantics.as_ref());
    let coverage_summary =
        coverage_summary_from_preview(&trust_delta, competency_gate, runtime_semantics.as_ref());
    let exploration_next_actions =
        exploration_next_actions_from_primitives(&typed_change.primitives);
    let residual_obligations = collect_residual_obligations(
        quality_delta.summary.error_count,
        competency_gate,
        extra_residual_obligations,
    );

    EvolutionPreviewV1 {
        version: EVOLUTION_PREVIEW_VERSION_V1.to_string(),
        kind: kind.to_string(),
        base_snapshot_id,
        candidate_label,
        semantic_delta: semantic_delta_from_typed_change(&typed_change),
        typed_change,
        quality_delta: quality_delta.clone(),
        competency_gate: competency_gate.cloned(),
        trust: trust.clone(),
        trust_summary,
        runtime_semantics,
        rule_summary,
        coverage_summary,
        trust_delta,
        exploration_next_actions,
        residual_obligations,
        refinement_candidates: Vec::new(),
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
        primitives: Vec::new(),
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
        primitives: Vec::new(),
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

#[allow(dead_code)]
pub fn migration_typed_change_summary(
    morphism: &SchemaMorphismV1,
    source_schema: &SchemaV1,
    transport_obligations: &[MigrationTransportObligationV1],
) -> TypedChangeSummaryV1 {
    let morphism_id = format!("{}->{}", morphism.source_schema, morphism.target_schema);
    let mut subjects: BTreeSet<String> = BTreeSet::from([
        morphism.source_schema.clone(),
        morphism.target_schema.clone(),
    ]);
    let mut primitives = vec![EvolutionPrimitiveV1::TransportAlongSchemaMorphism {
        operator: MigrationFunctorKindV1::DeltaF,
        morphism_id: morphism_id.clone(),
        source_schema: morphism.source_schema.clone(),
        target_schema: morphism.target_schema.clone(),
        object_mappings: morphism.objects.len(),
        arrow_mappings: morphism.arrows.len(),
        rationale: Some(
            "schema evolution is interpreted as transport/reindexing along an explicit schema morphism"
                .to_string(),
        ),
    }];

    let object_images: HashMap<&str, &str> = morphism
        .objects
        .iter()
        .map(|mapping| {
            subjects.insert(mapping.source_object.clone());
            subjects.insert(mapping.target_object.clone());
            (
                mapping.source_object.as_str(),
                mapping.target_object.as_str(),
            )
        })
        .collect();

    let mut merged_images: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for mapping in &morphism.objects {
        merged_images
            .entry(mapping.target_object.clone())
            .or_default()
            .push(mapping.source_object.clone());
    }

    let merged_type_images = merged_images
        .values()
        .filter(|sources| sources.len() > 1)
        .count();
    for (target_object, mut source_objects) in merged_images {
        if source_objects.len() <= 1 {
            continue;
        }
        source_objects.sort();
        primitives.push(EvolutionPrimitiveV1::MergeTypesUnderSupertype {
            supertype: target_object,
            merged_types: source_objects.clone(),
            affected_subject_refs: Vec::new(),
            rationale: Some(
                "multiple source objects are identified by the schema morphism and therefore require explicit review of lost distinctions"
                    .to_string(),
            ),
        });
    }

    let mut collapsed_subtypes_total = 0usize;
    for subtype in &source_schema.subtypes {
        subjects.insert(subtype.sub.clone());
        subjects.insert(subtype.sup.clone());
        let Some(sub_img) = object_images.get(subtype.sub.as_str()) else {
            continue;
        };
        let Some(sup_img) = object_images.get(subtype.sup.as_str()) else {
            continue;
        };
        if sub_img == sup_img {
            collapsed_subtypes_total += 1;
            primitives.push(EvolutionPrimitiveV1::GeneralizeToSupertype {
                from: subtype.sub.clone(),
                to: subtype.sup.clone(),
                affected_subject_refs: vec![subtype.sub.clone(), subtype.sup.clone()],
                rationale: Some(format!(
                    "transport collapses subtype `{}` into `{}` because both map to `{}`",
                    subtype.sub, subtype.sup, sub_img
                )),
            });
        }
    }

    let mut identity_arrow_mappings = 0usize;
    let mut nontrivial_arrow_transport_total = 0usize;
    for mapping in &morphism.arrows {
        subjects.insert(mapping.source_arrow.clone());
        let is_identity =
            mapping.target_path.len() == 1 && mapping.target_path[0] == mapping.source_arrow;
        if is_identity {
            identity_arrow_mappings += 1;
            continue;
        }
        nontrivial_arrow_transport_total += 1;
        primitives.push(EvolutionPrimitiveV1::AddPathEquation {
            equation_id: format!("transport/{}", mapping.source_arrow),
            start_box: None,
            end_box: None,
            lhs_steps: 1,
            rhs_steps: mapping.target_path.len(),
            rationale: Some(format!(
                "schema morphism sends `{}` to target path `{}` and therefore requires explicit transport review",
                mapping.source_arrow,
                if mapping.target_path.is_empty() {
                    "identity".to_string()
                } else {
                    mapping.target_path.join(" ; ")
                }
            )),
        });
    }

    let mut counts = BTreeMap::new();
    counts.insert("object_mappings_total".to_string(), morphism.objects.len());
    counts.insert("arrow_mappings_total".to_string(), morphism.arrows.len());
    counts.insert(
        "transport_obligations_total".to_string(),
        transport_obligations.len(),
    );
    counts.insert(
        "identity_arrow_mappings_total".to_string(),
        identity_arrow_mappings,
    );
    counts.insert(
        "nontrivial_arrow_transport_total".to_string(),
        nontrivial_arrow_transport_total,
    );
    counts.insert(
        "collapsed_subtypes_total".to_string(),
        collapsed_subtypes_total,
    );
    counts.insert("merged_type_images_total".to_string(), merged_type_images);
    counts.insert(
        "source_subtypes_total".to_string(),
        source_schema.subtypes.len(),
    );

    TypedChangeSummaryV1 {
        kind: "schema_transport_delta".to_string(),
        subjects: subjects.into_iter().collect(),
        primitives,
        counts,
        notes: vec![
            "migration preview interprets schema evolution as transport/reindexing along an explicit schema morphism"
                .to_string(),
            "nontrivial arrow mappings are surfaced as transported path equations rather than hidden adapter logic"
                .to_string(),
            "this preview is runtime-checked and obligation-oriented; it is not yet a certified proof of functorial migration completeness"
                .to_string(),
        ],
        schema: TypedChangeBucketV1 {
            added: merged_type_images + collapsed_subtypes_total,
            reused: morphism.objects.len() + morphism.arrows.len() + source_schema.subtypes.len(),
            notes: vec![
                "schema bucket counts transported object/arrow/subtype structure touched by the morphism"
                    .to_string(),
            ],
            ..TypedChangeBucketV1::default()
        },
        theory: TypedChangeBucketV1 {
            added: nontrivial_arrow_transport_total,
            notes: vec![
                "theory bucket counts explicit path-equation obligations induced by non-identity arrow transport"
                    .to_string(),
            ],
            ..TypedChangeBucketV1::default()
        },
        instance: TypedChangeBucketV1 {
            notes: vec![
                "instance effects are previewed through residual transport obligations rather than by replaying a concrete instance migration here"
                    .to_string(),
            ],
            ..TypedChangeBucketV1::default()
        },
        context: TypedChangeBucketV1 {
            reused: transport_obligations
                .iter()
                .filter(|obligation| artifact_kind_layer(&obligation.obligation_kind) == "context")
                .count(),
            notes: vec![
                "context bucket records obligations explicitly marked as world/context/time transport work"
                    .to_string(),
            ],
            ..TypedChangeBucketV1::default()
        },
    }
}

#[allow(dead_code)]
pub fn build_migration_evolution_preview_v1(
    base_snapshot_id: Option<AcceptedSnapshotId>,
    candidate_label: String,
    morphism: &SchemaMorphismV1,
    source_schema: &SchemaV1,
    transport_obligations: &[MigrationTransportObligationV1],
) -> EvolutionPreviewV1 {
    let typed_change =
        migration_typed_change_summary(morphism, source_schema, transport_obligations);
    let quality_delta = synthetic_quality_report(
        "migration_preview",
        "schema_transport_preview",
        transport_obligations.len(),
        typed_change.primitives.len(),
    );
    let trust = crate::proposals_validate::ProposalValidationTrustContractV1 {
        trust_class: "runtime_checked_migration_preview".to_string(),
        soundness: "schema_morphism_transport_preview_runtime_checked".to_string(),
        coverage: "schema_morphism_plus_transport_obligation_inventory".to_string(),
        scope: "schema_transport_preview".to_string(),
        reasons: vec![
            "preview is derived from an explicit SchemaMorphismV1 and source schema".to_string(),
            "transport obligations remain explicit residual work rather than hidden adapter behavior"
                .to_string(),
            "preview does not claim left/right adjoint completeness or certified migration soundness"
                .to_string(),
        ],
    };
    let mut preview = build_evolution_preview_v1(
        "migration_preview",
        base_snapshot_id,
        candidate_label,
        typed_change,
        &quality_delta,
        None,
        &trust,
        None,
        transport_obligations.iter().map(|obligation| {
            format!(
                "{:?} {} [{}] {}: {}",
                obligation.operator,
                obligation.obligation_id,
                obligation.obligation_kind,
                obligation.subject_ref,
                obligation.detail
            )
        }),
        transport_obligations.is_empty(),
    );
    preview.refinement_candidates = migration_refinement_candidates(transport_obligations);
    preview
}

#[allow(dead_code)]
pub fn reconciliation_typed_change_summary(
    reconciliation: &crate::accepted_plane::SemReconciliationV1,
) -> TypedChangeSummaryV1 {
    let mut subjects: BTreeSet<String> = BTreeSet::from([
        reconciliation.base_commit_id.to_string(),
        reconciliation.left_commit_id.to_string(),
        reconciliation.right_commit_id.to_string(),
        reconciliation.reconciliation_id.to_string(),
    ]);
    let mut schema_touched = 0usize;
    let mut theory_touched = 0usize;
    let mut instance_touched = 0usize;
    let mut context_touched = 0usize;

    let mut counts = BTreeMap::new();
    counts.insert(
        "conflicts_total".to_string(),
        reconciliation.conflicts.len(),
    );
    counts.insert(
        "decisions_total".to_string(),
        reconciliation.decisions.len(),
    );

    let mut unresolved = Vec::new();
    let decision_map: HashMap<(&str, &str), &str> = reconciliation
        .decisions
        .iter()
        .map(|decision| {
            (
                (
                    decision.artifact.artifact_kind.as_str(),
                    decision.artifact.artifact_id.as_str(),
                ),
                decision.resolution.as_str(),
            )
        })
        .collect();

    for conflict in &reconciliation.conflicts {
        subjects.insert(conflict.artifact.artifact_id.clone());
        match artifact_kind_layer(&conflict.artifact.artifact_kind) {
            "schema" => schema_touched += 1,
            "theory" => theory_touched += 1,
            "context" => context_touched += 1,
            _ => instance_touched += 1,
        }
        if !decision_map.contains_key(&(
            conflict.artifact.artifact_kind.as_str(),
            conflict.artifact.artifact_id.as_str(),
        )) {
            unresolved.push(format!(
                "{} `{}`: {}",
                conflict.artifact.artifact_kind, conflict.artifact.artifact_id, conflict.detail
            ));
        }
    }

    let mut primitives = Vec::new();
    for decision in &reconciliation.decisions {
        subjects.insert(decision.artifact.artifact_id.clone());
        match artifact_kind_layer(&decision.artifact.artifact_kind) {
            "schema" => schema_touched += 1,
            "theory" => theory_touched += 1,
            "context" => context_touched += 1,
            _ => instance_touched += 1,
        }
        primitives.push(EvolutionPrimitiveV1::ResolveConflictByDecision {
            artifact_kind: decision.artifact.artifact_kind.clone(),
            artifact_id: decision.artifact.artifact_id.clone(),
            resolution: decision.resolution.clone(),
            rationale: Some(format!(
                "semantic reconciliation `{}` recorded an explicit decision under policy `{}`",
                reconciliation.reconciliation_id, reconciliation.policy
            )),
        });
    }

    counts.insert("unresolved_conflicts_total".to_string(), unresolved.len());
    counts.insert("schema_touched_total".to_string(), schema_touched);
    counts.insert("theory_touched_total".to_string(), theory_touched);
    counts.insert("instance_touched_total".to_string(), instance_touched);
    counts.insert("context_touched_total".to_string(), context_touched);

    TypedChangeSummaryV1 {
        kind: "semantic_reconciliation_delta".to_string(),
        subjects: subjects.into_iter().collect(),
        primitives,
        counts,
        notes: vec![
            "reconciliation preview is derived from persisted conflicts and explicit operator decisions"
                .to_string(),
            "the preview reports which semantic layers are touched, but it does not claim merge completeness or automatic categorical reconciliation"
                .to_string(),
        ],
        schema: TypedChangeBucketV1 {
            reused: schema_touched,
            notes: vec![
                "schema bucket counts schema-layer artifacts reviewed or resolved during reconciliation"
                    .to_string(),
            ],
            ..TypedChangeBucketV1::default()
        },
        theory: TypedChangeBucketV1 {
            reused: theory_touched,
            notes: vec![
                "theory bucket counts constraints/rewrite/path artifacts touched by reconciliation"
                    .to_string(),
            ],
            ..TypedChangeBucketV1::default()
        },
        instance: TypedChangeBucketV1 {
            reused: instance_touched,
            notes: vec![
                "instance bucket counts fact/module artifacts touched by reconciliation"
                    .to_string(),
            ],
            ..TypedChangeBucketV1::default()
        },
        context: TypedChangeBucketV1 {
            reused: context_touched,
            notes: vec![
                "context bucket counts world/time/context artifacts touched by reconciliation"
                    .to_string(),
            ],
            ..TypedChangeBucketV1::default()
        },
    }
}

#[allow(dead_code)]
pub fn build_reconciliation_evolution_preview_v1(
    base_snapshot_id: Option<AcceptedSnapshotId>,
    reconciliation: &crate::accepted_plane::SemReconciliationV1,
) -> EvolutionPreviewV1 {
    let typed_change = reconciliation_typed_change_summary(reconciliation);
    let unresolved_conflicts = reconciliation
        .conflicts
        .iter()
        .filter(|conflict| {
            !reconciliation.decisions.iter().any(|decision| {
                decision.artifact.artifact_kind == conflict.artifact.artifact_kind
                    && decision.artifact.artifact_id == conflict.artifact.artifact_id
            })
        })
        .map(|conflict| {
            format!(
                "{} `{}`: {}",
                conflict.artifact.artifact_kind, conflict.artifact.artifact_id, conflict.detail
            )
        })
        .collect::<Vec<_>>();
    let quality_delta = synthetic_quality_report(
        "semantic_reconciliation_preview",
        "semantic_reconciliation",
        unresolved_conflicts.len(),
        reconciliation.decisions.len(),
    );
    let trust = crate::proposals_validate::ProposalValidationTrustContractV1 {
        trust_class: "runtime_checked_reconciliation_preview".to_string(),
        soundness: "reconciliation_records_summarized_runtime".to_string(),
        coverage: "persisted_conflicts_plus_decisions".to_string(),
        scope: "semantic_reconciliation_preview".to_string(),
        reasons: vec![
            "preview is derived from persisted SemReconciliationV1 conflict/decision records"
                .to_string(),
            "operator decisions remain explicit; unresolved conflicts are carried forward as residual obligations"
                .to_string(),
            "preview does not prove merge optimality, completeness, or ontology closure".to_string(),
        ],
    };
    let mut preview = build_evolution_preview_v1(
        "semantic_reconciliation_preview",
        base_snapshot_id,
        format!("reconciliation:{}", reconciliation.reconciliation_id),
        typed_change,
        &quality_delta,
        None,
        &trust,
        None,
        unresolved_conflicts,
        reconciliation.conflicts.len() == reconciliation.decisions.len(),
    );
    preview.refinement_candidates = reconciliation_refinement_candidates(reconciliation);
    preview
}

pub fn compiled_ir_exploration_typed_change_summary(
    compiled_ir: &CompiledSchemaIr,
) -> TypedChangeSummaryV1 {
    let mut subjects: BTreeSet<String> = BTreeSet::from([compiled_ir.schema_id.to_string()]);
    let mut primitives = Vec::new();
    let mut relation_object_candidates = 0usize;
    let mut dependent_family_candidates = 0usize;
    let mut carrier_lift_candidates = 0usize;
    let mut rewrite_candidates = 0usize;
    let mut introduced_subtypes = 0usize;
    let mut split_candidates = 0usize;
    let mut factor_candidates = 0usize;

    let mut relations = compiled_ir.relations.values().collect::<Vec<_>>();
    relations.sort_by(|left, right| left.name.cmp(&right.name));

    for relation in relations {
        subjects.insert(relation.name.clone());
        let role_names = relation
            .roles
            .iter()
            .map(|role| role.name.clone())
            .collect::<Vec<_>>();
        let index_roles = relation
            .roles
            .iter()
            .filter(|role| matches!(role.kind, RoleKind::Context | RoleKind::Temporal))
            .map(|role| role.name.clone())
            .collect::<Vec<_>>();
        let fiber_roles = relation
            .carrier
            .as_ref()
            .map(|carrier| {
                carrier
                    .fiber_roles
                    .iter()
                    .filter_map(|role_idx| relation.roles.get(*role_idx as usize))
                    .map(|role| role.name.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let needs_explicit_relation_object =
            relation.roles.len() > 2 || !index_roles.is_empty() || !fiber_roles.is_empty();
        if needs_explicit_relation_object {
            relation_object_candidates += 1;
            primitives.push(EvolutionPrimitiveV1::ReifyRelationObject {
                relation: relation.name.clone(),
                tuple_type: relation.tuple_type_name.clone(),
                roles: role_names.clone(),
                rationale: Some(
                    "compiled IR exposes higher-arity or indexed structure that should remain explicit as a relation-object for authoring, query, and backend projection"
                        .to_string(),
                ),
            });
        }
        if !index_roles.is_empty() || !fiber_roles.is_empty() {
            dependent_family_candidates += 1;
            primitives.push(EvolutionPrimitiveV1::IntroduceDependentRelationFamily {
                relation: relation.name.clone(),
                tuple_type: relation.tuple_type_name.clone(),
                index_roles: index_roles.clone(),
                fiber_roles: fiber_roles.clone(),
                rationale: Some(
                    "compiled carrier/fiber semantics expose an indexed relation family rather than a plain binary edge"
                        .to_string(),
                ),
            });
        }
        if !fiber_roles.is_empty() {
            carrier_lift_candidates += 1;
            primitives.push(EvolutionPrimitiveV1::LiftRelationToCarrier {
                relation: relation.name.clone(),
                carrier_type: relation.tuple_type_name.clone(),
                roles: fiber_roles.clone(),
                rationale: Some(
                    "fiber roles should stay on the explicit carrier/tuple object instead of being erased into edge metadata"
                        .to_string(),
                ),
            });
        }
        if matches!(relation.witness_view, WitnessViewIr::Homotopy { .. }) {
            rewrite_candidates += 1;
            primitives.push(EvolutionPrimitiveV1::AddRewriteRule {
                rule_id: format!("candidate_rewrite_{}", relation.name),
                scope: relation.tuple_type_name.clone(),
                rationale: Some(
                    "compiled witness view is homotopy-shaped; review whether it should become an explicit path equivalence or rewrite rule"
                        .to_string(),
                ),
            });
        }
    }

    for family in compiled_ir.direct_subtype_families() {
        subjects.insert(family.supertype.clone());
        for subtype in &family.subtypes {
            subjects.insert(subtype.clone());
            introduced_subtypes += 1;
            primitives.push(EvolutionPrimitiveV1::IntroduceSubtype {
                sub: subtype.clone(),
                sup: family.supertype.clone(),
                affected_subject_refs: Vec::new(),
                rationale: Some(
                    "compiled subtype lattice already contains this immediate refinement; review whether query/migration surfaces expose it explicitly"
                        .to_string(),
                ),
            });
        }
        if family.subtypes.len() > 1 {
            split_candidates += 1;
            primitives.push(EvolutionPrimitiveV1::SplitTypeIntoSubtypes {
                source: family.supertype.clone(),
                subtypes: family.subtypes.clone(),
                discriminator: None,
                affected_subject_refs: Vec::new(),
                rationale: Some(
                    "compiled subtype lattice exposes a reviewable family of immediate refinements; treat the split as an explicit evolution move rather than implicit metadata"
                        .to_string(),
                ),
            });

            let projection =
                compiled_ir.subtype_role_projection(&family.supertype, &family.subtypes);
            factor_candidates += 1;
            primitives.push(EvolutionPrimitiveV1::FactorCommonStructureToSupertype {
                supertype: family.supertype,
                source_types: family.subtypes,
                relations: projection.relations,
                fields: projection.fields,
                rationale: Some(
                    "compiled subtype families and their attached relation roles should stay reviewable as explicit factoring candidates"
                        .to_string(),
                ),
            });
        }
    }

    let mut counts = BTreeMap::new();
    counts.insert(
        "object_types_total".to_string(),
        compiled_ir.object_types.len(),
    );
    counts.insert("relations_total".to_string(), compiled_ir.relations.len());
    counts.insert(
        "relation_object_candidates_total".to_string(),
        relation_object_candidates,
    );
    counts.insert(
        "dependent_family_candidates_total".to_string(),
        dependent_family_candidates,
    );
    counts.insert(
        "carrier_lift_candidates_total".to_string(),
        carrier_lift_candidates,
    );
    counts.insert("rewrite_candidates_total".to_string(), rewrite_candidates);
    counts.insert("introduced_subtypes_total".to_string(), introduced_subtypes);
    counts.insert("split_candidates_total".to_string(), split_candidates);
    counts.insert(
        "supertype_factor_candidates_total".to_string(),
        factor_candidates,
    );

    TypedChangeSummaryV1 {
        kind: "compiled_ir_directed_exploration".to_string(),
        subjects: subjects.into_iter().collect(),
        primitives,
        counts,
        notes: vec![
            "exploration preview is derived from the compiled schema/category IR rather than from token-level heuristics"
                .to_string(),
            "candidate primitives are typed exploration moves for ontology engineering; they are not accepted mutations until reviewed"
                .to_string(),
            "this surface extends relation-object, indexed-family, and rewrite discovery beyond hand-authored olog fragments"
                .to_string(),
        ],
        schema: TypedChangeBucketV1 {
            added: relation_object_candidates
                + dependent_family_candidates
                + carrier_lift_candidates
                + introduced_subtypes
                + split_candidates
                + factor_candidates,
            reused: compiled_ir.object_types.len() + compiled_ir.relations.len(),
            notes: vec![
                "schema bucket counts candidate semantic structure surfaced directly from compiled IR"
                    .to_string(),
            ],
            ..TypedChangeBucketV1::default()
        },
        theory: TypedChangeBucketV1 {
            added: rewrite_candidates,
            notes: vec![
                "theory bucket counts candidate rewrite/equivalence rules suggested by homotopy-shaped witness views"
                    .to_string(),
            ],
            ..TypedChangeBucketV1::default()
        },
        instance: TypedChangeBucketV1 {
            notes: vec![
                "directed exploration is schema/theory-oriented here; instance-level effects appear only after review and migration"
                    .to_string(),
            ],
            ..TypedChangeBucketV1::default()
        },
        context: TypedChangeBucketV1 {
            added: dependent_family_candidates,
            notes: vec![
                "context bucket counts candidate indexed families whose semantics depend on explicit world/time axes"
                    .to_string(),
            ],
            ..TypedChangeBucketV1::default()
        },
    }
}

pub fn build_compiled_ir_exploration_evolution_preview_v1(
    compiled_ir: &CompiledSchemaIr,
) -> EvolutionPreviewV1 {
    let typed_change = compiled_ir_exploration_typed_change_summary(compiled_ir);
    let quality_delta = synthetic_quality_report(
        "compiled_ir_directed_exploration",
        "compiled_ir_directed_exploration",
        0,
        typed_change.primitives.len(),
    );
    let trust = crate::proposals_validate::ProposalValidationTrustContractV1 {
        trust_class: "compiled_ir_directed_exploration".to_string(),
        soundness: "compiled_schema_ir_structure_exploration".to_string(),
        coverage: "compiled_schema_relations_subtypes_and_witness_views".to_string(),
        scope: "schema_scoped_compiled_ir_exploration".to_string(),
        reasons: vec![
            "exploration is derived from compiled schema semantics rather than raw string heuristics"
                .to_string(),
            "preview remains suggestion-level and does not claim ontology closure or promotion readiness"
                .to_string(),
        ],
    };
    build_evolution_preview_v1(
        "compiled_ir_directed_exploration",
        None,
        format!("compiled_ir:{}", compiled_ir.schema_id),
        typed_change,
        &quality_delta,
        None,
        &trust,
        None,
        std::iter::empty::<String>(),
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sem_gate_summary_from_evolution_preview_is_compact_and_eq_safe() {
        let preview = build_evolution_preview_v1(
            "proposal_review",
            None,
            "fnv1a64:proposal-set".to_string(),
            TypedChangeSummaryV1 {
                kind: "proposal_delta".to_string(),
                instance: TypedChangeBucketV1 {
                    added: 1,
                    ..TypedChangeBucketV1::default()
                },
                ..TypedChangeSummaryV1::default()
            },
            &crate::quality::QualityReportV1 {
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
            Some(&crate::proposals_validate::CompetencyGateReportV1 {
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
            &crate::proposals_validate::ProposalValidationTrustContractV1 {
                trust_class: "preview_validated".to_string(),
                soundness: "preview_typechecked_and_quality_gated".to_string(),
                coverage: "proposal_delta_plus_competency_questions".to_string(),
                scope: "snapshot_scoped_preview".to_string(),
                reasons: vec!["preview is scoped to the proposal delta".to_string()],
            },
            Some(crate::semantic_claim::RuntimeSemanticSummaryV1 {
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
            ["author review".to_string()],
            true,
        );

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
        assert_eq!(
            summary
                .rule
                .as_ref()
                .expect("rule summary")
                .runtime_visible_rules,
            0
        );
        assert_eq!(
            summary
                .runtime_semantics
                .as_ref()
                .expect("runtime semantics")
                .ontology_closure_claim,
            "not_claimed"
        );
        assert_eq!(
            preview.semantic_delta.changed_layers,
            vec!["instance".to_string()]
        );
        assert_eq!(preview.coverage_summary.competency_questions_total, 2);
    }

    #[test]
    fn build_evolution_preview_derives_semantic_delta_and_rule_sidecars() {
        let preview = build_evolution_preview_v1(
            "proposal_review",
            Some(AcceptedSnapshotId::new("fnv1a64:base")),
            "candidate".to_string(),
            TypedChangeSummaryV1 {
                kind: "proposal_delta".to_string(),
                subjects: vec!["fnv1a64:proposal".to_string()],
                schema: TypedChangeBucketV1 {
                    added: 2,
                    ..TypedChangeBucketV1::default()
                },
                instance: TypedChangeBucketV1 {
                    added: 3,
                    reused: 1,
                    ..TypedChangeBucketV1::default()
                },
                notes: vec!["delta note".to_string()],
                ..TypedChangeSummaryV1::default()
            },
            &crate::quality::QualityReportV1 {
                version: "quality_report_v1".to_string(),
                generated_at_unix_secs: 7,
                input: "proposal".to_string(),
                profile: "strict".to_string(),
                plane: "both".to_string(),
                summary: crate::quality::QualitySummaryV1 {
                    error_count: 1,
                    warning_count: 0,
                    info_count: 0,
                },
                findings: Vec::new(),
            },
            None,
            &crate::proposals_validate::ProposalValidationTrustContractV1 {
                trust_class: "preview_validated".to_string(),
                soundness: "preview_typechecked".to_string(),
                coverage: "proposal_delta_only".to_string(),
                scope: "snapshot_scoped_preview".to_string(),
                reasons: vec!["preview delta only".to_string()],
            },
            Some(crate::semantic_claim::RuntimeSemanticSummaryV1 {
                version: crate::semantic_claim::RUNTIME_SEMANTIC_SUMMARY_VERSION_V1.to_string(),
                trust_class: "preview_validated".to_string(),
                soundness: "preview_typechecked".to_string(),
                coverage: "proposal_delta_only".to_string(),
                scope: "snapshot_scoped_preview".to_string(),
                completeness_claim: "not_claimed".to_string(),
                ontology_closure_claim: "not_claimed".to_string(),
                rule_inventory: crate::semantic_claim::SemanticRuleInventoryV1 {
                    total_rules: 4,
                    relation_constraints: 2,
                    rewrite_rules: 1,
                    named_block_constraints: 1,
                    runtime_checkable_rules: 2,
                    runtime_visible_rules: 3,
                    review_only_rules: 1,
                    relations_with_rules: 1,
                    theories_with_rules: 1,
                    relation_names: vec!["Edge".to_string()],
                    theory_names: vec!["DemoTheory".to_string()],
                    notes: vec!["rule note".to_string()],
                },
                semantic_coverage: crate::semantic_claim::SemanticCoverageSummaryV1 {
                    typed_fact_surface: "schema_scoped_fact_typing_present".to_string(),
                    structured_constraint_surface:
                        "partial_runtime_enforced_structured_constraints".to_string(),
                    rewrite_surface: "declared_runtime_visible".to_string(),
                    named_block_surface: "review_only".to_string(),
                    competency_surface: "not_configured".to_string(),
                    quality_surface: "preview_quality_delta".to_string(),
                    gaps: vec!["semantic gap".to_string()],
                },
                notes: Vec::new(),
            }),
            std::iter::empty::<String>(),
            false,
        );

        assert_eq!(preview.semantic_delta.delta_kind, "proposal_delta");
        assert_eq!(
            preview.semantic_delta.changed_layers,
            vec!["schema".to_string(), "instance".to_string()]
        );
        assert_eq!(preview.semantic_delta.total_added, 5);
        assert_eq!(preview.semantic_delta.total_reused, 1);
        assert_eq!(preview.trust_summary.coverage, "proposal_delta_only");
        assert_eq!(preview.rule_summary.total_rules, 4);
        assert_eq!(preview.rule_summary.rewrite_rules, 1);
        assert_eq!(
            preview.coverage_summary.typed_fact_surface,
            "schema_scoped_fact_typing_present"
        );
        assert_eq!(
            preview.coverage_summary.quality_surface,
            "preview_quality_delta"
        );
        assert_eq!(preview.coverage_summary.competency_questions_total, 0);
        assert!(
            preview
                .coverage_summary
                .gaps
                .iter()
                .any(|gap| gap.contains("semantic gap")),
            "expected runtime semantic gaps to survive in coverage summary"
        );
    }

    #[test]
    fn structural_evolution_primitives_drive_semantic_delta_layers() {
        let preview = build_evolution_preview_v1(
            "typed_authoring_review",
            Some(AcceptedSnapshotId::new("fnv1a64:base")),
            "olog-fragment".to_string(),
            TypedChangeSummaryV1 {
                kind: "schema_refinement".to_string(),
                subjects: vec!["PlantAsset".to_string(), "Pump".to_string()],
                primitives: vec![
                    EvolutionPrimitiveV1::introduce_subtype("Pump", "PlantAsset"),
                    EvolutionPrimitiveV1::PushRelationRoleToSubtype {
                        relation: "requiresCertification".to_string(),
                        role: "asset".to_string(),
                        from_supertype: "PlantAsset".to_string(),
                        to_subtype: "Pump".to_string(),
                        affected_subject_refs: vec!["pump-17".to_string()],
                        rationale: Some(
                            "certification applies only to rotating assets".to_string(),
                        ),
                    },
                ],
                notes: vec!["typed exploration introduced a narrower asset class".to_string()],
                ..TypedChangeSummaryV1::default()
            },
            &crate::quality::QualityReportV1 {
                version: "quality_report_v1".to_string(),
                generated_at_unix_secs: 11,
                input: "olog-fragment".to_string(),
                profile: "strict".to_string(),
                plane: "accepted".to_string(),
                summary: crate::quality::QualitySummaryV1 {
                    error_count: 0,
                    warning_count: 0,
                    info_count: 0,
                },
                findings: Vec::new(),
            },
            None,
            &crate::proposals_validate::ProposalValidationTrustContractV1 {
                trust_class: "preview_validated".to_string(),
                soundness: "typed_authoring_preview".to_string(),
                coverage: "schema_refinement_only".to_string(),
                scope: "snapshot_scoped_preview".to_string(),
                reasons: vec!["typed refinement over compiled schema IR".to_string()],
            },
            None,
            std::iter::empty::<String>(),
            true,
        );

        assert_eq!(preview.semantic_delta.delta_kind, "schema_refinement");
        assert_eq!(preview.semantic_delta.subject_refs.len(), 2);
        assert_eq!(
            preview.semantic_delta.primitives,
            preview.typed_change.primitives
        );
        assert_eq!(
            preview.semantic_delta.changed_layers,
            vec!["schema".to_string()]
        );
        assert!(
            preview
                .semantic_delta
                .primitives
                .iter()
                .any(|primitive| matches!(
                    primitive,
                    EvolutionPrimitiveV1::IntroduceSubtype { sub, sup, .. }
                    if sub == "Pump" && sup == "PlantAsset"
                )),
            "expected structural subtype introduction to survive in semantic delta"
        );
        assert!(
            preview
                .exploration_next_actions
                .iter()
                .any(|action| action.contains("`Pump <: PlantAsset`")),
            "expected subtype evolution to produce directed exploration guidance"
        );
        assert!(
            preview
                .exploration_next_actions
                .iter()
                .any(|action| action.contains("requiresCertification.asset")),
            "expected role push-down to produce directed exploration guidance"
        );
    }

    #[test]
    fn evolution_primitives_serialize_with_stable_kind_tags() {
        let primitives = vec![
            EvolutionPrimitiveV1::reify_relation_object(
                "WorksFor",
                "WorksFor",
                ["employee", "employer", "ctx"],
            ),
            EvolutionPrimitiveV1::introduce_dependent_relation_family(
                "WorksFor",
                "WorksFor",
                ["ctx"],
                std::iter::empty::<String>(),
            ),
            EvolutionPrimitiveV1::transport_along_schema_morphism(
                MigrationFunctorKindV1::DeltaF,
                "Plant->Ops",
                "Plant",
                "Ops",
                3,
                2,
            ),
            EvolutionPrimitiveV1::introduce_subtype("Pump", "PlantAsset"),
            EvolutionPrimitiveV1::generalize_to_supertype("CentrifugalPump", "Pump"),
            EvolutionPrimitiveV1::specialize_to_subtype("Equipment", "Pump"),
            EvolutionPrimitiveV1::push_relation_role_to_subtype(
                "requiresInspection",
                "asset",
                "Equipment",
                "Pump",
            ),
            EvolutionPrimitiveV1::pull_relation_role_to_supertype(
                "installedAt",
                "asset",
                "Pump",
                "Equipment",
            ),
            EvolutionPrimitiveV1::factor_common_structure_to_supertype(
                "RotatingEquipment",
                ["Pump", "Compressor"],
            ),
            EvolutionPrimitiveV1::split_type_into_subtypes(
                "Certification",
                ["PressureCertification", "SafetyCertification"],
            ),
            EvolutionPrimitiveV1::merge_types_under_supertype(
                "MaintenanceEvent",
                ["InspectionEvent", "RepairEvent"],
            ),
            EvolutionPrimitiveV1::lift_relation_to_carrier(
                "delivery_commitment",
                "DeliveryCommitmentFact",
                ["material", "supplier", "plant"],
            ),
            EvolutionPrimitiveV1::add_path_equation("eq_parent", 1, 1),
            EvolutionPrimitiveV1::add_rewrite_rule("normalize_parent", "DemoTheory"),
            EvolutionPrimitiveV1::resolve_conflict_by_decision(
                "rewrite_rule",
                "normalize_parent",
                "prefer_right",
            ),
        ];

        let json = serde_json::to_value(&primitives).expect("serialize evolution primitives");
        let kinds = json
            .as_array()
            .expect("array")
            .iter()
            .map(|value| value["kind"].as_str().expect("kind").to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                "reify_relation_object",
                "introduce_dependent_relation_family",
                "transport_along_schema_morphism",
                "introduce_subtype",
                "generalize_to_supertype",
                "specialize_to_subtype",
                "push_relation_role_to_subtype",
                "pull_relation_role_to_supertype",
                "factor_common_structure_to_supertype",
                "split_type_into_subtypes",
                "merge_types_under_supertype",
                "lift_relation_to_carrier",
                "add_path_equation",
                "add_rewrite_rule",
                "resolve_conflict_by_decision",
            ]
        );
        assert_eq!(
            json[0],
            json!({
                "kind": "reify_relation_object",
                "relation": "WorksFor",
                "tuple_type": "WorksFor",
                "roles": ["employee", "employer", "ctx"]
            })
        );
        assert_eq!(
            json[5],
            json!({
                "kind": "specialize_to_subtype",
                "from": "Equipment",
                "to": "Pump"
            })
        );
        assert_eq!(
            json[11],
            json!({
                "kind": "lift_relation_to_carrier",
                "relation": "delivery_commitment",
                "carrier_type": "DeliveryCommitmentFact",
                "roles": ["material", "supplier", "plant"]
            })
        );
        assert_eq!(
            json[12],
            json!({
                "kind": "add_path_equation",
                "equation_id": "eq_parent",
                "lhs_steps": 1,
                "rhs_steps": 1
            })
        );
        assert_eq!(
            json[13],
            json!({
                "kind": "add_rewrite_rule",
                "rule_id": "normalize_parent",
                "scope": "DemoTheory"
            })
        );
        assert_eq!(
            json[14],
            json!({
                "kind": "resolve_conflict_by_decision",
                "artifact_kind": "rewrite_rule",
                "artifact_id": "normalize_parent",
                "resolution": "prefer_right"
            })
        );
    }

    #[test]
    fn semantic_delta_from_typed_change_preserves_primitive_payloads() {
        let typed_change = TypedChangeSummaryV1 {
            kind: "schema_refinement".to_string(),
            subjects: vec!["PlantAsset".to_string(), "Pump".to_string()],
            primitives: vec![
                EvolutionPrimitiveV1::IntroduceSubtype {
                    sub: "Pump".to_string(),
                    sup: "PlantAsset".to_string(),
                    affected_subject_refs: vec!["pump-17".to_string()],
                    rationale: Some(
                        "typed refinement introduced a narrower asset class".to_string(),
                    ),
                },
                EvolutionPrimitiveV1::GeneralizeToSupertype {
                    from: "CentrifugalPump".to_string(),
                    to: "Pump".to_string(),
                    affected_subject_refs: vec!["pump-17".to_string(), "pump-18".to_string()],
                    rationale: Some("shared inspection policy moved to Pump".to_string()),
                },
            ],
            notes: vec!["delta note".to_string()],
            schema: TypedChangeBucketV1 {
                added: 1,
                reused: 2,
                ..TypedChangeBucketV1::default()
            },
            theory: TypedChangeBucketV1 {
                removed: 1,
                ..TypedChangeBucketV1::default()
            },
            ..TypedChangeSummaryV1::default()
        };

        let delta = semantic_delta_from_typed_change(&typed_change);

        assert_eq!(delta.delta_kind, typed_change.kind);
        assert_eq!(delta.subject_refs, typed_change.subjects);
        assert_eq!(delta.primitives, typed_change.primitives);
        assert_eq!(
            delta.changed_layers,
            vec!["schema".to_string(), "theory".to_string()]
        );
        assert_eq!(delta.schema, typed_change.schema);
        assert_eq!(delta.theory, typed_change.theory);
        assert_eq!(delta.total_added, 1);
        assert_eq!(delta.total_reused, 2);
        assert_eq!(delta.total_removed, 1);
        assert_eq!(delta.notes, typed_change.notes);
    }

    #[test]
    fn subtype_supertype_primitives_roundtrip_with_stable_serde_shape() {
        let generalize = EvolutionPrimitiveV1::GeneralizeToSupertype {
            from: "CentrifugalPump".to_string(),
            to: "Pump".to_string(),
            affected_subject_refs: vec!["pump-17".to_string()],
            rationale: Some("shared inspection policy moved to Pump".to_string()),
        };
        let specialize = EvolutionPrimitiveV1::SpecializeToSubtype {
            from: "Equipment".to_string(),
            to: "Pump".to_string(),
            affected_subject_refs: vec!["pump-17".to_string()],
            rationale: None,
        };

        let generalize_json = json!({
            "kind": "generalize_to_supertype",
            "from": "CentrifugalPump",
            "to": "Pump",
            "affected_subject_refs": ["pump-17"],
            "rationale": "shared inspection policy moved to Pump"
        });
        let specialize_json = json!({
            "kind": "specialize_to_subtype",
            "from": "Equipment",
            "to": "Pump",
            "affected_subject_refs": ["pump-17"]
        });

        assert_eq!(
            serde_json::to_value(&generalize).expect("serialize generalize_to_supertype"),
            generalize_json
        );
        assert_eq!(
            serde_json::to_value(&specialize).expect("serialize specialize_to_subtype"),
            specialize_json
        );
        assert_eq!(
            serde_json::from_value::<EvolutionPrimitiveV1>(generalize_json)
                .expect("deserialize generalize_to_supertype"),
            generalize
        );
        assert_eq!(
            serde_json::from_value::<EvolutionPrimitiveV1>(specialize_json)
                .expect("deserialize specialize_to_subtype"),
            specialize
        );
    }

    fn compiled_theory_migration_fixture(
    ) -> (SchemaV1, SchemaMorphismV1, CompiledSchemaIr, Vec<TheoryIr>) {
        let source_schema = axiograph_pathdb::migration::SchemaV1 {
            name: "Plant".to_string(),
            objects: vec![
                "PlantAsset".to_string(),
                "Pump".to_string(),
                "Compressor".to_string(),
            ],
            arrows: vec![axiograph_pathdb::migration::ArrowDeclV1 {
                name: "installed_at".to_string(),
                src: "PlantAsset".to_string(),
                dst: "PlantAsset".to_string(),
            }],
            subtypes: vec![
                axiograph_pathdb::migration::SubtypeDeclV1 {
                    sub: "Pump".to_string(),
                    sup: "PlantAsset".to_string(),
                    incl: "pump_incl".to_string(),
                },
                axiograph_pathdb::migration::SubtypeDeclV1 {
                    sub: "Compressor".to_string(),
                    sup: "PlantAsset".to_string(),
                    incl: "compressor_incl".to_string(),
                },
            ],
        };
        let morphism = axiograph_pathdb::migration::SchemaMorphismV1 {
            source_schema: "Plant".to_string(),
            target_schema: "Ops".to_string(),
            objects: vec![
                axiograph_pathdb::migration::ObjectMappingV1 {
                    source_object: "PlantAsset".to_string(),
                    target_object: "Equipment".to_string(),
                },
                axiograph_pathdb::migration::ObjectMappingV1 {
                    source_object: "Pump".to_string(),
                    target_object: "Equipment".to_string(),
                },
                axiograph_pathdb::migration::ObjectMappingV1 {
                    source_object: "Compressor".to_string(),
                    target_object: "Equipment".to_string(),
                },
            ],
            arrows: vec![axiograph_pathdb::migration::ArrowMappingV1 {
                source_arrow: "installed_at".to_string(),
                target_path: vec!["owned_by".to_string(), "located_at".to_string()],
            }],
        };
        let module = axiograph_dsl::axi_v1::parse_axi_v1(
            r#"
module Plant

schema Plant:
  object PlantAsset
  object Pump
  object Compressor
  object Context
  relation installed_at(asset: PlantAsset, site: PlantAsset, ctx: Context)
  subtype Pump < PlantAsset
  subtype Compressor < PlantAsset

theory PlantTransport on Plant:
  constraint key installed_at(asset, site, ctx)
"#,
        )
        .expect("parse transport theory");
        let compiled_schema = axiograph_pathdb::kernel_ir::compile_schema_ir(&module.schemas[0]);
        let theories = module
            .theories
            .iter()
            .map(|theory| {
                axiograph_pathdb::kernel_ir::compile_theory_ir(&compiled_schema, theory)
                    .expect("compile theory ir")
            })
            .collect::<Vec<_>>();

        (source_schema, morphism, compiled_schema, theories)
    }

    #[test]
    fn migration_preview_emits_transport_and_path_equation_primitives() {
        let (source_schema, morphism, compiled_schema, theories) =
            compiled_theory_migration_fixture();

        let preview = build_migration_evolution_preview_from_compiled_theory_v1(
            Some(AcceptedSnapshotId::new("fnv1a64:base")),
            "Plant->Ops".to_string(),
            &morphism,
            &source_schema,
            &compiled_schema,
            &theories,
        );

        assert_eq!(preview.kind, "migration_preview");
        assert_eq!(preview.typed_change.kind, "schema_transport_delta");
        assert!(
            preview.typed_change.primitives.iter().any(|primitive| matches!(
                primitive,
                EvolutionPrimitiveV1::TransportAlongSchemaMorphism { source_schema, target_schema, .. }
                if source_schema == "Plant" && target_schema == "Ops"
            ))
        );
        assert!(preview
            .typed_change
            .primitives
            .iter()
            .any(|primitive| matches!(
                primitive,
                EvolutionPrimitiveV1::AddPathEquation { equation_id, rhs_steps, .. }
                if equation_id == "transport/installed_at" && *rhs_steps == 2
            )));
        assert!(preview
            .typed_change
            .primitives
            .iter()
            .any(|primitive| matches!(
                primitive,
                EvolutionPrimitiveV1::GeneralizeToSupertype { from, to, .. }
                if from == "Pump" && to == "PlantAsset"
            )));
        assert!(
            preview
                .exploration_next_actions
                .iter()
                .any(|action| action.contains("schema morphism `Plant->Ops`")),
            "expected transport primitive to drive next-step guidance"
        );
        assert!(
            !preview.ok,
            "residual transport obligations should fail closed"
        );
        assert_eq!(
            preview.semantic_delta.changed_layers,
            vec![
                "schema".to_string(),
                "theory".to_string(),
                "context".to_string()
            ]
        );
        assert_eq!(preview.refinement_candidates.len(), 1);
        assert!(matches!(
            preview.refinement_candidates[0].handle.domain(),
            crate::typed_refinement::RuntimeRefinementDomainV1::MigrationAuthoring
        ));
        assert!(preview.refinement_candidates[0]
            .obligation_id
            .as_deref()
            .is_some_and(|id| id.contains("installed_at")));
        assert!(matches!(
            preview.refinement_candidates[0].theory_obligation_ref.as_ref(),
            Some(TheoryObligationRefIr::Constraint { relation_name, .. })
                if relation_name.as_deref() == Some("installed_at")
        ));
        assert!(preview.refinement_candidates[0]
            .theory_subject_refs
            .iter()
            .any(|subject| matches!(
                subject,
                TheorySubjectRefIr::Relation { relation_name, .. } if relation_name == "installed_at"
            )));
        assert!(matches!(
            preview.refinement_candidates[0].theory_subject_ref.as_ref(),
            Some(TheorySubjectRefIr::Relation { relation_name, .. }) if relation_name == "installed_at"
        ));
    }

    #[test]
    fn migration_refinement_apply_by_id_resolves_pending_transport_obligation() {
        let (source_schema, morphism, compiled_schema, theories) =
            compiled_theory_migration_fixture();
        let preview = build_migration_evolution_preview_from_compiled_theory_v1(
            Some(AcceptedSnapshotId::new("fnv1a64:base")),
            "Plant->Ops".to_string(),
            &morphism,
            &source_schema,
            &compiled_schema,
            &theories,
        );
        let handle_id = preview.refinement_candidates[0].handle.id.clone();

        let applied = apply_runtime_refinement_by_id_to_migration_preview_from_compiled_theory_v1(
            Some(AcceptedSnapshotId::new("fnv1a64:base")),
            "Plant->Ops".to_string(),
            &compiled_schema,
            &theories,
            &morphism,
            &source_schema,
            &handle_id,
        )
        .expect("apply migration refinement by id");

        assert_eq!(applied.transport_apply.handle.id, handle_id);
        assert_eq!(applied.transport_apply.base_transport_obligations.len(), 1);
        assert_eq!(
            applied
                .transport_apply
                .remaining_transport_obligations
                .len(),
            0
        );
        assert!(matches!(
            applied
                .transport_apply
                .resolved_transport_obligation
                .theory_obligation_ref
                .as_ref(),
            Some(TheoryObligationRefIr::Constraint { relation_name, .. })
                if relation_name.as_deref() == Some("installed_at")
        ));
        assert!(matches!(
            applied
                .transport_apply
                .resolved_transport_obligation
                .theory_subject_ref
                .as_ref(),
            Some(TheorySubjectRefIr::Relation { relation_name, .. }) if relation_name == "installed_at"
        ));
        assert!(applied.evolution_preview.ok);
        assert!(applied.evolution_preview.residual_obligations.is_empty());
        assert!(applied.evolution_preview.refinement_candidates.is_empty());
        assert_eq!(
            applied.evolution_preview.semantic_delta.changed_layers,
            vec!["schema".to_string(), "theory".to_string()]
        );
    }

    #[test]
    fn migration_refinement_apply_rejects_weakened_theory_subject_metadata() {
        let (_source_schema, morphism, compiled_schema, theories) =
            compiled_theory_migration_fixture();
        let transport_obligations = build_migration_transport_obligations_from_compiled_theory_v1(
            &compiled_schema,
            &theories,
            &morphism,
        );
        let candidate = migration_refinement_candidates(&transport_obligations)
            .into_iter()
            .next()
            .expect("migration refinement candidate");
        let weakened_op = match candidate.handle.payload.clone() {
            crate::typed_refinement::RuntimeRefinementPayloadV1::MigrationAuthoring { mut op } => {
                match &mut op {
                    crate::typed_refinement::MigrationRefinementOpV1::AddressTransportObligation {
                        theory_subject_refs,
                        ..
                    } => theory_subject_refs.clear(),
                }
                op
            }
            payload => panic!("unexpected payload for migration candidate: {payload:?}"),
        };
        let weakened_handle =
            crate::typed_refinement::RuntimeRefinementHandleV1::new_migration(weakened_op);

        let err = apply_runtime_refinement_handle_to_migration_transport_obligations(
            &transport_obligations,
            &weakened_handle,
        )
        .expect_err("reject weakened theory metadata");

        assert!(err.to_string().contains("mismatched theory subjects"));
    }

    #[test]
    fn reconciliation_preview_derives_layered_conflicts_and_decisions() {
        let module = axiograph_dsl::axi_v1::parse_axi_v1(
            r#"
module Demo

schema Demo:
  object Person
  relation WorksFor(employee: Person, employer: Person)
  relation Parent(parent: Person, child: Person)

theory DemoRules on Demo:
  constraint key WorksFor(employee, employer)
  rewrite normalize_parent:
    orientation: bidirectional
    vars: a: Person, b: Person, c: Person
    lhs: trans(step(a, Parent, b), step(b, Parent, c))
    rhs: step(a, Parent, c)
"#,
        )
        .expect("parse reconciliation theory");
        let compiled_schema = axiograph_pathdb::kernel_ir::compile_schema_ir(&module.schemas[0]);
        let theories = module
            .theories
            .iter()
            .map(|theory| {
                axiograph_pathdb::kernel_ir::compile_theory_ir(&compiled_schema, theory)
                    .expect("compile theory ir")
            })
            .collect::<Vec<_>>();
        let reconciliation = crate::accepted_plane::SemReconciliationV1 {
            version: "accepted_plane_sem_reconciliation_v1".to_string(),
            reconciliation_id: AxiDigest::new("fnv1a64:reconcile"),
            created_at_unix_secs: 0,
            base_commit_id: AxiDigest::new("fnv1a64:base"),
            left_commit_id: AxiDigest::new("fnv1a64:left"),
            right_commit_id: AxiDigest::new("fnv1a64:right"),
            policy: "cq_gated_merge".to_string(),
            source_ref_name: Some("heads/review/left".to_string()),
            target_ref_name: Some("heads/review/right".to_string()),
            resolved_ref_name: None,
            outcome_commit_id: None,
            conflicts: vec![
                crate::accepted_plane::SemConflictRecordV1 {
                    artifact: crate::accepted_plane::ArtifactRefV1 {
                        artifact_kind: "schema_relation".to_string(),
                        artifact_id: "WorksFor".to_string(),
                        theory_obligation_ref: None,
                        theory_subject_ref: None,
                        theory_subject_refs: Vec::new(),
                    },
                    detail: "left changes carrier roles, right changes subtype target".to_string(),
                },
                crate::accepted_plane::SemConflictRecordV1 {
                    artifact: crate::accepted_plane::ArtifactRefV1 {
                        artifact_kind: "rewrite_rule".to_string(),
                        artifact_id: "normalize_parent".to_string(),
                        theory_obligation_ref: None,
                        theory_subject_ref: None,
                        theory_subject_refs: Vec::new(),
                    },
                    detail: "conflicting normalization scopes".to_string(),
                },
            ],
            decisions: vec![crate::accepted_plane::SemDecisionRecordV1 {
                artifact: crate::accepted_plane::ArtifactRefV1 {
                    artifact_kind: "rewrite_rule".to_string(),
                    artifact_id: "normalize_parent".to_string(),
                    theory_obligation_ref: None,
                    theory_subject_ref: None,
                    theory_subject_refs: Vec::new(),
                },
                resolution: "prefer_right".to_string(),
            }],
            certificate_refs: Vec::new(),
        };

        let preview = build_reconciliation_evolution_preview_from_compiled_theory_v1(
            Some(AcceptedSnapshotId::new("fnv1a64:base")),
            &compiled_schema,
            &theories,
            &reconciliation,
        );

        assert_eq!(preview.kind, "semantic_reconciliation_preview");
        assert_eq!(preview.typed_change.kind, "semantic_reconciliation_delta");
        assert!(
            preview.typed_change.primitives.iter().any(|primitive| matches!(
                primitive,
                EvolutionPrimitiveV1::ResolveConflictByDecision { artifact_kind, artifact_id, resolution, .. }
                if artifact_kind == "rewrite_rule" && artifact_id == "normalize_parent" && resolution == "prefer_right"
            ))
        );
        assert!(
            preview
                .residual_obligations
                .iter()
                .any(|obligation| obligation.contains("WorksFor")),
            "unresolved schema conflicts should survive as residual obligations"
        );
        assert!(!preview.ok);
        assert_eq!(
            preview.semantic_delta.changed_layers,
            vec!["schema".to_string(), "theory".to_string()]
        );
        assert_eq!(preview.refinement_candidates.len(), 3);
        assert!(preview
            .refinement_candidates
            .iter()
            .all(|candidate| matches!(
                candidate.handle.domain(),
                crate::typed_refinement::RuntimeRefinementDomainV1::ReconciliationReview
            )));
        assert!(preview.refinement_candidates.iter().any(|candidate| {
            candidate.artifact_id.as_deref() == Some("WorksFor")
                && candidate.resolution.as_deref() == Some("prefer_left")
        }));
        assert!(preview.refinement_candidates.iter().any(|candidate| {
            candidate.artifact_id.as_deref() == Some("WorksFor")
                && candidate.theory_subject_refs.iter().any(|subject| matches!(
                    subject,
                    TheorySubjectRefIr::Relation { relation_name, .. } if relation_name == "WorksFor"
                ))
        }));
        assert!(preview.refinement_candidates.iter().any(|candidate| {
            candidate.artifact_id.as_deref() == Some("WorksFor")
                && matches!(
                    candidate.handle.payload,
                    crate::typed_refinement::RuntimeRefinementPayloadV1::ReconciliationReview {
                        op: crate::typed_refinement::ReconciliationRefinementOpV1::ResolveConflictByDecision {
                            theory_subject_ref: Some(TheorySubjectRefIr::Relation { ref relation_name, .. }),
                            ..
                        }
                    } if relation_name == "WorksFor"
                )
        }));
        assert!(!preview
            .refinement_candidates
            .iter()
            .any(|candidate| { candidate.artifact_id.as_deref() == Some("normalize_parent") }));
    }

    #[test]
    fn compiled_ir_exploration_preview_surfaces_relation_family_and_rewrite_candidates() {
        let module = axiograph_dsl::schema_v1::parse_schema_v1(
            r#"
module Demo

schema Plant:
  object PlantAsset
  object Pump
  object Compressor
  object Batch
  object Process
  object Context
  object Time
  subtype Pump <: PlantAsset
  subtype Compressor <: PlantAsset
  relation Certification(asset: Pump, batch: Batch, ctx: Context, time: Time)
  relation Maintenance(asset: Compressor, batch: Batch, ctx: Context, time: Time)
  relation ProcessEquiv(lhs: Process, rhs: Process)

instance PlantInst of Plant:
  PlantAsset = {asset_a}
  Pump = {pump_a}
  Compressor = {compressor_a}
  Batch = {batch_a}
  Process = {proc_a}
  Context = {ctx_a}
  Time = {t_a}
"#,
        )
        .expect("parse schema");
        let schema = module
            .schemas
            .iter()
            .find(|schema| schema.name == "Plant")
            .unwrap();
        let compiled_ir = axiograph_pathdb::kernel_ir::compile_schema_ir(schema);

        let preview = build_compiled_ir_exploration_evolution_preview_v1(&compiled_ir);

        assert_eq!(preview.kind, "compiled_ir_directed_exploration");
        assert_eq!(
            preview.typed_change.kind,
            "compiled_ir_directed_exploration"
        );
        assert!(preview
            .typed_change
            .primitives
            .iter()
            .any(|primitive| matches!(
                primitive,
                EvolutionPrimitiveV1::ReifyRelationObject { relation, .. }
                if relation == "Certification"
            )));
        assert!(preview
            .typed_change
            .primitives
            .iter()
            .any(|primitive| matches!(
                primitive,
                EvolutionPrimitiveV1::IntroduceDependentRelationFamily { relation, .. }
                if relation == "Certification"
            )));
        assert!(preview
            .typed_change
            .primitives
            .iter()
            .any(|primitive| matches!(
                primitive,
                EvolutionPrimitiveV1::AddRewriteRule { rule_id, .. }
                if rule_id == "candidate_rewrite_ProcessEquiv"
            )));
        assert!(preview
            .typed_change
            .primitives
            .iter()
            .any(|primitive| matches!(
                primitive,
                EvolutionPrimitiveV1::IntroduceSubtype { sub, sup, .. }
                if sub == "Pump" && sup == "PlantAsset"
            )));
        assert!(preview
            .typed_change
            .primitives
            .iter()
            .any(|primitive| matches!(
                primitive,
                EvolutionPrimitiveV1::SplitTypeIntoSubtypes { source, subtypes, .. }
                if source == "PlantAsset"
                    && subtypes.iter().any(|t| t == "Pump")
                    && subtypes.iter().any(|t| t == "Compressor")
            )));
        assert!(
            preview.typed_change.primitives.iter().any(|primitive| matches!(
                primitive,
                EvolutionPrimitiveV1::FactorCommonStructureToSupertype { supertype, source_types, .. }
                if supertype == "PlantAsset"
                    && source_types.iter().any(|t| t == "Pump")
                    && source_types.iter().any(|t| t == "Compressor")
            ))
        );
        assert!(preview
            .typed_change
            .primitives
            .iter()
            .any(|primitive| matches!(
                primitive,
                EvolutionPrimitiveV1::FactorCommonStructureToSupertype {
                    relations,
                    fields,
                    ..
                }
                if relations.iter().any(|relation| relation == "Certification")
                    && relations.iter().any(|relation| relation == "Maintenance")
                    && fields.iter().any(|field| field == "Certification.asset")
                    && fields.iter().any(|field| field == "Maintenance.asset")
            )));
        assert!(
            preview
                .exploration_next_actions
                .iter()
                .any(|action| action.contains("indexed relation family `Certification`")),
            "expected dependent-family discovery to drive next actions"
        );
        assert!(preview
            .exploration_next_actions
            .iter()
            .any(|action| action.contains("splitting `PlantAsset` into `Compressor`, `Pump`")));
    }
}
