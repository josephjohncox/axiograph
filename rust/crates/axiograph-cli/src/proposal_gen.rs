//! Proposal generation helpers (evidence-plane overlays).
//!
//! These helpers are deliberately **untrusted** and are intended to support:
//! - REPL/Viz "add data" UX (generate `proposals.json` overlays),
//! - LLM tool-loops (generate reviewable artifacts instead of mutating the DB),
//! - gradual promotion into canonical `.axi`.
//!
//! The output is the generic Evidence/Proposals schema (`ProposalsFileV1`) from
//! `axiograph-ingest-docs`, plus optional `Chunk` evidence suitable for loading
//! into the PathDB WAL.

use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use axiograph_ingest_docs::{Chunk, EvidencePointer, ProposalMetaV1, ProposalSourceV1, ProposalV1, ProposalsFileV1};
use axiograph_pathdb::PathDB;
use axiograph_pathdb::axi_semantics::{MetaPlaneIndex, RelationDecl};

use crate::axql::AxqlContextSpec;
use crate::relation_resolution::{EndpointOrientation, ResolvedSchemaRelation};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposeRelationInputV1 {
    pub rel_type: String,
    pub source_name: String,
    pub target_name: String,
    pub source_type: Option<String>,
    pub target_type: Option<String>,
    /// Optional override: which relation field `source_name` should bind to.
    ///
    /// This is useful for directional relations with semantics-laden field names,
    /// e.g. `Parent(child, parent)` where users naturally say:
    /// - "Jamison is a child of Bob"   (source_field=child, target_field=parent)
    /// - "Bob is a parent of Jamison"  (source_field=parent, target_field=child)
    ///
    /// If provided, both `source_field` and `target_field` must be set.
    #[serde(default)]
    pub source_field: Option<String>,
    /// Optional override: which relation field `target_name` should bind to.
    ///
    /// See `source_field`.
    #[serde(default)]
    pub target_field: Option<String>,
    pub context: Option<String>,
    /// Optional explicit value for a `time`-like field (when the schema has one).
    ///
    /// If omitted and the schema requires a `time` field, we default to:
    /// - the "latest" existing `Time` entity name when available, else
    /// - a deterministic-ish `T<unix_secs>` identifier.
    #[serde(default)]
    pub time: Option<String>,
    pub confidence: Option<f64>,
    pub schema_hint: Option<String>,
    pub public_rationale: Option<String>,
    /// Optional evidence text to store as a `DocChunk` (WAL overlay).
    pub evidence_text: Option<String>,
    /// Optional source locator for the evidence chunk (e.g. "viz_ui").
    pub evidence_locator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposeRelationOutputV1 {
    pub proposals: ProposalsFileV1,
    pub chunks: Vec<Chunk>,
    pub summary: ProposeRelationSummaryV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposeRelationSummaryV1 {
    /// The raw user/LLM-provided relation type (before canonicalization).
    #[serde(default)]
    pub rel_type_input: Option<String>,
    pub rel_type: String,
    /// The raw user/LLM-provided source name (before any endpoint swapping).
    #[serde(default)]
    pub source_name_input: Option<String>,
    pub source_name: String,
    /// The raw user/LLM-provided target name (before any endpoint swapping).
    #[serde(default)]
    pub target_name_input: Option<String>,
    pub target_name: String,
    /// Whether the relation canonicalization swapped endpoints (e.g. `parent_of` → `Parent(child,parent)`).
    #[serde(default)]
    pub swapped_endpoints: bool,
    pub context: Option<String>,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub axi_schema: Option<String>,
    #[serde(default)]
    pub axi_source_field: Option<String>,
    #[serde(default)]
    pub axi_target_field: Option<String>,
    pub confidence: f64,
    pub evidence_chunk_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposeRelationsPairingV1 {
    Cartesian,
    Zip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposeRelationsInputV1 {
    pub rel_type: String,
    pub source_names: Vec<String>,
    pub target_names: Vec<String>,
    #[serde(default)]
    pub pairing: Option<ProposeRelationsPairingV1>,
    pub source_type: Option<String>,
    pub target_type: Option<String>,
    /// Optional override: which relation field `source_names[*]` should bind to.
    #[serde(default)]
    pub source_field: Option<String>,
    /// Optional override: which relation field `target_names[*]` should bind to.
    #[serde(default)]
    pub target_field: Option<String>,
    pub context: Option<String>,
    /// Optional explicit value for a `time`-like field (when the schema has one).
    #[serde(default)]
    pub time: Option<String>,
    pub confidence: Option<f64>,
    pub schema_hint: Option<String>,
    pub public_rationale: Option<String>,
    /// Optional evidence text to store as a single `DocChunk` (WAL overlay) and
    /// attach to every generated proposal.
    pub evidence_text: Option<String>,
    /// Optional source locator for the evidence chunk (e.g. "viz_ui").
    pub evidence_locator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposeRelationsSummaryV1 {
    pub rel_type: String,
    pub sources: usize,
    pub targets: usize,
    pub pairs: usize,
    pub proposals: usize,
    pub context: Option<String>,
    pub confidence: f64,
    pub evidence_chunk_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposeRelationsOutputV1 {
    pub proposals: ProposalsFileV1,
    pub chunks: Vec<Chunk>,
    pub summary: ProposeRelationsSummaryV1,
}

pub fn propose_relation_proposals_v1(
    db: &PathDB,
    default_contexts: &[AxqlContextSpec],
    input: ProposeRelationInputV1,
) -> Result<ProposeRelationOutputV1> {
    let rel_type_input = input.rel_type.trim().to_string();
    if rel_type_input.is_empty() {
        return Err(anyhow!("propose_relation_proposals: rel_type must be non-empty"));
    }
    let source_name_input = input.source_name.trim().to_string();
    if source_name_input.is_empty() {
        return Err(anyhow!(
            "propose_relation_proposals: source_name must be non-empty"
        ));
    }
    let target_name_input = input.target_name.trim().to_string();
    if target_name_input.is_empty() {
        return Err(anyhow!(
            "propose_relation_proposals: target_name must be non-empty"
        ));
    }

    let mut rel_type = rel_type_input.clone();
    let mut source_name = source_name_input.clone();
    let mut target_name = target_name_input.clone();

    let confidence = input.confidence.unwrap_or(0.9).clamp(0.0, 1.0);

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let now_secs = now.as_secs();
    let nonce = now.as_nanos();

    fn attr_string(db: &PathDB, entity_id: u32, key: &str) -> Option<String> {
        let key_id = db.interner.id_of(key)?;
        let value_id = db.entities.get_attr(entity_id, key_id)?;
        db.interner.lookup(value_id).map(|s| s.to_string())
    }

    fn matches_type_hint(db: &PathDB, entity_id: u32, want_type: &str) -> bool {
        let want_type = want_type.trim();
        if want_type.is_empty() {
            return true;
        }
        let Some(view) = db.get_entity(entity_id) else {
            return false;
        };
        if view.entity_type == want_type {
            return true;
        }
        db.find_by_type(want_type)
            .map(|bm| bm.contains(entity_id))
            .unwrap_or(false)
    }

    fn resolve_entity_by_name_case_robust(
        db: &PathDB,
        name: &str,
        type_hint: Option<&str>,
    ) -> Option<u32> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        let want_type = type_hint.map(|s| s.trim()).filter(|s| !s.is_empty());

        // Fast path: exact match against interned value ids.
        if let Some(key_id) = db.interner.id_of("name") {
            if let Some(value_id) = db.interner.id_of(name) {
                let ids = db.entities.entities_with_attr_value(key_id, value_id);
                for id in ids.iter() {
                    if want_type
                        .map(|t| matches_type_hint(db, id, t))
                        .unwrap_or(true)
                    {
                        return Some(id);
                    }
                }
            }
        }

        // Robust fallback: token/fts/fuzzy.
        let mut candidates = db.entities_with_attr_fts("name", name);
        if candidates.is_empty() {
            candidates = db.entities_with_attr_fts_any("name", name);
        }
        if candidates.is_empty() {
            candidates = db.entities_with_attr_fuzzy("name", name, 2);
        }
        if candidates.is_empty() {
            return None;
        }

        let needle_lc = name.to_ascii_lowercase();
        for id in candidates.iter() {
            if let Some(entity_name) = attr_string(db, id, "name") {
                if entity_name.to_ascii_lowercase() == needle_lc
                    && want_type
                        .map(|t| matches_type_hint(db, id, t))
                        .unwrap_or(true)
                {
                    return Some(id);
                }
            }
        }

        for id in candidates.iter() {
            if want_type
                .map(|t| matches_type_hint(db, id, t))
                .unwrap_or(true)
            {
                return Some(id);
            }
        }
        None
    }

    fn sanitize_external_id(raw: &str) -> String {
        let mut out = String::new();
        for c in raw.trim().chars() {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
                out.push(c);
            } else {
                out.push('_');
            }
            if out.len() >= 96 {
                break;
            }
        }
        if out.is_empty() {
            "x".to_string()
        } else {
            out
        }
    }

    fn default_context_name(db: &PathDB, ctxs: &[AxqlContextSpec]) -> Option<String> {
        let spec = ctxs.first()?;
        match spec {
            AxqlContextSpec::Name(name) => Some(name.clone()),
            AxqlContextSpec::EntityId(id) => attr_string(db, *id, "name").or_else(|| Some(id.to_string())),
        }
    }

    fn normalize_key(s: &str) -> String {
        s.chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .map(|c| c.to_ascii_lowercase())
            .collect()
    }

    fn canonicalize_name_of_type(db: &PathDB, type_name: &str, raw: &str) -> Option<String> {
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }

        // If this looks like an entity id, prefer the stored name.
        if let Ok(id) = raw.parse::<u32>() {
            if let Some(view) = db.get_entity(id) {
                if view.entity_type == type_name {
                    if let Some(name) = attr_string(db, id, "name") {
                        return Some(name);
                    }
                }
            }
        }

        // Best-effort: normalize by stripping punctuation/whitespace so inputs like
        // "family tree" can resolve to "FamilyTree".
        let want_key = normalize_key(raw);
        if !want_key.is_empty() {
            if let Some(bm) = db.find_by_type(type_name) {
                for id in bm.iter() {
                    if let Some(name) = attr_string(db, id, "name") {
                        if normalize_key(&name) == want_key {
                            return Some(name);
                        }
                    }
                }
            }
        }

        // Fallback to fuzzy lookup by name.
        resolve_entity_by_name_case_robust(db, raw, Some(type_name))
            .and_then(|id| attr_string(db, id, "name"))
    }

    fn choose_default_time_name(db: &PathDB, now_secs: u64) -> String {
        let Some(times) = db.find_by_type("Time") else {
            return format!("T{now_secs}");
        };

        fn numeric_hint(s: &str) -> Option<u64> {
            let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
            digits.parse::<u64>().ok()
        }

        let mut best: Option<(u64, String)> = None;
        for id in times.iter() {
            let Some(name) = attr_string(db, id, "name") else {
                continue;
            };
            let score = numeric_hint(&name).unwrap_or(0);
            match &best {
                None => best = Some((score, name)),
                Some((best_score, best_name)) => {
                    if score > *best_score || (score == *best_score && name > *best_name) {
                        best = Some((score, name));
                    }
                }
            }
        }

        best.map(|(_, name)| name).unwrap_or_else(|| format!("T{now_secs}"))
    }

    let mut context = input
        .context
        .as_deref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| default_context_name(db, default_contexts));

    // Canonicalize common structured values so "family tree" resolves to "FamilyTree"
    // when a `Context` entity exists.
    if let Some(ctx) = context.clone() {
        context = canonicalize_name_of_type(db, "Context", &ctx).or(Some(ctx));
    }

    // Schema-directed defaults:
    // - infer endpoint types for new entities,
    // - fill common required fields (ctx/time),
    // - and persist an explicit (source_field, target_field) mapping so the importer can
    //   build typed fact nodes (reified tuples) that match the canonical `.axi` schema.
    let mut axi_schema: Option<String> = None;
    let mut axi_source_field: Option<String> = None;
    let mut axi_target_field: Option<String> = None;
    let mut time_name: Option<String> = input
        .time
        .as_deref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let mut source_type_hint: Option<String> = input.source_type.clone();
    let mut target_type_hint: Option<String> = input.target_type.clone();
    let mut schema_hint: Option<String> = input.schema_hint.clone();
    let mut swapped_endpoints = false;
    let mut rel_alias_used: Option<String> = None;

    let meta = MetaPlaneIndex::from_db(db).unwrap_or_default();
    if !meta.schemas.is_empty() {
        let schema_hint_raw = schema_hint
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());
        if let Some(resolved) =
            crate::relation_resolution::resolve_schema_relation(&meta, schema_hint_raw, &rel_type)
        {
            let ResolvedSchemaRelation {
                schema_name,
                schema,
                rel_decl,
                rel_name,
                orientation,
                alias_used,
            } = resolved;

            rel_alias_used = alias_used;

            // If the caller explicitly pins source/target fields, do not apply
            // endpoint swapping: the mapping is already explicit.
            let explicit_field_override = input
                .source_field
                .as_deref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false)
                || input
                    .target_field
                    .as_deref()
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);

            if orientation == EndpointOrientation::Swap && !explicit_field_override {
                swapped_endpoints = true;
                std::mem::swap(&mut source_name, &mut target_name);
                std::mem::swap(&mut source_type_hint, &mut target_type_hint);
            }

            rel_type = rel_name.clone();
            axi_schema = Some(schema_name.clone());
            if schema_hint.is_none() {
                // For downstream reconciliation/promote tools, a schema name is a reasonable default hint.
                schema_hint = Some(schema_name.clone());
            }

            let (mut src_field, mut dst_field) = infer_endpoint_fields(rel_decl)?;

            // Optional override: bind (source,target) to explicit field names.
            let input_src_field = input
                .source_field
                .as_deref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let input_dst_field = input
                .target_field
                .as_deref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            match (&input_src_field, &input_dst_field) {
                (None, None) => {}
                (Some(_), None) | (None, Some(_)) => {
                    return Err(anyhow!(
                        "relation `{rel_type}`: if you set `source_field`, you must also set `target_field` (and vice versa)"
                    ));
                }
                (Some(sf), Some(tf)) => {
                    if sf == tf {
                        return Err(anyhow!(
                            "relation `{rel_type}`: source_field and target_field must be different (got {sf:?})"
                        ));
                    }
                    let has_field = |name: &str| rel_decl.fields.iter().any(|f| f.field_name == name);
                    if !has_field(sf) {
                        return Err(anyhow!(
                            "relation `{rel_type}`: unknown source_field `{sf}`"
                        ));
                    }
                    if !has_field(tf) {
                        return Err(anyhow!(
                            "relation `{rel_type}`: unknown target_field `{tf}`"
                        ));
                    }
                    src_field = sf.clone();
                    dst_field = tf.clone();
                }
            }
            axi_source_field = Some(src_field.clone());
            axi_target_field = Some(dst_field.clone());

            let extra_fields: Vec<&str> = rel_decl
                .fields
                .iter()
                .map(|f| f.field_name.as_str())
                .filter(|f| *f != src_field && *f != dst_field)
                .collect();

            // Fill ctx/time if present; reject other extra fields so the UI/LLM must
            // ask for explicit values (prevents injecting junk placeholders).
            for f in &extra_fields {
                if *f == "ctx" {
                    if context.is_none() {
                        // If the schema requires a context, pick a sensible default.
                        // (User can always override with `context=...`.)
                        context = Some("Observed".to_string());
                    }
                } else if *f == "time" {
                    if time_name.is_none() {
                        time_name = Some(choose_default_time_name(db, now_secs));
                    }
                } else {
                    return Err(anyhow!(
                        "relation `{rel_type}` requires additional field `{f}` (only ctx/time can be defaulted); use a richer fact builder or specify the missing field"
                    ));
                }
            }

            // Infer endpoint entity types when not explicitly provided.
            if source_type_hint.as_deref().unwrap_or("").trim().is_empty() {
                if let Some(ft) = rel_decl
                    .fields
                    .iter()
                    .find(|f| f.field_name == src_field)
                    .map(|f| f.field_type.clone())
                {
                    source_type_hint = Some(ft);
                }
            }
            if target_type_hint.as_deref().unwrap_or("").trim().is_empty() {
                if let Some(ft) = rel_decl
                    .fields
                    .iter()
                    .find(|f| f.field_name == dst_field)
                    .map(|f| f.field_type.clone())
                {
                    target_type_hint = Some(ft);
                }
            }

            // Sanity: relation decl should belong to this schema, but double-check we don't mis-hint.
            let _ = schema; // keep for future use
        }
    }

    let public_rationale = input.public_rationale.unwrap_or_else(|| {
        format!("Proposed relation assertion: {source_name} -{rel_type}-> {target_name}.")
    });

    // Decide whether endpoints already exist in the snapshot.
    let src_existing =
        resolve_entity_by_name_case_robust(db, &source_name, source_type_hint.as_deref());
    let dst_existing =
        resolve_entity_by_name_case_robust(db, &target_name, target_type_hint.as_deref());

    // Prefer stable external ids when we have them (e.g. proposal overlays may
    // have already introduced `external_id` attrs). Fall back to canonical
    // `name` so this works against accepted-plane imports too.
    let src_ref = src_existing
        .and_then(|id| attr_string(db, id, "external_id").or_else(|| attr_string(db, id, "name")))
        .unwrap_or_else(|| source_name.to_string());
    let dst_ref = dst_existing
        .and_then(|id| attr_string(db, id, "external_id").or_else(|| attr_string(db, id, "name")))
        .unwrap_or_else(|| target_name.to_string());

    // Optional evidence chunk (conversation / UI).
    let evidence_text = input.evidence_text.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let evidence_locator = input
        .evidence_locator
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "conversation".to_string());

    let mut chunks: Vec<Chunk> = Vec::new();
    let evidence_ptrs: Vec<EvidencePointer> = if let Some(text) = evidence_text.as_ref() {
        let chunk_id = format!("chunk_viz_ui_{nonce}");
        let document_id = format!("doc_viz_ui_{nonce}");
        chunks.push(Chunk {
            chunk_id: chunk_id.clone(),
            document_id: document_id.clone(),
            page: None,
            span_id: "user_input".to_string(),
            text: text.clone(),
            bbox: None,
            metadata: std::collections::HashMap::new(),
        });
        vec![EvidencePointer {
            chunk_id,
            locator: Some(evidence_locator.clone()),
            span_id: None,
        }]
    } else {
        Vec::new()
    };

    let mut file = ProposalsFileV1 {
        version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
        generated_at: now_secs.to_string(),
        source: ProposalSourceV1 {
            source_type: "conversation".to_string(),
            locator: "viz_ui".to_string(),
        },
        schema_hint: schema_hint.clone(),
        proposals: Vec::new(),
    };

    fn push_entity_proposal(
        file: &mut ProposalsFileV1,
        confidence: f64,
        schema_hint: Option<String>,
        entity_id: String,
        entity_type: String,
        name: String,
        evidence: &[EvidencePointer],
        public_rationale: &str,
    ) {
        let meta = ProposalMetaV1 {
            proposal_id: entity_id.clone(),
            confidence,
            evidence: evidence.to_vec(),
            public_rationale: public_rationale.to_string(),
            metadata: std::collections::HashMap::new(),
            schema_hint,
        };
        file.proposals.push(ProposalV1::Entity {
            meta,
            entity_id,
            entity_type,
            name,
            attributes: std::collections::HashMap::new(),
            description: None,
        });
    }

    // Only propose endpoint entities when we fail to resolve them in the current
    // snapshot (prevents duplicate entity proposals for accepted-plane nodes).
    let mut src_entity_id: Option<String> = None;
    if src_existing.is_none() {
        let entity_type = source_type_hint
            .clone()
            .unwrap_or_else(|| "UnknownEntity".to_string());
        let entity_id = format!(
            "entity::{entity_type}::{}",
            sanitize_external_id(&source_name)
        );
        push_entity_proposal(
            &mut file,
            confidence,
            schema_hint.clone(),
            entity_id.clone(),
            entity_type,
            source_name.to_string(),
            &evidence_ptrs,
            &public_rationale,
        );
        src_entity_id = Some(entity_id);
    }

    let mut dst_entity_id: Option<String> = None;
    if dst_existing.is_none() {
        let entity_type = target_type_hint
            .clone()
            .unwrap_or_else(|| "UnknownEntity".to_string());
        let entity_id = format!(
            "entity::{entity_type}::{}",
            sanitize_external_id(&target_name)
        );
        push_entity_proposal(
            &mut file,
            confidence,
            schema_hint.clone(),
            entity_id.clone(),
            entity_type,
            target_name.to_string(),
            &evidence_ptrs,
            &public_rationale,
        );
        dst_entity_id = Some(entity_id);
    }

    let relation_id = format!(
        "rel::{rel_type}::{}::{}",
        sanitize_external_id(&src_ref),
        sanitize_external_id(&dst_ref)
    );
    let meta = ProposalMetaV1 {
        proposal_id: relation_id.clone(),
        confidence,
        evidence: evidence_ptrs.clone(),
        public_rationale: public_rationale.clone(),
        metadata: std::collections::HashMap::new(),
        schema_hint: schema_hint.clone(),
    };
    let mut attributes = std::collections::HashMap::<String, String>::new();
    // Preserve the original user/LLM surface relation label for UX/debugging.
    attributes.insert("axi_rel_type_input".to_string(), rel_type_input.clone());
    if let Some(alias) = rel_alias_used.as_ref() {
        attributes.insert("axi_rel_alias".to_string(), alias.clone());
    }
    if swapped_endpoints {
        attributes.insert("axi_rel_swapped".to_string(), "true".to_string());
    }
    if let Some(schema) = axi_schema.as_ref() {
        attributes.insert("axi_schema".to_string(), schema.clone());
    }
    if let Some(sf) = axi_source_field.as_ref() {
        attributes.insert("axi_source_field".to_string(), sf.clone());
    }
    if let Some(tf) = axi_target_field.as_ref() {
        attributes.insert("axi_target_field".to_string(), tf.clone());
    }
    if let Some(ctx) = context.as_ref() {
        attributes.insert("context".to_string(), ctx.clone());
        attributes.insert("ctx".to_string(), ctx.clone());
    }
    if let Some(t) = time_name.as_ref() {
        attributes.insert("time".to_string(), t.clone());
    }

    file.proposals.push(ProposalV1::Relation {
        meta,
        relation_id: relation_id.clone(),
        rel_type: rel_type.to_string(),
        source: src_entity_id.unwrap_or(src_ref),
        target: dst_entity_id.unwrap_or(dst_ref),
        attributes,
    });

    let evidence_chunk_id = chunks.first().map(|c| c.chunk_id.clone());

    Ok(ProposeRelationOutputV1 {
        proposals: file,
        chunks,
        summary: ProposeRelationSummaryV1 {
            rel_type_input: Some(rel_type_input),
            rel_type: rel_type.to_string(),
            source_name_input: Some(source_name_input),
            source_name: source_name.to_string(),
            target_name_input: Some(target_name_input),
            target_name: target_name.to_string(),
            swapped_endpoints,
            context,
            time: time_name,
            axi_schema,
            axi_source_field,
            axi_target_field,
            confidence,
            evidence_chunk_id,
        },
    })
}

