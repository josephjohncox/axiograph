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

use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper::header::{AUTHORIZATION, CONTENT_TYPE};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use url::form_urlencoded;

use axiograph_pathdb::axi_semantics::MetaPlaneIndex;
use axiograph_pathdb::{
    read_sidecar_file, AcceptedAxiAnchor, AcceptedSnapshotId, AxiDigest, IndexSidecarWriter,
    PathDB, PathdbSnapshotId, WorldModelRunId,
};

use crate::accepted_plane::{AcceptedPlaneEventV1, AcceptedPlaneSnapshotV1};
use crate::llm::{GeneratedQuery, LlmBackend, LlmState, ToolLoopOptions};
use crate::pathdb_wal::{PathDbSnapshotV1, PathDbWalEventV1};
use crate::world_model::{WorldModelBackend, WorldModelState};

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
    world_model: WorldModelState,
    world_model_workers: usize,
    path_index_lru_capacity: usize,
    path_index_lru_async: bool,
    path_index_lru_queue: usize,
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
    accepted_snapshot_id: Option<AcceptedSnapshotId>,
    /// For store-backed snapshots that resolve to exactly one accepted module,
    /// the accepted-module anchor bound to this loaded runtime.
    accepted_axi_anchor: Option<AcceptedAxiAnchor>,
    /// Canonical `.axi` text for the accepted anchor above, when available.
    accepted_axi_text: Option<String>,
    /// For store-based loads, the resolved PathDB WAL snapshot id.
    pathdb_snapshot_id: Option<PathdbSnapshotId>,
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
    entries: HashMap<QueryCacheKey, Arc<Mutex<crate::query_ir::PreparedQueryV1>>>,
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

    fn get(&mut self, key: &QueryCacheKey) -> Option<Arc<Mutex<crate::query_ir::PreparedQueryV1>>> {
        let value = self.entries.get(key).cloned()?;
        self.touch(key);
        Some(value)
    }

    fn insert(&mut self, key: QueryCacheKey, value: Arc<Mutex<crate::query_ir::PreparedQueryV1>>) {
        self.entries.insert(key.clone(), value);
        self.touch(&key);

        while self.lru.len() > Self::MAX_ENTRIES {
            if let Some(oldest) = self.lru.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }
}

#[derive(Clone)]
struct WorldModelExecutor {
    semaphore: Arc<Semaphore>,
}

impl WorldModelExecutor {
    fn new(workers: usize) -> Self {
        let workers = workers.max(1);
        Self {
            semaphore: Arc::new(Semaphore::new(workers)),
        }
    }

    async fn run<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce() -> Result<R> + Send + 'static,
        R: Send + 'static,
    {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| anyhow!("world model executor closed"))?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            f()
        })
        .await
        .map_err(|e| anyhow!("world model task join failed: {e}"))?
    }
}

