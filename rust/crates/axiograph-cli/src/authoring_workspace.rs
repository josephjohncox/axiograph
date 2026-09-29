//! Workspace-aware typed ontology authoring service and transport adapters.
//!
//! CLI, LSP, MCP, and HTTP all deserialize `AuthoringWorkspaceRequestV1` and
//! call `AuthoringWorkspaceService::execute`. The adapters do not implement
//! semantic logic of their own. Runtime reports are finite, decidable checks
//! over canonical `.axi` plus the compiled kernel/runtime IR; they are not part
//! of the trusted `Axiograph.VerifyMain` import closure.

use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;
use std::io::{self, BufRead, BufReader, Write};
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use std::thread::JoinHandle;

use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper::header::CONTENT_TYPE;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use lsp_server::{Connection, ErrorCode, Message, Notification, Request as LspRequest};
use lsp_types::{
    CodeActionProviderCapability, Diagnostic, DiagnosticSeverity, ExecuteCommandOptions,
    InitializeResult, Position, PublishDiagnosticsParams, Range,
    ServerCapabilities as LspServerCapabilities, ServerInfo as LspServerInfo,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri, WorkDoneProgressOptions,
};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ErrorData, Implementation, JsonObject,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServiceExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

use axiograph_kernel::{KernelPayloadFingerprintV2, KernelRefV2};
use axiograph_pathdb::axi_semantics::MetaPlaneIndex;
use axiograph_pathdb::kernel_ir::{RuntimeIrRef, RuntimeModuleIndex, RuntimeSchemaIndex};
use axiograph_pathdb::{
    default_evidence_policy_v1, default_world_assumption_v1, AxiDigest, PathDB,
    RuntimeTheoryClosureTierV1,
};

mod projection;
pub(crate) use projection::{
    AuthoringDetailV1, AuthoringNestedCollectionV1, AuthoringNestedPageRequestV1,
    AuthoringPresentationV1, AuthoringSectionV1,
};

