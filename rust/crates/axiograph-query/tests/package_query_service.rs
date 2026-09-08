//! Process-free external embedding of the derived canonical-package query seam.
use anyhow::Result;
use axiograph_kernel::{
    CanonicalCompiler, CanonicalModuleSource, KernelCompilationRequest, RepositoryIdV2,
    SnapshotIdV2,
};
use axiograph_pathdb::axi_module_import::{derive_package_query_index, UnsupportedQueryProjection};
use axiograph_query::{
    axql::parse_axql_query,
    competency_questions::{
        evaluate_competency_questions_with_trust, generate_from_schema, CompetencyQuestionOptions,
    },
    query_ir::QueryIrV1,
};

#[test]
fn embedded_package_repeated_generator_execution_labels_reject() -> Result<()> {
    let relation = "  relation Parent(child: Person, parent: Person)\n";
    let generator = "  function Parent: Person -> Person\n";
    for (base_arrow, other_arrow) in [
        (relation, generator),
        (generator, relation),
        (generator, generator),
    ] {
        for with_data in [false, true] {
            let mut sources = Vec::new();
            for (module, schema, arrow) in [
                ("Base", "Shared", base_arrow),
                ("Other", "Separate", other_arrow),
            ] {
                let mut text =
                    format!("module {module}\nschema {schema}:\n  object Person\n{arrow}");
                if with_data {
                    let tuple = if arrow == relation {
                        "p: (child=Alice, parent=Alice)"
                    } else {
                        "(source=Alice, target=Alice)"
                    };
                    text.push_str(&format!("instance {module}Data of {schema}:\n  Person = {{Alice}}\n  Parent = {{{tuple}}}\n"));
                }
                sources.push(CanonicalModuleSource::parse(text.into_bytes())?);
            }
            sources.push(CanonicalModuleSource::parse(
                b"module Root\nimport Base\nimport Other\n".to_vec(),
            )?);
            let snapshot = CanonicalCompiler::compile(KernelCompilationRequest {
                repository_id: RepositoryIdV2::from_descriptor_bytes(b"embedded-package-collision"),
                accepted_snapshot_id: SnapshotIdV2::from_canonical_fields(&[b"test"]),
                root_module: "Root".into(),
                modules: sources.clone(),
            })?;
            // Canonically valid, but neither qualified nor unqualified named
            // execution can faithfully represent this mixed arrow namespace.
            for _ in 0..2 {
                let error = derive_package_query_index(&snapshot, &sources)
                    .err()
                    .expect("repeated explicit generator execution label must reject");
                assert!(error.is::<UnsupportedQueryProjection>(), "{error}");
                assert!(
                    error
                        .to_string()
                        .contains("repeated execution arrow `Parent`"),
                    "{error}"
                );
                sources.reverse();
            }
        }
    }
    Ok(())
}

#[test]
fn embedded_package_schema_only_import_named_queries_and_cqs() -> Result<()> {
    let base = CanonicalModuleSource::parse(b"module Base\nschema Shared:\n  object Person\n  relation Parent(child: Person, parent: Person)\n".to_vec())?;
    let other = CanonicalModuleSource::parse(b"module Other\nschema Separate:\n  object Person\n  relation Parent(child: Person, parent: Person)\ninstance Others of Separate:\n  Person = {Carol}\n  Parent = {p: (child=Carol, parent=Carol)}\n".to_vec())?;
    let extension = CanonicalModuleSource::parse(b"module Extension\nimport Base\nimport Other\ninstance Family of Shared:\n  Person = {Alice, Bob}\n  Parent = {p: (child=Alice, parent=Bob)}\n".to_vec())?;
    let sources = vec![extension, other, base];
    let snapshot = CanonicalCompiler::compile(KernelCompilationRequest {
        repository_id: RepositoryIdV2::from_descriptor_bytes(b"embedded-package"),
        accepted_snapshot_id: SnapshotIdV2::from_canonical_fields(&[b"test"]),
        root_module: "Extension".into(),
        modules: sources.clone(),
    })?;
    let derived = derive_package_query_index(&snapshot, &sources)?;
    for (text, count) in [
        ("select ?x where ?x is Shared.Person limit 10", 2),
        ("select ?x where ?x is Separate.Person limit 10", 1),
        ("select ?x where ?x -Shared.Parent-> ?y limit 10", 1),
        ("select ?x where ?x -Separate.Parent-> ?y limit 10", 1),
        (
            "select ?x where ?f = Shared.Parent(child=?x, parent=Bob) limit 10",
            1,
        ),
    ] {
        let query = QueryIrV1::from_axql_query(&parse_axql_query(text)?);
        let mut prepared = query.compile_with_meta(&derived.db, Some(&derived.meta))?;
        let result = prepared.execute_answer(&derived.db, Some(&derived.meta))?;
        assert_eq!(result.result().rows.len(), count, "{text}");
        let meta = prepared.metadata_with_meta(Some(&derived.meta))?;
        assert_eq!(meta.non_claims.completeness_claim, "not_claimed");
        assert!(!meta.trust.soundness.contains("lean_verified"));
    }
    let questions = generate_from_schema(&derived.db, &CompetencyQuestionOptions::default())?;
    assert!(!questions.is_empty());
    let report = evaluate_competency_questions_with_trust(&derived.db, &questions)?;
    assert_eq!(report.total, report.satisfied);
    assert!(report.total > 0);
    Ok(())
}
