use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use axiograph_dsl::axi_v1::parse_axi_v1;
use axiograph_ingest_docs::{
    Chunk, EvidencePointer, ProposalMetaV1, ProposalSourceV1, ProposalV1, ProposalsFileV1,
};
use axiograph_pathdb::certificate::{CertificatePayloadV2, CertificateV2};
use walkdir::WalkDir;

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
        .join("rust/target/tmp/axiograph_examples_e2e")
        .join(format!("{label}_{pid}_{nanos}"));
    fs::create_dir_all(dir.join("build")).expect("create run dir build/");
    dir
}

fn scenario_query_axql(scenario: &str) -> String {
    match scenario {
        "enterprise" => "select ?svc where name(\"team_0\") -owns-> ?svc limit 10".to_string(),
        "enterprise_large_api_proto_import" => {
            "select ?psvc where name(\"svc_users\") -mapsToProtoService-> ?psvc limit 10"
                .to_string()
        }
        "continuous_ingest" => {
            "select ?svc where name(\"doc_stream_0\") -mentionsService-> ?svc limit 10".to_string()
        }
        "economic_flows" => {
            "select ?f where name(\"household_0\") -Consumption-> ?f limit 10".to_string()
        }
        "economic_flows_axi" => {
            "select ?to where name(\"Household_A\") -Flow-> ?to limit 10".to_string()
        }
        "machinist_learning" => {
            "select ?g where name(\"op_0\") -guardrailedBy-> ?g limit 10".to_string()
        }
        "schema_evolution" => {
            "select ?s where name(\"ProductV1_0\") -outgoingMigration/toSchema-> ?s limit 10"
                .to_string()
        }
        "schema_evolution_axi" => {
            "select ?dst where name(\"AddCategories\") -MigrationTo-> ?dst limit 10".to_string()
        }
        "proto_api" => {
            "select ?rpc where name(\"doc_proto_api_0\") -mentions_rpc-> ?rpc limit 10".to_string()
        }
        "proto_schema_discovery" => {
            "select ?rpc where name(\"UserService\") -proto_service_has_rpc-> ?rpc limit 10"
                .to_string()
        }
        "fts_demo" => "select ?c where ?c is DocChunk limit 10".to_string(),
        "modalities_axi" => "select ?p where name(\"Alice\") -Knows-> ?p limit 10".to_string(),
        "physics_ontology_axi" => {
            "select ?g where name(\"QFT_QED\") -QFTHasGaugeGroup-> ?g limit 10".to_string()
        }
        "physics_knowledge" => {
            "select ?cat where name(\"NewtonsSecond\") -LawCategory-> ?cat limit 10".to_string()
        }
        "physics_learning" => {
            "select ?g where name(\"RegenerativeChatter\") -explains-> ?g limit 10".to_string()
        }
        "ontology_rewrites_axi" => {
            "select ?gc where name(\"Alice\") -Grandparent-> ?gc limit 10".to_string()
        }
        "social_network" => "select ?x where name(\"Alice_0\") -Friend-> ?x limit 10".to_string(),
        "social_network_axi" => {
            "select ?x where name(\"Alice\") -Relationship-> ?x limit 10".to_string()
        }
        "family_hott" => "select ?p where name(\"Alice\") -Parent-> ?p limit 10".to_string(),
        "supply_chain" => {
            "select ?f where name(\"supplier_0\") -supplies-> ?f limit 10".to_string()
        }
        "supply_chain_hott" => {
            "select ?to where name(\"RawMetal_A\") -Flow-> ?to limit 10".to_string()
        }
        "world_model_mpc" => "select ?p where ?p is Person limit 1".to_string(),
        "world_model_mpc_physics" => "select ?c where ?c is Concept limit 1".to_string(),
        "sql_schema_discovery" => {
            "select ?c where name(\"Users\") -SqlHasColumn-> ?c limit 10".to_string()
        }
        _other => "select ?h where ?h is Homotopy limit 1".to_string(),
    }
}

#[test]
fn validate_all_examples_axi() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let mut axi_files: Vec<PathBuf> = WalkDir::new(repo_root.join("examples"))
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| p.extension().map(|s| s == "axi").unwrap_or(false))
        .collect();
    axi_files.sort();

    assert!(
        !axi_files.is_empty(),
        "expected at least one `.axi` under examples/"
    );

    for path in axi_files {
        let status = Command::new(&bin)
            .current_dir(&repo_root)
            .arg("check")
            .arg("validate")
            .arg(&path)
            .status()
            .expect("run axiograph check validate");

        assert!(
            status.success(),
            "validate failed for `{}` (exit={})",
            path.display(),
            status.code().unwrap_or(-1)
        );
    }
}

