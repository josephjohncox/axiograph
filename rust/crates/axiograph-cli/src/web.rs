//! Web ingestion (scrape/crawl) helpers.
//!
//! This is **untrusted tooling** intended for discovery workflows:
//! - fetch pages (respectful defaults: rate limits, size caps),
//! - extract text/markdown from HTML,
//! - emit `EvidenceChunkBundleV1` chunks + extracted facts + `proposals.json` (Evidence/Proposals schema).
//!
//! This is NOT part of the trusted semantics kernel.

use anyhow::{anyhow, Context, Result};
use clap::Subcommand;
use colored::Colorize;
use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderMap, HeaderValue, LOCATION, USER_AGENT};
use scraper::{Html, Selector};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

const MAX_WEB_PAGES: usize = 1000;
const MAX_WEB_DEPTH: usize = 16;
const MAX_WEB_HTML_BYTES: usize = 4 * 1024 * 1024;
const MAX_WEB_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const MAX_WEB_URL_FILE_BYTES: usize = 1024 * 1024;
const MAX_ROBOTS_BYTES: usize = 256 * 1024;
const MAX_WEB_TIMEOUT_SECS: u64 = 300;
const MAX_WEB_DELAY_MS: u64 = 60_000;
const MAX_WEB_REDIRECTS: usize = 5;
const MAX_DNS_ANSWERS: usize = 16;
const MAX_DNS_CONCURRENCY: usize = 16;
const MAX_DNS_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_HOSTNAME_BYTES: usize = 253;
const MAX_WEB_HOST_RULES: usize = 256;
const MAX_WEB_USER_AGENT_BYTES: usize = 512;

#[derive(Subcommand)]
pub enum WebCommands {
    /// Fetch (and optionally crawl) web pages, then emit typed chunk evidence + `proposals.json`.
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

fn prepare_web_output_dir(out_dir: &Path, overwrite: bool) -> Result<()> {
    match fs::symlink_metadata(out_dir) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
                return Err(anyhow!(
                    "web output `{}` must be a real directory, not a symlink or special file",
                    out_dir.display()
                ));
            }
            const OWNED_NAMES: [&str; 5] = [
                "pages",
                "manifest.jsonl",
                "chunks.json",
                "facts.json",
                "proposals.json",
            ];
            let mut entries = Vec::with_capacity(OWNED_NAMES.len());
            for entry in fs::read_dir(out_dir)? {
                if entries.len() >= OWNED_NAMES.len() {
                    return Err(anyhow!(
                        "web output directory contains more than {} top-level artifacts",
                        OWNED_NAMES.len()
                    ));
                }
                entries.push(entry?);
            }
            if !entries.is_empty() && !overwrite {
                return Err(anyhow!(
                    "web output directory `{}` is not empty; pass --overwrite only for a prior Axiograph web-ingest directory",
                    out_dir.display()
                ));
            }
            if overwrite {
                entries.sort_by_key(|entry| entry.file_name());
                for entry in entries {
                    let name = entry.file_name();
                    let Some(name_text) = name.to_str() else {
                        return Err(anyhow!("web output contains a non-UTF-8 entry"));
                    };
                    if !OWNED_NAMES.contains(&name_text) {
                        return Err(anyhow!(
                            "refusing --overwrite because `{}` is not an Axiograph web-ingest artifact",
                            entry.path().display()
                        ));
                    }
                    let metadata = fs::symlink_metadata(entry.path())?;
                    if metadata.file_type().is_symlink() {
                        return Err(anyhow!(
                            "refusing --overwrite through symlink `{}`",
                            entry.path().display()
                        ));
                    }
                    if metadata.file_type().is_dir() {
                        if name_text != "pages" {
                            return Err(anyhow!(
                                "unexpected directory `{}` in web output",
                                entry.path().display()
                            ));
                        }
                        remove_web_pages_dir_bounded(&entry.path())?;
                    } else if metadata.file_type().is_file() {
                        fs::remove_file(entry.path())?;
                    } else {
                        return Err(anyhow!(
                            "refusing to remove special file `{}`",
                            entry.path().display()
                        ));
                    }
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(out_dir)?;
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn remove_web_pages_dir_bounded(pages_dir: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(pages_dir)?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        return Err(anyhow!("web pages artifact must be a real directory"));
    }
    let mut entries = 0_usize;
    for entry in fs::read_dir(pages_dir)? {
        entries = entries.saturating_add(1);
        if entries > MAX_WEB_PAGES {
            return Err(anyhow!("web pages artifact exceeds {MAX_WEB_PAGES} files"));
        }
        let entry = entry?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| anyhow!("web pages artifact contains a non-UTF-8 filename"))?;
        let page_id = name
            .strip_suffix(".html")
            .and_then(|value| value.strip_prefix("axi:object-blob:v2:sha256:"));
        if page_id.is_none_or(|hex| {
            hex.len() != 64
                || !hex
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        }) {
            return Err(anyhow!(
                "refusing --overwrite because `{name}` is not an Axiograph web page artifact"
            ));
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            return Err(anyhow!(
                "web pages artifact must contain regular files only"
            ));
        }
        fs::remove_file(entry.path())?;
    }
    fs::remove_dir(pages_dir)?;
    Ok(())
}

fn is_public_ipv4(address: Ipv4Addr) -> bool {
    let octets = address.octets();
    !(address.is_unspecified()
        || address.is_loopback()
        || address.is_private()
        || address.is_link_local()
        || address.is_multicast()
        || address == Ipv4Addr::BROADCAST
        || octets[0] == 0
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 0)
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
        || (octets[0] == 198 && (18..=19).contains(&octets[1]))
        || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
        || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
        || octets[0] >= 240)
}

