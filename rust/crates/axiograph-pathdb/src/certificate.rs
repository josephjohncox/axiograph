//! Certificate formats for proof-carrying results.
//!
//! This module defines a minimal, versioned JSON shape intended to be consumed
//! by a trusted checker (Lean during migration).

use crate::migration::DeltaFMigrationProofV1;
use crate::ReachabilityProof;
use axiograph_dsl::schema_v1::PathExprV3 as AxiPathExprV3;
use serde::{Deserialize, Serialize};

pub const CERTIFICATE_VERSION: u32 = 1;
pub const CERTIFICATE_VERSION_V2: u32 = 2;

/// Fixed-point denominator shared with the Lean checker (`Axiograph.Prob.Precision`).
pub const FIXED_POINT_DENOMINATOR: u32 = 1_000_000;

/// Backwards-compatible name for the fixed-point denominator.
pub const FIXED_PROB_PRECISION: u32 = FIXED_POINT_DENOMINATOR;

/// Top-level certificate wrapper (versioned and extensible).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Certificate {
    pub version: u32,
    #[serde(flatten)]
    pub payload: CertificatePayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CertificatePayload {
    Reachability { proof: ReachabilityProof },
}

impl Certificate {
    pub fn reachability(proof: ReachabilityProof) -> Self {
        Self {
            version: CERTIFICATE_VERSION,
            payload: CertificatePayload::Reachability { proof },
        }
    }
}

// ============================================================================
// Certificate v2: fixed-point probabilities (no floats in the trusted checker)
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
                "FixedProb numerator must be ≤ {}",
                FIXED_POINT_DENOMINATOR
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

        // Clamp infinities; treat NaNs as 0 (should never appear in PathDB exports).
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
    pub fn mul(self, other: Self) -> Self {
        let scaled =
            ((self.numerator as u64) * (other.numerator as u64)) / (FIXED_POINT_DENOMINATOR as u64);
        let scaled = u32::try_from(scaled).unwrap_or(FIXED_POINT_DENOMINATOR);
        Self {
            numerator: scaled.min(FIXED_POINT_DENOMINATOR),
        }
    }
}

/// Reachability witness with fixed-point confidences.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReachabilityProofV2 {
    Reflexive {
        entity: u32,
    },
    Step {
        from: u32,
        rel_type: u32,
        to: u32,
        /// Relation confidence as a fixed-point numerator (Lean checks this).
        rel_confidence_fp: FixedPointProbability,
        /// Optional fact id for this edge (for `.axi`-anchored query certificates).
        ///
        /// For PathDB export snapshots (`PathDBExportV1`), this corresponds to
        /// the `Relation_<id>` identifier in the snapshot instance.
        #[serde(skip_serializing_if = "Option::is_none")]
        relation_id: Option<u32>,
        rest: Box<ReachabilityProofV2>,
    },
}

impl ReachabilityProofV2 {
    pub fn start(&self) -> u32 {
        match self {
            ReachabilityProofV2::Reflexive { entity } => *entity,
            ReachabilityProofV2::Step { from, .. } => *from,
        }
    }

    pub fn end(&self) -> u32 {
        match self {
            ReachabilityProofV2::Reflexive { entity } => *entity,
            ReachabilityProofV2::Step { rest, .. } => rest.end(),
        }
    }

    pub fn path_len(&self) -> usize {
        match self {
            ReachabilityProofV2::Reflexive { .. } => 0,
            ReachabilityProofV2::Step { rest, .. } => 1 + rest.path_len(),
        }
    }

    pub fn path_confidence(&self) -> FixedPointProbability {
        match self {
            ReachabilityProofV2::Reflexive { .. } => FixedPointProbability {
                numerator: FIXED_POINT_DENOMINATOR,
            },
            ReachabilityProofV2::Step {
                rel_confidence_fp,
                rest,
                ..
            } => rel_confidence_fp.mul(rest.path_confidence()),
        }
    }
}

/// Versioned wrapper for v2 certificates (fixed-point probabilities).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateV2 {
    pub version: u32,
    /// Optional binding to canonical `.axi` inputs (snapshot-scoped).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<AxiAnchorV1>,
    #[serde(flatten)]
    pub payload: CertificatePayloadV2,
}

