//! Document ingestion for Axiograph
//!
//! Extracts knowledge from:
//! - PDF documents (text, structure, metadata)
//! - Text files (markdown, plain text)
//! - Technical manuals and books
//! - Conversations and transcripts
//! - Confluence wiki pages
//! - Recommended readings
//!
//! Output:
//! - `EvidenceChunkBundleV1` JSON for RAG/vector search
//! - Extracted facts with confidence scores
//! - `proposals.json` (Evidence/Proposals schema) for explicit promotion into canonical `.axi`
//!
//! **Untrusted boundary**: this crate is heavy IO/parsing. Its output stays in
//! the evidence plane until typed review, CQ/trust gates, and promotion lower
//! accepted meaning through canonical `.axi`.

use anyhow::{anyhow, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

pub mod augment;
pub mod confluence;
pub mod conversations;
pub mod discovery_trace;
pub mod evidence;
pub mod fact_extraction;
pub mod pdf;
pub mod promotion;
pub mod proposals;
pub mod readings;
pub mod repo;

pub use augment::*;
pub use confluence::*;
pub use conversations::*;
pub use discovery_trace::*;
pub use evidence::*;
pub use fact_extraction::*;
pub use pdf::{PdfDocument, PdfError, PdfParser};
pub use promotion::*;
pub use proposals::*;
pub use readings::*;
pub use repo::*;

// ============================================================================
// Chunk representation (for RAG)
// ============================================================================

pub const EVIDENCE_CHUNK_BUNDLE_VERSION_V1: &str = "evidence_chunk_bundle_v1";

/// A document chunk with source pointer
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Chunk {
    pub chunk_id: String,
    pub document_id: String,
    pub page: Option<usize>,
    pub span_id: String,
    pub text: String,
    pub bbox: Option<[f64; 4]>, // [x0, y0, x1, y1]
    pub metadata: HashMap<String, String>,
}

/// Document extraction result
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DocumentExtraction {
    pub source_path: String,
    pub document_id: String,
    pub title: Option<String>,
    pub chunks: Vec<Chunk>,
    pub metadata: HashMap<String, String>,
}

/// Typed evidence-plane bundle for retrieved/indexed text.
///
/// This is intentionally not `.axi` truth. It is an evidence overlay input for
/// retrieval, embeddings, proposals, and review workflows.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EvidenceChunkBundleV1 {
    pub version: String,
    pub evidence_plane: String,
    pub source: ProposalSourceV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub chunks: Vec<Chunk>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub caveats: Vec<String>,
}

// ============================================================================
// Text extraction
// ============================================================================

/// Extract chunks from plain text
pub fn extract_text(text: &str, doc_id: &str) -> DocumentExtraction {
    let mut chunks = Vec::new();

    // Split into paragraphs
    let paragraphs: Vec<&str> = text
        .split("\n\n")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    for (i, para) in paragraphs.iter().enumerate() {
        chunks.push(Chunk {
            chunk_id: format!("{}_{}", doc_id, i),
            document_id: doc_id.to_string(),
            page: None,
            span_id: format!("para_{}", i),
            text: para.to_string(),
            bbox: None,
            metadata: HashMap::new(),
        });
    }

    DocumentExtraction {
        source_path: "".to_string(),
        document_id: doc_id.to_string(),
        title: None,
        chunks,
        metadata: HashMap::new(),
    }
}

/// Extract chunks from markdown
pub fn extract_markdown(text: &str, doc_id: &str) -> DocumentExtraction {
    let mut chunks = Vec::new();
    let mut current_section = String::new();
    let mut current_text = String::new();
    let mut chunk_idx = 0;

    for line in text.lines() {
        if line.starts_with('#') {
            // Save previous section
            if !current_text.trim().is_empty() {
                chunks.push(Chunk {
                    chunk_id: format!("{}_{}", doc_id, chunk_idx),
                    document_id: doc_id.to_string(),
                    page: None,
                    span_id: format!("section_{}", chunk_idx),
                    text: current_text.trim().to_string(),
                    bbox: None,
                    metadata: {
                        let mut m = HashMap::new();
                        m.insert("section".to_string(), current_section.clone());
                        m
                    },
                });
                chunk_idx += 1;
            }

            // Start new section
            current_section = line.trim_start_matches('#').trim().to_string();
            current_text.clear();
        } else {
            current_text.push_str(line);
            current_text.push('\n');
        }
    }

    // Save last section
    if !current_text.trim().is_empty() {
        chunks.push(Chunk {
            chunk_id: format!("{}_{}", doc_id, chunk_idx),
            document_id: doc_id.to_string(),
            page: None,
            span_id: format!("section_{}", chunk_idx),
            text: current_text.trim().to_string(),
            bbox: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert("section".to_string(), current_section);
                m
            },
        });
    }

    DocumentExtraction {
        source_path: "".to_string(),
        document_id: doc_id.to_string(),
        title: None,
        chunks,
        metadata: HashMap::new(),
    }
}