#[test]
fn typecheck_cert_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "typecheck_cert");
    let cert_path = run_dir.join("build/typecheck_cert.json");

    let input = repo_root.join("examples/economics/EconomicFlows.axi");
    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("cert")
        .arg("typecheck")
        .arg(&input)
        .arg("--out")
        .arg(&cert_path)
        .status()
        .expect("run axiograph cert typecheck");

    assert!(
        status.success(),
        "typecheck-cert failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let cert_text = fs::read_to_string(&cert_path).expect("read typecheck cert json");
    let cert: CertificateV2 = serde_json::from_str(&cert_text).expect("parse typecheck cert json");

    assert_eq!(cert.version, 2);
    let anchor = cert.anchor.expect("expected anchor");
    assert!(
        anchor.axi_digest_v1.starts_with("fnv1a64:"),
        "unexpected digest format: {}",
        anchor.axi_digest_v1
    );

    match cert.payload {
        CertificatePayloadV2::AxiWellTypedV1 { proof } => {
            assert_eq!(proof.module_name, "EconomicFlows");
            assert!(proof.schema_count >= 1);
        }
        other => panic!("expected axi_well_typed_v1 certificate, got {other:?}"),
    }
}

#[test]
fn constraints_cert_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "constraints_cert");
    let cert_path = run_dir.join("build/constraints_cert.json");

    let input = repo_root.join("examples/ontology/OntologyRewrites.axi");
    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("cert")
        .arg("constraints")
        .arg(&input)
        .arg("--out")
        .arg(&cert_path)
        .status()
        .expect("run axiograph cert constraints");

    assert!(
        status.success(),
        "constraints-cert failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let cert_text = fs::read_to_string(&cert_path).expect("read constraints cert json");
    let cert: CertificateV2 =
        serde_json::from_str(&cert_text).expect("parse constraints cert json");

    assert_eq!(cert.version, 2);
    let anchor = cert.anchor.expect("expected anchor");
    assert!(
        anchor.axi_digest_v1.starts_with("fnv1a64:"),
        "unexpected digest format: {}",
        anchor.axi_digest_v1
    );

    match cert.payload {
        CertificatePayloadV2::AxiConstraintsOkV1 { proof } => {
            assert_eq!(proof.module_name, "OntologyRewrites");
            assert!(proof.constraint_count >= 1);
            assert!(proof.instance_count >= 1);
            assert!(proof.check_count >= 1);
        }
        other => panic!("expected axi_constraints_ok_v1 certificate, got {other:?}"),
    }
}