/// Certificate anchor for canonical `.axi` inputs (v1).
///
/// This is intentionally minimal at first: a stable module digest.
///
/// Digest format (shared with Lean `Axiograph.Util.Fnv1a`):
/// - `axi_digest_v1 = "fnv1a64:<16 lowercase hex digits>"`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AxiAnchorV1 {
    pub axi_digest_v1: String,
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
/// - `constraint symmetric Rel`
/// - `constraint symmetric Rel where Rel.field in {A, B, ...}`
/// - `constraint transitive Rel` (closure-compatibility for keys/functionals on carrier fields)
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
pub enum CertificatePayloadV2 {
    #[serde(rename = "axi_well_typed_v1")]
    AxiWellTypedV1 {
        proof: AxiWellTypedProofV1,
    },
    #[serde(rename = "axi_constraints_ok_v1")]
    AxiConstraintsOkV1 {
        proof: AxiConstraintsOkProofV1,
    },
    ReachabilityV2 {
        proof: ReachabilityProofV2,
    },
    ResolutionV2 {
        proof: ResolutionProofV2,
    },
    NormalizePathV2 {
        proof: NormalizePathProofV2,
    },
    RewriteDerivationV2 {
        proof: RewriteDerivationProofV2,
    },
    RewriteDerivationV3 {
        proof: RewriteDerivationProofV3,
    },
    PathEquivV2 {
        proof: PathEquivProofV2,
    },
    #[serde(rename = "query_result_v1")]
    QueryResultV1 {
        proof: QueryResultProofV1,
    },
    #[serde(rename = "query_result_v2")]
    QueryResultV2 {
        proof: QueryResultProofV2,
    },
    #[serde(rename = "query_result_v3")]
    QueryResultV3 {
        proof: QueryResultProofV3,
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

    pub fn reachability(proof: ReachabilityProofV2) -> Self {
        Self {
            version: CERTIFICATE_VERSION_V2,
            anchor: None,
            payload: CertificatePayloadV2::ReachabilityV2 { proof },
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

    pub fn rewrite_derivation(proof: RewriteDerivationProofV2) -> Self {
        Self {
            version: CERTIFICATE_VERSION_V2,
            anchor: None,
            payload: CertificatePayloadV2::RewriteDerivationV2 { proof },
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

    pub fn query_result_v1(proof: QueryResultProofV1) -> Self {
        Self {
            version: CERTIFICATE_VERSION_V2,
            anchor: None,
            payload: CertificatePayloadV2::QueryResultV1 { proof },
        }
    }

    pub fn query_result_v2(proof: QueryResultProofV2) -> Self {
        Self {
            version: CERTIFICATE_VERSION_V2,
            anchor: None,
            payload: CertificatePayloadV2::QueryResultV2 { proof },
        }
    }

    pub fn query_result_v3(proof: QueryResultProofV3) -> Self {
        Self {
            version: CERTIFICATE_VERSION_V2,
            anchor: None,
            payload: CertificatePayloadV2::QueryResultV3 { proof },
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
    let gap = if n1 >= n2 { n1 - n2 } else { n2 - n1 };
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
#[serde(tag = "type", rename_all = "snake_case")]
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
/// This is the `.axi`-anchored successor to `rewrite_derivation_v2`:
/// - v2 hardcodes the groupoid normalization rules as an enum,
/// - v3 allows certificates to reference `.axi`-declared rewrite rules.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RewriteDerivationProofV3 {
    pub input: AxiPathExprV3,
    pub output: AxiPathExprV3,
    pub derivation: Vec<PathRewriteStepV3>,
}

impl PathExprV2 {
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
    pub fn apply_derivation_v2(&self, derivation: &[PathRewriteStepV2]) -> Result<PathExprV2, String> {
        let mut current = self.clone();
        for step in derivation {
            current = PathExprV2::apply_at(&current, &step.pos, &step.rule)?;
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

    pub fn normalize_with_derivation(&self) -> (PathExprV2, Option<Vec<PathRewriteStepV2>>) {
        let target = self.normalize();
        let mut current = self.clone();
        let mut derivation: Vec<PathRewriteStepV2> = Vec::new();

        const MAX_STEPS: usize = 50_000;
        for _ in 0..MAX_STEPS {
            if current == target {
                return (target, Some(derivation));
            }

            let Some((pos, rule)) = PathExprV2::find_first_rewrite(&current) else {
                return (target, None);
            };

            let next = match PathExprV2::apply_at(&current, &pos, &rule) {
                Ok(next) => next,
                Err(_) => return (target, None),
            };

            derivation.push(PathRewriteStepV2 { pos, rule });
            current = next;
        }

        (target, None)
    }

    pub fn normalize(&self) -> PathExprV2 {
        let start = self.start_entity();
        let atoms = PathExprV2::reduce_atoms(self.flatten_atoms());
        PathExprV2::build_from_atoms(start, &atoms)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NormalizePathProofV2 {
    pub input: PathExprV2,
    pub normalized: PathExprV2,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derivation: Option<Vec<PathRewriteStepV2>>,
}

/// A replayable rewrite derivation (v2).
///
/// This is a reusable certificate kind: normalization, reconciliation explanations,
/// and domain rewrites can all be expressed as “rewrite input into output by applying
/// these rule-at-position steps”.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
/// - Rust may optionally provide explicit rewrite derivations showing `left ↦ normalized`
///   and `right ↦ normalized` via local groupoid rules (rule + position).
///
/// Lean checks:
/// - endpoint well-formedness,
/// - optional derivation replay for both sides,
/// - and recomputes normalization to ensure the claimed common normal form is correct.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathEquivProofV2 {
    pub left: PathExprV2,
    pub right: PathExprV2,
    pub normalized: PathExprV2,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_derivation: Option<Vec<PathRewriteStepV2>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right_derivation: Option<Vec<PathRewriteStepV2>>,
}

// =============================================================================
// Query result certificates (conjunctive queries; AxQL / SQL-ish)
// =============================================================================

/// Certified query term (v1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueryTermV1 {
    Var { name: String },
    Const { entity: u32 },
}

/// Regular-path query (RPQ) expression over relation-type IDs (interned string ids).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueryRegexV1 {
    Epsilon,
    Rel { rel_type_id: u32 },
    Seq { parts: Vec<QueryRegexV1> },
    Alt { parts: Vec<QueryRegexV1> },
    Star { inner: Box<QueryRegexV1> },
    Plus { inner: Box<QueryRegexV1> },
    Opt { inner: Box<QueryRegexV1> },
}

/// Query atom in the certified core query IR (v1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueryAtomV1 {
    Type {
        term: QueryTermV1,
        type_id: u32,
    },
    AttrEq {
        term: QueryTermV1,
        key_id: u32,
        value_id: u32,
    },
    Path {
        left: QueryTermV1,
        regex: QueryRegexV1,
        right: QueryTermV1,
    },
}

/// Certified conjunctive query (v1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryV1 {
    pub select_vars: Vec<String>,
    pub atoms: Vec<QueryAtomV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_hops: Option<u32>,
    /// Optional minimum per-edge confidence threshold for path witnesses.
    ///
    /// Semantics: for every `ReachabilityProofV2::Step` used to witness a `path`
    /// atom, the step's `rel_confidence_fp` must be ≥ this value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_confidence_fp: Option<FixedPointProbability>,
}

/// A single variable binding in a query result row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryBindingV1 {
    pub var: String,
    pub entity: u32,
}

/// Witness for a single query atom under a given binding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueryAtomWitnessV1 {
    Type {
        entity: u32,
        type_id: u32,
    },
    AttrEq {
        entity: u32,
        key_id: u32,
        value_id: u32,
    },
    /// A path witness (as a reachability proof).
    ///
    /// In anchored mode, every step must include a `relation_id` fact id that
    /// the checker can validate against a `PathDBExportV1` snapshot.
    Path {
        proof: ReachabilityProofV2,
    },
}

/// One certified query result row: bindings + witnesses for each atom.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryRowV1 {
    pub bindings: Vec<QueryBindingV1>,
    pub witnesses: Vec<QueryAtomWitnessV1>,
}

/// Certified query result set (v1).
///
/// This does **not** claim completeness: it proves that each returned row
/// satisfies the query, but not that all satisfying rows were returned.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryResultProofV1 {
    pub query: QueryV1,
    pub rows: Vec<QueryRowV1>,
    pub truncated: bool,
}

/// Certified disjunctive query: a disjunction (OR) of conjunctive queries (v2).
///
/// This is the natural next step beyond conjunctive queries (CQs): **unions of
/// conjunctive queries** (UCQs). It keeps certification simple:
///
/// - a row is valid if it satisfies *one* disjunct,
/// - the certificate records which disjunct was used and provides witnesses for
///   that disjunct's atoms.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryV2 {
    pub select_vars: Vec<String>,
    /// Disjuncts (OR-branches), each a conjunction of atoms.
    pub disjuncts: Vec<Vec<QueryAtomV1>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_hops: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_confidence_fp: Option<FixedPointProbability>,
}

/// One certified query result row for a disjunctive query (v2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryRowV2 {
    /// Which disjunct in `QueryV2.disjuncts` this row satisfies.
    pub disjunct: u32,
    pub bindings: Vec<QueryBindingV1>,
    pub witnesses: Vec<QueryAtomWitnessV1>,
}

