use super::*;
use http_body_util::BodyExt;

mod lsp_regressions;

const VALID: &str =
    "module Root\nschema Hiring:\n  object Company\n  relation Employment(company: Company)\n";

fn setup() -> Result<(
    tempfile::TempDir,
    AuthoringWorkspaceService,
    AuthoringWorkspaceRequestV1,
)> {
    let temp = tempfile::tempdir()?;
    std::fs::write(temp.path().join("Root.axi"), VALID)?;
    let service = AuthoringWorkspaceService::new(temp.path())?;
    let mut request = super::tests::request();
    request.axi_path = "Root.axi".into();
    request.baseline_axi_path = None;
    request.cq_path = None;
    request.query_ir_v1 = None;
    request.schema = None;
    Ok((temp, service, request))
}

#[test]
fn authoring_diagnostics_pretty_chains_preserve_typed_causes_without_duplicate_messages(
) -> Result<()> {
    use crate::axi_input::diagnostics::CanonicalSourceDiagnostic;
    use axiograph_kernel::KernelCompileError;
    let (_temp, service, _) = setup()?;
    let path = service.root().join("Root.axi");
    let error = crate::axi_input::compile_canonical_axi_path_with_root_bytes(
        &path,
        VALID
            .replace("company: Company", "company: Compny")
            .into_bytes(),
        &[service.root().to_path_buf()],
    )
    .unwrap_err();
    let diagnostic = error.downcast_ref::<CanonicalSourceDiagnostic>().unwrap();
    assert!(diagnostic.location.is_some());
    let KernelCompileError::RoleCarrier { cause, .. } = &diagnostic.cause else {
        panic!("typed role carrier")
    };
    assert!(
        matches!(cause.as_ref(), KernelCompileError::UnknownObjectTarget { target, .. } if target == "Compny")
    );
    let semantic_message = cause.to_string();
    assert_eq!(error.to_string(), semantic_message);
    let operation = format!("compile authoring input {}", path.display());
    let error = error.context(operation.clone());
    assert!(error.downcast_ref::<CanonicalSourceDiagnostic>().is_some());
    assert_eq!(
        format!("{error:#}"),
        format!("{operation}: {semantic_message}")
    );
    let error = crate::axi_input::compile_canonical_axi_path_with_root_bytes(
        &path,
        b"module Root\nschema S:\n object Company\n subtype Compny <: Company\n".to_vec(),
        &[service.root().to_path_buf()],
    )
    .unwrap_err();
    assert!(error.downcast_ref::<CanonicalSourceDiagnostic>().is_none());
    assert!(matches!(
        error.downcast_ref::<KernelCompileError>(),
        Some(KernelCompileError::UnknownObjectTarget { .. })
    ));
    let message = error.to_string();
    assert_eq!(
        format!("{:#}", error.context(operation.clone())),
        format!("{operation}: {message}")
    );
    Ok(())
}

#[test]
fn authoring_diagnostics_exact_unsaved_and_imported_occurrences() -> Result<()> {
    for newline in ["\n", "\r\n"] {
        let (temp, service, mut request) = setup()?;
        let invalid = [
            "module Base",
            "# Compny 😀",
            "schema Earlier:",
            "  object Compny",
            "  relation Previous(value: Compny)",
            "schema Hiring:",
            "  object Company",
            "  relation Employment(",
            "    first: refined(Company; eq(Compny)), # Compny",
            "\u{2003} next: indexed(refined(Compny; eq(Compny)); first),",
            "    last: Missing)",
        ]
        .join(newline);
        for imported in [false, true] {
            let root = if imported {
                "module Root\nimport Base\n".to_string()
            } else {
                invalid.replace("module Base", "module Root")
            };
            std::fs::write(temp.path().join("Base.axi"), &invalid)?;
            request.axi_text = Some(root.clone());
            let report = service.execute(request.clone())?;
            assert!(!report.ok, "{:?}", report.diagnostics);
            assert!(!report.validation.canonical_axi_valid);
            assert!(!report.validation.compiled_kernel_ir_valid);
            assert!(!report.promotion.candidate_reviewable);
            assert!(!report.promotion.protected_main_eligible);
            assert!(report.source.is_none());
            let location = report.diagnostics[0]
                .location
                .as_ref()
                .expect("precise carrier location");
            let image = if imported { &invalid } else { &root };
            assert_eq!(&image[location.byte_start..location.byte_end], "Compny");
            assert_eq!(
                location.byte_start,
                image.find("Compny; eq(Compny)").unwrap()
            );
            assert_eq!(location.module_name, if imported { "Base" } else { "Root" });
            assert_eq!(
                Path::new(&location.path),
                service
                    .root()
                    .join(if imported { "Base.axi" } else { "Root.axi" })
            );
            assert_eq!(
                location.revision_digest,
                axiograph_kernel::RevisionDigestV2::from_accepted_text(image).as_str()
            );
            assert_eq!(location.syntactic_role_carrier.schema_index, 1);
            assert_eq!(location.syntactic_role_carrier.role_index, 1);
            assert_eq!(location.suggested_name.as_deref(), Some("Company"));
            assert_eq!(location.start.line, 10);
            assert_eq!(location.start.lsp_line, 9);
            assert_eq!(location.end.lsp_character - location.start.lsp_character, 6);
            assert!(location.excerpt.starts_with('\u{2003}'));
            assert!(!location.excerpt.contains('\r'));
            let prefix = &image[..location.byte_start];
            let line_prefix = prefix.rsplit('\n').next().unwrap();
            assert_eq!(location.start.column, line_prefix.chars().count() + 1);
            assert_eq!(
                location.start.lsp_character as usize,
                line_prefix.encode_utf16().count()
            );
        }
        assert_eq!(
            std::fs::read_to_string(temp.path().join("Root.axi"))?,
            VALID
        );
        request.axi_text = Some(VALID.into());
        assert!(service.execute(request)?.diagnostics.is_empty());
    }
    Ok(())
}