#[test]
fn pathdb_wal_import_proposals_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "pathdb_wal_proposals");
    let out_dir = run_dir.join("build");
    let accepted_dir = out_dir.join("accepted_plane");
    fs::create_dir_all(&accepted_dir).expect("create accepted dir");

    // ---------------------------------------------------------------------
    // A) Create a tiny accepted-plane base snapshot (canonical meaning plane).
    // ---------------------------------------------------------------------
    let base_axi = out_dir.join("WalBase.axi");
    fs::write(
        &base_axi,
        r#"module WalBase

schema WalBase:
  object Dummy

instance WalBaseInst of WalBase:
  Dummy = {dummy0}
"#,
    )
    .expect("write base module");

    let promote = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("promote")
        .arg(&base_axi)
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--message")
        .arg("test: base snapshot")
        .output()
        .expect("run promote");
    assert!(
        promote.status.success(),
        "promote failed: {}",
        String::from_utf8_lossy(&promote.stderr)
    );
    let accepted_snapshot_id = String::from_utf8_lossy(&promote.stdout).trim().to_string();
    assert!(
        !accepted_snapshot_id.is_empty(),
        "expected promote to print snapshot id"
    );

    // ---------------------------------------------------------------------
    // B) Ingest RDF TriG fixture → proposals.json.
    // ---------------------------------------------------------------------
    let fixture_dir = repo_root.join("examples/rdfowl/named_graphs_minimal");
    let ingest = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("ingest")
        .arg("dir")
        .arg(&fixture_dir)
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--domain")
        .arg("rdfowl")
        .output()
        .expect("run ingest dir");
    assert!(
        ingest.status.success(),
        "ingest failed: {}",
        String::from_utf8_lossy(&ingest.stderr)
    );
    let proposals_path = out_dir.join("proposals.json");
    assert!(proposals_path.exists(), "expected proposals.json");

    // ---------------------------------------------------------------------
    // C) Commit proposals.json into the PathDB WAL and checkout .axpd.
    // ---------------------------------------------------------------------
    let commit = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("pathdb-commit")
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--accepted-snapshot")
        .arg(&accepted_snapshot_id)
        .arg("--proposals")
        .arg(&proposals_path)
        .arg("--message")
        .arg("test: preserve proposals")
        .output()
        .expect("run pathdb-commit");
    assert!(
        commit.status.success(),
        "pathdb-commit failed: {}",
        String::from_utf8_lossy(&commit.stderr)
    );
    let wal_snapshot_id = String::from_utf8_lossy(&commit.stdout).trim().to_string();
    assert!(!wal_snapshot_id.is_empty(), "expected WAL snapshot id");

    let axpd = out_dir.join("evidence_plane.axpd");
    let build = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("pathdb-build")
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--snapshot")
        .arg(&wal_snapshot_id)
        .arg("--out")
        .arg(&axpd)
        .output()
        .expect("run pathdb-build");
    assert!(
        build.status.success(),
        "pathdb-build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    // ---------------------------------------------------------------------
    // D) Validate that evidence-plane data was preserved in PathDB.
    // ---------------------------------------------------------------------
    let bytes = fs::read(&axpd).expect("read axpd");
    let db = axiograph_pathdb::PathDB::from_bytes(&bytes).expect("parse axpd");

    // Resolve resource entities by `name`.
    let name_key = db.interner.id_of("name").expect("interned name key");
    let a_name = db.interner.id_of("a").expect("interned a");
    let b_name = db.interner.id_of("b").expect("interned b");
    let c_name = db.interner.id_of("c").expect("interned c");
    let g_plan_name = db.interner.id_of("g_plan").expect("interned g_plan");
    let g_observed_name = db.interner.id_of("g_observed").expect("interned g_observed");

    let a_id = db
        .entities
        .entities_with_attr_value(name_key, a_name)
        .iter()
        .next()
        .expect("entity named a");
    let b_id = db
        .entities
        .entities_with_attr_value(name_key, b_name)
        .iter()
        .next()
        .expect("entity named b");
    let c_id = db
        .entities
        .entities_with_attr_value(name_key, c_name)
        .iter()
        .next()
        .expect("entity named c");

    let g_plan_id = db
        .entities
        .entities_with_attr_value(name_key, g_plan_name)
        .iter()
        .next()
        .expect("context named g_plan");
    let g_observed_id = db
        .entities
        .entities_with_attr_value(name_key, g_observed_name)
        .iter()
        .next()
        .expect("context named g_observed");

    // Ensure `iri` attribute is preserved for `a`.
    let iri_key = db.interner.id_of("iri").expect("interned iri key");
    let iri_val = db
        .interner
        .id_of("http://example.org/a")
        .expect("interned a iri");
    assert_eq!(
        db.entities.get_attr(a_id, iri_key),
        Some(iri_val),
        "expected entity `a` to preserve iri attribute"
    );

    // Find `knows` fact nodes and confirm they are correctly scoped per context.
    let axi_relation_key = db
        .interner
        .id_of(axiograph_pathdb::axi_meta::ATTR_AXI_RELATION)
        .expect("interned axi_relation key");
    let knows_val = db.interner.id_of("knows").expect("interned knows");
    let knows_facts = db
        .entities
        .entities_with_attr_value(axi_relation_key, knows_val);
    assert!(!knows_facts.is_empty(), "expected knows fact nodes");

    let from_rel = db.interner.id_of("from").expect("interned from");
    let to_rel = db.interner.id_of("to").expect("interned to");
    let in_ctx_rel = db
        .interner
        .id_of(axiograph_pathdb::axi_meta::REL_AXI_FACT_IN_CONTEXT)
        .expect("interned axi_fact_in_context");

    let mut saw_plan = false;
    let mut saw_observed = false;
    for f in knows_facts.iter() {
        let has_from_a = db.relations.has_edge(f, from_rel, a_id);
        if !has_from_a {
            continue;
        }
        let has_ctx_plan = db.relations.has_edge(f, in_ctx_rel, g_plan_id);
        let has_ctx_observed = db.relations.has_edge(f, in_ctx_rel, g_observed_id);

        if has_ctx_plan && db.relations.has_edge(f, to_rel, b_id) {
            saw_plan = true;
        }
        if has_ctx_observed && db.relations.has_edge(f, to_rel, c_id) {
            saw_observed = true;
        }
    }

    assert!(saw_plan, "expected g_plan to assert knows(a,b)");
    assert!(saw_observed, "expected g_observed to assert knows(a,c)");
}

#[test]
fn analyze_network_and_quality_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "analyze_quality");
    let net_path = run_dir.join("build/network.json");
    let quality_path = run_dir.join("build/quality.json");

    let input = repo_root.join("examples/ontology/OntologyRewrites.axi");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("tools")
        .arg("analyze")
        .arg("network")
        .arg(&input)
        .arg("--plane")
        .arg("both")
        .arg("--skip-facts")
        .arg("--communities")
        .arg("--format")
        .arg("json")
        .arg("--out")
        .arg(&net_path)
        .status()
        .expect("run axiograph tools analyze network");
    assert!(
        status.success(),
        "analyze network failed (exit={})",
        status.code().unwrap_or(-1)
    );
    let net_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&net_path).expect("read network report"))
            .expect("parse network report json");
    assert_eq!(net_json["version"], "network_analysis_v1");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("check")
        .arg("quality")
        .arg(&input)
        .arg("--plane")
        .arg("both")
        .arg("--profile")
        .arg("strict")
        .arg("--format")
        .arg("json")
        .arg("--no-fail")
        .arg("--out")
        .arg(&quality_path)
        .status()
        .expect("run axiograph check quality");
    assert!(
        status.success(),
        "quality failed (exit={})",
        status.code().unwrap_or(-1)
    );
    let q_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&quality_path).expect("read quality report"))
            .expect("parse quality report json");
    assert_eq!(q_json["version"], "quality_report_v1");
}

