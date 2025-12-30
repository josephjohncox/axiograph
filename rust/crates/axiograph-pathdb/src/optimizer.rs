//! Proof-producing optimizations (Rust runtime side).
//!
//! The “untrusted engine / trusted checker” pattern means we want:
//!
//! - a runtime implementation that can optimize/transform data structures, and
//! - a proof/certificate witness that a trusted checker can validate.
//!
//! This module provides a minimal, extensible scaffold:
//!
//! - **Path normalization** (free-groupoid word reduction) with explicit rewrite steps.
//! - **Reconciliation** (resolution decision) with a recomputable proof payload.
//! - A first **Δ_F (pullback)** data-migration operator on `.axi` instances.
//!
//! The concrete proof payloads are chosen to make the trusted-checker story easy:
//! in early stages, the checker can simply **recompute** the operation and compare.

#![allow(unused_imports)]

use crate::branding::DbBranded;
use crate::certificate::{
    CertificateV2, FixedProb, NormalizePathProofV2, PathEquivProofV2, PathExprV2,
    ResolutionDecisionV2, ResolutionProofV2,
};
use crate::migration::{
    ArrowMapV1, ArrowMappingV1, DeltaFMigrationProofV1, InstanceV1, ObjectElementsV1,
    ObjectMappingV1, SchemaMorphismV1, SchemaV1, SigmaFMigrationProofV1,
};
use crate::proof_mode::{ProofMode, Proved};
use crate::typestate::{NormalizedPathExprV2, UnnormalizedPathExprV2};
use crate::DbToken;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// A minimal “optimizer rule vocabulary” that can grow over time.
///
/// Today:
/// - Path rewrites are the local groupoid rules used in `normalize_path_v2`.
/// - Reconciliation is represented by the chosen decision tag.
/// - Migration operators are the categorical Δ/Σ building blocks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OptimizerRuleV1 {
    PathRewrite(crate::certificate::PathRewriteRuleV2),
    ResolutionDecision(ResolutionDecisionV2),
    Migration(MigrationOperatorV1),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum MigrationOperatorV1 {
    DeltaF,
    SigmaF,
}

// =============================================================================
// Path normalization (v2)
// =============================================================================

/// Proof-producing optimizer entrypoint (scaffold).
#[derive(Debug, Default, Clone)]
pub struct ProofProducingOptimizer;

impl ProofProducingOptimizer {
    /// Typestate wrapper for `normalize_path_v2`:
    /// consume an unnormalized path and return a `NormalizedPathExprV2`.
    pub fn normalize_path_typed_v2<M: ProofMode>(
        &self,
        input: UnnormalizedPathExprV2,
    ) -> Proved<M, NormalizedPathExprV2, NormalizePathProofV2> {
        let input = input.into_expr();
        let normalized = input.normalize();

        let proof = M::capture(|| {
            let (normalized_with_derivation, derivation) = input.normalize_with_derivation();
            debug_assert_eq!(normalized, normalized_with_derivation);
            NormalizePathProofV2 {
                input,
                normalized: normalized_with_derivation,
                derivation,
            }
        });

        Proved {
            value: NormalizedPathExprV2::new_unchecked(normalized),
            proof,
        }
    }

    /// Normalize a `PathExprV2` and (optionally) return a full `NormalizePathProofV2`.
    ///
    /// With proofs enabled:
    /// - we compute a derivation (rewrite-step list) via `normalize_with_derivation`,
    /// - and return a proof payload that Lean can replay.
    ///
    /// With proofs disabled:
    /// - we compute only the canonical normalized form.
    pub fn normalize_path_v2<M: ProofMode>(
        &self,
        input: PathExprV2,
    ) -> Proved<M, PathExprV2, NormalizePathProofV2> {
        let normalized = input.normalize();

        let proof = M::capture(|| {
            let (normalized_with_derivation, derivation) = input.normalize_with_derivation();
            debug_assert_eq!(normalized, normalized_with_derivation);
            NormalizePathProofV2 {
                input,
                normalized: normalized_with_derivation,
                derivation,
            }
        });

        Proved {
            value: normalized,
            proof,
        }
    }

    /// Normalize a `PathExprV2` and (optionally) return a DB-branded proof payload.
    ///
    /// The returned proof (when `M = WithProof`) is wrapped in `DbBranded<_>` so it
    /// cannot accidentally be reused against a different in-memory snapshot.
    pub fn normalize_path_v2_branded<M: ProofMode>(
        &self,
        db_token: DbToken,
        input: PathExprV2,
    ) -> Proved<M, PathExprV2, DbBranded<NormalizePathProofV2>> {
        let normalized = input.normalize();

        let proof = M::capture(|| {
            let (normalized_with_derivation, derivation) = input.normalize_with_derivation();
            debug_assert_eq!(normalized, normalized_with_derivation);
            DbBranded::new(
                db_token,
                NormalizePathProofV2 {
                    input,
                    normalized: normalized_with_derivation,
                    derivation,
                },
            )
        });

        Proved {
            value: normalized,
            proof,
        }
    }

