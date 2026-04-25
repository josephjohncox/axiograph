use anyhow::{anyhow, Result};
use axiograph_pathdb::PathDB;
use serde_json::{json, Value};

pub(crate) const PATH_CERTIFY_TOOL_NAME: &str = "path_certify";

#[derive(Debug, Clone)]
pub(crate) struct PathCertToolSpecV1 {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

#[derive(Clone, Copy)]
pub(crate) struct PathCertToolContext<'a> {
    pub db: &'a PathDB,
}

pub(crate) fn path_cert_tool_specs() -> Vec<PathCertToolSpecV1> {
    vec![PathCertToolSpecV1 {
        name: PATH_CERTIFY_TOOL_NAME,
        description: "Certify one concrete relation-id path using the same `reachability_v3` certificate machinery as `/cert/reachability`, returning the shared path-cert report with anchor digest, trust contract, and optional verification result.",
        input_schema: json!({
            "type": "object",
            "required": ["start"],
            "properties": {
                "start": { "type": "integer", "minimum": 0 },
                "relation_ids": {
                    "type": "array",
                    "items": { "type": "integer", "minimum": 0 }
                },
                "verify": { "type": "boolean" }
            }
        }),
    }]
}

pub(crate) fn is_path_cert_tool(name: &str) -> bool {
    name == PATH_CERTIFY_TOOL_NAME
}

pub(crate) fn invoke_path_cert_tool(
    name: &str,
    context: PathCertToolContext<'_>,
    arguments: Value,
) -> Result<Value> {
    match name {
        PATH_CERTIFY_TOOL_NAME => {
            serde_json::to_value(call_path_cert(context, arguments)?).map_err(Into::into)
        }
        other => Err(anyhow!("unknown path cert tool `{other}`")),
    }
}

pub(crate) fn call_path_cert(
    context: PathCertToolContext<'_>,
    arguments: Value,
) -> Result<crate::path_cert::PathCertReportV1> {
    let verifier: &crate::path_cert::PathCertVerifier =
        &crate::db_server::verify_certificate_with_default_resolution;
    crate::path_cert::certify_path_from_value(context.db, arguments, Some(verifier))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_db() -> Result<(PathDB, u32, u32, u32)> {
        let axi = r#"
module Demo

schema S:
  object Node
  relation road(from: Node, to: Node)

instance I of S:
  Node = {A, B}
  road = {(from=A, to=B)}
"#;
        let mut db = PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();

        let a = find_named_entity(&db, "Node", "A").expect("find A");
        let b = find_named_entity(&db, "Node", "B").expect("find B");
        let ab = find_relation(&db, "road", a, b).expect("find road relation");
        Ok((db, a, b, ab))
    }

    fn find_named_entity(db: &PathDB, type_name: &str, name: &str) -> Option<u32> {
        let ids = db.find_by_type(type_name)?;
        let key = db.interner.id_of("name")?;
        ids.iter().find(|id| {
            db.entities
                .get_attr(*id, key)
                .and_then(|value| db.interner.lookup(value))
                .as_deref()
                == Some(name)
        })
    }

    fn find_relation(db: &PathDB, rel_type: &str, source: u32, target: u32) -> Option<u32> {
        (0..db.relations.len() as u32).find(|rel_id| {
            let Some(rel) = db.relations.get_relation(*rel_id) else {
                return false;
            };
            rel.source == source
                && rel.target == target
                && db.interner.lookup(rel.rel_type).as_deref() == Some(rel_type)
        })
    }

    #[test]
    fn path_cert_tool_specs_expose_path_certify_tool() {
        let spec = path_cert_tool_specs()
            .into_iter()
            .find(|tool| tool.name == PATH_CERTIFY_TOOL_NAME)
            .expect("path cert tool spec");
        assert!(spec.description.contains("/cert/reachability"));
        assert_eq!(
            spec.input_schema["required"].as_array(),
            Some(&vec![Value::String("start".to_string())])
        );
    }

    #[test]
    fn path_cert_dispatch_returns_shared_report_shape() -> Result<()> {
        let (db, a, _b, ab) = sample_db()?;
        let out = call_path_cert(
            PathCertToolContext { db: &db },
            json!({
                "start": a,
                "relation_ids": [ab],
                "verify": false
            }),
        )?;

        assert!(out.anchor_digest.as_str().starts_with("fnv1a64:"));
        assert_eq!(out.certificate["kind"].as_str(), Some("reachability_v3"));
        assert_eq!(out.certificate["proof"]["type"].as_str(), Some("step"));
        Ok(())
    }

    #[test]
    fn path_cert_tool_schema_allows_empty_relation_ids() {
        let spec = path_cert_tool_specs()
            .into_iter()
            .find(|tool| tool.name == PATH_CERTIFY_TOOL_NAME)
            .expect("path cert tool spec");
        assert!(spec.input_schema["properties"]["relation_ids"]
            .get("items")
            .is_some());
    }
}
