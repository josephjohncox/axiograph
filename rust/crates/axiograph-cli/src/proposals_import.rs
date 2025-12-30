//! Import `proposals.json` (Evidence/Proposals schema) into PathDB.
//!
//! This is the **evidence plane** import:
//! - it preserves cross-domain extracted structure (entities/relations + metadata),
//! - keeps contexts/worlds explicit when available (`attributes.context`),
//! - and links back to `DocChunk` evidence when chunk nodes exist.
//!
//! The accepted `.axi` plane remains canonical (meaning/spec). Proposals are
//! deliberately untrusted inputs that can later be reconciled/promoted into
//! canonical `.axi` modules.

use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use axiograph_ingest_docs::{EvidencePointer, ProposalV1, ProposalsFileV1};
use axiograph_pathdb::axi_meta::{
    ATTR_AXI_RELATION, ATTR_AXI_SCHEMA, META_ATTR_NAME, REL_AXI_FACT_IN_CONTEXT,
};
use axiograph_pathdb::axi_semantics::{MetaPlaneIndex, RelationDecl};
use axiograph_pathdb::CheckedDbMut;
use axiograph_pathdb::PathDB;

use crate::relation_resolution::EndpointOrientation;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub(crate) struct ImportProposalsSummary {
    pub proposals_total: usize,
    pub entities_added: usize,
    pub entities_reused: usize,
    pub relation_facts_added: usize,
    pub relation_facts_reused: usize,
    pub derived_edges_added: usize,
    pub contexts_created: usize,
    pub evidence_links_added: usize,
}

