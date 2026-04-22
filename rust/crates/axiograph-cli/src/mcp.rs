use std::collections::BTreeMap;
use std::io::{self, BufRead, BufReader, Write};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use axiograph_pathdb::PathDB;

const JSONRPC_VERSION: &str = "2.0";
const MCP_PROTOCOL_VERSION: &str = "2025-03-26";

pub(crate) fn cmd_mcp(args: crate::McpArgs) -> Result<()> {
    let runtime = crate::db_server::load_read_only_semantic_runtime(
        args.axpd.as_deref(),
        args.dir.as_deref(),
        &args.layer,
        &args.snapshot,
    )?;

    let mut server = SemanticMcpServer {
        runtime,
        tool_max_rows: args.tool_max_rows.clamp(1, 200),
    };

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();

    loop {
        let message = match read_message(&mut reader) {
            Ok(Some(message)) => message,
            Ok(None) => break,
            Err(err) => {
                let response = error_response(
                    Value::Null,
                    -32700,
                    &format!("failed to parse MCP message: {err}"),
                );
                write_message(&mut writer, &response)?;
                continue;
            }
        };

        if let Some(response) = server.handle_message(message) {
            write_message(&mut writer, &response)?;
        }
    }

    Ok(())
}

struct SemanticMcpServer {
    runtime: crate::db_server::ReadOnlySemanticRuntime,
    tool_max_rows: usize,
}

impl SemanticMcpServer {
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
                    "instructions": "Read-only typed semantic MCP surface over Axiograph query elaboration/exploration/execution and semantic rule reporting."
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
                "description": "Typecheck/elaborate a structured `query_ir_v1` query using the meta-plane, returning the elaborated query, inferred types, typed holes, exploration suggestions, plan, and trust contract.",
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
                "description": "Ask the typed elaborator what can go here next for a structured `query_ir_v1` query, returning focused typed holes, refinement candidates, semantic claims, and trust gaps.",
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
                "description": "Run a structured `query_ir_v1` query over the loaded snapshot, returning rows plus the same typed elaboration and trust metadata used by other semantic surfaces. Accepted-anchor runtimes also return an evidence support summary when the query stays inside the current certifiable subset.",
                "inputSchema": {
                    "type": "object",
                    "required": ["query_ir_v1"],
                    "properties": {
                        "query_ir_v1": query_ir_v1_schema,
                        "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
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
        tools.extend(
            crate::industrial_harness_tools::industrial_harness_tool_specs()
                .into_iter()
                .map(|tool| {
                    json!({
                        "name": tool.name,
                        "description": tool.description,
                        "inputSchema": tool.input_schema,
                        "annotations": { "readOnlyHint": tool.read_only_hint }
                    })
                }),
        );
        tools
    }

    fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value> {
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
            name if crate::industrial_harness_tools::is_industrial_harness_tool(name) => {
                crate::industrial_harness_tools::invoke_industrial_harness_tool(
                    name,
                    crate::industrial_harness_tools::IndustrialHarnessToolContext {
                        db: Some(self.runtime.db.as_ref()),
                        meta: self.runtime.meta.as_ref(),
                        accepted_axi_anchor: self.runtime.accepted_axi_anchor.as_ref(),
                    },
                    arguments,
                )
            }
            other => Err(anyhow!("unknown read-only MCP tool `{other}`")),
        }
    }

