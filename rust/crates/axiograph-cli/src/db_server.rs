//! Axiograph DB server (read-only replicas + optional write master).
//!
//! This module provides a small HTTP server that keeps a PathDB snapshot loaded
//! in memory for low-latency queries and exploration tooling.
//!
//! Trust boundary
//! -------------
//! The server is an **untrusted runtime surface**:
//! - it can run queries and (optionally) mutate the snapshot store,
//! - but it is not a trusted checker.
//!
//! The trusted correctness boundary is still:
//! - Rust emits certificates, and
//! - Lean verifies them against the formal semantics.
//!
//! The server is meant to be a practical deployment wrapper around the
//! snapshot-store model documented in `docs/howto/SNAPSHOT_STORE.md`:
//! - canonical accepted `.axi` modules are stored in an append-only store,
//! - PathDB `.axpd` snapshots are derived and rebuildable,
//! - and extension-layer overlays live in the PathDB WAL.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::header::{AUTHORIZATION, CONTENT_TYPE};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use url::form_urlencoded;

use axiograph_pathdb::axi_semantics::MetaPlaneIndex;
use axiograph_pathdb::PathDB;

use crate::accepted_plane::{AcceptedPlaneEventV1, AcceptedPlaneSnapshotV1};
use crate::llm::{GeneratedQuery, LlmBackend, LlmState, ToolLoopOptions};
use crate::pathdb_wal::{PathDbSnapshotV1, PathDbWalEventV1};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServerRole {
    Standalone,
    Master,
    Replica,
}

