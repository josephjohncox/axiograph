use crate::materialization::{
    AxpdAnchors, AxpdBuildSpec, AxpdFailureInjector, AxpdLimits, AxpdMaterializer, AxpdReceipt,
    AxpdRecoveryReport, AxpdResult, VerifiedAxpd,
};
use crate::model::*;
use axiograph_kernel::{
    CanonicalCompiler, CanonicalModuleSource, CommitIdV2, KernelCompilationRequest,
    MaterializationIdV2, ObjectBlobIdV2, ReconciliationIdV2, RepositoryIdV2, RevisionDigestV2,
    SnapshotIdV2, TreeIdV2,
};
use rusqlite::limits::Limit;
use rusqlite::{
    params, Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use thiserror::Error;

const CATALOG_FILE: &str = "catalog.sqlite";
const OBJECTS_DIR: &str = "objects/sha256";
pub const AXI_STORE_APPLICATION_ID: i32 = 0x4158_4953; // `AXIS`
pub const AXI_STORE_SCHEMA_VERSION: u32 = 2;
pub const AXI_STORE_PAGE_SIZE: u32 = 4096;
pub const AXI_STORE_MAX_CATALOG_BYTES: u64 = 512 * 1024 * 1024;
pub const AXI_STORE_MAX_OBJECT_BYTES: u64 = 64 * 1024 * 1024;
pub const AXI_STORE_MAX_PUBLICATION_BYTES: u64 = 512 * 1024 * 1024;
pub const AXI_STORE_MAX_PUBLICATION_OBJECTS: usize = 16_384;
pub const AXI_STORE_MAX_MODULES: usize = 4096;
pub const AXI_STORE_MAX_REFS: usize = 16_384;
const MAX_REACHABLE_OBJECTS: usize = 131_072;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Error)]
pub enum AxiStoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid semantic object: {0}")]
    Model(#[from] ModelError),
    #[error("store corruption: {0}")]
    Corruption(String),
    #[error("store limit `{name}` exceeded: limit {limit}, actual {actual}")]
    LimitExceeded {
        name: &'static str,
        limit: u64,
        actual: u64,
    },
    #[error("invalid store operation: {0}")]
    Invalid(String),
    #[error("stale accepted state: expected generation {expected}, actual generation {actual}")]
    StaleState { expected: u64, actual: u64 },
    #[error("immutable object collision for `{object_id}`")]
    ObjectCollision { object_id: String },
    #[error("ref `{ref_name}` expected {expected:?}, actual {actual:?}")]
    StaleRef {
        ref_name: String,
        expected: Option<CommitIdV2>,
        actual: Option<CommitIdV2>,
    },
    #[error("tag `{0}` already exists and is immutable")]
    ImmutableTag(String),
    #[error("no common merge base exists")]
    NoMergeBase,
    #[error("merge base is ambiguous; maximal common ancestors: {candidates:?}")]
    AmbiguousMergeBase { candidates: Vec<CommitIdV2> },
    #[error("failure injected at {0:?}")]
    Injected(FailurePoint),
}

pub type StoreResult<T> = Result<T, AxiStoreError>;

fn invalid(message: impl Into<String>) -> AxiStoreError {
    AxiStoreError::Invalid(message.into())
}

fn corrupt(message: impl Into<String>) -> AxiStoreError {
    AxiStoreError::Corruption(message.into())
}

fn limit_exceeded(name: &'static str, limit: u64, actual: u64) -> AxiStoreError {
    AxiStoreError::LimitExceeded {
        name,
        limit,
        actual,
    }
}

fn validate_count(name: &'static str, actual: usize, limit: usize) -> StoreResult<()> {
    if actual > limit {
        return Err(limit_exceeded(name, limit as u64, actual as u64));
    }
    Ok(())
}

