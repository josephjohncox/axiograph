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
use std::marker::PhantomData;

use anyhow::{anyhow, Result};

use axiograph_dsl::schema_v1::{
    parse_path_expr_v3, ConstraintV1, GeneratorDeclV1, PathExprV3, RelationDeclV1, RewriteRuleV1,
    RewriteVarTypeV1, SchemaV1Instance, SchemaV1Module, SchemaV1Schema, SchemaV1Theory, SetItemV1,
};

use crate::certificate::AxiWellTypedProofV1;
use crate::kernel_ir::{
    derive_relation_semantics, derive_runtime_theory_index, role_kind_from_decl,
    RelationSemanticsIr, RoleIr, RuntimeSchemaIndex, RuntimeTheoryFragmentSummaryV1, TheoryIr,
};
use crate::lifecycle::{LifecycleState, Reviewed, Validated};

/// Marker trait for lifecycle states that carry a conservative Rust-side
/// well-typedness witness.
///
/// ```compile_fail
/// use axiograph_pathdb::{Accepted, Module, WellTypedModuleState};
///
/// fn require_well_typed<S: WellTypedModuleState>(_module: &Module<S>) {}
///
/// let _boundary: fn(&Module<Accepted>) = require_well_typed::<Accepted>;
/// ```
pub trait WellTypedModuleState: LifecycleState {}

impl WellTypedModuleState for Validated {}
impl WellTypedModuleState for Reviewed {}

/// Optional review metadata carried when a validated module is promoted to the
/// reviewed lifecycle state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReviewStamp {
    pub reviewer: Option<String>,
    pub note: Option<String>,
}

/// A canonical `.axi` module packaged together with a Rust-side well-typedness witness.
///
/// This is the Rust analogue of Lean's `TypedModule` wrapper:
///
/// - construction is *checked* (fail-closed),
/// - downstream code can accept a `Module<Validated>` or `Module<Reviewed>`
///   instead of a raw AST,
/// - and we avoid a large class of “forgot to typecheck this input” bugs.
///
/// Note: this is an internal Rust safety/convenience tool. The trusted gate is
/// still the Lean checker (`axi_well_typed_v1` certificates).
#[derive(Debug, Clone)]
pub struct Module<S> {
    module: SchemaV1Module,
    proof: AxiWellTypedProofV1,
    review: Option<ReviewStamp>,
    _state: PhantomData<S>,
}

impl<S: WellTypedModuleState> Module<S> {
    pub fn module(&self) -> &SchemaV1Module {
        &self.module
    }

    pub fn proof(&self) -> &AxiWellTypedProofV1 {
        &self.proof
    }

    pub fn into_parts(self) -> (SchemaV1Module, AxiWellTypedProofV1) {
        (self.module, self.proof)
    }

    fn schema_indexes(&self) -> Result<HashMap<String, SchemaIndex>> {
        build_schema_indexes(&self.module)
    }

    pub fn compiled_schema_ir(&self, schema_name: &str) -> Result<Option<RuntimeSchemaIndex>> {
        Ok(self
            .schema_indexes()?
            .get(schema_name)
            .map(|schema| schema.compiled_schema_ir()))
    }

    pub fn compiled_theories(&self) -> Result<Vec<TheoryIr>> {
        let schema_indexes = self.schema_indexes()?;
        self.module
            .theories
            .iter()
            .map(|theory| typecheck_theory(theory, &schema_indexes))
            .collect()
    }

    pub fn compiled_theories_for_schema(&self, schema_name: &str) -> Result<Vec<TheoryIr>> {
        let schema_indexes = self.schema_indexes()?;
        self.module
            .theories
            .iter()
            .filter(|theory| theory.schema == schema_name)
            .map(|theory| typecheck_theory(theory, &schema_indexes))
            .collect()
    }

    pub fn compiled_theory_ir(
        &self,
        schema_name: &str,
        theory_name: &str,
    ) -> Result<Option<TheoryIr>> {
        let schema_indexes = self.schema_indexes()?;
        let Some(theory) = self
            .module
            .theories
            .iter()
            .find(|theory| theory.schema == schema_name && theory.name == theory_name)
        else {
            return Ok(None);
        };
        Ok(Some(typecheck_theory(theory, &schema_indexes)?))
    }