pub(crate) const AUTHORING_WORKSPACE_REQUEST_VERSION_V1: &str = "authoring_workspace_request_v1";
pub(crate) const AUTHORING_WORKSPACE_REPORT_VERSION_V1: &str = "authoring_workspace_report_v1";
pub(crate) const AUTHORING_WORKSPACE_TOOL_NAME: &str = "axiograph_authoring_workspace";
pub(crate) const AUTHORING_WORKSPACE_LSP_COMMAND: &str = "axiograph.authoring.workspace";
const MAX_AUTHORING_BODY_BYTES: usize = 1024 * 1024;
const MAX_AUTHORING_SOURCE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_AUTHORING_PROTOCOL_FRAME_BYTES: usize = 16 * 1024 * 1024;
const MAX_AUTHORING_PROTOCOL_QUEUE: usize = 64;
const MAX_AUTHORING_HTTP_CONNECTIONS: usize = 32;
const MAX_AUTHORING_HTTP_CONNECTION_TIME: std::time::Duration = std::time::Duration::from_secs(30);
const MAX_AUTHORING_LSP_DOCUMENTS: usize = 64;
const MAX_AUTHORING_LSP_TOTAL_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const AUTHORING_DIAGNOSTIC_ORDER_V1: &str =
    "authoring_pipeline_order_with_canonical_import_closure_source_suborder";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AuthoringWorkspaceOperationV1 {
    #[default]
    Inspect,
    ApplyRepair,
    Validate,
    PromotionReview,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AuthoringWorkspaceRequestV1 {
    pub version: String,
    #[serde(default)]
    pub presentation: AuthoringPresentationV1,
    #[serde(default)]
    pub operation: AuthoringWorkspaceOperationV1,
    /// Workspace-relative canonical `.axi` root module.
    pub axi_path: String,
    /// Unsaved root-buffer replacement. Imports still resolve from the workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axi_text: Option<String>,
    /// Optional workspace-relative baseline used for finite compiled-IR diffing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_axi_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_axi_text: Option<String>,
    /// Optional workspace-relative `.cq` source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cq_path: Option<String>,
    /// Unsaved or inline `.cq` replacement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cq_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub olog_fragment: Option<crate::typed_authoring::OlogFragmentV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_ir_v1: Option<crate::query_ir::QueryIrV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus_variable: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apply_refinement_handle_id: Option<String>,
}

impl AuthoringWorkspaceRequestV1 {
    fn validate(&self) -> Result<()> {
        self.presentation.validate()?;
        if self.version != AUTHORING_WORKSPACE_REQUEST_VERSION_V1 {
            return Err(anyhow!(
                "unsupported authoring request version `{}` (expected `{}`)",
                self.version,
                AUTHORING_WORKSPACE_REQUEST_VERSION_V1
            ));
        }
        if self.axi_path.trim().is_empty() {
            return Err(anyhow!("authoring request requires non-empty `axi_path`"));
        }
        if self.operation == AuthoringWorkspaceOperationV1::ApplyRepair
            && self
                .apply_refinement_handle_id
                .as_deref()
                .is_none_or(str::is_empty)
        {
            return Err(anyhow!(
                "apply_repair requires `apply_refinement_handle_id`"
            ));
        }
        if self.apply_refinement_handle_id.is_some()
            && self.olog_fragment.is_none()
            && self.query_ir_v1.is_none()
        {
            return Err(anyhow!(
                "a refinement handle requires `olog_fragment` or `query_ir_v1`"
            ));
        }
        if self.olog_fragment.is_some()
            && self.query_ir_v1.is_some()
            && self.apply_refinement_handle_id.is_some()
        {
            return Err(anyhow!(
                "apply one refinement domain at a time; do not combine olog and query inputs"
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AuthoringDiagnosticSeverityV1 {
    Error,
    Warning,
    Information,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct AuthoringDiagnosticV1 {
    pub severity: AuthoringDiagnosticSeverityV1,
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_hint: Option<String>,
    /// Absent means explicitly unlocated; legacy path/line alone is not token precision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<crate::axi_input::diagnostics::SourceLocationV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct AuthoringDiagnosticCollectionV1 {
    pub observed_errors: usize,
    pub returned_errors: usize,
    pub omitted_observed_errors: usize,
    pub message_bytes: usize,
    pub work_units: usize,
    pub work_exhausted: bool,
    pub truncated: bool,
    pub max_items: usize,
    pub max_message_bytes: usize,
    pub max_work: usize,
    pub order: String,
}

impl Default for AuthoringDiagnosticCollectionV1 {
    fn default() -> Self {
        Self {
            observed_errors: 0,
            returned_errors: 0,
            omitted_observed_errors: 0,
            message_bytes: 0,
            work_units: 0,
            work_exhausted: false,
            truncated: false,
            max_items: axiograph_kernel::MAX_KERNEL_DIAGNOSTIC_ITEMS,
            max_message_bytes: axiograph_kernel::MAX_KERNEL_DIAGNOSTIC_MESSAGE_BYTES,
            max_work: axiograph_kernel::MAX_KERNEL_DIAGNOSTIC_WORK,
            order: AUTHORING_DIAGNOSTIC_ORDER_V1.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct AuthoringModuleAnchorV1 {
    pub module_name: String,
    pub module_id: String,
    pub revision_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct AuthoringSourceAnchorV1 {
    pub workspace_relative_path: String,
    pub root_module: String,
    pub repository_id: String,
    /// This is the candidate compiler handle, not proof of AxiStore acceptance.
    pub compiled_snapshot_id: String,
    pub kernel_ir_digest: String,
    pub exact_root_axi_digest: String,
    pub ordered_module_closure: Vec<AuthoringModuleAnchorV1>,
    pub runtime_ir_ref_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AuthoringValidationV1 {
    pub canonical_axi_valid: bool,
    pub compiled_kernel_ir_valid: bool,
    pub finite_category_fragment_valid: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finite_theory_gate: Option<axiograph_kernel::FiniteTheoryGateReceiptIr>,
    pub runtime_theory_gate: AuthoringGateDecisionV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_theory: Option<crate::runtime_theory_check::RuntimeTheoryCheckModuleReportV1>,
    pub scope: String,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AuthoringCompetencyReportV1 {
    pub questions: Vec<crate::predictive_proposals::CompetencyQuestionV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluation: Option<crate::competency_questions::CompetencyCoverageWithTrustV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved_question_names: Vec<String>,
    pub promotion_gate: AuthoringGateDecisionV1,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct AuthoringCqHoleV1 {
    pub question_name: String,
    pub summary: String,
    pub expected_next_form: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct AuthoringTheoryHoleV1 {
    pub obligation_ref: axiograph_pathdb::kernel_ir::TheoryObligationRefIr,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subject_refs: Vec<axiograph_pathdb::kernel_ir::TheorySubjectRefIr>,
    pub runtime_status: axiograph_pathdb::RuntimeTheoryCheckStatusV1,
    pub lifecycle: axiograph_kernel::CheckedLifecycleStateIr,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_obligations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repair_handle_ids: Vec<String>,
    pub authority: String,
    pub lean_certification: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct AuthoringTypedHolesV1 {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub olog: Vec<crate::typed_authoring::OlogTypedHoleV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query: Vec<crate::axql::AxqlTypedHoleV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub competency_questions: Vec<AuthoringCqHoleV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub theory: Vec<AuthoringTheoryHoleV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct AuthoringDependentContextSummaryV1 {
    pub context_id: String,
    pub axis: axiograph_kernel::ScopeAxisIr,
    pub object: axiograph_kernel::SchemaObjectRefIr,
    pub value: axiograph_kernel::TypedValueIr,
    pub visible_scope_witnesses: usize,
    pub lifecycle: axiograph_kernel::CheckedLifecycleStateIr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct AuthoringDependentRefinementSummaryV1 {
    pub instance_id: String,
    pub object_membership_witnesses: usize,
    pub role_indexed_witnesses: usize,
    pub typed_constraint_witnesses: usize,
    pub contexts: Vec<AuthoringDependentContextSummaryV1>,
    pub lifecycle: axiograph_kernel::CheckedLifecycleStateIr,
    pub residual_obligations: Vec<axiograph_kernel::TheoryResidualObligationIr>,
    pub runtime_authority: String,
    pub lean_certification: String,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AuthoringQueryExplanationV1 {
    pub elaborated_query: String,
    pub elaborated_query_ir_v1: crate::query_ir::QueryIrV1,
    pub plan: Vec<String>,
    pub exploration: crate::query_ir::PreparedQueryExplorationV1,
    pub scope: String,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct FiniteEvolutionItemV1 {
    pub semantic_kind: String,
    pub stable_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_payload_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_payload_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct FiniteKernelEvolutionPreviewV1 {
    pub fragment: String,
    pub baseline_snapshot_id: String,
    pub candidate_snapshot_id: String,
    pub preserved_payloads: usize,
    pub changed_payloads: Vec<FiniteEvolutionItemV1>,
    pub added_payloads: Vec<FiniteEvolutionItemV1>,
    pub removed_payloads: Vec<FiniteEvolutionItemV1>,
    pub scope: String,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub(crate) enum AuthoringEvolutionPreviewV1 {
    FiniteKernel(FiniteKernelEvolutionPreviewV1),
    Olog(crate::evolution_preview::EvolutionPreviewV1),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AuthoringGateDecisionV1 {
    Passed,
    Blocked,
    NotEvaluated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct AuthoringPromotionGateV1 {
    pub gate: String,
    pub decision: AuthoringGateDecisionV1,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct AuthoringPromotionReviewV1 {
    pub candidate_reviewable: bool,
    pub protected_main_eligible: bool,
    pub gates: Vec<AuthoringPromotionGateV1>,
    pub blockers: Vec<String>,
    pub required_write_authority: String,
    pub scope: String,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct AuthoringTrustV1 {
    pub trust_class: String,
    pub authority: String,
    pub checked_fragment: String,
    pub trusted_checker_import_closure: String,
    pub completeness_claim: String,
    pub ontology_closure_claim: String,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AuthoringWorkspaceReportV1 {
    pub version: String,
    pub operation: AuthoringWorkspaceOperationV1,
    pub workspace_root: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<AuthoringSourceAnchorV1>,
    pub diagnostics: Vec<AuthoringDiagnosticV1>,
    pub diagnostic_collection: AuthoringDiagnosticCollectionV1,
    pub validation: AuthoringValidationV1,
    pub typed_holes: AuthoringTypedHolesV1,
    pub dependent_refinements: Vec<AuthoringDependentRefinementSummaryV1>,
    pub repairs: Vec<crate::typed_refinement::RuntimeRefinementCandidateV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub competency_questions: Option<AuthoringCompetencyReportV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepared_query: Option<crate::query_ir::PreparedQueryMetadataV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_explanation: Option<AuthoringQueryExplanationV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_query_repair: Option<crate::query_ir::QueryRefinementApplyResultV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_olog: Option<crate::typed_authoring::CheckedOlogFragmentV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_olog_repair: Option<crate::typed_authoring::OlogRefinementApplyResultV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evolution_previews: Vec<AuthoringEvolutionPreviewV1>,
    pub promotion: AuthoringPromotionReviewV1,
    pub stable_runtime_refs: Vec<RuntimeIrRef>,
    pub next_actions: Vec<String>,
    pub trust: AuthoringTrustV1,
}

struct CompiledWorkspaceSource {
    path: PathBuf,
    text: String,
    package: crate::axi_input::CanonicalAxiPackage,
    kernel: RuntimeModuleIndex,
    db: PathDB,
    meta: Option<MetaPlaneIndex>,
}

#[derive(Debug)]
struct WorkspaceQueryProjectionError {
    source: AuthoringSourceAnchorV1,
    cause: anyhow::Error,
}

impl std::fmt::Display for WorkspaceQueryProjectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.cause, f)
    }
}
impl std::error::Error for WorkspaceQueryProjectionError {}

#[derive(Debug, Clone)]
pub(crate) struct AuthoringWorkspaceService {
    root: PathBuf,
}

impl AuthoringWorkspaceService {
    pub(crate) fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = std::fs::canonicalize(root.as_ref()).with_context(|| {
            format!(
                "canonicalize authoring workspace `{}`",
                root.as_ref().display()
            )
        })?;
        let metadata = std::fs::symlink_metadata(&root)?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err(anyhow!(
                "authoring workspace must be a real directory, not a symlink"
            ));
        }
        Ok(Self { root })
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn execute(
        &self,
        request: AuthoringWorkspaceRequestV1,
    ) -> Result<AuthoringWorkspaceReportV1> {
        self.execute_bound(request, &mut Vec::new(), &BTreeMap::new())
    }

    /// Same as [`Self::execute`], but any import candidate whose canonicalized
    /// path matches a key of `import_overlays` compiles from that unsaved
    /// buffer instead of disk bytes, so an unsaved root can resolve against
    /// unsaved-but-open importee buffers (including importees never written
    /// to disk) as one exact overlay closure. Nothing is written to disk by
    /// this call.
    pub(crate) fn execute_with_import_overlays(
        &self,
        request: AuthoringWorkspaceRequestV1,
        import_overlays: &BTreeMap<PathBuf, Vec<u8>>,
    ) -> Result<AuthoringWorkspaceReportV1> {
        self.execute_bound(request, &mut Vec::new(), import_overlays)
    }

    fn execute_bound(
        &self,
        request: AuthoringWorkspaceRequestV1,
        input_anchors: &mut Vec<String>,
        import_overlays: &BTreeMap<PathBuf, Vec<u8>>,
    ) -> Result<AuthoringWorkspaceReportV1> {
        request.validate()?;
        let candidate_path = self.resolve_existing_file(&request.axi_path, "axi_path")?;
        let candidate_text = self.read_or_override(&candidate_path, request.axi_text.as_deref())?;
        input_anchors.push(projection::digest(candidate_text.as_bytes()));

        let mut report = empty_report(self, request.operation);
        let candidate = match self.compile_source(candidate_path, candidate_text, import_overlays) {
            Ok(candidate) => candidate,
            Err(error) => {
                let projection = error.downcast_ref::<WorkspaceQueryProjectionError>();
                if let Some(projection) = projection {
                    report.source = Some(projection.source.clone());
                    input_anchors.push(serde_json::to_string(&report.source)?);
                    report.validation.canonical_axi_valid = true;
                    report.validation.compiled_kernel_ir_valid = true;
                }
                if let Some(collection) = error.downcast_ref::<
                    crate::axi_input::diagnostics::CanonicalSourceDiagnosticCollection,
                >() {
                    append_canonical_diagnostic_collection(
                        &mut report,
                        collection,
                        "authoring",
                    );
                    report.promotion = promotion_review(&report, false, None);
                    report.next_actions = next_actions(&report);
                    return Ok(report);
                }
                let location = error
                    .downcast_ref::<crate::axi_input::diagnostics::CanonicalSourceDiagnostic>()
                    .and_then(|diagnostic| diagnostic.location.clone());
                report.diagnostics.push(AuthoringDiagnosticV1 {
                    severity: AuthoringDiagnosticSeverityV1::Error,
                    code: projection.map_or("authoring_canonical_compile_failed", |p| {
                        if p.cause.is::<axiograph_pathdb::axi_module_import::UnsupportedQueryProjection>() {
                            "authoring_query_projection_unsupported"
                        } else {
                            "authoring_query_projection_failed"
                        }
                    }).to_string(),
                    message: error.to_string(),
                    path: location.as_ref().map(|location| location.path.clone()),
                    line: location.as_ref().map(|location| location.start.line),
                    location,
                    repair_hint: Some(
                        if projection.is_some() {
                            "use a representable query projection (unique schema/theory/instance labels and supported execution types); canonical validity does not imply query projection support".to_string()
                        } else {
                            "repair the canonical `.axi` module/import closure before requesting typed authoring operations".to_string()
                        },
                    ),
                });
                refresh_diagnostic_collection(&mut report);
                report.promotion = promotion_review(&report, false, None);
                report.next_actions = next_actions(&report);
                return Ok(report);
            }
        };

        report.source = Some(source_anchor(self, &candidate)?);
        input_anchors.push(serde_json::to_string(&report.source)?);
        report.validation.canonical_axi_valid = true;
        report.validation.compiled_kernel_ir_valid = true;
        match candidate
            .package
            .snapshot()
            .finite_theory_gate_receipt(axiograph_kernel::FiniteTheoryGateConsumerIr::Authoring)
        {
            Ok(receipt) => {
                report.validation.finite_category_fragment_valid = receipt.passed;
                if !receipt.passed {
                    report.diagnostics.push(AuthoringDiagnosticV1 {
                        severity: AuthoringDiagnosticSeverityV1::Error,
                        location: None,
                        code: "authoring_finite_theory_residual".to_string(),
                        message: format!(
                            "canonical finite-theory gate has {} residual obligation(s)",
                            receipt.residual_obligations.len()
                        ),
                        path: Some(request.axi_path.clone()),
                        line: None,
                        repair_hint: Some(
                            "resolve the bounded finite-theory residuals before promotion"
                                .to_string(),
                        ),
                    });
                }
                report.validation.finite_theory_gate = Some(receipt);
            }
            Err(error) => report.diagnostics.push(AuthoringDiagnosticV1 {
                severity: AuthoringDiagnosticSeverityV1::Error,
                location: None,
                code: "authoring_finite_theory_replay_failed".to_string(),
                message: error.to_string(),
                path: Some(request.axi_path.clone()),
                line: None,
                repair_hint: Some(
                    "reject the candidate: its finite-theory evidence does not replay".to_string(),
                ),
            }),
        }
        report.stable_runtime_refs = candidate.kernel.runtime_semantic_index().refs;
        report.dependent_refinements = dependent_refinement_summaries(&candidate);

        if candidate.kernel.theories.is_empty() {
            report.validation.runtime_theory_gate = AuthoringGateDecisionV1::Passed;
        } else {
            match crate::runtime_theory_check::runtime_theory_check_reports_from_package(
                &candidate.package,
                None,
                RuntimeTheoryClosureTierV1::FiniteFragment,
                default_world_assumption_v1(),
                default_evidence_policy_v1(),
            ) {
                Ok(theory) => {
                    let blocked = theory.summary.blocking_errors > 0
                        || theory.summary.blocked_obligations > 0
                        || theory.summary.review_only_obligations > 0
                        || theory.summary.residual_obligations > 0
                        || !theory.summary.residual_obligation_ids.is_empty();
                    report.validation.runtime_theory_gate = if blocked {
                        AuthoringGateDecisionV1::Blocked
                    } else {
                        AuthoringGateDecisionV1::Passed
                    };
                    if blocked {
                        let hard_failure = theory.summary.blocking_errors > 0
                            || theory.summary.blocked_obligations > 0;
                        report.diagnostics.push(AuthoringDiagnosticV1 {
                            severity: if hard_failure {
                                AuthoringDiagnosticSeverityV1::Error
                            } else {
                                AuthoringDiagnosticSeverityV1::Warning
                            },
                            location: None,
                            code: "authoring_runtime_theory_blocked".to_string(),
                            message: format!(
                                "runtime finite-fragment theory review has {} blocker(s), {} review-only obligation(s), {} residual obligation(s), and unresolved ids [{}]",
                                theory.summary.blocked_obligations
                                    + theory.summary.blocking_errors,
                                theory.summary.review_only_obligations,
                                theory.summary.residual_obligations,
                                theory.summary.residual_obligation_ids.join(", ")
                            ),
                            path: Some(request.axi_path.clone()),
                            line: None,
                            repair_hint: Some(
                                "resolve every blocking/residual theory obligation or keep it explicit on a review branch"
                                    .to_string(),
                            ),
                        });
                    }
                    for judgment in theory
                        .reports
                        .iter()
                        .flat_map(|module| module.judgments.iter())
                        .filter(|judgment| {
                            judgment.status != axiograph_pathdb::RuntimeTheoryCheckStatusV1::Checked
                        })
                    {
                        let required_action = match judgment.status {
                            axiograph_pathdb::RuntimeTheoryCheckStatusV1::Blocked => {
                                "repair the canonical .axi endpoints, roles, or scope axes and recompile"
                            }
                            axiograph_pathdb::RuntimeTheoryCheckStatusV1::ReviewOnly => {
                                "replace the review-only source with a supported finite obligation or obtain a separate accepted certificate"
                            }
                            axiograph_pathdb::RuntimeTheoryCheckStatusV1::ResidualObligation => {
                                "supply the missing typed scope/transport evidence or retain the obligation explicitly on a review branch"
                            }
                            axiograph_pathdb::RuntimeTheoryCheckStatusV1::Checked => {
                                "no repair required"
                            }
                        }
                        .to_string();
                        let lifecycle = if judgment.status
                            == axiograph_pathdb::RuntimeTheoryCheckStatusV1::Blocked
                        {
                            axiograph_kernel::CheckedLifecycleStateIr::Rejected
                        } else {
                            axiograph_kernel::CheckedLifecycleStateIr::Residual
                        };
                        let repair = crate::typed_refinement::RuntimeRefinementCandidateV2::new_theory(
                            format!(
                                "address runtime theory obligation `{}`",
                                judgment.obligation_ref.stable_id()
                            ),
                            crate::typed_refinement::TheoryRefinementOpV1::AddressRuntimeTheoryObligation {
                                source_artifact_digest: candidate.package.snapshot().ir().ir_digest().to_string(),
                                expected_lifecycle: lifecycle,
                                obligation_ref: judgment.obligation_ref.clone(),
                                subject_refs: judgment.subject_refs.clone(),
                                status: judgment.status,
                                residual_obligations: judgment.residual_obligations.clone(),
                                required_action,
                            },
                            judgment.obligation_ref.clone(),
                            judgment.subject_refs.clone(),
                        );
                        let repair_handle_id = repair.handle.id.clone();
                        report.typed_holes.theory.push(AuthoringTheoryHoleV1 {
                            obligation_ref: judgment.obligation_ref.clone(),
                            subject_refs: judgment.subject_refs.clone(),
                            runtime_status: judgment.status,
                            lifecycle,
                            residual_obligations: judgment.residual_obligations.clone(),
                            repair_handle_ids: vec![repair_handle_id.clone()],
                            authority: "untrusted_rust_runtime_admissibility".to_string(),
                            lean_certification: "not_certified; VerifyMain acceptance requires a separate supported certificate"
                                .to_string(),
                        });
                        extend_unique_repairs(&mut report.repairs, vec![repair]);
                        report.diagnostics.push(AuthoringDiagnosticV1 {
                            severity: match judgment.status {
                                axiograph_pathdb::RuntimeTheoryCheckStatusV1::Blocked => {
                                    AuthoringDiagnosticSeverityV1::Error
                                }
                                axiograph_pathdb::RuntimeTheoryCheckStatusV1::ReviewOnly
                                | axiograph_pathdb::RuntimeTheoryCheckStatusV1::ResidualObligation => {
                                    AuthoringDiagnosticSeverityV1::Warning
                                }
                                axiograph_pathdb::RuntimeTheoryCheckStatusV1::Checked => {
                                    AuthoringDiagnosticSeverityV1::Information
                                }
                            },
                            location: None,
                            code: match judgment.status {
                                axiograph_pathdb::RuntimeTheoryCheckStatusV1::Checked => {
                                    "authoring_runtime_theory_checked"
                                }
                                axiograph_pathdb::RuntimeTheoryCheckStatusV1::ReviewOnly => {
                                    "authoring_runtime_theory_review_only"
                                }
                                axiograph_pathdb::RuntimeTheoryCheckStatusV1::ResidualObligation => {
                                    "authoring_runtime_theory_residual_obligation"
                                }
                                axiograph_pathdb::RuntimeTheoryCheckStatusV1::Blocked => {
                                    "authoring_runtime_theory_blocked_obligation"
                                }
                            }
                            .to_string(),
                            message: format!(
                                "theory obligation `{}` ({}) is {:?}: {}",
                                judgment.obligation_ref.stable_id(),
                                judgment.label,
                                judgment.status,
                                judgment.message
                            ),
                            path: Some(request.axi_path.clone()),
                            line: None,
                            repair_hint: Some(format!(
                                "apply or inspect repair handle `{repair_handle_id}` over typed subjects [{}] and residuals [{}] before requesting promotion; this handle is runtime guidance, not Lean certification",
                                judgment
                                    .subject_refs
                                    .iter()
                                    .map(|subject| subject.stable_id())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                                judgment.residual_obligations.join(", ")
                            )),
                        });
                    }
                    report.validation.runtime_theory = Some(theory);
                }
                Err(error) => {
                    report.validation.runtime_theory_gate = AuthoringGateDecisionV1::Blocked;
                    report.diagnostics.push(AuthoringDiagnosticV1 {
                        severity: AuthoringDiagnosticSeverityV1::Error,
                        location: None,
                        code: "authoring_runtime_theory_check_failed".to_string(),
                        message: error.to_string(),
                        path: Some(request.axi_path.clone()),
                        line: None,
                        repair_hint: Some(
                            "repair the compiled theory fragment before promotion review"
                                .to_string(),
                        ),
                    });
                }
            }
        }

        if let Some(baseline_path) = request.baseline_axi_path.as_deref() {
            let path = self.resolve_existing_file(baseline_path, "baseline_axi_path")?;
            let text = self.read_or_override(&path, request.baseline_axi_text.as_deref())?;
            match self.compile_source(path, text, &BTreeMap::new()) {
                Ok(baseline) => {
                    input_anchors.push(serde_json::to_string(&source_anchor(self, &baseline)?)?);
                    report
                        .evolution_previews
                        .push(AuthoringEvolutionPreviewV1::FiniteKernel(
                            finite_kernel_evolution_preview(&baseline, &candidate)?,
                        ))
                }
                Err(error) => {
                    let projection = error.downcast_ref::<WorkspaceQueryProjectionError>();
                    if let Some(projection) = projection {
                        input_anchors.push(serde_json::to_string(&projection.source)?);
                    }
                    if let Some(collection) = error.downcast_ref::<
                        crate::axi_input::diagnostics::CanonicalSourceDiagnosticCollection,
                    >() {
                        append_canonical_diagnostic_collection(
                            &mut report,
                            collection,
                            "authoring_baseline",
                        );
                    } else {
                        let location = error
                            .downcast_ref::<crate::axi_input::diagnostics::CanonicalSourceDiagnostic>()
                            .and_then(|diagnostic| diagnostic.location.clone());
                        report.diagnostics.push(AuthoringDiagnosticV1 {
                            severity: AuthoringDiagnosticSeverityV1::Error,
                            code: projection.map_or("authoring_baseline_compile_failed", |p| {
                                if p.cause.is::<axiograph_pathdb::axi_module_import::UnsupportedQueryProjection>() {
                                    "authoring_baseline_query_projection_unsupported"
                                } else {
                                    "authoring_baseline_query_projection_failed"
                                }
                            }).to_string(),
                            message: error.to_string(),
                            path: location.as_ref().map(|location| location.path.clone()),
                            line: location.as_ref().map(|location| location.start.line),
                            location,
                            repair_hint: Some(if projection.is_some() {
                                "use a baseline representable by the named query projection before requesting an evolution preview".to_string()
                            } else {
                                "repair the baseline module before requesting an evolution preview".to_string()
                            }),
                        });
                    }
                }
            }
        } else if request.baseline_axi_text.is_some() {
            return Err(anyhow!(
                "`baseline_axi_text` requires workspace-relative `baseline_axi_path`"
            ));
        }

        if let Some(fragment) = request.olog_fragment.clone() {
            self.check_olog(&candidate, &request, fragment, &mut report);
        }

        if let Some(query) = request.query_ir_v1.clone() {
            self.prepare_query(&candidate, &request, query, &mut report);
        }

        if request.cq_path.is_some() || request.cq_text.is_some() {
            self.check_competency_questions(&candidate, &request, &mut report, input_anchors)?;
        }

        refresh_diagnostic_collection(&mut report);
        report.ok = !report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == AuthoringDiagnosticSeverityV1::Error);
        let cq_gate = report
            .competency_questions
            .as_ref()
            .map(|competency| competency.promotion_gate);
        report.promotion = promotion_review(&report, true, cq_gate);
        report.next_actions = next_actions(&report);
        Ok(report)
    }

    fn resolve_existing_file(&self, raw: &str, field: &str) -> Result<PathBuf> {
        let relative = Path::new(raw);
        if relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(anyhow!(
                "{field} must be a workspace-relative path without `..`: `{raw}`"
            ));
        }
        let joined = self.root.join(relative);
        let resolved = std::fs::canonicalize(&joined)
            .with_context(|| format!("resolve {field} `{}`", joined.display()))?;
        if !resolved.starts_with(&self.root) {
            return Err(anyhow!("{field} escapes authoring workspace: `{raw}`"));
        }
        let metadata = std::fs::symlink_metadata(&resolved)?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(anyhow!("{field} must resolve to a regular file: `{raw}`"));
        }
        if metadata.len() > MAX_AUTHORING_SOURCE_BYTES {
            return Err(anyhow!(
                "{field} exceeds {MAX_AUTHORING_SOURCE_BYTES} bytes: `{raw}`"
            ));
        }
        Ok(resolved)
    }

    fn read_or_override(&self, path: &Path, override_text: Option<&str>) -> Result<String> {
        if let Some(text) = override_text {
            if text.len() as u64 > MAX_AUTHORING_SOURCE_BYTES {
                return Err(anyhow!(
                    "inline authoring source exceeds {MAX_AUTHORING_SOURCE_BYTES} bytes"
                ));
            }
            return Ok(text.to_string());
        }
        crate::security::read_utf8_file_bounded(
            path,
            crate::security::MAX_TEXT_INPUT_BYTES,
            "CLI input",
        )
        .with_context(|| format!("read authoring source `{}` as UTF-8", path.display()))
    }

    fn compile_source(
        &self,
        path: PathBuf,
        text: String,
        import_overlays: &BTreeMap<PathBuf, Vec<u8>>,
    ) -> Result<CompiledWorkspaceSource> {
        let package = crate::axi_input::compile_canonical_axi_path_with_overlays_collecting(
            &path,
            text.as_bytes().to_vec(),
            std::slice::from_ref(&self.root),
            import_overlays,
        )?;
        let sources = package.ordered_sources();
        let source = package_source_anchor(self, &path, &text, &package)?;
        let derived = axiograph_pathdb::axi_module_import::derive_package_query_index(
            package.snapshot(),
            &sources,
        )
        .map_err(|cause| {
            let mut source = source;
            source.runtime_ir_ref_count = 0;
            WorkspaceQueryProjectionError { source, cause }
        })?;
        let kernel = derived.kernel;
        let db = derived.db;
        let meta = Some(derived.meta);
        Ok(CompiledWorkspaceSource {
            path,
            text,
            package,
            kernel,
            db,
            meta,
        })
    }

    fn check_olog(
        &self,
        candidate: &CompiledWorkspaceSource,
        request: &AuthoringWorkspaceRequestV1,
        fragment: crate::typed_authoring::OlogFragmentV1,
        report: &mut AuthoringWorkspaceReportV1,
    ) {
        let schema = match select_runtime_schema(&candidate.kernel, request.schema.as_deref()) {
            Ok(schema) => schema,
            Err(error) => {
                report.diagnostics.push(AuthoringDiagnosticV1 {
                    severity: AuthoringDiagnosticSeverityV1::Error,
                    location: None,
                    code: "authoring_schema_selection_failed".to_string(),
                    message: error.to_string(),
                    path: Some(request.axi_path.clone()),
                    line: None,
                    repair_hint: Some(
                        "set `schema` to one compiled schema id or local schema name".to_string(),
                    ),
                });
                return;
            }
        };
        let theories = candidate
            .kernel
            .theories
            .iter()
            .filter(|theory| theory.schema_id == schema.schema_id)
            .cloned()
            .collect::<Vec<_>>();
        let checked = crate::typed_authoring::check_olog_fragment_against_compiled_semantics_ir(
            schema, &theories, fragment,
        );
        let (checked, applied) = match request.apply_refinement_handle_id.as_deref() {
            Some(handle_id) if request.query_ir_v1.is_none() => {
                match crate::typed_authoring::apply_runtime_refinement_by_id_to_olog_fragment_against_compiled_semantics_ir(
                    schema,
                    &theories,
                    &checked,
                    handle_id,
                ) {
                    Ok(applied) => (applied.checked_fragment.clone(), Some(applied)),
                    Err(error) => {
                        report.diagnostics.push(AuthoringDiagnosticV1 {
                            severity: AuthoringDiagnosticSeverityV1::Error,
                            location: None,
                            code: "authoring_olog_repair_failed".to_string(),
                            message: error.to_string(),
                            path: Some(request.axi_path.clone()),
                            line: None,
                            repair_hint: Some(
                                "choose a refinement handle emitted by this exact compiled snapshot and fragment"
                                    .to_string(),
                            ),
                        });
                        (checked, None)
                    }
                }
            }
            _ => (checked, None),
        };
        for diagnostic in &checked.diagnostics {
            report.diagnostics.push(AuthoringDiagnosticV1 {
                severity: match diagnostic.severity {
                    crate::typed_authoring::OlogDiagnosticSeverityV1::Error => {
                        AuthoringDiagnosticSeverityV1::Error
                    }
                    crate::typed_authoring::OlogDiagnosticSeverityV1::Warning => {
                        AuthoringDiagnosticSeverityV1::Warning
                    }
                },
                location: None,
                code: diagnostic.code.clone(),
                message: diagnostic.message.clone(),
                path: Some(request.axi_path.clone()),
                line: None,
                repair_hint: diagnostic.repair_hint.clone(),
            });
        }
        report.typed_holes.olog = checked.typed_holes.clone();
        extend_unique_repairs(&mut report.repairs, checked.refinement_candidates.clone());
        if let Some(preview) =
            crate::typed_authoring::build_olog_fragment_evolution_preview_against_compiled_semantics_ir(
                schema,
                &theories,
                &checked,
            )
        {
            report
                .evolution_previews
                .push(AuthoringEvolutionPreviewV1::Olog(preview));
        }
        report.applied_olog_repair = applied;
        report.checked_olog = Some(checked);
    }

    fn prepare_query(
        &self,
        candidate: &CompiledWorkspaceSource,
        request: &AuthoringWorkspaceRequestV1,
        query: crate::query_ir::QueryIrV1,
        report: &mut AuthoringWorkspaceReportV1,
    ) {
        let prepared = match query.compile_with_meta(&candidate.db, candidate.meta.as_ref()) {
            Ok(prepared) => prepared,
            Err(error) => {
                report.diagnostics.push(AuthoringDiagnosticV1 {
                    severity: AuthoringDiagnosticSeverityV1::Error,
                    location: None,
                    code: "authoring_query_prepare_failed".to_string(),
                    message: error.to_string(),
                    path: Some(request.axi_path.clone()),
                    line: None,
                    repair_hint: Some(
                        "repair the structured `query_ir_v1` against the compiled workspace schema"
                            .to_string(),
                    ),
                });
                return;
            }
        };
        let focus = request
            .focus_variable
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let exploration = prepared.exploration_view(focus);
        report.typed_holes.query = exploration.typed_holes.clone();
        extend_unique_repairs(
            &mut report.repairs,
            exploration.refinement_candidates.clone(),
        );
        match prepared.metadata_with_meta_and_kernel(candidate.meta.as_ref(), &candidate.kernel) {
            Ok(mut metadata) => match candidate
                .package
                .snapshot()
                .require_finite_theory_gate(axiograph_kernel::FiniteTheoryGateConsumerIr::Query)
            {
                Ok(receipt) => {
                    metadata.finite_theory_gate = Some(receipt);
                    report.prepared_query = Some(metadata);
                }
                Err(error) => report.diagnostics.push(AuthoringDiagnosticV1 {
                    severity: AuthoringDiagnosticSeverityV1::Error,
                    location: None,
                    code: "authoring_query_finite_theory_blocked".to_string(),
                    message: error.to_string(),
                    path: Some(request.axi_path.clone()),
                    line: None,
                    repair_hint: Some(
                        "resolve finite-theory residuals before preparing a canonical query"
                            .to_string(),
                    ),
                }),
            },
            Err(error) => report.diagnostics.push(AuthoringDiagnosticV1 {
                severity: AuthoringDiagnosticSeverityV1::Error,
                location: None,
                code: "authoring_query_metadata_failed".to_string(),
                message: error.to_string(),
                path: Some(request.axi_path.clone()),
                line: None,
                repair_hint: None,
            }),
        }
        match prepared.elaborated_query_ir_v1() {
            Ok(elaborated_query_ir_v1) => {
                report.query_explanation = Some(AuthoringQueryExplanationV1 {
                    elaborated_query: prepared.elaborated_query_text(),
                    elaborated_query_ir_v1,
                    plan: prepared.explain_plan_lines(),
                    exploration,
                    scope: "compiled_finite_query_over_workspace_runtime_index".to_string(),
                    non_claims: vec![
                        "query preparation and explanation are untrusted Rust runtime services"
                            .to_string(),
                        "no answer-set completeness or ontology-closure claim".to_string(),
                        "certifiability metadata is not a certificate or a Lean verification receipt"
                            .to_string(),
                    ],
                });
            }
            Err(error) => report.diagnostics.push(AuthoringDiagnosticV1 {
                severity: AuthoringDiagnosticSeverityV1::Error,
                location: None,
                code: "authoring_query_explanation_failed".to_string(),
                message: error.to_string(),
                path: Some(request.axi_path.clone()),
                line: None,
                repair_hint: None,
            }),
        }
        if let Some(handle_id) = request.apply_refinement_handle_id.as_deref() {
            match prepared.apply_runtime_refinement_by_id(
                &candidate.db,
                candidate.meta.as_ref(),
                handle_id,
            ) {
                Ok(applied) => report.applied_query_repair = Some(applied),
                Err(error) => report.diagnostics.push(AuthoringDiagnosticV1 {
                    severity: AuthoringDiagnosticSeverityV1::Error,
                    location: None,
                    code: "authoring_query_repair_failed".to_string(),
                    message: error.to_string(),
                    path: Some(request.axi_path.clone()),
                    line: None,
                    repair_hint: Some(
                        "choose a query refinement handle emitted by this exact prepared query"
                            .to_string(),
                    ),
                }),
            }
        }
    }

    fn check_competency_questions(
        &self,
        candidate: &CompiledWorkspaceSource,
        request: &AuthoringWorkspaceRequestV1,
        report: &mut AuthoringWorkspaceReportV1,
        input_anchors: &mut Vec<String>,
    ) -> Result<()> {
        let (cq_text, cq_path) = match (request.cq_text.as_deref(), request.cq_path.as_deref()) {
            (Some(text), path) => (text.to_string(), path.map(str::to_string)),
            (None, Some(path)) => {
                let resolved = self.resolve_existing_file(path, "cq_path")?;
                (
                    self.read_or_override(&resolved, None)?,
                    Some(path.to_string()),
                )
            }
            (None, None) => return Ok(()),
        };
        if cq_text.len() as u64 > MAX_AUTHORING_SOURCE_BYTES {
            return Err(anyhow!(
                "inline competency source exceeds authoring source limit"
            ));
        }
        input_anchors.push(projection::digest(cq_text.as_bytes()));
        let questions = match crate::predictive_proposals::parse_competency_question_text(&cq_text)
        {
            Ok(questions) => questions,
            Err(error) => {
                report.diagnostics.push(AuthoringDiagnosticV1 {
                    severity: AuthoringDiagnosticSeverityV1::Error,
                    location: None,
                    code: "authoring_cq_parse_failed".to_string(),
                    message: error.to_string(),
                    path: cq_path,
                    line: line_from_error(&error.to_string()),
                    repair_hint: Some(
                        "use the canonical competency_question_bundle_v1 `.cq` grammar".to_string(),
                    ),
                });
                return Ok(());
            }
        };
        let unresolved = questions
            .iter()
            .filter(|question| question.query.trim().is_empty())
            .map(|question| question.name.clone())
            .collect::<Vec<_>>();
        report.typed_holes.competency_questions = unresolved
            .iter()
            .map(|name| AuthoringCqHoleV1 {
                question_name: name.clone(),
                summary: "authored competency question has no executable typed lowering"
                    .to_string(),
                expected_next_form:
                    "add `expect: exists Schema.Rel(...)`, `expect: instance of Schema.Type`, or an explicit `axql:` debug lowering"
                        .to_string(),
            })
            .collect();
        for name in &unresolved {
            report.diagnostics.push(AuthoringDiagnosticV1 {
                severity: AuthoringDiagnosticSeverityV1::Error,
                location: None,
                code: "authoring_cq_unresolved".to_string(),
                message: format!(
                    "competency question `{name}` remains an unresolved authoring obligation"
                ),
                path: cq_path.clone(),
                line: line_for_cq_question(&cq_text, name),
                repair_hint: Some(
                    "lower the question to one prepared typed query before promotion review"
                        .to_string(),
                ),
            });
        }
        let evaluation = match crate::competency_questions::evaluate_competency_questions_with_trust(
            &candidate.db,
            &questions,
        ) {
            Ok(evaluation) => {
                for question in &evaluation.questions {
                    extend_unique_repairs(
                        &mut report.repairs,
                        question.refinement_candidates.clone(),
                    );
                }
                Some(evaluation)
            }
            Err(error) => {
                report.diagnostics.push(AuthoringDiagnosticV1 {
                    severity: AuthoringDiagnosticSeverityV1::Error,
                    location: None,
                    code: "authoring_cq_evaluation_failed".to_string(),
                    message: error.to_string(),
                    path: cq_path.clone(),
                    line: None,
                    repair_hint: Some(
                        "repair every CQ lowering against the compiled workspace query service"
                            .to_string(),
                    ),
                });
                None
            }
        };
        let promotion_gate = evaluation
            .as_ref()
            .filter(|_| unresolved.is_empty())
            .map(|evaluation| {
                if evaluation.total > 0 && evaluation.satisfied == evaluation.total {
                    AuthoringGateDecisionV1::Passed
                } else {
                    AuthoringGateDecisionV1::Blocked
                }
            })
            .unwrap_or(AuthoringGateDecisionV1::Blocked);
        report.competency_questions = Some(AuthoringCompetencyReportV1 {
            questions,
            evaluation,
            unresolved_question_names: unresolved,
            promotion_gate,
            non_claims: vec![
                "CQ evaluation is finite execution over the candidate workspace runtime index"
                    .to_string(),
                "CQ satisfaction is not proof of ontology completeness or correctness".to_string(),
            ],
        });
        Ok(())
    }
}

fn append_canonical_diagnostic_collection(
    report: &mut AuthoringWorkspaceReportV1,
    collection: &crate::axi_input::diagnostics::CanonicalSourceDiagnosticCollection,
    code_prefix: &str,
) {
    let errors_before = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == AuthoringDiagnosticSeverityV1::Error)
        .count();
    let diagnostics_before = report.diagnostics.len();
    let message_bytes_before = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == AuthoringDiagnosticSeverityV1::Error)
        .map(|diagnostic| diagnostic.message.len())
        .sum::<usize>();
    for diagnostic in &collection.diagnostics {
        let location = diagnostic.location.clone();
        let code = match &diagnostic.cause {
            axiograph_kernel::KernelCompileError::Parse { .. } => {
                format!("{code_prefix}_canonical_parse_failed")
            }
            axiograph_kernel::KernelCompileError::ImportResolution { .. }
            | axiograph_kernel::KernelCompileError::UnknownImport { .. }
            | axiograph_kernel::KernelCompileError::ImportCycle(_) => {
                format!("{code_prefix}_canonical_import_failed")
            }
            axiograph_kernel::KernelCompileError::RoleCarrier { .. } => {
                format!("{code_prefix}_canonical_type_failed")
            }
            _ => format!("{code_prefix}_canonical_compile_failed"),
        };
        let repair_hint = location
            .as_ref()
            .and_then(|location| location.suggested_name.as_deref())
            .map_or_else(
                || {
                    "repair this exact canonical source occurrence, or inspect the explicitly unlocated compiler error; suggestions are advisory and do not write or promote"
                        .to_string()
                },
                |suggestion| {
                    format!(
                        "advisory namespace-local candidate `{suggestion}`; review the exact source image before editing"
                    )
                },
            );
        report.diagnostics.push(AuthoringDiagnosticV1 {
            severity: AuthoringDiagnosticSeverityV1::Error,
            code,
            message: diagnostic.cause.to_string(),
            path: location.as_ref().map(|location| location.path.clone()),
            line: location.as_ref().map(|location| location.start.line),
            repair_hint: Some(repair_hint),
            location,
        });
    }
    if report.diagnostics.len() == diagnostics_before {
        report.diagnostics.push(AuthoringDiagnosticV1 {
            severity: AuthoringDiagnosticSeverityV1::Error,
            code: format!("{code_prefix}_canonical_diagnostic_bound"),
            message: "canonical compilation failed and the bounded collector retained no displayable item"
                .to_string(),
            path: None,
            line: None,
            repair_hint: Some(
                "inspect the source under the reported item/byte/work ceilings; do not treat truncation as validation"
                    .to_string(),
            ),
            location: None,
        });
    }
    let returned_errors = errors_before.saturating_add(collection.diagnostics.len());
    let observed_errors = errors_before.saturating_add(collection.observed_errors.max(1));
    report.diagnostic_collection = AuthoringDiagnosticCollectionV1 {
        observed_errors,
        returned_errors,
        omitted_observed_errors: observed_errors.saturating_sub(returned_errors),
        message_bytes: message_bytes_before.saturating_add(
            collection
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.cause.to_string().len())
                .sum::<usize>(),
        ),
        work_units: collection.work_units,
        work_exhausted: collection.work_exhausted,
        truncated: collection.truncated(),
        ..AuthoringDiagnosticCollectionV1::default()
    };
}

fn refresh_diagnostic_collection(report: &mut AuthoringWorkspaceReportV1) {
    let prior_omitted = report.diagnostic_collection.omitted_observed_errors;
    let mut retained_errors = 0_usize;
    let mut message_bytes = 0_usize;
    let mut newly_omitted = 0_usize;
    report.diagnostics.retain(|diagnostic| {
        if diagnostic.severity != AuthoringDiagnosticSeverityV1::Error
            || diagnostic.code.ends_with("_diagnostic_bound")
        {
            return true;
        }
        let next_bytes = message_bytes.saturating_add(diagnostic.message.len());
        if retained_errors >= axiograph_kernel::MAX_KERNEL_DIAGNOSTIC_ITEMS
            || next_bytes > axiograph_kernel::MAX_KERNEL_DIAGNOSTIC_MESSAGE_BYTES
        {
            newly_omitted = newly_omitted.saturating_add(1);
            return false;
        }
        retained_errors += 1;
        message_bytes = next_bytes;
        true
    });
    if newly_omitted > 0
        && !report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.ends_with("_diagnostic_bound"))
    {
        report.diagnostics.push(AuthoringDiagnosticV1 {
            severity: AuthoringDiagnosticSeverityV1::Error,
            code: "authoring_diagnostic_bound".to_string(),
            message: "additional authoring errors were omitted at the hard item or message-byte ceiling"
                .to_string(),
            path: None,
            line: None,
            repair_hint: Some(
                "inspect the retained errors and repair without treating bounded output as validation"
                    .to_string(),
            ),
            location: None,
        });
    }
    let omitted_observed_errors = prior_omitted.saturating_add(newly_omitted);
    report.diagnostic_collection.observed_errors =
        retained_errors.saturating_add(omitted_observed_errors);
    report.diagnostic_collection.returned_errors = retained_errors;
    report.diagnostic_collection.omitted_observed_errors = omitted_observed_errors;
    report.diagnostic_collection.message_bytes = message_bytes;
    report.diagnostic_collection.truncated =
        omitted_observed_errors > 0 || report.diagnostic_collection.work_exhausted;
}

fn empty_report(
    service: &AuthoringWorkspaceService,
    operation: AuthoringWorkspaceOperationV1,
) -> AuthoringWorkspaceReportV1 {
    AuthoringWorkspaceReportV1 {
        version: AUTHORING_WORKSPACE_REPORT_VERSION_V1.to_string(),
        operation,
        workspace_root: service.root.display().to_string(),
        ok: false,
        source: None,
        diagnostics: Vec::new(),
        diagnostic_collection: AuthoringDiagnosticCollectionV1::default(),
        validation: AuthoringValidationV1 {
            canonical_axi_valid: false,
            compiled_kernel_ir_valid: false,
            finite_category_fragment_valid: false,
            finite_theory_gate: None,
            runtime_theory_gate: AuthoringGateDecisionV1::NotEvaluated,
            runtime_theory: None,
            scope: "finite canonical compiler and runtime authoring fragment".to_string(),
            non_claims: authoring_non_claims(),
        },
        typed_holes: AuthoringTypedHolesV1::default(),
        dependent_refinements: Vec::new(),
        repairs: Vec::new(),
        competency_questions: None,
        prepared_query: None,
        query_explanation: None,
        applied_query_repair: None,
        checked_olog: None,
        applied_olog_repair: None,
        evolution_previews: Vec::new(),
        promotion: AuthoringPromotionReviewV1 {
            candidate_reviewable: false,
            protected_main_eligible: false,
            gates: Vec::new(),
            blockers: vec!["canonical validation has not run".to_string()],
            required_write_authority: "AxiStore::promote with a complete PromotionPlan"
                .to_string(),
            scope: "read_only_promotion_review".to_string(),
            non_claims: authoring_non_claims(),
        },
        stable_runtime_refs: Vec::new(),
        next_actions: Vec::new(),
        trust: AuthoringTrustV1 {
            trust_class: "untrusted_runtime_authoring".to_string(),
            authority: "canonical .axi exact bytes plus compiled kernel IR".to_string(),
            checked_fragment: "finite categorical formation, runtime refinements, finite CQ/query execution, and endpoint-safe path authoring"
                .to_string(),
            trusted_checker_import_closure: "lean/Axiograph/VerifyMain.lean".to_string(),
            completeness_claim: "not_claimed".to_string(),
            ontology_closure_claim: "not_claimed".to_string(),
            non_claims: authoring_non_claims(),
        },
    }
}

fn dependent_refinement_summaries(
    candidate: &CompiledWorkspaceSource,
) -> Vec<AuthoringDependentRefinementSummaryV1> {
    candidate
        .package
        .snapshot()
        .ir()
        .instances()
        .iter()
        .map(|instance| AuthoringDependentRefinementSummaryV1 {
            instance_id: instance.instance_id.to_string(),
            object_membership_witnesses: instance.object_membership_witnesses.len(),
            role_indexed_witnesses: instance.role_witnesses.len(),
            typed_constraint_witnesses: instance.typed_constraint_witnesses.len(),
            contexts: instance
                .dependent_contexts
                .iter()
                .map(|context| AuthoringDependentContextSummaryV1 {
                    context_id: context.context_id.clone(),
                    axis: context.axis,
                    object: context.object.clone(),
                    value: context.value.clone(),
                    visible_scope_witnesses: context.scope_witnesses.len(),
                    lifecycle: context.lifecycle,
                })
                .collect(),
            lifecycle: instance.lifecycle,
            residual_obligations: instance.residual_obligations.clone(),
            runtime_authority: "canonical_compiler_finite_decision_procedure".to_string(),
            lean_certification: "not_certified_by_category_kernel_v3".to_string(),
            non_claims: vec![
                "object membership, role fibers, finite constraints, and contexts are replayable Rust witnesses, not Lean proofs".to_string(),
                "the summary covers only the exact compiled finite instance".to_string(),
            ],
        })
        .collect()
}

fn source_anchor(
    service: &AuthoringWorkspaceService,
    candidate: &CompiledWorkspaceSource,
) -> Result<AuthoringSourceAnchorV1> {
    package_source_anchor(
        service,
        &candidate.path,
        &candidate.text,
        &candidate.package,
    )
}

fn package_source_anchor(
    service: &AuthoringWorkspaceService,
    path: &Path,
    text: &str,
    package: &crate::axi_input::CanonicalAxiPackage,
) -> Result<AuthoringSourceAnchorV1> {
    let ir = package.snapshot().ir();
    let root_module = package.root_source().parsed().module_name.clone();
    let path = path
        .strip_prefix(service.root())
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();
    Ok(AuthoringSourceAnchorV1 {
        workspace_relative_path: path,
        root_module,
        repository_id: ir.repository_id().to_string(),
        compiled_snapshot_id: ir.accepted_snapshot_id().to_string(),
        kernel_ir_digest: ir.ir_digest().to_string(),
        exact_root_axi_digest: AxiDigest::from_axi_text(text).to_string(),
        ordered_module_closure: ir
            .ordered_module_closure()
            .iter()
            .map(|module| AuthoringModuleAnchorV1 {
                module_name: module.module_name.clone(),
                module_id: module.module_id.to_string(),
                revision_digest: module.revision.to_string(),
            })
            .collect(),
        runtime_ir_ref_count: ir.refs().len(),
    })
}

fn select_runtime_schema<'a>(
    kernel: &'a RuntimeModuleIndex,
    requested: Option<&str>,
) -> Result<&'a RuntimeSchemaIndex> {
    if let Some(requested) = requested {
        return kernel
            .schemas
            .iter()
            .find(|schema| {
                schema.schema_id.as_str() == requested
                    || schema
                        .schema_id
                        .as_str()
                        .rsplit_once(':')
                        .is_some_and(|(_, local)| local == requested)
            })
            .ok_or_else(|| anyhow!("unknown compiled schema `{requested}`"));
    }
    match kernel.schemas.as_slice() {
        [schema] => Ok(schema),
        [] => Err(anyhow!("compiled workspace contains no schema")),
        schemas => Err(anyhow!(
            "compiled workspace contains {} schemas; request one explicitly",
            schemas.len()
        )),
    }
}

fn finite_kernel_evolution_preview(
    baseline: &CompiledWorkspaceSource,
    candidate: &CompiledWorkspaceSource,
) -> Result<FiniteKernelEvolutionPreviewV1> {
    let baseline_ir = baseline.package.snapshot().ir();
    let candidate_ir = candidate.package.snapshot().ir();
    let baseline_payloads = payload_map(baseline_ir.payload_fingerprints_v2()?)?;
    let candidate_payloads = payload_map(candidate_ir.payload_fingerprints_v2()?)?;
    let keys = baseline_payloads
        .keys()
        .chain(candidate_payloads.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut preserved = 0usize;
    let mut changed = Vec::new();
    let mut added = Vec::new();
    let mut removed = Vec::new();
    for key in keys {
        let baseline_value = baseline_payloads.get(&key);
        let candidate_value = candidate_payloads.get(&key);
        let item = FiniteEvolutionItemV1 {
            semantic_kind: key.0.clone(),
            stable_id: key.1.clone(),
            baseline_payload_fingerprint: baseline_value.cloned(),
            candidate_payload_fingerprint: candidate_value.cloned(),
        };
        match (baseline_value, candidate_value) {
            (Some(left), Some(right)) if left == right => preserved += 1,
            (Some(_), Some(_)) => changed.push(item),
            (None, Some(_)) => added.push(item),
            (Some(_), None) => removed.push(item),
            (None, None) => {
                return Err(anyhow!(
                    "semantic payload key `{}`/`{}` disappeared from both evolution inputs",
                    key.0,
                    key.1
                ));
            }
        }
    }
    Ok(FiniteKernelEvolutionPreviewV1 {
        fragment: "finite_compiled_kernel_payload_diff_v2".to_string(),
        baseline_snapshot_id: baseline_ir.accepted_snapshot_id().to_string(),
        candidate_snapshot_id: candidate_ir.accepted_snapshot_id().to_string(),
        preserved_payloads: preserved,
        changed_payloads: changed,
        added_payloads: added,
        removed_payloads: removed,
        scope: "exact finite payload fingerprints for canonical compiled modules, schemas, relation objects, roles, theories, equations, rewrites, instances, and facts"
            .to_string(),
        non_claims: vec![
            "payload equality is equality of the encoded finite compiled IR payload only"
                .to_string(),
            "no categorical equivalence, naturality, univalence, higher-path equality, transport completeness, or ontology closure is claimed"
                .to_string(),
            "this Rust diff is review evidence, not a VerifyMain theorem".to_string(),
        ],
    })
}

fn payload_map(
    payloads: Vec<KernelPayloadFingerprintV2>,
) -> Result<BTreeMap<(String, String), String>> {
    let mut out = BTreeMap::new();
    for payload in payloads {
        let key = kernel_ref_logical_key(&payload.semantic_ref);
        if out
            .insert(key.clone(), payload.payload_fingerprint.to_string())
            .is_some()
        {
            return Err(anyhow!(
                "compiled kernel payload index repeats logical ref {}:{}",
                key.0,
                key.1
            ));
        }
    }
    Ok(out)
}

fn kernel_ref_logical_key(reference: &KernelRefV2) -> (String, String) {
    match reference {
        KernelRefV2::Module { module_id, .. } => ("module".to_string(), module_id.to_string()),
        KernelRefV2::Schema { schema_id, .. } => ("schema".to_string(), schema_id.to_string()),
        KernelRefV2::ObjectType { object_type_id, .. } => {
            ("object_type".to_string(), object_type_id.to_string())
        }
        KernelRefV2::Relation { relation_id, .. } => {
            ("relation_object".to_string(), relation_id.to_string())
        }
        KernelRefV2::Role { role_id, .. } => ("role".to_string(), role_id.to_string()),
        KernelRefV2::Generator { semantic_key, .. } => {
            ("schema_generator".to_string(), semantic_key.to_string())
        }
        KernelRefV2::Theory { theory_id, .. } => ("theory".to_string(), theory_id.to_string()),
        KernelRefV2::Instance { instance_id, .. } => {
            ("instance_model".to_string(), instance_id.to_string())
        }
        KernelRefV2::Constraint { constraint_id, .. } => {
            ("constraint".to_string(), constraint_id.to_string())
        }
        KernelRefV2::Equation { equation_id, .. } => {
            ("path_equation".to_string(), equation_id.to_string())
        }
        KernelRefV2::RewriteRule {
            rewrite_rule_id, ..
        } => ("rewrite_rule".to_string(), rewrite_rule_id.to_string()),
        KernelRefV2::Fact { fact_id, .. } => ("fact".to_string(), fact_id.to_string()),
    }
}

fn extend_unique_repairs(
    target: &mut Vec<crate::typed_refinement::RuntimeRefinementCandidateV2>,
    candidates: Vec<crate::typed_refinement::RuntimeRefinementCandidateV2>,
) {
    let mut ids = target
        .iter()
        .map(|candidate| candidate.handle.id.clone())
        .collect::<BTreeSet<_>>();
    for candidate in candidates {
        if ids.insert(candidate.handle.id.clone()) {
            target.push(candidate);
        }
    }
}

fn promotion_review(
    report: &AuthoringWorkspaceReportV1,
    compiled: bool,
    cq_gate: Option<AuthoringGateDecisionV1>,
) -> AuthoringPromotionReviewV1 {
    let canonical_gate = if compiled && report.validation.canonical_axi_valid {
        AuthoringGateDecisionV1::Passed
    } else {
        AuthoringGateDecisionV1::Blocked
    };
    let cq_gate = cq_gate.unwrap_or(AuthoringGateDecisionV1::Blocked);
    let trust_gate = AuthoringGateDecisionV1::Blocked;
    let theory_gate = report.validation.runtime_theory_gate;
    let mut blockers = Vec::new();
    if canonical_gate != AuthoringGateDecisionV1::Passed {
        blockers.push("canonical validation/compiled IR gate did not pass".to_string());
    }
    if cq_gate != AuthoringGateDecisionV1::Passed {
        blockers.push(
            "an explicit CQ bundle with every required question satisfied is absent or blocked"
                .to_string(),
        );
    }
    if theory_gate != AuthoringGateDecisionV1::Passed {
        blockers.push("runtime finite-fragment theory gate did not pass".to_string());
    }
    blockers.push(
        "no trusted-checker receipt from the VerifyMain import closure is attached".to_string(),
    );
    blockers.push(
        "read-only authoring adapters do not construct or execute an AxiStore PromotionPlan"
            .to_string(),
    );
    let has_runtime_error = report
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == AuthoringDiagnosticSeverityV1::Error);
    AuthoringPromotionReviewV1 {
        candidate_reviewable: compiled && !has_runtime_error,
        protected_main_eligible: false,
        gates: vec![
            AuthoringPromotionGateV1 {
                gate: "canonical_validation".to_string(),
                decision: canonical_gate,
                detail: "exact workspace `.axi` import closure compiled through CanonicalCompiler"
                    .to_string(),
            },
            AuthoringPromotionGateV1 {
                gate: "competency_questions".to_string(),
                decision: cq_gate,
                detail: "finite prepared-query CQ execution; an omitted CQ bundle fails closed"
                    .to_string(),
            },
            AuthoringPromotionGateV1 {
                gate: "runtime_theory".to_string(),
                decision: theory_gate,
                detail: "finite_fragment runtime admissibility with residuals failing closed"
                    .to_string(),
            },
            AuthoringPromotionGateV1 {
                gate: "trusted_checker".to_string(),
                decision: trust_gate,
                detail: "must be supplied by the Lean VerifyMain checker boundary".to_string(),
            },
        ],
        blockers,
        required_write_authority: "AxiStore::promote(expected_generation, PromotionPlan)"
            .to_string(),
        scope: "read_only_fail_closed_promotion_review".to_string(),
        non_claims: authoring_non_claims(),
    }
}

fn next_actions(report: &AuthoringWorkspaceReportV1) -> Vec<String> {
    let mut actions = Vec::new();
    if !report.validation.canonical_axi_valid {
        actions
            .push("repair canonical `.axi` diagnostics and rerun workspace validation".to_string());
    }
    if !report.typed_holes.olog.is_empty()
        || !report.typed_holes.query.is_empty()
        || !report.typed_holes.competency_questions.is_empty()
    {
        actions.push(
            "apply one emitted typed refinement handle, then re-run the exact workspace request"
                .to_string(),
        );
    }
    if report
        .competency_questions
        .as_ref()
        .is_some_and(|cq| cq.promotion_gate != AuthoringGateDecisionV1::Passed)
    {
        actions.push("repair and satisfy every required competency question".to_string());
    }
    if report.validation.runtime_theory_gate == AuthoringGateDecisionV1::Blocked {
        actions.push("resolve runtime theory blockers and residual obligations".to_string());
    }
    if report.operation == AuthoringWorkspaceOperationV1::PromotionReview {
        actions.push(
            "obtain a trusted VerifyMain receipt and submit a complete typed PromotionPlan through AxiStore"
                .to_string(),
        );
    }
    if actions.is_empty() {
        actions.push(
            "review the finite preview; use PromotionReview before constructing an AxiStore plan"
                .to_string(),
        );
    }
    actions
}

fn authoring_non_claims() -> Vec<String> {
    vec![
        "Rust authoring checks are not Lean proofs and are outside the VerifyMain trusted import closure"
            .to_string(),
        "the finite category/refinement/path fragment is not general dependent type theory, HoTT, univalence, or higher-category semantics"
            .to_string(),
        "runtime diagnostics, CQs, explanations, and previews claim neither completeness nor ontology closure"
            .to_string(),
        "PathDB and runtime indexes are derived execution substrates, not the meaning plane"
            .to_string(),
    ]
}

fn line_from_error(message: &str) -> Option<usize> {
    let marker = "line ";
    let start = message.find(marker)? + marker.len();
    let digits = message[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    digits.parse::<usize>().ok()
}

fn line_for_cq_question(text: &str, name: &str) -> Option<usize> {
    let expected = format!("question {name}:");
    text.lines()
        .position(|line| line.trim() == expected)
        .map(|line| line + 1)
}

pub(crate) fn authoring_workspace_request_schema_v1() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["version", "axi_path"],
        "properties": {
            "version": { "const": AUTHORING_WORKSPACE_REQUEST_VERSION_V1 },
            "presentation": projection::presentation_schema(),
            "operation": { "enum": ["inspect", "apply_repair", "validate", "promotion_review"], "default": "inspect" },
            "axi_path": { "type": "string", "description": "Workspace-relative canonical .axi root" },
            "axi_text": { "type": "string", "description": "Optional unsaved root-buffer replacement" },
            "baseline_axi_path": { "type": "string" },
            "baseline_axi_text": { "type": "string" },
            "cq_path": { "type": "string" },
            "cq_text": { "type": "string" },
            "schema": { "type": "string" },
            "olog_fragment": { "type": "object" },
            "query_ir_v1": crate::query_ir::query_ir_v1_json_schema(),
            "focus_variable": { "type": "string" },
            "apply_refinement_handle_id": { "type": "string" }
        }
    })
}

pub(crate) fn authoring_workspace_capabilities_v1() -> Value {
    json!({
        "version": "authoring_workspace_capabilities_v1",
        "request": AUTHORING_WORKSPACE_REQUEST_VERSION_V1,
        "report": "authoring_workspace_response_v1 (summary/standard); authoring_workspace_report_v1 (explicit full)",
        "presentation": projection::presentation_schema(),
        "operations": ["inspect", "apply_repair", "validate", "promotion_review"],
        "services": [
            "diagnostics", "typed_holes", "typed_repairs", "competency_questions",
            "query_preparation", "query_explanations", "finite_evolution_preview",
            "runtime_validation", "promotion_review"
        ],
        "lsp": { "command": AUTHORING_WORKSPACE_LSP_COMMAND, "write_capable": false },
        "mcp": { "tool": AUTHORING_WORKSPACE_TOOL_NAME, "write_capable": false },
        "http": { "endpoint": "POST /authoring", "write_capable": false },
        "promotion": {
            "adapter_mode": "read_only_review",
            "write_authority": "AxiStore::promote",
            "protected_main_fails_closed_without_verify_main_receipt": true
        },
        "trusted_checker": "lean/Axiograph/VerifyMain.lean",
        "non_claims": authoring_non_claims()
    })
}

pub(crate) fn authoring_workspace_integration_manifest_v1(workspace: &Path) -> Value {
    let workspace = workspace.to_string_lossy();
    json!({
        "version": "authoring_workspace_integration_manifest_v1",
        "lsp": {
            "command": "axiograph",
            "args": ["authoring", "lsp", "--workspace", workspace]
        },
        "mcp": {
            "command": "axiograph",
            "args": ["authoring", "mcp", "--workspace", workspace]
        },
        "http": {
            "command": "axiograph",
            "args": ["authoring", "serve", "--workspace", workspace]
        },
        "request_contract": AUTHORING_WORKSPACE_REQUEST_VERSION_V1,
        "report_contract": "authoring_workspace_response_v1",
        "full_report_contract": AUTHORING_WORKSPACE_REPORT_VERSION_V1,
        "mutations": "none; accepted-state writes require an explicit AxiStore client"
    })
}

pub(crate) fn read_authoring_request(path: &Path) -> Result<AuthoringWorkspaceRequestV1> {
    let bytes = crate::security::read_file_bounded(
        path,
        MAX_AUTHORING_BODY_BYTES,
        "authoring workspace request",
    )?;
    crate::security::parse_json_bounded(
        &bytes,
        MAX_AUTHORING_BODY_BYTES,
        "AuthoringWorkspaceRequestV1",
    )
}

// --- MCP adapter ---------------------------------------------------------

#[derive(Clone)]
struct AuthoringWorkspaceRmcpServer {
    service: AuthoringWorkspaceService,
}

impl rmcp::handler::server::ServerHandler for AuthoringWorkspaceRmcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "axiograph-authoring-workspace",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Read-only workspace authoring. Defaults to compact authoring_workspace_response_v1; presentation.detail=full returns the canonical authoring_workspace_report_v1. Cursors are consistency checks, never authority.",
            )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(vec![authoring_rmcp_tool()]))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResponse, ErrorData> {
        Ok(execute_mcp_payload(&self.service, request).into())
    }
}

fn execute_mcp_payload(
    service: &AuthoringWorkspaceService,
    request: CallToolRequestParams,
) -> CallToolResult {
    let result = (|| -> Result<Value> {
        if request.name.as_ref() != AUTHORING_WORKSPACE_TOOL_NAME {
            return Err(anyhow!("unknown authoring tool `{}`", request.name));
        }
        let request = Value::Object(request.arguments.unwrap_or_default());
        let request: AuthoringWorkspaceRequestV1 =
            serde_json::from_value(request).context("invalid authoring request")?;
        Ok(serde_json::to_value(service.execute_response(request)?)?)
    })();
    match result {
        Ok(value) => CallToolResult::structured(value),
        Err(error) => CallToolResult::structured_error(json!({"error": format!("{error:#}")})),
    }
}

fn authoring_rmcp_tool() -> Tool {
    let schema = authoring_workspace_request_schema_v1()
        .as_object()
        .cloned()
        .unwrap_or_else(JsonObject::default);
    Tool::new(
        AUTHORING_WORKSPACE_TOOL_NAME,
        "Run diagnostics, typed holes/repairs, CQs, query preparation/explanations, finite evolution preview, validation, and fail-closed promotion review through one workspace service.",
        Arc::new(schema),
    )
    .with_raw_output_schema(Arc::new(projection::response_schema_object()))
    .with_annotations(ToolAnnotations::new().read_only(true).destructive(false))
}

pub(crate) struct BoundedJsonLineReader<R> {
    inner: R,
    line_bytes: usize,
    depth: usize,
    in_string: bool,
    escaped: bool,
}

impl<R> BoundedJsonLineReader<R> {
    pub(crate) fn new(inner: R) -> Self {
        Self {
            inner,
            line_bytes: 0,
            depth: 0,
            in_string: false,
            escaped: false,
        }
    }

    fn inspect(&mut self, bytes: &[u8]) -> io::Result<()> {
        for &byte in bytes {
            if byte == b'\n' {
                self.line_bytes = 0;
                self.depth = 0;
                self.in_string = false;
                self.escaped = false;
                continue;
            }
            self.line_bytes = self.line_bytes.saturating_add(1);
            if self.line_bytes > MAX_AUTHORING_PROTOCOL_FRAME_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "MCP frame exceeds byte limit",
                ));
            }
            if self.in_string {
                if self.escaped {
                    self.escaped = false;
                } else if byte == b'\\' {
                    self.escaped = true;
                } else if byte == b'"' {
                    self.in_string = false;
                }
                continue;
            }
            match byte {
                b'"' => self.in_string = true,
                b'{' | b'[' => {
                    self.depth = self.depth.saturating_add(1);
                    if self.depth > axiograph_security::MAX_JSON_NESTING_DEPTH {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "MCP JSON exceeds nesting limit",
                        ));
                    }
                }
                b'}' | b']' => self.depth = self.depth.saturating_sub(1),
                _ => {}
            }
        }
        Ok(())
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for BoundedJsonLineReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut TaskContext<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buffer.filled().len();
        match Pin::new(&mut self.inner).poll_read(context, buffer) {
            Poll::Ready(Ok(())) => {
                let bytes = &buffer.filled()[before..];
                if let Err(error) = self.inspect(bytes) {
                    Poll::Ready(Err(error))
                } else {
                    Poll::Ready(Ok(()))
                }
            }
            result => result,
        }
    }
}

pub(crate) struct BoundedLineWriter<W> {
    inner: W,
    line_bytes: usize,
}

impl<W> BoundedLineWriter<W> {
    pub(crate) fn new(inner: W) -> Self {
        Self {
            inner,
            line_bytes: 0,
        }
    }

    fn line_length_after(mut current: usize, bytes: &[u8]) -> Option<usize> {
        for &byte in bytes {
            if byte == b'\n' {
                current = 0;
            } else {
                current = current.checked_add(1)?;
                if current > MAX_AUTHORING_PROTOCOL_FRAME_BYTES {
                    return None;
                }
            }
        }
        Some(current)
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for BoundedLineWriter<W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut TaskContext<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        if Self::line_length_after(self.line_bytes, bytes).is_none() {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "MCP response frame exceeds byte limit",
            )));
        }
        match Pin::new(&mut self.inner).poll_write(context, bytes) {
            Poll::Ready(Ok(written)) => {
                let Some(written_bytes) = bytes.get(..written) else {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "MCP writer reported more bytes than supplied",
                    )));
                };
                let Some(line_bytes) = Self::line_length_after(self.line_bytes, written_bytes)
                else {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "MCP response frame exceeds byte limit",
                    )));
                };
                self.line_bytes = line_bytes;
                Poll::Ready(Ok(written))
            }
            result => result,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(context)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        context: &mut TaskContext<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(context)
    }
}