fn is_public_ipv6(address: Ipv6Addr) -> bool {
    let segments = address.segments();
    if let Some(mapped) = address.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    !(address.is_unspecified()
        || address.is_loopback()
        || address.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8))
}

fn validate_web_url(url: &Url) -> Result<()> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(anyhow!("web URL scheme must be http or https"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(anyhow!("web URLs must not contain credentials"));
    }
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("web URL must contain a host"))?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
    {
        return Err(anyhow!("web URL host `{host}` is local or reserved"));
    }
    let ip_literal = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(&host);
    if let Ok(address) = ip_literal.parse::<IpAddr>() {
        let public = match address {
            IpAddr::V4(address) => is_public_ipv4(address),
            IpAddr::V6(address) => is_public_ipv6(address),
        };
        if !public {
            return Err(anyhow!(
                "web URL address `{address}` is not globally routable"
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
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
    if !(1..=MAX_WEB_PAGES).contains(&max_pages) {
        return Err(anyhow!("--max-pages must be in 1..={MAX_WEB_PAGES}"));
    }
    if max_depth > MAX_WEB_DEPTH {
        return Err(anyhow!("--max-depth must be <= {MAX_WEB_DEPTH}"));
    }
    if !(1..=MAX_WEB_HTML_BYTES).contains(&max_html_bytes) {
        return Err(anyhow!(
            "--max-html-bytes must be in 1..={MAX_WEB_HTML_BYTES}"
        ));
    }
    if !(1..=MAX_WEB_TIMEOUT_SECS).contains(&timeout_secs) {
        return Err(anyhow!(
            "--timeout-secs must be in 1..={MAX_WEB_TIMEOUT_SECS}"
        ));
    }
    if delay_ms > MAX_WEB_DELAY_MS {
        return Err(anyhow!("--delay-ms must be <= {MAX_WEB_DELAY_MS}"));
    }
    if allow_hosts.len() > MAX_WEB_HOST_RULES {
        return Err(anyhow!("--allow-host count exceeds {MAX_WEB_HOST_RULES}"));
    }
    if user_agent.is_empty() || user_agent.len() > MAX_WEB_USER_AGENT_BYTES {
        return Err(anyhow!(
            "--user-agent must be non-empty and at most {MAX_WEB_USER_AGENT_BYTES} bytes"
        ));
    }

    prepare_web_output_dir(out_dir, overwrite)?;

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
        enqueue_url(&mut queue, &mut seen, u, 0, max_pages);
    }
    for u in seed_urls {
        enqueue_url(&mut queue, &mut seen, u, 0, max_pages);
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
    let mut fetched_bytes = 0_usize;

    while let Some(item) = queue.pop_front() {
        if fetched >= max_pages {
            break;
        }
        if crawl && item.depth > max_depth {
            continue;
        }

        if let Err(error) = validate_web_url(&item.url) {
            return Err(error).with_context(|| format!("refused URL `{}`", item.url));
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
            Ok(r) => {
                fetched_bytes = fetched_bytes
                    .checked_add(r.body_bytes)
                    .ok_or_else(|| anyhow!("web ingest byte count overflow"))?;
                if fetched_bytes > MAX_WEB_TOTAL_BYTES {
                    return Err(anyhow!(
                        "web ingest exceeded {MAX_WEB_TOTAL_BYTES} downloaded bytes"
                    ));
                }
                (r.status, r.content_type, Some(r.body), None)
            }
            Err(e) => (None, None, None, Some(e.to_string())),
        };

        let page_id = url_id(&item.url);
        let mut stored_path = None;
        if store_html {
            if let Some(html) = &html_text {
                let path = pages_dir.join(format!("{page_id}.html"));
                crate::security::write_output_bounded(&path, html.as_bytes(), "CLI output")?;
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

        let doc_id = format!("web_{page_id}");
        let mut extraction = axiograph_ingest_docs::extract_markdown(&markdown, &doc_id)?;

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
            for link in extract_links(&item.url, &html, max_pages) {
                if should_enqueue_link(&item.url, &link) {
                    enqueue_url(&mut queue, &mut seen, link, item.depth + 1, max_pages);
                }
            }
        }

        fetched += 1;
    }

    crate::security::write_output_bounded(&manifest_path, &manifest, "CLI output")?;

    // Aggregate facts only at the end (dedup).
    let facts = axiograph_ingest_docs::aggregate_facts(all_facts);

    let chunks_path = out_dir.join("chunks.json");
    let facts_path = out_dir.join("facts.json");
    let proposals_path = out_dir.join("proposals.json");

    crate::security::write_output_bounded(
        &chunks_path,
        axiograph_ingest_docs::chunks_to_json_for_chunks(
            "web_crawl",
            out_dir.display().to_string(),
            all_chunks.clone(),
        )?,
        "CLI output",
    )?;
    crate::security::write_output_bounded(
        &facts_path,
        serde_json::to_string_pretty(&facts)?,
        "CLI output",
    )?;

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
    crate::security::write_output_bounded(
        &proposals_path,
        serde_json::to_string_pretty(&file)?,
        "CLI output",
    )?;

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

#[derive(Debug, Clone)]
struct HttpClientPolicy {
    user_agent: HeaderValue,
    timeout: Duration,
}

#[derive(Debug, Clone)]
pub(crate) struct PinnedPublicClient {
    client: Client,
    url: Url,
    addresses: Vec<SocketAddr>,
}

#[derive(Debug, Clone)]
pub(crate) struct PinnedLoopbackClient {
    client: Client,
    url: Url,
    addresses: Vec<SocketAddr>,
}

fn build_http_client(user_agent: &str, timeout_secs: u64) -> Result<HttpClientPolicy> {
    if timeout_secs == 0 || timeout_secs > MAX_WEB_TIMEOUT_SECS {
        return Err(anyhow!(
            "web timeout must be in 1..={MAX_WEB_TIMEOUT_SECS} seconds"
        ));
    }
    let user_agent = HeaderValue::from_str(user_agent)
        .map_err(|_| anyhow!("web user-agent is not a valid HTTP header"))?;
    Ok(HttpClientPolicy {
        user_agent,
        timeout: Duration::from_secs(timeout_secs),
    })
}

impl HttpClientPolicy {
    fn send(&self, url: &Url) -> Result<Response> {
        let mut current = url.clone();
        for redirect_count in 0..=MAX_WEB_REDIRECTS {
            validate_web_url(&current)?;
            let response = self.send_once(&current)?;
            if !response.status().is_redirection() {
                return Ok(response);
            }
            if redirect_count == MAX_WEB_REDIRECTS {
                return Err(anyhow!("redirect count exceeds {MAX_WEB_REDIRECTS}"));
            }
            let location = response
                .headers()
                .get(LOCATION)
                .ok_or_else(|| anyhow!("redirect response omitted Location"))?
                .to_str()
                .context("redirect Location is not valid ASCII/UTF-8")?;
            current = validated_redirect_target(&current, location)?;
        }
        Err(anyhow!("redirect count exceeds {MAX_WEB_REDIRECTS}"))
    }

    fn send_once(&self, url: &Url) -> Result<Response> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, self.user_agent.clone());
        let client = PinnedPublicClient::new(url, headers, self.timeout)?;
        let response = client
            .get()
            .send()
            .with_context(|| format!("failed to fetch {url}"))?;
        client.verify_response(response)
    }
}

impl PinnedLoopbackClient {
    pub(crate) fn new(url: &Url, timeout: Duration) -> Result<Self> {
        if timeout.is_zero() || timeout > Duration::from_secs(MAX_WEB_TIMEOUT_SECS) {
            return Err(anyhow!(
                "loopback HTTP timeout must be in 1..={MAX_WEB_TIMEOUT_SECS} seconds"
            ));
        }
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(anyhow!("loopback endpoint URL is not canonical HTTP(S)"));
        }
        let host = url
            .host()
            .ok_or_else(|| anyhow!("loopback endpoint must include a host"))?;
        let port = url
            .port_or_known_default()
            .ok_or_else(|| anyhow!("loopback endpoint has no known port"))?;
        let domain = host.to_string();
        if domain.len() > MAX_HOSTNAME_BYTES {
            return Err(anyhow!("loopback hostname exceeds byte limit"));
        }
        let addresses = match host {
            url::Host::Ipv4(ip) => vec![SocketAddr::new(IpAddr::V4(ip), port)],
            url::Host::Ipv6(ip) => vec![SocketAddr::new(IpAddr::V6(ip), port)],
            url::Host::Domain(name) if name.eq_ignore_ascii_case("localhost") => {
                resolve_addresses_bounded(&domain, port, timeout)?
            }
            url::Host::Domain(_) => {
                return Err(anyhow!(
                    "local endpoint host must be localhost or a loopback IP"
                ))
            }
        };
        if addresses.is_empty() || addresses.len() > MAX_DNS_ANSWERS {
            return Err(anyhow!("loopback DNS answer count is invalid"));
        }
        if addresses
            .iter()
            .any(|address| !normalize_ip(address.ip()).is_loopback())
        {
            return Err(anyhow!(
                "loopback endpoint resolved to a non-loopback address"
            ));
        }
        let client = Client::builder()
            .timeout(timeout)
            .connect_timeout(timeout.min(Duration::from_secs(10)))
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .resolve_to_addrs(&domain, &addresses)
            .build()
            .map_err(|error| anyhow!("failed to build loopback HTTP client: {error}"))?;
        Ok(Self {
            client,
            url: url.clone(),
            addresses,
        })
    }

    pub(crate) fn post(&self) -> reqwest::blocking::RequestBuilder {
        self.client.post(self.url.clone())
    }

    pub(crate) fn verify_response(&self, response: Response) -> Result<Response> {
        let remote = response
            .remote_addr()
            .ok_or_else(|| anyhow!("HTTP transport did not report a remote address"))?;
        let remote_ip = normalize_ip(remote.ip());
        if !remote_ip.is_loopback()
            || !self
                .addresses
                .iter()
                .any(|address| normalize_ip(address.ip()) == remote_ip)
        {
            return Err(anyhow!(
                "local HTTP remote address {remote_ip} was not a pinned loopback answer"
            ));
        }
        Ok(response)
    }
}

impl PinnedPublicClient {
    pub(crate) fn new(url: &Url, headers: HeaderMap, timeout: Duration) -> Result<Self> {
        if timeout.is_zero() || timeout > Duration::from_secs(MAX_WEB_TIMEOUT_SECS) {
            return Err(anyhow!(
                "public HTTP timeout must be in 1..={MAX_WEB_TIMEOUT_SECS} seconds"
            ));
        }
        validate_web_url(url)?;
        let (domain, addresses) = resolve_public_addresses(url, timeout)?;
        let client = Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .connect_timeout(timeout.min(Duration::from_secs(10)))
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .resolve_to_addrs(&domain, &addresses)
            .build()
            .map_err(|error| anyhow!("failed to build pinned HTTP client: {error}"))?;
        Ok(Self {
            client,
            url: url.clone(),
            addresses,
        })
    }

    pub(crate) fn get(&self) -> reqwest::blocking::RequestBuilder {
        self.client.get(self.url.clone())
    }

    pub(crate) fn post(&self) -> reqwest::blocking::RequestBuilder {
        self.client.post(self.url.clone())
    }

    pub(crate) fn verify_response(&self, response: Response) -> Result<Response> {
        let remote = response
            .remote_addr()
            .ok_or_else(|| anyhow!("HTTP transport did not report a remote address"))?;
        validate_pinned_remote(remote, &self.addresses)?;
        Ok(response)
    }
}

#[derive(Debug)]
struct DnsLimiter {
    active: AtomicUsize,
}

impl DnsLimiter {
    const fn new() -> Self {
        Self {
            active: AtomicUsize::new(0),
        }
    }

    fn acquire(&self) -> Result<DnsPermit<'_>> {
        loop {
            let current = self.active.load(Ordering::Acquire);
            if current >= MAX_DNS_CONCURRENCY {
                return Err(anyhow!("DNS concurrency exceeds {MAX_DNS_CONCURRENCY}"));
            }
            if self
                .active
                .compare_exchange_weak(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Ok(DnsPermit { limiter: self });
            }
        }
    }
}

#[derive(Debug)]
struct DnsPermit<'a> {
    limiter: &'a DnsLimiter,
}

