//! Schema-scoped, witness-carrying typed views over PathDB.
//!
//! Rust does not have full dependent types, but we can still encode many of the
//! useful “dependent-typing effects” by introducing *validated wrappers*:
//!
//! - `AxiTypedEntity` means: “this entity belongs to schema `S` and inhabits type `T`”
//! - `AxiTypedFact` means: “this fact node is a well-typed tuple for relation `R` in schema `S`”
//!
//! These wrappers are constructed by *checking* against the `.axi` meta-plane
//! index (`axi_semantics::MetaPlaneIndex`). Once constructed, downstream code can
//! accept `AxiTyped*` values instead of raw `u32`s and avoid a large class of
//! “wrong schema / wrong type” bugs.
//!
//! ## Lean correspondence
//!
//! Conceptually, these are Rust analogues of Lean `Sigma`/`Subtype` patterns:
//! the wrapper carries data (an id) together with a witness that it is well-typed
//! under the schema/theory semantics.
//!
//! See:
//! - `docs/explanation/RUST_DEPENDENT_TYPES.md` (design patterns),
//! - `docs/explanation/TOPOS_THEORY.md` and `lean/Axiograph/Topos/Overview.lean` (semantics-level correspondence),
//! - `lean/Axiograph/Axi/TypeCheck.lean` (Lean-side well-formedness checks).
//!
//! Long-term:
//! - Rust can optionally emit certificates for these checks,
//! - Lean verifies those certificates, and
//! - the runtime can treat “typed” operations as auditable optimization inputs.

use crate::axi_meta::*;
use crate::axi_semantics::{MetaPlaneIndex, SchemaIndex};
use crate::PathDB;
use crate::{DbToken, DbTokenMismatch};
use roaring::RoaringBitmap;

/// Errors produced by schema-scoped typing checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AxiTypingError {
    UnknownSchema {
        schema: String,
    },
    MissingSchemaAttr {
        entity: u32,
    },
    SchemaMismatch {
        entity: u32,
        expected_schema: String,
        actual_schema: String,
    },
    MissingEntityType {
        entity: u32,
    },
    MissingEntityTypeName {
        entity: u32,
        type_id: u32,
    },
    TypeMismatch {
        entity: u32,
        expected_type: String,
        actual_type: String,
    },
    MissingRelationAttr {
        fact: u32,
    },
    UnknownRelation {
        fact: u32,
        relation: String,
    },
    MissingFieldEdge {
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
}

impl std::fmt::Display for AxiTypingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AxiTypingError::UnknownSchema { schema } => write!(f, "unknown `.axi` schema `{schema}`"),
            AxiTypingError::MissingSchemaAttr { entity } => {
                write!(f, "entity {entity}: missing `{ATTR_AXI_SCHEMA}` attribute")
            }
            AxiTypingError::SchemaMismatch {
                entity,
                expected_schema,
                actual_schema,
            } => write!(
                f,
                "entity {entity}: schema mismatch (expected `{expected_schema}`, got `{actual_schema}`)"
            ),
            AxiTypingError::MissingEntityType { entity } => write!(f, "entity {entity}: missing type"),
            AxiTypingError::MissingEntityTypeName { entity, type_id } => write!(
                f,
                "entity {entity}: type id {type_id} not found in interner"
            ),
            AxiTypingError::TypeMismatch {
                entity,
                expected_type,
                actual_type,
            } => write!(
                f,
                "entity {entity}: expected type `{expected_type}` but got `{actual_type}`"
            ),
            AxiTypingError::MissingRelationAttr { fact } => {
                write!(f, "fact {fact}: missing `{ATTR_AXI_RELATION}` attribute")
            }
            AxiTypingError::UnknownRelation { fact, relation } => {
                write!(f, "fact {fact}: unknown relation `{relation}`")
            }
            AxiTypingError::MissingFieldEdge { fact, relation, field } => write!(
                f,
                "fact {fact} ({relation}): missing field edge `{field}`"
            ),
            AxiTypingError::MultipleFieldValues {
                fact,
                relation,
                field,
                values,
            } => write!(
                f,
                "fact {fact} ({relation}): multiple values for `{field}`: {values:?}"
            ),
        }
    }
}

impl std::error::Error for AxiTypingError {}

/// A cached schema/type index built from the PathDB meta-plane.
#[derive(Debug, Clone)]
pub struct AxiTypingContext {
    meta: MetaPlaneIndex,
}

impl AxiTypingContext {
    pub fn from_db(db: &PathDB) -> anyhow::Result<Self> {
        Ok(Self {
            meta: MetaPlaneIndex::from_db(db)?,
        })
    }

