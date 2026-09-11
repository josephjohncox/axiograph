use super::super::tests::{request, write_workspace};
use super::*;

fn compact(
    service: &AuthoringWorkspaceService,
    request: AuthoringWorkspaceRequestV1,
) -> Result<AuthoringCompactResponseV1> {
    match service.execute_response(request)? {
        AuthoringWorkspaceResponseV1::Compact(report) => Ok(*report),
        _ => Err(anyhow!("expected compact response")),
    }
}
fn nested_request(collection: AuthoringNestedCollectionV1) -> AuthoringWorkspaceRequestV1 {
    let mut request = request();
    request.presentation.nested = Some(AuthoringNestedPageRequestV1 {
        collection,
        limit: 2,
        byte_limit: MAX_NESTED_BYTE_LIMIT,
        cursor: None,
    });
    request
}

fn canonical_nested_collection(
    full: &AuthoringWorkspaceReportV1,
    collection: AuthoringNestedCollectionV1,
) -> Result<Vec<Value>> {
    use AuthoringNestedCollectionV1::*;
    let mut values = Vec::new();
    match collection {
        ValidationFiniteResiduals => {
            if let Some(gate) = &full.validation.finite_theory_gate {
                values.extend(
                    gate.residual_obligations
                        .iter()
                        .map(serde_json::to_value)
                        .collect::<std::result::Result<Vec<_>, _>>()?,
                );
            }
        }
        RuntimeJudgments
        | RuntimeAdmissibilityChecks
        | RuntimeClosureSteps
        | RuntimeAssumptionDiagnostics => {
            if let Some(module) = &full.validation.runtime_theory {
                for report in &module.reports {
                    let source = match collection {
                        RuntimeJudgments => serde_json::to_value(&report.judgments)?,
                        RuntimeAdmissibilityChecks => {
                            serde_json::to_value(&report.admissibility_checks)?
                        }
                        RuntimeClosureSteps => {
                            serde_json::to_value(&report.admissibility_scan.steps)?
                        }
                        RuntimeAssumptionDiagnostics => {
                            serde_json::to_value(&report.assumption_diagnostics)?
                        }
                        _ => return Err(anyhow!("test runtime collection mismatch")),
                    };
                    values.extend(source.as_array().expect("array").iter().cloned());
                }
            }
        }
        DependentContexts | DependentResiduals => {
            for refinement in &full.dependent_refinements {
                let source = if collection == DependentContexts {
                    serde_json::to_value(&refinement.contexts)?
                } else {
                    serde_json::to_value(&refinement.residual_obligations)?
                };
                values.extend(source.as_array().expect("array").iter().cloned());
            }
        }
        PreparedInferredTypes | PreparedKernelRefs | PreparedRefinementHandles => {
            if let Some(prepared) = &full.prepared_query {
                match collection {
                    PreparedInferredTypes => values.extend(prepared.inferred_types.iter().map(|(variable, inferred_types)| json!({"variable":variable,"inferred_types":inferred_types}))),
                    PreparedKernelRefs => values.extend(serde_json::to_value(&prepared.kernel_refs)?.as_array().expect("array").iter().cloned()),
                    PreparedRefinementHandles => values.extend(serde_json::to_value(&prepared.refinement_handles)?.as_array().expect("array").iter().cloned()),
                    _ => return Err(anyhow!("test prepared collection mismatch")),
                }
            }
        }
        ExplanationPlan
        | ExplanationTypedHoles
        | ExplanationSuggestions
        | ExplanationRefinementCandidates
        | ExplanationSemanticClaims
        | ExplanationTrustGaps => {
            if let Some(explanation) = &full.query_explanation {
                let source = match collection {
                    ExplanationPlan => serde_json::to_value(&explanation.plan)?,
                    ExplanationTypedHoles => {
                        serde_json::to_value(&explanation.exploration.typed_holes)?
                    }
                    ExplanationSuggestions => {
                        serde_json::to_value(&explanation.exploration.exploration_suggestions)?
                    }
                    ExplanationRefinementCandidates => {
                        serde_json::to_value(&explanation.exploration.refinement_candidates)?
                    }
                    ExplanationSemanticClaims => {
                        serde_json::to_value(&explanation.exploration.semantic_claims)?
                    }
                    ExplanationTrustGaps => {
                        serde_json::to_value(&explanation.exploration.trust_gaps)?
                    }
                    _ => return Err(anyhow!("test explanation collection mismatch")),
                };
                values.extend(source.as_array().expect("array").iter().cloned());
            }
        }
        CompetencyQuestions | CompetencyUnresolved => {
            if let Some(competency) = &full.competency_questions {
                let source = if collection == CompetencyQuestions {
                    serde_json::to_value(&competency.questions)?
                } else {
                    serde_json::to_value(&competency.unresolved_question_names)?
                };
                values.extend(source.as_array().expect("array").iter().cloned());
            }
        }
        CompetencyEvaluations
        | CompetencyPreparedKernelRefs
        | CompetencyPreparedRefinementHandles
        | CompetencyRefinementCandidates => {
            if let Some(evaluation) = full
                .competency_questions
                .as_ref()
                .and_then(|competency| competency.evaluation.as_ref())
            {
                for question in &evaluation.questions {
                    let source = match collection {
                        CompetencyEvaluations => serde_json::to_value(vec![question])?,
                        CompetencyPreparedKernelRefs => serde_json::to_value(
                            question
                                .prepared_query
                                .as_ref()
                                .map_or(&[][..], |prepared| prepared.kernel_refs.as_slice()),
                        )?,
                        CompetencyPreparedRefinementHandles => serde_json::to_value(
                            question
                                .prepared_query
                                .as_ref()
                                .map_or(&[][..], |prepared| prepared.refinement_handles.as_slice()),
                        )?,
                        CompetencyRefinementCandidates => {
                            serde_json::to_value(&question.refinement_candidates)?
                        }
                        _ => return Err(anyhow!("test competency evaluation collection mismatch")),
                    };
                    values.extend(source.as_array().expect("array").iter().cloned());
                }
            }
        }
    }
    Ok(values)
}

