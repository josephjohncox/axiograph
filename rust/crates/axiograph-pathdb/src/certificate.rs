//! Certificate formats for proof-carrying results.
//!
//! This module defines a minimal, versioned JSON shape intended to be consumed
//! by a trusted checker (Lean during migration).

use crate::migration::DeltaFMigrationProofV1;
use crate::{AnswerIdV2, AxiDigest, CertificateIdV2, QueryIdV2, RevisionDigestV2};
use axiograph_dsl::schema_v1::PathExprV3 as AxiPathExprV3;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const CERTIFICATE_VERSION_V2: u32 = 2;
pub const CERTIFICATE_VERSION_V3: u32 = 3;
pub const PREPARED_QUERY_BINDING_VERSION_V1: u32 = 1;

/// Fixed-point denominator shared with the Lean checker (`Axiograph.Prob.Precision`).
pub const FIXED_POINT_DENOMINATOR: u32 = 1_000_000;
/// Explicit finite bound for replayable path-normalization traces.
pub const MAX_PATH_REWRITE_STEPS_V2: usize = 50_000;

// ============================================================================
// Certificate v2+: fixed-point probabilities and anchored typed witnesses.
// ============================================================================

/// Fixed-point probability numerator in `[0, FIXED_POINT_DENOMINATOR]`.
///
/// Serialized as a bare `u32` (JSON number) for stable interchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedPointProbability {
    numerator: u32,
}

/// Short alias kept for convenience in Rust code.
pub type FixedProb = FixedPointProbability;

/// Lean-compatible name: `Axiograph.Prob.VProb` is this same fixed-point shape.
pub type VProb = FixedPointProbability;

impl Serialize for FixedPointProbability {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u32(self.numerator)
    }
}

impl<'de> Deserialize<'de> for FixedPointProbability {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let numerator = u32::deserialize(deserializer)?;
        FixedPointProbability::try_new(numerator).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "FixedProb numerator must be ≤ {FIXED_POINT_DENOMINATOR}"
            ))
        })
    }
}

impl FixedPointProbability {
    pub(crate) const fn new_unchecked(numerator: u32) -> Self {
        Self { numerator }
    }

    pub fn try_new(numerator: u32) -> Option<Self> {
        (numerator <= FIXED_POINT_DENOMINATOR).then_some(Self { numerator })
    }

    /// Deterministically convert an IEEE754 binary32 probability to fixed-point.
    ///
    /// This is defined in terms of the **exact f32 bits** (not float arithmetic),
    /// so Rust and Lean can agree on the mapping.
    ///
    /// Semantics (for finite, non-negative inputs):
    /// - clamp to `[0, 1]`,
    /// - compute `round(p * FIXED_POINT_DENOMINATOR)` with ties rounded up.
    pub fn from_f32_bits(bits: u32) -> Self {
        fn round_div_pow2(n: u128, k: u32) -> u128 {
            match k {
                0 => n,
                _ if k >= 128 => 0,
                _ => (n + (1u128 << (k - 1))) >> k,
            }
        }

        let sign = bits >> 31;
        let exp = (bits >> 23) & 0xff;
        let frac = bits & 0x7fffff;

        // Clamp negatives to 0.
        if sign != 0 {
            return Self { numerator: 0 };
        }

        // Clamp infinities; treat NaNs as 0 (should never appear in checked facts).
        if exp == 255 {
            return if frac == 0 {
                Self {
                    numerator: FIXED_POINT_DENOMINATOR,
                }
            } else {
                Self { numerator: 0 }
            };
        }

        let scaled: u128 = if exp == 0 {
            // Subnormal (or zero): value = frac * 2^(-149)
            round_div_pow2((frac as u128) * (FIXED_POINT_DENOMINATOR as u128), 149)
        } else if exp >= 127 {
            // `exp = 127` with `frac = 0` is exactly 1.0; anything larger clamps to 1.
            FIXED_POINT_DENOMINATOR as u128
        } else {
            // Normal: value = (2^23 + frac) * 2^(exp - 150)
            let mantissa = ((1u32 << 23) + frac) as u128;
            let k = 150 - exp;
            round_div_pow2(mantissa * (FIXED_POINT_DENOMINATOR as u128), k)
        };

        let scaled = scaled.min(FIXED_POINT_DENOMINATOR as u128) as u32;
        Self { numerator: scaled }
    }

    /// Convert an `f32` probability to fixed-point using `from_f32_bits`.
    pub fn from_f32(p: f32) -> Self {
        Self::from_f32_bits(p.to_bits())
    }

    pub fn numerator(&self) -> u32 {
        self.numerator
    }

    pub fn to_f32(&self) -> f32 {
        (self.numerator as f32) / (FIXED_POINT_DENOMINATOR as f32)
    }

    /// Fixed-point multiplication with rounding down.
    #[allow(clippy::should_implement_trait)]
    pub fn mul(self, other: Self) -> Self {
        let scaled =
            ((self.numerator as u64) * (other.numerator as u64)) / (FIXED_POINT_DENOMINATOR as u64);
        let scaled = u32::try_from(scaled).unwrap_or(FIXED_POINT_DENOMINATOR);
        Self {
            numerator: scaled.min(FIXED_POINT_DENOMINATOR),
        }
    }
}

#[cfg(kani)]
#[kani::proof]
fn kani_fixed_point_constructor_enforces_bounds() {
    let numerator: u32 = kani::any();
    match FixedPointProbability::try_new(numerator) {
        Some(probability) => {
            assert!(numerator <= FIXED_POINT_DENOMINATOR);
            assert_eq!(probability.numerator(), numerator);
        }
        None => assert!(numerator > FIXED_POINT_DENOMINATOR),
    }
}

/// Versioned wrapper for v2 certificates (fixed-point probabilities).
#[derive(Debug, Clone, Serialize)]
pub struct CertificateV2 {
    pub version: u32,
    /// Optional binding to canonical `.axi` inputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<AxiAnchorV1>,
    #[serde(flatten)]
    pub payload: CertificatePayloadV2,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CertificateV2Wire {
    version: u32,
    #[serde(default)]
    anchor: Option<AxiAnchorV1>,
    kind: String,
    proof: serde_json::Value,
}

impl<'de> Deserialize<'de> for CertificateV2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let CertificateV2Wire {
            version,
            anchor,
            kind,
            proof,
        } = CertificateV2Wire::deserialize(deserializer)?;
        if version != CERTIFICATE_VERSION_V2 {
            return Err(serde::de::Error::custom(format!(
                "unsupported CertificateV2 version {version}, expected {CERTIFICATE_VERSION_V2}"
            )));
        }
        let payload = match kind.as_str() {
            "axi_well_typed_v1" => CertificatePayloadV2::AxiWellTypedV1 {
                proof: serde_json::from_value(proof).map_err(serde::de::Error::custom)?,
            },
            "axi_constraints_ok_v1" => CertificatePayloadV2::AxiConstraintsOkV1 {
                proof: serde_json::from_value(proof).map_err(serde::de::Error::custom)?,
            },
            "reachability_v3" => CertificatePayloadV2::ReachabilityV3 {
                proof: serde_json::from_value(proof).map_err(serde::de::Error::custom)?,
            },
            "resolution_v2" => CertificatePayloadV2::ResolutionV2 {
                proof: serde_json::from_value(proof).map_err(serde::de::Error::custom)?,
            },
            "normalize_path_v2" => CertificatePayloadV2::NormalizePathV2 {
                proof: serde_json::from_value(proof).map_err(serde::de::Error::custom)?,
            },
            "rewrite_derivation_v3" => CertificatePayloadV2::RewriteDerivationV3 {
                proof: serde_json::from_value(proof).map_err(serde::de::Error::custom)?,
            },
            "path_equiv_v2" => CertificatePayloadV2::PathEquivV2 {
                proof: serde_json::from_value(proof).map_err(serde::de::Error::custom)?,
            },
            "delta_f_v1" => CertificatePayloadV2::DeltaFMigrationV1 {
                proof: serde_json::from_value(proof).map_err(serde::de::Error::custom)?,
            },
            _ => {
                return Err(serde::de::Error::custom(format!(
                    "unknown CertificateV2 kind `{kind}`"
                )));
            }
        };
        Ok(Self {
            version,
            anchor,
            payload,
        })
    }
}

/// Certificate anchor for canonical `.axi` inputs (v1).
///
/// This is intentionally minimal at first: a stable module digest.
///
/// Revision identity format (shared with Lean `Axiograph.Identity`):
/// - `revision_digest_v2 = "axi:revision:v2:sha256:<64 lowercase hex digits>"`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AxiAnchorV1 {
    pub revision_digest_v2: AxiDigest,
}

