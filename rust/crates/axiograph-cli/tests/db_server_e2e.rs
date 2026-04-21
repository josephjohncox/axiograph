use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("canonicalize repo root")
}

fn axiograph_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_axiograph"))
}

fn unique_run_dir(repo_root: &Path, label: &str) -> PathBuf {
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let dir = repo_root
        .join("rust/target/tmp/axiograph_db_server_e2e")
        .join(format!("{label}_{pid}_{nanos}"));
    fs::create_dir_all(dir.join("build")).expect("create run dir build/");
    dir
}

struct ChildGuard {
    child: Child,
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn http_post_json(addr: &str, path: &str, body: &serde_json::Value) -> (u16, serde_json::Value) {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();

    let body_bytes = serde_json::to_vec(body).expect("serialize request");
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );

    stream.write_all(request.as_bytes()).expect("write request");
    stream.write_all(&body_bytes).expect("write body");
    stream.flush().ok();

    let mut response_bytes = Vec::new();
    stream
        .read_to_end(&mut response_bytes)
        .expect("read response");
    let response = String::from_utf8_lossy(&response_bytes);

    let mut lines = response.lines();
    let status_line = lines.next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);

    let (_, body_text) = response
        .split_once("\r\n\r\n")
        .unwrap_or(("", response.as_ref()));
    let json: serde_json::Value = serde_json::from_str(body_text).expect("parse JSON response");
    (status, json)
}

fn http_post_json_auth(
    addr: &str,
    path: &str,
    body: &serde_json::Value,
    auth_token: Option<&str>,
) -> (u16, serde_json::Value) {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();

    let body_bytes = serde_json::to_vec(body).expect("serialize request");
    let mut request = format!(
        "POST {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        body_bytes.len()
    );
    if let Some(tok) = auth_token {
        request.push_str(&format!("Authorization: Bearer {tok}\r\n"));
    }
    request.push_str("\r\n");

    stream.write_all(request.as_bytes()).expect("write request");
    stream.write_all(&body_bytes).expect("write body");
    stream.flush().ok();

    let mut response_bytes = Vec::new();
    stream
        .read_to_end(&mut response_bytes)
        .expect("read response");
    let response = String::from_utf8_lossy(&response_bytes);

    let mut lines = response.lines();
    let status_line = lines.next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);

    let (_, body_text) = response
        .split_once("\r\n\r\n")
        .unwrap_or(("", response.as_ref()));
    let json: serde_json::Value = serde_json::from_str(body_text).expect("parse JSON response");
    (status, json)
}

fn http_get_json(addr: &str, path_and_query: &str) -> (u16, serde_json::Value) {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();

    let request =
        format!("GET {path_and_query} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).expect("write request");
    stream.flush().ok();

    let mut response_bytes = Vec::new();
    stream
        .read_to_end(&mut response_bytes)
        .expect("read response");
    let response = String::from_utf8_lossy(&response_bytes);

    let mut lines = response.lines();
    let status_line = lines.next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);

    let (_, body_text) = response
        .split_once("\r\n\r\n")
        .unwrap_or(("", response.as_ref()));
    let json: serde_json::Value = serde_json::from_str(body_text).expect("parse JSON response");
    (status, json)
}

fn wait_for_ready_addr(ready_file: &Path) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !ready_file.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(ready_file.exists(), "server did not write ready file");

    let ready_text = fs::read_to_string(ready_file).expect("read ready file");
    let ready_json: serde_json::Value =
        serde_json::from_str(&ready_text).expect("parse ready json");
    ready_json["addr"]
        .as_str()
        .expect("ready.addr is string")
        .to_string()
}

fn snapshot_id_filename(id: &str) -> String {
    id.replace(':', "_")
}

