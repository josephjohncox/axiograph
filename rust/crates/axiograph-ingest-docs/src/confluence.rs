//! Confluence document parsing
//!
//! Extracts knowledge from Confluence-style wiki pages.
//! Handles:
//! - HTML export format
//! - Structured content (tables, lists, code blocks)
//! - Page hierarchy and links

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{Chunk, DocumentExtraction};

/// A Confluence page structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfluencePage {
    pub page_id: String,
    pub title: String,
    pub space: String,
    pub parent_id: Option<String>,
    pub content: PageContent,
    pub labels: Vec<String>,
    pub last_modified: Option<String>,
    pub author: Option<String>,
}

/// Structured content from a page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageContent {
    pub sections: Vec<Section>,
    pub tables: Vec<Table>,
    pub code_blocks: Vec<CodeBlock>,
    pub links: Vec<PageLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub heading: String,
    pub level: u8,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub caption: Option<String>,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeBlock {
    pub language: Option<String>,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageLink {
    pub text: String,
    pub target_page_id: Option<String>,
    pub url: Option<String>,
}

/// Parse Confluence HTML export
pub fn parse_confluence_html(html: &str, page_id: &str, space: &str) -> Result<ConfluencePage> {
    // Extract title from <title> or <h1>
    let title_re = Regex::new(r"<title>([^<]+)</title>").unwrap();
    let h1_re = Regex::new(r"<h1[^>]*>([^<]+)</h1>").unwrap();

    let title = title_re
        .captures(html)
        .or_else(|| h1_re.captures(html))
        .map(|c| c[1].to_string())
        .unwrap_or_else(|| "Untitled".to_string());

    // Extract sections (h2, h3, etc. with following content)
    let section_re = Regex::new(r"<h([2-6])[^>]*>([^<]+)</h\d>").unwrap();
    let mut sections = Vec::new();

    for caps in section_re.captures_iter(html) {
        let level: u8 = caps[1].parse().unwrap_or(2);
        let heading = caps[2].to_string();
        // Extract text between this heading and the next
        let start = caps.get(0).unwrap().end();
        let end = section_re
            .find_at(html, start)
            .map(|m| m.start())
            .unwrap_or(html.len());

        let section_html = &html[start..end];
        let text = strip_html_tags(section_html);

        sections.push(Section {
            heading,
            level,
            text,
        });
    }

    // Extract tables
    let tables = extract_tables(html);

    // Extract code blocks
    let code_blocks = extract_code_blocks(html);

    // Extract links
    let links = extract_links(html);

    // Extract labels (often in a specific div or meta)
    let labels_re = Regex::new(r#"data-label="([^"]+)""#).unwrap();
    let labels: Vec<String> = labels_re
        .captures_iter(html)
        .map(|c| c[1].to_string())
        .collect();

    Ok(ConfluencePage {
        page_id: page_id.to_string(),
        title,
        space: space.to_string(),
        parent_id: None,
        content: PageContent {
            sections,
            tables,
            code_blocks,
            links,
        },
        labels,
        last_modified: None,
        author: None,
    })
}

fn strip_html_tags(html: &str) -> String {
    let tag_re = Regex::new(r"<[^>]+>").unwrap();
    let text = tag_re.replace_all(html, " ");
    // Collapse whitespace
    let ws_re = Regex::new(r"\s+").unwrap();
    ws_re.replace_all(&text, " ").trim().to_string()
}

fn extract_tables(html: &str) -> Vec<Table> {
    let table_re = Regex::new(r"(?s)<table[^>]*>(.*?)</table>").unwrap();
    let row_re = Regex::new(r"(?s)<tr[^>]*>(.*?)</tr>").unwrap();
    let cell_re = Regex::new(r"(?s)<t[hd][^>]*>(.*?)</t[hd]>").unwrap();

    let mut tables = Vec::new();

    for table_caps in table_re.captures_iter(html) {
        let table_html = &table_caps[1];
        let mut headers = Vec::new();
        let mut rows = Vec::new();
        let mut is_first_row = true;

        for row_caps in row_re.captures_iter(table_html) {
            let row_html = &row_caps[1];
            let cells: Vec<String> = cell_re
                .captures_iter(row_html)
                .map(|c| strip_html_tags(&c[1]))
                .collect();

            if is_first_row && row_html.contains("<th") {
                headers = cells;
                is_first_row = false;
            } else if !cells.is_empty() {
                rows.push(cells);
            }
        }

        if !rows.is_empty() || !headers.is_empty() {
            tables.push(Table {
                caption: None,
                headers,
                rows,
            });
        }
    }

    tables
}

fn extract_code_blocks(html: &str) -> Vec<CodeBlock> {
    let code_re = Regex::new(
        r#"(?s)<pre[^>]*(?:data-language="([^"]*)")?[^>]*><code[^>]*>(.*?)</code></pre>"#,
    )
    .unwrap();
    let alt_re = Regex::new(
        r#"(?s)<ac:structured-macro[^>]*ac:name="code"[^>]*>.*?<ac:parameter ac:name="language">([^<]*)</ac:parameter>.*?<ac:plain-text-body><!\[CDATA\[(.*?)\]\]></ac:plain-text-body>"#,
    )
    .unwrap();

    let mut blocks = Vec::new();

    for caps in code_re.captures_iter(html) {
        let language = caps.get(1).map(|m| m.as_str().to_string());
        let code = strip_html_tags(&caps[2]);
        if !code.trim().is_empty() {
            blocks.push(CodeBlock { language, code });
        }
    }

    for caps in alt_re.captures_iter(html) {
        let language = Some(caps[1].to_string());
        let code = caps[2].to_string();
        if !code.trim().is_empty() {
            blocks.push(CodeBlock { language, code });
        }
    }

    blocks
}

fn extract_links(html: &str) -> Vec<PageLink> {
    let link_re = Regex::new(r#"<a[^>]*href="([^"]*)"[^>]*>([^<]*)</a>"#).unwrap();

    link_re
        .captures_iter(html)
        .map(|caps| PageLink {
            url: Some(caps[1].to_string()),
            text: caps[2].to_string(),
            target_page_id: None,
        })
        .collect()
}

/// Convert Confluence page to document extraction
pub fn confluence_to_extraction(page: &ConfluencePage) -> DocumentExtraction {
    let mut chunks = Vec::new();

    // Sections as chunks
    for (i, section) in page.content.sections.iter().enumerate() {
        let mut metadata = HashMap::new();
        metadata.insert("section".to_string(), section.heading.clone());
        metadata.insert("level".to_string(), section.level.to_string());
        metadata.insert("source_type".to_string(), "confluence".to_string());
        metadata.insert("space".to_string(), page.space.clone());
        for label in &page.labels {
            metadata.insert(format!("label_{}", label), "true".to_string());
        }

        chunks.push(Chunk {
            chunk_id: format!("{}_section_{}", page.page_id, i),
            document_id: page.page_id.clone(),
            page: None,
            span_id: format!("section_{}", i),
            text: section.text.clone(),
            bbox: None,
            metadata,
        });
    }

    // Tables as chunks (structured)
    for (i, table) in page.content.tables.iter().enumerate() {
        let mut text = String::new();
        if !table.headers.is_empty() {
            text.push_str(&table.headers.join(" | "));
            text.push('\n');
        }
        for row in &table.rows {
            text.push_str(&row.join(" | "));
            text.push('\n');
        }

        let mut metadata = HashMap::new();
        metadata.insert("type".to_string(), "table".to_string());
        metadata.insert("source_type".to_string(), "confluence".to_string());
        if let Some(cap) = &table.caption {
            metadata.insert("caption".to_string(), cap.clone());
        }

        chunks.push(Chunk {
            chunk_id: format!("{}_table_{}", page.page_id, i),
            document_id: page.page_id.clone(),
            page: None,
            span_id: format!("table_{}", i),
            text,
            bbox: None,
            metadata,
        });
    }

    // Code blocks (often contain examples)
    for (i, block) in page.content.code_blocks.iter().enumerate() {
        let mut metadata = HashMap::new();
        metadata.insert("type".to_string(), "code".to_string());
        metadata.insert("source_type".to_string(), "confluence".to_string());
        if let Some(lang) = &block.language {
            metadata.insert("language".to_string(), lang.clone());
        }

        chunks.push(Chunk {
            chunk_id: format!("{}_code_{}", page.page_id, i),
            document_id: page.page_id.clone(),
            page: None,
            span_id: format!("code_{}", i),
            text: block.code.clone(),
            bbox: None,
            metadata,
        });
    }

    DocumentExtraction {
        source_path: format!("confluence://{}/{}", page.space, page.page_id),
        document_id: page.page_id.clone(),
        title: Some(page.title.clone()),
        chunks,
        metadata: {
            let mut m = HashMap::new();
            m.insert("space".to_string(), page.space.clone());
            m.insert("type".to_string(), "confluence".to_string());
            m.insert("labels".to_string(), page.labels.join(", "));
            m
        },
    }
}
