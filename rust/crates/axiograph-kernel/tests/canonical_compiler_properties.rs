#![allow(clippy::result_large_err)]

use std::{fs, path::Path};

use axiograph_kernel::{
    validate_instance_model_ir, CanonicalCompiler, CanonicalModuleSource, KernelCompilationRequest,
    KernelCompileError, RepositoryIdV2, SchemaGeneratorKindIr, SnapshotIdV2,
};
use proptest::prelude::*;

fn compile(text: &str) -> Result<axiograph_kernel::CompiledKernelSnapshot, KernelCompileError> {
    let source = CanonicalModuleSource::parse(text.as_bytes().to_vec())?;
    CanonicalCompiler::compile(KernelCompilationRequest {
        repository_id: RepositoryIdV2::from_descriptor_bytes(b"compiler-property-tests"),
        accepted_snapshot_id: SnapshotIdV2::from_canonical_fields(&[b"accepted-test-snapshot"]),
        root_module: source.parsed().module_name.clone(),
        modules: vec![source],
    })
}

#[test]
fn role_carrier_error_preserves_first_structured_occurrence_and_source() {
    let text = "module M\nschema Earlier:\n  object Compny\n  relation Previous(value: Compny)\nschema Actual:\n  object Company\n  relation Employment(first: Company, next: indexed(refined(Compny; eq(Compny)); first), later: OtherTypo)\n";
    let error = compile(text).unwrap_err();
    let rendered = error.to_string();
    let contextual = anyhow::Error::new(error.clone()).context("compile exact module M.axi");
    let pretty = format!("{contextual:#}");
    assert_eq!(pretty.matches(&rendered).count(), 1);
    assert!(pretty.starts_with("compile exact module M.axi: "));
    assert!(matches!(
        contextual.downcast_ref::<KernelCompileError>(),
        Some(KernelCompileError::RoleCarrier { .. })
    ));
    let KernelCompileError::RoleCarrier {
        module,
        schema_index,
        relation_index,
        role_index,
        cause,
    } = error
    else {
        panic!("structured carrier error")
    };
    assert_eq!(
        (module.as_str(), schema_index, relation_index, role_index),
        ("M", 1, 0, 1)
    );
    assert!(
        matches!(cause.as_ref(), KernelCompileError::UnknownObjectTarget { target, .. } if target == "Compny")
    );
    assert_eq!(rendered, cause.to_string());
    assert!(matches!(
        compile("module M\nschema S:\n  object Company\n  subtype Compny <: Company\n"),
        Err(KernelCompileError::UnknownObjectTarget { .. })
    ));
    let error = compile("module M\nschema S:\n  object Company\n  relation Employment(company: relation(Employmnt))\n").unwrap_err();
    assert!(
        matches!(error, KernelCompileError::RoleCarrier { cause, .. } if matches!(*cause, KernelCompileError::UnknownRelationTarget { .. }))
    );
}

fn atom_strategy() -> impl Strategy<Value = String> {
    "[A-Za-z][A-Za-z0-9_]{0,12}".prop_filter("avoid fixture names", |value| {
        !matches!(value.as_str(), "A" | "B" | "C" | "Current")
    })
}