impl ServerRole {
    fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "standalone" => Ok(Self::Standalone),
            "master" => Ok(Self::Master),
            "replica" => Ok(Self::Replica),
            other => Err(anyhow!(
                "unknown --role `{}` (expected standalone|master|replica)",
                other
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SnapshotSource {
    Axpd(PathBuf),
    Store {
        dir: PathBuf,
        layer: String,
        snapshot: String,
    },
}

#[derive(Debug, Clone)]
struct ServerConfig {
    listen: SocketAddr,
    role: ServerRole,
    source: SnapshotSource,
    watch_head: bool,
    poll_interval: Duration,
    admin_token: Option<String>,
    ready_file: Option<PathBuf>,
    cert_verify: CertVerifyConfig,
    llm: LlmState,
}

#[derive(Debug, Clone)]
struct CertVerifyConfig {
    /// Optional path to `axiograph_verify` (Lean checker executable).
    ///
    /// If not set, we attempt a few best-effort locations.
    verifier_bin: Option<PathBuf>,
    /// Timeout for invoking the verifier (None = no timeout).
    timeout: Option<Duration>,
}

#[derive(Clone)]
struct LoadedSnapshot {
    /// Stable key for caching (digest of the `.axpd` bytes, or store snapshot id).
    snapshot_key: String,
    /// Human-facing identifier describing what we loaded.
    snapshot_label: String,
    /// For store-based loads, the resolved accepted-plane snapshot id.
    accepted_snapshot_id: Option<String>,
    /// For store-based loads, the resolved PathDB WAL snapshot id.
    pathdb_snapshot_id: Option<String>,
    loaded_at_unix_secs: u64,
    entities: usize,
    relations: usize,
    db: Arc<PathDB>,
    meta: Option<MetaPlaneIndex>,
    embeddings: Option<Arc<crate::embeddings::ResolvedEmbeddingsIndexV1>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct QueryCacheKey {
    snapshot: String,
    query_ir: String,
}

#[derive(Default)]
struct QueryPlanCache {
    entries: HashMap<QueryCacheKey, Arc<Mutex<crate::axql::PreparedAxqlQueryExpr>>>,
    lru: VecDeque<QueryCacheKey>,
}

impl QueryPlanCache {
    const MAX_ENTRIES: usize = 64;

    fn clear(&mut self) {
        self.entries.clear();
        self.lru.clear();
    }

    fn touch(&mut self, key: &QueryCacheKey) {
        if let Some(pos) = self.lru.iter().position(|k| k == key) {
            self.lru.remove(pos);
        }
        self.lru.push_back(key.clone());
    }

    fn get(&mut self, key: &QueryCacheKey) -> Option<Arc<Mutex<crate::axql::PreparedAxqlQueryExpr>>> {
        let value = self.entries.get(key).cloned()?;
        self.touch(key);
        Some(value)
    }

    fn insert(&mut self, key: QueryCacheKey, value: Arc<Mutex<crate::axql::PreparedAxqlQueryExpr>>) {
        self.entries.insert(key.clone(), value);
        self.touch(&key);

        while self.lru.len() > Self::MAX_ENTRIES {
            if let Some(oldest) = self.lru.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }
}

struct ServerState {
    config: ServerConfig,
    loaded: RwLock<LoadedSnapshot>,
    query_cache: Mutex<QueryPlanCache>,
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn now_unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn resolve_verifier_bin(config: &ServerConfig) -> Option<PathBuf> {
    if let Some(p) = config.cert_verify.verifier_bin.as_ref() {
        return Some(p.clone());
    }
    if let Ok(p) = std::env::var("AXIOGRAPH_VERIFY_BIN") {
        let p = p.trim();
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }

    // Prefer a verifier binary colocated next to the running server binary.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("axiograph_verify");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    // Dev fallback: repo-relative path (useful when running `cargo run` from repo root).
    let dev = PathBuf::from("lean")
        .join(".lake")
        .join("build")
        .join("bin")
        .join("axiograph_verify");
    if dev.exists() {
        return Some(dev);
    }

    None
}

fn export_pathdb_anchor_axi(db: &PathDB) -> Result<(String, String)> {
    let axi = axiograph_pathdb::axi_export::export_pathdb_to_axi_v1(db)?;
    let digest = axiograph_dsl::digest::axi_digest_v1(&axi);
    Ok((digest, axi))
}

fn write_temp_file_unique(suffix: &str, contents: &str) -> Result<PathBuf> {
    let mut path = std::env::temp_dir();
    let ts = now_unix_nanos();
    let pid = std::process::id();
    path.push(format!("axiograph_db_server_{pid}_{ts}_{suffix}"));
    std::fs::write(&path, contents)?;
    Ok(path)
}

fn run_command_output_with_timeout(
    mut cmd: Command,
    timeout: Option<Duration>,
) -> Result<std::process::Output> {
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| anyhow!("failed to spawn verifier: {e}"))?;

    if let Some(timeout) = timeout {
        let start = Instant::now();
        loop {
            if let Some(_status) = child
                .try_wait()
                .map_err(|e| anyhow!("failed to poll verifier process: {e}"))?
            {
                return child
                    .wait_with_output()
                    .map_err(|e| anyhow!("failed to collect verifier output: {e}"));
            }
            if start.elapsed() > timeout {
                let _ = child.kill();
                return Err(anyhow!("verifier timed out after {}s", timeout.as_secs()));
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    child
        .wait_with_output()
        .map_err(|e| anyhow!("failed to collect verifier output: {e}"))
}

fn verify_certificate_with_lean(
    config: &ServerConfig,
    anchor_axi: &str,
    certificate_json: &str,
) -> Result<(bool, String)> {
    let Some(verifier) = resolve_verifier_bin(config) else {
        return Err(anyhow!(
            "Lean verifier not configured (set --verify-bin or AXIOGRAPH_VERIFY_BIN, or build with `make lean-exe`)"
        ));
    };

    let anchor_path = write_temp_file_unique("anchor.axi", anchor_axi)?;
    let cert_path = write_temp_file_unique("cert.json", certificate_json)?;

    let timeout = config.cert_verify.timeout;
    let mut cmd = Command::new(&verifier);
    cmd.arg(&anchor_path).arg(&cert_path);
    let output = run_command_output_with_timeout(cmd, timeout);

    let _ = std::fs::remove_file(&anchor_path);
    let _ = std::fs::remove_file(&cert_path);

    let output = output?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    Ok((output.status.success(), combined.trim().to_string()))
}

pub(crate) fn cmd_db_serve(args: crate::DbServeArgs) -> Result<()> {
    let role = ServerRole::parse(&args.role)?;
    let poll_interval = Duration::from_secs(args.poll_interval_secs.max(1));

    if (args.llm_mock as usize)
        + (args.llm_ollama as usize)
        + (args.llm_openai as usize)
        + (args.llm_anthropic as usize)
        + (args.llm_plugin.is_some() as usize)
        > 1
    {
        return Err(anyhow!(
            "db serve: choose at most one LLM backend: `--llm-mock`, `--llm-ollama`, `--llm-openai`, `--llm-anthropic`, or `--llm-plugin ...`"
        ));
    }

    let mut llm = LlmState::default();
    if args.llm_mock {
        llm.backend = LlmBackend::Mock;
        llm.model = Some("mock".to_string());
    } else if args.llm_ollama {
        #[cfg(feature = "llm-ollama")]
        {
            let host = args
                .llm_ollama_host
                .clone()
                .unwrap_or_else(crate::llm::default_ollama_host);
            llm.backend = LlmBackend::Ollama { host };
            let model = args.llm_model.clone().ok_or_else(|| {
                anyhow!("db serve: `--llm-ollama` requires `--llm-model <model>`")
            })?;
            llm.model = Some(model);
        }
        #[cfg(not(feature = "llm-ollama"))]
        {
            return Err(anyhow!(
                "db serve: ollama support not compiled (enable `axiograph-cli` feature `llm-ollama`)"
            ));
        }
    } else if args.llm_openai {
        #[cfg(feature = "llm-openai")]
        {
            let key =
                std::env::var(crate::llm::OPENAI_API_KEY_ENV).unwrap_or_default();
            if key.trim().is_empty() {
                return Err(anyhow!(
                    "db serve: openai backend requires {}",
                    crate::llm::OPENAI_API_KEY_ENV
                ));
            }
            llm.backend = LlmBackend::OpenAI {
                base_url: args
                    .llm_openai_base_url
                    .clone()
                    .unwrap_or_else(crate::llm::default_openai_base_url),
            };
            let model = args.llm_model.clone().or_else(|| {
                let env =
                    std::env::var(crate::llm::OPENAI_MODEL_ENV).unwrap_or_default();
                let env = env.trim().to_string();
                if env.is_empty() { None } else { Some(env) }
            });
            let model = model.ok_or_else(|| {
                anyhow!(
                    "db serve: `--llm-openai` requires `--llm-model <model>` (or set {})",
                    crate::llm::OPENAI_MODEL_ENV
                )
            })?;
            llm.model = Some(model);
        }
        #[cfg(not(feature = "llm-openai"))]
        {
            return Err(anyhow!(
                "db serve: openai support not compiled (enable `axiograph-cli` feature `llm-openai`)"
            ));
        }
    } else if args.llm_anthropic {
        #[cfg(feature = "llm-anthropic")]
        {
            let key =
                std::env::var(crate::llm::ANTHROPIC_API_KEY_ENV).unwrap_or_default();
            if key.trim().is_empty() {
                return Err(anyhow!(
                    "db serve: anthropic backend requires {}",
                    crate::llm::ANTHROPIC_API_KEY_ENV
                ));
            }
            llm.backend = LlmBackend::Anthropic {
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
                    "db serve: `--llm-anthropic` requires `--llm-model <model>` (or set {})",
                    crate::llm::ANTHROPIC_MODEL_ENV
                )
            })?;
            llm.model = Some(model);
        }
        #[cfg(not(feature = "llm-anthropic"))]
        {
            return Err(anyhow!(
                "db serve: anthropic support not compiled (enable `axiograph-cli` feature `llm-anthropic`)"
            ));
        }
    } else if let Some(plugin) = args.llm_plugin.as_ref() {
        llm.backend = LlmBackend::Command {
            program: plugin.clone(),
            args: args.llm_plugin_arg.clone(),
        };
        llm.model = args.llm_model.clone();
    }

    let source = match (&args.axpd, &args.dir) {
        (Some(_), Some(_)) => {
            return Err(anyhow!("db serve: pass only one of --axpd or --dir"));
        }
        (Some(axpd), None) => SnapshotSource::Axpd(axpd.clone()),
        (None, Some(dir)) => SnapshotSource::Store {
            dir: dir.clone(),
            layer: args.layer.clone(),
            snapshot: args.snapshot.clone(),
        },
        (None, None) => {
            return Err(anyhow!("db serve: pass either --axpd <file.axpd> or --dir <accepted_plane_dir>"));
        }
    };

    let config = ServerConfig {
        listen: args.listen,
        role,
        source,
        watch_head: args.watch_head || role == ServerRole::Replica,
        poll_interval,
        admin_token: args.admin_token.clone(),
        ready_file: args.ready_file.clone(),
        cert_verify: CertVerifyConfig {
            verifier_bin: args.verify_bin.clone(),
            timeout: if args.verify_timeout_secs == 0 {
                None
            } else {
                Some(Duration::from_secs(args.verify_timeout_secs))
            },
        },
        llm,
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| anyhow!("failed to initialize tokio runtime: {e}"))?;

    rt.block_on(async move { serve_async(config).await })
}

async fn serve_async(config: ServerConfig) -> Result<()> {
    let initial = tokio::task::spawn_blocking({
        let config = config.clone();
        move || load_snapshot(&config)
    })
    .await
    .map_err(|e| anyhow!("db serve: failed to join loader task: {e}"))??;

    let state = Arc::new(ServerState {
        config: config.clone(),
        loaded: RwLock::new(initial),
        query_cache: Mutex::new(QueryPlanCache::default()),
    });

    if config.watch_head {
        let state = state.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(config.poll_interval);
            loop {
                ticker.tick().await;
                if let Err(e) = reload_if_head_changed(&state).await {
                    eprintln!("db serve: watch-head reload failed: {e}");
                }
            }
        });
    }

    let listener = TcpListener::bind(config.listen)
        .await
        .map_err(|e| anyhow!("db serve: failed to bind {}: {e}", config.listen))?;
    let bound = listener
        .local_addr()
        .map_err(|e| anyhow!("db serve: failed to read bound addr: {e}"))?;

    eprintln!(
        "db serve: listening on http://{} (role={:?})",
        bound,
        config.role
    );
    if let Some(path) = config.ready_file.as_ref() {
        let payload = serde_json::json!({
            "version": "axiograph_db_server_ready_v1",
            "addr": bound.to_string(),
            "pid": std::process::id(),
        });
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(path, serde_json::to_string_pretty(&payload).unwrap_or_default()).ok();
    }

    loop {
        let (stream, _peer) = listener
            .accept()
            .await
            .map_err(|e| anyhow!("db serve: accept failed: {e}"))?;
        let io = TokioIo::new(stream);
        let state = state.clone();

        tokio::spawn(async move {
            let service = service_fn(move |req| handle_request(req, state.clone()));
            if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
                eprintln!("db serve: connection error: {e}");
            }
        });
    }
}

async fn handle_request(
    req: Request<Incoming>,
    state: Arc<ServerState>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    let resp = match (method, path.as_str()) {
        (Method::GET, "/healthz") => text_response(StatusCode::OK, "ok\n"),
        (Method::GET, "/status") => match status_payload(&state) {
            Ok(v) => json_response(StatusCode::OK, &v),
            Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        },
        (Method::GET, "/snapshots") => match snapshots_payload(&state, req.uri().query()) {
            Ok(v) => json_response(StatusCode::OK, &v),
            Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
        },
        (Method::GET, "/anchor.axi") => match handle_anchor_get(&state, req.uri().query()).await {
            Ok(r) => r,
            Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
        },
        (Method::GET, "/entity/describe") => match handle_entity_describe_get(&state, req.uri().query()).await {
            Ok(v) => json_response(StatusCode::OK, &v),
            Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
        },
        (Method::GET, "/docchunk/get") => match handle_docchunk_get(&state, req.uri().query()).await {
            Ok(v) => json_response(StatusCode::OK, &v),
            Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
        },
        (Method::GET, "/contexts") => match handle_contexts_get(&state).await {
            Ok(v) => json_response(StatusCode::OK, &v),
            Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
        },
        (Method::GET, "/viz") => match handle_viz_get(&state, req.uri().query()).await {
            Ok(r) => r,
            Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
        },
        (Method::GET, "/viz.json") => match handle_viz_get_as(&state, req.uri().query(), "json").await
        {
            Ok(r) => r,
            Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
        },
        (Method::GET, "/viz.dot") => match handle_viz_get_as(&state, req.uri().query(), "dot").await
        {
            Ok(r) => r,
            Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
        },
        (Method::POST, "/query") => {
            let body = req
                .into_body()
                .collect()
                .await?
                .to_bytes()
                .to_vec();
            match handle_query(&state, &body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/cert/reachability") => {
            let body = req
                .into_body()
                .collect()
                .await?
                .to_bytes()
                .to_vec();
            match handle_reachability_cert(&state, &body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/llm/to_query") => {
            let body = req
                .into_body()
                .collect()
                .await?
                .to_bytes()
                .to_vec();
            match handle_llm_to_query(&state, &body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/llm/agent") => {
            let auth_header = req
                .headers()
                .get(AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            let body = req
                .into_body()
                .collect()
                .await?
                .to_bytes()
                .to_vec();

            let parsed: LlmAgentRequestV1 = match serde_json::from_slice(&body) {
                Ok(v) => v,
                Err(e) => {
                    return Ok(json_error(
                        StatusCode::BAD_REQUEST,
                        &format!("failed to parse llm/agent request JSON: {e}"),
                    ));
                }
            };
            if parsed.auto_commit {
                if let Err(resp) = require_admin_auth_header(auth_header.as_deref(), state.as_ref()) {
                    return Ok(resp);
                }
            }
            match handle_llm_agent(&state, parsed).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/discover/draft-axi") => {
            let body = req
                .into_body()
                .collect()
                .await?
                .to_bytes()
                .to_vec();
            match handle_discover_draft_axi(&state, &body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/proposals/relation") => {
            let body = req
                .into_body()
                .collect()
                .await?
                .to_bytes()
                .to_vec();
            match handle_proposals_relation(&state, &body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/proposals/relations") => {
            let body = req
                .into_body()
                .collect()
                .await?
                .to_bytes()
                .to_vec();
            match handle_proposals_relations(&state, &body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/viz") => {
            let body = req
                .into_body()
                .collect()
                .await?
                .to_bytes()
                .to_vec();
            match handle_viz_post(&state, &body).await {
                Ok(r) => r,
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/admin/reload") => {
            if let Err(e) = require_admin(&req, &state) {
                return Ok(e);
            }
            match reload_now(&state).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
            }
        }
        (Method::POST, "/admin/accept/promote") => {
            if let Err(e) = require_admin(&req, &state) {
                return Ok(e);
            }
            let body = req
                .into_body()
                .collect()
                .await?
                .to_bytes()
                .to_vec();
            match handle_promote(&state, &body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/admin/accept/pathdb-commit") => {
            if let Err(e) = require_admin(&req, &state) {
                return Ok(e);
            }
            let body = req
                .into_body()
                .collect()
                .await?
                .to_bytes()
                .to_vec();
            match handle_pathdb_commit(&state, &body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        _ => json_error(StatusCode::NOT_FOUND, "not found"),
    };

    Ok(resp)
}

fn text_response(status: StatusCode, body: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Full::new(Bytes::from(body.to_string())))
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::from_static(b"internal error"))))
}

fn json_response<T: Serialize>(status: StatusCode, value: &T) -> Response<Full<Bytes>> {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{\"error\":\"serialize\"}".to_vec());
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(body)))
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::from_static(b"{\"error\":\"internal\"}"))))
}

fn json_error(status: StatusCode, msg: &str) -> Response<Full<Bytes>> {
    let v = serde_json::json!({ "error": msg });
    json_response(status, &v)
}

fn require_admin(req: &Request<Incoming>, state: &ServerState) -> Result<(), Response<Full<Bytes>>> {
    if state.config.role != ServerRole::Master {
        return Err(json_error(
            StatusCode::FORBIDDEN,
            "admin endpoints require --role master",
        ));
    }

    let Some(expected) = state.config.admin_token.as_deref() else {
        return Ok(());
    };

    let Some(header) = req.headers().get(AUTHORIZATION).and_then(|v| v.to_str().ok()) else {
        return Err(json_error(
            StatusCode::UNAUTHORIZED,
            "missing Authorization: Bearer <token>",
        ));
    };

    let token = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))
        .unwrap_or("");
    if token != expected {
        return Err(json_error(StatusCode::UNAUTHORIZED, "invalid admin token"));
    }

    Ok(())
}

fn require_admin_auth_header(
    auth_header: Option<&str>,
    state: &ServerState,
) -> Result<(), Response<Full<Bytes>>> {
    if state.config.role != ServerRole::Master {
        return Err(json_error(
            StatusCode::FORBIDDEN,
            "admin endpoints require --role master",
        ));
    }

    let Some(expected) = state.config.admin_token.as_deref() else {
        return Ok(());
    };

    let Some(header) = auth_header else {
        return Err(json_error(
            StatusCode::UNAUTHORIZED,
            "missing Authorization: Bearer <token>",
        ));
    };

    let token = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))
        .unwrap_or("");
    if token != expected {
        return Err(json_error(StatusCode::UNAUTHORIZED, "invalid admin token"));
    }

    Ok(())
}