#[test]
fn authoring_diagnostics_namespace_ties_excerpt_bounds_and_unlocated_fallback() -> Result<()> {
    let (_temp, service, mut request) = setup()?;
    for (source, expected) in [
        ("module Root\nschema S:\n object Company\n object Compnay\n relation R(x: Compny)\n", None),
        ("module Root\nschema S:\n object Company\n relation Employment(x: Company)\n relation R(x: relation(Employmnt))\n", Some("Employment")),
        ("module Root\nschema S:\n object Company\n relation R(x: Unknown)\n", None),
    ] {
        request.axi_text = Some(source.into());
        let report = service.execute(request.clone())?;
        assert_eq!(report.diagnostics[0].location.as_ref().unwrap().suggested_name.as_deref(), expected);
    }
    request.axi_text = Some(format!(
        "module Root\nschema S:\n object Company\n relation R({}x: Compny) # {}\n",
        " ".repeat(300),
        "😀".repeat(300)
    ));
    let report = service.execute(request.clone())?;
    let location = report.diagnostics[0].location.as_ref().unwrap();
    assert!(location.excerpt_truncated);
    assert!(location.excerpt.chars().count() <= 240);
    assert!(location.excerpt.contains("Compny"));
    for source in [
        "module Root\nschema S:\n object Company\n subtype Compny <: Company\n",
        "module Root\nimport Absent\n",
        "module Root\nschema S:\n relation R(x: )\n",
    ] {
        request.axi_text = Some(source.into());
        let report = service.execute(request.clone())?;
        assert!(!report.ok);
        assert!(report.diagnostics[0].location.is_none());
        assert!(report.diagnostics[0].path.is_none());
        let lsp = authoring_diagnostic_to_lsp(report.diagnostics[0].clone());
        assert_eq!(lsp.data.unwrap()["sourceLocated"], false);
        assert_eq!(lsp.range.start, lsp.range.end);
    }
    Ok(())
}

