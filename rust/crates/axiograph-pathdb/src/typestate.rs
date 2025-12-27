//! Typestate wrappers for runtime invariants.
//!
//! These are “Rust-side dependent type” building blocks: small newtypes that
//! encode preconditions in the type system, so APIs become self-documenting.
//!
//! Today we start with the highest-ROI invariant:
//! - **normalized paths** (free-groupoid word reduction normal form)
//!
//! Over time we can extend this module with additional invariants:
//! - “typechecked query IR”
//! - “snapshot-scoped ids”
//! - “well-typed `.axi` module” wrappers (already exist as separate types)

use crate::PathExprV2;
use anyhow::{anyhow, Result};

/// A `PathExprV2` that has not been proven/checked to be in normal form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnnormalizedPathExprV2(PathExprV2);

/// A `PathExprV2` that is in the canonical normalization normal form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedPathExprV2(PathExprV2);

impl UnnormalizedPathExprV2 {
    pub fn new(expr: PathExprV2) -> Self {
        Self(expr)
    }

    pub fn as_expr(&self) -> &PathExprV2 {
        &self.0
    }

    pub fn into_expr(self) -> PathExprV2 {
        self.0
    }

    pub fn normalize(self) -> NormalizedPathExprV2 {
        NormalizedPathExprV2(self.0.normalize())
    }
}

impl From<PathExprV2> for UnnormalizedPathExprV2 {
    fn from(value: PathExprV2) -> Self {
        Self::new(value)
    }
}

impl NormalizedPathExprV2 {
    pub(crate) fn new_unchecked(expr: PathExprV2) -> Self {
        Self(expr)
    }

    /// Construct a `NormalizedPathExprV2` by checking the normal-form invariant.
    pub fn new_checked(expr: PathExprV2) -> Result<Self> {
        if expr.normalize() != expr {
            return Err(anyhow!("path expression is not normalized"));
        }
        Ok(Self(expr))
    }

    pub fn as_expr(&self) -> &PathExprV2 {
        &self.0
    }

    pub fn into_expr(self) -> PathExprV2 {
        self.0
    }
}