    pub fn runtime_theory_fragment_summary(
        &self,
        schema_name: &str,
        theory_name: &str,
    ) -> Result<Option<RuntimeTheoryFragmentSummaryV1>> {
        Ok(self
            .compiled_theory_ir(schema_name, theory_name)?
            .map(|theory| theory.runtime_fragment_summary()))
    }
}

impl<S: WellTypedModuleState> AsRef<SchemaV1Module> for Module<S> {
    fn as_ref(&self) -> &SchemaV1Module {
        self.module()
    }
}

impl Module<Validated> {
    /// Validate and wrap a parsed canonical module.
    pub fn new(module: SchemaV1Module) -> Result<Self> {
        validate_axi_v1_module(module)
    }

    pub fn into_reviewed(self, stamp: ReviewStamp) -> Module<Reviewed> {
        review_axi_v1_module(self, stamp)
    }
}

impl Module<Reviewed> {
    pub fn review_stamp(&self) -> Option<&ReviewStamp> {
        self.review.as_ref()
    }
}

pub fn validate_axi_v1_module(module: SchemaV1Module) -> Result<Module<Validated>> {
    let proof = typecheck_axi_v1_module(&module)?;
    Ok(Module {
        module,
        proof,
        review: None,
        _state: PhantomData,
    })
}

pub fn review_axi_v1_module(module: Module<Validated>, stamp: ReviewStamp) -> Module<Reviewed> {
    Module {
        module: module.module,
        proof: module.proof,
        review: Some(stamp),
        _state: PhantomData,
    }
}

fn build_schema_indexes(module: &SchemaV1Module) -> Result<HashMap<String, SchemaIndex>> {
    let mut schemas: HashMap<String, SchemaIndex> = HashMap::new();
    for schema in &module.schemas {
        if schemas.contains_key(&schema.name) {
            return Err(anyhow!("duplicate schema `{}` in module", schema.name));
        }
        schemas.insert(schema.name.clone(), SchemaIndex::try_from_schema(schema)?);
    }
    Ok(schemas)
}

#[derive(Debug, Clone)]
struct SchemaIndex {
    name: String,
    object_types: HashSet<String>,
    relation_decls: HashMap<String, RelationDeclV1>,
    generator_decls: HashMap<String, GeneratorDeclV1>,
    supertypes_of: HashMap<String, HashSet<String>>,
    subtypes_of: HashMap<String, HashSet<String>>,
}

