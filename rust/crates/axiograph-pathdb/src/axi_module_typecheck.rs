//! AST-level type checking for canonical `.axi` modules (`axi_v1`).
//!
//! The PathDB importer is intentionally permissive: it may create entities
//! implicitly when they appear in relation tuples.
//!
//! For *trusted* verification we also want a conservative, auditable check that
//! a `.axi` module is self-contained and well-formed with respect to its
//! declared schema:
//!
//! - instances reference declared schemas,
//! - object assignments reference declared object types,
//! - relation assignments reference declared relations,
//! - each relation tuple has exactly the declared fields, and
//! - every tuple field value refers to a compatible object type, and subtyping
//!   does not introduce ambiguous name resolution at supertypes.
//!
//! This module implements that small decision procedure and returns an
//! `AxiWellTypedProofV1` summary that can be re-checked in Lean.

use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Result};

use axiograph_dsl::schema_v1::{
    RelationDeclV1, SchemaV1Instance, SchemaV1Module, SchemaV1Schema, SetItemV1,
};

use crate::certificate::AxiWellTypedProofV1;

/// A canonical `.axi` module packaged together with a Rust-side well-typedness witness.
///
/// This is the Rust analogue of Lean's `TypedModule` wrapper:
///
/// - construction is *checked* (fail-closed),
/// - downstream code can accept a `TypedAxiV1Module` instead of a raw AST,
/// - and we avoid a large class of “forgot to typecheck this input” bugs.
///
/// Note: this is an internal Rust safety/convenience tool. The trusted gate is
/// still the Lean checker (`axi_well_typed_v1` certificates).
#[derive(Debug, Clone)]
pub struct TypedAxiV1Module {
    module: SchemaV1Module,
    proof: AxiWellTypedProofV1,
}

impl TypedAxiV1Module {
    /// Validate and wrap a parsed canonical module.
    pub fn new(module: SchemaV1Module) -> Result<Self> {
        let proof = typecheck_axi_v1_module(&module)?;
        Ok(Self { module, proof })
    }

    pub fn module(&self) -> &SchemaV1Module {
        &self.module
    }

    pub fn proof(&self) -> &AxiWellTypedProofV1 {
        &self.proof
    }

    pub fn into_parts(self) -> (SchemaV1Module, AxiWellTypedProofV1) {
        (self.module, self.proof)
    }
}

#[derive(Debug, Clone)]
struct SchemaIndex {
    object_types: HashSet<String>,
    relation_decls: HashMap<String, RelationDeclV1>,
    supertypes_of: HashMap<String, HashSet<String>>,
    subtypes_of: HashMap<String, HashSet<String>>,
}

impl SchemaIndex {
    fn from_schema(schema: &SchemaV1Schema) -> Self {
        let object_types: HashSet<String> = schema.objects.iter().cloned().collect();
        let relation_decls: HashMap<String, RelationDeclV1> = schema
            .relations
            .iter()
            .map(|r| (r.name.clone(), r.clone()))
            .collect();
        let supertypes_of = compute_supertypes_closure(&object_types, &schema.subtypes);
        let subtypes_of = compute_subtypes_closure(&object_types, &schema.subtypes);
        Self {
            object_types,
            relation_decls,
            supertypes_of,
            subtypes_of,
        }
    }

    fn is_subtype(&self, sub: &str, sup: &str) -> bool {
        self.supertypes_of
            .get(sub)
            .map(|s| s.contains(sup))
            .unwrap_or(sub == sup)
    }

    fn related_types_including_self(&self, ty: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        if let Some(supers) = self.supertypes_of.get(ty) {
            out.extend(supers.iter().cloned());
        } else {
            out.push(ty.to_string());
        }
        if let Some(subs) = self.subtypes_of.get(ty) {
            out.extend(subs.iter().cloned());
        } else {
            out.push(ty.to_string());
        }
        out.sort();
        out.dedup();
        out
    }
}

