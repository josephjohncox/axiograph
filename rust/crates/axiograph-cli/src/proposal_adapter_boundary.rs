//! Bounded typed boundary for predictive-proposal adapter responses.

use anyhow::{anyhow, Result};
use axiograph_ingest_docs::{validate_proposals_file_v1, ProposalsFileV1};
use axiograph_pathdb::ProposalAdapterRunId;
use serde::{Deserialize, Serialize};

pub const PREDICTIVE_PROPOSAL_PROTOCOL_V1: &str = "axiograph_predictive_proposal_v1";
pub const MAX_PREDICTIVE_PROPOSAL_RESPONSE_NOTES: usize = 1_024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PredictiveProposalResponseV1 {
    pub protocol: String,
    pub trace_id: ProposalAdapterRunId,
    pub generated_at_unix_secs: u64,
    pub proposals: ProposalsFileV1,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn validate_predictive_proposal_response_shape(
    response: &PredictiveProposalResponseV1,
) -> Result<()> {
    if response.protocol != PREDICTIVE_PROPOSAL_PROTOCOL_V1 {
        return Err(anyhow!(
            "predictive proposal response protocol must be `{PREDICTIVE_PROPOSAL_PROTOCOL_V1}`"
        ));
    }
    if response.notes.len() > MAX_PREDICTIVE_PROPOSAL_RESPONSE_NOTES {
        return Err(anyhow!(
            "predictive proposal response note count exceeds {MAX_PREDICTIVE_PROPOSAL_RESPONSE_NOTES}"
        ));
    }
    validate_proposals_file_v1(&response.proposals)
}

pub fn parse_predictive_proposal_response_bounded(
    bytes: &[u8],
    limit: usize,
    label: &str,
) -> Result<PredictiveProposalResponseV1> {
    let response = axiograph_security::parse_json_bounded(bytes, limit, label)?;
    validate_predictive_proposal_response_shape(&response)?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_EMPTY_RESPONSE: &str = r#"{
      "protocol": "axiograph_predictive_proposal_v1",
      "trace_id": "proposal::test",
      "generated_at_unix_secs": 0,
      "proposals": {
        "version": 1,
        "generated_at": "0",
        "source": {
          "source_type": "predictive_proposal_adapter",
          "locator": "proposal::test"
        },
        "schema_hint": null,
        "proposals": []
      },
      "notes": [],
      "error": null
    }"#;

    #[test]
    fn accepted_empty_response_reaches_shape_validation() {
        let response = parse_predictive_proposal_response_bounded(
            VALID_EMPTY_RESPONSE.as_bytes(),
            64 * 1024,
            "test response",
        )
        .expect("valid response");
        assert!(response.proposals.proposals.is_empty());
    }

    #[test]
    fn unknown_response_fields_reject_at_the_plugin_boundary() {
        let tampered = VALID_EMPTY_RESPONSE.replace(
            "\"error\": null",
            "\"error\": null, \"unreviewed_extension\": true",
        );
        let error = parse_predictive_proposal_response_bounded(
            tampered.as_bytes(),
            64 * 1024,
            "test response",
        )
        .expect_err("unknown adapter fields must reject");
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn production_byte_limit_accepts_exactly_n_and_rejects_n_plus_one() {
        const LIMIT: usize = 8 * 1024 * 1024;
        const MARKER: &str = "\"error\": null";
        let empty_error = VALID_EMPTY_RESPONSE.replace(MARKER, "\"error\": \"\"");
        let padding_len = LIMIT
            .checked_sub(empty_error.len())
            .expect("test fixture must fit below production limit");
        let payload = VALID_EMPTY_RESPONSE.replace(
            MARKER,
            &format!("\"error\": \"{}\"", "x".repeat(padding_len)),
        );
        assert_eq!(payload.len(), LIMIT);
        parse_predictive_proposal_response_bounded(payload.as_bytes(), LIMIT, "test response")
            .expect("exact production limit must be accepted");

        let mut oversized = payload.into_bytes();
        oversized.push(b' ');
        let error = parse_predictive_proposal_response_bounded(&oversized, LIMIT, "test response")
            .expect_err("N+1 response bytes must reject");
        assert!(error.to_string().contains("exceeds"));
    }
}