fn validate_byte_len(name: &'static str, actual: usize, limit: u64) -> StoreResult<()> {
    let actual = u64::try_from(actual).map_err(|_| limit_exceeded(name, limit, u64::MAX))?;
    if actual > limit {
        return Err(limit_exceeded(name, limit, actual));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailurePoint {
    ObjectWrite(usize),
    ObjectFsync(usize),
    ObjectPublish(usize),
    CatalogBegin,
    CatalogEvent,
    CatalogState,
    CatalogCommit,
}

pub trait FailureInjector: Send + Sync {
    fn check(&self, point: &FailurePoint) -> StoreResult<()>;
}

#[derive(Debug, Default)]
pub struct NoFailure;

impl FailureInjector for NoFailure {
    fn check(&self, _point: &FailurePoint) -> StoreResult<()> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct FailAt(pub FailurePoint);

impl FailureInjector for FailAt {
    fn check(&self, point: &FailurePoint) -> StoreResult<()> {
        if &self.0 == point {
            Err(AxiStoreError::Injected(point.clone()))
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImmutableBlob {
    pub kind: ImmutableObjectKind,
    pub digest: ObjectBlobIdV2,
    pub bytes: Vec<u8>,
}

impl ImmutableBlob {
    pub fn new(kind: ImmutableObjectKind, bytes: Vec<u8>) -> StoreResult<Self> {
        if matches!(
            kind,
            ImmutableObjectKind::RepositoryDescriptor
                | ImmutableObjectKind::AxiModule
                | ImmutableObjectKind::AcceptedTree
                | ImmutableObjectKind::AcceptedSnapshot
                | ImmutableObjectKind::BuildManifest
                | ImmutableObjectKind::Reconciliation
                | ImmutableObjectKind::SemanticCommit
        ) {
            return Err(invalid(
                "structured authority objects are constructed by PromotionPlan",
            ));
        }
        Ok(Self {
            digest: blob_id(kind, &bytes),
            kind,
            bytes,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ModulePublication {
    pub module: AcceptedModule,
    pub exact_bytes: Vec<u8>,
}

impl ModulePublication {
    pub fn new(
        repository_id: &RepositoryIdV2,
        module_name: impl Into<String>,
        exact_bytes: Vec<u8>,
    ) -> StoreResult<Self> {
        Ok(Self {
            module: AcceptedModule::from_bytes(repository_id, module_name, &exact_bytes)?,
            exact_bytes,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RefUpdate {
    pub name: String,
    pub expected: Option<CommitIdV2>,
    pub target: CommitIdV2,
}

#[derive(Debug, Clone)]
pub struct PromotionPlan {
    pub modules: Vec<ModulePublication>,
    pub objects: Vec<ImmutableBlob>,
    pub tree: AcceptedTree,
    pub snapshot: AcceptedSnapshot,
    pub manifest: AcceptedBuildManifest,
    pub reconciliation: Option<SemReconciliationV2>,
    pub commit: SemCommitV2,
    pub ref_updates: Vec<RefUpdate>,
}

fn manifest_gate_reports(manifest: &AcceptedBuildManifest) -> CandidateGateReportsV2 {
    CandidateGateReportsV2 {
        canonical_validation: manifest.validation_report_digest.clone(),
        competency_questions: manifest.competency_question_report_digest.clone(),
        trust: manifest.trusted_checker_receipt_digest.clone(),
        runtime_theory: manifest.runtime_theory_report_digest.clone(),
    }
}

fn validate_commit_gate_anchors(
    commit: &SemCommitV2,
    manifest: &AcceptedBuildManifest,
) -> StoreResult<()> {
    let expected = manifest_gate_reports(manifest);
    for gate in &commit.gates {
        let exact = match gate.kind {
            GateKind::CanonicalValidation => &expected.canonical_validation,
            GateKind::CompetencyQuestions => &expected.competency_questions,
            GateKind::Trust => &expected.trust,
            GateKind::RuntimeTheory => &expected.runtime_theory,
        };
        if gate.decision != GateDecision::Passed || &gate.report_digest != exact {
            return Err(invalid(
                "commit gate decisions must pass and bind the exact build-manifest reports",
            ));
        }
    }
    Ok(())
}

impl PromotionPlan {
    fn validate_static(&self, repository_id: &RepositoryIdV2) -> StoreResult<()> {
        validate_count(
            "publication_modules",
            self.modules.len(),
            AXI_STORE_MAX_MODULES,
        )?;
        validate_count(
            "publication_objects",
            self.objects.len(),
            AXI_STORE_MAX_PUBLICATION_OBJECTS,
        )?;
        validate_count(
            "publication_refs",
            self.ref_updates.len(),
            AXI_STORE_MAX_REFS,
        )?;
        for module in &self.modules {
            validate_byte_len(
                "accepted_module_bytes",
                module.exact_bytes.len(),
                AXI_STORE_MAX_OBJECT_BYTES,
            )?;
        }
        for object in &self.objects {
            validate_byte_len(
                "immutable_object_bytes",
                object.bytes.len(),
                AXI_STORE_MAX_OBJECT_BYTES,
            )?;
        }
        self.tree.validate()?;
        self.snapshot.validate()?;
        self.manifest.validate()?;
        self.commit.validate()?;
        if let Some(reconciliation) = &self.reconciliation {
            reconciliation.validate()?;
        }
        for bound_repository in [
            &self.tree.repository_id,
            &self.snapshot.repository_id,
            &self.manifest.repository_id,
            &self.commit.repository_id,
        ] {
            if bound_repository != repository_id {
                return Err(invalid("promotion object binds a different repository"));
            }
        }
        if self
            .reconciliation
            .as_ref()
            .is_some_and(|value| &value.repository_id != repository_id)
        {
            return Err(invalid("reconciliation binds a different repository"));
        }
        if self.snapshot.tree_id != self.tree.tree_id
            || self.manifest.accepted_tree_id != self.tree.tree_id
            || self.manifest.accepted_snapshot_id != self.snapshot.snapshot_id
            || self.commit.accepted_tree_id != self.tree.tree_id
            || self.commit.accepted_snapshot_id != self.snapshot.snapshot_id
            || self.commit.build_manifest_digest != self.manifest.digest()?
        {
            return Err(invalid(
                "promotion tree/snapshot/manifest/commit anchors disagree",
            ));
        }
        if self.commit.reconciliation_id
            != self
                .reconciliation
                .as_ref()
                .map(|value| value.reconciliation_id.clone())
        {
            return Err(invalid(
                "commit does not bind the exact supplied reconciliation",
            ));
        }
        validate_commit_gate_anchors(&self.commit, &self.manifest)?;
        if let Some(reconciliation) = &self.reconciliation {
            if reconciliation.outcome != ReconciliationOutcomeV2::Materialized
                || self.commit.kind != CommitKind::Merge
                || self.commit.ordered_parents
                    != [
                        reconciliation.left.commit_id.clone(),
                        reconciliation.right.commit_id.clone(),
                    ]
                || reconciliation.merged.candidate.accepted_snapshot_id != self.snapshot.snapshot_id
                || reconciliation.merged.candidate.accepted_tree_id != self.tree.tree_id
                || reconciliation.merged.candidate.kernel_ir_digest
                    != self.manifest.kernel_ir_digest
                || reconciliation.merged.gates != manifest_gate_reports(&self.manifest)
            {
                return Err(invalid(
                    "merge commit, reviewed merged candidate, and exact two-parent reconciliation anchors disagree",
                ));
            }
        }

        let mut modules = self.modules.iter().collect::<Vec<_>>();
        modules.sort_by(|left, right| left.module.module_name.cmp(&right.module.module_name));
        if modules.len() != self.tree.modules.len() {
            return Err(invalid(
                "promotion module closure does not match accepted tree",
            ));
        }
        for (publication, expected) in modules.iter().zip(&self.tree.modules) {
            let recomputed = AcceptedModule::from_bytes(
                repository_id,
                publication.module.module_name.clone(),
                &publication.exact_bytes,
            )?;
            if recomputed != publication.module || &publication.module != expected {
                return Err(invalid(format!(
                    "exact bytes for module `{}` do not match accepted tree",
                    expected.module_name
                )));
            }
        }
        let closure = self
            .tree
            .modules
            .iter()
            .map(|module| module.revision_digest.clone())
            .collect::<Vec<_>>();
        if self.manifest.ordered_module_closure != closure {
            return Err(invalid(
                "build manifest closure does not exactly match accepted tree order",
            ));
        }
        let mut object_ids = BTreeSet::new();
        for object in &self.objects {
            if object.digest != blob_id(object.kind, &object.bytes) {
                return Err(invalid(
                    "immutable blob digest does not match exact bytes and kind",
                ));
            }
            if !object_ids.insert(object.digest.clone()) {
                return Err(invalid("promotion contains duplicate immutable object ids"));
            }
        }
        let mut refs = BTreeSet::new();
        for update in &self.ref_updates {
            validate_ref_name(&update.name, false)?;
            if update.name == "heads/main" || update.name.starts_with("tags/") {
                return Err(invalid(
                    "promotion ref updates may not bypass protected main or create tags",
                ));
            }
            if update.target != self.commit.commit_id {
                return Err(invalid(
                    "promotion branch updates must target the promoted commit",
                ));
            }
            if !refs.insert(update.name.as_str()) {
                return Err(invalid("promotion repeats a ref update"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RefStatus {
    pub name: String,
    pub target: CommitIdV2,
    pub generation: u64,
    pub immutable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StoreStatus {
    pub state: StoreState,
    pub state_digest: ObjectBlobIdV2,
    pub refs: Vec<RefStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineagePin {
    AcceptedState(ObjectBlobIdV2),
    SubjectCommit(CommitIdV2),
}

#[derive(Debug, Clone)]
pub struct AxiStore {
    root: PathBuf,
}

#[derive(Debug, Clone)]
struct PlannedObject {
    id: String,
    kind: ImmutableObjectKind,
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct AuditRow {
    sequence: u64,
    event_id: ObjectBlobIdV2,
    previous: Option<ObjectBlobIdV2>,
    action: String,
    generation: u64,
    accepted_snapshot: Option<SnapshotIdV2>,
    accepted_commit: Option<CommitIdV2>,
    ref_map_digest: ObjectBlobIdV2,
    manifest: Option<ObjectBlobIdV2>,
}

impl AxiStore {
    pub fn init(root: impl Into<PathBuf>, descriptor: &RepositoryDescriptor) -> StoreResult<Self> {
        descriptor.validate()?;
        let requested_root = root.into();
        fs::create_dir_all(&requested_root)?;
        let metadata = fs::symlink_metadata(&requested_root)?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
            return Err(corrupt(
                "AxiStore root must be a real directory, not a symlink",
            ));
        }
        fs::create_dir_all(requested_root.join(OBJECTS_DIR))?;
        let store = Self {
            root: requested_root,
        };
        let descriptor_bytes = descriptor.canonical_bytes()?;
        let repository_id = descriptor.repository_id()?;
        let descriptor_object = PlannedObject {
            id: repository_id.to_string(),
            kind: ImmutableObjectKind::RepositoryDescriptor,
            bytes: descriptor_bytes,
        };
        store.publish_object(&descriptor_object, 0, &NoFailure)?;
        let mut connection = store.connect(true)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<String> = transaction
            .query_row(
                "SELECT repository_id FROM repository WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            if existing != repository_id.as_str() {
                return Err(invalid(
                    "store is already initialized for a different repository",
                ));
            }
            transaction.rollback()?;
            return store.open_validated();
        }
        store.insert_object_metadata(&transaction, &descriptor_object)?;
        transaction.execute(
            "INSERT INTO repository(singleton, repository_id, descriptor_object_id) VALUES(1, ?1, ?2)",
            params![repository_id.as_str(), repository_id.as_str()],
        )?;
        let ref_map_digest = empty_ref_map_digest();
        let event_id = audit_digest(
            &repository_id,
            0,
            None,
            "repository_genesis",
            0,
            None,
            None,
            &ref_map_digest,
            None,
        );
        transaction.execute(
            "INSERT INTO audit_events(sequence, event_id, previous_event_id, action, generation, accepted_snapshot_id, accepted_commit_id, ref_map_digest, build_manifest_digest) VALUES(0, ?1, NULL, 'repository_genesis', 0, NULL, NULL, ?2, NULL)",
            params![event_id.as_str(), ref_map_digest.as_str()],
        )?;
        transaction.execute(
            "INSERT INTO store_state(singleton, repository_id, generation, accepted_snapshot_id, accepted_commit_id, ref_map_digest, build_manifest_digest, audit_event_tail) VALUES(1, ?1, 0, NULL, NULL, ?2, NULL, ?3)",
            params![repository_id.as_str(), ref_map_digest.as_str(), event_id.as_str()],
        )?;
        validate_catalog_counts(&transaction)?;
        transaction.commit()?;
        store.open_validated()
    }

    pub fn open(root: impl Into<PathBuf>) -> StoreResult<Self> {
        let requested_root = root.into();
        let metadata = fs::symlink_metadata(&requested_root)?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
            return Err(corrupt(
                "AxiStore root must be a real directory, not a symlink",
            ));
        }
        Self {
            root: requested_root,
        }
        .open_validated()
    }

    fn open_validated(self) -> StoreResult<Self> {
        self.status()?;
        Ok(self)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn publish_axpd(
        &self,
        spec: AxpdBuildSpec,
        limits: &AxpdLimits,
    ) -> AxpdResult<AxpdReceipt> {
        self.require_axpd_anchors(&spec.anchors)?;
        AxpdMaterializer::publish_in_store(&self.root, spec, limits)
    }

    #[doc(hidden)]
    pub fn publish_axpd_with_injector(
        &self,
        spec: AxpdBuildSpec,
        limits: &AxpdLimits,
        injector: &dyn AxpdFailureInjector,
    ) -> AxpdResult<AxpdReceipt> {
        self.require_axpd_anchors(&spec.anchors)?;
        AxpdMaterializer::publish_in_store_with_injector(&self.root, spec, limits, injector)
    }

    pub fn open_axpd(
        &self,
        materialization_id: &MaterializationIdV2,
        limits: &AxpdLimits,
    ) -> AxpdResult<VerifiedAxpd> {
        let verified = AxpdMaterializer::open_from_store(&self.root, materialization_id, limits)?;
        self.require_axpd_anchors(&verified.receipt().anchors)?;
        Ok(verified)
    }

    pub fn recover_axpd(
        &self,
        spec: AxpdBuildSpec,
        limits: &AxpdLimits,
    ) -> AxpdResult<AxpdRecoveryReport> {
        self.require_axpd_anchors(&spec.anchors)?;
        AxpdMaterializer::recover_in_store(&self.root, spec, limits)
    }

    pub fn axpd_image_path(&self, materialization_id: &MaterializationIdV2) -> PathBuf {
        AxpdMaterializer::image_path_in_store(&self.root, materialization_id)
    }

    pub fn axpd_receipt_path(&self, materialization_id: &MaterializationIdV2) -> PathBuf {
        AxpdMaterializer::receipt_path_in_store(&self.root, materialization_id)
    }

    fn require_axpd_anchors(&self, anchors: &AxpdAnchors) -> AxpdResult<()> {
        let status = self.status().map_err(|error| {
            crate::materialization::AxpdError::Invalid(format!(
                "AxiStore validation failed before materialization access: {error}"
            ))
        })?;
        if status.state.repository_id != anchors.repository_id {
            return Err(crate::materialization::AxpdError::AnchorMismatch {
                field: "repository_id",
                expected: status.state.repository_id.to_string(),
                actual: anchors.repository_id.to_string(),
            });
        }
        let connection = self.connect(false).map_err(|error| {
            crate::materialization::AxpdError::Invalid(format!(
                "cannot reopen validated AxiStore catalog: {error}"
            ))
        })?;
        let mut statement = connection
            .prepare(
                "SELECT DISTINCT bm.manifest_digest FROM build_manifests bm JOIN semantic_commits sc ON sc.manifest_digest=bm.manifest_digest JOIN audit_events ae ON ae.accepted_commit_id=sc.commit_id WHERE bm.repository_id=?1 AND bm.snapshot_id=?2 AND bm.tree_id=?3 ORDER BY bm.manifest_digest",
            )
            .map_err(|error| crate::materialization::AxpdError::Invalid(error.to_string()))?;
        let rows = statement
            .query_map(
                params![
                    anchors.repository_id.as_str(),
                    anchors.accepted_snapshot_id.as_str(),
                    anchors.accepted_tree_id.as_str(),
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| crate::materialization::AxpdError::Invalid(error.to_string()))?;
        for row in rows {
            let digest: ObjectBlobIdV2 = row
                .map_err(|error| crate::materialization::AxpdError::Invalid(error.to_string()))?
                .parse()
                .map_err(|error: axiograph_kernel::IdentityError| {
                    crate::materialization::AxpdError::Invalid(error.to_string())
                })?;
            let manifest = load_manifest_connection(&self.root, &connection, &digest)
                .map_err(|error| crate::materialization::AxpdError::Invalid(error.to_string()))?;
            if manifest.ordered_module_closure == anchors.ordered_module_closure
                && manifest.kernel_ir_digest == anchors.kernel_ir_digest
                && manifest.canonical_fact_log_digest == anchors.canonical_fact_log_digest
            {
                validate_manifest_objects(&self.root, &connection, &manifest).map_err(|error| {
                    crate::materialization::AxpdError::Invalid(error.to_string())
                })?;
                return Ok(());
            }
        }
        Err(crate::materialization::AxpdError::AnchorMismatch {
            field: "accepted_build_manifest",
            expected: "an exact accepted AxiStore manifest closure".to_string(),
            actual: format!(
                "snapshot={}, tree={}, kernel={}, fact_log={}",
                anchors.accepted_snapshot_id,
                anchors.accepted_tree_id,
                anchors.kernel_ir_digest,
                anchors.canonical_fact_log_digest
            ),
        })
    }

    fn connect(&self, create_schema: bool) -> StoreResult<Connection> {
        fs::create_dir_all(&self.root)?;
        let root_metadata = fs::symlink_metadata(&self.root)?;
        if !root_metadata.file_type().is_dir() || root_metadata.file_type().is_symlink() {
            return Err(corrupt(
                "AxiStore root must be a real directory, not a symlink",
            ));
        }

        // SQLite's NOFOLLOW policy rejects symlinks in any path component on
        // supported Unix VFSes. Preserve the caller-facing root spelling while
        // opening the mutable catalog through its canonical parent (for
        // example, macOS `/var` is a system symlink to `/private/var`).
        let catalog = fs::canonicalize(&self.root)?.join(CATALOG_FILE);
        let existed = match fs::symlink_metadata(&catalog) {
            Ok(_) => {
                validate_catalog_file(&catalog)?;
                true
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };
        if !create_schema && !existed {
            return Err(corrupt("missing AxiStore SQLite catalog"));
        }

        let mut flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW;
        if create_schema {
            flags |= OpenFlags::SQLITE_OPEN_CREATE;
        }
        let connection = Connection::open_with_flags(&catalog, flags)?;
        connection.busy_timeout(Duration::from_secs(10))?;
        configure_catalog_connection_limits(&connection)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "trusted_schema", "OFF")?;
        if existed {
            validate_catalog_schema(&connection)?;
        } else {
            connection.pragma_update(None, "page_size", AXI_STORE_PAGE_SIZE)?;
            connection.pragma_update(None, "application_id", AXI_STORE_APPLICATION_ID)?;
            connection.pragma_update(None, "user_version", AXI_STORE_SCHEMA_VERSION)?;
            connection.pragma_update(
                None,
                "max_page_count",
                u64_to_i64(AXI_STORE_MAX_CATALOG_BYTES / u64::from(AXI_STORE_PAGE_SIZE))?,
            )?;
            connection.execute_batch(SCHEMA)?;
        }
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        connection.pragma_update(None, "wal_autocheckpoint", 1000)?;
        connection.pragma_update(
            None,
            "journal_size_limit",
            u64_to_i64(AXI_STORE_MAX_CATALOG_BYTES)?,
        )?;
        Ok(connection)
    }

    pub fn promote(
        &self,
        expected_generation: u64,
        plan: &PromotionPlan,
    ) -> StoreResult<StoreStatus> {
        self.promote_with_injector(expected_generation, plan, &NoFailure)
    }

    pub fn promote_with_injector(
        &self,
        expected_generation: u64,
        plan: &PromotionPlan,
        injector: &dyn FailureInjector,
    ) -> StoreResult<StoreStatus> {
        if plan.commit.kind == CommitKind::Merge {
            return Err(invalid(
                "merge commits materialize only through authenticated materialize_merge",
            ));
        }
        self.advance_main(expected_generation, plan, injector, None)
    }

    /// Materialize one reviewed merge on protected main. The target is the
    /// exact current `heads/main`; the source must be the exact current tip of
    /// `source_ref`; and the resulting commit must have ordered parents
    /// `[target_tip, source_tip]`. No generic/single-parent merge writer exists.
    pub fn materialize_merge(
        &self,
        expected_generation: u64,
        source_ref: &str,
        expected_source_tip: &CommitIdV2,
        plan: &PromotionPlan,
    ) -> StoreResult<StoreStatus> {
        validate_ref_name(source_ref, false)?;
        if source_ref == "heads/main" || !source_ref.starts_with("heads/") {
            return Err(invalid(
                "merge source must be an authenticated non-main branch ref",
            ));
        }
        if plan.commit.kind != CommitKind::Merge || !plan.ref_updates.is_empty() {
            return Err(invalid(
                "materialize_merge requires one merge commit and no side ref updates",
            ));
        }
        self.advance_main(
            expected_generation,
            plan,
            &NoFailure,
            Some((source_ref, expected_source_tip)),
        )
    }

    fn advance_main(
        &self,
        expected_generation: u64,
        plan: &PromotionPlan,
        injector: &dyn FailureInjector,
        merge_source: Option<(&str, &CommitIdV2)>,
    ) -> StoreResult<StoreStatus> {
        let repository_id = self.repository_id()?;
        plan.validate_static(&repository_id)?;
        self.validate_promotion_objects_exist_or_supplied(plan)?;
        let objects = self.plan_objects(plan)?;
        for (index, object) in objects.iter().enumerate() {
            self.publish_object(object, index, injector)?;
        }

        let mut connection = self.connect(false)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        injector.check(&FailurePoint::CatalogBegin)?;
        let old = read_state(&transaction)?;
        if old.generation != expected_generation {
            return Err(AxiStoreError::StaleState {
                expected: expected_generation,
                actual: old.generation,
            });
        }
        self.validate_main_transition(&transaction, &old, plan)?;
        match (plan.commit.kind, merge_source) {
            (CommitKind::Normal, None) => {}
            (CommitKind::Merge, Some((source_ref, expected_source_tip))) => {
                if plan.commit.ordered_parents.get(1) != Some(expected_source_tip) {
                    return Err(invalid(
                        "merge source authentication does not match the exact second parent",
                    ));
                }
                let actual = query_ref_target(&transaction, source_ref)?;
                if actual.as_ref() != Some(expected_source_tip) {
                    return Err(AxiStoreError::StaleRef {
                        ref_name: source_ref.to_string(),
                        expected: Some(expected_source_tip.clone()),
                        actual,
                    });
                }
            }
            (CommitKind::Merge, None) => {
                return Err(invalid(
                    "merge materialization lacks source-ref authentication",
                ));
            }
            (CommitKind::Normal, Some(_)) => {
                return Err(invalid(
                    "normal promotion cannot carry merge authentication",
                ));
            }
        }
        for object in &objects {
            self.insert_object_metadata(&transaction, object)?;
        }
        self.insert_tree(&transaction, &plan.tree)?;
        self.insert_snapshot(&transaction, &plan.snapshot)?;
        self.insert_manifest(&transaction, &plan.manifest)?;
        self.validate_plan_object_closure(&transaction, plan)?;
        self.validate_parent_alignment(&transaction, plan)?;
        if let Some(reconciliation) = &plan.reconciliation {
            self.validate_reconciliation_base(&transaction, reconciliation)?;
            self.validate_reconciliation_candidates(&transaction, plan, reconciliation)?;
            self.insert_reconciliation(&transaction, reconciliation)?;
        }
        self.insert_commit(&transaction, &plan.commit)?;
        self.upsert_ref(
            &transaction,
            "heads/main",
            old.accepted_commit_id.as_ref(),
            &plan.commit.commit_id,
            expected_generation + 1,
            false,
            true,
        )?;
        for update in &plan.ref_updates {
            self.upsert_ref(
                &transaction,
                &update.name,
                update.expected.as_ref(),
                &update.target,
                expected_generation + 1,
                false,
                false,
            )?;
        }
        let ref_map_digest = compute_ref_map_digest(&transaction)?;
        let event = self.append_audit(
            &transaction,
            &old,
            if merge_source.is_some() {
                "materialize_merge"
            } else {
                "promote"
            },
            expected_generation + 1,
            Some(&plan.snapshot.snapshot_id),
            Some(&plan.commit.commit_id),
            &ref_map_digest,
            Some(&plan.commit.build_manifest_digest),
        )?;
        injector.check(&FailurePoint::CatalogEvent)?;
        transaction.execute(
            "UPDATE store_state SET generation=?1, accepted_snapshot_id=?2, accepted_commit_id=?3, ref_map_digest=?4, build_manifest_digest=?5, audit_event_tail=?6 WHERE singleton=1 AND generation=?7",
            params![
                u64_to_i64(expected_generation + 1)?,
                plan.snapshot.snapshot_id.as_str(),
                plan.commit.commit_id.as_str(),
                ref_map_digest.as_str(),
                plan.commit.build_manifest_digest.as_str(),
                event.as_str(),
                u64_to_i64(expected_generation)?,
            ],
        )?;
        injector.check(&FailurePoint::CatalogState)?;
        validate_catalog_counts(&transaction)?;
        transaction.commit()?;
        injector.check(&FailurePoint::CatalogCommit)?;
        self.status()
    }

    /// Publish a fully authenticated candidate commit to a review/evidence branch
    /// without changing accepted main. The branch move, ref-map digest, audit
    /// event, and singleton generation still commit atomically.
    pub fn publish_candidate(
        &self,
        expected_generation: u64,
        branch: &str,
        expected_target: Option<&CommitIdV2>,
        plan: &PromotionPlan,
    ) -> StoreResult<StoreStatus> {
        validate_ref_name(branch, false)?;
        if branch == "heads/main" {
            return Err(invalid("candidate publication cannot move protected main"));
        }
        let repository_id = self.repository_id()?;
        plan.validate_static(&repository_id)?;
        if !plan.ref_updates.is_empty() {
            return Err(invalid(
                "candidate publication takes one explicit branch and rejects embedded ref updates",
            ));
        }
        self.validate_promotion_objects_exist_or_supplied(plan)?;
        let objects = self.plan_objects(plan)?;
        for (index, object) in objects.iter().enumerate() {
            self.publish_object(object, index, &NoFailure)?;
        }
        let mut connection = self.connect(false)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = read_state(&transaction)?;
        if old.generation != expected_generation {
            return Err(AxiStoreError::StaleState {
                expected: expected_generation,
                actual: old.generation,
            });
        }
        for object in &objects {
            self.insert_object_metadata(&transaction, object)?;
        }
        self.insert_tree(&transaction, &plan.tree)?;
        self.insert_snapshot(&transaction, &plan.snapshot)?;
        self.insert_manifest(&transaction, &plan.manifest)?;
        self.validate_plan_object_closure(&transaction, plan)?;
        self.validate_parent_alignment(&transaction, plan)?;
        if let Some(reconciliation) = &plan.reconciliation {
            self.validate_reconciliation_base(&transaction, reconciliation)?;
            self.validate_reconciliation_candidates(&transaction, plan, reconciliation)?;
            self.insert_reconciliation(&transaction, reconciliation)?;
        }
        self.insert_commit(&transaction, &plan.commit)?;
        self.upsert_ref(
            &transaction,
            branch,
            expected_target,
            &plan.commit.commit_id,
            expected_generation + 1,
            false,
            false,
        )?;
        let ref_map_digest = compute_ref_map_digest(&transaction)?;
        let event_id = self.append_audit(
            &transaction,
            &old,
            "publish_candidate",
            expected_generation + 1,
            old.accepted_snapshot_id.as_ref(),
            old.accepted_commit_id.as_ref(),
            &ref_map_digest,
            old.build_manifest_digest.as_ref(),
        )?;
        transaction.execute(
            "UPDATE store_state SET generation=?1, ref_map_digest=?2, audit_event_tail=?3 WHERE singleton=1 AND generation=?4",
            params![u64_to_i64(expected_generation + 1)?, ref_map_digest.as_str(), event_id.as_str(), u64_to_i64(expected_generation)?],
        )?;
        validate_catalog_counts(&transaction)?;
        transaction.commit()?;
        self.status()
    }

    fn validate_promotion_objects_exist_or_supplied(
        &self,
        plan: &PromotionPlan,
    ) -> StoreResult<()> {
        let supplied = plan
            .objects
            .iter()
            .map(|value| value.digest.to_string())
            .collect::<BTreeSet<_>>();
        let connection = self.connect(false)?;
        let required = [
            &plan.manifest.kernel_ir_digest,
            &plan.manifest.canonical_fact_log_digest,
            &plan.manifest.validation_report_digest,
            &plan.manifest.competency_question_report_digest,
            &plan.manifest.runtime_theory_report_digest,
            &plan.manifest.trusted_checker_receipt_digest,
        ];
        for digest in required {
            if !supplied.contains(digest.as_str()) && !object_exists(&connection, digest.as_str())?
            {
                return Err(invalid(format!(
                    "promotion closure is missing immutable object `{digest}`"
                )));
            }
        }
        for attachment in &plan.commit.attachments {
            if !supplied.contains(attachment.digest.as_str())
                && !object_exists(&connection, attachment.digest.as_str())?
            {
                return Err(invalid(format!(
                    "commit attachment `{}` is missing",
                    attachment.digest
                )));
            }
        }
        for gate in &plan.commit.gates {
            if !supplied.contains(gate.report_digest.as_str())
                && !object_exists(&connection, gate.report_digest.as_str())?
            {
                return Err(invalid(format!(
                    "promotion gate report `{}` is missing",
                    gate.report_digest
                )));
            }
        }
        if let Some(reconciliation) = &plan.reconciliation {
            let witness_digests = reconciliation.decisions.iter().filter_map(|decision| {
                if let TypedReconciliationDecisionV2::Transport { witness_digest, .. } = decision {
                    Some(witness_digest)
                } else {
                    None
                }
            });
            for digest in witness_digests.chain(std::iter::once(&reconciliation.preview_digest)) {
                if !supplied.contains(digest.as_str())
                    && !object_exists(&connection, digest.as_str())?
                {
                    return Err(invalid(format!(
                        "reconciliation closure is missing `{digest}`"
                    )));
                }
            }
        }
        Ok(())
    }

    fn plan_objects(&self, plan: &PromotionPlan) -> StoreResult<Vec<PlannedObject>> {
        let mut values = Vec::new();
        for module in &plan.modules {
            values.push(PlannedObject {
                id: module.module.revision_digest.to_string(),
                kind: ImmutableObjectKind::AxiModule,
                bytes: module.exact_bytes.clone(),
            });
        }
        for object in &plan.objects {
            values.push(PlannedObject {
                id: object.digest.to_string(),
                kind: object.kind,
                bytes: object.bytes.clone(),
            });
        }
        values.push(structured_object(
            plan.tree.tree_id.as_str(),
            ImmutableObjectKind::AcceptedTree,
            &plan.tree,
        )?);
        values.push(structured_object(
            plan.snapshot.snapshot_id.as_str(),
            ImmutableObjectKind::AcceptedSnapshot,
            &plan.snapshot,
        )?);
        values.push(structured_object(
            plan.manifest.digest()?.as_str(),
            ImmutableObjectKind::BuildManifest,
            &plan.manifest,
        )?);
        if let Some(reconciliation) = &plan.reconciliation {
            values.push(structured_object(
                reconciliation.reconciliation_id.as_str(),
                ImmutableObjectKind::Reconciliation,
                reconciliation,
            )?);
        }
        values.push(structured_object(
            plan.commit.commit_id.as_str(),
            ImmutableObjectKind::SemanticCommit,
            &plan.commit,
        )?);
        let mut by_id = BTreeMap::<String, PlannedObject>::new();
        for value in values {
            match by_id.get(&value.id) {
                Some(existing) if existing.kind == value.kind && existing.bytes == value.bytes => {}
                Some(_) => {
                    return Err(AxiStoreError::ObjectCollision {
                        object_id: value.id,
                    })
                }
                None => {
                    by_id.insert(value.id.clone(), value);
                }
            }
        }
        validate_count(
            "publication_objects",
            by_id.len(),
            AXI_STORE_MAX_PUBLICATION_OBJECTS,
        )?;
        let mut total_bytes = 0_u64;
        for object in by_id.values() {
            validate_byte_len(
                "immutable_object_bytes",
                object.bytes.len(),
                AXI_STORE_MAX_OBJECT_BYTES,
            )?;
            total_bytes = total_bytes
                .checked_add(object.bytes.len() as u64)
                .ok_or_else(|| {
                    limit_exceeded(
                        "publication_bytes",
                        AXI_STORE_MAX_PUBLICATION_BYTES,
                        u64::MAX,
                    )
                })?;
            if total_bytes > AXI_STORE_MAX_PUBLICATION_BYTES {
                return Err(limit_exceeded(
                    "publication_bytes",
                    AXI_STORE_MAX_PUBLICATION_BYTES,
                    total_bytes,
                ));
            }
        }
        Ok(by_id.into_values().collect())
    }

    fn validate_main_transition(
        &self,
        transaction: &Transaction<'_>,
        old: &StoreState,
        plan: &PromotionPlan,
    ) -> StoreResult<()> {
        match (&old.accepted_commit_id, &old.accepted_snapshot_id) {
            (None, None) => {
                if !plan.commit.ordered_parents.is_empty()
                    || !plan.snapshot.ordered_parents.is_empty()
                {
                    return Err(invalid(
                        "genesis promotion must have no commit or snapshot parents",
                    ));
                }
            }
            (Some(old_commit), Some(old_snapshot)) => {
                if plan.commit.ordered_parents.first() != Some(old_commit)
                    || plan.snapshot.ordered_parents.first() != Some(old_snapshot)
                {
                    return Err(invalid("protected main transition must parent the exact current commit and snapshot first"));
                }
                if plan.commit.kind == CommitKind::Normal
                    && (plan.commit.ordered_parents.len() != 1
                        || plan.snapshot.ordered_parents.len() != 1)
                {
                    return Err(invalid(
                        "normal protected-main transition has one exact parent",
                    ));
                }
                if plan.commit.kind == CommitKind::Merge {
                    if plan.snapshot.ordered_parents.len() != 2 {
                        return Err(invalid(
                            "merge snapshot requires ordered [target, source] parents",
                        ));
                    }
                    let source_commit = load_commit(transaction, &plan.commit.ordered_parents[1])?;
                    if source_commit.accepted_snapshot_id != plan.snapshot.ordered_parents[1] {
                        return Err(invalid("merge commit and snapshot source parents disagree"));
                    }
                }
            }
            _ => return Err(corrupt("accepted state has split snapshot/semantic heads")),
        }
        Ok(())
    }

    fn validate_parent_alignment(
        &self,
        transaction: &Transaction<'_>,
        plan: &PromotionPlan,
    ) -> StoreResult<()> {
        if plan.commit.ordered_parents.len() != plan.snapshot.ordered_parents.len() {
            return Err(invalid("commit and snapshot parent arity disagree"));
        }
        for (commit_parent, snapshot_parent) in plan
            .commit
            .ordered_parents
            .iter()
            .zip(&plan.snapshot.ordered_parents)
        {
            let parent = load_commit(transaction, commit_parent)?;
            if &parent.accepted_snapshot_id != snapshot_parent {
                return Err(invalid(
                    "commit and snapshot ordered parent anchors disagree",
                ));
            }
        }
        Ok(())
    }

    fn validate_reconciliation_base(
        &self,
        transaction: &Transaction<'_>,
        reconciliation: &SemReconciliationV2,
    ) -> StoreResult<()> {
        let candidates = maximal_common_ancestors(
            transaction,
            &reconciliation.left.commit_id,
            &reconciliation.right.commit_id,
        )?;
        if candidates.is_empty() {
            return Err(AxiStoreError::NoMergeBase);
        }
        if candidates.len() != 1 {
            return Err(AxiStoreError::AmbiguousMergeBase { candidates });
        }
        if candidates[0] != reconciliation.base_commit_id {
            return Err(invalid(
                "reconciliation base is not the unique maximal common ancestor",
            ));
        }
        Ok(())
    }

    fn validate_plan_object_closure(
        &self,
        transaction: &Transaction<'_>,
        plan: &PromotionPlan,
    ) -> StoreResult<()> {
        let catalog: String =
            transaction.query_row("PRAGMA database_list", [], |row| row.get(2))?;
        let root = Path::new(&catalog)
            .parent()
            .ok_or_else(|| corrupt("catalog path has no parent"))?;
        validate_manifest_objects(root, transaction, &plan.manifest)?;
        for attachment in &plan.commit.attachments {
            read_verified_object(
                root,
                transaction,
                attachment.digest.as_str(),
                attachment.kind,
            )?;
        }
        if let Some(reconciliation) = &plan.reconciliation {
            read_verified_object(
                root,
                transaction,
                reconciliation.preview_digest.as_str(),
                ImmutableObjectKind::Evidence,
            )?;
            for digest in reconciliation.decisions.iter().filter_map(|decision| {
                if let TypedReconciliationDecisionV2::Transport { witness_digest, .. } = decision {
                    Some(witness_digest)
                } else {
                    None
                }
            }) {
                read_verified_object(
                    root,
                    transaction,
                    digest.as_str(),
                    ImmutableObjectKind::Certificate,
                )?;
            }
        }
        Ok(())
    }

    fn validate_reconciliation_candidates(
        &self,
        transaction: &Transaction<'_>,
        plan: &PromotionPlan,
        reconciliation: &SemReconciliationV2,
    ) -> StoreResult<()> {
        let catalog: String =
            transaction.query_row("PRAGMA database_list", [], |row| row.get(2))?;
        let root = Path::new(&catalog)
            .parent()
            .ok_or_else(|| corrupt("catalog path has no parent"))?;
        for reviewed in [&reconciliation.left, &reconciliation.right] {
            let commit = load_commit(transaction, &reviewed.commit_id)?;
            if commit.accepted_snapshot_id != reviewed.candidate.accepted_snapshot_id
                || commit.accepted_tree_id != reviewed.candidate.accepted_tree_id
            {
                return Err(invalid(
                    "reviewed parent candidate anchors do not match its exact semantic commit",
                ));
            }
            let manifest =
                load_manifest_connection(root, transaction, &commit.build_manifest_digest)?;
            validate_commit_gate_anchors(&commit, &manifest)?;
            if reviewed.gates != manifest_gate_reports(&manifest)
                || reviewed.candidate.kernel_ir_digest != manifest.kernel_ir_digest
            {
                return Err(invalid(
                    "reviewed parent candidate gates or kernel IR do not match its build manifest",
                ));
            }
            validate_typed_candidate_compilation(root, transaction, &reviewed.candidate)?;
        }
        if reconciliation.merged.gates != manifest_gate_reports(&plan.manifest)
            || reconciliation.merged.candidate.accepted_snapshot_id != plan.snapshot.snapshot_id
            || reconciliation.merged.candidate.accepted_tree_id != plan.tree.tree_id
            || reconciliation.merged.candidate.kernel_ir_digest != plan.manifest.kernel_ir_digest
        {
            return Err(invalid(
                "reviewed merged candidate does not bind the materialized plan and exact gates",
            ));
        }
        validate_typed_candidate_compilation(root, transaction, &reconciliation.merged.candidate)
    }

    pub fn status(&self) -> StoreResult<StoreStatus> {
        let connection = self.connect(false)?;
        let state = read_state_connection(&connection)?;
        validate_repository(&self.root, &connection, &state.repository_id)?;
        validate_audit_chain(&connection, &state)?;
        let refs = read_refs(&connection)?;
        if compute_ref_map_digest_connection(&connection)? != state.ref_map_digest {
            return Err(corrupt("store_state ref-map digest mismatch"));
        }
        match (
            &state.accepted_commit_id,
            &state.accepted_snapshot_id,
            &state.build_manifest_digest,
        ) {
            (None, None, None) => {
                if refs.iter().any(|value| value.name == "heads/main") {
                    return Err(corrupt("uninitialized state has a protected main ref"));
                }
            }
            (Some(commit_id), Some(snapshot_id), Some(manifest_id)) => {
                let main = refs
                    .iter()
                    .find(|value| value.name == "heads/main")
                    .ok_or_else(|| corrupt("accepted state is missing protected main ref"))?;
                if &main.target != commit_id {
                    return Err(corrupt(
                        "protected main ref is split from accepted semantic head",
                    ));
                }
                validate_commit_closure(&self.root, &connection, commit_id)?;
                let commit = load_commit_connection(&self.root, &connection, commit_id)?;
                if &commit.accepted_snapshot_id != snapshot_id
                    || &commit.build_manifest_digest != manifest_id
                {
                    return Err(corrupt("store_state anchors disagree with accepted commit"));
                }
                validate_snapshot_closure(&self.root, &connection, snapshot_id)?;
                let manifest = load_manifest_connection(&self.root, &connection, manifest_id)?;
                if &manifest.accepted_snapshot_id != snapshot_id
                    || manifest.accepted_tree_id != commit.accepted_tree_id
                {
                    return Err(corrupt(
                        "accepted build manifest anchors disagree with state",
                    ));
                }
                validate_manifest_objects(&self.root, &connection, &manifest)?;
            }
            _ => {
                return Err(corrupt(
                    "store_state has a partial accepted authority tuple",
                ))
            }
        }
        for reference in &refs {
            validate_commit_closure(&self.root, &connection, &reference.target)?;
        }
        Ok(StoreStatus {
            state_digest: state.digest(),
            state,
            refs,
        })
    }

    pub fn branches(&self) -> StoreResult<Vec<RefStatus>> {
        Ok(self
            .status()?
            .refs
            .into_iter()
            .filter(|value| value.name.starts_with("heads/"))
            .collect())
    }

    pub fn tags(&self) -> StoreResult<Vec<RefStatus>> {
        Ok(self
            .status()?
            .refs
            .into_iter()
            .filter(|value| value.name.starts_with("tags/"))
            .collect())
    }

    pub fn update_branch(
        &self,
        expected_generation: u64,
        name: &str,
        expected_target: Option<&CommitIdV2>,
        target: &CommitIdV2,
    ) -> StoreResult<StoreStatus> {
        validate_ref_name(name, false)?;
        if name == "heads/main" || !name.starts_with("heads/") {
            return Err(invalid(
                "protected main moves only through promotion; branch name must start with heads/",
            ));
        }
        self.mutate_ref(expected_generation, name, expected_target, target, false)
    }

    pub fn create_tag(
        &self,
        expected_generation: u64,
        name: &str,
        target: &CommitIdV2,
    ) -> StoreResult<StoreStatus> {
        validate_ref_name(name, true)?;
        if !name.starts_with("tags/") {
            return Err(invalid("tag name must start with tags/"));
        }
        self.mutate_ref(expected_generation, name, None, target, true)
    }

    fn mutate_ref(
        &self,
        expected_generation: u64,
        name: &str,
        expected_target: Option<&CommitIdV2>,
        target: &CommitIdV2,
        immutable: bool,
    ) -> StoreResult<StoreStatus> {
        let mut connection = self.connect(false)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = read_state(&transaction)?;
        if old.generation != expected_generation {
            return Err(AxiStoreError::StaleState {
                expected: expected_generation,
                actual: old.generation,
            });
        }
        load_commit(&transaction, target)?;
        self.upsert_ref(
            &transaction,
            name,
            expected_target,
            target,
            expected_generation + 1,
            immutable,
            false,
        )?;
        let ref_map_digest = compute_ref_map_digest(&transaction)?;
        let event_id = self.append_audit(
            &transaction,
            &old,
            if immutable {
                "create_tag"
            } else {
                "update_branch"
            },
            expected_generation + 1,
            old.accepted_snapshot_id.as_ref(),
            old.accepted_commit_id.as_ref(),
            &ref_map_digest,
            old.build_manifest_digest.as_ref(),
        )?;
        transaction.execute(
            "UPDATE store_state SET generation=?1, ref_map_digest=?2, audit_event_tail=?3 WHERE singleton=1 AND generation=?4",
            params![u64_to_i64(expected_generation + 1)?, ref_map_digest.as_str(), event_id.as_str(), u64_to_i64(expected_generation)?],
        )?;
        validate_catalog_counts(&transaction)?;
        transaction.commit()?;
        self.status()
    }

    #[allow(clippy::too_many_arguments)]
    fn upsert_ref(
        &self,
        transaction: &Transaction<'_>,
        name: &str,
        expected: Option<&CommitIdV2>,
        target: &CommitIdV2,
        generation: u64,
        immutable: bool,
        protected_main: bool,
    ) -> StoreResult<()> {
        let actual = query_ref_target(transaction, name)?;
        if actual.as_ref() != expected {
            return Err(AxiStoreError::StaleRef {
                ref_name: name.to_string(),
                expected: expected.cloned(),
                actual,
            });
        }
        if transaction
            .query_row(
                "SELECT immutable FROM refs WHERE ref_name=?1",
                [name],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .is_some_and(|value| value != 0)
        {
            return Err(AxiStoreError::ImmutableTag(name.to_string()));
        }
        if let Some(old_target) = expected {
            if !is_ancestor(transaction, old_target, target)? {
                return Err(invalid(format!(
                    "ref `{name}` update is not a semantic fast-forward"
                )));
            }
        }
        transaction.execute(
            "INSERT INTO refs(ref_name, target_commit_id, generation, immutable, protected_main) VALUES(?1, ?2, ?3, ?4, ?5) ON CONFLICT(ref_name) DO UPDATE SET target_commit_id=excluded.target_commit_id, generation=excluded.generation WHERE refs.immutable=0",
            params![name, target.as_str(), u64_to_i64(generation)?, i64::from(immutable), i64::from(protected_main)],
        )?;
        Ok(())
    }

    pub fn lineage_proof(
        &self,
        subject: &CommitIdV2,
        ancestor: &CommitIdV2,
        pin: &LineagePin,
    ) -> StoreResult<LineageProof> {
        let status = self.status()?;
        match pin {
            LineagePin::AcceptedState(expected)
                if expected != &status.state_digest
                    || status.state.accepted_commit_id.as_ref() != Some(subject) =>
            {
                return Err(invalid(
                    "lineage state pin does not match the current accepted subject head",
                ));
            }
            LineagePin::SubjectCommit(expected) if expected != subject => {
                return Err(invalid(
                    "lineage subject pin does not match requested subject",
                ));
            }
            _ => {}
        }
        let connection = self.connect(false)?;
        let ids = shortest_parent_path(&connection, subject, ancestor)?
            .ok_or(AxiStoreError::NoMergeBase)?;
        let mut ordered_commits = Vec::with_capacity(ids.len());
        for id in ids {
            ordered_commits.push(load_commit_connection(&self.root, &connection, &id)?);
        }
        let proof = LineageProof {
            format: LINEAGE_PROOF_FORMAT.to_string(),
            version: FORMAT_VERSION,
            repository_id: status.state.repository_id,
            subject: subject.clone(),
            ancestor: ancestor.clone(),
            ordered_commits,
        };
        proof.validate()?;
        Ok(proof)
    }

    pub fn verify_lineage_proof(&self, proof: &LineageProof, pin: &LineagePin) -> StoreResult<()> {
        proof.validate()?;
        let status = self.status()?;
        if proof.repository_id != status.state.repository_id {
            return Err(invalid("lineage proof repository does not match store"));
        }
        match pin {
            LineagePin::AcceptedState(expected)
                if expected == &status.state_digest
                    && status.state.accepted_commit_id.as_ref() == Some(&proof.subject) => {}
            LineagePin::SubjectCommit(expected) if expected == &proof.subject => {}
            _ => {
                return Err(invalid(
                    "lineage proof lacks the required exact state or subject-head pin",
                ))
            }
        }
        Ok(())
    }

    pub fn maximal_common_ancestors(
        &self,
        left: &CommitIdV2,
        right: &CommitIdV2,
    ) -> StoreResult<Vec<CommitIdV2>> {
        let connection = self.connect(false)?;
        maximal_common_ancestors_connection(&connection, left, right)
    }

    pub fn merge_base(&self, left: &CommitIdV2, right: &CommitIdV2) -> StoreResult<CommitIdV2> {
        let candidates = self.maximal_common_ancestors(left, right)?;
        match candidates.as_slice() {
            [] => Err(AxiStoreError::NoMergeBase),
            [only] => Ok(only.clone()),
            _ => Err(AxiStoreError::AmbiguousMergeBase { candidates }),
        }
    }

    fn repository_id(&self) -> StoreResult<RepositoryIdV2> {
        let connection = self.connect(false)?;
        let value: String = connection.query_row(
            "SELECT repository_id FROM repository WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        parse_id(&value, "repository id")
    }

    fn publish_object(
        &self,
        object: &PlannedObject,
        index: usize,
        injector: &dyn FailureInjector,
    ) -> StoreResult<()> {
        validate_byte_len(
            "immutable_object_bytes",
            object.bytes.len(),
            AXI_STORE_MAX_OBJECT_BYTES,
        )?;
        let path = object_path(&self.root, &object.id)?;
        if fs::symlink_metadata(&path).is_ok() {
            if read_regular_file_bounded(
                &path,
                "immutable_object_bytes",
                AXI_STORE_MAX_OBJECT_BYTES,
            )? == object.bytes
            {
                return Ok(());
            }
            return Err(AxiStoreError::ObjectCollision {
                object_id: object.id.clone(),
            });
        }
        let directory = path
            .parent()
            .ok_or_else(|| invalid("object path has no parent"))?;
        fs::create_dir_all(directory)?;
        let directory_metadata = fs::symlink_metadata(directory)?;
        if !directory_metadata.file_type().is_dir() || directory_metadata.file_type().is_symlink() {
            return Err(corrupt("immutable object directory must not be a symlink"));
        }
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = directory.join(format!(".tmp.{}.{}", std::process::id(), sequence));
        let result = (|| -> StoreResult<()> {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(&object.bytes)?;
            injector.check(&FailurePoint::ObjectWrite(index))?;
            file.sync_all()?;
            injector.check(&FailurePoint::ObjectFsync(index))?;
            drop(file);
            if fs::symlink_metadata(&path).is_ok() {
                if read_regular_file_bounded(
                    &path,
                    "immutable_object_bytes",
                    AXI_STORE_MAX_OBJECT_BYTES,
                )? != object.bytes
                {
                    return Err(AxiStoreError::ObjectCollision {
                        object_id: object.id.clone(),
                    });
                }
                fs::remove_file(&temporary)?;
                return Ok(());
            }
            fs::rename(&temporary, &path)?;
            File::open(directory)?.sync_all()?;
            injector.check(&FailurePoint::ObjectPublish(index))?;
            Ok(())
        })();
        if temporary.exists() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn insert_object_metadata(
        &self,
        transaction: &Transaction<'_>,
        object: &PlannedObject,
    ) -> StoreResult<()> {
        let raw_sha256 = raw_sha256(&object.bytes);
        let path = object_path(&self.root, &object.id)?;
        let relative = path
            .strip_prefix(&self.root)
            .map_err(|_| invalid("object path escapes store root"))?
            .to_string_lossy()
            .to_string();
        let existing: Option<(String, String, i64, String)> = transaction
            .query_row(
                "SELECT kind, raw_sha256, byte_len, relative_path FROM objects WHERE object_id=?1",
                [&object.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let expected = (
            object.kind.wire().to_string(),
            raw_sha256,
            i64::try_from(object.bytes.len()).map_err(|_| invalid("object is too large"))?,
            relative,
        );
        match existing {
            Some(actual) if actual == expected => Ok(()),
            Some(_) => Err(AxiStoreError::ObjectCollision {
                object_id: object.id.clone(),
            }),
            None => {
                transaction.execute(
                    "INSERT INTO objects(object_id, kind, raw_sha256, byte_len, relative_path) VALUES(?1, ?2, ?3, ?4, ?5)",
                    params![object.id, expected.0, expected.1, expected.2, expected.3],
                )?;
                Ok(())
            }
        }
    }

    fn insert_tree(&self, transaction: &Transaction<'_>, tree: &AcceptedTree) -> StoreResult<()> {
        transaction.execute(
            "INSERT OR IGNORE INTO accepted_trees(tree_id, repository_id, object_id) VALUES(?1, ?2, ?3)",
            params![tree.tree_id.as_str(), tree.repository_id.as_str(), tree.tree_id.as_str()],
        )?;
        for (position, module) in tree.modules.iter().enumerate() {
            transaction.execute(
                "INSERT OR IGNORE INTO accepted_tree_modules(tree_id, position, module_name, module_id, revision_digest) VALUES(?1, ?2, ?3, ?4, ?5)",
                params![tree.tree_id.as_str(), usize_to_i64(position)?, module.module_name, module.module_id.as_str(), module.revision_digest.as_str()],
            )?;
        }
        Ok(())
    }

    fn insert_snapshot(
        &self,
        transaction: &Transaction<'_>,
        snapshot: &AcceptedSnapshot,
    ) -> StoreResult<()> {
        transaction.execute(
            "INSERT OR IGNORE INTO accepted_snapshots(snapshot_id, repository_id, tree_id, object_id) VALUES(?1, ?2, ?3, ?4)",
            params![snapshot.snapshot_id.as_str(), snapshot.repository_id.as_str(), snapshot.tree_id.as_str(), snapshot.snapshot_id.as_str()],
        )?;
        for (position, parent) in snapshot.ordered_parents.iter().enumerate() {
            transaction.execute(
                "INSERT OR IGNORE INTO accepted_snapshot_parents(snapshot_id, position, parent_snapshot_id) VALUES(?1, ?2, ?3)",
                params![snapshot.snapshot_id.as_str(), usize_to_i64(position)?, parent.as_str()],
            )?;
        }
        Ok(())
    }

    fn insert_manifest(
        &self,
        transaction: &Transaction<'_>,
        manifest: &AcceptedBuildManifest,
    ) -> StoreResult<()> {
        let digest = manifest.digest()?;
        transaction.execute(
            "INSERT OR IGNORE INTO build_manifests(manifest_digest, repository_id, snapshot_id, tree_id, object_id) VALUES(?1, ?2, ?3, ?4, ?5)",
            params![digest.as_str(), manifest.repository_id.as_str(), manifest.accepted_snapshot_id.as_str(), manifest.accepted_tree_id.as_str(), digest.as_str()],
        )?;
        Ok(())
    }

    fn insert_reconciliation(
        &self,
        transaction: &Transaction<'_>,
        value: &SemReconciliationV2,
    ) -> StoreResult<()> {
        transaction.execute(
            "INSERT OR IGNORE INTO reconciliations(reconciliation_id, repository_id, base_commit_id, left_commit_id, right_commit_id, outcome, object_id) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![value.reconciliation_id.as_str(), value.repository_id.as_str(), value.base_commit_id.as_str(), value.left.commit_id.as_str(), value.right.commit_id.as_str(), format!("{:?}", value.outcome).to_lowercase(), value.reconciliation_id.as_str()],
        )?;
        Ok(())
    }

    fn insert_commit(
        &self,
        transaction: &Transaction<'_>,
        commit: &SemCommitV2,
    ) -> StoreResult<()> {
        transaction.execute(
            "INSERT OR IGNORE INTO semantic_commits(commit_id, repository_id, kind, snapshot_id, tree_id, manifest_digest, reconciliation_id, object_id) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![commit.commit_id.as_str(), commit.repository_id.as_str(), format!("{:?}", commit.kind).to_lowercase(), commit.accepted_snapshot_id.as_str(), commit.accepted_tree_id.as_str(), commit.build_manifest_digest.as_str(), commit.reconciliation_id.as_ref().map(|value| value.as_str()), commit.commit_id.as_str()],
        )?;
        for (position, parent) in commit.ordered_parents.iter().enumerate() {
            transaction.execute(
                "INSERT OR IGNORE INTO semantic_commit_parents(commit_id, position, parent_commit_id) VALUES(?1, ?2, ?3)",
                params![commit.commit_id.as_str(), usize_to_i64(position)?, parent.as_str()],
            )?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn append_audit(
        &self,
        transaction: &Transaction<'_>,
        old: &StoreState,
        action: &str,
        generation: u64,
        snapshot: Option<&SnapshotIdV2>,
        commit: Option<&CommitIdV2>,
        ref_map_digest: &ObjectBlobIdV2,
        manifest: Option<&ObjectBlobIdV2>,
    ) -> StoreResult<ObjectBlobIdV2> {
        let sequence: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(sequence), -1) + 1 FROM audit_events",
            [],
            |row| row.get(0),
        )?;
        let sequence = u64::try_from(sequence).map_err(|_| corrupt("negative audit sequence"))?;
        let event_id = audit_digest(
            &old.repository_id,
            sequence,
            Some(&old.audit_event_tail),
            action,
            generation,
            snapshot,
            commit,
            ref_map_digest,
            manifest,
        );
        transaction.execute(
            "INSERT INTO audit_events(sequence, event_id, previous_event_id, action, generation, accepted_snapshot_id, accepted_commit_id, ref_map_digest, build_manifest_digest) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![u64_to_i64(sequence)?, event_id.as_str(), old.audit_event_tail.as_str(), action, u64_to_i64(generation)?, snapshot.map(|value| value.as_str()), commit.map(|value| value.as_str()), ref_map_digest.as_str(), manifest.map(|value| value.as_str())],
        )?;
        Ok(event_id)
    }
}

fn structured_object<T: Serialize>(
    id: &str,
    kind: ImmutableObjectKind,
    value: &T,
) -> StoreResult<PlannedObject> {
    Ok(PlannedObject {
        id: id.to_string(),
        kind,
        bytes: serde_json::to_vec(value)?,
    })
}

const EXPECTED_CATALOG_TABLES: [&str; 13] = [
    "accepted_snapshot_parents",
    "accepted_snapshots",
    "accepted_tree_modules",
    "accepted_trees",
    "audit_events",
    "build_manifests",
    "objects",
    "reconciliations",
    "refs",
    "repository",
    "semantic_commit_parents",
    "semantic_commits",
    "store_state",
];

fn configure_catalog_connection_limits(connection: &Connection) -> StoreResult<()> {
    connection.set_limit(Limit::SQLITE_LIMIT_LENGTH, 1024 * 1024)?;
    connection.set_limit(Limit::SQLITE_LIMIT_SQL_LENGTH, 128 * 1024)?;
    connection.set_limit(Limit::SQLITE_LIMIT_COLUMN, 64)?;
    connection.set_limit(Limit::SQLITE_LIMIT_EXPR_DEPTH, 32)?;
    connection.set_limit(Limit::SQLITE_LIMIT_COMPOUND_SELECT, 8)?;
    connection.set_limit(Limit::SQLITE_LIMIT_VARIABLE_NUMBER, 64)?;
    connection.set_limit(Limit::SQLITE_LIMIT_TRIGGER_DEPTH, 0)?;
    connection.set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0)?;
    connection.set_limit(Limit::SQLITE_LIMIT_WORKER_THREADS, 0)?;
    Ok(())
}

fn validate_catalog_file(path: &Path) -> StoreResult<()> {
    validate_regular_file_limit(path, "catalog_bytes", AXI_STORE_MAX_CATALOG_BYTES)?;
    for suffix in ["-wal", "-shm"] {
        let mut sidecar_name = path.as_os_str().to_os_string();
        sidecar_name.push(suffix);
        let sidecar = PathBuf::from(sidecar_name);
        match fs::symlink_metadata(&sidecar) {
            Ok(_) => {
                let limit = if suffix == "-wal" {
                    AXI_STORE_MAX_CATALOG_BYTES
                } else {
                    64 * 1024 * 1024
                };
                validate_regular_file_limit(&sidecar, "catalog_sidecar_bytes", limit)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let (mut catalog, _) = axiograph_security::open_regular_file_bounded(
        path,
        usize::try_from(AXI_STORE_MAX_CATALOG_BYTES)
            .map_err(|_| corrupt("catalog byte limit exceeds usize"))?,
        "AxiStore SQLite catalog",
    )
    .map_err(|error| corrupt(error.to_string()))?;
    let mut prefix = [0_u8; 16];
    catalog
        .read_exact(&mut prefix)
        .map_err(|_| corrupt("truncated AxiStore SQLite catalog header"))?;
    if &prefix != b"SQLite format 3\0" {
        return Err(corrupt("AxiStore catalog is not SQLite"));
    }
    Ok(())
}

fn validate_regular_file_limit(path: &Path, name: &'static str, limit: u64) -> StoreResult<u64> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(corrupt(format!(
            "`{}` must be a regular file, not a symlink",
            path.display()
        )));
    }
    let actual = metadata.len();
    if actual > limit {
        return Err(limit_exceeded(name, limit, actual));
    }
    Ok(actual)
}

fn read_regular_file_bounded(path: &Path, name: &'static str, limit: u64) -> StoreResult<Vec<u8>> {
    // Preserve the public typed limit error for a stable oversized path. The
    // shared reader still repeats type and size validation on the opened
    // no-follow handle, so a replacement race cannot bypass the limit.
    validate_regular_file_limit(path, name, limit)?;
    let limit = usize::try_from(limit).map_err(|_| corrupt("file byte limit exceeds usize"))?;
    axiograph_security::read_file_bounded(path, limit, name)
        .map_err(|error| corrupt(error.to_string()))
}

fn parse_store_json<T: DeserializeOwned>(bytes: &[u8], label: &str) -> StoreResult<T> {
    axiograph_security::validate_json_nesting(
        bytes,
        axiograph_security::MAX_JSON_NESTING_DEPTH,
        label,
    )
    .map_err(|error| corrupt(error.to_string()))?;
    serde_json::from_slice(bytes).map_err(AxiStoreError::from)
}

fn validate_catalog_schema(connection: &Connection) -> StoreResult<()> {
    let application_id: i64 =
        connection.query_row("PRAGMA application_id", [], |row| row.get(0))?;
    let user_version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let page_size: i64 = connection.query_row("PRAGMA page_size", [], |row| row.get(0))?;
    let page_count: i64 = connection.query_row("PRAGMA page_count", [], |row| row.get(0))?;
    if application_id != i64::from(AXI_STORE_APPLICATION_ID)
        || user_version != i64::from(AXI_STORE_SCHEMA_VERSION)
        || page_size != i64::from(AXI_STORE_PAGE_SIZE)
    {
        return Err(corrupt("unsupported AxiStore catalog identity or schema"));
    }
    let bounded_page_count =
        u64::try_from(page_count).map_err(|_| corrupt("negative AxiStore catalog page count"))?;
    let catalog_bytes = bounded_page_count
        .checked_mul(u64::from(AXI_STORE_PAGE_SIZE))
        .ok_or_else(|| limit_exceeded("catalog_pages", AXI_STORE_MAX_CATALOG_BYTES, u64::MAX))?;
    if catalog_bytes > AXI_STORE_MAX_CATALOG_BYTES {
        return Err(limit_exceeded(
            "catalog_pages",
            AXI_STORE_MAX_CATALOG_BYTES,
            catalog_bytes,
        ));
    }

    let mut statement = connection.prepare(
        "SELECT name, strict FROM pragma_table_list WHERE schema='main' AND type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut actual_tables = Vec::new();
    for row in rows {
        let (name, strict) = row?;
        if strict != 1 {
            return Err(corrupt(format!("AxiStore table `{name}` is not STRICT")));
        }
        actual_tables.push(name);
    }
    if actual_tables != EXPECTED_CATALOG_TABLES {
        return Err(corrupt("AxiStore catalog table set mismatch"));
    }

    let quick_check: String =
        connection.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
    if quick_check != "ok" {
        return Err(corrupt(format!(
            "AxiStore catalog quick_check failed: {quick_check}"
        )));
    }
    let mut foreign_key_check = connection.prepare("PRAGMA foreign_key_check")?;
    if foreign_key_check.query([])?.next()?.is_some() {
        return Err(corrupt("AxiStore catalog foreign-key check failed"));
    }
    validate_catalog_counts(connection)
}

fn validate_catalog_counts(connection: &Connection) -> StoreResult<()> {
    for (name, table, limit) in [
        ("catalog_objects", "objects", MAX_REACHABLE_OBJECTS),
        ("catalog_trees", "accepted_trees", MAX_REACHABLE_OBJECTS),
        (
            "catalog_tree_modules",
            "accepted_tree_modules",
            MAX_REACHABLE_OBJECTS,
        ),
        (
            "catalog_snapshots",
            "accepted_snapshots",
            MAX_REACHABLE_OBJECTS,
        ),
        ("catalog_commits", "semantic_commits", MAX_REACHABLE_OBJECTS),
        ("catalog_refs", "refs", AXI_STORE_MAX_REFS),
        (
            "catalog_audit_events",
            "audit_events",
            MAX_REACHABLE_OBJECTS + 1,
        ),
    ] {
        let sql = format!("SELECT count(*) FROM {table}");
        let actual: i64 = connection.query_row(&sql, [], |row| row.get(0))?;
        let actual = u64::try_from(actual).map_err(|_| corrupt("negative catalog row count"))?;
        if actual > limit as u64 {
            return Err(limit_exceeded(name, limit as u64, actual));
        }
    }
    Ok(())
}

fn validate_ref_name(name: &str, tag: bool) -> StoreResult<()> {
    let allowed_prefix = if tag { "tags/" } else { "heads/" };
    if !name.starts_with(allowed_prefix)
        || name.len() <= allowed_prefix.len()
        || name
            .split('/')
            .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.'))
    {
        return Err(invalid(format!("invalid semantic ref name `{name}`")));
    }
    Ok(())
}

fn object_path(root: &Path, object_id: &str) -> StoreResult<PathBuf> {
    let hex = object_id
        .rsplit(':')
        .next()
        .ok_or_else(|| invalid("object id has no SHA-256 component"))?;
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid("object id does not end in full lowercase SHA-256"));
    }
    Ok(root.join(OBJECTS_DIR).join(hex))
}

fn raw_sha256(bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(bytes);
    format!("{:x}", hash.finalize())
}

fn u64_to_i64(value: u64) -> StoreResult<i64> {
    i64::try_from(value).map_err(|_| invalid("u64 value exceeds SQLite INTEGER range"))
}

fn usize_to_i64(value: usize) -> StoreResult<i64> {
    i64::try_from(value).map_err(|_| invalid("usize value exceeds SQLite INTEGER range"))
}

fn parse_id<T>(value: &str, label: &str) -> StoreResult<T>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    value
        .parse()
        .map_err(|error| corrupt(format!("malformed {label} `{value}`: {error}")))
}

fn object_exists(connection: &Connection, object_id: &str) -> StoreResult<bool> {
    Ok(connection
        .query_row(
            "SELECT 1 FROM objects WHERE object_id=?1",
            [object_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn read_state(transaction: &Transaction<'_>) -> StoreResult<StoreState> {
    read_state_query(transaction)
}

fn read_state_connection(connection: &Connection) -> StoreResult<StoreState> {
    read_state_query(connection)
}

fn read_state_query(connection: &Connection) -> StoreResult<StoreState> {
    let row = connection.query_row(
        "SELECT repository_id, generation, accepted_snapshot_id, accepted_commit_id, ref_map_digest, build_manifest_digest, audit_event_tail FROM store_state WHERE singleton=1",
        [],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, Option<String>>(2)?, row.get::<_, Option<String>>(3)?, row.get::<_, String>(4)?, row.get::<_, Option<String>>(5)?, row.get::<_, String>(6)?)),
    )?;
    Ok(StoreState {
        format: STORE_STATE_FORMAT.to_string(),
        version: FORMAT_VERSION,
        repository_id: parse_id(&row.0, "store repository id")?,
        generation: u64::try_from(row.1).map_err(|_| corrupt("negative state generation"))?,
        accepted_snapshot_id: row
            .2
            .as_deref()
            .map(|value| parse_id(value, "state snapshot id"))
            .transpose()?,
        accepted_commit_id: row
            .3
            .as_deref()
            .map(|value| parse_id(value, "state commit id"))
            .transpose()?,
        ref_map_digest: parse_id(&row.4, "state ref-map digest")?,
        build_manifest_digest: row
            .5
            .as_deref()
            .map(|value| parse_id(value, "state build manifest digest"))
            .transpose()?,
        audit_event_tail: parse_id(&row.6, "state audit tail")?,
    })
}

fn read_refs(connection: &Connection) -> StoreResult<Vec<RefStatus>> {
    let mut statement = connection.prepare(
        "SELECT ref_name, target_commit_id, generation, immutable FROM refs ORDER BY ref_name",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
        ))
    })?;
    let mut values = Vec::new();
    for row in rows {
        let row = row?;
        values.push(RefStatus {
            name: row.0,
            target: parse_id(&row.1, "ref target")?,
            generation: u64::try_from(row.2).map_err(|_| corrupt("negative ref generation"))?,
            immutable: row.3 != 0,
        });
    }
    Ok(values)
}

fn query_ref_target(connection: &Connection, name: &str) -> StoreResult<Option<CommitIdV2>> {
    connection
        .query_row(
            "SELECT target_commit_id FROM refs WHERE ref_name=?1",
            [name],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .as_deref()
        .map(|value| parse_id(value, "ref target"))
        .transpose()
}

fn compute_ref_map_digest(transaction: &Transaction<'_>) -> StoreResult<ObjectBlobIdV2> {
    compute_ref_map_digest_connection(transaction)
}

fn compute_ref_map_digest_connection(connection: &Connection) -> StoreResult<ObjectBlobIdV2> {
    let refs = read_refs(connection)?;
    let count = u32::try_from(refs.len())
        .map_err(|_| corrupt("too many semantic refs"))?
        .to_be_bytes();
    let version = FORMAT_VERSION.to_be_bytes();
    let mut fields = vec![
        b"axiograph_ref_map".as_slice(),
        version.as_slice(),
        count.as_slice(),
    ];
    for reference in &refs {
        fields.push(reference.name.as_bytes());
        fields.push(reference.target.as_str().as_bytes());
        fields.push(bool_bytes(reference.immutable));
    }
    Ok(ObjectBlobIdV2::from_canonical_fields(&fields))
}

fn bool_bytes(value: bool) -> &'static [u8] {
    if value {
        b"true"
    } else {
        b"false"
    }
}

fn validate_repository(
    root: &Path,
    connection: &Connection,
    repository_id: &RepositoryIdV2,
) -> StoreResult<()> {
    let row: (String, String) = connection.query_row(
        "SELECT repository_id, descriptor_object_id FROM repository WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if row.0 != repository_id.as_str() || row.1 != repository_id.as_str() {
        return Err(corrupt("repository descriptor anchors disagree"));
    }
    let bytes = read_verified_object(
        root,
        connection,
        repository_id.as_str(),
        ImmutableObjectKind::RepositoryDescriptor,
    )?;
    let descriptor: RepositoryDescriptor = parse_store_json(&bytes, "AxiStore JSON object")?;
    descriptor.validate()?;
    if descriptor.repository_id()? != *repository_id {
        return Err(corrupt("repository descriptor identity mismatch"));
    }
    Ok(())
}

fn read_verified_object(
    root: &Path,
    connection: &Connection,
    object_id: &str,
    expected_kind: ImmutableObjectKind,
) -> StoreResult<Vec<u8>> {
    let row: (String, String, i64, String) = connection
        .query_row(
            "SELECT kind, raw_sha256, byte_len, relative_path FROM objects WHERE object_id=?1",
            [object_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?
        .ok_or_else(|| corrupt(format!("missing object metadata for `{object_id}`")))?;
    if row.0 != expected_kind.wire() {
        return Err(corrupt(format!("object `{object_id}` kind mismatch")));
    }
    let expected_path = object_path(root, object_id)?;
    let actual_path = root.join(&row.3);
    if actual_path != expected_path || !actual_path.starts_with(root) {
        return Err(corrupt(format!(
            "object `{object_id}` path is not canonical"
        )));
    }
    let catalog_byte_len = u64::try_from(row.2)
        .map_err(|_| corrupt(format!("object `{object_id}` has a negative byte length")))?;
    if catalog_byte_len > AXI_STORE_MAX_OBJECT_BYTES {
        return Err(limit_exceeded(
            "immutable_object_bytes",
            AXI_STORE_MAX_OBJECT_BYTES,
            catalog_byte_len,
        ));
    }
    let bytes = read_regular_file_bounded(
        &actual_path,
        "immutable_object_bytes",
        AXI_STORE_MAX_OBJECT_BYTES,
    )?;
    if raw_sha256(&bytes) != row.1 || bytes.len() as u64 != catalog_byte_len {
        return Err(corrupt(format!(
            "object `{object_id}` exact bytes were tampered"
        )));
    }
    Ok(bytes)
}

fn validate_typed_candidate_compilation(
    root: &Path,
    connection: &Connection,
    candidate: &TypedCandidatePayloadV2,
) -> StoreResult<()> {
    let snapshot = load_snapshot_connection(root, connection, &candidate.accepted_snapshot_id)?;
    if snapshot.tree_id != candidate.accepted_tree_id {
        return Err(invalid(
            "typed candidate snapshot does not bind its declared accepted tree",
        ));
    }
    let tree = load_tree_connection(root, connection, &candidate.accepted_tree_id)?;
    let root_module = tree
        .modules
        .iter()
        .find(|module| module.module_id == candidate.root_module_id)
        .ok_or_else(|| invalid("typed candidate root module is absent from accepted tree"))?;
    let mut sources = Vec::with_capacity(tree.modules.len());
    for module in &tree.modules {
        let exact = read_verified_object(
            root,
            connection,
            module.revision_digest.as_str(),
            ImmutableObjectKind::AxiModule,
        )?;
        sources.push(CanonicalModuleSource::parse(exact).map_err(|error| {
            invalid(format!("candidate canonical compilation failed: {error}"))
        })?);
    }
    let compiled = CanonicalCompiler::compile(KernelCompilationRequest {
        repository_id: tree.repository_id.clone(),
        accepted_snapshot_id: candidate.accepted_snapshot_id.clone(),
        root_module: root_module.module_name.clone(),
        modules: sources,
    })
    .map_err(|error| invalid(format!("candidate canonical compilation failed: {error}")))?;
    compiled
        .require_finite_theory_gate(axiograph_kernel::FiniteTheoryGateConsumerIr::Merge)
        .map_err(|error| {
            invalid(format!(
                "candidate finite-theory merge gate failed: {error}"
            ))
        })?;
    let ir_bytes = serde_json::to_vec(compiled.ir())?;
    if blob_id(ImmutableObjectKind::KernelIr, &ir_bytes) != candidate.kernel_ir_digest {
        return Err(invalid(
            "typed candidate kernel IR digest does not match canonical recompilation",
        ));
    }
    let stored_ir = read_verified_object(
        root,
        connection,
        candidate.kernel_ir_digest.as_str(),
        ImmutableObjectKind::KernelIr,
    )?;
    if stored_ir != ir_bytes {
        return Err(invalid(
            "typed candidate kernel IR bytes differ from canonical recompilation",
        ));
    }
    let payloads = compiled
        .payload_fingerprints()
        .map_err(|error| invalid(format!("candidate payload indexing failed: {error}")))?;
    if payloads != candidate.payloads {
        return Err(invalid(
            "typed candidate payload fingerprints differ from canonical compiled IR",
        ));
    }
    Ok(())
}

fn load_commit(transaction: &Transaction<'_>, commit_id: &CommitIdV2) -> StoreResult<SemCommitV2> {
    let object_id: String = transaction
        .query_row(
            "SELECT object_id FROM semantic_commits WHERE commit_id=?1",
            [commit_id.as_str()],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| corrupt(format!("missing semantic commit `{commit_id}`")))?;
    if object_id != commit_id.as_str() {
        return Err(corrupt("semantic commit row/object id mismatch"));
    }
    let catalog: String = transaction.query_row("PRAGMA database_list", [], |row| row.get(2))?;
    let root = Path::new(&catalog)
        .parent()
        .ok_or_else(|| corrupt("catalog path has no parent"))?;
    let bytes = read_verified_object(
        root,
        transaction,
        commit_id.as_str(),
        ImmutableObjectKind::SemanticCommit,
    )?;
    let commit: SemCommitV2 = parse_store_json(&bytes, "AxiStore JSON object")?;
    commit.validate()?;
    if commit.commit_id != *commit_id {
        return Err(corrupt("semantic commit object identity mismatch"));
    }
    Ok(commit)
}

fn load_commit_connection(
    root: &Path,
    connection: &Connection,
    commit_id: &CommitIdV2,
) -> StoreResult<SemCommitV2> {
    let row: (String, String, String, String, Option<String>, String) = connection.query_row(
        "SELECT repository_id, snapshot_id, tree_id, manifest_digest, reconciliation_id, object_id FROM semantic_commits WHERE commit_id=?1",
        [commit_id.as_str()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
    ).optional()?.ok_or_else(|| corrupt(format!("missing semantic commit `{commit_id}`")))?;
    let bytes = read_verified_object(
        root,
        connection,
        commit_id.as_str(),
        ImmutableObjectKind::SemanticCommit,
    )?;
    let value: SemCommitV2 = parse_store_json(&bytes, "AxiStore JSON object")?;
    value.validate()?;
    if value.commit_id != *commit_id
        || value.repository_id.as_str() != row.0
        || value.accepted_snapshot_id.as_str() != row.1
        || value.accepted_tree_id.as_str() != row.2
        || value.build_manifest_digest.as_str() != row.3
        || value.reconciliation_id.as_ref().map(|id| id.as_str()) != row.4.as_deref()
        || row.5 != commit_id.as_str()
    {
        return Err(corrupt(format!(
            "semantic commit `{commit_id}` catalog fields were tampered"
        )));
    }
    let parents = query_parent_ids(
        connection,
        "semantic_commit_parents",
        "commit_id",
        "parent_commit_id",
        commit_id.as_str(),
    )?;
    if parents
        != value
            .ordered_parents
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    {
        return Err(corrupt(format!(
            "semantic commit `{commit_id}` parent rows disagree"
        )));
    }
    Ok(value)
}

fn load_snapshot_connection(
    root: &Path,
    connection: &Connection,
    snapshot_id: &SnapshotIdV2,
) -> StoreResult<AcceptedSnapshot> {
    let row: (String, String, String) = connection
        .query_row(
            "SELECT repository_id, tree_id, object_id FROM accepted_snapshots WHERE snapshot_id=?1",
            [snapshot_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| corrupt(format!("missing accepted snapshot `{snapshot_id}`")))?;
    let bytes = read_verified_object(
        root,
        connection,
        snapshot_id.as_str(),
        ImmutableObjectKind::AcceptedSnapshot,
    )?;
    let value: AcceptedSnapshot = parse_store_json(&bytes, "AxiStore JSON object")?;
    value.validate()?;
    if value.snapshot_id != *snapshot_id
        || value.repository_id.as_str() != row.0
        || value.tree_id.as_str() != row.1
        || row.2 != snapshot_id.as_str()
    {
        return Err(corrupt(format!(
            "accepted snapshot `{snapshot_id}` catalog fields were tampered"
        )));
    }
    let parents = query_parent_ids(
        connection,
        "accepted_snapshot_parents",
        "snapshot_id",
        "parent_snapshot_id",
        snapshot_id.as_str(),
    )?;
    if parents
        != value
            .ordered_parents
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    {
        return Err(corrupt(format!(
            "accepted snapshot `{snapshot_id}` parent rows disagree"
        )));
    }
    load_tree_connection(root, connection, &value.tree_id)?;
    Ok(value)
}

fn load_tree_connection(
    root: &Path,
    connection: &Connection,
    tree_id: &TreeIdV2,
) -> StoreResult<AcceptedTree> {
    let row: (String, String) = connection
        .query_row(
            "SELECT repository_id, object_id FROM accepted_trees WHERE tree_id=?1",
            [tree_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| corrupt(format!("missing accepted tree `{tree_id}`")))?;
    let bytes = read_verified_object(
        root,
        connection,
        tree_id.as_str(),
        ImmutableObjectKind::AcceptedTree,
    )?;
    let value: AcceptedTree = parse_store_json(&bytes, "AxiStore JSON object")?;
    value.validate()?;
    if value.tree_id != *tree_id
        || value.repository_id.as_str() != row.0
        || row.1 != tree_id.as_str()
    {
        return Err(corrupt(format!(
            "accepted tree `{tree_id}` catalog fields were tampered"
        )));
    }
    let mut statement = connection.prepare("SELECT position, module_name, module_id, revision_digest FROM accepted_tree_modules WHERE tree_id=?1 ORDER BY position")?;
    let rows = statement.query_map([tree_id.as_str()], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    let mut modules = Vec::new();
    for (expected_position, row) in rows.enumerate() {
        let row = row?;
        if row.0 != usize_to_i64(expected_position)? {
            return Err(corrupt("accepted tree module positions are not contiguous"));
        }
        modules.push(AcceptedModule {
            module_name: row.1,
            module_id: parse_id(&row.2, "tree module id")?,
            revision_digest: parse_id(&row.3, "tree revision digest")?,
        });
    }
    if modules != value.modules {
        return Err(corrupt(format!(
            "accepted tree `{tree_id}` module rows disagree"
        )));
    }
    for module in &value.modules {
        let exact = read_verified_object(
            root,
            connection,
            module.revision_digest.as_str(),
            ImmutableObjectKind::AxiModule,
        )?;
        if RevisionDigestV2::from_accepted_bytes(&exact)
            .map_err(|error| corrupt(error.to_string()))?
            != module.revision_digest
        {
            return Err(corrupt(format!(
                "module `{}` exact revision digest mismatch",
                module.module_name
            )));
        }
    }
    Ok(value)
}

fn load_manifest_connection(
    root: &Path,
    connection: &Connection,
    digest: &ObjectBlobIdV2,
) -> StoreResult<AcceptedBuildManifest> {
    let row: (String, String, String, String) = connection.query_row(
        "SELECT repository_id, snapshot_id, tree_id, object_id FROM build_manifests WHERE manifest_digest=?1",
        [digest.as_str()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).optional()?.ok_or_else(|| corrupt(format!("missing build manifest `{digest}`")))?;
    let bytes = read_verified_object(
        root,
        connection,
        digest.as_str(),
        ImmutableObjectKind::BuildManifest,
    )?;
    let value: AcceptedBuildManifest = parse_store_json(&bytes, "AxiStore JSON object")?;
    value.validate()?;
    if value.digest()? != *digest
        || value.repository_id.as_str() != row.0
        || value.accepted_snapshot_id.as_str() != row.1
        || value.accepted_tree_id.as_str() != row.2
        || row.3 != digest.as_str()
    {
        return Err(corrupt(format!(
            "build manifest `{digest}` catalog fields were tampered"
        )));
    }
    Ok(value)
}

fn load_reconciliation_connection(
    root: &Path,
    connection: &Connection,
    id: &ReconciliationIdV2,
) -> StoreResult<SemReconciliationV2> {
    let row: (String, String, String, String, String) = connection.query_row(
        "SELECT repository_id, base_commit_id, left_commit_id, right_commit_id, object_id FROM reconciliations WHERE reconciliation_id=?1",
        [id.as_str()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
    ).optional()?.ok_or_else(|| corrupt(format!("missing reconciliation `{id}`")))?;
    let bytes = read_verified_object(
        root,
        connection,
        id.as_str(),
        ImmutableObjectKind::Reconciliation,
    )?;
    let value: SemReconciliationV2 = parse_store_json(&bytes, "AxiStore JSON object")?;
    value.validate()?;
    if value.reconciliation_id != *id
        || value.repository_id.as_str() != row.0
        || value.base_commit_id.as_str() != row.1
        || value.left.commit_id.as_str() != row.2
        || value.right.commit_id.as_str() != row.3
        || row.4 != id.as_str()
    {
        return Err(corrupt(format!(
            "reconciliation `{id}` catalog fields were tampered"
        )));
    }
    for digest in value.decisions.iter().filter_map(|decision| {
        if let TypedReconciliationDecisionV2::Transport { witness_digest, .. } = decision {
            Some(witness_digest)
        } else {
            None
        }
    }) {
        read_verified_object(
            root,
            connection,
            digest.as_str(),
            ImmutableObjectKind::Certificate,
        )?;
    }
    read_verified_object(
        root,
        connection,
        value.preview_digest.as_str(),
        ImmutableObjectKind::Evidence,
    )?;
    Ok(value)
}

fn validate_manifest_objects(
    root: &Path,
    connection: &Connection,
    manifest: &AcceptedBuildManifest,
) -> StoreResult<()> {
    for (digest, kind) in [
        (&manifest.kernel_ir_digest, ImmutableObjectKind::KernelIr),
        (
            &manifest.canonical_fact_log_digest,
            ImmutableObjectKind::CanonicalFactLog,
        ),
        (
            &manifest.validation_report_digest,
            ImmutableObjectKind::ValidationReport,
        ),
        (
            &manifest.competency_question_report_digest,
            ImmutableObjectKind::CompetencyQuestionReport,
        ),
        (
            &manifest.runtime_theory_report_digest,
            ImmutableObjectKind::TheoryReport,
        ),
        (
            &manifest.trusted_checker_receipt_digest,
            ImmutableObjectKind::VerificationReceipt,
        ),
    ] {
        read_verified_object(root, connection, digest.as_str(), kind)?;
    }
    Ok(())
}

fn query_parent_ids(
    connection: &Connection,
    table: &str,
    child_column: &str,
    parent_column: &str,
    child: &str,
) -> StoreResult<Vec<String>> {
    let sql = format!(
        "SELECT position, {parent_column} FROM {table} WHERE {child_column}=?1 ORDER BY position"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([child], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut values = Vec::new();
    for (expected, row) in rows.enumerate() {
        let (position, parent) = row?;
        if position != usize_to_i64(expected)? {
            return Err(corrupt(format!(
                "{table} parent positions are not contiguous"
            )));
        }
        values.push(parent);
    }
    Ok(values)
}

fn validate_commit_closure(
    root: &Path,
    connection: &Connection,
    head: &CommitIdV2,
) -> StoreResult<()> {
    let mut colors = BTreeMap::<CommitIdV2, u8>::new();
    let mut stack = vec![(head.clone(), false)];
    while let Some((id, exiting)) = stack.pop() {
        if exiting {
            colors.insert(id, 2);
            continue;
        }
        match colors.get(&id).copied().unwrap_or(0) {
            1 => return Err(corrupt("semantic commit DAG contains a cycle")),
            2 => continue,
            _ => {}
        }
        if colors.len() >= MAX_REACHABLE_OBJECTS {
            return Err(corrupt("semantic commit DAG exceeds bounded closure"));
        }
        colors.insert(id.clone(), 1);
        let commit = load_commit_connection(root, connection, &id)?;
        let snapshot = load_snapshot_connection(root, connection, &commit.accepted_snapshot_id)?;
        if snapshot.tree_id != commit.accepted_tree_id {
            return Err(corrupt("commit tree does not match accepted snapshot tree"));
        }
        let manifest = load_manifest_connection(root, connection, &commit.build_manifest_digest)?;
        validate_manifest_objects(root, connection, &manifest)?;
        for gate in &commit.gates {
            let kind = match gate.kind {
                GateKind::CanonicalValidation => ImmutableObjectKind::ValidationReport,
                GateKind::CompetencyQuestions => ImmutableObjectKind::CompetencyQuestionReport,
                GateKind::RuntimeTheory => ImmutableObjectKind::TheoryReport,
                GateKind::Trust => ImmutableObjectKind::VerificationReceipt,
            };
            read_verified_object(root, connection, gate.report_digest.as_str(), kind)?;
        }
        for attachment in &commit.attachments {
            read_verified_object(
                root,
                connection,
                attachment.digest.as_str(),
                attachment.kind,
            )?;
        }
        if let Some(reconciliation) = &commit.reconciliation_id {
            let reconciliation = load_reconciliation_connection(root, connection, reconciliation)?;
            if commit.kind != CommitKind::Merge
                || reconciliation.outcome != ReconciliationOutcomeV2::Materialized
                || reconciliation.left.commit_id != commit.ordered_parents[0]
                || reconciliation.right.commit_id != commit.ordered_parents[1]
                || reconciliation.merged.candidate.accepted_snapshot_id
                    != commit.accepted_snapshot_id
                || reconciliation.merged.candidate.accepted_tree_id != commit.accepted_tree_id
                || reconciliation.merged.candidate.kernel_ir_digest != manifest.kernel_ir_digest
                || reconciliation.merged.gates != manifest_gate_reports(&manifest)
            {
                return Err(corrupt(
                    "merge commit does not bind exact reviewed reconciliation candidates",
                ));
            }
            validate_typed_candidate_compilation(
                root,
                connection,
                &reconciliation.merged.candidate,
            )?;
            for reviewed in [&reconciliation.left, &reconciliation.right] {
                let parent = load_commit_connection(root, connection, &reviewed.commit_id)?;
                let parent_manifest =
                    load_manifest_connection(root, connection, &parent.build_manifest_digest)?;
                if parent.accepted_snapshot_id != reviewed.candidate.accepted_snapshot_id
                    || parent.accepted_tree_id != reviewed.candidate.accepted_tree_id
                    || reviewed.candidate.kernel_ir_digest != parent_manifest.kernel_ir_digest
                    || reviewed.gates != manifest_gate_reports(&parent_manifest)
                {
                    return Err(corrupt(
                        "reviewed parent candidate disagrees with persisted parent commit",
                    ));
                }
                validate_typed_candidate_compilation(root, connection, &reviewed.candidate)?;
            }
        }
        stack.push((id, true));
        for parent in commit.ordered_parents.iter().rev() {
            if colors.get(parent).copied() == Some(1) {
                return Err(corrupt("semantic commit DAG contains a cycle"));
            }
            stack.push((parent.clone(), false));
        }
    }
    Ok(())
}

fn validate_snapshot_closure(
    root: &Path,
    connection: &Connection,
    head: &SnapshotIdV2,
) -> StoreResult<()> {
    let mut colors = BTreeMap::<SnapshotIdV2, u8>::new();
    let mut stack = vec![(head.clone(), false)];
    while let Some((id, exiting)) = stack.pop() {
        if exiting {
            colors.insert(id, 2);
            continue;
        }
        match colors.get(&id).copied().unwrap_or(0) {
            1 => return Err(corrupt("accepted snapshot DAG contains a cycle")),
            2 => continue,
            _ => {}
        }
        if colors.len() >= MAX_REACHABLE_OBJECTS {
            return Err(corrupt("accepted snapshot DAG exceeds bounded closure"));
        }
        colors.insert(id.clone(), 1);
        let snapshot = load_snapshot_connection(root, connection, &id)?;
        stack.push((id, true));
        for parent in snapshot.ordered_parents.iter().rev() {
            if colors.get(parent).copied() == Some(1) {
                return Err(corrupt("accepted snapshot DAG contains a cycle"));
            }
            stack.push((parent.clone(), false));
        }
    }
    Ok(())
}

fn validate_audit_chain(connection: &Connection, state: &StoreState) -> StoreResult<()> {
    let mut statement = connection.prepare("SELECT sequence, event_id, previous_event_id, action, generation, accepted_snapshot_id, accepted_commit_id, ref_map_digest, build_manifest_digest FROM audit_events ORDER BY sequence")?;
    let rows = statement.query_map([], |row| {
        Ok(AuditRow {
            sequence: u64::try_from(row.get::<_, i64>(0)?).unwrap_or(u64::MAX),
            event_id: parse_sql_id(row.get::<_, String>(1)?)?,
            previous: row
                .get::<_, Option<String>>(2)?
                .map(parse_sql_id)
                .transpose()?,
            action: row.get(3)?,
            generation: u64::try_from(row.get::<_, i64>(4)?).unwrap_or(u64::MAX),
            accepted_snapshot: row
                .get::<_, Option<String>>(5)?
                .map(parse_sql_id)
                .transpose()?,
            accepted_commit: row
                .get::<_, Option<String>>(6)?
                .map(parse_sql_id)
                .transpose()?,
            ref_map_digest: parse_sql_id(row.get::<_, String>(7)?)?,
            manifest: row
                .get::<_, Option<String>>(8)?
                .map(parse_sql_id)
                .transpose()?,
        })
    })?;
    let mut previous: Option<ObjectBlobIdV2> = None;
    let mut count = 0u64;
    let mut tail = None;
    for row in rows {
        let row = row?;
        if row.sequence != count || row.previous != previous {
            return Err(corrupt(
                "audit event chain has a gap or previous-hash mismatch",
            ));
        }
        let recomputed = audit_digest(
            &state.repository_id,
            row.sequence,
            row.previous.as_ref(),
            &row.action,
            row.generation,
            row.accepted_snapshot.as_ref(),
            row.accepted_commit.as_ref(),
            &row.ref_map_digest,
            row.manifest.as_ref(),
        );
        if recomputed != row.event_id {
            return Err(corrupt("audit event hash mismatch"));
        }
        previous = Some(row.event_id.clone());
        tail = Some(row.event_id);
        count += 1;
    }
    if tail.as_ref() != Some(&state.audit_event_tail) || count != state.generation + 1 {
        return Err(corrupt("store_state audit tail/generation mismatch"));
    }
    Ok(())
}

fn parse_sql_id<T>(value: String) -> rusqlite::Result<T>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    value.parse::<T>().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    })
}

fn parent_map(connection: &Connection) -> StoreResult<BTreeMap<CommitIdV2, Vec<CommitIdV2>>> {
    let mut statement =
        connection.prepare("SELECT commit_id FROM semantic_commits ORDER BY commit_id")?;
    let ids = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut graph = BTreeMap::new();
    for id in ids {
        let id: CommitIdV2 = parse_id(&id?, "semantic commit id")?;
        let parents = query_parent_ids(
            connection,
            "semantic_commit_parents",
            "commit_id",
            "parent_commit_id",
            id.as_str(),
        )?
        .into_iter()
        .map(|value| parse_id(&value, "parent commit id"))
        .collect::<StoreResult<Vec<_>>>()?;
        graph.insert(id, parents);
    }
    for parents in graph.values() {
        for parent in parents {
            if !graph.contains_key(parent) {
                return Err(corrupt(format!(
                    "semantic DAG references missing parent `{parent}`"
                )));
            }
        }
    }
    validate_graph_acyclic(&graph)?;
    Ok(graph)
}

fn validate_graph_acyclic(graph: &BTreeMap<CommitIdV2, Vec<CommitIdV2>>) -> StoreResult<()> {
    let mut colors = BTreeMap::<CommitIdV2, u8>::new();
    for root in graph.keys() {
        let mut stack = vec![(root.clone(), false)];
        while let Some((id, exiting)) = stack.pop() {
            if exiting {
                colors.insert(id, 2);
                continue;
            }
            match colors.get(&id).copied().unwrap_or(0) {
                1 => return Err(corrupt("semantic DAG contains a parent cycle")),
                2 => continue,
                _ => {}
            }
            colors.insert(id.clone(), 1);
            stack.push((id.clone(), true));
            for parent in graph
                .get(&id)
                .ok_or_else(|| corrupt("semantic DAG node disappeared"))?
                .iter()
                .rev()
            {
                if colors.get(parent).copied() == Some(1) {
                    return Err(corrupt("semantic DAG contains a parent cycle"));
                }
                stack.push((parent.clone(), false));
            }
        }
    }
    Ok(())
}

fn ancestor_set(
    graph: &BTreeMap<CommitIdV2, Vec<CommitIdV2>>,
    start: &CommitIdV2,
) -> StoreResult<BTreeSet<CommitIdV2>> {
    if !graph.contains_key(start) {
        return Err(corrupt(format!(
            "semantic DAG is missing subject `{start}`"
        )));
    }
    let mut values = BTreeSet::new();
    let mut stack = vec![start.clone()];
    while let Some(id) = stack.pop() {
        if !values.insert(id.clone()) {
            continue;
        }
        if values.len() > MAX_REACHABLE_OBJECTS {
            return Err(corrupt("semantic ancestry exceeds bounded closure"));
        }
        stack.extend(
            graph
                .get(&id)
                .ok_or_else(|| corrupt("semantic DAG node disappeared"))?
                .iter()
                .cloned(),
        );
    }
    Ok(values)
}

fn is_ancestor_graph(
    graph: &BTreeMap<CommitIdV2, Vec<CommitIdV2>>,
    ancestor: &CommitIdV2,
    subject: &CommitIdV2,
) -> StoreResult<bool> {
    Ok(ancestor_set(graph, subject)?.contains(ancestor))
}

fn is_ancestor(
    connection: &Connection,
    ancestor: &CommitIdV2,
    subject: &CommitIdV2,
) -> StoreResult<bool> {
    is_ancestor_graph(&parent_map(connection)?, ancestor, subject)
}

fn maximal_common_ancestors(
    transaction: &Transaction<'_>,
    left: &CommitIdV2,
    right: &CommitIdV2,
) -> StoreResult<Vec<CommitIdV2>> {
    maximal_common_ancestors_connection(transaction, left, right)
}

fn maximal_common_ancestors_connection(
    connection: &Connection,
    left: &CommitIdV2,
    right: &CommitIdV2,
) -> StoreResult<Vec<CommitIdV2>> {
    let graph = parent_map(connection)?;
    let left_ancestors = ancestor_set(&graph, left)?;
    let right_ancestors = ancestor_set(&graph, right)?;
    let common = left_ancestors
        .intersection(&right_ancestors)
        .cloned()
        .collect::<Vec<_>>();
    let mut maximal = Vec::new();
    'candidate: for candidate in &common {
        for other in &common {
            if candidate != other && is_ancestor_graph(&graph, candidate, other)? {
                continue 'candidate;
            }
        }
        maximal.push(candidate.clone());
    }
    maximal.sort();
    Ok(maximal)
}

fn shortest_parent_path(
    connection: &Connection,
    subject: &CommitIdV2,
    ancestor: &CommitIdV2,
) -> StoreResult<Option<Vec<CommitIdV2>>> {
    let graph = parent_map(connection)?;
    if !graph.contains_key(subject) || !graph.contains_key(ancestor) {
        return Err(corrupt("lineage endpoint is absent from semantic DAG"));
    }
    let mut queue = VecDeque::from([subject.clone()]);
    let mut next_toward_subject = BTreeMap::<CommitIdV2, CommitIdV2>::new();
    let mut seen = BTreeSet::from([subject.clone()]);
    while let Some(id) = queue.pop_front() {
        if &id == ancestor {
            let mut reverse = vec![ancestor.clone()];
            let mut current = ancestor.clone();
            while &current != subject {
                current = next_toward_subject
                    .get(&current)
                    .ok_or_else(|| corrupt("lineage path reconstruction failed"))?
                    .clone();
                reverse.push(current.clone());
            }
            reverse.reverse();
            return Ok(Some(reverse));
        }
        for parent in graph
            .get(&id)
            .ok_or_else(|| corrupt("semantic DAG node disappeared"))?
        {
            if seen.insert(parent.clone()) {
                next_toward_subject.insert(parent.clone(), id.clone());
                queue.push_back(parent.clone());
            }
        }
    }
    Ok(None)
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS objects(
    object_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK(kind IN ('repository_descriptor','axi_module','kernel_ir','canonical_fact_log','certificate','verification_receipt','validation_report','competency_question_report','theory_report','evidence','accepted_tree','accepted_snapshot','build_manifest','reconciliation','semantic_commit','audit_payload')),
    raw_sha256 TEXT NOT NULL CHECK(length(raw_sha256)=64 AND raw_sha256=lower(raw_sha256)),
    byte_len INTEGER NOT NULL CHECK(byte_len>=0),
    relative_path TEXT NOT NULL UNIQUE
) STRICT;
CREATE TABLE IF NOT EXISTS repository(
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    repository_id TEXT NOT NULL UNIQUE,
    descriptor_object_id TEXT NOT NULL REFERENCES objects(object_id)
) STRICT;
CREATE TABLE IF NOT EXISTS accepted_trees(
    tree_id TEXT PRIMARY KEY,
    repository_id TEXT NOT NULL,
    object_id TEXT NOT NULL UNIQUE REFERENCES objects(object_id),
    FOREIGN KEY(repository_id) REFERENCES repository(repository_id)
) STRICT;
CREATE TABLE IF NOT EXISTS accepted_tree_modules(
    tree_id TEXT NOT NULL REFERENCES accepted_trees(tree_id),
    position INTEGER NOT NULL CHECK(position>=0),
    module_name TEXT NOT NULL,
    module_id TEXT NOT NULL,
    revision_digest TEXT NOT NULL REFERENCES objects(object_id),
    PRIMARY KEY(tree_id, position),
    UNIQUE(tree_id, module_name),
    UNIQUE(tree_id, module_id)
) STRICT;
CREATE TABLE IF NOT EXISTS accepted_snapshots(
    snapshot_id TEXT PRIMARY KEY,
    repository_id TEXT NOT NULL REFERENCES repository(repository_id),
    tree_id TEXT NOT NULL REFERENCES accepted_trees(tree_id),
    object_id TEXT NOT NULL UNIQUE REFERENCES objects(object_id)
) STRICT;
CREATE TABLE IF NOT EXISTS accepted_snapshot_parents(
    snapshot_id TEXT NOT NULL REFERENCES accepted_snapshots(snapshot_id),
    position INTEGER NOT NULL CHECK(position IN (0,1)),
    parent_snapshot_id TEXT NOT NULL REFERENCES accepted_snapshots(snapshot_id),
    PRIMARY KEY(snapshot_id, position),
    UNIQUE(snapshot_id, parent_snapshot_id),
    CHECK(snapshot_id<>parent_snapshot_id)
) STRICT;
CREATE TABLE IF NOT EXISTS build_manifests(
    manifest_digest TEXT PRIMARY KEY,
    repository_id TEXT NOT NULL REFERENCES repository(repository_id),
    snapshot_id TEXT NOT NULL REFERENCES accepted_snapshots(snapshot_id),
    tree_id TEXT NOT NULL REFERENCES accepted_trees(tree_id),
    object_id TEXT NOT NULL UNIQUE REFERENCES objects(object_id)
) STRICT;
CREATE TABLE IF NOT EXISTS semantic_commits(
    commit_id TEXT PRIMARY KEY,
    repository_id TEXT NOT NULL REFERENCES repository(repository_id),
    kind TEXT NOT NULL CHECK(kind IN ('normal','merge')),
    snapshot_id TEXT NOT NULL REFERENCES accepted_snapshots(snapshot_id),
    tree_id TEXT NOT NULL REFERENCES accepted_trees(tree_id),
    manifest_digest TEXT NOT NULL REFERENCES build_manifests(manifest_digest),
    reconciliation_id TEXT,
    object_id TEXT NOT NULL UNIQUE REFERENCES objects(object_id)
) STRICT;
CREATE TABLE IF NOT EXISTS semantic_commit_parents(
    commit_id TEXT NOT NULL REFERENCES semantic_commits(commit_id),
    position INTEGER NOT NULL CHECK(position IN (0,1)),
    parent_commit_id TEXT NOT NULL REFERENCES semantic_commits(commit_id),
    PRIMARY KEY(commit_id, position),
    UNIQUE(commit_id, parent_commit_id),
    CHECK(commit_id<>parent_commit_id)
) STRICT;
CREATE TABLE IF NOT EXISTS reconciliations(
    reconciliation_id TEXT PRIMARY KEY,
    repository_id TEXT NOT NULL REFERENCES repository(repository_id),
    base_commit_id TEXT NOT NULL REFERENCES semantic_commits(commit_id),
    left_commit_id TEXT NOT NULL REFERENCES semantic_commits(commit_id),
    right_commit_id TEXT NOT NULL REFERENCES semantic_commits(commit_id),
    outcome TEXT NOT NULL CHECK(outcome IN ('materialized','rejected')),
    object_id TEXT NOT NULL UNIQUE REFERENCES objects(object_id),
    CHECK(left_commit_id<>right_commit_id)
) STRICT;
CREATE TABLE IF NOT EXISTS refs(
    ref_name TEXT PRIMARY KEY,
    target_commit_id TEXT NOT NULL REFERENCES semantic_commits(commit_id),
    generation INTEGER NOT NULL CHECK(generation>=0),
    immutable INTEGER NOT NULL CHECK(immutable IN (0,1)),
    protected_main INTEGER NOT NULL CHECK(protected_main IN (0,1)),
    CHECK((ref_name='heads/main' AND protected_main=1 AND immutable=0) OR (ref_name<>'heads/main' AND protected_main=0)),
    CHECK((substr(ref_name,1,5)='tags/' AND immutable=1) OR (substr(ref_name,1,5)<>'tags/' AND immutable=0))
) STRICT;
CREATE TABLE IF NOT EXISTS audit_events(
    sequence INTEGER PRIMARY KEY CHECK(sequence>=0),
    event_id TEXT NOT NULL UNIQUE,
    previous_event_id TEXT,
    action TEXT NOT NULL,
    generation INTEGER NOT NULL CHECK(generation>=0),
    accepted_snapshot_id TEXT,
    accepted_commit_id TEXT,
    ref_map_digest TEXT NOT NULL,
    build_manifest_digest TEXT
) STRICT;
CREATE TABLE IF NOT EXISTS store_state(
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    repository_id TEXT NOT NULL REFERENCES repository(repository_id),
    generation INTEGER NOT NULL CHECK(generation>=0),
    accepted_snapshot_id TEXT REFERENCES accepted_snapshots(snapshot_id),
    accepted_commit_id TEXT REFERENCES semantic_commits(commit_id),
    ref_map_digest TEXT NOT NULL,
    build_manifest_digest TEXT REFERENCES build_manifests(manifest_digest),
    audit_event_tail TEXT NOT NULL,
    CHECK((accepted_snapshot_id IS NULL AND accepted_commit_id IS NULL AND build_manifest_digest IS NULL) OR (accepted_snapshot_id IS NOT NULL AND accepted_commit_id IS NOT NULL AND build_manifest_digest IS NOT NULL))
) STRICT;
"#;
