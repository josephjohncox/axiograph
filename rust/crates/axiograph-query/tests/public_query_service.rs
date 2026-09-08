//! An external library client: no private source inclusion or CLI execution.
use anyhow::Result;
use axiograph_pathdb::{axi_semantics::MetaPlaneIndex, PathDB};
use axiograph_query::{
    axql::parse_axql_query,
    competency_questions::{
        evaluate_competency_questions_with_trust, generate_from_schema, CompetencyQuestionOptions,
        CompetencyQuestionV1,
    },
    query_ir::{QueryCertificatePolicyV1, QueryIrV1},
};

const SOURCE: &str = include_str!("fixtures/EmbeddedQuery.axi");

fn database() -> Result<(PathDB, MetaPlaneIndex)> {
    let mut db = PathDB::new();
    axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, SOURCE)?;
    db.build_indexes();
    let meta = MetaPlaneIndex::from_db(&db)?;
    Ok((db, meta))
}

fn query(text: &str) -> Result<QueryIrV1> {
    Ok(QueryIrV1::from_axql_query(&parse_axql_query(text)?))
}

#[test]
fn embedded_query_and_generated_cqs_share_preparation_and_scoped_trust() -> Result<()> {
    let (db, meta) = database()?;
    let questions = generate_from_schema(&db, &CompetencyQuestionOptions::default())?;
    assert!(!questions.is_empty());
    let report = evaluate_competency_questions_with_trust(&db, &questions)?;
    assert_eq!(report.total, questions.len());
    assert_eq!(report.satisfied, report.total);
    for (question, evaluation) in questions.iter().zip(&report.questions) {
        let mut prepared = query(&question.query)?.compile_with_meta(&db, Some(&meta))?;
        let answer = prepared.execute_answer(&db, Some(&meta))?;
        assert_eq!(answer.result().rows.len(), evaluation.rows);
        assert_eq!(answer.lifecycle_state_name(), "validated");
        let metadata = prepared.metadata_with_meta(Some(&meta))?;
        assert_eq!(
            serde_json::to_value(&metadata)?,
            serde_json::to_value(&evaluation.prepared_query)?
        );
        assert_eq!(metadata.non_claims.ontology_closure_claim, "not_claimed");
        assert_eq!(metadata.non_claims.completeness_claim, "not_claimed");
        assert!(!metadata.trust.soundness.contains("lean_verified"));
    }
    Ok(())
}

#[test]
fn embedded_query_rejects_wrong_database_drift_limits_and_unverified_policy() -> Result<()> {
    let (mut db, meta) = database()?;
    let ir = query("select ?x where ?x is Demo.Supplier limit 5")?;
    let mut prepared = ir.compile_with_meta(&db, Some(&meta))?;
    let (other, other_meta) = database()?;
    assert!(prepared.execute(&other, Some(&other_meta)).is_err());
    assert!(QueryCertificatePolicyV1::RequireVerified
        .ensure_require_verified_preconditions(&prepared.certifiability(), None, Some(SOURCE))
        .is_err());
    assert!(QueryCertificatePolicyV1::RequireVerified
        .ensure_verified_result(None)
        .is_err());
    let mut oversized = ir;
    oversized.limit = Some(201);
    assert!(oversized.compile_with_meta(&db, Some(&meta)).is_err());
    let answer = prepared.execute_answer(&db, Some(&meta))?;
    db.add_entity("Supplier", vec![("name", "c"), ("axi_schema", "Demo")]);
    db.build_indexes();
    assert!(prepared
        .certify_answer_with_anchors(
            answer,
            &db,
            Some(&meta),
            axiograph_kernel::RevisionDigestV2::from_accepted_text(SOURCE),
        )
        .is_err());
    Ok(())
}

#[test]
fn embedded_refinement_is_real_and_rejects_tampered_identity() -> Result<()> {
    let (db, meta) = database()?;
    let prepared = query("select ?dst where ?f = Demo.Flow(from=a, to=?dst) limit 5")?
        .compile_with_meta(&db, Some(&meta))?;
    let metadata = prepared.metadata_with_meta(Some(&meta))?;
    let (handle, applied) = metadata
        .refinement_handles
        .iter()
        .find_map(|handle| {
            prepared
                .apply_runtime_refinement_handle(&db, Some(&meta), handle)
                .ok()
                .map(|applied| (handle, applied))
        })
        .expect("production planner should offer an applicable typed refinement");
    assert_ne!(
        applied.base_prepared_query.prepared_query_id,
        applied.refined_prepared_query.prepared_query_id
    );
    let mut tampered = handle.clone();
    tampered.id.push_str("-tampered");
    assert!(prepared
        .apply_runtime_refinement_handle(&db, Some(&meta), &tampered)
        .is_err());
    assert!(prepared
        .apply_runtime_refinement_by_id(&db, Some(&meta), "unknown")
        .is_err());
    Ok(())
}

#[test]
fn embedded_cq_failures_stay_failures_with_repair_and_trust_reports() -> Result<()> {
    let (db, _) = database()?;
    let questions = [
        CompetencyQuestionV1 {
            name: "missing".into(),
            query: "select ?x where ?x is Demo.Compny limit 1".into(),
            min_rows: 1,
            weight: 2.0,
            ..Default::default()
        },
        CompetencyQuestionV1 {
            name: "not-lowered".into(),
            question: Some("Find a supplier".into()),
            min_rows: 1,
            weight: 3.0,
            ..Default::default()
        },
    ];
    // Typed lowering errors reject the evaluation; unlowered authored questions
    // remain reportable residuals. Neither is silently counted as satisfied.
    assert!(evaluate_competency_questions_with_trust(&db, &questions[..1]).is_err());
    let report = evaluate_competency_questions_with_trust(&db, &questions[1..])?;
    assert_eq!(report.total, 1);
    assert_eq!(report.satisfied, 0);
    assert_eq!(report.cost, 3.0);
    assert!(report
        .questions
        .iter()
        .all(|q| !q.satisfied && !q.trust.reasons.is_empty()));
    Ok(())
}
