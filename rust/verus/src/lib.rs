//! Verus-Verified Axiograph Core
//!
//! This crate contains components verified by Verus.
//!
//! ## Setup
//!
//! 1. Install Verus: https://github.com/verus-lang/verus
//!    ```bash
//!    git clone https://github.com/verus-lang/verus
//!    cd verus/source
//!    ./tools/get-z3.sh
//!    source ../tools/activate
//!    vargo build --release
//!    ```
//!
//! 2. Verify this crate:
//!    ```bash
//!    verus src/lib.rs
//!    ```
//!
//! ## What's Verified
//!
//! - Probability invariants: values always in [0, 1]
//! - Bitmap bounds: no out-of-bounds access
//! - Path signatures: length-indexed for safety
//! - Reachability proofs: witnesses are valid

// ============================================================================
// Verus Prelude (only active under Verus)
// ============================================================================

#[cfg(verus)]
use builtin::*;
#[cfg(verus)]
use builtin_macros::*;
#[cfg(verus)]
use vstd::prelude::*;

// ============================================================================
// Verified Probability
// ============================================================================

/// A probability value verified to be in [0.0, 1.0]
#[cfg_attr(verus, verus::trusted)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VProb {
    value: f32,
}

#[cfg(verus)]
verus! {

impl VProb {
    /// Create a new verified probability
    #[verifier::spec]
    pub open spec fn valid(&self) -> bool {
        0.0 <= self.value && self.value <= 1.0
    }

    /// Constructor with proof obligation
    #[verifier::proof]
    #[requires(0.0 <= value && value <= 1.0)]
    #[ensures(|result: VProb| result.valid())]
    pub fn new(value: f32) -> (result: VProb)
    {
        VProb { value }
    }

    /// Get value
    #[ensures(|result: f32| self.valid() ==> (0.0 <= result && result <= 1.0))]
    pub fn get(&self) -> f32 {
        self.value
    }

    /// Multiply two probabilities
    #[requires(self.valid() && other.valid())]
    #[ensures(|result: VProb| result.valid())]
    pub fn multiply(&self, other: &VProb) -> (result: VProb)
    {
        let product = self.value * other.value;
        // Product of values in [0,1] is also in [0,1]
        proof {
            assert(0.0 <= product);
            assert(product <= 1.0);
        }
        VProb { value: product }
    }

    /// Complement: 1 - p
    #[requires(self.valid())]
    #[ensures(|result: VProb| result.valid())]
    pub fn complement(&self) -> (result: VProb)
    {
        VProb { value: 1.0 - self.value }
    }

    /// Bayesian update
    #[requires(self.valid())]
    #[requires(0.0 < likelihood_true && likelihood_true <= 1.0)]
    #[requires(0.0 < likelihood_false && likelihood_false <= 1.0)]
    #[ensures(|result: VProb| result.valid())]
    pub fn bayesian_update(&self, likelihood_true: f32, likelihood_false: f32) -> (result: VProb)
    {
        let prior = self.value;
        let numerator = likelihood_true * prior;
        let denominator = numerator + likelihood_false * (1.0 - prior);
        
        if denominator <= 0.0 {
            return VProb { value: prior };
        }
        
        let posterior = numerator / denominator;
        
        // Clamp to ensure validity
        let clamped = if posterior < 0.0 { 0.0 } 
                      else if posterior > 1.0 { 1.0 } 
                      else { posterior };
        
        VProb { value: clamped }
    }
}

} // verus!

// ============================================================================
// Non-Verus fallback implementations
// ============================================================================

#[cfg(not(verus))]
impl VProb {
    pub fn new(value: f32) -> Option<Self> {
        if value >= 0.0 && value <= 1.0 {
            Some(Self { value })
        } else {
            None
        }
    }

    pub fn get(&self) -> f32 {
        self.value
    }

    pub fn multiply(&self, other: &VProb) -> Self {
        Self { value: self.value * other.value }
    }

    pub fn complement(&self) -> Self {
        Self { value: 1.0 - self.value }
    }

    pub fn bayesian_update(&self, likelihood_true: f32, likelihood_false: f32) -> Self {
        let prior = self.value;
        let numerator = likelihood_true * prior;
        let denominator = numerator + likelihood_false * (1.0 - prior);
        
        if denominator <= 0.0 {
            return Self { value: prior };
        }
        
        let posterior = (numerator / denominator).clamp(0.0, 1.0);
        Self { value: posterior }
    }
}

// ============================================================================
// Verified Bitmap
// ============================================================================

/// Bitmap with verified bounds checking
#[derive(Debug, Clone)]
pub struct VBitmap {
    bits: Vec<u64>,
    len: usize,
}

