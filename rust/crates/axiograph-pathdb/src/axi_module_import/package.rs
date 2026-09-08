//! Exact-package, derived-only named query projection. No store or receipt authority.
use super::*;
use crate::axi_semantics::MetaPlaneIndex;
use crate::kernel_ir::{
    derive_runtime_package_index, ordered_runtime_package_sources, RuntimeModuleIndex,
};
use axiograph_kernel::{
    CanonicalModuleSource, CompiledKernelSnapshot, SchemaObjectRefIr, TypedValueIr,
};

/// A valid canonical package cannot be represented by the current named query adapter.
#[derive(Debug)]
pub struct UnsupportedQueryProjection(pub String);

impl std::fmt::Display for UnsupportedQueryProjection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unsupported derived query projection: {}", self.0)
    }
}
impl std::error::Error for UnsupportedQueryProjection {}

pub struct DerivedPackageQueryIndex {
    pub db: PathDB,
    pub kernel: RuntimeModuleIndex,
    pub meta: MetaPlaneIndex,
    pub summary: AxiSchemaV1ImportSummary,
    /// Canonical citations for the retained runtime fact-ID wire contract.
    pub fact_citations: std::collections::BTreeMap<String, axiograph_kernel::FactIdV2>,
}

fn unsupported(message: impl Into<String>) -> anyhow::Error {
    UnsupportedQueryProjection(message.into()).into()
}

/// Preflight the *execution* namespaces, without changing canonical acceptance.
/// Local schema names key runtime/type/meta indexes; theory and instance names
/// key derived IDs. Object/tuple types are schema-scoped; arrows share one edge
/// namespace. Only repeated relation labels support named qualified queries;
/// repeated labels involving explicit generators must reject.
fn check_capability(
    snapshot: &CompiledKernelSnapshot,
    sources: &[&CanonicalModuleSource],
) -> Result<()> {
    for (kind, labels) in [
        (
            "schema",
            snapshot
                .ir()
                .schemas()
                .iter()
                .map(|s| s.label.as_str())
                .collect::<Vec<_>>(),
        ),
        (
            "theory",
            snapshot
                .ir()
                .theories()
                .iter()
                .map(|t| t.label.as_str())
                .collect(),
        ),
        (
            "instance",
            snapshot
                .ir()
                .instances()
                .iter()
                .map(|i| i.label.as_str())
                .collect(),
        ),
    ] {
        let mut seen = HashSet::new();
        for label in labels {
            if !seen.insert(label) {
                return Err(unsupported(format!("duplicate unqualified {kind} label `{label}`; identity-aware query namespaces are not yet supported")));
            }
        }
    }
    let mut arrow_counts = std::collections::BTreeMap::new();
    let mut generator_names = HashSet::new();
    let mut role_names = HashSet::new();
    for source in sources {
        for schema in &source.parsed().schemas {
            let mut local_arrows = HashSet::new();
            for name in schema
                .relations
                .iter()
                .map(|r| &r.name)
                .chain(schema.generators.iter().map(|g| &g.name))
            {
                if !local_arrows.insert(name) {
                    return Err(unsupported(format!(
                        "schema `{}` has colliding relation/generator arrow `{name}`",
                        schema.name
                    )));
                }
                *arrow_counts.entry(name.as_str()).or_insert(0) += 1;
            }
            generator_names.extend(schema.generators.iter().map(|g| g.name.as_str()));
            role_names.extend(
                schema
                    .relations
                    .iter()
                    .flat_map(|r| r.fields.iter().map(|f| f.field.as_str())),
            );
            if schema.name.contains('.') {
                return Err(unsupported(format!(
                    "schema `{}` uses the reserved qualification namespace",
                    schema.name
                )));
            }
            let runtime = derive_runtime_schema_index(schema);
            let mut types: std::collections::BTreeSet<String> =
                schema.objects.iter().cloned().collect();
            for declaration in &schema.relations {
                let relation = &runtime.relations[&declaration.name];
                if !types.insert(relation.tuple_type_name.clone()) {
                    return Err(unsupported(format!(
                        "schema `{}` has colliding execution tuple type `{}`",
                        schema.name, relation.tuple_type_name
                    )));
                }
            }
            for name in types {
                if name.starts_with("AxiMeta")
                    || name.contains('.')
                    || matches!(name.as_str(), "Morphism" | "Homotopy")
                {
                    return Err(unsupported(format!(
                        "execution type `{name}` uses a reserved meta/qualification namespace"
                    )));
                }
            }
            // Meta edges and role projections share the raw edge namespace with
            // convenient relation/generator edges. Do not let those merge.
            for name in schema
                .relations
                .iter()
                .flat_map(|r| std::iter::once(&r.name).chain(r.fields.iter().map(|f| &f.field)))
                .chain(schema.generators.iter().map(|g| &g.name))
            {
                if name.starts_with("axi_") || name.contains('.') {
                    return Err(unsupported(format!(
                        "execution arrow `{name}` uses a reserved meta/qualification namespace"
                    )));
                }
            }
        }
    }
    for (name, count) in arrow_counts {
        // AxQL qualifies repeated relation declarations only, not explicit
        // generators. The importer's broader arrow counts would disagree.
        if count > 1 && generator_names.contains(name) {
            return Err(unsupported(format!(
                "repeated execution arrow `{name}` includes an explicit generator; qualified generator query resolution is not supported"
            )));
        }
        if count == 1
            && (role_names.contains(name) || matches!(name, "from" | "to" | "lhs" | "rhs"))
        {
            return Err(unsupported(format!(
                "execution arrow `{name}` collides with a role or virtual projection label"
            )));
        }
    }
    for instance in snapshot.ir().instances() {
        if instance.functions.iter().any(|function| {
            matches!(
                function.generator,
                axiograph_kernel::SchemaGeneratorRefIr::Explicit { .. }
            ) && (matches!(function.source, SchemaObjectRefIr::RelationObject { .. })
                || matches!(function.target, SchemaObjectRefIr::RelationObject { .. }))
        }) {
            return Err(unsupported(format!(
                "instance `{}` has a relation-object generator endpoint",
                instance.label
            )));
        }
    }
    Ok(())
}

