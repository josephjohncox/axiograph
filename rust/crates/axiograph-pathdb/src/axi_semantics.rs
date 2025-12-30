//! Schema-directed semantics and type checking for canonical `.axi` data in PathDB.
//!
//! PathDB stores *everything* as a labeled directed graph. To support richer,
//! type-directed operations (query planning, validation, proof production), we
//! embed canonical `.axi` schemas/theories into the DB as a **meta-plane**
//! graph (see `axi_meta.rs` and `axi_module_import.rs`).
//!
//! ## Lean correspondence (semantics)
//!
//! This meta-plane index is the Rust execution-layer analogue of the Lean
//! semantics described in:
//!
//! - `docs/explanation/TOPOS_THEORY.md` (explanation-level),
//! - `lean/Axiograph/Topos/Overview.lean` (mathlib-backed semantic scaffold).
//!
//! In that view:
//! - a schema presents a category (objects = types, relations-as-objects + projection arrows),
//! - an instance is a functor into finite sets,
//! - and theory constraints are predicates/subobjects in the internal logic.
//!
//! Rust uses this index to stay **type-directed** (and efficient) while keeping
//! the trusted checker small (Lean checks certificates at the boundary).
//!
//! This module builds a structured index over that meta-plane and exposes
//! basic, schema-directed checks over imported instance data:
//!
//! - object types and declared subtyping (`sub < sup`)
//! - n-ary relation declarations (fields + field types)
//! - well-typedness of imported relation tuples (aka “fact nodes”)
//!
//! Long-term, this evolves into:
//! - type-directed query optimization (keys/functionals as join hints),
//! - proof-producing validation (Rust emits a certificate; Lean verifies),
//! - and first-class “higher structure” metadata (morphisms, homotopies,
//!   modalities, migrations) anchored to canonical `.axi`.

use std::collections::{HashMap, HashSet};

use anyhow::Result;

use axiograph_dsl::schema_v1::RewriteVarDeclV1;

use crate::axi_meta::*;
use crate::PathDB;

#[derive(Debug, Clone, Default)]
pub struct MetaPlaneIndex {
    /// Map `schema_name -> schema_index`.
    pub schemas: HashMap<String, SchemaIndex>,
}

#[derive(Debug, Clone)]
pub struct SchemaIndex {
    pub schema_entity: u32,
    pub module_name: Option<String>,
    pub object_types: HashSet<String>,
    pub subtype_decls: Vec<SubtypeDecl>,
    pub relation_decls: HashMap<String, RelationDecl>,
    /// Theory constraints indexed by relation name.
    ///
    /// These come from `AxiMetaConstraint` nodes linked under:
    /// `schema -> axi_schema_has_theory -> axi_theory_has_constraint`.
    pub constraints_by_relation: HashMap<String, Vec<ConstraintDecl>>,
    /// First-class rewrite rules declared in theories attached to this schema.
    ///
    /// These come from `AxiMetaRewriteRule` nodes linked under:
    /// `schema -> axi_schema_has_theory -> axi_theory_has_rewrite_rule`.
    pub rewrite_rules_by_theory: HashMap<String, Vec<RewriteRuleDecl>>,
    pub supertypes_of: HashMap<String, HashSet<String>>,
}

#[derive(Debug, Clone)]
pub struct RewriteRuleDecl {
    pub rule_entity: u32,
    pub theory_name: String,
    pub name: String,
    pub orientation: String,
    pub vars_text: String,
    pub vars: Vec<RewriteVarDeclV1>,
    #[allow(dead_code)]
    pub vars_parse_error: Option<String>,
    pub lhs: String,
    pub rhs: String,
    pub index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstraintDecl {
    Functional {
        relation: String,
        src_field: String,
        dst_field: String,
    },
    Symmetric {
        relation: String,
    },
    Transitive {
        relation: String,
    },
    Key {
        relation: String,
        fields: Vec<String>,
    },
    Unknown {
        relation: Option<String>,
        text: String,
    },
}

#[derive(Debug, Clone)]
pub struct RelationDecl {
    pub relation_entity: u32,
    pub name: String,
    pub fields: Vec<FieldDecl>,
}

#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub field_entity: u32,
    pub field_name: String,
    pub field_type: String,
    pub field_index: usize,
}

#[derive(Debug, Clone)]
pub struct SubtypeDecl {
    pub subtype_entity: u32,
    pub sub: String,
    pub sup: String,
}

impl SchemaIndex {
    pub fn is_subtype(&self, sub: &str, sup: &str) -> bool {
        self.supertypes_of
            .get(sub)
            .map(|s| s.contains(sup))
            .unwrap_or(sub == sup)
    }

