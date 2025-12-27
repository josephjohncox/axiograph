//! Visualization / exploration helpers.
//!
//! This module intentionally lives in the CLI crate:
//! - it is *tooling* (untrusted/evidence-plane friendly),
//! - it should not bloat the core PathDB crate,
//! - and it can evolve quickly without touching certificate semantics.
//!
//! The goal is to make it easy to inspect a PathDB snapshot or imported `.axi`
//! module as a small “neighborhood graph” around an entity of interest.
//!
//! Output formats:
//! - Graphviz DOT (best-in-class layout, external tooling)
//! - Self-contained HTML explorer (no deps; works offline)
//! - JSON (for custom frontends)

#![allow(dead_code)]

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use axiograph_pathdb::axi_meta::{
    ATTR_AXI_RELATION, ATTR_AXI_SCHEMA, ATTR_CONSTRAINT_RELATION, ATTR_FIELD_TYPE, REL_AXI_FACT_IN_CONTEXT,
};
use axiograph_pathdb::axi_semantics::{ConstraintDecl, MetaPlaneIndex, SchemaIndex};
use axiograph_pathdb::PathDB;

const ATTR_OVERLAY_SUPERTYPES: &str = "axi_overlay_supertypes";
const ATTR_OVERLAY_RELATION_SIGNATURE: &str = "axi_overlay_relation_signature";
const ATTR_OVERLAY_CONSTRAINTS: &str = "axi_overlay_constraints";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VizFormat {
    Dot,
    Html,
    Json,
}