fn status_payload(state: &ServerState) -> Result<serde_json::Value> {
    let loaded = state
        .loaded
        .read()
        .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?;
    let llm_backend = match &state.config.llm.backend {
        LlmBackend::Disabled => "disabled".to_string(),
        LlmBackend::Mock => "mock".to_string(),
        #[cfg(feature = "llm-ollama")]
        LlmBackend::Ollama { host } => format!("ollama({host})"),
        #[cfg(feature = "llm-openai")]
        LlmBackend::OpenAI { base_url } => format!("openai({base_url})"),
        #[cfg(feature = "llm-anthropic")]
        LlmBackend::Anthropic { base_url } => format!("anthropic({base_url})"),
        LlmBackend::Command { program, .. } => format!("command({})", program.display()),
    };
    let verifier_bin = resolve_verifier_bin(&state.config);
    Ok(serde_json::json!({
        "version": "axiograph_db_server_status_v1",
        "role": format!("{:?}", state.config.role).to_ascii_lowercase(),
        "listen": state.config.listen.to_string(),
        "snapshot": {
            "snapshot_key": loaded.snapshot_key,
            "label": loaded.snapshot_label,
            "accepted_snapshot_id": loaded.accepted_snapshot_id,
            "pathdb_snapshot_id": loaded.pathdb_snapshot_id,
            "loaded_at_unix_secs": loaded.loaded_at_unix_secs,
            "entities": loaded.entities,
            "relations": loaded.relations,
        },
        "llm": {
            "enabled": !matches!(state.config.llm.backend, LlmBackend::Disabled),
            "backend": llm_backend,
            "model": state.config.llm.model.clone(),
            "status": state.config.llm.status_line(),
        },
        "certificates": {
            "lean_verifier_available": verifier_bin.is_some(),
            "lean_verifier_bin": verifier_bin.as_ref().map(|p| p.display().to_string()),
            "lean_verifier_timeout_secs": state.config.cert_verify.timeout.map(|d| d.as_secs()),
        },
    }))
}

fn read_jsonl_map_latest_message(path: &Path) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return out;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Try accepted-plane event first.
        if let Ok(ev) = serde_json::from_str::<AcceptedPlaneEventV1>(line) {
            if let Some(msg) = ev.message {
                out.insert(ev.snapshot_id, msg);
            }
            continue;
        }
        // Then PathDB WAL event.
        if let Ok(ev) = serde_json::from_str::<PathDbWalEventV1>(line) {
            if let Some(msg) = ev.message {
                out.insert(ev.snapshot_id, msg);
            }
            continue;
        }
    }
    out
}

fn snapshots_payload(state: &ServerState, query: Option<&str>) -> Result<serde_json::Value> {
    let SnapshotSource::Store { dir, layer, .. } = &state.config.source else {
        return Err(anyhow!(
            "/snapshots requires a store-backed server (`axiograph db serve --dir ...`)"
        ));
    };

    let mut want_layer = layer.trim().to_ascii_lowercase();
    let mut limit: usize = 50;

    let p = parse_query_params(query);
    if let Some(v) = p.get("layer") {
        want_layer = v.trim().to_ascii_lowercase();
    }
    if let Some(v) = p.get("limit") {
        if let Ok(n) = v.parse::<usize>() {
            limit = n.clamp(1, 500);
        }
    }

    if !matches!(want_layer.as_str(), "accepted" | "pathdb") {
        return Err(anyhow!(
            "unknown layer `{}` (expected accepted|pathdb)",
            want_layer
        ));
    }

    #[derive(Debug, Clone, Serialize)]
    struct SnapshotEntryV1 {
        snapshot_id: String,
        previous_snapshot_id: Option<String>,
        created_at_unix_secs: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        accepted_snapshot_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        modules_count: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        ops_count: Option<usize>,
    }

    let mut entries: Vec<SnapshotEntryV1> = Vec::new();
    if want_layer == "accepted" {
        let snapshots_dir = dir.join("snapshots");
        let messages = read_jsonl_map_latest_message(&dir.join("accepted_plane.log.jsonl"));

        let rd = std::fs::read_dir(&snapshots_dir).map_err(|e| {
            anyhow!(
                "failed to read accepted snapshots dir `{}`: {e}",
                snapshots_dir.display()
            )
        })?;
        for entry in rd {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            if path.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(snap) = serde_json::from_str::<AcceptedPlaneSnapshotV1>(&text) else {
                continue;
            };
            let msg = messages.get(&snap.snapshot_id).cloned();
            entries.push(SnapshotEntryV1 {
                snapshot_id: snap.snapshot_id,
                previous_snapshot_id: snap.previous_snapshot_id,
                created_at_unix_secs: snap.created_at_unix_secs,
                message: msg,
                accepted_snapshot_id: None,
                modules_count: Some(snap.modules.len()),
                ops_count: None,
            });
        }
    } else {
        let wal_dir = dir.join("pathdb");
        let snapshots_dir = wal_dir.join("snapshots");
        let messages = read_jsonl_map_latest_message(&wal_dir.join("pathdb_wal.log.jsonl"));

        let rd = std::fs::read_dir(&snapshots_dir).map_err(|e| {
            anyhow!(
                "failed to read pathdb snapshots dir `{}`: {e}",
                snapshots_dir.display()
            )
        })?;
        for entry in rd {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            if path.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(snap) = serde_json::from_str::<PathDbSnapshotV1>(&text) else {
                continue;
            };
            let msg = messages.get(&snap.snapshot_id).cloned();
            entries.push(SnapshotEntryV1 {
                snapshot_id: snap.snapshot_id,
                previous_snapshot_id: snap.previous_snapshot_id,
                created_at_unix_secs: snap.created_at_unix_secs,
                message: msg,
                accepted_snapshot_id: Some(snap.accepted_snapshot_id),
                modules_count: None,
                ops_count: Some(snap.ops.len()),
            });
        }
    }

    entries.sort_by(|a, b| b.created_at_unix_secs.cmp(&a.created_at_unix_secs));
    if entries.len() > limit {
        entries.truncate(limit);
    }

    Ok(serde_json::json!({
        "version": "axiograph_db_server_snapshots_v1",
        "layer": want_layer,
        "count": entries.len(),
        "snapshots": entries,
    }))
}