pub(crate) fn run_mcp_stdio(service: AuthoringWorkspaceService) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build authoring MCP runtime")?;
    runtime.block_on(async move {
        let server = AuthoringWorkspaceRmcpServer { service };
        let transport = rmcp::transport::async_rw::AsyncRwTransport::<RoleServer, _, _>::new_server(
            BoundedJsonLineReader::new(tokio::io::stdin()),
            BoundedLineWriter::new(tokio::io::stdout()),
        );
        let service = server
            .serve(transport)
            .await
            .context("serve bounded authoring MCP over rmcp stdio")?;
        service.waiting().await.context("wait for authoring MCP")?;
        Ok(())
    })
}

// --- LSP adapter ---------------------------------------------------------

struct BoundedLspIoThreads {
    reader: JoinHandle<io::Result<()>>,
    writer: JoinHandle<io::Result<()>>,
}

impl BoundedLspIoThreads {
    fn join(self) -> io::Result<()> {
        self.reader
            .join()
            .map_err(|_| io::Error::other("LSP reader thread panicked"))??;
        self.writer
            .join()
            .map_err(|_| io::Error::other("LSP writer thread panicked"))??;
        Ok(())
    }
}

fn bounded_lsp_stdio() -> (Connection, BoundedLspIoThreads) {
    let (outgoing_sender, outgoing_receiver) =
        crossbeam_channel::bounded::<Message>(MAX_AUTHORING_PROTOCOL_QUEUE);
    let (incoming_sender, incoming_receiver) =
        crossbeam_channel::bounded::<Message>(MAX_AUTHORING_PROTOCOL_QUEUE);

    let reader = std::thread::spawn(move || {
        let stdin = io::stdin();
        let mut input = BufReader::new(stdin.lock());
        while let Some(message) = read_bounded_lsp_message(&mut input)? {
            if incoming_sender.send(message).is_err() {
                break;
            }
        }
        Ok(())
    });
    let writer = std::thread::spawn(move || {
        let stdout = io::stdout();
        let mut output = stdout.lock();
        for message in outgoing_receiver {
            write_bounded_lsp_message(&mut output, &message)?;
        }
        Ok(())
    });

    (
        Connection {
            sender: outgoing_sender,
            receiver: incoming_receiver,
        },
        BoundedLspIoThreads { reader, writer },
    )
}

