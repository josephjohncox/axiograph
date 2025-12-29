//! AxQL: A small query language for Axiograph / PathDB.
//!
//! Design goals:
//! - **Readable** in a REPL (datalog-ish pattern matching).
//! - **Path-aware**: `?x -rel_0/rel_1-> ?y` expands into intermediate joins.
//! - **Shape-friendly**: shapes are conjunctions of type/edge/attr constraints.
//! - **Homomorphism semantics**: conjunctions are evaluated as graph pattern matches.
//!
//! ## Type-directed execution (and Lean correspondence)
//!
//! AxQL is intentionally “small” on the surface, but the execution layer is
//! **type-directed** when the `.axi` meta-plane is present:
//!
//! - the planner uses `MetaPlaneIndex` as a type layer (schema/relation/field checks),
//! - it inserts implied type atoms and supertypes closure (elaboration),
//! - and it exploits imported theory constraints (keys/functionals) for near-index lookups.
//!
//! This lines up with the Lean semantics story in:
//! - `docs/explanation/TOPOS_THEORY.md` and `lean/Axiograph/Topos/Overview.lean` (schemas-as-categories,
//!   instances-as-functors, contexts as world indexing).
//!
//! The trusted boundary remains certificates: Rust computes; Lean checks
//! (`lean/Axiograph/VerifyMain.lean`).
//!
//! Non-goals (for the initial version):
//! - recursion / fixpoint Datalog rules
//! - full SQL support (a small SQL-ish surface exists, but it compiles into AxQL)

#![allow(dead_code)]

use anyhow::{anyhow, Result};
use nom::branch::alt;
use nom::bytes::complete::{escaped_transform, is_not, tag, take_while, take_while1};
use nom::character::complete::{char as pchar, digit1, multispace0, multispace1};
use nom::combinator::{all_consuming, map, map_res, opt, recognize};
use nom::multi::{many0, separated_list1};
use nom::number::complete::recognize_float;
use nom::sequence::{delimited, preceded, tuple};
use nom::IResult;
use roaring::RoaringBitmap;
use std::collections::{BTreeMap, HashMap, HashSet};

use axiograph_dsl::schema_v1::{parse_path_expr_v3, PathExprV3};
use axiograph_pathdb::axi_meta::{
    ATTR_AXI_RELATION, ATTR_AXI_SCHEMA, REL_AXI_FACT_IN_CONTEXT,
};
use axiograph_pathdb::axi_meta::{
    ATTR_REWRITE_RULE_LHS, ATTR_REWRITE_RULE_ORIENTATION, ATTR_REWRITE_RULE_RHS,
    ATTR_REWRITE_RULE_VARS, META_ATTR_ID, META_TYPE_REWRITE_RULE,
};
use axiograph_pathdb::axi_semantics::MetaPlaneIndex;
use axiograph_pathdb::certificate::{
    CertificatePayloadV2, CertificateV2, FixedPointProbability, QueryAtomV1, QueryAtomWitnessV1,
    QueryBindingV1, QueryRegexV1, QueryResultProofV1, QueryResultProofV2, QueryRowV1, QueryRowV2,
    QueryTermV1, QueryV1, QueryV2, ReachabilityProofV2,
    QueryAtomV3, QueryAtomWitnessV3, QueryBindingV3, QueryRegexV3, QueryResultProofV3, QueryRowV3,
    QueryTermV3, QueryV3, PathRewriteStepV3, RewriteDerivationProofV3,
};
use axiograph_pathdb::witness;

/// A context/world selector for query scoping.
///
/// Canonical `.axi` supports context via tuple fields (e.g. `ctx=Accepted`),
/// and PathDB derives a uniform edge `axi_fact_in_context` on import.
///
/// In AxQL, context scoping is an *optional* filter on fact-node matches:
///
/// - Facts outside the selected context(s) are ignored.
/// - Facts with no context edge are treated as *unscoped* (they will not match
///   a scoped query).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AxqlContextSpec {
    /// Refer to a context by numeric entity id.
    EntityId(u32),
    /// Refer to a context by its `name` attribute value.
    Name(String),
}

