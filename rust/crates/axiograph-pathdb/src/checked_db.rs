//! “Checked kernel” wrappers for PathDB (Rust-side typechecking boundary).
//!
//! Axiograph’s trusted semantics live in Lean, but the Rust runtime still wants
//! *correct-by-construction* APIs wherever feasible:
//!
//! - avoid constructing ill-typed fact nodes,
//! - surface errors early (before expensive indexing/querying),
//! - and make optimizers/query planners safely assume basic invariants.
//!
//! This module provides:
//! - `CheckedDb`: a read-only wrapper asserting that a `PathDB` snapshot is
//!   well-typed against its meta-plane.
//! - `CheckedDbMut`: a write wrapper with typed builders for safe construction.
//!
//! These checks are **not** the trusted gate (Lean is). They are runtime
//! guardrails and ergonomics.

use crate::axi_meta::{ATTR_AXI_SCHEMA, META_REL_FACT_OF, REL_AXI_FACT_IN_CONTEXT};
use crate::axi_semantics::{AxiTypeCheckReport, MetaPlaneIndex, RelationDecl, SchemaIndex};
use crate::axi_type::TypingEnv;
use crate::PathDB;
use anyhow::{anyhow, Result};
use std::collections::HashMap;

fn entity_attr_string(db: &PathDB, entity: u32, key: &str) -> Option<String> {
    let key_id = db.interner.id_of(key)?;
    let value_id = db.entities.get_attr(entity, key_id)?;
    db.interner.lookup(value_id)
}

