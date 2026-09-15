//! Canonical `.axi` schema/theory/instance surface backing `axi_v1`
//!
//! This module defines the single canonical `.axi` authoring surface used by
//! the live example corpus and the Rust-side import/checking path.
//!
//! Internal note:
//! - the module name stays `schema_v1` because it is the schema/theory/instance
//!   AST behind `axi_v1`;
//! - contributors should think in terms of one canonical `.axi` surface, not
//!   multiple end-user dialects.

use nom::{
    branch::alt,
    bytes::complete::{tag, take_while, take_while1},
    character::complete::char as pchar,
    combinator::{all_consuming, opt, recognize},
    multi::separated_list1,
    sequence::preceded,
    IResult, Parser,
};
use serde::{Deserialize, Serialize};
use std::ops::Range;
use thiserror::Error;

pub type Name = String;

pub const MAX_AXI_SOURCE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_AXI_SOURCE_LINES: usize = 200_000;
pub const MAX_AXI_SYNTAX_DEPTH: usize = 64;
pub const MAX_AXI_LINE_BYTES: usize = 1024 * 1024;

// ============================================================================
// AST
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemaV1Module {
    pub module_name: Name,
    /// Ordered imports are part of the exact accepted module closure.
    pub imports: Vec<Name>,
    pub schemas: Vec<SchemaV1Schema>,
    pub theories: Vec<SchemaV1Theory>,
    pub instances: Vec<SchemaV1Instance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemaV1Schema {
    pub name: Name,
    pub objects: Vec<Name>,
    pub subtypes: Vec<SubtypeDeclV1>,
    pub relations: Vec<RelationDeclV1>,
    /// Explicit total arrows in the schema presentation.
    pub generators: Vec<GeneratorDeclV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubtypeDeclV1 {
    pub sub: Name,
    pub sup: Name,
    /// Optional explicit inclusion morphism name.
    ///
    /// This is preserved for now because it still appears in some internal
    /// lowering paths, but it is not part of the preferred canonical authoring
    /// style.
    pub inclusion: Option<Name>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelationDeclV1 {
    pub name: Name,
    pub fields: Vec<FieldDeclV1>,
}

/// Closed, decidable role-type syntax. The compiler resolves names to strict
/// schema-local object or relation identities.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TypeExprV1 {
    Object {
        name: Name,
    },
    RelationObject {
        relation: Name,
    },
    Indexed {
        base: Box<TypeExprV1>,
        over_roles: Vec<Name>,
    },
    Refined {
        base: Box<TypeExprV1>,
        predicates: Vec<RefinementPredicateV1>,
    },
}

impl TypeExprV1 {
    pub fn referenced_name(&self) -> &str {
        match self {
            Self::Object { name } => name,
            Self::RelationObject { relation } => relation,
            Self::Indexed { base, .. } | Self::Refined { base, .. } => base.referenced_name(),
        }
    }

    pub fn relation_object_name(&self) -> Option<&str> {
        match self {
            Self::RelationObject { relation } => Some(relation),
            Self::Indexed { base, .. } | Self::Refined { base, .. } => base.relation_object_name(),
            Self::Object { .. } => None,
        }
    }
}

impl std::fmt::Display for TypeExprV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Object { name } => f.write_str(name),
            Self::RelationObject { relation } => write!(f, "relation({relation})"),
            Self::Indexed { base, over_roles } => {
                write!(f, "indexed({base}; {})", over_roles.join("|"))
            }
            Self::Refined { base, predicates } => {
                let rendered = predicates
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ");
                write!(f, "refined({base}; {rendered})")
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RefinementPredicateV1 {
    Equals { value: Name },
    MemberOf { values: Vec<Name> },
    Cardinality { min: u32, max: u32 },
    Key { roles: Vec<Name> },
    Enum { values: Vec<Name> },
    Predicate { name: Name, args: Vec<Name> },
}

impl std::fmt::Display for RefinementPredicateV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Equals { value } => write!(f, "eq({value})"),
            Self::MemberOf { values } => write!(f, "in({})", values.join("|")),
            Self::Cardinality { min, max } => write!(f, "cardinality({min}|{max})"),
            Self::Key { roles } => write!(f, "key({})", roles.join("|")),
            Self::Enum { values } => write!(f, "enum({})", values.join("|")),
            Self::Predicate { name, args } => {
                if args.is_empty() {
                    write!(f, "predicate({name})")
                } else {
                    write!(f, "predicate({name}|{})", args.join("|"))
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoleKindV1 {
    Data,
    Context,
    World,
    Temporal,
    Parameter,
    Evidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FieldDeclV1 {
    pub field: Name,
    pub ty: TypeExprV1,
    /// Role semantics are explicit syntax, never inferred from names.
    pub kind: RoleKindV1,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GeneratorKindV1 {
    Aspect,
    Function,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneratorDeclV1 {
    pub name: Name,
    pub source: Name,
    pub target: Name,
    pub kind: GeneratorKindV1,
    pub reversible: bool,
}

/// Carrier-field pair for closure-style constraints (symmetric/transitive).
///
/// By default, Axiograph treats the *first two* fields of a relation declaration
/// as the carrier pair. When a relation has extra fields (e.g. context/time or
/// witnesses), authors may want to explicitly name which two fields are the
/// "endpoints" of the closure operation.
///
/// Canonical surface syntax:
/// - `constraint symmetric Rel on (from, to)`
/// - `constraint transitive Rel on (from, to)`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CarrierFieldsV1 {
    pub left_field: Name,
    pub right_field: Name,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemaV1Theory {
    pub name: Name,
    pub schema: Name,
    pub constraints: Vec<ConstraintV1>,
    pub equations: Vec<EquationV1>,
    pub rewrite_rules: Vec<RewriteRuleV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "tag", rename_all = "snake_case")]
pub enum ConstraintV1 {
    Functional {
        relation: Name,
        src_field: Name,
        dst_field: Name,
    },
    /// Bounded fan-out: a source field maps to at most `max` distinct targets.
    ///
    /// Canonical surface syntax:
    /// - `constraint at_most N Rel.src -> Rel.dst`
    /// - `constraint at_most N Rel.src -> Rel.dst param (ctx, time)`
    AtMost {
        relation: Name,
        src_field: Name,
        dst_field: Name,
        max: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        params: Option<Vec<Name>>,
    },
    /// A first-class *typing rule annotation* for a relation.
    ///
    /// Canonical surface syntax:
    /// - `constraint typing Rel: rule_name`
    ///
    /// Notes:
    /// - Axiograph treats these as *typed semantics hints*. A small builtin set
    ///   is certificate-checked via `axi_constraints_ok_v1`; other rule names are
    ///   still parsed/stored for tooling but are not yet executable/certifiable.
    Typing {
        relation: Name,
        rule: Name,
    },
    /// Conditional symmetry: the relation must be symmetric only for tuples
    /// whose `field` value is in `values`.
    ///
    /// Canonical surface syntax:
    /// - `constraint symmetric Rel where Rel.field in {A, B, ...}`
    ///
    /// Notes:
    /// - We intentionally keep the initial guard language small (membership in a
    ///   finite set of constructor-like names) to stay readable and portable
    ///   across Rust/Lean.
    /// - This is part of the initial certifiable constraint subset via
    ///   `axi_constraints_ok_v1` (it checks compatibility under symmetric
    ///   closure; it does not require materializing inverse tuples).
    SymmetricWhereIn {
        relation: Name,
        field: Name,
        values: Vec<Name>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        carriers: Option<CarrierFieldsV1>,
        /// Optional "fiber" parameter fields for closure-style constraints.
        ///
        /// When present, the closure is interpreted as operating on the carrier
        /// pair **within each fixed assignment** of these parameter fields
        /// (e.g. `ctx`, `time`), rather than globally.
        ///
        /// Canonical surface syntax:
        /// - `constraint symmetric Rel where Rel.field in {...} param (ctx, time)`
        #[serde(default, skip_serializing_if = "Option::is_none")]
        params: Option<Vec<Name>>,
    },
    Symmetric {
        relation: Name,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        carriers: Option<CarrierFieldsV1>,
        /// Optional "fiber" parameter fields for closure-style constraints.
        ///
        /// Canonical surface syntax:
        /// - `constraint symmetric Rel param (ctx, time)`
        #[serde(default, skip_serializing_if = "Option::is_none")]
        params: Option<Vec<Name>>,
    },
    Transitive {
        relation: Name,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        carriers: Option<CarrierFieldsV1>,
        /// Optional "fiber" parameter fields for transitive closure.
        ///
        /// When present, transitivity is interpreted as operating on the carrier
        /// pair within each fixed assignment of these parameter fields (e.g.
        /// `ctx`, `time`), rather than globally.
        ///
        /// Canonical surface syntax:
        /// - `constraint transitive Rel param (ctx, time)`
        #[serde(default, skip_serializing_if = "Option::is_none")]
        params: Option<Vec<Name>>,
    },
    Key {
        relation: Name,
        fields: Vec<Name>,
    },
    /// An opaque, named constraint block that is preserved as structured data.
    ///
    /// Canonical surface syntax:
    /// - `constraint Name:` followed by an indented block (stored verbatim as
    ///   trimmed lines).
    ///
    /// These blocks are used by some examples to record richer rules (deontic,
    /// epistemic, query patterns, etc.) before they have an executable /
    /// certifiable semantics.
    NamedBlock {
        name: Name,
        body: Vec<String>,
    },
    Unknown {
        text: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EquationV1 {
    pub name: Name,
    pub lhs: String,
    pub rhs: String,
}

/// Orientation of a rewrite rule.
///
/// For `axi_v1`, rewrite rules are stored as *directed* rules at first.
/// If you want the reverse direction, define a second rule explicitly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum RewriteOrientationV1 {
    #[default]
    Forward,
    Backward,
    Bidirectional,
}

/// Typed variable declarations for rewrite rules.
///
/// We keep this intentionally small and first-order:
/// - object variables range over schema object types, and
/// - path variables range over (start,end) endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RewriteVarDeclV1 {
    pub name: Name,
    pub ty: RewriteVarTypeV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "tag", rename_all = "snake_case")]
pub enum RewriteVarTypeV1 {
    Object { ty: Name },
    Path { from: Name, to: Name },
}

/// Minimal path expression language for rewrite rules and certificates (v3).
///
/// This is the `.axi`-anchored, name-based analogue of `axiograph-pathdb`'s
/// `PathExprV2` (which uses numeric ids).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PathExprV3 {
    /// A path metavariable, used in rewrite rule patterns.
    Var {
        name: Name,
    },
    Reflexive {
        entity: Name,
    },
    Step {
        from: Name,
        rel: Name,
        to: Name,
    },
    Trans {
        left: Box<PathExprV3>,
        right: Box<PathExprV3>,
    },
    Inv {
        path: Box<PathExprV3>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RewriteRuleV1 {
    pub name: Name,
    #[serde(default)]
    pub orientation: RewriteOrientationV1,
    pub vars: Vec<RewriteVarDeclV1>,
    pub lhs: PathExprV3,
    pub rhs: PathExprV3,
}

impl std::fmt::Display for RewriteVarTypeV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RewriteVarTypeV1::Object { ty } => write!(f, "{ty}"),
            RewriteVarTypeV1::Path { from, to } => write!(f, "Path({from},{to})"),
        }
    }
}

impl std::fmt::Display for RewriteVarDeclV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, self.ty)
    }
}

impl std::fmt::Display for PathExprV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathExprV3::Var { name } => write!(f, "{name}"),
            PathExprV3::Reflexive { entity } => write!(f, "refl({entity})"),
            PathExprV3::Step { from, rel, to } => write!(f, "step({from},{rel},{to})"),
            PathExprV3::Trans { left, right } => write!(f, "trans({left},{right})"),
            PathExprV3::Inv { path } => write!(f, "inv({path})"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemaV1Instance {
    pub name: Name,
    pub schema: Name,
    pub assignments: Vec<InstanceAssignmentV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstanceAssignmentV1 {
    pub name: Name,
    pub value: SetLiteralV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetLiteralV1 {
    pub items: Vec<SetItemV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "tag", rename_all = "snake_case")]
pub enum SetItemV1 {
    Ident {
        name: Name,
    },
    /// Optional local labels make relation-valued roles refer to real facts.
    /// Labels are not semantic identities; the compiler replaces references
    /// with the recomputed stable fact id.
    Tuple {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<Name>,
        fields: Vec<(Name, Name)>,
    },
}

// ============================================================================
// Parser
// ============================================================================

#[derive(Debug, Error)]
pub enum SchemaV1ParseError {
    #[error("parse error on line {line}: {message}")]
    Line { line: usize, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    Schema(usize),
    Theory(usize),
    Instance(usize),
}

fn validate_axi_resource_limits(text: &str) -> Result<(), SchemaV1ParseError> {
    if text.len() > MAX_AXI_SOURCE_BYTES {
        return Err(SchemaV1ParseError::Line {
            line: 1,
            message: format!("source exceeds {MAX_AXI_SOURCE_BYTES} bytes"),
        });
    }
    let mut stack = Vec::with_capacity(MAX_AXI_SYNTAX_DEPTH);
    let mut quote = None;
    let mut escaped = false;
    let mut comment = false;
    let mut previous_unquoted_dash = false;
    let mut line = 1_usize;
    let mut line_bytes = 0_usize;
    for byte in text.bytes() {
        if byte == b'\n' {
            if line_bytes > MAX_AXI_LINE_BYTES {
                return Err(SchemaV1ParseError::Line {
                    line,
                    message: format!("line exceeds {MAX_AXI_LINE_BYTES} bytes"),
                });
            }
            line = line.saturating_add(1);
            if line > MAX_AXI_SOURCE_LINES {
                return Err(SchemaV1ParseError::Line {
                    line,
                    message: format!("source exceeds {MAX_AXI_SOURCE_LINES} lines"),
                });
            }
            line_bytes = 0;
            comment = false;
            previous_unquoted_dash = false;
            continue;
        }
        line_bytes = line_bytes.saturating_add(1);
        if comment {
            continue;
        }
        if let Some(active_quote) = quote {
            previous_unquoted_dash = false;
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
            continue;
        }
        if byte == b'-' && previous_unquoted_dash {
            comment = true;
            previous_unquoted_dash = false;
            continue;
        }
        previous_unquoted_dash = byte == b'-';
        match byte {
            b'#' => {
                comment = true;
                previous_unquoted_dash = false;
            }
            b'"' | b'\'' => quote = Some(byte),
            b'(' | b'[' | b'{' => {
                stack.push(byte);
                if stack.len() > MAX_AXI_SYNTAX_DEPTH {
                    return Err(SchemaV1ParseError::Line {
                        line,
                        message: format!(
                            "syntax nesting exceeds {MAX_AXI_SYNTAX_DEPTH} delimiters"
                        ),
                    });
                }
            }
            b')' if stack.pop() != Some(b'(') => {
                return Err(SchemaV1ParseError::Line {
                    line,
                    message: "unbalanced syntax delimiter".to_string(),
                });
            }
            b']' if stack.pop() != Some(b'[') => {
                return Err(SchemaV1ParseError::Line {
                    line,
                    message: "unbalanced syntax delimiter".to_string(),
                });
            }
            b'}' if stack.pop() != Some(b'{') => {
                return Err(SchemaV1ParseError::Line {
                    line,
                    message: "unbalanced syntax delimiter".to_string(),
                });
            }
            _ => {}
        }
    }
    if line_bytes > MAX_AXI_LINE_BYTES {
        return Err(SchemaV1ParseError::Line {
            line,
            message: format!("line exceeds {MAX_AXI_LINE_BYTES} bytes"),
        });
    }
    if quote.is_some() || escaped || !stack.is_empty() {
        return Err(SchemaV1ParseError::Line {
            line,
            message: "unbalanced quoted string or syntax delimiter".to_string(),
        });
    }
    Ok(())
}

/// Operational parser metadata, deliberately separate from the serialized AST.
///
/// Occurrences are emitted by the canonical parser while it consumes the exact
/// input image. They are not serialized, hashed, compiled into kernel IR, or
/// accepted as semantic references.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanonicalSourceMap {
    /// Preserved accepted slice for role-carrier diagnostics.
    pub role_carriers: Vec<RoleCarrierSpan>,
    occurrences: Vec<CanonicalSourceOccurrenceV1>,
}

impl CanonicalSourceMap {
    /// Parser-order occurrences. Their spans are monotone in the original image.
    pub fn occurrences(&self) -> &[CanonicalSourceOccurrenceV1] {
        &self.occurrences
    }

    /// Return one exact occurrence. Missing and unsupported syntax stays absent.
    pub fn occurrence(
        &self,
        address: &CanonicalSyntacticAddressV1,
    ) -> Option<&CanonicalSourceOccurrenceV1> {
        let mut matches = self
            .occurrences
            .iter()
            .filter(|occurrence| &occurrence.address == address);
        let occurrence = matches.next()?;
        matches.next().is_none().then_some(occurrence)
    }

    fn push(&mut self, address: CanonicalSyntacticAddressV1, bytes: Range<usize>) {
        debug_assert!(bytes.start < bytes.end);
        debug_assert!(
            self.occurrences
                .last()
                .is_none_or(|previous| previous.bytes.start <= bytes.start),
            "canonical source occurrences must be emitted in source order"
        );
        self.occurrences
            .push(CanonicalSourceOccurrenceV1 { address, bytes });
    }
}

/// One typed syntactic occurrence and its exact half-open UTF-8 byte span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalSourceOccurrenceV1 {
    pub address: CanonicalSyntacticAddressV1,
    pub bytes: Range<usize>,
}

/// A path to a nested role type. The empty path addresses the complete role
/// type; each step selects the recursively nested base of a wrapper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RoleTypePathStepV1 {
    IndexedBase,
    RefinedBase,
}

/// A leaf field inside one parsed refinement predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RefinementTermV1 {
    EqualsValue,
    MemberValue { value_index: usize },
    CardinalityMin,
    CardinalityMax,
    KeyRole { role_index: usize },
    EnumValue { value_index: usize },
    PredicateName,
    PredicateArgument { argument_index: usize },
}

/// A leaf field inside one supported constraint. Occurrence indices distinguish
/// repeated relation spellings in forms such as `R.a -> R.b`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConstraintTermV1 {
    Bound,
    Relation { occurrence_index: usize },
    SourceField,
    TargetField,
    Rule,
    GuardField,
    GuardValue { value_index: usize },
    CarrierField { carrier_index: usize },
    Parameter { parameter_index: usize },
    KeyField { field_index: usize },
}

/// A path to a recursively nested rewrite path expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RewritePathStepV1 {
    TransLeft,
    TransRight,
    InvPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RewriteSideV1 {
    Left,
    Right,
}

/// A leaf field inside one rewrite path-expression constructor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RewritePathTermV1 {
    Variable,
    ReflexiveEntity,
    StepFrom,
    StepRelation,
    StepTo,
}