#[test]
fn authoring_diagnostics_http_mcp_lsp_full_compact_schema_parity() -> Result<()> {
    let (_temp, service, mut request) = setup()?;
    request.axi_text = Some(VALID.replace("company: Company", "company: Compny"));
    let full = serde_json::to_value(service.execute(request.clone())?)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let validator = jsonschema::validator_for(&projection::response_schema())?;
    for detail in [
        AuthoringDetailV1::Full,
        AuthoringDetailV1::Summary,
        AuthoringDetailV1::Standard,
    ] {
        request.presentation.detail = detail;
        request.presentation.sections = (detail != AuthoringDetailV1::Full)
            .then(|| vec![projection::AuthoringSectionV1::Diagnostics]);
        request.presentation.limit = 1;
        let expected = serde_json::to_value(service.execute_response(request.clone())?)?;
        assert!(validator.is_valid(&expected), "{expected}");
        assert_eq!(expected["ok"], false);
        assert_eq!(expected["promotion"], full["promotion"]);
        assert_eq!(expected["trust"], full["trust"]);
        let diagnostics = if detail == AuthoringDetailV1::Full {
            &expected["diagnostics"]
        } else {
            &expected["sections"]
                .as_array()
                .unwrap()
                .iter()
                .find(|page| page["section"] == "diagnostics")
                .unwrap()["items"]
        };
        assert_eq!(diagnostics, &full["diagnostics"]);
        let http = execute_http_payload(&service, &serde_json::to_vec(&request)?);
        assert_eq!(http.status(), StatusCode::OK);
        assert_eq!(
            serde_json::from_slice::<Value>(
                &runtime.block_on(http.into_body().collect())?.to_bytes()
            )?,
            expected
        );
        let mcp = execute_mcp_payload(
            &service,
            serde_json::from_value(
                json!({"name":AUTHORING_WORKSPACE_TOOL_NAME,"arguments":request}),
            )?,
        );
        assert_eq!(serde_json::to_value(mcp)?["structuredContent"], expected);
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
                json!({"command":AUTHORING_WORKSPACE_LSP_COMMAND,"arguments":[request]}),
            ),
        ) else {
            panic!("response")
        };
        assert_eq!(response.response_result.unwrap(), expected);
        let mut malformed = expected.clone();
        if detail == AuthoringDetailV1::Full {
            malformed["diagnostics"][0]["location"]["byte_start"] = json!(-1);
        } else {
            let page = malformed["sections"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|page| page["section"] == "diagnostics")
                .unwrap();
            page["items"][0]["location"]["byte_start"] = json!(-1);
        }
        assert!(!validator.is_valid(&malformed));
    }
    Ok(())
}

fn notify(state: &mut AuthoringWorkspaceLspState, method: &str, params: Value) -> Vec<Value> {
    // Exercise the production bounded frame decoder and notification dispatcher.
    let message = Message::Notification(Notification::new(method.into(), params));
    let mut bytes = Vec::new();
    write_bounded_lsp_message(&mut bytes, &message).unwrap();
    let decoded = read_bounded_lsp_message(&mut std::io::Cursor::new(bytes))
        .unwrap()
        .unwrap();
    let Message::Notification(notification) = decoded else {
        panic!("notification")
    };
    handle_lsp_notification(state, notification)
        .into_iter()
        .map(|message| {
            let Message::Notification(notification) = message else {
                panic!("publication")
            };
            notification.params
        })
        .collect()
}

#[test]
fn authoring_diagnostics_lsp_changed_import_and_new_file_are_explicitly_unlocated() -> Result<()> {
    let (temp, service, _) = setup()?;
    let base = VALID
        .replace("module Root", "module Base")
        .replace("company: Company", "company: Compny");
    std::fs::write(temp.path().join("Base.axi"), &base)?;
    let root_uri = url::Url::from_file_path(service.root().join("Root.axi"))
        .unwrap()
        .to_string();
    let base_uri = url::Url::from_file_path(service.root().join("Base.axi"))
        .unwrap()
        .to_string();
    let missing_uri = url::Url::from_file_path(service.root().join("Missing.axi"))
        .unwrap()
        .to_string();
    let mut state = AuthoringWorkspaceLspState {
        service,
        default_axi_path: None,
        documents: BTreeMap::new(),
        document_images_incomplete: false,
        publications: BTreeMap::new(),
    };
    notify(
        &mut state,
        "textDocument/didOpen",
        json!({"textDocument":{"uri":root_uri,"text":"module Root\nimport Base\n"}}),
    );
    let change = notify(
        &mut state,
        "textDocument/didOpen",
        json!({"textDocument":{"uri":base_uri,"text":base.replace("Compny", "Company")}}),
    );
    assert!(change
        .iter()
        .any(|p| p["uri"] == base_uri && p["diagnostics"] == json!([])));
    let stale = &change.iter().find(|p| p["uri"] == root_uri).unwrap()["diagnostics"][0];
    assert_eq!(stale["data"]["sourceLocated"], false);
    assert_eq!(stale["range"]["start"], stale["range"]["end"]);
    let revalidate = notify(
        &mut state,
        "textDocument/didChange",
        json!({"textDocument":{"uri":root_uri},"contentChanges":[{"text":"module Root\nimport Base\n"}]}),
    );
    assert_eq!(
        revalidate.iter().find(|p| p["uri"] == root_uri).unwrap()["diagnostics"][0]["data"]
            ["sourceLocated"],
        false
    );
    let missing = notify(
        &mut state,
        "textDocument/didOpen",
        json!({"textDocument":{"uri":missing_uri,"text":VALID}}),
    );
    assert_eq!(
        missing.iter().find(|p| p["uri"] == missing_uri).unwrap()["diagnostics"][0]["code"],
        "authoring_lsp_document_unsupported"
    );
    assert!(!temp.path().join("Missing.axi").exists());
    assert_eq!(std::fs::read_to_string(temp.path().join("Base.axi"))?, base);
    Ok(())
}

