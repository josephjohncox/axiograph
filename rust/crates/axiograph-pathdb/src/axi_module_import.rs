//! Import canonical `.axi` modules into PathDB (for REPL/querying).
//!
//! This module derives an in-memory query index from a canonical
//! `axi_schema_v1` module. Durable PathDB state uses authenticated SQLite
//! `.axpd` materializations; PathDB never exports accepted `.axi` authority.
//!
//! ## Mapping (schema_v1 → PathDB)
//!
//! PathDB is a binary (directed) labeled graph. `axi_schema_v1` instances can
//! contain **n-ary relations**, so we import them using a *reification* pattern:
//!
//! - Each relation tuple becomes a dedicated **tuple entity** (aka "fact node").
//! - For each tuple field `f = v`, we add an edge: `tuple -f-> v`.
//! - We also add a **derived binary edge** for convenient traversal when a
//!   relation has clear endpoints (e.g. exactly 2 fields, or `from/to`).
//! - Explicit aspect/function interpretations with object-type endpoints become
//!   labeled execution edges. Relation-object generator endpoints remain in the
//!   canonical kernel model and reject at this derived PathDB boundary.
//!
//! This supports “higher-kind”/HoTT-ish encodings where proofs, equivalences,
//! and homotopies are themselves first-class objects referenced by relation
//! fields.

#![allow(unused_mut, dead_code)]

use std::collections::{HashMap, HashSet};

use ahash::AHashMap;
use anyhow::{anyhow, Result};

use axiograph_dsl::schema_v1::{
    parse_schema_v1, ConstraintV1, GeneratorDeclV1, RelationDeclV1, RewriteOrientationV1,
    RoleKindV1, SchemaV1Instance, SchemaV1Module, SchemaV1Schema, SetItemV1,
};
use axiograph_kernel::runtime_fact_id_v2;

use crate::axi_meta::*;
use crate::axi_module_typecheck::{validate_axi_v1_module, Module, WellTypedModuleState};
use crate::kernel_ir::{derive_runtime_schema_index, RuntimeSchemaIndex};
use crate::PathDB;

fn role_kind_wire(kind: RoleKindV1) -> &'static str {
    match kind {
        RoleKindV1::Data => "data",
        RoleKindV1::Context => "context",
        RoleKindV1::World => "world",
        RoleKindV1::Temporal => "temporal",
        RoleKindV1::Parameter => "parameter",
        RoleKindV1::Evidence => "evidence",
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AxiSchemaV1ImportSummary {
    pub meta_entities_added: usize,
    pub meta_relations_added: usize,
    pub instances_imported: usize,
    pub entities_added: usize,
    pub tuple_entities_added: usize,
    pub relations_added: usize,
    pub derived_edges_added: usize,
    pub entity_type_upgrades: usize,
}

pub fn import_axi_schema_v1_into_pathdb(
    db: &mut PathDB,
    text: &str,
) -> Result<AxiSchemaV1ImportSummary> {
    let module =
        parse_schema_v1(text).map_err(|e| anyhow!("failed to parse axi_schema_v1 module: {e}"))?;
    let module = validate_axi_v1_module(module)
        .map_err(|e| anyhow!("failed to validate axi_schema_v1 module: {e}"))?;
    import_axi_schema_v1_module_into_pathdb(db, &module)
}

/// Import a lifecycle-typed canonical `.axi` module into PathDB.
///
/// This boundary accepts only modules that already carry a Rust-side
/// well-typedness witness (`Module<Validated>` or `Module<Reviewed>`). Use
/// [`validate_axi_v1_module`] first, or [`import_axi_schema_v1_into_pathdb`] to
/// parse, validate, and import text in one step.
///
/// ```compile_fail
/// use axiograph_dsl::axi_v1::parse_axi_v1;
/// use axiograph_pathdb::axi_module_import::import_axi_schema_v1_module_into_pathdb;
/// use axiograph_pathdb::PathDB;
///
/// let raw = parse_axi_v1(
///     r#"
/// module Demo
///
/// schema S:
///   object Person
///
/// instance I of S:
///   Person = {Alice}
/// "#,
/// )
/// .unwrap();
///
/// let mut db = PathDB::new();
/// let _ = import_axi_schema_v1_module_into_pathdb(&mut db, &raw);
/// ```
pub fn import_axi_schema_v1_module_into_pathdb<S: WellTypedModuleState>(
    db: &mut PathDB,
    module: &Module<S>,
) -> Result<AxiSchemaV1ImportSummary> {
    import_axi_schema_v1_module_into_pathdb_impl(db, module.module())
}

fn import_axi_schema_v1_module_into_pathdb_impl(
    db: &mut PathDB,
    module: &SchemaV1Module,
) -> Result<AxiSchemaV1ImportSummary> {
    let mut summary = AxiSchemaV1ImportSummary::default();

    if module.instances.is_empty() {
        return Err(anyhow!(
            "axi_schema_v1 module `{}` has no instances to import",
            module.module_name
        ));
    }

    let mut meta = MetaImportContext::new(db, module)?;
    let handles = meta.import_meta_plane()?;
    summary.meta_entities_added += meta.summary.meta_entities_added;
    summary.meta_relations_added += meta.summary.meta_relations_added;

    // Derived traversal edges are a runtime convenience. To avoid collisions when
    // multiple schemas define the same relation or explicit generator name, choose
    // a stable label across the shared edge namespace:
    //
    // - emit an unqualified name only when it is unique across schemas and arrow kinds;
    // - otherwise emit schema-qualified `<Schema>.<Arrow>`.
    //
    // AxQL elaboration desugars qualified relation names accordingly.
    let meta_plane = crate::axi_semantics::MetaPlaneIndex::from_db(db)?;
    let mut relation_name_counts: HashMap<String, usize> = HashMap::new();
    for schema in meta_plane.schemas.values() {
        for rel in schema.relation_decls.keys() {
            *relation_name_counts.entry(rel.clone()).or_insert(0) += 1;
        }
    }
    for schema in &module.schemas {
        for generator in &schema.generators {
            *relation_name_counts
                .entry(generator.name.clone())
                .or_insert(0) += 1;
        }
    }

    for inst in &module.instances {
        let schema = module
            .schemas
            .iter()
            .find(|s| s.name == inst.schema)
            .ok_or_else(|| {
                anyhow!(
                    "instance `{}` references unknown schema `{}`",
                    inst.name,
                    inst.schema
                )
            })?;

        let schema_index = SchemaIndex::new(schema);
        let schema_handles = handles.schemas.get(&schema.name).cloned();
        let mut ctx = InstanceImportContext::new(
            db,
            module,
            inst,
            schema,
            schema_index,
            schema_handles,
            &relation_name_counts,
        );
        ctx.import_instance_data()?;
        summary.instances_imported += 1;
        summary.entities_added += ctx.summary.entities_added;
        summary.tuple_entities_added += ctx.summary.tuple_entities_added;
        summary.relations_added += ctx.summary.relations_added;
        summary.derived_edges_added += ctx.summary.derived_edges_added;
        summary.entity_type_upgrades += ctx.summary.entity_type_upgrades;
    }

    Ok(summary)
}

// ============================================================================
// Meta-plane import (schema/theory/constraints/equations)
// ============================================================================

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct MetaImportSummary {
    meta_entities_added: usize,
    meta_relations_added: usize,
}

#[derive(Debug, Clone)]
struct ModuleMetaHandles {
    module_entity: u32,
    schemas: HashMap<String, SchemaMetaHandles>,
}

#[derive(Debug, Clone)]
struct SchemaMetaHandles {
    schema_entity: u32,
    object_types: HashMap<String, u32>,
    relations: HashMap<String, u32>,
}

struct MetaImportContext<'a> {
    db: &'a mut PathDB,
    module: &'a SchemaV1Module,
    summary: MetaImportSummary,
}