/// Typed parser occurrence addresses. Indices are syntactic declaration order,
/// not checked `KernelRefV2` identities.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CanonicalSyntacticAddressV1 {
    ModuleDeclaration,
    ModuleName,
    ImportDeclaration {
        import_index: usize,
    },
    ImportName {
        import_index: usize,
    },
    SchemaDeclaration {
        schema_index: usize,
    },
    SchemaName {
        schema_index: usize,
    },
    ObjectDeclaration {
        schema_index: usize,
        object_index: usize,
    },
    ObjectName {
        schema_index: usize,
        object_index: usize,
    },
    SubtypeDeclaration {
        schema_index: usize,
        subtype_index: usize,
    },
    SubtypeSub {
        schema_index: usize,
        subtype_index: usize,
    },
    SubtypeSuper {
        schema_index: usize,
        subtype_index: usize,
    },
    SubtypeInclusion {
        schema_index: usize,
        subtype_index: usize,
    },
    RelationDeclaration {
        schema_index: usize,
        relation_index: usize,
    },
    RelationName {
        schema_index: usize,
        relation_index: usize,
    },
    RoleDeclaration {
        schema_index: usize,
        relation_index: usize,
        role_index: usize,
    },
    RoleName {
        schema_index: usize,
        relation_index: usize,
        role_index: usize,
    },
    RoleType {
        schema_index: usize,
        relation_index: usize,
        role_index: usize,
        path: Vec<RoleTypePathStepV1>,
    },
    RoleTypeCarrier {
        schema_index: usize,
        relation_index: usize,
        role_index: usize,
    },
    RoleIndexedOver {
        schema_index: usize,
        relation_index: usize,
        role_index: usize,
        path: Vec<RoleTypePathStepV1>,
        over_role_index: usize,
    },
    RoleRefinementPredicate {
        schema_index: usize,
        relation_index: usize,
        role_index: usize,
        path: Vec<RoleTypePathStepV1>,
        predicate_index: usize,
    },
    RoleRefinementTerm {
        schema_index: usize,
        relation_index: usize,
        role_index: usize,
        path: Vec<RoleTypePathStepV1>,
        predicate_index: usize,
        term: RefinementTermV1,
    },
    RoleKind {
        schema_index: usize,
        relation_index: usize,
        role_index: usize,
    },
    GeneratorDeclaration {
        schema_index: usize,
        generator_index: usize,
    },
    GeneratorName {
        schema_index: usize,
        generator_index: usize,
    },
    GeneratorSource {
        schema_index: usize,
        generator_index: usize,
    },
    GeneratorTarget {
        schema_index: usize,
        generator_index: usize,
    },
    GeneratorKind {
        schema_index: usize,
        generator_index: usize,
    },
    GeneratorReversible {
        schema_index: usize,
        generator_index: usize,
    },
    TheoryDeclaration {
        theory_index: usize,
    },
    TheoryName {
        theory_index: usize,
    },
    TheorySchema {
        theory_index: usize,
    },
    ConstraintDeclaration {
        theory_index: usize,
        constraint_index: usize,
    },
    ConstraintTerm {
        theory_index: usize,
        constraint_index: usize,
        term: ConstraintTermV1,
    },
    NamedConstraintName {
        theory_index: usize,
        constraint_index: usize,
    },
    NamedConstraintBodyLine {
        theory_index: usize,
        constraint_index: usize,
        body_index: usize,
    },
    EquationDeclaration {
        theory_index: usize,
        equation_index: usize,
    },
    EquationName {
        theory_index: usize,
        equation_index: usize,
    },
    EquationLeft {
        theory_index: usize,
        equation_index: usize,
    },
    EquationRight {
        theory_index: usize,
        equation_index: usize,
    },
    RewriteDeclaration {
        theory_index: usize,
        rewrite_index: usize,
    },
    RewriteName {
        theory_index: usize,
        rewrite_index: usize,
    },
    RewriteVariables {
        theory_index: usize,
        rewrite_index: usize,
    },
    RewriteLeft {
        theory_index: usize,
        rewrite_index: usize,
    },
    RewriteRight {
        theory_index: usize,
        rewrite_index: usize,
    },
    RewriteOrientation {
        theory_index: usize,
        rewrite_index: usize,
        occurrence_index: usize,
    },
    RewriteVariableDeclaration {
        theory_index: usize,
        rewrite_index: usize,
        variable_index: usize,
    },
    RewriteVariableName {
        theory_index: usize,
        rewrite_index: usize,
        variable_index: usize,
    },
    RewriteVariableType {
        theory_index: usize,
        rewrite_index: usize,
        variable_index: usize,
    },
    RewriteVariableObjectType {
        theory_index: usize,
        rewrite_index: usize,
        variable_index: usize,
    },
    RewriteVariablePathFrom {
        theory_index: usize,
        rewrite_index: usize,
        variable_index: usize,
    },
    RewriteVariablePathTo {
        theory_index: usize,
        rewrite_index: usize,
        variable_index: usize,
    },
    RewritePathNode {
        theory_index: usize,
        rewrite_index: usize,
        side: RewriteSideV1,
        path: Vec<RewritePathStepV1>,
    },
    RewritePathTerm {
        theory_index: usize,
        rewrite_index: usize,
        side: RewriteSideV1,
        path: Vec<RewritePathStepV1>,
        term: RewritePathTermV1,
    },
    InstanceDeclaration {
        instance_index: usize,
    },
    InstanceName {
        instance_index: usize,
    },
    InstanceSchema {
        instance_index: usize,
    },
    AssignmentDeclaration {
        instance_index: usize,
        assignment_index: usize,
    },
    AssignmentName {
        instance_index: usize,
        assignment_index: usize,
    },
    SetLiteral {
        instance_index: usize,
        assignment_index: usize,
    },
    SetItem {
        instance_index: usize,
        assignment_index: usize,
        item_index: usize,
    },
    SetIdentName {
        instance_index: usize,
        assignment_index: usize,
        item_index: usize,
    },
    SetTupleLabel {
        instance_index: usize,
        assignment_index: usize,
        item_index: usize,
    },
    SetTupleFieldRole {
        instance_index: usize,
        assignment_index: usize,
        item_index: usize,
        field_index: usize,
    },
    SetTupleFieldValue {
        instance_index: usize,
        assignment_index: usize,
        item_index: usize,
        field_index: usize,
    },
}

/// Declaration occurrence indices and a half-open UTF-8 byte range in the exact input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleCarrierSpan {
    pub schema_index: usize,
    pub relation_index: usize,
    pub role_index: usize,
    pub bytes: Range<usize>,
}

pub fn parse_schema_v1(text: &str) -> Result<SchemaV1Module, SchemaV1ParseError> {
    parse_schema_v1_with_source_map(text).map(|(module, _)| module)
}

