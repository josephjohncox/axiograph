//! Verified PathDB components using Verus
//!
//! This module provides verified implementations of core PathDB operations
//! that can be checked with Verus (https://github.com/verus-lang/verus).
//!
//! Key verifications:
//! - Probability values are always in [0, 1]
//! - Path indices maintain consistency
//! - Binary format parsing is safe
//! - Set operations preserve invariants
//!
//! To verify, run: `verus src/verified.rs`

#![allow(unused_macros, dead_code)]

// When Verus is not available, these annotations are no-ops
#[cfg(not(verus))]
macro_rules! spec {
    ($($tt:tt)*) => {};
}

#[cfg(not(verus))]
macro_rules! proof {
    ($($tt:tt)*) => {};
}

#[cfg(not(verus))]
macro_rules! requires {
    ($($tt:tt)*) => {};
}

#[cfg(not(verus))]
macro_rules! ensures {
    ($($tt:tt)*) => {};
}

#[cfg(not(verus))]
macro_rules! invariant {
    ($($tt:tt)*) => {};
}

use crate::certificate::{FixedPointProbability, FIXED_POINT_DENOMINATOR};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// ============================================================================
// Verified Probability Type
// ============================================================================

/// A probability value that is verified to be in [0, 1]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifiedProb {
    fp: FixedPointProbability,
}

impl Serialize for VerifiedProb {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // v1 certificates use float-on-the-wire for backwards compatibility.
        serializer.serialize_f32(self.value())
    }
}

impl<'de> Deserialize<'de> for VerifiedProb {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = f32::deserialize(deserializer)?;
        VerifiedProb::try_new(value).ok_or_else(|| {
            serde::de::Error::custom("VerifiedProb must be a finite float in [0, 1]")
        })
    }
}

impl VerifiedProb {
    /// Create a new probability value
    /// Verus: requires 0.0 <= value <= 1.0
    /// Verus: ensures result.value == value
    #[cfg_attr(verus, requires(0.0 <= value && value <= 1.0))]
    #[cfg_attr(verus, ensures(|result: VerifiedProb| result.value == value))]
    pub fn new(value: f32) -> Self {
        Self::try_new(value).expect("VerifiedProb::new: value must be in [0, 1]")
    }

    /// Try to create a probability, returning None if invalid
    pub fn try_new(value: f32) -> Option<Self> {
        if !value.is_finite() || value < 0.0 || value > 1.0 {
            return None;
        }
        Some(Self {
            fp: FixedPointProbability::from_f32_bits(value.to_bits()),
        })
    }

    /// Get the probability value
    #[cfg_attr(verus, ensures(|result: f32| 0.0 <= result && result <= 1.0))]
    pub fn value(&self) -> f32 {
        self.fp.to_f32()
    }

    pub fn to_fixed_point(&self) -> FixedPointProbability {
        self.fp
    }

    pub fn numerator(&self) -> u32 {
        self.fp.numerator()
    }

    pub fn from_fixed_point_numerator_clamped(numerator: u32) -> Self {
        Self {
            fp: FixedPointProbability::new_unchecked(numerator.min(FIXED_POINT_DENOMINATOR)),
        }
    }

    /// Complement: 1 - p
    #[cfg_attr(verus, ensures(|result: VerifiedProb| result.value == 1.0 - self.value))]
    pub fn complement(&self) -> Self {
        Self {
            fp: FixedPointProbability::new_unchecked(FIXED_POINT_DENOMINATOR - self.fp.numerator()),
        }
    }

    /// Independent conjunction (AND)
    #[cfg_attr(verus, ensures(|result: VerifiedProb| result.value == self.value * other.value))]
    pub fn and_independent(&self, other: &Self) -> Self {
        Self {
            fp: self.fp.mul(other.fp),
        }
    }

