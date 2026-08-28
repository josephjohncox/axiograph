//! Software-authoring example harness.
//!
//! This crate is intentionally pedagogical. It demonstrates how a domain or
//! application package can consume `axiograph-software-authoring` as a library
//! to run continuous semantic coverage gates over behavior-case reports. It is
//! not an ontology kernel and does not mutate accepted ontology state.

use std::path::PathBuf;

use anyhow::{Context, Result};
use axiograph_software_authoring::{
    build_continuous_software_coverage_report_from_json_str, ContinuousCheckOptions,
    ContinuousSoftwareCoverageReportV1,
};
use serde::Serialize;

pub const SOFTWARE_AUTHORING_EXAMPLE_CONTINUOUS_CHECK_VERSION_V1: &str =
    "software_authoring_example_continuous_check_v1";

#[derive(Debug, Clone)]
pub struct ContinuousSemanticCoverageExampleOptions {
    pub behavior_report: PathBuf,
    pub repo_root: PathBuf,
    pub require_codegen: Vec<String>,
    pub strict_coverage: bool,
    pub require_code_refs: bool,
    pub require_runtime_theory: bool,
}

#[derive(Debug, Serialize)]
pub struct ContinuousSemanticCoverageExampleReportV1 {
    pub version: &'static str,
    pub source_behavior_report: String,
    pub coverage_report: ContinuousSoftwareCoverageReportV1,
    pub teaching_notes: Vec<String>,
}

pub fn run_continuous_semantic_coverage_example(
    options: &ContinuousSemanticCoverageExampleOptions,
) -> Result<ContinuousSemanticCoverageExampleReportV1> {
    let behavior_report_text = axiograph_security::read_utf8_file_bounded(
        &options.behavior_report,
        8 * 1024 * 1024,
        "software-authoring behavior report",
    )
    .with_context(|| format!("read `{}`", options.behavior_report.display()))?;
    let coverage_report = build_continuous_software_coverage_report_from_json_str(
        &behavior_report_text,
        &ContinuousCheckOptions {
            repo_root: options.repo_root.clone(),
            require_codegen: options.require_codegen.clone(),
            strict_coverage: options.strict_coverage,
            require_code_refs: options.require_code_refs,
            require_runtime_theory: options.require_runtime_theory,
        },
    )?;
    Ok(ContinuousSemanticCoverageExampleReportV1 {
        version: SOFTWARE_AUTHORING_EXAMPLE_CONTINUOUS_CHECK_VERSION_V1,
        source_behavior_report: options.behavior_report.display().to_string(),
        coverage_report,
        teaching_notes: vec![
            "This example crate consumes Axiograph reports as a library client.".to_string(),
            "Continuous semantic coverage is a software gate over typed ontology-derived reports, not an ontology fact.".to_string(),
            "Use the core CLI to produce behavior reports; use this crate to show how application packages can enforce them.".to_string(),
        ],
    })
}

