//! Deterministic authenticated SQLite materializations for derived PathDB state.
//!
//! `.axpd` files are immutable, disposable execution artifacts. They bind an
//! accepted snapshot/tree, compiled kernel IR, canonical fact log, and ordered
//! overlays. They are never an authority for reconstructing accepted `.axi`
//! bytes, and successful runtime validation is not a Lean proof.

use crate::{blob_id, AcceptedBuildManifest, ImmutableObjectKind};
use axiograph_kernel::{
    FactIdV2, InstanceIdV2, KernelSnapshotIr, MaterializationIdV2, ObjectBlobIdV2, RepositoryIdV2,
    RevisionDigestV2, RoleIdV2, RoleKindIr, SchemaObjectRefIr, SnapshotIdV2, TreeIdV2,
    TypedValueIr,
};
use rusqlite::limits::Limit;
use rusqlite::{params, Connection, OptionalExtension, Transaction, MAIN_DB};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

pub const AXPD_FORMAT: &str = "axiograph_pathdb_sqlite";
pub const AXPD_SCHEMA_VERSION: u32 = 2;
pub const AXPD_APPLICATION_ID: i32 = 0x4158_5044; // "AXPD"
pub const AXPD_PAGE_SIZE: u32 = 4096;
pub const AXPD_MATERIALIZER_VERSION: &str = "axiograph-pathdb-materializer/2";
pub const AXPD_MATERIALIZATIONS_DIR: &str = "materializations";
const AXPD_RECEIPT_MAX_BYTES: u64 = 1024 * 1024;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Error)]
pub enum AxpdError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("invalid materialization: {0}")]
    Invalid(String),
    #[error("materialization anchor mismatch for {field}: expected {expected}, found {actual}")]
    AnchorMismatch {
        field: &'static str,
        expected: String,
        actual: String,
    },
    #[error("materialization digest mismatch for {field}: expected {expected}, found {actual}")]
    DigestMismatch {
        field: &'static str,
        expected: String,
        actual: String,
    },
    #[error("materialization limit `{limit_name}` exceeded: limit {limit}, actual {actual}")]
    LimitExceeded {
        limit_name: &'static str,
        limit: u64,
        actual: u64,
    },
    #[error("immutable materialization collision at {path}")]
    ImmutableCollision { path: PathBuf },
    #[error("injected materialization failure at {0:?}")]
    Injected(AxpdFailurePoint),
}

pub type AxpdResult<T> = Result<T, AxpdError>;

fn invalid(message: impl Into<String>) -> AxpdError {
    AxpdError::Invalid(message.into())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxpdLimits {
    pub max_file_bytes: u64,
    pub max_page_count: u64,
    pub max_string_bytes: usize,
    pub max_module_count: usize,
    pub max_kernel_ref_count: usize,
    pub max_entity_count: usize,
    pub max_relation_fact_count: usize,
    pub max_projection_count: usize,
    pub max_context_count: usize,
    pub max_equivalence_count: usize,
    pub max_overlay_count: usize,
    pub max_projection_fanout: usize,
}

impl Default for AxpdLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 256 * 1024 * 1024,
            max_page_count: 65_536,
            max_string_bytes: 1024 * 1024,
            max_module_count: 16_384,
            max_kernel_ref_count: 2_000_000,
            max_entity_count: 2_000_000,
            max_relation_fact_count: 2_000_000,
            max_projection_count: 8_000_000,
            max_context_count: 2_000_000,
            max_equivalence_count: 2_000_000,
            max_overlay_count: 4096,
            max_projection_fanout: 4096,
        }
    }
}

