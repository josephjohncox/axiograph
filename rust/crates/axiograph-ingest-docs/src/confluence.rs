//! Confluence document parsing
//!
//! Extracts knowledge from Confluence-style wiki pages.
//! Handles:
//! - HTML export format
//! - Structured content (tables, lists, code blocks)
//! - Page hierarchy and links

use anyhow::{anyhow, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{Chunk, DocumentExtraction};

const MAX_CONFLUENCE_HTML_BYTES: usize = 16 * 1024 * 1024;
const MAX_CONFLUENCE_ITEMS: usize = 100_000;

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
    if html.len() > MAX_CONFLUENCE_HTML_BYTES {
        return Err(anyhow!(
            "Confluence HTML exceeds {MAX_CONFLUENCE_HTML_BYTES} bytes"
        ));
    }
    if page_id.is_empty() || page_id.len() > 1024 || space.len() > 1024 {
        return Err(anyhow!(
            "Confluence page id/space exceeds identifier limits"
        ));
    }
    // Extract title from <title> or <h1>
    let title_re = Regex::new(r"<title>([^<]+)</title>")?;
    let h1_re = Regex::new(r"<h1[^>]*>([^<]+)</h1>")?;

    let title = title_re
        .captures(html)
        .or_else(|| h1_re.captures(html))
        .and_then(|captures| captures.get(1))
        .map(|title| title.as_str().to_string())
        .unwrap_or_else(|| "Untitled".to_string());

    // Extract sections (h2, h3, etc. with following content)
    let section_re = Regex::new(r"<h([2-6])[^>]*>([^<]+)</h\d>")?;
    let mut sections = Vec::new();

    for caps in section_re.captures_iter(html) {
        if sections.len() >= MAX_CONFLUENCE_ITEMS {
            return Err(anyhow!(
                "Confluence section count exceeds {MAX_CONFLUENCE_ITEMS}"
            ));
        }
        let level = caps
            .get(1)
            .and_then(|level| level.as_str().parse::<u8>().ok())
            .unwrap_or(2);
        let Some(heading) = caps.get(2) else {
            continue;
        };
        let Some(full_match) = caps.get(0) else {
            continue;
        };
        let heading = heading.as_str().to_string();
        // Extract text between this heading and the next
        let start = full_match.end();
        let end = section_re
            .find_at(html, start)
            .map(|m| m.start())
            .unwrap_or(html.len());

        let section_html = &html[start..end];
        let text = strip_html_tags(section_html)?;

        sections.push(Section {
            heading,
            level,
            text,
        });
    }

    // Extract tables
    let tables = extract_tables(html)?;

    // Extract code blocks
    let code_blocks = extract_code_blocks(html)?;

    // Extract links
    let links = extract_links(html)?;

    // Extract labels (often in a specific div or meta)
    let labels_re = Regex::new(r#"data-label="([^"]+)""#)?;
    let labels: Vec<String> = labels_re
        .captures_iter(html)
        .take(MAX_CONFLUENCE_ITEMS + 1)
        .filter_map(|captures| captures.get(1))
        .map(|label| label.as_str().to_string())
        .collect();

    let table_items = tables.iter().try_fold(0_usize, |total, table| {
        table
            .rows
            .iter()
            .try_fold(total.saturating_add(table.headers.len()), |total, row| {
                total.checked_add(row.len())
            })
    });
    let total_items = table_items
        .and_then(|count| count.checked_add(sections.len()))
        .and_then(|count| count.checked_add(code_blocks.len()))
        .and_then(|count| count.checked_add(links.len()))
        .and_then(|count| count.checked_add(labels.len()))
        .ok_or_else(|| anyhow!("Confluence item count overflow"))?;
    if total_items > MAX_CONFLUENCE_ITEMS {
        return Err(anyhow!(
            "Confluence item count {total_items} exceeds {MAX_CONFLUENCE_ITEMS}"
        ));
    }

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

fn strip_html_tags(html: &str) -> Result<String> {
    let tag_re = Regex::new(r"<[^>]+>")?;
    let text = tag_re.replace_all(html, " ");
    // Collapse whitespace
    let ws_re = Regex::new(r"\s+")?;
    Ok(ws_re.replace_all(&text, " ").trim().to_string())
}

fn extract_tables(html: &str) -> Result<Vec<Table>> {
    let table_re = Regex::new(r"(?s)<table[^>]*>(.*?)</table>")?;
    let row_re = Regex::new(r"(?s)<tr[^>]*>(.*?)</tr>")?;
    let cell_re = Regex::new(r"(?s)<t[hd][^>]*>(.*?)</t[hd]>")?;

    let mut tables = Vec::new();
    let mut item_count = 0_usize;

    for table_caps in table_re.captures_iter(html) {
        let Some(table_html) = table_caps.get(1) else {
            continue;
        };
        let table_html = table_html.as_str();
        let mut headers = Vec::new();
        let mut rows = Vec::new();
        let mut is_first_row = true;

        for row_caps in row_re.captures_iter(table_html) {
            let Some(row_html) = row_caps.get(1) else {
                continue;
            };
            let row_html = row_html.as_str();
            let mut cells = Vec::new();
            for cell_caps in cell_re.captures_iter(row_html) {
                if let Some(cell) = cell_caps.get(1) {
                    if item_count >= MAX_CONFLUENCE_ITEMS {
                        return Err(anyhow!(
                            "Confluence table item count exceeds {MAX_CONFLUENCE_ITEMS}"
                        ));
                    }
                    cells.push(strip_html_tags(cell.as_str())?);
                    item_count += 1;
                }
            }

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

    Ok(tables)
}

fn extract_code_blocks(html: &str) -> Result<Vec<CodeBlock>> {
    let code_re = Regex::new(
        r#"(?s)<pre[^>]*(?:data-language="([^"]*)")?[^>]*><code[^>]*>(.*?)</code></pre>"#,
    )?;
    let alt_re = Regex::new(
        r#"(?s)<ac:structured-macro[^>]*ac:name="code"[^>]*>.*?<ac:parameter ac:name="language">([^<]*)</ac:parameter>.*?<ac:plain-text-body><!\[CDATA\[(.*?)\]\]></ac:plain-text-body>"#,
    )?;

    let mut blocks = Vec::new();

    for caps in code_re.captures_iter(html) {
        let language = caps.get(1).map(|value| value.as_str().to_string());
        let Some(code) = caps.get(2) else {
            continue;
        };
        let code = strip_html_tags(code.as_str())?;
        if !code.trim().is_empty() {
            if blocks.len() >= MAX_CONFLUENCE_ITEMS {
                return Err(anyhow!(
                    "Confluence code block count exceeds {MAX_CONFLUENCE_ITEMS}"
                ));
            }
            blocks.push(CodeBlock { language, code });
        }
    }

    for caps in alt_re.captures_iter(html) {
        let Some(language) = caps.get(1) else {
            continue;
        };
        let Some(code) = caps.get(2) else {
            continue;
        };
        let language = Some(language.as_str().to_string());
        let code = code.as_str().to_string();
        if !code.trim().is_empty() {
            if blocks.len() >= MAX_CONFLUENCE_ITEMS {
                return Err(anyhow!(
                    "Confluence code block count exceeds {MAX_CONFLUENCE_ITEMS}"
                ));
            }
            blocks.push(CodeBlock { language, code });
        }
    }

    Ok(blocks)
}

fn extract_links(html: &str) -> Result<Vec<PageLink>> {
    let link_re = Regex::new(r#"<a[^>]*href="([^"]*)"[^>]*>([^<]*)</a>"#)?;
    let mut links = Vec::new();
    for captures in link_re.captures_iter(html) {
        let (Some(url), Some(text)) = (captures.get(1), captures.get(2)) else {
            continue;
        };
        if links.len() >= MAX_CONFLUENCE_ITEMS {
            return Err(anyhow!(
                "Confluence link count exceeds {MAX_CONFLUENCE_ITEMS}"
            ));
        }
        links.push(PageLink {
            url: Some(url.as_str().to_string()),
            text: text.as_str().to_string(),
            target_page_id: None,
        });
    }
    Ok(links)
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
            metadata.insert(format!("label_{label}"), "true".to_string());
        }

        chunks.push(Chunk {
            chunk_id: format!("{}_section_{}", page.page_id, i),
            document_id: page.page_id.clone(),
            page: None,
            span_id: format!("section_{i}"),
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
            span_id: format!("table_{i}"),
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
            span_id: format!("code_{i}"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confluence_parser_extracts_bounded_structured_content() {
        let html = r#"
            <title>Operations</title>
            <h2>Cutting</h2><p>Use coolant.</p>
            <table><tr><th>Tool</th></tr><tr><td>Mill</td></tr></table>
            <pre data-language="rust"><code>fn cut() {}</code></pre>
            <a href="/next">Next</a>
            <span data-label="machining"></span>
        "#;
        let page =
            parse_confluence_html(html, "page-1", "OPS").expect("parse bounded Confluence fixture");
        assert_eq!(page.title, "Operations");
        assert_eq!(page.content.sections.len(), 1);
        assert_eq!(page.content.tables.len(), 1);
        assert_eq!(page.content.code_blocks.len(), 1);
        assert_eq!(page.content.links.len(), 1);
        assert_eq!(page.labels, ["machining"]);
    }

    #[test]
    fn confluence_parser_rejects_link_fanout_during_extraction() {
        let html = r#"<a href="/next">Next</a>"#.repeat(MAX_CONFLUENCE_ITEMS + 1);
        let error = parse_confluence_html(&html, "page-1", "OPS")
            .expect_err("link fanout must fail closed");
        assert!(error.to_string().contains("link count"));
    }
}
