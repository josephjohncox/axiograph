//! Axiograph CLI
//!
//! Unified command-line interface for:
//! - Validating canonical `.axi` modules (`axi_v1`)
//! - Ingesting sources into `proposals.json` (Evidence/Proposals schema)
//! - Promoting proposals into candidate domain `.axi` modules (explicit, reviewable)
//! - Managing PathDB snapshots (`.axpd` ↔ `.axi`)

use anyhow::{anyhow, Result};
use clap::{Args, Parser, Subcommand};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

mod accepted_plane;
mod analyze;
mod axi_fmt;
mod axql;
mod competency_questions;
mod db_server;
mod doc_chunks;
mod embeddings;
mod github;
mod llm;
mod nlq;
mod pathdb_wal;
mod perf;
mod profiling;
mod proposal_gen;
mod proposals_import;
mod proposals_validate;
mod proto;
mod query_ir;
mod quality;
mod relation_resolution;
mod repl;
mod schema_discovery;
mod sqlish;
mod store_sync;
mod synthetic_pathdb;
mod viz;
mod web;
mod world_model;

#[derive(Parser)]
#[command(name = "axiograph")]
#[command(
    author,
    version,
    about = "Axiograph: Dependently typed ontology language"
)]
struct Cli {
    #[command(flatten)]
    profile: profiling::ProfileArgs,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Ingest sources (docs/SQL/JSON/RDF/Proto/Web/Repo) into `proposals.json` (+ optional `chunks.json`).
    ///
    /// This is the preferred, "clean" ingestion entrypoint. Older top-level
    /// ingestion commands still exist for compatibility but are hidden from help.
    Ingest {
        #[command(subcommand)]
        command: IngestCommands,
    },

    /// Check/lint canonical `.axi` modules and `.axpd` snapshots.
    ///
    /// This is the preferred, "clean" entrypoint for:
    /// - Rust-side `.axi` validation, and
    /// - practical quality/lint reports.
    Check {
        #[command(subcommand)]
        command: CheckCommands,
    },

    /// Emit certificates (Rust computes, Lean verifies).
    ///
    /// Certificates are untrusted proof objects emitted by the Rust engine
    /// and checked by the Lean trusted checker (`axiograph_verify`).
    Cert {
        #[command(subcommand)]
        command: CertCommands,
    },

    /// Tooling commands (visualization, analysis, perf harnesses).
    Tools {
        #[command(subcommand)]
        command: ToolsCommands,
    },

    /// Database commands (snapshot store + PathDB snapshots/WAL).
    ///
    /// This is the preferred, "clean" entrypoint for:
    /// - accepted-plane management (canonical `.axi` snapshots), and
    /// - PathDB (`.axpd`) import/export and WAL-based overlays.
    Db {
        #[command(subcommand)]
        command: DbCommands,
    },

    /// Ingest SQL DDL → `proposals.json`
    #[command(hide = true)]
    Sql {
        /// Input SQL file
        input: PathBuf,
        /// Output proposals JSON (Evidence/Proposals schema)
        #[arg(short, long)]
        out: PathBuf,
    },

    /// Ingest document (text, markdown)
    #[command(hide = true)]
    Doc {
        /// Input document
        input: PathBuf,
        /// Output proposals JSON (Evidence/Proposals schema)
        #[arg(short, long)]
        out: PathBuf,
        /// Output chunks JSON (for RAG)
        #[arg(long)]
        chunks: Option<PathBuf>,
        /// Output extracted facts JSON
        #[arg(long)]
        facts: Option<PathBuf>,
        /// Treat as machining knowledge
        #[arg(long)]
        machining: bool,
        /// Domain for fact extraction (default: general)
        #[arg(long, default_value = "general")]
        domain: String,
    },

    /// Ingest conversation transcript
    #[command(hide = true)]
    Conversation {
        /// Input transcript file
        input: PathBuf,
        /// Output proposals JSON (Evidence/Proposals schema)
        #[arg(short, long)]
        out: PathBuf,
        /// Output chunks JSON
        #[arg(long)]
        chunks: Option<PathBuf>,
        /// Output extracted facts JSON
        #[arg(long)]
        facts: Option<PathBuf>,
        /// Format: slack, meeting
        #[arg(long, default_value = "slack")]
        format: String,
    },

    /// Ingest Confluence HTML export
    #[command(hide = true)]
    Confluence {
        /// Input HTML file
        input: PathBuf,
        /// Output proposals JSON (Evidence/Proposals schema)
        #[arg(short, long)]
        out: PathBuf,
        /// Confluence space name
        #[arg(long, default_value = "DOCS")]
        space: String,
        /// Output chunks JSON
        #[arg(long)]
        chunks: Option<PathBuf>,
        /// Output extracted facts JSON
        #[arg(long)]
        facts: Option<PathBuf>,
    },

    /// Ingest JSON data → `proposals.json`
    #[command(hide = true)]
    Json {
        /// Input JSON file
        input: PathBuf,
        /// Output proposals JSON (Evidence/Proposals schema)
        #[arg(short, long)]
        out: PathBuf,
    },

    /// Ingest recommended readings (BibTeX or markdown list)
    #[command(hide = true)]
    Readings {
        /// Input file (BibTeX or markdown)
        input: PathBuf,
        /// Output proposals JSON (Evidence/Proposals schema)
        #[arg(short, long)]
        out: PathBuf,
        /// Output chunks JSON
        #[arg(long)]
        chunks: Option<PathBuf>,
        /// Format: bibtex, markdown
        #[arg(long, default_value = "markdown")]
        format: String,
    },

    /// Convert PathDB snapshots between `.axpd` and `.axi` (export schema `PathDBExportV1`)
    #[command(hide = true)]
    Pathdb {
        #[command(subcommand)]
        command: PathdbCommands,
    },

    /// Validate an .axi file
    #[command(hide = true)]
    Validate {
        /// Input .axi file
        input: PathBuf,
    },

    /// Index a repository / codebase into chunks + lightweight graph edges
    #[command(hide = true)]
    Repo {
        #[command(subcommand)]
        command: RepoCommands,
    },

    /// Import a GitHub repo (or local repo path) into merged `proposals.json` + `chunks.json`
    #[command(hide = true)]
    Github {
        #[command(subcommand)]
        command: github::GithubCommands,
    },

    /// Scrape/crawl web pages into `chunks.json` + `proposals.json` (discovery tooling)
    #[command(hide = true)]
    Web {
        #[command(subcommand)]
        command: web::WebCommands,
    },

    /// Run discovery tasks over ingestion artifacts (chunks/facts/edges)
    Discover {
        #[command(subcommand)]
        command: DiscoverCommands,
    },

    /// Manage the accepted/canonical `.axi` plane (append-only log + snapshot ids).
    #[command(hide = true)]
    Accept {
        #[command(subcommand)]
        command: AcceptedCommands,
    },

    /// Ingest a directory of heterogeneous sources (docs, SQL, RDF/OWL, JSON, Confluence)
    #[command(hide = true)]
    IngestDir {
        /// Root directory to ingest
        root: PathBuf,
        /// Output directory for ingestion artifacts
        #[arg(short, long, default_value = "build/ingest")]
        out_dir: PathBuf,
        /// Confluence space name (used for `.html` ingestion)
        #[arg(long, default_value = "DOCS")]
        confluence_space: String,
        /// Domain for document fact extraction
        #[arg(long, default_value = "general")]
        domain: String,
        /// Output aggregated chunks JSON (for RAG)
        #[arg(long)]
        chunks: Option<PathBuf>,
        /// Output aggregated extracted-facts JSON
        #[arg(long)]
        facts: Option<PathBuf>,
        /// Output generic proposals JSON (Evidence/Proposals schema)
        #[arg(long)]
        proposals: Option<PathBuf>,
        /// Maximum file size to ingest (bytes)
        #[arg(long, default_value_t = 524288)]
        max_file_bytes: u64,
        /// Maximum number of files to ingest
        #[arg(long, default_value_t = 50000)]
        max_files: usize,
    },

    /// Performance harnesses (synthetic ingestion/query timings).
    #[command(hide = true)]
    Perf {
        #[command(subcommand)]
        command: perf::PerfCommands,
    },

    /// Interactive REPL for PathDB snapshots and reversible `.axi` exports.
    Repl {
        /// Optional `.axpd` file to load on startup.
        #[arg(long)]
        axpd: Option<PathBuf>,
        /// Run a non-interactive REPL script (one command per line). Use `-` to read from stdin.
        #[arg(long)]
        script: Option<PathBuf>,
        /// Run one REPL command (may be repeated).
        #[arg(long, value_name = "CMD")]
        cmd: Vec<String>,
        /// Continue executing script/commands after a failure (default is fail-fast).
        #[arg(long)]
        continue_on_error: bool,
        /// Do not echo commands while running a script / `--cmd`.
        #[arg(long)]
        quiet: bool,
    },

    /// Run an AxQL/SQL-ish query over a `PathDBExportV1` `.axi` snapshot and emit a certificate.
    ///
    /// This is a helper for “Rust computes, Lean verifies” end-to-end checks:
    /// - the `.axi` snapshot is the canonical anchor (digest),
    /// - the query runs over the imported PathDB,
    /// - and Rust emits a query-result certificate anchored to the snapshot digest:
    ///   - `query_result_v1` for conjunctive queries
    ///   - `query_result_v2` for disjunctions (`or`, i.e. UCQs)
    #[command(hide = true)]
    QueryCert {
        /// Input `.axi` file.
        ///
        /// This may be either:
        /// - a `PathDBExportV1` snapshot export (reversible `.axi` export), or
        /// - a canonical `axi_v1` module (schema/theory/instance).
        ///
        /// If the input is a canonical module, you must pass `--anchor-out` so
        /// this command can write a derived `PathDBExportV1` snapshot anchor for
        /// `axiograph_verify`.
        input: PathBuf,

        /// Query language: `axql` or `sql`.
        #[arg(long, default_value = "axql")]
        lang: String,

        /// Query text (quote it in your shell).
        query: String,

        /// Write certificate JSON to this path (defaults to stdout).
        #[arg(short, long)]
        out: Option<PathBuf>,

        /// Write the derived `PathDBExportV1` anchor snapshot to this `.axi` path.
        ///
        /// Required when `input` is a canonical module (because the Lean verifier
        /// currently anchors query-result certificates to snapshot exports).
        #[arg(long)]
        anchor_out: Option<PathBuf>,
    },

    /// Typecheck a canonical `.axi` module and emit an `axi_well_typed_v1` certificate.
    ///
    /// This is the smallest "trusted gate" for the canonical input language:
    /// Rust emits a certificate envelope anchored to the input module digest,
    /// and Lean re-parses + re-checks the module.
    #[command(hide = true)]
    TypecheckCert {
        /// Input `.axi` file (canonical `axi_v1` schema/theory/instance module).
        input: PathBuf,

        /// Write certificate JSON to this path (defaults to stdout).
        #[arg(short, long)]
        out: Option<PathBuf>,
    },

    /// Check a conservative subset of theory constraints and emit an `axi_constraints_ok_v1` certificate.
    ///
    /// This is intended as a future “promotion gate” for canonical `.axi` inputs:
    /// Rust emits an envelope anchored to the input module digest, and Lean re-parses +
    /// re-checks the same constraint subset.
    #[command(hide = true)]
    ConstraintsCert {
        /// Input `.axi` file (canonical `axi_v1` schema/theory/instance module).
        input: PathBuf,

        /// Write certificate JSON to this path (defaults to stdout).
        #[arg(short, long)]
        out: Option<PathBuf>,
    },

    /// Protobuf / gRPC ingestion (`buf build` → descriptor set → proposals).
    #[command(hide = true)]
    Proto {
        #[command(subcommand)]
        command: proto::ProtoCommands,
    },

    /// Visualize a `.axpd` snapshot or imported `.axi` module as a neighborhood graph.
    ///
    /// Output formats:
    /// - `dot`: Graphviz DOT (use `dot -Tsvg graph.dot -o graph.svg`)
    /// - `html`: self-contained offline explorer
    /// - `json`: raw graph JSON for custom frontends
    #[command(hide = true)]
    Viz(VizArgs),

    /// Tooling-focused analysis commands (untrusted / evidence-plane friendly).
    #[command(hide = true)]
    Analyze {
        #[command(subcommand)]
        command: analyze::AnalyzeCommands,
    },

    /// Lint/quality checks for `.axi` modules and `.axpd` snapshots.
    ///
    /// This is a practical ontology-engineering helper. It produces a structured
    /// report (JSON) and exits non-zero when errors are found.
    #[command(hide = true)]
    Quality {
        /// Input `.axpd` or `.axi` file.
        input: PathBuf,
        /// Output report path (defaults to stdout).
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Output format: json|text
        #[arg(long, default_value = "text")]
        format: String,
        /// Profile: fast|strict
        #[arg(long, default_value = "fast")]
        profile: String,
        /// Plane selection: data|meta|both
        #[arg(long, default_value = "both")]
        plane: String,
        /// Do not fail the process even if errors are found (always exit 0).
        #[arg(long)]
        no_fail: bool,
    },
}

#[derive(Subcommand)]
enum CheckCommands {
    /// Validate a canonical `.axi` module (parse + typecheck).
    Validate {
        /// Input `.axi` file.
        input: PathBuf,
    },

    /// Format a canonical `.axi` module (surgically; preserves comments).
    ///
    /// Today this focuses on canonicalizing `constraint ...` syntax so
    /// unknown/dialect-ish constraint forms are fixable now that the
    /// certificate/promote gates fail closed.
    Fmt {
        /// Input `.axi` file.
        input: PathBuf,
        /// Write formatted output to this file (defaults to stdout).
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Overwrite the input file in-place.
        #[arg(long)]
        write: bool,
    },

    /// Lint/quality checks for `.axi` modules and `.axpd` snapshots.
    ///
    /// This is a practical ontology-engineering helper. It produces a structured
    /// report (JSON/text) and exits non-zero when errors are found (unless
    /// `--no-fail` is set).
    Quality {
        /// Input `.axpd` or `.axi` file.
        input: PathBuf,
        /// Output report path (defaults to stdout).
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Output format: json|text
        #[arg(long, default_value = "text")]
        format: String,
        /// Profile: fast|strict
        #[arg(long, default_value = "fast")]
        profile: String,
        /// Plane selection: data|meta|both
        #[arg(long, default_value = "both")]
        plane: String,
        /// Do not fail the process even if errors are found (always exit 0).
        #[arg(long)]
        no_fail: bool,
    },
}

#[derive(Subcommand)]
enum ToolsCommands {
    /// Visualize a `.axpd` snapshot or imported `.axi` module as a neighborhood graph.
    Viz(VizArgs),

    /// Tooling-focused analysis commands (untrusted / evidence-plane friendly).
    Analyze {
        #[command(subcommand)]
        command: analyze::AnalyzeCommands,
    },

    /// Performance harnesses (synthetic ingestion/query timings).
    Perf {
        #[command(subcommand)]
        command: perf::PerfCommands,
    },
}

#[derive(Subcommand)]
enum DbCommands {
    /// Manage the accepted/canonical `.axi` plane (append-only log + snapshot ids).
    Accept {
        #[command(subcommand)]
        command: AcceptedCommands,
    },

    /// Convert PathDB snapshots between `.axpd` and `.axi` and import chunk overlays.
    Pathdb {
        #[command(subcommand)]
        command: PathdbCommands,
    },

    /// Run a database server process over a loaded snapshot.
    ///
    /// This keeps a `.axpd` snapshot loaded in memory and serves:
    /// - `/healthz`
    /// - `/status`
    /// - `/snapshots` (store-backed only; list snapshots for time-travel)
    /// - `/query` (AxQL)
    /// - `/viz` (HTML), `/viz.json` (graph JSON), `/viz.dot` (Graphviz DOT)
    ///
    /// Time travel:
    /// - `GET /viz?...&snapshot=<id>` renders an older snapshot (store-backed only).
    ///
    /// In `--role master` mode, the server can also accept write operations
    /// that mutate the snapshot store (accepted plane + PathDB WAL). Treat
    /// this as an **untrusted** runtime surface: trusted correctness remains
    /// certificate checking in Lean.
    Serve(DbServeArgs),
}

#[derive(Args, Debug, Clone)]
struct DbServeArgs {
    /// Listen address (use `127.0.0.1:0` to auto-pick a free port).
    #[arg(long, default_value = "127.0.0.1:7878")]
    listen: std::net::SocketAddr,

    /// Role: `standalone` (read-only), `master` (read/write), or `replica` (read-only + watch).
    #[arg(long, default_value = "standalone")]
    role: String,

    /// Load a `.axpd` snapshot directly.
    #[arg(long)]
    axpd: Option<PathBuf>,

    /// Load from a snapshot store directory (accepted plane + PathDB WAL).
    ///
    /// Use with `--layer` + `--snapshot`.
    #[arg(long)]
    dir: Option<PathBuf>,

    /// Which store layer to serve: `pathdb` (WAL head) or `accepted` (canonical head).
    #[arg(long, default_value = "pathdb")]
    layer: String,

    /// Snapshot id (or `head`/`latest`) when loading from `--dir`.
    #[arg(long, default_value = "head")]
    snapshot: String,

    /// Reload when `HEAD` changes (polling).
    #[arg(long)]
    watch_head: bool,

    /// Polling interval for `--watch-head`.
    #[arg(long, default_value_t = 2)]
    poll_interval_secs: u64,

    /// Optional admin token required for write endpoints (recommended for `--role master`).
    #[arg(long)]
    admin_token: Option<String>,

    /// If set, write a small JSON file once the server is listening.
    ///
    /// Useful for scripts/tests to learn the chosen port when `--listen ...:0`.
    #[arg(long)]
    ready_file: Option<PathBuf>,

    /// Optional Lean verifier executable (axiograph_verify) to validate certificates server-side.
    ///
    /// If omitted, the server will try (in order):
    /// - `AXIOGRAPH_VERIFY_BIN`,
    /// - `axiograph_verify` next to the running `axiograph` binary,
    /// - `lean/.lake/build/bin/axiograph_verify` (when running from repo root).
    #[arg(long)]
    verify_bin: Option<PathBuf>,

    /// Certificate verification timeout (seconds). `0` disables the timeout.
    ///
    /// This is only used for server-side verification calls (e.g. `POST /query`
    /// with `"verify": true`).
    #[arg(long, default_value_t = 30)]
    verify_timeout_secs: u64,

    /// Enable LLM endpoints for the server UI (`/viz`).
    ///
    /// This is an untrusted convenience feature: the model proposes tool calls
    /// and/or structured queries; Rust executes them against the loaded snapshot.
    /// Trusted correctness remains certificate-checking in Lean.
    ///
    /// Choose at most one backend: `--llm-mock`, `--llm-ollama`, `--llm-openai`, `--llm-anthropic`, or `--llm-plugin ...`.
    #[arg(long)]
    llm_mock: bool,

    /// Optional LLM plugin executable (supports v2 query mode and v3 tool-loop mode).
    #[arg(long)]
    llm_plugin: Option<PathBuf>,

    /// Extra args for `--llm-plugin` (repeatable).
    #[arg(long)]
    llm_plugin_arg: Vec<String>,

    /// Use the built-in Ollama backend (local models via Ollama).
    #[arg(long)]
    llm_ollama: bool,

    /// Optional Ollama host override (defaults to `OLLAMA_HOST` or `http://127.0.0.1:11434`).
    #[arg(long)]
    llm_ollama_host: Option<String>,

    /// Use the built-in OpenAI backend (networked).
    #[arg(long)]
    llm_openai: bool,

    /// Optional OpenAI base URL override (defaults to `OPENAI_BASE_URL` or `https://api.openai.com`).
    #[arg(long)]
    llm_openai_base_url: Option<String>,

    /// Use the built-in Anthropic backend (networked).
    #[arg(long)]
    llm_anthropic: bool,

    /// Optional Anthropic base URL override (defaults to `ANTHROPIC_BASE_URL` or `https://api.anthropic.com`).
    #[arg(long)]
    llm_anthropic_base_url: Option<String>,

    /// Optional model name for the plugin, or for Ollama (required when `--llm-ollama` is set).
    #[arg(long)]
    llm_model: Option<String>,

    /// Enable world model plugin endpoints for proposal generation.
    ///
    /// Choose at most one backend: `--world-model-stub`, `--world-model-plugin ...`,
    /// `--world-model-http ...`, or `--world-model-llm`.
    #[arg(long)]
    world_model_stub: bool,

    /// Optional world model plugin executable (speaks `axiograph_world_model_v1`).
    #[arg(long)]
    world_model_plugin: Option<PathBuf>,

    /// Extra args for `--world-model-plugin` (repeatable).
    #[arg(long)]
    world_model_plugin_arg: Vec<String>,

    /// Optional world model HTTP endpoint (speaks `axiograph_world_model_v1`).
    #[arg(long)]
    world_model_http: Option<String>,

    /// Use the built-in LLM-backed world model plugin.
    #[arg(long)]
    world_model_llm: bool,

    /// Optional world model model name for provenance (free-form).
    #[arg(long)]
    world_model_model: Option<String>,

    /// Number of worker slots reserved for world-model jobs.
    #[arg(long, default_value_t = 2)]
    world_model_workers: usize,

    /// LRU capacity (number of path signatures) for deeper-than-indexed paths.
    /// `0` disables the LRU cache.
    #[arg(long, default_value_t = 0)]
    path_index_lru_capacity: usize,

    /// Enable async updates for the deeper-path LRU cache.
    #[arg(long)]
    path_index_lru_async: bool,

    /// Async queue size for deeper-path LRU updates (ignored unless async is enabled).
    #[arg(long, default_value_t = 1024)]
    path_index_lru_queue: usize,
}

#[derive(Subcommand)]
enum CertCommands {
    /// Run an AxQL/SQL-ish query over a `.axi` snapshot/module and emit a query-result certificate.
    Query {
        /// Input `.axi` file.
        ///
        /// This may be either:
        /// - a `PathDBExportV1` snapshot export (reversible `.axi` export), or
        /// - a canonical `axi_v1` module (schema/theory/instance).
        ///
        /// If the input is a canonical module, you must pass `--anchor-out` so
        /// this command can write a derived `PathDBExportV1` snapshot anchor for
        /// `axiograph_verify`.
        input: PathBuf,

        /// Query language: `axql` or `sql`.
        #[arg(long, default_value = "axql")]
        lang: String,

        /// Query text (quote it in your shell).
        query: String,

        /// Write certificate JSON to this path (defaults to stdout).
        #[arg(short, long)]
        out: Option<PathBuf>,

        /// Write the derived `PathDBExportV1` anchor snapshot to this `.axi` path.
        ///
        /// Required when `input` is a canonical module (because the Lean verifier
        /// currently anchors query-result certificates to snapshot exports).
        #[arg(long)]
        anchor_out: Option<PathBuf>,
    },

    /// Typecheck a canonical `.axi` module and emit an `axi_well_typed_v1` certificate.
    Typecheck {
        /// Input `.axi` file (canonical `axi_v1` schema/theory/instance module).
        input: PathBuf,

        /// Write certificate JSON to this path (defaults to stdout).
        #[arg(short, long)]
        out: Option<PathBuf>,
    },

    /// Check a conservative subset of theory constraints and emit an `axi_constraints_ok_v1` certificate.
    Constraints {
        /// Input `.axi` file (canonical `axi_v1` schema/theory/instance module).
        input: PathBuf,

        /// Write certificate JSON to this path (defaults to stdout).
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
}

#[derive(Args)]
struct VizArgs {
    /// Input `.axpd` or `.axi` file.
    input: PathBuf,
    /// Output file (extension does not matter; use `--format`).
    #[arg(short, long)]
    out: PathBuf,
    /// Output format: dot|html|json
    #[arg(long, default_value = "dot")]
    format: String,
    /// Plane selection: data|meta|both.
    ///
    /// - `data`: only instance/data-plane nodes (default)
    /// - `meta`: only `.axi` meta-plane nodes (`AxiMeta*`)
    /// - `both`: include both planes
    #[arg(long, default_value = "data")]
    plane: String,
    /// Focus entity id (repeatable).
    #[arg(long)]
    focus_id: Vec<u32>,
    /// Focus entity name (matches `attr(name) == <value>`).
    #[arg(long)]
    focus_name: Option<String>,
    /// Optional focus entity type filter (used only with `--focus-name`).
    ///
    /// Useful when both the meta-plane and data-plane contain the same `name`.
    #[arg(long)]
    focus_type: Option<String>,
    /// BFS radius around focus nodes.
    #[arg(long, default_value_t = 2)]
    hops: usize,
    /// Max nodes in the extracted subgraph.
    #[arg(long, default_value_t = 250)]
    max_nodes: usize,
    /// Max edges in the extracted subgraph.
    #[arg(long, default_value_t = 4000)]
    max_edges: usize,
    /// Direction for neighborhood expansion: out|in|both
    #[arg(long, default_value = "both")]
    direction: String,
    /// Include `.axi` meta-plane nodes (`AxiMeta*` types).
    #[arg(long)]
    include_meta: bool,
    /// Annotate data-plane nodes using the `.axi` meta-plane as a type layer.
    ///
    /// Adds:
    /// - inferred supertypes for object entities
    /// - relation signatures + theory constraints for fact nodes
    #[arg(long)]
    typed_overlay: bool,
    /// Exclude PathDB equivalence edges from the visualization.
    #[arg(long)]
    no_equivalences: bool,
}

#[derive(Subcommand)]
enum IngestCommands {
    /// Ingest SQL DDL → `proposals.json`
    Sql {
        /// Input SQL file
        input: PathBuf,
        /// Output proposals JSON (Evidence/Proposals schema)
        #[arg(short, long)]
        out: PathBuf,
        /// Output chunks JSON (for RAG)
        #[arg(long)]
        chunks: Option<PathBuf>,
    },

    /// Ingest document (text, markdown)
    Doc {
        /// Input document
        input: PathBuf,
        /// Output proposals JSON (Evidence/Proposals schema)
        #[arg(short, long)]
        out: PathBuf,
        /// Output chunks JSON (for RAG)
        #[arg(long)]
        chunks: Option<PathBuf>,
        /// Output extracted facts JSON
        #[arg(long)]
        facts: Option<PathBuf>,
        /// Treat as machining knowledge
        #[arg(long)]
        machining: bool,
        /// Domain for fact extraction (default: general)
        #[arg(long, default_value = "general")]
        domain: String,
    },

    /// Ingest conversation transcript
    Conversation {
        /// Input transcript file
        input: PathBuf,
        /// Output proposals JSON (Evidence/Proposals schema)
        #[arg(short, long)]
        out: PathBuf,
        /// Output chunks JSON
        #[arg(long)]
        chunks: Option<PathBuf>,
        /// Output extracted facts JSON
        #[arg(long)]
        facts: Option<PathBuf>,
        /// Format: slack, meeting
        #[arg(long, default_value = "slack")]
        format: String,
    },

    /// Ingest Confluence HTML export
    Confluence {
        /// Input HTML file
        input: PathBuf,
        /// Output proposals JSON (Evidence/Proposals schema)
        #[arg(short, long)]
        out: PathBuf,
        /// Confluence space name
        #[arg(long, default_value = "DOCS")]
        space: String,
        /// Output chunks JSON
        #[arg(long)]
        chunks: Option<PathBuf>,
        /// Output extracted facts JSON
        #[arg(long)]
        facts: Option<PathBuf>,
    },

    /// Ingest JSON data → `proposals.json`
    Json {
        /// Input JSON file
        input: PathBuf,
        /// Output proposals JSON (Evidence/Proposals schema)
        #[arg(short, long)]
        out: PathBuf,
        /// Output chunks JSON (for RAG)
        #[arg(long)]
        chunks: Option<PathBuf>,
    },

    /// Ingest recommended readings (BibTeX or markdown list)
    Readings {
        /// Input file (BibTeX or markdown)
        input: PathBuf,
        /// Output proposals JSON (Evidence/Proposals schema)
        #[arg(short, long)]
        out: PathBuf,
        /// Output chunks JSON
        #[arg(long)]
        chunks: Option<PathBuf>,
        /// Format: bibtex, markdown
        #[arg(long, default_value = "markdown")]
        format: String,
    },

    /// Protobuf / gRPC ingestion (`buf build` → descriptor set → proposals).
    Proto {
        #[command(subcommand)]
        command: proto::ProtoCommands,
    },

    /// Index a repository / codebase into chunks + lightweight graph edges
    Repo {
        #[command(subcommand)]
        command: RepoCommands,
    },

    /// Import a GitHub repo (or local repo path) into merged `proposals.json` + `chunks.json`
    Github {
        #[command(subcommand)]
        command: github::GithubCommands,
    },

    /// Scrape/crawl web pages into `chunks.json` + `proposals.json` (discovery tooling)
    Web {
        #[command(subcommand)]
        command: web::WebCommands,
    },