fn read_bounded_lsp_message(reader: &mut impl BufRead) -> io::Result<Option<Message>> {
    const MAX_HEADER_BYTES: usize = 8 * 1024;
    const MAX_HEADER_COUNT: usize = 32;
    let mut total_header_bytes = 0_usize;
    let mut content_length = None;
    let mut header_count = 0_usize;

    loop {
        let Some(line) = read_bounded_crlf_line(reader, MAX_HEADER_BYTES)? else {
            return if header_count == 0 {
                Ok(None)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "truncated LSP headers",
                ))
            };
        };
        total_header_bytes = total_header_bytes.saturating_add(line.len());
        header_count = header_count.saturating_add(1);
        if total_header_bytes > MAX_HEADER_BYTES || header_count > MAX_HEADER_COUNT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "LSP headers exceed hard limit",
            ));
        }
        if line == b"\r\n" {
            break;
        }
        let line = line
            .strip_suffix(b"\r\n")
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "malformed LSP header"))?;
        let line = std::str::from_utf8(line)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let (name, value) = line
            .split_once(": ")
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "malformed LSP header"))?;
        if name.eq_ignore_ascii_case("Content-Length") {
            if content_length.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "duplicate LSP Content-Length",
                ));
            }
            let parsed = value
                .parse::<usize>()
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            if parsed > MAX_AUTHORING_PROTOCOL_FRAME_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "LSP frame exceeds byte limit",
                ));
            }
            content_length = Some(parsed);
        }
    }

    let length = content_length
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing LSP Content-Length"))?;
    let mut bytes = vec![0_u8; length];
    reader.read_exact(&mut bytes)?;
    axiograph_security::validate_json_nesting(
        &bytes,
        axiograph_security::MAX_JSON_NESTING_DEPTH,
        "LSP frame",
    )
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn read_bounded_crlf_line(reader: &mut impl BufRead, limit: usize) -> io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Ok(Some(line))
            };
        }
        let count = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        if line.len().saturating_add(count) > limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "LSP header line exceeds byte limit",
            ));
        }
        line.extend_from_slice(&available[..count]);
        reader.consume(count);
        if line.ends_with(b"\n") {
            return Ok(Some(line));
        }
    }
}

