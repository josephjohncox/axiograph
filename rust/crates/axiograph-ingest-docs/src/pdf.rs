//! PDF Extraction Module
//!
//! Extracts text, metadata, and structure from PDF documents
//! for knowledge graph ingestion.

#![allow(unused_variables, dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

// ============================================================================
// PDF Document Types
// ============================================================================

/// Extracted PDF content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfDocument {
    pub metadata: PdfMetadata,
    pub pages: Vec<PdfPage>,
    pub outline: Vec<OutlineItem>,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PdfMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Vec<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub creation_date: Option<String>,
    pub modification_date: Option<String>,
    pub page_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfPage {
    pub number: usize,
    pub text: String,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlineItem {
    pub title: String,
    pub level: usize,
    pub page: Option<usize>,
    pub children: Vec<OutlineItem>,
}

// ============================================================================
// PDF Parser
// ============================================================================

/// PDF parser using pdf-extract
pub struct PdfParser {
    extract_images: bool,
    chunk_size: usize,
}

impl Default for PdfParser {
    fn default() -> Self {
        Self {
            extract_images: false,
            chunk_size: 1000,
        }
    }
}

impl PdfParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_images(mut self, extract: bool) -> Self {
        self.extract_images = extract;
        self
    }

    pub fn with_chunk_size(mut self, size: usize) -> Self {
        self.chunk_size = size;
        self
    }

    /// Parse PDF from file path
    #[cfg(feature = "pdf")]
    pub fn parse_file(&self, path: &Path) -> Result<PdfDocument, PdfError> {
        use pdf_extract::extract_text;

        let text = extract_text(path).map_err(|e| PdfError::ExtractionFailed(e.to_string()))?;

        // Basic metadata extraction
        let metadata = self.extract_metadata(path)?;

        // Split into pages (heuristic based on form feeds)
        let pages = self.split_into_pages(&text);

        Ok(PdfDocument {
            metadata,
            pages,
            outline: Vec::new(),
            text,
        })
    }

    /// Parse PDF from bytes
    #[cfg(feature = "pdf")]
    pub fn parse_bytes(&self, data: &[u8]) -> Result<PdfDocument, PdfError> {
        use pdf_extract::extract_text_from_mem;

        let text =
            extract_text_from_mem(data).map_err(|e| PdfError::ExtractionFailed(e.to_string()))?;

        let pages = self.split_into_pages(&text);

        Ok(PdfDocument {
            metadata: PdfMetadata::default(),
            pages,
            outline: Vec::new(),
            text,
        })
    }

    /// Fallback when pdf feature not enabled
    #[cfg(not(feature = "pdf"))]
    pub fn parse_file(&self, path: &Path) -> Result<PdfDocument, PdfError> {
        Err(PdfError::FeatureNotEnabled)
    }

    #[cfg(not(feature = "pdf"))]
    pub fn parse_bytes(&self, _data: &[u8]) -> Result<PdfDocument, PdfError> {
        Err(PdfError::FeatureNotEnabled)
    }

    fn extract_metadata(&self, path: &Path) -> Result<PdfMetadata, PdfError> {
        // Would use lopdf or pdf crate for full metadata
        // Simplified version
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());

        Ok(PdfMetadata {
            title: filename,
            ..Default::default()
        })
    }

    fn split_into_pages(&self, text: &str) -> Vec<PdfPage> {
        // Simple heuristic: split on form feed or large whitespace gaps
        let mut pages = Vec::new();
        let mut current_page = String::new();
        let mut page_num = 1;

        for line in text.lines() {
            if line.contains('\x0C') || (current_page.len() > 3000 && line.trim().is_empty()) {
                if !current_page.trim().is_empty() {
                    pages.push(PdfPage {
                        number: page_num,
                        text: current_page.clone(),
                        width: 612.0, // Letter size default
                        height: 792.0,
                    });
                    page_num += 1;
                    current_page.clear();
                }
            } else {
                current_page.push_str(line);
                current_page.push('\n');
            }
        }

        if !current_page.trim().is_empty() {
            pages.push(PdfPage {
                number: page_num,
                text: current_page,
                width: 612.0,
                height: 792.0,
            });
        }

        pages
    }

    /// Create chunks for RAG
    pub fn create_chunks(&self, doc: &PdfDocument) -> Vec<TextChunk> {
        let mut chunks = Vec::new();
        let mut current_chunk = String::new();
        let mut chunk_start = 0;

        for (i, word) in doc.text.split_whitespace().enumerate() {
            current_chunk.push_str(word);
            current_chunk.push(' ');

            if current_chunk.len() >= self.chunk_size {
                chunks.push(TextChunk {
                    id: chunks.len(),
                    text: current_chunk.trim().to_string(),
                    start_offset: chunk_start,
                    end_offset: chunk_start + current_chunk.len(),
                    page: None,
                    metadata: HashMap::new(),
                });
                chunk_start += current_chunk.len();
                current_chunk.clear();
            }
        }

        if !current_chunk.trim().is_empty() {
            chunks.push(TextChunk {
                id: chunks.len(),
                text: current_chunk.trim().to_string(),
                start_offset: chunk_start,
                end_offset: chunk_start + current_chunk.len(),
                page: None,
                metadata: HashMap::new(),
            });
        }

        chunks
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextChunk {
    pub id: usize,
    pub text: String,
    pub start_offset: usize,
    pub end_offset: usize,
    pub page: Option<usize>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("PDF extraction failed: {0}")]
    ExtractionFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("PDF feature not enabled. Compile with --features pdf")]
    FeatureNotEnabled,
}