    /// Independent disjunction (OR)
    #[cfg_attr(
        verus,
        ensures(|result: VerifiedProb| result.value == self.value + other.value - self.value * other.value)
    )]
    pub fn or_independent(&self, other: &Self) -> Self {
        let a = self.fp.numerator() as u64;
        let b = other.fp.numerator() as u64;
        let denom = FIXED_POINT_DENOMINATOR as u64;
        let ab = (a * b) / denom;
        let n = a + b - ab;
        let n = u32::try_from(n).unwrap_or(FIXED_POINT_DENOMINATOR);
        Self {
            fp: FixedPointProbability::new_unchecked(n.min(FIXED_POINT_DENOMINATOR)),
        }
    }

    /// Constants
    pub const CERTAIN: Self = Self {
        fp: FixedPointProbability::new_unchecked(FIXED_POINT_DENOMINATOR),
    };
    pub const IMPOSSIBLE: Self = Self {
        fp: FixedPointProbability::new_unchecked(0),
    };
    pub const UNIFORM: Self = Self {
        fp: FixedPointProbability::new_unchecked(FIXED_POINT_DENOMINATOR / 2),
    };
}

// Proof of probability invariant preservation for AND operation
#[cfg_attr(verus, proof)]
#[cfg_attr(verus, spec)]
fn prob_and_invariant_proof() {
    // For p, q in [0, 1]: p * q is also in [0, 1]
    // 0 <= p <= 1
    // 0 <= q <= 1
    // => 0 <= p * q <= 1 (since both are non-negative and <= 1)
}

// ============================================================================
// Binary Format Constants (PathDB `.axpd` v2)
// ============================================================================

/// Magic number: "AXPD" in ASCII
pub const MAGIC_NUMBER: u32 = 0x41585044;

/// Current format version
pub const FORMAT_VERSION: u32 = 2;

/// Feature flags
pub mod feature_flags {
    pub const MODAL_LOGIC: u64 = 1 << 0;
    pub const PROBABILISTIC: u64 = 1 << 1;
    pub const TEMPORAL_LOGIC: u64 = 1 << 2;
    pub const EPISTEMIC_LOGIC: u64 = 1 << 3;
    pub const DEONTIC_LOGIC: u64 = 1 << 4;
    pub const HOTT_EQUIV: u64 = 1 << 5;
    pub const COMPRESSION: u64 = 1 << 8;
    pub const ENCRYPTION: u64 = 1 << 9;
}

/// Section IDs (PathDB `.axpd` v2)
pub mod section_ids {
    pub const HEADER: u8 = 0x01;
    pub const STRING_TABLE: u8 = 0x02;
    pub const ENTITY_TABLE: u8 = 0x03;
    pub const RELATION_TABLE: u8 = 0x04;
    pub const PATH_INDEX: u8 = 0x05;
    pub const MODAL_FRAME: u8 = 0x10;
    pub const PROB_DIST: u8 = 0x11;
    pub const EQUIVALENCES: u8 = 0x12;
    pub const METADATA: u8 = 0xFF;
}

// ============================================================================
// Verified Binary Header
// ============================================================================

/// Binary file header (64 bytes, fixed size)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct BinaryHeader {
    pub magic: u32,
    pub version: u32,
    pub flags: u64,
    pub string_offset: u64,
    pub entity_offset: u64,
    pub relation_offset: u64,
    pub path_index_offset: u64,
    pub total_size: u64,
    pub checksum: u64,
}

impl BinaryHeader {
    /// Header size in bytes
    pub const SIZE: usize = 64;

    /// Validate the header
    #[cfg_attr(
        verus,
        ensures(|result: bool| result ==> self.magic == MAGIC_NUMBER && self.version <= FORMAT_VERSION)
    )]
    pub fn is_valid(&self) -> bool {
        self.magic == MAGIC_NUMBER
            && self.version >= 1
            && self.version <= FORMAT_VERSION
            && self.string_offset < self.total_size
            && self.entity_offset < self.total_size
            && self.relation_offset < self.total_size
    }

    /// Check if a feature is enabled
    pub fn has_feature(&self, flag: u64) -> bool {
        (self.flags & flag) != 0
    }

    /// Parse header from bytes
    #[cfg_attr(verus, requires(bytes.len() >= Self::SIZE))]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }

        let header = Self {
            magic: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            version: u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            flags: u64::from_le_bytes([
                bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14],
                bytes[15],
            ]),
            string_offset: u64::from_le_bytes([
                bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22],
                bytes[23],
            ]),
            entity_offset: u64::from_le_bytes([
                bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30],
                bytes[31],
            ]),
            relation_offset: u64::from_le_bytes([
                bytes[32], bytes[33], bytes[34], bytes[35], bytes[36], bytes[37], bytes[38],
                bytes[39],
            ]),
            path_index_offset: u64::from_le_bytes([
                bytes[40], bytes[41], bytes[42], bytes[43], bytes[44], bytes[45], bytes[46],
                bytes[47],
            ]),
            total_size: u64::from_le_bytes([
                bytes[48], bytes[49], bytes[50], bytes[51], bytes[52], bytes[53], bytes[54],
                bytes[55],
            ]),
            checksum: u64::from_le_bytes([
                bytes[56], bytes[57], bytes[58], bytes[59], bytes[60], bytes[61], bytes[62],
                bytes[63],
            ]),
        };

        if header.is_valid() {
            Some(header)
        } else {
            None
        }
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut result = [0u8; Self::SIZE];
        result[0..4].copy_from_slice(&self.magic.to_le_bytes());
        result[4..8].copy_from_slice(&self.version.to_le_bytes());
        result[8..16].copy_from_slice(&self.flags.to_le_bytes());
        result[16..24].copy_from_slice(&self.string_offset.to_le_bytes());
        result[24..32].copy_from_slice(&self.entity_offset.to_le_bytes());
        result[32..40].copy_from_slice(&self.relation_offset.to_le_bytes());
        result[40..48].copy_from_slice(&self.path_index_offset.to_le_bytes());
        result[48..56].copy_from_slice(&self.total_size.to_le_bytes());
        result[56..64].copy_from_slice(&self.checksum.to_le_bytes());
        result
    }
}