impl Drop for DnsPermit<'_> {
    fn drop(&mut self) {
        self.limiter.active.fetch_sub(1, Ordering::AcqRel);
    }
}

static DNS_LIMITER: DnsLimiter = DnsLimiter::new();

fn validated_redirect_target(current: &Url, location: &str) -> Result<Url> {
    let target = current
        .join(location)
        .with_context(|| format!("invalid redirect target `{location}`"))?;
    validate_web_url(&target)?;
    Ok(target)
}

fn validate_pinned_remote(remote: SocketAddr, addresses: &[SocketAddr]) -> Result<()> {
    let remote_ip = normalize_ip(remote.ip());
    if !is_public_ip(remote_ip)
        || !addresses
            .iter()
            .any(|address| normalize_ip(address.ip()) == remote_ip)
    {
        return Err(anyhow!(
            "HTTP remote address {remote_ip} was not one of the pinned public DNS answers"
        ));
    }
    Ok(())
}

fn resolve_addresses_bounded(
    domain: &str,
    port: u16,
    request_timeout: Duration,
) -> Result<Vec<SocketAddr>> {
    let _permit = DNS_LIMITER.acquire()?;
    let lookup_host = domain.to_string();
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let result = (lookup_host.as_str(), port)
            .to_socket_addrs()
            .map(|addresses| addresses.take(MAX_DNS_ANSWERS + 1).collect::<Vec<_>>())
            .map_err(|error| error.to_string());
        let _ = sender.send(result);
        drop(_permit);
    });
    receiver
        .recv_timeout(request_timeout.min(MAX_DNS_TIMEOUT))
        .map_err(|_| anyhow!("DNS lookup timed out or failed"))?
        .map_err(|error| anyhow!("DNS lookup failed: {error}"))
}