    /// Normalize a `PathExprV2` and (optionally) emit a `CertificateV2` wrapper.
    pub fn normalize_path_certificate_v2<M: ProofMode>(
        &self,
        input: PathExprV2,
    ) -> Proved<M, PathExprV2, CertificateV2> {
        let normalized = input.normalize();
        let proof = M::capture(|| {
            let (normalized_with_derivation, derivation) = input.normalize_with_derivation();
            debug_assert_eq!(normalized, normalized_with_derivation);
            CertificateV2::normalize_path(NormalizePathProofV2 {
                input,
                normalized: normalized_with_derivation,
                derivation,
            })
        });
        Proved {
            value: normalized,
            proof,
        }
    }

    // =============================================================================
    // Path equivalence (v2): congruence-building block for “rewrite/groupoid semantics”
    // =============================================================================

    /// Prove that two path expressions are equivalent by normalization:
    /// they are equivalent iff they normalize to the same normal form.
    ///
    /// This returns the shared normal form as the `value`, and (optionally) a
    /// replayable `PathEquivProofV2` certificate payload.
    pub fn path_equiv_v2<M: ProofMode>(
        &self,
        left: PathExprV2,
        right: PathExprV2,
    ) -> Result<Proved<M, PathExprV2, PathEquivProofV2>> {
        let left_start = left.start();
        let left_end = left.end();
        let right_start = right.start();
        let right_end = right.end();
        if left_start != right_start || left_end != right_end {
            return Err(anyhow!(
                "path_equiv_v2: endpoint mismatch: left=({left_start},{left_end}) right=({right_start},{right_end})"
            ));
        }

        let left_norm = left.normalize();
        let right_norm = right.normalize();
        if left_norm != right_norm {
            return Err(anyhow!(
                "path_equiv_v2: not equivalent by normalization (left_norm != right_norm)"
            ));
        }
        let normalized = left_norm;

        let proof = M::capture(|| {
            let (left_norm2, left_derivation) = left.normalize_with_derivation();
            let (right_norm2, right_derivation) = right.normalize_with_derivation();
            debug_assert_eq!(left_norm2, normalized);
            debug_assert_eq!(right_norm2, normalized);

            PathEquivProofV2 {
                left,
                right,
                normalized: left_norm2,
                left_derivation,
                right_derivation,
            }
        });

        Ok(Proved {
            value: normalized,
            proof,
        })
    }

    /// Like `path_equiv_v2`, but brands the proof payload to a DB token.
    pub fn path_equiv_v2_branded<M: ProofMode>(
        &self,
        db_token: DbToken,
        left: PathExprV2,
        right: PathExprV2,
    ) -> Result<Proved<M, PathExprV2, DbBranded<PathEquivProofV2>>> {
        let left_start = left.start();
        let left_end = left.end();
        let right_start = right.start();
        let right_end = right.end();
        if left_start != right_start || left_end != right_end {
            return Err(anyhow!(
                "path_equiv_v2: endpoint mismatch: left=({left_start},{left_end}) right=({right_start},{right_end})"
            ));
        }

        let left_norm = left.normalize();
        let right_norm = right.normalize();
        if left_norm != right_norm {
            return Err(anyhow!(
                "path_equiv_v2: not equivalent by normalization (left_norm != right_norm)"
            ));
        }
        let normalized = left_norm;

        let proof = M::capture(|| {
            let (left_norm2, left_derivation) = left.normalize_with_derivation();
            let (right_norm2, right_derivation) = right.normalize_with_derivation();
            debug_assert_eq!(left_norm2, normalized);
            debug_assert_eq!(right_norm2, normalized);

            DbBranded::new(
                db_token,
                PathEquivProofV2 {
                    left,
                    right,
                    normalized: left_norm2,
                    left_derivation,
                    right_derivation,
                },
            )
        });

        Ok(Proved {
            value: normalized,
            proof,
        })
    }