// ============================================================================
// PDF extraction (when feature enabled)
// ============================================================================

#[cfg(feature = "pdf")]
pub fn extract_pdf(path: &Path) -> Result<DocumentExtraction> {
    use pdf_extract::extract_text_from_mem;

    let bytes = std::fs::read(path)?;
    let text = extract_text_from_mem(&bytes)?;

    let doc_id = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "doc".to_string());

    let mut extraction = extract_text(&text, &doc_id);
    extraction.source_path = path.to_string_lossy().to_string();

    Ok(extraction)
}

#[cfg(not(feature = "pdf"))]
pub fn extract_pdf(_path: &Path) -> Result<DocumentExtraction> {
    Err(anyhow!("PDF feature not enabled"))
}

pub fn evidence_chunk_bundle_from_extraction(
    extraction: &DocumentExtraction,
) -> EvidenceChunkBundleV1 {
    EvidenceChunkBundleV1 {
        version: EVIDENCE_CHUNK_BUNDLE_VERSION_V1.to_string(),
        evidence_plane: "evidence_overlay".to_string(),
        source: ProposalSourceV1 {
            source_type: "document_extraction".to_string(),
            locator: if extraction.source_path.is_empty() {
                extraction.document_id.clone()
            } else {
                extraction.source_path.clone()
            },
        },
        document_id: Some(extraction.document_id.clone()),
        title: extraction.title.clone(),
        chunks: extraction.chunks.clone(),
        metadata: extraction
            .metadata
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        caveats: vec![
            "EvidenceChunkBundleV1 is an evidence-plane artifact; it is not accepted ontology truth."
                .to_string(),
        ],
    }
}

pub fn evidence_chunk_bundle_from_chunks(
    source_type: impl Into<String>,
    locator: impl Into<String>,
    chunks: Vec<Chunk>,
) -> EvidenceChunkBundleV1 {
    EvidenceChunkBundleV1 {
        version: EVIDENCE_CHUNK_BUNDLE_VERSION_V1.to_string(),
        evidence_plane: "evidence_overlay".to_string(),
        source: ProposalSourceV1 {
            source_type: source_type.into(),
            locator: locator.into(),
        },
        document_id: None,
        title: None,
        chunks,
        metadata: BTreeMap::new(),
        caveats: vec![
            "EvidenceChunkBundleV1 is an evidence-plane artifact; it is not accepted ontology truth."
                .to_string(),
        ],
    }
}

/// Output chunks as typed evidence-bundle JSON.
pub fn chunks_to_json(extraction: &DocumentExtraction) -> Result<String> {
    Ok(serde_json::to_string_pretty(
        &evidence_chunk_bundle_from_extraction(extraction),
    )?)
}

pub fn chunks_to_json_for_chunks(
    source_type: impl Into<String>,
    locator: impl Into<String>,
    chunks: Vec<Chunk>,
) -> Result<String> {
    Ok(serde_json::to_string_pretty(
        &evidence_chunk_bundle_from_chunks(source_type, locator, chunks),
    )?)
}

pub fn chunk_bundle_from_json_str(text: &str) -> Result<EvidenceChunkBundleV1> {
    let bundle: EvidenceChunkBundleV1 = serde_json::from_str(text)?;
    validate_chunk_bundle(&bundle)?;
    Ok(bundle)
}

pub fn chunks_from_json_str(text: &str) -> Result<Vec<Chunk>> {
    Ok(chunk_bundle_from_json_str(text)?.chunks)
}

pub fn chunks_from_json_slice(bytes: &[u8]) -> Result<Vec<Chunk>> {
    let bundle: EvidenceChunkBundleV1 = serde_json::from_slice(bytes)?;
    validate_chunk_bundle(&bundle)?;
    Ok(bundle.chunks)
}

fn validate_chunk_bundle(bundle: &EvidenceChunkBundleV1) -> Result<()> {
    if bundle.version != EVIDENCE_CHUNK_BUNDLE_VERSION_V1 {
        return Err(anyhow!(
            "unsupported evidence chunk bundle version `{}` (expected `{}`)",
            bundle.version,
            EVIDENCE_CHUNK_BUNDLE_VERSION_V1
        ));
    }
    if bundle.evidence_plane != "evidence_overlay" {
        return Err(anyhow!(
            "EvidenceChunkBundleV1 must stay in evidence_overlay plane, got `{}`",
            bundle.evidence_plane
        ));
    }
    Ok(())
}

// ============================================================================
// Machinist knowledge extraction (specialized)
// ============================================================================