fn init_store_backed_pathdb_head(bin: &Path, run_dir: &Path, input: &Path) -> PathBuf {
    let accepted_dir = run_dir.join("build/accepted_plane");

    let status = Command::new(bin)
        .current_dir(run_dir)
        .arg("db")
        .arg("accept")
        .arg("promote")
        .arg(input)
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--message")
        .arg("e2e: accept promote (world_model)")
        .status()
        .expect("run axiograph db accept promote");
    assert!(
        status.success(),
        "accept promote failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let chunks_path = run_dir.join("build/init_chunks.json");
    let init_chunks = serde_json::json!([{
        "chunk_id": "init_chunk_0",
        "document_id": "init",
        "page": null,
        "span_id": "span0",
        "text": "init wal snapshot",
        "bbox": null,
        "metadata": {}
    }]);
    fs::write(
        &chunks_path,
        serde_json::to_string_pretty(&init_chunks).expect("serialize init chunks"),
    )
    .expect("write init_chunks.json");

    let status = Command::new(bin)
        .current_dir(run_dir)
        .arg("db")
        .arg("accept")
        .arg("pathdb-commit")
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--accepted-snapshot")
        .arg("latest")
        .arg("--chunks")
        .arg(&chunks_path)
        .arg("--message")
        .arg("e2e: init wal head (world_model)")
        .status()
        .expect("run axiograph db accept pathdb-commit");
    assert!(
        status.success(),
        "accept pathdb-commit failed (exit={})",
        status.code().unwrap_or(-1)
    );

    accepted_dir
}

fn write_world_model_plugin_script(
    run_dir: &Path,
    label: &str,
    response: &serde_json::Value,
) -> PathBuf {
    let response_path = run_dir
        .join("build")
        .join(format!("{label}_plugin_response.json"));
    fs::write(
        &response_path,
        serde_json::to_string(response).expect("serialize plugin response"),
    )
    .expect("write plugin response");

    let script_path = run_dir.join("build").join(format!("{label}_plugin.sh"));
    let script = format!("cat >/dev/null\ncat \"{}\"\n", response_path.display());
    fs::write(&script_path, script).expect("write plugin script");
    script_path
}

fn export_module_digest(bin: &Path, run_dir: &Path, checkpoint: &Path, label: &str) -> String {
    let exported = run_dir
        .join("build")
        .join(format!("{label}_world_model_input.axi"));
    let status = Command::new(bin)
        .current_dir(run_dir)
        .arg("db")
        .arg("pathdb")
        .arg("export-module")
        .arg(checkpoint)
        .arg("--out")
        .arg(&exported)
        .status()
        .expect("export canonical module from checkpoint");
    assert!(
        status.success(),
        "db pathdb export-module failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let axi_text = fs::read_to_string(&exported).expect("read exported module");
    axiograph_dsl::digest::axi_digest_v1(&axi_text).to_string()
}

#[test]
fn db_serve_admin_accept_promote_reloads_head_and_persists_constraints_cert() {
    let repo_root = repo_root();
    let bin = axiograph_bin();
    let run_dir = unique_run_dir(&repo_root, "db_serve_admin_accept_promote");
    let accepted_dir = run_dir.join("build/accepted_plane");
    fs::create_dir_all(&accepted_dir).expect("create accepted dir");

    let base_axi = run_dir.join("build/AdminBase.axi");
    fs::write(
        &base_axi,
        r#"module AdminBase

schema AdminBase:
  object Seed

instance AdminBaseInst of AdminBase:
  Seed = {seed0}
"#,
    )
    .expect("write base module");

    let promote_base = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("promote")
        .arg(&base_axi)
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--message")
        .arg("e2e: base accepted snapshot")
        .status()
        .expect("run base accept promote");
    assert!(
        promote_base.success(),
        "base accept promote failed (exit={})",
        promote_base.code().unwrap_or(-1)
    );

    let promoted_text =
        fs::read_to_string(repo_root.join("examples/ontology/OntologyRewrites.axi"))
            .expect("read promoted axi");
    let promoted_module =
        axiograph_dsl::axi_v1::parse_axi_v1(&promoted_text).expect("parse promoted axi");
    let promoted_module_name = promoted_module.module_name.clone();
    let expected_module_digest = axiograph_dsl::digest::axi_digest_v1(&promoted_text).to_string();

    let ready_file = run_dir.join("build/ready.json");
    let token = "e2e_accept_promote_admin_token";
    let child = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("serve")
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--layer")
        .arg("accepted")
        .arg("--snapshot")
        .arg("head")
        .arg("--listen")
        .arg("127.0.0.1:0")
        .arg("--ready-file")
        .arg(&ready_file)
        .arg("--role")
        .arg("master")
        .arg("--admin-token")
        .arg(token)
        .spawn()
        .expect("spawn db serve (accepted admin promote)");
    let _guard = ChildGuard { child };

    let addr = wait_for_ready_addr(&ready_file);

    let (status_before, json_before) = http_get_json(&addr, "/status");
    assert_eq!(
        status_before, 200,
        "expected 200, got {status_before}: {json_before}"
    );
    let previous_snapshot_id = json_before
        .pointer("/snapshot/accepted_snapshot_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    assert!(
        !previous_snapshot_id.is_empty(),
        "expected accepted snapshot id in /status: {json_before}"
    );

    let (unauth_status, unauth_json) = http_post_json(
        &addr,
        "/admin/accept/promote",
        &serde_json::json!({
            "axi_text": promoted_text,
            "message": "e2e: promote via admin",
            "quality": "off"
        }),
    );
    assert_eq!(
        unauth_status, 401,
        "expected 401, got {unauth_status}: {unauth_json}"
    );
    assert!(
        unauth_json["error"]
            .as_str()
            .is_some_and(|s| s.contains("missing Authorization: Bearer <token>")),
        "expected missing auth error: {unauth_json}"
    );

    let (promote_status, promote_json) = http_post_json_auth(
        &addr,
        "/admin/accept/promote",
        &serde_json::json!({
            "axi_text": fs::read_to_string(repo_root.join("examples/ontology/OntologyRewrites.axi"))
                .expect("read promoted axi for request"),
            "message": "e2e: promote via admin",
            "quality": "off"
        }),
        Some(token),
    );
    assert_eq!(
        promote_status, 200,
        "expected 200, got {promote_status}: {promote_json}"
    );
    let promoted_snapshot_id = promote_json["snapshot_id"]
        .as_str()
        .unwrap_or("")
        .to_string();
    assert!(
        !promoted_snapshot_id.is_empty() && promoted_snapshot_id != previous_snapshot_id,
        "expected promote to advance accepted head: {promote_json}"
    );

    let (status_after, json_after) = http_get_json(&addr, "/status");
    assert_eq!(
        status_after, 200,
        "expected 200, got {status_after}: {json_after}"
    );
    assert_eq!(
        json_after.pointer("/snapshot/accepted_snapshot_id"),
        Some(&serde_json::Value::String(promoted_snapshot_id.clone())),
        "expected accepted/head reload after promote: {json_after}"
    );

    let (snapshots_status, snapshots_json) =
        http_get_json(&addr, "/snapshots?layer=accepted&limit=1");
    assert_eq!(
        snapshots_status, 200,
        "expected 200, got {snapshots_status}: {snapshots_json}"
    );
    let newest = snapshots_json["snapshots"]
        .as_array()
        .and_then(|rows| rows.first())
        .cloned()
        .expect("expected latest accepted snapshot entry");
    assert_eq!(
        newest["snapshot_id"].as_str(),
        Some(promoted_snapshot_id.as_str())
    );
    assert_eq!(newest["message"].as_str(), Some("e2e: promote via admin"));
    assert_eq!(newest["modules_count"].as_u64(), Some(2));

    let snapshot_manifest = accepted_dir.join("snapshots").join(format!(
        "{}.json",
        snapshot_id_filename(&promoted_snapshot_id)
    ));
    assert!(
        snapshot_manifest.exists(),
        "expected accepted snapshot manifest at {}",
        snapshot_manifest.display()
    );
    let snapshot_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&snapshot_manifest).expect("read snapshot manifest"),
    )
    .expect("parse snapshot manifest");
    let module_entry = &snapshot_json["modules"][&promoted_module_name];
    assert_eq!(
        module_entry["module_digest"].as_str(),
        Some(expected_module_digest.as_str())
    );
    let stored_module_rel = module_entry["stored_path"]
        .as_str()
        .expect("expected stored module path");
    assert!(
        accepted_dir.join(stored_module_rel).exists(),
        "expected stored accepted module at {}",
        accepted_dir.join(stored_module_rel).display()
    );

    let log_path = accepted_dir.join("accepted_plane.log.jsonl");
    let last_event_line = fs::read_to_string(&log_path)
        .expect("read accepted plane log")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .last()
        .expect("expected at least one accepted-plane event")
        .to_string();
    let event_json: serde_json::Value =
        serde_json::from_str(&last_event_line).expect("parse accepted-plane event");
    assert_eq!(
        event_json["snapshot_id"].as_str(),
        Some(promoted_snapshot_id.as_str())
    );
    assert_eq!(
        event_json["module_digest"].as_str(),
        Some(expected_module_digest.as_str())
    );
    assert_eq!(
        event_json["stored_module_path"].as_str(),
        Some(stored_module_rel)
    );
    let cert_rel = event_json["constraints_cert_path"]
        .as_str()
        .expect("expected constraints_cert_path on event");
    assert!(
        event_json["constraints_constraint_count"]
            .as_u64()
            .is_some_and(|n| n > 0),
        "expected constraint count on event: {event_json}"
    );
    assert!(
        event_json["constraints_instance_count"]
            .as_u64()
            .is_some_and(|n| n > 0),
        "expected instance count on event: {event_json}"
    );
    assert!(
        event_json["constraints_check_count"]
            .as_u64()
            .is_some_and(|n| n > 0),
        "expected check count on event: {event_json}"
    );

    let cert_path = accepted_dir.join(cert_rel);
    assert!(
        cert_path.exists(),
        "expected stored constraints cert at {}",
        cert_path.display()
    );
    let cert: axiograph_pathdb::certificate::CertificateV2 =
        serde_json::from_str(&fs::read_to_string(&cert_path).expect("read constraints cert"))
            .expect("parse constraints cert");
    let anchor = cert.anchor.expect("expected certificate anchor");
    assert_eq!(
        anchor.axi_digest_v1.as_str(),
        expected_module_digest.as_str()
    );
    match cert.payload {
        axiograph_pathdb::certificate::CertificatePayloadV2::AxiConstraintsOkV1 { proof } => {
            assert_eq!(proof.module_name, promoted_module_name);
            assert_eq!(
                u64::from(proof.constraint_count),
                event_json["constraints_constraint_count"]
                    .as_u64()
                    .expect("event constraint count")
            );
            assert_eq!(
                u64::from(proof.instance_count),
                event_json["constraints_instance_count"]
                    .as_u64()
                    .expect("event instance count")
            );
            assert_eq!(
                u64::from(proof.check_count),
                event_json["constraints_check_count"]
                    .as_u64()
                    .expect("event check count")
            );
        }
        other => panic!("expected axi_constraints_ok_v1 certificate, got {other:?}"),
    }
}