impl AxiAnchorV1 {
    pub fn new(revision_digest_v2: impl Into<AxiDigest>) -> Self {
        Self {
            revision_digest_v2: revision_digest_v2.into(),
        }
    }
}

/// Certificate proof: canonical `.axi` module well-typedness (v1).
///
/// This is intentionally a *small decision procedure* that can be re-run in the
/// trusted checker (Lean). The proof payload is a lightweight summary intended
/// to:
/// - make debugging easier (counts), and
/// - keep Rust/Lean typecheckers in lockstep.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AxiWellTypedProofV1 {
    pub module_name: String,
    pub schema_count: u32,
    pub theory_count: u32,
    pub instance_count: u32,
    pub assignment_count: u32,
    pub tuple_count: u32,
}

/// Certificate proof: canonical `.axi` module core-constraint satisfaction (v1).
///
/// This is an intentionally small, re-checkable gate:
/// - Rust claims the module satisfies a conservative subset of theory constraints, and
/// - Lean re-parses the anchored `.axi` module and re-checks the same subset.
///
/// Certified subset (high ROI, low ambiguity):
/// - `constraint key Rel(...)`
/// - `constraint functional Rel.field -> Rel.field`
/// - `constraint at_most N Rel.field -> Rel.field [param (...)]`
/// - `constraint symmetric Rel`
/// - `constraint symmetric Rel where Rel.field in {A, B, ...}`
/// - `constraint transitive Rel` (certified transitive-closure checks for keys/functionals on carrier fields)
/// - `constraint typing Rel: rule_name` (small builtin rule set; see docs)
///
/// We intentionally do **not** certify global entailment/inference or full
/// relational-algebra semantics in this first pass. The goal is auditable
/// structural sanity checks for canonical snapshots/modules that are useful for
/// query planning and data hygiene under an open-world reading.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AxiConstraintsOkProofV1 {
    pub module_name: String,
    /// Number of constraints checked (theory-local count, within the certified subset).
    pub constraint_count: u32,
    /// Number of instances checked (by schema match).
    pub instance_count: u32,
    /// Number of (constraint × instance) checks performed.
    pub check_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum CertificatePayloadV2 {
    #[serde(rename = "axi_well_typed_v1")]
    AxiWellTypedV1 {
        proof: AxiWellTypedProofV1,
    },
    #[serde(rename = "axi_constraints_ok_v1")]
    AxiConstraintsOkV1 {
        proof: AxiConstraintsOkProofV1,
    },
    #[serde(rename = "reachability_v3")]
    ReachabilityV3 {
        proof: ReachabilityProofV3,
    },
    ResolutionV2 {
        proof: ResolutionProofV2,
    },
    NormalizePathV2 {
        proof: NormalizePathProofV2,
    },
    RewriteDerivationV3 {
        proof: RewriteDerivationProofV3,
    },
    PathEquivV2 {
        proof: PathEquivProofV2,
    },
    #[serde(rename = "delta_f_v1")]
    DeltaFMigrationV1 {
        proof: DeltaFMigrationProofV1,
    },
}

impl CertificateV2 {
    pub fn axi_well_typed_v1(proof: AxiWellTypedProofV1) -> Self {
        Self {
            version: CERTIFICATE_VERSION_V2,
            anchor: None,
            payload: CertificatePayloadV2::AxiWellTypedV1 { proof },
        }
    }

    pub fn axi_constraints_ok_v1(proof: AxiConstraintsOkProofV1) -> Self {
        Self {
            version: CERTIFICATE_VERSION_V2,
            anchor: None,
            payload: CertificatePayloadV2::AxiConstraintsOkV1 { proof },
        }
    }

    pub fn reachability_v3(proof: ReachabilityProofV3) -> Self {
        Self {
            version: CERTIFICATE_VERSION_V2,
            anchor: None,
            payload: CertificatePayloadV2::ReachabilityV3 { proof },
        }
    }

    pub fn resolution(proof: ResolutionProofV2) -> Self {
        Self {
            version: CERTIFICATE_VERSION_V2,
            anchor: None,
            payload: CertificatePayloadV2::ResolutionV2 { proof },
        }
    }

    pub fn normalize_path(proof: NormalizePathProofV2) -> Self {
        Self {
            version: CERTIFICATE_VERSION_V2,
            anchor: None,
            payload: CertificatePayloadV2::NormalizePathV2 { proof },
        }
    }

    pub fn rewrite_derivation_v3(proof: RewriteDerivationProofV3) -> Self {
        Self {
            version: CERTIFICATE_VERSION_V2,
            anchor: None,
            payload: CertificatePayloadV2::RewriteDerivationV3 { proof },
        }
    }

    pub fn path_equiv(proof: PathEquivProofV2) -> Self {
        Self {
            version: CERTIFICATE_VERSION_V2,
            anchor: None,
            payload: CertificatePayloadV2::PathEquivV2 { proof },
        }
    }

    pub fn delta_f_v1(proof: DeltaFMigrationProofV1) -> Self {
        Self {
            version: CERTIFICATE_VERSION_V2,
            anchor: None,
            payload: CertificatePayloadV2::DeltaFMigrationV1 { proof },
        }
    }

    pub fn with_anchor(mut self, anchor: AxiAnchorV1) -> Self {
        self.anchor = Some(anchor);
        self
    }
}

// ============================================================================
// Additional v2 proof kinds (beyond reachability)
// ============================================================================

/// Conflict-resolution decision mirroring `Axiograph.Prob.decideResolution` (Lean side).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "tag", rename_all = "snake_case")]
pub enum ResolutionDecisionV2 {
    ChooseFirst,
    ChooseSecond,
    Merge {
        w1_fp: FixedPointProbability,
        w2_fp: FixedPointProbability,
    },
    NeedReview,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolutionProofV2 {
    pub first_confidence_fp: FixedPointProbability,
    pub second_confidence_fp: FixedPointProbability,
    pub threshold_fp: FixedPointProbability,
    pub decision: ResolutionDecisionV2,
}

impl ResolutionProofV2 {
    pub fn decide(
        first_confidence_fp: FixedPointProbability,
        second_confidence_fp: FixedPointProbability,
        threshold_fp: FixedPointProbability,
    ) -> Self {
        let decision =
            decide_resolution_v2(first_confidence_fp, second_confidence_fp, threshold_fp);
        Self {
            first_confidence_fp,
            second_confidence_fp,
            threshold_fp,
            decision,
        }
    }
}

fn decide_resolution_v2(
    first_confidence_fp: FixedPointProbability,
    second_confidence_fp: FixedPointProbability,
    threshold_fp: FixedPointProbability,
) -> ResolutionDecisionV2 {
    let n1 = first_confidence_fp.numerator();
    let n2 = second_confidence_fp.numerator();
    let gap = n1.abs_diff(n2);
    let thresh = threshold_fp.numerator();

    if gap >= thresh {
        if n1 >= n2 {
            ResolutionDecisionV2::ChooseFirst
        } else {
            ResolutionDecisionV2::ChooseSecond
        }
    } else if gap >= (thresh / 2) {
        ResolutionDecisionV2::Merge {
            w1_fp: first_confidence_fp,
            w2_fp: second_confidence_fp,
        }
    } else {
        ResolutionDecisionV2::NeedReview
    }
}

/// Path expression used for normalization certificates.
///
/// This mirrors the HoTT-style constructors in the Lean checker (`Axiograph.HoTT.*`),
/// but keeps the certificate payload independent of any particular graph representation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PathExprV2 {
    Reflexive {
        entity: u32,
    },
    Step {
        from: u32,
        rel_type: u32,
        to: u32,
    },
    Trans {
        left: Box<PathExprV2>,
        right: Box<PathExprV2>,
    },
    Inv {
        path: Box<PathExprV2>,
    },
}

/// Local rewrite rules for path normalization proofs (v2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PathRewriteRuleV2 {
    AssocRight,
    IdLeft,
    IdRight,
    InvRefl,
    InvInv,
    InvTrans,
    CancelHead,
}

/// A single rewrite step applied at a position in the AST.
///
/// Positions are a path from the root:
/// - `0` = `.trans.left`
/// - `1` = `.trans.right`
/// - `2` = `.inv.path`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PathRewriteStepV2 {
    pub pos: Vec<u32>,
    pub rule: PathRewriteRuleV2,
}

/// A v3 rewrite step referencing either a builtin rule or an `.axi` rule.
///
/// `rule_ref` formats:
/// - `builtin:<tag>` where `<tag>` is e.g. `id_left`,
/// - `axi:<axi_digest_v1>:<theory_name>:<rule_name>`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathRewriteStepV3 {
    pub pos: Vec<u32>,
    pub rule_ref: String,
}