pub(crate) fn import_proposals_file_into_pathdb(
    db: &mut PathDB,
    file: &ProposalsFileV1,
    proposals_digest: &str,
) -> Result<ImportProposalsSummary> {
    let mut summary = ImportProposalsSummary::default();
    summary.proposals_total = file.proposals.len();

    let meta_plane = MetaPlaneIndex::from_db(db).unwrap_or_default();
    let mut relation_name_counts: HashMap<String, usize> = HashMap::new();
    for schema in meta_plane.schemas.values() {
        for rel in schema.relation_decls.keys() {
            *relation_name_counts.entry(rel.clone()).or_insert(0) += 1;
        }
    }

    let now_secs: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Represent the proposals file itself as a run node, so evidence-plane data
    // can be traced back to its source (cross-domain provenance).
    let run_id = get_or_create_proposal_run(db, file, proposals_digest)?;

    let mut id_map: HashMap<String, u32> = HashMap::new();

    // Pass 1: import entities first so relation endpoints exist.
    for p in &file.proposals {
        let ProposalV1::Entity {
            meta: proposal_meta,
            entity_id,
            entity_type,
            name,
            attributes,
            description,
        } = p
        else {
            continue;
        };

        let entity_id = entity_id.trim();
        if entity_id.is_empty() {
            continue;
        }

        let schema_hint = proposal_meta
            .schema_hint
            .as_deref()
            .or(file.schema_hint.as_deref())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());

        let id = match find_entity_by_external_id(db, entity_id)? {
            Some(existing) => {
                // Keep existing entity record, but mark the more specific type
                // and enrich missing attributes.
                db.mark_virtual_type(existing, entity_type)?;
                if let Some(schema_name) = schema_hint {
                    if meta_plane.schemas.contains_key(schema_name) && !entity_has_schema(db, existing, schema_name)? {
                        upsert_if_missing(db, existing, ATTR_AXI_SCHEMA, schema_name)?;
                    }
                }
                enrich_entity_from_proposal(
                    db,
                    existing,
                    proposal_meta,
                    name,
                    attributes,
                    description,
                )?;
                attach_evidence_attrs(db, existing, &proposal_meta.evidence)?;
                summary.entities_reused += 1;
                existing
            }
            None => {
                let attrs = build_entity_attrs(proposal_meta, entity_id, name, attributes, description);
                let id = if let Some(schema_name) = schema_hint {
                    if meta_plane
                        .schemas
                        .get(schema_name)
                        .map(|s| s.object_types.contains(entity_type))
                        .unwrap_or(false)
                    {
                        let mut checked = CheckedDbMut::new(db)?;
                        let mut builder = checked.entity_builder(schema_name, entity_type)?;
                        for (k, v) in &attrs {
                            builder = builder.with_attr(k, v);
                        }
                        let id = builder.commit()?;
                        checked.db_mut().mark_virtual_type(id, "ProposalEntity")?;
                        id
                    } else {
                        let attrs_ref = attrs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                        let id = db.add_entity(entity_type, attrs_ref);
                        db.mark_virtual_type(id, "ProposalEntity")?;
                        id
                    }
                } else {
                    let attrs_ref = attrs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                    let id = db.add_entity(entity_type, attrs_ref);
                    db.mark_virtual_type(id, "ProposalEntity")?;
                    id
                };
                summary.entities_added += 1;
                id
            }
        };

        link_run_to_proposal(db, run_id, id)?;
        id_map.insert(entity_id.to_string(), id);
        summary.evidence_links_added += link_evidence(db, id, &proposal_meta.evidence)?;
    }

    // Pass 2: import relation proposals as fact nodes + derived binary edges.
    for p in &file.proposals {
        let ProposalV1::Relation {
            meta: proposal_meta,
            relation_id,
            rel_type,
            source,
            target,
            attributes,
        } = p
        else {
            continue;
        };

        let relation_id = relation_id.trim();
        if relation_id.is_empty() {
            continue;
        }

        let schema_hint = proposal_meta
            .schema_hint
            .as_deref()
            .or(file.schema_hint.as_deref())
            .or_else(|| attributes.get("axi_schema").map(|s| s.as_str()))
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());

        let mut rel_type = rel_type.trim().to_string();
        let mut source_key = source.as_str();
        let mut target_key = target.as_str();

        let schema_rel = crate::relation_resolution::resolve_schema_relation(
            &meta_plane,
            schema_hint,
            rel_type.as_str(),
        );
        let schema_rel = schema_rel.map(|r| {
            // Apply semantic alias orientation (e.g. `parent_of` swaps endpoints).
            if r.orientation == EndpointOrientation::Swap {
                std::mem::swap(&mut source_key, &mut target_key);
            }
            rel_type = r.rel_name.clone();
            r
        });

        let resolved = match schema_rel {
            Some(v) => v,
            None => {
                // Legacy fallback: preserve structure without meta-plane typing.
                let src = resolve_or_stub_entity(db, &id_map, source_key)?;
                let dst = resolve_or_stub_entity(db, &id_map, target_key)?;

                // Context/world scoping (recommended): `attributes.context` creates an
                // `axi_fact_in_context` edge so queries can scope facts efficiently.
                let context_id = if let Some(ctx) = attributes.get("context") {
                    Some(get_or_create_context(db, &meta_plane, None, ctx, &mut summary)?)
                } else {
                    None
                };

                let fact_type = format!("{}Fact", rel_type.trim());
                let fact_id =
                    match find_entity_by_external_id_and_type(db, relation_id, &fact_type)? {
                        Some(existing) => {
                            // Enrich attrs if possible (best-effort).
                            enrich_relation_fact_from_proposal(
                                db,
                                existing,
                                proposal_meta,
                                &rel_type,
                                attributes,
                            )?;
                            attach_evidence_attrs(db, existing, &proposal_meta.evidence)?;
                            summary.relation_facts_reused += 1;
                            existing
                        }
                        None => {
                            let attrs =
                                build_relation_fact_attrs(
                                    proposal_meta,
                                    relation_id,
                                    &rel_type,
                                    None,
                                    attributes,
                                );
                            let attrs_ref =
                                attrs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                            let id = db.add_entity(&fact_type, attrs_ref);
                            db.mark_virtual_type(id, "FactNode")?;
                            db.mark_virtual_type(id, "ProposalFact")?;
                            summary.relation_facts_added += 1;
                            id
                        }
                    };

                link_run_to_proposal(db, run_id, fact_id)?;
                summary
                    .evidence_links_added
                    += link_evidence(db, fact_id, &proposal_meta.evidence)?;

                add_edge_if_missing(db, "from", fact_id, src, 1.0)?;
                add_edge_if_missing(db, "to", fact_id, dst, 1.0)?;
                if let Some(ctx_id) = context_id {
                    add_edge_if_missing(db, REL_AXI_FACT_IN_CONTEXT, fact_id, ctx_id, 1.0)?;
                }

                // Derived traversal edge: source -rel_type-> target.
                // This keeps AxQL ergonomic even when relations are reified into fact nodes.
                let confidence = proposal_meta.confidence.clamp(0.0, 1.0) as f32;
                if !rel_type.is_empty() {
                    let derived_label = if let Some(hint) = schema_hint {
                        let hint = hint.trim();
                        if !hint.is_empty()
                            && meta_plane.schemas.contains_key(hint)
                            && meta_plane
                                .schemas
                                .get(hint)
                                .map(|s| s.relation_decls.contains_key(rel_type.as_str()))
                                .unwrap_or(false)
                            && relation_name_counts
                                .get(rel_type.as_str())
                                .copied()
                                .unwrap_or(0)
                                > 1
                        {
                            format!("{hint}.{rel_type}")
                        } else {
                            rel_type.clone()
                        }
                    } else {
                        rel_type.clone()
                    };

                    let rel_id = db.interner.intern(&derived_label);
                    if !db.relations.has_edge(src, rel_id, dst) {
                        db.add_relation(&derived_label, src, dst, confidence, vec![]);
                        summary.derived_edges_added += 1;
                    }
                }

                continue;
            }
        };

        let schema_name = resolved.schema_name.clone();
        let schema = resolved.schema;
        let rel_decl = resolved.rel_decl;

        // We’re about to build a canonical schema-typed fact, so keep a checked DB
        // handle around for:
        // - schema-aware stub entity creation (typed by construction), and
        // - typed fact-node construction via `TypedFactBuilder`.
        let mut checked = CheckedDbMut::new(db)?;

        // Endpoints (schema-directed when possible). For "simple relation" overlays we
        // allow either:
        // - explicit field mapping in attributes, or
        // - a deterministic fallback (e.g. from/to, lhs/rhs, or first two fields).
        let (src_field, dst_field) = resolve_endpoint_fields(attributes, rel_decl)?;
        let src_type_hint = rel_decl
            .fields
            .iter()
            .find(|f| f.field_name == src_field)
            .map(|f| f.field_type.as_str());
        let dst_type_hint = rel_decl
            .fields
            .iter()
            .find(|f| f.field_name == dst_field)
            .map(|f| f.field_type.as_str());

        let src = resolve_or_stub_entity_with_type_in_schema(
            &mut checked,
            &id_map,
            source_key,
            src_type_hint,
            &schema_name,
        )?;
        let dst = resolve_or_stub_entity_with_type_in_schema(
            &mut checked,
            &id_map,
            target_key,
            dst_type_hint,
            &schema_name,
        )?;

        // Context/world scoping (recommended): `attributes.context` creates an
        // `axi_fact_in_context` edge so queries can scope facts efficiently.
        let context_name = attributes
            .get("ctx")
            .or_else(|| attributes.get("context"))
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());
        let context_id = if let Some(ctx) = context_name {
            Some(get_or_create_context(
                checked.db_mut(),
                &meta_plane,
                Some(schema_name.as_str()),
                ctx,
                &mut summary,
            )?)
        } else {
            None
        };

        let tuple_entity_type = schema.tuple_entity_type_name(&rel_type);

        let existing_fact_id =
            find_entity_by_external_id_and_type(checked.db_mut(), relation_id, &tuple_entity_type)?;

        // Precompute all required field values before we borrow `db` via the typed builder.
        let confidence = proposal_meta.confidence.clamp(0.0, 1.0) as f32;
        let mut field_values: HashMap<String, u32> = HashMap::new();
        let src_field = src_field.as_str();
        let dst_field = dst_field.as_str();

        for f in &rel_decl.fields {
            let field = f.field_name.as_str();
            let value = if field == src_field {
                src
            } else if field == dst_field {
                dst
            } else if field == "ctx" {
                match context_id {
                    Some(id) => id,
                    None => get_or_create_context(
                        checked.db_mut(),
                        &meta_plane,
                        Some(schema_name.as_str()),
                        "Observed",
                        &mut summary,
                    )?,
                }
            } else if field == "time" {
                let time_name = attributes
                    .get("time")
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| format!("T{now_secs}"));
                resolve_or_stub_entity_with_type_in_schema(
                    &mut checked,
                    &id_map,
                    &time_name,
                    Some(&f.field_type),
                    &schema_name,
                )?
            } else if let Some(v) = attributes.get(field) {
                let v = v.trim();
                if v.is_empty() {
                    return Err(anyhow!(
                        "proposal relation `{relation_id}`: field `{field}` is present but empty"
                    ));
                }
                resolve_or_stub_entity_with_type_in_schema(
                    &mut checked,
                    &id_map,
                    v,
                    Some(&f.field_type),
                    &schema_name,
                )?
            } else {
                return Err(anyhow!(
                    "proposal relation `{relation_id}`: missing required field `{field}` for `{schema_name}.{}`",
                    rel_decl.name
                ));
            };
            field_values.insert(field.to_string(), value);
        }

        // Use the typed builder so fact nodes are well-formed by construction.
        let mut builder = checked
            .fact_builder(&schema_name, &rel_type)?
            .with_edge_confidence(confidence);

        // Attach proposal provenance attrs to the fact node. Do not overwrite `name`:
        // keep the deterministic fact-node name derived from (schema, relation, fields).
        let attrs = build_relation_fact_attrs(
            proposal_meta,
            relation_id,
            &rel_type,
            Some(schema_name.as_str()),
            attributes,
        );
        for (k, v) in &attrs {
            if k == META_ATTR_NAME {
                continue;
            }
            builder = builder.with_attr(k, v);
        }

        for (field, value) in &field_values {
            builder.set_field(field, *value)?;
        }

        let fact_id = match existing_fact_id {
            Some(existing) => {
                builder.commit_into_existing(existing)?
            }
            None => {
                let id = builder.commit()?;
                checked.db_mut().mark_virtual_type(id, "ProposalFact")?;
                summary.relation_facts_added += 1;
                id
            }
        };

        let db = checked.db_mut();
        if existing_fact_id.is_some() {
            // Enrich attrs if possible (best-effort).
            enrich_relation_fact_from_proposal(db, fact_id, proposal_meta, &rel_type, attributes)?;
            upsert_if_missing(db, fact_id, ATTR_AXI_SCHEMA, schema_name.as_str())?;
            attach_evidence_attrs(db, fact_id, &proposal_meta.evidence)?;
            db.mark_virtual_type(fact_id, "ProposalFact")?;
            summary.relation_facts_reused += 1;
        }
        link_run_to_proposal(db, run_id, fact_id)?;
        summary.evidence_links_added += link_evidence(db, fact_id, &proposal_meta.evidence)?;

        // Uniform context scoping: treat `attributes.ctx/context` as a request to
        // scope the fact to a world/context, even if the relation signature does
        // not include a `ctx` field. When a `ctx` field exists, the typed
        // builder already maintains the `axi_fact_in_context` invariant; this is
        // a best-effort "fill missing" for the general case.
        if let Some(ctx_id) = context_id {
            add_edge_if_missing(db, REL_AXI_FACT_IN_CONTEXT, fact_id, ctx_id, confidence)?;
        }

        // Derived traversal edge: source -rel_type-> target.
        // This keeps AxQL ergonomic even when relations are reified into fact nodes.
        if !rel_type.is_empty() {
            let derived_label = if relation_name_counts
                .get(rel_type.as_str())
                .copied()
                .unwrap_or(0)
                > 1
            {
                format!("{schema_name}.{rel_type}")
            } else {
                rel_type.clone()
            };

            let rel_id = db.interner.intern(&derived_label);
            if !db.relations.has_edge(src, rel_id, dst) {
                db.add_relation(&derived_label, src, dst, confidence, vec![]);
                summary.derived_edges_added += 1;
            }
        }
    }

    Ok(summary)
}