    /// Ingest a directory of heterogeneous sources (docs, SQL, RDF/OWL, JSON, Confluence)
    Dir {
        /// Root directory to ingest
        root: PathBuf,
        /// Output directory for ingestion artifacts
        #[arg(short, long, default_value = "build/ingest")]
        out_dir: PathBuf,
        /// Confluence space name (used for `.html` ingestion)
        #[arg(long, default_value = "DOCS")]
        confluence_space: String,
        /// Domain for document fact extraction
        #[arg(long, default_value = "general")]
        domain: String,
        /// Output aggregated chunks JSON (for RAG)
        #[arg(long)]
        chunks: Option<PathBuf>,
        /// Output aggregated extracted-facts JSON
        #[arg(long)]
        facts: Option<PathBuf>,
        /// Output generic proposals JSON (Evidence/Proposals schema)
        #[arg(long)]
        proposals: Option<PathBuf>,
        /// Maximum file size to ingest (bytes)
        #[arg(long, default_value_t = 524288)]
        max_file_bytes: u64,
        /// Maximum number of files to ingest
        #[arg(long, default_value_t = 50000)]
        max_files: usize,
    },

    /// Merge multiple `proposals.json` files (and optional `chunks.json`) into one.
    ///
    /// This is useful for ontology-engineering workflows where you ingest from
    /// heterogeneous sources (docs + SQL + proto + RDF + repo index) and then
    /// run a single `discover` pipeline step.
    Merge {
        /// Input proposals JSON files (Evidence/Proposals schema).
        #[arg(long)]
        proposals: Vec<PathBuf>,
        /// Optional input chunks JSON files (arrays of `Chunk`).
        #[arg(long)]
        chunks: Vec<PathBuf>,
        /// Output merged proposals JSON.
        #[arg(short, long)]
        out: PathBuf,
        /// Output merged chunks JSON (if any chunks are provided).
        #[arg(long)]
        chunks_out: Option<PathBuf>,
        /// Override `schema_hint` in the merged proposals file (default: keep the first non-empty).
        #[arg(long)]
        schema_hint: Option<String>,
    },

    /// Run a world model plugin to propose new facts/relations (evidence plane).
    WorldModel(WorldModelProposeArgs),

    /// Built-in world model plugin (LLM-backed). Reads request JSON from stdin and writes a response to stdout.
    #[command(name = "world-model-plugin-llm")]
    WorldModelPluginLlm(WorldModelPluginLlmArgs),
}

#[derive(Subcommand)]
enum RepoCommands {
    /// Index a repo into chunks + repo edges (definitions/imports/TODOs)
    Index {
        /// Root directory to index
        root: PathBuf,
        /// Output proposals JSON (Evidence/Proposals schema)
        #[arg(short, long)]
        out: PathBuf,
        /// Output chunks JSON (for RAG)
        #[arg(long)]
        chunks: Option<PathBuf>,
        /// Output repo edges JSON (definitions/imports/TODOs)
        #[arg(long)]
        edges: Option<PathBuf>,
        /// Maximum file size to read (bytes)
        #[arg(long, default_value_t = 524288)]
        max_file_bytes: u64,
        /// Maximum number of files to index
        #[arg(long, default_value_t = 50000)]
        max_files: usize,
        /// Lines per code chunk (non-markdown)
        #[arg(long, default_value_t = 80)]
        lines_per_chunk: usize,
    },

    /// Continuously re-index a repo (polling)
    Watch {
        /// Root directory to index
        root: PathBuf,
        /// Output proposals JSON (Evidence/Proposals schema)
        #[arg(short, long)]
        out: PathBuf,
        /// Output chunks JSON (for RAG)
        #[arg(long)]
        chunks: Option<PathBuf>,
        /// Output repo edges JSON
        #[arg(long)]
        edges: Option<PathBuf>,
        /// Output discovery trace JSON (suggested links)
        #[arg(long)]
        trace: Option<PathBuf>,
        /// Polling interval (seconds)
        #[arg(long, default_value_t = 30)]
        interval_secs: u64,
        /// Maximum number of suggestions per run
        #[arg(long, default_value_t = 1000)]
        max_suggestions: usize,
    },
}

#[derive(Subcommand)]
enum PathdbCommands {
    /// Export a `.axpd` PathDB file to a reversible `.axi` snapshot (`PathDBExportV1`)
    ExportAxi {
        /// Input `.axpd` file
        input: PathBuf,
        /// Output `.axi` file
        #[arg(short, long)]
        out: PathBuf,
    },

    /// Export a canonical `.axi` module from a `.axpd` file (schema/theory/instance).
    ///
    /// This requires that the PathDB contains the `.axi` meta-plane produced by
    /// importing a canonical `.axi` module (e.g. via `axiograph db pathdb import-axi`
    /// or `axiograph repl import_axi`).
    ///
    /// If multiple modules are present, pass `--module <name>`.
    ExportModule {
        /// Input `.axpd` file
        input: PathBuf,
        /// Output `.axi` file
        #[arg(short, long)]
        out: PathBuf,
        /// Module name to export (required if multiple modules are present).
        #[arg(long)]
        module: Option<String>,
    },

    /// Import a `.axi` file into a `.axpd` PathDB file
    ///
    /// Accepts either:
    /// - a reversible PathDB snapshot export (schema `PathDBExportV1`), or
    /// - a canonical `axi_v1` module (schema/theory/instance), which is imported into a fresh PathDB.
    ImportAxi {
        /// Input `.axi` file
        input: PathBuf,
        /// Output `.axpd` file
        #[arg(short, long)]
        out: PathBuf,
    },

    /// Import `chunks.json` into a `.axpd` snapshot as `DocChunk` entities.
    ///
    /// This is an **extension layer** intended for discovery workflows:
    /// - enables `fts(...)` / `contains(...)` queries over chunk text
    /// - links chunk evidence to typed entities when chunk metadata permits
    ImportChunks {
        /// Input `.axpd` file
        input: PathBuf,
        /// Input chunks JSON (array of `Chunk` objects)
        #[arg(long)]
        chunks: PathBuf,
        /// Output `.axpd` file
        #[arg(short, long)]
        out: PathBuf,
    },
}

#[derive(Subcommand)]
enum DiscoverCommands {
    /// Suggest lightweight links based on chunks + repo edges
    SuggestLinks {
        /// Input chunks JSON
        chunks: PathBuf,
        /// Input repo edges JSON
        edges: PathBuf,
        /// Output discovery trace JSON
        #[arg(short, long)]
        out: PathBuf,
        /// Maximum number of proposals to emit
        #[arg(long, default_value_t = 1000)]
        max_proposals: usize,
    },

    /// Promote `proposals.json` into candidate domain `.axi` modules (explicit, reviewable)
    PromoteProposals {
        /// Input proposals JSON (Evidence/Proposals schema)
        proposals: PathBuf,
        /// Output directory for candidate `.axi` files + trace
        #[arg(short, long, default_value = "build/candidates")]
        out_dir: PathBuf,
        /// Optional output trace JSON (defaults to `<out_dir>/promotion_trace.json`)
        #[arg(long)]
        trace: Option<PathBuf>,
        /// Confidence threshold (drop proposals below this)
        #[arg(long, default_value_t = 0.0)]
        min_confidence: f64,
        /// Domains to emit: `all`, or a comma-separated list like `economic_flows,machinist_learning,schema_evolution`
        #[arg(long, default_value = "all")]
        domains: String,
    },

    /// Augment `proposals.json` with derived structure (and optional LLM suggestions).
    ///
    /// This stays in the evidence plane: the output remains untrusted and must be
    /// explicitly promoted into candidate `.axi` modules.
    AugmentProposals {
        /// Input proposals JSON (Evidence/Proposals schema)
        proposals: PathBuf,
        /// Output augmented proposals JSON
        #[arg(short, long)]
        out: PathBuf,
        /// Optional output trace JSON (defaults to `<out>.trace.json`)
        #[arg(long)]
        trace: Option<PathBuf>,
        /// Optional chunks JSON (used only to provide evidence snippets to an LLM plugin).
        #[arg(long)]
        chunks: Option<PathBuf>,
        /// Optional LLM plugin executable (speaks `axiograph_llm_plugin_v2`).
        #[arg(long)]
        llm_plugin: Option<PathBuf>,
        /// Extra args for `--llm-plugin` (repeatable).
        #[arg(long)]
        llm_plugin_arg: Vec<String>,
        /// Use the built-in Ollama backend (local models via Ollama) instead of a plugin.
        ///
        /// This is equivalent in spirit to `repl`'s `llm use ollama ...`, but it runs in the
        /// discovery pipeline: it updates `schema_hint` and may add proposals (untrusted).
        #[arg(long)]
        llm_ollama: bool,
        /// Optional Ollama host override (defaults to `OLLAMA_HOST` or `http://127.0.0.1:11434`).
        #[arg(long)]
        llm_ollama_host: Option<String>,
        /// Use the built-in OpenAI backend (networked).
        #[arg(long)]
        llm_openai: bool,
        /// Optional OpenAI base URL override (defaults to `OPENAI_BASE_URL` or `https://api.openai.com`).
        #[arg(long)]
        llm_openai_base_url: Option<String>,
        /// Use the built-in Anthropic backend (networked).
        #[arg(long)]
        llm_anthropic: bool,
        /// Optional Anthropic base URL override (defaults to `ANTHROPIC_BASE_URL` or `https://api.anthropic.com`).
        #[arg(long)]
        llm_anthropic_base_url: Option<String>,
        /// Optional model name for the plugin.
        #[arg(long)]
        llm_model: Option<String>,
        /// LLM request timeout in seconds (0 disables). Can also be set via `AXIOGRAPH_LLM_TIMEOUT_SECS`.
        #[arg(long)]
        llm_timeout_secs: Option<u64>,
        /// Allow the LLM to add new proposals (untrusted).
        ///
        /// This is useful for domain grounding and "fill in the blanks" structure:
        /// the LLM can propose additional entities/relations that are implied by
        /// the evidence chunks (and optionally by background knowledge).
        ///
        /// Safety notes:
        /// - proposals remain untrusted and stay in the evidence plane
        /// - new proposals are capped by `--max-new-proposals`
        /// - prefer passing `--chunks` so the LLM can cite concrete chunk ids
        #[arg(long)]
        llm_add_proposals: bool,
        /// Max number of new proposals to add (heuristics + plugin combined).
        #[arg(long, default_value_t = 25_000)]
        max_new_proposals: usize,
        /// Overwrite existing `schema_hint` values (default: only fill missing).
        #[arg(long)]
        overwrite_schema_hints: bool,
        /// Disable deterministic mention-role augmentation.
        #[arg(long)]
        no_roles: bool,
        /// Disable deterministic Todo→Symbol mention linking.
        #[arg(long)]
        no_todo_symbol: bool,
        /// Disable heuristic schema-hint inference.
        #[arg(long)]
        no_infer_hints: bool,
    },

    /// Draft a canonical `axi_v1` `.axi` module from `proposals.json` (schema discovery).
    ///
    /// This is an “automated ontology engineering” helper:
    /// - entity types become object types,
    /// - relation types become binary relations `Rel(from, to)`,
    /// - instance elements come from proposal names (globally disambiguated),
    /// - optional extensional constraints (keys/functionals) can be inferred from current tuples.
    ///
    /// The output is **untrusted** and intended for review/promotion.
    DraftModule {
        /// Input proposals JSON (Evidence/Proposals schema)
        proposals: PathBuf,

        /// Output `.axi` file (candidate module)
        #[arg(short, long)]
        out: PathBuf,

        /// Module name (default: `Discovered`)
        #[arg(long, default_value = "Discovered")]
        module: String,

        /// Schema name (default: `Discovered`)
        #[arg(long, default_value = "Discovered")]
        schema: String,

        /// Instance name (default: `DiscoveredInstance`)
        #[arg(long, default_value = "DiscoveredInstance")]
        instance: String,

        /// Infer extensional constraints (keys + simple functionals) from observed tuples.
        #[arg(long)]
        infer_constraints: bool,

        /// Use the built-in Ollama backend to suggest additional *structure* (untrusted).
        ///
        /// This can propose:
        /// - additional subtype edges between discovered object types, and
        /// - relational properties (e.g. symmetric/transitive) as candidate constraints.
        ///
        /// The result is still a **candidate** module for review/promotion.
        #[arg(long)]
        llm_ollama: bool,

        /// Optional Ollama host override (defaults to `OLLAMA_HOST` or `http://127.0.0.1:11434`).
        #[arg(long)]
        llm_ollama_host: Option<String>,
        /// Use the built-in OpenAI backend (networked) to suggest additional structure (untrusted).
        #[arg(long)]
        llm_openai: bool,
        /// Optional OpenAI base URL override (defaults to `OPENAI_BASE_URL` or `https://api.openai.com`).
        #[arg(long)]
        llm_openai_base_url: Option<String>,
        /// Use the built-in Anthropic backend (networked) to suggest additional structure (untrusted).
        #[arg(long)]
        llm_anthropic: bool,
        /// Optional Anthropic base URL override (defaults to `ANTHROPIC_BASE_URL` or `https://api.anthropic.com`).
        #[arg(long)]
        llm_anthropic_base_url: Option<String>,

        /// Optional model name for Ollama (required when `--llm-ollama` is set).
        #[arg(long)]
        llm_model: Option<String>,

        /// LLM request timeout in seconds (0 disables). Can also be set via `AXIOGRAPH_LLM_TIMEOUT_SECS`.
        #[arg(long)]
        llm_timeout_secs: Option<u64>,
    },

    /// Export JEPA/SSL training pairs from a canonical `.axi` module.
    ///
    /// This exports **full** schema+theory+instance context and a list of
    /// masked targets derived from instance tuples. It is anchored to the
    /// module's `axi_digest_v1` and is suitable for self-supervised training
    /// pipelines.
    JepaExport {
        /// Input `.axi` module (canonical `axi_v1`)
        input: PathBuf,
        /// Output JSON file
        #[arg(short, long)]
        out: PathBuf,
        /// Optional instance name filter (only export targets from this instance)
        #[arg(long)]
        instance: Option<String>,
        /// Max number of target items to emit (0 = all)
        #[arg(long, default_value_t = 0)]
        max_items: usize,
        /// Number of fields to mask per tuple (default: 1)
        #[arg(long, default_value_t = 1)]
        mask_fields: usize,
        /// RNG seed (deterministic)
        #[arg(long, default_value_t = 1)]
        seed: u64,
    },

    /// Generate or translate competency questions (AxQL) for coverage checks.
    CompetencyQuestions(CompetencyQuestionsArgs),

    /// Run a world model plugin to propose new facts/relations (evidence plane).
    WorldModelPropose(WorldModelProposeArgs),
}

#[derive(Args, Debug, Clone)]
struct WorldModelProposeArgs {
    /// Input `.axi` or `.axpd` snapshot (used for guardrails / validation).
    input: PathBuf,

    /// Optional JEPA export JSON (if provided, passed to the world model).
    #[arg(long)]
    export: Option<PathBuf>,

    /// Optional output path for a generated JEPA export.
    #[arg(long)]
    export_out: Option<PathBuf>,

    /// Instance filter for generated JEPA export (only when `--export` is not set).
    #[arg(long)]
    export_instance: Option<String>,

    /// Cap the number of JEPA items generated (0 = no cap).
    #[arg(long, default_value_t = 0)]
    export_max_items: usize,

    /// Number of fields to mask per JEPA item.
    #[arg(long, default_value_t = 1)]
    export_mask_fields: usize,

    /// Random seed for JEPA export masking.
    #[arg(long, default_value_t = 1)]
    export_seed: u64,

    /// Output proposals JSON (Evidence/Proposals schema).
    #[arg(short, long)]
    out: PathBuf,

    /// Optional world model plugin executable (speaks `axiograph_world_model_v1`).
    #[arg(long)]
    world_model_plugin: Option<PathBuf>,

    /// Extra args for `--world-model-plugin` (repeatable).
    #[arg(long)]
    world_model_plugin_arg: Vec<String>,

    /// Optional world model HTTP endpoint (speaks `axiograph_world_model_v1`).
    #[arg(long)]
    world_model_http: Option<String>,

    /// Use the built-in LLM-backed world model plugin.
    #[arg(long)]
    world_model_llm: bool,

    /// Use the stub world model backend (emits no proposals).
    #[arg(long)]
    world_model_stub: bool,

    /// Optional model name for provenance (free-form).
    #[arg(long)]
    world_model_model: Option<String>,

    /// Max new proposals to keep (0 = no cap).
    #[arg(long, default_value_t = 0)]
    max_new_proposals: usize,

    /// Optional goal strings passed to the world model (repeatable).
    #[arg(long)]
    goal: Vec<String>,

    /// Optional random seed passed to the world model.
    #[arg(long)]
    seed: Option<u64>,

    /// Guardrail profile: off|fast|strict.
    #[arg(long, default_value = "fast")]
    guardrail_profile: String,

    /// Guardrail plane: meta|data|both.
    #[arg(long, default_value = "both")]
    guardrail_plane: String,

    /// Override guardrail weights (repeatable): key=value.
    ///
    /// Keys: quality_error, quality_warning, quality_info,
    /// axi_fact_error, rewrite_rule_error, context_error, modal_error.
    #[arg(long)]
    guardrail_weight: Vec<String>,

    /// Task cost terms (repeatable): name=value[:weight[:unit]].
    #[arg(long)]
    task_cost: Vec<String>,

    /// Optional planning horizon (steps) passed to the world model.
    #[arg(long)]
    horizon_steps: Option<usize>,

    /// Optional guardrail report output path.
    #[arg(long)]
    guardrail_out: Option<PathBuf>,

    /// Commit proposals into the PathDB WAL under this accepted-plane directory.
    #[arg(long)]
    commit_dir: Option<PathBuf>,

    /// Accepted snapshot id for WAL commit (default: head).
    #[arg(long, default_value = "head")]
    accepted_snapshot: String,

    /// Commit message for WAL commit.
    #[arg(long)]
    commit_message: Option<String>,

    /// Validate proposals before commit (default: true when committing).
    #[arg(long)]
    validate: Option<bool>,

    /// Validation quality profile: off|fast|strict.
    #[arg(long, default_value = "fast")]
    quality: String,

    /// Validation plane: meta|data|both.
    #[arg(long, default_value = "both")]
    quality_plane: String,
}

#[derive(Args, Debug, Clone)]
struct WorldModelPluginLlmArgs {
    /// Backend: openai|anthropic|ollama|mock (defaults to WORLD_MODEL_BACKEND or openai).
    #[arg(long)]
    backend: Option<String>,

    /// Optional model name (defaults to WORLD_MODEL_MODEL or provider defaults).
    #[arg(long)]
    model: Option<String>,

    /// Optional OpenAI base URL override (defaults to OPENAI_BASE_URL or https://api.openai.com).
    #[arg(long)]
    openai_base_url: Option<String>,

    /// Optional Anthropic base URL override (defaults to ANTHROPIC_BASE_URL or https://api.anthropic.com).
    #[arg(long)]
    anthropic_base_url: Option<String>,

    /// Optional Ollama host override (defaults to OLLAMA_HOST or http://127.0.0.1:11434).
    #[arg(long)]
    ollama_host: Option<String>,
}

#[derive(Args, Debug, Clone)]
struct CompetencyQuestionsArgs {
    /// Input `.axi` or `.axpd` snapshot (for schema + NL query translation).
    input: PathBuf,

    /// Output JSON file (array of competency questions).
    #[arg(short, long)]
    out: PathBuf,

    /// Optional natural-language question file (txt or json) to translate.
    #[arg(long)]
    from_nl: Option<PathBuf>,

    /// Disable schema-based generation (use only `--from-nl`).
    #[arg(long)]
    no_schema: bool,

    /// Exclude object-type questions from schema generation.
    #[arg(long)]
    no_types: bool,

    /// Exclude relation questions from schema generation.
    #[arg(long)]
    no_relations: bool,

    /// Include generic `Entity` types if present in the schema.
    #[arg(long)]
    include_entity: bool,

    /// Default minimum rows required to satisfy a CQ.
    #[arg(long, default_value_t = 1)]
    min_rows: usize,

    /// Default weight (penalty) when a CQ is unsatisfied.
    #[arg(long, default_value_t = 1.0)]
    weight: f64,

    /// Default contexts to scope the CQ (repeatable).
    #[arg(long)]
    context: Vec<String>,

    /// Cap the number of emitted questions (0 = no cap).
    #[arg(long, default_value_t = 0)]
    max_questions: usize,

    /// Use the mock NLQ backend for NL->AxQL translation.
    #[arg(long)]
    llm_mock: bool,

    /// Use the built-in Ollama backend for NL->AxQL translation.
    #[arg(long)]
    llm_ollama: bool,

    /// Optional Ollama host override.
    #[arg(long)]
    llm_ollama_host: Option<String>,

    /// Use the built-in OpenAI backend for NL->AxQL translation.
    #[arg(long)]
    llm_openai: bool,

    /// Optional OpenAI base URL override.
    #[arg(long)]
    llm_openai_base_url: Option<String>,

    /// Use the built-in Anthropic backend for NL->AxQL translation.
    #[arg(long)]
    llm_anthropic: bool,

    /// Optional Anthropic base URL override.
    #[arg(long)]
    llm_anthropic_base_url: Option<String>,

    /// Optional LLM plugin executable (speaks `axiograph_llm_plugin_v2`).
    #[arg(long)]
    llm_plugin: Option<PathBuf>,

    /// Extra args for `--llm-plugin` (repeatable).
    #[arg(long)]
    llm_plugin_arg: Vec<String>,

    /// Model name (for LLM backends or plugins).
    #[arg(long)]
    llm_model: Option<String>,
}

#[derive(Subcommand)]
enum AcceptedCommands {
    /// Initialize the accepted-plane + PathDB WAL directory layout.
    ///
    /// This is idempotent and safe to run even if the directory already exists.
    Init {
        /// Accepted-plane directory (contains `modules/`, `snapshots/`, `HEAD`, and logs).
        #[arg(long, default_value = "build/accepted_plane")]
        dir: PathBuf,
    },

    /// Sync an accepted-plane directory to another directory (master → replica).
    ///
    /// This is filesystem-only and intended to be used with:
    /// - local disk copies (cp/rsync),
    /// - shared storage (NFS),
    /// - or object-store sync (future).
    ///
    /// The snapshot store is treated as:
    /// - immutable, content-addressed objects (`modules/`, `snapshots/`, `pathdb/blobs/`, …), plus
    /// - a small mutable pointer (`HEAD`) per layer.
    ///
    /// Sync copies missing immutable objects first, then optionally updates `HEAD`
    /// pointers (so a replica becomes queryable immediately).
    Sync {
        /// Source accepted-plane directory (master).
        #[arg(long)]
        from: PathBuf,
        /// Destination accepted-plane directory (replica).
        #[arg(long, default_value = "build/accepted_plane")]
        dir: PathBuf,
        /// Which layer to sync: `accepted`, `pathdb`, or `both`.
        #[arg(long, default_value = "both")]
        layer: String,
        /// Include PathDB `.axpd` checkpoints when syncing `pathdb`.
        ///
        /// If omitted, a replica can still rebuild checkpoints from manifests + blobs.
        #[arg(long)]
        include_checkpoints: bool,
        /// Include append-only logs (`accepted_plane.log.jsonl`, `pathdb_wal.log.jsonl`).
        #[arg(long)]
        include_logs: bool,
        /// Do not update `HEAD` pointers (copy immutable objects only).
        #[arg(long)]
        no_update_head: bool,
        /// Print what would be copied, but do not write.
        #[arg(long)]
        dry_run: bool,
    },

    /// List accepted-plane and/or PathDB WAL snapshots (most recent first).
    #[command(aliases = ["ls"])]
    List {
        /// Accepted-plane directory.
        #[arg(long, default_value = "build/accepted_plane")]
        dir: PathBuf,
        /// Which layer to list: `accepted`, `pathdb`, or `both`.
        #[arg(long, default_value = "accepted")]
        layer: String,
        /// Maximum number of snapshots to print.
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Print full snapshot ids (default prints shortened ids).
        #[arg(long)]
        full: bool,
    },

    /// Show details of an accepted-plane or PathDB WAL snapshot.
    #[command(aliases = ["cat", "describe"])]
    Show {
        /// Accepted-plane directory.
        #[arg(long, default_value = "build/accepted_plane")]
        dir: PathBuf,
        /// Which layer: `accepted` or `pathdb`.
        #[arg(long, default_value = "accepted")]
        layer: String,
        /// Snapshot id (or `latest` / `head`, or a unique prefix).
        #[arg(long, default_value = "head")]
        snapshot: String,
        /// Print the raw JSON manifest.
        #[arg(long)]
        json: bool,
        /// Print full snapshot ids (default prints shortened ids).
        #[arg(long)]
        full: bool,
    },

    /// Promote a reviewed canonical `.axi` module into the accepted plane.
    ///
    /// This:
    /// - parses + typechecks the module (Rust gate),
    /// - stores it under `modules/<name>/<digest>.axi`,
    /// - appends a JSONL log event, and
    /// - writes a new snapshot manifest (content-derived snapshot id).
    Promote {
        /// Input canonical `.axi` module (axi_v1).
        input: PathBuf,
        /// Accepted-plane directory (contains `modules/`, `snapshots/`, `HEAD`, and `accepted_plane.log.jsonl`).
        #[arg(long, default_value = "build/accepted_plane")]
        dir: PathBuf,
        /// Optional promotion message (for human audit trail).
        #[arg(long)]
        message: Option<String>,
        /// Optional quality gate (and report attachment): off|fast|strict.
        ///
        /// - `off`: do not run quality checks (default)
        /// - `fast`: run cheap lints + key/functional checks when meta-plane is present
        /// - `strict`: run additional expensive lints (still untrusted tooling)
        #[arg(long, default_value = "off")]
        quality: String,
    },

    /// Rebuild a `.axpd` PathDB snapshot from an accepted-plane snapshot id.
    BuildPathdb {
        /// Accepted-plane directory.
        #[arg(long, default_value = "build/accepted_plane")]
        dir: PathBuf,
        /// Snapshot id (or `latest` / `head`).
        #[arg(long, default_value = "latest")]
        snapshot: String,
        /// Output `.axpd` file.
        #[arg(short, long)]
        out: PathBuf,
    },

    /// Commit a PathDB WAL snapshot (append-only) under the accepted-plane directory.
    ///
    /// This adds *extension-layer* overlays (currently: `chunks.json` + `proposals.json` imports) on
    /// top of an accepted-plane snapshot. The resulting PathDB snapshot id is
    /// content-derived and can be checked out later via `pathdb-build`.
    PathdbCommit {
        /// Accepted-plane directory.
        #[arg(long, default_value = "build/accepted_plane")]
        dir: PathBuf,
        /// Accepted-plane snapshot id (or `latest` / `head`).
        #[arg(long, default_value = "latest")]
        accepted_snapshot: String,
        /// One or more chunks JSON files (array of `Chunk`) to import.
        #[arg(long)]
        chunks: Vec<PathBuf>,
        /// One or more proposals JSON files (`ProposalsFileV1`) to import.
        #[arg(long)]
        proposals: Vec<PathBuf>,
        /// Optional message (for human audit trail).
        #[arg(long)]
        message: Option<String>,

        /// Print phase timings (useful for profiling large overlay commits).
        #[arg(long)]
        timings: bool,

        /// Write phase timings JSON to this path.
        #[arg(long)]
        timings_json: Option<PathBuf>,

        /// Override path index depth for this commit (0 disables path indexing).
        #[arg(long)]
        path_index_depth: Option<usize>,
    },

    /// Compute and commit snapshot-scoped embeddings into the PathDB WAL (extension layer).
    ///
    /// This stores embeddings as immutable blobs under `pathdb/blobs/` and
    /// references them from the PathDB snapshot manifest.
    ///
    /// Intended usage:
    /// 1) `axiograph db accept pathdb-commit ... --chunks <chunks.json>`
    /// 2) `axiograph db accept pathdb-embed --snapshot head --target docchunks --embed-backend ollama --embed-model nomic-embed-text`
    PathdbEmbed {
        /// Accepted-plane directory.
        #[arg(long, default_value = "build/accepted_plane")]
        dir: PathBuf,
        /// Base PathDB snapshot id to embed (or `latest`/`head`).
        #[arg(long, default_value = "head")]
        snapshot: String,
        /// What to embed: `docchunks`, `entities`, or `both`.
        #[arg(long, default_value = "docchunks")]
        target: String,
        /// Which embedding backend to use: `ollama` or `openai`.
        ///
        /// Notes:
        /// - Anthropic does not provide embeddings; use `openai` or rely on deterministic retrieval.
        #[arg(long, default_value = "ollama")]
        embed_backend: String,
        /// Optional Ollama host override (defaults to `OLLAMA_HOST` or `http://127.0.0.1:11434`).
        #[arg(long)]
        ollama_host: Option<String>,
        /// Embedding model name (backend-dependent).
        ///
        /// Common values:
        /// - Ollama: `nomic-embed-text`
        /// - OpenAI: `text-embedding-3-small` / `text-embedding-3-large`
        ///
        /// Back-compat: `--ollama-model` is accepted as an alias for `--embed-model`.
        #[arg(long, alias = "ollama-model")]
        embed_model: Option<String>,
        /// Optional OpenAI base URL override (defaults to `OPENAI_BASE_URL` or `https://api.openai.com`).
        #[arg(long)]
        openai_base_url: Option<String>,
        /// Max number of items to embed (safety valve).
        #[arg(long, default_value_t = 25_000)]
        max_items: usize,
        /// Batch size for `/api/embed` (fallback to per-item calls when unsupported).
        #[arg(long, default_value_t = 32)]
        batch_size: usize,
        /// Optional Ollama request timeout in seconds (0 disables). Can also be set via `AXIOGRAPH_LLM_TIMEOUT_SECS`.
        #[arg(long)]
        timeout_secs: Option<u64>,
        /// Optional message (for human audit trail).
        #[arg(long)]
        message: Option<String>,
    },