/// Replayable rewrite derivation with first-class rule references (v3).
///
/// This is the only standalone generic rewrite certificate. It references
/// accepted `.axi` rules while the V2 builtin enum remains internal to path
/// normalization/equivalence replay.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RewriteDerivationProofV3 {
    pub input: AxiPathExprV3,
    pub output: AxiPathExprV3,
    pub derivation: Vec<PathRewriteStepV3>,
}

impl PathExprV2 {
    /// Validate endpoint indexing and return the unique source/target pair.
    ///
    /// Serialized certificate expressions are untrusted trees. Proof-producing
    /// APIs call this before normalization so an ill-typed composition never
    /// becomes a path-equality witness.
    pub fn checked_endpoints(&self) -> Result<(u32, u32), String> {
        match self {
            PathExprV2::Reflexive { entity } => Ok((*entity, *entity)),
            PathExprV2::Step { from, to, .. } => Ok((*from, *to)),
            PathExprV2::Trans { left, right } => {
                let (left_start, left_end) = left.checked_endpoints()?;
                let (right_start, right_end) = right.checked_endpoints()?;
                if left_end != right_start {
                    return Err(format!(
                        "invalid trans endpoints: left.end={left_end} right.start={right_start}"
                    ));
                }
                Ok((left_start, right_end))
            }
            PathExprV2::Inv { path } => {
                let (start, end) = path.checked_endpoints()?;
                Ok((end, start))
            }
        }
    }

    /// Construct a checked identity path.
    pub fn identity(entity: u32) -> Self {
        Self::Reflexive { entity }
    }