    /// Prove path equivalence and (optionally) emit a `CertificateV2` wrapper.
    pub fn path_equiv_certificate_v2<M: ProofMode>(
        &self,
        left: PathExprV2,
        right: PathExprV2,
    ) -> Result<Proved<M, PathExprV2, CertificateV2>> {
        let left_start = left.start();
        let left_end = left.end();
        let right_start = right.start();
        let right_end = right.end();
        if left_start != right_start || left_end != right_end {
            return Err(anyhow!(
                "path_equiv_v2: endpoint mismatch: left=({left_start},{left_end}) right=({right_start},{right_end})"
            ));
        }

        let left_norm = left.normalize();
        let right_norm = right.normalize();
        if left_norm != right_norm {
            return Err(anyhow!(
                "path_equiv_v2: not equivalent by normalization (left_norm != right_norm)"
            ));
        }
        let normalized = left_norm;

        let proof = M::capture(|| {
            let (left_norm2, left_derivation) = left.normalize_with_derivation();
            let (right_norm2, right_derivation) = right.normalize_with_derivation();
            debug_assert_eq!(left_norm2, normalized);
            debug_assert_eq!(right_norm2, normalized);

            CertificateV2::path_equiv(PathEquivProofV2 {
                left,
                right,
                normalized: left_norm2,
                left_derivation,
                right_derivation,
            })
        });

        Ok(Proved {
            value: normalized,
            proof,
        })
    }

    /// Right congruence (post-composition / “left whiskering”):
    /// if `p ≈ q`, then `p · r ≈ q · r`.
    pub fn path_equiv_congr_right_v2<M: ProofMode>(
        &self,
        base: &PathEquivProofV2,
        r: PathExprV2,
    ) -> Result<Proved<M, PathExprV2, PathEquivProofV2>> {
        let base_end = base.left.end();
        let r_start = r.start();
        if base_end != r_start {
            return Err(anyhow!(
                "path_equiv_congr_right_v2: cannot compose: base.end={base_end} r.start={r_start}"
            ));
        }

        let left = PathExprV2::Trans {
            left: Box::new(base.left.clone()),
            right: Box::new(r.clone()),
        };
        let right = PathExprV2::Trans {
            left: Box::new(base.right.clone()),
            right: Box::new(r),
        };

        self.path_equiv_v2::<M>(left, right)
    }

    /// Like `path_equiv_congr_right_v2`, but brands the derived proof and checks
    /// that the base proof has the same DB token.
    pub fn path_equiv_congr_right_v2_branded<M: ProofMode>(
        &self,
        db_token: DbToken,
        base: &DbBranded<PathEquivProofV2>,
        r: PathExprV2,
    ) -> Result<Proved<M, PathExprV2, DbBranded<PathEquivProofV2>>> {
        let base = base.get_with_token(db_token).map_err(|e| anyhow!(e))?;
        let r_start = r.start();
        let base_end = base.left.end();
        if r_start != base_end {
            return Err(anyhow!(
                "path_equiv_congr_right_v2: cannot compose: base.end={base_end} r.start={r_start}"
            ));
        }

        let left = PathExprV2::Trans {
            left: Box::new(base.left.clone()),
            right: Box::new(r.clone()),
        };
        let right = PathExprV2::Trans {
            left: Box::new(base.right.clone()),
            right: Box::new(r),
        };

        self.path_equiv_v2_branded::<M>(db_token, left, right)
    }

    /// Left congruence (pre-composition / “right whiskering”):
    /// if `p ≈ q`, then `r · p ≈ r · q`.
    pub fn path_equiv_congr_left_v2<M: ProofMode>(
        &self,
        r: PathExprV2,
        base: &PathEquivProofV2,
    ) -> Result<Proved<M, PathExprV2, PathEquivProofV2>> {
        let r_end = r.end();
        let base_start = base.left.start();
        if r_end != base_start {
            return Err(anyhow!(
                "path_equiv_congr_left_v2: cannot compose: r.end={r_end} base.start={base_start}"
            ));
        }

        let left = PathExprV2::Trans {
            left: Box::new(r.clone()),
            right: Box::new(base.left.clone()),
        };
        let right = PathExprV2::Trans {
            left: Box::new(r),
            right: Box::new(base.right.clone()),
        };

        self.path_equiv_v2::<M>(left, right)
    }

    /// Like `path_equiv_congr_left_v2`, but brands the derived proof and checks
    /// that the base proof has the same DB token.
    pub fn path_equiv_congr_left_v2_branded<M: ProofMode>(
        &self,
        db_token: DbToken,
        r: PathExprV2,
        base: &DbBranded<PathEquivProofV2>,
    ) -> Result<Proved<M, PathExprV2, DbBranded<PathEquivProofV2>>> {
        let base = base.get_with_token(db_token).map_err(|e| anyhow!(e))?;
        let r_end = r.end();
        let base_start = base.left.start();
        if r_end != base_start {
            return Err(anyhow!(
                "path_equiv_congr_left_v2: cannot compose: r.end={r_end} base.start={base_start}"
            ));
        }

        let left = PathExprV2::Trans {
            left: Box::new(r.clone()),
            right: Box::new(base.left.clone()),
        };
        let right = PathExprV2::Trans {
            left: Box::new(r),
            right: Box::new(base.right.clone()),
        };

        self.path_equiv_v2_branded::<M>(db_token, left, right)
    }