// Bridge canonical typed fact identities to the existing runtime fact-ID wire
// contract. The compiler, not source tuple interpretation, selects fact targets.
fn canonical_fact_bindings(
    schema: &axiograph_kernel::SchemaPresentationIr,
    instance: &axiograph_kernel::InstanceModelIr,
) -> Result<HashMap<String, CanonicalFactBinding>> {
    let mut bindings = HashMap::new();
    let facts_by_id = instance
        .facts
        .iter()
        .map(|fact| (&fact.fact_id, fact))
        .collect::<HashMap<_, _>>();
    for fact in &instance.facts {
        let relation = schema
            .relations
            .iter()
            .find(|r| r.relation_id == fact.relation_id)
            .ok_or_else(|| anyhow!("canonical fact relation is outside its schema"))?;
        let mut fields = Vec::new();
        let mut roles = HashMap::new();
        for role in &relation.roles {
            let value = &fact
                .ordered_role_values
                .iter()
                .find(|v| v.role_id == role.role_id)
                .ok_or_else(|| anyhow!("canonical fact role is missing"))?
                .value;
            let name = match value {
                TypedValueIr::ObjectElement { value } => value.clone(),
                TypedValueIr::RelationFact { fact_id } => {
                    let target = facts_by_id
                        .get(fact_id)
                        .ok_or_else(|| anyhow!("canonical fact target is outside its instance"))?;
                    if role.type_expr.carrier()
                        != (SchemaObjectRefIr::RelationObject {
                            relation_id: target.relation_id.clone(),
                        })
                    {
                        return Err(anyhow!("canonical fact target has the wrong relation type"));
                    }
                    target.local_label.clone().ok_or_else(|| {
                        unsupported("unlabelled canonical fact target has no named query encoding")
                    })?
                }
            };
            fields.push((role.label.clone(), name));
            roles.insert(role.label.clone(), value.clone());
        }
        let fields = fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect::<Vec<_>>();
        let runtime_id = runtime_fact_id_v2(
            &instance.module_name,
            &schema.label,
            &instance.label,
            &relation.label,
            &fields,
        );
        if bindings
            .insert(
                runtime_id,
                CanonicalFactBinding {
                    fact_id: fact.fact_id.clone(),
                    roles,
                },
            )
            .is_some()
        {
            return Err(unsupported(format!(
                "instance `{}` has distinct canonical facts with the same execution fact identity",
                instance.label
            )));
        }
    }
    Ok(bindings)
}