#[test]
fn accepted_plane_promote_and_build_pathdb_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "accepted_plane");
    let accepted_dir = run_dir.join("build/accepted_plane");
    let out_axpd = run_dir.join("build/accepted_plane.axpd");

    let input = repo_root.join("examples/economics/EconomicFlows.axi");
    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("promote")
        .arg(&input)
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--message")
        .arg("e2e smoke: accept promote")
        .status()
        .expect("run axiograph db accept promote");

    assert!(
        status.success(),
        "accept promote failed for `{}` (exit={})",
        input.display(),
        status.code().unwrap_or(-1)
    );

    let snapshot_id = fs::read_to_string(accepted_dir.join("HEAD"))
        .expect("read accepted plane HEAD")
        .trim()
        .to_string();
    assert!(
        !snapshot_id.is_empty(),
        "expected accepted plane HEAD snapshot id"
    );

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("build-pathdb")
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--snapshot")
        .arg("latest")
        .arg("--out")
        .arg(&out_axpd)
        .status()
        .expect("run axiograph db accept build-pathdb");

    assert!(
        status.success(),
        "accept build-pathdb failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let bytes = fs::read(&out_axpd).expect("read rebuilt axpd");
    assert!(bytes.len() > 64, "expected non-empty axpd output");

    let db = axiograph_pathdb::PathDB::from_bytes(&bytes).expect("parse rebuilt axpd");
    assert!(
        !db.entities.is_empty(),
        "expected non-empty PathDB entities after rebuild"
    );

    // Grounding always has evidence: accepted-plane builds embed `.axi` module
    // source as DocChunks so LLM/UI flows can cite and open it.
    let has_chunks = db
        .find_by_type("DocChunk")
        .map(|bm| !bm.is_empty())
        .unwrap_or(false);
    assert!(has_chunks, "expected at least one DocChunk in accepted build");
}

#[test]
fn accepted_plane_pathdb_wal_commit_and_build_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "pathdb_wal");
    let accepted_dir = run_dir.join("build/accepted_plane");
    let out_axpd = run_dir.join("build/pathdb_wal.axpd");

    // 1) Create an accepted-plane snapshot (canonical `.axi` is the anchor).
    let input = repo_root.join("examples/economics/EconomicFlows.axi");
    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("promote")
        .arg(&input)
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--message")
        .arg("e2e smoke: accept promote (pathdb wal)")
        .status()
        .expect("run axiograph db accept promote");
    assert!(
        status.success(),
        "accept promote failed for `{}` (exit={})",
        input.display(),
        status.code().unwrap_or(-1)
    );

    // 2) Commit an extension-layer overlay (chunks.json) into the PathDB WAL.
    let chunks_path = run_dir.join("build/chunks.json");
    let chunks: Vec<Chunk> = vec![Chunk {
        chunk_id: "chunk0".to_string(),
        document_id: "doc0.txt".to_string(),
        page: None,
        span_id: "span0".to_string(),
        text: "EconomicFlows mentions Household_A and Firm_A".to_string(),
        bbox: None,
        metadata: HashMap::new(),
    }];
    fs::write(
        &chunks_path,
        serde_json::to_string_pretty(&chunks).expect("serialize chunks"),
    )
    .expect("write chunks.json");

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
        .arg("e2e smoke: pathdb wal commit")
        .status()
        .expect("run axiograph db accept pathdb-commit");
    assert!(
        status.success(),
        "accept pathdb-commit failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let pathdb_head = fs::read_to_string(accepted_dir.join("pathdb").join("HEAD"))
        .expect("read pathdb wal HEAD")
        .trim()
        .to_string();
    assert!(
        !pathdb_head.is_empty(),
        "expected non-empty pathdb wal HEAD"
    );

    // 3) Check out the `.axpd` from the WAL snapshot.
    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("pathdb-build")
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--snapshot")
        .arg("latest")
        .arg("--out")
        .arg(&out_axpd)
        .status()
        .expect("run axiograph db accept pathdb-build");
    assert!(
        status.success(),
        "accept pathdb-build failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let bytes = fs::read(&out_axpd).expect("read pathdb wal axpd");
    let db = axiograph_pathdb::PathDB::from_bytes(&bytes).expect("parse pathdb wal axpd");

    let chunk_id_key = db
        .interner
        .id_of("chunk_id")
        .expect("chunk_id attr key id");
    let want = db.interner.id_of("chunk0").expect("chunk0 value id");
    let mut found = false;
    if let Some(chunks) = db.find_by_type("DocChunk") {
        for id in chunks.iter() {
            if db.entities.get_attr(id, chunk_id_key) == Some(want) {
                found = true;
                break;
            }
        }
    }
    assert!(found, "expected committed DocChunk chunk_id=chunk0 after wal commit");
}

#[test]
fn accepted_plane_promote_with_quality_report_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "accepted_plane_quality");
    let accepted_dir = run_dir.join("build/accepted_plane");

    let input = repo_root.join("examples/ontology/OntologyRewrites.axi");
    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("promote")
        .arg(&input)
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--quality")
        .arg("strict")
        .arg("--message")
        .arg("e2e smoke: accept promote (quality)")
        .status()
        .expect("run axiograph db accept promote --quality strict");

    assert!(
        status.success(),
        "accept promote --quality strict failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let log_path = accepted_dir.join("accepted_plane.log.jsonl");
    let log = fs::read_to_string(&log_path).expect("read accepted plane log");
    let last_line = log
        .lines()
        .filter(|l| !l.trim().is_empty())
        .last()
        .expect("expected at least one log line");
    let event: serde_json::Value = serde_json::from_str(last_line).expect("parse event json");

    let rel = event["quality_report_path"]
        .as_str()
        .expect("expected quality_report_path on event");
    let report_path = accepted_dir.join(rel);
    assert!(
        report_path.exists(),
        "expected stored quality report at `{}`",
        report_path.display()
    );
}

