//! Read-only HTTP query service for authenticated SQLite `.axpd` materializations.
//!
//! The service never accepts a bare SQLite file. Startup requires an AxiStore
//! directory and a `MaterializationIdV2`; the receipt and exact image are
//! validated before PathDB hydration or listener publication.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper::header::CONTENT_TYPE;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

use axiograph_kernel::MaterializationIdV2;
use axiograph_pathdb::axi_semantics::MetaPlaneIndex;
use axiograph_pathdb::materialization::{load_verified_pathdb, MaterializedPathDb};
use axiograph_pathdb::{AcceptedAxiAnchor, AcceptedSnapshotId, AxiDigest, PathDB};
use axiograph_store::{AxpdLimits, AxpdReceipt};

const MAX_QUERY_BODY_BYTES: usize = 1024 * 1024;
const MAX_QUERY_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const MAX_DB_CONNECTIONS: usize = 128;
const MAX_DB_CONNECTION_TIMEOUT_SECS: u64 = 300;
const MAX_DB_CONCURRENT_QUERIES: usize = 16;
const MAX_PATH_LRU_CAPACITY: usize = 1_000_000;
const MAX_PATH_LRU_QUEUE: usize = 65_536;

type HttpResponse = Response<Full<Bytes>>;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FiniteQueryRequest {
    query: crate::query_ir::QueryIrV1,
}

#[derive(Debug, Serialize)]
struct FiniteQueryResponse {
    family: &'static str,
    result: crate::axql::AxqlResult,
    trust: crate::trust_contract::QueryTrustContractV1,
    non_claims: [&'static str; 2],
}

#[derive(Debug, Serialize)]
struct StatusResponseV2<'a> {
    format: &'static str,
    loaded_at_unix_secs: u64,
    entities: usize,
    relations: usize,
    receipt: &'a AxpdReceipt,
}

pub(crate) struct ReadOnlySemanticRuntime {
    pub(crate) db: Arc<PathDB>,
    pub(crate) meta: Option<MetaPlaneIndex>,
    pub(crate) accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub(crate) accepted_axi_anchor: Option<AcceptedAxiAnchor>,
    pub(crate) accepted_axi_text: Option<String>,
}

pub(crate) fn load_read_only_semantic_runtime(
    store_root: &std::path::Path,
    materialization: &str,
) -> Result<ReadOnlySemanticRuntime> {
    let materialization_id: MaterializationIdV2 = materialization
        .parse()
        .map_err(|error| anyhow!("invalid materialization id `{materialization}`: {error}"))?;
    let materialized =
        load_verified_pathdb(store_root, &materialization_id, &AxpdLimits::default())?;
    let accepted_snapshot_id = Some(AcceptedSnapshotId::new(
        materialized
            .receipt()
            .anchors
            .accepted_snapshot_id
            .to_string(),
    ));
    let db = Arc::new(materialized.into_db());
    let meta = MetaPlaneIndex::from_db(&db).ok();
    Ok(ReadOnlySemanticRuntime {
        db,
        meta,
        accepted_snapshot_id,
        accepted_axi_anchor: None,
        accepted_axi_text: None,
    })
}

pub(crate) fn export_canonical_module_axi(_db: &PathDB) -> Result<(AxiDigest, String)> {
    Err(anyhow!(
        "canonical `.axi` bytes cannot be reconstructed from an `.axpd` materialization"
    ))
}

struct ServerState {
    db: Arc<PathDB>,
    meta: Option<MetaPlaneIndex>,
    receipt: AxpdReceipt,
    loaded_at_unix_secs: u64,
    query_permits: Arc<Semaphore>,
    viz_html: Option<Vec<u8>>,
}