impl<'a> MetaImportContext<'a> {
    fn new(db: &'a mut PathDB, module: &'a SchemaV1Module) -> Result<Self> {
        Ok(Self {
            db,
            module,
            summary: MetaImportSummary::default(),
        })
    }

    fn import_meta_plane(&mut self) -> Result<ModuleMetaHandles> {
        let module_name = self.module.module_name.as_str();
        let module_entity = self.get_or_create_meta_entity(
            META_TYPE_MODULE,
            &meta_id_module(module_name),
            vec![
                (META_ATTR_NAME.to_string(), module_name.to_string()),
                (META_ATTR_DIALECT.to_string(), "axi_schema_v1".to_string()),
                (ATTR_AXI_MODULE.to_string(), module_name.to_string()),
            ],
        )?;

        let mut schema_handles: HashMap<String, SchemaMetaHandles> = HashMap::new();

        for schema in &self.module.schemas {
            let schema_entity = self.get_or_create_meta_entity(
                META_TYPE_SCHEMA,
                &meta_id_schema(module_name, &schema.name),
                vec![
                    (META_ATTR_NAME.to_string(), schema.name.clone()),
                    (ATTR_AXI_MODULE.to_string(), module_name.to_string()),
                    (ATTR_AXI_SCHEMA.to_string(), schema.name.clone()),
                ],
            )?;
            self.add_meta_edge_if_missing(META_REL_HAS_SCHEMA, module_entity, schema_entity)?;

            let mut object_type_ids: HashMap<String, u32> = HashMap::new();
            for obj in &schema.objects {
                let obj_entity = self.get_or_create_meta_entity(
                    META_TYPE_OBJECT_TYPE,
                    &meta_id_object_type(module_name, &schema.name, obj),
                    vec![
                        (META_ATTR_NAME.to_string(), obj.clone()),
                        (ATTR_AXI_MODULE.to_string(), module_name.to_string()),
                        (ATTR_AXI_SCHEMA.to_string(), schema.name.clone()),
                    ],
                )?;
                self.add_meta_edge_if_missing(
                    META_REL_SCHEMA_HAS_OBJECT,
                    schema_entity,
                    obj_entity,
                )?;
                object_type_ids.insert(obj.clone(), obj_entity);
            }

            // Ensure subtype endpoints exist even if not declared in `object`.
            for st in &schema.subtypes {
                for obj in [&st.sub, &st.sup] {
                    if object_type_ids.contains_key(obj) {
                        continue;
                    }
                    let obj_entity = self.get_or_create_meta_entity(
                        META_TYPE_OBJECT_TYPE,
                        &meta_id_object_type(module_name, &schema.name, obj),
                        vec![
                            (META_ATTR_NAME.to_string(), obj.to_string()),
                            (ATTR_AXI_MODULE.to_string(), module_name.to_string()),
                            (ATTR_AXI_SCHEMA.to_string(), schema.name.clone()),
                        ],
                    )?;
                    self.add_meta_edge_if_missing(
                        META_REL_SCHEMA_HAS_OBJECT,
                        schema_entity,
                        obj_entity,
                    )?;
                    object_type_ids.insert(obj.to_string(), obj_entity);
                }

                let subtype_entity = self.get_or_create_meta_entity(
                    META_TYPE_SUBTYPE_DECL,
                    &meta_id_subtype_decl(module_name, &schema.name, &st.sub, &st.sup),
                    vec![
                        (META_ATTR_NAME.to_string(), format!("{}<{}", st.sub, st.sup)),
                        (ATTR_AXI_MODULE.to_string(), module_name.to_string()),
                        (ATTR_AXI_SCHEMA.to_string(), schema.name.clone()),
                        (ATTR_SUBTYPE_SUB.to_string(), st.sub.clone()),
                        (ATTR_SUBTYPE_SUP.to_string(), st.sup.clone()),
                        (
                            ATTR_SUBTYPE_INCLUSION.to_string(),
                            st.inclusion.clone().unwrap_or_default(),
                        ),
                    ],
                )?;
                self.add_meta_edge_if_missing(
                    META_REL_SCHEMA_HAS_SUBTYPE,
                    schema_entity,
                    subtype_entity,
                )?;

                let sub_id = *object_type_ids
                    .get(&st.sub)
                    .ok_or_else(|| anyhow!("missing meta object type for `{}`", st.sub))?;
                let sup_id = *object_type_ids
                    .get(&st.sup)
                    .ok_or_else(|| anyhow!("missing meta object type for `{}`", st.sup))?;
                self.add_meta_edge_if_missing(META_REL_SUBTYPE_OF, sub_id, sup_id)?;
            }

            let mut relation_ids: HashMap<String, u32> = HashMap::new();
            for rel in &schema.relations {
                let rel_entity = self.get_or_create_meta_entity(
                    META_TYPE_RELATION_DECL,
                    &meta_id_relation_decl(module_name, &schema.name, &rel.name),
                    vec![
                        (META_ATTR_NAME.to_string(), rel.name.clone()),
                        (ATTR_AXI_MODULE.to_string(), module_name.to_string()),
                        (ATTR_AXI_SCHEMA.to_string(), schema.name.clone()),
                    ],
                )?;
                self.add_meta_edge_if_missing(
                    META_REL_SCHEMA_HAS_RELATION,
                    schema_entity,
                    rel_entity,
                )?;

                for (field_index, field) in rel.fields.iter().enumerate() {
                    let field_entity = self.get_or_create_meta_entity(
                        META_TYPE_FIELD_DECL,
                        &meta_id_field_decl(module_name, &schema.name, &rel.name, &field.field),
                        vec![
                            (
                                META_ATTR_NAME.to_string(),
                                format!("{}.{}", rel.name, field.field),
                            ),
                            (ATTR_AXI_MODULE.to_string(), module_name.to_string()),
                            (ATTR_AXI_SCHEMA.to_string(), schema.name.clone()),
                            (ATTR_FIELD_NAME.to_string(), field.field.clone()),
                            (ATTR_FIELD_TYPE.to_string(), field.ty.to_string()),
                            (
                                ATTR_FIELD_KIND.to_string(),
                                role_kind_wire(field.kind).to_string(),
                            ),
                            (ATTR_FIELD_INDEX.to_string(), field_index.to_string()),
                        ],
                    )?;
                    self.add_meta_edge_if_missing(
                        META_REL_RELATION_HAS_FIELD,
                        rel_entity,
                        field_entity,
                    )?;

                    // Object and relation-object targets are declared explicitly.
                    // The importer never synthesizes shadow object types from roles.
                }

                relation_ids.insert(rel.name.clone(), rel_entity);
            }

            schema_handles.insert(
                schema.name.clone(),
                SchemaMetaHandles {
                    schema_entity,
                    object_types: object_type_ids,
                    relations: relation_ids,
                },
            );
        }

        // Theories (linked to schemas).
        for theory in &self.module.theories {
            let schema_entity = schema_handles
                .get(&theory.schema)
                .map(|h| h.schema_entity)
                .ok_or_else(|| {
                    anyhow!(
                        "theory `{}` references unknown schema `{}`",
                        theory.name,
                        theory.schema
                    )
                })?;

            let theory_entity = self.get_or_create_meta_entity(
                META_TYPE_THEORY,
                &meta_id_theory(module_name, &theory.name),
                vec![
                    (META_ATTR_NAME.to_string(), theory.name.clone()),
                    (ATTR_AXI_MODULE.to_string(), module_name.to_string()),
                    (ATTR_AXI_SCHEMA.to_string(), theory.schema.clone()),
                ],
            )?;
            self.add_meta_edge_if_missing(
                META_REL_SCHEMA_HAS_THEORY,
                schema_entity,
                theory_entity,
            )?;

            for (index, c) in theory.constraints.iter().enumerate() {
                let (kind, attrs) = constraint_attrs(c);
                let mut attrs = attrs;
                attrs.push((META_ATTR_NAME.to_string(), format!("constraint_{index}")));
                attrs.push((ATTR_AXI_MODULE.to_string(), module_name.to_string()));
                attrs.push((ATTR_AXI_SCHEMA.to_string(), theory.schema.clone()));
                attrs.push((ATTR_CONSTRAINT_KIND.to_string(), kind.to_string()));
                attrs.push((ATTR_CONSTRAINT_INDEX.to_string(), index.to_string()));

                let constraint_entity = self.get_or_create_meta_entity(
                    META_TYPE_CONSTRAINT,
                    &meta_id_constraint(module_name, &theory.name, index),
                    attrs,
                )?;
                self.add_meta_edge_if_missing(
                    META_REL_THEORY_HAS_CONSTRAINT,
                    theory_entity,
                    constraint_entity,
                )?;
            }

            for (index, e) in theory.equations.iter().enumerate() {
                let mut attrs = vec![
                    (META_ATTR_NAME.to_string(), e.name.clone()),
                    (ATTR_AXI_MODULE.to_string(), module_name.to_string()),
                    (ATTR_AXI_SCHEMA.to_string(), theory.schema.clone()),
                    (ATTR_EQUATION_LHS.to_string(), e.lhs.clone()),
                    (ATTR_EQUATION_RHS.to_string(), e.rhs.clone()),
                    (ATTR_EQUATION_INDEX.to_string(), index.to_string()),
                ];
                let eq_entity = self.get_or_create_meta_entity(
                    META_TYPE_EQUATION,
                    &meta_id_equation(module_name, &theory.name, &e.name),
                    attrs,
                )?;
                self.add_meta_edge_if_missing(
                    META_REL_THEORY_HAS_EQUATION,
                    theory_entity,
                    eq_entity,
                )?;
            }

            for (index, r) in theory.rewrite_rules.iter().enumerate() {
                let vars_text = r
                    .vars
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                let orientation = rewrite_orientation_str(&r.orientation).to_string();
                let attrs = vec![
                    (META_ATTR_NAME.to_string(), r.name.clone()),
                    (ATTR_AXI_MODULE.to_string(), module_name.to_string()),
                    (ATTR_AXI_SCHEMA.to_string(), theory.schema.clone()),
                    (ATTR_REWRITE_RULE_ORIENTATION.to_string(), orientation),
                    (ATTR_REWRITE_RULE_VARS.to_string(), vars_text),
                    (ATTR_REWRITE_RULE_LHS.to_string(), r.lhs.to_string()),
                    (ATTR_REWRITE_RULE_RHS.to_string(), r.rhs.to_string()),
                    (ATTR_REWRITE_RULE_INDEX.to_string(), index.to_string()),
                ];
                let rule_entity = self.get_or_create_meta_entity(
                    META_TYPE_REWRITE_RULE,
                    &meta_id_rewrite_rule(module_name, &theory.name, &r.name),
                    attrs,
                )?;
                self.add_meta_edge_if_missing(
                    META_REL_THEORY_HAS_REWRITE_RULE,
                    theory_entity,
                    rule_entity,
                )?;
            }
        }

        // Instances.
        for inst in &self.module.instances {
            let instance_entity = self.get_or_create_meta_entity(
                META_TYPE_INSTANCE,
                &meta_id_instance(module_name, &inst.name),
                vec![
                    (META_ATTR_NAME.to_string(), inst.name.clone()),
                    (ATTR_AXI_MODULE.to_string(), module_name.to_string()),
                    (ATTR_AXI_SCHEMA.to_string(), inst.schema.clone()),
                    (ATTR_INSTANCE_SCHEMA.to_string(), inst.schema.clone()),
                ],
            )?;
            self.add_meta_edge_if_missing(META_REL_HAS_INSTANCE, module_entity, instance_entity)?;
        }

        Ok(ModuleMetaHandles {
            module_entity,
            schemas: schema_handles,
        })
    }

