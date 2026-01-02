//! Axiograph meta-plane vocabulary for representing `.axi` schema/theory metadata inside PathDB.
//!
//! PathDB is fundamentally a labeled directed graph. To support “smart”, schema-directed
//! querying and to enable exporting back into canonical `.axi` modules, we embed the
//! schema/theory layer as a *meta graph* alongside instance data.
//!
//! This module centralizes the string constants used for:
//! - meta entity types (`AxiMetaSchema`, `AxiMetaRelation`, …)
//! - meta relations (`axi_has_schema`, `axi_relation_has_field`, …)
//! - meta attributes (`axi_meta_id`, `axi_constraint_kind`, …)
//!
//! Keeping these in one place avoids accidental drift between importer/exporter code.

// -----------------------------------------------------------------------------
// Meta entity types
// -----------------------------------------------------------------------------

pub const META_TYPE_MODULE: &str = "AxiMetaModule";
pub const META_TYPE_SCHEMA: &str = "AxiMetaSchema";
pub const META_TYPE_OBJECT_TYPE: &str = "AxiMetaObjectType";
pub const META_TYPE_RELATION_DECL: &str = "AxiMetaRelationDecl";
pub const META_TYPE_FIELD_DECL: &str = "AxiMetaFieldDecl";
pub const META_TYPE_SUBTYPE_DECL: &str = "AxiMetaSubtypeDecl";
pub const META_TYPE_THEORY: &str = "AxiMetaTheory";
pub const META_TYPE_CONSTRAINT: &str = "AxiMetaConstraint";
pub const META_TYPE_EQUATION: &str = "AxiMetaEquation";
pub const META_TYPE_REWRITE_RULE: &str = "AxiMetaRewriteRule";
pub const META_TYPE_INSTANCE: &str = "AxiMetaInstance";

// -----------------------------------------------------------------------------
// Meta relations (edge labels)
// -----------------------------------------------------------------------------

pub const META_REL_HAS_SCHEMA: &str = "axi_has_schema";
pub const META_REL_SCHEMA_HAS_OBJECT: &str = "axi_schema_has_object";
pub const META_REL_SCHEMA_HAS_RELATION: &str = "axi_schema_has_relation";
pub const META_REL_RELATION_HAS_FIELD: &str = "axi_relation_has_field";
pub const META_REL_SCHEMA_HAS_SUBTYPE: &str = "axi_schema_has_subtype";
pub const META_REL_SCHEMA_HAS_THEORY: &str = "axi_schema_has_theory";
pub const META_REL_THEORY_HAS_CONSTRAINT: &str = "axi_theory_has_constraint";
pub const META_REL_THEORY_HAS_EQUATION: &str = "axi_theory_has_equation";
pub const META_REL_THEORY_HAS_REWRITE_RULE: &str = "axi_theory_has_rewrite_rule";
pub const META_REL_HAS_INSTANCE: &str = "axi_has_instance";

/// Subtype relation between object type declarations (sub → sup).
pub const META_REL_SUBTYPE_OF: &str = "axi_subtype_of";

/// A relation-tuple (fact node) belongs to a relation declaration.
pub const META_REL_FACT_OF: &str = "axi_fact_of";

/// Optional context/world scoping: a fact node holds in a context.
///
/// Canonical `.axi` represents context by a regular tuple field (typically named `ctx`,
/// introduced via the `@context ContextType` annotation). PathDB derives this edge at
/// import time so queries and indexes can scope facts efficiently without the checker
/// depending on any special “DB semantics”.
pub const REL_AXI_FACT_IN_CONTEXT: &str = "axi_fact_in_context";

// -----------------------------------------------------------------------------
// Common attributes
// -----------------------------------------------------------------------------

/// Stable id for meta entities (used to avoid duplicates on repeated import).
pub const META_ATTR_ID: &str = "axi_meta_id";

/// Human-readable name of the meta entity.
pub const META_ATTR_NAME: &str = "name";

/// Dialect tag (currently `"axi_v1"` / `"axi_schema_v1"`).
pub const META_ATTR_DIALECT: &str = "axi_dialect";

/// Digest of the original `.axi` text (FNV-1a 64-bit), if known.
pub const META_ATTR_AXI_DIGEST_V1: &str = "axi_digest_v1";

/// Reused on instance data and meta entities to indicate provenance.
pub const ATTR_AXI_MODULE: &str = "axi_module";
pub const ATTR_AXI_SCHEMA: &str = "axi_schema";
pub const ATTR_AXI_INSTANCE: &str = "axi_instance";

/// Attached to tuple entities imported from relation assignments.
pub const ATTR_AXI_RELATION: &str = "axi_relation";
pub const ATTR_AXI_FACT_ID: &str = "axi_fact_id";