pub(crate) fn resolve_public_addresses(
    url: &Url,
    request_timeout: Duration,
) -> Result<(String, Vec<SocketAddr>)> {
    let host = url
        .host()
        .ok_or_else(|| anyhow!("web URL must include a host"))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| anyhow!("web URL has no known port"))?;
    let domain = host.to_string();
    if domain.len() > MAX_HOSTNAME_BYTES {
        return Err(anyhow!("web hostname exceeds {MAX_HOSTNAME_BYTES} bytes"));
    }

    let addresses = match host {
        url::Host::Ipv4(ip) => vec![SocketAddr::new(IpAddr::V4(ip), port)],
        url::Host::Ipv6(ip) => vec![SocketAddr::new(IpAddr::V6(ip), port)],
        url::Host::Domain(_) => resolve_addresses_bounded(&domain, port, request_timeout)?,
    };

    if addresses.is_empty() {
        return Err(anyhow!("DNS lookup returned no addresses"));
    }
    if addresses.len() > MAX_DNS_ANSWERS {
        return Err(anyhow!("DNS answer count exceeds {MAX_DNS_ANSWERS}"));
    }
    for address in &addresses {
        if !is_public_ip(normalize_ip(address.ip())) {
            return Err(anyhow!(
                "DNS answer {} is local, reserved, or otherwise non-public",
                address.ip()
            ));
        }
    }
    Ok((domain, addresses))
}

