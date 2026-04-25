use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use axiograph_pathdb::axi_semantics::MetaPlaneIndex;
use axiograph_pathdb::{AcceptedSnapshotId, PathDB};

pub(crate) const SEMANTIC_BUSINESS_RULE_TOOL_NAME: &str = "semantic_business_rule";
pub(crate) const SEMANTIC_COVERAGE_TOOL_NAME: &str = "semantic_coverage";
pub(crate) const SEMANTIC_AGENT_REPORT_TOOL_NAME: &str = "semantic_agent_report";
pub(crate) const SEMANTIC_CONTEXT_REPORT_TOOL_NAME: &str = "semantic_context_report";
pub(crate) const SEMANTIC_CONTEXT_MAP_TOOL_NAME: &str = "semantic_context_map";
pub(crate) const SEMANTIC_BEHAVIOR_CASE_TOOL_NAME: &str = "semantic_behavior_case";
pub(crate) const SEMANTIC_SLICE_BUILD_TOOL_NAME: &str = "semantic_slice_build";
pub(crate) const SEMANTIC_SLICE_SHOW_TOOL_NAME: &str = "semantic_slice_show";
pub(crate) const SEMANTIC_SLICE_DIFF_TOOL_NAME: &str = "semantic_slice_diff";
pub(crate) const SEMANTIC_MERGE_PLAN_TOOL_NAME: &str = "semantic_merge_plan";
pub(crate) const SEMANTIC_REBASE_PLAN_TOOL_NAME: &str = "semantic_rebase_plan";
pub(crate) const SEMANTIC_RESOLVER_STEPS_TOOL_NAME: &str = "semantic_resolver_steps";
pub(crate) const SEMANTIC_THEORY_GRAPH_TOOL_NAME: &str = "semantic_theory_graph";
pub(crate) const SEMANTIC_THEORY_CHECK_TOOL_NAME: &str = "semantic_theory_check";

const SEMANTIC_BUSINESS_RULE_TOOL_VERSION: &str = "axiograph_semantic_business_rule_v1";
const SEMANTIC_COVERAGE_TOOL_VERSION: &str = "axiograph_semantic_coverage_v1";
const SEMANTIC_AGENT_REPORT_TOOL_VERSION: &str = "axiograph_semantic_agent_report_v1";
const SEMANTIC_CONTEXT_REPORT_TOOL_VERSION: &str = "axiograph_semantic_context_report_v1";
const SEMANTIC_CONTEXT_MAP_TOOL_VERSION: &str = "axiograph_semantic_context_map_v1";
const SEMANTIC_BEHAVIOR_CASE_TOOL_VERSION: &str = "axiograph_semantic_behavior_case_v1";
const SEMANTIC_SLICE_BUILD_TOOL_VERSION: &str = "axiograph_semantic_slice_build_v1";
const SEMANTIC_SLICE_SHOW_TOOL_VERSION: &str = "axiograph_semantic_slice_show_v1";
const SEMANTIC_SLICE_DIFF_TOOL_VERSION: &str = "axiograph_semantic_slice_diff_v1";
const SEMANTIC_MERGE_PLAN_TOOL_VERSION: &str = "axiograph_semantic_merge_plan_v1";
const SEMANTIC_REBASE_PLAN_TOOL_VERSION: &str = "axiograph_semantic_rebase_plan_v1";
const SEMANTIC_RESOLVER_STEPS_TOOL_VERSION: &str = "axiograph_semantic_resolver_steps_v1";
const SEMANTIC_THEORY_GRAPH_TOOL_VERSION: &str = "axiograph_semantic_theory_graph_v1";
const SEMANTIC_THEORY_CHECK_TOOL_VERSION: &str = "axiograph_semantic_theory_check_v1";

#[derive(Debug, Clone)]
pub(crate) struct SemanticToolSpecV1 {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

#[derive(Clone, Copy)]
pub(crate) struct SemanticToolContext<'a> {
    pub db: &'a PathDB,
    pub meta: Option<&'a MetaPlaneIndex>,
    pub accepted_snapshot_id: Option<&'a AcceptedSnapshotId>,
}

impl<'a> SemanticToolContext<'a> {
    fn accepted_snapshot_id(self) -> Option<AcceptedSnapshotId> {
        self.accepted_snapshot_id.cloned()
    }

    fn lifecycle_state(self, provided: Option<String>) -> String {
        provided.unwrap_or_else(|| {
            if self.accepted_snapshot_id.is_some() {
                "accepted".to_string()
            } else {
                "runtime_checked".to_string()
            }
        })
    }

