//! Unified `.axi` entrypoint: `axi_v1`
//!
//! `axi_v1` is the **single canonical** `.axi` surface language entrypoint.
//!
//! The concrete schema/theory/instance parser currently lives in
//! `schema_v1::SchemaV1Module`, but that is an internal module name rather than
//! a second end-user dialect.
//!
//! We intentionally keep one canonical authoring surface so:
//! - PathDB import/export has one canonical `.axi` plane
//! - certificates can be anchored to a single parser/AST
//! - Rust and Lean stay in lockstep without end-user dialect drift

use crate::schema_v1::{SchemaV1Module, SchemaV1ParseError};

pub fn parse_axi_v1(text: &str) -> Result<SchemaV1Module, SchemaV1ParseError> {
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
            assert!(!module.module_name.is_empty());
        }
    }
}