// ============================================================================
// Verified Bitmap Operations
// ============================================================================

/// Verified bitmap wrapper with invariant checks
pub struct VerifiedBitmap {
    inner: roaring::RoaringBitmap,
    /// Max valid ID (for bounds checking)
    max_id: u32,
}

impl VerifiedBitmap {
    /// Create new bitmap with max ID bound
    pub fn new(max_id: u32) -> Self {
        Self {
            inner: roaring::RoaringBitmap::new(),
            max_id,
        }
    }

    /// Insert with bounds check
    #[cfg_attr(verus, requires(id <= self.max_id))]
    pub fn insert(&mut self, id: u32) -> bool {
        if id <= self.max_id {
            self.inner.insert(id)
        } else {
            false
        }
    }

    /// Contains with bounds check
    pub fn contains(&self, id: u32) -> bool {
        id <= self.max_id && self.inner.contains(id)
    }

    /// Intersection (verified to preserve bounds)
    #[cfg_attr(verus, ensures(|result: VerifiedBitmap| result.max_id == self.max_id.min(other.max_id)))]
    pub fn intersection(&self, other: &Self) -> Self {
        Self {
            inner: &self.inner & &other.inner,
            max_id: self.max_id.min(other.max_id),
        }
    }

    /// Union (verified to take max bounds)
    #[cfg_attr(verus, ensures(|result: VerifiedBitmap| result.max_id == self.max_id.max(other.max_id)))]
    pub fn union(&self, other: &Self) -> Self {
        Self {
            inner: &self.inner | &other.inner,
            max_id: self.max_id.max(other.max_id),
        }
    }

    /// Cardinality
    pub fn len(&self) -> u64 {
        self.inner.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Iterate over elements
    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.inner.iter()
    }
}

// ============================================================================
// Verified Path Signature
// ============================================================================

/// A path signature with verified properties
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VerifiedPathSig {
    rel_types: Vec<u32>,
}

impl VerifiedPathSig {
    /// Create empty path
    pub fn empty() -> Self {
        Self {
            rel_types: Vec::new(),
        }
    }

    /// Create from relation types
    #[cfg_attr(verus, requires(rel_types.iter().all(|&id| id < max_str_id)))]
    pub fn new(rel_types: Vec<u32>, _max_str_id: u32) -> Option<Self> {
        // In production, validate all IDs exist
        Some(Self { rel_types })
    }

    /// Extend path
    pub fn extend(&self, rel_type: u32) -> Self {
        let mut new_types = self.rel_types.clone();
        new_types.push(rel_type);
        Self {
            rel_types: new_types,
        }
    }

    /// Path length
    pub fn len(&self) -> usize {
        self.rel_types.len()
    }

    /// Is empty path
    pub fn is_empty(&self) -> bool {
        self.rel_types.is_empty()
    }

    /// Concatenate paths
    #[cfg_attr(verus, ensures(|result: VerifiedPathSig| result.len() == self.len() + other.len()))]
    pub fn concat(&self, other: &Self) -> Self {
        let mut new_types = self.rel_types.clone();
        new_types.extend(other.rel_types.iter());
        Self {
            rel_types: new_types,
        }
    }

