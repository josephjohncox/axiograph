//! Presentation only: canonical reports and their certificate/repair identities are unchanged.
//! Cursors commit to exact inputs, not acceptance authority or client authentication.
use super::*;
use sha2::{Digest, Sha256};

#[cfg(test)]
mod tests;

const RESPONSE_VERSION: &str = "authoring_workspace_response_v1";
const CURSOR_VERSION: &str = "authoring-page-v1";
const MAX_CURSOR_BYTES: usize = 256;
const MAX_PAGE_LIMIT: usize = 100;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AuthoringDetailV1 {
    #[default]
    Summary,
    Standard,
    Full,
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum,
)]
#[serde(rename_all = "snake_case")]
#[value(rename_all = "snake_case")]
pub(crate) enum AuthoringSectionV1 {
    Diagnostics,
    StableRuntimeRefs,
    Repairs,
    OlogHoles,
    QueryHoles,
    CompetencyHoles,
    TheoryHoles,
    DependentRefinements,
    EvolutionPreviews,
    CompetencyQuestions,
    RuntimeTheoryReports,
    Validation,
    PreparedQuery,
    QueryExplanation,
    AppliedQueryRepair,
    CheckedOlog,
    AppliedOlogRepair,
    CompetencyEvaluation,
}

const SECTIONS: &[AuthoringSectionV1] = &[
    AuthoringSectionV1::Diagnostics,
    AuthoringSectionV1::StableRuntimeRefs,
    AuthoringSectionV1::Repairs,
    AuthoringSectionV1::OlogHoles,
    AuthoringSectionV1::QueryHoles,
    AuthoringSectionV1::CompetencyHoles,
    AuthoringSectionV1::TheoryHoles,
    AuthoringSectionV1::DependentRefinements,
    AuthoringSectionV1::EvolutionPreviews,
    AuthoringSectionV1::CompetencyQuestions,
    AuthoringSectionV1::RuntimeTheoryReports,
    AuthoringSectionV1::Validation,
    AuthoringSectionV1::PreparedQuery,
    AuthoringSectionV1::QueryExplanation,
    AuthoringSectionV1::AppliedQueryRepair,
    AuthoringSectionV1::CheckedOlog,
    AuthoringSectionV1::AppliedOlogRepair,
    AuthoringSectionV1::CompetencyEvaluation,
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AuthoringPresentationV1 {
    #[serde(default)]
    pub detail: AuthoringDetailV1,
    /// None uses the detail preset; an empty array explicitly selects no sections.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sections: Option<Vec<AuthoringSectionV1>>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}
fn default_limit() -> usize {
    20
}
impl Default for AuthoringPresentationV1 {
    fn default() -> Self {
        Self {
            detail: AuthoringDetailV1::Summary,
            sections: None,
            limit: default_limit(),
            cursor: None,
        }
    }
}
impl AuthoringPresentationV1 {
    pub(super) fn validate(&self) -> Result<()> {
        if !(1..=MAX_PAGE_LIMIT).contains(&self.limit) {
            return Err(anyhow!(
                "presentation.limit must be between 1 and {MAX_PAGE_LIMIT}"
            ));
        }
        if let Some(sections) = &self.sections {
            if sections.iter().collect::<BTreeSet<_>>().len() != sections.len() {
                return Err(anyhow!("duplicate presentation section"));
            }
        }
        if self.detail == AuthoringDetailV1::Full
            && (self.sections.is_some() || self.cursor.is_some())
        {
            return Err(anyhow!(
                "full returns the canonical report; sections/cursor require summary or standard"
            ));
        }
        if let Some(cursor) = &self.cursor {
            if cursor.len() > MAX_CURSOR_BYTES || self.sections.as_ref().map(Vec::len) != Some(1) {
                return Err(anyhow!(
                    "cursor requires exactly one section and at most {MAX_CURSOR_BYTES} bytes"
                ));
            }
        }
        Ok(())
    }
    fn selected(&self) -> Vec<AuthoringSectionV1> {
        self.sections.clone().unwrap_or_else(|| match self.detail {
            AuthoringDetailV1::Standard => {
                vec![AuthoringSectionV1::Diagnostics, AuthoringSectionV1::Repairs]
            }
            _ => Vec::new(),
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum AuthoringWorkspaceResponseV1 {
    Full(Box<AuthoringWorkspaceReportV1>),
    Compact(Box<AuthoringCompactResponseV1>),
}

#[derive(Debug, Serialize)]
pub(crate) struct AuthoringCompactResponseV1 {
    version: &'static str,
    operation: AuthoringWorkspaceOperationV1,
    detail: AuthoringDetailV1,
    workspace_root: String,
    requested_path: String,
    /// Includes exact root bytes even when compilation fails. Not an accepted anchor.
    input_identity: String,
    source: Option<AuthoringSourceAnchorV1>,
    ok: bool,
    summary: String,
    validation: ValidationSummary,
    promotion: AuthoringPromotionReviewV1,
    trust: AuthoringTrustV1,
    next_actions: Vec<String>,
    follow_up_available: bool,
    /// Includes every selectable section, including omitted and empty sections.
    sections: Vec<SectionPage>,
    omitted_items: usize,
    truncated: bool,
}

#[derive(Debug, Serialize)]
struct ValidationSummary {
    canonical_axi_valid: bool,
    compiled_kernel_ir_valid: bool,
    finite_category_fragment_valid: bool,
    runtime_theory_gate: AuthoringGateDecisionV1,
    diagnostic_errors: usize,
    diagnostic_warnings: usize,
    finite_coverage: Option<axiograph_kernel::FiniteTheoryCoverageIr>,
    finite_residuals: usize,
    dependent_residuals: usize,
    runtime_theory: Option<RuntimeCounts>,
    competency: Option<CompetencyCounts>,
    scope: String,
    non_claims: Vec<String>,
}
#[derive(Debug, Serialize)]
struct RuntimeCounts {
    theory_count: usize,
    checked_obligations: usize,
    review_only_obligations: usize,
    residual_obligations: usize,
    residual_obligation_ids: usize,
    blocked_obligations: usize,
    excluded_by_evidence: usize,
    blocking_errors: usize,
}
#[derive(Debug, Serialize)]
struct CompetencyCounts {
    questions: usize,
    unresolved: usize,
    evaluated: bool,
    satisfied: usize,
    total: usize,
    promotion_gate: AuthoringGateDecisionV1,
}
#[derive(Debug, Serialize)]
struct SectionPage {
    section: AuthoringSectionV1,
    selected: bool,
    total: usize,
    offset: usize,
    returned: usize,
    omitted: usize,
    truncated: bool,
    next_cursor: Option<String>,
    /// Only the selected slice is serialized; never build a full JSON tree to prune it.
    items: Vec<Value>,
}

pub(super) fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

impl AuthoringWorkspaceService {
    pub(crate) fn execute_response(
        &self,
        request: AuthoringWorkspaceRequestV1,
    ) -> Result<AuthoringWorkspaceResponseV1> {
        request.validate()?;
        if request.presentation.detail == AuthoringDetailV1::Full {
            return Ok(AuthoringWorkspaceResponseV1::Full(Box::new(
                self.execute(request)?,
            )));
        }
        let mut anchors = Vec::new();
        let report = self.execute_bound(request.clone(), &mut anchors)?;
        // A failed compiler cannot establish the complete import closure. No cursors
        // are issued in that case; full remains available to inspect failures.
        let expected_anchors = 2
            + usize::from(request.baseline_axi_path.is_some())
            + usize::from(request.cq_path.is_some() || request.cq_text.is_some());
        let follow_up_available = anchors.len() == expected_anchors;
        let presentation = request.presentation.clone();
        let mut semantic_request = request.clone();
        semantic_request.presentation = AuthoringPresentationV1::default();
        let input_identity = digest(&serde_json::to_vec(&(
            CURSOR_VERSION,
            &report.workspace_root,
            semantic_request,
            anchors,
        ))?);
        if presentation.cursor.is_some() && !follow_up_available {
            return Err(anyhow!(
                "cursor cannot be checked: source/import closure compilation failed"
            ));
        }
        let selected = presentation.selected();
        let mut sections = Vec::new();
        for &section in SECTIONS {
            let is_selected = selected.contains(&section);
            let offset = if is_selected {
                presentation
                    .cursor
                    .as_deref()
                    .map(|cursor| parse_cursor(cursor, &input_identity, section))
                    .transpose()?
                    .unwrap_or(0)
            } else {
                0
            };
            let mut page = section_page(&report, section, is_selected, offset, presentation.limit)?;
            if page.offset + page.returned < page.total && follow_up_available {
                page.next_cursor = Some(cursor(
                    &input_identity,
                    section,
                    page.offset + page.returned,
                )?);
            }
            sections.push(page);
        }
        let omitted_items = sections.iter().map(|page| page.omitted).sum();
        let errors = report
            .diagnostics
            .iter()
            .filter(|d| d.severity == AuthoringDiagnosticSeverityV1::Error)
            .count();
        let warnings = report
            .diagnostics
            .iter()
            .filter(|d| d.severity == AuthoringDiagnosticSeverityV1::Warning)
            .count();
        let finite = report.validation.finite_theory_gate.as_ref();
        let validation = ValidationSummary {
            canonical_axi_valid: report.validation.canonical_axi_valid,
            compiled_kernel_ir_valid: report.validation.compiled_kernel_ir_valid,
            finite_category_fragment_valid: report.validation.finite_category_fragment_valid,
            runtime_theory_gate: report.validation.runtime_theory_gate,
            diagnostic_errors: errors,
            diagnostic_warnings: warnings,
            finite_coverage: finite.map(|f| f.coverage.clone()),
            finite_residuals: finite.map_or(0, |f| f.residual_obligations.len()),
            dependent_residuals: report
                .dependent_refinements
                .iter()
                .map(|d| d.residual_obligations.len())
                .sum(),
            runtime_theory: report.validation.runtime_theory.as_ref().map(|r| {
                let s = &r.summary;
                RuntimeCounts {
                    theory_count: s.theory_count,
                    checked_obligations: s.checked_obligations,
                    review_only_obligations: s.review_only_obligations,
                    residual_obligations: s.residual_obligations,
                    residual_obligation_ids: s.residual_obligation_ids.len(),
                    blocked_obligations: s.blocked_obligations,
                    excluded_by_evidence: s.excluded_by_evidence,
                    blocking_errors: s.blocking_errors,
                }
            }),
            competency: report
                .competency_questions
                .as_ref()
                .map(|cq| CompetencyCounts {
                    questions: cq.questions.len(),
                    unresolved: cq.unresolved_question_names.len(),
                    evaluated: cq.evaluation.is_some(),
                    satisfied: cq.evaluation.as_ref().map_or(0, |e| e.satisfied),
                    total: cq.evaluation.as_ref().map_or(0, |e| e.total),
                    promotion_gate: cq.promotion_gate,
                }),
            scope: report.validation.scope.clone(),
            non_claims: report.validation.non_claims.clone(),
        };
        Ok(AuthoringWorkspaceResponseV1::Compact(Box::new(AuthoringCompactResponseV1 {
            version: RESPONSE_VERSION, operation: report.operation, detail: presentation.detail,
            workspace_root: report.workspace_root, requested_path: request.axi_path, input_identity,
            source: report.source, ok: report.ok,
            summary: format!("{}; {errors} error(s), {warnings} warning(s); protected-main eligible: {}; {} promotion blocker(s). Read-only runtime review, not Lean certification.",
                if report.ok { "Runtime checks completed without error diagnostics" } else { "Runtime checks failed" },
                report.promotion.protected_main_eligible, report.promotion.blockers.len()),
            validation, promotion: report.promotion, trust: report.trust, next_actions: report.next_actions,
            follow_up_available, sections, omitted_items, truncated: omitted_items > 0,
        })))
    }
}

fn cursor(anchor: &str, section: AuthoringSectionV1, offset: usize) -> Result<String> {
    let checksum = digest(&serde_json::to_vec(&(
        CURSOR_VERSION,
        anchor,
        section,
        offset,
    ))?);
    Ok(format!("{CURSOR_VERSION}:{anchor}:{offset}:{checksum}"))
}
fn parse_cursor(token: &str, anchor: &str, section: AuthoringSectionV1) -> Result<usize> {
    if token.len() > MAX_CURSOR_BYTES {
        return Err(anyhow!("cursor exceeds byte limit"));
    }
    let fields: Vec<_> = token.split(':').collect();
    if fields.len() != 4
        || fields[0] != CURSOR_VERSION
        || fields[1] != anchor
        || fields[3].len() != 64
        || !fields[3].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(anyhow!("invalid or stale authoring cursor binding"));
    }
    let offset = fields[2]
        .parse::<usize>()
        .map_err(|_| anyhow!("invalid cursor offset"))?;
    if token != cursor(anchor, section, offset)? {
        return Err(anyhow!("invalid authoring cursor consistency check"));
    }
    Ok(offset)
}

fn section_page(
    report: &AuthoringWorkspaceReportV1,
    section: AuthoringSectionV1,
    selected: bool,
    offset: usize,
    limit: usize,
) -> Result<SectionPage> {
    fn page<T: Serialize>(
        items: &[T],
        section: AuthoringSectionV1,
        selected: bool,
        offset: usize,
        limit: usize,
    ) -> Result<SectionPage> {
        if offset > items.len() || (offset != 0 && offset == items.len()) {
            return Err(anyhow!("cursor offset outside section collection"));
        }
        let end = offset.saturating_add(limit).min(items.len());
        let values = if selected {
            items[offset..end]
                .iter()
                .map(serde_json::to_value)
                .collect::<std::result::Result<Vec<_>, _>>()?
        } else {
            vec![]
        };
        let returned = values.len();
        Ok(SectionPage {
            section,
            selected,
            total: items.len(),
            offset,
            returned,
            omitted: items.len() - returned,
            truncated: returned < items.len(),
            next_cursor: None,
            items: values,
        })
    }
    macro_rules! p {
        ($items:expr) => {
            page($items, section, selected, offset, limit)
        };
    }
    use AuthoringSectionV1::*;
    match section {
        Diagnostics => p!(&report.diagnostics),
        StableRuntimeRefs => p!(&report.stable_runtime_refs),
        Repairs => p!(&report.repairs),
        OlogHoles => p!(&report.typed_holes.olog),
        QueryHoles => p!(&report.typed_holes.query),
        CompetencyHoles => p!(&report.typed_holes.competency_questions),
        TheoryHoles => p!(&report.typed_holes.theory),
        DependentRefinements => p!(&report.dependent_refinements),
        EvolutionPreviews => p!(&report.evolution_previews),
        CompetencyQuestions => p!(report
            .competency_questions
            .as_ref()
            .map_or(&[], |c| c.questions.as_slice())),
        RuntimeTheoryReports => p!(report
            .validation
            .runtime_theory
            .as_ref()
            .map_or(&[], |r| r.reports.as_slice())),
        Validation => p!(std::slice::from_ref(&report.validation)),
        PreparedQuery => p!(report.prepared_query.as_slice()),
        QueryExplanation => p!(report.query_explanation.as_slice()),
        AppliedQueryRepair => p!(report.applied_query_repair.as_slice()),
        CheckedOlog => p!(report.checked_olog.as_slice()),
        AppliedOlogRepair => p!(report.applied_olog_repair.as_slice()),
        CompetencyEvaluation => p!(report
            .competency_questions
            .as_ref()
            .and_then(|c| c.evaluation.as_ref())
            .as_slice()),
    }
}

pub(super) fn presentation_schema() -> Value {
    json!({"type":"object", "additionalProperties":false, "properties": {
        "detail":{"enum":["summary","standard","full"],"default":"summary"},
        "sections":{"type":"array","uniqueItems":true,"maxItems":SECTIONS.len(),"items":{"enum":SECTIONS}},
        "limit":{"type":"integer","minimum":1,"maximum":MAX_PAGE_LIMIT,"default":default_limit()},
        "cursor":{"type":"string","maxLength":MAX_CURSOR_BYTES}
    }})
}

// The envelope and aggregate contract are closed. Selected canonical artifacts are
// intentionally opaque object payloads: this schema is not a generated ontology SDK.
pub(super) fn response_schema_object() -> JsonObject {
    fn object(properties: JsonObject) -> Value {
        let required: Vec<_> = properties.keys().cloned().collect();
        json!({"type":"object","additionalProperties":false,"required":required,"properties":properties})
    }
    macro_rules! object_properties {
        ({$($key:literal : $value:tt),* $(,)?}) => {
            JsonObject::from_iter([$(($key.to_owned(), json!($value))),*])
        };
    }
    let count = json!({"type":"integer","minimum":0});
    let string = json!({"type":"string"});
    let boolean = json!({"type":"boolean"});
    let strings = json!({"type":"array","items":string});
    let gate = json!({"enum":["passed","blocked","not_evaluated"]});
    let trust = object(
        object_properties!({"trust_class":string,"authority":string,"checked_fragment":string,
        "trusted_checker_import_closure":string,"completeness_claim":string,"ontology_closure_claim":string,"non_claims":strings}),
    );
    let promotion_gate =
        object(object_properties!({"gate":string,"decision":gate,"detail":string}));
    let promotion = object(
        object_properties!({"candidate_reviewable":boolean,"protected_main_eligible":boolean,
        "gates":{"type":"array","items":promotion_gate},"blockers":strings,"required_write_authority":string,"scope":string,"non_claims":strings}),
    );
    let module = object(
        object_properties!({"module_name":string,"module_id":string,"revision_digest":string}),
    );
    let source = object(
        object_properties!({"workspace_relative_path":string,"root_module":string,"repository_id":string,
        "compiled_snapshot_id":string,"kernel_ir_digest":string,"exact_root_axi_digest":string,
        "ordered_module_closure":{"type":"array","items":module},"runtime_ir_ref_count":count}),
    );
    let finite = object(
        object_properties!({"category_formations_replayed":count,"saturated_presentations":count,"identity_paths_replayed":count,
        "path_explanations_replayed":count,"object_memberships_replayed":count,"dependent_role_witnesses_replayed":count,
        "finite_refinement_predicates_replayed":count,"finite_constraint_witnesses_replayed":count,"dependent_contexts_replayed":count,
        "identity_scope_transports_replayed":count,"non_identity_scope_transports_certified":count}),
    );
    let runtime = object(
        object_properties!({"theory_count":count,"checked_obligations":count,"review_only_obligations":count,
        "residual_obligations":count,"residual_obligation_ids":count,"blocked_obligations":count,"excluded_by_evidence":count,"blocking_errors":count}),
    );
    let cq = object(
        object_properties!({"questions":count,"unresolved":count,"evaluated":boolean,"satisfied":count,"total":count,"promotion_gate":gate}),
    );
    let validation = object(
        object_properties!({"canonical_axi_valid":boolean,"compiled_kernel_ir_valid":boolean,"finite_category_fragment_valid":boolean,
        "runtime_theory_gate":gate,"diagnostic_errors":count,"diagnostic_warnings":count,"finite_coverage":{"anyOf":[finite,{"type":"null"}]},
        "finite_residuals":count,"dependent_residuals":count,"runtime_theory":{"anyOf":[runtime,{"type":"null"}]},
        "competency":{"anyOf":[cq,{"type":"null"}]},"scope":string,"non_claims":strings}),
    );
    let diagnostic = json!({"type":"object","additionalProperties":false,"required":["severity","code","message"],
        "properties":{"severity":{"enum":["error","warning","information"]},"code":string,"message":string,
        "path":string,"line":{"type":"integer","minimum":1},"repair_hint":string,
        "location":crate::axi_input::diagnostics::location_schema()},
        "description":"Absent location means unlocated; path/line alone does not claim token precision."});
    let mut page = object(
        object_properties!({"section":{"enum":SECTIONS},"selected":boolean,"total":count,"offset":count,"returned":count,"omitted":count,
        "truncated":boolean,"next_cursor":{"type":["string","null"],"maxLength":MAX_CURSOR_BYTES},
        "items":{"type":"array","maxItems":MAX_PAGE_LIMIT,"items":{"type":"object"}}}),
    );
    page["allOf"] = json!([{"if":{"properties":{"section":{"const":"diagnostics"}}},
        "then":{"properties":{"items":{"type":"array","items":diagnostic}}}}]);
    let compact = object(
        object_properties!({"version":{"const":RESPONSE_VERSION},"operation":{"enum":["inspect","validate","apply_repair","promotion_review"]},
        "detail":{"enum":["summary","standard"]},"workspace_root":string,"requested_path":string,"input_identity":{"type":"string","pattern":"^[0-9a-f]{64}$"},
        "source":{"anyOf":[source,{"type":"null"}]},"ok":boolean,"summary":string,"validation":validation,"promotion":promotion,"trust":trust,
        "next_actions":strings,"follow_up_available":boolean,"sections":{"type":"array","minItems":SECTIONS.len(),"maxItems":SECTIONS.len(),"items":page},
        "omitted_items":count,"truncated":boolean}),
    );
    let artifact = json!({"type":"object"});
    let artifacts = json!({"type":"array","items":artifact});
    let full = json!({"type":"object","additionalProperties":false,"required":["version","operation","workspace_root","ok","validation","promotion","trust","diagnostics","typed_holes","dependent_refinements","repairs","stable_runtime_refs","next_actions"],
        "properties":{"version":{"const":AUTHORING_WORKSPACE_REPORT_VERSION_V1},"ok":boolean,"source":source,"promotion":promotion,"trust":trust,
        "operation":{"enum":["inspect","validate","apply_repair","promotion_review"]},"workspace_root":string,"validation":artifact,
        "diagnostics":{"type":"array","items":diagnostic},"stable_runtime_refs":artifacts,"next_actions":strings,
        "typed_holes":artifact,"dependent_refinements":artifacts,"repairs":artifacts,"competency_questions":artifact,
        "prepared_query":artifact,"query_explanation":artifact,"applied_query_repair":artifact,"checked_olog":artifact,
        "applied_olog_repair":artifact,"evolution_previews":artifacts},
        "description":"Canonical full artifact; nested domain artifact schemas are not expanded by this presentation contract."});
    JsonObject::from_iter([
        ("type".to_owned(), json!("object")),
        (
            "oneOf".to_owned(),
            json!([compact, full, object(object_properties!({"error":string}))]),
        ),
    ])
}

#[cfg(test)]
pub(super) fn response_schema() -> Value {
    Value::Object(response_schema_object())
}
