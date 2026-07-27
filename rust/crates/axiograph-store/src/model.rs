use axiograph_kernel::{
    AnswerIdV2, CertificateIdV2, CheckerIdV2, CommitIdV2, FactIdV2, KernelPayloadFingerprintV2,
    MaterializationIdV2, ModuleIdV2, ObjectBlobIdV2, ObjectTypeIdV2, ObligationIdV2, ProposalIdV2,
    QueryIdV2, ReconciliationIdV2, RelationIdV2, RepositoryIdV2, RevisionDigestV2, RoleIdV2,
    RunIdV2, SchemaIdV2, SnapshotIdV2, TheoryIdV2, TreeIdV2,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;

pub const REPOSITORY_DESCRIPTOR_FORMAT: &str = "axiograph_repository_descriptor";
pub const ACCEPTED_TREE_FORMAT: &str = "axiograph_accepted_tree";
pub const ACCEPTED_SNAPSHOT_FORMAT: &str = "axiograph_accepted_snapshot";
pub const BUILD_MANIFEST_FORMAT: &str = "axiograph_accepted_build_manifest";
pub const SEMANTIC_COMMIT_FORMAT: &str = "axiograph_semantic_commit";
pub const RECONCILIATION_FORMAT: &str = "axiograph_reconciliation";
pub const LINEAGE_PROOF_FORMAT: &str = "axiograph_lineage_proof";
pub const STORE_STATE_FORMAT: &str = "axiograph_store_state";
pub const FORMAT_VERSION: u32 = 2;
pub const MAX_RECONCILIATION_PAYLOADS_V2: usize = 131_072;
pub const MAX_RECONCILIATION_DECISIONS_V2: usize = 393_216;

const REQUIRED_NON_CLAIM_FRAGMENTS: [&str; 4] = [
    "not ontology closure",
    "not open-world completeness",
    "not a Lean proof of runtime validation",
    "not general dependent type theory",
];

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ModelError {
    #[error("{0}")]
    Invalid(String),
    #[error("accepted module bytes are not UTF-8: {0}")]
    InvalidUtf8(String),
}

pub type ModelResult<T> = Result<T, ModelError>;

fn invalid(message: impl Into<String>) -> ModelError {
    ModelError::Invalid(message.into())
}

fn validate_text(value: &str, field: &str) -> ModelResult<()> {
    if value.is_empty() || value.len() > 64 * 1024 {
        return Err(invalid(format!(
            "{field} must be non-empty and at most 65536 UTF-8 bytes"
        )));
    }
    Ok(())
}

fn count_field(count: usize) -> ModelResult<[u8; 4]> {
    Ok(u32::try_from(count)
        .map_err(|_| invalid("canonical collection exceeds u32 item bound"))?
        .to_be_bytes())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RepositoryDescriptor {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub genesis_nonce: String,
}

impl RepositoryDescriptor {
    pub fn new(name: impl Into<String>, genesis_nonce: impl Into<String>) -> ModelResult<Self> {
        let descriptor = Self {
            format: REPOSITORY_DESCRIPTOR_FORMAT.to_string(),
            version: FORMAT_VERSION,
            name: name.into(),
            genesis_nonce: genesis_nonce.into(),
        };
        descriptor.validate()?;
        Ok(descriptor)
    }

    pub fn validate(&self) -> ModelResult<()> {
        if self.format != REPOSITORY_DESCRIPTOR_FORMAT || self.version != FORMAT_VERSION {
            return Err(invalid("unsupported repository descriptor format"));
        }
        validate_text(&self.name, "repository name")?;
        validate_text(&self.genesis_nonce, "repository genesis nonce")
    }

    pub fn canonical_bytes(&self) -> ModelResult<Vec<u8>> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|error| invalid(error.to_string()))
    }

    pub fn repository_id(&self) -> ModelResult<RepositoryIdV2> {
        Ok(RepositoryIdV2::from_descriptor_bytes(
            &self.canonical_bytes()?,
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AcceptedModule {
    pub module_name: String,
    pub module_id: ModuleIdV2,
    pub revision_digest: RevisionDigestV2,
}

impl AcceptedModule {
    pub fn from_bytes(
        repository_id: &RepositoryIdV2,
        module_name: impl Into<String>,
        bytes: &[u8],
    ) -> ModelResult<Self> {
        let module_name = module_name.into();
        validate_text(&module_name, "module name")?;
        let revision_digest = RevisionDigestV2::from_accepted_bytes(bytes)
            .map_err(|error| ModelError::InvalidUtf8(error.to_string()))?;
        Ok(Self {
            module_id: ModuleIdV2::derive(repository_id, &module_name),
            module_name,
            revision_digest,
        })
    }

    pub fn validate(&self, repository_id: &RepositoryIdV2) -> ModelResult<()> {
        validate_text(&self.module_name, "module name")?;
        let expected = ModuleIdV2::derive(repository_id, &self.module_name);
        if self.module_id != expected {
            return Err(invalid(format!(
                "module `{}` id does not match repository-scoped identity",
                self.module_name
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AcceptedTree {
    pub format: String,
    pub version: u32,
    pub repository_id: RepositoryIdV2,
    pub tree_id: TreeIdV2,
    pub modules: Vec<AcceptedModule>,
}

impl AcceptedTree {
    pub fn new(repository_id: RepositoryIdV2, modules: Vec<AcceptedModule>) -> ModelResult<Self> {
        let mut tree = Self {
            format: ACCEPTED_TREE_FORMAT.to_string(),
            version: FORMAT_VERSION,
            repository_id,
            tree_id: TreeIdV2::from_canonical_fields(&[b"uninitialized"]),
            modules,
        };
        tree.validate_modules()?;
        tree.tree_id = tree.recompute_id()?;
        Ok(tree)
    }

    fn validate_modules(&self) -> ModelResult<()> {
        let mut previous: Option<&str> = None;
        for module in &self.modules {
            module.validate(&self.repository_id)?;
            if previous.is_some_and(|value| value >= module.module_name.as_str()) {
                return Err(invalid(
                    "accepted tree modules must be strictly sorted by module name",
                ));
            }
            previous = Some(&module.module_name);
        }
        Ok(())
    }

    fn recompute_id(&self) -> ModelResult<TreeIdV2> {
        let count = count_field(self.modules.len())?;
        let version = FORMAT_VERSION.to_be_bytes();
        let mut fields: Vec<&[u8]> = vec![
            ACCEPTED_TREE_FORMAT.as_bytes(),
            &version,
            self.repository_id.as_str().as_bytes(),
            &count,
        ];
        for module in &self.modules {
            fields.push(module.module_name.as_bytes());
            fields.push(module.module_id.as_str().as_bytes());
            fields.push(module.revision_digest.as_str().as_bytes());
        }
        Ok(TreeIdV2::from_canonical_fields(&fields))
    }

    pub fn validate(&self) -> ModelResult<()> {
        if self.format != ACCEPTED_TREE_FORMAT || self.version != FORMAT_VERSION {
            return Err(invalid("unsupported accepted tree format"));
        }
        self.validate_modules()?;
        if self.tree_id != self.recompute_id()? {
            return Err(invalid("accepted tree identity mismatch"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AcceptedSnapshot {
    pub format: String,
    pub version: u32,
    pub repository_id: RepositoryIdV2,
    pub snapshot_id: SnapshotIdV2,
    pub tree_id: TreeIdV2,
    pub ordered_parents: Vec<SnapshotIdV2>,
}

impl AcceptedSnapshot {
    pub fn new(
        repository_id: RepositoryIdV2,
        tree_id: TreeIdV2,
        ordered_parents: Vec<SnapshotIdV2>,
    ) -> ModelResult<Self> {
        let mut snapshot = Self {
            format: ACCEPTED_SNAPSHOT_FORMAT.to_string(),
            version: FORMAT_VERSION,
            repository_id,
            snapshot_id: SnapshotIdV2::from_canonical_fields(&[b"uninitialized"]),
            tree_id,
            ordered_parents,
        };
        snapshot.validate_shape()?;
        snapshot.snapshot_id = snapshot.recompute_id()?;
        Ok(snapshot)
    }

    fn validate_shape(&self) -> ModelResult<()> {
        if self.ordered_parents.len() > 2 {
            return Err(invalid(
                "accepted snapshots permit at most two ordered parents",
            ));
        }
        if self.ordered_parents.iter().collect::<BTreeSet<_>>().len() != self.ordered_parents.len()
        {
            return Err(invalid("accepted snapshot parents must be distinct"));
        }
        Ok(())
    }

    fn recompute_id(&self) -> ModelResult<SnapshotIdV2> {
        let count = count_field(self.ordered_parents.len())?;
        let version = FORMAT_VERSION.to_be_bytes();
        let mut fields: Vec<&[u8]> = vec![
            ACCEPTED_SNAPSHOT_FORMAT.as_bytes(),
            &version,
            self.repository_id.as_str().as_bytes(),
            self.tree_id.as_str().as_bytes(),
            &count,
        ];
        fields.extend(
            self.ordered_parents
                .iter()
                .map(|parent| parent.as_str().as_bytes()),
        );
        Ok(SnapshotIdV2::from_canonical_fields(&fields))
    }

    pub fn validate(&self) -> ModelResult<()> {
        if self.format != ACCEPTED_SNAPSHOT_FORMAT || self.version != FORMAT_VERSION {
            return Err(invalid("unsupported accepted snapshot format"));
        }
        self.validate_shape()?;
        if self.snapshot_id != self.recompute_id()? {
            return Err(invalid("accepted snapshot identity mismatch"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ImmutableObjectKind {
    RepositoryDescriptor,
    AxiModule,
    KernelIr,
    CanonicalFactLog,
    Certificate,
    VerificationReceipt,
    ValidationReport,
    CompetencyQuestionReport,
    TheoryReport,
    Evidence,
    AcceptedTree,
    AcceptedSnapshot,
    BuildManifest,
    Reconciliation,
    SemanticCommit,
    AuditPayload,
}

impl ImmutableObjectKind {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::RepositoryDescriptor => "repository_descriptor",
            Self::AxiModule => "axi_module",
            Self::KernelIr => "kernel_ir",
            Self::CanonicalFactLog => "canonical_fact_log",
            Self::Certificate => "certificate",
            Self::VerificationReceipt => "verification_receipt",
            Self::ValidationReport => "validation_report",
            Self::CompetencyQuestionReport => "competency_question_report",
            Self::TheoryReport => "theory_report",
            Self::Evidence => "evidence",
            Self::AcceptedTree => "accepted_tree",
            Self::AcceptedSnapshot => "accepted_snapshot",
            Self::BuildManifest => "build_manifest",
            Self::Reconciliation => "reconciliation",
            Self::SemanticCommit => "semantic_commit",
            Self::AuditPayload => "audit_payload",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AcceptedBuildManifest {
    pub format: String,
    pub version: u32,
    pub repository_id: RepositoryIdV2,
    pub accepted_tree_id: TreeIdV2,
    pub accepted_snapshot_id: SnapshotIdV2,
    pub ordered_module_closure: Vec<RevisionDigestV2>,
    pub compiler_version: String,
    pub ir_version: String,
    pub kernel_ir_digest: ObjectBlobIdV2,
    pub canonical_fact_log_digest: ObjectBlobIdV2,
    pub validation_report_digest: ObjectBlobIdV2,
    pub competency_question_report_digest: ObjectBlobIdV2,
    pub runtime_theory_report_digest: ObjectBlobIdV2,
    pub trusted_checker_receipt_digest: ObjectBlobIdV2,
    pub non_claims: Vec<String>,
}

impl AcceptedBuildManifest {
    pub fn digest(&self) -> ModelResult<ObjectBlobIdV2> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|error| invalid(error.to_string()))?;
        Ok(ObjectBlobIdV2::from_canonical_fields(&[
            ImmutableObjectKind::BuildManifest.wire().as_bytes(),
            &bytes,
        ]))
    }

    pub fn validate(&self) -> ModelResult<()> {
        if self.format != BUILD_MANIFEST_FORMAT || self.version != FORMAT_VERSION {
            return Err(invalid("unsupported accepted build manifest format"));
        }
        validate_text(&self.compiler_version, "compiler version")?;
        validate_text(&self.ir_version, "IR version")?;
        if self
            .ordered_module_closure
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            != self.ordered_module_closure.len()
        {
            return Err(invalid("build manifest module closure contains duplicates"));
        }
        for required in REQUIRED_NON_CLAIM_FRAGMENTS {
            if !self.non_claims.iter().any(|claim| claim == required) {
                return Err(invalid(format!(
                    "build manifest is missing explicit non-claim `{required}`"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum SemanticId {
    Module(ModuleIdV2),
    Revision(RevisionDigestV2),
    Schema(SchemaIdV2),
    Object(ObjectTypeIdV2),
    Relation(RelationIdV2),
    Role(RoleIdV2),
    Theory(TheoryIdV2),
    Obligation(ObligationIdV2),
    Fact(FactIdV2),
    Tree(TreeIdV2),
    Snapshot(SnapshotIdV2),
    Commit(CommitIdV2),
    Reconciliation(ReconciliationIdV2),
    ObjectBlob(ObjectBlobIdV2),
    Materialization(MaterializationIdV2),
    Query(QueryIdV2),
    Answer(AnswerIdV2),
    Certificate(CertificateIdV2),
    Proposal(ProposalIdV2),
    Run(RunIdV2),
    Checker(CheckerIdV2),
}

impl SemanticId {
    fn wire(&self) -> (&'static str, &str) {
        match self {
            Self::Module(value) => ("module", value.as_str()),
            Self::Revision(value) => ("revision", value.as_str()),
            Self::Schema(value) => ("schema", value.as_str()),
            Self::Object(value) => ("object", value.as_str()),
            Self::Relation(value) => ("relation", value.as_str()),
            Self::Role(value) => ("role", value.as_str()),
            Self::Theory(value) => ("theory", value.as_str()),
            Self::Obligation(value) => ("obligation", value.as_str()),
            Self::Fact(value) => ("fact", value.as_str()),
            Self::Tree(value) => ("tree", value.as_str()),
            Self::Snapshot(value) => ("snapshot", value.as_str()),
            Self::Commit(value) => ("commit", value.as_str()),
            Self::Reconciliation(value) => ("reconciliation", value.as_str()),
            Self::ObjectBlob(value) => ("object_blob", value.as_str()),
            Self::Materialization(value) => ("materialization", value.as_str()),
            Self::Query(value) => ("query", value.as_str()),
            Self::Answer(value) => ("answer", value.as_str()),
            Self::Certificate(value) => ("certificate", value.as_str()),
            Self::Proposal(value) => ("proposal", value.as_str()),
            Self::Run(value) => ("run", value.as_str()),
            Self::Checker(value) => ("checker", value.as_str()),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReindexOperation {
    Preserve,
    Rename,
    Split,
    Merge,
    Drop,
    Add,
    Replace,
}

impl ReindexOperation {
    fn wire(self) -> &'static str {
        match self {
            Self::Preserve => "preserve",
            Self::Rename => "rename",
            Self::Split => "split",
            Self::Merge => "merge",
            Self::Drop => "drop",
            Self::Add => "add",
            Self::Replace => "replace",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SemanticChange {
    pub operation: ReindexOperation,
    pub sources: Vec<SemanticId>,
    pub targets: Vec<SemanticId>,
}

impl SemanticChange {
    fn validate(&self) -> ModelResult<()> {
        let shape_ok = match self.operation {
            ReindexOperation::Preserve | ReindexOperation::Rename | ReindexOperation::Replace => {
                self.sources.len() == 1 && self.targets.len() == 1
            }
            ReindexOperation::Split => self.sources.len() == 1 && self.targets.len() > 1,
            ReindexOperation::Merge => self.sources.len() > 1 && self.targets.len() == 1,
            ReindexOperation::Drop => self.sources.len() == 1 && self.targets.is_empty(),
            ReindexOperation::Add => self.sources.is_empty() && self.targets.len() == 1,
        };
        if !shape_ok {
            return Err(invalid(format!(
                "semantic {:?} change has invalid source/target cardinality {}/{}",
                self.operation,
                self.sources.len(),
                self.targets.len()
            )));
        }
        if self.sources.iter().collect::<BTreeSet<_>>().len() != self.sources.len()
            || self.targets.iter().collect::<BTreeSet<_>>().len() != self.targets.len()
        {
            return Err(invalid(
                "semantic change contains duplicate typed identities",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct SemanticDelta {
    pub changes: Vec<SemanticChange>,
}

impl SemanticDelta {
    fn validate(&self) -> ModelResult<()> {
        for change in &self.changes {
            change.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum GateKind {
    CanonicalValidation,
    CompetencyQuestions,
    RuntimeTheory,
    Trust,
}

impl GateKind {
    fn wire(self) -> &'static str {
        match self {
            Self::CanonicalValidation => "canonical_validation",
            Self::CompetencyQuestions => "competency_questions",
            Self::RuntimeTheory => "runtime_theory",
            Self::Trust => "trust",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateDecision {
    Passed,
    Failed,
}

impl GateDecision {
    fn wire(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PromotionGate {
    pub kind: GateKind,
    pub decision: GateDecision,
    pub report_digest: ObjectBlobIdV2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CommitAttachment {
    pub kind: ImmutableObjectKind,
    pub digest: ObjectBlobIdV2,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStage {
    Proposed,
    Validated,
    Reviewed,
    Accepted,
    Certified,
    Superseded,
    Retracted,
}

impl LifecycleStage {
    fn wire(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Validated => "validated",
            Self::Reviewed => "reviewed",
            Self::Accepted => "accepted",
            Self::Certified => "certified",
            Self::Superseded => "superseded",
            Self::Retracted => "retracted",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LifecycleEvent {
    pub artifact: SemanticId,
    pub from: Option<LifecycleStage>,
    pub to: LifecycleStage,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommitKind {
    Normal,
    Merge,
}

impl CommitKind {
    fn wire(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Merge => "merge",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OriginTrust {
    Native,
    OperatorAttested,
}

impl OriginTrust {
    fn wire(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::OperatorAttested => "operator_attested",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CommitProvenance {
    pub source: String,
    pub command: Option<String>,
    pub origin_trust: OriginTrust,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SemCommitV2 {
    pub format: String,
    pub version: u32,
    pub commit_id: CommitIdV2,
    pub repository_id: RepositoryIdV2,
    pub kind: CommitKind,
    pub ordered_parents: Vec<CommitIdV2>,
    pub accepted_tree_id: TreeIdV2,
    pub accepted_snapshot_id: SnapshotIdV2,
    pub build_manifest_digest: ObjectBlobIdV2,
    pub reconciliation_id: Option<ReconciliationIdV2>,
    pub author: String,
    pub created_at_unix_secs: u64,
    pub message: Option<String>,
    pub action: String,
    pub policy: String,
    pub provenance: CommitProvenance,
    pub delta: SemanticDelta,
    pub gates: Vec<PromotionGate>,
    pub attachments: Vec<CommitAttachment>,
    pub lifecycle_events: Vec<LifecycleEvent>,
}

impl SemCommitV2 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repository_id: RepositoryIdV2,
        kind: CommitKind,
        ordered_parents: Vec<CommitIdV2>,
        accepted_tree_id: TreeIdV2,
        accepted_snapshot_id: SnapshotIdV2,
        build_manifest_digest: ObjectBlobIdV2,
        reconciliation_id: Option<ReconciliationIdV2>,
        author: impl Into<String>,
        created_at_unix_secs: u64,
        message: Option<String>,
        action: impl Into<String>,
        policy: impl Into<String>,
        provenance: CommitProvenance,
        delta: SemanticDelta,
        gates: Vec<PromotionGate>,
        attachments: Vec<CommitAttachment>,
        lifecycle_events: Vec<LifecycleEvent>,
    ) -> ModelResult<Self> {
        let mut commit = Self {
            format: SEMANTIC_COMMIT_FORMAT.to_string(),
            version: FORMAT_VERSION,
            commit_id: CommitIdV2::from_canonical_fields(&[b"uninitialized"]),
            repository_id,
            kind,
            ordered_parents,
            accepted_tree_id,
            accepted_snapshot_id,
            build_manifest_digest,
            reconciliation_id,
            author: author.into(),
            created_at_unix_secs,
            message,
            action: action.into(),
            policy: policy.into(),
            provenance,
            delta,
            gates,
            attachments,
            lifecycle_events,
        };
        commit.validate_shape()?;
        commit.commit_id = commit.recompute_id()?;
        Ok(commit)
    }

    fn validate_shape(&self) -> ModelResult<()> {
        match self.kind {
            CommitKind::Normal if self.ordered_parents.len() <= 1 => {}
            CommitKind::Merge if self.ordered_parents.len() == 2 => {}
            CommitKind::Normal => return Err(invalid("normal commit requires zero or one parent")),
            CommitKind::Merge => {
                return Err(invalid(
                    "merge commit requires exactly two ordered parents [target, source]",
                ))
            }
        }
        if self.ordered_parents.iter().collect::<BTreeSet<_>>().len() != self.ordered_parents.len()
        {
            return Err(invalid("semantic commit parents must be distinct"));
        }
        if self.kind == CommitKind::Merge && self.reconciliation_id.is_none() {
            return Err(invalid("merge commit requires an immutable reconciliation"));
        }
        if self.kind != CommitKind::Merge && self.reconciliation_id.is_some() {
            return Err(invalid("only merge commits may bind reconciliation"));
        }
        validate_text(&self.author, "commit author")?;
        validate_text(&self.action, "commit action")?;
        validate_text(&self.policy, "commit policy")?;
        validate_text(&self.provenance.source, "commit provenance source")?;
        if self
            .message
            .as_ref()
            .is_some_and(|value| value.len() > 64 * 1024)
            || self
                .provenance
                .command
                .as_ref()
                .is_some_and(|value| value.len() > 64 * 1024)
        {
            return Err(invalid("optional commit text exceeds 65536 UTF-8 bytes"));
        }
        self.delta.validate()?;
        let gate_kinds = self
            .gates
            .iter()
            .map(|gate| gate.kind)
            .collect::<BTreeSet<_>>();
        if gate_kinds.len() != self.gates.len() {
            return Err(invalid(
                "semantic commit has duplicate promotion gate kinds",
            ));
        }
        let required = BTreeSet::from([
            GateKind::CanonicalValidation,
            GateKind::CompetencyQuestions,
            GateKind::RuntimeTheory,
            GateKind::Trust,
        ]);
        if gate_kinds != required {
            return Err(invalid("protected main requires exactly the canonical validation, competency-question, trust, and runtime-theory gates"));
        }
        if self
            .gates
            .iter()
            .any(|gate| gate.decision != GateDecision::Passed)
        {
            return Err(invalid("protected main rejects failed promotion gates"));
        }
        for event in &self.lifecycle_events {
            validate_text(&event.reason, "lifecycle reason")?;
        }
        Ok(())
    }

    fn recompute_id(&self) -> ModelResult<CommitIdV2> {
        let parent_count = count_field(self.ordered_parents.len())?;
        let change_count = count_field(self.delta.changes.len())?;
        let gate_count = count_field(self.gates.len())?;
        let attachment_count = count_field(self.attachments.len())?;
        let lifecycle_count = count_field(self.lifecycle_events.len())?;
        let created = self.created_at_unix_secs.to_be_bytes();
        let mut fields: Vec<Vec<u8>> = vec![
            SEMANTIC_COMMIT_FORMAT.as_bytes().to_vec(),
            FORMAT_VERSION.to_be_bytes().to_vec(),
            self.repository_id.as_str().as_bytes().to_vec(),
            self.kind.wire().as_bytes().to_vec(),
            parent_count.to_vec(),
        ];
        for parent in &self.ordered_parents {
            fields.push(parent.as_str().as_bytes().to_vec());
        }
        fields.extend([
            self.accepted_tree_id.as_str().as_bytes().to_vec(),
            self.accepted_snapshot_id.as_str().as_bytes().to_vec(),
            self.build_manifest_digest.as_str().as_bytes().to_vec(),
        ]);
        match &self.reconciliation_id {
            Some(value) => {
                fields.push(vec![1]);
                fields.push(value.as_str().as_bytes().to_vec());
            }
            None => fields.push(vec![0]),
        }
        fields.push(self.author.as_bytes().to_vec());
        fields.push(created.to_vec());
        for value in [&self.message, &self.provenance.command] {
            match value {
                Some(value) => {
                    fields.push(vec![1]);
                    fields.push(value.as_bytes().to_vec());
                }
                None => fields.push(vec![0]),
            }
        }
        fields.extend([
            self.action.as_bytes().to_vec(),
            self.policy.as_bytes().to_vec(),
            self.provenance.source.as_bytes().to_vec(),
            self.provenance.origin_trust.wire().as_bytes().to_vec(),
            change_count.to_vec(),
        ]);
        for change in &self.delta.changes {
            fields.push(change.operation.wire().as_bytes().to_vec());
            fields.push(count_field(change.sources.len())?.to_vec());
            for value in &change.sources {
                let (kind, id) = value.wire();
                fields.push(kind.as_bytes().to_vec());
                fields.push(id.as_bytes().to_vec());
            }
            fields.push(count_field(change.targets.len())?.to_vec());
            for value in &change.targets {
                let (kind, id) = value.wire();
                fields.push(kind.as_bytes().to_vec());
                fields.push(id.as_bytes().to_vec());
            }
        }
        fields.push(gate_count.to_vec());
        for gate in &self.gates {
            fields.push(gate.kind.wire().as_bytes().to_vec());
            fields.push(gate.decision.wire().as_bytes().to_vec());
            fields.push(gate.report_digest.as_str().as_bytes().to_vec());
        }
        fields.push(attachment_count.to_vec());
        for attachment in &self.attachments {
            fields.push(attachment.kind.wire().as_bytes().to_vec());
            fields.push(attachment.digest.as_str().as_bytes().to_vec());
        }
        fields.push(lifecycle_count.to_vec());
        for event in &self.lifecycle_events {
            let (kind, id) = event.artifact.wire();
            fields.push(kind.as_bytes().to_vec());
            fields.push(id.as_bytes().to_vec());
            match event.from {
                Some(value) => {
                    fields.push(vec![1]);
                    fields.push(value.wire().as_bytes().to_vec());
                }
                None => fields.push(vec![0]),
            }
            fields.push(event.to.wire().as_bytes().to_vec());
            fields.push(event.reason.as_bytes().to_vec());
        }
        let refs = fields.iter().map(Vec::as_slice).collect::<Vec<_>>();
        Ok(CommitIdV2::from_canonical_fields(&refs))
    }

    pub fn validate(&self) -> ModelResult<()> {
        if self.format != SEMANTIC_COMMIT_FORMAT || self.version != FORMAT_VERSION {
            return Err(invalid("unsupported semantic commit format"));
        }
        self.validate_shape()?;
        if self.commit_id != self.recompute_id()? {
            return Err(invalid("semantic commit identity mismatch"));
        }
        if self.ordered_parents.contains(&self.commit_id) {
            return Err(invalid("semantic commit cannot parent itself"));
        }
        Ok(())
    }
}

/// Exact report commitments required before a typed candidate can participate
/// in protected reconciliation. `trust` is the receipt from the configured
/// trusted checker; the other three reports are Rust/runtime evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CandidateGateReportsV2 {
    pub canonical_validation: ObjectBlobIdV2,
    pub competency_questions: ObjectBlobIdV2,
    pub trust: ObjectBlobIdV2,
    pub runtime_theory: ObjectBlobIdV2,
}

/// The finite, typed semantic payload of one candidate snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TypedCandidatePayloadV2 {
    pub accepted_snapshot_id: SnapshotIdV2,
    pub accepted_tree_id: TreeIdV2,
    pub root_module_id: ModuleIdV2,
    pub kernel_ir_digest: ObjectBlobIdV2,
    /// Typed finite category/refinement/context gate for the exact compiled
    /// candidate. AxiStore independently recompiles and reproduces this receipt.
    pub finite_theory_gate: axiograph_kernel::FiniteTheoryGateReceiptIr,
    pub payloads: Vec<KernelPayloadFingerprintV2>,
}

impl TypedCandidatePayloadV2 {
    pub fn checked(
        accepted_snapshot_id: SnapshotIdV2,
        accepted_tree_id: TreeIdV2,
        root_module_id: ModuleIdV2,
        kernel_ir_digest: ObjectBlobIdV2,
        finite_theory_gate: axiograph_kernel::FiniteTheoryGateReceiptIr,
        mut payloads: Vec<KernelPayloadFingerprintV2>,
    ) -> ModelResult<Self> {
        payloads.sort();
        let candidate = Self {
            accepted_snapshot_id,
            accepted_tree_id,
            root_module_id,
            kernel_ir_digest,
            finite_theory_gate,
            payloads,
        };
        candidate.validate()?;
        Ok(candidate)
    }

    fn validate(&self) -> ModelResult<()> {
        if !self.finite_theory_gate.passed
            || self.finite_theory_gate.consumer
                != axiograph_kernel::FiniteTheoryGateConsumerIr::Merge
            || self.finite_theory_gate.accepted_snapshot_id != self.accepted_snapshot_id
            || !self.finite_theory_gate.residual_obligations.is_empty()
        {
            return Err(invalid(
                "typed candidate requires a passing residual-free Merge finite-theory gate bound to its exact snapshot",
            ));
        }
        if self.payloads.len() > MAX_RECONCILIATION_PAYLOADS_V2 {
            return Err(invalid(format!(
                "candidate payload count exceeds finite bound {MAX_RECONCILIATION_PAYLOADS_V2}"
            )));
        }
        if self.payloads.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(invalid(
                "candidate payload fingerprints must be strictly sorted and unique",
            ));
        }
        if self
            .payloads
            .iter()
            .map(|payload| &payload.semantic_ref)
            .collect::<BTreeSet<_>>()
            .len()
            != self.payloads.len()
        {
            return Err(invalid(
                "candidate contains multiple payload fingerprints for one typed semantic ref",
            ));
        }
        Ok(())
    }
}

/// A parent candidate whose exact commit and complete gate evidence were
/// reviewed before a reconciliation decision was recorded.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReviewedParentCandidateV2 {
    pub commit_id: CommitIdV2,
    pub candidate: TypedCandidatePayloadV2,
    pub gates: CandidateGateReportsV2,
    pub reviewer: String,
    pub reviewed_at_unix_secs: u64,
}

impl ReviewedParentCandidateV2 {
    fn validate(&self) -> ModelResult<()> {
        self.candidate.validate()?;
        validate_text(&self.reviewer, "candidate reviewer")
    }
}

/// The reviewed candidate selected for materialization. It has no commit id
/// yet: `SemCommitV2` is derived only after this reconciliation is immutable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReviewedMergedCandidateV2 {
    pub candidate: TypedCandidatePayloadV2,
    pub gates: CandidateGateReportsV2,
    pub reviewer: String,
    pub reviewed_at_unix_secs: u64,
}

impl ReviewedMergedCandidateV2 {
    fn validate(&self) -> ModelResult<()> {
        self.candidate.validate()?;
        validate_text(&self.reviewer, "merged candidate reviewer")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CandidateOriginV2 {
    Left,
    Right,
    Both,
}

/// Every parent payload occurrence and every merged payload is accounted for
/// by exactly one decision. `Keep` is exact payload identity. `Transport`
/// requires an immutable witness commitment; Rust does not treat that witness
/// as a proof until the configured checker accepts it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TypedReconciliationDecisionV2 {
    Keep {
        origin: CandidateOriginV2,
        source: KernelPayloadFingerprintV2,
        target: KernelPayloadFingerprintV2,
        rationale: String,
    },
    Drop {
        origin: CandidateOriginV2,
        source: KernelPayloadFingerprintV2,
        rationale: String,
    },
    Introduce {
        target: KernelPayloadFingerprintV2,
        rationale: String,
    },
    Transport {
        origin: CandidateOriginV2,
        source: KernelPayloadFingerprintV2,
        target: KernelPayloadFingerprintV2,
        witness_digest: ObjectBlobIdV2,
        rationale: String,
    },
}

impl TypedReconciliationDecisionV2 {
    fn rationale(&self) -> &str {
        match self {
            Self::Keep { rationale, .. }
            | Self::Drop { rationale, .. }
            | Self::Introduce { rationale, .. }
            | Self::Transport { rationale, .. } => rationale,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationOutcomeV2 {
    Materialized,
    Rejected,
}

const RECONCILIATION_NON_CLAIMS_V2: [&str; 4] = [
    "finite payload union only",
    "not arbitrary categorical colimit completeness",
    "not general dependent type transport",
    "not HoTT univalence or higher-path equivalence",
];

/// Immutable V2 semantic reconciliation over two reviewed, typed parent
/// candidates and one reviewed result candidate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SemReconciliationV2 {
    pub format: String,
    pub version: u32,
    pub reconciliation_id: ReconciliationIdV2,
    pub repository_id: RepositoryIdV2,
    pub base_commit_id: CommitIdV2,
    pub left: ReviewedParentCandidateV2,
    pub right: ReviewedParentCandidateV2,
    pub merged: ReviewedMergedCandidateV2,
    pub decisions: Vec<TypedReconciliationDecisionV2>,
    pub preview_digest: ObjectBlobIdV2,
    pub outcome: ReconciliationOutcomeV2,
    pub non_claims: Vec<String>,
}

impl SemReconciliationV2 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repository_id: RepositoryIdV2,
        base_commit_id: CommitIdV2,
        left: ReviewedParentCandidateV2,
        right: ReviewedParentCandidateV2,
        merged: ReviewedMergedCandidateV2,
        decisions: Vec<TypedReconciliationDecisionV2>,
        preview_digest: ObjectBlobIdV2,
        outcome: ReconciliationOutcomeV2,
    ) -> ModelResult<Self> {
        let mut keyed_decisions = decisions
            .into_iter()
            .map(|decision| {
                serde_json::to_vec(&decision)
                    .map(|bytes| (bytes, decision))
                    .map_err(|error| invalid(error.to_string()))
            })
            .collect::<ModelResult<Vec<_>>>()?;
        keyed_decisions.sort_by(|left, right| left.0.cmp(&right.0));
        let decisions = keyed_decisions
            .into_iter()
            .map(|(_, decision)| decision)
            .collect();
        let mut value = Self {
            format: RECONCILIATION_FORMAT.to_string(),
            version: FORMAT_VERSION,
            reconciliation_id: ReconciliationIdV2::from_canonical_fields(&[b"uninitialized"]),
            repository_id,
            base_commit_id,
            left,
            right,
            merged,
            decisions,
            preview_digest,
            outcome,
            non_claims: RECONCILIATION_NON_CLAIMS_V2
                .into_iter()
                .map(str::to_string)
                .collect(),
        };
        value.validate_shape()?;
        value.reconciliation_id = value.recompute_id()?;
        Ok(value)
    }

    fn validate_shape(&self) -> ModelResult<()> {
        if self.left.commit_id == self.right.commit_id {
            return Err(invalid("reconciliation parent tips must differ"));
        }
        self.left.validate()?;
        self.right.validate()?;
        self.merged.validate()?;
        for required in RECONCILIATION_NON_CLAIMS_V2 {
            if !self.non_claims.iter().any(|claim| claim == required) {
                return Err(invalid(format!(
                    "reconciliation is missing explicit non-claim `{required}`"
                )));
            }
        }
        if self.decisions.len() > MAX_RECONCILIATION_DECISIONS_V2 {
            return Err(invalid(format!(
                "typed decision count exceeds finite bound {MAX_RECONCILIATION_DECISIONS_V2}"
            )));
        }
        let encoded = self
            .decisions
            .iter()
            .map(serde_json::to_vec)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| invalid(error.to_string()))?;
        if encoded.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(invalid(
                "typed reconciliation decisions must be strictly canonical and unique",
            ));
        }
        for decision in &self.decisions {
            validate_text(decision.rationale(), "typed reconciliation rationale")?;
        }
        if self.outcome == ReconciliationOutcomeV2::Materialized {
            self.validate_union_preservation()?;
        }
        Ok(())
    }

    fn validate_union_preservation(&self) -> ModelResult<()> {
        let left = self
            .left
            .candidate
            .payloads
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let right = self
            .right
            .candidate
            .payloads
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let merged = self
            .merged
            .candidate
            .payloads
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut covered = BTreeSet::<(u8, KernelPayloadFingerprintV2)>::new();
        let mut produced = BTreeSet::<KernelPayloadFingerprintV2>::new();

        let mut cover = |origin: CandidateOriginV2,
                         source: &KernelPayloadFingerprintV2|
         -> ModelResult<()> {
            let sides: &[u8] = match origin {
                CandidateOriginV2::Left => &[0],
                CandidateOriginV2::Right => &[1],
                CandidateOriginV2::Both => &[0, 1],
            };
            for side in sides {
                let exists = if *side == 0 {
                    left.contains(source)
                } else {
                    right.contains(source)
                };
                if !exists {
                    return Err(invalid(
                        "typed reconciliation decision names a payload absent from its declared origin",
                    ));
                }
                if !covered.insert((*side, source.clone())) {
                    return Err(invalid(
                        "parent payload occurrence has more than one reconciliation decision",
                    ));
                }
            }
            Ok(())
        };

        for decision in &self.decisions {
            match decision {
                TypedReconciliationDecisionV2::Keep {
                    origin,
                    source,
                    target,
                    ..
                } => {
                    cover(*origin, source)?;
                    if source != target {
                        return Err(invalid(
                            "keep requires exact typed ref and payload-fingerprint identity",
                        ));
                    }
                    if !merged.contains(target) || !produced.insert(target.clone()) {
                        return Err(invalid(
                            "keep target is absent from merged payload or produced more than once",
                        ));
                    }
                }
                TypedReconciliationDecisionV2::Drop { origin, source, .. } => {
                    cover(*origin, source)?;
                }
                TypedReconciliationDecisionV2::Introduce { target, .. } => {
                    if !merged.contains(target) || !produced.insert(target.clone()) {
                        return Err(invalid(
                            "introduce target is absent from merged payload or produced more than once",
                        ));
                    }
                }
                TypedReconciliationDecisionV2::Transport {
                    origin,
                    source,
                    target,
                    ..
                } => {
                    cover(*origin, source)?;
                    if source == target {
                        return Err(invalid("transport cannot disguise an exact keep"));
                    }
                    if !merged.contains(target) || !produced.insert(target.clone()) {
                        return Err(invalid(
                            "transport target is absent from merged payload or produced more than once",
                        ));
                    }
                }
            }
        }

        let expected_inputs = left
            .into_iter()
            .map(|payload| (0, payload))
            .chain(right.into_iter().map(|payload| (1, payload)))
            .collect::<BTreeSet<_>>();
        if covered != expected_inputs {
            return Err(invalid(
                "materialized reconciliation does not decide every left/right payload occurrence",
            ));
        }
        if produced != merged {
            return Err(invalid(
                "materialized reconciliation does not account for every merged payload fingerprint",
            ));
        }
        Ok(())
    }

    fn recompute_id(&self) -> ModelResult<ReconciliationIdV2> {
        let payload = serde_json::to_vec(&(
            RECONCILIATION_FORMAT,
            FORMAT_VERSION,
            &self.repository_id,
            &self.base_commit_id,
            &self.left,
            &self.right,
            &self.merged,
            &self.decisions,
            &self.preview_digest,
            self.outcome,
            &self.non_claims,
        ))
        .map_err(|error| invalid(error.to_string()))?;
        Ok(ReconciliationIdV2::from_canonical_fields(&[&payload]))
    }

    pub fn validate(&self) -> ModelResult<()> {
        if self.format != RECONCILIATION_FORMAT || self.version != FORMAT_VERSION {
            return Err(invalid("unsupported V2 semantic reconciliation format"));
        }
        self.validate_shape()?;
        if self.reconciliation_id != self.recompute_id()? {
            return Err(invalid("V2 semantic reconciliation identity mismatch"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StoreState {
    pub format: String,
    pub version: u32,
    pub repository_id: RepositoryIdV2,
    pub generation: u64,
    pub accepted_snapshot_id: Option<SnapshotIdV2>,
    pub accepted_commit_id: Option<CommitIdV2>,
    pub ref_map_digest: ObjectBlobIdV2,
    pub build_manifest_digest: Option<ObjectBlobIdV2>,
    pub audit_event_tail: ObjectBlobIdV2,
}

impl StoreState {
    pub fn digest(&self) -> ObjectBlobIdV2 {
        let generation = self.generation.to_be_bytes();
        let version = FORMAT_VERSION.to_be_bytes();
        let mut fields = vec![
            STORE_STATE_FORMAT.as_bytes(),
            version.as_slice(),
            self.repository_id.as_str().as_bytes(),
            generation.as_slice(),
        ];
        let snapshot_presence = [u8::from(self.accepted_snapshot_id.is_some())];
        fields.push(&snapshot_presence);
        if let Some(value) = &self.accepted_snapshot_id {
            fields.push(value.as_str().as_bytes());
        }
        let commit_presence = [u8::from(self.accepted_commit_id.is_some())];
        fields.push(&commit_presence);
        if let Some(value) = &self.accepted_commit_id {
            fields.push(value.as_str().as_bytes());
        }
        fields.push(self.ref_map_digest.as_str().as_bytes());
        let manifest_presence = [u8::from(self.build_manifest_digest.is_some())];
        fields.push(&manifest_presence);
        if let Some(value) = &self.build_manifest_digest {
            fields.push(value.as_str().as_bytes());
        }
        fields.push(self.audit_event_tail.as_str().as_bytes());
        ObjectBlobIdV2::from_canonical_fields(&fields)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LineageProof {
    pub format: String,
    pub version: u32,
    pub repository_id: RepositoryIdV2,
    pub subject: CommitIdV2,
    pub ancestor: CommitIdV2,
    pub ordered_commits: Vec<SemCommitV2>,
}

impl LineageProof {
    pub fn validate(&self) -> ModelResult<()> {
        if self.format != LINEAGE_PROOF_FORMAT || self.version != FORMAT_VERSION {
            return Err(invalid("unsupported lineage proof format"));
        }
        if self.ordered_commits.first().map(|value| &value.commit_id) != Some(&self.subject)
            || self.ordered_commits.last().map(|value| &value.commit_id) != Some(&self.ancestor)
        {
            return Err(invalid(
                "lineage proof endpoints do not match ordered commits",
            ));
        }
        let mut seen = BTreeSet::new();
        for (index, commit) in self.ordered_commits.iter().enumerate() {
            commit.validate()?;
            if commit.repository_id != self.repository_id {
                return Err(invalid("lineage proof mixes repositories"));
            }
            if !seen.insert(commit.commit_id.clone()) {
                return Err(invalid("lineage proof repeats a commit"));
            }
            if let Some(next) = self.ordered_commits.get(index + 1) {
                if !commit.ordered_parents.contains(&next.commit_id) {
                    return Err(invalid("lineage proof skips a parent edge"));
                }
            }
        }
        Ok(())
    }
}

pub fn blob_id(kind: ImmutableObjectKind, bytes: &[u8]) -> ObjectBlobIdV2 {
    ObjectBlobIdV2::from_canonical_fields(&[kind.wire().as_bytes(), bytes])
}

pub fn required_non_claims() -> Vec<String> {
    REQUIRED_NON_CLAIM_FRAGMENTS
        .into_iter()
        .map(str::to_string)
        .collect()
}

pub(crate) fn empty_ref_map_digest() -> ObjectBlobIdV2 {
    ObjectBlobIdV2::from_canonical_fields(&[
        b"axiograph_ref_map",
        &FORMAT_VERSION.to_be_bytes(),
        b"\0\0\0\0",
    ])
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn audit_digest(
    repository_id: &RepositoryIdV2,
    sequence: u64,
    previous: Option<&ObjectBlobIdV2>,
    action: &str,
    generation: u64,
    accepted_snapshot: Option<&SnapshotIdV2>,
    accepted_commit: Option<&CommitIdV2>,
    ref_map_digest: &ObjectBlobIdV2,
    manifest: Option<&ObjectBlobIdV2>,
) -> ObjectBlobIdV2 {
    let sequence = sequence.to_be_bytes();
    let generation = generation.to_be_bytes();
    let version = FORMAT_VERSION.to_be_bytes();
    let previous_presence = [u8::from(previous.is_some())];
    let snapshot_presence = [u8::from(accepted_snapshot.is_some())];
    let commit_presence = [u8::from(accepted_commit.is_some())];
    let manifest_presence = [u8::from(manifest.is_some())];
    let mut fields = vec![
        b"axiograph_audit_event".as_slice(),
        version.as_slice(),
        repository_id.as_str().as_bytes(),
        sequence.as_slice(),
        previous_presence.as_slice(),
    ];
    if let Some(value) = previous {
        fields.push(value.as_str().as_bytes());
    }
    fields.extend([
        action.as_bytes(),
        generation.as_slice(),
        snapshot_presence.as_slice(),
    ]);
    if let Some(value) = accepted_snapshot {
        fields.push(value.as_str().as_bytes());
    }
    fields.push(&commit_presence);
    if let Some(value) = accepted_commit {
        fields.push(value.as_str().as_bytes());
    }
    fields.push(ref_map_digest.as_str().as_bytes());
    fields.push(&manifest_presence);
    if let Some(value) = manifest {
        fields.push(value.as_str().as_bytes());
    }
    ObjectBlobIdV2::from_canonical_fields(&fields)
}
