use std::collections::BTreeSet;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use axiograph_pathdb::axi_semantics::{
    ConstraintDecl, MetaPlaneIndex, NamedBlockConstraintDecl, RewriteRuleDecl,
};
use axiograph_pathdb::kernel_ir::{RuntimeTheoryFragmentSummaryV1, TheorySubjectRefIr};
use axiograph_pathdb::{AcceptedSnapshotId, RelationId, TheoryId};

use crate::proposals_validate::{CompetencyGateReportV1, ProposalValidationTrustContractV1};

pub const RUNTIME_RULE_CATALOG_VERSION_V1: &str = "runtime_rule_catalog_v1";
#[allow(dead_code)]
pub const RUNTIME_RULE_REPORT_VERSION_V1: &str = "runtime_rule_report_v1";
pub const BUSINESS_RULE_APPLICABILITY_REPORT_VERSION_V1: &str =
    "business_rule_applicability_report_v1";
pub const RUNTIME_SEMANTIC_SUMMARY_VERSION_V1: &str = "runtime_semantic_summary_v1";
pub const IMPLEMENTATION_SURFACE_RULE_REPORT_VERSION_V1: &str =
    "implementation_surface_rule_report_v1";
pub const SEMANTIC_COVERAGE_REPORT_VERSION_V1: &str = "semantic_coverage_report_v1";
pub const AGENT_ENGINEERING_REPORT_VERSION_V1: &str = "agent_engineering_report_v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRuleClassV1 {
    FunctionalConstraint,
    AtMostConstraint,
    TypingConstraint,
    SymmetricWhereInConstraint,
    SymmetricConstraint,
    TransitiveConstraint,
    KeyConstraint,
    RewriteRule,
    NamedBlockConstraint,
    UnknownConstraint,
}

