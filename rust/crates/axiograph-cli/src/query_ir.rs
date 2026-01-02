//! Typed query IR (JSON) for tooling/LLMs.
//!
//! Motivation:
//! - LLMs are good at producing *structured* JSON, but often produce invalid
//!   AxQL text (small syntax errors, wrong sugar forms, etc).
//! - A typed JSON IR lets us validate and compile into the same AxQL core,
//!   avoiding brittle parsing and enabling better error messages.
//!
//! Non-goals (v1):
//! - This is not a stable public API yet; it is a pragmatic bridge for REPL/LLM
//!   integration.
//! - We keep the IR minimal and compile into the existing AxQL core.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

use crate::axql::{parse_axql_path_expr, AxqlAtom, AxqlContextSpec, AxqlQuery, AxqlTerm};

pub const QUERY_IR_V1_VERSION: u32 = 1;

/// JSON schema for `QueryIrV1` (for tooling/LLMs).
///
/// This is intentionally hand-written and conservative:
/// - it documents the IR shape in a machine-readable way,
/// - it is used by the LLM tool-loop to strongly bias models toward producing
///   `query_ir_v1` rather than brittle AxQL text,
/// - it is **not** a compatibility promise yet (v1 is an internal bridge).
pub fn query_ir_v1_json_schema() -> serde_json::Value {
    // Notes on schema design:
    //
    // - We include both the canonical field names (`select_vars`, `where_atoms`) and the
    //   user-friendly aliases (`select`, `where`) because serde accepts both.
    // - For terms/contexts we allow the compact string/integer forms, but models should
    //   prefer the explicit object forms to avoid ambiguity.
    //
    // The schema is embedded inside the LLM tool definitions; it is not used for
    // untrusted runtime validation (we still parse with serde + do semantic checks).
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "version": { "type": "integer", "const": QUERY_IR_V1_VERSION },
            "select": { "type": "array", "items": { "type": "string" } },
            "select_vars": { "type": "array", "items": { "type": "string" } },
            "where": { "type": "array", "items": { "$ref": "#/$defs/query_atom" } },
            "where_atoms": { "type": "array", "items": { "$ref": "#/$defs/query_atom" } },
            "disjuncts": {
                "type": "array",
                "items": { "type": "array", "items": { "$ref": "#/$defs/query_atom" } }
            },
            "limit": { "type": "integer", "minimum": 1, "maximum": 200 },
            "max_hops": { "type": "integer", "minimum": 0, "maximum": 1000 },
            "min_confidence": { "type": "number", "minimum": 0, "maximum": 1 },
            "contexts": { "type": "array", "items": { "$ref": "#/$defs/query_context" } }
        },
        "required": ["version"],
        "oneOf": [
            { "required": ["where"] },
            { "required": ["where_atoms"] },
            { "required": ["disjuncts"] }
        ],
        "$defs": {
            "query_term": {
                "description": "A query term. Preferred object forms: {kind:var|name|entity|wildcard}. Compact forms: string (\"?x\" or \"Alice\" or \"_\") or integer id.",
                "oneOf": [
                    { "type": "string" },
                    { "type": "integer", "minimum": 0 },
                    {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["kind"],
                        "oneOf": [
                            {
                                "properties": {
                                    "kind": { "const": "var" },
                                    "name": { "type": "string" }
                                },
                                "required": ["kind", "name"]
                            },
                            {
                                "properties": {
                                    "kind": { "const": "name" },
                                    "value": { "type": "string" }
                                },
                                "required": ["kind", "value"]
                            },
                            {
                                "properties": {
                                    "kind": { "const": "entity" },
                                    "key": { "type": "string" },
                                    "value": { "type": "string" }
                                },
                                "required": ["kind", "key", "value"]
                            },
                            {
                                "properties": { "kind": { "const": "wildcard" } },
                                "required": ["kind"]
                            }
                        ]
                    }
                ]
            },
            "query_context": {
                "description": "Context/world selector for scoping fact-node matches.",
                "oneOf": [
                    { "type": "string" },
                    { "type": "integer", "minimum": 0 },
                    {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["kind"],
                        "oneOf": [
                            {
                                "properties": {
                                    "kind": { "const": "name" },
                                    "name": { "type": "string" }
                                },
                                "required": ["kind", "name"]
                            },
                            {
                                "properties": {
                                    "kind": { "const": "entity_id" },
                                    "id": { "type": "integer", "minimum": 0 }
                                },
                                "required": ["kind", "id"]
                            }
                        ]
                    }
                ]
            },
            "query_atom": {
                "type": "object",
                "required": ["kind"],
                "oneOf": [
                    {
                        "additionalProperties": false,
                        "properties": {
                            "kind": { "const": "type" },
                            "term": { "$ref": "#/$defs/query_term" },
                            "type": { "type": "string" },
                            "ty": { "type": "string" },
                            "type_name": { "type": "string" }
                        },
                        "required": ["kind", "term"],
                        "anyOf": [
                            { "required": ["type"] },
                            { "required": ["ty"] },
                            { "required": ["type_name"] }
                        ]
                    },
                    {
                        "additionalProperties": false,
                        "properties": {
                            "kind": { "const": "edge" },
                            "left": { "$ref": "#/$defs/query_term" },
                            "path": { "type": "string" },
                            "right": { "$ref": "#/$defs/query_term" }
                        },
                        "required": ["kind", "left", "path", "right"]
                    },
                    {
                        "additionalProperties": false,
                        "properties": {
                            "kind": { "const": "attr_eq" },
                            "term": { "$ref": "#/$defs/query_term" },
                            "key": { "type": "string" },
                            "value": { "type": "string" }
                        },
                        "required": ["kind", "term", "key", "value"]
                    },
                    {
                        "additionalProperties": false,
                        "properties": {
                            "kind": { "const": "attr_contains" },
                            "term": { "$ref": "#/$defs/query_term" },
                            "key": { "type": "string" },
                            "needle": { "type": "string" }
                        },
                        "required": ["kind", "term", "key", "needle"]
                    },
                    {
                        "additionalProperties": false,
                        "properties": {
                            "kind": { "const": "attr_fts" },
                            "term": { "$ref": "#/$defs/query_term" },
                            "key": { "type": "string" },
                            "query": { "type": "string" }
                        },
                        "required": ["kind", "term", "key", "query"]
                    },
                    {
                        "additionalProperties": false,
                        "properties": {
                            "kind": { "const": "attr_fuzzy" },
                            "term": { "$ref": "#/$defs/query_term" },
                            "key": { "type": "string" },
                            "needle": { "type": "string" },
                            "max_dist": { "type": "integer", "minimum": 0, "maximum": 16 }
                        },
                        "required": ["kind", "term", "key", "needle", "max_dist"]
                    },
                    {
                        "additionalProperties": false,
                        "properties": {
                            "kind": { "const": "fact" },
                            "fact": { "$ref": "#/$defs/query_term" },
                            "relation": { "type": "string" },
                            "fields": {
                                "type": "object",
                                "additionalProperties": { "$ref": "#/$defs/query_term" }
                            }
                        },
                        "required": ["kind", "relation", "fields"]
                    },
                    {
                        "additionalProperties": false,
                        "properties": {
                            "kind": { "const": "has_out" },
                            "term": { "$ref": "#/$defs/query_term" },
                            "rels": { "type": "array", "items": { "type": "string" } }
                        },
                        "required": ["kind", "term", "rels"]
                    },
                    {
                        "additionalProperties": false,
                        "properties": {
                            "kind": { "const": "attrs" },
                            "term": { "$ref": "#/$defs/query_term" },
                            "pairs": { "type": "object", "additionalProperties": { "type": "string" } }
                        },
                        "required": ["kind", "term", "pairs"]
                    },
                    {
                        "additionalProperties": false,
                        "properties": {
                            "kind": { "const": "shape" },
                            "term": { "$ref": "#/$defs/query_term" },
                            "type_name": { "type": "string" },
                            "rels": { "type": "array", "items": { "type": "string" } },
                            "attrs": { "type": "object", "additionalProperties": { "type": "string" } }
                        },
                        "required": ["kind", "term"]
                    }
                ]
            }
        }
    })
}

