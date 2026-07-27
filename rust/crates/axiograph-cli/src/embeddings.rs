//! Materialization-scoped embedding evidence artifacts.
//!
//! Embeddings stay outside the trusted kernel. Sidecars bind to accepted `.axi`
//! anchors and, when available, an authenticated SQLite materialization id. They
//! are advisory evidence and are never stored inside or used to recover `.axpd`.
//!
//! See `docs/reference/EMBEDDINGS_AND_EVIDENCE.md` for the trust contract:
//! embedding-derived relationships are weak evidence/proposal overlays until
//! typed validation and review promote them into canonical `.axi`.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use axiograph_kernel::MaterializationIdV2;
use axiograph_pathdb::{AcceptedAxiAnchor, DbToken};

pub const EMBEDDINGS_FILE_VERSION_V1: &str = "axiograph_embeddings_v1";
pub const EMBEDDING_SIDECAR_MANIFEST_VERSION_V1: &str = "axiograph_embedding_sidecar_manifest_v1";
pub const EMBEDDING_EVIDENCE_OVERLAY_VERSION_V1: &str = "axiograph_embedding_evidence_overlay_v1";
pub const EMBEDDING_RELATIONSHIP_DISCOVERY_METHOD_V1: &str =
    "deterministic_pairwise_cosine_tiny_vectors_v1";
pub const EMBEDDING_SIMILARITY_OBSERVATION_METHOD_V1: &str =
    "deterministic_pairwise_cosine_top_pairs_v1";
