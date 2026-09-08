//! Embedded query and competency-question services over derived PathDB indexes.
//!
//! Prepare [`query_ir::QueryIrV1`] against a caller-owned database, execute the
//! resulting handle against that same database identity, and inspect its scoped
//! trust report. Runtime results are not accepted ontology state or Lean proofs.
//! File resolution, LLM prompt translation, workspace authoring, and transports
//! belong to callers; this crate has no CLI dependency.

pub mod axql;
pub mod competency_questions;
pub mod query_ir;
mod security;
pub mod trust_contract;
pub mod typed_refinement;
pub mod verifier_bridge;

#[cfg(test)]
fn load_test_fixture(input: &std::path::Path) -> anyhow::Result<axiograph_pathdb::PathDB> {
    let text =
        security::read_utf8_file_bounded(input, security::MAX_TEXT_INPUT_BYTES, "query fixture")?;
    let mut db = axiograph_pathdb::PathDB::new();
    axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, &text)?;
    db.build_indexes();
    Ok(db)
}
