//! Repository (codebase) ingestion for Axiograph.
//!
//! Goal: provide a simple, dependency-light way to index a repo into:
//!
//! - document/code chunks (for approximate discovery),
//! - lightweight structured edges (for “repo knowledge graphs”),
//! - and a stable output contract suitable for later reconciliation/certification.
//!
//! This is intentionally a *prototype* ingester:
//!
//! - It uses regex-based extraction for symbol definitions/imports/TODOs.
//! - It is designed to be replaced or upgraded (e.g. tree-sitter) without changing the
//!   downstream artifact shape.

use crate::{extract_markdown, extract_text, Chunk, DocumentExtraction};
use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use walkdir::WalkDir;

/// Options controlling repository indexing behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoIndexOptions {
    /// Maximum number of files to index (safety cap).
    pub max_files: usize,
    /// Maximum file size to read (bytes).
    pub max_file_bytes: u64,
    /// Lines per code chunk (non-markdown).
    pub lines_per_chunk: usize,
    /// File extensions to include (lowercase, without dot).
    pub include_extensions: Vec<String>,
    /// Directory names to skip (exact match).
    pub exclude_dir_names: Vec<String>,
}

impl Default for RepoIndexOptions {
    fn default() -> Self {
        Self {
            max_files: 50_000,
            max_file_bytes: 512 * 1024,
            lines_per_chunk: 80,
            include_extensions: vec![
                // Code
                "rs".to_string(),
                "lean".to_string(),
                "idr".to_string(),
                "py".to_string(),
                "ts".to_string(),
                "js".to_string(),
                "sql".to_string(),
                "proto".to_string(),
                // Docs / configs
                "md".to_string(),
                "txt".to_string(),
                "toml".to_string(),
                "yaml".to_string(),
                "yml".to_string(),
                "json".to_string(),
            ],
            exclude_dir_names: vec![
                ".git".to_string(),
                "target".to_string(),
                "build".to_string(),
                "dist".to_string(),
                "node_modules".to_string(),
            ],
        }
    }
}

/// Lightweight structured facts extracted from a repo.
///
/// These are *proposal facts* for discovery and navigation. They are not accepted truth by
/// default; they should flow through reconciliation before being promoted into canonical `.axi`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum RepoEdgeV1 {
    /// `file` defines `symbol` (with extracted language + kind).
    DefinesSymbol {
        file: String,
        symbol: String,
        symbol_kind: String,
        language: String,
        confidence: f64,
        source_chunk_id: String,
        evidence_span: String,
    },
    /// `file` imports `module_path` (language-specific import).
    ImportsModule {
        file: String,
        module_path: String,
        language: String,
        confidence: f64,
        source_chunk_id: String,
        evidence_span: String,
    },
    /// A TODO-like marker in `file`.
    Todo {
        file: String,
        language: String,
        confidence: f64,
        source_chunk_id: String,
        evidence_span: String,
    },
}

/// Result of indexing a repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoIndexResult {
    pub root: String,
    pub extraction: DocumentExtraction,
    pub edges: Vec<RepoEdgeV1>,
}

/// Index a repository directory into chunks and lightweight structured edges.
pub fn index_repo(root: &Path, options: &RepoIndexOptions) -> Result<RepoIndexResult> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let root_display = root.to_string_lossy().to_string();

    let root_id = root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".to_string());

    let include: BTreeSet<String> = options.include_extensions.iter().cloned().collect();
    let exclude_dirs: BTreeSet<String> = options.exclude_dir_names.iter().cloned().collect();

    let mut chunks = Vec::new();
    let mut edges = Vec::new();
    let mut files_indexed = 0usize;

    let walker = WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if !entry.file_type().is_dir() {
                return true;
            }

            let name = entry.file_name().to_string_lossy();
            !exclude_dirs.contains(name.as_ref())
        });

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if entry.depth() == 0 {
            continue;
        }

        let path = entry.path();

        if entry.file_type().is_dir() {
            continue;
        }

        if !entry.file_type().is_file() {
            continue;
        }

        if files_indexed >= options.max_files {
            break;
        }

        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if metadata.len() > options.max_file_bytes {
            continue;
        }

        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e.to_lowercase(),
            None => continue,
        };

        if !include.contains(&ext) {
            continue;
        }

        let rel_path = path.strip_prefix(&root).unwrap_or(path);
        let rel_path_str = rel_path.to_string_lossy().to_string();

        let text = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let language = language_from_extension(&ext);
        let doc_id = sanitize_repo_id(&rel_path_str);

        let mut file_chunks = if ext == "md" {
            extract_markdown(&text, &doc_id)
        } else if ext == "txt" {
            extract_text(&text, &doc_id)
        } else {
            extract_code_by_lines(&text, &doc_id, options.lines_per_chunk)
        };

        for chunk in &mut file_chunks.chunks {
            chunk
                .metadata
                .insert("source_type".to_string(), "repo".to_string());
            chunk
                .metadata
                .insert("path".to_string(), rel_path_str.clone());
            chunk
                .metadata
                .insert("language".to_string(), language.to_string());
        }

        // Extract lightweight repo edges from chunks (language-aware).
        for chunk in &file_chunks.chunks {
            edges.extend(extract_repo_edges_from_chunk(
                &rel_path_str,
                language,
                chunk,
            ));
        }

        chunks.extend(file_chunks.chunks);
        files_indexed += 1;
    }

    let mut extraction = DocumentExtraction {
        source_path: root_display.clone(),
        document_id: root_id,
        title: Some(root_display.clone()),
        chunks,
        metadata: HashMap::new(),
    };
    extraction
        .metadata
        .insert("files_indexed".to_string(), files_indexed.to_string());

    Ok(RepoIndexResult {
        root: root_display,
        extraction,
        edges,
    })
}

