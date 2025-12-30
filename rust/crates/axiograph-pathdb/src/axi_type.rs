//! Axiograph “type system” primitives (explicit, data-driven).
//!
//! Rust cannot represent the `.axi` language’s dependent typing directly in
//! the host type system, so Axiograph treats typing as **runtime data**:
//!
//! - the canonical `.axi` meta-plane defines object types, relations, subtyping,
//!   constraints, and rewrite rules,
//! - `MetaPlaneIndex` is the in-memory representation of that meta-plane, and
//! - this module provides a small, explicit “type algebra” (`AxiType`) plus a
//!   corresponding environment (`TypingEnv`) that higher-level code can depend
//!   on without smuggling schema semantics into ad-hoc strings.
//!
//! This is intentionally minimal: it’s a foundation for “checked by construction”
//! APIs in Rust (builders/typestate), not a second copy of Lean.
//!
//! Lean remains the trusted checker for certificates. Rust-side typing is:
//! - a correctness guardrail (avoid building nonsense),
//! - an ergonomics improvement (better errors), and
//! - a performance affordance (type-directed pruning).

use crate::axi_meta::{ATTR_AXI_RELATION, ATTR_AXI_SCHEMA};
use crate::axi_semantics::{MetaPlaneIndex, SchemaIndex};
use crate::PathDB;

/// A small “type algebra” for Axiograph entities.
///
/// This is schema-scoped: the same type name can exist in multiple schemas.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AxiType {
    /// An object type declared in a schema: `object Person`.
    ObjectType { schema: String, name: String },

    /// A reified relation tuple type (fact node).
    ///
    /// In canonical `.axi` import, n-ary relation instances are represented as
    /// first-class “fact node” entities with field edges, plus derived binary
    /// edges for ergonomic traversal.
    FactType { schema: String, relation: String },

    /// A path expression type `Path(x,y)` (used in rewrite rules / certificates).
    ///
    /// Note: this is about the *endpoints* of a path in a schema, not about a
    /// specific runtime path witness.
    PathType {
        schema: String,
        from_type: String,
        to_type: String,
    },
}

impl AxiType {
    pub fn schema_name(&self) -> &str {
        match self {
            AxiType::ObjectType { schema, .. } => schema,
            AxiType::FactType { schema, .. } => schema,
            AxiType::PathType { schema, .. } => schema,
        }
    }
}

/// A runtime typing environment derived from the meta-plane.
#[derive(Debug, Clone)]
pub struct TypingEnv {
    pub meta: MetaPlaneIndex,
}

impl TypingEnv {
    pub fn from_db(db: &PathDB) -> anyhow::Result<Self> {
        Ok(Self {
            meta: MetaPlaneIndex::from_db(db)?,
        })
    }

    pub fn schema(&self, schema_name: &str) -> Option<&SchemaIndex> {
        self.meta.schemas.get(schema_name)
    }

    /// Determine the `.axi` schema name associated with a PathDB entity.
    pub fn axi_schema_of_entity(&self, db: &PathDB, entity: u32) -> Option<String> {
        let key_id = db.interner.id_of(ATTR_AXI_SCHEMA)?;
        let value_id = db.entities.get_attr(entity, key_id)?;
        db.interner.lookup(value_id)
    }

    /// Determine the `.axi` relation name associated with a fact node.
    pub fn axi_relation_of_fact(&self, db: &PathDB, entity: u32) -> Option<String> {
        let key_id = db.interner.id_of(ATTR_AXI_RELATION)?;
        let value_id = db.entities.get_attr(entity, key_id)?;
        db.interner.lookup(value_id)
    }

    /// Best-effort: compute an `AxiType` for a PathDB entity by inspecting
    /// `axi_schema` and (when present) `axi_relation`.
    ///
    /// Notes:
    /// - Not every entity in a PathDB is schema-scoped (e.g. some ingestion
    ///   overlays), so this can return `None`.
    /// - For object entities, we use the entity’s concrete PathDB type name.
    pub fn axi_type_of_entity(&self, db: &PathDB, entity: u32) -> Option<AxiType> {
        let schema = self.axi_schema_of_entity(db, entity)?;
        if let Some(relation) = self.axi_relation_of_fact(db, entity) {
            return Some(AxiType::FactType { schema, relation });
        }

        let type_id = db.entities.get_type(entity)?;
        let type_name = db.interner.lookup(type_id)?;
        Some(AxiType::ObjectType {
            schema,
            name: type_name,
        })
    }
}