/// Certified query result set for `QueryV2` (v2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryResultProofV2 {
    pub query: QueryV2,
    pub rows: Vec<QueryRowV2>,
    pub truncated: bool,
}

// =============================================================================
// Query result certificates (v3): `.axi`-anchored, name-based
// =============================================================================

/// Certified query term (v3): name-based (canonical `.axi` anchoring).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueryTermV3 {
    Var { name: String },
    /// Constant entity identifier (stable within the anchored `.axi` meaning-plane).
    ///
    /// For `.axi`-anchored certificates we treat entity IDs as:
    /// - object element names (e.g. `"Alice"`), or
    /// - tuple fact ids (`"factfnv1a64:..."`) when referring to fact nodes.
    Const { entity: String },
}

/// Regular-path query (RPQ) expression over relation labels (v3).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueryRegexV3 {
    Epsilon,
    Rel { rel: String },
    Seq { parts: Vec<QueryRegexV3> },
    Alt { parts: Vec<QueryRegexV3> },
    Star { inner: Box<QueryRegexV3> },
    Plus { inner: Box<QueryRegexV3> },
    Opt { inner: Box<QueryRegexV3> },
}

/// Query atom in the certified core query IR (v3).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueryAtomV3 {
    Type {
        term: QueryTermV3,
        type_name: String,
    },
    AttrEq {
        term: QueryTermV3,
        key: String,
        value: String,
    },
    Path {
        left: QueryTermV3,
        regex: QueryRegexV3,
        right: QueryTermV3,
    },
}