fn resolve_endpoint_fields(
    attrs: &HashMap<String, String>,
    rel_decl: &RelationDecl,
) -> Result<(String, String)> {
    if let (Some(a), Some(b)) = (
        attrs.get("axi_source_field").map(|s| s.trim()),
        attrs.get("axi_target_field").map(|s| s.trim()),
    ) {
        if !a.is_empty() && !b.is_empty() {
            return Ok((a.to_string(), b.to_string()));
        }
    }

    let names: Vec<&str> = rel_decl.fields.iter().map(|f| f.field_name.as_str()).collect();
    if names.contains(&"from") && names.contains(&"to") {
        return Ok(("from".to_string(), "to".to_string()));
    }
    if names.contains(&"source") && names.contains(&"target") {
        return Ok(("source".to_string(), "target".to_string()));
    }
    if names.contains(&"lhs") && names.contains(&"rhs") {
        return Ok(("lhs".to_string(), "rhs".to_string()));
    }
    if names.contains(&"child") && names.contains(&"parent") {
        return Ok(("child".to_string(), "parent".to_string()));
    }
    if rel_decl.fields.len() >= 2 {
        return Ok((rel_decl.fields[0].field_name.clone(), rel_decl.fields[1].field_name.clone()));
    }

    Err(anyhow!(
        "relation `{}` has fewer than 2 fields (cannot map source/target)",
        rel_decl.name
    ))
}

