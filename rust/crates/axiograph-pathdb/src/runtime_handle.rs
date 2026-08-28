//! Stable semantic identifiers and anchors for PathDB-adjacent workflows.
//!
//! These types are intentionally separate from `DbToken` / `DbBranded<T>`:
//!
//! - `DbToken` is process-local and protects in-memory misuse.
//! - the identifiers in this module are persistent, serializable semantic ids.
//!
//! They are the Rust-side foothold for:
//!
//! - accepted snapshot anchoring,
//! - proposal-adapter lineage,
//! - schema-scoped typed execution,
//! - and semantic VCS history.
//!
//! ```compile_fail
//! use axiograph_pathdb::{AcceptedAxiAnchor, AxiDigest, MaterializationIdV2};
//!
//! let _ = AcceptedAxiAnchor::new(
//!     MaterializationIdV2::from_canonical_fields(&[b"materialization:42"]),
//!     AxiDigest::from_axi_text("module Demo\n"),
//! );
//! ```

use axiograph_kernel::{revision_digest_v2, FACT_ID_V2_PREFIX, REVISION_ID_V2_PREFIX};
pub use axiograph_kernel::{
    AnswerIdV2, CertificateIdV2, CommitIdV2, ConstraintIdV2, EquationIdV2, FactIdV2, InstanceIdV2,
    KernelRefV2, MaterializationIdV2, ObjectBlobIdV2, ObjectTypeIdV2, QueryIdV2,
    ReconciliationIdV2, RelationIdV2, RevisionDigestV2, RewriteRuleIdV2, RoleIdV2, SchemaIdV2,
    SnapshotIdV2, TheoryIdV2,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! runtime_handle_type {
    ($name:ident) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }
    };
}

runtime_handle_type!(AxiDigest);
runtime_handle_type!(AcceptedSnapshotId);
runtime_handle_type!(ProposalDigest);
runtime_handle_type!(ProposalAdapterRunId);
runtime_handle_type!(SchemaId);
runtime_handle_type!(TheoryId);
runtime_handle_type!(InstanceId);
runtime_handle_type!(ObjectTypeId);
runtime_handle_type!(RelationId);
runtime_handle_type!(RoleId);
runtime_handle_type!(ConstraintId);
runtime_handle_type!(EquationId);
runtime_handle_type!(RewriteRuleId);
runtime_handle_type!(ContextId);
runtime_handle_type!(StableFactId);

impl AxiDigest {
    /// Compute a stable digest for canonical `.axi` text.
    pub fn from_axi_text(text: &str) -> Self {
        Self(revision_digest_v2(text))
    }

    pub fn is_revision_id_v2(&self) -> bool {
        self.0.starts_with(REVISION_ID_V2_PREFIX)
    }
}

impl StableFactId {
    pub fn is_fact_id_v2(&self) -> bool {
        self.0.starts_with(FACT_ID_V2_PREFIX)
    }
}

/// A semantic anchor identifying one accepted module inside one accepted
/// snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AcceptedAxiAnchor {
    pub accepted_snapshot_id: AcceptedSnapshotId,
    pub axi_digest: AxiDigest,
}

impl AcceptedAxiAnchor {
    pub fn new(accepted_snapshot_id: AcceptedSnapshotId, axi_digest: AxiDigest) -> Self {
        Self {
            accepted_snapshot_id,
            axi_digest,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct SemanticIdFixture {
        schema_id: SchemaId,
        theory_id: TheoryId,
        object_type_id: ObjectTypeId,
        relation_id: RelationId,
        role_id: RoleId,
        constraint_id: ConstraintId,
        equation_id: EquationId,
        rewrite_rule_id: RewriteRuleId,
        context_id: ContextId,
        proposal_digest: ProposalDigest,
    }

    #[test]
    fn axi_digest_from_text_uses_expected_prefix() {
        let digest = AxiDigest::from_axi_text("module X\n");
        assert!(digest.is_revision_id_v2());
        assert!(digest.as_str().starts_with(REVISION_ID_V2_PREFIX));
    }

    #[test]
    fn stable_fact_id_prefix_check_is_available() {
        let fact_id = StableFactId::new(axiograph_kernel::runtime_fact_id_v2(
            "M",
            "S",
            "I",
            "R",
            &[("value", "x")],
        ));
        assert!(fact_id.is_fact_id_v2());
    }

    #[test]
    fn accepted_anchor_round_trips_fields() {
        let anchor = AcceptedAxiAnchor::new(
            AcceptedSnapshotId::new("accepted:123"),
            AxiDigest::from_axi_text("module X\n"),
        );
        assert_eq!(anchor.accepted_snapshot_id.as_str(), "accepted:123");
        assert!(anchor.axi_digest.is_revision_id_v2());
    }

    #[test]
    fn semantic_id_newtypes_round_trip_with_stable_json_wire_shape() {
        let fixture = SemanticIdFixture {
            schema_id: SchemaId::new("schema:demo"),
            theory_id: TheoryId::new("theory:demo"),
            object_type_id: ObjectTypeId::new("object:demo:Thing"),
            relation_id: RelationId::new("relation:demo:Rel"),
            role_id: RoleId::new("role:demo:Rel:from"),
            constraint_id: ConstraintId::new("constraint:demo:T:0"),
            equation_id: EquationId::new("equation:demo:T:eq"),
            rewrite_rule_id: RewriteRuleId::new("rewrite:demo:T:r"),
            context_id: ContextId::new("context:demo"),
            proposal_digest: ProposalDigest::new("proposal:deadbeef"),
        };

        let value = serde_json::to_value(&fixture).expect("semantic ids should serialize");
        assert_eq!(
            value,
            json!({
                "schema_id": "schema:demo",
                "theory_id": "theory:demo",
                "object_type_id": "object:demo:Thing",
                "relation_id": "relation:demo:Rel",
                "role_id": "role:demo:Rel:from",
                "constraint_id": "constraint:demo:T:0",
                "equation_id": "equation:demo:T:eq",
                "rewrite_rule_id": "rewrite:demo:T:r",
                "context_id": "context:demo",
                "proposal_digest": "proposal:deadbeef"
            })
        );

        let round_trip: SemanticIdFixture =
            serde_json::from_value(value).expect("semantic ids should deserialize");
        assert_eq!(round_trip, fixture);
    }
}
