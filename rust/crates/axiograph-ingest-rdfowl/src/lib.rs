//! RDF/OWL ingestion for Axiograph (boundary adapter).
//!
//! This crate sits at the **interop boundary**:
//!
//! - It parses RDF/OWL-shaped inputs (untrusted).
//! - It emits Axiograph ingestion artifacts (structured `proposals.json`).
//! - It does *not* define or extend the trusted kernel semantics (Lean does that).
//!
//! Today this crate uses **Sophia** to parse common RDF serializations:
//! - N-Triples (`.nt`)
//! - Turtle (`.ttl`)
//! - N-Quads (`.nq`)
//! - TriG (`.trig`)
//! - RDF/XML (`.rdf`, `.owl`, `.xml`)
//!
//! Roadmap:
//! - Add SHACL-like validation as a certificate-checked ingestion gate.
//! - Add named-graph / provenance exports (PROV-inspired) as a boundary layer.

pub mod owl;

use anyhow::{anyhow, Result};
use axiograph_dsl::digest::fnv1a64_digest_bytes;
use axiograph_ingest_docs::{EvidencePointer, ProposalMetaV1, ProposalV1};
use sophia::api::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub const RDF_TYPE_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

// ============================================================================
// RDF term model (sufficient for proposals emission)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum RdfNode {
    Iri(String),
    BlankNode(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct RdfLiteral {
    lexical: String,
    datatype: Option<String>,
    language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum RdfObject {
    Node(RdfNode),
    Literal(RdfLiteral),
}

#[derive(Debug, Clone)]
struct RdfStatement {
    index: usize,
    subject: RdfNode,
    predicate_iri: String,
    object: RdfObject,
    graph_name: Option<RdfNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RdfFormatV1 {
    NTriples,
    Turtle,
    NQuads,
    TriG,
    RdfXml,
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
struct RdfIngestSinkError {
    message: String,
}

impl From<anyhow::Error> for RdfIngestSinkError {
    fn from(value: anyhow::Error) -> Self {
        Self {
            message: value.to_string(),
        }
    }
}

// ============================================================================
// Proposals emission
// ============================================================================

fn local_name(iri: &str) -> String {
    iri.rsplit(['#', '/']).next().unwrap_or(iri).to_string()
}

fn sanitize_id_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "_".to_string()
    } else {
        out
    }
}

fn rdf_entity_id(iri: &str) -> String {
    let local = sanitize_id_component(&local_name(iri));
    let digest = fnv1a64_digest_bytes(iri.as_bytes());
    format!("rdf_entity::{local}::{digest}")
}

fn rdf_bnode_entity_id(bnode: &str, evidence_locator: &str) -> String {
    let text = format!("bnode:{evidence_locator}\n{bnode}");
    let digest = fnv1a64_digest_bytes(text.as_bytes());
    format!("rdf_bnode::{digest}")
}

fn rdf_context_id(evidence_locator: &str) -> String {
    let digest = fnv1a64_digest_bytes(evidence_locator.as_bytes());
    format!("rdf_context::{digest}")
}

fn rdf_graph_id(graph_name: &RdfNode, evidence_locator: &str) -> String {
    match graph_name {
        RdfNode::Iri(iri) => {
            let local = sanitize_id_component(&local_name(iri));
            let digest = fnv1a64_digest_bytes(iri.as_bytes());
            format!("rdf_graph::{local}::{digest}")
        }
        RdfNode::BlankNode(bn) => {
            let text = format!("graph_bnode:{evidence_locator}\n{bn}");
            let digest = fnv1a64_digest_bytes(text.as_bytes());
            format!("rdf_graph_bnode::{digest}")
        }
    }
}

fn rdf_entity_id_for_node(node: &RdfNode, evidence_locator: &str) -> String {
    match node {
        RdfNode::Iri(iri) => rdf_entity_id(iri),
        RdfNode::BlankNode(bn) => rdf_bnode_entity_id(bn, evidence_locator),
    }
}

fn rdf_relation_id(statement: &RdfStatement, evidence_locator: &str, context_id: &str) -> String {
    let subject_text = match &statement.subject {
        RdfNode::Iri(iri) => iri.as_str(),
        RdfNode::BlankNode(bn) => bn.as_str(),
    };
    let object_text = match &statement.object {
        RdfObject::Node(node) => match node {
            RdfNode::Iri(iri) => iri.as_str(),
            RdfNode::BlankNode(bn) => bn.as_str(),
        },
        RdfObject::Literal(lit) => lit.lexical.as_str(),
    };
    let text = format!(
        "{evidence_locator}\n{context_id}\n{subject_text}\n{}\n{object_text}\n{}",
        statement.predicate_iri, statement.index
    );
    let digest = fnv1a64_digest_bytes(text.as_bytes());
    format!("rdf_rel::{digest}")
}

fn unescape_rdf_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn parse_term_display(term: &str) -> Result<RdfObject> {
    let s = term.trim();

    if let Some(rest) = s.strip_prefix("<").and_then(|t| t.strip_suffix(">")) {
        return Ok(RdfObject::Node(RdfNode::Iri(rest.to_string())));
    }

    if let Some(rest) = s.strip_prefix("_:") {
        return Ok(RdfObject::Node(RdfNode::BlankNode(rest.to_string())));
    }

    if s.starts_with('"') {
        // Very small literal parser (N-Triples-ish display form).
        let mut end_quote = None;
        let mut prev_was_escape = false;
        for (i, ch) in s.char_indices().skip(1) {
            if ch == '"' && !prev_was_escape {
                end_quote = Some(i);
                break;
            }
            prev_was_escape = ch == '\\' && !prev_was_escape;
            if ch != '\\' {
                prev_was_escape = false;
            }
        }
        let Some(end) = end_quote else {
            return Err(anyhow!("invalid literal term (missing closing quote): {s}"));
        };

        let lexical_raw = &s[1..end];
        let lexical = unescape_rdf_string(lexical_raw);
        let mut rest = s[end + 1..].trim();

        let mut language = None;
        let mut datatype = None;

        if let Some(lang) = rest.strip_prefix('@') {
            language = Some(lang.to_string());
            rest = "";
        } else if let Some(dt) = rest.strip_prefix("^^") {
            let dt = dt.trim();
            if let Some(dt_iri) = dt.strip_prefix("<").and_then(|t| t.strip_suffix(">")) {
                datatype = Some(dt_iri.to_string());
            } else if !dt.is_empty() {
                datatype = Some(dt.to_string());
            }
            rest = "";
        }

        if !rest.is_empty() {
            // Keep best-effort; don't fail ingestion on future extensions.
        }

        return Ok(RdfObject::Literal(RdfLiteral {
            lexical,
            datatype,
            language,
        }));
    }

    Err(anyhow!("unsupported RDF term form: {s}"))
}

fn parse_node_term_display(term: &str) -> Result<RdfNode> {
    match parse_term_display(term)? {
        RdfObject::Node(node) => Ok(node),
        RdfObject::Literal(_) => Err(anyhow!("expected IRI/blank node, got literal: {term}")),
    }
}

fn compact_predicate_name(iri: &str) -> String {
    let local = local_name(iri);
    if local == iri {
        let digest = fnv1a64_digest_bytes(iri.as_bytes());
        format!("iri_{digest}")
    } else {
        local
    }
}

fn push_attr_value(attrs: &mut HashMap<String, String>, key: String, value: String) {
    match attrs.get_mut(&key) {
        Some(existing) => {
            if !existing.is_empty() {
                existing.push('\n');
            }
            existing.push_str(&value);
        }
        None => {
            attrs.insert(key, value);
        }
    }
}

fn parse_rdf_statements_from_bytes_v1(
    bytes: &[u8],
    format: RdfFormatV1,
) -> Result<Vec<RdfStatement>> {
    let cursor = std::io::Cursor::new(bytes);
    let reader = std::io::BufReader::new(cursor);

    match format {
        RdfFormatV1::NTriples => {
            let mut out: Vec<RdfStatement> = Vec::new();
            let mut parser = sophia::turtle::parser::nt::parse_bufread(reader);
            parser
                .try_for_each_triple(|t| -> std::result::Result<(), RdfIngestSinkError> {
                    let subject = parse_node_term_display(&t.s().to_string())
                        .map_err(RdfIngestSinkError::from)?;
                    let predicate_iri = parse_node_term_display(&t.p().to_string())
                        .map_err(RdfIngestSinkError::from)?;
                    let RdfNode::Iri(predicate_iri) = predicate_iri else {
                        return Ok(());
                    };
                    let object =
                        parse_term_display(&t.o().to_string()).map_err(RdfIngestSinkError::from)?;
                    let index = out.len();
                    out.push(RdfStatement {
                        index,
                        subject,
                        predicate_iri,
                        object,
                        graph_name: None,
                    });
                    Ok(())
                })
                .map_err(|e| anyhow!("failed to parse N-Triples: {e}"))?;
            Ok(out)
        }
        RdfFormatV1::Turtle => {
            let mut out: Vec<RdfStatement> = Vec::new();
            let mut parser = sophia::turtle::parser::turtle::parse_bufread(reader);
            parser
                .try_for_each_triple(|t| -> std::result::Result<(), RdfIngestSinkError> {
                    let subject = parse_node_term_display(&t.s().to_string())
                        .map_err(RdfIngestSinkError::from)?;
                    let predicate_iri = parse_node_term_display(&t.p().to_string())
                        .map_err(RdfIngestSinkError::from)?;
                    let RdfNode::Iri(predicate_iri) = predicate_iri else {
                        return Ok(());
                    };
                    let object =
                        parse_term_display(&t.o().to_string()).map_err(RdfIngestSinkError::from)?;
                    let index = out.len();
                    out.push(RdfStatement {
                        index,
                        subject,
                        predicate_iri,
                        object,
                        graph_name: None,
                    });
                    Ok(())
                })
                .map_err(|e| anyhow!("failed to parse Turtle: {e}"))?;
            Ok(out)
        }
        RdfFormatV1::NQuads => {
            let mut out: Vec<RdfStatement> = Vec::new();
            let mut parser = sophia::turtle::parser::nq::parse_bufread(reader);
            parser
                .try_for_each_quad(|q| -> std::result::Result<(), RdfIngestSinkError> {
                    let subject = parse_node_term_display(&q.s().to_string())
                        .map_err(RdfIngestSinkError::from)?;
                    let predicate_iri = parse_node_term_display(&q.p().to_string())
                        .map_err(RdfIngestSinkError::from)?;
                    let RdfNode::Iri(predicate_iri) = predicate_iri else {
                        return Ok(());
                    };
                    let object =
                        parse_term_display(&q.o().to_string()).map_err(RdfIngestSinkError::from)?;
                    let graph_name = q
                        .g()
                        .map(|g| {
                            parse_node_term_display(&g.to_string())
                                .map_err(RdfIngestSinkError::from)
                        })
                        .transpose()?;
                    let index = out.len();
                    out.push(RdfStatement {
                        index,
                        subject,
                        predicate_iri,
                        object,
                        graph_name,
                    });
                    Ok(())
                })
                .map_err(|e| anyhow!("failed to parse N-Quads: {e}"))?;
            Ok(out)
        }
        RdfFormatV1::TriG => {
            let mut out: Vec<RdfStatement> = Vec::new();
            let mut parser = sophia::turtle::parser::trig::parse_bufread(reader);
            parser
                .try_for_each_quad(|q| -> std::result::Result<(), RdfIngestSinkError> {
                    let subject = parse_node_term_display(&q.s().to_string())
                        .map_err(RdfIngestSinkError::from)?;
                    let predicate_iri = parse_node_term_display(&q.p().to_string())
                        .map_err(RdfIngestSinkError::from)?;
                    let RdfNode::Iri(predicate_iri) = predicate_iri else {
                        return Ok(());
                    };
                    let object =
                        parse_term_display(&q.o().to_string()).map_err(RdfIngestSinkError::from)?;
                    let graph_name = q
                        .g()
                        .map(|g| {
                            parse_node_term_display(&g.to_string())
                                .map_err(RdfIngestSinkError::from)
                        })
                        .transpose()?;
                    let index = out.len();
                    out.push(RdfStatement {
                        index,
                        subject,
                        predicate_iri,
                        object,
                        graph_name,
                    });
                    Ok(())
                })
                .map_err(|e| anyhow!("failed to parse TriG: {e}"))?;
            Ok(out)
        }
        RdfFormatV1::RdfXml => {
            let mut out: Vec<RdfStatement> = Vec::new();
            let mut parser = sophia::xml::parser::parse_bufread(reader);
            parser
                .try_for_each_triple(|t| -> std::result::Result<(), RdfIngestSinkError> {
                    let subject = parse_node_term_display(&t.s().to_string())
                        .map_err(RdfIngestSinkError::from)?;
                    let predicate_iri = parse_node_term_display(&t.p().to_string())
                        .map_err(RdfIngestSinkError::from)?;
                    let RdfNode::Iri(predicate_iri) = predicate_iri else {
                        return Ok(());
                    };
                    let object =
                        parse_term_display(&t.o().to_string()).map_err(RdfIngestSinkError::from)?;
                    let index = out.len();
                    out.push(RdfStatement {
                        index,
                        subject,
                        predicate_iri,
                        object,
                        graph_name: None,
                    });
                    Ok(())
                })
                .map_err(|e| anyhow!("failed to parse RDF/XML: {e}"))?;
            Ok(out)
        }
    }
}

/// Convert N-Triples into the generic Evidence/Proposals schema (`ProposalV1`).
///
/// Design choice (MVP):
/// - `rdf:type` triples are mapped to `entity_type`.
/// - triples with literal objects become entity attributes.
/// - triples with IRI objects become `ProposalV1::Relation`.
pub fn proposals_from_ntriples_v1(
    text: &str,
    evidence_locator: Option<String>,
    schema_hint: Option<String>,
) -> Result<Vec<ProposalV1>> {
    proposals_from_rdf_v1(
        text.as_bytes(),
        RdfFormatV1::NTriples,
        evidence_locator,
        schema_hint,
    )
}

pub fn proposals_from_rdf_file_v1(
    path: &Path,
    evidence_locator: Option<String>,
    schema_hint: Option<String>,
) -> Result<Vec<ProposalV1>> {
    let bytes = std::fs::read(path)?;
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    let format = match ext.as_str() {
        "nt" | "ntriples" => RdfFormatV1::NTriples,
        "ttl" | "turtle" => RdfFormatV1::Turtle,
        "nq" | "nquads" => RdfFormatV1::NQuads,
        "trig" => RdfFormatV1::TriG,
        "rdf" | "owl" | "xml" => RdfFormatV1::RdfXml,
        other => return Err(anyhow!("unsupported RDF format: .{other}")),
    };

    proposals_from_rdf_v1(&bytes, format, evidence_locator, schema_hint)
}

pub fn proposals_from_rdf_v1(
    bytes: &[u8],
    format: RdfFormatV1,
    evidence_locator: Option<String>,
    schema_hint: Option<String>,
) -> Result<Vec<ProposalV1>> {
    let evidence_locator = evidence_locator.unwrap_or_else(|| "<memory>".to_string());
    let context_id = rdf_context_id(&evidence_locator);

    let statements = parse_rdf_statements_from_bytes_v1(bytes, format)?;

    // Collect resources, types, attributes and edges.
    let mut resources: HashSet<RdfNode> = HashSet::new();
    let mut graphs: HashSet<RdfNode> = HashSet::new();
    let mut types_by_resource: HashMap<RdfNode, HashSet<String>> = HashMap::new();
    let mut attrs_by_resource: HashMap<RdfNode, HashMap<String, Vec<RdfLiteral>>> = HashMap::new();

    // All edges (including rdf:type) that connect node → node.
    let mut node_edges: Vec<(RdfStatement, RdfNode)> = Vec::new();

    for stmt in &statements {
        resources.insert(stmt.subject.clone());

        if let Some(g) = &stmt.graph_name {
            graphs.insert(g.clone());
        }

        match &stmt.object {
            RdfObject::Node(obj_node) => {
                resources.insert(obj_node.clone());
                node_edges.push((stmt.clone(), obj_node.clone()));

                if stmt.predicate_iri == RDF_TYPE_IRI {
                    if let RdfNode::Iri(ty_iri) = obj_node {
                        types_by_resource
                            .entry(stmt.subject.clone())
                            .or_default()
                            .insert(ty_iri.clone());
                    }
                }
            }
            RdfObject::Literal(lit) => {
                let key = compact_predicate_name(&stmt.predicate_iri);
                attrs_by_resource
                    .entry(stmt.subject.clone())
                    .or_default()
                    .entry(key)
                    .or_default()
                    .push(lit.clone());
            }
        }
    }

    // Deterministic ordering.
    let mut resources: Vec<RdfNode> = resources.into_iter().collect();
    resources.sort();
    let mut graphs: Vec<RdfNode> = graphs.into_iter().collect();
    graphs.sort();

    let mut out: Vec<ProposalV1> = Vec::new();

    // Emit the document-level context (so every statement can be scoped).
    {
        let mut attrs = HashMap::new();
        attrs.insert("kind".to_string(), "rdf_document".to_string());
        attrs.insert("locator".to_string(), evidence_locator.clone());

        let mut metadata = HashMap::new();
        metadata.insert("source".to_string(), "rdf_sophia".to_string());

        out.push(ProposalV1::Entity {
            meta: ProposalMetaV1 {
                proposal_id: context_id.clone(),
                confidence: 1.0,
                evidence: vec![EvidencePointer {
                    chunk_id: format!("rdf_context::{}", sanitize_id_component(&context_id)),
                    locator: Some(evidence_locator.clone()),
                    span_id: None,
                }],
                public_rationale: "RDF document context (used to scope statements).".to_string(),
                metadata,
                schema_hint: schema_hint.clone(),
            },
            entity_id: context_id.clone(),
            entity_type: "Context".to_string(),
            name: "RdfDocumentContext".to_string(),
            attributes: attrs,
            description: None,
        });
    }

    // Emit named graph contexts (if any).
    for g in &graphs {
        let graph_id = rdf_graph_id(g, &evidence_locator);
        let mut attrs = HashMap::new();
        attrs.insert("kind".to_string(), "rdf_named_graph".to_string());
        attrs.insert("document_context".to_string(), context_id.clone());
        match g {
            RdfNode::Iri(iri) => {
                attrs.insert("iri".to_string(), iri.clone());
            }
            RdfNode::BlankNode(bn) => {
                attrs.insert("bnode_id".to_string(), bn.clone());
            }
        }

        let mut metadata = HashMap::new();
        metadata.insert("source".to_string(), "rdf_sophia".to_string());

        out.push(ProposalV1::Entity {
            meta: ProposalMetaV1 {
                proposal_id: graph_id.clone(),
                confidence: 1.0,
                evidence: vec![EvidencePointer {
                    chunk_id: format!("rdf_graph::{}", sanitize_id_component(&graph_id)),
                    locator: Some(evidence_locator.clone()),
                    span_id: None,
                }],
                public_rationale: "RDF named graph (context/world) parsed from dataset."
                    .to_string(),
                metadata,
                schema_hint: schema_hint.clone(),
            },
            entity_id: graph_id.clone(),
            entity_type: "Context".to_string(),
            name: match g {
                RdfNode::Iri(iri) => local_name(iri),
                RdfNode::BlankNode(bn) => format!("_:{bn}"),
            },
            attributes: attrs,
            description: None,
        });
    }

    // Emit resource entities.
    for node in &resources {
        let entity_id = rdf_entity_id_for_node(node, &evidence_locator);

        let mut attributes = HashMap::new();
        match node {
            RdfNode::Iri(iri) => {
                attributes.insert("iri".to_string(), iri.clone());
            }
            RdfNode::BlankNode(bn) => {
                attributes.insert("bnode_id".to_string(), bn.clone());
            }
        }

        if let Some(attr_map) = attrs_by_resource.get(node) {
            for (k, vals) in attr_map {
                for lit in vals {
                    let mut v = lit.lexical.clone();
                    if let Some(lang) = &lit.language {
                        v.push_str(&format!("@{lang}"));
                    }
                    if let Some(dt) = &lit.datatype {
                        v.push_str(&format!("^^{dt}"));
                    }
                    push_attr_value(&mut attributes, k.clone(), v);
                }
            }
        }

        let mut all_types: Vec<String> = types_by_resource
            .get(node)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default();
        all_types.sort();
        if !all_types.is_empty() {
            let type_names: Vec<String> = all_types.iter().map(|t| local_name(t)).collect();
            attributes.insert("rdf_types".to_string(), type_names.join("\n"));
        }

        let entity_type = all_types
            .first()
            .map(|t| local_name(t))
            .unwrap_or_else(|| "RdfResource".to_string());

        let mut metadata = HashMap::new();
        metadata.insert("source".to_string(), "rdf_sophia".to_string());
        metadata.insert(
            "format".to_string(),
            match format {
                RdfFormatV1::NTriples => "ntriples",
                RdfFormatV1::Turtle => "turtle",
                RdfFormatV1::NQuads => "nquads",
                RdfFormatV1::TriG => "trig",
                RdfFormatV1::RdfXml => "rdfxml",
            }
            .to_string(),
        );

        out.push(ProposalV1::Entity {
            meta: ProposalMetaV1 {
                proposal_id: entity_id.clone(),
                confidence: 1.0,
                evidence: vec![EvidencePointer {
                    chunk_id: format!("rdf_resource::{}", sanitize_id_component(&entity_id)),
                    locator: Some(evidence_locator.clone()),
                    span_id: None,
                }],
                public_rationale: "Parsed RDF resource.".to_string(),
                metadata,
                schema_hint: schema_hint.clone(),
            },
            entity_id: entity_id.clone(),
            entity_type,
            name: match node {
                RdfNode::Iri(iri) => local_name(iri),
                RdfNode::BlankNode(bn) => format!("_:{bn}"),
            },
            attributes,
            description: None,
        });
    }

    // Deterministic ordering for relations.
    node_edges.sort_by(|(a, a_obj), (b, b_obj)| {
        (
            a.subject.clone(),
            a.predicate_iri.as_str(),
            a_obj.clone(),
            a.index,
        )
            .cmp(&(
                b.subject.clone(),
                b.predicate_iri.as_str(),
                b_obj.clone(),
                b.index,
            ))
    });

    for (stmt, obj_node) in node_edges {
        let source = rdf_entity_id_for_node(&stmt.subject, &evidence_locator);
        let target = rdf_entity_id_for_node(&obj_node, &evidence_locator);

        let stmt_context_id = stmt
            .graph_name
            .as_ref()
            .map(|g| rdf_graph_id(g, &evidence_locator))
            .unwrap_or_else(|| context_id.clone());

        let relation_id = rdf_relation_id(&stmt, &evidence_locator, &stmt_context_id);

        let mut metadata = HashMap::new();
        metadata.insert("predicate_iri".to_string(), stmt.predicate_iri.clone());
        metadata.insert("index".to_string(), stmt.index.to_string());
        if let Some(g) = &stmt.graph_name {
            metadata.insert(
                "graph".to_string(),
                match g {
                    RdfNode::Iri(iri) => iri.clone(),
                    RdfNode::BlankNode(bn) => format!("_:{bn}"),
                },
            );
        }

        let mut attrs = HashMap::new();
        attrs.insert("context".to_string(), stmt_context_id.clone());

        out.push(ProposalV1::Relation {
            meta: ProposalMetaV1 {
                proposal_id: relation_id.clone(),
                confidence: 1.0,
                evidence: vec![EvidencePointer {
                    chunk_id: format!("rdf_stmt::{}", stmt.index),
                    locator: Some(evidence_locator.clone()),
                    span_id: Some(format!("stmt:{:}", stmt.index)),
                }],
                public_rationale: "Parsed RDF statement.".to_string(),
                metadata,
                schema_hint: schema_hint.clone(),
            },
            relation_id,
            rel_type: compact_predicate_name(&stmt.predicate_iri),
            source,
            target,
            attributes: attrs,
        });
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_NT: &str = r#"
<http://example.org/Steel> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/Metal> .
<http://example.org/Metal> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/Material> .
<http://example.org/Steel> <http://example.org/label> "Steel" .
"#;

    #[test]
    fn emits_entity_and_relation_proposals_from_nt() {
        let proposals =
            proposals_from_ntriples_v1(SAMPLE_NT, Some("file://demo".to_string()), None)
                .expect("proposals");

        let entity_names: HashSet<String> = proposals
            .iter()
            .filter_map(|p| match p {
                ProposalV1::Entity { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        assert!(entity_names.contains("Steel"));
        assert!(entity_names.contains("Metal"));
        assert!(entity_names.contains("Material"));

        let rel_types: Vec<String> = proposals
            .iter()
            .filter_map(|p| match p {
                ProposalV1::Relation { rel_type, .. } => Some(rel_type.clone()),
                _ => None,
            })
            .collect();
        assert!(rel_types.contains(&"subClassOf".to_string()));
    }

    #[test]
    fn parses_turtle_and_emits_context() {
        let turtle = r#"
@prefix ex: <http://example.org/> .
ex:a ex:knows ex:b .
ex:a ex:label "Alice"@en .
"#;

        let proposals = proposals_from_rdf_v1(
            turtle.as_bytes(),
            RdfFormatV1::Turtle,
            Some("file://demo.ttl".to_string()),
            None,
        )
        .expect("turtle proposals");

        assert!(proposals.iter().any(|p| matches!(
            p,
            ProposalV1::Entity {
                entity_type,
                ..
            } if entity_type == "Context"
        )));
    }

    #[test]
    fn ingests_local_shacl_fixture() -> Result<()> {
        use std::path::PathBuf;

        let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let repo_root = crate_dir.join("../../..");
        let fixture_dir = repo_root.join("examples/rdfowl/w3c_shacl_minimal");

        let data = fixture_dir.join("data.ttl");
        let shapes = fixture_dir.join("shapes.ttl");

        let mut proposals = Vec::new();
        proposals.extend(proposals_from_rdf_file_v1(
            &data,
            Some("file://w3c_shacl_minimal/data.ttl".to_string()),
            Some("rdfowl".to_string()),
        )?);
        proposals.extend(proposals_from_rdf_file_v1(
            &shapes,
            Some("file://w3c_shacl_minimal/shapes.ttl".to_string()),
            Some("rdfowl".to_string()),
        )?);

        let entity_names: HashSet<String> = proposals
            .iter()
            .filter_map(|p| match p {
                ProposalV1::Entity { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        assert!(entity_names.contains("Alice"));
        assert!(entity_names.contains("Bob"));
        assert!(entity_names.contains("PersonShape"));

        let rel_types: HashSet<String> = proposals
            .iter()
            .filter_map(|p| match p {
                ProposalV1::Relation { rel_type, .. } => Some(rel_type.clone()),
                _ => None,
            })
            .collect();
        assert!(rel_types.contains("targetClass"));
        assert!(rel_types.contains("path"));
        assert!(rel_types.contains("datatype"));

        Ok(())
    }
}
