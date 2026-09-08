use super::*;

fn import_lsp_fixture() -> Result<(
    tempfile::TempDir,
    AuthoringWorkspaceLspState,
    String,
    String,
    String,
)> {
    let (temp, service, _) = setup()?;
    let base = VALID
        .replace("module Root", "module Base")
        .replace("company: Company", "company: Compny");
    std::fs::write(temp.path().join("Base.axi"), &base)?;
    let uri = |name| {
        url::Url::from_file_path(service.root().join(name))
            .unwrap()
            .to_string()
    };
    let root_uri = uri("Root.axi");
    let base_uri = uri("Base.axi");
    Ok((
        temp,
        AuthoringWorkspaceLspState {
            service,
            default_axi_path: None,
            documents: BTreeMap::new(),
            document_images_incomplete: false,
            publications: BTreeMap::new(),
        },
        root_uri,
        base_uri,
        base,
    ))
}

fn revalidate_import(state: &mut AuthoringWorkspaceLspState, root_uri: &str) -> Vec<Value> {
    notify(
        state,
        "textDocument/didChange",
        json!({"textDocument":{"uri":root_uri},"contentChanges":[{"text":"module Root\nimport Base\n"}]}),
    )
}

fn assert_unlocated_import(publications: &[Value], root_uri: &str) {
    let diagnostics = publications.iter().find(|p| p["uri"] == root_uri).unwrap()["diagnostics"]
        .as_array()
        .unwrap();
    assert!(diagnostics
        .iter()
        .any(|d| d["message"].as_str().unwrap().contains("Compny")));
    for publication in publications {
        for diagnostic in publication["diagnostics"].as_array().unwrap() {
            assert_eq!(diagnostic["data"]["sourceLocated"], false, "{diagnostic}");
            assert!(diagnostic["data"]["location"].is_null());
            assert_eq!(diagnostic["range"]["start"], diagnostic["range"]["end"]);
        }
    }
}

#[test]
fn authoring_diagnostics_lsp_equivalent_uri_import_is_unlocated() -> Result<()> {
    let (temp, mut state, root_uri, base_uri, base) = import_lsp_fixture()?;
    let alias = base_uri.replace("Base.axi", "%42ase.axi");
    let initial = revalidate_import(&mut state, &root_uri);
    assert_eq!(
        initial.iter().find(|p| p["uri"] == base_uri).unwrap()["diagnostics"][0]["data"]
            ["sourceLocated"],
        true
    );
    let opened = notify(
        &mut state,
        "textDocument/didOpen",
        json!({"textDocument":{"uri":alias,"text":base.replace("Compny", "Company")}}),
    );
    assert_unlocated_import(&opened, &root_uri);
    assert_unlocated_import(&revalidate_import(&mut state, &root_uri), &root_uri);
    assert_eq!(std::fs::read_to_string(temp.path().join("Base.axi"))?, base);
    Ok(())
}

#[test]
fn authoring_diagnostics_lsp_alias_publication_and_ambiguity() -> Result<()> {
    let (_temp, mut state, root_uri, base_uri, base) = import_lsp_fixture()?;
    let alias = base_uri.replace("Base.axi", "%42ase.axi");
    let opened = notify(
        &mut state,
        "textDocument/didOpen",
        json!({"textDocument":{"uri":alias,"text":base}}),
    );
    assert_eq!(
        opened.iter().find(|p| p["uri"] == alias).unwrap()["diagnostics"][0]["data"]
            ["sourceLocated"],
        true
    );
    let imported = revalidate_import(&mut state, &root_uri);
    assert_eq!(
        imported.iter().find(|p| p["uri"] == alias).unwrap()["diagnostics"][0]["data"]
            ["sourceLocated"],
        true
    );
    // Equal images under two simultaneously open aliases cannot select a unique owner.
    let ambiguous = notify(
        &mut state,
        "textDocument/didOpen",
        json!({"textDocument":{"uri":base_uri,"text":base}}),
    );
    assert_unlocated_import(&ambiguous, &root_uri);
    assert_unlocated_import(&revalidate_import(&mut state, &root_uri), &root_uri);
    notify(
        &mut state,
        "textDocument/didClose",
        json!({"textDocument":{"uri":base_uri}}),
    );
    // Invalidation is not recompilation: refresh the alias's own old unlocated report too.
    notify(
        &mut state,
        "textDocument/didChange",
        json!({"textDocument":{"uri":alias},"contentChanges":[{"text":base}]}),
    );
    let recovered = revalidate_import(&mut state, &root_uri);
    assert_eq!(
        recovered.iter().find(|p| p["uri"] == alias).unwrap()["diagnostics"][0]["data"]
            ["sourceLocated"],
        true
    );
    Ok(())
}

