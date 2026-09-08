use super::projection::AuthoringWorkspaceResponseV1;
use super::*;

fn setup(
    base: &str,
    extension: &str,
) -> Result<(
    tempfile::TempDir,
    AuthoringWorkspaceService,
    AuthoringWorkspaceRequestV1,
)> {
    let temp = tempfile::tempdir()?;
    std::fs::write(temp.path().join("Base.axi"), base)?;
    std::fs::write(temp.path().join("Extension.axi"), extension)?;
    let service = AuthoringWorkspaceService::new(temp.path())?;
    let mut request = super::tests::request();
    request.axi_path = "Extension.axi".into();
    request.baseline_axi_path = None;
    request.cq_path = None;
    request.query_ir_v1 = None;
    request.schema = None;
    Ok((temp, service, request))
}
const BASE: &str = "module Base\nschema Shared:\n  object Person\n  relation Parent(child: Person, parent: Person)\n";
const EXTENSION: &str = "module Extension\nimport Base\ninstance Family of Shared:\n  Person = {Alice, Bob}\n  Parent = {p: (child=Alice, parent=Bob)}\n";

#[test]
fn authoring_workspace_package_metadata_only_is_not_a_data_import() -> Result<()> {
    let (_temp, service, request) = setup(BASE, "module Extension\nimport Base\n")?;
    let report = service.execute(request)?;
    assert!(report.ok, "{:?}", report.diagnostics);
    assert!(report.validation.canonical_axi_valid);
    assert!(!report.stable_runtime_refs.is_empty());
    assert!(!report.promotion.protected_main_eligible);
    let compiled = service.compile_source(
        service.root().join("Extension.axi"),
        "module Extension\nimport Base\n".into(),
    )?;
    assert_eq!(compiled.db.find_by_axi_type("Shared", "Person").len(), 0);
    assert!(compiled
        .meta
        .as_ref()
        .unwrap()
        .schemas
        .contains_key("Shared"));
    Ok(())
}

#[test]
fn authoring_workspace_package_collision_is_not_a_canonical_compile_failure() -> Result<()> {
    let duplicate_instance = format!("{BASE}instance Family of Shared:\n  Person = {{Other}}\n");
    let generator_base =
        "module Base\nschema Shared:\n  object Person\n  function Parent: Person -> Person\n";
    let generator_extension = "module Extension\nimport Base\nschema Separate:\n  object Person\n  function Parent: Person -> Person\n";
    for (base, extension, diagnostic) in [
        (duplicate_instance.as_str(), EXTENSION, "instance"),
        (
            BASE,
            generator_extension,
            "repeated execution arrow `Parent`",
        ),
        (
            generator_base,
            generator_extension,
            "repeated execution arrow `Parent`",
        ),
    ] {
        let (temp, service, request) = setup(base, extension)?;
        let full = service.execute(request.clone())?;
        assert!(!full.ok);
        assert!(full.validation.canonical_axi_valid);
        assert!(full.validation.compiled_kernel_ir_valid);
        assert!(full.source.is_some());
        assert!(full
            .diagnostics
            .iter()
            .any(|d| d.code == "authoring_query_projection_unsupported"
                && d.message.contains(diagnostic)));
        assert!(!full.promotion.candidate_reviewable);
        assert!(!full.promotion.protected_main_eligible);
        let AuthoringWorkspaceResponseV1::Compact(compact) = service.execute_response(request)?
        else {
            panic!("compact")
        };
        let compact = serde_json::to_value(compact)?;
        assert_eq!(compact["ok"], false);
        assert_eq!(compact["promotion"], serde_json::to_value(full.promotion)?);
        assert_eq!(compact["source"], serde_json::to_value(full.source)?);
        assert_eq!(compact["trust"], serde_json::to_value(full.trust)?);
        assert_eq!(
            std::fs::read(temp.path().join("Base.axi"))?,
            base.as_bytes()
        );
        assert_eq!(
            std::fs::read(temp.path().join("Extension.axi"))?,
            extension.as_bytes()
        );
    }
    Ok(())
}

#[test]
fn authoring_workspace_package_imported_theory_remains_review_only() -> Result<()> {
    let extension = EXTENSION.replace("instance Family", "theory Imported on Shared:\n  constraint transitive Parent on (child, parent)\ninstance Family");
    let (_temp, service, request) = setup(BASE, &extension)?;
    let full = service.execute(request)?;
    let runtime = full
        .validation
        .runtime_theory
        .as_ref()
        .expect("runtime theory metadata");
    assert_eq!(runtime.summary.review_only_obligations, 1);
    assert!(!runtime.summary.residual_obligation_ids.is_empty());
    assert_eq!(
        full.validation.runtime_theory_gate,
        AuthoringGateDecisionV1::Blocked
    );
    assert!(!full.promotion.protected_main_eligible);
    Ok(())
}

#[test]
fn authoring_workspace_package_invalid_missing_and_cyclic_imports_stay_errors() -> Result<()> {
    let (temp, service, request) = setup(BASE, EXTENSION)?;
    for invalid in [
        "module Base\nimport Extension\n",
        "module Base\nschema Shared:\n  relation Broken(from: Missing)\n",
    ] {
        std::fs::write(temp.path().join("Base.axi"), invalid)?;
        let report = service.execute(request.clone())?;
        assert!(!report.ok);
        assert!(!report.validation.canonical_axi_valid);
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.code == "authoring_canonical_compile_failed"));
        assert!(!report.promotion.protected_main_eligible);
    }
    std::fs::remove_file(temp.path().join("Base.axi"))?;
    let report = service.execute(request)?;
    assert!(!report.ok);
    assert!(report.source.is_none());
    Ok(())
}

#[cfg(unix)]
#[test]
fn authoring_workspace_package_import_symlink_escape_is_rejected() -> Result<()> {
    let (temp, service, request) = setup(BASE, EXTENSION)?;
    let outside = tempfile::tempdir()?;
    std::fs::write(outside.path().join("Base.axi"), BASE)?;
    std::fs::remove_file(temp.path().join("Base.axi"))?;
    std::os::unix::fs::symlink(
        outside.path().join("Base.axi"),
        temp.path().join("Base.axi"),
    )?;
    let report = service.execute(request)?;
    assert!(!report.ok);
    assert!(!report.validation.canonical_axi_valid);
    assert!(!report.promotion.protected_main_eligible);
    Ok(())
}