fn normalize_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(ip) => ip
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(ip)),
        ip => ip,
    }
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => is_public_ipv6(ip),
    }
}

struct FetchResult {
    status: Option<u16>,
    content_type: Option<String>,
    body: String,
    body_bytes: usize,
}

fn fetch_html(client: &HttpClientPolicy, url: &Url, max_html_bytes: usize) -> Result<FetchResult> {
    let resp = client.send(url)?;

    let status = Some(resp.status().as_u16());
    if !resp.status().is_success() {
        return Err(anyhow!("http status {}", resp.status()));
    }

    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let bytes = crate::security::read_blocking_response_bounded(
        resp,
        max_html_bytes,
        &format!("web response body for {url}"),
    )?;
    let body_bytes = bytes.len();
    let body = String::from_utf8_lossy(&bytes).to_string();
    Ok(FetchResult {
        status,
        content_type,
        body,
        body_bytes,
    })
}

fn parse_url_list(urls: &[String]) -> Result<Vec<Url>> {
    if urls.len() > MAX_WEB_PAGES {
        return Err(anyhow!("URL input count exceeds {MAX_WEB_PAGES}"));
    }
    let mut out = Vec::new();
    for s in urls {
        let s = s.trim();
        if s.is_empty() {
            continue;
        }
        let url = Url::parse(s).with_context(|| format!("invalid url: {s}"))?;
        validate_web_url(&url)?;
        out.push(url);
    }
    Ok(out)
}