#[derive(Debug, Clone, Default)]
pub struct RewriteRuleTypecheckReport {
    pub checked_rules: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ContextInvariantReport {
    pub checked_facts: usize,
    pub checked_scope_edges: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ModalInvariantReport {
    pub checked_edges: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CheckedDbReport {
    pub axi_fact_typecheck: AxiTypeCheckReport,
    pub rewrite_rule_typecheck: RewriteRuleTypecheckReport,
    pub context_invariants: ContextInvariantReport,
    pub modal_invariants: ModalInvariantReport,
    pub ok: bool,
}

impl CheckedDbReport {
    pub fn empty_ok() -> Self {
        Self {
            axi_fact_typecheck: AxiTypeCheckReport::default(),
            rewrite_rule_typecheck: RewriteRuleTypecheckReport::default(),
            context_invariants: ContextInvariantReport::default(),
            modal_invariants: ModalInvariantReport::default(),
            ok: true,
        }
    }
}

/// A read-only PathDB wrapper that has passed basic `.axi` meta-plane checks.
#[derive(Clone)]
pub struct CheckedDb<'db> {
    db: &'db PathDB,
    pub env: TypingEnv,
}

impl<'db> CheckedDb<'db> {
    /// Check the DB and return a report (does not fail-fast).
    pub fn check(db: &'db PathDB) -> Result<CheckedDbReport> {
        let meta = MetaPlaneIndex::from_db(db)?;
        if meta.schemas.is_empty() {
            return Ok(CheckedDbReport::empty_ok());
        }

        let axi_fact_typecheck = meta.typecheck_axi_facts(db);
        let rewrite_rule_typecheck = check_rewrite_rules(db, &meta);
        let context_invariants = check_context_invariants(db, &meta);
        let modal_invariants = check_modal_invariants(db);

        let ok = axi_fact_typecheck.ok()
            && rewrite_rule_typecheck.errors.is_empty()
            && context_invariants.errors.is_empty()
            && modal_invariants.errors.is_empty();
        Ok(CheckedDbReport {
            axi_fact_typecheck,
            rewrite_rule_typecheck,
            context_invariants,
            modal_invariants,
            ok,
        })
    }

    /// Construct a checked wrapper, failing if `.axi` meta-plane typechecks fail.
    pub fn new(db: &'db PathDB) -> Result<Self> {
        let env = TypingEnv::from_db(db)?;
        if env.meta.schemas.is_empty() {
            return Err(anyhow!(
                "cannot construct CheckedDb: no `.axi` schemas present in meta-plane"
            ));
        }

        let report = CheckedDb::check(db)?;
        if !report.ok {
            let mut msg = format!(
                "PathDB failed Rust-side checks (axi_facts_checked={}, axi_fact_errors={}, rewrite_rule_errors={}, context_errors={}, modal_errors={})",
                report.axi_fact_typecheck.checked_facts,
                report.axi_fact_typecheck.errors.len(),
                report.rewrite_rule_typecheck.errors.len(),
                report.context_invariants.errors.len(),
                report.modal_invariants.errors.len(),
            );

            if let Some(first) = report.axi_fact_typecheck.errors.first() {
                msg.push_str(&format!("\nfirst `.axi` fact type error: {first}"));
            } else if let Some(first) = report.rewrite_rule_typecheck.errors.first() {
                msg.push_str(&format!("\nfirst rewrite rule error: {first}"));
            } else if let Some(first) = report.context_invariants.errors.first() {
                msg.push_str(&format!("\nfirst context invariant error: {first}"));
            } else if let Some(first) = report.modal_invariants.errors.first() {
                msg.push_str(&format!("\nfirst modal/approx invariant error: {first}"));
            }
            return Err(anyhow!(msg));
        }

        Ok(Self { db, env })
    }

    pub fn db(&self) -> &'db PathDB {
        self.db
    }
}

fn infer_binary_endpoint_fields(rel_decl: &RelationDecl) -> Option<(&str, &str)> {
    let names: Vec<&str> = rel_decl.fields.iter().map(|f| f.field_name.as_str()).collect();
    if names.contains(&"from") && names.contains(&"to") {
        return Some(("from", "to"));
    }
    if names.contains(&"source") && names.contains(&"target") {
        return Some(("source", "target"));
    }
    if names.contains(&"lhs") && names.contains(&"rhs") {
        return Some(("lhs", "rhs"));
    }
    if names.contains(&"child") && names.contains(&"parent") {
        return Some(("child", "parent"));
    }
    if rel_decl.fields.len() >= 2 {
        return Some((
            rel_decl.fields[0].field_name.as_str(),
            rel_decl.fields[1].field_name.as_str(),
        ));
    }
    None
}

fn check_rewrite_rules(db: &PathDB, meta: &MetaPlaneIndex) -> RewriteRuleTypecheckReport {
    use axiograph_dsl::schema_v1::{parse_path_expr_v3, PathExprV3, RewriteVarTypeV1};
    use std::collections::HashMap;

    #[derive(Debug, Clone)]
    struct RewriteTypingEnv {
        object_vars: HashMap<String, String>,          // x -> Ty
        path_vars: HashMap<String, (String, String)>,  // p -> (from_term, to_term)
    }

    fn infer_expr_endpoints(
        schema_name: &str,
        schema: &SchemaIndex,
        env: &RewriteTypingEnv,
        expr: &PathExprV3,
    ) -> Result<(String, String), String> {
        match expr {
            PathExprV3::Var { name } => env
                .path_vars
                .get(name.as_str())
                .cloned()
                .ok_or_else(|| format!("unbound path variable `{name}`")),
            PathExprV3::Reflexive { entity } => {
                if !env.object_vars.contains_key(entity.as_str()) {
                    return Err(format!("unbound object variable `{entity}`"));
                }
                Ok((entity.to_string(), entity.to_string()))
            }
            PathExprV3::Step { from, rel, to } => {
                if !env.object_vars.contains_key(from.as_str()) {
                    return Err(format!("unbound object variable `{from}`"));
                }
                if !env.object_vars.contains_key(to.as_str()) {
                    return Err(format!("unbound object variable `{to}`"));
                }

                let rel_name = rel.to_string();
                let Some(rel_decl) = schema.relation_decls.get(&rel_name) else {
                    return Err(format!("unknown relation `{schema_name}.{rel_name}`"));
                };
                let Some((src_field, dst_field)) = infer_binary_endpoint_fields(rel_decl) else {
                    return Err(format!(
                        "relation `{schema_name}.{rel_name}` has fewer than 2 fields (cannot infer step endpoints)"
                    ));
                };
                let src_ty = rel_decl
                    .fields
                    .iter()
                    .find(|f| f.field_name == src_field)
                    .map(|f| f.field_type.as_str())
                    .unwrap_or("");
                let dst_ty = rel_decl
                    .fields
                    .iter()
                    .find(|f| f.field_name == dst_field)
                    .map(|f| f.field_type.as_str())
                    .unwrap_or("");

                let from_ty = env
                    .object_vars
                    .get(from.as_str())
                    .map(|s| s.as_str())
                    .unwrap_or("");
                let to_ty = env
                    .object_vars
                    .get(to.as_str())
                    .map(|s| s.as_str())
                    .unwrap_or("");

                if !schema.is_subtype(from_ty, src_ty) {
                    return Err(format!(
                        "step({from},{rel},{to}): `{from}` has type `{from_ty}`, expected subtype of `{src_ty}` (field `{src_field}`)"
                    ));
                }
                if !schema.is_subtype(to_ty, dst_ty) {
                    return Err(format!(
                        "step({from},{rel},{to}): `{to}` has type `{to_ty}`, expected subtype of `{dst_ty}` (field `{dst_field}`)"
                    ));
                }

                Ok((from.to_string(), to.to_string()))
            }
            PathExprV3::Trans { left, right } => {
                let (a, b) = infer_expr_endpoints(schema_name, schema, env, left)?;
                let (c, d) = infer_expr_endpoints(schema_name, schema, env, right)?;
                if b != c {
                    return Err(format!(
                        "trans(...): cannot compose (left ends at `{b}`, right starts at `{c}`)"
                    ));
                }
                Ok((a, d))
            }
            PathExprV3::Inv { path } => {
                let (a, b) = infer_expr_endpoints(schema_name, schema, env, path)?;
                Ok((b, a))
            }
        }
    }

    let mut report = RewriteRuleTypecheckReport::default();

    for (schema_name, schema) in &meta.schemas {
        for (theory_name, rules) in &schema.rewrite_rules_by_theory {
            for rule in rules {
                report.checked_rules += 1;

                if let Some(e) = rule.vars_parse_error.as_ref() {
                    report.errors.push(format!(
                        "{schema_name}.{theory_name}.{}: invalid vars: {e}",
                        rule.name
                    ));
                    continue;
                }

                let mut env = RewriteTypingEnv {
                    object_vars: HashMap::new(),
                    path_vars: HashMap::new(),
                };

                // First, register object vars.
                let mut pending_paths: Vec<(String, String, String)> = Vec::new();
                for v in &rule.vars {
                    let var_name = v.name.to_string();
                    if env.object_vars.contains_key(&var_name) || env.path_vars.contains_key(&var_name) {
                        report.errors.push(format!(
                            "{schema_name}.{theory_name}.{}: duplicate var `{var_name}`",
                            rule.name
                        ));
                        continue;
                    }

                    match &v.ty {
                        RewriteVarTypeV1::Object { ty } => {
                            let ty_name = ty.to_string();
                            if !schema.object_types.contains(&ty_name) {
                                report.errors.push(format!(
                                    "{schema_name}.{theory_name}.{}: unknown object type `{ty_name}` for var `{var_name}`",
                                    rule.name
                                ));
                                continue;
                            }
                            env.object_vars.insert(var_name, ty_name);
                        }
                        RewriteVarTypeV1::Path { from, to } => {
                            pending_paths.push((var_name, from.to_string(), to.to_string()));
                        }
                    }
                }

                // Now resolve path vars against the declared object vars.
                let mut ok = true;
                for (p, from, to) in pending_paths {
                    if !env.object_vars.contains_key(&from) {
                        report.errors.push(format!(
                            "{schema_name}.{theory_name}.{}: path var `{p}` references unknown endpoint `{from}`",
                            rule.name
                        ));
                        ok = false;
                        continue;
                    }
                    if !env.object_vars.contains_key(&to) {
                        report.errors.push(format!(
                            "{schema_name}.{theory_name}.{}: path var `{p}` references unknown endpoint `{to}`",
                            rule.name
                        ));
                        ok = false;
                        continue;
                    }
                    env.path_vars.insert(p, (from, to));
                }
                if !ok {
                    continue;
                }

                let lhs = match parse_path_expr_v3(&rule.lhs) {
                    Ok(v) => v,
                    Err(e) => {
                        report.errors.push(format!(
                            "{schema_name}.{theory_name}.{}: invalid lhs: {e}",
                            rule.name
                        ));
                        continue;
                    }
                };
                let rhs = match parse_path_expr_v3(&rule.rhs) {
                    Ok(v) => v,
                    Err(e) => {
                        report.errors.push(format!(
                            "{schema_name}.{theory_name}.{}: invalid rhs: {e}",
                            rule.name
                        ));
                        continue;
                    }
                };

                let lhs_endpoints = match infer_expr_endpoints(schema_name, schema, &env, &lhs) {
                    Ok(v) => v,
                    Err(e) => {
                        report.errors.push(format!(
                            "{schema_name}.{theory_name}.{}: lhs ill-typed: {e}",
                            rule.name
                        ));
                        continue;
                    }
                };
                let rhs_endpoints = match infer_expr_endpoints(schema_name, schema, &env, &rhs) {
                    Ok(v) => v,
                    Err(e) => {
                        report.errors.push(format!(
                            "{schema_name}.{theory_name}.{}: rhs ill-typed: {e}",
                            rule.name
                        ));
                        continue;
                    }
                };

                if lhs_endpoints != rhs_endpoints {
                    report.errors.push(format!(
                        "{schema_name}.{theory_name}.{}: endpoints mismatch (lhs=Path({},{}) rhs=Path({},{}))",
                        rule.name,
                        lhs_endpoints.0,
                        lhs_endpoints.1,
                        rhs_endpoints.0,
                        rhs_endpoints.1,
                    ));
                }

                // Optional: sanity-check that any referenced evidence anchors (rule_entity) exist.
                if db.get_entity(rule.rule_entity).is_none() {
                    report.errors.push(format!(
                        "{schema_name}.{theory_name}.{}: missing rule entity {} in DB",
                        rule.name, rule.rule_entity
                    ));
                }
            }
        }
    }

    report
}

fn check_context_invariants(db: &PathDB, meta: &MetaPlaneIndex) -> ContextInvariantReport {
    let mut report = ContextInvariantReport::default();

    let Some(scope_rel_id) = db.interner.id_of(REL_AXI_FACT_IN_CONTEXT) else {
        return report;
    };

    // Compute the set of entity types that count as Contexts (including schema-local
    // subtypes of `Context`). This keeps the invariant robust when domains extend
    // the context/world model.
    let mut allowed_context_types: std::collections::HashSet<String> =
        ["Context".to_string(), "World".to_string()].into_iter().collect();
    for schema in meta.schemas.values() {
        for obj in &schema.object_types {
            if schema.is_subtype(obj, "Context") {
                allowed_context_types.insert(obj.clone());
            }
        }
    }

    // Global invariant: every `axi_fact_in_context` edge must target a Context/World.
    for (i, r) in db.relations.relations.iter().enumerate() {
        if r.rel_type != scope_rel_id {
            continue;
        }
        report.checked_scope_edges += 1;
        let Some(type_id) = db.entities.get_type(r.target) else {
            report.errors.push(format!(
                "edge#{i} {src} -{REL_AXI_FACT_IN_CONTEXT}-> {dst}: target has missing type",
                src = r.source,
                dst = r.target
            ));
            continue;
        };
        let Some(type_name) = db.interner.lookup(type_id) else {
            report.errors.push(format!(
                "edge#{i} {src} -{REL_AXI_FACT_IN_CONTEXT}-> {dst}: target has unknown type id {}",
                type_id.raw(),
                src = r.source,
                dst = r.target
            ));
            continue;
        };
        if !allowed_context_types.contains(&type_name) {
            report.errors.push(format!(
                "edge#{i} {src} -{REL_AXI_FACT_IN_CONTEXT}-> {dst}: target must be a Context/World (got `{type_name}`)",
                src = r.source,
                dst = r.target
            ));
        }
    }

    // Additional invariant: when a relation signature contains a `ctx` field,
    // the fact node should have `ctx` and `axi_fact_in_context` pointing to the
    // same context entity.
    let Some(relation_key_id) = db.interner.id_of(crate::axi_meta::ATTR_AXI_RELATION) else {
        return report;
    };
    let Some(relation_col) = db.entities.attrs.get(&relation_key_id) else {
        return report;
    };

    let Some(ctx_rel_id) = db.interner.id_of("ctx") else {
        return report;
    };

    for (&fact_entity, &relation_value_id) in relation_col {
        let Some(relation_name) = db.interner.lookup(relation_value_id) else {
            continue;
        };
        let Some(schema_name) = entity_attr_string(db, fact_entity, ATTR_AXI_SCHEMA) else {
            continue;
        };
        let Some(schema_index) = meta.schemas.get(&schema_name) else {
            continue;
        };
        let Some(rel_decl) = schema_index.relation_decls.get(&relation_name) else {
            continue;
        };

        if !rel_decl.fields.iter().any(|f| f.field_name == "ctx") {
            continue;
        }

        report.checked_facts += 1;

        let ctx_edges = db.relations.outgoing(fact_entity, ctx_rel_id);
        if ctx_edges.len() != 1 {
            // The core typecheck already reports missing/multi ctx; do not duplicate.
            continue;
        }
        let ctx_target = ctx_edges[0].target;
        let scopes = db.relations.outgoing(fact_entity, scope_rel_id);
        if scopes.len() != 1 {
            report.errors.push(format!(
                "fact {fact_entity} ({schema_name}.{relation_name}): expected exactly one `{REL_AXI_FACT_IN_CONTEXT}` edge (found {})",
                scopes.len()
            ));
            continue;
        }
        if scopes[0].target != ctx_target {
            report.errors.push(format!(
                "fact {fact_entity} ({schema_name}.{relation_name}): `{REL_AXI_FACT_IN_CONTEXT}` mismatch (ctx={} axi_fact_in_context={})",
                ctx_target,
                scopes[0].target
            ));
        }
    }

    report
}

fn check_modal_invariants(db: &PathDB) -> ModalInvariantReport {
    let mut report = ModalInvariantReport::default();

    // Proposal confidence bounds (extension-layer hygiene).
    if let Some(key_id) = db.interner.id_of("proposal_confidence") {
        if let Some(col) = db.entities.attrs.get(&key_id) {
            for (&entity_id, &value_id) in col {
                let Some(text) = db.interner.lookup(value_id) else {
                    continue;
                };
                let parsed = text.parse::<f64>().ok();
                let ok = parsed.is_some_and(|v| v.is_finite() && (0.0..=1.0).contains(&v));
                if !ok {
                    report.errors.push(format!(
                        "entity {entity_id}: invalid proposal_confidence `{text}` (expected finite number in [0,1])"
                    ));
                }
            }
        }
    }

    // Proposal hygiene: `proposal_id` <-> `proposal_confidence` pairing and evidence presence.
    let proposal_id_key = db.interner.id_of("proposal_id");
    let proposal_conf_key = db.interner.id_of("proposal_confidence");
    let has_evidence_rel = db.interner.id_of("has_evidence_chunk");

    let mut entities_with_proposal_id: Vec<u32> = Vec::new();
    if let Some(key_id) = proposal_id_key {
        if let Some(col) = db.entities.attrs.get(&key_id) {
            entities_with_proposal_id.extend(col.keys().copied());
        }
    }
    entities_with_proposal_id.sort_unstable();
    entities_with_proposal_id.dedup();

    let mut entities_with_conf: std::collections::HashSet<u32> = std::collections::HashSet::new();
    if let Some(key_id) = proposal_conf_key {
        if let Some(col) = db.entities.attrs.get(&key_id) {
            for (&id, _) in col {
                entities_with_conf.insert(id);
            }
        }
    }

    // Helper: does this entity have any evidence pointer (attr or edge)?
    let has_any_evidence = |db: &PathDB, entity_id: u32| -> bool {
        if let Some(rel_id) = has_evidence_rel {
            if !db.relations.outgoing(entity_id, rel_id).is_empty() {
                return true;
            }
        }
        for i in 0usize..64 {
            let key = format!("evidence_{i}_chunk_id");
            let Some(key_id) = db.interner.id_of(&key) else {
                continue;
            };
            if db.entities.get_attr(entity_id, key_id).is_some() {
                return true;
            }
        }
        false
    };

    for entity_id in &entities_with_proposal_id {
        if !entities_with_conf.contains(entity_id) {
            report.errors.push(format!(
                "entity {entity_id}: has proposal_id but missing proposal_confidence"
            ));
        }
        if !has_any_evidence(db, *entity_id) {
            report.errors.push(format!(
                "entity {entity_id}: proposal has no evidence pointers (expected at least one `evidence_*_chunk_id` attr or `has_evidence_chunk` edge)"
            ));
        }
    }
    for entity_id in &entities_with_conf {
        if !entities_with_proposal_id.binary_search(entity_id).is_ok() {
            report.errors.push(format!(
                "entity {entity_id}: has proposal_confidence but missing proposal_id"
            ));
        }
    }

    // Evidence edge targets must be DocChunks (when present).
    if let Some(rel_id) = has_evidence_rel {
        for (i, r) in db.relations.relations.iter().enumerate() {
            if r.rel_type != rel_id {
                continue;
            }
            let Some(type_id) = db.entities.get_type(r.target) else {
                report.errors.push(format!(
                    "edge#{i} {src} -has_evidence_chunk-> {dst}: target has missing type",
                    src = r.source,
                    dst = r.target
                ));
                continue;
            };
            let Some(type_name) = db.interner.lookup(type_id) else {
                report.errors.push(format!(
                    "edge#{i} {src} -has_evidence_chunk-> {dst}: target has unknown type id {}",
                    type_id.raw(),
                    src = r.source,
                    dst = r.target
                ));
                continue;
            };
            if type_name != "DocChunk" {
                report.errors.push(format!(
                    "edge#{i} {src} -has_evidence_chunk-> {dst}: target must be a DocChunk (got `{type_name}`)",
                    src = r.source,
                    dst = r.target
                ));
            }
        }
    }

    // Relation confidence bounds.
    for (i, r) in db.relations.relations.iter().enumerate() {
        report.checked_edges += 1;
        if !r.confidence.is_finite() || !(0.0..=1.0).contains(&r.confidence) {
            let rel_name = db.interner.lookup(r.rel_type).unwrap_or_else(|| "<rel?>".to_string());
            report.errors.push(format!(
                "edge#{i} {src} -{rel_name}-> {dst}: invalid confidence {} (expected finite number in [0,1])",
                r.confidence,
                src = r.source,
                dst = r.target
            ));
        }
    }

    report
}

/// A write wrapper that pairs a mutable `PathDB` with a fixed typing environment.
///
/// This is the entry point for “typed by construction” builders.
pub struct CheckedDbMut<'db> {
    db: &'db mut PathDB,
    pub env: TypingEnv,
}

