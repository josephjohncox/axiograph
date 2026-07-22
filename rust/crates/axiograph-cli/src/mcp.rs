use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ErrorData, Implementation, JsonObject, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServiceExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use axiograph_pathdb::PathDB;

#[cfg(test)]
const JSONRPC_VERSION: &str = "2.0";
#[cfg(test)]
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

pub(crate) fn cmd_mcp(args: crate::McpArgs) -> Result<()> {
    if args.tool_max_rows == 0 || args.tool_max_rows > 200 {
        return Err(anyhow!("--tool-max-rows must be in 1..=200"));
    }
    if args.verify_timeout_secs == 0
        || args.verify_timeout_secs > crate::security::MAX_CHILD_RUNTIME.as_secs()
    {
        return Err(anyhow!(
            "--verify-timeout-secs must be in 1..={}",
            crate::security::MAX_CHILD_RUNTIME.as_secs()
        ));
    }
    let runtime =
        crate::db_server::load_read_only_semantic_runtime(&args.dir, &args.materialization)?;

    let server = SemanticMcpServer {
        runtime,
        tool_max_rows: args.tool_max_rows,
        cert_verify: crate::verifier_bridge::CertVerifyConfig {
            verifier_bin: args.verify_bin,
            timeout: Some(Duration::from_secs(args.verify_timeout_secs)),
            approved_checker_sha256: args.verify_sha256,
            approved_checker_build_id: args.verify_build_id,
        },
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime for rmcp semantic MCP server")?;
    runtime.block_on(run_rmcp_mcp_stdio(server))
}

async fn run_rmcp_mcp_stdio(server: SemanticMcpServer) -> Result<()> {
    let transport = rmcp::transport::async_rw::AsyncRwTransport::<RoleServer, _, _>::new_server(
        crate::authoring_workspace::BoundedJsonLineReader::new(tokio::io::stdin()),
        crate::authoring_workspace::BoundedLineWriter::new(tokio::io::stdout()),
    );
    let service = SemanticRmcpServer { server }
        .serve(transport)
        .await
        .context("serve bounded Axiograph semantic MCP stdio transport")?;
    service
        .waiting()
        .await
        .context("wait for Axiograph semantic MCP server shutdown")?;
    Ok(())
}

struct SemanticMcpServer {
    runtime: crate::db_server::ReadOnlySemanticRuntime,
    tool_max_rows: usize,
    cert_verify: crate::verifier_bridge::CertVerifyConfig,
}

struct SemanticRmcpServer {
    server: SemanticMcpServer,
}

impl rmcp::handler::server::ServerHandler for SemanticRmcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("axiograph-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Read-only typed semantic MCP surface over Axiograph query preparation/typechecking, exploration, execution, and semantic rule reporting.",
            )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, ErrorData> {
        let tools = self
            .server
            .tool_definitions()
            .into_iter()
            .map(rmcp_tool_from_spec_value)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let name = request.name.to_string();
        let arguments = Value::Object(request.arguments.unwrap_or_default());
        match self.server.call_tool(&name, arguments) {
            Ok(structured) => Ok(rmcp_tool_result(structured, false)),
            Err(err) => Ok(rmcp_tool_result(json!({ "error": err.to_string() }), true)),
        }
    }
}