proptest! {
    #[test]
    fn object_role_targets_outside_the_finite_carrier_are_rejected(ghost in atom_strategy()) {
        let source = format!(
            "module M\nschema S:\n  object Node\n  relation Edge(from: Node, to: Node)\ninstance I of S:\n  Node = {{A}}\n  Edge = {{(from=A, to={ghost})}}\n"
        );
        let rejected = matches!(
            compile(&source),
            Err(KernelCompileError::OutOfCodomain { .. })
        );
        prop_assert!(rejected);
    }

    #[test]
    fn missing_role_projections_are_rejected(role_value in atom_strategy()) {
        let source = format!(
            "module M\nschema S:\n  object Node\n  relation Edge(from: Node, to: Node)\ninstance I of S:\n  Node = {{A, {role_value}}}\n  Edge = {{(from=A)}}\n"
        );
        let rejected = matches!(
            compile(&source),
            Err(KernelCompileError::MissingRoleValue { .. })
        );
        prop_assert!(rejected);
    }

    #[test]
    fn repeated_role_projections_are_rejected(target in atom_strategy()) {
        let source = format!(
            "module M\nschema S:\n  object Node\n  relation Edge(from: Node, to: Node)\ninstance I of S:\n  Node = {{A, {target}}}\n  Edge = {{(from=A, from={target}, to={target})}}\n"
        );
        let rejected = matches!(
            compile(&source),
            Err(KernelCompileError::DuplicateRoleValue { .. })
        );
        prop_assert!(rejected);
    }

    #[test]
    fn duplicate_fact_ids_are_never_silently_deduplicated(target in atom_strategy()) {
        let source = format!(
            "module M\nschema S:\n  object Node\n  relation Edge(from: Node, to: Node)\ninstance I of S:\n  Node = {{A, {target}}}\n  Edge = {{(from=A, to={target}), (from=A, to={target})}}\n"
        );
        let rejected = matches!(
            compile(&source),
            Err(KernelCompileError::DuplicateFactId { .. })
        );
        prop_assert!(rejected);
    }

    #[test]
    fn partial_total_functions_are_rejected(extra in atom_strategy()) {
        let source = format!(
            "module M\nschema S:\n  object Node\n  function next: Node -> Node\ninstance I of S:\n  Node = {{A, {extra}}}\n  next = {{(source=A, target=A)}}\n"
        );
        let rejected = matches!(
            compile(&source),
            Err(KernelCompileError::PartialGenerator { .. })
        );
        prop_assert!(rejected);
    }
}

#[test]
fn explicit_axis_parameter_and_evidence_roles_survive_canonical_lowering() {
    let source = r#"module Axes
schema S:
  object Entity
  object Context
  object World
  object Time
  object Parameter
  object Evidence
  relation Observation(
    subject: Entity @data,
    ctx: Context @context,
    world: World @world,
    time: Time @temporal,
    parameter: Parameter @parameter,
    evidence: Evidence @evidence
  )
instance I of S:
  Entity = {E}
  Context = {C}
  World = {W}
  Time = {T}
  Parameter = {P}
  Evidence = {Proof}
  Observation = {(subject=E, ctx=C, world=W, time=T, parameter=P, evidence=Proof)}
"#;
    let compiled = compile(source).expect("explicit role kinds compile");
    let roles = &compiled.ir().schemas()[0].relations[0].roles;
    assert_eq!(
        roles.iter().map(|role| role.kind).collect::<Vec<_>>(),
        vec![
            axiograph_kernel::RoleKindIr::Data,
            axiograph_kernel::RoleKindIr::Context,
            axiograph_kernel::RoleKindIr::World,
            axiograph_kernel::RoleKindIr::Temporal,
            axiograph_kernel::RoleKindIr::Parameter,
            axiograph_kernel::RoleKindIr::Evidence,
        ]
    );
    assert_eq!(
        roles
            .iter()
            .map(|role| role.declared_order)
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 4, 5]
    );
    let scopes = &compiled.ir().instances()[0].scope_witnesses;
    assert!(scopes
        .iter()
        .any(|witness| witness.axis == axiograph_kernel::ScopeAxisIr::Context));
    assert!(scopes
        .iter()
        .any(|witness| witness.axis == axiograph_kernel::ScopeAxisIr::World));
    assert!(scopes
        .iter()
        .any(|witness| witness.axis == axiograph_kernel::ScopeAxisIr::Temporal));
    let receipt = compiled
        .require_finite_theory_gate(axiograph_kernel::FiniteTheoryGateConsumerIr::Authoring)
        .expect("axis witnesses replay");
    assert!(receipt.passed);
    assert_eq!(receipt.context_witness_count, 1);
    assert_eq!(receipt.world_witness_count, 1);
    assert_eq!(receipt.temporal_witness_count, 1);
}

