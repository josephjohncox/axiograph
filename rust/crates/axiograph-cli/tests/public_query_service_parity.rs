//! Adapter parity is separate from the query crate's process-free embedding test.
use anyhow::Result;
use axiograph_pathdb::{axi_semantics::MetaPlaneIndex, PathDB};
use axiograph_query::{
    axql::parse_axql_query,
    competency_questions::{
        evaluate_competency_questions_with_trust, generate_from_schema, CompetencyQuestionOptions,
        CompetencyQuestionV1,
    },
    query_ir::QueryIrV1,
};
use serde_json::{json, Value};
use std::process::Command;

#[test]
fn public_query_cq_service_matches_cli_generation_and_workspace_reports() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let source = include_str!("../../axiograph-query/tests/fixtures/EmbeddedQuery.axi");
    let axi = temp.path().join("EmbeddedQuery.axi");
    std::fs::write(&axi, source)?;
    let mut db = PathDB::new();
    axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, source)?;
    db.build_indexes();
    let meta = MetaPlaneIndex::from_db(&db)?;
    let generated = generate_from_schema(&db, &CompetencyQuestionOptions::default())?;
    let output_path = temp.path().join("questions.json");
    let output = Command::new(env!("CARGO_BIN_EXE_axiograph"))
        .args(["discover", "competency-questions"])
        .arg(&axi)
        .arg("--out")
        .arg(&output_path)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::to_value(&generated)?,
        serde_json::from_slice::<Value>(&std::fs::read(output_path)?)?
    );

    let text = "select ?x where ?x is Demo.Supplier limit 5";
    let ir = QueryIrV1::from_axql_query(&parse_axql_query(text)?);
    let mut prepared = ir.compile_with_meta(&db, Some(&meta))?;
    let answer = prepared.execute_answer(&db, Some(&meta))?;
    assert_eq!(answer.result().rows.len(), 2);
    let metadata = prepared.metadata_with_meta(Some(&meta))?;
    let cq = CompetencyQuestionV1 {
        name: "suppliers".into(),
        query: text.into(),
        min_rows: 1,
        weight: 1.0,
        ..Default::default()
    };
    let coverage = evaluate_competency_questions_with_trust(&db, &[cq])?;
    let request = temp.path().join("request.json");
    std::fs::write(
        &request,
        serde_json::to_vec(&json!({
            "version": "authoring_workspace_request_v1",
            "presentation": {"detail":"full"},
            "axi_path": "EmbeddedQuery.axi",
            "query_ir_v1": ir,
            "cq_text": format!("version competency_question_bundle_v1\nquestion suppliers:\n  axql: {text}\n")
        }))?,
    )?;
    let report_path = temp.path().join("report.json");
    let output = Command::new(env!("CARGO_BIN_EXE_axiograph"))
        .args(["authoring", "workspace", "--workspace"])
        .arg(temp.path())
        .arg("--request")
        .arg(&request)
        .arg("--out")
        .arg(&report_path)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&std::fs::read(report_path)?)?;
    assert_eq!(
        report["prepared_query"]["prepared_query_id"],
        metadata.prepared_query_id
    );
    assert_eq!(
        report["prepared_query"]["query_ir_id"],
        metadata.query_ir_id
    );
    assert_eq!(
        report["prepared_query"]["non_claims"],
        serde_json::to_value(&metadata.non_claims)?
    );
    assert_eq!(
        report["prepared_query"]["trust"],
        serde_json::to_value(&metadata.trust)?
    );
    let evaluation = &report["competency_questions"]["evaluation"]["questions"][0];
    assert_eq!(evaluation["rows"], coverage.questions[0].rows);
    assert_eq!(evaluation["satisfied"], coverage.questions[0].satisfied);
    assert_eq!(
        evaluation["trust"],
        serde_json::to_value(&coverage.questions[0].trust)?
    );
    Ok(())
}