    /// Build/check out a PathDB `.axpd` from a PathDB WAL snapshot id.
    PathdbBuild {
        /// Accepted-plane directory.
        #[arg(long, default_value = "build/accepted_plane")]
        dir: PathBuf,
        /// PathDB snapshot id (or `latest` / `head`).
        #[arg(long, default_value = "latest")]
        snapshot: String,
        /// Output `.axpd` file.
        #[arg(short, long)]
        out: PathBuf,

        /// Print phase timings (useful for profiling large checkouts).
        #[arg(long)]
        timings: bool,

        /// Write phase timings JSON to this path.
        #[arg(long)]
        timings_json: Option<PathBuf>,

        /// Force a full rebuild (ignore any stored `.axpd` checkpoint).
        ///
        /// This is useful to:
        /// - profile the rebuild hot-path (apply ops + build indexes), and
        /// - sanity-check determinism vs checkpoints.
        #[arg(long)]
        rebuild: bool,

        /// Override path index depth (0 disables path indexing).
        #[arg(long)]
        path_index_depth: Option<usize>,

        /// Rewrite the checkpoint for this snapshot (implies rebuild).
        #[arg(long)]
        update_checkpoint: bool,
    },

    /// Show accepted-plane + PathDB WAL snapshot status (HEADs, counts).
    ///
    /// This is intended to be a “git status”-like quick diagnostic.
    Status {
        /// Accepted-plane directory.
        #[arg(long, default_value = "build/accepted_plane")]
        dir: PathBuf,
    },

    /// Show recent accepted-plane or PathDB WAL log events.
    ///
    /// This is intended to be a “git log”-like view over snapshot history.
    Log {
        /// Accepted-plane directory.
        #[arg(long, default_value = "build/accepted_plane")]
        dir: PathBuf,
        /// Which log to show: `accepted`, `pathdb`, or `both`.
        #[arg(long, default_value = "accepted")]
        layer: String,
        /// Maximum number of events to print (most recent first).
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let profiler = profiling::Profiler::start(&cli.profile)?;

    let result = (|| {
        match cli.command {
            Commands::Ingest { command } => match command {
                IngestCommands::Sql { input, out, chunks } => {
                    cmd_sql(&input, &out, chunks.as_ref())?;
                }
            IngestCommands::Doc {
                input,
                out,
                chunks,
                facts,
                machining,
                domain,
            } => {
                cmd_doc(&input, &out, chunks.as_ref(), facts.as_ref(), machining, &domain)?;
            }
            IngestCommands::Conversation {
                input,
                out,
                chunks,
                facts,
                format,
            } => {
                cmd_conversation(&input, &out, chunks.as_ref(), facts.as_ref(), &format)?;
            }
            IngestCommands::Confluence {
                input,
                out,
                space,
                chunks,
                facts,
            } => {
                cmd_confluence(&input, &out, &space, chunks.as_ref(), facts.as_ref())?;
            }
            IngestCommands::Json { input, out, chunks } => {
                cmd_json(&input, &out, chunks.as_ref())?;
            }
            IngestCommands::Readings {
                input,
                out,
                chunks,
                format,
            } => {
                cmd_readings(&input, &out, chunks.as_ref(), &format)?;
            }
            IngestCommands::Proto { command } => {
                proto::cmd_proto(command)?;
            }
            IngestCommands::Repo { command } => match command {
                RepoCommands::Index {
                    root,
                    out,
                    chunks,
                    edges,
                    max_file_bytes,
                    max_files,
                    lines_per_chunk,
                } => {
                    cmd_repo_index(
                        &root,
                        &out,
                        chunks.as_ref(),
                        edges.as_ref(),
                        max_file_bytes,
                        max_files,
                        lines_per_chunk,
                    )?;
                }
                RepoCommands::Watch {
                    root,
                    out,
                    chunks,
                    edges,
                    trace,
                    interval_secs,
                    max_suggestions,
                } => {
                    cmd_repo_watch(
                        &root,
                        &out,
                        chunks.as_ref(),
                        edges.as_ref(),
                        trace.as_ref(),
                        interval_secs,
                        max_suggestions,
                    )?;
                }
            },
            IngestCommands::Github { command } => {
                github::cmd_github(command)?;
            }
            IngestCommands::Web { command } => {
                web::cmd_web(command)?;
            }
            IngestCommands::Dir {
                root,
                out_dir,
                confluence_space,
                domain,
                chunks,
                facts,
                proposals,
                max_file_bytes,
                max_files,
            } => {
                cmd_ingest_dir(
                    &root,
                    &out_dir,
                    &confluence_space,
                    &domain,
                    chunks.as_ref(),
                    facts.as_ref(),
                    proposals.as_ref(),
                    max_file_bytes,
                    max_files,
                )?;
            }
            IngestCommands::Merge {
                proposals,
                chunks,
                out,
                chunks_out,
                schema_hint,
            } => {
                cmd_ingest_merge(
                    &proposals,
                    &chunks,
                    &out,
                    chunks_out.as_ref(),
                    schema_hint.as_deref(),
                )?;
            }
            IngestCommands::WorldModel(args) => {
                cmd_world_model_propose(&args)?;
            }
            IngestCommands::WorldModelPluginLlm(args) => {
                cmd_world_model_plugin_llm(&args)?;
            }
        },
        Commands::Check { command } => match command {
            CheckCommands::Validate { input } => {
                cmd_validate(&input)?;
            }
            CheckCommands::Fmt { input, out, write } => {
                axi_fmt::cmd_fmt_axi(&input, out.as_deref(), write)?;
            }
            CheckCommands::Quality {
                input,
                out,
                format,
                profile,
                plane,
                no_fail,
            } => {
                quality::cmd_quality(&input, out.as_ref(), &format, &profile, &plane, no_fail)?;
            }
        },
        Commands::Cert { command } => match command {
            CertCommands::Query {
                input,
                lang,
                query,
                out,
                anchor_out,
            } => {
                cmd_query_cert(&input, &lang, &query, out.as_ref(), anchor_out.as_ref())?;
            }
            CertCommands::Typecheck { input, out } => {
                cmd_typecheck_cert(&input, out.as_ref())?;
            }
            CertCommands::Constraints { input, out } => {
                cmd_constraints_cert(&input, out.as_ref())?;
            }
        },
        Commands::Tools { command } => match command {
            ToolsCommands::Viz(args) => {
                cmd_viz_from_args(&args)?;
            }
            ToolsCommands::Analyze { command } => {
                analyze::cmd_analyze(command)?;
            }
            ToolsCommands::Perf { command } => {
                perf::cmd_perf(command)?;
            }
        },
        Commands::Db { command } => match command {
            DbCommands::Accept { command } => {
                cmd_accept(command)?;
            }
            DbCommands::Pathdb { command } => {
                cmd_pathdb(command)?;
            }
            DbCommands::Serve(args) => {
                db_server::cmd_db_serve(args)?;
            }
        },
        Commands::Sql { input, out } => {
            cmd_sql(&input, &out, None)?;
        }
        Commands::Doc {
            input,
            out,
            chunks,
            facts,
            machining,
            domain,
        } => {
            cmd_doc(
                &input,
                &out,
                chunks.as_ref(),
                facts.as_ref(),
                machining,
                &domain,
            )?;
        }
        Commands::Conversation {
            input,
            out,
            chunks,
            facts,
            format,
        } => {
            cmd_conversation(&input, &out, chunks.as_ref(), facts.as_ref(), &format)?;
        }
        Commands::Confluence {
            input,
            out,
            space,
            chunks,
            facts,
        } => {
            cmd_confluence(&input, &out, &space, chunks.as_ref(), facts.as_ref())?;
        }
        Commands::Json { input, out } => {
            cmd_json(&input, &out, None)?;
        }
        Commands::Readings {
            input,
            out,
            chunks,
            format,
        } => {
            cmd_readings(&input, &out, chunks.as_ref(), &format)?;
        }
        Commands::Pathdb { command } => {
            cmd_pathdb(command)?;
        }
        Commands::Validate { input } => {
            cmd_validate(&input)?;
        }
        Commands::Repo { command } => match command {
            RepoCommands::Index {
                root,
                out,
                chunks,
                edges,
                max_file_bytes,
                max_files,
                lines_per_chunk,
            } => {
                cmd_repo_index(
                    &root,
                    &out,
                    chunks.as_ref(),
                    edges.as_ref(),
                    max_file_bytes,
                    max_files,
                    lines_per_chunk,
                )?;
            }
            RepoCommands::Watch {
                root,
                out,
                chunks,
                edges,
                trace,
                interval_secs,
                max_suggestions,
            } => {
                cmd_repo_watch(
                    &root,
                    &out,
                    chunks.as_ref(),
                    edges.as_ref(),
                    trace.as_ref(),
                    interval_secs,
                    max_suggestions,
                )?;
            }
        },
        Commands::Github { command } => {
            github::cmd_github(command)?;
        }
        Commands::Web { command } => {
            web::cmd_web(command)?;
        }
        Commands::Discover { command } => match command {
            DiscoverCommands::SuggestLinks {
                chunks,
                edges,
                out,
                max_proposals,
            } => {
                cmd_discover_suggest_links(&chunks, &edges, &out, max_proposals)?;
            }
            DiscoverCommands::PromoteProposals {
                proposals,
                out_dir,
                trace,
                min_confidence,
                domains,
            } => {
                cmd_discover_promote_proposals(
                    &proposals,
                    &out_dir,
                    trace.as_ref(),
                    min_confidence,
                    &domains,
                )?;
            }
            DiscoverCommands::AugmentProposals {
                proposals,
                out,
                trace,
                chunks,
                llm_plugin,
                llm_plugin_arg,
                llm_ollama,
                llm_ollama_host,
                llm_openai,
                llm_openai_base_url,
                llm_anthropic,
                llm_anthropic_base_url,
                llm_model,
                llm_timeout_secs,
                llm_add_proposals,
                max_new_proposals,
                overwrite_schema_hints,
                no_roles,
                no_todo_symbol,
                no_infer_hints,
            } => {
                cmd_discover_augment_proposals(
                    &proposals,
                    &out,
                    trace.as_ref(),
                    chunks.as_ref(),
                    llm_plugin.as_ref(),
                    &llm_plugin_arg,
                    llm_ollama,
                    llm_ollama_host.as_deref(),
                    llm_openai,
                    llm_openai_base_url.as_deref(),
                    llm_anthropic,
                    llm_anthropic_base_url.as_deref(),
                    llm_model.as_deref(),
                    llm_timeout_secs,
                    llm_add_proposals,
                    axiograph_ingest_docs::AugmentOptionsV1 {
                        infer_schema_hints: !no_infer_hints,
                        add_mention_role_entities: !no_roles,
                        add_todo_mentions_symbol: !no_todo_symbol,
                        max_new_proposals,
                        overwrite_schema_hints,
                    },
                )?;
            }
            DiscoverCommands::DraftModule {
                proposals,
                out,
                module,
                schema,
                instance,
                infer_constraints,
                llm_ollama,
                llm_ollama_host,
                llm_openai,
                llm_openai_base_url,
                llm_anthropic,
                llm_anthropic_base_url,
                llm_model,
                llm_timeout_secs,
            } => {
                let text = fs::read_to_string(&proposals)?;
                let file: axiograph_ingest_docs::ProposalsFileV1 = serde_json::from_str(&text)?;

                let options = crate::schema_discovery::DraftAxiModuleOptions {
                    module_name: module,
                    schema_name: schema,
                    instance_name: instance,
                    infer_constraints,
                };

                let base_draft =
                    crate::schema_discovery::draft_axi_module_from_proposals(&file, &options)?;

                let llm_selected =
                    (llm_ollama as usize) + (llm_openai as usize) + (llm_anthropic as usize);
                if llm_selected > 1 {
                    return Err(anyhow!(
                        "choose at most one LLM integration: either `--llm-ollama`, `--llm-openai`, or `--llm-anthropic`"
                    ));
                }

                let suggestions = {
                    let timeout = crate::llm::llm_timeout(llm_timeout_secs)?;
                    if llm_ollama {
                        #[cfg(feature = "llm-ollama")]
                        {
                            let model = llm_model.as_deref().ok_or_else(|| {
                                anyhow!("missing `--llm-model` (example: --llm-model nemotron-3-nano)")
                            })?;
                            let host = llm_ollama_host
                                .as_deref()
                                .map(|s| s.to_string())
                                .unwrap_or_else(crate::llm::default_ollama_host);
                            Some(ollama_suggest_schema_structure(
                                &host,
                                model,
                                &base_draft,
                                &options.schema_name,
                                timeout,
                            )?)
                        }
                        #[cfg(not(feature = "llm-ollama"))]
                        {
                            let _ = timeout;
                            return Err(anyhow!(
                                "ollama support not compiled (enable `axiograph-cli` feature `llm-ollama`)"
                            ));
                        }
                    } else if llm_openai {
                        #[cfg(feature = "llm-openai")]
                        {
                            let model = llm_model.as_deref().ok_or_else(|| {
                                anyhow!("missing `--llm-model` (example: --llm-model gpt-4o-mini)")
                            })?;
                            let base_url = llm_openai_base_url
                                .as_deref()
                                .map(|s| s.to_string())
                                .unwrap_or_else(crate::llm::default_openai_base_url);
                            Some(openai_suggest_schema_structure(
                                &base_url,
                                model,
                                &base_draft,
                                &options.schema_name,
                                timeout,
                            )?)
                        }
                        #[cfg(not(feature = "llm-openai"))]
                        {
                            let _ = timeout;
                            return Err(anyhow!(
                                "openai support not compiled (enable `axiograph-cli` feature `llm-openai`)"
                            ));
                        }
                    } else if llm_anthropic {
                        #[cfg(feature = "llm-anthropic")]
                        {
                            let model = llm_model.as_deref().ok_or_else(|| {
                                anyhow!("missing `--llm-model` (example: --llm-model claude-3-5-sonnet-20241022)")
                            })?;
                            let base_url = llm_anthropic_base_url
                                .as_deref()
                                .map(|s| s.to_string())
                                .unwrap_or_else(crate::llm::default_anthropic_base_url);
                            Some(anthropic_suggest_schema_structure(
                                &base_url,
                                model,
                                &base_draft,
                                &options.schema_name,
                                timeout,
                            )?)
                        }
                        #[cfg(not(feature = "llm-anthropic"))]
                        {
                            let _ = timeout;
                            return Err(anyhow!(
                                "anthropic support not compiled (enable `axiograph-cli` feature `llm-anthropic`)"
                            ));
                        }
                    } else {
                        None
                    }
                };

                let draft =
                    crate::schema_discovery::draft_axi_module_from_proposals_with_suggestions(
                        &file,
                        &options,
                        suggestions.as_ref(),
                    )?;

                fs::write(&out, draft)?;
                println!("wrote {}", out.display());
            }
            DiscoverCommands::JepaExport {
                input,
                out,
                instance,
                max_items,
                mask_fields,
                seed,
            } => {
                cmd_discover_jepa_export(
                    &input,
                    &out,
                    instance.as_deref(),
                    max_items,
                    mask_fields,
                    seed,
                )?;
            }
            DiscoverCommands::CompetencyQuestions(args) => {
                cmd_discover_competency_questions(&args)?;
            }
            DiscoverCommands::WorldModelPropose(args) => {
                cmd_world_model_propose(&args)?;
            }
        },
        Commands::Accept { command } => {
            cmd_accept(command)?;
        }
        Commands::IngestDir {
            root,
            out_dir,
            confluence_space,
            domain,
            chunks,
            facts,
            proposals,
            max_file_bytes,
            max_files,
        } => {
            cmd_ingest_dir(
                &root,
                &out_dir,
                &confluence_space,
                &domain,
                chunks.as_ref(),
                facts.as_ref(),
                proposals.as_ref(),
                max_file_bytes,
                max_files,
            )?;
        }
        Commands::Perf { command } => {
            perf::cmd_perf(command)?;
        }
        Commands::Viz(args) => {
            cmd_viz_from_args(&args)?;
        }
        Commands::Analyze { command } => {
            analyze::cmd_analyze(command)?;
        }
        Commands::Quality {
            input,
            out,
            format,
            profile,
            plane,
            no_fail,
        } => {
            quality::cmd_quality(&input, out.as_ref(), &format, &profile, &plane, no_fail)?;
        }
        Commands::Repl {
            axpd,
            script,
            cmd,
            continue_on_error,
            quiet,
        } => {
            if script.is_some() || !cmd.is_empty() {
                repl::cmd_repl_script(
                    axpd.as_ref(),
                    script.as_ref(),
                    &cmd,
                    continue_on_error,
                    quiet,
                )?;
            } else {
                repl::cmd_repl(axpd.as_ref())?;
            }
        }
        Commands::QueryCert {
            input,
            lang,
            query,
            out,
            anchor_out,
        } => {
            cmd_query_cert(&input, &lang, &query, out.as_ref(), anchor_out.as_ref())?;
        }
        Commands::TypecheckCert { input, out } => {
            cmd_typecheck_cert(&input, out.as_ref())?;
        }
        Commands::ConstraintsCert { input, out } => {
            cmd_constraints_cert(&input, out.as_ref())?;
        }
            Commands::Proto { command } => {
                proto::cmd_proto(command)?;
            }
        }
        Ok(())
    })();

    if let Some(profiler) = profiler {
        if let Err(err) = profiler.finish() {
            eprintln!("profile: {err}");
        }
    }

    result
}

fn cmd_pathdb(command: PathdbCommands) -> Result<()> {
    match command {
        PathdbCommands::ExportAxi { input, out } => {
            cmd_pathdb_export_axi(&input, &out)?;
        }
        PathdbCommands::ExportModule { input, out, module } => {
            cmd_pathdb_export_module(&input, &out, module.as_deref())?;
        }
        PathdbCommands::ImportAxi { input, out } => {
            cmd_pathdb_import_axi(&input, &out)?;
        }
        PathdbCommands::ImportChunks { input, chunks, out } => {
            cmd_pathdb_import_chunks(&input, &chunks, &out)?;
        }
    }
    Ok(())
}

fn cmd_accept(command: AcceptedCommands) -> Result<()> {
    match command {
        AcceptedCommands::Init { dir } => {
            cmd_accept_init(&dir)?;
        }
        AcceptedCommands::Sync {
            from,
            dir,
            layer,
            include_checkpoints,
            include_logs,
            no_update_head,
            dry_run,
        } => {
            let layer = crate::store_sync::SyncLayer::parse(&layer)?;
            let stats = crate::store_sync::sync_snapshot_store_dirs(
                &from,
                &dir,
                layer,
                include_checkpoints,
                include_logs,
                !no_update_head,
                dry_run,
            )?;
            eprintln!(
                "{} synced snapshot store {} → {} (files={} bytes={})",
                "ok".green().bold(),
                from.display(),
                dir.display(),
                stats.files_copied,
                stats.bytes_copied
            );
        }
        AcceptedCommands::List {
            dir,
            layer,
            limit,
            full,
        } => {
            cmd_accept_list(&dir, &layer, limit, full)?;
        }
        AcceptedCommands::Show {
            dir,
            layer,
            snapshot,
            json,
            full,
        } => {
            cmd_accept_show(&dir, &layer, &snapshot, json, full)?;
        }
        AcceptedCommands::Promote {
            input,
            dir,
            message,
            quality,
        } => {
            let snapshot_id =
                accepted_plane::promote_reviewed_module(&input, &dir, message.as_deref(), &quality)?;
            eprintln!(
                "{} promoted module to accepted snapshot {}",
                "ok".green().bold(),
                snapshot_id
            );
            eprintln!(
                "next: {}",
                format!(
                    "axiograph db accept build-pathdb --dir {} --snapshot {} --out build/accepted.axpd",
                    dir.display(),
                    snapshot_id
                )
                .bold()
            );
            println!("{snapshot_id}");
        }
        AcceptedCommands::BuildPathdb { dir, snapshot, out } => {
            accepted_plane::build_pathdb_from_snapshot(&dir, &snapshot, &out)?;
            eprintln!("{} {}", "wrote".green().bold(), out.display().to_string().bold());
        }
        AcceptedCommands::PathdbCommit {
            dir,
            accepted_snapshot,
            chunks,
            proposals,
            message,
            timings,
            timings_json,
            path_index_depth,
        } => {
            if chunks.is_empty() && proposals.is_empty() {
                return Err(anyhow!(
                    "pathdb-commit requires at least one --chunks <file.json> or --proposals <file.json>"
                ));
            }
            let result = pathdb_wal::commit_pathdb_snapshot_with_overlays_with_options(
                &dir,
                &accepted_snapshot,
                &chunks,
                &proposals,
                message.as_deref(),
                pathdb_wal::PathdbCommitOptions {
                    timings,
                    timings_json,
                    path_index_depth,
                },
            )?;
            eprintln!(
                "{} committed {} WAL op(s) on accepted snapshot {} → pathdb snapshot {}",
                "ok".green().bold(),
                result.ops_added,
                result.accepted_snapshot_id,
                result.snapshot_id
            );
            eprintln!(
                "next: {}",
                format!(
                    "axiograph db accept pathdb-build --dir {} --snapshot {} --out build/accepted_wal.axpd",
                    dir.display(),
                    result.snapshot_id
                )
                .bold()
            );
            println!("{}", result.snapshot_id);
        }
        AcceptedCommands::PathdbBuild {
            dir,
            snapshot,
            out,
            timings,
            timings_json,
            rebuild,
            path_index_depth,
            update_checkpoint,
        } => {
            pathdb_wal::build_pathdb_from_pathdb_snapshot_with_options(
                &dir,
                &snapshot,
                &out,
                pathdb_wal::PathdbBuildOptions {
                    timings,
                    timings_json,
                    rebuild,
                    path_index_depth,
                    update_checkpoint,
                },
            )?;
            eprintln!("{} {}", "wrote".green().bold(), out.display().to_string().bold());
        }
        AcceptedCommands::PathdbEmbed {
            dir,
            snapshot,
            target,
            embed_backend,
            ollama_host,
            embed_model,
            openai_base_url,
            max_items,
            batch_size,
            timeout_secs,
            message,
        } => {
            cmd_accept_pathdb_embed(
                &dir,
                &snapshot,
                &target,
                embed_backend.as_str(),
                ollama_host.as_deref(),
                embed_model.as_deref(),
                openai_base_url.as_deref(),
                max_items,
                batch_size,
                timeout_secs,
                message.as_deref(),
            )?;
        }
        AcceptedCommands::Status { dir } => {
            cmd_accept_status(&dir)?;
        }
        AcceptedCommands::Log { dir, layer, limit } => {
            cmd_accept_log(&dir, &layer, limit)?;
        }
    }

    Ok(())
}

// =============================================================================
// Accepted plane / snapshot store CLI helpers
// =============================================================================

fn now_unix_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn format_age_ago(created_at_unix_secs: u64) -> String {
    let now = now_unix_secs();
    let delta = if created_at_unix_secs > now {
        0
    } else {
        now.saturating_sub(created_at_unix_secs)
    };
    let days = delta / 86_400;
    let hours = (delta % 86_400) / 3_600;
    let mins = (delta % 3_600) / 60;
    if days > 0 {
        format!("{days}d{hours}h ago")
    } else if hours > 0 {
        format!("{hours}h{mins}m ago")
    } else if mins > 0 {
        format!("{mins}m ago")
    } else {
        format!("{delta}s ago")
    }
}

fn format_age_compact(created_at_unix_secs: u64) -> String {
    let now = now_unix_secs();
    let delta = if created_at_unix_secs > now {
        0
    } else {
        now.saturating_sub(created_at_unix_secs)
    };
    let days = delta / 86_400;
    let hours = (delta % 86_400) / 3_600;
    let mins = (delta % 3_600) / 60;
    if days > 0 {
        format!("{days}d{hours}h")
    } else if hours > 0 {
        format!("{hours}h{mins}m")
    } else if mins > 0 {
        format!("{mins}m")
    } else {
        format!("{delta}s")
    }
}

fn short_snapshot_id(id: &str) -> String {
    let (prefix, rest) = id.split_once(':').unwrap_or(("", id));
    let rest = rest.chars().take(12).collect::<String>();
    if prefix.is_empty() {
        rest
    } else {
        format!("{prefix}:{rest}")
    }
}

fn format_snapshot_id(id: &str, full: bool) -> String {
    if full {
        id.to_string()
    } else {
        short_snapshot_id(id)
    }
}

fn snapshot_id_filename(id: &str) -> String {
    id.replace(':', "_")
}

fn cmd_accept_init(dir: &PathBuf) -> Result<()> {
    accepted_plane::init_accepted_plane_dir(dir)?;
    pathdb_wal::init_pathdb_wal_dir(dir)?;
    println!("ok: initialized snapshot store at {}", dir.display());
    println!(
        "  next: axiograph db accept promote <module.axi> --dir {}",
        dir.display()
    );
    Ok(())
}

fn cmd_accept_list(dir: &PathBuf, layer: &str, limit: usize, full: bool) -> Result<()> {
    use std::fs;

    fn read_head(path: &std::path::Path) -> Option<String> {
        let text = fs::read_to_string(path).ok()?;
        let id = text.trim().to_string();
        if id.is_empty() {
            None
        } else {
            Some(id)
        }
    }

    fn read_json_files(dir: &std::path::Path) -> Vec<String> {
        let Ok(rd) = fs::read_dir(dir) else {
            return Vec::new();
        };
        rd.filter_map(|e| e.ok())
            .filter(|e| e.file_type().ok().map(|t| t.is_file()).unwrap_or(false))
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
            .filter_map(|e| fs::read_to_string(e.path()).ok())
            .collect()
    }

    let layer = layer.trim().to_ascii_lowercase();
    let show_accepted = layer == "accepted" || layer == "both";
    let show_pathdb = layer == "pathdb" || layer == "both";
    if !show_accepted && !show_pathdb {
        return Err(anyhow!(
            "unknown --layer `{}` (expected accepted|pathdb|both)",
            layer
        ));
    }

    if show_accepted {
        let head = read_head(&dir.join("HEAD"));
        let mut snaps: Vec<accepted_plane::AcceptedPlaneSnapshotV1> =
            read_json_files(&dir.join("snapshots"))
                .into_iter()
                .filter_map(|text| serde_json::from_str(&text).ok())
                .collect();
        snaps.sort_by_key(|s| std::cmp::Reverse(s.created_at_unix_secs));

        println!("accepted snapshots (dir={}):", dir.display());
        for s in snaps.into_iter().take(limit) {
            let mark = head
                .as_deref()
                .map(|h| if h == s.snapshot_id { "*" } else { " " })
                .unwrap_or(" ");
            let prev = s
                .previous_snapshot_id
                .as_deref()
                .map(|p| format_snapshot_id(p, full))
                .unwrap_or_else(|| "(none)".to_string());
            println!(
                " {mark} {} age={} modules={} prev={}",
                format_snapshot_id(&s.snapshot_id, full),
                format_age_compact(s.created_at_unix_secs),
                s.modules.len(),
                prev
            );
        }
        if head.is_none() {
            println!("  (no HEAD yet; run `axiograph db accept promote ...`)");
        }
        println!();
    }

    if show_pathdb {
        let pathdb_dir = dir.join("pathdb");
        let head = read_head(&pathdb_dir.join("HEAD"));
        let mut snaps: Vec<pathdb_wal::PathDbSnapshotV1> =
            read_json_files(&pathdb_dir.join("snapshots"))
                .into_iter()
                .filter_map(|text| serde_json::from_str(&text).ok())
                .collect();
        snaps.sort_by_key(|s| std::cmp::Reverse(s.created_at_unix_secs));

        println!("pathdb snapshots (dir={}):", pathdb_dir.display());
        for s in snaps.into_iter().take(limit) {
            let mark = head
                .as_deref()
                .map(|h| if h == s.snapshot_id { "*" } else { " " })
                .unwrap_or(" ");
            let prev = s
                .previous_snapshot_id
                .as_deref()
                .map(|p| format_snapshot_id(p, full))
                .unwrap_or_else(|| "(none)".to_string());
            println!(
                " {mark} {} age={} base={} ops={} prev={}",
                format_snapshot_id(&s.snapshot_id, full),
                format_age_compact(s.created_at_unix_secs),
                format_snapshot_id(&s.accepted_snapshot_id, full),
                s.ops.len(),
                prev
            );
        }
        if head.is_none() {
            println!("  (no HEAD yet; run `axiograph db accept pathdb-commit ...`)");
        }
    }

    Ok(())
}

fn cmd_accept_show(
    dir: &PathBuf,
    layer: &str,
    snapshot: &str,
    json: bool,
    full: bool,
) -> Result<()> {
    let layer = layer.trim().to_ascii_lowercase();
    match layer.as_str() {
        "accepted" => {
            let snap = accepted_plane::read_snapshot_for_cli(dir, snapshot)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&snap)?);
                return Ok(());
            }
            println!(
                "accepted snapshot {}",
                format_snapshot_id(&snap.snapshot_id, full)
            );
            println!(
                "  prev: {}",
                snap.previous_snapshot_id
                    .as_deref()
                    .map(|p| format_snapshot_id(p, full))
                    .unwrap_or_else(|| "(none)".to_string())
            );
            println!("  created_at_unix_secs: {}", snap.created_at_unix_secs);
            println!("  age: {}", format_age_ago(snap.created_at_unix_secs));
            println!("  modules: {}", snap.modules.len());
            for (name, m) in snap.modules {
                println!(
                    "    - {} digest={} path={}",
                    name,
                    format_snapshot_id(&m.module_digest, full),
                    m.stored_path
                );
            }
            Ok(())
        }
        "pathdb" => {
            let snap = pathdb_wal::read_pathdb_snapshot_for_cli(dir, snapshot)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&snap)?);
                return Ok(());
            }
            println!(
                "pathdb snapshot {}",
                format_snapshot_id(&snap.snapshot_id, full)
            );
            println!(
                "  prev: {}",
                snap.previous_snapshot_id
                    .as_deref()
                    .map(|p| format_snapshot_id(p, full))
                    .unwrap_or_else(|| "(none)".to_string())
            );
            println!(
                "  base_accepted_snapshot: {}",
                format_snapshot_id(&snap.accepted_snapshot_id, full)
            );
            println!("  created_at_unix_secs: {}", snap.created_at_unix_secs);
            println!("  age: {}", format_age_ago(snap.created_at_unix_secs));

            let checkpoint = dir
                .join("pathdb")
                .join("checkpoints")
                .join(format!("{}.axpd", snapshot_id_filename(&snap.snapshot_id)));
            println!(
                "  checkpoint: {}",
                if checkpoint.exists() {
                    checkpoint.display().to_string()
                } else {
                    "(missing)".to_string()
                }
            );

            println!("  ops: {}", snap.ops.len());
            for (idx, op) in snap.ops.iter().enumerate() {
                match op {
                    pathdb_wal::PathDbWalOpV1::ImportChunksV1 {
                        chunks_digest,
                        stored_path,
                    } => {
                        println!(
                            "    {}. import_chunks_v1 digest={} path={}",
                            idx + 1,
                            format_snapshot_id(chunks_digest, full),
                            stored_path
                        );
                    }
                    pathdb_wal::PathDbWalOpV1::ImportEmbeddingsV1 {
                        embeddings_digest,
                        stored_path,
                    } => {
                        println!(
                            "    {}. import_embeddings_v1 digest={} path={}",
                            idx + 1,
                            format_snapshot_id(embeddings_digest, full),
                            stored_path
                        );
                    }
                    pathdb_wal::PathDbWalOpV1::ImportProposalsV1 {
                        proposals_digest,
                        stored_path,
                    } => {
                        println!(
                            "    {}. import_proposals_v1 digest={} path={}",
                            idx + 1,
                            format_snapshot_id(proposals_digest, full),
                            stored_path
                        );
                    }
                }
            }
            Ok(())
        }
        other => Err(anyhow!(
            "unknown --layer `{other}` (expected accepted|pathdb)"
        )),
    }
}