fn page_request() -> AuthoringWorkspaceRequestV1 {
    let mut r = request();
    r.presentation.sections = Some(vec![AuthoringSectionV1::StableRuntimeRefs]);
    r.presentation.limit = 1;
    r
}
fn next(
    service: &AuthoringWorkspaceService,
    request: &AuthoringWorkspaceRequestV1,
) -> Result<String> {
    compact(service, request.clone())?
        .sections
        .into_iter()
        .find(|p| p.selected)
        .and_then(|p| p.next_cursor)
        .ok_or_else(|| {
            anyhow!(
                "fixture must contain another page: {:?}",
                service.execute(request.clone()).map(|r| r.diagnostics)
            )
        })
}

#[test]
fn authoring_workspace_page_unions_match_every_full_collection() -> Result<()> {
    let (_temp, service) = write_workspace()?;
    let full = service.execute(request())?;
    let canonical = serde_json::to_value(&full)?;
    let validator = jsonschema::validator_for(&response_schema())?;
    use AuthoringSectionV1::*;
    // Independent oracle: compare with canonical wire fields, not section_page itself.
    let collections = [
        (Diagnostics, "/diagnostics", false),
        (StableRuntimeRefs, "/stable_runtime_refs", false),
        (Repairs, "/repairs", false),
        (OlogHoles, "/typed_holes/olog", false),
        (QueryHoles, "/typed_holes/query", false),
        (CompetencyHoles, "/typed_holes/competency_questions", false),
        (TheoryHoles, "/typed_holes/theory", false),
        (DependentRefinements, "/dependent_refinements", false),
        (EvolutionPreviews, "/evolution_previews", false),
        (
            CompetencyQuestions,
            "/competency_questions/questions",
            false,
        ),
        (
            RuntimeTheoryReports,
            "/validation/runtime_theory/reports",
            false,
        ),
        (Validation, "/validation", true),
        (PreparedQuery, "/prepared_query", true),
        (QueryExplanation, "/query_explanation", true),
        (AppliedQueryRepair, "/applied_query_repair", true),
        (CheckedOlog, "/checked_olog", true),
        (AppliedOlogRepair, "/applied_olog_repair", true),
        (
            CompetencyEvaluation,
            "/competency_questions/evaluation",
            true,
        ),
    ];
    assert_eq!(collections.len(), SECTIONS.len());
    for (section, pointer, singleton) in collections {
        let expected = match canonical.pointer(pointer) {
            None => vec![],
            Some(value) if singleton => vec![value.clone()],
            Some(value) => value.as_array().expect("canonical collection").clone(),
        };
        let mut r = request();
        r.presentation.sections = Some(vec![section]);
        r.presentation.limit = 1;
        let mut union = Vec::new();
        loop {
            let report = compact(&service, r.clone())?;
            assert!(validator.is_valid(&serde_json::to_value(&report)?));
            assert_eq!(report.ok, full.ok);
            assert_eq!(report.promotion, full.promotion);
            assert_eq!(report.trust, full.trust);
            assert_eq!(report.source, full.source);
            let page = report
                .sections
                .into_iter()
                .find(|p| p.selected)
                .expect("selected");
            assert_eq!(page.total, expected.len());
            assert_eq!(page.offset, union.len());
            assert_eq!(page.returned + page.omitted, page.total);
            assert!(page.returned <= r.presentation.limit);
            union.extend(page.items);
            r.presentation.cursor = page.next_cursor;
            if r.presentation.cursor.is_none() {
                break;
            }
        }
        assert_eq!(union, expected, "{section:?}");
    }
    let mut r = request();
    r.presentation.detail = AuthoringDetailV1::Full;
    assert_eq!(
        serde_json::to_value(service.execute_response(r)?)?,
        serde_json::to_value(full)?
    );
    Ok(())
}

