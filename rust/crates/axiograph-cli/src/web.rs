//! Web ingestion (scrape/crawl) helpers.
//!
//! This is **untrusted tooling** intended for discovery workflows:
//! - fetch pages (respectful defaults: rate limits, size caps),
//! - extract text/markdown from HTML,
//! - emit `chunks.json` + extracted facts + `proposals.json` (Evidence/Proposals schema).
//!
//! This is NOT part of the trusted semantics kernel.

use anyhow::{anyhow, Context, Result};
use clap::Subcommand;
use colored::Colorize;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use scraper::{Html, Selector};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

#[derive(Subcommand)]
pub enum WebCommands {
    /// Fetch (and optionally crawl) web pages, then emit `chunks.json` + `proposals.json`.
    ///
    /// Inputs:
    /// - list mode: `--url ...` and/or `--urls-file ...`
    /// - crawl mode: `--crawl --seed ...` (follows links up to `--max-pages`)
    Ingest {
        /// Output directory for artifacts (`pages/`, `chunks.json`, `facts.json`, `proposals.json`).
        #[arg(short, long, default_value = "build/web_ingest")]
        out_dir: PathBuf,

        /// Fetch this URL (repeatable).
        #[arg(long)]
        url: Vec<String>,

        /// Read URLs from a file (one per line; `#` comments allowed).
        #[arg(long)]
        urls_file: Option<PathBuf>,

        /// Enable crawling (follow links from fetched pages).
        #[arg(long)]
        crawl: bool,

        /// Seed URL(s) for crawling (repeatable).
        #[arg(long)]
        seed: Vec<String>,

        /// Maximum pages to fetch (safety cap).
        #[arg(long, default_value_t = 200)]
        max_pages: usize,

        /// Maximum crawl depth (0 = only seeds).
        #[arg(long, default_value_t = 2)]
        max_depth: usize,

        /// Only follow links within the same host as the seed(s).
        #[arg(long, default_value_t = true)]
        same_host: bool,

        /// Allow following links to these hostnames (repeatable).
        ///
        /// If set, `--same-host` is ignored and the allowlist is used instead.
        #[arg(long)]
        allow_host: Vec<String>,

        /// HTTP User-Agent.
        #[arg(
            long,
            default_value = "axiograph/0.6 (+https://github.com/axiograph/axiograph)"
        )]
        user_agent: String,

        /// Per-request timeout in seconds.
        #[arg(long, default_value_t = 20)]
        timeout_secs: u64,

        /// Delay between requests in milliseconds (politeness).
        #[arg(long, default_value_t = 250)]
        delay_ms: u64,

        /// Respect `robots.txt` (recommended).
        #[arg(long, default_value_t = true)]
        respect_robots: bool,

        /// Skip downloading pages larger than this many bytes (Content-Length guard).
        #[arg(long, default_value_t = 2_000_000)]
        max_html_bytes: usize,

        /// Keep raw HTML under `<out_dir>/pages/`.
        #[arg(long, default_value_t = true)]
        store_html: bool,

        /// Overwrite existing output directory contents.
        #[arg(long)]
        overwrite: bool,

        /// Domain for fact extraction (`general` / `machining` / etc).
        #[arg(long, default_value = "general")]
        domain: String,
    },
}

pub fn cmd_web(command: WebCommands) -> Result<()> {
    match command {
        WebCommands::Ingest {
            out_dir,
            url,
            urls_file,
            crawl,
            seed,
            max_pages,
            max_depth,
            same_host,
            allow_host,
            user_agent,
            timeout_secs,
            delay_ms,
            respect_robots,
            max_html_bytes,
            store_html,
            overwrite,
            domain,
        } => cmd_web_ingest(
            &out_dir,
            &url,
            urls_file.as_ref(),
            crawl,
            &seed,
            max_pages,
            max_depth,
            same_host,
            &allow_host,
            &user_agent,
            timeout_secs,
            delay_ms,
            respect_robots,
            max_html_bytes,
            store_html,
            overwrite,
            &domain,
        ),
    }
}