// =============================================================================
// Run/context nodes
// =============================================================================

fn get_or_create_proposal_run(
    db: &mut PathDB,
    file: &ProposalsFileV1,
    proposals_digest: &str,
) -> Result<u32> {
    let external_id = format!("proposals::{proposals_digest}");
    if let Some(id) = find_entity_by_external_id(db, &external_id)? {
        return Ok(id);
    }

    let mut attrs: Vec<(String, String)> = Vec::new();
    attrs.push((META_ATTR_NAME.to_string(), external_id.clone()));
    attrs.push(("external_id".to_string(), external_id));
    attrs.push(("proposals_digest".to_string(), proposals_digest.to_string()));
    attrs.push(("generated_at".to_string(), file.generated_at.clone()));
    attrs.push(("source_type".to_string(), file.source.source_type.clone()));
    attrs.push(("source_locator".to_string(), file.source.locator.clone()));
    if let Some(hint) = file.schema_hint.as_ref() {
        attrs.push(("schema_hint".to_string(), hint.clone()));
    }

    let attrs_ref = attrs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    Ok(db.add_entity("ProposalRun", attrs_ref))
}

fn get_or_create_context(
    db: &mut PathDB,
    meta_plane: &MetaPlaneIndex,
    schema_name: Option<&str>,
    ctx: &str,
    summary: &mut ImportProposalsSummary,
) -> Result<u32> {
    let ctx = ctx.trim();
    if ctx.is_empty() {
        return Err(anyhow!("empty context id in proposal relation"));
    }

    if let Some(schema_name) = schema_name.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        // Prefer linking to proposals-imported Context entities by external id.
        if let Some(id) = find_entity_by_external_id_and_type(db, ctx, "Context")? {
            match find_attr_string(db, id, ATTR_AXI_SCHEMA) {
                Some(existing) if existing != schema_name => {}
                Some(_) => return Ok(id),
                None => {
                    db.upsert_entity_attr(id, ATTR_AXI_SCHEMA, schema_name)?;
                    return Ok(id);
                }
            }
        }

        // Prefer canonical `.axi` Context objects in this schema.
        if let Some(id) = find_entity_by_name_case_robust_with_type_and_schema(
            db,
            ctx,
            "Context",
            schema_name,
        )? {
            return Ok(id);
        }

        // Otherwise, create an extension-layer Context entity in this schema.
        let mut attrs: Vec<(String, String)> = Vec::new();
        attrs.push((META_ATTR_NAME.to_string(), ctx.to_string()));
        attrs.push(("external_id".to_string(), ctx.to_string()));
        attrs.push((ATTR_AXI_SCHEMA.to_string(), schema_name.to_string()));

        let can_typecheck = meta_plane
            .schemas
            .get(schema_name)
            .map(|s| s.object_types.contains("Context"))
            .unwrap_or(false);

        let id = if can_typecheck {
            let mut checked = CheckedDbMut::new(db)?;
            let mut builder = checked.entity_builder(schema_name, "Context")?;
            for (k, v) in &attrs {
                builder = builder.with_attr(k, v);
            }
            let id = builder.commit()?;
            checked.db_mut().mark_virtual_type(id, "ProposalContext")?;
            id
        } else {
            let attrs_ref = attrs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            let id = db.add_entity("Context", attrs_ref);
            db.mark_virtual_type(id, "ProposalContext")?;
            id
        };

        summary.contexts_created += 1;
        return Ok(id);
    }

    // Unscoped fallback:
    // Prefer linking to proposals-imported Context entities by external id.
    if let Some(id) = find_entity_by_external_id_and_type(db, ctx, "Context")? {
        return Ok(id);
    }
    // Fall back to canonical `.axi` Context objects (which typically do not
    // carry an `external_id`, but do carry `name`).
    if let Some(id) = find_entity_by_name_and_type(db, ctx, "Context")? {
        return Ok(id);
    }
    // Otherwise, create an extension-layer Context entity.
    let mut attrs: Vec<(String, String)> = Vec::new();
    attrs.push((META_ATTR_NAME.to_string(), ctx.to_string()));
    attrs.push(("external_id".to_string(), ctx.to_string()));
    let attrs_ref = attrs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let id = db.add_entity("Context", attrs_ref);
    db.mark_virtual_type(id, "ProposalContext")?;
    summary.contexts_created += 1;
    Ok(id)
}