impl<'db> CheckedDbMut<'db> {
    pub fn new(db: &'db mut PathDB) -> Result<Self> {
        // Build the typing environment from an immutable view first.
        let env = {
            let view: &PathDB = &*db;
            TypingEnv::from_db(view)?
        };
        if env.meta.schemas.is_empty() {
            return Err(anyhow!(
                "cannot construct CheckedDbMut: no `.axi` schemas present in meta-plane"
            ));
        }

        Ok(Self { db, env })
    }

    pub fn db(&self) -> &PathDB {
        &*self.db
    }

    pub fn db_mut(&mut self) -> &mut PathDB {
        &mut *self.db
    }

    pub fn schema(&self, schema_name: &str) -> Result<&SchemaIndex> {
        self.env
            .meta
            .schemas
            .get(schema_name)
            .ok_or_else(|| anyhow!("unknown schema `{schema_name}`"))
    }

    pub fn relation_decl(&self, schema_name: &str, relation_name: &str) -> Result<&RelationDecl> {
        let schema = self.schema(schema_name)?;
        schema
            .relation_decls
            .get(relation_name)
            .ok_or_else(|| anyhow!("unknown relation `{relation_name}` in schema `{schema_name}`"))
    }

    /// Start constructing a schema-scoped object entity.
    ///
    /// This is the “typed by construction” alternative to calling `PathDB::add_entity`
    /// directly when the entity is intended to live in the canonical `.axi` universe.
    pub fn entity_builder<'a>(
        &'a mut self,
        schema_name: &str,
        type_name: &str,
    ) -> Result<TypedEntityBuilder<'a>> {
        let schema = self
            .env
            .meta
            .schemas
            .get(schema_name)
            .ok_or_else(|| anyhow!("unknown schema `{schema_name}`"))?
            .clone();

        if !schema.object_types.contains(type_name) {
            return Err(anyhow!(
                "unknown object type `{}` in schema `{}`",
                type_name,
                schema_name
            ));
        }

        Ok(TypedEntityBuilder {
            db: &mut *self.db,
            schema_name: schema_name.to_string(),
            schema,
            type_name: type_name.to_string(),
            attrs: Vec::new(),
        })
    }

    /// Add a generic edge with basic runtime checks (existence + confidence bounds).
    ///
    /// This is intended for evidence-plane / tooling edges that are not reified
    /// as `.axi` relation tuples.
    pub fn add_edge_checked(
        &mut self,
        rel_type: &str,
        source: u32,
        target: u32,
        confidence: f32,
        attrs: Vec<(&str, &str)>,
    ) -> Result<bool> {
        if self.db.get_entity(source).is_none() {
            return Err(anyhow!("add_edge: missing source entity {source}"));
        }
        if self.db.get_entity(target).is_none() {
            return Err(anyhow!("add_edge: missing target entity {target}"));
        }
        if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
            return Err(anyhow!(
                "add_edge: confidence must be a finite number in [0,1] (got {confidence})"
            ));
        }

        // Enforce a core invariant: `axi_fact_in_context` must target a Context.
        if rel_type == REL_AXI_FACT_IN_CONTEXT {
            let Some(type_id) = self.db.entities.get_type(target) else {
                return Err(anyhow!(
                    "add_edge: `{REL_AXI_FACT_IN_CONTEXT}` target entity {target} has missing type"
                ));
            };
            let Some(type_name) = self.db.interner.lookup(type_id) else {
                return Err(anyhow!(
                    "add_edge: `{REL_AXI_FACT_IN_CONTEXT}` target entity {target} has unknown type id {}",
                    type_id.raw()
                ));
            };
            if type_name != "Context" && type_name != "World" {
                return Err(anyhow!(
                    "add_edge: `{REL_AXI_FACT_IN_CONTEXT}` target must be a Context/World (got `{type_name}` for entity {target})"
                ));
            }
        }

        let rel_id = self.db.interner.intern(rel_type);
        if self.db.relations.has_edge(source, rel_id, target) {
            return Ok(false);
        }

        self.db.add_relation(rel_type, source, target, confidence, attrs);
        Ok(true)
    }

    /// Start constructing a well-typed relation tuple (fact node).
    pub fn fact_builder<'a>(
        &'a mut self,
        schema_name: &str,
        relation_name: &str,
    ) -> Result<TypedFactBuilder<'a>> {
        let schema = self
            .env
            .meta
            .schemas
            .get(schema_name)
            .ok_or_else(|| anyhow!("unknown schema `{schema_name}`"))?
            .clone();
        let rel_decl = schema
            .relation_decls
            .get(relation_name)
            .ok_or_else(|| anyhow!("unknown relation `{relation_name}` in schema `{schema_name}`"))?
            .clone();

        Ok(TypedFactBuilder {
            db: &mut *self.db,
            schema_name: schema_name.to_string(),
            schema,
            relation: relation_name.to_string(),
            decl: rel_decl,
            field_values: HashMap::new(),
            fact_attrs: Vec::new(),
            edge_confidence: 1.0,
        })
    }
}