pub fn typecheck_axi_v1_module(module: &SchemaV1Module) -> Result<AxiWellTypedProofV1> {
    let mut schemas: HashMap<String, SchemaIndex> = HashMap::new();
    for schema in &module.schemas {
        if schemas.contains_key(&schema.name) {
            return Err(anyhow!("duplicate schema `{}` in module", schema.name));
        }
        schemas.insert(schema.name.clone(), SchemaIndex::from_schema(schema));
    }

    for inst in &module.instances {
        typecheck_instance(inst, &schemas)?;
    }

    let assignment_count: u32 = module
        .instances
        .iter()
        .map(|i| i.assignments.len() as u32)
        .sum();
    let tuple_count: u32 = module
        .instances
        .iter()
        .flat_map(|i| i.assignments.iter())
        .flat_map(|a| a.value.items.iter())
        .filter(|it| matches!(it, SetItemV1::Tuple { .. }))
        .count() as u32;

    Ok(AxiWellTypedProofV1 {
        module_name: module.module_name.clone(),
        schema_count: module.schemas.len() as u32,
        theory_count: module.theories.len() as u32,
        instance_count: module.instances.len() as u32,
        assignment_count,
        tuple_count,
    })
}

fn typecheck_instance(
    instance: &SchemaV1Instance,
    schemas: &HashMap<String, SchemaIndex>,
) -> Result<()> {
    let Some(schema_index) = schemas.get(&instance.schema) else {
        return Err(anyhow!(
            "instance `{}` references unknown schema `{}`",
            instance.name,
            instance.schema
        ));
    };

    for assignment in &instance.assignments {
        let all_idents = assignment
            .value
            .items
            .iter()
            .all(|it| matches!(it, SetItemV1::Ident { .. }));
        let all_tuples = assignment
            .value
            .items
            .iter()
            .all(|it| matches!(it, SetItemV1::Tuple { .. }));

        if !(all_idents || all_tuples) {
            return Err(anyhow!(
                "instance `{}` assignment `{}` mixes identifiers and tuples",
                instance.name,
                assignment.name
            ));
        }

        if all_idents {
            if !schema_index.object_types.contains(&assignment.name)
                && schema_index.relation_decls.contains_key(&assignment.name)
            {
                return Err(anyhow!(
                    "instance `{}` assignment `{}` contains identifiers but `{}` is declared as a relation",
                    instance.name,
                    assignment.name,
                    assignment.name
                ));
            }
            if !schema_index.object_types.contains(&assignment.name) {
                return Err(anyhow!(
                    "instance `{}` assignment `{}` contains identifiers but `{}` is not a declared object type",
                    instance.name,
                    assignment.name,
                    assignment.name
                ));
            }
        }
    }

    // Simulate importer semantics: relation tuples may introduce objects
    // implicitly, but subtyping-based name reuse must remain unambiguous.
    let mut entities_by_key: HashSet<(String, String)> = HashSet::new();

    for assignment in &instance.assignments {
        let all_idents = assignment
            .value
            .items
            .iter()
            .all(|it| matches!(it, SetItemV1::Ident { .. }));

        if all_idents {
            for it in &assignment.value.items {
                let SetItemV1::Ident { name } = it else {
                    continue;
                };
                get_or_create_entity(schema_index, &mut entities_by_key, &assignment.name, name)?;
            }
            continue;
        }

        let Some(rel_decl) = schema_index.relation_decls.get(&assignment.name) else {
            return Err(anyhow!(
                "instance `{}` assignment `{}` contains tuples but `{}` is not a declared relation in schema `{}`",
                instance.name,
                assignment.name,
                assignment.name,
                instance.schema
            ));
        };

        for it in &assignment.value.items {
            let SetItemV1::Tuple { fields } = it else {
                continue;
            };

            let mut field_values: HashMap<&str, &str> = HashMap::new();
            for (field_name, value_name) in fields {
                if field_values
                    .insert(field_name.as_str(), value_name.as_str())
                    .is_some()
                {
                    return Err(anyhow!(
                        "instance `{}` relation `{}`: duplicate field `{}` in tuple",
                        instance.name,
                        assignment.name,
                        field_name
                    ));
                }
                if !rel_decl.fields.iter().any(|f| f.field == *field_name) {
                    return Err(anyhow!(
                        "instance `{}` relation `{}`: unknown field `{}`",
                        instance.name,
                        assignment.name,
                        field_name
                    ));
                }
            }

            for f in &rel_decl.fields {
                let Some(value_name) = field_values.get(f.field.as_str()).copied() else {
                    return Err(anyhow!(
                        "instance `{}` relation `{}`: missing field `{}` in tuple",
                        instance.name,
                        assignment.name,
                        f.field
                    ));
                };

                if !schema_index.object_types.contains(&f.ty) {
                    return Err(anyhow!(
                        "instance `{}` relation `{}`: field `{}` expects unknown object type `{}`",
                        instance.name,
                        assignment.name,
                        f.field,
                        f.ty
                    ));
                }

                get_or_create_entity(schema_index, &mut entities_by_key, &f.ty, value_name)?;
            }
        }
    }

    Ok(())
}