/// A JSON query IR that compiles into AxQL.
///
/// This IR is designed to be easy for tools/LLMs:
/// - most terms can be written as simple strings (e.g. `"?x"`, `"Alice"`, `"_"`
///   where bare names mean `name("...")`)
/// - paths are written as AxQL path expressions (e.g. `"rel_0/rel_1"`, `"(a|b)*"`)
/// - disjunction is explicit via `disjuncts`, but a single `where` clause is also accepted
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryIrV1 {
    #[serde(default = "default_query_ir_v1_version")]
    pub version: u32,

    /// Optional explicit select list. Empty means “implicit select”.
    #[serde(default, alias = "select")]
    pub select_vars: Vec<String>,

    /// Convenience: a single conjunctive `where` clause.
    ///
    /// If present, this is compiled into `disjuncts = [where]` unless `disjuncts`
    /// is also present.
    #[serde(default, alias = "where")]
    pub where_atoms: Option<Vec<QueryAtomIrV1>>,

    /// Top-level disjunction (UCQ): OR of conjunctive branches.
    #[serde(default)]
    pub disjuncts: Option<Vec<Vec<QueryAtomIrV1>>>,

    #[serde(default)]
    pub limit: Option<usize>,

    #[serde(default)]
    pub max_hops: Option<u32>,

    /// Minimum per-edge confidence threshold (0..=1).
    #[serde(default)]
    pub min_confidence: Option<f32>,

    /// Optional context/world scoping for fact nodes.
    #[serde(default)]
    pub contexts: Vec<QueryContextIrV1>,
}