fn link_run_to_proposal(db: &mut PathDB, run_id: u32, proposal_entity_id: u32) -> Result<()> {
    add_edge_if_missing(db, "run_has_proposal", run_id, proposal_entity_id, 1.0)?;
    add_edge_if_missing(db, "proposal_in_run", proposal_entity_id, run_id, 1.0)?;
    Ok(())
}

// =============================================================================
// Entity import helpers
// =============================================================================

fn build_entity_attrs(
    meta: &axiograph_ingest_docs::ProposalMetaV1,
    entity_id: &str,
    name: &str,
    attributes: &HashMap<String, String>,
    description: &Option<String>,
) -> Vec<(String, String)> {
    let mut attrs: Vec<(String, String)> = Vec::new();
    attrs.push((META_ATTR_NAME.to_string(), name.to_string()));
    attrs.push(("external_id".to_string(), entity_id.to_string()));
    attrs.push(("proposal_id".to_string(), meta.proposal_id.clone()));
    attrs.push(("proposal_confidence".to_string(), meta.confidence.to_string()));
    if let Some(hint) = meta.schema_hint.as_ref() {
        attrs.push(("schema_hint".to_string(), hint.clone()));
    }
    if !meta.public_rationale.trim().is_empty() {
        attrs.push(("public_rationale".to_string(), meta.public_rationale.clone()));
    }
    if let Some(desc) = description.as_ref() {
        if !desc.trim().is_empty() {
            attrs.push(("description".to_string(), desc.clone()));
        }
    }

    // Proposal attributes (best-effort; avoid overwriting reserved keys).
    let reserved: HashSet<&'static str> = [
        META_ATTR_NAME,
        "external_id",
        "proposal_id",
        "proposal_confidence",
        "schema_hint",
        "public_rationale",
        "description",
    ]
    .into_iter()
    .collect();

    for (k, v) in attributes {
        if reserved.contains(k.as_str()) {
            attrs.push((format!("attr_{k}"), v.clone()));
        } else {
            attrs.push((k.clone(), v.clone()));
        }
    }
    for (k, v) in &meta.metadata {
        attrs.push((format!("meta_{k}"), v.clone()));
    }

    // Evidence pointers are attached as attrs so they survive even if chunks
    // are not imported into the snapshot.
    for (i, ev) in meta.evidence.iter().enumerate() {
        attrs.push((format!("evidence_{i}_chunk_id"), ev.chunk_id.clone()));
        if let Some(loc) = ev.locator.as_ref() {
            attrs.push((format!("evidence_{i}_locator"), loc.clone()));
        }
        if let Some(span) = ev.span_id.as_ref() {
            attrs.push((format!("evidence_{i}_span_id"), span.clone()));
        }
    }

    attrs
}

fn enrich_entity_from_proposal(
    db: &mut PathDB,
    entity_id: u32,
    meta: &axiograph_ingest_docs::ProposalMetaV1,
    name: &str,
    attributes: &HashMap<String, String>,
    description: &Option<String>,
) -> Result<()> {
    // Only fill missing keys; don't overwrite existing values.
    upsert_if_missing(db, entity_id, META_ATTR_NAME, name)?;
    upsert_if_missing(db, entity_id, "proposal_id", &meta.proposal_id)?;
    upsert_if_missing(db, entity_id, "proposal_confidence", &meta.confidence.to_string())?;
    if let Some(hint) = meta.schema_hint.as_ref() {
        upsert_if_missing(db, entity_id, "schema_hint", hint)?;
    }
    if !meta.public_rationale.trim().is_empty() {
        upsert_if_missing(db, entity_id, "public_rationale", &meta.public_rationale)?;
    }
    if let Some(desc) = description.as_ref() {
        if !desc.trim().is_empty() {
            upsert_if_missing(db, entity_id, "description", desc)?;
        }
    }
    for (k, v) in attributes {
        if k == META_ATTR_NAME {
            upsert_if_missing(db, entity_id, "attr_name", v)?;
        } else {
            upsert_if_missing(db, entity_id, k, v)?;
        }
    }
    for (k, v) in &meta.metadata {
        upsert_if_missing(db, entity_id, &format!("meta_{k}"), v)?;
    }
    Ok(())
}

