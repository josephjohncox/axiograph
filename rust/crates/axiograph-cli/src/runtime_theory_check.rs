use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use axiograph_pathdb::{
    check_runtime_theory_with_options_v1, default_evidence_policy_v1,
    default_world_assumption_v1, RuntimeTheoryCheckReportV1, RuntimeTheoryCheckStatusV1,
    RuntimeTheoryClosureTierV1,
};

pub(crate) fn parse_runtime_theory_closure_tier(
    raw: &str,
) -> Result<RuntimeTheoryClosureTierV1> {
    match raw {
        "finite_fragment" | "finite" => Ok(RuntimeTheoryClosureTierV1::FiniteFragment),
        "evidence_weighted" | "evidence" => Ok(RuntimeTheoryClosureTierV1::EvidenceWeighted),
        "global_indexed" | "global" => Ok(RuntimeTheoryClosureTierV1::GlobalIndexed),
        other => Err(anyhow!(
            "unknown closure tier `{other}` (expected finite_fragment, evidence_weighted, or global_indexed)"
        )),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RuntimeTheoryCheckInputV1 {
    pub axi_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closure_tier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RuntimeTheoryCheckSummaryV1 {
    pub version: String,
    pub report_version: String,
    pub module_digest: String,
    #[serde(default)]
    pub theory_count: usize,
    #[serde(default)]
    pub checked_obligations: usize,
    #[serde(default)]
    pub review_only_obligations: usize,
    #[serde(default)]
    pub residual_obligations: usize,
    #[serde(default)]
    pub blocked_obligations: usize,
    #[serde(default)]
    pub excluded_by_evidence: usize,
    #[serde(default)]
    pub blocking_errors: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub closure_tiers: Vec<String>,
    pub completeness_claim: String,
    pub ontology_closure_claim: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_obligation_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RuntimeTheoryCheckModuleReportV1 {
    pub version: String,
    pub module_digest: String,
    pub summary: RuntimeTheoryCheckSummaryV1,
    pub reports: Vec<RuntimeTheoryCheckReportV1>,
    pub blocking_errors: usize,
    pub trust_boundary: String,
    pub completeness_claim: String,
    pub ontology_closure_claim: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

pub(crate) fn runtime_theory_check_reports_from_axi_text(
    axi_text: &str,
    theory_filter: Option<&str>,
    closure_tier: RuntimeTheoryClosureTierV1,
) -> Result<RuntimeTheoryCheckModuleReportV1> {
    let canonical = crate::axi_input::require_canonical_axi_text(axi_text)?;
    let kernel = axiograph_pathdb::compile_kernel_module_ir(canonical.module().module(), axi_text)
        .map_err(|err| anyhow!("failed to compile KernelModuleIr: {err}"))?;

    let mut reports = Vec::new();
    for theory in kernel.theories.iter().filter(|theory| {
        let Some(filter) = theory_filter else {
            return true;
        };
        theory.theory_id.as_str() == filter
            || theory
                .theory_id
                .as_str()
                .rsplit_once(':')
                .is_some_and(|(_, local)| local == filter)
    }) {
        let schema = kernel
            .schemas
            .iter()
            .find(|schema| schema.schema_id == theory.schema_id)
            .ok_or_else(|| {
                anyhow!(
                    "compiled theory `{}` references missing schema `{}`",
                    theory.theory_id,
                    theory.schema_id
                )
            })?;
        reports.push(check_runtime_theory_with_options_v1(
            schema,
            theory,
            closure_tier,
            default_world_assumption_v1(),
            default_evidence_policy_v1(),
            None,
        ));
    }

    reports.sort_by(|a, b| a.theory_ref.stable_id().cmp(&b.theory_ref.stable_id()));
    if reports.is_empty() {
        return Err(anyhow!(
            "no compiled theories matched{}",
            theory_filter
                .map(|filter| format!(" `{filter}`"))
                .unwrap_or_default()
        ));
    }

    let blocking_errors = reports
        .iter()
        .flat_map(|report| report.judgments.iter())
        .filter(|judgment| judgment.status == RuntimeTheoryCheckStatusV1::Blocked)
        .count();
    let all_complete = reports
        .iter()
        .all(|report| report.completeness_claim.claimed);
    let all_closed = reports
        .iter()
        .all(|report| report.ontology_closure_claim.claimed);

    let version = "runtime_theory_check_module_report_v1".to_string();
    let module_digest = canonical.digest().to_string();
    let completeness_claim = if all_complete {
        format!("claimed_under_{}", closure_tier.as_str())
    } else {
        "not_claimed_for_all_obligations".to_string()
    };
    let ontology_closure_claim = if all_closed {
        format!("claimed_under_{}", closure_tier.as_str())
    } else {
        "not_claimed_for_all_obligations".to_string()
    };
    let notes = vec![
        "runtime theory check reports are typed operational artifacts, not Lean certificates"
            .to_string(),
        "blocking judgments should fail promotion/check gates; review-only judgments remain explicit weak claims"
            .to_string(),
    ];
    let summary = runtime_theory_check_summary_from_reports(
        &module_digest,
        &reports,
        blocking_errors,
        completeness_claim.clone(),
        ontology_closure_claim.clone(),
        notes.clone(),
    );

    Ok(RuntimeTheoryCheckModuleReportV1 {
        version,
        module_digest,
        summary,
        reports,
        blocking_errors,
        trust_boundary: "rust_runtime_operational_not_lean_certificate".to_string(),
        completeness_claim,
        ontology_closure_claim,
        notes,
    })
}

pub(crate) fn runtime_theory_check_summary_from_input(
    input: &RuntimeTheoryCheckInputV1,
) -> Result<RuntimeTheoryCheckSummaryV1> {
    let closure_tier = parse_runtime_theory_closure_tier(
        input.closure_tier.as_deref().unwrap_or("finite_fragment"),
    )?;
    Ok(runtime_theory_check_reports_from_axi_text(
        &input.axi_text,
        input.theory.as_deref(),
        closure_tier,
    )?
    .summary)
}

pub(crate) fn runtime_theory_check_summary_from_reports(
    module_digest: &str,
    reports: &[RuntimeTheoryCheckReportV1],
    blocking_errors: usize,
    completeness_claim: String,
    ontology_closure_claim: String,
    mut notes: Vec<String>,
) -> RuntimeTheoryCheckSummaryV1 {
    let mut closure_tiers = reports
        .iter()
        .map(|report| report.fragment.closure_tier.as_str().to_string())
        .collect::<Vec<_>>();
    closure_tiers.sort();
    closure_tiers.dedup();
    let mut residual_obligation_ids = reports
        .iter()
        .flat_map(|report| report.closure.residual_obligations.iter().cloned())
        .collect::<Vec<_>>();
    residual_obligation_ids.sort();
    residual_obligation_ids.dedup();
    if blocking_errors > 0 {
        notes.push(format!(
            "{blocking_errors} runtime theory judgment(s) are blocking"
        ));
    }

    RuntimeTheoryCheckSummaryV1 {
        version: "runtime_theory_check_summary_v1".to_string(),
        report_version: axiograph_pathdb::RUNTIME_THEORY_CHECK_REPORT_VERSION_V1.to_string(),
        module_digest: module_digest.to_string(),
        theory_count: reports.len(),
        checked_obligations: reports
            .iter()
            .map(|report| report.checked_obligations)
            .sum(),
        review_only_obligations: reports
            .iter()
            .map(|report| report.review_only_obligations)
            .sum(),
        residual_obligations: reports
            .iter()
            .map(|report| report.residual_obligations)
            .sum(),
        blocked_obligations: reports
            .iter()
            .map(|report| report.blocked_obligations)
            .sum(),
        excluded_by_evidence: reports
            .iter()
            .map(|report| report.excluded_by_evidence)
            .sum(),
        blocking_errors,
        closure_tiers,
        completeness_claim,
        ontology_closure_claim,
        residual_obligation_ids,
        notes,
    }
}

pub(crate) fn runtime_theory_check_human_summary(
    report: &RuntimeTheoryCheckModuleReportV1,
) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "runtime theory check: {} report(s), {} blocking error(s)",
        report.reports.len(),
        report.blocking_errors
    ));
    for theory_report in &report.reports {
        lines.push(format!(
            "- {}: checked={}, review_only={}, residual={}, blocked={}, closure_complete={}, ontology_closed={}",
            theory_report.theory_ref.display_name(),
            theory_report.checked_obligations,
            theory_report.review_only_obligations,
            theory_report.residual_obligations,
            theory_report.blocked_obligations,
            theory_report.closure.complete,
            theory_report.closure.closed,
        ));
    }
    lines.join("\n")
}
