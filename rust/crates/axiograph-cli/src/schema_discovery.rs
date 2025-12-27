//! Schema discovery / draft-module generation.
//!
//! This module turns a generic `proposals.json` file (Evidence/Proposals schema)
//! into a **candidate** canonical `.axi` module (`axi_v1`).
//!
//! Why this exists:
//! - AxQL query planning becomes much smarter when the `.axi` meta-plane is present
//!   (field typing, keys/functionals as hints, fact-node indexing).
//! - Many ingestion sources are already structured (SQL DDL, proto descriptors, JSON),
//!   but they first land in the evidence plane as `proposals.json`.
//! - This command gives us an automated “ontology engineering” starting point:
//!   produce a readable draft schema+instance module that can be imported into PathDB,
//!   queried, and iterated on.
//!
//! Important: the output is **untrusted** and intended for review. Any “constraints”
//! inferred here are *extensional* (based on current data) and should be treated as
//! hypotheses until promoted into the accepted `.axi` plane.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write;

use anyhow::{anyhow, Result};

use axiograph_ingest_docs::{ProposalV1, ProposalsFileV1};

#[derive(Debug, Clone)]
pub struct DraftAxiModuleOptions {
    pub module_name: String,
    pub schema_name: String,
    pub instance_name: String,
    /// If true, add a small theory block with extensional constraints inferred
    /// from the observed relation tuples (keys + simple functionals).
    pub infer_constraints: bool,
}

/// Optional structure suggestions (typically LLM-driven) to make draft schemas more readable.
///
/// These are **untrusted** hints intended for review/promotion. The draft module generator
/// applies basic guardrails:
/// - only references existing discovered types/relations,
/// - de-duplicates repeated suggestions,
/// - drops subtype suggestions that would introduce cycles.
#[derive(Debug, Clone, Default)]
pub struct DraftAxiModuleSuggestions {
    pub subtypes: Vec<SuggestedSubtype>,
    pub constraints: Vec<SuggestedConstraint>,
}