pub(crate) fn cmd_db_serve(args: crate::DbServeArgs) -> Result<()> {
    validate_server_args(&args)?;
    let store_root = &args.dir;
    let materialization_id: MaterializationIdV2 =
        args.materialization.parse().map_err(|error| {
            anyhow!(
                "invalid materialization id `{}`: {error}",
                args.materialization
            )
        })?;

    let limits = AxpdLimits::default();
    let mut materialized = load_verified_pathdb(store_root, &materialization_id, &limits)
        .context("load authenticated PathDB materialization")?;
    configure_runtime_indexes(&mut materialized, &args);
    let receipt = materialized.receipt().clone();
    let db = Arc::new(materialized.into_db());
    let meta = MetaPlaneIndex::from_db(&db).ok();
    // Authentication failures above are fatal; optional UI preparation is not.
    let viz_html = match crate::viz::prepare_server_html(&db) {
        Ok(html) => Some(html),
        Err(error) => {
            eprintln!("read-only /viz unavailable: {error}");
            None
        }
    };
    let state = Arc::new(ServerState {
        viz_html,
        db,
        meta,
        receipt,
        loaded_at_unix_secs: now_unix_secs(),
        query_permits: Arc::new(Semaphore::new(MAX_DB_CONCURRENT_QUERIES)),
    });

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .max_blocking_threads(MAX_DB_CONCURRENT_QUERIES)
        .enable_all()
        .build()
        .context("create DB server runtime")?;
    runtime.block_on(run_server(args, state))
}

fn validate_server_args(args: &crate::DbServeArgs) -> Result<()> {
    for (name, actual, maximum) in [
        (
            "--max-connections",
            args.max_connections,
            MAX_DB_CONNECTIONS,
        ),
        (
            "--path-index-lru-capacity",
            args.path_index_lru_capacity,
            MAX_PATH_LRU_CAPACITY,
        ),
        (
            "--path-index-lru-queue",
            args.path_index_lru_queue,
            MAX_PATH_LRU_QUEUE,
        ),
    ] {
        if actual > maximum {
            return Err(anyhow!("{name} exceeds hard maximum {maximum}"));
        }
    }
    if args.max_connections == 0 {
        return Err(anyhow!("--max-connections must be positive"));
    }
    if args.connection_timeout_secs == 0
        || args.connection_timeout_secs > MAX_DB_CONNECTION_TIMEOUT_SECS
    {
        return Err(anyhow!(
            "--connection-timeout-secs must be in 1..={MAX_DB_CONNECTION_TIMEOUT_SECS}"
        ));
    }
    if args.path_index_lru_async && args.path_index_lru_queue == 0 {
        return Err(anyhow!(
            "--path-index-lru-queue must be positive when async LRU updates are enabled"
        ));
    }
    Ok(())
}

fn configure_runtime_indexes(materialized: &mut MaterializedPathDb, args: &crate::DbServeArgs) {
    let async_queue_size = args
        .path_index_lru_async
        .then_some(args.path_index_lru_queue);
    materialized.configure_path_index_cache(args.path_index_lru_capacity, async_queue_size);
}

async fn run_server(args: crate::DbServeArgs, state: Arc<ServerState>) -> Result<()> {
    let listener = TcpListener::bind(args.listen)
        .await
        .with_context(|| format!("bind {}", args.listen))?;
    let address = listener.local_addr()?;
    if let Some(path) = args.ready_file.as_ref() {
        let payload = serde_json::json!({
            "format": "axiograph_db_server_ready_v2",
            "listen": address.to_string(),
            "materialization_id": state.receipt.materialization_id,
            "exact_image_digest": state.receipt.exact_image_digest,
        });
        crate::security::write_output_bounded(path, serde_json::to_vec(&payload)?, "CLI output")?;
    }
    println!("axiograph db server listening on {address}");

    let permits = Arc::new(Semaphore::new(args.max_connections));
    let timeout = Duration::from_secs(args.connection_timeout_secs);
    loop {
        let (stream, _) = listener.accept().await?;
        let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
            drop(stream);
            continue;
        };
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            let service = service_fn(move |request| {
                let state = Arc::clone(&state);
                async move { Ok::<_, Infallible>(handle_request(request, state).await) }
            });
            let mut builder = http1::Builder::new();
            builder.max_headers(64).max_buf_size(64 * 1024);
            let connection = builder.serve_connection(TokioIo::new(stream), service);
            let _ = tokio::time::timeout(timeout, connection).await;
            drop(permit);
        });
    }
}

