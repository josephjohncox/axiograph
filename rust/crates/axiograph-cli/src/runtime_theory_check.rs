use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use axiograph_pathdb::{
    check_runtime_theory_with_options_v1, default_evidence_policy_v1, default_world_assumption_v1,
    EvidencePolicyV1, EvidenceWeightSemanticsV1, RuntimeTheoryCheckReportV1,
    RuntimeTheoryCheckStatusV1, RuntimeTheoryClosureTierV1, WorldAssumptionV1,
};

pub(crate) fn parse_runtime_theory_closure_tier(raw: &str) -> Result<RuntimeTheoryClosureTierV1> {
    raw.parse().map_err(anyhow::Error::msg)
}

pub(crate) fn parse_evidence_weight_semantics(raw: &str) -> Result<EvidenceWeightSemanticsV1> {
    raw.parse().map_err(anyhow::Error::msg)
}

pub(crate) fn parse_evidence_weight_assignment(raw: &str) -> Result<(String, u32)> {
    let (obligation_id, ppm) = raw
        .split_once('=')
        .ok_or_else(|| anyhow!("evidence weight `{raw}` must be obligation_id=ppm"))?;
    let ppm = ppm
        .parse::<u32>()
        .map_err(|err| anyhow!("invalid evidence weight ppm in `{raw}`: {err}"))?;
    if ppm > 1_000_000 {
        return Err(anyhow!(
            "invalid evidence weight `{raw}`: ppm must be <= 1000000"
        ));
    }
    Ok((obligation_id.to_string(), ppm))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RuntimeTheoryCheckInputV1 {
    pub axi_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closure_tier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finite_world: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub included_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub included_worlds: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub included_slices: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub included_imports: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub undeclared_imports: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_threshold_ppm: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_semantics: Option<String>,
    #[serde(default)]
    pub weighted_evidence: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_weights: Vec<String>,
}

#[cfg(test)]
pub(crate) use axiograph_tooling_overlays::RuntimeTheoryScopeSummaryV1;
pub(crate) use axiograph_tooling_overlays::{
    RuntimeTheoryCheckSummaryV1, RuntimeTheoryNonClaimSummaryV1,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RuntimeTheoryCheckModuleReportV1 {
    pub version: String,
    pub module_digest: String,
    pub summary: RuntimeTheoryCheckSummaryV1,
    pub reports: Vec<RuntimeTheoryCheckReportV1>,
    pub blocking_errors: usize,
    pub trust_boundary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_claims: Vec<RuntimeTheoryNonClaimSummaryV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

pub(crate) fn runtime_theory_check_reports_from_axi_text_with_assumptions(
    axi_text: &str,
    theory_filter: Option<&str>,
    closure_tier: RuntimeTheoryClosureTierV1,
    world: WorldAssumptionV1,
    evidence_policy: EvidencePolicyV1,
) -> Result<RuntimeTheoryCheckModuleReportV1> {
    let canonical = crate::axi_input::require_canonical_axi_text(axi_text)?;
    let kernel =
        axiograph_pathdb::derive_runtime_module_index(canonical.module().module(), axi_text)
            .map_err(|err| anyhow!("failed to derive runtime module index: {err}"))?;
    runtime_theory_check_reports_from_runtime_index(
        &kernel,
        canonical.digest().to_string(),
        theory_filter,
        closure_tier,
        world,
        evidence_policy,
    )
}

pub(crate) fn runtime_theory_check_reports_from_package(
    package: &crate::axi_input::CanonicalAxiPackage,
    theory_filter: Option<&str>,
    closure_tier: RuntimeTheoryClosureTierV1,
    world: WorldAssumptionV1,
    evidence_policy: EvidencePolicyV1,
) -> Result<RuntimeTheoryCheckModuleReportV1> {
    let sources = package.ordered_sources();
    let kernel = axiograph_pathdb::derive_runtime_package_index(package.snapshot(), &sources)
        .map_err(|err| anyhow!("failed to derive runtime package index: {err}"))?;
    runtime_theory_check_reports_from_runtime_index(
        &kernel,
        kernel.module_digest.to_string(),
        theory_filter,
        closure_tier,
        world,
        evidence_policy,
    )
}

fn runtime_theory_check_reports_from_runtime_index(
    kernel: &axiograph_pathdb::RuntimeModuleIndex,
    module_digest: String,
    theory_filter: Option<&str>,
    closure_tier: RuntimeTheoryClosureTierV1,
    world: WorldAssumptionV1,
    evidence_policy: EvidencePolicyV1,
) -> Result<RuntimeTheoryCheckModuleReportV1> {
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
            world.clone(),
            evidence_policy.clone(),
            None,
        ));
    }

    reports.sort_by_key(|a| a.theory_ref.stable_id());
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
    let version = "runtime_theory_check_module_report_v1".to_string();
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
        notes.clone(),
    );

    let non_claims = summary.non_claims.clone();
    Ok(RuntimeTheoryCheckModuleReportV1 {
        version,
        module_digest,
        summary,
        reports,
        blocking_errors,
        trust_boundary: "Runtime checked in Rust; not Lean verified.".to_string(),
        non_claims,
        notes,
    })
}