/// Certified query IR (v3): union-of-conjunctive queries (UCQ).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryV3 {
    pub select_vars: Vec<String>,
    pub disjuncts: Vec<Vec<QueryAtomV3>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_hops: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_confidence_fp: Option<FixedPointProbability>,
}

/// A single variable binding in a query result row (v3).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryBindingV3 {
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

/// Witness for a single query atom under a given binding (v3).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueryAtomWitnessV3 {
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

/// One certified query result row for a disjunctive query (v3).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryRowV3 {
    pub disjunct: u32,
    pub bindings: Vec<QueryBindingV3>,
    pub witnesses: Vec<QueryAtomWitnessV3>,
}

/// Certified query result set (v3).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryResultProofV3 {
    pub query: QueryV3,
    pub rows: Vec<QueryRowV3>,
    pub truncated: bool,
    /// Optional rewrite-derivation witnesses for query elaboration.
    ///
    /// These are intended to justify semantics-preserving path canonicalization
    /// steps (e.g. `.axi` rewrite rules applied during elaboration).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elaboration_rewrites: Vec<RewriteDerivationProofV3>,
}

#[cfg(test)]
mod normalize_path_v2_tests {
    use super::*;

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
        let (normalized, derivation) = input.normalize_with_derivation();
        assert_eq!(normalized, expected);

        let steps = derivation.expect("should emit a derivation for this input");
        let mut current = input.clone();
        for step in steps {
            current = PathExprV2::apply_at(&current, &step.pos, &step.rule)
                .expect("rewrite step must apply");
        }
        assert_eq!(current, normalized);
    }
}