#[test]
fn nested_page_unions_losslessly_match_canonical_full_collections() -> Result<()> {
    let (_temp, service) = write_workspace()?;
    let full = service.execute(request())?;
    let validator = jsonschema::validator_for(&response_schema())?;
    for &collection in NESTED_COLLECTIONS {
        let expected = canonical_nested_collection(&full, collection)?;
        let mut request = nested_request(collection);
        request.presentation.nested.as_mut().unwrap().limit = 1;
        let mut actual: Vec<Value> = Vec::new();
        loop {
            let response = compact(&service, request.clone())?;
            assert_eq!(response.source, full.source);
            assert_eq!(response.promotion, full.promotion);
            assert_eq!(response.trust, full.trust);
            assert!(validator.is_valid(&serde_json::to_value(&response)?));
            let page = response.nested_page.expect("requested nested page");
            assert_eq!(page.collection, collection);
            assert_eq!(page.total, expected.len());
            assert_eq!(page.offset, actual.len());
            assert_eq!(page.returned + page.omitted, page.total);
            assert!(page.returned <= page.entry_limit);
            assert!(page.returned_bytes <= page.byte_limit);
            for item in &page.items {
                let bytes = item.canonical_item_json.as_bytes();
                assert_eq!(item.canonical_item_bytes, bytes.len());
                assert_eq!(item.canonical_item_sha256, digest(bytes));
                assert_eq!(item.ordinal, actual.len());
                actual.push(serde_json::from_str(&item.canonical_item_json)?);
            }
            request.presentation.nested.as_mut().unwrap().cursor = page.next_cursor;
            if request
                .presentation
                .nested
                .as_ref()
                .unwrap()
                .cursor
                .is_none()
            {
                break;
            }
        }
        assert_eq!(actual, expected, "{collection:?}");
    }
    Ok(())
}

#[test]
fn nested_pages_enforce_n_n_plus_one_byte_entry_and_large_item_bounds() -> Result<()> {
    let (_temp, service) = write_workspace()?;
    let full = service.execute(request())?;
    let collection = AuthoringNestedCollectionV1::PreparedKernelRefs;
    let expected = canonical_nested_collection(&full, collection)?;
    assert!(expected.len() > 2);

    let mut request = nested_request(collection);
    request.presentation.nested.as_mut().unwrap().limit = 2;
    let two = compact(&service, request.clone())?.nested_page.unwrap();
    assert_eq!(two.returned, 2);
    assert!(two.next_cursor.is_some());
    request.presentation.nested.as_mut().unwrap().limit = expected.len();
    let all = compact(&service, request.clone())?.nested_page.unwrap();
    assert_eq!(all.returned, expected.len());
    assert!(all.next_cursor.is_none());

    let item_bytes = serde_json::to_vec(&nested_collection_items(&full, collection)?[0])?.len();
    request.presentation.nested.as_mut().unwrap().limit = 1;
    request.presentation.nested.as_mut().unwrap().byte_limit = item_bytes - 1;
    assert!(service.execute_response(request.clone()).is_err());
    request.presentation.nested.as_mut().unwrap().byte_limit = item_bytes;
    let exact = compact(&service, request)?.nested_page.unwrap();
    assert_eq!((exact.returned, exact.returned_bytes), (1, item_bytes));

    let mut items = Vec::new();
    let large = "x".repeat(MAX_NESTED_ITEM_BYTES);
    assert!(push_nested_item(&mut items, collection, "a".repeat(64), &large).is_err());
    Ok(())
}