impl AxqlContextSpec {
    fn render(&self) -> String {
        match self {
            AxqlContextSpec::EntityId(id) => id.to_string(),
            AxqlContextSpec::Name(name) => name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AxqlQuery {
    pub select_vars: Vec<String>,
    /// Disjuncts (OR-branches) in the `where` clause.
    ///
    /// Each disjunct is a conjunction of atoms.
    pub disjuncts: Vec<Vec<AxqlAtom>>,
    pub limit: usize,
    /// Optional context/world scoping.
    ///
    /// For a single context, the query is lowered into a conjunction that can be
    /// certificate-checked (`fact -axi_fact_in_context-> ctx`).
    ///
    /// For multiple contexts, AxQL currently treats scoping as an execution-time
    /// filter (union of contexts). This is **not** part of the certified query
    /// core yet.
    pub contexts: Vec<AxqlContextSpec>,
    /// Optional bound on how many graph edges a path constraint may traverse.
    ///
    /// This applies to RPQ-style path constraints (including `*`/`+`).
    /// A value of `0` means only empty-path matches are allowed.
    pub max_hops: Option<u32>,
    /// Minimum per-edge confidence threshold for all path/edge atoms.
    ///
    /// Semantics: edges with `confidence < min_confidence` are ignored for:
    /// - `?x -rel-> ?y` atoms
    /// - RPQ atoms (`*`, `+`, alternation, etc)
    ///
    /// This is intended for "probabilistic" / evidence-scoped querying:
    /// keep low-confidence edges explicit, but make it easy to exclude them in queries.
    pub min_confidence: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AxqlAtom {
    Type {
        term: AxqlTerm,
        type_name: String,
    },
    Edge {
        left: AxqlTerm,
        path: AxqlPathExpr,
        right: AxqlTerm,
    },
    AttrEq {
        term: AxqlTerm,
        key: String,
        value: String,
    },
    /// Approximate attribute substring constraint (case-insensitive).
    ///
    /// This is intended for discovery workflows and REPL convenience. It is
    /// **not** part of the certified query core, so queries using it cannot be
    /// turned into Lean-checkable query certificates.
    AttrContains {
        term: AxqlTerm,
        key: String,
        needle: String,
    },
    /// Approximate attribute full-text search (token-based).
    ///
    /// This is intended for discovery workflows and REPL convenience. It is
    /// **not** part of the certified query core, so queries using it cannot be
    /// turned into Lean-checkable query certificates.
    AttrFts {
        term: AxqlTerm,
        key: String,
        query: String,
    },
    /// Approximate attribute fuzzy match (case-insensitive Levenshtein).
    ///
    /// This is intended for discovery workflows and REPL convenience. It is
    /// **not** part of the certified query core, so queries using it cannot be
    /// turned into Lean-checkable query certificates.
    AttrFuzzy {
        term: AxqlTerm,
        key: String,
        needle: String,
        max_dist: usize,
    },
    /// N-ary relation (fact) atom, matching the canonical `.axi` import shape.
    ///
    /// Canonical `.axi` instances are imported into PathDB by *reifying* each
    /// relation tuple as a dedicated "fact node" with:
    /// - attribute `axi_relation = <relation name>`, and
    /// - edges `fact -field-> value` for each declared field.
    ///
    /// This atom is sugar for that representation:
    ///
    /// - `Flow(from=a, to=b)` introduces an implicit fact node `f` and expands to:
    ///   - `attr(f, "axi_relation", "Flow")`
    ///   - `f -from-> a`
    ///   - `f -to-> b`
    /// - `?f = Flow(...)` binds the fact node explicitly so it can be selected.
    ///
    /// Because it expands into the certified core atoms (`AttrEq` + `Edge`),
    /// it remains compatible with query certificates (when anchored to a snapshot).
    Fact {
        /// Optional explicit fact/tuple variable (must be a variable term).
        fact: Option<AxqlTerm>,
        relation: String,
        fields: Vec<(String, AxqlTerm)>,
    },
    /// Shape macro: `has(?x, rel_0, rel_1, ...)` expands into multiple edge atoms
    /// of the form `?x -rel_i-> _`.
    HasOut {
        term: AxqlTerm,
        rels: Vec<String>,
    },
    /// Shape macro: `attrs(?x, key="value", ...)` expands into multiple `attr(...)` atoms.
    Attrs {
        term: AxqlTerm,
        pairs: Vec<(String, String)>,
    },
    /// Shape literal: `?x { rel_0, rel_1, name="node_42", is Node }`.
    ///
    /// This is purely surface-level sugar that expands into a conjunction of
    /// `Type`, `Edge` (with wildcard target), and `AttrEq` atoms.
    Shape {
        term: AxqlTerm,
        type_name: Option<String>,
        rels: Vec<String>,
        attrs: Vec<(String, String)>,
    },
}

/// Regular-path query (RPQ) expression over relation labels.
///
/// This is intentionally small and maps well to both:
/// - SPARQL property paths, and
/// - mathlib regular-expression semantics on words of labels.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AxqlRegex {
    Epsilon,
    Rel(String),
    Seq(Vec<AxqlRegex>),
    Alt(Vec<AxqlRegex>),
    Star(Box<AxqlRegex>),
    Plus(Box<AxqlRegex>),
    Opt(Box<AxqlRegex>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AxqlPathExpr {
    pub regex: AxqlRegex,
}

impl AxqlPathExpr {
    pub fn rel(rel: impl Into<String>) -> Self {
        Self {
            regex: AxqlRegex::Rel(rel.into()),
        }
    }

    pub fn seq(rels: Vec<String>) -> Self {
        let parts = rels.into_iter().map(AxqlRegex::Rel).collect::<Vec<_>>();
        Self {
            regex: AxqlRegex::Seq(parts),
        }
    }

    pub fn star(rel: impl Into<String>) -> Self {
        Self {
            regex: AxqlRegex::Star(Box::new(AxqlRegex::Rel(rel.into()))),
        }
    }

    pub fn plus(rel: impl Into<String>) -> Self {
        Self {
            regex: AxqlRegex::Plus(Box::new(AxqlRegex::Rel(rel.into()))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AxqlTerm {
    Var(String),
    Const(u32),
    Wildcard,
    /// Macro term: lookup entity ids by attribute equality.
    ///
    /// Example: `name("node_42")` becomes `.Lookup { key: "name", value: "node_42" }`.
    Lookup {
        key: String,
        value: String,
    },
}

#[derive(Debug, Clone)]
pub struct AxqlResult {
    pub selected_vars: Vec<String>,
    pub rows: Vec<BTreeMap<String, u32>>,
    pub truncated: bool,
}

pub fn parse_axql_query(input: &str) -> Result<AxqlQuery> {
    let (_, mut q) = all_consuming(ws(axql_query))(input)
        .map_err(|e| anyhow!("failed to parse axql query: {e:?}"))?;
    if let Some(c) = q.min_confidence {
        if !c.is_finite() || !(0.0..=1.0).contains(&c) {
            return Err(anyhow!(
                "min_confidence must be a finite number in [0, 1] (got {c})"
            ));
        }
        // Defensive normalization: keep it within bounds for downstream comparisons.
        q.min_confidence = Some(c.clamp(0.0, 1.0));
    }
    Ok(q)
}

pub fn parse_axql_path_expr(input: &str) -> Result<AxqlPathExpr> {
    let (_, p) = all_consuming(ws(path_expr))(input)
        .map_err(|e| anyhow!("failed to parse axql path expr: {e:?}"))?;
    Ok(p)
}

pub fn follow_path_expr(
    db: &axiograph_pathdb::PathDB,
    start: u32,
    expr: &AxqlPathExpr,
    max_hops: Option<u32>,
) -> Result<RoaringBitmap> {
    if let Some(rels) = simple_chain(&expr.regex) {
        let path_refs: Vec<&str> = rels.iter().map(|s| s.as_str()).collect();
        return Ok(db.follow_path(start, &path_refs));
    }

    let mut rpq = RpqContext::new(db, std::slice::from_ref(&expr.regex), max_hops, None)?;
    rpq.reachable_set(db, 0, start)
}

pub fn execute_axql_query(db: &axiograph_pathdb::PathDB, query: &AxqlQuery) -> Result<AxqlResult> {
    let meta = MetaPlaneIndex::from_db(db)?;
    execute_axql_query_with_meta(db, query, Some(&meta))
}

pub fn certify_axql_query(
    db: &axiograph_pathdb::PathDB,
    query: &AxqlQuery,
) -> Result<CertificateV2> {
    let meta = MetaPlaneIndex::from_db(db)?;
    certify_axql_query_with_meta(db, query, Some(&meta))
}

/// Compute a stable digest for an AxQL query IR.
///
/// This is used as a key for the REPL's compiled-query cache, together with a
/// snapshot key.
pub fn axql_query_ir_digest_v1(query: &AxqlQuery) -> String {
    use std::fmt::Write as _;

    let mut s = String::new();
    let _ = write!(&mut s, "select={:?};", query.select_vars);
    let _ = write!(&mut s, "limit={};", query.limit);
    let _ = write!(&mut s, "contexts={:?};", query.contexts);
    let _ = write!(&mut s, "max_hops={:?};", query.max_hops);
    let _ = write!(
        &mut s,
        "min_conf_bits={:?};",
        query.min_confidence.map(|c| c.to_bits())
    );
    let _ = write!(&mut s, "disjuncts={:?};", query.disjuncts);

    axiograph_dsl::digest::axi_digest_v1(&s)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AxqlQueryCacheKey {
    snapshot: String,
    query_ir: String,
}

pub(crate) struct AxqlPreparedQueryCache {
    entries: HashMap<AxqlQueryCacheKey, PreparedAxqlQueryExpr>,
    lru: std::collections::VecDeque<AxqlQueryCacheKey>,
    max_entries: usize,
}

impl AxqlPreparedQueryCache {
    const DEFAULT_MAX_ENTRIES: usize = 32;

    pub(crate) fn new(max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            lru: std::collections::VecDeque::new(),
            max_entries: max_entries.max(1),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.lru.clear();
    }

    fn touch(&mut self, key: &AxqlQueryCacheKey) {
        if let Some(pos) = self.lru.iter().position(|k| k == key) {
            self.lru.remove(pos);
        }
        self.lru.push_back(key.clone());
    }

    pub(crate) fn get_mut(
        &mut self,
        key: &AxqlQueryCacheKey,
    ) -> Option<&mut PreparedAxqlQueryExpr> {
        if self.entries.contains_key(key) {
            self.touch(key);
            return self.entries.get_mut(key);
        }
        None
    }

    pub(crate) fn insert(&mut self, key: AxqlQueryCacheKey, value: PreparedAxqlQueryExpr) {
        self.entries.insert(key.clone(), value);
        self.touch(&key);

        let limit = if self.max_entries == 0 {
            Self::DEFAULT_MAX_ENTRIES
        } else {
            self.max_entries
        };
        while self.lru.len() > limit {
            if let Some(oldest) = self.lru.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }
}

impl Default for AxqlPreparedQueryCache {
    fn default() -> Self {
        Self::new(Self::DEFAULT_MAX_ENTRIES)
    }
}

pub(crate) fn axql_query_cache_key(snapshot_key: &str, query: &AxqlQuery) -> AxqlQueryCacheKey {
    AxqlQueryCacheKey {
        snapshot: snapshot_key.to_string(),
        query_ir: axql_query_ir_digest_v1(query),
    }
}

pub(crate) fn execute_axql_query_cached(
    db: &axiograph_pathdb::PathDB,
    query: &AxqlQuery,
    meta: Option<&MetaPlaneIndex>,
    snapshot_key: &str,
    cache: &mut AxqlPreparedQueryCache,
) -> Result<AxqlResult> {
    let key = axql_query_cache_key(snapshot_key, query);
    if let Some(prepared) = cache.get_mut(&key) {
        return prepared.execute(db, meta);
    }
    let prepared = prepare_axql_query_with_meta(db, query, meta)?;
    cache.insert(key.clone(), prepared);
    cache
        .get_mut(&key)
        .expect("query cache insert")
        .execute(db, meta)
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AxqlElaborationReport {
    /// Map `?var -> inferred types` (including supertypes closure).
    pub inferred_types: BTreeMap<String, Vec<String>>,
    /// Additional notes (e.g., ambiguity) that are helpful to show in a REPL.
    pub notes: Vec<String>,
    /// Optional elaboration rewrite witnesses (e.g. `.axi` path canonicalization).
    pub elaboration_rewrites: Vec<AxqlElaborationRewriteStepV1>,
}

/// One recorded rewrite application during AxQL elaboration.
///
/// This is intended to justify “rewrite-normalized” query atoms in a way that
/// Lean can independently check against the anchored `.axi` rewrite rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AxqlElaborationRewriteStepV1 {
    pub theory_name: String,
    pub rule_name: String,
    pub input: PathExprV3,
    pub output: PathExprV3,
}

/// A compiled AxQL query for repeated execution against a single snapshot.
///
/// This caches:
/// - the lowered query,
/// - the initial candidate bitmaps + join order,
/// - and the compiled RPQ automata (plus per-source reachability cache).
pub(crate) struct PreparedAxqlQuery {
    lowered: LoweredQuery,
    plan: QueryPlan,
    rpq: RpqContext,
    elaboration: AxqlElaborationReport,
}

/// A compiled AxQL query that may contain disjunction (OR).
pub(crate) enum PreparedAxqlQueryExpr {
    Conjunction(PreparedAxqlQuery),
    Disjunction(PreparedAxqlDisjunction),
}

pub(crate) struct PreparedAxqlDisjunction {
    disjuncts: Vec<PreparedAxqlQuery>,
    elaboration: AxqlElaborationReport,
    select_vars: Vec<String>,
    limit: usize,
}

/// A lowered AxQL query that has been checked against the meta-plane and
/// elaborated with implied typing assumptions.
///
/// This is a typestate boundary: once you have this value, downstream passes
/// can assume (and do not need to re-check) basic schema/type correctness.
struct TypecheckedLoweredQuery {
    lowered: LoweredQuery,
    elaboration: AxqlElaborationReport,
}

impl TypecheckedLoweredQuery {
    fn from_disjunct(
        db: &axiograph_pathdb::PathDB,
        query: &AxqlQuery,
        disjunct: &[AxqlAtom],
        meta: Option<&MetaPlaneIndex>,
    ) -> Result<Self> {
        let mut lowered = lower_query_disjunct(query, disjunct)?;
        let mut elaboration = lowered.typecheck_and_elaborate(db, meta)?;
        lowered.canonicalize_paths(db, meta, &mut elaboration)?;
        Ok(Self {
            lowered,
            elaboration,
        })
    }

    fn prepare(
        self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&MetaPlaneIndex>,
    ) -> Result<PreparedAxqlQuery> {
        let mut rpq = RpqContext::new(
            db,
            &self.lowered.rpqs,
            self.lowered.max_hops,
            self.lowered.min_confidence,
        )?;
        let plan = self.lowered.plan(db, &mut rpq, meta)?;
        Ok(PreparedAxqlQuery {
            lowered: self.lowered,
            plan,
            rpq,
            elaboration: self.elaboration,
        })
    }

    fn certify(
        self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&MetaPlaneIndex>,
    ) -> Result<CertificateV2> {
        self.lowered.certify(db, meta)
    }

    fn certify_v3(
        self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&MetaPlaneIndex>,
        axi_digest_v1: &str,
    ) -> Result<CertificateV2> {
        self.lowered
            .certify_v3(db, meta, axi_digest_v1, &self.elaboration)
    }
}

/// A typechecked AxQL query expression: either a single conjunctive query, or a
/// disjunction (OR) of conjunctive branches.
enum TypecheckedAxqlQueryExpr {
    Conjunction(TypecheckedLoweredQuery),
    Disjunction(Vec<TypecheckedLoweredQuery>),
}

impl TypecheckedAxqlQueryExpr {
    fn from_parsed(
        db: &axiograph_pathdb::PathDB,
        query: &AxqlQuery,
        meta: Option<&MetaPlaneIndex>,
    ) -> Result<Self> {
        if query.disjuncts.is_empty() {
            return Err(anyhow!("AxQL query must have at least one disjunct"));
        }

        let mut out: Vec<TypecheckedLoweredQuery> = Vec::with_capacity(query.disjuncts.len());
        for disjunct in &query.disjuncts {
            out.push(TypecheckedLoweredQuery::from_disjunct(
                db, query, disjunct, meta,
            )?);
        }

        if out.len() == 1 {
            Ok(Self::Conjunction(
                out.into_iter().next().expect("len checked"),
            ))
        } else {
            Ok(Self::Disjunction(out))
        }
    }

    fn prepare(
        self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&MetaPlaneIndex>,
        select_vars: Vec<String>,
        limit: usize,
    ) -> Result<PreparedAxqlQueryExpr> {
        match self {
            TypecheckedAxqlQueryExpr::Conjunction(q) => {
                Ok(PreparedAxqlQueryExpr::Conjunction(q.prepare(db, meta)?))
            }
            TypecheckedAxqlQueryExpr::Disjunction(disjuncts) => {
                let mut prepared: Vec<PreparedAxqlQuery> = Vec::with_capacity(disjuncts.len());
                let mut merged = AxqlElaborationReport::default();
                merged
                    .notes
                    .push(format!("disjunction: {} branches", disjuncts.len()));

                for d in disjuncts {
                    let p = d.prepare(db, meta)?;
                    for (var, tys) in &p.elaboration.inferred_types {
                        merged
                            .inferred_types
                            .entry(var.clone())
                            .or_default()
                            .extend(tys.iter().cloned());
                    }
                    merged.notes.extend(p.elaboration.notes.iter().cloned());
                    merged
                        .elaboration_rewrites
                        .extend(p.elaboration.elaboration_rewrites.iter().cloned());
                    prepared.push(p);
                }

                let select_vars = if !select_vars.is_empty() {
                    select_vars
                } else {
                    // Implicit select (`select *`) for a disjunction:
                    // choose the intersection of free vars across disjuncts.
                    //
                    // This matches the UCQ intuition: variables not common to all
                    // disjuncts are existential, not returned.
                    let mut common = prepared
                        .first()
                        .map(|p| p.lowered.select_vars.clone())
                        .unwrap_or_default();
                    for p in &prepared[1..] {
                        let set: HashSet<&str> =
                            p.lowered.select_vars.iter().map(|s| s.as_str()).collect();
                        common.retain(|v| set.contains(v.as_str()));
                    }
                    merged.notes.push(format!(
                        "implicit select for disjunction = intersection of disjunct free vars: {}",
                        if common.is_empty() {
                            "(none)".to_string()
                        } else {
                            common.join(", ")
                        }
                    ));
                    common
                };

                for tys in merged.inferred_types.values_mut() {
                    tys.sort();
                    tys.dedup();
                }
                merged.notes.sort();
                merged.notes.dedup();

                Ok(PreparedAxqlQueryExpr::Disjunction(
                    PreparedAxqlDisjunction {
                        disjuncts: prepared,
                        elaboration: merged,
                        select_vars,
                        limit,
                    },
                ))
            }
        }
    }

    fn certify(
        self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&MetaPlaneIndex>,
        limit: usize,
    ) -> Result<CertificateV2> {
        match self {
            TypecheckedAxqlQueryExpr::Conjunction(q) => q.certify(db, meta),
            TypecheckedAxqlQueryExpr::Disjunction(disjuncts) => {
                certify_disjunctive_query(db, disjuncts, meta, limit)
            }
        }
    }
}

impl PreparedAxqlQuery {
    pub(crate) fn execute(
        &mut self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&MetaPlaneIndex>,
    ) -> Result<AxqlResult> {
        if self.lowered.vars.is_empty() {
            let ok = self.lowered.check_grounded(db, &mut self.rpq, meta)?;
            let rows = if ok { vec![BTreeMap::new()] } else { vec![] };
            return Ok(AxqlResult {
                selected_vars: Vec::new(),
                rows,
                truncated: false,
            });
        }

        if let Some(result) = self
            .lowered
            .fast_single_path_query(db, &mut self.rpq, meta)?
        {
            return Ok(result);
        }

        let mut assigned: Vec<Option<u32>> = vec![None; self.lowered.vars.len()];
        let mut rows: Vec<BTreeMap<String, u32>> = Vec::new();
        let mut truncated = false;

        self.lowered.search(
            db,
            &self.plan.candidates,
            &self.plan.order,
            &self.plan.atom_order,
            0,
            &mut assigned,
            &mut rows,
            &mut truncated,
            &mut self.rpq,
            meta,
        )?;

        Ok(AxqlResult {
            selected_vars: self.lowered.select_vars.clone(),
            rows,
            truncated,
        })
    }

    pub(crate) fn elaborated_query_text(&self) -> String {
        self.lowered.render_as_axql()
    }

    pub(crate) fn elaboration_report(&self) -> &AxqlElaborationReport {
        &self.elaboration
    }

    fn explain_plan_lines(&self, indent: Option<&str>) -> Vec<String> {
        let indent = indent.unwrap_or("");

        fn render_term(vars: &[String], term: &LoweredTerm) -> String {
            match term {
                LoweredTerm::Var(v) => vars.get(*v).cloned().unwrap_or_else(|| format!("?v{v}")),
                LoweredTerm::Const(id) => id.to_string(),
            }
        }

        fn render_regex(re: &AxqlRegex) -> String {
            match re {
                AxqlRegex::Epsilon => "ε".to_string(),
                AxqlRegex::Rel(r) => r.clone(),
                AxqlRegex::Seq(parts) => parts.iter().map(render_regex).collect::<Vec<_>>().join("/"),
                AxqlRegex::Alt(parts) => {
                    format!(
                        "({})",
                        parts.iter().map(render_regex).collect::<Vec<_>>().join("|")
                    )
                }
                AxqlRegex::Star(inner) => format!("{}*", render_regex(inner)),
                AxqlRegex::Plus(inner) => format!("{}+", render_regex(inner)),
                AxqlRegex::Opt(inner) => format!("{}?", render_regex(inner)),
            }
        }

        let render_atom = |atom: &LoweredAtom| -> String {
            match atom {
                LoweredAtom::Type { term, type_name } => {
                    format!("{} : {}", render_term(&self.lowered.vars, term), type_name)
                }
                LoweredAtom::AttrEq { term, key, value } => format!(
                    "attr({}, \"{}\", \"{}\")",
                    render_term(&self.lowered.vars, term),
                    key,
                    value
                ),
                LoweredAtom::AttrContains { term, key, needle } => format!(
                    "contains({}, \"{}\", \"{}\")",
                    render_term(&self.lowered.vars, term),
                    key,
                    needle
                ),
                LoweredAtom::AttrFts { term, key, query } => format!(
                    "fts({}, \"{}\", \"{}\")",
                    render_term(&self.lowered.vars, term),
                    key,
                    query
                ),
                LoweredAtom::AttrFuzzy {
                    term,
                    key,
                    needle,
                    max_dist,
                } => format!(
                    "fuzzy({}, \"{}\", \"{}\", {})",
                    render_term(&self.lowered.vars, term),
                    key,
                    needle,
                    max_dist
                ),
                LoweredAtom::Edge { left, rel, right } => format!(
                    "{} -{}-> {}",
                    render_term(&self.lowered.vars, left),
                    rel,
                    render_term(&self.lowered.vars, right)
                ),
                LoweredAtom::Rpq {
                    left,
                    rpq_id,
                    right,
                } => {
                    let rpq = self
                        .lowered
                        .rpqs
                        .get(*rpq_id)
                        .map(render_regex)
                        .unwrap_or_else(|| format!("#{rpq_id}"));
                    format!(
                        "{} -{}-> {}",
                        render_term(&self.lowered.vars, left),
                        rpq,
                        render_term(&self.lowered.vars, right)
                    )
                }
            }
        };

        let mut lines: Vec<String> = Vec::new();

        if self.lowered.vars.is_empty() {
            lines.push(format!("{indent}plan: grounded (no vars)"));
            return lines;
        }

        // Join order (smallest domains first).
        let mut join_parts: Vec<String> = Vec::new();
        for vid in &self.plan.order {
            let name = self
                .lowered
                .vars
                .get(*vid)
                .cloned()
                .unwrap_or_else(|| format!("?v{vid}"));
            let sz = self.plan.candidates.get(*vid).map(|c| c.len()).unwrap_or(0);
            join_parts.push(format!("{name}({sz})"));
        }
        lines.push(format!("{indent}join order: {}", join_parts.join(" → ")));

        // Candidate domains.
        lines.push(format!("{indent}domains:"));
        for vid in &self.plan.order {
            let name = self
                .lowered
                .vars
                .get(*vid)
                .cloned()
                .unwrap_or_else(|| format!("?v{vid}"));
            let sz = self.plan.candidates.get(*vid).map(|c| c.len()).unwrap_or(0);
            lines.push(format!("{indent}  - {name}: {sz}"));
        }

        // Atom order (cheap-first).
        lines.push(format!("{indent}atom order:"));
        let max_atoms = 32usize;
        for (i, atom_idx) in self.plan.atom_order.iter().enumerate().take(max_atoms) {
            let atom = self
                .lowered
                .atoms
                .get(*atom_idx)
                .map(render_atom)
                .unwrap_or_else(|| format!("<missing atom {atom_idx}>"));
            lines.push(format!("{indent}  {}. {}", i + 1, atom));
        }
        if self.plan.atom_order.len() > max_atoms {
            lines.push(format!("{indent}  …"));
        }

        // Index hints (best-effort): show which vars are constrained by FactIndex.
        for atom in &self.lowered.atoms {
            let LoweredAtom::AttrEq { term, key, value } = atom else {
                continue;
            };
            let LoweredTerm::Var(v) = term else {
                continue;
            };
            if key == axiograph_pathdb::axi_meta::ATTR_AXI_RELATION {
                let name = self
                    .lowered
                    .vars
                    .get(*v)
                    .cloned()
                    .unwrap_or_else(|| format!("?v{v}"));
                lines.push(format!(
                    "{indent}hint: FactIndex prunes {name} by axi_relation={value}"
                ));
            }
        }

        lines
    }
}

impl PreparedAxqlQueryExpr {
    pub(crate) fn execute(
        &mut self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&MetaPlaneIndex>,
    ) -> Result<AxqlResult> {
        match self {
            PreparedAxqlQueryExpr::Conjunction(q) => q.execute(db, meta),
            PreparedAxqlQueryExpr::Disjunction(q) => q.execute(db, meta),
        }
    }

    pub(crate) fn elaborated_query_text(&self) -> String {
        match self {
            PreparedAxqlQueryExpr::Conjunction(q) => q.elaborated_query_text(),
            PreparedAxqlQueryExpr::Disjunction(q) => q.elaborated_query_text(),
        }
    }

    pub(crate) fn elaboration_report(&self) -> &AxqlElaborationReport {
        match self {
            PreparedAxqlQueryExpr::Conjunction(q) => q.elaboration_report(),
            PreparedAxqlQueryExpr::Disjunction(q) => &q.elaboration,
        }
    }

    /// Human-readable plan/debug output for `q --explain`.
    pub(crate) fn explain_plan_lines(&self) -> Vec<String> {
        match self {
            PreparedAxqlQueryExpr::Conjunction(q) => q.explain_plan_lines(None),
            PreparedAxqlQueryExpr::Disjunction(q) => {
                let mut out: Vec<String> = Vec::new();
                out.push(format!("disjunction: {} branch(es)", q.disjuncts.len()));
                for (i, d) in q.disjuncts.iter().enumerate() {
                    out.push(format!("branch {}:", i + 1));
                    out.extend(d.explain_plan_lines(Some("  ")).into_iter());
                }
                out
            }
        }
    }
}

impl PreparedAxqlDisjunction {
    fn execute(
        &mut self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&MetaPlaneIndex>,
    ) -> Result<AxqlResult> {
        let mut rows: Vec<BTreeMap<String, u32>> = Vec::new();
        let mut truncated = false;

        for disjunct in &mut self.disjuncts {
            if rows.len() >= self.limit {
                truncated = true;
                break;
            }
            let result = disjunct.execute(db, meta)?;
            for row in result.rows {
                if rows.len() >= self.limit {
                    truncated = true;
                    break;
                }
                let mut filtered: BTreeMap<String, u32> = BTreeMap::new();
                for v in &self.select_vars {
                    if let Some(value) = row.get(v).copied() {
                        filtered.insert(v.clone(), value);
                    }
                }
                rows.push(filtered);
            }
            if truncated {
                break;
            }
        }

        Ok(AxqlResult {
            selected_vars: self.select_vars.clone(),
            rows,
            truncated,
        })
    }

    fn elaborated_query_text(&self) -> String {
        let mut out = String::new();
        out.push_str("disjunction:");
        for (i, d) in self.disjuncts.iter().enumerate() {
            out.push_str(&format!("\n  [{i}] {}", d.elaborated_query_text()));
        }
        out
    }
}

pub(crate) fn prepare_axql_query_with_meta(
    db: &axiograph_pathdb::PathDB,
    query: &AxqlQuery,
    meta: Option<&MetaPlaneIndex>,
) -> Result<PreparedAxqlQueryExpr> {
    TypecheckedAxqlQueryExpr::from_parsed(db, query, meta)?.prepare(
        db,
        meta,
        query.select_vars.clone(),
        query.limit,
    )
}

/// Execute an AxQL query with an optional precomputed meta-plane index.
///
/// This is primarily used by the REPL to avoid rebuilding the `.axi` meta-plane
/// index on every query when repeatedly querying the same snapshot.
pub fn execute_axql_query_with_meta(
    db: &axiograph_pathdb::PathDB,
    query: &AxqlQuery,
    meta: Option<&MetaPlaneIndex>,
) -> Result<AxqlResult> {
    let mut prepared = prepare_axql_query_with_meta(db, query, meta)?;
    prepared.execute(db, meta)
}

/// Emit a `query_result_v1` certificate with an optional precomputed meta-plane index.
pub fn certify_axql_query_with_meta(
    db: &axiograph_pathdb::PathDB,
    query: &AxqlQuery,
    meta: Option<&MetaPlaneIndex>,
) -> Result<CertificateV2> {
    if query.contexts.len() > 1 {
        return Err(anyhow!(
            "cannot certify multi-context scoping yet; use a single `in <context>`"
        ));
    }
    TypecheckedAxqlQueryExpr::from_parsed(db, query, meta)?.certify(db, meta, query.limit)
}

/// Emit a `query_result_v3` certificate (name-based, `.axi`-anchored) with an
/// optional precomputed meta-plane index.
///
/// This format is intended to make certificates verifiable against canonical
/// `.axi` inputs without requiring a `PathDBExportV1` snapshot export as an
/// anchor.
pub fn certify_axql_query_v3_with_meta(
    db: &axiograph_pathdb::PathDB,
    query: &AxqlQuery,
    meta: Option<&MetaPlaneIndex>,
    axi_digest_v1: &str,
) -> Result<CertificateV2> {
    if query.contexts.len() > 1 {
        return Err(anyhow!(
            "cannot certify multi-context scoping yet; use a single `in <context>`"
        ));
    }

    let expr = TypecheckedAxqlQueryExpr::from_parsed(db, query, meta)?;
    match expr {
        TypecheckedAxqlQueryExpr::Conjunction(q) => q.certify_v3(db, meta, axi_digest_v1),
        TypecheckedAxqlQueryExpr::Disjunction(disjuncts) => {
            certify_disjunctive_query_v3(db, disjuncts, meta, query.limit, axi_digest_v1)
        }
    }
}

fn certify_disjunctive_query(
    db: &axiograph_pathdb::PathDB,
    disjuncts: Vec<TypecheckedLoweredQuery>,
    meta: Option<&MetaPlaneIndex>,
    limit: usize,
) -> Result<CertificateV2> {
    if disjuncts.is_empty() {
        return Err(anyhow!("AxQL query must have at least one disjunct"));
    }

    // For a UCQ (union of conjunctive queries), the natural “implicit select”
    // semantics is: return only variables common to all branches. When the
    // user explicitly selects vars, the per-branch lowerings will already have
    // enforced that the vars exist in every disjunct.
    let mut select_vars = disjuncts
        .first()
        .map(|d| d.lowered.select_vars.clone())
        .unwrap_or_default();
    for d in &disjuncts[1..] {
        let set: HashSet<&str> = d.lowered.select_vars.iter().map(|s| s.as_str()).collect();
        select_vars.retain(|v| set.contains(v.as_str()));
    }

    // Query IR: we record all disjuncts’ atoms, even if we truncate before
    // producing rows from later branches.
    let mut query_disjuncts: Vec<Vec<QueryAtomV1>> = Vec::with_capacity(disjuncts.len());
    let mut max_hops: Option<u32> = None;
    let mut min_confidence_fp: Option<FixedPointProbability> = None;
    for (i, d) in disjuncts.iter().enumerate() {
        let q1 = d.lowered.to_query_ir(db)?;
        if i == 0 {
            max_hops = q1.max_hops;
            min_confidence_fp = q1.min_confidence_fp;
        } else {
            if q1.max_hops != max_hops {
                return Err(anyhow!(
                    "internal error: disjunct max_hops mismatch (expected {max_hops:?}, got {:?})",
                    q1.max_hops
                ));
            }
            if q1.min_confidence_fp != min_confidence_fp {
                return Err(anyhow!(
                    "internal error: disjunct min_confidence mismatch (expected {min_confidence_fp:?}, got {:?})",
                    q1.min_confidence_fp
                ));
            }
        }
        query_disjuncts.push(q1.atoms);
    }

    let query = QueryV2 {
        select_vars,
        disjuncts: query_disjuncts,
        max_hops,
        min_confidence_fp,
    };

    // Rows: prove each returned row satisfies *some* branch. We do not claim
    // completeness; the only global control is the output limit.
    let mut rows: Vec<QueryRowV2> = Vec::new();
    let mut truncated = rows.len() >= limit;

    for (i, mut d) in disjuncts.into_iter().enumerate() {
        if rows.len() >= limit {
            truncated = true;
            break;
        }
        let remaining = limit - rows.len();
        d.lowered.limit = remaining;

        let cert = d.lowered.certify(db, meta)?;
        let proof = match cert.payload {
            CertificatePayloadV2::QueryResultV1 { proof } => proof,
            other => {
                return Err(anyhow!(
                    "internal error: expected query_result_v1 from conjunctive certifier, got {other:?}"
                ))
            }
        };

        for row in proof.rows {
            if rows.len() >= limit {
                truncated = true;
                break;
            }
            rows.push(QueryRowV2 {
                disjunct: u32::try_from(i).unwrap_or(u32::MAX),
                bindings: row.bindings,
                witnesses: row.witnesses,
            });
        }
    }

    Ok(CertificateV2::query_result_v2(QueryResultProofV2 {
        query,
        rows,
        truncated,
    }))
}

fn certify_disjunctive_query_v3(
    db: &axiograph_pathdb::PathDB,
    disjuncts: Vec<TypecheckedLoweredQuery>,
    meta: Option<&MetaPlaneIndex>,
    limit: usize,
    axi_digest_v1: &str,
) -> Result<CertificateV2> {
    if disjuncts.is_empty() {
        return Err(anyhow!("AxQL query must have at least one disjunct"));
    }

    // For a UCQ (union of conjunctive queries), the natural “implicit select”
    // semantics is: return only variables common to all branches.
    let mut select_vars = disjuncts
        .first()
        .map(|d| d.lowered.select_vars.clone())
        .unwrap_or_default();
    for d in &disjuncts[1..] {
        let set: HashSet<&str> = d.lowered.select_vars.iter().map(|s| s.as_str()).collect();
        select_vars.retain(|v| set.contains(v.as_str()));
    }

    let mut query_disjuncts: Vec<Vec<QueryAtomV3>> = Vec::with_capacity(disjuncts.len());
    let mut max_hops: Option<u32> = None;
    let mut min_confidence_fp: Option<FixedPointProbability> = None;
    for (i, d) in disjuncts.iter().enumerate() {
        let q = d.lowered.to_query_ir_v3_disjunct(db)?;
        if i == 0 {
            max_hops = d.lowered.max_hops;
            min_confidence_fp = d
                .lowered
                .min_confidence
                .map(fixed_prob_from_confidence);
        } else {
            if d.lowered.max_hops != max_hops {
                return Err(anyhow!(
                    "internal error: disjunct max_hops mismatch (expected {max_hops:?}, got {:?})",
                    d.lowered.max_hops
                ));
            }
            if d.lowered.min_confidence.map(fixed_prob_from_confidence) != min_confidence_fp {
                return Err(anyhow!(
                    "internal error: disjunct min_confidence mismatch (expected {min_confidence_fp:?}, got {:?})",
                    d.lowered.min_confidence.map(fixed_prob_from_confidence)
                ));
            }
        }
        query_disjuncts.push(q);
    }

    let query = QueryV3 {
        select_vars,
        disjuncts: query_disjuncts,
        max_hops,
        min_confidence_fp,
    };

    let mut rows: Vec<QueryRowV3> = Vec::new();
    let mut truncated = rows.len() >= limit;
    let mut elaboration_rewrites: Vec<RewriteDerivationProofV3> = Vec::new();

    for (i, mut d) in disjuncts.into_iter().enumerate() {
        if rows.len() >= limit {
            truncated = true;
            break;
        }

        let remaining = limit - rows.len();
        d.lowered.limit = remaining;

        let cert = d.lowered.certify_v3(db, meta, axi_digest_v1, &d.elaboration)?;
        let proof = match cert.payload {
            CertificatePayloadV2::QueryResultV3 { proof } => proof,
            other => {
                return Err(anyhow!(
                    "internal error: expected query_result_v3 from conjunctive certifier, got {other:?}"
                ))
            }
        };

        elaboration_rewrites.extend(proof.elaboration_rewrites.iter().cloned());
        for row in proof.rows {
            if rows.len() >= limit {
                truncated = true;
                break;
            }
            rows.push(QueryRowV3 {
                disjunct: u32::try_from(i).unwrap_or(u32::MAX),
                bindings: row.bindings,
                witnesses: row.witnesses,
            });
        }
    }

    Ok(CertificateV2::query_result_v3(QueryResultProofV3 {
        query,
        rows,
        truncated,
        elaboration_rewrites,
    }))
}

// =============================================================================
// Parsing
// =============================================================================

fn axql_query(input: &str) -> IResult<&str, AxqlQuery> {
    // Either:
    //   select ?x ?y where <atoms> [limit N]
    // or:
    //   where <atoms> [limit N]
    // (implicit select *)

    let (input, explicit_select) =
        opt(preceded(ws(tag_no_case("select")), ws(select_list)))(input)?;
    let select_vars = explicit_select.unwrap_or_default();

    let (input, _) = ws(tag_no_case("where"))(input)?;
    let (input, disjuncts) = separated_list1(ws(tag_no_case("or")), ws(atom_list))(input)?;
    let (input, opts) = many0(ws(query_option))(input)?;

    let mut limit: Option<usize> = None;
    let mut max_hops: Option<u32> = None;
    let mut min_confidence: Option<f32> = None;
    let mut contexts: Vec<AxqlContextSpec> = Vec::new();
    for opt in opts {
        match opt {
            QueryOption::Limit(n) => limit = Some(n as usize),
            QueryOption::MaxHops(n) => max_hops = Some(n),
            QueryOption::MinConf(c) => {
                min_confidence = Some(match min_confidence {
                    None => c,
                    Some(prev) => prev.max(c),
                })
            }
            QueryOption::InContexts(mut cs) => contexts.append(&mut cs),
        }
    }

    let limit = limit.unwrap_or(20);

    Ok((
        input,
        AxqlQuery {
            select_vars,
            disjuncts,
            limit,
            contexts,
            max_hops,
            min_confidence,
        },
    ))
}

fn select_list(input: &str) -> IResult<&str, Vec<String>> {
    // `select *` means “implicit select” (all non-anonymous vars).
    alt((map(pchar('*'), |_| Vec::new()), var_list))(input)
}

#[derive(Debug, Clone, PartialEq)]
enum QueryOption {
    Limit(u64),
    MaxHops(u32),
    MinConf(f32),
    InContexts(Vec<AxqlContextSpec>),
}

fn query_option(input: &str) -> IResult<&str, QueryOption> {
    alt((
        map(
            preceded(ws(tag_no_case("limit")), ws(u64_number)),
            QueryOption::Limit,
        ),
        map(
            preceded(ws(tag_no_case("max_hops")), ws(u32_number)),
            QueryOption::MaxHops,
        ),
        map(
            preceded(
                ws(alt((
                    tag_no_case("min_confidence"),
                    tag_no_case("min_conf"),
                ))),
                ws(f32_number),
            ),
            QueryOption::MinConf,
        ),
        map(
            preceded(ws(tag_no_case("in")), ws(context_spec_set)),
            QueryOption::InContexts,
        ),
    ))(input)
}

fn context_spec(input: &str) -> IResult<&str, AxqlContextSpec> {
    alt((
        map(u32_number, AxqlContextSpec::EntityId),
        map(string_lit_or_ident, AxqlContextSpec::Name),
    ))(input)
}

fn context_spec_set(input: &str) -> IResult<&str, Vec<AxqlContextSpec>> {
    alt((
        map(
            delimited(
                ws(pchar('{')),
                ws(separated_list1(ws(pchar(',')), ws(context_spec))),
                ws(pchar('}')),
            ),
            |items| items,
        ),
        map(context_spec, |c| vec![c]),
    ))(input)
}

fn atom_list(input: &str) -> IResult<&str, Vec<AxqlAtom>> {
    separated_list1(ws(pchar(',')), ws(axql_atom))(input)
}

fn axql_atom(input: &str) -> IResult<&str, AxqlAtom> {
    alt((
        shape_literal_atom,
        has_atom,
        attrs_atom,
        has_infix_atom,
        attr_prop_atom,
        contains_atom,
        fts_atom,
        fuzzy_atom,
        attr_atom,
        type_atom,
        type_atom_infix,
        fact_atom,
        edge_atom,
    ))(input)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ShapeItem {
    Type(String),
    Rel(String),
    Attr(String, String),
}

fn shape_literal_atom(input: &str) -> IResult<&str, AxqlAtom> {
    // ?x { rel_0, rel_1, name="node_42", is Node }
    map(
        tuple((
            ws(axql_term),
            ws(pchar('{')),
            ws(separated_list1(ws(pchar(',')), ws(shape_item))),
            ws(pchar('}')),
        )),
        |(term, _, items, _)| {
            let mut type_name: Option<String> = None;
            let mut rels: Vec<String> = Vec::new();
            let mut attrs: Vec<(String, String)> = Vec::new();

            for item in items {
                match item {
                    ShapeItem::Type(t) => type_name = Some(t),
                    ShapeItem::Rel(r) => rels.push(r),
                    ShapeItem::Attr(k, v) => attrs.push((k, v)),
                }
            }

            AxqlAtom::Shape {
                term,
                type_name,
                rels,
                attrs,
            }
        },
    )(input)
}

fn shape_item(input: &str) -> IResult<&str, ShapeItem> {
    alt((
        // : Node
        map(preceded(ws(pchar(':')), ws(type_name)), ShapeItem::Type),
        // is Node
        map(
            preceded(ws(tag_no_case("is")), ws(type_name)),
            ShapeItem::Type,
        ),
        // type Node
        map(
            preceded(ws(tag_no_case("type")), ws(type_name)),
            ShapeItem::Type,
        ),
        // has rel_0
        map(
            preceded(ws(tag_no_case("has")), ws(string_lit_or_ident)),
            ShapeItem::Rel,
        ),
        // name="node_42"
        map(ws(attr_pair), |(k, v)| ShapeItem::Attr(k, v)),
        // rel_0
        map(ws(string_lit_or_ident), ShapeItem::Rel),
    ))(input)
}

fn type_atom(input: &str) -> IResult<&str, AxqlAtom> {
    map(
        tuple((ws(axql_term), ws(pchar(':')), ws(type_name))),
        |(term, _, type_name)| AxqlAtom::Type { term, type_name },
    )(input)
}

fn type_atom_infix(input: &str) -> IResult<&str, AxqlAtom> {
    // ?x is TypeName
    map(
        tuple((ws(axql_term), ws(tag_no_case("is")), ws(type_name))),
        |(term, _, type_name)| AxqlAtom::Type { term, type_name },
    )(input)
}

fn edge_atom(input: &str) -> IResult<&str, AxqlAtom> {
    map(
        tuple((
            ws(axql_term),
            ws(pchar('-')),
            ws(bracketed_or_plain_path_expr),
            ws(tag("->")),
            ws(axql_term),
        )),
        |(left, _, path, _, right)| AxqlAtom::Edge { left, path, right },
    )(input)
}

fn bracketed_or_plain_path_expr(input: &str) -> IResult<&str, AxqlPathExpr> {
    alt((
        delimited(ws(pchar('[')), ws(path_expr), ws(pchar(']'))),
        path_expr,
    ))(input)
}

fn path_expr(input: &str) -> IResult<&str, AxqlPathExpr> {
    map(rpq_alt, |regex| AxqlPathExpr { regex })(input)
}

fn rpq_alt(input: &str) -> IResult<&str, AxqlRegex> {
    map(separated_list1(ws(pchar('|')), rpq_seq), mk_alt)(input)
}

fn rpq_seq(input: &str) -> IResult<&str, AxqlRegex> {
    map(separated_list1(ws(pchar('/')), rpq_rep), mk_seq)(input)
}

fn rpq_rep(input: &str) -> IResult<&str, AxqlRegex> {
    map(
        tuple((
            ws(rpq_atom),
            opt(ws(alt((pchar('*'), pchar('+'), pchar('?'))))),
        )),
        |(atom, op)| match op {
            Some('*') => AxqlRegex::Star(Box::new(atom)),
            Some('+') => AxqlRegex::Plus(Box::new(atom)),
            Some('?') => AxqlRegex::Opt(Box::new(atom)),
            _ => atom,
        },
    )(input)
}

fn rpq_atom(input: &str) -> IResult<&str, AxqlRegex> {
    alt((
        map(identifier_with_dots, AxqlRegex::Rel),
        delimited(ws(pchar('(')), rpq_alt, ws(pchar(')'))),
    ))(input)
}

fn mk_seq(parts: Vec<AxqlRegex>) -> AxqlRegex {
    let mut out: Vec<AxqlRegex> = Vec::new();
    for p in parts {
        match p {
            AxqlRegex::Epsilon => {}
            AxqlRegex::Seq(inner) => out.extend(inner),
            other => out.push(other),
        }
    }
    match out.len() {
        0 => AxqlRegex::Epsilon,
        1 => out.remove(0),
        _ => AxqlRegex::Seq(out),
    }
}

fn mk_alt(parts: Vec<AxqlRegex>) -> AxqlRegex {
    let mut out: Vec<AxqlRegex> = Vec::new();
    for p in parts {
        match p {
            AxqlRegex::Alt(inner) => out.extend(inner),
            other => out.push(other),
        }
    }
    match out.len() {
        0 => AxqlRegex::Epsilon,
        1 => out.remove(0),
        _ => AxqlRegex::Alt(out),
    }
}

fn attr_atom(input: &str) -> IResult<&str, AxqlAtom> {
    // attr(?x, "key", "value")
    map(
        tuple((
            ws(tag_no_case("attr")),
            ws(pchar('(')),
            ws(axql_term),
            ws(pchar(',')),
            ws(string_lit_or_ident),
            ws(pchar(',')),
            ws(string_lit_or_ident),
            ws(pchar(')')),
        )),
        |(_, _, term, _, key, _, value, _)| AxqlAtom::AttrEq { term, key, value },
    )(input)
}

fn contains_atom(input: &str) -> IResult<&str, AxqlAtom> {
    // contains(?x, "key", "needle")
    map(
        tuple((
            ws(tag_no_case("contains")),
            ws(pchar('(')),
            ws(axql_term),
            ws(pchar(',')),
            ws(string_lit_or_ident),
            ws(pchar(',')),
            ws(string_lit_or_ident),
            ws(pchar(')')),
        )),
        |(_, _, term, _, key, _, needle, _)| AxqlAtom::AttrContains { term, key, needle },
    )(input)
}

fn fts_atom(input: &str) -> IResult<&str, AxqlAtom> {
    // fts(?x, "key", "query")
    map(
        tuple((
            ws(tag_no_case("fts")),
            ws(pchar('(')),
            ws(axql_term),
            ws(pchar(',')),
            ws(string_lit_or_ident),
            ws(pchar(',')),
            ws(string_lit_or_ident),
            ws(pchar(')')),
        )),
        |(_, _, term, _, key, _, query, _)| AxqlAtom::AttrFts { term, key, query },
    )(input)
}

fn fuzzy_atom(input: &str) -> IResult<&str, AxqlAtom> {
    // fuzzy(?x, "key", "needle", 2)
    map(
        tuple((
            ws(tag_no_case("fuzzy")),
            ws(pchar('(')),
            ws(axql_term),
            ws(pchar(',')),
            ws(string_lit_or_ident),
            ws(pchar(',')),
            ws(string_lit_or_ident),
            ws(pchar(',')),
            ws(u64_number),
            ws(pchar(')')),
        )),
        |(_, _, term, _, key, _, needle, _, max_dist, _)| AxqlAtom::AttrFuzzy {
            term,
            key,
            needle,
            max_dist: max_dist as usize,
        },
    )(input)
}

fn attr_prop_atom(input: &str) -> IResult<&str, AxqlAtom> {
    // ?x.key = "value"
    map(
        tuple((
            ws(axql_term),
            ws(pchar('.')),
            ws(string_lit_or_ident),
            ws(pchar('=')),
            ws(string_lit_or_ident),
        )),
        |(term, _, key, _, value)| AxqlAtom::AttrEq { term, key, value },
    )(input)
}

fn has_atom(input: &str) -> IResult<&str, AxqlAtom> {
    // has(?x, rel_0, rel_1, ...)
    let (input, _) = ws(tag_no_case("has"))(input)?;
    let (input, _) = ws(pchar('('))(input)?;
    let (input, term) = ws(axql_term)(input)?;
    let (input, _) = ws(pchar(','))(input)?;
    let (input, rels) = separated_list1(ws(pchar(',')), ws(string_lit_or_ident))(input)?;
    let (input, _) = ws(pchar(')'))(input)?;
    Ok((input, AxqlAtom::HasOut { term, rels }))
}

fn has_infix_atom(input: &str) -> IResult<&str, AxqlAtom> {
    // ?x has rel_0
    map(
        tuple((
            ws(axql_term),
            ws(tag_no_case("has")),
            ws(string_lit_or_ident),
        )),
        |(term, _, rel)| AxqlAtom::HasOut {
            term,
            rels: vec![rel],
        },
    )(input)
}

fn attrs_atom(input: &str) -> IResult<&str, AxqlAtom> {
    // attrs(?x, key="value", ...)
    let (input, _) = ws(tag_no_case("attrs"))(input)?;
    let (input, _) = ws(pchar('('))(input)?;
    let (input, term) = ws(axql_term)(input)?;
    let (input, _) = ws(pchar(','))(input)?;
    let (input, pairs) = separated_list1(ws(pchar(',')), ws(attr_pair))(input)?;
    let (input, _) = ws(pchar(')'))(input)?;
    Ok((input, AxqlAtom::Attrs { term, pairs }))
}

fn attr_pair(input: &str) -> IResult<&str, (String, String)> {
    map(
        tuple((
            ws(string_lit_or_ident),
            ws(pchar('=')),
            ws(string_lit_or_ident),
        )),
        |(k, _, v)| (k, v),
    )(input)
}

fn var_list(input: &str) -> IResult<&str, Vec<String>> {
    separated_list1(multispace1, variable)(input)
}

fn axql_term(input: &str) -> IResult<&str, AxqlTerm> {
    alt((
        lookup_term,
        map(variable, AxqlTerm::Var),
        map(u32_number, AxqlTerm::Const),
        map(pchar('_'), |_| AxqlTerm::Wildcard),
        // Bare identifiers are treated as `name("...")` for convenience when
        // querying canonical `.axi` corpora (where object elements are named).
        map(identifier_with_dots, |value| AxqlTerm::Lookup {
            key: "name".to_string(),
            value,
        }),
    ))(input)
}

fn fact_atom(input: &str) -> IResult<&str, AxqlAtom> {
    // `Flow(from=?a, to=?b)`
    // `?f = Flow(from=?a, to=?b)`

    // Binder term: only variables or `_` (wildcard), to avoid confusing this
    // with attribute equality (`?x.name = "..."`) or numeric constants.
    let binder_term = alt((
        map(variable, AxqlTerm::Var),
        map(pchar('_'), |_| AxqlTerm::Wildcard),
    ));

    // With binder
    let with_binder = map(
        tuple((
            ws(binder_term),
            ws(pchar('=')),
            ws(identifier_with_dots),
            ws(pchar('(')),
            ws(opt(separated_list1(ws(pchar(',')), ws(fact_field_binding)))),
            ws(pchar(')')),
        )),
        |(fact, _, relation, _, fields, _)| AxqlAtom::Fact {
            fact: match fact {
                AxqlTerm::Wildcard => None,
                other => Some(other),
            },
            relation,
            fields: fields.unwrap_or_default(),
        },
    );

    // Without binder
    let without_binder = map(
        tuple((
            ws(identifier_with_dots),
            ws(pchar('(')),
            ws(opt(separated_list1(ws(pchar(',')), ws(fact_field_binding)))),
            ws(pchar(')')),
        )),
        |(relation, _, fields, _)| AxqlAtom::Fact {
            fact: None,
            relation,
            fields: fields.unwrap_or_default(),
        },
    );

    alt((with_binder, without_binder))(input)
}

fn fact_field_binding(input: &str) -> IResult<&str, (String, AxqlTerm)> {
    map(
        tuple((ws(identifier_with_dots), ws(pchar('=')), ws(axql_term))),
        |(field, _, term)| (field, term),
    )(input)
}

fn lookup_term(input: &str) -> IResult<&str, AxqlTerm> {
    alt((name_term, entity_term))(input)
}

fn name_term(input: &str) -> IResult<&str, AxqlTerm> {
    // name("node_42")  ≡  Lookup { key = "name", value = "node_42" }
    map(
        tuple((
            ws(tag_no_case("name")),
            ws(pchar('(')),
            ws(string_lit_or_ident),
            ws(pchar(')')),
        )),
        |(_, _, value, _)| AxqlTerm::Lookup {
            key: "name".to_string(),
            value,
        },
    )(input)
}

fn entity_term(input: &str) -> IResult<&str, AxqlTerm> {
    // entity("key", "value")  ≡  Lookup { key, value }
    map(
        tuple((
            ws(tag_no_case("entity")),
            ws(pchar('(')),
            ws(string_lit_or_ident),
            ws(pchar(',')),
            ws(string_lit_or_ident),
            ws(pchar(')')),
        )),
        |(_, _, key, _, value, _)| AxqlTerm::Lookup { key, value },
    )(input)
}

fn variable(input: &str) -> IResult<&str, String> {
    map(preceded(pchar('?'), identifier), |s| format!("?{s}"))(input)
}

fn type_name(input: &str) -> IResult<&str, String> {
    // Allow dotted names (e.g. proto packages/types) and underscores.
    identifier_with_dots(input)
}

fn identifier(input: &str) -> IResult<&str, String> {
    map(
        recognize(tuple((
            take_while1(is_ident_start),
            take_while(is_ident_continue),
        ))),
        |s: &str| s.to_string(),
    )(input)
}

fn identifier_with_dots(input: &str) -> IResult<&str, String> {
    map(
        recognize(tuple((
            take_while1(is_ident_start),
            take_while(is_ident_continue_or_dot),
        ))),
        |s: &str| s.to_string(),
    )(input)
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_ident_continue_or_dot(c: char) -> bool {
    is_ident_continue(c) || c == '.'
}

fn u32_number(input: &str) -> IResult<&str, u32> {
    map_res(digit1, |s: &str| s.parse::<u32>())(input)
}

fn u64_number(input: &str) -> IResult<&str, u64> {
    map_res(digit1, |s: &str| s.parse::<u64>())(input)
}

fn f32_number(input: &str) -> IResult<&str, f32> {
    map_res(recognize_float, |s: &str| s.parse::<f32>())(input)
}

fn string_lit_or_ident(input: &str) -> IResult<&str, String> {
    alt((string_lit, single_string_lit, identifier_with_dots))(input)
}

fn string_lit(input: &str) -> IResult<&str, String> {
    let esc = escaped_transform(
        is_not("\\\""),
        '\\',
        alt((
            map(tag("\\"), |_| "\\"),
            map(tag("\""), |_| "\""),
            map(tag("n"), |_| "\n"),
            map(tag("t"), |_| "\t"),
            map(tag("r"), |_| "\r"),
        )),
    );
    delimited(pchar('"'), esc, pchar('"'))(input)
}

fn single_string_lit(input: &str) -> IResult<&str, String> {
    let esc = escaped_transform(
        is_not("\\'"),
        '\\',
        alt((
            map(tag("\\"), |_| "\\"),
            map(tag("'"), |_| "'"),
            map(tag("n"), |_| "\n"),
            map(tag("t"), |_| "\t"),
            map(tag("r"), |_| "\r"),
        )),
    );
    delimited(pchar('\''), esc, pchar('\''))(input)
}

fn ws<'a, F, O>(inner: F) -> impl FnMut(&'a str) -> IResult<&'a str, O>
where
    F: FnMut(&'a str) -> IResult<&'a str, O>,
{
    delimited(multispace0, inner, multispace0)
}

fn tag_no_case<'a>(s: &'static str) -> impl FnMut(&'a str) -> IResult<&'a str, &'a str> {
    // Minimal ASCII case-insensitive matcher for keywords.
    move |input: &'a str| {
        let mut i = 0usize;
        for expected in s.chars() {
            let Some(got) = input.chars().nth(i) else {
                return Err(nom::Err::Error(nom::error::Error::new(
                    input,
                    nom::error::ErrorKind::Tag,
                )));
            };
            if got.to_ascii_lowercase() != expected.to_ascii_lowercase() {
                return Err(nom::Err::Error(nom::error::Error::new(
                    input,
                    nom::error::ErrorKind::Tag,
                )));
            }
            i += got.len_utf8();
        }
        Ok((&input[i..], &input[..i]))
    }
}

// =============================================================================
// Lowering + evaluation (conjunctive query)
// =============================================================================

#[derive(Debug, Clone)]
struct LoweredQuery {
    select_vars: Vec<String>,
    vars: Vec<String>,
    var_index: HashMap<String, usize>,
    atoms: Vec<LoweredAtom>,
    limit: usize,
    context_scope: Vec<AxqlContextSpec>,
    max_hops: Option<u32>,
    min_confidence: Option<f32>,
    rpqs: Vec<AxqlRegex>,
    /// Field names that were specified via `Rel(field=..., ...)` fact-atom syntax,
    /// keyed by the fact-node variable id.
    ///
    /// We use this to provide better error messages:
    /// `Flow(foo=a)` should fail fast with “field foo not in relation Flow”,
    /// rather than silently producing empty results.
    fact_field_intent: HashMap<usize, HashSet<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum LoweredTerm {
    Var(usize),
    Const(u32),
}

#[derive(Debug, Clone)]
enum LoweredAtom {
    Type {
        term: LoweredTerm,
        type_name: String,
    },
    Edge {
        left: LoweredTerm,
        rel: String,
        right: LoweredTerm,
    },
    Rpq {
        left: LoweredTerm,
        rpq_id: usize,
        right: LoweredTerm,
    },
    AttrEq {
        term: LoweredTerm,
        key: String,
        value: String,
    },
    AttrContains {
        term: LoweredTerm,
        key: String,
        needle: String,
    },
    AttrFts {
        term: LoweredTerm,
        key: String,
        query: String,
    },
    AttrFuzzy {
        term: LoweredTerm,
        key: String,
        needle: String,
        max_dist: usize,
    },
}

#[derive(Debug, Clone)]
struct QueryPlan {
    candidates: Vec<RoaringBitmap>,
    order: Vec<usize>,
    atom_order: Vec<usize>,
}

fn lower_query_disjunct(query: &AxqlQuery, disjunct: &[AxqlAtom]) -> Result<LoweredQuery> {
    let mut vars: Vec<String> = Vec::new();
    let mut var_index: HashMap<String, usize> = HashMap::new();

    let mut fresh_anon = 0usize;
    let mut expanded: Vec<AxqlAtom> = Vec::new();
    let mut fact_field_intent_by_var: HashMap<String, HashSet<String>> = HashMap::new();

    // Expand surface-level macros (`has`, `attrs`, fact atoms) first.
    for atom in disjunct.iter().cloned() {
        match atom {
            AxqlAtom::Fact {
                fact,
                relation,
                fields,
            } => {
                let fact_term = match fact {
                    Some(AxqlTerm::Var(v)) => AxqlTerm::Var(v),
                    Some(other) => {
                        return Err(anyhow!("fact binder must be a variable (got {other:?})"))
                    }
                    None => {
                        let name = format!("?_fact{}", fresh_anon);
                        fresh_anon += 1;
                        AxqlTerm::Var(name)
                    }
                };

                let fact_var_name = match &fact_term {
                    AxqlTerm::Var(v) => v.clone(),
                    _ => unreachable!("fact term is always a variable"),
                };
                let field_set = fact_field_intent_by_var.entry(fact_var_name).or_default();

                expanded.push(AxqlAtom::AttrEq {
                    term: fact_term.clone(),
                    key: axiograph_pathdb::axi_meta::ATTR_AXI_RELATION.to_string(),
                    value: relation,
                });

                for (field, value) in fields {
                    field_set.insert(field.clone());
                    expanded.push(AxqlAtom::Edge {
                        left: fact_term.clone(),
                        path: AxqlPathExpr::rel(field),
                        right: value,
                    });
                }
            }
            AxqlAtom::HasOut { term, rels } => {
                for rel in rels {
                    expanded.push(AxqlAtom::Edge {
                        left: term.clone(),
                        path: AxqlPathExpr::rel(rel),
                        right: AxqlTerm::Wildcard,
                    });
                }
            }
            AxqlAtom::Attrs { term, pairs } => {
                for (key, value) in pairs {
                    expanded.push(AxqlAtom::AttrEq {
                        term: term.clone(),
                        key,
                        value,
                    });
                }
            }
            AxqlAtom::Shape {
                term,
                type_name,
                rels,
                attrs,
            } => {
                if let Some(type_name) = type_name {
                    expanded.push(AxqlAtom::Type {
                        term: term.clone(),
                        type_name,
                    });
                }
                for rel in rels {
                    expanded.push(AxqlAtom::Edge {
                        left: term.clone(),
                        path: AxqlPathExpr::rel(rel),
                        right: AxqlTerm::Wildcard,
                    });
                }
                for (key, value) in attrs {
                    expanded.push(AxqlAtom::AttrEq {
                        term: term.clone(),
                        key,
                        value,
                    });
                }
            }
            other => expanded.push(other),
        }
    }

    // Optional context scoping (single context is lowered into core atoms so it
    // remains certificate-checkable).
    //
    // For multiple contexts, we currently treat scoping as an execution-time
    // filter during planning (union of contexts). While AxQL now supports UCQ
    // (disjunction) in its certified core, lowering multi-context scoping would
    // require distributing the context disjunction across *each* fact-node
    // binder, which can be a large blow-up. We'll add a certified lowering once
    // we have a good story for that expansion (or a richer boolean core IR).
    if query.contexts.len() == 1 {
        let ctx_term = match &query.contexts[0] {
            AxqlContextSpec::EntityId(id) => AxqlTerm::Const(*id),
            AxqlContextSpec::Name(name) => AxqlTerm::Lookup {
                key: "name".to_string(),
                value: name.clone(),
            },
        };

        for fact_var in fact_field_intent_by_var.keys() {
            expanded.push(AxqlAtom::Edge {
                left: AxqlTerm::Var(fact_var.clone()),
                path: AxqlPathExpr::rel(REL_AXI_FACT_IN_CONTEXT.to_string()),
                right: ctx_term.clone(),
            });
        }
    }

    let mut lookup_vars: HashMap<(String, String), String> = HashMap::new();
    let mut atoms: Vec<AxqlAtom> = Vec::new();

    // Normalize:
    // - wildcards (`_`) into fresh anonymous variables,
    // - lookup terms (`name(...)`, `entity(...)`) into fresh variables + `attr(...)`,
    // - and keep simple path chains intact so they can be planned as RPQs.
    for atom in expanded {
        let mut extra_atoms: Vec<AxqlAtom> = Vec::new();
        match atom {
            AxqlAtom::Fact { .. } => {
                return Err(anyhow!(
                    "internal error: fact atoms must be expanded before lowering"
                ));
            }
            AxqlAtom::Edge { left, path, right } => {
                let left =
                    normalize_term(left, &mut fresh_anon, &mut lookup_vars, &mut extra_atoms);
                let right =
                    normalize_term(right, &mut fresh_anon, &mut lookup_vars, &mut extra_atoms);

                atoms.extend(extra_atoms);

                if let Some(rels) = simple_chain(&path.regex) {
                    if rels.len() <= 1 {
                        let rel = rels.into_iter().next().unwrap_or_default();
                        atoms.push(AxqlAtom::Edge {
                            left,
                            path: AxqlPathExpr::rel(rel),
                            right,
                        });
                        continue;
                    }
                }

                atoms.push(AxqlAtom::Edge { left, path, right });
            }
            AxqlAtom::Type { term, type_name } => {
                let term =
                    normalize_term(term, &mut fresh_anon, &mut lookup_vars, &mut extra_atoms);
                atoms.extend(extra_atoms);
                atoms.push(AxqlAtom::Type { term, type_name });
            }
            AxqlAtom::AttrEq { term, key, value } => {
                let term =
                    normalize_term(term, &mut fresh_anon, &mut lookup_vars, &mut extra_atoms);
                atoms.extend(extra_atoms);
                atoms.push(AxqlAtom::AttrEq { term, key, value });
            }
            AxqlAtom::AttrContains { term, key, needle } => {
                let term =
                    normalize_term(term, &mut fresh_anon, &mut lookup_vars, &mut extra_atoms);
                atoms.extend(extra_atoms);
                atoms.push(AxqlAtom::AttrContains { term, key, needle });
            }
            AxqlAtom::AttrFts { term, key, query } => {
                let term =
                    normalize_term(term, &mut fresh_anon, &mut lookup_vars, &mut extra_atoms);
                atoms.extend(extra_atoms);
                atoms.push(AxqlAtom::AttrFts { term, key, query });
            }
            AxqlAtom::AttrFuzzy {
                term,
                key,
                needle,
                max_dist,
            } => {
                let term =
                    normalize_term(term, &mut fresh_anon, &mut lookup_vars, &mut extra_atoms);
                atoms.extend(extra_atoms);
                atoms.push(AxqlAtom::AttrFuzzy {
                    term,
                    key,
                    needle,
                    max_dist,
                });
            }
            AxqlAtom::HasOut { .. } | AxqlAtom::Attrs { .. } => {
                return Err(anyhow!(
                    "internal error: shape macros must be expanded before lowering"
                ));
            }
            AxqlAtom::Shape { .. } => {
                return Err(anyhow!(
                    "internal error: shape literals must be expanded before lowering"
                ));
            }
        }
    }

    // Register variables.
    for atom in &atoms {
        for term in atom_terms(atom) {
            if let AxqlTerm::Var(v) = term {
                ensure_var(&mut vars, &mut var_index, &v);
            }
        }
    }

    // Selection defaults: if none provided, select all non-anonymous vars.
    let mut select_vars = if query.select_vars.is_empty() {
        vars.iter()
            .filter(|v| !is_internal_var(v))
            .cloned()
            .collect::<Vec<_>>()
    } else {
        query.select_vars.clone()
    };

    // Ensure select variables exist.
    for v in &select_vars {
        if !var_index.contains_key(v) {
            return Err(anyhow!(
                "select variable `{v}` does not appear in where-clause"
            ));
        }
    }

    let mut rpqs: Vec<AxqlRegex> = Vec::new();
    let mut rpq_index: HashMap<AxqlRegex, usize> = HashMap::new();

    // Lower terms and choose between single-edge constraints vs RPQs.
    let lowered_atoms = atoms
        .into_iter()
        .map(|a| match a {
            AxqlAtom::Fact { .. } => Err(anyhow!(
                "internal error: fact atoms must be expanded before lowering"
            )),
            AxqlAtom::Type { term, type_name } => Ok(LoweredAtom::Type {
                term: lower_term(&term, &var_index)?,
                type_name,
            }),
            AxqlAtom::Edge { left, path, right } => {
                let left = lower_term(&left, &var_index)?;
                let right = lower_term(&right, &var_index)?;
                if let Some(rels) = simple_chain(&path.regex) {
                    if rels.len() == 1 {
                        return Ok(LoweredAtom::Edge {
                            left,
                            rel: rels[0].clone(),
                            right,
                        });
                    }
                }

                let rpq_id = if let Some(id) = rpq_index.get(&path.regex) {
                    *id
                } else {
                    let id = rpqs.len();
                    rpqs.push(path.regex.clone());
                    rpq_index.insert(path.regex.clone(), id);
                    id
                };
                Ok(LoweredAtom::Rpq {
                    left,
                    rpq_id,
                    right,
                })
            }
            AxqlAtom::AttrEq { term, key, value } => Ok(LoweredAtom::AttrEq {
                term: lower_term(&term, &var_index)?,
                key,
                value,
            }),
            AxqlAtom::AttrContains { term, key, needle } => Ok(LoweredAtom::AttrContains {
                term: lower_term(&term, &var_index)?,
                key,
                needle,
            }),
            AxqlAtom::AttrFts { term, key, query } => Ok(LoweredAtom::AttrFts {
                term: lower_term(&term, &var_index)?,
                key,
                query,
            }),
            AxqlAtom::AttrFuzzy {
                term,
                key,
                needle,
                max_dist,
            } => Ok(LoweredAtom::AttrFuzzy {
                term: lower_term(&term, &var_index)?,
                key,
                needle,
                max_dist,
            }),
            AxqlAtom::HasOut { .. } | AxqlAtom::Attrs { .. } | AxqlAtom::Shape { .. } => Err(
                anyhow!("internal error: shape macros must be expanded before lowering"),
            ),
        })
        .collect::<Result<Vec<_>>>()?;

    // Deduplicate select vars while preserving order.
    let mut seen = std::collections::HashSet::new();
    select_vars.retain(|v| seen.insert(v.clone()));

    let mut fact_field_intent: HashMap<usize, HashSet<String>> = HashMap::new();
    for (var, fields) in fact_field_intent_by_var {
        let Some(&id) = var_index.get(&var) else {
            continue;
        };
        fact_field_intent.insert(id, fields);
    }

    Ok(LoweredQuery {
        select_vars,
        vars,
        var_index,
        atoms: lowered_atoms,
        limit: query.limit,
        context_scope: query.contexts.clone(),
        max_hops: query.max_hops,
        min_confidence: query.min_confidence,
        rpqs,
        fact_field_intent,
    })
}

fn atom_terms(a: &AxqlAtom) -> Vec<AxqlTerm> {
    match a {
        AxqlAtom::Type { term, .. } => vec![term.clone()],
        AxqlAtom::Edge { left, right, .. } => vec![left.clone(), right.clone()],
        AxqlAtom::AttrEq { term, .. } => vec![term.clone()],
        AxqlAtom::AttrContains { term, .. } => vec![term.clone()],
        AxqlAtom::AttrFts { term, .. } => vec![term.clone()],
        AxqlAtom::AttrFuzzy { term, .. } => vec![term.clone()],
        AxqlAtom::Fact { .. } => Vec::new(),
        AxqlAtom::HasOut { term, .. } => vec![term.clone()],
        AxqlAtom::Attrs { term, .. } => vec![term.clone()],
        AxqlAtom::Shape { term, .. } => vec![term.clone()],
    }
}

fn ensure_var(vars: &mut Vec<String>, var_index: &mut HashMap<String, usize>, v: &str) {
    if var_index.contains_key(v) {
        return;
    }
    let id = vars.len();
    vars.push(v.to_string());
    var_index.insert(v.to_string(), id);
}

fn lower_term(t: &AxqlTerm, var_index: &HashMap<String, usize>) -> Result<LoweredTerm> {
    Ok(match t {
        AxqlTerm::Var(v) => {
            let Some(id) = var_index.get(v) else {
                return Err(anyhow!("unknown variable `{v}`"));
            };
            LoweredTerm::Var(*id)
        }
        AxqlTerm::Const(n) => LoweredTerm::Const(*n),
        AxqlTerm::Wildcard => {
            return Err(anyhow!("wildcard `_` must be expanded before lowering"));
        }
        AxqlTerm::Lookup { .. } => {
            return Err(anyhow!(
                "lookup term must be expanded before lowering (this is a bug in lower_query_disjunct)"
            ));
        }
    })
}

fn is_internal_var(v: &str) -> bool {
    v.starts_with("?_anon") || v.starts_with("?_lookup") || v.starts_with("?_fact")
}

fn simple_chain(regex: &AxqlRegex) -> Option<Vec<String>> {
    match regex {
        AxqlRegex::Rel(r) => Some(vec![r.clone()]),
        AxqlRegex::Seq(parts) => {
            let mut out = Vec::with_capacity(parts.len());
            for p in parts {
                match p {
                    AxqlRegex::Rel(r) => out.push(r.clone()),
                    _ => return None,
                }
            }
            Some(out)
        }
        _ => None,
    }
}

#[derive(Debug, Clone)]
struct ChainRewriteRule {
    /// Canonicalization direction (`from` -> `to`), chosen deterministically by
    /// `chain_key` ordering.
    from: Vec<String>,
    to: Vec<String>,

    /// Optional endpoint typing guards inferred from `.axi` rewrite rule vars.
    ///
    /// These are used during elaboration so we only apply rewrite rules when the
    /// query endpoints are consistent with the rule’s declared endpoint types.
    start_type: Option<String>,
    end_type: Option<String>,

    /// `.axi` rule identity (for certificates): `axi:<digest>:<theory>:<rule>`.
    theory_name: String,
    rule_name: String,

    /// Endpoint variable names from the rule patterns (used for deterministic
    /// substitution when emitting elaboration rewrite proofs).
    start_var: String,
    end_var: String,

    /// Path-expression templates (name-based) for the canonicalization direction.
    ///
    /// These are the parsed `.axi` rewrite rule LHS/RHS, reordered so that
    /// `from_expr` corresponds to `from` and `to_expr` corresponds to `to`.
    from_expr: PathExprV3,
    to_expr: PathExprV3,
}

fn chain_key(chain: &[String]) -> (usize, String) {
    (chain.len(), chain.join("/"))
}

fn parse_rewrite_vars(vars_text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for chunk in vars_text.split(',') {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        let mut parts = chunk.splitn(2, ':');
        let Some(name) = parts.next() else { continue };
        let Some(ty) = parts.next() else { continue };
        let name = name.trim();
        let ty = ty.trim();
        if name.is_empty() || ty.is_empty() {
            continue;
        }
        if ty.starts_with("Path(") {
            continue;
        }
        out.insert(name.to_string(), ty.to_string());
    }
    out
}

fn path_expr_v3_to_chain(expr: &PathExprV3) -> Option<Vec<String>> {
    match expr {
        PathExprV3::Step { rel, .. } => Some(vec![rel.to_string()]),
        PathExprV3::Trans { left, right } => {
            let mut out = path_expr_v3_to_chain(left)?;
            out.extend(path_expr_v3_to_chain(right)?);
            Some(out)
        }
        _ => None,
    }
}

fn path_expr_v3_endpoints(expr: &PathExprV3) -> Option<(String, String)> {
    match expr {
        PathExprV3::Step { from, to, .. } => Some((from.to_string(), to.to_string())),
        PathExprV3::Trans { left, right } => {
            let (start, _) = path_expr_v3_endpoints(left)?;
            let (_, end) = path_expr_v3_endpoints(right)?;
            Some((start, end))
        }
        _ => None,
    }
}

fn collect_chain_rewrite_rules(
    db: &axiograph_pathdb::PathDB,
) -> Vec<ChainRewriteRule> {
    let Some(rule_ids) = db.find_by_type(META_TYPE_REWRITE_RULE) else {
        return Vec::new();
    };

    let mut rules = Vec::new();
    for rid in rule_ids.iter() {
        let Some(view) = db.get_entity(rid) else { continue };
        let orientation = view
            .attrs
            .get(ATTR_REWRITE_RULE_ORIENTATION)
            .map(|s| s.to_lowercase())
            .unwrap_or_else(|| "forward".to_string());
        if orientation != "bidirectional" {
            continue;
        }

        let (theory_name, rule_name) = match view.attrs.get(META_ATTR_ID) {
            Some(meta_id) => {
                let mut parts = meta_id.split(':');
                let Some(prefix) = parts.next() else { continue };
                if prefix != "axi_meta_rewrite_rule" {
                    continue;
                }
                // Format: axi_meta_rewrite_rule:<module>:<theory>:<rule>
                let Some(_module_name) = parts.next() else { continue };
                let Some(theory_name) = parts.next() else { continue };
                let Some(rule_name) = parts.next() else { continue };
                if parts.next().is_some() {
                    continue;
                }
                (theory_name.to_string(), rule_name.to_string())
            }
            None => continue,
        };

        let lhs_text = view
            .attrs
            .get(ATTR_REWRITE_RULE_LHS)
            .cloned()
            .unwrap_or_default();
        let rhs_text = view
            .attrs
            .get(ATTR_REWRITE_RULE_RHS)
            .cloned()
            .unwrap_or_default();
        if lhs_text.trim().is_empty() || rhs_text.trim().is_empty() {
            continue;
        }

        let lhs_expr = match parse_path_expr_v3(lhs_text.trim()) {
            Ok(expr) => expr,
            Err(_) => continue,
        };
        let rhs_expr = match parse_path_expr_v3(rhs_text.trim()) {
            Ok(expr) => expr,
            Err(_) => continue,
        };

        let lhs_chain = match path_expr_v3_to_chain(&lhs_expr) {
            Some(chain) => chain,
            None => continue,
        };
        let rhs_chain = match path_expr_v3_to_chain(&rhs_expr) {
            Some(chain) => chain,
            None => continue,
        };
        if lhs_chain.is_empty() || rhs_chain.is_empty() {
            continue;
        }

        let (start_var, end_var) = match (
            path_expr_v3_endpoints(&lhs_expr),
            path_expr_v3_endpoints(&rhs_expr),
        ) {
            (Some(lhs_ep), Some(rhs_ep)) => {
                if lhs_ep != rhs_ep {
                    continue;
                }
                lhs_ep
            }
            (Some(ep), None) | (None, Some(ep)) => ep,
            (None, None) => continue,
        };

        let vars_text = view
            .attrs
            .get(ATTR_REWRITE_RULE_VARS)
            .cloned()
            .unwrap_or_default();
        let var_types = parse_rewrite_vars(&vars_text);
        let start_type = var_types.get(&start_var).cloned();
        let end_type = var_types.get(&end_var).cloned();

        let lhs_key = chain_key(&lhs_chain);
        let rhs_key = chain_key(&rhs_chain);
        if lhs_key == rhs_key {
            continue;
        }

        let (from, to, from_expr, to_expr) = if lhs_key > rhs_key {
            (lhs_chain, rhs_chain, lhs_expr, rhs_expr)
        } else {
            (rhs_chain, lhs_chain, rhs_expr, lhs_expr)
        };

        rules.push(ChainRewriteRule {
            from,
            to,
            start_type,
            end_type,
            theory_name,
            rule_name,
            start_var,
            end_var,
            from_expr,
            to_expr,
        });
    }

    rules
}

fn entity_matches_type(
    meta: Option<&MetaPlaneIndex>,
    entity_type: &str,
    required: &str,
) -> bool {
    if entity_type == required {
        return true;
    }
    let Some(meta) = meta else {
        return false;
    };
    for schema in meta.schemas.values() {
        if schema.is_subtype(entity_type, required) {
            return true;
        }
    }
    false
}

fn term_satisfies_type(
    db: &axiograph_pathdb::PathDB,
    meta: Option<&MetaPlaneIndex>,
    term: &LoweredTerm,
    required: &Option<String>,
    vars: &[String],
    inferred: &BTreeMap<String, Vec<String>>,
) -> bool {
    let Some(required) = required else {
        return true;
    };
    match term {
        LoweredTerm::Var(idx) => {
            let Some(name) = vars.get(*idx) else { return true };
            let Some(types) = inferred.get(name) else {
                return true;
            };
            types.iter().any(|t| t == required)
        }
        LoweredTerm::Const(id) => {
            let Some(entity) = db.get_entity(*id) else {
                return false;
            };
            entity_matches_type(meta, &entity.entity_type, required)
        }
    }
}

fn term_name_for_elaboration_rewrite_expr(
    db: &axiograph_pathdb::PathDB,
    term: &LoweredTerm,
    vars: &[String],
) -> String {
    match term {
        LoweredTerm::Var(idx) => vars.get(*idx).cloned().unwrap_or_else(|| format!("?v{idx}")),
        LoweredTerm::Const(id) => witness::stable_entity_id_v1(db, *id).unwrap_or_else(|_| {
            // Fallback for non-.axi anchored graphs (these rewrites are optional).
            id.to_string()
        }),
    }
}

fn substitute_rewrite_endpoints_v3(
    expr: &PathExprV3,
    start_var: &str,
    start_value: &str,
    end_var: &str,
    end_value: &str,
) -> PathExprV3 {
    let subst_entity = |e: &str| -> String {
        if e == start_var {
            start_value.to_string()
        } else if e == end_var {
            end_value.to_string()
        } else {
            e.to_string()
        }
    };

    match expr {
        PathExprV3::Var { name } => PathExprV3::Var { name: name.clone() },
        PathExprV3::Reflexive { entity } => PathExprV3::Reflexive {
            entity: subst_entity(entity),
        },
        PathExprV3::Step { from, rel, to } => PathExprV3::Step {
            from: subst_entity(from),
            rel: rel.clone(),
            to: subst_entity(to),
        },
        PathExprV3::Trans { left, right } => PathExprV3::Trans {
            left: Box::new(substitute_rewrite_endpoints_v3(
                left,
                start_var,
                start_value,
                end_var,
                end_value,
            )),
            right: Box::new(substitute_rewrite_endpoints_v3(
                right,
                start_var,
                start_value,
                end_var,
                end_value,
            )),
        },
        PathExprV3::Inv { path } => PathExprV3::Inv {
            path: Box::new(substitute_rewrite_endpoints_v3(
                path,
                start_var,
                start_value,
                end_var,
                end_value,
            )),
        },
    }
}

fn apply_chain_rewrites<'a>(
    chain: &[String],
    rules: &'a [ChainRewriteRule],
    left: &LoweredTerm,
    right: &LoweredTerm,
    vars: &[String],
    inferred: &BTreeMap<String, Vec<String>>,
    db: &axiograph_pathdb::PathDB,
    meta: Option<&MetaPlaneIndex>,
) -> Option<&'a ChainRewriteRule> {
    for rule in rules {
        if chain != rule.from {
            continue;
        }
        if !term_satisfies_type(db, meta, left, &rule.start_type, vars, inferred) {
            continue;
        }
        if !term_satisfies_type(db, meta, right, &rule.end_type, vars, inferred) {
            continue;
        }
        return Some(rule);
    }
    None
}

fn normalize_term(
    term: AxqlTerm,
    fresh_anon: &mut usize,
    lookup_vars: &mut HashMap<(String, String), String>,
    extra_atoms: &mut Vec<AxqlAtom>,
) -> AxqlTerm {
    match term {
        AxqlTerm::Wildcard => {
            let name = format!("?_anon{}", *fresh_anon);
            *fresh_anon += 1;
            AxqlTerm::Var(name)
        }
        AxqlTerm::Lookup { key, value } => {
            let k = (key.clone(), value.clone());
            let v = if let Some(existing) = lookup_vars.get(&k) {
                existing.clone()
            } else {
                let name = format!("?_lookup{}", *fresh_anon);
                *fresh_anon += 1;
                lookup_vars.insert(k, name.clone());
                extra_atoms.push(AxqlAtom::AttrEq {
                    term: AxqlTerm::Var(name.clone()),
                    key,
                    value,
                });
                name
            };
            AxqlTerm::Var(v)
        }
        other => other,
    }
}

impl LoweredQuery {
    fn render_as_axql(&self) -> String {
        fn render_term(vars: &[String], term: &LoweredTerm) -> String {
            match term {
                LoweredTerm::Var(v) => vars.get(*v).cloned().unwrap_or_else(|| format!("?v{v}")),
                LoweredTerm::Const(id) => id.to_string(),
            }
        }

        fn render_regex(re: &AxqlRegex) -> String {
            match re {
                AxqlRegex::Epsilon => "ε".to_string(),
                AxqlRegex::Rel(r) => r.clone(),
                AxqlRegex::Seq(parts) => {
                    parts.iter().map(render_regex).collect::<Vec<_>>().join("/")
                }
                AxqlRegex::Alt(parts) => {
                    format!(
                        "({})",
                        parts.iter().map(render_regex).collect::<Vec<_>>().join("|")
                    )
                }
                AxqlRegex::Star(inner) => format!("{}*", render_regex(inner)),
                AxqlRegex::Plus(inner) => format!("{}+", render_regex(inner)),
                AxqlRegex::Opt(inner) => format!("{}?", render_regex(inner)),
            }
        }

        let mut out = String::new();
        out.push_str("select ");
        if self.select_vars.is_empty() {
            out.push_str("* ");
        } else {
            out.push_str(&self.select_vars.join(" "));
            out.push(' ');
        }
        out.push_str("where ");

        let mut atoms: Vec<String> = Vec::new();
        for atom in &self.atoms {
            match atom {
                LoweredAtom::Type { term, type_name } => {
                    atoms.push(format!("{} : {}", render_term(&self.vars, term), type_name));
                }
                LoweredAtom::AttrEq { term, key, value } => {
                    atoms.push(format!(
                        "attr({}, \"{}\", \"{}\")",
                        render_term(&self.vars, term),
                        key,
                        value
                    ));
                }
                LoweredAtom::AttrContains { term, key, needle } => {
                    atoms.push(format!(
                        "contains({}, \"{}\", \"{}\")",
                        render_term(&self.vars, term),
                        key,
                        needle
                    ));
                }
                LoweredAtom::AttrFts { term, key, query } => {
                    atoms.push(format!(
                        "fts({}, \"{}\", \"{}\")",
                        render_term(&self.vars, term),
                        key,
                        query
                    ));
                }
                LoweredAtom::AttrFuzzy {
                    term,
                    key,
                    needle,
                    max_dist,
                } => {
                    atoms.push(format!(
                        "fuzzy({}, \"{}\", \"{}\", {})",
                        render_term(&self.vars, term),
                        key,
                        needle,
                        max_dist
                    ));
                }
                LoweredAtom::Edge { left, rel, right } => {
                    atoms.push(format!(
                        "{} -{}-> {}",
                        render_term(&self.vars, left),
                        rel,
                        render_term(&self.vars, right)
                    ));
                }
                LoweredAtom::Rpq {
                    left,
                    rpq_id,
                    right,
                } => {
                    let re = self
                        .rpqs
                        .get(*rpq_id)
                        .map(render_regex)
                        .unwrap_or_else(|| format!("rpq_{rpq_id}"));
                    atoms.push(format!(
                        "{} -{}-> {}",
                        render_term(&self.vars, left),
                        re,
                        render_term(&self.vars, right)
                    ));
                }
            }
        }

        out.push_str(&atoms.join(", "));

        if self.context_scope.len() > 1 {
            out.push_str(" in {");
            out.push_str(
                &self
                    .context_scope
                    .iter()
                    .map(|c| c.render())
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            out.push('}');
        }

        if self.limit != 20 {
            out.push_str(&format!(" limit {}", self.limit));
        }
        if let Some(max_hops) = self.max_hops {
            out.push_str(&format!(" max_hops {max_hops}"));
        }
        if let Some(min_conf) = self.min_confidence {
            out.push_str(&format!(" min_conf {min_conf}"));
        }

        out
    }

    /// Typecheck + elaborate an AxQL query using the meta-plane as a type layer.
    ///
    /// This pass is intentionally *user-facing*:
    /// - it catches common typos ("field foo not in relation Flow") early,
    /// - and it makes implied typing assumptions explicit by inserting `?x : T`
    ///   atoms (plus supertypes closure when known).
    fn typecheck_and_elaborate(
        &mut self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&MetaPlaneIndex>,
    ) -> Result<AxqlElaborationReport> {
        let Some(meta) = meta else {
            return Ok(AxqlElaborationReport::default());
        };
        if meta.schemas.is_empty() {
            return Ok(AxqlElaborationReport::default());
        }

        fn is_tuple_fact_type(
            schema: &axiograph_pathdb::axi_semantics::SchemaIndex,
            ty: &str,
        ) -> bool {
            let Some(base) = ty.strip_suffix("Fact") else {
                return false;
            };
            schema.object_types.contains(base) && schema.relation_decls.contains_key(base)
        }

        fn type_declared_in_meta(meta: &MetaPlaneIndex, type_name: &str) -> bool {
            for schema in meta.schemas.values() {
                if schema.object_types.contains(type_name) {
                    return true;
                }
                if schema.relation_decls.contains_key(type_name) {
                    return true;
                }
                if is_tuple_fact_type(schema, type_name) {
                    return true;
                }
                if schema
                    .supertypes_of
                    .values()
                    .any(|supers| supers.contains(type_name))
                {
                    return true;
                }
            }
            false
        }

        // Fail fast on unknown types when the meta-plane is present.
        // (If a type is declared but has no instances yet, the meta-plane still
        // knows it; if it is neither declared nor instantiated, it's likely a typo.)
        for atom in &self.atoms {
            let LoweredAtom::Type { type_name, .. } = atom else {
                continue;
            };
            if db.find_by_type(type_name).is_some() {
                continue;
            }
            if type_declared_in_meta(meta, type_name) {
                continue;
            }
            return Err(anyhow!(
                "unknown type `{type_name}` (not present in data-plane and not declared in the `.axi` meta-plane)"
            ));
        }

        // relation_name -> candidate schemas containing it
        let mut schemas_by_relation: HashMap<&str, Vec<&str>> = HashMap::new();
        for (schema_name, schema) in &meta.schemas {
            for rel in schema.relation_decls.keys() {
                schemas_by_relation
                    .entry(rel.as_str())
                    .or_default()
                    .push(schema_name.as_str());
            }
        }

        // Collect per-var schema/relation constraints (if present).
        let mut schema_by_fact_var: HashMap<usize, String> = HashMap::new();
        let mut relation_by_fact_var: HashMap<usize, String> = HashMap::new();
        for atom in &self.atoms {
            let LoweredAtom::AttrEq { term, key, value } = atom else {
                continue;
            };
            let LoweredTerm::Var(v) = term else {
                continue;
            };

            if key == ATTR_AXI_SCHEMA {
                schema_by_fact_var.insert(*v, value.clone());
            } else if key == ATTR_AXI_RELATION {
                relation_by_fact_var.insert(*v, value.clone());
            }
        }

        for (fact_var, schema_name) in &schema_by_fact_var {
            if !meta.schemas.contains_key(schema_name) {
                let fact_name = self
                    .vars
                    .get(*fact_var)
                    .cloned()
                    .unwrap_or_else(|| format!("?v{fact_var}"));
                return Err(anyhow!(
                    "unknown schema `{schema_name}` (in constraint attr({fact_name}, \"{ATTR_AXI_SCHEMA}\", \"{schema_name}\"))"
                ));
            }
        }

        // Build a small index of `fact_var -> [(field_name, right_term)]` so we don't scan atoms
        // repeatedly for each relation.
        let mut edges_by_fact_var: HashMap<usize, Vec<(&str, &LoweredTerm)>> = HashMap::new();
        for atom in &self.atoms {
            let LoweredAtom::Edge { left, rel, right } = atom else {
                continue;
            };
            let LoweredTerm::Var(v) = left else {
                continue;
            };
            edges_by_fact_var
                .entry(*v)
                .or_default()
                .push((rel.as_str(), right));
        }

        // Existing type constraints (to avoid duplicates).
        let mut existing_types: HashSet<(LoweredTerm, String)> = HashSet::new();
        for atom in &self.atoms {
            if let LoweredAtom::Type { term, type_name } = atom {
                existing_types.insert((term.clone(), type_name.clone()));
            }
        }

        let mut extra_atoms: Vec<LoweredAtom> = Vec::new();
        let mut report = AxqlElaborationReport::default();

        if !self.context_scope.is_empty() {
            report.notes.push(format!(
                "scoped to context(s): {}",
                self.context_scope
                    .iter()
                    .map(|c| c.render())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            if self.context_scope.len() > 1 {
                report.notes.push(
                    "note: multi-context scoping is execution-only right now (not certifiable yet)"
                        .to_string(),
                );
            }
        }

        let sole_schema_name = if meta.schemas.len() == 1 {
            meta.schemas.keys().next().map(|s| s.as_str())
        } else {
            None
        };

        for (&fact_var, relation_name) in &relation_by_fact_var {
            let fact_name = self
                .vars
                .get(fact_var)
                .cloned()
                .unwrap_or_else(|| format!("?v{fact_var}"));

            let candidate_schemas: Vec<&str> =
                if let Some(schema_name) = schema_by_fact_var.get(&fact_var) {
                    vec![schema_name.as_str()]
                } else if let Some(sole) = sole_schema_name {
                    vec![sole]
                } else {
                    schemas_by_relation
                        .get(relation_name.as_str())
                        .cloned()
                        .unwrap_or_default()
                };

            if candidate_schemas.is_empty() {
                return Err(anyhow!(
                    "unknown relation `{relation_name}` (in constraint attr({fact_name}, \"{ATTR_AXI_RELATION}\", \"{relation_name}\"))"
                ));
            }

            let chosen_schema_name: Option<&str> = if candidate_schemas.len() == 1 {
                Some(candidate_schemas[0])
            } else {
                // If the query did not constrain schema, and multiple schemas
                // declare the relation, keep this ambiguous. We'll still validate
                // fact-atom fields below (by union), but we avoid inventing a
                // specific schema for type inference.
                report.notes.push(format!(
                    "relation `{relation_name}` is ambiguous across schemas: {} (add `attr({fact_name}, \"{ATTR_AXI_SCHEMA}\", \"<Schema>\")` to disambiguate)",
                    candidate_schemas.join(", ")
                ));
                None
            };

            if let Some(schema_name) = chosen_schema_name {
                let Some(schema) = meta.schemas.get(schema_name) else {
                    continue;
                };
                let Some(rel_decl) = schema.relation_decls.get(relation_name) else {
                    return Err(anyhow!(
                        "relation `{relation_name}` is not declared in schema `{schema_name}`"
                    ));
                };

                // Fact node itself is a typed record.
                let tuple_type = tuple_entity_type_name(schema, relation_name);
                let fact_term = LoweredTerm::Var(fact_var);
                if existing_types.insert((fact_term.clone(), tuple_type.clone())) {
                    extra_atoms.push(LoweredAtom::Type {
                        term: fact_term.clone(),
                        type_name: tuple_type.clone(),
                    });
                    report
                        .inferred_types
                        .entry(fact_name.clone())
                        .or_default()
                        .push(tuple_type);
                }

                // Field edges: infer types for field values (plus supertypes closure).
                let edges = edges_by_fact_var
                    .get(&fact_var)
                    .cloned()
                    .unwrap_or_default();
                for (field_name, right) in edges {
                    let Some(field_decl) =
                        rel_decl.fields.iter().find(|f| f.field_name == field_name)
                    else {
                        // Only fail fast when the field came from `Rel(field=...)` syntax.
                        let is_intended_field = self
                            .fact_field_intent
                            .get(&fact_var)
                            .map(|set| set.contains(field_name))
                            .unwrap_or(false);
                        if is_intended_field {
                            return Err(anyhow!(
                                "field `{field_name}` not in relation `{relation_name}` (schema `{schema_name}`)"
                            ));
                        }
                        continue;
                    };

                    let Some(canonical_field_type) =
                        canonical_entity_type_for_axi_type(schema, &field_decl.field_type)
                    else {
                        continue;
                    };

                    let mut implied_types: Vec<String> = Vec::new();
                    if let Some(supers) = schema.supertypes_of.get(&canonical_field_type) {
                        implied_types.extend(supers.iter().cloned());
                    } else {
                        implied_types.push(canonical_field_type.clone());
                    }
                    implied_types.sort();
                    implied_types.dedup();

                    for ty in implied_types {
                        if existing_types.insert(((*right).clone(), ty.clone())) {
                            extra_atoms.push(LoweredAtom::Type {
                                term: (*right).clone(),
                                type_name: ty.clone(),
                            });
                            if let LoweredTerm::Var(var_id) = right {
                                if let Some(var_name) = self.vars.get(*var_id).cloned() {
                                    report.inferred_types.entry(var_name).or_default().push(ty);
                                }
                            }
                        }
                    }
                }
            } else {
                // Ambiguous schema: still validate that any fact-atom fields exist in *some*
                // candidate schema's relation declaration.
                let Some(edges) = edges_by_fact_var.get(&fact_var) else {
                    continue;
                };
                for (field_name, _right) in edges {
                    let is_intended_field = self
                        .fact_field_intent
                        .get(&fact_var)
                        .map(|set| set.contains(*field_name))
                        .unwrap_or(false);
                    if !is_intended_field {
                        continue;
                    }

                    let mut field_ok = false;
                    for schema_name in &candidate_schemas {
                        let Some(schema) = meta.schemas.get(*schema_name) else {
                            continue;
                        };
                        let Some(rel_decl) = schema.relation_decls.get(relation_name) else {
                            continue;
                        };
                        if rel_decl.fields.iter().any(|f| f.field_name == *field_name) {
                            field_ok = true;
                            break;
                        }
                    }

                    if !field_ok {
                        return Err(anyhow!(
                            "field `{field_name}` not in relation `{relation_name}` (schemas: {})",
                            candidate_schemas.join(", ")
                        ));
                    }
                }
            }
        }

        // Edge/RPQ endpoint inference:
        // If the meta-plane declares a relation signature, infer the endpoint types for:
        // - binary edge atoms (`?x -Rel-> ?y`), and
        // - simple RPQ chains (`?x -Rel0/Rel1-> ?y`).
        //
        // This improves:
        // - join planning (smaller domains),
        // - rewrite-rule applicability (typed gating), and
        // - UX (better inferred-types output in REPL/UI).
        let derive_endpoints_field_types =
            |decl: &axiograph_pathdb::axi_semantics::RelationDecl| -> Option<(String, String)> {
            // Mirror the PathDB importer’s deterministic endpoint selection
            // (`axi_module_import::derive_binary_endpoints`), but at the meta level.
            let mut primary: Vec<&axiograph_pathdb::axi_semantics::FieldDecl> = decl
                .fields
                .iter()
                .filter(|f| f.field_name != "ctx" && f.field_name != "time")
                .collect();
            primary.sort_by_key(|f| f.field_index);

            if primary.len() == 2 {
                let src: &axiograph_pathdb::axi_semantics::FieldDecl = primary[0];
                let dst: &axiograph_pathdb::axi_semantics::FieldDecl = primary[1];
                return Some((src.field_type.clone(), dst.field_type.clone()));
            }

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
                let a = decl.fields.iter().find(|f| f.field_name == src);
                let b = decl.fields.iter().find(|f| f.field_name == dst);
                if let (Some(a), Some(b)) = (a, b) {
                    return Some((a.field_type.clone(), b.field_type.clone()));
                }
            }
            None
        };

        let mut edge_rel_ambiguity: HashSet<String> = HashSet::new();

        let mut apply_implied_types =
            |term: &LoweredTerm, implied: Vec<String>| -> Result<()> {
                for ty in implied {
                    if existing_types.insert((term.clone(), ty.clone())) {
                        extra_atoms.push(LoweredAtom::Type {
                            term: term.clone(),
                            type_name: ty.clone(),
                        });
                        if let LoweredTerm::Var(var_id) = term {
                            if let Some(var_name) = self.vars.get(*var_id).cloned() {
                                report.inferred_types.entry(var_name).or_default().push(ty);
                            }
                        }
                    }
                }
                Ok(())
            };

        // Helper: resolve implied endpoint types for a single relation name.
        let mut relation_endpoints_implied_types =
            |relation_name: &str| -> Result<Option<(Vec<String>, Vec<String>)>> {
            let candidate_schemas: Vec<&str> = if let Some(sole) = sole_schema_name {
                vec![sole]
            } else {
                schemas_by_relation
                    .get(relation_name)
                    .cloned()
                    .unwrap_or_default()
            };
            if candidate_schemas.is_empty() {
                return Ok(None);
            }
            if candidate_schemas.len() > 1 {
                // We avoid guessing a schema here; edge atoms don’t currently
                // have an explicit schema qualifier.
                edge_rel_ambiguity.insert(relation_name.to_string());
                return Ok(None);
            }

            let schema_name = candidate_schemas[0];
            let Some(schema) = meta.schemas.get(schema_name) else {
                return Ok(None);
            };
            let Some(rel_decl) = schema.relation_decls.get(relation_name) else {
                return Ok(None);
            };
            let Some((src_field_type, dst_field_type)) =
                derive_endpoints_field_types(rel_decl)
            else {
                return Ok(None);
            };

            let Some(src_type) =
                canonical_entity_type_for_axi_type(schema, &src_field_type)
            else {
                return Ok(None);
            };
            let Some(dst_type) =
                canonical_entity_type_for_axi_type(schema, &dst_field_type)
            else {
                return Ok(None);
            };

            let mut src_implied: Vec<String> = Vec::new();
            if let Some(supers) = schema.supertypes_of.get(&src_type) {
                src_implied.extend(supers.iter().cloned());
            } else {
                src_implied.push(src_type.clone());
            }
            src_implied.sort();
            src_implied.dedup();

            let mut dst_implied: Vec<String> = Vec::new();
            if let Some(supers) = schema.supertypes_of.get(&dst_type) {
                dst_implied.extend(supers.iter().cloned());
            } else {
                dst_implied.push(dst_type.clone());
            }
            dst_implied.sort();
            dst_implied.dedup();

            Ok(Some((src_implied, dst_implied)))
        };

        for atom in &self.atoms {
            match atom {
                LoweredAtom::Edge { left, rel, right } => {
                    if let Some((src, dst)) = relation_endpoints_implied_types(rel)? {
                        apply_implied_types(left, src)?;
                        apply_implied_types(right, dst)?;
                    }
                }
                LoweredAtom::Rpq { left, rpq_id, right } => {
                    let Some(regex) = self.rpqs.get(*rpq_id) else {
                        return Err(anyhow!("invalid rpq_id {rpq_id}"));
                    };
                    if let Some(chain) = simple_chain(regex) {
                        if let (Some(first), Some(last)) = (chain.first(), chain.last()) {
                            // For a chain `r0/r1/.../rn`, infer:
                            // - `left` from the source endpoint type of `r0`
                            // - `right` from the destination endpoint type of `rn`
                            //
                            // We do not currently infer intermediate types for the hidden join vars.
                            if let Some((src0, _dst0)) =
                                relation_endpoints_implied_types(first)?
                            {
                                apply_implied_types(left, src0)?;
                            }
                            if let Some((_srcn, dstn)) =
                                relation_endpoints_implied_types(last)?
                            {
                                apply_implied_types(right, dstn)?;
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if !edge_rel_ambiguity.is_empty() {
            let mut rels = edge_rel_ambiguity.into_iter().collect::<Vec<_>>();
            rels.sort();
            report.notes.push(format!(
                "note: did not infer endpoint types for ambiguous edge relations: {}",
                rels.join(", ")
            ));
        }

        // Stable inferred-types ordering for display.
        for types in report.inferred_types.values_mut() {
            types.sort();
            types.dedup();
        }

        self.atoms.extend(extra_atoms);
        Ok(report)
    }

    fn canonicalize_paths(
        &mut self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&MetaPlaneIndex>,
        report: &mut AxqlElaborationReport,
    ) -> Result<()> {
        let Some(meta) = meta else {
            return Ok(());
        };
        if meta.schemas.is_empty() {
            return Ok(());
        }

        let rules = collect_chain_rewrite_rules(db);
        if rules.is_empty() {
            return Ok(());
        }

        let old_rpqs = self.rpqs.clone();
        let mut new_atoms: Vec<LoweredAtom> = Vec::with_capacity(self.atoms.len());
        let mut new_rpqs: Vec<AxqlRegex> = Vec::new();
        let mut rpq_index: HashMap<AxqlRegex, usize> = HashMap::new();
        let mut rewrites = 0usize;

        let intern_rpq = |regex: AxqlRegex,
                          rpq_index: &mut HashMap<AxqlRegex, usize>,
                          rpqs: &mut Vec<AxqlRegex>|
         -> usize {
            if let Some(id) = rpq_index.get(&regex) {
                *id
            } else {
                let id = rpqs.len();
                rpqs.push(regex.clone());
                rpq_index.insert(regex, id);
                id
            }
        };

        for atom in self.atoms.drain(..) {
            match atom {
                LoweredAtom::Edge { left, rel, right } => {
                    let left_name =
                        term_name_for_elaboration_rewrite_expr(db, &left, &self.vars);
                    let right_name =
                        term_name_for_elaboration_rewrite_expr(db, &right, &self.vars);
                    let mut chain = vec![rel];
                    for _ in 0..16 {
                        if let Some(rule) = apply_chain_rewrites(
                            &chain,
                            &rules,
                            &left,
                            &right,
                            &self.vars,
                            &report.inferred_types,
                            db,
                            Some(meta),
                        ) {
                            report.elaboration_rewrites.push(AxqlElaborationRewriteStepV1 {
                                theory_name: rule.theory_name.clone(),
                                rule_name: rule.rule_name.clone(),
                                input: substitute_rewrite_endpoints_v3(
                                    &rule.from_expr,
                                    &rule.start_var,
                                    &left_name,
                                    &rule.end_var,
                                    &right_name,
                                ),
                                output: substitute_rewrite_endpoints_v3(
                                    &rule.to_expr,
                                    &rule.start_var,
                                    &left_name,
                                    &rule.end_var,
                                    &right_name,
                                ),
                            });
                            chain = rule.to.clone();
                            rewrites += 1;
                        } else {
                            break;
                        }
                    }
                    if chain.len() == 1 {
                        new_atoms.push(LoweredAtom::Edge {
                            left,
                            rel: chain[0].clone(),
                            right,
                        });
                    } else {
                        let regex = AxqlRegex::Seq(
                            chain.into_iter().map(AxqlRegex::Rel).collect(),
                        );
                        let rpq_id = intern_rpq(regex, &mut rpq_index, &mut new_rpqs);
                        new_atoms.push(LoweredAtom::Rpq { left, rpq_id, right });
                    }
                }
                LoweredAtom::Rpq { left, rpq_id, right } => {
                    let left_name =
                        term_name_for_elaboration_rewrite_expr(db, &left, &self.vars);
                    let right_name =
                        term_name_for_elaboration_rewrite_expr(db, &right, &self.vars);
                    let Some(regex) = old_rpqs.get(rpq_id).cloned() else {
                        return Err(anyhow!("invalid rpq_id {rpq_id}"));
                    };
                    if let Some(mut chain) = simple_chain(&regex) {
                        for _ in 0..16 {
                            if let Some(rule) = apply_chain_rewrites(
                                &chain,
                                &rules,
                                &left,
                                &right,
                                &self.vars,
                                &report.inferred_types,
                                db,
                                Some(meta),
                            ) {
                                report.elaboration_rewrites.push(AxqlElaborationRewriteStepV1 {
                                    theory_name: rule.theory_name.clone(),
                                    rule_name: rule.rule_name.clone(),
                                    input: substitute_rewrite_endpoints_v3(
                                        &rule.from_expr,
                                        &rule.start_var,
                                        &left_name,
                                        &rule.end_var,
                                        &right_name,
                                    ),
                                    output: substitute_rewrite_endpoints_v3(
                                        &rule.to_expr,
                                        &rule.start_var,
                                        &left_name,
                                        &rule.end_var,
                                        &right_name,
                                    ),
                                });
                                chain = rule.to.clone();
                                rewrites += 1;
                            } else {
                                break;
                            }
                        }
                        if chain.len() == 1 {
                            new_atoms.push(LoweredAtom::Edge {
                                left,
                                rel: chain[0].clone(),
                                right,
                            });
                        } else {
                            let regex = AxqlRegex::Seq(
                                chain.into_iter().map(AxqlRegex::Rel).collect(),
                            );
                            let rpq_id = intern_rpq(regex, &mut rpq_index, &mut new_rpqs);
                            new_atoms.push(LoweredAtom::Rpq { left, rpq_id, right });
                        }
                    } else {
                        let rpq_id = intern_rpq(regex, &mut rpq_index, &mut new_rpqs);
                        new_atoms.push(LoweredAtom::Rpq { left, rpq_id, right });
                    }
                }
                other => new_atoms.push(other),
            }
        }

        self.atoms = new_atoms;
        self.rpqs = new_rpqs;

        if rewrites > 0 {
            report
                .notes
                .push(format!(
                    "canonicalized {rewrites} path atom(s) via `.axi` rewrite rules"
                ));
        }

        Ok(())
    }

    fn certify(
        &self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&MetaPlaneIndex>,
    ) -> Result<CertificateV2> {
        let mut rpq = RpqContext::new(db, &self.rpqs, self.max_hops, self.min_confidence)?;

        let query = self.to_query_ir(db)?;
        let mut rows: Vec<QueryRowV1> = Vec::new();

        let truncated = if self.vars.is_empty() {
            // Boolean query: either 0 or 1 row, with no bindings.
            if let Some(witnesses) = self.witnesses_for_assignment(db, &[], &mut rpq, meta)? {
                rows.push(QueryRowV1 {
                    bindings: Vec::new(),
                    witnesses,
                });
            }
            false
        } else {
            let (assignments, truncated) = self.execute_assignments(db, &mut rpq, meta)?;
            for assignment in assignments {
                let bindings = self
                    .vars
                    .iter()
                    .zip(assignment.iter().copied())
                    .map(|(var, entity)| QueryBindingV1 {
                        var: var.clone(),
                        entity,
                    })
                    .collect::<Vec<_>>();

                let Some(witnesses) =
                    self.witnesses_for_assignment(db, &assignment, &mut rpq, meta)?
                else {
                    return Err(anyhow!(
                        "internal error: assignment from search does not satisfy constraints"
                    ));
                };
                rows.push(QueryRowV1 {
                    bindings,
                    witnesses,
                });
            }
            truncated
        };

        let proof = QueryResultProofV1 {
            query,
            rows,
            truncated,
        };
        Ok(CertificateV2::query_result_v1(proof))
    }

    fn certify_v3(
        &self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&MetaPlaneIndex>,
        axi_digest_v1: &str,
        elaboration: &AxqlElaborationReport,
    ) -> Result<CertificateV2> {
        let mut rpq = RpqContext::new(db, &self.rpqs, self.max_hops, self.min_confidence)?;

        let disjunct_atoms = self.to_query_ir_v3_disjunct(db)?;
        let query = QueryV3 {
            select_vars: self.select_vars.clone(),
            disjuncts: vec![disjunct_atoms],
            max_hops: self.max_hops,
            min_confidence_fp: self.min_confidence.map(fixed_prob_from_confidence),
        };

        let mut rows: Vec<QueryRowV3> = Vec::new();

        let truncated = if self.vars.is_empty() {
            // Boolean query: either 0 or 1 row, with no bindings.
            if let Some(witnesses) =
                self.witnesses_for_assignment_v3(db, &[], &mut rpq, meta)?
            {
                rows.push(QueryRowV3 {
                    disjunct: 0,
                    bindings: Vec::new(),
                    witnesses,
                });
            }
            false
        } else {
            let (assignments, truncated) = self.execute_assignments(db, &mut rpq, meta)?;
            for assignment in assignments {
                let bindings = self
                    .vars
                    .iter()
                    .zip(assignment.iter().copied())
                    .map(|(var, entity)| {
                        Ok(QueryBindingV3 {
                            var: var.clone(),
                            entity: witness::stable_entity_id_v1(db, entity)?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;

                let Some(witnesses) =
                    self.witnesses_for_assignment_v3(db, &assignment, &mut rpq, meta)?
                else {
                    return Err(anyhow!(
                        "internal error: assignment from search does not satisfy constraints"
                    ));
                };
                rows.push(QueryRowV3 {
                    disjunct: 0,
                    bindings,
                    witnesses,
                });
            }
            truncated
        };

        let proof = QueryResultProofV3 {
            query,
            rows,
            truncated,
            elaboration_rewrites: elaboration
                .elaboration_rewrites
                .iter()
                .map(|step| RewriteDerivationProofV3 {
                    input: step.input.clone(),
                    output: step.output.clone(),
                    derivation: vec![PathRewriteStepV3 {
                        pos: Vec::new(),
                        rule_ref: format!(
                            "axi:{}:{}:{}",
                            axi_digest_v1, step.theory_name, step.rule_name
                        ),
                    }],
                })
                .collect(),
        };
        Ok(CertificateV2::query_result_v3(proof))
    }

    fn to_query_ir(&self, db: &axiograph_pathdb::PathDB) -> Result<QueryV1> {
        let atoms = self
            .atoms
            .iter()
            .map(|a| self.atom_to_query_ir(db, a))
            .collect::<Result<Vec<_>>>()?;

        Ok(QueryV1 {
            select_vars: self.select_vars.clone(),
            atoms,
            max_hops: self.max_hops,
            min_confidence_fp: self.min_confidence.map(fixed_prob_from_confidence),
        })
    }

    fn atom_to_query_ir(
        &self,
        db: &axiograph_pathdb::PathDB,
        atom: &LoweredAtom,
    ) -> Result<QueryAtomV1> {
        match atom {
            LoweredAtom::Type { term, type_name } => {
                let type_id = db
                    .interner
                    .id_of(type_name)
                    .ok_or_else(|| anyhow!("unknown type `{type_name}` (missing from interner)"))?;
                Ok(QueryAtomV1::Type {
                    term: self.term_to_query_ir(term),
                    type_id: type_id.raw(),
                })
            }
            LoweredAtom::AttrEq { term, key, value } => {
                let key_id = db.interner.id_of(key).ok_or_else(|| {
                    anyhow!("unknown attribute key `{key}` (missing from interner)")
                })?;
                let value_id = db.interner.id_of(value).ok_or_else(|| {
                    anyhow!("unknown attribute value `{value}` (missing from interner)")
                })?;
                Ok(QueryAtomV1::AttrEq {
                    term: self.term_to_query_ir(term),
                    key_id: key_id.raw(),
                    value_id: value_id.raw(),
                })
            }
            LoweredAtom::AttrContains { .. } => Err(anyhow!(
                "cannot certify `contains(...)` atoms (approximate querying is not in the certified core)"
            )),
            LoweredAtom::AttrFts { .. } => Err(anyhow!(
                "cannot certify `fts(...)` atoms (approximate querying is not in the certified core)"
            )),
            LoweredAtom::AttrFuzzy { .. } => Err(anyhow!(
                "cannot certify `fuzzy(...)` atoms (approximate querying is not in the certified core)"
            )),
            LoweredAtom::Edge { left, rel, right } => {
                let rel_type_id = db
                    .interner
                    .id_of(rel)
                    .ok_or_else(|| anyhow!("unknown relation `{rel}` (missing from interner)"))?;
                Ok(QueryAtomV1::Path {
                    left: self.term_to_query_ir(left),
                    regex: QueryRegexV1::Rel {
                        rel_type_id: rel_type_id.raw(),
                    },
                    right: self.term_to_query_ir(right),
                })
            }
            LoweredAtom::Rpq {
                left,
                rpq_id,
                right,
            } => {
                let regex = self
                    .rpqs
                    .get(*rpq_id)
                    .ok_or_else(|| anyhow!("invalid rpq_id {rpq_id}"))?;
                Ok(QueryAtomV1::Path {
                    left: self.term_to_query_ir(left),
                    regex: axql_regex_to_query_regex(db, regex)?,
                    right: self.term_to_query_ir(right),
                })
            }
        }
    }

    fn term_to_query_ir(&self, term: &LoweredTerm) -> QueryTermV1 {
        match term {
            LoweredTerm::Var(v) => QueryTermV1::Var {
                name: self.vars[*v].clone(),
            },
            LoweredTerm::Const(entity) => QueryTermV1::Const { entity: *entity },
        }
    }

    fn to_query_ir_v3_disjunct(&self, db: &axiograph_pathdb::PathDB) -> Result<Vec<QueryAtomV3>> {
        self.atoms
            .iter()
            .map(|a| self.atom_to_query_ir_v3(db, a))
            .collect::<Result<Vec<_>>>()
    }

    fn term_to_query_ir_v3(
        &self,
        db: &axiograph_pathdb::PathDB,
        term: &LoweredTerm,
    ) -> Result<QueryTermV3> {
        Ok(match term {
            LoweredTerm::Var(v) => QueryTermV3::Var {
                name: self.vars.get(*v).cloned().unwrap_or_else(|| format!("?v{v}")),
            },
            LoweredTerm::Const(entity) => QueryTermV3::Const {
                entity: witness::stable_entity_id_v1(db, *entity)?,
            },
        })
    }

    fn regex_to_query_ir_v3(&self, regex: &AxqlRegex) -> QueryRegexV3 {
        match regex {
            AxqlRegex::Epsilon => QueryRegexV3::Epsilon,
            AxqlRegex::Rel(r) => QueryRegexV3::Rel { rel: r.clone() },
            AxqlRegex::Seq(parts) => QueryRegexV3::Seq {
                parts: parts.iter().map(|p| self.regex_to_query_ir_v3(p)).collect(),
            },
            AxqlRegex::Alt(parts) => QueryRegexV3::Alt {
                parts: parts.iter().map(|p| self.regex_to_query_ir_v3(p)).collect(),
            },
            AxqlRegex::Star(inner) => QueryRegexV3::Star {
                inner: Box::new(self.regex_to_query_ir_v3(inner)),
            },
            AxqlRegex::Plus(inner) => QueryRegexV3::Plus {
                inner: Box::new(self.regex_to_query_ir_v3(inner)),
            },
            AxqlRegex::Opt(inner) => QueryRegexV3::Opt {
                inner: Box::new(self.regex_to_query_ir_v3(inner)),
            },
        }
    }

    fn atom_to_query_ir_v3(
        &self,
        db: &axiograph_pathdb::PathDB,
        atom: &LoweredAtom,
    ) -> Result<QueryAtomV3> {
        match atom {
            LoweredAtom::Type { term, type_name } => Ok(QueryAtomV3::Type {
                term: self.term_to_query_ir_v3(db, term)?,
                type_name: type_name.clone(),
            }),
            LoweredAtom::AttrEq { term, key, value } => Ok(QueryAtomV3::AttrEq {
                term: self.term_to_query_ir_v3(db, term)?,
                key: key.clone(),
                value: value.clone(),
            }),
            LoweredAtom::Edge { left, rel, right } => Ok(QueryAtomV3::Path {
                left: self.term_to_query_ir_v3(db, left)?,
                regex: QueryRegexV3::Rel { rel: rel.clone() },
                right: self.term_to_query_ir_v3(db, right)?,
            }),
            LoweredAtom::Rpq {
                left,
                rpq_id,
                right,
            } => {
                let regex = self
                    .rpqs
                    .get(*rpq_id)
                    .ok_or_else(|| anyhow!("invalid rpq_id {rpq_id}"))?;
                Ok(QueryAtomV3::Path {
                    left: self.term_to_query_ir_v3(db, left)?,
                    regex: self.regex_to_query_ir_v3(regex),
                    right: self.term_to_query_ir_v3(db, right)?,
                })
            }
            other => Err(anyhow!(
                "cannot certify atom {other:?} in query_result_v3 (not in certified core)"
            )),
        }
    }

    fn estimate_atom_cost(
        &self,
        db: &axiograph_pathdb::PathDB,
        rpq: &mut RpqContext,
        candidates: &[RoaringBitmap],
        atom: &LoweredAtom,
    ) -> usize {
        match atom {
            LoweredAtom::Type { term, .. }
            | LoweredAtom::AttrEq { term, .. }
            | LoweredAtom::AttrContains { term, .. }
            | LoweredAtom::AttrFts { term, .. }
            | LoweredAtom::AttrFuzzy { term, .. } => match term {
                LoweredTerm::Var(v) => candidates
                    .get(*v)
                    .map(|c| c.len() as usize)
                    .unwrap_or(0),
                LoweredTerm::Const(_) => 1,
            },
            LoweredAtom::Edge { left, rel, right } => {
                let Some(rel_id) = db.interner.id_of(rel) else {
                    return 0;
                };
                match (left, right) {
                    (LoweredTerm::Const(_s), LoweredTerm::Const(_)) => 1,
                    (LoweredTerm::Const(s), _) => match self.min_confidence {
                        None => db.relations.targets(*s, rel_id).len() as usize,
                        Some(min) => db
                            .relations
                            .targets_with_min_confidence(*s, rel_id, min)
                            .len() as usize,
                    },
                    (_, LoweredTerm::Const(t)) => match self.min_confidence {
                        None => db.relations.sources(*t, rel_id).len() as usize,
                        Some(min) => db
                            .relations
                            .sources_with_min_confidence(*t, rel_id, min)
                            .len() as usize,
                    },
                    _ => db.relations.rel_type_count(rel_id),
                }
            }
            LoweredAtom::Rpq {
                left,
                rpq_id,
                right,
            } => match (left, right) {
                (LoweredTerm::Const(_s), LoweredTerm::Const(_)) => 1,
                (LoweredTerm::Const(s), _) => rpq
                    .reachable_set(db, *rpq_id, *s)
                    .map(|set| set.len() as usize)
                    .unwrap_or(0),
                (_, LoweredTerm::Const(t)) => rpq
                    .reachable_set_reverse(db, *rpq_id, *t)
                    .map(|set| set.len() as usize)
                    .unwrap_or(0),
                _ => {
                    let l = term_var(left)
                        .and_then(|v| candidates.get(v))
                        .map(|c| c.len() as usize)
                        .unwrap_or(0);
                    let r = term_var(right)
                        .and_then(|v| candidates.get(v))
                        .map(|c| c.len() as usize)
                        .unwrap_or(0);
                    l.saturating_mul(r)
                }
            },
        }
    }

    fn estimate_var_selectivity(
        &self,
        db: &axiograph_pathdb::PathDB,
        rpq: &mut RpqContext,
        candidates: &[RoaringBitmap],
    ) -> Vec<usize> {
        let mut scores = vec![0usize; self.vars.len()];
        for atom in &self.atoms {
            let cost = self.estimate_atom_cost(db, rpq, candidates, atom);
            for v in lowered_atom_vars(atom) {
                if v < scores.len() {
                    scores[v] = scores[v].saturating_add(cost);
                }
            }
        }
        scores
    }

    fn atom_order(
        &self,
        db: &axiograph_pathdb::PathDB,
        rpq: &mut RpqContext,
        candidates: &[RoaringBitmap],
    ) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.atoms.len()).collect();
        order.sort_by_key(|&idx| self.estimate_atom_cost(db, rpq, candidates, &self.atoms[idx]));
        order
    }

    fn plan(
        &self,
        db: &axiograph_pathdb::PathDB,
        rpq: &mut RpqContext,
        meta: Option<&MetaPlaneIndex>,
    ) -> Result<QueryPlan> {
        if self.vars.is_empty() {
            return Ok(QueryPlan {
                candidates: Vec::new(),
                order: Vec::new(),
                atom_order: Vec::new(),
            });
        }

        let entity_count = db.entities.len();
        let all_entities: RoaringBitmap = (0u32..(entity_count as u32)).collect();

        let mut candidates: Vec<RoaringBitmap> = vec![all_entities.clone(); self.vars.len()];

        // Apply unary constraints first (type, attr).
        for atom in &self.atoms {
            match atom {
                LoweredAtom::Type { term, type_name } => {
                    self.apply_type_constraint(db, &mut candidates, term, type_name, meta)?;
                }
                LoweredAtom::AttrEq { term, key, value } => {
                    self.apply_attr_eq_constraint(db, &mut candidates, term, key, value)?;
                }
                LoweredAtom::AttrContains { term, key, needle } => {
                    self.apply_attr_contains_constraint(db, &mut candidates, term, key, needle)?;
                }
                LoweredAtom::AttrFts { term, key, query } => {
                    self.apply_attr_fts_constraint(db, &mut candidates, term, key, query)?;
                }
                LoweredAtom::AttrFuzzy {
                    term,
                    key,
                    needle,
                    max_dist,
                } => {
                    self.apply_attr_fuzzy_constraint(
                        db,
                        &mut candidates,
                        term,
                        key,
                        needle,
                        *max_dist,
                    )?;
                }
                LoweredAtom::Edge { .. } | LoweredAtom::Rpq { .. } => {}
            }
        }

        // FactIndex-driven pruning: use `(axi_schema, axi_relation)` and (when available)
        // key constraints to reduce the candidate set for fact-node variables.
        self.apply_fact_index_constraints(db, &mut candidates, meta)?;

        // Arc-consistency-like propagation (best effort pruning).
        self.propagate_edges(db, &mut candidates, rpq)?;

        // Backtracking search order (smallest domain first, biased by constraint counts).
        let mut order: Vec<usize> = (0..self.vars.len()).collect();
        let mut constraint_counts = self.constraint_counts();
        self.apply_schema_constraint_hints(meta, &mut constraint_counts)?;
        let selectivity = self.estimate_var_selectivity(db, rpq, &candidates);
        order.sort_by_key(|&vid| {
            (
                candidates[vid].len(),
                selectivity.get(vid).copied().unwrap_or(usize::MAX),
                std::cmp::Reverse(constraint_counts[vid]),
            )
        });

        let atom_order = self.atom_order(db, rpq, &candidates);

        Ok(QueryPlan {
            candidates,
            order,
            atom_order,
        })
    }

    fn execute_assignments(
        &self,
        db: &axiograph_pathdb::PathDB,
        rpq: &mut RpqContext,
        meta: Option<&MetaPlaneIndex>,
    ) -> Result<(Vec<Vec<u32>>, bool)> {
        if self.vars.is_empty() {
            return Ok((Vec::new(), false));
        }

        let plan = self.plan(db, rpq, meta)?;

        let mut assigned: Vec<Option<u32>> = vec![None; self.vars.len()];
        let mut assignments: Vec<Vec<u32>> = Vec::new();
        let mut truncated = false;

        self.search_assignments(
            db,
            &plan.candidates,
            &plan.order,
            &plan.atom_order,
            0,
            &mut assigned,
            &mut assignments,
            &mut truncated,
            rpq,
            meta,
        )?;

        Ok((assignments, truncated))
    }

    fn search_assignments(
        &self,
        db: &axiograph_pathdb::PathDB,
        candidates: &[RoaringBitmap],
        order: &[usize],
        atom_order: &[usize],
        idx: usize,
        assigned: &mut [Option<u32>],
        out: &mut Vec<Vec<u32>>,
        truncated: &mut bool,
        rpq: &mut RpqContext,
        meta: Option<&MetaPlaneIndex>,
    ) -> Result<()> {
        if out.len() >= self.limit {
            *truncated = true;
            return Ok(());
        }

        if idx == order.len() {
            let mut row: Vec<u32> = Vec::with_capacity(self.vars.len());
            for v in 0..self.vars.len() {
                let Some(value) = assigned[v] else {
                    return Err(anyhow!(
                        "internal error: missing assignment for var {}",
                        self.vars[v]
                    ));
                };
                row.push(value);
            }
            out.push(row);
            return Ok(());
        }

        let var = order[idx];
        for value in candidates[var].iter() {
            assigned[var] = Some(value);

            if self.partial_check(db, candidates, assigned, rpq, atom_order, meta)? {
                self.search_assignments(
                    db,
                    candidates,
                    order,
                    atom_order,
                    idx + 1,
                    assigned,
                    out,
                    truncated,
                    rpq,
                    meta,
                )?;
                if *truncated {
                    assigned[var] = None;
                    return Ok(());
                }
            }

            assigned[var] = None;
        }

        Ok(())
    }

    fn witnesses_for_assignment(
        &self,
        db: &axiograph_pathdb::PathDB,
        assignment: &[u32],
        rpq: &mut RpqContext,
        meta: Option<&MetaPlaneIndex>,
    ) -> Result<Option<Vec<QueryAtomWitnessV1>>> {
        let mut witnesses: Vec<QueryAtomWitnessV1> = Vec::with_capacity(self.atoms.len());

        for atom in &self.atoms {
            let wit = match atom {
                LoweredAtom::Type { term, type_name } => {
                    let entity = resolve_term_assigned(term, assignment);
                    let expected = db.interner.id_of(type_name).ok_or_else(|| {
                        anyhow!("unknown type `{type_name}` (missing from interner)")
                    })?;
                    let bitmap = type_bitmap_including_subtypes(db, type_name, meta);
                    if !bitmap.contains(entity) {
                        return Ok(None);
                    }
                    QueryAtomWitnessV1::Type {
                        entity,
                        type_id: expected.raw(),
                    }
                }
                LoweredAtom::AttrEq { term, key, value } => {
                    let entity = resolve_term_assigned(term, assignment);
                    let key_id = db.interner.id_of(key).ok_or_else(|| {
                        anyhow!("unknown attribute key `{key}` (missing from interner)")
                    })?;
                    let value_id = db.interner.id_of(value).ok_or_else(|| {
                        anyhow!("unknown attribute value `{value}` (missing from interner)")
                    })?;
                    if db.entities.get_attr(entity, key_id) != Some(value_id) {
                        return Ok(None);
                    }
                    QueryAtomWitnessV1::AttrEq {
                        entity,
                        key_id: key_id.raw(),
                        value_id: value_id.raw(),
                    }
                }
                LoweredAtom::AttrContains { .. } => {
                    return Err(anyhow!(
                        "cannot certify `contains(...)` atoms (approximate querying is not in the certified core)"
                    ))
                }
                LoweredAtom::AttrFts { .. } => {
                    return Err(anyhow!(
                        "cannot certify `fts(...)` atoms (approximate querying is not in the certified core)"
                    ))
                }
                LoweredAtom::AttrFuzzy { .. } => {
                    return Err(anyhow!(
                        "cannot certify `fuzzy(...)` atoms (approximate querying is not in the certified core)"
                    ))
                }
                LoweredAtom::Edge { left, rel, right } => {
                    let src = resolve_term_assigned(left, assignment);
                    let dst = resolve_term_assigned(right, assignment);
                    let rel_type_id = db.interner.id_of(rel).ok_or_else(|| {
                        anyhow!("unknown relation `{rel}` (missing from interner)")
                    })?;
                    let relation_id = match self.min_confidence {
                        None => db.relations.edge_relation_id(src, rel_type_id, dst),
                        Some(min) => db
                            .relations
                            .edge_relation_id_with_min_confidence(src, rel_type_id, dst, min),
                    };
                    let Some(relation_id) = relation_id else {
                        return Ok(None);
                    };
                    let rel = db.relations.get_relation(relation_id).ok_or_else(|| {
                        anyhow!("internal error: missing relation {relation_id} in RelationStore")
                    })?;
                    let rel_confidence_fp = fixed_prob_from_confidence(rel.confidence);
                    let proof = ReachabilityProofV2::Step {
                        from: rel.source,
                        rel_type: rel.rel_type.raw(),
                        to: rel.target,
                        rel_confidence_fp,
                        relation_id: Some(relation_id),
                        rest: Box::new(ReachabilityProofV2::Reflexive { entity: rel.target }),
                    };
                    QueryAtomWitnessV1::Path { proof }
                }
                LoweredAtom::Rpq {
                    left,
                    rpq_id,
                    right,
                } => {
                    let src = resolve_term_assigned(left, assignment);
                    let dst = resolve_term_assigned(right, assignment);
                    let Some(rel_ids) = rpq.witness_relation_ids(db, *rpq_id, src, dst)? else {
                        return Ok(None);
                    };
                    let proof = axiograph_pathdb::witness::reachability_proof_v2_from_relation_ids(
                        db, src, &rel_ids,
                    )?
                    .into_inner_in_db(db)
                    .map_err(|e| anyhow!(e))?;
                    QueryAtomWitnessV1::Path { proof }
                }
            };

            witnesses.push(wit);
        }

        Ok(Some(witnesses))
    }

    fn witnesses_for_assignment_v3(
        &self,
        db: &axiograph_pathdb::PathDB,
        assignment: &[u32],
        rpq: &mut RpqContext,
        meta: Option<&MetaPlaneIndex>,
    ) -> Result<Option<Vec<QueryAtomWitnessV3>>> {
        let mut witnesses: Vec<QueryAtomWitnessV3> = Vec::with_capacity(self.atoms.len());

        for atom in &self.atoms {
            let wit = match atom {
                LoweredAtom::Type { term, type_name } => {
                    let entity = resolve_term_assigned(term, assignment);
                    if db.interner.id_of(type_name).is_none() {
                        return Err(anyhow!("unknown type `{type_name}` (missing from interner)"));
                    }
                    let bitmap = type_bitmap_including_subtypes(db, type_name, meta);
                    if !bitmap.contains(entity) {
                        return Ok(None);
                    }
                    QueryAtomWitnessV3::Type {
                        entity: witness::stable_entity_id_v1(db, entity)?,
                        type_name: type_name.clone(),
                    }
                }
                LoweredAtom::AttrEq { term, key, value } => {
                    let entity = resolve_term_assigned(term, assignment);
                    let key_id = db.interner.id_of(key).ok_or_else(|| {
                        anyhow!("unknown attribute key `{key}` (missing from interner)")
                    })?;
                    let value_id = db.interner.id_of(value).ok_or_else(|| {
                        anyhow!("unknown attribute value `{value}` (missing from interner)")
                    })?;
                    if db.entities.get_attr(entity, key_id) != Some(value_id) {
                        return Ok(None);
                    }
                    QueryAtomWitnessV3::AttrEq {
                        entity: witness::stable_entity_id_v1(db, entity)?,
                        key: key.clone(),
                        value: value.clone(),
                    }
                }
                LoweredAtom::AttrContains { .. } => {
                    return Err(anyhow!(
                        "cannot certify `contains(...)` atoms (approximate querying is not in the certified core)"
                    ))
                }
                LoweredAtom::AttrFts { .. } => {
                    return Err(anyhow!(
                        "cannot certify `fts(...)` atoms (approximate querying is not in the certified core)"
                    ))
                }
                LoweredAtom::AttrFuzzy { .. } => {
                    return Err(anyhow!(
                        "cannot certify `fuzzy(...)` atoms (approximate querying is not in the certified core)"
                    ))
                }
                LoweredAtom::Edge { left, rel, right } => {
                    let src = resolve_term_assigned(left, assignment);
                    let dst = resolve_term_assigned(right, assignment);
                    let rel_type_id = db.interner.id_of(rel).ok_or_else(|| {
                        anyhow!("unknown relation `{rel}` (missing from interner)")
                    })?;
                    let relation_id = match self.min_confidence {
                        None => db.relations.edge_relation_id(src, rel_type_id, dst),
                        Some(min) => db
                            .relations
                            .edge_relation_id_with_min_confidence(src, rel_type_id, dst, min),
                    };
                    let Some(relation_id) = relation_id else {
                        return Ok(None);
                    };
                    let proof =
                        witness::reachability_proof_v3_from_relation_ids(db, src, &[relation_id])?
                            .into_inner_in_db(db)
                            .map_err(|e| anyhow!(e))?;
                    QueryAtomWitnessV3::Path { proof }
                }
                LoweredAtom::Rpq {
                    left,
                    rpq_id,
                    right,
                } => {
                    let src = resolve_term_assigned(left, assignment);
                    let dst = resolve_term_assigned(right, assignment);
                    let Some(rel_ids) = rpq.witness_relation_ids(db, *rpq_id, src, dst)? else {
                        return Ok(None);
                    };
                    let proof = witness::reachability_proof_v3_from_relation_ids(db, src, &rel_ids)?
                        .into_inner_in_db(db)
                        .map_err(|e| anyhow!(e))?;
                    QueryAtomWitnessV3::Path { proof }
                }
            };

            witnesses.push(wit);
        }

        Ok(Some(witnesses))
    }

    fn constraint_counts(&self) -> Vec<usize> {
        let mut counts = vec![0usize; self.vars.len()];
        for atom in &self.atoms {
            for v in lowered_atom_vars(atom) {
                counts[v] += 1;
            }
        }
        counts
    }

    fn apply_schema_constraint_hints(
        &self,
        meta: Option<&MetaPlaneIndex>,
        counts: &mut [usize],
    ) -> Result<()> {
        let Some(meta) = meta else {
            return Ok(());
        };
        if meta.schemas.is_empty() {
            return Ok(());
        }

        // relation_name -> candidate schemas containing it
        let mut schemas_by_relation: HashMap<&str, Vec<&str>> = HashMap::new();
        for (schema_name, schema) in &meta.schemas {
            for rel in schema.relation_decls.keys() {
                schemas_by_relation
                    .entry(rel.as_str())
                    .or_default()
                    .push(schema_name.as_str());
            }
        }

        let mut schema_by_fact_var: HashMap<usize, String> = HashMap::new();
        let mut relation_by_fact_var: HashMap<usize, String> = HashMap::new();
        let mut field_targets_by_fact_var: HashMap<(usize, &str), Vec<&LoweredTerm>> =
            HashMap::new();

        for atom in &self.atoms {
            match atom {
                LoweredAtom::AttrEq { term, key, value } => {
                    let LoweredTerm::Var(v) = term else {
                        continue;
                    };

                    if key == ATTR_AXI_SCHEMA {
                        schema_by_fact_var.insert(*v, value.clone());
                    } else if key == ATTR_AXI_RELATION {
                        relation_by_fact_var.insert(*v, value.clone());
                    }
                }
                LoweredAtom::Edge { left, rel, right } => {
                    let LoweredTerm::Var(v) = left else {
                        continue;
                    };
                    field_targets_by_fact_var
                        .entry((*v, rel.as_str()))
                        .or_default()
                        .push(right);
                }
                _ => {}
            }
        }

        for (&fact_var, relation_name) in &relation_by_fact_var {
            if fact_var >= counts.len() {
                continue;
            }

            let schema_name = if let Some(s) = schema_by_fact_var.get(&fact_var) {
                Some(s.as_str())
            } else {
                match schemas_by_relation.get(relation_name.as_str()) {
                    Some(schemas) if schemas.len() == 1 => Some(schemas[0]),
                    _ => None,
                }
            };

            let Some(schema_name) = schema_name else {
                continue;
            };
            let Some(schema) = meta.schemas.get(schema_name) else {
                continue;
            };

            let Some(constraints) = schema.constraints_by_relation.get(relation_name) else {
                continue;
            };

            for c in constraints {
                match c {
                    axiograph_pathdb::axi_semantics::ConstraintDecl::Key { fields, .. } => {
                        // Keys make the fact node strongly constrained by its key fields; bias
                        // search ordering toward assigning it earlier.
                        counts[fact_var] = counts[fact_var].saturating_add(fields.len() * 2);

                        for field in fields {
                            for right in field_targets_by_fact_var
                                .get(&(fact_var, field.as_str()))
                                .into_iter()
                                .flatten()
                            {
                                match right {
                                    LoweredTerm::Var(v) => {
                                        if *v < counts.len() {
                                            counts[*v] = counts[*v].saturating_add(1);
                                        }
                                    }
                                    LoweredTerm::Const(_) => {
                                        counts[fact_var] = counts[fact_var].saturating_add(1);
                                    }
                                }
                            }
                        }
                    }
                    axiograph_pathdb::axi_semantics::ConstraintDecl::Functional {
                        src_field,
                        dst_field,
                        ..
                    } => {
                        // Very small heuristic: functional deps tend to reduce branching.
                        // Prefer assigning the involved field vars earlier when present.
                        for field in [src_field, dst_field] {
                            for right in field_targets_by_fact_var
                                .get(&(fact_var, field.as_str()))
                                .into_iter()
                                .flatten()
                            {
                                if let LoweredTerm::Var(v) = right {
                                    if *v < counts.len() {
                                        counts[*v] = counts[*v].saturating_add(1);
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    fn fast_single_path_query(
        &self,
        db: &axiograph_pathdb::PathDB,
        rpq: &mut RpqContext,
        meta: Option<&MetaPlaneIndex>,
    ) -> Result<Option<AxqlResult>> {
        if self.vars.is_empty() {
            return Ok(None);
        }

        let mut path_atom: Option<&LoweredAtom> = None;
        let mut unary_atoms: Vec<&LoweredAtom> = Vec::new();

        for atom in &self.atoms {
            match atom {
                LoweredAtom::Edge { .. } | LoweredAtom::Rpq { .. } => {
                    if path_atom.is_some() {
                        return Ok(None);
                    }
                    path_atom = Some(atom);
                }
                LoweredAtom::Type { .. }
                | LoweredAtom::AttrEq { .. }
                | LoweredAtom::AttrContains { .. }
                | LoweredAtom::AttrFts { .. }
                | LoweredAtom::AttrFuzzy { .. } => unary_atoms.push(atom),
            }
        }

        let Some(path_atom) = path_atom else {
            return Ok(None);
        };

        let (var_idx, reachable) = match path_atom {
            LoweredAtom::Edge { left, rel, right } => {
                let Some(rel_id) = db.interner.id_of(rel) else {
                    return Ok(Some(AxqlResult {
                        selected_vars: self.select_vars.clone(),
                        rows: Vec::new(),
                        truncated: false,
                    }));
                };
                match (left, right) {
                    (LoweredTerm::Const(s), LoweredTerm::Var(v)) => {
                        let set = match self.min_confidence {
                            None => db.relations.targets(*s, rel_id),
                            Some(min) => db.relations.targets_with_min_confidence(*s, rel_id, min),
                        };
                        (*v, set)
                    }
                    (LoweredTerm::Var(v), LoweredTerm::Const(t)) => {
                        let set = match self.min_confidence {
                            None => db.relations.sources(*t, rel_id),
                            Some(min) => db.relations.sources_with_min_confidence(*t, rel_id, min),
                        };
                        (*v, set)
                    }
                    _ => return Ok(None),
                }
            }
            LoweredAtom::Rpq {
                left,
                rpq_id,
                right,
            } => match (left, right) {
                (LoweredTerm::Const(s), LoweredTerm::Var(v)) => {
                    let set = rpq.reachable_set(db, *rpq_id, *s)?;
                    (*v, set)
                }
                (LoweredTerm::Var(v), LoweredTerm::Const(t)) => {
                    let set = rpq.reachable_set_reverse(db, *rpq_id, *t)?;
                    (*v, set)
                }
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };

        // Ensure selected vars align with the single variable in scope.
        for v in &self.select_vars {
            if self.var_index.get(v).copied() != Some(var_idx) {
                return Ok(None);
            }
        }

        let mut type_filter: Option<RoaringBitmap> = None;
        let mut attr_atoms: Vec<&LoweredAtom> = Vec::new();

        for atom in &unary_atoms {
            match atom {
                LoweredAtom::Type { term, type_name } => match term {
                    LoweredTerm::Const(id) => {
                        let bitmap = type_bitmap_including_subtypes(db, type_name, meta);
                        if !bitmap.contains(*id) {
                            return Ok(Some(AxqlResult {
                                selected_vars: self.select_vars.clone(),
                                rows: Vec::new(),
                                truncated: false,
                            }));
                        }
                    }
                    LoweredTerm::Var(v) if *v == var_idx => {
                        let bitmap = type_bitmap_including_subtypes(db, type_name, meta);
                        match &mut type_filter {
                            Some(current) => *current &= bitmap,
                            None => type_filter = Some(bitmap),
                        }
                    }
                    _ => return Ok(None),
                },
                LoweredAtom::AttrEq { term, .. }
                | LoweredAtom::AttrContains { term, .. }
                | LoweredAtom::AttrFts { term, .. }
                | LoweredAtom::AttrFuzzy { term, .. } => match term {
                    LoweredTerm::Const(id) => {
                        let ok = match atom {
                            LoweredAtom::AttrEq { key, value, .. } => {
                                entity_has_attr(db, *id, key, value)?
                            }
                            LoweredAtom::AttrContains { key, needle, .. } => {
                                entity_attr_contains(db, *id, key, needle)?
                            }
                            LoweredAtom::AttrFts { key, query, .. } => {
                                entity_attr_fts(db, *id, key, query)?
                            }
                            LoweredAtom::AttrFuzzy {
                                key,
                                needle,
                                max_dist,
                                ..
                            } => entity_attr_fuzzy(db, *id, key, needle, *max_dist)?,
                            _ => false,
                        };
                        if !ok {
                            return Ok(Some(AxqlResult {
                                selected_vars: self.select_vars.clone(),
                                rows: Vec::new(),
                                truncated: false,
                            }));
                        }
                    }
                    LoweredTerm::Var(v) if *v == var_idx => attr_atoms.push(*atom),
                    _ => return Ok(None),
                },
                _ => return Ok(None),
            }
        }

        let mut filtered = reachable;
        if let Some(filter) = type_filter {
            filtered &= filter;
        }

        let mut rows = Vec::new();
        let mut truncated = false;
        for entity_id in filtered.iter() {
            let mut ok = true;
            for atom in &attr_atoms {
                let passes = match atom {
                    LoweredAtom::AttrEq {
                        key,
                        value: attr_value,
                        ..
                    } => entity_has_attr(db, entity_id, key, attr_value)?,
                    LoweredAtom::AttrContains { key, needle, .. } => {
                        entity_attr_contains(db, entity_id, key, needle)?
                    }
                    LoweredAtom::AttrFts { key, query, .. } => {
                        entity_attr_fts(db, entity_id, key, query)?
                    }
                    LoweredAtom::AttrFuzzy {
                        key,
                        needle,
                        max_dist,
                        ..
                    } => entity_attr_fuzzy(db, entity_id, key, needle, *max_dist)?,
                    _ => true,
                };
                if !passes {
                    ok = false;
                    break;
                }
            }
            if !ok {
                continue;
            }

            if rows.len() >= self.limit {
                truncated = true;
                break;
            }
            let mut row = BTreeMap::new();
            for v in &self.select_vars {
                row.insert(v.clone(), entity_id);
            }
            rows.push(row);
        }

        Ok(Some(AxqlResult {
            selected_vars: self.select_vars.clone(),
            rows,
            truncated,
        }))
    }

    fn execute(
        &self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&MetaPlaneIndex>,
    ) -> Result<AxqlResult> {
        let mut rpq = RpqContext::new(db, &self.rpqs, self.max_hops, self.min_confidence)?;

        if self.vars.is_empty() {
            // A query without variables: treat it as a boolean check.
            let ok = self.check_grounded(db, &mut rpq, meta)?;
            let rows = if ok { vec![BTreeMap::new()] } else { vec![] };
            return Ok(AxqlResult {
                selected_vars: Vec::new(),
                rows,
                truncated: false,
            });
        }

        if let Some(result) = self.fast_single_path_query(db, &mut rpq, meta)? {
            return Ok(result);
        }
        let plan = self.plan(db, &mut rpq, meta)?;

        let mut assigned: Vec<Option<u32>> = vec![None; self.vars.len()];

        let mut rows: Vec<BTreeMap<String, u32>> = Vec::new();
        let mut truncated = false;

        self.search(
            db,
            &plan.candidates,
            &plan.order,
            &plan.atom_order,
            0,
            &mut assigned,
            &mut rows,
            &mut truncated,
            &mut rpq,
            meta,
        )?;

        Ok(AxqlResult {
            selected_vars: self.select_vars.clone(),
            rows,
            truncated,
        })
    }

    fn check_grounded(
        &self,
        db: &axiograph_pathdb::PathDB,
        rpq: &mut RpqContext,
        meta: Option<&MetaPlaneIndex>,
    ) -> Result<bool> {
        for atom in &self.atoms {
            match atom {
                LoweredAtom::Type { term, type_name } => match term {
                    LoweredTerm::Const(id) => {
                        let bitmap = type_bitmap_including_subtypes(db, type_name, meta);
                        if !bitmap.contains(*id) {
                            return Ok(false);
                        }
                    }
                    LoweredTerm::Var(_) => {
                        return Err(anyhow!("internal error: ungrounded var in grounded query"))
                    }
                },
                LoweredAtom::AttrEq { term, key, value } => match term {
                    LoweredTerm::Const(id) => {
                        if !entity_has_attr(db, *id, key, value)? {
                            return Ok(false);
                        }
                    }
                    LoweredTerm::Var(_) => {
                        return Err(anyhow!("internal error: ungrounded var in grounded query"))
                    }
                },
                LoweredAtom::AttrContains { term, key, needle } => match term {
                    LoweredTerm::Const(id) => {
                        if !entity_attr_contains(db, *id, key, needle)? {
                            return Ok(false);
                        }
                    }
                    LoweredTerm::Var(_) => {
                        return Err(anyhow!("internal error: ungrounded var in grounded query"))
                    }
                },
                LoweredAtom::AttrFts { term, key, query } => match term {
                    LoweredTerm::Const(id) => {
                        if !entity_attr_fts(db, *id, key, query)? {
                            return Ok(false);
                        }
                    }
                    LoweredTerm::Var(_) => {
                        return Err(anyhow!("internal error: ungrounded var in grounded query"))
                    }
                },
                LoweredAtom::AttrFuzzy {
                    term,
                    key,
                    needle,
                    max_dist,
                } => match term {
                    LoweredTerm::Const(id) => {
                        if !entity_attr_fuzzy(db, *id, key, needle, *max_dist)? {
                            return Ok(false);
                        }
                    }
                    LoweredTerm::Var(_) => {
                        return Err(anyhow!("internal error: ungrounded var in grounded query"))
                    }
                },
                LoweredAtom::Edge { left, rel, right } => match (left, right) {
                    (LoweredTerm::Const(s), LoweredTerm::Const(t)) => {
                        if !edge_exists(db, *s, rel, *t, self.min_confidence)? {
                            return Ok(false);
                        }
                    }
                    _ => return Err(anyhow!("internal error: ungrounded var in grounded query")),
                },
                LoweredAtom::Rpq {
                    left,
                    rpq_id,
                    right,
                } => match (left, right) {
                    (LoweredTerm::Const(s), LoweredTerm::Const(t)) => {
                        if !rpq.reachable(db, *rpq_id, *s, *t)? {
                            return Ok(false);
                        }
                    }
                    _ => return Err(anyhow!("internal error: ungrounded var in grounded query")),
                },
            }
        }
        Ok(true)
    }

    fn apply_type_constraint(
        &self,
        db: &axiograph_pathdb::PathDB,
        candidates: &mut [RoaringBitmap],
        term: &LoweredTerm,
        type_name: &str,
        meta: Option<&MetaPlaneIndex>,
    ) -> Result<()> {
        let bitmap = type_bitmap_including_subtypes(db, type_name, meta);
        match term {
            LoweredTerm::Var(v) => {
                candidates[*v] &= bitmap;
            }
            LoweredTerm::Const(id) => {
                if !bitmap.contains(*id) {
                    for c in candidates {
                        c.clear();
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_attr_eq_constraint(
        &self,
        db: &axiograph_pathdb::PathDB,
        candidates: &mut [RoaringBitmap],
        term: &LoweredTerm,
        key: &str,
        value: &str,
    ) -> Result<()> {
        match term {
            LoweredTerm::Var(v) => {
                // Canonical `.axi` fact nodes are heavily filtered by `axi_relation`.
                // Use the PathDB FactIndex rather than scanning the attribute column.
                if key == ATTR_AXI_RELATION {
                    candidates[*v] &= db.fact_nodes_by_axi_relation(value);
                    return Ok(());
                }

                let Some(key_id) = db.interner.id_of(key) else {
                    candidates[*v] = RoaringBitmap::new();
                    return Ok(());
                };
                let Some(value_id) = db.interner.id_of(value) else {
                    candidates[*v] = RoaringBitmap::new();
                    return Ok(());
                };
                candidates[*v] &= db.entities.entities_with_attr_value(key_id, value_id);
            }
            LoweredTerm::Const(id) => {
                if !entity_has_attr(db, *id, key, value)? {
                    for c in candidates {
                        c.clear();
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_fact_index_constraints(
        &self,
        db: &axiograph_pathdb::PathDB,
        candidates: &mut [RoaringBitmap],
        meta: Option<&MetaPlaneIndex>,
    ) -> Result<()> {
        if candidates.is_empty() {
            return Ok(());
        }

        // Optional context scoping.
        //
        // We resolve context selectors here (against the current snapshot) and
        // use the FactIndex to prune fact-node variables early.
        let mut resolved_context_ids: Vec<u32> = Vec::new();
        if !self.context_scope.is_empty() {
            for c in &self.context_scope {
                match c {
                    AxqlContextSpec::EntityId(id) => {
                        if db.get_entity(*id).is_none() {
                            return Err(anyhow!("unknown context entity id {id}"));
                        }
                        resolved_context_ids.push(*id);
                    }
                    AxqlContextSpec::Name(name) => {
                        let Some(key_id) = db.interner.id_of("name") else {
                            return Err(anyhow!("database has no `name` attribute interned"));
                        };
                        let Some(value_id) = db.interner.id_of(name) else {
                            return Err(anyhow!(
                                "unknown context `{name}` (no entity with that name)"
                            ));
                        };
                        let ids = db.entities.entities_with_attr_value(key_id, value_id);
                        if ids.is_empty() {
                            return Err(anyhow!(
                                "unknown context `{name}` (no entity with that name)"
                            ));
                        }
                        if ids.len() > 1 {
                            return Err(anyhow!(
                                "ambiguous context name `{name}` ({} matches). Use a numeric id.",
                                ids.len()
                            ));
                        }
                        resolved_context_ids.push(ids.iter().next().unwrap_or(0));
                    }
                }
            }

            resolved_context_ids.sort_unstable();
            resolved_context_ids.dedup();
        }

        // Collect explicit per-var schema/relation constraints (if present), plus any
        // constant field bindings of the form `fact -field-> Const(...)`.
        let mut schema_by_fact_var: HashMap<usize, String> = HashMap::new();
        let mut relation_by_fact_var: HashMap<usize, String> = HashMap::new();
        let mut const_fields_by_fact_var: HashMap<usize, HashMap<String, u32>> = HashMap::new();

        for atom in &self.atoms {
            match atom {
                LoweredAtom::AttrEq { term, key, value } => {
                    let LoweredTerm::Var(v) = term else {
                        continue;
                    };
                    if key == ATTR_AXI_SCHEMA {
                        schema_by_fact_var.insert(*v, value.clone());
                    } else if key == ATTR_AXI_RELATION {
                        relation_by_fact_var.insert(*v, value.clone());
                    }
                }
                LoweredAtom::Edge { left, rel, right } => {
                    let (LoweredTerm::Var(v), LoweredTerm::Const(id)) = (left, right) else {
                        continue;
                    };
                    const_fields_by_fact_var
                        .entry(*v)
                        .or_default()
                        .insert(rel.clone(), *id);
                }
                _ => {}
            }
        }

        if relation_by_fact_var.is_empty() {
            return Ok(());
        }

        // relation_name -> candidate schemas containing it (from the meta-plane).
        let mut schemas_by_relation: HashMap<&str, Vec<&str>> = HashMap::new();
        if let Some(meta) = meta {
            for (schema_name, schema) in &meta.schemas {
                for rel in schema.relation_decls.keys() {
                    schemas_by_relation
                        .entry(rel.as_str())
                        .or_default()
                        .push(schema_name.as_str());
                }
            }
        }

        for (&fact_var, relation_name) in &relation_by_fact_var {
            if fact_var >= candidates.len() {
                continue;
            }

            let schema_name: Option<&str> = if let Some(s) = schema_by_fact_var.get(&fact_var) {
                Some(s.as_str())
            } else {
                match schemas_by_relation.get(relation_name.as_str()) {
                    Some(schemas) if schemas.len() == 1 => Some(schemas[0]),
                    _ => None,
                }
            };

            let Some(schema_name) = schema_name else {
                // Still apply context scoping when requested, even when we can't
                // disambiguate schema purely from meta-plane info.
                if !resolved_context_ids.is_empty() {
                    let mut scoped = RoaringBitmap::new();
                    for ctx in &resolved_context_ids {
                        scoped |= db.fact_nodes_by_context(*ctx);
                    }
                    candidates[fact_var] &= scoped;
                }
                continue;
            };

            // Tighten to (schema, relation) candidates using the FactIndex.
            if resolved_context_ids.is_empty() {
                candidates[fact_var] &=
                    db.fact_nodes_by_axi_schema_relation(schema_name, relation_name);
            } else {
                let mut scoped = RoaringBitmap::new();
                for ctx in &resolved_context_ids {
                    scoped |= db.fact_nodes_by_context_axi_schema_relation(
                        *ctx,
                        schema_name,
                        relation_name,
                    );
                }
                candidates[fact_var] &= scoped;
            }

            // If the schema has an imported key constraint for this relation, and the
            // query binds all key fields to constants, do a near-index lookup.
            let Some(meta) = meta else {
                continue;
            };
            let Some(schema) = meta.schemas.get(schema_name) else {
                continue;
            };
            let Some(constraints) = schema.constraints_by_relation.get(relation_name) else {
                continue;
            };

            let Some(const_fields) = const_fields_by_fact_var.get(&fact_var) else {
                continue;
            };

            for c in constraints {
                let axiograph_pathdb::axi_semantics::ConstraintDecl::Key { fields, .. } = c else {
                    continue;
                };
                if fields.is_empty() {
                    continue;
                }

                let mut values: Vec<u32> = Vec::with_capacity(fields.len());
                let mut ok = true;
                for f in fields {
                    let Some(v) = const_fields.get(f) else {
                        ok = false;
                        break;
                    };
                    values.push(*v);
                }
                if !ok {
                    continue;
                }

                let key_fields = fields.iter().map(|s| s.as_str()).collect::<Vec<_>>();
                let Some(fact_ids) =
                    db.fact_nodes_by_axi_key(schema_name, relation_name, &key_fields, &values)
                else {
                    continue;
                };

                let mut keyed = RoaringBitmap::new();
                for id in fact_ids {
                    keyed.insert(id);
                }
                candidates[fact_var] &= keyed;
            }
        }

        Ok(())
    }

    fn apply_attr_contains_constraint(
        &self,
        db: &axiograph_pathdb::PathDB,
        candidates: &mut [RoaringBitmap],
        term: &LoweredTerm,
        key: &str,
        needle: &str,
    ) -> Result<()> {
        match term {
            LoweredTerm::Var(v) => {
                candidates[*v] &= db.entities_with_attr_contains(key, needle);
            }
            LoweredTerm::Const(id) => {
                if !entity_attr_contains(db, *id, key, needle)? {
                    for c in candidates {
                        c.clear();
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_attr_fts_constraint(
        &self,
        db: &axiograph_pathdb::PathDB,
        candidates: &mut [RoaringBitmap],
        term: &LoweredTerm,
        key: &str,
        query: &str,
    ) -> Result<()> {
        match term {
            LoweredTerm::Var(v) => {
                candidates[*v] &= db.entities_with_attr_fts(key, query);
            }
            LoweredTerm::Const(id) => {
                if !entity_attr_fts(db, *id, key, query)? {
                    for c in candidates {
                        c.clear();
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_attr_fuzzy_constraint(
        &self,
        db: &axiograph_pathdb::PathDB,
        candidates: &mut [RoaringBitmap],
        term: &LoweredTerm,
        key: &str,
        needle: &str,
        max_dist: usize,
    ) -> Result<()> {
        match term {
            LoweredTerm::Var(v) => {
                candidates[*v] &= db.entities_with_attr_fuzzy(key, needle, max_dist);
            }
            LoweredTerm::Const(id) => {
                if !entity_attr_fuzzy(db, *id, key, needle, max_dist)? {
                    for c in candidates {
                        c.clear();
                    }
                }
            }
        }
        Ok(())
    }

    fn propagate_edges(
        &self,
        db: &axiograph_pathdb::PathDB,
        candidates: &mut [RoaringBitmap],
        rpq: &mut RpqContext,
    ) -> Result<()> {
        // Fixed-point propagation over edge constraints.
        // This is best-effort pruning; correctness comes from the final search checks.
        let mut changed = true;
        let mut iter = 0usize;
        while changed && iter < 8 {
            iter += 1;
            changed = false;

            for atom in &self.atoms {
                match atom {
                    LoweredAtom::Edge { left, rel, right } => {
                        let Some(rel_id) = db.interner.id_of(rel) else {
                            // Relation name not present => no edges.
                            if let LoweredTerm::Var(v) = left {
                                if !candidates[*v].is_empty() {
                                    candidates[*v].clear();
                                    changed = true;
                                }
                            }
                            if let LoweredTerm::Var(v) = right {
                                if !candidates[*v].is_empty() {
                                    candidates[*v].clear();
                                    changed = true;
                                }
                            }
                            continue;
                        };

                        // left -> right
                        if let Some(right_var) = term_var(right) {
                            let mut reach = RoaringBitmap::new();
                            for_each_value(left, candidates, |s| {
                                reach |= match self.min_confidence {
                                    None => db.relations.targets(s, rel_id),
                                    Some(min) => {
                                        db.relations.targets_with_min_confidence(s, rel_id, min)
                                    }
                                };
                            });
                            let before = candidates[right_var].len();
                            candidates[right_var] &= reach;
                            if candidates[right_var].len() != before {
                                changed = true;
                            }
                        }

                        // right -> left (reverse)
                        if let Some(left_var) = term_var(left) {
                            let mut reach = RoaringBitmap::new();
                            for_each_value(right, candidates, |t| {
                                reach |= match self.min_confidence {
                                    None => db.relations.sources(t, rel_id),
                                    Some(min) => {
                                        db.relations.sources_with_min_confidence(t, rel_id, min)
                                    }
                                };
                            });
                            let before = candidates[left_var].len();
                            candidates[left_var] &= reach;
                            if candidates[left_var].len() != before {
                                changed = true;
                            }
                        }
                    }
                    LoweredAtom::Rpq {
                        left,
                        rpq_id,
                        right,
                    } => {
                        // Best-effort pruning for RPQs: only run when the driving side
                        // is small enough to avoid exploding work.
                        const MAX_STARTS: u64 = 128;

                        // left -> right
                        if let Some(right_var) = term_var(right) {
                            let mut reach = RoaringBitmap::new();
                            if let LoweredTerm::Const(s) = left {
                                reach |= rpq.reachable_set(db, *rpq_id, *s)?;
                            } else if let Some(left_var) = term_var(left) {
                                if candidates[left_var].len() <= MAX_STARTS {
                                    for s in candidates[left_var].iter() {
                                        reach |= rpq.reachable_set(db, *rpq_id, s)?;
                                    }
                                }
                            }
                            if !reach.is_empty() {
                                let before = candidates[right_var].len();
                                candidates[right_var] &= reach;
                                if candidates[right_var].len() != before {
                                    changed = true;
                                }
                            }
                        }

                        // right -> left (reverse reachability)
                        if let Some(left_var) = term_var(left) {
                            let mut reach = RoaringBitmap::new();
                            if let LoweredTerm::Const(t) = right {
                                reach |= rpq.reachable_set_reverse(db, *rpq_id, *t)?;
                            } else if let Some(right_var) = term_var(right) {
                                if candidates[right_var].len() <= MAX_STARTS {
                                    for t in candidates[right_var].iter() {
                                        reach |= rpq.reachable_set_reverse(db, *rpq_id, t)?;
                                    }
                                }
                            }
                            if !reach.is_empty() {
                                let before = candidates[left_var].len();
                                candidates[left_var] &= reach;
                                if candidates[left_var].len() != before {
                                    changed = true;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn search(
        &self,
        db: &axiograph_pathdb::PathDB,
        candidates: &[RoaringBitmap],
        order: &[usize],
        atom_order: &[usize],
        idx: usize,
        assigned: &mut [Option<u32>],
        rows: &mut Vec<BTreeMap<String, u32>>,
        truncated: &mut bool,
        rpq: &mut RpqContext,
        meta: Option<&MetaPlaneIndex>,
    ) -> Result<()> {
        if rows.len() >= self.limit {
            *truncated = true;
            return Ok(());
        }

        if idx == order.len() {
            let mut row = BTreeMap::new();
            for v in &self.select_vars {
                let id = self.var_index[v];
                if let Some(value) = assigned[id] {
                    row.insert(v.clone(), value);
                }
            }
            rows.push(row);
            return Ok(());
        }

        let var = order[idx];
        for value in candidates[var].iter() {
            assigned[var] = Some(value);

            if self.partial_check(db, candidates, assigned, rpq, atom_order, meta)? {
                self.search(
                    db,
                    candidates,
                    order,
                    atom_order,
                    idx + 1,
                    assigned,
                    rows,
                    truncated,
                    rpq,
                    meta,
                )?;
                if *truncated {
                    assigned[var] = None;
                    return Ok(());
                }
            }

            assigned[var] = None;
        }

        Ok(())
    }

    fn partial_check(
        &self,
        db: &axiograph_pathdb::PathDB,
        candidates: &[RoaringBitmap],
        assigned: &[Option<u32>],
        rpq: &mut RpqContext,
        atom_order: &[usize],
        meta: Option<&MetaPlaneIndex>,
    ) -> Result<bool> {
        for &atom_idx in atom_order {
            let atom = &self.atoms[atom_idx];
            match atom {
                LoweredAtom::Type { term, type_name } => {
                    if let Some(id) = resolve_term(term, assigned) {
                        let bitmap = type_bitmap_including_subtypes(db, type_name, meta);
                        if !bitmap.contains(id) {
                            return Ok(false);
                        }
                    }
                }
                LoweredAtom::AttrEq { term, key, value } => {
                    if let Some(id) = resolve_term(term, assigned) {
                        if !entity_has_attr(db, id, key, value)? {
                            return Ok(false);
                        }
                    }
                }
                LoweredAtom::AttrContains { term, key, needle } => {
                    if let Some(id) = resolve_term(term, assigned) {
                        if !entity_attr_contains(db, id, key, needle)? {
                            return Ok(false);
                        }
                    }
                }
                LoweredAtom::AttrFts { term, key, query } => {
                    if let Some(id) = resolve_term(term, assigned) {
                        if !entity_attr_fts(db, id, key, query)? {
                            return Ok(false);
                        }
                    }
                }
                LoweredAtom::AttrFuzzy {
                    term,
                    key,
                    needle,
                    max_dist,
                } => {
                    if let Some(id) = resolve_term(term, assigned) {
                        if !entity_attr_fuzzy(db, id, key, needle, *max_dist)? {
                            return Ok(false);
                        }
                    }
                }
                LoweredAtom::Edge { left, rel, right } => {
                    let Some(rel_id) = db.interner.id_of(rel) else {
                        return Ok(false);
                    };

                    let l = resolve_term(left, assigned);
                    let r = resolve_term(right, assigned);

                    match (l, r) {
                        (Some(s), Some(t)) => {
                            let ok = match self.min_confidence {
                                None => db.relations.has_edge(s, rel_id, t),
                                Some(min) => {
                                    db.relations.has_edge_with_min_confidence(s, rel_id, t, min)
                                }
                            };
                            if !ok {
                                return Ok(false);
                            }
                        }
                        (Some(s), None) => {
                            // Prune: does `s` have any `rel` edges into the remaining candidates of `right`?
                            if let Some(rv) = term_var(right) {
                                let mut targets = match self.min_confidence {
                                    None => db.relations.targets(s, rel_id),
                                    Some(min) => {
                                        db.relations.targets_with_min_confidence(s, rel_id, min)
                                    }
                                };
                                targets &= candidates[rv].clone();
                                if targets.is_empty() {
                                    return Ok(false);
                                }
                            }
                        }
                        (None, Some(t)) => {
                            if let Some(lv) = term_var(left) {
                                let mut sources = match self.min_confidence {
                                    None => db.relations.sources(t, rel_id),
                                    Some(min) => {
                                        db.relations.sources_with_min_confidence(t, rel_id, min)
                                    }
                                };
                                sources &= candidates[lv].clone();
                                if sources.is_empty() {
                                    return Ok(false);
                                }
                            }
                        }
                        (None, None) => {}
                    }
                }
                LoweredAtom::Rpq {
                    left,
                    rpq_id,
                    right,
                } => {
                    let l = resolve_term(left, assigned);
                    let r = resolve_term(right, assigned);

                    match (l, r) {
                        (Some(s), Some(t)) => {
                            if !rpq.reachable(db, *rpq_id, s, t)? {
                                return Ok(false);
                            }
                        }
                        (Some(s), None) => {
                            if let Some(rv) = term_var(right) {
                                if !rpq.intersects_candidates(db, *rpq_id, s, &candidates[rv])? {
                                    return Ok(false);
                                }
                            }
                        }
                        (None, Some(t)) => {
                            if let Some(lv) = term_var(left) {
                                if !rpq.intersects_candidates_reverse(
                                    db,
                                    *rpq_id,
                                    t,
                                    &candidates[lv],
                                )? {
                                    return Ok(false);
                                }
                            }
                        }
                        (None, None) => {}
                    }
                }
            }
        }
        Ok(true)
    }
}

fn tuple_entity_type_name(
    schema: &axiograph_pathdb::axi_semantics::SchemaIndex,
    relation: &str,
) -> String {
    if schema.object_types.contains(relation) {
        format!("{relation}Fact")
    } else {
        relation.to_string()
    }
}

fn canonical_entity_type_for_axi_type(
    schema: &axiograph_pathdb::axi_semantics::SchemaIndex,
    axi_type: &str,
) -> Option<String> {
    if schema.object_types.contains(axi_type) {
        return Some(axi_type.to_string());
    }
    if schema.relation_decls.contains_key(axi_type) {
        return Some(tuple_entity_type_name(schema, axi_type));
    }
    None
}

fn term_var(t: &LoweredTerm) -> Option<usize> {
    match t {
        LoweredTerm::Var(v) => Some(*v),
        LoweredTerm::Const(_) => None,
    }
}

fn lowered_atom_vars(a: &LoweredAtom) -> Vec<usize> {
    let mut out = Vec::new();
    match a {
        LoweredAtom::Type { term, .. } => {
            if let Some(v) = term_var(term) {
                out.push(v);
            }
        }
        LoweredAtom::AttrEq { term, .. } => {
            if let Some(v) = term_var(term) {
                out.push(v);
            }
        }
        LoweredAtom::AttrContains { term, .. } => {
            if let Some(v) = term_var(term) {
                out.push(v);
            }
        }
        LoweredAtom::AttrFts { term, .. } => {
            if let Some(v) = term_var(term) {
                out.push(v);
            }
        }
        LoweredAtom::AttrFuzzy { term, .. } => {
            if let Some(v) = term_var(term) {
                out.push(v);
            }
        }
        LoweredAtom::Edge { left, right, .. } => {
            if let Some(v) = term_var(left) {
                out.push(v);
            }
            if let Some(v) = term_var(right) {
                out.push(v);
            }
        }
        LoweredAtom::Rpq { left, right, .. } => {
            if let Some(v) = term_var(left) {
                out.push(v);
            }
            if let Some(v) = term_var(right) {
                out.push(v);
            }
        }
    }
    out
}

fn for_each_value<F: FnMut(u32)>(term: &LoweredTerm, candidates: &[RoaringBitmap], mut f: F) {
    match term {
        LoweredTerm::Const(id) => f(*id),
        LoweredTerm::Var(v) => {
            for id in candidates[*v].iter() {
                f(id);
            }
        }
    }
}

fn resolve_term(t: &LoweredTerm, assigned: &[Option<u32>]) -> Option<u32> {
    match t {
        LoweredTerm::Const(id) => Some(*id),
        LoweredTerm::Var(v) => assigned[*v],
    }
}

fn resolve_term_assigned(t: &LoweredTerm, assigned: &[u32]) -> u32 {
    match t {
        LoweredTerm::Const(id) => *id,
        LoweredTerm::Var(v) => assigned[*v],
    }
}

fn fixed_prob_from_confidence(confidence: f32) -> FixedPointProbability {
    // Deterministic: defined over the exact IEEE754 bits, so Rust and Lean agree.
    FixedPointProbability::from_f32(confidence)
}

fn entity_is_type(db: &axiograph_pathdb::PathDB, entity_id: u32, type_name: &str) -> Result<bool> {
    let Some(bitmap) = db.find_by_type(type_name) else {
        return Ok(false);
    };
    Ok(bitmap.contains(entity_id))
}

fn type_bitmap_including_subtypes(
    db: &axiograph_pathdb::PathDB,
    type_name: &str,
    meta: Option<&MetaPlaneIndex>,
) -> RoaringBitmap {
    // If a meta-plane is present, interpret `?x : T` as:
    //   “x has type T *or any subtype of T* (in any imported schema)”.
    //
    // This is the expected meaning of subtyping in most query settings:
    // asking for `SmoothManifold` should include `Spacetime` / `PhaseSpace`, etc.
    //
    // If no meta-plane exists, fall back to exact type matching.
    let Some(meta) = meta else {
        return db.find_by_type(type_name).cloned().unwrap_or_default();
    };

    let mut candidate_types: HashSet<&str> = HashSet::new();
    for schema in meta.schemas.values() {
        for (ty, supers) in &schema.supertypes_of {
            if supers.contains(type_name) {
                candidate_types.insert(ty.as_str());
            }
        }
    }

    if candidate_types.is_empty() {
        candidate_types.insert(type_name);
    }

    let mut out = RoaringBitmap::new();
    for ty in candidate_types {
        if let Some(bm) = db.find_by_type(ty) {
            out |= bm.clone();
        }
    }
    out
}

fn entity_has_attr(
    db: &axiograph_pathdb::PathDB,
    entity_id: u32,
    key: &str,
    value: &str,
) -> Result<bool> {
    let Some(key_id) = db.interner.id_of(key) else {
        return Ok(false);
    };
    let Some(value_id) = db.interner.id_of(value) else {
        return Ok(false);
    };
    Ok(db.entities.get_attr(entity_id, key_id) == Some(value_id))
}

fn entity_attr_contains(
    db: &axiograph_pathdb::PathDB,
    entity_id: u32,
    key: &str,
    needle: &str,
) -> Result<bool> {
    let Some(key_id) = db.interner.id_of(key) else {
        return Ok(false);
    };

    let needle = needle.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return Ok(false);
    }

    let Some(value_id) = db.entities.get_attr(entity_id, key_id) else {
        return Ok(false);
    };
    let Some(value) = db.interner.lookup(value_id) else {
        return Ok(false);
    };

    Ok(value.to_ascii_lowercase().contains(&needle))
}

fn entity_attr_fts(
    db: &axiograph_pathdb::PathDB,
    entity_id: u32,
    key: &str,
    query: &str,
) -> Result<bool> {
    Ok(db.entities_with_attr_fts(key, query).contains(entity_id))
}

fn entity_attr_fuzzy(
    db: &axiograph_pathdb::PathDB,
    entity_id: u32,
    key: &str,
    needle: &str,
    max_dist: usize,
) -> Result<bool> {
    let Some(key_id) = db.interner.id_of(key) else {
        return Ok(false);
    };

    let needle = needle.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return Ok(false);
    }

    let Some(value_id) = db.entities.get_attr(entity_id, key_id) else {
        return Ok(false);
    };
    let Some(value) = db.interner.lookup(value_id) else {
        return Ok(false);
    };

    let max_dist = max_dist.min(16);
    let needle_chars: Vec<char> = needle.chars().collect();
    let value = value.to_ascii_lowercase();
    Ok(levenshtein_with_max(&value, &needle_chars, max_dist) <= max_dist)
}

fn levenshtein_with_max(value: &str, needle_chars: &[char], max_dist: usize) -> usize {
    if max_dist == 0 {
        return if value.chars().eq(needle_chars.iter().copied()) {
            0
        } else {
            1
        };
    }

    let n = needle_chars.len();
    if n == 0 {
        return 0;
    }

    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr: Vec<usize> = vec![0; n + 1];

    for (i, c) in value.chars().enumerate() {
        curr[0] = i + 1;
        let mut row_min = curr[0];

        for j in 1..=n {
            let cost = if c == needle_chars[j - 1] { 0 } else { 1 };
            let deletion = prev[j] + 1;
            let insertion = curr[j - 1] + 1;
            let substitution = prev[j - 1] + cost;
            let d = deletion.min(insertion).min(substitution);
            curr[j] = d;
            row_min = row_min.min(d);
        }

        if row_min > max_dist {
            return max_dist + 1;
        }

        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

fn edge_exists(
    db: &axiograph_pathdb::PathDB,
    source: u32,
    rel: &str,
    target: u32,
    min_confidence: Option<f32>,
) -> Result<bool> {
    let Some(rel_id) = db.interner.id_of(rel) else {
        return Ok(false);
    };
    Ok(match min_confidence {
        None => db.relations.has_edge(source, rel_id, target),
        Some(min) => db
            .relations
            .has_edge_with_min_confidence(source, rel_id, target, min),
    })
}

fn axql_regex_to_query_regex(
    db: &axiograph_pathdb::PathDB,
    regex: &AxqlRegex,
) -> Result<QueryRegexV1> {
    Ok(match regex {
        AxqlRegex::Epsilon => QueryRegexV1::Epsilon,
        AxqlRegex::Rel(name) => {
            let rel_type_id = db
                .interner
                .id_of(name)
                .ok_or_else(|| anyhow!("unknown relation `{name}` (missing from interner)"))?;
            QueryRegexV1::Rel {
                rel_type_id: rel_type_id.raw(),
            }
        }
        AxqlRegex::Seq(parts) => QueryRegexV1::Seq {
            parts: parts
                .iter()
                .map(|p| axql_regex_to_query_regex(db, p))
                .collect::<Result<Vec<_>>>()?,
        },
        AxqlRegex::Alt(parts) => QueryRegexV1::Alt {
            parts: parts
                .iter()
                .map(|p| axql_regex_to_query_regex(db, p))
                .collect::<Result<Vec<_>>>()?,
        },
        AxqlRegex::Star(inner) => QueryRegexV1::Star {
            inner: Box::new(axql_regex_to_query_regex(db, inner)?),
        },
        AxqlRegex::Plus(inner) => QueryRegexV1::Plus {
            inner: Box::new(axql_regex_to_query_regex(db, inner)?),
        },
        AxqlRegex::Opt(inner) => QueryRegexV1::Opt {
            inner: Box::new(axql_regex_to_query_regex(db, inner)?),
        },
    })
}

// =============================================================================
// RPQ (regular-path query) engine (evaluation + caching)
// =============================================================================

#[derive(Debug, Clone)]
struct RpqNfa {
    start: usize,
    accept: usize,
    epsilon: Vec<Vec<usize>>,
    labeled: Vec<Vec<(axiograph_pathdb::StrId, usize)>>,
}

impl RpqNfa {
    fn states(&self) -> usize {
        self.epsilon.len()
    }
}

#[derive(Debug, Clone)]
struct RpqProgram {
    nfa: RpqNfa,
    eps_closure: Vec<Vec<usize>>,
    accepting: Vec<bool>,
}

#[derive(Debug, Clone)]
struct CompiledRpq {
    forward: RpqProgram,
    reverse: RpqProgram,
    simple_chain: Option<Vec<String>>,
}

#[derive(Debug, Default)]
struct RpqCache {
    forward: HashMap<(u32, usize), RoaringBitmap>,
    reverse: HashMap<(u32, usize), RoaringBitmap>,
}

struct RpqContext {
    compiled: Vec<CompiledRpq>,
    cache: RpqCache,
    max_hops: Option<u32>,
    min_confidence: Option<f32>,
}

impl RpqContext {
    fn new(
        db: &axiograph_pathdb::PathDB,
        rpqs: &[AxqlRegex],
        max_hops: Option<u32>,
        min_confidence: Option<f32>,
    ) -> Result<Self> {
        let compiled = rpqs
            .iter()
            .map(|r| compile_rpq(db, r))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            compiled,
            cache: RpqCache::default(),
            max_hops,
            min_confidence,
        })
    }

    fn reachable(
        &mut self,
        db: &axiograph_pathdb::PathDB,
        rpq_id: usize,
        source: u32,
        target: u32,
    ) -> Result<bool> {
        if let Some(hit) = self.cache.forward.get(&(source, rpq_id)) {
            return Ok(hit.contains(target));
        }
        Ok(self.reachable_set(db, rpq_id, source)?.contains(target))
    }

    fn reachable_set(
        &mut self,
        db: &axiograph_pathdb::PathDB,
        rpq_id: usize,
        source: u32,
    ) -> Result<RoaringBitmap> {
        if let Some(hit) = self.cache.forward.get(&(source, rpq_id)) {
            return Ok(hit.clone());
        }
        let prog = self
            .compiled
            .get(rpq_id)
            .ok_or_else(|| anyhow!("invalid rpq_id {rpq_id}"))?;
        let set = if let Some(chain) = prog.simple_chain.as_ref() {
            follow_simple_chain_forward(db, chain, source, self.min_confidence)
        } else {
            eval_rpq_program_forward(
                db,
                &prog.forward,
                source,
                self.max_hops,
                self.min_confidence,
            )
        };
        self.cache.forward.insert((source, rpq_id), set.clone());
        Ok(set)
    }

    fn reachable_set_reverse(
        &mut self,
        db: &axiograph_pathdb::PathDB,
        rpq_id: usize,
        target: u32,
    ) -> Result<RoaringBitmap> {
        if let Some(hit) = self.cache.reverse.get(&(target, rpq_id)) {
            return Ok(hit.clone());
        }
        let prog = self
            .compiled
            .get(rpq_id)
            .ok_or_else(|| anyhow!("invalid rpq_id {rpq_id}"))?;
        let set = if let Some(chain) = prog.simple_chain.as_ref() {
            follow_simple_chain_reverse(db, chain, target, self.min_confidence)
        } else {
            eval_rpq_program_reverse(
                db,
                &prog.reverse,
                target,
                self.max_hops,
                self.min_confidence,
            )
        };
        self.cache.reverse.insert((target, rpq_id), set.clone());
        Ok(set)
    }

    fn intersects_candidates(
        &mut self,
        db: &axiograph_pathdb::PathDB,
        rpq_id: usize,
        source: u32,
        candidates: &RoaringBitmap,
    ) -> Result<bool> {
        if let Some(hit) = self.cache.forward.get(&(source, rpq_id)) {
            return Ok(hit.intersection_len(candidates) > 0);
        }
        let set = self.reachable_set(db, rpq_id, source)?;
        Ok(set.intersection_len(candidates) > 0)
    }

    fn intersects_candidates_reverse(
        &mut self,
        db: &axiograph_pathdb::PathDB,
        rpq_id: usize,
        target: u32,
        candidates: &RoaringBitmap,
    ) -> Result<bool> {
        if let Some(hit) = self.cache.reverse.get(&(target, rpq_id)) {
            return Ok(hit.intersection_len(candidates) > 0);
        }
        let set = self.reachable_set_reverse(db, rpq_id, target)?;
        Ok(set.intersection_len(candidates) > 0)
    }

    fn witness_relation_ids(
        &self,
        db: &axiograph_pathdb::PathDB,
        rpq_id: usize,
        source: u32,
        target: u32,
    ) -> Result<Option<Vec<u32>>> {
        let prog = self
            .compiled
            .get(rpq_id)
            .ok_or_else(|| anyhow!("invalid rpq_id {rpq_id}"))?;
        let program = &prog.forward;

        #[derive(Debug, Clone, Copy)]
        struct Prev {
            prev_node: u32,
            prev_state: usize,
            relation_id: u32,
        }

        let n_states = program.nfa.states();
        let mut visited: Vec<RoaringBitmap> = (0..n_states).map(|_| RoaringBitmap::new()).collect();
        let mut prev: HashMap<(u32, usize), Prev> = HashMap::new();
        let mut queue: std::collections::VecDeque<(u32, usize, u32)> =
            std::collections::VecDeque::new();

        // Seed with epsilon-closure of start state.
        for &st in &program.eps_closure[program.nfa.start] {
            visited[st].insert(source);
            queue.push_back((source, st, 0));
        }

        // Fast path: empty-path match.
        for &st in &program.eps_closure[program.nfa.start] {
            if source == target && program.accepting[st] {
                return Ok(Some(Vec::new()));
            }
        }

        let mut found: Option<(u32, usize)> = None;

        while let Some((node, st, depth)) = queue.pop_front() {
            if let Some(max) = self.max_hops {
                if depth >= max {
                    continue;
                }
            }

            for &(label, dest) in &program.nfa.labeled[st] {
                for &relation_id in db.relations.outgoing_relation_ids(node, label) {
                    let rel = db.relations.get_relation(relation_id).ok_or_else(|| {
                        anyhow!("internal error: missing relation {relation_id} in RelationStore")
                    })?;
                    if let Some(min) = self.min_confidence {
                        if rel.confidence < min {
                            continue;
                        }
                    }
                    let next_node = rel.target;
                    let next_depth = depth + 1;

                    for &st2 in &program.eps_closure[dest] {
                        if !visited[st2].contains(next_node) {
                            visited[st2].insert(next_node);
                            prev.insert(
                                (next_node, st2),
                                Prev {
                                    prev_node: node,
                                    prev_state: st,
                                    relation_id,
                                },
                            );
                            if next_node == target && program.accepting[st2] {
                                found = Some((next_node, st2));
                                break;
                            }
                            queue.push_back((next_node, st2, next_depth));
                        }
                    }
                    if found.is_some() {
                        break;
                    }
                }
                if found.is_some() {
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }

        let Some((mut node, mut st)) = found else {
            return Ok(None);
        };

        let mut rel_ids_rev: Vec<u32> = Vec::new();
        while let Some(p) = prev.get(&(node, st)).copied() {
            rel_ids_rev.push(p.relation_id);
            node = p.prev_node;
            st = p.prev_state;
        }

        if node != source {
            return Err(anyhow!(
                "internal error: RPQ witness reconstruction did not reach source (got node={node}, expected source={source})"
            ));
        }

        rel_ids_rev.reverse();
        Ok(Some(rel_ids_rev))
    }
}

fn follow_simple_chain_forward(
    db: &axiograph_pathdb::PathDB,
    chain: &[String],
    start: u32,
    min_confidence: Option<f32>,
) -> RoaringBitmap {
    const PATH_INDEX_MIN_LEN: usize = 3;
    if chain.is_empty() {
        let mut out = RoaringBitmap::new();
        out.insert(start);
        return out;
    }
    if chain.len() < PATH_INDEX_MIN_LEN {
        return follow_chain_direct(db, chain, start, min_confidence);
    }
    let refs: Vec<&str> = chain.iter().map(|s| s.as_str()).collect();
    match min_confidence {
        None => db.follow_path(start, &refs),
        Some(min) => db.follow_path_with_min_confidence(start, &refs, min),
    }
}

fn follow_simple_chain_reverse(
    db: &axiograph_pathdb::PathDB,
    chain: &[String],
    target: u32,
    min_confidence: Option<f32>,
) -> RoaringBitmap {
    let mut current = RoaringBitmap::new();
    current.insert(target);

    for rel in chain.iter().rev() {
        let Some(rel_id) = db.interner.id_of(rel) else {
            return RoaringBitmap::new();
        };
        let mut next = RoaringBitmap::new();
        for entity in current.iter() {
            next |= match min_confidence {
                None => db.relations.sources(entity, rel_id),
                Some(min) => db.relations.sources_with_min_confidence(entity, rel_id, min),
            };
        }
        current = next;
        if current.is_empty() {
            break;
        }
    }

    current
}

fn follow_chain_direct(
    db: &axiograph_pathdb::PathDB,
    chain: &[String],
    start: u32,
    min_confidence: Option<f32>,
) -> RoaringBitmap {
    let mut current = RoaringBitmap::new();
    current.insert(start);

    for rel in chain {
        let Some(rel_id) = db.interner.id_of(rel) else {
            return RoaringBitmap::new();
        };
        let mut next = RoaringBitmap::new();
        for entity in current.iter() {
            next |= match min_confidence {
                None => db.relations.targets(entity, rel_id),
                Some(min) => db.relations.targets_with_min_confidence(entity, rel_id, min),
            };
        }
        current = next;
        if current.is_empty() {
            break;
        }
    }

    current
}

fn compile_rpq(db: &axiograph_pathdb::PathDB, regex: &AxqlRegex) -> Result<CompiledRpq> {
    let simple_chain = simple_chain(regex);
    let forward_nfa = compile_regex_to_nfa(db, regex);
    let forward = build_program(forward_nfa);

    let reversed = reverse_regex(regex);
    let reverse_nfa = compile_regex_to_nfa(db, &reversed);
    let reverse = build_program(reverse_nfa);

    Ok(CompiledRpq {
        forward,
        reverse,
        simple_chain,
    })
}

fn reverse_regex(r: &AxqlRegex) -> AxqlRegex {
    match r {
        AxqlRegex::Epsilon => AxqlRegex::Epsilon,
        AxqlRegex::Rel(x) => AxqlRegex::Rel(x.clone()),
        AxqlRegex::Seq(parts) => AxqlRegex::Seq(parts.iter().rev().map(reverse_regex).collect()),
        AxqlRegex::Alt(parts) => AxqlRegex::Alt(parts.iter().map(reverse_regex).collect()),
        AxqlRegex::Star(inner) => AxqlRegex::Star(Box::new(reverse_regex(inner))),
        AxqlRegex::Plus(inner) => AxqlRegex::Plus(Box::new(reverse_regex(inner))),
        AxqlRegex::Opt(inner) => AxqlRegex::Opt(Box::new(reverse_regex(inner))),
    }
}

fn build_program(nfa: RpqNfa) -> RpqProgram {
    let eps_closure = compute_eps_closure(&nfa.epsilon);
    let mut accepting = vec![false; nfa.states()];
    for st in 0..nfa.states() {
        accepting[st] = eps_closure[st].binary_search(&nfa.accept).is_ok();
    }
    RpqProgram {
        nfa,
        eps_closure,
        accepting,
    }
}

fn compute_eps_closure(epsilon: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let n = epsilon.len();
    let mut out: Vec<Vec<usize>> = Vec::with_capacity(n);
    for i in 0..n {
        let mut seen = vec![false; n];
        let mut stack = vec![i];
        seen[i] = true;
        while let Some(s) = stack.pop() {
            for &t in &epsilon[s] {
                if !seen[t] {
                    seen[t] = true;
                    stack.push(t);
                }
            }
        }
        let mut closure = seen
            .iter()
            .enumerate()
            .filter_map(|(idx, ok)| ok.then_some(idx))
            .collect::<Vec<_>>();
        closure.sort_unstable();
        out.push(closure);
    }
    out
}

struct NfaBuilder {
    epsilon: Vec<Vec<usize>>,
    labeled: Vec<Vec<(axiograph_pathdb::StrId, usize)>>,
}

impl NfaBuilder {
    fn new() -> Self {
        Self {
            epsilon: Vec::new(),
            labeled: Vec::new(),
        }
    }

    fn new_state(&mut self) -> usize {
        let id = self.epsilon.len();
        self.epsilon.push(Vec::new());
        self.labeled.push(Vec::new());
        id
    }

    fn add_eps(&mut self, from: usize, to: usize) {
        self.epsilon[from].push(to);
    }

    fn add_labeled(&mut self, from: usize, label: axiograph_pathdb::StrId, to: usize) {
        self.labeled[from].push((label, to));
    }

    fn build(self, start: usize, accept: usize) -> RpqNfa {
        RpqNfa {
            start,
            accept,
            epsilon: self.epsilon,
            labeled: self.labeled,
        }
    }
}

fn compile_regex_to_nfa(db: &axiograph_pathdb::PathDB, r: &AxqlRegex) -> RpqNfa {
    let mut b = NfaBuilder::new();
    let (start, accept) = compile_regex_fragment(db, &mut b, r);
    b.build(start, accept)
}

fn compile_regex_fragment(
    db: &axiograph_pathdb::PathDB,
    b: &mut NfaBuilder,
    r: &AxqlRegex,
) -> (usize, usize) {
    match r {
        AxqlRegex::Epsilon => {
            let s = b.new_state();
            let t = b.new_state();
            b.add_eps(s, t);
            (s, t)
        }
        AxqlRegex::Rel(name) => {
            let s = b.new_state();
            let t = b.new_state();
            if let Some(label) = db.interner.id_of(name) {
                b.add_labeled(s, label, t);
            }
            (s, t)
        }
        AxqlRegex::Seq(parts) => {
            if parts.is_empty() {
                return compile_regex_fragment(db, b, &AxqlRegex::Epsilon);
            }
            let mut it = parts.iter();
            let (start, mut accept) = compile_regex_fragment(db, b, it.next().unwrap());
            for p in it {
                let (s2, a2) = compile_regex_fragment(db, b, p);
                b.add_eps(accept, s2);
                accept = a2;
            }
            (start, accept)
        }
        AxqlRegex::Alt(parts) => {
            let s = b.new_state();
            let t = b.new_state();
            if parts.is_empty() {
                b.add_eps(s, t);
                return (s, t);
            }
            for p in parts {
                let (ps, pa) = compile_regex_fragment(db, b, p);
                b.add_eps(s, ps);
                b.add_eps(pa, t);
            }
            (s, t)
        }
        AxqlRegex::Star(inner) => {
            let s = b.new_state();
            let t = b.new_state();
            let (is, ia) = compile_regex_fragment(db, b, inner);
            b.add_eps(s, t);
            b.add_eps(s, is);
            b.add_eps(ia, t);
            b.add_eps(ia, is);
            (s, t)
        }
        AxqlRegex::Plus(inner) => {
            let s = b.new_state();
            let t = b.new_state();
            let (is, ia) = compile_regex_fragment(db, b, inner);
            b.add_eps(s, is);
            b.add_eps(ia, t);
            b.add_eps(ia, is);
            (s, t)
        }
        AxqlRegex::Opt(inner) => {
            let s = b.new_state();
            let t = b.new_state();
            let (is, ia) = compile_regex_fragment(db, b, inner);
            b.add_eps(s, t);
            b.add_eps(s, is);
            b.add_eps(ia, t);
            (s, t)
        }
    }
}

fn eval_rpq_program_forward(
    db: &axiograph_pathdb::PathDB,
    program: &RpqProgram,
    start_node: u32,
    max_hops: Option<u32>,
    min_confidence: Option<f32>,
) -> RoaringBitmap {
    eval_rpq_program_impl(
        db,
        program,
        start_node,
        max_hops,
        min_confidence,
        Direction::Forward,
    )
}

fn eval_rpq_program_reverse(
    db: &axiograph_pathdb::PathDB,
    program: &RpqProgram,
    target_node: u32,
    max_hops: Option<u32>,
    min_confidence: Option<f32>,
) -> RoaringBitmap {
    eval_rpq_program_impl(
        db,
        program,
        target_node,
        max_hops,
        min_confidence,
        Direction::Reverse,
    )
}

#[derive(Debug, Clone, Copy)]
enum Direction {
    Forward,
    Reverse,
}

fn eval_rpq_program_impl(
    db: &axiograph_pathdb::PathDB,
    program: &RpqProgram,
    start_node: u32,
    max_hops: Option<u32>,
    min_confidence: Option<f32>,
    dir: Direction,
) -> RoaringBitmap {
    let n_states = program.nfa.states();
    let mut visited: Vec<RoaringBitmap> = (0..n_states).map(|_| RoaringBitmap::new()).collect();
    let mut queue: std::collections::VecDeque<(u32, usize, u32)> =
        std::collections::VecDeque::new();

    // Seed with epsilon-closure of start state.
    for &st in &program.eps_closure[program.nfa.start] {
        visited[st].insert(start_node);
        queue.push_back((start_node, st, 0));
    }

    let mut results = RoaringBitmap::new();

    while let Some((node, st, depth)) = queue.pop_front() {
        if program.accepting[st] {
            results.insert(node);
        }
        if let Some(max) = max_hops {
            if depth >= max {
                continue;
            }
        }

        for &(label, dest) in &program.nfa.labeled[st] {
            let next_nodes = match dir {
                Direction::Forward => match min_confidence {
                    None => db.relations.targets(node, label),
                    Some(min) => db.relations.targets_with_min_confidence(node, label, min),
                },
                Direction::Reverse => match min_confidence {
                    None => db.relations.sources(node, label),
                    Some(min) => db.relations.sources_with_min_confidence(node, label, min),
                },
            };
            for next_node in next_nodes.iter() {
                let next_depth = depth + 1;
                for &st2 in &program.eps_closure[dest] {
                    if !visited[st2].contains(next_node) {
                        visited[st2].insert(next_node);
                        queue.push_back((next_node, st2, next_depth));
                    }
                }
            }
        }
    }

    results
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use axiograph_pathdb::certificate::CertificatePayloadV2;

    fn db_with_axi_meta_plane() -> axiograph_pathdb::PathDB {
        let mut db = axiograph_pathdb::PathDB::new();
        let axi = r#"
module Demo

schema Demo:
  object Node
  object Supplier
  subtype Supplier < Node

  relation Flow(from: Supplier, to: Supplier)

instance DemoInst of Demo:
  Supplier = {a, b}
  Flow = {(from=a, to=b)}
"#;
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)
            .expect("import demo axi module");
        db.build_indexes();
        db
    }

    fn tiny_db() -> axiograph_pathdb::PathDB {
        let mut db = axiograph_pathdb::PathDB::new();
        let a = db.add_entity("Node", vec![("name", "a")]);
        let b = db.add_entity("Node", vec![("name", "b")]);
        let c = db.add_entity("Node", vec![("name", "c")]);
        let _ = db.add_relation("rel_0", a, b, 0.9, Vec::new());
        let _ = db.add_relation("rel_1", b, c, 0.9, Vec::new());
        db.build_indexes();
        db
    }

    fn tiny_db_with_fact_tuple() -> axiograph_pathdb::PathDB {
        let mut db = axiograph_pathdb::PathDB::new();
        let a = db.add_entity("Node", vec![("name", "a")]);
        let b = db.add_entity("Node", vec![("name", "b")]);

        // A "fact node" (tuple) like those produced by canonical `.axi` import.
        let f = db.add_entity(
            "Fact",
            vec![
                ("name", "flow_0"),
                (axiograph_pathdb::axi_meta::ATTR_AXI_RELATION, "Flow"),
            ],
        );
        let _ = db.add_relation("from", f, a, 1.0, Vec::new());
        let _ = db.add_relation("to", f, b, 1.0, Vec::new());

        db.build_indexes();
        db
    }

    fn materials_db() -> axiograph_pathdb::PathDB {
        let mut db = axiograph_pathdb::PathDB::new();
        let _ = db.add_entity("Material", vec![("name", "titanium")]);
        let _ = db.add_entity("Material", vec![("name", "steel")]);
        db.build_indexes();
        db
    }

    #[test]
    fn axql_elaboration_infers_field_types_and_supertypes() -> Result<()> {
        let db = db_with_axi_meta_plane();
        let meta = MetaPlaneIndex::from_db(&db)?;

        let q = parse_axql_query(r#"select ?dst where ?f = Flow(from=a, to=?dst) limit 5"#)?;
        let prepared = prepare_axql_query_with_meta(&db, &q, Some(&meta))?;
        let report = prepared.elaboration_report();

        let inferred = report
            .inferred_types
            .get("?dst")
            .cloned()
            .unwrap_or_default();
        assert!(inferred.contains(&"Supplier".to_string()));
        assert!(inferred.contains(&"Node".to_string()));
        Ok(())
    }

    #[test]
    fn axql_elaboration_errors_on_unknown_fact_field() {
        let db = db_with_axi_meta_plane();
        let meta = MetaPlaneIndex::from_db(&db).expect("meta");

        let q = parse_axql_query(r#"select ?x where Flow(foo=a, to=?x)"#).expect("parse");
        let result = prepare_axql_query_with_meta(&db, &q, Some(&meta));
        let err = match result {
            Ok(_) => panic!("expected error"),
            Err(e) => e,
        };
        assert!(err
            .to_string()
            .contains("field `foo` not in relation `Flow`"));
    }

    #[test]
    fn axql_elaboration_errors_on_unknown_type() {
        let db = db_with_axi_meta_plane();
        let q = parse_axql_query(r#"select ?x where ?x : Ndoe"#).expect("parse");
        let err = execute_axql_query(&db, &q).unwrap_err();
        assert!(err.to_string().contains("unknown type `Ndoe`"));
    }

    #[test]
    fn parse_select_where_limit() -> Result<()> {
        let q = parse_axql_query(r#"select ?x ?y where ?x : Node, ?x -rel_0-> ?y limit 10"#)?;
        assert_eq!(q.select_vars, vec!["?x".to_string(), "?y".to_string()]);
        assert_eq!(q.limit, 10);
        Ok(())
    }

    #[test]
    fn parse_select_star_is_allowed() -> Result<()> {
        let q = parse_axql_query(r#"select * where ?x : Node, ?x.name = "a" limit 5"#)?;
        assert_eq!(q.select_vars, Vec::<String>::new());
        assert_eq!(q.limit, 5);
        Ok(())
    }

    #[test]
    fn parse_attr_atom() -> Result<()> {
        let q = parse_axql_query(r#"where attr(?x, "name", "a"), ?x : Node"#)?;
        assert_eq!(q.limit, 20);
        assert_eq!(q.disjuncts.len(), 1);
        assert_eq!(q.disjuncts[0].len(), 2);
        Ok(())
    }

    #[test]
    fn parse_or_creates_multiple_disjuncts() -> Result<()> {
        let q = parse_axql_query(r#"select ?x where ?x : Node or ?x : Material limit 10"#)?;
        assert_eq!(q.select_vars, vec!["?x".to_string()]);
        assert_eq!(q.limit, 10);
        assert_eq!(q.disjuncts.len(), 2);
        assert_eq!(q.disjuncts[0].len(), 1);
        assert_eq!(q.disjuncts[1].len(), 1);
        Ok(())
    }

    #[test]
    fn query_path_seq_lowers_to_rpq() -> Result<()> {
        let q = parse_axql_query(r#"select ?y where 0 -rel_0/rel_1-> ?y"#)?;
        let lowered = lower_query_disjunct(&q, q.disjuncts.first().expect("disjunct"))?;
        let edge_count = lowered
            .atoms
            .iter()
            .filter(|a| matches!(a, LoweredAtom::Edge { .. }))
            .count();
        let rpq_count = lowered
            .atoms
            .iter()
            .filter(|a| matches!(a, LoweredAtom::Rpq { .. }))
            .count();
        assert_eq!(edge_count, 0);
        assert_eq!(rpq_count, 1);
        assert_eq!(lowered.rpqs.len(), 1);
        Ok(())
    }

    #[test]
    fn query_finds_expected_binding() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?y where 0 -rel_0/rel_1-> ?y"#)?;
        let res = execute_axql_query(&db, &q)?;
        assert_eq!(res.rows.len(), 1);
        assert_eq!(res.rows[0].get("?y").copied(), Some(2));
        Ok(())
    }

    #[test]
    fn query_or_returns_union_of_rows() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?y where 0 -rel_0-> ?y or 1 -rel_1-> ?y limit 10"#)?;
        let res = execute_axql_query(&db, &q)?;
        let ys: Vec<u32> = res
            .rows
            .iter()
            .filter_map(|row| row.get("?y").copied())
            .collect();
        assert_eq!(ys, vec![1, 2]);
        Ok(())
    }

    #[test]
    fn certify_or_emits_query_result_v2() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?y where 0 -rel_0-> ?y or 1 -rel_1-> ?y limit 10"#)?;
        let cert = certify_axql_query(&db, &q)?;
        let proof = match cert.payload {
            CertificatePayloadV2::QueryResultV2 { proof } => proof,
            other => return Err(anyhow!("expected query_result_v2, got {other:?}")),
        };
        assert_eq!(proof.rows.len(), 2);
        assert!(proof.rows.iter().any(|r| r.disjunct == 0));
        assert!(proof.rows.iter().any(|r| r.disjunct == 1));
        Ok(())
    }

    #[test]
    fn query_attr_filters() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?x where ?x : Node, attr(?x, "name", "b")"#)?;
        let res = execute_axql_query(&db, &q)?;
        assert_eq!(res.rows.len(), 1);
        assert_eq!(res.rows[0].get("?x").copied(), Some(1));
        Ok(())
    }

    #[test]
    fn query_rel_star_includes_reflexive() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?y where 0 -rel_0*-> ?y limit 10"#)?;
        let res = execute_axql_query(&db, &q)?;
        let ys: Vec<u32> = res
            .rows
            .iter()
            .filter_map(|row| row.get("?y").copied())
            .collect();
        assert_eq!(ys, vec![0, 1]);
        Ok(())
    }

    #[test]
    fn query_rel_plus_excludes_reflexive_without_cycle() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?y where 0 -rel_0+-> ?y limit 10"#)?;
        let res = execute_axql_query(&db, &q)?;
        let ys: Vec<u32> = res
            .rows
            .iter()
            .filter_map(|row| row.get("?y").copied())
            .collect();
        assert_eq!(ys, vec![1]);
        Ok(())
    }

    #[test]
    fn query_rel_plus_allows_cycle() -> Result<()> {
        let mut db = axiograph_pathdb::PathDB::new();
        let a = db.add_entity("Node", vec![("name", "a")]);
        let b = db.add_entity("Node", vec![("name", "b")]);
        let _ = db.add_relation("rel_0", a, a, 0.9, Vec::new()); // self-loop
        let _ = db.add_relation("rel_0", a, b, 0.9, Vec::new());
        db.build_indexes();

        let q = parse_axql_query(r#"select ?y where 0 -rel_0+-> ?y limit 10"#)?;
        let res = execute_axql_query(&db, &q)?;
        let ys: Vec<u32> = res
            .rows
            .iter()
            .filter_map(|row| row.get("?y").copied())
            .collect();
        assert_eq!(ys, vec![0, 1]);
        Ok(())
    }

    #[test]
    fn shape_macro_has_out_expands_to_edges() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?x where ?x : Node, has(?x, rel_0)"#)?;
        let res = execute_axql_query(&db, &q)?;
        assert_eq!(res.rows.len(), 1);
        assert_eq!(res.rows[0].get("?x").copied(), Some(0));
        Ok(())
    }

    #[test]
    fn shape_macro_attrs_expands_to_attr_constraints() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?x where ?x : Node, attrs(?x, name="c")"#)?;
        let res = execute_axql_query(&db, &q)?;
        assert_eq!(res.rows.len(), 1);
        assert_eq!(res.rows[0].get("?x").copied(), Some(2));
        Ok(())
    }

    #[test]
    fn lookup_term_name_resolves_by_attr() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?x where ?x -rel_0-> name("b")"#)?;
        let res = execute_axql_query(&db, &q)?;
        assert_eq!(res.rows.len(), 1);
        assert_eq!(res.rows[0].get("?x").copied(), Some(0));
        Ok(())
    }

    #[test]
    fn rpq_alternation_works() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?y where 0 -(rel_0|rel_1)-> ?y limit 10"#)?;
        let res = execute_axql_query(&db, &q)?;
        let ys: Vec<u32> = res
            .rows
            .iter()
            .filter_map(|row| row.get("?y").copied())
            .collect();
        assert_eq!(ys, vec![1]);
        Ok(())
    }

    #[test]
    fn rpq_optional_allows_empty_path() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?y where 0 -rel_0?-> ?y limit 10"#)?;
        let res = execute_axql_query(&db, &q)?;
        let ys: Vec<u32> = res
            .rows
            .iter()
            .filter_map(|row| row.get("?y").copied())
            .collect();
        assert_eq!(ys, vec![0, 1]);
        Ok(())
    }

    #[test]
    fn rpq_grouping_and_plus_works() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?y where 0 -((rel_0/rel_1)+)-> ?y limit 10"#)?;
        let res = execute_axql_query(&db, &q)?;
        let ys: Vec<u32> = res
            .rows
            .iter()
            .filter_map(|row| row.get("?y").copied())
            .collect();
        assert_eq!(ys, vec![2]);
        Ok(())
    }

    #[test]
    fn max_hops_bounds_path_search() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?y where 0 -rel_0*-> ?y max_hops 0 limit 10"#)?;
        let res = execute_axql_query(&db, &q)?;
        let ys: Vec<u32> = res
            .rows
            .iter()
            .filter_map(|row| row.get("?y").copied())
            .collect();
        assert_eq!(ys, vec![0]);
        Ok(())
    }

    #[test]
    fn attr_dot_syntax_parses_and_executes() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?x where ?x : Node, ?x.name = "b""#)?;
        let res = execute_axql_query(&db, &q)?;
        assert_eq!(res.rows.len(), 1);
        assert_eq!(res.rows[0].get("?x").copied(), Some(1));
        Ok(())
    }

    #[test]
    fn type_infix_is_parses() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?x where ?x is Node, ?x.name = "a""#)?;
        let res = execute_axql_query(&db, &q)?;
        assert_eq!(res.rows.len(), 1);
        assert_eq!(res.rows[0].get("?x").copied(), Some(0));
        Ok(())
    }

    #[test]
    fn has_infix_parses_and_executes() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?x where ?x is Node, ?x has rel_0"#)?;
        let res = execute_axql_query(&db, &q)?;
        assert_eq!(res.rows.len(), 1);
        assert_eq!(res.rows[0].get("?x").copied(), Some(0));
        Ok(())
    }

    #[test]
    fn bracketed_path_syntax_parses() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?y where 0 -[rel_0/rel_1]-> ?y"#)?;
        let res = execute_axql_query(&db, &q)?;
        assert_eq!(res.rows.len(), 1);
        assert_eq!(res.rows[0].get("?y").copied(), Some(2));
        Ok(())
    }

    #[test]
    fn single_quoted_strings_work() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?x where ?x -rel_0-> name('b')"#)?;
        let res = execute_axql_query(&db, &q)?;
        assert_eq!(res.rows.len(), 1);
        assert_eq!(res.rows[0].get("?x").copied(), Some(0));
        Ok(())
    }

    #[test]
    fn shape_literal_parses_and_executes() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?x where ?x { is Node, name="b" }"#)?;
        let res = execute_axql_query(&db, &q)?;
        assert_eq!(res.rows.len(), 1);
        assert_eq!(res.rows[0].get("?x").copied(), Some(1));
        Ok(())
    }

    #[test]
    fn shape_literal_rel_sugar_parses_and_executes() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?x where ?x { : Node, rel_0 }"#)?;
        let res = execute_axql_query(&db, &q)?;
        assert_eq!(res.rows.len(), 1);
        assert_eq!(res.rows[0].get("?x").copied(), Some(0));
        Ok(())
    }

    #[test]
    fn bare_identifier_terms_parse_as_name_lookup() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?y where a -rel_0-> ?y"#)?;
        let res = execute_axql_query(&db, &q)?;
        assert_eq!(res.rows.len(), 1);
        assert_eq!(res.rows[0].get("?y").copied(), Some(1));
        Ok(())
    }

    #[test]
    fn fact_atom_binds_tuple_entity() -> Result<()> {
        let db = tiny_db_with_fact_tuple();
        let q = parse_axql_query(r#"select ?f where ?f = Flow(from=a, to=b)"#)?;
        let res = execute_axql_query(&db, &q)?;
        assert_eq!(res.rows.len(), 1);

        // The only fact node we created is entity id 2 (after a=0, b=1).
        assert_eq!(res.rows[0].get("?f").copied(), Some(2));
        Ok(())
    }

    #[test]
    fn fact_atom_without_binder_joins_fields() -> Result<()> {
        let db = tiny_db_with_fact_tuple();
        let q = parse_axql_query(r#"select ?x where Flow(from=?x, to=b)"#)?;
        let res = execute_axql_query(&db, &q)?;
        assert_eq!(res.rows.len(), 1);
        assert_eq!(res.rows[0].get("?x").copied(), Some(0));
        Ok(())
    }

    #[test]
    fn query_contains_filters() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?x where ?x : Node, contains(?x, "name", "B")"#)?;
        let res = execute_axql_query(&db, &q)?;
        assert_eq!(res.rows.len(), 1);
        assert_eq!(res.rows[0].get("?x").copied(), Some(1));
        Ok(())
    }

    #[test]
    fn query_fuzzy_filters() -> Result<()> {
        let db = materials_db();
        let q =
            parse_axql_query(r#"select ?x where ?x : Material, fuzzy(?x, "name", "titainum", 2)"#)?;
        let res = execute_axql_query(&db, &q)?;
        assert_eq!(res.rows.len(), 1);
        assert_eq!(res.rows[0].get("?x").copied(), Some(0));
        Ok(())
    }

    #[test]
    fn certify_rejects_approximate_atoms() -> Result<()> {
        let db = tiny_db();
        let q = parse_axql_query(r#"select ?x where ?x : Node, contains(?x, "name", "a")"#)?;
        let err = certify_axql_query(&db, &q).expect_err("expected certification to fail");
        assert!(err.to_string().contains("cannot certify"));
        Ok(())
    }

    #[test]
    fn certify_emits_relation_id_for_edge_atoms() -> Result<()> {
        let mut db = axiograph_pathdb::PathDB::new();
        let a = db.add_entity("Node", vec![("name", "a")]);
        let b = db.add_entity("Node", vec![("name", "b")]);
        let rel_id = db.add_relation("rel_0", a, b, 0.9, Vec::new());
        db.build_indexes();

        let q = parse_axql_query(r#"select ?y where 0 -rel_0-> ?y"#)?;
        let cert = certify_axql_query(&db, &q)?;

        let axiograph_pathdb::certificate::CertificatePayloadV2::QueryResultV1 { proof } =
            cert.payload
        else {
            return Err(anyhow!("expected query_result_v1 certificate"));
        };

        assert_eq!(proof.rows.len(), 1);
        let row = &proof.rows[0];

        assert!(row.bindings.iter().any(|b| b.var == "?y" && b.entity == 1));
        assert_eq!(row.witnesses.len(), 1);

        match &row.witnesses[0] {
            QueryAtomWitnessV1::Path {
                proof:
                    ReachabilityProofV2::Step {
                        relation_id: Some(rid),
                        ..
                    },
            } => assert_eq!(*rid, rel_id),
            other => return Err(anyhow!("unexpected witness shape: {other:?}")),
        }

        Ok(())
    }

    #[test]
    fn certify_emits_relation_ids_for_path_seq_rpq() -> Result<()> {
        let mut db = axiograph_pathdb::PathDB::new();
        let a = db.add_entity("Node", vec![("name", "a")]);
        let b = db.add_entity("Node", vec![("name", "b")]);
        let c = db.add_entity("Node", vec![("name", "c")]);
        let rel_0_id = db.add_relation("rel_0", a, b, 0.9, Vec::new());
        let rel_1_id = db.add_relation("rel_1", b, c, 0.9, Vec::new());
        db.build_indexes();

        let q = parse_axql_query(r#"select ?y where 0 -rel_0/rel_1-> ?y"#)?;
        let cert = certify_axql_query(&db, &q)?;

        let axiograph_pathdb::certificate::CertificatePayloadV2::QueryResultV1 { proof } =
            cert.payload
        else {
            return Err(anyhow!("expected query_result_v1 certificate"));
        };

        assert_eq!(proof.rows.len(), 1);
        let row = &proof.rows[0];

        assert!(row.bindings.iter().any(|b| b.var == "?y" && b.entity == 2));

        assert_eq!(row.witnesses.len(), 1);

        let rid_chain = match &row.witnesses[0] {
            QueryAtomWitnessV1::Path {
                proof,
            } => {
                let mut ids: Vec<u32> = Vec::new();
                let mut cur = proof;
                loop {
                    match cur {
                        ReachabilityProofV2::Reflexive { .. } => break,
                        ReachabilityProofV2::Step {
                            relation_id: Some(rid),
                            rest,
                            ..
                        } => {
                            ids.push(*rid);
                            cur = rest;
                        }
                        ReachabilityProofV2::Step {
                            relation_id: None,
                            ..
                        } => return Err(anyhow!("expected relation_id in reachability proof")),
                    }
                }
                ids
            }
            other => return Err(anyhow!("unexpected witness[0] shape: {other:?}")),
        };

        assert_eq!(rid_chain, vec![rel_0_id, rel_1_id]);
        Ok(())
    }

    #[test]
    fn query_min_confidence_filters_low_conf_edges() -> Result<()> {
        let mut db = axiograph_pathdb::PathDB::new();
        let a = db.add_entity("Node", vec![("name", "a")]);
        let b = db.add_entity("Node", vec![("name", "b")]);
        let c = db.add_entity("Node", vec![("name", "c")]);
        let _ = db.add_relation("rel_0", a, b, 0.9, Vec::new());
        let _ = db.add_relation("rel_1", b, c, 0.3, Vec::new()); // below threshold
        db.build_indexes();

        let q = parse_axql_query(r#"select ?y where 0 -rel_0/rel_1-> ?y min_conf 0.5 limit 10"#)?;
        let res = execute_axql_query(&db, &q)?;
        assert_eq!(res.rows.len(), 0);

        let cert = certify_axql_query(&db, &q)?;
        match cert.payload {
            CertificatePayloadV2::QueryResultV1 { proof } => {
                assert!(proof.query.min_confidence_fp.is_some());
                assert_eq!(proof.rows.len(), 0);
            }
            other => panic!("expected query_result_v1 certificate, got {other:?}"),
        }

        Ok(())
    }
}