    /// Inversion congruence: if `p ≈ q`, then `p⁻¹ ≈ q⁻¹`.
    pub fn path_equiv_congr_inv_v2<M: ProofMode>(
        &self,
        base: &PathEquivProofV2,
    ) -> Result<Proved<M, PathExprV2, PathEquivProofV2>> {
        let left = PathExprV2::Inv {
            path: Box::new(base.left.clone()),
        };
        let right = PathExprV2::Inv {
            path: Box::new(base.right.clone()),
        };
        self.path_equiv_v2::<M>(left, right)
    }

    /// Like `path_equiv_congr_inv_v2`, but brands the derived proof and checks
    /// that the base proof has the same DB token.
    pub fn path_equiv_congr_inv_v2_branded<M: ProofMode>(
        &self,
        db_token: DbToken,
        base: &DbBranded<PathEquivProofV2>,
    ) -> Result<Proved<M, PathExprV2, DbBranded<PathEquivProofV2>>> {
        let base = base.get_with_token(db_token).map_err(|e| anyhow!(e))?;
        let left = PathExprV2::Inv {
            path: Box::new(base.left.clone()),
        };
        let right = PathExprV2::Inv {
            path: Box::new(base.right.clone()),
        };
        self.path_equiv_v2_branded::<M>(db_token, left, right)
    }

    /// Decide a reconciliation action (v2) and (optionally) return a full `ResolutionProofV2`.
    pub fn resolve_conflict_v2<M: ProofMode>(
        &self,
        first_confidence_fp: FixedProb,
        second_confidence_fp: FixedProb,
        threshold_fp: FixedProb,
    ) -> Proved<M, ResolutionDecisionV2, ResolutionProofV2> {
        let proof_payload =
            ResolutionProofV2::decide(first_confidence_fp, second_confidence_fp, threshold_fp);
        let decision = proof_payload.decision.clone();

        let proof = M::capture(|| proof_payload);
        Proved {
            value: decision,
            proof,
        }
    }

    /// Decide a reconciliation action (v2) and (optionally) return a DB-branded proof payload.
    pub fn resolve_conflict_v2_branded<M: ProofMode>(
        &self,
        db_token: DbToken,
        first_confidence_fp: FixedProb,
        second_confidence_fp: FixedProb,
        threshold_fp: FixedProb,
    ) -> Proved<M, ResolutionDecisionV2, DbBranded<ResolutionProofV2>> {
        let proof_payload =
            ResolutionProofV2::decide(first_confidence_fp, second_confidence_fp, threshold_fp);
        let decision = proof_payload.decision.clone();

        let proof = M::capture(|| DbBranded::new(db_token, proof_payload));
        Proved {
            value: decision,
            proof,
        }
    }

    /// Decide a reconciliation action (v2) and (optionally) emit a `CertificateV2` wrapper.
    pub fn resolve_conflict_certificate_v2<M: ProofMode>(
        &self,
        first_confidence_fp: FixedProb,
        second_confidence_fp: FixedProb,
        threshold_fp: FixedProb,
    ) -> Proved<M, ResolutionDecisionV2, CertificateV2> {
        let proof_payload =
            ResolutionProofV2::decide(first_confidence_fp, second_confidence_fp, threshold_fp);
        let decision = proof_payload.decision.clone();
        let proof = M::capture(|| CertificateV2::resolution(proof_payload));
        Proved {
            value: decision,
            proof,
        }
    }

    // =============================================================================
    // Δ_F / Σ_F schema migration (v1 scaffold)
    // =============================================================================

