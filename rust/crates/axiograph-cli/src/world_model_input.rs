//! World-model input helpers for canonical `.axi` meaning-plane export.
//!
//! The world model should reason over the canonical `.axi` meaning-plane (schema/theory/instance),
//! not over reversible `PathDBExportV1` snapshots (which contain interned string tables and other
//! implementation details).
//!
//! This module provides a single, shared exporter used by:
//! - the REPL/LLM tool-loop (`llm.rs`)
//! - the DB server world-model endpoints (`db_server.rs`)
//!
//! so the behavior cannot drift.

use anyhow::{anyhow, Result};
use axiograph_pathdb::{AcceptedSnapshotId, AxiDigest, PathDB, PathdbSnapshotId};

#[derive(Debug, Clone)]
pub(crate) struct WorldModelAxiInputV1 {
    pub(crate) axi_digest_v1: AxiDigest,
    pub(crate) axi_text: String,
    pub(crate) selected_module_name: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct WorldModelAxiInputOptionsV1 {
    pub(crate) module_name: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct WorldModelInputBuildOptionsV1 {
    pub(crate) module_name: Option<String>,
    pub(crate) pathdb_snapshot_id: Option<PathdbSnapshotId>,
    pub(crate) accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub(crate) training_export: Option<crate::world_model::JepaExportOptions>,
}

fn entity_attr_string(db: &PathDB, entity_id: u32, key: &str) -> Option<String> {
    let key_id = db.interner.id_of(key)?;
    let value_id = db.entities.get_attr(entity_id, key_id)?;
    db.interner.lookup(value_id)
}

fn list_module_names(db: &PathDB) -> Vec<String> {
    let Some(module_ids) = db.find_by_type(axiograph_pathdb::axi_meta::META_TYPE_MODULE) else {
        return Vec::new();
    };
    let mut names: Vec<String> = module_ids
        .iter()
        .filter_map(|id| entity_attr_string(db, id, axiograph_pathdb::axi_meta::META_ATTR_NAME))
        .collect();
    names.sort();
    names.dedup();
    names
}

fn choose_module_name(db: &PathDB, module_names: &[String]) -> Option<String> {
    if module_names.is_empty() {
        return None;
    }
    if module_names.len() == 1 {
        return Some(module_names[0].clone());
    }

    // Pick the module that dominates the snapshot by entity count, falling back to
    // stable lexical order.
    match db
        .interner
        .id_of(axiograph_pathdb::axi_meta::ATTR_AXI_MODULE)
    {
        Some(key_id) => {
            let mut counts: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for entity_id in 0..(db.entities.len() as u32) {
                let Some(value_id) = db.entities.get_attr(entity_id, key_id) else {
                    continue;
                };
                let Some(name) = db.interner.lookup(value_id) else {
                    continue;
                };
                *counts.entry(name).or_insert(0) += 1;
            }

            module_names
                .iter()
                .max_by(|a, b| {
                    let ca = counts.get(*a).copied().unwrap_or(0);
                    let cb = counts.get(*b).copied().unwrap_or(0);
                    ca.cmp(&cb).then_with(|| b.cmp(a)) // stable tie-break (lexicographic)
                })
                .cloned()
        }
        None => Some(module_names[0].clone()),
    }
}

pub(crate) fn export_pathdb_world_model_axi(
    db: &PathDB,
    opts: &WorldModelAxiInputOptionsV1,
) -> Result<WorldModelAxiInputV1> {
    // Prefer canonical module export when a meta-plane module is present.
    let module_names = list_module_names(db);
    let selected = if let Some(want) = opts.module_name.as_ref() {
        if module_names.iter().any(|n| n == want) {
            Some(want.clone())
        } else {
            return Err(anyhow!(
                "unknown module `{}` (available: {})",
                want,
                if module_names.is_empty() {
                    "<none>".to_string()
                } else {
                    module_names.join(", ")
                }
            ));
        }
    } else {
        choose_module_name(db, &module_names)
    };

    if let Some(module_name) = selected.as_ref() {
        match axiograph_pathdb::axi_module_export::export_axi_schema_v1_module_from_pathdb(
            db,
            module_name,
        ) {
            Ok(axi_text) => {
                let digest = AxiDigest::from_axi_text(&axi_text);
                return Ok(WorldModelAxiInputV1 {
                    axi_digest_v1: digest,
                    axi_text,
                    selected_module_name: selected,
                });
            }
            Err(e) => {
                return Err(anyhow!(
                    "failed to export canonical module `{module_name}`: {e}"
                ));
            }
        }
    }
    Err(anyhow!(
        "no canonical `.axi` module is available in this snapshot (import a canonical module before running world-model or agent proposal flows)"
    ))
}

pub(crate) fn build_world_model_input_from_pathdb(
    db: &PathDB,
    opts: &WorldModelInputBuildOptionsV1,
) -> Result<crate::world_model::WorldModelInputV1> {
    let exported = export_pathdb_world_model_axi(
        db,
        &WorldModelAxiInputOptionsV1 {
            module_name: opts.module_name.clone(),
        },
    )?;
    build_world_model_input_from_axi_text(
        &exported.axi_text,
        exported.selected_module_name,
        opts.pathdb_snapshot_id.clone(),
        opts.accepted_snapshot_id.clone(),
        opts.training_export.clone(),
    )
}

pub(crate) fn build_world_model_input_from_axi_text(
    axi_text: &str,
    module_name: Option<String>,
    pathdb_snapshot_id: Option<PathdbSnapshotId>,
    accepted_snapshot_id: Option<AcceptedSnapshotId>,
    training_export: Option<crate::world_model::JepaExportOptions>,
) -> Result<crate::world_model::WorldModelInputV1> {
    let canonical = crate::axi_input::require_canonical_axi_text(axi_text)?;
    let mut input = crate::world_model::WorldModelInputV1::default();
    input.axi_digest_v1 = Some(canonical.digest().clone());
    input.axi_module_text = Some(axi_text.to_string());
    input.set_canonical_axi_semantics(
        module_name.or_else(|| Some(canonical.module().module().module_name.clone())),
        pathdb_snapshot_id,
        accepted_snapshot_id,
    );
    input.notes.push(format!(
        "semantic_input={}",
        crate::world_model::WORLD_MODEL_SEMANTIC_INPUT_KIND_V1
    ));
    if let Some(module_name) = input.semantic_input.module_name.as_ref() {
        input.notes.push(format!("semantic_module={module_name}"));
    }
    if let Some(export_opts) = training_export.as_ref() {
        let export = crate::world_model::build_jepa_export_from_axi_text(axi_text, export_opts)?;
        input.set_training_export_layer(export);
    }
    Ok(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiograph_pathdb::{AcceptedSnapshotId, PathdbSnapshotId};

    #[test]
    fn canonical_world_model_export_avoids_pathdb_internals() {
        let mut db = axiograph_pathdb::PathDB::new();
        let axi = r#"
module Demo

schema Demo:
  object Person
  relation Parent(child: Person, parent: Person)

instance DemoInst of Demo:
  Person = {Alice, Bob}
  Parent = {(child=Alice, parent=Bob)}
"#;
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)
            .expect("import demo module");

        let opts = WorldModelAxiInputOptionsV1 { module_name: None };
        let out =
            export_pathdb_world_model_axi(&db, &opts).expect("export canonical world-model axi");
        assert!(out.axi_digest_v1.has_v1_prefix());
        assert!(
            !out.axi_text.contains("InternedString"),
            "should not include PathDB export intern tables"
        );
        assert!(
            !out.axi_text.contains("interned_string"),
            "should not include PathDB export intern tables"
        );
        assert!(
            !out.axi_text.contains("AxiMeta"),
            "should not include meta-plane types in canonical module export"
        );
    }

    #[test]
    fn world_model_export_errors_on_unknown_module() {
        let mut db = axiograph_pathdb::PathDB::new();
        let axi = "module Demo\nschema Demo:\n  object X\ninstance I of Demo:\n  X = {a}\n";
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)
            .expect("import demo module");

        let opts = WorldModelAxiInputOptionsV1 {
            module_name: Some("NoSuchModule".to_string()),
        };
        let err = export_pathdb_world_model_axi(&db, &opts).unwrap_err();
        assert!(err.to_string().contains("unknown module"));
    }

    #[test]
    fn canonical_export_fails_without_meta_plane_module() {
        let db = axiograph_pathdb::PathDB::new();
        let opts = WorldModelAxiInputOptionsV1 { module_name: None };
        let err = export_pathdb_world_model_axi(&db, &opts).unwrap_err();
        assert!(err.to_string().contains("no canonical"));
    }

    #[test]
    fn built_world_model_input_uses_canonical_semantic_envelope() {
        let mut db = axiograph_pathdb::PathDB::new();
        let axi = r#"
module Demo

schema Demo:
  object Person
  relation Parent(child: Person, parent: Person)

instance DemoInst of Demo:
  Person = {Alice, Bob}
  Parent = {(child=Alice, parent=Bob)}
"#;
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)
            .expect("import demo module");

        let input = build_world_model_input_from_pathdb(
            &db,
            &WorldModelInputBuildOptionsV1 {
                module_name: None,
                pathdb_snapshot_id: Some(PathdbSnapshotId::new("pathdb:test")),
                accepted_snapshot_id: Some(AcceptedSnapshotId::new("accepted:test")),
                training_export: Some(crate::world_model::JepaExportOptions {
                    instance_filter: None,
                    max_items: 8,
                    mask_fields: 1,
                    seed: 1,
                    exclude_relations: Vec::new(),
                }),
            },
        )
        .expect("build world-model input");

        assert_eq!(input.semantic_input.kind, "canonical_axi_semantics_v1");
        assert_eq!(input.semantic_input.module_name.as_deref(), Some("Demo"));
        assert_eq!(
            input
                .semantic_input
                .pathdb_snapshot_id
                .as_ref()
                .map(PathdbSnapshotId::as_str),
            Some("pathdb:test")
        );
        assert_eq!(
            input
                .semantic_input
                .accepted_snapshot_id
                .as_ref()
                .map(AcceptedSnapshotId::as_str),
            Some("accepted:test")
        );
        assert!(
            input.training_export().is_some(),
            "expected training export to be carried as a semantic layer"
        );

        let json = serde_json::to_value(&input).expect("serialize world-model input");
        assert_eq!(json["semantic_input"]["kind"], "canonical_axi_semantics_v1");
        assert_eq!(json["semantic_input"]["module_name"], "Demo");
        assert_eq!(json["semantic_input"]["pathdb_snapshot_id"], "pathdb:test");
        assert_eq!(
            json["semantic_input"]["accepted_snapshot_id"],
            "accepted:test"
        );
        assert_eq!(
            json["semantic_input"]["layers"][0]["kind"],
            "training_export"
        );
    }
}