const MAX_EMBEDDINGS_FILE_BYTES: usize = 64 * 1024 * 1024;
const MAX_EMBEDDING_DIM: usize = 8_192;
const MAX_EMBEDDING_ITEMS: usize = 100_000;
const MAX_EMBEDDING_COMPONENTS: usize = 4_000_000;
const MAX_EMBEDDING_METADATA_ENTRIES: usize = 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingTargetKindV1 {
    DocChunks,
    Entities,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EmbeddingKeyV1 {
    DocChunk { chunk_id: String },
    Entity { entity_type: String, name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingItemV1 {
    pub key: EmbeddingKeyV1,
    pub vector: Vec<f32>,
    #[serde(default)]
    pub text_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingsFileV1 {
    pub version: String,
    pub created_at_unix_secs: u64,
    pub backend: String,
    pub model: String,
    pub dim: usize,
    pub target: EmbeddingTargetKindV1,
    pub items: Vec<EmbeddingItemV1>,
    /// Optional free-form metadata (e.g. prompt template, truncation settings).
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

// ============================================================================
// Reviewable embedding sidecar metadata and evidence overlays
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingAcceptedRefV1 {
    /// Review ref used when the sidecar was built.
    pub accepted_ref: String,
    /// Carries the accepted snapshot id and canonical module digest.
    pub accepted_axi_anchor: AcceptedAxiAnchor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiled_ir_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingModelRefV1 {
    pub backend: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployment_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingNormalizationPolicyV1 {
    None,
    UnitL2,
    BackendDefault,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingSidecarAuthorityV1 {
    EvidenceIndexOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingPromotionPolicyV1 {
    RequiresTypedValidationReviewCqReconciliationPromotion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingSidecarTrustV1 {
    pub authority: EmbeddingSidecarAuthorityV1,
    pub promotion_policy: EmbeddingPromotionPolicyV1,
    /// Human-readable caveats surfaced to agents and reviewers.
    pub caveats: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingTargetIdentityV1 {
    /// Stable id local to the sidecar/manifest, used by overlay observations.
    pub target_id: String,
    pub key: EmbeddingKeyV1,
    /// Optional accepted `.axi` or compiled-IR object/relation/chunk ref.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typed_ref: Option<String>,
    /// Digest of the exact text used to produce this target embedding.
    pub source_text_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingSidecarManifestV1 {
    pub version: String,
    pub sidecar_id: String,
    pub created_at_unix_secs: u64,
    pub accepted: EmbeddingAcceptedRefV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub materialization_id: Option<MaterializationIdV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embeddings_file_digest: Option<String>,
    pub source_model: EmbeddingModelRefV1,
    pub target_kind: EmbeddingTargetKindV1,
    pub dim: usize,
    pub normalization: EmbeddingNormalizationPolicyV1,
    pub targets: Vec<EmbeddingTargetIdentityV1>,
    pub trust: EmbeddingSidecarTrustV1,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingSimilarityMetricV1 {
    Cosine,
    Dot,
    EuclideanDistance,
    RerankScore,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingSimilarityObservationV1 {
    pub observation_id: String,
    pub left: EmbeddingTargetIdentityV1,
    pub right: EmbeddingTargetIdentityV1,
    pub metric: EmbeddingSimilarityMetricV1,
    pub score: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<u32>,
    pub method: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub caveats: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingRelationshipKindV1 {
    SimilarTo,
    Supports,
    Mentions,
    Implements,
    Violates,
    SubtypeCandidate,
    SameAsCandidate,
    RelationCandidate,
    AxiomCandidate,
    Contradicts,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingRelationshipEvidenceV1 {
    pub evidence_id: String,
    pub relationship: EmbeddingRelationshipKindV1,
    pub subject: EmbeddingTargetIdentityV1,
    pub object: EmbeddingTargetIdentityV1,
    /// Optional model score. This is advisory evidence, not a semantic proof.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_observation_ids: Vec<String>,
    pub method: String,
    pub advisory_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typed_relation_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_refinement_handles: Vec<crate::typed_refinement::RuntimeRefinementHandleV2>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub caveats: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingEvidenceOverlayV1 {
    pub version: String,
    pub overlay_id: String,
    pub created_at_unix_secs: u64,
    pub manifest_sidecar_id: String,
    pub accepted: EmbeddingAcceptedRefV1,
    pub source_model: EmbeddingModelRefV1,
    pub trust: EmbeddingSidecarTrustV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observations: Vec<EmbeddingSimilarityObservationV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<EmbeddingRelationshipEvidenceV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proposal_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct EmbeddingSidecarManifestBuildInputV1 {
    pub accepted: EmbeddingAcceptedRefV1,
    pub sidecar_id: Option<String>,
    pub created_at_unix_secs: Option<u64>,
    pub materialization_id: Option<MaterializationIdV2>,
    pub embeddings_file_digest: Option<String>,
    pub model_version: Option<String>,
    pub model_digest: Option<String>,
    pub deployment_id: Option<String>,
    pub normalization: EmbeddingNormalizationPolicyV1,
    pub trust: Option<EmbeddingSidecarTrustV1>,
    pub metadata: HashMap<String, String>,
}

impl EmbeddingSidecarManifestBuildInputV1 {
    pub fn new(accepted: EmbeddingAcceptedRefV1) -> Self {
        Self {
            accepted,
            sidecar_id: None,
            created_at_unix_secs: None,
            materialization_id: None,
            embeddings_file_digest: None,
            model_version: None,
            model_digest: None,
            deployment_id: None,
            normalization: EmbeddingNormalizationPolicyV1::Unknown,
            trust: None,
            metadata: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EmbeddingRelationshipDiscoveryConfigV1 {
    pub overlay_id: Option<String>,
    pub created_at_unix_secs: Option<u64>,
    pub min_cosine_similarity: f32,
    pub max_relationships: usize,
    pub relationship: EmbeddingRelationshipKindV1,
    pub observation_method: String,
    pub relationship_method: String,
    pub metadata: HashMap<String, String>,
}

impl Default for EmbeddingRelationshipDiscoveryConfigV1 {
    fn default() -> Self {
        Self {
            overlay_id: None,
            created_at_unix_secs: None,
            min_cosine_similarity: 0.75,
            max_relationships: 32,
            relationship: EmbeddingRelationshipKindV1::SimilarTo,
            observation_method: EMBEDDING_SIMILARITY_OBSERVATION_METHOD_V1.to_string(),
            relationship_method: EMBEDDING_RELATIONSHIP_DISCOVERY_METHOD_V1.to_string(),
            metadata: HashMap::new(),
        }
    }
}

pub fn default_embedding_sidecar_trust_v1() -> EmbeddingSidecarTrustV1 {
    EmbeddingSidecarTrustV1 {
        authority: EmbeddingSidecarAuthorityV1::EvidenceIndexOnly,
        promotion_policy:
            EmbeddingPromotionPolicyV1::RequiresTypedValidationReviewCqReconciliationPromotion,
        caveats: vec![
            "embedding scores are advisory evidence, not accepted .axi truth".to_string(),
            "ontology mutation requires typed validation, review, CQ gates, reconciliation, and promotion"
                .to_string(),
        ],
    }
}

pub fn validate_embeddings_file_v1(file: &EmbeddingsFileV1) -> Result<()> {
    ensure_eq(
        &file.version,
        EMBEDDINGS_FILE_VERSION_V1,
        "embeddings file version",
    )?;
    ensure_non_empty(&file.backend, "embeddings_file.backend")?;
    ensure_non_empty(&file.model, "embeddings_file.model")?;
    if file.dim == 0 || file.dim > MAX_EMBEDDING_DIM {
        return Err(anyhow!(
            "embeddings file dim must be in 1..={MAX_EMBEDDING_DIM}"
        ));
    }
    if file.items.is_empty() || file.items.len() > MAX_EMBEDDING_ITEMS {
        return Err(anyhow!(
            "embeddings file item count must be in 1..={MAX_EMBEDDING_ITEMS}"
        ));
    }
    if file.metadata.len() > MAX_EMBEDDING_METADATA_ENTRIES {
        return Err(anyhow!(
            "embeddings metadata count exceeds {MAX_EMBEDDING_METADATA_ENTRIES}"
        ));
    }
    let component_count = file
        .items
        .len()
        .checked_mul(file.dim)
        .ok_or_else(|| anyhow!("embedding component count overflow"))?;
    if component_count > MAX_EMBEDDING_COMPONENTS {
        return Err(anyhow!(
            "embedding component count {component_count} exceeds {MAX_EMBEDDING_COMPONENTS}"
        ));
    }

    let mut keys = BTreeSet::new();
    for (idx, item) in file.items.iter().enumerate() {
        validate_embedding_key_for_target_kind_v1(
            &item.key,
            file.target,
            &format!("embeddings_file.items[{idx}].key"),
        )?;
        let stable_key = embedding_key_stable_material_v1(&item.key)?;
        if !keys.insert(stable_key) {
            return Err(anyhow!(
                "embeddings file item key is duplicated at index {idx}"
            ));
        }
        if item.vector.len() != file.dim {
            return Err(anyhow!(
                "embeddings file item at index {idx} has wrong dim: expected {} got {}",
                file.dim,
                item.vector.len()
            ));
        }
        for (component_idx, component) in item.vector.iter().enumerate() {
            if !component.is_finite() {
                return Err(anyhow!(
                    "embeddings file item at index {idx} has non-finite vector component at {component_idx}"
                ));
            }
        }
        if let Some(text_digest) = &item.text_digest {
            ensure_non_empty(
                text_digest,
                &format!("embeddings_file.items[{idx}].text_digest"),
            )?;
        }
    }

    Ok(())
}

pub fn embedding_file_digest_v1(file: &EmbeddingsFileV1) -> Result<String> {
    validate_embeddings_file_v1(file)?;

    let mut material = String::new();
    push_stable_field(&mut material, "version", &file.version);
    push_stable_field(
        &mut material,
        "created_at_unix_secs",
        &file.created_at_unix_secs.to_string(),
    );
    push_stable_field(&mut material, "backend", &file.backend);
    push_stable_field(&mut material, "model", &file.model);
    push_stable_field(&mut material, "dim", &file.dim.to_string());
    push_stable_field(
        &mut material,
        "target",
        embedding_target_kind_wire_v1(file.target),
    );

    for item in &file.items {
        material.push_str("item{");
        push_stable_field(
            &mut material,
            "key",
            &embedding_key_stable_material_v1(&item.key)?,
        );
        push_stable_field(
            &mut material,
            "text_digest",
            item.text_digest.as_deref().unwrap_or(""),
        );
        material.push_str("vector{");
        for component in &item.vector {
            push_stable_field(
                &mut material,
                "f32_bits",
                &format!("{:08x}", component.to_bits()),
            );
        }
        material.push_str("}}");
    }

    let mut metadata = file.metadata.iter().collect::<Vec<_>>();
    metadata.sort_by_key(|(ka, _)| *ka);
    for (key, value) in metadata {
        material.push_str("metadata{");
        push_stable_field(&mut material, "key", key);
        push_stable_field(&mut material, "value", value);
        material.push('}');
    }

    Ok(axiograph_kernel::object_blob_digest_v2(material.as_bytes()))
}

pub fn build_embedding_sidecar_manifest_v1(
    file: &EmbeddingsFileV1,
    input: EmbeddingSidecarManifestBuildInputV1,
) -> Result<EmbeddingSidecarManifestV1> {
    validate_embeddings_file_v1(file)?;

    let embeddings_file_digest = input
        .embeddings_file_digest
        .or_else(|| embedding_file_digest_v1(file).ok())
        .ok_or_else(|| anyhow!("failed to compute embeddings_file_digest"))?;

    let source_model = EmbeddingModelRefV1 {
        backend: file.backend.clone(),
        model: file.model.clone(),
        model_version: input.model_version.or_else(|| {
            metadata_first_value(
                &file.metadata,
                &[
                    "model_version",
                    "embedding_model_version",
                    "source_model_version",
                ],
            )
        }),
        model_digest: input.model_digest.or_else(|| {
            metadata_first_value(
                &file.metadata,
                &[
                    "model_digest",
                    "embedding_model_digest",
                    "source_model_digest",
                ],
            )
        }),
        deployment_id: input.deployment_id.or_else(|| {
            metadata_first_value(
                &file.metadata,
                &[
                    "deployment_id",
                    "embedding_deployment_id",
                    "source_model_deployment_id",
                ],
            )
        }),
    };

    let mut targets = Vec::with_capacity(file.items.len());
    let mut seen_keys = BTreeSet::new();
    for (idx, item) in file.items.iter().enumerate() {
        let source_text_digest = item
            .text_digest
            .as_deref()
            .ok_or_else(|| anyhow!("embeddings file item at index {idx} is missing text_digest"))?;
        ensure_non_empty(
            source_text_digest,
            &format!("embeddings_file.items[{idx}].text_digest"),
        )?;

        let stable_key = embedding_key_stable_material_v1(&item.key)?;
        if !seen_keys.insert(stable_key.clone()) {
            return Err(anyhow!(
                "embeddings file item key is duplicated at index {idx}"
            ));
        }

        targets.push(EmbeddingTargetIdentityV1 {
            target_id: embedding_target_id_v1(&stable_key, source_text_digest),
            key: item.key.clone(),
            typed_ref: None,
            source_text_digest: source_text_digest.to_string(),
        });
    }

    let mut metadata = file.metadata.clone();
    metadata.extend(input.metadata);
    metadata.insert(
        "builder".to_string(),
        "build_embedding_sidecar_manifest_v1".to_string(),
    );

    let sidecar_id = input.sidecar_id.unwrap_or_else(|| {
        embedding_sidecar_id_v1(
            &embeddings_file_digest,
            &input.accepted,
            input.materialization_id.as_ref(),
        )
    });

    let manifest = EmbeddingSidecarManifestV1 {
        version: EMBEDDING_SIDECAR_MANIFEST_VERSION_V1.to_string(),
        sidecar_id,
        created_at_unix_secs: input
            .created_at_unix_secs
            .unwrap_or(file.created_at_unix_secs),
        accepted: input.accepted,
        materialization_id: input.materialization_id,
        embeddings_file_digest: Some(embeddings_file_digest),
        source_model,
        target_kind: file.target,
        dim: file.dim,
        normalization: input.normalization,
        targets,
        trust: input
            .trust
            .unwrap_or_else(default_embedding_sidecar_trust_v1),
        metadata,
    };
    manifest.validate()?;
    Ok(manifest)
}

pub fn discover_embedding_evidence_overlay_v1(
    file: &EmbeddingsFileV1,
    manifest: &EmbeddingSidecarManifestV1,
    config: EmbeddingRelationshipDiscoveryConfigV1,
) -> Result<EmbeddingEvidenceOverlayV1> {
    validate_embeddings_file_v1(file)?;
    manifest.validate()?;

    if manifest.target_kind != file.target {
        return Err(anyhow!(
            "embedding evidence discovery target kind mismatch: file={:?} manifest={:?}",
            file.target,
            manifest.target_kind
        ));
    }
    if manifest.dim != file.dim {
        return Err(anyhow!(
            "embedding evidence discovery dim mismatch: file={} manifest={}",
            file.dim,
            manifest.dim
        ));
    }
    if manifest.source_model.backend != file.backend || manifest.source_model.model != file.model {
        return Err(anyhow!(
            "embedding evidence discovery source model mismatch: file={}/{} manifest={}/{}",
            file.backend,
            file.model,
            manifest.source_model.backend,
            manifest.source_model.model
        ));
    }
    if manifest.targets.len() != file.items.len() {
        return Err(anyhow!(
            "embedding evidence discovery requires manifest targets to match embeddings file items: file={} manifest={}",
            file.items.len(),
            manifest.targets.len()
        ));
    }
    if !(config.min_cosine_similarity.is_finite()
        && (-1.0..=1.0).contains(&config.min_cosine_similarity))
    {
        return Err(anyhow!(
            "min_cosine_similarity must be finite and in [-1, 1]"
        ));
    }
    if config.max_relationships == 0 {
        return Err(anyhow!("max_relationships must be > 0"));
    }
    ensure_non_empty(&config.observation_method, "discovery.observation_method")?;
    ensure_non_empty(&config.relationship_method, "discovery.relationship_method")?;

    let mut items_by_key = BTreeMap::new();
    for item in &file.items {
        let stable_key = embedding_key_stable_material_v1(&item.key)?;
        if items_by_key.insert(stable_key, item).is_some() {
            return Err(anyhow!("embeddings file item key is duplicated"));
        }
    }

    let mut rows = Vec::with_capacity(manifest.targets.len());
    for target in &manifest.targets {
        let stable_key = embedding_key_stable_material_v1(&target.key)?;
        let item = items_by_key.get(&stable_key).ok_or_else(|| {
            anyhow!(
                "manifest target {} does not have a matching embeddings file item",
                target.target_id
            )
        })?;
        if item.text_digest.as_deref() != Some(target.source_text_digest.as_str()) {
            return Err(anyhow!(
                "manifest target {} source_text_digest does not match embeddings file item",
                target.target_id
            ));
        }
        rows.push((target, item.vector.as_slice()));
    }
    rows.sort_by(|(left, _), (right, _)| left.target_id.cmp(&right.target_id));

    if rows.len() < 2 {
        return Err(anyhow!(
            "embedding evidence discovery requires at least two targets"
        ));
    }

    let mut candidates = Vec::new();
    let mut candidate_pairs_considered = 0usize;
    for i in 0..rows.len() {
        for j in (i + 1)..rows.len() {
            candidate_pairs_considered += 1;
            let score = cosine_similarity_v1(rows[i].1, rows[j].1)?;
            if score >= config.min_cosine_similarity {
                candidates.push(EmbeddingSimilarityCandidateV1 {
                    left: rows[i].0,
                    right: rows[j].0,
                    score,
                });
            }
        }
    }

    candidates.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.left.target_id.cmp(&b.left.target_id))
            .then_with(|| a.right.target_id.cmp(&b.right.target_id))
    });
    candidates.truncate(config.max_relationships);

    if candidates.is_empty() {
        return Err(anyhow!(
            "embedding evidence discovery found no pairs above min_cosine_similarity={}",
            config.min_cosine_similarity
        ));
    }

    let mut observations = Vec::with_capacity(candidates.len());
    let mut relationships = Vec::with_capacity(candidates.len());
    for (rank_idx, candidate) in candidates.into_iter().enumerate() {
        let rank = (rank_idx + 1) as u32;
        let observation_id = embedding_observation_id_v1(
            &manifest.sidecar_id,
            &candidate.left.target_id,
            &candidate.right.target_id,
            candidate.score,
            rank,
        );
        let evidence_id = embedding_relationship_evidence_id_v1(
            &manifest.sidecar_id,
            &candidate.left.target_id,
            &candidate.right.target_id,
            candidate.score,
            rank,
        );
        observations.push(EmbeddingSimilarityObservationV1 {
            observation_id: observation_id.clone(),
            left: (*candidate.left).clone(),
            right: (*candidate.right).clone(),
            metric: EmbeddingSimilarityMetricV1::Cosine,
            score: candidate.score,
            rank: Some(rank),
            method: config.observation_method.clone(),
            caveats: vec!["cosine similarity is not semantic equivalence".to_string()],
        });
        relationships.push(EmbeddingRelationshipEvidenceV1 {
            suggested_refinement_handles: vec![embedding_relationship_refinement_handle_v1(
                &manifest.sidecar_id,
                &evidence_id,
                config.relationship,
            )],
            evidence_id,
            relationship: config.relationship,
            subject: (*candidate.left).clone(),
            object: (*candidate.right).clone(),
            score: Some(candidate.score),
            confidence: Some(((candidate.score + 1.0) / 2.0).clamp(0.0, 1.0)),
            source_observation_ids: vec![observation_id],
            method: config.relationship_method.clone(),
            advisory_only: true,
            typed_relation_ref: None,
            proposal_ref: None,
            caveats: vec![
                "embedding-derived relationship is advisory evidence only".to_string(),
                "promotion requires typed validation, review, CQ gates, reconciliation, and semantic VCS acceptance"
                    .to_string(),
            ],
        });
    }

    let source_embeddings_file_digest = embedding_file_digest_v1(file)?;
    let overlay_id = config.overlay_id.unwrap_or_else(|| {
        embedding_overlay_id_v1(
            &manifest.sidecar_id,
            &source_embeddings_file_digest,
            config.min_cosine_similarity,
            config.max_relationships,
        )
    });

    let mut metadata = config.metadata;
    metadata.insert(
        "source_embeddings_file_digest".to_string(),
        source_embeddings_file_digest,
    );
    metadata.insert(
        "candidate_pairs_considered".to_string(),
        candidate_pairs_considered.to_string(),
    );
    metadata.insert(
        "relationships_emitted".to_string(),
        relationships.len().to_string(),
    );
    metadata.insert(
        "min_cosine_similarity".to_string(),
        config.min_cosine_similarity.to_string(),
    );
    metadata.insert(
        "max_relationships".to_string(),
        config.max_relationships.to_string(),
    );

    let overlay = EmbeddingEvidenceOverlayV1 {
        version: EMBEDDING_EVIDENCE_OVERLAY_VERSION_V1.to_string(),
        overlay_id,
        created_at_unix_secs: config
            .created_at_unix_secs
            .unwrap_or(manifest.created_at_unix_secs),
        manifest_sidecar_id: manifest.sidecar_id.clone(),
        accepted: manifest.accepted.clone(),
        source_model: manifest.source_model.clone(),
        trust: manifest.trust.clone(),
        observations,
        relationships,
        proposal_refs: Vec::new(),
        metadata,
    };
    overlay.validate()?;
    Ok(overlay)
}

pub fn validate_embedding_sidecar_manifest_v1(manifest: &EmbeddingSidecarManifestV1) -> Result<()> {
    manifest.validate()
}

pub fn validate_embedding_evidence_overlay_v1(overlay: &EmbeddingEvidenceOverlayV1) -> Result<()> {
    overlay.validate()
}

impl EmbeddingSidecarManifestV1 {
    pub fn validate(&self) -> Result<()> {
        ensure_eq(
            &self.version,
            EMBEDDING_SIDECAR_MANIFEST_VERSION_V1,
            "embedding sidecar manifest version",
        )?;
        ensure_non_empty(&self.sidecar_id, "sidecar_id")?;
        self.accepted.validate()?;
        self.source_model.validate()?;
        if let Some(digest) = &self.embeddings_file_digest {
            ensure_non_empty(digest, "embeddings_file_digest")?;
        }
        if self.dim == 0 {
            return Err(anyhow!("embedding sidecar manifest dim must be > 0"));
        }
        if self.targets.is_empty() {
            return Err(anyhow!(
                "embedding sidecar manifest must include at least one target identity"
            ));
        }
        let mut target_ids = BTreeSet::new();
        for target in &self.targets {
            target.validate_for_kind(self.target_kind)?;
            if !target_ids.insert(target.target_id.as_str()) {
                return Err(anyhow!(
                    "embedding sidecar manifest target_id is duplicated: {}",
                    target.target_id
                ));
            }
        }
        self.trust.validate()
    }
}

impl EmbeddingEvidenceOverlayV1 {
    pub fn validate(&self) -> Result<()> {
        ensure_eq(
            &self.version,
            EMBEDDING_EVIDENCE_OVERLAY_VERSION_V1,
            "embedding evidence overlay version",
        )?;
        ensure_non_empty(&self.overlay_id, "overlay_id")?;
        ensure_non_empty(&self.manifest_sidecar_id, "manifest_sidecar_id")?;
        self.accepted.validate()?;
        self.source_model.validate()?;
        self.trust.validate()?;
        if self.observations.is_empty() && self.relationships.is_empty() {
            return Err(anyhow!(
                "embedding evidence overlay must include observations or relationships"
            ));
        }

        let mut observation_ids = BTreeSet::new();
        for observation in &self.observations {
            observation.validate()?;
            if !observation_ids.insert(observation.observation_id.as_str()) {
                return Err(anyhow!(
                    "embedding evidence overlay observation_id is duplicated: {}",
                    observation.observation_id
                ));
            }
        }

        let mut relationship_ids = BTreeSet::new();
        for relationship in &self.relationships {
            relationship.validate(&observation_ids)?;
            if !relationship_ids.insert(relationship.evidence_id.as_str()) {
                return Err(anyhow!(
                    "embedding evidence overlay evidence_id is duplicated: {}",
                    relationship.evidence_id
                ));
            }
        }
        Ok(())
    }
}

impl EmbeddingAcceptedRefV1 {
    fn validate(&self) -> Result<()> {
        ensure_non_empty(&self.accepted_ref, "accepted.accepted_ref")?;
        ensure_non_empty(
            self.accepted_axi_anchor.accepted_snapshot_id.as_str(),
            "accepted.accepted_axi_anchor.accepted_snapshot_id",
        )?;
        ensure_non_empty(
            self.accepted_axi_anchor.axi_digest.as_str(),
            "accepted.accepted_axi_anchor.axi_digest",
        )?;
        if let Some(name) = &self.module_name {
            ensure_non_empty(name, "accepted.module_name")?;
        }
        if let Some(digest) = &self.compiled_ir_digest {
            ensure_non_empty(digest, "accepted.compiled_ir_digest")?;
        }
        Ok(())
    }
}

impl EmbeddingModelRefV1 {
    fn validate(&self) -> Result<()> {
        ensure_non_empty(&self.backend, "source_model.backend")?;
        ensure_non_empty(&self.model, "source_model.model")?;
        for (field, value) in [
            ("source_model.model_version", &self.model_version),
            ("source_model.model_digest", &self.model_digest),
            ("source_model.deployment_id", &self.deployment_id),
        ] {
            if let Some(value) = value {
                ensure_non_empty(value, field)?;
            }
        }
        if self.model_version.is_none()
            && self.model_digest.is_none()
            && self.deployment_id.is_none()
        {
            return Err(anyhow!(
                "source_model must include model_version, model_digest, or deployment_id"
            ));
        }
        Ok(())
    }
}

impl EmbeddingSidecarTrustV1 {
    fn validate(&self) -> Result<()> {
        if self.caveats.is_empty() {
            return Err(anyhow!(
                "embedding sidecar trust must include at least one caveat"
            ));
        }
        for caveat in &self.caveats {
            ensure_non_empty(caveat, "trust.caveats[]")?;
        }
        Ok(())
    }
}

impl EmbeddingTargetIdentityV1 {
    fn validate_for_kind(&self, expected: EmbeddingTargetKindV1) -> Result<()> {
        ensure_non_empty(&self.target_id, "target.target_id")?;
        ensure_non_empty(&self.source_text_digest, "target.source_text_digest")?;
        match &self.key {
            EmbeddingKeyV1::DocChunk { chunk_id } => {
                ensure_non_empty(chunk_id, "target.key.chunk_id")?;
                if expected != EmbeddingTargetKindV1::DocChunks {
                    return Err(anyhow!(
                        "target {} uses doc_chunk key but manifest target_kind is {:?}",
                        self.target_id,
                        expected
                    ));
                }
            }
            EmbeddingKeyV1::Entity { entity_type, name } => {
                ensure_non_empty(entity_type, "target.key.entity_type")?;
                ensure_non_empty(name, "target.key.name")?;
                if expected != EmbeddingTargetKindV1::Entities {
                    return Err(anyhow!(
                        "target {} uses entity key but manifest target_kind is {:?}",
                        self.target_id,
                        expected
                    ));
                }
            }
        }
        if let Some(typed_ref) = &self.typed_ref {
            ensure_non_empty(typed_ref, "target.typed_ref")?;
        }
        Ok(())
    }

    fn validate_endpoint(&self, field: &str) -> Result<()> {
        ensure_non_empty(&self.target_id, &format!("{field}.target_id"))?;
        ensure_non_empty(
            &self.source_text_digest,
            &format!("{field}.source_text_digest"),
        )?;
        match &self.key {
            EmbeddingKeyV1::DocChunk { chunk_id } => {
                ensure_non_empty(chunk_id, &format!("{field}.key.chunk_id"))?;
            }
            EmbeddingKeyV1::Entity { entity_type, name } => {
                ensure_non_empty(entity_type, &format!("{field}.key.entity_type"))?;
                ensure_non_empty(name, &format!("{field}.key.name"))?;
            }
        }
        if let Some(typed_ref) = &self.typed_ref {
            ensure_non_empty(typed_ref, &format!("{field}.typed_ref"))?;
        }
        Ok(())
    }
}

impl EmbeddingSimilarityObservationV1 {
    fn validate(&self) -> Result<()> {
        ensure_non_empty(&self.observation_id, "observation.observation_id")?;
        self.left.validate_endpoint("observation.left")?;
        self.right.validate_endpoint("observation.right")?;
        validate_finite_score(self.score, "observation.score")?;
        if self.metric == EmbeddingSimilarityMetricV1::Cosine && !(-1.0..=1.0).contains(&self.score)
        {
            return Err(anyhow!(
                "observation.score must be in [-1, 1] for cosine similarity"
            ));
        }
        ensure_non_empty(&self.method, "observation.method")?;
        for caveat in &self.caveats {
            ensure_non_empty(caveat, "observation.caveats[]")?;
        }
        Ok(())
    }
}

impl EmbeddingRelationshipEvidenceV1 {
    fn validate(&self, observation_ids: &BTreeSet<&str>) -> Result<()> {
        ensure_non_empty(&self.evidence_id, "relationship.evidence_id")?;
        self.subject.validate_endpoint("relationship.subject")?;
        self.object.validate_endpoint("relationship.object")?;
        if let Some(score) = self.score {
            validate_finite_score(score, "relationship.score")?;
        }
        if let Some(confidence) = self.confidence {
            validate_finite_score(confidence, "relationship.confidence")?;
            if !(0.0..=1.0).contains(&confidence) {
                return Err(anyhow!("relationship.confidence must be in [0, 1]"));
            }
        }
        for observation_id in &self.source_observation_ids {
            ensure_non_empty(observation_id, "relationship.source_observation_ids[]")?;
            if !observation_ids.contains(observation_id.as_str()) {
                return Err(anyhow!(
                    "relationship {} references unknown observation_id {}",
                    self.evidence_id,
                    observation_id
                ));
            }
        }
        ensure_non_empty(&self.method, "relationship.method")?;
        if !self.advisory_only {
            return Err(anyhow!(
                "embedding relationship evidence must be advisory_only=true"
            ));
        }
        if let Some(typed_relation_ref) = &self.typed_relation_ref {
            ensure_non_empty(typed_relation_ref, "relationship.typed_relation_ref")?;
        }
        if let Some(proposal_ref) = &self.proposal_ref {
            ensure_non_empty(proposal_ref, "relationship.proposal_ref")?;
        }
        if self.suggested_refinement_handles.is_empty() {
            return Err(anyhow!(
                "embedding relationship evidence must include at least one suggested refinement handle"
            ));
        }
        for handle in &self.suggested_refinement_handles {
            handle.validate()?;
        }
        for caveat in &self.caveats {
            ensure_non_empty(caveat, "relationship.caveats[]")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct EmbeddingSimilarityCandidateV1<'a> {
    left: &'a EmbeddingTargetIdentityV1,
    right: &'a EmbeddingTargetIdentityV1,
    score: f32,
}

fn embedding_relationship_refinement_handle_v1(
    sidecar_id: &str,
    evidence_id: &str,
    relationship: EmbeddingRelationshipKindV1,
) -> crate::typed_refinement::RuntimeRefinementHandleV2 {
    crate::typed_refinement::RuntimeRefinementHandleV2::new_reconciliation(
        crate::typed_refinement::ReconciliationRefinementOpV1::ResolveConflictByDecision {
            reconciliation_id: format!("embedding_evidence:{sidecar_id}"),
            artifact_kind: "embedding_relationship_evidence".to_string(),
            artifact_id: evidence_id.to_string(),
            resolution: format!("review_{relationship:?}_as_typed_ontology_delta")
                .to_ascii_lowercase(),
            theory_obligation_ref: None,
            theory_subject_ref: None,
            theory_subject_refs: Vec::new(),
        },
    )
}

fn validate_embedding_key_for_target_kind_v1(
    key: &EmbeddingKeyV1,
    expected: EmbeddingTargetKindV1,
    field: &str,
) -> Result<()> {
    match key {
        EmbeddingKeyV1::DocChunk { chunk_id } => {
            ensure_non_empty(chunk_id, &format!("{field}.chunk_id"))?;
            if expected != EmbeddingTargetKindV1::DocChunks {
                return Err(anyhow!(
                    "{field} uses doc_chunk key but embeddings target is {expected:?}"
                ));
            }
        }
        EmbeddingKeyV1::Entity { entity_type, name } => {
            ensure_non_empty(entity_type, &format!("{field}.entity_type"))?;
            ensure_non_empty(name, &format!("{field}.name"))?;
            if expected != EmbeddingTargetKindV1::Entities {
                return Err(anyhow!(
                    "{field} uses entity key but embeddings target is {expected:?}"
                ));
            }
        }
    }
    Ok(())
}

fn embedding_target_kind_wire_v1(target: EmbeddingTargetKindV1) -> &'static str {
    match target {
        EmbeddingTargetKindV1::DocChunks => "doc_chunks",
        EmbeddingTargetKindV1::Entities => "entities",
    }
}

fn embedding_key_stable_material_v1(key: &EmbeddingKeyV1) -> Result<String> {
    let mut out = String::new();
    match key {
        EmbeddingKeyV1::DocChunk { chunk_id } => {
            ensure_non_empty(chunk_id, "embedding_key.chunk_id")?;
            push_stable_field(&mut out, "kind", "doc_chunk");
            push_stable_field(&mut out, "chunk_id", chunk_id);
        }
        EmbeddingKeyV1::Entity { entity_type, name } => {
            ensure_non_empty(entity_type, "embedding_key.entity_type")?;
            ensure_non_empty(name, "embedding_key.name")?;
            push_stable_field(&mut out, "kind", "entity");
            push_stable_field(&mut out, "entity_type", entity_type);
            push_stable_field(&mut out, "name", name);
        }
    }
    Ok(out)
}

fn embedding_target_id_v1(stable_key: &str, source_text_digest: &str) -> String {
    let mut material = String::new();
    push_stable_field(&mut material, "key", stable_key);
    push_stable_field(&mut material, "source_text_digest", source_text_digest);
    format!(
        "embedding_target_v1:{}",
        axiograph_kernel::object_blob_digest_v2(material.as_bytes())
    )
}

fn embedding_sidecar_id_v1(
    embeddings_file_digest: &str,
    accepted: &EmbeddingAcceptedRefV1,
    materialization_id: Option<&MaterializationIdV2>,
) -> String {
    let mut material = String::new();
    push_stable_field(
        &mut material,
        "embeddings_file_digest",
        embeddings_file_digest,
    );
    push_stable_field(&mut material, "accepted_ref", &accepted.accepted_ref);
    push_stable_field(
        &mut material,
        "accepted_snapshot_id",
        accepted.accepted_axi_anchor.accepted_snapshot_id.as_str(),
    );
    push_stable_field(
        &mut material,
        "axi_digest",
        accepted.accepted_axi_anchor.axi_digest.as_str(),
    );
    push_stable_field(
        &mut material,
        "materialization_id",
        materialization_id
            .map(MaterializationIdV2::as_str)
            .unwrap_or(""),
    );
    format!(
        "embedding_sidecar_v1:{}",
        axiograph_kernel::object_blob_digest_v2(material.as_bytes())
    )
}

fn embedding_overlay_id_v1(
    manifest_sidecar_id: &str,
    embeddings_file_digest: &str,
    min_cosine_similarity: f32,
    max_relationships: usize,
) -> String {
    let mut material = String::new();
    push_stable_field(&mut material, "manifest_sidecar_id", manifest_sidecar_id);
    push_stable_field(
        &mut material,
        "embeddings_file_digest",
        embeddings_file_digest,
    );
    push_stable_field(
        &mut material,
        "min_cosine_similarity_bits",
        &format!("{:08x}", min_cosine_similarity.to_bits()),
    );
    push_stable_field(
        &mut material,
        "max_relationships",
        &max_relationships.to_string(),
    );
    format!(
        "embedding_overlay_v1:{}",
        axiograph_kernel::object_blob_digest_v2(material.as_bytes())
    )
}

fn embedding_observation_id_v1(
    manifest_sidecar_id: &str,
    left_target_id: &str,
    right_target_id: &str,
    score: f32,
    rank: u32,
) -> String {
    let mut material = String::new();
    push_stable_field(&mut material, "manifest_sidecar_id", manifest_sidecar_id);
    push_stable_field(&mut material, "left_target_id", left_target_id);
    push_stable_field(&mut material, "right_target_id", right_target_id);
    push_stable_field(
        &mut material,
        "score_bits",
        &format!("{:08x}", score.to_bits()),
    );
    push_stable_field(&mut material, "rank", &rank.to_string());
    format!(
        "embedding_observation_v1:{}",
        axiograph_kernel::object_blob_digest_v2(material.as_bytes())
    )
}

fn embedding_relationship_evidence_id_v1(
    manifest_sidecar_id: &str,
    left_target_id: &str,
    right_target_id: &str,
    score: f32,
    rank: u32,
) -> String {
    let mut material = String::new();
    push_stable_field(&mut material, "manifest_sidecar_id", manifest_sidecar_id);
    push_stable_field(&mut material, "left_target_id", left_target_id);
    push_stable_field(&mut material, "right_target_id", right_target_id);
    push_stable_field(
        &mut material,
        "score_bits",
        &format!("{:08x}", score.to_bits()),
    );
    push_stable_field(&mut material, "rank", &rank.to_string());
    format!(
        "embedding_relationship_v1:{}",
        axiograph_kernel::object_blob_digest_v2(material.as_bytes())
    )
}

fn metadata_first_value(metadata: &HashMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        metadata
            .get(*key)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn push_stable_field(out: &mut String, field: &str, value: &str) {
    out.push_str(field);
    out.push('=');
    out.push_str(&value.len().to_string());
    out.push(':');
    out.push_str(value);
    out.push('|');
}

fn cosine_similarity_v1(left: &[f32], right: &[f32]) -> Result<f32> {
    if left.len() != right.len() {
        return Err(anyhow!(
            "cannot compute cosine similarity for vectors with different dimensions: {} vs {}",
            left.len(),
            right.len()
        ));
    }

    let mut dot = 0.0f32;
    let mut left_norm2 = 0.0f32;
    let mut right_norm2 = 0.0f32;
    for (idx, (left_component, right_component)) in left.iter().zip(right.iter()).enumerate() {
        if !left_component.is_finite() || !right_component.is_finite() {
            return Err(anyhow!(
                "cannot compute cosine similarity with non-finite component at {idx}"
            ));
        }
        dot += left_component * right_component;
        left_norm2 += left_component * left_component;
        right_norm2 += right_component * right_component;
    }
    if left_norm2 <= 0.0 || right_norm2 <= 0.0 {
        return Err(anyhow!(
            "cannot compute cosine similarity for zero-norm embedding vector"
        ));
    }

    Ok((dot / (left_norm2.sqrt() * right_norm2.sqrt())).clamp(-1.0, 1.0))
}

fn ensure_eq(actual: &str, expected: &str, field: &str) -> Result<()> {
    if actual != expected {
        return Err(anyhow!(
            "unsupported {field}: {actual} (expected {expected})"
        ));
    }
    Ok(())
}

fn ensure_non_empty(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(anyhow!("{field} must not be empty"));
    }
    Ok(())
}

fn validate_finite_score(score: f32, field: &str) -> Result<()> {
    if !score.is_finite() {
        return Err(anyhow!("{field} must be finite"));
    }
    Ok(())
}

pub fn encode_embeddings_file_v1(file: &EmbeddingsFileV1) -> Result<Vec<u8>> {
    validate_embeddings_file_v1(file)?;
    let mut out = Vec::new();
    ciborium::ser::into_writer(file, &mut out)
        .map_err(|e| anyhow!("failed to CBOR-encode embeddings file: {e}"))?;
    if out.len() > MAX_EMBEDDINGS_FILE_BYTES {
        return Err(anyhow!(
            "embeddings file exceeds {MAX_EMBEDDINGS_FILE_BYTES} bytes"
        ));
    }
    Ok(out)
}

pub fn decode_embeddings_file_v1(bytes: &[u8]) -> Result<EmbeddingsFileV1> {
    if bytes.len() > MAX_EMBEDDINGS_FILE_BYTES {
        return Err(anyhow!(
            "embeddings file exceeds {MAX_EMBEDDINGS_FILE_BYTES} bytes"
        ));
    }
    let mut cursor = std::io::Cursor::new(bytes);
    let file: EmbeddingsFileV1 = ciborium::de::from_reader_with_recursion_limit(&mut cursor, 64)
        .map_err(|e| anyhow!("failed to CBOR-decode embeddings file: {e}"))?;
    if cursor.position() != bytes.len() as u64 {
        return Err(anyhow!("embeddings file contains trailing CBOR data"));
    }
    validate_embeddings_file_v1(&file)?;
    Ok(file)
}

// ============================================================================
// Runtime: resolved embedding rows keyed by snapshot-local entity ids
// ============================================================================

#[derive(Debug, Clone)]
pub struct ResolvedEmbeddingRowV1 {
    pub id: u32,
    pub vector: Vec<f32>,
    #[allow(dead_code)]
    pub text_digest: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedEmbeddingsTargetV1 {
    pub backend: String,
    pub model: String,
    pub dim: usize,
    pub rows: Vec<ResolvedEmbeddingRowV1>,
}

#[derive(Debug, Clone, Default)]
pub struct ResolvedEmbeddingsIndexV1 {
    db_token: Option<DbToken>,
    pub docchunks: Option<ResolvedEmbeddingsTargetV1>,
    pub entities: Option<ResolvedEmbeddingsTargetV1>,
}

fn normalize_in_place(v: &mut [f32]) {
    let mut norm2 = 0.0f32;
    for x in v.iter() {
        norm2 += x * x;
    }
    if norm2 <= 0.0 {
        return;
    }
    let inv = 1.0f32 / norm2.sqrt();
    for x in v.iter_mut() {
        *x *= inv;
    }
}

impl ResolvedEmbeddingsIndexV1 {
    pub fn assert_in_db(&self, db: &axiograph_pathdb::PathDB) -> Result<()> {
        let Some(expected) = self.db_token else {
            return Ok(());
        };
        let actual = db.db_token();
        if expected != actual {
            return Err(anyhow!(
                "embeddings index is for db#{} but was used with db#{} (stale snapshot?)",
                expected.raw(),
                actual.raw()
            ));
        }
        Ok(())
    }

    /// Resolve one embeddings file against the currently loaded snapshot DB.
    ///
    /// This converts stable keys (chunk_id, (type,name)) into snapshot-local entity ids,
    /// dropping any entries that can't be resolved.
    pub fn resolve_and_set(
        &mut self,
        db: &axiograph_pathdb::PathDB,
        file: EmbeddingsFileV1,
    ) -> Result<()> {
        let token = db.db_token();
        if let Some(existing) = self.db_token {
            if existing != token {
                return Err(anyhow!(
                    "cannot resolve embeddings into existing index: db token mismatch (expected db#{}, got db#{})",
                    existing.raw(),
                    token.raw()
                ));
            }
        } else {
            self.db_token = Some(token);
        }

        let dim = file.dim;
        if dim == 0 {
            return Err(anyhow!(
                "embeddings file has dim=0 (backend={}, model={})",
                file.backend,
                file.model
            ));
        }

        // Precompute lookup maps once per file.
        let mut chunk_id_to_entity_id: HashMap<String, u32> = HashMap::new();
        if file.target == EmbeddingTargetKindV1::DocChunks {
            if let Some(chunks) = db.find_by_type("DocChunk") {
                for id in chunks.iter() {
                    if let Some(view) = db.get_entity(id) {
                        if let Some(cid) = view.attrs.get("chunk_id") {
                            chunk_id_to_entity_id.insert(cid.to_string(), id);
                        }
                    }
                }
            }
        }

        let mut entity_key_to_id: HashMap<(String, String), u32> = HashMap::new();
        if file.target == EmbeddingTargetKindV1::Entities {
            for id in 0..(db.entities.len() as u32) {
                let Some(view) = db.get_entity(id) else {
                    continue;
                };
                let Some(name) = view.attrs.get("name") else {
                    continue;
                };
                entity_key_to_id.insert((view.entity_type.to_string(), name.to_string()), id);
            }
        }

        let mut rows: Vec<ResolvedEmbeddingRowV1> = Vec::new();
        for item in file.items {
            if item.vector.len() != dim {
                return Err(anyhow!(
                    "embeddings file item has wrong dim: expected {} got {}",
                    dim,
                    item.vector.len()
                ));
            }

            let id = match item.key {
                EmbeddingKeyV1::DocChunk { chunk_id } => {
                    if file.target != EmbeddingTargetKindV1::DocChunks {
                        continue;
                    }
                    let Some(&id) = chunk_id_to_entity_id.get(&chunk_id) else {
                        continue;
                    };
                    id
                }
                EmbeddingKeyV1::Entity { entity_type, name } => {
                    if file.target != EmbeddingTargetKindV1::Entities {
                        continue;
                    }
                    let Some(&id) = entity_key_to_id.get(&(entity_type, name)) else {
                        continue;
                    };
                    id
                }
            };

            let mut v = item.vector;
            normalize_in_place(&mut v);
            rows.push(ResolvedEmbeddingRowV1 {
                id,
                vector: v,
                text_digest: item.text_digest,
            });
        }

        let target = ResolvedEmbeddingsTargetV1 {
            backend: file.backend,
            model: file.model,
            dim,
            rows,
        };

        match file.target {
            EmbeddingTargetKindV1::DocChunks => self.docchunks = Some(target),
            EmbeddingTargetKindV1::Entities => self.entities = Some(target),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiograph_pathdb::{AcceptedSnapshotId, AxiDigest};

    fn assert_vec_approx_eq(a: &[f32], b: &[f32], eps: f32) {
        assert_eq!(a.len(), b.len(), "vector length mismatch");
        for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            let d = (x - y).abs();
            assert!(
                d <= eps,
                "vector mismatch at idx {i}: left={x} right={y} (|d|={d} eps={eps})"
            );
        }
    }

    fn sample_accepted_ref() -> EmbeddingAcceptedRefV1 {
        EmbeddingAcceptedRefV1 {
            accepted_ref: "heads/main".to_string(),
            accepted_axi_anchor: AcceptedAxiAnchor::new(
                AcceptedSnapshotId::new(
                    axiograph_kernel::SnapshotIdV2::from_canonical_fields(&[
                        b"embedding-test-snapshot",
                    ])
                    .to_string(),
                ),
                AxiDigest::from_axi_text("module Demo\n"),
            ),
            module_name: Some("Demo".to_string()),
            compiled_ir_digest: Some("sha256:compiled-ir".to_string()),
        }
    }

    fn sample_model_ref() -> EmbeddingModelRefV1 {
        EmbeddingModelRefV1 {
            backend: "ollama".to_string(),
            model: "nomic-embed-text".to_string(),
            model_version: Some("v1".to_string()),
            model_digest: Some("sha256:model".to_string()),
            deployment_id: None,
        }
    }

    fn sample_trust() -> EmbeddingSidecarTrustV1 {
        EmbeddingSidecarTrustV1 {
            authority: EmbeddingSidecarAuthorityV1::EvidenceIndexOnly,
            promotion_policy:
                EmbeddingPromotionPolicyV1::RequiresTypedValidationReviewCqReconciliationPromotion,
            caveats: vec![
                "embedding scores are advisory evidence, not accepted .axi truth".to_string(),
                "ontology mutation requires typed validation, review, CQ gates, reconciliation, and promotion".to_string(),
            ],
        }
    }

    fn sample_doc_target(target_id: &str, chunk_id: &str) -> EmbeddingTargetIdentityV1 {
        EmbeddingTargetIdentityV1 {
            target_id: target_id.to_string(),
            key: EmbeddingKeyV1::DocChunk {
                chunk_id: chunk_id.to_string(),
            },
            typed_ref: Some(format!("docchunk:{chunk_id}")),
            source_text_digest: format!("sha256:text-{chunk_id}"),
        }
    }

    fn sample_manifest() -> EmbeddingSidecarManifestV1 {
        EmbeddingSidecarManifestV1 {
            version: EMBEDDING_SIDECAR_MANIFEST_VERSION_V1.to_string(),
            sidecar_id: "embeddings:demo:1".to_string(),
            created_at_unix_secs: 10,
            accepted: sample_accepted_ref(),
            materialization_id: Some(MaterializationIdV2::from_canonical_fields(&[
                b"embedding-test-materialization",
            ])),
            embeddings_file_digest: Some("sha256:embeddings-file".to_string()),
            source_model: sample_model_ref(),
            target_kind: EmbeddingTargetKindV1::DocChunks,
            dim: 3,
            normalization: EmbeddingNormalizationPolicyV1::UnitL2,
            targets: vec![
                sample_doc_target("target:doc:0", "doc_0"),
                sample_doc_target("target:doc:1", "doc_1"),
            ],
            trust: sample_trust(),
            metadata: HashMap::from([("chunker".to_string(), "test".to_string())]),
        }
    }

    fn sample_overlay() -> EmbeddingEvidenceOverlayV1 {
        let left = sample_doc_target("target:doc:0", "doc_0");
        let right = sample_doc_target("target:doc:1", "doc_1");
        EmbeddingEvidenceOverlayV1 {
            version: EMBEDDING_EVIDENCE_OVERLAY_VERSION_V1.to_string(),
            overlay_id: "embedding-overlay:demo:1".to_string(),
            created_at_unix_secs: 11,
            manifest_sidecar_id: "embeddings:demo:1".to_string(),
            accepted: sample_accepted_ref(),
            source_model: sample_model_ref(),
            trust: sample_trust(),
            observations: vec![EmbeddingSimilarityObservationV1 {
                observation_id: "obs:similar:0".to_string(),
                left: left.clone(),
                right: right.clone(),
                metric: EmbeddingSimilarityMetricV1::Cosine,
                score: 0.82,
                rank: Some(1),
                method: "ann_cosine_top_k".to_string(),
                caveats: vec!["similarity is not semantic equivalence".to_string()],
            }],
            relationships: vec![EmbeddingRelationshipEvidenceV1 {
                evidence_id: "rel:supports:0".to_string(),
                relationship: EmbeddingRelationshipKindV1::Supports,
                subject: left,
                object: right,
                score: Some(0.82),
                confidence: Some(0.64),
                source_observation_ids: vec!["obs:similar:0".to_string()],
                method: "embedding_relationship_classifier_v1".to_string(),
                advisory_only: true,
                typed_relation_ref: Some("relation:EvidenceSupports".to_string()),
                proposal_ref: Some("proposal:rel:0".to_string()),
                suggested_refinement_handles: vec![embedding_relationship_refinement_handle_v1(
                    "embeddings:demo:1",
                    "rel:supports:0",
                    EmbeddingRelationshipKindV1::Supports,
                )],
                caveats: vec![
                    "candidate relationship must be promoted through typed review".to_string(),
                ],
            }],
            proposal_refs: vec!["proposal:rel:0".to_string()],
            metadata: HashMap::new(),
        }
    }

    fn sample_embeddings_file_for_sidecar() -> EmbeddingsFileV1 {
        EmbeddingsFileV1 {
            version: EMBEDDINGS_FILE_VERSION_V1.to_string(),
            created_at_unix_secs: 123,
            backend: "ollama".to_string(),
            model: "nomic-embed-text".to_string(),
            dim: 2,
            target: EmbeddingTargetKindV1::DocChunks,
            items: vec![
                EmbeddingItemV1 {
                    key: EmbeddingKeyV1::DocChunk {
                        chunk_id: "doc_a".to_string(),
                    },
                    vector: vec![1.0, 0.0],
                    text_digest: Some(
                        axiograph_kernel::object_blob_digest_v2(b"text-a").to_string(),
                    ),
                },
                EmbeddingItemV1 {
                    key: EmbeddingKeyV1::DocChunk {
                        chunk_id: "doc_b".to_string(),
                    },
                    vector: vec![0.96, 0.28],
                    text_digest: Some(
                        axiograph_kernel::object_blob_digest_v2(b"text-b").to_string(),
                    ),
                },
                EmbeddingItemV1 {
                    key: EmbeddingKeyV1::DocChunk {
                        chunk_id: "doc_c".to_string(),
                    },
                    vector: vec![0.0, 1.0],
                    text_digest: Some(
                        axiograph_kernel::object_blob_digest_v2(b"text-c").to_string(),
                    ),
                },
            ],
            metadata: HashMap::from([
                ("model_version".to_string(), "test-model-v1".to_string()),
                ("chunker".to_string(), "unit-test".to_string()),
            ]),
        }
    }

    fn sample_manifest_build_input() -> EmbeddingSidecarManifestBuildInputV1 {
        let mut input = EmbeddingSidecarManifestBuildInputV1::new(sample_accepted_ref());
        input.materialization_id = Some(MaterializationIdV2::from_canonical_fields(&[
            b"embedding-input-test-materialization",
        ]));
        input.normalization = EmbeddingNormalizationPolicyV1::UnitL2;
        input
    }

    #[test]
    fn embeddings_file_v1_roundtrip_cbor() {
        let file = EmbeddingsFileV1 {
            version: EMBEDDINGS_FILE_VERSION_V1.to_string(),
            created_at_unix_secs: 1,
            backend: "ollama".to_string(),
            model: "nomic-embed-text".to_string(),
            dim: 2,
            target: EmbeddingTargetKindV1::DocChunks,
            items: vec![EmbeddingItemV1 {
                key: EmbeddingKeyV1::DocChunk {
                    chunk_id: "doc_0".to_string(),
                },
                vector: vec![3.0, 4.0],
                text_digest: Some("sha256:deadbeef".to_string()),
            }],
            metadata: HashMap::from([("note".to_string(), "test".to_string())]),
        };

        let bytes = encode_embeddings_file_v1(&file).expect("encode");
        let decoded = decode_embeddings_file_v1(&bytes).expect("decode");

        assert_eq!(decoded.version, EMBEDDINGS_FILE_VERSION_V1);
        assert_eq!(decoded.backend, "ollama");
        assert_eq!(decoded.model, "nomic-embed-text");
        assert_eq!(decoded.dim, 2);
        assert_eq!(decoded.target, EmbeddingTargetKindV1::DocChunks);
        assert_eq!(decoded.items.len(), 1);
        assert_eq!(
            decoded.metadata.get("note").map(|s| s.as_str()),
            Some("test")
        );
    }

    #[test]
    fn embedding_sidecar_manifest_builder_is_deterministic_from_embeddings_file() {
        let file = sample_embeddings_file_for_sidecar();
        let manifest_a = build_embedding_sidecar_manifest_v1(&file, sample_manifest_build_input())
            .expect("build manifest");
        let manifest_b = build_embedding_sidecar_manifest_v1(&file, sample_manifest_build_input())
            .expect("build manifest again");

        validate_embedding_sidecar_manifest_v1(&manifest_a).expect("manifest valid");
        assert_eq!(manifest_a.sidecar_id, manifest_b.sidecar_id);
        assert_eq!(manifest_a.targets, manifest_b.targets);
        assert_eq!(manifest_a.created_at_unix_secs, 123);
        assert_eq!(manifest_a.source_model.backend, "ollama");
        assert_eq!(
            manifest_a.source_model.model_version.as_deref(),
            Some("test-model-v1")
        );
        assert_eq!(manifest_a.target_kind, EmbeddingTargetKindV1::DocChunks);
        assert_eq!(manifest_a.dim, 2);
        assert_eq!(manifest_a.targets.len(), 3);
        assert!(manifest_a
            .embeddings_file_digest
            .as_deref()
            .is_some_and(|digest| digest.starts_with("axi:object-blob:v2:sha256:")));
        assert!(manifest_a.targets.iter().all(|target| target
            .target_id
            .starts_with("embedding_target_v1:axi:object-blob:v2:sha256:")));
        assert_eq!(
            manifest_a.metadata.get("builder").map(String::as_str),
            Some("build_embedding_sidecar_manifest_v1")
        );
    }

    #[test]
    fn embedding_sidecar_manifest_builder_requires_text_digests() {
        let mut file = sample_embeddings_file_for_sidecar();
        file.items[1].text_digest = None;

        let err =
            build_embedding_sidecar_manifest_v1(&file, sample_manifest_build_input()).unwrap_err();
        assert!(
            err.to_string().contains("missing text_digest"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn embedding_relationship_discovery_emits_ranked_advisory_overlay() {
        let file = sample_embeddings_file_for_sidecar();
        let manifest = build_embedding_sidecar_manifest_v1(&file, sample_manifest_build_input())
            .expect("build manifest");
        let config = EmbeddingRelationshipDiscoveryConfigV1 {
            min_cosine_similarity: 0.95,
            max_relationships: 4,
            ..Default::default()
        };

        let overlay =
            discover_embedding_evidence_overlay_v1(&file, &manifest, config).expect("discover");
        validate_embedding_evidence_overlay_v1(&overlay).expect("overlay valid");

        assert_eq!(overlay.manifest_sidecar_id, manifest.sidecar_id);
        assert_eq!(overlay.observations.len(), 1);
        assert_eq!(overlay.relationships.len(), 1);
        let observation = &overlay.observations[0];
        assert_eq!(observation.rank, Some(1));
        assert_eq!(observation.metric, EmbeddingSimilarityMetricV1::Cosine);
        assert!(observation.score > 0.95);

        let relationship = &overlay.relationships[0];
        assert_eq!(
            relationship.relationship,
            EmbeddingRelationshipKindV1::SimilarTo
        );
        assert!(relationship.advisory_only);
        assert_eq!(
            relationship.source_observation_ids,
            vec![observation.observation_id.clone()]
        );
        assert!(relationship
            .evidence_id
            .starts_with("embedding_relationship_v1:axi:object-blob:v2:sha256:"));
        assert_eq!(relationship.suggested_refinement_handles.len(), 1);
        assert!(relationship.suggested_refinement_handles[0]
            .validate()
            .is_ok());
        assert!(relationship
            .caveats
            .iter()
            .any(|caveat| caveat.contains("advisory evidence only")));
        assert_eq!(
            overlay
                .metadata
                .get("candidate_pairs_considered")
                .map(String::as_str),
            Some("3")
        );
        assert_eq!(
            overlay
                .metadata
                .get("relationships_emitted")
                .map(String::as_str),
            Some("1")
        );
    }

    #[test]
    fn embedding_relationship_discovery_is_deterministic() {
        let file = sample_embeddings_file_for_sidecar();
        let manifest = build_embedding_sidecar_manifest_v1(&file, sample_manifest_build_input())
            .expect("build manifest");

        let overlay_a = discover_embedding_evidence_overlay_v1(
            &file,
            &manifest,
            EmbeddingRelationshipDiscoveryConfigV1::default(),
        )
        .expect("discover");
        let overlay_b = discover_embedding_evidence_overlay_v1(
            &file,
            &manifest,
            EmbeddingRelationshipDiscoveryConfigV1::default(),
        )
        .expect("discover again");

        assert_eq!(overlay_a.overlay_id, overlay_b.overlay_id);
        assert_eq!(
            overlay_a.observations[0].observation_id,
            overlay_b.observations[0].observation_id
        );
        assert_eq!(
            overlay_a.relationships[0].evidence_id,
            overlay_b.relationships[0].evidence_id
        );
    }

    #[test]
    fn resolves_docchunk_embeddings_by_chunk_id() {
        let mut db = axiograph_pathdb::PathDB::new();
        let doc_chunk_id = db.add_entity(
            "DocChunk",
            vec![
                ("name", "doc_0"),
                ("chunk_id", "doc_0"),
                ("text", "hello world"),
            ],
        );
        db.build_indexes();

        let file = EmbeddingsFileV1 {
            version: EMBEDDINGS_FILE_VERSION_V1.to_string(),
            created_at_unix_secs: 1,
            backend: "ollama".to_string(),
            model: "nomic-embed-text".to_string(),
            dim: 2,
            target: EmbeddingTargetKindV1::DocChunks,
            items: vec![EmbeddingItemV1 {
                key: EmbeddingKeyV1::DocChunk {
                    chunk_id: "doc_0".to_string(),
                },
                vector: vec![3.0, 4.0],
                text_digest: None,
            }],
            metadata: HashMap::new(),
        };

        let mut idx = ResolvedEmbeddingsIndexV1::default();
        idx.resolve_and_set(&db, file).expect("resolve");
        idx.assert_in_db(&db).expect("token ok");

        let target = idx.docchunks.expect("docchunks target set");
        assert_eq!(target.backend, "ollama");
        assert_eq!(target.model, "nomic-embed-text");
        assert_eq!(target.dim, 2);
        assert_eq!(target.rows.len(), 1);
        assert_eq!(target.rows[0].id, doc_chunk_id);
        assert_vec_approx_eq(&target.rows[0].vector, &[0.6, 0.8], 1e-4);
    }

    #[test]
    fn resolves_entity_embeddings_by_type_and_name() {
        let mut db = axiograph_pathdb::PathDB::new();
        let alice_id = db.add_entity(
            "Person",
            vec![("name", "Alice"), ("description", "likes cats")],
        );
        db.build_indexes();

        let file = EmbeddingsFileV1 {
            version: EMBEDDINGS_FILE_VERSION_V1.to_string(),
            created_at_unix_secs: 1,
            backend: "ollama".to_string(),
            model: "nomic-embed-text".to_string(),
            dim: 2,
            target: EmbeddingTargetKindV1::Entities,
            items: vec![EmbeddingItemV1 {
                key: EmbeddingKeyV1::Entity {
                    entity_type: "Person".to_string(),
                    name: "Alice".to_string(),
                },
                vector: vec![0.0, 5.0],
                text_digest: None,
            }],
            metadata: HashMap::new(),
        };

        let mut idx = ResolvedEmbeddingsIndexV1::default();
        idx.resolve_and_set(&db, file).expect("resolve");
        idx.assert_in_db(&db).expect("token ok");

        let target = idx.entities.expect("entities target set");
        assert_eq!(target.rows.len(), 1);
        assert_eq!(target.rows[0].id, alice_id);
        assert_vec_approx_eq(&target.rows[0].vector, &[0.0, 1.0], 1e-4);
    }

    #[test]
    fn embedding_sidecar_manifest_v1_validates_anchors_targets_and_trust() {
        let manifest = sample_manifest();
        validate_embedding_sidecar_manifest_v1(&manifest).expect("valid manifest");

        let value = serde_json::to_value(&manifest).expect("serialize manifest");
        assert_eq!(
            value["accepted"]["accepted_ref"].as_str(),
            Some("heads/main")
        );
        let expected_axi_digest = AxiDigest::from_axi_text("module Demo\n");
        assert_eq!(
            value["accepted"]["accepted_axi_anchor"]["axi_digest"].as_str(),
            Some(expected_axi_digest.as_str())
        );
        assert_eq!(
            value["source_model"]["model_digest"].as_str(),
            Some("sha256:model")
        );
        assert_eq!(value["target_kind"].as_str(), Some("doc_chunks"));
        assert!(value["trust"]["caveats"]
            .as_array()
            .is_some_and(|caveats| !caveats.is_empty()));

        let mut bad_target_kind = manifest.clone();
        bad_target_kind.target_kind = EmbeddingTargetKindV1::Entities;
        let err = validate_embedding_sidecar_manifest_v1(&bad_target_kind).unwrap_err();
        assert!(
            err.to_string().contains("target_kind"),
            "unexpected error: {err}"
        );

        let mut missing_model_identity = manifest;
        missing_model_identity.source_model.model_version = None;
        missing_model_identity.source_model.model_digest = None;
        let err = validate_embedding_sidecar_manifest_v1(&missing_model_identity).unwrap_err();
        assert!(
            err.to_string().contains("source_model must include"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn embedding_evidence_overlay_v1_serializes_advisory_relationships() {
        let overlay = sample_overlay();
        validate_embedding_evidence_overlay_v1(&overlay).expect("valid overlay");

        let json = serde_json::to_string_pretty(&overlay).expect("serialize overlay");
        let decoded: EmbeddingEvidenceOverlayV1 =
            serde_json::from_str(&json).expect("deserialize overlay");
        validate_embedding_evidence_overlay_v1(&decoded).expect("decoded overlay valid");

        let value = serde_json::to_value(&decoded).expect("overlay json value");
        assert_eq!(
            value["relationships"][0]["relationship"].as_str(),
            Some("supports")
        );
        assert_eq!(
            value["relationships"][0]["advisory_only"].as_bool(),
            Some(true)
        );
        assert_eq!(
            value["relationships"][0]["typed_relation_ref"].as_str(),
            Some("relation:EvidenceSupports")
        );
        assert!(value["relationships"][0]["suggested_refinement_handles"]
            .as_array()
            .is_some_and(|handles| handles.len() == 1));
        assert_eq!(value["observations"][0]["metric"].as_str(), Some("cosine"));
        assert!(value["trust"]["caveats"]
            .as_array()
            .is_some_and(|caveats| caveats.iter().any(|caveat| caveat
                .as_str()
                .is_some_and(|text| text.contains("not accepted .axi truth")))));
    }

    #[test]
    fn embedding_evidence_overlay_rejects_non_advisory_relationships() {
        let mut overlay = sample_overlay();
        overlay.relationships[0].advisory_only = false;

        let err = validate_embedding_evidence_overlay_v1(&overlay).unwrap_err();
        assert!(
            err.to_string().contains("advisory_only=true"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn embeddings_index_is_snapshot_scoped() {
        let mut db1 = axiograph_pathdb::PathDB::new();
        db1.add_entity("DocChunk", vec![("name", "doc_0"), ("chunk_id", "doc_0")]);
        db1.build_indexes();

        let file = EmbeddingsFileV1 {
            version: EMBEDDINGS_FILE_VERSION_V1.to_string(),
            created_at_unix_secs: 1,
            backend: "ollama".to_string(),
            model: "nomic-embed-text".to_string(),
            dim: 2,
            target: EmbeddingTargetKindV1::DocChunks,
            items: vec![EmbeddingItemV1 {
                key: EmbeddingKeyV1::DocChunk {
                    chunk_id: "doc_0".to_string(),
                },
                vector: vec![1.0, 0.0],
                text_digest: None,
            }],
            metadata: HashMap::new(),
        };

        let mut idx = ResolvedEmbeddingsIndexV1::default();
        idx.resolve_and_set(&db1, file).expect("resolve");
        idx.assert_in_db(&db1).expect("token ok");

        let db2 = axiograph_pathdb::PathDB::new();
        let err = idx.assert_in_db(&db2).unwrap_err();
        assert!(
            err.to_string().contains("stale snapshot"),
            "unexpected error: {err}"
        );
    }
}
