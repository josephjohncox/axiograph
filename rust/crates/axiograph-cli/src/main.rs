//! Axiograph CLI
//!
//! Unified command-line interface for:
//! - Validating canonical `.axi` modules (`axi_v1`)
//! - Ingesting sources into `proposals.json` (Evidence/Proposals schema)
//! - Promoting proposals into candidate domain `.axi` modules (explicit, reviewable)
//! - Publishing and inspecting authenticated SQLite `.axpd` materializations

use anyhow::{anyhow, Context, Result};
use clap::{Args, Parser, Subcommand};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

mod analyze;
mod authoring_workspace;
mod axi_fmt;
mod axi_input;
mod axql;
mod behavior_case;
mod competency_questions;
mod context_report;
mod db_server;
mod doc_chunks;
mod embeddings;
mod evolution_preview;
mod github;
mod llm;
mod mcp;
mod nlq;
mod perf;
mod predictive_proposal_input;
mod predictive_proposals;
mod profiling;
mod projection;
mod proposal_gen;
mod proposals_import;
mod proposals_validate;
mod proto;
mod quality;
mod query_ir;
mod relation_resolution;
mod repl;
mod repl_command;
mod route_preview;
mod route_preview_tools;
mod runtime_theory_check;
mod schema_discovery;
mod security;
mod semantic_claim;
mod semantic_merge_lattice;
mod semantic_model;
mod semantic_tools;
mod sqlish;
mod synthetic_pathdb;
mod transport_preview_tools;
mod trust_contract;
mod typed_authoring;
mod typed_refinement;
mod verifier_bridge;
mod viz;
mod web;

#[derive(Parser)]
#[command(name = "axiograph")]
#[command(
    author,
    version,
    about = "Axiograph: proof-carrying ontology workbench with canonical .axi authority"
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
    Ingest {
        #[command(subcommand)]
        command: IngestCommands,
    },

    /// Check/lint exact canonical `.axi` modules.
    ///
    /// Runs Rust-side validation and quality reports over reviewable inputs.
    Check {
        #[command(subcommand)]
        command: CheckCommands,
    },

    /// Emit untrusted certificates; this command does not invoke Lean.
    ///
    /// Verify emitted certificates separately with the trusted
    /// `axiograph_verify` checker or a `make verify-lean-*` target.
    Cert {
        #[command(subcommand)]
        command: CertCommands,
    },

    /// Tooling commands (visualization, analysis, performance runners).
    Tools {
        #[command(subcommand)]
        command: ToolsCommands,
    },

    /// Software-authoring, codegen, overlay, and editor-integration commands.
    Authoring {
        #[command(subcommand)]
        command: AuthoringCommands,
    },

    /// Manage accepted state and serve authenticated AxiStore materializations.
    Db {
        #[command(subcommand)]
        command: DbCommands,
    },

    /// Run a read-only stdio MCP transport over typed semantic services.
    Mcp(McpArgs),

    /// Run discovery tasks over ingestion artifacts (chunks/facts/edges)
    Discover {
        #[command(subcommand)]
        command: DiscoverCommands,
    },

    /// Interactive REPL with process-local query state.
    Repl {
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
}

#[derive(Subcommand)]
enum CheckCommands {
    /// Validate a canonical `.axi` module (parse + typecheck).
    Validate {
        /// Input `.axi` file.
        input: PathBuf,
    },

    /// Classify compiled theory obligations under an explicit runtime scope.
    Theory(CheckTheoryArgs),

    /// Execute and Lean-verify one exact finite query against canonical `.axi` bytes.
    FiniteQuery(CheckFiniteQueryArgs),

    /// Check behavior-case software coverage against a typed tooling overlay.
    SoftwareCoverage(CheckSoftwareCoverageArgs),

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