/// A typed object-entity builder that enforces schema scoping and object-type existence.
pub struct TypedEntityBuilder<'db> {
    db: &'db mut PathDB,
    schema_name: String,
    #[allow(dead_code)]
    schema: SchemaIndex,
    type_name: String,
    attrs: Vec<(String, String)>,
}

impl<'db> TypedEntityBuilder<'db> {
    /// Add an entity attribute (e.g. `name`, `iri`, `comment`).
    pub fn with_attr(mut self, key: &str, value: &str) -> Self {
        self.attrs.push((key.to_string(), value.to_string()));
        self
    }

    /// Commit the entity into the DB, returning its entity id.
    pub fn commit(mut self) -> Result<u32> {
        // Ensure schema scoping is always present for typed entities.
        if !self
            .attrs
            .iter()
            .any(|(k, _)| k.as_str() == ATTR_AXI_SCHEMA)
        {
            self.attrs
                .push((ATTR_AXI_SCHEMA.to_string(), self.schema_name.clone()));
        }

        let attrs_ref: Vec<(&str, &str)> = self
            .attrs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        Ok(self.db.add_entity(&self.type_name, attrs_ref))
    }
}

/// A typed fact-node builder that enforces schema typing rules during construction.
pub struct TypedFactBuilder<'db> {
    db: &'db mut PathDB,
    schema_name: String,
    schema: SchemaIndex,
    relation: String,
    decl: RelationDecl,
    field_values: HashMap<String, u32>,
    fact_attrs: Vec<(String, String)>,
    edge_confidence: f32,
}