pub fn parse_schema_v1_with_source_map(
    text: &str,
) -> Result<(SchemaV1Module, CanonicalSourceMap), SchemaV1ParseError> {
    validate_axi_resource_limits(text)?;
    let mut source_map = CanonicalSourceMap::default();
    let mut module = SchemaV1Module {
        module_name: "Unnamed".to_string(),
        imports: vec![],
        schemas: vec![],
        theories: vec![],
        instances: vec![],
    };

    let mut section = Section::None;
    let mut module_header_line = None;
    let lines: Vec<&str> = text.lines().collect();

    let mut i = 0usize;
    while i < lines.len() {
        let line_no = i + 1;
        let raw = lines[i];
        let line = strip_comment(raw).trim_axi();
        if line.is_empty() {
            i += 1;
            continue;
        }

        // ------------------------------------------------------------------
        // Section headers
        // ------------------------------------------------------------------
        if let Some(rest) = line.strip_prefix("module ").map(trim_axi) {
            let name = parse_module_header(rest).map_err(|message| SchemaV1ParseError::Line {
                line: line_no,
                message,
            })?;
            if let Some(first_line) = module_header_line {
                return Err(SchemaV1ParseError::Line {
                    line: line_no,
                    message: format!(
                        "canonical .axi input requires exactly one module header; first header was on line {first_line}"
                    ),
                });
            }
            let segments = line_segments(text, line);
            source_map.push(
                CanonicalSyntacticAddressV1::ModuleDeclaration,
                original_range(&segments, 0..line.len()).expect("line source range"),
            );
            source_map.push(
                CanonicalSyntacticAddressV1::ModuleName,
                original_range(&segments, relative_range(line, rest)).expect("module name range"),
            );
            if !module.schemas.is_empty()
                || !module.theories.is_empty()
                || !module.instances.is_empty()
            {
                return Err(SchemaV1ParseError::Line {
                    line: line_no,
                    message: "the module header must be the first canonical header".to_string(),
                });
            }
            module_header_line = Some(line_no);
            module.module_name = name;
            section = Section::None;
            i += 1;
            continue;
        }

        if module_header_line.is_none() {
            return Err(SchemaV1ParseError::Line {
                line: line_no,
                message: "the module header must be the first canonical header".to_string(),
            });
        }

        if let Some(rest) = line.strip_prefix("import ").map(trim_axi) {
            if module_header_line.is_none() || section != Section::None {
                return Err(SchemaV1ParseError::Line {
                    line: line_no,
                    message: "imports must follow the module header and precede all schema/theory/instance sections".to_string(),
                });
            }
            let import = parse_module_header(rest).map_err(|message| SchemaV1ParseError::Line {
                line: line_no,
                message: format!("import expects exactly `import <Module>`: {message}"),
            })?;
            if module.imports.contains(&import) {
                return Err(SchemaV1ParseError::Line {
                    line: line_no,
                    message: format!("duplicate import `{import}`"),
                });
            }
            let import_index = module.imports.len();
            let segments = line_segments(text, line);
            source_map.push(
                CanonicalSyntacticAddressV1::ImportDeclaration { import_index },
                original_range(&segments, 0..line.len()).expect("line source range"),
            );
            source_map.push(
                CanonicalSyntacticAddressV1::ImportName { import_index },
                original_range(&segments, relative_range(line, rest)).expect("import name range"),
            );
            module.imports.push(import);
            i += 1;
            continue;
        }

        if let Some(rest) = line.strip_prefix("schema ").map(trim_axi) {
            let name = parse_schema_header(rest).map_err(|message| SchemaV1ParseError::Line {
                line: line_no,
                message,
            })?;
            let schema_index = module.schemas.len();
            let segments = line_segments(text, line);
            source_map.push(
                CanonicalSyntacticAddressV1::SchemaDeclaration { schema_index },
                original_range(&segments, 0..line.len()).expect("line source range"),
            );
            source_map.push(
                CanonicalSyntacticAddressV1::SchemaName { schema_index },
                original_range(&segments, relative_range(line, name)).expect("schema name range"),
            );
            module.schemas.push(SchemaV1Schema {
                name: name.to_string(),
                objects: vec![],
                subtypes: vec![],
                relations: vec![],
                generators: vec![],
            });
            section = Section::Schema(schema_index);
            i += 1;
            continue;
        }

        if let Some(rest) = line.strip_prefix("theory ").map(trim_axi) {
            let (name, schema, name_range, schema_range) = parse_theory_header_with_source(rest)
                .map_err(|message| SchemaV1ParseError::Line {
                    line: line_no,
                    message,
                })?;
            let theory_index = module.theories.len();
            let rest_start = relative_range(line, rest).start;
            let segments = line_segments(text, line);
            source_map.push(
                CanonicalSyntacticAddressV1::TheoryDeclaration { theory_index },
                original_range(&segments, 0..line.len()).expect("line source range"),
            );
            source_map.push(
                CanonicalSyntacticAddressV1::TheoryName { theory_index },
                original_range(
                    &segments,
                    (rest_start + name_range.start)..(rest_start + name_range.end),
                )
                .expect("theory name range"),
            );
            source_map.push(
                CanonicalSyntacticAddressV1::TheorySchema { theory_index },
                original_range(
                    &segments,
                    (rest_start + schema_range.start)..(rest_start + schema_range.end),
                )
                .expect("theory schema range"),
            );
            module.theories.push(SchemaV1Theory {
                name,
                schema,
                constraints: vec![],
                equations: vec![],
                rewrite_rules: vec![],
            });
            section = Section::Theory(theory_index);
            i += 1;
            continue;
        }

        if let Some(rest) = line.strip_prefix("instance ").map(trim_axi) {
            let (name, schema, name_range, schema_range) = parse_instance_header_with_source(rest)
                .map_err(|message| SchemaV1ParseError::Line {
                    line: line_no,
                    message,
                })?;
            let instance_index = module.instances.len();
            let rest_start = relative_range(line, rest).start;
            let segments = line_segments(text, line);
            source_map.push(
                CanonicalSyntacticAddressV1::InstanceDeclaration { instance_index },
                original_range(&segments, 0..line.len()).expect("line source range"),
            );
            source_map.push(
                CanonicalSyntacticAddressV1::InstanceName { instance_index },
                original_range(
                    &segments,
                    (rest_start + name_range.start)..(rest_start + name_range.end),
                )
                .expect("instance name range"),
            );
            source_map.push(
                CanonicalSyntacticAddressV1::InstanceSchema { instance_index },
                original_range(
                    &segments,
                    (rest_start + schema_range.start)..(rest_start + schema_range.end),
                )
                .expect("instance schema range"),
            );
            module.instances.push(SchemaV1Instance {
                name,
                schema,
                assignments: vec![],
            });
            section = Section::Instance(instance_index);
            i += 1;
            continue;
        }

        // ------------------------------------------------------------------
        // Section bodies
        // ------------------------------------------------------------------
        match section {
            Section::Schema(schema_index) => {
                if let Some(name) = line.strip_prefix("object ") {
                    parse_identifier_text(name, "object name").map_err(|message| {
                        SchemaV1ParseError::Line {
                            line: line_no,
                            message,
                        }
                    })?;
                    let object_index = module.schemas[schema_index].objects.len();
                    let segments = line_segments(text, line);
                    source_map.push(
                        CanonicalSyntacticAddressV1::ObjectDeclaration {
                            schema_index,
                            object_index,
                        },
                        original_range(&segments, 0..line.len()).expect("line source range"),
                    );
                    source_map.push(
                        CanonicalSyntacticAddressV1::ObjectName {
                            schema_index,
                            object_index,
                        },
                        original_range(&segments, relative_range(line, name))
                            .expect("object name range"),
                    );
                    module.schemas[schema_index].objects.push(name.to_string());
                    i += 1;
                    continue;
                }

                if let Some(rest) = line.strip_prefix("subtype ").map(trim_axi) {
                    let (subtype, metadata) =
                        parse_subtype_decl_with_source(rest).map_err(|message| {
                            SchemaV1ParseError::Line {
                                line: line_no,
                                message,
                            }
                        })?;
                    let subtype_index = module.schemas[schema_index].subtypes.len();
                    let rest_start = relative_range(line, rest).start;
                    let segments = line_segments(text, line);
                    let map_rest = |range: Range<usize>| {
                        original_range(
                            &segments,
                            (rest_start + range.start)..(rest_start + range.end),
                        )
                        .expect("subtype source range")
                    };
                    source_map.push(
                        CanonicalSyntacticAddressV1::SubtypeDeclaration {
                            schema_index,
                            subtype_index,
                        },
                        original_range(&segments, 0..line.len()).expect("line source range"),
                    );
                    source_map.push(
                        CanonicalSyntacticAddressV1::SubtypeSub {
                            schema_index,
                            subtype_index,
                        },
                        map_rest(metadata.sub),
                    );
                    source_map.push(
                        CanonicalSyntacticAddressV1::SubtypeSuper {
                            schema_index,
                            subtype_index,
                        },
                        map_rest(metadata.sup),
                    );
                    if let Some(inclusion) = metadata.inclusion {
                        source_map.push(
                            CanonicalSyntacticAddressV1::SubtypeInclusion {
                                schema_index,
                                subtype_index,
                            },
                            map_rest(inclusion),
                        );
                    }
                    module.schemas[schema_index].subtypes.push(subtype);
                    i += 1;
                    continue;
                }

                if line.starts_with("relation ") {
                    let (combined, next_index, segments) =
                        collect_balanced_parens(text, lines.as_slice(), i, "relation").map_err(
                            |message| SchemaV1ParseError::Line {
                                line: line_no,
                                message,
                            },
                        )?;
                    let (relation, metadata) =
                        parse_relation_decl_with_source(&combined).map_err(|message| {
                            SchemaV1ParseError::Line {
                                line: line_no,
                                message,
                            }
                        })?;
                    let relation_index = module.schemas[schema_index].relations.len();

                    // Carrier and segment offsets are ordered. The cursor advances at most
                    // segments.len() times across ALL carriers: O(carriers + segments), not
                    // a fresh scan per role. Synthetic join spaces have no source origin.
                    let mut segment_index = 0;
                    for (role_index, field) in metadata.fields.iter().enumerate() {
                        let start = field.ty.carrier.start;
                        let end = field.ty.carrier.end;
                        while segments
                            .get(segment_index)
                            .is_some_and(|(range, _)| range.end <= start)
                        {
                            segment_index += 1;
                        }
                        if let Some((range, origin)) = segments
                            .get(segment_index)
                            .filter(|(range, _)| range.start <= start && end <= range.end)
                        {
                            source_map.role_carriers.push(RoleCarrierSpan {
                                schema_index,
                                relation_index,
                                role_index,
                                bytes: (origin + start - range.start)..(origin + end - range.start),
                            });
                        }
                    }

                    let mut events = vec![
                        (
                            CanonicalSyntacticAddressV1::RelationDeclaration {
                                schema_index,
                                relation_index,
                            },
                            0..combined.len(),
                        ),
                        (
                            CanonicalSyntacticAddressV1::RelationName {
                                schema_index,
                                relation_index,
                            },
                            metadata.name,
                        ),
                    ];
                    for (role_index, field) in metadata.fields.into_iter().enumerate() {
                        events.push((
                            CanonicalSyntacticAddressV1::RoleDeclaration {
                                schema_index,
                                relation_index,
                                role_index,
                            },
                            field.declaration,
                        ));
                        events.push((
                            CanonicalSyntacticAddressV1::RoleName {
                                schema_index,
                                relation_index,
                                role_index,
                            },
                            field.name,
                        ));
                        for (path, range) in field.ty.nodes {
                            events.push((
                                CanonicalSyntacticAddressV1::RoleType {
                                    schema_index,
                                    relation_index,
                                    role_index,
                                    path,
                                },
                                range,
                            ));
                        }
                        events.push((
                            CanonicalSyntacticAddressV1::RoleTypeCarrier {
                                schema_index,
                                relation_index,
                                role_index,
                            },
                            field.ty.carrier,
                        ));
                        for (path, over_role_index, range) in field.ty.indexed_roles {
                            events.push((
                                CanonicalSyntacticAddressV1::RoleIndexedOver {
                                    schema_index,
                                    relation_index,
                                    role_index,
                                    path,
                                    over_role_index,
                                },
                                range,
                            ));
                        }
                        for (path, predicate_index, predicate) in field.ty.predicates {
                            events.push((
                                CanonicalSyntacticAddressV1::RoleRefinementPredicate {
                                    schema_index,
                                    relation_index,
                                    role_index,
                                    path: path.clone(),
                                    predicate_index,
                                },
                                predicate.declaration,
                            ));
                            for (term, range) in predicate.terms {
                                events.push((
                                    CanonicalSyntacticAddressV1::RoleRefinementTerm {
                                        schema_index,
                                        relation_index,
                                        role_index,
                                        path: path.clone(),
                                        predicate_index,
                                        term,
                                    },
                                    range,
                                ));
                            }
                        }
                        if let Some(range) = field.kind {
                            events.push((
                                CanonicalSyntacticAddressV1::RoleKind {
                                    schema_index,
                                    relation_index,
                                    role_index,
                                },
                                range,
                            ));
                        }
                    }
                    // O(E log E + E log S): one bounded sort plus two binary
                    // segment lookups per emitted event. E is bounded by parsed
                    // role/type nodes and S by physical source lines.
                    let mut mapped = events
                        .into_iter()
                        .filter_map(|(address, range)| {
                            original_range(&segments, range).map(|bytes| (address, bytes))
                        })
                        .collect::<Vec<_>>();
                    mapped.sort_by(|left, right| {
                        left.1
                            .start
                            .cmp(&right.1.start)
                            .then_with(|| right.1.end.cmp(&left.1.end))
                    });
                    for (address, bytes) in mapped {
                        source_map.push(address, bytes);
                    }
                    module.schemas[schema_index].relations.push(relation);
                    i = next_index;
                    continue;
                }

                if line.starts_with("aspect ") || line.starts_with("function ") {
                    let (generator, metadata) =
                        parse_generator_decl_with_source(line).map_err(|message| {
                            SchemaV1ParseError::Line {
                                line: line_no,
                                message,
                            }
                        })?;
                    let generator_index = module.schemas[schema_index].generators.len();
                    let segments = line_segments(text, line);
                    source_map.push(
                        CanonicalSyntacticAddressV1::GeneratorDeclaration {
                            schema_index,
                            generator_index,
                        },
                        original_range(&segments, 0..line.len()).expect("line source range"),
                    );
                    let mut generator_events = vec![
                        (
                            CanonicalSyntacticAddressV1::GeneratorName {
                                schema_index,
                                generator_index,
                            },
                            metadata.name,
                        ),
                        (
                            CanonicalSyntacticAddressV1::GeneratorSource {
                                schema_index,
                                generator_index,
                            },
                            metadata.source,
                        ),
                        (
                            CanonicalSyntacticAddressV1::GeneratorTarget {
                                schema_index,
                                generator_index,
                            },
                            metadata.target,
                        ),
                        (
                            CanonicalSyntacticAddressV1::GeneratorKind {
                                schema_index,
                                generator_index,
                            },
                            metadata.kind,
                        ),
                    ];
                    generator_events.sort_by_key(|(_, range)| range.start);
                    for (address, range) in generator_events {
                        source_map.push(
                            address,
                            original_range(&segments, range).expect("generator source range"),
                        );
                    }
                    if let Some(range) = metadata.reversible {
                        source_map.push(
                            CanonicalSyntacticAddressV1::GeneratorReversible {
                                schema_index,
                                generator_index,
                            },
                            original_range(&segments, range)
                                .expect("generator reversible source range"),
                        );
                    }
                    module.schemas[schema_index].generators.push(generator);
                    i += 1;
                    continue;
                }

                return Err(SchemaV1ParseError::Line {
                    line: line_no,
                    message: format!("unrecognized schema line: {line}"),
                });
            }
            Section::Theory(theory_index) => {
                if let Some(rest) = line.strip_prefix("constraint ").map(trim_axi) {
                    let rest = rest.trim_axi();
                    let recognized_family = [
                        "functional",
                        "at_most",
                        "typing",
                        "symmetric",
                        "transitive",
                        "key",
                    ]
                    .iter()
                    .any(|family| starts_with_syntax_keyword(rest, family));
                    let named_header_candidate = !recognized_family
                        && (rest.contains(':') || !rest.chars().any(is_ascii_syntax_whitespace));
                    // Named constraints use the same all-consuming optional-single-colon
                    // name grammar as equations and rewrites.
                    if named_header_candidate {
                        let name = parse_optional_colon_name_header(rest, "constraint").map_err(
                            |message| SchemaV1ParseError::Line {
                                line: line_no,
                                message,
                            },
                        )?;
                        let (body, next_index) =
                            collect_indented_block_lines(lines.as_slice(), i + 1);
                        let constraint_index = module.theories[theory_index].constraints.len();
                        source_map.push(
                            CanonicalSyntacticAddressV1::ConstraintDeclaration {
                                theory_index,
                                constraint_index,
                            },
                            consumed_source_range(text, line, lines.as_slice(), i, next_index),
                        );
                        let segments = line_segments(text, line);
                        source_map.push(
                            CanonicalSyntacticAddressV1::NamedConstraintName {
                                theory_index,
                                constraint_index,
                            },
                            original_range(&segments, relative_range(line, name))
                                .expect("named constraint range"),
                        );
                        for (body_index, body_line) in lines[i + 1..next_index]
                            .iter()
                            .map(|raw| strip_comment(raw).trim_axi())
                            .filter(|body_line| !body_line.is_empty())
                            .enumerate()
                        {
                            source_map.push(
                                CanonicalSyntacticAddressV1::NamedConstraintBodyLine {
                                    theory_index,
                                    constraint_index,
                                    body_index,
                                },
                                (body_line.as_ptr() as usize - text.as_ptr() as usize)
                                    ..(body_line.as_ptr() as usize - text.as_ptr() as usize
                                        + body_line.len()),
                            );
                        }
                        module.theories[theory_index]
                            .constraints
                            .push(ConstraintV1::NamedBlock {
                                name: name.to_string(),
                                body,
                            });
                        i = next_index;
                        continue;
                    }

                    // Support multi-line constraint blocks (e.g. `... where` followed
                    // by a few lines). We join the block and try to parse it as a
                    // known constraint; otherwise we preserve the text as `Unknown`
                    // so examples can record richer (not-yet-executable) constraints
                    // without failing parsing of the whole module.
                    let (combined, consumed_index, segments) =
                        collect_constraint_with_segments(text, lines.as_slice(), i + 1, rest);

                    let constraint = parse_constraint(&combined).map_err(|message| {
                        SchemaV1ParseError::Line {
                            line: line_no,
                            message,
                        }
                    })?;
                    let constraint_index = module.theories[theory_index].constraints.len();
                    if !matches!(constraint, ConstraintV1::Unknown { .. }) {
                        source_map.push(
                            CanonicalSyntacticAddressV1::ConstraintDeclaration {
                                theory_index,
                                constraint_index,
                            },
                            consumed_source_range(text, line, lines.as_slice(), i, consumed_index),
                        );
                        if let Some(metadata) = constraint_source_metadata(&combined, &constraint) {
                            let mut terms = metadata
                                .terms
                                .into_iter()
                                .filter_map(|(term, range)| {
                                    original_range(&segments, range).map(|bytes| (term, bytes))
                                })
                                .collect::<Vec<_>>();
                            terms.sort_by_key(|(_, bytes)| bytes.start);
                            for (term, bytes) in terms {
                                source_map.push(
                                    CanonicalSyntacticAddressV1::ConstraintTerm {
                                        theory_index,
                                        constraint_index,
                                        term,
                                    },
                                    bytes,
                                );
                            }
                        }
                    }
                    module.theories[theory_index].constraints.push(constraint);
                    i = consumed_index;
                    continue;
                }

                if let Some(rest) = line.strip_prefix("equation ").map(trim_axi) {
                    let equation_name = parse_optional_colon_name_header(rest, "equation")
                        .map_err(|message| SchemaV1ParseError::Line {
                            line: line_no,
                            message,
                        })?;

                    let (equation_text, next_index, segments) =
                        collect_indented_block_with_segments(text, lines.as_slice(), i + 1);
                    let (lhs_text, rhs_text) =
                        split_equation_slices(&equation_text).map_err(|message| {
                            SchemaV1ParseError::Line {
                                line: line_no,
                                message,
                            }
                        })?;
                    let equation_index = module.theories[theory_index].equations.len();
                    source_map.push(
                        CanonicalSyntacticAddressV1::EquationDeclaration {
                            theory_index,
                            equation_index,
                        },
                        consumed_source_range(text, line, lines.as_slice(), i, next_index),
                    );
                    let line_source = line_segments(text, line);
                    source_map.push(
                        CanonicalSyntacticAddressV1::EquationName {
                            theory_index,
                            equation_index,
                        },
                        original_range(&line_source, relative_range(line, equation_name))
                            .expect("equation name range"),
                    );
                    source_map.push(
                        CanonicalSyntacticAddressV1::EquationLeft {
                            theory_index,
                            equation_index,
                        },
                        original_range(&segments, relative_range(&equation_text, lhs_text))
                            .expect("equation left range"),
                    );
                    source_map.push(
                        CanonicalSyntacticAddressV1::EquationRight {
                            theory_index,
                            equation_index,
                        },
                        original_range(&segments, relative_range(&equation_text, rhs_text))
                            .expect("equation right range"),
                    );

                    module.theories[theory_index].equations.push(EquationV1 {
                        name: equation_name.to_string(),
                        lhs: lhs_text.to_string(),
                        rhs: rhs_text.to_string(),
                    });

                    i = next_index;
                    continue;
                }

                if let Some(rest) = line.strip_prefix("rewrite ").map(trim_axi) {
                    let rule_name =
                        parse_optional_colon_name_header(rest, "rewrite").map_err(|message| {
                            SchemaV1ParseError::Line {
                                line: line_no,
                                message,
                            }
                        })?;

                    let (block_lines, next_index) =
                        collect_indented_block_lines(lines.as_slice(), i + 1);
                    let rule = parse_rewrite_rule(rule_name, &block_lines).map_err(|message| {
                        SchemaV1ParseError::Line {
                            line: line_no,
                            message,
                        }
                    })?;
                    let rewrite_index = module.theories[theory_index].rewrite_rules.len();
                    source_map.push(
                        CanonicalSyntacticAddressV1::RewriteDeclaration {
                            theory_index,
                            rewrite_index,
                        },
                        consumed_source_range(text, line, lines.as_slice(), i, next_index),
                    );
                    let line_source = line_segments(text, line);
                    source_map.push(
                        CanonicalSyntacticAddressV1::RewriteName {
                            theory_index,
                            rewrite_index,
                        },
                        original_range(&line_source, relative_range(line, rule_name))
                            .expect("rewrite name range"),
                    );
                    let inputs =
                        collect_rewrite_source_inputs(text, lines.as_slice(), i + 1, next_index);
                    let mut events = Vec::new();
                    if let Some(bytes) = inputs.all_vars.original_span() {
                        events.push((
                            CanonicalSyntacticAddressV1::RewriteVariables {
                                theory_index,
                                rewrite_index,
                            },
                            bytes,
                        ));
                    }
                    let mut variable_index = 0;
                    for input in &inputs.vars {
                        let (variables, metadata) =
                            parse_rewrite_var_decl_list_with_source(&input.text)
                                .expect("rewrite variables already parsed");
                        for (variable, metadata) in variables.iter().zip(metadata) {
                            debug_assert_eq!(rule.vars.get(variable_index), Some(variable));
                            for (address, range) in [
                                (
                                    CanonicalSyntacticAddressV1::RewriteVariableDeclaration {
                                        theory_index,
                                        rewrite_index,
                                        variable_index,
                                    },
                                    Some(metadata.declaration),
                                ),
                                (
                                    CanonicalSyntacticAddressV1::RewriteVariableName {
                                        theory_index,
                                        rewrite_index,
                                        variable_index,
                                    },
                                    Some(metadata.name),
                                ),
                                (
                                    CanonicalSyntacticAddressV1::RewriteVariableType {
                                        theory_index,
                                        rewrite_index,
                                        variable_index,
                                    },
                                    Some(metadata.ty),
                                ),
                                (
                                    CanonicalSyntacticAddressV1::RewriteVariableObjectType {
                                        theory_index,
                                        rewrite_index,
                                        variable_index,
                                    },
                                    metadata.object_type,
                                ),
                                (
                                    CanonicalSyntacticAddressV1::RewriteVariablePathFrom {
                                        theory_index,
                                        rewrite_index,
                                        variable_index,
                                    },
                                    metadata.path_from,
                                ),
                                (
                                    CanonicalSyntacticAddressV1::RewriteVariablePathTo {
                                        theory_index,
                                        rewrite_index,
                                        variable_index,
                                    },
                                    metadata.path_to,
                                ),
                            ] {
                                if let Some(bytes) =
                                    range.and_then(|range| original_range(&input.segments, range))
                                {
                                    events.push((address, bytes));
                                }
                            }
                            variable_index += 1;
                        }
                    }
                    debug_assert_eq!(variable_index, rule.vars.len());

                    for (side, input, aggregate, expected) in [
                        (
                            RewriteSideV1::Left,
                            &inputs.lhs,
                            CanonicalSyntacticAddressV1::RewriteLeft {
                                theory_index,
                                rewrite_index,
                            },
                            &rule.lhs,
                        ),
                        (
                            RewriteSideV1::Right,
                            &inputs.rhs,
                            CanonicalSyntacticAddressV1::RewriteRight {
                                theory_index,
                                rewrite_index,
                            },
                            &rule.rhs,
                        ),
                    ] {
                        if let Some(bytes) = input.original_span() {
                            events.push((aggregate, bytes));
                        }
                        let (parsed, metadata) = parse_path_expr_v3_with_source(&input.text)
                            .expect("rewrite path already parsed");
                        debug_assert_eq!(&parsed, expected);
                        for (path, range) in metadata.nodes {
                            if let Some(bytes) = original_range(&input.segments, range) {
                                events.push((
                                    CanonicalSyntacticAddressV1::RewritePathNode {
                                        theory_index,
                                        rewrite_index,
                                        side,
                                        path,
                                    },
                                    bytes,
                                ));
                            }
                        }
                        for (path, term, range) in metadata.terms {
                            if let Some(bytes) = original_range(&input.segments, range) {
                                events.push((
                                    CanonicalSyntacticAddressV1::RewritePathTerm {
                                        theory_index,
                                        rewrite_index,
                                        side,
                                        path,
                                        term,
                                    },
                                    bytes,
                                ));
                            }
                        }
                    }
                    for (occurrence_index, orientation) in inputs.orientations.iter().enumerate() {
                        if let Some(bytes) = orientation.original_span() {
                            events.push((
                                CanonicalSyntacticAddressV1::RewriteOrientation {
                                    theory_index,
                                    rewrite_index,
                                    occurrence_index,
                                },
                                bytes,
                            ));
                        }
                    }
                    events.sort_by(|left, right| {
                        left.1
                            .start
                            .cmp(&right.1.start)
                            .then_with(|| right.1.end.cmp(&left.1.end))
                    });
                    for (address, bytes) in events {
                        source_map.push(address, bytes);
                    }
                    module.theories[theory_index].rewrite_rules.push(rule);

                    i = next_index;
                    continue;
                }

                return Err(SchemaV1ParseError::Line {
                    line: line_no,
                    message: format!("unrecognized theory line: {line}"),
                });
            }
            Section::Instance(instance_index) => {
                if let Some((lhs, rhs)) = split_assignment(line) {
                    parse_identifier_text(lhs, "assignment name").map_err(|message| {
                        SchemaV1ParseError::Line {
                            line: line_no,
                            message,
                        }
                    })?;
                    let (set_text, next_index, segments) =
                        collect_balanced_braces(text, lines.as_slice(), i, rhs).map_err(
                            |message| SchemaV1ParseError::Line {
                                line: line_no,
                                message,
                            },
                        )?;

                    let (set_literal, item_sources) = parse_set_literal_with_source(&set_text)
                        .map_err(|message| SchemaV1ParseError::Line {
                            line: line_no,
                            message,
                        })?;
                    let assignment_index = module.instances[instance_index].assignments.len();
                    source_map.push(
                        CanonicalSyntacticAddressV1::AssignmentDeclaration {
                            instance_index,
                            assignment_index,
                        },
                        consumed_source_range(text, line, lines.as_slice(), i, next_index),
                    );
                    let line_source = line_segments(text, line);
                    source_map.push(
                        CanonicalSyntacticAddressV1::AssignmentName {
                            instance_index,
                            assignment_index,
                        },
                        original_range(&line_source, relative_range(line, lhs))
                            .expect("assignment name range"),
                    );
                    source_map.push(
                        CanonicalSyntacticAddressV1::SetLiteral {
                            instance_index,
                            assignment_index,
                        },
                        original_range(&segments, 0..set_text.len())
                            .expect("set literal source range"),
                    );
                    for (item_index, item) in item_sources.into_iter().enumerate() {
                        let mut events = vec![(
                            CanonicalSyntacticAddressV1::SetItem {
                                instance_index,
                                assignment_index,
                                item_index,
                            },
                            item.declaration,
                        )];
                        if let Some(range) = item.ident {
                            events.push((
                                CanonicalSyntacticAddressV1::SetIdentName {
                                    instance_index,
                                    assignment_index,
                                    item_index,
                                },
                                range,
                            ));
                        }
                        if let Some(range) = item.label {
                            events.push((
                                CanonicalSyntacticAddressV1::SetTupleLabel {
                                    instance_index,
                                    assignment_index,
                                    item_index,
                                },
                                range,
                            ));
                        }
                        for (field_index, (role, value)) in item.fields.into_iter().enumerate() {
                            events.push((
                                CanonicalSyntacticAddressV1::SetTupleFieldRole {
                                    instance_index,
                                    assignment_index,
                                    item_index,
                                    field_index,
                                },
                                role,
                            ));
                            events.push((
                                CanonicalSyntacticAddressV1::SetTupleFieldValue {
                                    instance_index,
                                    assignment_index,
                                    item_index,
                                    field_index,
                                },
                                value,
                            ));
                        }
                        let mut mapped = events
                            .into_iter()
                            .filter_map(|(address, range)| {
                                original_range(&segments, range).map(|bytes| (address, bytes))
                            })
                            .collect::<Vec<_>>();
                        mapped.sort_by(|left, right| {
                            left.1
                                .start
                                .cmp(&right.1.start)
                                .then_with(|| right.1.end.cmp(&left.1.end))
                        });
                        for (address, bytes) in mapped {
                            source_map.push(address, bytes);
                        }
                    }

                    module.instances[instance_index]
                        .assignments
                        .push(InstanceAssignmentV1 {
                            name: lhs.to_string(),
                            value: set_literal,
                        });

                    i = next_index;
                    continue;
                }

                return Err(SchemaV1ParseError::Line {
                    line: line_no,
                    message: format!("unrecognized instance line: {line}"),
                });
            }
            Section::None => {
                return Err(SchemaV1ParseError::Line {
                    line: line_no,
                    message: "line outside any section".to_string(),
                });
            }
        }
    }

    if module_header_line.is_none() {
        return Err(SchemaV1ParseError::Line {
            line: 1,
            message: "canonical .axi input requires exactly one explicit `module <Name>` header"
                .to_string(),
        });
    }

    Ok((module, source_map))
}