fn write_bounded_lsp_message(writer: &mut impl Write, message: &Message) -> io::Result<()> {
    let bytes = serde_json::to_vec(message)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if bytes.len() > MAX_AUTHORING_PROTOCOL_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "LSP response frame exceeds byte limit",
        ));
    }
    write!(writer, "Content-Length: {}\r\n\r\n", bytes.len())?;
    writer.write_all(&bytes)?;
    writer.flush()
}

struct LspDocumentImage {
    // The key retains the client URI; ownership uses the workspace-validated path.
    identity: Option<PathBuf>,
    text: String,
    revision: axiograph_kernel::RevisionDigestV2,
    // LSP textDocument version (didOpen/didChange). `None` means the client never
    // sent a version (e.g. some didOpen payloads); version checks are then skipped
    // for that document rather than fabricating an ordering.
    version: Option<i64>,
}

// A single LSP `TextDocumentContentChangeEvent` decoded from the wire. `range`
// is `None` for full-document sync; `Some` means the client sent an
// incremental edit and every offset in it is expressed in UTF-16 code units
// (per the LSP spec), not bytes or Unicode scalar values.
struct LspContentChangeV1 {
    range: Option<LspRangeV1>,
    text: String,
}

#[derive(Clone, Copy)]
struct LspPositionV1 {
    line: u32,
    character: u32,
}

#[derive(Clone, Copy)]
struct LspRangeV1 {
    start: LspPositionV1,
    end: LspPositionV1,
}

#[derive(Debug, PartialEq, Eq)]
enum LspRangeEditErrorV1 {
    /// The range's start line/character sorts after its end line/character.
    InvertedRange,
    /// A line/character position does not exist in the current document, or a
    /// character offset lands inside a UTF-16 surrogate pair (an astral code
    /// point split across two UTF-16 units) rather than on its boundary.
    OutOfBounds,
}

// Decode a raw JSON `contentChanges` entry into a typed content change,
// rejecting the entry (via `None`) if it is not a well-formed full-text or
// range-edit payload. LSP full-text-sync payloads carry `text` with no
// `range`; incremental-sync payloads carry both `range` and `text`.
fn decode_lsp_content_change(change: &Value) -> Option<LspContentChangeV1> {
    let text = change.get("text").and_then(Value::as_str)?.to_string();
    let range = match change.get("range") {
        None | Some(Value::Null) => None,
        Some(range) => Some(LspRangeV1 {
            start: decode_lsp_position(range.get("start")?)?,
            end: decode_lsp_position(range.get("end")?)?,
        }),
    };
    Some(LspContentChangeV1 { range, text })
}

fn decode_lsp_position(position: &Value) -> Option<LspPositionV1> {
    Some(LspPositionV1 {
        line: u32::try_from(position.get("line")?.as_u64()?).ok()?,
        character: u32::try_from(position.get("character")?.as_u64()?).ok()?,
    })
}

