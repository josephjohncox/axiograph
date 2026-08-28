//! Schema-aware relation resolution helpers.
//!
//! Motivation
//! ----------
//! User-facing tooling may speak in labels, but mutation and proposal import
//! surfaces must resolve to canonical `.axi` schema relation names before they
//! can create evidence-plane facts.
//!
//! This module is intentionally exact and boring:
//! - no case-insensitive matching,
//! - no semantic aliases such as `parent_of`,
//! - no endpoint swapping.
//!
//! If a user or agent has inverse phrasing, the caller must supply the canonical
//! relation plus explicit endpoint fields. This keeps implicit label rewrites
//! out of the typed semantic spine.

use axiograph_pathdb::axi_semantics::{MetaPlaneIndex, RelationDecl, SchemaIndex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointOrientation {
    /// Keep (source,target) as provided.
    AsIs,
    /// Swap (source,target) before building the canonical fact.
    Swap,
}

#[derive(Debug, Clone)]
pub struct ResolvedSchemaRelation<'a> {
    pub schema_name: String,
    pub schema: &'a SchemaIndex,
    pub rel_decl: &'a RelationDecl,
    /// Canonical relation name (preserves schema casing).
    pub rel_name: String,
    pub orientation: EndpointOrientation,
}

/// Resolve an exact canonical relation name against the meta-plane.
///
/// Returns `None` if no unambiguous resolution exists.
pub fn resolve_schema_relation<'a>(
    meta: &'a MetaPlaneIndex,
    schema_hint: Option<&str>,
    rel_type_input: &str,
) -> Option<ResolvedSchemaRelation<'a>> {
    let rel_type_input = rel_type_input.trim();
    if rel_type_input.is_empty() {
        return None;
    }

    // 1) Exact match first (fast, deterministic).
    if let Some(schema_name) = schema_hint {
        if let Some(schema) = meta.schemas.get(schema_name) {
            if let Some(rel) = schema.relation_decls.get(rel_type_input) {
                return Some(ResolvedSchemaRelation {
                    schema_name: schema_name.to_string(),
                    schema,
                    rel_decl: rel,
                    rel_name: rel.name.clone(),
                    orientation: EndpointOrientation::AsIs,
                });
            }
        }
    }

    if meta.schemas.len() == 1 {
        if let Some((schema_name, schema)) = meta.schemas.iter().next() {
            if let Some(rel) = schema.relation_decls.get(rel_type_input) {
                return Some(ResolvedSchemaRelation {
                    schema_name: schema_name.clone(),
                    schema,
                    rel_decl: rel,
                    rel_name: rel.name.clone(),
                    orientation: EndpointOrientation::AsIs,
                });
            }
        }
    }

    let mut exact_matches: Vec<ResolvedSchemaRelation<'_>> = Vec::new();
    for (schema_name, schema) in &meta.schemas {
        if let Some(rel) = schema.relation_decls.get(rel_type_input) {
            exact_matches.push(ResolvedSchemaRelation {
                schema_name: schema_name.clone(),
                schema,
                rel_decl: rel,
                rel_name: rel.name.clone(),
                orientation: EndpointOrientation::AsIs,
            });
        }
    }
    if exact_matches.len() == 1 {
        return exact_matches.pop();
    }

    None
}