impl RuntimeRuleClassV1 {
    fn stable_label(self) -> &'static str {
        match self {
            RuntimeRuleClassV1::FunctionalConstraint => "functional",
            RuntimeRuleClassV1::AtMostConstraint => "at-most",
            RuntimeRuleClassV1::TypingConstraint => "typing",
            RuntimeRuleClassV1::SymmetricWhereInConstraint => "symmetric-where-in",
            RuntimeRuleClassV1::SymmetricConstraint => "symmetric",
            RuntimeRuleClassV1::TransitiveConstraint => "transitive",
            RuntimeRuleClassV1::KeyConstraint => "key",
            RuntimeRuleClassV1::RewriteRule => "rewrite",
            RuntimeRuleClassV1::NamedBlockConstraint => "named-block",
            RuntimeRuleClassV1::UnknownConstraint => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRuleScopeClassV1 {
    Relation,
    Theory,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct RuntimeRuleScopeV1 {
    #[serde(default)]
    pub scope_id: String,
    pub schema: String,
    pub scope_class: RuntimeRuleScopeClassV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_ref: Option<TheorySubjectRefIr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theory: Option<String>,
}

impl RuntimeRuleScopeV1 {
    pub fn relation(schema: &str, relation: &str) -> Self {
        Self {
            scope_id: relation_scope_id(schema, relation),
            schema: schema.to_string(),
            scope_class: RuntimeRuleScopeClassV1::Relation,
            scope_ref: Some(TheorySubjectRefIr::Relation {
                relation_id: RelationId::new(format!("relation:{schema}:{relation}")),
                relation_name: relation.to_string(),
            }),
            relation: Some(relation.to_string()),
            theory: None,
        }
    }

    pub fn theory(schema: &str, theory: &str) -> Self {
        Self {
            scope_id: theory_scope_id(schema, theory),
            schema: schema.to_string(),
            scope_class: RuntimeRuleScopeClassV1::Theory,
            scope_ref: Some(TheorySubjectRefIr::Theory {
                theory_id: TheoryId::new(format!("theory:{schema}:{theory}")),
            }),
            relation: None,
            theory: Some(theory.to_string()),
        }
    }

    pub fn inferred_scope_ref(&self) -> Option<TheorySubjectRefIr> {
        self.scope_ref.clone().or_else(|| match self.scope_class {
            RuntimeRuleScopeClassV1::Relation => {
                self.relation
                    .as_ref()
                    .map(|relation| TheorySubjectRefIr::Relation {
                        relation_id: RelationId::new(format!(
                            "relation:{}:{}",
                            self.schema, relation
                        )),
                        relation_name: relation.clone(),
                    })
            }
            RuntimeRuleScopeClassV1::Theory => {
                self.theory
                    .as_ref()
                    .map(|theory| TheorySubjectRefIr::Theory {
                        theory_id: TheoryId::new(format!("theory:{}:{}", self.schema, theory)),
                    })
            }
        })
    }

    pub fn inferred_scope_id(&self) -> Option<String> {
        if !self.scope_id.is_empty() {
            return Some(self.scope_id.clone());
        }
        match self.scope_class {
            RuntimeRuleScopeClassV1::Relation => self
                .relation
                .as_ref()
                .map(|relation| relation_scope_id(&self.schema, relation)),
            RuntimeRuleScopeClassV1::Theory => self
                .theory
                .as_ref()
                .map(|theory| theory_scope_id(&self.schema, theory)),
        }
    }

    pub fn normalized(&self) -> Self {
        let mut out = self.clone();
        if let Some(scope_id) = self.inferred_scope_id() {
            out.scope_id = scope_id;
        }
        out.scope_ref = self.inferred_scope_ref();
        out
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRuleSupportV1 {
    QualityGate,
    QualityGateAndQueryPlanning,
    TypedMetadataOnly,
    RuntimeRewriteHelper,
    ReviewOnly,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRuleTrustClassV1 {
    RuntimeEnforced,
    RuntimeAdvisory,
    ReviewOnly,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRuleCertificationV1 {
    NotClaimed,
    CertificateEmittableSubset,
    OutOfScope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeRuleV1 {
    pub rule_id: String,
    pub class: RuntimeRuleClassV1,
    pub scope: RuntimeRuleScopeV1,
    pub runtime_support: RuntimeRuleSupportV1,
    pub trust_class: RuntimeRuleTrustClassV1,
    pub certification: RuntimeRuleCertificationV1,
    pub summary: String,
}

impl RuntimeRuleV1 {
    pub fn is_runtime_checkable(&self) -> bool {
        self.trust_class == RuntimeRuleTrustClassV1::RuntimeEnforced
    }

    pub fn is_runtime_visible(&self) -> bool {
        self.trust_class != RuntimeRuleTrustClassV1::ReviewOnly
    }

    pub fn is_review_only(&self) -> bool {
        self.trust_class == RuntimeRuleTrustClassV1::ReviewOnly
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeRuleCatalogV1 {
    pub version: String,
    #[serde(default)]
    pub rules: Vec<RuntimeRuleV1>,
    #[serde(default)]
    pub notes: Vec<String>,
}

impl Default for RuntimeRuleCatalogV1 {
    fn default() -> Self {
        Self {
            version: RUNTIME_RULE_CATALOG_VERSION_V1.to_string(),
            rules: Vec::new(),
            notes: Vec::new(),
        }
    }
}

#[allow(dead_code)]
impl RuntimeRuleCatalogV1 {
    pub fn rules_for_scope_id(&self, scope_id: &str) -> Vec<&RuntimeRuleV1> {
        self.rules
            .iter()
            .filter(|rule| rule.scope.scope_id == scope_id)
            .collect()
    }

    pub fn relation_rules(&self, schema: &str, relation: &str) -> Vec<&RuntimeRuleV1> {
        self.rules_for_scope_id(&relation_scope_id(schema, relation))
    }

    pub fn theory_rules(&self, schema: &str, theory: &str) -> Vec<&RuntimeRuleV1> {
        self.rules_for_scope_id(&theory_scope_id(schema, theory))
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeRuleReportV1 {
    pub version: String,
    pub scope: RuntimeRuleScopeV1,
    #[serde(default)]
    pub total_rules: usize,
    #[serde(default)]
    pub runtime_enforced_rules: usize,
    #[serde(default)]
    pub runtime_visible_rules: usize,
    #[serde(default)]
    pub review_only_rules: usize,
    #[serde(default)]
    pub certificate_subset_rules: usize,
    #[serde(default)]
    pub rules: Vec<RuntimeRuleV1>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SemanticClaimStrengthV1 {
    Strong,
    Weak,
    Unknown,
    Conflicted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BusinessRuleApplicabilityReportV1 {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub lifecycle_state: String,
    pub scope: RuntimeRuleScopeV1,
    pub trust_class: RuntimeRuleTrustClassV1,
    pub strength: SemanticClaimStrengthV1,
    pub checked_surface: String,
    #[serde(default)]
    pub runtime_enforced_rules: usize,
    #[serde(default)]
    pub runtime_visible_rules: usize,
    #[serde(default)]
    pub review_only_rules: usize,
    #[serde(default)]
    pub certificate_subset_rules: usize,
    #[serde(default)]
    pub rules: Vec<RuntimeRuleV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_theory_fragment_summary: Option<RuntimeTheoryFragmentSummaryV1>,
    #[serde(default)]
    pub missing_obligations: Vec<String>,
    #[serde(default)]
    pub next_actions: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImplementationSurfaceKindV1 {
    Endpoint,
    Workflow,
    Report,
    Job,
    Migration,
    Config,
    DocSection,
    AgentTask,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImplementationSurfaceRefV1 {
    pub surface_id: String,
    pub kind: ImplementationSurfaceKindV1,
    pub label: String,
    #[serde(default)]
    pub scopes: Vec<RuntimeRuleScopeV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub code_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl ImplementationSurfaceRefV1 {
    fn normalized(&self) -> Self {
        let mut out = self.clone();
        out.scopes = self
            .scopes
            .iter()
            .map(RuntimeRuleScopeV1::normalized)
            .collect();
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentTaskRefV1 {
    pub task_id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objective: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifact_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImplementationSurfaceRuleReportV1 {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub lifecycle_state: String,
    pub surface: ImplementationSurfaceRefV1,
    pub trust_class: RuntimeRuleTrustClassV1,
    pub strength: SemanticClaimStrengthV1,
    pub checked_surface: String,
    #[serde(default)]
    pub runtime_enforced_rules: usize,
    #[serde(default)]
    pub runtime_visible_rules: usize,
    #[serde(default)]
    pub review_only_rules: usize,
    #[serde(default)]
    pub certificate_subset_rules: usize,
    #[serde(default)]
    pub rules: Vec<RuntimeRuleV1>,
    #[serde(default)]
    pub missing_obligations: Vec<String>,
    #[serde(default)]
    pub next_actions: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoverageStatusV1 {
    Tested,
    Implemented,
    DocumentedOnly,
    OntologyOnly,
    Drifted,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoverageEdgeV1 {
    pub surface_id: String,
    pub rule_id: String,
    pub status: CoverageStatusV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoverageRuleStatusV1 {
    pub rule_id: String,
    pub scope_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_ref: Option<TheorySubjectRefIr>,
    pub trust_class: RuntimeRuleTrustClassV1,
    pub best_status: CoverageStatusV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surface_ids: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoverageReportV1 {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub lifecycle_state: String,
    #[serde(default)]
    pub total_rules: usize,
    #[serde(default)]
    pub runtime_enforced_rules: usize,
    #[serde(default)]
    pub covered_rules: usize,
    #[serde(default)]
    pub tested_rules: usize,
    #[serde(default)]
    pub implemented_rules: usize,
    #[serde(default)]
    pub documented_only_rules: usize,
    #[serde(default)]
    pub ontology_only_rules: usize,
    #[serde(default)]
    pub drifted_rules: usize,
    #[serde(default)]
    pub unknown_rules: usize,
    #[serde(default)]
    pub surface_reports: Vec<ImplementationSurfaceRuleReportV1>,
    #[serde(default)]
    pub rule_statuses: Vec<CoverageRuleStatusV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_theory_check: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
    #[serde(default)]
    pub uncovered_rule_ids: Vec<String>,
    #[serde(default)]
    pub missing_obligations: Vec<String>,
    #[serde(default)]
    pub next_actions: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentEngineeringReportV1 {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub lifecycle_state: String,
    pub task: AgentTaskRefV1,
    pub trust_class: RuntimeRuleTrustClassV1,
    pub strength: SemanticClaimStrengthV1,
    pub checked_surface: String,
    #[serde(default)]
    pub matched_scope_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_scope_refs: Vec<TheorySubjectRefIr>,
    #[serde(default)]
    pub matched_rule_ids: Vec<String>,
    #[serde(default)]
    pub surface_reports: Vec<ImplementationSurfaceRuleReportV1>,
    pub coverage: CoverageReportV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_theory_check: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
    #[serde(default)]
    pub residual_unknowns: Vec<String>,
    #[serde(default)]
    pub next_actions: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

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

fn stable_name_fragment(raw: &str) -> String {
    let mut out = String::new();
    let mut last_was_dash = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed
    }
}

fn relation_scope_id(schema: &str, relation: &str) -> String {
    format!(
        "schema/{}/relation/{}",
        stable_name_fragment(schema),
        stable_name_fragment(relation)
    )
}

fn theory_scope_id(schema: &str, theory: &str) -> String {
    format!(
        "schema/{}/theory/{}",
        stable_name_fragment(schema),
        stable_name_fragment(theory)
    )
}

fn relation_rule_id(
    schema: &str,
    relation: &str,
    class: RuntimeRuleClassV1,
    index: usize,
) -> String {
    format!(
        "{}/rule/{}/{}",
        relation_scope_id(schema, relation),
        class.stable_label(),
        index
    )
}

fn theory_rule_id(
    schema: &str,
    theory: &str,
    class: RuntimeRuleClassV1,
    name: &str,
    index: usize,
) -> String {
    format!(
        "{}/rule/{}/{}/{}",
        theory_scope_id(schema, theory),
        class.stable_label(),
        stable_name_fragment(name),
        index
    )
}

fn runtime_rule_from_constraint(
    schema_name: &str,
    relation_name: &str,
    index: usize,
    decl: &ConstraintDecl,
) -> RuntimeRuleV1 {
    let scope = RuntimeRuleScopeV1::relation(schema_name, relation_name);
    let (class, runtime_support, trust_class, certification, summary) = match decl {
        ConstraintDecl::Functional {
            src_field,
            dst_field,
            ..
        } => (
            RuntimeRuleClassV1::FunctionalConstraint,
            RuntimeRuleSupportV1::QualityGateAndQueryPlanning,
            RuntimeRuleTrustClassV1::RuntimeEnforced,
            RuntimeRuleCertificationV1::NotClaimed,
            format!("{relation_name}.{src_field} -> {relation_name}.{dst_field} is functional"),
        ),
        ConstraintDecl::AtMost {
            src_field,
            dst_field,
            max,
            params,
            ..
        } => {
            let params_suffix = params
                .as_ref()
                .filter(|params| !params.is_empty())
                .map(|params| format!(" with params {}", params.join(", ")))
                .unwrap_or_default();
            (
                RuntimeRuleClassV1::AtMostConstraint,
                RuntimeRuleSupportV1::QualityGate,
                RuntimeRuleTrustClassV1::RuntimeEnforced,
                RuntimeRuleCertificationV1::NotClaimed,
                format!(
                    "{relation_name}.{src_field} -> {relation_name}.{dst_field} has at_most {max}{params_suffix}"
                ),
            )
        }
        ConstraintDecl::Typing { rule, .. } => (
            RuntimeRuleClassV1::TypingConstraint,
            RuntimeRuleSupportV1::TypedMetadataOnly,
            RuntimeRuleTrustClassV1::RuntimeAdvisory,
            RuntimeRuleCertificationV1::NotClaimed,
            format!("{relation_name} carries typing rule `{rule}`"),
        ),
        ConstraintDecl::SymmetricWhereIn { field, values, .. } => (
            RuntimeRuleClassV1::SymmetricWhereInConstraint,
            RuntimeRuleSupportV1::ReviewOnly,
            RuntimeRuleTrustClassV1::ReviewOnly,
            RuntimeRuleCertificationV1::OutOfScope,
            format!(
                "{relation_name} is symmetric when {field} is in [{}]",
                values.join(", ")
            ),
        ),
        ConstraintDecl::Symmetric { .. } => (
            RuntimeRuleClassV1::SymmetricConstraint,
            RuntimeRuleSupportV1::ReviewOnly,
            RuntimeRuleTrustClassV1::ReviewOnly,
            RuntimeRuleCertificationV1::OutOfScope,
            format!("{relation_name} is declared symmetric"),
        ),
        ConstraintDecl::Transitive { .. } => (
            RuntimeRuleClassV1::TransitiveConstraint,
            RuntimeRuleSupportV1::ReviewOnly,
            RuntimeRuleTrustClassV1::ReviewOnly,
            RuntimeRuleCertificationV1::OutOfScope,
            format!("{relation_name} is declared transitive"),
        ),
        ConstraintDecl::Key { fields, .. } => (
            RuntimeRuleClassV1::KeyConstraint,
            RuntimeRuleSupportV1::QualityGateAndQueryPlanning,
            RuntimeRuleTrustClassV1::RuntimeEnforced,
            RuntimeRuleCertificationV1::NotClaimed,
            format!("{relation_name} has key({})", fields.join(", ")),
        ),
        ConstraintDecl::NamedBlock { name, .. } => (
            RuntimeRuleClassV1::NamedBlockConstraint,
            RuntimeRuleSupportV1::ReviewOnly,
            RuntimeRuleTrustClassV1::ReviewOnly,
            RuntimeRuleCertificationV1::OutOfScope,
            format!("named block `{name}` is stored for review on {relation_name}"),
        ),
        ConstraintDecl::Unknown { text, .. } => (
            RuntimeRuleClassV1::UnknownConstraint,
            RuntimeRuleSupportV1::ReviewOnly,
            RuntimeRuleTrustClassV1::ReviewOnly,
            RuntimeRuleCertificationV1::OutOfScope,
            format!("unsupported relation-scoped rule on {relation_name}: {text}"),
        ),
    };

    RuntimeRuleV1 {
        rule_id: relation_rule_id(schema_name, relation_name, class, index),
        class,
        scope,
        runtime_support,
        trust_class,
        certification,
        summary,
    }
}

fn runtime_rule_from_rewrite(
    schema_name: &str,
    theory_name: &str,
    rule: &RewriteRuleDecl,
) -> RuntimeRuleV1 {
    let class = RuntimeRuleClassV1::RewriteRule;
    RuntimeRuleV1 {
        rule_id: theory_rule_id(schema_name, theory_name, class, &rule.name, rule.index),
        class,
        scope: RuntimeRuleScopeV1::theory(schema_name, theory_name),
        runtime_support: RuntimeRuleSupportV1::RuntimeRewriteHelper,
        trust_class: RuntimeRuleTrustClassV1::RuntimeAdvisory,
        certification: RuntimeRuleCertificationV1::CertificateEmittableSubset,
        summary: format!("rewrite rule `{}`: {} => {}", rule.name, rule.lhs, rule.rhs),
    }
}

fn runtime_rule_from_named_block(
    schema_name: &str,
    theory_name: &str,
    block: &NamedBlockConstraintDecl,
) -> RuntimeRuleV1 {
    let class = RuntimeRuleClassV1::NamedBlockConstraint;
    RuntimeRuleV1 {
        rule_id: theory_rule_id(schema_name, theory_name, class, &block.name, block.index),
        class,
        scope: RuntimeRuleScopeV1::theory(schema_name, theory_name),
        runtime_support: RuntimeRuleSupportV1::ReviewOnly,
        trust_class: RuntimeRuleTrustClassV1::ReviewOnly,
        certification: RuntimeRuleCertificationV1::OutOfScope,
        summary: format!("named block `{}` is stored for review", block.name),
    }
}

pub fn runtime_rule_catalog(meta: &MetaPlaneIndex) -> RuntimeRuleCatalogV1 {
    let mut catalog = RuntimeRuleCatalogV1::default();

    let mut schema_names = meta.schemas.keys().cloned().collect::<Vec<_>>();
    schema_names.sort();
    for schema_name in schema_names {
        let Some(schema) = meta.schemas.get(&schema_name) else {
            continue;
        };

        let mut relation_names = schema
            .constraints_by_relation
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        relation_names.sort();
        for relation_name in relation_names {
            let Some(decls) = schema.constraints_by_relation.get(&relation_name) else {
                continue;
            };
            for (index, decl) in decls.iter().enumerate() {
                catalog.rules.push(runtime_rule_from_constraint(
                    &schema_name,
                    &relation_name,
                    index,
                    decl,
                ));
            }
        }

        let mut theory_names = schema
            .rewrite_rules_by_theory
            .keys()
            .chain(schema.named_block_constraints_by_theory.keys())
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        theory_names.sort();
        for theory_name in theory_names {
            if let Some(rules) = schema.rewrite_rules_by_theory.get(&theory_name) {
                let mut sorted_rules = rules.iter().collect::<Vec<_>>();
                sorted_rules.sort_by(|left, right| {
                    left.index
                        .cmp(&right.index)
                        .then(left.name.cmp(&right.name))
                        .then(left.lhs.cmp(&right.lhs))
                        .then(left.rhs.cmp(&right.rhs))
                });
                for rule in sorted_rules {
                    catalog
                        .rules
                        .push(runtime_rule_from_rewrite(&schema_name, &theory_name, rule));
                }
            }

            if let Some(blocks) = schema.named_block_constraints_by_theory.get(&theory_name) {
                let mut sorted_blocks = blocks.iter().collect::<Vec<_>>();
                sorted_blocks.sort_by(|left, right| {
                    left.index
                        .cmp(&right.index)
                        .then(left.name.cmp(&right.name))
                        .then(left.body.cmp(&right.body))
                });
                for block in sorted_blocks {
                    catalog.rules.push(runtime_rule_from_named_block(
                        &schema_name,
                        &theory_name,
                        block,
                    ));
                }
            }
        }
    }

    catalog.rules.sort_by(|left, right| {
        left.scope
            .scope_id
            .cmp(&right.scope.scope_id)
            .then(left.rule_id.cmp(&right.rule_id))
            .then(left.summary.cmp(&right.summary))
    });

    if catalog
        .rules
        .iter()
        .any(|rule| rule.class == RuntimeRuleClassV1::RewriteRule)
    {
        catalog.notes.push(
            "rewrite rules are runtime-visible and authoring-relevant, but they are not yet uniformly certificate-emitted across all execution paths".to_string(),
        );
    }
    if catalog
        .rules
        .iter()
        .any(|rule| rule.class == RuntimeRuleClassV1::NamedBlockConstraint)
    {
        catalog.notes.push(
            "named-block constraints remain review-only until lowered into structured constraint or certificate-backed forms".to_string(),
        );
    }
    if catalog.rules.iter().any(|rule| {
        rule.scope.scope_class == RuntimeRuleScopeClassV1::Relation
            && rule.trust_class == RuntimeRuleTrustClassV1::RuntimeAdvisory
    }) {
        catalog.notes.push(
            "some structured business rules are visible to authoring/query tooling but remain advisory metadata rather than fail-closed runtime guarantees".to_string(),
        );
    }
    if catalog.rules.is_empty() {
        catalog.notes.push(
            "no schema/theory rule surface was indexed from the current snapshot".to_string(),
        );
    }

    catalog
}

#[allow(dead_code)]
fn runtime_rule_report_from_rules(
    scope: RuntimeRuleScopeV1,
    rules: Vec<RuntimeRuleV1>,
) -> RuntimeRuleReportV1 {
    let runtime_enforced_rules = rules
        .iter()
        .filter(|rule| rule.trust_class == RuntimeRuleTrustClassV1::RuntimeEnforced)
        .count();
    let runtime_visible_rules = rules
        .iter()
        .filter(|rule| rule.trust_class != RuntimeRuleTrustClassV1::ReviewOnly)
        .count();
    let review_only_rules = rules
        .iter()
        .filter(|rule| rule.trust_class == RuntimeRuleTrustClassV1::ReviewOnly)
        .count();
    let certificate_subset_rules = rules
        .iter()
        .filter(|rule| rule.certification == RuntimeRuleCertificationV1::CertificateEmittableSubset)
        .count();

    let mut notes = Vec::new();
    if rules.is_empty() {
        notes.push(format!(
            "no runtime-visible business rules were indexed for {}",
            scope.scope_id
        ));
    }
    if review_only_rules > 0 {
        notes.push(format!(
            "{review_only_rules} rule(s) remain review-only for {}",
            scope.scope_id
        ));
    }
    if certificate_subset_rules > 0 {
        notes.push(format!(
            "{certificate_subset_rules} rule(s) currently sit in the certificate-emittable subset for {}",
            scope.scope_id
        ));
    }

    RuntimeRuleReportV1 {
        version: RUNTIME_RULE_REPORT_VERSION_V1.to_string(),
        scope,
        total_rules: rules.len(),
        runtime_enforced_rules,
        runtime_visible_rules,
        review_only_rules,
        certificate_subset_rules,
        rules,
        notes,
    }
}

#[allow(dead_code)]
pub fn runtime_rule_report_for_relation(
    meta: &MetaPlaneIndex,
    schema: &str,
    relation: &str,
) -> RuntimeRuleReportV1 {
    let catalog = runtime_rule_catalog(meta);
    let scope = RuntimeRuleScopeV1::relation(schema, relation);
    let rules = catalog
        .relation_rules(schema, relation)
        .into_iter()
        .cloned()
        .collect();
    runtime_rule_report_from_rules(scope, rules)
}

#[allow(dead_code)]
pub fn runtime_rule_report_for_theory(
    meta: &MetaPlaneIndex,
    schema: &str,
    theory: &str,
) -> RuntimeRuleReportV1 {
    let catalog = runtime_rule_catalog(meta);
    let scope = RuntimeRuleScopeV1::theory(schema, theory);
    let rules = catalog
        .theory_rules(schema, theory)
        .into_iter()
        .cloned()
        .collect();
    runtime_rule_report_from_rules(scope, rules)
}

fn claim_strength_for_report(
    lifecycle_state: &str,
    report: &RuntimeRuleReportV1,
) -> SemanticClaimStrengthV1 {
    let lifecycle = lifecycle_state.trim().to_ascii_lowercase();
    if report.total_rules == 0 {
        return SemanticClaimStrengthV1::Unknown;
    }
    if lifecycle == "conflicted" {
        return SemanticClaimStrengthV1::Conflicted;
    }
    if (lifecycle == "accepted" || lifecycle == "certified")
        && report.review_only_rules == 0
        && (report.runtime_enforced_rules > 0 || report.certificate_subset_rules > 0)
    {
        return SemanticClaimStrengthV1::Strong;
    }
    SemanticClaimStrengthV1::Weak
}

fn applicability_from_rule_report(
    accepted_snapshot_id: Option<AcceptedSnapshotId>,
    lifecycle_state: &str,
    checked_surface: &str,
    report: RuntimeRuleReportV1,
    runtime_theory_fragment_summary: Option<RuntimeTheoryFragmentSummaryV1>,
) -> BusinessRuleApplicabilityReportV1 {
    let strength = claim_strength_for_report(lifecycle_state, &report);

    let trust_class = if report.review_only_rules == report.total_rules && report.total_rules > 0 {
        RuntimeRuleTrustClassV1::ReviewOnly
    } else if report.runtime_enforced_rules > 0 {
        RuntimeRuleTrustClassV1::RuntimeEnforced
    } else {
        RuntimeRuleTrustClassV1::RuntimeAdvisory
    };

    let mut missing_obligations = Vec::new();
    let mut next_actions = Vec::new();
    let mut notes = report.notes.clone();

    if report.total_rules == 0 {
        missing_obligations
            .push("no typed business rules are indexed for this scope yet".to_string());
        next_actions.push(
            "author or import scoped theory/rule material before treating this surface as ontology-governed"
                .to_string(),
        );
    }
    if report.review_only_rules > 0 {
        missing_obligations.push(format!(
            "{} rule(s) remain review-only and are not yet runtime-enforced",
            report.review_only_rules
        ));
        next_actions.push(
            "lower review-only rules into structured runtime checks or certificate-backed theory fragments"
                .to_string(),
        );
    }
    if lifecycle_state != "accepted" && lifecycle_state != "certified" {
        missing_obligations.push(format!(
            "scope is in lifecycle state `{lifecycle_state}` so claims remain weaker than accepted/certified semantics"
        ));
        next_actions.push(
            "review and promote this scope before treating obligations as strong engineering claims"
                .to_string(),
        );
    }
    if report.certificate_subset_rules > 0 {
        notes.push(format!(
            "{} rule(s) already sit in the current certificate-emittable subset",
            report.certificate_subset_rules
        ));
    }
    if let Some(fragment_summary) = runtime_theory_fragment_summary.as_ref() {
        if fragment_summary.opaque_or_out_of_fragment_obligations > 0 {
            missing_obligations.push(format!(
                "{} theory obligation(s) remain opaque or outside the current runtime theory fragment",
                fragment_summary.opaque_or_out_of_fragment_obligations
            ));
            next_actions.push(
                "lower opaque theory obligations into the current structured runtime fragment or keep them explicit as review-only/runtime-opaque obligations"
                    .to_string(),
            );
        }
        notes.push(
            "attached runtime theory fragment summary is a Rust-side operational artifact outside the trusted-kernel/certificate boundary; it does not claim completeness or ontology closure"
                .to_string(),
        );
    }

    BusinessRuleApplicabilityReportV1 {
        version: BUSINESS_RULE_APPLICABILITY_REPORT_VERSION_V1.to_string(),
        accepted_snapshot_id,
        lifecycle_state: lifecycle_state.to_string(),
        scope: report.scope,
        trust_class,
        strength,
        checked_surface: checked_surface.to_string(),
        runtime_enforced_rules: report.runtime_enforced_rules,
        runtime_visible_rules: report.runtime_visible_rules,
        review_only_rules: report.review_only_rules,
        certificate_subset_rules: report.certificate_subset_rules,
        rules: report.rules,
        runtime_theory_fragment_summary,
        missing_obligations,
        next_actions,
        notes,
    }
}

#[allow(dead_code)]
pub fn business_rule_applicability_for_relation(
    meta: &MetaPlaneIndex,
    accepted_snapshot_id: Option<AcceptedSnapshotId>,
    lifecycle_state: &str,
    schema: &str,
    relation: &str,
) -> BusinessRuleApplicabilityReportV1 {
    applicability_from_rule_report(
        accepted_snapshot_id,
        lifecycle_state,
        "relation_scope",
        runtime_rule_report_for_relation(meta, schema, relation),
        None,
    )
}

#[allow(dead_code)]
pub fn business_rule_applicability_for_theory(
    meta: &MetaPlaneIndex,
    accepted_snapshot_id: Option<AcceptedSnapshotId>,
    lifecycle_state: &str,
    schema: &str,
    theory: &str,
) -> BusinessRuleApplicabilityReportV1 {
    applicability_from_rule_report(
        accepted_snapshot_id,
        lifecycle_state,
        "theory_scope",
        runtime_rule_report_for_theory(meta, schema, theory),
        None,
    )
}

#[allow(dead_code)]
pub fn business_rule_applicability_for_theory_with_runtime_fragment(
    meta: &MetaPlaneIndex,
    accepted_snapshot_id: Option<AcceptedSnapshotId>,
    lifecycle_state: &str,
    schema: &str,
    theory: &str,
    runtime_theory_fragment_summary: RuntimeTheoryFragmentSummaryV1,
) -> Result<BusinessRuleApplicabilityReportV1> {
    let expected_theory_id = TheoryId::new(format!("theory:{schema}:{theory}"));
    match &runtime_theory_fragment_summary.theory_ref {
        TheorySubjectRefIr::Theory { theory_id } if theory_id == &expected_theory_id => {}
        TheorySubjectRefIr::Theory { theory_id } => {
            return Err(anyhow!(
                "runtime theory fragment summary references `{theory_id}` but theory applicability requested `{expected_theory_id}`"
            ));
        }
        other => {
            return Err(anyhow!(
                "runtime theory fragment summary must be anchored to a theory ref, got `{}`",
                other.display_name()
            ));
        }
    }

    Ok(applicability_from_rule_report(
        accepted_snapshot_id,
        lifecycle_state,
        "theory_scope",
        runtime_rule_report_for_theory(meta, schema, theory),
        Some(runtime_theory_fragment_summary),
    ))
}

fn aggregate_rules_for_scopes(
    catalog: &RuntimeRuleCatalogV1,
    scopes: &[RuntimeRuleScopeV1],
) -> Vec<RuntimeRuleV1> {
    let mut seen = BTreeSet::new();
    let mut rules = Vec::new();
    for scope in scopes {
        for rule in catalog.rules.iter().filter(|rule| rule.scope == *scope) {
            if seen.insert(rule.rule_id.clone()) {
                rules.push(rule.clone());
            }
        }
    }
    rules.sort_by(|left, right| left.rule_id.cmp(&right.rule_id));
    rules
}

pub fn implementation_surface_rule_report(
    meta: &MetaPlaneIndex,
    accepted_snapshot_id: Option<AcceptedSnapshotId>,
    lifecycle_state: &str,
    surface: &ImplementationSurfaceRefV1,
) -> ImplementationSurfaceRuleReportV1 {
    let surface = surface.normalized();
    let catalog = runtime_rule_catalog(meta);
    let rules = aggregate_rules_for_scopes(&catalog, &surface.scopes);
    let runtime_enforced_rules = rules
        .iter()
        .filter(|rule| rule.trust_class == RuntimeRuleTrustClassV1::RuntimeEnforced)
        .count();
    let runtime_visible_rules = rules
        .iter()
        .filter(|rule| rule.trust_class != RuntimeRuleTrustClassV1::ReviewOnly)
        .count();
    let review_only_rules = rules
        .iter()
        .filter(|rule| rule.trust_class == RuntimeRuleTrustClassV1::ReviewOnly)
        .count();
    let certificate_subset_rules = rules
        .iter()
        .filter(|rule| rule.certification == RuntimeRuleCertificationV1::CertificateEmittableSubset)
        .count();
    let strength = claim_strength_for_report(
        lifecycle_state,
        &RuntimeRuleReportV1 {
            version: RUNTIME_RULE_REPORT_VERSION_V1.to_string(),
            scope: surface.scopes.first().cloned().unwrap_or_else(|| {
                RuntimeRuleScopeV1::theory("unscoped", "implementation-surface")
            }),
            total_rules: rules.len(),
            runtime_enforced_rules,
            runtime_visible_rules,
            review_only_rules,
            certificate_subset_rules,
            rules: rules.clone(),
            notes: Vec::new(),
        },
    );
    let trust_class = if review_only_rules == rules.len() && !rules.is_empty() {
        RuntimeRuleTrustClassV1::ReviewOnly
    } else if runtime_enforced_rules > 0 {
        RuntimeRuleTrustClassV1::RuntimeEnforced
    } else {
        RuntimeRuleTrustClassV1::RuntimeAdvisory
    };

    let mut missing_obligations = Vec::new();
    let mut next_actions = Vec::new();
    let mut notes = Vec::new();

    if surface.scopes.is_empty() {
        missing_obligations
            .push("implementation surface is not mapped to any ontology rule scope".to_string());
        next_actions.push(
            "attach relation/theory scopes to this implementation surface so rule applicability is explicit".to_string(),
        );
    }
    if rules.is_empty() {
        missing_obligations.push(
            "no typed business rules were resolved for this implementation surface".to_string(),
        );
        next_actions.push(
            "author or import scoped theory/rule material before relying on this surface as ontology-governed".to_string(),
        );
    }
    if review_only_rules > 0 {
        missing_obligations.push(format!(
            "{review_only_rules} rule(s) mapped to this surface remain review-only"
        ));
        next_actions.push(
            "lower review-only rules into structured runtime checks or certificate-backed theory fragments".to_string(),
        );
    }
    if lifecycle_state != "accepted" && lifecycle_state != "certified" {
        missing_obligations.push(format!(
            "implementation surface is evaluated in lifecycle state `{lifecycle_state}`, so claims remain weaker than accepted/certified semantics"
        ));
        next_actions.push(
            "review and promote the governing ontology state before treating this surface as a strong claim".to_string(),
        );
    }
    if certificate_subset_rules > 0 {
        notes.push(format!(
            "{certificate_subset_rules} mapped rule(s) already sit in the current certificate-emittable subset"
        ));
    }

    ImplementationSurfaceRuleReportV1 {
        version: IMPLEMENTATION_SURFACE_RULE_REPORT_VERSION_V1.to_string(),
        accepted_snapshot_id,
        lifecycle_state: lifecycle_state.to_string(),
        surface,
        trust_class,
        strength,
        checked_surface: "implementation_surface".to_string(),
        runtime_enforced_rules,
        runtime_visible_rules,
        review_only_rules,
        certificate_subset_rules,
        rules,
        missing_obligations,
        next_actions,
        notes,
    }
}

fn coverage_status_rank(status: CoverageStatusV1) -> usize {
    match status {
        CoverageStatusV1::Drifted => 0,
        CoverageStatusV1::OntologyOnly => 1,
        CoverageStatusV1::Unknown => 2,
        CoverageStatusV1::DocumentedOnly => 3,
        CoverageStatusV1::Implemented => 4,
        CoverageStatusV1::Tested => 5,
    }
}

fn merge_coverage_status(left: CoverageStatusV1, right: CoverageStatusV1) -> CoverageStatusV1 {
    if left == CoverageStatusV1::Drifted || right == CoverageStatusV1::Drifted {
        CoverageStatusV1::Drifted
    } else if coverage_status_rank(right) > coverage_status_rank(left) {
        right
    } else {
        left
    }
}

fn status_counts_as_covered(status: CoverageStatusV1) -> bool {
    matches!(
        status,
        CoverageStatusV1::Tested | CoverageStatusV1::Implemented | CoverageStatusV1::DocumentedOnly
    )
}

pub fn semantic_coverage_report(
    meta: &MetaPlaneIndex,
    accepted_snapshot_id: Option<AcceptedSnapshotId>,
    lifecycle_state: &str,
    surfaces: &[ImplementationSurfaceRefV1],
    edges: &[CoverageEdgeV1],
    runtime_theory_check: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
) -> CoverageReportV1 {
    let catalog = runtime_rule_catalog(meta);
    let surfaces = surfaces
        .iter()
        .map(ImplementationSurfaceRefV1::normalized)
        .collect::<Vec<_>>();
    let surface_reports = surfaces
        .iter()
        .map(|surface| {
            implementation_surface_rule_report(
                meta,
                accepted_snapshot_id.clone(),
                lifecycle_state,
                surface,
            )
        })
        .collect::<Vec<_>>();

    let known_surface_ids = surfaces
        .iter()
        .map(|surface| surface.surface_id.as_str())
        .collect::<BTreeSet<_>>();
    let known_rule_ids = catalog
        .rules
        .iter()
        .map(|rule| rule.rule_id.as_str())
        .collect::<BTreeSet<_>>();

    let mut notes = Vec::new();
    let mut edge_statuses: std::collections::HashMap<String, (CoverageStatusV1, BTreeSet<String>)> =
        std::collections::HashMap::new();

    for edge in edges {
        if !known_surface_ids.contains(edge.surface_id.as_str()) {
            notes.push(format!(
                "coverage edge references unknown implementation surface `{}`",
                edge.surface_id
            ));
            continue;
        }
        if !known_rule_ids.contains(edge.rule_id.as_str()) {
            notes.push(format!(
                "coverage edge references unknown rule `{}`",
                edge.rule_id
            ));
            continue;
        }
        let entry = edge_statuses
            .entry(edge.rule_id.clone())
            .or_insert((edge.status, BTreeSet::new()));
        entry.0 = merge_coverage_status(entry.0, edge.status);
        entry.1.insert(edge.surface_id.clone());
    }

    let mut rule_statuses = Vec::new();
    let mut uncovered_rule_ids = Vec::new();
    let mut missing_obligations = Vec::new();
    let mut next_actions = Vec::new();

    let mut covered_rules = 0usize;
    let mut tested_rules = 0usize;
    let mut implemented_rules = 0usize;
    let mut documented_only_rules = 0usize;
    let mut ontology_only_rules = 0usize;
    let mut drifted_rules = 0usize;
    let mut unknown_rules = 0usize;

    for rule in &catalog.rules {
        let (best_status, surface_ids) = match edge_statuses.get(&rule.rule_id) {
            Some((status, surface_ids)) => (*status, surface_ids.iter().cloned().collect()),
            None => (CoverageStatusV1::OntologyOnly, Vec::new()),
        };

        if status_counts_as_covered(best_status) {
            covered_rules += 1;
        } else {
            uncovered_rule_ids.push(rule.rule_id.clone());
        }

        match best_status {
            CoverageStatusV1::Tested => tested_rules += 1,
            CoverageStatusV1::Implemented => implemented_rules += 1,
            CoverageStatusV1::DocumentedOnly => documented_only_rules += 1,
            CoverageStatusV1::OntologyOnly => ontology_only_rules += 1,
            CoverageStatusV1::Drifted => drifted_rules += 1,
            CoverageStatusV1::Unknown => unknown_rules += 1,
        }

        if best_status == CoverageStatusV1::Drifted {
            missing_obligations.push(format!(
                "rule `{}` is marked drifted against mapped implementation surfaces",
                rule.rule_id
            ));
            next_actions.push(format!(
                "reconcile ontology and code/tests for rule `{}` before promotion or release",
                rule.rule_id
            ));
        } else if rule.trust_class == RuntimeRuleTrustClassV1::RuntimeEnforced
            && !status_counts_as_covered(best_status)
        {
            missing_obligations.push(format!(
                "runtime-enforced rule `{}` is not yet covered by a tested/implemented/documented implementation surface",
                rule.rule_id
            ));
            next_actions.push(format!(
                "add an implementation-surface mapping and coverage edge for runtime-enforced rule `{}`",
                rule.rule_id
            ));
        }

        if rule.trust_class == RuntimeRuleTrustClassV1::ReviewOnly && !surface_ids.is_empty() {
            notes.push(format!(
                "review-only rule `{}` is mapped to implementation surfaces but still does not contribute a strong runtime claim",
                rule.rule_id
            ));
        }

        rule_statuses.push(CoverageRuleStatusV1 {
            rule_id: rule.rule_id.clone(),
            scope_id: rule.scope.scope_id.clone(),
            scope_ref: rule.scope.scope_ref.clone(),
            trust_class: rule.trust_class,
            best_status,
            surface_ids,
            summary: rule.summary.clone(),
        });
    }

    if surfaces.is_empty() {
        missing_obligations
            .push("no implementation surfaces were supplied for semantic coverage".to_string());
        next_actions.push(
            "declare endpoints/workflows/jobs/reports as implementation surfaces so ontology coverage can be computed".to_string(),
        );
    }
    if let Some(summary) = runtime_theory_check.as_ref() {
        if summary.blocking_errors > 0 {
            missing_obligations.push(format!(
                "runtime theory check has {} blocking error(s)",
                summary.blocking_errors
            ));
            next_actions.push(
                "resolve RuntimeTheoryCheckReportV1 blocking judgments before claiming semantic coverage is promotion-ready".to_string(),
            );
        }
        if !summary.residual_obligation_ids.is_empty() {
            missing_obligations.push(format!(
                "{} runtime theory residual obligation(s) remain outside strong coverage",
                summary.residual_obligation_ids.len()
            ));
            next_actions.push(
                "review runtime theory residual obligations alongside uncovered rule coverage"
                    .to_string(),
            );
        }
        notes.push(format!(
            "runtime theory check summary attached: completeness={}, ontology_closure={}",
            summary.completeness_claim, summary.ontology_closure_claim
        ));
    }

    CoverageReportV1 {
        version: SEMANTIC_COVERAGE_REPORT_VERSION_V1.to_string(),
        accepted_snapshot_id,
        lifecycle_state: lifecycle_state.to_string(),
        total_rules: catalog.rules.len(),
        runtime_enforced_rules: catalog
            .rules
            .iter()
            .filter(|rule| rule.trust_class == RuntimeRuleTrustClassV1::RuntimeEnforced)
            .count(),
        covered_rules,
        tested_rules,
        implemented_rules,
        documented_only_rules,
        ontology_only_rules,
        drifted_rules,
        unknown_rules,
        surface_reports,
        rule_statuses,
        runtime_theory_check,
        uncovered_rule_ids,
        missing_obligations,
        next_actions,
        notes,
    }
}

pub fn agent_engineering_report(
    meta: &MetaPlaneIndex,
    accepted_snapshot_id: Option<AcceptedSnapshotId>,
    lifecycle_state: &str,
    task: &AgentTaskRefV1,
    surfaces: &[ImplementationSurfaceRefV1],
    edges: &[CoverageEdgeV1],
    runtime_theory_check: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
) -> AgentEngineeringReportV1 {
    let surfaces = surfaces
        .iter()
        .map(ImplementationSurfaceRefV1::normalized)
        .collect::<Vec<_>>();
    let surface_reports = surfaces
        .iter()
        .map(|surface| {
            implementation_surface_rule_report(
                meta,
                accepted_snapshot_id.clone(),
                lifecycle_state,
                surface,
            )
        })
        .collect::<Vec<_>>();
    let coverage = semantic_coverage_report(
        meta,
        accepted_snapshot_id.clone(),
        lifecycle_state,
        &surfaces,
        edges,
        runtime_theory_check.clone(),
    );

    let mut matched_scope_ids = surfaces
        .iter()
        .flat_map(|surface| surface.scopes.iter().map(|scope| scope.scope_id.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    matched_scope_ids.sort();

    let mut matched_scope_refs = surfaces
        .iter()
        .flat_map(|surface| {
            surface
                .scopes
                .iter()
                .filter_map(|scope| scope.inferred_scope_ref())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    matched_scope_refs.sort();

    let mut matched_rule_ids = surface_reports
        .iter()
        .flat_map(|report| report.rules.iter().map(|rule| rule.rule_id.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    matched_rule_ids.sort();

    let lifecycle = lifecycle_state.trim().to_ascii_lowercase();
    let trust_class = if surface_reports
        .iter()
        .any(|report| report.trust_class == RuntimeRuleTrustClassV1::RuntimeEnforced)
    {
        RuntimeRuleTrustClassV1::RuntimeEnforced
    } else if surface_reports
        .iter()
        .any(|report| report.trust_class == RuntimeRuleTrustClassV1::RuntimeAdvisory)
    {
        RuntimeRuleTrustClassV1::RuntimeAdvisory
    } else {
        RuntimeRuleTrustClassV1::ReviewOnly
    };

    let strength = if lifecycle == "conflicted" {
        SemanticClaimStrengthV1::Conflicted
    } else if matched_rule_ids.is_empty() {
        SemanticClaimStrengthV1::Unknown
    } else if (lifecycle == "accepted" || lifecycle == "certified")
        && coverage
            .missing_obligations
            .iter()
            .all(|item| !item.contains("runtime-enforced rule"))
        && coverage.drifted_rules == 0
    {
        SemanticClaimStrengthV1::Strong
    } else {
        SemanticClaimStrengthV1::Weak
    };

    let mut residual_unknowns = Vec::new();
    if surfaces.is_empty() {
        residual_unknowns.push(
            "no implementation surfaces were supplied, so engineering applicability remains mostly implicit"
                .to_string(),
        );
    }
    if matched_scope_ids.is_empty() {
        residual_unknowns
            .push("task is not mapped to any ontology relation/theory scope yet".to_string());
    }
    if matched_rule_ids.is_empty() {
        residual_unknowns.push(
            "no typed business rules were resolved for the supplied implementation surfaces"
                .to_string(),
        );
    }
    if coverage.unknown_rules > 0 {
        residual_unknowns.push(format!(
            "{} rule(s) remain at unknown coverage status across the supplied implementation surfaces",
            coverage.unknown_rules
        ));
    }
    if let Some(summary) = runtime_theory_check.as_ref() {
        if summary.blocking_errors > 0 {
            residual_unknowns.push(format!(
                "runtime theory check has {} blocking error(s)",
                summary.blocking_errors
            ));
        }
        if !summary.residual_obligation_ids.is_empty() {
            residual_unknowns.push(format!(
                "{} runtime theory residual obligation(s) remain before this agent task has strong semantic coverage",
                summary.residual_obligation_ids.len()
            ));
        }
    }

    let mut next_actions_set = surface_reports
        .iter()
        .flat_map(|report| report.next_actions.iter().cloned())
        .collect::<BTreeSet<_>>();
    next_actions_set.extend(coverage.next_actions.iter().cloned());
    let next_actions = next_actions_set.into_iter().collect::<Vec<_>>();

    let mut notes = task.notes.clone();
    notes.push(
        "agent-facing engineering reports are scoped to accepted anchors, mapped implementation surfaces, and runtime-visible ontology objects; they do not claim full ontology closure or exhaustive implementation completeness"
            .to_string(),
    );
    if !task.languages.is_empty() {
        notes.push(format!(
            "task spans implementation languages/artifacts: {}",
            task.languages.join(", ")
        ));
    }
    if coverage.drifted_rules > 0 {
        notes.push(format!(
            "{} mapped rule(s) are explicitly drifted and should be reconciled before treating the task as semantically aligned",
            coverage.drifted_rules
        ));
    }
    if let Some(summary) = runtime_theory_check.as_ref() {
        notes.push(format!(
            "runtime theory check is attached for agent planning: completeness={}, ontology_closure={}",
            summary.completeness_claim, summary.ontology_closure_claim
        ));
    }

    AgentEngineeringReportV1 {
        version: AGENT_ENGINEERING_REPORT_VERSION_V1.to_string(),
        accepted_snapshot_id,
        lifecycle_state: lifecycle_state.to_string(),
        task: task.clone(),
        trust_class,
        strength,
        checked_surface: "agent_engineering_task".to_string(),
        matched_scope_ids,
        matched_scope_refs,
        matched_rule_ids,
        surface_reports,
        coverage,
        runtime_theory_check,
        residual_unknowns,
        next_actions,
        notes,
    }
}

fn summarize_rule_inventory_from_catalog(
    catalog: &RuntimeRuleCatalogV1,
) -> SemanticRuleInventoryV1 {
    let mut relation_names = BTreeSet::new();
    let mut theory_names = BTreeSet::new();
    let mut relation_constraints = 0usize;
    let mut rewrite_rules = 0usize;
    let mut named_block_constraints = 0usize;
    let mut runtime_checkable_rules = 0usize;
    let mut runtime_visible_rules = 0usize;
    let mut review_only_rules = 0usize;

    for rule in &catalog.rules {
        if rule.is_runtime_checkable() {
            runtime_checkable_rules += 1;
        }
        if rule.is_runtime_visible() {
            runtime_visible_rules += 1;
        }
        if rule.is_review_only() {
            review_only_rules += 1;
        }

        match rule.class {
            RuntimeRuleClassV1::RewriteRule => {
                rewrite_rules += 1;
            }
            RuntimeRuleClassV1::NamedBlockConstraint => {
                named_block_constraints += 1;
                if let Some(theory) = rule.scope.theory.as_ref() {
                    theory_names.insert(theory.clone());
                }
            }
            _ => {
                if rule.scope.scope_class == RuntimeRuleScopeClassV1::Relation {
                    relation_constraints += 1;
                    if let Some(relation) = rule.scope.relation.as_ref() {
                        relation_names.insert(relation.clone());
                    }
                }
            }
        }

        if let Some(theory) = rule.scope.theory.as_ref() {
            theory_names.insert(theory.clone());
        }
    }

    SemanticRuleInventoryV1 {
        total_rules: catalog.rules.len(),
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
        notes: catalog.notes.clone(),
    }
}

pub fn runtime_semantic_summary_for_preview(
    meta: &MetaPlaneIndex,
    trust: &ProposalValidationTrustContractV1,
    competency_gate: Option<&CompetencyGateReportV1>,
    quality_error_count: usize,
) -> RuntimeSemanticSummaryV1 {
    let catalog = runtime_rule_catalog(meta);
    let rule_inventory = summarize_rule_inventory_from_catalog(&catalog);
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

    let advisory_structured_rules = catalog
        .rules
        .iter()
        .filter(|rule| {
            rule.scope.scope_class == RuntimeRuleScopeClassV1::Relation
                && rule.trust_class == RuntimeRuleTrustClassV1::RuntimeAdvisory
        })
        .count();
    let review_only_structured_rules = catalog
        .rules
        .iter()
        .filter(|rule| {
            rule.scope.scope_class == RuntimeRuleScopeClassV1::Relation
                && rule.trust_class == RuntimeRuleTrustClassV1::ReviewOnly
        })
        .count();
    let structured_constraint_surface = if rule_inventory.runtime_checkable_rules > 0 {
        if advisory_structured_rules > 0 {
            gaps.push(
                "some structured business rules are visible to tooling, but remain advisory rather than enforced at runtime"
                    .to_string(),
            );
        }
        "partial_runtime_enforced_structured_constraints".to_string()
    } else if advisory_structured_rules > 0 {
        gaps.push(
            "structured business rules are indexed, but they currently act as typed metadata rather than fail-closed runtime checks"
                .to_string(),
        );
        "structured_rule_metadata_only".to_string()
    } else if review_only_structured_rules > 0 {
        gaps.push(
            "structured business rules are present, but remain review-only in the current runtime slice"
                .to_string(),
        );
        "review_only".to_string()
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
            "preview still has {quality_error_count} quality error(s) requiring operator or author intervention"
        ));
    }

    let quality_surface = "preview_quality_delta".to_string();
    let mut notes = trust.reasons.clone();
    notes.extend(catalog.notes.clone());
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
    use axiograph_dsl::axi_v1::parse_axi_v1;
    use axiograph_pathdb::axi_semantics::{
        NamedBlockConstraintDecl, RewriteRuleDecl, SchemaIndex, SubtypeDecl,
    };
    use axiograph_pathdb::{
        validate_axi_v1_module, RuntimeTheoryObligationFragmentStatusV1,
        RuntimeTheoryObligationTrustClassV1, TheoryObligationRefIr,
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

    fn sample_runtime_theory_summary() -> crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1 {
        crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1 {
            version: "runtime_theory_check_summary_v1".to_string(),
            report_version: "runtime_theory_check_report_v1".to_string(),
            module_digest: "fnv1a64:test".to_string(),
            theory_count: 1,
            checked_obligations: 2,
            review_only_obligations: 0,
            residual_obligations: 1,
            blocked_obligations: 0,
            excluded_by_evidence: 0,
            blocking_errors: 0,
            admissibility_scopes: vec!["finite_fragment".to_string()],
            admissibility_trace: Default::default(),
            transport_summary: Default::default(),
            completeness_claim: "not_claimed_for_all_obligations".to_string(),
            ontology_closure_claim: "not_claimed_for_all_obligations".to_string(),
            residual_obligation_ids: vec!["theory:family/residual/path".to_string()],
            notes: vec!["test sidecar".to_string()],
        }
    }

    fn sample_meta() -> MetaPlaneIndex {
        let mut meta = MetaPlaneIndex::default();
        meta.schemas.insert(
            "Family".to_string(),
            SchemaIndex {
                schema_name: "Family".to_string(),
                schema_entity: 1,
                module_name: Some("Family".to_string()),
                object_types: HashSet::new(),
                subtype_decls: vec![SubtypeDecl {
                    subtype_entity: 2,
                    sub: "Person".to_string(),
                    sup: "Agent".to_string(),
                }],
                relation_decls: HashMap::new(),
                constraints_by_relation: HashMap::from([
                    (
                        "parent".to_string(),
                        vec![
                            ConstraintDecl::Functional {
                                relation: "parent".to_string(),
                                src_field: "child".to_string(),
                                dst_field: "mother".to_string(),
                            },
                            ConstraintDecl::Typing {
                                relation: "parent".to_string(),
                                rule: "child,parent : Person".to_string(),
                            },
                            ConstraintDecl::Unknown {
                                relation: Some("parent".to_string()),
                                text: "custom parent policy".to_string(),
                            },
                        ],
                    ),
                    (
                        "ancestor".to_string(),
                        vec![
                            ConstraintDecl::AtMost {
                                relation: "ancestor".to_string(),
                                src_field: "child".to_string(),
                                dst_field: "generation".to_string(),
                                max: 2,
                                params: Some(vec!["ctx".to_string()]),
                            },
                            ConstraintDecl::Key {
                                relation: "ancestor".to_string(),
                                fields: vec!["child".to_string(), "generation".to_string()],
                            },
                        ],
                    ),
                ]),
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
        meta
    }

    fn sample_runtime_theory_fragment_summary() -> RuntimeTheoryFragmentSummaryV1 {
        let axi = r#"
module Family

schema Family:
  object Person
  relation parent(child: Person, parent: Person)

theory FamilyTheory on Family:
  constraint key parent(child, parent)
  equation opaque_business_rule:
    ParentCompose(a,b,c) = c
  rewrite parent_assoc:
    vars: x: Person, y: Person
    lhs: step(x, parent, y)
    rhs: step(x, parent, y)
"#;

        let module = parse_axi_v1(axi).expect("parse family theory summary module");
        let validated =
            validate_axi_v1_module(module).expect("typecheck family theory summary module");
        validated
            .runtime_theory_fragment_summary("Family", "FamilyTheory")
            .expect("runtime theory fragment summary")
            .expect("family theory summary present")
    }

    #[test]
    fn runtime_rule_catalog_assigns_stable_ids_scope_and_classes() {
        let catalog = runtime_rule_catalog(&sample_meta());
        assert_eq!(catalog.version, RUNTIME_RULE_CATALOG_VERSION_V1);

        let parent_rules = catalog.relation_rules("Family", "parent");
        assert_eq!(parent_rules.len(), 3);
        assert_eq!(
            parent_rules[0].rule_id,
            "schema/family/relation/parent/rule/functional/0"
        );
        assert_eq!(
            parent_rules[0].trust_class,
            RuntimeRuleTrustClassV1::RuntimeEnforced
        );
        assert_eq!(
            parent_rules[1].trust_class,
            RuntimeRuleTrustClassV1::RuntimeAdvisory
        );
        assert_eq!(
            parent_rules[1].runtime_support,
            RuntimeRuleSupportV1::TypedMetadataOnly
        );
        assert_eq!(
            parent_rules[2].trust_class,
            RuntimeRuleTrustClassV1::ReviewOnly
        );

        let theory_rules = catalog.theory_rules("Family", "FamilyTheory");
        assert_eq!(theory_rules.len(), 2);
        assert!(theory_rules
            .iter()
            .any(|rule| rule.class == RuntimeRuleClassV1::RewriteRule));
        assert!(theory_rules.iter().any(|rule| {
            rule.class == RuntimeRuleClassV1::NamedBlockConstraint
                && rule.rule_id
                    == "schema/family/theory/familytheory/rule/named-block/family-policy/0"
        }));
    }

    #[test]
    fn runtime_rule_report_for_relation_summarizes_enforced_and_review_only_rules() {
        let report = runtime_rule_report_for_relation(&sample_meta(), "Family", "parent");
        assert_eq!(report.version, RUNTIME_RULE_REPORT_VERSION_V1);
        assert_eq!(report.scope.scope_id, "schema/family/relation/parent");
        assert_eq!(report.total_rules, 3);
        assert_eq!(report.runtime_enforced_rules, 1);
        assert_eq!(report.runtime_visible_rules, 2);
        assert_eq!(report.review_only_rules, 1);
        assert_eq!(report.certificate_subset_rules, 0);
        assert!(report.notes.iter().any(|note| note.contains("review-only")));
    }

    #[test]
    fn runtime_rule_report_for_theory_tracks_certificate_subset_rules() {
        let report = runtime_rule_report_for_theory(&sample_meta(), "Family", "FamilyTheory");
        assert_eq!(report.scope.scope_id, "schema/family/theory/familytheory");
        assert_eq!(report.total_rules, 2);
        assert_eq!(report.runtime_enforced_rules, 0);
        assert_eq!(report.runtime_visible_rules, 1);
        assert_eq!(report.review_only_rules, 1);
        assert_eq!(report.certificate_subset_rules, 1);
        assert!(report
            .rules
            .iter()
            .any(|rule| rule.class == RuntimeRuleClassV1::RewriteRule));
    }

    #[test]
    fn business_rule_applicability_for_relation_marks_review_state_as_weak() {
        let report = business_rule_applicability_for_relation(
            &sample_meta(),
            Some(AcceptedSnapshotId::new("accepted:family")),
            "reviewed",
            "Family",
            "parent",
        );
        assert_eq!(
            report.version,
            BUSINESS_RULE_APPLICABILITY_REPORT_VERSION_V1
        );
        assert_eq!(
            report.accepted_snapshot_id,
            Some(AcceptedSnapshotId::new("accepted:family"))
        );
        assert_eq!(report.scope.scope_id, "schema/family/relation/parent");
        assert_eq!(report.trust_class, RuntimeRuleTrustClassV1::RuntimeEnforced);
        assert_eq!(report.strength, SemanticClaimStrengthV1::Weak);
        assert_eq!(report.checked_surface, "relation_scope");
        assert_eq!(report.runtime_enforced_rules, 1);
        assert_eq!(report.review_only_rules, 1);
        assert!(report
            .missing_obligations
            .iter()
            .any(|item| item.contains("review-only")));
        assert!(report
            .next_actions
            .iter()
            .any(|item| item.contains("review and promote")));
    }

    #[test]
    fn business_rule_applicability_for_theory_can_be_strong_when_accepted() {
        let report = business_rule_applicability_for_theory(
            &sample_meta(),
            Some(AcceptedSnapshotId::new("accepted:family")),
            "accepted",
            "Family",
            "FamilyTheory",
        );
        assert_eq!(report.scope.scope_id, "schema/family/theory/familytheory");
        assert_eq!(report.trust_class, RuntimeRuleTrustClassV1::RuntimeAdvisory);
        assert_eq!(report.strength, SemanticClaimStrengthV1::Weak);
        assert_eq!(report.certificate_subset_rules, 1);
        assert!(report.runtime_theory_fragment_summary.is_none());
        assert!(report
            .notes
            .iter()
            .any(|note| note.contains("certificate-emittable subset")));
    }

    #[test]
    fn business_rule_applicability_for_theory_threads_runtime_fragment_summary() {
        let fragment_summary = sample_runtime_theory_fragment_summary();
        let report = business_rule_applicability_for_theory_with_runtime_fragment(
            &sample_meta(),
            Some(AcceptedSnapshotId::new("accepted:family")),
            "accepted",
            "Family",
            "FamilyTheory",
            fragment_summary.clone(),
        )
        .expect("theory fragment summary should match requested theory");

        let attached = report
            .runtime_theory_fragment_summary
            .as_ref()
            .expect("attached runtime theory fragment summary");
        assert_eq!(attached.theory_ref, fragment_summary.theory_ref);
        assert_eq!(attached.runtime_checked_obligations, 2);
        assert_eq!(attached.opaque_or_out_of_fragment_obligations, 1);
        assert!(attached.obligation_statuses.iter().any(|status| {
            matches!(
                status.obligation_ref,
                TheoryObligationRefIr::OpaqueEquation { ref name, .. }
                    if name == "opaque_business_rule"
            ) && status.fragment_status
                == RuntimeTheoryObligationFragmentStatusV1::OpaqueOrOutOfFragment
                && status.trust_class == RuntimeTheoryObligationTrustClassV1::ReviewOnly
        }));
        assert!(attached.obligation_statuses.iter().any(|status| {
            matches!(
                status.obligation_ref,
                TheoryObligationRefIr::RewriteRule { ref name, .. }
                    if name == "parent_assoc"
            ) && status.fragment_status == RuntimeTheoryObligationFragmentStatusV1::RuntimeChecked
                && status.trust_class == RuntimeTheoryObligationTrustClassV1::RuntimeAdvisory
        }));
        assert!(report
            .missing_obligations
            .iter()
            .any(|item| item.contains("opaque or outside the current runtime theory fragment")));
        assert!(report
            .notes
            .iter()
            .any(|note| note.contains("outside the trusted-kernel/certificate boundary")));
    }

    #[test]
    fn business_rule_applicability_for_theory_rejects_mismatched_runtime_fragment_summary() {
        let mut fragment_summary = sample_runtime_theory_fragment_summary();
        fragment_summary.theory_ref = TheorySubjectRefIr::Theory {
            theory_id: TheoryId::new("theory:Family:OtherTheory"),
        };

        let err = business_rule_applicability_for_theory_with_runtime_fragment(
            &sample_meta(),
            Some(AcceptedSnapshotId::new("accepted:family")),
            "accepted",
            "Family",
            "FamilyTheory",
            fragment_summary,
        )
        .expect_err("mismatched runtime fragment summary should be rejected");

        assert!(err
            .to_string()
            .contains("runtime theory fragment summary references"));
    }

    #[test]
    fn business_rule_applicability_for_missing_scope_is_unknown() {
        let report = business_rule_applicability_for_relation(
            &sample_meta(),
            None,
            "accepted",
            "Family",
            "missing_relation",
        );
        assert_eq!(report.strength, SemanticClaimStrengthV1::Unknown);
        assert!(report.rules.is_empty());
        assert!(report
            .missing_obligations
            .iter()
            .any(|item| item.contains("no typed business rules")));
    }

    #[test]
    fn runtime_rule_scope_normalized_infers_missing_relation_scope_id() {
        let raw: RuntimeRuleScopeV1 = serde_json::from_value(serde_json::json!({
            "schema": "Family",
            "scope_class": "relation",
            "relation": "parent"
        }))
        .expect("deserialize relation scope without scope_id");

        let normalized = raw.normalized();
        assert_eq!(normalized.scope_id, "schema/family/relation/parent");
        assert_eq!(
            normalized.scope_ref,
            RuntimeRuleScopeV1::relation("Family", "parent").scope_ref
        );
    }

    #[test]
    fn implementation_surface_rule_report_aggregates_mapped_scopes() {
        let report = implementation_surface_rule_report(
            &sample_meta(),
            Some(AcceptedSnapshotId::new("accepted:family")),
            "accepted",
            &ImplementationSurfaceRefV1 {
                surface_id: "endpoint:family_tree".to_string(),
                kind: ImplementationSurfaceKindV1::Endpoint,
                label: "GET /family/tree".to_string(),
                scopes: vec![
                    RuntimeRuleScopeV1::relation("Family", "parent"),
                    RuntimeRuleScopeV1::theory("Family", "FamilyTheory"),
                ],
                code_refs: vec!["src/family/tree.rs".to_string()],
                notes: Vec::new(),
            },
        );
        assert_eq!(
            report.version,
            IMPLEMENTATION_SURFACE_RULE_REPORT_VERSION_V1
        );
        assert_eq!(report.surface.surface_id, "endpoint:family_tree");
        assert_eq!(report.runtime_enforced_rules, 1);
        assert_eq!(report.review_only_rules, 2);
        assert_eq!(report.certificate_subset_rules, 1);
        assert!(report.rules.iter().any(|rule| matches!(
            rule.scope.scope_ref.as_ref(),
            Some(TheorySubjectRefIr::Relation { relation_name, .. }) if relation_name == "parent"
        )));
        assert!(report
            .missing_obligations
            .iter()
            .any(|item| item.contains("review-only")));
    }

    #[test]
    fn semantic_coverage_report_marks_uncovered_runtime_rules() {
        let coverage = semantic_coverage_report(
            &sample_meta(),
            Some(AcceptedSnapshotId::new("accepted:family")),
            "accepted",
            &[ImplementationSurfaceRefV1 {
                surface_id: "endpoint:family_tree".to_string(),
                kind: ImplementationSurfaceKindV1::Endpoint,
                label: "GET /family/tree".to_string(),
                scopes: vec![RuntimeRuleScopeV1::relation("Family", "parent")],
                code_refs: vec!["src/family/tree.rs".to_string()],
                notes: Vec::new(),
            }],
            &[CoverageEdgeV1 {
                surface_id: "endpoint:family_tree".to_string(),
                rule_id: "schema/family/relation/parent/rule/functional/0".to_string(),
                status: CoverageStatusV1::Implemented,
                notes: vec!["enforced in handler".to_string()],
            }],
            None,
        );

        assert_eq!(coverage.version, SEMANTIC_COVERAGE_REPORT_VERSION_V1);
        assert_eq!(coverage.total_rules, 7);
        assert_eq!(coverage.covered_rules, 1);
        assert_eq!(coverage.implemented_rules, 1);
        assert!(coverage
            .uncovered_rule_ids
            .iter()
            .any(|id| id == "schema/family/relation/ancestor/rule/at-most/0"));
        assert!(coverage.missing_obligations.iter().any(|item| item
            .contains("runtime-enforced rule `schema/family/relation/ancestor/rule/at-most/0`")));
        assert!(coverage.rule_statuses.iter().any(|status| matches!(
            status.scope_ref.as_ref(),
            Some(TheorySubjectRefIr::Relation { relation_name, .. }) if relation_name == "parent"
        )));
    }

    #[test]
    fn semantic_coverage_report_keeps_drift_explicit() {
        let coverage = semantic_coverage_report(
            &sample_meta(),
            Some(AcceptedSnapshotId::new("accepted:family")),
            "accepted",
            &[ImplementationSurfaceRefV1 {
                surface_id: "job:family_reconcile".to_string(),
                kind: ImplementationSurfaceKindV1::Job,
                label: "family reconcile job".to_string(),
                scopes: vec![RuntimeRuleScopeV1::relation("Family", "ancestor")],
                code_refs: vec!["jobs/family/reconcile.rs".to_string()],
                notes: Vec::new(),
            }],
            &[CoverageEdgeV1 {
                surface_id: "job:family_reconcile".to_string(),
                rule_id: "schema/family/relation/ancestor/rule/at-most/0".to_string(),
                status: CoverageStatusV1::Drifted,
                notes: vec!["job no longer respects ctx parameter".to_string()],
            }],
            None,
        );

        assert_eq!(coverage.drifted_rules, 1);
        assert!(coverage
            .missing_obligations
            .iter()
            .any(|item| item.contains("marked drifted")));
        assert!(coverage
            .notes
            .iter()
            .all(|note| !note.contains("unknown implementation surface")));
    }

    #[test]
    fn semantic_coverage_report_surfaces_runtime_theory_summary() {
        let summary = sample_runtime_theory_summary();
        let coverage = semantic_coverage_report(
            &sample_meta(),
            Some(AcceptedSnapshotId::new("accepted:family")),
            "accepted",
            &[ImplementationSurfaceRefV1 {
                surface_id: "endpoint:family_tree".to_string(),
                kind: ImplementationSurfaceKindV1::Endpoint,
                label: "GET /family/tree".to_string(),
                scopes: vec![RuntimeRuleScopeV1::relation("Family", "parent")],
                code_refs: vec!["src/family/tree.rs".to_string()],
                notes: Vec::new(),
            }],
            &[],
            Some(summary),
        );

        assert!(coverage.runtime_theory_check.is_some());
        assert!(coverage
            .missing_obligations
            .iter()
            .any(|item| item.contains("runtime theory residual obligation")));
        assert!(coverage
            .notes
            .iter()
            .any(|item| item.contains("runtime theory check summary attached")));
    }

    #[test]
    fn agent_engineering_report_composes_surface_rules_and_coverage() {
        let report = agent_engineering_report(
            &sample_meta(),
            Some(AcceptedSnapshotId::new("accepted:family")),
            "accepted",
            &AgentTaskRefV1 {
                task_id: "task:family_api_alignment".to_string(),
                label: "Align family API to ontology".to_string(),
                objective: Some(
                    "check whether the family endpoint and reconcile job implement accepted semantics"
                        .to_string(),
                ),
                languages: vec!["rust".to_string(), "typescript".to_string()],
                artifact_refs: vec!["src/family/tree.rs".to_string(), "ui/family.tsx".to_string()],
                notes: vec!["review generated implementation plan before promotion".to_string()],
            },
            &[
                ImplementationSurfaceRefV1 {
                    surface_id: "endpoint:family_tree".to_string(),
                    kind: ImplementationSurfaceKindV1::Endpoint,
                    label: "GET /family/tree".to_string(),
                    scopes: vec![RuntimeRuleScopeV1::relation("Family", "parent")],
                    code_refs: vec!["src/family/tree.rs".to_string()],
                    notes: Vec::new(),
                },
                ImplementationSurfaceRefV1 {
                    surface_id: "job:family_reconcile".to_string(),
                    kind: ImplementationSurfaceKindV1::Job,
                    label: "family reconcile job".to_string(),
                    scopes: vec![RuntimeRuleScopeV1::relation("Family", "ancestor")],
                    code_refs: vec!["jobs/family/reconcile.rs".to_string()],
                    notes: Vec::new(),
                },
            ],
            &[
                CoverageEdgeV1 {
                    surface_id: "endpoint:family_tree".to_string(),
                    rule_id: "schema/family/relation/parent/rule/functional/0".to_string(),
                    status: CoverageStatusV1::Implemented,
                    notes: vec!["enforced in endpoint".to_string()],
                },
                CoverageEdgeV1 {
                    surface_id: "job:family_reconcile".to_string(),
                    rule_id: "schema/family/relation/ancestor/rule/at-most/0".to_string(),
                    status: CoverageStatusV1::Drifted,
                    notes: vec!["job no longer respects ctx parameter".to_string()],
                },
            ],
            None,
        );

        assert_eq!(report.version, AGENT_ENGINEERING_REPORT_VERSION_V1);
        assert_eq!(report.checked_surface, "agent_engineering_task");
        assert_eq!(report.task.task_id, "task:family_api_alignment");
        assert_eq!(report.trust_class, RuntimeRuleTrustClassV1::RuntimeEnforced);
        assert_eq!(report.strength, SemanticClaimStrengthV1::Weak);
        assert!(report
            .matched_scope_ids
            .iter()
            .any(|id| id == "schema/family/relation/parent"));
        assert!(report.matched_scope_refs.iter().any(|scope| matches!(
            scope,
            TheorySubjectRefIr::Relation { relation_name, .. } if relation_name == "parent"
        )));
        assert!(report
            .matched_rule_ids
            .iter()
            .any(|id| id == "schema/family/relation/parent/rule/functional/0"));
        assert_eq!(report.surface_reports.len(), 2);
        assert_eq!(report.coverage.drifted_rules, 1);
        assert!(report
            .next_actions
            .iter()
            .any(|item| item.contains("reconcile ontology and code/tests")));
        assert!(report
            .notes
            .iter()
            .any(|item| item.contains("do not claim full ontology closure")));
    }

    #[test]
    fn runtime_semantic_summary_counts_rule_surfaces_and_gaps() {
        let summary =
            runtime_semantic_summary_for_preview(&sample_meta(), &sample_trust(), None, 0);
        assert_eq!(summary.rule_inventory.total_rules, 7);
        assert_eq!(summary.rule_inventory.relation_constraints, 5);
        assert_eq!(summary.rule_inventory.rewrite_rules, 1);
        assert_eq!(summary.rule_inventory.named_block_constraints, 1);
        assert_eq!(summary.rule_inventory.runtime_checkable_rules, 3);
        assert_eq!(summary.rule_inventory.runtime_visible_rules, 5);
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
        assert!(summary
            .semantic_coverage
            .gaps
            .iter()
            .any(|gap| gap.contains("advisory rather than enforced")));
    }

    #[test]
    fn runtime_semantic_summary_marks_metadata_only_structured_rules() {
        let mut meta = MetaPlaneIndex::default();
        meta.schemas.insert(
            "Family".to_string(),
            SchemaIndex {
                schema_name: "Family".to_string(),
                schema_entity: 1,
                module_name: Some("Family".to_string()),
                object_types: HashSet::new(),
                subtype_decls: Vec::new(),
                relation_decls: HashMap::new(),
                constraints_by_relation: HashMap::from([(
                    "parent".to_string(),
                    vec![ConstraintDecl::Typing {
                        relation: "parent".to_string(),
                        rule: "child,parent : Person".to_string(),
                    }],
                )]),
                rewrite_rules_by_theory: HashMap::new(),
                named_block_constraints_by_theory: HashMap::new(),
                supertypes_of: HashMap::new(),
            },
        );

        let summary = runtime_semantic_summary_for_preview(&meta, &sample_trust(), None, 0);
        assert_eq!(summary.rule_inventory.runtime_checkable_rules, 0);
        assert_eq!(summary.rule_inventory.runtime_visible_rules, 1);
        assert_eq!(
            summary.semantic_coverage.structured_constraint_surface,
            "structured_rule_metadata_only"
        );
        assert!(summary
            .semantic_coverage
            .gaps
            .iter()
            .any(|gap| gap.contains("typed metadata rather than fail-closed runtime checks")));
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