#[test]
fn authoring_diagnostics_baseline_and_ambiguous_import_do_not_borrow_root_location() -> Result<()> {
    let (temp, service, mut request) = setup()?;
    let invalid = VALID
        .replace("module Root", "module Base")
        .replace("company: Company", "company: Compny");
    std::fs::write(temp.path().join("Base.axi"), &invalid)?;
    request.baseline_axi_path = Some("Base.axi".into());
    let report = service.execute(request.clone())?;
    assert!(report.validation.canonical_axi_valid);
    assert!(!report.ok);
    assert_eq!(
        report.diagnostics[0].code,
        "authoring_baseline_compile_failed"
    );
    assert_eq!(
        report.diagnostics[0].location.as_ref().unwrap().module_name,
        "Base"
    );
    request.baseline_axi_path = None;
    request.axi_text = Some("module Root\nimport Base\n".into());
    std::fs::write(temp.path().join("Duplicate.axi"), &invalid)?;
    let report = service.execute(request.clone())?;
    assert!(!report.ok);
    assert!(report.diagnostics[0].message.contains("ambiguous"));
    assert!(report.diagnostics[0].location.is_none());
    assert!(report.diagnostics[0].path.is_none());
    request.presentation.sections = Some(vec![projection::AuthoringSectionV1::Diagnostics]);
    request.presentation.cursor = Some("untrusted-cursor".into());
    assert!(service
        .execute_response(request)
        .unwrap_err()
        .to_string()
        .contains("closure compilation failed"));
    Ok(())
}

#[test]
fn authoring_diagnostics_lsp_import_routing_correction_close_and_unsaved_root() -> Result<()> {
    let (temp, service, _) = setup()?;
    let base = VALID
        .replace("module Root", "module Base")
        .replace("company: Company", "company: Compny")
        .replace('\n', "\r\n");
    std::fs::write(temp.path().join("Base.axi"), &base)?;
    let root_uri = url::Url::from_file_path(service.root().join("Root.axi"))
        .unwrap()
        .to_string();
    let base_uri = url::Url::from_file_path(service.root().join("Base.axi"))
        .unwrap()
        .to_string();
    let mut state = AuthoringWorkspaceLspState {
        service,
        default_axi_path: None,
        documents: BTreeMap::new(),
        document_images_incomplete: false,
        publications: BTreeMap::new(),
    };
    let open = notify(
        &mut state,
        "textDocument/didOpen",
        json!({"textDocument":{"uri":root_uri,"text":"module Root\nimport Base\n"}}),
    );
    let diagnostic = &open.iter().find(|p| p["uri"] == base_uri).unwrap()["diagnostics"][0];
    assert_eq!(diagnostic["data"]["sourceLocated"], true);
    assert_eq!(diagnostic["range"]["start"]["line"], 3);
    assert_eq!(
        diagnostic["range"]["end"]["character"].as_u64().unwrap()
            - diagnostic["range"]["start"]["character"].as_u64().unwrap(),
        6
    );
    let correction = notify(
        &mut state,
        "textDocument/didChange",
        json!({"textDocument":{"uri":root_uri},"contentChanges":[{"text":VALID}]}),
    );
    assert!(correction
        .iter()
        .any(|p| p["uri"] == base_uri && p["diagnostics"] == json!([])));
    let invalid = VALID.replace("company: Company", "company: Compny");
    let edited = notify(
        &mut state,
        "textDocument/didChange",
        json!({"textDocument":{"uri":root_uri},"contentChanges":[{"text":invalid}]}),
    );
    assert_eq!(edited[0]["uri"], root_uri);
    assert_eq!(edited[0]["diagnostics"][0]["data"]["sourceLocated"], true);
    let close = notify(
        &mut state,
        "textDocument/didClose",
        json!({"textDocument":{"uri":root_uri}}),
    );
    assert_eq!(close[0]["diagnostics"], json!([]));
    assert!(state.publications.is_empty());
    assert_eq!(
        std::fs::read_to_string(temp.path().join("Root.axi"))?,
        VALID
    );
    Ok(())
}