pub(crate) fn runtime_theory_check_summary_from_input(
    input: &RuntimeTheoryCheckInputV1,
) -> Result<RuntimeTheoryCheckSummaryV1> {
    Ok(runtime_theory_check_reports_from_input(input)?.summary)
}

pub(crate) fn runtime_theory_check_reports_from_input(
    input: &RuntimeTheoryCheckInputV1,
) -> Result<RuntimeTheoryCheckModuleReportV1> {
    let closure_tier = parse_runtime_theory_closure_tier(
        input.closure_tier.as_deref().unwrap_or("finite_fragment"),
    )?;
    let (world, evidence_policy) = runtime_theory_assumptions_from_input(input)?;
    runtime_theory_check_reports_from_axi_text_with_assumptions(
        &input.axi_text,
        input.theory.as_deref(),
        closure_tier,
        world,
        evidence_policy,
    )
}

pub(crate) fn runtime_theory_assumptions_from_input(
    input: &RuntimeTheoryCheckInputV1,
) -> Result<(WorldAssumptionV1, EvidencePolicyV1)> {
    let mut world = default_world_assumption_v1();
    if let Some(world_id) = input.world_id.as_ref() {
        world.world_id = world_id.clone();
    }
    world.finite = input.finite_world.unwrap_or(true);
    if !input.included_refs.is_empty() {
        world.included_refs = input.included_refs.clone();
    }
    if !input.included_worlds.is_empty() {
        world.included_worlds = input.included_worlds.clone();
    }
    if !input.included_slices.is_empty() {
        world.included_slices = input.included_slices.clone();
    }
    if !input.included_imports.is_empty() {
        world.included_imports = input.included_imports.clone();
    }
    if !input.undeclared_imports.is_empty() {
        world.undeclared_imports = input.undeclared_imports.clone();
    }

    let mut evidence_policy = default_evidence_policy_v1();
    if let Some(threshold) = input.evidence_threshold_ppm {
        if threshold > 1_000_000 {
            return Err(anyhow!(
                "invalid evidence_threshold_ppm {threshold}: must be <= 1000000"
            ));
        }
        evidence_policy.threshold_ppm = threshold;
    }
    evidence_policy.semantics = if input.weighted_evidence {
        EvidenceWeightSemanticsV1::WeightedLattice
    } else {
        parse_evidence_weight_semantics(
            input
                .evidence_semantics
                .as_deref()
                .unwrap_or("thresholded_world"),
        )?
    };
    evidence_policy.weighted_propagation_enabled = input.weighted_evidence;
    for raw in &input.evidence_weights {
        let (obligation_id, ppm) = parse_evidence_weight_assignment(raw)?;
        evidence_policy
            .obligation_weights_ppm
            .insert(obligation_id, ppm);
    }

    Ok((world, evidence_policy))
}