// Convert a UTF-16 `(line, character)` position into a byte offset into
// `text`, failing closed (rather than clamping/rounding) when the line does
// not exist, the character offset exceeds the line's UTF-16 length, or the
// offset lands inside a surrogate pair (an astral/non-BMP code point is two
// UTF-16 units but one Unicode scalar value; a boundary between them is not a
// valid edit position).
fn utf16_position_to_byte_offset(
    text: &str,
    position: LspPositionV1,
) -> Result<usize, LspRangeEditErrorV1> {
    let mut current_line = 0u32;
    let mut line_start_byte = 0usize;
    if position.line > 0 {
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                current_line += 1;
                line_start_byte = index + 1;
                if current_line == position.line {
                    break;
                }
            }
        }
        if current_line < position.line {
            return Err(LspRangeEditErrorV1::OutOfBounds);
        }
    }
    let line_text = match text[line_start_byte..].split_once('\n') {
        Some((line, _)) => line,
        None => &text[line_start_byte..],
    };
    let mut utf16_units = 0u32;
    for (char_byte_offset, ch) in line_text.char_indices() {
        if utf16_units == position.character {
            return Ok(line_start_byte + char_byte_offset);
        }
        let width = ch.len_utf16() as u32;
        if utf16_units < position.character && position.character < utf16_units + width {
            // The requested offset lands inside this scalar value's UTF-16
            // encoding (only possible for astral code points, width == 2):
            // not a valid boundary.
            return Err(LspRangeEditErrorV1::OutOfBounds);
        }
        utf16_units += width;
    }
    if utf16_units == position.character {
        return Ok(line_start_byte + line_text.len());
    }
    Err(LspRangeEditErrorV1::OutOfBounds)
}

// Apply one incremental LSP range edit to `text`, returning the new full
// text. Fails closed (no partial/best-effort merge) on an inverted range or
// any position that does not resolve to a valid UTF-16 boundary.
fn apply_lsp_range_edit(
    text: &str,
    range: LspRangeV1,
    replacement: &str,
) -> Result<String, LspRangeEditErrorV1> {
    if (range.start.line, range.start.character) > (range.end.line, range.end.character) {
        return Err(LspRangeEditErrorV1::InvertedRange);
    }
    let start = utf16_position_to_byte_offset(text, range.start)?;
    let end = utf16_position_to_byte_offset(text, range.end)?;
    if start > end || end > text.len() {
        return Err(LspRangeEditErrorV1::OutOfBounds);
    }
    let mut merged = String::with_capacity(text.len() - (end - start) + replacement.len());
    merged.push_str(&text[..start]);
    merged.push_str(replacement);
    merged.push_str(&text[end..]);
    Ok(merged)
}

// Apply a full `contentChanges` sequence (as sent by one `didChange`) against
// `base` in order, and return the resulting text. Each entry may be a
// full-text replacement (`range: None`) or an incremental range edit; a
// malformed entry or a range edit that fails `apply_lsp_range_edit` fails the
// whole sequence closed rather than applying a prefix.
fn apply_lsp_content_changes(base: &str, changes: &[Value]) -> Result<String, LspRangeEditErrorV1> {
    let mut current = base.to_string();
    for change in changes {
        let Some(decoded) = decode_lsp_content_change(change) else {
            return Err(LspRangeEditErrorV1::OutOfBounds);
        };
        current = match decoded.range {
            None => decoded.text,
            Some(range) => apply_lsp_range_edit(&current, range, &decoded.text)?,
        };
    }
    Ok(current)
}

struct AuthoringWorkspaceLspState {
    service: AuthoringWorkspaceService,
    default_axi_path: Option<String>,
    documents: BTreeMap<String, LspDocumentImage>,
    // Sticky, bounded fallback: a rejected image cannot be tracked by an unbounded
    // URI tombstone map. Restart the session to restore precise publications.
    document_images_incomplete: bool,
    // Publications by requesting document, then owning source URI.
    publications: BTreeMap<String, BTreeMap<String, Vec<Diagnostic>>>,
}

pub(crate) fn run_lsp_stdio(
    service: AuthoringWorkspaceService,
    default_axi_path: Option<String>,
) -> Result<()> {
    let (connection, io_threads) = bounded_lsp_stdio();
    let (initialize_id, _) = connection.initialize_start()?;
    connection.initialize_finish(
        initialize_id,
        serde_json::to_value(lsp_initialize_result())?,
    )?;
    let mut state = AuthoringWorkspaceLspState {
        service,
        default_axi_path,
        documents: BTreeMap::new(),
        document_images_incomplete: false,
        publications: BTreeMap::new(),
    };
    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    break;
                }
                let response = handle_lsp_request(&mut state, request);
                connection.sender.send(response)?;
            }
            Message::Notification(notification) => {
                if notification.method == "exit" {
                    break;
                }
                for message in handle_lsp_notification(&mut state, notification) {
                    connection.sender.send(message)?;
                }
            }
            Message::Response(_) => {}
        }
    }
    drop(connection);
    io_threads.join()?;
    Ok(())
}

