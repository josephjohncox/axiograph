use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use axiograph_ingest_docs::{Chunk, ProposalsFileV1};

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
        .join("rust/target/tmp/axiograph_github_import_e2e")
        .join(format!("{label}_{pid}_{nanos}"));
    fs::create_dir_all(&dir).expect("create run dir");
    dir
}

#[test]
fn github_import_local_repo_with_proto_descriptor() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "local_proto_descriptor");
    let repo_src = run_dir.join("repo_src");
    fs::create_dir_all(repo_src.join("src")).expect("create repo src");

    fs::write(
        repo_src.join("README.md"),
        "# Demo Repo\n\nThis is a tiny repo fixture.\n",
    )
    .expect("write README");

    // Include a `.proto` file so repo indexing exercises proto sources too.
    fs::write(
        repo_src.join("api.proto"),
        r#"
syntax = "proto3";

package demo.v1;

service DemoService {
  rpc GetWidget (GetWidgetRequest) returns (GetWidgetResponse);
}

message GetWidgetRequest {
  string widget_id = 1;
}

message GetWidgetResponse {
  string widget_id = 1;
  string status = 2;
}
"#,
    )
    .expect("write api.proto");

    fs::write(
        repo_src.join("src/lib.rs"),
        "pub fn hello() -> &'static str { \"hello\" }\n",
    )
    .expect("write lib.rs");

    let out_dir = run_dir.join("out");
    let descriptor = repo_root.join("examples/proto/large_api/descriptor.json");

    let status = Command::new(&bin)
        .arg("ingest")
        .arg("github")
        .arg("import")
        .arg(&repo_src)
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--proto-descriptor")
        .arg(&descriptor)
        .status()
        .expect("run axiograph ingest github import");

    assert!(status.success(), "github import should succeed");

    let merged_chunks: Vec<Chunk> =
        serde_json::from_str(&fs::read_to_string(out_dir.join("chunks.json")).unwrap()).unwrap();
    assert!(!merged_chunks.is_empty(), "expected some merged chunks");
    assert!(
        merged_chunks
            .iter()
            .any(|c| c.text.contains("syntax = \"proto3\"")),
        "expected repo chunk content to include api.proto text"
    );

    let merged_proposals: ProposalsFileV1 =
        serde_json::from_str(&fs::read_to_string(out_dir.join("proposals.json")).unwrap()).unwrap();
    assert!(
        !merged_proposals.proposals.is_empty(),
        "expected some merged proposals"
    );
}
