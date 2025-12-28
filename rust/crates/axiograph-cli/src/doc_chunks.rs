//! Import chunks (RAG-style evidence snippets) into a PathDB snapshot.
//!
//! Why this exists:
//! - `chunks.json` is a great untrusted artifact for LLM grounding and discovery.
//! - A `.axpd` snapshot is the REPL/query substrate.
//! - Importing chunks as graph nodes enables:
//!   - full-text-ish search over chunk text (`fts` / `contains`)
//!   - linking chunk evidence to typed entities (ProtoRpc, ProtoField, ...)
//!   - visualizing "docs ↔ graph" neighborhoods in the existing viz layer
//!
//! This is an **extension layer**:
//! - It is intentionally not part of the certified query core.
//! - It is best-effort (linking uses heuristic matching on `name`).

use std::collections::{HashMap, HashSet};

use anyhow::Result;

use axiograph_ingest_docs::Chunk;
use axiograph_pathdb::axi_meta::META_ATTR_NAME;
use axiograph_pathdb::axi_meta::META_TYPE_MODULE;
use axiograph_pathdb::PathDB;

#[derive(Debug, Default, Clone)]
pub struct ImportChunksSummary {
    pub chunks_total: usize,
    pub chunks_added: usize,
    pub documents_added: usize,
    pub links_added: usize,
    pub links_missing_target: usize,
}

pub fn import_chunks_into_pathdb(db: &mut PathDB, chunks: &[Chunk]) -> Result<ImportChunksSummary> {
    let mut summary = ImportChunksSummary::default();
    summary.chunks_total = chunks.len();

    // Dedup already-imported chunks (by chunk_id attribute).
    let chunk_id_key_id = db.interner.intern("chunk_id");
    let existing_chunk_ids: HashSet<axiograph_pathdb::StrId> = db
        .find_by_type("DocChunk")
        .into_iter()
        .flat_map(|bm| bm.iter())
        .filter_map(|entity_id| db.entities.get_attr(entity_id, chunk_id_key_id))
        .collect();

    let mut document_by_document_id: HashMap<String, u32> = HashMap::new();
    let mut seen_new_chunk_ids: HashSet<axiograph_pathdb::StrId> = HashSet::new();

    for chunk in chunks {
        let chunk_id_value_id = db.interner.intern(&chunk.chunk_id);
        if existing_chunk_ids.contains(&chunk_id_value_id)
            || !seen_new_chunk_ids.insert(chunk_id_value_id)
        {
            continue;
        }

        // -----------------------------------------------------------------
        // Ensure a Document node exists for this chunk.
        // -----------------------------------------------------------------
        let doc_entity_id = if let Some(&id) = document_by_document_id.get(&chunk.document_id) {
            id
        } else {
            let document_name = crate::schema_discovery::sanitize_axi_ident(&chunk.document_id);
            let mut doc_attrs: Vec<(String, String)> = Vec::new();
            doc_attrs.push((META_ATTR_NAME.to_string(), document_name));
            doc_attrs.push(("document_id".to_string(), chunk.document_id.clone()));
            // Extension-layer: index document IDs in a single `search_text` field so
            // `fts(...)` can find documents by path / filename / etc.
            doc_attrs.push(("search_text".to_string(), chunk.document_id.clone()));

            let attrs_ref: Vec<(&str, &str)> = doc_attrs
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();

            let id = db.add_entity("Document", attrs_ref);
            document_by_document_id.insert(chunk.document_id.clone(), id);
            summary.documents_added += 1;
            id
        };

        // -----------------------------------------------------------------
        // Create the DocChunk node.
        // -----------------------------------------------------------------
        let chunk_name = crate::schema_discovery::sanitize_axi_ident(&chunk.chunk_id);

        let mut attrs: Vec<(String, String)> = Vec::new();
        attrs.push((META_ATTR_NAME.to_string(), chunk_name));
        attrs.push(("chunk_id".to_string(), chunk.chunk_id.clone()));
        attrs.push(("document_id".to_string(), chunk.document_id.clone()));
        attrs.push(("span_id".to_string(), chunk.span_id.clone()));
        attrs.push(("text".to_string(), chunk.text.clone()));
        // Extension-layer: aggregate semantic metadata + identifiers into one
        // search field, so `fts`/LLM grounding can match on:
        // - doc IDs / span IDs / chunk IDs
        // - proto element FQNs and "kind" metadata
        // - names/identifiers that may not appear verbatim in the chunk text
        // Note: we do *not* duplicate `text` into `search_text` to avoid
        // doubling large chunk bodies in the string interner. Query text via
        // `fts(..., "text", ...)`, and query metadata via
        // `fts(..., "search_text", ...)`. LLM grounding uses both.
        attrs.push(("search_text".to_string(), build_chunk_search_text(chunk)));
        if let Some(page) = chunk.page {
            attrs.push(("page".to_string(), page.to_string()));
        }
        if let Some(bbox) = chunk.bbox {
            attrs.push((
                "bbox".to_string(),
                format!("{},{},{},{}", bbox[0], bbox[1], bbox[2], bbox[3]),
            ));
        }
        for (k, v) in &chunk.metadata {
            // Avoid collisions with reserved-ish names like `name`, `text`, ...
            attrs.push((format!("meta_{k}"), v.clone()));
        }

        let attrs_ref: Vec<(&str, &str)> = attrs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let chunk_entity_id = db.add_entity("DocChunk", attrs_ref);
        summary.chunks_added += 1;

        // doc ↔ chunk link (helps viz and query navigation).
        db.add_relation(
            "document_has_chunk",
            doc_entity_id,
            chunk_entity_id,
            0.98,
            vec![],
        );
        db.add_relation(
            "chunk_in_document",
            chunk_entity_id,
            doc_entity_id,
            0.98,
            vec![],
        );

        // -----------------------------------------------------------------
        // Best-effort semantic link: chunk ↔ referenced entity.
        // -----------------------------------------------------------------
        if let Some((expected_type, target_name)) = chunk_target(chunk) {
            if let Some(target_id) = find_entity_by_name_and_type(db, &target_name, &expected_type)
            {
                db.add_relation("doc_chunk_about", chunk_entity_id, target_id, 0.98, vec![]);
                db.add_relation("has_doc_chunk", target_id, chunk_entity_id, 0.98, vec![]);
                summary.links_added += 2;
            } else {
                summary.links_missing_target += 1;
            }
        }
    }

    Ok(summary)
}

