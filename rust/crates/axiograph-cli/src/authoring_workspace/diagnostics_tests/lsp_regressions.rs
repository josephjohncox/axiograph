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

#[test]
fn authoring_diagnostics_lsp_did_save_revalidates_other_open_documents() -> Result<()> {
    let (_temp, mut state, root_uri, base_uri, base) = import_lsp_fixture()?;
    notify(
        &mut state,
        "textDocument/didOpen",
        json!({"textDocument":{"uri":base_uri,"text":base}}),
    );
    revalidate_import(&mut state, &root_uri);

    // didSave republishes the saved document's own diagnostics and must also
    // deterministically revalidate every other currently open document (here, the
    // open Root that imports Base), even though didSave in full-document sync mode
    // carries no text of its own and Root's own buffer text never changed.
    let saved = notify(
        &mut state,
        "textDocument/didSave",
        json!({"textDocument":{"uri":base_uri}}),
    );
    assert!(
        saved.iter().any(|p| p["uri"] == base_uri),
        "didSave must republish the saved document's own diagnostics: {saved:?}"
    );
    assert!(
        saved.iter().any(|p| p["uri"] == root_uri),
        "didSave must deterministically revalidate every other open document: {saved:?}"
    );
    Ok(())
}

#[test]
fn authoring_diagnostics_lsp_did_close_revalidates_other_open_documents() -> Result<()> {
    let (_temp, mut state, root_uri, base_uri, base) = import_lsp_fixture()?;
    notify(
        &mut state,
        "textDocument/didOpen",
        json!({"textDocument":{"uri":base_uri,"text":base}}),
    );
    revalidate_import(&mut state, &root_uri);

    // Closing Base drops its in-memory override; every other currently open document
    // (here, Root) must be deterministically revalidated rather than silently keep
    // serving diagnostics computed while Base was open.
    let closed = notify(
        &mut state,
        "textDocument/didClose",
        json!({"textDocument":{"uri":base_uri}}),
    );
    assert!(
        closed.iter().any(|p| p["uri"] == root_uri),
        "didClose must deterministically revalidate every other open document: {closed:?}"
    );
    Ok(())
}

#[test]
fn authoring_diagnostics_lsp_rejects_out_of_order_did_change_versions() -> Result<()> {
    let (_temp, service, _) = setup()?;
    let root_uri = url::Url::from_file_path(service.root().join("Root.axi"))
        .map_err(|_| anyhow!("build test file URI"))?
        .to_string();
    let mut state = AuthoringWorkspaceLspState {
        service,
        default_axi_path: None,
        documents: BTreeMap::new(),
        document_images_incomplete: false,
        publications: BTreeMap::new(),
    };
    let opened = notify(
        &mut state,
        "textDocument/didOpen",
        json!({"textDocument":{"uri":root_uri,"version":5,"text":VALID}}),
    );
    assert!(!opened
        .iter()
        .any(|p| p["diagnostics"][0]["code"] == "authoring.stale_version"));
    assert_eq!(state.documents.get(&root_uri).unwrap().version, Some(5));

    // A same-or-lower version than the last tracked one is stale/out-of-order and
    // must fail closed: the image is dropped and a typed diagnostic is published,
    // never a silent best-effort merge of older text over newer state.
    let stale = notify(
        &mut state,
        "textDocument/didChange",
        json!({"textDocument":{"uri":root_uri,"version":5},"contentChanges":[{"text":"module Root\n"}]}),
    );
    assert!(stale
        .iter()
        .any(|p| p["diagnostics"][0]["code"] == "authoring.stale_version"));
    assert!(!state.documents.contains_key(&root_uri));
    assert!(state.document_images_incomplete);

    // Recover by reopening in-order; state is precise again once a fresh version
    // sequence is established.
    state.document_images_incomplete = false;
    let reopened = notify(
        &mut state,
        "textDocument/didOpen",
        json!({"textDocument":{"uri":root_uri,"version":6,"text":VALID}}),
    );
    assert!(!reopened
        .iter()
        .any(|p| p["diagnostics"][0]["code"] == "authoring.stale_version"));
    let advanced = notify(
        &mut state,
        "textDocument/didChange",
        json!({"textDocument":{"uri":root_uri,"version":7},"contentChanges":[{"text":VALID}]}),
    );
    assert!(!advanced
        .iter()
        .any(|p| p["diagnostics"][0]["code"] == "authoring.stale_version"));
    assert_eq!(state.documents.get(&root_uri).unwrap().version, Some(7));
    Ok(())
}