    /// Compose endpoint-indexed paths, rejecting a mismatched middle object.
    pub fn compose(left: Self, right: Self) -> Result<Self, String> {
        let (_, left_end) = left.checked_endpoints()?;
        let (right_start, _) = right.checked_endpoints()?;
        if left_end != right_start {
            return Err(format!(
                "invalid trans endpoints: left.end={left_end} right.start={right_start}"
            ));
        }
        Ok(Self::Trans {
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    /// Construct the formal inverse of a checked path.
    pub fn inverse(path: Self) -> Result<Self, String> {
        path.checked_endpoints()?;
        Ok(Self::Inv {
            path: Box::new(path),
        })
    }

    fn start_entity(&self) -> u32 {
        match self {
            PathExprV2::Reflexive { entity } => *entity,
            PathExprV2::Step { from, .. } => *from,
            PathExprV2::Trans { left, .. } => left.start_entity(),
            PathExprV2::Inv { path } => path.end_entity(),
        }
    }

    fn end_entity(&self) -> u32 {
        match self {
            PathExprV2::Reflexive { entity } => *entity,
            PathExprV2::Step { to, .. } => *to,
            PathExprV2::Trans { right, .. } => right.end_entity(),
            PathExprV2::Inv { path } => path.start_entity(),
        }
    }

    /// Starting endpoint of the path expression (certificate-level, untyped).
    pub fn start(&self) -> u32 {
        self.start_entity()
    }

    /// Ending endpoint of the path expression (certificate-level, untyped).
    pub fn end(&self) -> u32 {
        self.end_entity()
    }

    fn is_atom(&self) -> bool {
        match self {
            PathExprV2::Step { .. } => true,
            PathExprV2::Inv { path } => matches!(path.as_ref(), PathExprV2::Step { .. }),
            _ => false,
        }
    }

    fn atoms_are_inverse(left: &PathExprV2, right: &PathExprV2) -> bool {
        match (left, right) {
            (PathExprV2::Step { from, rel_type, to }, PathExprV2::Inv { path }) => {
                matches!(path.as_ref(), PathExprV2::Step {
                from: from2,
                rel_type: rel2,
                to: to2
            } if from == from2 && rel_type == rel2 && to == to2)
            }
            (PathExprV2::Inv { path }, PathExprV2::Step { from, rel_type, to }) => {
                matches!(path.as_ref(), PathExprV2::Step {
                from: from2,
                rel_type: rel2,
                to: to2
            } if from == from2 && rel_type == rel2 && to == to2)
            }
            _ => false,
        }
    }

    fn invert_atom(atom: PathExprV2) -> PathExprV2 {
        match atom {
            PathExprV2::Step { from, rel_type, to } => PathExprV2::Inv {
                path: Box::new(PathExprV2::Step { from, rel_type, to }),
            },
            PathExprV2::Inv { path } => *path,
            other => PathExprV2::Inv {
                path: Box::new(other),
            },
        }
    }

    fn flatten_atoms(&self) -> Vec<PathExprV2> {
        match self {
            PathExprV2::Reflexive { .. } => Vec::new(),
            PathExprV2::Step { from, rel_type, to } => vec![PathExprV2::Step {
                from: *from,
                rel_type: *rel_type,
                to: *to,
            }],
            PathExprV2::Trans { left, right } => {
                let mut atoms = left.flatten_atoms();
                atoms.extend(right.flatten_atoms());
                atoms
            }
            PathExprV2::Inv { path } => {
                let mut atoms = path.flatten_atoms();
                atoms.reverse();
                atoms
                    .into_iter()
                    .map(PathExprV2::invert_atom)
                    .collect::<Vec<_>>()
            }
        }
    }

    fn reduce_atoms(atoms: Vec<PathExprV2>) -> Vec<PathExprV2> {
        let mut reduced: Vec<PathExprV2> = Vec::with_capacity(atoms.len());
        for atom in atoms {
            if let Some(prev) = reduced.last() {
                if PathExprV2::atoms_are_inverse(prev, &atom) {
                    reduced.pop();
                    continue;
                }
            }
            reduced.push(atom);
        }
        reduced
    }

    fn build_from_atoms(start_entity: u32, atoms: &[PathExprV2]) -> PathExprV2 {
        match atoms.split_first() {
            None => PathExprV2::Reflexive {
                entity: start_entity,
            },
            Some((first, rest)) => {
                if rest.is_empty() {
                    first.clone()
                } else {
                    PathExprV2::Trans {
                        left: Box::new(first.clone()),
                        right: Box::new(PathExprV2::build_from_atoms(start_entity, rest)),
                    }
                }
            }
        }
    }

    fn atom_start_entity(atom: &PathExprV2) -> Option<u32> {
        match atom {
            PathExprV2::Step { from, .. } => Some(*from),
            PathExprV2::Inv { path } => match path.as_ref() {
                PathExprV2::Step { to, .. } => Some(*to),
                _ => None,
            },
            _ => None,
        }
    }

    fn apply_rule(rule: &PathRewriteRuleV2, expr: &PathExprV2) -> Result<PathExprV2, String> {
        match rule {
            PathRewriteRuleV2::IdLeft => match expr {
                PathExprV2::Trans { left, right }
                    if matches!(left.as_ref(), PathExprV2::Reflexive { .. }) =>
                {
                    Ok((**right).clone())
                }
                _ => Err("id_left: expected `trans (reflexive _) p`".into()),
            },

            PathRewriteRuleV2::IdRight => match expr {
                PathExprV2::Trans { left, right }
                    if matches!(right.as_ref(), PathExprV2::Reflexive { .. }) =>
                {
                    Ok((**left).clone())
                }
                _ => Err("id_right: expected `trans p (reflexive _)`".into()),
            },

            PathRewriteRuleV2::AssocRight => match expr {
                PathExprV2::Trans { left, right } => match left.as_ref() {
                    PathExprV2::Trans {
                        left: left_left,
                        right: left_right,
                    } => Ok(PathExprV2::Trans {
                        left: Box::new((**left_left).clone()),
                        right: Box::new(PathExprV2::Trans {
                            left: Box::new((**left_right).clone()),
                            right: Box::new((**right).clone()),
                        }),
                    }),
                    _ => Err("assoc_right: expected `trans (trans p q) r`".into()),
                },
                _ => Err("assoc_right: expected `trans (trans p q) r`".into()),
            },

            PathRewriteRuleV2::InvRefl => match expr {
                PathExprV2::Inv { path } => match path.as_ref() {
                    PathExprV2::Reflexive { entity } => Ok(PathExprV2::Reflexive { entity: *entity }),
                    _ => Err("inv_refl: expected `inv (reflexive a)`".into()),
                },
                _ => Err("inv_refl: expected `inv (reflexive a)`".into()),
            },

            PathRewriteRuleV2::InvInv => match expr {
                PathExprV2::Inv { path } => match path.as_ref() {
                    PathExprV2::Inv { path: inner } => Ok((**inner).clone()),
                    _ => Err("inv_inv: expected `inv (inv p)`".into()),
                },
                _ => Err("inv_inv: expected `inv (inv p)`".into()),
            },

            PathRewriteRuleV2::InvTrans => match expr {
                PathExprV2::Inv { path } => match path.as_ref() {
                    PathExprV2::Trans { left, right } => Ok(PathExprV2::Trans {
                        left: Box::new(PathExprV2::Inv {
                            path: Box::new((**right).clone()),
                        }),
                        right: Box::new(PathExprV2::Inv {
                            path: Box::new((**left).clone()),
                        }),
                    }),
                    _ => Err("inv_trans: expected `inv (trans p q)`".into()),
                },
                _ => Err("inv_trans: expected `inv (trans p q)`".into()),
            },

            PathRewriteRuleV2::CancelHead => match expr {
                PathExprV2::Trans { left, right } => {
                    if let PathExprV2::Trans {
                        left: middle,
                        right: rest,
                    } = right.as_ref()
                    {
                        if left.is_atom()
                            && middle.is_atom()
                            && PathExprV2::atoms_are_inverse(left, middle)
                        {
                            Ok((**rest).clone())
                        } else {
                            Err("cancel_head: expected `trans atom (trans invAtom rest)` with matching inverse atoms".into())
                        }
                    } else if left.is_atom()
                        && right.is_atom()
                        && PathExprV2::atoms_are_inverse(left, right)
                    {
                        match PathExprV2::atom_start_entity(left) {
                            Some(start) => Ok(PathExprV2::Reflexive { entity: start }),
                            None => Err("cancel_head: internal error (expected atom start entity)".into()),
                        }
                    } else {
                        Err("cancel_head: expected `trans atom (trans invAtom rest)` or `trans atom invAtom`".into())
                    }
                }
                _ => Err(
                    "cancel_head: expected `trans atom (trans invAtom rest)` or `trans atom invAtom`"
                        .into(),
                ),
            },
        }
    }

    fn apply_at(
        expr: &PathExprV2,
        pos: &[u32],
        rule: &PathRewriteRuleV2,
    ) -> Result<PathExprV2, String> {
        match (pos.split_first(), expr) {
            (None, _) => PathExprV2::apply_rule(rule, expr),
            (Some((&0, rest)), PathExprV2::Trans { left, right }) => Ok(PathExprV2::Trans {
                left: Box::new(PathExprV2::apply_at(left, rest, rule)?),
                right: right.clone(),
            }),
            (Some((&1, rest)), PathExprV2::Trans { left, right }) => Ok(PathExprV2::Trans {
                left: left.clone(),
                right: Box::new(PathExprV2::apply_at(right, rest, rule)?),
            }),
            (Some((&2, rest)), PathExprV2::Inv { path }) => Ok(PathExprV2::Inv {
                path: Box::new(PathExprV2::apply_at(path, rest, rule)?),
            }),
            (Some((other, _)), _) => Err(format!("invalid rewrite position head: {other}")),
        }
    }

    /// Apply a v2 rewrite derivation (rule + position steps) to this expression.
    ///
    /// This is a small deterministic “replay” utility:
    /// - Rust uses it for sanity-checking proof payloads in tests,
    /// - and it is useful for debugging certificates (does this derivation actually
    ///   rewrite the input to the claimed output?).
    ///
    /// Note: v3 derivations (`rule_ref`) require `.axi`-anchored rule lookup and are
    /// replayed in the Lean checker.
    pub fn apply_derivation_v2(
        &self,
        derivation: &[PathRewriteStepV2],
    ) -> Result<PathExprV2, String> {
        if derivation.len() > MAX_PATH_REWRITE_STEPS_V2 {
            return Err(format!(
                "path rewrite trace exceeds the finite bound {MAX_PATH_REWRITE_STEPS_V2}"
            ));
        }
        let endpoints = self.checked_endpoints()?;
        let mut current = self.clone();
        for step in derivation {
            let next = PathExprV2::apply_at(&current, &step.pos, &step.rule)?;
            if next.checked_endpoints()? != endpoints {
                return Err("rewrite step changed path endpoints".to_string());
            }
            current = next;
        }
        Ok(current)
    }

    fn first_applicable_rule(expr: &PathExprV2) -> Option<PathRewriteRuleV2> {
        match expr {
            PathExprV2::Inv { path } => match path.as_ref() {
                PathExprV2::Reflexive { .. } => Some(PathRewriteRuleV2::InvRefl),
                PathExprV2::Inv { .. } => Some(PathRewriteRuleV2::InvInv),
                PathExprV2::Trans { .. } => Some(PathRewriteRuleV2::InvTrans),
                _ => None,
            },
            PathExprV2::Trans { left, right } => {
                if matches!(left.as_ref(), PathExprV2::Reflexive { .. }) {
                    return Some(PathRewriteRuleV2::IdLeft);
                }
                if matches!(right.as_ref(), PathExprV2::Reflexive { .. }) {
                    return Some(PathRewriteRuleV2::IdRight);
                }
                if matches!(left.as_ref(), PathExprV2::Trans { .. }) {
                    return Some(PathRewriteRuleV2::AssocRight);
                }
                if left.is_atom() && right.is_atom() && PathExprV2::atoms_are_inverse(left, right) {
                    return Some(PathRewriteRuleV2::CancelHead);
                }
                if let PathExprV2::Trans {
                    left: middle,
                    right: _,
                } = right.as_ref()
                {
                    if left.is_atom()
                        && middle.is_atom()
                        && PathExprV2::atoms_are_inverse(left, middle)
                    {
                        return Some(PathRewriteRuleV2::CancelHead);
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn find_first_rewrite(expr: &PathExprV2) -> Option<(Vec<u32>, PathRewriteRuleV2)> {
        if let Some(rule) = PathExprV2::first_applicable_rule(expr) {
            return Some((Vec::new(), rule));
        }

        match expr {
            PathExprV2::Trans { left, right } => {
                if let Some((mut pos, rule)) = PathExprV2::find_first_rewrite(left) {
                    pos.insert(0, 0);
                    return Some((pos, rule));
                }
                if let Some((mut pos, rule)) = PathExprV2::find_first_rewrite(right) {
                    pos.insert(0, 1);
                    return Some((pos, rule));
                }
                None
            }
            PathExprV2::Inv { path } => {
                PathExprV2::find_first_rewrite(path).map(|(mut pos, rule)| {
                    pos.insert(0, 2);
                    (pos, rule)
                })
            }
            _ => None,
        }
    }

    /// Normalize a well-typed path and emit the complete replay trace.
    ///
    /// Certificate production fails closed if the deterministic rewrite system
    /// cannot reach the independently computed normal form within the explicit
    /// finite trace bound. There is no trace-free certificate mode.
    pub fn normalize_with_derivation(
        &self,
    ) -> Result<(PathExprV2, Vec<PathRewriteStepV2>), String> {
        let endpoints = self.checked_endpoints()?;
        let target = self.normalize();
        if target.checked_endpoints()? != endpoints {
            return Err("normalization changed path endpoints".to_string());
        }
        let mut current = self.clone();
        let mut derivation: Vec<PathRewriteStepV2> = Vec::new();

        for _ in 0..MAX_PATH_REWRITE_STEPS_V2 {
            if current == target {
                return Ok((target, derivation));
            }

            let Some((pos, rule)) = PathExprV2::find_first_rewrite(&current) else {
                return Err(
                    "normalization trace got stuck before the canonical normal form".to_string(),
                );
            };

            let next = PathExprV2::apply_at(&current, &pos, &rule)?;
            if next.checked_endpoints()? != endpoints {
                return Err("normalization rewrite changed path endpoints".to_string());
            }

            derivation.push(PathRewriteStepV2 { pos, rule });
            current = next;
        }

        Err(format!(
            "normalization trace exceeds the finite bound {MAX_PATH_REWRITE_STEPS_V2}"
        ))
    }

    pub fn normalize(&self) -> PathExprV2 {
        let start = self.start_entity();
        let atoms = PathExprV2::reduce_atoms(self.flatten_atoms());
        PathExprV2::build_from_atoms(start, &atoms)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NormalizePathProofV2 {
    pub input: PathExprV2,
    pub normalized: PathExprV2,
    /// Mandatory replay trace. A checker never accepts normalization by trusting
    /// a trace-free runtime claim.
    pub derivation: Vec<PathRewriteStepV2>,
}

/// Internal builtin rewrite replay payload reused by `path_equiv_v2`.
///
/// There is no standalone `rewrite_derivation_v2` wire kind. Anchored generic
/// rewrite certificates use `rewrite_derivation_v3` and stable rule references.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RewriteDerivationProofV2 {
    pub input: PathExprV2,
    pub output: PathExprV2,
    pub derivation: Vec<PathRewriteStepV2>,
}

/// Path equivalence certificate (v2).
///
/// This kind is a reusable building block for §3 of `docs/explanation/BOOK.md`:
///
/// - Two path expressions are considered equivalent if they normalize to the same normal form.
/// - Rust must provide explicit rewrite derivations showing `left ↦ normalized`
///   and `right ↦ normalized` via local groupoid rules (rule + position).
///
/// Lean checks endpoint well-formedness, replays both mandatory derivations,
/// and recomputes normalization to ensure the claimed common normal form is correct.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PathEquivProofV2 {
    pub left: PathExprV2,
    pub right: PathExprV2,
    pub normalized: PathExprV2,
    /// Mandatory traces from both endpoint-indexed expressions to the shared
    /// normal form. Confidence arithmetic is intentionally absent.
    pub left_derivation: Vec<PathRewriteStepV2>,
    pub right_derivation: Vec<PathRewriteStepV2>,
}

// =============================================================================
// Exact finite query witnesses (v4): `.axi`-anchored, name-based
// =============================================================================

/// Exact finite query term (v4), name-based under canonical `.axi` anchoring.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FiniteQueryTermV4 {
    Var {
        name: String,
    },
    /// Constant entity identifier (stable within the anchored `.axi` meaning-plane).
    ///
    /// For `.axi`-anchored certificates we treat entity IDs as:
    /// - object element names (e.g. `"Alice"`), or
    /// - typed fact ids (`"axi:fact:v2:sha256:..."`) when referring to fact nodes.
    Const {
        entity: String,
    },
}

/// Finite regular-path expression over canonical relation labels (v4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FiniteQueryRegexV4 {
    Epsilon,
    Rel { rel: String },
    Seq { parts: Vec<FiniteQueryRegexV4> },
    Alt { parts: Vec<FiniteQueryRegexV4> },
    Star { inner: Box<FiniteQueryRegexV4> },
    Plus { inner: Box<FiniteQueryRegexV4> },
    Opt { inner: Box<FiniteQueryRegexV4> },
}

/// Query atom in the exact finite certified query IR (v4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FiniteQueryAtomV4 {
    Type {
        term: FiniteQueryTermV4,
        type_name: String,
    },
    AttrEq {
        term: FiniteQueryTermV4,
        key: String,
        value: String,
    },
    Path {
        left: FiniteQueryTermV4,
        regex: FiniteQueryRegexV4,
        right: FiniteQueryTermV4,
    },
}

/// Exact finite certified query IR (v4): a bounded UCQ/RPQ.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FiniteQueryV4 {
    pub select_vars: Vec<String>,
    pub disjuncts: Vec<Vec<FiniteQueryAtomV4>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_hops: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_confidence_fp: Option<FixedPointProbability>,
}

pub const FINITE_QUERY_MAX_DISJUNCTS: usize = 16;
pub const FINITE_QUERY_MAX_ATOMS_PER_DISJUNCT: usize = 64;
pub const FINITE_QUERY_MAX_REGEX_NODES: usize = 128;
pub const FINITE_QUERY_MAX_HOPS: u32 = 32;
pub const FINITE_QUERY_MAX_ASSIGNMENTS: usize = 1_000_000;

fn collect_term_vars(term: &FiniteQueryTermV4, vars: &mut BTreeSet<String>) {
    if let FiniteQueryTermV4::Var { name } = term {
        vars.insert(name.clone());
    }
}

fn regex_profile(regex: &FiniteQueryRegexV4) -> (usize, bool) {
    match regex {
        FiniteQueryRegexV4::Epsilon | FiniteQueryRegexV4::Rel { .. } => (1, false),
        FiniteQueryRegexV4::Seq { parts } | FiniteQueryRegexV4::Alt { parts } => {
            parts.iter().fold((1, false), |(nodes, repeated), part| {
                let (part_nodes, part_repeated) = regex_profile(part);
                (nodes.saturating_add(part_nodes), repeated || part_repeated)
            })
        }
        FiniteQueryRegexV4::Star { inner } | FiniteQueryRegexV4::Plus { inner } => {
            let (nodes, _) = regex_profile(inner);
            (nodes.saturating_add(1), true)
        }
        FiniteQueryRegexV4::Opt { inner } => {
            let (nodes, repeated) = regex_profile(inner);
            (nodes.saturating_add(1), repeated)
        }
    }
}

/// Validate the exact finite fragment shared by Rust and Lean.
///
/// The fragment is a finite union of conjunctions over type, canonical derived
/// attribute equality, and regular-path atoms. Repetition (`*`/`+`) requires an
/// explicit hop bound. The checker also caps syntax, hops, and the Cartesian
/// assignment universe so certificate checking is total and operationally
/// bounded. These bounds are query-checker scope, not ontology-closure claims.
pub fn validate_finite_exact_fragment(
    binding: &PreparedQueryBindingV1,
    entity_count: usize,
) -> Result<(), String> {
    if binding.query.disjuncts.is_empty()
        || binding.query.disjuncts.len() > FINITE_QUERY_MAX_DISJUNCTS
    {
        return Err(format!(
            "finite exact query requires 1..={FINITE_QUERY_MAX_DISJUNCTS} disjuncts"
        ));
    }
    if binding
        .query
        .max_hops
        .is_some_and(|hops| hops > FINITE_QUERY_MAX_HOPS)
    {
        return Err(format!(
            "finite exact query max_hops exceeds {FINITE_QUERY_MAX_HOPS}"
        ));
    }

    let mut total_assignments = 0usize;
    for disjunct in &binding.query.disjuncts {
        if disjunct.len() > FINITE_QUERY_MAX_ATOMS_PER_DISJUNCT {
            return Err(format!(
                "finite exact query disjunct exceeds {FINITE_QUERY_MAX_ATOMS_PER_DISJUNCT} atoms"
            ));
        }
        let mut vars = BTreeSet::new();
        for atom in disjunct {
            match atom {
                FiniteQueryAtomV4::Type { term, .. } | FiniteQueryAtomV4::AttrEq { term, .. } => {
                    collect_term_vars(term, &mut vars);
                }
                FiniteQueryAtomV4::Path { left, regex, right } => {
                    collect_term_vars(left, &mut vars);
                    collect_term_vars(right, &mut vars);
                    let (nodes, repeated) = regex_profile(regex);
                    if nodes > FINITE_QUERY_MAX_REGEX_NODES {
                        return Err(format!(
                            "finite exact query regex exceeds {FINITE_QUERY_MAX_REGEX_NODES} nodes"
                        ));
                    }
                    if repeated && binding.query.max_hops.is_none() {
                        return Err(
                            "finite exact query repetition requires explicit max_hops".to_string()
                        );
                    }
                }
            }
        }
        let assignments = entity_count
            .checked_pow(vars.len() as u32)
            .ok_or_else(|| "finite exact query assignment universe overflows usize".to_string())?;
        total_assignments = total_assignments
            .checked_add(assignments)
            .ok_or_else(|| "finite exact query assignment universe overflows usize".to_string())?;
        if total_assignments > FINITE_QUERY_MAX_ASSIGNMENTS {
            return Err(format!(
                "finite exact query assignment universe exceeds {FINITE_QUERY_MAX_ASSIGNMENTS}"
            ));
        }
    }
    Ok(())
}

/// A single variable binding in an exact finite query-witness row (v4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FiniteQueryBindingV4 {
    pub var: String,
    pub entity: String,
}

/// Reachability witness (v3): name-based, `.axi`-anchored.
///
/// Each step carries an `axi_fact_id` that lets the trusted checker validate
/// the step against the anchored canonical `.axi` inputs (no PathDBExport tables).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReachabilityProofV3 {
    Reflexive {
        entity: String,
    },
    Step {
        from: String,
        rel: String,
        to: String,
        rel_confidence_fp: FixedPointProbability,
        axi_fact_id: String,
        rest: Box<ReachabilityProofV3>,
    },
}

/// Witness for one exact finite query atom under a full binding (v4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FiniteQueryAtomWitnessV4 {
    Type {
        entity: String,
        type_name: String,
    },
    AttrEq {
        entity: String,
        key: String,
        value: String,
    },
    Path {
        proof: ReachabilityProofV3,
    },
}

/// One full-binding witness row for a finite disjunctive query (v4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FiniteQueryRowV4 {
    pub disjunct: u32,
    pub bindings: Vec<FiniteQueryBindingV4>,
    pub witnesses: Vec<FiniteQueryAtomWitnessV4>,
}