fn build_relation_fact_attrs(
    meta: &axiograph_ingest_docs::ProposalMetaV1,
    relation_id: &str,
    rel_type: &str,
    axi_schema: Option<&str>,
    attributes: &HashMap<String, String>,
) -> Vec<(String, String)> {
    let mut attrs: Vec<(String, String)> = Vec::new();
    attrs.push((META_ATTR_NAME.to_string(), relation_id.to_string()));
    attrs.push(("external_id".to_string(), relation_id.to_string()));
    attrs.push(("proposal_id".to_string(), meta.proposal_id.clone()));
    attrs.push(("proposal_confidence".to_string(), meta.confidence.to_string()));
    if let Some(schema) = axi_schema {
        if !schema.trim().is_empty() {
            attrs.push((ATTR_AXI_SCHEMA.to_string(), schema.to_string()));
        }
    }
    if let Some(hint) = meta.schema_hint.as_ref() {
        attrs.push(("schema_hint".to_string(), hint.clone()));
    }
    if !meta.public_rationale.trim().is_empty() {
        attrs.push(("public_rationale".to_string(), meta.public_rationale.clone()));
    }
    attrs.push((ATTR_AXI_RELATION.to_string(), rel_type.to_string()));

    // Include relation attributes verbatim (prefix reserved keys).
    let reserved: HashSet<&'static str> = [
        META_ATTR_NAME,
        "external_id",
        "proposal_id",
        "proposal_confidence",
        "schema_hint",
        "public_rationale",
        ATTR_AXI_RELATION,
    ]
    .into_iter()
    .collect();

    for (k, v) in attributes {
        if reserved.contains(k.as_str()) {
            attrs.push((format!("attr_{k}"), v.clone()));
        } else {
            attrs.push((k.clone(), v.clone()));
        }
    }
    for (k, v) in &meta.metadata {
        attrs.push((format!("meta_{k}"), v.clone()));
    }

    for (i, ev) in meta.evidence.iter().enumerate() {
        attrs.push((format!("evidence_{i}_chunk_id"), ev.chunk_id.clone()));
        if let Some(loc) = ev.locator.as_ref() {
            attrs.push((format!("evidence_{i}_locator"), loc.clone()));
        }
        if let Some(span) = ev.span_id.as_ref() {
            attrs.push((format!("evidence_{i}_span_id"), span.clone()));
        }
    }

    attrs
}

fn enrich_relation_fact_from_proposal(
    db: &mut PathDB,
    fact_id: u32,
    meta: &axiograph_ingest_docs::ProposalMetaV1,
    rel_type: &str,
    attributes: &HashMap<String, String>,
) -> Result<()> {
    upsert_if_missing(db, fact_id, ATTR_AXI_RELATION, rel_type)?;
    upsert_if_missing(db, fact_id, "proposal_id", &meta.proposal_id)?;
    upsert_if_missing(db, fact_id, "proposal_confidence", &meta.confidence.to_string())?;
    if let Some(hint) = meta.schema_hint.as_ref() {
        upsert_if_missing(db, fact_id, "schema_hint", hint)?;
    }
    if !meta.public_rationale.trim().is_empty() {
        upsert_if_missing(db, fact_id, "public_rationale", &meta.public_rationale)?;
    }
    for (k, v) in attributes {
        if k == META_ATTR_NAME {
            upsert_if_missing(db, fact_id, "attr_name", v)?;
        } else {
            upsert_if_missing(db, fact_id, k, v)?;
        }
    }
    for (k, v) in &meta.metadata {
        upsert_if_missing(db, fact_id, &format!("meta_{k}"), v)?;
    }
    Ok(())
}

fn resolve_or_stub_entity(db: &mut PathDB, id_map: &HashMap<String, u32>, key: &str) -> Result<u32> {
    if let Some(&id) = id_map.get(key) {
        return Ok(id);
    }
    if let Some(id) = find_entity_by_external_id(db, key)? {
        return Ok(id);
    }
    // UX-first fallback: treat the key as a canonical entity name so simple
    // "conversation" proposals can refer to existing accepted-plane entities
    // without having to know a DB-internal id or carry external_id attrs.
    if let Some(id) = find_entity_by_name_case_robust(db, key)? {
        return Ok(id);
    }

    // Stub to preserve relation structure even if the endpoint is missing.
    // If a later proposals import provides more info for this external_id, the
    // importer can enrich it via `upsert_entity_attr` + `mark_virtual_type`.
    let mut attrs: Vec<(String, String)> = Vec::new();
    attrs.push((META_ATTR_NAME.to_string(), key.to_string()));
    attrs.push(("external_id".to_string(), key.to_string()));
    let attrs_ref = attrs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let id = db.add_entity("UnknownEntity", attrs_ref);
    db.mark_virtual_type(id, "ProposalStub")?;
    Ok(id)
}