    fn call_axql_elaborate(&mut self, arguments: Value) -> Result<Value> {
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

    fn call_axql_explore(&mut self, arguments: Value) -> Result<Value> {
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

    fn call_axql_run(&mut self, arguments: Value) -> Result<Value> {
        let args: QueryToolArgs = serde_json::from_value(arguments)
            .map_err(|err| anyhow!("axql_run: invalid args: {err}"))?;
        let mut prepared = self.prepare_query(with_bounded_limit(
            args.query_ir_v1,
            args.limit,
            self.tool_max_rows,
        ))?;
        let query = prepared.query_ir_v1().to_axql_text()?;
        let elaborated = prepared.elaborated_query_text();
        let report = prepared.elaboration_report().clone();
        let trust = prepared.trust_contract_with_meta(self.runtime.meta.as_ref());
        let mut support_summary = None;
        let result = if let Some(accepted_axi_anchor) = self.runtime.accepted_axi_anchor.clone() {
            let (result, summary) = crate::evidence_support::execute_anchored_query_with_support_summary(
                &mut prepared,
                self.runtime.db.as_ref(),
                self.runtime.meta.as_ref(),
                accepted_axi_anchor,
            )?;
            support_summary = summary;
            result
        } else {
            prepared.execute(self.runtime.db.as_ref(), self.runtime.meta.as_ref())?
        };

        let mut out = json!({
            "query": query,
            "elaborated": elaborated,
            "inferred_types": report.inferred_types,
            "notes": report.notes,
            "typed_holes": report.typed_holes,
            "exploration_suggestions": report.exploration_suggestions,
            "plan": prepared.explain_plan_lines(),
            "trust": trust,
            "results": ToolQueryResultsV1::from_axql_result(self.runtime.db.as_ref(), &result)
        });
        if let Some(summary) = support_summary {
            out.as_object_mut()
                .expect("MCP tool output should be a JSON object")
                .insert("support_summary".to_string(), serde_json::to_value(summary)?);
        }

        Ok(out)
    }

    fn prepare_query(
        &self,
        query_ir_v1: crate::query_ir::QueryIrV1,
    ) -> Result<crate::query_ir::PreparedQueryV1> {
        query_ir_v1.prepare_with_meta(self.runtime.db.as_ref(), self.runtime.meta.as_ref())
    }
}

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

fn read_message<R: BufRead>(reader: &mut R) -> Result<Option<Value>> {
    let mut content_length: Option<usize> = None;
    let mut line = String::new();

    loop {
        line.clear();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            if content_length.is_none() {
                return Ok(None);
            }
            return Err(anyhow!("unexpected EOF while reading MCP headers"));
        }

        if line == "\r\n" || line == "\n" {
            break;
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if let Some(raw_len) = trimmed.strip_prefix("Content-Length:") {
            content_length = Some(
                raw_len
                    .trim()
                    .parse()
                    .context("invalid Content-Length header")?,
            );
        }
    }

    let content_length = content_length.ok_or_else(|| anyhow!("missing Content-Length header"))?;
    let mut body = vec![0_u8; content_length];
    reader.read_exact(&mut body)?;
    let value = serde_json::from_slice(&body).context("invalid MCP JSON body")?;
    Ok(Some(value))
}

fn write_message<W: Write>(writer: &mut W, value: &Value) -> Result<()> {
    let body = serde_json::to_vec(value)?;
    write!(
        writer,
        "Content-Length: {}\r\nContent-Type: application/json\r\n\r\n",
        body.len()
    )?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

fn success_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "result": result
    })
}

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

fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;

    use axiograph_pathdb::{AcceptedAxiAnchor, AcceptedSnapshotId, AxiDigest};

    fn sample_anchor() -> AcceptedAxiAnchor {
        AcceptedAxiAnchor::new(
            AcceptedSnapshotId::new("accepted:regulated-line"),
            AxiDigest::new("fnv1a64:regulated-line"),
        )
    }

    fn sample_trust() -> crate::trust_contract::TrustContractV1 {
        crate::trust_contract::TrustContractV1 {
            trust_class: "runtime_guarded".to_string(),
            soundness: "seed_cache_artifacts_are_anchor_scoped".to_string(),
            coverage: "regulated_line_seed_shadow_harness".to_string(),
            scope: crate::trust_contract::TrustScopeV1 {
                anchor: "accepted:regulated-line@fnv1a64:regulated-line".to_string(),
                context: "regulated_production_line_seed".to_string(),
            },
            reasons: Vec::new(),
            certifiable_disjuncts: None,
            execution_only_disjuncts: None,
            semantic_coverage: None,
            semantic_claims: Vec::new(),
            gaps: Vec::new(),
        }
    }

    fn temp_test_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "axiograph-mcp-industrial-harness-{name}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("temp test dir");
        path
    }

    fn test_server(accepted_axi_anchor: Option<AcceptedAxiAnchor>) -> SemanticMcpServer {
        SemanticMcpServer {
            runtime: crate::db_server::ReadOnlySemanticRuntime {
                snapshot_key: "test-snapshot".to_string(),
                accepted_snapshot_id: None,
                accepted_axi_anchor,
                db: Arc::new(PathDB::new()),
                meta: None,
            },
            tool_max_rows: 25,
        }
    }

    fn test_server_with_regulated_line_runtime() -> SemanticMcpServer {
        let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap_or_else(|_| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
            });
        let axi_path = repo_root.join("examples/industrial/RegulatedProductionLine.axi");
        let axi_text = std::fs::read_to_string(&axi_path).expect("read regulated line axi");
        let anchor = AcceptedAxiAnchor::new(
            AcceptedSnapshotId::new("accepted:regulated-line"),
            AxiDigest::from_axi_text(&axi_text),
        );
        let db = crate::load_pathdb_for_cli(&axi_path).expect("load regulated line pathdb");
        let meta =
            axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db).expect("meta plane");
        SemanticMcpServer {
            runtime: crate::db_server::ReadOnlySemanticRuntime {
                snapshot_key: "regulated-line-snapshot".to_string(),
                accepted_snapshot_id: Some(AcceptedSnapshotId::new("accepted:regulated-line")),
                accepted_axi_anchor: Some(anchor),
                db: Arc::new(db),
                meta: Some(meta),
            },
            tool_max_rows: 25,
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
                    snapshot_key: "route-preview-snapshot".to_string(),
                    accepted_snapshot_id: None,
                    accepted_axi_anchor: None,
                    db: Arc::new(db),
                    meta: None,
                },
                tool_max_rows: 25,
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
        assert!(names
            .iter()
            .any(|name| name == crate::route_preview_tools::ROUTE_PREVIEW_TOOL_NAME));
        assert!(names
            .iter()
            .any(|name| name == crate::transport_preview_tools::TRANSPORT_PREVIEW_TOOL_NAME));
        assert!(names
            .iter()
            .any(|name| name == "industrial_harness_inspect"));
        assert!(names
            .iter()
            .any(|name| name == "industrial_harness_run_regulated_seed"));
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
    fn tools_list_advertises_transport_preview_schema() {
        let mut server = test_server(None);
        let response = server
            .handle_message(json!({
                "jsonrpc": "2.0",
                "id": 13,
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
                "id": 14,
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

    #[test]
    fn tools_list_advertises_industrial_harness_inspect_schema() {
        let mut server = test_server(None);
        let response = server
            .handle_message(json!({
                "jsonrpc": "2.0",
                "id": 1,
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
                    == Some(crate::industrial_harness_tools::INDUSTRIAL_HARNESS_INSPECT_TOOL_NAME)
            })
            .cloned()
            .expect("industrial harness tool should be advertised");

        let expected = crate::industrial_harness_tools::industrial_harness_tool_specs()
            .into_iter()
            .find(|tool| {
                tool.name == crate::industrial_harness_tools::INDUSTRIAL_HARNESS_INSPECT_TOOL_NAME
            })
            .expect("industrial harness tool spec");

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
    fn tools_call_dispatches_industrial_harness_inspect() -> Result<()> {
        let anchor = sample_anchor();
        let cache_root = temp_test_dir("dispatch");
        crate::industrial_harness::materialize_regulated_production_line_seed_harness(
            &cache_root,
            "mcp-inspect-run",
            4242,
            anchor.clone(),
            sample_trust(),
        )?;

        let mut server = test_server(Some(anchor));
        let response = server
            .handle_message(json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": crate::industrial_harness_tools::INDUSTRIAL_HARNESS_INSPECT_TOOL_NAME,
                    "arguments": {
                        "cache_root": cache_root.to_string_lossy().to_string(),
                        "campaign_id": crate::industrial_harness::REGULATED_PRODUCTION_LINE_CAMPAIGN_ID,
                        "run_id": "mcp-inspect-run"
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
                .pointer("/result/structuredContent/bundle/run/run_id")
                .and_then(Value::as_str),
            Some("mcp-inspect-run")
        );
        assert_eq!(
            response
                .pointer("/result/structuredContent/verification/failed")
                .and_then(Value::as_u64),
            Some(0)
        );
        std::fs::remove_dir_all(&cache_root).expect("cleanup temp dir");
        Ok(())
    }

    #[test]
    fn tools_list_advertises_industrial_harness_run_schema() {
        let mut server = test_server(None);
        let response = server
            .handle_message(json!({
                "jsonrpc": "2.0",
                "id": 3,
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
                    == Some(crate::industrial_harness_tools::INDUSTRIAL_HARNESS_RUN_REGULATED_SEED_TOOL_NAME)
            })
            .cloned()
            .expect("industrial harness run tool should be advertised");

        let expected = crate::industrial_harness_tools::industrial_harness_tool_specs()
            .into_iter()
            .find(|tool| {
                tool.name
                    == crate::industrial_harness_tools::INDUSTRIAL_HARNESS_RUN_REGULATED_SEED_TOOL_NAME
            })
            .expect("industrial harness run tool spec");

        assert_eq!(
            tool.get("description").and_then(Value::as_str),
            Some(expected.description)
        );
        assert_eq!(tool.get("inputSchema"), Some(&expected.input_schema));
        assert_eq!(
            tool.pointer("/annotations/readOnlyHint")
                .and_then(Value::as_bool),
            Some(false)
        );
    }

    #[test]
    fn tools_call_dispatches_industrial_harness_run_regulated_seed() -> Result<()> {
        let cache_root = temp_test_dir("dispatch-run");
        let mut server = test_server_with_regulated_line_runtime();
        let response = server
            .handle_message(json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": crate::industrial_harness_tools::INDUSTRIAL_HARNESS_RUN_REGULATED_SEED_TOOL_NAME,
                    "arguments": {
                        "cache_root": cache_root.to_string_lossy().to_string(),
                        "run_id": "mcp-run-seed",
                        "created_at_unix_secs": 4242
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
                .pointer("/result/structuredContent/run_id")
                .and_then(Value::as_str),
            Some("mcp-run-seed")
        );
        assert_eq!(
            response
                .pointer("/result/structuredContent/trust/trust_class")
                .and_then(Value::as_str),
            Some("runtime_guarded")
        );
        std::fs::remove_dir_all(&cache_root).expect("cleanup temp dir");
        Ok(())
    }

    #[test]
    fn axql_run_returns_support_summary_for_anchored_queries() -> Result<()> {
        let axi = r#"
module Demo

schema S:
  object Person
  object Context
  relation Parent(child: Person, parent: Person) @context Context

instance I of S:
  Person = {Alice, Bob}
  Context = {CensusData}
  Parent = {
    (child=Alice, parent=Bob, ctx=CensusData)
  }
"#;
        let mut db = PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;
        let anchor = AcceptedAxiAnchor::new(
            AcceptedSnapshotId::new("accepted:test"),
            AxiDigest::from_axi_text(axi),
        );
        let mut server = SemanticMcpServer {
            runtime: crate::db_server::ReadOnlySemanticRuntime {
                snapshot_key: "support-summary-snapshot".to_string(),
                accepted_snapshot_id: Some(AcceptedSnapshotId::new("accepted:test")),
                accepted_axi_anchor: Some(anchor),
                db: Arc::new(db),
                meta: Some(meta),
            },
            tool_max_rows: 25,
        };

        let response = server.call_axql_run(json!({
            "query_ir_v1": {
                "version": 1,
                "select": ["?p"],
                "where": [
                    {
                        "kind": "fact",
                        "fact": "?f",
                        "relation": "S.Parent",
                        "fields": {
                            "child": "Alice",
                            "parent": "?p",
                            "ctx": "CensusData"
                        }
                    }
                ],
                "limit": 10
            }
        }))?;

        assert_eq!(
            response["trust"]["soundness"].as_str(),
            Some("certificate_available_but_not_emitted")
        );
        assert_eq!(
            response["support_summary"]["accepted_axi_anchor"]["accepted_snapshot_id"].as_str(),
            Some("accepted:test")
        );
        assert_eq!(
            response["support_summary"]["trust"]["soundness"].as_str(),
            Some("certificate_emitted_row_soundness_unverified")
        );
        assert!(response["support_summary"]["supported_facts"]
            .as_array()
            .is_some_and(|facts| !facts.is_empty()));
        Ok(())
    }
}