    fn get_or_create_meta_entity(
        &mut self,
        meta_type: &str,
        meta_id: &str,
        attrs: Vec<(String, String)>,
    ) -> Result<u32> {
        if let Some(existing) =
            find_entity_by_type_and_attr(self.db, meta_type, META_ATTR_ID, meta_id)
        {
            return Ok(existing);
        }
        let mut attrs = attrs;
        attrs.push((META_ATTR_ID.to_string(), meta_id.to_string()));

        let attrs_ref: Vec<(&str, &str)> = attrs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let id = self.db.add_entity(meta_type, attrs_ref);
        self.summary.meta_entities_added += 1;
        Ok(id)
    }

    fn add_meta_edge_if_missing(&mut self, rel: &str, source: u32, target: u32) -> Result<()> {
        let rel_id = self.db.interner.intern(rel);
        if self.db.relations.has_edge(source, rel_id, target) {
            return Ok(());
        }
        self.db.add_relation(rel, source, target, 1.0, vec![]);
        self.summary.meta_relations_added += 1;
        Ok(())
    }
}

fn find_entity_by_type_and_attr(
    db: &PathDB,
    type_name: &str,
    key: &str,
    value: &str,
) -> Option<u32> {
    let type_id = db.interner.id_of(type_name)?;
    let key_id = db.interner.id_of(key)?;
    let value_id = db.interner.id_of(value)?;

    let mut candidates = db.entities.entities_with_attr_value(key_id, value_id);
    let type_bitmap = db.entities.by_type(type_id)?;
    candidates &= type_bitmap.clone();
    candidates.iter().next()
}