    /// Canonical tuple (fact-node) entity type name for a relation in this schema.
    ///
    /// In Axiograph, relations are often reified as first-class fact nodes so we
    /// can attach attributes, provenance, context/world scoping, etc.
    ///
    /// If a schema has both:
    /// - `object Foo`, and
    /// - `relation Foo(...)`,
    /// we use `FooFact` as the tuple type to avoid a name collision with the
    /// object type.
    pub fn tuple_entity_type_name(&self, relation_name: &str) -> String {
        if self.object_types.contains(relation_name) {
            format!("{relation_name}Fact")
        } else {
            relation_name.to_string()
        }
    }
}

impl MetaPlaneIndex {
    pub fn from_db(db: &PathDB) -> Result<Self> {
        let mut out = MetaPlaneIndex::default();

        let Some(schema_ids) = db.find_by_type(META_TYPE_SCHEMA) else {
            return Ok(out);
        };

        for schema_entity in schema_ids.iter() {
            let Some(schema_name) = entity_attr_string(db, schema_entity, META_ATTR_NAME) else {
                continue;
            };

            let module_name = entity_attr_string(db, schema_entity, ATTR_AXI_MODULE);

            let mut object_types = HashSet::new();
            for oid in db
                .follow_one(schema_entity, META_REL_SCHEMA_HAS_OBJECT)
                .iter()
            {
                if let Some(name) = entity_attr_string(db, oid, META_ATTR_NAME) {
                    object_types.insert(name);
                }
            }

            let mut subtype_decls: Vec<SubtypeDecl> = Vec::new();
            for stid in db
                .follow_one(schema_entity, META_REL_SCHEMA_HAS_SUBTYPE)
                .iter()
            {
                let Some(sub) = entity_attr_string(db, stid, ATTR_SUBTYPE_SUB) else {
                    continue;
                };
                let Some(sup) = entity_attr_string(db, stid, ATTR_SUBTYPE_SUP) else {
                    continue;
                };
                subtype_decls.push(SubtypeDecl {
                    subtype_entity: stid,
                    sub,
                    sup,
                });
            }

            let mut relation_decls: HashMap<String, RelationDecl> = HashMap::new();
            for rid in db
                .follow_one(schema_entity, META_REL_SCHEMA_HAS_RELATION)
                .iter()
            {
                let Some(name) = entity_attr_string(db, rid, META_ATTR_NAME) else {
                    continue;
                };

                let mut fields: Vec<FieldDecl> = Vec::new();
                for fid in db.follow_one(rid, META_REL_RELATION_HAS_FIELD).iter() {
                    let Some(field_name) = entity_attr_string(db, fid, ATTR_FIELD_NAME) else {
                        continue;
                    };
                    let Some(field_type) = entity_attr_string(db, fid, ATTR_FIELD_TYPE) else {
                        continue;
                    };
                    let field_index = entity_attr_string(db, fid, ATTR_FIELD_INDEX)
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(0);
                    fields.push(FieldDecl {
                        field_entity: fid,
                        field_name,
                        field_type,
                        field_index,
                    });
                }
                fields.sort_by_key(|f| f.field_index);

                relation_decls.insert(
                    name.clone(),
                    RelationDecl {
                        relation_entity: rid,
                        name,
                        fields,
                    },
                );
            }

            // Constraints (from theories attached to this schema).
            let mut constraints_by_relation: HashMap<String, Vec<ConstraintDecl>> = HashMap::new();
            let mut rewrite_rules_by_theory: HashMap<String, Vec<RewriteRuleDecl>> = HashMap::new();
            for theory_id in db
                .follow_one(schema_entity, META_REL_SCHEMA_HAS_THEORY)
                .iter()
            {
                let theory_name = entity_attr_string(db, theory_id, META_ATTR_NAME)
                    .unwrap_or_else(|| format!("theory_{theory_id}"));

                for cid in db
                    .follow_one(theory_id, META_REL_THEORY_HAS_CONSTRAINT)
                    .iter()
                {
                    let kind = entity_attr_string(db, cid, ATTR_CONSTRAINT_KIND)
                        .unwrap_or_else(|| "unknown".to_string());
                    let rel_name = entity_attr_string(db, cid, ATTR_CONSTRAINT_RELATION);

                    let decl = match kind.as_str() {
                        "functional" => {
                            let relation = rel_name.clone().unwrap_or_default();
                            let src_field = entity_attr_string(db, cid, ATTR_CONSTRAINT_SRC_FIELD)
                                .unwrap_or_default();
                            let dst_field = entity_attr_string(db, cid, ATTR_CONSTRAINT_DST_FIELD)
                                .unwrap_or_default();
                            ConstraintDecl::Functional {
                                relation,
                                src_field,
                                dst_field,
                            }
                        }
                        "symmetric" => ConstraintDecl::Symmetric {
                            relation: rel_name.clone().unwrap_or_default(),
                        },
                        "transitive" => ConstraintDecl::Transitive {
                            relation: rel_name.clone().unwrap_or_default(),
                        },
                        "key" => {
                            let relation = rel_name.clone().unwrap_or_default();
                            let fields_csv = entity_attr_string(db, cid, ATTR_CONSTRAINT_FIELDS)
                                .unwrap_or_default();
                            let fields = fields_csv
                                .split(',')
                                .map(|s| s.trim())
                                .filter(|s| !s.is_empty())
                                .map(|s| s.to_string())
                                .collect::<Vec<_>>();
                            ConstraintDecl::Key { relation, fields }
                        }
                        _other => ConstraintDecl::Unknown {
                            relation: rel_name.clone(),
                            text: entity_attr_string(db, cid, ATTR_CONSTRAINT_TEXT)
                                .unwrap_or_else(|| kind.clone()),
                        },
                    };

                    let key = match &decl {
                        ConstraintDecl::Functional { relation, .. } => relation.clone(),
                        ConstraintDecl::Symmetric { relation } => relation.clone(),
                        ConstraintDecl::Transitive { relation } => relation.clone(),
                        ConstraintDecl::Key { relation, .. } => relation.clone(),
                        ConstraintDecl::Unknown { relation, .. } => {
                            relation.clone().unwrap_or_default()
                        }
                    };

                    if !key.is_empty() {
                        constraints_by_relation.entry(key).or_default().push(decl);
                    }
                }

                // Rewrite rules (first-class, certificate-addressable semantics).
                for rid in db
                    .follow_one(theory_id, META_REL_THEORY_HAS_REWRITE_RULE)
                    .iter()
                {
                    let Some(name) = entity_attr_string(db, rid, META_ATTR_NAME) else {
                        continue;
                    };
                    let orientation = entity_attr_string(db, rid, ATTR_REWRITE_RULE_ORIENTATION)
                        .unwrap_or_else(|| "forward".to_string());
                    let vars_text = entity_attr_string(db, rid, ATTR_REWRITE_RULE_VARS)
                        .unwrap_or_default();
                    let (vars, vars_parse_error) =
                        match axiograph_dsl::schema_v1::parse_rewrite_var_decl_list_v1(&vars_text) {
                            Ok(v) => (v, None),
                            Err(e) => (Vec::new(), Some(e)),
                        };
                    let lhs = entity_attr_string(db, rid, ATTR_REWRITE_RULE_LHS).unwrap_or_default();
                    let rhs = entity_attr_string(db, rid, ATTR_REWRITE_RULE_RHS).unwrap_or_default();
                    let index = entity_attr_string(db, rid, ATTR_REWRITE_RULE_INDEX)
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(usize::MAX);

                    rewrite_rules_by_theory
                        .entry(theory_name.clone())
                        .or_default()
                        .push(RewriteRuleDecl {
                            rule_entity: rid,
                            theory_name: theory_name.clone(),
                            name,
                            orientation,
                            vars_text,
                            vars,
                            vars_parse_error,
                            lhs,
                            rhs,
                            index,
                        });
                }
            }

            let supertypes_of = compute_supertypes_closure(&object_types, &subtype_decls);

            out.schemas.insert(
                schema_name.clone(),
                SchemaIndex {
                    schema_entity,
                    module_name,
                    object_types,
                    subtype_decls,
                    relation_decls,
                    constraints_by_relation,
                    rewrite_rules_by_theory,
                    supertypes_of,
                },
            );
        }

        Ok(out)
    }