#[derive(Debug, Clone, Deserialize)]
struct QueryRequestV1 {
    query: String,
    #[serde(default)]
    lang: Option<String>,
    /// Include elaboration output (inferred types + notes + elaborated query text).
    #[serde(default)]
    show_elaboration: bool,
    /// Optional default contexts/worlds (applied only when the query text has no explicit `in ...`).
    ///
    /// Values may be numeric entity ids ("123") or context `name` values.
    #[serde(default)]
    contexts: Vec<String>,
    /// Emit a Lean-checkable certificate for this query result (if possible).
    ///
    /// Notes:
    /// - approximate atoms (`fts`, `contains`, `fuzzy`) are not certifiable,
    /// - multi-context scoping is execution-only for now.
    #[serde(default)]
    certify: bool,
    /// Verify the emitted certificate using the Lean checker (`axiograph_verify`).
    ///
    /// This implies `certify=true`.
    #[serde(default)]
    verify: bool,
    /// Include the snapshot anchor `.axi` (PathDBExportV1) in the response.
    ///
    /// This can be large; default is false.
    #[serde(default)]
    include_anchor: bool,
    /// Optional snapshot id override when running in store-backed mode.
    ///
    /// If set, the server will load and query that snapshot for this request
    /// (does not affect the currently loaded snapshot for other requests).
    #[serde(default)]
    snapshot: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct LlmToQueryRequestV1 {
    question: String,
    /// Optional snapshot id override when running in store-backed mode.
    #[serde(default)]
    snapshot: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct LlmAgentRequestV1 {
    question: String,
    /// Optional chat history (for conversational UI).
    ///
    /// This is **not trusted**: assistant messages are treated as context only.
    #[serde(default)]
    history: Vec<ChatMessageV1>,
    /// Optional default contexts/worlds for tool-loop queries.
    ///
    /// Values may be numeric entity ids ("123") or context `name` values.
    #[serde(default)]
    contexts: Vec<String>,
    #[serde(default)]
    max_steps: Option<usize>,
    #[serde(default)]
    max_rows: Option<usize>,
    /// If set, and the tool loop produced a validated proposals overlay, auto-commit it to the PathDB WAL.
    ///
    /// This requires:
    /// - `db serve --role master`
    /// - and (if configured) `Authorization: Bearer <token>`.
    #[serde(default)]
    auto_commit: bool,
    /// Optional accepted-plane snapshot id override for WAL commits.
    ///
    /// Default is the accepted snapshot backing the currently loaded PathDB snapshot.
    #[serde(default)]
    accepted_snapshot: Option<String>,
    /// Optional message to attach to the WAL commit (audit log).
    #[serde(default)]
    commit_message: Option<String>,
    /// Emit Lean-checkable certificates for `axql_run` steps executed by the tool-loop (best-effort).
    #[serde(default)]
    certify_queries: bool,
    /// Verify emitted query certificates using the Lean checker (`axiograph_verify`) (best-effort).
    ///
    /// This implies `certify_queries=true`.
    #[serde(default)]
    verify_queries: bool,
    /// Require that every executed `axql_run` step is accompanied by a certificate.
    ///
    /// If this is true and any query certificate fails to emit, the server will
    /// **refuse** to return an un-gated answer (it will attach a gate report and
    /// overwrite the final answer with a refusal message).
    #[serde(default)]
    require_query_certs: bool,
    /// Require that every emitted query certificate is verified by Lean.
    ///
    /// This implies `verify_queries=true` and `certify_queries=true`.
    #[serde(default)]
    require_verified_queries: bool,
    /// Include the snapshot anchor `.axi` in the response (can be large).
    #[serde(default)]
    include_anchor: bool,
    /// Optional snapshot id override when running in store-backed mode.
    #[serde(default)]
    snapshot: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ChatMessageV1 {
    role: String,   // "user" | "assistant" | "system" (best-effort)
    content: String,
}

#[derive(Debug, Clone, Deserialize)]
struct VizRequestV1 {
    /// html|json|dot
    #[serde(default)]
    format: Option<String>,
    /// data|meta|both
    #[serde(default)]
    plane: Option<String>,
    #[serde(default)]
    focus_name: Option<String>,
    #[serde(default)]
    focus_type: Option<String>,
    #[serde(default)]
    focus_id: Option<u32>,
    #[serde(default)]
    hops: Option<usize>,
    #[serde(default)]
    max_nodes: Option<usize>,
    #[serde(default)]
    max_edges: Option<usize>,
    /// out|in|both
    #[serde(default)]
    direction: Option<String>,
    #[serde(default)]
    include_equivalences: Option<bool>,
    #[serde(default)]
    typed_overlay: Option<bool>,
    /// Auto-refresh interval for HTML output (0 disables).
    #[serde(default)]
    refresh_secs: Option<u64>,
    /// Optional snapshot id override when running in store-backed mode.
    ///
    /// If set, the server will load that snapshot for this request (does not
    /// affect the currently loaded snapshot for other requests).
    #[serde(default)]
    snapshot: Option<String>,
}

fn parse_query_params(query: Option<&str>) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(q) = query else {
        return out;
    };
    for (k, v) in form_urlencoded::parse(q.as_bytes()) {
        out.insert(k.into_owned(), v.into_owned());
    }
    out
}

fn parse_bool(v: Option<&str>) -> Option<bool> {
    let s = v?.trim().to_ascii_lowercase();
    match s.as_str() {
        "1" | "true" | "t" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "f" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

fn inject_meta_refresh(mut html: String, refresh_secs: u64) -> String {
    if refresh_secs == 0 {
        return html;
    }
    let needle = "<meta charset=\"utf-8\"/>";
    if let Some(pos) = html.find(needle) {
        let insert_at = pos + needle.len();
        html.insert_str(
            insert_at,
            &format!("\n<meta http-equiv=\"refresh\" content=\"{}\"/>", refresh_secs),
        );
        return html;
    }
    // Fallback: insert into <head>.
    if let Some(pos) = html.find("<head>") {
        let insert_at = pos + "<head>".len();
        html.insert_str(
            insert_at,
            &format!(
                "\n<meta http-equiv=\"refresh\" content=\"{}\"/>",
                refresh_secs
            ),
        );
    }
    html
}

#[derive(Debug, Clone, Serialize)]
struct QueryResponseV1 {
    vars: Vec<String>,
    rows: Vec<BTreeMap<String, EntityViewV1>>,
    truncated: bool,
    elapsed_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    elaborated_query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inferred_types: Option<BTreeMap<String, Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    anchor_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    anchor_axi: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    certificate: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    certificate_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    certificate_verify_output: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct EntityViewV1 {
    id: u32,
    entity_type: Option<String>,
    name: Option<String>,
}

impl EntityViewV1 {
    fn from_id(db: &PathDB, id: u32) -> Self {
        let Some(view) = db.get_entity(id) else {
            return Self {
                id,
                entity_type: None,
                name: None,
            };
        };
        Self {
            id,
            entity_type: Some(view.entity_type),
            name: view.attrs.get("name").cloned(),
        }
    }
}

async fn handle_query(state: &Arc<ServerState>, body: &[u8]) -> Result<QueryResponseV1> {
    let req: QueryRequestV1 = serde_json::from_slice(body)
        .map_err(|e| anyhow!("failed to parse query request JSON: {e}"))?;
    let lang = req
        .lang
        .as_deref()
        .unwrap_or("axql")
        .trim()
        .to_ascii_lowercase();
    if lang != "axql" {
        return Err(anyhow!(
            "unsupported lang `{}` (only `axql` is supported by db serve for now)",
            lang
        ));
    }

    let query_text = req.query.clone();
    let show_elaboration = req.show_elaboration;
    let contexts_raw = req.contexts.clone();
    let want_cert = req.certify || req.verify;
    let want_verify = req.verify;
    let include_anchor = req.include_anchor;
    let snapshot_override = req.snapshot.clone();
    let state = state.clone();

    tokio::task::spawn_blocking(move || {
        let (db, meta, snapshot_key) = if let Some(snapshot) = snapshot_override.as_deref() {
            let SnapshotSource::Store { dir, layer, .. } = &state.config.source else {
                return Err(anyhow!(
                    "query snapshot override requires a store-backed server (`--dir ...`)"
                ));
            };
            let loaded = load_from_store(dir, layer, snapshot)?;
            (loaded.db, loaded.meta, loaded.snapshot_key)
        } else {
            let loaded = state
                .loaded
                .read()
                .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?;
            (loaded.db.clone(), loaded.meta.clone(), loaded.snapshot_key.clone())
        };

        let mut parsed = crate::axql::parse_axql_query(&query_text)?;
        if parsed.contexts.is_empty() && !contexts_raw.is_empty() {
            let mut contexts: Vec<crate::axql::AxqlContextSpec> = Vec::new();
            for c in contexts_raw {
                let c = c.trim();
                if c.is_empty() || c == "*" || c.eq_ignore_ascii_case("all") {
                    continue;
                }
                if let Ok(id) = c.parse::<u32>() {
                    contexts.push(crate::axql::AxqlContextSpec::EntityId(id));
                } else {
                    contexts.push(crate::axql::AxqlContextSpec::Name(c.to_string()));
                }
            }
            parsed.contexts = contexts;
        }
        let query_ir = crate::axql::axql_query_ir_digest_v1(&parsed);
        let cache_key = QueryCacheKey {
            snapshot: snapshot_key,
            query_ir,
        };

        let start = Instant::now();
        let prepared = {
            let mut cache = state
                .query_cache
                .lock()
                .map_err(|_| anyhow!("query cache lock poisoned"))?;
            if let Some(p) = cache.get(&cache_key) {
                p
            } else {
                let prepared =
                    crate::axql::prepare_axql_query_with_meta(&db, &parsed, meta.as_ref())?;
                let prepared = Arc::new(Mutex::new(prepared));
                cache.insert(cache_key.clone(), prepared.clone());
                prepared
            }
        };

        let mut prepared = prepared
            .lock()
            .map_err(|_| anyhow!("prepared query lock poisoned"))?;
        let elaborated_query = show_elaboration.then(|| prepared.elaborated_query_text());
        let elaboration = show_elaboration.then(|| prepared.elaboration_report().clone());
        let plan = show_elaboration.then(|| prepared.explain_plan_lines());
        let res = prepared.execute(&db)?;
        let elapsed_ms = start.elapsed().as_millis();

        let vars = res.selected_vars.clone();
        let mut rows: Vec<BTreeMap<String, EntityViewV1>> = Vec::new();
        for row in &res.rows {
            let mut out: BTreeMap<String, EntityViewV1> = BTreeMap::new();
            for (k, id) in row {
                out.insert(k.clone(), EntityViewV1::from_id(&db, *id));
            }
            rows.push(out);
        }

        let mut anchor_digest: Option<String> = None;
        let mut anchor_axi: Option<String> = None;
        let mut certificate: Option<serde_json::Value> = None;
        let mut certificate_verified: Option<bool> = None;
        let mut certificate_verify_output: Option<String> = None;

        if want_cert {
            let (digest, axi) = export_pathdb_anchor_axi(&db)?;
            anchor_digest = Some(digest.clone());
            if include_anchor {
                anchor_axi = Some(axi.clone());
            }

            let cert = crate::axql::certify_axql_query_with_meta(&db, &parsed, meta.as_ref())?
                .with_anchor(axiograph_pathdb::certificate::AxiAnchorV1 {
                    axi_digest_v1: digest,
                });
            let cert_json = serde_json::to_value(&cert)?;
            certificate = Some(cert_json);

            if want_verify {
                let cert_text = serde_json::to_string_pretty(&cert)?;
                let (ok, out) = verify_certificate_with_lean(&state.config, &axi, &cert_text)?;
                certificate_verified = Some(ok);
                certificate_verify_output = Some(out);
            }
        }

        Ok(QueryResponseV1 {
            vars,
            rows,
            truncated: res.truncated,
            elapsed_ms,
            elaborated_query,
            inferred_types: elaboration.as_ref().map(|e| e.inferred_types.clone()),
            notes: elaboration.as_ref().map(|e| e.notes.clone()),
            plan,
            anchor_digest,
            anchor_axi,
            certificate,
            certificate_verified,
            certificate_verify_output,
        })
    })
    .await
    .map_err(|e| anyhow!("query task join failed: {e}"))?
}

async fn handle_anchor_get(
    state: &Arc<ServerState>,
    query: Option<&str>,
) -> Result<Response<Full<Bytes>>> {
    let p = parse_query_params(query);
    let snapshot_override = p.get("snapshot").cloned();
    let state = state.clone();

    tokio::task::spawn_blocking(move || {
        let db = if let Some(snapshot) = snapshot_override.as_deref() {
            let SnapshotSource::Store { dir, layer, .. } = &state.config.source else {
                return Err(anyhow!(
                    "anchor snapshot override requires a store-backed server (`--dir ...`)"
                ));
            };
            let loaded = load_from_store(dir, layer, snapshot)?;
            loaded.db
        } else {
            let loaded = state
                .loaded
                .read()
                .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?;
            loaded.db.clone()
        };

        let (_digest, axi) = export_pathdb_anchor_axi(&db)?;
        Ok::<_, anyhow::Error>(
            Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(Full::new(Bytes::from(axi)))
                .unwrap_or_else(|_| Response::new(Full::new(Bytes::from_static(b"internal error")))),
        )
    })
    .await
    .map_err(|e| anyhow!("anchor task join failed: {e}"))?
}

#[derive(Debug, Clone, Deserialize)]
struct ReachabilityCertRequestV1 {
    start: u32,
    relation_ids: Vec<u32>,
    #[serde(default)]
    verify: bool,
    #[serde(default)]
    include_anchor: bool,
    /// Optional snapshot id override when running in store-backed mode.
    #[serde(default)]
    snapshot: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct CertResponseV1 {
    anchor_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    anchor_axi: Option<String>,
    certificate: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    certificate_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    certificate_verify_output: Option<String>,
}

async fn handle_reachability_cert(
    state: &Arc<ServerState>,
    body: &[u8],
) -> Result<CertResponseV1> {
    let req: ReachabilityCertRequestV1 = serde_json::from_slice(body)
        .map_err(|e| anyhow!("failed to parse reachability cert request JSON: {e}"))?;
    if req.relation_ids.is_empty() {
        return Err(anyhow!("reachability cert requires non-empty `relation_ids`"));
    }

    let snapshot_override = req.snapshot.clone();
    let verify = req.verify;
    let include_anchor = req.include_anchor;
    let state = state.clone();

    tokio::task::spawn_blocking(move || {
        let db = if let Some(snapshot) = snapshot_override.as_deref() {
            let SnapshotSource::Store { dir, layer, .. } = &state.config.source else {
                return Err(anyhow!(
                    "reachability snapshot override requires a store-backed server (`--dir ...`)"
                ));
            };
            let loaded = load_from_store(dir, layer, snapshot)?;
            loaded.db
        } else {
            let loaded = state
                .loaded
                .read()
                .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?;
            loaded.db.clone()
        };

        let (digest, axi) = export_pathdb_anchor_axi(&db)?;
        let proof = axiograph_pathdb::witness::reachability_proof_v2_from_relation_ids(
            &db,
            req.start,
            &req.relation_ids,
        )?
        .into_inner();

        let cert = axiograph_pathdb::certificate::CertificateV2::reachability(proof).with_anchor(
            axiograph_pathdb::certificate::AxiAnchorV1 {
                axi_digest_v1: digest.clone(),
            },
        );

        let cert_json = serde_json::to_value(&cert)?;
        let (verified, verify_out) = if verify {
            let cert_text = serde_json::to_string_pretty(&cert)?;
            let (ok, out) = verify_certificate_with_lean(&state.config, &axi, &cert_text)?;
            (Some(ok), Some(out))
        } else {
            (None, None)
        };

        Ok::<_, anyhow::Error>(CertResponseV1 {
            anchor_digest: digest,
            anchor_axi: include_anchor.then_some(axi),
            certificate: cert_json,
            certificate_verified: verified,
            certificate_verify_output: verify_out,
        })
    })
    .await
    .map_err(|e| anyhow!("reachability cert task join failed: {e}"))?
}

async fn handle_llm_to_query(
    state: &Arc<ServerState>,
    body: &[u8],
) -> Result<serde_json::Value> {
    let req: LlmToQueryRequestV1 = serde_json::from_slice(body)
        .map_err(|e| anyhow!("failed to parse llm/to_query request JSON: {e}"))?;

    if matches!(state.config.llm.backend, LlmBackend::Disabled) {
        return Err(anyhow!(
            "LLM is disabled for this server. Start with: `axiograph db serve ... --llm-ollama --llm-model <model>` (or `--llm-mock`)"
        ));
    }

    let question = req.question.clone();
    let snapshot_override = req.snapshot.clone();
    let state = state.clone();

    tokio::task::spawn_blocking(move || {
        let db = if let Some(snapshot) = snapshot_override.as_deref() {
            let SnapshotSource::Store { dir, layer, .. } = &state.config.source else {
                return Err(anyhow!(
                    "llm snapshot override requires a store-backed server (`--dir ...`)"
                ));
            };
            let loaded = load_from_store(dir, layer, snapshot)?;
            loaded.db
        } else {
            let loaded = state
                .loaded
                .read()
                .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?;
            loaded.db.clone()
        };

        let generated = state.config.llm.generate_query(&db, &question)?;
        Ok::<_, anyhow::Error>(match generated {
            GeneratedQuery::Axql(axql) => {
                // Prefer returning a typed IR even if the backend returned AxQL.
                // This keeps downstream tooling/LLMs on the stable JSON form and
                // avoids fragile parsing by clients.
                match crate::axql::parse_axql_query(&axql) {
                    Ok(parsed) => {
                        let ir = crate::query_ir::QueryIrV1::from_axql_query(&parsed);
                        let axql_text = ir.to_axql_text()?;
                        serde_json::json!({
                            "version": "axiograph_db_server_llm_to_query_v1",
                            "query_ir_v1": ir,
                            "axql": axql_text
                        })
                    }
                    Err(_) => serde_json::json!({
                        "version": "axiograph_db_server_llm_to_query_v1",
                        "axql": axql,
                        "notes": ["note: failed to parse AxQL into query_ir_v1; returning raw AxQL only"]
                    }),
                }
            }
            GeneratedQuery::QueryIrV1(ir) => serde_json::json!({
                "version": "axiograph_db_server_llm_to_query_v1",
                "query_ir_v1": ir,
                "axql": ir.to_axql_text()?
            }),
        })
    })
    .await
    .map_err(|e| anyhow!("llm/to_query task join failed: {e}"))?
}

#[derive(Debug, Clone, Serialize)]
struct LlmAgentCommitResultV1 {
    attempted: bool,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    accepted_snapshot_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ops_added: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

async fn handle_llm_agent(state: &Arc<ServerState>, req: LlmAgentRequestV1) -> Result<serde_json::Value> {
    if matches!(state.config.llm.backend, LlmBackend::Disabled) {
        return Err(anyhow!(
            "LLM is disabled for this server. Start with: `axiograph db serve ... --llm-ollama --llm-model <model>` (or `--llm-mock`)"
        ));
    }

    let question = req.question.clone();
    let question_for_prompt = question.clone();
    let history = req.history.clone();
    let snapshot_override = req.snapshot.clone();
    let max_steps = match req.max_steps {
        Some(v) => v,
        None => crate::llm::llm_default_max_steps()?,
    };
    let max_rows = req.max_rows.unwrap_or(25);
    let contexts_raw = req.contexts.clone();
    let auto_commit = req.auto_commit;
    let accepted_snapshot_override = req.accepted_snapshot.clone();
    let commit_message = req.commit_message.clone();
    let require_query_certs = req.require_query_certs || req.require_verified_queries;
    let verify_queries = req.verify_queries || req.require_verified_queries;
    let certify_queries = req.certify_queries || verify_queries || require_query_certs;
    let include_anchor = req.include_anchor;
    let max_steps_cap = crate::llm::llm_max_steps_cap()?;

    let state2 = state.clone();
    let (mut outcome, accepted_snapshot_id, query_certs, anchor_axi) = tokio::task::spawn_blocking(move || {
        let (db, meta, embeddings, snapshot_key, accepted_snapshot_id) =
            if let Some(snapshot) = snapshot_override.as_deref() {
                let SnapshotSource::Store { dir, layer, .. } = &state2.config.source else {
                    return Err(anyhow!(
                        "llm snapshot override requires a store-backed server (`--dir ...`)"
                    ));
                };
                let loaded = load_from_store(dir, layer, snapshot)?;
                (
                    loaded.db,
                    loaded.meta,
                    loaded.embeddings,
                    loaded.snapshot_key,
                    loaded.accepted_snapshot_id,
                )
            } else {
                let loaded = state2
                    .loaded
                    .read()
                    .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?;
                (
                    loaded.db.clone(),
                    loaded.meta.clone(),
                    loaded.embeddings.clone(),
                    loaded.snapshot_key.clone(),
                    loaded.accepted_snapshot_id.clone(),
                )
            };

        let mut contexts: Vec<crate::axql::AxqlContextSpec> = Vec::new();
        for c in contexts_raw {
            let c = c.trim();
            if c.is_empty() || c == "*" || c.eq_ignore_ascii_case("all") {
                continue;
            }
            if let Ok(id) = c.parse::<u32>() {
                contexts.push(crate::axql::AxqlContextSpec::EntityId(id));
            } else {
                contexts.push(crate::axql::AxqlContextSpec::Name(c.to_string()));
            }
        }

        // Thread conversation context into the question prompt.
        //
        // The assistant messages are untrusted convenience text: the tool loop
        // is expected to validate and ground claims via tools (AxQL, describe_entity, etc).
        let mut full_question = String::new();
        if !history.is_empty() {
            full_question.push_str("Conversation so far (untrusted; use tools to verify):\n");

            // Keep prompts bounded (local models can be sensitive to long inputs).
            let max_msgs = crate::llm::llm_chat_max_messages()?;
            let start = history.len().saturating_sub(max_msgs.max(1));
            for m in history.iter().skip(start) {
                let role = m.role.trim();
                let role = if role.is_empty() { "unknown" } else { role };
                let mut content = m.content.trim().to_string();
                if content.chars().count() > 800 {
                    content = content.chars().take(800).collect::<String>() + "…";
                }
                full_question.push_str(&format!("[{role}] {content}\n"));
            }
            full_question.push('\n');
            full_question.push_str("Current question:\n");
            full_question.push_str(&question_for_prompt);
        } else {
            full_question = question_for_prompt.clone();
        }

        let mut query_cache = crate::axql::AxqlPreparedQueryCache::default();
        let opts = ToolLoopOptions {
            max_steps: max_steps.clamp(1, max_steps_cap),
            max_rows: max_rows.clamp(1, 200),
            ..Default::default()
        };

        let embed_host = match &state2.config.llm.backend {
            #[cfg(feature = "llm-ollama")]
            LlmBackend::Ollama { host } => Some(host.as_str()),
            _ => None,
        };

        let store_ctx = match &state2.config.source {
            SnapshotSource::Store { dir, layer, .. } => Some(crate::llm::ToolLoopStoreContext {
                dir: dir.to_path_buf(),
                default_layer: layer.to_string(),
            }),
            _ => None,
        };

        let outcome = crate::llm::run_tool_loop_with_meta(
            &state2.config.llm,
            &db,
            meta.as_ref(),
            &contexts,
            &snapshot_key,
            store_ctx.as_ref(),
            embeddings.as_deref(),
            embed_host,
            &mut query_cache,
            &full_question,
            opts,
        )?;

        // Optional: certify (and optionally verify) queries executed by the tool loop.
        let mut query_certs: Option<Vec<serde_json::Value>> = None;
        let mut anchor_axi: Option<serde_json::Value> = None;
        if certify_queries {
            let want_verify = verify_queries;
            let (digest, axi) = export_pathdb_anchor_axi(&db)?;
            if include_anchor {
                anchor_axi = Some(serde_json::json!({
                    "anchor_digest": digest.clone(),
                    "anchor_axi": axi.clone(),
                }));
            }

            let mut out: Vec<serde_json::Value> = Vec::new();
            for (i, step) in outcome.steps.iter().enumerate() {
                if step.tool != "axql_run" {
                    continue;
                }
                let q = step
                    .result
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if q.trim().is_empty() {
                    continue;
                }

                match crate::axql::parse_axql_query(&q)
                    .and_then(|parsed| crate::axql::certify_axql_query_with_meta(&db, &parsed, meta.as_ref()))
                {
                    Ok(cert) => {
                        let cert = cert.with_anchor(axiograph_pathdb::certificate::AxiAnchorV1 {
                            axi_digest_v1: digest.clone(),
                        });
                        let cert_json = serde_json::to_value(&cert).unwrap_or(serde_json::Value::Null);

                        let (verified, verify_out, verify_err) = if want_verify {
                            let cert_text = serde_json::to_string_pretty(&cert)?;
                            match verify_certificate_with_lean(&state2.config, &axi, &cert_text) {
                                Ok((ok, out_text)) => (Some(ok), Some(out_text), None),
                                Err(e) => (Some(false), None, Some(e.to_string())),
                            }
                        } else {
                            (None, None, None)
                        };

                        out.push(serde_json::json!({
                            "step_index": i,
                            "query": q,
                            "certificate": cert_json,
                            "certificate_verified": verified,
                            "certificate_verify_output": verify_out,
                            "certificate_verify_error": verify_err,
                        }));
                    }
                    Err(e) => {
                        out.push(serde_json::json!({
                            "step_index": i,
                            "query": q,
                            "error": e.to_string(),
                        }));
                    }
                }
            }
            query_certs = Some(out);
        }

        Ok::<_, anyhow::Error>((outcome, accepted_snapshot_id, query_certs, anchor_axi))
    })
    .await
    .map_err(|e| anyhow!("llm/agent task join failed: {e}"))??;

    let mut gate: Option<serde_json::Value> = None;
    if require_query_certs {
        let ran_any_query = outcome.steps.iter().any(|s| s.tool == "axql_run");
        let mut failures: Vec<String> = Vec::new();

        if ran_any_query {
            if query_certs.is_none() {
                failures.push("no query_certificates emitted".to_string());
                gate = Some(serde_json::json!({
                    "ok": false,
                    "require_query_certs": true,
                    "require_verified_queries": req.require_verified_queries,
                    "ran_any_query": ran_any_query,
                    "failures": failures,
                }));
                // Refuse to return an un-gated answer.
                outcome.final_answer.answer = "Refusing to answer: certificate gate failed (enable certify+verify and ensure `axiograph_verify` is available).".to_string();
                outcome.final_answer.citations.clear();
                outcome.final_answer.queries.clear();
                outcome.final_answer.notes.push("gate: require_query_certs".to_string());
                if req.require_verified_queries {
                    outcome
                        .final_answer
                        .notes
                        .push("gate: require_verified_queries".to_string());
                }
                // Continue: still allow auto-commit of overlays, and return debug info.
                // (The caller may still want the tool-loop transcript/artifacts.)
            }

            if let Some(certs) = query_certs.as_ref() {
                for c in certs {
                    if let Some(err) = c.get("error").and_then(|v| v.as_str()) {
                        failures.push(format!("query cert error: {err}"));
                        continue;
                    }
                    if req.require_verified_queries {
                        match c.get("certificate_verified").and_then(|v| v.as_bool()) {
                            Some(true) => {}
                            Some(false) => {
                                if let Some(e) =
                                    c.get("certificate_verify_error").and_then(|v| v.as_str())
                                {
                                    failures.push(format!("query cert verify error: {e}"));
                                } else {
                                    failures.push("query cert not verified".to_string());
                                }
                            }
                            None => failures.push("query cert missing verification status".to_string()),
                        }
                    }
                }

                let ok = failures.is_empty();
                gate = Some(serde_json::json!({
                    "ok": ok,
                    "require_query_certs": true,
                    "require_verified_queries": req.require_verified_queries,
                    "ran_any_query": ran_any_query,
                    "failures": failures,
                }));

                if !ok {
                    outcome.final_answer.answer = "Refusing to answer: certificate gate failed (enable certify+verify and ensure `axiograph_verify` is available).".to_string();
                    outcome.final_answer.citations.clear();
                    outcome.final_answer.queries.clear();
                    outcome.final_answer.notes.push("gate: require_query_certs".to_string());
                    if req.require_verified_queries {
                        outcome
                            .final_answer
                            .notes
                            .push("gate: require_verified_queries".to_string());
                    }
                }
            }
        } else {
            // No certified queries were executed; treat the gate as vacuously satisfied.
            gate = Some(serde_json::json!({
                "ok": true,
                "require_query_certs": true,
                "require_verified_queries": req.require_verified_queries,
                "ran_any_query": ran_any_query,
                "failures": [],
            }));
        }
    }

    let mut commit: Option<LlmAgentCommitResultV1> = None;
    if auto_commit {
        commit = Some(LlmAgentCommitResultV1 {
            attempted: false,
            ok: false,
            snapshot_id: None,
            accepted_snapshot_id: None,
            ops_added: None,
            error: None,
        });

        #[derive(Debug, Clone, Deserialize)]
        struct OverlayV1 {
            proposals_json: axiograph_ingest_docs::ProposalsFileV1,
            #[serde(default)]
            chunks: Vec<axiograph_ingest_docs::Chunk>,
            #[serde(default)]
            validation: Option<serde_json::Value>,
        }

        let Some(overlay_json) = outcome.artifacts.generated_overlay.clone() else {
            if let Some(c) = commit.as_mut() {
                c.error = Some("no generated overlay to commit".to_string());
            }
            let v = serde_json::json!({
                "version": "axiograph_db_server_llm_agent_v1",
                "outcome": outcome,
                "commit": commit,
            });
            return Ok(v);
        };

        let overlay: OverlayV1 = match serde_json::from_value(overlay_json) {
            Ok(v) => v,
            Err(e) => {
                if let Some(c) = commit.as_mut() {
                    c.attempted = true;
                    c.error = Some(format!("failed to parse generated overlay: {e}"));
                }
                return Ok(serde_json::json!({
                    "version": "axiograph_db_server_llm_agent_v1",
                    "outcome": outcome,
                    "commit": commit,
                }));
            }
        };

        let overlay_ok = overlay
            .validation
            .as_ref()
            .and_then(|v| v.get("ok"))
            .and_then(|v| v.as_bool());
        if overlay_ok == Some(false) {
            if let Some(c) = commit.as_mut() {
                c.attempted = true;
                c.error = Some("refusing to auto-commit: overlay validation failed".to_string());
            }
            return Ok(serde_json::json!({
                "version": "axiograph_db_server_llm_agent_v1",
                "outcome": outcome,
                "commit": commit,
            }));
        }

        let message = commit_message
            .as_deref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                let q = question.trim();
                if q.is_empty() {
                    None
                } else {
                    Some(format!("llm: {q}"))
                }
            });

        let commit_req = PathdbCommitRequestV1 {
            accepted_snapshot: accepted_snapshot_override
                .clone()
                .or_else(|| accepted_snapshot_id.clone()),
            chunks: overlay.chunks,
            proposals: Some(overlay.proposals_json),
            validate: Some(overlay_ok != Some(true)),
            quality: None,
            quality_plane: None,
            message,
        };

        match handle_pathdb_commit_req(state, commit_req).await {
            Ok(res) => {
                if let Some(c) = commit.as_mut() {
                    c.attempted = true;
                    c.ok = true;
                    c.snapshot_id = Some(res.snapshot_id.clone());
                    c.accepted_snapshot_id = Some(res.accepted_snapshot_id.clone());
                    c.ops_added = Some(res.ops_added);
                }
            }
            Err(e) => {
                if let Some(c) = commit.as_mut() {
                    c.attempted = true;
                    c.error = Some(e.to_string());
                }
            }
        }
    }

    let mut out = serde_json::json!({
        "version": "axiograph_db_server_llm_agent_v1",
        "outcome": outcome,
    });
    if let Some(g) = gate {
        out["gate"] = g;
    }
    if let Some(certs) = query_certs {
        out["query_certificates"] = serde_json::to_value(certs).unwrap_or(serde_json::Value::Null);
    }
    if let Some(anchor) = anchor_axi {
        out["anchor"] = anchor;
    }
    if auto_commit {
        out["commit"] = serde_json::to_value(commit).unwrap_or(serde_json::Value::Null);
    }
    Ok(out)
}

async fn handle_entity_describe_get(
    state: &Arc<ServerState>,
    query: Option<&str>,
) -> Result<serde_json::Value> {
    let p = parse_query_params(query);

    let id = p.get("id").and_then(|s| s.parse::<u32>().ok());
    let name = p.get("name").cloned();
    let type_name = p
        .get("type")
        .or_else(|| p.get("type_name"))
        .cloned();
    let max_attrs = p.get("max_attrs").and_then(|s| s.parse::<usize>().ok());
    let max_rel_types = p
        .get("max_rel_types")
        .and_then(|s| s.parse::<usize>().ok());
    let out_limit = p.get("out_limit").and_then(|s| s.parse::<usize>().ok());
    let in_limit = p.get("in_limit").and_then(|s| s.parse::<usize>().ok());

    let args = serde_json::json!({
        "id": id,
        "name": name,
        "type": type_name,
        "max_attrs": max_attrs,
        "max_rel_types": max_rel_types,
        "out_limit": out_limit,
        "in_limit": in_limit,
    });

    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        let loaded = state
            .loaded
            .read()
            .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?;
        let db = loaded.db.clone();
        let result = crate::llm::describe_entity_v1(db.as_ref(), &args)?;
        Ok::<_, anyhow::Error>(serde_json::json!({
            "version": "axiograph_entity_describe_v1",
            "result": result,
        }))
    })
    .await
    .map_err(|e| anyhow!("entity/describe task join failed: {e}"))?
}

async fn handle_docchunk_get(state: &Arc<ServerState>, query: Option<&str>) -> Result<serde_json::Value> {
    let p = parse_query_params(query);

    let id = p.get("id").and_then(|s| s.parse::<u32>().ok());
    let chunk_id = p
        .get("chunk_id")
        .or_else(|| p.get("chunkId"))
        .cloned();
    let max_chars = p.get("max_chars").and_then(|s| s.parse::<usize>().ok());
    let snapshot_override = p.get("snapshot").cloned();

    let args = serde_json::json!({
        "id": id,
        "chunk_id": chunk_id,
        "max_chars": max_chars,
    });

    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        let db = if let Some(snapshot) = snapshot_override.as_deref() {
            let SnapshotSource::Store { dir, layer, .. } = &state.config.source else {
                return Err(anyhow!(
                    "docchunk/get snapshot override requires a store-backed server (`--dir ...`)"
                ));
            };
            let loaded = load_from_store(dir, layer, snapshot)?;
            loaded.db
        } else {
            let loaded = state
                .loaded
                .read()
                .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?;
            loaded.db.clone()
        };

        let result = crate::llm::docchunk_get_v1(db.as_ref(), &args)?;
        Ok::<_, anyhow::Error>(serde_json::json!({
            "version": "axiograph_docchunk_get_v1",
            "result": result,
        }))
    })
    .await
    .map_err(|e| anyhow!("docchunk/get task join failed: {e}"))?
}

async fn handle_contexts_get(state: &Arc<ServerState>) -> Result<serde_json::Value> {
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        let loaded = state
            .loaded
            .read()
            .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?;
        let db = loaded.db.clone();

        let mut contexts: Vec<serde_json::Value> = Vec::new();
        if let Some(bm) = db.find_by_type("Context") {
            for id in bm.iter() {
                let name = db
                    .get_entity(id)
                    .and_then(|v| v.attrs.get("name").cloned())
                    .unwrap_or_else(|| format!("Context#{id}"));
                let fact_count = db.fact_nodes_by_context(id).len();
                contexts.push(serde_json::json!({
                    "id": id,
                    "name": name,
                    "fact_count": fact_count,
                }));
            }
        }
        contexts.sort_by(|a, b| {
            let an = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let bn = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
            an.cmp(bn)
        });

        Ok::<_, anyhow::Error>(serde_json::json!({
            "version": "axiograph_contexts_v1",
            "contexts": contexts,
        }))
    })
    .await
    .map_err(|e| anyhow!("contexts task join failed: {e}"))?
}