fn default_query_ir_v1_version() -> u32 {
    QUERY_IR_V1_VERSION
}

impl QueryIrV1 {
    /// Convert an AxQL query into the typed JSON IR.
    ///
    /// This is primarily used to keep the LLM/tooling pipeline “typed” even if
    /// a backend returns (or a user supplies) AxQL text.
    ///
    /// Notes:
    /// - The conversion is best-effort but should preserve semantics for the
    ///   AxQL core atoms supported by `QueryIrV1`.
    /// - We use the compact IR forms where possible (e.g. bare `"Alice"` for
    ///   `name("Alice")`), but retain explicit `{"kind":"entity",...}` for
    ///   non-name lookups.
    pub fn from_axql_query(query: &AxqlQuery) -> Self {
        fn term_ir(term: &AxqlTerm) -> QueryTermIrV1 {
            match term {
                AxqlTerm::Var(v) => QueryTermIrV1::Simple(v.clone()),
                AxqlTerm::Const(id) => QueryTermIrV1::Id(*id),
                AxqlTerm::Wildcard => QueryTermIrV1::Simple("_".to_string()),
                AxqlTerm::Lookup { key, value } => {
                    if key == "name" {
                        QueryTermIrV1::Simple(value.clone())
                    } else {
                        QueryTermIrV1::Obj(QueryTermObjIrV1::Entity {
                            key: key.clone(),
                            value: value.clone(),
                        })
                    }
                }
            }
        }

        fn atom_ir(atom: &AxqlAtom) -> QueryAtomIrV1 {
            match atom {
                AxqlAtom::Type { term, type_name } => QueryAtomIrV1::Type {
                    term: term_ir(term),
                    type_name: type_name.clone(),
                },
                AxqlAtom::Edge { left, path, right } => QueryAtomIrV1::Edge {
                    left: term_ir(left),
                    path: render_path_expr(path),
                    right: term_ir(right),
                },
                AxqlAtom::AttrEq { term, key, value } => QueryAtomIrV1::AttrEq {
                    term: term_ir(term),
                    key: key.clone(),
                    value: value.clone(),
                },
                AxqlAtom::AttrContains { term, key, needle } => QueryAtomIrV1::AttrContains {
                    term: term_ir(term),
                    key: key.clone(),
                    needle: needle.clone(),
                },
                AxqlAtom::AttrFts { term, key, query } => QueryAtomIrV1::AttrFts {
                    term: term_ir(term),
                    key: key.clone(),
                    query: query.clone(),
                },
                AxqlAtom::AttrFuzzy {
                    term,
                    key,
                    needle,
                    max_dist,
                } => QueryAtomIrV1::AttrFuzzy {
                    term: term_ir(term),
                    key: key.clone(),
                    needle: needle.clone(),
                    max_dist: *max_dist,
                },
                AxqlAtom::Fact {
                    fact,
                    relation,
                    fields,
                } => {
                    let fact = fact.as_ref().map(term_ir);

                    let mut out_fields: BTreeMap<String, QueryTermIrV1> = BTreeMap::new();
                    for (k, v) in fields {
                        out_fields.insert(k.clone(), term_ir(v));
                    }

                    QueryAtomIrV1::Fact {
                        fact,
                        relation: relation.clone(),
                        fields: out_fields,
                    }
                }
                AxqlAtom::HasOut { term, rels } => QueryAtomIrV1::HasOut {
                    term: term_ir(term),
                    rels: rels.clone(),
                },
                AxqlAtom::Attrs { term, pairs } => {
                    let mut out_pairs: BTreeMap<String, String> = BTreeMap::new();
                    for (k, v) in pairs {
                        out_pairs.insert(k.clone(), v.clone());
                    }
                    QueryAtomIrV1::Attrs {
                        term: term_ir(term),
                        pairs: out_pairs,
                    }
                }
                AxqlAtom::Shape {
                    term,
                    type_name,
                    rels,
                    attrs,
                } => {
                    let mut out_attrs: BTreeMap<String, String> = BTreeMap::new();
                    for (k, v) in attrs {
                        out_attrs.insert(k.clone(), v.clone());
                    }
                    QueryAtomIrV1::Shape {
                        term: term_ir(term),
                        type_name: type_name.clone(),
                        rels: rels.clone(),
                        attrs: out_attrs,
                    }
                }
            }
        }

        fn ctx_ir(ctx: &AxqlContextSpec) -> QueryContextIrV1 {
            match ctx {
                AxqlContextSpec::EntityId(id) => QueryContextIrV1::EntityId(*id),
                AxqlContextSpec::Name(name) => QueryContextIrV1::Name(name.clone()),
            }
        }

        let mut disjuncts_ir: Vec<Vec<QueryAtomIrV1>> = Vec::new();
        for d in &query.disjuncts {
            disjuncts_ir.push(d.iter().map(atom_ir).collect());
        }

        let contexts = query.contexts.iter().map(ctx_ir).collect::<Vec<_>>();

        let (where_atoms, disjuncts) = if disjuncts_ir.len() <= 1 {
            (Some(disjuncts_ir.into_iter().next().unwrap_or_default()), None)
        } else {
            (None, Some(disjuncts_ir))
        };

        QueryIrV1 {
            version: QUERY_IR_V1_VERSION,
            select_vars: query.select_vars.clone(),
            where_atoms,
            disjuncts,
            limit: Some(query.limit),
            max_hops: query.max_hops,
            min_confidence: query.min_confidence,
            contexts,
        }
    }

