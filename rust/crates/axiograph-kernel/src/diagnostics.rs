//! Shared typed diagnostics emitted by runtime validation and authoring tools.

use crate::{ObligationIdV2, RevisionDigestV2, SemanticKeyV2};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverityV2 {
    Error,
    Warning,
    Information,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DiagnosticSubjectV2 {
    Revision { revision: RevisionDigestV2 },
    SemanticKey { semantic_key: SemanticKeyV2 },
    Obligation { obligation_id: ObligationIdV2 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KernelDiagnosticV2 {
    pub code: String,
    pub severity: DiagnosticSeverityV2,
    pub message: String,
    pub subjects: Vec<DiagnosticSubjectV2>,
}