#[derive(Debug, Clone)]
struct CrawlItem {
    url: Url,
    depth: usize,
}

fn cmd_web_ingest(
    out_dir: &PathBuf,
    urls: &[String],
    urls_file: Option<&PathBuf>,
    crawl: bool,
    seeds: &[String],
    max_pages: usize,
    max_depth: usize,
    same_host: bool,
    allow_hosts: &[String],
    user_agent: &str,
    timeout_secs: u64,
    delay_ms: u64,
    respect_robots: bool,
    max_html_bytes: usize,
    store_html: bool,
    overwrite: bool,
    domain: &str,
) -> Result<()> {
    if max_pages == 0 {
        return Err(anyhow!("--max-pages must be > 0"));
    }

    if overwrite && out_dir.exists() {
        fs::remove_dir_all(out_dir).with_context(|| {
            format!(
                "failed to remove existing output dir (use without --overwrite to keep): {}",
                out_dir.display()
            )
        })?;
    }
    fs::create_dir_all(out_dir)?;

    let pages_dir = out_dir.join("pages");
    if store_html {
        fs::create_dir_all(&pages_dir)?;
    }

    let mut initial_urls: Vec<Url> = Vec::new();
    initial_urls.extend(parse_url_list(urls)?);
    if let Some(path) = urls_file {
        initial_urls.extend(read_urls_file(path)?);
    }

    let mut seed_urls: Vec<Url> = Vec::new();
    seed_urls.extend(parse_url_list(seeds)?);

    if crawl && seed_urls.is_empty() {
        return Err(anyhow!("--crawl requires at least one --seed URL"));
    }
    if !crawl && initial_urls.is_empty() {
        return Err(anyhow!(
            "provide at least one --url or --urls-file, or enable --crawl"
        ));
    }

    let allow_hosts_norm: HashSet<String> = allow_hosts
        .iter()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let seed_hosts: HashSet<String> = seed_urls
        .iter()
        .filter_map(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
        .collect();

    let allowed_hosts: HashSet<String> = if !allow_hosts_norm.is_empty() {
        allow_hosts_norm
    } else if same_host {
        seed_hosts
    } else {
        HashSet::new()
    };

    let client = build_http_client(user_agent, timeout_secs)?;
    let robots_user_agent = robots_user_agent_token(user_agent);
    let mut robots_cache: HashMap<String, String> = HashMap::new();

    println!(
        "{} out={} max_pages={} crawl={} max_depth={} delay_ms={} domain={}",
        "Web ingest".green().bold(),
        out_dir.display(),
        max_pages,
        crawl,
        max_depth,
        delay_ms,
        domain
    );

    let mut queue: VecDeque<CrawlItem> = VecDeque::new();
    let mut seen: HashSet<String> = HashSet::new();

    for u in initial_urls {
        enqueue_url(&mut queue, &mut seen, u, 0);
    }
    for u in seed_urls {
        enqueue_url(&mut queue, &mut seen, u, 0);
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let manifest_path = out_dir.join("manifest.jsonl");
    let mut manifest = String::new();

    let mut all_chunks: Vec<axiograph_ingest_docs::Chunk> = Vec::new();
    let mut all_facts: Vec<axiograph_ingest_docs::ExtractedFact> = Vec::new();
    let mut all_proposals: Vec<axiograph_ingest_docs::ProposalV1> = Vec::new();

    let mut fetched = 0usize;

    while let Some(item) = queue.pop_front() {
        if fetched >= max_pages {
            break;
        }
        if crawl && item.depth > max_depth {
            continue;
        }

        if !allowed_hosts.is_empty() {
            let Some(host) = item.url.host_str().map(|h| h.to_ascii_lowercase()) else {
                continue;
            };
            if !allowed_hosts.contains(&host) {
                continue;
            }
        }

        if respect_robots
            && !robots_allows_url(&client, &mut robots_cache, &robots_user_agent, &item.url)
        {
            continue;
        }

        if delay_ms > 0 && fetched > 0 {
            thread::sleep(Duration::from_millis(delay_ms));
        }

        let res = fetch_html(&client, &item.url, max_html_bytes);
        let fetched_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let (status, content_type, html_text, error) = match res {
            Ok(r) => (r.status, r.content_type, Some(r.body), None),
            Err(e) => (None, None, None, Some(e.to_string())),
        };

        let page_id = url_id(&item.url);
        let mut stored_path = None;
        if store_html {
            if let Some(html) = &html_text {
                let path = pages_dir.join(format!("{page_id}.html"));
                fs::write(&path, html.as_bytes()).ok();
                stored_path = Some(path);
            }
        }

        let manifest_entry = WebManifestEntryV1 {
            url: item.url.as_str().to_string(),
            page_id: page_id.clone(),
            depth: item.depth,
            fetched_at_unix_secs: fetched_at,
            status,
            content_type,
            stored_path: stored_path.as_ref().map(|p| {
                p.strip_prefix(out_dir)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .to_string()
            }),
            error,
        };
        manifest.push_str(&serde_json::to_string(&manifest_entry)?);
        manifest.push('\n');

        let Some(html) = html_text else {
            fetched += 1;
            continue;
        };

        let markdown = html_to_markdown(&html).unwrap_or_else(|_| strip_html_to_text(&html));
        if markdown.trim().is_empty() {
            fetched += 1;
            continue;
        }

        let doc_id = format!("web_{}", page_id);
        let mut extraction = axiograph_ingest_docs::extract_markdown(&markdown, &doc_id);

        for chunk in &mut extraction.chunks {
            chunk
                .metadata
                .insert("source_type".to_string(), "web".to_string());
            chunk
                .metadata
                .insert("url".to_string(), item.url.as_str().to_string());
            chunk
                .metadata
                .insert("fetched_at".to_string(), fetched_at.to_string());
        }

        // Fact extraction (same default patterns as `extract_knowledge_full`).
        let patterns = axiograph_ingest_docs::machining_patterns();
        let mut page_facts: Vec<axiograph_ingest_docs::ExtractedFact> = Vec::new();
        for chunk in &extraction.chunks {
            page_facts.extend(axiograph_ingest_docs::extract_facts_from_chunk(
                chunk,
                &patterns,
                Some(domain),
            ));
        }
        let page_facts = axiograph_ingest_docs::aggregate_facts(page_facts);

        let proposals = axiograph_ingest_docs::proposals_from_extracted_facts_v1(
            &page_facts,
            Some(item.url.as_str().to_string()),
            Some(domain.to_string()),
        );

        all_facts.extend(page_facts);
        all_chunks.extend(extraction.chunks);
        all_proposals.extend(proposals);

        if crawl && item.depth < max_depth {
            for link in extract_links(&item.url, &html) {
                if should_enqueue_link(&item.url, &link) {
                    enqueue_url(&mut queue, &mut seen, link, item.depth + 1);
                }
            }
        }

        fetched += 1;
    }

    fs::write(&manifest_path, &manifest)?;

    // Aggregate facts only at the end (dedup).
    let facts = axiograph_ingest_docs::aggregate_facts(all_facts);

    let chunks_path = out_dir.join("chunks.json");
    let facts_path = out_dir.join("facts.json");
    let proposals_path = out_dir.join("proposals.json");

    fs::write(&chunks_path, serde_json::to_string_pretty(&all_chunks)?)?;
    fs::write(&facts_path, serde_json::to_string_pretty(&facts)?)?;

    let generated_at = now.to_string();
    let file = axiograph_ingest_docs::ProposalsFileV1 {
        version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
        generated_at,
        source: axiograph_ingest_docs::ProposalSourceV1 {
            source_type: "web".to_string(),
            locator: out_dir.to_string_lossy().to_string(),
        },
        schema_hint: Some("web".to_string()),
        proposals: all_proposals,
    };
    fs::write(&proposals_path, serde_json::to_string_pretty(&file)?)?;

    println!("  {} fetched_pages={fetched}", "→".yellow());
    println!("  {} {}", "→".cyan(), manifest_path.display());
    println!("  {} {}", "→".cyan(), chunks_path.display());
    println!("  {} {}", "→".cyan(), facts_path.display());
    println!("  {} {}", "→".cyan(), proposals_path.display());

    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
struct WebManifestEntryV1 {
    url: String,
    page_id: String,
    depth: usize,
    fetched_at_unix_secs: u64,
    status: Option<u16>,
    content_type: Option<String>,
    stored_path: Option<String>,
    error: Option<String>,
}

fn build_http_client(user_agent: &str, timeout_secs: u64) -> Result<Client> {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(user_agent).unwrap_or_else(|_| HeaderValue::from_static("axiograph")),
    );

    Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| anyhow!("failed to build http client: {e}"))
}

struct FetchResult {
    status: Option<u16>,
    content_type: Option<String>,
    body: String,
}

fn fetch_html(client: &Client, url: &Url, max_html_bytes: usize) -> Result<FetchResult> {
    let resp = client
        .get(url.clone())
        .send()
        .with_context(|| format!("failed to fetch {url}"))?;

    let status = Some(resp.status().as_u16());
    if !resp.status().is_success() {
        return Err(anyhow!("http status {}", resp.status()));
    }

    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if let Some(len) = resp.content_length() {
        if len as usize > max_html_bytes {
            return Err(anyhow!(
                "content-length {} exceeds --max-html-bytes {}",
                len,
                max_html_bytes
            ));
        }
    }

    let bytes = resp
        .bytes()
        .with_context(|| format!("failed to read body for {url}"))?;
    if bytes.len() > max_html_bytes {
        return Err(anyhow!(
            "body size {} exceeds --max-html-bytes {}",
            bytes.len(),
            max_html_bytes
        ));
    }

    let body = String::from_utf8_lossy(&bytes).to_string();
    Ok(FetchResult {
        status,
        content_type,
        body,
    })
}

fn parse_url_list(urls: &[String]) -> Result<Vec<Url>> {
    let mut out = Vec::new();
    for s in urls {
        let s = s.trim();
        if s.is_empty() {
            continue;
        }
        out.push(Url::parse(s).with_context(|| format!("invalid url: {s}"))?);
    }
    Ok(out)
}

fn read_urls_file(path: &Path) -> Result<Vec<Url>> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read urls file: {}", path.display()))?;
    let mut out = Vec::new();
    for line in text.lines() {
        let s = line.trim();
        if s.is_empty() || s.starts_with('#') {
            continue;
        }
        out.push(Url::parse(s).with_context(|| format!("invalid url in file: {s}"))?);
    }
    Ok(out)
}

