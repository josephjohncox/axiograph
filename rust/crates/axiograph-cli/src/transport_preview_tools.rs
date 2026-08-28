use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub(crate) const TRANSPORT_PREVIEW_TOOL_NAME: &str = "transport_preview";

#[derive(Debug, Clone)]
pub(crate) struct TransportPreviewToolSpecV1 {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct TransportPreviewToolContext;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TransportPreviewToolArgsV1 {
    pub axi_text: String,
    pub morphism: axiograph_pathdb::migration::SchemaMorphismV1,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub apply_refinement_handle_id: Option<String>,
}

pub(crate) fn transport_preview_tool_specs() -> Vec<TransportPreviewToolSpecV1> {
    vec![TransportPreviewToolSpecV1 {
        name: TRANSPORT_PREVIEW_TOOL_NAME,
        description: "Preview schema-morphism transport over canonical `.axi`, returning the same `EvolutionPreviewV1` JSON shape as `axiograph discover transport-preview`.",
        input_schema: json!({
            "type": "object",
            "required": ["axi_text", "morphism"],
            "properties": {
                "axi_text": { "type": "string" },
                "schema": { "type": "string" },
                "morphism": schema_morphism_json_schema(),
                "apply_refinement_handle_id": { "type": "string" }
            }
        }),
    }]
}

pub(crate) fn is_transport_preview_tool(name: &str) -> bool {
    name == TRANSPORT_PREVIEW_TOOL_NAME
}

pub(crate) fn invoke_transport_preview_tool(
    name: &str,
    _context: TransportPreviewToolContext,
    arguments: Value,
) -> Result<Value> {
    match name {
        TRANSPORT_PREVIEW_TOOL_NAME => {
            serde_json::to_value(call_transport_preview(arguments)?).map_err(Into::into)
        }
        other => Err(anyhow!("unknown transport preview tool `{other}`")),
    }
}

pub(crate) fn call_transport_preview(
    arguments: Value,
) -> Result<crate::evolution_preview::EvolutionPreviewV1> {
    let args: TransportPreviewToolArgsV1 = serde_json::from_value(arguments)
        .map_err(|err| anyhow!("transport_preview: invalid args: {err}"))?;
    let morphism_json = serde_json::to_string(&args.morphism)
        .map_err(|err| anyhow!("transport_preview: failed to serialize morphism args: {err}"))?;
    crate::discover_transport_preview_from_inputs(
        &args.axi_text,
        args.schema.as_deref(),
        &morphism_json,
        args.apply_refinement_handle_id.as_deref(),
    )
}

fn schema_morphism_json_schema() -> Value {
    json!({
        "type": "object",
        "required": ["source_schema", "target_schema", "objects", "arrows"],
        "properties": {
            "source_schema": { "type": "string" },
            "target_schema": { "type": "string" },
            "objects": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["source_object", "target_object"],
                    "properties": {
                        "source_object": { "type": "string" },
                        "target_object": { "type": "string" }
                    }
                }
            },
            "arrows": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["source_arrow", "target_path"],
                    "properties": {
                        "source_arrow": { "type": "string" },
                        "target_path": {
                            "type": "array",
                            "items": { "type": "string" }
                        }
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_axi() -> &'static str {
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
"#
    }

    fn sample_args() -> TransportPreviewToolArgsV1 {
        TransportPreviewToolArgsV1 {
            axi_text: sample_axi().to_string(),
            schema: Some("Plant".to_string()),
            morphism: axiograph_pathdb::migration::SchemaMorphismV1 {
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
            },
            apply_refinement_handle_id: None,
        }
    }

    #[test]
    fn transport_preview_tool_specs_expose_transport_preview_tool() {
        let spec = transport_preview_tool_specs()
            .into_iter()
            .find(|tool| tool.name == TRANSPORT_PREVIEW_TOOL_NAME)
            .expect("transport preview tool spec");
        assert!(spec.description.contains("discover transport-preview"));
        assert_eq!(
            spec.input_schema["required"].as_array(),
            Some(&vec![
                Value::String("axi_text".to_string()),
                Value::String("morphism".to_string())
            ])
        );
    }

    #[test]
    fn transport_preview_dispatch_returns_cli_report_shape() -> Result<()> {
        let out = call_transport_preview(serde_json::to_value(sample_args())?)?;

        assert_eq!(
            out.version,
            crate::evolution_preview::EVOLUTION_PREVIEW_VERSION_V1
        );
        assert_eq!(out.kind, "migration_preview");
        assert_eq!(out.candidate_label, "Plant->Ops");
        assert!(!out.refinement_candidates.is_empty());
        Ok(())
    }

    #[test]
    fn transport_preview_dispatch_can_apply_refinement_handle() -> Result<()> {
        let first_preview = call_transport_preview(serde_json::to_value(sample_args())?)?;
        let handle_id = first_preview
            .refinement_candidates
            .first()
            .map(|candidate| candidate.handle.id.clone())
            .expect("expected migration refinement handle");

        let mut applied_args = sample_args();
        applied_args.apply_refinement_handle_id = Some(handle_id);
        let applied = call_transport_preview(serde_json::to_value(applied_args)?)?;

        assert_eq!(applied.kind, "migration_preview");
        assert!(applied.ok);
        assert!(applied.residual_obligations.is_empty());
        assert!(applied.refinement_candidates.is_empty());
        Ok(())
    }
}
