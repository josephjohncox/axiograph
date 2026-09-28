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
        EvolutionOlogPrimitives
        | EvolutionOlogResidualObligations
        | EvolutionOlogRefinementCandidates => {
            for preview in &full.evolution_previews {
                if let AuthoringEvolutionPreviewV1::Olog(olog_preview) = preview {
                    let source = match collection {
                        EvolutionOlogPrimitives => {
                            serde_json::to_value(&olog_preview.semantic_delta.primitives)?
                        }
                        EvolutionOlogResidualObligations => {
                            serde_json::to_value(&olog_preview.residual_obligations)?
                        }
                        EvolutionOlogRefinementCandidates => {
                            serde_json::to_value(&olog_preview.refinement_candidates)?
                        }
                        _ => {
                            return Err(anyhow!("test evolution-olog-preview collection mismatch"))
                        }
                    };
                    values.extend(source.as_array().expect("array").iter().cloned());
                }
            }
        }
        EvolutionFiniteKernelChangedPayloads
        | EvolutionFiniteKernelAddedPayloads
        | EvolutionFiniteKernelRemovedPayloads => {
            for preview in &full.evolution_previews {
                if let AuthoringEvolutionPreviewV1::FiniteKernel(finite_preview) = preview {
                    let source = match collection {
                        EvolutionFiniteKernelChangedPayloads => {
                            serde_json::to_value(&finite_preview.changed_payloads)?
                        }
                        EvolutionFiniteKernelAddedPayloads => {
                            serde_json::to_value(&finite_preview.added_payloads)?
                        }
                        EvolutionFiniteKernelRemovedPayloads => {
                            serde_json::to_value(&finite_preview.removed_payloads)?
                        }
                        _ => {
                            return Err(anyhow!(
                                "test evolution-finite-kernel-preview collection mismatch"
                            ))
                        }
                    };
                    values.extend(source.as_array().expect("array").iter().cloned());
                }
            }
        }
        CheckedOlogTypedHoles | CheckedOlogRefinementCandidates => {
            if let Some(checked) = &full.checked_olog {
                let source = match collection {
                    CheckedOlogTypedHoles => serde_json::to_value(&checked.typed_holes)?,
                    CheckedOlogRefinementCandidates => {
                        serde_json::to_value(&checked.refinement_candidates)?
                    }
                    _ => return Err(anyhow!("test checked-olog collection mismatch")),
                };
                values.extend(source.as_array().expect("array").iter().cloned());
            }
        }
        AppliedOlogRepairRefinementCandidates => {
            if let Some(applied) = &full.applied_olog_repair {
                let source = serde_json::to_value(&applied.checked_fragment.refinement_candidates)?;
                values.extend(source.as_array().expect("array").iter().cloned());
            }
        }
        AppliedQueryRepairRefinementCandidates => {
            if let Some(applied) = &full.applied_query_repair {
                let source =
                    serde_json::to_value(&applied.refined_exploration.refinement_candidates)?;
                values.extend(source.as_array().expect("array").iter().cloned());
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
        let mut item_identities = Vec::new();
        let mut seen_item_identities = BTreeSet::new();
        let mut expected_input_identity = None;
        let mut expected_collection_identity = None;
        loop {
            let response = compact(&service, request.clone())?;
            assert_eq!(response.source, full.source);
            assert_eq!(response.promotion, full.promotion);
            assert_eq!(response.trust, full.trust);
            assert!(validator.is_valid(&serde_json::to_value(&response)?));
            let input_identity = response.input_identity.clone();
            let page = response.nested_page.expect("requested nested page");
            assert_eq!(page.collection, collection);
            assert_eq!(page.input_identity, input_identity);
            assert_eq!(
                expected_input_identity.get_or_insert(input_identity.clone()),
                &input_identity
            );
            assert_eq!(
                expected_collection_identity.get_or_insert(page.collection_identity.clone()),
                &page.collection_identity
            );
            assert_eq!(page.total, expected.len());
            assert_eq!(page.offset, actual.len());
            assert_eq!(page.returned + page.omitted, page.total);
            assert!(page.returned <= page.entry_limit);
            assert!(page.returned_bytes <= page.byte_limit);
            assert_eq!(page.returned, page.items.len());
            assert_eq!(
                page.returned_bytes,
                page.items
                    .iter()
                    .map(|item| serde_json::to_vec(item).map(|bytes| bytes.len()))
                    .sum::<std::result::Result<usize, _>>()?
            );
            for item in &page.items {
                let bytes = item.canonical_item_json.as_bytes();
                assert_eq!(item.version, NESTED_CURSOR_VERSION);
                assert_eq!(item.collection, collection);
                assert_eq!(item.canonical_item_bytes, bytes.len());
                assert_eq!(item.canonical_item_sha256, digest(bytes));
                assert_eq!(item.ordinal, actual.len());
                assert_eq!(
                    item.item_identity,
                    bound_item_identity(
                        &input_identity,
                        collection,
                        &item.parent_identity,
                        item.ordinal,
                        &item.canonical_item_sha256,
                    )?
                );
                assert!(seen_item_identities.insert(item.item_identity.clone()));
                item_identities.push(item.item_identity.clone());
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
                assert_eq!(
                    page.collection_identity,
                    nested_collection_identity(&input_identity, collection, &item_identities,)?
                );
                break;
            }
        }
        assert_eq!(actual, expected, "{collection:?}");
    }
    Ok(())
}