async fn handle_discover_draft_axi(
    _state: &Arc<ServerState>,
    body: &[u8],
) -> Result<serde_json::Value> {
    #[derive(Debug, Clone, Deserialize)]
    struct Req {
        proposals: axiograph_ingest_docs::ProposalsFileV1,
        #[serde(default)]
        module_name: Option<String>,
        #[serde(default)]
        schema_name: Option<String>,
        #[serde(default)]
        instance_name: Option<String>,
        #[serde(default)]
        infer_constraints: Option<bool>,
    }
    let req: Req = serde_json::from_slice(body)
        .map_err(|e| anyhow!("failed to parse discover/draft-axi request JSON: {e}"))?;

    let opts = crate::schema_discovery::DraftAxiModuleOptions {
        module_name: req.module_name.unwrap_or_else(|| "DraftModule".to_string()),
        schema_name: req.schema_name.unwrap_or_else(|| "DraftSchema".to_string()),
        instance_name: req.instance_name.unwrap_or_else(|| "DraftInstance".to_string()),
        infer_constraints: req.infer_constraints.unwrap_or(true),
    };

    let axi_text = crate::schema_discovery::draft_axi_module_from_proposals(&req.proposals, &opts)?;
    let digest = axiograph_dsl::digest::axi_digest_v1(&axi_text);

    Ok(serde_json::json!({
        "version": "axiograph_discover_draft_axi_v1",
        "digest": digest,
        "module_name": opts.module_name,
        "schema_name": opts.schema_name,
        "instance_name": opts.instance_name,
        "axi_text": axi_text,
    }))
}