impl<'db> TypedFactBuilder<'db> {
    /// Add an attribute to the fact node (e.g. `name`, `axi_fact_id`, provenance pointers).
    pub fn with_attr(mut self, key: &str, value: &str) -> Self {
        self.fact_attrs.push((key.to_string(), value.to_string()));
        self
    }

    /// Set the confidence used for field edges (default = 1.0).
    pub fn with_edge_confidence(mut self, confidence: f32) -> Self {
        self.edge_confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Set a field value, checking:
    /// - the field exists in the relation signature, and
    /// - the value entity is schema-scoped and has an allowed type (with subtyping).
    pub fn set_field(&mut self, field: &str, value: u32) -> Result<()> {
        let Some(field_decl) = self.decl.fields.iter().find(|f| f.field_name == field) else {
            return Err(anyhow!(
                "unknown field `{field}` for relation `{}` (schema `{}`)",
                self.relation,
                self.schema_name
            ));
        };

        // Adopt schema scoping for previously-unscoped entities (e.g. evidence-plane stubs).
        // This keeps the *typed* construction path correct-by-construction without forcing
        // every upstream importer to eagerly stamp `axi_schema`.
        let actual_schema = match entity_attr_string(self.db, value, ATTR_AXI_SCHEMA) {
            Some(s) => s,
            None => {
                self.db
                    .upsert_entity_attr(value, ATTR_AXI_SCHEMA, &self.schema_name)?;
                self.schema_name.clone()
            }
        };
        if actual_schema != self.schema_name {
            return Err(anyhow!(
                "field `{field}` for relation `{}`: schema mismatch (expected `{}`, got `{}` for entity {value})",
                self.relation,
                self.schema_name,
                actual_schema
            ));
        }

        let Some(type_id) = self.db.entities.get_type(value) else {
            return Err(anyhow!(
                "field `{field}` for relation `{}`: value entity {value} has missing type",
                self.relation
            ));
        };
        let Some(actual_type) = self.db.interner.lookup(type_id) else {
            return Err(anyhow!(
                "field `{field}` for relation `{}`: value entity {value} has unknown type id {}",
                self.relation,
                type_id.raw()
            ));
        };

        if !self.schema.is_subtype(&actual_type, &field_decl.field_type) {
            return Err(anyhow!(
                "field `{field}` for relation `{}`: expected `{}` but got `{}` (entity {value})",
                self.relation,
                field_decl.field_type,
                actual_type
            ));
        }

        if self
            .field_values
            .insert(field.to_string(), value)
            .is_some()
        {
            return Err(anyhow!(
                "duplicate assignment for field `{field}` in relation `{}`",
                self.relation
            ));
        }

        Ok(())
    }

    /// Commit the fact node into the DB, returning its entity id.
    pub fn commit(mut self) -> Result<u32> {
        // Ensure all declared fields are present.
        for f in &self.decl.fields {
            if !self.field_values.contains_key(&f.field_name) {
                return Err(anyhow!(
                    "missing field `{}` for relation `{}` (schema `{}`)",
                    f.field_name,
                    self.relation,
                    self.schema_name
                ));
            }
        }

        // Canonical fact-node entity type name.
        let tuple_type = self.schema.tuple_entity_type_name(&self.relation);

        // Default name: stable hash of (schema, relation, ordered field ids).
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(self.schema_name.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(self.relation.as_bytes());
        bytes.push(0);
        for f in &self.decl.fields {
            bytes.extend_from_slice(f.field_name.as_bytes());
            bytes.push(b'=');
            let id = self
                .field_values
                .get(&f.field_name)
                .copied()
                .expect("checked above");
            bytes.extend_from_slice(id.to_string().as_bytes());
            bytes.push(0);
        }
        let digest = axiograph_dsl::digest::fnv1a64_digest_bytes(&bytes);
        let default_name = format!("{}_fact_{}", self.relation, digest);

        // Build attrs.
        let mut attrs: Vec<(String, String)> = Vec::new();
        attrs.push(("name".to_string(), default_name));
        attrs.push((ATTR_AXI_SCHEMA.to_string(), self.schema_name.clone()));
        attrs.push((crate::axi_meta::ATTR_AXI_RELATION.to_string(), self.relation.clone()));
        attrs.extend(self.fact_attrs.drain(..));
        let attrs_ref: Vec<(&str, &str)> = attrs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        let fact = self.db.add_entity(&tuple_type, attrs_ref);
        self.db.mark_virtual_type(fact, "FactNode")?;

        // Link fact node to its relation declaration (meta-plane).
        self.db.add_relation(META_REL_FACT_OF, fact, self.decl.relation_entity, 1.0, vec![]);

        // Field edges + derived uniform context edge.
        for f in &self.decl.fields {
            let value = self
                .field_values
                .get(&f.field_name)
                .copied()
                .expect("checked above");
            self.db
                .add_relation(&f.field_name, fact, value, self.edge_confidence, vec![]);
            if f.field_name == "ctx" {
                self.db.add_relation(
                    REL_AXI_FACT_IN_CONTEXT,
                    fact,
                    value,
                    self.edge_confidence,
                    vec![],
                );
            }
        }

        Ok(fact)
    }

    /// Apply the builder to an existing fact node, enforcing:
    /// - all declared fields are present (and unique),
    /// - existing field edges do not conflict,
    /// - required meta attrs exist (`axi_schema`, `axi_relation`),
    /// - and the `axi_fact_in_context` invariant holds when a `ctx` field exists.
    pub fn commit_into_existing(mut self, fact_id: u32) -> Result<u32> {
        // Ensure all declared fields are present.
        for f in &self.decl.fields {
            if !self.field_values.contains_key(&f.field_name) {
                return Err(anyhow!(
                    "missing field `{}` for relation `{}` (schema `{}`)",
                    f.field_name,
                    self.relation,
                    self.schema_name
                ));
            }
        }

        // Ensure required meta attrs are present and consistent.
        if let Some(existing_schema) = entity_attr_string(self.db, fact_id, ATTR_AXI_SCHEMA) {
            if existing_schema != self.schema_name {
                return Err(anyhow!(
                    "fact {fact_id}: schema mismatch (expected `{}`, got `{}`)",
                    self.schema_name,
                    existing_schema
                ));
            }
        } else {
            self.db
                .upsert_entity_attr(fact_id, ATTR_AXI_SCHEMA, &self.schema_name)?;
        }
        if let Some(existing_rel) =
            entity_attr_string(self.db, fact_id, crate::axi_meta::ATTR_AXI_RELATION)
        {
            if existing_rel != self.relation {
                return Err(anyhow!(
                    "fact {fact_id}: relation mismatch (expected `{}`, got `{}`)",
                    self.relation,
                    existing_rel
                ));
            }
        } else {
            self.db.upsert_entity_attr(
                fact_id,
                crate::axi_meta::ATTR_AXI_RELATION,
                &self.relation,
            )?;
        }

        // Attach extra attrs (best-effort: fill missing only).
        for (k, v) in self.fact_attrs.drain(..) {
            if entity_attr_string(self.db, fact_id, &k).is_none() {
                self.db.upsert_entity_attr(fact_id, &k, &v)?;
            }
        }

        self.db.mark_virtual_type(fact_id, "FactNode")?;

        // Link fact node to its relation declaration (meta-plane).
        let rel_id = self.db.interner.intern(META_REL_FACT_OF);
        if !self
            .db
            .relations
            .has_edge(fact_id, rel_id, self.decl.relation_entity)
        {
            self.db.add_relation(
                META_REL_FACT_OF,
                fact_id,
                self.decl.relation_entity,
                1.0,
                vec![],
            );
        }

        // Field edges + derived uniform context edge.
        for f in &self.decl.fields {
            let value = self
                .field_values
                .get(&f.field_name)
                .copied()
                .expect("checked above");
            let Some(field_rel_id) = self.db.interner.id_of(&f.field_name) else {
                return Err(anyhow!(
                    "fact {fact_id}: missing interned relation id for field `{}`",
                    f.field_name
                ));
            };
            let outgoing = self.db.relations.outgoing(fact_id, field_rel_id);
            match outgoing.len() {
                0 => {
                    self.db.add_relation(
                        &f.field_name,
                        fact_id,
                        value,
                        self.edge_confidence,
                        vec![],
                    );
                }
                1 => {
                    if outgoing[0].target != value {
                        return Err(anyhow!(
                            "fact {fact_id}: conflicting value for field `{}` (existing={}, new={})",
                            f.field_name,
                            outgoing[0].target,
                            value
                        ));
                    }
                }
                _ => {
                    return Err(anyhow!(
                        "fact {fact_id}: multiple values already present for field `{}`",
                        f.field_name
                    ));
                }
            }

            if f.field_name == "ctx" {
                let Some(scope_rel_id) = self.db.interner.id_of(REL_AXI_FACT_IN_CONTEXT) else {
                    return Err(anyhow!(
                        "fact {fact_id}: missing interned relation id for `{REL_AXI_FACT_IN_CONTEXT}`"
                    ));
                };
                let scopes = self.db.relations.outgoing(fact_id, scope_rel_id);
                match scopes.len() {
                    0 => {
                        self.db.add_relation(
                            REL_AXI_FACT_IN_CONTEXT,
                            fact_id,
                            value,
                            self.edge_confidence,
                            vec![],
                        );
                    }
                    1 => {
                        if scopes[0].target != value {
                            return Err(anyhow!(
                                "fact {fact_id}: `{REL_AXI_FACT_IN_CONTEXT}` mismatch (ctx={}, axi_fact_in_context={})",
                                value,
                                scopes[0].target
                            ));
                        }
                    }
                    _ => {
                        return Err(anyhow!(
                            "fact {fact_id}: multiple `{REL_AXI_FACT_IN_CONTEXT}` edges present"
                        ));
                    }
                }
            }
        }

        Ok(fact_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_fact_builder_requires_all_fields_and_schema_scoping() -> Result<()> {
        let mut db = PathDB::new();
        let axi = r#"
module Demo

schema S:
  object Person
  relation Parent(parent: Person, child: Person)

instance I of S:
  Person = {Alice, Bob}
  Parent = {(parent=Alice, child=Bob)}
"#;
        crate::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();

        // Find existing object entities in schema `S`.
        let alice = db
            .find_by_axi_type("S", "Person")
            .iter()
            .find(|id| db.get_entity(*id).map(|e| e.attrs.get("name").is_some_and(|n| n == "Alice")).unwrap_or(false))
            .ok_or_else(|| anyhow!("missing Alice"))?;
        let bob = db
            .find_by_axi_type("S", "Person")
            .iter()
            .find(|id| db.get_entity(*id).map(|e| e.attrs.get("name").is_some_and(|n| n == "Bob")).unwrap_or(false))
            .ok_or_else(|| anyhow!("missing Bob"))?;

        let mut checked = CheckedDbMut::new(&mut db)?;
        let mut builder = checked.fact_builder("S", "Parent")?;
        builder.set_field("parent", alice)?;

        // Missing field should fail.
        let err = builder
            .clone_for_test()
            .commit()
            .expect_err("missing child should be rejected");
        assert!(err.to_string().contains("missing field `child`"));

        // Now construct successfully.
        builder.set_field("child", bob)?;
        let fact = builder.commit()?;
        let meta = MetaPlaneIndex::from_db(&db)?;
        assert!(meta.typecheck_axi_facts(&db).ok());
        assert!(db.get_entity(fact).is_some());
        Ok(())
    }

    #[test]
    fn meta_plane_rewrite_rule_vars_parse_path_endpoints() -> Result<()> {
        let mut db = PathDB::new();
        let axi = r#"
module RewriteVars

schema S:
  object Person
  relation Parent(parent: Person, child: Person)

theory T on S:
  rewrite inv_inv:
    orientation: forward
    vars: x: Person, y: Person, p: Path(x,y)
    lhs: inv(inv(p))
    rhs: p

instance I of S:
  Person = {Alice, Bob}
  Parent = {(parent=Alice, child=Bob)}
"#;
        crate::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();

        let meta = MetaPlaneIndex::from_db(&db)?;
        let schema = meta.schemas.get("S").ok_or_else(|| anyhow!("missing schema S"))?;
        let rules = schema
            .rewrite_rules_by_theory
            .get("T")
            .ok_or_else(|| anyhow!("missing theory T rewrite rules"))?;
        let rule = rules
            .iter()
            .find(|r| r.name == "inv_inv")
            .ok_or_else(|| anyhow!("missing rewrite rule inv_inv"))?;

        assert!(
            rule.vars_parse_error.is_none(),
            "unexpected vars_parse_error: {:?}",
            rule.vars_parse_error
        );
        assert_eq!(rule.vars.len(), 3, "expected 3 vars (x,y,p)");
        Ok(())
    }

    #[test]
    fn checked_db_reports_ill_typed_rewrite_rule() -> Result<()> {
        let mut db = PathDB::new();
        let axi = r#"
module BadRewrite

schema S:
  object Person

theory T on S:
  rewrite bad_endpoints:
    orientation: forward
    vars: x: Person, p: Path(x,y)
    lhs: p
    rhs: p

instance I of S:
  Person = {Alice}
"#;
        crate::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();

        let report = CheckedDb::check(&db)?;
        assert!(!report.ok, "expected checked_db to report errors");
        assert!(
            report
                .rewrite_rule_typecheck
                .errors
                .iter()
                .any(|e| e.contains("unknown endpoint `y`")),
            "expected unknown endpoint error, got: {:?}",
            report.rewrite_rule_typecheck.errors
        );
        Ok(())
    }

    #[test]
    fn typed_entity_builder_stamps_schema_and_checks_object_type() -> Result<()> {
        let mut db = PathDB::new();
        let axi = r#"
module Demo

schema S:
  object Person

instance I of S:
  Person = {Alice}
"#;
        crate::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();

        let mut checked = CheckedDbMut::new(&mut db)?;

        let id = checked
            .entity_builder("S", "Person")?
            .with_attr("name", "Bob")
            .commit()?;
        let view = checked
            .db()
            .get_entity(id)
            .ok_or_else(|| anyhow!("missing entity {id}"))?;
        assert_eq!(view.entity_type, "Person");
        assert_eq!(view.attrs.get("axi_schema").map(|s| s.as_str()), Some("S"));
        assert_eq!(view.attrs.get("name").map(|s| s.as_str()), Some("Bob"));

        match checked.entity_builder("S", "NotAType") {
            Ok(_) => return Err(anyhow!("expected unknown object type to be rejected")),
            Err(e) => assert!(e.to_string().contains("unknown object type")),
        }
        Ok(())
    }

    // Helper for tests: cloning a builder is tricky because it owns &mut PathDB.
    // We implement a tiny “test-only” clone by rebuilding from the current state.
    trait CloneForTest {
        fn clone_for_test(&mut self) -> TypedFactBuilder<'_>;
    }

    impl<'db> CloneForTest for TypedFactBuilder<'db> {
        fn clone_for_test(&mut self) -> TypedFactBuilder<'_> {
            TypedFactBuilder {
                db: &mut *self.db,
                schema_name: self.schema_name.clone(),
                schema: self.schema.clone(),
                relation: self.relation.clone(),
                decl: self.decl.clone(),
                field_values: self.field_values.clone(),
                fact_attrs: self.fact_attrs.clone(),
                edge_confidence: self.edge_confidence,
            }
        }
    }
}