fn enqueue_url(
    queue: &mut VecDeque<CrawlItem>,
    seen: &mut HashSet<String>,
    url: Url,
    depth: usize,
) {
    let key = url.as_str().to_string();
    if seen.insert(key) {
        queue.push_back(CrawlItem { url, depth });
    }
}

fn url_id(url: &Url) -> String {
    let digest = axiograph_dsl::digest::fnv1a64_digest_bytes(url.as_str().as_bytes());
    format!("{digest}")
}

fn html_to_markdown(html: &str) -> Result<String> {
    let conv = htmd::HtmlToMarkdown::builder().build();
    conv.convert(html)
        .map_err(|e| anyhow!("failed to convert html to markdown: {e}"))
}

fn strip_html_to_text(html: &str) -> String {
    // Conservative fallback: use `scraper` to extract visible-ish text.
    let doc = Html::parse_document(html);
    let selector = Selector::parse("body").unwrap();
    let Some(body) = doc.select(&selector).next() else {
        return String::new();
    };

    let mut out = String::new();
    for t in body.text() {
        let s = t.trim();
        if s.is_empty() {
            continue;
        }
        out.push_str(s);
        out.push('\n');
    }
    out
}

fn extract_links(base: &Url, html: &str) -> Vec<Url> {
    let mut out = Vec::new();
    let doc = Html::parse_document(html);
    let selector = match Selector::parse("a[href]") {
        Ok(s) => s,
        Err(_) => return out,
    };

    for a in doc.select(&selector) {
        let Some(href) = a.value().attr("href") else {
            continue;
        };
        let href = href.trim();
        if href.is_empty() {
            continue;
        }
        if href.starts_with('#')
            || href.starts_with("mailto:")
            || href.starts_with("javascript:")
            || href.starts_with("data:")
        {
            continue;
        }

        let url = match base.join(href) {
            Ok(u) => u,
            Err(_) => continue,
        };
        if url.scheme() != "http" && url.scheme() != "https" {
            continue;
        }
        out.push(url);
    }
    out
}