#[test]
fn nested_pages_cover_empty_singleton_and_fail_closed_inputs() -> Result<()> {
    let (temp, service) = write_workspace()?;
    let empty = compact(
        &service,
        nested_request(AuthoringNestedCollectionV1::CompetencyUnresolved),
    )?
    .nested_page
    .unwrap();
    assert_eq!((empty.total, empty.returned, empty.omitted), (0, 0, 0));
    assert!(!empty.truncated);

    let mut singleton_report = service.execute(request())?;
    singleton_report
        .prepared_query
        .as_mut()
        .expect("prepared query")
        .kernel_refs
        .truncate(1);
    let singleton = nested_collection_items(
        &singleton_report,
        AuthoringNestedCollectionV1::PreparedKernelRefs,
    )?;
    assert_eq!(singleton.len(), 1);

    for nested in [
        json!({"collection":"prepared_kernel_refs","limit":0}),
        json!({"collection":"prepared_kernel_refs","limit":101}),
        json!({"collection":"prepared_kernel_refs","byte_limit":0}),
        json!({"collection":"prepared_kernel_refs","byte_limit":MAX_NESTED_BYTE_LIMIT + 1}),
        json!({"collection":"unknown"}),
        json!({"collection":"prepared_kernel_refs","unexpected":true}),
    ] {
        let mut value = serde_json::to_value(request())?;
        value["presentation"] = json!({"detail":"summary","nested":nested});
        match serde_json::from_value::<AuthoringWorkspaceRequestV1>(value) {
            Err(_) => (),
            Ok(request) => assert!(service.execute_response(request).is_err()),
        }
    }

    let mut request = nested_request(AuthoringNestedCollectionV1::PreparedKernelRefs);
    let first = compact(&service, request.clone())?.nested_page.unwrap();
    request.presentation.nested.as_mut().unwrap().cursor = first.next_cursor;
    let token = request
        .presentation
        .nested
        .as_ref()
        .unwrap()
        .cursor
        .clone()
        .unwrap();
    for malformed in [
        "x".to_string(),
        token.replace(NESTED_CURSOR_VERSION, "authoring-nested-page-v2"),
        "x".repeat(MAX_CURSOR_BYTES + 1),
    ] {
        request.presentation.nested.as_mut().unwrap().cursor = Some(malformed);
        assert!(service.execute_response(request.clone()).is_err());
    }
    request.presentation.nested.as_mut().unwrap().cursor = Some(token);
    let source = temp.path().join("domain.axi");
    let original = std::fs::read_to_string(&source)?;
    std::fs::write(&source, format!("{original}\n"))?;
    assert!(service.execute_response(request.clone()).is_err());
    std::fs::write(&source, original)?;
    let (_other, other_service) = write_workspace()?;
    assert!(other_service.execute_response(request).is_err());

    let mut failed = nested_request(AuthoringNestedCollectionV1::RuntimeJudgments);
    failed.axi_text = Some("invalid axi".into());
    assert!(service.execute_response(failed).is_err());
    Ok(())
}

#[test]
fn authoring_workspace_cursor_rejects_changed_inputs_and_workspace() -> Result<()> {
    let (temp, service) = write_workspace()?;
    let mut r = page_request();
    r.presentation.cursor = Some(next(&service, &r)?);
    // Page size and detail are display-only and may vary between follow-ups.
    let mut display = r.clone();
    display.presentation.limit = 2;
    display.presentation.detail = AuthoringDetailV1::Standard;
    assert!(service.execute_response(display).is_ok());
    let (_other, other_service) = write_workspace()?;
    assert!(other_service.execute_response(r.clone()).is_err());
    let mut changed = r.clone();
    changed.focus_variable = Some("different".into());
    assert!(service.execute_response(changed).is_err());
    let mut changed = r.clone();
    changed.presentation.sections = Some(vec![AuthoringSectionV1::Diagnostics]);
    assert!(service.execute_response(changed).is_err());
    for field in ["axi_text", "baseline_axi_text", "cq_text"] {
        let mut value = serde_json::to_value(&r)?;
        let path = match field {
            "axi_text" => "domain.axi",
            "baseline_axi_text" => "baseline.axi",
            _ => "questions.cq",
        };
        value[field] = json!(format!(
            "{}\n",
            std::fs::read_to_string(temp.path().join(path))?
        ));
        assert!(
            service
                .execute_response(serde_json::from_value(value)?)
                .is_err(),
            "{field}"
        );
    }
    for path in ["domain.axi", "baseline.axi", "questions.cq"] {
        let path = temp.path().join(path);
        let original = std::fs::read_to_string(&path)?;
        std::fs::write(&path, format!("{original}\n"))?;
        assert!(
            service.execute_response(r.clone()).is_err(),
            "{}",
            path.display()
        );
        std::fs::write(&path, original)?;
    }
    assert!(service.execute_response(r).is_ok());
    Ok(())
}