// =============================================================================
// Query-result v4: cryptographic binding plus exact finite completeness
// =============================================================================

/// The only certifiable claim: exact answer completeness for the explicitly
/// bounded finite query denotation checked by Lean. It says nothing about
/// open-world ontology closure, omitted evidence, approximate operators, or
/// queries rejected by `validate_finite_exact_fragment`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PreparedQueryClaimKindV1 {
    FiniteExactComplete,
}

/// Lean-checkable binding constructed from the prepared lowered query AST.
///
/// Raw source, plans, inferred types, runtime kernel references, rows, and the
/// runtime truncation observation are deliberately absent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PreparedQueryBindingV1 {
    pub version: u32,
    pub query: FiniteQueryV4,
    pub row_limit: u64,
    pub claim_kind: PreparedQueryClaimKindV1,
}

/// Ordered selected projection for one returned certificate row. The order of
/// `projections` is exactly `binding.query.select_vars`; row order is retained.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StableSelectedRowV1 {
    pub projections: Vec<FiniteQueryBindingV4>,
}

fn push_text_field(fields: &mut Vec<Vec<u8>>, value: &str) {
    fields.push(value.as_bytes().to_vec());
}

fn push_term_fields_v1(fields: &mut Vec<Vec<u8>>, term: &FiniteQueryTermV4) {
    match term {
        FiniteQueryTermV4::Var { name } => {
            push_text_field(fields, "term_var");
            push_text_field(fields, name);
        }
        FiniteQueryTermV4::Const { entity } => {
            push_text_field(fields, "term_const");
            push_text_field(fields, entity);
        }
    }
}