#[test]
fn authoring_diagnostics_lsp_count_rejection_invalidates_import_locations() -> Result<()> {
    let (temp, mut state, root_uri, base_uri, base) = import_lsp_fixture()?;
    revalidate_import(&mut state, &root_uri);
    for index in 1..MAX_AUTHORING_LSP_DOCUMENTS {
        let uri = url::Url::from_file_path(temp.path().join(format!("filler{index}.txt")))
            .unwrap()
            .to_string();
        notify(
            &mut state,
            "textDocument/didOpen",
            json!({"textDocument":{"uri":uri,"text":""}}),
        );
    }
    let rejected = notify(
        &mut state,
        "textDocument/didOpen",
        json!({"textDocument":{"uri":base_uri,"text":base.replace("Compny", "Company")}}),
    );
    assert!(rejected
        .iter()
        .any(|p| p["diagnostics"][0]["code"] == "authoring.resource_limit"));
    assert_unlocated_import(&rejected, &root_uri);
    assert_unlocated_import(&revalidate_import(&mut state, &root_uri), &root_uri);
    assert_eq!(state.documents.len(), MAX_AUTHORING_LSP_DOCUMENTS);
    let publication_count = state.publications.len();
    for index in 0..2 * MAX_AUTHORING_LSP_DOCUMENTS {
        let rejected_uri = base_uri.replace("Base.axi", &format!("Rejected{index}.axi"));
        notify(
            &mut state,
            "textDocument/didOpen",
            json!({"textDocument":{"uri":rejected_uri,"text":""}}),
        );
    }
    assert!(state.document_images_incomplete);
    assert_eq!(state.documents.len(), MAX_AUTHORING_LSP_DOCUMENTS);
    assert_eq!(state.publications.len(), publication_count);
    notify(
        &mut state,
        "textDocument/didClose",
        json!({"textDocument":{"uri":base_uri}}),
    );
    assert_unlocated_import(&revalidate_import(&mut state, &root_uri), &root_uri);
    assert_eq!(std::fs::read_to_string(temp.path().join("Base.axi"))?, base);
    Ok(())
}

#[test]
fn authoring_diagnostics_lsp_aggregate_rejection_invalidates_cached_revision() -> Result<()> {
    let (temp, mut state, root_uri, base_uri, base) = import_lsp_fixture()?;
    revalidate_import(&mut state, &root_uri);
    notify(
        &mut state,
        "textDocument/didOpen",
        json!({"textDocument":{"uri":base_uri,"text":base}}),
    );
    let root_text = "module Root\nimport Base\n";
    let filler = " ".repeat((MAX_AUTHORING_LSP_TOTAL_BYTES - base.len() - root_text.len()) / 8);
    for index in 0..8 {
        let uri = url::Url::from_file_path(temp.path().join(format!("filler{index}.txt")))
            .unwrap()
            .to_string();
        let messages = notify(
            &mut state,
            "textDocument/didOpen",
            json!({"textDocument":{"uri":uri,"text":filler}}),
        );
        assert!(!messages
            .iter()
            .any(|p| p["diagnostics"][0]["code"] == "authoring.resource_limit"));
    }
    let corrected = base.replace("Compny", "Company") + &" ".repeat(4096);
    let rejected = notify(
        &mut state,
        "textDocument/didChange",
        json!({"textDocument":{"uri":base_uri},"contentChanges":[{"text":corrected}]}),
    );
    assert!(rejected
        .iter()
        .any(|p| p["diagnostics"][0]["code"] == "authoring.resource_limit"));
    assert_unlocated_import(&rejected, &root_uri);
    assert_unlocated_import(&revalidate_import(&mut state, &root_uri), &root_uri);
    let action = handle_lsp_request(
        &mut state,
        LspRequest::new(
            2.into(),
            "textDocument/codeAction".into(),
            json!({"textDocument":{"uri":base_uri}}),
        ),
    );
    let Message::Response(action) = action else {
        panic!("response")
    };
    assert_eq!(
        action.response_result.unwrap(),
        json!([]),
        "rejected revision must not leave a stale code action"
    );
    assert_eq!(std::fs::read_to_string(temp.path().join("Base.axi"))?, base);
    Ok(())
}