fn constraint_attrs(c: &ConstraintV1) -> (&'static str, Vec<(String, String)>) {
    match c {
        ConstraintV1::Functional {
            relation,
            src_field,
            dst_field,
        } => (
            "functional",
            vec![
                (ATTR_CONSTRAINT_RELATION.to_string(), relation.clone()),
                (ATTR_CONSTRAINT_SRC_FIELD.to_string(), src_field.clone()),
                (ATTR_CONSTRAINT_DST_FIELD.to_string(), dst_field.clone()),
            ],
        ),
        ConstraintV1::AtMost {
            relation,
            src_field,
            dst_field,
            max,
            params,
        } => ("at_most", {
            let mut attrs = vec![
                (ATTR_CONSTRAINT_RELATION.to_string(), relation.clone()),
                (ATTR_CONSTRAINT_SRC_FIELD.to_string(), src_field.clone()),
                (ATTR_CONSTRAINT_DST_FIELD.to_string(), dst_field.clone()),
                (ATTR_CONSTRAINT_MAX.to_string(), max.to_string()),
            ];
            if let Some(ps) = params.as_ref() {
                attrs.push((ATTR_CONSTRAINT_PARAM_FIELDS.to_string(), ps.join(",")));
            }
            attrs
        }),
        ConstraintV1::Typing { relation, rule } => (
            "typing",
            vec![
                (ATTR_CONSTRAINT_RELATION.to_string(), relation.clone()),
                (ATTR_CONSTRAINT_TEXT.to_string(), rule.clone()),
            ],
        ),
        ConstraintV1::SymmetricWhereIn {
            relation,
            field,
            values,
            carriers,
            params,
        } => ("symmetric_where_in", {
            let mut attrs = vec![
                (ATTR_CONSTRAINT_RELATION.to_string(), relation.clone()),
                (ATTR_CONSTRAINT_WHERE_FIELD.to_string(), field.clone()),
                (
                    ATTR_CONSTRAINT_WHERE_IN_VALUES.to_string(),
                    values.join(","),
                ),
            ];
            if let Some(c) = carriers.as_ref() {
                // Reuse src/dst field attrs as the carrier pair for closure constraints.
                attrs.push((ATTR_CONSTRAINT_SRC_FIELD.to_string(), c.left_field.clone()));
                attrs.push((ATTR_CONSTRAINT_DST_FIELD.to_string(), c.right_field.clone()));
            }
            if let Some(ps) = params.as_ref() {
                attrs.push((ATTR_CONSTRAINT_PARAM_FIELDS.to_string(), ps.join(",")));
            }
            attrs
        }),
        ConstraintV1::Symmetric {
            relation,
            carriers,
            params,
        } => ("symmetric", {
            let mut attrs = vec![(ATTR_CONSTRAINT_RELATION.to_string(), relation.clone())];
            if let Some(c) = carriers.as_ref() {
                attrs.push((ATTR_CONSTRAINT_SRC_FIELD.to_string(), c.left_field.clone()));
                attrs.push((ATTR_CONSTRAINT_DST_FIELD.to_string(), c.right_field.clone()));
            }
            if let Some(ps) = params.as_ref() {
                attrs.push((ATTR_CONSTRAINT_PARAM_FIELDS.to_string(), ps.join(",")));
            }
            attrs
        }),
        ConstraintV1::Transitive {
            relation,
            carriers,
            params,
        } => ("transitive", {
            let mut attrs = vec![(ATTR_CONSTRAINT_RELATION.to_string(), relation.clone())];
            if let Some(c) = carriers.as_ref() {
                attrs.push((ATTR_CONSTRAINT_SRC_FIELD.to_string(), c.left_field.clone()));
                attrs.push((ATTR_CONSTRAINT_DST_FIELD.to_string(), c.right_field.clone()));
            }
            if let Some(ps) = params.as_ref() {
                attrs.push((ATTR_CONSTRAINT_PARAM_FIELDS.to_string(), ps.join(",")));
            }
            attrs
        }),
        ConstraintV1::Key { relation, fields } => (
            "key",
            vec![
                (ATTR_CONSTRAINT_RELATION.to_string(), relation.clone()),
                (ATTR_CONSTRAINT_FIELDS.to_string(), fields.join(",")),
            ],
        ),
        ConstraintV1::NamedBlock { name, body } => (
            "named_block",
            vec![
                (ATTR_CONSTRAINT_NAME.to_string(), name.clone()),
                (ATTR_CONSTRAINT_TEXT.to_string(), body.join("\n")),
            ],
        ),
        ConstraintV1::Unknown { text } => {
            // Unsupported constraint syntax stays addressable as `Unknown`.
            // Accepted/certified claims require later typed constraint checking;
            // this import path does not turn opaque text into trusted theory.
            //
            // We still try to extract a *relation name* so:
            // - `MetaPlaneIndex.constraints_by_relation` can index them, and
            // - the REPL can display them under `constraints <schema>`.
            let mut attrs = vec![(ATTR_CONSTRAINT_TEXT.to_string(), text.clone())];
            if let Some(relation) = extract_relation_from_unknown_constraint(text) {
                attrs.push((ATTR_CONSTRAINT_RELATION.to_string(), relation));
            }
            ("unknown", attrs)
        }
    }
}