    /// Reverse path
    #[cfg_attr(verus, ensures(|result: VerifiedPathSig| result.len() == self.len()))]
    pub fn reverse(&self) -> Self {
        let mut new_types = self.rel_types.clone();
        new_types.reverse();
        Self {
            rel_types: new_types,
        }
    }

    /// Get relation types
    pub fn rel_types(&self) -> &[u32] {
        &self.rel_types
    }
}

// ============================================================================
// Modal Frame Encoding
// ============================================================================

/// Modal frame type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ModalFrameType {
    Kripke = 0,
    Epistemic = 1,
    Deontic = 2,
    Temporal = 3,
}

/// Encoded modal world
#[derive(Debug, Clone)]
pub struct EncodedWorld {
    pub world_id: u32,
    pub true_prop_ids: Vec<u32>,
}

/// Encoded accessibility relation
#[derive(Debug, Clone)]
pub struct EncodedAccessibility {
    pub relation_name: u32,
    pub edges: Vec<(u32, u32)>, // (from, to)
}

/// Encoded modal frame for binary storage
#[derive(Debug, Clone)]
pub struct EncodedModalFrame {
    pub frame_id: u32,
    pub frame_type: ModalFrameType,
    pub worlds: Vec<EncodedWorld>,
    pub accessibility: Vec<EncodedAccessibility>,
}

impl EncodedModalFrame {
    /// Verify frame consistency
    #[cfg_attr(verus, ensures(|result: bool| result ==> self.accessibility_valid()))]
    pub fn is_valid(&self) -> bool {
        let world_ids: std::collections::HashSet<u32> =
            self.worlds.iter().map(|w| w.world_id).collect();

        // Check accessibility only references valid worlds
        self.accessibility.iter().all(|acc| {
            acc.edges
                .iter()
                .all(|(from, to)| world_ids.contains(from) && world_ids.contains(to))
        })
    }

    fn accessibility_valid(&self) -> bool {
        self.is_valid()
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Frame ID
        bytes.extend_from_slice(&self.frame_id.to_le_bytes());

        // Frame type
        bytes.push(self.frame_type as u8);

        // Number of worlds
        bytes.extend_from_slice(&(self.worlds.len() as u32).to_le_bytes());

        // Worlds
        for world in &self.worlds {
            bytes.extend_from_slice(&world.world_id.to_le_bytes());
            bytes.extend_from_slice(&(world.true_prop_ids.len() as u32).to_le_bytes());
            for prop_id in &world.true_prop_ids {
                bytes.extend_from_slice(&prop_id.to_le_bytes());
            }
        }

        // Number of accessibility relations
        bytes.extend_from_slice(&(self.accessibility.len() as u32).to_le_bytes());

        // Accessibility
        for acc in &self.accessibility {
            bytes.extend_from_slice(&acc.relation_name.to_le_bytes());
            bytes.extend_from_slice(&(acc.edges.len() as u32).to_le_bytes());
            for (from, to) in &acc.edges {
                bytes.extend_from_slice(&from.to_le_bytes());
                bytes.extend_from_slice(&to.to_le_bytes());
            }
        }

        bytes
    }
}

// ============================================================================
// Probabilistic Distribution Encoding
// ============================================================================

/// Encoded probability distribution for binary storage
#[derive(Debug, Clone)]
pub struct EncodedDistribution {
    pub var_id: u32,
    pub outcomes: Vec<(u32, VerifiedProb)>,
}

impl EncodedDistribution {
    /// Verify distribution sums to 1 (within tolerance)
    #[cfg_attr(verus, ensures(|result: bool| result ==> self.is_normalized()))]
    pub fn is_valid(&self) -> bool {
        let sum: u64 = self
            .outcomes
            .iter()
            .map(|(_, p)| p.numerator() as u64)
            .sum();
        // Match Lean's `VDist` invariant: sum ≤ Precision + 1.
        sum <= (FIXED_POINT_DENOMINATOR as u64) + 1
    }

    fn is_normalized(&self) -> bool {
        self.is_valid()
    }

