//! Protobuf / gRPC ingestion (Buf descriptor sets → proposals).
//!
//! This crate is intentionally **descriptor-driven**:
//!
//! - We call `buf build --as-file-descriptor-set -o <descriptor.json>`
//! - We parse the descriptor set JSON
//! - We emit `proposals.json` (Evidence/Proposals schema) + optional RAG chunks
//!
//! Why JSON?
//!
//! The binary `google.protobuf.FileDescriptorSet` format is easy to decode, but
//! **custom options / annotations** (e.g. `(google.api.http)` or Buf/Acme
//! extensions) are encoded as extensions. In Rust, decoding those extensions
//! requires a reflective/extension-aware stack.
//!
//! Buf’s JSON output, however, renders extension fields explicitly, using keys
//! like:
//!
//! ```json
//! { "[acme.annotations.v1.http]": { "get": "/v1/payments/{payment_id}" } }
//! ```
//!
//! That makes annotation-driven ingestion practical without introducing a heavy
//! runtime dependency.

#![allow(unused_variables, dead_code)]

use anyhow::{anyhow, Result};
use axiograph_ingest_docs::{Chunk, EvidencePointer, ProposalMetaV1, ProposalV1};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};

// =============================================================================
// Public API
// =============================================================================

#[derive(Debug, Clone)]
pub struct ProtoIngestResultV1 {
    pub chunks: Vec<Chunk>,
    pub proposals: Vec<ProposalV1>,
    pub stats: ProtoIngestStatsV1,
}

#[derive(Debug, Default, Clone)]
pub struct ProtoIngestStatsV1 {
    pub files: usize,
    pub packages: usize,
    pub messages: usize,
    pub enums: usize,
    pub services: usize,
    pub rpcs: usize,
    pub fields: usize,
    pub enum_values: usize,
    pub workflows: usize,
    pub relations: usize,
    pub chunks: usize,
}

// =============================================================================
// Semantic entity deduplication (annotation-driven edges)
// =============================================================================

/// A small helper to avoid emitting duplicate semantic value entities.
///
/// Examples:
/// - `bool::true` / `bool::false`
/// - `proto_auth_scope::payments_write`
/// - `proto_tag::payments`
#[derive(Debug, Default)]
struct SemanticEntityCache {
    seen_entity_ids: HashSet<String>,
}

impl SemanticEntityCache {
    fn ensure_entity(
        &mut self,
        proposals: &mut Vec<ProposalV1>,
        schema_hint: &Option<String>,
        evidence_locator: &Option<String>,
        confidence: f64,
        entity_id: String,
        entity_type: &str,
        name: &str,
        attributes: HashMap<String, String>,
        rationale: &str,
    ) -> String {
        if self.seen_entity_ids.insert(entity_id.clone()) {
            proposals.push(entity_proposal(
                schema_hint,
                evidence_locator,
                confidence,
                &entity_id,
                entity_type,
                name,
                attributes,
                None,
                rationale,
            ));
        }
        entity_id
    }

    fn ensure_bool(
        &mut self,
        proposals: &mut Vec<ProposalV1>,
        schema_hint: &Option<String>,
        evidence_locator: &Option<String>,
        value: bool,
    ) -> String {
        let name = if value { "true" } else { "false" };
        self.ensure_entity(
            proposals,
            schema_hint,
            evidence_locator,
            0.98,
            format!("bool::{name}"),
            "Bool",
            name,
            HashMap::new(),
            "Derived from explicit proto annotation (boolean).",
        )
    }
}

/// Ingest a Buf-generated descriptor set JSON into generic proposals.
pub fn ingest_descriptor_set_json(
    text: &str,
    evidence_locator: Option<String>,
    schema_hint: Option<String>,
) -> Result<ProtoIngestResultV1> {
    let set: FileDescriptorSetJson = serde_json::from_str(text)
        .map_err(|e| anyhow!("failed to parse descriptor set JSON: {e}"))?;

    let mut proposals: Vec<ProposalV1> = Vec::new();
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut stats = ProtoIngestStatsV1::default();
    let mut semantic_entities = SemanticEntityCache::default();

    stats.files = set.file.len();

    // -------------------------------------------------------------------------
    // Pass 1: build indexes for type lookup and comments.
    // -------------------------------------------------------------------------
    let mut packages: BTreeMap<String, ()> = BTreeMap::new();
    let mut message_fqns: BTreeMap<String, ()> = BTreeMap::new();
    let mut enum_fqns: BTreeMap<String, ()> = BTreeMap::new();
    let mut package_message_name_to_fqn: HashMap<(String, String), String> = HashMap::new();

    // comment_index[(file_name, path_vec)] = comment
    let mut comment_index: HashMap<(String, Vec<i32>), String> = HashMap::new();

    for file in &set.file {
        let file_name = file.name.clone().unwrap_or_else(|| "<unknown>".to_string());
        let package = file.package.clone().unwrap_or_default();
        packages.insert(package.clone(), ());

        if let Some(sci) = &file.source_code_info {
            for loc in &sci.location {
                let leading = loc.leading_comments.as_deref().unwrap_or("").trim();
                let trailing = loc.trailing_comments.as_deref().unwrap_or("").trim();
                let detached = loc
                    .leading_detached_comments
                    .iter()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>();

                let mut parts: Vec<String> = Vec::new();
                for d in detached {
                    parts.push(d.to_string());
                }
                if !leading.is_empty() {
                    parts.push(leading.to_string());
                }
                if !trailing.is_empty() {
                    parts.push(trailing.to_string());
                }
                let joined = parts.join("\n\n").trim().to_string();
                if !joined.is_empty() {
                    comment_index.insert((file_name.clone(), loc.path.clone()), joined);
                }
            }
        }

        // Top-level messages/enums + nested messages/enums.
        for m in &file.message_type {
            index_message(
                &package,
                m,
                &mut message_fqns,
                &mut enum_fqns,
                &mut package_message_name_to_fqn,
                &mut stats,
                Vec::new(),
            );
        }
        for e in &file.enum_type {
            let fqn = qualify_type_name(&package, &e.name.clone().unwrap_or_default());
            enum_fqns.insert(fqn, ());
            stats.enums += 1;
            stats.enum_values += e.value.len();
        }
    }

    stats.packages = packages.len();

    // -------------------------------------------------------------------------
    // Pass 2: emit proposals (entities + relations) and chunks (doc comments).
    // -------------------------------------------------------------------------
    for file in &set.file {
        let file_name = file.name.clone().unwrap_or_else(|| "<unknown>".to_string());
        let package = file.package.clone().unwrap_or_default();

        // Package entity (shared across files).
        if !package.is_empty() {
            proposals.push(entity_proposal(
                &schema_hint,
                &evidence_locator,
                0.98,
                &format!("proto_package::{package}"),
                "ProtoPackage",
                &package,
                HashMap::new(),
                None,
                "Derived from Buf descriptor set (package).",
            ));
        }

        // File entity.
        proposals.push(entity_proposal(
            &schema_hint,
            &evidence_locator,
            0.98,
            &format!("proto_file::{}", sanitize_id(&file_name)),
            "ProtoFile",
            &file_name,
            HashMap::from([
                ("package".to_string(), package.clone()),
                (
                    "syntax".to_string(),
                    file.syntax.clone().unwrap_or_default(),
                ),
            ]),
            None,
            "Derived from Buf descriptor set (file).",
        ));

        // Link file → package.
        if !package.is_empty() {
            proposals.push(relation_proposal(
                &schema_hint,
                &evidence_locator,
                0.98,
                "proto_file_in_package",
                &format!("proto_file::{}", sanitize_id(&file_name)),
                &format!("proto_package::{package}"),
                HashMap::new(),
                "File declares package.",
            ));
        }

        // Top-level messages + services + enums.
        for (msg_idx, m) in file.message_type.iter().enumerate() {
            emit_message(
                &mut proposals,
                &mut chunks,
                &mut stats,
                &mut semantic_entities,
                &schema_hint,
                &evidence_locator,
                &comment_index,
                &package_message_name_to_fqn,
                &message_fqns,
                &enum_fqns,
                &file_name,
                &package,
                m,
                Vec::new(),
                vec![4, msg_idx as i32],
            )?;
        }

        for (enum_idx, e) in file.enum_type.iter().enumerate() {
            emit_enum(
                &mut proposals,
                &mut chunks,
                &mut stats,
                &schema_hint,
                &evidence_locator,
                &comment_index,
                &file_name,
                &package,
                e,
                vec![5, enum_idx as i32],
            )?;
        }

        for (svc_idx, svc) in file.service.iter().enumerate() {
            emit_service(
                &mut proposals,
                &mut chunks,
                &mut stats,
                &mut semantic_entities,
                &schema_hint,
                &evidence_locator,
                &comment_index,
                &package_message_name_to_fqn,
                &message_fqns,
                &file_name,
                &package,
                svc,
                vec![6, svc_idx as i32],
            )?;
        }
    }

    stats.chunks = chunks.len();

    Ok(ProtoIngestResultV1 {
        chunks,
        proposals,
        stats,
    })
}