/// Build an isolated named PathDB query index from an immutable canonical package.
/// Exact sources are checked against the complete canonical closure and reordered
/// to that closure. Schema/instance ownership is selected by canonical IDs, never
/// by the adapter root name. Source ASTs only supply the existing rendering/lowering
/// implementation after canonical compilation; they do not establish authority.
///
/// Metadata-only packages are supported. Unrepresentable execution namespaces or
/// fact references fail closed before constructing the DB. Failure never exposes a
/// partial DB. This function cannot hydrate, publish, or manufacture a store receipt.
pub fn derive_package_query_index(
    snapshot: &CompiledKernelSnapshot,
    sources: &[CanonicalModuleSource],
) -> Result<DerivedPackageQueryIndex> {
    let ordered = ordered_runtime_package_sources(snapshot, sources).map_err(anyhow::Error::msg)?;
    check_capability(snapshot, &ordered)?;
    let kernel = derive_runtime_package_index(snapshot, sources).map_err(anyhow::Error::msg)?;
    let mut bindings = HashMap::new();
    for instance in snapshot.ir().instances() {
        let schema = snapshot
            .ir()
            .schemas()
            .iter()
            .find(|s| s.schema_id == instance.schema_id)
            .ok_or_else(|| anyhow!("canonical instance schema is missing"))?;
        bindings.insert(
            instance.instance_id.clone(),
            canonical_fact_bindings(schema, instance)?,
        );
    }
    let fact_citations = bindings
        .values()
        .flat_map(|facts| {
            facts
                .iter()
                .map(|(runtime_id, binding)| (runtime_id.clone(), binding.fact_id.clone()))
        })
        .collect();
    let mut db = PathDB::new();
    let mut summary = AxiSchemaV1ImportSummary::default();
    let mut schemas_by_id = HashMap::new();
    let mut handles_by_id = HashMap::new();
    let mut module_handles = HashMap::new();
    // All schemas precede all declarations/data, including schema-only imports.
    for source in &ordered {
        let module = source.parsed();
        let mut meta = MetaImportContext::new(&mut db, module)?;
        let handles = meta.import_schema_meta_plane()?;
        summary.meta_entities_added += meta.summary.meta_entities_added;
        summary.meta_relations_added += meta.summary.meta_relations_added;
        for schema in snapshot
            .ir()
            .schemas()
            .iter()
            .filter(|s| s.module_name == module.module_name)
        {
            let declaration = module
                .schemas
                .iter()
                .find(|s| s.name == schema.label)
                .ok_or_else(|| anyhow!("canonical schema declaration is missing"))?;
            schemas_by_id.insert(schema.schema_id.clone(), declaration);
            handles_by_id.insert(
                schema.schema_id.clone(),
                handles.schemas[&schema.label].clone(),
            );
        }
        module_handles.insert(module.module_name.clone(), handles);
    }
    for source in &ordered {
        let module = source.parsed();
        let mut handles = module_handles[&module.module_name].clone();
        // Attach imported theories to the referenced canonical schema, retaining
        // the theory's own module identity in metadata IDs and provenance.
        for theory in snapshot
            .ir()
            .theories()
            .iter()
            .filter(|t| t.module_name == module.module_name)
        {
            let schema = schemas_by_id[&theory.schema_id];
            handles.schemas.insert(
                schema.name.clone(),
                handles_by_id[&theory.schema_id].clone(),
            );
        }
        let mut meta = MetaImportContext::new(&mut db, module)?;
        meta.import_declaration_meta_plane(&handles)?;
        summary.meta_entities_added += meta.summary.meta_entities_added;
        summary.meta_relations_added += meta.summary.meta_relations_added;
    }
    let meta = MetaPlaneIndex::from_db(&db)?;
    let mut relation_name_counts = HashMap::new();
    for schema in schemas_by_id.values() {
        for name in schema
            .relations
            .iter()
            .map(|r| &r.name)
            .chain(schema.generators.iter().map(|g| &g.name))
        {
            *relation_name_counts.entry(name.clone()).or_insert(0) += 1;
        }
    }
    for owner in snapshot.ir().instances() {
        let module = ordered
            .iter()
            .find(|source| source.parsed().module_name == owner.module_name)
            .ok_or_else(|| anyhow!("canonical instance module is missing"))?
            .parsed();
        let inst = module
            .instances
            .iter()
            .find(|i| i.name == owner.label)
            .ok_or_else(|| anyhow!("canonical instance declaration is missing"))?;
        let schema = schemas_by_id[&owner.schema_id];
        let mut ctx = InstanceImportContext::new(
            &mut db,
            module,
            inst,
            schema,
            SchemaIndex::new(schema),
            Some(handles_by_id[&owner.schema_id].clone()),
            &relation_name_counts,
        );
        ctx.canonical_facts = Some(&bindings[&owner.instance_id]);
        ctx.import_instance_data()?;
        summary.instances_imported += 1;
        summary.entities_added += ctx.summary.entities_added;
        summary.tuple_entities_added += ctx.summary.tuple_entities_added;
        summary.relations_added += ctx.summary.relations_added;
        summary.derived_edges_added += ctx.summary.derived_edges_added;
        summary.entity_type_upgrades += ctx.summary.entity_type_upgrades;
    }
    db.build_indexes();
    Ok(DerivedPackageQueryIndex {
        db,
        kernel,
        meta,
        summary,
        fact_citations,
    })
}

#[cfg(test)]
mod tests;
