//! World-model input helpers (canonical `.axi` export + fallback behavior).
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
use axiograph_pathdb::PathDB;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorldModelAxiInputKindV1 {
    CanonicalModuleExport,
    PathdbExportFallback,
}

impl WorldModelAxiInputKindV1 {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            WorldModelAxiInputKindV1::CanonicalModuleExport => "canonical_module_export",
            WorldModelAxiInputKindV1::PathdbExportFallback => "pathdb_export_fallback",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WorldModelAxiInputV1 {
    pub(crate) kind: WorldModelAxiInputKindV1,
    pub(crate) axi_digest_v1: String,
    pub(crate) axi_text: String,
    pub(crate) selected_module_name: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct WorldModelAxiInputOptionsV1 {
    pub(crate) module_name: Option<String>,
    pub(crate) require_canonical: bool,
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
    match db.interner.id_of(axiograph_pathdb::axi_meta::ATTR_AXI_MODULE) {
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
                let digest = axiograph_dsl::digest::axi_digest_v1(&axi_text);
                return Ok(WorldModelAxiInputV1 {
                    kind: WorldModelAxiInputKindV1::CanonicalModuleExport,
                    axi_digest_v1: digest,
                    axi_text,
                    selected_module_name: selected,
                });
            }
            Err(e) => {
                if opts.require_canonical {
                    return Err(anyhow!(
                        "failed to export canonical module `{module_name}`: {e}"
                    ));
                }
            }
        }
    } else if opts.require_canonical {
        return Err(anyhow!(
            "no canonical `.axi` module is available in this snapshot (import a `.axi` module first, or disable require_canonical)"
        ));
    }

    // Fallback: reversible snapshot export (`PathDBExportV1`). This is not ideal as an
    // LLM/world-model input, but can be useful for debugging.
    let axi_text = axiograph_pathdb::axi_export::export_pathdb_to_axi_v1(db)?;
    let digest = axiograph_dsl::digest::axi_digest_v1(&axi_text);
    Ok(WorldModelAxiInputV1 {
        kind: WorldModelAxiInputKindV1::PathdbExportFallback,
        axi_digest_v1: digest,
        axi_text,
        selected_module_name: selected,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let opts = WorldModelAxiInputOptionsV1 {
            module_name: None,
            require_canonical: true,
        };
        let out = export_pathdb_world_model_axi(&db, &opts).expect("export canonical world-model axi");
        assert_eq!(out.kind, WorldModelAxiInputKindV1::CanonicalModuleExport);
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
            require_canonical: true,
        };
        let err = export_pathdb_world_model_axi(&db, &opts).unwrap_err();
        assert!(err.to_string().contains("unknown module"));
    }

    #[test]
    fn require_canonical_fails_without_meta_plane_module() {
        let db = axiograph_pathdb::PathDB::new();
        let opts = WorldModelAxiInputOptionsV1 {
            module_name: None,
            require_canonical: true,
        };
        let err = export_pathdb_world_model_axi(&db, &opts).unwrap_err();
        assert!(err.to_string().contains("no canonical"));
    }
}

