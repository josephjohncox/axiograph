use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use axiograph_pathdb::kernel_ir::{
    CompiledSchemaIr, KernelModuleIr, KernelRefV1, RelationSemanticsIr, SchemaCategoryObjectRefIr,
    TheoryObligationRefIr,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const TOOLING_OVERLAY_BUNDLE_VERSION_V1: &str = "tooling_overlay_bundle_v1";
pub const OVERLAY_VALIDATION_REPORT_VERSION_V1: &str = "overlay_validation_report_v1";
pub const CONTINUOUS_SOFTWARE_COVERAGE_REPORT_VERSION_V1: &str =
    "continuous_software_coverage_report_v1";
pub const COVERAGE_QUERY_REPORT_VERSION_V1: &str = "coverage_query_report_v1";
pub const DEFINITION_QUERY_REPORT_VERSION_V1: &str = "definition_query_report_v1";
pub const CODEGEN_PLAN_REPORT_VERSION_V1: &str = "codegen_plan_report_v1";
pub const AUTHORING_FLOW_REPORT_VERSION_V1: &str = "authoring_flow_report_v1";

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(rename_all = "snake_case")]
pub enum CoverageModeV1 {
    Enforced,
    Advisory,
    Exploratory,
    DefinitionQuery,
    InsufficientlyGrounded,
}

impl Default for CoverageModeV1 {
    fn default() -> Self {
        Self::Advisory
    }
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
    pub kernel_ref: Value,
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub struct RuntimeTheorySidecarSummaryV1 {
    #[serde(default)]
    pub present: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub closure_tiers: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_obligations: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_only_obligations: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub residual_obligations: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_obligations: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocking_errors: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completeness_claim: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ontology_closure_claim: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_obligation_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
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
pub struct ContinuousSoftwareCoverageReportV1 {
    pub version: String,
    pub coverage_mode: CoverageModeV1,
    pub coverage_policy: CoveragePolicySummaryV1,
    pub authoring_flow: AuthoringFlowReportV1,
    pub pass: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case_id: Option<String>,
    #[serde(default)]
    pub typed_refs: ContinuousCoverageTypedRefsV1,
    #[serde(default)]
    pub runtime_theory: RuntimeTheorySidecarSummaryV1,
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
    let bundle: ToolingOverlayBundleV1 = serde_json::from_str(json_text)
        .map_err(|err| anyhow!("failed to parse ToolingOverlayBundleV1 JSON: {err}"))?;
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

pub fn continuous_software_coverage_report_schema() -> Value {
    schema_value::<ContinuousSoftwareCoverageReportV1>()
}

fn schema_value<T: JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T))
        .expect("schemars schema should serialize to JSON")
}

pub fn compile_kernel_from_axi_text(axi_text: &str) -> Result<KernelModuleIr> {
    let module = axiograph_dsl::axi_v1::parse_axi_v1(axi_text)
        .map_err(|err| anyhow!("failed to parse canonical .axi: {err}"))?;
    axiograph_pathdb::compile_kernel_module_ir(&module, axi_text)
        .map_err(|err| anyhow!("failed to compile KernelModuleIr: {err}"))
}

