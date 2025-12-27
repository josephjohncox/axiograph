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
//! - JSON chunks file for RAG/vector search
//! - Extracted facts with confidence scores
//! - `proposals.json` (Evidence/Proposals schema) for explicit promotion into canonical `.axi`
//!
//! **Untrusted boundary**: this crate is heavy IO/parsing; semantic meaning is defined
//! in Lean and enforced via certificates (Rust computes, Lean verifies).

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

/// A document chunk with source pointer
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentExtraction {
    pub source_path: String,
    pub document_id: String,
    pub title: Option<String>,
    pub chunks: Vec<Chunk>,
    pub metadata: HashMap<String, String>,
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

/// Output chunks as JSON
pub fn chunks_to_json(extraction: &DocumentExtraction) -> Result<String> {
    Ok(serde_json::to_string_pretty(&extraction.chunks)?)
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