fn cmd_accept_status(dir: &PathBuf) -> Result<()> {
    use std::fs;

    fn read_head(path: &std::path::Path) -> Option<String> {
        let text = fs::read_to_string(path).ok()?;
        let id = text.trim().to_string();
        if id.is_empty() {
            None
        } else {
            Some(id)
        }
    }

    fn count_json_files(dir: &std::path::Path) -> usize {
        let Ok(rd) = fs::read_dir(dir) else {
            return 0;
        };
        rd.filter_map(|e| e.ok())
            .filter(|e| e.file_type().ok().map(|t| t.is_file()).unwrap_or(false))
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
            .count()
    }

    let accepted_head = read_head(&dir.join("HEAD"));
    let accepted_snapshots = count_json_files(&dir.join("snapshots"));
    let accepted_head_snapshot = accepted_plane::read_snapshot_for_cli(dir, "head").ok();
    let accepted_modules_in_head = accepted_head_snapshot.as_ref().map(|s| s.modules.len());

    let pathdb_dir = dir.join("pathdb");
    let pathdb_head = read_head(&pathdb_dir.join("HEAD"));
    let pathdb_snapshots = count_json_files(&pathdb_dir.join("snapshots"));
    let pathdb_head_snapshot = pathdb_wal::read_pathdb_snapshot_for_cli(dir, "head").ok();
    let pathdb_head_info = pathdb_head_snapshot.as_ref().map(|s| {
        (
            s.accepted_snapshot_id.clone(),
            s.ops.len(),
            s.created_at_unix_secs,
        )
    });

    println!("accepted_plane:");
    println!("  dir: {}", dir.display());
    println!(
        "  head: {}",
        accepted_head
            .as_deref()
            .map(|id| format!("{} ({})", short_snapshot_id(id), id))
            .unwrap_or_else(|| "(none)".to_string())
    );
    println!("  snapshots: {accepted_snapshots}");
    if let Some(n) = accepted_modules_in_head {
        let age = accepted_head_snapshot
            .as_ref()
            .map(|s| format_age_ago(s.created_at_unix_secs))
            .unwrap_or_default();
        if age.is_empty() {
            println!("  modules_in_head: {n}");
        } else {
            println!("  modules_in_head: {n} ({age})");
        }
    }

    println!("pathdb_wal:");
    println!(
        "  head: {}",
        pathdb_head
            .as_deref()
            .map(|id| format!("{} ({})", short_snapshot_id(id), id))
            .unwrap_or_else(|| "(none)".to_string())
    );
    println!("  snapshots: {pathdb_snapshots}");
    if let Some((base, ops, created_at)) = pathdb_head_info {
        println!(
            "  base_accepted_snapshot: {} ({})",
            short_snapshot_id(&base),
            base
        );
        println!("  ops_total: {ops}");
        println!("  age: {}", format_age_ago(created_at));
    }

    if let (Some(accepted_id), Some(pathdb_snap)) = (accepted_head.as_deref(), pathdb_head_snapshot)
    {
        if pathdb_snap.accepted_snapshot_id != accepted_id {
            println!(
                "note: pathdb WAL HEAD is based on an older accepted snapshot (base={} head={}).",
                short_snapshot_id(&pathdb_snap.accepted_snapshot_id),
                short_snapshot_id(accepted_id)
            );
            println!(
                "      run: axiograph db accept pathdb-commit --dir {} --accepted-snapshot head --chunks <file.json>",
                dir.display()
            );
        }
    }

    Ok(())
}

fn cmd_accept_log(dir: &PathBuf, layer: &str, limit: usize) -> Result<()> {
    use std::fs;

    fn read_jsonl_lines(path: &std::path::Path) -> Result<Vec<String>> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let text = fs::read_to_string(path)?;
        Ok(text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect())
    }

    let layer = layer.trim().to_ascii_lowercase();
    let show_accepted = layer == "accepted" || layer == "both";
    let show_pathdb = layer == "pathdb" || layer == "both";
    if !show_accepted && !show_pathdb {
        return Err(anyhow!(
            "unknown --layer `{}` (expected accepted|pathdb|both)",
            layer
        ));
    }

    if show_accepted {
        let path = dir.join("accepted_plane.log.jsonl");
        let mut lines = read_jsonl_lines(&path)?;
        if lines.is_empty() {
            println!("accepted_plane log: (empty)");
        } else {
            println!("accepted_plane log:");
            let start = lines.len().saturating_sub(limit);
            for line in lines.drain(start..) {
                match serde_json::from_str::<accepted_plane::AcceptedPlaneEventV1>(&line) {
                    Ok(e) => {
                        let msg = e.message.unwrap_or_default();
                        let age = format_age_compact(e.created_at_unix_secs);
                        if msg.is_empty() {
                            println!(
                                "  {} {} {} module={} snapshot={} prev={}",
                                e.created_at_unix_secs,
                                age,
                                e.action,
                                e.module_name,
                                short_snapshot_id(&e.snapshot_id),
                                e.previous_snapshot_id
                                    .as_deref()
                                    .map(short_snapshot_id)
                                    .unwrap_or_else(|| "(none)".to_string())
                            );
                        } else {
                            println!(
                                "  {} {} {} module={} snapshot={} prev={} msg={}",
                                e.created_at_unix_secs,
                                age,
                                e.action,
                                e.module_name,
                                short_snapshot_id(&e.snapshot_id),
                                e.previous_snapshot_id
                                    .as_deref()
                                    .map(short_snapshot_id)
                                    .unwrap_or_else(|| "(none)".to_string()),
                                msg
                            );
                        }
                    }
                    Err(_) => {
                        // Preserve raw JSON on parse failures so users can still inspect it.
                        println!("  {line}");
                    }
                }
            }
        }
    }

    if show_pathdb {
        let path = dir.join("pathdb").join("pathdb_wal.log.jsonl");
        let mut lines = read_jsonl_lines(&path)?;
        if lines.is_empty() {
            println!("pathdb_wal log: (empty)");
        } else {
            println!("pathdb_wal log:");
            let start = lines.len().saturating_sub(limit);
            for line in lines.drain(start..) {
                match serde_json::from_str::<pathdb_wal::PathDbWalEventV1>(&line) {
                    Ok(e) => {
                        let msg = e.message.unwrap_or_default();
                        let age = format_age_compact(e.created_at_unix_secs);
                        let ops = e.ops_appended.len();
                        if msg.is_empty() {
                            println!(
                                "  {} {} {} snapshot={} base={} ops+={} prev={}",
                                e.created_at_unix_secs,
                                age,
                                e.action,
                                short_snapshot_id(&e.snapshot_id),
                                short_snapshot_id(&e.accepted_snapshot_id),
                                ops,
                                e.previous_snapshot_id
                                    .as_deref()
                                    .map(short_snapshot_id)
                                    .unwrap_or_else(|| "(none)".to_string())
                            );
                        } else {
                            println!(
                                "  {} {} {} snapshot={} base={} ops+={} prev={} msg={}",
                                e.created_at_unix_secs,
                                age,
                                e.action,
                                short_snapshot_id(&e.snapshot_id),
                                short_snapshot_id(&e.accepted_snapshot_id),
                                ops,
                                e.previous_snapshot_id
                                    .as_deref()
                                    .map(short_snapshot_id)
                                    .unwrap_or_else(|| "(none)".to_string()),
                                msg
                            );
                        }
                    }
                    Err(_) => {
                        println!("  {line}");
                    }
                }
            }
        }
    }

    Ok(())
}

fn cmd_accept_pathdb_embed(
    dir: &PathBuf,
    base_snapshot: &str,
    target: &str,
    embed_backend: &str,
    ollama_host: Option<&str>,
    embed_model: Option<&str>,
    openai_base_url: Option<&str>,
    max_items: usize,
    batch_size: usize,
    timeout_secs: Option<u64>,
    message: Option<&str>,
) -> Result<()> {
    let target = target.trim().to_ascii_lowercase();
    let want_docchunks = target == "docchunks" || target == "doc_chunks" || target == "both";
    let want_entities = target == "entities" || target == "both";
    if !want_docchunks && !want_entities {
        return Err(anyhow!(
            "invalid --target `{}` (expected docchunks|entities|both)",
            target
        ));
    }

    let embed_backend = embed_backend.trim().to_ascii_lowercase();
    let embed_model = embed_model.map(|s| s.trim()).filter(|s| !s.is_empty());
    let embed_model = match (embed_backend.as_str(), embed_model) {
        ("ollama", Some(m)) => m.to_string(),
        ("ollama", None) => "nomic-embed-text".to_string(),
        ("openai", Some(m)) => m.to_string(),
        ("openai", None) => "text-embedding-3-small".to_string(),
        ("anthropic", _) => {
            return Err(anyhow!(
                "anthropic does not provide embeddings; use `--embed-backend openai` or rely on deterministic retrieval"
            ));
        }
        _ => {
            return Err(anyhow!(
                "invalid --embed-backend `{}` (expected ollama|openai)",
                embed_backend
            ))
        }
    };

    use crate::embeddings::{
        EmbeddingItemV1, EmbeddingKeyV1, EmbeddingTargetKindV1, EmbeddingsFileV1,
        EMBEDDINGS_FILE_VERSION_V1,
    };

    let base = pathdb_wal::read_pathdb_snapshot_for_cli(dir, base_snapshot)?;

    let checkpoint = dir
        .join("pathdb")
        .join("checkpoints")
        .join(format!("{}.axpd", snapshot_id_filename(&base.snapshot_id)));
    let bytes = if checkpoint.exists() {
        fs::read(&checkpoint)?
    } else {
        // Rare path: no checkpoint present; rebuild into a temp `.axpd` file.
        fs::create_dir_all(dir.join("pathdb").join("tmp"))?;
        let tmp = dir.join("pathdb").join("tmp").join("embed_tmp.axpd");
        pathdb_wal::build_pathdb_from_pathdb_snapshot(dir, &base.snapshot_id, &tmp)?;
        let bytes = fs::read(&tmp)?;
        let _ = fs::remove_file(&tmp);
        bytes
    };

    let db = axiograph_pathdb::PathDB::from_bytes(&bytes)?;

    let timeout = crate::llm::llm_timeout(timeout_secs)?;

    #[allow(unused_variables)]
    let resolved_ollama_host: Option<String> = if embed_backend == "ollama" {
        #[cfg(feature = "llm-ollama")]
        {
            Some(
                ollama_host
                    .map(|s| s.to_string())
                    .unwrap_or_else(crate::llm::default_ollama_host),
            )
        }
        #[cfg(not(feature = "llm-ollama"))]
        {
            None
        }
    } else {
        None
    };

    #[allow(unused_variables)]
    let resolved_openai_base_url: Option<String> = if embed_backend == "openai" {
        #[cfg(feature = "llm-openai")]
        {
            Some(
                openai_base_url
                    .map(|s| s.to_string())
                    .unwrap_or_else(crate::llm::default_openai_base_url),
            )
        }
        #[cfg(not(feature = "llm-openai"))]
        {
            None
        }
    } else {
        None
    };

    fn db_attr(db: &axiograph_pathdb::PathDB, id: u32, key: &str) -> Option<String> {
        let view = db.get_entity(id)?;
        view.attrs.get(key).cloned()
    }

    fn truncate_chars(s: &str, max_chars: usize) -> String {
        if max_chars == 0 {
            return String::new();
        }
        if s.chars().count() <= max_chars {
            return s.to_string();
        }
        let mut out = String::new();
        out.extend(s.chars().take(max_chars));
        out.push('…');
        out
    }

    #[allow(clippy::needless_return)]
    fn embed_batches(
        embed_backend: &str,
        embed_model: &str,
        ollama_host: Option<&str>,
        openai_base_url: Option<&str>,
        texts: &[String],
        batch_size: usize,
        timeout: Option<Duration>,
    ) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let bs = batch_size.clamp(1, 256);
        let mut out: Vec<Vec<f32>> = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(bs) {
            match embed_backend {
                "ollama" => {
                    #[cfg(feature = "llm-ollama")]
                    {
                        let host = ollama_host.unwrap_or("http://127.0.0.1:11434");
                        let e = crate::llm::ollama_embed_texts_with_timeout(
                            host,
                            embed_model,
                            chunk,
                            timeout,
                        )?;
                        out.extend(e);
                    }
                    #[cfg(not(feature = "llm-ollama"))]
                    {
                        let _ = (ollama_host, embed_model, chunk, timeout);
                        return Err(anyhow!(
                            "ollama embeddings not available (compiled without `llm-ollama`)"
                        ));
                    }
                }
                "openai" => {
                    #[cfg(feature = "llm-openai")]
                    {
                        let base_url = openai_base_url.unwrap_or("https://api.openai.com");
                        let e = crate::llm::openai_embed_texts_with_timeout(
                            base_url,
                            embed_model,
                            chunk,
                            timeout,
                        )?;
                        out.extend(e);
                    }
                    #[cfg(not(feature = "llm-openai"))]
                    {
                        let _ = (openai_base_url, embed_model, chunk, timeout);
                        return Err(anyhow!(
                            "openai embeddings not available (compiled without `llm-openai`)"
                        ));
                    }
                }
                _ => {
                    return Err(anyhow!(
                        "invalid embed backend `{}` (expected ollama|openai)",
                        embed_backend
                    ))
                }
            }
        }
        Ok(out)
    }

    let mut blobs: Vec<Vec<u8>> = Vec::new();

    if want_docchunks {
        let Some(chunks) = db.find_by_type("DocChunk") else {
            return Err(anyhow!(
                "no DocChunk loaded in this snapshot; import chunks first, then embed (try: `axiograph db accept pathdb-commit --chunks <chunks.json> ...`)"
            ));
        };

        let mut keys: Vec<EmbeddingKeyV1> = Vec::new();
        let mut texts: Vec<String> = Vec::new();
        let mut digests: Vec<String> = Vec::new();

        for id in chunks.iter().take(max_items) {
            let chunk_id = db_attr(&db, id, "chunk_id").unwrap_or_else(|| id.to_string());
            let text = db_attr(&db, id, "text").unwrap_or_default();
            let search_text = db_attr(&db, id, "search_text").unwrap_or_default();
            let mut combined = String::new();
            combined.push_str(&text);
            if !search_text.trim().is_empty() {
                combined.push('\n');
                combined.push_str(&search_text);
            }
            let combined = truncate_chars(&combined, 2500);
            if combined.trim().is_empty() {
                continue;
            }
            let digest = axiograph_dsl::digest::fnv1a64_digest_bytes(combined.as_bytes());
            digests.push(digest);
            keys.push(EmbeddingKeyV1::DocChunk { chunk_id });
            texts.push(combined);
        }

        if keys.is_empty() {
            return Err(anyhow!("no docchunk text found to embed"));
        }

        eprintln!(
            "{} embedding docchunks (n={}) via {} model={}",
            "info:".yellow().bold(),
            keys.len(),
            embed_backend,
            embed_model
        );
    let vectors = embed_batches(
            &embed_backend,
            &embed_model,
            resolved_ollama_host.as_deref(),
            resolved_openai_base_url.as_deref(),
            &texts,
            batch_size,
            timeout,
        )?;
        if vectors.len() != keys.len() {
            return Err(anyhow!(
                "embed returned {} vectors for {} inputs",
                vectors.len(),
                keys.len()
            ));
        }
        let dim = vectors.first().map(|v| v.len()).unwrap_or(0);
        if dim == 0 {
            return Err(anyhow!("embed returned empty vectors"));
        }

        let items = keys
            .into_iter()
            .zip(vectors)
            .zip(digests)
            .map(|((key, vector), text_digest)| EmbeddingItemV1 {
                key,
                vector,
                text_digest: Some(text_digest),
            })
            .collect::<Vec<_>>();

        let file = EmbeddingsFileV1 {
            version: EMBEDDINGS_FILE_VERSION_V1.to_string(),
            created_at_unix_secs: now_unix_secs(),
            backend: embed_backend.to_string(),
            model: embed_model.to_string(),
            dim,
            target: EmbeddingTargetKindV1::DocChunks,
            items,
            metadata: std::collections::HashMap::from([
                ("base_pathdb_snapshot".to_string(), base.snapshot_id.clone()),
                (
                    "base_accepted_snapshot".to_string(),
                    base.accepted_snapshot_id.clone(),
                ),
            ]),
        };
        blobs.push(crate::embeddings::encode_embeddings_file_v1(&file)?);
    }

    if want_entities {
        let mut keys: Vec<EmbeddingKeyV1> = Vec::new();
        let mut texts: Vec<String> = Vec::new();
        let mut digests: Vec<String> = Vec::new();

        for id in 0..(db.entities.len() as u32) {
            let Some(view) = db.get_entity(id) else { continue };
            if view.entity_type == "DocChunk"
                || view.entity_type == "Document"
                || view.entity_type.starts_with("AxiMeta")
            {
                continue;
            }
            let Some(name) = view.attrs.get("name").cloned() else {
                continue;
            };

            let mut text = String::new();
            text.push_str(&view.entity_type);
            text.push(' ');
            text.push_str(&name);
            for k in ["search_text", "description", "comment", "iri"] {
                if let Some(v) = view.attrs.get(k) {
                    if !v.trim().is_empty() {
                        text.push(' ');
                        text.push_str(v);
                    }
                }
            }

            let text = truncate_chars(&text, 1500);
            if text.trim().is_empty() {
                continue;
            }

            let digest = axiograph_dsl::digest::fnv1a64_digest_bytes(text.as_bytes());
            digests.push(digest);
            keys.push(EmbeddingKeyV1::Entity {
                entity_type: view.entity_type.to_string(),
                name,
            });
            texts.push(text);

            if keys.len() >= max_items {
                break;
            }
        }

        if keys.is_empty() {
            return Err(anyhow!("no entities found to embed (no `name` attrs?)"));
        }

        eprintln!(
            "{} embedding entities (n={}) via {} model={}",
            "info:".yellow().bold(),
            keys.len(),
            embed_backend,
            embed_model
        );
    let vectors = embed_batches(
            &embed_backend,
            &embed_model,
            resolved_ollama_host.as_deref(),
            resolved_openai_base_url.as_deref(),
            &texts,
            batch_size,
            timeout,
        )?;
        if vectors.len() != keys.len() {
            return Err(anyhow!(
                "embed returned {} vectors for {} inputs",
                vectors.len(),
                keys.len()
            ));
        }
        let dim = vectors.first().map(|v| v.len()).unwrap_or(0);
        if dim == 0 {
            return Err(anyhow!("embed returned empty vectors"));
        }

        let items = keys
            .into_iter()
            .zip(vectors)
            .zip(digests)
            .map(|((key, vector), text_digest)| EmbeddingItemV1 {
                key,
                vector,
                text_digest: Some(text_digest),
            })
            .collect::<Vec<_>>();

        let file = EmbeddingsFileV1 {
            version: EMBEDDINGS_FILE_VERSION_V1.to_string(),
            created_at_unix_secs: now_unix_secs(),
            backend: embed_backend.to_string(),
            model: embed_model.to_string(),
            dim,
            target: EmbeddingTargetKindV1::Entities,
            items,
            metadata: std::collections::HashMap::from([
                ("base_pathdb_snapshot".to_string(), base.snapshot_id.clone()),
                (
                    "base_accepted_snapshot".to_string(),
                    base.accepted_snapshot_id.clone(),
                ),
            ]),
        };
        blobs.push(crate::embeddings::encode_embeddings_file_v1(&file)?);
    }

    let result = pathdb_wal::commit_pathdb_snapshot_with_embedding_bytes(
        dir,
        &base.snapshot_id,
        &blobs,
        message,
    )?;
    eprintln!(
        "{} committed embeddings ops={} base_pathdb_snapshot={} → pathdb_snapshot={}",
        "ok".green().bold(),
        result.ops_added,
        short_snapshot_id(&base.snapshot_id),
        short_snapshot_id(&result.snapshot_id)
    );
    println!("{}", result.snapshot_id);
    Ok(())
}

fn cmd_query_cert(
    input: &PathBuf,
    lang: &str,
    query_text: &str,
    out: Option<&PathBuf>,
    anchor_out: Option<&PathBuf>,
) -> Result<()> {
    let axi_text = fs::read_to_string(input)?;
    let digest = axiograph_dsl::digest::axi_digest_v1(&axi_text);

    let m = axiograph_dsl::axi_v1::parse_axi_v1(&axi_text)?;

    let is_snapshot = m
        .schemas
        .iter()
        .any(|s| s.name == axiograph_pathdb::axi_export::PATHDB_EXPORT_SCHEMA_NAME_V1)
        && m.instances.iter().any(|i| {
            i.schema == axiograph_pathdb::axi_export::PATHDB_EXPORT_SCHEMA_NAME_V1
                && i.name == axiograph_pathdb::axi_export::PATHDB_EXPORT_INSTANCE_NAME_V1
        });

    let (db, anchor_digest, is_pathdb_export_anchor) = if is_snapshot {
        (
            axiograph_pathdb::axi_export::import_pathdb_from_axi_v1_module(&m)?,
            digest,
            true,
        )
    } else {
        let mut db = axiograph_pathdb::PathDB::new();
        let _summary =
            axiograph_pathdb::axi_module_import::import_axi_schema_v1_module_into_pathdb(
                &mut db, &m,
            )?;
        db.build_indexes();
        if let Some(anchor_path) = anchor_out {
            // Optional convenience export for debugging / legacy workflows.
            // Certificates are still anchored to the canonical `.axi` digest.
            let anchor_text = axiograph_pathdb::axi_export::export_pathdb_to_axi_v1(&db)?;
            let anchor_digest = axiograph_dsl::digest::axi_digest_v1(&anchor_text);
            fs::write(anchor_path, anchor_text)?;
            eprintln!(
                "wrote derived PathDBExportV1 export {} (digest={})",
                anchor_path.display(),
                anchor_digest
            );
        }

        (db, digest, false)
    };
    let query = match lang {
        "axql" => crate::axql::parse_axql_query(query_text)?,
        "sql" => crate::sqlish::parse_sqlish_query(query_text)?,
        other => {
            return Err(anyhow::anyhow!(
                "unknown --lang `{other}` (expected `axql` or `sql`)"
            ))
        }
    };

    let cert = if is_pathdb_export_anchor {
        crate::axql::certify_axql_query(&db, &query)?
    } else {
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;
        crate::axql::certify_axql_query_v3_with_meta(&db, &query, Some(&meta), &anchor_digest)?
    }
    .with_anchor(axiograph_pathdb::certificate::AxiAnchorV1 {
        axi_digest_v1: anchor_digest,
    });

    let json = serde_json::to_string_pretty(&cert)?;
    match out {
        Some(path) => {
            fs::write(path, json)?;
            println!("wrote {}", path.display());
        }
        None => {
            println!("{json}");
        }
    }

    Ok(())
}

fn cmd_typecheck_cert(input: &PathBuf, out: Option<&PathBuf>) -> Result<()> {
    let axi_text = fs::read_to_string(input)?;
    let digest = axiograph_dsl::digest::axi_digest_v1(&axi_text);

    let m = axiograph_dsl::axi_v1::parse_axi_v1(&axi_text)?;
    let (_m, proof) =
        axiograph_pathdb::axi_module_typecheck::TypedAxiV1Module::new(m)?.into_parts();

    let cert = axiograph_pathdb::certificate::CertificateV2::axi_well_typed_v1(proof).with_anchor(
        axiograph_pathdb::certificate::AxiAnchorV1 {
            axi_digest_v1: digest,
        },
    );

    let json = serde_json::to_string_pretty(&cert)?;
    match out {
        Some(path) => {
            fs::write(path, json)?;
            println!("wrote {}", path.display());
        }
        None => {
            println!("{json}");
        }
    }

    Ok(())
}

fn cmd_constraints_cert(input: &PathBuf, out: Option<&PathBuf>) -> Result<()> {
    let axi_text = fs::read_to_string(input)?;
    let digest = axiograph_dsl::digest::axi_digest_v1(&axi_text);

    let m = axiograph_dsl::axi_v1::parse_axi_v1(&axi_text)?;
    let typed = axiograph_pathdb::axi_module_typecheck::TypedAxiV1Module::new(m)?;
    let proof =
        axiograph_pathdb::axi_module_constraints::check_axi_constraints_ok_v1(typed.module())?;

    let cert = axiograph_pathdb::certificate::CertificateV2::axi_constraints_ok_v1(proof)
        .with_anchor(axiograph_pathdb::certificate::AxiAnchorV1 {
            axi_digest_v1: digest,
        });

    let json = serde_json::to_string_pretty(&cert)?;
    match out {
        Some(path) => {
            fs::write(path, json)?;
            println!("wrote {}", path.display());
        }
        None => {
            println!("{json}");
        }
    }

    Ok(())
}

fn is_pathdb_export_v1_module(m: &axiograph_dsl::schema_v1::SchemaV1Module) -> bool {
    m.schemas
        .iter()
        .any(|s| s.name == axiograph_pathdb::axi_export::PATHDB_EXPORT_SCHEMA_NAME_V1)
        && m.instances.iter().any(|i| {
            i.schema == axiograph_pathdb::axi_export::PATHDB_EXPORT_SCHEMA_NAME_V1
                && i.name == axiograph_pathdb::axi_export::PATHDB_EXPORT_INSTANCE_NAME_V1
        })
}