fn language_from_extension(ext: &str) -> &'static str {
    match ext {
        "rs" => "rust",
        "lean" => "lean",
        "idr" => "idris",
        "md" => "markdown",
        "txt" => "text",
        "sql" => "sql",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "json" => "json",
        "py" => "python",
        "ts" => "typescript",
        "js" => "javascript",
        _ => "text",
    }
}

fn extract_code_by_lines(text: &str, doc_id: &str, lines_per_chunk: usize) -> DocumentExtraction {
    let mut chunks = Vec::new();
    let mut chunk_idx = 0usize;

    let mut current = Vec::new();
    for line in text.lines() {
        current.push(line);
        if current.len() >= lines_per_chunk {
            chunks.push(make_code_chunk(doc_id, chunk_idx, &current));
            chunk_idx += 1;
            current.clear();
        }
    }

    if !current.is_empty() {
        chunks.push(make_code_chunk(doc_id, chunk_idx, &current));
    }

    DocumentExtraction {
        source_path: "".to_string(),
        document_id: doc_id.to_string(),
        title: None,
        chunks,
        metadata: HashMap::new(),
    }
}

fn make_code_chunk(doc_id: &str, chunk_idx: usize, lines: &[&str]) -> Chunk {
    Chunk {
        chunk_id: format!("{}_{}", doc_id, chunk_idx),
        document_id: doc_id.to_string(),
        page: None,
        span_id: format!("lines_{}", chunk_idx),
        text: lines.join("\n"),
        bbox: None,
        metadata: HashMap::new(),
    }
}

fn extract_repo_edges_from_chunk(file: &str, language: &str, chunk: &Chunk) -> Vec<RepoEdgeV1> {
    let mut out = Vec::new();
    let mut counter = 0usize;

    // Common TODO markers.
    let todo_re = Regex::new(r"(?i)\b(TODO|FIXME|HACK)\b").ok();

    let (define_re, import_re) = match language {
        "rust" => (
            Regex::new(r"^\s*(?:pub\s+)?(struct|enum|trait|fn|type|mod)\s+([A-Za-z_][A-Za-z0-9_]*)").ok(),
            Regex::new(r"^\s*use\s+([^;]+);").ok(),
        ),
        "lean" => (
            Regex::new(r"^\s*(inductive|structure|class|def|theorem|lemma|abbrev)\s+([A-Za-z_][A-Za-z0-9_'.]*)").ok(),
            Regex::new(r"^\s*import\s+(.+)").ok(),
        ),
        "idris" => (
            Regex::new(r"^\s*(data|record|interface)\s+([A-Za-z_][A-Za-z0-9_']*)").ok(),
            Regex::new(r"^\s*import\s+(.+)").ok(),
        ),
        _ => (None, None),
    };

    for line in chunk.text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(re) = &define_re {
            if let Some(caps) = re.captures(trimmed) {
                let symbol_kind = caps.get(1).map(|m| m.as_str()).unwrap_or("def");
                let symbol = caps.get(2).map(|m| m.as_str()).unwrap_or("Unknown");
                out.push(RepoEdgeV1::DefinesSymbol {
                    file: file.to_string(),
                    symbol: symbol.to_string(),
                    symbol_kind: symbol_kind.to_string(),
                    language: language.to_string(),
                    confidence: 0.95,
                    source_chunk_id: chunk.chunk_id.clone(),
                    evidence_span: trimmed.to_string(),
                });
                counter += 1;
            }
        }

        if let Some(re) = &import_re {
            if let Some(caps) = re.captures(trimmed) {
                let module_path = caps.get(1).map(|m| m.as_str()).unwrap_or("").trim();
                if !module_path.is_empty() {
                    out.push(RepoEdgeV1::ImportsModule {
                        file: file.to_string(),
                        module_path: module_path.to_string(),
                        language: language.to_string(),
                        confidence: 0.95,
                        source_chunk_id: chunk.chunk_id.clone(),
                        evidence_span: trimmed.to_string(),
                    });
                    counter += 1;
                }
            }
        }

        if let Some(re) = &todo_re {
            if re.is_match(trimmed) {
                out.push(RepoEdgeV1::Todo {
                    file: file.to_string(),
                    language: language.to_string(),
                    confidence: 0.9,
                    source_chunk_id: chunk.chunk_id.clone(),
                    evidence_span: trimmed.to_string(),
                });
                counter += 1;
            }
        }

        if counter > 10_000 {
            break;
        }
    }

    out
}

fn sanitize_repo_id(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(120)
        .collect()
}