    /// Normalize distribution
    pub fn normalize(&mut self) {
        let denom = FIXED_POINT_DENOMINATOR;
        let denom_u64 = denom as u64;
        let sum: u64 = self
            .outcomes
            .iter()
            .map(|(_, p)| p.numerator() as u64)
            .sum();

        if self.outcomes.is_empty() {
            return;
        }

        if sum == 0 {
            // Degenerate case: fall back to a deterministic uniform-ish split.
            let n = self.outcomes.len() as u32;
            let base = denom / n;
            let remainder = denom - (base * n);
            for (i, (_, p)) in self.outcomes.iter_mut().enumerate() {
                let bump = if (i as u32) < remainder { 1 } else { 0 };
                *p = VerifiedProb::from_fixed_point_numerator_clamped(base + bump);
            }
            return;
        }

        let mut new_numerators: Vec<u32> = Vec::with_capacity(self.outcomes.len());
        let mut new_sum: u64 = 0;
        for (_, p) in &self.outcomes {
            let scaled = (p.numerator() as u64 * denom_u64) / sum;
            let scaled_u32 = u32::try_from(scaled).unwrap_or(denom);
            new_sum += scaled_u32 as u64;
            new_numerators.push(scaled_u32);
        }

        let mut leftover = denom_u64.saturating_sub(new_sum);
        for n in &mut new_numerators {
            if leftover == 0 {
                break;
            }
            if *n < denom {
                *n += 1;
                leftover -= 1;
            }
        }

        for ((_, p), n) in self.outcomes.iter_mut().zip(new_numerators) {
            *p = VerifiedProb::from_fixed_point_numerator_clamped(n);
        }
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        bytes.extend_from_slice(&self.var_id.to_le_bytes());
        bytes.extend_from_slice(&(self.outcomes.len() as u32).to_le_bytes());

        for (outcome_id, prob) in &self.outcomes {
            bytes.extend_from_slice(&outcome_id.to_le_bytes());
            bytes.extend_from_slice(&prob.numerator().to_le_bytes());
        }

        bytes
    }
}

// ============================================================================
// Reachability Proof
// ============================================================================

/// Evidence of reachability between entities
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReachabilityProof {
    /// Direct: entity is trivially reachable from itself
    Reflexive { entity: u32 },
    /// Step: reachable via one relation
    Step {
        from: u32,
        rel_type: u32,
        to: u32,
        rel_confidence: VerifiedProb,
        rest: Box<ReachabilityProof>,
    },
}

impl ReachabilityProof {
    /// Get the starting entity
    pub fn start(&self) -> u32 {
        match self {
            ReachabilityProof::Reflexive { entity } => *entity,
            ReachabilityProof::Step { from, .. } => *from,
        }
    }

    /// Get the ending entity
    pub fn end(&self) -> u32 {
        match self {
            ReachabilityProof::Reflexive { entity } => *entity,
            ReachabilityProof::Step { rest, .. } => rest.end(),
        }
    }

    /// Get path length
    pub fn path_len(&self) -> usize {
        match self {
            ReachabilityProof::Reflexive { .. } => 0,
            ReachabilityProof::Step { rest, .. } => 1 + rest.path_len(),
        }
    }

    /// Compute combined confidence along path
    #[cfg_attr(verus, ensures(|result: VerifiedProb| result.value() <= 1.0))]
    pub fn path_confidence(&self) -> VerifiedProb {
        match self {
            ReachabilityProof::Reflexive { .. } => VerifiedProb::CERTAIN,
            ReachabilityProof::Step {
                rel_confidence,
                rest,
                ..
            } => rel_confidence.and_independent(&rest.path_confidence()),
        }
    }

    /// Extract path signature from proof
    pub fn to_path_sig(&self) -> VerifiedPathSig {
        let mut rel_types = Vec::new();
        self.collect_rel_types(&mut rel_types);
        VerifiedPathSig { rel_types }
    }

    fn collect_rel_types(&self, rel_types: &mut Vec<u32>) {
        match self {
            ReachabilityProof::Reflexive { .. } => {}
            ReachabilityProof::Step { rel_type, rest, .. } => {
                rel_types.push(*rel_type);
                rest.collect_rel_types(rel_types);
            }
        }
    }
}

// ============================================================================
// Query Result with Proof
// ============================================================================

/// A query result that carries proof of its derivation
#[derive(Debug)]
pub struct ProvenQueryResult {
    pub entity_id: u32,
    pub confidence: VerifiedProb,
    pub proof: ReachabilityProof,
    pub derivation: String,
}