pub(crate) fn runtime_theory_check_summary_from_reports(
    module_digest: &str,
    reports: &[RuntimeTheoryCheckReportV1],
    blocking_errors: usize,
    notes: Vec<String>,
) -> RuntimeTheoryCheckSummaryV1 {
    axiograph_tooling_overlays::runtime_theory_check_summary_v1(
        module_digest,
        reports,
        blocking_errors,
        notes,
    )
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
    lines.push(format!("anchor: revision_digest={}", report.module_digest));
    let mut worlds: Vec<String> = report
        .reports
        .iter()
        .map(|report| {
            format!(
                "{}{}",
                report.world.world_id,
                if report.world.finite {
                    " (finite)"
                } else {
                    " (open/advisory)"
                }
            )
        })
        .collect();
    worlds.sort();
    worlds.dedup();
    let mut evidence_policies: Vec<String> = report
        .reports
        .iter()
        .map(|report| {
            format!(
                "{}:{}ppm:{}{}",
                report.evidence_policy.policy_id,
                report.evidence_policy.threshold_ppm,
                report.evidence_policy.semantics.as_str(),
                if report.evidence_policy.weighted_propagation_enabled {
                    ":weighted"
                } else {
                    ""
                }
            )
        })
        .collect();
    evidence_policies.sort();
    evidence_policies.dedup();
    lines.push(format!(
        "scope: fragments={}, worlds={}, evidence_policies={}",
        list_or_none(&report.summary.scope.fragments),
        list_or_none(&worlds),
        list_or_none(&evidence_policies)
    ));
    lines.push(format!(
        "admissibility trace: steps={}, checked_seed={}, evidence_filtered={}, review_residual={}, blocking={}, scan_complete={}",
        report.summary.admissibility_trace.total_steps,
        report.summary.admissibility_trace.checked_seed_steps,
        report.summary.admissibility_trace.evidence_filtered_steps,
        report.summary.admissibility_trace.review_residual_steps,
        report.summary.admissibility_trace.blocking_error_steps,
        report.summary.admissibility_trace.admissibility_scan_complete_steps,
    ));
    if report
        .summary
        .transport_summary
        .resolver_required_obligations
        > 0
        || report.summary.transport_summary.preserved_obligations > 0
    {
        lines.push(format!(
            "transport: preserved={}, transported={}, missing_object={}, missing_arrow={}, opaque={}, resolver_required={}",
            report.summary.transport_summary.preserved_obligations,
            report.summary.transport_summary.transported_obligations,
            report.summary.transport_summary.missing_object_image_obligations,
            report.summary.transport_summary.missing_arrow_image_obligations,
            report.summary.transport_summary.opaque_or_out_of_fragment_obligations,
            report.summary.transport_summary.resolver_required_obligations,
        ));
    }
    for theory_report in &report.reports {
        lines.push(format!(
            "- {}: checked={}, review_only={}, residual={}, blocked={}, saturation=not_run",
            theory_report.theory_ref.display_name(),
            theory_report.checked_obligations,
            theory_report.review_only_obligations,
            theory_report.residual_obligations,
            theory_report.blocked_obligations,
        ));
    }
    if report.blocking_errors > 0 {
        lines.push(
            "next: resolve blocking RuntimeTheoryCheckReportV1 judgments before promotion; rerun with --json for typed handles"
                .to_string(),
        );
    } else if report.summary.residual_obligations > 0 || report.summary.review_only_obligations > 0
    {
        lines.push(
            "next: review residual/review-only obligations; this admissibility scan does not compute saturation"
                .to_string(),
        );
    } else {
        lines.push(
            "next: runtime admissibility checks passed; closure requires a separate replayable finite certificate"
                .to_string(),
        );
    }
    lines.join("\n")
}

fn list_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "<none>".to_string()
    } else {
        values.join(", ")
    }
}