    pub fn typecheck_axi_facts(&self, db: &PathDB) -> AxiTypeCheckReport {
        let mut report = AxiTypeCheckReport::default();

        let Some(relation_key_id) = db.interner.id_of(ATTR_AXI_RELATION) else {
            return report;
        };
        let Some(relation_col) = db.entities.attrs.get(&relation_key_id) else {
            return report;
        };

        for (&fact_entity, &relation_value_id) in relation_col {
            let Some(relation_name) = db.interner.lookup(relation_value_id) else {
                continue;
            };

            let Some(schema_name) = entity_attr_string(db, fact_entity, ATTR_AXI_SCHEMA) else {
                report
                    .errors
                    .push(AxiTypeCheckError::MissingSchema { fact: fact_entity });
                continue;
            };

            let Some(schema_index) = self.schemas.get(&schema_name) else {
                report.errors.push(AxiTypeCheckError::UnknownSchema {
                    fact: fact_entity,
                    schema: schema_name,
                });
                continue;
            };

            let Some(relation_decl) = schema_index.relation_decls.get(&relation_name) else {
                report.errors.push(AxiTypeCheckError::UnknownRelation {
                    fact: fact_entity,
                    schema: schema_name,
                    relation: relation_name,
                });
                continue;
            };

            report.checked_facts += 1;

            for field in &relation_decl.fields {
                let Some(field_rel_id) = db.interner.id_of(&field.field_name) else {
                    report.errors.push(AxiTypeCheckError::MissingField {
                        fact: fact_entity,
                        relation: relation_decl.name.clone(),
                        field: field.field_name.clone(),
                    });
                    continue;
                };

                let outgoing = db.relations.outgoing(fact_entity, field_rel_id);
                if outgoing.is_empty() {
                    report.errors.push(AxiTypeCheckError::MissingField {
                        fact: fact_entity,
                        relation: relation_decl.name.clone(),
                        field: field.field_name.clone(),
                    });
                    continue;
                }
                if outgoing.len() > 1 {
                    report.errors.push(AxiTypeCheckError::MultipleFieldValues {
                        fact: fact_entity,
                        relation: relation_decl.name.clone(),
                        field: field.field_name.clone(),
                        values: outgoing.iter().map(|r| r.target).collect(),
                    });
                    continue;
                }

                let value_entity = outgoing[0].target;
                let Some(actual_type_id) = db.entities.get_type(value_entity) else {
                    report.errors.push(AxiTypeCheckError::MissingEntityType {
                        fact: fact_entity,
                        relation: relation_decl.name.clone(),
                        field: field.field_name.clone(),
                        value: value_entity,
                    });
                    continue;
                };
                let Some(actual_type) = db.interner.lookup(actual_type_id) else {
                    report.errors.push(AxiTypeCheckError::MissingEntityType {
                        fact: fact_entity,
                        relation: relation_decl.name.clone(),
                        field: field.field_name.clone(),
                        value: value_entity,
                    });
                    continue;
                };

                if !schema_index.is_subtype(&actual_type, &field.field_type) {
                    report.errors.push(AxiTypeCheckError::FieldTypeMismatch {
                        fact: fact_entity,
                        relation: relation_decl.name.clone(),
                        field: field.field_name.clone(),
                        expected_type: field.field_type.clone(),
                        actual_type,
                        value: value_entity,
                    });
                }
            }
        }

        report
    }
}