#[derive(Debug, Clone)]
pub struct SuggestedSubtype {
    pub sub: String,
    pub sup: String,
    pub public_rationale: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SuggestedConstraint {
    /// Expected: `symmetric` or `transitive` (other kinds are ignored for now).
    pub kind: String,
    pub relation: String,
    pub public_rationale: Option<String>,
}

#[derive(Debug, Clone)]
struct EntityRec {
    entity_id: String,
    entity_type_raw: String,
    entity_type_axi: String,
    name_raw: String,
}

#[derive(Debug, Clone)]
struct RelationRec {
    rel_type_raw: String,
    rel_type_axi: String,
    source_id: String,
    target_id: String,
    context_id: Option<String>,
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

pub(crate) fn sanitize_axi_ident(s: &str) -> String {
    let mut out = String::new();
    let mut prev_underscore = false;

    for c in s.trim().chars() {
        let c = if is_ident_continue(c) { c } else { '_' };
        if c == '_' {
            if prev_underscore {
                continue;
            }
            prev_underscore = true;
        } else {
            prev_underscore = false;
        }
        out.push(c);
        if out.len() >= 120 {
            break;
        }
    }

    let out = out.trim_matches('_').to_string();
    let mut out = if out.is_empty() { "_".to_string() } else { out };
    if !out.chars().next().is_some_and(is_ident_start) {
        out.insert(0, '_');
    }
    out
}

fn uniq_name(used: &mut HashSet<String>, base: &str) -> String {
    let base = if base.is_empty() { "_" } else { base };
    if used.insert(base.to_string()) {
        return base.to_string();
    }
    for i in 2.. {
        let candidate = format!("{base}_{i}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        if i > 10_000 {
            // Safety valve; should never happen for reasonable inputs.
            return format!("{base}_overflow");
        }
    }
    unreachable!("infinite loop above has a return");
}

pub fn draft_axi_module_from_proposals(
    file: &ProposalsFileV1,
    options: &DraftAxiModuleOptions,
) -> Result<String> {
    draft_axi_module_from_proposals_with_suggestions(file, options, None)
}

pub fn draft_axi_module_from_proposals_with_suggestions(
    file: &ProposalsFileV1,
    options: &DraftAxiModuleOptions,
    suggestions: Option<&DraftAxiModuleSuggestions>,
) -> Result<String> {
    // 1) Collect entities.
    let mut entities_by_id: BTreeMap<String, EntityRec> = BTreeMap::new();
    for p in &file.proposals {
        let ProposalV1::Entity {
            entity_id,
            entity_type,
            name,
            ..
        } = p
        else {
            continue;
        };

        let entity_type_axi = sanitize_axi_ident(entity_type);
        entities_by_id.insert(
            entity_id.clone(),
            EntityRec {
                entity_id: entity_id.clone(),
                entity_type_raw: entity_type.clone(),
                entity_type_axi,
                name_raw: name.clone(),
            },
        );
    }

    // 2) Collect relations and ensure endpoints exist (importer semantics).
    let mut relations: Vec<RelationRec> = Vec::new();
    for p in &file.proposals {
        let ProposalV1::Relation {
            rel_type,
            source,
            target,
            attributes,
            ..
        } = p
        else {
            continue;
        };

        let context_id = attributes.get("context").cloned();
        relations.push(RelationRec {
            rel_type_raw: rel_type.clone(),
            rel_type_axi: sanitize_axi_ident(rel_type),
            source_id: source.clone(),
            target_id: target.clone(),
            context_id: context_id.clone(),
        });

        for endpoint in [source, target] {
            if entities_by_id.contains_key(endpoint) {
                continue;
            }
            // Synthetic placeholder: treat as an untyped entity.
            entities_by_id.insert(
                endpoint.clone(),
                EntityRec {
                    entity_id: endpoint.clone(),
                    entity_type_raw: "Entity".to_string(),
                    entity_type_axi: "Entity".to_string(),
                    name_raw: endpoint.clone(),
                },
            );
        }

        // If the relation carries an explicit context, ensure the context entity exists.
        if let Some(ctx_id) = context_id {
            if !entities_by_id.contains_key(&ctx_id) {
                entities_by_id.insert(
                    ctx_id.clone(),
                    EntityRec {
                        entity_id: ctx_id.clone(),
                        entity_type_raw: "Context".to_string(),
                        entity_type_axi: "Context".to_string(),
                        name_raw: ctx_id.clone(),
                    },
                );
            }
        }
    }

    // 3) Assign globally unique `.axi` identifiers for each entity.
    //
    // NOTE: `.axi` element names live in a shared namespace across object types.
    // We therefore enforce uniqueness across *all* entities, not per type.
    let mut used_names: HashSet<String> = HashSet::new();
    let mut entity_axi_name: HashMap<String, String> = HashMap::new();

    let mut entities_sorted: Vec<&EntityRec> = entities_by_id.values().collect();
    entities_sorted.sort_by(|a, b| {
        (a.name_raw.as_str(), a.entity_id.as_str())
            .cmp(&(b.name_raw.as_str(), b.entity_id.as_str()))
    });

    for e in entities_sorted {
        let base = sanitize_axi_ident(&e.name_raw);
        let name = uniq_name(&mut used_names, &base);
        entity_axi_name.insert(e.entity_id.clone(), name);
    }

    // 4) Object types and memberships.
    let mut object_types: BTreeSet<String> = BTreeSet::new();
    object_types.insert("Entity".to_string());

    let mut members_by_type: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for e in entities_by_id.values() {
        object_types.insert(e.entity_type_axi.clone());
        let Some(name) = entity_axi_name.get(&e.entity_id).cloned() else {
            continue;
        };
        members_by_type
            .entry(e.entity_type_axi.clone())
            .or_default()
            .insert(name);
    }

    // 5) Per-relation observed endpoint types (for field typing).
    let mut rel_src_types: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut rel_dst_types: HashMap<String, BTreeSet<String>> = HashMap::new();

    // Also gather instance tuples.
    //
    // If the proposals include an explicit `context` attribute on relations
    // (e.g. from RDF named graphs), we preserve it by:
    // - adding `@context Context` to the relation declaration, and
    // - emitting tuples with `ctx=...`.
    let mut rel_tuples: BTreeMap<String, BTreeSet<(String, String)>> = BTreeMap::new();
    let mut rel_tuples_ctx: BTreeMap<String, BTreeSet<(String, String, String)>> = BTreeMap::new();
    let mut rel_has_context: HashSet<String> = HashSet::new();
    for r in &relations {
        let src_ty = entities_by_id
            .get(&r.source_id)
            .map(|e| e.entity_type_axi.clone())
            .unwrap_or_else(|| "Entity".to_string());
        let dst_ty = entities_by_id
            .get(&r.target_id)
            .map(|e| e.entity_type_axi.clone())
            .unwrap_or_else(|| "Entity".to_string());

        rel_src_types
            .entry(r.rel_type_axi.clone())
            .or_default()
            .insert(src_ty);
        rel_dst_types
            .entry(r.rel_type_axi.clone())
            .or_default()
            .insert(dst_ty);

        let Some(src_name) = entity_axi_name.get(&r.source_id).cloned() else {
            continue;
        };
        let Some(dst_name) = entity_axi_name.get(&r.target_id).cloned() else {
            continue;
        };

        if let Some(ctx_id) = &r.context_id {
            let Some(ctx_name) = entity_axi_name.get(ctx_id).cloned() else {
                continue;
            };
            rel_has_context.insert(r.rel_type_axi.clone());
            rel_tuples_ctx
                .entry(r.rel_type_axi.clone())
                .or_default()
                .insert((src_name, dst_name, ctx_name));
        } else {
            rel_tuples
                .entry(r.rel_type_axi.clone())
                .or_default()
                .insert((src_name, dst_name));
        }
    }

    // 6) Emit `.axi`.
    let mut out = String::new();

    writeln!(
        &mut out,
        "-- Draft `.axi` module generated from `proposals.json`.\n--\n-- This output is *untrusted* (evidence-plane). Review before promotion.\n--\n-- Design notes:\n-- - Entities become object inhabitants.\n-- - Relations become binary tuples: `Rel(from, to)`.\n-- - If proposals include a `context` attribute on relations, we preserve it:\n--     - relation decls gain `@context Context`\n--     - tuples add `ctx=...`\n-- - `Entity` is a supertype so heterogeneous endpoints remain well-typed.\n-- - Optional constraints are inferred *extensionally* from current tuples.\n"
    )?;

    writeln!(&mut out, "module {}", options.module_name)?;
    writeln!(&mut out)?;

    // Schema.
    writeln!(&mut out, "schema {}:", options.schema_name)?;
    writeln!(
        &mut out,
        "  -- Supertype used as a safe fallback for heterogeneous endpoints."
    )?;
    writeln!(&mut out, "  object Entity")?;

    // Objects.
    writeln!(&mut out)?;
    writeln!(
        &mut out,
        "  -- Object types observed in proposals (entity_type → object)."
    )?;
    for ty in object_types.iter().filter(|t| t.as_str() != "Entity") {
        writeln!(&mut out, "  object {ty}")?;
    }

    writeln!(&mut out)?;
    writeln!(
        &mut out,
        "  -- Each object type is a subtype of `Entity` (so relations can use `Entity`)."
    )?;
    for ty in object_types.iter().filter(|t| t.as_str() != "Entity") {
        writeln!(&mut out, "  subtype {ty} < Entity")?;
    }

    // Extra subtyping (optional, untrusted).
    if let Some(suggestions) = suggestions {
        // Keep output deterministic: sort suggestions first.
        let mut subtypes: Vec<&SuggestedSubtype> = suggestions.subtypes.iter().collect();
        subtypes.sort_by(|a, b| {
            (a.sub.as_str(), a.sup.as_str()).cmp(&(b.sub.as_str(), b.sup.as_str()))
        });

        // Track already-emitted edges and avoid introducing cycles.
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        let mut emitted: HashSet<(String, String)> = HashSet::new();
        for ty in object_types.iter().filter(|t| t.as_str() != "Entity") {
            adj.entry(ty.clone())
                .or_default()
                .push("Entity".to_string());
            emitted.insert((ty.clone(), "Entity".to_string()));
        }

        let would_create_cycle = |adj: &HashMap<String, Vec<String>>, sub: &str, sup: &str| {
            // If `sup` can reach `sub`, then adding `sub → sup` introduces a cycle.
            let mut stack: Vec<&str> = vec![sup];
            let mut seen: HashSet<&str> = HashSet::new();
            while let Some(cur) = stack.pop() {
                if !seen.insert(cur) {
                    continue;
                }
                if cur == sub {
                    return true;
                }
                if let Some(nexts) = adj.get(cur) {
                    for n in nexts {
                        stack.push(n.as_str());
                    }
                }
            }
            false
        };

        let mut emitted_any = false;
        for st in subtypes {
            let sub = sanitize_axi_ident(&st.sub);
            let sup = sanitize_axi_ident(&st.sup);
            if sub == sup {
                continue;
            }
            if !object_types.contains(&sub) || !object_types.contains(&sup) {
                continue;
            }
            if emitted.contains(&(sub.clone(), sup.clone())) {
                continue;
            }
            if would_create_cycle(&adj, &sub, &sup) {
                continue;
            }

            if !emitted_any {
                writeln!(&mut out)?;
                writeln!(
                    &mut out,
                    "  -- Suggested subtyping between discovered object types (untrusted; review before promotion)."
                )?;
                emitted_any = true;
            }

            if let Some(r) = st
                .public_rationale
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                let mut r = r.replace('\n', " ");
                if r.len() > 160 {
                    r.truncate(160);
                    r.push_str("…");
                }
                writeln!(&mut out, "  -- {r}")?;
            }
            writeln!(&mut out, "  subtype {sub} < {sup}")?;

            adj.entry(sub.clone()).or_default().push(sup.clone());
            emitted.insert((sub, sup));
        }
    }

