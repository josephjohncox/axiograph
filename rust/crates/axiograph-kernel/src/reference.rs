//! Typed semantic reference currency.

use crate::{
    ConstraintIdV2, EquationIdV2, FactIdV2, InstanceIdV2, ModuleIdV2, ObjectTypeIdV2, RelationIdV2,
    RevisionDigestV2, RewriteRuleIdV2, RoleIdV2, SchemaIdV2, SemanticKeyV2, TheoryIdV2,
};
use serde::{Deserialize, Serialize};

/// A reference is always module- and revision-scoped. Human labels are not
/// identity material and are intentionally absent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum KernelRefV2 {
    Module {
        module_id: ModuleIdV2,
        revision: RevisionDigestV2,
    },
    Schema {
        module_id: ModuleIdV2,
        revision: RevisionDigestV2,
        semantic_key: SemanticKeyV2,
        schema_id: SchemaIdV2,
    },
    ObjectType {
        module_id: ModuleIdV2,
        revision: RevisionDigestV2,
        schema_id: SchemaIdV2,
        semantic_key: SemanticKeyV2,
        object_type_id: ObjectTypeIdV2,
    },
    Relation {
        module_id: ModuleIdV2,
        revision: RevisionDigestV2,
        schema_id: SchemaIdV2,
        semantic_key: SemanticKeyV2,
        relation_id: RelationIdV2,
    },
    Role {
        module_id: ModuleIdV2,
        revision: RevisionDigestV2,
        schema_id: SchemaIdV2,
        relation_id: RelationIdV2,
        semantic_key: SemanticKeyV2,
        role_id: RoleIdV2,
    },
    /// Every category generator, including role projections, subtype
    /// inclusions, aspects, and functions, has one canonical typed address.
    Generator {
        module_id: ModuleIdV2,
        revision: RevisionDigestV2,
        schema_id: SchemaIdV2,
        semantic_key: SemanticKeyV2,
    },
    Theory {
        module_id: ModuleIdV2,
        revision: RevisionDigestV2,
        schema_id: SchemaIdV2,
        semantic_key: SemanticKeyV2,
        theory_id: TheoryIdV2,
    },
    Instance {
        module_id: ModuleIdV2,
        revision: RevisionDigestV2,
        schema_id: SchemaIdV2,
        semantic_key: SemanticKeyV2,
        instance_id: InstanceIdV2,
    },
    Constraint {
        module_id: ModuleIdV2,
        revision: RevisionDigestV2,
        theory_id: TheoryIdV2,
        semantic_key: SemanticKeyV2,
        constraint_id: ConstraintIdV2,
    },
    Equation {
        module_id: ModuleIdV2,
        revision: RevisionDigestV2,
        theory_id: TheoryIdV2,
        semantic_key: SemanticKeyV2,
        equation_id: EquationIdV2,
    },
    RewriteRule {
        module_id: ModuleIdV2,
        revision: RevisionDigestV2,
        theory_id: TheoryIdV2,
        semantic_key: SemanticKeyV2,
        rewrite_rule_id: RewriteRuleIdV2,
    },
    Fact {
        module_id: ModuleIdV2,
        revision: RevisionDigestV2,
        schema_id: SchemaIdV2,
        instance_id: InstanceIdV2,
        relation_id: RelationIdV2,
        fact_id: FactIdV2,
    },
}

impl KernelRefV2 {
    pub fn module_id(&self) -> &ModuleIdV2 {
        match self {
            Self::Module { module_id, .. }
            | Self::Schema { module_id, .. }
            | Self::ObjectType { module_id, .. }
            | Self::Relation { module_id, .. }
            | Self::Role { module_id, .. }
            | Self::Generator { module_id, .. }
            | Self::Theory { module_id, .. }
            | Self::Instance { module_id, .. }
            | Self::Constraint { module_id, .. }
            | Self::Equation { module_id, .. }
            | Self::RewriteRule { module_id, .. }
            | Self::Fact { module_id, .. } => module_id,
        }
    }

    pub fn revision(&self) -> &RevisionDigestV2 {
        match self {
            Self::Module { revision, .. }
            | Self::Schema { revision, .. }
            | Self::ObjectType { revision, .. }
            | Self::Relation { revision, .. }
            | Self::Role { revision, .. }
            | Self::Generator { revision, .. }
            | Self::Theory { revision, .. }
            | Self::Instance { revision, .. }
            | Self::Constraint { revision, .. }
            | Self::Equation { revision, .. }
            | Self::RewriteRule { revision, .. }
            | Self::Fact { revision, .. } => revision,
        }
    }

    pub fn stable_label(&self) -> String {
        match self {
            Self::Module {
                module_id,
                revision,
            } => format!("module:{module_id}:{revision}"),
            Self::Schema { schema_id, .. } => format!("schema:{schema_id}"),
            Self::ObjectType { object_type_id, .. } => format!("object:{object_type_id}"),
            Self::Relation { relation_id, .. } => format!("relation:{relation_id}"),
            Self::Role { role_id, .. } => format!("role:{role_id}"),
            Self::Generator { semantic_key, .. } => format!("generator:{semantic_key}"),
            Self::Theory { theory_id, .. } => format!("theory:{theory_id}"),
            Self::Instance { instance_id, .. } => format!("instance:{instance_id}"),
            Self::Constraint { constraint_id, .. } => format!("constraint:{constraint_id}"),
            Self::Equation { equation_id, .. } => format!("equation:{equation_id}"),
            Self::RewriteRule {
                rewrite_rule_id, ..
            } => format!("rewrite:{rewrite_rule_id}"),
            Self::Fact { fact_id, .. } => format!("fact:{fact_id}"),
        }
    }
}