#[test]
fn utf16_range_edit_replaces_a_bmp_character_at_the_correct_byte_offset() {
    // "café" -- 'é' is one Unicode scalar value and one UTF-16 code unit, but
    // two UTF-8 bytes, so a naive byte-offset range edit would corrupt this.
    let text = "café\n";
    let range = LspRangeV1 {
        start: LspPositionV1 {
            line: 0,
            character: 3,
        },
        end: LspPositionV1 {
            line: 0,
            character: 4,
        },
    };
    let edited = apply_lsp_range_edit(text, range, "e").expect("valid BMP range edit");
    assert_eq!(edited, "cafe\n");
}

#[test]
fn utf16_range_edit_replaces_an_astral_code_point_spanning_two_utf16_units() {
    // U+1F600 (😀) is one Unicode scalar value, encoded as two UTF-16 code
    // units (a surrogate pair) and four UTF-8 bytes. Deleting it must consume
    // both UTF-16 units, not one.
    let text = "a😀b";
    let range = LspRangeV1 {
        start: LspPositionV1 {
            line: 0,
            character: 1,
        },
        end: LspPositionV1 {
            line: 0,
            character: 3,
        },
    };
    let edited = apply_lsp_range_edit(text, range, "X").expect("valid astral range edit");
    assert_eq!(edited, "aXb");
}

#[test]
fn utf16_range_edit_rejects_a_boundary_inside_an_astral_surrogate_pair() {
    // character: 2 lands between the high and low surrogate of 😀's UTF-16
    // encoding -- there is no valid text position there, and accepting it
    // would silently split one scalar value's bytes.
    let text = "a😀b";
    let range = LspRangeV1 {
        start: LspPositionV1 {
            line: 0,
            character: 2,
        },
        end: LspPositionV1 {
            line: 0,
            character: 3,
        },
    };
    let error = apply_lsp_range_edit(text, range, "X")
        .expect_err("boundary inside surrogate pair must fail closed");
    assert_eq!(error, LspRangeEditErrorV1::OutOfBounds);
}

#[test]
fn utf16_range_edit_rejects_an_inverted_range() {
    let text = "abc\n";
    let range = LspRangeV1 {
        start: LspPositionV1 {
            line: 0,
            character: 2,
        },
        end: LspPositionV1 {
            line: 0,
            character: 0,
        },
    };
    let error = apply_lsp_range_edit(text, range, "").expect_err("inverted range must fail closed");
    assert_eq!(error, LspRangeEditErrorV1::InvertedRange);
}

#[test]
fn utf16_range_edit_rejects_a_line_beyond_the_document() {
    let text = "abc\n";
    let range = LspRangeV1 {
        start: LspPositionV1 {
            line: 5,
            character: 0,
        },
        end: LspPositionV1 {
            line: 5,
            character: 0,
        },
    };
    let error =
        apply_lsp_range_edit(text, range, "x").expect_err("out-of-range line must fail closed");
    assert_eq!(error, LspRangeEditErrorV1::OutOfBounds);
}

#[test]
fn utf16_range_edit_rejects_a_character_beyond_the_line_length() {
    let text = "abc\n";
    let range = LspRangeV1 {
        start: LspPositionV1 {
            line: 0,
            character: 99,
        },
        end: LspPositionV1 {
            line: 0,
            character: 99,
        },
    };
    let error = apply_lsp_range_edit(text, range, "x")
        .expect_err("out-of-range character must fail closed");
    assert_eq!(error, LspRangeEditErrorV1::OutOfBounds);
}

