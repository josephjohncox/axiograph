//! Axiograph `.axi` DSL
//!
//! This crate defines the canonical, versioned `.axi` surface used by
//! Axiograph and provides the parser + typed AST behind `axi_v1`.
//!
//! Internal module names remain versioned so Rust and Lean can stay in
//! lockstep, but contributors should treat `axi_v1` as the single canonical
//! authoring surface.

pub mod axi_v1;
pub mod digest;
pub mod schema_v1;