fn push_regex_fields_v1(fields: &mut Vec<Vec<u8>>, regex: &FiniteQueryRegexV4) {
    match regex {
        FiniteQueryRegexV4::Epsilon => push_text_field(fields, "regex_epsilon"),
        FiniteQueryRegexV4::Rel { rel } => {
            push_text_field(fields, "regex_rel");
            push_text_field(fields, rel);
        }
        FiniteQueryRegexV4::Seq { parts } => {
            push_text_field(fields, "regex_seq");
            push_text_field(fields, &parts.len().to_string());
            for part in parts {
                push_regex_fields_v1(fields, part);
            }
        }
        FiniteQueryRegexV4::Alt { parts } => {
            push_text_field(fields, "regex_alt");
            push_text_field(fields, &parts.len().to_string());
            for part in parts {
                push_regex_fields_v1(fields, part);
            }
        }
        FiniteQueryRegexV4::Star { inner } => {
            push_text_field(fields, "regex_star");
            push_regex_fields_v1(fields, inner);
        }
        FiniteQueryRegexV4::Plus { inner } => {
            push_text_field(fields, "regex_plus");
            push_regex_fields_v1(fields, inner);
        }
        FiniteQueryRegexV4::Opt { inner } => {
            push_text_field(fields, "regex_opt");
            push_regex_fields_v1(fields, inner);
        }
    }
}

fn push_atom_fields_v1(fields: &mut Vec<Vec<u8>>, atom: &FiniteQueryAtomV4) {
    match atom {
        FiniteQueryAtomV4::Type { term, type_name } => {
            push_text_field(fields, "atom_type");
            push_term_fields_v1(fields, term);
            push_text_field(fields, type_name);
        }
        FiniteQueryAtomV4::AttrEq { term, key, value } => {
            push_text_field(fields, "atom_attr_eq");
            push_term_fields_v1(fields, term);
            push_text_field(fields, key);
            push_text_field(fields, value);
        }
        FiniteQueryAtomV4::Path { left, regex, right } => {
            push_text_field(fields, "atom_path");
            push_term_fields_v1(fields, left);
            push_regex_fields_v1(fields, regex);
            push_term_fields_v1(fields, right);
        }
    }
}

impl PreparedQueryBindingV1 {
    pub fn new(query: FiniteQueryV4, row_limit: u64) -> Self {
        Self {
            version: PREPARED_QUERY_BINDING_VERSION_V1,
            query,
            row_limit,
            claim_kind: PreparedQueryClaimKindV1::FiniteExactComplete,
        }
    }

    /// Canonical ordered fields fed to the shared V2 identity framing.
    pub fn canonical_digest_fields_v1(&self) -> Result<Vec<Vec<u8>>, String> {
        if self.version != PREPARED_QUERY_BINDING_VERSION_V1 {
            return Err(format!(
                "unsupported prepared query binding version {}, expected {}",
                self.version, PREPARED_QUERY_BINDING_VERSION_V1
            ));
        }
        if self.claim_kind != PreparedQueryClaimKindV1::FiniteExactComplete {
            return Err("unsupported prepared query claim kind".to_string());
        }

        let mut fields = Vec::new();
        push_text_field(&mut fields, "prepared_query_binding_v1");
        push_text_field(&mut fields, "1");
        push_text_field(&mut fields, "finite_exact_complete");
        push_text_field(&mut fields, "select_count");
        push_text_field(&mut fields, &self.query.select_vars.len().to_string());
        for selected in &self.query.select_vars {
            push_text_field(&mut fields, "select");
            push_text_field(&mut fields, selected);
        }
        push_text_field(&mut fields, "disjunct_count");
        push_text_field(&mut fields, &self.query.disjuncts.len().to_string());
        for (index, disjunct) in self.query.disjuncts.iter().enumerate() {
            push_text_field(&mut fields, "disjunct");
            push_text_field(&mut fields, &index.to_string());
            push_text_field(&mut fields, &disjunct.len().to_string());
            for atom in disjunct {
                push_atom_fields_v1(&mut fields, atom);
            }
        }
        match self.query.max_hops {
            Some(max_hops) => {
                push_text_field(&mut fields, "max_hops_some");
                push_text_field(&mut fields, &max_hops.to_string());
            }
            None => push_text_field(&mut fields, "max_hops_none"),
        }
        match self.query.min_confidence_fp {
            Some(min_confidence) => {
                push_text_field(&mut fields, "min_confidence_some");
                push_text_field(&mut fields, &min_confidence.numerator().to_string());
            }
            None => push_text_field(&mut fields, "min_confidence_none"),
        }
        push_text_field(&mut fields, "row_limit");
        push_text_field(&mut fields, &self.row_limit.to_string());
        Ok(fields)
    }

    pub fn digest_v1(&self) -> Result<QueryIdV2, String> {
        let fields = self.canonical_digest_fields_v1()?;
        let refs = fields.iter().map(Vec::as_slice).collect::<Vec<_>>();
        Ok(QueryIdV2::from_canonical_fields(&refs))
    }
}

/// Query-result v4. It is valid only inside a certificate envelope V3.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QueryResultProofV4 {
    pub binding: PreparedQueryBindingV1,
    pub prepared_query_digest_v1: QueryIdV2,
    pub rows: Vec<FiniteQueryRowV4>,
    pub runtime_truncated: bool,
    pub answer_digest_v1: AnswerIdV2,
}

impl QueryResultProofV4 {
    pub fn selected_rows_v1(&self) -> Result<Vec<StableSelectedRowV1>, String> {
        selected_rows_v1(&self.binding, &self.rows)
    }

    pub fn recompute_answer_digest_v1(&self) -> Result<AnswerIdV2, String> {
        answer_digest_v1(
            &self.binding,
            &self.prepared_query_digest_v1,
            &self.rows,
            self.runtime_truncated,
        )
    }

    pub fn validate_internal_digests(&self) -> Result<(), String> {
        if self.runtime_truncated {
            return Err(
                "finite exact query certificate cannot claim a truncated runtime answer"
                    .to_string(),
            );
        }
        let prepared = self.binding.digest_v1()?;
        if prepared != self.prepared_query_digest_v1 {
            return Err("prepared-query digest does not match embedded binding".to_string());
        }
        if u64::try_from(self.rows.len()).unwrap_or(u64::MAX) > self.binding.row_limit {
            return Err(format!(
                "certificate row count {} exceeds row_limit {}",
                self.rows.len(),
                self.binding.row_limit
            ));
        }
        let answer = self.recompute_answer_digest_v1()?;
        if answer != self.answer_digest_v1 {
            return Err("answer digest does not match certificate rows".to_string());
        }
        Ok(())
    }
}

pub fn selected_rows_v1(
    binding: &PreparedQueryBindingV1,
    rows: &[FiniteQueryRowV4],
) -> Result<Vec<StableSelectedRowV1>, String> {
    rows.iter()
        .enumerate()
        .map(|(row_index, row)| {
            let mut projections = Vec::with_capacity(binding.query.select_vars.len());
            for selected in &binding.query.select_vars {
                let mut matches = row.bindings.iter().filter(|item| item.var == *selected);
                let Some(found) = matches.next() else {
                    return Err(format!(
                        "row {row_index} is missing selected variable `{selected}`"
                    ));
                };
                if matches.next().is_some() {
                    return Err(format!(
                        "row {row_index} has duplicate selected variable `{selected}`"
                    ));
                }
                projections.push(found.clone());
            }
            Ok(StableSelectedRowV1 { projections })
        })
        .collect()
}