fn resolve_or_stub_entity_with_type_in_schema(
    checked: &mut CheckedDbMut<'_>,
    id_map: &HashMap<String, u32>,
    key: &str,
    type_hint: Option<&str>,
    schema_name: &str,
) -> Result<u32> {
    let schema_name = schema_name.trim();
    if schema_name.is_empty() {
        return Err(anyhow!("empty schema name for schema-directed entity resolution"));
    }

    let key = key.trim();
    if key.is_empty() {
        return Err(anyhow!("empty entity reference"));
    }

    if let Some(&id) = id_map.get(key) {
        // Adopt schema scoping for proposal entities that were created without it.
        if let Some(existing) = find_attr_string(checked.db(), id, ATTR_AXI_SCHEMA) {
            if existing != schema_name {
                return Err(anyhow!(
                    "entity `{key}` resolved to entity {id} in schema `{existing}`, but schema `{schema_name}` is required"
                ));
            }
        } else {
            checked.db_mut().upsert_entity_attr(id, ATTR_AXI_SCHEMA, schema_name)?;
        }
        return Ok(id);
    }

    let Some(ty) = type_hint.map(|s| s.trim()).filter(|s| !s.is_empty()) else {
        // No type hint: fall back, but still prefer schema consistency when possible.
        let id = resolve_or_stub_entity(checked.db_mut(), id_map, key)?;
        if entity_has_schema(checked.db_mut(), id, schema_name)? {
            return Ok(id);
        }
        // Adopt schema if missing; otherwise fail (mismatch).
        if find_attr_string(checked.db(), id, ATTR_AXI_SCHEMA).is_none() {
            checked.db_mut().upsert_entity_attr(id, ATTR_AXI_SCHEMA, schema_name)?;
            return Ok(id);
        }
        return Err(anyhow!(
            "entity `{key}` resolved to entity {id}, but it is not in schema `{schema_name}`"
        ));
    };

    // Prefer proposals-imported entities by external id, but only when schema matches.
    if let Some(id) = find_entity_by_external_id_and_type(checked.db_mut(), key, ty)? {
        if entity_has_schema(checked.db_mut(), id, schema_name)? {
            return Ok(id);
        }
        if find_attr_string(checked.db(), id, ATTR_AXI_SCHEMA).is_none() {
            checked.db_mut().upsert_entity_attr(id, ATTR_AXI_SCHEMA, schema_name)?;
            return Ok(id);
        }
    }

    // Canonical `.axi` entities: prefer schema-scoped lookup.
    if let Some(id) = find_entity_by_name_case_robust_with_type_and_schema(
        checked.db_mut(),
        key,
        ty,
        schema_name,
    )? {
        return Ok(id);
    }

    // Stub to preserve structure even if the endpoint is missing in this schema.
    let mut builder = checked.entity_builder(schema_name, ty)?;
    builder = builder.with_attr(META_ATTR_NAME, key);
    builder = builder.with_attr("external_id", key);
    builder = builder.with_attr(ATTR_AXI_SCHEMA, schema_name);
    let id = builder.commit()?;
    checked.db_mut().mark_virtual_type(id, "ProposalStub")?;
    Ok(id)
}

