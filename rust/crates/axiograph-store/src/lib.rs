//! Axiograph's sole accepted-state and semantic-lineage persistence authority.
//!
//! `AxiStore` publishes immutable objects before atomically advancing one
//! SQLite `store_state` row. Rust validates the finite operational contract; it
//! does not turn those checks into a Lean proof or an ontology-closure claim.

mod materialization;
mod model;
mod store;

pub use materialization::*;
pub use model::*;
pub use store::*;