// ============================================================================
// Knowledge Extraction from PDF
// ============================================================================

/// Extract knowledge facts from PDF
pub fn extract_facts_from_pdf(doc: &PdfDocument, domain: &str) -> Vec<ExtractedFact> {
    let mut facts = Vec::new();

    // Extract title as entity
    if let Some(title) = &doc.metadata.title {
        facts.push(ExtractedFact {
            fact_type: "Document".to_string(),
            content: title.clone(),
            confidence: 0.95,
            source: "pdf_metadata".to_string(),
            attributes: HashMap::new(),
        });
    }

    // Extract author as entity
    if let Some(author) = &doc.metadata.author {
        facts.push(ExtractedFact {
            fact_type: "Author".to_string(),
            content: author.clone(),
            confidence: 0.95,
            source: "pdf_metadata".to_string(),
            attributes: HashMap::new(),
        });
    }

    // Extract section headers from outline
    for item in &doc.outline {
        extract_outline_facts(&mut facts, item, domain);
    }

    // Pattern-based extraction from text
    extract_pattern_facts(&mut facts, &doc.text, domain);

    facts
}

fn extract_outline_facts(facts: &mut Vec<ExtractedFact>, item: &OutlineItem, domain: &str) {
    facts.push(ExtractedFact {
        fact_type: "Section".to_string(),
        content: item.title.clone(),
        confidence: 0.9,
        source: "pdf_outline".to_string(),
        attributes: [("level".to_string(), item.level.to_string())]
            .into_iter()
            .collect(),
    });

    for child in &item.children {
        extract_outline_facts(facts, child, domain);
    }
}

fn extract_pattern_facts(facts: &mut Vec<ExtractedFact>, text: &str, domain: &str) {
    // Domain-specific patterns
    match domain {
        "machining" => extract_machining_facts(facts, text),
        "materials" => extract_materials_facts(facts, text),
        _ => {}
    }
}

fn extract_machining_facts(facts: &mut Vec<ExtractedFact>, text: &str) {
    // Speed patterns: "XXX SFM" or "XXX m/min"
    let speed_re = regex::Regex::new(r"(\d+(?:\.\d+)?)\s*(?:SFM|sfm|m/min)").unwrap();
    for cap in speed_re.captures_iter(text) {
        facts.push(ExtractedFact {
            fact_type: "CuttingSpeed".to_string(),
            content: cap[1].to_string(),
            confidence: 0.8,
            source: "text_pattern".to_string(),
            attributes: [("unit".to_string(), "SFM".to_string())]
                .into_iter()
                .collect(),
        });
    }

    // Feed patterns: "0.XXX ipr" or "X.X mm/rev"
    let feed_re = regex::Regex::new(r"(\d+(?:\.\d+)?)\s*(?:ipr|IPR|mm/rev)").unwrap();
    for cap in feed_re.captures_iter(text) {
        facts.push(ExtractedFact {
            fact_type: "FeedRate".to_string(),
            content: cap[1].to_string(),
            confidence: 0.8,
            source: "text_pattern".to_string(),
            attributes: [("unit".to_string(), "ipr".to_string())]
                .into_iter()
                .collect(),
        });
    }
}

fn extract_materials_facts(facts: &mut Vec<ExtractedFact>, text: &str) {
    // Material names (common alloys)
    let materials = [
        ("Ti-6Al-4V", "TitaniumAlloy"),
        ("Inconel 718", "NickelAlloy"),
        ("304 stainless", "StainlessSteel"),
        ("6061-T6", "AluminumAlloy"),
    ];

    for (pattern, mat_type) in materials {
        if text.contains(pattern) {
            facts.push(ExtractedFact {
                fact_type: mat_type.to_string(),
                content: pattern.to_string(),
                confidence: 0.9,
                source: "text_pattern".to_string(),
                attributes: HashMap::new(),
            });
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedFact {
    pub fact_type: String,
    pub content: String,
    pub confidence: f64,
    pub source: String,
    pub attributes: HashMap<String, String>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunking() {
        let doc = PdfDocument {
            metadata: PdfMetadata::default(),
            pages: vec![],
            outline: vec![],
            text: "This is a test document with some words that will be chunked.".to_string(),
        };

        let parser = PdfParser::new().with_chunk_size(20);
        let chunks = parser.create_chunks(&doc);

        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(chunk.text.len() <= 25); // Some tolerance
        }
    }

    #[test]
    fn test_machining_extraction() {
        let text = "Use a cutting speed of 150 SFM with a feed of 0.005 ipr.";
        let mut facts = Vec::new();
        extract_machining_facts(&mut facts, text);

        assert!(facts.iter().any(|f| f.fact_type == "CuttingSpeed"));
        assert!(facts.iter().any(|f| f.fact_type == "FeedRate"));
    }
}