fn infer_endpoint_fields(rel_decl: &RelationDecl) -> Result<(String, String)> {
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
        return Ok((
            rel_decl.fields[0].field_name.clone(),
            rel_decl.fields[1].field_name.clone(),
        ));
    }
    Err(anyhow!(
        "relation `{}` has fewer than 2 fields (cannot infer source/target)",
        rel_decl.name
    ))
}

pub fn propose_relations_proposals_v1(
    db: &PathDB,
    default_contexts: &[AxqlContextSpec],
    input: ProposeRelationsInputV1,
) -> Result<ProposeRelationsOutputV1> {
    let rel_type = input.rel_type.trim();
    if rel_type.is_empty() {
        return Err(anyhow!("propose_relations_proposals: rel_type must be non-empty"));
    }

    let sources: Vec<String> = input
        .source_names
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if sources.is_empty() {
        return Err(anyhow!(
            "propose_relations_proposals: source_names must be non-empty"
        ));
    }

    let targets: Vec<String> = input
        .target_names
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if targets.is_empty() {
        return Err(anyhow!(
            "propose_relations_proposals: target_names must be non-empty"
        ));
    }

    let pairing = input
        .pairing
        .clone()
        .unwrap_or(ProposeRelationsPairingV1::Cartesian);
    let pairs: Vec<(String, String)> = match pairing {
        ProposeRelationsPairingV1::Cartesian => {
            let mut out = Vec::new();
            for s in &sources {
                for t in &targets {
                    out.push((s.clone(), t.clone()));
                }
            }
            out
        }
        ProposeRelationsPairingV1::Zip => {
            if sources.len() != targets.len() {
                return Err(anyhow!(
                    "propose_relations_proposals: zip pairing requires source_names.len == target_names.len (got {} vs {})",
                    sources.len(),
                    targets.len()
                ));
            }
            sources
                .iter()
                .cloned()
                .zip(targets.iter().cloned())
                .collect()
        }
    };

    let confidence = input.confidence.unwrap_or(0.9).clamp(0.0, 1.0);

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let now_secs = now.as_secs();
    let nonce = now.as_nanos();

    // Optional evidence chunk (conversation / UI). We generate one chunk and
    // reuse it for every proposal in this batch to keep the evidence plane tidy.
    let evidence_text = input
        .evidence_text
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let evidence_locator = input
        .evidence_locator
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "conversation".to_string());

    let mut chunks: Vec<Chunk> = Vec::new();
    let evidence_ptrs: Vec<EvidencePointer> = if let Some(text) = evidence_text.as_ref() {
        let chunk_id = format!("chunk_viz_ui_{nonce}");
        let document_id = format!("doc_viz_ui_{nonce}");
        chunks.push(Chunk {
            chunk_id: chunk_id.clone(),
            document_id: document_id.clone(),
            page: None,
            span_id: "user_input".to_string(),
            text: text.clone(),
            bbox: None,
            metadata: std::collections::HashMap::new(),
        });
        vec![EvidencePointer {
            chunk_id,
            locator: Some(evidence_locator.clone()),
            span_id: None,
        }]
    } else {
        Vec::new()
    };

    let mut file = ProposalsFileV1 {
        version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
        generated_at: now_secs.to_string(),
        source: ProposalSourceV1 {
            source_type: "conversation".to_string(),
            locator: "viz_ui".to_string(),
        },
        schema_hint: input.schema_hint.clone(),
        proposals: Vec::new(),
    };

    let mut seen: HashSet<String> = HashSet::new();
    for (source_name, target_name) in pairs.iter() {
        let out = propose_relation_proposals_v1(
            db,
            default_contexts,
            ProposeRelationInputV1 {
                rel_type: rel_type.to_string(),
                source_name: source_name.clone(),
                target_name: target_name.clone(),
                source_type: input.source_type.clone(),
                target_type: input.target_type.clone(),
                source_field: input.source_field.clone(),
                target_field: input.target_field.clone(),
                context: input.context.clone(),
                time: input.time.clone(),
                confidence: Some(confidence),
                schema_hint: input.schema_hint.clone(),
                public_rationale: input.public_rationale.clone(),
                evidence_text: None,
                evidence_locator: Some(evidence_locator.clone()),
            },
        )?;

        for mut p in out.proposals.proposals.into_iter() {
            // Attach the shared evidence chunk (if any).
            if !evidence_ptrs.is_empty() {
                match &mut p {
                    ProposalV1::Entity { meta, .. } => {
                        meta.evidence.extend(evidence_ptrs.clone());
                    }
                    ProposalV1::Relation { meta, .. } => {
                        meta.evidence.extend(evidence_ptrs.clone());
                    }
                }
            }

            let proposal_id = match &p {
                ProposalV1::Entity { meta, .. } => meta.proposal_id.clone(),
                ProposalV1::Relation { meta, .. } => meta.proposal_id.clone(),
            };
            if seen.insert(proposal_id) {
                file.proposals.push(p);
            }
        }
    }

    let proposals_count = file.proposals.len();
    let evidence_chunk_id = chunks.first().map(|c| c.chunk_id.clone());

    Ok(ProposeRelationsOutputV1 {
        proposals: file,
        chunks,
        summary: ProposeRelationsSummaryV1 {
            rel_type: rel_type.to_string(),
            sources: sources.len(),
            targets: targets.len(),
            pairs: pairs.len(),
            proposals: proposals_count,
            context: input.context.clone(),
            confidence,
            evidence_chunk_id,
        },
    })
}
