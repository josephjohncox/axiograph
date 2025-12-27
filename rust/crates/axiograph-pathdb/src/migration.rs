//! Schema migration semantics scaffolding (Δ/Σ/Π).
//!
//! This module holds **shared, serializable** data structures that are used by:
//!
//! - the Rust runtime (to compute migrations), and
//! - the certificate layer (to emit witnesses to the trusted checker).
//!
//! We keep these types *out of* the optimizer/certificate modules to avoid
//! circular dependencies: both sides can depend on `migration`.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

pub type Name = String;

// =============================================================================
// Minimal categorical schema/instance IR (for Δ_F/Σ_F)
// =============================================================================

/// A minimal schema IR for migration semantics.
///
/// This is intentionally **not** the legacy `.axi` AST: migration needs a
/// category-shaped core (objects + arrows + functions), while the canonical
/// `axi_schema_v1` surface syntax is relation-oriented.
///
/// Long-term plan: express relations as objects + projection arrows so the
/// migration operators work uniformly. For now, we keep this IR small and
/// explicit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemaV1 {
    pub name: Name,
    pub objects: Vec<Name>,
    pub arrows: Vec<ArrowDeclV1>,
    /// Subtype declarations (optional; empty for the minimal migration IR).
    ///
    /// We keep this field in the certificate payload so the trusted Lean checker
    /// can share a stable shape with `.axi` schema parsing during migration.
    pub subtypes: Vec<SubtypeDeclV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArrowDeclV1 {
    pub name: Name,
    pub src: Name,
    pub dst: Name,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubtypeDeclV1 {
    pub sub: Name,
    pub sup: Name,
    pub incl: Name,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstanceV1 {
    pub name: Name,
    pub schema: Name,
    pub objects: Vec<ObjectElementsV1>,
    pub arrows: Vec<ArrowMapV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectElementsV1 {
    pub obj: Name,
    pub elems: Vec<Name>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArrowMapV1 {
    pub arrow: Name,
    pub pairs: Vec<(Name, Name)>,
}

/// A schema morphism (functor) `F : source_schema → target_schema`.
///
/// - `objects` maps source objects to target objects.
/// - `arrows` maps each source arrow (by name) to a *path* (list) of target arrows.
///
/// This is intentionally the minimal data needed to compute Δ_F on `.axi` instances.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemaMorphismV1 {
    pub source_schema: Name,
    pub target_schema: Name,
    pub objects: Vec<ObjectMappingV1>,
    pub arrows: Vec<ArrowMappingV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectMappingV1 {
    pub source_object: Name,
    pub target_object: Name,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArrowMappingV1 {
    pub source_arrow: Name,
    /// Target path expressed as a list of target arrow names (empty means identity).
    pub target_path: Vec<Name>,
}

impl SchemaMorphismV1 {
    pub fn object_image(&self, source_object: &str) -> Option<&str> {
        self.objects
            .iter()
            .find(|m| m.source_object == source_object)
            .map(|m| m.target_object.as_str())
    }

    pub fn arrow_image(&self, source_arrow: &str) -> Option<&[Name]> {
        self.arrows
            .iter()
            .find(|m| m.source_arrow == source_arrow)
            .map(|m| m.target_path.as_slice())
    }

    /// Compose schema morphisms (functors) by “pushing paths forward”.
    ///
    /// If `self : S₀ → S₁` and `after : S₁ → S₂`, then `self.then(after)` returns
    /// `after ∘ self : S₀ → S₂`.
    ///
    /// Composition rules:
    /// - Object images compose pointwise.
    /// - Arrow images compose by concatenating mapped paths:
    ///   if `self(f) = [g₁, g₂]` and `after(g₁) = p₁`, `after(g₂) = p₂`, then
    ///   `(after ∘ self)(f) = p₁ ++ p₂`.
    pub fn then(&self, after: &SchemaMorphismV1) -> Result<SchemaMorphismV1> {
        if self.target_schema != after.source_schema {
            return Err(anyhow!(
                "cannot compose schema morphisms: self.target_schema={} but after.source_schema={}",
                self.target_schema,
                after.source_schema
            ));
        }

        let mut composed_objects: Vec<ObjectMappingV1> = Vec::with_capacity(self.objects.len());
        for mapping in &self.objects {
            let Some(target_object) = after.object_image(mapping.target_object.as_str()) else {
                return Err(anyhow!(
                    "cannot compose: missing object mapping for intermediate object `{}`",
                    mapping.target_object
                ));
            };
            composed_objects.push(ObjectMappingV1 {
                source_object: mapping.source_object.clone(),
                target_object: target_object.to_string(),
            });
        }

        let mut composed_arrows: Vec<ArrowMappingV1> = Vec::with_capacity(self.arrows.len());
        for mapping in &self.arrows {
            let mut composed_path: Vec<Name> = Vec::new();
            for intermediate_arrow in &mapping.target_path {
                let Some(after_path) = after.arrow_image(intermediate_arrow.as_str()) else {
                    return Err(anyhow!(
                        "cannot compose: missing arrow mapping for intermediate arrow `{}`",
                        intermediate_arrow
                    ));
                };
                composed_path.extend(after_path.iter().cloned());
            }
            composed_arrows.push(ArrowMappingV1 {
                source_arrow: mapping.source_arrow.clone(),
                target_path: composed_path,
            });
        }

        Ok(SchemaMorphismV1 {
            source_schema: self.source_schema.clone(),
            target_schema: after.target_schema.clone(),
            objects: composed_objects,
            arrows: composed_arrows,
        })
    }
}

/// Proof payload for Δ_F (v1 scaffold).
///
/// A future Lean checker can validate this by recomputing Δ_F
/// from `(morphism, source_schema, target_instance)` and comparing the result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeltaFMigrationProofV1 {
    pub morphism: SchemaMorphismV1,
    pub source_schema: SchemaV1,
    pub target_instance: InstanceV1,
    pub pulled_back_instance: InstanceV1,
}

/// Proof payload for Σ_F (placeholder).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SigmaFMigrationProofV1 {
    pub morphism: SchemaMorphismV1,
    pub source_instance: InstanceV1,
    pub migrated_instance: InstanceV1,
}