fn strip_comment(line: &str) -> &str {
    let comment_start = [line.find('#'), line.find("--")]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(line.len());
    &line[..comment_start]
}

fn is_ascii_syntax_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

trait CanonicalSyntaxTrim {
    fn trim_axi(&self) -> &str;
    fn trim_axi_start(&self) -> &str;
    fn trim_axi_end(&self) -> &str;
}

impl CanonicalSyntaxTrim for str {
    fn trim_axi(&self) -> &str {
        self.trim_matches(is_ascii_syntax_whitespace)
    }

    fn trim_axi_start(&self) -> &str {
        self.trim_start_matches(is_ascii_syntax_whitespace)
    }

    fn trim_axi_end(&self) -> &str {
        self.trim_end_matches(is_ascii_syntax_whitespace)
    }
}

fn trim_axi(value: &str) -> &str {
    value.trim_axi()
}

fn ascii_whitespace0(input: &str) -> IResult<&str, &str> {
    take_while(is_ascii_syntax_whitespace).parse(input)
}

fn ascii_whitespace1(input: &str) -> IResult<&str, &str> {
    take_while1(is_ascii_syntax_whitespace).parse(input)
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn parse_ident(input: &str) -> IResult<&str, &str> {
    recognize((take_while1(is_ident_start), take_while(is_ident_continue))).parse(input)
}

fn parse_module_header(rest: &str) -> Result<Name, String> {
    all_consuming(preceded(
        ascii_whitespace0,
        (parse_ident, ascii_whitespace0),
    ))
    .parse(rest)
    .map(|(_, (name, _))| name.to_string())
    .map_err(|_| "module header expects exactly `module <Name>`".to_string())
}

fn parse_optional_single_colon_header_payload<'a>(
    rest: &'a str,
    what: &str,
) -> Result<&'a str, String> {
    let trimmed = rest.trim_axi();
    let payload = trimmed
        .strip_suffix(':')
        .map_or(trimmed, CanonicalSyntaxTrim::trim_axi_end);
    if payload.is_empty() || payload.ends_with(':') {
        return Err(format!(
            "{what} header expects canonical content and at most one trailing `:`"
        ));
    }
    Ok(payload)
}

fn parse_optional_colon_name_header<'a>(rest: &'a str, what: &str) -> Result<&'a str, String> {
    let payload = parse_optional_single_colon_header_payload(rest, what)?;
    all_consuming((parse_ident, ascii_whitespace0))
        .parse(payload)
        .map(|(_, (name, _))| name)
        .map_err(|_| format!("{what} header expects one identifier and at most one trailing `:`"))
}

fn parse_schema_header(rest: &str) -> Result<&str, String> {
    parse_optional_colon_name_header(rest, "schema")
}

fn parse_theory_header_with_source(
    rest: &str,
) -> Result<(Name, Name, Range<usize>, Range<usize>), String> {
    fn parser(input: &str) -> IResult<&str, (&str, &str)> {
        let (input, name) = parse_ident(input)?;
        let (input, _) = ascii_whitespace1(input)?;
        let (input, _) = tag("on").parse(input)?;
        let (input, _) = ascii_whitespace1(input)?;
        let (input, schema) = parse_ident(input)?;
        let (input, _) = ascii_whitespace0(input)?;
        Ok((input, (name, schema)))
    }

    let rest = parse_optional_single_colon_header_payload(rest, "theory")?;
    all_consuming(parser)
        .parse(rest)
        .map(|(_, (name, schema))| {
            (
                name.to_string(),
                schema.to_string(),
                relative_range(rest, name),
                relative_range(rest, schema),
            )
        })
        .map_err(|_| "theory header expects: `theory <Name> on <Schema>:`".to_string())
}

fn parse_instance_header_with_source(
    rest: &str,
) -> Result<(Name, Name, Range<usize>, Range<usize>), String> {
    fn parser(input: &str) -> IResult<&str, (&str, &str)> {
        let (input, name) = parse_ident(input)?;
        let (input, _) = ascii_whitespace1(input)?;
        let (input, _) = tag("of").parse(input)?;
        let (input, _) = ascii_whitespace1(input)?;
        let (input, schema) = parse_ident(input)?;
        let (input, _) = ascii_whitespace0(input)?;
        Ok((input, (name, schema)))
    }

    let rest = parse_optional_single_colon_header_payload(rest, "instance")?;
    all_consuming(parser)
        .parse(rest)
        .map(|(_, (name, schema))| {
            (
                name.to_string(),
                schema.to_string(),
                relative_range(rest, name),
                relative_range(rest, schema),
            )
        })
        .map_err(|_| "instance header expects: `instance <Name> of <Schema>:`".to_string())
}

fn parse_subtype_decl(rest: &str) -> Result<SubtypeDeclV1, String> {
    parse_subtype_decl_with_source(rest).map(|(decl, _)| decl)
}

fn parse_subtype_decl_with_source(
    rest: &str,
) -> Result<(SubtypeDeclV1, SubtypeSourceMetadata), String> {
    fn parser(input: &str) -> IResult<&str, (&str, &str, Option<&str>)> {
        let (input, sub) = parse_ident(input)?;
        let (input, _) = ascii_whitespace1(input)?;
        let (input, _) = tag("<").parse(input)?;
        let (input, _) = ascii_whitespace1(input)?;
        let (input, sup) = parse_ident(input)?;
        let (input, inclusion) =
            opt((ascii_whitespace1, tag("as"), ascii_whitespace1, parse_ident)).parse(input)?;
        let (input, _) = ascii_whitespace0(input)?;
        Ok((input, (sub, sup, inclusion.map(|(_, _, _, incl)| incl))))
    }

    let rest = rest.trim_axi();
    all_consuming(parser)
        .parse(rest)
        .map(|(_, (sub, sup, inclusion))| {
            (
                SubtypeDeclV1 {
                    sub: sub.to_string(),
                    sup: sup.to_string(),
                    inclusion: inclusion.map(str::to_string),
                },
                SubtypeSourceMetadata {
                    sub: relative_range(rest, sub),
                    sup: relative_range(rest, sup),
                    inclusion: inclusion.map(|value| relative_range(rest, value)),
                },
            )
        })
        .map_err(|_| {
            "subtype expects canonical `subtype <Sub> < <Sup>` (optionally `as Incl`)".to_string()
        })
}

type SourceSegments = Vec<(Range<usize>, usize)>;

fn relative_range(container: &str, slice: &str) -> Range<usize> {
    let start = slice.as_ptr() as usize - container.as_ptr() as usize;
    start..start + slice.len()
}

fn original_range(segments: &SourceSegments, range: Range<usize>) -> Option<Range<usize>> {
    if range.start >= range.end {
        return None;
    }
    // Binary searches make general occurrence mapping O(O log S), where O is
    // emitted occurrences and S is parser-owned source segments. Role carriers
    // retain the stricter monotone O(R + S) cursor below.
    let start_index = segments.partition_point(|(segment, _)| segment.end <= range.start);
    let end_offset = range.end.checked_sub(1)?;
    let end_index = segments.partition_point(|(segment, _)| segment.end <= end_offset);
    let (start_segment, start_origin) = segments.get(start_index)?;
    let (end_segment, end_origin) = segments.get(end_index)?;
    if range.start < start_segment.start || end_offset >= end_segment.end {
        return None;
    }
    Some(
        (start_origin + range.start - start_segment.start)
            ..(end_origin + range.end - end_segment.start),
    )
}

fn line_segments(source: &str, line: &str) -> SourceSegments {
    vec![(
        0..line.len(),
        line.as_ptr() as usize - source.as_ptr() as usize,
    )]
}

fn consumed_source_range(
    source: &str,
    first: &str,
    lines: &[&str],
    start_index: usize,
    end_index: usize,
) -> Range<usize> {
    let start = first.as_ptr() as usize - source.as_ptr() as usize;
    let last = lines[start_index..end_index]
        .iter()
        .rev()
        .map(|line| strip_comment(line).trim_axi())
        .find(|line| !line.is_empty())
        .unwrap_or(first);
    let end = last.as_ptr() as usize - source.as_ptr() as usize + last.len();
    start..end
}

#[derive(Debug)]
struct RefinementSourceMetadata {
    declaration: Range<usize>,
    terms: Vec<(RefinementTermV1, Range<usize>)>,
}

#[derive(Debug)]
struct TypeSourceMetadata {
    carrier: Range<usize>,
    nodes: Vec<(Vec<RoleTypePathStepV1>, Range<usize>)>,
    indexed_roles: Vec<(Vec<RoleTypePathStepV1>, usize, Range<usize>)>,
    predicates: Vec<(Vec<RoleTypePathStepV1>, usize, RefinementSourceMetadata)>,
}

#[derive(Debug)]
struct FieldSourceMetadata {
    declaration: Range<usize>,
    name: Range<usize>,
    kind: Option<Range<usize>>,
    ty: TypeSourceMetadata,
}

#[derive(Debug)]
struct RelationSourceMetadata {
    name: Range<usize>,
    fields: Vec<FieldSourceMetadata>,
}

#[derive(Debug)]
struct SubtypeSourceMetadata {
    sub: Range<usize>,
    sup: Range<usize>,
    inclusion: Option<Range<usize>>,
}

#[derive(Debug)]
struct GeneratorSourceMetadata {
    name: Range<usize>,
    source: Range<usize>,
    target: Range<usize>,
    kind: Range<usize>,
    reversible: Option<Range<usize>>,
}

fn collect_balanced_parens(
    source: &str,
    lines: &[&str],
    start_index: usize,
    keyword: &str,
) -> Result<(String, usize, SourceSegments), String> {
    let mut depth: i32 = 0;
    let mut combined = String::new();
    let mut segments = Vec::new();

    let mut i = start_index;
    while i < lines.len() {
        let line = strip_comment(lines[i]).trim_axi();
        if line.is_empty() {
            i += 1;
            continue;
        }
        if combined.is_empty() && !line.starts_with(keyword) {
            return Err(format!("expected `{keyword}` declaration"));
        }

        if !combined.is_empty() {
            combined.push(' ');
        }
        segments.push((
            combined.len()..combined.len() + line.len(),
            line.as_ptr() as usize - source.as_ptr() as usize,
        ));
        combined.push_str(line);

        for ch in line.chars() {
            if ch == '(' {
                depth += 1;
            } else if ch == ')' {
                depth -= 1;
            }
        }

        i += 1;
        if depth <= 0 {
            break;
        }
    }

    if depth != 0 {
        return Err("unclosed parenthesis block".to_string());
    }
    Ok((combined, i, segments))
}

fn parse_relation_decl(line: &str) -> Result<RelationDeclV1, String> {
    parse_relation_decl_with_source(line).map(|(relation, _)| relation)
}

fn parse_relation_decl_with_source(
    line: &str,
) -> Result<(RelationDeclV1, RelationSourceMetadata), String> {
    let line = line.trim_axi();
    let rest = line
        .strip_prefix("relation ")
        .ok_or_else(|| "relation declaration must start with `relation `".to_string())?;
    let open = rest
        .find('(')
        .ok_or_else(|| "relation declaration is missing `(`".to_string())?;
    let close = rest
        .rfind(')')
        .ok_or_else(|| "relation declaration is missing `)`".to_string())?;
    if close < open || !rest[close + 1..].trim_axi().is_empty() {
        return Err(
            "relation expects exactly `relation Name(role: Type @kind, ...)`; relation-level axis shorthands are not canonical"
                .to_string(),
        );
    }
    let name = rest[..open].trim_axi();
    parse_identifier_text(name, "relation name")?;
    let inner = rest[open + 1..close].trim_axi();
    if inner.is_empty() {
        return Err("relation must declare at least one role".to_string());
    }
    let parsed_fields = split_top_level_commas_nested(inner)
        .into_iter()
        .map(|field| parse_field_decl_v1(line, field))
        .collect::<Result<Vec<_>, _>>()?;
    let (fields, field_sources): (Vec<_>, Vec<_>) = parsed_fields.into_iter().unzip();
    Ok((
        RelationDeclV1 {
            name: name.to_string(),
            fields,
        },
        RelationSourceMetadata {
            name: relative_range(line, name),
            fields: field_sources,
        },
    ))
}

fn parse_field_decl_v1(
    relation_text: &str,
    text: &str,
) -> Result<(FieldDeclV1, FieldSourceMetadata), String> {
    let Some((field, raw_type)) = text.split_once(':') else {
        return Err(format!(
            "relation role expects `name: Type @kind`, got `{text}`"
        ));
    };
    let field = field.trim_axi();
    parse_identifier_text(field, "role name")?;
    let (type_text, kind, kind_source) = split_role_kind_annotation(raw_type.trim_axi())?;
    let (ty, ty_source) = parse_type_expr_with_source(relation_text, type_text)?;
    Ok((
        FieldDeclV1 {
            field: field.to_string(),
            ty,
            kind,
        },
        FieldSourceMetadata {
            declaration: relative_range(relation_text, text),
            name: relative_range(relation_text, field),
            kind: kind_source.map(|source| relative_range(relation_text, source)),
            ty: ty_source,
        },
    ))
}

fn split_role_kind_annotation(text: &str) -> Result<(&str, RoleKindV1, Option<&str>), String> {
    let annotations = [
        ("@context", RoleKindV1::Context),
        ("@world", RoleKindV1::World),
        ("@temporal", RoleKindV1::Temporal),
        ("@parameter", RoleKindV1::Parameter),
        ("@evidence", RoleKindV1::Evidence),
        ("@data", RoleKindV1::Data),
    ];
    for (suffix, kind) in annotations {
        if let Some(raw_base) = text.strip_suffix(suffix) {
            if !raw_base
                .chars()
                .next_back()
                .is_some_and(is_ascii_syntax_whitespace)
            {
                return Err(format!(
                    "role annotation `{suffix}` requires canonical separating whitespace"
                ));
            }
            let base = raw_base.trim_axi();
            if base.is_empty() {
                return Err(format!("role annotation `{suffix}` is missing a type"));
            }
            return Ok((base, kind, Some(&text[text.len() - suffix.len()..])));
        }
    }
    if text.contains('@') {
        return Err(format!("unknown or misplaced role annotation in `{text}`"));
    }
    Ok((text, RoleKindV1::Data, None))
}

pub fn parse_type_expr_v1(text: &str) -> Result<TypeExprV1, String> {
    parse_type_expr_with_source(text, text).map(|(ty, _)| ty)
}

fn parse_type_expr_with_source(
    root: &str,
    text: &str,
) -> Result<(TypeExprV1, TypeSourceMetadata), String> {
    fn parse_at(
        root: &str,
        text: &str,
        path: Vec<RoleTypePathStepV1>,
        metadata: &mut TypeSourceMetadata,
    ) -> Result<TypeExprV1, String> {
        let text = text.trim_axi();
        metadata
            .nodes
            .push((path.clone(), relative_range(root, text)));
        if let Some(inner) = wrapped_call(text, "relation") {
            parse_identifier_text(inner, "relation-object type")?;
            metadata.carrier = relative_range(root, inner);
            return Ok(TypeExprV1::RelationObject {
                relation: inner.to_string(),
            });
        }
        if let Some(inner) = wrapped_call(text, "indexed") {
            let parts = split_top_level_semicolons(inner);
            if parts.len() != 2 {
                return Err("indexed type expects `indexed(Base; earlier_role|...)`".to_string());
            }
            let mut base_path = path.clone();
            base_path.push(RoleTypePathStepV1::IndexedBase);
            let base = parse_at(root, parts[0], base_path, metadata)?;
            let role_slices = split_pipe_name_slices(parts[1], "indexed role")?;
            for (over_role_index, role) in role_slices.iter().enumerate() {
                metadata.indexed_roles.push((
                    path.clone(),
                    over_role_index,
                    relative_range(root, role),
                ));
            }
            return Ok(TypeExprV1::Indexed {
                base: Box::new(base),
                over_roles: role_slices.iter().map(|role| (*role).to_string()).collect(),
            });
        }
        if let Some(inner) = wrapped_call(text, "refined") {
            let parts = split_top_level_semicolons(inner);
            if parts.len() < 2 {
                return Err("refined type expects `refined(Base; predicate; ...)`".to_string());
            }
            let mut base_path = path.clone();
            base_path.push(RoleTypePathStepV1::RefinedBase);
            let base = parse_at(root, parts[0], base_path, metadata)?;
            let predicates = parts[1..]
                .iter()
                .enumerate()
                .map(|(predicate_index, part)| {
                    let (predicate, predicate_source) =
                        parse_refinement_predicate_with_source(root, part)?;
                    metadata
                        .predicates
                        .push((path.clone(), predicate_index, predicate_source));
                    Ok(predicate)
                })
                .collect::<Result<Vec<_>, String>>()?;
            return Ok(TypeExprV1::Refined {
                base: Box::new(base),
                predicates,
            });
        }
        parse_identifier_text(text, "object type")?;
        metadata.carrier = relative_range(root, text);
        Ok(TypeExprV1::Object {
            name: text.to_string(),
        })
    }

    let mut metadata = TypeSourceMetadata {
        carrier: 0..0,
        nodes: Vec::new(),
        indexed_roles: Vec::new(),
        predicates: Vec::new(),
    };
    let ty = parse_at(root, text, Vec::new(), &mut metadata)?;
    Ok((ty, metadata))
}