pub fn render_continuous_semantic_coverage_example(
    report: &ContinuousSemanticCoverageExampleReportV1,
) -> String {
    let mut out = String::new();
    out.push_str("software-authoring continuous semantic coverage\n");
    out.push_str(&format!(
        "  source behavior report: {}\n",
        report.source_behavior_report
    ));
    out.push_str(&format!("  pass: {}\n", report.coverage_report.pass));
    out.push_str(&format!("  status: {:?}\n", report.coverage_report.status));
    out.push_str(&format!(
        "  codegen present: [{}]\n",
        report.coverage_report.present_codegen_languages.join(", ")
    ));
    out.push_str(&format!(
        "  codegen missing: [{}]\n",
        report.coverage_report.missing_codegen_languages.join(", ")
    ));
    out.push_str(&format!(
        "  semantic coverage: {}/{} covered\n",
        report.coverage_report.coverage.covered_rules, report.coverage_report.coverage.total_rules
    ));
    for warning in &report.coverage_report.warnings {
        out.push_str(&format!("  warning: {warning}\n"));
    }
    for failure in &report.coverage_report.failures {
        out.push_str(&format!("  failure: {failure}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_behavior_report(json: &str, label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "axiograph_software_authoring_example_{}_{}",
            label,
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("behavior_report.json");
        fs::write(&path, json).expect("write report");
        path
    }

    fn complete_behavior_report() -> &'static str {
        r##"{
          "version": "behavior_case_report_v1",
          "behavior_case": { "case_id": "software_authoring.example" },
          "context_report": {
            "coverage": {
              "total_rules": 2,
              "covered_rules": 2,
              "tested_rules": 2,
              "implemented_rules": 2,
              "ontology_only_rules": 0,
              "drifted_rules": 0,
              "missing_obligations": []
            },
            "competency_coverage": {
              "total": 1,
              "satisfied": 1,
              "coverage": 1.0,
              "questions": [
                { "name": "reserve credit is executable", "satisfied": true }
              ]
            }
          },
          "codegen_previews": [
            { "language": "go", "file_hint": "internal/behaviorcases/example_test.go", "content": "package behaviorcases\n" },
            { "language": "python", "file_hint": "tests/behavior_cases/test_example.py", "content": "def test_example(): pass\n" },
            { "language": "rust", "file_hint": "tests/behavior_cases/example.rs", "content": "#[test] fn example() {}\n" },
            { "language": "typescript", "file_hint": "tests/behavior-cases/example.spec.ts", "content": "it('example', () => {})\n" }
          ]
        }"##
    }

    fn rust_only_behavior_report() -> &'static str {
        r##"{
          "version": "behavior_case_report_v1",
          "behavior_case": { "case_id": "software_authoring.example" },
          "context_report": {
            "coverage": {
              "total_rules": 2,
              "covered_rules": 2,
              "tested_rules": 2,
              "implemented_rules": 2,
              "ontology_only_rules": 0,
              "drifted_rules": 0,
              "missing_obligations": []
            },
            "competency_coverage": {
              "total": 1,
              "satisfied": 1,
              "coverage": 1.0,
              "questions": [
                { "name": "reserve credit is executable", "satisfied": true }
              ]
            }
          },
          "codegen_previews": [
            { "language": "rust", "file_hint": "tests/behavior_cases/example.rs", "content": "#[test] fn example() {}\n" }
          ]
        }"##
    }

    #[test]
    fn example_continuous_check_passes_with_advisory_theory_warning() {
        let path = write_behavior_report(complete_behavior_report(), "pass");
        let report =
            run_continuous_semantic_coverage_example(&ContinuousSemanticCoverageExampleOptions {
                behavior_report: path,
                repo_root: PathBuf::from("."),
                require_codegen: vec![
                    "go".into(),
                    "python".into(),
                    "rust".into(),
                    "typescript".into(),
                ],
                strict_coverage: false,
                require_code_refs: false,
                require_runtime_theory: false,
            })
            .expect("run continuous check");

        assert!(report.coverage_report.pass);
        assert!(report
            .coverage_report
            .warnings
            .iter()
            .any(|warning| warning.contains("runtime theory")));
    }

    #[test]
    fn example_continuous_check_fails_when_codegen_is_missing() {
        let path = write_behavior_report(rust_only_behavior_report(), "missing_codegen");
        let report =
            run_continuous_semantic_coverage_example(&ContinuousSemanticCoverageExampleOptions {
                behavior_report: path,
                repo_root: PathBuf::from("."),
                require_codegen: vec!["rust".into(), "typescript".into()],
                strict_coverage: false,
                require_code_refs: false,
                require_runtime_theory: false,
            })
            .expect("run continuous check");

        assert!(!report.coverage_report.pass);
        assert_eq!(
            report.coverage_report.missing_codegen_languages,
            vec!["typescript"]
        );
    }

    #[test]
    fn example_continuous_check_fails_when_runtime_theory_is_required() {
        let path = write_behavior_report(complete_behavior_report(), "runtime_theory");
        let report =
            run_continuous_semantic_coverage_example(&ContinuousSemanticCoverageExampleOptions {
                behavior_report: path,
                repo_root: PathBuf::from("."),
                require_codegen: vec!["rust".into()],
                strict_coverage: false,
                require_code_refs: false,
                require_runtime_theory: true,
            })
            .expect("run continuous check");

        assert!(!report.coverage_report.pass);
        assert!(report
            .coverage_report
            .failures
            .iter()
            .any(|failure| failure.contains("runtime theory-check")));
    }
}