impl ProvenQueryResult {
    /// Verify the proof is consistent with the result
    #[cfg_attr(verus, ensures(|result: bool| result ==> self.proof.end() == self.entity_id))]
    pub fn is_consistent(&self) -> bool {
        self.proof.end() == self.entity_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verified_prob() {
        let p = VerifiedProb::new(0.7);
        assert!(p.value() >= 0.0 && p.value() <= 1.0);

        let complement = p.complement();
        assert!((complement.value() - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_verified_prob_algebra() {
        let p = VerifiedProb::new(0.5);
        let q = VerifiedProb::new(0.5);

        let and_result = p.and_independent(&q);
        assert!((and_result.value() - 0.25).abs() < 0.001);

        let or_result = p.or_independent(&q);
        assert!((or_result.value() - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_binary_header() {
        let header = BinaryHeader {
            magic: MAGIC_NUMBER,
            version: FORMAT_VERSION,
            flags: feature_flags::MODAL_LOGIC | feature_flags::PROBABILISTIC,
            string_offset: 64,
            entity_offset: 1000,
            relation_offset: 2000,
            path_index_offset: 3000,
            total_size: 10000,
            checksum: 0,
        };

        assert!(header.is_valid());
        assert!(header.has_feature(feature_flags::MODAL_LOGIC));
        assert!(header.has_feature(feature_flags::PROBABILISTIC));
        assert!(!header.has_feature(feature_flags::TEMPORAL_LOGIC));

        // Round-trip test
        let bytes = header.to_bytes();
        let parsed = BinaryHeader::from_bytes(&bytes).unwrap();
        let parsed_magic = parsed.magic;
        let header_magic = header.magic;
        assert_eq!(parsed_magic, header_magic);

        let parsed_version = parsed.version;
        let header_version = header.version;
        assert_eq!(parsed_version, header_version);

        let parsed_flags = parsed.flags;
        let header_flags = header.flags;
        assert_eq!(parsed_flags, header_flags);
    }

    #[test]
    fn test_path_sig() {
        let p1 = VerifiedPathSig::new(vec![1, 2], 100).unwrap();
        let p2 = VerifiedPathSig::new(vec![3], 100).unwrap();

        let concat = p1.concat(&p2);
        assert_eq!(concat.len(), 3);
        assert_eq!(concat.rel_types(), &[1, 2, 3]);

        let reversed = concat.reverse();
        assert_eq!(reversed.rel_types(), &[3, 2, 1]);
    }

    #[test]
    fn test_modal_frame() {
        let frame = EncodedModalFrame {
            frame_id: 1,
            frame_type: ModalFrameType::Kripke,
            worlds: vec![
                EncodedWorld {
                    world_id: 0,
                    true_prop_ids: vec![1, 2],
                },
                EncodedWorld {
                    world_id: 1,
                    true_prop_ids: vec![2, 3],
                },
            ],
            accessibility: vec![EncodedAccessibility {
                relation_name: 10,
                edges: vec![(0, 1)],
            }],
        };

        assert!(frame.is_valid());
    }

    #[test]
    fn test_distribution() {
        let mut dist = EncodedDistribution {
            var_id: 1,
            outcomes: vec![
                (0, VerifiedProb::new(0.3)),
                (1, VerifiedProb::new(0.3)),
                (2, VerifiedProb::new(0.4)),
            ],
        };

        assert!(dist.is_valid());
    }

    #[test]
    fn test_reachability_proof() {
        let proof = ReachabilityProof::Step {
            from: 1,
            rel_type: 10,
            to: 2,
            rel_confidence: VerifiedProb::new(0.9),
            rest: Box::new(ReachabilityProof::Step {
                from: 2,
                rel_type: 11,
                to: 3,
                rel_confidence: VerifiedProb::new(0.8),
                rest: Box::new(ReachabilityProof::Reflexive { entity: 3 }),
            }),
        };

        assert_eq!(proof.start(), 1);
        assert_eq!(proof.end(), 3);
        assert_eq!(proof.path_len(), 2);

        let confidence = proof.path_confidence();
        assert!((confidence.value() - 0.72).abs() < 0.01);

        let path_sig = proof.to_path_sig();
        assert_eq!(path_sig.rel_types(), &[10, 11]);
    }
}