fn viz_request_from_query(query: Option<&str>) -> Result<VizRequestV1> {
    let p = parse_query_params(query);
    let hops = p.get("hops").and_then(|s| s.parse::<usize>().ok());
    let max_nodes = p.get("max_nodes").and_then(|s| s.parse::<usize>().ok());
    let max_edges = p.get("max_edges").and_then(|s| s.parse::<usize>().ok());
    let focus_id = p.get("focus_id").and_then(|s| s.parse::<u32>().ok());
    let refresh_secs = p.get("refresh_secs").and_then(|s| s.parse::<u64>().ok());
    let snapshot = p.get("snapshot").cloned();
    Ok(VizRequestV1 {
        format: p.get("format").cloned(),
        plane: p.get("plane").cloned(),
        focus_name: p.get("focus_name").cloned(),
        focus_type: p.get("focus_type").cloned(),
        focus_id,
        hops,
        max_nodes,
        max_edges,
        direction: p.get("direction").cloned(),
        include_equivalences: p
            .get("include_equivalences")
            .and_then(|s| parse_bool(Some(s.as_str()))),
        typed_overlay: p.get("typed_overlay").and_then(|s| parse_bool(Some(s.as_str()))),
        refresh_secs,
        snapshot,
    })
}

async fn handle_viz_get_as(
    state: &Arc<ServerState>,
    query: Option<&str>,
    force_format: &str,
) -> Result<Response<Full<Bytes>>> {
    let mut req = viz_request_from_query(query)?;
    req.format = Some(force_format.to_string());
    handle_viz_request(state, req).await
}