#[derive(Debug, Clone, Default)]
pub struct AxiTypeCheckReport {
    pub checked_facts: usize,
    pub errors: Vec<AxiTypeCheckError>,
}

impl AxiTypeCheckReport {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AxiTypeCheckError {
    MissingSchema {
        fact: u32,
    },
    UnknownSchema {
        fact: u32,
        schema: String,
    },
    UnknownRelation {
        fact: u32,
        schema: String,
        relation: String,
    },
    MissingField {
        fact: u32,
        relation: String,
        field: String,
    },
    MultipleFieldValues {
        fact: u32,
        relation: String,
        field: String,
        values: Vec<u32>,
    },
    MissingEntityType {
        fact: u32,
        relation: String,
        field: String,
        value: u32,
    },
    FieldTypeMismatch {
        fact: u32,
        relation: String,
        field: String,
        expected_type: String,
        actual_type: String,
        value: u32,
    },
}

impl std::fmt::Display for AxiTypeCheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AxiTypeCheckError::MissingSchema { fact } => {
                write!(f, "fact {fact}: missing `{ATTR_AXI_SCHEMA}` attribute")
            }
            AxiTypeCheckError::UnknownSchema { fact, schema } => {
                write!(f, "fact {fact}: unknown schema `{schema}`")
            }
            AxiTypeCheckError::UnknownRelation {
                fact,
                schema,
                relation,
            } => write!(
                f,
                "fact {fact}: unknown relation `{relation}` in schema `{schema}`"
            ),
            AxiTypeCheckError::MissingField { fact, relation, field } => write!(
                f,
                "fact {fact} ({relation}): missing field edge `{field}`"
            ),
            AxiTypeCheckError::MultipleFieldValues {
                fact,
                relation,
                field,
                values,
            } => write!(
                f,
                "fact {fact} ({relation}): multiple values for `{field}`: {values:?}"
            ),
            AxiTypeCheckError::MissingEntityType {
                fact,
                relation,
                field,
                value,
            } => write!(
                f,
                "fact {fact} ({relation}): field `{field}` points to entity {value} with missing type"
            ),
            AxiTypeCheckError::FieldTypeMismatch {
                fact,
                relation,
                field,
                expected_type,
                actual_type,
                value,
            } => write!(
                f,
                "fact {fact} ({relation}): field `{field}` expects `{expected_type}` but got `{actual_type}` (entity {value})"
            ),
        }
    }
}

impl std::error::Error for AxiTypeCheckError {}

fn entity_attr_string(db: &PathDB, entity_id: u32, key: &str) -> Option<String> {
    let key_id = db.interner.id_of(key)?;
    let value_id = db.entities.get_attr(entity_id, key_id)?;
    db.interner.lookup(value_id)
}

fn compute_supertypes_closure(
    object_types: &HashSet<String>,
    subtype_decls: &[SubtypeDecl],
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