fn rewrite_orientation_str(o: &RewriteOrientationV1) -> &'static str {
    match o {
        RewriteOrientationV1::Forward => "forward",
        RewriteOrientationV1::Backward => "backward",
        RewriteOrientationV1::Bidirectional => "bidirectional",
    }
}

fn extract_relation_from_unknown_constraint(text: &str) -> Option<String> {
    // Common patterns (kept intentionally loose):
    //   - `typing ExteriorDerivative: ...`
    //   - `antisymmetric Wedge`
    //   - `reflexive Accessible`
    //   - `functional Rel(...)` (handled by the parser when it matches the canonical form)
    //
    // We treat the *second token* as the relation name, stripping punctuation like `:` or `(`.
    let mut it = text.split_whitespace();
    let _kind = it.next()?;
    let raw_rel = it.next()?;

    let rel = raw_rel
        .trim()
        .trim_end_matches(':')
        .trim_end_matches('(')
        .trim_end_matches(',')
        .trim_end_matches('.');

    if rel.is_empty() {
        return None;
    }

    // Defensive: only accept identifiers that look like relation labels in the
    // canonical surface (ASCII letters/digits/underscores, not starting with digit).
    let mut chars = rel.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }

    Some(rel.to_string())
}

// ============================================================================
// Schema indexing (objects, relations, subtype closure)
// ============================================================================

#[derive(Debug, Clone)]
struct SchemaIndex {
    objects: HashSet<String>,
    relations: AHashMap<String, RelationDeclV1>,
    compiled_ir: RuntimeSchemaIndex,
    supertypes_of: AHashMap<String, HashSet<String>>,
    subtypes_of: AHashMap<String, HashSet<String>>,
}

impl SchemaIndex {
    fn new(schema: &SchemaV1Schema) -> Self {
        let mut objects: HashSet<String> = schema.objects.iter().cloned().collect();

        // Defensive: include types referenced in subtype decls even if the
        // schema forgot to list them under `object`.
        for st in &schema.subtypes {
            objects.insert(st.sub.clone());
            objects.insert(st.sup.clone());
        }

        let relations: AHashMap<String, RelationDeclV1> = schema
            .relations
            .iter()
            .map(|r| (r.name.clone(), r.clone()))
            .collect();

        let mut direct_supers: AHashMap<String, Vec<String>> = AHashMap::new();
        let mut direct_subs: AHashMap<String, Vec<String>> = AHashMap::new();
        for st in &schema.subtypes {
            direct_supers
                .entry(st.sub.clone())
                .or_default()
                .push(st.sup.clone());
            direct_subs
                .entry(st.sup.clone())
                .or_default()
                .push(st.sub.clone());
        }

        let mut supertypes_of: AHashMap<String, HashSet<String>> = AHashMap::new();
        let mut subtypes_of: AHashMap<String, HashSet<String>> = AHashMap::new();

        for ty in objects.iter() {
            // Compute transitive supertypes.
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

            // Compute transitive subtypes.
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

        Self {
            objects,
            relations,
            compiled_ir: derive_runtime_schema_index(schema),
            supertypes_of,
            subtypes_of,
        }
    }

    fn is_object_type(&self, ty: &str) -> bool {
        self.objects.contains(ty)
    }

    fn relation_decl(&self, name: &str) -> Option<&RelationDeclV1> {
        self.relations.get(name)
    }

    fn relation_semantics(&self, name: &str) -> Option<&crate::kernel_ir::RelationSemanticsIr> {
        self.compiled_ir.relation(name)
    }

    fn tuple_entity_type_name(&self, relation_name: &str) -> String {
        self.relation_semantics(relation_name)
            .map(|rel| rel.tuple_type_name.clone())
            .unwrap_or_else(|| {
                if self.is_object_type(relation_name) {
                    format!("{relation_name}Fact")
                } else {
                    relation_name.to_string()
                }
            })
    }

    fn canonical_entity_type_for_axi_type(&self, axi_type: &str) -> Result<String> {
        if self.is_object_type(axi_type) {
            return Ok(axi_type.to_string());
        }
        if self.relation_decl(axi_type).is_some() {
            // Some schemas treat relations as “morphism objects” without also
            // declaring an `object` of the same name. In that case, the tuple
            // entities *are* the object inhabitants.
            return Ok(self.tuple_entity_type_name(axi_type));
        }
        Err(anyhow!(
            "unknown type `{axi_type}` (not declared as object or relation)"
        ))
    }

    fn supertypes_including_self(&self, ty: &str) -> Vec<String> {
        self.supertypes_of
            .get(ty)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_else(|| vec![ty.to_string()])
    }

    fn subtypes_including_self(&self, ty: &str) -> Vec<String> {
        self.subtypes_of
            .get(ty)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_else(|| vec![ty.to_string()])
    }

    fn is_subtype(&self, sub: &str, sup: &str) -> bool {
        self.supertypes_of
            .get(sub)
            .map(|s| s.contains(sup))
            .unwrap_or(sub == sup)
    }
}

// ============================================================================
// Instance import
// ============================================================================

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct InstanceSummary {
    entities_added: usize,
    tuple_entities_added: usize,
    relations_added: usize,
    derived_edges_added: usize,
    entity_type_upgrades: usize,
}

struct InstanceImportContext<'a> {
    db: &'a mut PathDB,
    module: &'a SchemaV1Module,
    inst: &'a SchemaV1Instance,
    schema: &'a SchemaV1Schema,
    schema_index: SchemaIndex,
    schema_meta: Option<SchemaMetaHandles>,
    relation_name_counts: &'a HashMap<String, usize>,
    entities_by_key: HashMap<(String, String), u32>, // (type, name) → entity_id
    summary: InstanceSummary,
}

