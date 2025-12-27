//! Recommended readings ingestion
//!
//! Handles bibliographic data, book references, and academic papers.
//! Extracts metadata and key passages for the knowledge graph.

#![allow(unused_imports)]

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{Chunk, DocumentExtraction};

/// A bibliographic entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BibEntry {
    pub id: String,
    pub entry_type: EntryType,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<u32>,
    pub publisher: Option<String>,
    pub journal: Option<String>,
    pub volume: Option<String>,
    pub pages: Option<String>,
    pub doi: Option<String>,
    pub isbn: Option<String>,
    pub abstract_text: Option<String>,
    pub keywords: Vec<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EntryType {
    Book,
    Article,
    InProceedings,
    TechnicalReport,
    Manual,
    Thesis,
    Other,
}

/// A recommended reading with relevance information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendedReading {
    pub bib: BibEntry,
    pub relevance_domains: Vec<String>,
    pub key_topics: Vec<String>,
    pub importance: f64, // 0-1 scale
    pub notes: String,
}

/// Parse BibTeX format
pub fn parse_bibtex(content: &str) -> Vec<BibEntry> {
    let entry_re = Regex::new(r"@(\w+)\s*\{\s*([^,]+),\s*((?:[^@])*)\}").unwrap();
    let field_re = Regex::new(r"(\w+)\s*=\s*\{([^}]*)\}").unwrap();

    let mut entries = Vec::new();

    for caps in entry_re.captures_iter(content) {
        let entry_type = match caps[1].to_lowercase().as_str() {
            "book" => EntryType::Book,
            "article" => EntryType::Article,
            "inproceedings" | "conference" => EntryType::InProceedings,
            "techreport" => EntryType::TechnicalReport,
            "manual" => EntryType::Manual,
            "phdthesis" | "mastersthesis" => EntryType::Thesis,
            _ => EntryType::Other,
        };

        let id = caps[2].trim().to_string();
        let fields = &caps[3];

        let mut field_map: HashMap<String, String> = HashMap::new();
        for fcaps in field_re.captures_iter(fields) {
            field_map.insert(fcaps[1].to_lowercase(), fcaps[2].to_string());
        }

        let authors = field_map
            .get("author")
            .map(|a| a.split(" and ").map(|s| s.trim().to_string()).collect())
            .unwrap_or_default();

        entries.push(BibEntry {
            id,
            entry_type,
            title: field_map.get("title").cloned().unwrap_or_default(),
            authors,
            year: field_map.get("year").and_then(|y| y.parse().ok()),
            publisher: field_map.get("publisher").cloned(),
            journal: field_map.get("journal").cloned(),
            volume: field_map.get("volume").cloned(),
            pages: field_map.get("pages").cloned(),
            doi: field_map.get("doi").cloned(),
            isbn: field_map.get("isbn").cloned(),
            abstract_text: field_map.get("abstract").cloned(),
            keywords: field_map
                .get("keywords")
                .map(|k| k.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default(),
            notes: field_map.get("note").cloned(),
        });
    }

    entries
}

/// Parse a reading list (markdown format)
/// Expected format:
/// ```text
/// ## Domain: Machining
///
/// - **Title** by Author (Year)
///   Topics: topic1, topic2
///   Importance: high
///   Notes: Why this is recommended
/// ```
pub fn parse_reading_list(content: &str) -> Vec<RecommendedReading> {
    let domain_re = Regex::new(r"##\s*Domain:\s*(.+)").unwrap();
    let entry_re = Regex::new(r"(?m)^-\s+\*\*(.+?)\*\*\s+by\s+(.+?)\s*\((\d{4})\)").unwrap();
    let topics_re = Regex::new(r"Topics:\s*(.+)").unwrap();
    let importance_re = Regex::new(r"Importance:\s*(\w+)").unwrap();
    let notes_re = Regex::new(r"Notes:\s*(.+)").unwrap();

    let mut readings = Vec::new();
    let mut current_domain = "general".to_string();

    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        if let Some(caps) = domain_re.captures(line) {
            current_domain = caps[1].trim().to_string();
            i += 1;
            continue;
        }

        if let Some(caps) = entry_re.captures(line) {
            let title = caps[1].to_string();
            let author = caps[2].to_string();
            let year: u32 = caps[3].parse().unwrap_or(0);

            // Look ahead for metadata
            let mut topics = Vec::new();
            let mut importance = 0.5;
            let mut notes = String::new();

            while i + 1 < lines.len() {
                i += 1;
                let next = lines[i];

                if next.starts_with('-') || next.starts_with('#') {
                    i -= 1; // Back up for next iteration
                    break;
                }

                if let Some(tcaps) = topics_re.captures(next) {
                    topics = tcaps[1].split(',').map(|s| s.trim().to_string()).collect();
                }

                if let Some(icaps) = importance_re.captures(next) {
                    importance = match icaps[1].to_lowercase().as_str() {
                        "critical" | "essential" => 1.0,
                        "high" => 0.8,
                        "medium" => 0.5,
                        "low" => 0.3,
                        _ => 0.5,
                    };
                }

                if let Some(ncaps) = notes_re.captures(next) {
                    notes = ncaps[1].to_string();
                }
            }

            readings.push(RecommendedReading {
                bib: BibEntry {
                    id: sanitize_id(&title),
                    entry_type: EntryType::Book,
                    title,
                    authors: vec![author],
                    year: Some(year),
                    publisher: None,
                    journal: None,
                    volume: None,
                    pages: None,
                    doi: None,
                    isbn: None,
                    abstract_text: None,
                    keywords: topics.clone(),
                    notes: Some(notes.clone()),
                },
                relevance_domains: vec![current_domain.clone()],
                key_topics: topics,
                importance,
                notes,
            });
        }

        i += 1;
    }

    readings
}