    pub fn schema(&self, schema_name: &str) -> Result<AxiSchemaContext, AxiTypingError> {
        let Some(schema) = self.meta.schemas.get(schema_name) else {
            return Err(AxiTypingError::UnknownSchema {
                schema: schema_name.to_string(),
            });
        };
        Ok(AxiSchemaContext {
            schema_name: schema_name.to_string(),
            schema: schema.clone(),
        })
    }
}

/// A schema-scoped typing context.
#[derive(Debug, Clone)]
pub struct AxiSchemaContext {
    pub schema_name: String,
    schema: SchemaIndex,
}

/// A witness-carrying typed entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxiTypedEntity {
    entity: u32,
    db_token: DbToken,
    pub schema: String,
    pub expected_type: String,
    pub actual_type: String,
}

impl AxiTypedEntity {
    pub fn entity_id(&self, db: &PathDB) -> Result<u32, DbTokenMismatch> {
        if self.db_token != db.db_token() {
            return Err(DbTokenMismatch {
                expected: self.db_token,
                actual: db.db_token(),
            });
        }
        Ok(self.entity)
    }

    pub fn raw_entity_id(&self) -> u32 {
        self.entity
    }

    pub fn assert_in_db(&self, db: &PathDB) -> Result<(), DbTokenMismatch> {
        self.entity_id(db).map(|_| ())
    }
}

/// A witness-carrying typed fact node (relation tuple).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxiTypedFact {
    fact: u32,
    db_token: DbToken,
    pub schema: String,
    pub relation: String,
    pub fields: Vec<AxiTypedFieldValue>,
}

impl AxiTypedFact {
    pub fn fact_id(&self, db: &PathDB) -> Result<u32, DbTokenMismatch> {
        if self.db_token != db.db_token() {
            return Err(DbTokenMismatch {
                expected: self.db_token,
                actual: db.db_token(),
            });
        }
        Ok(self.fact)
    }

    pub fn raw_fact_id(&self) -> u32 {
        self.fact
    }