    pub fn to_axql_query(&self) -> Result<AxqlQuery> {
        if self.version != QUERY_IR_V1_VERSION {
            return Err(anyhow!(
                "unsupported query_ir_v1 version {} (expected {QUERY_IR_V1_VERSION})",
                self.version
            ));
        }

        let disjuncts = match (&self.where_atoms, &self.disjuncts) {
            (Some(_), Some(_)) => {
                return Err(anyhow!(
                    "query_ir_v1: cannot set both `where` and `disjuncts`"
                ))
            }
            (Some(w), None) => vec![w.clone()],
            (None, Some(d)) => d.clone(),
            (None, None) => {
                return Err(anyhow!(
                    "query_ir_v1: missing query body (provide `where` or `disjuncts`)"
                ))
            }
        };

        let mut compiled_disjuncts: Vec<Vec<AxqlAtom>> = Vec::with_capacity(disjuncts.len());
        for d in disjuncts {
            let mut atoms: Vec<AxqlAtom> = Vec::with_capacity(d.len());
            for a in d {
                atoms.push(a.to_axql_atom()?);
            }
            compiled_disjuncts.push(atoms);
        }

        let mut select_vars: Vec<String> = Vec::new();
        for v in &self.select_vars {
            let v = v.trim();
            if v.is_empty() || v == "*" {
                continue;
            }
            select_vars.push(normalize_var_name(v));
        }

        let mut contexts: Vec<AxqlContextSpec> = Vec::new();
        for c in &self.contexts {
            contexts.push(c.to_context_spec()?);
        }

        let limit = self.limit.unwrap_or(20);

        let min_confidence = self.min_confidence.map(|c| {
            if !c.is_finite() {
                return 0.0;
            }
            c.clamp(0.0, 1.0)
        });

        Ok(AxqlQuery {
            select_vars,
            disjuncts: compiled_disjuncts,
            limit,
            contexts,
            max_hops: self.max_hops,
            min_confidence,
        })
    }

    /// Render the IR as an AxQL query string (best-effort, for debugging).
    pub fn to_axql_text(&self) -> Result<String> {
        let q = self.to_axql_query()?;
        Ok(render_axql_query(&q))
    }
}

fn normalize_var_name(v: &str) -> String {
    if v.starts_with('?') {
        v.to_string()
    } else {
        format!("?{v}")
    }
}