#[test]
fn utf16_content_changes_sequence_applies_multiple_range_edits_in_order() {
    let base = "module Root\n";
    let changes = vec![
        json!({
            "range": {"start": {"line": 0, "character": 7}, "end": {"line": 0, "character": 11}},
            "text": "Renamed",
        }),
        json!({
            "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 6}},
            "text": "MODULE",
        }),
    ];
    let result = apply_lsp_content_changes(base, &changes).expect("sequential range edits");
    assert_eq!(result, "MODULE Renamed\n");
}

#[test]
fn authoring_diagnostics_lsp_did_change_applies_incremental_range_edits_via_utf16_offsets(
) -> Result<()> {
    let (_temp, service, _) = setup()?;
    let root_uri = url::Url::from_file_path(service.root().join("Root.axi"))
        .map_err(|_| anyhow!("build test file URI"))?
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
        json!({"textDocument":{"uri":root_uri,"version":1,"text":"module Root\n"}}),
    );
    assert_eq!(
        state.documents.get(&root_uri).unwrap().text,
        "module Root\n"
    );

    // Incremental range edit: rename `Root` -> `Renamed` in place via a UTF-16
    // range, not a full-text replacement. If the server fell back to treating
    // this as full text it would corrupt the document instead of renaming it.
    notify(
        &mut state,
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": root_uri, "version": 2},
            "contentChanges": [{
                "range": {"start": {"line": 0, "character": 7}, "end": {"line": 0, "character": 11}},
                "text": "Renamed",
            }],
        }),
    );
    assert_eq!(
        state.documents.get(&root_uri).unwrap().text,
        "module Renamed\n"
    );
    assert_eq!(state.documents.get(&root_uri).unwrap().version, Some(2));
    assert!(!state.document_images_incomplete);
    Ok(())
}

#[test]
fn authoring_diagnostics_lsp_did_change_range_edit_on_untracked_document_fails_closed() -> Result<()>
{
    let (_temp, service, _) = setup()?;
    let root_uri = url::Url::from_file_path(service.root().join("Root.axi"))
        .map_err(|_| anyhow!("build test file URI"))?
        .to_string();
    let mut state = AuthoringWorkspaceLspState {
        service,
        default_axi_path: None,
        documents: BTreeMap::new(),
        document_images_incomplete: false,
        publications: BTreeMap::new(),
    };
    // No prior didOpen for this URI: a range edit has no base text to apply
    // against and must fail closed rather than fabricate a document from an
    // empty base.
    let messages = notify(
        &mut state,
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": root_uri, "version": 1},
            "contentChanges": [{
                "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 0}},
                "text": "x",
            }],
        }),
    );
    assert!(messages
        .iter()
        .any(|p| p["diagnostics"][0]["code"] == "authoring.stale_version"));
    assert!(!state.documents.contains_key(&root_uri));
    assert!(state.document_images_incomplete);
    Ok(())
}

#[test]
fn authoring_diagnostics_lsp_did_change_rejects_an_astral_surrogate_boundary_and_drops_the_image(
) -> Result<()> {
    let (_temp, service, _) = setup()?;
    let root_uri = url::Url::from_file_path(service.root().join("Root.axi"))
        .map_err(|_| anyhow!("build test file URI"))?
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
        json!({"textDocument":{"uri":root_uri,"version":1,"text":"a😀b\n"}}),
    );
    // character: 2 splits the surrogate pair for 😀; this must fail closed
    // (drop the tracked image, publish the resource/stale diagnostic) instead
    // of silently corrupting the stored text.
    let messages = notify(
        &mut state,
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": root_uri, "version": 2},
            "contentChanges": [{
                "range": {"start": {"line": 0, "character": 2}, "end": {"line": 0, "character": 3}},
                "text": "X",
            }],
        }),
    );
    assert!(messages
        .iter()
        .any(|p| p["diagnostics"][0]["code"] == "authoring.stale_version"));
    assert!(!state.documents.contains_key(&root_uri));
    assert!(state.document_images_incomplete);
    Ok(())
}

