use anyhow::{anyhow, Result};
use axiograph_pathdb::PathDB;
use serde_json::{json, Value};

pub(crate) const ROUTE_PREVIEW_TOOL_NAME: &str = "route_preview";

#[derive(Debug, Clone)]
pub(crate) struct RoutePreviewToolSpecV1 {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

#[derive(Clone, Copy)]
pub(crate) struct RoutePreviewToolContext<'a> {
    pub db: &'a PathDB,
}

pub(crate) fn route_preview_tool_specs() -> Vec<RoutePreviewToolSpecV1> {
    vec![RoutePreviewToolSpecV1 {
        name: ROUTE_PREVIEW_TOOL_NAME,
        description: "Preview one concrete route chain and, optionally, compare it to a second route using the existing route normalization/equivalence runtime. Returns the same report shape as `axiograph discover route-preview`.",
        input_schema: json!({
            "type": "object",
            "required": ["route"],
            "properties": {
                "route": route_chain_json_schema(),
                "equivalent_to": route_chain_json_schema()
            }
        }),
    }]
}

pub(crate) fn is_route_preview_tool(name: &str) -> bool {
    name == ROUTE_PREVIEW_TOOL_NAME
}

pub(crate) fn invoke_route_preview_tool(
    name: &str,
    context: RoutePreviewToolContext<'_>,
    arguments: Value,
) -> Result<Value> {
    match name {
        ROUTE_PREVIEW_TOOL_NAME => {
            serde_json::to_value(call_route_preview(context, arguments)?).map_err(Into::into)
        }
        other => Err(anyhow!("unknown route preview tool `{other}`")),
    }
}

pub(crate) fn call_route_preview(
    context: RoutePreviewToolContext<'_>,
    arguments: Value,
) -> Result<crate::route_preview::DiscoverRoutePreviewReportV1> {
    let request: crate::route_preview::RoutePreviewRequestV1 = serde_json::from_value(arguments)
        .map_err(|err| anyhow!("route_preview: invalid args: {err}"))?;
    crate::route_preview::discover_route_preview_from_request(context.db, &request)
}

fn route_chain_json_schema() -> Value {
    json!({
        "type": "object",
        "required": ["start_entity"],
        "properties": {
            "start_entity": { "type": "integer", "minimum": 0 },
            "segments": {
                "type": "array",
                "items": route_segment_json_schema(),
                "maxItems": crate::route_preview::ROUTE_PREVIEW_MAX_SEGMENTS
            }
        }
    })
}

fn route_segment_json_schema() -> Value {
    json!({
        "type": "object",
        "required": ["relation_id"],
        "properties": {
            "relation_id": { "type": "integer", "minimum": 0 },
            "direction": {
                "type": "string",
                "enum": ["forward", "reverse"]
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route_preview::{RouteDirectionV1, RoutePreviewRequestV1};

    fn sample_db() -> (PathDB, u32, u32, u32) {
        let mut db = PathDB::new();
        let a = db.add_entity("Node", vec![("name", "A")]);
        let b = db.add_entity("Node", vec![("name", "B")]);
        let ab = db.add_relation("road", a, b, 0.9, Vec::new());
        db.build_indexes();
        (db, a, b, ab)
    }

    #[test]
    fn route_preview_tool_specs_expose_route_preview_tool() {
        let spec = route_preview_tool_specs()
            .into_iter()
            .find(|tool| tool.name == ROUTE_PREVIEW_TOOL_NAME)
            .expect("route preview tool spec");
        assert!(spec.description.contains("discover route-preview"));
        assert_eq!(
            spec.input_schema["required"].as_array(),
            Some(&vec![Value::String("route".to_string())])
        );
    }

    #[test]
    fn route_preview_dispatch_returns_cli_report_shape() -> Result<()> {
        let (db, a, b, ab) = sample_db();
        let out = call_route_preview(
            RoutePreviewToolContext { db: &db },
            serde_json::to_value(RoutePreviewRequestV1 {
                route: crate::route_preview::RouteChainV1 {
                    start_entity: a,
                    segments: vec![crate::route_preview::RouteSegmentRefV1 {
                        relation_id: ab,
                        direction: RouteDirectionV1::Forward,
                    }],
                },
                equivalent_to: None,
            })?,
        )?;

        assert_eq!(out.version, "axiograph_discover_route_preview_v1");
        assert_eq!(out.route.end_entity, b);
        assert_eq!(out.route.normalized.end_entity, b);
        assert_eq!(out.route.normalized.hops.len(), 1);
        assert_eq!(out.route.normalized.hops[0].relation, "road");
        Ok(())
    }

    #[test]
    fn route_preview_tool_schema_caps_segment_count() {
        let spec = route_preview_tool_specs()
            .into_iter()
            .find(|tool| tool.name == ROUTE_PREVIEW_TOOL_NAME)
            .expect("route preview tool spec");
        assert_eq!(
            spec.input_schema["properties"]["route"]["properties"]["segments"]["maxItems"].as_u64(),
            Some(crate::route_preview::ROUTE_PREVIEW_MAX_SEGMENTS as u64)
        );
    }
}
