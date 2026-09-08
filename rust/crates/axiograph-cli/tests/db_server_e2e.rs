use axiograph_kernel::ObjectBlobIdV2;
use axiograph_store::*;
use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use tempfile::tempdir;
#[path = "support/db_server_fixture.rs"]
mod fixture;
use fixture::*;

#[test]
fn bare_axpd_flag_is_not_a_cli_surface() {
    let output = Command::new(axiograph_bin())
        .args(["db", "serve", "--axpd", "bare.axpd"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unexpected argument '--axpd'"), "{stderr}");
}

#[test]
fn cli_publishes_and_inspects_only_for_an_accepted_axi_store_manifest() {
    let store = tempdir().unwrap();
    init_store(store.path(), "cli-publish");
    let spec_path = store.path().join("build-spec.json");
    fs::write(
        &spec_path,
        serde_json::to_vec(&materialization_spec("cli-publish")).unwrap(),
    )
    .unwrap();
    let published = Command::new(axiograph_bin())
        .args([
            "db",
            "materialize",
            "--dir",
            store.path().to_str().unwrap(),
            "--spec",
            spec_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        published.status.success(),
        "{}",
        String::from_utf8_lossy(&published.stderr)
    );
    let receipt: AxpdReceipt = serde_json::from_slice(&published.stdout).unwrap();
    let inspected = Command::new(axiograph_bin())
        .args([
            "db",
            "materialization-show",
            "--dir",
            store.path().to_str().unwrap(),
            "--materialization",
            receipt.materialization_id.as_str(),
        ])
        .output()
        .unwrap();
    assert!(
        inspected.status.success(),
        "{}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    let inspected_receipt: AxpdReceipt = serde_json::from_slice(&inspected.stdout).unwrap();
    assert_eq!(inspected_receipt, receipt);
}

#[test]
fn invalid_or_tampered_materialization_rejects_before_listener_publication() {
    let store = tempdir().unwrap();
    let ready = store.path().join("ready.json");
    let invalid = Command::new(axiograph_bin())
        .args([
            "db",
            "serve",
            "--dir",
            store.path().to_str().unwrap(),
            "--materialization",
            "not-a-materialization-id",
            "--listen",
            "127.0.0.1:0",
            "--ready-file",
            ready.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(!ready.exists());
    let axi_store = init_store(store.path(), "tampered-server");
    let receipt = axi_store
        .publish_axpd(
            materialization_spec("tampered-server"),
            &AxpdLimits::default(),
        )
        .unwrap();
    let receipt_path = axi_store.axpd_receipt_path(&receipt.materialization_id);
    let mut json: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).unwrap()).unwrap();
    json["logical_digest"] = serde_json::Value::String(
        ObjectBlobIdV2::from_canonical_fields(&[b"tampered"]).to_string(),
    );
    fs::write(&receipt_path, serde_json::to_vec(&json).unwrap()).unwrap();
    let tampered = Command::new(axiograph_bin())
        .args([
            "db",
            "serve",
            "--dir",
            store.path().to_str().unwrap(),
            "--materialization",
            receipt.materialization_id.as_str(),
            "--listen",
            "127.0.0.1:0",
            "--ready-file",
            ready.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!tampered.status.success());
    assert!(!ready.exists());
}

#[test]
fn server_publishes_only_after_authenticated_open() {
    let store = tempdir().unwrap();
    let axi_store = init_store(store.path(), "server-open");
    let receipt = axi_store
        .publish_axpd(materialization_spec("server-open"), &AxpdLimits::default())
        .unwrap();
    let ready = store.path().join("ready.json");
    let mut child = Command::new(axiograph_bin())
        .args([
            "db",
            "serve",
            "--dir",
            store.path().to_str().unwrap(),
            "--materialization",
            receipt.materialization_id.as_str(),
            "--listen",
            "127.0.0.1:0",
            "--ready-file",
            ready.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let payload = wait_for_ready(&ready, &mut child);
    assert_eq!(payload["format"], "axiograph_db_server_ready_v2");
    assert_eq!(
        payload["materialization_id"],
        receipt.materialization_id.to_string()
    );
    let listen = payload["listen"].as_str().unwrap();
    let health = reqwest::blocking::get(format!("http://{listen}/healthz")).unwrap();
    assert!(health.status().is_success());
    let status: serde_json::Value = reqwest::blocking::get(format!("http://{listen}/status"))
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(status["format"], "axiograph_authenticated_pathdb_status_v2");
    assert_eq!(
        status["receipt"]["materialization_id"],
        receipt.materialization_id.to_string()
    );
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn mcp_requires_and_opens_an_exact_materialization_id() {
    let store = tempdir().unwrap();
    let axi_store = init_store(store.path(), "mcp-open");
    let receipt = axi_store
        .publish_axpd(materialization_spec("mcp-open"), &AxpdLimits::default())
        .unwrap();
    let invalid = Command::new(axiograph_bin())
        .args([
            "mcp",
            "--dir",
            store.path().to_str().unwrap(),
            "--materialization",
            "invalid",
        ])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    let mut child = Command::new(axiograph_bin())
        .args([
            "mcp",
            "--dir",
            store.path().to_str().unwrap(),
            "--materialization",
            receipt.materialization_id.as_str(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdin = child.stdin.take().unwrap();
    thread::sleep(Duration::from_millis(250));
    assert!(child.try_wait().unwrap().is_none());
    drop(stdin);
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn read_only_http_contract_with_controlled_assets_needs_no_frontend_tools() {
    for (oversized_ui, missing_assets) in [(false, false), (true, false), (false, true)] {
        let store = tempdir().unwrap();
        let axi_store = init_store(store.path(), "finite-client");
        let receipt = axi_store
            .publish_axpd(
                finite_client_spec("finite-client", oversized_ui),
                &AxpdLimits::default(),
            )
            .unwrap();
        let ready = store.path().join("ready.json");
        let asset_root = tempdir().unwrap();
        let dist = asset_root.path().join("frontend/viz/dist");
        fs::create_dir_all(&dist).unwrap();
        fs::write(
            dist.join("index.html"),
            "<head><script type=\"module\" src=\"./fixture.js\"></script></head><body></body>",
        )
        .unwrap();
        if !missing_assets {
            fs::write(
                dist.join("fixture.js"),
                "console.log('test template, not production frontend');",
            )
            .unwrap();
        }
        let mut child = ServerChild(
            Command::new(axiograph_bin())
                .current_dir(asset_root.path())
                // This default Rust test must not need or invoke frontend tools.
                .env("PATH", asset_root.path())
                .args([
                    "db",
                    "serve",
                    "--dir",
                    store.path().to_str().unwrap(),
                    "--materialization",
                    receipt.materialization_id.as_str(),
                    "--listen",
                    "127.0.0.1:0",
                    "--ready-file",
                    ready.to_str().unwrap(),
                ])
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let payload = wait_for_ready(&ready, &mut child.0);
        let base = format!("http://{}", payload["listen"].as_str().unwrap());
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        let caps: serde_json::Value = http
            .get(format!("{base}/capabilities"))
            .send()
            .unwrap()
            .json()
            .unwrap();
        assert_eq!(caps["api"]["format"], "axiograph_read_only_api_v1");
        assert_eq!(caps["ui_available"], !oversized_ui && !missing_assets);
        let status: serde_json::Value = http
            .get(format!("{base}/status"))
            .send()
            .unwrap()
            .json()
            .unwrap();
        assert_eq!(status["entities"], 2);
        assert_eq!(
            status["receipt"]["materialization_id"],
            receipt.materialization_id.to_string()
        );
        let page = http.get(format!("{base}/viz")).send().unwrap();
        assert_eq!(
            page.status().as_u16(),
            if oversized_ui || missing_assets {
                503
            } else {
                200
            }
        );
        if !oversized_ui && !missing_assets {
            let html = page.text().unwrap();
            assert!(html.contains("axiograph_graph"));
            assert!(html.contains("test template"));
        }
        let valid = serde_json::json!({"version":1,"select_vars":["entity"],"where_atoms":[{"kind":"type","term":"?entity","type":"{\"kind\":\"object_type\",\"id\":\"fixture\"}"}],"limit":1});
        let validator = jsonschema::validator_for(&caps["query_schema"]).unwrap();
        assert!(validator.is_valid(&valid));
        let reply = http
            .post(format!("{base}/query"))
            .json(&serde_json::json!({"query":valid}))
            .send()
            .unwrap();
        assert!(reply.status().is_success());
        let result: serde_json::Value = reply.json().unwrap();
        assert_eq!(result["family"], "compiled_finite_query");
        assert_eq!(result["result"]["rows"].as_array().unwrap().len(), 1);
        assert_eq!(result["result"]["truncated"], true);
        assert_eq!(result["trust"]["completeness_claim"], "not_claimed");
        assert_eq!(
            result["non_claims"],
            serde_json::json!([
                "no_certificate_without_exact_accepted_axi_bytes",
                "no_ontology_closure_claim"
            ])
        );
        let approx = serde_json::json!({"query":{"version":1,"where_atoms":[{"kind":"attr_contains","term":"?entity","key":"axiograph.value","needle":if oversized_ui {"xx"} else {"o"}}],"limit":10}});
        let result: serde_json::Value = http
            .post(format!("{base}/query"))
            .json(&approx)
            .send()
            .unwrap()
            .json()
            .unwrap();
        assert!(!result["result"]["rows"].as_array().unwrap().is_empty());
        assert_eq!(result["trust"]["trust_class"], "execution_only");
        for body in [
            "{".to_string(),
            serde_json::json!({"query":"select ?x where ..."}).to_string(),
            serde_json::json!({"query":valid,"certify":true}).to_string(),
            serde_json::json!({"query":{"version":999,"where_atoms":[]}}).to_string(),
        ] {
            assert_eq!(
                http.post(format!("{base}/query"))
                    .body(body)
                    .send()
                    .unwrap()
                    .status()
                    .as_u16(),
                400
            );
        }
        for path in [
            "/snapshots",
            "/contexts",
            "/llm/agent",
            "/entity/describe",
            "/discover/draft-axi",
            "/cert/reachability",
            "/assets/fixture.js",
        ] {
            assert_eq!(
                http.get(format!("{base}{path}"))
                    .send()
                    .unwrap()
                    .status()
                    .as_u16(),
                404
            );
        }
        assert_eq!(
            http.get(format!("{base}/healthz"))
                .send()
                .unwrap()
                .text()
                .unwrap(),
            "ok\n"
        );
    }
}