#[test]
fn authoring_workspace_cursor_rejects_candidate_and_baseline_import_edits() -> Result<()> {
    let (temp, service) = write_workspace()?;
    let base = "module Base\n\nschema Shared:\n  object Person\n  relation Parent(child: Person, parent: Person)\n";
    let extension = "module Extension\nimport Base\ninstance Family of Shared:\n  Person = {Alice, Bob}\n  Parent = {(child=Alice, parent=Bob)}\n";
    std::fs::write(temp.path().join("Base.axi"), base)?;
    std::fs::write(temp.path().join("Extension.axi"), extension)?;
    for baseline in [false, true] {
        let mut r = page_request();
        if baseline {
            r.baseline_axi_path = Some("Extension.axi".into());
        } else {
            r.axi_path = "Extension.axi".into();
            r.schema = None;
            r.query_ir_v1 = None;
            r.cq_path = None;
        }
        r.presentation.cursor = Some(next(&service, &r)?);
        // An exact-byte-only imported revision change must invalidate the cursor.
        std::fs::write(temp.path().join("Base.axi"), format!("{base}\n"))?;
        assert!(service.execute_response(r).is_err());
        std::fs::write(temp.path().join("Base.axi"), base)?;
    }
    Ok(())
}

#[test]
fn authoring_workspace_rejects_invalid_limits_sections_and_cursors() -> Result<()> {
    let (_temp, service) = write_workspace()?;
    for presentation in [
        json!({"limit":-1}),
        json!({"limit":0}),
        json!({"limit":101}),
        json!({"limit":1.5}),
        json!({"sections":["not_a_section"]}),
        json!({"sections":["diagnostics","diagnostics"]}),
        json!({"unexpected":true}),
        json!({"detail":"full","sections":["diagnostics"]}),
        json!({"cursor":"x"}),
        json!({"sections":["diagnostics"],"cursor":"x".repeat(257)}),
    ] {
        let mut value = serde_json::to_value(request())?;
        value["presentation"] = presentation.clone();
        match serde_json::from_value::<AuthoringWorkspaceRequestV1>(value) {
            Err(_) => (),
            Ok(r) => assert!(service.execute_response(r).is_err(), "{presentation}"),
        }
    }
    let mut r = page_request();
    let token = next(&service, &r)?;
    let fields: Vec<_> = token.split(':').collect();
    let anchor = fields[1];
    for bad in [
        "".into(),
        token.replace(CURSOR_VERSION, "authoring-page-v2"),
        format!("{CURSOR_VERSION}:{anchor}:2:{}", fields[3]),
        format!(
            "{CURSOR_VERSION}:{anchor}:184467440737095516160:{}",
            fields[3]
        ),
        format!("{CURSOR_VERSION}:{anchor}:+1:{}", fields[3]),
        format!("{CURSOR_VERSION}:{anchor}:01:{}", fields[3]),
        format!("{CURSOR_VERSION}:{anchor}:1:{}", "z".repeat(64)),
        cursor(anchor, AuthoringSectionV1::StableRuntimeRefs, usize::MAX)?,
    ] {
        r.presentation.cursor = Some(bad);
        assert!(service.execute_response(r.clone()).is_err());
    }
    // Cursors are deliberately NOT authentication. Recomputed in-range selectors
    // remain bounded read-only pages and cannot change the promotion decision.
    r.presentation.cursor = Some(cursor(anchor, AuthoringSectionV1::StableRuntimeRefs, 2)?);
    let report = compact(&service, r)?;
    assert!(!report.promotion.protected_main_eligible);
    assert_eq!(
        report.sections.iter().find(|p| p.selected).unwrap().offset,
        2
    );
    Ok(())
}

#[test]
fn authoring_workspace_summary_preserves_review_only_theory_totals() -> Result<()> {
    let (temp, service) = write_workspace()?;
    std::fs::write(temp.path().join("review.axi"), "module Review\nschema S:\n  object Person\n  relation Parent(child: Person, parent: Person)\ntheory T on S:\n  constraint transitive Parent on (child, parent)\ninstance I of S:\n  Person = {Alice}\n  Parent = {p: (child=Alice, parent=Alice)}\n")?;
    let mut r = request();
    r.axi_path = "review.axi".into();
    r.query_ir_v1 = None;
    r.cq_path = None;
    let full = service.execute(r.clone())?;
    let summary = compact(&service, r)?;
    let counts = summary
        .validation
        .runtime_theory
        .as_ref()
        .expect("runtime counts");
    let canonical = &full
        .validation
        .runtime_theory
        .as_ref()
        .expect("runtime report")
        .summary;
    assert_eq!(
        counts.review_only_obligations,
        canonical.review_only_obligations
    );
    assert_eq!(counts.residual_obligations, canonical.residual_obligations);
    assert_eq!(
        counts.residual_obligation_ids,
        canonical.residual_obligation_ids.len()
    );
    assert!(counts.review_only_obligations > 0);
    assert_eq!(
        summary.validation.runtime_theory_gate,
        AuthoringGateDecisionV1::Blocked
    );
    assert_eq!(summary.promotion, full.promotion);
    assert!(!summary.promotion.protected_main_eligible);
    assert!(summary.sections.iter().all(|p| p.items.is_empty()));
    Ok(())
}