async fn handle_viz_get(
    state: &Arc<ServerState>,
    query: Option<&str>,
) -> Result<Response<Full<Bytes>>> {
    let req = viz_request_from_query(query)?;
    handle_viz_request(state, req).await
}

async fn handle_viz_post(
    state: &Arc<ServerState>,
    body: &[u8],
) -> Result<Response<Full<Bytes>>> {
    let req: VizRequestV1 =
        serde_json::from_slice(body).map_err(|e| anyhow!("failed to parse viz request JSON: {e}"))?;
    handle_viz_request(state, req).await
}

async fn handle_viz_request(
    state: &Arc<ServerState>,
    req: VizRequestV1,
) -> Result<Response<Full<Bytes>>> {
    let req_format = req.format.as_deref().unwrap_or("html");
    let format = crate::viz::VizFormat::parse(req_format)?;

    let plane = req.plane.as_deref().unwrap_or("data").trim().to_ascii_lowercase();
    let (include_meta_plane, include_data_plane) = match plane.as_str() {
        "data" => (false, true),
        "meta" => (true, false),
        "both" => (true, true),
        other => {
            return Err(anyhow!(
                "unknown plane `{other}` (expected data|meta|both)"
            ))
        }
    };

    let direction = crate::viz::VizDirection::parse(req.direction.as_deref().unwrap_or("both"))?;
    let typed_overlay = req.typed_overlay.unwrap_or(false);
    let include_equivalences = req.include_equivalences.unwrap_or(true);

    let hops = req.hops.unwrap_or(2);
    let max_nodes = req.max_nodes.unwrap_or(250);
    let max_edges = req.max_edges.unwrap_or(4_000);

    let refresh_secs = req.refresh_secs.unwrap_or(0);
    let snapshot_override = req.snapshot.clone();

    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        let (db, meta) = if let Some(snapshot) = snapshot_override.as_deref() {
            let SnapshotSource::Store { dir, layer, .. } = &state.config.source else {
                return Err(anyhow!(
                    "viz snapshot override requires a store-backed server (`--dir ...`)"
                ));
            };
            let loaded = load_from_store(dir, layer, snapshot)?;
            (loaded.db, loaded.meta)
        } else {
            let loaded = state
                .loaded
                .read()
                .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?;
            (loaded.db.clone(), loaded.meta.clone())
        };

        let mut focus_ids: Vec<u32> = Vec::new();
        if let Some(id) = req.focus_id {
            focus_ids.push(id);
        } else if let Some(name) = req.focus_name.as_deref() {
            let id = crate::viz::resolve_focus_by_name_and_type(
                &db,
                name,
                req.focus_type.as_deref(),
            )?
            .ok_or_else(|| anyhow!("could not resolve focus_name `{name}`"))?;
            focus_ids.push(id);
        }

        let options = crate::viz::VizOptions {
            focus_ids,
            hops,
            max_nodes,
            max_edges,
            direction,
            include_meta_plane,
            include_data_plane,
            include_equivalences,
            typed_overlay,
        };

        let mut meta_for_overlay = meta;
        if typed_overlay && meta_for_overlay.is_none() {
            meta_for_overlay = Some(MetaPlaneIndex::from_db(&db)?);
        }

        let g = crate::viz::extract_viz_graph_with_meta(&db, &options, meta_for_overlay.as_ref())?;
        let (content_type, bytes) = match format {
            crate::viz::VizFormat::Html => {
                let html = crate::viz::render_html(&db, &g)?;
                let html = inject_meta_refresh(html, refresh_secs);
                ("text/html; charset=utf-8", html.into_bytes())
            }
            crate::viz::VizFormat::Json => ("application/json", crate::viz::render_json(&g)?.into_bytes()),
            crate::viz::VizFormat::Dot => ("text/vnd.graphviz; charset=utf-8", crate::viz::render_dot(&db, &g).into_bytes()),
        };

        Ok::<_, anyhow::Error>(
            Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, content_type)
                .body(Full::new(Bytes::from(bytes)))
                .unwrap_or_else(|_| {
                    Response::new(Full::new(Bytes::from_static(
                        b"{\"error\":\"internal\"}",
                    )))
                }),
        )
    })
    .await
    .map_err(|e| anyhow!("viz task join failed: {e}"))?
}

