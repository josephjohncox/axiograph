#![forbid(unsafe_code)]
//! Semantic ownership boundary for Axiograph's compiled meaning plane.
//!
//! This crate owns the `AXIOGRAPH-ID` family, accepted anchors, typed semantic
//! references, diagnostics, trust vocabulary, evolution maps, and canonical
//! compiler entry packages. Runtime crates may compute and store values, but
//! they must use these checked types rather than inventing semantic ids.

pub mod anchor;
pub mod diagnostics;
pub mod evolution;
pub mod identity;
pub mod package;
pub mod reference;
pub mod theory;
pub mod trust;

pub use anchor::*;
pub use diagnostics::*;
pub use evolution::*;
pub use identity::*;
pub use package::*;
pub use reference::*;
pub use theory::*;
pub use trust::*;
