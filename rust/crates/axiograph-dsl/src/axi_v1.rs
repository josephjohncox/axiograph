//! Unified `.axi` entrypoint: `axi_v1`
//!
//! `axi_v1` is the **single canonical** `.axi` surface language entrypoint.
//!
//! For the initial Rust+Lean-only release we intentionally keep exactly one
//! concrete surface syntax:
//! - `axi_schema_v1` → `schema_v1::SchemaV1Module`
//!
//! Historical note: the repo previously carried a separate `axi_learning_v1`
//! dialect. We removed that split in favor of a single schema/theory/instance
//! syntax so:
//! - PathDB import/export has one canonical `.axi` plane
//! - certificates can be anchored to a single parser/AST
//! - Rust and Lean stay in lockstep without dialect detection

pub use crate::schema_v1::{SchemaV1Module as AxiV1Module, SchemaV1ParseError as AxiV1ParseError};

pub fn parse_axi_v1(text: &str) -> Result<AxiV1Module, AxiV1ParseError> {
    crate::schema_v1::parse_schema_v1(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .expect("canonicalize repo root")
    }

    #[test]
    fn parses_canonical_corpus_via_axi_v1() {
        for path in [
            "examples/economics/EconomicFlows.axi",
            "examples/learning/MachinistLearning.axi",
            "examples/ontology/SchemaEvolution.axi",
        ] {
            let text = std::fs::read_to_string(repo_root().join(path)).expect("read .axi");
            let module = parse_axi_v1(&text).expect("parse axi_v1");
            assert_eq!(module.module_name.is_empty(), false);
        }
    }
}