impl VizFormat {
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "dot" => Ok(Self::Dot),
            "html" | "htm" => Ok(Self::Html),
            "json" => Ok(Self::Json),
            other => Err(anyhow!(
                "unknown viz format `{other}` (expected dot|html|json)"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VizDirection {
    Out,
    In,
    Both,
}

impl VizDirection {
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "out" | "outgoing" => Ok(Self::Out),
            "in" | "incoming" => Ok(Self::In),
            "both" | "bi" | "bidir" | "bidirectional" => Ok(Self::Both),
            other => Err(anyhow!(
                "unknown direction `{other}` (expected out|in|both)"
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VizOptions {
    pub focus_ids: Vec<u32>,
    pub hops: usize,
    pub max_nodes: usize,
    pub max_edges: usize,
    pub direction: VizDirection,
    pub include_meta_plane: bool,
    pub include_data_plane: bool,
    pub include_equivalences: bool,
    /// When true, annotate data-plane nodes using the `.axi` meta-plane as a type layer.
    ///
    /// This is an "explain what the system knows" overlay:
    /// - inferred supertypes for object entities
    /// - relation signatures + theory constraints for fact nodes
    pub typed_overlay: bool,
}

impl Default for VizOptions {
    fn default() -> Self {
        Self {
            focus_ids: Vec::new(),
            hops: 2,
            max_nodes: 250,
            max_edges: 4_000,
            direction: VizDirection::Both,
            include_meta_plane: false,
            include_data_plane: true,
            include_equivalences: true,
            typed_overlay: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VizGraph {
    pub nodes: Vec<VizNode>,
    pub edges: Vec<VizEdge>,
    pub truncated: bool,
    pub summary: VizSummary,
    /// Context/world nodes referenced by tuple-like nodes in this view.
    ///
    /// This is provided by the server so clients can render stable context
    /// names even when the context nodes are not included in the neighborhood
    /// graph (edge truncation / max_nodes).
    #[serde(default)]
    pub contexts: Vec<VizContext>,
    /// Tuple-like node context membership.
    ///
    /// Key: tuple node id (fact/morphism/homotopy)
    /// Value: list of context ids the tuple is asserted in.
    #[serde(default)]
    pub tuple_contexts: BTreeMap<u32, Vec<u32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VizContext {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VizSummary {
    pub focus_ids: Vec<u32>,
    pub hops: usize,
    pub max_nodes: usize,
    pub max_edges: usize,
    pub include_meta_plane: bool,
    pub include_data_plane: bool,
    pub include_equivalences: bool,
    #[serde(default)]
    pub typed_overlay: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VizNode {
    pub id: u32,
    pub entity_type: String,
    #[serde(default)]
    pub kind: String, // "meta" | "fact" | "morphism" | "homotopy" | "entity"
    /// UI-friendly type label.
    ///
    /// For tuple-like nodes (fact/morphism/homotopy), this prefers `axi_relation`
    /// so grouping and search can treat `Parent_fact_...` as `Parent`, etc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_label: Option<String>,
    /// High-level plane classification for UI filtering.
    ///
    /// Values:
    /// - `meta`     (schema/theory layer imported into PathDB)
    /// - `accepted` (canonical meaning plane imported from reviewed `.axi`)
    /// - `evidence` (WAL overlays: proposals/chunks/provenance)
    /// - `data`     (generic runtime data not tagged as accepted/evidence)
    #[serde(default)]
    pub plane: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// UI-friendly display name (computed by the backend).
    ///
    /// This moves some summarization logic out of the HTML explorer so other
    /// clients (JSON, dot, server-mode UIs) share consistent labeling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub attrs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VizEdge {
    pub source: u32,
    pub target: u32,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    /// Underlying PathDB relation id (when this edge corresponds to a real relation-store row).
    ///
    /// This enables the UI/server to emit anchored reachability certificates for selected paths.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation_id: Option<u32>,
    #[serde(default)]
    pub kind: String, // "relation" | "equivalence"
}

pub fn resolve_focus_by_name(db: &PathDB, name: &str) -> Result<Option<u32>> {
    resolve_focus_by_name_and_type(db, name, None)
}

pub fn resolve_focus_by_name_and_type(
    db: &PathDB,
    name: &str,
    focus_type: Option<&str>,
) -> Result<Option<u32>> {
    let Some(key_id) = db.interner.id_of("name") else {
        return Ok(None);
    };
    let Some(value_id) = db.interner.id_of(name) else {
        return Ok(None);
    };
    let ids = db.entities.entities_with_attr_value(key_id, value_id);
    if let Some(want_type) = focus_type {
        let want_type_id = db.interner.id_of(want_type);
        let want_type_membership = want_type_id.and_then(|tid| db.entities.by_type(tid));
        for id in ids.iter() {
            let Some(view) = db.get_entity(id) else {
                continue;
            };
            // Prefer exact entity type matches.
            if view.entity_type == want_type {
                return Ok(Some(id));
            }
            // Also allow "virtual types" (e.g. Morphism/Homotopy) which are stored
            // in the type index but do not necessarily change the entity's base type.
            if want_type_membership
                .map(|bm| bm.contains(id))
                .unwrap_or(false)
            {
                return Ok(Some(id));
            }
        }
        return Ok(None);
    }
    Ok(ids.iter().next())
}

fn is_meta_plane_entity(entity_type: &str) -> bool {
    entity_type.starts_with("AxiMeta")
}

fn node_kind(
    entity_type: &str,
    is_morphism: bool,
    is_homotopy: bool,
    attrs: &BTreeMap<String, String>,
) -> &'static str {
    if is_meta_plane_entity(entity_type) {
        return "meta";
    }
    if is_homotopy {
        return "homotopy";
    }
    if is_morphism {
        return "morphism";
    }
    if attrs.contains_key(ATTR_AXI_RELATION) {
        return "fact";
    }
    "entity"
}

fn node_plane(entity_type: &str, kind: &str, attrs: &BTreeMap<String, String>) -> &'static str {
    if kind == "meta" || is_meta_plane_entity(entity_type) {
        return "meta";
    }
    // Evidence-plane nodes should remain evidence even if they carry `axi_*`
    // typing metadata (e.g. schema-directed proposal facts).
    if entity_type == "DocChunk"
        || entity_type == "Document"
        || entity_type == "ProposalRun"
        || attrs.contains_key("proposal_id")
        || attrs.contains_key("proposals_digest")
        || attrs.contains_key("chunk_id")
    {
        return "evidence";
    }
    if attrs.contains_key("axi_fact_id")
        || attrs.contains_key("axi_module")
        || attrs.contains_key("axi_schema")
        || attrs.contains_key("axi_dialect")
    {
        return "accepted";
    }
    "data"
}

fn schema_for_entity<'a>(
    meta: &'a MetaPlaneIndex,
    attrs: &BTreeMap<String, String>,
) -> Option<&'a SchemaIndex> {
    if let Some(schema_name) = attrs.get(ATTR_AXI_SCHEMA) {
        if let Some(schema) = meta.schemas.get(schema_name) {
            return Some(schema);
        }
    }
    if meta.schemas.len() == 1 {
        if let Some((_name, schema)) = meta.schemas.iter().next() {
            return Some(schema);
        }
    }
    None
}

fn db_entity_short_label(db: &PathDB, id: u32) -> String {
    let Some(view) = db.get_entity(id) else {
        return id.to_string();
    };
    if let Some(name) = view.attrs.get("name") {
        let name = name.trim();
        if !name.is_empty() {
            return name.to_string();
        }
    }
    format!("{}#{}", view.entity_type, id)
}

fn type_label_for_node(entity_type: &str, kind: &str, attrs: &BTreeMap<String, String>) -> Option<String> {
    if matches!(kind, "fact" | "morphism" | "homotopy") {
        if let Some(r) = attrs.get(ATTR_AXI_RELATION) {
            let r = r.trim();
            if !r.is_empty() {
                return Some(r.to_string());
            }
        }
        return Some(entity_type.to_string());
    }
    None
}

fn display_name_for_record_from_decl(
    db: &PathDB,
    tuple_id: u32,
    relation_name: &str,
    rel_decl: &axiograph_pathdb::axi_semantics::RelationDecl,
) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for f in &rel_decl.fields {
        let targets = db.follow_one(tuple_id, &f.field_name);
        if let Some(tid) = targets.iter().next() {
            parts.push(format!("{}={}", f.field_name, db_entity_short_label(db, tid)));
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(format!("{relation_name}({})", parts.join(", ")))
    }
}

fn shorten_hash(s: &str, max_len: usize) -> String {
    let s = s.trim();
    if s.is_empty() {
        return String::new();
    }
    if s.chars().count() <= max_len {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_len).collect();
    out.push('…');
    out
}

fn short_locator(s: &str) -> String {
    let s0 = s.trim();
    if s0.is_empty() {
        return String::new();
    }
    let s = s0.split(['?', '#']).next().unwrap_or(s0);
    let parts: Vec<&str> = s.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    if let Some(last) = parts.last() {
        return last.to_string();
    }
    shorten_hash(s0, 36)
}

fn display_name_for_node(
    db: &PathDB,
    node_id: u32,
    entity_type: &str,
    kind: &str,
    name: Option<&str>,
    attrs: &BTreeMap<String, String>,
    meta: Option<&MetaPlaneIndex>,
) -> Option<String> {
    // Specialized “tuple-like” summaries.
    if matches!(kind, "morphism" | "homotopy" | "fact") {
        let rel = attrs
            .get(ATTR_AXI_RELATION)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| entity_type.to_string());

        if kind == "morphism" {
            let from = db.follow_one(node_id, "from").iter().next();
            let to = db.follow_one(node_id, "to").iter().next();
            if let (Some(from), Some(to)) = (from, to) {
                return Some(format!(
                    "{rel}: {} → {}",
                    db_entity_short_label(db, from),
                    db_entity_short_label(db, to)
                ));
            }
        } else if kind == "homotopy" {
            let lhs = db.follow_one(node_id, "lhs").iter().next();
            let rhs = db.follow_one(node_id, "rhs").iter().next();
            if let (Some(lhs), Some(rhs)) = (lhs, rhs) {
                return Some(format!(
                    "{rel}: {} ≃ {}",
                    db_entity_short_label(db, lhs),
                    db_entity_short_label(db, rhs)
                ));
            }
        }

        // For fact nodes (and tuple-like fallbacks), prefer a schema-directed record summary.
        if let Some(meta) = meta {
            if let Some(schema) = schema_for_entity(meta, attrs) {
                if let Some(rel_decl) = schema.relation_decls.get(&rel) {
                    if let Some(summary) = display_name_for_record_from_decl(db, node_id, &rel, rel_decl) {
                        return Some(summary);
                    }
                }
            }
        }

        // Otherwise, fall back to a short relation label.
        return Some(rel);
    }

    // Evidence-plane convenience labeling (keeps the explorer scannable).
    if entity_type == "ProposalRun" {
        let digest = attrs
            .get("proposals_digest")
            .map(|s| shorten_hash(s, 10))
            .unwrap_or_default();
        let hint = attrs.get("schema_hint").map(|s| s.trim()).unwrap_or("");
        let stype = attrs.get("source_type").map(|s| s.trim()).unwrap_or("");
        let loc_raw = attrs
            .get("source_locator")
            .or_else(|| attrs.get("source"))
            .map(|s| s.as_str())
            .unwrap_or("");
        let loc = short_locator(loc_raw);
        let tag = if !hint.is_empty() {
            hint
        } else if !stype.is_empty() {
            stype
        } else {
            "run"
        };
        if !digest.is_empty() && !loc.is_empty() {
            return Some(format!("{tag} @ {loc} ({digest})"));
        }
        if !digest.is_empty() {
            return Some(format!("{tag} ({digest})"));
        }
        if !loc.is_empty() {
            return Some(format!("{tag} @ {loc}"));
        }
        return Some(tag.to_string());
    }

    if entity_type == "Document" {
        if let Some(doc) = attrs.get("document_id").map(|s| short_locator(s)).filter(|s| !s.is_empty()) {
            return Some(format!("doc {doc}"));
        }
    }

    if entity_type == "DocChunk" {
        let chunk = attrs
            .get("chunk_id")
            .map(|s| short_locator(s))
            .unwrap_or_default();
        let about = db
            .follow_one(node_id, "doc_chunk_about")
            .iter()
            .next()
            .map(|id| db_entity_short_label(db, id))
            .unwrap_or_default();
        if !chunk.is_empty() && !about.is_empty() {
            return Some(format!("chunk {chunk} (about {about})"));
        }
        if !chunk.is_empty() {
            return Some(format!("chunk {chunk}"));
        }
        if !about.is_empty() {
            return Some(format!("chunk (about {about})"));
        }
        return Some("chunk".to_string());
    }

    // Default: let clients use `name`/type/id directly.
    let _ = name;
    None
}

pub fn extract_viz_graph(db: &PathDB, options: &VizOptions) -> Result<VizGraph> {
    if options.typed_overlay {
        let meta = MetaPlaneIndex::from_db(db)?;
        extract_viz_graph_with_meta(db, options, Some(&meta))
    } else {
        // Best-effort: even when `typed_overlay` is off, the meta-plane is still
        // useful for UI-friendly labeling (record summaries, etc). If the
        // snapshot has no `.axi` meta-plane, this stays `None`.
        let meta = MetaPlaneIndex::from_db(db).ok();
        extract_viz_graph_with_meta(db, options, meta.as_ref())
    }
}

pub fn extract_viz_graph_with_meta(
    db: &PathDB,
    options: &VizOptions,
    meta: Option<&MetaPlaneIndex>,
) -> Result<VizGraph> {
    let hops = options.hops;
    let max_nodes = options.max_nodes.max(1);
    let max_edges = options.max_edges.max(1);

    // Build lightweight neighbor maps once.
    //
    // We intentionally keep these maps *untyped*: for neighborhood expansion we
    // only need to know which nodes are adjacent. We re-scan the RelationStore
    // later to materialize labeled edges within the selected node set.
    //
    // This keeps the extraction runtime reasonable even when a graph has many
    // relation labels, and avoids repeatedly scanning the `(source, rel_type)`
    // index for every visited node.
    let mut out_neighbors: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut in_neighbors: HashMap<u32, Vec<u32>> = HashMap::new();
    for rel_id in 0..db.relations.len() as u32 {
        let Some(rel) = db.relations.get_relation(rel_id) else {
            continue;
        };
        out_neighbors
            .entry(rel.source)
            .or_default()
            .push(rel.target);
        in_neighbors.entry(rel.target).or_default().push(rel.source);
    }

    let mut nodes: Vec<u32> = Vec::new();
    let mut visited: HashSet<u32> = HashSet::new();

    let mut queue: VecDeque<(u32, usize)> = VecDeque::new();
    if options.focus_ids.is_empty() {
        // Fallback: show the first few entity ids deterministically.
        for id in 0..(db.entities.len() as u32) {
            queue.push_back((id, 0));
            if queue.len() >= 1 {
                break;
            }
        }
    } else {
        for id in &options.focus_ids {
            queue.push_back((*id, 0));
        }
    }

    while let Some((entity, depth)) = queue.pop_front() {
        if visited.contains(&entity) {
            continue;
        }
        if nodes.len() >= max_nodes {
            break;
        }

        let Some(view) = db.get_entity(entity) else {
            continue;
        };
        let is_meta = is_meta_plane_entity(&view.entity_type);
        if is_meta && !options.include_meta_plane {
            // Skip meta-plane nodes unless explicitly requested.
            continue;
        }
        if !is_meta && !options.include_data_plane {
            // Skip data-plane nodes unless explicitly requested.
            continue;
        }

        visited.insert(entity);
        nodes.push(entity);

        if depth >= hops {
            continue;
        }

        // Expand neighbors (directional).
        if matches!(options.direction, VizDirection::Out | VizDirection::Both) {
            if let Some(list) = out_neighbors.get(&entity) {
                for &target in list {
                    if visited.contains(&target) {
                        continue;
                    }
                    if queue.len() + nodes.len() >= max_nodes * 6 {
                        continue;
                    }
                    queue.push_back((target, depth + 1));
                }
            }
        }
        if matches!(options.direction, VizDirection::In | VizDirection::Both) {
            if let Some(list) = in_neighbors.get(&entity) {
                for &source in list {
                    if visited.contains(&source) {
                        continue;
                    }
                    if queue.len() + nodes.len() >= max_nodes * 6 {
                        continue;
                    }
                    queue.push_back((source, depth + 1));
                }
            }
        }
    }

    let selected: HashSet<u32> = nodes.iter().copied().collect();

    // Materialize node views (stable ordering by id).
    let mut node_views: Vec<VizNode> = Vec::with_capacity(nodes.len());
    let mut sorted_nodes = nodes;
    sorted_nodes.sort_unstable();

    let mut kind_by_id: HashMap<u32, String> = HashMap::new();
    let morphism_membership = db
        .interner
        .id_of("Morphism")
        .and_then(|tid| db.entities.by_type(tid));
    let homotopy_membership = db
        .interner
        .id_of("Homotopy")
        .and_then(|tid| db.entities.by_type(tid));

    fn format_constraints(decls: &[ConstraintDecl]) -> String {
        let mut parts: Vec<String> = Vec::new();
        for d in decls {
            match d {
                ConstraintDecl::Functional {
                    src_field,
                    dst_field,
                    ..
                } => parts.push(format!("functional({src_field} -> {dst_field})")),
                ConstraintDecl::Symmetric { .. } => parts.push("symmetric".to_string()),
                ConstraintDecl::Transitive { .. } => parts.push("transitive".to_string()),
                ConstraintDecl::Key { fields, .. } => {
                    parts.push(format!("key({})", fields.join(", ")))
                }
                ConstraintDecl::Unknown { text, .. } => parts.push(format!("unknown({text})")),
            }
        }
        parts.join("; ")
    }
    for id in &sorted_nodes {
        let Some(view) = db.get_entity(*id) else {
            continue;
        };
        let name = view.attrs.get("name").cloned();
        let mut attrs: BTreeMap<String, String> = BTreeMap::new();
        for (k, v) in view.attrs {
            attrs.insert(k, v);
        }
        let is_morphism = morphism_membership
            .map(|bm| bm.contains(*id))
            .unwrap_or(false);
        let is_homotopy = homotopy_membership
            .map(|bm| bm.contains(*id))
            .unwrap_or(false);
        let kind = node_kind(&view.entity_type, is_morphism, is_homotopy, &attrs).to_string();

        if options.typed_overlay && kind != "meta" {
            if let Some(meta) = meta {
                if let Some(schema) = schema_for_entity(meta, &attrs) {
                    // Inferred supertypes for object entities.
                    if let Some(supers) = schema.supertypes_of.get(&view.entity_type) {
                        let mut list: Vec<String> = supers.iter().cloned().collect();
                        list.sort();
                        attrs.insert(ATTR_OVERLAY_SUPERTYPES.to_string(), list.join(", "));
                    }

                    // Relation signature + theory constraints for fact nodes.
                    if attrs.contains_key(ATTR_AXI_RELATION) {
                        let relation_name = attrs
                            .get(ATTR_AXI_RELATION)
                            .cloned()
                            .unwrap_or_else(|| view.entity_type.clone());
                        if let Some(rel_decl) = schema.relation_decls.get(&relation_name) {
                            let fields = rel_decl
                                .fields
                                .iter()
                                .map(|f| format!("{}: {}", f.field_name, f.field_type))
                                .collect::<Vec<_>>()
                                .join(", ");
                            attrs.insert(
                                ATTR_OVERLAY_RELATION_SIGNATURE.to_string(),
                                format!("{relation_name}({fields})"),
                            );
                        }
                        if let Some(constraints) =
                            schema.constraints_by_relation.get(&relation_name)
                        {
                            if !constraints.is_empty() {
                                attrs.insert(
                                    ATTR_OVERLAY_CONSTRAINTS.to_string(),
                                    format_constraints(constraints),
                                );
                            }
                        }
                    }
                }
            }
        }

        let plane = node_plane(&view.entity_type, kind.as_str(), &attrs).to_string();
        let type_label = type_label_for_node(&view.entity_type, kind.as_str(), &attrs);
        let display_name =
            display_name_for_node(db, *id, &view.entity_type, kind.as_str(), name.as_deref(), &attrs, meta);

        kind_by_id.insert(*id, kind.clone());
        node_views.push(VizNode {
            id: *id,
            entity_type: view.entity_type,
            kind,
            type_label,
            plane,
            name,
            display_name,
            attrs,
        });
    }

    // Collect edges among selected nodes.
    let mut edges: Vec<VizEdge> = Vec::new();
    let mut truncated = false;
    for rel_id in 0..db.relations.len() as u32 {
        if edges.len() >= max_edges {
            truncated = true;
            break;
        }
        let Some(rel) = db.relations.get_relation(rel_id) else {
            continue;
        };
        if !selected.contains(&rel.source) || !selected.contains(&rel.target) {
            continue;
        }
        let Some(rel_name) = db.interner.lookup(rel.rel_type) else {
            continue;
        };
        let kind = match (kind_by_id.get(&rel.source), kind_by_id.get(&rel.target)) {
            (Some(a), Some(b)) if a == "meta" && b == "meta" => "meta_relation",
            (Some(a), _) if a == "meta" => "meta_relation",
            (_, Some(b)) if b == "meta" => "meta_relation",
            _ => "relation",
        };
        edges.push(VizEdge {
            source: rel.source,
            target: rel.target,
            label: rel_name,
            confidence: Some(rel.confidence),
            relation_id: Some(rel_id),
            kind: kind.to_string(),
        });
    }

    // Add "virtual" meta-plane edges that are useful for visualization.
    //
    // The underlying meta-plane graph is intentionally minimal (it’s used for
    // indexing and validation, not as a hand-authored graph). For visualization,
    // it’s helpful to connect:
    // - constraint nodes → the relation they talk about
    // - field decl nodes → the object type they point to
    if options.include_meta_plane && edges.len() < max_edges {
        // Map relation name -> relation decl node id (within selected set).
        let mut rel_decl_by_name: HashMap<String, u32> = HashMap::new();
        for n in &node_views {
            if n.entity_type != axiograph_pathdb::axi_meta::META_TYPE_RELATION_DECL {
                continue;
            }
            let Some(name) = n.attrs.get("name").cloned().or_else(|| n.name.clone()) else {
                continue;
            };
            rel_decl_by_name.insert(name, n.id);
        }

        // Map object type name -> object type node id (within selected set).
        let mut obj_decl_by_name: HashMap<String, u32> = HashMap::new();
        for n in &node_views {
            if n.entity_type != axiograph_pathdb::axi_meta::META_TYPE_OBJECT_TYPE {
                continue;
            }
            let Some(name) = n.attrs.get("name").cloned().or_else(|| n.name.clone()) else {
                continue;
            };
            obj_decl_by_name.insert(name, n.id);
        }

        // Constraint → RelationDecl.
        for n in &node_views {
            if edges.len() >= max_edges {
                truncated = true;
                break;
            }
            if n.entity_type != axiograph_pathdb::axi_meta::META_TYPE_CONSTRAINT {
                continue;
            }
            let Some(rel_name) = n.attrs.get(ATTR_CONSTRAINT_RELATION) else {
                continue;
            };
            let Some(&rid) = rel_decl_by_name.get(rel_name) else {
                continue;
            };
            if !selected.contains(&n.id) || !selected.contains(&rid) {
                continue;
            }
            edges.push(VizEdge {
                source: n.id,
                target: rid,
                label: "constraint_on".to_string(),
                confidence: None,
                relation_id: None,
                kind: "meta_virtual".to_string(),
            });
        }

        // FieldDecl → ObjectType for field type.
        for n in &node_views {
            if edges.len() >= max_edges {
                truncated = true;
                break;
            }
            if n.entity_type != axiograph_pathdb::axi_meta::META_TYPE_FIELD_DECL {
                continue;
            }
            let Some(field_ty) = n.attrs.get(ATTR_FIELD_TYPE) else {
                continue;
            };
            let Some(&oid) = obj_decl_by_name.get(field_ty) else {
                continue;
            };
            edges.push(VizEdge {
                source: n.id,
                target: oid,
                label: "field_type".to_string(),
                confidence: None,
                relation_id: None,
                kind: "meta_virtual".to_string(),
            });
        }
    }

    // Add equivalence edges (deduplicated).
    if options.include_equivalences && edges.len() < max_edges {
        let mut seen: HashSet<(u32, u32, u32)> = HashSet::new();
        for (&a, list) in &db.equivalences {
            if !selected.contains(&a) {
                continue;
            }
            for (b, t) in list {
                if !selected.contains(b) {
                    continue;
                }
                let (x, y) = if a <= *b { (a, *b) } else { (*b, a) };
                let key = (x, y, t.raw());
                if !seen.insert(key) {
                    continue;
                }
                if edges.len() >= max_edges {
                    truncated = true;
                    break;
                }
                let type_name = db
                    .interner
                    .lookup(*t)
                    .unwrap_or_else(|| format!("str_{}", t.raw()));
                edges.push(VizEdge {
                    source: x,
                    target: y,
                    label: format!("equiv::{type_name}"),
                    confidence: None,
                    relation_id: None,
                    kind: "equivalence".to_string(),
                });
            }
            if truncated {
                break;
            }
        }
    }

    // Server-friendly context lookup:
    // - context nodes may not be included in the neighborhood graph (edge truncation),
    // - but tuple-like nodes can still carry context membership in the full DB.
    //
    // We compute an explicit membership map for nodes in this view and include the
    // referenced contexts so the UI can render stable names and filter correctly.
    let mut tuple_contexts: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut context_ids: HashSet<u32> = HashSet::new();
    for n in &node_views {
        if !(n.kind == "fact" || n.kind == "morphism" || n.kind == "homotopy") {
            continue;
        }
        let ctxs = db.follow_one(n.id, REL_AXI_FACT_IN_CONTEXT);
        if ctxs.is_empty() {
            continue;
        }
        let mut ids: Vec<u32> = ctxs.iter().collect();
        ids.sort_unstable();
        for id in &ids {
            context_ids.insert(*id);
        }
        tuple_contexts.insert(n.id, ids);
    }

    let mut contexts: Vec<VizContext> = Vec::new();
    for id in context_ids {
        let name = db
            .get_entity(id)
            .and_then(|v| v.attrs.get("name").cloned())
            .unwrap_or_else(|| format!("Context#{id}"));
        contexts.push(VizContext { id, name });
    }
    contexts.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(VizGraph {
        nodes: node_views,
        edges,
        truncated,
        summary: VizSummary {
            focus_ids: options.focus_ids.clone(),
            hops: options.hops,
            max_nodes: options.max_nodes,
            max_edges: options.max_edges,
            include_meta_plane: options.include_meta_plane,
            include_data_plane: options.include_data_plane,
            include_equivalences: options.include_equivalences,
            typed_overlay: options.typed_overlay,
        },
        contexts,
        tuple_contexts,
    })
}

pub fn render_dot(db: &PathDB, g: &VizGraph) -> String {
    fn dot_escape(s: &str) -> String {
        s.replace('\\', "\\\\").replace('"', "\\\"")
    }

    fn friendly_meta_type(ty: &str) -> &str {
        ty.strip_prefix("AxiMeta").unwrap_or(ty)
    }

    fn entity_name(db: &PathDB, id: u32) -> Option<String> {
        let view = db.get_entity(id)?;
        view.attrs.get("name").cloned()
    }

    let mut out = String::new();
    out.push_str("digraph axiograph {\n");
    out.push_str("  rankdir=LR;\n");
    out.push_str("  node [shape=box, fontname=\"Helvetica\"];\n");
    out.push_str("  edge [fontname=\"Helvetica\"];\n\n");

    // Collect a stable id->kind map for styling.
    let mut kind_by_id: HashMap<u32, &str> = HashMap::new();
    for n in &g.nodes {
        kind_by_id.insert(n.id, n.kind.as_str());
    }

    // Build node lines grouped by kind so we can optionally emit cluster "layers"
    // when we are visualizing both planes at once.
    let mut meta_nodes: Vec<String> = Vec::new();
    let mut entity_nodes: Vec<String> = Vec::new();
    let mut morphism_nodes: Vec<String> = Vec::new();
    let mut homotopy_nodes: Vec<String> = Vec::new();
    let mut fact_nodes: Vec<String> = Vec::new();

    for n in &g.nodes {
        let id = format!("n{}", n.id);
        let mut attrs: Vec<String> = Vec::new();

        let label = match n.kind.as_str() {
            "meta" => {
                let name = n.name.as_deref().unwrap_or("");
                if name.is_empty() {
                    format!("{}\\n(id={})", friendly_meta_type(&n.entity_type), n.id)
                } else {
                    format!(
                        "{}\\n{}\\n(id={})",
                        friendly_meta_type(&n.entity_type),
                        name,
                        n.id
                    )
                }
            }
            "morphism" => {
                let rel = n
                    .attrs
                    .get(ATTR_AXI_RELATION)
                    .cloned()
                    .unwrap_or_else(|| n.entity_type.clone());
                let from = db
                    .follow_one(n.id, "from")
                    .iter()
                    .next()
                    .and_then(|id| entity_name(db, id))
                    .unwrap_or_default();
                let to = db
                    .follow_one(n.id, "to")
                    .iter()
                    .next()
                    .and_then(|id| entity_name(db, id))
                    .unwrap_or_default();
                if !from.is_empty() && !to.is_empty() {
                    format!("Morphism\\n{rel}\\n{from} → {to}\\n(id={})", n.id)
                } else {
                    let name = n.name.as_deref().unwrap_or("");
                    if name.is_empty() {
                        format!("Morphism\\n{rel}\\n(id={})", n.id)
                    } else {
                        format!("Morphism\\n{rel}\\n{name}\\n(id={})", n.id)
                    }
                }
            }
            "homotopy" => {
                let rel = n
                    .attrs
                    .get(ATTR_AXI_RELATION)
                    .cloned()
                    .unwrap_or_else(|| n.entity_type.clone());
                let lhs = db
                    .follow_one(n.id, "lhs")
                    .iter()
                    .next()
                    .and_then(|id| entity_name(db, id))
                    .unwrap_or_default();
                let rhs = db
                    .follow_one(n.id, "rhs")
                    .iter()
                    .next()
                    .and_then(|id| entity_name(db, id))
                    .unwrap_or_default();
                if !lhs.is_empty() && !rhs.is_empty() {
                    format!("Homotopy\\n{rel}\\n{lhs} ≃ {rhs}\\n(id={})", n.id)
                } else {
                    let name = n.name.as_deref().unwrap_or("");
                    if name.is_empty() {
                        format!("Homotopy\\n{rel}\\n(id={})", n.id)
                    } else {
                        format!("Homotopy\\n{rel}\\n{name}\\n(id={})", n.id)
                    }
                }
            }
            "fact" => {
                let rel = n
                    .attrs
                    .get(ATTR_AXI_RELATION)
                    .cloned()
                    .unwrap_or_else(|| n.entity_type.clone());
                let from = db
                    .follow_one(n.id, "from")
                    .iter()
                    .next()
                    .and_then(|id| entity_name(db, id))
                    .unwrap_or_default();
                let to = db
                    .follow_one(n.id, "to")
                    .iter()
                    .next()
                    .and_then(|id| entity_name(db, id))
                    .unwrap_or_default();
                if !from.is_empty() && !to.is_empty() {
                    format!("{rel}\\n{from} → {to}\\n(id={})", n.id)
                } else {
                    // Fall back to the (sanitized) fact-node name if present.
                    let name = n.name.as_deref().unwrap_or("");
                    if name.is_empty() {
                        format!("{rel}\\n(id={})", n.id)
                    } else {
                        format!("{rel}\\n{name}\\n(id={})", n.id)
                    }
                }
            }
            _ => {
                let name = n.name.as_deref().unwrap_or("");
                if name.is_empty() {
                    format!("{}\\n(id={})", n.entity_type, n.id)
                } else {
                    format!("{}\\n{}\\n(id={})", n.entity_type, name, n.id)
                }
            }
        };

        if g.summary.typed_overlay {
            let mut tooltip_lines: Vec<String> = Vec::new();
            if let Some(s) = n.attrs.get(ATTR_OVERLAY_SUPERTYPES) {
                tooltip_lines.push(format!("supertypes: {s}"));
            }
            if let Some(sig) = n.attrs.get(ATTR_OVERLAY_RELATION_SIGNATURE) {
                tooltip_lines.push(format!("signature: {sig}"));
            }
            if let Some(c) = n.attrs.get(ATTR_OVERLAY_CONSTRAINTS) {
                tooltip_lines.push(format!("constraints: {c}"));
            }
            if !tooltip_lines.is_empty() {
                attrs.push(format!(
                    "tooltip=\"{}\"",
                    dot_escape(&tooltip_lines.join("\\n"))
                ));
            }
        }

        match n.kind.as_str() {
            "meta" => {
                attrs.push("shape=ellipse".to_string());
                attrs.push("style=filled".to_string());
                attrs.push("fillcolor=\"#f3f3f3\"".to_string());
                attrs.push("color=gray50".to_string());
            }
            "morphism" => {
                attrs.push("shape=box".to_string());
                attrs.push("style=\"rounded,filled\"".to_string());
                attrs.push("fillcolor=\"#c6f6d5\"".to_string());
                attrs.push("color=\"#2f855a\"".to_string());
            }
            "homotopy" => {
                attrs.push("shape=box".to_string());
                attrs.push("style=\"rounded,filled\"".to_string());
                attrs.push("fillcolor=\"#e9d8fd\"".to_string());
                attrs.push("color=\"#6b46c1\"".to_string());
            }
            "fact" => {
                attrs.push("shape=box".to_string());
                attrs.push("style=\"rounded,filled\"".to_string());
                attrs.push("fillcolor=\"#fff3c7\"".to_string());
                attrs.push("color=\"#c8a43a\"".to_string());
            }
            _ => {
                attrs.push("shape=box".to_string());
                attrs.push("style=filled".to_string());
                attrs.push("fillcolor=\"#eaf2ff\"".to_string());
                attrs.push("color=\"#4f6fab\"".to_string());
            }
        }

        attrs.push(format!("label=\"{}\"", dot_escape(&label)));
        let line = format!("    {id} [{}];\n", attrs.join(", "));

        match n.kind.as_str() {
            "meta" => meta_nodes.push(line),
            "morphism" => morphism_nodes.push(line),
            "homotopy" => homotopy_nodes.push(line),
            "fact" => fact_nodes.push(line),
            _ => entity_nodes.push(line),
        }
    }

    let layered = !meta_nodes.is_empty()
        && (!entity_nodes.is_empty()
            || !morphism_nodes.is_empty()
            || !homotopy_nodes.is_empty()
            || !fact_nodes.is_empty());
    if layered {
        out.push_str("  // Layers: meta-plane vs data-plane (entities + fact nodes).\n");
        if !meta_nodes.is_empty() {
            out.push_str("  subgraph cluster_meta {\n");
            out.push_str("    label=\"Meta plane\";\n");
            out.push_str("    style=\"rounded,dashed\";\n");
            out.push_str("    color=\"#bdbdbd\";\n");
            for line in &meta_nodes {
                out.push_str(line);
            }
            out.push_str("  }\n\n");
        }
        if !entity_nodes.is_empty() {
            out.push_str("  subgraph cluster_entities {\n");
            out.push_str("    label=\"Entities\";\n");
            out.push_str("    style=\"rounded\";\n");
            out.push_str("    color=\"#a5c7ff\";\n");
            for line in &entity_nodes {
                out.push_str(line);
            }
            out.push_str("  }\n\n");
        }
        if !morphism_nodes.is_empty() {
            out.push_str("  subgraph cluster_morphisms {\n");
            out.push_str("    label=\"Morphisms\";\n");
            out.push_str("    style=\"rounded\";\n");
            out.push_str("    color=\"#2f855a\";\n");
            for line in &morphism_nodes {
                out.push_str(line);
            }
            out.push_str("  }\n\n");
        }
        if !homotopy_nodes.is_empty() {
            out.push_str("  subgraph cluster_homotopies {\n");
            out.push_str("    label=\"Homotopies (equivalences)\";\n");
            out.push_str("    style=\"rounded\";\n");
            out.push_str("    color=\"#6b46c1\";\n");
            for line in &homotopy_nodes {
                out.push_str(line);
            }
            out.push_str("  }\n\n");
        }
        if !fact_nodes.is_empty() {
            out.push_str("  subgraph cluster_facts {\n");
            out.push_str("    label=\"Fact nodes (reified tuples)\";\n");
            out.push_str("    style=\"rounded\";\n");
            out.push_str("    color=\"#d8b24b\";\n");
            for line in &fact_nodes {
                out.push_str(line);
            }
            out.push_str("  }\n\n");
        }
    } else {
        for line in meta_nodes
            .iter()
            .chain(entity_nodes.iter())
            .chain(morphism_nodes.iter())
            .chain(homotopy_nodes.iter())
            .chain(fact_nodes.iter())
        {
            // Drop the inner-cluster indentation.
            out.push_str(line.replace("    ", "  ").as_str());
        }
        out.push('\n');
    }

    for e in &g.edges {
        let src = format!("n{}", e.source);
        let dst = format!("n{}", e.target);
        let mut attrs: Vec<String> = Vec::new();
        let mut label = e.label.clone();
        if let Some(c) = e.confidence {
            label = format!("{label} ({:.3})", c);
        }
        attrs.push(format!("label=\"{}\"", dot_escape(&label)));
        match e.kind.as_str() {
            "equivalence" => {
                attrs.push("style=dashed".to_string());
                attrs.push("color=gray40".to_string());
            }
            "meta_relation" => {
                attrs.push("style=dotted".to_string());
                attrs.push("color=gray55".to_string());
            }
            "meta_virtual" => {
                attrs.push("style=dashed".to_string());
                attrs.push("color=gray60".to_string());
            }
            _ => {
                // Highlight edges from proof-relevant/witness nodes.
                match kind_by_id.get(&e.source).copied() {
                    Some("fact") => attrs.push("color=\"#8a5a00\"".to_string()),
                    Some("morphism") => attrs.push("color=\"#2f855a\"".to_string()),
                    Some("homotopy") => attrs.push("color=\"#6b46c1\"".to_string()),
                    _ => {}
                }
            }
        };
        out.push_str(&format!("  {src} -> {dst} [{}];\n", attrs.join(", ")));
    }

    if g.truncated {
        out.push_str("\n  // NOTE: graph truncated by max_nodes/max_edges.\n");
    }

    out.push_str("}\n");
    out
}

pub fn render_json(g: &VizGraph) -> Result<String> {
    Ok(serde_json::to_string_pretty(g)?)
}

pub fn render_html(db: &PathDB, g: &VizGraph) -> Result<String> {
    // Render via a template file so the explorer remains readable/maintainable.
    //
    // Note: we escape `</` in the embedded JSON to avoid accidentally closing
    // the `<script>` tag if graph data contains `</script>`.
    let json = serde_json::to_string(g)?.replace("</", "<\\/");

    let template = include_str!("../templates/viz_explorer.html");
    let mut html = template.to_string();
    html = html.replace("{{GRAPH_JSON}}", &json);
    html = html.replace("{{NODES_COUNT}}", &g.nodes.len().to_string());
    html = html.replace("{{EDGES_COUNT}}", &g.edges.len().to_string());
    html = html.replace("{{TRUNCATED}}", &g.truncated.to_string());

    let _ = db; // reserved: future: include a header with snapshot info
    Ok(html)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn db_with_axi_meta_plane() -> axiograph_pathdb::PathDB {
        let mut db = axiograph_pathdb::PathDB::new();
        let axi = r#"
module Demo

schema Demo:
  object Node
  object Supplier
  subtype Supplier < Node

  relation Flow(from: Supplier, to: Supplier)

theory DemoRules on Demo:
  constraint key Flow(from)

instance DemoInst of Demo:
  Supplier = {a, b}
  Flow = {(from=a, to=b)}
"#;
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)
            .expect("import demo axi module");
        db.build_indexes();
        db
    }

    #[test]
    fn viz_typed_overlay_adds_supertypes_and_relation_info() -> Result<()> {
        let db = db_with_axi_meta_plane();

        // Focus a fact node.
        let flow_id = db
            .find_by_type("Flow")
            .and_then(|bm| bm.iter().next())
            .expect("flow tuple");
        let options = VizOptions {
            focus_ids: vec![flow_id],
            hops: 0,
            include_meta_plane: false,
            include_data_plane: true,
            include_equivalences: false,
            typed_overlay: true,
            ..VizOptions::default()
        };
        let g = extract_viz_graph(&db, &options)?;
        let node = g.nodes.iter().find(|n| n.id == flow_id).expect("node");
        assert!(node
            .attrs
            .get(ATTR_OVERLAY_RELATION_SIGNATURE)
            .unwrap_or(&String::new())
            .contains("Flow("));
        assert!(node
            .attrs
            .get(ATTR_OVERLAY_CONSTRAINTS)
            .unwrap_or(&String::new())
            .contains("key(from)"));

        // Focus an object entity and ensure the supertypes closure shows up.
        let supplier_id = db
            .find_by_type("Supplier")
            .and_then(|bm| bm.iter().next())
            .expect("supplier");
        let options = VizOptions {
            focus_ids: vec![supplier_id],
            hops: 0,
            include_meta_plane: false,
            include_data_plane: true,
            include_equivalences: false,
            typed_overlay: true,
            ..VizOptions::default()
        };
        let g = extract_viz_graph(&db, &options)?;
        let node = g.nodes.iter().find(|n| n.id == supplier_id).expect("node");
        assert!(node
            .attrs
            .get(ATTR_OVERLAY_SUPERTYPES)
            .unwrap_or(&String::new())
            .contains("Node"));
        Ok(())
    }
}