    // Relations.
    writeln!(&mut out)?;
    writeln!(
        &mut out,
        "  -- Binary relations observed in proposals (rel_type → relation)."
    )?;

    let mut rel_names: Vec<String> = rel_tuples.keys().cloned().collect();
    rel_names.extend(rel_tuples_ctx.keys().cloned());
    rel_names.sort();
    rel_names.dedup();

    for rel in &rel_names {
        let from_ty = match rel_src_types.get(rel).map(|s| s.len()) {
            Some(1) => rel_src_types
                .get(rel)
                .and_then(|s| s.iter().next().cloned())
                .unwrap_or_else(|| "Entity".to_string()),
            _ => "Entity".to_string(),
        };
        let to_ty = match rel_dst_types.get(rel).map(|s| s.len()) {
            Some(1) => rel_dst_types
                .get(rel)
                .and_then(|s| s.iter().next().cloned())
                .unwrap_or_else(|| "Entity".to_string()),
            _ => "Entity".to_string(),
        };
        if rel_has_context.contains(rel) {
            writeln!(
                &mut out,
                "  relation {rel}(from: {from_ty}, to: {to_ty}) @context Context"
            )?;
        } else {
            writeln!(&mut out, "  relation {rel}(from: {from_ty}, to: {to_ty})")?;
        }
    }

