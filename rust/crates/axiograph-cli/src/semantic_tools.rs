use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use axiograph_pathdb::axi_semantics::MetaPlaneIndex;
use axiograph_pathdb::{AcceptedSnapshotId, PathDB};

pub(crate) const SEMANTIC_BUSINESS_RULE_TOOL_NAME: &str = "semantic_business_rule";
pub(crate) const SEMANTIC_COVERAGE_TOOL_NAME: &str = "semantic_coverage";
pub(crate) const SEMANTIC_AGENT_REPORT_TOOL_NAME: &str = "semantic_agent_report";

const SEMANTIC_BUSINESS_RULE_TOOL_VERSION: &str = "axiograph_semantic_business_rule_v1";
const SEMANTIC_COVERAGE_TOOL_VERSION: &str = "axiograph_semantic_coverage_v1";
const SEMANTIC_AGENT_REPORT_TOOL_VERSION: &str = "axiograph_semantic_agent_report_v1";

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
                    }
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

pub(crate) fn call_semantic_coverage(
    context: SemanticToolContext<'_>,
    arguments: Value,
) -> Result<SemanticCoverageToolResultV1> {
    let args: SemanticCoverageArgs = serde_json::from_value(arguments)
        .map_err(|err| anyhow!("semantic_coverage: invalid args: {err}"))?;
    let accepted_snapshot_id = context.accepted_snapshot_id();
    let lifecycle_state = context.lifecycle_state(args.lifecycle_state);
    let meta = context.meta_plane()?;
    let coverage = crate::semantic_claim::semantic_coverage_report(
        &meta,
        accepted_snapshot_id.clone(),
        &lifecycle_state,
        &args.surfaces,
        &args.edges,
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
    let report = crate::semantic_claim::agent_engineering_report(
        &meta,
        accepted_snapshot_id.clone(),
        &lifecycle_state,
        &args.task,
        &args.surfaces,
        &args.edges,
    );

    Ok(SemanticAgentReportToolResultV1 {
        version: SEMANTIC_AGENT_REPORT_TOOL_VERSION,
        accepted_snapshot_id,
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
  constraint functional parent on child

instance FamilyInst of Family:
  Person = {alice, bob}
  parent = {(child=bob, parent=alice)}
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
    fn semantic_tool_specs_expose_all_semantic_report_tools() {
        let names = semantic_tool_specs()
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                SEMANTIC_BUSINESS_RULE_TOOL_NAME,
                SEMANTIC_COVERAGE_TOOL_NAME,
                SEMANTIC_AGENT_REPORT_TOOL_NAME,
            ]
        );
    }
}