fn should_enqueue_link(page_url: &Url, link: &Url) -> bool {
    // Skip obvious non-document assets.
    let path = link.path().to_ascii_lowercase();
    for ext in [
        ".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp", ".pdf", ".zip", ".gz", ".tar", ".tgz",
        ".css", ".js", ".json",
    ] {
        if path.ends_with(ext) {
            return false;
        }
    }

    // Special-case Wikipedia: keep to /wiki/* and avoid special pages.
    if page_url
        .host_str()
        .map(|h| h.ends_with("wikipedia.org"))
        .unwrap_or(false)
    {
        let p = link.path();
        if !p.starts_with("/wiki/") {
            return false;
        }
        let rest = &p["/wiki/".len()..];
        if rest.contains(':') {
            return false;
        }
        return true;
    }

    true
}

fn robots_user_agent_token(user_agent: &str) -> String {
    // `robotstxt` itself extracts the matchable portion of the UA string.
    // Keep this as a hook in case we later want to override UA matching rules.
    user_agent.trim().to_string()
}

fn robots_allows_url(
    client: &Client,
    cache: &mut HashMap<String, String>,
    user_agent: &str,
    url: &Url,
) -> bool {
    let Some(host) = url.host_str() else {
        return true;
    };
    let scheme = url.scheme();
    let port = url.port();
    let key = match port {
        Some(p) => format!("{scheme}://{host}:{p}"),
        None => format!("{scheme}://{host}"),
    };

    let robots_body = cache
        .entry(key.clone())
        .or_insert_with(|| fetch_robots_txt(client, url).unwrap_or_else(|| "".to_string()));

    // Missing/empty robots.txt => allow.
    if robots_body.trim().is_empty() {
        return true;
    }

    let mut matcher = robotstxt::DefaultMatcher::default();
    matcher.one_agent_allowed_by_robots(robots_body, user_agent, url.as_str())
}

