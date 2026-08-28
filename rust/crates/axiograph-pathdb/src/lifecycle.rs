//! Typestate markers for semantic workflow artifacts.
//!
//! Lean remains the trusted checker. These markers make the Rust-side workflow
//! explicit so callers cannot accidentally blur important state transitions.

/// Trait implemented by lifecycle marker states.
pub trait LifecycleState {
    const NAME: &'static str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Parsed {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Validated {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reviewed {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Accepted {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Certified {}

/// A certificate was emitted by the untrusted Rust producer but has not yet
/// been accepted by the approved Lean checker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CertificateEmitted {}

/// The approved Lean checker accepted the exact certificate, prepared-query
/// digest, and answer digest carried by the artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LeanVerified {}

impl LifecycleState for Parsed {
    const NAME: &'static str = "parsed";
}

impl LifecycleState for Validated {
    const NAME: &'static str = "validated";
}

impl LifecycleState for Reviewed {
    const NAME: &'static str = "reviewed";
}

impl LifecycleState for Accepted {
    const NAME: &'static str = "accepted";
}

impl LifecycleState for Certified {
    const NAME: &'static str = "certified";
}

impl LifecycleState for CertificateEmitted {
    const NAME: &'static str = "certificate_emitted";
}

impl LifecycleState for LeanVerified {
    const NAME: &'static str = "lean_verified";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_markers_have_stable_names() {
        assert_eq!(Parsed::NAME, "parsed");
        assert_eq!(Validated::NAME, "validated");
        assert_eq!(Reviewed::NAME, "reviewed");
        assert_eq!(Accepted::NAME, "accepted");
        assert_eq!(Certified::NAME, "certified");
        assert_eq!(CertificateEmitted::NAME, "certificate_emitted");
        assert_eq!(LeanVerified::NAME, "lean_verified");
    }
}