fn axql_string_lit(s: &str) -> String {
    let mut out = String::new();
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn render_term(t: &AxqlTerm) -> String {
    match t {
        AxqlTerm::Var(v) => v.clone(),
        AxqlTerm::Const(id) => id.to_string(),
        AxqlTerm::Wildcard => "_".to_string(),
        AxqlTerm::Lookup { key, value } => {
            if key == "name" {
                format!("name({})", axql_string_lit(value))
            } else {
                format!(
                    "entity({}, {})",
                    axql_string_lit(key),
                    axql_string_lit(value)
                )
            }
        }
    }
}

fn render_atom(a: &AxqlAtom) -> String {
    match a {
        AxqlAtom::Type { term, type_name } => {
            format!("{} : {}", render_term(term), type_name)
        }
        AxqlAtom::Edge { left, path, right } => {
            // Keep the path in the compact "unbracketed" form.
            format!(
                "{} -{}-> {}",
                render_term(left),
                render_path_expr(path),
                render_term(right)
            )
        }
        AxqlAtom::AttrEq { term, key, value } => format!(
            "attr({}, {}, {})",
            render_term(term),
            axql_string_lit(key),
            axql_string_lit(value)
        ),
        AxqlAtom::AttrContains { term, key, needle } => format!(
            "contains({}, {}, {})",
            render_term(term),
            axql_string_lit(key),
            axql_string_lit(needle)
        ),
        AxqlAtom::AttrFts { term, key, query } => format!(
            "fts({}, {}, {})",
            render_term(term),
            axql_string_lit(key),
            axql_string_lit(query)
        ),
        AxqlAtom::AttrFuzzy {
            term,
            key,
            needle,
            max_dist,
        } => format!(
            "fuzzy({}, {}, {}, {max_dist})",
            render_term(term),
            axql_string_lit(key),
            axql_string_lit(needle)
        ),
        AxqlAtom::Fact {
            fact,
            relation,
            fields,
        } => {
            let mut s = String::new();
            if let Some(fact) = fact {
                s.push_str(&format!("{} = ", render_term(fact)));
            }
            s.push_str(relation);
            s.push('(');
            let mut parts: Vec<String> = Vec::new();
            for (k, v) in fields {
                parts.push(format!("{k}={}", render_term(v)));
            }
            s.push_str(&parts.join(", "));
            s.push(')');
            s
        }
        AxqlAtom::HasOut { term, rels } => {
            format!("has({}, {})", render_term(term), rels.join(", "))
        }
        AxqlAtom::Attrs { term, pairs } => {
            let mut parts: Vec<String> = Vec::new();
            for (k, v) in pairs {
                parts.push(format!("{k}={}", axql_string_lit(v)));
            }
            format!("attrs({}, {})", render_term(term), parts.join(", "))
        }
        AxqlAtom::Shape {
            term,
            type_name,
            rels,
            attrs,
        } => {
            let mut parts: Vec<String> = Vec::new();
            if let Some(t) = type_name {
                parts.push(format!("is {t}"));
            }
            for r in rels {
                parts.push(r.clone());
            }
            for (k, v) in attrs {
                parts.push(format!("{k}={}", axql_string_lit(v)));
            }
            format!("{} {{ {} }}", render_term(term), parts.join(", "))
        }
    }
}

fn render_path_expr(p: &crate::axql::AxqlPathExpr) -> String {
    fn render_re(re: &crate::axql::AxqlRegex) -> String {
        use crate::axql::AxqlRegex;
        match re {
            AxqlRegex::Epsilon => "ε".to_string(),
            AxqlRegex::Rel(r) => r.clone(),
            AxqlRegex::Seq(parts) => parts.iter().map(render_re).collect::<Vec<_>>().join("/"),
            AxqlRegex::Alt(parts) => {
                format!("({})", parts.iter().map(render_re).collect::<Vec<_>>().join("|"))
            }
            AxqlRegex::Star(inner) => format!("{}*", render_re(inner)),
            AxqlRegex::Plus(inner) => format!("{}+", render_re(inner)),
            AxqlRegex::Opt(inner) => format!("{}?", render_re(inner)),
        }
    }
    render_re(&p.regex)
}

fn render_axql_query(q: &AxqlQuery) -> String {
    let mut out = String::new();
    if !q.select_vars.is_empty() {
        out.push_str("select ");
        out.push_str(&q.select_vars.join(" "));
        out.push(' ');
    }

    out.push_str("where ");
    let mut disjunct_texts: Vec<String> = Vec::new();
    for d in &q.disjuncts {
        let atoms = d.iter().map(render_atom).collect::<Vec<_>>().join(", ");
        disjunct_texts.push(atoms);
    }
    out.push_str(&disjunct_texts.join(" or "));

    if !q.contexts.is_empty() {
        let render_ctx = |c: &AxqlContextSpec| -> String {
            match c {
                AxqlContextSpec::EntityId(id) => id.to_string(),
                AxqlContextSpec::Name(name) => name.clone(),
            }
        };
        out.push_str(" in ");
        if q.contexts.len() == 1 {
            out.push_str(&render_ctx(&q.contexts[0]));
        } else {
            out.push('{');
            out.push_str(
                &q.contexts
                    .iter()
                    .map(|c| render_ctx(c))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            out.push('}');
        }
    }

    if let Some(max_hops) = q.max_hops {
        out.push_str(&format!(" max_hops {max_hops}"));
    }
    if let Some(min_conf) = q.min_confidence {
        out.push_str(&format!(" min_confidence {min_conf}"));
    }

    out.push_str(&format!(" limit {}", q.limit));
    out
}

/// A term in the typed query IR.
///
/// For convenience, tools may use:
/// - strings: `"?x"`, `"Alice"`, `"_"` (wildcard)
/// - numbers: `123` (entity id)
/// - objects: `{"kind": "name", "value": "Alice"}`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QueryTermIrV1 {
    /// Convenience form; compiled as:
    /// - `"?x"` → variable
    /// - `"_"` → wildcard
    /// - `"Alice"` → name("Alice")
    Simple(String),
    /// Convenience form: numeric entity id.
    Id(u32),
    /// Explicit term object.
    Obj(QueryTermObjIrV1),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QueryTermObjIrV1 {
    Var { name: String },
    Name { value: String },
    Entity { key: String, value: String },
    Wildcard {},
}

impl QueryTermIrV1 {
    fn to_axql_term(&self) -> Result<AxqlTerm> {
        Ok(match self {
            QueryTermIrV1::Simple(s) => {
                let s = s.trim();
                if s == "_" {
                    AxqlTerm::Wildcard
                } else if s.starts_with('?') {
                    AxqlTerm::Var(s.to_string())
                } else {
                    AxqlTerm::Lookup {
                        key: "name".to_string(),
                        value: s.to_string(),
                    }
                }
            }
            QueryTermIrV1::Id(id) => AxqlTerm::Const(*id),
            QueryTermIrV1::Obj(obj) => match obj {
                QueryTermObjIrV1::Var { name } => AxqlTerm::Var(normalize_var_name(name)),
                QueryTermObjIrV1::Name { value } => AxqlTerm::Lookup {
                    key: "name".to_string(),
                    value: value.clone(),
                },
                QueryTermObjIrV1::Entity { key, value } => AxqlTerm::Lookup {
                    key: key.clone(),
                    value: value.clone(),
                },
                QueryTermObjIrV1::Wildcard {} => AxqlTerm::Wildcard,
            },
        })
    }
}

/// Context/world selector for scoping fact-node matches.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QueryContextIrV1 {
    Name(String),
    EntityId(u32),
    Obj(QueryContextObjIrV1),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QueryContextObjIrV1 {
    Name { name: String },
    EntityId { id: u32 },
}

impl QueryContextIrV1 {
    fn to_context_spec(&self) -> Result<AxqlContextSpec> {
        Ok(match self {
            QueryContextIrV1::Name(name) => AxqlContextSpec::Name(name.clone()),
            QueryContextIrV1::EntityId(id) => AxqlContextSpec::EntityId(*id),
            QueryContextIrV1::Obj(obj) => match obj {
                QueryContextObjIrV1::Name { name } => AxqlContextSpec::Name(name.clone()),
                QueryContextObjIrV1::EntityId { id } => AxqlContextSpec::EntityId(*id),
            },
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QueryAtomIrV1 {
    Type {
        term: QueryTermIrV1,
        #[serde(alias = "type", alias = "ty")]
        type_name: String,
    },
    Edge {
        left: QueryTermIrV1,
        /// AxQL path expression (e.g. `rel_0/rel_1`, `(a|b)*`).
        path: String,
        right: QueryTermIrV1,
    },
    AttrEq {
        term: QueryTermIrV1,
        key: String,
        value: String,
    },
    AttrContains {
        term: QueryTermIrV1,
        key: String,
        needle: String,
    },
    AttrFts {
        term: QueryTermIrV1,
        key: String,
        query: String,
    },
    AttrFuzzy {
        term: QueryTermIrV1,
        key: String,
        needle: String,
        max_dist: usize,
    },
    Fact {
        /// Optional explicit fact node binder (must be a variable or `_`).
        #[serde(default)]
        fact: Option<QueryTermIrV1>,
        relation: String,
        /// Map field name → term (order is irrelevant).
        fields: BTreeMap<String, QueryTermIrV1>,
    },
    HasOut {
        term: QueryTermIrV1,
        rels: Vec<String>,
    },
    Attrs {
        term: QueryTermIrV1,
        pairs: BTreeMap<String, String>,
    },
    Shape {
        term: QueryTermIrV1,
        #[serde(default)]
        type_name: Option<String>,
        #[serde(default)]
        rels: Vec<String>,
        #[serde(default)]
        attrs: BTreeMap<String, String>,
    },
}

impl QueryAtomIrV1 {
    fn to_axql_atom(&self) -> Result<AxqlAtom> {
        Ok(match self {
            QueryAtomIrV1::Type { term, type_name } => AxqlAtom::Type {
                term: term.to_axql_term()?,
                type_name: type_name.clone(),
            },
            QueryAtomIrV1::Edge { left, path, right } => AxqlAtom::Edge {
                left: left.to_axql_term()?,
                path: parse_axql_path_expr(path)?,
                right: right.to_axql_term()?,
            },
            QueryAtomIrV1::AttrEq { term, key, value } => AxqlAtom::AttrEq {
                term: term.to_axql_term()?,
                key: key.clone(),
                value: value.clone(),
            },
            QueryAtomIrV1::AttrContains { term, key, needle } => AxqlAtom::AttrContains {
                term: term.to_axql_term()?,
                key: key.clone(),
                needle: needle.clone(),
            },
            QueryAtomIrV1::AttrFts { term, key, query } => AxqlAtom::AttrFts {
                term: term.to_axql_term()?,
                key: key.clone(),
                query: query.clone(),
            },
            QueryAtomIrV1::AttrFuzzy {
                term,
                key,
                needle,
                max_dist,
            } => AxqlAtom::AttrFuzzy {
                term: term.to_axql_term()?,
                key: key.clone(),
                needle: needle.clone(),
                max_dist: *max_dist,
            },
            QueryAtomIrV1::Fact {
                fact,
                relation,
                fields,
            } => {
                let fact = match fact {
                    None => None,
                    Some(t) => match t.to_axql_term()? {
                        AxqlTerm::Wildcard => None,
                        other => Some(other),
                    },
                };
                let mut out_fields: Vec<(String, AxqlTerm)> = Vec::new();
                for (k, v) in fields {
                    out_fields.push((k.clone(), v.to_axql_term()?));
                }
                out_fields.sort_by(|a, b| a.0.cmp(&b.0));
                AxqlAtom::Fact {
                    fact,
                    relation: relation.clone(),
                    fields: out_fields,
                }
            }
            QueryAtomIrV1::HasOut { term, rels } => AxqlAtom::HasOut {
                term: term.to_axql_term()?,
                rels: rels.clone(),
            },
            QueryAtomIrV1::Attrs { term, pairs } => {
                let mut out_pairs: Vec<(String, String)> = pairs
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                out_pairs.sort_by(|a, b| a.0.cmp(&b.0));
                AxqlAtom::Attrs {
                    term: term.to_axql_term()?,
                    pairs: out_pairs,
                }
            }
            QueryAtomIrV1::Shape {
                term,
                type_name,
                rels,
                attrs,
            } => {
                let mut out_attrs: Vec<(String, String)> = attrs
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                out_attrs.sort_by(|a, b| a.0.cmp(&b.0));
                AxqlAtom::Shape {
                    term: term.to_axql_term()?,
                    type_name: type_name.clone(),
                    rels: rels.clone(),
                    attrs: out_attrs,
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn query_ir_v1_compiles_where_clause() -> Result<()> {
        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["x"],
              "where": [
                {"kind": "type", "term": "?x", "type": "Node"},
                {"kind": "attr_eq", "term": "?x", "key": "name", "value": "a"}
              ],
              "limit": 10
            }"#,
        )?;
        let axql = q.to_axql_query()?;
        assert_eq!(axql.select_vars, vec!["?x"]);
        assert_eq!(axql.disjuncts.len(), 1);
        assert_eq!(axql.limit, 10);
        Ok(())
    }

    #[test]
    fn query_ir_v1_compiles_disjunction() -> Result<()> {
        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "disjuncts": [
                [ {"kind": "type", "term": "?x", "type": "A"} ],
                [ {"kind": "type", "term": "?x", "type": "B"} ]
              ]
            }"#,
        )?;
        let axql = q.to_axql_query()?;
        assert_eq!(axql.disjuncts.len(), 2);
        Ok(())
    }

    #[test]
    fn query_ir_term_string_is_name_lookup() -> Result<()> {
        let t: QueryTermIrV1 = serde_json::from_str(r#""Alice""#)?;
        let ax = t.to_axql_term()?;
        assert_eq!(
            ax,
            AxqlTerm::Lookup {
                key: "name".to_string(),
                value: "Alice".to_string()
            }
        );
        Ok(())
    }

    #[test]
    fn query_ir_v1_from_axql_roundtrips_basic() -> Result<()> {
        let axql = r#"select ?x where ?x : Node, attr(?x, "name", "a") limit 10"#;
        let parsed = crate::axql::parse_axql_query(axql)?;
        let ir = QueryIrV1::from_axql_query(&parsed);
        let back = ir.to_axql_query()?;
        assert_eq!(back.select_vars, vec!["?x"]);
        assert_eq!(back.limit, 10);
        assert_eq!(back.disjuncts.len(), 1);
        assert_eq!(back.disjuncts[0].len(), 2);
        Ok(())
    }

    fn rel_name_strategy() -> impl Strategy<Value = String> {
        // Keep relation names in the "identifier-ish" subset of AxQL for stable parsing.
        "[a-z][a-z0-9_]{0,6}".prop_map(|s| s)
    }

    fn type_name_strategy() -> impl Strategy<Value = String> {
        "[A-Z][A-Za-z0-9_]{0,10}".prop_map(|s| s)
    }

    fn attr_key_strategy() -> impl Strategy<Value = String> {
        // Attribute keys are rendered as string literals, so we allow a wider set,
        // but keep it small and ASCII for predictable shrinking.
        "[a-z][a-z0-9_]{0,10}".prop_map(|s| s)
    }

    fn attr_value_strategy() -> impl Strategy<Value = String> {
        // Keep values short; `axql_string_lit` escapes these.
        // Note: the current AxQL string literal parser does not accept `""`
        // (empty string), so we avoid generating it here.
        "[A-Za-z0-9_ \\-]{1,16}".prop_map(|s| s)
    }

    fn path_expr_strategy() -> impl Strategy<Value = String> {
        // Generate only simple chains `r0/r1/r2` to avoid grammar edge-cases in proptests.
        prop::collection::vec(rel_name_strategy(), 1..=4).prop_map(|parts| parts.join("/"))
    }

    fn query_ir_v1_strategy() -> impl Strategy<Value = QueryIrV1> {
        // Generate small, parseable `query_ir_v1` values that only use the core atoms:
        // Type / Edge / AttrEq. This is the subset we expect tool/LLM integrations
        // to emit most often.
        prop::collection::hash_set("[a-z]{1,6}", 1..=4).prop_flat_map(|vars_set| {
            let mut vars: Vec<String> = vars_set.into_iter().map(|v| format!("?{v}")).collect();
            vars.sort();
            let var_term = prop::sample::select(vars.clone()).prop_map(QueryTermIrV1::Simple);

            let atom = prop_oneof![
                (var_term.clone(), type_name_strategy()).prop_map(|(term, type_name)| {
                    QueryAtomIrV1::Type { term, type_name }
                }),
                (
                    var_term.clone(),
                    path_expr_strategy(),
                    var_term.clone(),
                )
                    .prop_map(|(left, path, right)| QueryAtomIrV1::Edge { left, path, right }),
                (var_term.clone(), attr_key_strategy(), attr_value_strategy()).prop_map(
                    |(term, key, value)| QueryAtomIrV1::AttrEq { term, key, value },
                ),
            ];

            let disjunct = prop::collection::vec(atom, 1..=6);
            let disjuncts = prop::collection::vec(disjunct, 1..=3);

            (Just(vars), disjuncts, 1usize..=50).prop_map(|(vars, disjuncts, limit)| QueryIrV1 {
                version: QUERY_IR_V1_VERSION,
                select_vars: vars,
                where_atoms: None,
                disjuncts: Some(disjuncts),
                limit: Some(limit),
                max_hops: None,
                min_confidence: None,
                contexts: Vec::new(),
            })
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 256,
            failure_persistence: None,
            ..ProptestConfig::default()
        })]

        #[test]
        fn query_ir_v1_json_roundtrips(q in query_ir_v1_strategy()) {
            let json = serde_json::to_value(&q).expect("serialize QueryIrV1");
            let back: QueryIrV1 = serde_json::from_value(json.clone()).expect("deserialize QueryIrV1");
            let json2 = serde_json::to_value(&back).expect("serialize QueryIrV1");
            prop_assert_eq!(json, json2);
        }

        #[test]
        fn query_ir_v1_to_axql_text_parses(q in query_ir_v1_strategy()) {
            let text = q.to_axql_text().expect("render query_ir_v1 to AxQL");
            let parsed = crate::axql::parse_axql_query(&text).expect("AxQL must parse");
            // Sanity: should always have a body (we always generate disjuncts).
            prop_assert!(!parsed.disjuncts.is_empty());
        }

        #[test]
        fn query_ir_v1_roundtrips_via_axql(q in query_ir_v1_strategy()) {
            let axql_1 = q.to_axql_query().expect("compile query_ir_v1");
            let text = render_axql_query(&axql_1);
            let parsed = crate::axql::parse_axql_query(&text).expect("parse rendered AxQL");
            prop_assert_eq!(&parsed, &axql_1);

            // `from_axql_query` is best-effort, but for this core subset we expect
            // semantics-preserving roundtrip.
            let ir2 = QueryIrV1::from_axql_query(&parsed);
            let axql_2 = ir2.to_axql_query().expect("compile roundtripped query_ir_v1");
            prop_assert_eq!(&axql_2, &parsed);
        }
    }
}
