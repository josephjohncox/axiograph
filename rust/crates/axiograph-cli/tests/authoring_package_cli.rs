use anyhow::Result;
use serde_json::{json, Value};
use std::process::Command;

#[test]
fn authoring_package_cli_runs_schema_only_import_query_and_cq_in_full_and_compact_modes(
) -> Result<()> {
    let temp = tempfile::tempdir()?;
    std::fs::write(temp.path().join("Base.axi"), "module Base\nschema Shared:\n  object Person\n  relation Parent(child: Person, parent: Person)\n")?;
    let root = "module Extension\nimport Base\ninstance Family of Shared:\n  Person = {Alice, Bob}\n  Parent = {p: (child=Alice, parent=Bob)}\n";
    std::fs::write(temp.path().join("Extension.axi"), root)?;
    std::fs::write(temp.path().join("questions.cq"), "version competency_question_bundle_v1\nquestion parent_exists:\n  ask: is there a parent?\n  expect: exists Shared.Parent(child=?child, parent=?parent)\n")?;
    let request = json!({
        "version":"authoring_workspace_request_v1", "operation":"inspect",
        "axi_path":"Extension.axi", "axi_text":format!("{root}\n-- unsaved CLI bytes\n"),
        "cq_path":"questions.cq", "schema":"Shared",
        "query_ir_v1":{"version":1,"select_vars":["person"],"where_atoms":[{"kind":"type","term":"?person","type":"Shared.Person"}],"limit":10}
    });
    std::fs::write(
        temp.path().join("request.json"),
        serde_json::to_vec(&request)?,
    )?;
    let run = |detail: &str| -> Result<Value> {
        let output = Command::new(env!("CARGO_BIN_EXE_axiograph"))
            .args(["authoring", "workspace", "--workspace"])
            .arg(temp.path())
            .arg("--request")
            .arg(temp.path().join("request.json"))
            .args(["--detail", detail])
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    };
    let full = run("full")?;
    assert_eq!(full["ok"], true, "{}", full["diagnostics"]);
    assert_eq!(full["competency_questions"]["evaluation"]["satisfied"], 1);
    assert!(full["prepared_query"].is_object());
    let compact = run("summary")?;
    assert_eq!(compact["ok"], true);
    for field in ["source", "trust", "promotion"] {
        assert_eq!(compact[field], full[field]);
    }
    assert_eq!(full["promotion"]["protected_main_eligible"], false);
    assert_eq!(
        std::fs::read_to_string(temp.path().join("Extension.axi"))?,
        root
    );
    Ok(())
}
