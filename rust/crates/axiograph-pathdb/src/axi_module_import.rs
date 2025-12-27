//! Import canonical `.axi` modules into PathDB (for REPL/querying).
//!
//! PathDB snapshots (`PathDBExportV1`) already round-trip via `axi_export`.
//! This module handles the *other* common case: a canonical `axi_schema_v1`
//! module (schema/theory/instance) that users want to load into PathDB so they
//! can explore/query it interactively.
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
//!
//! This supports “higher-kind”/HoTT-ish encodings where proofs, equivalences,
//! and homotopies are themselves first-class objects referenced by relation
//! fields.

#![allow(unused_mut, dead_code)]

use std::collections::{HashMap, HashSet};

use ahash::AHashMap;
use anyhow::{anyhow, Result};

use axiograph_dsl::digest::axi_fact_id_v1;
use axiograph_dsl::schema_v1::{
    parse_schema_v1, ConstraintV1, RelationDeclV1, RewriteOrientationV1, SchemaV1Instance,
    SchemaV1Module, SchemaV1Schema, SetItemV1,
};

use crate::axi_meta::*;
use crate::PathDB;

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
    import_axi_schema_v1_module_into_pathdb(db, &module)
}

pub fn import_axi_schema_v1_module_into_pathdb(
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
        let mut ctx =
            InstanceImportContext::new(db, module, inst, schema, schema_index, schema_handles);
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
                            (ATTR_FIELD_TYPE.to_string(), field.ty.clone()),
                            (ATTR_FIELD_INDEX.to_string(), field_index.to_string()),
                        ],
                    )?;
                    self.add_meta_edge_if_missing(
                        META_REL_RELATION_HAS_FIELD,
                        rel_entity,
                        field_entity,
                    )?;

                    // Ensure the field type exists as an object decl when possible.
                    if !object_type_ids.contains_key(&field.ty) {
                        let ty_entity = self.get_or_create_meta_entity(
                            META_TYPE_OBJECT_TYPE,
                            &meta_id_object_type(module_name, &schema.name, &field.ty),
                            vec![
                                (META_ATTR_NAME.to_string(), field.ty.clone()),
                                (ATTR_AXI_MODULE.to_string(), module_name.to_string()),
                                (ATTR_AXI_SCHEMA.to_string(), schema.name.clone()),
                            ],
                        )?;
                        self.add_meta_edge_if_missing(
                            META_REL_SCHEMA_HAS_OBJECT,
                            schema_entity,
                            ty_entity,
                        )?;
                        object_type_ids.insert(field.ty.clone(), ty_entity);
                    }
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
        ConstraintV1::Symmetric { relation } => (
            "symmetric",
            vec![(ATTR_CONSTRAINT_RELATION.to_string(), relation.clone())],
        ),
        ConstraintV1::Transitive { relation } => (
            "transitive",
            vec![(ATTR_CONSTRAINT_RELATION.to_string(), relation.clone())],
        ),
        ConstraintV1::Key { relation, fields } => (
            "key",
            vec![
                (ATTR_CONSTRAINT_RELATION.to_string(), relation.clone()),
                (ATTR_CONSTRAINT_FIELDS.to_string(), fields.join(",")),
            ],
        ),
        ConstraintV1::Unknown { text } => {
            // Best-effort: many domains want richer constraint vocabularies than the
            // v1 parser understands (e.g. typing rules, graded-commutativity, etc).
            //
            // We keep the canonical parser permissive by storing these as `Unknown`,
            // but we still try to extract a *relation name* so:
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
    let mut it = text.trim().split_whitespace();
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
    let Some(first) = chars.next() else {
        return None;
    };
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

    fn tuple_entity_type_name(&self, relation_name: &str) -> String {
        // If the schema also declares an object with the same name, keep tuple
        // entities distinct so we don't conflate:
        // - `LawCategory` (the category object)
        // - `LawCategory(law, category)` (the relation tuples)
        if self.is_object_type(relation_name) {
            format!("{relation_name}Fact")
        } else {
            relation_name.to_string()
        }
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
    ) -> Self {
        Self {
            db,
            module,
            inst,
            schema,
            schema_index,
            schema_meta,
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
                "ambiguous element `{}`: multiple entities exist across related types for `{}`",
                element_name,
                object_type
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
            .or_insert_with(roaring::RoaringBitmap::new)
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
                .or_insert_with(roaring::RoaringBitmap::new)
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

        for it in items {
            let SetItemV1::Tuple { fields } = it else {
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
                let v = field_value_names
                    .get(&f.field)
                    .expect("field presence checked above");
                ordered_fields.push((f.field.as_str(), v.as_str()));
            }
            let fact_id = axi_fact_id_v1(
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
                    .strip_prefix(axiograph_dsl::digest::AXI_FACT_ID_V1_PREFIX)
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
                    self.add_edge_if_missing(META_REL_FACT_OF, tuple_entity_id, rel_decl)?;
                }
            }

            // Pass 2: create object entities and field edges.
            let mut values_by_field: HashMap<String, u32> = HashMap::new();
            for f in &decl.fields {
                let value_name = field_value_names
                    .get(&f.field)
                    .expect("field presence checked above")
                    .as_str();
                let value_entity_id = self.get_or_create_object_entity(&f.ty, value_name)?;
                values_by_field.insert(f.field.clone(), value_entity_id);

                // Field edge: tuple -field-> value
                self.db
                    .add_relation(&f.field, tuple_entity_id, value_entity_id, 1.0, vec![]);
                self.summary.relations_added += 1;
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
                self.add_edge_if_missing(REL_AXI_FACT_IN_CONTEXT, tuple_entity_id, ctx_id)?;
            }

            // Treat certain “equivalence” relations as homotopy witnesses so
            // users can query them generically (not only by the domain-specific
            // relation name).
            let homotopy_sides = derive_homotopy_sides(relation_name, &values_by_field);
            if let Some((lhs, rhs)) = homotopy_sides {
                self.mark_virtual_type(tuple_entity_id, "Homotopy");
                self.add_edge_if_missing("lhs", tuple_entity_id, lhs)?;
                self.add_edge_if_missing("rhs", tuple_entity_id, rhs)?;
            }

            // Treat many relation-tuples as “morphisms with attributes”.
            // This is a lightweight bridge toward the HoTT/groupoid view where
            // arrows are first-class and can be inspected in the REPL.
            if homotopy_sides.is_none() {
                if let Some((from, to)) = derive_morphism_endpoints(&decl, &values_by_field) {
                    self.mark_virtual_type(tuple_entity_id, "Morphism");
                    self.add_edge_if_missing("from", tuple_entity_id, from)?;
                    self.add_edge_if_missing("to", tuple_entity_id, to)?;
                }
            }

            // Derived binary edge (convenience traversal).
            if let Some((src, dst)) = derive_binary_endpoints(&decl, &values_by_field) {
                let rel_type_id = self.db.interner.intern(relation_name);
                if !self.db.relations.has_edge(src, rel_type_id, dst) {
                    self.db.add_relation(relation_name, src, dst, 1.0, vec![]);
                    self.summary.relations_added += 1;
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
            .or_insert_with(roaring::RoaringBitmap::new)
            .insert(entity_id);
    }

    fn add_edge_if_missing(&mut self, rel: &str, source: u32, target: u32) -> Result<()> {
        let rel_id = self.db.interner.intern(rel);
        if self.db.relations.has_edge(source, rel_id, target) {
            return Ok(());
        }
        self.db.add_relation(rel, source, target, 1.0, vec![]);
        self.summary.relations_added += 1;
        Ok(())
    }
}

fn derive_binary_endpoints(
    decl: &RelationDeclV1,
    values_by_field: &HashMap<String, u32>,
) -> Option<(u32, u32)> {
    // If it is already binary, use the declared field order.
    if decl.fields.len() == 2 {
        let a = values_by_field.get(&decl.fields[0].field)?;
        let b = values_by_field.get(&decl.fields[1].field)?;
        return Some((*a, *b));
    }

    // Prefer “equivalence sides” over endpoints when present. For many canonical
    // examples, `from/to` are metadata about the endpoints of the *paths*, but
    // the equivalence itself relates *path objects* (e.g. `route1/route2`).
    for (src, dst) in [
        ("lhs", "rhs"),
        ("route1", "route2"),
        ("path1", "path2"),
        ("rel1", "rel2"),
        ("i1", "i2"),
        ("s1", "s2"),
        ("left", "right"),
        ("child", "parent"),
        ("from", "to"),
        ("source", "target"),
        ("src", "dst"),
    ] {
        if let (Some(a), Some(b)) = (values_by_field.get(src), values_by_field.get(dst)) {
            return Some((*a, *b));
        }
    }

    None
}

fn derive_morphism_endpoints(
    decl: &RelationDeclV1,
    values_by_field: &HashMap<String, u32>,
) -> Option<(u32, u32)> {
    // If it is already binary, use the declared field order.
    if decl.fields.len() == 2 {
        let a = values_by_field.get(&decl.fields[0].field)?;
        let b = values_by_field.get(&decl.fields[1].field)?;
        return Some((*a, *b));
    }

    // For n-ary relations, prefer explicit endpoint field names.
    for (src, dst) in [
        ("from", "to"),
        ("source", "target"),
        ("src", "dst"),
        ("child", "parent"),
    ] {
        if let (Some(a), Some(b)) = (values_by_field.get(src), values_by_field.get(dst)) {
            return Some((*a, *b));
        }
    }

    None
}

fn derive_homotopy_sides(
    relation_name: &str,
    values_by_field: &HashMap<String, u32>,
) -> Option<(u32, u32)> {
    // Heuristic: relations whose names include “Equiv/Equivalence” are treated
    // as 2-cells/homotopies when we can find a reasonable “lhs/rhs”-like pair.
    if !(relation_name.contains("Equiv") || relation_name.contains("Equivalence")) {
        return None;
    }

    for (lhs, rhs) in [
        ("lhs", "rhs"),
        ("route1", "route2"),
        ("path1", "path2"),
        ("rel1", "rel2"),
        ("i1", "i2"),
        ("s1", "s2"),
        ("left", "right"),
    ] {
        if let (Some(a), Some(b)) = (values_by_field.get(lhs), values_by_field.get(rhs)) {
            return Some((*a, *b));
        }
    }
    None
}