    /// Compute the pullback (Δ_F) of an instance along a schema morphism.
    ///
    /// This is the categorical “query” operator:
    /// given a functor `F : S₁ → S₂` and an instance `I : S₂ → Set`,
    /// produce `Δ_F(I) = I ∘ F : S₁ → Set`.
    ///
    /// Current implementation covers:
    /// - object carriers (copy element sets along `F`),
    /// - arrow carriers (compose arrow functions along `F`’s path mapping),
    /// - subtype inclusions are treated as arrows (by name) and are migrated the same way.
    ///
    /// Relations/tables are left empty for now; the long-term plan is to represent relations
    /// as objects + projection arrows in the core schema semantics so they migrate uniformly.
    pub fn delta_f_v1<M: ProofMode>(
        &self,
        morphism: SchemaMorphismV1,
        source_schema: SchemaV1,
        target_instance: InstanceV1,
    ) -> Result<Proved<M, InstanceV1, DeltaFMigrationProofV1>> {
        let pulled_back = delta_f_compute(&morphism, &source_schema, &target_instance)?;

        let proof = M::capture(|| DeltaFMigrationProofV1 {
            morphism,
            source_schema,
            target_instance,
            pulled_back_instance: pulled_back.clone(),
        });

        Ok(Proved {
            value: pulled_back,
            proof,
        })
    }

    /// Compute Δ_F and (optionally) emit a `CertificateV2` wrapper (`kind = delta_f_v1`).
    pub fn delta_f_certificate_v1<M: ProofMode>(
        &self,
        morphism: SchemaMorphismV1,
        source_schema: SchemaV1,
        target_instance: InstanceV1,
    ) -> Result<Proved<M, InstanceV1, CertificateV2>> {
        let pulled_back = delta_f_compute(&morphism, &source_schema, &target_instance)?;

        let proof = M::capture(|| {
            CertificateV2::delta_f_v1(DeltaFMigrationProofV1 {
                morphism,
                source_schema,
                target_instance,
                pulled_back_instance: pulled_back.clone(),
            })
        });

        Ok(Proved {
            value: pulled_back,
            proof,
        })
    }

    /// Left pushforward (Σ_F) scaffold.
    ///
    /// In general Σ_F is a left Kan extension and may require:
    /// - generating new IDs,
    /// - quotienting/identifying entities,
    /// - aggregation/colimits.
    ///
    /// We leave this as an explicit TODO so callers can still model the pipeline shape.
    pub fn sigma_f_v1<M: ProofMode>(
        &self,
        _morphism: SchemaMorphismV1,
        _source_instance: InstanceV1,
    ) -> Result<Proved<M, InstanceV1, SigmaFMigrationProofV1>> {
        Err(anyhow!(
            "sigma_f_v1 is not implemented yet (planned: left Kan extension / aggregation)"
        ))
    }
}

fn delta_f_compute(
    morphism: &SchemaMorphismV1,
    source_schema: &SchemaV1,
    target_instance: &InstanceV1,
) -> Result<InstanceV1> {
    if morphism.source_schema != source_schema.name {
        return Err(anyhow!(
            "delta_f: morphism.source_schema={} does not match source_schema.name={}",
            morphism.source_schema,
            source_schema.name
        ));
    }
    if morphism.target_schema != target_instance.schema {
        return Err(anyhow!(
            "delta_f: morphism.target_schema={} does not match target_instance.schema={}",
            morphism.target_schema,
            target_instance.schema
        ));
    }

    let target_object_elems: HashMap<&str, &Vec<String>> = target_instance
        .objects
        .iter()
        .map(|o| (o.obj.as_str(), &o.elems))
        .collect();

    let mut output_objects: Vec<ObjectElementsV1> = Vec::new();
    for source_object in &source_schema.objects {
        let Some(target_object) = morphism.object_image(source_object) else {
            return Err(anyhow!(
                "delta_f: missing object mapping for source object `{source_object}`"
            ));
        };
        let Some(elems) = target_object_elems.get(target_object) else {
            return Err(anyhow!(
                "delta_f: target instance missing elements for mapped object `{target_object}`"
            ));
        };
        output_objects.push(ObjectElementsV1 {
            obj: source_object.clone(),
            elems: (*elems).clone(),
        });
    }

    let target_arrow_functions = build_arrow_functions(target_instance)?;

    let mut output_arrows: Vec<ArrowMapV1> = Vec::new();
    let mut seen_arrow_names: HashSet<&str> = HashSet::new();

    for arrow in &source_schema.arrows {
        let source_arrow_name = &arrow.name;
        let source_src_object = &arrow.src;
        let source_dst_object = &arrow.dst;
        if !seen_arrow_names.insert(source_arrow_name.as_str()) {
            return Err(anyhow!(
                "delta_f: duplicate arrow name `{source_arrow_name}` in source schema"
            ));
        }

        let Some(target_src_object) = morphism.object_image(source_src_object) else {
            return Err(anyhow!(
                "delta_f: missing object mapping for source object `{source_src_object}`"
            ));
        };
        let Some(target_dst_object) = morphism.object_image(source_dst_object) else {
            return Err(anyhow!(
                "delta_f: missing object mapping for source object `{source_dst_object}`"
            ));
        };

        let Some(target_path) = morphism.arrow_image(source_arrow_name) else {
            return Err(anyhow!(
                "delta_f: missing arrow mapping for source arrow `{source_arrow_name}`"
            ));
        };

        let Some(domain_elems) = target_object_elems.get(target_src_object) else {
            return Err(anyhow!(
                "delta_f: target instance missing elements for mapped object `{target_src_object}`"
            ));
        };
        let Some(codomain_elems) = target_object_elems.get(target_dst_object) else {
            return Err(anyhow!(
                "delta_f: target instance missing elements for mapped object `{target_dst_object}`"
            ));
        };
        let codomain_set: HashSet<&str> = codomain_elems.iter().map(|s| s.as_str()).collect();

        if target_path.is_empty() && target_src_object != target_dst_object {
            return Err(anyhow!(
                "delta_f: arrow `{source_arrow_name}` maps to identity path but object images differ ({target_src_object} ≠ {target_dst_object})"
            ));
        }

        let mut pairs: Vec<(String, String)> = Vec::with_capacity(domain_elems.len());
        for domain_elem in domain_elems.iter() {
            let image = apply_arrow_path(&target_arrow_functions, domain_elem, target_path)
                .map_err(|e| anyhow!("delta_f: {e}"))?;

            if !codomain_set.contains(image.as_str()) {
                return Err(anyhow!(
                    "delta_f: arrow `{source_arrow_name}` maps `{}` to `{}`, but `{}` is not in the codomain object `{target_dst_object}`",
                    domain_elem,
                    image,
                    image
                ));
            }

            pairs.push((domain_elem.clone(), image));
        }

        output_arrows.push(ArrowMapV1 {
            arrow: source_arrow_name.clone(),
            pairs,
        });
    }

    Ok(InstanceV1 {
        name: format!("{}_delta_f", target_instance.name),
        schema: source_schema.name.clone(),
        objects: output_objects,
        arrows: output_arrows,
    })
}