pub fn validate_overlay_bundle(
    kernel: &KernelModuleIr,
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
                    label,
                    kernel_ref_label: kernel_ref.stable_label(),
                    kernel_ref: serde_json::to_value(&kernel_ref)
                        .unwrap_or_else(|_| Value::String(kernel_ref.stable_label())),
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

pub fn definition_query_report(
    kernel: &KernelModuleIr,
    bundle: Option<&ToolingOverlayBundleV1>,
    query: &DefinitionQueryV1,
) -> DefinitionQueryReportV1 {
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
    DefinitionQueryReportV1 {
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
    }
}

pub fn coverage_query_report(
    kernel: &KernelModuleIr,
    bundle: Option<&ToolingOverlayBundleV1>,
    query: &CoverageQueryV1,
) -> CoverageQueryReportV1 {
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
        prompt: terms.join(" "),
        kind_hint: None,
        context_hint: None,
        candidate_refs: Vec::new(),
        max_matches: query.max_matches,
        include_queries: true,
    };
    let def_report = definition_query_report(kernel, bundle, &def_query);
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
    CoverageQueryReportV1 {
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
    }
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

pub fn continuous_coverage_report_from_behavior_report(
    behavior_report: &Value,
    bundle: &ToolingOverlayBundleV1,
    repo_root: &Path,
) -> ContinuousSoftwareCoverageReportV1 {
    let policy = &bundle.coverage_policy;
    let coverage_policy = coverage_policy_summary(policy);
    let required_codegen_languages = if policy.require_codegen_languages.is_empty() {
        normalized_languages(&bundle.codegen_plan.languages)
    } else {
        normalized_languages(&policy.require_codegen_languages)
    };
    let present_codegen_languages = collect_codegen_languages(behavior_report)
        .into_iter()
        .chain(normalized_languages(&bundle.codegen_plan.languages))
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
    let runtime_theory = runtime_theory_sidecar_summary(behavior_report);
    let typed_refs = continuous_typed_refs(bundle);

    let mut failures = Vec::new();
    let mut warnings = Vec::new();
    if !missing_codegen.is_empty() {
        failures.push(format!(
            "missing required codegen languages: {}",
            missing_codegen.join(", ")
        ));
    }
    let fail_on_missing_code_refs = policy.require_code_refs || policy.strict_coverage;
    let fail_on_unresolved_obligations =
        policy.fail_on_unresolved_obligations || policy.strict_coverage;
    if fail_on_missing_code_refs && !missing_code_refs.is_empty() {
        failures.push(format!("{} code refs are missing", missing_code_refs.len()));
    } else if !missing_code_refs.is_empty() {
        warnings.push(format!(
            "{} code refs are target surfaces, not verified files",
            missing_code_refs.len()
        ));
    }
    let missing_obligations = behavior_report
        .pointer("/context_report/coverage/missing_obligations")
        .and_then(Value::as_array)
        .map(|values| values.len())
        .unwrap_or(0);
    if fail_on_unresolved_obligations && missing_obligations > 0 {
        failures.push(format!(
            "{missing_obligations} semantic coverage obligations remain unresolved"
        ));
    } else if missing_obligations > 0 {
        warnings.push(format!(
            "{missing_obligations} semantic coverage obligations remain unresolved"
        ));
    }
    if policy.require_runtime_theory && !runtime_theory.present {
        failures.push("runtime theory summary is required by policy but missing".to_string());
    } else if !runtime_theory.present {
        warnings.push(
            "runtime theory sidecar is absent; coverage remains a runtime/tooling claim only"
                .to_string(),
        );
    }
    let blocking_runtime_theory = runtime_theory.blocking_errors.unwrap_or(0)
        + runtime_theory.blocked_obligations.unwrap_or(0);
    if blocking_runtime_theory > 0 {
        failures.push(format!(
            "{blocking_runtime_theory} runtime theory sidecar obligation(s) are blocking"
        ));
    }
    if fail_on_unresolved_obligations && runtime_theory.residual_obligations.unwrap_or(0) > 0 {
        failures.push(format!(
            "{} runtime theory sidecar obligation(s) remain residual",
            runtime_theory.residual_obligations.unwrap_or(0)
        ));
    } else if runtime_theory.residual_obligations.unwrap_or(0) > 0 {
        warnings.push(format!(
            "{} runtime theory sidecar obligation(s) remain residual",
            runtime_theory.residual_obligations.unwrap_or(0)
        ));
    }
    if runtime_theory
        .completeness_claim
        .as_deref()
        .is_some_and(|claim| !claim.starts_with("claimed_under_"))
    {
        warnings.push(
            "runtime theory sidecar does not claim completeness for all obligations".to_string(),
        );
    }
    if runtime_theory
        .ontology_closure_claim
        .as_deref()
        .is_some_and(|claim| !claim.starts_with("claimed_under_"))
    {
        warnings.push(
            "runtime theory sidecar does not claim ontology closure for all obligations"
                .to_string(),
        );
    }
    let case_id = behavior_report
        .pointer("/behavior_case/case_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let pass = failures.is_empty();
    let mut next_actions = Vec::new();
    if !missing_code_refs.is_empty() {
        next_actions
            .push("materialize generated skeletons or map code refs to real files".to_string());
    }
    if missing_obligations > 0 {
        next_actions.push("add structured coverage edges or relax the advisory policy".to_string());
    }
    if !runtime_theory.present {
        next_actions.push(
            "run `axiograph check theory` and attach a RuntimeTheoryCheckSummaryV1 sidecar before requiring runtime theory"
                .to_string(),
        );
    }
    if runtime_theory.residual_obligations.unwrap_or(0) > 0 {
        next_actions.push(
            "resolve residual runtime-theory obligations or keep this gate advisory".to_string(),
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
            total_rules: behavior_report_u64(
                behavior_report,
                "/context_report/coverage/total_rules",
            ),
            covered_rules: behavior_report_u64(
                behavior_report,
                "/context_report/coverage/covered_rules",
            ),
            tested_rules: behavior_report_u64(
                behavior_report,
                "/context_report/coverage/tested_rules",
            ),
            implemented_rules: behavior_report_u64(
                behavior_report,
                "/context_report/coverage/implemented_rules",
            ),
            drifted_rules: behavior_report_u64(
                behavior_report,
                "/context_report/coverage/drifted_rules",
            ),
            missing_obligations: behavior_report_string_array(
                behavior_report,
                "/context_report/coverage/missing_obligations",
            ),
            uncovered_rule_ids: behavior_report_string_array(
                behavior_report,
                "/context_report/coverage/uncovered_rule_ids",
            ),
            code_refs_total: code_refs.len(),
            missing_code_refs: missing_code_refs.clone(),
            required_codegen_languages: required_codegen_languages.clone(),
            present_codegen_languages: present_codegen_languages.clone(),
            missing_codegen_languages: missing_codegen.clone(),
            runtime_theory_present: runtime_theory.present,
            runtime_theory_residual_obligations: runtime_theory.residual_obligations,
            runtime_theory_blocking_obligations: Some(blocking_runtime_theory),
        },
        pass,
        failures.clone(),
        warnings.clone(),
        next_actions.clone(),
    );
    ContinuousSoftwareCoverageReportV1 {
        version: CONTINUOUS_SOFTWARE_COVERAGE_REPORT_VERSION_V1.to_string(),
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
    let ci_ready = strict_coverage
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

fn continuous_typed_refs(bundle: &ToolingOverlayBundleV1) -> ContinuousCoverageTypedRefsV1 {
    let refs = all_overlay_refs(bundle);
    let mut ontology_refs = overlay_ref_resolution_summary(&refs, refs.len(), 0);
    ontology_refs.unresolved_refs = 0;
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

fn runtime_theory_sidecar_summary(report: &Value) -> RuntimeTheorySidecarSummaryV1 {
    let Some(theory) = first_object_by_key(report, &["runtime_theory_check", "runtime_theory"])
    else {
        return RuntimeTheorySidecarSummaryV1::default();
    };
    RuntimeTheorySidecarSummaryV1 {
        present: true,
        module_digest: find_string(theory, &["module_digest"]),
        closure_tiers: find_string_array(theory, &["closure_tiers", "closure_tier"]),
        checked_obligations: find_u64(theory, &["checked_obligations"]),
        review_only_obligations: find_u64(theory, &["review_only_obligations"]),
        residual_obligations: find_u64(theory, &["residual_obligations", "residual_count"]),
        blocked_obligations: find_u64(theory, &["blocked_obligations", "blocked_count"]),
        blocking_errors: find_u64(theory, &["blocking_errors", "blocking_judgments"]),
        completeness_claim: find_string(theory, &["completeness_claim"]),
        ontology_closure_claim: find_string(theory, &["ontology_closure_claim"]),
        residual_obligation_ids: find_string_array(
            theory,
            &["residual_obligation_ids", "residual_obligations"],
        ),
        notes: find_string_array(theory, &["notes"]),
    }
}

fn behavior_report_u64(report: &Value, pointer: &str) -> u64 {
    report
        .pointer(pointer)
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_array().map(|values| values.len() as u64))
        })
        .unwrap_or(0)
}

fn behavior_report_string_array(report: &Value, pointer: &str) -> Vec<String> {
    report
        .pointer(pointer)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn first_object_by_key<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(candidate @ Value::Object(_)) = map.get(*key) {
                    return Some(candidate);
                }
            }
            map.values()
                .find_map(|nested| first_object_by_key(nested, keys))
        }
        Value::Array(values) => values
            .iter()
            .find_map(|nested| first_object_by_key(nested, keys)),
        _ => None,
    }
}