async fn handle_request(request: Request<Incoming>, state: Arc<ServerState>) -> HttpResponse {
    match (request.method(), request.uri().path()) {
        (&Method::GET, "/healthz") => text_response(StatusCode::OK, "ok\n"),
        (&Method::GET, "/capabilities") => {
            match read_only_capabilities(state.viz_html.is_some()) {
                Ok(capabilities) => json_response(StatusCode::OK, &capabilities),
                Err(error) => error_response(StatusCode::INTERNAL_SERVER_ERROR, error),
            }
        }
        (&Method::GET, "/viz") => match &state.viz_html {
            Some(html) => response(StatusCode::OK, "text/html; charset=utf-8", html.clone()),
            None => error_response(StatusCode::SERVICE_UNAVAILABLE,
                "Visualization unavailable: build frontend assets and use an image within UI-only budgets; health/status/query remain available."),
        },
        (&Method::GET, "/status") => {
            let status = StatusResponseV2 {
                format: "axiograph_authenticated_pathdb_status_v2",
                loaded_at_unix_secs: state.loaded_at_unix_secs,
                entities: state.db.entities.len(),
                relations: state.db.relations.len(),
                receipt: &state.receipt,
            };
            json_response(StatusCode::OK, &status)
        }
        (&Method::POST, "/query") => match collect_query(request).await {
            Ok(query) => {
                let Ok(permit) = Arc::clone(&state.query_permits).try_acquire_owned() else {
                    return error_response(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "concurrent query limit reached",
                    );
                };
                let state = Arc::clone(&state);
                match tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    execute_finite_query(&state.db, state.meta.as_ref(), query.query)
                })
                .await
                {
                    Ok(Ok(response)) => json_response(StatusCode::OK, &response),
                    Ok(Err(error)) => error_response(StatusCode::BAD_REQUEST, error.to_string()),
                    Err(error) => error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("query worker failed: {error}"),
                    ),
                }
            }
            Err(error) => error_response(StatusCode::BAD_REQUEST, error.to_string()),
        },
        _ => error_response(StatusCode::NOT_FOUND, "not found"),
    }
}

// Shared, mechanically checked transport profile; no receipt constructors or
// semantic authority are described here. Query schema remains descriptive.
fn read_only_capabilities(ui_available: bool) -> Result<serde_json::Value> {
    Ok(serde_json::json!({
        "api": serde_json::from_str::<serde_json::Value>(include_str!(
            "../../../../frontend/viz/src/server/read-only-api.json"
        ))?,
        "query_schema": crate::query_ir::query_ir_v1_json_schema(),
        "ui_available": ui_available,
    }))
}

fn execute_finite_query(
    db: &PathDB,
    meta: Option<&MetaPlaneIndex>,
    query: crate::query_ir::QueryIrV1,
) -> Result<FiniteQueryResponse> {
    let mut compiled = query.compile_with_meta(db, meta)?;
    let result = compiled.execute(db, meta)?;
    Ok(FiniteQueryResponse {
        family: "compiled_finite_query",
        result,
        trust: compiled.trust_contract_with_meta(meta),
        non_claims: [
            "no_certificate_without_exact_accepted_axi_bytes",
            "no_ontology_closure_claim",
        ],
    })
}

async fn collect_query(request: Request<Incoming>) -> Result<FiniteQueryRequest> {
    let body = Limited::new(request.into_body(), MAX_QUERY_BODY_BYTES)
        .collect()
        .await
        .map_err(|error| anyhow!("invalid or oversized request body: {error}"))?
        .to_bytes();
    crate::security::parse_json_bounded(&body, MAX_QUERY_BODY_BYTES, "finite query request")
}

fn json_response<T: Serialize>(status: StatusCode, value: &T) -> HttpResponse {
    match serde_json::to_vec(value) {
        Ok(bytes) if bytes.len() <= MAX_QUERY_RESPONSE_BYTES => {
            response(status, "application/json", bytes)
        }
        Ok(bytes) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(
                "response exceeds {MAX_QUERY_RESPONSE_BYTES} bytes (actual {})",
                bytes.len()
            ),
        ),
        Err(error) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("response serialization failed: {error}"),
        ),
    }
}

fn error_response(status: StatusCode, message: impl ToString) -> HttpResponse {
    let value = serde_json::json!({"error": message.to_string()});
    json_response(status, &value)
}

fn text_response(status: StatusCode, text: &str) -> HttpResponse {
    response(
        status,
        "text/plain; charset=utf-8",
        text.as_bytes().to_vec(),
    )
}