pub fn answer_digest_v1(
    binding: &PreparedQueryBindingV1,
    prepared_query_digest: &QueryIdV2,
    rows: &[FiniteQueryRowV4],
    runtime_truncated: bool,
) -> Result<AnswerIdV2, String> {
    let selected_rows = selected_rows_v1(binding, rows)?;
    let mut fields = Vec::new();
    push_text_field(&mut fields, "query_answer_v1");
    push_text_field(&mut fields, prepared_query_digest.as_str());
    push_text_field(&mut fields, "select_count");
    push_text_field(&mut fields, &binding.query.select_vars.len().to_string());
    for selected in &binding.query.select_vars {
        push_text_field(&mut fields, selected);
    }
    push_text_field(&mut fields, "row_count");
    push_text_field(&mut fields, &selected_rows.len().to_string());
    for (row_index, row) in selected_rows.iter().enumerate() {
        push_text_field(&mut fields, "row");
        push_text_field(&mut fields, &row_index.to_string());
        for projection in &row.projections {
            push_text_field(&mut fields, &projection.var);
            push_text_field(&mut fields, &projection.entity);
        }
    }
    push_text_field(
        &mut fields,
        if runtime_truncated {
            "runtime_truncated_true"
        } else {
            "runtime_truncated_false"
        },
    );
    let refs = fields.iter().map(Vec::as_slice).collect::<Vec<_>>();
    Ok(AnswerIdV2::from_canonical_fields(&refs))
}

/// Exact accepted-byte revision anchor for certificate envelope V3.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CertificateAnchorV2 {
    pub revision_digest_v2: RevisionDigestV2,
}

impl CertificateAnchorV2 {
    pub fn new(revision_digest_v2: RevisionDigestV2) -> Self {
        Self { revision_digest_v2 }
    }
}

/// Strict V3 envelope. V2 certificates remain represented by `CertificateV2`;
/// no V2 payload can be reinterpreted as this stronger family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificateV3 {
    pub version: u32,
    pub anchor: CertificateAnchorV2,
    pub proof: QueryResultProofV4,
}

impl CertificateV3 {
    pub fn query_result_v4(
        anchor: CertificateAnchorV2,
        proof: QueryResultProofV4,
    ) -> Result<Self, String> {
        proof.validate_internal_digests()?;
        Ok(Self {
            version: CERTIFICATE_VERSION_V3,
            anchor,
            proof,
        })
    }

    pub const fn kind(&self) -> &'static str {
        "query_result_v4"
    }
}

impl Serialize for CertificateV3 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("CertificateV3", 4)?;
        state.serialize_field("version", &self.version)?;
        state.serialize_field("kind", self.kind())?;
        state.serialize_field("anchor", &self.anchor)?;
        state.serialize_field("proof", &self.proof)?;
        state.end()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CertificateV3Wire {
    version: u32,
    kind: String,
    anchor: CertificateAnchorV2,
    proof: QueryResultProofV4,
}

impl<'de> Deserialize<'de> for CertificateV3 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = CertificateV3Wire::deserialize(deserializer)?;
        if wire.version != CERTIFICATE_VERSION_V3 {
            return Err(serde::de::Error::custom(format!(
                "unsupported CertificateV3 version {}, expected {}",
                wire.version, CERTIFICATE_VERSION_V3
            )));
        }
        if wire.kind != "query_result_v4" {
            return Err(serde::de::Error::custom(format!(
                "unsupported CertificateV3 kind `{}`",
                wire.kind
            )));
        }
        wire.proof
            .validate_internal_digests()
            .map_err(serde::de::Error::custom)?;
        Ok(Self {
            version: wire.version,
            anchor: wire.anchor,
            proof: wire.proof,
        })
    }
}

pub fn certificate_digest_v2(canonical_certificate_bytes: &[u8]) -> CertificateIdV2 {
    CertificateIdV2::from_canonical_fields(&[canonical_certificate_bytes])
}

/// Runtime-facing alias for the canonical `.axi`-anchored query witness path.
#[cfg(test)]
mod query_result_v4_tests {
    use super::*;

    fn golden_binding() -> PreparedQueryBindingV1 {
        PreparedQueryBindingV1::new(
            FiniteQueryV4 {
                select_vars: vec!["?x".to_string()],
                disjuncts: vec![vec![
                    FiniteQueryAtomV4::Type {
                        term: FiniteQueryTermV4::Var {
                            name: "?x".to_string(),
                        },
                        type_name: "Node".to_string(),
                    },
                    FiniteQueryAtomV4::Path {
                        left: FiniteQueryTermV4::Var {
                            name: "?x".to_string(),
                        },
                        regex: FiniteQueryRegexV4::Seq {
                            parts: vec![
                                FiniteQueryRegexV4::Rel {
                                    rel: "parent".to_string(),
                                },
                                FiniteQueryRegexV4::Opt {
                                    inner: Box::new(FiniteQueryRegexV4::Rel {
                                        rel: "friend".to_string(),
                                    }),
                                },
                            ],
                        },
                        right: FiniteQueryTermV4::Const {
                            entity: "Bob".to_string(),
                        },
                    },
                ]],
                max_hops: Some(3),
                min_confidence_fp: Some(FixedPointProbability::try_new(500_000).unwrap()),
            },
            2,
        )
    }

    fn row(entity: &str) -> FiniteQueryRowV4 {
        FiniteQueryRowV4 {
            disjunct: 0,
            bindings: vec![FiniteQueryBindingV4 {
                var: "?x".to_string(),
                entity: entity.to_string(),
            }],
            witnesses: Vec::new(),
        }
    }

    #[test]
    fn prepared_and_answer_digest_goldens_are_stable() {
        let binding = golden_binding();
        let prepared = binding.digest_v1().unwrap();
        assert_eq!(
            prepared.as_str(),
            "axi:query:v2:sha256:966c31b16f91364c60c8c82f1d07645692ed72d440672a537efe2a5d7e621049"
        );
        let answer =
            answer_digest_v1(&binding, &prepared, &[row("Alice"), row("Bob")], false).unwrap();
        assert_eq!(
            answer.as_str(),
            "axi:answer:v2:sha256:f09ea571aa69cfca1701d37f321b6aab576791c0bda42b6acc21e9e017e07d8c"
        );
    }

    #[test]
    fn prepared_digest_binds_order_structure_bounds_and_limit() {
        let binding = golden_binding();
        let original = binding.digest_v1().unwrap();
        let mut mutations = Vec::new();

        let mut atom_order = binding.clone();
        atom_order.query.disjuncts[0].swap(0, 1);
        mutations.push(atom_order);

        let mut relation = binding.clone();
        let FiniteQueryAtomV4::Path { regex, .. } = &mut relation.query.disjuncts[0][1] else {
            unreachable!()
        };
        *regex = FiniteQueryRegexV4::Rel {
            rel: "different".to_string(),
        };
        mutations.push(relation);

        let mut disjunct_order = binding.clone();
        disjunct_order.query.disjuncts.push(Vec::new());
        disjunct_order.query.disjuncts.swap(0, 1);
        mutations.push(disjunct_order);

        let mut max_hops = binding.clone();
        max_hops.query.max_hops = Some(4);
        mutations.push(max_hops);

        let mut confidence = binding.clone();
        confidence.query.min_confidence_fp = Some(FixedPointProbability::try_new(500_001).unwrap());
        mutations.push(confidence);

        let mut limit = binding.clone();
        limit.row_limit = 1;
        mutations.push(limit);

        for mutation in mutations {
            assert_ne!(mutation.digest_v1().unwrap(), original);
        }
    }

    #[test]
    fn answer_digest_rejects_tamper_reorder_drop_duplicate_and_truncation() {
        let binding = golden_binding();
        let prepared = binding.digest_v1().unwrap();
        let rows = vec![row("Alice"), row("Bob")];
        let original = answer_digest_v1(&binding, &prepared, &rows, false).unwrap();

        let mut tampered = rows.clone();
        tampered[0].bindings[0].entity = "Mallory".to_string();
        assert_ne!(
            answer_digest_v1(&binding, &prepared, &tampered, false).unwrap(),
            original
        );

        let mut reordered = rows.clone();
        reordered.swap(0, 1);
        assert_ne!(
            answer_digest_v1(&binding, &prepared, &reordered, false).unwrap(),
            original
        );
        assert_ne!(
            answer_digest_v1(&binding, &prepared, &rows[..1], false).unwrap(),
            original
        );
        assert_ne!(
            answer_digest_v1(
                &binding,
                &prepared,
                &[rows[0].clone(), rows[0].clone()],
                false
            )
            .unwrap(),
            original
        );
        assert_ne!(
            answer_digest_v1(&binding, &prepared, &rows, true).unwrap(),
            original
        );
    }

    #[test]
    fn finite_exact_fragment_rejects_unbounded_repetition() {
        let mut binding = golden_binding();
        let FiniteQueryAtomV4::Path { regex, .. } = &mut binding.query.disjuncts[0][1] else {
            unreachable!()
        };
        *regex = FiniteQueryRegexV4::Star {
            inner: Box::new(FiniteQueryRegexV4::Rel {
                rel: "parent".to_string(),
            }),
        };
        binding.query.max_hops = None;
        let error = validate_finite_exact_fragment(&binding, 2)
            .expect_err("unbounded repetition must remain outside the exact finite fragment");
        assert!(error.contains("requires explicit max_hops"));
    }