#[test]
fn revalidation_rejects_non_injective_subtype_interpretations() {
    let source = r#"module Subtypes
schema S:
  object Person
  object Engineer
  subtype Engineer < Person
instance I of S:
  Person = {}
  Engineer = {E1, E2}
"#;
    let compiled = compile(source).expect("valid canonical subtype model");
    let schema = compiled.ir().schemas()[0].clone();
    let mut model = compiled.ir().instances()[0].clone();
    let subtype_ref = schema
        .generators
        .iter()
        .find(|generator| generator.kind == SchemaGeneratorKindIr::SubtypeInclusion)
        .expect("subtype generator")
        .generator_ref
        .clone();
    let function = model
        .functions
        .iter_mut()
        .find(|function| function.generator == subtype_ref)
        .expect("subtype interpretation");
    assert_eq!(function.mappings.len(), 2);
    function.mappings[1].target = function.mappings[0].target.clone();

    assert!(matches!(
        validate_instance_model_ir(&schema, &[], &model),
        Err(KernelCompileError::NonInjectiveSubtype { .. })
    ));
}

#[derive(serde::Deserialize)]
struct Corpus {
    cases: Vec<CorpusCase>,
}

#[derive(serde::Deserialize)]
struct CorpusCase {
    path: String,
    #[serde(default)]
    schema: Option<String>,
    compile: bool,
}

#[test]
fn imported_schema_is_visible_to_dependent_instances() {
    let base = CanonicalModuleSource::parse(
        b"module Base\n\nschema Shared:\n  object Person\n  relation Parent(child: Person, parent: Person)\n"
            .to_vec(),
    )
    .expect("parse base");
    let extension = CanonicalModuleSource::parse(
        b"module Extension\nimport Base\n\ninstance Family of Shared:\n  Person = {Alice, Bob}\n  Parent = {(child=Alice, parent=Bob)}\n"
            .to_vec(),
    )
    .expect("parse extension");
    let snapshot = CanonicalCompiler::compile(KernelCompilationRequest {
        repository_id: RepositoryIdV2::from_descriptor_bytes(b"import-closure-test"),
        accepted_snapshot_id: SnapshotIdV2::from_canonical_fields(&[b"accepted-import-test"]),
        root_module: "Extension".to_string(),
        modules: vec![extension, base],
    })
    .expect("compile ordered import closure");

    let closure = snapshot.ir().ordered_module_closure();
    assert_eq!(closure.len(), 2);
    assert_eq!(closure[0].module_name, "Base");
    assert_eq!(closure[1].module_name, "Extension");
    let schema = snapshot.ir().schema("Base", "Shared").expect("base schema");
    let instance = snapshot
        .ir()
        .instance("Extension", "Family")
        .expect("dependent instance");
    assert_eq!(instance.schema_id, schema.schema_id);
}