pub(crate) fn load_pathdb_for_cli(input: &PathBuf) -> Result<axiograph_pathdb::PathDB> {
    let ext = input.extension().and_then(|s| s.to_str()).unwrap_or("");
    if ext.eq_ignore_ascii_case("axpd") {
        let bytes = fs::read(input)?;
        return Ok(axiograph_pathdb::PathDB::from_bytes(&bytes)?);
    }
    if ext.eq_ignore_ascii_case("axi") {
        let text = fs::read_to_string(input)?;
        let m = axiograph_dsl::axi_v1::parse_axi_v1(&text)?;
        if is_pathdb_export_v1_module(&m) {
            return Ok(axiograph_pathdb::axi_export::import_pathdb_from_axi_v1_module(&m)?);
        }
        let mut db = axiograph_pathdb::PathDB::new();
        let _summary =
            axiograph_pathdb::axi_module_import::import_axi_schema_v1_module_into_pathdb(
                &mut db, &m,
            )?;
        db.build_indexes();
        return Ok(db);
    }
    Err(anyhow!(
        "unsupported input `{}` (expected .axpd or .axi)",
        input.display()
    ))
}

fn cmd_viz_from_args(args: &VizArgs) -> Result<()> {
    cmd_viz(
        &args.input,
        &args.out,
        &args.format,
        &args.plane,
        &args.focus_id,
        args.focus_name.as_deref(),
        args.focus_type.as_deref(),
        args.hops,
        args.max_nodes,
        args.max_edges,
        &args.direction,
        args.include_meta,
        args.typed_overlay,
        !args.no_equivalences,
    )
}

fn cmd_viz(
    input: &PathBuf,
    out: &PathBuf,
    format: &str,
    plane: &str,
    focus_id: &[u32],
    focus_name: Option<&str>,
    focus_type: Option<&str>,
    hops: usize,
    max_nodes: usize,
    max_edges: usize,
    direction: &str,
    include_meta: bool,
    typed_overlay: bool,
    include_equivalences: bool,
) -> Result<()> {
    let db = load_pathdb_for_cli(input)?;

    let format = crate::viz::VizFormat::parse(format)?;
    let direction = crate::viz::VizDirection::parse(direction)?;

    let plane = plane.trim().to_ascii_lowercase();
    let (include_meta_plane, include_data_plane) = match plane.as_str() {
        "data" => (include_meta, true),
        "meta" => (true, false),
        "both" => (true, true),
        other => return Err(anyhow!("unknown plane `{other}` (expected data|meta|both)")),
    };

    let mut focus: Vec<u32> = focus_id.to_vec();
    if focus.is_empty() {
        if let Some(name) = focus_name {
            if let Some(id) = crate::viz::resolve_focus_by_name_and_type(&db, name, focus_type)? {
                focus.push(id);
            } else {
                return Err(anyhow!(
                    "no entity found with name `{}` (tip: try `axiograph repl` + `find_by_type`/`q` to locate ids)",
                    name
                ));
            }
        }
    }
    if focus.is_empty() {
        eprintln!("note: no focus specified; using a fallback node");
    }

    let options = crate::viz::VizOptions {
        focus_ids: focus,
        hops,
        max_nodes,
        max_edges,
        direction,
        include_meta_plane,
        include_data_plane,
        include_equivalences,
        typed_overlay,
    };

    let g = crate::viz::extract_viz_graph(&db, &options)?;

    let rendered = match format {
        crate::viz::VizFormat::Dot => crate::viz::render_dot(&db, &g),
        crate::viz::VizFormat::Json => crate::viz::render_json(&g)?,
        crate::viz::VizFormat::Html => crate::viz::render_html(&db, &g)?,
    };

    fs::write(out, rendered)?;
    println!(
        "wrote {} (nodes={} edges={} truncated={})",
        out.display(),
        g.nodes.len(),
        g.edges.len(),
        g.truncated
    );
    Ok(())
}

fn sanitize_id_component(s: &str) -> String {
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

fn proposals_from_sql_schema(
    sql_schema: &axiograph_ingest_sql::SqlSchema,
    evidence_locator: Option<String>,
    chunks: &[axiograph_ingest_docs::Chunk],
) -> Vec<axiograph_ingest_docs::ProposalV1> {
    use axiograph_ingest_docs::{EvidencePointer, ProposalMetaV1, ProposalV1};
    use std::collections::HashMap;

    fn evidence_for_keywords(
        chunks: &[axiograph_ingest_docs::Chunk],
        locator: Option<&String>,
        keywords: &[&str],
    ) -> Vec<EvidencePointer> {
        if chunks.is_empty() {
            return Vec::new();
        }
        let keywords: Vec<String> = keywords
            .iter()
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        if keywords.is_empty() {
            return Vec::new();
        }

        let mut best: Option<&axiograph_ingest_docs::Chunk> = None;
        for c in chunks {
            let text = c.text.to_ascii_lowercase();
            if keywords.iter().all(|k| text.contains(k)) {
                best = Some(c);
                break;
            }
        }
        if best.is_none() {
            let first = &keywords[0];
            best = chunks
                .iter()
                .find(|c| c.text.to_ascii_lowercase().contains(first));
        }
        let Some(best) = best else {
            return Vec::new();
        };

        vec![EvidencePointer {
            chunk_id: best.chunk_id.clone(),
            locator: locator.cloned(),
            span_id: Some(best.span_id.clone()),
        }]
    }

    let mut out: Vec<ProposalV1> = Vec::new();

    for table in &sql_schema.tables {
        let table_id = format!("sql_table::{}", sanitize_id_component(&table.name));

        let mut attrs = HashMap::new();
        attrs.insert("table".to_string(), table.name.clone());
        if !table.primary_key.is_empty() {
            attrs.insert("primary_key".to_string(), table.primary_key.join(", "));
        }

        let evidence = evidence_for_keywords(
            chunks,
            evidence_locator.as_ref(),
            &[&table.name, "create table"],
        );

        out.push(ProposalV1::Entity {
            meta: ProposalMetaV1 {
                proposal_id: table_id.clone(),
                confidence: 0.98,
                evidence: evidence.clone(),
                public_rationale: "Parsed table from SQL DDL.".to_string(),
                metadata: HashMap::new(),
                schema_hint: Some("sql".to_string()),
            },
            entity_id: table_id.clone(),
            entity_type: "SqlTable".to_string(),
            name: table.name.clone(),
            attributes: attrs,
            description: None,
        });

        for col in &table.columns {
            let col_id = format!(
                "sql_column::{}::{}",
                sanitize_id_component(&table.name),
                sanitize_id_component(&col.name)
            );

            let mut col_attrs = HashMap::new();
            col_attrs.insert("table".to_string(), table.name.clone());
            col_attrs.insert("column".to_string(), col.name.clone());
            col_attrs.insert("data_type".to_string(), col.data_type.clone());
            col_attrs.insert("nullable".to_string(), col.nullable.to_string());

            out.push(ProposalV1::Entity {
                meta: ProposalMetaV1 {
                    proposal_id: col_id.clone(),
                    confidence: 0.98,
                    evidence: evidence_for_keywords(
                        chunks,
                        evidence_locator.as_ref(),
                        &[&table.name, &col.name],
                    ),
                    public_rationale: "Parsed column from SQL DDL.".to_string(),
                    metadata: HashMap::new(),
                    schema_hint: Some("sql".to_string()),
                },
                entity_id: col_id.clone(),
                entity_type: "SqlColumn".to_string(),
                name: format!("{}.{}", table.name, col.name),
                attributes: col_attrs,
                description: None,
            });

            let rel_id = format!(
                "sql_rel::has_column::{}::{}",
                sanitize_id_component(&table_id),
                sanitize_id_component(&col_id)
            );

            out.push(ProposalV1::Relation {
                meta: ProposalMetaV1 {
                    proposal_id: rel_id.clone(),
                    confidence: 0.98,
                    evidence: evidence.clone(),
                    public_rationale: "Derived HasColumn from parsed SQL DDL.".to_string(),
                    metadata: HashMap::new(),
                    schema_hint: Some("sql".to_string()),
                },
                relation_id: rel_id,
                rel_type: "SqlHasColumn".to_string(),
                source: table_id.clone(),
                target: col_id,
                attributes: HashMap::new(),
            });
        }
    }

    for fk in &sql_schema.foreign_keys {
        let from_table_id = format!("sql_table::{}", sanitize_id_component(&fk.from_table));
        let to_table_id = format!("sql_table::{}", sanitize_id_component(&fk.to_table));
        let rel_id = format!(
            "sql_rel::foreign_key::{}::{}",
            sanitize_id_component(&from_table_id),
            sanitize_id_component(&to_table_id)
        );

        let mut attrs = HashMap::new();
        attrs.insert("from_columns".to_string(), fk.from_columns.join(", "));
        attrs.insert("to_columns".to_string(), fk.to_columns.join(", "));

        out.push(ProposalV1::Relation {
            meta: ProposalMetaV1 {
                proposal_id: rel_id.clone(),
                confidence: 0.98,
                evidence: evidence_for_keywords(
                    chunks,
                    evidence_locator.as_ref(),
                    &[&fk.from_table, &fk.to_table, "foreign key"],
                ),
                public_rationale: "Parsed foreign key from SQL DDL.".to_string(),
                metadata: HashMap::new(),
                schema_hint: Some("sql".to_string()),
            },
            relation_id: rel_id,
            rel_type: "SqlForeignKey".to_string(),
            source: from_table_id,
            target: to_table_id,
            attributes: attrs,
        });
    }

    for uq in &sql_schema.unique_keys {
        let uq_id = format!(
            "sql_unique_key::{}::{}",
            sanitize_id_component(&uq.table),
            sanitize_id_component(&uq.columns.join("_"))
        );

        let mut attrs = HashMap::new();
        attrs.insert("table".to_string(), uq.table.clone());
        attrs.insert("columns".to_string(), uq.columns.join(", "));

        out.push(ProposalV1::Entity {
            meta: ProposalMetaV1 {
                proposal_id: uq_id.clone(),
                confidence: 0.98,
                evidence: evidence_for_keywords(
                    chunks,
                    evidence_locator.as_ref(),
                    &[&uq.table, "unique"],
                ),
                public_rationale: "Parsed unique key from SQL DDL.".to_string(),
                metadata: HashMap::new(),
                schema_hint: Some("sql".to_string()),
            },
            entity_id: uq_id,
            entity_type: "SqlUniqueKey".to_string(),
            name: format!("{}({})", uq.table, uq.columns.join(", ")),
            attributes: attrs,
            description: None,
        });
    }

    out
}

fn json_field_type_to_string(ft: &axiograph_ingest_json::JsonFieldType) -> String {
    use axiograph_ingest_json::JsonFieldType;
    match ft {
        JsonFieldType::Required(t) => format!("{t} (required)"),
        JsonFieldType::Optional(t) => format!("{t} (optional)"),
        JsonFieldType::Array(t) => format!("List {t}"),
    }
}

fn proposals_from_json_schema(
    schema: &axiograph_ingest_json::JsonSchema,
    evidence_locator: Option<String>,
    chunks: &[axiograph_ingest_docs::Chunk],
) -> Vec<axiograph_ingest_docs::ProposalV1> {
    use axiograph_ingest_docs::{EvidencePointer, ProposalMetaV1, ProposalV1};
    use axiograph_ingest_json::{JsonType, JsonType as JT};
    use std::collections::HashMap;

    fn evidence_for_field(
        chunks: &[axiograph_ingest_docs::Chunk],
        locator: Option<&String>,
        field_name: &str,
    ) -> Vec<EvidencePointer> {
        if chunks.is_empty() {
            return Vec::new();
        }
        let field_name = field_name.trim();
        if field_name.is_empty() {
            return Vec::new();
        }
        let needle = format!("\"{}\"", field_name.to_ascii_lowercase());
        let mut best: Option<&axiograph_ingest_docs::Chunk> = None;
        for c in chunks {
            let text = c.text.to_ascii_lowercase();
            if text.contains(&needle) || text.contains(&field_name.to_ascii_lowercase()) {
                best = Some(c);
                break;
            }
        }
        let Some(best) = best.or_else(|| chunks.first()) else {
            return Vec::new();
        };
        vec![EvidencePointer {
            chunk_id: best.chunk_id.clone(),
            locator: locator.cloned(),
            span_id: Some(best.span_id.clone()),
        }]
    }

    let mut out: Vec<ProposalV1> = Vec::new();

    let mut type_names: Vec<String> = schema.types.keys().cloned().collect();
    type_names.sort();

    for ty_name in type_names {
        let Some(ty) = schema.types.get(&ty_name) else {
            continue;
        };

        let type_id = format!("json_type::{}", sanitize_id_component(&ty_name));
        let kind = match ty {
            JT::Object { .. } => "object",
            JT::Array { .. } => "array",
            JT::Primitive(_) => "primitive",
        };

        let mut attrs = HashMap::new();
        attrs.insert("kind".to_string(), kind.to_string());
        if let JsonType::Primitive(p) = ty {
            attrs.insert("primitive".to_string(), p.clone());
        }
        if schema.root_type.as_deref() == Some(&ty_name) {
            attrs.insert("is_root".to_string(), "true".to_string());
        }

        out.push(ProposalV1::Entity {
            meta: ProposalMetaV1 {
                proposal_id: type_id.clone(),
                confidence: 0.85,
                evidence: chunks
                    .first()
                    .map(|c| {
                        vec![EvidencePointer {
                            chunk_id: c.chunk_id.clone(),
                            locator: evidence_locator.clone(),
                            span_id: Some(c.span_id.clone()),
                        }]
                    })
                    .unwrap_or_default(),
                public_rationale: "Inferred JSON type from sample payload.".to_string(),
                metadata: HashMap::new(),
                schema_hint: Some("json".to_string()),
            },
            entity_id: type_id.clone(),
            entity_type: "JsonType".to_string(),
            name: ty_name.clone(),
            attributes: attrs,
            description: None,
        });

        let JsonType::Object { fields } = ty else {
            continue;
        };

        let mut field_names: Vec<String> = fields.keys().cloned().collect();
        field_names.sort();

        for field_name in field_names {
            let Some(field_ty) = fields.get(&field_name) else {
                continue;
            };

            let field_id = format!(
                "json_field::{}::{}",
                sanitize_id_component(&ty_name),
                sanitize_id_component(&field_name)
            );

            let mut field_attrs = HashMap::new();
            field_attrs.insert("owner_type".to_string(), ty_name.clone());
            field_attrs.insert("field".to_string(), field_name.clone());
            field_attrs.insert(
                "field_type".to_string(),
                json_field_type_to_string(field_ty),
            );

            out.push(ProposalV1::Entity {
                meta: ProposalMetaV1 {
                    proposal_id: field_id.clone(),
                    confidence: 0.85,
                    evidence: evidence_for_field(chunks, evidence_locator.as_ref(), &field_name),
                    public_rationale: "Inferred JSON field from sample payload.".to_string(),
                    metadata: HashMap::new(),
                    schema_hint: Some("json".to_string()),
                },
                entity_id: field_id.clone(),
                entity_type: "JsonField".to_string(),
                name: format!("{}.{}", ty_name, field_name),
                attributes: field_attrs,
                description: None,
            });

            let rel_id = format!(
                "json_rel::has_field::{}::{}",
                sanitize_id_component(&type_id),
                sanitize_id_component(&field_id)
            );
            out.push(ProposalV1::Relation {
                meta: ProposalMetaV1 {
                    proposal_id: rel_id.clone(),
                    confidence: 0.85,
                    evidence: evidence_for_field(chunks, evidence_locator.as_ref(), &field_name),
                    public_rationale: "Derived HasField from inferred JSON schema.".to_string(),
                    metadata: HashMap::new(),
                    schema_hint: Some("json".to_string()),
                },
                relation_id: rel_id,
                rel_type: "JsonHasField".to_string(),
                source: type_id.clone(),
                target: field_id.clone(),
                attributes: HashMap::new(),
            });

            // If this field points to another inferred object type, record a link.
            let target_ty = match field_ty {
                axiograph_ingest_json::JsonFieldType::Required(t)
                | axiograph_ingest_json::JsonFieldType::Optional(t)
                | axiograph_ingest_json::JsonFieldType::Array(t) => t,
            };

            if matches!(schema.types.get(target_ty), Some(JsonType::Object { .. })) {
                let target_type_id = format!("json_type::{}", sanitize_id_component(target_ty));
                let rel_id = format!(
                    "json_rel::field_refers_to::{}::{}",
                    sanitize_id_component(&field_id),
                    sanitize_id_component(&target_type_id)
                );
                out.push(ProposalV1::Relation {
                    meta: ProposalMetaV1 {
                        proposal_id: rel_id.clone(),
                        confidence: 0.75,
                        evidence: evidence_for_field(chunks, evidence_locator.as_ref(), &field_name),
                        public_rationale: "Heuristic: field type matches another inferred object."
                            .to_string(),
                        metadata: HashMap::new(),
                        schema_hint: Some("json".to_string()),
                    },
                    relation_id: rel_id,
                    rel_type: "JsonFieldRefersToType".to_string(),
                    source: field_id,
                    target: target_type_id,
                    attributes: HashMap::new(),
                });
            }
        }
    }

    out
}

fn cmd_sql(input: &PathBuf, out: &PathBuf, chunks_path: Option<&PathBuf>) -> Result<()> {
    println!(
        "{} SQL schema {}",
        "Ingesting".green().bold(),
        input.display()
    );

    let text = fs::read_to_string(input)?;
    let sql_schema = axiograph_ingest_sql::parse_sql_ddl(&text)?;
    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();

    // Also emit DocChunks for RAG grounding (default: alongside the proposals output).
    let chunks_out = chunks_path
        .cloned()
        .unwrap_or_else(|| out.parent().unwrap_or(std::path::Path::new(".")).join("chunks.json"));
    fs::create_dir_all(chunks_out.parent().unwrap_or(std::path::Path::new(".")))?;

    let locator = input.to_string_lossy().to_string();
    let doc_digest = axiograph_dsl::digest::fnv1a64_digest_bytes(locator.as_bytes());
    let mut chunks: Vec<axiograph_ingest_docs::Chunk> = Vec::new();
    for (i, stmt) in text.split(';').enumerate() {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("kind".to_string(), "sql_ddl".to_string());
        metadata.insert("source_path".to_string(), locator.clone());
        chunks.push(axiograph_ingest_docs::Chunk {
            chunk_id: format!("sql_{doc_digest}_{i}"),
            document_id: locator.clone(),
            page: None,
            span_id: format!("stmt_{i}"),
            text: format!("{stmt};"),
            bbox: None,
            metadata,
        });
    }
    fs::write(&chunks_out, serde_json::to_string_pretty(&chunks)?)?;
    println!(
        "  {} {} (chunks={})",
        "→".cyan(),
        chunks_out.display(),
        chunks.len()
    );

    let proposals = proposals_from_sql_schema(&sql_schema, Some(locator.clone()), &chunks);
    let file = axiograph_ingest_docs::ProposalsFileV1 {
        version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
        generated_at,
        source: axiograph_ingest_docs::ProposalSourceV1 {
            source_type: "sql".to_string(),
            locator,
        },
        schema_hint: Some("sql".to_string()),
        proposals,
    };

    let json = serde_json::to_string_pretty(&file)?;
    fs::create_dir_all(out.parent().unwrap_or(std::path::Path::new(".")))?;
    fs::write(out, &json)?;
    println!("  {} {}", "→".cyan(), out.display());
    println!(
        "  {} {} tables, {} foreign keys",
        "→".yellow(),
        sql_schema.tables.len(),
        sql_schema.foreign_keys.len()
    );

    Ok(())
}

fn cmd_doc(
    input: &PathBuf,
    out: &PathBuf,
    chunks_path: Option<&PathBuf>,
    facts_path: Option<&PathBuf>,
    machining: bool,
    domain: &str,
) -> Result<()> {
    println!(
        "{} document {}",
        "Ingesting".green().bold(),
        input.display()
    );

    let text = fs::read_to_string(input)?;
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();

    let domain = if machining { "machining" } else { domain };

    // Full knowledge extraction with probabilistic facts
    let result = axiograph_ingest_docs::extract_knowledge_full(&text, &stem, domain);
    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    let proposals = axiograph_ingest_docs::proposals_from_extracted_facts_v1(
        &result.facts,
        Some(input.to_string_lossy().to_string()),
        Some(domain.to_string()),
    );
    let file = axiograph_ingest_docs::ProposalsFileV1 {
        version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
        generated_at,
        source: axiograph_ingest_docs::ProposalSourceV1 {
            source_type: "doc".to_string(),
            locator: input.to_string_lossy().to_string(),
        },
        schema_hint: Some(domain.to_string()),
        proposals,
    };
    let json = serde_json::to_string_pretty(&file)?;
    fs::write(out, &json)?;
    println!("  {} {}", "→".cyan(), out.display());
    println!("  {} {} facts extracted", "→".yellow(), result.facts.len());

    let chunks_out = chunks_path
        .cloned()
        .unwrap_or_else(|| out.parent().unwrap_or(std::path::Path::new(".")).join("chunks.json"));
    fs::create_dir_all(chunks_out.parent().unwrap_or(std::path::Path::new(".")))?;
    let chunks_json = axiograph_ingest_docs::chunks_to_json(&result.extraction)?;
    fs::write(&chunks_out, &chunks_json)?;
    println!("  {} {}", "→".cyan(), chunks_out.display());

    if let Some(facts_out) = facts_path {
        let facts_json = serde_json::to_string_pretty(&result.facts)?;
        fs::write(facts_out, &facts_json)?;
        println!("  {} {}", "→".cyan(), facts_out.display());
    }

    Ok(())
}

fn cmd_conversation(
    input: &PathBuf,
    out: &PathBuf,
    chunks_path: Option<&PathBuf>,
    facts_path: Option<&PathBuf>,
    format: &str,
) -> Result<()> {
    println!(
        "{} conversation {}",
        "Ingesting".green().bold(),
        input.display()
    );

    let text = fs::read_to_string(input)?;
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();

    let result = axiograph_ingest_docs::extract_knowledge_from_conversation(&text, &stem, format);
    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    let proposals = axiograph_ingest_docs::proposals_from_extracted_facts_v1(
        &result.facts,
        Some(input.to_string_lossy().to_string()),
        Some("conversation".to_string()),
    );
    let file = axiograph_ingest_docs::ProposalsFileV1 {
        version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
        generated_at,
        source: axiograph_ingest_docs::ProposalSourceV1 {
            source_type: "conversation".to_string(),
            locator: input.to_string_lossy().to_string(),
        },
        schema_hint: Some("conversation".to_string()),
        proposals,
    };
    let json = serde_json::to_string_pretty(&file)?;
    fs::write(out, &json)?;
    println!("  {} {}", "→".cyan(), out.display());
    println!(
        "  {} {} turns, {} facts",
        "→".yellow(),
        result.extraction.chunks.len(),
        result.facts.len()
    );

    let chunks_out = chunks_path
        .cloned()
        .unwrap_or_else(|| out.parent().unwrap_or(std::path::Path::new(".")).join("chunks.json"));
    fs::create_dir_all(chunks_out.parent().unwrap_or(std::path::Path::new(".")))?;
    let chunks_json = axiograph_ingest_docs::chunks_to_json(&result.extraction)?;
    fs::write(&chunks_out, &chunks_json)?;
    println!("  {} {}", "→".cyan(), chunks_out.display());

    if let Some(facts_out) = facts_path {
        let facts_json = serde_json::to_string_pretty(&result.facts)?;
        fs::write(facts_out, &facts_json)?;
        println!("  {} {}", "→".cyan(), facts_out.display());
    }

    Ok(())
}

fn cmd_confluence(
    input: &PathBuf,
    out: &PathBuf,
    space: &str,
    chunks_path: Option<&PathBuf>,
    facts_path: Option<&PathBuf>,
) -> Result<()> {
    println!(
        "{} Confluence page {}",
        "Ingesting".green().bold(),
        input.display()
    );

    let html = fs::read_to_string(input)?;
    let page_id = input.file_stem().unwrap_or_default().to_string_lossy();

    let result = axiograph_ingest_docs::extract_knowledge_from_confluence(&html, &page_id, space)?;
    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    let proposals = axiograph_ingest_docs::proposals_from_extracted_facts_v1(
        &result.facts,
        Some(input.to_string_lossy().to_string()),
        Some("confluence".to_string()),
    );
    let file = axiograph_ingest_docs::ProposalsFileV1 {
        version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
        generated_at,
        source: axiograph_ingest_docs::ProposalSourceV1 {
            source_type: "confluence".to_string(),
            locator: input.to_string_lossy().to_string(),
        },
        schema_hint: Some("confluence".to_string()),
        proposals,
    };
    let json = serde_json::to_string_pretty(&file)?;
    fs::write(out, &json)?;
    println!("  {} {}", "→".cyan(), out.display());
    println!(
        "  {} {} sections, {} facts",
        "→".yellow(),
        result.extraction.chunks.len(),
        result.facts.len()
    );

    let chunks_out = chunks_path
        .cloned()
        .unwrap_or_else(|| out.parent().unwrap_or(std::path::Path::new(".")).join("chunks.json"));
    fs::create_dir_all(chunks_out.parent().unwrap_or(std::path::Path::new(".")))?;
    let chunks_json = axiograph_ingest_docs::chunks_to_json(&result.extraction)?;
    fs::write(&chunks_out, &chunks_json)?;
    println!("  {} {}", "→".cyan(), chunks_out.display());

    if let Some(facts_out) = facts_path {
        let facts_json = serde_json::to_string_pretty(&result.facts)?;
        fs::write(facts_out, &facts_json)?;
        println!("  {} {}", "→".cyan(), facts_out.display());
    }

    Ok(())
}

fn cmd_json(input: &PathBuf, out: &PathBuf, chunks_path: Option<&PathBuf>) -> Result<()> {
    println!("{} JSON {}", "Ingesting".green().bold(), input.display());

    let text = fs::read_to_string(input)?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    let schema = axiograph_ingest_json::infer_schema(&value, "Root");
    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();

    // Also emit DocChunks for RAG grounding (default: alongside the proposals output).
    let chunks_out = chunks_path
        .cloned()
        .unwrap_or_else(|| out.parent().unwrap_or(std::path::Path::new(".")).join("chunks.json"));
    fs::create_dir_all(chunks_out.parent().unwrap_or(std::path::Path::new(".")))?;

    fn chunk_by_lines(text: &str, max_chars: usize) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut cur = String::new();
        for line in text.lines() {
            let line = line.trim_end();
            if cur.len().saturating_add(line.len() + 1) > max_chars && !cur.is_empty() {
                out.push(cur);
                cur = String::new();
            }
            if !cur.is_empty() {
                cur.push('\n');
            }
            cur.push_str(line);
        }
        if !cur.trim().is_empty() {
            out.push(cur);
        }
        out
    }

    let locator = input.to_string_lossy().to_string();
    let doc_digest = axiograph_dsl::digest::fnv1a64_digest_bytes(locator.as_bytes());
    let pretty = serde_json::to_string_pretty(&value).unwrap_or_else(|_| text.clone());
    let parts = chunk_by_lines(&pretty, 2_500);
    let mut chunks: Vec<axiograph_ingest_docs::Chunk> = Vec::new();
    for (i, part) in parts.into_iter().enumerate() {
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("kind".to_string(), "json".to_string());
        metadata.insert("source_path".to_string(), locator.clone());
        chunks.push(axiograph_ingest_docs::Chunk {
            chunk_id: format!("json_{doc_digest}_{i}"),
            document_id: locator.clone(),
            page: None,
            span_id: format!("part_{i}"),
            text: part,
            bbox: None,
            metadata,
        });
    }
    fs::write(&chunks_out, serde_json::to_string_pretty(&chunks)?)?;
    println!(
        "  {} {} (chunks={})",
        "→".cyan(),
        chunks_out.display(),
        chunks.len()
    );

    let proposals = proposals_from_json_schema(&schema, Some(locator.clone()), &chunks);
    let file = axiograph_ingest_docs::ProposalsFileV1 {
        version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
        generated_at,
        source: axiograph_ingest_docs::ProposalSourceV1 {
            source_type: "json".to_string(),
            locator,
        },
        schema_hint: Some("json".to_string()),
        proposals,
    };
    let json = serde_json::to_string_pretty(&file)?;
    fs::create_dir_all(out.parent().unwrap_or(std::path::Path::new(".")))?;
    fs::write(out, &json)?;
    println!("  {} {}", "→".cyan(), out.display());

    Ok(())
}

