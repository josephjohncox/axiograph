use std::process::Command;

#[test]
fn source_diagnostics_cli_runs_read_only_company_typo_example() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../examples/software_authoring/run_source_diagnostics.sh");
    let output = Command::new("bash")
        .arg(script)
        .env("AXIOGRAPH_BIN", env!("CARGO_BIN_EXE_axiograph"))
        .output()
        .expect("run real CLI diagnostic example");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("CLI report");
    assert_eq!(report["ok"], false);
    assert_eq!(report["validation"]["canonical_axi_valid"], false);
    assert_eq!(report["promotion"]["protected_main_eligible"], false);
    let location = &report["diagnostics"][0]["location"];
    assert!(location["path"].as_str().unwrap().ends_with("/Company.axi"));
    assert_eq!(location["module_name"], "CompanyExample");
    assert_eq!(location["suggested_name"], "Company");
    assert_eq!(location["start"]["line"], 5);
    assert_eq!(location["start"]["column"], 32);
    assert_eq!(location["start"]["lsp_character"], 31);
    assert_eq!(
        location["byte_end"].as_u64().unwrap() - location["byte_start"].as_u64().unwrap(),
        6
    );
    assert_eq!(
        location["excerpt"],
        "  relation Employment(company: Compny)"
    );
}