/// Build a DocChunk that embeds the canonical `.axi` module text as untrusted evidence.
///
/// Motivation:
/// - "Grounding always has evidence": even when the only available source is the
///   canonical `.axi` itself, the snapshot should contain at least one DocChunk
///   so the LLM/UI can cite and open it.
/// - We link the chunk to the meta-plane `AxiMetaModule` node (when present) so
///   the viz UI can navigate between "meaning plane" and "evidence plane".
pub fn chunk_from_axi_module_text(module_name: &str, module_digest: &str, text: &str) -> Chunk {
    let module_name = module_name.trim();
    let module_digest = module_digest.trim();

    // Keep identifiers stable and reasonably URL-friendly.
    let module_id = crate::schema_discovery::sanitize_axi_ident(module_name);
    let digest_id = module_digest.replace(':', "_");

    let mut metadata: HashMap<String, String> = HashMap::new();
    metadata.insert("kind".to_string(), "axi_module".to_string());
    metadata.insert("axi_module".to_string(), module_name.to_string());
    if !module_digest.is_empty() {
        metadata.insert("axi_digest_v1".to_string(), module_digest.to_string());
    }
    metadata.insert("about_type".to_string(), META_TYPE_MODULE.to_string());
    metadata.insert("about_name".to_string(), module_name.to_string());

    Chunk {
        chunk_id: format!("axi_module_{module_id}_{digest_id}"),
        document_id: format!("axi_module:{module_id}"),
        page: None,
        span_id: "axi_module_text".to_string(),
        text: text.to_string(),
        bbox: None,
        metadata,
    }
}

fn build_chunk_search_text(chunk: &Chunk) -> String {
    // Keep this deterministic and human-readable (it’s also useful for debugging):
    // newline-separated fields that tokenize well.
    let mut out = String::new();

    push_search_text_part(&mut out, &chunk.chunk_id);
    push_search_text_part(&mut out, &chunk.document_id);
    push_search_text_part(&mut out, &chunk.span_id);

    // Include metadata values (and their keys for a small amount of structure).
    // Example: `kind=proto_rpc`, `fqn=acme.payments.v1.PaymentService.GetPayment`.
    let mut keys: Vec<&String> = chunk.metadata.keys().collect();
    keys.sort();
    for k in keys {
        if let Some(v) = chunk.metadata.get(k) {
            push_search_text_part(&mut out, k);
            push_search_text_part(&mut out, v);
        }
    }

    // Include a sanitized `.axi` identifier for the primary target (when we can
    // derive one). This helps when the question uses the snapshot `name(...)`
    // form (underscored) rather than the raw proto FQN (dotted).
    if let Some((_, target_name)) = chunk_target(chunk) {
        push_search_text_part(&mut out, &target_name);
    }

    out
}