impl SchemaIndex {
    fn try_from_schema(schema: &SchemaV1Schema) -> Result<Self> {
        let mut object_types: HashSet<String> = HashSet::new();
        for object in &schema.objects {
            if !object_types.insert(object.clone()) {
                return Err(anyhow!(
                    "schema `{}` declares duplicate object type `{}`",
                    schema.name,
                    object
                ));
            }
        }

        let relation_names = schema
            .relations
            .iter()
            .map(|relation| relation.name.clone())
            .collect::<HashSet<_>>();
        let mut relation_decls: HashMap<String, RelationDeclV1> = HashMap::new();
        for relation in &schema.relations {
            let mut seen_fields = HashSet::new();
            for field in &relation.fields {
                if !seen_fields.insert(field.field.clone()) {
                    return Err(anyhow!(
                        "schema `{}` relation `{}` declares duplicate field `{}`",
                        schema.name,
                        relation.name,
                        field.field
                    ));
                }
                let target = field.ty.referenced_name();
                let known = match field.ty.relation_object_name() {
                    Some(relation) => relation_names.contains(relation),
                    None => object_types.contains(target),
                };
                if !known {
                    return Err(anyhow!(
                        "schema `{}` relation `{}` field `{}` references unknown value type `{}`",
                        schema.name,
                        relation.name,
                        field.field,
                        field.ty
                    ));
                }
            }
            if relation_decls
                .insert(relation.name.clone(), relation.clone())
                .is_some()
            {
                return Err(anyhow!(
                    "schema `{}` declares duplicate relation `{}`",
                    schema.name,
                    relation.name
                ));
            }
        }

        let mut generator_decls = HashMap::new();
        for generator in &schema.generators {
            if object_types.contains(&generator.name)
                || relation_decls.contains_key(&generator.name)
                || generator_decls
                    .insert(generator.name.clone(), generator.clone())
                    .is_some()
            {
                return Err(anyhow!(
                    "schema `{}` declares duplicate or colliding generator `{}`",
                    schema.name,
                    generator.name
                ));
            }
            for (endpoint, target) in [
                ("source", generator.source.as_str()),
                ("target", generator.target.as_str()),
            ] {
                if !object_types.contains(target) && !relation_decls.contains_key(target) {
                    return Err(anyhow!(
                        "schema `{}` generator `{}` references unknown {} `{}`",
                        schema.name,
                        generator.name,
                        endpoint,
                        target
                    ));
                }
            }
        }

        for subtype in &schema.subtypes {
            if !object_types.contains(&subtype.sub) {
                return Err(anyhow!(
                    "schema `{}` subtype declaration references unknown subtype `{}`",
                    schema.name,
                    subtype.sub
                ));
            }
            if !object_types.contains(&subtype.sup) {
                return Err(anyhow!(
                    "schema `{}` subtype declaration references unknown supertype `{}`",
                    schema.name,
                    subtype.sup
                ));
            }
        }

        let supertypes_of = compute_supertypes_closure(&object_types, &schema.subtypes);
        let subtypes_of = compute_subtypes_closure(&object_types, &schema.subtypes);
        for (ty, supers) in &supertypes_of {
            if supers.contains(ty) && supers.len() > 1 {
                for sup in supers {
                    if sup == ty {
                        continue;
                    }
                    if supertypes_of
                        .get(sup)
                        .is_some_and(|other_supers| other_supers.contains(ty))
                    {
                        return Err(anyhow!(
                            "schema `{}` has cyclic subtype declarations involving `{}` and `{}`",
                            schema.name,
                            ty,
                            sup
                        ));
                    }
                }
            }
        }

        Ok(Self {
            name: schema.name.clone(),
            object_types,
            relation_decls,
            generator_decls,
            supertypes_of,
            subtypes_of,
        })
    }

