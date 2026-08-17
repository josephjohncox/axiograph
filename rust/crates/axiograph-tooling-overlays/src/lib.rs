use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use axiograph_pathdb::kernel_ir::{
    RelationSemanticsIr, RuntimeIrRef, RuntimeModuleIndex, RuntimeSchemaIndex,
};
use axiograph_pathdb::KernelRefV2;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const TOOLING_OVERLAY_BUNDLE_VERSION_V1: &str = "tooling_overlay_bundle_v1";
pub const OVERLAY_VALIDATION_REPORT_VERSION_V1: &str = "overlay_validation_report_v1";
pub const OVERLAY_SOFTWARE_COVERAGE_REPORT_VERSION_V1: &str = "overlay_software_coverage_report_v1";
pub const COVERAGE_QUERY_REPORT_VERSION_V1: &str = "coverage_query_report_v1";
pub const COVERAGE_QUERY_VERSION_V1: &str = "coverage_query_v1";
pub const DEFINITION_QUERY_BUNDLE_VERSION_V1: &str = "definition_query_bundle_v1";
pub const DEFINITION_QUERY_REPORT_VERSION_V1: &str = "definition_query_report_v1";
pub const DEFINITION_QUERY_VERSION_V1: &str = "definition_query_v1";
pub const CODEGEN_PLAN_REPORT_VERSION_V1: &str = "codegen_plan_report_v1";
pub const AUTHORING_FLOW_REPORT_VERSION_V1: &str = "authoring_flow_report_v1";
pub const MAX_QUERY_ITEMS_V1: usize = 1_024;
pub const MAX_QUERY_STRING_BYTES_V1: usize = 16 * 1024;
pub const MAX_QUERY_TOTAL_BYTES_V1: usize = 1024 * 1024;
pub const MAX_QUERY_MATCHES_V1: usize = 1_024;

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CoverageModeV1 {
    Enforced,
    #[default]
    Advisory,
    Exploratory,
    DefinitionQuery,
    InsufficientlyGrounded,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthoringCoverageProfileV1 {
    Advisory,
    Strict,
    Ci,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthoringFlowSourceV1 {
    ContinuousCheck,
    OverlaySoftwareCoverage,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthoringFlowStatusV1 {
    Passed,
    PassedWithWarnings,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct AuthoringCoverageProfileSummaryV1 {
    pub profile: AuthoringCoverageProfileV1,
    pub coverage_mode: CoverageModeV1,
    #[serde(default)]
    pub strict_coverage: bool,
    #[serde(default)]
    pub require_code_refs: bool,
    #[serde(default)]
    pub require_runtime_theory: bool,
    #[serde(default)]
    pub fail_on_unresolved_obligations: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_codegen_languages: Vec<String>,
    #[serde(default)]
    pub ci_ready: bool,
    #[serde(default)]
    pub advisory_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub struct AuthoringCoverageSummaryV1 {
    #[serde(default)]
    pub total_rules: u64,
    #[serde(default)]
    pub covered_rules: u64,
    #[serde(default)]
    pub tested_rules: u64,
    #[serde(default)]
    pub implemented_rules: u64,
    #[serde(default)]
    pub drifted_rules: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_obligations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uncovered_rule_ids: Vec<String>,
    #[serde(default)]
    pub code_refs_total: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_code_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_codegen_languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub present_codegen_languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_codegen_languages: Vec<String>,
    #[serde(default)]
    pub runtime_theory_present: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_theory_residual_obligations: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_theory_blocking_obligations: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct AuthoringFlowReportV1 {
    pub version: String,
    pub source: AuthoringFlowSourceV1,
    pub status: AuthoringFlowStatusV1,
    pub pass: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case_id: Option<String>,
    pub profile: AuthoringCoverageProfileSummaryV1,
    pub coverage: AuthoringCoverageSummaryV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failures: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<String>,
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(rename_all = "snake_case")]
pub enum OverlayRefKindV1 {
    Schema,
    Object,
    Relation,
    Theory,
    Constraint,
    Equation,
    Rewrite,
    Instance,
    Fact,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord)]
pub struct OverlayRefV1 {
    pub kind: OverlayRefKindV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stable_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct OverlayBindingV1 {
    pub binding_id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ontology_refs: Vec<OverlayRefV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct FdddContextMapV1 {
    pub context_id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<OverlayRefV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bounded_contexts: Vec<OverlayBindingV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aggregates: Vec<OverlayBindingV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<OverlayBindingV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub processes: Vec<OverlayBindingV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub business_rules: Vec<OverlayBindingV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
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
    Ui,
    Plc,
    Simulator,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ImplementationSurfaceOverlayV1 {
    pub surface_id: String,
    pub kind: ImplementationSurfaceKindV1,
    pub label: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ontology_refs: Vec<OverlayRefV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub code_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoverageStatusV1 {
    Tested,
    Implemented,
    DocumentedOnly,
    OntologyOnly,
    Drifted,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CoverageEdgeOverlayV1 {
    pub surface_id: String,
    pub rule_id: String,
    pub status: CoverageStatusV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub struct ImplementationSurfaceManifestV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surfaces: Vec<ImplementationSurfaceOverlayV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub coverage_edges: Vec<CoverageEdgeOverlayV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CoveragePolicyV1 {
    #[serde(default)]
    pub coverage_mode: CoverageModeV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub require_codegen_languages: Vec<String>,
    #[serde(default)]
    pub strict_coverage: bool,
    #[serde(default)]
    pub require_code_refs: bool,
    #[serde(default)]
    pub require_runtime_theory: bool,
    #[serde(default)]
    pub fail_on_unresolved_obligations: bool,
}

impl Default for CoveragePolicyV1 {
    fn default() -> Self {
        Self {
            coverage_mode: CoverageModeV1::Advisory,
            require_codegen_languages: Vec::new(),
            strict_coverage: false,
            require_code_refs: false,
            require_runtime_theory: false,
            fail_on_unresolved_obligations: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CodegenPlanV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl Default for CodegenPlanV1 {
    fn default() -> Self {
        Self {
            languages: vec!["rust".to_string(), "typescript".to_string()],
            test_name: None,
            notes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ToolingOverlayBundleV1 {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fddd_context_map: Option<FdddContextMapV1>,
    #[serde(default)]
    pub implementation_surfaces: ImplementationSurfaceManifestV1,
    #[serde(default)]
    pub coverage_policy: CoveragePolicyV1,
    #[serde(default)]
    pub codegen_plan: CodegenPlanV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct OverlayDiagnosticV1 {
    pub severity: String,
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub struct OverlayRefResolutionSummaryV1 {
    #[serde(default)]
    pub total_refs: usize,
    #[serde(default)]
    pub resolved_refs: usize,
    #[serde(default)]
    pub unresolved_refs: usize,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub refs_by_kind: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct NormalizedOverlayRefV1 {
    pub source: OverlayRefV1,
    pub normalized_id: String,
    pub label: String,
    pub kernel_ref_label: String,
    pub kernel_ref: RuntimeIrRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct OverlayValidationReportV1 {
    pub version: String,
    pub coverage_mode: CoverageModeV1,
    pub valid: bool,
    pub module_digest: String,
    #[serde(default)]
    pub ref_summary: OverlayRefResolutionSummaryV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub normalized_refs: Vec<NormalizedOverlayRefV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<OverlayDiagnosticV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct CoverageQueryV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub coverage_mode: CoverageModeV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub terms: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relation_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cq_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub code_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surface_hints: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axql: Option<String>,
    #[serde(default)]
    pub max_matches: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct WeakCoverageProbeV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub terms: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub code_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surface_hints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CoverageQueryReportV1 {
    pub version: String,
    pub coverage_mode: CoverageModeV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_refs: Vec<DefinitionCandidateV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_surfaces: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unmatched_terms: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub caveats: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DefinitionQueryKindV1 {
    Process,
    Function,
    BusinessRule,
    DomainObject,
    Relation,
    Invariant,
    Policy,
    ImplementationSurface,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DefinitionQueryV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind_hint: Option<DefinitionQueryKindV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidate_refs: Vec<OverlayRefV1>,
    #[serde(default)]
    pub max_matches: Option<usize>,
    #[serde(default)]
    pub include_queries: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DefinitionQueryBundleV1 {
    pub version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queries: Vec<DefinitionQueryV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DefinitionCandidateV1 {
    pub ref_id: String,
    pub kind: OverlayRefKindV1,
    pub label: String,
    pub score: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_terms: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_axql: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DefinitionQueryReportV1 {
    pub version: String,
    pub coverage_mode: CoverageModeV1,
    pub classified_kind: DefinitionQueryKindV1,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<DefinitionCandidateV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub likely_relation_matches: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grounding_notes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved_ambiguities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recommended_followups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CodegenLanguagePlanV1 {
    pub language: String,
    pub file_hint: String,
    #[serde(default)]
    pub required_by_policy: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surface_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ontology_ref_labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CodegenPlanReportV1 {
    pub version: String,
    pub coverage_mode: CoverageModeV1,
    pub plan: CodegenPlanV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub language_plans: Vec<CodegenLanguagePlanV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_hints: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub caveats: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeTheoryAdmissibilityTraceSummaryV1 {
    #[serde(default)]
    pub total_steps: usize,
    #[serde(default)]
    pub checked_seed_steps: usize,
    #[serde(default)]
    pub evidence_filtered_steps: usize,
    #[serde(default)]
    pub review_residual_steps: usize,
    #[serde(default)]
    pub blocking_error_steps: usize,
    #[serde(default)]
    pub admissibility_scan_complete_steps: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeTheoryTransportSummaryV1 {
    #[serde(default)]
    pub preserved_obligations: usize,
    #[serde(default)]
    pub transported_obligations: usize,
    #[serde(default)]
    pub missing_object_image_obligations: usize,
    #[serde(default)]
    pub missing_arrow_image_obligations: usize,
    #[serde(default)]
    pub opaque_or_out_of_fragment_obligations: usize,
    #[serde(default)]
    pub resolver_required_obligations: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeTheoryScopeSummaryV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fragments: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub world_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_policy_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub included_imports: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeTheoryNonClaimSummaryV1 {
    pub code: String,
    pub message: String,
}

impl From<axiograph_pathdb::RuntimeTheoryNonClaimV1> for RuntimeTheoryNonClaimSummaryV1 {
    fn from(value: axiograph_pathdb::RuntimeTheoryNonClaimV1) -> Self {
        Self {
            code: value.code,
            message: value.message,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeTheoryCheckSummaryV1 {
    pub version: String,
    pub report_version: String,
    pub module_digest: String,
    #[serde(default)]
    pub theory_count: usize,
    #[serde(default)]
    pub checked_obligations: usize,
    #[serde(default)]
    pub review_only_obligations: usize,
    #[serde(default)]
    pub residual_obligations: usize,
    #[serde(default)]
    pub blocked_obligations: usize,
    #[serde(default)]
    pub excluded_by_evidence: usize,
    #[serde(default)]
    pub blocking_errors: usize,
    #[serde(default)]
    pub scope: RuntimeTheoryScopeSummaryV1,
    #[serde(default)]
    pub admissibility_trace: RuntimeTheoryAdmissibilityTraceSummaryV1,
    #[serde(default)]
    pub transport_summary: RuntimeTheoryTransportSummaryV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_obligation_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_claims: Vec<RuntimeTheoryNonClaimSummaryV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl RuntimeTheoryCheckSummaryV1 {
    pub fn gate_blockers(&self) -> Vec<String> {
        let mut blockers = Vec::new();
        if self.version != "runtime_theory_check_summary_v1" {
            blockers.push(format!(
                "unsupported runtime theory summary `{}`",
                self.version
            ));
        }
        if self.report_version != "runtime_theory_check_report_v1" {
            blockers.push(format!(
                "unsupported runtime theory report `{}`",
                self.report_version
            ));
        }
        let digest = self
            .module_digest
            .strip_prefix("axi:revision:v2:sha256:")
            .unwrap_or_default();
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            blockers.push("runtime theory summary has an invalid exact-module anchor".to_string());
        }
        if self.scope.fragments.is_empty() {
            blockers.push("runtime theory summary omits its finite fragment scope".to_string());
        }
        if !self
            .non_claims
            .iter()
            .any(|claim| claim.code == "closure_engine_not_implemented")
        {
            blockers.push("runtime theory summary omits the closure-engine non-claim".to_string());
        }
        if self.excluded_by_evidence > 0 {
            blockers.push(format!(
                "{} runtime theory obligation(s) were excluded by evidence",
                self.excluded_by_evidence
            ));
        }
        if self.admissibility_trace.review_residual_steps > 0
            || self.admissibility_trace.blocking_error_steps > 0
        {
            blockers
                .push("runtime theory admissibility trace contains non-checked steps".to_string());
        }
        if self.transport_summary.resolver_required_obligations > 0 {
            blockers.push(format!(
                "{} runtime theory transport obligation(s) require a resolver",
                self.transport_summary.resolver_required_obligations
            ));
        }
        if self.review_only_obligations > 0 {
            blockers.push(format!(
                "{} runtime theory obligation(s) remain review-only",
                self.review_only_obligations
            ));
        }
        if self.residual_obligations > 0 {
            blockers.push(format!(
                "{} runtime theory obligation(s) remain residual",
                self.residual_obligations
            ));
        }
        if self.blocked_obligations > 0 || self.blocking_errors > 0 {
            blockers.push(format!(
                "{} runtime theory obligation(s) are blocked",
                self.blocked_obligations
                    .saturating_add(self.blocking_errors)
            ));
        }
        if !self.residual_obligation_ids.is_empty() {
            blockers.push(format!(
                "runtime theory summary retains residual ids: {}",
                self.residual_obligation_ids.join(", ")
            ));
        }
        blockers
    }
}

pub fn runtime_theory_check_summary_v1(
    module_digest: &str,
    reports: &[axiograph_pathdb::RuntimeTheoryCheckReportV1],
    blocking_errors: usize,
    mut notes: Vec<String>,
) -> RuntimeTheoryCheckSummaryV1 {
    let mut fragments = reports
        .iter()
        .map(|report| report.fragment.closure_tier.as_str().to_string())
        .collect::<Vec<_>>();
    fragments.sort();
    fragments.dedup();
    let mut world_ids = reports
        .iter()
        .map(|report| report.world.world_id.clone())
        .collect::<Vec<_>>();
    world_ids.sort();
    world_ids.dedup();
    let mut evidence_policy_ids = reports
        .iter()
        .map(|report| report.evidence_policy.policy_id.clone())
        .collect::<Vec<_>>();
    evidence_policy_ids.sort();
    evidence_policy_ids.dedup();
    let mut included_imports = reports
        .iter()
        .flat_map(|report| report.world.included_imports.iter().cloned())
        .collect::<Vec<_>>();
    included_imports.sort();
    included_imports.dedup();
    let mut residual_obligation_ids = reports
        .iter()
        .flat_map(|report| {
            report
                .admissibility_scan
                .residual_obligations
                .iter()
                .cloned()
        })
        .collect::<Vec<_>>();
    residual_obligation_ids.sort();
    residual_obligation_ids.dedup();
    if blocking_errors > 0 {
        notes.push(format!(
            "{blocking_errors} runtime theory judgment(s) are blocking"
        ));
    }
    let mut admissibility_trace = RuntimeTheoryAdmissibilityTraceSummaryV1::default();
    for step in reports
        .iter()
        .flat_map(|report| report.admissibility_scan.steps.iter())
    {
        admissibility_trace.total_steps += 1;
        match step.kind {
            axiograph_pathdb::RuntimeTheoryClosureStepKindV1::CheckedSeed => {
                admissibility_trace.checked_seed_steps += 1;
            }
            axiograph_pathdb::RuntimeTheoryClosureStepKindV1::EvidenceFiltered => {
                admissibility_trace.evidence_filtered_steps += 1;
            }
            axiograph_pathdb::RuntimeTheoryClosureStepKindV1::ReviewResidual => {
                admissibility_trace.review_residual_steps += 1;
            }
            axiograph_pathdb::RuntimeTheoryClosureStepKindV1::BlockingError => {
                admissibility_trace.blocking_error_steps += 1;
            }
            axiograph_pathdb::RuntimeTheoryClosureStepKindV1::AdmissibilityScanComplete => {
                admissibility_trace.admissibility_scan_complete_steps += 1;
            }
        }
    }
    let mut transport_summary = RuntimeTheoryTransportSummaryV1::default();
    for judgment in reports.iter().flat_map(|report| report.judgments.iter()) {
        let Some(status) = judgment.transport_status else {
            continue;
        };
        match status {
            axiograph_pathdb::kernel_ir::TheoryTransportStatusIr::Preserved => {
                transport_summary.preserved_obligations += 1;
            }
            axiograph_pathdb::kernel_ir::TheoryTransportStatusIr::Transported => {
                transport_summary.transported_obligations += 1;
            }
            axiograph_pathdb::kernel_ir::TheoryTransportStatusIr::MissingObjectImage => {
                transport_summary.missing_object_image_obligations += 1;
            }
            axiograph_pathdb::kernel_ir::TheoryTransportStatusIr::MissingArrowImage => {
                transport_summary.missing_arrow_image_obligations += 1;
            }
            axiograph_pathdb::kernel_ir::TheoryTransportStatusIr::OpaqueOrOutOfFragment => {
                transport_summary.opaque_or_out_of_fragment_obligations += 1;
            }
        }
        if status.requires_resolver() {
            transport_summary.resolver_required_obligations += 1;
        }
    }
    let mut non_claims = reports
        .iter()
        .flat_map(|report| report.non_claims.iter().cloned())
        .map(RuntimeTheoryNonClaimSummaryV1::from)
        .collect::<Vec<_>>();
    non_claims
        .sort_by(|left, right| (&left.code, &left.message).cmp(&(&right.code, &right.message)));
    non_claims.dedup();

    RuntimeTheoryCheckSummaryV1 {
        version: "runtime_theory_check_summary_v1".to_string(),
        report_version: axiograph_pathdb::RUNTIME_THEORY_CHECK_REPORT_VERSION_V1.to_string(),
        module_digest: module_digest.to_string(),
        theory_count: reports.len(),
        checked_obligations: reports
            .iter()
            .map(|report| report.checked_obligations)
            .sum(),
        review_only_obligations: reports
            .iter()
            .map(|report| report.review_only_obligations)
            .sum(),
        residual_obligations: reports
            .iter()
            .map(|report| report.residual_obligations)
            .sum(),
        blocked_obligations: reports
            .iter()
            .map(|report| report.blocked_obligations)
            .sum(),
        excluded_by_evidence: reports
            .iter()
            .map(|report| report.excluded_by_evidence)
            .sum(),
        blocking_errors,
        scope: RuntimeTheoryScopeSummaryV1 {
            fragments,
            world_ids,
            evidence_policy_ids,
            included_imports,
        },
        admissibility_trace,
        transport_summary,
        residual_obligation_ids,
        non_claims,
        notes,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub struct BehaviorCaseReceiptCoverageViewV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub struct BehaviorCaseContextCoverageViewV1 {
    #[serde(default)]
    pub coverage: AuthoringCoverageSummaryV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_theory_check: Option<RuntimeTheoryCheckSummaryV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub struct BehaviorCaseCodegenPreviewCoverageViewV1 {
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub struct BehaviorCaseCoverageViewV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub behavior_case: BehaviorCaseReceiptCoverageViewV1,
    #[serde(default)]
    pub receipt: BehaviorCaseReceiptCoverageViewV1,
    #[serde(default)]
    pub context_report: BehaviorCaseContextCoverageViewV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_theory_check: Option<RuntimeTheoryCheckSummaryV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub codegen_previews: Vec<BehaviorCaseCodegenPreviewCoverageViewV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CoveragePolicySummaryV1 {
    pub coverage_mode: CoverageModeV1,
    #[serde(default)]
    pub strict_coverage: bool,
    #[serde(default)]
    pub require_code_refs: bool,
    #[serde(default)]
    pub require_runtime_theory: bool,
    #[serde(default)]
    pub fail_on_unresolved_obligations: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub struct ContinuousCoverageTypedRefsV1 {
    #[serde(default)]
    pub ontology_refs: OverlayRefResolutionSummaryV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surface_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rule_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OverlaySoftwareCoverageReportV1 {
    pub version: String,
    pub coverage_mode: CoverageModeV1,
    pub coverage_policy: CoveragePolicySummaryV1,
    pub authoring_flow: AuthoringFlowReportV1,
    pub pass: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case_id: Option<String>,
    #[serde(default)]
    pub typed_refs: ContinuousCoverageTypedRefsV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_theory: Option<RuntimeTheoryCheckSummaryV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failures: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_code_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_codegen_languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub present_codegen_languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<String>,
}

pub fn parse_overlay_bundle(json_text: &str) -> Result<ToolingOverlayBundleV1> {
    const MAX_OVERLAY_BYTES: usize = 8 * 1024 * 1024;
    const MAX_OVERLAY_ITEMS: usize = 100_000;
    let bundle: ToolingOverlayBundleV1 = axiograph_security::parse_json_bounded(
        json_text.as_bytes(),
        MAX_OVERLAY_BYTES,
        "ToolingOverlayBundleV1",
    )?;
    let mut item_count = bundle
        .implementation_surfaces
        .surfaces
        .len()
        .checked_add(bundle.implementation_surfaces.coverage_edges.len())
        .and_then(|count| count.checked_add(bundle.coverage_policy.require_codegen_languages.len()))
        .and_then(|count| count.checked_add(bundle.codegen_plan.languages.len()))
        .and_then(|count| count.checked_add(bundle.codegen_plan.notes.len()))
        .and_then(|count| count.checked_add(bundle.notes.len()))
        .ok_or_else(|| anyhow!("tooling overlay item count overflow"))?;
    if let Some(context) = &bundle.fddd_context_map {
        for count in [
            context.scopes.len(),
            context.bounded_contexts.len(),
            context.aggregates.len(),
            context.functions.len(),
            context.processes.len(),
            context.business_rules.len(),
            context.notes.len(),
        ] {
            item_count = item_count
                .checked_add(count)
                .ok_or_else(|| anyhow!("tooling overlay item count overflow"))?;
        }
    }
    if item_count > MAX_OVERLAY_ITEMS {
        return Err(anyhow!(
            "tooling overlay item count {item_count} exceeds {MAX_OVERLAY_ITEMS}"
        ));
    }
    if bundle.version != TOOLING_OVERLAY_BUNDLE_VERSION_V1 {
        return Err(anyhow!(
            "expected `{}`, got `{}`",
            TOOLING_OVERLAY_BUNDLE_VERSION_V1,
            bundle.version
        ));
    }
    Ok(bundle)
}

pub fn tooling_overlay_bundle_schema() -> Value {
    schema_value::<ToolingOverlayBundleV1>()
}

pub fn definition_query_schema() -> Value {
    schema_value::<DefinitionQueryV1>()
}

pub fn coverage_query_schema() -> Value {
    schema_value::<CoverageQueryV1>()
}

pub fn weak_coverage_probe_schema() -> Value {
    schema_value::<WeakCoverageProbeV1>()
}

pub fn overlay_software_coverage_report_schema() -> Value {
    schema_value::<OverlaySoftwareCoverageReportV1>()
}

#[allow(
    clippy::expect_used,
    reason = "schemars::Schema contains only JSON-representable values and has no fallible custom serializer"
)]
fn schema_value<T: JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T))
        // pi-lens-ignore: rust-expect
        .expect("schemars schema should serialize to JSON")
}

pub fn derive_runtime_index_from_axi_text(axi_text: &str) -> Result<RuntimeModuleIndex> {
    let module = axiograph_dsl::axi_v1::parse_axi_v1(axi_text)
        .map_err(|err| anyhow!("failed to parse canonical .axi: {err}"))?;
    axiograph_pathdb::derive_runtime_module_index(&module, axi_text)
        .map_err(|err| anyhow!("failed to derive RuntimeModuleIndex: {err}"))
}

pub fn validate_overlay_bundle(
    kernel: &RuntimeModuleIndex,
    bundle: &ToolingOverlayBundleV1,
) -> OverlayValidationReportV1 {
    let index = KernelRefIndex::new(kernel);
    let overlay_refs = all_overlay_refs(bundle);
    let mut normalized_refs = Vec::new();
    let mut diagnostics = Vec::new();
    for overlay_ref in &overlay_refs {
        let overlay_ref = *overlay_ref;
        match index.resolve(overlay_ref) {
            Some((normalized_id, label, kernel_ref)) => {
                normalized_refs.push(NormalizedOverlayRefV1 {
                    source: (*overlay_ref).clone(),
                    normalized_id,
                    kernel_ref_label: label.clone(),
                    label,
                    kernel_ref,
                })
            }
            None => diagnostics.push(OverlayDiagnosticV1 {
                severity: "error".to_string(),
                code: "unknown_overlay_ref".to_string(),
                message: format!(
                    "overlay ref could not be resolved: {}",
                    ref_label(overlay_ref)
                ),
                ref_id: overlay_ref
                    .stable_id
                    .clone()
                    .or_else(|| overlay_ref.name.clone()),
                suggestions: index.suggestions(overlay_ref),
            }),
        }
    }
    let ref_summary =
        overlay_ref_resolution_summary(&overlay_refs, normalized_refs.len(), diagnostics.len());
    let mut surface_ids = BTreeSet::new();
    for surface in &bundle.implementation_surfaces.surfaces {
        if !surface_ids.insert(surface.surface_id.clone()) {
            diagnostics.push(OverlayDiagnosticV1 {
                severity: "error".to_string(),
                code: "duplicate_surface_id".to_string(),
                message: format!("duplicate implementation surface `{}`", surface.surface_id),
                ref_id: Some(surface.surface_id.clone()),
                suggestions: Vec::new(),
            });
        }
    }
    for edge in &bundle.implementation_surfaces.coverage_edges {
        if !surface_ids.contains(&edge.surface_id) {
            diagnostics.push(OverlayDiagnosticV1 {
                severity: "error".to_string(),
                code: "coverage_edge_unknown_surface".to_string(),
                message: format!(
                    "coverage edge references unknown surface `{}`",
                    edge.surface_id
                ),
                ref_id: Some(edge.surface_id.clone()),
                suggestions: surface_ids.iter().take(5).cloned().collect(),
            });
        }
    }
    let valid = !diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == "error");
    let mut next_actions = Vec::new();
    if !valid {
        next_actions.push(
            "fix unresolved overlay refs before using this overlay for enforced coverage"
                .to_string(),
        );
        next_actions.push(
            "run a weak definition query to find replacement ontology refs, then update the overlay"
                .to_string(),
        );
    }
    if bundle.coverage_policy.coverage_mode != CoverageModeV1::Enforced {
        next_actions.push(
            "coverage policy is not enforced; use this overlay for advisory planning unless promoted"
                .to_string(),
        );
    } else if valid {
        next_actions.push(
            "overlay refs resolve under the compiled IR; run software-coverage or continuous-check as the enforcement gate"
                .to_string(),
        );
    }
    OverlayValidationReportV1 {
        version: OVERLAY_VALIDATION_REPORT_VERSION_V1.to_string(),
        coverage_mode: bundle.coverage_policy.coverage_mode,
        valid,
        module_digest: kernel.module_digest.to_string(),
        ref_summary,
        normalized_refs,
        diagnostics,
        next_actions,
    }
}

pub fn validate_definition_query_v1(query: &DefinitionQueryV1) -> Result<()> {
    if query
        .version
        .as_deref()
        .is_some_and(|version| version != DEFINITION_QUERY_VERSION_V1)
    {
        return Err(anyhow!("unsupported definition query version"));
    }
    if query.prompt.trim().is_empty() || query.prompt.len() > MAX_QUERY_STRING_BYTES_V1 {
        return Err(anyhow!("definition query prompt has invalid length"));
    }
    if query
        .context_hint
        .as_ref()
        .is_some_and(|hint| hint.len() > MAX_QUERY_STRING_BYTES_V1)
    {
        return Err(anyhow!("definition query context hint exceeds byte limit"));
    }
    if query.candidate_refs.len() > MAX_QUERY_ITEMS_V1 {
        return Err(anyhow!(
            "definition query candidate count exceeds hard limit"
        ));
    }
    if query
        .max_matches
        .is_some_and(|matches| matches == 0 || matches > MAX_QUERY_MATCHES_V1)
    {
        return Err(anyhow!(
            "definition query max_matches is outside 1..={MAX_QUERY_MATCHES_V1}"
        ));
    }
    let mut total = query.prompt.len() + query.context_hint.as_ref().map_or(0, String::len);
    for candidate in &query.candidate_refs {
        for value in [
            candidate.schema.as_ref(),
            candidate.name.as_ref(),
            candidate.stable_id.as_ref(),
            candidate.role.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            if value.len() > MAX_QUERY_STRING_BYTES_V1 {
                return Err(anyhow!(
                    "definition query candidate field exceeds byte limit"
                ));
            }
            total = total
                .checked_add(value.len())
                .ok_or_else(|| anyhow!("definition query size overflow"))?;
        }
    }
    if total > MAX_QUERY_TOTAL_BYTES_V1 {
        return Err(anyhow!("definition query exceeds aggregate byte limit"));
    }
    Ok(())
}

pub fn validate_coverage_query_v1(query: &CoverageQueryV1) -> Result<()> {
    if query
        .version
        .as_deref()
        .is_some_and(|version| version != COVERAGE_QUERY_VERSION_V1)
    {
        return Err(anyhow!("unsupported coverage query version"));
    }
    if query
        .max_matches
        .is_some_and(|matches| matches == 0 || matches > MAX_QUERY_MATCHES_V1)
    {
        return Err(anyhow!(
            "coverage query max_matches is outside 1..={MAX_QUERY_MATCHES_V1}"
        ));
    }
    let groups = [
        &query.terms,
        &query.relation_names,
        &query.cq_names,
        &query.code_refs,
        &query.surface_hints,
    ];
    let item_count = groups.iter().try_fold(0_usize, |total, group| {
        total
            .checked_add(group.len())
            .ok_or_else(|| anyhow!("coverage query item count overflow"))
    })?;
    if item_count > MAX_QUERY_ITEMS_V1 {
        return Err(anyhow!("coverage query item count exceeds hard limit"));
    }
    let mut total = query.axql.as_ref().map_or(0, String::len);
    if total > MAX_QUERY_STRING_BYTES_V1 {
        return Err(anyhow!("coverage query AxQL exceeds byte limit"));
    }
    for value in groups.into_iter().flat_map(|group| group.iter()) {
        if value.len() > MAX_QUERY_STRING_BYTES_V1 {
            return Err(anyhow!("coverage query field exceeds byte limit"));
        }
        total = total
            .checked_add(value.len())
            .ok_or_else(|| anyhow!("coverage query size overflow"))?;
    }
    if total > MAX_QUERY_TOTAL_BYTES_V1 {
        return Err(anyhow!("coverage query exceeds aggregate byte limit"));
    }
    Ok(())
}

pub fn definition_query_report(
    kernel: &RuntimeModuleIndex,
    bundle: Option<&ToolingOverlayBundleV1>,
    query: &DefinitionQueryV1,
) -> Result<DefinitionQueryReportV1> {
    validate_definition_query_v1(query)?;
    let classified_kind = query
        .kind_hint
        .unwrap_or_else(|| classify_prompt(&query.prompt));
    let max_matches = query.max_matches.unwrap_or(8).max(1);
    let terms = prompt_terms(&query.prompt);
    let index = KernelRefIndex::new(kernel);
    let mut candidates = index
        .definition_candidates(&terms, query.include_queries)
        .into_iter()
        .collect::<Vec<_>>();
    if let Some(bundle) = bundle {
        candidates.extend(overlay_definition_candidates(bundle, &terms));
    }
    if !query.candidate_refs.is_empty() {
        let allowed = query
            .candidate_refs
            .iter()
            .filter_map(|r| index.resolve(r).map(|(id, _, _)| id))
            .collect::<BTreeSet<_>>();
        if !allowed.is_empty() {
            candidates.retain(|candidate| allowed.contains(&candidate.ref_id));
        }
    }
    candidates.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.ref_id.cmp(&b.ref_id)));
    candidates.dedup_by(|a, b| a.ref_id == b.ref_id);
    candidates.truncate(max_matches);
    let likely_relation_matches = candidates
        .iter()
        .filter(|c| c.kind == OverlayRefKindV1::Relation)
        .map(|c| c.ref_id.clone())
        .collect::<Vec<_>>();
    let mut unresolved_ambiguities = Vec::new();
    if candidates.is_empty() {
        unresolved_ambiguities
            .push("no ontology refs matched the prompt strongly enough for grounding".to_string());
    } else if candidates.len() > 1 && candidates[0].score == candidates[1].score {
        unresolved_ambiguities.push(
            "multiple ontology refs tied for the top score; provide a kind_hint or candidate_refs"
                .to_string(),
        );
    }
    Ok(DefinitionQueryReportV1 {
        version: DEFINITION_QUERY_REPORT_VERSION_V1.to_string(),
        coverage_mode: CoverageModeV1::DefinitionQuery,
        classified_kind,
        prompt: query.prompt.clone(),
        candidates,
        likely_relation_matches,
        grounding_notes: vec![
            "definition_query mode is weak and advisory; it does not satisfy correctness or coverage gates".to_string(),
            "use selected candidates to author or validate a structured overlay before enforcement".to_string(),
        ],
        unresolved_ambiguities,
        recommended_followups: vec![
            "turn accepted candidate refs into a ToolingOverlayBundleV1 mapping".to_string(),
            "run overlay-check before relying on the mapping for coverage".to_string(),
        ],
    })
}

pub fn coverage_query_report(
    kernel: &RuntimeModuleIndex,
    bundle: Option<&ToolingOverlayBundleV1>,
    query: &CoverageQueryV1,
) -> Result<CoverageQueryReportV1> {
    validate_coverage_query_v1(query)?;
    let mode = if matches!(query.coverage_mode, CoverageModeV1::Enforced) {
        CoverageModeV1::Exploratory
    } else {
        query.coverage_mode
    };
    let mut terms = query.terms.clone();
    terms.extend(query.relation_names.clone());
    terms.extend(query.cq_names.clone());
    terms.extend(query.surface_hints.clone());
    terms.extend(query.code_refs.iter().map(|p| path_tokens(p).join(" ")));
    let def_query = DefinitionQueryV1 {
        version: Some(DEFINITION_QUERY_VERSION_V1.to_string()),
        prompt: terms.join(" "),
        kind_hint: None,
        context_hint: None,
        candidate_refs: Vec::new(),
        max_matches: query.max_matches,
        include_queries: true,
    };
    let def_report = definition_query_report(kernel, bundle, &def_query)?;
    let matched_surfaces = bundle
        .map(|bundle| {
            bundle
                .implementation_surfaces
                .surfaces
                .iter()
                .filter(|surface| {
                    query.surface_hints.iter().any(|hint| {
                        normalized(&surface.label).contains(&normalized(hint))
                            || normalized(&surface.surface_id).contains(&normalized(hint))
                    }) || query
                        .code_refs
                        .iter()
                        .any(|code_ref| surface.code_refs.iter().any(|p| p.contains(code_ref)))
                })
                .map(|surface| surface.surface_id.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let matched_terms = def_report
        .candidates
        .iter()
        .flat_map(|candidate| candidate.matched_terms.iter().cloned())
        .collect::<BTreeSet<_>>();
    let unmatched_terms = query
        .terms
        .iter()
        .filter(|term| !matched_terms.contains(&normalized(term)))
        .cloned()
        .collect::<Vec<_>>();
    Ok(CoverageQueryReportV1 {
        version: COVERAGE_QUERY_REPORT_VERSION_V1.to_string(),
        coverage_mode: mode,
        matched_refs: def_report.candidates,
        matched_surfaces,
        unmatched_terms,
        caveats: vec![
            "coverage-query mode is exploratory and cannot satisfy promotion gates".to_string(),
            "use overlay-check and software-coverage for enforced coverage".to_string(),
        ],
        next_actions: vec![
            "convert useful matches into structured overlay refs".to_string(),
            "attach explicit coverage policy before using results in CI".to_string(),
        ],
    })
}

pub fn codegen_plan_report(bundle: &ToolingOverlayBundleV1) -> CodegenPlanReportV1 {
    let test_name = bundle
        .codegen_plan
        .test_name
        .clone()
        .unwrap_or_else(|| "behavior_case".to_string());
    let required_languages =
        normalized_languages(&bundle.coverage_policy.require_codegen_languages)
            .into_iter()
            .collect::<BTreeSet<_>>();
    let planned_languages = normalized_languages(&bundle.codegen_plan.languages);
    let language_plans = planned_languages
        .into_iter()
        .map(|language| match language.as_str() {
            "go" => (
                language,
                format!("internal/behaviorcases/{test_name}_test.go"),
            ),
            "python" => (
                language,
                format!("tests/behavior_cases/test_{test_name}.py"),
            ),
            "rust" => (language, format!("tests/behavior_cases/{test_name}.rs")),
            "typescript" => (
                language,
                format!("tests/behavior-cases/{test_name}.spec.ts"),
            ),
            other => (
                language.clone(),
                format!("tests/behavior-cases/{test_name}.{other}.txt"),
            ),
        })
        .map(|(language, file_hint)| {
            let surfaces = surfaces_for_language(bundle, &language);
            CodegenLanguagePlanV1 {
                required_by_policy: required_languages.contains(&language),
                ontology_ref_labels: surface_ontology_ref_labels(&surfaces),
                surface_ids: surfaces
                    .iter()
                    .map(|surface| surface.surface_id.clone())
                    .collect(),
                language,
                file_hint,
            }
        })
        .collect::<Vec<_>>();
    let file_hints = language_plans
        .iter()
        .map(|plan| plan.file_hint.clone())
        .collect::<Vec<_>>();
    let planned_set = language_plans
        .iter()
        .map(|plan| plan.language.clone())
        .collect::<BTreeSet<_>>();
    let missing_required_languages = required_languages
        .iter()
        .filter(|language| !planned_set.contains(*language))
        .cloned()
        .collect::<Vec<_>>();
    let mut caveats = vec![
        "codegen plans are review artifacts; they do not mutate accepted ontology state"
            .to_string(),
    ];
    if !missing_required_languages.is_empty() {
        caveats.push(format!(
            "coverage policy requires languages not present in codegen_plan: {}",
            missing_required_languages.join(", ")
        ));
    }
    let mut next_actions = Vec::new();
    if !missing_required_languages.is_empty() {
        next_actions.push(format!(
            "add codegen_plan.languages entries for {} or relax coverage_policy.require_codegen_languages",
            missing_required_languages.join(", ")
        ));
    }
    next_actions.push(
        "generate a behavior-case report, review codegen_previews, then materialize skeletons through the CLI"
            .to_string(),
    );
    CodegenPlanReportV1 {
        version: CODEGEN_PLAN_REPORT_VERSION_V1.to_string(),
        coverage_mode: bundle.coverage_policy.coverage_mode,
        plan: bundle.codegen_plan.clone(),
        language_plans,
        file_hints,
        caveats,
        next_actions,
    }
}

pub fn behavior_case_coverage_view_from_value(
    behavior_report: &Value,
) -> Result<BehaviorCaseCoverageViewV1> {
    serde_json::from_value(behavior_report.clone()).map_err(|err| {
        anyhow!("expected BehaviorCaseCoverageViewV1 payload (`version=behavior_case_report_v1`): {err}")
    })
}

pub fn continuous_coverage_report_from_behavior_report(
    behavior_report: &BehaviorCaseCoverageViewV1,
    bundle: &ToolingOverlayBundleV1,
    repo_root: &Path,
) -> OverlaySoftwareCoverageReportV1 {
    continuous_coverage_report_impl(behavior_report, bundle, repo_root, None)
}

pub fn continuous_coverage_report_from_behavior_report_with_validation(
    behavior_report: &BehaviorCaseCoverageViewV1,
    bundle: &ToolingOverlayBundleV1,
    repo_root: &Path,
    overlay_validation: &OverlayValidationReportV1,
) -> OverlaySoftwareCoverageReportV1 {
    continuous_coverage_report_impl(behavior_report, bundle, repo_root, Some(overlay_validation))
}

fn continuous_coverage_report_impl(
    behavior_report: &BehaviorCaseCoverageViewV1,
    bundle: &ToolingOverlayBundleV1,
    repo_root: &Path,
    overlay_validation: Option<&OverlayValidationReportV1>,
) -> OverlaySoftwareCoverageReportV1 {
    let policy = &bundle.coverage_policy;
    let coverage_policy = coverage_policy_summary(policy);
    let required_codegen_languages = if policy.require_codegen_languages.is_empty() {
        normalized_languages(&bundle.codegen_plan.languages)
    } else {
        normalized_languages(&policy.require_codegen_languages)
    };
    let present_codegen_languages = collect_codegen_languages(behavior_report)
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let present_set = present_codegen_languages
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let missing_codegen = required_codegen_languages
        .iter()
        .filter(|language| !present_set.contains(*language))
        .cloned()
        .collect::<Vec<_>>();
    let code_refs = bundle
        .implementation_surfaces
        .surfaces
        .iter()
        .flat_map(|surface| surface.code_refs.iter())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let missing_code_refs = code_refs
        .iter()
        .filter(|code_ref| !repo_root.join(code_ref).exists())
        .cloned()
        .collect::<Vec<_>>();
    let runtime_theory = behavior_report
        .runtime_theory_check
        .clone()
        .or_else(|| behavior_report.context_report.runtime_theory_check.clone());
    let typed_refs = continuous_typed_refs(bundle, overlay_validation);

    let mut failures = Vec::new();
    let mut warnings = Vec::new();
    let unresolved_overlay_refs = typed_refs.ontology_refs.unresolved_refs;
    let enforced = matches!(policy.coverage_mode, CoverageModeV1::Enforced);
    if unresolved_overlay_refs > 0 {
        let message = format!(
            "{unresolved_overlay_refs} overlay ontology refs were not validated against compiled kernel IR"
        );
        if enforced || policy.strict_coverage {
            failures.push(message);
        } else {
            warnings.push(message);
        }
    }
    if overlay_validation.is_some_and(|validation| !validation.valid) {
        let message = "overlay validation contains blocking diagnostics".to_string();
        if enforced || policy.strict_coverage {
            failures.push(message);
        } else {
            warnings.push(message);
        }
    }
    if !missing_codegen.is_empty() {
        failures.push(format!(
            "missing required codegen languages: {}",
            missing_codegen.join(", ")
        ));
    }
    let fail_on_missing_code_refs = enforced || policy.require_code_refs || policy.strict_coverage;
    let fail_on_unresolved_obligations =
        enforced || policy.fail_on_unresolved_obligations || policy.strict_coverage;
    if fail_on_missing_code_refs && !missing_code_refs.is_empty() {
        failures.push(format!("{} code refs are missing", missing_code_refs.len()));
    } else if !missing_code_refs.is_empty() {
        warnings.push(format!(
            "{} code refs are target surfaces, not verified files",
            missing_code_refs.len()
        ));
    }
    let missing_obligations = behavior_report
        .context_report
        .coverage
        .missing_obligations
        .len();
    if fail_on_unresolved_obligations && missing_obligations > 0 {
        failures.push(format!(
            "{missing_obligations} semantic coverage obligations remain unresolved"
        ));
    } else if missing_obligations > 0 {
        warnings.push(format!(
            "{missing_obligations} semantic coverage obligations remain unresolved"
        ));
    }
    match runtime_theory.as_ref() {
        None if policy.require_runtime_theory => {
            failures.push("runtime theory summary is required by policy but missing".to_string());
        }
        None => warnings.push(
            "runtime theory sidecar is absent; coverage remains a runtime/tooling claim only"
                .to_string(),
        ),
        Some(summary) => failures.extend(summary.gate_blockers()),
    }
    let case_id = behavior_report
        .receipt
        .case_id
        .clone()
        .or_else(|| behavior_report.behavior_case.case_id.clone());
    let pass = failures.is_empty();
    let mut next_actions = Vec::new();
    if !missing_code_refs.is_empty() {
        next_actions
            .push("materialize generated skeletons or map code refs to real files".to_string());
    }
    if missing_obligations > 0 {
        next_actions.push("add structured coverage edges or relax the advisory policy".to_string());
    }
    if runtime_theory.is_none() {
        next_actions.push(
            "generate the behavior report from canonical `.axi` so it carries RuntimeTheoryCheckSummaryV1"
                .to_string(),
        );
    } else if runtime_theory
        .as_ref()
        .is_some_and(|summary| !summary.gate_blockers().is_empty())
    {
        next_actions.push(
            "resolve every review-only, residual, or blocked runtime-theory obligation".to_string(),
        );
    }
    let authoring_flow = build_authoring_flow_report_v1(
        AuthoringFlowSourceV1::OverlaySoftwareCoverage,
        case_id.clone(),
        authoring_coverage_profile_summary_v1(
            policy.coverage_mode,
            policy.strict_coverage,
            policy.require_code_refs,
            policy.require_runtime_theory,
            policy.fail_on_unresolved_obligations,
            required_codegen_languages.clone(),
        ),
        AuthoringCoverageSummaryV1 {
            total_rules: behavior_report.context_report.coverage.total_rules,
            covered_rules: behavior_report.context_report.coverage.covered_rules,
            tested_rules: behavior_report.context_report.coverage.tested_rules,
            implemented_rules: behavior_report.context_report.coverage.implemented_rules,
            drifted_rules: behavior_report.context_report.coverage.drifted_rules,
            missing_obligations: behavior_report
                .context_report
                .coverage
                .missing_obligations
                .clone(),
            uncovered_rule_ids: behavior_report
                .context_report
                .coverage
                .uncovered_rule_ids
                .clone(),
            code_refs_total: code_refs.len(),
            missing_code_refs: missing_code_refs.clone(),
            required_codegen_languages: required_codegen_languages.clone(),
            present_codegen_languages: present_codegen_languages.clone(),
            missing_codegen_languages: missing_codegen.clone(),
            runtime_theory_present: runtime_theory.is_some(),
            runtime_theory_residual_obligations: runtime_theory
                .as_ref()
                .map(|summary| summary.residual_obligations as u64),
            runtime_theory_blocking_obligations: runtime_theory.as_ref().map(|summary| {
                summary
                    .review_only_obligations
                    .saturating_add(summary.residual_obligations)
                    .saturating_add(summary.blocked_obligations)
                    .saturating_add(summary.blocking_errors) as u64
            }),
        },
        pass,
        failures.clone(),
        warnings.clone(),
        next_actions.clone(),
    );
    OverlaySoftwareCoverageReportV1 {
        version: OVERLAY_SOFTWARE_COVERAGE_REPORT_VERSION_V1.to_string(),
        coverage_mode: policy.coverage_mode,
        coverage_policy,
        authoring_flow,
        pass,
        case_id,
        typed_refs,
        runtime_theory,
        failures,
        warnings,
        missing_code_refs,
        required_codegen_languages,
        present_codegen_languages,
        next_actions,
    }
}

pub fn authoring_coverage_profile_summary_v1(
    coverage_mode: CoverageModeV1,
    strict_coverage: bool,
    require_code_refs: bool,
    require_runtime_theory: bool,
    fail_on_unresolved_obligations: bool,
    required_codegen_languages: Vec<String>,
) -> AuthoringCoverageProfileSummaryV1 {
    let fail_closed = strict_coverage
        || matches!(coverage_mode, CoverageModeV1::Enforced)
        || require_code_refs
        || require_runtime_theory
        || fail_on_unresolved_obligations;
    let ci_ready = matches!(coverage_mode, CoverageModeV1::Enforced)
        && strict_coverage
        && require_code_refs
        && require_runtime_theory
        && fail_on_unresolved_obligations;
    let profile = if ci_ready {
        AuthoringCoverageProfileV1::Ci
    } else if fail_closed {
        AuthoringCoverageProfileV1::Strict
    } else {
        AuthoringCoverageProfileV1::Advisory
    };
    AuthoringCoverageProfileSummaryV1 {
        profile,
        coverage_mode,
        strict_coverage,
        require_code_refs,
        require_runtime_theory,
        fail_on_unresolved_obligations,
        required_codegen_languages: normalized_languages(&required_codegen_languages),
        ci_ready,
        advisory_only: profile == AuthoringCoverageProfileV1::Advisory,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_authoring_flow_report_v1(
    source: AuthoringFlowSourceV1,
    case_id: Option<String>,
    profile: AuthoringCoverageProfileSummaryV1,
    coverage: AuthoringCoverageSummaryV1,
    pass: bool,
    failures: Vec<String>,
    warnings: Vec<String>,
    next_actions: Vec<String>,
) -> AuthoringFlowReportV1 {
    let status = if !pass || !failures.is_empty() {
        AuthoringFlowStatusV1::Failed
    } else if warnings.is_empty() {
        AuthoringFlowStatusV1::Passed
    } else {
        AuthoringFlowStatusV1::PassedWithWarnings
    };
    AuthoringFlowReportV1 {
        version: AUTHORING_FLOW_REPORT_VERSION_V1.to_string(),
        source,
        status,
        pass,
        case_id,
        profile,
        coverage,
        failures,
        warnings,
        next_actions,
    }
}

fn all_overlay_refs(bundle: &ToolingOverlayBundleV1) -> Vec<&OverlayRefV1> {
    let mut refs = Vec::new();
    if let Some(fddd) = &bundle.fddd_context_map {
        refs.extend(fddd.scopes.iter());
        for binding in fddd
            .bounded_contexts
            .iter()
            .chain(fddd.aggregates.iter())
            .chain(fddd.functions.iter())
            .chain(fddd.processes.iter())
            .chain(fddd.business_rules.iter())
        {
            refs.extend(binding.ontology_refs.iter());
        }
    }
    for surface in &bundle.implementation_surfaces.surfaces {
        refs.extend(surface.ontology_refs.iter());
    }
    refs
}

fn overlay_ref_resolution_summary(
    refs: &[&OverlayRefV1],
    resolved_refs: usize,
    unresolved_refs: usize,
) -> OverlayRefResolutionSummaryV1 {
    let mut refs_by_kind = BTreeMap::new();
    for reference in refs {
        *refs_by_kind
            .entry(ref_kind_name(reference.kind))
            .or_insert(0) += 1;
    }
    OverlayRefResolutionSummaryV1 {
        total_refs: refs.len(),
        resolved_refs,
        unresolved_refs,
        refs_by_kind,
    }
}

fn ref_kind_name(kind: OverlayRefKindV1) -> String {
    match kind {
        OverlayRefKindV1::Schema => "schema",
        OverlayRefKindV1::Object => "object",
        OverlayRefKindV1::Relation => "relation",
        OverlayRefKindV1::Theory => "theory",
        OverlayRefKindV1::Constraint => "constraint",
        OverlayRefKindV1::Equation => "equation",
        OverlayRefKindV1::Rewrite => "rewrite",
        OverlayRefKindV1::Instance => "instance",
        OverlayRefKindV1::Fact => "fact",
        OverlayRefKindV1::Unknown => "unknown",
    }
    .to_string()
}

fn surfaces_for_language<'a>(
    bundle: &'a ToolingOverlayBundleV1,
    language: &str,
) -> Vec<&'a ImplementationSurfaceOverlayV1> {
    bundle
        .implementation_surfaces
        .surfaces
        .iter()
        .filter(|surface| {
            surface
                .languages
                .iter()
                .any(|declared| declared.trim().eq_ignore_ascii_case(language))
        })
        .collect()
}

fn surface_ontology_ref_labels(surfaces: &[&ImplementationSurfaceOverlayV1]) -> Vec<String> {
    surfaces
        .iter()
        .flat_map(|surface| surface.ontology_refs.iter().map(ref_label))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn coverage_policy_summary(policy: &CoveragePolicyV1) -> CoveragePolicySummaryV1 {
    CoveragePolicySummaryV1 {
        coverage_mode: policy.coverage_mode,
        strict_coverage: policy.strict_coverage,
        require_code_refs: policy.require_code_refs,
        require_runtime_theory: policy.require_runtime_theory,
        fail_on_unresolved_obligations: policy.fail_on_unresolved_obligations,
    }
}

fn continuous_typed_refs(
    bundle: &ToolingOverlayBundleV1,
    overlay_validation: Option<&OverlayValidationReportV1>,
) -> ContinuousCoverageTypedRefsV1 {
    let refs = all_overlay_refs(bundle);
    let ontology_refs = overlay_validation
        .map(|validation| validation.ref_summary.clone())
        .unwrap_or_else(|| {
            // Without compiled kernel input, every ontology ref stays unresolved.
            overlay_ref_resolution_summary(&refs, 0, refs.len())
        });
    let surface_ids = bundle
        .implementation_surfaces
        .surfaces
        .iter()
        .map(|surface| surface.surface_id.clone())
        .collect::<Vec<_>>();
    let rule_ids = bundle
        .implementation_surfaces
        .coverage_edges
        .iter()
        .map(|edge| edge.rule_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    ContinuousCoverageTypedRefsV1 {
        ontology_refs,
        surface_ids,
        rule_ids,
    }
}

#[derive(Clone)]
struct KernelRefIndexEntry {
    kind: OverlayRefKindV1,
    label: String,
    kernel_ref: RuntimeIrRef,
}

struct KernelRefIndex {
    by_id: BTreeMap<String, KernelRefIndexEntry>,
    by_name: BTreeMap<(OverlayRefKindV1, Option<String>, String), String>,
    relations: BTreeMap<String, RelationForQuery>,
}

#[derive(Clone)]
struct RelationForQuery {
    schema: String,
    name: String,
    roles: Vec<String>,
}

impl KernelRefIndex {
    fn new(kernel: &RuntimeModuleIndex) -> Self {
        let mut by_id = BTreeMap::new();
        let mut by_name = BTreeMap::new();
        let mut relations = BTreeMap::new();
        let surface = kernel.runtime_semantic_index();
        let schema_names = surface
            .refs
            .iter()
            .filter_map(|surface_ref| match surface_ref {
                RuntimeIrRef::Canonical { citation } => match &citation.reference {
                    KernelRefV2::Schema { schema_id, .. } => {
                        Some((schema_id.to_string(), citation.label.clone()))
                    }
                    _ => None,
                },
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        let theory_schema_names = surface
            .refs
            .iter()
            .filter_map(|surface_ref| match surface_ref {
                RuntimeIrRef::Canonical { citation } => match &citation.reference {
                    KernelRefV2::Theory {
                        theory_id,
                        schema_id,
                        ..
                    } => Some((
                        theory_id.to_string(),
                        schema_names.get(&schema_id.to_string()).cloned(),
                    )),
                    _ => None,
                },
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        for surface_ref in &surface.refs {
            match surface_ref {
                RuntimeIrRef::Canonical { citation } => match &citation.reference {
                    KernelRefV2::Module { .. }
                    | KernelRefV2::Role { .. }
                    | KernelRefV2::Generator { .. } => {}
                    KernelRefV2::Schema { schema_id, .. } => insert_ref(
                        &mut by_id,
                        &mut by_name,
                        OverlayRefKindV1::Schema,
                        None,
                        schema_id.to_string(),
                        citation.label.clone(),
                        surface_ref.clone(),
                    ),
                    KernelRefV2::ObjectType {
                        schema_id,
                        object_type_id,
                        ..
                    } => insert_ref(
                        &mut by_id,
                        &mut by_name,
                        OverlayRefKindV1::Object,
                        schema_names.get(&schema_id.to_string()).cloned(),
                        object_type_id.to_string(),
                        citation.label.clone(),
                        surface_ref.clone(),
                    ),
                    KernelRefV2::Relation {
                        schema_id,
                        relation_id,
                        ..
                    } => {
                        let schema_label = schema_names.get(&schema_id.to_string()).cloned();
                        insert_ref(
                            &mut by_id,
                            &mut by_name,
                            OverlayRefKindV1::Relation,
                            schema_label.clone(),
                            relation_id.to_string(),
                            citation.label.clone(),
                            surface_ref.clone(),
                        );
                        if let Some((schema, relation)) = kernel.schemas.iter().find_map(|schema| {
                            (schema_label.as_deref() == Some(schema_name(schema).as_str()))
                                .then(|| {
                                    schema
                                        .relations
                                        .values()
                                        .find(|relation| relation.name == citation.label)
                                        .map(|relation| (schema, relation))
                                })
                                .flatten()
                        }) {
                            relations.insert(
                                relation_id.to_string(),
                                RelationForQuery {
                                    schema: schema_name(schema),
                                    name: relation.name.clone(),
                                    roles: relation_roles(relation),
                                },
                            );
                        }
                    }
                    KernelRefV2::Theory {
                        schema_id,
                        theory_id,
                        ..
                    } => insert_ref(
                        &mut by_id,
                        &mut by_name,
                        OverlayRefKindV1::Theory,
                        schema_names.get(&schema_id.to_string()).cloned(),
                        theory_id.to_string(),
                        citation.label.clone(),
                        surface_ref.clone(),
                    ),
                    KernelRefV2::Instance {
                        schema_id,
                        instance_id,
                        ..
                    } => insert_ref(
                        &mut by_id,
                        &mut by_name,
                        OverlayRefKindV1::Instance,
                        schema_names.get(&schema_id.to_string()).cloned(),
                        instance_id.to_string(),
                        citation.label.clone(),
                        surface_ref.clone(),
                    ),
                    KernelRefV2::Constraint {
                        theory_id,
                        constraint_id,
                        ..
                    } => insert_ref(
                        &mut by_id,
                        &mut by_name,
                        OverlayRefKindV1::Constraint,
                        theory_schema_names
                            .get(&theory_id.to_string())
                            .cloned()
                            .flatten(),
                        constraint_id.to_string(),
                        citation.label.clone(),
                        surface_ref.clone(),
                    ),
                    KernelRefV2::Equation {
                        theory_id,
                        equation_id,
                        ..
                    } => insert_ref(
                        &mut by_id,
                        &mut by_name,
                        OverlayRefKindV1::Equation,
                        theory_schema_names
                            .get(&theory_id.to_string())
                            .cloned()
                            .flatten(),
                        equation_id.to_string(),
                        citation.label.clone(),
                        surface_ref.clone(),
                    ),
                    KernelRefV2::RewriteRule {
                        theory_id,
                        rewrite_rule_id,
                        ..
                    } => insert_ref(
                        &mut by_id,
                        &mut by_name,
                        OverlayRefKindV1::Rewrite,
                        theory_schema_names
                            .get(&theory_id.to_string())
                            .cloned()
                            .flatten(),
                        rewrite_rule_id.to_string(),
                        citation.label.clone(),
                        surface_ref.clone(),
                    ),
                    KernelRefV2::Fact {
                        schema_id, fact_id, ..
                    } => insert_ref(
                        &mut by_id,
                        &mut by_name,
                        OverlayRefKindV1::Fact,
                        schema_names.get(&schema_id.to_string()).cloned(),
                        fact_id.to_string(),
                        citation.label.clone(),
                        surface_ref.clone(),
                    ),
                },
                RuntimeIrRef::Theory { .. }
                | RuntimeIrRef::TheoryObligation { .. }
                | RuntimeIrRef::TheorySubject { .. }
                | RuntimeIrRef::Instance { .. }
                | RuntimeIrRef::StableFact { .. } => {}
            }
        }
        Self {
            by_id,
            by_name,
            relations,
        }
    }

    fn resolve(&self, reference: &OverlayRefV1) -> Option<(String, String, RuntimeIrRef)> {
        if let Some(stable_id) = reference.stable_id.as_ref() {
            if let Some(entry) = self.by_id.get(stable_id) {
                return Some((
                    stable_id.clone(),
                    entry.label.clone(),
                    entry.kernel_ref.clone(),
                ));
            }
        }
        let name = reference.name.as_ref()?;
        let schema = reference.schema.clone();
        let key = (reference.kind, schema.clone(), normalized(name));
        if let Some(id) = self.by_name.get(&key) {
            return self
                .by_id
                .get(id)
                .map(|entry| (id.clone(), entry.label.clone(), entry.kernel_ref.clone()));
        }
        let key_without_schema = (reference.kind, None, normalized(name));
        if let Some(id) = self.by_name.get(&key_without_schema) {
            return self
                .by_id
                .get(id)
                .map(|entry| (id.clone(), entry.label.clone(), entry.kernel_ref.clone()));
        }
        None
    }

    fn suggestions(&self, reference: &OverlayRefV1) -> Vec<String> {
        let needle = reference
            .name
            .as_ref()
            .or(reference.stable_id.as_ref())
            .map(|s| normalized(s))
            .unwrap_or_default();
        self.by_id
            .iter()
            .filter(|(_, entry)| {
                entry.kind == reference.kind
                    && (needle.is_empty()
                        || normalized(&entry.label).contains(&needle)
                        || needle.contains(&normalized(&entry.label)))
            })
            .take(5)
            .map(|(id, entry)| format!("{} ({id})", entry.label))
            .collect()
    }

    fn definition_candidates(
        &self,
        terms: &[String],
        include_queries: bool,
    ) -> Vec<DefinitionCandidateV1> {
        let mut out = Vec::new();
        for (id, entry) in &self.by_id {
            let matched_terms = matched_terms(&entry.label, terms);
            let mut score = candidate_score(&entry.label, terms);
            if terms.iter().any(|term| normalized(id).contains(term)) {
                score += 10;
            }
            if score == 0 {
                continue;
            }
            let suggested_axql = if include_queries && entry.kind == OverlayRefKindV1::Relation {
                self.relations.get(id).map(suggested_axql_for_relation)
            } else {
                None
            };
            out.push(DefinitionCandidateV1 {
                ref_id: id.clone(),
                kind: entry.kind,
                label: entry.label.clone(),
                score,
                matched_terms,
                suggested_axql,
            });
        }
        out
    }
}

fn insert_ref(
    by_id: &mut BTreeMap<String, KernelRefIndexEntry>,
    by_name: &mut BTreeMap<(OverlayRefKindV1, Option<String>, String), String>,
    kind: OverlayRefKindV1,
    schema: Option<String>,
    id: String,
    label: String,
    kernel_ref: RuntimeIrRef,
) {
    by_id.insert(
        id.clone(),
        KernelRefIndexEntry {
            kind,
            label: label.clone(),
            kernel_ref,
        },
    );
    by_name.insert((kind, schema.clone(), normalized(&label)), id.clone());
    by_name.insert((kind, None, normalized(&label)), id);
}

fn schema_name(schema: &RuntimeSchemaIndex) -> String {
    schema
        .schema_id
        .as_str()
        .rsplit_once(':')
        .map(|(_, local)| local.to_string())
        .unwrap_or_else(|| schema.schema_id.to_string())
}

fn relation_roles(relation: &RelationSemanticsIr) -> Vec<String> {
    let mut roles = relation
        .roles
        .iter()
        .map(|role| (role.order, role.name.clone()))
        .collect::<Vec<_>>();
    roles.sort_by_key(|(order, _)| *order);
    roles.into_iter().map(|(_, name)| name).collect()
}

fn suggested_axql_for_relation(relation: &RelationForQuery) -> String {
    let roles = relation
        .roles
        .iter()
        .map(|role| format!("{role}=?{role}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "select ?f where ?f = {}.{}({}) limit 10",
        relation.schema, relation.name, roles
    )
}

fn classify_prompt(prompt: &str) -> DefinitionQueryKindV1 {
    let normalized = normalized(prompt);
    if normalized.contains("businessrule") || normalized.contains("rule") {
        DefinitionQueryKindV1::BusinessRule
    } else if normalized.contains("process") || normalized.contains("workflow") {
        DefinitionQueryKindV1::Process
    } else if normalized.contains("function") || normalized.contains("endpoint") {
        DefinitionQueryKindV1::Function
    } else if normalized.contains("invariant") {
        DefinitionQueryKindV1::Invariant
    } else if normalized.contains("policy") {
        DefinitionQueryKindV1::Policy
    } else if normalized.contains("relation") {
        DefinitionQueryKindV1::Relation
    } else if normalized.contains("surface") || normalized.contains("code") {
        DefinitionQueryKindV1::ImplementationSurface
    } else {
        DefinitionQueryKindV1::Unknown
    }
}

fn overlay_definition_candidates(
    bundle: &ToolingOverlayBundleV1,
    terms: &[String],
) -> Vec<DefinitionCandidateV1> {
    let mut out = Vec::new();
    if let Some(fddd) = &bundle.fddd_context_map {
        for binding in fddd
            .bounded_contexts
            .iter()
            .chain(fddd.aggregates.iter())
            .chain(fddd.functions.iter())
            .chain(fddd.processes.iter())
            .chain(fddd.business_rules.iter())
        {
            let score = candidate_score(&binding.label, terms);
            if score > 0 {
                out.push(DefinitionCandidateV1 {
                    ref_id: binding.binding_id.clone(),
                    kind: OverlayRefKindV1::Unknown,
                    label: binding.label.clone(),
                    score,
                    matched_terms: matched_terms(&binding.label, terms),
                    suggested_axql: None,
                });
            }
        }
    }
    for surface in &bundle.implementation_surfaces.surfaces {
        let score = candidate_score(&surface.label, terms);
        if score > 0 {
            out.push(DefinitionCandidateV1 {
                ref_id: surface.surface_id.clone(),
                kind: OverlayRefKindV1::Unknown,
                label: surface.label.clone(),
                score,
                matched_terms: matched_terms(&surface.label, terms),
                suggested_axql: None,
            });
        }
    }
    out
}

fn prompt_terms(prompt: &str) -> Vec<String> {
    normalized(prompt)
        .split_whitespace()
        .map(str::to_string)
        .chain(
            prompt
                .split(|c: char| !c.is_alphanumeric())
                .flat_map(split_camel)
                .map(|s| normalized(&s)),
        )
        .filter(|term| term.len() > 2 && !STOP_WORDS.contains(&term.as_str()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

const STOP_WORDS: &[&str] = &[
    "the", "this", "that", "define", "what", "which", "must", "with", "for", "and", "from", "into",
    "rule", "business", "process", "function",
];

fn candidate_score(label: &str, terms: &[String]) -> u32 {
    let label_norm = normalized(label);
    terms
        .iter()
        .map(|term| {
            if label_norm == *term {
                25
            } else if label_norm.contains(term) {
                10
            } else {
                0
            }
        })
        .sum()
}

fn matched_terms(label: &str, terms: &[String]) -> Vec<String> {
    let label_norm = normalized(label);
    terms
        .iter()
        .filter(|term| label_norm.contains(*term))
        .cloned()
        .collect()
}

fn ref_label(reference: &OverlayRefV1) -> String {
    reference
        .stable_id
        .as_deref()
        .or(reference.name.as_deref())
        .unwrap_or("<unnamed>")
        .to_string()
}

pub fn normalized(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("")
}

fn split_camel(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in value.chars() {
        if ch.is_uppercase() && !current.is_empty() {
            out.push(current.clone());
            current.clear();
        }
        current.push(ch);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn path_tokens(path: &str) -> Vec<String> {
    PathBuf::from(path)
        .components()
        .filter_map(|component| component.as_os_str().to_str().map(str::to_string))
        .flat_map(|part| {
            part.split(|c: char| !c.is_alphanumeric())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn normalized_languages(languages: &[String]) -> Vec<String> {
    languages
        .iter()
        .map(|language| language.trim().to_ascii_lowercase())
        .filter(|language| !language.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn collect_codegen_languages(report: &BehaviorCaseCoverageViewV1) -> Vec<String> {
    report
        .codegen_previews
        .iter()
        .map(|preview| preview.language.as_str())
        .map(|language| language.trim().to_ascii_lowercase())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_kernel() -> RuntimeModuleIndex {
        let axi = r#"
module OrderFulfillmentDomain

schema OrderFulfillment:
  object Order
  object Payment
  object Shipment
  relation OrderHasPayment(order: Order, payment: Payment)
  relation ShipmentFulfillsOrder(shipment: Shipment, order: Order)

theory OrderFulfillmentRules on OrderFulfillment:
  constraint key OrderHasPayment(order, payment)

instance Seed of OrderFulfillment:
  Order = {Order_1}
  Payment = {Payment_1}
  Shipment = {Shipment_1}
  OrderHasPayment = {(order=Order_1, payment=Payment_1)}
  ShipmentFulfillsOrder = {(shipment=Shipment_1, order=Order_1)}
"#;
        derive_runtime_index_from_axi_text(axi).expect("compile kernel")
    }

    fn sample_behavior_coverage_view(
        missing_obligations: Vec<String>,
        runtime_theory_check: Option<RuntimeTheoryCheckSummaryV1>,
    ) -> BehaviorCaseCoverageViewV1 {
        BehaviorCaseCoverageViewV1 {
            behavior_case: BehaviorCaseReceiptCoverageViewV1 {
                case_id: Some("checkout.reserve".to_string()),
            },
            context_report: BehaviorCaseContextCoverageViewV1 {
                coverage: AuthoringCoverageSummaryV1 {
                    missing_obligations,
                    ..AuthoringCoverageSummaryV1::default()
                },
                runtime_theory_check: None,
            },
            codegen_previews: vec![BehaviorCaseCodegenPreviewCoverageViewV1 {
                language: "typescript".to_string(),
            }],
            runtime_theory_check,
            ..BehaviorCaseCoverageViewV1::default()
        }
    }

    #[test]
    fn overlay_validation_resolves_structured_refs() {
        let kernel = sample_kernel();
        let bundle = ToolingOverlayBundleV1 {
            version: TOOLING_OVERLAY_BUNDLE_VERSION_V1.to_string(),
            fddd_context_map: Some(FdddContextMapV1 {
                context_id: "bounded-context:ordering".to_string(),
                label: "Ordering".to_string(),
                summary: None,
                scopes: vec![OverlayRefV1 {
                    kind: OverlayRefKindV1::Relation,
                    schema: Some("OrderFulfillment".to_string()),
                    name: Some("OrderHasPayment".to_string()),
                    stable_id: None,
                    role: None,
                }],
                bounded_contexts: Vec::new(),
                aggregates: Vec::new(),
                functions: Vec::new(),
                processes: Vec::new(),
                business_rules: Vec::new(),
                notes: Vec::new(),
            }),
            implementation_surfaces: ImplementationSurfaceManifestV1::default(),
            coverage_policy: CoveragePolicyV1::default(),
            codegen_plan: CodegenPlanV1::default(),
            notes: Vec::new(),
        };

        let report = validate_overlay_bundle(&kernel, &bundle);
        assert!(report.valid, "{report:?}");
        assert_eq!(report.ref_summary.total_refs, 1);
        assert_eq!(report.ref_summary.resolved_refs, 1);
        assert!(report
            .normalized_refs
            .iter()
            .any(|reference| reference.kernel_ref_label == "OrderHasPayment"));
        assert!(report.normalized_refs.iter().any(|reference| {
            reference.kernel_ref_label == "OrderHasPayment"
                && matches!(
                    &reference.kernel_ref,
                    RuntimeIrRef::Canonical { citation }
                        if matches!(&citation.reference, KernelRefV2::Relation { .. })
                )
        }));

        let behavior = sample_behavior_coverage_view(Vec::new(), None);
        let unvalidated =
            continuous_coverage_report_from_behavior_report(&behavior, &bundle, Path::new("."));
        assert_eq!(unvalidated.typed_refs.ontology_refs.resolved_refs, 0);
        assert_eq!(unvalidated.typed_refs.ontology_refs.unresolved_refs, 1);

        let validated = continuous_coverage_report_from_behavior_report_with_validation(
            &behavior,
            &bundle,
            Path::new("."),
            &report,
        );
        assert_eq!(validated.typed_refs.ontology_refs.resolved_refs, 1);
        assert_eq!(validated.typed_refs.ontology_refs.unresolved_refs, 0);
    }

    #[test]
    fn public_tooling_schemas_are_generated_from_typed_dtos() {
        let overlay_schema = tooling_overlay_bundle_schema();
        let definition_schema = definition_query_schema();
        let coverage_schema = coverage_query_schema();
        let report_schema = overlay_software_coverage_report_schema();

        let overlay_text = overlay_schema.to_string();
        assert!(overlay_text.contains("implementation_surfaces"));
        assert!(definition_schema.to_string().contains("prompt"));
        assert!(coverage_schema.to_string().contains("coverage_mode"));
        assert!(report_schema.to_string().contains("missing_code_refs"));
    }

    #[test]
    fn codegen_plan_reports_language_surface_mapping_and_policy_gaps() {
        let bundle = ToolingOverlayBundleV1 {
            version: TOOLING_OVERLAY_BUNDLE_VERSION_V1.to_string(),
            fddd_context_map: None,
            implementation_surfaces: ImplementationSurfaceManifestV1 {
                surfaces: vec![ImplementationSurfaceOverlayV1 {
                    surface_id: "endpoint:checkout".to_string(),
                    kind: ImplementationSurfaceKindV1::Endpoint,
                    label: "Checkout".to_string(),
                    ontology_refs: vec![OverlayRefV1 {
                        kind: OverlayRefKindV1::Relation,
                        schema: Some("OrderFulfillment".to_string()),
                        name: Some("OrderHasPayment".to_string()),
                        stable_id: None,
                        role: None,
                    }],
                    code_refs: Vec::new(),
                    test_refs: Vec::new(),
                    languages: vec!["typescript".to_string()],
                    notes: Vec::new(),
                }],
                coverage_edges: Vec::new(),
            },
            coverage_policy: CoveragePolicyV1 {
                require_codegen_languages: vec!["typescript".to_string(), "go".to_string()],
                ..CoveragePolicyV1::default()
            },
            codegen_plan: CodegenPlanV1 {
                languages: vec!["typescript".to_string()],
                test_name: Some("checkout".to_string()),
                notes: Vec::new(),
            },
            notes: Vec::new(),
        };

        let report = codegen_plan_report(&bundle);
        assert_eq!(report.language_plans.len(), 1);
        assert_eq!(report.language_plans[0].language, "typescript");
        assert_eq!(
            report.language_plans[0].surface_ids,
            vec!["endpoint:checkout".to_string()]
        );
        assert!(report.caveats.iter().any(|caveat| caveat.contains("go")));
        assert!(report
            .next_actions
            .iter()
            .any(|action| action.contains("codegen_plan.languages")));
    }

    #[test]
    fn definition_query_classifies_business_rule_and_returns_candidates() {
        let kernel = sample_kernel();
        let query = DefinitionQueryV1 {
            version: Some(DEFINITION_QUERY_VERSION_V1.to_string()),
            prompt: "define the shipment fulfills order business rule".to_string(),
            kind_hint: None,
            context_hint: None,
            candidate_refs: Vec::new(),
            max_matches: Some(4),
            include_queries: true,
        };

        let report = definition_query_report(&kernel, None, &query).unwrap();
        assert_eq!(report.coverage_mode, CoverageModeV1::DefinitionQuery);
        assert_eq!(report.classified_kind, DefinitionQueryKindV1::BusinessRule);
        assert!(report
            .candidates
            .iter()
            .any(|candidate| candidate.label == "ShipmentFulfillsOrder"));
        assert!(report
            .candidates
            .iter()
            .any(|candidate| candidate.suggested_axql.is_some()));
    }

    #[test]
    fn coverage_query_is_advisory_not_enforced() {
        let kernel = sample_kernel();
        let query = CoverageQueryV1 {
            version: Some(COVERAGE_QUERY_VERSION_V1.to_string()),
            coverage_mode: CoverageModeV1::Enforced,
            terms: vec!["payment".to_string()],
            relation_names: Vec::new(),
            cq_names: Vec::new(),
            code_refs: Vec::new(),
            surface_hints: Vec::new(),
            axql: None,
            max_matches: Some(3),
        };

        let report = coverage_query_report(&kernel, None, &query).unwrap();
        assert_eq!(report.coverage_mode, CoverageModeV1::Exploratory);
        assert!(!report.matched_refs.is_empty());
        assert!(report
            .caveats
            .iter()
            .any(|caveat| caveat.contains("cannot satisfy promotion gates")));
    }

    #[test]
    fn query_reports_reject_oversized_structures_and_invalid_limits() {
        let kernel = sample_kernel();
        let definition = DefinitionQueryV1 {
            version: Some(DEFINITION_QUERY_VERSION_V1.to_string()),
            prompt: "x".repeat(MAX_QUERY_STRING_BYTES_V1 + 1),
            kind_hint: None,
            context_hint: None,
            candidate_refs: Vec::new(),
            max_matches: Some(1),
            include_queries: false,
        };
        assert!(definition_query_report(&kernel, None, &definition).is_err());

        let coverage = CoverageQueryV1 {
            version: Some(COVERAGE_QUERY_VERSION_V1.to_string()),
            coverage_mode: CoverageModeV1::Exploratory,
            terms: vec!["x".to_string(); MAX_QUERY_ITEMS_V1 + 1],
            relation_names: Vec::new(),
            cq_names: Vec::new(),
            code_refs: Vec::new(),
            surface_hints: Vec::new(),
            axql: None,
            max_matches: Some(0),
        };
        assert!(coverage_query_report(&kernel, None, &coverage).is_err());
    }

    #[test]
    fn strict_coverage_policy_fails_closed_on_missing_code_refs_and_obligations() {
        let bundle = ToolingOverlayBundleV1 {
            version: TOOLING_OVERLAY_BUNDLE_VERSION_V1.to_string(),
            fddd_context_map: None,
            implementation_surfaces: ImplementationSurfaceManifestV1 {
                surfaces: vec![ImplementationSurfaceOverlayV1 {
                    surface_id: "endpoint:checkout".to_string(),
                    kind: ImplementationSurfaceKindV1::Endpoint,
                    label: "Checkout".to_string(),
                    ontology_refs: Vec::new(),
                    code_refs: vec!["missing/checkout.ts".to_string()],
                    test_refs: Vec::new(),
                    languages: vec!["typescript".to_string()],
                    notes: Vec::new(),
                }],
                coverage_edges: Vec::new(),
            },
            coverage_policy: CoveragePolicyV1 {
                coverage_mode: CoverageModeV1::Enforced,
                strict_coverage: true,
                ..CoveragePolicyV1::default()
            },
            codegen_plan: CodegenPlanV1 {
                languages: vec!["typescript".to_string()],
                test_name: None,
                notes: Vec::new(),
            },
            notes: Vec::new(),
        };
        let behavior_report = sample_behavior_coverage_view(
            vec!["schema/order/relation/payment/rule/key/0".to_string()],
            None,
        );
        let report = continuous_coverage_report_from_behavior_report(
            &behavior_report,
            &bundle,
            Path::new("."),
        );
        assert!(!report.pass);
        assert_eq!(report.coverage_mode, CoverageModeV1::Enforced);
        assert_eq!(
            report.authoring_flow.version,
            AUTHORING_FLOW_REPORT_VERSION_V1
        );
        assert_eq!(
            report.authoring_flow.profile.profile,
            AuthoringCoverageProfileV1::Strict
        );
        assert_eq!(
            report.authoring_flow.coverage.missing_code_refs,
            vec!["missing/checkout.ts".to_string()]
        );
        assert!(report
            .failures
            .iter()
            .any(|failure| failure.contains("code refs are missing")));
        assert!(report
            .failures
            .iter()
            .any(|failure| failure.contains("semantic coverage obligations")));
    }

    #[test]
    fn every_non_checked_runtime_theory_status_blocks_coverage() {
        let mut bundle = ToolingOverlayBundleV1 {
            version: TOOLING_OVERLAY_BUNDLE_VERSION_V1.to_string(),
            fddd_context_map: None,
            implementation_surfaces: ImplementationSurfaceManifestV1 {
                surfaces: vec![ImplementationSurfaceOverlayV1 {
                    surface_id: "endpoint:checkout".to_string(),
                    kind: ImplementationSurfaceKindV1::Endpoint,
                    label: "Checkout".to_string(),
                    ontology_refs: Vec::new(),
                    code_refs: vec!["missing/checkout.ts".to_string()],
                    test_refs: Vec::new(),
                    languages: vec!["typescript".to_string()],
                    notes: Vec::new(),
                }],
                coverage_edges: Vec::new(),
            },
            coverage_policy: CoveragePolicyV1 {
                coverage_mode: CoverageModeV1::Advisory,
                require_codegen_languages: vec!["typescript".to_string()],
                strict_coverage: false,
                require_code_refs: false,
                require_runtime_theory: false,
                fail_on_unresolved_obligations: false,
            },
            codegen_plan: CodegenPlanV1 {
                languages: vec!["typescript".to_string()],
                test_name: None,
                notes: Vec::new(),
            },
            notes: Vec::new(),
        };
        let behavior_report = sample_behavior_coverage_view(
            vec!["schema/order/relation/payment/rule/key/0".to_string()],
            Some(RuntimeTheoryCheckSummaryV1 {
                version: "runtime_theory_check_summary_v1".to_string(),
                report_version: "runtime_theory_check_report_v1".to_string(),
                module_digest: "axi:revision:v2:sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                theory_count: 1,
                checked_obligations: 0,
                review_only_obligations: 1,
                residual_obligations: 1,
                blocked_obligations: 0,
                excluded_by_evidence: 0,
                blocking_errors: 0,
                scope: RuntimeTheoryScopeSummaryV1 {
                    fragments: vec!["finite_fragment".to_string()],
                    ..RuntimeTheoryScopeSummaryV1::default()
                },
                admissibility_trace: RuntimeTheoryAdmissibilityTraceSummaryV1::default(),
                transport_summary: RuntimeTheoryTransportSummaryV1::default(),
                residual_obligation_ids: vec!["runtime/theory/residual".to_string()],
                non_claims: vec![RuntimeTheoryNonClaimSummaryV1 {
                    code: "closure_engine_not_implemented".to_string(),
                    message: "admissibility scan only".to_string(),
                }],
                notes: Vec::new(),
            }),
        );
        let advisory = continuous_coverage_report_from_behavior_report(
            &behavior_report,
            &bundle,
            Path::new("."),
        );
        assert!(!advisory.pass, "{advisory:?}");
        assert_eq!(
            advisory.authoring_flow.profile.profile,
            AuthoringCoverageProfileV1::Advisory
        );
        let runtime = advisory.runtime_theory.as_ref().expect("runtime summary");
        assert_eq!(runtime.scope.fragments, vec!["finite_fragment"]);
        assert!(runtime
            .non_claims
            .iter()
            .any(|claim| claim.code == "closure_engine_not_implemented"));
        assert!(advisory
            .failures
            .iter()
            .any(|failure| failure.contains("review-only")));
        assert!(advisory
            .failures
            .iter()
            .any(|failure| failure.contains("residual ids")));

        bundle.coverage_policy.coverage_mode = CoverageModeV1::Enforced;
        bundle.coverage_policy.fail_on_unresolved_obligations = true;
        let enforced = continuous_coverage_report_from_behavior_report(
            &behavior_report,
            &bundle,
            Path::new("."),
        );
        assert!(!enforced.pass);
        assert_eq!(
            enforced.authoring_flow.profile.profile,
            AuthoringCoverageProfileV1::Strict
        );
        assert!(enforced
            .failures
            .iter()
            .any(|failure| failure.contains("runtime theory obligation")));
    }

    #[test]
    fn profile_summary_classifies_ci_only_when_fail_closed_switches_are_enabled() {
        let advisory = authoring_coverage_profile_summary_v1(
            CoverageModeV1::Advisory,
            false,
            false,
            false,
            false,
            vec!["Rust".to_string()],
        );
        assert_eq!(advisory.profile, AuthoringCoverageProfileV1::Advisory);
        assert_eq!(
            advisory.required_codegen_languages,
            vec!["rust".to_string()]
        );

        let strict = authoring_coverage_profile_summary_v1(
            CoverageModeV1::Enforced,
            false,
            false,
            false,
            false,
            Vec::new(),
        );
        assert_eq!(strict.profile, AuthoringCoverageProfileV1::Strict);

        let ci = authoring_coverage_profile_summary_v1(
            CoverageModeV1::Enforced,
            true,
            true,
            true,
            true,
            vec!["go".to_string()],
        );
        assert_eq!(ci.profile, AuthoringCoverageProfileV1::Ci);
        assert!(ci.ci_ready);
    }
}