    // Suggested constraints (optional, untrusted). Keep separate from extensional inference
    // so diffs stay reviewable.
    if let Some(suggestions) = suggestions {
        let rel_set: HashSet<String> = rel_names.iter().cloned().collect();
        let mut any = false;
        let mut emitted: HashSet<String> = HashSet::new();

        for c in &suggestions.constraints {
            let kind = c.kind.trim().to_ascii_lowercase();
            let rel = sanitize_axi_ident(&c.relation);
            if !rel_set.contains(&rel) {
                continue;
            }
            let line = match kind.as_str() {
                "symmetric" => format!("  constraint symmetric {rel}"),
                "transitive" => format!("  constraint transitive {rel}"),
                _ => continue,
            };
            if !emitted.insert(line.clone()) {
                continue;
            }

            if !any {
                writeln!(&mut out)?;
                writeln!(
                    &mut out,
                    "theory {}Suggested on {}:",
                    options.schema_name, options.schema_name
                )?;
                writeln!(
                    &mut out,
                    "  -- Suggested constraints (untrusted; review before promotion)."
                )?;
                any = true;
            }

            if let Some(r) = c
                .public_rationale
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                let mut r = r.replace('\n', " ");
                if r.len() > 160 {
                    r.truncate(160);
                    r.push_str("…");
                }
                writeln!(&mut out, "  -- {r}")?;
            }
            writeln!(&mut out, "{line}")?;
        }
    }