    fn meta_plane(self) -> Result<MetaPlaneIndex> {
        match self.meta {
            Some(meta) => Ok(meta.clone()),
            None => MetaPlaneIndex::from_db(self.db),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SemanticBusinessRuleArgs {
    pub scope: crate::semantic_claim::RuntimeRuleScopeV1,
    #[serde(default)]
    pub lifecycle_state: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SemanticCoverageArgs {
    #[serde(default)]
    pub lifecycle_state: Option<String>,
    #[serde(default)]
    pub surfaces: Vec<crate::semantic_claim::ImplementationSurfaceRefV1>,
    #[serde(default)]
    pub edges: Vec<crate::semantic_claim::CoverageEdgeV1>,
    #[serde(default)]
    pub runtime_theory_check: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
    #[serde(default)]
    pub runtime_theory_check_input: Option<crate::runtime_theory_check::RuntimeTheoryCheckInputV1>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SemanticAgentReportArgs {
    pub task: crate::semantic_claim::AgentTaskRefV1,
    #[serde(default)]
    pub lifecycle_state: Option<String>,
    #[serde(default)]
    pub surfaces: Vec<crate::semantic_claim::ImplementationSurfaceRefV1>,
    #[serde(default)]
    pub edges: Vec<crate::semantic_claim::CoverageEdgeV1>,
    #[serde(default)]
    pub runtime_theory_check: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
    #[serde(default)]
    pub runtime_theory_check_input: Option<crate::runtime_theory_check::RuntimeTheoryCheckInputV1>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SemanticContextReportArgs {
    pub context: crate::context_report::BoundedContextV1,
    #[serde(default)]
    pub lifecycle_state: Option<String>,
    #[serde(default)]
    pub evolution_preview: Option<crate::evolution_preview::EvolutionPreviewV1>,
    #[serde(default)]
    pub runtime_theory_check: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
    #[serde(default)]
    pub runtime_theory_check_input: Option<crate::runtime_theory_check::RuntimeTheoryCheckInputV1>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SemanticContextMapArgs {
    pub source: crate::context_report::BoundedContextV1,
    pub target: crate::context_report::BoundedContextV1,
    pub relationship: crate::context_report::ContextMapRelationshipV1,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SemanticBehaviorCaseArgs {
    pub behavior_case: crate::behavior_case::BehaviorCaseV1,
    #[serde(default)]
    pub lifecycle_state: Option<String>,
    #[serde(default)]
    pub evolution_preview: Option<crate::evolution_preview::EvolutionPreviewV1>,
    #[serde(default)]
    pub runtime_theory_check: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
    #[serde(default)]
    pub runtime_theory_check_input: Option<crate::runtime_theory_check::RuntimeTheoryCheckInputV1>,
    #[serde(default)]
    pub codegen: crate::behavior_case::BehaviorCaseCodegenRequestV1,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SemanticSliceBuildArgs {
    pub ref_view: crate::accepted_plane::SemRefViewV1,
    #[serde(default)]
    pub selector: crate::semantic_merge_lattice::SemanticSliceSelectorV1,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SemanticSliceShowArgs {
    pub slice: crate::semantic_merge_lattice::SemanticSliceManifestV1,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SemanticSliceDiffArgs {
    pub left: crate::semantic_merge_lattice::SemanticSliceManifestV1,
    pub right: crate::semantic_merge_lattice::SemanticSliceManifestV1,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SemanticMergePlanArgs {
    pub dry_run: crate::accepted_plane::SemMergeDryRunResultV1,
    #[serde(default)]
    pub source_selector: crate::semantic_merge_lattice::SemanticSliceSelectorV1,
    #[serde(default)]
    pub target_selector: crate::semantic_merge_lattice::SemanticSliceSelectorV1,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SemanticResolverStepsArgs {
    pub merge_plan: crate::semantic_merge_lattice::SemanticMergePlanV1,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SemanticTheoryGraphArgs {
    pub axi_text: String,
    #[serde(default)]
    pub theory: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SemanticTheoryCheckArgs {
    pub axi_text: String,
    #[serde(default)]
    pub theory: Option<String>,
    #[serde(default)]
    pub closure_tier: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SemanticBusinessRuleToolResultV1 {
    pub version: &'static str,
    pub accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub report: crate::semantic_claim::BusinessRuleApplicabilityReportV1,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SemanticCoverageToolResultV1 {
    pub version: &'static str,
    pub accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub coverage: crate::semantic_claim::CoverageReportV1,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SemanticAgentReportToolResultV1 {
    pub version: &'static str,
    pub accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub report: crate::semantic_claim::AgentEngineeringReportV1,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SemanticContextReportToolResultV1 {
    pub version: &'static str,
    pub accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub report: crate::context_report::ContextReportV1,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SemanticContextMapToolResultV1 {
    pub version: &'static str,
    pub context_map: crate::context_report::ContextMapV1,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SemanticBehaviorCaseToolResultV1 {
    pub version: &'static str,
    pub accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub report: crate::behavior_case::BehaviorCaseReportV1,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SemanticSliceBuildToolResultV1 {
    pub version: &'static str,
    pub slice: crate::semantic_merge_lattice::SemanticSliceManifestV1,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SemanticSliceShowToolResultV1 {
    pub version: &'static str,
    pub slice: crate::semantic_merge_lattice::SemanticSliceManifestV1,
    pub selected_ref_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SemanticSliceDiffToolResultV1 {
    pub version: &'static str,
    pub diff: crate::semantic_merge_lattice::SemanticSliceDiffV1,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SemanticMergePlanToolResultV1 {
    pub version: &'static str,
    pub merge_plan: crate::semantic_merge_lattice::SemanticMergePlanV1,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SemanticResolverStepsToolResultV1 {
    pub version: &'static str,
    pub report: crate::semantic_merge_lattice::SemanticResolverStepsReportV1,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SemanticTheoryGraphToolResultV1 {
    pub version: &'static str,
    pub module_digest: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graphs: Vec<axiograph_pathdb::kernel_ir::TheoryObligationGraphV1>,
    pub trust_boundary: String,
    pub completeness_claim: String,
    pub ontology_closure_claim: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SemanticTheoryCheckToolResultV1 {
    pub version: &'static str,
    pub report: crate::runtime_theory_check::RuntimeTheoryCheckModuleReportV1,
}

pub(crate) fn semantic_tool_specs() -> Vec<SemanticToolSpecV1> {
    let semantic_scope_schema = semantic_scope_json_schema();
    let implementation_surface_schema = implementation_surface_json_schema();
    let coverage_edge_schema = coverage_edge_json_schema();
    vec![
        SemanticToolSpecV1 {
            name: SEMANTIC_BUSINESS_RULE_TOOL_NAME,
            description: "Return a typed business-rule applicability report for one relation or theory scope using the loaded snapshot's semantic metadata.",
            input_schema: json!({
                "type": "object",
                "required": ["scope"],
                "properties": {
                    "scope": semantic_scope_schema,
                    "lifecycle_state": { "type": "string" }
                }
            }),
        },
        SemanticToolSpecV1 {
            name: SEMANTIC_COVERAGE_TOOL_NAME,
            description: "Return a typed semantic coverage report over implementation surfaces, rule mappings, and coverage edges.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "lifecycle_state": { "type": "string" },
                    "surfaces": {
                        "type": "array",
                        "items": implementation_surface_schema.clone()
                    },
                    "edges": {
                        "type": "array",
                        "items": coverage_edge_schema.clone()
                    },
                    "runtime_theory_check": { "type": "object" },
                    "runtime_theory_check_input": { "type": "object" }
                }
            }),
        },
        SemanticToolSpecV1 {
            name: SEMANTIC_AGENT_REPORT_TOOL_NAME,
            description: "Return an agent-facing semantic engineering report over a task, mapped implementation surfaces, and coverage edges.",
            input_schema: json!({
                "type": "object",
                "required": ["task"],
                "properties": {
                    "task": { "type": "object" },
                    "lifecycle_state": { "type": "string" },
                    "surfaces": {
                        "type": "array",
                        "items": implementation_surface_schema
                    },
                    "edges": {
                        "type": "array",
                        "items": coverage_edge_schema
                    },
                    "runtime_theory_check": { "type": "object" },
                    "runtime_theory_check_input": { "type": "object" }
                }
            }),
        },
        SemanticToolSpecV1 {
            name: SEMANTIC_CONTEXT_REPORT_TOOL_NAME,
            description: "Return a read-only bounded-context report composed from rule-scope applicability, implementation-surface coverage, competency-question trust, and optional evolution-preview data.",
            input_schema: json!({
                "type": "object",
                "required": ["context"],
                "properties": {
                    "context": bounded_context_json_schema(),
                    "lifecycle_state": { "type": "string" },
                    "evolution_preview": { "type": "object" },
                    "runtime_theory_check": { "type": "object" },
                    "runtime_theory_check_input": { "type": "object" }
                }
            }),
        },
        SemanticToolSpecV1 {
            name: SEMANTIC_CONTEXT_MAP_TOOL_NAME,
            description: "Build a DDD/fDDD context-map object with source/target SemanticSliceSelectorV1 payloads for semantic merge/rebase planning.",
            input_schema: json!({
                "type": "object",
                "required": ["source", "target", "relationship"],
                "properties": {
                    "source": bounded_context_json_schema(),
                    "target": bounded_context_json_schema(),
                    "relationship": {
                        "type": "string",
                        "enum": [
                            "shared_kernel",
                            "customer_supplier",
                            "conformist",
                            "anti_corruption_layer",
                            "published_language",
                            "open_host_service",
                            "separate_ways",
                            "review_only"
                        ]
                    }
                }
            }),
        },
        SemanticToolSpecV1 {
            name: SEMANTIC_BEHAVIOR_CASE_TOOL_NAME,
            description: "Check a JSON BehaviorCaseV1 against bounded-context rules, CQ coverage, trust contracts, and emit a CaseReceiptV1 plus Rust/TypeScript test skeleton previews.",
            input_schema: json!({
                "type": "object",
                "required": ["behavior_case"],
                "properties": {
                    "behavior_case": behavior_case_json_schema(),
                    "lifecycle_state": { "type": "string" },
                    "evolution_preview": { "type": "object" },
                    "runtime_theory_check": { "type": "object" },
                    "runtime_theory_check_input": { "type": "object" },
                    "codegen": {
                        "type": "object",
                        "properties": {
                            "languages": {
                                "type": "array",
                                "items": {
                                    "type": "string",
                                    "enum": ["rust", "typescript"]
                                }
                            }
                        }
                    }
                }
            }),
        },
        SemanticToolSpecV1 {
            name: SEMANTIC_SLICE_BUILD_TOOL_NAME,
            description: "Build a typed semantic slice manifest from a supplied semantic ref view and optional SemanticSliceSelectorV1.",
            input_schema: json!({
                "type": "object",
                "required": ["ref_view"],
                "properties": {
                    "ref_view": { "type": "object" },
                    "selector": { "type": "object" }
                }
            }),
        },
        SemanticToolSpecV1 {
            name: SEMANTIC_SLICE_SHOW_TOOL_NAME,
            description: "Return a supplied SemanticSliceManifestV1 with a compact selected-ref summary.",
            input_schema: json!({
                "type": "object",
                "required": ["slice"],
                "properties": {
                    "slice": { "type": "object" }
                }
            }),
        },
        SemanticToolSpecV1 {
            name: SEMANTIC_SLICE_DIFF_TOOL_NAME,
            description: "Diff two SemanticSliceManifestV1 objects by stable typed refs.",
            input_schema: json!({
                "type": "object",
                "required": ["left", "right"],
                "properties": {
                    "left": { "type": "object" },
                    "right": { "type": "object" }
                }
            }),
        },
        SemanticToolSpecV1 {
            name: SEMANTIC_MERGE_PLAN_TOOL_NAME,
            description: "Build a conservative SemanticMergePlanV1 from a supplied semantic merge dry-run payload.",
            input_schema: json!({
                "type": "object",
                "required": ["dry_run"],
                "properties": {
                    "dry_run": { "type": "object" },
                    "source_selector": { "type": "object" },
                    "target_selector": { "type": "object" }
                }
            }),
        },
        SemanticToolSpecV1 {
            name: SEMANTIC_REBASE_PLAN_TOOL_NAME,
            description: "Build a conservative SemanticMergePlanV1 in rebase/transport mode from a supplied semantic merge dry-run payload.",
            input_schema: json!({
                "type": "object",
                "required": ["dry_run"],
                "properties": {
                    "dry_run": { "type": "object" },
                    "source_selector": { "type": "object" },
                    "target_selector": { "type": "object" }
                }
            }),
        },
        SemanticToolSpecV1 {
            name: SEMANTIC_RESOLVER_STEPS_TOOL_NAME,
            description: "Extract typed resolver handles, residual obligations, and next actions from a SemanticMergePlanV1.",
            input_schema: json!({
                "type": "object",
                "required": ["merge_plan"],
                "properties": {
                    "merge_plan": { "type": "object" }
                }
            }),
        },
        SemanticToolSpecV1 {
            name: SEMANTIC_THEORY_GRAPH_TOOL_NAME,
            description: "Compile canonical .axi text and return runtime theory-obligation graphs for type-directed exploration, CQ repair, migration, and reconciliation planning.",
            input_schema: json!({
                "type": "object",
                "required": ["axi_text"],
                "properties": {
                    "axi_text": { "type": "string" },
                    "theory": { "type": "string" }
                }
            }),
        },
        SemanticToolSpecV1 {
            name: SEMANTIC_THEORY_CHECK_TOOL_NAME,
            description: "Compile canonical .axi text and return runtime theory-check closure/completeness reports for CQ gates, fDDD invariants, and semantic merge planning.",
            input_schema: json!({
                "type": "object",
                "required": ["axi_text"],
                "properties": {
                    "axi_text": { "type": "string" },
                    "theory": { "type": "string" },
                    "closure_tier": {
                        "type": "string",
                        "enum": ["finite_fragment", "evidence_weighted", "global_indexed"]
                    }
                }
            }),
        },
    ]
}

pub(crate) fn is_semantic_tool(name: &str) -> bool {
    matches!(
        name,
        SEMANTIC_BUSINESS_RULE_TOOL_NAME
            | SEMANTIC_COVERAGE_TOOL_NAME
            | SEMANTIC_AGENT_REPORT_TOOL_NAME
            | SEMANTIC_CONTEXT_REPORT_TOOL_NAME
            | SEMANTIC_CONTEXT_MAP_TOOL_NAME
            | SEMANTIC_BEHAVIOR_CASE_TOOL_NAME
            | SEMANTIC_SLICE_BUILD_TOOL_NAME
            | SEMANTIC_SLICE_SHOW_TOOL_NAME
            | SEMANTIC_SLICE_DIFF_TOOL_NAME
            | SEMANTIC_MERGE_PLAN_TOOL_NAME
            | SEMANTIC_REBASE_PLAN_TOOL_NAME
            | SEMANTIC_RESOLVER_STEPS_TOOL_NAME
            | SEMANTIC_THEORY_GRAPH_TOOL_NAME
            | SEMANTIC_THEORY_CHECK_TOOL_NAME
    )
}

pub(crate) fn invoke_semantic_tool(
    name: &str,
    context: SemanticToolContext<'_>,
    arguments: Value,
) -> Result<Value> {
    match name {
        SEMANTIC_BUSINESS_RULE_TOOL_NAME => {
            serde_json::to_value(call_semantic_business_rule(context, arguments)?)
                .map_err(Into::into)
        }
        SEMANTIC_COVERAGE_TOOL_NAME => {
            serde_json::to_value(call_semantic_coverage(context, arguments)?).map_err(Into::into)
        }
        SEMANTIC_AGENT_REPORT_TOOL_NAME => {
            serde_json::to_value(call_semantic_agent_report(context, arguments)?)
                .map_err(Into::into)
        }
        SEMANTIC_CONTEXT_REPORT_TOOL_NAME => {
            serde_json::to_value(call_semantic_context_report(context, arguments)?)
                .map_err(Into::into)
        }
        SEMANTIC_CONTEXT_MAP_TOOL_NAME => {
            serde_json::to_value(call_semantic_context_map(arguments)?).map_err(Into::into)
        }
        SEMANTIC_BEHAVIOR_CASE_TOOL_NAME => {
            serde_json::to_value(call_semantic_behavior_case(context, arguments)?)
                .map_err(Into::into)
        }
        SEMANTIC_SLICE_BUILD_TOOL_NAME => {
            serde_json::to_value(call_semantic_slice_build(arguments)?).map_err(Into::into)
        }
        SEMANTIC_SLICE_SHOW_TOOL_NAME => {
            serde_json::to_value(call_semantic_slice_show(arguments)?).map_err(Into::into)
        }
        SEMANTIC_SLICE_DIFF_TOOL_NAME => {
            serde_json::to_value(call_semantic_slice_diff(arguments)?).map_err(Into::into)
        }
        SEMANTIC_MERGE_PLAN_TOOL_NAME => {
            serde_json::to_value(call_semantic_merge_plan(arguments)?).map_err(Into::into)
        }
        SEMANTIC_REBASE_PLAN_TOOL_NAME => {
            serde_json::to_value(call_semantic_rebase_plan(arguments)?).map_err(Into::into)
        }
        SEMANTIC_RESOLVER_STEPS_TOOL_NAME => {
            serde_json::to_value(call_semantic_resolver_steps(arguments)?).map_err(Into::into)
        }
        SEMANTIC_THEORY_GRAPH_TOOL_NAME => {
            serde_json::to_value(call_semantic_theory_graph(arguments)?).map_err(Into::into)
        }
        SEMANTIC_THEORY_CHECK_TOOL_NAME => {
            serde_json::to_value(call_semantic_theory_check(arguments)?).map_err(Into::into)
        }
        other => Err(anyhow!("unknown semantic tool `{other}`")),
    }
}

pub(crate) fn call_semantic_business_rule(
    context: SemanticToolContext<'_>,
    arguments: Value,
) -> Result<SemanticBusinessRuleToolResultV1> {
    let args: SemanticBusinessRuleArgs = serde_json::from_value(arguments)
        .map_err(|err| anyhow!("semantic_business_rule: invalid args: {err}"))?;
    let accepted_snapshot_id = context.accepted_snapshot_id();
    let lifecycle_state = context.lifecycle_state(args.lifecycle_state);
    let meta = context.meta_plane()?;
    let scope = normalized_business_rule_scope(args.scope)?;

    let report = match scope.scope_class {
        crate::semantic_claim::RuntimeRuleScopeClassV1::Relation => {
            let relation = scope.relation.as_deref().ok_or_else(|| {
                anyhow!("semantic_business_rule: relation scope requires `scope.relation`")
            })?;
            crate::semantic_claim::business_rule_applicability_for_relation(
                &meta,
                accepted_snapshot_id.clone(),
                &lifecycle_state,
                &scope.schema,
                relation,
            )
        }
        crate::semantic_claim::RuntimeRuleScopeClassV1::Theory => {
            let theory = scope.theory.as_deref().ok_or_else(|| {
                anyhow!("semantic_business_rule: theory scope requires `scope.theory`")
            })?;
            crate::semantic_claim::business_rule_applicability_for_theory(
                &meta,
                accepted_snapshot_id.clone(),
                &lifecycle_state,
                &scope.schema,
                theory,
            )
        }
    };

    Ok(SemanticBusinessRuleToolResultV1 {
        version: SEMANTIC_BUSINESS_RULE_TOOL_VERSION,
        accepted_snapshot_id,
        report,
    })
}

fn resolve_runtime_theory_check_summary(
    provided: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
    input: Option<crate::runtime_theory_check::RuntimeTheoryCheckInputV1>,
) -> Result<Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>> {
    if let Some(summary) = provided {
        return Ok(Some(summary));
    }
    input
        .as_ref()
        .map(crate::runtime_theory_check::runtime_theory_check_summary_from_input)
        .transpose()
}

pub(crate) fn call_semantic_coverage(
    context: SemanticToolContext<'_>,
    arguments: Value,
) -> Result<SemanticCoverageToolResultV1> {
    let args: SemanticCoverageArgs = serde_json::from_value(arguments)
        .map_err(|err| anyhow!("semantic_coverage: invalid args: {err}"))?;
    let accepted_snapshot_id = context.accepted_snapshot_id();
    let lifecycle_state = context.lifecycle_state(args.lifecycle_state);
    let meta = context.meta_plane()?;
    let runtime_theory_check =
        resolve_runtime_theory_check_summary(args.runtime_theory_check, args.runtime_theory_check_input)?;
    let coverage = crate::semantic_claim::semantic_coverage_report(
        &meta,
        accepted_snapshot_id.clone(),
        &lifecycle_state,
        &args.surfaces,
        &args.edges,
        runtime_theory_check,
    );

    Ok(SemanticCoverageToolResultV1 {
        version: SEMANTIC_COVERAGE_TOOL_VERSION,
        accepted_snapshot_id,
        coverage,
    })
}

pub(crate) fn call_semantic_agent_report(
    context: SemanticToolContext<'_>,
    arguments: Value,
) -> Result<SemanticAgentReportToolResultV1> {
    let args: SemanticAgentReportArgs = serde_json::from_value(arguments)
        .map_err(|err| anyhow!("semantic_agent_report: invalid args: {err}"))?;
    let accepted_snapshot_id = context.accepted_snapshot_id();
    let lifecycle_state = context.lifecycle_state(args.lifecycle_state);
    let meta = context.meta_plane()?;
    let runtime_theory_check =
        resolve_runtime_theory_check_summary(args.runtime_theory_check, args.runtime_theory_check_input)?;
    let report = crate::semantic_claim::agent_engineering_report(
        &meta,
        accepted_snapshot_id.clone(),
        &lifecycle_state,
        &args.task,
        &args.surfaces,
        &args.edges,
        runtime_theory_check,
    );

    Ok(SemanticAgentReportToolResultV1 {
        version: SEMANTIC_AGENT_REPORT_TOOL_VERSION,
        accepted_snapshot_id,
        report,
    })
}

pub(crate) fn call_semantic_context_report(
    context: SemanticToolContext<'_>,
    arguments: Value,
) -> Result<SemanticContextReportToolResultV1> {
    let args: SemanticContextReportArgs = serde_json::from_value(arguments)
        .map_err(|err| anyhow!("semantic_context_report: invalid args: {err}"))?;
    let accepted_snapshot_id = context.accepted_snapshot_id();
    let runtime_theory_check =
        resolve_runtime_theory_check_summary(args.runtime_theory_check, args.runtime_theory_check_input)?;
    let report = crate::context_report::build_context_report(
        context.db,
        context.meta,
        accepted_snapshot_id.clone(),
        args.lifecycle_state.as_deref(),
        &args.context,
        args.evolution_preview,
        runtime_theory_check,
    )?;

    Ok(SemanticContextReportToolResultV1 {
        version: SEMANTIC_CONTEXT_REPORT_TOOL_VERSION,
        accepted_snapshot_id,
        report,
    })
}

pub(crate) fn call_semantic_context_map(
    arguments: Value,
) -> Result<SemanticContextMapToolResultV1> {
    let args: SemanticContextMapArgs = serde_json::from_value(arguments)
        .map_err(|err| anyhow!("semantic_context_map: invalid args: {err}"))?;
    let context_map = crate::context_report::context_map_between_bounded_contexts(
        &args.source,
        &args.target,
        args.relationship,
    )?;
    Ok(SemanticContextMapToolResultV1 {
        version: SEMANTIC_CONTEXT_MAP_TOOL_VERSION,
        context_map,
    })
}

pub(crate) fn call_semantic_behavior_case(
    context: SemanticToolContext<'_>,
    arguments: Value,
) -> Result<SemanticBehaviorCaseToolResultV1> {
    let args: SemanticBehaviorCaseArgs = serde_json::from_value(arguments)
        .map_err(|err| anyhow!("semantic_behavior_case: invalid args: {err}"))?;
    let accepted_snapshot_id = context.accepted_snapshot_id();
    let runtime_theory_check =
        resolve_runtime_theory_check_summary(args.runtime_theory_check, args.runtime_theory_check_input)?;
    let report = crate::behavior_case::build_behavior_case_report(
        context.db,
        context.meta,
        accepted_snapshot_id.clone(),
        args.lifecycle_state.as_deref(),
        &args.behavior_case,
        args.evolution_preview,
        runtime_theory_check,
        &args.codegen,
    )?;

    Ok(SemanticBehaviorCaseToolResultV1 {
        version: SEMANTIC_BEHAVIOR_CASE_TOOL_VERSION,
        accepted_snapshot_id,
        report,
    })
}

pub(crate) fn call_semantic_slice_build(
    arguments: Value,
) -> Result<SemanticSliceBuildToolResultV1> {
    let args: SemanticSliceBuildArgs = serde_json::from_value(arguments)
        .map_err(|err| anyhow!("semantic_slice_build: invalid args: {err}"))?;
    let slice =
        crate::semantic_merge_lattice::semantic_slice_from_ref_view(&args.ref_view, args.selector);
    Ok(SemanticSliceBuildToolResultV1 {
        version: SEMANTIC_SLICE_BUILD_TOOL_VERSION,
        slice,
    })
}

pub(crate) fn call_semantic_slice_show(arguments: Value) -> Result<SemanticSliceShowToolResultV1> {
    let args: SemanticSliceShowArgs = serde_json::from_value(arguments)
        .map_err(|err| anyhow!("semantic_slice_show: invalid args: {err}"))?;
    let selected_ref_count = args.slice.selected_refs.len();
    Ok(SemanticSliceShowToolResultV1 {
        version: SEMANTIC_SLICE_SHOW_TOOL_VERSION,
        slice: args.slice,
        selected_ref_count,
    })
}

pub(crate) fn call_semantic_slice_diff(arguments: Value) -> Result<SemanticSliceDiffToolResultV1> {
    let args: SemanticSliceDiffArgs = serde_json::from_value(arguments)
        .map_err(|err| anyhow!("semantic_slice_diff: invalid args: {err}"))?;
    Ok(SemanticSliceDiffToolResultV1 {
        version: SEMANTIC_SLICE_DIFF_TOOL_VERSION,
        diff: crate::semantic_merge_lattice::semantic_slice_diff(&args.left, &args.right),
    })
}

pub(crate) fn call_semantic_merge_plan(arguments: Value) -> Result<SemanticMergePlanToolResultV1> {
    call_semantic_merge_or_rebase_plan(
        arguments,
        SEMANTIC_MERGE_PLAN_TOOL_VERSION,
        crate::semantic_merge_lattice::SemanticMergeOperationKindV1::Merge,
        "semantic_merge_plan",
    )
}

pub(crate) fn call_semantic_rebase_plan(arguments: Value) -> Result<SemanticMergePlanToolResultV1> {
    call_semantic_merge_or_rebase_plan(
        arguments,
        SEMANTIC_REBASE_PLAN_TOOL_VERSION,
        crate::semantic_merge_lattice::SemanticMergeOperationKindV1::Rebase,
        "semantic_rebase_plan",
    )
}

fn call_semantic_merge_or_rebase_plan(
    arguments: Value,
    version: &'static str,
    operation: crate::semantic_merge_lattice::SemanticMergeOperationKindV1,
    tool_name: &str,
) -> Result<SemanticMergePlanToolResultV1> {
    let args: SemanticMergePlanArgs = serde_json::from_value(arguments)
        .map_err(|err| anyhow!("{tool_name}: invalid args: {err}"))?;
    let merge_plan = crate::semantic_merge_lattice::semantic_merge_plan_from_dry_run(
        &args.dry_run,
        operation,
        args.source_selector,
        args.target_selector,
    );
    Ok(SemanticMergePlanToolResultV1 {
        version,
        merge_plan,
    })
}

pub(crate) fn call_semantic_resolver_steps(
    arguments: Value,
) -> Result<SemanticResolverStepsToolResultV1> {
    let args: SemanticResolverStepsArgs = serde_json::from_value(arguments)
        .map_err(|err| anyhow!("semantic_resolver_steps: invalid args: {err}"))?;
    Ok(SemanticResolverStepsToolResultV1 {
        version: SEMANTIC_RESOLVER_STEPS_TOOL_VERSION,
        report: crate::semantic_merge_lattice::resolver_steps_report(&args.merge_plan),
    })
}

pub(crate) fn call_semantic_theory_graph(
    arguments: Value,
) -> Result<SemanticTheoryGraphToolResultV1> {
    let args: SemanticTheoryGraphArgs = serde_json::from_value(arguments)
        .map_err(|err| anyhow!("semantic_theory_graph: invalid args: {err}"))?;
    let canonical = crate::axi_input::require_canonical_axi_text(&args.axi_text)
        .map_err(|err| anyhow!("semantic_theory_graph: expected canonical .axi text: {err}"))?;
    let kernel =
        axiograph_pathdb::compile_kernel_module_ir(canonical.module().module(), &args.axi_text)
            .map_err(|err| {
                anyhow!("semantic_theory_graph: failed to compile KernelModuleIr: {err}")
            })?;
    let mut graphs = kernel
        .theories
        .iter()
        .filter(|theory| {
            let Some(filter) = args.theory.as_deref() else {
                return true;
            };
            theory.theory_id.as_str() == filter
                || theory
                    .theory_id
                    .as_str()
                    .rsplit_once(':')
                    .is_some_and(|(_, local)| local == filter)
        })
        .map(axiograph_pathdb::kernel_ir::TheoryIr::obligation_graph)
        .collect::<Vec<_>>();
    graphs.sort_by(|a, b| a.theory_ref.stable_id().cmp(&b.theory_ref.stable_id()));
    if graphs.is_empty() {
        return Err(anyhow!(
            "semantic_theory_graph: no compiled theories matched{}",
            args.theory
                .as_ref()
                .map(|filter| format!(" `{filter}`"))
                .unwrap_or_default()
        ));
    }
    Ok(SemanticTheoryGraphToolResultV1 {
        version: SEMANTIC_THEORY_GRAPH_TOOL_VERSION,
        module_digest: canonical.digest().to_string(),
        graphs,
        trust_boundary: "rust_runtime_operational_not_lean_certificate".to_string(),
        completeness_claim: "not_claimed".to_string(),
        ontology_closure_claim: "not_claimed".to_string(),
        notes: vec![
            "semantic_theory_graph is a tool-loop surface over compiled TheoryIr; it is not a proof certificate".to_string(),
            "use graph nodes and edges as typed handles for exploration, CQ repair, migration, and reconciliation planning".to_string(),
        ],
    })
}

pub(crate) fn call_semantic_theory_check(
    arguments: Value,
) -> Result<SemanticTheoryCheckToolResultV1> {
    let args: SemanticTheoryCheckArgs = serde_json::from_value(arguments)
        .map_err(|err| anyhow!("semantic_theory_check: invalid args: {err}"))?;
    let closure_tier = crate::runtime_theory_check::parse_runtime_theory_closure_tier(
        args.closure_tier
            .as_deref()
            .unwrap_or("finite_fragment"),
    )
    .map_err(|err| anyhow!("semantic_theory_check: {err}"))?;
    let report = crate::runtime_theory_check::runtime_theory_check_reports_from_axi_text(
        &args.axi_text,
        args.theory.as_deref(),
        closure_tier,
    )
    .map_err(|err| anyhow!("semantic_theory_check: {err}"))?;
    Ok(SemanticTheoryCheckToolResultV1 {
        version: SEMANTIC_THEORY_CHECK_TOOL_VERSION,
        report,
    })
}

fn normalized_business_rule_scope(
    scope: crate::semantic_claim::RuntimeRuleScopeV1,
) -> Result<crate::semantic_claim::RuntimeRuleScopeV1> {
    let normalized = scope.normalized();
    match normalized.scope_class {
        crate::semantic_claim::RuntimeRuleScopeClassV1::Relation => {
            if normalized.relation.is_none() {
                return Err(anyhow!(
                    "semantic_business_rule: relation scope requires `scope.relation`"
                ));
            }
        }
        crate::semantic_claim::RuntimeRuleScopeClassV1::Theory => {
            if normalized.theory.is_none() {
                return Err(anyhow!(
                    "semantic_business_rule: theory scope requires `scope.theory`"
                ));
            }
        }
    }
    Ok(normalized)
}

fn semantic_scope_json_schema() -> Value {
    json!({
        "type": "object",
        "required": ["schema", "scope_class"],
        "properties": {
            "scope_id": { "type": "string" },
            "schema": { "type": "string" },
            "scope_class": { "type": "string", "enum": ["relation", "theory"] },
            "relation": { "type": "string" },
            "theory": { "type": "string" }
        }
    })
}

fn implementation_surface_json_schema() -> Value {
    let semantic_scope_schema = semantic_scope_json_schema();
    json!({
        "type": "object",
        "required": ["surface_id", "kind", "label"],
        "properties": {
            "surface_id": { "type": "string" },
            "kind": {
                "type": "string",
                "enum": ["endpoint", "workflow", "report", "job", "migration", "config", "doc_section", "agent_task"]
            },
            "label": { "type": "string" },
            "scopes": {
                "type": "array",
                "items": semantic_scope_schema
            },
            "code_refs": {
                "type": "array",
                "items": { "type": "string" }
            },
            "notes": {
                "type": "array",
                "items": { "type": "string" }
            }
        }
    })
}

fn coverage_edge_json_schema() -> Value {
    json!({
        "type": "object",
        "required": ["surface_id", "rule_id", "status"],
        "properties": {
            "surface_id": { "type": "string" },
            "rule_id": { "type": "string" },
            "status": {
                "type": "string",
                "enum": ["tested", "implemented", "documented_only", "ontology_only", "drifted", "unknown"]
            },
            "notes": {
                "type": "array",
                "items": { "type": "string" }
            }
        }
    })
}

fn bounded_context_json_schema() -> Value {
    let semantic_scope_schema = semantic_scope_json_schema();
    let implementation_surface_schema = implementation_surface_json_schema();
    let coverage_edge_schema = coverage_edge_json_schema();
    json!({
        "type": "object",
        "required": ["context_id", "label"],
        "properties": {
            "context_id": { "type": "string" },
            "label": { "type": "string" },
            "summary": { "type": "string" },
            "scopes": {
                "type": "array",
                "items": semantic_scope_schema
            },
            "surfaces": {
                "type": "array",
                "items": implementation_surface_schema
            },
            "edges": {
                "type": "array",
                "items": coverage_edge_schema
            },
            "competency_questions": {
                "type": "array",
                "items": { "type": "object" }
            },
            "notes": {
                "type": "array",
                "items": { "type": "string" }
            }
        }
    })
}

fn behavior_case_json_schema() -> Value {
    let bounded_context_schema = bounded_context_json_schema();
    let implementation_surface_schema = implementation_surface_json_schema();
    let coverage_edge_schema = coverage_edge_json_schema();
    let semantic_scope_schema = semantic_scope_json_schema();
    json!({
        "type": "object",
        "required": ["case_id", "title", "context"],
        "properties": {
            "case_id": { "type": "string" },
            "title": { "type": "string" },
            "summary": { "type": "string" },
            "context": bounded_context_schema,
            "given": { "type": "object" },
            "when": { "type": "object" },
            "then": {
                "type": "object",
                "properties": {
                    "expected_outcomes": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "competency_questions": {
                        "type": "array",
                        "items": { "type": "object" }
                    },
                    "rule_scopes": {
                        "type": "array",
                        "items": semantic_scope_schema
                    },
                    "trust_target": {
                        "type": "string",
                        "enum": ["strong", "weak", "unknown", "conflicted"]
                    }
                }
            },
            "implementation_surfaces": {
                "type": "array",
                "items": implementation_surface_schema
            },
            "coverage_edges": {
                "type": "array",
                "items": coverage_edge_schema
            },
            "notes": {
                "type": "array",
                "items": { "type": "string" }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_db_and_meta() -> Result<(PathDB, MetaPlaneIndex)> {
        let axi = r#"
module Family

schema Family:
  object Person
  relation parent(child: Person, parent: Person)

theory FamilyTheory on Family:
  constraint functional parent.child -> parent.parent

instance FamilyInst of Family:
  Person = {Alice, Bob}
  parent = {(child=Bob, parent=Alice)}
"#;
        let mut db = PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();
        let meta = MetaPlaneIndex::from_db(&db)?;
        Ok((db, meta))
    }

    #[test]
    fn semantic_business_rule_dispatch_normalizes_relation_scope() -> Result<()> {
        let (db, meta) = sample_db_and_meta()?;
        let accepted = AcceptedSnapshotId::new("accepted:family");
        let out = call_semantic_business_rule(
            SemanticToolContext {
                db: &db,
                meta: Some(&meta),
                accepted_snapshot_id: Some(&accepted),
            },
            json!({
                "scope": {
                    "schema": "Family",
                    "scope_class": "relation",
                    "relation": "parent"
                }
            }),
        )?;

        assert_eq!(out.version, SEMANTIC_BUSINESS_RULE_TOOL_VERSION);
        assert_eq!(out.accepted_snapshot_id, Some(accepted));
        assert_eq!(out.report.scope.scope_id, "schema/family/relation/parent");
        Ok(())
    }

    #[test]
    fn semantic_context_report_dispatch_returns_composed_bounded_context_report() -> Result<()> {
        let (db, meta) = sample_db_and_meta()?;
        let accepted = AcceptedSnapshotId::new("accepted:family");
        let out = call_semantic_context_report(
            SemanticToolContext {
                db: &db,
                meta: Some(&meta),
                accepted_snapshot_id: Some(&accepted),
            },
            json!({
                "context": {
                    "context_id": "domain:family_lookup",
                    "label": "Family lookup",
                    "scopes": [{
                        "schema": "Family",
                        "scope_class": "relation",
                        "relation": "parent"
                    }],
                    "surfaces": [{
                        "surface_id": "endpoint:family_lookup",
                        "kind": "endpoint",
                        "label": "GET /family/lookup",
                        "scopes": [{
                            "schema": "Family",
                            "scope_class": "relation",
                            "relation": "parent"
                        }]
                    }],
                    "edges": [{
                        "surface_id": "endpoint:family_lookup",
                        "rule_id": "schema/family/relation/parent/rule/functional/0",
                        "status": "tested"
                    }],
                    "competency_questions": [{
                        "name": "family_lookup_returns_bob",
                        "query": "select ?f where ?f = Family.parent(child=Bob, parent=Alice) limit 1",
                        "min_rows": 1,
                        "weight": 1.0
                    }]
                }
            }),
        )?;

        assert_eq!(out.version, SEMANTIC_CONTEXT_REPORT_TOOL_VERSION);
        assert_eq!(out.accepted_snapshot_id, Some(accepted));
        assert_eq!(
            out.report.context.context_id.as_str(),
            "domain:family_lookup"
        );
        assert_eq!(out.report.coverage.covered_rules, 1);
        assert_eq!(
            out.report
                .competency_coverage
                .as_ref()
                .map(|coverage| coverage.total),
            Some(1)
        );
        Ok(())
    }

    #[test]
    fn semantic_context_map_tool_returns_merge_ready_selectors() -> Result<()> {
        let source_context = crate::context_report::BoundedContextV1 {
            context_id: crate::context_report::DomainContextId::new("domain:family_read"),
            label: "Family read".to_string(),
            summary: None,
            scopes: vec![crate::semantic_claim::RuntimeRuleScopeV1::relation(
                "Family", "parent",
            )],
            surfaces: vec![crate::semantic_claim::ImplementationSurfaceRefV1 {
                surface_id: "endpoint:family_lookup".to_string(),
                kind: crate::semantic_claim::ImplementationSurfaceKindV1::Endpoint,
                label: "GET /family/lookup".to_string(),
                scopes: vec![crate::semantic_claim::RuntimeRuleScopeV1::relation(
                    "Family", "parent",
                )],
                code_refs: Vec::new(),
                notes: Vec::new(),
            }],
            edges: Vec::new(),
            competency_questions: Vec::new(),
            notes: Vec::new(),
        };
        let mut target_context = source_context.clone();
        target_context.context_id =
            crate::context_report::DomainContextId::new("domain:family_write");
        target_context.label = "Family write".to_string();
        target_context.surfaces[0].surface_id = "workflow:update_family".to_string();
        target_context.surfaces[0].kind =
            crate::semantic_claim::ImplementationSurfaceKindV1::Workflow;

        let out = call_semantic_context_map(json!({
            "source": source_context,
            "target": target_context,
            "relationship": "shared_kernel"
        }))?;

        assert_eq!(out.version, SEMANTIC_CONTEXT_MAP_TOOL_VERSION);
        assert_eq!(
            out.context_map.merge_policy_hint,
            "require_explicit_shared_kernel_review"
        );
        assert!(out
            .context_map
            .typed_overlap_refs
            .iter()
            .any(|reference| reference == "relation:Family:parent"));
        assert!(is_semantic_tool(SEMANTIC_CONTEXT_MAP_TOOL_NAME));
        Ok(())
    }

    #[test]
    fn semantic_theory_graph_tool_returns_runtime_obligation_graphs() -> Result<()> {
        let axi = r#"
module Family

schema Family:
  object Person
  relation parent(child: Person, parent: Person)

theory FamilyTheory on Family:
  constraint functional parent.child -> parent.parent

instance FamilyInst of Family:
  Person = {Alice, Bob}
  parent = {(child=Bob, parent=Alice)}
"#;
        let out = call_semantic_theory_graph(json!({
            "axi_text": axi,
            "theory": "FamilyTheory"
        }))?;

        assert_eq!(out.version, SEMANTIC_THEORY_GRAPH_TOOL_VERSION);
        assert_eq!(out.graphs.len(), 1);
        assert!(out.graphs[0].nodes.iter().any(|node| {
            node.kind == axiograph_pathdb::kernel_ir::TheoryObligationGraphNodeKindV1::Obligation
        }));
        assert!(out.graphs[0].edges.iter().any(|edge| {
            edge.kind
                == axiograph_pathdb::kernel_ir::TheoryObligationGraphEdgeKindV1::SubjectSupportsObligation
        }));
        assert_eq!(out.completeness_claim, "not_claimed");
        assert!(is_semantic_tool(SEMANTIC_THEORY_GRAPH_TOOL_NAME));
        Ok(())
    }

    #[test]
    fn semantic_theory_check_tool_returns_runtime_closure_report() -> Result<()> {
        let axi = r#"
module Family

schema Family:
  object Person
  relation parent(child: Person, parent: Person)

theory FamilyTheory on Family:
  constraint functional parent.child -> parent.parent
"#;
        let out = call_semantic_theory_check(json!({
            "axi_text": axi,
            "theory": "FamilyTheory",
            "closure_tier": "finite_fragment"
        }))?;

        assert_eq!(out.version, SEMANTIC_THEORY_CHECK_TOOL_VERSION);
        assert_eq!(out.report.reports.len(), 1);
        assert_eq!(out.report.blocking_errors, 0);
        assert_eq!(out.report.reports[0].checked_obligations, 1);
        assert!(out.report.reports[0].closure.complete);
        assert!(is_semantic_tool(SEMANTIC_THEORY_CHECK_TOOL_NAME));
        Ok(())
    }

    #[test]
    fn semantic_behavior_case_dispatch_returns_receipt_and_test_skeletons() -> Result<()> {
        let (db, meta) = sample_db_and_meta()?;
        let accepted = AcceptedSnapshotId::new("accepted:family");
        let out = call_semantic_behavior_case(
            SemanticToolContext {
                db: &db,
                meta: Some(&meta),
                accepted_snapshot_id: Some(&accepted),
            },
            json!({
                "behavior_case": {
                    "case_id": "family.parent_lookup",
                    "title": "Family lookup returns parent",
                    "context": {
                        "context_id": "domain:family_lookup",
                        "label": "Family lookup",
                        "scopes": [{
                            "schema": "Family",
                            "scope_class": "relation",
                            "relation": "parent"
                        }],
                        "surfaces": [{
                            "surface_id": "endpoint:family_lookup",
                            "kind": "endpoint",
                            "label": "GET /family/lookup",
                            "scopes": [{
                                "schema": "Family",
                                "scope_class": "relation",
                                "relation": "parent"
                            }]
                        }],
                        "edges": [{
                            "surface_id": "endpoint:family_lookup",
                            "rule_id": "schema/family/relation/parent/rule/functional/0",
                            "status": "tested"
                        }]
                    },
                    "then": {
                        "expected_outcomes": ["Bob has Alice as parent"],
                        "competency_questions": [{
                            "name": "family_lookup_returns_alice",
                            "query": "select ?f where ?f = Family.parent(child=Bob, parent=Alice) limit 1",
                            "min_rows": 1,
                            "weight": 1.0
                        }],
                        "rule_scopes": [{
                            "schema": "Family",
                            "scope_class": "relation",
                            "relation": "parent"
                        }],
                        "trust_target": "strong"
                    }
                }
            }),
        )?;

        assert_eq!(out.version, SEMANTIC_BEHAVIOR_CASE_TOOL_VERSION);
        assert_eq!(out.accepted_snapshot_id, Some(accepted));
        assert_eq!(out.report.receipt.case_id.as_str(), "family.parent_lookup");
        assert_eq!(out.report.receipt.competency_total, 1);
        assert_eq!(out.report.codegen_previews.len(), 2);
        assert!(out
            .report
            .codegen_previews
            .iter()
            .any(|preview| preview.content.contains("BehaviorCaseV1")));
        Ok(())
    }

    #[test]
    fn semantic_tool_specs_expose_all_semantic_report_tools() {
        let specs = semantic_tool_specs();
        let names = specs
            .iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                SEMANTIC_BUSINESS_RULE_TOOL_NAME,
                SEMANTIC_COVERAGE_TOOL_NAME,
                SEMANTIC_AGENT_REPORT_TOOL_NAME,
                SEMANTIC_CONTEXT_REPORT_TOOL_NAME,
                SEMANTIC_CONTEXT_MAP_TOOL_NAME,
                SEMANTIC_BEHAVIOR_CASE_TOOL_NAME,
                SEMANTIC_SLICE_BUILD_TOOL_NAME,
                SEMANTIC_SLICE_SHOW_TOOL_NAME,
                SEMANTIC_SLICE_DIFF_TOOL_NAME,
                SEMANTIC_MERGE_PLAN_TOOL_NAME,
                SEMANTIC_REBASE_PLAN_TOOL_NAME,
                SEMANTIC_RESOLVER_STEPS_TOOL_NAME,
                SEMANTIC_THEORY_GRAPH_TOOL_NAME,
                SEMANTIC_THEORY_CHECK_TOOL_NAME,
            ]
        );
        for tool_name in [
            SEMANTIC_COVERAGE_TOOL_NAME,
            SEMANTIC_AGENT_REPORT_TOOL_NAME,
            SEMANTIC_CONTEXT_REPORT_TOOL_NAME,
            SEMANTIC_BEHAVIOR_CASE_TOOL_NAME,
        ] {
            let schema = &specs
                .iter()
                .find(|tool| tool.name == tool_name)
                .expect("semantic report tool")
                .input_schema;
            assert!(schema["properties"]["runtime_theory_check"].is_object());
            assert!(schema["properties"]["runtime_theory_check_input"].is_object());
        }
    }
}
