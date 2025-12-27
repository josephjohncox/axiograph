//! Schema-aware relation resolution helpers.
//!
//! Motivation
//! ----------
//! User-facing tooling (REPL/Viz/LLM) often speaks in *surface* relation names
//! that should map onto a canonical `.axi` schema relation.
//!
//! Example:
//! - A user says: "Jamison is a child of Bob"
//! - The canonical schema relation is: `Parent(child: Person, parent: Person, ctx: Context, time: Time)`
//! - We want to accept inputs like `child`, `child_of`, `parent_of`, etc and
//!   deterministically map them to the canonical `Parent` relation with the
//!   correct endpoint orientation.
//!
//! This module provides a small, deterministic "semantic rewrite" layer:
//! - **Case-insensitive** relation name matching (`parent` → `Parent`)
//! - A minimal **alias vocabulary** for high-ROI cases (today: `child`/`parent`)
//!
//! It is intentionally conservative:
//! - only returns a resolution when it is *unambiguous*,
//! - otherwise returns `None` so callers can either reject the input (strict UX)
//!   or fall back to untyped/evidence-only structure (discovery workflows).

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
    /// Optional user-supplied alias that triggered this mapping (for UX/debugging).
    pub alias_used: Option<String>,
}

/// Resolve a relation name against the meta-plane, with a small alias layer.
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
                    alias_used: None,
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
                    alias_used: None,
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
                alias_used: None,
            });
        }
    }
    if exact_matches.len() == 1 {
        return exact_matches.pop();
    }

    // 2) Case-insensitive match (common UX issue: `parent` vs `Parent`).
    let needle = rel_type_input.to_ascii_lowercase();
    let mut ci_matches: Vec<ResolvedSchemaRelation<'_>> = Vec::new();

    let schema_names: Vec<String> = match schema_hint {
        Some(s) if meta.schemas.contains_key(s) => vec![s.to_string()],
        _ if meta.schemas.len() == 1 => meta.schemas.keys().cloned().collect(),
        _ => meta.schemas.keys().cloned().collect(),
    };

    for schema_name in schema_names {
        let schema = meta.schemas.get(&schema_name)?;
        for (name, rel) in &schema.relation_decls {
            if name.to_ascii_lowercase() == needle {
                ci_matches.push(ResolvedSchemaRelation {
                    schema_name: schema_name.clone(),
                    schema,
                    rel_decl: rel,
                    rel_name: rel.name.clone(),
                    orientation: EndpointOrientation::AsIs,
                    alias_used: None,
                });
            }
        }
    }
    if ci_matches.len() == 1 {
        return ci_matches.pop();
    }

    // 3) Minimal alias mapping (semantic rewrite layer).
    //
    // Today we only cover the highest ROI: parent/child phrasing.
    let alias = rel_type_input.to_ascii_lowercase();
    let (orientation, want_child_parent) = match alias.as_str() {
        // "child -> parent"
        "child" | "child_of" | "is_child_of" | "has_parent" | "son_of" | "daughter_of" => {
            (EndpointOrientation::AsIs, true)
        }
        // "parent -> child" (inverse phrasing)
        "parent_of" | "is_parent_of" | "has_child" => (EndpointOrientation::Swap, true),
        _ => (EndpointOrientation::AsIs, false),
    };
    if !want_child_parent {
        return None;
    }

    fn has_fields(rel: &RelationDecl, a: &str, b: &str) -> bool {
        let mut seen_a = false;
        let mut seen_b = false;
        for f in &rel.fields {
            if f.field_name == a {
                seen_a = true;
            }
            if f.field_name == b {
                seen_b = true;
            }
        }
        seen_a && seen_b
    }

    let mut alias_matches: Vec<ResolvedSchemaRelation<'_>> = Vec::new();

    // Search within the hinted schema when present; otherwise accept a unique match
    // across all schemas.
    if let Some(schema_name) = schema_hint {
        if let Some(schema) = meta.schemas.get(schema_name) {
            for rel in schema.relation_decls.values() {
                if has_fields(rel, "child", "parent") {
                    alias_matches.push(ResolvedSchemaRelation {
                        schema_name: schema_name.to_string(),
                        schema,
                        rel_decl: rel,
                        rel_name: rel.name.clone(),
                        orientation,
                        alias_used: Some(rel_type_input.to_string()),
                    });
                }
            }
        }
    } else if meta.schemas.len() == 1 {
        if let Some((schema_name, schema)) = meta.schemas.iter().next() {
            for rel in schema.relation_decls.values() {
                if has_fields(rel, "child", "parent") {
                    alias_matches.push(ResolvedSchemaRelation {
                        schema_name: schema_name.clone(),
                        schema,
                        rel_decl: rel,
                        rel_name: rel.name.clone(),
                        orientation,
                        alias_used: Some(rel_type_input.to_string()),
                    });
                }
            }
        }
    } else {
        for (schema_name, schema) in &meta.schemas {
            for rel in schema.relation_decls.values() {
                if has_fields(rel, "child", "parent") {
                    alias_matches.push(ResolvedSchemaRelation {
                        schema_name: schema_name.clone(),
                        schema,
                        rel_decl: rel,
                        rel_name: rel.name.clone(),
                        orientation,
                        alias_used: Some(rel_type_input.to_string()),
                    });
                }
            }
        }
    }

    if alias_matches.len() == 1 {
        return alias_matches.pop();
    }

    None
}
