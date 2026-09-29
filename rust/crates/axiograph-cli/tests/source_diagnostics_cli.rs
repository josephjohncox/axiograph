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
    assert_eq!(report["diagnostic_collection"]["observed_errors"], 3);
    assert_eq!(report["diagnostic_collection"]["returned_errors"], 3);
    assert_eq!(report["diagnostic_collection"]["truncated"], false);
    assert_eq!(
        report["diagnostic_collection"]["order"],
        "authoring_pipeline_order_with_canonical_import_closure_source_suborder"
    );
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 3);
    assert_eq!(diagnostics[0]["location"]["module_name"], "Base");
    assert_eq!(diagnostics[1]["location"]["module_name"], "Base");
    assert_eq!(diagnostics[2]["location"]["module_name"], "Root");
    for diagnostic in diagnostics {
        let location = &diagnostic["location"];
        assert_eq!(diagnostic["code"], "authoring_canonical_type_failed");
        assert_eq!(location["subject"]["status"], "syntactic");
        assert_eq!(location["subject"]["subject"]["kind"], "role_type_carrier");
        assert!(matches!(
            location["suggested_name"].as_str(),
            Some("Company" | "Person")
        ));
        let expected_bytes = if location["suggested_name"] == "Company" {
            6
        } else {
            5
        };
        assert_eq!(
            location["byte_end"].as_u64().unwrap() - location["byte_start"].as_u64().unwrap(),
            expected_bytes
        );
    }
    assert!(diagnostics[0]["location"]["path"]
        .as_str()
        .unwrap()
        .ends_with("/Base.axi"));
    assert_eq!(
        diagnostics[0]["location"]["excerpt"],
        "  relation Employment(company: Compny)"
    );
}