impl<'a> InstanceImportContext<'a> {
    fn new(
        db: &'a mut PathDB,
        module: &'a SchemaV1Module,
        inst: &'a SchemaV1Instance,
        schema: &'a SchemaV1Schema,
        schema_index: SchemaIndex,
        schema_meta: Option<SchemaMetaHandles>,
        relation_name_counts: &'a HashMap<String, usize>,
    ) -> Self {
        Self {
            db,
            module,
            inst,
            schema,
            schema_index,
            schema_meta,
            relation_name_counts,
            entities_by_key: HashMap::new(),
            summary: InstanceSummary::default(),
        }
    }

    fn import_instance_data(&mut self) -> Result<()> {
        for assignment in &self.inst.assignments {
            if assignment.value.items.is_empty() {
                continue;
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
                    "instance `{}` assignment `{}` mixes identifiers and tuples (unsupported)",
                    self.inst.name,
                    assignment.name
                ));
            }

            if all_idents {
                // Prefer treating ident-sets as object assignments.
                self.import_object_assignment(&assignment.name, &assignment.value.items)?;
            } else if let Some(generator) = self
                .schema
                .generators
                .iter()
                .find(|generator| generator.name == assignment.name)
                .cloned()
            {
                self.import_generator_assignment(&generator, &assignment.value.items)?;
            } else {
                self.import_relation_assignment(&assignment.name, &assignment.value.items)?;
            }
        }
        Ok(())
    }

    fn common_entity_attrs_owned(&self, name: &str) -> Vec<(String, String)> {
        vec![
            (META_ATTR_NAME.to_string(), name.to_string()),
            (ATTR_AXI_MODULE.to_string(), self.module.module_name.clone()),
            (ATTR_AXI_INSTANCE.to_string(), self.inst.name.clone()),
            (ATTR_AXI_SCHEMA.to_string(), self.schema.name.clone()),
        ]
    }

    fn get_or_create_object_entity(
        &mut self,
        object_type: &str,
        element_name: &str,
    ) -> Result<u32> {
        let object_type = self
            .schema_index
            .canonical_entity_type_for_axi_type(object_type)?;

        if let Some(&id) = self
            .entities_by_key
            .get(&(object_type.clone(), element_name.to_string()))
        {
            return Ok(id);
        }

        // Reuse an existing entity for the same name in a related subtype/supertype.
        let mut candidate_ids: Vec<u32> = Vec::new();
        for related in self.schema_index.subtypes_including_self(&object_type) {
            if let Some(&id) = self
                .entities_by_key
                .get(&(related, element_name.to_string()))
            {
                candidate_ids.push(id);
            }
        }
        for related in self.schema_index.supertypes_including_self(&object_type) {
            if let Some(&id) = self
                .entities_by_key
                .get(&(related, element_name.to_string()))
            {
                candidate_ids.push(id);
            }
        }

        candidate_ids.sort_unstable();
        candidate_ids.dedup();

        if candidate_ids.len() > 1 {
            return Err(anyhow!(
                "ambiguous element `{element_name}`: multiple entities exist across related types for `{object_type}`"
            ));
        }

        if let Some(id) = candidate_ids.first().copied() {
            // Alias the key to the existing entity.
            self.entities_by_key
                .insert((object_type.clone(), element_name.to_string()), id);

            // Prefer the more-specific type in the entity view (and for type indexes).
            self.maybe_upgrade_entity_type(id, &object_type)?;
            return Ok(id);
        }

        // Create a brand-new entity.
        let attrs_owned = self.common_entity_attrs_owned(element_name);
        let attrs: Vec<(&str, &str)> = attrs_owned
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let id = self.db.add_entity(&object_type, attrs);
        self.entities_by_key
            .insert((object_type.clone(), element_name.to_string()), id);
        self.summary.entities_added += 1;

        self.ensure_entity_in_supertypes(id, &object_type);
        Ok(id)
    }

    fn maybe_upgrade_entity_type(&mut self, entity_id: u32, preferred_type: &str) -> Result<()> {
        let Some(actual_type_id) = self.db.entities.get_type(entity_id) else {
            return Ok(());
        };
        let Some(actual_type) = self.db.interner.lookup(actual_type_id) else {
            return Ok(());
        };
        if actual_type == preferred_type {
            return Ok(());
        }

        // Only upgrade when it gets strictly more specific: Preferred <: Actual.
        if !self.schema_index.is_subtype(preferred_type, &actual_type) {
            return Ok(());
        }

        let preferred_type_id = self.db.interner.intern(preferred_type);
        self.db.entities.types[entity_id as usize] = preferred_type_id;
        self.db
            .entities
            .type_index
            .entry(preferred_type_id)
            .or_default()
            .insert(entity_id);

        self.ensure_entity_in_supertypes(entity_id, preferred_type);
        self.summary.entity_type_upgrades += 1;
        Ok(())
    }

    fn ensure_entity_in_supertypes(&mut self, entity_id: u32, ty: &str) {
        for sup in self.schema_index.supertypes_including_self(ty) {
            let sup_id = self.db.interner.intern(&sup);
            self.db
                .entities
                .type_index
                .entry(sup_id)
                .or_default()
                .insert(entity_id);
        }
    }

    fn import_object_assignment(&mut self, name: &str, items: &[SetItemV1]) -> Result<()> {
        if !self.schema_index.is_object_type(name)
            && self.schema_index.relation_decl(name).is_some()
        {
            return Err(anyhow!(
                "assignment `{name}` contains identifiers but `{name}` is declared as a relation; expected tuple items"
            ));
        }
        if !self.schema_index.is_object_type(name) {
            return Err(anyhow!(
                "assignment `{name}` contains identifiers but `{name}` is not a declared object type in schema `{}`",
                self.schema.name
            ));
        }

        for it in items {
            let SetItemV1::Ident { name: element_name } = it else {
                continue;
            };
            self.get_or_create_object_entity(name, element_name)?;
        }
        Ok(())
    }

    fn import_generator_assignment(
        &mut self,
        generator: &GeneratorDeclV1,
        items: &[SetItemV1],
    ) -> Result<()> {
        if !self.schema_index.is_object_type(&generator.source)
            || !self.schema_index.is_object_type(&generator.target)
        {
            return Err(anyhow!(
                "PathDB execution currently requires object-type endpoints for generator `{}`; canonical relation-object generator interpretations remain in KernelSnapshotIr",
                generator.name
            ));
        }
        let edge_label = if self
            .relation_name_counts
            .get(&generator.name)
            .copied()
            .unwrap_or(0)
            > 1
        {
            format!("{}.{}", self.schema.name, generator.name)
        } else {
            generator.name.clone()
        };

        for item in items {
            let SetItemV1::Tuple { label, fields } = item else {
                continue;
            };
            if label.is_some() {
                return Err(anyhow!(
                    "instance `{}` generator `{}` mappings may not have fact labels",
                    self.inst.name,
                    generator.name
                ));
            }
            let mut mapping = HashMap::new();
            for (field, value) in fields {
                if mapping.insert(field.as_str(), value.as_str()).is_some() {
                    return Err(anyhow!(
                        "instance `{}` generator `{}` repeats field `{field}`",
                        self.inst.name,
                        generator.name
                    ));
                }
            }
            if mapping.len() != 2
                || !mapping.contains_key("source")
                || !mapping.contains_key("target")
            {
                return Err(anyhow!(
                    "instance `{}` generator `{}` expects exactly source/target fields",
                    self.inst.name,
                    generator.name
                ));
            }
            let source = self.get_or_create_object_entity(&generator.source, mapping["source"])?;
            let target = self.get_or_create_object_entity(&generator.target, mapping["target"])?;
            let relation_id = self.db.interner.intern(&edge_label);
            let existed = self.db.relations.has_edge(source, relation_id, target);
            self.add_edge_if_missing_with_attrs(
                &edge_label,
                source,
                target,
                vec![
                    (ATTR_AXI_SCHEMA, self.schema.name.as_str()),
                    (ATTR_AXI_INSTANCE, self.inst.name.as_str()),
                ],
            )?;
            if !existed {
                self.summary.derived_edges_added += 1;
            }
        }
        Ok(())
    }

    fn import_relation_assignment(
        &mut self,
        relation_name: &str,
        items: &[SetItemV1],
    ) -> Result<()> {
        let Some(decl) = self.schema_index.relation_decl(relation_name).cloned() else {
            return Err(anyhow!(
                "instance `{}` assignment `{}` contains tuples but `{}` is not a declared relation in schema `{}`",
                self.inst.name,
                relation_name,
                relation_name,
                self.schema.name
            ));
        };
        let relation_semantics = self
            .schema_index
            .relation_semantics(relation_name)
            .cloned()
            .ok_or_else(|| anyhow!("missing compiled semantics for relation `{relation_name}`"))?;

        for it in items {
            let SetItemV1::Tuple { fields, .. } = it else {
                continue;
            };

            // Pass 1: validate fields and compute a stable fact id.
            let mut field_value_names: HashMap<String, String> = HashMap::new();
            for (field_name, value_name) in fields {
                if field_value_names
                    .insert(field_name.clone(), value_name.clone())
                    .is_some()
                {
                    return Err(anyhow!(
                        "duplicate field `{}` in `{}` tuple in instance `{}`",
                        field_name,
                        relation_name,
                        self.inst.name
                    ));
                }
                if !decl.fields.iter().any(|f| f.field == *field_name) {
                    return Err(anyhow!(
                        "unknown field `{}` for relation `{}` (schema `{}`)",
                        field_name,
                        relation_name,
                        self.schema.name
                    ));
                }
            }

            // Ensure all declared fields are present.
            for f in &decl.fields {
                if !field_value_names.contains_key(&f.field) {
                    return Err(anyhow!(
                        "missing field `{}` in `{}` tuple in instance `{}`",
                        f.field,
                        relation_name,
                        self.inst.name
                    ));
                }
            }

            // Canonicalize tuple fields in schema-declared order.
            let mut ordered_fields: Vec<(&str, &str)> = Vec::with_capacity(decl.fields.len());
            for f in &decl.fields {
                let v = field_value_names.get(&f.field).ok_or_else(|| {
                    anyhow!(
                        "missing field `{}` while canonicalizing `{}` tuple in instance `{}`",
                        f.field,
                        relation_name,
                        self.inst.name
                    )
                })?;
                ordered_fields.push((f.field.as_str(), v.as_str()));
            }
            let fact_id = runtime_fact_id_v2(
                self.module.module_name.as_str(),
                self.schema.name.as_str(),
                self.inst.name.as_str(),
                relation_name,
                &ordered_fields,
            );

            let tuple_entity_type = self.schema_index.tuple_entity_type_name(relation_name);
            let tuple_name = format!(
                "{relation_name}_fact_{}",
                fact_id
                    .strip_prefix(axiograph_kernel::FACT_ID_V2_PREFIX)
                    .unwrap_or(&fact_id)
            );

            if let Some(existing) = find_entity_by_type_and_attr(
                self.db,
                &tuple_entity_type,
                ATTR_AXI_FACT_ID,
                &fact_id,
            ) {
                // Duplicate tuple: set semantics treat this as redundant.
                // Keep the existing entity and skip re-adding edges.
                self.entities_by_key
                    .insert((tuple_entity_type.clone(), tuple_name), existing);
                continue;
            }

            let mut tuple_attrs_owned = self.common_entity_attrs_owned(&tuple_name);
            tuple_attrs_owned.push((ATTR_AXI_RELATION.to_string(), relation_name.to_string()));
            tuple_attrs_owned.push((ATTR_AXI_FACT_ID.to_string(), fact_id.clone()));
            let tuple_attrs: Vec<(&str, &str)> = tuple_attrs_owned
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            let tuple_entity_id = self.db.add_entity(&tuple_entity_type, tuple_attrs);
            self.summary.entities_added += 1;
            self.summary.tuple_entities_added += 1;
            self.ensure_entity_in_supertypes(tuple_entity_id, &tuple_entity_type);

            if let Some(schema_meta) = self.schema_meta.as_ref() {
                if let Some(&rel_decl) = schema_meta.relations.get(relation_name) {
                    self.add_edge_if_missing_with_attrs(
                        META_REL_FACT_OF,
                        tuple_entity_id,
                        rel_decl,
                        vec![(ATTR_AXI_FACT_ID, fact_id.as_str())],
                    )?;
                }
            }

            // Pass 2: create object entities and field edges.
            let mut values_by_field: HashMap<String, u32> = HashMap::new();
            for f in &decl.fields {
                let value_name = field_value_names
                    .get(&f.field)
                    .ok_or_else(|| {
                        anyhow!(
                            "missing field `{}` while importing `{}` tuple in instance `{}`",
                            f.field,
                            relation_name,
                            self.inst.name
                        )
                    })?
                    .as_str();
                let value_entity_id =
                    self.get_or_create_object_entity(f.ty.referenced_name(), value_name)?;
                values_by_field.insert(f.field.clone(), value_entity_id);

                // Field edge: tuple -field-> value
                self.add_edge_if_missing_with_attrs(
                    &f.field,
                    tuple_entity_id,
                    value_entity_id,
                    vec![(ATTR_AXI_FACT_ID, fact_id.as_str())],
                )?;
            }

            // Optional context/world scoping (recommended).
            //
            // In canonical `.axi`, `@context ContextType` expands into an ordinary tuple
            // field named `ctx`. We derive a uniform edge for PathDB so queries can
            // scope fact nodes efficiently without “hard-coding” context semantics into
            // the checker. Facts without a `ctx` field remain unscoped (unknown, not false).
            //
            // Lean correspondence:
            // - `docs/explanation/TOPOS_THEORY.md` explains the intended “world-indexed” semantics
            //   (presheaf/sheaf intuition) for contexts.
            // - `lean/Axiograph/Topos/Overview.lean` pins down the mathlib types we
            //   aim to target (schemas-as-categories, instances-as-functors, contexts-as-indexing).
            //
            // This derived edge is a *runtime index affordance*; certificate scoping
            // remains anchored to canonical `.axi` digests (see `docs/reference/CERTIFICATES.md`).
            if let Some(&ctx_id) = values_by_field.get("ctx") {
                self.add_edge_if_missing_with_attrs(
                    REL_AXI_FACT_IN_CONTEXT,
                    tuple_entity_id,
                    ctx_id,
                    vec![(ATTR_AXI_FACT_ID, fact_id.as_str())],
                )?;
            }

            // Treat certain “equivalence” relations as homotopy witnesses so
            // users can query them generically (not only by the domain-specific
            // relation name).
            let homotopy_sides =
                resolve_relation_pair(relation_semantics.homotopy_field_names(), &values_by_field);
            if let Some((lhs, rhs)) = homotopy_sides {
                self.mark_virtual_type(tuple_entity_id, "Homotopy");
                self.add_edge_if_missing_with_attrs(
                    "lhs",
                    tuple_entity_id,
                    lhs,
                    vec![(ATTR_AXI_FACT_ID, fact_id.as_str())],
                )?;
                self.add_edge_if_missing_with_attrs(
                    "rhs",
                    tuple_entity_id,
                    rhs,
                    vec![(ATTR_AXI_FACT_ID, fact_id.as_str())],
                )?;
            }

            // Treat many relation-tuples as “morphisms with attributes”.
            // This is a lightweight bridge toward the HoTT/groupoid view where
            // arrows are first-class and can be inspected in the REPL.
            if homotopy_sides.is_none() {
                if let Some((from, to)) = resolve_relation_pair(
                    relation_semantics.morphism_field_names(),
                    &values_by_field,
                ) {
                    self.mark_virtual_type(tuple_entity_id, "Morphism");
                    self.add_edge_if_missing_with_attrs(
                        "from",
                        tuple_entity_id,
                        from,
                        vec![(ATTR_AXI_FACT_ID, fact_id.as_str())],
                    )?;
                    self.add_edge_if_missing_with_attrs(
                        "to",
                        tuple_entity_id,
                        to,
                        vec![(ATTR_AXI_FACT_ID, fact_id.as_str())],
                    )?;
                }
            }

            // Derived binary edge (convenience traversal).
            if let Some((src, dst)) =
                resolve_relation_pair(relation_semantics.carrier_field_names(), &values_by_field)
            {
                let derived_label = if self
                    .relation_name_counts
                    .get(relation_name)
                    .copied()
                    .unwrap_or(0)
                    > 1
                {
                    format!("{}.{}", self.schema.name, relation_name)
                } else {
                    relation_name.to_string()
                };

                let rel_type_id = self.db.interner.intern(&derived_label);
                let existed = self.db.relations.has_edge(src, rel_type_id, dst);
                self.add_edge_if_missing_with_attrs(
                    &derived_label,
                    src,
                    dst,
                    vec![(ATTR_AXI_FACT_ID, fact_id.as_str())],
                )?;
                if !existed {
                    self.summary.derived_edges_added += 1;
                }
            }
        }

        Ok(())
    }

    fn mark_virtual_type(&mut self, entity_id: u32, type_name: &str) {
        let type_id = self.db.interner.intern(type_name);
        self.db
            .entities
            .type_index
            .entry(type_id)
            .or_default()
            .insert(entity_id);
    }

    fn add_edge_if_missing_with_attrs(
        &mut self,
        rel: &str,
        source: u32,
        target: u32,
        attrs: Vec<(&str, &str)>,
    ) -> Result<()> {
        let rel_id = self.db.interner.intern(rel);
        if let Some(existing) = self.db.relations.edge_relation_id(source, rel_id, target) {
            let Some(rel_mut) = self.db.relations.relations.get_mut(existing as usize) else {
                return Err(anyhow!("internal error: missing relation {existing}"));
            };
            for (k, v) in attrs {
                let k_id = self.db.interner.intern(k);
                let v_id = self.db.interner.intern(v);
                if !rel_mut
                    .attrs
                    .iter()
                    .any(|(kk, vv)| *kk == k_id && *vv == v_id)
                {
                    rel_mut.attrs.push((k_id, v_id));
                }
            }
            return Ok(());
        }

        self.db.add_relation(rel, source, target, 1.0, attrs);
        self.summary.relations_added += 1;
        Ok(())
    }
}

fn resolve_relation_pair(
    pair: Option<(&str, &str)>,
    values_by_field: &HashMap<String, u32>,
) -> Option<(u32, u32)> {
    let (left, right) = pair?;
    Some((*values_by_field.get(left)?, *values_by_field.get(right)?))
}