#[test]
fn w02_adversarial_corpus_matches_canonical_compiler_expectations() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("project root");
    let manifest: Corpus = serde_json::from_slice(
        &fs::read(project.join("fixtures/canonical/w02/corpus.json")).expect("W02 corpus manifest"),
    )
    .expect("valid W02 corpus manifest");
    let mut failures = Vec::new();
    for case in manifest.cases {
        let bytes = fs::read(project.join(&case.path)).expect("W02 corpus fixture");
        let result = CanonicalModuleSource::parse(bytes).and_then(|source| {
            CanonicalCompiler::compile(KernelCompilationRequest {
                repository_id: RepositoryIdV2::from_descriptor_bytes(b"w02-corpus"),
                accepted_snapshot_id: SnapshotIdV2::from_canonical_fields(&[b"w02-corpus"]),
                root_module: source.parsed().module_name.clone(),
                modules: vec![source],
            })
        });
        if result.is_ok() != case.compile {
            failures.push(format!(
                "{} expected compile={}, got {:?}",
                case.path,
                case.compile,
                result.err()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn category_kernel_conformance_corpus_matches_canonical_compiler() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("project root");
    let manifest: Corpus = serde_json::from_slice(
        &fs::read(project.join("fixtures/canonical/category_kernel/corpus.json"))
            .expect("category-kernel corpus manifest"),
    )
    .expect("valid category-kernel corpus manifest");
    let mut failures = Vec::new();
    for case in manifest.cases {
        let bytes = fs::read(project.join(&case.path)).expect("category-kernel corpus fixture");
        let result = CanonicalModuleSource::parse(bytes).and_then(|source| {
            CanonicalCompiler::compile(KernelCompilationRequest {
                repository_id: RepositoryIdV2::from_descriptor_bytes(
                    b"category-kernel-conformance-corpus",
                ),
                accepted_snapshot_id: SnapshotIdV2::from_canonical_fields(&[
                    b"category-kernel-conformance-corpus",
                ]),
                root_module: source.parsed().module_name.clone(),
                modules: vec![source],
            })
        });
        match result {
            Ok(snapshot) if case.compile => {
                let schema_name = case
                    .schema
                    .as_deref()
                    .expect("accepted case names a schema");
                let schema = snapshot
                    .ir()
                    .schemas()
                    .iter()
                    .find(|schema| schema.label == schema_name)
                    .expect("accepted case schema is compiled");
                schema
                    .category_formation
                    .verify(schema)
                    .expect("accepted category formation replays");
                let presentation = schema
                    .category_kernel_presentation_v3()
                    .expect("accepted category presentation exports");
                assert_eq!(
                    presentation.identity_objects.len(),
                    presentation.object_names.len(),
                    "{} omitted category identities",
                    case.path
                );
                assert!(presentation.relations.iter().all(|relation| {
                    relation
                        .roles
                        .iter()
                        .enumerate()
                        .all(|(index, role)| role.declared_order == index as u32)
                }));
            }
            Ok(_) => failures.push(format!("{} unexpectedly compiled", case.path)),
            Err(error) if !case.compile => {
                let _ = error;
            }
            Err(error) => failures.push(format!("{} unexpectedly rejected: {error}", case.path)),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn canonical_ir_constructors_exist_only_in_the_canonical_compiler_module() {
    fn walk(path: &Path, files: &mut Vec<std::path::PathBuf>) {
        for entry in fs::read_dir(path).expect("read source directory") {
            let entry = entry.expect("directory entry");
            let path = entry.path();
            if path.is_dir() {
                walk(&path, files);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let canonical_module = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/package.rs")
        .canonicalize()
        .expect("canonical compiler module");
    let source_gate_test = workspace
        .join(file!())
        .canonicalize()
        .expect("source gate test path");
    let mut files = Vec::new();
    walk(&workspace.join("crates"), &mut files);
    let forbidden_constructors = [
        "KernelSnapshotIr {",
        "SchemaPresentationIr {",
        "InstanceModelIr {",
    ];
    let forbidden_obsolete_names = [
        "CompiledSchemaIr",
        "KernelModuleIr",
        "KernelSurfaceV1",
        "KernelRefV1",
        "compile_kernel_module_ir",
        "compile_runtime_schema_index",
        "bincode",
    ];
    let mut violations = Vec::new();
    for file in files {
        if file == canonical_module || file == source_gate_test {
            continue;
        }
        let text = fs::read_to_string(&file).expect("Rust source is UTF-8");
        for needle in &forbidden_constructors {
            if text
                .lines()
                .any(|line| line.contains(needle) && !line.trim_start().starts_with("impl "))
            {
                violations.push(format!(
                    "{} contains constructor `{needle}`",
                    file.display()
                ));
            }
        }
        for needle in &forbidden_obsolete_names {
            if text.contains(needle) {
                violations.push(format!("{} contains obsolete `{needle}`", file.display()));
            }
        }
        if !file
            .components()
            .any(|component| component.as_os_str() == "tests")
        {
            let production = text.split("#[cfg(test)]").next().unwrap_or(&text);
            if production.to_ascii_lowercase().contains("fnv") {
                violations.push(format!(
                    "{} contains a production FNV compatibility path",
                    file.display()
                ));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "canonical compiler source gate violations:\n{}",
        violations.join("\n")
    );
}
