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
pub fn parse_bibtex(content: &str) -> Result<Vec<BibEntry>> {
    let entry_re = Regex::new(r"@(\w+)\s*\{\s*([^,]+),\s*((?:[^@])*)\}")?;
    let field_re = Regex::new(r"(\w+)\s*=\s*\{([^}]*)\}")?;

    let mut entries = Vec::new();

    for caps in entry_re.captures_iter(content) {
        let (Some(entry_type), Some(id), Some(fields)) = (caps.get(1), caps.get(2), caps.get(3))
        else {
            continue;
        };
        let entry_type = match entry_type.as_str().to_lowercase().as_str() {
            "book" => EntryType::Book,
            "article" => EntryType::Article,
            "inproceedings" | "conference" => EntryType::InProceedings,
            "techreport" => EntryType::TechnicalReport,
            "manual" => EntryType::Manual,
            "phdthesis" | "mastersthesis" => EntryType::Thesis,
            _ => EntryType::Other,
        };

        let id = id.as_str().trim().to_string();
        let fields = fields.as_str();

        let mut field_map: HashMap<String, String> = HashMap::new();
        for fcaps in field_re.captures_iter(fields) {
            if let (Some(key), Some(value)) = (fcaps.get(1), fcaps.get(2)) {
                field_map.insert(key.as_str().to_lowercase(), value.as_str().to_string());
            }
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

    Ok(entries)
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
pub fn parse_reading_list(content: &str) -> Result<Vec<RecommendedReading>> {
    let domain_re = Regex::new(r"##\s*Domain:\s*(.+)")?;
    let entry_re = Regex::new(r"(?m)^-\s+\*\*(.+?)\*\*\s+by\s+(.+?)\s*\((\d{4})\)")?;
    let topics_re = Regex::new(r"Topics:\s*(.+)")?;
    let importance_re = Regex::new(r"Importance:\s*(\w+)")?;
    let notes_re = Regex::new(r"Notes:\s*(.+)")?;

    let mut readings = Vec::new();
    let mut current_domain = "general".to_string();

    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        if let Some(domain) = domain_re
            .captures(line)
            .and_then(|captures| captures.get(1))
        {
            current_domain = domain.as_str().trim().to_string();
            i += 1;
            continue;
        }

        if let Some(caps) = entry_re.captures(line) {
            let (Some(title), Some(author), Some(year)) = (caps.get(1), caps.get(2), caps.get(3))
            else {
                i += 1;
                continue;
            };
            let title = title.as_str().to_string();
            let author = author.as_str().to_string();
            let year = year.as_str().parse::<u32>().unwrap_or(0);

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

                if let Some(topic_list) = topics_re
                    .captures(next)
                    .and_then(|captures| captures.get(1))
                {
                    topics = topic_list
                        .as_str()
                        .split(',')
                        .map(|value| value.trim().to_string())
                        .collect();
                }

                if let Some(importance_name) = importance_re
                    .captures(next)
                    .and_then(|captures| captures.get(1))
                {
                    importance = match importance_name.as_str().to_lowercase().as_str() {
                        "critical" | "essential" => 1.0,
                        "high" => 0.8,
                        "medium" => 0.5,
                        "low" => 0.3,
                        _ => 0.5,
                    };
                }

                if let Some(note) = notes_re.captures(next).and_then(|captures| captures.get(1)) {
                    notes = note.as_str().to_string();
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

    Ok(readings)
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
                chunk_id: format!("{doc_id}_{i}"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_parsers_remain_fallible_and_extract_expected_fields() {
        let bibtex = r#"@book{handbook,
            title={Machinery Handbook},
            author={Erik Oberg and Franklin Jones},
            year={2020}
        }"#;
        let entries = parse_bibtex(bibtex).expect("parse bounded BibTeX fixture");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "handbook");
        assert_eq!(entries[0].authors.len(), 2);

        let markdown = "## Domain: Machining\n\n- **Metal Cutting** by M. Shaw (2005)\n  Topics: cutting, tooling\n  Importance: high\n  Notes: Theory\n";
        let readings = parse_reading_list(markdown).expect("parse bounded reading fixture");
        assert_eq!(readings.len(), 1);
        assert_eq!(readings[0].relevance_domains, ["Machining"]);
        assert_eq!(readings[0].importance, 0.8);
    }
}