#[derive(Debug, Clone, Deserialize)]
struct PromoteRequestV1 {
    axi_text: String,
    #[serde(default)]
    message: Option<String>,
    /// off|fast|strict
    #[serde(default)]
    quality: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PromoteResponseV1 {
    snapshot_id: String,
}

async fn handle_promote(state: &Arc<ServerState>, body: &[u8]) -> Result<PromoteResponseV1> {
    let req: PromoteRequestV1 = serde_json::from_slice(body)
        .map_err(|e| anyhow!("failed to parse promote request JSON: {e}"))?;

    let SnapshotSource::Store { dir, .. } = &state.config.source else {
        return Err(anyhow!(
            "promote requires `db serve --dir <accepted_plane_dir>` (store-backed server)"
        ));
    };

    let dir = dir.clone();
    let quality = req
        .quality
        .as_deref()
        .unwrap_or("off")
        .trim()
        .to_string();
    let message = req.message.clone();
    let axi_text = req.axi_text.clone();

    let snapshot_id = tokio::task::spawn_blocking(move || {
        let tmp = write_temp_file("axi", &axi_text)?;
        let out = crate::accepted_plane::promote_reviewed_module(
            &tmp,
            &dir,
            message.as_deref(),
            &quality,
        )?;
        let _ = std::fs::remove_file(&tmp);
        Ok::<_, anyhow::Error>(out)
    })
    .await
    .map_err(|e| anyhow!("promote task join failed: {e}"))??;

    // If we're serving `accepted/head`, reload immediately so clients see it.
    let should_reload = match &state.config.source {
        SnapshotSource::Store { layer, snapshot, .. } => {
            layer.trim().eq_ignore_ascii_case("accepted")
                && (snapshot == "head" || snapshot == "latest")
        }
        _ => false,
    };
    if should_reload {
        let _ = reload_now(state).await;
    }

    Ok(PromoteResponseV1 { snapshot_id })
}

#[derive(Debug, Clone, Deserialize)]
struct PathdbCommitRequestV1 {
    #[serde(default)]
    accepted_snapshot: Option<String>,
    #[serde(default)]
    chunks: Vec<axiograph_ingest_docs::Chunk>,
    #[serde(default)]
    proposals: Option<axiograph_ingest_docs::ProposalsFileV1>,
    /// Whether to validate proposals before committing (default: true).
    ///
    /// Validation runs a preview import + meta-plane typecheck + quality delta.
    #[serde(default)]
    validate: Option<bool>,
    /// off|fast|strict (default: fast)
    #[serde(default)]
    quality: Option<String>,
    /// meta|data|both (default: both)
    #[serde(default)]
    quality_plane: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PathdbCommitResponseV1 {
    snapshot_id: String,
    accepted_snapshot_id: String,
    ops_added: usize,
}

async fn handle_pathdb_commit(
    state: &Arc<ServerState>,
    body: &[u8],
) -> Result<PathdbCommitResponseV1> {
    let req: PathdbCommitRequestV1 = serde_json::from_slice(body)
        .map_err(|e| anyhow!("failed to parse pathdb-commit request JSON: {e}"))?;
    handle_pathdb_commit_req(state, req).await
}

async fn handle_pathdb_commit_req(
    state: &Arc<ServerState>,
    req: PathdbCommitRequestV1,
) -> Result<PathdbCommitResponseV1> {
    if req.chunks.is_empty() && req.proposals.is_none() {
        return Err(anyhow!("must provide non-empty `chunks` or `proposals`"));
    }

    let SnapshotSource::Store { dir, .. } = &state.config.source else {
        return Err(anyhow!(
            "pathdb-commit requires `db serve --dir <accepted_plane_dir>` (store-backed server)"
        ));
    };

    let dir = dir.clone();
    let accepted_snapshot = req.accepted_snapshot.as_deref().unwrap_or("head").to_string();
    let message = req.message.clone();
    let chunks = req.chunks.clone();
    let proposals = req.proposals.clone();

    // Gate: validate proposal overlays (UI-friendly safe default).
    let should_validate = req.validate.unwrap_or(true);
    if should_validate {
        if let Some(file) = proposals.as_ref() {
            let quality = req
                .quality
                .as_deref()
                .unwrap_or("fast")
                .trim()
                .to_string();
            let quality_plane = req
                .quality_plane
                .as_deref()
                .unwrap_or("both")
                .trim()
                .to_string();

            let base = state
                .loaded
                .read()
                .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?
                .db
                .clone();
            let file = file.clone();

            let validation = tokio::task::spawn_blocking(move || {
                crate::proposals_validate::validate_proposals_v1(
                    base.as_ref(),
                    &file,
                    &quality,
                    &quality_plane,
                )
            })
            .await
            .map_err(|e| anyhow!("pathdb-commit validation join failed: {e}"))??;

            if !validation.ok {
                let mut msg = String::new();
                msg.push_str("proposals validation failed");
                if !validation.axi_typecheck.skipped && !validation.axi_typecheck.errors.is_empty() {
                    msg.push_str(": typecheck errors: ");
                    for (i, e) in validation.axi_typecheck.errors.iter().take(4).enumerate() {
                        if i > 0 {
                            msg.push_str(" | ");
                        }
                        msg.push_str(&e.message);
                    }
                    if validation.axi_typecheck.errors.len() > 4 {
                        msg.push_str(&format!(" (+{} more)", validation.axi_typecheck.errors.len() - 4));
                    }
                }
                if validation.quality_delta.summary.error_count > 0 {
                    msg.push_str(&format!(
                        "; quality errors added: {}",
                        validation.quality_delta.summary.error_count
                    ));
                }
                return Err(anyhow!(msg));
            }
        }
    }

    let result = tokio::task::spawn_blocking(move || {
        let mut chunk_paths: Vec<PathBuf> = Vec::new();
        if !chunks.is_empty() {
            let tmp = write_temp_file(
                "chunks.json",
                &serde_json::to_string_pretty(&chunks).unwrap_or_default(),
            )?;
            chunk_paths.push(tmp);
        }

        let mut proposal_paths: Vec<PathBuf> = Vec::new();
        if let Some(file) = proposals.as_ref() {
            let tmp = write_temp_file(
                "proposals.json",
                &serde_json::to_string_pretty(file).unwrap_or_default(),
            )?;
            proposal_paths.push(tmp);
        }

        let res = crate::pathdb_wal::commit_pathdb_snapshot_with_overlays(
            &dir,
            &accepted_snapshot,
            &chunk_paths,
            &proposal_paths,
            message.as_deref(),
        )?;

        for p in chunk_paths.into_iter().chain(proposal_paths.into_iter()) {
            let _ = std::fs::remove_file(&p);
        }
        Ok::<_, anyhow::Error>(res)
    })
    .await
    .map_err(|e| anyhow!("pathdb-commit task join failed: {e}"))??;

    // If we're serving `pathdb/head`, reload immediately so clients see it.
    let should_reload = match &state.config.source {
        SnapshotSource::Store { layer, snapshot, .. } => {
            layer.trim().eq_ignore_ascii_case("pathdb")
                && (snapshot == "head" || snapshot == "latest")
        }
        _ => false,
    };
    if should_reload {
        let _ = reload_now(state).await;
    }

    Ok(PathdbCommitResponseV1 {
        snapshot_id: result.snapshot_id,
        accepted_snapshot_id: result.accepted_snapshot_id,
        ops_added: result.ops_added,
    })
}

#[derive(Debug, Clone, Serialize)]
struct ProposalsRelationResponseV1 {
    proposals_json: axiograph_ingest_docs::ProposalsFileV1,
    #[serde(default)]
    chunks: Vec<axiograph_ingest_docs::Chunk>,
    summary: crate::proposal_gen::ProposeRelationSummaryV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    validation: Option<crate::proposals_validate::ProposalsValidationV1>,
}

#[derive(Debug, Clone, Serialize)]
struct ProposalsRelationsResponseV1 {
    proposals_json: axiograph_ingest_docs::ProposalsFileV1,
    #[serde(default)]
    chunks: Vec<axiograph_ingest_docs::Chunk>,
    summary: crate::proposal_gen::ProposeRelationsSummaryV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    validation: Option<crate::proposals_validate::ProposalsValidationV1>,
}

async fn handle_proposals_relation(
    state: &Arc<ServerState>,
    body: &[u8],
) -> Result<ProposalsRelationResponseV1> {
    #[derive(Deserialize)]
    struct Request {
        #[serde(flatten)]
        input: crate::proposal_gen::ProposeRelationInputV1,
        /// Whether to validate the resulting proposals by preview-importing them
        /// and running meta-plane typechecking + quality checks.
        #[serde(default)]
        validate: Option<bool>,
        #[serde(default)]
        quality_profile: Option<String>,
        #[serde(default)]
        quality_plane: Option<String>,
    }

    let req: Request = serde_json::from_slice(body)
        .map_err(|e| anyhow!("failed to parse /proposals/relation request JSON: {e}"))?;

    let loaded = state
        .loaded
        .read()
        .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?;
    let db = loaded.db.clone();

    // The server endpoint is deterministic and does not assume query scoping.
    // Clients can pass `context` explicitly; otherwise the proposal is context-free.
    let out = crate::proposal_gen::propose_relation_proposals_v1(&db, &[], req.input)?;

    let validate = req.validate.unwrap_or(true);
    let validation = if validate {
        let profile = req.quality_profile.unwrap_or_else(|| "fast".to_string());
        let plane = req.quality_plane.unwrap_or_else(|| "both".to_string());
        Some(crate::proposals_validate::validate_proposals_v1(
            &db,
            &out.proposals,
            &profile,
            &plane,
        )?)
    } else {
        None
    };

    Ok(ProposalsRelationResponseV1 {
        proposals_json: out.proposals,
        chunks: out.chunks,
        summary: out.summary,
        validation,
    })
}

async fn handle_proposals_relations(
    state: &Arc<ServerState>,
    body: &[u8],
) -> Result<ProposalsRelationsResponseV1> {
    #[derive(Deserialize)]
    struct Request {
        #[serde(flatten)]
        input: crate::proposal_gen::ProposeRelationsInputV1,
        #[serde(default)]
        validate: Option<bool>,
        #[serde(default)]
        quality_profile: Option<String>,
        #[serde(default)]
        quality_plane: Option<String>,
    }

    let req: Request = serde_json::from_slice(body)
        .map_err(|e| anyhow!("failed to parse /proposals/relations request JSON: {e}"))?;

    let loaded = state
        .loaded
        .read()
        .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?;
    let db = loaded.db.clone();

    // The server endpoint is deterministic and does not assume query scoping.
    // Clients can pass `context` explicitly; otherwise the proposal is context-free.
    let out = crate::proposal_gen::propose_relations_proposals_v1(&db, &[], req.input)?;

    let validate = req.validate.unwrap_or(true);
    let validation = if validate {
        let profile = req.quality_profile.unwrap_or_else(|| "fast".to_string());
        let plane = req.quality_plane.unwrap_or_else(|| "both".to_string());
        Some(crate::proposals_validate::validate_proposals_v1(
            &db,
            &out.proposals,
            &profile,
            &plane,
        )?)
    } else {
        None
    };

    Ok(ProposalsRelationsResponseV1 {
        proposals_json: out.proposals,
        chunks: out.chunks,
        summary: out.summary,
        validation,
    })
}

fn write_temp_file(suffix: &str, contents: &str) -> Result<PathBuf> {
    let mut path = std::env::temp_dir();
    let ts = now_unix_secs();
    let pid = std::process::id();
    path.push(format!("axiograph_db_server_{pid}_{ts}_{suffix}"));
    std::fs::write(&path, contents)?;
    Ok(path)
}

async fn reload_now(state: &Arc<ServerState>) -> Result<serde_json::Value> {
    let loaded = tokio::task::spawn_blocking({
        let config = state.config.clone();
        move || load_snapshot(&config)
    })
    .await
    .map_err(|e| anyhow!("reload task join failed: {e}"))??;

    {
        let mut guard = state
            .loaded
            .write()
            .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?;
        *guard = loaded;
    }
    if let Ok(mut cache) = state.query_cache.lock() {
        cache.clear();
    }

    status_payload(state)
}

async fn reload_if_head_changed(state: &Arc<ServerState>) -> Result<()> {
    let SnapshotSource::Store { dir, layer, snapshot } = &state.config.source else {
        return Ok(());
    };
    if snapshot != "head" && snapshot != "latest" {
        return Ok(());
    }

    let head_path = if layer.trim().eq_ignore_ascii_case("accepted") {
        dir.join("HEAD")
    } else if layer.trim().eq_ignore_ascii_case("pathdb") {
        dir.join("pathdb").join("HEAD")
    } else {
        return Ok(());
    };

    let head = std::fs::read_to_string(&head_path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let current = {
        let loaded = state
            .loaded
            .read()
            .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?;
        if layer.trim().eq_ignore_ascii_case("accepted") {
            loaded.accepted_snapshot_id.clone()
        } else {
            loaded.pathdb_snapshot_id.clone()
        }
    };

    if head.is_some() && head != current {
        let _ = reload_now(state).await?;
    }
    Ok(())
}

fn load_snapshot(config: &ServerConfig) -> Result<LoadedSnapshot> {
    match &config.source {
        SnapshotSource::Axpd(path) => load_from_axpd(path),
        SnapshotSource::Store {
            dir,
            layer,
            snapshot,
        } => load_from_store(dir, layer, snapshot),
    }
}

fn load_from_axpd(path: &Path) -> Result<LoadedSnapshot> {
    let bytes = std::fs::read(path)
        .map_err(|e| anyhow!("failed to read .axpd `{}`: {e}", path.display()))?;
    let snapshot_key = axiograph_dsl::digest::fnv1a64_digest_bytes(&bytes);
    let db = PathDB::from_bytes(&bytes)?;
    let meta = MetaPlaneIndex::from_db(&db).ok();
    Ok(LoadedSnapshot {
        snapshot_key: snapshot_key.clone(),
        snapshot_label: format!("axpd:{} ({})", path.display(), snapshot_key),
        accepted_snapshot_id: None,
        pathdb_snapshot_id: None,
        loaded_at_unix_secs: now_unix_secs(),
        entities: db.entities.len(),
        relations: db.relations.len(),
        db: Arc::new(db),
        meta,
        embeddings: None,
    })
}

fn load_from_store(dir: &Path, layer: &str, snapshot: &str) -> Result<LoadedSnapshot> {
    let layer = layer.trim().to_ascii_lowercase();
    if !matches!(layer.as_str(), "accepted" | "pathdb") {
        return Err(anyhow!(
            "unknown --layer `{}` (expected accepted|pathdb)",
            layer
        ));
    }

    let (accepted_snapshot_id, pathdb_snapshot_id, pathdb_manifest) = if layer == "accepted" {
        let id = crate::accepted_plane::resolve_snapshot_id_for_cli(dir, snapshot)?;
        (Some(id), None, None)
    } else {
        let snap = crate::pathdb_wal::read_pathdb_snapshot_for_cli(dir, snapshot)?;
        (
            Some(snap.accepted_snapshot_id.clone()),
            Some(snap.snapshot_id.clone()),
            Some(snap),
        )
    };

    let tmp = write_temp_file("axpd", ""); // reserve a unique name
    let tmp = tmp?;
    if layer == "accepted" {
        crate::accepted_plane::build_pathdb_from_snapshot(
            dir,
            accepted_snapshot_id
                .as_deref()
                .expect("accepted id set"),
            &tmp,
        )?;
    } else {
        crate::pathdb_wal::build_pathdb_from_pathdb_snapshot(
            dir,
            pathdb_snapshot_id.as_deref().expect("pathdb id set"),
            &tmp,
        )?;
    }

    let bytes = std::fs::read(&tmp)?;
    let _ = std::fs::remove_file(&tmp);
    let snapshot_key = if layer == "pathdb" {
        pathdb_snapshot_id
            .as_deref()
            .expect("pathdb id set")
            .to_string()
    } else {
        accepted_snapshot_id
            .as_deref()
            .expect("accepted id set")
            .to_string()
    };

    let db = PathDB::from_bytes(&bytes)?;
    let meta = MetaPlaneIndex::from_db(&db).ok();
    let embeddings = if let Some(manifest) = pathdb_manifest.as_ref() {
        let mut idx = crate::embeddings::ResolvedEmbeddingsIndexV1::default();
        let mut any = false;

        for op in &manifest.ops {
            if let crate::pathdb_wal::PathDbWalOpV1::ImportEmbeddingsV1 {
                embeddings_digest: _,
                stored_path,
            } = op
            {
                let path = dir.join(stored_path);
                let bytes = std::fs::read(&path)?;
                let file = crate::embeddings::decode_embeddings_file_v1(&bytes)?;
                idx.resolve_and_set(&db, file)?;
                any = true;
            }
        }

        if any { Some(Arc::new(idx)) } else { None }
    } else {
        None
    };

    Ok(LoadedSnapshot {
        snapshot_key: snapshot_key.clone(),
        snapshot_label: format!(
            "store:{} layer={} snapshot={}",
            dir.display(),
            layer,
            snapshot_key
        ),
        accepted_snapshot_id,
        pathdb_snapshot_id,
        loaded_at_unix_secs: now_unix_secs(),
        entities: db.entities.len(),
        relations: db.relations.len(),
        db: Arc::new(db),
        meta,
        embeddings,
    })
}