#[test]
fn db_serve_query_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();
    let run_dir = unique_run_dir(&repo_root, "db_serve_query");

    let axpd = run_dir.join("build/server.axpd");
    let input = repo_root.join("examples/ontology/OntologyRewrites.axi");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("pathdb")
        .arg("import-axi")
        .arg(&input)
        .arg("--out")
        .arg(&axpd)
        .status()
        .expect("import .axi into .axpd");
    assert!(
        status.success(),
        "db pathdb import-axi failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let ready_file = run_dir.join("build/ready.json");
    let child = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("serve")
        .arg("--axpd")
        .arg(&axpd)
        .arg("--listen")
        .arg("127.0.0.1:0")
        .arg("--ready-file")
        .arg(&ready_file)
        .spawn()
        .expect("spawn db serve");
    let _guard = ChildGuard { child };

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !ready_file.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(ready_file.exists(), "server did not write ready file");

    let ready_text = fs::read_to_string(&ready_file).expect("read ready file");
    let ready_json: serde_json::Value =
        serde_json::from_str(&ready_text).expect("parse ready json");
    let addr = ready_json["addr"].as_str().expect("ready.addr is string");

    let query = serde_json::json!({
        "lang": "query_ir_v1",
        "query_ir_v1": {
            "version": 1,
            "select": ["?gc"],
            "where": [
                {"kind": "edge", "left": "Alice", "path": "Grandparent", "right": "?gc"}
            ],
            "limit": 10
        },
        "show_elaboration": true,
    });

    let (status_code, response) = http_post_json(addr, "/query", &query);
    assert_eq!(
        status_code, 200,
        "expected 200, got {status_code}: {response}"
    );

    let vars = response["vars"].as_array().cloned().unwrap_or_default();
    assert!(
        vars.iter().any(|v| v.as_str() == Some("?gc")),
        "expected vars to include ?gc: {vars:?}"
    );

    let rows = response["rows"].as_array().cloned().unwrap_or_default();
    assert!(!rows.is_empty(), "expected at least one row");

    assert!(
        response.get("elaborated_query").is_some(),
        "expected elaborated_query when show_elaboration=true"
    );
    assert!(
        response.get("inferred_types").is_some(),
        "expected inferred_types when show_elaboration=true"
    );

    let query_cert = serde_json::json!({
        "lang": "query_ir_v1",
        "query_ir_v1": {
            "version": 1,
            "select": ["?gc"],
            "where": [
                {"kind": "edge", "left": "Alice", "path": "Grandparent", "right": "?gc"}
            ],
            "limit": 10
        },
        "certify": true,
        "verify": false
    });
    let (cert_status, cert_resp) = http_post_json(addr, "/query", &query_cert);
    assert_eq!(
        cert_status, 200,
        "expected 200 for certified query, got {cert_status}: {cert_resp}"
    );
    assert!(
        cert_resp.get("certificate").is_some(),
        "expected certificate in /query response when certify=true: {cert_resp}"
    );
    assert!(
        cert_resp.get("anchor_digest").is_some(),
        "expected anchor_digest in /query response when certify=true: {cert_resp}"
    );
    let anchor_digest_from_query = cert_resp["anchor_digest"].as_str().unwrap_or("");
    assert!(
        !anchor_digest_from_query.is_empty(),
        "expected non-empty canonical anchor digest in /query response: {cert_resp}"
    );

    let (viz_status, viz_json) = http_get_json(
        addr,
        "/viz.json?focus_name=Alice&hops=2&max_nodes=200&plane=both&typed_overlay=true",
    );
    assert_eq!(
        viz_status, 200,
        "expected 200, got {viz_status}: {viz_json}"
    );
    assert!(
        viz_json["nodes"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false),
        "expected non-empty viz nodes"
    );
    let has_typed_overlay = viz_json["nodes"]
        .as_array()
        .map(|nodes| {
            nodes.iter().any(|node| {
                node.get("attrs")
                    .and_then(|v| v.as_object())
                    .is_some_and(|attrs| {
                        attrs.contains_key("axi_overlay_supertypes")
                            || attrs.contains_key("axi_overlay_relation_signature")
                            || attrs.contains_key("axi_overlay_constraints")
                    })
            })
        })
        .unwrap_or(false);
    assert!(
        has_typed_overlay,
        "expected typed overlay attrs in /viz.json response: {viz_json}"
    );

    let alice_id = viz_json["nodes"]
        .as_array()
        .and_then(|nodes| {
            nodes.iter().find_map(|node| {
                let name = node.get("name").and_then(|v| v.as_str());
                if name == Some("Alice") {
                    node.get("id").and_then(|v| v.as_u64()).map(|id| id as u32)
                } else {
                    None
                }
            })
        })
        .expect("expected Alice node id in viz graph");
    let alice_parent_rel = viz_json["edges"]
        .as_array()
        .and_then(|edges| {
            edges.iter().find_map(|edge| {
                let source = edge
                    .get("source")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                let label = edge.get("label").and_then(|v| v.as_str());
                let rel_id = edge
                    .get("relation_id")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                if source == Some(alice_id) && label == Some("Parent") {
                    rel_id
                } else {
                    None
                }
            })
        })
        .expect("expected relation-backed Parent edge from Alice in viz graph");

    let (reach_status, reach_json) = http_post_json(
        addr,
        "/cert/reachability",
        &serde_json::json!({
            "start": alice_id,
            "relation_ids": [alice_parent_rel],
            "verify": false
        }),
    );
    assert_eq!(
        reach_status, 200,
        "expected 200 for /cert/reachability, got {reach_status}: {reach_json}"
    );
    assert!(
        reach_json.get("certificate").is_some(),
        "expected certificate in /cert/reachability response: {reach_json}"
    );
    let reach_anchor_digest = reach_json["anchor_digest"].as_str().unwrap_or("");
    assert!(
        !reach_anchor_digest.is_empty(),
        "expected anchor_digest in /cert/reachability response: {reach_json}"
    );
    assert_eq!(
        reach_json["certificate"]["kind"].as_str(),
        Some("reachability_v3"),
        "expected canonical reachability_v3 certificate payload: {reach_json}"
    );
    assert_eq!(
        reach_json["certificate"]["proof"]["type"].as_str(),
        Some("step"),
        "expected non-trivial reachability_v3 proof chain: {reach_json}"
    );

    let (prop_status, prop_json) = http_post_json(
        addr,
        "/proposals/relation",
        &serde_json::json!({
            "rel_type": "Parent",
            "source_name": "Alice",
            "target_name": "Bob",
            "context": "FamilyTree",
            "confidence": 0.8,
            "evidence_text": "Alice is Bob's parent."
        }),
    );
    assert_eq!(
        prop_status, 200,
        "expected 200 for /proposals/relation, got {prop_status}: {prop_json}"
    );
    assert!(
        prop_json.get("proposals_json").is_some(),
        "expected proposals_json in /proposals/relation response: {prop_json}"
    );

    let (propb_status, propb_json) = http_post_json(
        addr,
        "/proposals/relations",
        &serde_json::json!({
            "rel_type": "Parent",
            "source_names": ["Alice", "Carol"],
            "target_names": ["Bob"],
            "pairing": "cartesian",
            "context": "FamilyTree",
            "confidence": 0.8,
            "evidence_text": "Batch evidence for parent relations."
        }),
    );
    assert_eq!(
        propb_status, 200,
        "expected 200 for /proposals/relations, got {propb_status}: {propb_json}"
    );
    assert!(
        propb_json.get("proposals_json").is_some(),
        "expected proposals_json in /proposals/relations response: {propb_json}"
    );
    assert_eq!(
        propb_json["chunks"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0),
        1,
        "expected one shared evidence chunk in /proposals/relations response: {propb_json}"
    );
    let chunk_id = propb_json["chunks"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|c| c.get("chunk_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        !chunk_id.is_empty(),
        "expected chunks[0].chunk_id in /proposals/relations response: {propb_json}"
    );
    let proposals_len = propb_json["proposals_json"]["proposals"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    assert!(
        proposals_len >= 2,
        "expected >=2 proposals in /proposals/relations response, got {proposals_len}: {propb_json}"
    );
    if let Some(arr) = propb_json["proposals_json"]["proposals"].as_array() {
        for p in arr {
            let ev0 = p
                .get("evidence")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|e| e.get("chunk_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            assert_eq!(
                ev0, chunk_id,
                "expected every proposal to include shared evidence chunk_id={chunk_id}, got {ev0}: {propb_json}"
            );
        }
    }

    // Snapshot listing is only available in store-backed mode, but the endpoint
    // should exist (and return a structured error) even when serving a raw `.axpd`.
    let (snap_status, snap_json) = http_get_json(addr, "/snapshots");
    assert_eq!(
        snap_status, 400,
        "expected 400 for /snapshots in axpd mode, got {snap_status}: {snap_json}"
    );
    assert!(
        snap_json.get("error").is_some(),
        "expected /snapshots error payload"
    );
}

#[test]
fn db_serve_llm_agent_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();
    let run_dir = unique_run_dir(&repo_root, "db_serve_llm_agent");

    let axpd = run_dir.join("build/server.axpd");
    let input = repo_root.join("examples/ontology/OntologyRewrites.axi");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("pathdb")
        .arg("import-axi")
        .arg(&input)
        .arg("--out")
        .arg(&axpd)
        .status()
        .expect("import .axi into .axpd");
    assert!(
        status.success(),
        "db pathdb import-axi failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let ready_file = run_dir.join("build/ready.json");
    let child = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("serve")
        .arg("--axpd")
        .arg(&axpd)
        .arg("--listen")
        .arg("127.0.0.1:0")
        .arg("--ready-file")
        .arg(&ready_file)
        .arg("--llm-mock")
        .spawn()
        .expect("spawn db serve");
    let _guard = ChildGuard { child };

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !ready_file.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(ready_file.exists(), "server did not write ready file");

    let ready_text = fs::read_to_string(&ready_file).expect("read ready file");
    let ready_json: serde_json::Value =
        serde_json::from_str(&ready_text).expect("parse ready json");
    let addr = ready_json["addr"].as_str().expect("ready.addr is string");

    let (status_code, status_json) = http_get_json(addr, "/status");
    assert_eq!(
        status_code, 200,
        "expected 200, got {status_code}: {status_json}"
    );
    assert!(
        status_json
            .get("llm")
            .and_then(|v| v.get("enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "expected llm.enabled=true in /status: {status_json}"
    );

    let (to_query_status, to_query_json) = http_post_json(
        addr,
        "/llm/to_query",
        &serde_json::json!({ "question": "find Person named Alice" }),
    );
    assert_eq!(
        to_query_status, 200,
        "expected 200, got {to_query_status}: {to_query_json}"
    );
    assert!(
        to_query_json
            .get("query_ir_v1")
            .and_then(|v| v.as_object())
            .is_some(),
        "expected llm/to_query to return query_ir_v1: {to_query_json}"
    );
    assert!(
        to_query_json.get("axql").is_none(),
        "llm/to_query should no longer return raw axql fallback: {to_query_json}"
    );

    let (agent_status, agent_json) = http_post_json(
        addr,
        "/llm/agent",
        &serde_json::json!({
            "question": "find Person named Alice",
            "max_steps": 3,
            "max_rows": 5
        }),
    );
    assert_eq!(
        agent_status, 200,
        "expected 200, got {agent_status}: {agent_json}"
    );
    assert!(
        agent_json
            .pointer("/outcome/final_answer/answer")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.trim().is_empty()),
        "expected llm/agent outcome.final_answer.answer: {agent_json}"
    );
    assert!(
        agent_json
            .pointer("/outcome/steps")
            .and_then(|v| v.as_array())
            .is_some_and(|a| !a.is_empty()),
        "expected llm/agent outcome.steps non-empty: {agent_json}"
    );
}

#[test]
fn db_serve_llm_agent_auto_commit_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();
    let run_dir = unique_run_dir(&repo_root, "db_serve_llm_auto_commit");

    let accepted_dir = run_dir.join("build/accepted_plane");
    let input = repo_root.join("examples/ontology/OntologyRewrites.axi");

    // 1) Anchor accepted plane.
    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("promote")
        .arg(&input)
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--message")
        .arg("e2e: accept promote (llm auto-commit)")
        .status()
        .expect("run axiograph db accept promote");
    assert!(
        status.success(),
        "accept promote failed (exit={})",
        status.code().unwrap_or(-1)
    );

    // 2) Create the initial PathDB WAL HEAD snapshot so `--layer pathdb --snapshot head` can load.
    let chunks_path = run_dir.join("build/init_chunks.json");
    let init_chunks = serde_json::json!([{
        "chunk_id": "init_chunk_0",
        "document_id": "init",
        "page": null,
        "span_id": "span0",
        "text": "init wal snapshot",
        "bbox": null,
        "metadata": {}
    }]);
    fs::write(
        &chunks_path,
        serde_json::to_string_pretty(&init_chunks).expect("serialize init chunks"),
    )
    .expect("write init_chunks.json");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("pathdb-commit")
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--accepted-snapshot")
        .arg("latest")
        .arg("--chunks")
        .arg(&chunks_path)
        .arg("--message")
        .arg("e2e: init wal head")
        .status()
        .expect("run axiograph db accept pathdb-commit");
    assert!(
        status.success(),
        "accept pathdb-commit failed (exit={})",
        status.code().unwrap_or(-1)
    );

    // 3) Start store-backed server in master mode with the mock LLM backend enabled.
    let ready_file = run_dir.join("build/ready.json");
    let token = "e2e_admin_token";
    let child = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("serve")
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--layer")
        .arg("pathdb")
        .arg("--snapshot")
        .arg("head")
        .arg("--listen")
        .arg("127.0.0.1:0")
        .arg("--ready-file")
        .arg(&ready_file)
        .arg("--role")
        .arg("master")
        .arg("--admin-token")
        .arg(token)
        .arg("--llm-mock")
        .spawn()
        .expect("spawn db serve (store-backed)");
    let _guard = ChildGuard { child };

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !ready_file.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(ready_file.exists(), "server did not write ready file");

    let ready_text = fs::read_to_string(&ready_file).expect("read ready file");
    let ready_json: serde_json::Value =
        serde_json::from_str(&ready_text).expect("parse ready json");
    let addr = ready_json["addr"].as_str().expect("ready.addr is string");

    // 4) Auto-commit is admin-gated.
    let (unauth_status, unauth_json) = http_post_json(
        addr,
        "/llm/agent",
        &serde_json::json!({
            "question": "add Jamison who is a son of Bob",
            "auto_commit": true,
            "max_steps": 3,
            "max_rows": 5
        }),
    );
    assert_eq!(
        unauth_status, 401,
        "expected 401 for auto_commit without auth, got {unauth_status}: {unauth_json}"
    );

    let (auth_status, auth_json) = http_post_json_auth(
        addr,
        "/llm/agent",
        &serde_json::json!({
            "question": "add Jamison who is a son of Bob",
            "auto_commit": true,
            "max_steps": 3,
            "max_rows": 5
        }),
        Some(token),
    );
    assert_eq!(
        auth_status, 200,
        "expected 200, got {auth_status}: {auth_json}"
    );
    assert!(
        auth_json
            .pointer("/commit/ok")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "expected commit.ok=true: {auth_json}"
    );
    let committed_snapshot = auth_json
        .pointer("/commit/snapshot_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        !committed_snapshot.is_empty(),
        "expected commit.snapshot_id: {auth_json}"
    );

    // 5) Query should observe the new snapshot after auto-commit.
    let (q_status, q_json) = http_post_json(
        addr,
        "/query",
        &serde_json::json!({
            "lang": "query_ir_v1",
            "query_ir_v1": {
                "version": 1,
                "select": ["?p"],
                "where": [
                    {"kind": "edge", "left": "Jamison", "path": "Parent", "right": "?p"}
                ],
                "limit": 10
            }
        }),
    );
    assert_eq!(q_status, 200, "expected 200, got {q_status}: {q_json}");
    let rows = q_json["rows"].as_array().cloned().unwrap_or_default();
    assert!(
        !rows.is_empty(),
        "expected at least one Parent edge from Jamison after auto-commit: {q_json}"
    );
}

#[test]
fn db_serve_llm_agent_require_verified_queries_refuses_without_verifier() {
    let repo_root = repo_root();
    let bin = axiograph_bin();
    let run_dir = unique_run_dir(&repo_root, "db_serve_llm_require_verified_queries");

    let axpd = run_dir.join("build/server.axpd");
    let input = repo_root.join("examples/ontology/OntologyRewrites.axi");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("pathdb")
        .arg("import-axi")
        .arg(&input)
        .arg("--out")
        .arg(&axpd)
        .status()
        .expect("import .axi into .axpd");
    assert!(
        status.success(),
        "db pathdb import-axi failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let ready_file = run_dir.join("build/ready.json");
    let child = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("serve")
        .arg("--axpd")
        .arg(&axpd)
        .arg("--listen")
        .arg("127.0.0.1:0")
        .arg("--ready-file")
        .arg(&ready_file)
        .arg("--llm-mock")
        .spawn()
        .expect("spawn db serve");
    let _guard = ChildGuard { child };

    let addr = wait_for_ready_addr(&ready_file);

    let (agent_status, agent_json) = http_post_json(
        &addr,
        "/llm/agent",
        &serde_json::json!({
            "question": "find Person named Alice",
            "max_steps": 3,
            "max_rows": 5,
            "require_verified_queries": true
        }),
    );
    assert_eq!(
        agent_status, 200,
        "expected 200, got {agent_status}: {agent_json}"
    );
    assert_eq!(
        agent_json.pointer("/gate/ok").and_then(|v| v.as_bool()),
        Some(false),
        "expected gate failure when verifier is unavailable: {agent_json}"
    );
    let failures = agent_json
        .pointer("/gate/failures")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        failures.iter().any(|v| {
            v.as_str()
                .is_some_and(|s| s.contains("Lean verifier not configured"))
        }),
        "expected verifier configuration failure in gate report: {agent_json}"
    );
    assert!(
        agent_json
            .pointer("/outcome/final_answer/answer")
            .and_then(|v| v.as_str())
            .is_some_and(|s| s.contains("Refusing to answer")),
        "expected fail-closed refusal answer: {agent_json}"
    );
    let query_certs = agent_json
        .pointer("/query_certificates")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        query_certs.iter().any(|v| {
            v.get("certificate_verify_error")
                .and_then(|x| x.as_str())
                .is_some_and(|s| s.contains("Lean verifier not configured"))
        }),
        "expected query certificate verify error in transcript: {agent_json}"
    );
}

#[test]
fn db_serve_query_snapshot_override_uses_requested_anchor() {
    let repo_root = repo_root();
    let bin = axiograph_bin();
    let run_dir = unique_run_dir(&repo_root, "db_serve_query_snapshot_override");
    let input = repo_root.join("examples/ontology/OntologyRewrites.axi");
    let accepted_dir = init_store_backed_pathdb_head(&bin, &run_dir, &input);

    let ready_file = run_dir.join("build/ready.json");
    let token = "e2e_snapshot_override_token";
    let child = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("serve")
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--layer")
        .arg("pathdb")
        .arg("--snapshot")
        .arg("head")
        .arg("--listen")
        .arg("127.0.0.1:0")
        .arg("--ready-file")
        .arg(&ready_file)
        .arg("--role")
        .arg("master")
        .arg("--admin-token")
        .arg(token)
        .arg("--llm-mock")
        .spawn()
        .expect("spawn db serve (snapshot override)");
    let _guard = ChildGuard { child };

    let addr = wait_for_ready_addr(&ready_file);

    let (status_before, json_before) = http_get_json(&addr, "/status");
    assert_eq!(
        status_before, 200,
        "expected 200, got {status_before}: {json_before}"
    );
    let old_snapshot_id = json_before
        .pointer("/snapshot/pathdb_snapshot_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    assert!(
        !old_snapshot_id.is_empty(),
        "expected initial pathdb snapshot id in /status: {json_before}"
    );

    let (commit_status, commit_json) = http_post_json_auth(
        &addr,
        "/llm/agent",
        &serde_json::json!({
            "question": "add Jamison who is a son of Bob",
            "auto_commit": true,
            "max_steps": 3,
            "max_rows": 5
        }),
        Some(token),
    );
    assert_eq!(
        commit_status, 200,
        "expected 200, got {commit_status}: {commit_json}"
    );
    let new_snapshot_id = commit_json
        .pointer("/commit/snapshot_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    assert!(
        !new_snapshot_id.is_empty() && new_snapshot_id != old_snapshot_id,
        "expected auto-commit to advance pathdb snapshot head: {commit_json}"
    );

    let (current_q_status, current_q_json) = http_post_json(
        &addr,
        "/query",
        &serde_json::json!({
            "lang": "query_ir_v1",
            "query_ir_v1": {
                "version": 1,
                "select": ["?p"],
                "where": [
                    { "kind": "type", "term": "?p", "type": "Person" }
                ],
                "limit": 20
            },
            "certify": true
        }),
    );
    assert_eq!(
        current_q_status, 200,
        "expected 200, got {current_q_status}: {current_q_json}"
    );
    let current_rows = current_q_json["rows"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !current_rows.is_empty(),
        "expected person rows in current head snapshot: {current_q_json}"
    );
    assert!(
        current_rows.iter().any(|row| {
            row.get("?p")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
                == Some("Jamison")
        }),
        "expected Jamison to appear in current head snapshot: {current_q_json}"
    );
    let current_anchor_digest = current_q_json["anchor_digest"].as_str().unwrap_or("");
    assert!(
        !current_anchor_digest.is_empty(),
        "expected anchor_digest for current snapshot query: {current_q_json}"
    );

    let (old_q_status, old_q_json) = http_post_json(
        &addr,
        "/query",
        &serde_json::json!({
            "lang": "query_ir_v1",
            "query_ir_v1": {
                "version": 1,
                "select": ["?p"],
                "where": [
                    { "kind": "type", "term": "?p", "type": "Person" }
                ],
                "limit": 20
            },
            "certify": true,
            "snapshot": old_snapshot_id
        }),
    );
    assert_eq!(
        old_q_status, 200,
        "expected 200, got {old_q_status}: {old_q_json}"
    );
    let old_rows = old_q_json["rows"].as_array().cloned().unwrap_or_default();
    assert!(
        old_rows.len() < current_rows.len(),
        "expected fewer Person rows in the overridden older snapshot: {old_q_json}"
    );
    assert!(
        old_rows.iter().all(|row| {
            row.get("?p")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
                != Some("Jamison")
        }),
        "expected Jamison to be absent from the overridden older snapshot: {old_q_json}"
    );
    let old_anchor_digest = old_q_json["anchor_digest"].as_str().unwrap_or("");
    assert!(
        !old_anchor_digest.is_empty(),
        "expected anchor_digest for snapshot override query: {old_q_json}"
    );
    assert_eq!(
        current_anchor_digest, old_anchor_digest,
        "expected canonical anchor digest to stay stable across derived PathDB snapshot overrides"
    );
}

#[test]
fn db_serve_world_model_propose_lineage_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();
    let run_dir = unique_run_dir(&repo_root, "db_serve_world_model_propose_lineage");
    let input = repo_root.join("examples/ontology/OntologyRewrites.axi");
    let accepted_dir = init_store_backed_pathdb_head(&bin, &run_dir, &input);

    let plugin_script = write_world_model_plugin_script(
        &run_dir,
        "lineage",
        &serde_json::json!({
            "protocol": "axiograph_world_model_v1",
            "trace_id": "wm::db_server_e2e::lineage",
            "generated_at_unix_secs": 1,
            "proposals": {
                "version": 1,
                "generated_at": "1",
                "source": { "source_type": "plugin", "locator": "plugin" },
                "schema_hint": "OrgFamily",
                "proposals": [{
                    "kind": "Relation",
                    "proposal_id": "wm-lineage-rel",
                    "confidence": 0.77,
                    "evidence": [],
                    "public_rationale": "lineage smoke test",
                    "metadata": { "plugin_marker": "lineage" },
                    "schema_hint": "OrgFamily",
                    "relation_id": "wm::lineage::rel",
                    "rel_type": "Parent",
                    "source": "Alice",
                    "target": "Bob"
                }]
            },
            "notes": ["e2e deterministic plugin"]
        }),
    );

    let ready_file = run_dir.join("build/ready.json");
    let child = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("serve")
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--layer")
        .arg("pathdb")
        .arg("--snapshot")
        .arg("head")
        .arg("--listen")
        .arg("127.0.0.1:0")
        .arg("--ready-file")
        .arg(&ready_file)
        .arg("--world-model-plugin")
        .arg("/bin/sh")
        .arg("--world-model-plugin-arg")
        .arg(&plugin_script)
        .spawn()
        .expect("spawn db serve (world_model lineage)");
    let _guard = ChildGuard { child };

    let addr = wait_for_ready_addr(&ready_file);

    let (status_code, status_json) = http_get_json(&addr, "/status");
    assert_eq!(
        status_code, 200,
        "expected 200, got {status_code}: {status_json}"
    );
    let accepted_snapshot_id = status_json
        .pointer("/snapshot/accepted_snapshot_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let pathdb_snapshot_id = status_json
        .pointer("/snapshot/pathdb_snapshot_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    assert!(
        !accepted_snapshot_id.is_empty(),
        "expected accepted_snapshot_id in /status: {status_json}"
    );
    assert!(
        !pathdb_snapshot_id.is_empty(),
        "expected pathdb_snapshot_id in /status: {status_json}"
    );

    let checkpoint = accepted_dir
        .join("pathdb")
        .join("checkpoints")
        .join(format!(
            "{}.axpd",
            snapshot_id_filename(&pathdb_snapshot_id)
        ));
    assert!(
        checkpoint.exists(),
        "expected checkpoint for loaded pathdb snapshot: {}",
        checkpoint.display()
    );
    let expected_axi_digest = export_module_digest(&bin, &run_dir, &checkpoint, "lineage");

    let (prop_status, prop_json) = http_post_json(
        &addr,
        "/world_model/propose",
        &serde_json::json!({
            "guardrail_profile": "off",
            "include_guardrail": false,
            "max_new_proposals": 1
        }),
    );
    assert_eq!(
        prop_status, 200,
        "expected 200 for /world_model/propose, got {prop_status}: {prop_json}"
    );

    let trace_id = prop_json["trace_id"].as_str().unwrap_or("");
    assert_eq!(
        trace_id, "wm::db_server_e2e::lineage",
        "expected deterministic plugin trace_id: {prop_json}"
    );
    assert_eq!(
        prop_json
            .pointer("/proposals/source/locator")
            .and_then(|v| v.as_str()),
        Some(trace_id),
        "expected proposals.source.locator to track trace_id: {prop_json}"
    );

    let proposal = prop_json
        .pointer("/proposals/proposals/0")
        .expect("first proposal in world_model/propose response");
    let metadata = proposal["metadata"]
        .as_object()
        .expect("proposal.metadata is object");
    assert_eq!(
        metadata.get("plugin_marker").and_then(|v| v.as_str()),
        Some("lineage"),
        "expected plugin metadata to survive provenance stamping: {prop_json}"
    );
    assert_eq!(
        metadata
            .get("axiograph_world_model_trace_id")
            .and_then(|v| v.as_str()),
        Some(trace_id),
        "expected trace lineage in proposal metadata: {prop_json}"
    );
    assert_eq!(
        metadata
            .get("axiograph_world_model_run_id")
            .and_then(|v| v.as_str()),
        Some(trace_id),
        "expected run_id lineage in proposal metadata: {prop_json}"
    );
    assert_eq!(
        metadata
            .get("axiograph_axi_digest_v1")
            .and_then(|v| v.as_str()),
        Some(expected_axi_digest.as_str()),
        "expected canonical axi digest lineage in proposal metadata: {prop_json}"
    );
    assert_eq!(
        metadata
            .get("axiograph_pathdb_snapshot_id")
            .and_then(|v| v.as_str()),
        Some(pathdb_snapshot_id.as_str()),
        "expected pathdb snapshot lineage in proposal metadata: {prop_json}"
    );
    assert_eq!(
        metadata
            .get("axiograph_accepted_snapshot_id")
            .and_then(|v| v.as_str()),
        Some(accepted_snapshot_id.as_str()),
        "expected accepted snapshot lineage in proposal metadata: {prop_json}"
    );
    let proposals_digest = metadata
        .get("axiograph_proposals_digest")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        !proposals_digest.is_empty(),
        "expected proposals digest lineage in proposal metadata: {prop_json}"
    );

    let run_record_path = accepted_dir
        .join("sem")
        .join("world_model_runs")
        .join(format!("{}.json", snapshot_id_filename(trace_id)));
    assert!(
        run_record_path.exists(),
        "expected persisted world-model run record: {}",
        run_record_path.display()
    );
    let run_record: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&run_record_path).expect("read world-model run record"),
    )
    .expect("parse world-model run record");
    assert_eq!(run_record["run_id"].as_str(), Some(trace_id));
    assert_eq!(run_record["trace_id"].as_str(), Some(trace_id));
    assert_eq!(run_record["status"].as_str(), Some("previewed"));
    assert_eq!(run_record["backend"].as_str(), Some("command:/bin/sh"));
    assert_eq!(
        run_record["input_pathdb_snapshot_id"].as_str(),
        Some(pathdb_snapshot_id.as_str())
    );
    assert_eq!(
        run_record["input_accepted_snapshot_id"].as_str(),
        Some(accepted_snapshot_id.as_str())
    );
    assert_eq!(
        run_record["proposals_digest"].as_str(),
        Some(proposals_digest)
    );
    let notes = prop_json["notes"].as_array().expect("notes array");
    assert!(
        notes.iter().any(|note| {
            note.as_str()
                .map(|s| {
                    s == "world_model_run_record=sem/world_model_runs/wm__db_server_e2e__lineage.json"
                })
                .unwrap_or(false)
        }),
        "expected response notes to surface persisted run-record path: {prop_json}"
    );
}