    /// Lint/quality checks for canonical `.axi` modules.
    ///
    /// This is a practical ontology-engineering helper. It produces a structured
    /// report (JSON/text) and exits non-zero when errors are found (unless
    /// `--no-fail` is set).
    Quality {
        /// Input canonical `.axi` file.
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

#[derive(Args, Debug, Clone)]
struct CheckTheoryArgs {
    /// Input canonical `.axi` module.
    input: PathBuf,

    /// Optional theory id or local theory name filter.
    #[arg(long)]
    theory: Option<String>,

    /// Closure tier: finite_fragment|evidence_weighted|global_indexed.
    #[arg(long, default_value = "finite_fragment")]
    closure_tier: String,

    /// Declared world id for scoped closure.
    #[arg(long)]
    world_id: Option<String>,

    /// Treat the declared world as non-finite, making closure advisory.
    #[arg(long = "non-finite-world", action = clap::ArgAction::SetFalse, default_value_t = true)]
    finite_world: bool,

    /// Include a semantic ref in the declared global/indexed universe.
    #[arg(long = "included-ref")]
    included_refs: Vec<String>,

    /// Include a world id in the declared global/indexed universe.
    #[arg(long = "included-world")]
    included_worlds: Vec<String>,

    /// Include a semantic slice id in the declared global/indexed universe.
    #[arg(long = "included-slice")]
    included_slices: Vec<String>,

    /// Include an import anchor in the declared global/indexed universe.
    #[arg(long = "included-import")]
    included_imports: Vec<String>,

    /// Declare an import that should make global_indexed closure fail closed.
    #[arg(long = "undeclared-import")]
    undeclared_imports: Vec<String>,

    /// Evidence threshold in parts per million.
    #[arg(long)]
    evidence_threshold_ppm: Option<u32>,

    /// Evidence semantics: thresholded_world|weighted_lattice|deferred.
    #[arg(long, default_value = "thresholded_world")]
    evidence_semantics: String,

    /// Enable conservative weighted-lattice propagation before thresholding.
    #[arg(long)]
    weighted_evidence: bool,

    /// Per-obligation evidence weight as obligation_id=ppm.
    #[arg(long = "evidence-weight")]
    evidence_weights: Vec<String>,

    /// Emit JSON to stdout unless --out is provided.
    #[arg(long)]
    json: bool,

    /// Output JSON path.
    #[arg(short, long)]
    out: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
struct CheckFiniteQueryArgs {
    /// Exact canonical `.axi` module and finite instance used as the query anchor.
    input: PathBuf,

    /// JSON-encoded `query_ir_v1` request.
    #[arg(long)]
    query: PathBuf,

    /// Approved Lean verifier executable.
    #[arg(long)]
    verify_bin: PathBuf,

    /// SHA-256 of the approved verifier executable.
    #[arg(long)]
    verify_sha256: String,

    /// Approved stdio V2 checker build id.
    #[arg(long, default_value = "axiograph-verify-main-v3")]
    verify_build_id: String,

    /// Verifier timeout in seconds.
    #[arg(long, default_value_t = 30)]
    verify_timeout_secs: u64,

    /// Output verification report. Defaults to stdout.
    #[arg(short, long)]
    out: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
struct CheckSoftwareCoverageArgs {
    /// Input canonical `.axi` module.
    input: PathBuf,

    /// Typed behavior-case request payload file (`BehaviorCaseCheckRequestV1`).
    #[arg(long)]
    behavior_case: PathBuf,

    /// Typed tooling overlay payload file (`ToolingOverlayBundleV1`).
    #[arg(long)]
    overlay: PathBuf,

    /// Optional `.cq` or `competency_question_bundle_v1` JSON file to attach
    /// before building the coverage report. Prefer `.cq` for authored CQs.
    #[arg(long = "cq-file")]
    cq_files: Vec<PathBuf>,

    /// Repository root used to resolve code_refs.
    #[arg(long, default_value = ".")]
    repo_root: PathBuf,

    /// Output JSON path. Defaults to stdout.
    #[arg(short, long)]
    out: Option<PathBuf>,
}

#[derive(Subcommand)]
enum ToolsCommands {
    /// Visualize a canonical `.axi` module as a neighborhood graph.
    Viz(VizArgs),

    /// Tooling-focused analysis commands (untrusted / evidence-plane friendly).
    Analyze {
        #[command(subcommand)]
        command: analyze::AnalyzeCommands,
    },

    /// Performance runners (synthetic ingestion/query timings).
    Perf {
        #[command(subcommand)]
        command: perf::PerfCommands,
    },

    /// Emit and audit capability-declared derived backend projections.
    Projection {
        #[command(subcommand)]
        command: projection::ProjectionCommands,
    },
}

#[derive(Subcommand)]
enum AuthoringCommands {
    /// Run a cataloged software-authoring example flow and emit `authoring_suite_run_report_v1`.
    Run {
        /// Typed suite catalog file, for example examples/software_authoring/software_authoring_examples.json.
        #[arg(long)]
        suite: PathBuf,
        /// Example id from the suite catalog.
        #[arg(long)]
        example: String,
        /// Coverage profile: advisory, strict, or ci.
        #[arg(long, default_value = "advisory")]
        profile: String,
        /// Repository root used to resolve code_refs.
        #[arg(long, default_value = ".")]
        repo_root: PathBuf,
        /// Optional directory for typed per-step report files.
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// Output `authoring_suite_run_report_v1` path. Defaults to stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Return a non-zero exit code when the selected profile fails.
        #[arg(long)]
        fail_on_blocking: bool,
    },

    /// Return codegen skeleton file hints from a typed tooling overlay.
    CodegenPlan {
        /// Typed tooling overlay payload file (`ToolingOverlayBundleV1`).
        #[arg(long)]
        overlay: PathBuf,
        /// Output JSON path. Defaults to stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },

    /// Run the unified workspace-aware typed authoring service from one JSON request.
    Workspace {
        /// Workspace root. Every source path in the request is resolved beneath it.
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
        /// JSON-encoded `authoring_workspace_request_v1`.
        #[arg(long)]
        request: PathBuf,
        /// Output JSON path. Defaults to stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },

    /// Materialize generated test skeleton previews from a behavior-case report.
    MaterializeSkeletons {
        /// Path to a behavior_case_report_v1 JSON file.
        #[arg(long)]
        behavior_report: PathBuf,
        /// Output directory for generated skeletons.
        #[arg(long)]
        out_dir: PathBuf,
        /// Optional comma-separated language filter.
        #[arg(long, value_delimiter = ',')]
        language: Vec<String>,
        /// Overwrite existing generated files.
        #[arg(long)]
        overwrite: bool,
        /// Output JSON path. Defaults to stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },

    /// Check a behavior-case report as a continuous software-coverage gate.
    ContinuousCheck {
        /// Path to a behavior_case_report_v1 JSON file.
        #[arg(long)]
        behavior_report: PathBuf,
        /// Repository root used to resolve code_refs from the behavior report.
        #[arg(long, default_value = ".")]
        repo_root: PathBuf,
        /// Comma-separated list of generated languages required in codegen_previews.
        #[arg(
            long,
            value_delimiter = ',',
            default_value = "rust,typescript,python,go"
        )]
        require_codegen: Vec<String>,
        /// Fail when ontology rules are not mapped to implementation/test coverage.
        #[arg(long)]
        strict_coverage: bool,
        /// Fail when code_refs do not exist on disk.
        #[arg(long)]
        require_code_refs: bool,
        /// Fail unless the report carries a runtime theory-check summary.
        #[arg(long)]
        require_runtime_theory: bool,
        /// Output JSON path. Defaults to stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },

    /// Emit software-coverage, overlay-codegen, and workspace-service tool metadata.
    ToolSpecs {
        /// Output JSON path. Defaults to stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },

    /// Emit minimal LSP capability metadata for editor integrations.
    LspCapabilities {
        /// Output JSON path. Defaults to stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },

    /// Emit launch metadata for the unified LSP/MCP/HTTP authoring adapters.
    IntegrationManifest {
        /// Workspace root embedded in adapter launch commands.
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
        /// Output JSON path. Defaults to stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },

    /// Run the stdio LSP adapter over the unified workspace service.
    Lsp {
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
        /// Default workspace-relative `.axi` root used when diagnosing `.cq` buffers.
        #[arg(long)]
        axi: Option<String>,
    },

    /// Run the read-only stdio MCP adapter over the unified workspace service.
    Mcp {
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
    },

    /// Run the read-only HTTP adapter over the unified workspace service.
    Serve {
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
        #[arg(long, default_value = "127.0.0.1:8787")]
        listen: std::net::SocketAddr,
    },
}

#[derive(Subcommand)]
enum DbCommands {
    /// Publish a deterministic SQLite materialization from an explicit JSON build spec.
    Materialize {
        /// Initialized AxiStore root.
        #[arg(long)]
        dir: PathBuf,
        /// JSON-encoded `AxpdBuildSpec` with exact semantic anchors.
        #[arg(long)]
        spec: PathBuf,
    },

    /// Verify and print one immutable materialization receipt.
    MaterializationShow {
        /// Initialized AxiStore root.
        #[arg(long)]
        dir: PathBuf,
        /// Exact `MaterializationIdV2`.
        #[arg(long)]
        materialization: String,
    },

    /// Serve one immutable, authenticated AxiStore materialization read-only.
    ///
    /// Startup verifies the receipt, exact SQLite image, logical rows, and
    /// semantic anchors before publishing `/healthz`, `/status`, and `/query`.
    Serve(DbServeArgs),
}

#[derive(Args, Debug, Clone)]
struct DbServeArgs {
    /// Listen address (use `127.0.0.1:0` to auto-pick a free port).
    #[arg(long, default_value = "127.0.0.1:7878")]
    listen: std::net::SocketAddr,

    /// AxiStore root containing the immutable materialization and receipt.
    #[arg(long)]
    dir: PathBuf,

    /// Exact immutable `MaterializationIdV2` to verify and serve.
    #[arg(long)]
    materialization: String,

    /// Maximum number of simultaneously open HTTP connections.
    #[arg(long, default_value_t = 128)]
    max_connections: usize,

    /// Maximum lifetime of an HTTP connection in seconds.
    #[arg(long, default_value_t = 300)]
    connection_timeout_secs: u64,

    /// Write a readiness receipt after verification and listener publication.
    #[arg(long)]
    ready_file: Option<PathBuf>,

    /// LRU capacity for deeper-than-indexed paths; zero disables it.
    #[arg(long, default_value_t = 0)]
    path_index_lru_capacity: usize,

    /// Enable asynchronous updates for the process-local path LRU.
    #[arg(long)]
    path_index_lru_async: bool,

    /// Async path-LRU update queue size.
    #[arg(long, default_value_t = 1024)]
    path_index_lru_queue: usize,
}

#[derive(Args, Debug, Clone)]
struct McpArgs {
    /// AxiStore root containing the immutable materialization and receipt.
    #[arg(long)]
    dir: PathBuf,

    /// Exact immutable `MaterializationIdV2` to verify and expose.
    #[arg(long)]
    materialization: String,

    /// Hard cap for rows returned by `axql_run` calls.
    #[arg(long, default_value_t = 50)]
    tool_max_rows: usize,

    /// Approved Lean verifier executable for MCP query verification.
    #[arg(long)]
    verify_bin: Option<PathBuf>,

    /// SHA-256 of the approved verifier executable.
    #[arg(long)]
    verify_sha256: Option<String>,

    /// Approved stdio V2 checker build id.
    #[arg(long)]
    verify_build_id: Option<String>,

    /// Query verifier timeout in seconds.
    #[arg(long, default_value_t = 30)]
    verify_timeout_secs: u64,
}

#[derive(Subcommand)]
enum CertCommands {
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
    /// Input canonical `.axi` file.
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
    /// Include all nodes (ignores focus + hops; still respects max_nodes/max_edges).
    #[arg(long)]
    all: bool,
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

    /// Run a predictive proposal adapter plugin to propose new facts/relations (evidence plane).
    PredictiveProposal(PredictiveProposalsArgs),

    /// Built-in predictive proposal adapter plugin (LLM-backed). Reads request JSON from stdin and writes a response to stdout.
    #[command(name = "predictive-proposals-llm")]
    PredictiveProposalPluginLlm(PredictiveProposalsLlmArgs),
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

    /// Export masked-tuple SSL training pairs from a canonical `.axi` module.
    ///
    /// This exports **full** schema+theory+instance context and a list of
    /// masked targets derived from instance tuples. It is anchored to the
    /// module's exact-byte `revision_digest_v2` and is suitable for
    /// self-supervised training pipelines.
    #[command(name = "training-export")]
    MaskedTupleTrainingExport {
        /// Input canonical `.axi` module
        input: PathBuf,
        /// Output `MaskedTupleTrainingExportV1` (`version=axi_training_export_v1`).
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

    /// Generate, load, or lower competency questions for coverage checks.
    CompetencyQuestions(CompetencyQuestionsArgs),

    /// Check a typed olog fragment against canonical `.axi` and optionally
    /// apply one typed refinement handle before re-checking.
    CheckOlog(DiscoverCheckOlogArgs),

    /// Compose a read-only bounded-context report over existing semantic, CQ,
    /// trust, and optional evolution-preview contracts.
    ContextReport(DiscoverContextReportArgs),

    /// Validate a typed DDD/fDDD/software tooling overlay against reviewable `.axi`.
    OverlayCheck(DiscoverOverlayCheckArgs),

    /// Run an advisory software coverage lookup over ontology plus optional overlay.
    CoverageQuery(DiscoverCoverageQueryArgs),

    /// Define a process, function, business rule, relation, or surface from advisory prompts.
    Define(DiscoverDefineArgs),

    /// Discover advisory embedding-derived relationship evidence from an embeddings JSON file.
    EmbeddingRelationships(DiscoverEmbeddingRelationshipsArgs),

    /// Check a typed BehaviorCaseV1 request and emit trust receipts plus test skeleton previews.
    BehaviorCase(DiscoverBehaviorCaseArgs),

    /// Preview a concrete route and optional route equivalence over a snapshot.
    RoutePreview(DiscoverRoutePreviewArgs),

    /// Preview schema-morphism transport over canonical `.axi` as an EvolutionPreviewV1.
    TransportPreview(DiscoverTransportPreviewArgs),

    /// Emit runtime theory-obligation graphs from a canonical `.axi` module.
    TheoryGraph(DiscoverTheoryGraphArgs),

    /// Inspect the derived runtime semantic index for canonical `.axi`.
    KernelSurface(DiscoverKernelSurfaceArgs),

    /// Emit scoped runtime theory checker reports from canonical `.axi`.
    TheoryCheck(DiscoverTheoryCheckArgs),

    /// Run a predictive proposal adapter plugin to propose new facts/relations (evidence plane).
    PredictiveProposalPropose(PredictiveProposalsArgs),
}

#[derive(Args, Debug, Clone)]
struct DiscoverCheckOlogArgs {
    /// Input canonical `.axi` module.
    input: PathBuf,

    /// Input JSON file containing `OlogFragmentV1`.
    #[arg(long)]
    fragment: PathBuf,

    /// Optional schema name if the module contains multiple schemas.
    #[arg(long)]
    schema: Option<String>,

    /// Optional runtime refinement handle id to apply before returning the report.
    #[arg(long)]
    apply_refinement_handle_id: Option<String>,

    /// Output JSON path (defaults to stdout).
    #[arg(short, long)]
    out: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
struct DiscoverTheoryGraphArgs {
    /// Input canonical `.axi` module.
    input: PathBuf,

    /// Optional theory id or local theory name filter.
    #[arg(long)]
    theory: Option<String>,

    /// Output JSON path. Defaults to stdout.
    #[arg(short, long)]
    out: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
struct DiscoverKernelSurfaceArgs {
    /// Input canonical `.axi` module.
    input: PathBuf,

    /// Output JSON path. Defaults to stdout.
    #[arg(short, long)]
    out: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
struct DiscoverTheoryCheckArgs {
    /// Input canonical `.axi` module.
    input: PathBuf,

    /// Optional theory id or local theory name filter.
    #[arg(long)]
    theory: Option<String>,

    /// Closure tier: finite_fragment|evidence_weighted|global_indexed.
    #[arg(long, default_value = "finite_fragment")]
    closure_tier: String,

    /// Declared world id for scoped closure.
    #[arg(long)]
    world_id: Option<String>,

    /// Treat the declared world as non-finite, making closure advisory.
    #[arg(long = "non-finite-world", action = clap::ArgAction::SetFalse, default_value_t = true)]
    finite_world: bool,

    /// Include a semantic ref in the declared global/indexed universe.
    #[arg(long = "included-ref")]
    included_refs: Vec<String>,

    /// Include a world id in the declared global/indexed universe.
    #[arg(long = "included-world")]
    included_worlds: Vec<String>,

    /// Include a semantic slice id in the declared global/indexed universe.
    #[arg(long = "included-slice")]
    included_slices: Vec<String>,

    /// Include an import anchor in the declared global/indexed universe.
    #[arg(long = "included-import")]
    included_imports: Vec<String>,

    /// Declare an import that should make global_indexed closure fail closed.
    #[arg(long = "undeclared-import")]
    undeclared_imports: Vec<String>,

    /// Evidence threshold in parts per million.
    #[arg(long)]
    evidence_threshold_ppm: Option<u32>,

    /// Evidence semantics: thresholded_world|weighted_lattice|deferred.
    #[arg(long, default_value = "thresholded_world")]
    evidence_semantics: String,

    /// Enable conservative weighted-lattice propagation before thresholding.
    #[arg(long)]
    weighted_evidence: bool,

    /// Per-obligation evidence weight as obligation_id=ppm.
    #[arg(long = "evidence-weight")]
    evidence_weights: Vec<String>,

    /// Output JSON path. Defaults to stdout.
    #[arg(short, long)]
    out: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
struct DiscoverContextReportArgs {
    /// Input canonical `.axi` module.
    input: PathBuf,

    /// Input JSON file containing `ContextReportRequestV1`.
    #[arg(long)]
    request: PathBuf,

    /// Output JSON path (defaults to stdout).
    #[arg(short, long)]
    out: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
struct DiscoverOverlayCheckArgs {
    /// Input canonical `.axi` module.
    input: PathBuf,

    /// Input JSON file containing `ToolingOverlayBundleV1`.
    #[arg(long)]
    overlay: PathBuf,

    /// Output JSON path (defaults to stdout).
    #[arg(short, long)]
    out: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
struct DiscoverCoverageQueryArgs {
    /// Input canonical `.axi` module.
    input: PathBuf,

    /// Optional input JSON file containing `CoverageQueryV1`.
    #[arg(long)]
    query: Option<PathBuf>,

    /// Search term. Repeat for multiple terms.
    #[arg(long = "term")]
    terms: Vec<String>,

    /// Relation name to probe. Repeat for multiple relations.
    #[arg(long = "relation")]
    relation_names: Vec<String>,

    /// Competency-question name to probe. Repeat for multiple CQs.
    #[arg(long = "cq-name")]
    cq_names: Vec<String>,

    /// Code reference to probe. Repeat for multiple refs.
    #[arg(long = "code-ref")]
    code_refs: Vec<String>,

    /// Implementation surface hint to probe. Repeat for multiple hints.
    #[arg(long = "surface-hint")]
    surface_hints: Vec<String>,

    /// Optional AxQL fragment for advanced/debug coverage probes.
    #[arg(long)]
    axql: Option<String>,

    /// Maximum candidate matches to return.
    #[arg(long)]
    max_matches: Option<usize>,

    /// Optional input JSON file containing `ToolingOverlayBundleV1`.
    #[arg(long)]
    overlay: Option<PathBuf>,

    /// Output JSON path (defaults to stdout).
    #[arg(short, long)]
    out: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
struct DiscoverDefineArgs {
    /// Input canonical `.axi` module.
    input: PathBuf,

    /// Weak prompt such as "define the reserve-credit process".
    #[arg(long)]
    prompt: String,

    /// Optional kind hint: process|function|business_rule|domain_object|relation|invariant|policy|implementation_surface.
    #[arg(long)]
    kind_hint: Option<String>,

    /// Optional context hint for authoring/discovery.
    #[arg(long)]
    context_hint: Option<String>,

    /// Optional input JSON file containing `ToolingOverlayBundleV1`.
    #[arg(long)]
    overlay: Option<PathBuf>,

    /// Include suggested AxQL query fragments in candidates.
    #[arg(long)]
    include_queries: bool,

    /// Maximum candidate definitions to return.
    #[arg(long)]
    max_matches: Option<usize>,

    /// Output JSON path (defaults to stdout).
    #[arg(short, long)]
    out: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
struct DiscoverEmbeddingRelationshipsArgs {
    /// Input JSON file containing EmbeddingsFileV1.
    #[arg(long)]
    embeddings: PathBuf,

    /// Review ref used to scope the advisory sidecar.
    #[arg(long, default_value = "heads/main")]
    accepted_ref: String,

    /// Accepted snapshot id for the sidecar anchor.
    #[arg(long)]
    accepted_snapshot_id: String,

    /// Canonical `.axi` digest for the sidecar anchor.
    #[arg(long)]
    axi_digest: String,

    /// Optional compiled IR digest.
    #[arg(long)]
    compiled_ir_digest: Option<String>,

    /// Optional module name.
    #[arg(long)]
    module_name: Option<String>,

    /// Optional embedding model version for sidecar provenance.
    #[arg(long)]
    model_version: Option<String>,

    /// Optional embedding model digest for sidecar provenance.
    #[arg(long)]
    model_digest: Option<String>,

    /// Optional embedding deployment id for sidecar provenance.
    #[arg(long)]
    deployment_id: Option<String>,

    /// Minimum cosine similarity in [-1, 1].
    #[arg(long, default_value_t = 0.75)]
    min_cosine_similarity: f32,

    /// Maximum advisory relationships to emit.
    #[arg(long, default_value_t = 32)]
    max_relationships: usize,

    /// Advisory relationship kind, for example similar_to or subtype_candidate.
    #[arg(long, default_value = "similar_to")]
    relationship: String,

    /// Output JSON path (defaults to stdout).
    #[arg(short, long)]
    out: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
struct DiscoverBehaviorCaseArgs {
    /// Input canonical `.axi` module.
    input: PathBuf,

    /// Typed behavior-case request payload file (`BehaviorCaseCheckRequestV1`).
    #[arg(long)]
    request: PathBuf,

    /// Optional typed tooling overlay; when present, DDD/fDDD context,
    /// implementation surfaces, coverage edges, and codegen are loaded from it.
    #[arg(long)]
    overlay: Option<PathBuf>,

    /// Optional `.cq` or `competency_question_bundle_v1` JSON file to attach to
    /// the behavior case before checking. Prefer `.cq` for user-authored CQs.
    #[arg(long = "cq-file")]
    cq_files: Vec<PathBuf>,

    /// Output JSON path (defaults to stdout).
    #[arg(short, long)]
    out: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
struct DiscoverRoutePreviewArgs {
    /// Input canonical `.axi` module.
    input: PathBuf,

    /// Input JSON file containing `RoutePreviewRequestV1`.
    #[arg(long)]
    request: PathBuf,

    /// Output JSON path (defaults to stdout).
    #[arg(short, long)]
    out: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
struct DiscoverTransportPreviewArgs {
    /// Input canonical `.axi` module.
    input: PathBuf,

    /// Input JSON file containing `SchemaMorphismV1`.
    #[arg(long)]
    morphism: PathBuf,

    /// Optional schema name if the module contains multiple schemas.
    #[arg(long)]
    schema: Option<String>,

    /// Optional runtime refinement handle id to apply before returning the preview.
    #[arg(long)]
    apply_refinement_handle_id: Option<String>,

    /// Output JSON path (defaults to stdout).
    #[arg(short, long)]
    out: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
struct PredictiveProposalsArgs {
    /// Exact canonical `.axi` input used for guardrails and validation.
    input: PathBuf,

    /// Instance filter for generated masked-tuple training input.
    #[arg(long)]
    export_instance: Option<String>,

    /// Cap the number of masked-tuple training items generated (0 = no cap).
    #[arg(long, default_value_t = 0)]
    export_max_items: usize,

    /// Number of fields to mask per training item.
    #[arg(long, default_value_t = 1)]
    export_mask_fields: usize,

    /// Random seed for masked-tuple training item generation.
    #[arg(long, default_value_t = 1)]
    export_seed: u64,

    /// Output proposals JSON (Evidence/Proposals schema).
    #[arg(short, long)]
    out: PathBuf,

    /// Optional predictive proposal adapter plugin executable (speaks `axiograph_predictive_proposal_v1`).
    #[arg(long = "proposal-adapter-plugin")]
    predictive_proposal_plugin: Option<PathBuf>,

    /// Extra args for `--proposal-adapter-plugin` (repeatable).
    #[arg(long = "proposal-adapter-plugin-arg")]
    predictive_proposal_plugin_arg: Vec<String>,

    /// Optional predictive proposal adapter HTTP endpoint (speaks `axiograph_predictive_proposal_v1`).
    #[arg(long = "proposal-adapter-http")]
    predictive_proposal_http: Option<String>,

    /// Use the built-in LLM-backed predictive proposal adapter plugin.
    #[arg(long = "proposal-adapter-llm")]
    predictive_proposal_llm: bool,

    /// Use the stub predictive proposal adapter backend (emits no proposals).
    #[arg(long = "proposal-adapter-stub")]
    predictive_proposal_stub: bool,

    /// Optional model name for provenance (free-form).
    #[arg(long = "proposal-adapter-model")]
    predictive_proposal_model: Option<String>,

    /// Max new proposals to keep (0 = no cap).
    #[arg(long, default_value_t = 0)]
    max_new_proposals: usize,

    /// Optional goal strings passed to the predictive proposal adapter (repeatable).
    #[arg(long)]
    goal: Vec<String>,

    /// Optional random seed passed to the predictive proposal adapter.
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

    /// Optional planning horizon (steps) passed to the predictive proposal adapter.
    #[arg(long)]
    horizon_steps: Option<usize>,

    /// Optional guardrail report output path.
    #[arg(long)]
    guardrail_out: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
struct PredictiveProposalsLlmArgs {
    /// Backend: openai|anthropic|ollama|mock (defaults to PREDICTIVE_PROPOSAL_BACKEND or openai).
    #[arg(long)]
    backend: Option<String>,

    /// Optional model name (defaults to PREDICTIVE_PROPOSAL_MODEL or provider defaults).
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
    /// Exact canonical `.axi` input for schema and NL query translation.
    input: PathBuf,

    /// Output JSON file (array of competency questions).
    #[arg(short, long)]
    out: PathBuf,

    /// Optional natural-language question file (txt or json) to translate.
    #[arg(long)]
    from_nl: Option<PathBuf>,

    /// Optional authored `.cq` file to load/lower without requiring users to
    /// write JSON or AxQL directly.
    #[arg(long)]
    from_cq: Option<PathBuf>,

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

fn main() -> Result<()> {
    let cli = Cli::parse();
    let profiler = profiling::Profiler::start(&cli.profile)?;

    let result = (|| {
        match cli.command {
            Commands::Ingest { command } => match command {
                IngestCommands::Sql { input, out, chunks } => {
                    cmd_sql(&input, &out, chunks.as_deref())?;
                }
                IngestCommands::Doc {
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
                        chunks.as_deref(),
                        facts.as_deref(),
                        machining,
                        &domain,
                    )?;
                }
                IngestCommands::Conversation {
                    input,
                    out,
                    chunks,
                    facts,
                    format,
                } => {
                    cmd_conversation(&input, &out, chunks.as_deref(), facts.as_deref(), &format)?;
                }
                IngestCommands::Confluence {
                    input,
                    out,
                    space,
                    chunks,
                    facts,
                } => {
                    cmd_confluence(&input, &out, &space, chunks.as_deref(), facts.as_deref())?;
                }
                IngestCommands::Json { input, out, chunks } => {
                    cmd_json(&input, &out, chunks.as_deref())?;
                }
                IngestCommands::Readings {
                    input,
                    out,
                    chunks,
                    format,
                } => {
                    cmd_readings(&input, &out, chunks.as_deref(), &format)?;
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
                            chunks.as_deref(),
                            edges.as_deref(),
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
                            chunks.as_deref(),
                            edges.as_deref(),
                            trace.as_deref(),
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
                        chunks.as_deref(),
                        facts.as_deref(),
                        proposals.as_deref(),
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
                        chunks_out.as_deref(),
                        schema_hint.as_deref(),
                    )?;
                }
                IngestCommands::PredictiveProposal(args) => {
                    cmd_predictive_proposals(&args)?;
                }
                IngestCommands::PredictiveProposalPluginLlm(args) => {
                    cmd_predictive_proposal_plugin_llm(&args)?;
                }
            },
            Commands::Check { command } => match command {
                CheckCommands::Validate { input } => {
                    cmd_validate(&input)?;
                }
                CheckCommands::Theory(args) => {
                    cmd_check_theory(&args)?;
                }
                CheckCommands::FiniteQuery(args) => {
                    cmd_check_finite_query(&args)?;
                }
                CheckCommands::SoftwareCoverage(args) => {
                    cmd_check_software_coverage(&args)?;
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
                    quality::cmd_quality(
                        &input,
                        out.as_deref(),
                        &format,
                        &profile,
                        &plane,
                        no_fail,
                    )?;
                }
            },
            Commands::Cert { command } => match command {
                CertCommands::Typecheck { input, out } => {
                    cmd_typecheck_cert(&input, out.as_deref())?;
                }
                CertCommands::Constraints { input, out } => {
                    cmd_constraints_cert(&input, out.as_deref())?;
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
                ToolsCommands::Projection { command } => {
                    projection::cmd_projection(command)?;
                }
            },
            Commands::Authoring { command } => {
                cmd_authoring(command)?;
            }
            Commands::Db { command } => match command {
                DbCommands::Materialize { dir, spec } => {
                    cmd_publish_materialization(&dir, &spec)?;
                }
                DbCommands::MaterializationShow {
                    dir,
                    materialization,
                } => {
                    cmd_show_materialization(&dir, &materialization)?;
                }
                DbCommands::Serve(args) => {
                    db_server::cmd_db_serve(args)?;
                }
            },
            Commands::Mcp(args) => {
                mcp::cmd_mcp(args)?;
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
                        trace.as_deref(),
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
                        trace.as_deref(),
                        chunks.as_deref(),
                        llm_plugin.as_deref(),
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
                    let text = crate::security::read_utf8_file_bounded(
                        &proposals,
                        crate::security::MAX_TEXT_INPUT_BYTES,
                        "CLI input",
                    )?;
                    let file: axiograph_ingest_docs::ProposalsFileV1 =
                        crate::security::parse_json_bounded(
                            text.as_bytes(),
                            crate::security::MAX_JSON_INPUT_BYTES,
                            "CLI JSON input",
                        )?;
                    axiograph_ingest_docs::validate_proposals_file_v1(&file)?;

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
                                    anyhow!(
                                        "missing `--llm-model` (example: --llm-model gpt-4o-mini)"
                                    )
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

                    crate::security::write_output_bounded(&out, draft, "CLI output")?;
                    println!("wrote {}", out.display());
                }
                DiscoverCommands::MaskedTupleTrainingExport {
                    input,
                    out,
                    instance,
                    max_items,
                    mask_fields,
                    seed,
                } => {
                    cmd_discover_training_export(
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
                DiscoverCommands::CheckOlog(args) => {
                    cmd_discover_check_olog(&args)?;
                }
                DiscoverCommands::ContextReport(args) => {
                    cmd_discover_context_report(&args)?;
                }
                DiscoverCommands::OverlayCheck(args) => {
                    cmd_discover_overlay_check(&args)?;
                }
                DiscoverCommands::CoverageQuery(args) => {
                    cmd_discover_coverage_query(&args)?;
                }
                DiscoverCommands::Define(args) => {
                    cmd_discover_define(&args)?;
                }
                DiscoverCommands::EmbeddingRelationships(args) => {
                    cmd_discover_embedding_relationships(&args)?;
                }
                DiscoverCommands::BehaviorCase(args) => {
                    cmd_discover_behavior_case(&args)?;
                }
                DiscoverCommands::RoutePreview(args) => {
                    cmd_discover_route_preview(&args)?;
                }
                DiscoverCommands::TransportPreview(args) => {
                    cmd_discover_transport_preview(&args)?;
                }
                DiscoverCommands::TheoryGraph(args) => {
                    cmd_discover_theory_graph(&args)?;
                }
                DiscoverCommands::KernelSurface(args) => {
                    cmd_discover_kernel_surface(&args)?;
                }
                DiscoverCommands::TheoryCheck(args) => {
                    cmd_discover_theory_check(&args)?;
                }
                DiscoverCommands::PredictiveProposalPropose(args) => {
                    cmd_predictive_proposals(&args)?;
                }
            },
            Commands::Repl {
                script,
                cmd,
                continue_on_error,
                quiet,
            } => {
                if script.is_some() || !cmd.is_empty() {
                    repl::cmd_repl_script(script.as_ref(), &cmd, continue_on_error, quiet)?;
                } else {
                    repl::cmd_repl()?;
                }
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

// =============================================================================
// Accepted plane / snapshot store CLI helpers
// =============================================================================

fn cmd_typecheck_cert(input: &Path, out: Option<&Path>) -> Result<()> {
    let axi_text = crate::security::read_utf8_file_bounded(
        input,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let typed = crate::axi_input::require_canonical_axi_text(&axi_text)?;
    let digest = typed.digest().clone();
    let (_m, proof) = typed.module().clone().into_parts();

    let cert = axiograph_pathdb::certificate::CertificateV2::axi_well_typed_v1(proof)
        .with_anchor(axiograph_pathdb::certificate::AxiAnchorV1::new(digest));

    let json = serde_json::to_string_pretty(&cert)?;
    match out {
        Some(path) => {
            crate::security::write_output_bounded(path, json, "CLI output")?;
            println!("wrote {}", path.display());
        }
        None => {
            println!("{json}");
        }
    }

    Ok(())
}

fn cmd_constraints_cert(input: &Path, out: Option<&Path>) -> Result<()> {
    let axi_text = crate::security::read_utf8_file_bounded(
        input,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let typed = crate::axi_input::require_canonical_axi_text(&axi_text)?;
    let digest = typed.digest().clone();
    let proof =
        axiograph_pathdb::axi_module_constraints::check_axi_constraints_ok_v1(typed.module())?;

    let cert = axiograph_pathdb::certificate::CertificateV2::axi_constraints_ok_v1(proof)
        .with_anchor(axiograph_pathdb::certificate::AxiAnchorV1::new(digest));

    let json = serde_json::to_string_pretty(&cert)?;
    match out {
        Some(path) => {
            crate::security::write_output_bounded(path, json, "CLI output")?;
            println!("wrote {}", path.display());
        }
        None => {
            println!("{json}");
        }
    }

    Ok(())
}

fn cmd_publish_materialization(dir: &Path, spec_path: &Path) -> Result<()> {
    let bytes = crate::security::read_file_bounded(
        spec_path,
        crate::security::MAX_BINARY_INPUT_BYTES,
        "materialization build spec",
    )?;
    let spec: axiograph_store::AxpdBuildSpec = crate::security::parse_json_bounded(
        &bytes,
        crate::security::MAX_JSON_INPUT_BYTES,
        "CLI JSON input",
    )
    .with_context(|| format!("parse AxpdBuildSpec `{}`", spec_path.display()))?;
    let store = axiograph_store::AxiStore::open(dir).context("open AxiStore")?;
    let receipt = store
        .publish_axpd(spec, &axiograph_store::AxpdLimits::default())
        .context("publish authenticated SQLite materialization")?;
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    Ok(())
}

fn cmd_show_materialization(dir: &PathBuf, materialization: &str) -> Result<()> {
    let materialization_id: axiograph_kernel::MaterializationIdV2 = materialization
        .parse()
        .map_err(|error| anyhow!("invalid materialization id `{materialization}`: {error}"))?;
    let store = axiograph_store::AxiStore::open(dir).context("open AxiStore")?;
    let verified = store
        .open_axpd(&materialization_id, &axiograph_store::AxpdLimits::default())
        .context("verify authenticated SQLite materialization")?;
    println!("{}", serde_json::to_string_pretty(verified.receipt())?);
    Ok(())
}

pub(crate) fn load_pathdb_for_cli(input: &Path) -> Result<axiograph_pathdb::PathDB> {
    let ext = input.extension().and_then(|s| s.to_str()).unwrap_or("");
    if ext.eq_ignore_ascii_case("axi") {
        let text = crate::security::read_utf8_file_bounded(
            input,
            crate::security::MAX_TEXT_INPUT_BYTES,
            "CLI input",
        )?;
        let module = crate::axi_input::require_canonical_axi_text(&text).map_err(|err| {
            anyhow!(
                "{err}; semantic inspection and certified-query commands accept exact reviewable `.axi` modules"
            )
        })?;
        let mut db = axiograph_pathdb::PathDB::new();
        let _summary = module.import_into_pathdb(&mut db)?;
        db.build_indexes();
        return Ok(db);
    }
    Err(anyhow!(
        "unsupported input `{}` (expected exact canonical .axi)",
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
        args.all,
        args.hops,
        args.max_nodes,
        args.max_edges,
        &args.direction,
        args.include_meta,
        args.typed_overlay,
        !args.no_equivalences,
    )
}

#[allow(clippy::too_many_arguments)]
fn cmd_viz(
    input: &Path,
    out: &Path,
    format: &str,
    plane: &str,
    focus_id: &[u32],
    focus_name: Option<&str>,
    focus_type: Option<&str>,
    all: bool,
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
    if !all {
        if focus.is_empty() {
            if let Some(name) = focus_name {
                if let Some(id) = crate::viz::resolve_focus_by_name_and_type(&db, name, focus_type)?
                {
                    focus.push(id);
                } else {
                    return Err(anyhow!(
                        "no entity found with name `{name}` (tip: try `axiograph repl` + `find_by_type`/`q` to locate ids)"
                    ));
                }
            }
        }
        if focus.is_empty() {
            eprintln!("note: no focus specified; using a fallback node");
        }
    }

    let options = crate::viz::VizOptions {
        focus_ids: focus,
        all_nodes: all,
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

    if matches!(format, crate::viz::VizFormat::Html) {
        let json = crate::viz::render_json(&g)?;
        let out_dir = crate::viz::write_html_bundle(out, &rendered, Some(&json))?;
        crate::security::write_output_bounded(out_dir.join("graph.json"), json, "CLI output")?;
        println!(
            "wrote {} (nodes={} edges={} truncated={})",
            out_dir.display(),
            g.nodes.len(),
            g.edges.len(),
            g.truncated
        );
    } else {
        crate::security::write_output_bounded(out, rendered, "CLI output")?;
        println!(
            "wrote {} (nodes={} edges={} truncated={})",
            out.display(),
            g.nodes.len(),
            g.edges.len(),
            g.truncated
        );
    }
    Ok(())
}

fn write_json_output<T: Serialize>(value: &T, out: Option<&PathBuf>) -> Result<()> {
    let json = serde_json::to_string_pretty(value)?;
    if let Some(path) = out {
        crate::security::write_output_bounded(path, json, "CLI output")?;
        println!("wrote {}", path.display());
    } else {
        println!("{json}");
    }
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
                name: format!("{ty_name}.{field_name}"),
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
                        evidence: evidence_for_field(
                            chunks,
                            evidence_locator.as_ref(),
                            &field_name,
                        ),
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

fn cmd_sql(input: &Path, out: &Path, chunks_path: Option<&Path>) -> Result<()> {
    println!(
        "{} SQL schema {}",
        "Ingesting".green().bold(),
        input.display()
    );

    let text = crate::security::read_utf8_file_bounded(
        input,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let sql_schema = axiograph_ingest_sql::parse_sql_ddl(&text)?;
    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();

    // Also emit DocChunks for RAG grounding (default: alongside the proposals output).
    let chunks_out = chunks_path.map(Path::to_path_buf).unwrap_or_else(|| {
        out.parent()
            .unwrap_or(std::path::Path::new("."))
            .join("chunks.json")
    });
    fs::create_dir_all(chunks_out.parent().unwrap_or(std::path::Path::new(".")))?;

    let locator = input.to_string_lossy().to_string();
    let doc_digest = axiograph_kernel::object_blob_digest_v2(locator.as_bytes());
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
    crate::security::write_output_bounded(
        &chunks_out,
        axiograph_ingest_docs::chunks_to_json_for_chunks(
            "sql_ingest",
            locator.clone(),
            chunks.clone(),
        )?,
        "CLI output",
    )?;
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
    crate::security::write_output_bounded(out, &json, "CLI output")?;
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
    input: &Path,
    out: &Path,
    chunks_path: Option<&Path>,
    facts_path: Option<&Path>,
    machining: bool,
    domain: &str,
) -> Result<()> {
    println!(
        "{} document {}",
        "Ingesting".green().bold(),
        input.display()
    );

    let text = crate::security::read_utf8_file_bounded(
        input,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();

    let domain = if machining { "machining" } else { domain };

    // Full knowledge extraction with probabilistic facts
    let result = axiograph_ingest_docs::extract_knowledge_full(&text, &stem, domain)?;
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
    crate::security::write_output_bounded(out, &json, "CLI output")?;
    println!("  {} {}", "→".cyan(), out.display());
    println!("  {} {} facts extracted", "→".yellow(), result.facts.len());

    let chunks_out = chunks_path.map(Path::to_path_buf).unwrap_or_else(|| {
        out.parent()
            .unwrap_or(std::path::Path::new("."))
            .join("chunks.json")
    });
    fs::create_dir_all(chunks_out.parent().unwrap_or(std::path::Path::new(".")))?;
    let chunks_json = axiograph_ingest_docs::chunks_to_json(&result.extraction)?;
    crate::security::write_output_bounded(&chunks_out, &chunks_json, "CLI output")?;
    println!("  {} {}", "→".cyan(), chunks_out.display());

    if let Some(facts_out) = facts_path {
        let facts_json = serde_json::to_string_pretty(&result.facts)?;
        crate::security::write_output_bounded(facts_out, &facts_json, "CLI output")?;
        println!("  {} {}", "→".cyan(), facts_out.display());
    }

    Ok(())
}

fn cmd_conversation(
    input: &Path,
    out: &Path,
    chunks_path: Option<&Path>,
    facts_path: Option<&Path>,
    format: &str,
) -> Result<()> {
    println!(
        "{} conversation {}",
        "Ingesting".green().bold(),
        input.display()
    );

    let text = crate::security::read_utf8_file_bounded(
        input,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();

    let result = axiograph_ingest_docs::extract_knowledge_from_conversation(&text, &stem, format)?;
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
    crate::security::write_output_bounded(out, &json, "CLI output")?;
    println!("  {} {}", "→".cyan(), out.display());
    println!(
        "  {} {} turns, {} facts",
        "→".yellow(),
        result.extraction.chunks.len(),
        result.facts.len()
    );

    let chunks_out = chunks_path.map(Path::to_path_buf).unwrap_or_else(|| {
        out.parent()
            .unwrap_or(std::path::Path::new("."))
            .join("chunks.json")
    });
    fs::create_dir_all(chunks_out.parent().unwrap_or(std::path::Path::new(".")))?;
    let chunks_json = axiograph_ingest_docs::chunks_to_json(&result.extraction)?;
    crate::security::write_output_bounded(&chunks_out, &chunks_json, "CLI output")?;
    println!("  {} {}", "→".cyan(), chunks_out.display());

    if let Some(facts_out) = facts_path {
        let facts_json = serde_json::to_string_pretty(&result.facts)?;
        crate::security::write_output_bounded(facts_out, &facts_json, "CLI output")?;
        println!("  {} {}", "→".cyan(), facts_out.display());
    }

    Ok(())
}

fn cmd_confluence(
    input: &Path,
    out: &Path,
    space: &str,
    chunks_path: Option<&Path>,
    facts_path: Option<&Path>,
) -> Result<()> {
    println!(
        "{} Confluence page {}",
        "Ingesting".green().bold(),
        input.display()
    );

    let html = crate::security::read_utf8_file_bounded(
        input,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
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
    crate::security::write_output_bounded(out, &json, "CLI output")?;
    println!("  {} {}", "→".cyan(), out.display());
    println!(
        "  {} {} sections, {} facts",
        "→".yellow(),
        result.extraction.chunks.len(),
        result.facts.len()
    );

    let chunks_out = chunks_path.map(Path::to_path_buf).unwrap_or_else(|| {
        out.parent()
            .unwrap_or(std::path::Path::new("."))
            .join("chunks.json")
    });
    fs::create_dir_all(chunks_out.parent().unwrap_or(std::path::Path::new(".")))?;
    let chunks_json = axiograph_ingest_docs::chunks_to_json(&result.extraction)?;
    crate::security::write_output_bounded(&chunks_out, &chunks_json, "CLI output")?;
    println!("  {} {}", "→".cyan(), chunks_out.display());

    if let Some(facts_out) = facts_path {
        let facts_json = serde_json::to_string_pretty(&result.facts)?;
        crate::security::write_output_bounded(facts_out, &facts_json, "CLI output")?;
        println!("  {} {}", "→".cyan(), facts_out.display());
    }

    Ok(())
}

fn cmd_json(input: &Path, out: &Path, chunks_path: Option<&Path>) -> Result<()> {
    println!("{} JSON {}", "Ingesting".green().bold(), input.display());

    let text = crate::security::read_utf8_file_bounded(
        input,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let value: serde_json::Value = crate::security::parse_json_bounded(
        text.as_bytes(),
        crate::security::MAX_JSON_INPUT_BYTES,
        "CLI JSON input",
    )?;
    let schema = axiograph_ingest_json::infer_schema(&value, "Root");
    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();

    // Also emit DocChunks for RAG grounding (default: alongside the proposals output).
    let chunks_out = chunks_path.map(Path::to_path_buf).unwrap_or_else(|| {
        out.parent()
            .unwrap_or(std::path::Path::new("."))
            .join("chunks.json")
    });
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
    let doc_digest = axiograph_kernel::object_blob_digest_v2(locator.as_bytes());
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
    crate::security::write_output_bounded(
        &chunks_out,
        axiograph_ingest_docs::chunks_to_json_for_chunks(
            "json_ingest",
            locator.clone(),
            chunks.clone(),
        )?,
        "CLI output",
    )?;
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
    crate::security::write_output_bounded(out, &json, "CLI output")?;
    println!("  {} {}", "→".cyan(), out.display());

    Ok(())
}

fn cmd_readings(input: &Path, out: &Path, chunks_path: Option<&Path>, format: &str) -> Result<()> {
    println!(
        "{} readings {}",
        "Ingesting".green().bold(),
        input.display()
    );

    let text = crate::security::read_utf8_file_bounded(
        input,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();

    let readings = match format {
        "bibtex" => axiograph_ingest_docs::parse_bibtex(&text),
        "markdown" => axiograph_ingest_docs::parse_reading_list(&text)
            .into_iter()
            .map(|r| r.bib)
            .collect(),
        other => {
            return Err(anyhow!(
                "unsupported reading format `{other}` (expected bibtex|markdown)"
            ))
        }
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

    let chunks_out = chunks_path.map(Path::to_path_buf).unwrap_or_else(|| {
        out.parent()
            .unwrap_or(std::path::Path::new("."))
            .join("chunks.json")
    });
    fs::create_dir_all(chunks_out.parent().unwrap_or(std::path::Path::new(".")))?;
    let chunks_json = axiograph_ingest_docs::chunks_to_json(&extraction)?;
    crate::security::write_output_bounded(&chunks_out, &chunks_json, "CLI output")?;
    println!("  {} {}", "→".cyan(), chunks_out.display());

    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    // Readings ingestion currently produces chunks; treat each reading as a claim-like entity.
    let mut proposals = Vec::new();
    for (idx, r) in readings.iter().enumerate() {
        let id = format!("reading::{idx}");
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
    crate::security::write_output_bounded(out, &json, "CLI output")?;
    println!("  {} {}", "→".cyan(), out.display());

    Ok(())
}

fn cmd_validate(input: &Path) -> Result<()> {
    println!("{} {}", "Validating".green().bold(), input.display());

    let repository_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let package = crate::axi_input::compile_canonical_axi_path(input, &[repository_root])?;
    let root_source = package.root_source();
    let m = root_source.parsed();
    let snapshot = package.snapshot();

    println!("  Dialect: {}", "axi_v1 (schema/theory/instance)".cyan());
    println!("  Module: {}", m.module_name.cyan());
    println!(
        "  Import closure: {} module(s)",
        snapshot.ir().ordered_module_closure().len()
    );
    println!("  Schemas: {}", snapshot.ir().schemas().len());
    println!("  Theories: {}", snapshot.ir().theories().len());
    println!("  Instances: {}", snapshot.ir().instances().len());

    for schema in &m.schemas {
        println!(
            "    Schema {}: {} objects, {} relations",
            schema.name.yellow(),
            schema.objects.len(),
            schema.relations.len()
        );
    }

    if !snapshot.ir().theories().is_empty() {
        let theory_report = crate::runtime_theory_check::runtime_theory_check_reports_from_package(
            &package,
            None,
            axiograph_pathdb::RuntimeTheoryClosureTierV1::FiniteFragment,
            axiograph_pathdb::default_world_assumption_v1(),
            axiograph_pathdb::default_evidence_policy_v1(),
        )?;
        if theory_report.blocking_errors > 0 {
            return Err(anyhow!(
                "runtime theory check found {} blocking error(s)",
                theory_report.blocking_errors
            ));
        }
        println!(
            "  Runtime theory: {} report(s), {} blocking error(s)",
            theory_report.reports.len(),
            theory_report.blocking_errors
        );
    }

    println!("{}", "Valid.".green());
    Ok(())
}

fn cmd_check_theory(args: &CheckTheoryArgs) -> Result<()> {
    let repository_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let package = crate::axi_input::compile_canonical_axi_path(&args.input, &[repository_root])?;
    let closure_tier =
        crate::runtime_theory_check::parse_runtime_theory_closure_tier(&args.closure_tier)?;
    let (world, evidence_policy) = runtime_theory_cli_assumptions(
        args.world_id.as_deref(),
        args.finite_world,
        &args.included_refs,
        &args.included_worlds,
        &args.included_slices,
        &args.included_imports,
        &args.undeclared_imports,
        args.evidence_threshold_ppm,
        &args.evidence_semantics,
        args.weighted_evidence,
        &args.evidence_weights,
    )?;
    let report = crate::runtime_theory_check::runtime_theory_check_reports_from_package(
        &package,
        args.theory.as_deref(),
        closure_tier,
        world,
        evidence_policy,
    )?;

    if args.json || args.out.is_some() {
        write_json_output(&report, args.out.as_ref())?;
    } else {
        println!(
            "{}",
            crate::runtime_theory_check::runtime_theory_check_human_summary(&report)
        );
    }

    if report.blocking_errors > 0 {
        return Err(anyhow!(
            "runtime theory check found {} blocking error(s)",
            report.blocking_errors
        ));
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct FiniteQueryVerificationScopeV1 {
    revision_digest_v2: axiograph_kernel::RevisionDigestV2,
    accepted_snapshot_id: axiograph_kernel::SnapshotIdV2,
    kernel_ir_digest: axiograph_kernel::ObjectBlobIdV2,
    prepared_query_digest_v1: axiograph_kernel::QueryIdV2,
    answer_digest_v1: axiograph_kernel::AnswerIdV2,
    certificate_digest_v2: axiograph_kernel::CertificateIdV2,
    claim_kind: String,
}

#[derive(Debug, Serialize)]
struct FiniteQueryVerificationCoverageV1 {
    selected_row_count: usize,
    row_witness_count: usize,
    runtime_truncated: bool,
    query_shape_certifiable: bool,
    certificate_emitted: bool,
    accepted_receipt_bound_to_exact_answer: bool,
}

#[derive(Debug, Serialize)]
struct FiniteQueryVerificationReportV1 {
    version: String,
    decision: String,
    scope: FiniteQueryVerificationScopeV1,
    coverage: FiniteQueryVerificationCoverageV1,
    finite_theory_gate: axiograph_kernel::FiniteTheoryGateReceiptIr,
    prepared_query: crate::query_ir::PreparedQueryMetadataV2,
    certificate: axiograph_pathdb::CertificateV3,
    certificate_text: String,
    verifier_receipt_v2: crate::verifier_bridge::VerifierReceiptV2,
    verified_rows: Vec<axiograph_pathdb::certificate::StableSelectedRowV1>,
    residual_obligations: Vec<String>,
    non_claims: Vec<String>,
}

fn cmd_check_finite_query(args: &CheckFiniteQueryArgs) -> Result<()> {
    let repository_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let package = crate::axi_input::compile_canonical_axi_path(
        &args.input,
        std::slice::from_ref(&repository_root),
    )?;
    let exact_axi = package.root_source().exact_text().to_string();
    let revision_digest_v2 = axiograph_kernel::RevisionDigestV2::from_accepted_text(&exact_axi);
    let sources = package.ordered_sources();
    let kernel = axiograph_pathdb::derive_runtime_package_index(package.snapshot(), &sources)
        .map_err(|error| anyhow!("derive runtime package index: {error}"))?;
    let finite_theory_gate = package
        .snapshot()
        .require_finite_theory_gate(axiograph_kernel::FiniteTheoryGateConsumerIr::Query)?;

    let mut db = axiograph_pathdb::PathDB::new();
    for source in &sources {
        let module = crate::axi_input::require_canonical_axi_text(source.exact_text())?;
        module.import_into_pathdb(&mut db)?;
    }
    db.build_indexes();
    let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db).ok();
    let query_bytes = crate::security::read_file_bounded(
        &args.query,
        crate::security::MAX_JSON_INPUT_BYTES,
        "finite query JSON",
    )?;
    let query_ir: crate::query_ir::QueryIrV1 = crate::security::parse_json_bounded(
        &query_bytes,
        crate::security::MAX_JSON_INPUT_BYTES,
        "finite query JSON",
    )?;
    let mut prepared = query_ir.compile_with_meta(&db, meta.as_ref())?;
    if !prepared.certifiability().is_certifiable() {
        return Err(anyhow!(
            "finite-query verification requires a fully certifiable query shape"
        ));
    }
    let prepared_query = prepared.metadata_v2_with_meta_and_kernel(meta.as_ref(), &kernel)?;
    let validated = prepared.execute_answer(&db, meta.as_ref())?;
    let emitted = prepared.certify_answer_with_anchors(
        validated,
        &db,
        meta.as_ref(),
        revision_digest_v2.clone(),
    )?;
    let prepared_digest = emitted
        .prepared_query_digest_v1()
        .cloned()
        .ok_or_else(|| anyhow!("certifiable query omitted its prepared-query digest"))?;
    let answer_digest = emitted.answer_digest_v1().clone();
    let certificate_digest = emitted.certificate_digest_v2().clone();
    let certificate = emitted.certificate().clone();
    let certificate_text = emitted.certificate_text().to_string();
    let row_witness_count = certificate
        .proof
        .rows
        .iter()
        .map(|row| row.witnesses.len())
        .sum();
    let verifier_receipt_v2 = crate::verifier_bridge::verify_certificate_with_lean(
        &crate::verifier_bridge::CertVerifyConfig {
            verifier_bin: Some(args.verify_bin.clone()),
            timeout: Some(Duration::from_secs(args.verify_timeout_secs)),
            approved_checker_sha256: Some(args.verify_sha256.clone()),
            approved_checker_build_id: Some(args.verify_build_id.clone()),
        },
        &exact_axi,
        emitted.certificate_text(),
        &prepared_digest,
        &answer_digest,
    )?;
    if !verifier_receipt_v2.accepted() {
        return Err(anyhow!(
            "trusted finite-query checker rejected the exact answer"
        ));
    }
    let verified = emitted.into_lean_verified(verifier_receipt_v2.clone())?;
    let verified_rows = verified.selected_rows_v1().to_vec();
    let report = FiniteQueryVerificationReportV1 {
        version: "finite_query_verification_report_v1".to_string(),
        decision: "accepted".to_string(),
        scope: FiniteQueryVerificationScopeV1 {
            revision_digest_v2,
            accepted_snapshot_id: package.snapshot().ir().accepted_snapshot_id().clone(),
            kernel_ir_digest: package.snapshot().ir().ir_digest().clone(),
            prepared_query_digest_v1: prepared_digest,
            answer_digest_v1: answer_digest,
            certificate_digest_v2: certificate_digest,
            claim_kind: "finite_exact_complete".to_string(),
        },
        coverage: FiniteQueryVerificationCoverageV1 {
            selected_row_count: verified_rows.len(),
            row_witness_count,
            runtime_truncated: verified.runtime_truncated(),
            query_shape_certifiable: true,
            certificate_emitted: true,
            accepted_receipt_bound_to_exact_answer: true,
        },
        finite_theory_gate,
        prepared_query,
        certificate,
        certificate_text,
        verifier_receipt_v2,
        verified_rows,
        residual_obligations: Vec::new(),
        non_claims: vec![
            "exact completeness is limited to the declared bounded finite query denotation"
                .to_string(),
            "the query receipt does not establish ontology closure, evidence exhaustiveness, or backend completeness"
                .to_string(),
            "the attached finite-theory gate is Rust replay evidence; only the query_result_v4 receipt is accepted by VerifyMain"
                .to_string(),
            "non-identity dependent transport is outside this query certificate".to_string(),
        ],
    };
    write_json_output(&report, args.out.as_ref())
}

fn cmd_check_software_coverage(args: &CheckSoftwareCoverageArgs) -> Result<()> {
    let db = load_pathdb_for_cli(&args.input)?;
    let overlay = load_tooling_overlay(&args.overlay)?;
    let mut request = load_behavior_case_request(&args.behavior_case)?;
    attach_behavior_case_cq_files(&mut request, &args.cq_files)?;
    request.overlay = Some(overlay.clone());
    request.codegen = behavior_codegen_request_from_overlay(&overlay)?;
    let behavior_report =
        crate::behavior_case::build_behavior_case_report_from_request(&db, None, None, request)?;
    let behavior_report_json = serde_json::to_value(&behavior_report)?;
    let behavior_report_view =
        axiograph_tooling_overlays::behavior_case_coverage_view_from_value(&behavior_report_json)?;
    let report = axiograph_tooling_overlays::continuous_coverage_report_from_behavior_report(
        &behavior_report_view,
        &overlay,
        &args.repo_root,
    );
    write_json_output(&report, args.out.as_ref())?;
    if !report.pass {
        return Err(anyhow!("software coverage check failed"));
    }
    Ok(())
}

fn cmd_authoring(command: AuthoringCommands) -> Result<()> {
    match command {
        AuthoringCommands::Run {
            suite,
            example,
            profile,
            repo_root,
            out_dir,
            out,
            fail_on_blocking,
        } => {
            let report =
                cmd_authoring_run_suite(&suite, &example, &profile, &repo_root, out_dir.as_ref())?;
            let pass = report
                .get("pass")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            write_json_output(&report, out.as_ref())?;
            if fail_on_blocking && !pass {
                return Err(anyhow!(
                    "authoring flow `{example}` failed under profile `{profile}`"
                ));
            }
            Ok(())
        }
        AuthoringCommands::CodegenPlan { overlay, out } => {
            let overlay = load_tooling_overlay(&overlay)?;
            let report = axiograph_tooling_overlays::codegen_plan_report(&overlay);
            write_json_output(&report, out.as_ref())
        }
        AuthoringCommands::Workspace {
            workspace,
            request,
            out,
        } => {
            let service = crate::authoring_workspace::AuthoringWorkspaceService::new(&workspace)?;
            let request = crate::authoring_workspace::read_authoring_request(&request)?;
            let report = service.execute(request)?;
            write_json_output(&report, out.as_ref())
        }
        AuthoringCommands::MaterializeSkeletons {
            behavior_report,
            out_dir,
            language,
            overwrite,
            out,
        } => {
            let report_json = read_json_file(&behavior_report)?;
            let report = axiograph_software_authoring::materialize_skeletons_from_report(
                &report_json,
                &behavior_report,
                &axiograph_software_authoring::MaterializeSkeletonsOptions {
                    out_dir,
                    language,
                    overwrite,
                },
            )?;
            write_json_output(&report, out.as_ref())
        }
        AuthoringCommands::ContinuousCheck {
            behavior_report,
            repo_root,
            require_codegen,
            strict_coverage,
            require_code_refs,
            require_runtime_theory,
            out,
        } => {
            let report_json = read_json_file(&behavior_report)?;
            let report = axiograph_software_authoring::build_continuous_software_coverage_report(
                &report_json,
                &axiograph_software_authoring::ContinuousCheckOptions {
                    repo_root,
                    require_codegen,
                    strict_coverage,
                    require_code_refs,
                    require_runtime_theory,
                },
            )?;
            write_json_output(&report, out.as_ref())?;
            if !report.pass {
                return Err(anyhow!("continuous software coverage check failed"));
            }
            Ok(())
        }
        AuthoringCommands::ToolSpecs { out } => {
            let specs = axiograph_software_authoring::software_authoring_tool_specs_v1();
            write_json_output(&specs, out.as_ref())
        }
        AuthoringCommands::LspCapabilities { out } => {
            let capabilities = crate::authoring_workspace::authoring_workspace_capabilities_v1();
            write_json_output(&capabilities, out.as_ref())
        }
        AuthoringCommands::IntegrationManifest { workspace, out } => {
            let manifest =
                crate::authoring_workspace::authoring_workspace_integration_manifest_v1(&workspace);
            write_json_output(&manifest, out.as_ref())
        }
        AuthoringCommands::Lsp { workspace, axi } => {
            let service = crate::authoring_workspace::AuthoringWorkspaceService::new(&workspace)?;
            crate::authoring_workspace::run_lsp_stdio(service, axi)
        }
        AuthoringCommands::Mcp { workspace } => {
            let service = crate::authoring_workspace::AuthoringWorkspaceService::new(&workspace)?;
            crate::authoring_workspace::run_mcp_stdio(service)
        }
        AuthoringCommands::Serve { workspace, listen } => {
            let service = crate::authoring_workspace::AuthoringWorkspaceService::new(&workspace)?;
            crate::authoring_workspace::run_http(service, listen)
        }
    }
}

#[derive(Debug, Deserialize)]
struct SoftwareAuthoringExampleSuiteV1 {
    #[serde(default)]
    version: Option<String>,
    examples: Vec<SoftwareAuthoringExampleCatalogEntryV1>,
}

#[derive(Debug, Deserialize)]
struct SoftwareAuthoringExampleCatalogEntryV1 {
    id: String,
    title: Option<String>,
    axi: PathBuf,
    overlay: PathBuf,
    behavior_case: PathBuf,
    cq_file: Option<PathBuf>,
    #[serde(default)]
    coverage_terms: Vec<String>,
    #[serde(default)]
    coverage_relations: Vec<String>,
    #[serde(default)]
    coverage_cqs: Vec<String>,
    #[serde(default)]
    coverage_code_refs: Vec<String>,
    #[serde(default)]
    coverage_surface_hints: Vec<String>,
    #[serde(default)]
    coverage_max_matches: Option<usize>,
    #[serde(default)]
    definition_query_prompts: Vec<SoftwareAuthoringDefinitionQueryPromptV1>,
}

#[derive(Debug, Deserialize)]
struct SoftwareAuthoringDefinitionQueryPromptV1 {
    id: Option<String>,
    prompt: String,
    kind_hint: Option<String>,
    context_hint: Option<String>,
    #[serde(default)]
    include_queries: bool,
    max_matches: Option<usize>,
}

fn cmd_authoring_run_suite(
    suite_path: &Path,
    example_id: &str,
    profile: &str,
    repo_root: &Path,
    out_dir: Option<&PathBuf>,
) -> Result<Value> {
    let suite_text = crate::security::read_utf8_file_bounded(
        suite_path,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let suite: SoftwareAuthoringExampleSuiteV1 = crate::security::parse_json_bounded(
        suite_text.as_bytes(),
        crate::security::MAX_JSON_INPUT_BYTES,
        "CLI JSON input",
    )
    .map_err(|err| anyhow!("failed to parse software authoring suite JSON: {err}"))?;
    let entry = suite
        .examples
        .iter()
        .find(|entry| entry.id == example_id)
        .ok_or_else(|| anyhow!("software authoring suite has no example `{example_id}`"))?;

    let profile = parse_authoring_run_profile(profile)?;
    let axi_path = resolve_suite_relative_path(suite_path, &entry.axi)?;
    let overlay_path = resolve_suite_relative_path(suite_path, &entry.overlay)?;
    let behavior_case_path = resolve_suite_relative_path(suite_path, &entry.behavior_case)?;
    let cq_file_path = entry
        .cq_file
        .as_ref()
        .map(|path| resolve_suite_relative_path(suite_path, path))
        .transpose()?;
    if let Some(out_dir) = out_dir {
        fs::create_dir_all(out_dir)?;
    }

    let kernel = compile_kernel_for_tooling_overlay(&axi_path)?;
    let overlay = load_tooling_overlay(&overlay_path)?;
    let overlay_report = axiograph_tooling_overlays::validate_overlay_bundle(&kernel, &overlay);
    write_optional_step_report(out_dir, "overlay_validation.json", &overlay_report)?;

    let db = load_pathdb_for_cli(&axi_path)?;
    let mut request = load_behavior_case_request(&behavior_case_path)?;
    if let Some(cq_file_path) = cq_file_path.as_ref() {
        attach_behavior_case_cq_files(&mut request, std::slice::from_ref(cq_file_path))?;
    }
    request.codegen = behavior_codegen_request_from_overlay(&overlay)?;
    request.overlay = Some(overlay.clone());
    let behavior_report =
        crate::behavior_case::build_behavior_case_report_from_request(&db, None, None, request)?;
    let behavior_report_json = serde_json::to_value(&behavior_report)?;
    write_optional_step_report(out_dir, "behavior_case_report.json", &behavior_report_json)?;
    let behavior_report_view =
        axiograph_tooling_overlays::behavior_case_coverage_view_from_value(&behavior_report_json)?;

    let overlay_coverage =
        axiograph_tooling_overlays::continuous_coverage_report_from_behavior_report_with_validation(
            &behavior_report_view,
            &overlay,
            repo_root,
            &overlay_report,
        );
    write_optional_step_report(out_dir, "overlay_coverage.json", &overlay_coverage)?;

    let continuous_options = axiograph_software_authoring::ContinuousCheckOptions {
        repo_root: repo_root.to_path_buf(),
        require_codegen: vec![
            "rust".to_string(),
            "typescript".to_string(),
            "python".to_string(),
            "go".to_string(),
        ],
        strict_coverage: profile.strict_coverage(),
        require_code_refs: profile.require_code_refs(),
        require_runtime_theory: profile.require_runtime_theory(),
    };
    let continuous_report =
        axiograph_software_authoring::build_continuous_software_coverage_report(
            &behavior_report_json,
            &continuous_options,
        )?;
    write_optional_step_report(out_dir, "continuous_coverage.json", &continuous_report)?;

    let coverage_query_report = if let Some(query) = coverage_query_from_catalog_entry(entry) {
        let report =
            axiograph_tooling_overlays::coverage_query_report(&kernel, Some(&overlay), &query)?;
        write_optional_step_report(out_dir, "coverage_query.json", &report)?;
        Some(serde_json::to_value(report)?)
    } else {
        None
    };

    let definition_query_reports =
        definition_query_reports_from_catalog_entry(entry, &kernel, Some(&overlay))?;
    if !definition_query_reports.is_empty() {
        write_optional_step_report(
            out_dir,
            "definition_queries.json",
            &definition_query_reports,
        )?;
    }
    let definition_query_report_count = definition_query_reports.len();

    let pass = overlay_report.valid && overlay_coverage.pass && continuous_report.pass;
    Ok(serde_json::json!({
        "version": "authoring_suite_run_report_v1",
        "suite_version": suite.version,
        "example": {
            "id": entry.id,
            "title": entry.title.as_deref(),
            "axi": axi_path.display().to_string(),
            "overlay": overlay_path.display().to_string(),
            "behavior_case": behavior_case_path.display().to_string(),
            "definition_query_prompts": entry.definition_query_prompts.iter().map(|query| {
                serde_json::json!({
                    "id": query.id,
                    "prompt": query.prompt,
                    "kind_hint": query.kind_hint,
                    "context_hint": query.context_hint,
                    "include_queries": query.include_queries,
                    "max_matches": query.max_matches,
                })
            }).collect::<Vec<_>>(),
        },
        "profile": profile.as_str(),
        "pass": pass,
        "overlay_validation": overlay_report,
        "behavior_case_report": behavior_report,
        "overlay_coverage": overlay_coverage,
        "continuous_coverage": continuous_report,
        "coverage_query_report": coverage_query_report,
        "definition_query_reports": definition_query_reports,
        "definition_query_count": definition_query_report_count,
        "next_commands": [
            format!("axiograph check validate {}", axi_path.display()),
            format!("axiograph check theory {} --closure-tier finite_fragment", axi_path.display()),
            format!("axiograph discover overlay-check {} --overlay {}", axi_path.display(), overlay_path.display()),
            format!("axiograph discover behavior-case {} --request {} --overlay {}", axi_path.display(), behavior_case_path.display(), overlay_path.display()),
            format!("axiograph authoring continuous-check --behavior-report <behavior_case_report.json> --repo-root {}", repo_root.display())
        ],
    }))
}

fn definition_query_reports_from_catalog_entry(
    entry: &SoftwareAuthoringExampleCatalogEntryV1,
    kernel: &axiograph_pathdb::kernel_ir::RuntimeModuleIndex,
    overlay: Option<&axiograph_tooling_overlays::ToolingOverlayBundleV1>,
) -> Result<Vec<Value>> {
    let mut reports = Vec::new();
    for prompt in &entry.definition_query_prompts {
        let query = axiograph_tooling_overlays::DefinitionQueryV1 {
            version: Some(axiograph_tooling_overlays::DEFINITION_QUERY_VERSION_V1.to_string()),
            prompt: prompt.prompt.clone(),
            kind_hint: parse_definition_kind_hint(prompt.kind_hint.as_deref())?,
            context_hint: prompt.context_hint.clone(),
            candidate_refs: Vec::new(),
            max_matches: prompt.max_matches,
            include_queries: prompt.include_queries,
        };
        let report = axiograph_tooling_overlays::definition_query_report(kernel, overlay, &query)?;
        reports.push(serde_json::json!({
            "id": prompt.id,
            "report": report,
        }));
    }
    Ok(reports)
}

fn coverage_query_from_catalog_entry(
    entry: &SoftwareAuthoringExampleCatalogEntryV1,
) -> Option<axiograph_tooling_overlays::CoverageQueryV1> {
    if entry.coverage_terms.is_empty()
        && entry.coverage_relations.is_empty()
        && entry.coverage_cqs.is_empty()
        && entry.coverage_code_refs.is_empty()
        && entry.coverage_surface_hints.is_empty()
    {
        return None;
    }
    Some(axiograph_tooling_overlays::CoverageQueryV1 {
        version: Some(axiograph_tooling_overlays::COVERAGE_QUERY_VERSION_V1.to_string()),
        coverage_mode: axiograph_tooling_overlays::CoverageModeV1::Exploratory,
        terms: entry.coverage_terms.clone(),
        relation_names: entry.coverage_relations.clone(),
        cq_names: entry.coverage_cqs.clone(),
        code_refs: entry.coverage_code_refs.clone(),
        surface_hints: entry.coverage_surface_hints.clone(),
        axql: None,
        max_matches: entry.coverage_max_matches,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthoringRunProfile {
    Advisory,
    Strict,
    Ci,
}

impl AuthoringRunProfile {
    fn as_str(self) -> &'static str {
        match self {
            Self::Advisory => "advisory",
            Self::Strict => "strict",
            Self::Ci => "ci",
        }
    }

    fn strict_coverage(self) -> bool {
        matches!(self, Self::Strict | Self::Ci)
    }

    fn require_code_refs(self) -> bool {
        matches!(self, Self::Ci)
    }

    fn require_runtime_theory(self) -> bool {
        matches!(self, Self::Ci)
    }
}

fn parse_authoring_run_profile(raw: &str) -> Result<AuthoringRunProfile> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "advisory" => Ok(AuthoringRunProfile::Advisory),
        "strict" => Ok(AuthoringRunProfile::Strict),
        "ci" => Ok(AuthoringRunProfile::Ci),
        other => Err(anyhow!(
            "unknown authoring profile `{other}` (expected advisory|strict|ci)"
        )),
    }
}

fn resolve_suite_relative_path(suite_path: &Path, raw: &Path) -> Result<PathBuf> {
    if raw.is_absolute() {
        return Ok(raw.to_path_buf());
    }
    if raw.exists() {
        return Ok(raw.to_path_buf());
    }
    let Some(mut base) = suite_path.parent() else {
        return Ok(raw.to_path_buf());
    };
    loop {
        let candidate = base.join(raw);
        if candidate.exists() {
            return Ok(candidate);
        }
        match base.parent() {
            Some(parent) => base = parent,
            None => break,
        }
    }
    Ok(raw.to_path_buf())
}

fn write_optional_step_report<T: Serialize>(
    out_dir: Option<&PathBuf>,
    filename: &str,
    report: &T,
) -> Result<()> {
    if let Some(out_dir) = out_dir {
        let path = out_dir.join(filename);
        write_json_output(report, Some(&path))?;
    }
    Ok(())
}

fn read_json_file(path: &Path) -> Result<Value> {
    let text = crate::security::read_utf8_file_bounded(
        path,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    crate::security::parse_json_bounded(
        text.as_bytes(),
        crate::security::MAX_JSON_INPUT_BYTES,
        "CLI JSON input",
    )
    .map_err(|err| anyhow!("failed to parse `{}` as JSON: {err}", path.display()))
}

#[allow(clippy::too_many_arguments)]
fn runtime_theory_cli_assumptions(
    world_id: Option<&str>,
    finite_world: bool,
    included_refs: &[String],
    included_worlds: &[String],
    included_slices: &[String],
    included_imports: &[String],
    undeclared_imports: &[String],
    evidence_threshold_ppm: Option<u32>,
    evidence_semantics: &str,
    weighted_evidence: bool,
    evidence_weights: &[String],
) -> Result<(
    axiograph_pathdb::WorldAssumptionV1,
    axiograph_pathdb::EvidencePolicyV1,
)> {
    let mut world = axiograph_pathdb::default_world_assumption_v1();
    if let Some(world_id) = world_id {
        world.world_id = world_id.to_string();
    }
    world.finite = finite_world;
    if !included_refs.is_empty() {
        world.included_refs = included_refs.to_vec();
    }
    if !included_worlds.is_empty() {
        world.included_worlds = included_worlds.to_vec();
    }
    if !included_slices.is_empty() {
        world.included_slices = included_slices.to_vec();
    }
    if !included_imports.is_empty() {
        world.included_imports = included_imports.to_vec();
    }
    if !undeclared_imports.is_empty() {
        world.undeclared_imports = undeclared_imports.to_vec();
    }

    let mut evidence_policy = axiograph_pathdb::default_evidence_policy_v1();
    if let Some(threshold) = evidence_threshold_ppm {
        if threshold > 1_000_000 {
            return Err(anyhow!(
                "invalid --evidence-threshold-ppm {threshold}: must be <= 1000000"
            ));
        }
        evidence_policy.threshold_ppm = threshold;
    }
    evidence_policy.semantics = if weighted_evidence {
        axiograph_pathdb::EvidenceWeightSemanticsV1::WeightedLattice
    } else {
        crate::runtime_theory_check::parse_evidence_weight_semantics(evidence_semantics)?
    };
    evidence_policy.weighted_propagation_enabled = weighted_evidence;
    for raw in evidence_weights {
        let (obligation_id, ppm) =
            crate::runtime_theory_check::parse_evidence_weight_assignment(raw)?;
        evidence_policy
            .obligation_weights_ppm
            .insert(obligation_id, ppm);
    }

    Ok((world, evidence_policy))
}

fn cmd_repo_index(
    root: &Path,
    out: &Path,
    chunks_path: Option<&Path>,
    edges_path: Option<&Path>,
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

    let chunks_out = chunks_path.map(Path::to_path_buf).unwrap_or_else(|| {
        out.parent()
            .unwrap_or(std::path::Path::new("."))
            .join("chunks.json")
    });
    fs::create_dir_all(chunks_out.parent().unwrap_or(std::path::Path::new(".")))?;
    let chunks_json = axiograph_ingest_docs::chunks_to_json(&result.extraction)?;
    crate::security::write_output_bounded(&chunks_out, &chunks_json, "CLI output")?;
    println!("  {} {}", "→".cyan(), chunks_out.display());

    if let Some(edges_out) = edges_path {
        let edges_json = serde_json::to_string_pretty(&result.edges)?;
        crate::security::write_output_bounded(edges_out, &edges_json, "CLI output")?;
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
    crate::security::write_output_bounded(out, &json, "CLI output")?;
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
    root: &Path,
    out: &Path,
    chunks_path: Option<&Path>,
    edges_path: Option<&Path>,
    trace_path: Option<&Path>,
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
    chunks_path: &Path,
    edges_path: &Path,
    out: &Path,
    max_proposals: usize,
) -> Result<()> {
    println!(
        "{} (chunks: {}, edges: {})",
        "Discovering links".green().bold(),
        chunks_path.display(),
        edges_path.display()
    );

    let chunks_text = crate::security::read_utf8_file_bounded(
        chunks_path,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let edges_text = crate::security::read_utf8_file_bounded(
        edges_path,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;

    let chunks = axiograph_ingest_docs::chunks_from_json_str(&chunks_text)?;
    let edges: Vec<axiograph_ingest_docs::RepoEdgeV1> = crate::security::parse_json_bounded(
        edges_text.as_bytes(),
        crate::security::MAX_JSON_INPUT_BYTES,
        "CLI JSON input",
    )?;

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
    crate::security::write_output_bounded(out, &trace_json, "CLI output")?;
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
    proposals_path: &Path,
    out_dir: &Path,
    trace_path: Option<&Path>,
    min_confidence: f64,
    domains: &str,
) -> Result<()> {
    println!(
        "{} {}",
        "Promoting proposals".green().bold(),
        proposals_path.display()
    );

    let text = crate::security::read_utf8_file_bounded(
        proposals_path,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let proposals: axiograph_ingest_docs::ProposalsFileV1 = crate::security::parse_json_bounded(
        text.as_bytes(),
        crate::security::MAX_JSON_INPUT_BYTES,
        "CLI JSON input",
    )?;
    axiograph_ingest_docs::validate_proposals_file_v1(&proposals)?;

    let domains = parse_promotion_domains(domains)?;
    let options = axiograph_ingest_docs::PromoteOptionsV1 {
        min_confidence,
        domains,
    };
    let result = axiograph_ingest_docs::promote_proposals_to_candidates_v1(&proposals, &options)?;

    fs::create_dir_all(out_dir)?;

    for (domain, axi) in &result.candidates {
        let out_path = out_dir.join(domain.default_output_file());
        crate::security::write_output_bounded(&out_path, axi, "CLI output")?;
        println!("  {} {}", "→".cyan(), out_path.display());
    }

    let trace_out = trace_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| out_dir.join("promotion_trace.json"));
    let json = serde_json::to_string_pretty(&result.trace)?;
    crate::security::write_output_bounded(&trace_out, json, "CLI output")?;
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
    program: &Path,
    args: &[String],
    request: &AugmentPluginRequestV1,
    timeout: Option<Duration>,
) -> Result<AugmentPluginResponseV1> {
    let payload = serde_json::to_vec(request)?;
    let timeout = timeout.ok_or_else(|| anyhow!("LLM plugin timeout is required"))?;
    let limits = crate::security::ProcessLimits::plugin(timeout)?;
    let context = format!("llm plugin `{}`", program.display());
    let mut command = Command::new(program);
    command.args(args);
    let output = crate::security::run_command_bounded(command, &payload, limits, &context)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "llm plugin `{}` failed: {}\n{}",
            program.display(),
            output.status,
            stderr.trim()
        ));
    }

    crate::security::parse_json_bounded(
        &output.stdout,
        axiograph_security::DEFAULT_PLUGIN_STDOUT_BYTES,
        "LLM augmentation plugin response",
    )
}

#[allow(clippy::too_many_arguments)]
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
        let digest = axiograph_kernel::object_blob_digest_v2(key.as_bytes());
        format!("llm_entity::{et}::{digest}")
    }

    fn llm_relation_id(rel_type: &str, source: &str, target: &str) -> String {
        let rt = sanitize_symbol(rel_type, 64);
        let key = format!("llm_relation:{rt}:{source}:{target}");
        let digest = axiograph_kernel::object_blob_digest_v2(key.as_bytes());
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
                t.push('…');
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

        let content: String = match llm_backend {
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
                return Err(anyhow!(
                    "unsupported llm backend `{other}` for augment-proposals"
                ));
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
                t.push('…');
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

    let content: String = match llm_backend {
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
            return Err(anyhow!(
                "unsupported llm backend `{other}` for augment-proposals"
            ));
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
        metadata.insert(
            "derived_from".to_string(),
            format!("{llm_backend}_augment_proposals_v1"),
        );
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
        metadata.insert(
            "derived_from".to_string(),
            format!("{llm_backend}_augment_proposals_v1"),
        );
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
                from_type = f.ty.referenced_name().to_string();
            } else if f.field == "to" {
                to_type = f.ty.referenced_name().to_string();
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

    let content: String = match llm_backend {
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
            return Err(anyhow!(
                "unsupported llm backend `{other}` for draft-module structure suggestions"
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

    let out = crate::schema_discovery::DraftAxiModuleSuggestions {
        subtypes: parsed
            .subtypes
            .into_iter()
            .map(|s| crate::schema_discovery::SuggestedSubtype {
                sub: s.sub,
                sup: s.sup,
                public_rationale: s.public_rationale,
            })
            .collect(),
        constraints: parsed
            .constraints
            .into_iter()
            .map(|c| crate::schema_discovery::SuggestedConstraint {
                kind: c.kind,
                relation: c.relation,
                public_rationale: c.public_rationale,
            })
            .collect(),
    };
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
    llm_suggest_schema_structure(
        "openai",
        base_url,
        model,
        base_draft_axi,
        schema_name,
        timeout,
    )
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

#[allow(clippy::too_many_arguments)]
fn cmd_discover_augment_proposals(
    proposals_path: &Path,
    out: &Path,
    trace_path: Option<&Path>,
    chunks_path: Option<&Path>,
    llm_plugin: Option<&Path>,
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

    let text = crate::security::read_utf8_file_bounded(
        proposals_path,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let proposals: axiograph_ingest_docs::ProposalsFileV1 = crate::security::parse_json_bounded(
        text.as_bytes(),
        crate::security::MAX_JSON_INPUT_BYTES,
        "CLI JSON input",
    )?;
    axiograph_ingest_docs::validate_proposals_file_v1(&proposals)?;

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
            let chunks_text = crate::security::read_utf8_file_bounded(
                chunks_path,
                crate::security::MAX_TEXT_INPUT_BYTES,
                "CLI input",
            )?;
            let chunks = axiograph_ingest_docs::chunks_from_json_str(&chunks_text)?;

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
                    t.push('…');
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
                    anyhow!(
                        "missing `--llm-model` (example: --llm-model claude-3-5-sonnet-20241022)"
                    )
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
    crate::security::write_output_bounded(out, &json, "CLI output")?;
    println!("  {} {}", "→".cyan(), out.display());

    let trace_out = trace_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(format!("{}.trace.json", out.display())));
    let trace_json = serde_json::to_string_pretty(&trace)?;
    crate::security::write_output_bounded(&trace_out, &trace_json, "CLI output")?;
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

fn cmd_discover_training_export(
    input: &Path,
    out: &Path,
    instance_filter: Option<&str>,
    max_items: usize,
    mask_fields: usize,
    seed: u64,
) -> Result<()> {
    let opts = crate::predictive_proposals::MaskedTupleTrainingExportOptionsV1 {
        instance_filter: instance_filter.map(|s| s.to_string()),
        max_items,
        mask_fields,
        seed,
        exclude_relations: Vec::new(),
    };
    crate::predictive_proposals::write_training_export(input, out, &opts)?;
    println!("wrote {}", out.display());
    Ok(())
}

fn discover_check_olog_report_from_inputs(
    axi_text: &str,
    schema_name: Option<&str>,
    fragment_json: &str,
    apply_refinement_handle_id: Option<&str>,
) -> Result<crate::typed_authoring::DiscoverCheckOlogReportV1> {
    let fragment: crate::typed_authoring::OlogFragmentV1 = crate::security::parse_json_bounded(
        fragment_json.as_bytes(),
        crate::security::MAX_JSON_INPUT_BYTES,
        "CLI JSON input",
    )
    .map_err(|e| anyhow!("failed to parse olog fragment JSON: {e}"))?;
    crate::typed_authoring::discover_check_olog_report_against_axi_text(
        axi_text,
        schema_name,
        fragment,
        apply_refinement_handle_id,
    )
}

#[derive(Debug, Serialize)]
struct DiscoverTheoryGraphReportV1 {
    version: String,
    module_digest: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    graphs: Vec<axiograph_pathdb::kernel_ir::TheoryObligationGraphV1>,
    trust_boundary: String,
    completeness_claim: String,
    ontology_closure_claim: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    notes: Vec<String>,
}

fn discover_theory_graph_report_from_axi_text(
    axi_text: &str,
    theory_filter: Option<&str>,
) -> Result<DiscoverTheoryGraphReportV1> {
    let canonical = crate::axi_input::require_canonical_axi_text(axi_text)?;
    let kernel =
        axiograph_pathdb::derive_runtime_module_index(canonical.module().module(), axi_text)
            .map_err(|err| anyhow!("failed to derive runtime module index: {err}"))?;
    let mut graphs = kernel
        .theories
        .iter()
        .filter(|theory| {
            let Some(filter) = theory_filter else {
                return true;
            };
            theory.theory_id.as_str() == filter
                || theory
                    .theory_id
                    .as_str()
                    .rsplit_once(':')
                    .is_some_and(|(_, local)| local == filter)
        })
        .map(axiograph_pathdb::kernel_ir::TheoryIr::obligation_graph)
        .collect::<Vec<_>>();
    graphs.sort_by_key(|a| a.theory_ref.stable_id());
    if graphs.is_empty() {
        return Err(anyhow!(
            "no compiled theories matched{}",
            theory_filter
                .map(|filter| format!(" `{filter}`"))
                .unwrap_or_default()
        ));
    }
    Ok(DiscoverTheoryGraphReportV1 {
        version: "discover_theory_graph_report_v1".to_string(),
        module_digest: canonical.digest().to_string(),
        graphs,
        trust_boundary: "Runtime checked in Rust; not Lean verified.".to_string(),
        completeness_claim: "not_claimed".to_string(),
        ontology_closure_claim: "not_claimed".to_string(),
        notes: vec![
            "theory obligation graphs are runtime-addressable indexes for exploration, CQ repair, migration, and reconciliation".to_string(),
            "runtime graph emission does not certify completeness, ontology closure, or Lean proof obligations".to_string(),
        ],
    })
}

fn migration_preview_schema_from_axi_schema(
    schema: &axiograph_dsl::schema_v1::SchemaV1Schema,
) -> Result<axiograph_pathdb::migration::SchemaV1> {
    let arrows = schema
        .relations
        .iter()
        .map(|relation| {
            if relation.fields.len() < 2 {
                return Err(anyhow!(
                    "relation `{}` in schema `{}` cannot be lowered into SchemaV1: expected at least two fields",
                    relation.name,
                    schema.name
                ));
            }
            Ok(axiograph_pathdb::migration::ArrowDeclV1 {
                name: relation.name.clone(),
                src: relation.fields[0].ty.referenced_name().to_string(),
                dst: relation.fields[1].ty.referenced_name().to_string(),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(axiograph_pathdb::migration::SchemaV1 {
        name: schema.name.clone(),
        objects: schema.objects.clone(),
        arrows,
        subtypes: schema
            .subtypes
            .iter()
            .map(|subtype| axiograph_pathdb::migration::SubtypeDeclV1 {
                sub: subtype.sub.clone(),
                sup: subtype.sup.clone(),
                incl: subtype
                    .inclusion
                    .clone()
                    .unwrap_or_else(|| format!("{}_incl", subtype.sub)),
            })
            .collect(),
    })
}

fn select_transport_preview_schema<'a>(
    module: &'a axiograph_dsl::schema_v1::SchemaV1Module,
    requested_schema_name: Option<&str>,
    morphism_source_schema: &str,
) -> Result<&'a axiograph_dsl::schema_v1::SchemaV1Schema> {
    if let Some(schema_name) = requested_schema_name {
        return module
            .schemas
            .iter()
            .find(|schema| schema.name == schema_name)
            .ok_or_else(|| anyhow!("module contains no schema `{schema_name}`"));
    }

    if let Some(schema) = module
        .schemas
        .iter()
        .find(|schema| schema.name == morphism_source_schema)
    {
        return Ok(schema);
    }

    if module.schemas.len() == 1 {
        return Ok(&module.schemas[0]);
    }

    let mut schema_names = module
        .schemas
        .iter()
        .map(|schema| schema.name.clone())
        .collect::<Vec<_>>();
    schema_names.sort();
    Err(anyhow!(
        "module contains multiple schemas; pass --schema (available: {})",
        schema_names.join(", ")
    ))
}

pub(crate) fn discover_transport_preview_from_inputs(
    axi_text: &str,
    schema_name: Option<&str>,
    morphism_json: &str,
    apply_refinement_handle_id: Option<&str>,
) -> Result<crate::evolution_preview::EvolutionPreviewV1> {
    let canonical = crate::axi_input::require_canonical_axi_text(axi_text)?;
    let validated = canonical.module();
    let module = validated.module();
    let morphism: axiograph_pathdb::migration::SchemaMorphismV1 =
        crate::security::parse_json_bounded(
            morphism_json.as_bytes(),
            crate::security::MAX_JSON_INPUT_BYTES,
            "CLI JSON input",
        )
        .map_err(|e| anyhow!("failed to parse schema morphism JSON: {e}"))?;
    let schema = select_transport_preview_schema(module, schema_name, &morphism.source_schema)?;
    if schema.name != morphism.source_schema {
        return Err(anyhow!(
            "selected schema `{}` does not match morphism source schema `{}`",
            schema.name,
            morphism.source_schema
        ));
    }
    let source_schema = migration_preview_schema_from_axi_schema(schema)?;
    let compiled_schema = validated
        .compiled_schema_ir(&schema.name)?
        .ok_or_else(|| anyhow!("failed to compile schema IR for `{}`", schema.name))?;
    let theories = validated.compiled_theories_for_schema(&schema.name)?;
    let candidate_label = format!("{}->{}", morphism.source_schema, morphism.target_schema);

    if let Some(handle_id) = apply_refinement_handle_id {
        return Ok(
            crate::evolution_preview::apply_runtime_refinement_by_id_to_migration_preview_from_compiled_theory_v1(
                None,
                candidate_label,
                &compiled_schema,
                &theories,
                &morphism,
                &source_schema,
                handle_id,
            )?
            .evolution_preview,
        );
    }

    Ok(
        crate::evolution_preview::build_migration_evolution_preview_from_compiled_theory_v1(
            None,
            canonical.digest().as_str(),
            candidate_label,
            &morphism,
            &source_schema,
            &compiled_schema,
            &theories,
        ),
    )
}

fn cmd_discover_check_olog(args: &DiscoverCheckOlogArgs) -> Result<()> {
    let axi_text = crate::security::read_utf8_file_bounded(
        &args.input,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let fragment_json = crate::security::read_utf8_file_bounded(
        &args.fragment,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let report = discover_check_olog_report_from_inputs(
        &axi_text,
        args.schema.as_deref(),
        &fragment_json,
        args.apply_refinement_handle_id.as_deref(),
    )?;
    let json = serde_json::to_string_pretty(&report)?;
    if let Some(path) = args.out.as_ref() {
        crate::security::write_output_bounded(path, json, "CLI output")?;
        println!("wrote {}", path.display());
    } else {
        println!("{json}");
    }
    Ok(())
}

fn cmd_discover_theory_graph(args: &DiscoverTheoryGraphArgs) -> Result<()> {
    let axi_text = crate::security::read_utf8_file_bounded(
        &args.input,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let report = discover_theory_graph_report_from_axi_text(&axi_text, args.theory.as_deref())?;
    write_json_output(&report, args.out.as_ref())
}

fn cmd_discover_kernel_surface(args: &DiscoverKernelSurfaceArgs) -> Result<()> {
    let axi_text = crate::security::read_utf8_file_bounded(
        &args.input,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let canonical = crate::axi_input::require_canonical_axi_text(&axi_text)?;
    let kernel =
        axiograph_pathdb::derive_runtime_module_index(canonical.module().module(), &axi_text)
            .map_err(|err| anyhow!("failed to derive runtime module index: {err}"))?;
    write_json_output(&kernel.runtime_semantic_index(), args.out.as_ref())
}

fn cmd_discover_theory_check(args: &DiscoverTheoryCheckArgs) -> Result<()> {
    let axi_text = crate::security::read_utf8_file_bounded(
        &args.input,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let closure_tier =
        crate::runtime_theory_check::parse_runtime_theory_closure_tier(&args.closure_tier)?;
    let (world, evidence_policy) = runtime_theory_cli_assumptions(
        args.world_id.as_deref(),
        args.finite_world,
        &args.included_refs,
        &args.included_worlds,
        &args.included_slices,
        &args.included_imports,
        &args.undeclared_imports,
        args.evidence_threshold_ppm,
        &args.evidence_semantics,
        args.weighted_evidence,
        &args.evidence_weights,
    )?;
    let report =
        crate::runtime_theory_check::runtime_theory_check_reports_from_axi_text_with_assumptions(
            &axi_text,
            args.theory.as_deref(),
            closure_tier,
            world,
            evidence_policy,
        )?;
    write_json_output(&report, args.out.as_ref())
}

fn cmd_discover_context_report(args: &DiscoverContextReportArgs) -> Result<()> {
    let db = load_pathdb_for_cli(&args.input)?;
    let request_json = crate::security::read_utf8_file_bounded(
        &args.request,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let report = crate::context_report::discover_context_report_from_request_json(
        &db,
        None,
        None,
        &request_json,
    )?;
    write_json_output(&report, args.out.as_ref())
}

fn cmd_discover_overlay_check(args: &DiscoverOverlayCheckArgs) -> Result<()> {
    let kernel = compile_kernel_for_tooling_overlay(&args.input)?;
    let overlay = load_tooling_overlay(&args.overlay)?;
    let report = axiograph_tooling_overlays::validate_overlay_bundle(&kernel, &overlay);
    write_json_output(&report, args.out.as_ref())
}

fn cmd_discover_coverage_query(args: &DiscoverCoverageQueryArgs) -> Result<()> {
    let kernel = compile_kernel_for_tooling_overlay(&args.input)?;
    let query = coverage_query_from_discover_args(args)?;
    let overlay = args
        .overlay
        .as_deref()
        .map(load_tooling_overlay)
        .transpose()?;
    let report =
        axiograph_tooling_overlays::coverage_query_report(&kernel, overlay.as_ref(), &query)?;
    write_json_output(&report, args.out.as_ref())
}

fn coverage_query_from_discover_args(
    args: &DiscoverCoverageQueryArgs,
) -> Result<axiograph_tooling_overlays::CoverageQueryV1> {
    let mut query = if let Some(path) = args.query.as_ref() {
        let query_json = crate::security::read_file_bounded(
            path,
            crate::security::MAX_JSON_INPUT_BYTES,
            "CoverageQueryV1",
        )?;
        crate::security::parse_json_bounded::<axiograph_tooling_overlays::CoverageQueryV1>(
            &query_json,
            crate::security::MAX_JSON_INPUT_BYTES,
            "CoverageQueryV1",
        )?
    } else {
        axiograph_tooling_overlays::CoverageQueryV1 {
            version: Some(axiograph_tooling_overlays::COVERAGE_QUERY_VERSION_V1.to_string()),
            coverage_mode: axiograph_tooling_overlays::CoverageModeV1::Exploratory,
            terms: Vec::new(),
            relation_names: Vec::new(),
            cq_names: Vec::new(),
            code_refs: Vec::new(),
            surface_hints: Vec::new(),
            axql: None,
            max_matches: None,
        }
    };
    query.terms.extend(args.terms.iter().cloned());
    query
        .relation_names
        .extend(args.relation_names.iter().cloned());
    query.cq_names.extend(args.cq_names.iter().cloned());
    query.code_refs.extend(args.code_refs.iter().cloned());
    query
        .surface_hints
        .extend(args.surface_hints.iter().cloned());
    if let Some(axql) = args.axql.as_ref() {
        query.axql = Some(axql.clone());
    }
    if args.max_matches.is_some() {
        query.max_matches = args.max_matches;
    }
    if query.terms.is_empty()
        && query.relation_names.is_empty()
        && query.cq_names.is_empty()
        && query.code_refs.is_empty()
        && query.surface_hints.is_empty()
        && query.axql.as_deref().unwrap_or("").trim().is_empty()
    {
        return Err(anyhow!(
            "coverage-query requires --query or at least one --term, --relation, --cq-name, --code-ref, --surface-hint, or --axql"
        ));
    }
    Ok(query)
}

fn cmd_discover_define(args: &DiscoverDefineArgs) -> Result<()> {
    let kernel = compile_kernel_for_tooling_overlay(&args.input)?;
    let overlay = args
        .overlay
        .as_deref()
        .map(load_tooling_overlay)
        .transpose()?;
    let query = axiograph_tooling_overlays::DefinitionQueryV1 {
        version: Some(axiograph_tooling_overlays::DEFINITION_QUERY_VERSION_V1.to_string()),
        prompt: args.prompt.clone(),
        kind_hint: parse_definition_kind_hint(args.kind_hint.as_deref())?,
        context_hint: args.context_hint.clone(),
        candidate_refs: Vec::new(),
        max_matches: args.max_matches,
        include_queries: args.include_queries,
    };
    let report =
        axiograph_tooling_overlays::definition_query_report(&kernel, overlay.as_ref(), &query)?;
    write_json_output(&report, args.out.as_ref())
}

fn cmd_discover_embedding_relationships(args: &DiscoverEmbeddingRelationshipsArgs) -> Result<()> {
    let text = crate::security::read_utf8_file_bounded(
        &args.embeddings,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let file: crate::embeddings::EmbeddingsFileV1 = crate::security::parse_json_bounded(
        text.as_bytes(),
        crate::security::MAX_TEXT_INPUT_BYTES,
        "EmbeddingsFileV1",
    )?;
    crate::embeddings::validate_embeddings_file_v1(&file)?;
    let accepted = crate::embeddings::EmbeddingAcceptedRefV1 {
        accepted_ref: args.accepted_ref.clone(),
        accepted_axi_anchor: axiograph_pathdb::AcceptedAxiAnchor::new(
            axiograph_pathdb::AcceptedSnapshotId::new(args.accepted_snapshot_id.clone()),
            axiograph_pathdb::AxiDigest::new(args.axi_digest.clone()),
        ),
        module_name: args.module_name.clone(),
        compiled_ir_digest: args.compiled_ir_digest.clone(),
    };
    let mut manifest_input = crate::embeddings::EmbeddingSidecarManifestBuildInputV1::new(accepted);
    manifest_input.model_version = args.model_version.clone();
    manifest_input.model_digest = args.model_digest.clone();
    manifest_input.deployment_id = args
        .deployment_id
        .clone()
        .or_else(|| Some("embedding_relationships_cli_v1".to_string()));
    let manifest = crate::embeddings::build_embedding_sidecar_manifest_v1(&file, manifest_input)?;
    let config = crate::embeddings::EmbeddingRelationshipDiscoveryConfigV1 {
        min_cosine_similarity: args.min_cosine_similarity,
        max_relationships: args.max_relationships,
        relationship: parse_embedding_relationship_kind(&args.relationship)?,
        ..Default::default()
    };
    let overlay =
        crate::embeddings::discover_embedding_evidence_overlay_v1(&file, &manifest, config)?;
    write_json_output(
        &serde_json::json!({
            "version": "embedding_relationship_discovery_report_v1",
            "manifest": manifest,
            "overlay": overlay,
            "trust": {
                "authority": "evidence_plane_only",
                "mutation_authority": "axiograph_review_promotion_only",
                "non_claim": "embedding similarity is not semantic equivalence, subtype proof, or accepted .axi truth"
            }
        }),
        args.out.as_ref(),
    )
}

fn parse_embedding_relationship_kind(
    raw: &str,
) -> Result<crate::embeddings::EmbeddingRelationshipKindV1> {
    use crate::embeddings::EmbeddingRelationshipKindV1;
    match raw.trim().to_ascii_lowercase().as_str() {
        "similar_to" | "similar" => Ok(EmbeddingRelationshipKindV1::SimilarTo),
        "supports" | "support" => Ok(EmbeddingRelationshipKindV1::Supports),
        "mentions" | "mention" => Ok(EmbeddingRelationshipKindV1::Mentions),
        "implements" | "implement" => Ok(EmbeddingRelationshipKindV1::Implements),
        "violates" | "violate" => Ok(EmbeddingRelationshipKindV1::Violates),
        "subtype_candidate" | "subtype" => Ok(EmbeddingRelationshipKindV1::SubtypeCandidate),
        "same_as_candidate" | "same_as" => Ok(EmbeddingRelationshipKindV1::SameAsCandidate),
        "relation_candidate" | "relation" => Ok(EmbeddingRelationshipKindV1::RelationCandidate),
        "axiom_candidate" | "axiom" => Ok(EmbeddingRelationshipKindV1::AxiomCandidate),
        "contradicts" | "contradict" => Ok(EmbeddingRelationshipKindV1::Contradicts),
        "unknown" => Ok(EmbeddingRelationshipKindV1::Unknown),
        other => Err(anyhow!(
            "unknown embedding relationship `{other}` (expected similar_to|supports|mentions|implements|violates|subtype_candidate|same_as_candidate|relation_candidate|axiom_candidate|contradicts|unknown)"
        )),
    }
}

fn cmd_discover_behavior_case(args: &DiscoverBehaviorCaseArgs) -> Result<()> {
    let repository_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let package = crate::axi_input::compile_canonical_axi_path(&args.input, &[repository_root])?;
    let db = load_pathdb_for_cli(&args.input)?;
    let mut request = load_behavior_case_request(&args.request)?;
    if request.runtime_theory_check.is_some() || request.runtime_theory_check_input.is_some() {
        return Err(anyhow!(
            "discover behavior-case computes runtime theory from the canonical input; request-side theory summaries are not accepted"
        ));
    }
    if !package.snapshot().ir().theories().is_empty() {
        request.runtime_theory_check = Some(
            crate::runtime_theory_check::runtime_theory_check_reports_from_package(
                &package,
                None,
                axiograph_pathdb::RuntimeTheoryClosureTierV1::FiniteFragment,
                axiograph_pathdb::default_world_assumption_v1(),
                axiograph_pathdb::default_evidence_policy_v1(),
            )?
            .summary,
        );
    }
    attach_behavior_case_cq_files(&mut request, &args.cq_files)?;
    if let Some(overlay_path) = args.overlay.as_deref() {
        let overlay = load_tooling_overlay(overlay_path)?;
        request.codegen = behavior_codegen_request_from_overlay(&overlay)?;
        request.overlay = Some(overlay);
    } else if let Some(overlay) = request.overlay.as_ref() {
        request.codegen = behavior_codegen_request_from_overlay(overlay)?;
    }
    let report =
        crate::behavior_case::build_behavior_case_report_from_request(&db, None, None, request)?;
    write_json_output(&report, args.out.as_ref())
}

fn attach_behavior_case_cq_files(
    request: &mut crate::behavior_case::BehaviorCaseCheckRequestV1,
    cq_files: &[PathBuf],
) -> Result<()> {
    if cq_files.is_empty() {
        return Ok(());
    }
    let mut loaded = Vec::new();
    for path in cq_files {
        loaded.extend(crate::predictive_proposals::load_competency_questions(
            path,
        )?);
    }
    let then =
        request
            .behavior_case
            .then
            .get_or_insert_with(|| crate::behavior_case::BehaviorThenV1 {
                expected_outcomes: Vec::new(),
                competency_questions: Vec::new(),
                rule_scopes: Vec::new(),
                trust_target: None,
                notes: Vec::new(),
            });
    then.competency_questions.extend(loaded);
    Ok(())
}

fn compile_kernel_for_tooling_overlay(
    input: &Path,
) -> Result<axiograph_pathdb::kernel_ir::RuntimeModuleIndex> {
    let axi_text = crate::security::read_utf8_file_bounded(
        input,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    axiograph_tooling_overlays::derive_runtime_index_from_axi_text(&axi_text)
}

fn load_tooling_overlay(path: &Path) -> Result<axiograph_tooling_overlays::ToolingOverlayBundleV1> {
    let json_text = crate::security::read_utf8_file_bounded(
        path,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    axiograph_tooling_overlays::parse_overlay_bundle(&json_text)
}

fn load_behavior_case_request(
    path: &Path,
) -> Result<crate::behavior_case::BehaviorCaseCheckRequestV1> {
    let request_json = crate::security::read_utf8_file_bounded(
        path,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    crate::security::parse_json_bounded(
        request_json.as_bytes(),
        crate::security::MAX_JSON_INPUT_BYTES,
        "CLI JSON input",
    )
    .map_err(|err| anyhow!("failed to parse BehaviorCaseCheckRequestV1 JSON: {err}"))
}

fn behavior_codegen_request_from_overlay(
    overlay: &axiograph_tooling_overlays::ToolingOverlayBundleV1,
) -> Result<crate::behavior_case::BehaviorCaseCodegenRequestV1> {
    let languages = overlay
        .codegen_plan
        .languages
        .iter()
        .map(|language| parse_behavior_codegen_language(language))
        .collect::<Result<Vec<_>>>()?;
    Ok(crate::behavior_case::BehaviorCaseCodegenRequestV1 { languages })
}

fn parse_behavior_codegen_language(
    language: &str,
) -> Result<crate::behavior_case::BehaviorCaseCodegenLanguageV1> {
    match language.trim().to_ascii_lowercase().as_str() {
        "go" => Ok(crate::behavior_case::BehaviorCaseCodegenLanguageV1::Go),
        "python" | "py" => Ok(crate::behavior_case::BehaviorCaseCodegenLanguageV1::Python),
        "rust" | "rs" => Ok(crate::behavior_case::BehaviorCaseCodegenLanguageV1::Rust),
        "typescript" | "ts" => Ok(crate::behavior_case::BehaviorCaseCodegenLanguageV1::Typescript),
        other => Err(anyhow!(
            "unsupported behavior-case codegen language `{other}` (expected go|python|rust|typescript)"
        )),
    }
}

fn parse_definition_kind_hint(
    kind: Option<&str>,
) -> Result<Option<axiograph_tooling_overlays::DefinitionQueryKindV1>> {
    let Some(kind) = kind else {
        return Ok(None);
    };
    let parsed = match kind.trim().to_ascii_lowercase().as_str() {
        "process" => axiograph_tooling_overlays::DefinitionQueryKindV1::Process,
        "function" => axiograph_tooling_overlays::DefinitionQueryKindV1::Function,
        "business_rule" | "business-rule" | "rule" => {
            axiograph_tooling_overlays::DefinitionQueryKindV1::BusinessRule
        }
        "domain_object" | "domain-object" | "object" => {
            axiograph_tooling_overlays::DefinitionQueryKindV1::DomainObject
        }
        "relation" => axiograph_tooling_overlays::DefinitionQueryKindV1::Relation,
        "invariant" => axiograph_tooling_overlays::DefinitionQueryKindV1::Invariant,
        "policy" => axiograph_tooling_overlays::DefinitionQueryKindV1::Policy,
        "implementation_surface" | "implementation-surface" | "surface" => {
            axiograph_tooling_overlays::DefinitionQueryKindV1::ImplementationSurface
        }
        "unknown" => axiograph_tooling_overlays::DefinitionQueryKindV1::Unknown,
        other => {
            return Err(anyhow!(
                "unknown definition kind hint `{other}` (expected process|function|business_rule|domain_object|relation|invariant|policy|implementation_surface)"
            ))
        }
    };
    Ok(Some(parsed))
}

fn cmd_discover_route_preview(args: &DiscoverRoutePreviewArgs) -> Result<()> {
    let db = load_pathdb_for_cli(&args.input)?;
    let request_json = crate::security::read_utf8_file_bounded(
        &args.request,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let report =
        crate::route_preview::discover_route_preview_from_request_json(&db, &request_json)?;
    write_json_output(&report, args.out.as_ref())
}

fn cmd_discover_transport_preview(args: &DiscoverTransportPreviewArgs) -> Result<()> {
    let axi_text = crate::security::read_utf8_file_bounded(
        &args.input,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let morphism_json = crate::security::read_utf8_file_bounded(
        &args.morphism,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
    let preview = discover_transport_preview_from_inputs(
        &axi_text,
        args.schema.as_deref(),
        &morphism_json,
        args.apply_refinement_handle_id.as_deref(),
    )?;
    write_json_output(&preview, args.out.as_ref())
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

    let mut out: Vec<crate::predictive_proposals::CompetencyQuestionV1> = Vec::new();
    if !args.no_schema {
        let mut generated = crate::competency_questions::generate_from_schema(&db, &options)?;
        out.append(&mut generated);
    }

    if let Some(path) = args.from_cq.as_ref() {
        let mut loaded = crate::predictive_proposals::load_competency_questions(path)?;
        out.append(&mut loaded);
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
    crate::security::write_output_bounded(&args.out, json, "CLI output")?;
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
            let model = args
                .llm_model
                .clone()
                .ok_or_else(|| anyhow!("`--llm-ollama` requires `--llm-model <model>`"))?;
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
                if env.is_empty() {
                    None
                } else {
                    Some(env)
                }
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
            let key = std::env::var(crate::llm::ANTHROPIC_API_KEY_ENV).unwrap_or_default();
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
                let env = std::env::var(crate::llm::ANTHROPIC_MODEL_ENV).unwrap_or_default();
                let env = env.trim().to_string();
                if env.is_empty() {
                    None
                } else {
                    Some(env)
                }
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

const PREDICTIVE_PROPOSAL_BACKEND_ENV: &str = "PREDICTIVE_PROPOSAL_BACKEND";
const PREDICTIVE_PROPOSAL_MODEL_ENV: &str = "PREDICTIVE_PROPOSAL_MODEL";

fn resolve_llm_state_for_predictive_proposal_plugin(
    args: &PredictiveProposalsLlmArgs,
) -> Result<crate::llm::LlmState> {
    let backend = args
        .backend
        .clone()
        .or_else(|| {
            env::var(PREDICTIVE_PROPOSAL_BACKEND_ENV)
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "openai".to_string());

    let model = args.model.clone().or_else(|| {
        env::var(PREDICTIVE_PROPOSAL_MODEL_ENV)
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
                        anyhow!("no model selected (use --model, set PREDICTIVE_PROPOSAL_MODEL, or set OLLAMA_MODEL)")
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
                            "no model selected (use --model, set PREDICTIVE_PROPOSAL_MODEL, or set {})",
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
                            "no model selected (use --model, set PREDICTIVE_PROPOSAL_MODEL, or set {})",
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
            "predictive proposal adapter backend `{other}` is not supported by --proposal-adapter-llm / `axiograph ingest predictive-proposals-llm` (expected openai|anthropic|ollama|mock). If you meant an ONNX or custom model, use --proposal-adapter-plugin or --proposal-adapter-http instead."
        )),
    }
}

fn cmd_predictive_proposals(args: &PredictiveProposalsArgs) -> Result<()> {
    let selected = (args.predictive_proposal_stub as usize)
        + (args.predictive_proposal_plugin.is_some() as usize)
        + (args.predictive_proposal_http.is_some() as usize)
        + (args.predictive_proposal_llm as usize);
    if selected > 1 {
        return Err(anyhow!(
            "choose at most one predictive proposal adapter backend: --proposal-adapter-stub, --proposal-adapter-plugin, --proposal-adapter-http, or --proposal-adapter-llm"
        ));
    }
    if selected == 0 {
        return Err(anyhow!(
            "predictive proposal adapter backend is not configured (use --proposal-adapter-plugin, --proposal-adapter-http, --proposal-adapter-llm, or --proposal-adapter-stub)"
        ));
    }

    let mut adapter = crate::predictive_proposals::ProposalAdapterState::default();
    if args.predictive_proposal_stub {
        adapter.backend = crate::predictive_proposals::ProposalAdapterBackend::Stub;
    } else if let Some(url) = args.predictive_proposal_http.as_ref() {
        adapter.backend =
            crate::predictive_proposals::ProposalAdapterBackend::Http { url: url.clone() };
    } else if args.predictive_proposal_llm {
        let exe = std::env::current_exe()
            .map_err(|e| anyhow!("failed to resolve current executable: {e}"))?;
        let mut args_list = vec!["ingest".to_string(), "predictive-proposals-llm".to_string()];
        let has_model_arg = args
            .predictive_proposal_plugin_arg
            .iter()
            .any(|a| a == "--model");
        if let Some(model) = args.predictive_proposal_model.as_ref() {
            if !has_model_arg {
                args_list.push("--model".to_string());
                args_list.push(model.clone());
            }
        }
        args_list.extend(args.predictive_proposal_plugin_arg.clone());
        adapter.backend = crate::predictive_proposals::ProposalAdapterBackend::Command {
            program: exe,
            args: args_list,
        };
    } else if let Some(plugin) = args.predictive_proposal_plugin.as_ref() {
        adapter.backend = crate::predictive_proposals::ProposalAdapterBackend::Command {
            program: plugin.clone(),
            args: args.predictive_proposal_plugin_arg.clone(),
        };
    }
    adapter.model = args.predictive_proposal_model.clone();

    let input_ext = args
        .input
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    let training_export = Some(
        crate::predictive_proposals::MaskedTupleTrainingExportOptionsV1 {
            instance_filter: args.export_instance.clone(),
            max_items: args.export_max_items,
            mask_fields: args.export_mask_fields,
            seed: args.export_seed,
            exclude_relations: Vec::new(),
        },
    );

    let guardrail_profile = args.guardrail_profile.trim().to_ascii_lowercase();
    let guardrail_plane = args.guardrail_plane.trim().to_ascii_lowercase();
    let guardrail_weights = if args.guardrail_weight.is_empty() {
        crate::predictive_proposals::GuardrailCostWeightsV1::defaults()
    } else {
        crate::predictive_proposals::parse_guardrail_weights(&args.guardrail_weight)?
    };

    let task_costs = crate::predictive_proposals::parse_task_costs(&args.task_cost)?;

    let guardrail = if guardrail_profile != "off" {
        let loaded = crate::load_pathdb_for_cli(&args.input)?;
        let report = crate::predictive_proposals::compute_guardrail_costs(
            &loaded,
            &args.input.display().to_string(),
            &guardrail_profile,
            &guardrail_plane,
            &guardrail_weights,
        )?;
        if let Some(path) = args.guardrail_out.as_ref() {
            let json = serde_json::to_string_pretty(&report)?;
            crate::security::write_output_bounded(path, json, "CLI output")?;
            println!("wrote {}", path.display());
        }
        Some(report)
    } else {
        None
    };

    let mut input = if input_ext.eq_ignore_ascii_case("axi") {
        let text = crate::security::read_utf8_file_bounded(
            &args.input,
            crate::security::MAX_TEXT_INPUT_BYTES,
            "CLI input",
        )?;
        crate::predictive_proposal_input::build_predictive_proposal_input_from_axi_text(
            &text,
            None,
            None,
            None,
            training_export,
        )?
    } else {
        return Err(anyhow!(
            "predictive proposal adapter input must be exact canonical `.axi` bytes"
        ));
    };
    if let Some(guardrail) = guardrail.clone() {
        input.set_guardrail_layer(guardrail);
    }
    input
        .notes
        .push("source=cli_predictive_proposals".to_string());

    let options = crate::predictive_proposals::PredictiveProposalOptionsV1 {
        max_new_proposals: args.max_new_proposals,
        seed: args.seed,
        goals: args.goal.clone(),
        task_costs: task_costs.clone(),
        horizon_steps: args.horizon_steps,
        ..Default::default()
    };

    let input_materialization_id = input.materialization_id();
    let input_accepted_snapshot_id = input.accepted_snapshot_id();
    let req = crate::predictive_proposals::make_predictive_proposal_request(input, options);
    let mut response = adapter.propose(&req)?;
    if let Some(err) = response.error.take() {
        return Err(anyhow!("predictive proposal adapter error: {err}"));
    }

    let provenance = crate::predictive_proposals::build_predictive_proposal_provenance(
        &response,
        adapter.backend_label(),
        adapter.model.clone(),
        req.input.revision_digest_v2.clone(),
        input_materialization_id,
        input_accepted_snapshot_id,
        guardrail.as_ref().map(|g| g.summary.total_cost),
        if guardrail_profile == "off" {
            None
        } else {
            Some(guardrail_profile.clone())
        },
        if guardrail_profile == "off" {
            None
        } else {
            Some(guardrail_plane.clone())
        },
    )?;

    let mut proposals = crate::predictive_proposals::apply_predictive_proposal_provenance(
        response.proposals,
        &provenance,
    );

    if args.max_new_proposals > 0 && proposals.proposals.len() > args.max_new_proposals {
        proposals.proposals.truncate(args.max_new_proposals);
    }

    let json = serde_json::to_string_pretty(&proposals)?;
    crate::security::write_output_bounded(&args.out, &json, "CLI output")?;
    println!("wrote {}", args.out.display());

    Ok(())
}

fn cmd_predictive_proposal_plugin_llm(args: &PredictiveProposalsLlmArgs) -> Result<()> {
    let input = crate::security::read_utf8_stream_bounded(
        io::stdin(),
        crate::security::MAX_JSON_INPUT_BYTES,
        "predictive proposal stdin",
    )?;
    if input.trim().is_empty() {
        return Err(anyhow!("expected JSON request on stdin"));
    }
    let req: crate::predictive_proposals::PredictiveProposalRequestV1 =
        crate::security::parse_json_bounded(
            input.as_bytes(),
            crate::security::MAX_JSON_INPUT_BYTES,
            "CLI JSON input",
        )
        .map_err(|e| anyhow!("invalid JSON request: {e}"))?;
    let llm = resolve_llm_state_for_predictive_proposal_plugin(args)?;
    let resp = crate::llm::predictive_proposal_llm_plugin(&llm, &req)?;
    let json = serde_json::to_string(&resp)?;
    println!("{json}");
    Ok(())
}

fn account_directory_bytes(total: &mut u64, actual: usize, limit: u64) -> Result<()> {
    *total = total
        .checked_add(actual as u64)
        .ok_or_else(|| anyhow!("directory ingest byte count overflow"))?;
    if *total > limit {
        return Err(anyhow!("directory ingest exceeds {limit} input bytes"));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_ingest_dir(
    root: &Path,
    out_dir: &Path,
    confluence_space: &str,
    domain: &str,
    chunks_path: Option<&Path>,
    facts_path: Option<&Path>,
    proposals_path: Option<&Path>,
    max_file_bytes: u64,
    max_files: usize,
) -> Result<()> {
    const MAX_DIRECTORY_FILES: usize = 10_000;
    const MAX_DIRECTORY_SCAN_ENTRIES: usize = 100_000;
    const MAX_DIRECTORY_DEPTH: usize = 32;
    const MAX_DIRECTORY_CHUNKS: usize = 100_000;
    const MAX_DIRECTORY_FACTS: usize = 100_000;
    const MAX_DIRECTORY_TOTAL_BYTES: u64 = 128 * 1024 * 1024;
    if !(1..=MAX_DIRECTORY_FILES).contains(&max_files) {
        return Err(anyhow!("--max-files must be in 1..={MAX_DIRECTORY_FILES}"));
    }
    if max_file_bytes == 0 || max_file_bytes > crate::security::MAX_TEXT_INPUT_BYTES as u64 {
        return Err(anyhow!(
            "--max-file-bytes must be in 1..={} bytes",
            crate::security::MAX_TEXT_INPUT_BYTES
        ));
    }
    println!(
        "{} {} → {}",
        "Ingesting dir".green().bold(),
        root.display(),
        out_dir.display()
    );

    let root_metadata = fs::symlink_metadata(root)
        .with_context(|| format!("inspect ingest root `{}`", root.display()))?;
    if root_metadata.file_type().is_symlink() || !root_metadata.file_type().is_dir() {
        return Err(anyhow!(
            "directory ingest root must be a real directory, not a symlink or special file"
        ));
    }
    let root = root
        .canonicalize()
        .with_context(|| format!("canonicalize ingest root `{}`", root.display()))?;
    fs::create_dir_all(out_dir)?;

    let mut all_chunks: Vec<axiograph_ingest_docs::Chunk> = Vec::new();
    let mut all_facts: Vec<axiograph_ingest_docs::ExtractedFact> = Vec::new();
    let mut all_proposals: Vec<axiograph_ingest_docs::ProposalV1> = Vec::new();
    let mut files_ingested = 0usize;
    let mut entries_scanned = 0_usize;
    let mut total_input_bytes = 0_u64;

    fn chunk_by_lines(text: &str, max_chars: usize) -> Result<Vec<String>> {
        let mut out: Vec<String> = Vec::new();
        let mut cur = String::new();
        for line in text.lines() {
            let line = line.trim_end();
            if cur.len().saturating_add(line.len() + 1) > max_chars && !cur.is_empty() {
                if out.len() >= 100_000 {
                    return Err(anyhow!("directory chunk count exceeds 100000"));
                }
                out.push(cur);
                cur = String::new();
            }
            if !cur.is_empty() {
                cur.push('\n');
            }
            cur.push_str(line);
        }
        if !cur.trim().is_empty() {
            if out.len() >= 100_000 {
                return Err(anyhow!("directory chunk count exceeds 100000"));
            }
            out.push(cur);
        }
        Ok(out)
    }

    for entry in walkdir::WalkDir::new(&root)
        .follow_links(false)
        .max_depth(MAX_DIRECTORY_DEPTH)
        .into_iter()
        .filter_entry(|e| {
            if !e.file_type().is_dir() {
                return true;
            }
            let name = e.file_name().to_string_lossy();
            name != ".git" && name != "target" && name != "build" && name != "node_modules"
        })
    {
        let entry = entry
            .with_context(|| format!("failed while walking ingest root `{}`", root.display()))?;
        entries_scanned = entries_scanned.saturating_add(1);
        if entries_scanned > MAX_DIRECTORY_SCAN_ENTRIES {
            return Err(anyhow!(
                "directory ingest scan exceeds {MAX_DIRECTORY_SCAN_ENTRIES} filesystem entries"
            ));
        }

        if !entry.file_type().is_file() {
            continue;
        }

        if files_ingested >= max_files {
            break;
        }

        let path = entry.path();
        let metadata = std::fs::symlink_metadata(path)
            .with_context(|| format!("inspect ingest input `{}`", path.display()))?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            continue;
        }
        if metadata.len() > max_file_bytes {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let rel_path = path.strip_prefix(&root).unwrap_or(path);

        // Dispatch by extension. Count the bytes actually read from the
        // no-follow handle rather than metadata observed before opening.
        let mut rdf_grounding_text = None;
        match ext.as_str() {
            "md" | "txt" => {
                let text = match crate::security::read_utf8_file_bounded(
                    path,
                    max_file_bytes as usize,
                    "directory text input",
                ) {
                    Ok(text) => text,
                    Err(_) => continue,
                };
                account_directory_bytes(
                    &mut total_input_bytes,
                    text.len(),
                    MAX_DIRECTORY_TOTAL_BYTES,
                )?;
                let stem = path.file_stem().unwrap_or_default().to_string_lossy();
                let result = axiograph_ingest_docs::extract_knowledge_full(&text, &stem, domain)?;

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
                let html = match crate::security::read_utf8_file_bounded(
                    path,
                    max_file_bytes as usize,
                    "directory HTML input",
                ) {
                    Ok(html) => html,
                    Err(_) => continue,
                };
                account_directory_bytes(
                    &mut total_input_bytes,
                    html.len(),
                    MAX_DIRECTORY_TOTAL_BYTES,
                )?;
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
                let text = match crate::security::read_utf8_file_bounded(
                    path,
                    max_file_bytes as usize,
                    "directory SQL input",
                ) {
                    Ok(text) => text,
                    Err(_) => continue,
                };
                account_directory_bytes(
                    &mut total_input_bytes,
                    text.len(),
                    MAX_DIRECTORY_TOTAL_BYTES,
                )?;
                let doc_id = rel_path.to_string_lossy().to_string();
                let doc_digest = axiograph_kernel::object_blob_digest_v2(doc_id.as_bytes());

                // Evidence chunk(s) for grounding + provenance pointers.
                let mut chunks: Vec<axiograph_ingest_docs::Chunk> = Vec::new();
                for (i, stmt) in text.split(';').enumerate() {
                    let stmt = stmt.trim();
                    if stmt.is_empty() {
                        continue;
                    }
                    if chunks.len() >= MAX_DIRECTORY_CHUNKS {
                        return Err(anyhow!(
                            "SQL statement chunk count exceeds {MAX_DIRECTORY_CHUNKS}"
                        ));
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
                let text = match crate::security::read_utf8_file_bounded(
                    path,
                    max_file_bytes as usize,
                    "directory JSON input",
                ) {
                    Ok(text) => text,
                    Err(_) => continue,
                };
                account_directory_bytes(
                    &mut total_input_bytes,
                    text.len(),
                    MAX_DIRECTORY_TOTAL_BYTES,
                )?;
                if let Ok(value) = crate::security::parse_json_bounded::<serde_json::Value>(
                    text.as_bytes(),
                    max_file_bytes as usize,
                    "directory JSON input",
                ) {
                    let schema = axiograph_ingest_json::infer_schema(&value, "Root");
                    let doc_id = rel_path.to_string_lossy().to_string();
                    let doc_digest = axiograph_kernel::object_blob_digest_v2(doc_id.as_bytes());
                    let pretty = serde_json::to_string_pretty(&value).unwrap_or(text.clone());
                    let parts = chunk_by_lines(&pretty, 2_500)?;

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
            | "xml" => {
                let bytes = crate::security::read_file_bounded(
                    path,
                    max_file_bytes as usize,
                    "RDF ingest input",
                )?;
                account_directory_bytes(
                    &mut total_input_bytes,
                    bytes.len(),
                    MAX_DIRECTORY_TOTAL_BYTES,
                )?;
                rdf_grounding_text = String::from_utf8(bytes.clone()).ok();
                let format = match ext.as_str() {
                    "nt" | "ntriples" => axiograph_ingest_rdfowl::RdfFormatV1::NTriples,
                    "ttl" | "turtle" => axiograph_ingest_rdfowl::RdfFormatV1::Turtle,
                    "nq" | "nquads" => axiograph_ingest_rdfowl::RdfFormatV1::NQuads,
                    "trig" => axiograph_ingest_rdfowl::RdfFormatV1::TriG,
                    "rdf" | "owl" | "xml" => axiograph_ingest_rdfowl::RdfFormatV1::RdfXml,
                    _ => unreachable!("extension matched RDF branch"),
                };
                let proposals = axiograph_ingest_rdfowl::proposals_from_rdf_v1(
                    &bytes,
                    format,
                    Some(rel_path.to_string_lossy().to_string()),
                    Some(domain.to_string()),
                )?;
                all_proposals.extend(proposals);
            }
            _ => continue,
        }

        // For RDF/OWL, also try to preserve a text chunk for grounding (best-effort).
        if matches!(
            ext.as_str(),
            "nt" | "ntriples" | "ttl" | "turtle" | "nq" | "nquads" | "trig" | "rdf" | "owl" | "xml"
        ) {
            if let Some(text) = rdf_grounding_text {
                let doc_id = rel_path.to_string_lossy().to_string();
                let doc_digest = axiograph_kernel::object_blob_digest_v2(doc_id.as_bytes());
                let parts = chunk_by_lines(&text, 2_500)?;
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

        if all_chunks.len() > MAX_DIRECTORY_CHUNKS {
            return Err(anyhow!(
                "directory ingest chunk count exceeds {MAX_DIRECTORY_CHUNKS}"
            ));
        }
        if all_facts.len() > MAX_DIRECTORY_FACTS {
            return Err(anyhow!(
                "directory ingest fact count exceeds {MAX_DIRECTORY_FACTS}"
            ));
        }
        if all_proposals.len() > axiograph_ingest_docs::MAX_PROPOSALS_V1 {
            return Err(anyhow!(
                "directory ingest proposal count exceeds hard limit"
            ));
        }
        files_ingested += 1;
    }

    let chunks_out = chunks_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| out_dir.join("chunks.json"));
    let facts_out = facts_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| out_dir.join("facts.json"));
    let proposals_out = proposals_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| out_dir.join("proposals.json"));

    let chunks_json = axiograph_ingest_docs::chunks_to_json_for_chunks(
        "ingest_dir",
        root.display().to_string(),
        all_chunks,
    )?;
    crate::security::write_output_bounded(&chunks_out, &chunks_json, "CLI output")?;
    println!("  {} {}", "→".cyan(), chunks_out.display());

    let facts_json = serde_json::to_string_pretty(&all_facts)?;
    crate::security::write_output_bounded(&facts_out, &facts_json, "CLI output")?;
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
    axiograph_ingest_docs::validate_proposals_file_v1(&file)?;
    let json = serde_json::to_string_pretty(&file)?;
    crate::security::write_output_bounded(&proposals_out, &json, "CLI output")?;
    println!("  {} {}", "→".cyan(), proposals_out.display());

    println!("  {} {} files ingested", "→".yellow(), files_ingested);
    Ok(())
}

fn cmd_ingest_merge(
    proposals_paths: &[PathBuf],
    chunks_paths: &[PathBuf],
    out_proposals: &PathBuf,
    out_chunks: Option<&Path>,
    schema_hint_override: Option<&str>,
) -> Result<()> {
    if proposals_paths.is_empty() {
        return Err(anyhow!(
            "ingest merge requires at least one --proposals <file.json>"
        ));
    }

    let mut merged_proposals: Vec<axiograph_ingest_docs::ProposalV1> = Vec::new();
    let mut schema_hint: Option<String> = None;

    for p in proposals_paths {
        let text = crate::security::read_utf8_file_bounded(
            p,
            crate::security::MAX_TEXT_INPUT_BYTES,
            "CLI input",
        )?;
        let file: axiograph_ingest_docs::ProposalsFileV1 = crate::security::parse_json_bounded(
            text.as_bytes(),
            crate::security::MAX_JSON_INPUT_BYTES,
            "CLI JSON input",
        )?;
        axiograph_ingest_docs::validate_proposals_file_v1(&file)?;
        if schema_hint.is_none() {
            schema_hint = file.schema_hint.clone();
        }
        merged_proposals.extend(file.proposals);
        if merged_proposals.len() > axiograph_ingest_docs::MAX_PROPOSALS_V1 {
            return Err(anyhow!("merged proposal count exceeds hard limit"));
        }
    }

    // Deduplicate by proposal_id (stable identifiers).
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut deduped: Vec<axiograph_ingest_docs::ProposalV1> =
        Vec::with_capacity(merged_proposals.len());
    for p in merged_proposals {
        let id = match &p {
            axiograph_ingest_docs::ProposalV1::Entity { meta, .. } => meta.proposal_id.clone(),
            axiograph_ingest_docs::ProposalV1::Relation { meta, .. } => meta.proposal_id.clone(),
        };
        if seen.insert(id) {
            deduped.push(p);
        }
    }

    let schema_hint = schema_hint_override.map(|s| s.to_string()).or(schema_hint);

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
    axiograph_ingest_docs::validate_proposals_file_v1(&file)?;

    fs::create_dir_all(out_proposals.parent().unwrap_or(std::path::Path::new(".")))?;
    crate::security::write_output_bounded(
        out_proposals,
        serde_json::to_string_pretty(&file)?,
        "CLI output",
    )?;
    println!("wrote {}", out_proposals.display());

    if !chunks_paths.is_empty() {
        let mut merged_chunks: Vec<axiograph_ingest_docs::Chunk> = Vec::new();
        for p in chunks_paths {
            let text = crate::security::read_utf8_file_bounded(
                p,
                crate::security::MAX_TEXT_INPUT_BYTES,
                "CLI input",
            )?;
            let chunks = axiograph_ingest_docs::chunks_from_json_str(&text)?;
            merged_chunks.extend(chunks);
            if merged_chunks.len() > 100_000 {
                return Err(anyhow!("merged chunk count exceeds hard limit"));
            }
        }

        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut deduped: Vec<axiograph_ingest_docs::Chunk> =
            Vec::with_capacity(merged_chunks.len());
        for c in merged_chunks {
            if seen.insert(c.chunk_id.clone()) {
                deduped.push(c);
            }
        }

        let out_path = out_chunks.map(Path::to_path_buf).unwrap_or_else(|| {
            out_proposals
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join("chunks.json")
        });
        fs::create_dir_all(out_path.parent().unwrap_or(std::path::Path::new(".")))?;
        crate::security::write_output_bounded(
            &out_path,
            axiograph_ingest_docs::chunks_to_json_for_chunks(
                "merged_chunks",
                "merge-proposals",
                deduped,
            )?,
            "CLI output",
        )?;
        println!("wrote {}", out_path.display());
    }

    Ok(())
}

// Ingestion emits `ProposalsFileV1` evidence first; promotion into canonical
// `.axi` is explicit and reviewable.

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn discover_check_olog_report_from_inputs_can_apply_handle() {
        let axi_text = r#"
module Demo

schema S:
  object Person
  object Team
  object Context
  relation WorksFor(employee: Person, employer: Team, ctx: Context)

theory SRules on S:
  constraint key WorksFor(employee, employer, ctx)

instance I of S:
  Person = {Alice}
  Team = {Ops}
  Context = {Prod}
  WorksFor = {(employee=Alice, employer=Ops, ctx=Prod)}
"#;
        let fragment_json = serde_json::to_string(&serde_json::json!({
            "boxes": [
                {"box_id": "employee", "object_type": "Person"},
                {"box_id": "team", "object_type": "Team"},
                {"box_id": "ctx", "object_type": "Context"}
            ],
            "relation_boxes": [
                {
                    "box_id": "works_for_fact",
                    "relation": "WorksFor",
                    "role_bindings": [
                        {"role": "employee", "target_box": "employee"},
                        {"role": "ctx", "target_box": "ctx"}
                    ]
                }
            ],
            "aspects": [],
            "path_equations": []
        }))
        .expect("serialize fragment json");

        let first_report =
            discover_check_olog_report_from_inputs(axi_text, Some("S"), &fragment_json, None)
                .expect("initial check olog report");
        let handle_id = first_report
            .checked_olog
            .refinement_candidates
            .first()
            .map(|candidate| candidate.handle.id.clone())
            .expect("expected refinement handle");

        let report = discover_check_olog_report_from_inputs(
            axi_text,
            Some("S"),
            &fragment_json,
            Some(handle_id.as_str()),
        )
        .expect("applied check olog report");

        assert_eq!(report.version, "axiograph_discover_check_olog_v1");
        assert!(report.checked_olog.ok);
        assert!(report
            .applied_refinement
            .as_ref()
            .is_some_and(|applied| applied.handle.id == handle_id));
        let rel_box = report
            .applied_refinement
            .as_ref()
            .expect("applied refinement")
            .refined_fragment
            .relation_boxes
            .iter()
            .find(|relation_box| relation_box.box_id == "works_for_fact")
            .expect("refined relation box");
        assert!(rel_box
            .role_bindings
            .iter()
            .any(|binding| binding.role == "employer" && binding.target_box == "team"));
    }

    #[test]
    fn discover_transport_preview_from_inputs_can_apply_handle() {
        let axi_text = r#"
module Plant

schema Plant:
  object PlantAsset
  object Pump
  object Compressor
  object Context
  relation installed_at(asset: PlantAsset, site: PlantAsset, ctx: Context)
  subtype Pump < PlantAsset
  subtype Compressor < PlantAsset

theory PlantTransport on Plant:
  constraint key installed_at(asset, site, ctx)
"#;
        let morphism_json = serde_json::to_string(&serde_json::json!({
            "source_schema": "Plant",
            "target_schema": "Ops",
            "objects": [
                {"source_object": "PlantAsset", "target_object": "Equipment"},
                {"source_object": "Pump", "target_object": "Equipment"},
                {"source_object": "Compressor", "target_object": "Equipment"}
            ],
            "arrows": [
                {"source_arrow": "installed_at", "target_path": ["owned_by", "located_at"]}
            ]
        }))
        .expect("serialize morphism json");

        let first_preview =
            discover_transport_preview_from_inputs(axi_text, Some("Plant"), &morphism_json, None)
                .expect("initial transport preview");
        let handle_id = first_preview
            .refinement_candidates
            .first()
            .map(|candidate| candidate.handle.id.clone())
            .expect("expected migration refinement handle");

        let preview = discover_transport_preview_from_inputs(
            axi_text,
            Some("Plant"),
            &morphism_json,
            Some(handle_id.as_str()),
        )
        .expect("applied transport preview");

        assert_eq!(preview.kind, "migration_preview");
        assert!(preview.ok);
        assert!(preview.residual_obligations.is_empty());
        assert!(preview.refinement_candidates.is_empty());
        assert!(preview
            .typed_change
            .primitives
            .iter()
            .any(|primitive| matches!(
                primitive,
                crate::evolution_preview::EvolutionPrimitiveV1::TransportAlongSchemaMorphism {
                    source_schema,
                    target_schema,
                    ..
                } if source_schema == "Plant" && target_schema == "Ops"
            )));
    }

    #[test]
    fn discover_route_preview_command_parses_nested_subcommand() {
        let cli = Cli::try_parse_from([
            "axiograph",
            "discover",
            "route-preview",
            "/tmp/demo.axi",
            "--request",
            "/tmp/route.json",
            "--out",
            "/tmp/route_preview.json",
        ])
        .expect("parse discover route-preview");

        match cli.command {
            Commands::Discover {
                command: DiscoverCommands::RoutePreview(args),
            } => {
                assert_eq!(args.input, PathBuf::from("/tmp/demo.axi"));
                assert_eq!(args.request, PathBuf::from("/tmp/route.json"));
                assert_eq!(args.out, Some(PathBuf::from("/tmp/route_preview.json")));
            }
            _ => panic!("unexpected command parse result"),
        }
    }

    #[test]
    fn discover_context_report_command_parses_nested_subcommand() {
        let cli = Cli::try_parse_from([
            "axiograph",
            "discover",
            "context-report",
            "/tmp/demo.axi",
            "--request",
            "/tmp/context.json",
            "--out",
            "/tmp/context_report.json",
        ])
        .expect("parse discover context-report");

        match cli.command {
            Commands::Discover {
                command: DiscoverCommands::ContextReport(args),
            } => {
                assert_eq!(args.input, PathBuf::from("/tmp/demo.axi"));
                assert_eq!(args.request, PathBuf::from("/tmp/context.json"));
                assert_eq!(args.out, Some(PathBuf::from("/tmp/context_report.json")));
            }
            _ => panic!("unexpected command parse result"),
        }
    }

    #[test]
    fn discover_behavior_case_command_parses_nested_subcommand() {
        let cli = Cli::try_parse_from([
            "axiograph",
            "discover",
            "behavior-case",
            "/tmp/demo.axi",
            "--request",
            "/tmp/behavior_case.json",
            "--out",
            "/tmp/behavior_case_report.json",
        ])
        .expect("parse discover behavior-case");

        match cli.command {
            Commands::Discover {
                command: DiscoverCommands::BehaviorCase(args),
            } => {
                assert_eq!(args.input, PathBuf::from("/tmp/demo.axi"));
                assert_eq!(args.request, PathBuf::from("/tmp/behavior_case.json"));
                assert_eq!(
                    args.out,
                    Some(PathBuf::from("/tmp/behavior_case_report.json"))
                );
            }
            _ => panic!("unexpected command parse result"),
        }
    }

    #[test]
    fn authoring_workspace_command_parses_unified_request() {
        let cli = Cli::try_parse_from([
            "axiograph",
            "authoring",
            "workspace",
            "--workspace",
            "/tmp/workspace",
            "--request",
            "/tmp/authoring_request.json",
            "--out",
            "/tmp/authoring_report.json",
        ])
        .expect("parse authoring workspace request");

        match cli.command {
            Commands::Authoring {
                command:
                    AuthoringCommands::Workspace {
                        workspace,
                        request,
                        out,
                    },
            } => {
                assert_eq!(workspace, PathBuf::from("/tmp/workspace"));
                assert_eq!(request, PathBuf::from("/tmp/authoring_request.json"));
                assert_eq!(out, Some(PathBuf::from("/tmp/authoring_report.json")));
            }
            _ => panic!("unexpected command parse result"),
        }
    }

    #[test]
    fn discover_coverage_query_command_accepts_direct_terms() {
        let cli = Cli::try_parse_from([
            "axiograph",
            "discover",
            "coverage-query",
            "/tmp/domain.axi",
            "--term",
            "shipment eligibility",
            "--relation",
            "OrderEligibleForShipment",
            "--cq-name",
            "accepted_order_is_shipment_eligible",
            "--code-ref",
            "workers/shipping/src/eligibility.rs",
            "--surface-hint",
            "shipping",
            "--max-matches",
            "8",
            "--out",
            "/tmp/coverage_query.json",
        ])
        .expect("parse discover coverage-query direct flags");

        match cli.command {
            Commands::Discover {
                command: DiscoverCommands::CoverageQuery(args),
            } => {
                assert_eq!(args.input, PathBuf::from("/tmp/domain.axi"));
                assert_eq!(args.query, None);
                assert_eq!(args.terms, vec!["shipment eligibility"]);
                assert_eq!(args.relation_names, vec!["OrderEligibleForShipment"]);
                assert_eq!(args.cq_names, vec!["accepted_order_is_shipment_eligible"]);
                assert_eq!(args.code_refs, vec!["workers/shipping/src/eligibility.rs"]);
                assert_eq!(args.surface_hints, vec!["shipping"]);
                assert_eq!(args.max_matches, Some(8));
                assert_eq!(args.out, Some(PathBuf::from("/tmp/coverage_query.json")));
            }
            _ => panic!("unexpected command parse result"),
        }
    }

    #[test]
    fn discover_transport_preview_command_parses_nested_subcommand() {
        let cli = Cli::try_parse_from([
            "axiograph",
            "discover",
            "transport-preview",
            "/tmp/demo.axi",
            "--morphism",
            "/tmp/morphism.json",
            "--schema",
            "Plant",
            "--apply-refinement-handle-id",
            "migration_refine_v2:demo",
            "--out",
            "/tmp/preview.json",
        ])
        .expect("parse discover transport-preview");

        match cli.command {
            Commands::Discover {
                command: DiscoverCommands::TransportPreview(args),
            } => {
                assert_eq!(args.input, PathBuf::from("/tmp/demo.axi"));
                assert_eq!(args.morphism, PathBuf::from("/tmp/morphism.json"));
                assert_eq!(args.schema.as_deref(), Some("Plant"));
                assert_eq!(
                    args.apply_refinement_handle_id.as_deref(),
                    Some("migration_refine_v2:demo")
                );
                assert_eq!(args.out, Some(PathBuf::from("/tmp/preview.json")));
            }
            _ => panic!("unexpected command parse result"),
        }
    }

    #[test]
    fn discover_theory_graph_command_parses_nested_subcommand() {
        let cli = Cli::try_parse_from([
            "axiograph",
            "discover",
            "theory-graph",
            "/tmp/family.axi",
            "--theory",
            "FamRules",
            "--out",
            "/tmp/theory_graph.json",
        ])
        .expect("parse discover theory-graph");

        match cli.command {
            Commands::Discover {
                command: DiscoverCommands::TheoryGraph(args),
            } => {
                assert_eq!(args.input, PathBuf::from("/tmp/family.axi"));
                assert_eq!(args.theory.as_deref(), Some("FamRules"));
                assert_eq!(args.out, Some(PathBuf::from("/tmp/theory_graph.json")));
            }
            _ => panic!("unexpected command parse result"),
        }
    }

    #[test]
    fn check_theory_command_parses_nested_subcommand() {
        let cli = Cli::try_parse_from([
            "axiograph",
            "check",
            "theory",
            "/tmp/family.axi",
            "--theory",
            "FamRules",
            "--closure-tier",
            "evidence_weighted",
            "--world-id",
            "review:family",
            "--evidence-threshold-ppm",
            "700000",
            "--weighted-evidence",
            "--evidence-weight",
            "rule:family=250000",
            "--json",
        ])
        .expect("parse check theory");

        match cli.command {
            Commands::Check {
                command: CheckCommands::Theory(args),
            } => {
                assert_eq!(args.input, PathBuf::from("/tmp/family.axi"));
                assert_eq!(args.theory.as_deref(), Some("FamRules"));
                assert_eq!(args.closure_tier, "evidence_weighted");
                assert_eq!(args.world_id.as_deref(), Some("review:family"));
                assert_eq!(args.evidence_threshold_ppm, Some(700_000));
                assert!(args.weighted_evidence);
                assert_eq!(args.evidence_weights, vec!["rule:family=250000"]);
                assert!(args.json);
            }
            _ => panic!("unexpected command parse result"),
        }
    }

    #[test]
    fn discover_theory_check_command_parses_nested_subcommand() {
        let cli = Cli::try_parse_from([
            "axiograph",
            "discover",
            "theory-check",
            "/tmp/family.axi",
            "--theory",
            "FamRules",
            "--closure-tier",
            "global_indexed",
            "--included-ref",
            "refs/heads/main",
            "--included-slice",
            "slice:billing",
            "--included-import",
            "import:erp",
            "--undeclared-import",
            "import:rogue",
            "--out",
            "/tmp/theory_check.json",
        ])
        .expect("parse discover theory-check");

        match cli.command {
            Commands::Discover {
                command: DiscoverCommands::TheoryCheck(args),
            } => {
                assert_eq!(args.input, PathBuf::from("/tmp/family.axi"));
                assert_eq!(args.theory.as_deref(), Some("FamRules"));
                assert_eq!(args.closure_tier, "global_indexed");
                assert_eq!(args.included_refs, vec!["refs/heads/main"]);
                assert_eq!(args.included_slices, vec!["slice:billing"]);
                assert_eq!(args.included_imports, vec!["import:erp"]);
                assert_eq!(args.undeclared_imports, vec!["import:rogue"]);
                assert_eq!(args.out, Some(PathBuf::from("/tmp/theory_check.json")));
            }
            _ => panic!("unexpected command parse result"),
        }
    }

    #[test]
    fn discover_theory_graph_report_exposes_runtime_obligation_graphs() {
        let family_axi = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .join("examples/Family.axi");
        let axi_text = crate::security::read_utf8_file_bounded(
            &family_axi,
            crate::security::MAX_TEXT_INPUT_BYTES,
            "CLI input",
        )
        .expect("read Family.axi");
        let report = discover_theory_graph_report_from_axi_text(&axi_text, Some("FamRules"))
            .expect("build theory graph report");

        assert_eq!(report.version, "discover_theory_graph_report_v1");
        assert_eq!(report.graphs.len(), 1);
        let graph = &report.graphs[0];
        assert!(!graph.nodes.is_empty());
        assert!(!graph.edges.is_empty());
        assert!(graph.nodes.iter().any(|node| {
            node.kind == axiograph_pathdb::kernel_ir::TheoryObligationGraphNodeKindV1::Obligation
                && node.label.contains("Parent")
        }));
        assert!(graph.edges.iter().any(|edge| {
            edge.kind
                == axiograph_pathdb::kernel_ir::TheoryObligationGraphEdgeKindV1::SubjectSupportsObligation
        }));
        assert_eq!(
            graph.completeness_claim,
            "use RuntimeTheoryCheckReportV1 for scoped runtime completeness claims"
        );
    }
}