fn find_string(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(text) = map.get(*key).and_then(Value::as_str) {
                    return Some(text.to_string());
                }
            }
            map.values().find_map(|nested| find_string(nested, keys))
        }
        Value::Array(values) => values.iter().find_map(|nested| find_string(nested, keys)),
        _ => None,
    }
}

fn find_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(number) = map.get(*key).and_then(Value::as_u64) {
                    return Some(number);
                }
            }
            map.values().find_map(|nested| find_u64(nested, keys))
        }
        Value::Array(values) => values.iter().find_map(|nested| find_u64(nested, keys)),
        _ => None,
    }
}

fn find_string_array(value: &Value, keys: &[&str]) -> Vec<String> {
    let mut found = BTreeSet::new();
    find_string_array_inner(value, keys, &mut found);
    found.into_iter().collect()
}

fn find_string_array_inner(value: &Value, keys: &[&str], found: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(candidate) = map.get(*key) {
                    match candidate {
                        Value::Array(values) => {
                            for value in values {
                                if let Some(text) = value.as_str() {
                                    found.insert(text.to_string());
                                }
                            }
                        }
                        Value::String(text) => {
                            found.insert(text.clone());
                        }
                        _ => {}
                    }
                }
            }
            for nested in map.values() {
                find_string_array_inner(nested, keys, found);
            }
        }
        Value::Array(values) => {
            for nested in values {
                find_string_array_inner(nested, keys, found);
            }
        }
        _ => {}
    }
}