fn fetch_robots_txt(client: &Client, url: &Url) -> Option<String> {
    let host = url.host_str()?;
    let mut robots_url = url.clone();
    robots_url.set_path("/robots.txt");
    robots_url.set_query(None);
    robots_url.set_fragment(None);
    robots_url.set_host(Some(host)).ok()?;

    let resp = client.get(robots_url).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.text().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_to_markdown_smoke() {
        let md = html_to_markdown("<h1>Hello</h1>").expect("md");
        assert!(md.contains("Hello"));
    }

    #[test]
    fn extract_links_resolves_relative_urls() {
        let base = Url::parse("https://example.com/a/").unwrap();
        let html = r#"<a href="/b">B</a><a href="c">C</a>"#;
        let links = extract_links(&base, html);
        let out: HashSet<String> = links.iter().map(|u| u.as_str().to_string()).collect();
        assert!(out.contains("https://example.com/b"));
        assert!(out.contains("https://example.com/a/c"));
    }

    #[test]
    fn wikipedia_link_filter_skips_special_pages() {
        let page = Url::parse("https://en.wikipedia.org/wiki/Physics").unwrap();
        let ok = Url::parse("https://en.wikipedia.org/wiki/Category_theory").unwrap();
        let special = Url::parse("https://en.wikipedia.org/wiki/Special:Random").unwrap();
        assert!(should_enqueue_link(&page, &ok));
        assert!(!should_enqueue_link(&page, &special));
    }
}