struct ServerState {
    config: ServerConfig,
    loaded: RwLock<LoadedSnapshot>,
    query_cache: Mutex<QueryPlanCache>,
    world_model_executor: WorldModelExecutor,
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

fn resolve_verifier_bin(config: &CertVerifyConfig) -> Option<PathBuf> {
    if let Some(p) = config.verifier_bin.as_ref() {
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

fn export_canonical_module_axi(db: &PathDB) -> Result<(AxiDigest, String)> {
    let exported = crate::world_model_input::export_pathdb_world_model_axi(
        db,
        &crate::world_model_input::WorldModelAxiInputOptionsV1::default(),
    )?;
    Ok((exported.axi_digest_v1, exported.axi_text))
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
    let mut child = cmd
        .spawn()
        .map_err(|e| anyhow!("failed to spawn verifier: {e}"))?;

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
    config: &CertVerifyConfig,
    module_axi: &str,
    certificate_json: &str,
) -> Result<(bool, String)> {
    let Some(verifier) = resolve_verifier_bin(config) else {
        return Err(anyhow!(
            "Lean verifier not configured (set --verify-bin or AXIOGRAPH_VERIFY_BIN, or build with `make lean-exe`)"
        ));
    };

    let module_path = write_temp_file_unique("verify_module_input.axi", module_axi)?;
    let cert_path = write_temp_file_unique("cert.json", certificate_json)?;

    let timeout = config.timeout;
    let mut cmd = Command::new(&verifier);
    cmd.arg(&module_path).arg(&cert_path);
    let output = run_command_output_with_timeout(cmd, timeout);

    let _ = std::fs::remove_file(&module_path);
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
            let key = std::env::var(crate::llm::OPENAI_API_KEY_ENV).unwrap_or_default();
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
            let key = std::env::var(crate::llm::ANTHROPIC_API_KEY_ENV).unwrap_or_default();
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

    if (args.world_model_stub as usize)
        + (args.world_model_plugin.is_some() as usize)
        + (args.world_model_http.is_some() as usize)
        + (args.world_model_llm as usize)
        > 1
    {
        return Err(anyhow!(
            "db serve: choose at most one world model backend: `--world-model-stub`, `--world-model-plugin ...`, `--world-model-http ...`, or `--world-model-llm`"
        ));
    }

    let mut world_model = WorldModelState::default();
    if args.world_model_stub {
        world_model.backend = WorldModelBackend::Stub;
    } else if let Some(url) = args.world_model_http.as_ref() {
        world_model.backend = WorldModelBackend::Http { url: url.clone() };
    } else if args.world_model_llm {
        let exe = std::env::current_exe()
            .map_err(|e| anyhow!("db serve: failed to resolve current executable: {e}"))?;
        let mut args_list = vec!["ingest".to_string(), "world-model-plugin-llm".to_string()];
        let has_model_arg = args.world_model_plugin_arg.iter().any(|a| a == "--model");
        if let Some(model) = args.world_model_model.as_ref() {
            if !has_model_arg {
                args_list.push("--model".to_string());
                args_list.push(model.clone());
            }
        }
        args_list.extend(args.world_model_plugin_arg.clone());
        crate::llm::validate_world_model_llm_backend_arg(&args_list)?;
        world_model.backend = WorldModelBackend::Command {
            program: exe,
            args: args_list,
        };
    } else if let Some(plugin) = args.world_model_plugin.as_ref() {
        world_model.backend = WorldModelBackend::Command {
            program: plugin.clone(),
            args: args.world_model_plugin_arg.clone(),
        };
    }
    world_model.model = args.world_model_model.clone();

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
            return Err(anyhow!(
                "db serve: pass either --axpd <file.axpd> or --dir <accepted_plane_dir>"
            ));
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
        world_model,
        world_model_workers: args.world_model_workers,
        path_index_lru_capacity: args.path_index_lru_capacity,
        path_index_lru_async: args.path_index_lru_async,
        path_index_lru_queue: args.path_index_lru_queue,
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
        world_model_executor: WorldModelExecutor::new(config.world_model_workers),
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
        bound, config.role
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
        std::fs::write(
            path,
            serde_json::to_string_pretty(&payload).unwrap_or_default(),
        )
        .ok();
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
    macro_rules! request_body {
        ($req:expr) => {
            match read_request_body($req).await {
                Ok(body) => body,
                Err(response) => return Ok(response),
            }
        };
    }
    macro_rules! request_json {
        ($req:expr, $ty:ty, $label:literal) => {
            match read_json_request::<$ty>($req, $label).await {
                Ok(parsed) => parsed,
                Err(response) => return Ok(response),
            }
        };
    }

    if method == Method::GET && path.starts_with("/viz/") {
        if path == "/viz/" || path == "/viz/index.html" {
            return match handle_viz_get(&state, req.uri().query()).await {
                Ok(r) => Ok(r),
                Err(e) => Ok(json_error(StatusCode::BAD_REQUEST, &e.to_string())),
            };
        }
        return Ok(match handle_viz_static_get(&path).await {
            Ok(r) => r,
            Err(e) => json_error(StatusCode::NOT_FOUND, &e.to_string()),
        });
    }

    let resp = match (method, path.as_str()) {
        (Method::GET, "/healthz") => text_response(StatusCode::OK, "ok\n"),
        (Method::GET, "/status") => match status_payload(&state) {
            Ok(v) => json_response(StatusCode::OK, &v),
            Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        },
        (Method::GET, "/capabilities") => match capabilities_payload(&state) {
            Ok(v) => json_response(StatusCode::OK, &v),
            Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        },
        (Method::GET, "/snapshots") => match snapshots_payload(&state, req.uri().query()) {
            Ok(v) => json_response(StatusCode::OK, &v),
            Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
        },
        (Method::GET, "/entity/describe") => {
            match handle_entity_describe_get(&state, req.uri().query()).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::GET, "/docchunk/get") => {
            match handle_docchunk_get(&state, req.uri().query()).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::GET, "/contexts") => match handle_contexts_get(&state).await {
            Ok(v) => json_response(StatusCode::OK, &v),
            Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
        },
        (Method::GET, "/viz") => {
            let mut location = String::from("/viz/");
            if let Some(q) = req.uri().query() {
                location.push('?');
                location.push_str(q);
            }
            Response::builder()
                .status(StatusCode::FOUND)
                .header("location", location)
                .body(Full::new(Bytes::new()))
                .unwrap_or_else(|_e| {
                    text_response(StatusCode::INTERNAL_SERVER_ERROR, "viz redirect failed\n")
                })
        }
        (Method::GET, "/viz.json") => {
            match handle_viz_get_as(&state, req.uri().query(), "json").await {
                Ok(r) => r,
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::GET, "/viz.dot") => {
            match handle_viz_get_as(&state, req.uri().query(), "dot").await {
                Ok(r) => r,
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/query") => {
            let body = request_body!(req);
            match handle_query(&state, &body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/llm/to_query") => {
            let body = request_body!(req);
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
            let parsed = request_json!(req, LlmAgentRequestV1, "llm/agent");
            if parsed.auto_commit {
                if let Err(resp) = require_admin_auth_header(auth_header.as_deref(), state.as_ref())
                {
                    return Ok(resp);
                }
            }
            match handle_llm_agent(&state, parsed).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/world_model/propose") => {
            let auth_header = req
                .headers()
                .get(AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            let parsed = request_json!(req, WorldModelProposeRequestV1, "world_model/propose");
            if parsed.auto_commit {
                if let Err(resp) = require_admin_auth_header(auth_header.as_deref(), state.as_ref())
                {
                    return Ok(resp);
                }
            }
            match handle_world_model_propose(&state, parsed).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/world_model/plan") => {
            let auth_header = req
                .headers()
                .get(AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            let parsed = request_json!(req, WorldModelPlanRequestV1, "world_model/plan");
            if parsed.auto_commit {
                if let Err(resp) = require_admin_auth_header(auth_header.as_deref(), state.as_ref())
                {
                    return Ok(resp);
                }
            }
            match handle_world_model_plan(&state, parsed).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/discover/draft-axi") => {
            let body = request_body!(req);
            match handle_discover_draft_axi(&state, &body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/discover/check-olog") => {
            let body = request_body!(req);
            match handle_discover_check_olog(&body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/semantic/coverage") => {
            let body = request_body!(req);
            match handle_semantic_coverage(&state, &body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/semantic/business-rule") => {
            let body = request_body!(req);
            match handle_semantic_business_rule(&state, &body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/semantic/agent-report") => {
            let body = request_body!(req);
            match handle_semantic_agent_report(&state, &body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/semantic/context-report") => {
            let body = request_body!(req);
            match handle_semantic_context_report(&state, &body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/semantic/behavior-case") => {
            let body = request_body!(req);
            match handle_semantic_behavior_case(&state, &body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/semantic/overlay-check") => {
            let body = request_body!(req);
            match handle_semantic_overlay_check(&body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/semantic/software-coverage") => {
            let body = request_body!(req);
            match handle_semantic_software_coverage(&body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/semantic/codegen-plan") => {
            let body = request_body!(req);
            match handle_semantic_codegen_plan(&body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/semantic/coverage-query") => {
            let body = request_body!(req);
            match handle_semantic_coverage_query(&body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/semantic/definition-query") => {
            let body = request_body!(req);
            match handle_semantic_definition_query(&body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/semantic/theory-check") => {
            let body = request_body!(req);
            match handle_semantic_theory_check(&body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/proposals/relation") => {
            let body = request_body!(req);
            match handle_proposals_relation(&state, &body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/proposals/relations") => {
            let body = request_body!(req);
            match handle_proposals_relations(&state, &body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/viz") => {
            let body = request_body!(req);
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
            let body = request_body!(req);
            match handle_promote(&state, &body).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
            }
        }
        (Method::POST, "/admin/accept/pathdb-commit") => {
            if let Err(e) = require_admin(&req, &state) {
                return Ok(e);
            }
            let body = request_body!(req);
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

const MAX_JSON_REQUEST_BODY_BYTES: usize = 16 * 1024 * 1024;

async fn read_request_body(
    req: Request<Incoming>,
) -> std::result::Result<Bytes, Response<Full<Bytes>>> {
    Limited::new(req.into_body(), MAX_JSON_REQUEST_BODY_BYTES)
        .collect()
        .await
        .map(|body| body.to_bytes())
        .map_err(|err| {
            json_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                &format!(
                    "request body exceeds {} byte limit or failed to read: {err}",
                    MAX_JSON_REQUEST_BODY_BYTES
                ),
            )
        })
}

fn parse_json_request<T: DeserializeOwned>(body: &[u8], label: &str) -> Result<T> {
    serde_json::from_slice(body).map_err(|e| anyhow!("failed to parse {label} request JSON: {e}"))
}

async fn read_json_request<T: DeserializeOwned>(
    req: Request<Incoming>,
    label: &str,
) -> std::result::Result<T, Response<Full<Bytes>>> {
    let body = read_request_body(req).await?;
    parse_json_request(&body, label)
        .map_err(|e| json_error(StatusCode::BAD_REQUEST, &e.to_string()))
}

fn json_response<T: Serialize>(status: StatusCode, value: &T) -> Response<Full<Bytes>> {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{\"error\":\"serialize\"}".to_vec());
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(body)))
        .unwrap_or_else(|_| {
            Response::new(Full::new(Bytes::from_static(b"{\"error\":\"internal\"}")))
        })
}

fn json_error(status: StatusCode, msg: &str) -> Response<Full<Bytes>> {
    let v = serde_json::json!({ "error": msg });
    json_response(status, &v)
}

fn require_admin(
    req: &Request<Incoming>,
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

    let Some(header) = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    else {
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
    let world_model_backend = match &state.config.world_model.backend {
        WorldModelBackend::Disabled => "disabled".to_string(),
        WorldModelBackend::Stub => "stub".to_string(),
        WorldModelBackend::Command { program, .. } => format!("command({})", program.display()),
        WorldModelBackend::Http { url } => format!("http({url})"),
    };
    let verifier_bin = resolve_verifier_bin(&state.config.cert_verify);
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
        "world_model": {
            "enabled": !matches!(state.config.world_model.backend, WorldModelBackend::Disabled),
            "backend": world_model_backend,
            "model": state.config.world_model.model.clone(),
            "status": state.config.world_model.status_line(),
        },
        "certificates": {
            "lean_verifier_available": verifier_bin.is_some(),
            "lean_verifier_bin": verifier_bin.as_ref().map(|p| p.display().to_string()),
            "lean_verifier_timeout_secs": state.config.cert_verify.timeout.map(|d| d.as_secs()),
        },
    }))
}

fn capabilities_payload(state: &ServerState) -> Result<serde_json::Value> {
    let loaded = state
        .loaded
        .read()
        .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?;

    let store_ctx = match &state.config.source {
        SnapshotSource::Store { dir, layer, .. } => Some(crate::llm::ToolLoopStoreContext {
            dir: dir.clone(),
            default_layer: layer.clone(),
        }),
        _ => None,
    };
    let tool_loop_tools = crate::llm::tool_loop_tools_schema(
        store_ctx.as_ref(),
        !matches!(
            state.config.world_model.backend,
            WorldModelBackend::Disabled
        ),
    );
    let mut semantic_services = vec![
        serde_json::json!({
            "name": "discover_check_olog",
            "endpoint": "/discover/check-olog",
            "returns": ["typed_holes", "refinement_candidates", "evolution_preview"]
        }),
        serde_json::json!({
            "name": "semantic_coverage",
            "endpoint": "/semantic/coverage",
            "returns": ["coverage", "rule_statuses", "uncovered_rule_ids"]
        }),
        serde_json::json!({
            "name": "semantic_business_rule",
            "endpoint": "/semantic/business-rule",
            "returns": ["report", "rules", "next_actions"]
        }),
        serde_json::json!({
            "name": "semantic_agent_report",
            "endpoint": "/semantic/agent-report",
            "returns": ["report", "matched_scope_ids", "matched_scope_refs", "matched_rule_ids"]
        }),
        serde_json::json!({
            "name": "semantic_context_report",
            "endpoint": "/semantic/context-report",
            "returns": ["report", "rule_reports", "coverage", "competency_coverage"]
        }),
        serde_json::json!({
            "name": "semantic_behavior_case",
            "endpoint": "/semantic/behavior-case",
            "returns": ["report", "case_receipt", "codegen_previews", "context_report"]
        }),
        serde_json::json!({
            "name": "semantic_theory_check",
            "endpoint": "/semantic/theory-check",
            "returns": ["runtime_theory_check_report", "closure", "closure_trace", "transport_summary", "completeness_claim", "ontology_closure_claim"]
        }),
        serde_json::json!({
            "name": "proposals_relation",
            "endpoint": "/proposals/relation",
            "returns": ["proposals_json", "validation"]
        }),
        serde_json::json!({
            "name": "proposals_relations",
            "endpoint": "/proposals/relations",
            "returns": ["proposals_json", "validation"]
        }),
    ];
    if store_ctx.is_some() {
        semantic_services.push(serde_json::json!({
            "name": "promote_reviewed_module",
            "endpoint": "/admin/accept/promote",
            "returns": ["snapshot_id", "validation_report_path", "stored_report_path"]
        }));
    }

    Ok(serde_json::json!({
        "version": "axiograph_capabilities_v1",
        "snapshot": {
            "snapshot_key": loaded.snapshot_key,
            "accepted_snapshot_id": loaded.accepted_snapshot_id,
            "pathdb_snapshot_id": loaded.pathdb_snapshot_id,
        },
        "query": {
            "machine_contract": "query_ir_v1",
            "raw_query_text_supported": false,
            "prepared_query_wrapper": "PreparedQueryV1",
            "trust_contract_fields": ["trust_class", "soundness", "coverage", "scope"],
        },
        "semantic_services": semantic_services,
        "tool_loop": {
            "contract": "tool_specs_v1",
            "tools": tool_loop_tools,
        },
        "trust_classes": ["runtime_enforced", "runtime_advisory", "review_only"],
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
                out.insert(ev.snapshot_id.to_string(), msg);
            }
            continue;
        }
        // Then PathDB WAL event.
        if let Ok(ev) = serde_json::from_str::<PathDbWalEventV1>(line) {
            if let Some(msg) = ev.message {
                out.insert(ev.snapshot_id.to_string(), msg);
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
            let msg = messages.get(snap.snapshot_id.as_str()).cloned();
            entries.push(SnapshotEntryV1 {
                snapshot_id: snap.snapshot_id.to_string(),
                previous_snapshot_id: snap.previous_snapshot_id.map(|id| id.to_string()),
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
            let msg = messages.get(snap.snapshot_id.as_str()).cloned();
            entries.push(SnapshotEntryV1 {
                snapshot_id: snap.snapshot_id.to_string(),
                previous_snapshot_id: snap.previous_snapshot_id.map(|id| id.to_string()),
                created_at_unix_secs: snap.created_at_unix_secs,
                message: msg,
                accepted_snapshot_id: Some(snap.accepted_snapshot_id.to_string()),
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
#[serde(deny_unknown_fields)]
struct QueryRequestV1 {
    #[serde(default)]
    query: Option<String>,
    /// Structured typed query surface required for HTTP/tooling.
    #[serde(default)]
    query_ir_v1: Option<crate::query_ir::QueryIrV1>,
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
    /// Shared query certificate policy: none|emit|verify|require_verified.
    ///
    /// `require_verified` is fail-closed and requires a store-backed accepted
    /// `.axi` anchor plus canonical text.
    #[serde(default)]
    certificate_policy: Option<crate::query_ir::QueryCertificatePolicyV1>,
    /// Optional snapshot id override when running in store-backed mode.
    ///
    /// If set, the server will load and query that snapshot for this request
    /// (does not affect the currently loaded snapshot for other requests).
    #[serde(default)]
    snapshot: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct LlmToQueryRequestV1 {
    question: String,
    /// Optional snapshot id override when running in store-backed mode.
    #[serde(default)]
    snapshot: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// Omit to use the accepted snapshot backing the currently loaded PathDB snapshot.
    #[serde(default)]
    accepted_snapshot: Option<AcceptedSnapshotId>,
    /// Optional message to attach to the WAL commit (audit log).
    #[serde(default)]
    commit_message: Option<String>,
    /// Shared query certificate policy for tool-loop `axql_run` steps:
    /// none|emit|verify|require_verified.
    #[serde(default)]
    query_certificate_policy: Option<crate::query_ir::QueryCertificatePolicyV1>,
    /// Optional snapshot id override when running in store-backed mode.
    #[serde(default)]
    snapshot: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct WorldModelProposeRequestV1 {
    /// Optional goals/targets for the world model (free-form).
    #[serde(default)]
    goals: Vec<String>,
    /// Optional canonical `.axi` module name to export and feed into the world model.
    #[serde(default)]
    axi_module: Option<String>,
    /// Optional seed passed to the world model.
    #[serde(default)]
    seed: Option<u64>,
    /// Max new proposals to keep (0 = no cap).
    #[serde(default)]
    max_new_proposals: Option<usize>,
    /// Guardrail profile: off|fast|strict.
    #[serde(default)]
    guardrail_profile: Option<String>,
    /// Guardrail plane: meta|data|both.
    #[serde(default)]
    guardrail_plane: Option<String>,
    /// Optional guardrail weight overrides.
    #[serde(default)]
    guardrail_weights: Option<crate::world_model::GuardrailCostWeightsV1>,
    /// Task costs (objective terms) passed to the world model.
    #[serde(default)]
    task_costs: Vec<crate::world_model::WorldModelTaskCostV1>,
    /// Optional planning horizon (steps) passed to the world model.
    #[serde(default)]
    horizon_steps: Option<usize>,
    /// Include guardrail report in the response (default: true).
    #[serde(default)]
    include_guardrail: Option<bool>,
    /// Auto-commit proposals into the PathDB WAL.
    #[serde(default)]
    auto_commit: bool,
    /// Optional accepted-plane snapshot id override for WAL commits.
    ///
    /// Omit to use the accepted snapshot backing the currently loaded store snapshot.
    #[serde(default)]
    accepted_snapshot: Option<AcceptedSnapshotId>,
    /// Optional message to attach to the WAL commit (audit log).
    #[serde(default)]
    commit_message: Option<String>,
    /// Validate proposals before committing (default: true).
    #[serde(default)]
    validate: Option<bool>,
    /// Validation quality profile: off|fast|strict.
    #[serde(default)]
    quality: Option<String>,
    /// Validation plane: meta|data|both.
    #[serde(default)]
    quality_plane: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct WorldModelPlanRequestV1 {
    /// Optional goals/targets for the world model (free-form).
    #[serde(default)]
    goals: Vec<String>,
    /// Optional canonical `.axi` module name to export and feed into the world model.
    #[serde(default)]
    axi_module: Option<String>,
    /// Optional seed passed to the world model.
    #[serde(default)]
    seed: Option<u64>,
    /// Max new proposals to keep per step (0 = no cap).
    #[serde(default)]
    max_new_proposals: Option<usize>,
    /// Planning horizon (steps).
    #[serde(default)]
    horizon_steps: Option<usize>,
    /// Number of rollouts per step.
    #[serde(default)]
    rollouts: Option<usize>,
    /// Guardrail profile: off|fast|strict.
    #[serde(default)]
    guardrail_profile: Option<String>,
    /// Guardrail plane: meta|data|both.
    #[serde(default)]
    guardrail_plane: Option<String>,
    /// Optional guardrail weight overrides.
    #[serde(default)]
    guardrail_weights: Option<crate::world_model::GuardrailCostWeightsV1>,
    /// Task costs (objective terms) passed to the world model.
    #[serde(default)]
    task_costs: Vec<crate::world_model::WorldModelTaskCostV1>,
    /// Include guardrail report in the world model input (default: true).
    #[serde(default)]
    include_guardrail: Option<bool>,
    /// Optional competency questions (AxQL) for coverage-driven cost.
    #[serde(default)]
    competency_questions: Vec<crate::world_model::CompetencyQuestionV1>,
    /// Auto-commit aggregated proposals into the PathDB WAL.
    #[serde(default)]
    auto_commit: bool,
    /// If true, commit each step separately and reload between steps.
    #[serde(default)]
    commit_stepwise: bool,
    /// Optional accepted-plane snapshot id override for WAL commits.
    ///
    /// Omit to use the accepted snapshot backing the currently loaded store snapshot.
    #[serde(default)]
    accepted_snapshot: Option<AcceptedSnapshotId>,
    /// Optional message to attach to the WAL commit (audit log).
    #[serde(default)]
    commit_message: Option<String>,
    /// Validate proposals before committing (default: true).
    #[serde(default)]
    validate: Option<bool>,
    /// Validation quality profile: off|fast|strict.
    #[serde(default)]
    quality: Option<String>,
    /// Validation plane: meta|data|both.
    #[serde(default)]
    quality_plane: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct WorldModelProposeResponseV1 {
    version: String,
    trace_id: WorldModelRunId,
    proposals: axiograph_ingest_docs::ProposalsFileV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    guardrail: Option<crate::world_model::GuardrailCostReportV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    commit: Option<PathdbCommitResponseV1>,
    #[serde(default)]
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct WorldModelPlanResponseV1 {
    version: String,
    report: crate::world_model::WorldModelPlanReportV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    commit: Option<PathdbCommitResponseV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    commit_steps: Option<Vec<PathdbCommitResponseV1>>,
}

#[derive(Debug, Clone, Deserialize)]
struct ChatMessageV1 {
    role: String, // "user" | "assistant" | "system" (best-effort)
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
    /// Include all nodes (ignores focus + hops).
    #[serde(default)]
    all: Option<bool>,
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
            &format!(
                "\n<meta http-equiv=\"refresh\" content=\"{}\"/>",
                refresh_secs
            ),
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
    trust: crate::trust_contract::TrustContractV1,
    certificate_policy: crate::query_ir::QueryCertificatePolicyV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    prepared_query: Option<crate::query_ir::PreparedQueryMetadataV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    compiled_query_ir_v1: Option<crate::query_ir::QueryIrV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    elaborated_query_ir_v1: Option<crate::query_ir::QueryIrV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    elaborated_query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inferred_types: Option<BTreeMap<String, Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    typed_holes: Option<Vec<crate::axql::AxqlTypedHoleV1>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exploration_suggestions: Option<Vec<crate::axql::AxqlExplorationSuggestionV1>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    accepted_axi_anchor: Option<AcceptedAxiAnchor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    support_summary: Option<crate::evidence_support::EvidenceSupportSummaryV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    anchor_digest: Option<AxiDigest>,
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

fn parse_default_context_specs(contexts_raw: &[String]) -> Vec<crate::axql::AxqlContextSpec> {
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
    contexts
}

fn query_request_to_axql_query(req: &QueryRequestV1) -> Result<crate::axql::AxqlQuery> {
    let lang = req
        .lang
        .as_deref()
        .unwrap_or("query_ir_v1")
        .trim()
        .to_ascii_lowercase();

    if req.query.is_some() {
        return Err(anyhow!(
            "raw `query` text is no longer accepted at `/query`; send structured `query_ir_v1`"
        ));
    }

    let Some(ir) = req.query_ir_v1.as_ref() else {
        return Err(anyhow!("query request requires `query_ir_v1`"));
    };

    if lang != "query_ir_v1" {
        return Err(anyhow!(
            "unsupported lang `{}` for `/query` (expected `query_ir_v1`)",
            lang
        ));
    }

    let mut parsed = ir.to_axql_query()?;

    if parsed.contexts.is_empty() && !req.contexts.is_empty() {
        parsed.contexts = parse_default_context_specs(&req.contexts);
    }

    Ok(parsed)
}

fn kernel_module_ir_from_canonical_axi_text_for_query_metadata(
    axi_text: &str,
) -> Option<axiograph_pathdb::kernel_ir::KernelModuleIr> {
    let canonical = crate::axi_input::require_canonical_axi_text(axi_text).ok()?;
    axiograph_pathdb::compile_kernel_module_ir(canonical.module().module(), axi_text).ok()
}

fn query_request_certificate_policy(
    req: &QueryRequestV1,
) -> crate::query_ir::QueryCertificatePolicyV1 {
    req.certificate_policy.unwrap_or_default()
}

fn llm_agent_query_certificate_policy(
    req: &LlmAgentRequestV1,
) -> crate::query_ir::QueryCertificatePolicyV1 {
    req.query_certificate_policy.unwrap_or_default()
}

fn require_query_certificate_refusal(policy: crate::query_ir::QueryCertificatePolicyV1) -> String {
    if policy.requires_verified() {
        "Refusing to answer: required verified query certificate gate failed.".to_string()
    } else {
        "Refusing to answer: certificate gate failed.".to_string()
    }
}

fn mark_query_certificate_gate_refusal(
    outcome: &mut crate::llm::ToolLoopOutcome,
    policy: crate::query_ir::QueryCertificatePolicyV1,
) {
    outcome.final_answer.answer = require_query_certificate_refusal(policy);
    outcome.final_answer.citations.clear();
    outcome.final_answer.queries.clear();
    outcome
        .final_answer
        .notes
        .push("gate: query_certificate_policy".to_string());
    if policy.requires_verified() {
        outcome
            .final_answer
            .notes
            .push("gate: require_verified".to_string());
    }
}

async fn handle_query(state: &Arc<ServerState>, body: &[u8]) -> Result<QueryResponseV1> {
    let req: QueryRequestV1 = parse_json_request(body, "query")?;
    let show_elaboration = req.show_elaboration;
    let certificate_policy = query_request_certificate_policy(&req);
    let want_cert = certificate_policy.emits_certificate();
    let want_verify = certificate_policy.verifies_certificate();
    let snapshot_override = req.snapshot.clone();
    let state = state.clone();

    tokio::task::spawn_blocking(move || {
        let (db, meta, snapshot_key, accepted_axi_anchor, accepted_axi_text) =
            if let Some(snapshot) = snapshot_override.as_deref() {
                let SnapshotSource::Store { dir, layer, .. } = &state.config.source else {
                    return Err(anyhow!(
                        "query snapshot override requires a store-backed server (`--dir ...`)"
                    ));
                };
                let loaded = load_from_store(dir, layer, snapshot, &state.config)?;
                (
                    loaded.db,
                    loaded.meta,
                    loaded.snapshot_key,
                    loaded.accepted_axi_anchor,
                    loaded.accepted_axi_text,
                )
            } else {
                let loaded = state
                    .loaded
                    .read()
                    .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?;
                (
                    loaded.db.clone(),
                    loaded.meta.clone(),
                    loaded.snapshot_key.clone(),
                    loaded.accepted_axi_anchor.clone(),
                    loaded.accepted_axi_text.clone(),
                )
            };

        let parsed = query_request_to_axql_query(&req)?;
        let query_ir_v1 = crate::query_ir::QueryIrV1::from_axql_query(&parsed);
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
                let prepared = query_ir_v1.prepare_with_meta(&db, meta.as_ref())?;
                let prepared = Arc::new(Mutex::new(prepared));
                cache.insert(cache_key.clone(), prepared.clone());
                prepared
            }
        };

        let mut prepared = prepared
            .lock()
            .map_err(|_| anyhow!("prepared query lock poisoned"))?;
        let compiled_query_ir_v1 = show_elaboration.then_some(query_ir_v1.clone());
        let elaborated_query = show_elaboration.then(|| prepared.elaborated_query_text());
        let elaborated_query_ir_v1 = if show_elaboration {
            Some(prepared.elaborated_query_ir_v1()?)
        } else {
            None
        };
        let elaboration = show_elaboration.then(|| prepared.elaboration_report().clone());
        let plan = show_elaboration.then(|| prepared.explain_plan_lines());
        let certifiability = prepared.certifiability();
        let query_kernel = accepted_axi_text
            .as_deref()
            .and_then(kernel_module_ir_from_canonical_axi_text_for_query_metadata);
        let prepared_query = Some(if let Some(kernel) = query_kernel.as_ref() {
            prepared.metadata_with_meta_and_kernel(meta.as_ref(), kernel)?
        } else {
            prepared.metadata_with_meta(meta.as_ref())?
        });
        certificate_policy.ensure_require_verified_preconditions(
            &certifiability,
            accepted_axi_anchor.as_ref(),
            accepted_axi_text.as_deref(),
        )?;

        let mut accepted_query_anchor: Option<AcceptedAxiAnchor> = None;
        let mut rows: Vec<BTreeMap<String, EntityViewV1>> = Vec::new();
        let vars;
        let truncated;
        let elapsed_ms;
        let mut anchor_digest: Option<AxiDigest> = None;
        let mut support_summary: Option<crate::evidence_support::EvidenceSupportSummaryV1> = None;
        let mut support_certificate: Option<axiograph_pathdb::certificate::CertificateV2> = None;
        let mut support_certificate_emitted_to_client = false;
        let mut support_certificate_verified: Option<bool> = None;
        let mut certificate: Option<serde_json::Value> = None;
        let mut certificate_verified: Option<bool> = None;
        let mut certificate_verify_output: Option<String> = None;

        if let Some(accepted_axi_anchor) = accepted_axi_anchor {
            let validated = prepared
                .bind_accepted_axi_anchor(accepted_axi_anchor.clone())
                .execute_answer(&db, meta.as_ref())?;
            elapsed_ms = start.elapsed().as_millis();
            vars = validated.result().selected_vars.clone();
            truncated = validated.result().truncated;
            accepted_query_anchor = Some(validated.accepted_axi_anchor().clone());
            for row in &validated.result().rows {
                let mut out: BTreeMap<String, EntityViewV1> = BTreeMap::new();
                for (k, id) in row {
                    out.insert(k.clone(), EntityViewV1::from_id(&db, *id));
                }
                rows.push(out);
            }

            if want_cert {
                let certified = prepared
                    .bind_accepted_axi_anchor(accepted_axi_anchor.clone())
                    .certify_answer(validated, &db, meta.as_ref())?;
                if certifiability.is_certifiable() {
                    support_certificate = Some(certified.certificate().clone());
                    support_certificate_emitted_to_client = true;
                }

                let digest = certified.anchor_digest().clone();
                anchor_digest = Some(digest.clone());
                let cert = certified
                    .certificate()
                    .clone()
                    .with_anchor(axiograph_pathdb::certificate::AxiAnchorV1::new(digest));
                let cert_json = serde_json::to_value(&cert)?;
                certificate = Some(cert_json);

                if want_verify {
                    let axi = accepted_axi_text.as_deref().ok_or_else(|| {
                        anyhow!(
                            "accepted anchor bound query certificate is missing canonical `.axi` text"
                        )
                    })?;
                    let cert_text = serde_json::to_string_pretty(&cert)?;
                    let (ok, out) = verify_certificate_with_lean(
                        &state.config.cert_verify,
                        axi,
                        &cert_text,
                    )?;
                    certificate_verified = Some(ok);
                    certificate_verify_output = Some(out);
                    support_certificate_verified = Some(ok);
                    certificate_policy.ensure_verified_result(certificate_verified)?;
                }
            } else if certifiability.is_certifiable() {
                if let Ok(certified) = prepared
                    .bind_accepted_axi_anchor(accepted_axi_anchor.clone())
                    .certify_answer(validated, &db, meta.as_ref())
                {
                    support_certificate = Some(certified.certificate().clone());
                    support_certificate_emitted_to_client = false;
                }
            }
        } else {
            let answer = prepared.execute_answer(&db, meta.as_ref())?;
            elapsed_ms = start.elapsed().as_millis();
            vars = answer.result().selected_vars.clone();
            truncated = answer.result().truncated;
            for row in &answer.result().rows {
                let mut out: BTreeMap<String, EntityViewV1> = BTreeMap::new();
                for (k, id) in row {
                    out.insert(k.clone(), EntityViewV1::from_id(&db, *id));
                }
                rows.push(out);
            }

            if want_cert {
                let (digest, axi) = export_canonical_module_axi(&db)?;
                let certified_answer =
                    prepared.certify_answer_with_anchor(answer, &db, meta.as_ref(), digest.clone())?;
                anchor_digest = Some(certified_answer.anchor_digest().clone());
                let cert = certified_answer
                    .certificate()
                    .clone()
                    .with_anchor(axiograph_pathdb::certificate::AxiAnchorV1::new(digest));
                let cert_json = serde_json::to_value(&cert)?;
                certificate = Some(cert_json);

                if want_verify {
                    let cert_text = serde_json::to_string_pretty(&cert)?;
                    let (ok, out) = verify_certificate_with_lean(
                        &state.config.cert_verify,
                        &axi,
                        &cert_text,
                    )?;
                    certificate_verified = Some(ok);
                    certificate_verify_output = Some(out);
                    certificate_policy.ensure_verified_result(certificate_verified)?;
                }
            }
        }

        let query_trust = crate::trust_contract::query_trust_contract_with_meta(
            &parsed,
            &certifiability,
            certificate.is_some(),
            certificate_verified,
            meta.as_ref(),
        );

        if let (Some(anchor), Some(cert)) = (accepted_query_anchor.clone(), support_certificate.as_ref()) {
            let support_trust = crate::trust_contract::query_trust_contract_with_meta(
                &parsed,
                &certifiability,
                support_certificate_emitted_to_client,
                support_certificate_verified,
                meta.as_ref(),
            );
            support_summary = crate::evidence_support::evidence_support_summary_from_certificate(
                &db,
                anchor,
                support_trust,
                cert,
                support_certificate_emitted_to_client,
                support_certificate_verified,
            );
        }

        Ok(QueryResponseV1 {
            vars,
            rows,
            truncated,
            elapsed_ms,
            trust: query_trust,
            certificate_policy,
            prepared_query,
            compiled_query_ir_v1,
            elaborated_query_ir_v1,
            elaborated_query,
            inferred_types: elaboration.as_ref().map(|e| e.inferred_types.clone()),
            notes: elaboration.as_ref().map(|e| e.notes.clone()),
            typed_holes: elaboration.as_ref().map(|e| e.typed_holes.clone()),
            exploration_suggestions: elaboration
                .as_ref()
                .map(|e| e.exploration_suggestions.clone()),
            plan,
            accepted_axi_anchor: accepted_query_anchor,
            support_summary,
            anchor_digest,
            certificate,
            certificate_verified,
            certificate_verify_output,
        })
    })
    .await
    .map_err(|e| anyhow!("query task join failed: {e}"))?
}

async fn handle_llm_to_query(state: &Arc<ServerState>, body: &[u8]) -> Result<serde_json::Value> {
    let req: LlmToQueryRequestV1 = parse_json_request(body, "llm/to_query")?;

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
            let loaded = load_from_store(dir, layer, snapshot, &state.config)?;
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
            GeneratedQuery::QueryIrV1(ir) => serde_json::json!({
                "version": "axiograph_db_server_llm_to_query_v1",
                "query_ir_v1": ir
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
    snapshot_id: Option<PathdbSnapshotId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    accepted_snapshot_id: Option<AcceptedSnapshotId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ops_added: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

async fn handle_llm_agent(
    state: &Arc<ServerState>,
    req: LlmAgentRequestV1,
) -> Result<serde_json::Value> {
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
    let query_certificate_policy = llm_agent_query_certificate_policy(&req);
    let require_verified_query_gate = query_certificate_policy.requires_verified();
    let verify_query_certificates = query_certificate_policy.verifies_certificate();
    let emit_query_certificates = query_certificate_policy.emits_certificate();
    let max_steps_cap = crate::llm::llm_max_steps_cap()?;

    let state2 = state.clone();
    let (mut outcome, accepted_snapshot_id, query_certs) = tokio::task::spawn_blocking(move || {
        let (
            db,
            meta,
            embeddings,
            snapshot_key,
            accepted_snapshot_id,
            accepted_axi_anchor,
            accepted_axi_text,
            pathdb_snapshot_id,
            snapshot_label,
        ) = if let Some(snapshot) = snapshot_override.as_deref() {
            let SnapshotSource::Store { dir, layer, .. } = &state2.config.source else {
                return Err(anyhow!(
                    "llm snapshot override requires a store-backed server (`--dir ...`)"
                ));
            };
            let loaded = load_from_store(dir, layer, snapshot, &state2.config)?;
            (
                loaded.db,
                loaded.meta,
                loaded.embeddings,
                loaded.snapshot_key,
                loaded.accepted_snapshot_id,
                loaded.accepted_axi_anchor,
                loaded.accepted_axi_text,
                loaded.pathdb_snapshot_id,
                loaded.snapshot_label,
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
                loaded.accepted_axi_anchor.clone(),
                loaded.accepted_axi_text.clone(),
                loaded.pathdb_snapshot_id.clone(),
                loaded.snapshot_label.clone(),
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

        let world_model_ctx = if matches!(
            state2.config.world_model.backend,
            WorldModelBackend::Disabled
        ) {
            None
        } else {
            Some(crate::llm::ToolLoopWorldModelContext {
                world_model: state2.config.world_model.clone(),
                pathdb_snapshot_id: pathdb_snapshot_id.clone(),
                accepted_snapshot_id: accepted_snapshot_id.clone(),
                snapshot_label: snapshot_label.clone(),
            })
        };

        let outcome = crate::llm::run_tool_loop_with_meta(
            &state2.config.llm,
            &db,
            meta.as_ref(),
            &contexts,
            &snapshot_key,
            accepted_snapshot_id.as_ref(),
            accepted_axi_anchor.as_ref(),
            store_ctx.as_ref(),
            world_model_ctx.as_ref(),
            embeddings.as_deref(),
            embed_host,
            &mut query_cache,
            &full_question,
            opts,
        )?;

        // Optional: certify (and optionally verify) queries executed by the tool loop.
        let mut query_certs: Option<Vec<serde_json::Value>> = None;
        if emit_query_certificates {
            let want_verify = verify_query_certificates;
            let accepted_query_anchor = accepted_axi_anchor.clone();
            let accepted_axi_text = accepted_axi_text.clone();
            let accepted_query_kernel = accepted_axi_text
                .as_deref()
                .and_then(kernel_module_ir_from_canonical_axi_text_for_query_metadata);
            let mut exported_axi: Option<(AxiDigest, String)> = None;
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

                let parsed = match crate::axql::parse_axql_query(&q) {
                    Ok(parsed) => parsed,
                    Err(e) => {
                        out.push(serde_json::json!({
                            "step_index": i,
                            "query": q,
                            "certificate_policy": query_certificate_policy,
                            "error": e.to_string(),
                        }));
                        continue;
                    }
                };
                let query_ir_v1 = crate::query_ir::QueryIrV1::from_axql_query(&parsed);
                let prepared = match query_ir_v1.prepare_with_meta(&db, meta.as_ref()) {
                    Ok(prepared) => prepared,
                    Err(e) => {
                        out.push(serde_json::json!({
                            "step_index": i,
                            "query": q,
                            "certificate_policy": query_certificate_policy,
                            "error": e.to_string(),
                        }));
                        continue;
                    }
                };
                let prepared_query = if let Some(kernel) = accepted_query_kernel.as_ref() {
                    prepared.metadata_with_meta_and_kernel(meta.as_ref(), kernel)?
                } else {
                    prepared.metadata_with_meta(meta.as_ref())?
                };
                if let Err(e) = query_certificate_policy.ensure_require_verified_preconditions(
                    &prepared_query.certifiability,
                    accepted_query_anchor.as_ref(),
                    accepted_axi_text.as_deref(),
                ) {
                    out.push(serde_json::json!({
                        "step_index": i,
                        "query": q,
                        "certificate_policy": query_certificate_policy,
                        "prepared_query": prepared_query,
                        "error": e.to_string(),
                    }));
                    continue;
                }

                let (digest, axi_for_verify, accepted_anchor_for_entry) =
                    if let Some(anchor) = accepted_query_anchor.as_ref() {
                        (
                            anchor.axi_digest.clone(),
                            accepted_axi_text.as_deref(),
                            Some(anchor.clone()),
                        )
                    } else {
                        if exported_axi.is_none() {
                            exported_axi = Some(export_canonical_module_axi(&db)?);
                        }
                        let (digest, axi) = exported_axi.as_ref().expect("exported axi is set");
                        (digest.clone(), Some(axi.as_str()), None)
                    };

                match prepared.certify_typed_with_anchor(&db, meta.as_ref(), digest.as_str()) {
                    Ok(cert) => {
                        let cert = cert.with_anchor(
                            axiograph_pathdb::certificate::AxiAnchorV1::new(digest.clone()),
                        );
                        let cert_json =
                            serde_json::to_value(&cert).unwrap_or(serde_json::Value::Null);

                        let (verified, verify_out, verify_err) = if want_verify {
                            match axi_for_verify {
                                Some(axi) if !axi.trim().is_empty() => {
                                    let cert_text = serde_json::to_string_pretty(&cert)?;
                                    match verify_certificate_with_lean(
                                        &state2.config.cert_verify,
                                        axi,
                                        &cert_text,
                                    ) {
                                        Ok((ok, out_text)) => (Some(ok), Some(out_text), None),
                                        Err(e) => (Some(false), None, Some(e.to_string())),
                                    }
                                }
                                _ => (
                                    Some(false),
                                    None,
                                    Some(
                                        "accepted anchor bound tool-loop query certification is missing canonical `.axi` text"
                                            .to_string(),
                                    ),
                                ),
                            }
                        } else {
                            (None, None, None)
                        };
                        let require_verified_error = query_certificate_policy
                            .ensure_verified_result(verified)
                            .err()
                            .map(|e| e.to_string());

                        let mut entry = serde_json::json!({
                            "step_index": i,
                            "query": q,
                            "certificate_policy": query_certificate_policy,
                            "prepared_query": prepared_query,
                            "accepted_axi_anchor": accepted_anchor_for_entry,
                            "certificate": cert_json,
                            "certificate_verified": verified,
                            "certificate_verify_output": verify_out,
                            "certificate_verify_error": verify_err,
                        });
                        if let Some(error) = require_verified_error {
                            entry["error"] = serde_json::Value::String(error);
                        }
                        out.push(entry);
                    }
                    Err(e) => {
                        out.push(serde_json::json!({
                            "step_index": i,
                            "query": q,
                            "certificate_policy": query_certificate_policy,
                            "prepared_query": prepared_query,
                            "error": e.to_string(),
                        }));
                    }
                }
            }
            query_certs = Some(out);
        }

        Ok::<_, anyhow::Error>((outcome, accepted_snapshot_id, query_certs))
    })
    .await
    .map_err(|e| anyhow!("llm/agent task join failed: {e}"))??;

    let mut gate: Option<serde_json::Value> = None;
    if require_verified_query_gate {
        let ran_any_query = outcome.steps.iter().any(|s| s.tool == "axql_run");
        let mut failures: Vec<String> = Vec::new();

        if ran_any_query {
            if query_certs.is_none() {
                failures.push("no query_certificates emitted".to_string());
                gate = Some(serde_json::json!({
                    "ok": false,
                    "query_certificate_policy": query_certificate_policy,
                    "gate_mode": "require_verified",
                    "ran_any_query": ran_any_query,
                    "failures": failures,
                }));
                // Refuse to return an un-gated answer.
                mark_query_certificate_gate_refusal(&mut outcome, query_certificate_policy);
                // Continue: still allow auto-commit of overlays, and return debug info.
                // (The caller may still want the tool-loop transcript/artifacts.)
            }

            if let Some(certs) = query_certs.as_ref() {
                for c in certs {
                    if let Some(err) = c.get("error").and_then(|v| v.as_str()) {
                        failures.push(format!("query cert error: {err}"));
                        continue;
                    }
                    if query_certificate_policy.requires_verified() {
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
                            None => {
                                failures.push("query cert missing verification status".to_string())
                            }
                        }
                    }
                }

                let ok = failures.is_empty();
                gate = Some(serde_json::json!({
                    "ok": ok,
                    "query_certificate_policy": query_certificate_policy,
                    "gate_mode": "require_verified",
                    "ran_any_query": ran_any_query,
                    "failures": failures,
                }));

                if !ok {
                    mark_query_certificate_gate_refusal(&mut outcome, query_certificate_policy);
                }
            }
        } else {
            // No certified queries were executed; treat the gate as vacuously satisfied.
            gate = Some(serde_json::json!({
                "ok": true,
                "query_certificate_policy": query_certificate_policy,
                "gate_mode": "require_verified",
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
    if auto_commit {
        out["commit"] = serde_json::to_value(commit).unwrap_or(serde_json::Value::Null);
    }
    Ok(out)
}

async fn handle_world_model_propose(
    state: &Arc<ServerState>,
    req: WorldModelProposeRequestV1,
) -> Result<serde_json::Value> {
    if matches!(
        state.config.world_model.backend,
        WorldModelBackend::Disabled
    ) {
        return Err(anyhow!(
            "world model is disabled for this server (configure --world-model-plugin or --world-model-stub)"
        ));
    }

    let config = state.config.clone();
    let (db, accepted_snapshot_id, pathdb_snapshot_id, snapshot_label) = {
        let loaded = state
            .loaded
            .read()
            .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?;
        (
            loaded.db.clone(),
            loaded.accepted_snapshot_id.clone(),
            loaded.pathdb_snapshot_id.clone(),
            loaded.snapshot_label.clone(),
        )
    };

    let accepted_snapshot_id_for_commit = accepted_snapshot_id.clone();
    let accepted_snapshot_id_for_input = accepted_snapshot_id.clone();
    let pathdb_snapshot_id_for_input = pathdb_snapshot_id.clone();

    let req2 = req.clone();
    let (trace_id, proposals, provenance, guardrail, mut notes) = state
        .world_model_executor
        .run(move || {
            let guardrail_profile = req2
                .guardrail_profile
                .as_deref()
                .unwrap_or("fast")
                .trim()
                .to_ascii_lowercase();
            let guardrail_plane = req2
                .guardrail_plane
                .as_deref()
                .unwrap_or("both")
                .trim()
                .to_ascii_lowercase();
            let include_guardrail = req2.include_guardrail.unwrap_or(true);

            let guardrail_weights = req2
                .guardrail_weights
                .clone()
                .unwrap_or_else(crate::world_model::GuardrailCostWeightsV1::defaults);
            let guardrail = if include_guardrail && guardrail_profile != "off" {
                Some(crate::world_model::compute_guardrail_costs(
                    db.as_ref(),
                    &format!("db_server:{snapshot_label}"),
                    &guardrail_profile,
                    &guardrail_plane,
                    &guardrail_weights,
                )?)
            } else {
                None
            };

            let build_opts = crate::world_model_input::WorldModelInputBuildOptionsV1 {
                module_name: req2.axi_module.clone(),
                pathdb_snapshot_id: pathdb_snapshot_id_for_input.clone(),
                accepted_snapshot_id: accepted_snapshot_id_for_input.clone(),
                training_export: Some(crate::world_model::JepaExportOptions {
                    instance_filter: None,
                    max_items: req2
                        .max_new_proposals
                        .unwrap_or(0)
                        .saturating_mul(20)
                        .min(2000)
                        .max(1000),
                    mask_fields: 1,
                    seed: 1,
                    exclude_relations: Vec::new(),
                }),
            };
            let mut input = crate::world_model_input::build_world_model_input_from_pathdb(
                db.as_ref(),
                &build_opts,
            )?;
            if let Some(guardrail) = guardrail.clone() {
                input.set_guardrail_layer(guardrail);
            }
            input.notes.push("source=db_server".to_string());

            let max_keep = req2.max_new_proposals.unwrap_or(0);
            let mut options = crate::world_model::WorldModelOptionsV1::default();
            options.max_new_proposals = max_keep;
            options.seed = req2.seed;
            options.goals = req2.goals.clone();
            options.task_costs = req2.task_costs.clone();
            options.horizon_steps = req2.horizon_steps;

            let input_axi_digest = input.axi_digest_v1.clone();
            let input_pathdb_snapshot_id = input.pathdb_snapshot_id();
            let input_accepted_snapshot_id = input.accepted_snapshot_id();
            let input_module_name = input.semantic_input.module_name.clone();
            let request = crate::world_model::make_world_model_request(input, options);
            let mut response = config.world_model.propose(&request)?;
            if let Some(err) = response.error.take() {
                return Err(anyhow!("world model error: {err}"));
            }

            let guardrail_profile_label = if guardrail_profile == "off" {
                None
            } else {
                Some(guardrail_profile.clone())
            };
            let guardrail_plane_label = if guardrail_profile == "off" {
                None
            } else {
                Some(guardrail_plane.clone())
            };

            let provenance = crate::world_model::build_world_model_provenance(
                &response,
                config.world_model.backend_label(),
                config.world_model.model.clone(),
                input_axi_digest,
                input_pathdb_snapshot_id,
                input_accepted_snapshot_id,
                guardrail.as_ref().map(|g| g.summary.total_cost),
                guardrail_profile_label,
                guardrail_plane_label,
            )?;

            let mut proposals =
                crate::world_model::apply_world_model_provenance(response.proposals, &provenance);
            if max_keep > 0 && proposals.proposals.len() > max_keep {
                proposals.proposals.truncate(max_keep);
            }

            let mut notes = response.notes.clone();
            notes.push(format!(
                "semantic_input={}",
                crate::world_model::WORLD_MODEL_SEMANTIC_INPUT_KIND_V1
            ));
            if let Some(m) = input_module_name.as_ref() {
                notes.push(format!("semantic_module={m}"));
            }
            Ok::<_, anyhow::Error>((response.trace_id, proposals, provenance, guardrail, notes))
        })
        .await?;
    let mut commit: Option<PathdbCommitResponseV1> = None;
    if req.auto_commit {
        let commit_req = PathdbCommitRequestV1 {
            accepted_snapshot: req
                .accepted_snapshot
                .clone()
                .or(accepted_snapshot_id_for_commit),
            chunks: Vec::new(),
            proposals: Some(proposals.clone()),
            validate: req.validate,
            quality: req.quality.clone(),
            quality_plane: req.quality_plane.clone(),
            message: req.commit_message.clone(),
        };
        commit = Some(handle_pathdb_commit_req(state, commit_req).await?);
    }

    if let SnapshotSource::Store { dir, .. } = &state.config.source {
        let run_record = crate::world_model::build_world_model_run_record(
            &provenance,
            &proposals,
            commit.as_ref().map(|c| c.snapshot_id.clone()),
            commit.as_ref().map(|c| c.accepted_snapshot_id.clone()),
            notes.clone(),
        )?;
        let run_path = crate::accepted_plane::persist_world_model_run_record(dir, &run_record)?;
        let rel = run_path
            .strip_prefix(dir)
            .unwrap_or(&run_path)
            .to_string_lossy()
            .to_string();
        notes.push(format!("world_model_run_record={rel}"));
    }

    Ok(serde_json::json!(WorldModelProposeResponseV1 {
        version: "axiograph_world_model_propose_v1".to_string(),
        trace_id,
        proposals,
        guardrail,
        commit,
        notes,
    }))
}

async fn handle_world_model_plan(
    state: &Arc<ServerState>,
    req: WorldModelPlanRequestV1,
) -> Result<serde_json::Value> {
    if matches!(
        state.config.world_model.backend,
        WorldModelBackend::Disabled
    ) {
        return Err(anyhow!(
            "world model is disabled for this server (configure --world-model-plugin or --world-model-stub)"
        ));
    }

    let config = state.config.clone();
    let guardrail_profile = req
        .guardrail_profile
        .as_deref()
        .unwrap_or("fast")
        .trim()
        .to_ascii_lowercase();
    let guardrail_plane = req
        .guardrail_plane
        .as_deref()
        .unwrap_or("both")
        .trim()
        .to_ascii_lowercase();
    let include_guardrail = req.include_guardrail.unwrap_or(true);
    let guardrail_weights = req
        .guardrail_weights
        .clone()
        .unwrap_or_else(crate::world_model::GuardrailCostWeightsV1::defaults);
    let horizon_steps = req.horizon_steps.unwrap_or(3);
    let rollouts = req.rollouts.unwrap_or(2);
    let max_new = req.max_new_proposals.unwrap_or(0);
    let validation_profile = req.quality.clone().unwrap_or_else(|| "fast".to_string());
    let validation_plane = req
        .quality_plane
        .clone()
        .unwrap_or_else(|| "both".to_string());

    let mut commit: Option<PathdbCommitResponseV1> = None;
    let mut commit_steps: Option<Vec<PathdbCommitResponseV1>> = None;

    let report = if req.auto_commit && req.commit_stepwise {
        let plan_trace = format!("wm_plan::{}", now_unix_nanos());
        let mut steps: Vec<crate::world_model::WorldModelPlanStepV1> = Vec::new();
        let mut commits: Vec<PathdbCommitResponseV1> = Vec::new();

        for step_idx in 0..horizon_steps {
            let (db, accepted_snapshot_id, pathdb_snapshot_id) = {
                let loaded = state
                    .loaded
                    .read()
                    .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?;
                (
                    loaded.db.clone(),
                    loaded.accepted_snapshot_id.clone(),
                    loaded.pathdb_snapshot_id.clone(),
                )
            };

            let config = config.clone();
            let req2 = req.clone();
            let guardrail_profile = guardrail_profile.clone();
            let guardrail_plane = guardrail_plane.clone();
            let guardrail_weights = guardrail_weights.clone();
            let validation_profile = validation_profile.clone();
            let validation_plane = validation_plane.clone();
            let report_step = state
                .world_model_executor
                .run(move || {
                    let build_opts = crate::world_model_input::WorldModelInputBuildOptionsV1 {
                        module_name: req2.axi_module.clone(),
                        pathdb_snapshot_id: pathdb_snapshot_id.clone(),
                        accepted_snapshot_id: accepted_snapshot_id.clone(),
                        training_export: Some(crate::world_model::JepaExportOptions {
                            instance_filter: None,
                            max_items: max_new.saturating_mul(20).min(2000).max(1000),
                            mask_fields: 1,
                            seed: 1,
                            exclude_relations: Vec::new(),
                        }),
                    };
                    let mut base_input =
                        crate::world_model_input::build_world_model_input_from_pathdb(
                            db.as_ref(),
                            &build_opts,
                        )?;
                    base_input
                        .notes
                        .push(format!("source=db_server_plan step={step_idx}"));

                    let plan_opts = crate::world_model::WorldModelPlanOptionsV1 {
                        horizon_steps: 1,
                        rollouts,
                        max_new_proposals: max_new,
                        seed: req2.seed,
                        goals: req2.goals.clone(),
                        task_costs: req2.task_costs.clone(),
                        competency_questions: req2.competency_questions.clone(),
                        guardrail_profile: guardrail_profile.clone(),
                        guardrail_plane: guardrail_plane.clone(),
                        guardrail_weights: guardrail_weights.clone(),
                        include_guardrail,
                        validation_profile: validation_profile.clone(),
                        validation_plane: validation_plane.clone(),
                    };

                    crate::world_model::run_world_model_plan(
                        db.as_ref(),
                        &config.world_model,
                        &base_input,
                        &plan_opts,
                    )
                    .map_err(|e| anyhow!("world_model/plan step failed: {e}"))
                })
                .await?;

            let mut step_report = report_step
                .steps
                .into_iter()
                .next()
                .ok_or_else(|| anyhow!("world_model/plan step returned no steps"))?;
            step_report.step = step_idx;
            steps.push(step_report.clone());

            let commit_req = PathdbCommitRequestV1 {
                accepted_snapshot: req.accepted_snapshot.clone().or_else(|| {
                    let loaded = state.loaded.read().ok()?;
                    loaded.accepted_snapshot_id.clone()
                }),
                chunks: Vec::new(),
                proposals: Some(step_report.proposals.clone()),
                validate: req.validate,
                quality: req.quality.clone(),
                quality_plane: req.quality_plane.clone(),
                message: req
                    .commit_message
                    .clone()
                    .map(|m| format!("step {step_idx}: {m}"))
                    .or_else(|| Some(format!("world_model_plan step {step_idx}"))),
            };
            let res = handle_pathdb_commit_req(state, commit_req).await?;
            commits.push(res);

            let _ = reload_now(state).await;
        }

        let task_cost_total: f64 = req.task_costs.iter().map(|t| t.value * t.weight).sum();

        commit_steps = Some(commits);

        crate::world_model::WorldModelPlanReportV1 {
            version: "world_model_plan_v1".to_string(),
            trace_id: plan_trace.into(),
            generated_at_unix_secs: now_unix_secs(),
            horizon_steps,
            rollouts,
            max_new_proposals: max_new,
            guardrail_profile: guardrail_profile.clone(),
            guardrail_plane: guardrail_plane.clone(),
            guardrail_weights: guardrail_weights.clone(),
            task_costs: req.task_costs.clone(),
            task_cost_total,
            competency_questions: req.competency_questions.clone(),
            steps,
        }
    } else {
        let (db, accepted_snapshot_id, pathdb_snapshot_id) = {
            let loaded = state
                .loaded
                .read()
                .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?;
            (
                loaded.db.clone(),
                loaded.accepted_snapshot_id.clone(),
                loaded.pathdb_snapshot_id.clone(),
            )
        };

        let accepted_snapshot_id_for_commit = accepted_snapshot_id.clone();
        let pathdb_snapshot_id_for_input = pathdb_snapshot_id.clone();

        let req2 = req.clone();
        let report = state
            .world_model_executor
            .run(move || {
                let build_opts = crate::world_model_input::WorldModelInputBuildOptionsV1 {
                    module_name: req2.axi_module.clone(),
                    pathdb_snapshot_id: pathdb_snapshot_id_for_input.clone(),
                    accepted_snapshot_id: accepted_snapshot_id.clone(),
                    training_export: Some(crate::world_model::JepaExportOptions {
                        instance_filter: None,
                        max_items: max_new.saturating_mul(20).min(2000).max(1000),
                        mask_fields: 1,
                        seed: 1,
                        exclude_relations: Vec::new(),
                    }),
                };
                let mut base_input = crate::world_model_input::build_world_model_input_from_pathdb(
                    db.as_ref(),
                    &build_opts,
                )?;
                base_input.notes.push("source=db_server_plan".to_string());

                let plan_opts = crate::world_model::WorldModelPlanOptionsV1 {
                    horizon_steps,
                    rollouts,
                    max_new_proposals: max_new,
                    seed: req2.seed,
                    goals: req2.goals.clone(),
                    task_costs: req2.task_costs.clone(),
                    competency_questions: req2.competency_questions.clone(),
                    guardrail_profile: guardrail_profile.clone(),
                    guardrail_plane: guardrail_plane.clone(),
                    guardrail_weights: guardrail_weights.clone(),
                    include_guardrail,
                    validation_profile: validation_profile.clone(),
                    validation_plane: validation_plane.clone(),
                };

                crate::world_model::run_world_model_plan(
                    db.as_ref(),
                    &config.world_model,
                    &base_input,
                    &plan_opts,
                )
                .map_err(|e| anyhow!("world_model/plan failed: {e}"))
            })
            .await?;

        if req.auto_commit {
            let generated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .to_string();
            let mut merged = axiograph_ingest_docs::ProposalsFileV1 {
                version: axiograph_ingest_docs::proposals::PROPOSALS_VERSION_V1,
                generated_at,
                source: axiograph_ingest_docs::ProposalSourceV1 {
                    source_type: "world_model_plan".to_string(),
                    locator: report.trace_id.to_string(),
                },
                schema_hint: None,
                proposals: Vec::new(),
            };
            for step in &report.steps {
                merged.proposals.extend(step.proposals.proposals.clone());
            }

            let commit_req = PathdbCommitRequestV1 {
                accepted_snapshot: req
                    .accepted_snapshot
                    .clone()
                    .or(accepted_snapshot_id_for_commit),
                chunks: Vec::new(),
                proposals: Some(merged),
                validate: req.validate,
                quality: req.quality.clone(),
                quality_plane: req.quality_plane.clone(),
                message: req.commit_message.clone(),
            };
            commit = Some(handle_pathdb_commit_req(state, commit_req).await?);
        }

        report
    };

    Ok(serde_json::json!(WorldModelPlanResponseV1 {
        version: "axiograph_world_model_plan_v1".to_string(),
        report,
        commit,
        commit_steps,
    }))
}

async fn handle_entity_describe_get(
    state: &Arc<ServerState>,
    query: Option<&str>,
) -> Result<serde_json::Value> {
    let p = parse_query_params(query);

    let id = p.get("id").and_then(|s| s.parse::<u32>().ok());
    let name = p.get("name").cloned();
    let type_name = p.get("type").or_else(|| p.get("type_name")).cloned();
    let max_attrs = p.get("max_attrs").and_then(|s| s.parse::<usize>().ok());
    let max_rel_types = p.get("max_rel_types").and_then(|s| s.parse::<usize>().ok());
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

async fn handle_docchunk_get(
    state: &Arc<ServerState>,
    query: Option<&str>,
) -> Result<serde_json::Value> {
    let p = parse_query_params(query);

    let id = p.get("id").and_then(|s| s.parse::<u32>().ok());
    let chunk_id = p.get("chunk_id").or_else(|| p.get("chunkId")).cloned();
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
            let loaded = load_from_store(dir, layer, snapshot, &state.config)?;
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
    let req: Req = parse_json_request(body, "discover/draft-axi")?;

    let opts = crate::schema_discovery::DraftAxiModuleOptions {
        module_name: req.module_name.unwrap_or_else(|| "DraftModule".to_string()),
        schema_name: req.schema_name.unwrap_or_else(|| "DraftSchema".to_string()),
        instance_name: req
            .instance_name
            .unwrap_or_else(|| "DraftInstance".to_string()),
        infer_constraints: req.infer_constraints.unwrap_or(true),
    };

    let axi_text = crate::schema_discovery::draft_axi_module_from_proposals(&req.proposals, &opts)?;
    let digest = axiograph_dsl::digest::axi_digest_v1(&axi_text);
    let typed_authoring =
        crate::typed_authoring::draft_typed_authoring_summary_from_axi_text(&axi_text);
    let exploration_preview =
        crate::typed_authoring::build_compiled_ir_exploration_preview_against_axi_text(
            &axi_text,
            Some(&opts.schema_name),
        );

    Ok(serde_json::json!({
        "version": "axiograph_discover_draft_axi_v1",
        "digest": digest,
        "module_name": opts.module_name,
        "schema_name": opts.schema_name,
        "instance_name": opts.instance_name,
        "axi_text": axi_text,
        "typed_authoring": typed_authoring,
        "exploration_preview": exploration_preview,
    }))
}

async fn handle_discover_check_olog(body: &[u8]) -> Result<serde_json::Value> {
    #[derive(Debug, Clone, Deserialize)]
    struct Req {
        axi_text: String,
        #[serde(default)]
        schema_name: Option<String>,
        fragment: crate::typed_authoring::OlogFragmentV1,
        #[serde(default)]
        apply_refinement_handle_id: Option<String>,
    }

    let req: Req = parse_json_request(body, "discover/check-olog")?;
    let report = crate::typed_authoring::discover_check_olog_report_against_axi_text(
        &req.axi_text,
        req.schema_name.as_deref(),
        req.fragment,
        req.apply_refinement_handle_id.as_deref(),
    )?;
    Ok(serde_json::to_value(report)?)
}

async fn handle_semantic_coverage(
    state: &Arc<ServerState>,
    body: &[u8],
) -> Result<serde_json::Value> {
    #[derive(Debug, Clone, Deserialize)]
    struct Req {
        #[serde(default)]
        lifecycle_state: Option<String>,
        #[serde(default)]
        surfaces: Vec<crate::semantic_claim::ImplementationSurfaceRefV1>,
        #[serde(default)]
        edges: Vec<crate::semantic_claim::CoverageEdgeV1>,
        #[serde(default)]
        runtime_theory_check: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
        #[serde(default)]
        runtime_theory_check_input: Option<crate::runtime_theory_check::RuntimeTheoryCheckInputV1>,
    }

    let req: Req = parse_json_request(body, "semantic/coverage")?;

    let (db, accepted_snapshot_id, meta_from_state) = {
        let loaded = state.loaded.read().unwrap();
        (
            loaded.db.clone(),
            loaded.accepted_snapshot_id.clone(),
            loaded.meta.clone(),
        )
    };
    let meta = match meta_from_state {
        Some(meta) => meta,
        None => MetaPlaneIndex::from_db(&db)?,
    };
    let lifecycle_state = req.lifecycle_state.unwrap_or_else(|| {
        if accepted_snapshot_id.is_some() {
            "accepted".to_string()
        } else {
            "runtime_checked".to_string()
        }
    });
    let runtime_theory_check = resolve_runtime_theory_check_summary(
        req.runtime_theory_check,
        req.runtime_theory_check_input,
    )?;
    let coverage = crate::semantic_claim::semantic_coverage_report(
        &meta,
        accepted_snapshot_id.clone(),
        &lifecycle_state,
        &req.surfaces,
        &req.edges,
        runtime_theory_check,
    );

    Ok(serde_json::json!({
        "version": "axiograph_semantic_coverage_v1",
        "accepted_snapshot_id": accepted_snapshot_id,
        "coverage": coverage,
    }))
}

async fn handle_semantic_business_rule(
    state: &Arc<ServerState>,
    body: &[u8],
) -> Result<serde_json::Value> {
    #[derive(Debug, Clone, Deserialize)]
    struct Req {
        scope: crate::semantic_claim::RuntimeRuleScopeV1,
        #[serde(default)]
        lifecycle_state: Option<String>,
    }

    let req: Req = parse_json_request(body, "semantic/business-rule")?;

    let (db, accepted_snapshot_id, meta_from_state) = {
        let loaded = state.loaded.read().unwrap();
        (
            loaded.db.clone(),
            loaded.accepted_snapshot_id.clone(),
            loaded.meta.clone(),
        )
    };
    let meta = match meta_from_state {
        Some(meta) => meta,
        None => MetaPlaneIndex::from_db(&db)?,
    };
    let lifecycle_state = req.lifecycle_state.unwrap_or_else(|| {
        if accepted_snapshot_id.is_some() {
            "accepted".to_string()
        } else {
            "runtime_checked".to_string()
        }
    });

    let report = match req.scope.scope_class {
        crate::semantic_claim::RuntimeRuleScopeClassV1::Relation => {
            let relation = req
                .scope
                .relation
                .as_deref()
                .ok_or_else(|| anyhow!("relation scope requires `scope.relation`"))?;
            crate::semantic_claim::business_rule_applicability_for_relation(
                &meta,
                accepted_snapshot_id.clone(),
                &lifecycle_state,
                &req.scope.schema,
                relation,
            )
        }
        crate::semantic_claim::RuntimeRuleScopeClassV1::Theory => {
            let theory = req
                .scope
                .theory
                .as_deref()
                .ok_or_else(|| anyhow!("theory scope requires `scope.theory`"))?;
            crate::semantic_claim::business_rule_applicability_for_theory(
                &meta,
                accepted_snapshot_id.clone(),
                &lifecycle_state,
                &req.scope.schema,
                theory,
            )
        }
    };

    Ok(serde_json::json!({
        "version": "axiograph_semantic_business_rule_v1",
        "accepted_snapshot_id": accepted_snapshot_id,
        "report": report,
    }))
}

async fn handle_semantic_agent_report(
    state: &Arc<ServerState>,
    body: &[u8],
) -> Result<serde_json::Value> {
    #[derive(Debug, Clone, Deserialize)]
    struct Req {
        task: crate::semantic_claim::AgentTaskRefV1,
        #[serde(default)]
        lifecycle_state: Option<String>,
        #[serde(default)]
        surfaces: Vec<crate::semantic_claim::ImplementationSurfaceRefV1>,
        #[serde(default)]
        edges: Vec<crate::semantic_claim::CoverageEdgeV1>,
        #[serde(default)]
        runtime_theory_check: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
        #[serde(default)]
        runtime_theory_check_input: Option<crate::runtime_theory_check::RuntimeTheoryCheckInputV1>,
    }

    let req: Req = parse_json_request(body, "semantic/agent-report")?;

    let (db, accepted_snapshot_id, meta_from_state) = {
        let loaded = state.loaded.read().unwrap();
        (
            loaded.db.clone(),
            loaded.accepted_snapshot_id.clone(),
            loaded.meta.clone(),
        )
    };
    let meta = match meta_from_state {
        Some(meta) => meta,
        None => MetaPlaneIndex::from_db(&db)?,
    };
    let lifecycle_state = req.lifecycle_state.unwrap_or_else(|| {
        if accepted_snapshot_id.is_some() {
            "accepted".to_string()
        } else {
            "runtime_checked".to_string()
        }
    });
    let runtime_theory_check = resolve_runtime_theory_check_summary(
        req.runtime_theory_check,
        req.runtime_theory_check_input,
    )?;
    let report = crate::semantic_claim::agent_engineering_report(
        &meta,
        accepted_snapshot_id.clone(),
        &lifecycle_state,
        &req.task,
        &req.surfaces,
        &req.edges,
        runtime_theory_check,
    );

    Ok(serde_json::json!({
        "version": "axiograph_semantic_agent_report_v1",
        "accepted_snapshot_id": accepted_snapshot_id,
        "report": report,
    }))
}

async fn handle_semantic_context_report(
    state: &Arc<ServerState>,
    body: &[u8],
) -> Result<serde_json::Value> {
    let req: crate::context_report::ContextReportRequestV1 =
        parse_json_request(body, "semantic/context-report")?;

    let (db, accepted_snapshot_id, meta_from_state) = {
        let loaded = state.loaded.read().unwrap();
        (
            loaded.db.clone(),
            loaded.accepted_snapshot_id.clone(),
            loaded.meta.clone(),
        )
    };
    let report = crate::context_report::build_context_report_from_request(
        &db,
        meta_from_state.as_ref(),
        accepted_snapshot_id.clone(),
        req,
    )?;

    Ok(serde_json::json!({
        "version": "axiograph_semantic_context_report_v1",
        "accepted_snapshot_id": accepted_snapshot_id,
        "report": report,
    }))
}

async fn handle_semantic_behavior_case(
    state: &Arc<ServerState>,
    body: &[u8],
) -> Result<serde_json::Value> {
    let req: crate::behavior_case::BehaviorCaseCheckRequestV1 =
        parse_json_request(body, "semantic/behavior-case")?;

    let (db, accepted_snapshot_id, meta_from_state) = {
        let loaded = state.loaded.read().unwrap();
        (
            loaded.db.clone(),
            loaded.accepted_snapshot_id.clone(),
            loaded.meta.clone(),
        )
    };
    let report = crate::behavior_case::build_behavior_case_report_from_request(
        &db,
        meta_from_state.as_ref(),
        accepted_snapshot_id.clone(),
        req,
    )?;

    Ok(serde_json::json!({
        "version": "axiograph_semantic_behavior_case_v1",
        "accepted_snapshot_id": accepted_snapshot_id,
        "report": report,
    }))
}

async fn handle_semantic_overlay_check(body: &[u8]) -> Result<serde_json::Value> {
    let args: serde_json::Value = parse_json_request(body, "semantic/overlay-check")?;
    serde_json::to_value(crate::semantic_tools::call_semantic_overlay_check(args)?)
        .map_err(Into::into)
}

async fn handle_semantic_software_coverage(body: &[u8]) -> Result<serde_json::Value> {
    let args: serde_json::Value = parse_json_request(body, "semantic/software-coverage")?;
    serde_json::to_value(crate::semantic_tools::call_semantic_software_coverage(
        args,
    )?)
    .map_err(Into::into)
}

async fn handle_semantic_codegen_plan(body: &[u8]) -> Result<serde_json::Value> {
    let args: serde_json::Value = parse_json_request(body, "semantic/codegen-plan")?;
    serde_json::to_value(crate::semantic_tools::call_semantic_codegen_plan(args)?)
        .map_err(Into::into)
}

async fn handle_semantic_coverage_query(body: &[u8]) -> Result<serde_json::Value> {
    let args: serde_json::Value = parse_json_request(body, "semantic/coverage-query")?;
    serde_json::to_value(crate::semantic_tools::call_semantic_coverage_query(args)?)
        .map_err(Into::into)
}

async fn handle_semantic_definition_query(body: &[u8]) -> Result<serde_json::Value> {
    let args: serde_json::Value = parse_json_request(body, "semantic/definition-query")?;
    serde_json::to_value(crate::semantic_tools::call_semantic_definition_query(args)?)
        .map_err(Into::into)
}

async fn handle_semantic_theory_check(body: &[u8]) -> Result<serde_json::Value> {
    let req: crate::runtime_theory_check::RuntimeTheoryCheckInputV1 =
        parse_json_request(body, "semantic/theory-check")?;
    let report = crate::runtime_theory_check::runtime_theory_check_reports_from_input(&req)?;

    Ok(serde_json::json!({
        "version": "axiograph_semantic_theory_check_v1",
        "report": report,
    }))
}

fn resolve_runtime_theory_check_summary(
    provided: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
    input: Option<crate::runtime_theory_check::RuntimeTheoryCheckInputV1>,
) -> Result<Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>> {
    if let Some(summary) = provided {
        return Ok(Some(summary));
    }
    input
        .as_ref()
        .map(crate::runtime_theory_check::runtime_theory_check_summary_from_input)
        .transpose()
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
        all: p.get("all").and_then(|s| parse_bool(Some(s.as_str()))),
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
        typed_overlay: p
            .get("typed_overlay")
            .and_then(|s| parse_bool(Some(s.as_str()))),
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

async fn handle_viz_static_get(path: &str) -> Result<Response<Full<Bytes>>> {
    let dist_dir = crate::viz::viz_dist_dir();
    let rel = path.trim_start_matches("/viz/");
    if rel.is_empty() {
        return Err(anyhow!("missing viz asset"));
    }
    if rel.contains("..") {
        return Err(anyhow!("invalid viz asset path"));
    }
    let file_path = dist_dir.join(rel);
    let data = tokio::fs::read(&file_path).await.with_context(|| {
        format!(
            "viz asset not found (expected {}); run `npm install && npm run build` in frontend/viz",
            file_path.display()
        )
    })?;
    let mime = viz_static_mime(rel);
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("content-type", mime)
        .body(Full::new(Bytes::from(data)))?)
}

fn viz_static_mime(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".js") {
        "application/javascript"
    } else if lower.ends_with(".css") {
        "text/css"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".json") {
        "application/json"
    } else if lower.ends_with(".map") {
        "application/json"
    } else if lower.ends_with(".woff2") {
        "font/woff2"
    } else if lower.ends_with(".woff") {
        "font/woff"
    } else {
        "application/octet-stream"
    }
}

async fn handle_viz_post(state: &Arc<ServerState>, body: &[u8]) -> Result<Response<Full<Bytes>>> {
    let req: VizRequestV1 = parse_json_request(body, "viz")?;
    handle_viz_request(state, req).await
}

async fn handle_viz_request(
    state: &Arc<ServerState>,
    req: VizRequestV1,
) -> Result<Response<Full<Bytes>>> {
    let req_format = req.format.as_deref().unwrap_or("html");
    let format = crate::viz::VizFormat::parse(req_format)?;

    let plane = req
        .plane
        .as_deref()
        .unwrap_or("data")
        .trim()
        .to_ascii_lowercase();
    let (include_meta_plane, include_data_plane) = match plane.as_str() {
        "data" => (false, true),
        "meta" => (true, false),
        "both" => (true, true),
        other => return Err(anyhow!("unknown plane `{other}` (expected data|meta|both)")),
    };

    let direction = crate::viz::VizDirection::parse(req.direction.as_deref().unwrap_or("both"))?;
    let typed_overlay = req.typed_overlay.unwrap_or(false);
    let include_equivalences = req.include_equivalences.unwrap_or(true);

    let hops = req.hops.unwrap_or(2);
    let max_nodes = req.max_nodes.unwrap_or(250);
    let max_edges = req.max_edges.unwrap_or(4_000);
    let all_nodes = req.all.unwrap_or(false);

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
            let loaded = load_from_store(dir, layer, snapshot, &state.config)?;
            (loaded.db, loaded.meta)
        } else {
            let loaded = state
                .loaded
                .read()
                .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?;
            (loaded.db.clone(), loaded.meta.clone())
        };

        let mut focus_ids: Vec<u32> = Vec::new();
        if !all_nodes {
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
        }

        let options = crate::viz::VizOptions {
            focus_ids,
            all_nodes,
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
            crate::viz::VizFormat::Json => (
                "application/json",
                crate::viz::render_json(&g)?.into_bytes(),
            ),
            crate::viz::VizFormat::Dot => (
                "text/vnd.graphviz; charset=utf-8",
                crate::viz::render_dot(&db, &g).into_bytes(),
            ),
        };

        Ok::<_, anyhow::Error>(
            Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, content_type)
                .body(Full::new(Bytes::from(bytes)))
                .unwrap_or_else(|_| {
                    Response::new(Full::new(Bytes::from_static(b"{\"error\":\"internal\"}")))
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
    #[serde(default)]
    competency_questions: Vec<crate::world_model::CompetencyQuestionV1>,
    #[serde(default)]
    cq_fail_on_regression: bool,
    #[serde(default)]
    cq_fail_on_unsatisfied_after: bool,
}

#[derive(Debug, Clone, Serialize)]
struct PromoteResponseV1 {
    snapshot_id: AcceptedSnapshotId,
    #[serde(skip_serializing_if = "Option::is_none")]
    validation_report_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stored_report_path: Option<String>,
}

async fn handle_promote(state: &Arc<ServerState>, body: &[u8]) -> Result<PromoteResponseV1> {
    let req: PromoteRequestV1 = parse_json_request(body, "promote")?;

    let SnapshotSource::Store { dir, .. } = &state.config.source else {
        return Err(anyhow!(
            "promote requires `db serve --dir <accepted_plane_dir>` (store-backed server)"
        ));
    };

    let dir = dir.clone();
    let quality = req.quality.as_deref().unwrap_or("off").trim().to_string();
    let message = req.message.clone();
    let axi_text = req.axi_text.clone();

    let competency_questions = req.competency_questions.clone();
    let cq_fail_on_regression = req.cq_fail_on_regression;
    let cq_fail_on_unsatisfied_after = req.cq_fail_on_unsatisfied_after;

    let result = tokio::task::spawn_blocking(move || {
        let tmp = write_temp_file("axi", &axi_text)?;
        let out = crate::accepted_plane::promote_reviewed_module_with_options(
            &tmp,
            &dir,
            &crate::accepted_plane::PromoteReviewedModuleOptionsV1 {
                message,
                quality_profile: quality,
                quality_plane: "both".to_string(),
                competency_questions,
                competency_gate: crate::proposals_validate::CompetencyGatePolicyV1 {
                    fail_on_regression: cq_fail_on_regression,
                    fail_on_unsatisfied_after: cq_fail_on_unsatisfied_after,
                },
                persist_validation_report: true,
            },
        )?;
        let _ = std::fs::remove_file(&tmp);
        Ok::<_, anyhow::Error>(out)
    })
    .await
    .map_err(|e| anyhow!("promote task join failed: {e}"))??;

    // If we're serving `accepted/head`, reload immediately so clients see it.
    let should_reload = match &state.config.source {
        SnapshotSource::Store {
            layer, snapshot, ..
        } => {
            layer.trim().eq_ignore_ascii_case("accepted")
                && (snapshot == "head" || snapshot == "latest")
        }
        _ => false,
    };
    if should_reload {
        let _ = reload_now(state).await;
    }

    Ok(PromoteResponseV1 {
        snapshot_id: result.snapshot_id,
        validation_report_path: result.validation_report_path,
        stored_report_path: result.stored_report_path,
    })
}

#[derive(Debug, Clone, Deserialize)]
struct PathdbCommitRequestV1 {
    #[serde(default)]
    accepted_snapshot: Option<AcceptedSnapshotId>,
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
    snapshot_id: PathdbSnapshotId,
    accepted_snapshot_id: AcceptedSnapshotId,
    ops_added: usize,
}

async fn handle_pathdb_commit(
    state: &Arc<ServerState>,
    body: &[u8],
) -> Result<PathdbCommitResponseV1> {
    let req: PathdbCommitRequestV1 = parse_json_request(body, "pathdb-commit")?;
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
    let default_accepted_snapshot = state
        .loaded
        .read()
        .map_err(|_| anyhow!("loaded snapshot lock poisoned"))?
        .accepted_snapshot_id
        .clone();
    let accepted_snapshot = req.accepted_snapshot.clone().or(default_accepted_snapshot);
    let message = req.message.clone();
    let chunks = req.chunks.clone();
    let proposals = req.proposals.clone();

    // Gate: validate proposal overlays (UI-friendly safe default).
    let should_validate = req.validate.unwrap_or(true);
    if should_validate {
        if let Some(file) = proposals.as_ref() {
            let quality = req.quality.as_deref().unwrap_or("fast").trim().to_string();
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
                if !validation.axi_typecheck.skipped && !validation.axi_typecheck.errors.is_empty()
                {
                    msg.push_str(": typecheck errors: ");
                    for (i, e) in validation.axi_typecheck.errors.iter().take(4).enumerate() {
                        if i > 0 {
                            msg.push_str(" | ");
                        }
                        msg.push_str(&e.message);
                    }
                    if validation.axi_typecheck.errors.len() > 4 {
                        msg.push_str(&format!(
                            " (+{} more)",
                            validation.axi_typecheck.errors.len() - 4
                        ));
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
                &axiograph_ingest_docs::chunks_to_json_for_chunks(
                    "db_server_proposal_evidence",
                    "server-request",
                    chunks.clone(),
                )
                .unwrap_or_default(),
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

        let res = if let Some(accepted_snapshot_id) = accepted_snapshot.as_ref() {
            crate::pathdb_wal::commit_pathdb_snapshot_on_accepted_snapshot_with_overlays(
                &dir,
                accepted_snapshot_id,
                &chunk_paths,
                &proposal_paths,
                message.as_deref(),
            )?
        } else {
            crate::pathdb_wal::commit_pathdb_snapshot_with_overlays(
                &dir,
                "head",
                &chunk_paths,
                &proposal_paths,
                message.as_deref(),
            )?
        };

        for p in chunk_paths.into_iter().chain(proposal_paths.into_iter()) {
            let _ = std::fs::remove_file(&p);
        }
        Ok::<_, anyhow::Error>(res)
    })
    .await
    .map_err(|e| anyhow!("pathdb-commit task join failed: {e}"))??;

    // If we're serving `pathdb/head`, reload immediately so clients see it.
    let should_reload = match &state.config.source {
        SnapshotSource::Store {
            layer, snapshot, ..
        } => {
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

    let req: Request = parse_json_request(body, "/proposals/relation")?;

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

    let req: Request = parse_json_request(body, "/proposals/relations")?;

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
    let SnapshotSource::Store {
        dir,
        layer,
        snapshot,
    } = &state.config.source
    else {
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
            loaded.accepted_snapshot_id.clone().map(|id| id.to_string())
        } else {
            loaded.pathdb_snapshot_id.clone().map(|id| id.to_string())
        }
    };

    if head.is_some() && head != current {
        let _ = reload_now(state).await?;
    }
    Ok(())
}

fn load_snapshot(config: &ServerConfig) -> Result<LoadedSnapshot> {
    match &config.source {
        SnapshotSource::Axpd(path) => load_from_axpd(path, config),
        SnapshotSource::Store {
            dir,
            layer,
            snapshot,
        } => load_from_store(dir, layer, snapshot, config),
    }
}

fn configure_path_index(db: &mut PathDB, config: &ServerConfig) {
    if config.path_index_lru_async || config.path_index_lru_capacity > 0 {
        let queue = if config.path_index_lru_async {
            config.path_index_lru_queue
        } else {
            0
        };
        db.enable_path_index_lru_async(queue);
    }
    db.set_path_index_lru_capacity(config.path_index_lru_capacity);
}

fn sidecar_path_for_axpd(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.idx.cbor", path.display()))
}

fn load_from_axpd(path: &Path, config: &ServerConfig) -> Result<LoadedSnapshot> {
    let bytes = std::fs::read(path)
        .map_err(|e| anyhow!("failed to read .axpd `{}`: {e}", path.display()))?;
    let snapshot_key = axiograph_dsl::digest::fnv1a64_digest_bytes(&bytes);
    let mut db = PathDB::from_bytes(&bytes)?;
    configure_path_index(&mut db, config);
    let sidecar_path = sidecar_path_for_axpd(path);
    if sidecar_path.exists() {
        if let Ok(sidecar) = read_sidecar_file(&sidecar_path) {
            db.load_index_sidecar(sidecar);
        }
    }
    let db = Arc::new(db);
    db.attach_async_index_source(Arc::downgrade(&db));
    let writer = IndexSidecarWriter::new(
        sidecar_path,
        Arc::downgrade(&db),
        Some(snapshot_key.clone().into()),
    );
    db.attach_index_sidecar_writer(Arc::new(writer));
    let meta = MetaPlaneIndex::from_db(&db).ok();
    Ok(LoadedSnapshot {
        snapshot_key: snapshot_key.clone(),
        snapshot_label: format!("axpd:{} ({})", path.display(), snapshot_key),
        accepted_snapshot_id: None,
        accepted_axi_anchor: None,
        accepted_axi_text: None,
        pathdb_snapshot_id: None,
        loaded_at_unix_secs: now_unix_secs(),
        entities: db.entities.len(),
        relations: db.relations.len(),
        db,
        meta,
        embeddings: None,
    })
}

fn load_from_store(
    dir: &Path,
    layer: &str,
    snapshot: &str,
    config: &ServerConfig,
) -> Result<LoadedSnapshot> {
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
    let accepted_anchor_before_build =
        if let Some(accepted_snapshot_id) = accepted_snapshot_id.as_ref() {
            crate::accepted_plane::read_single_module_accepted_axi_anchor_and_text(
                dir,
                accepted_snapshot_id,
            )?
        } else {
            None
        };

    let tmp = write_temp_file("axpd", ""); // reserve a unique name
    let tmp = tmp?;
    if layer == "accepted" {
        let accepted_id = accepted_snapshot_id.as_ref().expect("accepted id set");
        crate::accepted_plane::build_pathdb_from_snapshot(dir, accepted_id.as_str(), &tmp)?;
    } else {
        crate::pathdb_wal::build_pathdb_from_pathdb_snapshot(
            dir,
            pathdb_snapshot_id.as_ref().expect("pathdb id set").as_str(),
            &tmp,
        )?;
    }

    let bytes = std::fs::read(&tmp)?;
    let _ = std::fs::remove_file(&tmp);
    let accepted_anchor_after_build =
        if let Some(accepted_snapshot_id) = accepted_snapshot_id.as_ref() {
            crate::accepted_plane::read_single_module_accepted_axi_anchor_and_text(
                dir,
                accepted_snapshot_id,
            )?
        } else {
            None
        };
    if accepted_anchor_before_build != accepted_anchor_after_build {
        return Err(anyhow!(
            "accepted snapshot anchor changed while rebuilding store-backed runtime; refusing to bind accepted anchor"
        ));
    }
    let (accepted_axi_anchor, accepted_axi_text) = match accepted_anchor_after_build {
        Some((anchor, text)) => (Some(anchor), Some(text)),
        None => (None, None),
    };

    let snapshot_key = if layer == "pathdb" {
        pathdb_snapshot_id
            .as_ref()
            .expect("pathdb id set")
            .to_string()
    } else {
        accepted_snapshot_id
            .as_ref()
            .expect("accepted id set")
            .to_string()
    };

    let mut db = PathDB::from_bytes(&bytes)?;
    configure_path_index(&mut db, config);
    if let Some(pathdb_snapshot_id) = pathdb_snapshot_id.as_ref() {
        let sidecar_path = crate::pathdb_wal::checkpoint_sidecar_path(dir, pathdb_snapshot_id);
        if sidecar_path.exists() {
            if let Ok(sidecar) = read_sidecar_file(&sidecar_path) {
                db.load_index_sidecar(sidecar);
            }
        }
    }
    let db = Arc::new(db);
    db.attach_async_index_source(Arc::downgrade(&db));
    if let Some(pathdb_snapshot_id) = pathdb_snapshot_id.as_ref() {
        let sidecar_path = crate::pathdb_wal::checkpoint_sidecar_path(dir, pathdb_snapshot_id);
        let writer = IndexSidecarWriter::new(
            sidecar_path,
            Arc::downgrade(&db),
            Some(pathdb_snapshot_id.clone()),
        );
        db.attach_index_sidecar_writer(Arc::new(writer));
    }
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

        if any {
            Some(Arc::new(idx))
        } else {
            None
        }
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
        accepted_axi_anchor,
        accepted_axi_text,
        pathdb_snapshot_id,
        loaded_at_unix_secs: now_unix_secs(),
        entities: db.entities.len(),
        relations: db.relations.len(),
        db,
        meta,
        embeddings,
    })
}

#[derive(Clone)]
pub(crate) struct ReadOnlySemanticRuntime {
    #[allow(dead_code)]
    pub snapshot_key: String,
    pub accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub accepted_axi_anchor: Option<AcceptedAxiAnchor>,
    pub db: Arc<PathDB>,
    pub meta: Option<MetaPlaneIndex>,
}

pub(crate) fn load_read_only_semantic_runtime(
    axpd: Option<&Path>,
    dir: Option<&Path>,
    layer: &str,
    snapshot: &str,
) -> Result<ReadOnlySemanticRuntime> {
    let source = match (axpd, dir) {
        (Some(_), Some(_)) => {
            return Err(anyhow!(
                "choose exactly one snapshot source: either `--axpd` or `--dir`"
            ))
        }
        (Some(path), None) => SnapshotSource::Axpd(path.to_path_buf()),
        (None, Some(store_dir)) => SnapshotSource::Store {
            dir: store_dir.to_path_buf(),
            layer: layer.to_string(),
            snapshot: snapshot.to_string(),
        },
        (None, None) => {
            return Err(anyhow!(
            "missing snapshot source: pass either `--axpd <file>` or `--dir <accepted_plane_dir>`"
        ))
        }
    };

    let config = ServerConfig {
        listen: "127.0.0.1:0"
            .parse()
            .expect("static loopback listen address should parse"),
        role: ServerRole::Standalone,
        source: source.clone(),
        watch_head: false,
        poll_interval: Duration::from_secs(0),
        admin_token: None,
        ready_file: None,
        cert_verify: CertVerifyConfig {
            verifier_bin: None,
            timeout: None,
        },
        llm: LlmState::default(),
        world_model: WorldModelState::default(),
        world_model_workers: 0,
        path_index_lru_capacity: 0,
        path_index_lru_async: false,
        path_index_lru_queue: 1024,
    };

    let loaded = match source {
        SnapshotSource::Axpd(path) => load_from_axpd(&path, &config)?,
        SnapshotSource::Store {
            dir,
            layer,
            snapshot,
        } => load_from_store(&dir, &layer, &snapshot, &config)?,
    };

    Ok(ReadOnlySemanticRuntime {
        snapshot_key: loaded.snapshot_key,
        accepted_snapshot_id: loaded.accepted_snapshot_id,
        accepted_axi_anchor: loaded.accepted_axi_anchor,
        db: loaded.db,
        meta: loaded.meta,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn typed_server_snapshot_responses_serialize_as_string_ids() {
        let promote = PromoteResponseV1 {
            snapshot_id: AcceptedSnapshotId::new("accepted:42"),
            validation_report_path: Some("sem/validations/demo.json".to_string()),
            stored_report_path: Some("sem/validations/demo.json".to_string()),
        };
        assert_eq!(
            serde_json::to_value(&promote).expect("serialize promote response"),
            json!({
                "snapshot_id": "accepted:42",
                "validation_report_path": "sem/validations/demo.json",
                "stored_report_path": "sem/validations/demo.json"
            })
        );

        let commit = PathdbCommitResponseV1 {
            snapshot_id: PathdbSnapshotId::new("pathdb:7"),
            accepted_snapshot_id: AcceptedSnapshotId::new("accepted:42"),
            ops_added: 3,
        };
        assert_eq!(
            serde_json::to_value(&commit).expect("serialize pathdb commit response"),
            json!({
                "snapshot_id": "pathdb:7",
                "accepted_snapshot_id": "accepted:42",
                "ops_added": 3
            })
        );
    }

    #[test]
    fn capabilities_payload_exposes_typed_service_manifest() {
        let state = test_server_state_with_store_backed_axi(
            r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

theory Rules on S:
  constraint key Parent(child, parent)

instance Tiny of S:
  Person = {Alice, Bob}
  Parent = {
    (child=Alice, parent=Bob)
  }
"#,
        );

        let payload = capabilities_payload(state.as_ref()).expect("capabilities payload");
        assert_eq!(payload["version"], json!("axiograph_capabilities_v1"));
        assert_eq!(payload["query"]["machine_contract"], json!("query_ir_v1"));
        assert!(payload["semantic_services"]
            .as_array()
            .is_some_and(|services| services.iter().any(|service| {
                service["name"] == json!("semantic_agent_report")
                    && service["endpoint"] == json!("/semantic/agent-report")
            })));
        assert!(payload["semantic_services"]
            .as_array()
            .is_some_and(|services| services.iter().any(|service| {
                service["name"] == json!("semantic_context_report")
                    && service["endpoint"] == json!("/semantic/context-report")
            })));
        assert!(payload["semantic_services"]
            .as_array()
            .is_some_and(|services| services.iter().any(|service| {
                service["name"] == json!("semantic_behavior_case")
                    && service["endpoint"] == json!("/semantic/behavior-case")
            })));
        assert!(payload["semantic_services"]
            .as_array()
            .is_some_and(|services| services.iter().any(|service| {
                service["name"] == json!("semantic_theory_check")
                    && service["endpoint"] == json!("/semantic/theory-check")
            })));
        assert!(payload["tool_loop"]["tools"]
            .as_array()
            .is_some_and(|tools| tools.iter().any(|tool| tool["name"] == json!("axql_run"))));
    }

    #[test]
    fn capabilities_payload_hides_promotion_when_not_store_backed() {
        let state = test_server_state_with_axi(
            r#"
module Demo

schema S:
  object Person

instance Tiny of S:
  Person = {Alice}
"#,
        );

        let payload = capabilities_payload(state.as_ref()).expect("capabilities payload");
        assert!(payload["semantic_services"]
            .as_array()
            .is_some_and(|services| services
                .iter()
                .all(|service| { service["name"] != json!("promote_reviewed_module") })));
    }

    #[test]
    fn typed_anchor_responses_serialize_as_string_digests() {
        let query = QueryResponseV1 {
            vars: vec!["?x".to_string()],
            rows: Vec::new(),
            truncated: false,
            elapsed_ms: 5,
            trust: crate::trust_contract::TrustContractV1 {
                trust_class: "certifiable".to_string(),
                soundness: "certificate_available_but_not_emitted".to_string(),
                coverage: "full_query".to_string(),
                scope: crate::trust_contract::TrustScopeV1 {
                    anchor: "snapshot_scoped".to_string(),
                    context: "unscoped".to_string(),
                },
                reasons: Vec::new(),
                certifiable_disjuncts: None,
                execution_only_disjuncts: None,
                semantic_coverage: None,
                semantic_claims: Vec::new(),
                gaps: Vec::new(),
            },
            certificate_policy: crate::query_ir::QueryCertificatePolicyV1::None,
            prepared_query: None,
            compiled_query_ir_v1: None,
            elaborated_query_ir_v1: None,
            elaborated_query: None,
            inferred_types: None,
            notes: None,
            typed_holes: None,
            exploration_suggestions: None,
            plan: None,
            accepted_axi_anchor: Some(AcceptedAxiAnchor::new(
                AcceptedSnapshotId::new("accepted:42"),
                AxiDigest::new("fnv1a64:abc"),
            )),
            support_summary: Some(crate::evidence_support::EvidenceSupportSummaryV1 {
                version: crate::evidence_support::SUPPORT_SUMMARY_VERSION_V2.to_string(),
                accepted_axi_anchor: AcceptedAxiAnchor::new(
                    AcceptedSnapshotId::new("accepted:42"),
                    AxiDigest::new("fnv1a64:abc"),
                ),
                trust: crate::trust_contract::TrustContractV1 {
                    trust_class: "certifiable".to_string(),
                    soundness: "certificate_available_but_not_emitted".to_string(),
                    coverage: "full_query".to_string(),
                    scope: crate::trust_contract::TrustScopeV1 {
                        anchor: "snapshot_scoped".to_string(),
                        context: "single_context".to_string(),
                    },
                    reasons: Vec::new(),
                    certifiable_disjuncts: None,
                    execution_only_disjuncts: None,
                    semantic_coverage: None,
                    semantic_claims: Vec::new(),
                    gaps: Vec::new(),
                },
                basis: crate::evidence_support::SupportBasisV1 {
                    certificate_kind: "query_result_v3".to_string(),
                    source: "internal_runtime_certificate".to_string(),
                    certificate_emitted_to_client: false,
                    certificate_verified: None,
                },
                coverage: crate::evidence_support::SupportCoverageV1 {
                    rows_total: 1,
                    rows_with_support: 1,
                    witnesses_total: 1,
                    path_witnesses_supported: 1,
                    unsupported_witness_kinds: Vec::new(),
                },
                supported_facts: vec![crate::evidence_support::FactSupportRefV1 {
                    axi_fact_id: "factfnv1a64:abc".to_string(),
                    fact_entity_id: Some(17),
                    relation_name: Some("Fam.Parent".to_string()),
                    support_kind: "query_result_v3_path_step".to_string(),
                    witness_rows: vec![crate::evidence_support::SupportRowRefV1 {
                        row_index: 0,
                        disjunct: 0,
                    }],
                    contexts: vec![crate::evidence_support::SupportEntityRefV1 {
                        entity_id: 30,
                        entity_type: "Context".to_string(),
                        name: Some("CensusData".to_string()),
                        chunk_id: None,
                    }],
                    evidence: Vec::new(),
                    unresolved_chunk_ids: Vec::new(),
                    notes: Vec::new(),
                }],
                notes: vec!["runtime-only support summary".to_string()],
            }),
            anchor_digest: Some(AxiDigest::new("fnv1a64:abc")),
            certificate: None,
            certificate_verified: None,
            certificate_verify_output: None,
        };
        let query_json = serde_json::to_value(&query).expect("serialize query response");
        assert_eq!(query_json["anchor_digest"], json!("fnv1a64:abc"));
        assert_eq!(
            query_json["accepted_axi_anchor"],
            json!({
                "accepted_snapshot_id": "accepted:42",
                "axi_digest": "fnv1a64:abc"
            })
        );
        assert_eq!(
            query_json["support_summary"]["accepted_axi_anchor"],
            json!({
                "accepted_snapshot_id": "accepted:42",
                "axi_digest": "fnv1a64:abc"
            })
        );
        assert_eq!(
            query_json["support_summary"]["supported_facts"][0]["axi_fact_id"],
            json!("factfnv1a64:abc")
        );
        assert_eq!(
            query_json["support_summary"]["basis"]["certificate_kind"],
            json!("query_result_v3")
        );
        assert_eq!(
            query_json["support_summary"]["coverage"]["rows_total"],
            json!(1)
        );
        assert_eq!(
            query_json["support_summary"]["supported_facts"][0]["witness_rows"][0]["row_index"],
            json!(0)
        );
    }

    #[test]
    fn typed_server_requests_deserialize_snapshot_ids_from_string_wire_shape() {
        let llm: LlmAgentRequestV1 = serde_json::from_value(json!({
            "question": "who is connected to x?",
            "accepted_snapshot": "accepted:llm"
        }))
        .expect("deserialize llm request");
        assert_eq!(
            llm.accepted_snapshot.as_ref().map(|id| id.as_str()),
            Some("accepted:llm")
        );

        let propose: WorldModelProposeRequestV1 = serde_json::from_value(json!({
            "accepted_snapshot": "accepted:wm-propose"
        }))
        .expect("deserialize wm propose request");
        assert_eq!(
            propose.accepted_snapshot.as_ref().map(|id| id.as_str()),
            Some("accepted:wm-propose")
        );

        let plan: WorldModelPlanRequestV1 = serde_json::from_value(json!({
            "accepted_snapshot": "accepted:wm-plan"
        }))
        .expect("deserialize wm plan request");
        assert_eq!(
            plan.accepted_snapshot.as_ref().map(|id| id.as_str()),
            Some("accepted:wm-plan")
        );

        let commit: PathdbCommitRequestV1 = serde_json::from_value(json!({
            "accepted_snapshot": "accepted:pathdb",
            "chunks": [],
            "proposals": null
        }))
        .expect("deserialize pathdb commit request");
        assert_eq!(
            commit.accepted_snapshot.as_ref().map(|id| id.as_str()),
            Some("accepted:pathdb")
        );
    }

    #[test]
    fn parse_json_request_adds_endpoint_context_to_errors() {
        let err = parse_json_request::<serde_json::Value>(b"{", "demo/endpoint")
            .expect_err("malformed JSON should be rejected");

        assert!(err
            .to_string()
            .contains("failed to parse demo/endpoint request JSON"));
    }

    #[test]
    fn query_request_supports_structured_query_ir_wire_shape() {
        let req: QueryRequestV1 = serde_json::from_value(json!({
            "lang": "query_ir_v1",
            "query_ir_v1": {
                "version": 1,
                "select": ["?x"],
                "where": [
                    { "kind": "type", "term": "?x", "type": "A" }
                ],
                "limit": 5
            }
        }))
        .expect("deserialize query request with query_ir_v1");

        let parsed = query_request_to_axql_query(&req).expect("compile query_ir_v1");
        assert_eq!(parsed.select_vars, vec!["?x".to_string()]);
        assert_eq!(parsed.limit, 5);
    }

    #[test]
    fn query_request_rejects_raw_query_text_wire_shape() {
        let req: QueryRequestV1 = serde_json::from_value(json!({
            "query": "select ?x where ?x is A limit 5",
            "lang": "axql"
        }))
        .expect("deserialize raw query request");

        let err = query_request_to_axql_query(&req).expect_err("raw query text should be rejected");
        assert!(err
            .to_string()
            .contains("raw `query` text is no longer accepted"));
    }

    #[test]
    fn query_request_rejects_certificate_boolean_aliases() {
        for field in [
            "certify",
            "verify",
            "require_query_certs",
            "require_verified_queries",
        ] {
            let mut value = json!({
                "lang": "query_ir_v1",
                "query_ir_v1": {
                    "version": 1,
                    "select": ["?x"],
                    "where": [
                        { "kind": "type", "term": "?x", "type": "A" }
                    ],
                    "limit": 5
                }
            });
            value
                .as_object_mut()
                .expect("request object")
                .insert(field.to_string(), json!(true));
            let err = serde_json::from_value::<QueryRequestV1>(value)
                .expect_err("certificate boolean aliases should not deserialize");

            assert!(
                err.to_string()
                    .contains(&format!("unknown field `{field}`")),
                "expected unknown-field error for `{field}`, got {err}"
            );
        }
    }

    #[test]
    fn llm_agent_request_rejects_query_certificate_boolean_aliases() {
        for field in [
            "certify_queries",
            "verify_queries",
            "require_query_certs",
            "require_verified_queries",
        ] {
            let mut value = json!({
                "question": "find Person named Alice"
            });
            value
                .as_object_mut()
                .expect("request object")
                .insert(field.to_string(), json!(true));
            let err = serde_json::from_value::<LlmAgentRequestV1>(value)
                .expect_err("LLM query-certificate boolean aliases should not deserialize");

            assert!(
                err.to_string()
                    .contains(&format!("unknown field `{field}`")),
                "expected unknown-field error for `{field}`, got {err}"
            );
        }
    }

    #[test]
    fn llm_to_query_request_rejects_query_certificate_boolean_aliases() {
        for field in [
            "certify_queries",
            "verify_queries",
            "require_query_certs",
            "require_verified_queries",
        ] {
            let mut value = json!({
                "question": "find Person named Alice"
            });
            value
                .as_object_mut()
                .expect("request object")
                .insert(field.to_string(), json!(true));
            let err = serde_json::from_value::<LlmToQueryRequestV1>(value)
                .expect_err("LLM query-certificate boolean aliases should not deserialize");

            assert!(
                err.to_string()
                    .contains(&format!("unknown field `{field}`")),
                "expected unknown-field error for `{field}`, got {err}"
            );
        }
    }

    fn test_server_state_with_axi(axi_text: &str) -> Arc<ServerState> {
        let mut db = PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi_text)
            .expect("import canonical axi text into pathdb");
        db.build_indexes();
        let db = Arc::new(db);
        let meta = MetaPlaneIndex::from_db(&db).ok();

        Arc::new(ServerState {
            config: ServerConfig {
                listen: "127.0.0.1:0".parse().expect("socket addr"),
                role: ServerRole::Standalone,
                source: SnapshotSource::Axpd(PathBuf::from("test.axpd")),
                watch_head: false,
                poll_interval: Duration::from_secs(60),
                admin_token: None,
                ready_file: None,
                cert_verify: CertVerifyConfig {
                    verifier_bin: None,
                    timeout: None,
                },
                llm: LlmState::default(),
                world_model: WorldModelState {
                    backend: WorldModelBackend::Disabled,
                    model: None,
                },
                world_model_workers: 1,
                path_index_lru_capacity: 1024,
                path_index_lru_async: false,
                path_index_lru_queue: 128,
            },
            loaded: RwLock::new(LoadedSnapshot {
                snapshot_key: "test-snapshot".to_string(),
                snapshot_label: "test snapshot".to_string(),
                accepted_snapshot_id: None,
                accepted_axi_anchor: None,
                accepted_axi_text: None,
                pathdb_snapshot_id: None,
                loaded_at_unix_secs: 0,
                entities: db.entities.len(),
                relations: db.relations.len(),
                db,
                meta,
                embeddings: None,
            }),
            query_cache: Mutex::new(QueryPlanCache::default()),
            world_model_executor: WorldModelExecutor::new(1),
        })
    }

    fn test_server_state_with_store_backed_axi(axi_text: &str) -> Arc<ServerState> {
        let mut db = PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi_text)
            .expect("import canonical axi text into pathdb");
        db.build_indexes();
        let db = Arc::new(db);
        let meta = MetaPlaneIndex::from_db(&db).ok();
        let digest = AxiDigest::from_axi_text(axi_text);

        Arc::new(ServerState {
            config: ServerConfig {
                listen: "127.0.0.1:0".parse().expect("socket addr"),
                role: ServerRole::Standalone,
                source: SnapshotSource::Store {
                    dir: PathBuf::from("test-store"),
                    layer: "accepted".to_string(),
                    snapshot: "head".to_string(),
                },
                watch_head: false,
                poll_interval: Duration::from_secs(60),
                admin_token: None,
                ready_file: None,
                cert_verify: CertVerifyConfig {
                    verifier_bin: None,
                    timeout: None,
                },
                llm: LlmState::default(),
                world_model: WorldModelState {
                    backend: WorldModelBackend::Disabled,
                    model: None,
                },
                world_model_workers: 1,
                path_index_lru_capacity: 1024,
                path_index_lru_async: false,
                path_index_lru_queue: 128,
            },
            loaded: RwLock::new(LoadedSnapshot {
                snapshot_key: "test-store-backed-snapshot".to_string(),
                snapshot_label: "test store snapshot".to_string(),
                accepted_snapshot_id: Some(AcceptedSnapshotId::new("accepted:test")),
                accepted_axi_anchor: Some(AcceptedAxiAnchor::new(
                    AcceptedSnapshotId::new("accepted:test"),
                    digest.clone(),
                )),
                accepted_axi_text: Some(axi_text.to_string()),
                pathdb_snapshot_id: None,
                loaded_at_unix_secs: 0,
                entities: db.entities.len(),
                relations: db.relations.len(),
                db,
                meta,
                embeddings: None,
            }),
            query_cache: Mutex::new(QueryPlanCache::default()),
            world_model_executor: WorldModelExecutor::new(1),
        })
    }

    #[tokio::test]
    async fn handle_query_accepts_query_ir_v1_and_returns_compiled_ir() {
        let state = test_server_state_with_store_backed_axi(
            r#"module Demo

schema S:
  object A

instance I of S:
  A = {x, y}
"#,
        );

        let body = serde_json::to_vec(&json!({
            "lang": "query_ir_v1",
            "query_ir_v1": {
                "version": 1,
                "select": ["?x"],
                "where": [
                    { "kind": "type", "term": "?x", "type": "A" }
                ],
                "limit": 10
            },
            "show_elaboration": true
        }))
        .expect("serialize query request");

        let resp = handle_query(&state, &body)
            .await
            .expect("query_ir_v1 request should execute");

        assert_eq!(resp.vars, vec!["?x".to_string()]);
        assert_eq!(resp.rows.len(), 2);
        assert!(resp.compiled_query_ir_v1.is_some());
        assert!(resp.elaborated_query_ir_v1.is_some());
        assert!(resp.elaborated_query.is_some());
        assert!(resp.typed_holes.is_some());
        assert!(resp.exploration_suggestions.is_some());
        let prepared_query = resp
            .prepared_query
            .as_ref()
            .expect("/query should return prepared-query metadata");
        assert!(prepared_query.query_ir_id.starts_with("query_ir_v1:"));
        assert!(prepared_query
            .prepared_query_id
            .starts_with("prepared_query_v1:"));
        assert_eq!(prepared_query.trust.trust_class, "certifiable");
        assert_eq!(prepared_query.non_claims.completeness_claim, "not_claimed");
        assert!(prepared_query.kernel_refs.iter().any(|reference| {
            matches!(reference, axiograph_pathdb::KernelRefV1::Module { .. })
        }));
        assert!(prepared_query.kernel_refs.iter().any(|reference| {
            matches!(
                reference,
                axiograph_pathdb::KernelRefV1::SchemaObject {
                    object: axiograph_pathdb::SchemaCategoryObjectRefIr::ObjectType {
                        name,
                        ..
                    },
                    ..
                } if name == "A"
            )
        }));
        assert_eq!(
            resp.certificate_policy,
            crate::query_ir::QueryCertificatePolicyV1::None
        );
        assert_eq!(resp.trust.trust_class, "certifiable");
        assert_eq!(
            resp.trust.soundness,
            "certificate_available_but_not_emitted"
        );
        assert_eq!(resp.trust.scope.anchor, "snapshot_scoped");
        assert_eq!(resp.trust.scope.context, "unscoped");
    }

    #[tokio::test]
    async fn handle_query_require_verified_fails_closed_without_accepted_anchor() {
        let state = test_server_state_with_axi(
            r#"
module Demo

schema S:
  object A

instance I of S:
  A = {x}
"#,
        );

        let body = serde_json::to_vec(&json!({
            "lang": "query_ir_v1",
            "query_ir_v1": {
                "version": 1,
                "select": ["?x"],
                "where": [
                    { "kind": "type", "term": "?x", "type": "A" }
                ],
                "limit": 10
            },
            "certificate_policy": "require_verified"
        }))
        .expect("serialize query request");

        let err = handle_query(&state, &body)
            .await
            .expect_err("require_verified must fail closed without an accepted anchor");
        assert!(err
            .to_string()
            .contains("query certificate policy `require_verified`"));
        assert!(err.to_string().contains("missing accepted `.axi` anchor"));
    }

    #[tokio::test]
    async fn handle_query_store_backed_certified_response_includes_support_summary() {
        let state = test_server_state_with_store_backed_axi(
            r#"
module Demo

schema S:
  object Person
  object Context
  relation Parent(child: Person, parent: Person) @context Context

instance I of S:
  Person = {Alice, Bob}
  Context = {CensusData}
  Parent = {
    (child=Alice, parent=Bob, ctx=CensusData)
  }
"#,
        );

        let body = serde_json::to_vec(&json!({
            "lang": "query_ir_v1",
            "query_ir_v1": {
                "version": 1,
                "select": ["?p"],
                "where": [
                    {
                        "kind": "fact",
                        "fact": "?f",
                        "relation": "S.Parent",
                        "fields": {
                            "child": "Alice",
                            "parent": "?p",
                            "ctx": "CensusData"
                        }
                    }
                ],
                "limit": 10
            },
            "certificate_policy": "emit"
        }))
        .expect("serialize query request");

        let resp = handle_query(&state, &body)
            .await
            .expect("store-backed certified query should execute");

        let support = resp
            .support_summary
            .as_ref()
            .expect("support summary should be present for anchored certified queries");
        assert_eq!(
            support.accepted_axi_anchor.accepted_snapshot_id.as_str(),
            "accepted:test"
        );
        assert_eq!(support.basis.certificate_kind, "query_result_v3");
        assert!(support.basis.certificate_emitted_to_client);
        assert_eq!(
            support.trust.soundness,
            "certificate_emitted_row_soundness_unverified"
        );
        assert!(!support.supported_facts.is_empty());
        assert!(support
            .supported_facts
            .iter()
            .all(|fact| !fact.witness_rows.is_empty()));
        assert!(support.supported_facts.iter().any(|fact| {
            fact.contexts
                .iter()
                .any(|ctx| ctx.name.as_deref() == Some("CensusData"))
        }));
    }

    #[tokio::test]
    async fn handle_query_store_backed_response_includes_support_summary_without_certificate_emit()
    {
        let state = test_server_state_with_store_backed_axi(
            r#"
module Demo

schema S:
  object Person
  object Context
  relation Parent(child: Person, parent: Person) @context Context

instance I of S:
  Person = {Alice, Bob}
  Context = {CensusData}
  Parent = {
    (child=Alice, parent=Bob, ctx=CensusData)
  }
"#,
        );

        let body = serde_json::to_vec(&json!({
            "lang": "query_ir_v1",
            "query_ir_v1": {
                "version": 1,
                "select": ["?p"],
                "where": [
                    {
                        "kind": "fact",
                        "fact": "?f",
                        "relation": "S.Parent",
                        "fields": {
                            "child": "Alice",
                            "parent": "?p",
                            "ctx": "CensusData"
                        }
                    }
                ],
                "limit": 10
            }
        }))
        .expect("serialize query request");

        let resp = handle_query(&state, &body)
            .await
            .expect("store-backed query should execute");

        assert!(resp.certificate.is_none());
        let support = resp
            .support_summary
            .as_ref()
            .expect("support summary should be present for anchored certifiable queries");
        assert_eq!(support.basis.certificate_kind, "query_result_v3");
        assert!(!support.basis.certificate_emitted_to_client);
        assert_eq!(
            support.trust.soundness,
            "certificate_available_but_not_emitted"
        );
        assert_eq!(support.coverage.rows_total, 1);
        assert_eq!(support.coverage.rows_with_support, 1);
        assert!(support
            .supported_facts
            .iter()
            .all(|fact| !fact.witness_rows.is_empty()));
    }

    #[tokio::test]
    async fn handle_query_uses_default_contexts_in_compiled_query_ir() {
        let state = test_server_state_with_axi(
            r#"
module Demo

schema S:
  object A
  object Context
  relation Parent(child: A, parent: A) @context Context

instance I of S:
  A = {x, y}
  Context = {FamilyTree}
  Parent = {
    (child=x, parent=y, ctx=FamilyTree)
  }
"#,
        );

        let body = serde_json::to_vec(&json!({
            "lang": "query_ir_v1",
            "query_ir_v1": {
                "version": 1,
                "select": ["?x"],
                "where": [
                    { "kind": "type", "term": "?x", "type": "A" }
                ],
                "limit": 10
            },
            "contexts": ["FamilyTree"],
            "show_elaboration": true
        }))
        .expect("serialize query request");

        let resp = handle_query(&state, &body)
            .await
            .expect("query with default contexts should execute");

        assert_eq!(
            resp.compiled_query_ir_v1
                .as_ref()
                .map(|ir| ir.contexts.len()),
            Some(1)
        );
    }

    #[tokio::test]
    async fn handle_query_surfaces_typed_holes_for_ambiguous_but_certifiable_query() {
        let state = test_server_state_with_axi(
            r#"
module Universe

schema Fam:
  object Person
  relation Parent(child: Person, parent: Person)

schema Census:
  object Person
  relation Parent(child: Person, parent: Person)

instance FamInst of Fam:
  Person = {Alice, Bob, Carol}
  Parent = {(child=Carol, parent=Alice), (child=Carol, parent=Bob)}

instance CensusInst of Census:
  Person = {Alice, Bob, Dan}
  Parent = {(child=Dan, parent=Alice), (child=Dan, parent=Bob)}
"#,
        );

        let body = serde_json::to_vec(&json!({
            "lang": "query_ir_v1",
            "query_ir_v1": {
                "version": 1,
                "select": ["?p"],
                "where": [
                    { "kind": "edge", "left": "Carol", "path": "Parent", "right": "?p" }
                ],
                "limit": 10
            },
            "show_elaboration": true
        }))
        .expect("serialize ambiguous query request");

        let resp = handle_query(&state, &body)
            .await
            .expect("ambiguous query request should execute");

        assert_eq!(resp.trust.trust_class, "certifiable");
        assert!(resp
            .typed_holes
            .as_ref()
            .is_some_and(|holes| !holes.is_empty()));
        assert!(resp
            .typed_holes
            .as_ref()
            .is_some_and(|holes| holes.iter().any(|hole| {
                hole.kind == crate::axql::AxqlTypedHoleKindV1::AmbiguousEdgeRelationSchema
                    && hole.relation == "Parent"
            })));
        assert!(resp.elaborated_query_ir_v1.is_some());
        assert!(resp.exploration_suggestions.is_some());
    }

    #[tokio::test]
    async fn handle_discover_draft_axi_returns_typed_authoring_summary() {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
        let db = crate::load_pathdb_for_cli(&repo_root.join("examples/Family.axi"))
            .expect("load Family.axi");
        let generated = crate::proposal_gen::propose_relation_proposals_v1(
            &db,
            &[],
            crate::proposal_gen::ProposeRelationInputV1 {
                rel_type: "Parent".to_string(),
                source_name: "Jamison".to_string(),
                target_name: "Bob".to_string(),
                source_type: None,
                target_type: None,
                source_field: Some("child".to_string()),
                target_field: Some("parent".to_string()),
                context: Some("FamilyTree".to_string()),
                time: Some("T2025".to_string()),
                confidence: Some(0.9),
                schema_hint: Some("Fam".to_string()),
                public_rationale: Some("Jamison is a child of Bob.".to_string()),
                evidence_text: None,
                evidence_locator: None,
                extra_fields: std::collections::HashMap::new(),
            },
        )
        .expect("generate proposals");

        let body = serde_json::to_vec(&json!({
            "proposals": generated.proposals,
            "module_name": "DraftFamily",
            "schema_name": "DraftFam",
            "instance_name": "DraftFamilyInst",
            "infer_constraints": true
        }))
        .expect("serialize draft-axi request");

        let state = test_server_state_with_axi(
            r#"
module Demo

schema S:
  object A

instance I of S:
  A = {x}
"#,
        );

        let resp = handle_discover_draft_axi(&state, &body)
            .await
            .expect("draft-axi endpoint should succeed");

        assert_eq!(
            resp["version"].as_str(),
            Some("axiograph_discover_draft_axi_v1")
        );
        assert_eq!(
            resp["typed_authoring"]["lifecycle_state"].as_str(),
            Some("validated")
        );
        assert_eq!(
            resp["typed_authoring"]["trust"]["trust_class"].as_str(),
            Some("validated_draft")
        );
        assert_eq!(
            resp["typed_authoring"]["trust"]["completeness_claim"].as_str(),
            Some("not_claimed")
        );
        assert_eq!(
            resp["typed_authoring"]["axi_well_typed_proof_v1"]["module_name"].as_str(),
            Some("DraftFamily")
        );
        assert_eq!(
            resp["typed_authoring"]["kernel_module_ir"]["instances"][0]["instance_id"].as_str(),
            Some("instance:DraftFam:DraftFamilyInst")
        );
        assert_eq!(
            resp["typed_authoring"]["kernel_module_ir"]["instances"][0]["relation_facts"]
                .as_array()
                .map(|facts| facts.len()),
            Some(1)
        );
        assert_eq!(
            resp["exploration_preview"]["kind"].as_str(),
            Some("compiled_ir_directed_exploration")
        );
        assert_eq!(
            resp["exploration_preview"]["typed_change"]["kind"].as_str(),
            Some("compiled_ir_directed_exploration")
        );
        let draft_primitives = resp["exploration_preview"]["typed_change"]["primitives"]
            .as_array()
            .expect("draft exploration primitives should be an array");
        assert!(
            draft_primitives
                .iter()
                .any(|primitive| primitive["kind"].as_str() == Some("reify_relation_object")),
            "draft exploration preview should surface relation-object semantics for some discovered relation"
        );
        assert!(
            draft_primitives.iter().any(|primitive| {
                primitive["kind"].as_str() == Some("introduce_dependent_relation_family")
            }),
            "draft exploration preview should surface dependent-family semantics for indexed discovered relations"
        );
    }

    #[tokio::test]
    async fn handle_discover_check_olog_returns_validated_fragment() {
        let fragment = crate::typed_authoring::OlogFragmentV1 {
            boxes: vec![
                crate::typed_authoring::OlogBoxV1 {
                    box_id: "employee".to_string(),
                    object_type: "Person".to_string(),
                    label: None,
                },
                crate::typed_authoring::OlogBoxV1 {
                    box_id: "team".to_string(),
                    object_type: "Team".to_string(),
                    label: None,
                },
                crate::typed_authoring::OlogBoxV1 {
                    box_id: "ctx".to_string(),
                    object_type: "Context".to_string(),
                    label: None,
                },
            ],
            relation_boxes: vec![crate::typed_authoring::OlogRelationBoxV1 {
                box_id: "works_for_fact".to_string(),
                relation: "WorksFor".to_string(),
                role_bindings: vec![
                    crate::typed_authoring::OlogRoleBindingV1 {
                        role: "employee".to_string(),
                        target_box: "employee".to_string(),
                    },
                    crate::typed_authoring::OlogRoleBindingV1 {
                        role: "employer".to_string(),
                        target_box: "team".to_string(),
                    },
                    crate::typed_authoring::OlogRoleBindingV1 {
                        role: "ctx".to_string(),
                        target_box: "ctx".to_string(),
                    },
                ],
            }],
            aspects: vec![crate::typed_authoring::OlogAspectV1 {
                aspect_id: "employee_role".to_string(),
                from_box: "works_for_fact".to_string(),
                to_box: "employee".to_string(),
                kind: crate::typed_authoring::OlogAspectKindV1::RelationProjection {
                    relation_box: "works_for_fact".to_string(),
                    role: "employee".to_string(),
                },
            }],
            path_equations: vec![crate::typed_authoring::OlogPathEquationV1 {
                equation_id: "employee_identity".to_string(),
                lhs: crate::typed_authoring::OlogPathV1 {
                    steps: vec!["employee_role".to_string()],
                },
                rhs: crate::typed_authoring::OlogPathV1 {
                    steps: vec!["employee_role".to_string()],
                },
            }],
        };

        let body = serde_json::to_vec(&json!({
            "axi_text": r#"
module Demo

schema S:
  object Person
  object Team
  object Context
  relation WorksFor(employee: Person, employer: Team, ctx: Context)

instance I of S:
  Person = {Alice}
  Team = {Ops}
  Context = {Prod}
  WorksFor = {(employee=Alice, employer=Ops, ctx=Prod)}
"#,
            "schema_name": "S",
            "fragment": fragment,
        }))
        .expect("serialize discover/check-olog request");

        let resp = handle_discover_check_olog(&body)
            .await
            .expect("check-olog endpoint should succeed");
        assert_eq!(
            resp["version"].as_str(),
            Some("axiograph_discover_check_olog_v1")
        );
        assert_eq!(resp["checked_olog"]["ok"].as_bool(), Some(true));
        assert_eq!(
            resp["checked_olog"]["trust"]["trust_class"].as_str(),
            Some("validated_olog_fragment")
        );
        assert_eq!(
            resp["checked_olog"]["typed_change"]["kind"].as_str(),
            Some("olog_fragment_delta")
        );
        assert!(resp["checked_olog"]["refinement_candidates"].is_null());
        let primitives = resp["checked_olog"]["typed_change"]["primitives"]
            .as_array()
            .expect("typed change primitives should be serialized");
        assert!(primitives.iter().any(|primitive| {
            primitive["kind"].as_str() == Some("reify_relation_object")
                && primitive["relation"].as_str() == Some("WorksFor")
        }));
        assert!(primitives.iter().any(|primitive| {
            primitive["kind"].as_str() == Some("introduce_dependent_relation_family")
                && primitive["relation"].as_str() == Some("WorksFor")
        }));
        assert!(primitives.iter().any(|primitive| {
            primitive["kind"].as_str() == Some("add_path_equation")
                && primitive["equation_id"].as_str() == Some("employee_identity")
        }));
        assert_eq!(
            resp["evolution_preview"]["kind"].as_str(),
            Some("typed_olog_authoring")
        );
        assert_eq!(
            resp["evolution_preview"]["typed_change"]["kind"].as_str(),
            Some("olog_fragment_delta")
        );
    }

    #[tokio::test]
    async fn handle_discover_check_olog_rejects_pathdb_export_axi_text() {
        let body = serde_json::to_vec(&json!({
            "axi_text": include_str!("../../../../examples/anchors/pathdb_export_anchor_v1.axi"),
            "schema_name": "PathDBExportV1",
            "fragment": {}
        }))
        .expect("serialize discover/check-olog request");

        let err = handle_discover_check_olog(&body)
            .await
            .expect_err("check-olog endpoint should reject PathDBExportV1");
        assert!(err
            .to_string()
            .contains("expected a canonical .axi module, but input is a PathDBExportV1 snapshot"));
    }

    #[tokio::test]
    async fn handle_discover_check_olog_can_apply_refinement_handle() {
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
        let fragment = crate::typed_authoring::OlogFragmentV1 {
            boxes: vec![
                crate::typed_authoring::OlogBoxV1 {
                    box_id: "employee".to_string(),
                    object_type: "Person".to_string(),
                    label: None,
                },
                crate::typed_authoring::OlogBoxV1 {
                    box_id: "team".to_string(),
                    object_type: "Team".to_string(),
                    label: None,
                },
                crate::typed_authoring::OlogBoxV1 {
                    box_id: "ctx".to_string(),
                    object_type: "Context".to_string(),
                    label: None,
                },
            ],
            relation_boxes: vec![crate::typed_authoring::OlogRelationBoxV1 {
                box_id: "works_for_fact".to_string(),
                relation: "WorksFor".to_string(),
                role_bindings: vec![
                    crate::typed_authoring::OlogRoleBindingV1 {
                        role: "employee".to_string(),
                        target_box: "employee".to_string(),
                    },
                    crate::typed_authoring::OlogRoleBindingV1 {
                        role: "ctx".to_string(),
                        target_box: "ctx".to_string(),
                    },
                ],
            }],
            aspects: Vec::new(),
            path_equations: Vec::new(),
        };
        let checked = crate::typed_authoring::check_olog_fragment_against_axi_text(
            axi_text,
            Some("S"),
            fragment.clone(),
        );
        let handle_id = checked
            .refinement_candidates
            .first()
            .map(|candidate| candidate.handle.id.clone())
            .expect("expected olog refinement handle");

        let body = serde_json::to_vec(&json!({
            "axi_text": axi_text,
            "schema_name": "S",
            "fragment": fragment,
            "apply_refinement_handle_id": handle_id,
        }))
        .expect("serialize discover/check-olog apply request");

        let resp = handle_discover_check_olog(&body)
            .await
            .expect("check-olog apply endpoint should succeed");
        assert_eq!(resp["checked_olog"]["ok"].as_bool(), Some(true));
        assert_eq!(
            resp["checked_olog"]["lifecycle_state"].as_str(),
            Some("runtime_checked_theory_enriched")
        );
        assert_eq!(
            resp["applied_refinement"]["handle"]["id"].as_str(),
            Some(handle_id.as_str())
        );
        let bindings = resp["applied_refinement"]["refined_fragment"]["relation_boxes"][0]
            ["role_bindings"]
            .as_array()
            .expect("refined role bindings");
        assert!(bindings.iter().any(|binding| {
            binding["role"].as_str() == Some("employer")
                && binding["target_box"].as_str() == Some("team")
        }));
        assert_eq!(resp["evolution_preview"]["ok"].as_bool(), Some(true));
    }

    #[tokio::test]
    async fn handle_semantic_coverage_returns_rule_coverage_report() {
        let state = test_server_state_with_axi(
            r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

theory TRules on S:
  constraint functional Parent.child -> Parent.parent

instance I of S:
  Person = {Alice, Bob}
  Parent = {(child=Alice, parent=Bob)}
"#,
        );

        let body = serde_json::to_vec(&json!({
            "surfaces": [{
                "surface_id": "endpoint:parent_lookup",
                "kind": "endpoint",
                "label": "GET /parent",
                "scopes": [{
                    "schema": "S",
                    "scope_class": "relation",
                    "relation": "Parent"
                }],
                "code_refs": ["src/parent.rs"]
            }],
            "edges": [{
                "surface_id": "endpoint:parent_lookup",
                "rule_id": "schema/s/relation/parent/rule/functional/0",
                "status": "tested",
                "notes": ["covered by endpoint integration test"]
            }]
        }))
        .expect("serialize semantic coverage request");

        let resp = handle_semantic_coverage(&state, &body)
            .await
            .expect("semantic coverage endpoint should succeed");
        assert_eq!(
            resp["version"].as_str(),
            Some("axiograph_semantic_coverage_v1")
        );
        assert_eq!(
            resp["coverage"]["version"].as_str(),
            Some(crate::semantic_claim::SEMANTIC_COVERAGE_REPORT_VERSION_V1)
        );
        assert_eq!(resp["coverage"]["total_rules"].as_u64(), Some(1));
        assert_eq!(resp["coverage"]["tested_rules"].as_u64(), Some(1));
        assert_eq!(resp["coverage"]["covered_rules"].as_u64(), Some(1));
        assert_eq!(
            resp["coverage"]["surface_reports"][0]["surface"]["scopes"][0]["scope_id"].as_str(),
            Some("schema/s/relation/parent")
        );
    }

    #[tokio::test]
    async fn handle_semantic_business_rule_returns_applicability_report() {
        let state = test_server_state_with_axi(
            r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

theory TRules on S:
  constraint functional Parent.child -> Parent.parent

instance I of S:
  Person = {Alice, Bob}
  Parent = {(child=Alice, parent=Bob)}
"#,
        );

        let body = serde_json::to_vec(&json!({
            "scope": {
                "scope_id": "schema/s/relation/parent",
                "schema": "S",
                "scope_class": "relation",
                "relation": "Parent"
            }
        }))
        .expect("serialize semantic business-rule request");

        let resp = handle_semantic_business_rule(&state, &body)
            .await
            .expect("semantic/business-rule endpoint should succeed");
        assert_eq!(
            resp["version"].as_str(),
            Some("axiograph_semantic_business_rule_v1")
        );
        assert_eq!(
            resp["report"]["version"].as_str(),
            Some(crate::semantic_claim::BUSINESS_RULE_APPLICABILITY_REPORT_VERSION_V1)
        );
        assert_eq!(
            resp["report"]["scope"]["scope_id"].as_str(),
            Some("schema/s/relation/parent")
        );
        assert_eq!(
            resp["report"]["trust_class"].as_str(),
            Some("runtime_enforced")
        );
    }

    #[tokio::test]
    async fn handle_semantic_agent_report_returns_agent_facing_engineering_report() {
        let state = test_server_state_with_axi(
            r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

theory TRules on S:
  constraint functional Parent.child -> Parent.parent

instance I of S:
  Person = {Alice, Bob}
  Parent = {(child=Alice, parent=Bob)}
"#,
        );

        let body = serde_json::to_vec(&json!({
            "task": {
                "task_id": "task:family_endpoint_alignment",
                "label": "Align family endpoint",
                "objective": "check whether endpoint and docs match accepted ontology",
                "languages": ["rust", "typescript"],
                "artifact_refs": ["src/family.rs", "ui/family.tsx"]
            },
            "surfaces": [{
                "surface_id": "endpoint:family_tree",
                "kind": "endpoint",
                "label": "GET /family/tree",
                "scopes": [{
                    "schema": "S",
                    "scope_class": "relation",
                    "relation": "Parent"
                }],
                "code_refs": ["src/family.rs"]
            }],
            "edges": [{
                "surface_id": "endpoint:family_tree",
                "rule_id": "schema/s/relation/parent/rule/functional/0",
                "status": "implemented",
                "notes": ["checked in endpoint handler"]
            }]
        }))
        .expect("serialize semantic agent report request");

        let resp = handle_semantic_agent_report(&state, &body)
            .await
            .expect("semantic/agent-report endpoint should succeed");
        assert_eq!(
            resp["version"].as_str(),
            Some("axiograph_semantic_agent_report_v1")
        );
        assert_eq!(
            resp["report"]["version"].as_str(),
            Some(crate::semantic_claim::AGENT_ENGINEERING_REPORT_VERSION_V1)
        );
        assert_eq!(
            resp["report"]["task"]["task_id"].as_str(),
            Some("task:family_endpoint_alignment")
        );
        assert_eq!(
            resp["report"]["trust_class"].as_str(),
            Some("runtime_enforced")
        );
        assert_eq!(
            resp["report"]["coverage"]["implemented_rules"].as_u64(),
            Some(1)
        );
        assert_eq!(
            resp["report"]["matched_scope_ids"][0].as_str(),
            Some("schema/s/relation/parent")
        );
        assert!(resp["report"]["matched_rule_ids"]
            .as_array()
            .is_some_and(|items| !items.is_empty()));
    }

    #[tokio::test]
    async fn handle_semantic_context_report_returns_bounded_context_report() {
        let state = test_server_state_with_axi(
            r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

theory TRules on S:
  constraint functional Parent.child -> Parent.parent

instance I of S:
  Person = {Alice, Bob}
  Parent = {(child=Alice, parent=Bob)}
"#,
        );

        let body = serde_json::to_vec(&json!({
            "context": {
                "context_id": "domain:family_lookup",
                "label": "Family lookup",
                "scopes": [{
                    "schema": "S",
                    "scope_class": "relation",
                    "relation": "Parent"
                }],
                "surfaces": [{
                    "surface_id": "endpoint:family_lookup",
                    "kind": "endpoint",
                    "label": "GET /family/lookup",
                    "scopes": [{
                        "schema": "S",
                        "scope_class": "relation",
                        "relation": "Parent"
                    }],
                    "code_refs": ["src/family.rs"]
                }],
                "edges": [{
                    "surface_id": "endpoint:family_lookup",
                    "rule_id": "schema/s/relation/parent/rule/functional/0",
                    "status": "tested"
                }],
                "competency_questions": [{
                    "name": "family_lookup_returns_bob",
                    "query": "select ?f where ?f = S.Parent(child=Alice, parent=Bob) limit 1",
                    "min_rows": 1,
                    "weight": 1.0
                }]
            }
        }))
        .expect("serialize semantic context report request");

        let resp = handle_semantic_context_report(&state, &body)
            .await
            .expect("semantic/context-report endpoint should succeed");
        assert_eq!(
            resp["version"].as_str(),
            Some("axiograph_semantic_context_report_v1")
        );
        assert_eq!(
            resp["report"]["version"].as_str(),
            Some(crate::context_report::CONTEXT_REPORT_VERSION_V1)
        );
        assert_eq!(
            resp["report"]["context"]["context_id"].as_str(),
            Some("domain:family_lookup")
        );
        assert_eq!(resp["report"]["coverage"]["tested_rules"].as_u64(), Some(1));
        assert_eq!(
            resp["report"]["competency_coverage"]["satisfied"].as_u64(),
            Some(1)
        );
    }

    #[tokio::test]
    async fn handle_semantic_behavior_case_returns_receipt_and_codegen() {
        let state = test_server_state_with_axi(
            r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

theory TRules on S:
  constraint functional Parent.child -> Parent.parent

instance I of S:
  Person = {Alice, Bob}
  Parent = {(child=Alice, parent=Bob)}
"#,
        );

        let body = serde_json::to_vec(&json!({
            "behavior_case": {
                "case_id": "family.parent_lookup",
                "title": "Family lookup returns parent",
                "then": {
                    "expected_outcomes": ["Alice has Bob as parent"],
                    "competency_questions": [{
                        "name": "family_lookup_returns_bob",
                        "query": "select ?f where ?f = S.Parent(child=Alice, parent=Bob) limit 1",
                        "min_rows": 1,
                        "weight": 1.0
                    }],
                    "rule_scopes": [{
                        "schema": "S",
                        "scope_class": "relation",
                        "relation": "Parent"
                    }],
                    "trust_target": "strong"
                }
            },
            "overlay": {
                "version": axiograph_tooling_overlays::TOOLING_OVERLAY_BUNDLE_VERSION_V1,
                "fddd_context_map": {
                    "context_id": "domain:family_lookup",
                    "label": "Family lookup",
                    "scopes": [{
                        "kind": "relation",
                        "schema": "S",
                        "name": "Parent"
                    }]
                },
                "implementation_surfaces": {
                    "surfaces": [{
                        "surface_id": "endpoint:family_lookup",
                        "kind": "endpoint",
                        "label": "GET /family/lookup",
                        "ontology_refs": [{
                            "kind": "relation",
                            "schema": "S",
                            "name": "Parent"
                        }],
                        "code_refs": ["src/family.rs"]
                    }],
                    "coverage_edges": [{
                        "surface_id": "endpoint:family_lookup",
                        "rule_id": "schema/s/relation/parent/rule/functional/0",
                        "status": "tested"
                    }]
                },
                "codegen_plan": {
                    "languages": ["rust", "typescript"]
                }
            }
        }))
        .expect("serialize semantic behavior case request");

        let resp = handle_semantic_behavior_case(&state, &body)
            .await
            .expect("semantic/behavior-case endpoint should succeed");
        assert_eq!(
            resp["version"].as_str(),
            Some("axiograph_semantic_behavior_case_v1")
        );
        assert_eq!(
            resp["report"]["version"].as_str(),
            Some(crate::behavior_case::BEHAVIOR_CASE_REPORT_VERSION_V1)
        );
        assert_eq!(
            resp["report"]["receipt"]["case_id"].as_str(),
            Some("family.parent_lookup")
        );
        assert_eq!(
            resp["report"]["receipt"]["competency_satisfied"].as_u64(),
            Some(1)
        );
        assert_eq!(
            resp["report"]["codegen_previews"].as_array().map(Vec::len),
            Some(2)
        );
    }

    #[tokio::test]
    async fn handle_semantic_authoring_endpoints_return_overlay_and_codegen_reports() {
        let axi_text = r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)
"#;
        let overlay = json!({
            "version": axiograph_tooling_overlays::TOOLING_OVERLAY_BUNDLE_VERSION_V1,
            "fddd_context_map": {
                "context_id": "domain:family_lookup",
                "label": "Family lookup",
                "scopes": [{
                    "kind": "relation",
                    "schema": "S",
                    "name": "Parent"
                }]
            },
            "implementation_surfaces": {
                "surfaces": [{
                    "surface_id": "endpoint:family_lookup",
                    "kind": "endpoint",
                    "label": "GET /family/lookup",
                    "ontology_refs": [{
                        "kind": "relation",
                        "schema": "S",
                        "name": "Parent"
                    }],
                    "code_refs": ["src/family.rs"]
                }]
            },
            "codegen_plan": {
                "languages": ["rust", "typescript"],
                "test_name": "family_lookup"
            }
        });

        let overlay_body = serde_json::to_vec(&json!({
            "axi_text": axi_text,
            "overlay": overlay
        }))
        .expect("serialize overlay-check request");
        let overlay_resp = handle_semantic_overlay_check(&overlay_body)
            .await
            .expect("semantic/overlay-check endpoint should succeed");
        assert_eq!(
            overlay_resp["version"].as_str(),
            Some("axiograph_semantic_overlay_check_v1")
        );
        assert_eq!(overlay_resp["report"]["valid"].as_bool(), Some(true));

        let codegen_body = serde_json::to_vec(&json!({ "overlay": overlay }))
            .expect("serialize codegen-plan request");
        let codegen_resp = handle_semantic_codegen_plan(&codegen_body)
            .await
            .expect("semantic/codegen-plan endpoint should succeed");
        assert_eq!(
            codegen_resp["version"].as_str(),
            Some("axiograph_semantic_codegen_plan_v1")
        );
        assert_eq!(
            codegen_resp["report"]["version"].as_str(),
            Some(axiograph_tooling_overlays::CODEGEN_PLAN_REPORT_VERSION_V1)
        );
    }

    #[tokio::test]
    async fn handle_semantic_theory_check_returns_closure_report() {
        let body = serde_json::to_vec(&json!({
            "axi_text": r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

theory TRules on S:
  constraint functional Parent.child -> Parent.parent
"#,
            "theory": "TRules",
            "closure_tier": "finite_fragment"
        }))
        .expect("serialize semantic theory check request");

        let resp = handle_semantic_theory_check(&body)
            .await
            .expect("semantic/theory-check endpoint should succeed");
        assert_eq!(
            resp["version"].as_str(),
            Some("axiograph_semantic_theory_check_v1")
        );
        assert_eq!(
            resp["report"]["reports"][0]["version"].as_str(),
            Some(axiograph_pathdb::RUNTIME_THEORY_CHECK_REPORT_VERSION_V1)
        );
        assert_eq!(resp["report"]["blocking_errors"].as_u64(), Some(0));
        assert_eq!(
            resp["report"]["reports"][0]["closure"]["complete"].as_bool(),
            Some(true)
        );
        assert_eq!(
            resp["report"]["reports"][0]["closure"]["steps"][0]["kind"].as_str(),
            Some("checked_seed")
        );
    }
}