#[test]
fn nested_item_and_collection_identities_bind_source_parent_type_bytes_and_order() -> Result<()> {
    let input = "a".repeat(64);
    let other_input = "b".repeat(64);
    let parent = "c".repeat(64);
    let other_parent = "d".repeat(64);
    let bytes = br#"{"name":"first"}"#;
    let other_bytes = br#"{"name":"second"}"#;
    let item_hash = digest(bytes);
    let other_hash = digest(other_bytes);
    let collection = AuthoringNestedCollectionV1::PreparedKernelRefs;
    let other_collection = AuthoringNestedCollectionV1::PreparedRefinementHandles;
    let identity = bound_item_identity(&input, collection, &parent, 0, &item_hash)?;

    assert_ne!(
        identity,
        bound_item_identity(&other_input, collection, &parent, 0, &item_hash)?
    );
    assert_ne!(
        identity,
        bound_item_identity(&input, other_collection, &parent, 0, &item_hash)?
    );
    assert_ne!(
        identity,
        bound_item_identity(&input, collection, &other_parent, 0, &item_hash)?
    );
    assert_ne!(
        identity,
        bound_item_identity(&input, collection, &parent, 1, &item_hash)?
    );
    assert_ne!(
        identity,
        bound_item_identity(&input, collection, &parent, 0, &other_hash)?
    );
    assert_ne!(
        bound_parent_identity(&input, &parent)?,
        bound_parent_identity(&other_input, &parent)?
    );

    let identities = vec![identity.clone(), "e".repeat(64)];
    let reversed = identities.iter().rev().cloned().collect::<Vec<_>>();
    let collection_identity = nested_collection_identity(&input, collection, &identities)?;
    assert_ne!(
        collection_identity,
        nested_collection_identity(&other_input, collection, &identities)?
    );
    assert_ne!(
        collection_identity,
        nested_collection_identity(&input, other_collection, &identities)?
    );
    assert_ne!(
        collection_identity,
        nested_collection_identity(&input, collection, &reversed)?
    );
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
    let no_progress = service
        .execute_response(request.clone())
        .expect_err("an indivisible first item must not yield an empty cursor page");
    assert!(no_progress.to_string().contains("nested item requires"));
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
    let first_response = compact(&service, request.clone())?;
    let input_identity = first_response.input_identity.clone();
    let first = first_response.nested_page.unwrap();
    request.presentation.nested.as_mut().unwrap().cursor = first.next_cursor.clone();
    let token = first.next_cursor.unwrap();
    let terminal_offset = first.total;
    let wrong_collection = AuthoringNestedCollectionV1::PreparedRefinementHandles;
    for malformed in [
        "x".to_string(),
        token.replace(NESTED_CURSOR_VERSION, "authoring-nested-page-v2"),
        "x".repeat(MAX_CURSOR_BYTES + 1),
        nested_cursor(
            &input_identity,
            wrong_collection,
            &first.collection_identity,
            first.returned,
        )?,
        nested_cursor(
            &input_identity,
            first.collection,
            &"f".repeat(64),
            first.returned,
        )?,
        nested_cursor(
            &input_identity,
            first.collection,
            &first.collection_identity,
            terminal_offset,
        )?,
        format!(
            "{NESTED_CURSOR_VERSION}:{input_identity}:{}:184467440737095516160:{}",
            first.collection_identity,
            "0".repeat(64)
        ),
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
        "/nested_page/input_identity",
        "/nested_page/collection_identity",
        "/nested_page/returned_bytes",
        "/nested_page/items/0/parent_identity",
        "/nested_page/items/0/canonical_item_bytes",
        "/nested_page/items/0/item_identity",
        "/nested_page/items/0/media_type",
    ] {
        let mut malformed = expected.clone();
        *malformed.pointer_mut(pointer).expect("nested schema field") = json!(false);
        assert!(!validator.is_valid(&malformed), "{pointer}");
    }
    let mut nested_as_top_level = expected.clone();
    nested_as_top_level["sections"][0]["entry_limit"] = json!(1);
    assert!(!validator.is_valid(&nested_as_top_level));
    let mut top_level_as_nested = expected.clone();
    top_level_as_nested["nested_page"]["section"] = json!("diagnostics");
    assert!(!validator.is_valid(&top_level_as_nested));
    let mut open_nested_item = expected.clone();
    open_nested_item["nested_page"]["items"][0]["unexpected"] = json!(true);
    assert!(!validator.is_valid(&open_nested_item));

    let page = expected["nested_page"].as_object().expect("nested page");
    assert_eq!(page["input_identity"], expected["input_identity"]);
    let item = page["items"][0].as_object().expect("nested item");
    assert_eq!(
        item["item_identity"],
        bound_item_identity(
            expected["input_identity"].as_str().expect("input identity"),
            AuthoringNestedCollectionV1::PreparedKernelRefs,
            item["parent_identity"].as_str().expect("parent identity"),
            item["ordinal"].as_u64().expect("ordinal") as usize,
            item["canonical_item_sha256"].as_str().expect("item digest"),
        )?
    );
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

fn works_for_olog_fragment(bind_employer: bool) -> crate::typed_authoring::OlogFragmentV1 {
    let mut role_bindings = vec![crate::typed_authoring::OlogRoleBindingV1 {
        role: "employee".to_string(),
        target_box: "employee".to_string(),
    }];
    if bind_employer {
        role_bindings.push(crate::typed_authoring::OlogRoleBindingV1 {
            role: "employer".to_string(),
            target_box: "team".to_string(),
        });
    }
    crate::typed_authoring::OlogFragmentV1 {
        boxes: vec![
            crate::typed_authoring::OlogBoxV1 {
                box_id: "employee".to_string(),
                object_type: "Person".to_string(),
                label: None,
            },
            crate::typed_authoring::OlogBoxV1 {
                box_id: "team".to_string(),
                object_type: "Team".to_string(),
                label: None,
            },
        ],
        relation_boxes: vec![crate::typed_authoring::OlogRelationBoxV1 {
            box_id: "works_for_fact".to_string(),
            relation: "WorksFor".to_string(),
            role_bindings,
        }],
        aspects: Vec::new(),
        path_equations: Vec::new(),
    }
}

/// Real negative-then-positive fixture: an olog fragment missing the `employer`
/// role binding produces a typed hole and an accompanying refinement candidate.
/// Nested paging over `checked_olog`, `evolution_previews` (Olog variant), and,
/// after applying the emitted refinement handle, `applied_olog_repair` must all
/// losslessly reproduce the exact canonical collections -- not just report
/// non-empty pages.
#[test]
fn nested_checked_and_applied_olog_and_evolution_collections_cover_real_repair_flow() -> Result<()>
{
    let (_temp, service) = write_workspace()?;
    let mut base_request = request();
    base_request.olog_fragment = Some(works_for_olog_fragment(false));
    let full = service.execute(base_request.clone())?;

    // Negative: the fragment is genuinely incomplete, so checking must fail closed
    // with a concrete typed hole rather than silently accepting a partial mapping.
    let checked = full
        .checked_olog
        .as_ref()
        .expect("checked_olog present when olog_fragment is set");
    assert!(
        !checked.ok,
        "expected an incomplete olog fragment to fail checking"
    );
    assert!(!checked.typed_holes.is_empty());
    assert!(
        !checked.refinement_candidates.is_empty(),
        "an emitted refinement handle is required to drive the repair flow"
    );
    assert!(full
        .evolution_previews
        .iter()
        .any(|preview| matches!(preview, AuthoringEvolutionPreviewV1::Olog(_))));

    for collection in [
        AuthoringNestedCollectionV1::CheckedOlogTypedHoles,
        AuthoringNestedCollectionV1::CheckedOlogRefinementCandidates,
        AuthoringNestedCollectionV1::EvolutionOlogPrimitives,
        AuthoringNestedCollectionV1::EvolutionOlogResidualObligations,
        AuthoringNestedCollectionV1::EvolutionOlogRefinementCandidates,
    ] {
        let expected = canonical_nested_collection(&full, collection)?;
        let mut nested_req = base_request.clone();
        nested_req.presentation.nested = Some(AuthoringNestedPageRequestV1 {
            collection,
            limit: 1,
            byte_limit: MAX_NESTED_BYTE_LIMIT,
            cursor: None,
        });
        let mut actual = Vec::new();
        loop {
            let response = compact(&service, nested_req.clone())?;
            let page = response.nested_page.expect("requested nested page");
            assert_eq!(page.collection, collection);
            for item in &page.items {
                actual.push(serde_json::from_str::<Value>(&item.canonical_item_json)?);
            }
            let next_cursor = page.next_cursor;
            nested_req.presentation.nested.as_mut().unwrap().cursor = next_cursor.clone();
            if next_cursor.is_none() {
                break;
            }
        }
        assert_eq!(actual, expected, "{collection:?}");
    }

    // Positive: apply the emitted handle. The repaired fragment must check clean,
    // and `applied_olog_repair`'s refinement candidates must page losslessly too.
    let handle_id = checked.refinement_candidates[0].handle.id.clone();
    let mut applied_request = base_request.clone();
    applied_request.query_ir_v1 = None;
    applied_request.focus_variable = None;
    applied_request.apply_refinement_handle_id = Some(handle_id);
    let applied_full = service.execute(applied_request.clone())?;
    let applied = applied_full
        .applied_olog_repair
        .as_ref()
        .expect("applied_olog_repair present after apply_refinement_handle_id");
    assert!(
        applied.checked_fragment.ok
            || !applied.checked_fragment.typed_holes.iter().any(|hole| {
                hole.kind == crate::typed_authoring::OlogTypedHoleKindV1::MissingRelationRoleBinding
            }),
        "applying the emitted handle must resolve the missing role binding"
    );

    let expected = canonical_nested_collection(
        &applied_full,
        AuthoringNestedCollectionV1::AppliedOlogRepairRefinementCandidates,
    )?;
    let mut nested_req = applied_request.clone();
    nested_req.presentation.nested = Some(AuthoringNestedPageRequestV1 {
        collection: AuthoringNestedCollectionV1::AppliedOlogRepairRefinementCandidates,
        limit: 1,
        byte_limit: MAX_NESTED_BYTE_LIMIT,
        cursor: None,
    });
    let mut actual = Vec::new();
    loop {
        let response = compact(&service, nested_req.clone())?;
        let page = response.nested_page.expect("requested nested page");
        for item in &page.items {
            actual.push(serde_json::from_str::<Value>(&item.canonical_item_json)?);
        }
        let next_cursor = page.next_cursor;
        nested_req.presentation.nested.as_mut().unwrap().cursor = next_cursor.clone();
        if next_cursor.is_none() {
            break;
        }
    }
    assert_eq!(actual, expected);
    Ok(())
}

/// Real negative test: requesting a nested collection with an oversized byte_limit
/// or a stale/foreign cursor must fail closed with an explicit error, not silently
/// truncate or return another collection's page.
#[test]
fn nested_evolution_and_checked_olog_collections_reject_invalid_requests() -> Result<()> {
    let (_temp, service) = write_workspace()?;
    let mut req = request();
    req.olog_fragment = Some(works_for_olog_fragment(false));

    // Oversized byte_limit must be rejected before any items are computed.
    req.presentation.nested = Some(AuthoringNestedPageRequestV1 {
        collection: AuthoringNestedCollectionV1::CheckedOlogTypedHoles,
        limit: 1,
        byte_limit: MAX_NESTED_BYTE_LIMIT + 1,
        cursor: None,
    });
    assert!(service.execute_response(req.clone()).is_err());

    // A cursor issued for one collection must not validate against another.
    req.presentation.nested = Some(AuthoringNestedPageRequestV1 {
        collection: AuthoringNestedCollectionV1::CheckedOlogTypedHoles,
        limit: 1,
        byte_limit: MAX_NESTED_BYTE_LIMIT,
        cursor: None,
    });
    let response = compact(&service, req.clone())?;
    let stray_cursor = response
        .nested_page
        .and_then(|page| page.next_cursor)
        .unwrap_or_else(|| "not-a-real-cursor".to_string());
    req.presentation.nested = Some(AuthoringNestedPageRequestV1 {
        collection: AuthoringNestedCollectionV1::EvolutionOlogPrimitives,
        limit: 1,
        byte_limit: MAX_NESTED_BYTE_LIMIT,
        cursor: Some(stray_cursor),
    });
    assert!(
        service.execute_response(req).is_err(),
        "a cursor bound to one collection must not validate against a different collection"
    );
    Ok(())
}