#[test]
fn authoring_workspace_summary_retains_hidden_failures_and_source_bytes() -> Result<()> {
    let (_temp, service) = write_workspace()?;
    let mut r = request();
    r.axi_text = Some("not canonical axi".into());
    let full = service.execute(r.clone())?;
    let failed = compact(&service, r.clone())?;
    assert!(!failed.ok);
    assert_eq!(failed.promotion, full.promotion);
    assert_eq!(failed.trust, full.trust);
    assert!(failed.validation.diagnostic_errors > 0);
    assert!(failed.truncated);
    assert!(!failed.follow_up_available);
    assert!(failed.sections.iter().all(|p| p.items.is_empty()));
    r.axi_text = Some("different invalid source".into());
    assert_ne!(compact(&service, r)?.input_identity, failed.input_identity);
    Ok(())
}

#[test]
fn authoring_workspace_schema_only_import_supports_query_cq_and_adapter_parity() -> Result<()> {
    let (temp, service) = write_workspace()?;
    std::fs::write(
        temp.path().join("Base.axi"),
        "module Base\nschema Shared:\n  object Person\n  relation Parent(child: Person, parent: Person)\n",
    )?;
    std::fs::write(
        temp.path().join("Extension.axi"),
        "module Extension\nimport Base\ninstance Family of Shared:\n  Person = {Alice, Bob}\n  Parent = {(child=Alice, parent=Bob)}\n",
    )?;
    let mut r = page_request();
    r.axi_path = "Extension.axi".into();
    r.baseline_axi_path = None;
    r.schema = Some("Shared".into());
    std::fs::write(temp.path().join("questions.cq"), "version competency_question_bundle_v1\nquestion parent_exists:\n  ask: is there a parent?\n  expect: exists Shared.Parent(child=?child, parent=?parent)\n")?;
    // Exact unsaved root bytes participate in the canonical package without disk writes.
    let disk = std::fs::read_to_string(temp.path().join("Extension.axi"))?;
    r.axi_text = Some(format!("{disk}\n-- unsaved root\n"));
    let full = service.execute(r.clone())?;
    assert!(full.ok, "{:?}", full.diagnostics);
    assert!(full.prepared_query.is_some());
    let cq = full
        .competency_questions
        .as_ref()
        .unwrap()
        .evaluation
        .as_ref()
        .unwrap();
    assert_eq!((cq.total, cq.satisfied, cq.questions[0].rows), (1, 1, 1));
    let source = full.source.as_ref().unwrap();
    assert_eq!(
        source
            .ordered_module_closure
            .iter()
            .map(|m| m.module_name.as_str())
            .collect::<Vec<_>>(),
        ["Base", "Extension"]
    );
    assert_eq!(
        source.exact_root_axi_digest,
        AxiDigest::from_axi_text(r.axi_text.as_ref().unwrap()).to_string()
    );
    assert_eq!(
        std::fs::read_to_string(temp.path().join("Extension.axi"))?,
        disk
    );
    let summary = compact(&service, r.clone())?;
    assert!(summary.ok);
    assert_eq!(summary.source, full.source);
    assert_eq!(summary.trust, full.trust);
    assert_eq!(summary.promotion, full.promotion);
    assert!(!summary.promotion.protected_main_eligible);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    for detail in [
        AuthoringDetailV1::Summary,
        AuthoringDetailV1::Standard,
        AuthoringDetailV1::Full,
    ] {
        r.presentation = AuthoringPresentationV1 {
            detail,
            ..Default::default()
        };
        let expected = serde_json::to_value(service.execute_response(r.clone())?)?;
        let mcp = execute_mcp_payload(
            &service,
            serde_json::from_value(json!({"name":AUTHORING_WORKSPACE_TOOL_NAME,"arguments":r}))?,
        );
        assert_eq!(serde_json::to_value(mcp)?["structuredContent"], expected);
        let http = execute_http_payload(&service, &serde_json::to_vec(&r)?);
        assert_eq!(http.status(), StatusCode::OK);
        let bytes = runtime.block_on(http.into_body().collect())?.to_bytes();
        assert_eq!(serde_json::from_slice::<Value>(&bytes)?, expected);
    }
    Ok(())
}