    #[test]
    fn certificate_v3_rejects_unknown_fields_and_internal_digest_mutation() {
        let binding = golden_binding();
        let prepared = binding.digest_v1().unwrap();
        let rows = vec![row("Alice"), row("Bob")];
        let answer = answer_digest_v1(&binding, &prepared, &rows, false).unwrap();
        let proof = QueryResultProofV4 {
            binding,
            prepared_query_digest_v1: prepared,
            rows,
            runtime_truncated: false,
            answer_digest_v1: answer,
        };
        let cert = CertificateV3::query_result_v4(
            CertificateAnchorV2::new(RevisionDigestV2::from_accepted_text("module M\n")),
            proof,
        )
        .unwrap();
        let mut value = serde_json::to_value(&cert).unwrap();
        value["unknown_upgrade"] = serde_json::json!(true);
        assert!(serde_json::from_value::<CertificateV3>(value).is_err());

        let mut value = serde_json::to_value(&cert).unwrap();
        value["proof"]["runtime_truncated"] = serde_json::json!(true);
        assert!(serde_json::from_value::<CertificateV3>(value).is_err());
    }
}

#[cfg(test)]
mod normalize_path_v2_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn axi_anchor_round_trips_revision_digest_with_stable_wire_shape() {
        let digest = format!("axi:revision:v2:sha256:{}", "0".repeat(64));
        let anchor = AxiAnchorV1::new(digest.clone());
        let value = serde_json::to_value(&anchor).expect("anchor should serialize");
        assert_eq!(value, json!({ "revision_digest_v2": digest }));

        let round_trip: AxiAnchorV1 =
            serde_json::from_value(value).expect("anchor should deserialize");
        assert_eq!(round_trip.revision_digest_v2, AxiDigest::new(digest));
    }

    #[test]
    fn certificate_v2_round_trips_typed_anchor() {
        let digest = AxiDigest::from_axi_text("module Demo\n");
        let cert = CertificateV2::reachability_v3(ReachabilityProofV3::Reflexive {
            entity: "axi:id:Node:alice".to_string(),
        })
        .with_anchor(AxiAnchorV1::new(digest.clone()));

        let json = serde_json::to_string(&cert).expect("certificate should serialize");
        let round_trip: CertificateV2 =
            serde_json::from_str(&json).expect("certificate should deserialize");

        assert_eq!(
            round_trip
                .anchor
                .expect("anchor should be present")
                .revision_digest_v2,
            digest
        );
    }

    #[test]
    fn certificate_v2_round_trips_reachability_v3_payload() {
        let cert = CertificateV2::reachability_v3(ReachabilityProofV3::Reflexive {
            entity: "axi:id:Node:alice".to_string(),
        });

        let json = serde_json::to_value(&cert).expect("certificate should serialize");
        assert_eq!(json["kind"], "reachability_v3");

        let round_trip: CertificateV2 =
            serde_json::from_value(json).expect("certificate should deserialize");

        match round_trip.payload {
            CertificatePayloadV2::ReachabilityV3 {
                proof: ReachabilityProofV3::Reflexive { entity },
            } => {
                assert_eq!(entity, "axi:id:Node:alice");
            }
            other => panic!("expected reachability_v3 payload, got {other:?}"),
        }
    }

    #[test]
    fn certificate_v2_rejects_wrong_wrapper_version() {
        let cert = CertificateV2::reachability_v3(ReachabilityProofV3::Reflexive {
            entity: "axi:id:Node:alice".to_string(),
        });
        let mut json = serde_json::to_value(&cert).expect("certificate should serialize");
        json["version"] = json!(1);

        let err = serde_json::from_value::<CertificateV2>(json)
            .expect_err("wrong certificate wrapper version must fail");
        assert!(err
            .to_string()
            .contains("unsupported CertificateV2 version"));
    }

    #[test]
    fn indexed_path_builders_reject_bad_composition_and_preserve_inverse_endpoints() {
        let first = PathExprV2::Step {
            from: 1,
            rel_type: 10,
            to: 2,
        };
        let second = PathExprV2::Step {
            from: 2,
            rel_type: 20,
            to: 3,
        };
        let composed = PathExprV2::compose(first.clone(), second)
            .expect("matching middle endpoint must compose");
        assert_eq!(composed.checked_endpoints().unwrap(), (1, 3));
        assert_eq!(
            PathExprV2::inverse(composed)
                .unwrap()
                .checked_endpoints()
                .unwrap(),
            (3, 1)
        );

        let wrong = PathExprV2::Step {
            from: 9,
            rel_type: 30,
            to: 10,
        };
        assert!(PathExprV2::compose(first, wrong).is_err());
    }

    #[test]
    fn path_certificates_require_traces_and_reject_unknown_envelope_fields() {
        let input = PathExprV2::identity(7);
        let proof = NormalizePathProofV2 {
            input: input.clone(),
            normalized: input,
            derivation: Vec::new(),
        };
        let certificate = CertificateV2::normalize_path(proof).with_anchor(AxiAnchorV1::new(
            AxiDigest::from_axi_text("module WireDomain"),
        ));
        let mut value = serde_json::to_value(certificate).unwrap();

        value["proof"].as_object_mut().unwrap().remove("derivation");
        assert!(serde_json::from_value::<CertificateV2>(value.clone()).is_err());

        value["proof"]["derivation"] = json!([]);
        value["proof"]["confidence_fp"] = json!(900_000);
        assert!(serde_json::from_value::<CertificateV2>(value.clone()).is_err());
        value["proof"]
            .as_object_mut()
            .unwrap()
            .remove("confidence_fp");

        value["confidence_fp"] = json!(900_000);
        assert!(serde_json::from_value::<CertificateV2>(value.clone()).is_err());
        value.as_object_mut().unwrap().remove("confidence_fp");

        value["anchor"]["unknown_anchor_field"] = json!(true);
        assert!(serde_json::from_value::<CertificateV2>(value).is_err());
    }

    #[test]
    fn obsolete_unanchored_rewrite_derivation_wire_kind_is_rejected() {
        let value = json!({
            "version": 2,
            "kind": "rewrite_derivation_v2",
            "proof": {
                "input": { "type": "reflexive", "entity": 7 },
                "output": { "type": "reflexive", "entity": 7 },
                "derivation": []
            }
        });
        assert!(serde_json::from_value::<CertificateV2>(value).is_err());
    }

    #[test]
    fn fixed_point_confidence_is_not_path_equality_arithmetic() {
        let a = FixedPointProbability::try_new(98_781).unwrap();
        let b = FixedPointProbability::try_new(427_863).unwrap();
        let c = FixedPointProbability::try_new(382_808).unwrap();

        assert_eq!(a.mul(b).mul(c).numerator(), 16_178);
        assert_eq!(a.mul(b.mul(c)).numerator(), 16_179);
        assert_ne!(a.mul(b).mul(c), a.mul(b.mul(c)));
    }

    #[test]
    fn normalize_with_derivation_replays_to_target() {
        let left_inner = PathExprV2::Trans {
            left: Box::new(PathExprV2::Trans {
                left: Box::new(PathExprV2::Reflexive { entity: 1 }),
                right: Box::new(PathExprV2::Step {
                    from: 1,
                    rel_type: 10,
                    to: 2,
                }),
            }),
            right: Box::new(PathExprV2::Trans {
                left: Box::new(PathExprV2::Reflexive { entity: 2 }),
                right: Box::new(PathExprV2::Step {
                    from: 2,
                    rel_type: 20,
                    to: 3,
                }),
            }),
        };

        let input = PathExprV2::Trans {
            left: Box::new(PathExprV2::Inv {
                path: Box::new(left_inner),
            }),
            right: Box::new(PathExprV2::Trans {
                left: Box::new(PathExprV2::Trans {
                    left: Box::new(PathExprV2::Step {
                        from: 1,
                        rel_type: 10,
                        to: 2,
                    }),
                    right: Box::new(PathExprV2::Step {
                        from: 2,
                        rel_type: 20,
                        to: 3,
                    }),
                }),
                right: Box::new(PathExprV2::Reflexive { entity: 3 }),
            }),
        };

        let expected = input.normalize();
        let (normalized, steps) = input
            .normalize_with_derivation()
            .expect("well-typed path must emit a derivation");
        assert_eq!(normalized, expected);

        let mut current = input.clone();
        for step in steps {
            current = PathExprV2::apply_at(&current, &step.pos, &step.rule)
                .expect("rewrite step must apply");
        }
        assert_eq!(current, normalized);
    }
}