    // Theory (optional).
    if options.infer_constraints {
        writeln!(&mut out)?;
        writeln!(
            &mut out,
            "theory {}Extensional on {}:",
            options.schema_name, options.schema_name
        )?;
        writeln!(
            &mut out,
            "  -- Extensional constraints inferred from current tuples (best-effort)."
        )?;
        writeln!(
            &mut out,
            "  -- Treat these as hypotheses: they may not generalize as new data arrives."
        )?;

        for rel in &rel_names {
            writeln!(&mut out)?;
            writeln!(
                &mut out,
                "  -- Keys: make fact atoms like `{rel}(from=a, to=b)` eligible for key pruning."
            )?;
            if rel_has_context.contains(rel) {
                writeln!(&mut out, "  constraint key {rel}(from, to, ctx)")?;
            } else {
                writeln!(&mut out, "  constraint key {rel}(from, to)")?;
            }

            if let Some(pairs) = rel_tuples.get(rel) {
                if pairs.is_empty() {
                    continue;
                }
                let mut by_from: HashMap<&str, HashSet<&str>> = HashMap::new();
                let mut by_to: HashMap<&str, HashSet<&str>> = HashMap::new();
                for (a, b) in pairs {
                    by_from.entry(a.as_str()).or_default().insert(b.as_str());
                    by_to.entry(b.as_str()).or_default().insert(a.as_str());
                }

                let functional_from = by_from.values().all(|s| s.len() <= 1);
                let functional_to = by_to.values().all(|s| s.len() <= 1);

                if functional_from {
                    writeln!(&mut out, "  constraint key {rel}(from)")?;
                    writeln!(&mut out, "  constraint functional {rel}.from -> {rel}.to")?;
                }
                if functional_to {
                    writeln!(&mut out, "  constraint key {rel}(to)")?;
                    writeln!(&mut out, "  constraint functional {rel}.to -> {rel}.from")?;
                }
            } else if let Some(triples) = rel_tuples_ctx.get(rel) {
                // For context-scoped relations, we currently do not infer additional
                // `functional` constraints because the constraint vocabulary only
                // supports single-field determiners (`from -> to`), not `(from, ctx) -> to`.
                //
                // We still emit a key constraint including `ctx` above to avoid
                // accidentally treating duplicates across contexts as violations.
                let _ = triples;
            }
        }
    }

    // Instance.
    writeln!(&mut out)?;
    writeln!(
        &mut out,
        "instance {} of {}:",
        options.instance_name, options.schema_name
    )?;

    // Object assignments (skip the `Entity` supertype to keep output smaller;
    // elements become `Entity` implicitly via the subtype closure and relation fields).
    for (ty, members) in &members_by_type {
        if ty == "Entity" {
            continue;
        }
        writeln!(&mut out, "  {ty} = {{")?;
        for (idx, name) in members.iter().enumerate() {
            if idx + 1 == members.len() {
                writeln!(&mut out, "    {name}")?;
            } else {
                writeln!(&mut out, "    {name},")?;
            }
        }
        writeln!(&mut out, "  }}")?;
        writeln!(&mut out)?;
    }

    // Relation assignments.
    for rel in &rel_names {
        if rel_has_context.contains(rel) {
            let Some(triples) = rel_tuples_ctx.get(rel) else {
                continue;
            };
            writeln!(&mut out, "  {rel} = {{")?;
            for (idx, (a, b, ctx)) in triples.iter().enumerate() {
                let tuple = format!("(from={a}, to={b}, ctx={ctx})");
                if idx + 1 == triples.len() {
                    writeln!(&mut out, "    {tuple}")?;
                } else {
                    writeln!(&mut out, "    {tuple},")?;
                }
            }
            writeln!(&mut out, "  }}")?;
            writeln!(&mut out)?;
        } else {
            let Some(pairs) = rel_tuples.get(rel) else {
                continue;
            };
            writeln!(&mut out, "  {rel} = {{")?;
            for (idx, (a, b)) in pairs.iter().enumerate() {
                let tuple = format!("(from={a}, to={b})");
                if idx + 1 == pairs.len() {
                    writeln!(&mut out, "    {tuple}")?;
                } else {
                    writeln!(&mut out, "    {tuple},")?;
                }
            }
            writeln!(&mut out, "  }}")?;
            writeln!(&mut out)?;
        }
    }

    // Sanity check: ensure output parses as `axi_v1` (helps catch naming bugs early).
    let parsed = axiograph_dsl::axi_v1::parse_axi_v1(&out).map_err(|e| anyhow!("{e}"))?;
    if parsed.module_name != options.module_name {
        return Err(anyhow!(
            "draft module parse mismatch: expected module `{}`, got `{}`",
            options.module_name,
            parsed.module_name
        ));
    }

    Ok(out)
}