fn cmd_readings(
    input: &PathBuf,
    out: &PathBuf,
    chunks_path: Option<&PathBuf>,
    format: &str,
) -> Result<()> {
    println!(
        "{} readings {}",
        "Ingesting".green().bold(),
        input.display()
    );

    let text = fs::read_to_string(input)?;
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();

    let readings = match format {
        "bibtex" => axiograph_ingest_docs::parse_bibtex(&text),
        "markdown" | _ => axiograph_ingest_docs::parse_reading_list(&text)
            .into_iter()
            .map(|r| r.bib)
            .collect(),
    };

    println!("  {} {} references found", "→".yellow(), readings.len());

    let extraction = axiograph_ingest_docs::readings_to_extraction(
        &readings
            .iter()
            .map(|b| axiograph_ingest_docs::RecommendedReading {
                bib: b.clone(),
                relevance_domains: vec!["general".to_string()],
                key_topics: b.keywords.clone(),
                importance: 0.5,
                notes: b.notes.clone().unwrap_or_default(),
            })
            .collect::<Vec<_>>(),
        &stem,
    );

    let chunks_out = chunks_path
        .cloned()
        .unwrap_or_else(|| out.parent().unwrap_or(std::path::Path::new(".")).join("chunks.json"));
    fs::create_dir_all(chunks_out.parent().unwrap_or(std::path::Path::new(".")))?;
    let chunks_json = axiograph_ingest_docs::chunks_to_json(&extraction)?;
    fs::write(&chunks_out, &chunks_json)?;
    println!("  {} {}", "→".cyan(), chunks_out.display());

    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    // Readings ingestion currently produces chunks; treat each reading as a claim-like entity.
    let mut proposals = Vec::new();
    for (idx, r) in readings.iter().enumerate() {
        let id = format!("reading::{}", idx);
        let title = if r.title.trim().is_empty() {
            "Untitled".to_string()
        } else {
            r.title.clone()
        };
        proposals.push(axiograph_ingest_docs::ProposalV1::Entity {
            meta: axiograph_ingest_docs::ProposalMetaV1 {
                proposal_id: id.clone(),
                confidence: 0.6,
                evidence: Vec::new(),
                public_rationale: "Parsed reading list entry.".to_string(),
                metadata: std::collections::HashMap::new(),
                schema_hint: Some("readings".to_string()),
            },
            entity_id: id,
            entity_type: "Reading".to_string(),
            name: title,
            attributes: {
                let mut m = std::collections::HashMap::new();
                if !r.authors.is_empty() {
                    m.insert("authors".to_string(), r.authors.join("; "));
                }
                if let Some(year) = r.year {
                    m.insert("year".to_string(), year.to_string());
                }
                if let Some(doi) = &r.doi {
                    m.insert("doi".to_string(), doi.clone());
                }
                m
            },
            description: r.abstract_text.clone(),
        });
    }

    let file = axiograph_ingest_docs::ProposalsFileV1 {
        version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
        generated_at,
        source: axiograph_ingest_docs::ProposalSourceV1 {
            source_type: "readings".to_string(),
            locator: input.to_string_lossy().to_string(),
        },
        schema_hint: Some("readings".to_string()),
        proposals,
    };
    let json = serde_json::to_string_pretty(&file)?;
    fs::write(out, &json)?;
    println!("  {} {}", "→".cyan(), out.display());

    Ok(())
}

fn cmd_pathdb_export_axi(input: &PathBuf, out: &PathBuf) -> Result<()> {
    println!(
        "{} {}",
        "Exporting PathDB (.axpd → .axi)".green().bold(),
        input.display()
    );

    let bytes = fs::read(input)?;
    let db = axiograph_pathdb::PathDB::from_bytes(&bytes)?;
    let axi = axiograph_pathdb::axi_export::export_pathdb_to_axi_v1(&db)?;
    fs::write(out, &axi)?;

    println!("  {} {}", "→".cyan(), out.display());
    Ok(())
}

fn cmd_pathdb_export_module(input: &PathBuf, out: &PathBuf, module: Option<&str>) -> Result<()> {
    println!(
        "{} {}",
        "Exporting canonical module from".green().bold(),
        input.display()
    );

    let bytes = fs::read(input)?;
    let db = axiograph_pathdb::PathDB::from_bytes(&bytes)?;

    let module_name = match module {
        Some(m) => m.to_string(),
        None => infer_single_meta_module_name(&db)?,
    };

    let axi = axiograph_pathdb::axi_module_export::export_axi_schema_v1_module_from_pathdb(
        &db,
        &module_name,
    )?;
    fs::write(out, axi)?;

    println!(
        "  {} module={} {} {}",
        "→".cyan(),
        module_name.cyan(),
        "→".cyan(),
        out.display()
    );
    Ok(())
}

fn cmd_pathdb_import_axi(input: &PathBuf, out: &PathBuf) -> Result<()> {
    println!(
        "{} {}",
        "Importing PathDB (.axi → .axpd)".green().bold(),
        input.display()
    );

    let text = fs::read_to_string(input)?;
    let m = axiograph_dsl::axi_v1::parse_axi_v1(&text)?;

    let is_snapshot = m
        .schemas
        .iter()
        .any(|s| s.name == axiograph_pathdb::axi_export::PATHDB_EXPORT_SCHEMA_NAME_V1)
        && m.instances.iter().any(|i| {
            i.schema == axiograph_pathdb::axi_export::PATHDB_EXPORT_SCHEMA_NAME_V1
                && i.name == axiograph_pathdb::axi_export::PATHDB_EXPORT_INSTANCE_NAME_V1
        });

    let mut db = if is_snapshot {
        axiograph_pathdb::axi_export::import_pathdb_from_axi_v1_module(&m)?
    } else {
        let mut db = axiograph_pathdb::PathDB::new();
        let summary = axiograph_pathdb::axi_module_import::import_axi_schema_v1_module_into_pathdb(
            &mut db, &m,
        )?;
        println!(
            "  {} imported module={} (meta_entities={} meta_relations={} instances={} entities={} upgraded_types={} tuple_entities={} relations={} derived_edges={})",
            "→".cyan(),
            m.module_name,
            summary.meta_entities_added,
            summary.meta_relations_added,
            summary.instances_imported,
            summary.entities_added,
            summary.entity_type_upgrades,
            summary.tuple_entities_added,
            summary.relations_added,
            summary.derived_edges_added
        );
        db
    };

    // Grounding always has evidence: embed the `.axi` module text as an untrusted
    // DocChunk so LLM/UIs can cite and open it even when no external docs exist.
    let digest = axiograph_dsl::digest::axi_digest_v1(&text);
    let module_chunk = crate::doc_chunks::chunk_from_axi_module_text(&m.module_name, &digest, &text);
    let _ = crate::doc_chunks::import_chunks_into_pathdb(&mut db, &[module_chunk]);

    db.build_indexes();
    let bytes = db.to_bytes()?;
    fs::write(out, bytes)?;

    println!("  {} {}", "→".cyan(), out.display());
    Ok(())
}

fn cmd_pathdb_import_chunks(input: &PathBuf, chunks: &PathBuf, out: &PathBuf) -> Result<()> {
    println!(
        "{} {}",
        "Importing chunks into PathDB (.axpd + chunks.json)"
            .green()
            .bold(),
        input.display()
    );

    let bytes = fs::read(input)?;
    let mut db = axiograph_pathdb::PathDB::from_bytes(&bytes)?;

    let chunks_text = fs::read_to_string(chunks)?;
    let chunks: Vec<axiograph_ingest_docs::Chunk> = serde_json::from_str(&chunks_text)?;

    let summary = crate::doc_chunks::import_chunks_into_pathdb(&mut db, &chunks)?;
    db.build_indexes();

    let bytes = db.to_bytes()?;
    fs::write(out, bytes)?;

    println!(
        "  {} chunks_total={} chunks_added={} documents_added={} links_added={} missing_targets={}",
        "→".cyan(),
        summary.chunks_total,
        summary.chunks_added,
        summary.documents_added,
        summary.links_added,
        summary.links_missing_target
    );
    println!("  {} {}", "→".cyan(), out.display());
    Ok(())
}

fn infer_single_meta_module_name(db: &axiograph_pathdb::PathDB) -> Result<String> {
    let Some(mods) = db.find_by_type(axiograph_pathdb::axi_meta::META_TYPE_MODULE) else {
        return Err(anyhow::anyhow!(
            "no `.axi` meta-plane module found (import a canonical `.axi` module first, or pass `--module <name>`)"
        ));
    };

    let mut names: Vec<String> = Vec::new();
    for id in mods.iter() {
        let Some(view) = db.get_entity(id) else {
            continue;
        };
        if let Some(name) = view.attrs.get("name") {
            names.push(name.clone());
        }
    }
    names.sort();
    names.dedup();

    if names.is_empty() {
        return Err(anyhow::anyhow!(
            "no `.axi` meta-plane modules have a `name` attribute"
        ));
    }
    if names.len() != 1 {
        return Err(anyhow::anyhow!(
            "multiple `.axi` modules imported: {:?} (pass `--module <name>`)",
            names
        ));
    }
    Ok(names[0].clone())
}

fn cmd_validate(input: &PathBuf) -> Result<()> {
    println!("{} {}", "Validating".green().bold(), input.display());

    let text = fs::read_to_string(input)?;
    let m = axiograph_dsl::axi_v1::parse_axi_v1(&text)?;

    println!("  Dialect: {}", "axi_v1 (schema/theory/instance)".cyan());
    println!("  Module: {}", m.module_name.cyan());
    println!("  Schemas: {}", m.schemas.len());
    println!("  Theories: {}", m.theories.len());
    println!("  Instances: {}", m.instances.len());

    for schema in &m.schemas {
        println!(
            "    Schema {}: {} objects, {} relations",
            schema.name.yellow(),
            schema.objects.len(),
            schema.relations.len()
        );
    }

    println!("{}", "Valid.".green());
    Ok(())
}

fn cmd_repo_index(
    root: &PathBuf,
    out: &PathBuf,
    chunks_path: Option<&PathBuf>,
    edges_path: Option<&PathBuf>,
    max_file_bytes: u64,
    max_files: usize,
    lines_per_chunk: usize,
) -> Result<()> {
    println!("{} {}", "Indexing repo".green().bold(), root.display());

    let options = axiograph_ingest_docs::RepoIndexOptions {
        max_files,
        max_file_bytes,
        lines_per_chunk,
        ..Default::default()
    };

    let result = axiograph_ingest_docs::index_repo(root, &options)?;

    let chunks_out = chunks_path
        .cloned()
        .unwrap_or_else(|| out.parent().unwrap_or(std::path::Path::new(".")).join("chunks.json"));
    fs::create_dir_all(chunks_out.parent().unwrap_or(std::path::Path::new(".")))?;
    let chunks_json = serde_json::to_string_pretty(&result.extraction.chunks)?;
    fs::write(&chunks_out, &chunks_json)?;
    println!("  {} {}", "→".cyan(), chunks_out.display());

    if let Some(edges_out) = edges_path {
        let edges_json = serde_json::to_string_pretty(&result.edges)?;
        fs::write(edges_out, &edges_json)?;
        println!("  {} {}", "→".cyan(), edges_out.display());
    }

    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    let proposals = axiograph_ingest_docs::proposals_from_repo_edges_v1(
        &result.edges,
        Some("repo".to_string()),
    );
    let file = axiograph_ingest_docs::ProposalsFileV1 {
        version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
        generated_at,
        source: axiograph_ingest_docs::ProposalSourceV1 {
            source_type: "repo".to_string(),
            locator: root.to_string_lossy().to_string(),
        },
        schema_hint: Some("repo".to_string()),
        proposals,
    };
    let json = serde_json::to_string_pretty(&file)?;
    fs::write(out, &json)?;
    println!("  {} {}", "→".cyan(), out.display());
    println!(
        "  {} {} chunks, {} edges",
        "→".yellow(),
        result.extraction.chunks.len(),
        result.edges.len()
    );

    Ok(())
}

fn cmd_repo_watch(
    root: &PathBuf,
    out: &PathBuf,
    chunks_path: Option<&PathBuf>,
    edges_path: Option<&PathBuf>,
    trace_path: Option<&PathBuf>,
    interval_secs: u64,
    max_suggestions: usize,
) -> Result<()> {
    println!(
        "{} {} (every {}s)",
        "Watching repo".green().bold(),
        root.display(),
        interval_secs
    );

    loop {
        cmd_repo_index(root, out, chunks_path, edges_path, 524_288, 50_000, 80)?;

        if let (Some(chunks), Some(edges), Some(trace_out)) = (chunks_path, edges_path, trace_path)
        {
            let _ = cmd_discover_suggest_links(chunks, edges, trace_out, max_suggestions);
        }

        std::thread::sleep(Duration::from_secs(interval_secs));
    }
}

fn cmd_discover_suggest_links(
    chunks_path: &PathBuf,
    edges_path: &PathBuf,
    out: &PathBuf,
    max_proposals: usize,
) -> Result<()> {
    println!(
        "{} (chunks: {}, edges: {})",
        "Discovering links".green().bold(),
        chunks_path.display(),
        edges_path.display()
    );

    let chunks_text = fs::read_to_string(chunks_path)?;
    let edges_text = fs::read_to_string(edges_path)?;

    let chunks: Vec<axiograph_ingest_docs::Chunk> = serde_json::from_str(&chunks_text)?;
    let edges: Vec<axiograph_ingest_docs::RepoEdgeV1> = serde_json::from_str(&edges_text)?;

    let trace_id = format!(
        "trace_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );

    let generated_at = format!(
        "{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );

    let trace = axiograph_ingest_docs::suggest_mentions_symbol_trace_v1(
        &chunks,
        &edges,
        max_proposals,
        trace_id,
        generated_at,
    )?;

    let trace_json = serde_json::to_string_pretty(&trace)?;
    fs::write(out, &trace_json)?;
    println!("  {} {}", "→".cyan(), out.display());
    println!("  {} {} proposals", "→".yellow(), trace.proposals.len());

    Ok(())
}

fn parse_promotion_domains(
    domains: &str,
) -> Result<std::collections::BTreeSet<axiograph_ingest_docs::PromotionDomainV1>> {
    use axiograph_ingest_docs::PromotionDomainV1;
    use std::collections::BTreeSet;

    let norm = domains.trim().to_lowercase().replace('-', "_");
    if norm == "all" {
        return Ok(BTreeSet::from([
            PromotionDomainV1::EconomicFlows,
            PromotionDomainV1::MachinistLearning,
            PromotionDomainV1::SchemaEvolution,
        ]));
    }

    let mut out = BTreeSet::new();
    for part in norm.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let d = match part {
            "economicflows" | "economic_flows" | "economy" | "economics" => {
                PromotionDomainV1::EconomicFlows
            }
            "machinistlearning" | "machinist_learning" | "machining" | "learning" => {
                PromotionDomainV1::MachinistLearning
            }
            "schemaevolution" | "schema_evolution" | "ontology" | "migration" | "migrations" => {
                PromotionDomainV1::SchemaEvolution
            }
            other => {
                return Err(anyhow::anyhow!(
                    "unknown domain `{other}` (expected all|economic_flows|machinist_learning|schema_evolution)"
                ))
            }
        };
        out.insert(d);
    }
    Ok(out)
}

fn cmd_discover_promote_proposals(
    proposals_path: &PathBuf,
    out_dir: &PathBuf,
    trace_path: Option<&PathBuf>,
    min_confidence: f64,
    domains: &str,
) -> Result<()> {
    println!(
        "{} {}",
        "Promoting proposals".green().bold(),
        proposals_path.display()
    );

    let text = fs::read_to_string(proposals_path)?;
    let proposals: axiograph_ingest_docs::ProposalsFileV1 = serde_json::from_str(&text)?;

    let domains = parse_promotion_domains(domains)?;
    let options = axiograph_ingest_docs::PromoteOptionsV1 {
        min_confidence,
        domains,
    };
    let result = axiograph_ingest_docs::promote_proposals_to_candidates_v1(&proposals, &options)?;

    fs::create_dir_all(out_dir)?;

    for (domain, axi) in &result.candidates {
        let out_path = out_dir.join(domain.default_output_file());
        fs::write(&out_path, axi)?;
        println!("  {} {}", "→".cyan(), out_path.display());
    }

    let trace_out = trace_path
        .cloned()
        .unwrap_or_else(|| out_dir.join("promotion_trace.json"));
    let json = serde_json::to_string_pretty(&result.trace)?;
    fs::write(&trace_out, json)?;
    println!("  {} {}", "→".cyan(), trace_out.display());

    Ok(())
}

const LLM_PLUGIN_PROTOCOL_V2: &str = "axiograph_llm_plugin_v2";

#[derive(Debug, Clone, Serialize)]
struct AugmentPluginRequestV1 {
    protocol: String,
    model: Option<String>,
    task: AugmentPluginTaskV1,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum AugmentPluginTaskV1 {
    AugmentProposals {
        proposals: axiograph_ingest_docs::ProposalsFileV1,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        evidence_chunks: Option<std::collections::BTreeMap<String, String>>,
        max_new_proposals: usize,
    },
}

#[derive(Debug, Clone, Deserialize)]
struct AugmentPluginResponseV1 {
    #[serde(default)]
    added_proposals: Vec<axiograph_ingest_docs::ProposalV1>,
    #[serde(default)]
    schema_hint_updates: Vec<SchemaHintUpdateV1>,
    #[serde(default)]
    notes: Vec<String>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SchemaHintUpdateV1 {
    proposal_id: String,
    schema_hint: String,
    #[serde(default)]
    public_rationale: Option<String>,
}

fn run_llm_plugin(
    program: &PathBuf,
    args: &[String],
    request: &AugmentPluginRequestV1,
    timeout: Option<Duration>,
) -> Result<AugmentPluginResponseV1> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("failed to start llm plugin `{}`: {e}", program.display()))?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow!("failed to open stdin for llm plugin"))?;
        serde_json::to_writer(stdin, request)?;
    }

    let output = crate::llm::wait_with_output_timeout(
        child,
        timeout,
        &format!("llm plugin `{}`", program.display()),
    )?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "llm plugin `{}` failed: {}\n{}",
            program.display(),
            output.status,
            stderr.trim()
        ));
    }

    serde_json::from_slice(&output.stdout)
        .map_err(|e| anyhow!("llm plugin returned invalid JSON: {e}"))
}