#[test]
fn db_serve_world_model_propose_auto_commit_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();
    let run_dir = unique_run_dir(&repo_root, "db_serve_world_model_propose_auto_commit");
    let input = repo_root.join("examples/ontology/OntologyRewrites.axi");
    let accepted_dir = init_store_backed_pathdb_head(&bin, &run_dir, &input);

    let plugin_script = write_world_model_plugin_script(
        &run_dir,
        "auto_commit",
        &serde_json::json!({
            "protocol": "axiograph_world_model_v1",
            "trace_id": "wm::db_server_e2e::auto_commit",
            "generated_at_unix_secs": 1,
            "proposals": {
                "version": 1,
                "generated_at": "1",
                "source": { "source_type": "plugin", "locator": "plugin" },
                "schema_hint": "OrgFamily",
                "proposals": [{
                    "kind": "Entity",
                    "proposal_id": "wm-auto-entity",
                    "confidence": 0.81,
                    "evidence": [],
                    "public_rationale": "world_model auto_commit smoke",
                    "metadata": { "plugin_marker": "auto_commit" },
                    "schema_hint": "OrgFamily",
                    "entity_id": "wm::jamison",
                    "entity_type": "Person",
                    "name": "Jamison"
                }]
            },
            "notes": ["e2e deterministic plugin"]
        }),
    );

    let ready_file = run_dir.join("build/ready.json");
    let token = "e2e_world_model_admin_token";
    let child = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("serve")
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--layer")
        .arg("pathdb")
        .arg("--snapshot")
        .arg("head")
        .arg("--listen")
        .arg("127.0.0.1:0")
        .arg("--ready-file")
        .arg(&ready_file)
        .arg("--role")
        .arg("master")
        .arg("--admin-token")
        .arg(token)
        .arg("--world-model-plugin")
        .arg("/bin/sh")
        .arg("--world-model-plugin-arg")
        .arg(&plugin_script)
        .spawn()
        .expect("spawn db serve (world_model auto_commit)");
    let _guard = ChildGuard { child };

    let addr = wait_for_ready_addr(&ready_file);

    let (status_code, status_json) = http_get_json(&addr, "/status");
    assert_eq!(
        status_code, 200,
        "expected 200, got {status_code}: {status_json}"
    );
    let accepted_snapshot_id = status_json
        .pointer("/snapshot/accepted_snapshot_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let pathdb_snapshot_id = status_json
        .pointer("/snapshot/pathdb_snapshot_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    assert!(
        !accepted_snapshot_id.is_empty() && !pathdb_snapshot_id.is_empty(),
        "expected store-backed snapshot lineage in /status: {status_json}"
    );

    let checkpoint = accepted_dir
        .join("pathdb")
        .join("checkpoints")
        .join(format!(
            "{}.axpd",
            snapshot_id_filename(&pathdb_snapshot_id)
        ));
    assert!(
        checkpoint.exists(),
        "expected checkpoint for loaded pathdb snapshot: {}",
        checkpoint.display()
    );
    let expected_axi_digest = export_module_digest(&bin, &run_dir, &checkpoint, "auto_commit");

    let (unauth_status, unauth_json) = http_post_json(
        &addr,
        "/world_model/propose",
        &serde_json::json!({
            "auto_commit": true,
            "validate": false,
            "guardrail_profile": "off",
            "include_guardrail": false,
            "commit_message": "e2e: world_model/propose auto_commit"
        }),
    );
    assert_eq!(
        unauth_status, 401,
        "expected 401 for auto_commit without auth, got {unauth_status}: {unauth_json}"
    );

    let (auth_status, auth_json) = http_post_json_auth(
        &addr,
        "/world_model/propose",
        &serde_json::json!({
            "auto_commit": true,
            "validate": false,
            "guardrail_profile": "off",
            "include_guardrail": false,
            "commit_message": "e2e: world_model/propose auto_commit"
        }),
        Some(token),
    );
    assert_eq!(
        auth_status, 200,
        "expected 200 for authorized /world_model/propose, got {auth_status}: {auth_json}"
    );

    let trace_id = auth_json["trace_id"].as_str().unwrap_or("");
    assert_eq!(
        trace_id, "wm::db_server_e2e::auto_commit",
        "expected deterministic plugin trace_id: {auth_json}"
    );
    let proposal = auth_json
        .pointer("/proposals/proposals/0")
        .expect("first proposal in authorized world_model/propose response");
    let metadata = proposal["metadata"]
        .as_object()
        .expect("proposal.metadata is object");
    assert_eq!(
        metadata.get("plugin_marker").and_then(|v| v.as_str()),
        Some("auto_commit"),
        "expected plugin metadata to survive provenance stamping: {auth_json}"
    );
    assert_eq!(
        metadata
            .get("axiograph_world_model_trace_id")
            .and_then(|v| v.as_str()),
        Some(trace_id),
        "expected trace lineage in proposal metadata: {auth_json}"
    );
    assert_eq!(
        metadata
            .get("axiograph_axi_digest_v1")
            .and_then(|v| v.as_str()),
        Some(expected_axi_digest.as_str()),
        "expected canonical axi digest lineage in auto-commit response: {auth_json}"
    );
    assert_eq!(
        metadata
            .get("axiograph_pathdb_snapshot_id")
            .and_then(|v| v.as_str()),
        Some(pathdb_snapshot_id.as_str()),
        "expected pre-commit pathdb snapshot lineage in proposal metadata: {auth_json}"
    );
    assert_eq!(
        metadata
            .get("axiograph_accepted_snapshot_id")
            .and_then(|v| v.as_str()),
        Some(accepted_snapshot_id.as_str()),
        "expected accepted snapshot lineage in proposal metadata: {auth_json}"
    );

    let commit = auth_json
        .get("commit")
        .expect("commit payload for auto_commit response");
    let committed_snapshot_id = commit["snapshot_id"].as_str().unwrap_or("");
    assert!(
        !committed_snapshot_id.is_empty(),
        "expected commit.snapshot_id in /world_model/propose auto_commit response: {auth_json}"
    );
    assert_eq!(
        commit["accepted_snapshot_id"].as_str(),
        Some(accepted_snapshot_id.as_str()),
        "expected commit.accepted_snapshot_id to stay anchored to the loaded accepted snapshot: {auth_json}"
    );
    assert!(
        commit["ops_added"].as_u64().unwrap_or(0) > 0,
        "expected commit.ops_added > 0 in /world_model/propose auto_commit response: {auth_json}"
    );

    let (post_status, post_json) = http_get_json(&addr, "/status");
    assert_eq!(
        post_status, 200,
        "expected 200, got {post_status}: {post_json}"
    );
    assert_eq!(
        post_json
            .pointer("/snapshot/pathdb_snapshot_id")
            .and_then(|v| v.as_str()),
        Some(committed_snapshot_id),
        "expected server to reload committed pathdb snapshot after auto_commit: {post_json}"
    );

    let run_record_path = accepted_dir
        .join("sem")
        .join("world_model_runs")
        .join(format!("{}.json", snapshot_id_filename(trace_id)));
    assert!(
        run_record_path.exists(),
        "expected persisted world-model run record: {}",
        run_record_path.display()
    );
    let run_record: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&run_record_path).expect("read committed world-model run record"),
    )
    .expect("parse committed world-model run record");
    assert_eq!(run_record["run_id"].as_str(), Some(trace_id));
    assert_eq!(run_record["status"].as_str(), Some("committed_to_pathdb"));
    assert_eq!(
        run_record["committed_pathdb_snapshot_id"].as_str(),
        Some(committed_snapshot_id)
    );
    assert_eq!(
        run_record["committed_accepted_snapshot_id"].as_str(),
        Some(accepted_snapshot_id.as_str())
    );
    let notes = auth_json["notes"].as_array().expect("notes array");
    assert!(
        notes.iter().any(|note| {
            note.as_str()
                .map(|s| {
                    s == "world_model_run_record=sem/world_model_runs/wm__db_server_e2e__auto_commit.json"
                })
                .unwrap_or(false)
        }),
        "expected response notes to surface persisted run-record path: {auth_json}"
    );
}