#[derive(Clone)]
struct KernelRefIndexEntry {
    kind: OverlayRefKindV1,
    label: String,
    kernel_ref: KernelRefV1,
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
    fn new(kernel: &KernelModuleIr) -> Self {
        let mut by_id = BTreeMap::new();
        let mut by_name = BTreeMap::new();
        let mut relations = BTreeMap::new();
        let surface = kernel.kernel_surface_v1();
        let schema_names = kernel
            .schemas
            .iter()
            .map(|schema| (schema.schema_id.to_string(), schema_name(schema)))
            .collect::<BTreeMap<_, _>>();
        let theory_schema_names = kernel
            .theories
            .iter()
            .map(|theory| {
                (
                    theory.theory_id.to_string(),
                    schema_names.get(theory.schema_id.as_str()).cloned(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        for surface_ref in &surface.refs {
            match surface_ref {
                KernelRefV1::Module { .. } => {}
                KernelRefV1::Schema { schema_id } => insert_ref(
                    &mut by_id,
                    &mut by_name,
                    OverlayRefKindV1::Schema,
                    None,
                    schema_id.to_string(),
                    local_ref_name(schema_id.as_str()),
                    surface_ref.clone(),
                ),
                KernelRefV1::SchemaObject {
                    schema_id,
                    object:
                        SchemaCategoryObjectRefIr::ObjectType {
                            object_type_id,
                            name,
                        },
                } => insert_ref(
                    &mut by_id,
                    &mut by_name,
                    OverlayRefKindV1::Object,
                    schema_names.get(schema_id.as_str()).cloned(),
                    object_type_id.to_string(),
                    name.clone(),
                    surface_ref.clone(),
                ),
                KernelRefV1::SchemaObject {
                    schema_id,
                    object: SchemaCategoryObjectRefIr::RelationObject { relation_id, name },
                } => insert_ref(
                    &mut by_id,
                    &mut by_name,
                    OverlayRefKindV1::Relation,
                    schema_names.get(schema_id.as_str()).cloned(),
                    relation_id.to_string(),
                    name.clone(),
                    surface_ref.clone(),
                ),
                KernelRefV1::SchemaArrow { .. } => {}
                KernelRefV1::Theory {
                    theory_id,
                    schema_id,
                } => insert_ref(
                    &mut by_id,
                    &mut by_name,
                    OverlayRefKindV1::Theory,
                    schema_names.get(schema_id.as_str()).cloned(),
                    theory_id.to_string(),
                    local_ref_name(theory_id.as_str()),
                    surface_ref.clone(),
                ),
                KernelRefV1::TheoryObligation { obligation } => {
                    let (kind, id, label, theory_id) =
                        overlay_ref_from_theory_obligation(obligation);
                    insert_ref(
                        &mut by_id,
                        &mut by_name,
                        kind,
                        theory_schema_names
                            .get(theory_id.as_str())
                            .cloned()
                            .flatten(),
                        id,
                        label,
                        surface_ref.clone(),
                    );
                }
                KernelRefV1::TheorySubject { .. } => {}
                KernelRefV1::Instance {
                    instance_id,
                    schema_id,
                } => insert_ref(
                    &mut by_id,
                    &mut by_name,
                    OverlayRefKindV1::Instance,
                    schema_names.get(schema_id.as_str()).cloned(),
                    instance_id.to_string(),
                    local_ref_name(instance_id.as_str()),
                    surface_ref.clone(),
                ),
                KernelRefV1::InstanceObjectImage { .. }
                | KernelRefV1::InstanceArrowImage { .. } => {}
                KernelRefV1::StableFact {
                    fact_id,
                    relation_id,
                    ..
                } => {
                    let label = kernel
                        .schemas
                        .iter()
                        .flat_map(|schema| schema.relations.values())
                        .find(|relation| relation.relation_id == *relation_id)
                        .map(|relation| relation.name.clone())
                        .unwrap_or_else(|| relation_id.to_string());
                    insert_ref(
                        &mut by_id,
                        &mut by_name,
                        OverlayRefKindV1::Fact,
                        None,
                        fact_id.to_string(),
                        label,
                        surface_ref.clone(),
                    );
                }
            }
        }
        for schema in &kernel.schemas {
            for relation in schema.relations.values() {
                let stable_id = relation.relation_id.to_string();
                relations.insert(
                    stable_id.clone(),
                    RelationForQuery {
                        schema: schema_name(schema),
                        name: relation.name.clone(),
                        roles: relation_roles(relation),
                    },
                );
            }
        }
        Self {
            by_id,
            by_name,
            relations,
        }
    }

    fn resolve(&self, reference: &OverlayRefV1) -> Option<(String, String, KernelRefV1)> {
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
    kernel_ref: KernelRefV1,
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

fn overlay_ref_from_theory_obligation(
    obligation: &TheoryObligationRefIr,
) -> (OverlayRefKindV1, String, String, String) {
    match obligation {
        TheoryObligationRefIr::Constraint {
            theory_id,
            constraint_id,
            summary,
            ..
        } => (
            OverlayRefKindV1::Constraint,
            constraint_id.to_string(),
            summary.clone(),
            theory_id.to_string(),
        ),
        TheoryObligationRefIr::PathEquation {
            theory_id,
            equation_id,
            name,
        }
        | TheoryObligationRefIr::OpaqueEquation {
            theory_id,
            equation_id,
            name,
        } => (
            OverlayRefKindV1::Equation,
            equation_id.to_string(),
            name.clone(),
            theory_id.to_string(),
        ),
        TheoryObligationRefIr::RewriteRule {
            theory_id,
            rule_id,
            name,
        } => (
            OverlayRefKindV1::Rewrite,
            rule_id.to_string(),
            name.clone(),
            theory_id.to_string(),
        ),
    }
}

fn local_ref_name(id: &str) -> String {
    id.rsplit_once(':')
        .map(|(_, local)| local.to_string())
        .unwrap_or_else(|| id.to_string())
}

fn schema_name(schema: &CompiledSchemaIr) -> String {
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

fn collect_codegen_languages(report: &Value) -> Vec<String> {
    report
        .get("codegen_previews")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|preview| preview.get("language").and_then(Value::as_str))
        .map(|language| language.trim().to_ascii_lowercase())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_kernel() -> KernelModuleIr {
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
        compile_kernel_from_axi_text(axi).expect("compile kernel")
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
            .any(|r| r.normalized_id.contains("OrderHasPayment")));
        assert!(report.normalized_refs.iter().any(|r| {
            r.kernel_ref_label.contains("OrderHasPayment")
                && r.kernel_ref["kind"].as_str() == Some("schema_object")
        }));
    }

    #[test]
    fn public_tooling_schemas_are_generated_from_typed_dtos() {
        let overlay_schema = tooling_overlay_bundle_schema();
        let definition_schema = definition_query_schema();
        let coverage_schema = coverage_query_schema();
        let report_schema = continuous_software_coverage_report_schema();

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
            prompt: "define the shipment fulfills order business rule".to_string(),
            kind_hint: None,
            context_hint: None,
            candidate_refs: Vec::new(),
            max_matches: Some(4),
            include_queries: true,
        };

        let report = definition_query_report(&kernel, None, &query);
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
            coverage_mode: CoverageModeV1::Enforced,
            terms: vec!["payment".to_string()],
            relation_names: Vec::new(),
            cq_names: Vec::new(),
            code_refs: Vec::new(),
            surface_hints: Vec::new(),
            axql: None,
            max_matches: Some(3),
        };

        let report = coverage_query_report(&kernel, None, &query);
        assert_eq!(report.coverage_mode, CoverageModeV1::Exploratory);
        assert!(!report.matched_refs.is_empty());
        assert!(report
            .caveats
            .iter()
            .any(|caveat| caveat.contains("cannot satisfy promotion gates")));
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
        let behavior_report = serde_json::json!({
            "behavior_case": { "case_id": "checkout.reserve" },
            "codegen_previews": [{ "language": "typescript" }],
            "context_report": {
                "coverage": {
                    "missing_obligations": ["schema/order/relation/payment/rule/key/0"]
                }
            }
        });

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
    fn advisory_coverage_warns_but_enforced_runtime_theory_blocks() {
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
        let behavior_report = serde_json::json!({
            "behavior_case": { "case_id": "checkout.reserve" },
            "codegen_previews": [{ "language": "typescript" }],
            "context_report": {
                "coverage": {
                    "missing_obligations": ["schema/order/relation/payment/rule/key/0"]
                }
            },
            "runtime_theory_check": {
                "version": "runtime_theory_check_summary_v1",
                "module_digest": "fnv1a64:test",
                "residual_obligations": 1,
                "residual_obligation_ids": ["runtime/theory/residual"],
                "blocking_errors": 0,
                "blocked_obligations": 0,
                "closure_tiers": ["finite_fragment"],
                "completeness_claim": "not_claimed_for_all_obligations",
                "ontology_closure_claim": "not_claimed_for_all_obligations"
            }
        });

        let advisory = continuous_coverage_report_from_behavior_report(
            &behavior_report,
            &bundle,
            Path::new("."),
        );
        assert!(advisory.pass, "{advisory:?}");
        assert_eq!(
            advisory.authoring_flow.profile.profile,
            AuthoringCoverageProfileV1::Advisory
        );
        assert!(advisory.runtime_theory.present);
        assert_eq!(advisory.runtime_theory.residual_obligations, Some(1));
        assert!(advisory
            .warnings
            .iter()
            .any(|warning| warning.contains("runtime theory sidecar")));

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
            .any(|failure| failure.contains("runtime theory sidecar")));
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