    fn is_value_type(&self, ty: &str) -> bool {
        self.object_types.contains(ty) || self.relation_decls.contains_key(ty)
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

    fn relation_fields(
        &self,
        relation_name: &str,
    ) -> Result<&[axiograph_dsl::schema_v1::FieldDeclV1]> {
        let Some(relation) = self.relation_decls.get(relation_name) else {
            return Err(anyhow!(
                "unknown relation `{}` in schema `{}`",
                relation_name,
                self.name
            ));
        };
        Ok(relation.fields.as_slice())
    }

    fn field_decl(
        &self,
        relation_name: &str,
        field_name: &str,
    ) -> Result<&axiograph_dsl::schema_v1::FieldDeclV1> {
        let relation = self.relation_fields(relation_name)?;
        relation
            .iter()
            .find(|field| field.field == field_name)
            .ok_or_else(|| {
                anyhow!(
                    "relation `{}` in schema `{}` has no field `{}`",
                    relation_name,
                    self.name,
                    field_name
                )
            })
    }

    fn compiled_relation_semantics(&self, relation_name: &str) -> Option<RelationSemanticsIr> {
        let relation = self.relation_decls.get(relation_name)?;
        let roles = relation
            .fields
            .iter()
            .enumerate()
            .map(|(idx, field)| RoleIr {
                role_id: crate::RoleId::new(format!(
                    "role:{}:{}:{}",
                    self.name, relation_name, field.field
                )),
                name: field.field.clone(),
                target_type: field.ty.referenced_name().to_string(),
                order: idx as u16,
                kind: role_kind_from_decl(field.kind),
            })
            .collect::<Vec<_>>();
        Some(derive_relation_semantics(
            &self.name,
            relation_name,
            if self.object_types.contains(relation_name) {
                format!("{relation_name}Fact")
            } else {
                relation_name.to_string()
            },
            roles,
        ))
    }

    fn compiled_schema_ir(&self) -> RuntimeSchemaIndex {
        let relations = self
            .relation_decls
            .keys()
            .filter_map(|name| {
                self.compiled_relation_semantics(name)
                    .map(|relation| (name.clone(), relation))
            })
            .collect::<HashMap<_, _>>();
        let object_type_ids = self
            .object_types
            .iter()
            .map(|object_type| {
                (
                    object_type.clone(),
                    crate::ObjectTypeId::new(format!("object:{}:{}", self.name, object_type)),
                )
            })
            .collect::<HashMap<_, _>>();

        let mut compiled = RuntimeSchemaIndex {
            schema_id: crate::SchemaId::new(self.name.clone()),
            object_types: self.object_types.clone(),
            object_type_ids,
            supertypes_of: self.supertypes_of.clone(),
            subtypes_of: self.subtypes_of.clone(),
            relations,
            role_interfaces: HashMap::new(),
        };
        compiled.role_interfaces = compiled
            .relations
            .values()
            .flat_map(|relation| {
                relation.roles.iter().map(|role| {
                    let scoped_role_name = format!("{}:{}", relation.name, role.name);
                    (
                        scoped_role_name.clone(),
                        crate::kernel_ir::RoleInterfaceIr {
                            role_id: role.role_id.clone(),
                            scoped_role_name,
                            relation_name: relation.name.clone(),
                            role_name: role.name.clone(),
                            declared_target_type: role.target_type.clone(),
                            role_kind: role.kind,
                            admissible_player_types: compiled
                                .admissible_player_types(&role.target_type),
                        },
                    )
                })
            })
            .collect();
        compiled
    }
}

pub fn typecheck_axi_v1_module(module: &SchemaV1Module) -> Result<AxiWellTypedProofV1> {
    let schemas = build_schema_indexes(module)?;

    let mut seen_theories = HashSet::new();
    let mut seen_theory_ids = HashSet::new();
    for theory in &module.theories {
        if !seen_theories.insert((theory.schema.clone(), theory.name.clone())) {
            return Err(anyhow!(
                "duplicate theory `{}` on schema `{}` in module",
                theory.name,
                theory.schema
            ));
        }
        let theory_ir = typecheck_theory(theory, &schemas)?;
        if !seen_theory_ids.insert(theory_ir.theory_id.clone()) {
            return Err(anyhow!(
                "duplicate theory semantic id `{}` for theory `{}` on schema `{}`",
                theory_ir.theory_id,
                theory.name,
                theory.schema
            ));
        }
    }

    let mut seen_instances = HashSet::new();
    for inst in &module.instances {
        if !seen_instances.insert((inst.schema.clone(), inst.name.clone())) {
            return Err(anyhow!(
                "duplicate instance `{}` on schema `{}` in module",
                inst.name,
                inst.schema
            ));
        }
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

    let mut seen_assignments = HashSet::new();
    for assignment in &instance.assignments {
        if !seen_assignments.insert(assignment.name.clone()) {
            return Err(anyhow!(
                "instance `{}` repeats assignment `{}`",
                instance.name,
                assignment.name
            ));
        }
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

    let mut fact_labels = HashMap::<String, String>::new();
    for assignment in &instance.assignments {
        if !schema_index.relation_decls.contains_key(&assignment.name) {
            continue;
        }
        for item in &assignment.value.items {
            if let SetItemV1::Tuple {
                label: Some(label), ..
            } = item
            {
                if fact_labels
                    .insert(label.clone(), assignment.name.clone())
                    .is_some()
                {
                    return Err(anyhow!(
                        "instance `{}` repeats fact label `{}`",
                        instance.name,
                        label
                    ));
                }
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

        if let Some(generator) = schema_index.generator_decls.get(&assignment.name) {
            for item in &assignment.value.items {
                let SetItemV1::Tuple {
                    label: None,
                    fields,
                } = item
                else {
                    return Err(anyhow!(
                        "instance `{}` generator `{}` expects unlabeled tuples",
                        instance.name,
                        generator.name
                    ));
                };
                let values = fields
                    .iter()
                    .map(|(field, value)| (field.as_str(), value.as_str()))
                    .collect::<HashMap<_, _>>();
                if values.len() != 2
                    || !values.contains_key("source")
                    || !values.contains_key("target")
                {
                    return Err(anyhow!(
                        "instance `{}` generator `{}` expects exactly source/target fields",
                        instance.name,
                        generator.name
                    ));
                }
                for (endpoint, target_type) in [
                    ("source", generator.source.as_str()),
                    ("target", generator.target.as_str()),
                ] {
                    let value = values[endpoint];
                    if schema_index.relation_decls.contains_key(target_type) {
                        if fact_labels.get(value).map(String::as_str) != Some(target_type) {
                            return Err(anyhow!(
                                "instance `{}` generator `{}` {} references unknown fact `{}` of relation `{}`",
                                instance.name,
                                generator.name,
                                endpoint,
                                value,
                                target_type
                            ));
                        }
                    } else {
                        get_or_create_entity(
                            schema_index,
                            &mut entities_by_key,
                            target_type,
                            value,
                        )?;
                    }
                }
            }
            continue;
        }

        let Some(rel_decl) = schema_index.relation_decls.get(&assignment.name) else {
            return Err(anyhow!(
                "instance `{}` assignment `{}` contains tuples but `{}` is not a declared relation or generator in schema `{}`",
                instance.name,
                assignment.name,
                assignment.name,
                instance.schema
            ));
        };

        for it in &assignment.value.items {
            let SetItemV1::Tuple { fields, .. } = it else {
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

                let target = f.ty.referenced_name();
                if !schema_index.is_value_type(target) {
                    return Err(anyhow!(
                        "instance `{}` relation `{}`: field `{}` expects unknown value type `{}`",
                        instance.name,
                        assignment.name,
                        f.field,
                        f.ty
                    ));
                }

                if let Some(target_relation) = f.ty.relation_object_name() {
                    if fact_labels.get(value_name).map(String::as_str) != Some(target_relation) {
                        return Err(anyhow!(
                            "instance `{}` relation `{}` field `{}` references unknown fact label `{}` of relation `{}`",
                            instance.name,
                            assignment.name,
                            f.field,
                            value_name,
                            target_relation
                        ));
                    }
                } else {
                    get_or_create_entity(schema_index, &mut entities_by_key, target, value_name)?;
                }
            }
        }
    }

    Ok(())
}

fn typecheck_theory(
    theory: &SchemaV1Theory,
    schemas: &HashMap<String, SchemaIndex>,
) -> Result<TheoryIr> {
    let Some(schema_index) = schemas.get(&theory.schema) else {
        return Err(anyhow!(
            "theory `{}` references unknown schema `{}`",
            theory.name,
            theory.schema
        ));
    };

    let mut seen_equations = HashSet::new();
    for equation in &theory.equations {
        if !seen_equations.insert(equation.name.clone()) {
            return Err(anyhow!(
                "theory `{}` declares duplicate equation `{}`",
                theory.name,
                equation.name
            ));
        }
        typecheck_equation(theory, schema_index, equation)?;
    }

    let mut seen_rewrite_rules = HashSet::new();
    for rule in &theory.rewrite_rules {
        if !seen_rewrite_rules.insert(rule.name.clone()) {
            return Err(anyhow!(
                "theory `{}` declares duplicate rewrite rule `{}`",
                theory.name,
                rule.name
            ));
        }
        typecheck_rewrite_rule(theory, schema_index, rule)?;
    }

    for constraint in &theory.constraints {
        typecheck_constraint(theory, schema_index, constraint)?;
    }

    derive_runtime_theory_index(&schema_index.compiled_schema_ir(), theory)
        .map_err(|message| anyhow!(message))
}

fn typecheck_constraint(
    theory: &SchemaV1Theory,
    schema_index: &SchemaIndex,
    constraint: &ConstraintV1,
) -> Result<()> {
    let relation_fields =
        |relation_name: &str| -> Result<&[axiograph_dsl::schema_v1::FieldDeclV1]> {
            schema_index.relation_fields(relation_name)
        };

    let check_param_fields = |relation_name: &str, params: Option<&[String]>| -> Result<()> {
        if let Some(params) = params {
            for param in params {
                schema_index.field_decl(relation_name, param)?;
            }
        }
        Ok(())
    };

    match constraint {
        ConstraintV1::Functional {
            relation,
            src_field,
            dst_field,
        } => {
            relation_fields(relation)?;
            schema_index.field_decl(relation, src_field)?;
            schema_index.field_decl(relation, dst_field)?;
        }
        ConstraintV1::AtMost {
            relation,
            src_field,
            dst_field,
            params,
            ..
        } => {
            relation_fields(relation)?;
            schema_index.field_decl(relation, src_field)?;
            schema_index.field_decl(relation, dst_field)?;
            check_param_fields(relation, params.as_deref())?;
        }
        ConstraintV1::Typing { relation, rule } => {
            relation_fields(relation)?;
            if rule.trim().is_empty() {
                return Err(anyhow!(
                    "theory `{}` typing constraint on relation `{}` has an empty rule name",
                    theory.name,
                    relation
                ));
            }
        }
        ConstraintV1::SymmetricWhereIn {
            relation,
            field,
            values,
            carriers,
            params,
        } => {
            relation_fields(relation)?;
            schema_index.field_decl(relation, field)?;
            if values.is_empty() {
                return Err(anyhow!(
                    "theory `{}` symmetric-where-in constraint on relation `{}` must list at least one value",
                    theory.name,
                    relation
                ));
            }
            if let Some(carriers) = carriers {
                schema_index.field_decl(relation, &carriers.left_field)?;
                schema_index.field_decl(relation, &carriers.right_field)?;
            }
            check_param_fields(relation, params.as_deref())?;
        }
        ConstraintV1::Symmetric {
            relation,
            carriers,
            params,
        }
        | ConstraintV1::Transitive {
            relation,
            carriers,
            params,
        } => {
            relation_fields(relation)?;
            if let Some(carriers) = carriers {
                schema_index.field_decl(relation, &carriers.left_field)?;
                schema_index.field_decl(relation, &carriers.right_field)?;
            }
            check_param_fields(relation, params.as_deref())?;
        }
        ConstraintV1::Key { relation, fields } => {
            relation_fields(relation)?;
            if fields.is_empty() {
                return Err(anyhow!(
                    "theory `{}` key constraint on relation `{}` must name at least one field",
                    theory.name,
                    relation
                ));
            }
            for field in fields {
                schema_index.field_decl(relation, field)?;
            }
        }
        ConstraintV1::NamedBlock { name, body } => {
            if name.trim().is_empty() {
                return Err(anyhow!(
                    "theory `{}` has a named constraint block with an empty name",
                    theory.name
                ));
            }
            if body.iter().all(|line| line.trim().is_empty()) {
                return Err(anyhow!(
                    "theory `{}` named constraint block `{}` must not be empty",
                    theory.name,
                    name
                ));
            }
        }
        ConstraintV1::Unknown { text } => {
            if text.trim().is_empty() {
                return Err(anyhow!(
                    "theory `{}` contains an empty unknown constraint",
                    theory.name
                ));
            }
        }
    }

    Ok(())
}

#[derive(Debug, Default)]
struct EquationTypeEnv {
    object_vars: HashMap<String, String>,
}

fn typecheck_equation(
    theory: &SchemaV1Theory,
    schema_index: &SchemaIndex,
    equation: &axiograph_dsl::schema_v1::EquationV1,
) -> Result<()> {
    if equation.lhs.trim().is_empty() || equation.rhs.trim().is_empty() {
        return Err(anyhow!(
            "theory `{}` equation `{}` must have non-empty lhs and rhs",
            theory.name,
            equation.name
        ));
    }

    let lhs = match parse_path_expr_v3(&equation.lhs) {
        Ok(expr) => expr,
        Err(_) => return Ok(()),
    };
    let rhs = match parse_path_expr_v3(&equation.rhs) {
        Ok(expr) => expr,
        Err(_) => return Ok(()),
    };

    let mut env = EquationTypeEnv::default();
    let lhs_endpoints =
        infer_equation_expr_endpoints(schema_index, &mut env, &lhs).map_err(|message| {
            anyhow!(
                "theory `{}` equation `{}` lhs ill-typed: {}",
                theory.name,
                equation.name,
                message
            )
        })?;
    let rhs_endpoints =
        infer_equation_expr_endpoints(schema_index, &mut env, &rhs).map_err(|message| {
            anyhow!(
                "theory `{}` equation `{}` rhs ill-typed: {}",
                theory.name,
                equation.name,
                message
            )
        })?;

    if lhs_endpoints != rhs_endpoints {
        return Err(anyhow!(
            "theory `{}` equation `{}` has mismatched path endpoints lhs=Path({},{}) rhs=Path({},{})",
            theory.name,
            equation.name,
            lhs_endpoints.0,
            lhs_endpoints.1,
            rhs_endpoints.0,
            rhs_endpoints.1,
        ));
    }

    Ok(())
}

fn infer_equation_expr_endpoints(
    schema_index: &SchemaIndex,
    env: &mut EquationTypeEnv,
    expr: &PathExprV3,
) -> std::result::Result<(String, String), String> {
    match expr {
        PathExprV3::Var { name } => Err(format!(
            "free path variable `{name}` is not yet supported in runtime-checked path equations"
        )),
        PathExprV3::Reflexive { entity } => Ok((entity.clone(), entity.clone())),
        PathExprV3::Step { from, rel, to } => {
            let Some(relation) = schema_index.compiled_relation_semantics(rel) else {
                return Err(format!(
                    "unknown relation `{}` in schema `{}`",
                    rel, schema_index.name
                ));
            };
            let Some((src_role, dst_role)) = relation.carrier_roles() else {
                return Err(format!(
                    "relation `{}` in schema `{}` does not expose a compiled carrier pair",
                    rel, schema_index.name
                ));
            };
            unify_object_requirement(
                schema_index,
                &mut env.object_vars,
                from,
                &src_role.target_type,
            )?;
            unify_object_requirement(
                schema_index,
                &mut env.object_vars,
                to,
                &dst_role.target_type,
            )?;
            Ok((from.clone(), to.clone()))
        }
        PathExprV3::Trans { left, right } => {
            let (a, b) = infer_equation_expr_endpoints(schema_index, env, left)?;
            let (c, d) = infer_equation_expr_endpoints(schema_index, env, right)?;
            if b != c {
                return Err(format!(
                    "cannot compose paths because the left path ends at `{b}` and the right path starts at `{c}`"
                ));
            }
            Ok((a, d))
        }
        PathExprV3::Inv { path } => {
            let (a, b) = infer_equation_expr_endpoints(schema_index, env, path)?;
            Ok((b, a))
        }
    }
}

fn typecheck_rewrite_rule(
    theory: &SchemaV1Theory,
    schema_index: &SchemaIndex,
    rule: &RewriteRuleV1,
) -> Result<()> {
    #[derive(Debug, Default)]
    struct RewriteEnv {
        object_vars: HashMap<String, String>,
        path_vars: HashMap<String, (String, String)>,
    }

    fn infer_rewrite_expr_endpoints(
        schema_index: &SchemaIndex,
        env: &RewriteEnv,
        expr: &PathExprV3,
    ) -> std::result::Result<(String, String), String> {
        match expr {
            PathExprV3::Var { name } => env
                .path_vars
                .get(name)
                .cloned()
                .ok_or_else(|| format!("unbound path variable `{name}`")),
            PathExprV3::Reflexive { entity } => {
                if !env.object_vars.contains_key(entity) {
                    return Err(format!("unbound object variable `{entity}`"));
                }
                Ok((entity.clone(), entity.clone()))
            }
            PathExprV3::Step { from, rel, to } => {
                let Some(relation) = schema_index.compiled_relation_semantics(rel) else {
                    return Err(format!(
                        "unknown relation `{}` in schema `{}`",
                        rel, schema_index.name
                    ));
                };
                let Some((src_role, dst_role)) = relation.carrier_roles() else {
                    return Err(format!(
                        "relation `{}` in schema `{}` does not expose a compiled carrier pair",
                        rel, schema_index.name
                    ));
                };
                let from_ty = env
                    .object_vars
                    .get(from)
                    .map(String::as_str)
                    .ok_or_else(|| format!("unbound object variable `{from}`"))?;
                let to_ty = env
                    .object_vars
                    .get(to)
                    .map(String::as_str)
                    .ok_or_else(|| format!("unbound object variable `{to}`"))?;
                if !schema_index.is_subtype(from_ty, &src_role.target_type) {
                    return Err(format!(
                        "`{from}` has type `{from_ty}`, expected subtype of `{}`",
                        src_role.target_type
                    ));
                }
                if !schema_index.is_subtype(to_ty, &dst_role.target_type) {
                    return Err(format!(
                        "`{to}` has type `{to_ty}`, expected subtype of `{}`",
                        dst_role.target_type
                    ));
                }
                Ok((from.clone(), to.clone()))
            }
            PathExprV3::Trans { left, right } => {
                let (a, b) = infer_rewrite_expr_endpoints(schema_index, env, left)?;
                let (c, d) = infer_rewrite_expr_endpoints(schema_index, env, right)?;
                if b != c {
                    return Err(format!(
                        "cannot compose paths because the left path ends at `{b}` and the right path starts at `{c}`"
                    ));
                }
                Ok((a, d))
            }
            PathExprV3::Inv { path } => {
                let (a, b) = infer_rewrite_expr_endpoints(schema_index, env, path)?;
                Ok((b, a))
            }
        }
    }

    let mut env = RewriteEnv::default();
    let mut pending_path_vars = Vec::new();
    for variable in &rule.vars {
        if env.object_vars.contains_key(&variable.name)
            || env.path_vars.contains_key(&variable.name)
        {
            return Err(anyhow!(
                "theory `{}` rewrite rule `{}` declares duplicate variable `{}`",
                theory.name,
                rule.name,
                variable.name
            ));
        }
        match &variable.ty {
            RewriteVarTypeV1::Object { ty } => {
                if !schema_index.is_value_type(ty) {
                    return Err(anyhow!(
                        "theory `{}` rewrite rule `{}` references unknown value type `{}` for variable `{}`",
                        theory.name,
                        rule.name,
                        ty,
                        variable.name
                    ));
                }
                env.object_vars.insert(variable.name.clone(), ty.clone());
            }
            RewriteVarTypeV1::Path { from, to } => {
                pending_path_vars.push((variable.name.clone(), from.clone(), to.clone()));
            }
        }
    }

    for (path_var, from, to) in pending_path_vars {
        if !env.object_vars.contains_key(&from) {
            return Err(anyhow!(
                "theory `{}` rewrite rule `{}` path variable `{}` references unknown endpoint `{}`",
                theory.name,
                rule.name,
                path_var,
                from
            ));
        }
        if !env.object_vars.contains_key(&to) {
            return Err(anyhow!(
                "theory `{}` rewrite rule `{}` path variable `{}` references unknown endpoint `{}`",
                theory.name,
                rule.name,
                path_var,
                to
            ));
        }
        env.path_vars.insert(path_var, (from, to));
    }

    let lhs_endpoints =
        infer_rewrite_expr_endpoints(schema_index, &env, &rule.lhs).map_err(|message| {
            anyhow!(
                "theory `{}` rewrite rule `{}` lhs ill-typed: {}",
                theory.name,
                rule.name,
                message
            )
        })?;
    let rhs_endpoints =
        infer_rewrite_expr_endpoints(schema_index, &env, &rule.rhs).map_err(|message| {
            anyhow!(
                "theory `{}` rewrite rule `{}` rhs ill-typed: {}",
                theory.name,
                rule.name,
                message
            )
        })?;

    if lhs_endpoints != rhs_endpoints {
        return Err(anyhow!(
            "theory `{}` rewrite rule `{}` has mismatched endpoints lhs=Path({},{}) rhs=Path({},{})",
            theory.name,
            rule.name,
            lhs_endpoints.0,
            lhs_endpoints.1,
            rhs_endpoints.0,
            rhs_endpoints.1,
        ));
    }

    Ok(())
}

fn unify_object_requirement(
    schema_index: &SchemaIndex,
    object_vars: &mut HashMap<String, String>,
    variable: &str,
    expected_type: &str,
) -> std::result::Result<(), String> {
    if !schema_index.is_value_type(expected_type) {
        return Err(format!(
            "unknown value type `{}` in schema `{}`",
            expected_type, schema_index.name
        ));
    }

    match object_vars.get(variable).cloned() {
        None => {
            object_vars.insert(variable.to_string(), expected_type.to_string());
            Ok(())
        }
        Some(existing) if existing == expected_type => Ok(()),
        Some(existing) if schema_index.is_subtype(expected_type, &existing) => {
            object_vars.insert(variable.to_string(), expected_type.to_string());
            Ok(())
        }
        Some(existing) if schema_index.is_subtype(&existing, expected_type) => Ok(()),
        Some(existing) => Err(format!(
            "variable `{variable}` is required to have incompatible object types `{existing}` and `{expected_type}`"
        )),
    }
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
            "ambiguous element `{name}`: multiple entities exist across related types for `{desired_type}`: {candidates:?}"
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