fn parse_refinement_predicate_with_source(
    root: &str,
    text: &str,
) -> Result<(RefinementPredicateV1, RefinementSourceMetadata), String> {
    let text = text.trim_axi();
    let declaration = relative_range(root, text);
    let finish = |predicate, terms| {
        Ok((
            predicate,
            RefinementSourceMetadata {
                declaration: declaration.clone(),
                terms,
            },
        ))
    };
    if let Some(inner) = wrapped_call(text, "eq") {
        parse_identifier_text(inner, "equality value")?;
        return finish(
            RefinementPredicateV1::Equals {
                value: inner.to_string(),
            },
            vec![(RefinementTermV1::EqualsValue, relative_range(root, inner))],
        );
    }
    if let Some(inner) = wrapped_call(text, "in") {
        let values = split_pipe_name_slices(inner, "membership value")?;
        let terms = values
            .iter()
            .enumerate()
            .map(|(value_index, value)| {
                (
                    RefinementTermV1::MemberValue { value_index },
                    relative_range(root, value),
                )
            })
            .collect();
        return finish(
            RefinementPredicateV1::MemberOf {
                values: values.into_iter().map(str::to_string).collect(),
            },
            terms,
        );
    }
    if let Some(inner) = wrapped_call(text, "enum") {
        let values = split_pipe_name_slices(inner, "enum value")?;
        let terms = values
            .iter()
            .enumerate()
            .map(|(value_index, value)| {
                (
                    RefinementTermV1::EnumValue { value_index },
                    relative_range(root, value),
                )
            })
            .collect();
        return finish(
            RefinementPredicateV1::Enum {
                values: values.into_iter().map(str::to_string).collect(),
            },
            terms,
        );
    }
    if let Some(inner) = wrapped_call(text, "key") {
        let roles = split_pipe_name_slices(inner, "key role")?;
        let terms = roles
            .iter()
            .enumerate()
            .map(|(role_index, role)| {
                (
                    RefinementTermV1::KeyRole { role_index },
                    relative_range(root, role),
                )
            })
            .collect();
        return finish(
            RefinementPredicateV1::Key {
                roles: roles.into_iter().map(str::to_string).collect(),
            },
            terms,
        );
    }
    if let Some(inner) = wrapped_call(text, "cardinality") {
        let values = inner.split('|').map(trim_axi).collect::<Vec<_>>();
        if values.len() != 2 {
            return Err("cardinality expects `cardinality(min|max)`".to_string());
        }
        let min = parse_u32_token(values[0], "cardinality minimum")?;
        let max = parse_u32_token(values[1], "cardinality maximum")?;
        if min > max {
            return Err("cardinality minimum exceeds maximum".to_string());
        }
        return finish(
            RefinementPredicateV1::Cardinality { min, max },
            vec![
                (
                    RefinementTermV1::CardinalityMin,
                    relative_range(root, values[0]),
                ),
                (
                    RefinementTermV1::CardinalityMax,
                    relative_range(root, values[1]),
                ),
            ],
        );
    }
    if let Some(inner) = wrapped_call(text, "predicate") {
        let names = split_pipe_name_slices(inner, "predicate name or argument")?;
        let (name, args) = names
            .split_first()
            .ok_or_else(|| "predicate must name a supported predicate".to_string())?;
        let mut terms = vec![(RefinementTermV1::PredicateName, relative_range(root, name))];
        terms.extend(args.iter().enumerate().map(|(argument_index, argument)| {
            (
                RefinementTermV1::PredicateArgument { argument_index },
                relative_range(root, argument),
            )
        }));
        return finish(
            RefinementPredicateV1::Predicate {
                name: (*name).to_string(),
                args: args.iter().map(|arg| (*arg).to_string()).collect(),
            },
            terms,
        );
    }
    Err(format!("unsupported refinement predicate `{text}`"))
}

fn wrapped_call<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.strip_prefix(name)?
        .strip_prefix('(')?
        .strip_suffix(')')
        .map(trim_axi)
}

fn reject_empty_finite_elements<'a>(
    parts: impl IntoIterator<Item = &'a str>,
    what: &str,
) -> Result<Vec<&'a str>, String> {
    let parts = parts.into_iter().map(trim_axi).collect::<Vec<_>>();
    if parts.is_empty() || parts.iter().any(|part| part.is_empty()) {
        return Err(format!(
            "{what} must not contain empty, interior-empty, or trailing elements"
        ));
    }
    Ok(parts)
}

fn split_pipe_name_slices<'a>(text: &'a str, what: &str) -> Result<Vec<&'a str>, String> {
    let names = reject_empty_finite_elements(text.split('|'), &format!("{what} list"))?;
    for name in &names {
        parse_identifier_text(name, what)?;
    }
    Ok(names)
}

fn split_top_level_semicolons(text: &str) -> Vec<&str> {
    split_at_top_level(text, ';')
}

fn split_top_level_commas_nested(text: &str) -> Vec<&str> {
    split_at_top_level(text, ',')
}

fn split_at_top_level(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    for (index, ch) in text.char_indices() {
        match ch {
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => depth -= 1,
            _ if ch == separator && depth == 0 => {
                parts.push(text[start..index].trim_axi());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim_axi());
    parts
}

fn parse_identifier_text(text: &str, what: &str) -> Result<(), String> {
    all_consuming(parse_ident)
        .parse(text)
        .map(|_| ())
        .map_err(|_| format!("{what} must be an identifier, got `{text}`"))
}

fn parse_u32_token(text: &str, what: &str) -> Result<u32, String> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!(
            "{what} must be a nonempty ASCII-digit token within u32"
        ));
    }
    text.parse::<u32>()
        .map_err(|_| format!("{what} must be within u32"))
}

fn parse_value_atom_text(text: &str, what: &str) -> Result<(), String> {
    if text.is_empty()
        || text.chars().any(|ch| {
            is_ascii_syntax_whitespace(ch) || matches!(ch, ',' | '(' | ')' | '{' | '}' | '=' | ':')
        })
    {
        return Err(format!("{what} must be a finite atom, got `{text}`"));
    }
    Ok(())
}

fn parse_generator_decl_with_source(
    line: &str,
) -> Result<(GeneratorDeclV1, GeneratorSourceMetadata), String> {
    let line = line.trim_axi();
    let (kind, kind_source, rest) = if let Some(rest) = line.strip_prefix("aspect ") {
        (GeneratorKindV1::Aspect, &line[.."aspect".len()], rest)
    } else if let Some(rest) = line.strip_prefix("function ") {
        (GeneratorKindV1::Function, &line[.."function".len()], rest)
    } else {
        return Err("generator must start with `aspect` or `function`".to_string());
    };
    let trimmed_rest = rest.trim_axi_end();
    let reversible_parts = trimmed_rest
        .strip_suffix("@reversible")
        .and_then(|before_marker| {
            before_marker
                .chars()
                .next_back()
                .is_some_and(is_ascii_syntax_whitespace)
                .then(|| {
                    (
                        before_marker.trim_axi_end(),
                        &trimmed_rest[trimmed_rest.len() - "@reversible".len()..],
                    )
                })
        });
    let (rest, reversible, reversible_source) = match reversible_parts {
        Some((without_marker, marker)) => (without_marker, true, Some(marker)),
        None => (rest, false, None),
    };
    let rest = rest.trim_axi();
    let Some((name, signature)) = rest.split_once(':') else {
        return Err("generator expects `name: Source -> Target`".to_string());
    };
    let name = name.trim_axi();
    parse_identifier_text(name, "generator name")?;
    let Some((source, target)) = signature.split_once("->") else {
        return Err("generator expects `Source -> Target`".to_string());
    };
    let source = source.trim_axi();
    let target = target.trim_axi();
    parse_identifier_text(source, "generator source")?;
    parse_identifier_text(target, "generator target")?;
    Ok((
        GeneratorDeclV1 {
            name: name.to_string(),
            source: source.to_string(),
            target: target.to_string(),
            kind,
            reversible,
        },
        GeneratorSourceMetadata {
            name: relative_range(line, name),
            source: relative_range(line, source),
            target: relative_range(line, target),
            kind: relative_range(line, kind_source),
            reversible: reversible_source.map(|source| relative_range(line, source)),
        },
    ))
}

#[derive(Debug)]
struct ConstraintSourceMetadata {
    terms: Vec<(ConstraintTermV1, Range<usize>)>,
}

fn constraint_source_metadata(
    root: &str,
    constraint: &ConstraintV1,
) -> Option<ConstraintSourceMetadata> {
    if matches!(
        constraint,
        ConstraintV1::Unknown { .. } | ConstraintV1::NamedBlock { .. }
    ) {
        return None;
    }

    let mut working = root.trim_axi();
    let mut suffix_terms = Vec::new();
    while working.ends_with(')') {
        let on_index = rfind_syntax_keyword(working, "on");
        let param_index = rfind_syntax_keyword(working, "param");
        let (kind, index) = match (on_index, param_index) {
            (None, None) => break,
            (Some(index), None) => ("on", index),
            (None, Some(index)) => ("param", index),
            (Some(on), Some(param)) if on > param => ("on", on),
            (Some(_), Some(param)) => ("param", param),
        };
        let clause = working[index + kind.len()..].trim_axi();
        let inner = clause.strip_prefix('(')?.strip_suffix(')')?;
        let fields = inner.split(',').map(trim_axi).collect::<Vec<_>>();
        match kind {
            "on" => suffix_terms.extend(fields.iter().enumerate().map(|(carrier_index, field)| {
                (
                    ConstraintTermV1::CarrierField { carrier_index },
                    relative_range(root, field),
                )
            })),
            "param" => {
                suffix_terms.extend(fields.iter().enumerate().map(|(parameter_index, field)| {
                    (
                        ConstraintTermV1::Parameter { parameter_index },
                        relative_range(root, field),
                    )
                }))
            }
            _ => unreachable!(),
        }
        working = working[..index].trim_axi_end();
    }

    let mut terms = Vec::new();
    let mut relation_index = 0;
    let mut push_relation = |slice: &str, terms: &mut Vec<_>| {
        terms.push((
            ConstraintTermV1::Relation {
                occurrence_index: relation_index,
            },
            relative_range(root, slice),
        ));
        relation_index += 1;
    };
    fn rel_field(text: &str) -> Option<(&str, &str)> {
        let (relation, field) = text.trim_axi().split_once('.')?;
        Some((relation.trim_axi(), field.trim_axi()))
    }

    match constraint {
        ConstraintV1::Functional { .. } => {
            let after = strip_syntax_keyword(working, "functional")?.trim_axi();
            let (left, right) = after.split_once("->")?;
            let (left_relation, source) = rel_field(left)?;
            let (right_relation, target) = rel_field(right)?;
            push_relation(left_relation, &mut terms);
            terms.push((ConstraintTermV1::SourceField, relative_range(root, source)));
            push_relation(right_relation, &mut terms);
            terms.push((ConstraintTermV1::TargetField, relative_range(root, target)));
        }
        ConstraintV1::AtMost { .. } => {
            let after = strip_syntax_keyword(working, "at_most")?.trim_axi();
            let separator = after.find(is_ascii_syntax_whitespace)?;
            let (bound, relation_fields) = after.split_at(separator);
            let relation_fields = relation_fields.trim_axi_start();
            terms.push((ConstraintTermV1::Bound, relative_range(root, bound)));
            let (left, right) = relation_fields.split_once("->")?;
            let (left_relation, source) = rel_field(left)?;
            let (right_relation, target) = rel_field(right)?;
            push_relation(left_relation, &mut terms);
            terms.push((ConstraintTermV1::SourceField, relative_range(root, source)));
            push_relation(right_relation, &mut terms);
            terms.push((ConstraintTermV1::TargetField, relative_range(root, target)));
        }
        ConstraintV1::Typing { .. } => {
            let after = strip_syntax_keyword(working, "typing")?.trim_axi();
            let (relation, rule) = after.split_once(':')?;
            push_relation(relation.trim_axi(), &mut terms);
            terms.push((
                ConstraintTermV1::Rule,
                relative_range(root, rule.trim_axi()),
            ));
        }
        ConstraintV1::SymmetricWhereIn { .. } => {
            let after = strip_syntax_keyword(working, "symmetric")?.trim_axi();
            let (relation, guard) = split_once_syntax_keyword(after, "where")?;
            push_relation(relation.trim_axi(), &mut terms);
            let (left, values) = split_once_syntax_keyword(guard, "in")?;
            if let Some((guard_relation, field)) = rel_field(left) {
                push_relation(guard_relation, &mut terms);
                terms.push((ConstraintTermV1::GuardField, relative_range(root, field)));
            } else {
                terms.push((
                    ConstraintTermV1::GuardField,
                    relative_range(root, left.trim_axi()),
                ));
            }
            let inner = values
                .trim_axi()
                .strip_prefix('{')?
                .strip_suffix('}')?
                .trim_axi();
            terms.extend(
                inner
                    .split(',')
                    .map(trim_axi)
                    .enumerate()
                    .map(|(value_index, value)| {
                        (
                            ConstraintTermV1::GuardValue { value_index },
                            relative_range(root, value),
                        )
                    }),
            );
        }
        ConstraintV1::Symmetric { .. } => {
            push_relation(
                strip_syntax_keyword(working, "symmetric")?.trim_axi(),
                &mut terms,
            );
        }
        ConstraintV1::Transitive { .. } => {
            push_relation(
                strip_syntax_keyword(working, "transitive")?.trim_axi(),
                &mut terms,
            );
        }
        ConstraintV1::Key { .. } => {
            let after = strip_syntax_keyword(working, "key")?.trim_axi();
            let open = after.find('(')?;
            let close = after.rfind(')')?;
            push_relation(after[..open].trim_axi(), &mut terms);
            let fields = after[open + 1..close].split(',').map(trim_axi);
            terms.extend(fields.enumerate().map(|(field_index, field)| {
                (
                    ConstraintTermV1::KeyField { field_index },
                    relative_range(root, field),
                )
            }));
        }
        ConstraintV1::Unknown { .. } | ConstraintV1::NamedBlock { .. } => return None,
    }
    terms.extend(suffix_terms);
    Some(ConstraintSourceMetadata { terms })
}

fn strip_syntax_keyword<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(keyword)?;
    if rest.is_empty() {
        return Some(rest);
    }
    rest.chars()
        .next()
        .is_some_and(is_ascii_syntax_whitespace)
        .then(|| rest.trim_axi_start())
}

fn syntax_keyword_at(text: &str, index: usize, keyword: &str) -> bool {
    let before = text[..index].chars().next_back();
    let after_index = index + keyword.len();
    let after = text[after_index..].chars().next();
    before.is_some_and(is_ascii_syntax_whitespace) && after.is_some_and(is_ascii_syntax_whitespace)
}

fn rfind_syntax_keyword(text: &str, keyword: &str) -> Option<usize> {
    text.match_indices(keyword)
        .map(|(index, _)| index)
        .filter(|index| syntax_keyword_at(text, *index, keyword))
        .last()
}

fn split_once_syntax_keyword<'a>(text: &'a str, keyword: &str) -> Option<(&'a str, &'a str)> {
    let index = text
        .match_indices(keyword)
        .map(|(index, _)| index)
        .find(|index| syntax_keyword_at(text, *index, keyword))?;
    Some((&text[..index], &text[index + keyword.len()..]))
}

fn starts_with_syntax_keyword(text: &str, keyword: &str) -> bool {
    strip_syntax_keyword(text, keyword).is_some()
}

