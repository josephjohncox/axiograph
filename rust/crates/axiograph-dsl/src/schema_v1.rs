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
    character::complete::{char as pchar, multispace0, multispace1},
    combinator::{all_consuming, opt, recognize},
    multi::separated_list1,
    sequence::{preceded, tuple},
    IResult, Parser,
};
use serde::{Deserialize, Serialize};
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

pub fn parse_schema_v1(text: &str) -> Result<SchemaV1Module, SchemaV1ParseError> {
    validate_axi_resource_limits(text)?;
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
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            i += 1;
            continue;
        }

        // ------------------------------------------------------------------
        // Section headers
        // ------------------------------------------------------------------
        if let Some(rest) = line.strip_prefix("module ").map(str::trim) {
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
            module_header_line = Some(line_no);
            module.module_name = name;
            section = Section::None;
            i += 1;
            continue;
        }

        if let Some(rest) = line.strip_prefix("import ").map(str::trim) {
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
            module.imports.push(import);
            i += 1;
            continue;
        }

        if let Some(rest) = line.strip_prefix("schema ").map(str::trim) {
            let name = rest.trim_end_matches(':').trim();
            if name.is_empty() {
                return Err(SchemaV1ParseError::Line {
                    line: line_no,
                    message: "schema name missing".to_string(),
                });
            }
            module.schemas.push(SchemaV1Schema {
                name: name.to_string(),
                objects: vec![],
                subtypes: vec![],
                relations: vec![],
                generators: vec![],
            });
            section = Section::Schema(module.schemas.len() - 1);
            i += 1;
            continue;
        }

        if let Some(rest) = line.strip_prefix("theory ").map(str::trim) {
            let (name, schema) =
                parse_theory_header(rest).map_err(|message| SchemaV1ParseError::Line {
                    line: line_no,
                    message,
                })?;
            module.theories.push(SchemaV1Theory {
                name,
                schema,
                constraints: vec![],
                equations: vec![],
                rewrite_rules: vec![],
            });
            section = Section::Theory(module.theories.len() - 1);
            i += 1;
            continue;
        }

        if let Some(rest) = line.strip_prefix("instance ").map(str::trim) {
            let (name, schema) =
                parse_instance_header(rest).map_err(|message| SchemaV1ParseError::Line {
                    line: line_no,
                    message,
                })?;
            module.instances.push(SchemaV1Instance {
                name,
                schema,
                assignments: vec![],
            });
            section = Section::Instance(module.instances.len() - 1);
            i += 1;
            continue;
        }

        // ------------------------------------------------------------------
        // Section bodies
        // ------------------------------------------------------------------
        match section {
            Section::Schema(schema_index) => {
                if let Some(name) = line
                    .strip_prefix("object ")
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    module.schemas[schema_index].objects.push(name.to_string());
                    i += 1;
                    continue;
                }

                if let Some(rest) = line.strip_prefix("subtype ").map(str::trim) {
                    let subtype =
                        parse_subtype_decl(rest).map_err(|message| SchemaV1ParseError::Line {
                            line: line_no,
                            message,
                        })?;
                    module.schemas[schema_index].subtypes.push(subtype);
                    i += 1;
                    continue;
                }

                if line.starts_with("relation ") {
                    let (combined, next_index) =
                        collect_balanced_parens(lines.as_slice(), i, "relation").map_err(
                            |message| SchemaV1ParseError::Line {
                                line: line_no,
                                message,
                            },
                        )?;
                    let relation = parse_relation_decl(&combined).map_err(|message| {
                        SchemaV1ParseError::Line {
                            line: line_no,
                            message,
                        }
                    })?;
                    module.schemas[schema_index].relations.push(relation);
                    i = next_index;
                    continue;
                }

                if line.starts_with("aspect ") || line.starts_with("function ") {
                    let generator =
                        parse_generator_decl(line).map_err(|message| SchemaV1ParseError::Line {
                            line: line_no,
                            message,
                        })?;
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
                if let Some(rest) = line.strip_prefix("constraint ").map(str::trim) {
                    let rest = rest.trim();
                    // Named constraint blocks:
                    //   `constraint Name:` followed by an indented body.
                    //
                    // We keep these as first-class (but opaque) structured data so
                    // they remain visible to tooling, even when the runtime doesn't
                    // execute them yet.
                    if rest.ends_with(':') {
                        let name = rest.trim_end_matches(':').trim();
                        if name.is_empty() {
                            return Err(SchemaV1ParseError::Line {
                                line: line_no,
                                message: "constraint name missing".to_string(),
                            });
                        }
                        let (body, next_index) =
                            collect_indented_block_lines(lines.as_slice(), i + 1);
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
                    let (extra, next_index) = collect_indented_block(lines.as_slice(), i + 1);
                    let combined = if extra.is_empty() {
                        rest.to_string()
                    } else {
                        format!("{rest} {extra}")
                    };

                    let constraint = parse_constraint(&combined).map_err(|message| {
                        SchemaV1ParseError::Line {
                            line: line_no,
                            message,
                        }
                    })?;
                    module.theories[theory_index].constraints.push(constraint);
                    i = if extra.is_empty() { i + 1 } else { next_index };
                    continue;
                }

                if let Some(rest) = line.strip_prefix("equation ").map(str::trim) {
                    let equation_name = rest.trim_end_matches(':').trim();
                    if equation_name.is_empty() {
                        return Err(SchemaV1ParseError::Line {
                            line: line_no,
                            message: "equation name missing".to_string(),
                        });
                    }

                    let (equation_text, next_index) =
                        collect_indented_block(lines.as_slice(), i + 1);
                    let (lhs, rhs) = split_equation(&equation_text).map_err(|message| {
                        SchemaV1ParseError::Line {
                            line: line_no,
                            message,
                        }
                    })?;

                    module.theories[theory_index].equations.push(EquationV1 {
                        name: equation_name.to_string(),
                        lhs,
                        rhs,
                    });

                    i = next_index;
                    continue;
                }

                if let Some(rest) = line.strip_prefix("rewrite ").map(str::trim) {
                    let rule_name = rest.trim_end_matches(':').trim();
                    if rule_name.is_empty() {
                        return Err(SchemaV1ParseError::Line {
                            line: line_no,
                            message: "rewrite rule name missing".to_string(),
                        });
                    }

                    let (block_lines, next_index) =
                        collect_indented_block_lines(lines.as_slice(), i + 1);
                    let rule = parse_rewrite_rule(rule_name, &block_lines).map_err(|message| {
                        SchemaV1ParseError::Line {
                            line: line_no,
                            message,
                        }
                    })?;
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
                    let (set_text, next_index) = collect_balanced_braces(lines.as_slice(), i, rhs)
                        .map_err(|message| SchemaV1ParseError::Line {
                            line: line_no,
                            message,
                        })?;

                    let set_literal = parse_set_literal(&set_text).map_err(|message| {
                        SchemaV1ParseError::Line {
                            line: line_no,
                            message,
                        }
                    })?;

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

    Ok(module)
}

fn strip_comment(line: &str) -> &str {
    if let Some((before, _)) = line.split_once('#') {
        return before;
    }
    line.split_once("--").map(|(a, _)| a).unwrap_or(line)
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn parse_ident(input: &str) -> IResult<&str, &str> {
    recognize(tuple((
        take_while1(is_ident_start),
        take_while(is_ident_continue),
    )))(input)
}

fn parse_module_header(rest: &str) -> Result<Name, String> {
    all_consuming(preceded(multispace0, tuple((parse_ident, multispace0))))(rest)
        .map(|(_, (name, _))| name.to_string())
        .map_err(|_| "module header expects exactly `module <Name>`".to_string())
}

fn parse_theory_header(rest: &str) -> Result<(Name, Name), String> {
    fn parser(input: &str) -> IResult<&str, (Name, Name)> {
        let (input, name) = parse_ident(input)?;
        let (input, _) = multispace1(input)?;
        let (input, _) = tag("on")(input)?;
        let (input, _) = multispace1(input)?;
        let (input, schema) = parse_ident(input)?;
        let (input, _) = multispace0(input)?;
        let (input, _) = opt(pchar(':'))(input)?;
        let (input, _) = multispace0(input)?;
        Ok((input, (name.to_string(), schema.to_string())))
    }

    all_consuming(parser)(rest.trim())
        .map(|(_, v)| v)
        .map_err(|_| "theory header expects: `theory <Name> on <Schema>:`".to_string())
}

fn parse_instance_header(rest: &str) -> Result<(Name, Name), String> {
    fn parser(input: &str) -> IResult<&str, (Name, Name)> {
        let (input, name) = parse_ident(input)?;
        let (input, _) = multispace1(input)?;
        let (input, _) = tag("of")(input)?;
        let (input, _) = multispace1(input)?;
        let (input, schema) = parse_ident(input)?;
        let (input, _) = multispace0(input)?;
        let (input, _) = opt(pchar(':'))(input)?;
        let (input, _) = multispace0(input)?;
        Ok((input, (name.to_string(), schema.to_string())))
    }

    all_consuming(parser)(rest.trim())
        .map(|(_, v)| v)
        .map_err(|_| "instance header expects: `instance <Name> of <Schema>:`".to_string())
}

fn parse_subtype_decl(rest: &str) -> Result<SubtypeDeclV1, String> {
    fn parser(input: &str) -> IResult<&str, SubtypeDeclV1> {
        let (input, sub) = parse_ident(input)?;
        let (input, _) = multispace1(input)?;
        let (input, _) = alt((tag("<:"), tag("<")))(input)?;
        let (input, _) = multispace1(input)?;
        let (input, sup) = parse_ident(input)?;
        let (input, inclusion) =
            opt(tuple((multispace1, tag("as"), multispace1, parse_ident)))(input)?;
        let (input, _) = multispace0(input)?;
        Ok((
            input,
            SubtypeDeclV1 {
                sub: sub.to_string(),
                sup: sup.to_string(),
                inclusion: inclusion.map(|(_, _, _, incl)| incl.to_string()),
            },
        ))
    }

    all_consuming(parser)(rest.trim())
        .map(|(_, v)| v)
        .map_err(|_| {
            "subtype expects canonical `subtype <Sub> < <Sup>` (optionally `as Incl`)".to_string()
        })
}

fn collect_balanced_parens(
    lines: &[&str],
    start_index: usize,
    keyword: &str,
) -> Result<(String, usize), String> {
    let mut depth: i32 = 0;
    let mut combined = String::new();

    let mut i = start_index;
    while i < lines.len() {
        let line = strip_comment(lines[i]).trim();
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
    Ok((combined, i))
}

fn parse_relation_decl(line: &str) -> Result<RelationDeclV1, String> {
    let rest = line
        .trim()
        .strip_prefix("relation ")
        .ok_or_else(|| "relation declaration must start with `relation `".to_string())?;
    let open = rest
        .find('(')
        .ok_or_else(|| "relation declaration is missing `(`".to_string())?;
    let close = rest
        .rfind(')')
        .ok_or_else(|| "relation declaration is missing `)`".to_string())?;
    if close < open || !rest[close + 1..].trim().is_empty() {
        return Err(
            "relation expects exactly `relation Name(role: Type @kind, ...)`; relation-level axis shorthands are not canonical"
                .to_string(),
        );
    }
    let name = rest[..open].trim();
    parse_identifier_text(name, "relation name")?;
    let inner = rest[open + 1..close].trim();
    if inner.is_empty() {
        return Err("relation must declare at least one role".to_string());
    }
    let fields = split_top_level_commas_nested(inner)
        .into_iter()
        .map(parse_field_decl_v1)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RelationDeclV1 {
        name: name.to_string(),
        fields,
    })
}

fn parse_field_decl_v1(text: &str) -> Result<FieldDeclV1, String> {
    let Some((field, raw_type)) = text.split_once(':') else {
        return Err(format!(
            "relation role expects `name: Type @kind`, got `{text}`"
        ));
    };
    let field = field.trim();
    parse_identifier_text(field, "role name")?;
    let (type_text, kind) = split_role_kind_annotation(raw_type.trim())?;
    Ok(FieldDeclV1 {
        field: field.to_string(),
        ty: parse_type_expr_v1(type_text)?,
        kind,
    })
}

fn split_role_kind_annotation(text: &str) -> Result<(&str, RoleKindV1), String> {
    let annotations = [
        ("@context", RoleKindV1::Context),
        ("@world", RoleKindV1::World),
        ("@temporal", RoleKindV1::Temporal),
        ("@parameter", RoleKindV1::Parameter),
        ("@evidence", RoleKindV1::Evidence),
        ("@data", RoleKindV1::Data),
    ];
    for (suffix, kind) in annotations {
        if let Some(base) = text.strip_suffix(suffix) {
            let base = base.trim();
            if base.is_empty() {
                return Err(format!("role annotation `{suffix}` is missing a type"));
            }
            return Ok((base, kind));
        }
    }
    if text.contains('@') {
        return Err(format!("unknown or misplaced role annotation in `{text}`"));
    }
    Ok((text, RoleKindV1::Data))
}

pub fn parse_type_expr_v1(text: &str) -> Result<TypeExprV1, String> {
    let text = text.trim();
    if let Some(inner) = wrapped_call(text, "relation") {
        parse_identifier_text(inner, "relation-object type")?;
        return Ok(TypeExprV1::RelationObject {
            relation: inner.to_string(),
        });
    }
    if let Some(inner) = wrapped_call(text, "indexed") {
        let parts = split_top_level_semicolons(inner);
        if parts.len() != 2 {
            return Err("indexed type expects `indexed(Base; earlier_role|...)`".to_string());
        }
        let base = parse_type_expr_v1(parts[0])?;
        let over_roles = split_pipe_names(parts[1], "indexed role")?;
        if over_roles.is_empty() {
            return Err("indexed type must name at least one earlier role".to_string());
        }
        return Ok(TypeExprV1::Indexed {
            base: Box::new(base),
            over_roles,
        });
    }
    if let Some(inner) = wrapped_call(text, "refined") {
        let parts = split_top_level_semicolons(inner);
        if parts.len() < 2 {
            return Err("refined type expects `refined(Base; predicate; ...)`".to_string());
        }
        let base = parse_type_expr_v1(parts[0])?;
        let predicates = parts[1..]
            .iter()
            .map(|part| parse_refinement_predicate_v1(part))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(TypeExprV1::Refined {
            base: Box::new(base),
            predicates,
        });
    }
    parse_identifier_text(text, "object type")?;
    Ok(TypeExprV1::Object {
        name: text.to_string(),
    })
}

fn parse_refinement_predicate_v1(text: &str) -> Result<RefinementPredicateV1, String> {
    let text = text.trim();
    if let Some(inner) = wrapped_call(text, "eq") {
        parse_identifier_text(inner, "equality value")?;
        return Ok(RefinementPredicateV1::Equals {
            value: inner.to_string(),
        });
    }
    if let Some(inner) = wrapped_call(text, "in") {
        return Ok(RefinementPredicateV1::MemberOf {
            values: split_pipe_names(inner, "membership value")?,
        });
    }
    if let Some(inner) = wrapped_call(text, "enum") {
        return Ok(RefinementPredicateV1::Enum {
            values: split_pipe_names(inner, "enum value")?,
        });
    }
    if let Some(inner) = wrapped_call(text, "key") {
        return Ok(RefinementPredicateV1::Key {
            roles: split_pipe_names(inner, "key role")?,
        });
    }
    if let Some(inner) = wrapped_call(text, "cardinality") {
        let values = inner.split('|').map(str::trim).collect::<Vec<_>>();
        if values.len() != 2 {
            return Err("cardinality expects `cardinality(min|max)`".to_string());
        }
        let min = values[0]
            .parse::<u32>()
            .map_err(|_| "cardinality minimum must be a u32".to_string())?;
        let max = values[1]
            .parse::<u32>()
            .map_err(|_| "cardinality maximum must be a u32".to_string())?;
        if min > max {
            return Err("cardinality minimum exceeds maximum".to_string());
        }
        return Ok(RefinementPredicateV1::Cardinality { min, max });
    }
    if let Some(inner) = wrapped_call(text, "predicate") {
        let mut names = split_pipe_names(inner, "predicate name or argument")?;
        if names.is_empty() {
            return Err("predicate must name a supported predicate".to_string());
        }
        let name = names.remove(0);
        return Ok(RefinementPredicateV1::Predicate { name, args: names });
    }
    Err(format!("unsupported refinement predicate `{text}`"))
}

fn wrapped_call<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.strip_prefix(name)?
        .strip_prefix('(')?
        .strip_suffix(')')
        .map(str::trim)
}

fn split_pipe_names(text: &str, what: &str) -> Result<Vec<Name>, String> {
    let names = text
        .split('|')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| {
            parse_identifier_text(name, what)?;
            Ok(name.to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;
    if names.is_empty() {
        return Err(format!("{what} list must not be empty"));
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
                parts.push(text[start..index].trim());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim());
    parts
}

fn parse_identifier_text(text: &str, what: &str) -> Result<(), String> {
    all_consuming(parse_ident)(text)
        .map(|_| ())
        .map_err(|_| format!("{what} must be an identifier, got `{text}`"))
}

fn parse_value_atom_text(text: &str, what: &str) -> Result<(), String> {
    if text.is_empty()
        || text
            .chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, ',' | '(' | ')' | '{' | '}' | '=' | ':'))
    {
        return Err(format!("{what} must be a finite atom, got `{text}`"));
    }
    Ok(())
}

fn parse_generator_decl(line: &str) -> Result<GeneratorDeclV1, String> {
    let (kind, rest) = if let Some(rest) = line.strip_prefix("aspect ") {
        (GeneratorKindV1::Aspect, rest)
    } else if let Some(rest) = line.strip_prefix("function ") {
        (GeneratorKindV1::Function, rest)
    } else {
        return Err("generator must start with `aspect` or `function`".to_string());
    };
    let reversible = rest.trim_end().ends_with(" @reversible");
    let rest = rest
        .trim_end()
        .strip_suffix(" @reversible")
        .unwrap_or(rest)
        .trim();
    let Some((name, signature)) = rest.split_once(':') else {
        return Err("generator expects `name: Source -> Target`".to_string());
    };
    let name = name.trim();
    parse_identifier_text(name, "generator name")?;
    let Some((source, target)) = signature.split_once("->") else {
        return Err("generator expects `Source -> Target`".to_string());
    };
    let source = source.trim();
    let target = target.trim();
    parse_identifier_text(source, "generator source")?;
    parse_identifier_text(target, "generator target")?;
    Ok(GeneratorDeclV1 {
        name: name.to_string(),
        source: source.to_string(),
        target: target.to_string(),
        kind,
        reversible,
    })
}

fn parse_constraint(rest: &str) -> Result<ConstraintV1, String> {
    #[derive(Debug)]
    enum ClosureClauseV1 {
        On(CarrierFieldsV1),
        Param(Vec<Name>),
    }

    type SplitClosureClausesV1 = (String, Option<CarrierFieldsV1>, Option<Vec<Name>>);

    fn peel_closure_clause_suffix(rest: &str) -> Result<Option<(String, ClosureClauseV1)>, String> {
        let trimmed = rest.trim_end();
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
        let part = part.trim();
        if !part.starts_with('(') || !part.ends_with(')') {
            return Ok(None);
        }
        let inner = &part[1..part.len() - 1];
        let fields: Vec<&str> = inner
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        match kind {
            "on" => {
                if fields.len() != 2 {
                    return Err("carrier fields clause expects: `on (field0, field1)`".to_string());
                }
                Ok(Some((
                    base.trim().to_string(),
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
                Ok(Some((
                    base.trim().to_string(),
                    ClosureClauseV1::Param(fields.iter().map(|s| (*s).to_string()).collect()),
                )))
            }
            _ => Ok(None),
        }
    }

    fn split_closure_clauses(rest: &str) -> Result<SplitClosureClausesV1, String> {
        let mut base = rest.trim().to_string();
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
                    params = Some(p);
                }
            }
            base = b;
        }

        Ok((base.trim().to_string(), carriers, params))
    }

    let (rest, carriers, params) = split_closure_clauses(rest)?;
    let rest = rest.trim();
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
    if let Some(after) = rest.strip_prefix("functional ").map(str::trim) {
        // Our canonical surface prefers `functional Rel.field -> Rel.field`, but
        // some examples use a more declarative `functional Rel(...)` form.
        //
        // For now, treat any unrecognized `functional ...` as an unknown constraint
        // rather than failing parsing of the whole file.
        let parts: Vec<&str> = after.split("->").collect();
        if parts.len() == 2 {
            if let (Ok((rel1, field1)), Ok((rel2, field2))) = (
                split_rel_field(parts[0].trim()),
                split_rel_field(parts[1].trim()),
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
        return Ok(ConstraintV1::Unknown {
            text: rest.to_string(),
        });
    }

    if let Some(after) = rest.strip_prefix("at_most ").map(str::trim) {
        if carriers.is_some() {
            return Err("`on (...)` is not supported for at_most constraints".to_string());
        }
        let (max_str, rest2) = after
            .split_once(' ')
            .ok_or_else(|| "at_most expects: `at_most N Rel.field -> Rel.field`".to_string())?;
        let max: u32 = max_str
            .parse()
            .map_err(|_| "at_most expects a non-negative integer bound".to_string())?;
        let parts: Vec<&str> = rest2.split("->").collect();
        if parts.len() != 2 {
            return Err("at_most expects: `Rel.field -> Rel.field`".to_string());
        }
        if let (Ok((rel1, field1)), Ok((rel2, field2))) = (
            split_rel_field(parts[0].trim()),
            split_rel_field(parts[1].trim()),
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

    if let Some(after) = rest.strip_prefix("typing ").map(str::trim) {
        // Canonical surface syntax:
        //   `typing Rel: some_rule_name`
        if let Some((relation, rule)) = after.split_once(':') {
            let relation = relation.trim();
            let rule = rule.trim();
            if !relation.is_empty() && !rule.is_empty() {
                return Ok(ConstraintV1::Typing {
                    relation: relation.to_string(),
                    rule: rule.to_string(),
                });
            }
        }
        return Ok(ConstraintV1::Unknown {
            text: rest.to_string(),
        });
    }

    if let Some(after) = rest.strip_prefix("symmetric ").map(str::trim) {
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
            let relation = relation.trim();
            let guard = guard.trim();
            if !relation.is_empty() {
                if let Some((lhs, rhs)) = guard.split_once(" in ") {
                    let lhs = lhs.trim();
                    let rhs = rhs.trim();
                    // Support both:
                    // - `Rel.field in {...}` (canonical), and
                    // - `field in {...}` (shorthand; formatter will expand).
                    let (rel2, field) = if let Ok((rel2, field)) = split_rel_field(lhs) {
                        (rel2, field)
                    } else {
                        (relation.to_string(), lhs.to_string())
                    };
                    if rel2 == relation {
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
            return Ok(ConstraintV1::Unknown {
                text: rest.to_string(),
            });
        }

        // Unconditional symmetry.
        return Ok(ConstraintV1::Symmetric {
            relation: after.to_string(),
            carriers,
            params,
        });
    }

    if let Some(after) = rest.strip_prefix("transitive ").map(str::trim) {
        if after.is_empty() {
            return Err("transitive expects a relation name".to_string());
        }
        return Ok(ConstraintV1::Transitive {
            relation: after.to_string(),
            carriers,
            params,
        });
    }

    if let Some(after) = rest.strip_prefix("key ").map(str::trim) {
        if carriers.is_some() || params.is_some() {
            return Err(
                "`on (...)` / `param (...)` are only supported for symmetric/transitive constraints"
                    .to_string(),
            );
        }
        let Some(open) = after.find('(') else {
            return Ok(ConstraintV1::Unknown {
                text: rest.to_string(),
            });
        };
        let Some(close) = after.rfind(')') else {
            return Ok(ConstraintV1::Unknown {
                text: rest.to_string(),
            });
        };
        let relation = after[..open].trim();
        let fields_text = after[open + 1..close].trim();
        let fields: Vec<String> = fields_text
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if relation.is_empty() || fields.is_empty() {
            return Ok(ConstraintV1::Unknown {
                text: rest.to_string(),
            });
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
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
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
    let Some((rel, field)) = s.split_once('.') else {
        return Err(format!("expected `Rel.field`, got `{s}`"));
    };
    let rel = rel.trim();
    let field = field.trim();
    if rel.is_empty() || field.is_empty() {
        return Err(format!("expected `Rel.field`, got `{s}`"));
    }
    Ok((rel.to_string(), field.to_string()))
}

fn parse_name_set_literal(s: &str) -> Option<Vec<Name>> {
    let s = s.trim();
    let inner = s.strip_prefix('{')?.strip_suffix('}')?.trim();
    let values = inner
        .split(',')
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
        .collect::<Vec<_>>();
    if values.is_empty() {
        return None;
    }
    Some(values)
}

fn collect_indented_block(lines: &[&str], start_index: usize) -> (String, usize) {
    let mut out_lines = Vec::new();
    let mut i = start_index;
    while i < lines.len() {
        let raw = lines[i];
        let trimmed = strip_comment(raw).trim();
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
    (out_lines.join(" "), i)
}

fn collect_indented_block_lines(lines: &[&str], start_index: usize) -> (Vec<String>, usize) {
    let mut out_lines: Vec<String> = Vec::new();
    let mut i = start_index;
    while i < lines.len() {
        let raw = lines[i];
        let trimmed = strip_comment(raw).trim();
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

fn split_equation(equation_text: &str) -> Result<(String, String), String> {
    let Some((lhs, rhs)) = equation_text.split_once('=') else {
        return Err("equation body must contain `=`".to_string());
    };
    let lhs = lhs.trim();
    let rhs = rhs.trim();
    if lhs.is_empty() || rhs.is_empty() {
        return Err("equation must have non-empty lhs and rhs".to_string());
    }
    Ok((lhs.to_string(), rhs.to_string()))
}

fn split_assignment(line: &str) -> Option<(&str, &str)> {
    let (lhs, rhs) = line.split_once('=')?;
    let lhs = lhs.trim();
    let rhs = rhs.trim();
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

    for raw in lines {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("vars:") {
            current = Field::Vars;
            let rest = rest.trim();
            if !rest.is_empty() {
                vars_lines.push(rest.to_string());
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("lhs:") {
            current = Field::Lhs;
            let rest = rest.trim();
            if !rest.is_empty() {
                lhs_lines.push(rest.to_string());
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("rhs:") {
            current = Field::Rhs;
            let rest = rest.trim();
            if !rest.is_empty() {
                rhs_lines.push(rest.to_string());
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("orientation:") {
            current = Field::Orientation;
            let rest = rest.trim();
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

    let mut vars: Vec<RewriteVarDeclV1> = Vec::new();
    for line in vars_lines {
        vars.extend(parse_rewrite_var_decl_list_v1(&line)?);
    }

    let lhs_text = lhs_lines.join(" ");
    let rhs_text = rhs_lines.join(" ");
    if lhs_text.trim().is_empty() {
        return Err(format!("rewrite `{rule_name}`: missing `lhs:`"));
    }
    if rhs_text.trim().is_empty() {
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
    match s.trim() {
        "forward" => Ok(RewriteOrientationV1::Forward),
        "backward" => Ok(RewriteOrientationV1::Backward),
        "bidirectional" | "both" => Ok(RewriteOrientationV1::Bidirectional),
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
pub fn parse_rewrite_var_decl_list_v1(line: &str) -> Result<Vec<RewriteVarDeclV1>, String> {
    fn comma(input: &str) -> IResult<&str, ()> {
        let (input, _) = multispace0(input)?;
        let (input, _) = pchar(',')(input)?;
        let (input, _) = multispace0(input)?;
        Ok((input, ()))
    }

    fn var_type(input: &str) -> IResult<&str, RewriteVarTypeV1> {
        let (input, _) = multispace0(input)?;
        if let Ok((input2, _)) = tag::<&str, &str, nom::error::Error<&str>>("Path")(input) {
            let (input2, _) = multispace0(input2)?;
            let (input2, (from, to)) = alt((
                // Path(x,y)
                preceded(
                    pchar('('),
                    tuple((
                        preceded(multispace0, parse_ident),
                        preceded(tuple((multispace0, pchar(','), multispace0)), parse_ident),
                        preceded(multispace0, pchar(')')),
                    )),
                )
                .map(|(from, to, _)| (from, to)),
                // Path x y
                tuple((
                    preceded(multispace1, parse_ident),
                    preceded(multispace1, parse_ident),
                )),
            ))(input2)?;
            Ok((
                input2,
                RewriteVarTypeV1::Path {
                    from: from.to_string(),
                    to: to.to_string(),
                },
            ))
        } else {
            let (input, ty) = parse_ident(input)?;
            Ok((input, RewriteVarTypeV1::Object { ty: ty.to_string() }))
        }
    }

    fn var_decl(input: &str) -> IResult<&str, RewriteVarDeclV1> {
        let (input, name) = preceded(multispace0, parse_ident)(input)?;
        let (input, _) = preceded(multispace0, pchar(':'))(input)?;
        let (input, ty) = var_type(input)?;
        Ok((
            input,
            RewriteVarDeclV1 {
                name: name.to_string(),
                ty,
            },
        ))
    }

    fn parser(input: &str) -> IResult<&str, Vec<RewriteVarDeclV1>> {
        let (input, decls) = separated_list1(comma, var_decl)(input)?;
        let (input, _) = multispace0(input)?;
        Ok((input, decls))
    }

    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    all_consuming(parser)(trimmed).map(|(_, v)| v).map_err(|_| {
        format!("invalid rewrite vars line: `{trimmed}` (expected `x: Ty, p: Path(x,y)` etc)")
    })
}

pub fn parse_path_expr_v3(text: &str) -> Result<PathExprV3, String> {
    fn comma(input: &str) -> IResult<&str, ()> {
        let (input, _) = multispace0(input)?;
        let (input, _) = pchar(',')(input)?;
        let (input, _) = multispace0(input)?;
        Ok((input, ()))
    }

    fn parens<'a, O>(
        mut inner: impl FnMut(&'a str) -> IResult<&'a str, O>,
    ) -> impl FnMut(&'a str) -> IResult<&'a str, O> {
        move |input: &'a str| {
            let (input, _) = multispace0(input)?;
            let (input, _) = pchar('(')(input)?;
            let (input, out) = inner(input)?;
            let (input, _) = multispace0(input)?;
            let (input, _) = pchar(')')(input)?;
            Ok((input, out))
        }
    }

    fn expr(input: &str) -> IResult<&str, PathExprV3> {
        preceded(
            multispace0,
            alt((refl_expr, step_expr, trans_expr, inv_expr, var_expr)),
        )(input)
    }

    fn var_expr(input: &str) -> IResult<&str, PathExprV3> {
        let (input, name) = parse_ident(input)?;
        Ok((
            input,
            PathExprV3::Var {
                name: name.to_string(),
            },
        ))
    }

    fn refl_expr(input: &str) -> IResult<&str, PathExprV3> {
        let (input, _) = alt((tag("refl"), tag("id")))(input)?;
        let (input, entity) = parens(preceded(multispace0, parse_ident))(input)?;
        Ok((
            input,
            PathExprV3::Reflexive {
                entity: entity.to_string(),
            },
        ))
    }

    fn step_expr(input: &str) -> IResult<&str, PathExprV3> {
        let (input, _) = tag("step")(input)?;
        let (input, (from, rel, to)) = parens(tuple((
            preceded(multispace0, parse_ident),
            preceded(comma, parse_ident),
            preceded(comma, parse_ident),
        )))(input)?;
        Ok((
            input,
            PathExprV3::Step {
                from: from.to_string(),
                rel: rel.to_string(),
                to: to.to_string(),
            },
        ))
    }

    fn trans_expr(input: &str) -> IResult<&str, PathExprV3> {
        let (input, _) = tag("trans")(input)?;
        let (input, (left, right)) = parens(tuple((expr, preceded(comma, expr))))(input)?;
        Ok((
            input,
            PathExprV3::Trans {
                left: Box::new(left),
                right: Box::new(right),
            },
        ))
    }

    fn inv_expr(input: &str) -> IResult<&str, PathExprV3> {
        let (input, _) = tag("inv")(input)?;
        let (input, path) = parens(expr)(input)?;
        Ok((
            input,
            PathExprV3::Inv {
                path: Box::new(path),
            },
        ))
    }

    all_consuming(expr)(text.trim())
        .map(|(_, v)| v)
        .map_err(|_| format!("invalid path expression: `{}`", text.trim()))
}

fn collect_balanced_braces(
    lines: &[&str],
    start_index: usize,
    first_rhs: &str,
) -> Result<(String, usize), String> {
    let mut depth: i32 = 0;
    let mut combined = String::new();

    // Start with the RHS on the current line (after `=`).
    {
        let rhs = strip_comment(first_rhs).trim();
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
        let line = strip_comment(lines[i]).trim();
        if !line.is_empty() {
            combined.push(' ');
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
    Ok((combined, i))
}

fn parse_set_literal(text: &str) -> Result<SetLiteralV1, String> {
    let text = text.trim();
    if !text.starts_with('{') || !text.ends_with('}') {
        return Err("expected set literal `{ ... }`".to_string());
    }
    let inner = text[1..text.len() - 1].trim();
    let items = split_top_level_commas(inner)
        .into_iter()
        .filter_map(|s| {
            let t = s.trim();
            (!t.is_empty()).then_some(t.to_string())
        })
        .map(parse_set_item)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SetLiteralV1 { items })
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

fn parse_set_item(item: String) -> Result<SetItemV1, String> {
    let trimmed = item.trim();
    let (label, tuple_text) = if trimmed.starts_with('(') {
        (None, trimmed)
    } else if let Some((label, tuple)) = trimmed.split_once(':') {
        let label = label.trim();
        parse_identifier_text(label, "fact label")?;
        (Some(label.to_string()), tuple.trim())
    } else {
        (None, trimmed)
    };
    if tuple_text.starts_with('(') && tuple_text.ends_with(')') {
        let inner = tuple_text[1..tuple_text.len() - 1].trim();
        let mut fields = Vec::new();
        for part in split_top_level_commas(inner) {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let Some((k, v)) = part.split_once('=') else {
                return Err(format!("tuple field missing `=`: `{part}`"));
            };
            let key = k.trim();
            let value = v.trim();
            parse_identifier_text(key, "tuple role")?;
            parse_value_atom_text(value, "tuple value")?;
            fields.push((key.to_string(), value.to_string()));
        }
        return Ok(SetItemV1::Tuple { label, fields });
    }
    if label.is_some() {
        return Err("fact label must prefix a relation tuple".to_string());
    }
    parse_value_atom_text(trimmed, "set element")?;
    Ok(SetItemV1::Ident {
        name: trimmed.to_string(),
    })
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
    fn rejects_missing_or_multiple_module_headers() {
        let missing = r#"
schema S:
  object A
"#;
        let err = parse_schema_v1(missing).expect_err("missing module header must reject");
        assert!(err.to_string().contains("exactly one explicit"));

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
    fn subtype_alias_formats_back_to_canonical_surface() {
        let subtype = parse_subtype_decl_v1("Child <: Parent").expect("parse subtype declaration");
        assert_eq!(format_subtype_decl_v1(&subtype), "subtype Child < Parent");
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