impl AxpdLimits {
    fn validate(&self) -> AxpdResult<()> {
        let positive = [
            ("max_file_bytes", self.max_file_bytes),
            ("max_page_count", self.max_page_count),
            ("max_string_bytes", self.max_string_bytes as u64),
            ("max_module_count", self.max_module_count as u64),
            ("max_kernel_ref_count", self.max_kernel_ref_count as u64),
            ("max_entity_count", self.max_entity_count as u64),
            (
                "max_relation_fact_count",
                self.max_relation_fact_count as u64,
            ),
            ("max_projection_count", self.max_projection_count as u64),
            ("max_context_count", self.max_context_count as u64),
            ("max_equivalence_count", self.max_equivalence_count as u64),
            ("max_overlay_count", self.max_overlay_count as u64),
            ("max_projection_fanout", self.max_projection_fanout as u64),
        ];
        if let Some((name, _)) = positive.into_iter().find(|(_, value)| *value == 0) {
            return Err(invalid(format!("{name} must be positive")));
        }
        let hard = Self::default();
        let bounded = [
            ("max_file_bytes", self.max_file_bytes, hard.max_file_bytes),
            ("max_page_count", self.max_page_count, hard.max_page_count),
            (
                "max_string_bytes",
                self.max_string_bytes as u64,
                hard.max_string_bytes as u64,
            ),
            (
                "max_module_count",
                self.max_module_count as u64,
                hard.max_module_count as u64,
            ),
            (
                "max_kernel_ref_count",
                self.max_kernel_ref_count as u64,
                hard.max_kernel_ref_count as u64,
            ),
            (
                "max_entity_count",
                self.max_entity_count as u64,
                hard.max_entity_count as u64,
            ),
            (
                "max_relation_fact_count",
                self.max_relation_fact_count as u64,
                hard.max_relation_fact_count as u64,
            ),
            (
                "max_projection_count",
                self.max_projection_count as u64,
                hard.max_projection_count as u64,
            ),
            (
                "max_context_count",
                self.max_context_count as u64,
                hard.max_context_count as u64,
            ),
            (
                "max_equivalence_count",
                self.max_equivalence_count as u64,
                hard.max_equivalence_count as u64,
            ),
            (
                "max_overlay_count",
                self.max_overlay_count as u64,
                hard.max_overlay_count as u64,
            ),
            (
                "max_projection_fanout",
                self.max_projection_fanout as u64,
                hard.max_projection_fanout as u64,
            ),
        ];
        if let Some((name, actual, maximum)) = bounded
            .into_iter()
            .find(|(_, actual, maximum)| actual > maximum)
        {
            return Err(limit_error(name, maximum, actual));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxpdConfiguration {
    pub page_size: u32,
    pub path_index_depth: u32,
    pub include_optional_caches: bool,
}

impl Default for AxpdConfiguration {
    fn default() -> Self {
        Self {
            page_size: AXPD_PAGE_SIZE,
            path_index_depth: 4,
            include_optional_caches: false,
        }
    }
}

impl AxpdConfiguration {
    pub fn validate(&self) -> AxpdResult<()> {
        if self.page_size != AXPD_PAGE_SIZE {
            return Err(invalid(format!(
                "unsupported page size {}; expected {AXPD_PAGE_SIZE}",
                self.page_size
            )));
        }
        if !(1..=64).contains(&self.path_index_depth) {
            return Err(invalid("path_index_depth must be in 1..=64"));
        }
        if self.include_optional_caches {
            return Err(invalid(
                "optional caches are external and may not enter authenticated `.axpd` semantics",
            ));
        }
        Ok(())
    }

    pub fn digest(&self) -> AxpdResult<ObjectBlobIdV2> {
        self.validate()?;
        Ok(ObjectBlobIdV2::from_canonical_fields(&[
            b"axiograph_axpd_configuration",
            &AXPD_SCHEMA_VERSION.to_be_bytes(),
            &self.page_size.to_be_bytes(),
            &self.path_index_depth.to_be_bytes(),
            &[u8::from(self.include_optional_caches)],
        ]))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxpdOverlayInput {
    pub kind: String,
    pub digest: ObjectBlobIdV2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxpdAnchors {
    pub repository_id: RepositoryIdV2,
    pub accepted_snapshot_id: SnapshotIdV2,
    pub accepted_tree_id: TreeIdV2,
    pub ordered_module_closure: Vec<RevisionDigestV2>,
    pub kernel_ir_digest: ObjectBlobIdV2,
    pub canonical_fact_log_digest: ObjectBlobIdV2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KernelRefRow {
    pub wire_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityRow {
    pub entity_key: ObjectBlobIdV2,
    pub instance_id: InstanceIdV2,
    pub object_ref_json: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationFactRow {
    pub fact_id: FactIdV2,
    pub instance_id: InstanceIdV2,
    pub relation_id: axiograph_kernel::RelationIdV2,
    pub local_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionRow {
    pub fact_id: FactIdV2,
    pub ordinal: u32,
    pub role_id: RoleIdV2,
    pub value_kind: String,
    pub value: String,
    pub target_entity_key: Option<ObjectBlobIdV2>,
    pub target_fact_id: Option<FactIdV2>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextRow {
    pub fact_id: FactIdV2,
    pub ordinal: u32,
    pub role_id: RoleIdV2,
    pub axis_kind: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquivalenceRow {
    pub instance_id: InstanceIdV2,
    pub generator_ref_json: String,
    pub source_entity_key: ObjectBlobIdV2,
    pub target_entity_key: ObjectBlobIdV2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverlayRow {
    pub ordinal: u32,
    pub kind: String,
    pub digest: ObjectBlobIdV2,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxpdLogicalImage {
    pub kernel_refs: Vec<KernelRefRow>,
    pub entities: Vec<EntityRow>,
    pub relation_facts: Vec<RelationFactRow>,
    pub projections: Vec<ProjectionRow>,
    pub contexts: Vec<ContextRow>,
    pub equivalences: Vec<EquivalenceRow>,
    pub overlays: Vec<OverlayRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxpdBuildSpec {
    pub anchors: AxpdAnchors,
    pub image: AxpdLogicalImage,
    pub configuration: AxpdConfiguration,
    pub materializer_version: String,
}

impl AxpdBuildSpec {
    pub fn from_accepted_kernel(
        manifest: &AcceptedBuildManifest,
        kernel: &KernelSnapshotIr,
        overlays: &[AxpdOverlayInput],
        configuration: AxpdConfiguration,
    ) -> AxpdResult<Self> {
        manifest
            .validate()
            .map_err(|error| invalid(format!("invalid accepted build manifest: {error}")))?;
        if &manifest.repository_id != kernel.repository_id() {
            return Err(anchor_mismatch(
                "repository_id",
                manifest.repository_id.as_str(),
                kernel.repository_id().as_str(),
            ));
        }
        if &manifest.accepted_snapshot_id != kernel.accepted_snapshot_id() {
            return Err(anchor_mismatch(
                "accepted_snapshot_id",
                manifest.accepted_snapshot_id.as_str(),
                kernel.accepted_snapshot_id().as_str(),
            ));
        }
        let kernel_ir_bytes = serde_json::to_vec(kernel)
            .map_err(|error| invalid(format!("failed to encode compiled kernel IR: {error}")))?;
        let kernel_object_digest = blob_id(ImmutableObjectKind::KernelIr, &kernel_ir_bytes);
        if manifest.kernel_ir_digest != kernel_object_digest {
            return Err(anchor_mismatch(
                "kernel_ir_digest",
                manifest.kernel_ir_digest.as_str(),
                kernel_object_digest.as_str(),
            ));
        }
        let closure = kernel
            .ordered_module_closure()
            .iter()
            .map(|module| module.revision.clone())
            .collect::<Vec<_>>();
        if manifest.ordered_module_closure != closure {
            return Err(invalid(
                "accepted build manifest module closure does not match kernel closure order",
            ));
        }
        configuration.validate()?;
        let anchors = AxpdAnchors {
            repository_id: manifest.repository_id.clone(),
            accepted_snapshot_id: manifest.accepted_snapshot_id.clone(),
            accepted_tree_id: manifest.accepted_tree_id.clone(),
            ordered_module_closure: closure,
            kernel_ir_digest: manifest.kernel_ir_digest.clone(),
            canonical_fact_log_digest: manifest.canonical_fact_log_digest.clone(),
        };
        let image = AxpdLogicalImage::from_kernel(kernel, overlays)?;
        Ok(Self {
            anchors,
            image,
            configuration,
            materializer_version: AXPD_MATERIALIZER_VERSION.to_string(),
        })
    }

    pub fn validate_and_canonicalize(&mut self, limits: &AxpdLimits) -> AxpdResult<()> {
        limits.validate()?;
        self.configuration.validate()?;
        validate_text(
            &self.materializer_version,
            "materializer_version",
            limits.max_string_bytes,
        )?;
        validate_count(
            "module_count",
            self.anchors.ordered_module_closure.len(),
            limits.max_module_count,
        )?;
        self.image.canonicalize_and_validate(limits)?;
        Ok(())
    }

    pub fn logical_digest(&self) -> AxpdResult<ObjectBlobIdV2> {
        self.configuration.validate()?;
        logical_digest(&self.anchors, &self.image)
    }
}

impl AxpdLogicalImage {
    pub fn from_kernel(
        kernel: &KernelSnapshotIr,
        overlays: &[AxpdOverlayInput],
    ) -> AxpdResult<Self> {
        let mut role_info = BTreeMap::<RoleIdV2, (RoleKindIr, SchemaObjectRefIr)>::new();
        let mut reversible_generators = BTreeSet::<String>::new();
        for schema in kernel.schemas() {
            for relation in &schema.relations {
                for role in &relation.roles {
                    let previous = role_info
                        .insert(role.role_id.clone(), (role.kind, role.type_expr.carrier()));
                    if previous.is_some() {
                        return Err(invalid(format!(
                            "duplicate role id in kernel: {}",
                            role.role_id
                        )));
                    }
                }
            }
            for generator in &schema.generators {
                if generator.reversible {
                    reversible_generators.insert(canonical_json(&generator.generator_ref)?);
                }
            }
        }

        let mut image = Self {
            kernel_refs: kernel
                .refs()
                .iter()
                .map(|reference| {
                    Ok(KernelRefRow {
                        wire_json: canonical_json(reference)?,
                    })
                })
                .collect::<AxpdResult<Vec<_>>>()?,
            overlays: overlays
                .iter()
                .enumerate()
                .map(|(ordinal, overlay)| {
                    Ok(OverlayRow {
                        ordinal: u32::try_from(ordinal)
                            .map_err(|_| invalid("overlay ordinal exceeds u32"))?,
                        kind: overlay.kind.clone(),
                        digest: overlay.digest.clone(),
                    })
                })
                .collect::<AxpdResult<Vec<_>>>()?,
            ..Self::default()
        };

        for instance in kernel.instances() {
            for carrier in &instance.carriers {
                let object_ref_json = canonical_json(&carrier.object)?;
                for value in &carrier.elements {
                    image.entities.push(EntityRow {
                        entity_key: entity_key(&instance.instance_id, &object_ref_json, value),
                        instance_id: instance.instance_id.clone(),
                        object_ref_json: object_ref_json.clone(),
                        value: value.clone(),
                    });
                }
            }

            for fact in &instance.facts {
                image.relation_facts.push(RelationFactRow {
                    fact_id: fact.fact_id.clone(),
                    instance_id: instance.instance_id.clone(),
                    relation_id: fact.relation_id.clone(),
                    local_label: fact.local_label.clone(),
                });
                for (ordinal, role_value) in fact.ordered_role_values.iter().enumerate() {
                    let ordinal = u32::try_from(ordinal)
                        .map_err(|_| invalid("projection ordinal exceeds u32"))?;
                    let (role_kind, carrier) =
                        role_info.get(&role_value.role_id).ok_or_else(|| {
                            invalid(format!(
                                "fact {} cites unknown role {}",
                                fact.fact_id, role_value.role_id
                            ))
                        })?;
                    let (value_kind, value, target_entity_key, target_fact_id) = match &role_value
                        .value
                    {
                        TypedValueIr::ObjectElement { value } => {
                            let object_ref_json = canonical_json(carrier)?;
                            (
                                "object_element".to_string(),
                                value.clone(),
                                Some(entity_key(&instance.instance_id, &object_ref_json, value)),
                                None,
                            )
                        }
                        TypedValueIr::RelationFact { fact_id } => (
                            "relation_fact".to_string(),
                            fact_id.to_string(),
                            None,
                            Some(fact_id.clone()),
                        ),
                    };
                    image.projections.push(ProjectionRow {
                        fact_id: fact.fact_id.clone(),
                        ordinal,
                        role_id: role_value.role_id.clone(),
                        value_kind,
                        value: value.clone(),
                        target_entity_key,
                        target_fact_id,
                    });
                    if matches!(
                        role_kind,
                        RoleKindIr::Context | RoleKindIr::World | RoleKindIr::Temporal
                    ) {
                        image.contexts.push(ContextRow {
                            fact_id: fact.fact_id.clone(),
                            ordinal,
                            role_id: role_value.role_id.clone(),
                            axis_kind: role_kind_wire(*role_kind).to_string(),
                            value,
                        });
                    }
                }
            }

            for function in &instance.functions {
                let generator_ref_json = canonical_json(&function.generator)?;
                if !reversible_generators.contains(&generator_ref_json) {
                    continue;
                }
                let source_ref = canonical_json(&function.source)?;
                let target_ref = canonical_json(&function.target)?;
                for mapping in &function.mappings {
                    image.equivalences.push(EquivalenceRow {
                        instance_id: instance.instance_id.clone(),
                        generator_ref_json: generator_ref_json.clone(),
                        source_entity_key: entity_key(
                            &instance.instance_id,
                            &source_ref,
                            &mapping.source,
                        ),
                        target_entity_key: entity_key(
                            &instance.instance_id,
                            &target_ref,
                            &mapping.target,
                        ),
                    });
                }
            }
        }
        Ok(image)
    }

    pub fn canonicalize_and_validate(&mut self, limits: &AxpdLimits) -> AxpdResult<()> {
        limits.validate()?;
        validate_count(
            "kernel_ref_count",
            self.kernel_refs.len(),
            limits.max_kernel_ref_count,
        )?;
        validate_count("entity_count", self.entities.len(), limits.max_entity_count)?;
        validate_count(
            "relation_fact_count",
            self.relation_facts.len(),
            limits.max_relation_fact_count,
        )?;
        validate_count(
            "projection_count",
            self.projections.len(),
            limits.max_projection_count,
        )?;
        validate_count(
            "context_count",
            self.contexts.len(),
            limits.max_context_count,
        )?;
        validate_count(
            "equivalence_count",
            self.equivalences.len(),
            limits.max_equivalence_count,
        )?;
        validate_count(
            "overlay_count",
            self.overlays.len(),
            limits.max_overlay_count,
        )?;

        for row in &self.kernel_refs {
            validate_text(
                &row.wire_json,
                "kernel_refs.wire_json",
                limits.max_string_bytes,
            )?;
        }
        for row in &self.entities {
            validate_text(
                &row.object_ref_json,
                "entities.object_ref_json",
                limits.max_string_bytes,
            )?;
            validate_text(&row.value, "entities.value", limits.max_string_bytes)?;
        }
        for row in &self.relation_facts {
            if let Some(label) = &row.local_label {
                validate_text(label, "relation_facts.local_label", limits.max_string_bytes)?;
            }
        }
        for row in &self.projections {
            validate_text(
                &row.value_kind,
                "projections.value_kind",
                limits.max_string_bytes,
            )?;
            validate_text(&row.value, "projections.value", limits.max_string_bytes)?;
            match row.value_kind.as_str() {
                "object_element"
                    if row.target_entity_key.is_some() && row.target_fact_id.is_none() => {}
                "relation_fact"
                    if row.target_entity_key.is_none() && row.target_fact_id.is_some() => {}
                _ => {
                    return Err(invalid(format!(
                        "projection {}:{} has inconsistent target kind",
                        row.fact_id, row.ordinal
                    )))
                }
            }
        }
        for row in &self.contexts {
            validate_text(
                &row.axis_kind,
                "contexts.axis_kind",
                limits.max_string_bytes,
            )?;
            validate_text(&row.value, "contexts.value", limits.max_string_bytes)?;
            if !matches!(row.axis_kind.as_str(), "context" | "world" | "temporal") {
                return Err(invalid(format!(
                    "unsupported context axis kind `{}`",
                    row.axis_kind
                )));
            }
        }
        for row in &self.equivalences {
            validate_text(
                &row.generator_ref_json,
                "equivalences.generator_ref_json",
                limits.max_string_bytes,
            )?;
        }
        for (expected, row) in self.overlays.iter().enumerate() {
            validate_text(&row.kind, "overlays.kind", limits.max_string_bytes)?;
            let expected =
                u32::try_from(expected).map_err(|_| invalid("overlay ordinal exceeds u32"))?;
            if row.ordinal != expected {
                return Err(invalid(format!(
                    "overlay order is not contiguous: expected {expected}, found {}",
                    row.ordinal
                )));
            }
        }

        self.kernel_refs
            .sort_by(|a, b| a.wire_json.cmp(&b.wire_json));
        self.entities
            .sort_by(|a, b| a.entity_key.cmp(&b.entity_key));
        self.relation_facts
            .sort_by(|a, b| a.fact_id.cmp(&b.fact_id));
        self.projections
            .sort_by(|a, b| (&a.fact_id, a.ordinal).cmp(&(&b.fact_id, b.ordinal)));
        self.contexts.sort_by(|a, b| {
            (&a.fact_id, a.ordinal, &a.role_id).cmp(&(&b.fact_id, b.ordinal, &b.role_id))
        });
        self.equivalences.sort_by(|a, b| {
            (
                &a.instance_id,
                &a.generator_ref_json,
                &a.source_entity_key,
                &a.target_entity_key,
            )
                .cmp(&(
                    &b.instance_id,
                    &b.generator_ref_json,
                    &b.source_entity_key,
                    &b.target_entity_key,
                ))
        });
        self.overlays.sort_by_key(|row| row.ordinal);

        reject_duplicate_by("kernel ref", &self.kernel_refs, |a, b| {
            a.wire_json == b.wire_json
        })?;
        reject_duplicate_by("entity", &self.entities, |a, b| {
            a.entity_key == b.entity_key
        })?;
        reject_duplicate_by("relation fact", &self.relation_facts, |a, b| {
            a.fact_id == b.fact_id
        })?;
        reject_duplicate_by("projection", &self.projections, |a, b| {
            a.fact_id == b.fact_id && a.ordinal == b.ordinal
        })?;
        reject_duplicate_by("context", &self.contexts, |a, b| {
            a.fact_id == b.fact_id && a.ordinal == b.ordinal && a.role_id == b.role_id
        })?;
        reject_duplicate_by("equivalence", &self.equivalences, |a, b| a == b)?;

        let entity_ids = self
            .entities
            .iter()
            .map(|row| row.entity_key.clone())
            .collect::<BTreeSet<_>>();
        let fact_ids = self
            .relation_facts
            .iter()
            .map(|row| row.fact_id.clone())
            .collect::<BTreeSet<_>>();
        let mut fanout = BTreeMap::<FactIdV2, usize>::new();
        for row in &self.projections {
            if !fact_ids.contains(&row.fact_id) {
                return Err(invalid(format!(
                    "projection cites missing fact {}",
                    row.fact_id
                )));
            }
            if let Some(key) = &row.target_entity_key {
                if !entity_ids.contains(key) {
                    return Err(invalid(format!("projection cites missing entity {key}")));
                }
            }
            if let Some(fact_id) = &row.target_fact_id {
                if !fact_ids.contains(fact_id) {
                    return Err(invalid(format!(
                        "projection cites missing relation fact {fact_id}"
                    )));
                }
            }
            let count = fanout.entry(row.fact_id.clone()).or_default();
            *count += 1;
            if *count > limits.max_projection_fanout {
                return Err(limit_error(
                    "projection_fanout",
                    limits.max_projection_fanout,
                    *count,
                ));
            }
        }
        let projection_keys = self
            .projections
            .iter()
            .map(|row| {
                (
                    row.fact_id.clone(),
                    row.ordinal,
                    row.role_id.clone(),
                    row.value.clone(),
                )
            })
            .collect::<BTreeSet<_>>();
        for row in &self.contexts {
            if !projection_keys.contains(&(
                row.fact_id.clone(),
                row.ordinal,
                row.role_id.clone(),
                row.value.clone(),
            )) {
                return Err(invalid(format!(
                    "context row {}:{} lacks matching projection",
                    row.fact_id, row.ordinal
                )));
            }
        }
        for row in &self.equivalences {
            if !entity_ids.contains(&row.source_entity_key)
                || !entity_ids.contains(&row.target_entity_key)
            {
                return Err(invalid("equivalence cites missing finite carrier element"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxpdReceipt {
    pub format: String,
    pub schema_version: u32,
    pub sqlite_version: String,
    pub materializer_version: String,
    pub configuration_digest: ObjectBlobIdV2,
    pub anchors: AxpdAnchors,
    pub logical_digest: ObjectBlobIdV2,
    pub exact_image_digest: ObjectBlobIdV2,
    pub materialization_id: MaterializationIdV2,
    pub byte_len: u64,
}

impl AxpdReceipt {
    pub fn expectation(&self) -> AxpdExpectation {
        AxpdExpectation {
            sqlite_version: self.sqlite_version.clone(),
            materializer_version: self.materializer_version.clone(),
            configuration_digest: self.configuration_digest.clone(),
            anchors: self.anchors.clone(),
            logical_digest: self.logical_digest.clone(),
            exact_image_digest: self.exact_image_digest.clone(),
            materialization_id: self.materialization_id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxpdExpectation {
    pub sqlite_version: String,
    pub materializer_version: String,
    pub configuration_digest: ObjectBlobIdV2,
    pub anchors: AxpdAnchors,
    pub logical_digest: ObjectBlobIdV2,
    pub exact_image_digest: ObjectBlobIdV2,
    pub materialization_id: MaterializationIdV2,
}

#[derive(Debug)]
pub struct VerifiedAxpd {
    receipt: AxpdReceipt,
    image: AxpdLogicalImage,
}

impl VerifiedAxpd {
    pub fn receipt(&self) -> &AxpdReceipt {
        &self.receipt
    }

    pub fn image(&self) -> &AxpdLogicalImage {
        &self.image
    }

    pub fn finite_facts_by_relation(
        &self,
        relation_id: &axiograph_kernel::RelationIdV2,
        limit: usize,
    ) -> (Vec<FactIdV2>, bool) {
        let all = self
            .image
            .relation_facts
            .iter()
            .filter(|row| &row.relation_id == relation_id)
            .map(|row| row.fact_id.clone())
            .collect::<Vec<_>>();
        let truncated = all.len() > limit;
        (all.into_iter().take(limit).collect(), truncated)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxpdFailurePoint {
    AfterPopulate,
    AfterBackup,
    BeforeValidation,
    AfterValidation,
    BeforePublish,
    AfterPublish,
    AfterImagePublish,
    AfterReceiptWrite,
    AfterReceiptFsync,
    AfterReceiptPublish,
}

pub trait AxpdFailureInjector: Send + Sync {
    fn check(&self, point: AxpdFailurePoint) -> AxpdResult<()>;
}

#[derive(Debug, Default)]
pub struct NoAxpdFailure;

impl AxpdFailureInjector for NoAxpdFailure {
    fn check(&self, _point: AxpdFailurePoint) -> AxpdResult<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct FailAxpdAt(pub AxpdFailurePoint);

impl AxpdFailureInjector for FailAxpdAt {
    fn check(&self, point: AxpdFailurePoint) -> AxpdResult<()> {
        if self.0 == point {
            Err(AxpdError::Injected(point))
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AxpdRecoveryAction {
    Reused,
    Rebuilt,
    QuarantinedAndRebuilt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxpdRecoveryReport {
    pub action: AxpdRecoveryAction,
    pub receipt: AxpdReceipt,
    pub quarantined_path: Option<PathBuf>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxpdCacheBinding {
    pub materialization_id: MaterializationIdV2,
    pub exact_image_digest: ObjectBlobIdV2,
    pub cache_digest: ObjectBlobIdV2,
}

impl AxpdCacheBinding {
    pub fn validate(&self, receipt: &AxpdReceipt) -> AxpdResult<()> {
        if self.materialization_id != receipt.materialization_id {
            return Err(anchor_mismatch(
                "cache.materialization_id",
                receipt.materialization_id.as_str(),
                self.materialization_id.as_str(),
            ));
        }
        if self.exact_image_digest != receipt.exact_image_digest {
            return Err(anchor_mismatch(
                "cache.exact_image_digest",
                receipt.exact_image_digest.as_str(),
                self.exact_image_digest.as_str(),
            ));
        }
        Ok(())
    }
}

pub(crate) struct AxpdMaterializer;

impl AxpdMaterializer {
    /// Publish into the authenticated AxiStore directory family.
    ///
    /// Both the SQLite image and its self-authenticating receipt are immutable
    /// and named by `MaterializationIdV2`. The receipt is not semantic authority:
    /// opening recomputes the exact image, logical digest, anchors, and id.
    pub(crate) fn publish_in_store(
        store_root: impl AsRef<Path>,
        spec: AxpdBuildSpec,
        limits: &AxpdLimits,
    ) -> AxpdResult<AxpdReceipt> {
        Self::publish_in_store_with_injector(store_root, spec, limits, &NoAxpdFailure)
    }

    pub(crate) fn publish_in_store_with_injector(
        store_root: impl AsRef<Path>,
        spec: AxpdBuildSpec,
        limits: &AxpdLimits,
        injector: &dyn AxpdFailureInjector,
    ) -> AxpdResult<AxpdReceipt> {
        let directory = store_root.as_ref().join(AXPD_MATERIALIZATIONS_DIR);
        fs::create_dir_all(&directory)?;
        validate_directory(&directory)?;
        let provisional = unique_temp_path(&directory, Some("axpd-build"));
        let guard = TemporaryFileGuard(provisional.clone());
        let receipt = Self::publish_image_with_injector(&provisional, spec, limits, injector)?;
        let image_path = materialization_image_path(&directory, &receipt.materialization_id);

        if image_path.exists() {
            Self::open(&image_path, &receipt.expectation(), limits)?;
            fs::remove_file(&provisional)?;
        } else {
            fs::rename(&provisional, &image_path)?;
            sync_directory(&directory)?;
        }
        std::mem::forget(guard);
        injector.check(AxpdFailurePoint::AfterImagePublish)?;
        publish_receipt(&directory, &receipt, injector)?;
        Ok(receipt)
    }

    pub(crate) fn image_path_in_store(
        store_root: impl AsRef<Path>,
        materialization_id: &MaterializationIdV2,
    ) -> PathBuf {
        materialization_image_path(
            &store_root.as_ref().join(AXPD_MATERIALIZATIONS_DIR),
            materialization_id,
        )
    }

    pub(crate) fn receipt_path_in_store(
        store_root: impl AsRef<Path>,
        materialization_id: &MaterializationIdV2,
    ) -> PathBuf {
        materialization_receipt_path(
            &store_root.as_ref().join(AXPD_MATERIALIZATIONS_DIR),
            materialization_id,
        )
    }

    pub(crate) fn recover_in_store(
        store_root: impl AsRef<Path>,
        spec: AxpdBuildSpec,
        limits: &AxpdLimits,
    ) -> AxpdResult<AxpdRecoveryReport> {
        let store_root = store_root.as_ref();
        let directory = store_root.join(AXPD_MATERIALIZATIONS_DIR);
        fs::create_dir_all(&directory)?;
        validate_directory(&directory)?;
        let provisional = unique_temp_path(&directory, Some("axpd-recovery"));
        let guard = TemporaryFileGuard(provisional.clone());
        let receipt =
            Self::publish_image_with_injector(&provisional, spec, limits, &NoAxpdFailure)?;
        let image_path = materialization_image_path(&directory, &receipt.materialization_id);
        let receipt_path = materialization_receipt_path(&directory, &receipt.materialization_id);
        let mut diagnostics = Vec::new();

        if image_path.exists() {
            match Self::open_from_store(store_root, &receipt.materialization_id, limits) {
                Ok(verified) if verified.receipt == receipt => {
                    fs::remove_file(&provisional)?;
                    std::mem::forget(guard);
                    return Ok(AxpdRecoveryReport {
                        action: AxpdRecoveryAction::Reused,
                        receipt,
                        quarantined_path: None,
                        diagnostics,
                    });
                }
                Ok(_) => {
                    diagnostics.push("existing receipt differs from deterministic rebuild".into())
                }
                Err(error) => diagnostics.push(error.to_string()),
            }

            let quarantined_image = quarantine_path(&image_path);
            fs::rename(&image_path, &quarantined_image)?;
            if receipt_path.exists() {
                let quarantined_receipt = quarantine_path(&receipt_path);
                fs::rename(&receipt_path, &quarantined_receipt)?;
                diagnostics.push(format!(
                    "quarantined receipt at {}",
                    quarantined_receipt.display()
                ));
            }
            fs::rename(&provisional, &image_path)?;
            sync_directory(&directory)?;
            std::mem::forget(guard);
            publish_receipt(&directory, &receipt, &NoAxpdFailure)?;
            return Ok(AxpdRecoveryReport {
                action: AxpdRecoveryAction::QuarantinedAndRebuilt,
                receipt,
                quarantined_path: Some(quarantined_image),
                diagnostics,
            });
        }

        fs::rename(&provisional, &image_path)?;
        sync_directory(&directory)?;
        std::mem::forget(guard);
        publish_receipt(&directory, &receipt, &NoAxpdFailure)?;
        Ok(AxpdRecoveryReport {
            action: AxpdRecoveryAction::Rebuilt,
            receipt,
            quarantined_path: None,
            diagnostics,
        })
    }

    /// Open a store-family materialization using its immutable receipt as the
    /// expected-value carrier. Every receipt field is revalidated against the
    /// SQLite image before any rows are returned.
    pub(crate) fn open_from_store(
        store_root: impl AsRef<Path>,
        materialization_id: &MaterializationIdV2,
        limits: &AxpdLimits,
    ) -> AxpdResult<VerifiedAxpd> {
        let directory = store_root.as_ref().join(AXPD_MATERIALIZATIONS_DIR);
        let receipt_path = materialization_receipt_path(&directory, materialization_id);
        let bytes = read_file_bounded(&receipt_path, AXPD_RECEIPT_MAX_BYTES)?;
        axiograph_security::validate_json_nesting(
            &bytes,
            axiograph_security::MAX_JSON_NESTING_DEPTH,
            "materialization receipt",
        )
        .map_err(|error| invalid(error.to_string()))?;
        let receipt: AxpdReceipt = serde_json::from_slice(&bytes)
            .map_err(|error| invalid(format!("invalid materialization receipt JSON: {error}")))?;
        if &receipt.materialization_id != materialization_id {
            return Err(digest_mismatch(
                "receipt.materialization_id",
                materialization_id.as_str(),
                receipt.materialization_id.as_str(),
            ));
        }
        let image_path = materialization_image_path(&directory, materialization_id);
        let verified = Self::open(&image_path, &receipt.expectation(), limits)?;
        if verified.receipt != receipt {
            return Err(invalid(
                "stored materialization receipt differs from recomputed verified receipt",
            ));
        }
        Ok(verified)
    }

    fn publish_image_with_injector(
        path: impl AsRef<Path>,
        mut spec: AxpdBuildSpec,
        limits: &AxpdLimits,
        injector: &dyn AxpdFailureInjector,
    ) -> AxpdResult<AxpdReceipt> {
        limits.validate()?;
        spec.validate_and_canonicalize(limits)?;
        let path = path.as_ref();
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let temporary = unique_temp_path(parent, path.file_name().and_then(|v| v.to_str()));
        let guard = TemporaryFileGuard(temporary.clone());
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;

        let logical_digest = spec.logical_digest()?;
        let configuration_digest = spec.configuration.digest()?;
        let sqlite_version = rusqlite::version().to_string();
        let connection = build_memory_database(
            &spec,
            &logical_digest,
            &configuration_digest,
            &sqlite_version,
            limits,
        )?;
        injector.check(AxpdFailurePoint::AfterPopulate)?;
        connection.backup(MAIN_DB, &temporary, None)?;
        drop(connection);
        sync_file(&temporary)?;
        injector.check(AxpdFailurePoint::AfterBackup)?;
        injector.check(AxpdFailurePoint::BeforeValidation)?;

        let exact_digest = exact_image_digest(&temporary, limits.max_file_bytes)?;
        let byte_len = fs::metadata(&temporary)?.len();
        let materialization_id = materialization_id(
            &spec.anchors,
            &logical_digest,
            &exact_digest,
            &configuration_digest,
            &sqlite_version,
            &spec.materializer_version,
        );
        let receipt = AxpdReceipt {
            format: AXPD_FORMAT.to_string(),
            schema_version: AXPD_SCHEMA_VERSION,
            sqlite_version,
            materializer_version: spec.materializer_version.clone(),
            configuration_digest,
            anchors: spec.anchors.clone(),
            logical_digest,
            exact_image_digest: exact_digest,
            materialization_id,
            byte_len,
        };
        Self::open(&temporary, &receipt.expectation(), limits)?;
        injector.check(AxpdFailurePoint::AfterValidation)?;
        injector.check(AxpdFailurePoint::BeforePublish)?;

        if path.exists() {
            let existing = exact_image_digest(path, limits.max_file_bytes)?;
            if existing != receipt.exact_image_digest {
                return Err(AxpdError::ImmutableCollision {
                    path: path.to_path_buf(),
                });
            }
            drop(guard);
            let _ = fs::remove_file(&temporary);
            return Ok(receipt);
        }
        fs::rename(&temporary, path)?;
        sync_directory(parent)?;
        std::mem::forget(guard);
        injector.check(AxpdFailurePoint::AfterPublish)?;
        Ok(receipt)
    }

    pub(crate) fn open(
        path: impl AsRef<Path>,
        expected: &AxpdExpectation,
        limits: &AxpdLimits,
    ) -> AxpdResult<VerifiedAxpd> {
        limits.validate()?;
        let path = path.as_ref();
        // Read, authenticate, and load one immutable byte image. Opening SQLite
        // by pathname after hashing would let an attacker swap the file between
        // those operations.
        let bytes = read_file_bounded(path, limits.max_file_bytes)?;
        validate_sqlite_prefix(&bytes)?;
        let exact = exact_image_digest_bytes(&bytes);
        if exact != expected.exact_image_digest {
            return Err(digest_mismatch(
                "exact_image_digest",
                expected.exact_image_digest.as_str(),
                exact.as_str(),
            ));
        }
        let byte_len = usize_to_u64(bytes.len())?;
        let mut connection = Connection::open_in_memory()?;
        configure_connection_limits(&connection, limits)?;
        connection.deserialize_read_exact(MAIN_DB, bytes.as_slice(), bytes.len(), true)?;
        validate_application_and_schema(&connection, limits)?;
        validate_schema_tables(&connection)?;
        validate_quick_check(&connection)?;
        let header = read_header(&connection)?;
        validate_header(&header, expected)?;
        validate_database_limits(&connection, limits)?;
        validate_module_closure(&connection, &expected.anchors.ordered_module_closure)?;
        let recomputed = logical_digest_from_database(&connection, &expected.anchors)?;
        if recomputed != expected.logical_digest {
            return Err(digest_mismatch(
                "logical_digest",
                expected.logical_digest.as_str(),
                recomputed.as_str(),
            ));
        }
        if header.logical_digest != recomputed {
            return Err(digest_mismatch(
                "header.logical_digest",
                recomputed.as_str(),
                header.logical_digest.as_str(),
            ));
        }
        let materialization = materialization_id(
            &expected.anchors,
            &recomputed,
            &exact,
            &expected.configuration_digest,
            &expected.sqlite_version,
            &expected.materializer_version,
        );
        if materialization != expected.materialization_id {
            return Err(digest_mismatch(
                "materialization_id",
                expected.materialization_id.as_str(),
                materialization.as_str(),
            ));
        }

        let image = read_logical_image(&mut connection, limits)?;
        let receipt = AxpdReceipt {
            format: AXPD_FORMAT.to_string(),
            schema_version: AXPD_SCHEMA_VERSION,
            sqlite_version: expected.sqlite_version.clone(),
            materializer_version: expected.materializer_version.clone(),
            configuration_digest: expected.configuration_digest.clone(),
            anchors: expected.anchors.clone(),
            logical_digest: recomputed,
            exact_image_digest: exact,
            materialization_id: materialization,
            byte_len,
        };
        Ok(VerifiedAxpd { receipt, image })
    }
}

#[derive(Debug)]
struct MaterializationHeader {
    format: String,
    schema_version: u32,
    application_id: i32,
    sqlite_version: String,
    materializer_version: String,
    repository_id: RepositoryIdV2,
    accepted_snapshot_id: SnapshotIdV2,
    accepted_tree_id: TreeIdV2,
    kernel_ir_digest: ObjectBlobIdV2,
    canonical_fact_log_digest: ObjectBlobIdV2,
    configuration_digest: ObjectBlobIdV2,
    logical_digest: ObjectBlobIdV2,
}

fn build_memory_database(
    spec: &AxpdBuildSpec,
    logical_digest: &ObjectBlobIdV2,
    configuration_digest: &ObjectBlobIdV2,
    sqlite_version: &str,
    limits: &AxpdLimits,
) -> AxpdResult<Connection> {
    let mut connection = Connection::open_in_memory()?;
    configure_connection_limits(&connection, limits)?;
    connection.pragma_update(None, "page_size", spec.configuration.page_size)?;
    connection.pragma_update(None, "application_id", AXPD_APPLICATION_ID)?;
    connection.pragma_update(None, "user_version", AXPD_SCHEMA_VERSION)?;
    connection.pragma_update(None, "journal_mode", "OFF")?;
    connection.pragma_update(None, "synchronous", "OFF")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "trusted_schema", "OFF")?;
    connection.execute_batch(AXPD_SCHEMA)?;
    let transaction = connection.transaction()?;
    insert_header(
        &transaction,
        spec,
        logical_digest,
        configuration_digest,
        sqlite_version,
    )?;
    insert_rows(&transaction, spec)?;
    transaction.commit()?;
    connection.execute_batch("VACUUM;")?;
    validate_database_limits(&connection, limits)?;
    Ok(connection)
}

fn insert_header(
    tx: &Transaction<'_>,
    spec: &AxpdBuildSpec,
    logical_digest: &ObjectBlobIdV2,
    configuration_digest: &ObjectBlobIdV2,
    sqlite_version: &str,
) -> AxpdResult<()> {
    tx.execute(
        "INSERT INTO materialization_meta (
            singleton, format, schema_version, application_id, sqlite_version,
            materializer_version, repository_id, accepted_snapshot_id,
            accepted_tree_id, kernel_ir_digest, canonical_fact_log_digest,
            configuration_digest, logical_digest
         ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            AXPD_FORMAT,
            AXPD_SCHEMA_VERSION,
            AXPD_APPLICATION_ID,
            sqlite_version,
            spec.materializer_version,
            spec.anchors.repository_id.as_str(),
            spec.anchors.accepted_snapshot_id.as_str(),
            spec.anchors.accepted_tree_id.as_str(),
            spec.anchors.kernel_ir_digest.as_str(),
            spec.anchors.canonical_fact_log_digest.as_str(),
            configuration_digest.as_str(),
            logical_digest.as_str(),
        ],
    )?;
    Ok(())
}

fn insert_rows(tx: &Transaction<'_>, spec: &AxpdBuildSpec) -> AxpdResult<()> {
    {
        let mut statement =
            tx.prepare("INSERT INTO module_closure (ordinal, revision_digest) VALUES (?1, ?2)")?;
        for (ordinal, revision) in spec.anchors.ordered_module_closure.iter().enumerate() {
            statement.execute(params![usize_to_i64(ordinal)?, revision.as_str()])?;
        }
    }
    {
        let mut statement = tx.prepare("INSERT INTO kernel_refs (wire_json) VALUES (?1)")?;
        for row in &spec.image.kernel_refs {
            statement.execute(params![row.wire_json])?;
        }
    }
    {
        let mut statement = tx.prepare(
            "INSERT INTO entities (entity_key, instance_id, object_ref_json, value)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for row in &spec.image.entities {
            statement.execute(params![
                row.entity_key.as_str(),
                row.instance_id.as_str(),
                row.object_ref_json,
                row.value,
            ])?;
        }
    }
    {
        let mut statement = tx.prepare(
            "INSERT INTO relation_facts (fact_id, instance_id, relation_id, local_label)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for row in &spec.image.relation_facts {
            statement.execute(params![
                row.fact_id.as_str(),
                row.instance_id.as_str(),
                row.relation_id.as_str(),
                row.local_label,
            ])?;
        }
    }
    {
        let mut statement = tx.prepare(
            "INSERT INTO projections (
                fact_id, ordinal, role_id, value_kind, value,
                target_entity_key, target_fact_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        for row in &spec.image.projections {
            statement.execute(params![
                row.fact_id.as_str(),
                row.ordinal,
                row.role_id.as_str(),
                row.value_kind,
                row.value,
                row.target_entity_key.as_ref().map(ObjectBlobIdV2::as_str),
                row.target_fact_id.as_ref().map(FactIdV2::as_str),
            ])?;
        }
    }
    {
        let mut statement = tx.prepare(
            "INSERT INTO contexts (fact_id, ordinal, role_id, axis_kind, value)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for row in &spec.image.contexts {
            statement.execute(params![
                row.fact_id.as_str(),
                row.ordinal,
                row.role_id.as_str(),
                row.axis_kind,
                row.value,
            ])?;
        }
    }
    {
        let mut statement = tx.prepare(
            "INSERT INTO equivalences (
                instance_id, generator_ref_json, source_entity_key, target_entity_key
             ) VALUES (?1, ?2, ?3, ?4)",
        )?;
        for row in &spec.image.equivalences {
            statement.execute(params![
                row.instance_id.as_str(),
                row.generator_ref_json,
                row.source_entity_key.as_str(),
                row.target_entity_key.as_str(),
            ])?;
        }
    }
    {
        let mut statement =
            tx.prepare("INSERT INTO overlays (ordinal, kind, digest) VALUES (?1, ?2, ?3)")?;
        for row in &spec.image.overlays {
            statement.execute(params![row.ordinal, row.kind, row.digest.as_str()])?;
        }
    }
    Ok(())
}

fn read_header(connection: &Connection) -> AxpdResult<MaterializationHeader> {
    connection
        .query_row(
            "SELECT format, schema_version, application_id, sqlite_version,
                    materializer_version, repository_id, accepted_snapshot_id,
                    accepted_tree_id, kernel_ir_digest, canonical_fact_log_digest,
                    configuration_digest, logical_digest
             FROM materialization_meta WHERE singleton = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, i32>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| invalid("missing materialization header"))
        .and_then(|raw| {
            Ok(MaterializationHeader {
                format: raw.0,
                schema_version: raw.1,
                application_id: raw.2,
                sqlite_version: raw.3,
                materializer_version: raw.4,
                repository_id: parse_id(&raw.5, "repository_id")?,
                accepted_snapshot_id: parse_id(&raw.6, "accepted_snapshot_id")?,
                accepted_tree_id: parse_id(&raw.7, "accepted_tree_id")?,
                kernel_ir_digest: parse_id(&raw.8, "kernel_ir_digest")?,
                canonical_fact_log_digest: parse_id(&raw.9, "canonical_fact_log_digest")?,
                configuration_digest: parse_id(&raw.10, "configuration_digest")?,
                logical_digest: parse_id(&raw.11, "logical_digest")?,
            })
        })
}

fn validate_header(header: &MaterializationHeader, expected: &AxpdExpectation) -> AxpdResult<()> {
    expect_eq("format", AXPD_FORMAT, &header.format)?;
    expect_eq(
        "schema_version",
        &AXPD_SCHEMA_VERSION.to_string(),
        &header.schema_version.to_string(),
    )?;
    expect_eq(
        "application_id",
        &AXPD_APPLICATION_ID.to_string(),
        &header.application_id.to_string(),
    )?;
    expect_eq(
        "sqlite_version",
        &expected.sqlite_version,
        &header.sqlite_version,
    )?;
    expect_eq(
        "materializer_version",
        &expected.materializer_version,
        &header.materializer_version,
    )?;
    expect_id(
        "repository_id",
        &expected.anchors.repository_id,
        &header.repository_id,
    )?;
    expect_id(
        "accepted_snapshot_id",
        &expected.anchors.accepted_snapshot_id,
        &header.accepted_snapshot_id,
    )?;
    expect_id(
        "accepted_tree_id",
        &expected.anchors.accepted_tree_id,
        &header.accepted_tree_id,
    )?;
    expect_id(
        "kernel_ir_digest",
        &expected.anchors.kernel_ir_digest,
        &header.kernel_ir_digest,
    )?;
    expect_id(
        "canonical_fact_log_digest",
        &expected.anchors.canonical_fact_log_digest,
        &header.canonical_fact_log_digest,
    )?;
    expect_id(
        "configuration_digest",
        &expected.configuration_digest,
        &header.configuration_digest,
    )?;
    expect_id(
        "logical_digest",
        &expected.logical_digest,
        &header.logical_digest,
    )?;
    Ok(())
}

#[allow(clippy::field_reassign_with_default)]
fn read_logical_image(
    connection: &mut Connection,
    limits: &AxpdLimits,
) -> AxpdResult<AxpdLogicalImage> {
    let tx = connection.transaction()?;
    let mut image = AxpdLogicalImage::default();
    image.kernel_refs = query_rows(
        &tx,
        "SELECT wire_json FROM kernel_refs ORDER BY wire_json",
        |row| {
            Ok(KernelRefRow {
                wire_json: row.get(0)?,
            })
        },
    )?;
    image.entities = query_rows(
        &tx,
        "SELECT entity_key, instance_id, object_ref_json, value FROM entities ORDER BY entity_key",
        |row| {
            Ok(EntityRow {
                entity_key: parse_sql_id(row.get(0)?)?,
                instance_id: parse_sql_id(row.get(1)?)?,
                object_ref_json: row.get(2)?,
                value: row.get(3)?,
            })
        },
    )?;
    image.relation_facts = query_rows(
        &tx,
        "SELECT fact_id, instance_id, relation_id, local_label FROM relation_facts ORDER BY fact_id",
        |row| {
            Ok(RelationFactRow {
                fact_id: parse_sql_id(row.get(0)?)?,
                instance_id: parse_sql_id(row.get(1)?)?,
                relation_id: parse_sql_id(row.get(2)?)?,
                local_label: row.get(3)?,
            })
        },
    )?;
    image.projections = query_rows(
        &tx,
        "SELECT fact_id, ordinal, role_id, value_kind, value, target_entity_key, target_fact_id
         FROM projections ORDER BY fact_id, ordinal",
        |row| {
            let entity: Option<String> = row.get(5)?;
            let fact: Option<String> = row.get(6)?;
            Ok(ProjectionRow {
                fact_id: parse_sql_id(row.get(0)?)?,
                ordinal: row.get(1)?,
                role_id: parse_sql_id(row.get(2)?)?,
                value_kind: row.get(3)?,
                value: row.get(4)?,
                target_entity_key: entity.map(parse_sql_id).transpose()?,
                target_fact_id: fact.map(parse_sql_id).transpose()?,
            })
        },
    )?;
    image.contexts = query_rows(
        &tx,
        "SELECT fact_id, ordinal, role_id, axis_kind, value
         FROM contexts ORDER BY fact_id, ordinal, role_id",
        |row| {
            Ok(ContextRow {
                fact_id: parse_sql_id(row.get(0)?)?,
                ordinal: row.get(1)?,
                role_id: parse_sql_id(row.get(2)?)?,
                axis_kind: row.get(3)?,
                value: row.get(4)?,
            })
        },
    )?;
    image.equivalences = query_rows(
        &tx,
        "SELECT instance_id, generator_ref_json, source_entity_key, target_entity_key
         FROM equivalences
         ORDER BY instance_id, generator_ref_json, source_entity_key, target_entity_key",
        |row| {
            Ok(EquivalenceRow {
                instance_id: parse_sql_id(row.get(0)?)?,
                generator_ref_json: row.get(1)?,
                source_entity_key: parse_sql_id(row.get(2)?)?,
                target_entity_key: parse_sql_id(row.get(3)?)?,
            })
        },
    )?;
    image.overlays = query_rows(
        &tx,
        "SELECT ordinal, kind, digest FROM overlays ORDER BY ordinal",
        |row| {
            Ok(OverlayRow {
                ordinal: row.get(0)?,
                kind: row.get(1)?,
                digest: parse_sql_id(row.get(2)?)?,
            })
        },
    )?;
    tx.commit()?;
    image.canonicalize_and_validate(limits)?;
    Ok(image)
}

fn query_rows<T, F>(tx: &Transaction<'_>, sql: &str, mut map: F) -> AxpdResult<Vec<T>>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
{
    let mut statement = tx.prepare(sql)?;
    let rows = statement.query_map([], |row| map(row))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

#[allow(clippy::field_reassign_with_default)]
fn logical_digest_from_database(
    connection: &Connection,
    anchors: &AxpdAnchors,
) -> AxpdResult<ObjectBlobIdV2> {
    let mut image = AxpdLogicalImage::default();
    image.kernel_refs = query_rows_connection(
        connection,
        "SELECT wire_json FROM kernel_refs ORDER BY wire_json",
        |row| {
            Ok(KernelRefRow {
                wire_json: row.get(0)?,
            })
        },
    )?;
    image.entities = query_rows_connection(
        connection,
        "SELECT entity_key, instance_id, object_ref_json, value FROM entities ORDER BY entity_key",
        |row| {
            Ok(EntityRow {
                entity_key: parse_sql_id(row.get(0)?)?,
                instance_id: parse_sql_id(row.get(1)?)?,
                object_ref_json: row.get(2)?,
                value: row.get(3)?,
            })
        },
    )?;
    image.relation_facts = query_rows_connection(
        connection,
        "SELECT fact_id, instance_id, relation_id, local_label FROM relation_facts ORDER BY fact_id",
        |row| {
            Ok(RelationFactRow {
                fact_id: parse_sql_id(row.get(0)?)?,
                instance_id: parse_sql_id(row.get(1)?)?,
                relation_id: parse_sql_id(row.get(2)?)?,
                local_label: row.get(3)?,
            })
        },
    )?;
    image.projections = query_rows_connection(
        connection,
        "SELECT fact_id, ordinal, role_id, value_kind, value, target_entity_key, target_fact_id
         FROM projections ORDER BY fact_id, ordinal",
        |row| {
            let entity: Option<String> = row.get(5)?;
            let fact: Option<String> = row.get(6)?;
            Ok(ProjectionRow {
                fact_id: parse_sql_id(row.get(0)?)?,
                ordinal: row.get(1)?,
                role_id: parse_sql_id(row.get(2)?)?,
                value_kind: row.get(3)?,
                value: row.get(4)?,
                target_entity_key: entity.map(parse_sql_id).transpose()?,
                target_fact_id: fact.map(parse_sql_id).transpose()?,
            })
        },
    )?;
    image.contexts = query_rows_connection(
        connection,
        "SELECT fact_id, ordinal, role_id, axis_kind, value
         FROM contexts ORDER BY fact_id, ordinal, role_id",
        |row| {
            Ok(ContextRow {
                fact_id: parse_sql_id(row.get(0)?)?,
                ordinal: row.get(1)?,
                role_id: parse_sql_id(row.get(2)?)?,
                axis_kind: row.get(3)?,
                value: row.get(4)?,
            })
        },
    )?;
    image.equivalences = query_rows_connection(
        connection,
        "SELECT instance_id, generator_ref_json, source_entity_key, target_entity_key
         FROM equivalences
         ORDER BY instance_id, generator_ref_json, source_entity_key, target_entity_key",
        |row| {
            Ok(EquivalenceRow {
                instance_id: parse_sql_id(row.get(0)?)?,
                generator_ref_json: row.get(1)?,
                source_entity_key: parse_sql_id(row.get(2)?)?,
                target_entity_key: parse_sql_id(row.get(3)?)?,
            })
        },
    )?;
    image.overlays = query_rows_connection(
        connection,
        "SELECT ordinal, kind, digest FROM overlays ORDER BY ordinal",
        |row| {
            Ok(OverlayRow {
                ordinal: row.get(0)?,
                kind: row.get(1)?,
                digest: parse_sql_id(row.get(2)?)?,
            })
        },
    )?;
    logical_digest(anchors, &image)
}

fn query_rows_connection<T, F>(connection: &Connection, sql: &str, mut map: F) -> AxpdResult<Vec<T>>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
{
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map([], |row| map(row))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn logical_digest(anchors: &AxpdAnchors, image: &AxpdLogicalImage) -> AxpdResult<ObjectBlobIdV2> {
    let mut framed = FramedHasher::new(b"axiograph_axpd_logical_image_v2");
    framed.field(anchors.repository_id.as_str().as_bytes());
    framed.field(anchors.accepted_snapshot_id.as_str().as_bytes());
    framed.field(anchors.accepted_tree_id.as_str().as_bytes());
    framed.field(anchors.kernel_ir_digest.as_str().as_bytes());
    framed.field(anchors.canonical_fact_log_digest.as_str().as_bytes());
    framed.table("module_closure", anchors.ordered_module_closure.len());
    for (ordinal, revision) in anchors.ordered_module_closure.iter().enumerate() {
        framed.field(&usize_to_u64(ordinal)?.to_be_bytes());
        framed.field(revision.as_str().as_bytes());
    }
    framed.table("kernel_refs", image.kernel_refs.len());
    for row in &image.kernel_refs {
        framed.field(row.wire_json.as_bytes());
    }
    framed.table("entities", image.entities.len());
    for row in &image.entities {
        framed.field(row.entity_key.as_str().as_bytes());
        framed.field(row.instance_id.as_str().as_bytes());
        framed.field(row.object_ref_json.as_bytes());
        framed.field(row.value.as_bytes());
    }
    framed.table("relation_facts", image.relation_facts.len());
    for row in &image.relation_facts {
        framed.field(row.fact_id.as_str().as_bytes());
        framed.field(row.instance_id.as_str().as_bytes());
        framed.field(row.relation_id.as_str().as_bytes());
        framed.optional(row.local_label.as_deref().map(str::as_bytes));
    }
    framed.table("projections", image.projections.len());
    for row in &image.projections {
        framed.field(row.fact_id.as_str().as_bytes());
        framed.field(&row.ordinal.to_be_bytes());
        framed.field(row.role_id.as_str().as_bytes());
        framed.field(row.value_kind.as_bytes());
        framed.field(row.value.as_bytes());
        framed.optional(
            row.target_entity_key
                .as_ref()
                .map(ObjectBlobIdV2::as_str)
                .map(str::as_bytes),
        );
        framed.optional(
            row.target_fact_id
                .as_ref()
                .map(FactIdV2::as_str)
                .map(str::as_bytes),
        );
    }
    framed.table("contexts", image.contexts.len());
    for row in &image.contexts {
        framed.field(row.fact_id.as_str().as_bytes());
        framed.field(&row.ordinal.to_be_bytes());
        framed.field(row.role_id.as_str().as_bytes());
        framed.field(row.axis_kind.as_bytes());
        framed.field(row.value.as_bytes());
    }
    framed.table("equivalences", image.equivalences.len());
    for row in &image.equivalences {
        framed.field(row.instance_id.as_str().as_bytes());
        framed.field(row.generator_ref_json.as_bytes());
        framed.field(row.source_entity_key.as_str().as_bytes());
        framed.field(row.target_entity_key.as_str().as_bytes());
    }
    framed.table("overlays", image.overlays.len());
    for row in &image.overlays {
        framed.field(&row.ordinal.to_be_bytes());
        framed.field(row.kind.as_bytes());
        framed.field(row.digest.as_str().as_bytes());
    }
    Ok(framed.finish_object_id())
}

struct FramedHasher(Sha256);

impl FramedHasher {
    fn new(domain: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"AXIOGRAPH-LOGICAL-IMAGE");
        hasher.update((domain.len() as u64).to_be_bytes());
        hasher.update(domain);
        Self(hasher)
    }

    fn field(&mut self, value: &[u8]) {
        self.0.update((value.len() as u64).to_be_bytes());
        self.0.update(value);
    }

    fn optional(&mut self, value: Option<&[u8]>) {
        match value {
            Some(value) => {
                self.0.update([1]);
                self.field(value);
            }
            None => self.0.update([0]),
        }
    }

    fn table(&mut self, name: &str, count: usize) {
        self.field(name.as_bytes());
        self.0.update((count as u64).to_be_bytes());
    }

    fn finish_object_id(self) -> ObjectBlobIdV2 {
        let digest = self.0.finalize();
        ObjectBlobIdV2::from_canonical_fields(&[
            b"axiograph_axpd_logical_stream_sha256",
            digest.as_slice(),
        ])
    }
}

fn entity_key(instance_id: &InstanceIdV2, object_ref_json: &str, value: &str) -> ObjectBlobIdV2 {
    ObjectBlobIdV2::from_canonical_fields(&[
        b"axiograph_finite_entity_key",
        instance_id.as_str().as_bytes(),
        object_ref_json.as_bytes(),
        value.as_bytes(),
    ])
}

fn materialization_id(
    anchors: &AxpdAnchors,
    logical_digest: &ObjectBlobIdV2,
    exact_image_digest: &ObjectBlobIdV2,
    configuration_digest: &ObjectBlobIdV2,
    sqlite_version: &str,
    materializer_version: &str,
) -> MaterializationIdV2 {
    MaterializationIdV2::from_canonical_fields(&[
        b"axiograph_axpd_materialization",
        &AXPD_SCHEMA_VERSION.to_be_bytes(),
        anchors.repository_id.as_str().as_bytes(),
        anchors.accepted_snapshot_id.as_str().as_bytes(),
        anchors.accepted_tree_id.as_str().as_bytes(),
        anchors.kernel_ir_digest.as_str().as_bytes(),
        anchors.canonical_fact_log_digest.as_str().as_bytes(),
        logical_digest.as_str().as_bytes(),
        exact_image_digest.as_str().as_bytes(),
        configuration_digest.as_str().as_bytes(),
        sqlite_version.as_bytes(),
        materializer_version.as_bytes(),
    ])
}

fn exact_image_digest(path: &Path, max_bytes: u64) -> AxpdResult<ObjectBlobIdV2> {
    let bytes = read_file_bounded(path, max_bytes)?;
    Ok(exact_image_digest_bytes(&bytes))
}

fn exact_image_digest_bytes(bytes: &[u8]) -> ObjectBlobIdV2 {
    let digest = Sha256::digest(bytes);
    ObjectBlobIdV2::from_canonical_fields(&[
        b"axiograph_axpd_exact_image_sha256",
        digest.as_slice(),
    ])
}

fn validate_application_and_schema(connection: &Connection, limits: &AxpdLimits) -> AxpdResult<()> {
    let application_id: i32 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    if application_id != AXPD_APPLICATION_ID {
        return Err(anchor_mismatch(
            "application_id",
            AXPD_APPLICATION_ID.to_string(),
            application_id.to_string(),
        ));
    }
    let user_version: u32 =
        connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if user_version != AXPD_SCHEMA_VERSION {
        return Err(anchor_mismatch(
            "schema_version",
            AXPD_SCHEMA_VERSION.to_string(),
            user_version.to_string(),
        ));
    }
    let page_size: u32 = connection.pragma_query_value(None, "page_size", |row| row.get(0))?;
    if page_size != AXPD_PAGE_SIZE {
        return Err(anchor_mismatch(
            "page_size",
            AXPD_PAGE_SIZE.to_string(),
            page_size.to_string(),
        ));
    }
    let page_count = sqlite_u64(
        connection.pragma_query_value::<i64, _>(None, "page_count", |row| row.get(0))?,
        "page_count",
    )?;
    if page_count > limits.max_page_count {
        return Err(limit_error("page_count", limits.max_page_count, page_count));
    }
    Ok(())
}

fn validate_schema_tables(connection: &Connection) -> AxpdResult<()> {
    let mut statement = connection.prepare(
        "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let found = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let expected = EXPECTED_TABLES
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    if found != expected {
        return Err(invalid(format!(
            "unexpected materialization table set: expected {expected:?}, found {found:?}"
        )));
    }
    Ok(())
}

fn validate_quick_check(connection: &Connection) -> AxpdResult<()> {
    let result: String = connection.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
    if result != "ok" {
        return Err(invalid(format!("SQLite quick_check failed: {result}")));
    }
    Ok(())
}

fn validate_module_closure(
    connection: &Connection,
    expected: &[RevisionDigestV2],
) -> AxpdResult<()> {
    let mut statement = connection
        .prepare("SELECT ordinal, revision_digest FROM module_closure ORDER BY ordinal")?;
    let found = statement
        .query_map([], |row| {
            Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if found.len() != expected.len() {
        return Err(anchor_mismatch(
            "ordered_module_closure.count",
            expected.len().to_string(),
            found.len().to_string(),
        ));
    }
    for (ordinal, ((stored_ordinal, stored_revision), expected_revision)) in
        found.iter().zip(expected).enumerate()
    {
        let ordinal =
            u32::try_from(ordinal).map_err(|_| invalid("module closure ordinal exceeds u32"))?;
        if *stored_ordinal != ordinal {
            return Err(anchor_mismatch(
                "ordered_module_closure.ordinal",
                ordinal.to_string(),
                stored_ordinal.to_string(),
            ));
        }
        if stored_revision != expected_revision.as_str() {
            return Err(anchor_mismatch(
                "ordered_module_closure.revision",
                expected_revision.as_str(),
                stored_revision,
            ));
        }
    }
    Ok(())
}

fn validate_database_limits(connection: &Connection, limits: &AxpdLimits) -> AxpdResult<()> {
    let counts = [
        ("module_count", "module_closure", limits.max_module_count),
        (
            "kernel_ref_count",
            "kernel_refs",
            limits.max_kernel_ref_count,
        ),
        ("entity_count", "entities", limits.max_entity_count),
        (
            "relation_fact_count",
            "relation_facts",
            limits.max_relation_fact_count,
        ),
        (
            "projection_count",
            "projections",
            limits.max_projection_count,
        ),
        ("context_count", "contexts", limits.max_context_count),
        (
            "equivalence_count",
            "equivalences",
            limits.max_equivalence_count,
        ),
        ("overlay_count", "overlays", limits.max_overlay_count),
    ];
    for (name, table, limit) in counts {
        let sql = format!("SELECT count(*) FROM {table}");
        let actual = sqlite_u64(
            connection.query_row::<i64, _, _>(&sql, [], |row| row.get(0))?,
            name,
        )?;
        if actual > limit as u64 {
            return Err(limit_error(name, limit, actual));
        }
    }
    let fanout = sqlite_u64(
        connection.query_row::<i64, _, _>(
            "SELECT COALESCE(MAX(n), 0) FROM (SELECT count(*) AS n FROM projections GROUP BY fact_id)",
            [],
            |row| row.get(0),
        )?,
        "projection_fanout",
    )?;
    if fanout > limits.max_projection_fanout as u64 {
        return Err(limit_error(
            "projection_fanout",
            limits.max_projection_fanout,
            fanout,
        ));
    }
    for (table, column) in STRING_COLUMNS {
        let sql = format!("SELECT COALESCE(MAX(length(CAST({column} AS BLOB))), 0) FROM {table}");
        let actual = sqlite_u64(
            connection.query_row::<i64, _, _>(&sql, [], |row| row.get(0))?,
            "string_bytes",
        )?;
        if actual > limits.max_string_bytes as u64 {
            return Err(limit_error("string_bytes", limits.max_string_bytes, actual));
        }
    }
    Ok(())
}

fn configure_connection_limits(connection: &Connection, limits: &AxpdLimits) -> AxpdResult<()> {
    connection.set_limit(Limit::SQLITE_LIMIT_LENGTH, limits.max_string_bytes as i32)?;
    connection.set_limit(Limit::SQLITE_LIMIT_SQL_LENGTH, 256 * 1024)?;
    connection.set_limit(Limit::SQLITE_LIMIT_COLUMN, 128)?;
    connection.set_limit(Limit::SQLITE_LIMIT_EXPR_DEPTH, 64)?;
    connection.set_limit(Limit::SQLITE_LIMIT_COMPOUND_SELECT, 16)?;
    connection.set_limit(Limit::SQLITE_LIMIT_VARIABLE_NUMBER, 128)?;
    connection.set_limit(Limit::SQLITE_LIMIT_TRIGGER_DEPTH, 1)?;
    // SQLite's deterministic VACUUM path uses one transient attached database.
    connection.set_limit(Limit::SQLITE_LIMIT_ATTACHED, 1)?;
    connection.set_limit(Limit::SQLITE_LIMIT_WORKER_THREADS, 0)?;
    Ok(())
}

fn read_file_bounded(path: &Path, limit: u64) -> AxpdResult<Vec<u8>> {
    // Preserve the typed stable-path error while retaining same-handle
    // enforcement in the shared no-follow reader below.
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(invalid(".axpd input must be a regular non-symlink file"));
    }
    if metadata.len() > limit {
        return Err(limit_error("file_bytes", limit, metadata.len()));
    }
    let limit = usize::try_from(limit).map_err(|_| invalid("file byte limit exceeds usize"))?;
    axiograph_security::read_file_bounded(path, limit, ".axpd materialization")
        .map_err(|error| invalid(error.to_string()))
}

fn validate_sqlite_prefix(bytes: &[u8]) -> AxpdResult<()> {
    if !bytes.starts_with(b"SQLite format 3\0") {
        return Err(invalid(
            "not an authenticated SQLite `.axpd` materialization",
        ));
    }
    Ok(())
}

fn canonical_json<T: Serialize>(value: &T) -> AxpdResult<String> {
    serde_json::to_string(value).map_err(|error| invalid(format!("canonical JSON failed: {error}")))
}

fn role_kind_wire(kind: RoleKindIr) -> &'static str {
    match kind {
        RoleKindIr::Data => "data",
        RoleKindIr::Context => "context",
        RoleKindIr::World => "world",
        RoleKindIr::Temporal => "temporal",
        RoleKindIr::Parameter => "parameter",
        RoleKindIr::Evidence => "evidence",
    }
}

fn validate_count(name: &'static str, actual: usize, limit: usize) -> AxpdResult<()> {
    if actual > limit {
        Err(limit_error(name, limit, actual))
    } else {
        Ok(())
    }
}

fn validate_text(value: &str, field: &'static str, limit: usize) -> AxpdResult<()> {
    if value.len() > limit {
        return Err(limit_error("string_bytes", limit, value.len()));
    }
    if value.contains('\0') {
        return Err(invalid(format!("{field} contains NUL")));
    }
    Ok(())
}

fn reject_duplicate_by<T>(
    label: &str,
    rows: &[T],
    equal: impl Fn(&T, &T) -> bool,
) -> AxpdResult<()> {
    if rows.windows(2).any(|pair| equal(&pair[0], &pair[1])) {
        Err(invalid(format!("duplicate {label} row")))
    } else {
        Ok(())
    }
}

fn limit_error(
    name: &'static str,
    limit: impl TryInto<u64>,
    actual: impl TryInto<u64>,
) -> AxpdError {
    AxpdError::LimitExceeded {
        limit_name: name,
        limit: limit.try_into().ok().unwrap_or(u64::MAX),
        actual: actual.try_into().ok().unwrap_or(u64::MAX),
    }
}

fn anchor_mismatch(
    field: &'static str,
    expected: impl Into<String>,
    actual: impl Into<String>,
) -> AxpdError {
    AxpdError::AnchorMismatch {
        field,
        expected: expected.into(),
        actual: actual.into(),
    }
}

fn digest_mismatch(
    field: &'static str,
    expected: impl Into<String>,
    actual: impl Into<String>,
) -> AxpdError {
    AxpdError::DigestMismatch {
        field,
        expected: expected.into(),
        actual: actual.into(),
    }
}

fn expect_eq(field: &'static str, expected: &str, actual: &str) -> AxpdResult<()> {
    if expected == actual {
        Ok(())
    } else {
        Err(anchor_mismatch(field, expected, actual))
    }
}

fn expect_id<T: ToString + PartialEq>(
    field: &'static str,
    expected: &T,
    actual: &T,
) -> AxpdResult<()> {
    if expected == actual {
        Ok(())
    } else {
        Err(anchor_mismatch(
            field,
            expected.to_string(),
            actual.to_string(),
        ))
    }
}

fn parse_id<T: FromStr>(value: &str, field: &str) -> AxpdResult<T>
where
    T::Err: std::fmt::Display,
{
    value
        .parse()
        .map_err(|error| invalid(format!("invalid {field}: {error}")))
}

fn parse_sql_id<T: FromStr>(value: String) -> rusqlite::Result<T>
where
    T::Err: std::error::Error + Send + Sync + 'static,
{
    value.parse().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            value.len(),
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn usize_to_i64(value: usize) -> AxpdResult<i64> {
    i64::try_from(value).map_err(|_| invalid("value exceeds SQLite integer range"))
}

fn sqlite_u64(value: i64, field: &str) -> AxpdResult<u64> {
    u64::try_from(value).map_err(|_| invalid(format!("negative SQLite value for {field}")))
}

fn usize_to_u64(value: usize) -> AxpdResult<u64> {
    u64::try_from(value).map_err(|_| invalid("value exceeds u64"))
}

fn materialization_file_stem(materialization_id: &MaterializationIdV2) -> String {
    materialization_id
        .as_str()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn materialization_image_path(
    directory: &Path,
    materialization_id: &MaterializationIdV2,
) -> PathBuf {
    directory.join(format!(
        "{}.axpd",
        materialization_file_stem(materialization_id)
    ))
}

fn materialization_receipt_path(
    directory: &Path,
    materialization_id: &MaterializationIdV2,
) -> PathBuf {
    directory.join(format!(
        "{}.receipt.json",
        materialization_file_stem(materialization_id)
    ))
}

fn publish_receipt(
    directory: &Path,
    receipt: &AxpdReceipt,
    injector: &dyn AxpdFailureInjector,
) -> AxpdResult<()> {
    validate_directory(directory)?;
    let path = materialization_receipt_path(directory, &receipt.materialization_id);
    let bytes = serde_json::to_vec(receipt)
        .map_err(|error| invalid(format!("cannot encode materialization receipt: {error}")))?;
    if usize_to_u64(bytes.len())? > AXPD_RECEIPT_MAX_BYTES {
        return Err(limit_error(
            "receipt_bytes",
            AXPD_RECEIPT_MAX_BYTES,
            bytes.len(),
        ));
    }
    if fs::symlink_metadata(&path).is_ok() {
        if read_file_bounded(&path, AXPD_RECEIPT_MAX_BYTES)? == bytes {
            return Ok(());
        }
        return Err(AxpdError::ImmutableCollision { path });
    }

    let temporary = unique_temp_path(directory, path.file_name().and_then(|name| name.to_str()));
    let guard = TemporaryFileGuard(temporary.clone());
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(&bytes)?;
    injector.check(AxpdFailurePoint::AfterReceiptWrite)?;
    file.sync_all()?;
    injector.check(AxpdFailurePoint::AfterReceiptFsync)?;
    drop(file);
    fs::rename(&temporary, &path)?;
    sync_directory(directory)?;
    std::mem::forget(guard);
    injector.check(AxpdFailurePoint::AfterReceiptPublish)?;
    Ok(())
}

fn unique_temp_path(parent: &Path, file_name: Option<&str>) -> PathBuf {
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let file_name = file_name.unwrap_or("materialization.axpd");
    parent.join(format!(
        ".{file_name}.tmp.{}.{}",
        std::process::id(),
        sequence
    ))
}

fn quarantine_path(path: &Path) -> PathBuf {
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("materialization.axpd");
    parent.join(format!("{name}.quarantine.{sequence}"))
}

fn sync_file(path: &Path) -> AxpdResult<()> {
    let maximum = usize::try_from(AxpdLimits::default().max_file_bytes)
        .map_err(|_| invalid("file byte limit exceeds usize"))?;
    let (file, _) =
        axiograph_security::open_regular_file_bounded(path, maximum, ".axpd publication file")
            .map_err(|error| invalid(error.to_string()))?;
    file.sync_all()?;
    Ok(())
}

fn sync_directory(path: &Path) -> AxpdResult<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    Ok(())
}

fn validate_directory(path: &Path) -> AxpdResult<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(invalid(format!(
            "`{}` must be a real directory, not a symlink",
            path.display()
        )));
    }
    Ok(())
}

struct TemporaryFileGuard(PathBuf);

impl Drop for TemporaryFileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

const EXPECTED_TABLES: &[&str] = &[
    "contexts",
    "entities",
    "equivalences",
    "kernel_refs",
    "materialization_meta",
    "module_closure",
    "overlays",
    "projections",
    "relation_facts",
];

const STRING_COLUMNS: &[(&str, &str)] = &[
    ("materialization_meta", "format"),
    ("materialization_meta", "sqlite_version"),
    ("materialization_meta", "materializer_version"),
    ("materialization_meta", "repository_id"),
    ("materialization_meta", "accepted_snapshot_id"),
    ("materialization_meta", "accepted_tree_id"),
    ("materialization_meta", "kernel_ir_digest"),
    ("materialization_meta", "canonical_fact_log_digest"),
    ("materialization_meta", "configuration_digest"),
    ("materialization_meta", "logical_digest"),
    ("module_closure", "revision_digest"),
    ("kernel_refs", "wire_json"),
    ("entities", "entity_key"),
    ("entities", "instance_id"),
    ("entities", "object_ref_json"),
    ("entities", "value"),
    ("relation_facts", "fact_id"),
    ("relation_facts", "instance_id"),
    ("relation_facts", "relation_id"),
    ("relation_facts", "local_label"),
    ("projections", "fact_id"),
    ("projections", "role_id"),
    ("projections", "value_kind"),
    ("projections", "value"),
    ("projections", "target_entity_key"),
    ("projections", "target_fact_id"),
    ("contexts", "fact_id"),
    ("contexts", "role_id"),
    ("contexts", "axis_kind"),
    ("contexts", "value"),
    ("equivalences", "instance_id"),
    ("equivalences", "generator_ref_json"),
    ("equivalences", "source_entity_key"),
    ("equivalences", "target_entity_key"),
    ("overlays", "kind"),
    ("overlays", "digest"),
];

const AXPD_SCHEMA: &str = r#"
CREATE TABLE materialization_meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    format TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    application_id INTEGER NOT NULL,
    sqlite_version TEXT NOT NULL,
    materializer_version TEXT NOT NULL,
    repository_id TEXT NOT NULL,
    accepted_snapshot_id TEXT NOT NULL,
    accepted_tree_id TEXT NOT NULL,
    kernel_ir_digest TEXT NOT NULL,
    canonical_fact_log_digest TEXT NOT NULL,
    configuration_digest TEXT NOT NULL,
    logical_digest TEXT NOT NULL
) STRICT, WITHOUT ROWID;

CREATE TABLE module_closure (
    ordinal INTEGER PRIMARY KEY CHECK (ordinal >= 0),
    revision_digest TEXT NOT NULL UNIQUE
) STRICT, WITHOUT ROWID;

CREATE TABLE kernel_refs (
    wire_json TEXT PRIMARY KEY
) STRICT, WITHOUT ROWID;

CREATE TABLE entities (
    entity_key TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL,
    object_ref_json TEXT NOT NULL,
    value TEXT NOT NULL
) STRICT, WITHOUT ROWID;

CREATE TABLE relation_facts (
    fact_id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL,
    relation_id TEXT NOT NULL,
    local_label TEXT
) STRICT, WITHOUT ROWID;

CREATE TABLE projections (
    fact_id TEXT NOT NULL REFERENCES relation_facts(fact_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    role_id TEXT NOT NULL,
    value_kind TEXT NOT NULL CHECK (value_kind IN ('object_element', 'relation_fact')),
    value TEXT NOT NULL,
    target_entity_key TEXT REFERENCES entities(entity_key),
    target_fact_id TEXT REFERENCES relation_facts(fact_id),
    PRIMARY KEY (fact_id, ordinal),
    CHECK (
        (value_kind = 'object_element' AND target_entity_key IS NOT NULL AND target_fact_id IS NULL)
        OR
        (value_kind = 'relation_fact' AND target_entity_key IS NULL AND target_fact_id IS NOT NULL)
    )
) STRICT, WITHOUT ROWID;

CREATE TABLE contexts (
    fact_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    role_id TEXT NOT NULL,
    axis_kind TEXT NOT NULL CHECK (axis_kind IN ('context', 'world', 'temporal')),
    value TEXT NOT NULL,
    PRIMARY KEY (fact_id, ordinal, role_id),
    FOREIGN KEY (fact_id, ordinal) REFERENCES projections(fact_id, ordinal)
) STRICT, WITHOUT ROWID;

CREATE TABLE equivalences (
    instance_id TEXT NOT NULL,
    generator_ref_json TEXT NOT NULL,
    source_entity_key TEXT NOT NULL REFERENCES entities(entity_key),
    target_entity_key TEXT NOT NULL REFERENCES entities(entity_key),
    PRIMARY KEY (instance_id, generator_ref_json, source_entity_key, target_entity_key)
) STRICT, WITHOUT ROWID;

CREATE TABLE overlays (
    ordinal INTEGER PRIMARY KEY CHECK (ordinal >= 0),
    kind TEXT NOT NULL,
    digest TEXT NOT NULL UNIQUE
) STRICT, WITHOUT ROWID;
"#;