impl SemanticMcpServer {
    #[cfg(test)]
    fn handle_message(&mut self, message: Value) -> Option<Value> {
        let id = message.get("id").cloned();
        let method = message.get("method").and_then(Value::as_str)?;
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));

        match method {
            "initialize" => Some(success_response(
                id.unwrap_or(Value::Null),
                json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {
                        "tools": {
                            "listChanged": false
                        }
                    },
                    "serverInfo": {
                        "name": "axiograph-mcp",
                        "version": env!("CARGO_PKG_VERSION")
                    },
                    "instructions": "Read-only typed semantic MCP surface over Axiograph query preparation/typechecking, exploration, execution, and semantic rule reporting."
                }),
            )),
            "notifications/initialized" => None,
            "ping" => Some(success_response(id.unwrap_or(Value::Null), json!({}))),
            "tools/list" => Some(success_response(
                id.unwrap_or(Value::Null),
                json!({ "tools": self.tool_definitions() }),
            )),
            "tools/call" => {
                let request_id = id.unwrap_or(Value::Null);
                let params = match serde_json::from_value::<ToolCallParams>(params) {
                    Ok(params) => params,
                    Err(err) => {
                        return Some(error_response(
                            request_id,
                            -32602,
                            &format!("invalid tools/call params: {err}"),
                        ));
                    }
                };

                match self.call_tool(&params.name, params.arguments.unwrap_or_else(|| json!({}))) {
                    Ok(structured) => Some(success_response(
                        request_id,
                        json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": pretty_json(&structured)
                                }
                            ],
                            "structuredContent": structured,
                            "isError": false
                        }),
                    )),
                    Err(err) => Some(success_response(
                        request_id,
                        json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": format!("tool `{}` failed: {err}", params.name)
                                }
                            ],
                            "structuredContent": {
                                "error": err.to_string()
                            },
                            "isError": true
                        }),
                    )),
                }
            }
            _ => id.map(|request_id| {
                error_response(
                    request_id,
                    -32601,
                    &format!("unsupported MCP method `{method}`"),
                )
            }),
        }
    }

    fn tool_definitions(&self) -> Vec<Value> {
        let query_ir_v1_schema = crate::query_ir::query_ir_v1_json_schema();
        let mut tools = vec![
            json!({
                "name": "axql_elaborate",
                "description": "Prepare and typecheck a structured `query_ir_v1` query, returning the executable query, inferred types, missing typed pieces, exploration suggestions, plan, and trust metadata.",
                "inputSchema": {
                    "type": "object",
                    "required": ["query_ir_v1"],
                    "properties": {
                        "query_ir_v1": query_ir_v1_schema.clone(),
                        "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
                    }
                },
                "annotations": { "readOnlyHint": true }
            }),
            json!({
                "name": "axql_explore",
                "description": "Ask what can go here next for a structured `query_ir_v1` query, returning missing typed pieces, refinement candidates, semantic claims, and trust gaps.",
                "inputSchema": {
                    "type": "object",
                    "required": ["query_ir_v1"],
                    "properties": {
                        "query_ir_v1": query_ir_v1_schema.clone(),
                        "variable": { "type": "string" }
                    }
                },
                "annotations": { "readOnlyHint": true }
            }),
            json!({
                "name": "axql_run",
                "description": "Compile and run structured `query_ir_v1` through `CompiledFiniteQuery`, returning rows and shared trust metadata. With exact accepted module bytes and approved verifier settings, `certificate_policy=require_verified` returns only a bound `query_result_v4` Lean receipt and authoritative verified rows.",
                "inputSchema": {
                    "type": "object",
                    "required": ["query_ir_v1"],
                    "properties": {
                        "query_ir_v1": query_ir_v1_schema,
                        "limit": { "type": "integer", "minimum": 1, "maximum": 200 },
                        "certificate_policy": {
                            "type": "string",
                            "enum": ["none", "emit", "verify", "require_verified"]
                        }
                    }
                },
                "annotations": { "readOnlyHint": true }
            }),
        ];
        tools.extend(
            crate::semantic_tools::semantic_tool_specs()
                .into_iter()
                .map(|tool| {
                    json!({
                        "name": tool.name,
                        "description": tool.description,
                        "inputSchema": tool.input_schema,
                        "annotations": { "readOnlyHint": true }
                    })
                }),
        );
        tools.extend(
            crate::route_preview_tools::route_preview_tool_specs()
                .into_iter()
                .map(|tool| {
                    json!({
                        "name": tool.name,
                        "description": tool.description,
                        "inputSchema": tool.input_schema,
                        "annotations": { "readOnlyHint": true }
                    })
                }),
        );
        tools.extend(
            crate::transport_preview_tools::transport_preview_tool_specs()
                .into_iter()
                .map(|tool| {
                    json!({
                        "name": tool.name,
                        "description": tool.description,
                        "inputSchema": tool.input_schema,
                        "annotations": { "readOnlyHint": true }
                    })
                }),
        );
        tools
    }

    fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        match name {
            "axql_elaborate" => self.call_axql_elaborate(arguments),
            "axql_explore" => self.call_axql_explore(arguments),
            "axql_run" => self.call_axql_run(arguments),
            name if crate::semantic_tools::is_semantic_tool(name) => {
                crate::semantic_tools::invoke_semantic_tool(
                    name,
                    crate::semantic_tools::SemanticToolContext {
                        db: self.runtime.db.as_ref(),
                        meta: self.runtime.meta.as_ref(),
                        accepted_snapshot_id: self.runtime.accepted_snapshot_id.as_ref(),
                    },
                    arguments,
                )
            }
            name if crate::route_preview_tools::is_route_preview_tool(name) => {
                crate::route_preview_tools::invoke_route_preview_tool(
                    name,
                    crate::route_preview_tools::RoutePreviewToolContext {
                        db: self.runtime.db.as_ref(),
                    },
                    arguments,
                )
            }
            name if crate::transport_preview_tools::is_transport_preview_tool(name) => {
                crate::transport_preview_tools::invoke_transport_preview_tool(
                    name,
                    crate::transport_preview_tools::TransportPreviewToolContext,
                    arguments,
                )
            }
            other => Err(anyhow!("unknown read-only MCP tool `{other}`")),
        }
    }

    fn call_axql_elaborate(&self, arguments: Value) -> Result<Value> {
        let args: QueryToolArgs = serde_json::from_value(arguments)
            .map_err(|err| anyhow!("axql_elaborate: invalid args: {err}"))?;
        let prepared = self.prepare_query(with_bounded_limit(
            args.query_ir_v1,
            args.limit,
            self.tool_max_rows,
        ))?;
        let report = prepared.elaboration_report();
        let trust = prepared.trust_contract_with_meta(self.runtime.meta.as_ref());

        Ok(json!({
            "elaborated": prepared.elaborated_query_text(),
            "inferred_types": report.inferred_types.clone(),
            "notes": report.notes.clone(),
            "typed_holes": report.typed_holes.clone(),
            "exploration_suggestions": report.exploration_suggestions.clone(),
            "plan": prepared.explain_plan_lines(),
            "trust": trust
        }))
    }

    fn call_axql_explore(&self, arguments: Value) -> Result<Value> {
        let args: ExploreToolArgs = serde_json::from_value(arguments)
            .map_err(|err| anyhow!("axql_explore: invalid args: {err}"))?;
        let prepared = self.prepare_query(args.query_ir_v1)?;
        let focus = args
            .variable
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        Ok(json!({
            "elaborated": prepared.elaborated_query_text(),
            "elaborated_query_ir_v1": prepared.elaborated_query_ir_v1()?,
            "focus_variable": focus,
            "exploration": prepared.exploration_view(focus.as_deref()),
            "trust": prepared.trust_contract_with_meta(self.runtime.meta.as_ref())
        }))
    }

    fn call_axql_run(&self, arguments: Value) -> Result<Value> {
        let args: QueryToolArgs = serde_json::from_value(arguments)
            .map_err(|err| anyhow!("axql_run: invalid args: {err}"))?;
        let certificate_policy = args.certificate_policy;
        let mut prepared = self.prepare_query(with_bounded_limit(
            args.query_ir_v1,
            args.limit,
            self.tool_max_rows,
        ))?;
        let query = prepared.query_ir_v1().to_axql_text()?;
        let elaborated = prepared.elaborated_query_text();
        let report = prepared.elaboration_report().clone();
        let certifiability = prepared.certifiability();
        certificate_policy.ensure_require_verified_preconditions(
            &certifiability,
            self.runtime.accepted_axi_anchor.as_ref(),
            self.runtime.accepted_axi_text.as_deref(),
        )?;
        let prepared_query = certifiability
            .is_certifiable()
            .then(|| prepared.metadata_v2_with_meta(self.runtime.meta.as_ref()))
            .transpose()?;

        let validated =
            prepared.execute_answer(self.runtime.db.as_ref(), self.runtime.meta.as_ref())?;
        let result = validated.result().clone();
        let mut certificate = None;
        let mut certificate_verified = None;
        let mut verifier_receipt_v2 = None;
        let mut verified_rows = None;
        let mut answer_digest_v1 = None;

        if certificate_policy.emits_certificate() {
            let (_revision_digest_v2, axi_text) = match (
                self.runtime.accepted_axi_anchor.as_ref(),
                self.runtime.accepted_axi_text.as_deref(),
            ) {
                (Some(anchor), Some(text)) if !text.trim().is_empty() => {
                    let recomputed = axiograph_pathdb::AxiDigest::from_axi_text(text);
                    if recomputed != anchor.axi_digest {
                        return Err(anyhow!(
                            "accepted .axi source digest differs from MCP runtime anchor"
                        ));
                    }
                    (anchor.axi_digest.clone(), text.to_string())
                }
                (Some(_), _) => {
                    return Err(anyhow!(
                        "MCP certificate emission requires exact accepted .axi source text"
                    ))
                }
                (None, _) => {
                    crate::db_server::export_canonical_module_axi(self.runtime.db.as_ref())?
                }
            };
            let emitted = prepared.certify_answer_with_anchors(
                validated,
                self.runtime.db.as_ref(),
                self.runtime.meta.as_ref(),
                axiograph_pathdb::RevisionDigestV2::from_accepted_text(&axi_text),
            )?;
            let cert = emitted.certificate().clone();
            answer_digest_v1 = Some(emitted.answer_digest_v1().clone());
            certificate = Some(serde_json::to_value(&cert)?);

            if certificate_policy.verifies_certificate() {
                let receipt = crate::verifier_bridge::verify_certificate_with_lean(
                    &self.cert_verify,
                    &axi_text,
                    emitted.certificate_text(),
                    emitted
                        .prepared_query_digest_v1()
                        .ok_or_else(|| anyhow!("emitted MCP answer lost prepared digest"))?,
                    emitted.answer_digest_v1(),
                )?;
                let accepted = receipt.accepted();
                certificate_verified = Some(accepted);
                if accepted {
                    verified_rows = Some(
                        emitted
                            .into_lean_verified(receipt.clone())?
                            .selected_rows_v1()
                            .to_vec(),
                    );
                }
                verifier_receipt_v2 = Some(receipt);
                certificate_policy.ensure_verified_result(certificate_verified)?;
            }
        }

        let trust = crate::trust_contract::query_user_visible_trust_contract_with_meta(
            prepared.as_query(),
            &certifiability,
            certificate.is_some(),
            certificate_verified,
            self.runtime.meta.as_ref(),
        );
        let out = json!({
            "query": query,
            "elaborated": elaborated,
            "inferred_types": report.inferred_types,
            "notes": report.notes,
            "typed_holes": report.typed_holes,
            "exploration_suggestions": report.exploration_suggestions,
            "plan": prepared.explain_plan_lines(),
            "trust": trust,
            "prepared_query": prepared_query,
            "certificate_policy": certificate_policy,
            "certificate": certificate,
            "certificate_verified": certificate_verified,
            "verifier_receipt_v2": verifier_receipt_v2,
            "verified_rows": verified_rows,
            "answer_digest_v1": answer_digest_v1,
            "entity_view_enrichment_claim": "unbound_runtime_entity_view_no_verified_row_claim",
            "results": ToolQueryResultsV1::from_axql_result(self.runtime.db.as_ref(), &result)
        });
        Ok(out)
    }

    fn prepare_query(
        &self,
        query_ir_v1: crate::query_ir::QueryIrV1,
    ) -> Result<crate::query_ir::CompiledFiniteQuery> {
        query_ir_v1.compile_with_meta(self.runtime.db.as_ref(), self.runtime.meta.as_ref())
    }
}