/// Extract machining-relevant knowledge from text
/// Looks for: materials, tools, parameters, observations
pub fn extract_machining_knowledge(text: &str, doc_id: &str) -> DocumentExtraction {
    let mut extraction = extract_text(text, doc_id);

    // Tag chunks with machining-relevant metadata
    let material_patterns = [
        "aluminum", "steel", "titanium", "inconel", "brass", "copper",
    ];
    let tool_patterns = ["endmill", "drill", "tap", "reamer", "boring", "face mill"];
    let param_patterns = ["rpm", "sfm", "ipm", "feed", "speed", "depth of cut", "doc"];
    let quality_patterns = ["chatter", "vibration", "finish", "tolerance", "runout"];

    for chunk in &mut extraction.chunks {
        let text_lower = chunk.text.to_lowercase();

        // Detect material mentions
        for mat in &material_patterns {
            if text_lower.contains(mat) {
                chunk
                    .metadata
                    .insert("mentions_material".to_string(), "true".to_string());
                break;
            }
        }

        // Detect tool mentions
        for tool in &tool_patterns {
            if text_lower.contains(tool) {
                chunk
                    .metadata
                    .insert("mentions_tool".to_string(), "true".to_string());
                break;
            }
        }

        // Detect parameter mentions
        for param in &param_patterns {
            if text_lower.contains(param) {
                chunk
                    .metadata
                    .insert("mentions_parameters".to_string(), "true".to_string());
                break;
            }
        }

        // Detect quality/observation mentions
        for qual in &quality_patterns {
            if text_lower.contains(qual) {
                chunk
                    .metadata
                    .insert("mentions_quality".to_string(), "true".to_string());
                break;
            }
        }
    }

    extraction
}

// ============================================================================
// Full knowledge extraction pipeline
// ============================================================================

/// Result of full knowledge extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeExtractionResult {
    pub extraction: DocumentExtraction,
    pub facts: Vec<ExtractedFact>,
    pub domain: String,
}

/// Extract knowledge from text with probabilistic fact extraction
pub fn extract_knowledge_full(text: &str, doc_id: &str, domain: &str) -> KnowledgeExtractionResult {
    let extraction = if domain == "machining" {
        extract_machining_knowledge(text, doc_id)
    } else {
        extract_text(text, doc_id)
    };

    // Extract facts from chunks
    let patterns = machining_patterns();
    let mut all_facts = Vec::new();

    for chunk in &extraction.chunks {
        let facts = extract_facts_from_chunk(chunk, &patterns, Some(domain));
        all_facts.extend(facts);
    }

    // Aggregate and deduplicate
    let facts = aggregate_facts(all_facts);

    KnowledgeExtractionResult {
        extraction,
        facts,
        domain: domain.to_string(),
    }
}

/// Extract knowledge from a conversation
pub fn extract_knowledge_from_conversation(
    text: &str,
    conv_id: &str,
    format: &str,
) -> KnowledgeExtractionResult {
    let conv = match format {
        "slack" => parse_slack_transcript(text, conv_id),
        "meeting" => parse_meeting_transcript(text, conv_id),
        _ => parse_slack_transcript(text, conv_id), // default
    };

    let extraction = conversation_to_extraction(&conv);

    // Extract facts
    let patterns = machining_patterns();
    let mut all_facts = Vec::new();

    for chunk in &extraction.chunks {
        let facts = extract_facts_from_chunk(chunk, &patterns, None);
        all_facts.extend(facts);
    }

    KnowledgeExtractionResult {
        extraction,
        facts: aggregate_facts(all_facts),
        domain: "conversation".to_string(),
    }
}

/// Extract knowledge from Confluence HTML
pub fn extract_knowledge_from_confluence(
    html: &str,
    page_id: &str,
    space: &str,
) -> Result<KnowledgeExtractionResult> {
    let page = parse_confluence_html(html, page_id, space)?;
    let extraction = confluence_to_extraction(&page);

    let patterns = machining_patterns();
    let mut all_facts = Vec::new();

    for chunk in &extraction.chunks {
        let facts = extract_facts_from_chunk(chunk, &patterns, None);
        all_facts.extend(facts);
    }

    Ok(KnowledgeExtractionResult {
        extraction,
        facts: aggregate_facts(all_facts),
        domain: "confluence".to_string(),
    })
}

#[cfg(test)]
mod typed_chunk_bundle_tests {
    use super::*;

    #[test]
    fn chunks_json_is_typed_evidence_bundle() {
        let extraction = extract_text("alpha\n\nbeta", "doc1");
        let json = chunks_to_json(&extraction).expect("serialize chunk bundle");
        let bundle = chunk_bundle_from_json_str(&json).expect("parse typed chunk bundle");
        assert_eq!(bundle.version, EVIDENCE_CHUNK_BUNDLE_VERSION_V1);
        assert_eq!(bundle.evidence_plane, "evidence_overlay");
        assert_eq!(bundle.document_id.as_deref(), Some("doc1"));
        assert_eq!(bundle.chunks.len(), 2);
        assert!(bundle
            .caveats
            .iter()
            .any(|caveat| caveat.contains("not accepted ontology truth")));
    }

    #[test]
    fn bare_chunk_arrays_are_not_the_public_boundary() {
        let err = chunks_from_json_str(r#"[{"chunk_id":"c0"}]"#)
            .expect_err("bare arrays should not parse as EvidenceChunkBundleV1");
        assert!(err.to_string().contains("invalid type"));
    }
}