#[test]
fn repl_scripts_export_and_querycert_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let scripts_root = repo_root.join("examples/repl_scripts");
    let mut scripts: Vec<PathBuf> = WalkDir::new(&scripts_root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| p.extension().map(|s| s == "repl").unwrap_or(false))
        .collect();
    scripts.sort();

    assert!(
        !scripts.is_empty(),
        "expected `.repl` scripts under examples/repl_scripts/"
    );

    for script in scripts {
        let label = script
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "script".to_string());
        let run_dir = unique_run_dir(&repo_root, &label);

        let status = Command::new(&bin)
            .current_dir(&run_dir)
            .arg("repl")
            .arg("--script")
            .arg(&script)
            .arg("--quiet")
            .status()
            .expect("run axiograph repl --script");
        assert!(
            status.success(),
            "repl script `{}` failed (exit={})",
            script.display(),
            status.code().unwrap_or(-1)
        );

        let build_dir = run_dir.join("build");
        let mut exports: Vec<PathBuf> = fs::read_dir(&build_dir)
            .expect("read build dir")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension().map(|s| s == "axi").unwrap_or(false)
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.ends_with("_export_v1.axi"))
                        .unwrap_or(false)
            })
            .collect();
        exports.sort();

        assert_eq!(
            exports.len(),
            1,
            "expected exactly one `*_export_v1.axi` in {}, got: {:?}",
            build_dir.display(),
            exports
        );
        let export_axi = exports[0].clone();

        let stem = export_axi
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let scenario = stem.strip_suffix("_export_v1").unwrap_or(stem);
        let query = scenario_query_axql(scenario);

        let cert_path = build_dir.join(format!("{scenario}_query_cert.json"));
        let status = Command::new(&bin)
            .current_dir(&run_dir)
            .arg("cert")
            .arg("query")
            .arg(&export_axi)
            .arg("--lang")
            .arg("axql")
            .arg(&query)
            .arg("--out")
            .arg(&cert_path)
            .status()
            .expect("run axiograph cert query");
        assert!(
            status.success(),
            "querycert failed for scenario `{scenario}` (exit={})",
            status.code().unwrap_or(-1)
        );

        let cert_text = fs::read_to_string(&cert_path).expect("read query cert json");
        let cert: CertificateV2 = serde_json::from_str(&cert_text).expect("parse query cert json");

        assert_eq!(cert.version, 2);
        let anchor = cert.anchor.expect("expected snapshot anchor");
        assert!(
            anchor.axi_digest_v1.starts_with("fnv1a64:"),
            "unexpected digest format: {}",
            anchor.axi_digest_v1
        );

        match cert.payload {
            CertificatePayloadV2::QueryResultV1 { proof } => {
                assert!(
                    !proof.rows.is_empty(),
                    "expected non-empty query result rows for scenario `{scenario}` (query={query})"
                );
            }
            other => panic!("expected query_result_v1 certificate, got {other:?}"),
        }

        // If this REPL script imported a canonical `.axi` module (meta-plane),
        // we should be able to export it back as a canonical module from the `.axpd`.
        //
        // Scripts ending with `_axi_demo` (and `physics_knowledge_demo`) are the
        // canonical-module demos; the others are purely synthetic scenarios.
        let should_have_meta_plane =
            label.ends_with("_axi_demo") || label == "physics_knowledge_demo";
        if should_have_meta_plane {
            let axpd = build_dir.join(format!("{scenario}.axpd"));
            assert!(
                axpd.exists(),
                "expected `{}` to write `{}`",
                script.display(),
                axpd.display()
            );

            let module_out = build_dir.join(format!("{scenario}_module_export_v1.axi"));
            let status = Command::new(&bin)
                .current_dir(&run_dir)
                .arg("db")
                .arg("pathdb")
                .arg("export-module")
                .arg(&axpd)
                .arg("-o")
                .arg(&module_out)
                .status()
                .expect("run axiograph db pathdb export-module");
            assert!(
                status.success(),
                "pathdb export-module failed for `{}` (exit={})",
                script.display(),
                status.code().unwrap_or(-1)
            );

            let text = fs::read_to_string(&module_out).expect("read exported module .axi");
            let m = parse_axi_v1(&text).expect("parse exported module via axi_v1");
            assert_eq!(
                m.module_name.is_empty(),
                false,
                "expected non-empty module name in exported module"
            );
        }
    }
}