fn lsp_initialize_result() -> InitializeResult {
    InitializeResult {
        capabilities: LspServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
            execute_command_provider: Some(ExecuteCommandOptions {
                commands: vec![AUTHORING_WORKSPACE_LSP_COMMAND.to_string()],
                work_done_progress_options: WorkDoneProgressOptions::default(),
            }),
            ..LspServerCapabilities::default()
        },
        server_info: Some(LspServerInfo {
            name: "axiograph-authoring-workspace".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
    }
}

fn handle_lsp_request(state: &mut AuthoringWorkspaceLspState, request: LspRequest) -> Message {
    let id = request.id.clone();
    let result = match request.method.as_str() {
        "textDocument/codeAction" => {
            let uri = request
                .params
                .pointer("/textDocument/uri")
                .and_then(Value::as_str)
                .unwrap_or("");
            let authoring_request = state
                .documents
                .get(uri)
                .and_then(|image| lsp_request_for_document(state, uri, &image.text, false));
            Ok(authoring_request.map_or_else(
                || json!([]),
                |authoring_request| {
                    json!([{
                        "title": "Axiograph: inspect typed workspace authoring state",
                        "kind": "quickfix",
                        "command": {
                            "title": "Inspect authoring workspace",
                            "command": AUTHORING_WORKSPACE_LSP_COMMAND,
                            "arguments": [authoring_request]
                        }
                    }])
                },
            ))
        }
        "workspace/executeCommand" => {
            let params = request.params;
            let command = params.get("command").and_then(Value::as_str).unwrap_or("");
            if command != AUTHORING_WORKSPACE_LSP_COMMAND {
                Err(anyhow!("unsupported authoring LSP command `{command}`"))
            } else {
                let value = params
                    .get("arguments")
                    .and_then(Value::as_array)
                    .and_then(|arguments| arguments.first())
                    .cloned()
                    .ok_or_else(|| anyhow!("authoring LSP command requires one request argument"));
                value.and_then(|value| {
                    let request: AuthoringWorkspaceRequestV1 = serde_json::from_value(value)?;
                    Ok(serde_json::to_value(
                        state.service.execute_response(request)?,
                    )?)
                })
            }
        }
        other => Err(anyhow!("unsupported authoring LSP request `{other}`")),
    };
    match result {
        Ok(value) => Message::Response(lsp_server::Response::new_ok(id, value)),
        Err(error) => Message::Response(lsp_server::Response::new_err(
            id,
            ErrorCode::RequestFailed as i32,
            error.to_string(),
        )),
    }
}

fn handle_lsp_notification(
    state: &mut AuthoringWorkspaceLspState,
    notification: Notification,
) -> Vec<Message> {
    match notification.method.as_str() {
        "textDocument/didOpen" => {
            let uri = notification
                .params
                .pointer("/textDocument/uri")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let text = notification
                .params
                .pointer("/textDocument/text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let version = notification
                .params
                .pointer("/textDocument/version")
                .and_then(Value::as_i64);
            if store_lsp_document(state, &uri, &text, version) {
                let mut messages = publish_lsp_diagnostics(state, &uri, &text);
                // A newly opened unsaved buffer can be an import overlay for
                // other open documents (see `open_document_import_overlays`);
                // those importers must be recomputed now, not left pinned to
                // whatever they resolved against before this buffer existed.
                messages.extend(revalidate_dependent_lsp_importers(state, &uri));
                messages
            } else {
                let mut messages = replace_lsp_publications(state, &uri, BTreeMap::new());
                messages.extend(lsp_resource_limit_diagnostic(&uri));
                messages
            }
        }
        "textDocument/didChange" => {
            let uri = notification
                .params
                .pointer("/textDocument/uri")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let version = notification
                .params
                .pointer("/textDocument/version")
                .and_then(Value::as_i64);
            if !lsp_version_is_monotonic(state, &uri, version) {
                // Stale/out-of-order version: fail closed. Drop the tracked image so a
                // rejected edit cannot silently keep serving diagnostics against text
                // the client no longer holds, and publish a typed resource diagnostic
                // instead of an unsafe/best-effort merge.
                state.documents.remove(&uri);
                state.document_images_incomplete = true;
                let mut messages = replace_lsp_publications(state, &uri, BTreeMap::new());
                messages.extend(lsp_stale_version_diagnostic(&uri));
                return messages;
            }
            let changes = notification
                .params
                .get("contentChanges")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            // Incremental range edits (UTF-16 line/character positions, per the
            // LSP spec) are applied against the last tracked image for this URI;
            // a lone full-text entry (no `range`) replaces it directly. Either an
            // unresolvable range (out-of-bounds line/character, a boundary
            // inside a UTF-16 surrogate pair, or an inverted range) or an
            // unknown base document (a range edit with nothing tracked yet)
            // fails the whole edit closed: the tracked image is dropped rather
            // than silently applying a partial/best-effort merge.
            let base = state.documents.get(&uri).map(|image| image.text.clone());
            let applied = match (
                base,
                changes
                    .iter()
                    .any(|change| change.get("range").is_some_and(|range| !range.is_null())),
            ) {
                (_, false) => changes
                    .last()
                    .and_then(|change| change.get("text"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .ok_or(LspRangeEditErrorV1::OutOfBounds),
                (None, true) => Err(LspRangeEditErrorV1::OutOfBounds),
                (Some(base), true) => apply_lsp_content_changes(&base, &changes),
            };
            let Ok(text) = applied else {
                state.documents.remove(&uri);
                state.document_images_incomplete = true;
                let mut messages = replace_lsp_publications(state, &uri, BTreeMap::new());
                messages.extend(lsp_stale_version_diagnostic(&uri));
                return messages;
            };
            if store_lsp_document(state, &uri, &text, version) {
                publish_lsp_diagnostics(state, &uri, &text)
            } else {
                let mut messages = replace_lsp_publications(state, &uri, BTreeMap::new());
                messages.extend(lsp_resource_limit_diagnostic(&uri));
                messages
            }
        }
        "textDocument/didSave" => {
            let Some(uri) = notification
                .params
                .pointer("/textDocument/uri")
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                return Vec::new();
            };
            // didSave carries no authoritative text in this session's sync mode
            // (full-document sync only); the tracked in-memory image from the
            // last didOpen/didChange remains the single source of truth. Saving
            // does not invalidate that image, but it must deterministically
            // revalidate this document and every other tracked document whose
            // last-published diagnostics depended on this document's on-disk
            // bytes, since a save is exactly the point at which those bytes are
            // guaranteed to change from the caller's perspective.
            let Some(text) = state.documents.get(&uri).map(|image| image.text.clone()) else {
                return Vec::new();
            };
            let mut messages = publish_lsp_diagnostics(state, &uri, &text);
            messages.extend(revalidate_dependent_lsp_importers(state, &uri));
            messages
        }
        "textDocument/didClose" => {
            if let Some(uri) = notification
                .params
                .pointer("/textDocument/uri")
                .and_then(Value::as_str)
            {
                state.documents.remove(uri);
                let mut messages = replace_lsp_publications(state, uri, BTreeMap::new());
                messages.extend(revalidate_dependent_lsp_importers(state, uri));
                return messages;
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

fn store_lsp_document(
    state: &mut AuthoringWorkspaceLspState,
    uri: &str,
    text: &str,
    version: Option<i64>,
) -> bool {
    let previous = state.documents.get(uri).map_or(0, |image| image.text.len());
    let total = state
        .documents
        .values()
        .try_fold(0_usize, |total, image| total.checked_add(image.text.len()))
        .and_then(|total| total.checked_sub(previous))
        .and_then(|total| total.checked_add(text.len()));
    if uri.is_empty()
        || text.len() > MAX_AUTHORING_SOURCE_BYTES as usize
        || (!state.documents.contains_key(uri)
            && state.documents.len() >= MAX_AUTHORING_LSP_DOCUMENTS)
        || total.is_none_or(|total| total > MAX_AUTHORING_LSP_TOTAL_BYTES)
    {
        state.document_images_incomplete = true;
        state.documents.remove(uri);
        return false;
    }
    state.documents.insert(
        uri.to_string(),
        LspDocumentImage {
            identity: uri_to_workspace_path(state.service.root(), uri).ok(),
            text: text.to_string(),
            revision: axiograph_kernel::RevisionDigestV2::from_accepted_text(text),
            version,
        },
    );
    true
}

// Version monotonicity: a didChange whose version is not strictly greater than the
// last tracked version for this document is stale/out-of-order and must fail closed
// rather than silently overwrite newer client state with older text. Documents with
// no tracked version yet, or edits carrying no version at all, are accepted (the
// client did not opt into version tracking for this exchange).
fn lsp_version_is_monotonic(
    state: &AuthoringWorkspaceLspState,
    uri: &str,
    version: Option<i64>,
) -> bool {
    let (Some(next), Some(previous)) = (
        version,
        state.documents.get(uri).and_then(|image| image.version),
    ) else {
        return true;
    };
    next > previous
}

fn lsp_stale_version_diagnostic(uri: &str) -> Vec<Message> {
    let Ok(uri) = uri.parse::<Uri>() else {
        return Vec::new();
    };
    let diagnostic = Diagnostic {
        range: Range::new(Position::new(0, 0), Position::new(0, 0)),
        data: Some(json!({"sourceLocated":false,"location":null})),
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(lsp_types::NumberOrString::String(
            "authoring.stale_version".to_string(),
        )),
        source: Some("axiograph-authoring-workspace".to_string()),
        message: "document edit rejected: version is not strictly greater than the last tracked version; precise diagnostic publications are disabled until the client resends an in-order edit".to_string(),
        ..Diagnostic::default()
    };
    vec![Message::Notification(Notification::new(
        "textDocument/publishDiagnostics".to_string(),
        serde_json::to_value(PublishDiagnosticsParams::new(uri, vec![diagnostic], None))
            .unwrap_or(Value::Null),
    ))]
}

// At most MAX_AUTHORING_LSP_DOCUMENTS images are inspected. Multiple client aliases
// are ambiguous even when their text matches; never choose one arbitrarily.
fn verified_lsp_owner<'a>(
    documents: &'a BTreeMap<String, LspDocumentImage>,
    images_incomplete: bool,
    path: &Path,
    revision: &str,
) -> std::result::Result<Option<&'a str>, ()> {
    if images_incomplete {
        return Err(());
    }
    let mut matches = documents
        .iter()
        .filter(|(_, image)| image.identity.as_deref() == Some(path));
    let Some((uri, image)) = matches.next() else {
        return Ok(None);
    };
    if matches.next().is_some() || image.revision.as_str() != revision {
        return Err(());
    }
    Ok(Some(uri))
}

fn lsp_resource_limit_diagnostic(uri: &str) -> Vec<Message> {
    let Ok(uri) = uri.parse::<Uri>() else {
        return Vec::new();
    };
    let diagnostic = Diagnostic {
        range: Range::new(Position::new(0, 0), Position::new(0, 0)),
        data: Some(json!({"sourceLocated":false,"location":null})),
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(lsp_types::NumberOrString::String(
            "authoring.resource_limit".to_string(),
        )),
        source: Some("axiograph-authoring-workspace".to_string()),
        message: "document rejected by authoring LSP byte/count limits; precise diagnostic publications are disabled until the LSP session restarts".to_string(),
        ..Diagnostic::default()
    };
    vec![Message::Notification(Notification::new(
        "textDocument/publishDiagnostics".to_string(),
        serde_json::to_value(PublishDiagnosticsParams::new(uri, vec![diagnostic], None))
            .unwrap_or(Value::Null),
    ))]
}

// Build a bounded canonical-path -> unsaved-bytes overlay from every other
// currently open LSP document. This lets an importer resolve against an
// open-but-unsaved importee buffer instead of stale/absent disk bytes,
// without ever writing to disk. `requester_uri` is excluded (its own text is
// supplied as the root buffer, not as an import overlay of itself); documents
// with no validated workspace identity (rejected/never-verified paths) are
// skipped rather than guessed at. A document opened under an alias URI (a
// URI that does not round-trip to the same string once resolved back from
// its canonical workspace path) is never trusted as an overlay: aliases and
// ambiguous images must not recover stale precision, matching the ownership
// policy `verified_lsp_owner` already enforces for diagnostic locations. Two
// distinct *canonical* URIs racing to claim the same import path are also
// excluded outright rather than picking one arbitrarily.
fn open_document_import_overlays(
    state: &AuthoringWorkspaceLspState,
    requester_uri: &str,
) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut claims = BTreeMap::<PathBuf, Vec<u8>>::new();
    let mut ambiguous = BTreeSet::<PathBuf>::new();
    let mut seen = 0_usize;
    for (document_uri, image) in &state.documents {
        if document_uri.as_str() == requester_uri || seen >= MAX_AUTHORING_LSP_DOCUMENTS {
            continue;
        }
        seen += 1;
        let Some(path) = image.identity.clone() else {
            continue;
        };
        let is_canonical_uri = url::Url::from_file_path(&path)
            .is_ok_and(|canonical| canonical.as_str() == document_uri.as_str());
        if !is_canonical_uri {
            continue;
        }
        if claims
            .insert(path.clone(), image.text.as_bytes().to_vec())
            .is_some()
        {
            ambiguous.insert(path);
        }
    }
    for path in ambiguous {
        claims.remove(&path);
    }
    claims
}

fn lsp_request_for_document(
    state: &AuthoringWorkspaceLspState,
    uri: &str,
    text: &str,
    diagnostics_only: bool,
) -> Option<AuthoringWorkspaceRequestV1> {
    let path = uri_to_workspace_relative(state.service.root(), uri).ok()?;
    if path.ends_with(".axi") {
        Some(AuthoringWorkspaceRequestV1 {
            version: AUTHORING_WORKSPACE_REQUEST_VERSION_V1.to_string(),
            presentation: AuthoringPresentationV1::default(),
            operation: if diagnostics_only {
                AuthoringWorkspaceOperationV1::Validate
            } else {
                AuthoringWorkspaceOperationV1::Inspect
            },
            axi_path: path,
            axi_text: Some(text.to_string()),
            baseline_axi_path: None,
            baseline_axi_text: None,
            cq_path: None,
            cq_text: None,
            schema: None,
            olog_fragment: None,
            query_ir_v1: None,
            focus_variable: None,
            apply_refinement_handle_id: None,
        })
    } else if path.ends_with(".cq") {
        Some(AuthoringWorkspaceRequestV1 {
            version: AUTHORING_WORKSPACE_REQUEST_VERSION_V1.to_string(),
            presentation: AuthoringPresentationV1::default(),
            operation: AuthoringWorkspaceOperationV1::Inspect,
            axi_path: state.default_axi_path.clone()?,
            axi_text: None,
            baseline_axi_path: None,
            baseline_axi_text: None,
            cq_path: Some(path),
            cq_text: Some(text.to_string()),
            schema: None,
            olog_fragment: None,
            query_ir_v1: None,
            focus_variable: None,
            apply_refinement_handle_id: None,
        })
    } else {
        None
    }
}

fn publish_lsp_diagnostics(
    state: &mut AuthoringWorkspaceLspState,
    uri: &str,
    text: &str,
) -> Vec<Message> {
    let Some(request) = lsp_request_for_document(state, uri, text, true) else {
        let diagnostic = authoring_diagnostic_to_lsp(AuthoringDiagnosticV1 {
            severity: AuthoringDiagnosticSeverityV1::Error,
            code: "authoring_lsp_document_unsupported".to_string(),
            message: "diagnostics require an existing on-disk workspace .axi file as the root (or a .cq file with a configured default); imports of that root now resolve against open-but-unsaved importee buffers as an overlay closure, but a brand-new root with no on-disk backing is still unsupported".to_string(),
            path: None, line: None, repair_hint: None, location: None,
        });
        return replace_lsp_publications(
            state,
            uri,
            BTreeMap::from([(uri.to_string(), vec![diagnostic])]),
        );
    };
    let report = match state
        .service
        .execute_with_import_overlays(request, &open_document_import_overlays(state, uri))
    {
        Ok(report) => report,
        Err(error) => {
            let diagnostic = authoring_diagnostic_to_lsp(AuthoringDiagnosticV1 {
                severity: AuthoringDiagnosticSeverityV1::Error,
                code: "authoring_workspace_request_failed".to_string(),
                message: error.to_string(),
                path: None,
                line: None,
                repair_hint: None,
                location: None,
            });
            return replace_lsp_publications(
                state,
                uri,
                BTreeMap::from([(uri.to_string(), vec![diagnostic])]),
            );
        }
    };
    let mut owners = BTreeMap::<String, Vec<Diagnostic>>::from([(uri.to_string(), Vec::new())]);
    for mut diagnostic in report.diagnostics {
        let owner = diagnostic
            .location
            .as_ref()
            .and_then(|location| {
                url::Url::from_file_path(&location.path)
                    .ok()
                    .map(|url| url.to_string())
            })
            .unwrap_or_else(|| uri.to_string());
        // Imports remain disk-backed. Only a unique, verified editor image can own
        // a precise range; equivalent file URIs must not bypass this check.
        let verified_owner = diagnostic.location.as_ref().map_or(Ok(None), |location| {
            verified_lsp_owner(
                &state.documents,
                state.document_images_incomplete,
                Path::new(&location.path),
                &location.revision_digest,
            )
        });
        let owner = match verified_owner {
            Ok(Some(client_uri)) => client_uri.to_string(),
            Ok(None) => owner,
            Err(()) => {
                diagnostic.message.push_str(" (source editor image differs, is ambiguous, or is unavailable; unsaved import overlays are unsupported)");
                diagnostic.location = None;
                uri.to_string()
            }
        };
        owners
            .entry(owner)
            .or_default()
            .push(authoring_diagnostic_to_lsp(diagnostic));
    }
    replace_lsp_publications(state, uri, owners)
}

fn replace_lsp_publications(
    state: &mut AuthoringWorkspaceLspState,
    requester: &str,
    owners: BTreeMap<String, Vec<Diagnostic>>,
) -> Vec<Message> {
    let mut affected = owners.keys().cloned().collect::<BTreeSet<_>>();
    affected.insert(requester.to_string());
    if let Some(previous) = state.publications.remove(requester) {
        affected.extend(previous.into_keys());
    }
    if !owners.is_empty() {
        state.publications.insert(requester.to_string(), owners);
    }
    // An edit to an imported document also invalidates previous disk-backed publications.
    // Do not recompile every importer or silently treat the editor map as an import overlay.
    for (root_uri, owners) in &mut state.publications {
        let stale = owners
            .keys()
            .filter(|owner| {
                owners[*owner].iter().any(|diagnostic| {
                    let Some(location) = diagnostic
                        .data
                        .as_ref()
                        .and_then(|data| data.get("location"))
                        .filter(|location| !location.is_null())
                    else {
                        return false;
                    };
                    let Some(path) = location.get("path").and_then(Value::as_str) else {
                        return true;
                    };
                    let Some(revision) = location.get("revision_digest").and_then(Value::as_str)
                    else {
                        return true;
                    };
                    match verified_lsp_owner(
                        &state.documents,
                        state.document_images_incomplete,
                        Path::new(path),
                        revision,
                    ) {
                        Err(()) => true,
                        // A newly opened alias also invalidates the former publication URI.
                        Ok(Some(client_uri)) => client_uri != owner.as_str(),
                        Ok(None) => false,
                    }
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        for owner in stale {
            if let Some(mut diagnostics) = owners.remove(&owner) {
                for diagnostic in &mut diagnostics {
                    diagnostic.range = Range::new(Position::new(0, 0), Position::new(0, 0));
                    diagnostic.data = Some(json!({"sourceLocated":false,"location":null}));
                    diagnostic.message.push_str(" (source editor image or owner changed, is ambiguous, or is unavailable; revalidate the importing document; unsaved import overlays are unsupported)");
                }
                owners
                    .entry(root_uri.clone())
                    .or_default()
                    .extend(diagnostics);
                affected.insert(owner);
                affected.insert(root_uri.clone());
            }
        }
    }
    affected
        .into_iter()
        .filter_map(|owner| {
            let diagnostics = state
                .publications
                .values()
                .filter_map(|owners| owners.get(&owner))
                .flatten()
                .cloned()
                .collect();
            Some(Message::Notification(Notification::new(
                "textDocument/publishDiagnostics".to_string(),
                serde_json::to_value(PublishDiagnosticsParams::new(
                    owner.parse().ok()?,
                    diagnostics,
                    None,
                ))
                .ok()?,
            )))
        })
        .collect()
}

// On didSave/didClose the on-disk bytes for `saved_uri` are now authoritative (a
// save writes them; a close drops the in-memory override that was shadowing them).
// `replace_lsp_publications` already invalidates (unlocates) any other open root's
// stale-owned diagnostics as soon as the edit happens, so by the time didSave/
// didClose runs there is no reliable ownership key left to search for. Instead,
// deterministically revalidate every other currently open document: this is a
// bounded scan (at most MAX_AUTHORING_LSP_DOCUMENTS open documents) and guarantees
// no open importer keeps serving diagnostics computed against bytes that a save or
// close has just superseded.
fn revalidate_dependent_lsp_importers(
    state: &mut AuthoringWorkspaceLspState,
    saved_uri: &str,
) -> Vec<Message> {
    let dependents: Vec<(String, String)> = state
        .documents
        .iter()
        .filter(|(uri, _)| uri.as_str() != saved_uri)
        .map(|(uri, image)| (uri.clone(), image.text.clone()))
        .collect();
    let mut messages = Vec::new();
    for (dependent_uri, text) in dependents {
        messages.extend(publish_lsp_diagnostics(state, &dependent_uri, &text));
    }
    messages
}

fn uri_to_workspace_relative(root: &Path, uri: &str) -> Result<String> {
    let path = uri_to_workspace_path(root, uri)?;
    Ok(path.strip_prefix(root)?.to_string_lossy().to_string())
}

fn uri_to_workspace_path(root: &Path, uri: &str) -> Result<PathBuf> {
    let url = url::Url::parse(uri)?;
    let path = url
        .to_file_path()
        .map_err(|_| anyhow!("LSP URI is not a local file: `{uri}`"))?;
    let path = std::fs::canonicalize(path)?;
    path.strip_prefix(root)
        .map_err(|_| anyhow!("LSP document is outside the authoring workspace"))?;
    Ok(path)
}

fn authoring_diagnostic_to_lsp(diagnostic: AuthoringDiagnosticV1) -> Diagnostic {
    let range = diagnostic.location.as_ref().map_or_else(
        || Range::new(Position::new(0, 0), Position::new(0, 0)),
        |location| {
            Range::new(
                Position::new(location.start.lsp_line, location.start.lsp_character),
                Position::new(location.end.lsp_line, location.end.lsp_character),
            )
        },
    );
    let data =
        json!({"sourceLocated": diagnostic.location.is_some(), "location": diagnostic.location});
    Diagnostic {
        range,
        data: Some(data),
        severity: Some(match diagnostic.severity {
            AuthoringDiagnosticSeverityV1::Error => DiagnosticSeverity::ERROR,
            AuthoringDiagnosticSeverityV1::Warning => DiagnosticSeverity::WARNING,
            AuthoringDiagnosticSeverityV1::Information => DiagnosticSeverity::INFORMATION,
        }),
        code: Some(lsp_types::NumberOrString::String(diagnostic.code)),
        source: Some("axiograph-authoring-workspace".to_string()),
        message: diagnostic.message,
        ..Diagnostic::default()
    }
}

// --- HTTP adapter --------------------------------------------------------

type HttpResponse = Response<Full<Bytes>>;

pub(crate) fn run_http(service: AuthoringWorkspaceService, listen: SocketAddr) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .max_blocking_threads(16)
        .enable_all()
        .build()
        .context("build authoring HTTP runtime")?;
    runtime.block_on(async move {
        let listener = TcpListener::bind(listen).await?;
        println!(
            "axiograph authoring workspace HTTP listening on {}",
            listener.local_addr()?
        );
        let service = Arc::new(service);
        let connections = Arc::new(Semaphore::new(MAX_AUTHORING_HTTP_CONNECTIONS));
        loop {
            let (stream, _) = listener.accept().await?;
            let Ok(permit) = Arc::clone(&connections).try_acquire_owned() else {
                drop(stream);
                continue;
            };
            let service = Arc::clone(&service);
            tokio::spawn(async move {
                let _permit = permit;
                let handler = service_fn(move |request| {
                    let service = Arc::clone(&service);
                    async move { Ok::<_, Infallible>(handle_http_request(request, service).await) }
                });
                let mut builder = http1::Builder::new();
                builder.max_headers(64).max_buf_size(64 * 1024);
                let connection = builder.serve_connection(TokioIo::new(stream), handler);
                let _ = tokio::time::timeout(MAX_AUTHORING_HTTP_CONNECTION_TIME, connection).await;
            });
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    })
}

async fn handle_http_request(
    request: Request<Incoming>,
    service: Arc<AuthoringWorkspaceService>,
) -> HttpResponse {
    match (request.method(), request.uri().path()) {
        (&Method::GET, "/healthz") => response(StatusCode::OK, "text/plain", b"ok\n".to_vec()),
        (&Method::GET, "/authoring/capabilities") => {
            json_response(StatusCode::OK, &authoring_workspace_capabilities_v1())
        }
        (&Method::POST, "/authoring") => {
            let body = match Limited::new(request.into_body(), MAX_AUTHORING_BODY_BYTES)
                .collect()
                .await
            {
                Ok(body) => body.to_bytes(),
                Err(error) => {
                    return error_response(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        format!("invalid or oversized authoring request: {error}"),
                    )
                }
            };
            let execution =
                tokio::task::spawn_blocking(move || execute_http_payload(&service, &body));
            match execution.await {
                Ok(response) => response,
                Err(error) => error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("authoring worker failed: {error}"),
                ),
            }
        }
        _ => error_response(StatusCode::NOT_FOUND, "not found"),
    }
}

fn execute_http_payload(service: &AuthoringWorkspaceService, body: &[u8]) -> HttpResponse {
    let request: AuthoringWorkspaceRequestV1 = match crate::security::parse_json_bounded(
        body,
        MAX_AUTHORING_BODY_BYTES,
        "authoring HTTP request",
    ) {
        Ok(request) => request,
        Err(error) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                format!("invalid authoring request: {error}"),
            )
        }
    };
    match service.execute_response(request) {
        Ok(report) => json_response(StatusCode::OK, &report),
        Err(error) => error_response(StatusCode::BAD_REQUEST, error.to_string()),
    }
}

fn json_response<T: Serialize>(status: StatusCode, value: &T) -> HttpResponse {
    match serde_json::to_vec(value) {
        Ok(bytes) if bytes.len() <= MAX_AUTHORING_PROTOCOL_FRAME_BYTES => {
            response(status, "application/json", bytes)
        }
        Ok(bytes) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(
                "response exceeds {} bytes (actual {})",
                MAX_AUTHORING_PROTOCOL_FRAME_BYTES,
                bytes.len()
            ),
        ),
        Err(error) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("response serialization failed: {error}"),
        ),
    }
}

fn error_response(status: StatusCode, message: impl ToString) -> HttpResponse {
    json_response(status, &json!({"error": message.to_string()}))
}

fn response(status: StatusCode, content_type: &'static str, body: Vec<u8>) -> HttpResponse {
    let mut response = Response::new(Full::new(Bytes::from(body)));
    *response.status_mut() = status;
    if let Ok(content_type) = content_type.parse() {
        response.headers_mut().insert(CONTENT_TYPE, content_type);
    }
    response
}

#[cfg(test)]
mod diagnostics_tests;
#[cfg(test)]
mod package_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    use tempfile::TempDir;

    #[test]
    fn lsp_reader_rejects_oversized_and_deep_frames_before_decode() {
        let oversized = format!(
            "Content-Length: {}\r\n\r\n",
            MAX_AUTHORING_PROTOCOL_FRAME_BYTES + 1
        );
        let error = read_bounded_lsp_message(&mut std::io::Cursor::new(oversized))
            .expect_err("oversized LSP frame must reject");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);

        let body = format!(
            "{}0{}",
            "[".repeat(axiograph_security::MAX_JSON_NESTING_DEPTH + 1),
            "]".repeat(axiograph_security::MAX_JSON_NESTING_DEPTH + 1)
        );
        let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let error = read_bounded_lsp_message(&mut std::io::Cursor::new(frame))
            .expect_err("deep LSP JSON must reject");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn mcp_reader_rejects_oversized_and_deep_lines() {
        let mut oversized = BoundedJsonLineReader::new(());
        oversized.line_bytes = MAX_AUTHORING_PROTOCOL_FRAME_BYTES;
        assert_eq!(
            oversized.inspect(b"x").unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );

        let mut deep = BoundedJsonLineReader::new(());
        let nesting = vec![b'['; axiograph_security::MAX_JSON_NESTING_DEPTH + 1];
        assert_eq!(
            deep.inspect(&nesting).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    pub(super) fn write_workspace() -> Result<(TempDir, AuthoringWorkspaceService)> {
        let temp = tempfile::tempdir()?;
        crate::security::write_output_bounded(
            temp.path().join("domain.axi"),
            r#"module Demo
        schema S:
          object Person
          object Team
          relation WorksFor(employee: Person, employer: Team)
        instance I of S:
          Person = {Alice}
          Team = {Ops}
          WorksFor = {(employee=Alice, employer=Ops)}
        "#,
            "CLI output",
        )?;
        crate::security::write_output_bounded(
            temp.path().join("baseline.axi"),
            r#"module Demo
        schema S:
          object Person
        instance I of S:
          Person = {Alice}
        "#,
            "CLI output",
        )?;
        crate::security::write_output_bounded(
            temp.path().join("questions.cq"),
            r#"version competency_question_bundle_v1
        question works_for_exists:
          ask: does an employee work for a team?
          expect: exists S.WorksFor(employee=?employee, employer=?team)
        "#,
            "CLI output",
        )?;
        let service = AuthoringWorkspaceService::new(temp.path())?;
        Ok((temp, service))
    }

    pub(super) fn request() -> AuthoringWorkspaceRequestV1 {
        AuthoringWorkspaceRequestV1 {
            version: AUTHORING_WORKSPACE_REQUEST_VERSION_V1.to_string(),
            presentation: AuthoringPresentationV1::default(),
            operation: AuthoringWorkspaceOperationV1::Inspect,
            axi_path: "domain.axi".to_string(),
            axi_text: None,
            baseline_axi_path: Some("baseline.axi".to_string()),
            baseline_axi_text: None,
            cq_path: Some("questions.cq".to_string()),
            cq_text: None,
            schema: Some("S".to_string()),
            olog_fragment: None,
            query_ir_v1: Some(
                serde_json::from_value(json!({
                    "version": 1,
                    "select_vars": ["person"],
                    "where_atoms": [{"kind": "type", "term": "?person", "type": "Person"}],
                    "limit": 10
                }))
                .expect("query"),
            ),
            focus_variable: Some("person".to_string()),
            apply_refinement_handle_id: None,
        }
    }

    #[test]
    fn workspace_service_unifies_validation_cq_query_explanation_and_evolution() -> Result<()> {
        let (_temp, service) = write_workspace()?;
        let report = service.execute(request())?;
        assert!(report.ok, "{:?}", report.diagnostics);
        assert!(report.validation.canonical_axi_valid);
        assert!(report.validation.compiled_kernel_ir_valid);
        assert!(report.validation.finite_category_fragment_valid);
        assert!(report
            .validation
            .finite_theory_gate
            .as_ref()
            .is_some_and(|receipt| {
                receipt.passed
                    && receipt.consumer == axiograph_kernel::FiniteTheoryGateConsumerIr::Authoring
                    && receipt.coverage.category_formations_replayed == 1
                    && receipt.coverage.saturated_presentations == 1
                    && receipt.coverage.identity_paths_replayed > 0
                    && receipt.coverage.path_explanations_replayed > 0
                    && receipt.coverage.non_identity_scope_transports_certified == 0
            }));
        assert!(report.prepared_query.as_ref().is_some_and(|metadata| {
            metadata.finite_theory_gate.as_ref().is_some_and(|receipt| {
                receipt.passed
                    && receipt.consumer == axiograph_kernel::FiniteTheoryGateConsumerIr::Query
                    && receipt.coverage.path_explanations_replayed > 0
            })
        }));
        assert!(report.query_explanation.is_some());
        assert_eq!(
            report
                .competency_questions
                .as_ref()
                .and_then(|cq| cq.evaluation.as_ref())
                .map(|cq| (cq.satisfied, cq.total)),
            Some((1, 1))
        );
        assert!(matches!(
            report.evolution_previews.first(),
            Some(AuthoringEvolutionPreviewV1::FiniteKernel(_))
        ));
        assert!(!report.promotion.protected_main_eligible);
        assert!(report
            .promotion
            .blockers
            .iter()
            .any(|blocker| blocker.contains("VerifyMain")));
        Ok(())
    }

    #[test]
    fn review_only_theory_obligations_block_the_authoring_gate() -> Result<()> {
        let temp = tempfile::tempdir()?;
        crate::security::write_output_bounded(
            temp.path().join("review_only.axi"),
            r#"module ReviewOnly
schema S:
  object Person
  relation Parent(child: Person, parent: Person)
theory T on S:
  constraint transitive Parent on (child, parent)
instance I of S:
  Person = {Alice}
  Parent = {p: (child=Alice, parent=Alice)}
"#,
            "CLI output",
        )?;
        let service = AuthoringWorkspaceService::new(temp.path())?;
        let report = service.execute(AuthoringWorkspaceRequestV1 {
            version: AUTHORING_WORKSPACE_REQUEST_VERSION_V1.to_string(),
            presentation: AuthoringPresentationV1::default(),
            operation: AuthoringWorkspaceOperationV1::PromotionReview,
            axi_path: "review_only.axi".to_string(),
            axi_text: None,
            baseline_axi_path: None,
            baseline_axi_text: None,
            cq_path: None,
            cq_text: None,
            schema: Some("S".to_string()),
            olog_fragment: None,
            query_ir_v1: None,
            focus_variable: None,
            apply_refinement_handle_id: None,
        })?;

        assert_eq!(
            report.validation.runtime_theory_gate,
            AuthoringGateDecisionV1::Blocked,
            "{:?}",
            report.diagnostics
        );
        let summary = &report
            .validation
            .runtime_theory
            .as_ref()
            .expect("runtime theory report")
            .summary;
        assert_eq!(summary.review_only_obligations, 1);
        assert!(!summary.residual_obligation_ids.is_empty());
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "authoring_runtime_theory_blocked"
                && diagnostic.message.contains("review-only obligation")
        }));
        assert!(report.promotion.candidate_reviewable);
        assert!(!report.promotion.protected_main_eligible);
        Ok(())
    }

    #[test]
    fn workspace_service_rejects_path_escape_before_reading() -> Result<()> {
        let (_temp, service) = write_workspace()?;
        let mut request = request();
        request.axi_path = "../outside.axi".to_string();
        let error = service
            .execute(request)
            .expect_err("path escape must reject");
        assert!(error.to_string().contains("workspace-relative"));
        Ok(())
    }

    #[test]
    fn adversarial_olog_path_and_missing_role_block_promotion_review() -> Result<()> {
        let (_temp, service) = write_workspace()?;
        let mut request = request();
        request.operation = AuthoringWorkspaceOperationV1::PromotionReview;
        request.query_ir_v1 = None;
        request.baseline_axi_path = None;
        request.olog_fragment = Some(crate::typed_authoring::OlogFragmentV1 {
            boxes: vec![
                crate::typed_authoring::OlogBoxV1 {
                    box_id: "employee".to_string(),
                    object_type: "Person".to_string(),
                    label: None,
                },
                crate::typed_authoring::OlogBoxV1 {
                    box_id: "team".to_string(),
                    object_type: "Team".to_string(),
                    label: None,
                },
            ],
            relation_boxes: vec![crate::typed_authoring::OlogRelationBoxV1 {
                box_id: "works_for".to_string(),
                relation: "WorksFor".to_string(),
                role_bindings: vec![crate::typed_authoring::OlogRoleBindingV1 {
                    role: "employee".to_string(),
                    target_box: "employee".to_string(),
                }],
            }],
            aspects: vec![crate::typed_authoring::OlogAspectV1 {
                aspect_id: "employer".to_string(),
                from_box: "works_for".to_string(),
                to_box: "team".to_string(),
                kind: crate::typed_authoring::OlogAspectKindV1::RelationProjection {
                    relation_box: "works_for".to_string(),
                    role: "employer".to_string(),
                },
            }],
            path_equations: vec![crate::typed_authoring::OlogPathEquationV1 {
                equation_id: "bad".to_string(),
                lhs: crate::typed_authoring::OlogPathV1 {
                    steps: vec!["employer".to_string(), "employer".to_string()],
                },
                rhs: crate::typed_authoring::OlogPathV1 {
                    steps: vec!["employer".to_string()],
                },
            }],
        });
        let report = service.execute(request)?;
        assert!(!report.ok);
        assert!(!report.typed_holes.olog.is_empty());
        assert!(!report.repairs.is_empty());
        assert!(!report.promotion.candidate_reviewable);
        assert!(!report.promotion.protected_main_eligible);
        Ok(())
    }

    #[test]
    fn runtime_theory_holes_expose_bound_repairs_without_claiming_lean_certification() -> Result<()>
    {
        let (_temp, service) = write_workspace()?;
        let mut request = request();
        request.operation = AuthoringWorkspaceOperationV1::Validate;
        request.axi_text = Some(
            r#"module TheoryDraft
schema S:
  object Person
  relation Link(left: Person, right: Person)

theory T on S:
  constraint functional Link.left -> Link.right
  equation narrative:
    review route = approved route

instance I of S:
  Person = {Alice, Bob}
  Link = {l: (left=Alice, right=Bob)}
"#
            .to_string(),
        );
        request.baseline_axi_path = None;
        request.cq_path = None;
        request.query_ir_v1 = None;

        let report = service.execute(request)?;
        assert_eq!(
            report.validation.runtime_theory_gate,
            AuthoringGateDecisionV1::Blocked
        );
        assert_eq!(report.typed_holes.theory.len(), 1);
        let hole = &report.typed_holes.theory[0];
        assert_eq!(
            hole.runtime_status,
            axiograph_pathdb::RuntimeTheoryCheckStatusV1::ReviewOnly
        );
        assert_eq!(
            hole.lifecycle,
            axiograph_kernel::CheckedLifecycleStateIr::Residual
        );
        assert_eq!(hole.repair_handle_ids.len(), 1);
        assert!(hole.authority.contains("runtime"));
        assert!(hole.lean_certification.contains("not_certified"));

        let repair = report
            .repairs
            .iter()
            .find(|repair| repair.handle.id == hole.repair_handle_ids[0])
            .expect("theory hole must cite an emitted repair");
        assert_eq!(
            repair.kind,
            crate::typed_refinement::RuntimeRefinementCandidateKindV2::AddressTheoryObligation
        );
        assert_eq!(
            repair.handle.domain(),
            crate::typed_refinement::RuntimeRefinementDomainV2::TheoryAuthoring
        );
        repair.handle.validate()?;
        assert_eq!(repair.handle.version, 2);
        assert!(repair.handle.id.starts_with("theory_refine_v2:"));
        assert_eq!(
            repair.theory_obligation_ref.as_ref(),
            Some(&hole.obligation_ref)
        );
        let mut forged = repair.handle.clone();
        forged.id.push_str(":forged");
        assert!(forged.validate().is_err());
        let mut obsolete_version = repair.handle.clone();
        obsolete_version.version = 1;
        assert!(obsolete_version.validate().is_err());
        let mut unbound_op = match repair.handle.payload.clone() {
            crate::typed_refinement::RuntimeRefinementPayloadV2::TheoryAuthoring { op } => op,
            _ => unreachable!("theory repair must carry theory-authoring payload"),
        };
        let crate::typed_refinement::TheoryRefinementOpV1::AddressRuntimeTheoryObligation {
            source_artifact_digest,
            ..
        } = &mut unbound_op;
        source_artifact_digest.clear();
        let unbound = crate::typed_refinement::RuntimeRefinementHandleV2::new_theory(unbound_op);
        assert!(unbound.validate().is_err());

        assert_eq!(report.dependent_refinements.len(), 1);
        let refinements = &report.dependent_refinements[0];
        assert_eq!(refinements.object_membership_witnesses, 3);
        assert_eq!(refinements.role_indexed_witnesses, 2);
        assert_eq!(refinements.typed_constraint_witnesses, 1);
        assert_eq!(
            refinements.lean_certification,
            "not_certified_by_category_kernel_v3"
        );
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.repair_hint.as_deref().is_some_and(|hint| {
                hint.contains(&repair.handle.id) && hint.contains("not Lean certification")
            })
        }));
        Ok(())
    }

    #[test]
    fn http_adapter_returns_the_same_workspace_report_contract() -> Result<()> {
        let (_temp, service) = write_workspace()?;
        let response = execute_http_payload(&service, &serde_json::to_vec(&request())?);
        assert_eq!(response.status(), StatusCode::OK);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let bytes = runtime.block_on(response.into_body().collect())?.to_bytes();
        let value: Value = serde_json::from_slice(&bytes)?;
        assert_eq!(value["version"], json!("authoring_workspace_response_v1"));
        assert_eq!(value["detail"], "summary");
        assert!(value.get("query_explanation").is_none());
        Ok(())
    }

    #[test]
    fn lsp_code_action_executes_the_same_workspace_request() -> Result<()> {
        let (temp, service) = write_workspace()?;
        let path = temp.path().join("domain.axi");
        let uri = url::Url::from_file_path(&path)
            .map_err(|_| anyhow!("build test file URI"))?
            .to_string();
        let text = crate::security::read_utf8_file_bounded(
            &path,
            crate::security::MAX_TEXT_INPUT_BYTES,
            "CLI input",
        )?;
        let mut state = AuthoringWorkspaceLspState {
            service,
            default_axi_path: Some("domain.axi".to_string()),
            documents: BTreeMap::new(),
            document_images_incomplete: false,
            publications: BTreeMap::new(),
        };
        assert!(store_lsp_document(&mut state, &uri, &text, Some(1)));
        let action_response = handle_lsp_request(
            &mut state,
            LspRequest::new(
                1.into(),
                "textDocument/codeAction".to_string(),
                json!({"textDocument": {"uri": uri}}),
            ),
        );
        let Message::Response(action_response) = action_response else {
            panic!("expected code-action response")
        };
        let actions = action_response.response_result.expect("code-action result");
        let request_value = actions[0]["command"]["arguments"][0].clone();
        assert_eq!(
            request_value["version"],
            json!(AUTHORING_WORKSPACE_REQUEST_VERSION_V1)
        );

        let execute_response = handle_lsp_request(
            &mut state,
            LspRequest::new(
                2.into(),
                "workspace/executeCommand".to_string(),
                json!({
                    "command": AUTHORING_WORKSPACE_LSP_COMMAND,
                    "arguments": [request_value]
                }),
            ),
        );
        let Message::Response(execute_response) = execute_response else {
            panic!("expected execute-command response")
        };
        let report = execute_response.response_result.expect("authoring report");
        assert_eq!(report["version"], json!("authoring_workspace_response_v1"));
        assert_eq!(report["validation"]["canonical_axi_valid"], json!(true));
        Ok(())
    }

    #[test]
    fn mcp_and_lsp_publish_only_the_unified_workspace_contract() {
        let tool = authoring_rmcp_tool();
        assert_eq!(tool.name.as_ref(), AUTHORING_WORKSPACE_TOOL_NAME);
        assert_eq!(
            tool.annotations.as_ref().and_then(|a| a.read_only_hint),
            Some(true)
        );
        let capabilities = authoring_workspace_capabilities_v1();
        assert_eq!(
            capabilities["lsp"]["command"],
            json!(AUTHORING_WORKSPACE_LSP_COMMAND)
        );
        assert_eq!(capabilities["http"]["endpoint"], json!("POST /authoring"));
    }
}
