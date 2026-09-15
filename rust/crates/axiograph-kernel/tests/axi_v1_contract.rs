use std::{fs, path::PathBuf};

use axiograph_kernel::{
    CanonicalCompiler, CanonicalModuleSource, KernelCompilationRequest, KernelCompileError,
    RepositoryIdV2, RevisionDigestV2, SnapshotIdV2,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("canonicalize repository root")
}

fn compile_one(
    bytes: Vec<u8>,
) -> Result<axiograph_kernel::CompiledKernelSnapshot, Box<KernelCompileError>> {
    let source = CanonicalModuleSource::parse(bytes).map_err(Box::new)?;
    CanonicalCompiler::compile(KernelCompilationRequest {
        repository_id: RepositoryIdV2::from_descriptor_bytes(b"axi-v1-contract-test"),
        accepted_snapshot_id: SnapshotIdV2::from_canonical_fields(&[b"contract-snapshot"]),
        root_module: source.parsed().module_name.clone(),
        modules: vec![source],
    })
    .map_err(Box::new)
}

#[test]
fn exact_source_variants_keep_ast_order_but_have_distinct_revision_anchors() {
    let root = repo_root();
    let lf = fs::read(root.join("fixtures/canonical/contract/exact_lf.axi")).expect("read LF");
    let comment = fs::read(root.join("fixtures/canonical/contract/exact_comment.axi"))
        .expect("read comment variant");
    let crlf =
        fs::read(root.join("fixtures/canonical/contract/exact_crlf.axi")).expect("read CRLF");

    let exact_inputs = [&lf, &comment, &crlf];
    let sources = exact_inputs
        .map(|bytes| CanonicalModuleSource::parse(bytes.clone()).expect("parse exact source"));
    for (source, input) in sources.iter().zip(exact_inputs) {
        assert_eq!(source.parsed().module_name, "ExactBytes");
        assert_eq!(source.parsed().schemas[0].objects, ["B", "A"]);
        assert_eq!(source.parsed().schemas[1].objects, ["Z"]);
        assert_eq!(source.exact_text().as_bytes(), input.as_slice());
    }
    assert_eq!(sources[0].parsed(), sources[1].parsed());
    assert_eq!(sources[0].parsed(), sources[2].parsed());

    let anchors = [&lf, &comment, &crlf]
        .map(|bytes| RevisionDigestV2::from_accepted_bytes(bytes).expect("valid UTF-8"));
    assert_ne!(anchors[0], anchors[1]);
    assert_ne!(anchors[0], anchors[2]);
    assert_ne!(anchors[1], anchors[2]);
}

#[test]
fn parse_acceptance_does_not_imply_import_closure_formation() {
    let bytes = fs::read(repo_root().join("fixtures/canonical/contract/missing_import.axi"))
        .expect("read missing-import fixture");
    let parsed = CanonicalModuleSource::parse(bytes.clone()).expect("syntax is accepted");
    assert_eq!(parsed.parsed().imports, ["NotProvided"]);
    assert!(matches!(
        compile_one(bytes),
        Err(error)
            if matches!(*error, KernelCompileError::UnknownImport { ref module, ref import }
                if module == "MissingImport" && import == "NotProvided")
    ));
}

#[test]
fn role_key_refinement_is_a_canonical_formation_rejection() {
    let bytes = fs::read(
        repo_root().join("fixtures/canonical/contract/formation_reject_key_refinement.axi"),
    )
    .expect("read key-refinement fixture");
    let parsed = CanonicalModuleSource::parse(bytes.clone()).expect("syntax is accepted");
    assert_eq!(parsed.parsed().schemas[0].relations[0].fields.len(), 2);
    assert!(matches!(
        compile_one(bytes),
        Err(error)
            if matches!(*error, KernelCompileError::InvalidRefinement { ref detail, .. }
                if detail.contains("key(...) role refinements have no implemented finite witness"))
    ));
}

#[test]
fn well_formed_exact_byte_fixture_compiles_without_reordering() {
    let bytes = fs::read(repo_root().join("fixtures/canonical/contract/exact_lf.axi"))
        .expect("read valid fixture");
    let parsed = CanonicalModuleSource::parse(bytes.clone()).expect("parse exact source");
    assert_eq!(parsed.exact_text().as_bytes(), bytes);
    assert_eq!(parsed.parsed().schemas[0].objects, ["B", "A"]);
    assert_eq!(parsed.parsed().schemas[1].objects, ["Z"]);

    let compiled = compile_one(bytes).expect("canonical formation");
    assert_eq!(compiled.ir().schemas()[0].label, "First");
    assert_eq!(
        compiled.ir().schemas()[0]
            .objects
            .iter()
            .map(|object| object.label.as_str())
            .collect::<Vec<_>>(),
        ["B", "A"]
    );
    assert_eq!(compiled.ir().schemas()[1].label, "Second");
}

#[test]
fn invalid_utf8_is_a_boundary_rejection_before_parsing() {
    assert!(RevisionDigestV2::from_accepted_bytes(b"module Bad\n# \xff\n").is_err());
    assert!(CanonicalModuleSource::parse(b"module Bad\n# \xff\n".to_vec()).is_err());
}