fn any_published_diagnostic_contains(publications: &[Value], needle: &str) -> bool {
    publications.iter().any(|publication| {
        publication["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diagnostic| {
                diagnostic["message"]
                    .as_str()
                    .unwrap_or_default()
                    .contains(needle)
            })
    })
}

#[test]
fn authoring_diagnostics_lsp_import_resolves_against_open_unsaved_importee_overlay() -> Result<()> {
    // `Base.axi` is written to disk with the typo, but the client's open
    // in-memory buffer (never saved) fixes it. Diagnostics for the root must
    // reflect the *unsaved* importee overlay, not the stale disk bytes, and
    // no disk write is allowed for it to happen.
    let (temp, mut state, root_uri, base_uri, base) = import_lsp_fixture()?;
    let fixed_base = base.replace("Compny", "Company");
    notify(
        &mut state,
        "textDocument/didOpen",
        json!({"textDocument":{"uri":base_uri,"version":1,"text":fixed_base}}),
    );
    let published = revalidate_import(&mut state, &root_uri);
    assert!(
        !any_published_diagnostic_contains(&published, "Compny"),
        "unsaved importee overlay must resolve the typo without a disk save: {published:?}"
    );
    // The disk file underneath the unsaved overlay must remain byte-identical.
    assert_eq!(std::fs::read_to_string(temp.path().join("Base.axi"))?, base);
    Ok(())
}

#[test]
fn authoring_diagnostics_lsp_import_overlay_reverts_when_importee_closes() -> Result<()> {
    // Once the unsaved importee buffer closes, its overlay must stop
    // applying: the importer falls back to the disk bytes exactly as before,
    // with no residual overlay state leaking into a later resolution.
    let (temp, mut state, root_uri, base_uri, base) = import_lsp_fixture()?;
    let fixed_base = base.replace("Compny", "Company");
    notify(
        &mut state,
        "textDocument/didOpen",
        json!({"textDocument":{"uri":base_uri,"version":1,"text":fixed_base}}),
    );
    let overlaid = revalidate_import(&mut state, &root_uri);
    assert!(!any_published_diagnostic_contains(&overlaid, "Compny"));
    notify(
        &mut state,
        "textDocument/didClose",
        json!({"textDocument":{"uri":base_uri}}),
    );
    let after_close = revalidate_import(&mut state, &root_uri);
    assert!(
        any_published_diagnostic_contains(&after_close, "Compny"),
        "closing the unsaved importee must revert to disk bytes: {after_close:?}"
    );
    assert_eq!(std::fs::read_to_string(temp.path().join("Base.axi"))?, base);
    Ok(())
}

#[test]
fn authoring_diagnostics_lsp_import_overlay_does_not_apply_to_unrelated_module_name() -> Result<()>
{
    // An open document whose canonical path does not match the import's
    // expected `<import>.axi` candidate path must never be treated as an
    // overlay for that import (e.g. an unrelated open file named similarly
    // under a different directory it does not actually resolve to).
    let (temp, mut state, root_uri, _base_uri, base) = import_lsp_fixture()?;
    std::fs::create_dir(temp.path().join("nested"))?;
    let unrelated_uri = url::Url::from_file_path(temp.path().join("nested").join("Other.axi"))
        .map_err(|_| anyhow!("build unrelated test file URI"))?
        .to_string();
    notify(
        &mut state,
        "textDocument/didOpen",
        json!({"textDocument":{"uri":unrelated_uri,"version":1,"text":"module Other\n"}}),
    );
    let published = revalidate_import(&mut state, &root_uri);
    assert!(
        any_published_diagnostic_contains(&published, "Compny"),
        "an unrelated open document must not shadow the real import resolution: {published:?}"
    );
    assert_eq!(std::fs::read_to_string(temp.path().join("Base.axi"))?, base);
    Ok(())
}