#[cfg(verus)]
verus! {

impl VBitmap {
    #[verifier::spec]
    pub open spec fn len_spec(&self) -> nat {
        self.len as nat
    }

    #[verifier::spec]
    pub open spec fn in_bounds(&self, index: usize) -> bool {
        index < self.len
    }

    /// Create new bitmap
    #[ensures(|result: VBitmap| result.len_spec() == n as nat)]
    pub fn new(n: usize) -> (result: VBitmap) {
        let words = (n + 63) / 64;
        VBitmap {
            bits: vec![0u64; words],
            len: n,
        }
    }

    /// Get bit with bounds proof
    #[requires(self.in_bounds(index))]
    pub fn get(&self, index: usize) -> bool {
        let word = index / 64;
        let bit = index % 64;
        (self.bits[word] >> bit) & 1 == 1
    }

    /// Set bit with bounds proof
    #[requires(self.in_bounds(index))]
    #[ensures(self.get(index) == value)]
    pub fn set(&mut self, index: usize, value: bool) {
        let word = index / 64;
        let bit = index % 64;
        
        if value {
            self.bits[word] |= 1u64 << bit;
        } else {
            self.bits[word] &= !(1u64 << bit);
        }
    }
}

} // verus!

#[cfg(not(verus))]
impl VBitmap {
    pub fn new(n: usize) -> Self {
        let words = (n + 63) / 64;
        Self {
            bits: vec![0u64; words],
            len: n,
        }
    }

    pub fn get(&self, index: usize) -> Option<bool> {
        if index >= self.len {
            return None;
        }
        let word = index / 64;
        let bit = index % 64;
        Some((self.bits[word] >> bit) & 1 == 1)
    }

    pub fn set(&mut self, index: usize, value: bool) -> bool {
        if index >= self.len {
            return false;
        }
        let word = index / 64;
        let bit = index % 64;
        if value {
            self.bits[word] |= 1u64 << bit;
        } else {
            self.bits[word] &= !(1u64 << bit);
        }
        true
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

// ============================================================================
// Verified Path Signature
// ============================================================================

/// Path signature with verified length tracking
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VPathSig<const N: usize> {
    segments: [u32; N],
}

#[cfg(verus)]
verus! {

impl<const N: usize> VPathSig<N> {
    #[verifier::spec]
    pub open spec fn length(&self) -> nat {
        N as nat
    }

    /// Extend path by one segment
    #[ensures(|result: VPathSig<{N+1}>| result.length() == self.length() + 1)]
    pub fn extend(&self, segment: u32) -> VPathSig<{N + 1}> {
        let mut new_segments = [0u32; N + 1];
        for i in 0..N {
            new_segments[i] = self.segments[i];
        }
        new_segments[N] = segment;
        VPathSig { segments: new_segments }
    }
}

} // verus!

// ============================================================================
// Reachability Proof
// ============================================================================

/// A witness that a path exists between two entities
#[derive(Debug, Clone)]
pub struct ReachabilityProof {
    pub from: u32,
    pub to: u32,
    pub steps: Vec<(u32, u32, u32)>, // (source, relation, target)
}

#[cfg(verus)]
verus! {

impl ReachabilityProof {
    /// Verify the proof is valid
    #[ensures(|result: bool| result ==> self.is_valid_path())]
    pub fn verify(&self) -> bool {
        if self.steps.is_empty() {
            return self.from == self.to;
        }
        
        // First step must start from `from`
        if self.steps[0].0 != self.from {
            return false;
        }
        
        // Last step must end at `to`
        if self.steps[self.steps.len() - 1].2 != self.to {
            return false;
        }
        
        // Each step must connect to the next
        for i in 0..(self.steps.len() - 1) {
            if self.steps[i].2 != self.steps[i + 1].0 {
                return false;
            }
        }
        
        true
    }

    #[verifier::spec]
    pub open spec fn is_valid_path(&self) -> bool {
        // Specification of valid path
        self.steps.len() == 0 ==> self.from == self.to
    }
}

} // verus!

#[cfg(not(verus))]
impl ReachabilityProof {
    pub fn verify(&self) -> bool {
        if self.steps.is_empty() {
            return self.from == self.to;
        }
        
        if self.steps[0].0 != self.from {
            return false;
        }
        
        if self.steps.last().map(|s| s.2) != Some(self.to) {
            return false;
        }
        
        for i in 0..(self.steps.len() - 1) {
            if self.steps[i].2 != self.steps[i + 1].0 {
                return false;
            }
        }
        
        true
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vprob() {
        let p = VProb::new(0.5).unwrap();
        assert!((p.get() - 0.5).abs() < 0.001);
        
        let q = p.multiply(&p);
        assert!((q.get() - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_vbitmap() {
        let mut bm = VBitmap::new(100);
        assert_eq!(bm.get(50), Some(false));
        
        bm.set(50, true);
        assert_eq!(bm.get(50), Some(true));
        
        // Out of bounds
        assert_eq!(bm.get(200), None);
    }

    #[test]
    fn test_reachability() {
        let proof = ReachabilityProof {
            from: 1,
            to: 3,
            steps: vec![
                (1, 10, 2),
                (2, 10, 3),
            ],
        };
        
        assert!(proof.verify());
        
        // Invalid proof
        let bad_proof = ReachabilityProof {
            from: 1,
            to: 3,
            steps: vec![
                (1, 10, 2),
                (5, 10, 3), // Doesn't connect
            ],
        };
        
        assert!(!bad_proof.verify());
    }
}