fn sanitize_id(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>()
        .to_lowercase()
}

/// Convert readings to Axiograph schema
pub fn readings_to_extraction(readings: &[RecommendedReading], doc_id: &str) -> DocumentExtraction {
    let chunks: Vec<Chunk> = readings
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let mut metadata = HashMap::new();
            metadata.insert("type".to_string(), "bibliography".to_string());
            metadata.insert("entry_type".to_string(), format!("{:?}", r.bib.entry_type));
            metadata.insert("authors".to_string(), r.bib.authors.join(", "));
            if let Some(year) = r.bib.year {
                metadata.insert("year".to_string(), year.to_string());
            }
            metadata.insert("importance".to_string(), format!("{:.2}", r.importance));
            metadata.insert("domains".to_string(), r.relevance_domains.join(", "));
            metadata.insert("topics".to_string(), r.key_topics.join(", "));

            let text = format!(
                "{}: {} ({}). {}",
                r.bib.title,
                r.bib.authors.join(", "),
                r.bib.year.map(|y| y.to_string()).unwrap_or_default(),
                r.notes
            );

            Chunk {
                chunk_id: format!("{}_{}", doc_id, i),
                document_id: doc_id.to_string(),
                page: None,
                span_id: r.bib.id.clone(),
                text,
                bbox: None,
                metadata,
            }
        })
        .collect();

    DocumentExtraction {
        source_path: "readings".to_string(),
        document_id: doc_id.to_string(),
        title: Some("Recommended Readings".to_string()),
        chunks,
        metadata: {
            let mut m = HashMap::new();
            m.insert("type".to_string(), "bibliography".to_string());
            m.insert("count".to_string(), readings.len().to_string());
            m
        },
    }
}

/// Well-known machining references
pub fn canonical_machining_references() -> Vec<RecommendedReading> {
    vec![
        RecommendedReading {
            bib: BibEntry {
                id: "machinery_handbook".to_string(),
                entry_type: EntryType::Book,
                title: "Machinery's Handbook".to_string(),
                authors: vec!["Erik Oberg".to_string(), "Franklin D. Jones".to_string()],
                year: Some(2020),
                publisher: Some("Industrial Press".to_string()),
                journal: None,
                volume: None,
                pages: None,
                doi: None,
                isbn: Some("978-0-8311-3091-3".to_string()),
                abstract_text: None,
                keywords: vec!["machining".to_string(), "reference".to_string()],
                notes: Some("The essential machinist's reference".to_string()),
            },
            relevance_domains: vec!["machining".to_string()],
            key_topics: vec![
                "speeds".to_string(),
                "feeds".to_string(),
                "materials".to_string(),
            ],
            importance: 1.0,
            notes: "Foundational reference for all machining parameters".to_string(),
        },
        RecommendedReading {
            bib: BibEntry {
                id: "metal_cutting_principles".to_string(),
                entry_type: EntryType::Book,
                title: "Metal Cutting Principles".to_string(),
                authors: vec!["Milton C. Shaw".to_string()],
                year: Some(2005),
                publisher: Some("Oxford University Press".to_string()),
                journal: None,
                volume: None,
                pages: None,
                doi: None,
                isbn: Some("978-0195142068".to_string()),
                abstract_text: None,
                keywords: vec!["cutting theory".to_string(), "chip formation".to_string()],
                notes: None,
            },
            relevance_domains: vec!["machining".to_string(), "physics".to_string()],
            key_topics: vec![
                "chip formation".to_string(),
                "cutting forces".to_string(),
                "tool wear".to_string(),
            ],
            importance: 0.9,
            notes: "Theoretical foundation for metal cutting".to_string(),
        },
        RecommendedReading {
            bib: BibEntry {
                id: "manufacturing_engineering".to_string(),
                entry_type: EntryType::Book,
                title: "Manufacturing Engineering and Technology".to_string(),
                authors: vec!["Serope Kalpakjian".to_string(), "Steven Schmid".to_string()],
                year: Some(2013),
                publisher: Some("Pearson".to_string()),
                journal: None,
                volume: None,
                pages: None,
                doi: None,
                isbn: Some("978-0133128741".to_string()),
                abstract_text: None,
                keywords: vec!["manufacturing".to_string(), "processes".to_string()],
                notes: None,
            },
            relevance_domains: vec!["machining".to_string(), "manufacturing".to_string()],
            key_topics: vec![
                "processes".to_string(),
                "materials".to_string(),
                "design".to_string(),
            ],
            importance: 0.85,
            notes: "Comprehensive manufacturing textbook".to_string(),
        },
    ]
}