fn find_entity_by_name_case_robust_with_type(
    db: &mut PathDB,
    name: &str,
    type_name: &str,
) -> Result<Option<u32>> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(None);
    }

    let Some(type_bm) = db.find_by_type(type_name) else {
        return Ok(None);
    };

    let Some(key_id) = db.interner.id_of("name") else {
        return Ok(None);
    };
    if let Some(value_id) = db.interner.id_of(name) {
        let ids = db.entities.entities_with_attr_value(key_id, value_id);
        for id in ids.iter() {
            if type_bm.contains(id) {
                return Ok(Some(id));
            }
        }
    }

    let mut candidates = db.entities_with_attr_fts("name", name);
    if candidates.is_empty() {
        candidates = db.entities_with_attr_fts_any("name", name);
    }
    if candidates.is_empty() {
        candidates = db.entities_with_attr_fuzzy("name", name, 2);
    }
    if candidates.is_empty() {
        return Ok(None);
    }

    let needle_lc = name.to_ascii_lowercase();
    for id in candidates.iter() {
        if !type_bm.contains(id) {
            continue;
        }
        if let Some(entity_name) = find_attr_string(db, id, "name") {
            if entity_name.to_ascii_lowercase() == needle_lc {
                return Ok(Some(id));
            }
        }
    }

    for id in candidates.iter() {
        if type_bm.contains(id) {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

fn find_entity_by_name_case_robust_with_type_and_schema(
    db: &mut PathDB,
    name: &str,
    type_name: &str,
    schema_name: &str,
) -> Result<Option<u32>> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(None);
    }
    let schema_name = schema_name.trim();
    if schema_name.is_empty() {
        return find_entity_by_name_case_robust_with_type(db, name, type_name);
    }

    let Some(type_bm) = db.find_by_type(type_name) else {
        return Ok(None);
    };

    let schema_key_id = db.interner.intern(ATTR_AXI_SCHEMA);
    let schema_value_id = db.interner.intern(schema_name);
    let schema_bm = db.entities.entities_with_attr_value(schema_key_id, schema_value_id);

    let Some(name_key_id) = db.interner.id_of("name") else {
        return Ok(None);
    };
    if let Some(value_id) = db.interner.id_of(name) {
        let ids = db.entities.entities_with_attr_value(name_key_id, value_id);
        for id in ids.iter() {
            if type_bm.contains(id) && schema_bm.contains(id) {
                return Ok(Some(id));
            }
        }
    }

    let mut candidates = db.entities_with_attr_fts("name", name);
    if candidates.is_empty() {
        candidates = db.entities_with_attr_fts_any("name", name);
    }
    if candidates.is_empty() {
        candidates = db.entities_with_attr_fuzzy("name", name, 2);
    }
    if candidates.is_empty() {
        return Ok(None);
    }

    let needle_lc = name.to_ascii_lowercase();
    for id in candidates.iter() {
        if !type_bm.contains(id) || !schema_bm.contains(id) {
            continue;
        }
        if let Some(entity_name) = find_attr_string(db, id, "name") {
            if entity_name.to_ascii_lowercase() == needle_lc {
                return Ok(Some(id));
            }
        }
    }

    for id in candidates.iter() {
        if type_bm.contains(id) && schema_bm.contains(id) {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

fn find_entity_by_name_case_robust(db: &mut PathDB, name: &str) -> Result<Option<u32>> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(None);
    }

    // Fast path: exact `name` match via interned attr value ids.
    if let Some(id) = find_entity_by_type_and_attr(db, "", "name", name)? {
        return Ok(Some(id));
    }

    // Robust fallback: token/fts/fuzzy so "alice" can still resolve to "Alice".
    let mut candidates = db.entities_with_attr_fts("name", name);
    if candidates.is_empty() {
        candidates = db.entities_with_attr_fts_any("name", name);
    }
    if candidates.is_empty() {
        candidates = db.entities_with_attr_fuzzy("name", name, 2);
    }
    if candidates.is_empty() {
        return Ok(None);
    }

    let needle_lc = name.to_ascii_lowercase();
    for id in candidates.iter() {
        if let Some(entity_name) = find_attr_string(db, id, "name") {
            if entity_name.to_ascii_lowercase() == needle_lc {
                return Ok(Some(id));
            }
        }
    }

    Ok(candidates.iter().next())
}

fn find_attr_string(db: &PathDB, entity_id: u32, key: &str) -> Option<String> {
    let key_id = db.interner.id_of(key)?;
    let value_id = db.entities.get_attr(entity_id, key_id)?;
    db.interner.lookup(value_id).map(|s| s.to_string())
}

// =============================================================================
// Evidence linking
// =============================================================================

fn attach_evidence_attrs(db: &mut PathDB, entity_id: u32, evidence: &[EvidencePointer]) -> Result<()> {
    // Find the next free evidence slot. Evidence is modeled as attributes so it
    // survives even when chunks are not imported into the snapshot.
    let mut next: Option<usize> = None;
    for i in 0usize..1024 {
        let key = format!("evidence_{i}_chunk_id");
        let key_id = db.interner.intern(&key);
        if db.entities.get_attr(entity_id, key_id).is_none() {
            next = Some(i);
            break;
        }
    }
    let mut next = next.ok_or_else(|| anyhow!("too many evidence pointers attached to entity {entity_id}"))?;

    for ev in evidence {
        db.upsert_entity_attr(entity_id, &format!("evidence_{next}_chunk_id"), &ev.chunk_id)?;
        if let Some(loc) = ev.locator.as_ref() {
            db.upsert_entity_attr(entity_id, &format!("evidence_{next}_locator"), loc)?;
        }
        if let Some(span) = ev.span_id.as_ref() {
            db.upsert_entity_attr(entity_id, &format!("evidence_{next}_span_id"), span)?;
        }
        next = next.saturating_add(1);
    }

    Ok(())
}

fn link_evidence(db: &mut PathDB, proposal_entity_id: u32, evidence: &[EvidencePointer]) -> Result<usize> {
    let mut added = 0usize;
    for ev in evidence {
        let Some(chunk_id) = find_doc_chunk_by_chunk_id(db, &ev.chunk_id)? else {
            continue;
        };
        add_edge_if_missing(db, "has_evidence_chunk", proposal_entity_id, chunk_id, 1.0)?;
        add_edge_if_missing(db, "evidence_for", chunk_id, proposal_entity_id, 1.0)?;
        added += 2;
    }
    Ok(added)
}

fn find_doc_chunk_by_chunk_id(db: &mut PathDB, chunk_id: &str) -> Result<Option<u32>> {
    find_entity_by_type_and_attr(db, "DocChunk", "chunk_id", chunk_id)
}

// =============================================================================
// Generic lookup helpers
// =============================================================================

fn find_entity_by_external_id(db: &mut PathDB, external_id: &str) -> Result<Option<u32>> {
    find_entity_by_type_and_attr(db, "", "external_id", external_id)
}

fn find_entity_by_external_id_and_type(
    db: &mut PathDB,
    external_id: &str,
    type_name: &str,
) -> Result<Option<u32>> {
    find_entity_by_type_and_attr(db, type_name, "external_id", external_id)
}

fn find_entity_by_name_and_type(db: &mut PathDB, name: &str, type_name: &str) -> Result<Option<u32>> {
    find_entity_by_type_and_attr(db, type_name, META_ATTR_NAME, name)
}

fn find_entity_by_type_and_attr(
    db: &mut PathDB,
    type_name: &str,
    attr_key: &str,
    attr_value: &str,
) -> Result<Option<u32>> {
    let key_id = db.interner.intern(attr_key);
    let value_id = db.interner.intern(attr_value);

    let candidates = db.entities.entities_with_attr_value(key_id, value_id);
    if candidates.is_empty() {
        return Ok(None);
    }

    if type_name.trim().is_empty() {
        return Ok(candidates.iter().next());
    }

    let type_id = db.interner.intern(type_name);
    for entity_id in candidates.iter() {
        if db.entities.get_type(entity_id) == Some(type_id) {
            return Ok(Some(entity_id));
        }
    }
    Ok(None)
}

fn entity_has_schema(db: &mut PathDB, entity_id: u32, schema_name: &str) -> Result<bool> {
    let schema_name = schema_name.trim();
    if schema_name.is_empty() {
        return Ok(true);
    }
    Ok(find_attr_string(db, entity_id, ATTR_AXI_SCHEMA)
        .map(|s| s == schema_name)
        .unwrap_or(false))
}

fn upsert_if_missing(db: &mut PathDB, entity_id: u32, key: &str, value: &str) -> Result<()> {
    let key_id = db.interner.intern(key);
    if db.entities.get_attr(entity_id, key_id).is_some() {
        return Ok(());
    }
    db.upsert_entity_attr(entity_id, key, value)
}

fn add_edge_if_missing(db: &mut PathDB, rel: &str, source: u32, target: u32, confidence: f32) -> Result<()> {
    let rel_id = db.interner.intern(rel);
    if db.relations.has_edge(source, rel_id, target) {
        return Ok(());
    }
    db.add_relation(rel, source, target, confidence, vec![]);
    Ok(())
}
