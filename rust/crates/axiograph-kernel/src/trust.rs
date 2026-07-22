//! Trust vocabulary shared across runtime reports.
//!
//! These values classify claims; they do not create Lean authority.

use crate::{CheckerIdV2, RevisionDigestV2};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustClassV2 {
    RuntimeValidated,
    FiniteModelChecked,
    LeanReceiptVerified,
    EvidenceOnly,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExplicitNonClaimV2 {
    Univalence,
    HigherInductiveTypes,
    ArbitraryHigherCategories,
    GeneralDependentTypeTheory,
    OpenWorldEntailment,
    OntologyClosure,
    QueryFrontendLoweringCorrectness,
    BackendCompleteness,
    SigmaPiMigration,
    KanExtension,
    ConfidenceInvariantUnderReassociation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TrustBasisV2 {
    pub revision: RevisionDigestV2,
    pub trust_class: TrustClassV2,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checker_id: Option<CheckerIdV2>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_claims: Vec<ExplicitNonClaimV2>,
}