// =============================================================================
// Descriptor JSON (subset)
// =============================================================================

#[derive(Debug, Clone, Deserialize)]
struct FileDescriptorSetJson {
    #[serde(default)]
    file: Vec<FileDescriptorProtoJson>,
}

#[derive(Debug, Clone, Deserialize)]
struct FileDescriptorProtoJson {
    name: Option<String>,
    package: Option<String>,
    #[serde(default, rename = "messageType")]
    message_type: Vec<DescriptorProtoJson>,
    #[serde(default, rename = "enumType")]
    enum_type: Vec<EnumDescriptorProtoJson>,
    #[serde(default)]
    service: Vec<ServiceDescriptorProtoJson>,
    #[serde(default, rename = "sourceCodeInfo")]
    source_code_info: Option<SourceCodeInfoJson>,
    syntax: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DescriptorProtoJson {
    name: Option<String>,
    #[serde(default)]
    field: Vec<FieldDescriptorProtoJson>,
    #[serde(default, rename = "nestedType")]
    nested_type: Vec<DescriptorProtoJson>,
    #[serde(default, rename = "enumType")]
    enum_type: Vec<EnumDescriptorProtoJson>,
    #[serde(default, rename = "oneofDecl")]
    oneof_decl: Vec<OneofDescriptorProtoJson>,
    #[serde(default)]
    options: Option<OptionsJson>,
}

#[derive(Debug, Clone, Deserialize)]
struct OneofDescriptorProtoJson {
    name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct FieldDescriptorProtoJson {
    name: Option<String>,
    number: Option<i32>,
    label: Option<String>,
    #[serde(rename = "type")]
    typ: Option<String>,
    #[serde(rename = "typeName")]
    type_name: Option<String>,
    #[serde(rename = "jsonName")]
    json_name: Option<String>,
    #[serde(default)]
    options: Option<OptionsJson>,
    #[serde(rename = "oneofIndex")]
    oneof_index: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
struct EnumDescriptorProtoJson {
    name: Option<String>,
    #[serde(default)]
    value: Vec<EnumValueDescriptorProtoJson>,
    #[serde(default)]
    options: Option<OptionsJson>,
}

#[derive(Debug, Clone, Deserialize)]
struct EnumValueDescriptorProtoJson {
    name: Option<String>,
    number: Option<i32>,
    #[serde(default)]
    options: Option<OptionsJson>,
}

#[derive(Debug, Clone, Deserialize)]
struct ServiceDescriptorProtoJson {
    name: Option<String>,
    #[serde(default)]
    method: Vec<MethodDescriptorProtoJson>,
    #[serde(default)]
    options: Option<OptionsJson>,
}

#[derive(Debug, Clone, Deserialize)]
struct MethodDescriptorProtoJson {
    name: Option<String>,
    #[serde(rename = "inputType")]
    input_type: Option<String>,
    #[serde(rename = "outputType")]
    output_type: Option<String>,
    #[serde(rename = "clientStreaming")]
    client_streaming: Option<bool>,
    #[serde(rename = "serverStreaming")]
    server_streaming: Option<bool>,
    #[serde(default)]
    options: Option<OptionsJson>,
}

#[derive(Debug, Clone, Deserialize)]
struct SourceCodeInfoJson {
    #[serde(default)]
    location: Vec<LocationJson>,
}

#[derive(Debug, Clone, Deserialize)]
struct LocationJson {
    #[serde(default)]
    path: Vec<i32>,
    #[serde(default)]
    span: Vec<i32>,
    #[serde(rename = "leadingComments")]
    leading_comments: Option<String>,
    #[serde(rename = "trailingComments")]
    trailing_comments: Option<String>,
    #[serde(default, rename = "leadingDetachedComments")]
    leading_detached_comments: Vec<String>,
}

type OptionsJson = BTreeMap<String, Value>;

// =============================================================================
// Indexing + emission helpers
// =============================================================================

fn index_message(
    package: &str,
    m: &DescriptorProtoJson,
    message_fqns: &mut BTreeMap<String, ()>,
    enum_fqns: &mut BTreeMap<String, ()>,
    package_message_name_to_fqn: &mut HashMap<(String, String), String>,
    stats: &mut ProtoIngestStatsV1,
    mut prefix: Vec<String>,
) {
    let Some(name) = m.name.clone() else {
        return;
    };
    prefix.push(name.clone());
    let fqn = qualify_nested_type_name(package, &prefix);
    message_fqns.insert(fqn.clone(), ());
    stats.messages += 1;
    package_message_name_to_fqn.insert((package.to_string(), name), fqn.clone());

    stats.fields += m.field.len();
    stats.enums += m.enum_type.len();
    for e in &m.enum_type {
        if let Some(en) = &e.name {
            enum_fqns.insert(
                qualify_nested_type_name(package, &[prefix.clone(), vec![en.clone()]].concat()),
                (),
            );
            stats.enum_values += e.value.len();
        }
    }

    for (i, nested) in m.nested_type.iter().enumerate() {
        index_message(
            package,
            nested,
            message_fqns,
            enum_fqns,
            package_message_name_to_fqn,
            stats,
            prefix.clone(),
        );
    }
}

fn emit_message(
    proposals: &mut Vec<ProposalV1>,
    chunks: &mut Vec<Chunk>,
    stats: &mut ProtoIngestStatsV1,
    semantic_entities: &mut SemanticEntityCache,
    schema_hint: &Option<String>,
    evidence_locator: &Option<String>,
    comment_index: &HashMap<(String, Vec<i32>), String>,
    package_message_name_to_fqn: &HashMap<(String, String), String>,
    message_fqns: &BTreeMap<String, ()>,
    enum_fqns: &BTreeMap<String, ()>,
    file_name: &str,
    package: &str,
    m: &DescriptorProtoJson,
    mut prefix: Vec<String>,
    base_path: Vec<i32>,
) -> Result<()> {
    let Some(name) = m.name.clone() else {
        return Ok(());
    };
    prefix.push(name.clone());
    let fqn = qualify_nested_type_name(package, &prefix);
    let message_id = format!("proto_message::{}", sanitize_id(&fqn));

    let mut attrs = HashMap::new();
    attrs.insert("package".to_string(), package.to_string());
    attrs.insert("file".to_string(), file_name.to_string());
    attrs.insert("fqn".to_string(), fqn.clone());
    if let Some(opts) = &m.options {
        attrs.insert("options_json".to_string(), serde_json::to_string(opts)?);
    }

    let evidence = comment_index
        .get(&(file_name.to_string(), base_path.clone()))
        .map(|comment| {
            let chunk_id = make_chunk_id("proto_doc", file_name, &fqn);
            chunks.push(Chunk {
                chunk_id: chunk_id.clone(),
                document_id: file_name.to_string(),
                page: None,
                span_id: format!("message:{fqn}"),
                text: comment.clone(),
                bbox: None,
                metadata: HashMap::from([
                    ("kind".to_string(), "proto_message".to_string()),
                    ("fqn".to_string(), fqn.clone()),
                ]),
            });
            vec![EvidencePointer {
                chunk_id,
                locator: evidence_locator.clone(),
                span_id: None,
            }]
        })
        .unwrap_or_default();

    proposals.push(ProposalV1::Entity {
        meta: ProposalMetaV1 {
            proposal_id: message_id.clone(),
            confidence: 0.98,
            evidence,
            public_rationale: "Derived from Buf descriptor set (message).".to_string(),
            metadata: HashMap::new(),
            schema_hint: schema_hint.clone(),
        },
        entity_id: message_id.clone(),
        entity_type: "ProtoMessage".to_string(),
        name: fqn.clone(),
        attributes: attrs,
        description: None,
    });

    // file → message
    proposals.push(relation_proposal(
        schema_hint,
        evidence_locator,
        0.98,
        "proto_file_declares_message",
        &format!("proto_file::{}", sanitize_id(file_name)),
        &message_id,
        HashMap::new(),
        "Message declared in file.",
    ));

    // message fields
    for (field_idx, f) in m.field.iter().enumerate() {
        emit_field(
            proposals,
            chunks,
            stats,
            semantic_entities,
            schema_hint,
            evidence_locator,
            comment_index,
            package_message_name_to_fqn,
            message_fqns,
            enum_fqns,
            file_name,
            package,
            &fqn,
            f,
            [base_path.clone(), vec![2, field_idx as i32]].concat(),
            &prefix,
            &m.oneof_decl,
        )?;
    }

    // nested enums
    for (i, e) in m.enum_type.iter().enumerate() {
        emit_enum(
            proposals,
            chunks,
            stats,
            schema_hint,
            evidence_locator,
            comment_index,
            file_name,
            package,
            e,
            [base_path.clone(), vec![4, i as i32]].concat(),
        )?;
    }

    // nested messages
    for (i, nested) in m.nested_type.iter().enumerate() {
        emit_message(
            proposals,
            chunks,
            stats,
            semantic_entities,
            schema_hint,
            evidence_locator,
            comment_index,
            package_message_name_to_fqn,
            message_fqns,
            enum_fqns,
            file_name,
            package,
            nested,
            prefix.clone(),
            [base_path.clone(), vec![3, i as i32]].concat(),
        )?;
    }

    Ok(())
}

fn emit_field(
    proposals: &mut Vec<ProposalV1>,
    chunks: &mut Vec<Chunk>,
    _stats: &mut ProtoIngestStatsV1,
    semantic_entities: &mut SemanticEntityCache,
    schema_hint: &Option<String>,
    evidence_locator: &Option<String>,
    comment_index: &HashMap<(String, Vec<i32>), String>,
    _package_message_name_to_fqn: &HashMap<(String, String), String>,
    message_fqns: &BTreeMap<String, ()>,
    enum_fqns: &BTreeMap<String, ()>,
    file_name: &str,
    package: &str,
    message_fqn: &str,
    f: &FieldDescriptorProtoJson,
    path: Vec<i32>,
    message_prefix: &[String],
    oneofs: &[OneofDescriptorProtoJson],
) -> Result<()> {
    let Some(field_name) = f.name.clone() else {
        return Ok(());
    };
    let field_id = format!(
        "proto_field::{}::{}",
        sanitize_id(message_fqn),
        sanitize_id(&field_name)
    );

    let mut attrs = HashMap::new();
    attrs.insert("package".to_string(), package.to_string());
    attrs.insert("file".to_string(), file_name.to_string());
    attrs.insert("message_fqn".to_string(), message_fqn.to_string());
    attrs.insert("name".to_string(), field_name.clone());
    if let Some(n) = f.number {
        attrs.insert("number".to_string(), n.to_string());
    }
    if let Some(label) = &f.label {
        attrs.insert("label".to_string(), label.clone());
    }
    if let Some(t) = &f.typ {
        attrs.insert("type".to_string(), t.clone());
    }
    if let Some(tn) = &f.type_name {
        attrs.insert("type_name".to_string(), tn.clone());
    }
    if let Some(jn) = &f.json_name {
        attrs.insert("json_name".to_string(), jn.clone());
    }
    if let Some(opts) = &f.options {
        attrs.insert("options_json".to_string(), serde_json::to_string(opts)?);
    }
    if let Some(oneof_idx) = f.oneof_index {
        if let Some(oneof_name) = oneofs.get(oneof_idx as usize).and_then(|o| o.name.clone()) {
            let mut oneof_path = message_prefix.iter().cloned().collect::<Vec<_>>();
            oneof_path.push(format!("oneof:{oneof_name}"));
            attrs.insert("oneof".to_string(), oneof_path.join("."));
        }
    }

    let evidence = comment_index
        .get(&(file_name.to_string(), path.clone()))
        .map(|comment| {
            let span = format!("field:{}.{field_name}", message_fqn);
            let chunk_id = make_chunk_id("proto_doc", file_name, &span);
            chunks.push(Chunk {
                chunk_id: chunk_id.clone(),
                document_id: file_name.to_string(),
                page: None,
                span_id: span.clone(),
                text: comment.clone(),
                bbox: None,
                metadata: HashMap::from([
                    ("kind".to_string(), "proto_field".to_string()),
                    ("message".to_string(), message_fqn.to_string()),
                    ("field".to_string(), field_name.clone()),
                ]),
            });
            vec![EvidencePointer {
                chunk_id,
                locator: evidence_locator.clone(),
                span_id: None,
            }]
        })
        .unwrap_or_default();

    proposals.push(ProposalV1::Entity {
        meta: ProposalMetaV1 {
            proposal_id: field_id.clone(),
            confidence: 0.98,
            evidence,
            public_rationale: "Derived from Buf descriptor set (field).".to_string(),
            metadata: HashMap::new(),
            schema_hint: schema_hint.clone(),
        },
        entity_id: field_id.clone(),
        entity_type: "ProtoField".to_string(),
        name: format!("{message_fqn}.{field_name}"),
        attributes: attrs,
        description: None,
    });

    // Field-level semantics (annotation-driven).
    if let Some(opts) = &f.options {
        if let Some(sem) = extract_field_semantics(opts) {
            if let Some(required) = sem.required {
                let bool_id = semantic_entities.ensure_bool(
                    proposals,
                    schema_hint,
                    evidence_locator,
                    required,
                );
                proposals.push(relation_proposal(
                    schema_hint,
                    evidence_locator,
                    0.98,
                    "proto_field_required",
                    &field_id,
                    &bool_id,
                    HashMap::new(),
                    "Derived from explicit field annotation (required).",
                ));
            }
            if let Some(pii) = sem.pii {
                let bool_id =
                    semantic_entities.ensure_bool(proposals, schema_hint, evidence_locator, pii);
                proposals.push(relation_proposal(
                    schema_hint,
                    evidence_locator,
                    0.98,
                    "proto_field_pii",
                    &field_id,
                    &bool_id,
                    HashMap::new(),
                    "Derived from explicit field annotation (pii).",
                ));
            }
            if let Some(units) = sem.units.as_deref().filter(|s| !s.trim().is_empty()) {
                let unit_id = semantic_entities.ensure_entity(
                    proposals,
                    schema_hint,
                    evidence_locator,
                    0.98,
                    format!("proto_unit::{}", sanitize_id(units)),
                    "ProtoUnit",
                    units,
                    HashMap::new(),
                    "Derived from explicit field annotation (units).",
                );
                proposals.push(relation_proposal(
                    schema_hint,
                    evidence_locator,
                    0.98,
                    "proto_field_units",
                    &field_id,
                    &unit_id,
                    HashMap::new(),
                    "Field units annotation.",
                ));
            }
            if let Some(example) = sem.example.as_deref().filter(|s| !s.trim().is_empty()) {
                let example_id = semantic_entities.ensure_entity(
                    proposals,
                    schema_hint,
                    evidence_locator,
                    0.98,
                    format!("proto_example::{}", sanitize_id(example)),
                    "ProtoExampleValue",
                    example,
                    HashMap::new(),
                    "Derived from explicit field annotation (example).",
                );
                proposals.push(relation_proposal(
                    schema_hint,
                    evidence_locator,
                    0.98,
                    "proto_field_example",
                    &field_id,
                    &example_id,
                    HashMap::new(),
                    "Field example annotation.",
                ));
            }
        }
    }

    // message → field
    proposals.push(relation_proposal(
        schema_hint,
        evidence_locator,
        0.98,
        "proto_message_has_field",
        &format!("proto_message::{}", sanitize_id(message_fqn)),
        &field_id,
        HashMap::new(),
        "Field declared in message.",
    ));

    // field → referenced type (if present in this descriptor set).
    if let Some(type_name) = &f.type_name {
        let cleaned = type_name.trim_start_matches('.');
        if message_fqns.contains_key(cleaned) {
            proposals.push(relation_proposal(
                schema_hint,
                evidence_locator,
                0.98,
                "proto_field_type_message",
                &field_id,
                &format!("proto_message::{}", sanitize_id(cleaned)),
                HashMap::new(),
                "Field references message type.",
            ));
        } else if enum_fqns.contains_key(cleaned) {
            proposals.push(relation_proposal(
                schema_hint,
                evidence_locator,
                0.98,
                "proto_field_type_enum",
                &field_id,
                &format!("proto_enum::{}", sanitize_id(cleaned)),
                HashMap::new(),
                "Field references enum type.",
            ));
        }
    }

    Ok(())
}

fn emit_enum(
    proposals: &mut Vec<ProposalV1>,
    chunks: &mut Vec<Chunk>,
    stats: &mut ProtoIngestStatsV1,
    schema_hint: &Option<String>,
    evidence_locator: &Option<String>,
    comment_index: &HashMap<(String, Vec<i32>), String>,
    file_name: &str,
    package: &str,
    e: &EnumDescriptorProtoJson,
    path: Vec<i32>,
) -> Result<()> {
    let Some(name) = e.name.clone() else {
        return Ok(());
    };
    let fqn = qualify_type_name(package, &name);
    let enum_id = format!("proto_enum::{}", sanitize_id(&fqn));

    let mut attrs = HashMap::new();
    attrs.insert("package".to_string(), package.to_string());
    attrs.insert("file".to_string(), file_name.to_string());
    attrs.insert("fqn".to_string(), fqn.clone());
    if let Some(opts) = &e.options {
        attrs.insert("options_json".to_string(), serde_json::to_string(opts)?);
    }

    let evidence = comment_index
        .get(&(file_name.to_string(), path.clone()))
        .map(|comment| {
            let chunk_id = make_chunk_id("proto_doc", file_name, &fqn);
            chunks.push(Chunk {
                chunk_id: chunk_id.clone(),
                document_id: file_name.to_string(),
                page: None,
                span_id: format!("enum:{fqn}"),
                text: comment.clone(),
                bbox: None,
                metadata: HashMap::from([
                    ("kind".to_string(), "proto_enum".to_string()),
                    ("fqn".to_string(), fqn.clone()),
                ]),
            });
            vec![EvidencePointer {
                chunk_id,
                locator: evidence_locator.clone(),
                span_id: None,
            }]
        })
        .unwrap_or_default();

    proposals.push(ProposalV1::Entity {
        meta: ProposalMetaV1 {
            proposal_id: enum_id.clone(),
            confidence: 0.98,
            evidence,
            public_rationale: "Derived from Buf descriptor set (enum).".to_string(),
            metadata: HashMap::new(),
            schema_hint: schema_hint.clone(),
        },
        entity_id: enum_id.clone(),
        entity_type: "ProtoEnum".to_string(),
        name: fqn.clone(),
        attributes: attrs,
        description: None,
    });

    // file → enum
    proposals.push(relation_proposal(
        schema_hint,
        evidence_locator,
        0.98,
        "proto_file_declares_enum",
        &format!("proto_file::{}", sanitize_id(file_name)),
        &enum_id,
        HashMap::new(),
        "Enum declared in file.",
    ));

    for (value_idx, v) in e.value.iter().enumerate() {
        let Some(vn) = v.name.clone() else { continue };
        let value_id = format!(
            "proto_enum_value::{}::{}",
            sanitize_id(&fqn),
            sanitize_id(&vn)
        );

        let mut vattrs = HashMap::new();
        vattrs.insert("package".to_string(), package.to_string());
        vattrs.insert("file".to_string(), file_name.to_string());
        vattrs.insert("enum_fqn".to_string(), fqn.clone());
        vattrs.insert("name".to_string(), vn.clone());
        if let Some(n) = v.number {
            vattrs.insert("number".to_string(), n.to_string());
        }
        if let Some(opts) = &v.options {
            vattrs.insert("options_json".to_string(), serde_json::to_string(opts)?);
        }

        let ev_path = [path.clone(), vec![2, value_idx as i32]].concat();
        let evidence = comment_index
            .get(&(file_name.to_string(), ev_path))
            .map(|comment| {
                let span = format!("enum_value:{fqn}.{vn}");
                let chunk_id = make_chunk_id("proto_doc", file_name, &span);
                chunks.push(Chunk {
                    chunk_id: chunk_id.clone(),
                    document_id: file_name.to_string(),
                    page: None,
                    span_id: span.clone(),
                    text: comment.clone(),
                    bbox: None,
                    metadata: HashMap::from([
                        ("kind".to_string(), "proto_enum_value".to_string()),
                        ("enum".to_string(), fqn.clone()),
                        ("value".to_string(), vn.clone()),
                    ]),
                });
                vec![EvidencePointer {
                    chunk_id,
                    locator: evidence_locator.clone(),
                    span_id: None,
                }]
            })
            .unwrap_or_default();

        proposals.push(ProposalV1::Entity {
            meta: ProposalMetaV1 {
                proposal_id: value_id.clone(),
                confidence: 0.98,
                evidence,
                public_rationale: "Derived from Buf descriptor set (enum value).".to_string(),
                metadata: HashMap::new(),
                schema_hint: schema_hint.clone(),
            },
            entity_id: value_id.clone(),
            entity_type: "ProtoEnumValue".to_string(),
            name: format!("{fqn}.{vn}"),
            attributes: vattrs,
            description: None,
        });

        proposals.push(relation_proposal(
            schema_hint,
            evidence_locator,
            0.98,
            "proto_enum_has_value",
            &enum_id,
            &value_id,
            HashMap::new(),
            "Enum value declared in enum.",
        ));

        stats.enum_values += 0; // already counted in indexing pass
    }

    Ok(())
}

fn emit_service(
    proposals: &mut Vec<ProposalV1>,
    chunks: &mut Vec<Chunk>,
    stats: &mut ProtoIngestStatsV1,
    semantic_entities: &mut SemanticEntityCache,
    schema_hint: &Option<String>,
    evidence_locator: &Option<String>,
    comment_index: &HashMap<(String, Vec<i32>), String>,
    package_message_name_to_fqn: &HashMap<(String, String), String>,
    message_fqns: &BTreeMap<String, ()>,
    file_name: &str,
    package: &str,
    svc: &ServiceDescriptorProtoJson,
    svc_path: Vec<i32>,
) -> Result<()> {
    let Some(name) = svc.name.clone() else {
        return Ok(());
    };
    let fqn = qualify_type_name(package, &name);
    let service_id = format!("proto_service::{}", sanitize_id(&fqn));

    let mut attrs = HashMap::new();
    attrs.insert("package".to_string(), package.to_string());
    attrs.insert("file".to_string(), file_name.to_string());
    attrs.insert("fqn".to_string(), fqn.clone());
    if let Some(opts) = &svc.options {
        attrs.insert("options_json".to_string(), serde_json::to_string(opts)?);
    }

    let evidence = comment_index
        .get(&(file_name.to_string(), svc_path.clone()))
        .map(|comment| {
            let chunk_id = make_chunk_id("proto_doc", file_name, &fqn);
            chunks.push(Chunk {
                chunk_id: chunk_id.clone(),
                document_id: file_name.to_string(),
                page: None,
                span_id: format!("service:{fqn}"),
                text: comment.clone(),
                bbox: None,
                metadata: HashMap::from([
                    ("kind".to_string(), "proto_service".to_string()),
                    ("fqn".to_string(), fqn.clone()),
                ]),
            });
            vec![EvidencePointer {
                chunk_id,
                locator: evidence_locator.clone(),
                span_id: None,
            }]
        })
        .unwrap_or_default();

    proposals.push(ProposalV1::Entity {
        meta: ProposalMetaV1 {
            proposal_id: service_id.clone(),
            confidence: 0.98,
            evidence,
            public_rationale: "Derived from Buf descriptor set (service).".to_string(),
            metadata: HashMap::new(),
            schema_hint: schema_hint.clone(),
        },
        entity_id: service_id.clone(),
        entity_type: "ProtoService".to_string(),
        name: fqn.clone(),
        attributes: attrs,
        description: None,
    });

    proposals.push(relation_proposal(
        schema_hint,
        evidence_locator,
        0.98,
        "proto_file_declares_service",
        &format!("proto_file::{}", sanitize_id(file_name)),
        &service_id,
        HashMap::new(),
        "Service declared in file.",
    ));

    // Methods/RPCs.
    let mut methods_for_workflows: Vec<MethodForWorkflow> = Vec::new();

    for (method_idx, m) in svc.method.iter().enumerate() {
        let rpc_path = [svc_path.clone(), vec![2, method_idx as i32]].concat();
        let Some(rpc) = emit_method(
            proposals,
            chunks,
            semantic_entities,
            schema_hint,
            evidence_locator,
            comment_index,
            package_message_name_to_fqn,
            message_fqns,
            file_name,
            package,
            &service_id,
            &fqn,
            m,
            rpc_path,
        )?
        else {
            continue;
        };
        stats.rpcs += 1;
        methods_for_workflows.push(rpc);
    }

    // Tacit workflows: group by guessed resource name / type.
    let workflows = build_workflow_groups(&methods_for_workflows);
    for wf in workflows {
        stats.workflows += 1;
        emit_workflow(proposals, schema_hint, evidence_locator, &service_id, &wf)?;
    }

    stats.services += 1;
    Ok(())
}

#[derive(Debug, Clone)]
struct MethodForWorkflow {
    rpc_id: String,
    rpc_fqn: String,
    resource_fqn: Option<String>,
    operation_kind: Option<String>,
}

#[derive(Debug, Clone)]
struct WorkflowGroup {
    workflow_id: String,
    name: String,
    resource_fqn: Option<String>,
    rpc_ids: Vec<String>,
    ordering: Vec<(String, String)>,
}

fn emit_method(
    proposals: &mut Vec<ProposalV1>,
    chunks: &mut Vec<Chunk>,
    semantic_entities: &mut SemanticEntityCache,
    schema_hint: &Option<String>,
    evidence_locator: &Option<String>,
    comment_index: &HashMap<(String, Vec<i32>), String>,
    package_message_name_to_fqn: &HashMap<(String, String), String>,
    message_fqns: &BTreeMap<String, ()>,
    file_name: &str,
    package: &str,
    service_id: &str,
    service_fqn: &str,
    m: &MethodDescriptorProtoJson,
    rpc_path: Vec<i32>,
) -> Result<Option<MethodForWorkflow>> {
    let Some(name) = m.name.clone() else {
        return Ok(None);
    };
    let rpc_fqn = format!("{service_fqn}.{name}");
    let rpc_id = format!("proto_rpc::{}", sanitize_id(&rpc_fqn));

    let input = m
        .input_type
        .clone()
        .unwrap_or_default()
        .trim_start_matches('.')
        .to_string();
    let output = m
        .output_type
        .clone()
        .unwrap_or_default()
        .trim_start_matches('.')
        .to_string();

    let mut attrs = HashMap::new();
    attrs.insert("package".to_string(), package.to_string());
    attrs.insert("file".to_string(), file_name.to_string());
    attrs.insert("service_fqn".to_string(), service_fqn.to_string());
    attrs.insert("rpc_fqn".to_string(), rpc_fqn.clone());
    attrs.insert("name".to_string(), name.clone());
    attrs.insert("input_type".to_string(), input.clone());
    attrs.insert("output_type".to_string(), output.clone());
    attrs.insert(
        "client_streaming".to_string(),
        m.client_streaming.unwrap_or(false).to_string(),
    );
    attrs.insert(
        "server_streaming".to_string(),
        m.server_streaming.unwrap_or(false).to_string(),
    );

    if let Some(opts) = &m.options {
        // Keep the raw options as JSON for downstream reconciliation.
        attrs.insert("options_json".to_string(), serde_json::to_string(opts)?);

        // Extract a common case: HTTP annotations.
        if let Some(binding) = extract_http_binding(opts) {
            attrs.insert("http_method".to_string(), binding.method.clone());
            attrs.insert("http_path".to_string(), binding.path.clone());
            if let Some(body) = binding.body.clone() {
                attrs.insert("http_body".to_string(), body);
            }
        }
    }

    // Heuristic: operation kind + resource.
    let (operation_kind, resource_name_guess) = guess_operation_and_resource(&name);
    if let Some(op) = &operation_kind {
        attrs.insert("operation_kind_guess".to_string(), op.clone());
    }
    if let Some(res) = &resource_name_guess {
        attrs.insert("resource_name_guess".to_string(), res.clone());
    }

    let resource_fqn = resource_name_guess.as_ref().and_then(|res| {
        package_message_name_to_fqn
            .get(&(package.to_string(), res.clone()))
            .cloned()
    });
    if let Some(rf) = &resource_fqn {
        attrs.insert("resource_fqn_guess".to_string(), rf.clone());
    }

    let evidence = comment_index
        .get(&(file_name.to_string(), rpc_path.clone()))
        .map(|comment| {
            let chunk_id = make_chunk_id("proto_doc", file_name, &rpc_fqn);
            chunks.push(Chunk {
                chunk_id: chunk_id.clone(),
                document_id: file_name.to_string(),
                page: None,
                span_id: format!("rpc:{rpc_fqn}"),
                text: comment.clone(),
                bbox: None,
                metadata: HashMap::from([
                    ("kind".to_string(), "proto_rpc".to_string()),
                    ("fqn".to_string(), rpc_fqn.clone()),
                ]),
            });
            vec![EvidencePointer {
                chunk_id,
                locator: evidence_locator.clone(),
                span_id: None,
            }]
        })
        .unwrap_or_default();

    proposals.push(ProposalV1::Entity {
        meta: ProposalMetaV1 {
            proposal_id: rpc_id.clone(),
            confidence: 0.98,
            evidence,
            public_rationale: "Derived from Buf descriptor set (rpc).".to_string(),
            metadata: HashMap::new(),
            schema_hint: schema_hint.clone(),
        },
        entity_id: rpc_id.clone(),
        entity_type: "ProtoRpc".to_string(),
        name: rpc_fqn.clone(),
        attributes: attrs,
        description: None,
    });

    // RPC-level semantics (annotation-driven).
    if let Some(opts) = &m.options {
        if let Some(sem) = extract_rpc_semantics(opts) {
            if let Some(idempotent) = sem.idempotent {
                let bool_id = semantic_entities.ensure_bool(
                    proposals,
                    schema_hint,
                    evidence_locator,
                    idempotent,
                );
                proposals.push(relation_proposal(
                    schema_hint,
                    evidence_locator,
                    0.98,
                    "proto_rpc_idempotent",
                    &rpc_id,
                    &bool_id,
                    HashMap::new(),
                    "Derived from explicit rpc annotation (idempotent).",
                ));
            }

            if let Some(scope) = sem.auth_scope.as_deref().filter(|s| !s.trim().is_empty()) {
                let scope_id = semantic_entities.ensure_entity(
                    proposals,
                    schema_hint,
                    evidence_locator,
                    0.98,
                    format!("proto_auth_scope::{}", sanitize_id(scope)),
                    "ProtoAuthScope",
                    scope,
                    HashMap::new(),
                    "Derived from explicit rpc annotation (auth_scope).",
                );
                proposals.push(relation_proposal(
                    schema_hint,
                    evidence_locator,
                    0.98,
                    "proto_rpc_auth_scope",
                    &rpc_id,
                    &scope_id,
                    HashMap::new(),
                    "RPC auth scope annotation.",
                ));
            }

            if let Some(stability) = sem.stability.as_deref().filter(|s| !s.trim().is_empty()) {
                let stability_id = semantic_entities.ensure_entity(
                    proposals,
                    schema_hint,
                    evidence_locator,
                    0.98,
                    format!("proto_stability::{}", sanitize_id(stability)),
                    "ProtoStability",
                    stability,
                    HashMap::new(),
                    "Derived from explicit rpc annotation (stability).",
                );
                proposals.push(relation_proposal(
                    schema_hint,
                    evidence_locator,
                    0.98,
                    "proto_rpc_stability",
                    &rpc_id,
                    &stability_id,
                    HashMap::new(),
                    "RPC stability annotation.",
                ));
            }

            for tag in sem.tags.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                let tag_id = semantic_entities.ensure_entity(
                    proposals,
                    schema_hint,
                    evidence_locator,
                    0.98,
                    format!("proto_tag::{}", sanitize_id(tag)),
                    "ProtoTag",
                    tag,
                    HashMap::new(),
                    "Derived from explicit rpc annotation (tag).",
                );
                proposals.push(relation_proposal(
                    schema_hint,
                    evidence_locator,
                    0.98,
                    "proto_rpc_has_tag",
                    &rpc_id,
                    &tag_id,
                    HashMap::new(),
                    "RPC tag annotation.",
                ));
            }
        }
    }

    // service → rpc
    proposals.push(relation_proposal(
        schema_hint,
        evidence_locator,
        0.98,
        "proto_service_has_rpc",
        service_id,
        &rpc_id,
        HashMap::new(),
        "RPC declared in service.",
    ));

    // rpc → request/response types (if present in descriptor set).
    if message_fqns.contains_key(&input) {
        proposals.push(relation_proposal(
            schema_hint,
            evidence_locator,
            0.98,
            "proto_rpc_request",
            &rpc_id,
            &format!("proto_message::{}", sanitize_id(&input)),
            HashMap::new(),
            "RPC request message type.",
        ));
    }
    if message_fqns.contains_key(&output) {
        proposals.push(relation_proposal(
            schema_hint,
            evidence_locator,
            0.98,
            "proto_rpc_response",
            &rpc_id,
            &format!("proto_message::{}", sanitize_id(&output)),
            HashMap::new(),
            "RPC response message type.",
        ));
    }

    // rpc → http endpoint entity (annotation-driven).
    if let Some(opts) = &m.options {
        if let Some(binding) = extract_http_binding(opts) {
            let endpoint_key = format!("{} {}", binding.method, binding.path);
            let endpoint_id = format!("http_endpoint::{}", sanitize_id(&endpoint_key));

            proposals.push(entity_proposal(
                schema_hint,
                evidence_locator,
                0.98,
                &endpoint_id,
                "HttpEndpoint",
                &endpoint_key,
                HashMap::from([
                    ("method".to_string(), binding.method.clone()),
                    ("path".to_string(), binding.path.clone()),
                ]),
                None,
                "Derived from proto rpc HTTP annotation.",
            ));

            proposals.push(relation_proposal(
                schema_hint,
                evidence_locator,
                0.98,
                "proto_rpc_http_endpoint",
                &rpc_id,
                &endpoint_id,
                HashMap::new(),
                "HTTP endpoint mapping from proto annotation.",
            ));
        }
    }

    // rpc → resource message (tacit).
    if let Some(rf) = &resource_fqn {
        proposals.push(relation_proposal(
            schema_hint,
            evidence_locator,
            0.65,
            "proto_rpc_resource_guess",
            &rpc_id,
            &format!("proto_message::{}", sanitize_id(rf)),
            HashMap::new(),
            "Heuristic: inferred resource type from rpc name.",
        ));
    }

    Ok(Some(MethodForWorkflow {
        rpc_id,
        rpc_fqn,
        resource_fqn,
        operation_kind,
    }))
}

fn build_workflow_groups(methods: &[MethodForWorkflow]) -> Vec<WorkflowGroup> {
    // Group by resource (if present), else by service-local unknown bucket.
    let mut by_resource: HashMap<String, Vec<MethodForWorkflow>> = HashMap::new();
    for m in methods {
        let key = m
            .resource_fqn
            .clone()
            .unwrap_or_else(|| "<unknown_resource>".to_string());
        by_resource.entry(key).or_default().push(m.clone());
    }

    let mut out = Vec::new();
    for (resource_key, mut group) in by_resource {
        group.sort_by_key(|m| operation_rank(m.operation_kind.as_deref()));

        let workflow_id = format!(
            "api_workflow::{}",
            short_hash(&format!(
                "wf::{resource_key}::{:?}",
                group.iter().map(|g| &g.rpc_id).collect::<Vec<_>>()
            ))
        );

        let resource_fqn = (resource_key != "<unknown_resource>").then_some(resource_key.clone());
        let name = match &resource_fqn {
            Some(r) => format!("Workflow: {r}"),
            None => "Workflow: (unknown resource)".to_string(),
        };

        let rpc_ids = group.iter().map(|m| m.rpc_id.clone()).collect::<Vec<_>>();

        let mut ordering = Vec::new();
        for window in group.windows(2) {
            if let [a, b] = window {
                ordering.push((a.rpc_id.clone(), b.rpc_id.clone()));
            }
        }

        out.push(WorkflowGroup {
            workflow_id,
            name,
            resource_fqn,
            rpc_ids,
            ordering,
        });
    }
    out
}

fn emit_workflow(
    proposals: &mut Vec<ProposalV1>,
    schema_hint: &Option<String>,
    evidence_locator: &Option<String>,
    service_id: &str,
    wf: &WorkflowGroup,
) -> Result<()> {
    let mut attrs = HashMap::new();
    if let Some(r) = &wf.resource_fqn {
        attrs.insert("resource_fqn".to_string(), r.clone());
    }

    proposals.push(entity_proposal(
        schema_hint,
        evidence_locator,
        0.60,
        &wf.workflow_id,
        "ApiWorkflow",
        &wf.name,
        attrs,
        None,
        "Heuristic grouping of RPCs into a workflow (tacit).",
    ));

    proposals.push(relation_proposal(
        schema_hint,
        evidence_locator,
        0.60,
        "proto_service_has_workflow",
        service_id,
        &wf.workflow_id,
        HashMap::new(),
        "Heuristic workflow grouping within service.",
    ));

    for rpc_id in &wf.rpc_ids {
        proposals.push(relation_proposal(
            schema_hint,
            evidence_locator,
            0.60,
            "workflow_includes_rpc",
            &wf.workflow_id,
            rpc_id,
            HashMap::new(),
            "RPC is part of heuristic workflow group.",
        ));
    }

    for (a, b) in &wf.ordering {
        proposals.push(relation_proposal(
            schema_hint,
            evidence_locator,
            0.55,
            "workflow_suggests_order",
            a,
            b,
            HashMap::new(),
            "Heuristic RPC ordering by operation kind.",
        ));
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct HttpBinding {
    method: String,
    path: String,
    body: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct RpcSemantics {
    idempotent: Option<bool>,
    auth_scope: Option<String>,
    stability: Option<String>,
    tags: Vec<String>,
}

fn extract_rpc_semantics(options: &OptionsJson) -> Option<RpcSemantics> {
    for (k, v) in options {
        if !k.starts_with('[') || !k.ends_with(']') {
            continue;
        }
        // Common convention: `[acme.annotations.v1.semantics]`
        if !k.ends_with(".semantics]") {
            continue;
        }

        let obj = v.as_object()?;
        let idempotent = obj.get("idempotent").and_then(|x| x.as_bool());
        let auth_scope = obj
            .get("authScope")
            .or_else(|| obj.get("auth_scope"))
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        let stability = obj
            .get("stability")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());

        let mut tags: Vec<String> = Vec::new();
        if let Some(t) = obj.get("tags") {
            if let Some(arr) = t.as_array() {
                for it in arr {
                    if let Some(s) = it.as_str() {
                        tags.push(s.to_string());
                    }
                }
            } else if let Some(s) = t.as_str() {
                tags.push(s.to_string());
            }
        }

        let out = RpcSemantics {
            idempotent,
            auth_scope,
            stability,
            tags,
        };
        if out.idempotent.is_none()
            && out.auth_scope.is_none()
            && out.stability.is_none()
            && out.tags.is_empty()
        {
            return None;
        }
        return Some(out);
    }
    None
}

#[derive(Debug, Clone, Default)]
struct FieldSemantics {
    required: Option<bool>,
    pii: Option<bool>,
    units: Option<String>,
    example: Option<String>,
}

fn extract_field_semantics(options: &OptionsJson) -> Option<FieldSemantics> {
    for (k, v) in options {
        if !k.starts_with('[') || !k.ends_with(']') {
            continue;
        }
        // Common convention: `[acme.annotations.v1.field]`
        if !k.ends_with(".field]") {
            continue;
        }

        let obj = v.as_object()?;
        let required = obj.get("required").and_then(|x| x.as_bool());
        let pii = obj.get("pii").and_then(|x| x.as_bool());
        let units = obj
            .get("units")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        let example = obj
            .get("example")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());

        let out = FieldSemantics {
            required,
            pii,
            units,
            example,
        };
        if out.required.is_none()
            && out.pii.is_none()
            && out.units.is_none()
            && out.example.is_none()
        {
            return None;
        }
        return Some(out);
    }
    None
}

fn extract_http_binding(options: &OptionsJson) -> Option<HttpBinding> {
    for (k, v) in options {
        if !k.starts_with('[') || !k.ends_with(']') {
            continue;
        }
        // Common conventions:
        // - [google.api.http]
        // - [acme.annotations.v1.http]
        if !k.ends_with(".http]") && k != "[google.api.http]" {
            continue;
        }
        let obj = v.as_object()?;
        let body = obj
            .get("body")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());

        for (method, key) in [
            ("GET", "get"),
            ("POST", "post"),
            ("PUT", "put"),
            ("PATCH", "patch"),
            ("DELETE", "delete"),
        ] {
            if let Some(path) = obj.get(key).and_then(|x| x.as_str()) {
                return Some(HttpBinding {
                    method: method.to_string(),
                    path: path.to_string(),
                    body,
                });
            }
        }
    }
    None
}

fn guess_operation_and_resource(name: &str) -> (Option<String>, Option<String>) {
    for (prefix, op) in [
        ("Get", "get"),
        ("List", "list"),
        ("Create", "create"),
        ("Update", "update"),
        ("Patch", "patch"),
        ("Delete", "delete"),
        ("Upsert", "upsert"),
        ("Search", "search"),
        ("Capture", "capture"),
        ("Refund", "refund"),
    ] {
        if let Some(rest) = name.strip_prefix(prefix) {
            if rest.is_empty() {
                return (Some(op.to_string()), None);
            }
            return (Some(op.to_string()), Some(rest.to_string()));
        }
    }
    (None, None)
}

fn operation_rank(kind: Option<&str>) -> i32 {
    match kind.unwrap_or("") {
        "create" => 10,
        "get" => 20,
        "list" => 21,
        "search" => 22,
        "update" => 30,
        "patch" => 31,
        "upsert" => 32,
        "delete" => 40,
        "capture" => 50,
        "refund" => 60,
        _ => 100,
    }
}

fn qualify_type_name(package: &str, name: &str) -> String {
    if package.is_empty() {
        name.to_string()
    } else {
        format!("{package}.{name}")
    }
}

fn qualify_nested_type_name(package: &str, parts: &[String]) -> String {
    let name = parts.join(".");
    qualify_type_name(package, &name)
}

fn make_chunk_id(prefix: &str, file_name: &str, span: &str) -> String {
    format!(
        "{}::{}::{}",
        prefix,
        sanitize_id(file_name),
        short_hash(span)
    )
}

fn short_hash(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(16);
    for b in digest[..8].iter() {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{:02x}", b);
    }
    out
}

fn sanitize_id(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .take(160)
        .collect()
}

fn entity_proposal(
    schema_hint: &Option<String>,
    evidence_locator: &Option<String>,
    confidence: f64,
    entity_id: &str,
    entity_type: &str,
    name: &str,
    attributes: HashMap<String, String>,
    description: Option<String>,
    rationale: &str,
) -> ProposalV1 {
    ProposalV1::Entity {
        meta: ProposalMetaV1 {
            proposal_id: entity_id.to_string(),
            confidence,
            evidence: Vec::new(),
            public_rationale: rationale.to_string(),
            metadata: HashMap::from([(
                "evidence_locator".to_string(),
                evidence_locator.clone().unwrap_or_default(),
            )]),
            schema_hint: schema_hint.clone(),
        },
        entity_id: entity_id.to_string(),
        entity_type: entity_type.to_string(),
        name: name.to_string(),
        attributes,
        description,
    }
}

fn relation_proposal(
    schema_hint: &Option<String>,
    evidence_locator: &Option<String>,
    confidence: f64,
    rel_type: &str,
    source: &str,
    target: &str,
    attributes: HashMap<String, String>,
    rationale: &str,
) -> ProposalV1 {
    let relation_id = format!(
        "proto_rel::{}::{}",
        sanitize_id(rel_type),
        short_hash(&format!("{rel_type}|{source}|{target}"))
    );
    ProposalV1::Relation {
        meta: ProposalMetaV1 {
            proposal_id: relation_id.clone(),
            confidence,
            evidence: Vec::new(),
            public_rationale: rationale.to_string(),
            metadata: HashMap::from([(
                "evidence_locator".to_string(),
                evidence_locator.clone().unwrap_or_default(),
            )]),
            schema_hint: schema_hint.clone(),
        },
        relation_id,
        rel_type: rel_type.to_string(),
        source: source.to_string(),
        target: target.to_string(),
        attributes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingest_large_api_fixture_extracts_http_annotations_and_workflows() -> Result<()> {
        let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../examples/proto/large_api/descriptor.json");
        let text = std::fs::read_to_string(&fixture_path)?;

        let result = ingest_descriptor_set_json(
            &text,
            Some(fixture_path.to_string_lossy().to_string()),
            Some("proto_api".to_string()),
        )?;

        // A few sanity checks: we should have services/RPCs and some chunks.
        assert!(result.stats.services > 0);
        assert!(result.stats.rpcs > 0);
        assert!(result.chunks.len() > 0);

        let mut saw_http = false;
        let mut saw_workflow = false;

        for p in &result.proposals {
            match p {
                ProposalV1::Entity {
                    entity_type,
                    attributes,
                    ..
                } if entity_type == "ProtoRpc" => {
                    if attributes.get("http_method").is_some()
                        && attributes.get("http_path").is_some()
                    {
                        saw_http = true;
                    }
                }
                ProposalV1::Entity { entity_type, .. } if entity_type == "ApiWorkflow" => {
                    saw_workflow = true;
                }
                _ => {}
            }
        }

        assert!(saw_http, "expected at least one rpc with http annotations");
        assert!(
            saw_workflow,
            "expected at least one heuristic workflow entity"
        );

        Ok(())
    }
}