fn get_or_create_entity(
    schema_index: &SchemaIndex,
    entities_by_key: &mut HashSet<(String, String)>,
    desired_type: &str,
    name: &str,
) -> Result<String> {
    let desired_type = desired_type.to_string();
    let name = name.to_string();

    let mut candidates: Vec<String> = Vec::new();
    for related in schema_index.related_types_including_self(&desired_type) {
        if entities_by_key.contains(&(related.clone(), name.clone())) {
            candidates.push(related);
        }
    }

    candidates.sort();
    candidates.dedup();

    if candidates.len() > 1 {
        return Err(anyhow!(
            "ambiguous element `{}`: multiple entities exist across related types for `{}`: {:?}",
            name,
            desired_type,
            candidates
        ));
    }

    if let Some(existing_type) = candidates.first().cloned() {
        if schema_index.is_subtype(&desired_type, &existing_type) && desired_type != existing_type {
            // Upgrade to the more-specific type.
            entities_by_key.remove(&(existing_type, name.clone()));
            entities_by_key.insert((desired_type.clone(), name.clone()));
            return Ok(desired_type);
        }
        // Keep the existing (already-specific) representative.
        return Ok(existing_type);
    }

    entities_by_key.insert((desired_type.clone(), name));
    Ok(desired_type)
}

fn compute_supertypes_closure(
    object_types: &HashSet<String>,
    subtype_decls: &[axiograph_dsl::schema_v1::SubtypeDeclV1],
) -> HashMap<String, HashSet<String>> {
    let mut direct_supers: HashMap<String, Vec<String>> = HashMap::new();
    for st in subtype_decls {
        direct_supers
            .entry(st.sub.clone())
            .or_default()
            .push(st.sup.clone());
    }

    let mut supertypes_of: HashMap<String, HashSet<String>> = HashMap::new();
    for ty in object_types {
        let mut supers = HashSet::new();
        supers.insert(ty.clone());
        let mut stack: Vec<String> = direct_supers.get(ty).cloned().unwrap_or_default();
        while let Some(sup) = stack.pop() {
            if supers.insert(sup.clone()) {
                if let Some(next) = direct_supers.get(&sup) {
                    stack.extend(next.iter().cloned());
                }
            }
        }
        supertypes_of.insert(ty.clone(), supers);
    }

    supertypes_of
}

fn compute_subtypes_closure(
    object_types: &HashSet<String>,
    subtype_decls: &[axiograph_dsl::schema_v1::SubtypeDeclV1],
) -> HashMap<String, HashSet<String>> {
    let mut direct_subs: HashMap<String, Vec<String>> = HashMap::new();
    for st in subtype_decls {
        direct_subs
            .entry(st.sup.clone())
            .or_default()
            .push(st.sub.clone());
    }

    let mut subtypes_of: HashMap<String, HashSet<String>> = HashMap::new();
    for ty in object_types {
        let mut subs = HashSet::new();
        subs.insert(ty.clone());
        let mut stack: Vec<String> = direct_subs.get(ty).cloned().unwrap_or_default();
        while let Some(sub) = stack.pop() {
            if subs.insert(sub.clone()) {
                if let Some(next) = direct_subs.get(&sub) {
                    stack.extend(next.iter().cloned());
                }
            }
        }
        subtypes_of.insert(ty.clone(), subs);
    }

    subtypes_of
}