#[test]
fn discover_augment_proposals_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "augment_proposals");
    let in_path = run_dir.join("build/in_proposals.json");
    let out_path = run_dir.join("build/out_proposals.json");
    let trace_path = run_dir.join("build/augment_trace.json");

    let mut mention_attrs = HashMap::new();
    mention_attrs.insert("role".to_string(), "material".to_string());
    mention_attrs.insert("domain".to_string(), "machining".to_string());
    mention_attrs.insert("value".to_string(), "Titanium".to_string());

    let input = ProposalsFileV1 {
        version: 1,
        generated_at: "0".to_string(),
        source: ProposalSourceV1 {
            source_type: "test".to_string(),
            locator: "discover_augment_proposals_smoke".to_string(),
        },
        schema_hint: None,
        proposals: vec![ProposalV1::Entity {
            meta: ProposalMetaV1 {
                proposal_id: "mention::fact0::material".to_string(),
                confidence: 0.9,
                evidence: vec![EvidencePointer {
                    chunk_id: "chunk0".to_string(),
                    locator: None,
                    span_id: None,
                }],
                public_rationale: "test mention".to_string(),
                metadata: HashMap::new(),
                schema_hint: None,
            },
            entity_id: "mention::fact0::material".to_string(),
            entity_type: "Mention".to_string(),
            name: "Titanium".to_string(),
            attributes: mention_attrs,
            description: None,
        }],
    };

    fs::write(
        &in_path,
        serde_json::to_string_pretty(&input).expect("serialize proposals"),
    )
    .expect("write proposals json");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("discover")
        .arg("augment-proposals")
        .arg(&in_path)
        .arg("--out")
        .arg(&out_path)
        .arg("--trace")
        .arg(&trace_path)
        .status()
        .expect("run axiograph discover augment-proposals");

    assert!(
        status.success(),
        "discover augment-proposals failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let out_text = fs::read_to_string(&out_path).expect("read out proposals json");
    let out: ProposalsFileV1 = serde_json::from_str(&out_text).expect("parse out proposals json");

    assert!(
        out.proposals.iter().any(|p| matches!(
            p,
            ProposalV1::Entity { entity_id, entity_type, .. }
                if entity_type == "Role" && entity_id == "role::material"
        )),
        "expected derived Role entity"
    );

    assert!(
        out.proposals.iter().any(|p| matches!(
            p,
            ProposalV1::Relation { rel_type, .. } if rel_type == "HasRole"
        )),
        "expected derived HasRole relation"
    );

    assert!(
        out.proposals.iter().any(|p| matches!(
            p,
            ProposalV1::Entity { meta, entity_id, .. }
                if entity_id == "mention::fact0::material"
                    && meta.schema_hint.as_deref() == Some("machinist_learning")
        )),
        "expected inferred schema_hint on Mention"
    );
}