fn push_search_text_part(out: &mut String, part: &str) {
    let part = part.trim();
    if part.is_empty() {
        return;
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(part);
}

fn chunk_target(chunk: &Chunk) -> Option<(String, String)> {
    // Generic extension hook: allow any importer (including synthetic scenario
    // generators) to declare a canonical "about" target without baking type
    // heuristics into the loader.
    //
    // Example:
    //   metadata: { "about_type": "Person", "about_name": "Alice_0" }
    if let (Some(about_type), Some(about_name)) = (
        chunk.metadata.get("about_type"),
        chunk.metadata.get("about_name"),
    ) {
        let about_type = about_type.trim();
        let about_name = about_name.trim();
        if !about_type.is_empty() && !about_name.is_empty() {
            return Some((about_type.to_string(), about_name.to_string()));
        }
    }

    let kind = chunk.metadata.get("kind")?.to_ascii_lowercase();
    let expected_type = match kind.as_str() {
        "proto_message" => "ProtoMessage",
        "proto_field" => "ProtoField",
        "proto_service" => "ProtoService",
        "proto_rpc" => "ProtoRpc",
        "proto_enum" => "ProtoEnum",
        "proto_enum_value" => "ProtoEnumValue",
        _ => return None,
    }
    .to_string();

    let fqn = if let Some(fqn) = chunk.metadata.get("fqn") {
        fqn.to_string()
    } else if let (Some(message), Some(field)) =
        (chunk.metadata.get("message"), chunk.metadata.get("field"))
    {
        format!("{message}.{field}")
    } else if let (Some(en), Some(v)) = (chunk.metadata.get("enum"), chunk.metadata.get("value")) {
        format!("{en}.{v}")
    } else {
        return None;
    };

    // PathDB stores canonical `.axi` element names as `attr(name)`, so we link
    // by sanitizing the raw proto FQN into an `.axi` identifier.
    let target_name = crate::schema_discovery::sanitize_axi_ident(&fqn);
    Some((expected_type, target_name))
}

fn find_entity_by_name_and_type(db: &PathDB, name: &str, type_name: &str) -> Option<u32> {
    let name_key_id = db.interner.id_of(META_ATTR_NAME)?;
    let name_value_id = db.interner.id_of(name)?;
    let expected_type_id = db.interner.id_of(type_name)?;

    let candidates = db
        .entities
        .entities_with_attr_value(name_key_id, name_value_id);
    for entity_id in candidates.iter() {
        if db.entities.get_type(entity_id) == Some(expected_type_id) {
            return Some(entity_id);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn chunk_import_adds_search_text_with_metadata() {
        let mut db = PathDB::new();

        let mut metadata = HashMap::new();
        metadata.insert("kind".to_string(), "proto_rpc".to_string());
        metadata.insert(
            "fqn".to_string(),
            "acme.svc0.v1.Service0.GetWidget".to_string(),
        );

        let chunk = Chunk {
            chunk_id: "chunk_0".to_string(),
            document_id: "doc_0.proto".to_string(),
            page: None,
            span_id: "span_0".to_string(),
            text: "Returns a widget.".to_string(),
            bbox: None,
            metadata,
        };

        let summary = import_chunks_into_pathdb(&mut db, &[chunk]).expect("import chunks");
        assert_eq!(summary.chunks_added, 1);
        assert_eq!(summary.documents_added, 1);

        // We should be able to find the chunk by searching on the FQN metadata,
        // even if the raw chunk text doesn't mention it.
        let hits = db.entities_with_attr_fts("search_text", "Service0.GetWidget");
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn chunk_import_links_generic_about_target() {
        let mut db = PathDB::new();
        let alice = db.add_entity("Person", vec![("name", "Alice_0")]);

        let chunk = Chunk {
            chunk_id: "chunk_about_alice".to_string(),
            document_id: "doc_synthetic.md".to_string(),
            page: None,
            span_id: "s0".to_string(),
            text: "Alice is a synthetic person in this scenario.".to_string(),
            bbox: None,
            metadata: HashMap::from([
                ("kind".to_string(), "demo_note".to_string()),
                ("about_type".to_string(), "Person".to_string()),
                ("about_name".to_string(), "Alice_0".to_string()),
            ]),
        };

        let _summary = import_chunks_into_pathdb(&mut db, &[chunk]).expect("import chunks");
        db.build_indexes();

        // Find the DocChunk id we just imported, then confirm it links to Alice.
        let Some(doc_chunks) = db.find_by_type("DocChunk") else {
            panic!("expected DocChunk type");
        };
        let chunk_id = doc_chunks.iter().next().expect("at least one DocChunk");

        let about = db.follow_path(chunk_id, &["doc_chunk_about"]);
        assert!(
            about.contains(alice),
            "DocChunk should link to target entity via doc_chunk_about"
        );
    }
}