fn rmcp_tool_from_spec_value(value: Value) -> std::result::Result<Tool, ErrorData> {
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ErrorData::internal_error("MCP tool spec is missing name", Some(value.clone()))
        })?
        .to_string();
    let description = value
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("Axiograph semantic MCP tool.")
        .to_string();
    let input_schema = value
        .get("inputSchema")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_else(default_object_schema);

    Ok(Tool::new(name, description, Arc::new(input_schema))
        .with_raw_output_schema(Arc::new(default_object_schema()))
        .with_annotations(ToolAnnotations::new().read_only(true).destructive(false)))
}

fn rmcp_tool_result(structured: Value, is_error: bool) -> CallToolResult {
    if is_error {
        CallToolResult::structured_error(structured)
    } else {
        CallToolResult::structured(structured)
    }
}

fn default_object_schema() -> JsonObject {
    match json!({
        "type": "object",
        "additionalProperties": true
    }) {
        Value::Object(map) => map,
        _ => JsonObject::default(),
    }
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
struct ToolCallParams {
    name: String,
    #[serde(default)]
    arguments: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct QueryToolArgs {
    query_ir_v1: crate::query_ir::QueryIrV1,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    certificate_policy: crate::query_ir::QueryCertificatePolicyV1,
}

#[derive(Debug, Deserialize)]
struct ExploreToolArgs {
    query_ir_v1: crate::query_ir::QueryIrV1,
    #[serde(default)]
    variable: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolQueryResultsV1 {
    vars: Vec<String>,
    rows: Vec<BTreeMap<String, EntityViewV1>>,
    truncated: bool,
}

impl ToolQueryResultsV1 {
    fn from_axql_result(db: &PathDB, result: &crate::axql::AxqlResult) -> Self {
        let mut rows = Vec::new();
        for row in &result.rows {
            let mut out = BTreeMap::new();
            for (var, id) in row {
                out.insert(var.clone(), EntityViewV1::from_id(db, *id));
            }
            rows.push(out);
        }
        Self {
            vars: result.selected_vars.clone(),
            rows,
            truncated: result.truncated,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EntityViewV1 {
    id: u32,
    entity_type: Option<String>,
    name: Option<String>,
}

impl EntityViewV1 {
    fn from_id(db: &PathDB, id: u32) -> Self {
        let Some(view) = db.get_entity(id) else {
            return Self {
                id,
                entity_type: None,
                name: None,
            };
        };
        Self {
            id,
            entity_type: Some(view.entity_type),
            name: view.attrs.get("name").cloned(),
        }
    }
}

fn with_bounded_limit(
    mut query_ir_v1: crate::query_ir::QueryIrV1,
    limit: Option<usize>,
    tool_max_rows: usize,
) -> crate::query_ir::QueryIrV1 {
    if let Some(limit) = limit {
        query_ir_v1.limit = Some(limit.clamp(1, tool_max_rows));
    } else if let Some(existing) = query_ir_v1.limit {
        query_ir_v1.limit = Some(existing.clamp(1, tool_max_rows));
    }
    query_ir_v1
}

#[cfg(test)]
fn success_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "result": result
    })
}

#[cfg(test)]
fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

#[cfg(test)]
fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use axiograph_pathdb::{AcceptedAxiAnchor, AcceptedSnapshotId, AxiDigest};

    fn disabled_verifier() -> crate::verifier_bridge::CertVerifyConfig {
        crate::verifier_bridge::CertVerifyConfig {
            verifier_bin: None,
            timeout: None,
            approved_checker_sha256: None,
            approved_checker_build_id: None,
        }
    }

    fn test_server(accepted_axi_anchor: Option<AcceptedAxiAnchor>) -> SemanticMcpServer {
        SemanticMcpServer {
            runtime: crate::db_server::ReadOnlySemanticRuntime {
                accepted_snapshot_id: None,
                accepted_axi_anchor,
                accepted_axi_text: None,
                db: Arc::new(PathDB::new()),
                meta: None,
            },
            tool_max_rows: 25,
            cert_verify: disabled_verifier(),
        }
    }

    fn test_server_with_semantic_family_runtime() -> SemanticMcpServer {
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
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)
            .expect("import semantic family module");
        db.build_indexes();
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)
            .expect("semantic family meta plane");
        SemanticMcpServer {
            runtime: crate::db_server::ReadOnlySemanticRuntime {
                accepted_snapshot_id: Some(AcceptedSnapshotId::new("accepted:family")),
                accepted_axi_anchor: None,
                accepted_axi_text: Some(axi.to_string()),
                db: Arc::new(db),
                meta: Some(meta),
            },
            tool_max_rows: 25,
            cert_verify: disabled_verifier(),
        }
    }

    fn test_server_with_route_runtime() -> (SemanticMcpServer, u32, u32, u32) {
        let mut db = PathDB::new();
        let a = db.add_entity("Node", vec![("name", "A")]);
        let b = db.add_entity("Node", vec![("name", "B")]);
        let ab = db.add_relation("road", a, b, 0.9, Vec::new());
        db.build_indexes();
        (
            SemanticMcpServer {
                runtime: crate::db_server::ReadOnlySemanticRuntime {
                    accepted_snapshot_id: None,
                    accepted_axi_anchor: None,
                    accepted_axi_text: None,
                    db: Arc::new(db),
                    meta: None,
                },
                tool_max_rows: 25,
                cert_verify: disabled_verifier(),
            },
            a,
            b,
            ab,
        )
    }

    #[test]
    fn tool_definitions_include_shared_semantic_report_tools() {
        let server = test_server(None);

        let names = server
            .tool_definitions()
            .into_iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_string))
            .collect::<Vec<_>>();

        assert!(names.iter().any(|name| name == "semantic_business_rule"));
        assert!(names.iter().any(|name| name == "semantic_coverage"));
        assert!(names.iter().any(|name| name == "semantic_agent_report"));
        assert!(names.iter().any(|name| name == "semantic_context_report"));
        assert!(names.iter().any(|name| name == "semantic_behavior_case"));
        assert!(names.iter().any(|name| name == "semantic_slice_build"));
        assert!(names.iter().any(|name| name == "semantic_slice_show"));
        assert!(names.iter().any(|name| name == "semantic_slice_diff"));
        assert!(names.iter().any(|name| name == "semantic_merge_plan"));
        assert!(names.iter().any(|name| name == "semantic_rebase_plan"));
        assert!(names.iter().any(|name| name == "semantic_resolver_steps"));
        assert!(names
            .iter()
            .any(|name| name == crate::route_preview_tools::ROUTE_PREVIEW_TOOL_NAME));
        assert!(names
            .iter()
            .any(|name| name == crate::transport_preview_tools::TRANSPORT_PREVIEW_TOOL_NAME));
    }

    fn sample_transport_preview_arguments() -> Value {
        json!({
            "axi_text": r#"
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
            "schema": "Plant",
            "morphism": {
                "source_schema": "Plant",
                "target_schema": "Ops",
                "objects": [
                    {"source_object": "PlantAsset", "target_object": "Equipment"},
                    {"source_object": "Pump", "target_object": "Equipment"},
                    {"source_object": "Compressor", "target_object": "Equipment"}
                ],
                "arrows": [
                    {"source_arrow": "installed_at", "target_path": ["owned_by", "located_at"]}
                ]
            }
        })
    }

    #[test]
    fn tools_list_advertises_route_preview_schema() {
        let mut server = test_server(None);
        let response = server
            .handle_message(json!({
                "jsonrpc": "2.0",
                "id": 11,
                "method": "tools/list"
            }))
            .expect("tools/list response");

        let tool = response
            .pointer("/result/tools")
            .and_then(Value::as_array)
            .expect("tool array")
            .iter()
            .find(|tool| {
                tool.get("name").and_then(Value::as_str)
                    == Some(crate::route_preview_tools::ROUTE_PREVIEW_TOOL_NAME)
            })
            .cloned()
            .expect("route preview tool should be advertised");

        let expected = crate::route_preview_tools::route_preview_tool_specs()
            .into_iter()
            .find(|tool| tool.name == crate::route_preview_tools::ROUTE_PREVIEW_TOOL_NAME)
            .expect("route preview tool spec");

        assert_eq!(
            tool.get("description").and_then(Value::as_str),
            Some(expected.description)
        );
        assert_eq!(tool.get("inputSchema"), Some(&expected.input_schema));
        assert_eq!(
            tool.pointer("/annotations/readOnlyHint")
                .and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn tools_call_dispatches_route_preview() {
        let (mut server, a, b, ab) = test_server_with_route_runtime();
        let response = server
            .handle_message(json!({
                "jsonrpc": "2.0",
                "id": 12,
                "method": "tools/call",
                "params": {
                    "name": crate::route_preview_tools::ROUTE_PREVIEW_TOOL_NAME,
                    "arguments": {
                        "route": {
                            "start_entity": a,
                            "segments": [
                                {
                                    "relation_id": ab,
                                    "direction": "forward"
                                }
                            ]
                        }
                    }
                }
            }))
            .expect("tools/call response");

        assert_eq!(
            response.pointer("/result/isError").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            response
                .pointer("/result/structuredContent/version")
                .and_then(Value::as_str),
            Some("axiograph_discover_route_preview_v1")
        );
        assert_eq!(
            response
                .pointer("/result/structuredContent/route/end_entity")
                .and_then(Value::as_u64),
            Some(b as u64)
        );
        assert_eq!(
            response
                .pointer("/result/structuredContent/route/normalized/hops/0/relation")
                .and_then(Value::as_str),
            Some("road")
        );
    }

    #[test]
    fn tools_call_dispatches_semantic_context_report() {
        let mut server = test_server_with_semantic_family_runtime();
        let response = server
            .handle_message(json!({
                "jsonrpc": "2.0",
                "id": 121,
                "method": "tools/call",
                "params": {
                    "name": "semantic_context_report",
                    "arguments": {
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
                    }
                }
            }))
            .expect("tools/call response");

        assert_eq!(
            response.pointer("/result/isError").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            response
                .pointer("/result/structuredContent/version")
                .and_then(Value::as_str),
            Some("axiograph_semantic_context_report_v1")
        );
        assert_eq!(
            response
                .pointer("/result/structuredContent/report/context/context_id")
                .and_then(Value::as_str),
            Some("domain:family_lookup")
        );
        assert_eq!(
            response
                .pointer("/result/structuredContent/report/coverage/tested_rules")
                .and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            response
                .pointer("/result/structuredContent/report/competency_coverage/total")
                .and_then(Value::as_u64),
            Some(1)
        );
    }

    #[test]
    fn tools_list_advertises_transport_preview_schema() {
        let mut server = test_server(None);
        let response = server
            .handle_message(json!({
                "jsonrpc": "2.0",
                "id": 15,
                "method": "tools/list"
            }))
            .expect("tools/list response");

        let tool = response
            .pointer("/result/tools")
            .and_then(Value::as_array)
            .expect("tool array")
            .iter()
            .find(|tool| {
                tool.get("name").and_then(Value::as_str)
                    == Some(crate::transport_preview_tools::TRANSPORT_PREVIEW_TOOL_NAME)
            })
            .cloned()
            .expect("transport preview tool should be advertised");

        let expected = crate::transport_preview_tools::transport_preview_tool_specs()
            .into_iter()
            .find(|tool| tool.name == crate::transport_preview_tools::TRANSPORT_PREVIEW_TOOL_NAME)
            .expect("transport preview tool spec");

        assert_eq!(
            tool.get("description").and_then(Value::as_str),
            Some(expected.description)
        );
        assert_eq!(tool.get("inputSchema"), Some(&expected.input_schema));
        assert_eq!(
            tool.pointer("/annotations/readOnlyHint")
                .and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn tools_call_dispatches_transport_preview() {
        let mut server = test_server(None);
        let response = server
            .handle_message(json!({
                "jsonrpc": "2.0",
                "id": 16,
                "method": "tools/call",
                "params": {
                    "name": crate::transport_preview_tools::TRANSPORT_PREVIEW_TOOL_NAME,
                    "arguments": sample_transport_preview_arguments()
                }
            }))
            .expect("tools/call response");

        assert_eq!(
            response.pointer("/result/isError").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            response
                .pointer("/result/structuredContent/version")
                .and_then(Value::as_str),
            Some(crate::evolution_preview::EVOLUTION_PREVIEW_VERSION_V1)
        );
        assert_eq!(
            response
                .pointer("/result/structuredContent/kind")
                .and_then(Value::as_str),
            Some("migration_preview")
        );
        assert_eq!(
            response
                .pointer("/result/structuredContent/candidate_label")
                .and_then(Value::as_str),
            Some("Plant->Ops")
        );
    }

    #[cfg(unix)]
    #[test]
    fn axql_run_require_verified_returns_v4_receipt_and_authoritative_rows() -> Result<()> {
        let repo = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let verifier = repo.join("lean/.lake/build/bin/axiograph_verify");
        if !verifier.exists() {
            return Ok(());
        }
        let axi = r#"module Demo
schema S:
  object Person
instance I of S:
  Person = {Alice, Bob}
"#;
        let mut db = PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;
        let mut server = SemanticMcpServer {
            runtime: crate::db_server::ReadOnlySemanticRuntime {
                accepted_snapshot_id: Some(AcceptedSnapshotId::new("accepted:mcp")),
                accepted_axi_anchor: Some(AcceptedAxiAnchor::new(
                    AcceptedSnapshotId::new("accepted:mcp"),
                    AxiDigest::from_axi_text(axi),
                )),
                accepted_axi_text: Some(axi.to_string()),
                db: Arc::new(db),
                meta: Some(meta),
            },
            tool_max_rows: 25,
            cert_verify: disabled_verifier(),
        };
        server.cert_verify.verifier_bin = Some(verifier.clone());
        server.cert_verify.timeout = Some(Duration::from_secs(10));
        server.cert_verify.approved_checker_sha256 =
            Some(crate::verifier_bridge::sha256_file(&verifier)?);
        server.cert_verify.approved_checker_build_id = Some("axiograph-verify-main-v3".to_string());

        let response = server.call_axql_run(json!({
            "query_ir_v1": {
                "version": 1,
                "select_vars": ["?person"],
                "where_atoms": [
                    {"kind":"type","term":"?person","type":"Person"}
                ],
                "limit": 10
            },
            "certificate_policy": "require_verified"
        }))?;
        assert_eq!(response["certificate"]["version"], 3);
        assert_eq!(response["certificate"]["kind"], "query_result_v4");
        assert_eq!(response["certificate_verified"], true);
        assert_eq!(
            response["trust"]["soundness"],
            "lean_verified_finite_exact_complete"
        );
        assert_eq!(response["verified_rows"].as_array().map(Vec::len), Some(2));
        assert_eq!(
            response["trust"]["completeness_claim"],
            "exact_for_declared_finite_decidable_fragment"
        );
        assert_eq!(
            response["entity_view_enrichment_claim"],
            "unbound_runtime_entity_view_no_verified_row_claim"
        );
        Ok(())
    }
}