fn canonicalize_constraint_whitespace(text: &str) -> String {
    text.split(is_ascii_syntax_whitespace)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_constraint(rest: &str) -> Result<ConstraintV1, String> {
    let original = rest.trim_axi();
    let families = [
        "functional",
        "at_most",
        "typing",
        "symmetric",
        "transitive",
        "key",
    ];
    if families.iter().any(|family| {
        original
            .strip_prefix(family)
            .and_then(|suffix| suffix.chars().next())
            .is_some_and(|character| {
                character.is_whitespace() && !is_ascii_syntax_whitespace(character)
            })
    }) {
        return Err(
            "constraint family keyword requires canonical ASCII separating whitespace".to_string(),
        );
    }
    let recognized_family = families
        .iter()
        .any(|family| starts_with_syntax_keyword(original, family));
    if !recognized_family {
        return Ok(ConstraintV1::Unknown {
            text: original.to_string(),
        });
    }
    let canonical = canonicalize_constraint_whitespace(original);
    let original = canonical.as_str();

    #[derive(Debug)]
    enum ClosureClauseV1 {
        On(CarrierFieldsV1),
        Param(Vec<Name>),
    }

    type SplitClosureClausesV1 = (String, Option<CarrierFieldsV1>, Option<Vec<Name>>);

    fn peel_closure_clause_suffix(rest: &str) -> Result<Option<(String, ClosureClauseV1)>, String> {
        let trimmed = rest.trim_axi_end();
        if !trimmed.ends_with(')') {
            return Ok(None);
        }

        let on_idx = trimmed.rfind(" on ");
        let param_idx = trimmed.rfind(" param ");

        // Prefer the rightmost clause (closest to the end of the string).
        let (kind, idx) = match (on_idx, param_idx) {
            (None, None) => return Ok(None),
            (Some(i), None) => ("on", i),
            (None, Some(i)) => ("param", i),
            (Some(i1), Some(i2)) => {
                if i1 > i2 {
                    ("on", i1)
                } else {
                    ("param", i2)
                }
            }
        };

        let (base, part) = trimmed.split_at(idx);
        let part = if kind == "on" {
            part.strip_prefix(" on ").unwrap_or(part)
        } else {
            part.strip_prefix(" param ").unwrap_or(part)
        };
        let part = part.trim_axi();
        if !part.starts_with('(') || !part.ends_with(')') {
            return Ok(None);
        }
        let inner = &part[1..part.len() - 1];
        let fields = reject_empty_finite_elements(inner.split(','), &format!("{kind} fields"))?;

        match kind {
            "on" => {
                if fields.len() != 2 {
                    return Err("carrier fields clause expects: `on (field0, field1)`".to_string());
                }
                for field in &fields {
                    parse_identifier_text(field, "carrier field")?;
                }
                Ok(Some((
                    base.trim_axi().to_string(),
                    ClosureClauseV1::On(CarrierFieldsV1 {
                        left_field: fields[0].to_string(),
                        right_field: fields[1].to_string(),
                    }),
                )))
            }
            "param" => {
                if fields.is_empty() {
                    return Err(
                        "param fields clause expects: `param (field0, field1, ...)`".to_string()
                    );
                }
                for field in &fields {
                    parse_identifier_text(field, "parameter field")?;
                }
                Ok(Some((
                    base.trim_axi().to_string(),
                    ClosureClauseV1::Param(fields.iter().map(|s| (*s).to_string()).collect()),
                )))
            }
            _ => Ok(None),
        }
    }

    fn split_closure_clauses(rest: &str) -> Result<SplitClosureClausesV1, String> {
        let mut base = rest.trim_axi().to_string();
        let mut carriers: Option<CarrierFieldsV1> = None;
        let mut params: Option<Vec<Name>> = None;

        while let Some((b, clause)) = peel_closure_clause_suffix(&base)? {
            match clause {
                ClosureClauseV1::On(c) => {
                    if carriers.is_some() {
                        return Err("duplicate `on (...)` clause in constraint".to_string());
                    }
                    carriers = Some(c);
                }
                ClosureClauseV1::Param(p) => {
                    if params.is_some() {
                        return Err("duplicate `param (...)` clause in constraint".to_string());
                    }
                    if carriers.is_some() {
                        return Err(
                            "closure clauses must use canonical `on (...) param (...)` order"
                                .to_string(),
                        );
                    }
                    params = Some(p);
                }
            }
            base = b;
        }

        Ok((base.trim_axi().to_string(), carriers, params))
    }

    let (rest, carriers, params) = split_closure_clauses(original)?;
    let rest = rest.trim_axi();
    if carriers.is_some() && !(rest.starts_with("symmetric ") || rest.starts_with("transitive ")) {
        return Err(
            "`on (...)` is only supported for symmetric/transitive constraints".to_string(),
        );
    }
    if params.is_some()
        && !(rest.starts_with("symmetric ")
            || rest.starts_with("transitive ")
            || rest.starts_with("at_most "))
    {
        return Err(
            "`param (...)` is only supported for symmetric/transitive/at_most constraints"
                .to_string(),
        );
    }
    if rest == "functional" || rest.starts_with("functional ") {
        let after = rest
            .strip_prefix("functional")
            .expect("matched prefix")
            .trim_axi();
        let parts: Vec<&str> = after.split("->").collect();
        if parts.len() == 2 {
            if let (Ok((rel1, field1)), Ok((rel2, field2))) = (
                split_rel_field(parts[0].trim_axi()),
                split_rel_field(parts[1].trim_axi()),
            ) {
                if rel1 == rel2 {
                    return Ok(ConstraintV1::Functional {
                        relation: rel1,
                        src_field: field1,
                        dst_field: field2,
                    });
                }
            }
        }
        return Err(
            "functional expects: `functional Rel.field -> Rel.field` on one relation".to_string(),
        );
    }

    if rest == "at_most" || rest.starts_with("at_most ") {
        let after = rest
            .strip_prefix("at_most")
            .expect("matched prefix")
            .trim_axi();
        if carriers.is_some() {
            return Err("`on (...)` is not supported for at_most constraints".to_string());
        }
        let (max_str, rest2) = after
            .split_once(' ')
            .ok_or_else(|| "at_most expects: `at_most N Rel.field -> Rel.field`".to_string())?;
        let max = parse_u32_token(max_str, "at_most expects a non-negative integer bound")?;
        let parts: Vec<&str> = rest2.split("->").collect();
        if parts.len() != 2 {
            return Err("at_most expects: `Rel.field -> Rel.field`".to_string());
        }
        if let (Ok((rel1, field1)), Ok((rel2, field2))) = (
            split_rel_field(parts[0].trim_axi()),
            split_rel_field(parts[1].trim_axi()),
        ) {
            if rel1 == rel2 {
                return Ok(ConstraintV1::AtMost {
                    relation: rel1,
                    src_field: field1,
                    dst_field: field2,
                    max,
                    params,
                });
            }
        }
        return Err("at_most expects: `Rel.field -> Rel.field` (same relation)".to_string());
    }

    if rest == "typing" || rest.starts_with("typing ") {
        let after = rest
            .strip_prefix("typing")
            .expect("matched prefix")
            .trim_axi();
        if let Some((relation, rule)) = after.split_once(':') {
            let relation = relation.trim_axi();
            let rule = rule.trim_axi();
            if !relation.is_empty() && !rule.is_empty() {
                parse_identifier_text(relation, "typing relation")?;
                parse_identifier_text(rule, "typing rule")?;
                return Ok(ConstraintV1::Typing {
                    relation: relation.to_string(),
                    rule: rule.to_string(),
                });
            }
        }
        return Err("typing expects: `typing Relation: rule_name`".to_string());
    }

    if rest == "symmetric" || rest.starts_with("symmetric ") {
        let after = rest
            .strip_prefix("symmetric")
            .expect("matched prefix")
            .trim_axi();
        if after.is_empty() {
            return Err("symmetric expects a relation name".to_string());
        }
        // Support a minimal conditional form:
        //
        //   `symmetric Rel where Rel.field in {A, B, ...}`
        //
        // This is useful for relations that include a "kind" field (e.g. a
        // polymorphic `Relationship` relation where only some relTypes are
        // symmetric).
        if let Some((relation, guard)) = after.split_once(" where ") {
            let relation = relation.trim_axi();
            let guard = guard.trim_axi();
            if !relation.is_empty() {
                parse_identifier_text(relation, "symmetric relation")?;
                if let Some((lhs, rhs)) = guard.split_once(" in ") {
                    let lhs = lhs.trim_axi();
                    let rhs = rhs.trim_axi();
                    let (rel2, field) = split_rel_field(lhs).map_err(|_| {
                        "symmetric guard must use canonical qualified `Relation.field` syntax"
                            .to_string()
                    })?;
                    if rel2 == relation {
                        parse_identifier_text(&field, "symmetric guard field")?;
                        if let Some(values) = parse_name_set_literal(rhs) {
                            return Ok(ConstraintV1::SymmetricWhereIn {
                                relation: relation.to_string(),
                                field,
                                values,
                                carriers,
                                params,
                            });
                        }
                    }
                }
            }
            return Err("symmetric expects a relation name and optional canonical `where ... in {...}` guard".to_string());
        }

        // Unconditional symmetry.
        parse_identifier_text(after, "symmetric relation")?;
        return Ok(ConstraintV1::Symmetric {
            relation: after.to_string(),
            carriers,
            params,
        });
    }

    if rest == "transitive" || rest.starts_with("transitive ") {
        let after = rest
            .strip_prefix("transitive")
            .expect("matched prefix")
            .trim_axi();
        if after.is_empty() {
            return Err("transitive expects a relation name".to_string());
        }
        parse_identifier_text(after, "transitive relation")?;
        return Ok(ConstraintV1::Transitive {
            relation: after.to_string(),
            carriers,
            params,
        });
    }

    if rest == "key" || rest.starts_with("key ") {
        let after = rest.strip_prefix("key").expect("matched prefix").trim_axi();
        if carriers.is_some() || params.is_some() {
            return Err(
                "`on (...)` / `param (...)` are only supported for symmetric/transitive constraints"
                    .to_string(),
            );
        }
        let invalid_shape = || "key expects: `key Relation(field, ...)`".to_string();
        let open = after.find('(').ok_or_else(invalid_shape)?;
        let close = after.rfind(')').ok_or_else(invalid_shape)?;
        if close != after.len() - 1 || close <= open {
            return Err(invalid_shape());
        }
        let relation = after[..open].trim_axi();
        let fields_text = after[open + 1..close].trim_axi();
        let fields = reject_empty_finite_elements(fields_text.split(','), "key field list")?
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        if relation.is_empty() || fields.is_empty() {
            return Err(invalid_shape());
        }
        parse_identifier_text(relation, "key relation")?;
        for field in &fields {
            parse_identifier_text(field, "key field")?;
        }
        return Ok(ConstraintV1::Key {
            relation: relation.to_string(),
            fields,
        });
    }

    Ok(ConstraintV1::Unknown {
        text: rest.to_string(),
    })
}

/// Parse a `constraint ...` line body in canonical `axi_v1` surface syntax.
///
/// This takes the text after the `constraint ` keyword.
///
/// Notes:
/// - The parser is intentionally robust: unrecognized/dialect-ish forms are
///   returned as `ConstraintV1::Unknown` so tooling can surface and repair them.
pub fn parse_constraint_v1(rest: &str) -> Result<ConstraintV1, String> {
    parse_constraint(rest)
}

/// Format a `ConstraintV1` back into canonical `axi_v1` surface syntax.
///
/// This returns a single-line `constraint ...` string. Named-block constraints
/// (`ConstraintV1::NamedBlock`) require multi-line rendering and are **not**
/// supported by this helper.
pub fn format_constraint_v1(constraint: &ConstraintV1) -> Result<String, String> {
    fn on_clause(carriers: &Option<CarrierFieldsV1>) -> String {
        match carriers {
            Some(c) => format!(" on ({}, {})", c.left_field, c.right_field),
            None => String::new(),
        }
    }
    fn param_clause(params: &Option<Vec<Name>>) -> String {
        match params {
            Some(p) if !p.is_empty() => format!(" param ({})", p.join(", ")),
            _ => String::new(),
        }
    }

    match constraint {
        ConstraintV1::Functional {
            relation,
            src_field,
            dst_field,
        } => Ok(format!(
            "constraint functional {relation}.{src_field} -> {relation}.{dst_field}"
        )),
        ConstraintV1::AtMost {
            relation,
            src_field,
            dst_field,
            max,
            params,
        } => {
            let mut out = format!(
                "constraint at_most {max} {relation}.{src_field} -> {relation}.{dst_field}"
            );
            if let Some(params) = params {
                let params = params
                    .iter()
                    .map(|s| s.trim_axi())
                    .collect::<Vec<_>>()
                    .join(", ");
                if !params.is_empty() {
                    out.push_str(&format!(" param ({params})"));
                }
            }
            Ok(out)
        }
        ConstraintV1::Typing { relation, rule } => {
            Ok(format!("constraint typing {relation}: {rule}"))
        }
        ConstraintV1::SymmetricWhereIn {
            relation,
            field,
            values,
            carriers,
            params,
        } => Ok(format!(
            "constraint symmetric {relation} where {relation}.{field} in {{{}}}{}{}",
            values.join(", "),
            on_clause(carriers),
            param_clause(params)
        )),
        ConstraintV1::Symmetric {
            relation,
            carriers,
            params,
        } => Ok(format!(
            "constraint symmetric {relation}{}{}",
            on_clause(carriers),
            param_clause(params)
        )),
        ConstraintV1::Transitive {
            relation,
            carriers,
            params,
        } => Ok(format!(
            "constraint transitive {relation}{}{}",
            on_clause(carriers),
            param_clause(params)
        )),
        ConstraintV1::Key { relation, fields } => {
            Ok(format!("constraint key {relation}({})", fields.join(", ")))
        }
        ConstraintV1::Unknown { text } => Ok(format!("constraint {text}")),
        ConstraintV1::NamedBlock { .. } => Err(
            "named-block constraints require multi-line rendering; keep the original block"
                .to_string(),
        ),
    }
}

/// Parse a canonical `subtype ...` declaration body.
pub fn parse_subtype_decl_v1(rest: &str) -> Result<SubtypeDeclV1, String> {
    parse_subtype_decl(rest)
}

/// Format a subtype declaration back into canonical `axi_v1` surface syntax.
pub fn format_subtype_decl_v1(decl: &SubtypeDeclV1) -> String {
    match decl.inclusion.as_ref() {
        Some(inclusion) => format!("subtype {} < {} as {}", decl.sub, decl.sup, inclusion),
        None => format!("subtype {} < {}", decl.sub, decl.sup),
    }
}

/// Parse a canonical `relation ...` declaration.
pub fn parse_relation_decl_v1(line: &str) -> Result<RelationDeclV1, String> {
    parse_relation_decl(line)
}

/// Format a relation declaration back into canonical `axi_v1` surface syntax.
/// Role-axis semantics are always rendered explicitly; relation-level
/// shorthands and role-name inference are not part of the canonical language.
pub fn format_relation_decl_v1(relation: &RelationDeclV1) -> String {
    let fields = relation
        .fields
        .iter()
        .map(|field| {
            let annotation = match field.kind {
                RoleKindV1::Data => "@data",
                RoleKindV1::Context => "@context",
                RoleKindV1::World => "@world",
                RoleKindV1::Temporal => "@temporal",
                RoleKindV1::Parameter => "@parameter",
                RoleKindV1::Evidence => "@evidence",
            };
            format!("{}: {} {annotation}", field.field, field.ty)
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("relation {}({fields})", relation.name)
}

pub fn format_generator_decl_v1(generator: &GeneratorDeclV1) -> String {
    let keyword = match generator.kind {
        GeneratorKindV1::Aspect => "aspect",
        GeneratorKindV1::Function => "function",
    };
    let reversible = if generator.reversible {
        " @reversible"
    } else {
        ""
    };
    format!(
        "{keyword} {}: {} -> {}{reversible}",
        generator.name, generator.source, generator.target
    )
}

fn split_rel_field(s: &str) -> Result<(Name, Name), String> {
    let s = s.trim_axi();
    let Some((rel, field)) = s.split_once('.') else {
        return Err(format!("expected `Rel.field`, got `{s}`"));
    };
    if rel.is_empty() || field.is_empty() {
        return Err(format!("expected `Rel.field`, got `{s}`"));
    }
    parse_identifier_text(rel, "relation")?;
    parse_identifier_text(field, "relation field")?;
    Ok((rel.to_string(), field.to_string()))
}

fn parse_name_set_literal(s: &str) -> Option<Vec<Name>> {
    let s = s.trim_axi();
    let inner = s.strip_prefix('{')?.strip_suffix('}')?.trim_axi();
    let values = reject_empty_finite_elements(inner.split(','), "guard value set").ok()?;
    if values
        .iter()
        .any(|value| parse_identifier_text(value, "set value").is_err())
    {
        return None;
    }
    Some(values.into_iter().map(str::to_string).collect())
}

fn collect_constraint_with_segments(
    source: &str,
    lines: &[&str],
    start_index: usize,
    first: &str,
) -> (String, usize, SourceSegments) {
    let mut combined = String::new();
    let mut segments = Vec::new();
    let first = first.trim_axi();
    segments.push((
        0..first.len(),
        first.as_ptr() as usize - source.as_ptr() as usize,
    ));
    combined.push_str(first);

    let mut index = start_index;
    while index < lines.len() {
        let line = strip_comment(lines[index]).trim_axi();
        if line.is_empty() {
            index += 1;
            continue;
        }
        if is_top_level_keyword(line) {
            break;
        }
        combined.push(' ');
        segments.push((
            combined.len()..combined.len() + line.len(),
            line.as_ptr() as usize - source.as_ptr() as usize,
        ));
        combined.push_str(line);
        index += 1;
    }
    (combined, index, segments)
}

fn collect_indented_block_with_segments(
    source: &str,
    lines: &[&str],
    start_index: usize,
) -> (String, usize, SourceSegments) {
    let mut combined = String::new();
    let mut segments = Vec::new();
    let mut i = start_index;
    while i < lines.len() {
        let trimmed = strip_comment(lines[i]).trim_axi();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }
        if is_top_level_keyword(trimmed) {
            break;
        }
        if !combined.is_empty() {
            combined.push(' ');
        }
        segments.push((
            combined.len()..combined.len() + trimmed.len(),
            trimmed.as_ptr() as usize - source.as_ptr() as usize,
        ));
        combined.push_str(trimmed);
        i += 1;
    }
    (combined, i, segments)
}

#[derive(Debug, Default)]
struct SegmentedSource {
    text: String,
    segments: SourceSegments,
}

impl SegmentedSource {
    fn push(&mut self, source: &str, value: &str) {
        let value = value.trim_axi();
        if value.is_empty() {
            return;
        }
        if !self.text.is_empty() {
            self.text.push(' ');
        }
        self.segments.push((
            self.text.len()..self.text.len() + value.len(),
            value.as_ptr() as usize - source.as_ptr() as usize,
        ));
        self.text.push_str(value);
    }

    fn original_span(&self) -> Option<Range<usize>> {
        original_range(&self.segments, 0..self.text.len())
    }
}

#[derive(Debug, Default)]
struct RewriteSourceInputs {
    vars: Vec<SegmentedSource>,
    all_vars: SegmentedSource,
    lhs: SegmentedSource,
    rhs: SegmentedSource,
    orientations: Vec<SegmentedSource>,
}

fn collect_rewrite_source_inputs(
    source: &str,
    lines: &[&str],
    start_index: usize,
    end_index: usize,
) -> RewriteSourceInputs {
    #[derive(Clone, Copy)]
    enum Field {
        Vars,
        Lhs,
        Rhs,
        Orientation,
    }

    let mut out = RewriteSourceInputs::default();
    let mut current = None;
    for raw in &lines[start_index..end_index] {
        let line = strip_comment(raw).trim_axi();
        if line.is_empty() {
            continue;
        }
        let header = [
            ("vars", Field::Vars),
            ("lhs", Field::Lhs),
            ("rhs", Field::Rhs),
            ("orientation", Field::Orientation),
        ]
        .into_iter()
        .find_map(|(name, field)| {
            line.strip_prefix(name)
                .and_then(|rest| rest.strip_prefix(':'))
                .map(|rest| (field, rest.trim_axi()))
        });
        if let Some((field, value)) = header {
            current = Some(field);
            if !value.is_empty() {
                match field {
                    Field::Vars => {
                        let mut entry = SegmentedSource::default();
                        entry.push(source, value);
                        out.all_vars.push(source, value);
                        out.vars.push(entry);
                    }
                    Field::Lhs => out.lhs.push(source, value),
                    Field::Rhs => out.rhs.push(source, value),
                    Field::Orientation => {
                        let mut entry = SegmentedSource::default();
                        entry.push(source, value);
                        out.orientations.push(entry);
                        current = None;
                    }
                }
            }
            continue;
        }
        if let Some(field) = current {
            match field {
                Field::Vars => {
                    let mut entry = SegmentedSource::default();
                    entry.push(source, line);
                    out.all_vars.push(source, line);
                    out.vars.push(entry);
                }
                Field::Lhs => out.lhs.push(source, line),
                Field::Rhs => out.rhs.push(source, line),
                Field::Orientation => {
                    let mut entry = SegmentedSource::default();
                    entry.push(source, line);
                    out.orientations.push(entry);
                    current = None;
                }
            }
        }
    }
    out
}

fn collect_indented_block_lines(lines: &[&str], start_index: usize) -> (Vec<String>, usize) {
    let mut out_lines: Vec<String> = Vec::new();
    let mut i = start_index;
    while i < lines.len() {
        let raw = lines[i];
        let trimmed = strip_comment(raw).trim_axi();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        if is_top_level_keyword(trimmed) {
            break;
        }

        out_lines.push(trimmed.to_string());
        i += 1;
    }
    (out_lines, i)
}

fn is_top_level_keyword(trimmed: &str) -> bool {
    matches!(
        trimmed,
        s if s.starts_with("schema ")
            || s.starts_with("theory ")
            || s.starts_with("instance ")
            || s.starts_with("module ")
            || s.starts_with("import ")
            || s.starts_with("aspect ")
            || s.starts_with("function ")
            || s.starts_with("constraint ")
            || s.starts_with("equation ")
            || s.starts_with("rewrite ")
    )
}

fn split_equation_slices(equation_text: &str) -> Result<(&str, &str), String> {
    let Some((lhs, rhs)) = equation_text.split_once('=') else {
        return Err("equation body must contain `=`".to_string());
    };
    let lhs = lhs.trim_axi();
    let rhs = rhs.trim_axi();
    if lhs.is_empty() || rhs.is_empty() {
        return Err("equation must have non-empty lhs and rhs".to_string());
    }
    Ok((lhs, rhs))
}

fn split_assignment(line: &str) -> Option<(&str, &str)> {
    let (lhs, rhs) = line.split_once('=')?;
    let lhs = lhs.trim_axi();
    let rhs = rhs.trim_axi();
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }
    Some((lhs, rhs))
}

fn parse_rewrite_rule(rule_name: &str, lines: &[String]) -> Result<RewriteRuleV1, String> {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Field {
        None,
        Vars,
        Lhs,
        Rhs,
        Orientation,
    }

    let mut current = Field::None;
    let mut vars_lines: Vec<String> = Vec::new();
    let mut lhs_lines: Vec<String> = Vec::new();
    let mut rhs_lines: Vec<String> = Vec::new();
    let mut orientation: Option<RewriteOrientationV1> = None;
    let mut seen_vars = false;
    let mut seen_lhs = false;
    let mut seen_rhs = false;
    let mut seen_orientation = false;

    for raw in lines {
        let line = raw.trim_axi();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("vars:") {
            if current == Field::Orientation {
                return Err(format!("rewrite `{rule_name}`: missing orientation value"));
            }
            if seen_vars {
                return Err(format!("rewrite `{rule_name}`: duplicate `vars:` field"));
            }
            seen_vars = true;
            current = Field::Vars;
            let rest = rest.trim_axi();
            if !rest.is_empty() {
                vars_lines.push(rest.to_string());
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("lhs:") {
            if current == Field::Orientation {
                return Err(format!("rewrite `{rule_name}`: missing orientation value"));
            }
            if seen_lhs {
                return Err(format!("rewrite `{rule_name}`: duplicate `lhs:` field"));
            }
            seen_lhs = true;
            current = Field::Lhs;
            let rest = rest.trim_axi();
            if !rest.is_empty() {
                lhs_lines.push(rest.to_string());
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("rhs:") {
            if current == Field::Orientation {
                return Err(format!("rewrite `{rule_name}`: missing orientation value"));
            }
            if seen_rhs {
                return Err(format!("rewrite `{rule_name}`: duplicate `rhs:` field"));
            }
            seen_rhs = true;
            current = Field::Rhs;
            let rest = rest.trim_axi();
            if !rest.is_empty() {
                rhs_lines.push(rest.to_string());
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("orientation:") {
            if current == Field::Orientation {
                return Err(format!("rewrite `{rule_name}`: missing orientation value"));
            }
            if seen_orientation {
                return Err(format!(
                    "rewrite `{rule_name}`: duplicate `orientation:` field"
                ));
            }
            seen_orientation = true;
            current = Field::Orientation;
            let rest = rest.trim_axi();
            if !rest.is_empty() {
                orientation = Some(parse_rewrite_orientation(rest)?);
                current = Field::None;
            }
            continue;
        }

        match current {
            Field::Vars => vars_lines.push(line.to_string()),
            Field::Lhs => lhs_lines.push(line.to_string()),
            Field::Rhs => rhs_lines.push(line.to_string()),
            Field::Orientation => {
                orientation = Some(parse_rewrite_orientation(line)?);
                current = Field::None;
            }
            Field::None => {
                return Err(format!(
                    "rewrite `{rule_name}`: unexpected line (expected vars/lhs/rhs): `{line}`"
                ));
            }
        }
    }

    if current == Field::Orientation {
        return Err(format!("rewrite `{rule_name}`: missing orientation value"));
    }
    if !seen_vars {
        return Err(format!("rewrite `{rule_name}`: missing `vars:`"));
    }

    let mut vars: Vec<RewriteVarDeclV1> = Vec::new();
    for line in vars_lines {
        vars.extend(parse_rewrite_var_decl_list_v1(&line)?);
    }

    let lhs_text = lhs_lines.join(" ");
    let rhs_text = rhs_lines.join(" ");
    if lhs_text.trim_axi().is_empty() {
        return Err(format!("rewrite `{rule_name}`: missing `lhs:`"));
    }
    if rhs_text.trim_axi().is_empty() {
        return Err(format!("rewrite `{rule_name}`: missing `rhs:`"));
    }

    let lhs = parse_path_expr_v3(&lhs_text)?;
    let rhs = parse_path_expr_v3(&rhs_text)?;

    Ok(RewriteRuleV1 {
        name: rule_name.to_string(),
        orientation: orientation.unwrap_or_default(),
        vars,
        lhs,
        rhs,
    })
}

fn parse_rewrite_orientation(s: &str) -> Result<RewriteOrientationV1, String> {
    match s.trim_axi() {
        "forward" => Ok(RewriteOrientationV1::Forward),
        "backward" => Ok(RewriteOrientationV1::Backward),
        "bidirectional" => Ok(RewriteOrientationV1::Bidirectional),
        other => Err(format!(
            "unknown rewrite orientation `{other}` (expected forward|backward|bidirectional)"
        )),
    }
}

/// Parse a comma-separated list of rewrite-rule variable declarations.
///
/// Examples:
/// - `x: Person, y: Person`
/// - `x: Person, y: Person, p: Path(x,y)`
/// - `p: Path(x, y)` (whitespace is flexible)
///
/// This parser is shared between:
/// - the canonical `.axi` parser (schema/theory surface), and
/// - meta-plane tooling that reads stored rewrite rules from PathDB.
#[derive(Debug)]
struct RewriteVarSourceMetadata {
    declaration: Range<usize>,
    name: Range<usize>,
    ty: Range<usize>,
    object_type: Option<Range<usize>>,
    path_from: Option<Range<usize>>,
    path_to: Option<Range<usize>>,
}

pub fn parse_rewrite_var_decl_list_v1(line: &str) -> Result<Vec<RewriteVarDeclV1>, String> {
    parse_rewrite_var_decl_list_with_source(line).map(|(variables, _)| variables)
}

fn parse_rewrite_var_decl_list_with_source(
    line: &str,
) -> Result<(Vec<RewriteVarDeclV1>, Vec<RewriteVarSourceMetadata>), String> {
    fn comma(input: &str) -> IResult<&str, ()> {
        let (input, _) = ascii_whitespace0(input)?;
        let (input, _) = pchar(',').parse(input)?;
        let (input, _) = ascii_whitespace0(input)?;
        Ok((input, ()))
    }

    type ParsedVarType<'a> = (
        RewriteVarTypeV1,
        Option<&'a str>,
        Option<(&'a str, &'a str)>,
    );

    fn var_type(input: &str) -> IResult<&str, ParsedVarType<'_>> {
        fn path_type(input: &str) -> IResult<&str, (&str, &str)> {
            let (input, _) = tag("Path").parse(input)?;
            alt((
                // Path(x,y), with optional canonical whitespace before `(`.
                preceded(
                    (ascii_whitespace0, pchar('(')),
                    (
                        preceded(ascii_whitespace0, parse_ident),
                        preceded(
                            (ascii_whitespace0, pchar(','), ascii_whitespace0),
                            parse_ident,
                        ),
                        preceded(ascii_whitespace0, pchar(')')),
                    ),
                )
                .map(|(from, to, _)| (from, to)),
                // Path x y. This alternative owns the required first separator.
                (
                    preceded(ascii_whitespace1, parse_ident),
                    preceded(ascii_whitespace1, parse_ident),
                ),
            ))
            .parse(input)
        }

        let (input, _) = ascii_whitespace0(input)?;
        if let Ok((remaining, (from, to))) = path_type(input) {
            Ok((
                remaining,
                (
                    RewriteVarTypeV1::Path {
                        from: from.to_string(),
                        to: to.to_string(),
                    },
                    None,
                    Some((from, to)),
                ),
            ))
        } else {
            let (input, ty) = parse_ident(input)?;
            Ok((
                input,
                (
                    RewriteVarTypeV1::Object { ty: ty.to_string() },
                    Some(ty),
                    None,
                ),
            ))
        }
    }

    type ParsedVar<'a> = (
        RewriteVarDeclV1,
        &'a str,
        &'a str,
        &'a str,
        Option<&'a str>,
        Option<(&'a str, &'a str)>,
    );

    fn var_decl(input: &str) -> IResult<&str, ParsedVar<'_>> {
        let (input, _) = ascii_whitespace0(input)?;
        let declaration_start = input;
        let (input, name) = parse_ident(input)?;
        let (input, _) = preceded(ascii_whitespace0, pchar(':')).parse(input)?;
        let type_start = input.trim_axi_start();
        let (input, (ty, object_type, path_ends)) = var_type(input)?;
        let type_source = &type_start[..type_start.len() - input.len()];
        let declaration = &declaration_start[..declaration_start.len() - input.len()];
        Ok((
            input,
            (
                RewriteVarDeclV1 {
                    name: name.to_string(),
                    ty,
                },
                declaration,
                name,
                type_source,
                object_type,
                path_ends,
            ),
        ))
    }

    fn parser(input: &str) -> IResult<&str, Vec<ParsedVar<'_>>> {
        let (input, decls) = separated_list1(comma, var_decl).parse(input)?;
        let (input, _) = ascii_whitespace0(input)?;
        Ok((input, decls))
    }

    let trimmed = line.trim_axi();
    if trimmed.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    all_consuming(parser)
        .parse(trimmed)
        .map(|(_, parsed)| {
            parsed
                .into_iter()
                .map(
                    |(variable, declaration, name, ty, object_type, path_ends)| {
                        let source = RewriteVarSourceMetadata {
                            declaration: relative_range(trimmed, declaration),
                            name: relative_range(trimmed, name),
                            ty: relative_range(trimmed, ty),
                            object_type: object_type.map(|ty| relative_range(trimmed, ty)),
                            path_from: path_ends.map(|(from, _)| relative_range(trimmed, from)),
                            path_to: path_ends.map(|(_, to)| relative_range(trimmed, to)),
                        };
                        (variable, source)
                    },
                )
                .unzip()
        })
        .map_err(|_| {
            format!("invalid rewrite vars line: `{trimmed}` (expected `x: Ty, p: Path(x,y)` etc)")
        })
}

#[derive(Debug)]
struct PathSourceMetadata {
    nodes: Vec<(Vec<RewritePathStepV1>, Range<usize>)>,
    terms: Vec<(Vec<RewritePathStepV1>, RewritePathTermV1, Range<usize>)>,
}

fn prefix_path_metadata(metadata: &mut PathSourceMetadata, step: RewritePathStepV1) {
    for (path, _) in &mut metadata.nodes {
        path.insert(0, step);
    }
    for (path, _, _) in &mut metadata.terms {
        path.insert(0, step);
    }
}

pub fn parse_path_expr_v3(text: &str) -> Result<PathExprV3, String> {
    parse_path_expr_v3_with_source(text).map(|(path, _)| path)
}

fn parse_path_expr_v3_with_source(text: &str) -> Result<(PathExprV3, PathSourceMetadata), String> {
    fn comma(input: &str) -> IResult<&str, ()> {
        let (input, _) = ascii_whitespace0(input)?;
        let (input, _) = pchar(',').parse(input)?;
        let (input, _) = ascii_whitespace0(input)?;
        Ok((input, ()))
    }

    fn parens<'a, O>(
        mut inner: impl Parser<&'a str, Output = O, Error = nom::error::Error<&'a str>>,
    ) -> impl FnMut(&'a str) -> IResult<&'a str, O> {
        move |input: &'a str| {
            let (input, _) = ascii_whitespace0(input)?;
            let (input, _) = pchar('(').parse(input)?;
            let (input, out) = inner.parse(input)?;
            let (input, _) = ascii_whitespace0(input)?;
            let (input, _) = pchar(')').parse(input)?;
            Ok((input, out))
        }
    }

    type ParsedPath = (PathExprV3, PathSourceMetadata);

    fn expr<'a>(root: &'a str, input: &'a str) -> IResult<&'a str, ParsedPath> {
        let (input, _) = ascii_whitespace0(input)?;
        let is_call = |name: &str| {
            input
                .strip_prefix(name)
                .is_some_and(|rest| rest.trim_axi_start().starts_with('('))
        };
        if is_call("refl") {
            return refl_expr(root, input);
        }
        if is_call("step") {
            return step_expr(root, input);
        }
        if is_call("trans") {
            return trans_expr(root, input);
        }
        if is_call("inv") {
            return inv_expr(root, input);
        }
        var_expr(root, input)
    }

    fn var_expr<'a>(root: &'a str, input: &'a str) -> IResult<&'a str, ParsedPath> {
        let start = input;
        let (input, name) = parse_ident(input)?;
        Ok((
            input,
            (
                PathExprV3::Var {
                    name: name.to_string(),
                },
                PathSourceMetadata {
                    nodes: vec![(
                        Vec::new(),
                        relative_range(root, &start[..start.len() - input.len()]),
                    )],
                    terms: vec![(
                        Vec::new(),
                        RewritePathTermV1::Variable,
                        relative_range(root, name),
                    )],
                },
            ),
        ))
    }

    fn refl_expr<'a>(root: &'a str, input: &'a str) -> IResult<&'a str, ParsedPath> {
        let start = input;
        let (input, _) = tag("refl").parse(input)?;
        let (input, entity) = parens(preceded(ascii_whitespace0, parse_ident)).parse(input)?;
        Ok((
            input,
            (
                PathExprV3::Reflexive {
                    entity: entity.to_string(),
                },
                PathSourceMetadata {
                    nodes: vec![(
                        Vec::new(),
                        relative_range(root, &start[..start.len() - input.len()]),
                    )],
                    terms: vec![(
                        Vec::new(),
                        RewritePathTermV1::ReflexiveEntity,
                        relative_range(root, entity),
                    )],
                },
            ),
        ))
    }

    fn step_expr<'a>(root: &'a str, input: &'a str) -> IResult<&'a str, ParsedPath> {
        let start = input;
        let (input, _) = tag("step").parse(input)?;
        let (input, (from, rel, to)) = parens((
            preceded(ascii_whitespace0, parse_ident),
            preceded(comma, parse_ident),
            preceded(comma, parse_ident),
        ))
        .parse(input)?;
        Ok((
            input,
            (
                PathExprV3::Step {
                    from: from.to_string(),
                    rel: rel.to_string(),
                    to: to.to_string(),
                },
                PathSourceMetadata {
                    nodes: vec![(
                        Vec::new(),
                        relative_range(root, &start[..start.len() - input.len()]),
                    )],
                    terms: vec![
                        (
                            Vec::new(),
                            RewritePathTermV1::StepFrom,
                            relative_range(root, from),
                        ),
                        (
                            Vec::new(),
                            RewritePathTermV1::StepRelation,
                            relative_range(root, rel),
                        ),
                        (
                            Vec::new(),
                            RewritePathTermV1::StepTo,
                            relative_range(root, to),
                        ),
                    ],
                },
            ),
        ))
    }

    fn trans_expr<'a>(root: &'a str, input: &'a str) -> IResult<&'a str, ParsedPath> {
        let start = input;
        let (input, _) = tag("trans").parse(input)?;
        let (input, _) = ascii_whitespace0(input)?;
        let (input, _) = pchar('(').parse(input)?;
        let (input, (left, mut left_source)) = expr(root, input)?;
        let (input, _) = comma(input)?;
        let (input, (right, mut right_source)) = expr(root, input)?;
        let (input, _) = ascii_whitespace0(input)?;
        let (input, _) = pchar(')').parse(input)?;
        prefix_path_metadata(&mut left_source, RewritePathStepV1::TransLeft);
        prefix_path_metadata(&mut right_source, RewritePathStepV1::TransRight);
        let mut source = PathSourceMetadata {
            nodes: vec![(
                Vec::new(),
                relative_range(root, &start[..start.len() - input.len()]),
            )],
            terms: Vec::new(),
        };
        source.nodes.extend(left_source.nodes);
        source.nodes.extend(right_source.nodes);
        source.terms.extend(left_source.terms);
        source.terms.extend(right_source.terms);
        Ok((
            input,
            (
                PathExprV3::Trans {
                    left: Box::new(left),
                    right: Box::new(right),
                },
                source,
            ),
        ))
    }

    fn inv_expr<'a>(root: &'a str, input: &'a str) -> IResult<&'a str, ParsedPath> {
        let start = input;
        let (input, _) = tag("inv").parse(input)?;
        let (input, _) = ascii_whitespace0(input)?;
        let (input, _) = pchar('(').parse(input)?;
        let (input, (path, mut path_source)) = expr(root, input)?;
        let (input, _) = ascii_whitespace0(input)?;
        let (input, _) = pchar(')').parse(input)?;
        prefix_path_metadata(&mut path_source, RewritePathStepV1::InvPath);
        let mut source = PathSourceMetadata {
            nodes: vec![(
                Vec::new(),
                relative_range(root, &start[..start.len() - input.len()]),
            )],
            terms: Vec::new(),
        };
        source.nodes.extend(path_source.nodes);
        source.terms.extend(path_source.terms);
        Ok((
            input,
            (
                PathExprV3::Inv {
                    path: Box::new(path),
                },
                source,
            ),
        ))
    }

    let root = text.trim_axi();
    all_consuming(|input| expr(root, input))
        .parse(root)
        .map(|(_, value)| value)
        .map_err(|_| format!("invalid path expression: `{root}`"))
}

fn collect_balanced_braces(
    source: &str,
    lines: &[&str],
    start_index: usize,
    first_rhs: &str,
) -> Result<(String, usize, SourceSegments), String> {
    let mut depth: i32 = 0;
    let mut combined = String::new();
    let mut segments = Vec::new();

    // Start with the RHS on the current line (after `=`).
    {
        let rhs = strip_comment(first_rhs).trim_axi();
        segments.push((
            0..rhs.len(),
            rhs.as_ptr() as usize - source.as_ptr() as usize,
        ));
        combined.push_str(rhs);
        for ch in rhs.chars() {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
            }
        }
    }

    let mut i = start_index + 1;
    while i < lines.len() && depth > 0 {
        let line = strip_comment(lines[i]).trim_axi();
        if !line.is_empty() {
            combined.push(' ');
            segments.push((
                combined.len()..combined.len() + line.len(),
                line.as_ptr() as usize - source.as_ptr() as usize,
            ));
            combined.push_str(line);
            for ch in line.chars() {
                if ch == '{' {
                    depth += 1;
                } else if ch == '}' {
                    depth -= 1;
                }
            }
        }
        i += 1;
    }

    if depth != 0 {
        return Err("unclosed `{ ... }` block".to_string());
    }
    Ok((combined, i, segments))
}