fn read_urls_file(path: &Path) -> Result<Vec<Url>> {
    let text =
        crate::security::read_utf8_file_bounded(path, MAX_WEB_URL_FILE_BYTES, "web URL list")?;
    let mut out = Vec::new();
    for line in text.lines() {
        let s = line.trim();
        if s.is_empty() || s.starts_with('#') {
            continue;
        }
        let url = Url::parse(s).with_context(|| format!("invalid url in file: {s}"))?;
        validate_web_url(&url)?;
        out.push(url);
        if out.len() > MAX_WEB_PAGES {
            return Err(anyhow!("URL file count exceeds {MAX_WEB_PAGES}"));
        }
    }
    Ok(out)
}

fn enqueue_url(
    queue: &mut VecDeque<CrawlItem>,
    seen: &mut HashSet<String>,
    url: Url,
    depth: usize,
    max_urls: usize,
) {
    if seen.len() >= max_urls {
        return;
    }
    let key = url.as_str().to_string();
    if seen.insert(key) {
        queue.push_back(CrawlItem { url, depth });
    }
}

fn url_id(url: &Url) -> String {
    let digest = axiograph_kernel::object_blob_digest_v2(url.as_str().as_bytes());
    digest.to_string()
}

fn html_to_markdown(html: &str) -> Result<String> {
    let conv = htmd::HtmlToMarkdown::builder().build();
    conv.convert(html)
        .map_err(|e| anyhow!("failed to convert html to markdown: {e}"))
}