// Field decl attrs
pub const ATTR_FIELD_NAME: &str = "axi_field";
pub const ATTR_FIELD_TYPE: &str = "axi_field_type";
pub const ATTR_FIELD_INDEX: &str = "axi_field_index";

// Subtype decl attrs
pub const ATTR_SUBTYPE_SUB: &str = "axi_sub";
pub const ATTR_SUBTYPE_SUP: &str = "axi_sup";
pub const ATTR_SUBTYPE_INCLUSION: &str = "axi_inclusion";

// Constraint attrs
pub const ATTR_CONSTRAINT_KIND: &str = "axi_constraint_kind";
pub const ATTR_CONSTRAINT_RELATION: &str = "axi_constraint_relation";
pub const ATTR_CONSTRAINT_NAME: &str = "axi_constraint_name";
pub const ATTR_CONSTRAINT_SRC_FIELD: &str = "axi_constraint_src_field";
pub const ATTR_CONSTRAINT_DST_FIELD: &str = "axi_constraint_dst_field";
pub const ATTR_CONSTRAINT_FIELDS: &str = "axi_constraint_fields";
pub const ATTR_CONSTRAINT_PARAM_FIELDS: &str = "axi_constraint_param_fields";
pub const ATTR_CONSTRAINT_WHERE_FIELD: &str = "axi_constraint_where_field";
pub const ATTR_CONSTRAINT_WHERE_IN_VALUES: &str = "axi_constraint_where_in_values";
pub const ATTR_CONSTRAINT_MAX: &str = "axi_constraint_max";
pub const ATTR_CONSTRAINT_TEXT: &str = "axi_constraint_text";
pub const ATTR_CONSTRAINT_INDEX: &str = "axi_constraint_index";

// Equation attrs
pub const ATTR_EQUATION_LHS: &str = "axi_equation_lhs";
pub const ATTR_EQUATION_RHS: &str = "axi_equation_rhs";
pub const ATTR_EQUATION_INDEX: &str = "axi_equation_index";

// Rewrite rule attrs
pub const ATTR_REWRITE_RULE_ORIENTATION: &str = "axi_rewrite_rule_orientation";
pub const ATTR_REWRITE_RULE_VARS: &str = "axi_rewrite_rule_vars";
pub const ATTR_REWRITE_RULE_LHS: &str = "axi_rewrite_rule_lhs";
pub const ATTR_REWRITE_RULE_RHS: &str = "axi_rewrite_rule_rhs";
pub const ATTR_REWRITE_RULE_INDEX: &str = "axi_rewrite_rule_index";

// Instance decl attrs
pub const ATTR_INSTANCE_SCHEMA: &str = "axi_instance_schema";

// -----------------------------------------------------------------------------
// Meta id helpers
// -----------------------------------------------------------------------------

pub fn meta_id_module(module_name: &str) -> String {
    format!("axi_meta_module:{module_name}")
}

pub fn meta_id_schema(module_name: &str, schema_name: &str) -> String {
    format!("axi_meta_schema:{module_name}:{schema_name}")
}

pub fn meta_id_object_type(module_name: &str, schema_name: &str, object_name: &str) -> String {
    format!("axi_meta_object:{module_name}:{schema_name}:{object_name}")
}

pub fn meta_id_relation_decl(module_name: &str, schema_name: &str, relation_name: &str) -> String {
    format!("axi_meta_relation:{module_name}:{schema_name}:{relation_name}")
}

pub fn meta_id_field_decl(
    module_name: &str,
    schema_name: &str,
    relation_name: &str,
    field_name: &str,
) -> String {
    format!("axi_meta_field:{module_name}:{schema_name}:{relation_name}:{field_name}")
}

pub fn meta_id_subtype_decl(module_name: &str, schema_name: &str, sub: &str, sup: &str) -> String {
    format!("axi_meta_subtype:{module_name}:{schema_name}:{sub}<{sup}")
}

pub fn meta_id_theory(module_name: &str, theory_name: &str) -> String {
    format!("axi_meta_theory:{module_name}:{theory_name}")
}

pub fn meta_id_constraint(module_name: &str, theory_name: &str, index: usize) -> String {
    format!("axi_meta_constraint:{module_name}:{theory_name}:{index}")
}

pub fn meta_id_equation(module_name: &str, theory_name: &str, equation_name: &str) -> String {
    format!("axi_meta_equation:{module_name}:{theory_name}:{equation_name}")
}

pub fn meta_id_rewrite_rule(module_name: &str, theory_name: &str, rule_name: &str) -> String {
    format!("axi_meta_rewrite_rule:{module_name}:{theory_name}:{rule_name}")
}

pub fn meta_id_instance(module_name: &str, instance_name: &str) -> String {
    format!("axi_meta_instance:{module_name}:{instance_name}")
}