#[derive(Debug)]
struct SetItemSourceMetadata {
    declaration: Range<usize>,
    ident: Option<Range<usize>>,
    label: Option<Range<usize>>,
    fields: Vec<(Range<usize>, Range<usize>)>,
}

fn parse_set_literal_with_source(
    text: &str,
) -> Result<(SetLiteralV1, Vec<SetItemSourceMetadata>), String> {
    let text = text.trim_axi();
    if !text.starts_with('{') || !text.ends_with('}') {
        return Err("expected set literal `{ ... }`".to_string());
    }
    let inner = text[1..text.len() - 1].trim_axi();
    let item_slices = if inner.is_empty() {
        Vec::new()
    } else {
        reject_empty_finite_elements(split_top_level_commas(inner), "set literal")?
    };
    let parsed = item_slices
        .iter()
        .map(|item| parse_set_item_with_source(text, item))
        .collect::<Result<Vec<_>, _>>()?;
    let (items, metadata): (Vec<_>, Vec<_>) = parsed.into_iter().unzip();
    Ok((SetLiteralV1 { items }, metadata))
}

fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut paren_depth: i32 = 0;
    for (idx, ch) in s.char_indices() {
        match ch {
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            ',' if paren_depth == 0 => {
                parts.push(&s[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

fn parse_set_item_with_source(
    root: &str,
    item: &str,
) -> Result<(SetItemV1, SetItemSourceMetadata), String> {
    let trimmed = item.trim_axi();
    let declaration = relative_range(root, trimmed);
    let (label, tuple_text) = if trimmed.starts_with('(') {
        (None, trimmed)
    } else if let Some((label, tuple)) = trimmed.split_once(':') {
        let label = label.trim_axi();
        parse_identifier_text(label, "fact label")?;
        (Some(label), tuple.trim_axi())
    } else {
        (None, trimmed)
    };
    if tuple_text.starts_with('(') && tuple_text.ends_with(')') {
        let inner = tuple_text[1..tuple_text.len() - 1].trim_axi();
        let field_slices =
            reject_empty_finite_elements(split_top_level_commas(inner), "tuple field list")?;
        let mut fields = Vec::new();
        for part in &field_slices {
            let Some((k, v)) = part.split_once('=') else {
                return Err(format!("tuple field missing `=`: `{part}`"));
            };
            let key = k.trim_axi();
            let value = v.trim_axi();
            parse_identifier_text(key, "tuple role")?;
            parse_value_atom_text(value, "tuple value")?;
            fields.push((key.to_string(), value.to_string()));
        }
        let field_sources = field_slices
            .into_iter()
            .map(|part| {
                let (key, value) = part.split_once('=').expect("validated tuple field");
                (
                    relative_range(root, key.trim_axi()),
                    relative_range(root, value.trim_axi()),
                )
            })
            .collect();
        return Ok((
            SetItemV1::Tuple {
                label: label.map(str::to_string),
                fields,
            },
            SetItemSourceMetadata {
                declaration,
                ident: None,
                label: label.map(|label| relative_range(root, label)),
                fields: field_sources,
            },
        ));
    }
    if label.is_some() {
        return Err("fact label must prefix a relation tuple".to_string());
    }
    parse_value_atom_text(trimmed, "set element")?;
    Ok((
        SetItemV1::Ident {
            name: trimmed.to_string(),
        },
        SetItemSourceMetadata {
            declaration: declaration.clone(),
            ident: Some(declaration),
            label: None,
            fields: Vec::new(),
        },
    ))
}

// ============================================================================
// Tests (parsing the canonical corpus)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .expect("canonicalize repo root")
    }

    #[test]
    fn parses_economic_flows_schema_v1() {
        let text =
            std::fs::read_to_string(repo_root().join("examples/economics/EconomicFlows.axi"))
                .expect("read EconomicFlows.axi");
        let module = parse_schema_v1(&text).expect("parse schema v1");
        assert_eq!(module.module_name, "EconomicFlows");
        assert!(!module.schemas.is_empty());
        assert!(module.schemas.iter().any(|s| s.name == "Economy"));
        assert!(module.instances.iter().any(|i| i.name == "SimpleEconomy"));
    }

    #[test]
    fn parses_schema_evolution_schema_v1() {
        let text =
            std::fs::read_to_string(repo_root().join("examples/ontology/SchemaEvolution.axi"))
                .expect("read SchemaEvolution.axi");
        let module = parse_schema_v1(&text).expect("parse schema v1");
        assert_eq!(module.module_name, "SchemaEvolution");
        assert!(module.schemas.iter().any(|s| s.name == "OntologyMeta"));
        assert!(module.instances.iter().any(|i| i.name == "ProductCatalog"));
    }

    #[test]
    fn rejects_missing_late_or_multiple_module_headers() {
        let missing = r#"
schema S:
  object A
"#;
        let err = parse_schema_v1(missing).expect_err("missing module header must reject");
        assert!(err
            .to_string()
            .contains("module header must be the first canonical header"));

        let late = r#"
schema S:
  object A
module Late
"#;
        let err = parse_schema_v1(late).expect_err("late first module header must reject");
        assert!(err
            .to_string()
            .contains("module header must be the first canonical header"));

        let multiple = r#"
module Left
schema L:
  object A
module Right
schema R:
  object B
"#;
        let err = parse_schema_v1(multiple).expect_err("multiple module headers must reject");
        assert!(err.to_string().contains("exactly one module header"));
        assert!(err.to_string().contains("first header was on line 2"));
    }

    #[test]
    fn rejects_non_identifier_module_header() {
        let err = parse_schema_v1("module Left Right\n")
            .expect_err("module header must carry exactly one validated identifier");
        assert!(err.to_string().contains("module header expects exactly"));
    }

    #[test]
    fn rejects_malformed_schema_header_and_attached_role_kind() {
        let err = parse_schema_v1("module BadSchema\nschema S::\n  object A\n")
            .expect_err("a schema header must be all-consuming");
        assert!(err.to_string().contains("schema header expects"));

        let err = parse_relation_decl_v1("relation R(a:A@data)")
            .expect_err("a role-kind annotation requires separating whitespace");
        assert!(err.contains("requires canonical separating whitespace"));
    }

    #[test]
    fn recursive_calls_accept_ascii_whitespace_inside_parentheses() {
        for ty in [
            "relation( A )",
            "indexed( A; role )",
            "refined( A; eq(A) )",
            "refined(A; eq( A ))",
            "refined(A; in( A | B ))",
            "refined(A; enum( A | B ))",
            "refined(A; key( left | right ))",
            "refined(A; cardinality( 0 | 1 ))",
            "refined(A; predicate( non_empty | left ))",
        ] {
            parse_type_expr_v1(ty)
                .expect("recursive calls admit the complete ASCII syntax-whitespace set");
        }
    }

    #[test]
    fn rewrite_path_type_backtracks_to_complete_object_identifiers() {
        let variables = parse_rewrite_var_decl_list_v1(
            "a: Path, b: Pathology, x: A, y: A, p: Path(x,y), q: Path x y",
        )
        .expect("both complete Path forms parse while bare prefixes remain object identifiers");
        assert!(matches!(
            &variables[0].ty,
            RewriteVarTypeV1::Object { ty } if ty == "Path"
        ));
        assert!(matches!(
            &variables[1].ty,
            RewriteVarTypeV1::Object { ty } if ty == "Pathology"
        ));
        assert!(matches!(
            &variables[4].ty,
            RewriteVarTypeV1::Path { from, to } if from == "x" && to == "y"
        ));
        assert!(matches!(
            &variables[5].ty,
            RewriteVarTypeV1::Path { from, to } if from == "x" && to == "y"
        ));
    }

    #[test]
    fn u32_tokens_are_nonempty_ascii_digits_without_signs() {
        for accepted in ["0", "1", "4294967295"] {
            parse_type_expr_v1(&format!("refined(A;cardinality(0|{accepted}))"))
                .expect("canonical u32 token must parse");
            parse_constraint_v1(&format!("at_most {accepted} R.left -> R.right"))
                .expect("canonical at_most u32 token must parse");
        }
        for rejected in ["", "+1", "-1", "1 0", "١", "4294967296"] {
            parse_type_expr_v1(&format!("refined(A;cardinality(0|{rejected}))"))
                .expect_err("noncanonical cardinality token must reject");
            parse_constraint_v1(&format!("at_most {rejected} R.left -> R.right"))
                .expect_err("noncanonical at_most token must reject");
        }
    }

    #[test]
    fn reversible_marker_uses_canonical_ascii_separator_whitespace() {
        for separator in [" ", "\t", "\r"] {
            let source = format!(
                "module M\nschema S\n  object A\n  function f: A -> A{separator}@reversible\n"
            );
            let module = parse_schema_v1(&source)
                .expect("every canonical ASCII separator must admit @reversible");
            assert!(module.schemas[0].generators[0].reversible);
        }
        parse_schema_v1(
            "module M\nschema S\n  object A\n  function f: A -> A\u{000b}@reversible\n",
        )
        .expect_err("noncanonical whitespace must not separate @reversible");
    }

    #[test]
    fn theory_item_headers_are_all_consuming_with_at_most_one_colon() {
        let accepted = [
            "  equation e\n    opaque(A) = opaque(A)\n",
            "  equation e:\n    opaque(A) = opaque(A)\n",
            "  rewrite r\n    vars: x: A\n    lhs: refl(x)\n    rhs: refl(x)\n",
            "  rewrite r:\n    vars: x: A\n    lhs: refl(x)\n    rhs: refl(x)\n",
            "  constraint Review\n    opaque body\n",
            "  constraint Review:\n    opaque body\n",
        ];
        for item in accepted {
            let source = format!("module M\nschema S\n  object A\ntheory T on S\n{item}");
            parse_schema_v1(&source).expect("zero or one header colon must parse");
        }

        let rejected = [
            "  equation e::\n    opaque(A) = opaque(A)\n",
            "  rewrite r::\n    vars: x: A\n    lhs: refl(x)\n    rhs: refl(x)\n",
            "  constraint Review::\n    opaque body\n",
        ];
        for item in rejected {
            let source = format!("module M\nschema S\n  object A\ntheory T on S\n{item}");
            let error = parse_schema_v1(&source).expect_err("repeated colon must reject");
            assert!(error.to_string().contains("at most one trailing"));
        }

        for item in [
            "  constraint Review: trailing\n",
            "  constraint Review extra:\n",
        ] {
            let source = format!("module M\nschema S\n  object A\ntheory T on S\n{item}");
            let error = parse_schema_v1(&source)
                .expect_err("malformed named-constraint candidate must reject");
            assert!(error.to_string().contains("constraint header expects"));
        }
    }

    #[test]
    fn recognized_constraint_dispatch_uses_ascii_keyword_boundaries() {
        let valid = [
            ("functional", "R.left -> R.right"),
            ("at_most", "1 R.left -> R.right"),
            ("typing", "R: rule_name"),
            ("symmetric", "R"),
            ("transitive", "R"),
            ("key", "R(left)"),
        ];
        let malformed = [
            ("functional", "R.left ->"),
            ("at_most", "x R.left -> R.right"),
            ("typing", "R:"),
            ("symmetric", "R where"),
            ("transitive", "R extra"),
            ("key", "R()"),
        ];
        for separator in [" ", "\t", "\n", "\r"] {
            for (family, tail) in valid {
                let parsed = parse_constraint_v1(&format!("{family}{separator}{tail}"))
                    .expect("recognized family with canonical boundary must parse");
                assert!(!matches!(parsed, ConstraintV1::Unknown { .. }));
            }
            for (family, tail) in malformed {
                parse_constraint_v1(&format!("{family}{separator}{tail}"))
                    .expect_err("malformed recognized family must not downgrade to opaque text");
            }
        }
        for separator in ["\u{000b}", "\u{000c}", "\u{00a0}", "\u{2003}"] {
            for (family, tail) in valid {
                parse_constraint_v1(&format!("{family}{separator}{tail}")).expect_err(
                    "noncanonical family whitespace must reject, not become opaque text",
                );
            }
        }
    }

    #[test]
    fn theory_key_owns_ascii_whitespace_around_its_field_list() {
        for separator in [" ", "\t", "\n", "\r"] {
            for body in [
                format!("key R{separator}(left,right)"),
                format!("key R({separator}left,right)"),
                format!("key R(left{separator},right)"),
                format!("key R(left,{separator}right)"),
                format!("key R(left,right{separator})"),
            ] {
                let parsed = parse_constraint_v1(&body)
                    .expect("theory-key delimiters own canonical ASCII whitespace");
                assert_eq!(
                    parsed,
                    ConstraintV1::Key {
                        relation: "R".to_string(),
                        fields: vec!["left".to_string(), "right".to_string()],
                    }
                );
            }
            parse_constraint_v1(&format!("key R(left{separator},,right)"))
                .expect_err("theory-key field lists must retain empty elements as errors");
        }
        for separator in ["\u{000b}", "\u{000c}", "\u{00a0}", "\u{2003}"] {
            parse_constraint_v1(&format!("key R({separator}left,right)"))
                .expect_err("noncanonical whitespace must not enter a theory-key field list");
        }
    }

    #[test]
    fn qualified_constraint_fields_require_dot_adjacency() {
        let prefix = "module M\nschema S\n  object A\n  object Kind\n  relation R(left:A,right:A,kind:Kind)\ntheory T on S\n";
        for constraint in [
            "  constraint functional R .left -> R.right\n",
            "  constraint functional R.left -> R. right\n",
            "  constraint at_most 1 R .left -> R.right\n",
            "  constraint at_most 1 R.left -> R. right\n",
            "  constraint symmetric R where R .kind in {friend}\n",
            "  constraint symmetric R where R. kind in {friend}\n",
        ] {
            parse_schema_v1(&format!("{prefix}{constraint}"))
                .expect_err("qualified constraint fields must be adjacent to the dot");
        }
    }

    #[test]
    fn set_value_atoms_preserve_leading_zero_and_digit_prefix() {
        let module = parse_schema_v1(
            "module M\nschema S\n  object Thing\ninstance I of S\n  Thing = {001, 1abc}\n",
        )
        .expect("value atoms may start with digits");
        let items = &module.instances[0].assignments[0].value.items;
        assert!(matches!(&items[0], SetItemV1::Ident { name } if name == "001"));
        assert!(matches!(&items[1], SetItemV1::Ident { name } if name == "1abc"));
    }

    #[test]
    fn finite_list_parsers_reject_empty_elements() {
        let rejected = [
            "relation R(x: refined(A; in(A|)))",
            "relation R(x: refined(A; enum(A||A)))",
            "relation R(a: A,)",
        ];
        for declaration in rejected {
            parse_relation_decl_v1(declaration).expect_err("malformed finite list must reject");
        }

        let prefix = "module M\nschema S\n  object A\ninstance I of S\n";
        for assignment in ["  A = {,a}\n", "  A = {a,,b}\n", "  A = {a,}\n"] {
            parse_schema_v1(&format!("{prefix}{assignment}"))
                .expect_err("malformed set list must reject");
        }
    }

    #[test]
    fn syntax_whitespace_is_the_explicit_ascii_set() {
        for boundary in [' ', '\t', '\r'] {
            parse_schema_v1(&format!(
                "{boundary}module M{boundary}\nschema S\n  object A\n"
            ))
            .expect("canonical ASCII boundary whitespace must parse");
        }
        for boundary in ['\u{000b}', '\u{000c}', '\u{00a0}', '\u{2003}'] {
            parse_schema_v1(&format!("{boundary}module M\nschema S\n  object A\n"))
                .expect_err("noncanonical boundary whitespace must reject");
        }
        parse_schema_v1(
            "module M\nschema S\n  object A\ntheory T on S\n  constraint Review:\n    café 東京 λ\n",
        )
        .expect("supported Unicode opaque text must remain accepted");
    }

    #[test]
    fn relation_roles_preserve_explicit_axes_and_type_expressions() {
        let relation = parse_relation_decl_v1(
            "relation Depends(ctx: Context @context, flow: indexed(relation(Flow); ctx) @data)",
        )
        .expect("parse relation declaration");
        assert_eq!(relation.fields[0].kind, RoleKindV1::Context);
        assert!(matches!(relation.fields[1].ty, TypeExprV1::Indexed { .. }));
        assert_eq!(
            format_relation_decl_v1(&relation),
            "relation Depends(ctx: Context @context, flow: indexed(relation(Flow); ctx) @data)"
        );
    }

    #[test]
    fn relation_level_axis_shorthand_is_rejected() {
        let err = parse_relation_decl_v1(
            "relation Parent(child: Person, parent: Person) @context Context",
        )
        .expect_err("relation-level shorthand must not survive the greenfield grammar");
        assert!(err.contains("relation-level axis shorthands are not canonical"));
    }

    #[test]
    fn parses_imports_generators_refinements_and_fact_labels() {
        let module = parse_schema_v1(
            r#"module Root
import Base
schema S:
  object Person
  object Context
  relation R(ctx: Context @context, person: refined(indexed(Person; ctx); enum(Alice|Bob)) @data)
  function owner: Person -> Person @reversible
instance I of S:
  Person = {Alice, Bob}
  Context = {Current}
  R = { fact1: (ctx=Current, person=Alice) }
  owner = {(source=Alice, target=Alice), (source=Bob, target=Bob)}
"#,
        )
        .expect("parse full canonical surface");
        assert_eq!(module.imports, vec!["Base"]);
        assert_eq!(module.schemas[0].generators.len(), 1);
        assert!(matches!(
            module.instances[0].assignments[2].value.items[0],
            SetItemV1::Tuple { label: Some(_), .. }
        ));
    }

    #[test]
    fn subtype_uses_one_canonical_operator_and_optional_named_inclusion() {
        let subtype = parse_subtype_decl_v1("Child < Parent").expect("parse subtype declaration");
        assert_eq!(format_subtype_decl_v1(&subtype), "subtype Child < Parent");
        let named = parse_subtype_decl_v1("Child < Parent as child_to_parent")
            .expect("parse semantically named inclusion");
        assert_eq!(
            format_subtype_decl_v1(&named),
            "subtype Child < Parent as child_to_parent"
        );
        for rejected in [
            "Child <: Parent",
            "Child <: Parent as child_to_parent",
            "Child << Parent",
            "Child <= Parent",
            "Child : Parent",
            "Child < Parent as",
            "Child < Parent child_to_parent",
            "Child < Parent as child_to_parent as duplicate",
        ] {
            parse_subtype_decl_v1(rejected)
                .expect_err("undeclared subtype operator/inclusion combinations must reject");
        }
    }

    #[test]
    fn syntax_limit_scan_ignores_canonical_comments() {
        let module = parse_schema_v1(
            "module Comments\n-- unmatched comment delimiters ({[\n# unmatched comment quote \\\"\nschema S:\n  object A -- unmatched )]}\n",
        )
        .expect("comment contents must not affect syntax-depth validation");
        assert_eq!(module.module_name, "Comments");
    }

    #[test]
    fn syntax_limit_scan_rejects_excessive_nesting_before_parse() {
        let source = format!(
            "module Deep\n{}{}\n",
            "(".repeat(MAX_AXI_SYNTAX_DEPTH + 1),
            ")".repeat(MAX_AXI_SYNTAX_DEPTH + 1)
        );
        let err = parse_schema_v1(&source).expect_err("excessive syntax nesting must reject");
        assert!(err.to_string().contains("syntax nesting exceeds"));
    }
}