fn llm_augment_proposals(
    llm_backend: &str,
    endpoint: &str,
    model: &str,
    proposals: &axiograph_ingest_docs::ProposalsFileV1,
    evidence_chunks: Option<&std::collections::BTreeMap<String, String>>,
    llm_add_proposals: bool,
    overwrite_schema_hints: bool,
    max_new_proposals: usize,
    timeout: Option<Duration>,
) -> Result<AugmentPluginResponseV1> {
    fn sanitize_symbol(s: &str, max: usize) -> String {
        s.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .take(max)
            .collect()
    }

    fn clamp01(x: f64) -> f64 {
        if x.is_nan() {
            return 0.0;
        }
        x.clamp(0.0, 1.0)
    }

    fn llm_entity_id(entity_type: &str, name: &str) -> String {
        let et = sanitize_symbol(entity_type, 64);
        let key = format!("llm_entity:{et}:{name}");
        let digest = axiograph_dsl::digest::fnv1a64_digest_bytes(key.as_bytes());
        format!("llm_entity::{et}::{digest}")
    }

    fn llm_relation_id(rel_type: &str, source: &str, target: &str) -> String {
        let rt = sanitize_symbol(rel_type, 64);
        let key = format!("llm_relation:{rt}:{source}:{target}");
        let digest = axiograph_dsl::digest::fnv1a64_digest_bytes(key.as_bytes());
        format!("llm_rel::{rt}::{digest}")
    }

    #[derive(Debug, Clone, Serialize)]
    struct HintCandidateV1 {
        proposal_id: String,
        kind: String,
        confidence: f64,
        entity_type: Option<String>,
        name: Option<String>,
        rel_type: Option<String>,
        source_name: Option<String>,
        target_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        current_schema_hint: Option<String>,
        public_rationale: String,
        evidence_snippet: Option<String>,
    }

    #[derive(Debug, Clone, Serialize)]
    struct EntitySummaryV1 {
        entity_id: String,
        entity_type: String,
        name: String,
        confidence: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        evidence_chunk_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        statement: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        role: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        evidence_snippet: Option<String>,
    }

    let mut name_by_entity_id: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut unique_id_by_name: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut ambiguous_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut existing_entity_ids: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    let mut existing_entity_keys: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for p in &proposals.proposals {
        let axiograph_ingest_docs::ProposalV1::Entity {
            entity_id,
            entity_type,
            name,
            ..
        } = p
        else {
            continue;
        };
        name_by_entity_id.insert(entity_id.clone(), name.clone());
        existing_entity_ids.insert(entity_id.clone());
        existing_entity_keys.insert(format!("{}::{}", entity_type.trim(), name.trim()));

        if ambiguous_names.contains(name) {
            continue;
        }
        if unique_id_by_name.contains_key(name) {
            unique_id_by_name.remove(name);
            ambiguous_names.insert(name.clone());
        } else {
            unique_id_by_name.insert(name.clone(), entity_id.clone());
        }
    }

    let mut candidates: Vec<HintCandidateV1> = Vec::new();
    for p in &proposals.proposals {
        let meta = match p {
            axiograph_ingest_docs::ProposalV1::Entity { meta, .. } => meta,
            axiograph_ingest_docs::ProposalV1::Relation { meta, .. } => meta,
        };

        // Keep the prompt small: by default we only fill missing hints.
        if meta.schema_hint.is_some() && !overwrite_schema_hints {
            continue;
        }

        let evidence_snippet = meta.evidence.first().and_then(|ev| {
            let text = evidence_chunks?.get(&ev.chunk_id)?;
            let mut t = text.clone();
            if t.len() > 400 {
                t.truncate(400);
                t.push_str("…");
            }
            Some(t)
        });

        match p {
            axiograph_ingest_docs::ProposalV1::Entity {
                meta,
                entity_type,
                name,
                ..
            } => {
                candidates.push(HintCandidateV1 {
                    proposal_id: meta.proposal_id.clone(),
                    kind: "entity".to_string(),
                    confidence: meta.confidence,
                    entity_type: Some(entity_type.clone()),
                    name: Some(name.clone()),
                    rel_type: None,
                    source_name: None,
                    target_name: None,
                    current_schema_hint: meta.schema_hint.clone(),
                    public_rationale: meta.public_rationale.clone(),
                    evidence_snippet,
                });
            }
            axiograph_ingest_docs::ProposalV1::Relation {
                meta,
                rel_type,
                source,
                target,
                ..
            } => {
                candidates.push(HintCandidateV1 {
                    proposal_id: meta.proposal_id.clone(),
                    kind: "relation".to_string(),
                    confidence: meta.confidence,
                    entity_type: None,
                    name: None,
                    rel_type: Some(rel_type.clone()),
                    source_name: name_by_entity_id.get(source).cloned(),
                    target_name: name_by_entity_id.get(target).cloned(),
                    current_schema_hint: meta.schema_hint.clone(),
                    public_rationale: meta.public_rationale.clone(),
                    evidence_snippet,
                });
            }
        }
    }

    // Highest-confidence items first (so the model can focus).
    candidates.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
    candidates.truncate(120);

    if candidates.is_empty() && !llm_add_proposals {
        return Ok(AugmentPluginResponseV1 {
            added_proposals: Vec::new(),
            schema_hint_updates: Vec::new(),
            notes: vec![if overwrite_schema_hints {
                "ollama: no candidates for schema_hint updates".to_string()
            } else {
                "ollama: no candidates with missing schema_hint".to_string()
            }],
            error: None,
        });
    }

    let candidates_json =
        serde_json::to_string_pretty(&candidates).unwrap_or_else(|_| "[]".to_string());

    let system = r#"You assist with ontology engineering for Axiograph.

You must return a single JSON object (no markdown).
The output is untrusted and should be conservative.
"#;

    if !llm_add_proposals {
        let user = format!(
            r#"Task: For each candidate proposal, decide whether it belongs to one of these domains:

- economic_flows  (economics, accounts, money, transactions, supply/demand, costs, prices)
- machinist_learning (machining, manufacturing, tools, feeds/speeds, materials, process planning, ML for machining)
- schema_evolution (schemas, migrations, ontology engineering, type theory, constraints, rewriting)

Notes:
- Each candidate may include a `current_schema_hint` (existing routing hint).
- If you agree with the current hint, omit the proposal (no update needed).
- If you are not confident, omit the proposal (do not guess).

Input candidates (JSON):
{candidates_json}

Return a single JSON object with these keys:
- schema_hint_updates: [{{"proposal_id": "...", "schema_hint": "...", "public_rationale": "..."}}]
- notes: ["..."] (optional)

Do NOT add proposals in this mode.
Max new proposals budget (ignored here): {max_new_proposals}"#
        );

        let content = match llm_backend {
            "ollama" => {
                #[cfg(feature = "llm-ollama")]
                {
                    crate::llm::ollama_chat_with_timeout(
                        endpoint,
                        model,
                        &user,
                        Some(system),
                        Some(serde_json::json!("json")),
                        timeout,
                    )?
                }
                #[cfg(not(feature = "llm-ollama"))]
                {
                    return Err(anyhow!(
                        "ollama support not compiled (enable `axiograph-cli` feature `llm-ollama`)"
                    ));
                }
            }
            "openai" => {
                #[cfg(feature = "llm-openai")]
                {
                    crate::llm::openai_chat_with_timeout(
                        endpoint,
                        model,
                        &user,
                        Some(system),
                        Some(serde_json::json!("json")),
                        timeout,
                    )?
                }
                #[cfg(not(feature = "llm-openai"))]
                {
                    return Err(anyhow!(
                        "openai support not compiled (enable `axiograph-cli` feature `llm-openai`)"
                    ));
                }
            }
            "anthropic" => {
                #[cfg(feature = "llm-anthropic")]
                {
                    crate::llm::anthropic_chat_with_timeout(
                        endpoint,
                        model,
                        &user,
                        Some(system),
                        timeout,
                    )?
                }
                #[cfg(not(feature = "llm-anthropic"))]
                {
                    return Err(anyhow!(
                        "anthropic support not compiled (enable `axiograph-cli` feature `llm-anthropic`)"
                    ));
                }
            }
            other => {
                return Err(anyhow!("unsupported llm backend `{}` for augment-proposals", other));
            }
        };

        return crate::llm::parse_llm_json_object::<AugmentPluginResponseV1>(&content).map_err(
            |e| {
                anyhow!(
                    "{llm_backend} returned invalid JSON ({e}). content preview: {}",
                    content.chars().take(400).collect::<String>()
                )
            },
        );
    }

    // Grounded expansion mode: include a compact list of entities (especially
    // claims/mentions) so the model can reference them by id when proposing new relations.
    let chunks =
        evidence_chunks.ok_or_else(|| anyhow!("missing chunks (required for grounded mode)"))?;
    let mut entity_summaries: Vec<EntitySummaryV1> = Vec::new();
    for p in &proposals.proposals {
        let axiograph_ingest_docs::ProposalV1::Entity {
            meta,
            entity_id,
            entity_type,
            name,
            attributes,
            ..
        } = p
        else {
            continue;
        };

        let evidence_chunk_id = meta.evidence.first().map(|ev| ev.chunk_id.clone());
        let mut evidence_snippet = evidence_chunk_id
            .as_deref()
            .and_then(|id| chunks.get(id))
            .cloned();
        if let Some(t) = evidence_snippet.as_mut() {
            if t.len() > 400 {
                t.truncate(400);
                t.push_str("…");
            }
        }

        entity_summaries.push(EntitySummaryV1 {
            entity_id: entity_id.clone(),
            entity_type: entity_type.clone(),
            name: name.clone(),
            confidence: meta.confidence,
            evidence_chunk_id,
            statement: attributes.get("statement").cloned(),
            role: attributes.get("role").cloned(),
            value: attributes.get("value").cloned(),
            evidence_snippet,
        });
    }
    entity_summaries.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
    if entity_summaries.len() > 80 {
        entity_summaries.truncate(80);
    }
    let entity_summaries_json =
        serde_json::to_string_pretty(&entity_summaries).unwrap_or_else(|_| "[]".to_string());

    #[derive(Debug, Clone, Deserialize)]
    struct LlmNewEntityV1 {
        entity_type: String,
        name: String,
        #[serde(default)]
        attributes: std::collections::HashMap<String, String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        confidence: Option<f64>,
        #[serde(default, alias = "chunk_id")]
        evidence_chunk_id: Option<String>,
        #[serde(default)]
        public_rationale: Option<String>,
        #[serde(default)]
        schema_hint: Option<String>,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct LlmNewRelationV1 {
        rel_type: String,
        #[serde(alias = "source_entity_id", alias = "source_id", alias = "from")]
        source: String,
        #[serde(alias = "target_entity_id", alias = "target_id", alias = "to")]
        target: String,
        #[serde(default)]
        attributes: std::collections::HashMap<String, String>,
        #[serde(default)]
        confidence: Option<f64>,
        #[serde(default, alias = "chunk_id")]
        evidence_chunk_id: Option<String>,
        #[serde(default)]
        public_rationale: Option<String>,
        #[serde(default)]
        schema_hint: Option<String>,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct LlmAugmentResponseV1 {
        #[serde(default)]
        schema_hint_updates: Vec<SchemaHintUpdateV1>,
        #[serde(default)]
        new_entities: Vec<LlmNewEntityV1>,
        #[serde(default)]
        new_relations: Vec<LlmNewRelationV1>,
        #[serde(default)]
        notes: Vec<String>,
        #[serde(default)]
        error: Option<String>,
    }

    let user = format!(
        r#"You are running grounded discovery to enrich untrusted `proposals.json` with extra structure.

Task A (optional): schema_hint routing
Choose schema hints for some candidates (only if helpful). Domains:
- economic_flows
- machinist_learning
- schema_evolution

Candidates (JSON):
{candidates_json}

Task B: grounded expansion
Propose additional entities/relations to make the physics knowledge more structured.

Rules:
- New proposals MUST be grounded in evidence: include `evidence_chunk_id` from the entity summaries below.
- Prefer reusing existing entity ids in relations (use `entity_id`).
- You may also refer to entities by `name` if unambiguous.
- Keep suggestions minimal and defensible (do not guess wildly).
- Confidence must be between 0 and 1.
- Max budget: {max_new_proposals} new proposals (entities + relations).

Entity summaries (JSON):
{entity_summaries_json}

Return ONE JSON object with keys:
- schema_hint_updates: [{{"proposal_id": "...", "schema_hint": "...", "public_rationale": "..."}}]
- new_entities: [{{"entity_type": "...", "name": "...", "attributes": {{}}, "confidence": 0.7, "evidence_chunk_id": "...", "public_rationale": "..."}}]
- new_relations: [{{"rel_type": "...", "source": "<entity_id or name>", "target": "<entity_id or name>", "attributes": {{}}, "confidence": 0.7, "evidence_chunk_id": "...", "public_rationale": "..."}}]
- notes: ["..."] (optional)

If you have no good suggestions, return empty arrays."#
    );

    let content = match llm_backend {
        "ollama" => {
            #[cfg(feature = "llm-ollama")]
            {
                crate::llm::ollama_chat_with_timeout(
                    endpoint,
                    model,
                    &user,
                    Some(system),
                    Some(serde_json::json!("json")),
                    timeout,
                )?
            }
            #[cfg(not(feature = "llm-ollama"))]
            {
                return Err(anyhow!(
                    "ollama support not compiled (enable `axiograph-cli` feature `llm-ollama`)"
                ));
            }
        }
        "openai" => {
            #[cfg(feature = "llm-openai")]
            {
                crate::llm::openai_chat_with_timeout(
                    endpoint,
                    model,
                    &user,
                    Some(system),
                    Some(serde_json::json!("json")),
                    timeout,
                )?
            }
            #[cfg(not(feature = "llm-openai"))]
            {
                return Err(anyhow!(
                    "openai support not compiled (enable `axiograph-cli` feature `llm-openai`)"
                ));
            }
        }
        "anthropic" => {
            #[cfg(feature = "llm-anthropic")]
            {
                crate::llm::anthropic_chat_with_timeout(
                    endpoint,
                    model,
                    &user,
                    Some(system),
                    timeout,
                )?
            }
            #[cfg(not(feature = "llm-anthropic"))]
            {
                return Err(anyhow!(
                    "anthropic support not compiled (enable `axiograph-cli` feature `llm-anthropic`)"
                ));
            }
        }
        other => {
            return Err(anyhow!("unsupported llm backend `{}` for augment-proposals", other));
        }
    };
    let parsed: LlmAugmentResponseV1 =
        crate::llm::parse_llm_json_object(&content).map_err(|e| {
            anyhow!(
                "{llm_backend} returned invalid JSON ({e}). content preview: {}",
                content.chars().take(400).collect::<String>()
            )
        })?;

    if let Some(err) = parsed.error {
        return Ok(AugmentPluginResponseV1 {
            added_proposals: Vec::new(),
            schema_hint_updates: parsed.schema_hint_updates,
            notes: parsed.notes,
            error: Some(err),
        });
    }

    let resolve_ref =
        |s: &str, new_by_name: &std::collections::HashMap<String, String>| -> Option<String> {
            if existing_entity_ids.contains(s) {
                return Some(s.to_string());
            }
            if let Some(id) = new_by_name.get(s) {
                return Some(id.clone());
            }
            unique_id_by_name.get(s).cloned()
        };

    let default_schema_hint = proposals.schema_hint.clone();

    let mut added_proposals: Vec<axiograph_ingest_docs::ProposalV1> = Vec::new();
    let mut new_entity_id_by_name: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    // First create entities so relations can reference them by name.
    for ent in &parsed.new_entities {
        if added_proposals.len() >= max_new_proposals {
            break;
        }
        let entity_type = ent.entity_type.trim();
        let name = ent.name.trim();
        if entity_type.is_empty() || name.is_empty() {
            continue;
        }
        let key = format!("{entity_type}::{name}");
        if existing_entity_keys.contains(&key) {
            continue;
        }
        let Some(chunk_id) = ent.evidence_chunk_id.as_deref() else {
            continue;
        };
        if !chunks.contains_key(chunk_id) {
            continue;
        }

        let entity_id = llm_entity_id(entity_type, name);
        let confidence = clamp01(ent.confidence.unwrap_or(0.55));
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("derived_from".to_string(), format!("{llm_backend}_augment_proposals_v1"));
        metadata.insert("llm_model".to_string(), model.to_string());

        let schema_hint = ent
            .schema_hint
            .clone()
            .or_else(|| default_schema_hint.clone());
        let public_rationale = ent
            .public_rationale
            .clone()
            .unwrap_or_else(|| "Added by LLM grounded augmentation.".to_string());
        let evidence = vec![axiograph_ingest_docs::EvidencePointer {
            chunk_id: chunk_id.to_string(),
            locator: None,
            span_id: None,
        }];

        added_proposals.push(axiograph_ingest_docs::ProposalV1::Entity {
            meta: axiograph_ingest_docs::ProposalMetaV1 {
                proposal_id: entity_id.clone(),
                confidence,
                evidence,
                public_rationale,
                metadata,
                schema_hint,
            },
            entity_id: entity_id.clone(),
            entity_type: entity_type.to_string(),
            name: name.to_string(),
            attributes: ent.attributes.clone(),
            description: ent.description.clone(),
        });

        new_entity_id_by_name
            .entry(name.to_string())
            .or_insert(entity_id);
    }

    for rel in &parsed.new_relations {
        if added_proposals.len() >= max_new_proposals {
            break;
        }
        let rel_type = rel.rel_type.trim();
        if rel_type.is_empty() {
            continue;
        }
        let Some(chunk_id) = rel.evidence_chunk_id.as_deref() else {
            continue;
        };
        if !chunks.contains_key(chunk_id) {
            continue;
        }

        let Some(source) = resolve_ref(rel.source.trim(), &new_entity_id_by_name) else {
            continue;
        };
        let Some(target) = resolve_ref(rel.target.trim(), &new_entity_id_by_name) else {
            continue;
        };

        let relation_id = llm_relation_id(rel_type, &source, &target);
        let confidence = clamp01(rel.confidence.unwrap_or(0.55));
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("derived_from".to_string(), format!("{llm_backend}_augment_proposals_v1"));
        metadata.insert("llm_model".to_string(), model.to_string());

        let schema_hint = rel
            .schema_hint
            .clone()
            .or_else(|| default_schema_hint.clone());
        let public_rationale = rel
            .public_rationale
            .clone()
            .unwrap_or_else(|| "Added by LLM grounded augmentation.".to_string());
        let evidence = vec![axiograph_ingest_docs::EvidencePointer {
            chunk_id: chunk_id.to_string(),
            locator: None,
            span_id: None,
        }];

        added_proposals.push(axiograph_ingest_docs::ProposalV1::Relation {
            meta: axiograph_ingest_docs::ProposalMetaV1 {
                proposal_id: relation_id.clone(),
                confidence,
                evidence,
                public_rationale,
                metadata,
                schema_hint,
            },
            relation_id,
            rel_type: rel_type.to_string(),
            source,
            target,
            attributes: rel.attributes.clone(),
        });
    }

    Ok(AugmentPluginResponseV1 {
        added_proposals,
        schema_hint_updates: parsed.schema_hint_updates,
        notes: parsed.notes,
        error: None,
    })
}

fn llm_suggest_schema_structure(
    llm_backend: &str,
    endpoint: &str,
    model: &str,
    base_draft_axi: &str,
    schema_name: &str,
    timeout: Option<Duration>,
) -> Result<crate::schema_discovery::DraftAxiModuleSuggestions> {
    #[derive(Debug, Clone, Deserialize)]
    struct LlmStructureResponseV1 {
        #[serde(default)]
        subtypes: Vec<LlmSubtypeV1>,
        #[serde(default)]
        constraints: Vec<LlmConstraintV1>,
        #[serde(default)]
        notes: Vec<String>,
        #[serde(default)]
        error: Option<String>,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct LlmSubtypeV1 {
        sub: String,
        sup: String,
        #[serde(default)]
        public_rationale: Option<String>,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct LlmConstraintV1 {
        kind: String,
        relation: String,
        #[serde(default)]
        public_rationale: Option<String>,
    }

    let module = axiograph_dsl::axi_v1::parse_axi_v1(base_draft_axi)?;
    let Some(schema) = module.schemas.iter().find(|s| s.name == schema_name) else {
        return Err(anyhow!("draft module contains no schema `{schema_name}`"));
    };
    let Some(instance) = module.instances.iter().find(|i| i.schema == schema_name) else {
        return Err(anyhow!(
            "draft module contains no instance for schema `{schema_name}`"
        ));
    };

    // Summarize object types + a few example inhabitants for each.
    let mut sample_members: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for a in &instance.assignments {
        // Only consider object assignments: `{ Ident ... }`.
        if a.value
            .items
            .iter()
            .all(|it| matches!(it, axiograph_dsl::schema_v1::SetItemV1::Ident { .. }))
        {
            let mut names: Vec<String> = Vec::new();
            for it in &a.value.items {
                let axiograph_dsl::schema_v1::SetItemV1::Ident { name } = it else {
                    continue;
                };
                names.push(name.clone());
                if names.len() >= 6 {
                    break;
                }
            }
            if !names.is_empty() {
                sample_members.insert(a.name.clone(), names);
            }
        }
    }

    #[derive(Debug, Clone, Serialize)]
    struct TypeSummaryV1 {
        name: String,
        examples: Vec<String>,
    }

    #[derive(Debug, Clone, Serialize)]
    struct RelationSummaryV1 {
        name: String,
        from_type: String,
        to_type: String,
    }

    let mut types: Vec<TypeSummaryV1> = Vec::new();
    for t in &schema.objects {
        let examples = sample_members.get(t).cloned().unwrap_or_default();
        types.push(TypeSummaryV1 {
            name: t.clone(),
            examples,
        });
    }

    let mut relations: Vec<RelationSummaryV1> = Vec::new();
    for r in &schema.relations {
        let mut from_type = "Entity".to_string();
        let mut to_type = "Entity".to_string();
        for f in &r.fields {
            if f.field == "from" {
                from_type = f.ty.clone();
            } else if f.field == "to" {
                to_type = f.ty.clone();
            }
        }
        relations.push(RelationSummaryV1 {
            name: r.name.clone(),
            from_type,
            to_type,
        });
    }

    // Keep the prompt small and stable.
    types.sort_by(|a, b| a.name.cmp(&b.name));
    relations.sort_by(|a, b| a.name.cmp(&b.name));

    if types.len() > 80 {
        types.truncate(80);
    }
    if relations.len() > 140 {
        relations.truncate(140);
    }

    let types_json = serde_json::to_string_pretty(&types).unwrap_or_else(|_| "[]".to_string());
    let relations_json =
        serde_json::to_string_pretty(&relations).unwrap_or_else(|_| "[]".to_string());

    let system = r#"You assist with ontology engineering for Axiograph.

You may suggest:
- additional subtype edges between existing object types, and
- candidate relation constraints: symmetric/transitive.

Constraints:
- Use ONLY names that appear in the provided type/relation lists.
- Do NOT invent new types or relations.
- Keep suggestions minimal and defensible.
- Return JSON only."#;

    let user = format!(
        r#"Schema: {schema_name}

Types (JSON):
{types_json}

Relations (JSON):
{relations_json}

Suggest additional structure, if appropriate.

Return a single JSON object with keys:
- subtypes: [{{"sub": "TypeA", "sup": "TypeB", "public_rationale": "..."}}]
- constraints: [{{"kind": "symmetric|transitive", "relation": "RelName", "public_rationale": "..."}}]
- notes: ["..."] (optional)

If you have no good suggestions, return empty arrays."#
    );

    let content = match llm_backend {
        "ollama" => {
            #[cfg(feature = "llm-ollama")]
            {
                crate::llm::ollama_chat_with_timeout(
                    endpoint,
                    model,
                    &user,
                    Some(system),
                    Some(serde_json::json!("json")),
                    timeout,
                )?
            }
            #[cfg(not(feature = "llm-ollama"))]
            {
                return Err(anyhow!(
                    "ollama support not compiled (enable `axiograph-cli` feature `llm-ollama`)"
                ));
            }
        }
        "openai" => {
            #[cfg(feature = "llm-openai")]
            {
                crate::llm::openai_chat_with_timeout(
                    endpoint,
                    model,
                    &user,
                    Some(system),
                    Some(serde_json::json!("json")),
                    timeout,
                )?
            }
            #[cfg(not(feature = "llm-openai"))]
            {
                return Err(anyhow!(
                    "openai support not compiled (enable `axiograph-cli` feature `llm-openai`)"
                ));
            }
        }
        "anthropic" => {
            #[cfg(feature = "llm-anthropic")]
            {
                crate::llm::anthropic_chat_with_timeout(endpoint, model, &user, Some(system), timeout)?
            }
            #[cfg(not(feature = "llm-anthropic"))]
            {
                return Err(anyhow!(
                    "anthropic support not compiled (enable `axiograph-cli` feature `llm-anthropic`)"
                ));
            }
        }
        other => {
            return Err(anyhow!(
                "unsupported llm backend `{}` for draft-module structure suggestions",
                other
            ));
        }
    };
    let parsed: LlmStructureResponseV1 =
        crate::llm::parse_llm_json_object(&content).map_err(|e| {
            anyhow!(
                "{llm_backend} returned invalid JSON ({e}). content preview: {}",
                content.chars().take(400).collect::<String>()
            )
        })?;

    if let Some(err) = parsed.error {
        return Err(anyhow!("{llm_backend}: {err}"));
    }

    if !parsed.notes.is_empty() {
        println!("  {} structure notes:", "→".yellow());
        for n in parsed.notes.iter().take(10) {
            println!("    - {n}");
        }
    }

    let mut out = crate::schema_discovery::DraftAxiModuleSuggestions::default();
    out.subtypes = parsed
        .subtypes
        .into_iter()
        .map(|s| crate::schema_discovery::SuggestedSubtype {
            sub: s.sub,
            sup: s.sup,
            public_rationale: s.public_rationale,
        })
        .collect();
    out.constraints = parsed
        .constraints
        .into_iter()
        .map(|c| crate::schema_discovery::SuggestedConstraint {
            kind: c.kind,
            relation: c.relation,
            public_rationale: c.public_rationale,
        })
        .collect();
    Ok(out)
}

#[cfg(feature = "llm-ollama")]
fn ollama_suggest_schema_structure(
    host: &str,
    model: &str,
    base_draft_axi: &str,
    schema_name: &str,
    timeout: Option<Duration>,
) -> Result<crate::schema_discovery::DraftAxiModuleSuggestions> {
    llm_suggest_schema_structure("ollama", host, model, base_draft_axi, schema_name, timeout)
}

#[cfg(feature = "llm-openai")]
fn openai_suggest_schema_structure(
    base_url: &str,
    model: &str,
    base_draft_axi: &str,
    schema_name: &str,
    timeout: Option<Duration>,
) -> Result<crate::schema_discovery::DraftAxiModuleSuggestions> {
    llm_suggest_schema_structure("openai", base_url, model, base_draft_axi, schema_name, timeout)
}

#[cfg(feature = "llm-anthropic")]
fn anthropic_suggest_schema_structure(
    base_url: &str,
    model: &str,
    base_draft_axi: &str,
    schema_name: &str,
    timeout: Option<Duration>,
) -> Result<crate::schema_discovery::DraftAxiModuleSuggestions> {
    llm_suggest_schema_structure(
        "anthropic",
        base_url,
        model,
        base_draft_axi,
        schema_name,
        timeout,
    )
}

fn proposal_id(p: &axiograph_ingest_docs::ProposalV1) -> &str {
    match p {
        axiograph_ingest_docs::ProposalV1::Entity { meta, .. } => meta.proposal_id.as_str(),
        axiograph_ingest_docs::ProposalV1::Relation { meta, .. } => meta.proposal_id.as_str(),
    }
}

fn proposal_meta_mut(
    p: &mut axiograph_ingest_docs::ProposalV1,
) -> &mut axiograph_ingest_docs::ProposalMetaV1 {
    match p {
        axiograph_ingest_docs::ProposalV1::Entity { meta, .. } => meta,
        axiograph_ingest_docs::ProposalV1::Relation { meta, .. } => meta,
    }
}

fn cmd_discover_augment_proposals(
    proposals_path: &PathBuf,
    out: &PathBuf,
    trace_path: Option<&PathBuf>,
    chunks_path: Option<&PathBuf>,
    llm_plugin: Option<&PathBuf>,
    llm_plugin_args: &[String],
    llm_ollama: bool,
    llm_ollama_host: Option<&str>,
    llm_openai: bool,
    llm_openai_base_url: Option<&str>,
    llm_anthropic: bool,
    llm_anthropic_base_url: Option<&str>,
    llm_model: Option<&str>,
    llm_timeout_secs: Option<u64>,
    llm_add_proposals: bool,
    options: axiograph_ingest_docs::AugmentOptionsV1,
) -> Result<()> {
    println!(
        "{} {}",
        "Augmenting proposals".green().bold(),
        proposals_path.display()
    );

    let text = fs::read_to_string(proposals_path)?;
    let proposals: axiograph_ingest_docs::ProposalsFileV1 = serde_json::from_str(&text)?;

    let trace_id = format!(
        "augment_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );
    let generated_at = format!(
        "{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );

    let llm_selected = (llm_plugin.is_some() as usize)
        + (llm_ollama as usize)
        + (llm_openai as usize)
        + (llm_anthropic as usize);
    if llm_selected > 1 {
        return Err(anyhow!(
            "choose at most one LLM integration: either `--llm-plugin ...`, `--llm-ollama`, `--llm-openai`, or `--llm-anthropic`"
        ));
    }

    let (mut augmented, mut trace) =
        axiograph_ingest_docs::augment_proposals_v1(&proposals, trace_id, generated_at, &options)?;

    let llm_enabled = llm_selected > 0;
    let evidence_chunks = if llm_enabled {
        if let Some(chunks_path) = chunks_path {
            let chunks_text = fs::read_to_string(chunks_path)?;
            let chunks: Vec<axiograph_ingest_docs::Chunk> = serde_json::from_str(&chunks_text)?;

            let mut needed: BTreeSet<String> = BTreeSet::new();
            for p in &augmented.proposals {
                let meta = match p {
                    axiograph_ingest_docs::ProposalV1::Entity { meta, .. } => meta,
                    axiograph_ingest_docs::ProposalV1::Relation { meta, .. } => meta,
                };
                for ev in &meta.evidence {
                    if needed.len() >= 2000 {
                        break;
                    }
                    needed.insert(ev.chunk_id.clone());
                }
                if needed.len() >= 2000 {
                    break;
                }
            }

            let mut out_map: std::collections::BTreeMap<String, String> =
                std::collections::BTreeMap::new();
            for c in chunks {
                if !needed.contains(&c.chunk_id) {
                    continue;
                }
                let mut t = c.text;
                if t.len() > 1200 {
                    t.truncate(1200);
                    t.push_str("…");
                }
                out_map.insert(c.chunk_id, t);
                if out_map.len() >= 2000 {
                    break;
                }
            }

            Some(out_map)
        } else {
            None
        }
    } else {
        None
    };

    if llm_enabled {
        let timeout = crate::llm::llm_timeout(llm_timeout_secs)?;

        if llm_add_proposals && evidence_chunks.is_none() {
            return Err(anyhow!(
                "`--llm-add-proposals` requires `--chunks <chunks.json>` so the LLM can cite evidence chunk ids"
            ));
        }

        let response = if let Some(plugin) = llm_plugin {
            let request = AugmentPluginRequestV1 {
                protocol: LLM_PLUGIN_PROTOCOL_V2.to_string(),
                model: llm_model.map(|s| s.to_string()),
                task: AugmentPluginTaskV1::AugmentProposals {
                    proposals: augmented.clone(),
                    evidence_chunks: evidence_chunks.clone(),
                    max_new_proposals: options.max_new_proposals,
                },
            };

            run_llm_plugin(plugin, llm_plugin_args, &request, timeout)?
        } else if llm_ollama {
            #[cfg(feature = "llm-ollama")]
            {
                let model = llm_model.ok_or_else(|| {
                    anyhow!("missing `--llm-model` (example: --llm-model nemotron-3-nano)")
                })?;
                let host = llm_ollama_host
                    .map(|s| s.to_string())
                    .unwrap_or_else(crate::llm::default_ollama_host);
                llm_augment_proposals(
                    "ollama",
                    &host,
                    model,
                    &augmented,
                    evidence_chunks.as_ref(),
                    llm_add_proposals,
                    options.overwrite_schema_hints,
                    options.max_new_proposals,
                    timeout,
                )?
            }
            #[cfg(not(feature = "llm-ollama"))]
            {
                return Err(anyhow!(
                    "ollama support not compiled (enable `axiograph-cli` feature `llm-ollama`)"
                ));
            }
        } else if llm_openai {
            #[cfg(feature = "llm-openai")]
            {
                let model = llm_model.ok_or_else(|| {
                    anyhow!("missing `--llm-model` (example: --llm-model gpt-4o-mini)")
                })?;
                let base_url = llm_openai_base_url
                    .map(|s| s.to_string())
                    .unwrap_or_else(crate::llm::default_openai_base_url);
                llm_augment_proposals(
                    "openai",
                    &base_url,
                    model,
                    &augmented,
                    evidence_chunks.as_ref(),
                    llm_add_proposals,
                    options.overwrite_schema_hints,
                    options.max_new_proposals,
                    timeout,
                )?
            }
            #[cfg(not(feature = "llm-openai"))]
            {
                return Err(anyhow!(
                    "openai support not compiled (enable `axiograph-cli` feature `llm-openai`)"
                ));
            }
        } else if llm_anthropic {
            #[cfg(feature = "llm-anthropic")]
            {
                let model = llm_model.ok_or_else(|| {
                    anyhow!("missing `--llm-model` (example: --llm-model claude-3-5-sonnet-20241022)")
                })?;
                let base_url = llm_anthropic_base_url
                    .map(|s| s.to_string())
                    .unwrap_or_else(crate::llm::default_anthropic_base_url);
                llm_augment_proposals(
                    "anthropic",
                    &base_url,
                    model,
                    &augmented,
                    evidence_chunks.as_ref(),
                    llm_add_proposals,
                    options.overwrite_schema_hints,
                    options.max_new_proposals,
                    timeout,
                )?
            }
            #[cfg(not(feature = "llm-anthropic"))]
            {
                return Err(anyhow!(
                    "anthropic support not compiled (enable `axiograph-cli` feature `llm-anthropic`)"
                ));
            }
        } else {
            AugmentPluginResponseV1 {
                added_proposals: Vec::new(),
                schema_hint_updates: Vec::new(),
                notes: Vec::new(),
                error: None,
            }
        };

        if let Some(err) = response.error {
            return Err(anyhow!("llm augmentation error: {err}"));
        }

        if !response.notes.is_empty() {
            println!("  {} llm notes:", "→".yellow());
            for n in response.notes.iter().take(10) {
                println!("    - {n}");
            }
        }

        let mut id_index: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for (i, p) in augmented.proposals.iter().enumerate() {
            id_index.insert(proposal_id(p).to_string(), i);
        }
        let mut existing_ids: std::collections::HashSet<String> =
            id_index.keys().cloned().collect();

        for upd in response.schema_hint_updates {
            let Some(&idx) = id_index.get(&upd.proposal_id) else {
                continue;
            };
            let meta = proposal_meta_mut(&mut augmented.proposals[idx]);
            let old = meta.schema_hint.clone();
            if old.is_some() && !options.overwrite_schema_hints {
                continue;
            }
            meta.schema_hint = Some(upd.schema_hint.clone());
            trace.summary.schema_hints_set += 1;
            trace
                .actions
                .push(axiograph_ingest_docs::AugmentActionV1::SetSchemaHint {
                    proposal_id: upd.proposal_id,
                    old_hint: old,
                    new_hint: upd.schema_hint,
                    public_rationale: upd.public_rationale.unwrap_or_else(|| {
                        "Set schema hint based on LLM/plugin suggestion.".to_string()
                    }),
                });
        }

        for p in response.added_proposals {
            if trace.summary.proposals_added >= options.max_new_proposals {
                break;
            }
            let pid = proposal_id(&p).to_string();
            if pid.trim().is_empty() || existing_ids.contains(&pid) {
                continue;
            }
            existing_ids.insert(pid.clone());
            augmented.proposals.push(p);
            trace.summary.proposals_added += 1;
            trace
                .actions
                .push(axiograph_ingest_docs::AugmentActionV1::AddProposal {
                    proposal_id: pid,
                    public_rationale: "Added proposal from LLM/plugin augmentation.".to_string(),
                });
        }

        trace.summary.proposals_out = augmented.proposals.len();
    }

    let json = serde_json::to_string_pretty(&augmented)?;
    fs::write(out, &json)?;
    println!("  {} {}", "→".cyan(), out.display());

    let trace_out = trace_path
        .cloned()
        .unwrap_or_else(|| PathBuf::from(format!("{}.trace.json", out.display())));
    let trace_json = serde_json::to_string_pretty(&trace)?;
    fs::write(&trace_out, &trace_json)?;
    println!("  {} {}", "→".cyan(), trace_out.display());
    println!(
        "  {} {} → {} proposals (+{}, schema_hints_set={})",
        "→".yellow(),
        trace.summary.proposals_in,
        trace.summary.proposals_out,
        trace.summary.proposals_added,
        trace.summary.schema_hints_set
    );

    Ok(())
}

fn cmd_discover_jepa_export(
    input: &PathBuf,
    out: &PathBuf,
    instance_filter: Option<&str>,
    max_items: usize,
    mask_fields: usize,
    seed: u64,
) -> Result<()> {
    let opts = crate::world_model::JepaExportOptions {
        instance_filter: instance_filter.map(|s| s.to_string()),
        max_items,
        mask_fields,
        seed,
    };
    crate::world_model::write_jepa_export(input, out, &opts)?;
    println!("wrote {}", out.display());
    Ok(())
}

fn cmd_discover_competency_questions(args: &CompetencyQuestionsArgs) -> Result<()> {
    let db = load_pathdb_for_cli(&args.input)?;

    let options = crate::competency_questions::CompetencyQuestionOptions {
        include_types: !args.no_types,
        include_relations: !args.no_relations,
        include_entity: args.include_entity,
        min_rows: args.min_rows,
        weight: args.weight,
        contexts: args.context.clone(),
    };

    let mut out: Vec<crate::world_model::CompetencyQuestionV1> = Vec::new();
    if !args.no_schema {
        let mut generated = crate::competency_questions::generate_from_schema(&db, &options)?;
        out.append(&mut generated);
    }

    if let Some(path) = args.from_nl.as_ref() {
        let prompts = crate::competency_questions::load_question_prompts(path)?;
        let needs_llm = prompts.iter().any(|p| p.query.is_none());
        let llm = if needs_llm {
            Some(resolve_llm_state_for_competency_questions(args)?)
        } else {
            None
        };
        let mut translated = crate::competency_questions::prompts_to_competency_questions(
            &db,
            llm.as_ref(),
            &prompts,
            &options,
        )?;
        out.append(&mut translated);
    }

    if out.is_empty() {
        return Err(anyhow!(
            "no competency questions generated (enable schema generation or pass --from-nl)"
        ));
    }

    if args.max_questions > 0 && out.len() > args.max_questions {
        out.truncate(args.max_questions);
    }

    let json = serde_json::to_string_pretty(&out)?;
    fs::write(&args.out, json)?;
    println!("wrote {}", args.out.display());
    Ok(())
}

fn resolve_llm_state_for_competency_questions(
    args: &CompetencyQuestionsArgs,
) -> Result<crate::llm::LlmState> {
    let selected = (args.llm_mock as usize)
        + (args.llm_ollama as usize)
        + (args.llm_openai as usize)
        + (args.llm_anthropic as usize)
        + (args.llm_plugin.is_some() as usize);
    if selected == 0 {
        return Err(anyhow!(
            "no LLM backend configured (use --llm-mock, --llm-ollama, --llm-openai, --llm-anthropic, or --llm-plugin)"
        ));
    }
    if selected > 1 {
        return Err(anyhow!(
            "choose at most one LLM backend: --llm-mock, --llm-ollama, --llm-openai, --llm-anthropic, or --llm-plugin"
        ));
    }

    let mut llm = crate::llm::LlmState::default();
    if args.llm_mock {
        llm.backend = crate::llm::LlmBackend::Mock;
        llm.model = Some("mock".to_string());
        return Ok(llm);
    }

    if let Some(plugin) = args.llm_plugin.as_ref() {
        llm.backend = crate::llm::LlmBackend::Command {
            program: plugin.clone(),
            args: args.llm_plugin_arg.clone(),
        };
        llm.model = args.llm_model.clone();
        return Ok(llm);
    }

    if args.llm_ollama {
        #[cfg(feature = "llm-ollama")]
        {
            let host = args
                .llm_ollama_host
                .clone()
                .unwrap_or_else(crate::llm::default_ollama_host);
            llm.backend = crate::llm::LlmBackend::Ollama { host };
            let model = args.llm_model.clone().ok_or_else(|| {
                anyhow!("`--llm-ollama` requires `--llm-model <model>`")
            })?;
            llm.model = Some(model);
            return Ok(llm);
        }
        #[cfg(not(feature = "llm-ollama"))]
        {
            return Err(anyhow!(
                "ollama support not compiled (enable `axiograph-cli` feature `llm-ollama`)"
            ));
        }
    }

    if args.llm_openai {
        #[cfg(feature = "llm-openai")]
        {
            let key = std::env::var(crate::llm::OPENAI_API_KEY_ENV).unwrap_or_default();
            if key.trim().is_empty() {
                return Err(anyhow!(
                    "openai backend requires {}",
                    crate::llm::OPENAI_API_KEY_ENV
                ));
            }
            llm.backend = crate::llm::LlmBackend::OpenAI {
                base_url: args
                    .llm_openai_base_url
                    .clone()
                    .unwrap_or_else(crate::llm::default_openai_base_url),
            };
            let model = args.llm_model.clone().or_else(|| {
                let env = std::env::var(crate::llm::OPENAI_MODEL_ENV).unwrap_or_default();
                let env = env.trim().to_string();
                if env.is_empty() { None } else { Some(env) }
            });
            let model = model.ok_or_else(|| {
                anyhow!(
                    "`--llm-openai` requires `--llm-model <model>` (or set {})",
                    crate::llm::OPENAI_MODEL_ENV
                )
            })?;
            llm.model = Some(model);
            return Ok(llm);
        }
        #[cfg(not(feature = "llm-openai"))]
        {
            return Err(anyhow!(
                "openai support not compiled (enable `axiograph-cli` feature `llm-openai`)"
            ));
        }
    }

    if args.llm_anthropic {
        #[cfg(feature = "llm-anthropic")]
        {
            let key =
                std::env::var(crate::llm::ANTHROPIC_API_KEY_ENV).unwrap_or_default();
            if key.trim().is_empty() {
                return Err(anyhow!(
                    "anthropic backend requires {}",
                    crate::llm::ANTHROPIC_API_KEY_ENV
                ));
            }
            llm.backend = crate::llm::LlmBackend::Anthropic {
                base_url: args
                    .llm_anthropic_base_url
                    .clone()
                    .unwrap_or_else(crate::llm::default_anthropic_base_url),
            };
            let model = args.llm_model.clone().or_else(|| {
                let env =
                    std::env::var(crate::llm::ANTHROPIC_MODEL_ENV).unwrap_or_default();
                let env = env.trim().to_string();
                if env.is_empty() { None } else { Some(env) }
            });
            let model = model.ok_or_else(|| {
                anyhow!(
                    "`--llm-anthropic` requires `--llm-model <model>` (or set {})",
                    crate::llm::ANTHROPIC_MODEL_ENV
                )
            })?;
            llm.model = Some(model);
            return Ok(llm);
        }
        #[cfg(not(feature = "llm-anthropic"))]
        {
            return Err(anyhow!(
                "anthropic support not compiled (enable `axiograph-cli` feature `llm-anthropic`)"
            ));
        }
    }

    Err(anyhow!("no LLM backend configured"))
}

const WORLD_MODEL_BACKEND_ENV: &str = "WORLD_MODEL_BACKEND";
const WORLD_MODEL_MODEL_ENV: &str = "WORLD_MODEL_MODEL";

fn resolve_llm_state_for_world_model_plugin(
    args: &WorldModelPluginLlmArgs,
) -> Result<crate::llm::LlmState> {
    let backend = args
        .backend
        .clone()
        .or_else(|| {
            env::var(WORLD_MODEL_BACKEND_ENV)
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "openai".to_string());

    let model = args
        .model
        .clone()
        .or_else(|| {
            env::var(WORLD_MODEL_MODEL_ENV)
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        });

    let backend_lc = backend.trim().to_ascii_lowercase();
    let mut llm = crate::llm::LlmState::default();
    match backend_lc.as_str() {
        "mock" => {
            llm.backend = crate::llm::LlmBackend::Mock;
            llm.model = Some("mock".to_string());
            Ok(llm)
        }
        "ollama" => {
            #[cfg(feature = "llm-ollama")]
            {
                let host = args
                    .ollama_host
                    .clone()
                    .or_else(|| {
                        env::var("OLLAMA_HOST")
                            .ok()
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                    })
                    .unwrap_or_else(crate::llm::default_ollama_host);
                let model = model
                    .or_else(|| env::var("OLLAMA_MODEL").ok().filter(|s| !s.trim().is_empty()))
                    .ok_or_else(|| {
                        anyhow!("no model selected (use --model, set WORLD_MODEL_MODEL, or set OLLAMA_MODEL)")
                    })?;
                llm.backend = crate::llm::LlmBackend::Ollama { host };
                llm.model = Some(model);
                Ok(llm)
            }
            #[cfg(not(feature = "llm-ollama"))]
            {
                Err(anyhow!(
                    "ollama support not compiled (enable `axiograph-cli` feature `llm-ollama`)"
                ))
            }
        }
        "anthropic" => {
            #[cfg(feature = "llm-anthropic")]
            {
                let key = env::var(crate::llm::ANTHROPIC_API_KEY_ENV).unwrap_or_default();
                if key.trim().is_empty() {
                    return Err(anyhow!(
                        "anthropic backend requires {}",
                        crate::llm::ANTHROPIC_API_KEY_ENV
                    ));
                }
                let base_url = args
                    .anthropic_base_url
                    .clone()
                    .or_else(|| {
                        env::var(crate::llm::ANTHROPIC_BASE_URL_ENV)
                            .ok()
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                    })
                    .unwrap_or_else(crate::llm::default_anthropic_base_url);
                let model = model
                    .or_else(|| env::var(crate::llm::ANTHROPIC_MODEL_ENV).ok().filter(|s| !s.trim().is_empty()))
                    .ok_or_else(|| {
                        anyhow!(
                            "no model selected (use --model, set WORLD_MODEL_MODEL, or set {})",
                            crate::llm::ANTHROPIC_MODEL_ENV
                        )
                    })?;
                llm.backend = crate::llm::LlmBackend::Anthropic { base_url };
                llm.model = Some(model);
                Ok(llm)
            }
            #[cfg(not(feature = "llm-anthropic"))]
            {
                Err(anyhow!(
                    "anthropic support not compiled (enable `axiograph-cli` feature `llm-anthropic`)"
                ))
            }
        }
        "openai" => {
            #[cfg(feature = "llm-openai")]
            {
                let key = env::var(crate::llm::OPENAI_API_KEY_ENV).unwrap_or_default();
                if key.trim().is_empty() {
                    return Err(anyhow!(
                        "openai backend requires {}",
                        crate::llm::OPENAI_API_KEY_ENV
                    ));
                }
                let base_url = args
                    .openai_base_url
                    .clone()
                    .or_else(|| {
                        env::var(crate::llm::OPENAI_BASE_URL_ENV)
                            .ok()
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                    })
                    .unwrap_or_else(crate::llm::default_openai_base_url);
                let model = model
                    .or_else(|| env::var(crate::llm::OPENAI_MODEL_ENV).ok().filter(|s| !s.trim().is_empty()))
                    .ok_or_else(|| {
                        anyhow!(
                            "no model selected (use --model, set WORLD_MODEL_MODEL, or set {})",
                            crate::llm::OPENAI_MODEL_ENV
                        )
                    })?;
                llm.backend = crate::llm::LlmBackend::OpenAI { base_url };
                llm.model = Some(model);
                Ok(llm)
            }
            #[cfg(not(feature = "llm-openai"))]
            {
                Err(anyhow!(
                    "openai support not compiled (enable `axiograph-cli` feature `llm-openai`)"
                ))
            }
        }
        other => Err(anyhow!(
            "world model backend `{other}` is not supported by --world-model-llm / `axiograph ingest world-model-plugin-llm` (expected openai|anthropic|ollama|mock). If you meant an ONNX or custom model, use --world-model-plugin or --world-model-http instead."
        )),
    }
}

fn cmd_world_model_propose(args: &WorldModelProposeArgs) -> Result<()> {
    let selected = (args.world_model_stub as usize)
        + (args.world_model_plugin.is_some() as usize)
        + (args.world_model_http.is_some() as usize)
        + (args.world_model_llm as usize);
    if selected > 1 {
        return Err(anyhow!(
            "choose at most one world model backend: --world-model-stub, --world-model-plugin, --world-model-http, or --world-model-llm"
        ));
    }
    if selected == 0 {
        return Err(anyhow!(
            "world model backend is not configured (use --world-model-plugin, --world-model-http, --world-model-llm, or --world-model-stub)"
        ));
    }

    let mut wm = crate::world_model::WorldModelState::default();
    if args.world_model_stub {
        wm.backend = crate::world_model::WorldModelBackend::Stub;
    } else if let Some(url) = args.world_model_http.as_ref() {
        wm.backend = crate::world_model::WorldModelBackend::Http { url: url.clone() };
    } else if args.world_model_llm {
        let exe = std::env::current_exe()
            .map_err(|e| anyhow!("failed to resolve current executable: {e}"))?;
        let mut args_list = vec!["ingest".to_string(), "world-model-plugin-llm".to_string()];
        let has_model_arg = args
            .world_model_plugin_arg
            .iter()
            .any(|a| a == "--model");
        if let Some(model) = args.world_model_model.as_ref() {
            if !has_model_arg {
                args_list.push("--model".to_string());
                args_list.push(model.clone());
            }
        }
        args_list.extend(args.world_model_plugin_arg.clone());
        wm.backend = crate::world_model::WorldModelBackend::Command {
            program: exe,
            args: args_list,
        };
    } else if let Some(plugin) = args.world_model_plugin.as_ref() {
        wm.backend = crate::world_model::WorldModelBackend::Command {
            program: plugin.clone(),
            args: args.world_model_plugin_arg.clone(),
        };
    }
    wm.model = args.world_model_model.clone();

    let input_ext = args
        .input
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    let mut axi_text: Option<String> = None;
    let mut axi_digest: Option<String> = None;
    if input_ext.eq_ignore_ascii_case("axi") {
        let text = fs::read_to_string(&args.input)?;
        axi_digest = Some(axiograph_dsl::digest::axi_digest_v1(&text));
        axi_text = Some(text);
    }

    let mut export_inline: Option<crate::world_model::JepaExportFileV1> = None;
    let mut export_path: Option<String> = None;
    if let Some(export) = args.export.as_ref() {
        export_path = Some(export.display().to_string());
    } else if let Some(text) = axi_text.as_ref() {
        let opts = crate::world_model::JepaExportOptions {
            instance_filter: args.export_instance.clone(),
            max_items: args.export_max_items,
            mask_fields: args.export_mask_fields,
            seed: args.export_seed,
        };
        let export = crate::world_model::build_jepa_export_from_axi_text(text, &opts)?;
        if let Some(out_path) = args.export_out.as_ref() {
            let json = serde_json::to_string_pretty(&export)?;
            fs::write(out_path, json)?;
            export_path = Some(out_path.display().to_string());
            println!("wrote {}", out_path.display());
        } else {
            export_inline = Some(export);
        }
    } else if args.export_out.is_some() {
        return Err(anyhow!("--export-out requires `.axi` input or --export"));
    }

    let mut db: Option<axiograph_pathdb::PathDB> = None;
    let guardrail_profile = args.guardrail_profile.trim().to_ascii_lowercase();
    let guardrail_plane = args.guardrail_plane.trim().to_ascii_lowercase();
    let guardrail_weights = if args.guardrail_weight.is_empty() {
        crate::world_model::GuardrailCostWeightsV1::defaults()
    } else {
        crate::world_model::parse_guardrail_weights(&args.guardrail_weight)?
    };

    let task_costs = crate::world_model::parse_task_costs(&args.task_cost)?;

    let guardrail = if guardrail_profile != "off" {
        let loaded = crate::load_pathdb_for_cli(&args.input)?;
        let report = crate::world_model::compute_guardrail_costs(
            &loaded,
            &args.input.display().to_string(),
            &guardrail_profile,
            &guardrail_plane,
            &guardrail_weights,
        )?;
        db = Some(loaded);
        if let Some(path) = args.guardrail_out.as_ref() {
            let json = serde_json::to_string_pretty(&report)?;
            fs::write(path, json)?;
            println!("wrote {}", path.display());
        }
        Some(report)
    } else {
        None
    };

    let mut input = crate::world_model::WorldModelInputV1::default();
    input.axi_digest_v1 = axi_digest.clone();
    input.axi_module_text = axi_text.clone();
    input.export = export_inline;
    input.export_path = export_path;
    if guardrail.is_some() {
        input.guardrail = guardrail.clone();
    }

    if input_ext.eq_ignore_ascii_case("axpd") || input_ext.eq_ignore_ascii_case("axi") {
        let kind = if input_ext.eq_ignore_ascii_case("axpd") {
            "axpd"
        } else {
            "axi"
        };
        input.snapshot = Some(crate::world_model::WorldModelSnapshotRefV1 {
            kind: kind.to_string(),
            path: args.input.display().to_string(),
            snapshot_id: None,
            accepted_snapshot_id: None,
        });
    }

    let mut options = crate::world_model::WorldModelOptionsV1::default();
    options.max_new_proposals = args.max_new_proposals;
    options.seed = args.seed;
    options.goals = args.goal.clone();
    options.task_costs = task_costs.clone();
    options.horizon_steps = args.horizon_steps;

    let req = crate::world_model::make_world_model_request(input, options);
    let mut response = wm.propose(&req)?;
    if let Some(err) = response.error.take() {
        return Err(anyhow!("world model error: {err}"));
    }

    let provenance = crate::world_model::WorldModelProvenance {
        trace_id: response.trace_id.clone(),
        backend: wm.backend_label(),
        model: wm.model.clone(),
        axi_digest_v1: axi_digest.clone(),
        guardrail_total_cost: guardrail
            .as_ref()
            .map(|g| g.summary.total_cost),
        guardrail_profile: if guardrail_profile == "off" {
            None
        } else {
            Some(guardrail_profile.clone())
        },
        guardrail_plane: if guardrail_profile == "off" {
            None
        } else {
            Some(guardrail_plane.clone())
        },
    };

    let mut proposals =
        crate::world_model::apply_world_model_provenance(response.proposals, &provenance);

    if args.max_new_proposals > 0 && proposals.proposals.len() > args.max_new_proposals {
        proposals.proposals.truncate(args.max_new_proposals);
    }

    let json = serde_json::to_string_pretty(&proposals)?;
    fs::write(&args.out, &json)?;
    println!("wrote {}", args.out.display());

    if let Some(dir) = args.commit_dir.as_ref() {
        let should_validate = args.validate.unwrap_or(true);
        if should_validate {
            let base = if let Some(db) = db.as_ref() {
                db
            } else {
                db = Some(crate::load_pathdb_for_cli(&args.input)?);
                db.as_ref().expect("db loaded")
            };
            let validation = crate::proposals_validate::validate_proposals_v1(
                base,
                &proposals,
                &args.quality,
                &args.quality_plane,
            )?;
            if !validation.ok {
                return Err(anyhow!(
                    "refusing to commit: proposals validation failed (errors={}, warnings={})",
                    validation.quality_delta.summary.error_count,
                    validation.quality_delta.summary.warning_count
                ));
            }
        }

        let res = crate::pathdb_wal::commit_pathdb_snapshot_with_overlays(
            dir,
            &args.accepted_snapshot,
            &[],
            &[args.out.clone()],
            args.commit_message.as_deref(),
        )?;
        println!(
            "ok committed {} WAL op(s) on accepted snapshot {} → pathdb snapshot {}",
            res.ops_added, res.accepted_snapshot_id, res.snapshot_id
        );
    }

    Ok(())
}

fn cmd_world_model_plugin_llm(args: &WorldModelPluginLlmArgs) -> Result<()> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| anyhow!("failed to read stdin: {e}"))?;
    if input.trim().is_empty() {
        return Err(anyhow!("expected JSON request on stdin"));
    }
    let req: crate::world_model::WorldModelRequestV1 =
        serde_json::from_str(&input).map_err(|e| anyhow!("invalid JSON request: {e}"))?;
    let llm = resolve_llm_state_for_world_model_plugin(args)?;
    let resp = crate::llm::world_model_llm_plugin(&llm, &req)?;
    let json = serde_json::to_string(&resp)?;
    println!("{json}");
    Ok(())
}

fn cmd_ingest_dir(
    root: &PathBuf,
    out_dir: &PathBuf,
    confluence_space: &str,
    domain: &str,
    chunks_path: Option<&PathBuf>,
    facts_path: Option<&PathBuf>,
    proposals_path: Option<&PathBuf>,
    max_file_bytes: u64,
    max_files: usize,
) -> Result<()> {
    println!(
        "{} {} → {}",
        "Ingesting dir".green().bold(),
        root.display(),
        out_dir.display()
    );

    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    fs::create_dir_all(out_dir)?;

    let mut all_chunks: Vec<axiograph_ingest_docs::Chunk> = Vec::new();
    let mut all_facts: Vec<axiograph_ingest_docs::ExtractedFact> = Vec::new();
    let mut all_proposals: Vec<axiograph_ingest_docs::ProposalV1> = Vec::new();
    let mut files_ingested = 0usize;

    fn chunk_by_lines(text: &str, max_chars: usize) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut cur = String::new();
        for line in text.lines() {
            let line = line.trim_end();
            if cur.len().saturating_add(line.len() + 1) > max_chars && !cur.is_empty() {
                out.push(cur);
                cur = String::new();
            }
            if !cur.is_empty() {
                cur.push('\n');
            }
            cur.push_str(line);
        }
        if !cur.trim().is_empty() {
            out.push(cur);
        }
        out
    }

    for entry in walkdir::WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            if !e.file_type().is_dir() {
                return true;
            }
            let name = e.file_name().to_string_lossy();
            name != ".git" && name != "target" && name != "build" && name != "node_modules"
        })
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().is_file() {
            continue;
        }

        if files_ingested >= max_files {
            break;
        }

        let path = entry.path();
        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if metadata.len() > max_file_bytes {
            continue;
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let rel_path = path.strip_prefix(&root).unwrap_or(path);

        // Dispatch by extension.
        match ext.as_str() {
            "md" | "txt" => {
                let text = match fs::read_to_string(path) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let stem = path.file_stem().unwrap_or_default().to_string_lossy();
                let result = axiograph_ingest_docs::extract_knowledge_full(&text, &stem, domain);

                // Emit generic proposals (claims + mentions) before moving facts.
                let proposals = axiograph_ingest_docs::proposals_from_extracted_facts_v1(
                    &result.facts,
                    Some(rel_path.to_string_lossy().to_string()),
                    Some(domain.to_string()),
                );

                all_chunks.extend(result.extraction.chunks);
                all_facts.extend(result.facts);
                all_proposals.extend(proposals);
            }
            "html" => {
                let html = match fs::read_to_string(path) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let page_id = path.file_stem().unwrap_or_default().to_string_lossy();
                match axiograph_ingest_docs::extract_knowledge_from_confluence(
                    &html,
                    &page_id,
                    confluence_space,
                ) {
                    Ok(result) => {
                        let proposals = axiograph_ingest_docs::proposals_from_extracted_facts_v1(
                            &result.facts,
                            Some(rel_path.to_string_lossy().to_string()),
                            Some("confluence".to_string()),
                        );

                        all_chunks.extend(result.extraction.chunks);
                        all_facts.extend(result.facts);
                        all_proposals.extend(proposals);
                    }
                    Err(_) => {
                        // Not all HTML is Confluence; skip quietly for now.
                        continue;
                    }
                }
            }
            "sql" => {
                let text = match fs::read_to_string(path) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let doc_id = rel_path.to_string_lossy().to_string();
                let doc_digest = axiograph_dsl::digest::fnv1a64_digest_bytes(doc_id.as_bytes());

                // Evidence chunk(s) for grounding + provenance pointers.
                let mut chunks: Vec<axiograph_ingest_docs::Chunk> = Vec::new();
                for (i, stmt) in text.split(';').enumerate() {
                    let stmt = stmt.trim();
                    if stmt.is_empty() {
                        continue;
                    }
                    let mut metadata = std::collections::HashMap::new();
                    metadata.insert("kind".to_string(), "sql_ddl".to_string());
                    metadata.insert("source_path".to_string(), doc_id.clone());
                    chunks.push(axiograph_ingest_docs::Chunk {
                        chunk_id: format!("sql_{doc_digest}_{i}"),
                        document_id: doc_id.clone(),
                        page: None,
                        span_id: format!("stmt_{i}"),
                        text: format!("{stmt};"),
                        bbox: None,
                        metadata,
                    });
                }
                all_chunks.extend(chunks.clone());

                if let Ok(sql_schema) = axiograph_ingest_sql::parse_sql_ddl(&text) {
                    all_proposals.extend(proposals_from_sql_schema(
                        &sql_schema,
                        Some(doc_id.clone()),
                        &chunks,
                    ));
                }
            }
            "json" => {
                let text = match fs::read_to_string(path) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                    let schema = axiograph_ingest_json::infer_schema(&value, "Root");
                    let doc_id = rel_path.to_string_lossy().to_string();
                    let doc_digest =
                        axiograph_dsl::digest::fnv1a64_digest_bytes(doc_id.as_bytes());
                    let pretty = serde_json::to_string_pretty(&value).unwrap_or(text.clone());
                    let parts = chunk_by_lines(&pretty, 2_500);

                    // Evidence chunks for grounding + provenance pointers.
                    let mut chunks: Vec<axiograph_ingest_docs::Chunk> = Vec::new();
                    for (i, part) in parts.into_iter().enumerate() {
                        let mut metadata = std::collections::HashMap::new();
                        metadata.insert("kind".to_string(), "json".to_string());
                        metadata.insert("source_path".to_string(), doc_id.clone());
                        chunks.push(axiograph_ingest_docs::Chunk {
                            chunk_id: format!("json_{doc_digest}_{i}"),
                            document_id: doc_id.clone(),
                            page: None,
                            span_id: format!("part_{i}"),
                            text: part,
                            bbox: None,
                            metadata,
                        });
                    }
                    all_chunks.extend(chunks.clone());
                    all_proposals.extend(proposals_from_json_schema(
                        &schema,
                        Some(doc_id.clone()),
                        &chunks,
                    ));
                }
            }
            "nt" | "ntriples" | "ttl" | "turtle" | "nq" | "nquads" | "trig" | "rdf" | "owl"
            | "xml" => match axiograph_ingest_rdfowl::proposals_from_rdf_file_v1(
                path,
                Some(rel_path.to_string_lossy().to_string()),
                Some(domain.to_string()),
            ) {
                Ok(proposals) => {
                    all_proposals.extend(proposals);
                }
                Err(_) => {
                    // Treat RDF ingestion as best-effort for now: keep going so we
                    // can still preserve text chunks for grounding.
                }
            },
            _ => continue,
        }

        // For RDF/OWL, also try to preserve a text chunk for grounding (best-effort).
        if matches!(
            ext.as_str(),
            "nt"
                | "ntriples"
                | "ttl"
                | "turtle"
                | "nq"
                | "nquads"
                | "trig"
                | "rdf"
                | "owl"
                | "xml"
        ) {
            if let Ok(text) = fs::read_to_string(path) {
                let doc_id = rel_path.to_string_lossy().to_string();
                let doc_digest = axiograph_dsl::digest::fnv1a64_digest_bytes(doc_id.as_bytes());
                let parts = chunk_by_lines(&text, 2_500);
                for (i, part) in parts.into_iter().enumerate() {
                    let mut metadata = std::collections::HashMap::new();
                    metadata.insert("kind".to_string(), "rdf".to_string());
                    metadata.insert("source_path".to_string(), doc_id.clone());
                    all_chunks.push(axiograph_ingest_docs::Chunk {
                        chunk_id: format!("rdf_{doc_digest}_{i}"),
                        document_id: doc_id.clone(),
                        page: None,
                        span_id: format!("part_{i}"),
                        text: part,
                        bbox: None,
                        metadata,
                    });
                }
            }
        }

        files_ingested += 1;
    }

    let chunks_out = chunks_path
        .cloned()
        .unwrap_or_else(|| out_dir.join("chunks.json"));
    let facts_out = facts_path
        .cloned()
        .unwrap_or_else(|| out_dir.join("facts.json"));
    let proposals_out = proposals_path
        .cloned()
        .unwrap_or_else(|| out_dir.join("proposals.json"));

    let chunks_json = serde_json::to_string_pretty(&all_chunks)?;
    fs::write(&chunks_out, &chunks_json)?;
    println!("  {} {}", "→".cyan(), chunks_out.display());

    let facts_json = serde_json::to_string_pretty(&all_facts)?;
    fs::write(&facts_out, &facts_json)?;
    println!("  {} {}", "→".cyan(), facts_out.display());

    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    let file = axiograph_ingest_docs::ProposalsFileV1 {
        version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
        generated_at,
        source: axiograph_ingest_docs::ProposalSourceV1 {
            source_type: "ingest_dir".to_string(),
            locator: root.to_string_lossy().to_string(),
        },
        schema_hint: Some(domain.to_string()),
        proposals: all_proposals,
    };
    let json = serde_json::to_string_pretty(&file)?;
    fs::write(&proposals_out, &json)?;
    println!("  {} {}", "→".cyan(), proposals_out.display());

    println!("  {} {} files ingested", "→".yellow(), files_ingested);
    Ok(())
}

fn cmd_ingest_merge(
    proposals_paths: &[PathBuf],
    chunks_paths: &[PathBuf],
    out_proposals: &PathBuf,
    out_chunks: Option<&PathBuf>,
    schema_hint_override: Option<&str>,
) -> Result<()> {
    if proposals_paths.is_empty() {
        return Err(anyhow!("ingest merge requires at least one --proposals <file.json>"));
    }

    let mut merged_proposals: Vec<axiograph_ingest_docs::ProposalV1> = Vec::new();
    let mut schema_hint: Option<String> = None;

    for p in proposals_paths {
        let text = fs::read_to_string(p)?;
        let file: axiograph_ingest_docs::ProposalsFileV1 = serde_json::from_str(&text)?;
        if schema_hint.is_none() {
            schema_hint = file.schema_hint.clone();
        }
        merged_proposals.extend(file.proposals);
    }

    // Deduplicate by proposal_id (stable identifiers).
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut deduped: Vec<axiograph_ingest_docs::ProposalV1> = Vec::with_capacity(merged_proposals.len());
    for p in merged_proposals {
        let id = match &p {
            axiograph_ingest_docs::ProposalV1::Entity { meta, .. } => meta.proposal_id.clone(),
            axiograph_ingest_docs::ProposalV1::Relation { meta, .. } => meta.proposal_id.clone(),
        };
        if seen.insert(id) {
            deduped.push(p);
        }
    }

    let schema_hint = schema_hint_override
        .map(|s| s.to_string())
        .or(schema_hint);

    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();

    let locator = proposals_paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");

    let file = axiograph_ingest_docs::ProposalsFileV1 {
        version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
        generated_at,
        source: axiograph_ingest_docs::ProposalSourceV1 {
            source_type: "merge".to_string(),
            locator,
        },
        schema_hint,
        proposals: deduped,
    };

    fs::create_dir_all(out_proposals.parent().unwrap_or(std::path::Path::new(".")))?;
    fs::write(out_proposals, serde_json::to_string_pretty(&file)?)?;
    println!("wrote {}", out_proposals.display());

    if !chunks_paths.is_empty() {
        let mut merged_chunks: Vec<axiograph_ingest_docs::Chunk> = Vec::new();
        for p in chunks_paths {
            let text = fs::read_to_string(p)?;
            let chunks: Vec<axiograph_ingest_docs::Chunk> = serde_json::from_str(&text)?;
            merged_chunks.extend(chunks);
        }

        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut deduped: Vec<axiograph_ingest_docs::Chunk> =
            Vec::with_capacity(merged_chunks.len());
        for c in merged_chunks {
            if seen.insert(c.chunk_id.clone()) {
                deduped.push(c);
            }
        }

        let out_path = out_chunks
            .cloned()
            .unwrap_or_else(|| out_proposals.parent().unwrap_or(std::path::Path::new(".")).join("chunks.json"));
        fs::create_dir_all(out_path.parent().unwrap_or(std::path::Path::new(".")))?;
        fs::write(&out_path, serde_json::to_string_pretty(&deduped)?)?;
        println!("wrote {}", out_path.display());
    }

    Ok(())
}

// Legacy `.axi` emission has been removed. Ingestion produces `proposals.json`
// first; promotion into canonical `.axi` is explicit and reviewable.
