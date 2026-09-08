//! Reusable boundary parsers shared by the Axiograph CLI and adversarial tests.
//!
//! Embedded query preparation/execution, CQ services, refinement identities,
//! and scoped trust reports belong to the public `axiograph-query` crate.
//! Workspace authoring and transport orchestration remain in the CLI binary.

pub mod proposal_adapter_boundary;
pub mod repl_command;