#[test]
fn discover_draft_module_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "draft_module");
    let proposals_path = run_dir.join("build/proposals.json");
    let out_axi = run_dir.join("build/discovered.proposals.axi");

    let file = ProposalsFileV1 {
        version: 1,
        generated_at: "0".to_string(),
        source: ProposalSourceV1 {
            source_type: "test".to_string(),
            locator: "discover_draft_module_smoke".to_string(),
        },
        schema_hint: Some("sql".to_string()),
        proposals: vec![
            ProposalV1::Entity {
                meta: ProposalMetaV1 {
                    proposal_id: "sql_table::Users".to_string(),
                    confidence: 1.0,
                    evidence: vec![],
                    public_rationale: "test table".to_string(),
                    metadata: HashMap::new(),
                    schema_hint: Some("sql".to_string()),
                },
                entity_id: "sql_table::Users".to_string(),
                entity_type: "SqlTable".to_string(),
                name: "Users".to_string(),
                attributes: HashMap::new(),
                description: None,
            },
            ProposalV1::Entity {
                meta: ProposalMetaV1 {
                    proposal_id: "sql_column::Users::id".to_string(),
                    confidence: 1.0,
                    evidence: vec![],
                    public_rationale: "test column".to_string(),
                    metadata: HashMap::new(),
                    schema_hint: Some("sql".to_string()),
                },
                entity_id: "sql_column::Users::id".to_string(),
                entity_type: "SqlColumn".to_string(),
                name: "Users.id".to_string(),
                attributes: HashMap::new(),
                description: None,
            },
            ProposalV1::Entity {
                meta: ProposalMetaV1 {
                    proposal_id: "sql_column::Users::name".to_string(),
                    confidence: 1.0,
                    evidence: vec![],
                    public_rationale: "test column".to_string(),
                    metadata: HashMap::new(),
                    schema_hint: Some("sql".to_string()),
                },
                entity_id: "sql_column::Users::name".to_string(),
                entity_type: "SqlColumn".to_string(),
                name: "Users.name".to_string(),
                attributes: HashMap::new(),
                description: None,
            },
            // One table has multiple columns: not functional `from -> to`, but functional `to -> from`.
            ProposalV1::Relation {
                meta: ProposalMetaV1 {
                    proposal_id: "sql_rel::has_column::Users::id".to_string(),
                    confidence: 1.0,
                    evidence: vec![],
                    public_rationale: "test has_column".to_string(),
                    metadata: HashMap::new(),
                    schema_hint: Some("sql".to_string()),
                },
                relation_id: "sql_rel::has_column::Users::id".to_string(),
                rel_type: "SqlHasColumn".to_string(),
                source: "sql_table::Users".to_string(),
                target: "sql_column::Users::id".to_string(),
                attributes: HashMap::new(),
            },
            ProposalV1::Relation {
                meta: ProposalMetaV1 {
                    proposal_id: "sql_rel::has_column::Users::name".to_string(),
                    confidence: 1.0,
                    evidence: vec![],
                    public_rationale: "test has_column".to_string(),
                    metadata: HashMap::new(),
                    schema_hint: Some("sql".to_string()),
                },
                relation_id: "sql_rel::has_column::Users::name".to_string(),
                rel_type: "SqlHasColumn".to_string(),
                source: "sql_table::Users".to_string(),
                target: "sql_column::Users::name".to_string(),
                attributes: HashMap::new(),
            },
        ],
    };

    fs::write(
        &proposals_path,
        serde_json::to_string_pretty(&file).expect("serialize proposals"),
    )
    .expect("write proposals file");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("discover")
        .arg("draft-module")
        .arg(&proposals_path)
        .arg("--out")
        .arg(&out_axi)
        .arg("--infer-constraints")
        .status()
        .expect("run axiograph discover draft-module");

    assert!(
        status.success(),
        "draft-module failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let text = fs::read_to_string(&out_axi).expect("read drafted .axi");
    let module = parse_axi_v1(&text).expect("parse drafted module");
    assert_eq!(module.module_name, "Discovered");

    // Extensional inference should have included a key and the functional `to -> from`.
    let theory = module
        .theories
        .iter()
        .find(|t| t.name == "DiscoveredExtensional")
        .expect("expected extensional theory");

    use axiograph_dsl::schema_v1::ConstraintV1;
    assert!(
        theory.constraints.iter().any(|c| matches!(
            c,
            ConstraintV1::Key { relation, fields } if relation == "SqlHasColumn" && fields == &vec!["from".to_string(), "to".to_string()]
        )),
        "expected key(SqlHasColumn(from,to))"
    );
    assert!(
        theory.constraints.iter().any(|c| matches!(
            c,
            ConstraintV1::Functional { relation, src_field, dst_field }
                if relation == "SqlHasColumn" && src_field == "to" && dst_field == "from"
        )),
        "expected functional SqlHasColumn.to -> SqlHasColumn.from"
    );
}

