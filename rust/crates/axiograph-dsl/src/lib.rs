//! Axiograph `.axi` DSL (canonical dialects)
//!
//! This crate defines the canonical, versioned `.axi` surface syntaxes used by
//! Axiograph and provides parsers + typed ASTs for each dialect.
//!
//! We intentionally keep dialects explicit (and machine-checkable in Lean)
//! during migration, while providing a single Rust entrypoint (`axi_v1`) that
//! auto-detects the dialect for the canonical corpus.

pub mod axi_v1;
pub mod digest;
pub mod schema_v1;