#[test]
fn authoring_workspace_adapter_payloads_match_service_in_all_modes() -> Result<()> {
    let (_temp, service) = write_workspace()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let validator = jsonschema::validator_for(&response_schema())?;
    for detail in [
        AuthoringDetailV1::Summary,
        AuthoringDetailV1::Standard,
        AuthoringDetailV1::Full,
    ] {
        let mut r = request();
        r.presentation.detail = detail;
        let expected = serde_json::to_value(service.execute_response(r.clone())?)?;
        let value = serde_json::to_value(&r)?;
        let mcp_request: CallToolRequestParams = serde_json::from_value(
            json!({"name":AUTHORING_WORKSPACE_TOOL_NAME,"arguments":value}),
        )?;
        let mcp = serde_json::to_value(execute_mcp_payload(&service, mcp_request))?;
        assert_eq!(mcp["structuredContent"], expected);
        let http = execute_http_payload(&service, &serde_json::to_vec(&r)?);
        assert_eq!(http.status(), StatusCode::OK);
        let bytes = runtime.block_on(http.into_body().collect())?.to_bytes();
        assert_eq!(serde_json::from_slice::<Value>(&bytes)?, expected);
        let mut state = AuthoringWorkspaceLspState {
            service: service.clone(),
            default_axi_path: None,
            documents: BTreeMap::new(),
            document_images_incomplete: false,
            publications: BTreeMap::new(),
        };
        let Message::Response(lsp) = handle_lsp_request(
            &mut state,
            LspRequest::new(
                1.into(),
                "workspace/executeCommand".into(),
                json!({"command":AUTHORING_WORKSPACE_LSP_COMMAND,"arguments":[value]}),
            ),
        ) else {
            panic!("LSP response")
        };
        assert_eq!(lsp.response_result.expect("LSP result"), expected);
        assert!(validator.is_valid(&expected));
    }
    let mut bad = serde_json::to_value(request())?;
    bad["presentation"] = json!({"limit":-1});
    let mcp = execute_mcp_payload(
        &service,
        serde_json::from_value(json!({"name":AUTHORING_WORKSPACE_TOOL_NAME,"arguments":bad}))?,
    );
    let mcp = serde_json::to_value(mcp)?;
    assert_eq!(mcp["isError"], true);
    assert!(validator.is_valid(&mcp["structuredContent"]));
    let http = execute_http_payload(&service, &serde_json::to_vec(&bad)?);
    assert_eq!(http.status(), StatusCode::BAD_REQUEST);
    let bytes = runtime.block_on(http.into_body().collect())?.to_bytes();
    assert!(validator.is_valid(&serde_json::from_slice(&bytes)?));
    Ok(())
}

#[test]
fn authoring_workspace_summary_cursors_bind_first_detail_request() -> Result<()> {
    let (temp, service) = write_workspace()?;
    let summary = compact(&service, request())?;
    let section = summary
        .sections
        .iter()
        .find(|p| p.section == AuthoringSectionV1::StableRuntimeRefs)
        .unwrap();
    let mut r = page_request();
    r.presentation.cursor = section.next_cursor.clone();
    assert!(r.presentation.cursor.is_some());
    let first = compact(&service, r.clone())?;
    assert_eq!(
        first.sections.iter().find(|p| p.selected).unwrap().offset,
        0
    );
    let path = temp.path().join("domain.axi");
    std::fs::write(&path, format!("{}\n", std::fs::read_to_string(&path)?))?;
    assert!(service.execute_response(r).is_err());
    Ok(())
}

