//! Runtime “branding” for PathDB-scoped identifiers.
//!
//! Rust cannot express “this id belongs to *that* DB snapshot” as a dependent
//! type, but we can still prevent a large class of bugs by:
//!
//! - assigning each `PathDB` instance a fresh `DbToken`, and
//! - storing the token alongside typed/witness-bearing wrappers, then
//! - checking token equality at runtime at the DB boundary.
//!
//! This is intentionally **not** serialized. It is a process-local safety
//! mechanism, not a persistence identity. Snapshot identity is handled by
//! `.axi` anchors / module digests in certificates.
//!
//! See also:
//! - `docs/explanation/RUST_DEPENDENT_TYPES.md` (why we do this),
//! - `docs/explanation/TOPOS_THEORY.md` (how this fits into the “schemas/instances/contexts” semantics story).

use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DB_TOKEN: AtomicU64 = AtomicU64::new(1);

/// A process-local token identifying a particular `PathDB` instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DbToken(NonZeroU64);

impl DbToken {
    pub fn new() -> Self {
        let raw = NEXT_DB_TOKEN.fetch_add(1, Ordering::Relaxed);
        Self(NonZeroU64::new(raw).expect("NEXT_DB_TOKEN starts at 1"))
    }

    pub fn raw(self) -> u64 {
        self.0.get()
    }
}

impl Default for DbToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Error when a typed/witness-bearing value is used with the wrong DB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbTokenMismatch {
    pub expected: DbToken,
    pub actual: DbToken,
}

impl std::fmt::Display for DbTokenMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "db token mismatch (expected db#{}, got db#{})",
            self.expected.raw(),
            self.actual.raw()
        )
    }
}

impl std::error::Error for DbTokenMismatch {}

/// A value that is “branded” to a specific `PathDB` instance via a `DbToken`.
///
/// This is a runtime-only safety wrapper: it prevents accidentally mixing
/// witness/proof objects (paths, rewrites, reachability chains, etc.) across
/// different in-memory DB instances/snapshots.
///
/// The brand token is **not** serialized. For persistent identity, use
/// `.axi` anchors / module digests in certificates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbBranded<T> {
    db_token: DbToken,
    value: T,
}

impl<T> DbBranded<T> {
    pub fn new(db_token: DbToken, value: T) -> Self {
        Self { db_token, value }
    }

    pub fn db_token(&self) -> DbToken {
        self.db_token
    }

    pub fn assert_token(&self, actual: DbToken) -> Result<(), DbTokenMismatch> {
        if self.db_token != actual {
            return Err(DbTokenMismatch {
                expected: self.db_token,
                actual,
            });
        }
        Ok(())
    }

    pub fn assert_in_db(&self, db: &crate::PathDB) -> Result<(), DbTokenMismatch> {
        self.assert_token(db.db_token())
    }

    pub fn get(&self, db: &crate::PathDB) -> Result<&T, DbTokenMismatch> {
        self.assert_in_db(db)?;
        Ok(&self.value)
    }

    pub fn get_with_token(&self, actual: DbToken) -> Result<&T, DbTokenMismatch> {
        self.assert_token(actual)?;
        Ok(&self.value)
    }

    pub fn into_inner(self) -> T {
        self.value
    }

    pub fn into_inner_in_db(self, db: &crate::PathDB) -> Result<T, DbTokenMismatch> {
        self.assert_in_db(db)?;
        Ok(self.value)
    }

    pub fn into_inner_with_token(self, actual: DbToken) -> Result<T, DbTokenMismatch> {
        self.assert_token(actual)?;
        Ok(self.value)
    }
}