fn strip_html_to_text(html: &str) -> String {
    // Conservative fallback: use `scraper` to extract visible-ish text.
    let doc = Html::parse_document(html);
    let Ok(selector) = Selector::parse("body") else {
        return String::new();
    };
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

fn extract_links(base: &Url, html: &str, limit: usize) -> Vec<Url> {
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
        if out.len() >= limit {
            break;
        }
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
    client: &HttpClientPolicy,
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
        .or_insert_with(|| fetch_robots_txt(client, url).unwrap_or_default());

    // Missing/empty robots.txt => allow.
    if robots_body.trim().is_empty() {
        return true;
    }

    let mut matcher = robotstxt::DefaultMatcher::default();
    matcher.one_agent_allowed_by_robots(robots_body, user_agent, url.as_str())
}

fn fetch_robots_txt(client: &HttpClientPolicy, url: &Url) -> Option<String> {
    let host = url.host_str()?;
    let mut robots_url = url.clone();
    robots_url.set_path("/robots.txt");
    robots_url.set_query(None);
    robots_url.set_fragment(None);
    robots_url.set_host(Some(host)).ok()?;

    let resp = client.send(&robots_url).ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = crate::security::read_blocking_response_bounded(
        resp,
        MAX_ROBOTS_BYTES,
        "robots.txt response",
    )
    .ok()?;
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_to_markdown_regression() {
        let md = html_to_markdown("<h1>Hello</h1>").expect("md");
        assert!(md.contains("Hello"));
    }

    #[test]
    fn extract_links_resolves_relative_urls() {
        let base = Url::parse("https://example.com/a/").unwrap();
        let html = r#"<a href="/b">B</a><a href="c">C</a>"#;
        let links = extract_links(&base, html, 16);
        let out: HashSet<String> = links.iter().map(|u| u.as_str().to_string()).collect();
        assert!(out.contains("https://example.com/b"));
        assert!(out.contains("https://example.com/a/c"));
    }

    #[test]
    fn pinned_client_rejects_local_target_before_connecting() -> Result<()> {
        let client = build_http_client("axiograph-test", 2)?;
        let url = Url::parse("http://127.0.0.1:9/")?;
        let error = client
            .send(&url)
            .expect_err("local target must reject before connection");
        assert!(error.to_string().contains("globally routable"));
        Ok(())
    }

    #[test]
    fn web_url_policy_rejects_local_and_reserved_targets() {
        for value in [
            "http://127.0.0.1/",
            "http://169.254.169.254/latest/meta-data/",
            "http://[::1]/",
            "https://service.internal/",
            "file:///etc/passwd",
            "https://user:password@example.com/",
        ] {
            let url = Url::parse(value).expect("parse adversarial URL");
            assert!(validate_web_url(&url).is_err(), "accepted {value}");
        }
        assert!(validate_web_url(&Url::parse("https://example.com/").unwrap()).is_ok());
    }

    #[test]
    fn redirect_revalidation_rejects_private_target() {
        let current = Url::parse("https://example.com/start").unwrap();
        assert!(validated_redirect_target(&current, "http://169.254.169.254/metadata").is_err());
        assert!(validated_redirect_target(&current, "/next").is_ok());
    }

    #[test]
    fn connected_peer_must_match_pinned_public_dns_answer() {
        let pinned = ["1.1.1.1:443".parse::<SocketAddr>().unwrap()];
        assert!(validate_pinned_remote("1.1.1.1:443".parse().unwrap(), &pinned).is_ok());
        assert!(validate_pinned_remote("8.8.8.8:443".parse().unwrap(), &pinned).is_err());
        assert!(validate_pinned_remote("127.0.0.1:443".parse().unwrap(), &pinned).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn web_output_refuses_symlink_directory() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let target = tempfile::tempdir()?;
        let link = directory.path().join("output");
        std::os::unix::fs::symlink(target.path(), &link)?;
        let error =
            prepare_web_output_dir(&link, true).expect_err("symlink output directory must reject");
        assert!(error.to_string().contains("not a symlink"));
        Ok(())
    }

    #[test]
    fn web_overwrite_is_bounded_and_deletes_only_canonical_flat_page_artifacts() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let output = directory.path().join("output");
        fs::create_dir(&output)?;
        for index in 0..6 {
            fs::write(output.join(format!("unknown-{index}")), b"x")?;
        }
        let error = prepare_web_output_dir(&output, true)
            .expect_err("excess top-level output entries must reject");
        assert!(error.to_string().contains("more than 5"));

        let bounded = directory.path().join("bounded");
        let pages = bounded.join("pages");
        fs::create_dir_all(&pages)?;
        fs::create_dir(pages.join("nested"))?;
        let error = prepare_web_output_dir(&bounded, true)
            .expect_err("nested web page artifacts must reject");
        assert!(error
            .to_string()
            .contains("not an Axiograph web page artifact"));
        assert!(pages.join("nested").is_dir());

        let valid = directory.path().join("valid");
        let valid_pages = valid.join("pages");
        fs::create_dir_all(&valid_pages)?;
        let page_id = axiograph_kernel::object_blob_digest_v2(b"https://example.com/");
        fs::write(valid_pages.join(format!("{page_id}.html")), b"page")?;
        fs::write(valid.join("manifest.jsonl"), b"{}\n")?;
        prepare_web_output_dir(&valid, true)?;
        assert!(fs::read_dir(&valid)?.next().is_none());
        Ok(())
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