#[test]
fn nested_pages_have_cli_mcp_http_and_lsp_service_parity() -> Result<()> {
    let (_temp, service) = write_workspace()?;
    let request = nested_request(AuthoringNestedCollectionV1::PreparedKernelRefs);
    let expected = serde_json::to_value(service.execute_response(request.clone())?)?;
    let validator = jsonschema::validator_for(&response_schema())?;
    assert!(validator.is_valid(&expected));
    for pointer in [
        "/nested_page/returned_bytes",
        "/nested_page/items/0/canonical_item_bytes",
        "/nested_page/items/0/item_identity",
    ] {
        let mut malformed = expected.clone();
        *malformed.pointer_mut(pointer).expect("nested schema field") = json!(false);
        assert!(!validator.is_valid(&malformed), "{pointer}");
    }
    let value = serde_json::to_value(&request)?;
    let mcp = execute_mcp_payload(
        &service,
        serde_json::from_value(
            json!({"name":AUTHORING_WORKSPACE_TOOL_NAME,"arguments":value.clone()}),
        )?,
    );
    assert_eq!(serde_json::to_value(mcp)?["structuredContent"], expected);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let http = execute_http_payload(&service, &serde_json::to_vec(&request)?);
    assert_eq!(http.status(), StatusCode::OK);
    let bytes = runtime.block_on(http.into_body().collect())?.to_bytes();
    assert_eq!(serde_json::from_slice::<Value>(&bytes)?, expected);

    let mut state = AuthoringWorkspaceLspState {
        service: service.clone(),
        default_axi_path: None,
        documents: BTreeMap::new(),
        document_images_incomplete: false,
        publications: BTreeMap::new(),
    };
    let Message::Response(response) = handle_lsp_request(
        &mut state,
        LspRequest::new(
            1.into(),
            "workspace/executeCommand".into(),
            json!({"command":AUTHORING_WORKSPACE_LSP_COMMAND,"arguments":[value]}),
        ),
    ) else {
        panic!("LSP response")
    };
    assert_eq!(response.response_result.expect("LSP result"), expected);
    Ok(())
}

#[test]
fn authoring_workspace_response_schema_validates_real_modes_and_errors() -> Result<()> {
    let (_temp, service) = write_workspace()?;
    let tool = authoring_rmcp_tool();
    let schema = serde_json::to_value(tool.output_schema.expect("output schema"))?;
    assert_eq!(schema, response_schema());
    assert!(schema.is_object());
    let validator = jsonschema::validator_for(&schema)?;
    for detail in [
        AuthoringDetailV1::Summary,
        AuthoringDetailV1::Standard,
        AuthoringDetailV1::Full,
    ] {
        for invalid_source in [false, true] {
            let mut r = request();
            r.presentation.detail = detail;
            if invalid_source {
                r.axi_text = Some("invalid axi".into());
            }
            let value = serde_json::to_value(service.execute_response(r)?)?;
            let errors: Vec<_> = validator
                .iter_errors(&value)
                .map(|e| e.to_string())
                .collect();
            assert!(errors.is_empty(), "{detail:?}: {errors:?}");
        }
    }
    assert!(validator.is_valid(&json!({"error":"invalid authoring request"})));
    let good = serde_json::to_value(service.execute_response(request())?)?;
    for pointer in [
        "/ok",
        "/trust",
        "/promotion/blockers",
        "/validation/diagnostic_errors",
        "/sections/0/total",
        "/sections/0/items",
    ] {
        let mut bad = good.clone();
        *bad.pointer_mut(pointer).expect("schema test field") = json!("malformed");
        assert!(!validator.is_valid(&bad), "{pointer}");
    }
    for bad in [
        json!({}),
        json!({"error":false}),
        json!({"error":"x","ok":true}),
    ] {
        assert!(!validator.is_valid(&bad));
    }
    Ok(())
}

#[test]
fn authoring_workspace_real_fixture_compact_byte_budget() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let service = AuthoringWorkspaceService::new(&root)?;
    let validator = jsonschema::validator_for(&response_schema())?;
    for path in [
        "examples/software_authoring/authoring_workspace_request.json",
        "examples/regulated_shipment/authoring_request.json",
    ] {
        let mut r = read_authoring_request(&root.join(path))?;
        let full_report = service.execute(r.clone())?;
        let full = serde_json::to_vec(&full_report)?;
        r.presentation.detail = AuthoringDetailV1::Summary;
        let summary = compact(&service, r.clone())?;
        assert_eq!(summary.source, full_report.source);
        assert_eq!(summary.promotion, full_report.promotion);
        assert_eq!(summary.trust, full_report.trust);
        let compact_bytes = serde_json::to_vec(&summary)?;
        let errors: Vec<_> = validator
            .iter_errors(&serde_json::to_value(&summary)?)
            .map(|e| e.to_string())
            .collect();
        assert!(errors.is_empty(), "{errors:?}");
        eprintln!(
            "{path}: full={} compact={}",
            full.len(),
            compact_bytes.len()
        );
        assert!(
            compact_bytes.len() < 12_000,
            "compact budget exceeded: {}",
            compact_bytes.len()
        );
        assert!(
            compact_bytes.len() * 10 < full.len(),
            "must reduce actual fixture payload by >90%"
        );
        // Full is explicit and remains byte-identical, including certificate metadata.
        r.presentation.detail = AuthoringDetailV1::Full;
        assert_eq!(serde_json::to_vec(&service.execute_response(r)?)?, full);
    }
    Ok(())
}
