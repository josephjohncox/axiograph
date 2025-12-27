//! Proof-mode plumbing for “untrusted engine, trusted checker”.
//!
//! Axiograph’s runtime often wants two APIs for the same operation:
//!
//! - a **fast** version that just computes a result, and
//! - a **proof-producing** version that additionally emits a witness/certificate.
//!
//! Rather than duplicating every function, we use a compile-time switch:
//! a generic `ProofMode` parameter whose implementation decides whether
//! to **evaluate** proof-producing closures and what proof type to return.
//!
//! This keeps “no proof” execution zero-overhead and makes “with proof”
//! execution explicit in types.

use std::marker::PhantomData;

/// Compile-time switch for whether an operation should produce proofs/certificates.
pub trait ProofMode {
    /// The proof payload type produced by an operation.
    ///
    /// - In `NoProof`, this is `()`.
    /// - In `WithProof`, this is the actual proof value `P`.
    type Proof<P>;

    /// Conditionally evaluate `produce` and return a proof payload.
    ///
    /// Implementations must **not** evaluate `produce` when proofs are disabled.
    fn capture<P>(produce: impl FnOnce() -> P) -> Self::Proof<P>;

    /// Conditionally access the underlying proof value mutably.
    ///
    /// This is useful for “journals” that accumulate proof events.
    fn with_mut<P>(proof: &mut Self::Proof<P>, f: impl FnOnce(&mut P));
}

/// Proofs disabled: closures are not evaluated; proof payload is `()`.
#[derive(Debug)]
pub enum NoProof {}

impl ProofMode for NoProof {
    type Proof<P> = ();

    #[inline]
    fn capture<P>(_produce: impl FnOnce() -> P) -> Self::Proof<P> {
        ()
    }

    #[inline]
    fn with_mut<P>(_proof: &mut Self::Proof<P>, _f: impl FnOnce(&mut P)) {}
}

/// Proofs enabled: closures are evaluated; proof payload is `P`.
#[derive(Debug)]
pub enum WithProof {}

impl ProofMode for WithProof {
    type Proof<P> = P;

    #[inline]
    fn capture<P>(produce: impl FnOnce() -> P) -> Self::Proof<P> {
        produce()
    }

    #[inline]
    fn with_mut<P>(proof: &mut Self::Proof<P>, f: impl FnOnce(&mut P)) {
        f(proof)
    }
}

/// A result bundled with a proof payload, parameterized by the chosen `ProofMode`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proved<M: ProofMode, T, P> {
    pub value: T,
    pub proof: M::Proof<P>,
}

impl<M: ProofMode, T, P> Proved<M, T, P> {
    pub fn map_value<U>(self, f: impl FnOnce(T) -> U) -> Proved<M, U, P> {
        Proved {
            value: f(self.value),
            proof: self.proof,
        }
    }
}

/// Accumulates proof events only when `M = WithProof`.
///
/// This is a lightweight building block for “execution contexts”:
/// query evaluation can emit a trace, and optimization passes can emit certificates.
#[derive(Debug, Clone)]
pub struct ProofJournal<M: ProofMode, Entry> {
    entries: M::Proof<Vec<Entry>>,
    _phantom: PhantomData<Entry>,
}

impl<M: ProofMode, Entry> ProofJournal<M, Entry> {
    pub fn new() -> Self {
        Self {
            entries: M::capture(Vec::new),
            _phantom: PhantomData,
        }
    }

    /// Record an entry, but only if proofs are enabled.
    pub fn record(&mut self, produce: impl FnOnce() -> Entry) {
        M::with_mut(&mut self.entries, |entries| entries.push(produce()));
    }

    pub fn into_entries(self) -> M::Proof<Vec<Entry>> {
        self.entries
    }
}

impl<M: ProofMode, Entry> Default for ProofJournal<M, Entry> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn no_proof_does_not_evaluate_closure() {
        let evaluated = Cell::new(false);
        let _: <NoProof as ProofMode>::Proof<u32> = NoProof::capture(|| {
            evaluated.set(true);
            42
        });
        assert!(!evaluated.get());
    }

    #[test]
    fn with_proof_evaluates_closure() {
        let evaluated = Cell::new(false);
        let proof: <WithProof as ProofMode>::Proof<u32> = WithProof::capture(|| {
            evaluated.set(true);
            42
        });
        assert!(evaluated.get());
        assert_eq!(proof, 42);
    }

    #[test]
    fn journal_records_only_with_proofs() {
        let mut none: ProofJournal<NoProof, u32> = ProofJournal::new();
        none.record(|| 1);
        let entries: <NoProof as ProofMode>::Proof<Vec<u32>> = none.into_entries();
        let _: () = entries;

        let mut some: ProofJournal<WithProof, u32> = ProofJournal::new();
        some.record(|| 1);
        some.record(|| 2);
        let entries: Vec<u32> = some.into_entries();
        assert_eq!(entries, vec![1, 2]);
    }
}
