//! Proposals validation (preview import + type/constraint checks).
//!
//! This is for "LLM/UI add-data" flows:
//! - proposals are untrusted and do not mutate the DB directly,
//! - but we still want fast feedback when a proposal would be ill-typed or
//!   violate structured theory constraints (key/functional, etc).
//!
//! The strategy is:
//! 1) clone the current snapshot,
//! 2) import the proposals overlay into the clone (evidence plane),
//! 3) run meta-plane typechecking and tooling-level quality checks,
//! 4) return only *delta* findings introduced by the proposals (baseline vs preview).

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use axiograph_ingest_docs::ProposalsFileV1;
use axiograph_pathdb::axi_semantics::{AxiTypeCheckError, MetaPlaneIndex};
use axiograph_pathdb::PathDB;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalAxiTypecheckErrorV1 {
    pub fact: u32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalAxiTypecheckReportV1 {
    pub skipped: bool,
    pub checked_facts: usize,
    pub errors: Vec<ProposalAxiTypecheckErrorV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalsValidationV1 {
    pub version: String,
    pub import_summary: crate::proposals_import::ImportProposalsSummary,
    pub axi_typecheck: ProposalAxiTypecheckReportV1,
    pub quality_delta: crate::quality::QualityReportV1,
    pub ok: bool,
}

fn proposals_digest(file: &ProposalsFileV1) -> Result<String> {
    let bytes = serde_json::to_vec(file)
        .map_err(|e| anyhow!("failed to serialize proposals for digest: {e}"))?;
    Ok(axiograph_dsl::digest::fnv1a64_digest_bytes(&bytes))
}

fn clone_db(db: &PathDB) -> Result<PathDB> {
    let bytes = db.to_bytes()?;
    PathDB::from_bytes(&bytes)
}

fn typecheck_preview(db: &PathDB) -> ProposalAxiTypecheckReportV1 {
    let meta = MetaPlaneIndex::from_db(db).unwrap_or_default();
    if meta.schemas.is_empty() {
        return ProposalAxiTypecheckReportV1 {
            skipped: true,
            checked_facts: 0,
            errors: Vec::new(),
        };
    }

    let report = meta.typecheck_axi_facts(db);
    let errors = report
        .errors
        .into_iter()
        .filter_map(|e| {
            let fact = match &e {
                AxiTypeCheckError::MissingSchema { fact }
                | AxiTypeCheckError::UnknownSchema { fact, .. }
                | AxiTypeCheckError::UnknownRelation { fact, .. }
                | AxiTypeCheckError::MissingField { fact, .. }
                | AxiTypeCheckError::MultipleFieldValues { fact, .. }
                | AxiTypeCheckError::MissingEntityType { fact, .. }
                | AxiTypeCheckError::FieldTypeMismatch { fact, .. } => *fact,
            };

            // Only surface errors for evidence-plane fact nodes produced by proposals.
            let is_proposal_fact = db
                .get_entity(fact)
                .map(|v| v.attrs.contains_key("proposal_id"))
                .unwrap_or(false);
            if !is_proposal_fact {
                return None;
            }

            Some(ProposalAxiTypecheckErrorV1 {
                fact,
                message: e.to_string(),
            })
        })
        .collect::<Vec<_>>();

    ProposalAxiTypecheckReportV1 {
        skipped: false,
        checked_facts: report.checked_facts,
        errors,
    }
}

fn quality_delta_report(
    base: &PathDB,
    preview: &PathDB,
    profile: &str,
    plane: &str,
) -> Result<crate::quality::QualityReportV1> {
    let profile = profile.trim().to_ascii_lowercase();
    let plane = plane.trim().to_ascii_lowercase();

    let base_report = crate::quality::run_quality_checks(
        base,
        &PathBuf::from("proposals_validate:base"),
        &profile,
        &plane,
    )?;

    let preview_report = crate::quality::run_quality_checks(
        preview,
        &PathBuf::from("proposals_validate:preview"),
        &profile,
        &plane,
    )?;

    fn key(f: &crate::quality::QualityFindingV1) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}",
            f.level,
            f.code,
            f.message,
            f.schema.as_deref().unwrap_or(""),
            f.relation.as_deref().unwrap_or(""),
            f.entity_id.unwrap_or(0),
        )
    }

    let base_keys: HashSet<String> = base_report.findings.iter().map(key).collect();
    let mut delta = preview_report.clone();
    delta.input = "proposals_validate:delta".to_string();
    delta.findings = preview_report
        .findings
        .into_iter()
        .filter(|f| !base_keys.contains(&key(f)))
        .collect();

    // Recompute summary for the delta-only findings.
    let mut summary = crate::quality::QualitySummaryV1::default();
    for f in &delta.findings {
        match f.level.as_str() {
            "error" => summary.error_count += 1,
            "warning" => summary.warning_count += 1,
            "info" => summary.info_count += 1,
            _ => {}
        }
    }
    delta.summary = summary;

    Ok(delta)
}

pub fn validate_proposals_v1(
    base: &PathDB,
    proposals: &ProposalsFileV1,
    quality_profile: &str,
    quality_plane: &str,
) -> Result<ProposalsValidationV1> {
    let digest = proposals_digest(proposals)?;

    let mut preview = clone_db(base)?;
    let import_summary =
        crate::proposals_import::import_proposals_file_into_pathdb(&mut preview, proposals, &digest)?;

    let axi_typecheck = typecheck_preview(&preview);
    let quality_delta = quality_delta_report(base, &preview, quality_profile, quality_plane)?;

    let ok = (!axi_typecheck.skipped && axi_typecheck.errors.is_empty() && quality_delta.summary.error_count == 0)
        || (axi_typecheck.skipped && quality_delta.summary.error_count == 0);

    Ok(ProposalsValidationV1 {
        version: "proposals_validation_v1".to_string(),
        import_summary,
        axi_typecheck,
        quality_delta,
        ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_proposed_parent_fact_is_well_typed_and_has_no_new_errors() -> Result<()> {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
        let base = crate::load_pathdb_for_cli(&repo_root.join("examples/Family.axi"))?;
        let out = crate::proposal_gen::propose_relation_proposals_v1(
            &base,
            &[],
            crate::proposal_gen::ProposeRelationInputV1 {
                // Alias mapping: `child` should resolve to the canonical `Parent(child,parent,...)` relation.
                rel_type: "child".to_string(),
                source_name: "Jamison".to_string(),
                target_name: "Bob".to_string(),
                source_type: None,
                target_type: None,
                source_field: None,
                target_field: None,
                context: Some("FamilyTree".to_string()),
                time: None,
                confidence: Some(0.9),
                schema_hint: Some("Fam".to_string()),
                public_rationale: Some("Jamison is a son of Bob (FamilyTree).".to_string()),
                evidence_text: None,
                evidence_locator: None,
            },
        )?;
        assert_eq!(out.summary.rel_type, "Parent");
        assert!(!out.summary.swapped_endpoints);

        let validation = validate_proposals_v1(&base, &out.proposals, "fast", "both")?;
        assert!(!validation.axi_typecheck.skipped, "expected meta-plane typecheck");
        assert!(
            validation.axi_typecheck.errors.is_empty(),
            "unexpected typecheck errors: {:?}",
            validation.axi_typecheck.errors
        );
        assert_eq!(
            validation.quality_delta.summary.error_count, 0,
            "unexpected new quality errors: {:?}",
            validation.quality_delta.findings
        );
        Ok(())
    }
}
