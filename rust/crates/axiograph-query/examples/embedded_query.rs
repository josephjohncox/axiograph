//! Run with `cargo run -p axiograph-query --example embedded_query` from rust/.
//! This is an in-memory draft query, not accepted-store publication or a proof.
use anyhow::Result;
use axiograph_pathdb::{axi_semantics::MetaPlaneIndex, PathDB};
use axiograph_query::{
    axql::parse_axql_query,
    competency_questions::{
        evaluate_competency_questions_with_trust, generate_from_schema, CompetencyQuestionOptions,
    },
    query_ir::QueryIrV1,
};

fn main() -> Result<()> {
    let source = include_str!("../tests/fixtures/EmbeddedQuery.axi");
    let mut db = PathDB::new();
    axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, source)?;
    db.build_indexes();
    let meta = MetaPlaneIndex::from_db(&db)?;
    let ir = QueryIrV1::from_axql_query(&parse_axql_query(
        "select ?dst where ?f = Demo.Flow(from=a, to=?dst) limit 5",
    )?);
    let mut prepared = ir.compile_with_meta(&db, Some(&meta))?;
    let answer = prepared.execute_answer(&db, Some(&meta))?;
    let questions = generate_from_schema(&db, &CompetencyQuestionOptions::default())?;
    let coverage = evaluate_competency_questions_with_trust(&db, &questions)?;
    println!(
        "rows={} cq={}/{} trust={} completeness={}",
        answer.result().rows.len(),
        coverage.satisfied,
        coverage.total,
        answer.trust_contract().soundness,
        answer.trust_contract().completeness_claim
    );
    Ok(())
}
