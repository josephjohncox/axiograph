use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use axiograph_pathdb::axi_semantics::{ConstraintDecl, MetaPlaneIndex};

use crate::proposals_validate::{CompetencyGateReportV1, ProposalValidationTrustContractV1};

pub const RUNTIME_SEMANTIC_SUMMARY_VERSION_V1: &str = "runtime_semantic_summary_v1";

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SemanticRuleInventoryV1 {
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
    pub relation_names: Vec<String>,
    #[serde(default)]
    pub theory_names: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SemanticCoverageSummaryV1 {
    pub typed_fact_surface: String,
    pub structured_constraint_surface: String,
    pub rewrite_surface: String,
    pub named_block_surface: String,
    pub competency_surface: String,
    pub quality_surface: String,
    #[serde(default)]
    pub gaps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RuntimeSemanticSummaryV1 {
    pub version: String,
    pub trust_class: String,
    pub soundness: String,
    pub coverage: String,
    pub scope: String,
    pub completeness_claim: String,
    pub ontology_closure_claim: String,
    pub rule_inventory: SemanticRuleInventoryV1,
    pub semantic_coverage: SemanticCoverageSummaryV1,
    #[serde(default)]
    pub notes: Vec<String>,
}

pub fn summarize_rule_inventory(meta: &MetaPlaneIndex) -> SemanticRuleInventoryV1 {
    let mut relation_names = BTreeSet::new();
    let mut theory_names = BTreeSet::new();
    let mut relation_constraints = 0usize;
    let mut rewrite_rules = 0usize;
    let mut named_block_constraints = 0usize;
    let mut runtime_checkable_rules = 0usize;
    let mut review_only_rules = 0usize;

    for schema in meta.schemas.values() {
        for (relation, decls) in &schema.constraints_by_relation {
            if !decls.is_empty() {
                relation_names.insert(relation.clone());
            }
            relation_constraints += decls.len();
            for decl in decls {
                match decl {
                    ConstraintDecl::NamedBlock { .. } | ConstraintDecl::Unknown { .. } => {
                        review_only_rules += 1;
                    }
                    _ => runtime_checkable_rules += 1,
                }
            }
        }

        for (theory, rules) in &schema.rewrite_rules_by_theory {
            if !rules.is_empty() {
                theory_names.insert(theory.clone());
            }
            rewrite_rules += rules.len();
        }

        for (theory, blocks) in &schema.named_block_constraints_by_theory {
            if !blocks.is_empty() {
                theory_names.insert(theory.clone());
            }
            named_block_constraints += blocks.len();
        }
    }

    review_only_rules += named_block_constraints;
    let runtime_visible_rules = runtime_checkable_rules + rewrite_rules;
    let total_rules = relation_constraints + rewrite_rules + named_block_constraints;

    let mut notes = Vec::new();
    if rewrite_rules > 0 {
        notes.push(
            "rewrite rules are runtime-visible and authoring-relevant, but they are not yet uniformly certificate-emitted across all execution paths".to_string(),
        );
    }
    if named_block_constraints > 0 {
        notes.push(
            "named-block constraints remain review-only until lowered into structured constraint or certificate-backed forms".to_string(),
        );
    }
    if relation_constraints == 0 && rewrite_rules == 0 && named_block_constraints == 0 {
        notes.push(
            "no schema/theory rule surface was indexed from the current snapshot".to_string(),
        );
    }

    SemanticRuleInventoryV1 {
        total_rules,
        relation_constraints,
        rewrite_rules,
        named_block_constraints,
        runtime_checkable_rules,
        runtime_visible_rules,
        review_only_rules,
        relations_with_rules: relation_names.len(),
        theories_with_rules: theory_names.len(),
        relation_names: relation_names.into_iter().collect(),
        theory_names: theory_names.into_iter().collect(),
        notes,
    }
}

pub fn runtime_semantic_summary_for_preview(
    meta: &MetaPlaneIndex,
    trust: &ProposalValidationTrustContractV1,
    competency_gate: Option<&CompetencyGateReportV1>,
    quality_error_count: usize,
) -> RuntimeSemanticSummaryV1 {
    let rule_inventory = summarize_rule_inventory(meta);
    let mut gaps = Vec::new();

    let typed_fact_surface = if meta.schemas.is_empty() {
        gaps.push(
            "no canonical schema/theory metadata was loaded, so runtime typing coverage is unavailable"
                .to_string(),
        );
        "absent".to_string()
    } else {
        "schema_scoped_fact_typing_present".to_string()
    };

    let structured_constraint_surface = if rule_inventory.runtime_checkable_rules > 0 {
        "partial_runtime_enforced_structured_constraints".to_string()
    } else {
        gaps.push(
            "no structured runtime-checkable business rules were found in the current semantic surface"
                .to_string(),
        );
        "none_detected".to_string()
    };

    let rewrite_surface = if rule_inventory.rewrite_rules > 0 {
        gaps.push(
            "rewrite rules are visible to authoring/review flows but still need broader runtime and certificate alignment"
                .to_string(),
        );
        "declared_runtime_visible".to_string()
    } else {
        "not_declared".to_string()
    };

    let named_block_surface = if rule_inventory.named_block_constraints > 0 {
        gaps.push(
            "named-block constraints are preserved for review, but remain outside the structured certifiable subset"
                .to_string(),
        );
        "review_only".to_string()
    } else {
        "none_declared".to_string()
    };

    let competency_surface = if competency_gate.is_some() {
        "cq_gated_preview".to_string()
    } else {
        gaps.push(
            "competency questions are not configured, so intent coverage remains partly implicit"
                .to_string(),
        );
        "not_configured".to_string()
    };

    if quality_error_count > 0 {
        gaps.push(format!(
            "preview still has {} quality error(s) requiring operator or author intervention",
            quality_error_count
        ));
    }

    let quality_surface = "preview_quality_delta".to_string();
    let mut notes = trust.reasons.clone();
    notes.extend(rule_inventory.notes.clone());
    notes.push(
        "runtime semantic claims are operationally useful for authoring/review/querying, but they are not a claim of completeness or full ontology closure".to_string(),
    );

    RuntimeSemanticSummaryV1 {
        version: RUNTIME_SEMANTIC_SUMMARY_VERSION_V1.to_string(),
        trust_class: trust.trust_class.clone(),
        soundness: trust.soundness.clone(),
        coverage: trust.coverage.clone(),
        scope: trust.scope.clone(),
        completeness_claim: "not_claimed".to_string(),
        ontology_closure_claim: "not_claimed".to_string(),
        rule_inventory,
        semantic_coverage: SemanticCoverageSummaryV1 {
            typed_fact_surface,
            structured_constraint_surface,
            rewrite_surface,
            named_block_surface,
            competency_surface,
            quality_surface,
            gaps,
        },
        notes,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;
    use axiograph_pathdb::axi_semantics::{
        NamedBlockConstraintDecl, RewriteRuleDecl, SchemaIndex, SubtypeDecl,
    };

    fn sample_trust() -> ProposalValidationTrustContractV1 {
        ProposalValidationTrustContractV1 {
            trust_class: "preview_validated".to_string(),
            soundness: "preview_typechecked_and_quality_gated".to_string(),
            coverage: "proposal_delta_only".to_string(),
            scope: "snapshot_scoped_preview".to_string(),
            reasons: vec![
                "preview is scoped to the current snapshot plus imported proposal delta"
                    .to_string(),
            ],
        }
    }

    #[test]
    fn runtime_semantic_summary_counts_rule_surfaces_and_gaps() {
        let mut meta = MetaPlaneIndex::default();
        meta.schemas.insert(
            "Family".to_string(),
            SchemaIndex {
                schema_entity: 1,
                module_name: Some("Family".to_string()),
                object_types: HashSet::new(),
                subtype_decls: vec![SubtypeDecl {
                    subtype_entity: 2,
                    sub: "Person".to_string(),
                    sup: "Agent".to_string(),
                }],
                relation_decls: HashMap::new(),
                constraints_by_relation: HashMap::from([(
                    "parent".to_string(),
                    vec![
                        ConstraintDecl::Functional {
                            relation: "parent".to_string(),
                            src_field: "child".to_string(),
                            dst_field: "mother".to_string(),
                        },
                        ConstraintDecl::Unknown {
                            relation: Some("parent".to_string()),
                            text: "custom parent policy".to_string(),
                        },
                    ],
                )]),
                rewrite_rules_by_theory: HashMap::from([(
                    "FamilyTheory".to_string(),
                    vec![RewriteRuleDecl {
                        rule_entity: 3,
                        theory_name: "FamilyTheory".to_string(),
                        name: "parent_assoc".to_string(),
                        orientation: "forward".to_string(),
                        vars_text: "x: Person".to_string(),
                        vars: Vec::new(),
                        vars_parse_error: None,
                        lhs: "parent(x,y)".to_string(),
                        rhs: "ancestor(x,y)".to_string(),
                        index: 0,
                    }],
                )]),
                named_block_constraints_by_theory: HashMap::from([(
                    "FamilyTheory".to_string(),
                    vec![NamedBlockConstraintDecl {
                        constraint_entity: 4,
                        theory_name: "FamilyTheory".to_string(),
                        name: "family policy".to_string(),
                        body: "forall x".to_string(),
                        index: 0,
                    }],
                )]),
                supertypes_of: HashMap::new(),
            },
        );

        let summary = runtime_semantic_summary_for_preview(&meta, &sample_trust(), None, 0);
        assert_eq!(summary.rule_inventory.total_rules, 4);
        assert_eq!(summary.rule_inventory.relation_constraints, 2);
        assert_eq!(summary.rule_inventory.rewrite_rules, 1);
        assert_eq!(summary.rule_inventory.named_block_constraints, 1);
        assert_eq!(summary.rule_inventory.runtime_checkable_rules, 1);
        assert_eq!(summary.rule_inventory.runtime_visible_rules, 2);
        assert_eq!(summary.rule_inventory.review_only_rules, 2);
        assert_eq!(summary.completeness_claim, "not_claimed");
        assert_eq!(summary.ontology_closure_claim, "not_claimed");
        assert_eq!(
            summary.semantic_coverage.structured_constraint_surface,
            "partial_runtime_enforced_structured_constraints"
        );
        assert_eq!(
            summary.semantic_coverage.competency_surface,
            "not_configured"
        );
        assert!(summary
            .semantic_coverage
            .gaps
            .iter()
            .any(|gap| gap.contains("named-block constraints are preserved for review")));
        assert!(summary
            .semantic_coverage
            .gaps
            .iter()
            .any(|gap| gap.contains("competency questions are not configured")));
    }

    #[test]
    fn runtime_semantic_summary_marks_absent_type_surface() {
        let summary = runtime_semantic_summary_for_preview(
            &MetaPlaneIndex::default(),
            &sample_trust(),
            None,
            2,
        );
        assert_eq!(summary.semantic_coverage.typed_fact_surface, "absent");
        assert!(summary
            .semantic_coverage
            .gaps
            .iter()
            .any(|gap| gap.contains("runtime typing coverage is unavailable")));
        assert!(summary
            .semantic_coverage
            .gaps
            .iter()
            .any(|gap| gap.contains("preview still has 2 quality error(s)")));
    }
}