#[test]
fn viz_dot_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "viz_dot");
    let axpd_path = run_dir.join("build/viz.axpd");
    let dot_path = run_dir.join("build/viz.dot");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("repl")
        .arg("--cmd")
        .arg("gen scenario social_network 3 3 1")
        .arg("--cmd")
        .arg(format!("save {}", axpd_path.display()))
        .arg("--quiet")
        .status()
        .expect("run axiograph repl --cmd ...");

    assert!(
        status.success(),
        "repl gen/save failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("tools")
        .arg("viz")
        .arg(&axpd_path)
        .arg("--out")
        .arg(&dot_path)
        .arg("--focus-name")
        .arg("Alice_0")
        .arg("--hops")
        .arg("2")
        .arg("--max-nodes")
        .arg("120")
        .status()
        .expect("run axiograph tools viz");

    assert!(
        status.success(),
        "viz failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let dot = fs::read_to_string(&dot_path).expect("read dot");
    assert!(
        dot.contains("digraph axiograph"),
        "expected dot output to contain graph header"
    );
    assert!(
        dot.contains("Alice_0") || dot.contains("Person"),
        "expected dot output to contain some node labels"
    );
}

#[test]
fn querycert_anchor_snapshot_export_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "anchor_snapshot_export");

    let input = repo_root.join("examples/anchors/pathdb_export_anchor_v1.axi");
    let cert_path = run_dir.join("build/anchor_query_cert.json");

    let query = "select ?y where name(\"a\") -r1-> ?y limit 10";

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("cert")
        .arg("query")
        .arg(&input)
        .arg("--lang")
        .arg("axql")
        .arg(query)
        .arg("--out")
        .arg(&cert_path)
        .status()
        .expect("run axiograph cert query (anchor snapshot)");
    assert!(
        status.success(),
        "querycert on anchor snapshot export failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let cert_text = fs::read_to_string(&cert_path).expect("read query cert json");
    let cert: CertificateV2 = serde_json::from_str(&cert_text).expect("parse query cert json");

    match cert.payload {
        CertificatePayloadV2::QueryResultV1 { proof } => {
            assert!(
                !proof.rows.is_empty(),
                "expected non-empty rows for anchor snapshot query"
            );
        }
        other => panic!("expected query_result_v1 certificate, got {other:?}"),
    }
}

#[test]
fn querycert_canonical_axi_v3_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "querycert_canonical_v3");

    let input = repo_root.join("examples/manufacturing/SupplyChainHoTT.axi");
    let cert_path = run_dir.join("build/canonical_query_cert_v3.json");

    let query = "select ?to where name(\"RawMetal_A\") -Flow-> ?to limit 10";

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("cert")
        .arg("query")
        .arg(&input)
        .arg("--lang")
        .arg("axql")
        .arg(query)
        .arg("--out")
        .arg(&cert_path)
        .status()
        .expect("run axiograph cert query (canonical .axi)");
    assert!(
        status.success(),
        "querycert on canonical module failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let cert_text = fs::read_to_string(&cert_path).expect("read query cert json");
    let cert: CertificateV2 = serde_json::from_str(&cert_text).expect("parse query cert json");

    assert_eq!(cert.version, 2);
    let anchor = cert.anchor.expect("expected anchor");
    assert!(
        anchor.axi_digest_v1.starts_with("fnv1a64:"),
        "unexpected digest format: {}",
        anchor.axi_digest_v1
    );

    match cert.payload {
        CertificatePayloadV2::QueryResultV3 { proof } => {
            assert!(
                !proof.rows.is_empty(),
                "expected non-empty rows for canonical module query"
            );
        }
        other => panic!("expected query_result_v3 certificate, got {other:?}"),
    }
}

#[test]
fn doc_to_proposals_to_candidate_axi_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "doc_to_candidates");
    let build_dir = run_dir.join("build");

    let input = repo_root.join("examples/docs/sample_conversation.txt");
    let proposals_path = build_dir.join("proposals.json");
    let chunks_path = build_dir.join("chunks.json");
    let facts_path = build_dir.join("facts.json");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("doc")
        .arg(&input)
        .arg("--out")
        .arg(&proposals_path)
        .arg("--chunks")
        .arg(&chunks_path)
        .arg("--facts")
        .arg(&facts_path)
        .arg("--machining")
        .status()
        .expect("run axiograph doc");
    assert!(
        status.success(),
        "doc ingestion failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let proposals_text = fs::read_to_string(&proposals_path).expect("read proposals.json");
    let proposals: ProposalsFileV1 =
        serde_json::from_str(&proposals_text).expect("parse proposals.json");
    assert!(
        !proposals.proposals.is_empty(),
        "expected non-empty proposals from sample conversation"
    );

    let candidates_dir = build_dir.join("candidates");
    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("discover")
        .arg("promote-proposals")
        .arg(&proposals_path)
        .arg("-o")
        .arg(&candidates_dir)
        .arg("--domains")
        .arg("machinist_learning")
        .status()
        .expect("run axiograph discover promote-proposals");
    assert!(
        status.success(),
        "promotion failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let candidate_axi = candidates_dir.join("MachinistLearning.proposals.axi");
    let candidate_text = fs::read_to_string(&candidate_axi).expect("read candidate .axi");
    let parsed = parse_axi_v1(&candidate_text).expect("parse candidate .axi via axi_v1");
    assert!(
        !parsed.instances.is_empty(),
        "expected at least one instance in candidate output"
    );
    let inst = &parsed.instances[0];
    assert!(
        inst.assignments
            .iter()
            .any(|a| a.name == "TacitKnowledge" || a.name == "tacitRule"),
        "expected TacitKnowledge content in candidate output"
    );

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("check")
        .arg("validate")
        .arg(&candidate_axi)
        .status()
        .expect("run axiograph check validate on candidate .axi");
    assert!(
        status.success(),
        "validate failed for candidate module (exit={})",
        status.code().unwrap_or(-1)
    );
}