fn build_arrow_functions(instance: &InstanceV1) -> Result<HashMap<&str, HashMap<&str, &str>>> {
    let mut functions: HashMap<&str, HashMap<&str, &str>> = HashMap::new();

    for entry in &instance.arrows {
        let mut mapping: HashMap<&str, &str> = HashMap::new();
        for (src, dst) in &entry.pairs {
            if mapping.insert(src.as_str(), dst.as_str()).is_some() {
                return Err(anyhow!(
                    "delta_f: duplicate mapping for arrow `{}` at source element `{}`",
                    entry.arrow,
                    src
                ));
            }
        }
        functions.insert(entry.arrow.as_str(), mapping);
    }

    Ok(functions)
}

fn apply_arrow_path(
    arrow_functions: &HashMap<&str, HashMap<&str, &str>>,
    start: &String,
    path: &[String],
) -> Result<String> {
    let mut current: &str = start.as_str();

    for arrow_name in path {
        let Some(f) = arrow_functions.get(arrow_name.as_str()) else {
            return Err(anyhow!("missing arrow function for `{}`", arrow_name));
        };
        let Some(next) = f.get(current) else {
            return Err(anyhow!(
                "arrow `{}` missing mapping for input element `{}`",
                arrow_name,
                current
            ));
        };
        current = next;
    }

    Ok(current.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proof_mode::{NoProof, WithProof};

    #[test]
    fn optimizer_normalize_path_v2_produces_proof_in_with_proof_mode() {
        let optimizer = ProofProducingOptimizer::default();

        let input = PathExprV2::Trans {
            left: Box::new(PathExprV2::Inv {
                path: Box::new(PathExprV2::Trans {
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
            }),
            right: Box::new(PathExprV2::Trans {
                left: Box::new(PathExprV2::Step {
                    from: 3,
                    rel_type: 30,
                    to: 4,
                }),
                right: Box::new(PathExprV2::Inv {
                    path: Box::new(PathExprV2::Step {
                        from: 3,
                        rel_type: 30,
                        to: 4,
                    }),
                }),
            }),
        };

        let proved = optimizer.normalize_path_v2::<WithProof>(input.clone());
        assert_eq!(proved.value, input.normalize());
        assert_eq!(proved.proof.normalized, proved.value);
        assert!(proved.proof.derivation.is_some());

        let proved_no = optimizer.normalize_path_v2::<NoProof>(input);
        let _: () = proved_no.proof;
    }

    #[test]
    fn branded_optimizer_proofs_cannot_cross_db_tokens() {
        let optimizer = ProofProducingOptimizer::default();
        let db1 = DbToken::new();
        let db2 = DbToken::new();

        let input = PathExprV2::Trans {
            left: Box::new(PathExprV2::Step {
                from: 1,
                rel_type: 10,
                to: 2,
            }),
            right: Box::new(PathExprV2::Inv {
                path: Box::new(PathExprV2::Step {
                    from: 1,
                    rel_type: 10,
                    to: 2,
                }),
            }),
        };

        let proved = optimizer.normalize_path_v2_branded::<WithProof>(db1, input);
        assert!(proved.proof.assert_token(db1).is_ok());
        assert!(proved.proof.assert_token(db2).is_err());
    }

    #[test]
    fn branded_equivalence_proofs_cannot_cross_db_tokens() {
        let optimizer = ProofProducingOptimizer::default();
        let db1 = DbToken::new();
        let db2 = DbToken::new();

        let left = PathExprV2::Step {
            from: 1,
            rel_type: 10,
            to: 2,
        };
        let right = PathExprV2::Trans {
            left: Box::new(PathExprV2::Reflexive { entity: 1 }),
            right: Box::new(PathExprV2::Step {
                from: 1,
                rel_type: 10,
                to: 2,
            }),
        };

        let proved = optimizer
            .path_equiv_v2_branded::<WithProof>(db1, left, right)
            .expect("equivalence should hold");
        assert!(proved.proof.assert_token(db1).is_ok());
        assert!(proved.proof.assert_token(db2).is_err());
    }

    #[test]
    fn branded_reconciliation_proofs_cannot_cross_db_tokens() {
        let optimizer = ProofProducingOptimizer::default();
        let db1 = DbToken::new();
        let db2 = DbToken::new();

        let proved = optimizer.resolve_conflict_v2_branded::<WithProof>(
            db1,
            FixedProb::new_unchecked(900_000),
            FixedProb::new_unchecked(850_000),
            FixedProb::new_unchecked(800_000),
        );
        assert!(proved.proof.assert_token(db1).is_ok());
        assert!(proved.proof.assert_token(db2).is_err());
    }

    #[test]
    fn path_equiv_congruence_builders_produce_valid_equivalence_proofs() {
        let optimizer = ProofProducingOptimizer::default();

        // Base equivalence: two different spellings of `p ; q`.
        let p = PathExprV2::Step {
            from: 1,
            rel_type: 10,
            to: 2,
        };
        let q = PathExprV2::Step {
            from: 2,
            rel_type: 20,
            to: 3,
        };

        let left0 = PathExprV2::Trans {
            left: Box::new(PathExprV2::Trans {
                left: Box::new(PathExprV2::Trans {
                    left: Box::new(PathExprV2::Reflexive { entity: 1 }),
                    right: Box::new(p.clone()),
                }),
                right: Box::new(q.clone()),
            }),
            right: Box::new(PathExprV2::Reflexive { entity: 3 }),
        };

        let right0 = PathExprV2::Trans {
            left: Box::new(p),
            right: Box::new(PathExprV2::Trans {
                left: Box::new(PathExprV2::Reflexive { entity: 2 }),
                right: Box::new(q),
            }),
        };

        let base = optimizer
            .path_equiv_v2::<WithProof>(left0.clone(), right0.clone())
            .expect("base equivalence should hold");
        assert_eq!(base.value, left0.normalize());
        assert_eq!(base.value, right0.normalize());
        assert_eq!(base.proof.normalized, base.value);
        let base_normalized = base.value.clone();

        // Post-compose with `r`.
        let r = PathExprV2::Step {
            from: 3,
            rel_type: 30,
            to: 4,
        };
        let post = optimizer
            .path_equiv_congr_right_v2::<WithProof>(&base.proof, r.clone())
            .expect("congruence-right should preserve equivalence");
        assert_eq!(post.proof.normalized, post.value);
        assert_eq!(
            post.value,
            PathExprV2::Trans {
                left: Box::new(base_normalized.clone()),
                right: Box::new(r.clone()),
            }
            .normalize()
        );

        // Pre-compose with `l`.
        let l = PathExprV2::Step {
            from: 0,
            rel_type: 5,
            to: 1,
        };
        let pre = optimizer
            .path_equiv_congr_left_v2::<WithProof>(l.clone(), &base.proof)
            .expect("congruence-left should preserve equivalence");
        assert_eq!(pre.proof.normalized, pre.value);
        assert_eq!(
            pre.value,
            PathExprV2::Trans {
                left: Box::new(l),
                right: Box::new(base_normalized.clone()),
            }
            .normalize()
        );

        // Invert the equivalence.
        let inv = optimizer
            .path_equiv_congr_inv_v2::<WithProof>(&base.proof)
            .expect("congruence-inv should preserve equivalence");
        assert_eq!(inv.proof.normalized, inv.value);
        assert_eq!(
            inv.value,
            PathExprV2::Inv {
                path: Box::new(base_normalized.clone())
            }
            .normalize()
        );

        // Sanity: NoProof mode still checks equivalence but does not construct payloads.
        let none = optimizer
            .path_equiv_congr_right_v2::<NoProof>(&base.proof, r)
            .expect("NoProof congruence-right should still succeed");
        let _: () = none.proof;
    }

    #[test]
    fn branded_congruence_rejects_mismatched_base_proof_token() {
        let optimizer = ProofProducingOptimizer::default();
        let db1 = DbToken::new();
        let db2 = DbToken::new();

        let p = PathExprV2::Step {
            from: 1,
            rel_type: 10,
            to: 2,
        };
        let q = PathExprV2::Step {
            from: 2,
            rel_type: 20,
            to: 3,
        };

        // `p ; q`
        let left0 = PathExprV2::Trans {
            left: Box::new(p.clone()),
            right: Box::new(q.clone()),
        };

        // `id ; (p ; q)`
        let right0 = PathExprV2::Trans {
            left: Box::new(PathExprV2::Reflexive { entity: 1 }),
            right: Box::new(PathExprV2::Trans {
                left: Box::new(p),
                right: Box::new(q),
            }),
        };

        let base = optimizer
            .path_equiv_v2_branded::<WithProof>(db1, left0, right0)
            .expect("base equivalence should hold");

        let r = PathExprV2::Step {
            from: 3,
            rel_type: 30,
            to: 4,
        };

        let err = optimizer
            .path_equiv_congr_right_v2_branded::<WithProof>(db2, &base.proof, r)
            .unwrap_err();
        assert!(err.to_string().contains("db token mismatch"));
    }

    #[test]
    fn delta_f_copies_objects_and_composes_arrows() {
        let optimizer = ProofProducingOptimizer::default();

        let source_schema = SchemaV1 {
            name: "S1".to_string(),
            objects: vec!["A".to_string(), "B".to_string()],
            arrows: vec![crate::migration::ArrowDeclV1 {
                name: "f".to_string(),
                src: "A".to_string(),
                dst: "B".to_string(),
            }],
            subtypes: vec![],
        };

        let target_instance = InstanceV1 {
            name: "I2".to_string(),
            schema: "S2".to_string(),
            objects: vec![
                ObjectElementsV1 {
                    obj: "X".to_string(),
                    elems: vec!["x1".to_string(), "x2".to_string()],
                },
                ObjectElementsV1 {
                    obj: "Y".to_string(),
                    elems: vec!["y1".to_string(), "y2".to_string()],
                },
            ],
            arrows: vec![ArrowMapV1 {
                arrow: "g".to_string(),
                pairs: vec![
                    ("x1".to_string(), "y1".to_string()),
                    ("x2".to_string(), "y2".to_string()),
                ],
            }],
        };

        let morphism = SchemaMorphismV1 {
            source_schema: "S1".to_string(),
            target_schema: "S2".to_string(),
            objects: vec![
                ObjectMappingV1 {
                    source_object: "A".to_string(),
                    target_object: "X".to_string(),
                },
                ObjectMappingV1 {
                    source_object: "B".to_string(),
                    target_object: "Y".to_string(),
                },
            ],
            arrows: vec![ArrowMappingV1 {
                source_arrow: "f".to_string(),
                target_path: vec!["g".to_string()],
            }],
        };

        let proved = optimizer
            .delta_f_v1::<WithProof>(
                morphism.clone(),
                source_schema.clone(),
                target_instance.clone(),
            )
            .expect("delta_f should succeed");
        assert_eq!(proved.value.schema, "S1");
        assert_eq!(proved.value.objects.len(), 2);
        assert_eq!(proved.value.arrows.len(), 1);

        let f_map = &proved.value.arrows[0];
        assert_eq!(f_map.arrow, "f");
        assert_eq!(
            f_map.pairs,
            vec![
                ("x1".to_string(), "y1".to_string()),
                ("x2".to_string(), "y2".to_string())
            ]
        );

        // Proof payload is self-contained for recomputation later.
        assert_eq!(proved.proof.morphism.source_schema, "S1");
        assert_eq!(proved.proof.morphism.target_schema, "S2");
        assert_eq!(proved.proof.source_schema.name, "S1");
        assert_eq!(proved.proof.target_instance.schema, "S2");

        let proved_no = optimizer
            .delta_f_v1::<NoProof>(morphism, source_schema, target_instance)
            .expect("delta_f should succeed");
        let _: () = proved_no.proof;
    }
}