fn response(status: StatusCode, content_type: &'static str, body: Vec<u8>) -> HttpResponse {
    let mut response = Response::new(Full::new(Bytes::from(body)));
    *response.status_mut() = status;
    if let Ok(content_type) = content_type.parse() {
        response.headers_mut().insert(CONTENT_TYPE, content_type);
    }
    response
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finite_db() -> Result<(PathDB, MetaPlaneIndex)> {
        let axi = r#"module Demo
schema S:
  object Person
instance I of S:
  Person = {Alice, Bob}
"#;
        let mut db = PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();
        let meta = MetaPlaneIndex::from_db(&db)?;
        Ok((db, meta))
    }

    #[test]
    fn read_only_descriptor_and_descriptive_query_profile_are_checked() -> Result<()> {
        let caps = read_only_capabilities(false)?;
        assert_eq!(caps["api"]["limits"]["request_bytes"], MAX_QUERY_BODY_BYTES);
        assert_eq!(
            caps["api"]["limits"]["response_bytes"],
            MAX_QUERY_RESPONSE_BYTES
        );
        assert_eq!(caps["ui_available"], false);
        let schema = &caps["query_schema"];
        let validator = jsonschema::validator_for(schema)?;
        for term in [
            serde_json::json!({"kind":"var","name":"person"}),
            serde_json::json!({"kind":"name","value":"Alice"}),
            serde_json::json!({"kind":"entity","key":"name","value":"Alice"}),
            serde_json::json!({"kind":"wildcard"}),
        ] {
            let mut query = serde_json::json!({"version":1,"where_atoms":[{"kind":"type","term":term,"type":"Person"}]});
            assert!(validator.is_valid(&query), "{query}");
            assert!(serde_json::from_value::<crate::query_ir::QueryIrV1>(query.clone()).is_ok());
            query["where_atoms"][0]["term"]["extra"] = true.into();
            assert!(!validator.is_valid(&query));
        }
        for context in [
            serde_json::json!({"kind":"name","name":"world"}),
            serde_json::json!({"kind":"entity_id","id":0}),
        ] {
            let mut query = serde_json::json!({"version":1,"where_atoms":[],"contexts":[context]});
            assert!(validator.is_valid(&query));
            assert!(serde_json::from_value::<crate::query_ir::QueryIrV1>(query.clone()).is_ok());
            query["contexts"][0]["extra"] = true.into();
            assert!(!validator.is_valid(&query));
        }
        for query in [
            serde_json::json!({"where_atoms":[]}),
            serde_json::json!({"version":1,"where_atoms":[],"extra":true}),
        ] {
            // Intentionally NOT claiming schema/deserializer equivalence.
            assert!(!validator.is_valid(&query));
            assert!(serde_json::from_value::<crate::query_ir::QueryIrV1>(query).is_ok());
        }
        for request in [
            serde_json::json!({"query":"select ?x where ..."}),
            serde_json::json!({"query":{"version":1,"where_atoms":[]},"certify":true}),
        ] {
            assert!(serde_json::from_value::<FiniteQueryRequest>(request).is_err());
        }
        Ok(())
    }

    #[test]
    fn http_query_runs_only_through_compiled_finite_query() -> Result<()> {
        let (db, meta) = finite_db()?;
        let query = serde_json::from_value(serde_json::json!({
            "version": 1,
            "select_vars": ["person"],
            "where_atoms": [{"kind":"type","term":"?person","type":"Person"}],
            "limit": 10
        }))?;
        let response = execute_finite_query(&db, Some(&meta), query)?;
        assert_eq!(response.family, "compiled_finite_query");
        assert_eq!(response.result.rows.len(), 2);
        assert_eq!(response.trust.trust_class, "certifiable");
        assert_eq!(
            response.non_claims,
            [
                "no_certificate_without_exact_accepted_axi_bytes",
                "no_ontology_closure_claim"
            ]
        );
        Ok(())
    }

    #[test]
    fn http_approximate_query_is_explicitly_execution_only() -> Result<()> {
        let (db, meta) = finite_db()?;
        let query = serde_json::from_value(serde_json::json!({
            "version": 1,
            "select_vars": ["person"],
            "where_atoms": [{
                "kind":"attr_contains",
                "term":"?person",
                "key":"name",
                "needle":"ali"
            }],
            "limit": 10
        }))?;
        let response = execute_finite_query(&db, Some(&meta), query)?;
        assert_eq!(response.trust.trust_class, "execution_only");
        assert_eq!(response.trust.completeness_claim, "not_claimed");
        Ok(())
    }
}
