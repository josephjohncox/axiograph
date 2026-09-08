use super::*;
use axiograph_kernel::{CanonicalCompiler, KernelCompilationRequest, RepositoryIdV2, SnapshotIdV2};

const BASE: &str = "module Base\nschema Shared:\n  object Person\n  relation Parent(child: Person, parent: Person)\n";
const EXTENSION: &str = "module Extension\nimport Base\ninstance Family of Shared:\n  Person = {Alice, Bob}\n  Parent = {p: (child=Alice, parent=Bob)}\n";

fn compile(
    texts: &[&str],
    root: &str,
) -> Result<(CompiledKernelSnapshot, Vec<CanonicalModuleSource>)> {
    let sources = texts
        .iter()
        .map(|text| {
            CanonicalModuleSource::parse(text.as_bytes().to_vec()).map_err(anyhow::Error::from)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let snapshot = CanonicalCompiler::compile(KernelCompilationRequest {
        repository_id: RepositoryIdV2::from_descriptor_bytes(b"derived-package-test"),
        accepted_snapshot_id: SnapshotIdV2::from_canonical_fields(
            &texts.iter().map(|t| t.as_bytes()).collect::<Vec<_>>(),
        ),
        root_module: root.into(),
        modules: sources.clone(),
    })?;
    Ok((snapshot, sources))
}

#[test]
fn package_schema_only_and_metadata_only_preserve_canonical_citations() -> Result<()> {
    let (snapshot, sources) = compile(&[EXTENSION, BASE], "Extension")?;
    let derived = derive_package_query_index(&snapshot, &sources)?;
    assert_eq!(derived.summary.instances_imported, 1);
    assert_eq!(derived.db.find_by_axi_type("Shared", "Person").len(), 2);
    assert_eq!(derived.db.find_by_axi_type("Shared", "Parent").len(), 1);
    assert_eq!(
        derived.meta.schemas["Shared"].module_name.as_deref(),
        Some("Base")
    );
    assert_eq!(
        derived
            .kernel
            .canonical_snapshot()
            .unwrap()
            .ir()
            .ir_digest(),
        snapshot.ir().ir_digest()
    );
    let citations = derived
        .kernel
        .canonical_citations
        .iter()
        .map(|c| c.reference.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        citations,
        snapshot.ir().refs().iter().cloned().collect::<Vec<_>>()
    );
    assert_eq!(
        derived.fact_citations.values().cloned().collect::<Vec<_>>(),
        snapshot.ir().instances()[0]
            .facts
            .iter()
            .map(|f| f.fact_id.clone())
            .collect::<Vec<_>>()
    );
    let (snapshot, sources) = compile(&[BASE], "Base")?;
    let empty = derive_package_query_index(&snapshot, &sources)?;
    assert_eq!(empty.summary.instances_imported, 0);
    assert_eq!(empty.summary.entities_added, 0);
    assert!(!empty.meta.schemas["Shared"].object_types.is_empty());
    Ok(())
}

#[test]
fn package_imported_instances_keep_runtime_ids_and_original_provenance() -> Result<()> {
    let base = format!("{BASE}instance Original of Shared:\n  Person = {{Carol, Dan}}\n  Parent = {{q: (child=Carol, parent=Dan)}}\n");
    let (standalone, source) = compile(&[&base], "Base")?;
    let single = derive_package_query_index(&standalone, &source)?;
    let (snapshot, mut sources) = compile(&[EXTENSION, &base], "Extension")?;
    let derived = derive_package_query_index(&snapshot, &sources)?;
    let imported_id = runtime_fact_id_v2(
        "Base",
        "Shared",
        "Original",
        "Parent",
        &[("child", "Carol"), ("parent", "Dan")],
    );
    assert_eq!(
        single.kernel.instances[0].relation_facts[0]
            .fact_id
            .as_str(),
        imported_id
    );
    assert!(derived
        .kernel
        .instances
        .iter()
        .flat_map(|i| &i.relation_facts)
        .any(|f| f.fact_id.as_str() == imported_id));
    let node = find_entity_by_type_and_attr(&derived.db, "Parent", ATTR_AXI_FACT_ID, &imported_id)
        .unwrap();
    let module_key = derived.db.interner.id_of(ATTR_AXI_MODULE).unwrap();
    let base_value = derived.db.interner.id_of("Base").unwrap();
    assert!(derived
        .db
        .entities
        .entities_with_attr_value(module_key, base_value)
        .contains(node));
    let owner = snapshot.ir().instance("Base", "Original").unwrap();
    assert_eq!(derived.fact_citations[&imported_id], owner.facts[0].fact_id);
    sources.reverse();
    let permuted = derive_package_query_index(&snapshot, &sources)?;
    assert_eq!(derived.fact_citations, permuted.fact_citations);
    assert_eq!(
        serde_json::to_value(&derived.kernel)?,
        serde_json::to_value(&permuted.kernel)?
    );
    // The old standalone adapter and the package seam use the same fact builder.
    let mut legacy = PathDB::new();
    let summary = import_axi_schema_v1_into_pathdb(&mut legacy, &base)?;
    assert_eq!(summary, single.summary);
    assert_eq!(legacy.entities.types, single.db.entities.types);
    assert_eq!(
        legacy.entities.entity_attrs,
        single.db.entities.entity_attrs
    );
    assert_eq!(
        legacy.relations.relations.len(),
        single.db.relations.relations.len()
    );
    Ok(())
}

#[test]
fn package_relation_roles_link_actual_forward_fact_nodes_not_placeholders() -> Result<()> {
    let base = "module Base\nschema Shared:\n  object Person\n  relation Parent(child: Person, parent: Person)\n  relation Review(fact: relation(Parent), reviewer: Person)\n";
    let extension = "module Extension\nimport Base\ninstance Family of Shared:\n  Person = {Alice, Bob}\n  Review = {r: (fact=p, reviewer=Bob)}\n  Parent = {p: (child=Alice, parent=Bob)}\n";
    let (snapshot, sources) = compile(&[base, extension], "Extension")?;
    let derived = derive_package_query_index(&snapshot, &sources)?;
    assert_eq!(
        derived.db.find_by_axi_type("Shared", "Parent").len(),
        1,
        "no shadow named p"
    );
    let parent = derived
        .db
        .find_by_axi_type("Shared", "Parent")
        .iter()
        .next()
        .unwrap();
    let review = derived
        .db
        .find_by_axi_type("Shared", "Review")
        .iter()
        .next()
        .unwrap();
    assert!(derived.db.relations.has_edge(
        review,
        derived.db.interner.id_of("fact").unwrap(),
        parent
    ));
    let owner = snapshot.ir().instance("Extension", "Family").unwrap();
    let canonical_parent = &owner
        .facts
        .iter()
        .find(|f| f.local_label.as_deref() == Some("p"))
        .unwrap()
        .fact_id;
    let runtime_parent = runtime_fact_id_v2(
        "Extension",
        "Shared",
        "Family",
        "Parent",
        &[("child", "Alice"), ("parent", "Bob")],
    );
    assert_eq!(&derived.fact_citations[&runtime_parent], canonical_parent);
    Ok(())
}

#[test]
fn package_multiple_imports_preserve_scoped_types_and_imported_theory() -> Result<()> {
    let other = "module Other\nschema Separate:\n  object Person\ninstance People of Separate:\n  Person = {OtherPerson}\n";
    let extension = EXTENSION.replace("import Base", "import Base\nimport Other\ntheory Imported on Shared:\n  constraint transitive Parent on (child, parent)");
    let (snapshot, sources) = compile(&[BASE, other, &extension], "Extension")?;
    let derived = derive_package_query_index(&snapshot, &sources)?;
    assert_eq!(derived.db.find_by_axi_type("Shared", "Person").len(), 2);
    assert_eq!(derived.db.find_by_axi_type("Separate", "Person").len(), 1);
    assert_eq!(
        derived.meta.schemas["Shared"].constraints_by_relation["Parent"].len(),
        1
    );
    assert!(!derived.meta.schemas["Separate"]
        .constraints_by_relation
        .contains_key("Parent"));
    Ok(())
}

#[test]
fn package_collisions_reject_only_at_derived_boundary() -> Result<()> {
    let duplicate_schema = compile(
        &[
            BASE,
            "module Other\nschema Shared:\n  object Other\n",
            "module Root\nimport Base\nimport Other\n",
        ],
        "Root",
    )
    .unwrap_err();
    assert!(duplicate_schema
        .to_string()
        .contains("duplicate visible schema label"));
    let cases = [
        ("module Other\nschema Separate:\n  object Person\ninstance Family of Separate:\n  Person = {Elsewhere}\n", "module Root\nimport Base\nimport Other\ninstance Family of Shared:\n  Person = {Here}\n", "instance"),
        ("module Other\nschema Separate:\n  object Person\ntheory T on Separate:\n", "module Root\nimport Base\nimport Other\ntheory T on Shared:\n", "theory"),
    ];
    for (other, root, kind) in cases {
        let (snapshot, sources) = compile(&[BASE, other, root], "Root")?;
        let before = sources
            .iter()
            .map(|s| s.exact_text().to_string())
            .collect::<Vec<_>>();
        for _ in 0..2 {
            let error = derive_package_query_index(&snapshot, &sources)
                .err()
                .expect("must reject collision");
            assert!(error.is::<UnsupportedQueryProjection>());
            assert!(error
                .to_string()
                .contains(&format!("duplicate unqualified {kind}")));
        }
        assert_eq!(
            before,
            sources
                .iter()
                .map(|s| s.exact_text().to_string())
                .collect::<Vec<_>>()
        );
    }
    Ok(())
}

#[test]
fn package_exact_membership_and_revisions_fail_closed() -> Result<()> {
    let (snapshot, sources) = compile(&[EXTENSION, BASE], "Extension")?;
    for inputs in [
        vec![sources[0].clone()],
        vec![sources[0].clone(), sources[0].clone()],
        vec![sources[0].clone(), sources[1].clone(), sources[1].clone()],
        vec![
            sources[0].clone(),
            sources[1].clone(),
            CanonicalModuleSource::parse(b"module Extra\n".to_vec())?,
        ],
        vec![
            sources[0].clone(),
            CanonicalModuleSource::parse(b"module Extra\n".to_vec())?,
        ],
        vec![
            sources[0].clone(),
            CanonicalModuleSource::parse(format!("{BASE}\n-- changed bytes\n").into_bytes())?,
        ],
    ] {
        assert!(derive_package_query_index(&snapshot, &inputs).is_err());
    }
    assert!(compile(&[EXTENSION], "Extension").is_err());
    assert!(compile(&["module A\nimport B\n", "module B\nimport A\n"], "A").is_err());
    assert!(compile(
        &[BASE, &EXTENSION.replace("parent=Bob", "parent=Missing")],
        "Extension"
    )
    .is_err());
    Ok(())
}

#[test]
fn package_subtypes_and_explicit_object_generators_remain_queryable() -> Result<()> {
    let base = "module Base\nschema Shared:\n  object Person\n  object Employee\n  subtype Employee < Person\n  function manager: Employee -> Person\n";
    let extension = "module Extension\nimport Base\ninstance Staff of Shared:\n  Person = {Bob}\n  Employee = {Alice}\n  manager = {(source=Alice, target=Bob)}\n";
    let (snapshot, sources) = compile(&[base, extension], "Extension")?;
    let derived = derive_package_query_index(&snapshot, &sources)?;
    assert_eq!(derived.db.find_by_axi_type("Shared", "Person").len(), 2);
    let employee = derived
        .db
        .find_by_axi_type("Shared", "Employee")
        .iter()
        .next()
        .unwrap();
    let bob = find_entity_by_type_and_attr(&derived.db, "Person", "name", "Bob").unwrap();
    assert!(derived.db.relations.has_edge(
        employee,
        derived.db.interner.id_of("manager").unwrap(),
        bob
    ));
    Ok(())
}

#[test]
fn package_execution_namespace_and_generator_capabilities_fail_closed() -> Result<()> {
    assert!(compile(&["module Base\nschema Shared:\n  object Person\n  object Parent\n  relation Parent(child: Person, parent: Person)\n"], "Base").is_err());
    for text in [
        "module Base\nschema Shared:\n  object AxiMetaSchema\n",
        "module Base\nschema Shared:\n  object Morphism\n",
        "module Base\nschema Shared:\n  object Person\n  relation R(axi_fact_of: Person)\n",
        "module Base\nschema Shared:\n  object Person\n  relation R(R: Person)\n",
        "module Base\nschema Shared:\n  object Person\n  relation Parent(child: Person, parent: Person)\n  function owner: Parent -> Person\ninstance I of Shared:\n  Person = {}\n  Parent = {}\n  owner = {}\n",
    ] {
        let (snapshot, sources) = compile(&[text], "Base")?;
        let error = derive_package_query_index(&snapshot, &sources).err().expect("must reject unsupported projection");
        assert!(error.is::<UnsupportedQueryProjection>(), "{error}");
    }
    Ok(())
}

#[test]
fn package_fact_references_are_instance_scoped_even_with_repeated_local_labels() -> Result<()> {
    let base = "module Base\nschema Shared:\n  object Person\n  relation Parent(child: Person, parent: Person)\n  relation Review(fact: relation(Parent), reviewer: Person)\ninstance Original of Shared:\n  Person = {BasePerson}\n  Parent = {p: (child=BasePerson, parent=BasePerson)}\n  Review = {r: (fact=p, reviewer=BasePerson)}\n";
    let extension = "module Extension\nimport Base\ninstance Family of Shared:\n  Person = {p}\n  Review = {r: (fact=p, reviewer=p)}\n  Parent = {p: (child=p, parent=p)}\n";
    let (snapshot, sources) = compile(&[base, extension], "Extension")?;
    let derived = derive_package_query_index(&snapshot, &sources)?;
    assert_eq!(derived.db.find_by_axi_type("Shared", "Parent").len(), 2);
    for (module, instance, value) in [
        ("Base", "Original", "BasePerson"),
        ("Extension", "Family", "p"),
    ] {
        let parent = runtime_fact_id_v2(
            module,
            "Shared",
            instance,
            "Parent",
            &[("child", value), ("parent", value)],
        );
        let review = runtime_fact_id_v2(
            module,
            "Shared",
            instance,
            "Review",
            &[("fact", "p"), ("reviewer", value)],
        );
        let p =
            find_entity_by_type_and_attr(&derived.db, "Parent", ATTR_AXI_FACT_ID, &parent).unwrap();
        let r =
            find_entity_by_type_and_attr(&derived.db, "Review", ATTR_AXI_FACT_ID, &review).unwrap();
        assert!(derived
            .db
            .relations
            .has_edge(r, derived.db.interner.id_of("fact").unwrap(), p));
    }
    // A Base fact label alone does not license an Extension reference.
    let invalid = extension.replace("  Parent = {p: (child=p, parent=p)}\n", "");
    assert!(compile(&[base, &invalid], "Extension").is_err());
    Ok(())
}