    pub fn assert_in_db(&self, db: &PathDB) -> Result<(), DbTokenMismatch> {
        self.fact_id(db).map(|_| ())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxiTypedFieldValue {
    pub field: String,
    value: u32,
    db_token: DbToken,
    pub expected_type: String,
    pub actual_type: String,
}

impl AxiTypedFieldValue {
    pub fn value_id(&self, db: &PathDB) -> Result<u32, DbTokenMismatch> {
        if self.db_token != db.db_token() {
            return Err(DbTokenMismatch {
                expected: self.db_token,
                actual: db.db_token(),
            });
        }
        Ok(self.value)
    }

    pub fn raw_value_id(&self) -> u32 {
        self.value
    }

    pub fn assert_in_db(&self, db: &PathDB) -> Result<(), DbTokenMismatch> {
        self.value_id(db).map(|_| ())
    }
}

impl AxiSchemaContext {
    fn entity_attr_string(db: &PathDB, entity_id: u32, key: &str) -> Option<String> {
        let key_id = db.interner.id_of(key)?;
        let value_id = db.entities.get_attr(entity_id, key_id)?;
        db.interner.lookup(value_id)
    }

    /// Attempt to view a raw PathDB entity as an inhabitant of a schema-scoped type.
    pub fn typed_entity(
        &self,
        db: &PathDB,
        entity: u32,
        expected_type: &str,
    ) -> Result<AxiTypedEntity, AxiTypingError> {
        let Some(actual_schema) = Self::entity_attr_string(db, entity, ATTR_AXI_SCHEMA) else {
            return Err(AxiTypingError::MissingSchemaAttr { entity });
        };
        if actual_schema != self.schema_name {
            return Err(AxiTypingError::SchemaMismatch {
                entity,
                expected_schema: self.schema_name.clone(),
                actual_schema,
            });
        }

        let Some(actual_type_id) = db.entities.get_type(entity) else {
            return Err(AxiTypingError::MissingEntityType { entity });
        };
        let Some(actual_type) = db.interner.lookup(actual_type_id) else {
            return Err(AxiTypingError::MissingEntityTypeName {
                entity,
                type_id: actual_type_id.raw(),
            });
        };

        if !self.schema.is_subtype(&actual_type, expected_type) {
            return Err(AxiTypingError::TypeMismatch {
                entity,
                expected_type: expected_type.to_string(),
                actual_type,
            });
        }

        Ok(AxiTypedEntity {
            entity,
            db_token: db.db_token(),
            schema: self.schema_name.clone(),
            expected_type: expected_type.to_string(),
            actual_type,
        })
    }

    /// Validate a fact node as a well-typed relation tuple in this schema.
    pub fn typed_fact(&self, db: &PathDB, fact: u32) -> Result<AxiTypedFact, AxiTypingError> {
        let Some(actual_schema) = Self::entity_attr_string(db, fact, ATTR_AXI_SCHEMA) else {
            return Err(AxiTypingError::MissingSchemaAttr { entity: fact });
        };
        if actual_schema != self.schema_name {
            return Err(AxiTypingError::SchemaMismatch {
                entity: fact,
                expected_schema: self.schema_name.clone(),
                actual_schema,
            });
        }

        let Some(relation_name) = Self::entity_attr_string(db, fact, ATTR_AXI_RELATION) else {
            return Err(AxiTypingError::MissingRelationAttr { fact });
        };
        let Some(relation_decl) = self.schema.relation_decls.get(&relation_name) else {
            return Err(AxiTypingError::UnknownRelation {
                fact,
                relation: relation_name,
            });
        };

        let mut fields: Vec<AxiTypedFieldValue> = Vec::with_capacity(relation_decl.fields.len());
        for field in &relation_decl.fields {
            let Some(field_rel_id) = db.interner.id_of(&field.field_name) else {
                return Err(AxiTypingError::MissingFieldEdge {
                    fact,
                    relation: relation_name.clone(),
                    field: field.field_name.clone(),
                });
            };

            let outgoing = db.relations.outgoing(fact, field_rel_id);
            if outgoing.is_empty() {
                return Err(AxiTypingError::MissingFieldEdge {
                    fact,
                    relation: relation_name.clone(),
                    field: field.field_name.clone(),
                });
            }
            if outgoing.len() > 1 {
                return Err(AxiTypingError::MultipleFieldValues {
                    fact,
                    relation: relation_name.clone(),
                    field: field.field_name.clone(),
                    values: outgoing.iter().map(|r| r.target).collect(),
                });
            }

            let value = outgoing[0].target;
            let Some(value_type_id) = db.entities.get_type(value) else {
                return Err(AxiTypingError::MissingEntityType { entity: value });
            };
            let Some(actual_type) = db.interner.lookup(value_type_id) else {
                return Err(AxiTypingError::MissingEntityTypeName {
                    entity: value,
                    type_id: value_type_id.raw(),
                });
            };

            if !self.schema.is_subtype(&actual_type, &field.field_type) {
                return Err(AxiTypingError::TypeMismatch {
                    entity: value,
                    expected_type: field.field_type.clone(),
                    actual_type,
                });
            }

            fields.push(AxiTypedFieldValue {
                field: field.field_name.clone(),
                value,
                db_token: db.db_token(),
                expected_type: field.field_type.clone(),
                actual_type,
            });
        }

        Ok(AxiTypedFact {
            fact,
            db_token: db.db_token(),
            schema: self.schema_name.clone(),
            relation: relation_name,
            fields,
        })
    }

    /// Entities in this schema whose (Axiograph) type is `type_name` (including subtypes).
    ///
    /// This is schema-scoped: it does not conflate types across schemas/modules that
    /// happen to share the same object name (e.g. many schemas have a `Text` type).
    pub fn find_by_axi_type(&self, db: &PathDB, type_name: &str) -> RoaringBitmap {
        db.find_by_axi_type(&self.schema_name, type_name)
    }
}

// =============================================================================
// PathDB helpers (schema-scoped selection)
// =============================================================================

impl PathDB {
    /// Return entities that belong to a particular `.axi` schema (by `axi_schema` attribute).
    pub fn entities_in_axi_schema(&self, schema_name: &str) -> RoaringBitmap {
        let Some(key_id) = self.interner.id_of(ATTR_AXI_SCHEMA) else {
            return RoaringBitmap::new();
        };
        let Some(value_id) = self.interner.id_of(schema_name) else {
            return RoaringBitmap::new();
        };
        self.entities.entities_with_attr_value(key_id, value_id)
    }

    /// Find entities by type, but scoped to a single `.axi` schema.
    ///
    /// Notes:
    /// - This uses PathDB's existing type index (fast), then intersects with the
    ///   schema plane using the `axi_schema` attribute.
    /// - The type index is populated by the `.axi` importer so supertypes include
    ///   subtype inhabitants.
    pub fn find_by_axi_type(&self, schema_name: &str, type_name: &str) -> RoaringBitmap {
        let mut candidates = self.find_by_type(type_name).cloned().unwrap_or_default();
        candidates &= self.entities_in_axi_schema(schema_name);
        candidates
    }
}
