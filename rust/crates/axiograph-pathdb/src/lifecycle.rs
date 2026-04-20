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
    }
}
