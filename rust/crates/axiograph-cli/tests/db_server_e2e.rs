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
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .ok();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .ok();

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
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .ok();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .ok();

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
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .ok();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .ok();

    let request = format!(
        "GET {path_and_query} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
    );
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

fn http_get_text(addr: &str, path_and_query: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .ok();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .ok();

    let request = format!(
        "GET {path_and_query} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
    );
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
    (status, body_text.to_string())
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
    let ready_json: serde_json::Value = serde_json::from_str(&ready_text).expect("parse ready json");
    let addr = ready_json["addr"]
        .as_str()
        .expect("ready.addr is string");

    let query = serde_json::json!({
        "query": "select ?gc where name(\"Alice\") -Grandparent-> ?gc limit 10",
        "lang": "axql",
        "show_elaboration": true,
    });

    let (status_code, response) = http_post_json(addr, "/query", &query);
    assert_eq!(status_code, 200, "expected 200, got {status_code}: {response}");

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
        "query": "select ?gc where name(\"Alice\") -Grandparent-> ?gc limit 10",
        "lang": "axql",
        "certify": true,
        "verify": false,
        "include_anchor": false,
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

    let (anchor_status, anchor_text) = http_get_text(addr, "/anchor.axi");
    assert_eq!(
        anchor_status, 200,
        "expected 200 for /anchor.axi, got {anchor_status}: {anchor_text}"
    );
    assert!(
        anchor_text.contains("module"),
        "expected /anchor.axi to look like an axi module"
    );

    let (viz_status, viz_json) = http_get_json(
        addr,
        "/viz.json?focus_name=Alice&hops=2&max_nodes=200&plane=both&typed_overlay=true",
    );
    assert_eq!(viz_status, 200, "expected 200, got {viz_status}: {viz_json}");
    assert!(
        viz_json["nodes"].as_array().map(|a| !a.is_empty()).unwrap_or(false),
        "expected non-empty viz nodes"
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
        propb_json["chunks"].as_array().map(|a| a.len()).unwrap_or(0),
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
    let ready_json: serde_json::Value = serde_json::from_str(&ready_text).expect("parse ready json");
    let addr = ready_json["addr"]
        .as_str()
        .expect("ready.addr is string");

    let (status_code, status_json) = http_get_json(addr, "/status");
    assert_eq!(status_code, 200, "expected 200, got {status_code}: {status_json}");
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
        to_query_json.get("axql").and_then(|v| v.as_str()).is_some(),
        "expected llm/to_query to return axql: {to_query_json}"
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
    let ready_json: serde_json::Value = serde_json::from_str(&ready_text).expect("parse ready json");
    let addr = ready_json["addr"]
        .as_str()
        .expect("ready.addr is string");

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
            "query": "select ?p where name(\"Jamison\") -Parent-> ?p limit 10",
            "lang": "axql"
        }),
    );
    assert_eq!(q_status, 200, "expected 200, got {q_status}: {q_json}");
    let rows = q_json["rows"].as_array().cloned().unwrap_or_default();
    assert!(
        !rows.is_empty(),
        "expected at least one Parent edge from Jamison after auto-commit: {q_json}"
    );
}
