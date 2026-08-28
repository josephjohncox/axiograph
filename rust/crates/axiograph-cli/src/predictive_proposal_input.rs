//! Predictive proposal inputs from exact canonical `.axi` bytes.
//!
//! Derived PathDB rows are never reverse-exported into accepted meaning.

use anyhow::Result;
use axiograph_kernel::MaterializationIdV2;
use axiograph_pathdb::AcceptedSnapshotId;

pub(crate) fn build_predictive_proposal_input_from_axi_text(
    axi_text: &str,
    module_name: Option<String>,
    materialization_id: Option<MaterializationIdV2>,
    accepted_snapshot_id: Option<AcceptedSnapshotId>,
    training_export: Option<crate::predictive_proposals::MaskedTupleTrainingExportOptionsV1>,
) -> Result<crate::predictive_proposals::PredictiveProposalInputV1> {
    let canonical = crate::axi_input::require_canonical_axi_text(axi_text)?;
    let mut input = crate::predictive_proposals::PredictiveProposalInputV1 {
        revision_digest_v2: Some(canonical.digest().clone()),
        axi_module_text: Some(axi_text.to_string()),
        ..Default::default()
    };
    input.set_canonical_axi_semantics(
        module_name.or_else(|| Some(canonical.module().module().module_name.clone())),
        materialization_id,
        accepted_snapshot_id,
    );
    input.notes.push(format!(
        "semantic_input={}",
        crate::predictive_proposals::PREDICTIVE_PROPOSAL_SEMANTIC_INPUT_KIND_V1
    ));
    if let Some(module_name) = input.semantic_input.module_name.as_ref() {
        input.notes.push(format!("semantic_module={module_name}"));
    }
    if let Some(export_opts) = training_export.as_ref() {
        let export = crate::predictive_proposals::build_training_export_from_axi_text(
            axi_text,
            export_opts,
        )?;
        input.set_training_export_layer(export);
    }
    Ok(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_bytes_build_the_semantic_envelope() {
        let axi = r#"
module Demo
schema Demo:
  object Person
instance DemoInst of Demo:
  Person = {Alice, Bob}
"#;
        let input = build_predictive_proposal_input_from_axi_text(
            axi,
            None,
            Some(MaterializationIdV2::from_canonical_fields(&[
                b"proposal-input-test-materialization",
            ])),
            Some(AcceptedSnapshotId::new("accepted:test")),
            None,
        )
        .expect("canonical proposal input");
        assert_eq!(input.semantic_input.module_name.as_deref(), Some("Demo"));
        assert_eq!(input.axi_module_text.as_deref(), Some(axi));
    }
}
