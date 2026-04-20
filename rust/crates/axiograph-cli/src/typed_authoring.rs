use serde::{Deserialize, Serialize};

use axiograph_pathdb::certificate::AxiWellTypedProofV1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TypedAuthoringTrustV1 {
    pub trust_class: String,
    pub soundness: String,
    pub coverage: String,
    pub scope: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
    #[serde(default = "default_non_claimed")]
    pub completeness_claim: String,
    #[serde(default = "default_non_claimed")]
    pub ontology_closure_claim: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TypedAuthoringSummaryV1 {
    pub lifecycle_state: String,
    pub trust: TypedAuthoringTrustV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axi_well_typed_proof_v1: Option<AxiWellTypedProofV1>,
}

fn default_non_claimed() -> String {
    "not_claimed".to_string()
}

pub fn draft_typed_authoring_summary_from_axi_text(axi_text: &str) -> TypedAuthoringSummaryV1 {
    match axiograph_dsl::schema_v1::parse_schema_v1(axi_text) {
        Ok(module) => match axiograph_pathdb::validate_axi_v1_module(module) {
            Ok(validated) => {
                let proof = validated.proof().clone();
                TypedAuthoringSummaryV1 {
                    lifecycle_state: "validated".to_string(),
                    trust: TypedAuthoringTrustV1 {
                        trust_class: "validated_draft".to_string(),
                        soundness: "rust_side_well_typed_module_check".to_string(),
                        coverage: "draft_module_only".to_string(),
                        scope: "canonical_axi_draft".to_string(),
                        reasons: vec![
                            "draft parses as canonical .axi".to_string(),
                            "module passed the Rust-side well-typed module gate".to_string(),
                            "review/promotion and Lean-side certificate checking are still separate steps".to_string(),
                        ],
                        completeness_claim: default_non_claimed(),
                        ontology_closure_claim: default_non_claimed(),
                    },
                    axi_well_typed_proof_v1: Some(proof),
                }
            }
            Err(err) => TypedAuthoringSummaryV1 {
                lifecycle_state: "draft_only".to_string(),
                trust: TypedAuthoringTrustV1 {
                    trust_class: "draft_only".to_string(),
                    soundness: "not_yet_well_typed".to_string(),
                    coverage: "draft_module_only".to_string(),
                    scope: "canonical_axi_draft".to_string(),
                    reasons: vec![format!(
                        "draft did not pass the Rust-side well-typed module gate: {err}"
                    )],
                    completeness_claim: default_non_claimed(),
                    ontology_closure_claim: default_non_claimed(),
                },
                axi_well_typed_proof_v1: None,
            },
        },
        Err(err) => TypedAuthoringSummaryV1 {
            lifecycle_state: "draft_only".to_string(),
            trust: TypedAuthoringTrustV1 {
                trust_class: "draft_only".to_string(),
                soundness: "not_yet_well_typed".to_string(),
                coverage: "draft_module_only".to_string(),
                scope: "canonical_axi_draft".to_string(),
                reasons: vec![format!("draft did not parse as canonical .axi: {err}")],
                completeness_claim: default_non_claimed(),
                ontology_closure_claim: default_non_claimed(),
            },
            axi_well_typed_proof_v1: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_typed_authoring_summary_reports_validated_module() {
        let axi = r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

instance I of S:
  Person = {Alice, Bob}
  Parent = {(child=Alice, parent=Bob)}
"#;

        let summary = draft_typed_authoring_summary_from_axi_text(axi);
        assert_eq!(summary.lifecycle_state, "validated");
        assert_eq!(summary.trust.trust_class, "validated_draft");
        assert_eq!(summary.trust.completeness_claim, "not_claimed");
        assert_eq!(summary.trust.ontology_closure_claim, "not_claimed");
        assert_eq!(
            summary
                .axi_well_typed_proof_v1
                .as_ref()
                .map(|proof| proof.module_name.as_str()),
            Some("Demo")
        );
    }

    #[test]
    fn draft_typed_authoring_summary_reports_draft_only_parse_failure() {
        let axi = "module Broken\nschema S:\n  object Person\ninstance I of S:\n  Parent = {oops}";
        let summary = draft_typed_authoring_summary_from_axi_text(axi);
        assert_eq!(summary.lifecycle_state, "draft_only");
        assert_eq!(summary.trust.trust_class, "draft_only");
        assert!(summary.axi_well_typed_proof_v1.is_none());
        assert!(summary
            .trust
            .reasons
            .iter()
            .any(|reason| reason.contains("did not pass") || reason.contains("did not parse")));
    }
}
