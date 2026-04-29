//! Typed query IR (JSON) for tooling/LLMs.
//!
//! Motivation:
//! - LLMs are good at producing *structured* JSON, but often produce invalid
//!   AxQL text (small syntax errors, wrong sugar forms, etc).
//! - A typed JSON IR lets us validate and compile into the same AxQL core,
//!   avoiding brittle parsing and enabling better error messages.
//!
//! Current scope:
//! - `query_ir_v1` is the active machine-facing query surface for LLM/server
//!   integration.
//! - We keep the IR minimal and compile into the existing AxQL core.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::marker::PhantomData;

use crate::axql::PreparedQueryHandle;
use crate::axql::{
    parse_axql_path_expr, parse_axql_query, AxqlAtom, AxqlContextSpec, AxqlElaborationReport,
    AxqlQuery, AxqlRefinementApplicationScopeV1, AxqlRefinementHandleV1, AxqlRefinementOpV1,
    AxqlRefinementTermV1, AxqlResult, AxqlTerm, PreparedQueryIntrospection, QueryCertifiability,
};
use crate::trust_contract::{
    query_user_visible_trust_contract, query_user_visible_trust_contract_with_meta,
    QueryTrustContractV1,
};

use axiograph_pathdb::certificate::CertificateV2;
use axiograph_pathdb::kernel_ir::{
    CompiledSchemaIr, KernelModuleIr, KernelRefV1, SchemaCategoryArrowRefIr,
    SchemaCategoryObjectRefIr, TheoryIr, TheoryObligationRefIr, TheorySubjectRefIr,
};
use axiograph_pathdb::{AcceptedAxiAnchor, AxiDigest, Certified, LifecycleState, Validated};

pub const QUERY_IR_V1_VERSION: u32 = 1;
pub const PREPARED_QUERY_METADATA_V1_VERSION: u32 = 1;

pub type QueryTrustContract = QueryTrustContractV1;

/// Shared query certificate policy for HTTP, CQ, and agent/server surfaces.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum QueryCertificatePolicyV1 {
    #[default]
    None,
    Emit,
    Verify,
    RequireVerified,
}

impl QueryCertificatePolicyV1 {
    pub const fn emits_certificate(self) -> bool {
        !matches!(self, Self::None)
    }

    pub const fn verifies_certificate(self) -> bool {
        matches!(self, Self::Verify | Self::RequireVerified)
    }

    pub const fn requires_verified(self) -> bool {
        matches!(self, Self::RequireVerified)
    }

    pub fn ensure_require_verified_preconditions(
        self,
        certifiability: &QueryCertifiability,
        accepted_axi_anchor: Option<&AcceptedAxiAnchor>,
        canonical_axi_text: Option<&str>,
    ) -> Result<()> {
        if !self.requires_verified() {
            return Ok(());
        }

        let mut failures = Vec::new();
        if !certifiability.is_certifiable() {
            let reasons = certifiability.reasons();
            if reasons.is_empty() {
                failures.push(format!(
                    "query is not fully certifiable (trust_class={})",
                    certifiability.trust_class()
                ));
            } else {
                failures.push(format!(
                    "query is not fully certifiable (trust_class={}; reasons={})",
                    certifiability.trust_class(),
                    reasons.join(", ")
                ));
            }
        }
        if accepted_axi_anchor.is_none() {
            failures.push("missing accepted `.axi` anchor".to_string());
        }
        if canonical_axi_text
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .is_none()
        {
            failures.push("missing canonical `.axi` text for accepted anchor".to_string());
        }

        if failures.is_empty() {
            Ok(())
        } else {
            Err(anyhow!(
                "query certificate policy `require_verified` cannot be satisfied: {}",
                failures.join("; ")
            ))
        }
    }

    pub fn ensure_verified_result(self, certificate_verified: Option<bool>) -> Result<()> {
        if !self.requires_verified() {
            return Ok(());
        }
        match certificate_verified {
            Some(true) => Ok(()),
            Some(false) => Err(anyhow!(
                "query certificate policy `require_verified` cannot be satisfied: certificate verification failed"
            )),
            None => Err(anyhow!(
                "query certificate policy `require_verified` cannot be satisfied: certificate verification status missing"
            )),
        }
    }
}

/// Explicit query trust non-claims carried with prepared-query metadata.
///
/// This keeps answer-set completeness and ontology-closure boundaries visible
/// anywhere a prepared query handle is cited, including CQ and refinement
/// reports that should not have to reconstruct these fields from prose notes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryNonClaimsV1 {
    pub claim_scope: String,
    pub completeness_claim: String,
    pub ontology_closure_claim: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl QueryNonClaimsV1 {
    fn from_trust(trust: &QueryTrustContract) -> Self {
        Self {
            claim_scope: trust.claim_scope.clone(),
            completeness_claim: trust.completeness_claim.clone(),
            ontology_closure_claim: trust.ontology_closure_claim.clone(),
            notes: trust.notes.clone(),
        }
    }
}

/// Stable, serializable handle metadata for a prepared `query_ir_v1` query.
///
/// This is the small report envelope that CQ, refinement, agent, and review
/// surfaces can cite instead of passing only raw AxQL strings around.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreparedQueryMetadataV1 {
    pub version: u32,
    pub prepared_query_id: String,
    pub query_ir_id: String,
    pub elaborated_query_ir_id: String,
    pub introspection: PreparedQueryIntrospection,
    pub inferred_types: BTreeMap<String, Vec<String>>,
    pub certifiability: QueryCertifiability,
    pub trust: QueryTrustContract,
    pub non_claims: QueryNonClaimsV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refinement_handles: Vec<crate::typed_refinement::RuntimeRefinementHandleV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kernel_refs: Vec<KernelRefV1>,
}

/// Focused exploration payload for editor/agent workflows.
///
/// This packages the runtime elaborator's "what can go here next?" view over a
/// prepared query without implying execution completeness or ontology closure.
///
/// The key operational contract is that typed holes remain human-readable, while
/// `exploration_suggestions[*].refinement_candidates[*].handle` is the
/// machine-applicable refinement payload that editors/agents can feed back into
/// the typed apply/refine helpers below.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreparedQueryExplorationV1 {
    pub introspection: PreparedQueryIntrospection,
    pub inferred_types: BTreeMap<String, Vec<String>>,
    pub notes: Vec<String>,
    pub typed_holes: Vec<crate::axql::AxqlTypedHoleV1>,
    pub exploration_suggestions: Vec<crate::axql::AxqlExplorationSuggestionV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refinement_candidates: Vec<crate::typed_refinement::RuntimeRefinementCandidateV1>,
    pub semantic_claims: Vec<crate::trust_contract::SemanticClaimSummaryV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_coverage: Option<crate::trust_contract::SemanticCoverageSummaryV1>,
    pub trust_gaps: Vec<crate::trust_contract::TrustGapV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRefinementApplyResultV1 {
    pub handle: AxqlRefinementHandleV1,
    pub base_prepared_query: PreparedQueryMetadataV1,
    pub refined_prepared_query: PreparedQueryMetadataV1,
    pub base_query_ir_v1: QueryIrV1,
    pub refined_query_ir_v1: QueryIrV1,
    pub refined_elaborated_query_ir_v1: QueryIrV1,
    pub trust_before: QueryTrustContract,
    pub trust_after: QueryTrustContract,
    pub introspection_before: PreparedQueryIntrospection,
    pub introspection_after: PreparedQueryIntrospection,
    pub refined_exploration: PreparedQueryExplorationV1,
}

/// JSON schema for `QueryIrV1` (for tooling/LLMs).
///
/// This is intentionally hand-written and conservative:
/// - it documents the IR shape in a machine-readable way,
/// - it is used by the LLM tool-loop to strongly bias models toward producing
///   `query_ir_v1` rather than brittle AxQL text,
/// - and it reflects the current machine-facing query contract.
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
    /// Compile, typecheck, and prepare this query against the given database.
    ///
    /// The returned value is a typed execution handle for this exact IR instance:
    /// it owns both the parsed `AxqlQuery` and the prepared execution plan.
    #[allow(dead_code)]
    pub fn prepare_with_meta(
        &self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
    ) -> Result<PreparedQueryV1> {
        let query = self.to_axql_query()?;
        let handle = crate::axql::prepare_axql_query_with_meta(db, &query, meta)?;
        let trust = query_user_visible_trust_contract_with_meta(
            &query,
            &handle.certifiability(),
            false,
            None,
            meta,
        );
        Ok(PreparedQueryV1 {
            query_ir: QueryIrV1::from_axql_query(&query),
            query,
            handle,
            trust,
        })
    }

    /// Compile, typecheck, and prepare this query with prepared-handle reuse.
    ///
    /// This keeps the typed `PreparedQueryV1` boundary while allowing callers
    /// that already maintain an AxQL prepared-query cache to preserve reuse.
    #[allow(dead_code)]
    pub fn prepare_with_meta_cached(
        &self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
        snapshot_key: &str,
        cache: &mut crate::axql::AxqlPreparedQueryCache,
    ) -> Result<PreparedQueryV1> {
        let query = self.to_axql_query()?;
        let handle = crate::axql::get_or_prepare_axql_query_handle_mut(
            db,
            &query,
            meta,
            snapshot_key,
            cache,
        )?
        .clone();
        let trust = query_user_visible_trust_contract_with_meta(
            &query,
            &handle.certifiability(),
            false,
            None,
            meta,
        );
        Ok(PreparedQueryV1 {
            query_ir: QueryIrV1::from_axql_query(&query),
            query,
            handle,
            trust,
        })
    }

    /// Classify whether this query can be executed in the current certified subset.
    ///
    /// Returns a parsing or validation error only if the IR itself is invalid for
    /// this compilation path (e.g. bad version or malformed shape).
    #[allow(dead_code)]
    pub fn certifiability(&self) -> Result<QueryCertifiability> {
        let query = self.to_axql_query()?;
        Ok(query.certifiability())
    }

    /// Return a structured trust contract for the current IR without touching the
    /// execution engine.
    ///
    /// This makes the soundness boundary explicit: the contract is about returned
    /// rows within the current snapshot/context scope and does not claim complete
    /// answers or full ontology closure.
    #[allow(dead_code)]
    pub fn trust_contract(&self) -> Result<QueryTrustContract> {
        Ok(query_user_visible_trust_contract(
            &self.to_axql_query()?,
            &self.certifiability()?,
            false,
            None,
        ))
    }

    /// Return structured trust metadata enriched with ontology/business-rule
    /// coverage when meta-plane data is available.
    #[allow(dead_code)]
    pub fn trust_contract_with_meta(
        &self,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
    ) -> Result<QueryTrustContract> {
        Ok(query_user_visible_trust_contract_with_meta(
            &self.to_axql_query()?,
            &self.certifiability()?,
            false,
            None,
            meta,
        ))
    }

    /// Render the IR as an AxQL query string (best-effort, for debugging).
    pub fn to_axql_text(&self) -> Result<String> {
        let q = self.to_axql_query()?;
        Ok(render_axql_query(&q))
    }

    /// Apply a typed refinement handle to this IR and return the refined query.
    ///
    /// Current conservative scope: the refinement protocol only applies to a
    /// single conjunctive query body. Disjunctive IR is left unchanged unless a
    /// future protocol version carries explicit disjunct targeting.
    pub fn apply_refinement_handle(&self, handle: &AxqlRefinementHandleV1) -> Result<QueryIrV1> {
        handle.validate()?;
        match handle.scope {
            AxqlRefinementApplicationScopeV1::SingleConjunction => {}
        }

        let mut refined = self.clone();
        let mut used_variables = refined.variable_names();
        let where_atoms = refined.ensure_single_conjunctive_where_mut()?;
        let atom = query_atom_from_refinement_handle(handle, &mut used_variables)?;
        where_atoms.push(atom);
        Ok(refined)
    }

    fn ensure_single_conjunctive_where_mut(&mut self) -> Result<&mut Vec<QueryAtomIrV1>> {
        if let Some(disjuncts) = self.disjuncts.as_ref() {
            if disjuncts.len() != 1 {
                return Err(anyhow!(
                    "typed refinement apply currently requires a single conjunctive query"
                ));
            }
        }

        if let Some(disjuncts) = self.disjuncts.take() {
            let mut iter = disjuncts.into_iter();
            let where_atoms = iter
                .next()
                .ok_or_else(|| anyhow!("typed refinement apply requires a non-empty query body"))?;
            self.where_atoms = Some(where_atoms);
        }

        Ok(self.where_atoms.get_or_insert_with(Vec::new))
    }

    fn variable_names(&self) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        for var in &self.select_vars {
            names.insert(normalize_var_name(var));
        }
        if let Some(where_atoms) = &self.where_atoms {
            for atom in where_atoms {
                collect_atom_variables(atom, &mut names);
            }
        }
        if let Some(disjuncts) = &self.disjuncts {
            for disjunct in disjuncts {
                for atom in disjunct {
                    collect_atom_variables(atom, &mut names);
                }
            }
        }
        names
    }
}

/// A prepared query wrapper that keeps IR-level provenance with a concrete prepared plan.
///
/// This is the intended typed execution currency for typed flows (`query_ir_v1` ->
/// prepared -> execute/certify), while keeping the internal `PreparedQueryHandle`
/// as an implementation detail.
#[allow(dead_code)]
pub struct PreparedQueryV1 {
    query_ir: QueryIrV1,
    query: AxqlQuery,
    handle: PreparedQueryHandle,
    trust: QueryTrustContract,
}

/// A temporary anchor-bound view over a prepared query.
///
/// This keeps the existing `PreparedQueryV1` caches and ownership model intact
/// while allowing callers that already know an accepted module anchor to run the
/// query workflow in an anchor-aware lifecycle path.
#[allow(dead_code)]
pub struct AcceptedAnchoredPreparedQueryV1<'a> {
    accepted_axi_anchor: AcceptedAxiAnchor,
    prepared: &'a mut PreparedQueryV1,
}

/// A typed query answer artifact that preserves the workflow state of a query
/// result after execution and optional certification.
///
/// This extends the existing typestate pattern used by `Module<Validated>` /
/// `Module<Reviewed>` into the query workflow without changing the underlying
/// AxQL execution engine.
#[allow(dead_code)]
#[derive(Debug)]
pub struct QueryAnswer<S> {
    result: AxqlResult,
    trust: QueryTrustContract,
    query_fingerprint: String,
    anchor_digest: Option<AxiDigest>,
    certificate: Option<CertificateV2>,
    _state: PhantomData<S>,
}

/// A query answer artifact that preserves the accepted-snapshot/module anchor
/// throughout validated and certified query-answer lifecycle transitions.
#[allow(dead_code)]
#[derive(Debug)]
pub struct AcceptedAnchoredQueryAnswer<S> {
    accepted_axi_anchor: AcceptedAxiAnchor,
    answer: QueryAnswer<S>,
}

fn query_ir_id_for_axql_query(query: &AxqlQuery) -> String {
    format!(
        "query_ir_v1:{}",
        crate::axql::axql_query_ir_digest_v1(query)
    )
}

fn prepared_query_id_for_ir_ids(query_ir_id: &str, elaborated_query_ir_id: &str) -> String {
    format!(
        "prepared_query_v1:{}",
        axiograph_dsl::digest::axi_digest_v1(&format!(
            "query_ir_id={query_ir_id};elaborated_query_ir_id={elaborated_query_ir_id}"
        ))
    )
}

fn runtime_refinement_handles_from_report(
    report: &AxqlElaborationReport,
) -> Vec<crate::typed_refinement::RuntimeRefinementHandleV1> {
    let mut seen = BTreeSet::new();
    let mut handles = Vec::new();
    for handle in report
        .exploration_suggestions
        .iter()
        .flat_map(|suggestion| suggestion.refinement_candidates.iter())
        .map(|candidate| {
            crate::typed_refinement::RuntimeRefinementHandleV1::from_query(candidate.handle.clone())
        })
    {
        if seen.insert(handle.id.clone()) {
            handles.push(handle);
        }
    }
    handles
}

fn kernel_refs_for_prepared_query(
    query: &AxqlQuery,
    elaboration: &AxqlElaborationReport,
    kernel: &KernelModuleIr,
) -> Vec<KernelRefV1> {
    let selectors = QueryKernelRefSelectors::from_query(query, elaboration);
    let mut object_type_ids = BTreeSet::new();
    let mut relation_ids = BTreeSet::new();
    let mut role_ids = BTreeSet::new();

    for schema in &kernel.schemas {
        for (name, object_type_id) in &schema.object_type_ids {
            if selectors.matches_type(name) {
                object_type_ids.insert(object_type_id.to_string());
            }
        }
        for relation in schema.relations.values() {
            let relation_matches = selectors.matches_relation(&relation.name)
                || selectors.matches_relation(&relation.tuple_type_name);
            if relation_matches {
                relation_ids.insert(relation.relation_id.to_string());
            }
            for role in &relation.roles {
                if relation_matches || selectors.matches_role(&role.name) {
                    role_ids.insert(role.role_id.to_string());
                }
            }
        }
    }

    let mut theory_ids = BTreeSet::new();
    let mut refs = BTreeSet::new();
    for surface_ref in kernel.kernel_surface_v1().refs {
        match &surface_ref {
            KernelRefV1::Module { .. } => {
                refs.insert(surface_ref.clone());
            }
            KernelRefV1::SchemaObject {
                object: SchemaCategoryObjectRefIr::ObjectType { object_type_id, .. },
                ..
            } if object_type_ids.contains(object_type_id.as_str()) => {
                refs.insert(surface_ref.clone());
            }
            KernelRefV1::SchemaObject {
                object: SchemaCategoryObjectRefIr::RelationObject { relation_id, .. },
                ..
            } if relation_ids.contains(relation_id.as_str()) => {
                refs.insert(surface_ref.clone());
            }
            KernelRefV1::SchemaArrow {
                arrow:
                    SchemaCategoryArrowRefIr::RoleProjection {
                        role_id,
                        relation_id,
                        ..
                    },
                ..
            } if role_ids.contains(role_id.as_str())
                || relation_ids.contains(relation_id.as_str()) =>
            {
                refs.insert(surface_ref.clone());
            }
            KernelRefV1::TheoryObligation { obligation } => {
                if theory_obligation_touches_query_refs(
                    kernel,
                    obligation,
                    &relation_ids,
                    &role_ids,
                ) {
                    theory_ids.insert(theory_id_for_obligation(obligation));
                    refs.insert(surface_ref.clone());
                }
            }
            KernelRefV1::TheorySubject {
                theory_id,
                subject: TheorySubjectRefIr::Relation { relation_id, .. },
            } if relation_ids.contains(relation_id.as_str()) => {
                theory_ids.insert(theory_id.to_string());
                refs.insert(surface_ref.clone());
            }
            KernelRefV1::TheorySubject {
                theory_id,
                subject: TheorySubjectRefIr::Role { role_id, .. },
            } if role_ids.contains(role_id.as_str()) => {
                theory_ids.insert(theory_id.to_string());
                refs.insert(surface_ref.clone());
            }
            _ => {}
        }
    }

    for surface_ref in kernel.kernel_surface_v1().refs {
        if matches!(
            &surface_ref,
            KernelRefV1::Theory { theory_id, .. } if theory_ids.contains(theory_id.as_str())
        ) {
            refs.insert(surface_ref);
        }
    }

    refs.into_iter().collect()
}

#[derive(Debug, Default)]
struct QueryKernelRefSelectors {
    type_names: BTreeSet<String>,
    relation_names: BTreeSet<String>,
    role_names: BTreeSet<String>,
}

impl QueryKernelRefSelectors {
    fn from_query(query: &AxqlQuery, elaboration: &AxqlElaborationReport) -> Self {
        let mut selectors = Self::default();
        for types in elaboration.inferred_types.values() {
            for ty in types {
                selectors.insert_type(ty);
            }
        }
        for disjunct in &query.disjuncts {
            for atom in disjunct {
                selectors.add_atom(atom);
            }
        }
        selectors
    }

    fn add_atom(&mut self, atom: &AxqlAtom) {
        match atom {
            AxqlAtom::Type { type_name, .. } => self.insert_type(type_name),
            AxqlAtom::Edge { path, .. } => self.add_regex(&path.regex),
            AxqlAtom::Fact {
                relation, fields, ..
            } => {
                self.insert_relation(relation);
                for (role, _) in fields {
                    self.insert_role(role);
                }
            }
            AxqlAtom::HasOut { rels, .. } => {
                for rel in rels {
                    self.insert_role(rel);
                    self.insert_relation(rel);
                }
            }
            AxqlAtom::Shape {
                type_name, rels, ..
            } => {
                if let Some(type_name) = type_name {
                    self.insert_type(type_name);
                }
                for rel in rels {
                    self.insert_role(rel);
                    self.insert_relation(rel);
                }
            }
            AxqlAtom::AttrEq { .. }
            | AxqlAtom::AttrContains { .. }
            | AxqlAtom::AttrFts { .. }
            | AxqlAtom::AttrFuzzy { .. }
            | AxqlAtom::Attrs { .. } => {}
        }
    }

    fn add_regex(&mut self, regex: &crate::axql::AxqlRegex) {
        use crate::axql::AxqlRegex;
        match regex {
            AxqlRegex::Rel(rel) => {
                self.insert_relation(rel);
                self.insert_role(rel);
            }
            AxqlRegex::Seq(parts) | AxqlRegex::Alt(parts) => {
                for part in parts {
                    self.add_regex(part);
                }
            }
            AxqlRegex::Star(inner) | AxqlRegex::Plus(inner) | AxqlRegex::Opt(inner) => {
                self.add_regex(inner);
            }
            AxqlRegex::Epsilon => {}
        }
    }

    fn insert_type(&mut self, name: &str) {
        insert_name_variants(&mut self.type_names, name);
    }

    fn insert_relation(&mut self, name: &str) {
        insert_name_variants(&mut self.relation_names, name);
    }

    fn insert_role(&mut self, name: &str) {
        insert_name_variants(&mut self.role_names, name);
    }

    fn matches_type(&self, name: &str) -> bool {
        name_matches(&self.type_names, name)
    }

    fn matches_relation(&self, name: &str) -> bool {
        name_matches(&self.relation_names, name)
    }

    fn matches_role(&self, name: &str) -> bool {
        name_matches(&self.role_names, name)
    }
}

fn theory_obligation_touches_query_refs(
    kernel: &KernelModuleIr,
    obligation: &TheoryObligationRefIr,
    relation_ids: &BTreeSet<String>,
    role_ids: &BTreeSet<String>,
) -> bool {
    kernel
        .theories
        .iter()
        .find(|theory| theory.theory_id.as_str() == theory_id_for_obligation(obligation))
        .is_some_and(|theory| {
            theory
                .subject_refs_for_obligation(obligation)
                .into_iter()
                .any(|subject| match subject {
                    TheorySubjectRefIr::Relation { relation_id, .. } => {
                        relation_ids.contains(relation_id.as_str())
                    }
                    TheorySubjectRefIr::Role { role_id, .. } => role_ids.contains(role_id.as_str()),
                    TheorySubjectRefIr::Theory { .. } => false,
                })
        })
}

fn theory_id_for_obligation(obligation: &TheoryObligationRefIr) -> String {
    match obligation {
        TheoryObligationRefIr::Constraint { theory_id, .. }
        | TheoryObligationRefIr::PathEquation { theory_id, .. }
        | TheoryObligationRefIr::OpaqueEquation { theory_id, .. }
        | TheoryObligationRefIr::RewriteRule { theory_id, .. } => theory_id.to_string(),
    }
}

fn insert_name_variants(target: &mut BTreeSet<String>, name: &str) {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return;
    }
    target.insert(trimmed.to_string());
    target.insert(local_query_ref_name(trimmed));
}

fn name_matches(selectors: &BTreeSet<String>, name: &str) -> bool {
    selectors.contains(name) || selectors.contains(&local_query_ref_name(name))
}

fn local_query_ref_name(name: &str) -> String {
    name.rsplit_once([':', '.'])
        .map(|(_, local)| local.to_string())
        .unwrap_or_else(|| name.to_string())
}

#[allow(dead_code)]
impl<S: LifecycleState> QueryAnswer<S> {
    pub fn result(&self) -> &AxqlResult {
        &self.result
    }

    pub fn trust_contract(&self) -> &QueryTrustContract {
        &self.trust
    }

    pub fn lifecycle_state_name(&self) -> &'static str {
        S::NAME
    }
}

#[allow(dead_code)]
impl QueryAnswer<Certified> {
    pub fn anchor_digest(&self) -> &AxiDigest {
        self.anchor_digest
            .as_ref()
            .expect("certified query answers always carry an anchor digest")
    }

    pub fn certificate(&self) -> &CertificateV2 {
        self.certificate
            .as_ref()
            .expect("certified query answers always carry a certificate")
    }
}

#[allow(dead_code)]
impl<S: LifecycleState> AcceptedAnchoredQueryAnswer<S> {
    pub fn accepted_axi_anchor(&self) -> &AcceptedAxiAnchor {
        &self.accepted_axi_anchor
    }

    pub fn accepted_snapshot_id(&self) -> &axiograph_pathdb::AcceptedSnapshotId {
        &self.accepted_axi_anchor.accepted_snapshot_id
    }

    pub fn axi_digest(&self) -> &AxiDigest {
        &self.accepted_axi_anchor.axi_digest
    }

    pub fn result(&self) -> &AxqlResult {
        self.answer.result()
    }

    pub fn trust_contract(&self) -> &QueryTrustContract {
        self.answer.trust_contract()
    }

    pub fn lifecycle_state_name(&self) -> &'static str {
        self.answer.lifecycle_state_name()
    }
}

#[allow(dead_code)]
impl AcceptedAnchoredQueryAnswer<Certified> {
    pub fn anchor_digest(&self) -> &AxiDigest {
        self.answer.anchor_digest()
    }

    pub fn certificate(&self) -> &CertificateV2 {
        self.answer.certificate()
    }
}

#[allow(dead_code)]
impl PreparedQueryV1 {
    /// Bind this prepared query to an accepted module anchor for an
    /// anchor-aware validated/certified answer workflow.
    pub fn bind_accepted_axi_anchor<'a>(
        &'a mut self,
        accepted_axi_anchor: AcceptedAxiAnchor,
    ) -> AcceptedAnchoredPreparedQueryV1<'a> {
        AcceptedAnchoredPreparedQueryV1 {
            accepted_axi_anchor,
            prepared: self,
        }
    }

    fn apply_refinement_handle_internal(
        &self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
        handle: &AxqlRefinementHandleV1,
        theory_graph: Option<(&CompiledSchemaIr, &[TheoryIr])>,
    ) -> Result<QueryRefinementApplyResultV1> {
        let refined_query_ir_v1 = self.query_ir.apply_refinement_handle(handle)?;
        let refined_prepared = refined_query_ir_v1.prepare_with_meta(db, meta)?;
        let refined_exploration = match theory_graph {
            Some((compiled_schema, theories)) => {
                refined_prepared.exploration_view_with_theory_graph(None, compiled_schema, theories)
            }
            None => refined_prepared.exploration_view(None),
        };
        Ok(QueryRefinementApplyResultV1 {
            handle: handle.clone(),
            base_prepared_query: self.metadata_with_meta(meta)?,
            refined_prepared_query: refined_prepared.metadata_with_meta(meta)?,
            base_query_ir_v1: self.query_ir.clone(),
            refined_query_ir_v1,
            refined_elaborated_query_ir_v1: refined_prepared.elaborated_query_ir_v1()?,
            trust_before: self.trust_contract_with_meta(meta),
            trust_after: refined_prepared.trust_contract_with_meta(meta),
            introspection_before: self.introspection(),
            introspection_after: refined_prepared.introspection(),
            refined_exploration,
        })
    }

    /// Return the canonical typed IR for this prepared query.
    pub fn query_ir_v1(&self) -> &QueryIrV1 {
        &self.query_ir
    }

    /// Stable id for the normalized `query_ir_v1` body this handle prepared.
    pub fn query_ir_id(&self) -> String {
        query_ir_id_for_axql_query(&self.query)
    }

    /// Stable id for the elaborated `query_ir_v1` body the runtime actually prepared.
    pub fn elaborated_query_ir_id(&self) -> Result<String> {
        let elaborated = parse_axql_query(&self.handle.elaborated_query_text())?;
        Ok(query_ir_id_for_axql_query(&elaborated))
    }

    /// Stable prepared-query handle id derived from input and elaborated IR ids.
    pub fn prepared_query_id(&self) -> Result<String> {
        let query_ir_id = self.query_ir_id();
        let elaborated_query_ir_id = self.elaborated_query_ir_id()?;
        Ok(prepared_query_id_for_ir_ids(
            &query_ir_id,
            &elaborated_query_ir_id,
        ))
    }

    /// Return the prepared-query metadata envelope attached to this handle.
    pub fn metadata(&self) -> Result<PreparedQueryMetadataV1> {
        self.metadata_with_meta(None)
    }

    /// Return prepared-query metadata, enriching trust/coverage if meta-plane
    /// data is available.
    pub fn metadata_with_meta(
        &self,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
    ) -> Result<PreparedQueryMetadataV1> {
        let query_ir_id = self.query_ir_id();
        let elaborated_query_ir_id = self.elaborated_query_ir_id()?;
        let trust = self.trust_contract_with_meta(meta);
        Ok(PreparedQueryMetadataV1 {
            version: PREPARED_QUERY_METADATA_V1_VERSION,
            prepared_query_id: prepared_query_id_for_ir_ids(&query_ir_id, &elaborated_query_ir_id),
            query_ir_id,
            elaborated_query_ir_id,
            introspection: self.introspection(),
            inferred_types: self.elaboration_report().inferred_types.clone(),
            certifiability: self.certifiability(),
            non_claims: QueryNonClaimsV1::from_trust(&trust),
            trust,
            refinement_handles: runtime_refinement_handles_from_report(self.elaboration_report()),
            kernel_refs: Vec::new(),
        })
    }

    /// Return prepared-query metadata enriched with compiled kernel refs.
    ///
    /// This is intentionally additive: query preparation still only needs the
    /// PathDB/meta-plane boundary, while callers that already compiled the
    /// canonical module can attach the shared `KernelRefV1` currency.
    pub fn metadata_with_meta_and_kernel(
        &self,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
        kernel: &KernelModuleIr,
    ) -> Result<PreparedQueryMetadataV1> {
        let mut metadata = self.metadata_with_meta(meta)?;
        metadata.kernel_refs = self.kernel_refs(kernel);
        kernel
            .kernel_surface_v1()
            .validate_refs(&metadata.kernel_refs)
            .map_err(|err| anyhow::anyhow!(err))?;
        Ok(metadata)
    }

    pub fn kernel_refs(&self, kernel: &KernelModuleIr) -> Vec<KernelRefV1> {
        kernel_refs_for_prepared_query(&self.query, self.elaboration_report(), kernel)
    }

    /// Return a borrowed AxQL view of the compiled query.
    pub fn as_query(&self) -> &AxqlQuery {
        &self.query
    }

    /// Execute the prepared query.
    pub fn execute(
        &mut self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
    ) -> Result<AxqlResult> {
        self.handle.execute(db, meta)
    }

    /// Execute the prepared query and package the result as a lifecycle-typed
    /// workflow artifact.
    pub fn execute_answer(
        &mut self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
    ) -> Result<QueryAnswer<Validated>> {
        Ok(QueryAnswer {
            result: self.handle.execute(db, meta)?,
            trust: self.trust_contract_with_meta(meta),
            query_fingerprint: crate::axql::axql_query_ir_digest_v1(&self.query),
            anchor_digest: None,
            certificate: None,
            _state: PhantomData,
        })
    }

    /// Certify a previously executed validated answer for this exact prepared
    /// query.
    pub fn certify_answer_with_anchor(
        &mut self,
        answer: QueryAnswer<Validated>,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
        axi_digest: AxiDigest,
    ) -> Result<QueryAnswer<Certified>> {
        let expected_fingerprint = crate::axql::axql_query_ir_digest_v1(&self.query);
        if answer.query_fingerprint != expected_fingerprint {
            return Err(anyhow!(
                "validated query answer does not belong to this prepared query"
            ));
        }

        let rerun_result = self.handle.execute(db, meta)?;
        if rerun_result != answer.result {
            return Err(anyhow!(
                "validated query answer no longer matches the prepared query result for this runtime"
            ));
        }

        let certificate =
            self.handle
                .certify_typed_with_anchor(db, &self.query, meta, axi_digest.as_str())?;
        let trust = query_user_visible_trust_contract_with_meta(
            &self.query,
            &self.certifiability(),
            true,
            None,
            meta,
        );
        Ok(QueryAnswer {
            result: answer.result,
            trust,
            query_fingerprint: expected_fingerprint,
            anchor_digest: Some(axi_digest),
            certificate: Some(certificate),
            _state: PhantomData,
        })
    }

    /// Return the fully elaborated plan shape as human-readable text.
    pub fn explain_plan_lines(&self) -> Vec<String> {
        self.handle.explain_plan_lines()
    }

    /// Return the elaborated query text that the runtime actually prepared.
    pub fn elaborated_query_text(&self) -> String {
        self.handle.elaborated_query_text()
    }

    /// Return structured trust metadata for this prepared query.
    pub fn trust_contract(&self) -> QueryTrustContract {
        self.trust.clone()
    }

    /// Return structured trust metadata enriched with ontology/business-rule
    /// coverage when meta-plane data is available.
    pub fn trust_contract_with_meta(
        &self,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
    ) -> QueryTrustContract {
        if meta.is_none() || self.trust.semantic_coverage.is_some() {
            return self.trust.clone();
        }
        query_user_visible_trust_contract_with_meta(
            &self.query,
            &self.certifiability(),
            false,
            None,
            meta,
        )
    }

    /// Return the semantic claims currently attached to this prepared query.
    ///
    /// This is a typed/runtime-friendly surface for agent tooling and
    /// business-rule checks: if the query was prepared with meta-plane data,
    /// the returned claims describe which ontology surfaces are in scope.
    pub fn semantic_claims(&self) -> &[crate::trust_contract::SemanticClaimSummaryV1] {
        &self.trust.semantic_claims
    }

    /// Return semantic-coverage metadata attached to this prepared query, if
    /// available.
    pub fn semantic_coverage(&self) -> Option<&crate::trust_contract::SemanticCoverageSummaryV1> {
        self.trust.semantic_coverage.as_ref()
    }

    /// Return explicit trust gaps attached to this prepared query.
    pub fn trust_gaps(&self) -> &[crate::trust_contract::TrustGapV1] {
        &self.trust.gaps
    }

    /// Return a short explainable summary of the prepared query shape and trust class.
    pub fn introspection(&self) -> PreparedQueryIntrospection {
        self.handle.introspection()
    }

    /// Return the structured elaboration payload for this prepared query.
    pub fn elaboration_report(&self) -> &AxqlElaborationReport {
        self.handle.elaboration_report()
    }

    /// Return a focused exploration view over the prepared query.
    ///
    /// This is the runtime/editor-facing typed-hole surface: it preserves
    /// unresolved query structure as holes and returns admissible next moves
    /// scoped to the compiled schema/category semantics already in play.
    pub fn exploration_view(&self, focus_variable: Option<&str>) -> PreparedQueryExplorationV1 {
        let report = self.elaboration_report();
        let focus_variable = focus_variable.map(str::trim).filter(|v| !v.is_empty());

        let typed_holes = report
            .typed_holes
            .iter()
            .filter(|hole| match focus_variable {
                Some(var) => hole.variable.as_deref() == Some(var) || hole.variable.is_none(),
                None => true,
            })
            .cloned()
            .collect();
        let exploration_suggestions = report
            .exploration_suggestions
            .iter()
            .filter(|suggestion| match focus_variable {
                Some(var) => suggestion.variable == var,
                None => true,
            })
            .cloned()
            .collect();
        let refinement_candidates = report
            .exploration_suggestions
            .iter()
            .filter(|suggestion| match focus_variable {
                Some(var) => suggestion.variable == var,
                None => true,
            })
            .flat_map(|suggestion| suggestion.refinement_candidates.clone().into_iter())
            .map(crate::typed_refinement::RuntimeRefinementCandidateV1::from_axql)
            .collect();

        PreparedQueryExplorationV1 {
            introspection: self.introspection(),
            inferred_types: report.inferred_types.clone(),
            notes: report.notes.clone(),
            typed_holes,
            exploration_suggestions,
            refinement_candidates,
            semantic_claims: self.semantic_claims().to_vec(),
            semantic_coverage: self.semantic_coverage().cloned(),
            trust_gaps: self.trust_gaps().to_vec(),
        }
    }

    /// Return a focused exploration view enriched with compiled-theory handles.
    ///
    /// This is the typed exploration surface for higher-order/dependent
    /// ontology tooling: query holes still come from AxQL elaboration, but the
    /// machine-usable refinement candidates are lifted onto the same
    /// obligation/subject graph used by migration and reconciliation builders.
    pub fn exploration_view_with_theory_graph(
        &self,
        focus_variable: Option<&str>,
        compiled_schema: &CompiledSchemaIr,
        theories: &[TheoryIr],
    ) -> PreparedQueryExplorationV1 {
        let report = self.elaboration_report();
        let focus_variable = focus_variable.map(str::trim).filter(|v| !v.is_empty());

        let typed_holes = report
            .typed_holes
            .iter()
            .filter(|hole| match focus_variable {
                Some(var) => hole.variable.as_deref() == Some(var) || hole.variable.is_none(),
                None => true,
            })
            .cloned()
            .collect();
        let exploration_suggestions = report
            .exploration_suggestions
            .iter()
            .filter(|suggestion| match focus_variable {
                Some(var) => suggestion.variable == var,
                None => true,
            })
            .cloned()
            .collect();
        let refinement_candidates = report
            .exploration_suggestions
            .iter()
            .filter(|suggestion| match focus_variable {
                Some(var) => suggestion.variable == var,
                None => true,
            })
            .flat_map(|suggestion| suggestion.refinement_candidates.clone().into_iter())
            .map(|candidate| {
                crate::typed_refinement::RuntimeRefinementCandidateV1::from_axql_with_theory(
                    candidate,
                    compiled_schema,
                    theories,
                )
            })
            .collect();

        PreparedQueryExplorationV1 {
            introspection: self.introspection(),
            inferred_types: report.inferred_types.clone(),
            notes: report.notes.clone(),
            typed_holes,
            exploration_suggestions,
            refinement_candidates,
            semantic_claims: self.semantic_claims().to_vec(),
            semantic_coverage: self.semantic_coverage().cloned(),
            trust_gaps: self.trust_gaps().to_vec(),
        }
    }

    /// Apply a typed refinement handle and return the refined query plus updated
    /// trust/introspection/exploration metadata.
    pub fn apply_refinement_handle(
        &self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
        handle: &AxqlRefinementHandleV1,
    ) -> Result<QueryRefinementApplyResultV1> {
        self.apply_refinement_handle_internal(db, meta, handle, None)
    }

    /// Apply a typed refinement handle and keep the compiled theory graph on the
    /// refined exploration payload.
    pub fn apply_refinement_handle_with_theory_graph(
        &self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
        handle: &AxqlRefinementHandleV1,
        compiled_schema: &CompiledSchemaIr,
        theories: &[TheoryIr],
    ) -> Result<QueryRefinementApplyResultV1> {
        self.apply_refinement_handle_internal(db, meta, handle, Some((compiled_schema, theories)))
    }

    /// Resolve a refinement handle by id from the current exploration payload
    /// and apply it.
    pub fn apply_refinement_by_id(
        &self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
        handle_id: &str,
    ) -> Result<QueryRefinementApplyResultV1> {
        let handle = self
            .exploration_view(None)
            .exploration_suggestions
            .into_iter()
            .flat_map(|suggestion| suggestion.refinement_candidates.into_iter())
            .find(|candidate| candidate.handle.id == handle_id)
            .map(|candidate| candidate.handle)
            .ok_or_else(|| anyhow!("unknown refinement handle `{handle_id}`"))?;
        self.apply_refinement_handle(db, meta, &handle)
    }

    /// Resolve a refinement handle by id from the theory-enriched exploration
    /// payload and apply it while keeping theory handles on the result.
    pub fn apply_refinement_by_id_with_theory_graph(
        &self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
        handle_id: &str,
        compiled_schema: &CompiledSchemaIr,
        theories: &[TheoryIr],
    ) -> Result<QueryRefinementApplyResultV1> {
        let handle = self
            .exploration_view_with_theory_graph(None, compiled_schema, theories)
            .refinement_candidates
            .into_iter()
            .find(|candidate| candidate.handle.id == handle_id)
            .map(|candidate| match candidate.handle.payload {
                crate::typed_refinement::RuntimeRefinementPayloadV1::Query { handle } => handle,
                _ => unreachable!("query exploration must only emit query-domain candidates"),
            })
            .ok_or_else(|| anyhow!("unknown refinement handle `{handle_id}`"))?;
        self.apply_refinement_handle_with_theory_graph(db, meta, &handle, compiled_schema, theories)
    }

    /// Apply a shared runtime refinement handle. Query refinements and
    /// authoring refinements can now travel through one machine-facing handle
    /// protocol, even though this method only accepts the query domain.
    pub fn apply_runtime_refinement_handle(
        &self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
        handle: &crate::typed_refinement::RuntimeRefinementHandleV1,
    ) -> Result<QueryRefinementApplyResultV1> {
        handle.validate()?;
        let crate::typed_refinement::RuntimeRefinementPayloadV1::Query { handle } = &handle.payload
        else {
            return Err(anyhow!(
                "runtime refinement handle `{}` is not a query refinement",
                handle.id
            ));
        };
        self.apply_refinement_handle(db, meta, handle)
    }

    /// Apply a shared runtime refinement handle while preserving compiled
    /// theory handles on the refined exploration payload.
    pub fn apply_runtime_refinement_handle_with_theory_graph(
        &self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
        handle: &crate::typed_refinement::RuntimeRefinementHandleV1,
        compiled_schema: &CompiledSchemaIr,
        theories: &[TheoryIr],
    ) -> Result<QueryRefinementApplyResultV1> {
        handle.validate()?;
        let crate::typed_refinement::RuntimeRefinementPayloadV1::Query { handle } = &handle.payload
        else {
            return Err(anyhow!(
                "runtime refinement handle `{}` is not a query refinement",
                handle.id
            ));
        };
        self.apply_refinement_handle_with_theory_graph(db, meta, handle, compiled_schema, theories)
    }

    /// Resolve a shared runtime refinement handle by id from the current
    /// exploration payload and apply it.
    pub fn apply_runtime_refinement_by_id(
        &self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
        handle_id: &str,
    ) -> Result<QueryRefinementApplyResultV1> {
        let handle = self
            .exploration_view(None)
            .refinement_candidates
            .into_iter()
            .find(|candidate| candidate.handle.id == handle_id)
            .map(|candidate| candidate.handle)
            .ok_or_else(|| anyhow!("unknown runtime refinement handle `{handle_id}`"))?;
        self.apply_runtime_refinement_handle(db, meta, &handle)
    }

    /// Resolve a shared runtime refinement handle by id from the theory-enriched
    /// exploration payload and apply it while preserving theory handles.
    pub fn apply_runtime_refinement_by_id_with_theory_graph(
        &self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
        handle_id: &str,
        compiled_schema: &CompiledSchemaIr,
        theories: &[TheoryIr],
    ) -> Result<QueryRefinementApplyResultV1> {
        let handle = self
            .exploration_view_with_theory_graph(None, compiled_schema, theories)
            .refinement_candidates
            .into_iter()
            .find(|candidate| candidate.handle.id == handle_id)
            .map(|candidate| candidate.handle)
            .ok_or_else(|| anyhow!("unknown runtime refinement handle `{handle_id}`"))?;
        self.apply_runtime_refinement_handle_with_theory_graph(
            db,
            meta,
            &handle,
            compiled_schema,
            theories,
        )
    }

    /// Return the elaborated query IR that the runtime actually prepared.
    pub fn elaborated_query_ir_v1(&self) -> Result<QueryIrV1> {
        let elaborated = parse_axql_query(&self.handle.elaborated_query_text())?;
        Ok(QueryIrV1::from_axql_query(&elaborated))
    }

    /// Classification of what this prepared query can be certified under.
    pub fn certifiability(&self) -> QueryCertifiability {
        self.handle.certifiability()
    }

    /// Emit the canonical `.axi`-anchored typed query witness for this prepared query.
    pub fn certify_typed_with_anchor(
        &self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
        axi_digest_v1: &str,
    ) -> Result<CertificateV2> {
        self.handle
            .certify_typed_with_anchor(db, &self.query, meta, axi_digest_v1)
    }
}

#[allow(dead_code)]
impl<'a> AcceptedAnchoredPreparedQueryV1<'a> {
    pub fn accepted_axi_anchor(&self) -> &AcceptedAxiAnchor {
        &self.accepted_axi_anchor
    }

    pub fn accepted_snapshot_id(&self) -> &axiograph_pathdb::AcceptedSnapshotId {
        &self.accepted_axi_anchor.accepted_snapshot_id
    }

    pub fn axi_digest(&self) -> &AxiDigest {
        &self.accepted_axi_anchor.axi_digest
    }

    pub fn query_ir_v1(&self) -> &QueryIrV1 {
        self.prepared.query_ir_v1()
    }

    pub fn elaborated_query_text(&self) -> String {
        self.prepared.elaborated_query_text()
    }

    pub fn elaboration_report(&self) -> &AxqlElaborationReport {
        self.prepared.elaboration_report()
    }

    pub fn explain_plan_lines(&self) -> Vec<String> {
        self.prepared.explain_plan_lines()
    }

    pub fn trust_contract_with_meta(
        &self,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
    ) -> QueryTrustContract {
        self.prepared.trust_contract_with_meta(meta)
    }

    pub fn exploration_view(&self, focus_variable: Option<&str>) -> PreparedQueryExplorationV1 {
        self.prepared.exploration_view(focus_variable)
    }

    pub fn certifiability(&self) -> QueryCertifiability {
        self.prepared.certifiability()
    }

    pub fn execute(
        &mut self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
    ) -> Result<AxqlResult> {
        self.prepared.execute(db, meta)
    }

    pub fn execute_answer(
        &mut self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
    ) -> Result<AcceptedAnchoredQueryAnswer<Validated>> {
        Ok(AcceptedAnchoredQueryAnswer {
            accepted_axi_anchor: self.accepted_axi_anchor.clone(),
            answer: self.prepared.execute_answer(db, meta)?,
        })
    }

    pub fn certify_typed(
        &self,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
    ) -> Result<CertificateV2> {
        self.prepared.certify_typed_with_anchor(
            db,
            meta,
            self.accepted_axi_anchor.axi_digest.as_str(),
        )
    }

    pub fn certify_answer(
        &mut self,
        answer: AcceptedAnchoredQueryAnswer<Validated>,
        db: &axiograph_pathdb::PathDB,
        meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
    ) -> Result<AcceptedAnchoredQueryAnswer<Certified>> {
        if answer.accepted_axi_anchor != self.accepted_axi_anchor {
            return Err(anyhow!(
                "validated query answer is bound to a different accepted anchor"
            ));
        }

        Ok(AcceptedAnchoredQueryAnswer {
            accepted_axi_anchor: self.accepted_axi_anchor.clone(),
            answer: self.prepared.certify_answer_with_anchor(
                answer.answer,
                db,
                meta,
                self.accepted_axi_anchor.axi_digest.clone(),
            )?,
        })
    }
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
            (
                Some(disjuncts_ir.into_iter().next().unwrap_or_default()),
                None,
            )
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
                format!(
                    "({})",
                    parts.iter().map(render_re).collect::<Vec<_>>().join("|")
                )
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
                let mut out_pairs: Vec<(String, String)> =
                    pairs.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
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
                let mut out_attrs: Vec<(String, String)> =
                    attrs.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
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

fn collect_term_variables(term: &QueryTermIrV1, vars: &mut BTreeSet<String>) {
    match term {
        QueryTermIrV1::Simple(name) => {
            if name.starts_with('?') {
                vars.insert(normalize_var_name(name));
            }
        }
        QueryTermIrV1::Id(_) => {}
        QueryTermIrV1::Obj(obj) => {
            if let QueryTermObjIrV1::Var { name } = obj {
                vars.insert(normalize_var_name(name));
            }
        }
    }
}

fn collect_atom_variables(atom: &QueryAtomIrV1, vars: &mut BTreeSet<String>) {
    match atom {
        QueryAtomIrV1::Type { term, .. }
        | QueryAtomIrV1::AttrEq { term, .. }
        | QueryAtomIrV1::AttrContains { term, .. }
        | QueryAtomIrV1::AttrFts { term, .. }
        | QueryAtomIrV1::AttrFuzzy { term, .. }
        | QueryAtomIrV1::HasOut { term, .. }
        | QueryAtomIrV1::Attrs { term, .. }
        | QueryAtomIrV1::Shape { term, .. } => collect_term_variables(term, vars),
        QueryAtomIrV1::Edge { left, right, .. } => {
            collect_term_variables(left, vars);
            collect_term_variables(right, vars);
        }
        QueryAtomIrV1::Fact { fact, fields, .. } => {
            if let Some(fact) = fact {
                collect_term_variables(fact, vars);
            }
            for term in fields.values() {
                collect_term_variables(term, vars);
            }
        }
    }
}

fn fresh_variable_name(base: &str, used: &mut BTreeSet<String>) -> String {
    let normalized = normalize_var_name(base);
    if used.insert(normalized.clone()) {
        return normalized;
    }

    let stem = normalized.trim_start_matches('?');
    let mut suffix = 2usize;
    loop {
        let candidate = format!("?{stem}_{suffix}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}

fn query_term_from_refinement_term(
    term: &AxqlRefinementTermV1,
    used: &mut BTreeSet<String>,
    suggested_names: &mut BTreeMap<String, String>,
) -> Result<QueryTermIrV1> {
    Ok(match term {
        AxqlRefinementTermV1::ExistingVariable { name } => {
            let normalized = normalize_var_name(name);
            if !used.contains(&normalized) {
                return Err(anyhow!(
                    "refinement handle refers to unknown query variable `{normalized}`"
                ));
            }
            QueryTermIrV1::Obj(QueryTermObjIrV1::Var { name: normalized })
        }
        AxqlRefinementTermV1::SuggestedVariable { name } => {
            let actual = suggested_names
                .entry(name.clone())
                .or_insert_with(|| fresh_variable_name(name, used))
                .clone();
            QueryTermIrV1::Obj(QueryTermObjIrV1::Var { name: actual })
        }
        AxqlRefinementTermV1::NameLookup { value } => QueryTermIrV1::Obj(QueryTermObjIrV1::Name {
            value: value.clone(),
        }),
        AxqlRefinementTermV1::Wildcard => QueryTermIrV1::Obj(QueryTermObjIrV1::Wildcard {}),
    })
}

fn query_atom_from_refinement_handle(
    handle: &AxqlRefinementHandleV1,
    used: &mut BTreeSet<String>,
) -> Result<QueryAtomIrV1> {
    let mut suggested_names: BTreeMap<String, String> = BTreeMap::new();
    Ok(match &handle.op {
        AxqlRefinementOpV1::AddTypeGuard { term, type_name } => QueryAtomIrV1::Type {
            term: query_term_from_refinement_term(term, used, &mut suggested_names)?,
            type_name: type_name.clone(),
        },
        AxqlRefinementOpV1::AddEdgeAtom { left, path, right } => QueryAtomIrV1::Edge {
            left: query_term_from_refinement_term(left, used, &mut suggested_names)?,
            path: path.clone(),
            right: query_term_from_refinement_term(right, used, &mut suggested_names)?,
        },
        AxqlRefinementOpV1::AddFactAtom {
            fact,
            relation,
            fields,
        } => {
            let fact = fact
                .as_ref()
                .map(|term| query_term_from_refinement_term(term, used, &mut suggested_names))
                .transpose()?;
            let fields = fields
                .iter()
                .map(|(field, term)| {
                    Ok((
                        field.clone(),
                        query_term_from_refinement_term(term, used, &mut suggested_names)?,
                    ))
                })
                .collect::<Result<BTreeMap<_, _>>>()?;
            QueryAtomIrV1::Fact {
                fact,
                relation: relation.clone(),
                fields,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn query_certificate_policy_v1_require_verified_preconditions_fail_closed() -> Result<()> {
        let policy: QueryCertificatePolicyV1 = serde_json::from_str("\"require_verified\"")?;
        assert!(policy.emits_certificate());
        assert!(policy.verifies_certificate());
        assert!(policy.requires_verified());

        let unsupported = QueryCertifiability::ExecutionOnly {
            reasons: vec!["cannot certify approximate string atom".to_string()],
        };
        let err = policy
            .ensure_require_verified_preconditions(&unsupported, None, None)
            .expect_err("require_verified should reject unsupported, unanchored queries");
        let err = err.to_string();
        assert!(err.contains("query certificate policy `require_verified`"));
        assert!(err.contains("query is not fully certifiable"));
        assert!(err.contains("missing accepted `.axi` anchor"));
        assert!(err.contains("missing canonical `.axi` text"));

        let anchor = AcceptedAxiAnchor::new(
            axiograph_pathdb::AcceptedSnapshotId::new("accepted:test"),
            AxiDigest::new("fnv1a64:abc"),
        );
        policy.ensure_require_verified_preconditions(
            &QueryCertifiability::Certifiable,
            Some(&anchor),
            Some("module Demo\n"),
        )?;
        assert!(policy.ensure_verified_result(Some(true)).is_ok());
        assert!(policy.ensure_verified_result(Some(false)).is_err());
        assert!(policy.ensure_verified_result(None).is_err());
        Ok(())
    }

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
    fn query_ir_edge_simple_name_term_renders_as_name_lookup() -> Result<()> {
        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["p"],
              "where": [
                {"kind": "edge", "left": "Alice", "path": "Parent", "right": "?p"}
              ],
              "limit": 10
            }"#,
        )?;

        assert_eq!(
            q.to_axql_text()?,
            r#"select ?p where name("Alice") -Parent-> ?p limit 10"#
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

    #[test]
    fn query_ir_v1_prepare_with_meta_builds_prepared_handle() -> Result<()> {
        let axi = r#"
module Demo

schema S:
  object Node

instance I of S:
  Node = {a}
"#;
        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;

        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["x"],
              "where": [
                {"kind": "type", "term": "?x", "type": "Node"}
              ],
              "limit": 10
            }"#,
        )?;

        let prepared = q.prepare_with_meta(&db, Some(&meta))?;
        assert_eq!(prepared.certifiability(), QueryCertifiability::Certifiable);
        let introspection = prepared.introspection();
        assert_eq!(introspection.disjunct_count, 1);
        assert_eq!(introspection.selected_vars, vec!["?x"]);
        assert_eq!(introspection.limit, 10);
        assert_eq!(introspection.context_count, 0);
        assert_eq!(introspection.typed_hole_count, 0);
        assert_eq!(introspection.exploration_target_count, 0);
        assert_eq!(prepared.trust_contract().trust_class, "certifiable");
        assert!(prepared.semantic_coverage().is_some());
        assert!(prepared
            .semantic_claims()
            .iter()
            .any(|claim| claim.kind == "declared_object_type"));
        assert!(prepared.trust_gaps().is_empty());
        assert!(prepared
            .explain_plan_lines()
            .iter()
            .any(|line| line.contains("join order")));
        Ok(())
    }

    #[test]
    fn query_ir_v1_prepared_handle_exposes_elaboration_payload() -> Result<()> {
        let axi = r#"
module Demo

schema Demo:
  object Node
  object Supplier
  subtype Supplier < Node
  relation Flow(from: Supplier, to: Supplier)

instance I of Demo:
  Supplier = {a, b}
  Flow = {(from=a, to=b)}
"#;
        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;

        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["dst"],
              "where": [
                {
                  "kind": "fact",
                  "fact": "?f",
                  "relation": "Flow",
                  "fields": {
                    "from": "a",
                    "to": "?dst"
                  }
                }
              ],
              "limit": 5
            }"#,
        )?;

        let prepared = q.prepare_with_meta(&db, Some(&meta))?;
        let report = prepared.elaboration_report();
        assert!(report
            .inferred_types
            .get("?dst")
            .is_some_and(|tys| tys.iter().any(|ty| ty == "Supplier")));
        assert!(report
            .exploration_suggestions
            .iter()
            .any(|suggestion| suggestion.variable == "?dst"));

        let elaborated_ir = prepared.elaborated_query_ir_v1()?;
        assert_eq!(elaborated_ir.version, QUERY_IR_V1_VERSION);
        assert!(
            elaborated_ir
                .where_atoms
                .as_ref()
                .is_some_and(|atoms| !atoms.is_empty())
                || elaborated_ir
                    .disjuncts
                    .as_ref()
                    .is_some_and(|disjuncts| !disjuncts.is_empty())
        );
        Ok(())
    }

    #[test]
    fn query_ir_v1_prepared_handle_exposes_focused_exploration_view() -> Result<()> {
        let axi = r#"
module Demo

schema Demo:
  object Node
  object Supplier
  subtype Supplier < Node
  relation Flow(from: Supplier, to: Supplier)

instance I of Demo:
  Supplier = {a, b}
  Flow = {(from=a, to=b)}
"#;
        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;

        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["src", "dst"],
              "where": [
                {
                  "kind": "fact",
                  "fact": "?f",
                  "relation": "Flow",
                  "fields": {
                    "from": "?src",
                    "to": "?dst"
                  }
                }
              ],
              "limit": 5
            }"#,
        )?;

        let prepared = q.prepare_with_meta(&db, Some(&meta))?;
        let full = prepared.exploration_view(None);
        assert!(full
            .exploration_suggestions
            .iter()
            .any(|suggestion| suggestion.variable == "?src"));
        assert!(full
            .exploration_suggestions
            .iter()
            .any(|suggestion| suggestion.variable == "?dst"));
        assert!(full.exploration_suggestions.iter().any(|suggestion| {
            suggestion.variable == "?dst" && !suggestion.refinement_candidates.is_empty()
        }));
        assert!(full.refinement_candidates.iter().any(|candidate| matches!(
            candidate.handle.domain(),
            crate::typed_refinement::RuntimeRefinementDomainV1::Query
        )));
        assert!(!full.semantic_claims.is_empty());
        assert!(full.semantic_coverage.is_some());

        let focused = prepared.exploration_view(Some("?dst"));
        assert!(focused
            .exploration_suggestions
            .iter()
            .all(|suggestion| suggestion.variable == "?dst"));
        Ok(())
    }

    #[test]
    fn prepared_query_v1_metadata_cites_ir_ids_trust_non_claims_and_handles() -> Result<()> {
        let axi = r#"
module Demo

schema Demo:
  object Node
  object Supplier
  subtype Supplier < Node
  relation Flow(from: Supplier, to: Supplier)

theory DemoRules on Demo:
  constraint key Flow(from, to)

instance I of Demo:
  Supplier = {a, b}
  Flow = {(from=a, to=b)}
"#;
        let parsed = axiograph_dsl::axi_v1::parse_axi_v1(axi)?;
        let kernel =
            axiograph_pathdb::compile_kernel_module_ir(&parsed, axi).map_err(anyhow::Error::msg)?;
        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;

        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["dst"],
              "where": [
                {
                  "kind": "fact",
                  "fact": "?f",
                  "relation": "Flow",
                  "fields": {
                    "from": "a",
                    "to": "?dst"
                  }
                }
              ],
              "limit": 5
            }"#,
        )?;

        let prepared = q.prepare_with_meta(&db, Some(&meta))?;
        let metadata = prepared.metadata_with_meta(Some(&meta))?;
        assert_eq!(metadata.version, PREPARED_QUERY_METADATA_V1_VERSION);
        assert_eq!(metadata.query_ir_id, prepared.query_ir_id());
        assert_eq!(
            metadata.elaborated_query_ir_id,
            prepared.elaborated_query_ir_id()?
        );
        assert_eq!(metadata.prepared_query_id, prepared.prepared_query_id()?);
        assert!(metadata.query_ir_id.starts_with("query_ir_v1:"));
        assert!(metadata.prepared_query_id.starts_with("prepared_query_v1:"));
        assert_eq!(metadata.certifiability, QueryCertifiability::Certifiable);
        assert_eq!(metadata.trust.trust_class, "certifiable");
        assert_eq!(
            metadata.non_claims.claim_scope,
            "returned_rows_within_snapshot_and_context"
        );
        assert_eq!(metadata.non_claims.completeness_claim, "not_claimed");
        assert_eq!(metadata.non_claims.ontology_closure_claim, "not_claimed");
        assert!(metadata.kernel_refs.is_empty());
        assert!(metadata
            .inferred_types
            .get("?dst")
            .is_some_and(|tys| tys.iter().any(|ty| ty == "Supplier")));
        assert!(!metadata.refinement_handles.is_empty());
        assert!(metadata.refinement_handles.iter().all(|handle| {
            handle.validate().is_ok()
                && matches!(
                    handle.domain(),
                    crate::typed_refinement::RuntimeRefinementDomainV1::Query
                )
        }));

        let metadata_with_kernel = prepared.metadata_with_meta_and_kernel(Some(&meta), &kernel)?;
        kernel
            .kernel_surface_v1()
            .validate_refs(&metadata_with_kernel.kernel_refs)
            .expect("prepared-query metadata only cites declared kernel refs");
        assert!(metadata_with_kernel
            .kernel_refs
            .iter()
            .any(|reference| { matches!(reference, KernelRefV1::Module { .. }) }));
        assert!(metadata_with_kernel.kernel_refs.iter().any(|reference| {
            matches!(
                reference,
                KernelRefV1::SchemaObject {
                    object: SchemaCategoryObjectRefIr::ObjectType { name, .. },
                    ..
                } if name == "Supplier"
            )
        }));
        assert!(metadata_with_kernel.kernel_refs.iter().any(|reference| {
            matches!(
                reference,
                KernelRefV1::SchemaObject {
                    object: SchemaCategoryObjectRefIr::RelationObject { name, .. },
                    ..
                } if name == "Flow"
            )
        }));
        assert!(metadata_with_kernel.kernel_refs.iter().any(|reference| {
            matches!(
                reference,
                KernelRefV1::SchemaArrow {
                    arrow: SchemaCategoryArrowRefIr::RoleProjection { role_name, .. },
                    ..
                } if role_name == "to"
            )
        }));
        assert!(metadata_with_kernel.kernel_refs.iter().any(|reference| {
            matches!(
                reference,
                KernelRefV1::TheoryObligation {
                    obligation: TheoryObligationRefIr::Constraint { summary, .. }
                } if summary == "key Flow(from, to)"
            )
        }));

        let applied = metadata
            .refinement_handles
            .iter()
            .find_map(|handle| {
                prepared
                    .apply_runtime_refinement_by_id(&db, Some(&meta), &handle.id)
                    .ok()
            })
            .ok_or_else(|| anyhow!("expected at least one metadata refinement handle to apply"))?;
        assert_eq!(
            applied.base_prepared_query.prepared_query_id,
            metadata.prepared_query_id
        );
        assert_ne!(
            applied.refined_prepared_query.prepared_query_id,
            applied.base_prepared_query.prepared_query_id
        );
        assert_eq!(
            applied.refined_prepared_query.non_claims.completeness_claim,
            "not_claimed"
        );
        Ok(())
    }

    #[test]
    fn query_ir_v1_exploration_view_can_attach_compiled_theory_handles() -> Result<()> {
        let axi = r#"
module Demo

schema Demo:
  object Person
  relation Parent(parent: Person, child: Person)

theory DemoRules on Demo:
  constraint key Parent(parent, child)

instance I of Demo:
  Person = {Alice, Bob}
  Parent = {(parent=Alice, child=Bob)}
"#;
        let parsed = axiograph_dsl::axi_v1::parse_axi_v1(axi)?;
        let compiled_schema = axiograph_pathdb::kernel_ir::compile_schema_ir(&parsed.schemas[0]);
        let theories = parsed
            .theories
            .iter()
            .map(|theory| axiograph_pathdb::kernel_ir::compile_theory_ir(&compiled_schema, theory))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(anyhow::Error::msg)?;

        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;

        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["c"],
              "where": [
                {
                  "kind": "fact",
                  "fact": "?f",
                  "relation": "Parent",
                  "fields": {
                    "parent": "Alice",
                    "child": "?c"
                  }
                }
              ],
              "limit": 5
            }"#,
        )?;

        let prepared = q.prepare_with_meta(&db, Some(&meta))?;
        let exploration =
            prepared.exploration_view_with_theory_graph(None, &compiled_schema, &theories);
        assert!(exploration.refinement_candidates.iter().any(|candidate| {
            matches!(
                candidate.theory_obligation_ref.as_ref(),
                Some(axiograph_pathdb::kernel_ir::TheoryObligationRefIr::Constraint {
                    relation_name,
                    ..
                }) if relation_name.as_deref() == Some("Parent")
            ) && candidate.theory_subject_refs.iter().any(|subject| {
                matches!(
                    subject,
                    axiograph_pathdb::kernel_ir::TheorySubjectRefIr::Relation {
                        relation_name,
                        ..
                    } if relation_name == "Parent"
                )
            })
        }));
        Ok(())
    }

    #[test]
    fn prepared_query_runtime_refinement_with_theory_graph_preserves_typed_handles() -> Result<()> {
        let axi = r#"
module Demo

schema Demo:
  object Node
  object Supplier
  subtype Supplier < Node
  relation Flow(from: Supplier, to: Supplier)

theory DemoRules on Demo:
  constraint key Flow(from, to)

instance I of Demo:
  Supplier = {a, b}
  Flow = {(from=a, to=b)}
"#;
        let parsed = axiograph_dsl::axi_v1::parse_axi_v1(axi)?;
        let compiled_schema = axiograph_pathdb::kernel_ir::compile_schema_ir(&parsed.schemas[0]);
        let theories = parsed
            .theories
            .iter()
            .map(|theory| axiograph_pathdb::kernel_ir::compile_theory_ir(&compiled_schema, theory))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(anyhow::Error::msg)?;

        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;

        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["dst"],
              "where": [
                {
                  "kind": "fact",
                  "fact": "?f",
                  "relation": "Flow",
                  "fields": {
                    "from": "a",
                    "to": "?dst"
                  }
                }
              ],
              "limit": 5
            }"#,
        )?;

        let prepared = q.prepare_with_meta(&db, Some(&meta))?;
        let handle_id = prepared
            .exploration_view_with_theory_graph(Some("?dst"), &compiled_schema, &theories)
            .refinement_candidates
            .into_iter()
            .find(|candidate| {
                matches!(
                    candidate.kind,
                    crate::typed_refinement::RuntimeRefinementCandidateKindV1::BindFactRelation
                )
            })
            .map(|candidate| candidate.handle.id)
            .expect("expected shared runtime refinement handle");

        let applied = prepared.apply_runtime_refinement_by_id_with_theory_graph(
            &db,
            Some(&meta),
            &handle_id,
            &compiled_schema,
            &theories,
        )?;
        assert_eq!(applied.handle.id, handle_id);
        assert!(applied
            .refined_exploration
            .refinement_candidates
            .iter()
            .any(|candidate| matches!(
                candidate.theory_obligation_ref.as_ref(),
                Some(axiograph_pathdb::kernel_ir::TheoryObligationRefIr::Constraint {
                    relation_name,
                    ..
                }) if relation_name.as_deref() == Some("Flow")
            ) && candidate.theory_subject_refs.iter().any(|subject| {
                matches!(
                    subject,
                    axiograph_pathdb::kernel_ir::TheorySubjectRefIr::Relation {
                        relation_name,
                        ..
                    } if relation_name == "Flow"
                )
            })));
        Ok(())
    }

    #[test]
    fn query_ir_v1_apply_refinement_handle_appends_typed_atom() -> Result<()> {
        let axi = r#"
module Demo

schema Demo:
  object Node
  object Supplier
  subtype Supplier < Node
  relation Flow(from: Supplier, to: Supplier)

instance I of Demo:
  Supplier = {a, b}
  Flow = {(from=a, to=b)}
"#;
        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;

        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["dst"],
              "where": [
                {
                  "kind": "fact",
                  "fact": "?f",
                  "relation": "Flow",
                  "fields": {
                    "from": "a",
                    "to": "?dst"
                  }
                }
              ],
              "limit": 5
            }"#,
        )?;

        let prepared = q.prepare_with_meta(&db, Some(&meta))?;
        let candidate = prepared
            .exploration_view(Some("?dst"))
            .exploration_suggestions
            .into_iter()
            .flat_map(|suggestion| suggestion.refinement_candidates.into_iter())
            .find(|candidate| {
                candidate.kind == crate::axql::AxqlRefinementCandidateKindV1::AddTypeGuard
                    && candidate.target_type.as_deref() == Some("Demo.Supplier")
            })
            .expect("expected typed guard refinement for ?dst");

        let refined = q.apply_refinement_handle(&candidate.handle)?;
        let where_atoms = refined.where_atoms.expect("expected conjunctive body");
        assert!(where_atoms.iter().any(|atom| matches!(
            atom,
            QueryAtomIrV1::Type {
                term: QueryTermIrV1::Obj(QueryTermObjIrV1::Var { name }),
                type_name
            } if name == "?dst" && type_name == "Demo.Supplier"
        )));
        Ok(())
    }

    #[test]
    fn query_ir_v1_apply_refinement_handle_freshens_suggested_variables() -> Result<()> {
        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["prev", "dst"],
              "where": [
                { "kind": "edge", "left": "?prev", "path": "Flow", "right": "?dst" }
              ],
              "limit": 5
            }"#,
        )?;

        let handle = AxqlRefinementHandleV1::new(
            AxqlRefinementApplicationScopeV1::SingleConjunction,
            AxqlRefinementOpV1::AddEdgeAtom {
                left: AxqlRefinementTermV1::SuggestedVariable {
                    name: "?prev".to_string(),
                },
                path: "Demo.Flow".to_string(),
                right: AxqlRefinementTermV1::ExistingVariable {
                    name: "?dst".to_string(),
                },
            },
        );

        let refined = q.apply_refinement_handle(&handle)?;
        let where_atoms = refined.where_atoms.expect("expected conjunctive body");
        let added_edge = where_atoms
            .iter()
            .find_map(|atom| match atom {
                QueryAtomIrV1::Edge { left, path, right } if path == "Demo.Flow" => {
                    Some((left, right))
                }
                _ => None,
            })
            .expect("expected added refinement edge");
        match added_edge {
            (
                QueryTermIrV1::Obj(QueryTermObjIrV1::Var { name }),
                QueryTermIrV1::Obj(QueryTermObjIrV1::Var { name: right_name }),
            ) => {
                assert_ne!(name, "?prev");
                assert!(name.starts_with("?prev"));
                assert_eq!(right_name, "?dst");
            }
            other => panic!("unexpected added edge terms: {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn prepared_query_v1_apply_refinement_by_id_returns_refined_payload() -> Result<()> {
        let axi = r#"
module Demo

schema Demo:
  object Node
  object Supplier
  subtype Supplier < Node
  relation Flow(from: Supplier, to: Supplier)

instance I of Demo:
  Supplier = {a, b}
  Flow = {(from=a, to=b)}
"#;
        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;

        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["dst"],
              "where": [
                {
                  "kind": "fact",
                  "fact": "?f",
                  "relation": "Flow",
                  "fields": {
                    "from": "a",
                    "to": "?dst"
                  }
                }
              ],
              "limit": 5
            }"#,
        )?;

        let prepared = q.prepare_with_meta(&db, Some(&meta))?;
        let handle_id = prepared
            .exploration_view(Some("?dst"))
            .exploration_suggestions
            .into_iter()
            .flat_map(|suggestion| suggestion.refinement_candidates.into_iter())
            .find(|candidate| {
                candidate.kind == crate::axql::AxqlRefinementCandidateKindV1::BindFactRelation
            })
            .map(|candidate| candidate.handle.id)
            .expect("expected bind-fact refinement handle");

        let applied = prepared.apply_refinement_by_id(&db, Some(&meta), &handle_id)?;
        assert_eq!(applied.handle.id, handle_id);
        assert!(applied.introspection_after.disjunct_count >= 1);
        assert!(!applied
            .refined_exploration
            .exploration_suggestions
            .is_empty());
        assert!(applied
            .refined_query_ir_v1
            .where_atoms
            .as_ref()
            .is_some_and(
                |atoms| atoms.len() > q.where_atoms.as_ref().map(|atoms| atoms.len()).unwrap_or(0)
            ));
        Ok(())
    }

    #[test]
    fn prepared_query_v1_apply_runtime_refinement_by_id_returns_refined_payload() -> Result<()> {
        let axi = r#"
module Demo

schema Demo:
  object Node
  object Supplier
  subtype Supplier < Node
  relation Flow(from: Supplier, to: Supplier)

instance I of Demo:
  Supplier = {a, b}
  Flow = {(from=a, to=b)}
"#;
        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;

        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["dst"],
              "where": [
                {
                  "kind": "fact",
                  "fact": "?f",
                  "relation": "Flow",
                  "fields": {
                    "from": "a",
                    "to": "?dst"
                  }
                }
              ],
              "limit": 5
            }"#,
        )?;

        let prepared = q.prepare_with_meta(&db, Some(&meta))?;
        let handle_id = prepared
            .exploration_view(Some("?dst"))
            .refinement_candidates
            .into_iter()
            .find(|candidate| {
                matches!(
                    candidate.kind,
                    crate::typed_refinement::RuntimeRefinementCandidateKindV1::BindFactRelation
                )
            })
            .map(|candidate| candidate.handle.id)
            .expect("expected shared runtime refinement handle");

        let applied = prepared.apply_runtime_refinement_by_id(&db, Some(&meta), &handle_id)?;
        assert_eq!(applied.handle.id, handle_id);
        assert!(!applied.refined_exploration.refinement_candidates.is_empty());
        Ok(())
    }

    #[test]
    fn query_ir_v1_apply_refinement_rejects_disjunctions() -> Result<()> {
        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["x"],
              "disjuncts": [
                [{ "kind": "type", "term": "?x", "type": "Node" }],
                [{ "kind": "type", "term": "?x", "type": "Supplier" }]
              ],
              "limit": 5
            }"#,
        )?;
        let handle = AxqlRefinementHandleV1::new(
            AxqlRefinementApplicationScopeV1::SingleConjunction,
            AxqlRefinementOpV1::AddTypeGuard {
                term: AxqlRefinementTermV1::ExistingVariable {
                    name: "?x".to_string(),
                },
                type_name: "Demo.Node".to_string(),
            },
        );
        let err = q
            .apply_refinement_handle(&handle)
            .expect_err("expected disjunctive refinement rejection");
        assert!(err.to_string().contains("single conjunctive query"));
        Ok(())
    }

    #[test]
    fn query_ir_v1_prepare_with_meta_marks_execution_only() -> Result<()> {
        let axi = r#"
module Demo

schema S:
  object Node

instance I of S:
  Node = {a}
"#;
        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;

        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["x"],
              "where": [
                {"kind": "type", "term": "?x", "type": "Node"},
                {"kind": "attr_contains", "term": "?x", "key": "name", "needle": "a"}
              ],
              "limit": 10
            }"#,
        )?;

        let prepared = q.prepare_with_meta(&db, Some(&meta))?;
        let cert = prepared.certifiability();
        assert!(
            matches!(cert, QueryCertifiability::ExecutionOnly { .. }),
            "contains(...) should force execution-only classification"
        );
        assert!(cert.reasons().iter().any(|r| r.contains("contains")));
        let trust = prepared.trust_contract();
        assert_eq!(trust.trust_class, "execution_only");
        assert!(trust.semantic_coverage.is_some());
        assert!(prepared
            .semantic_claims()
            .iter()
            .any(|claim| claim.kind == "declared_object_type"));
        Ok(())
    }

    #[test]
    fn query_ir_v1_prepare_without_meta_can_be_enriched_later() -> Result<()> {
        let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap_or_else(|_| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
            });
        let db = crate::load_pathdb_for_cli(&repo_root.join("examples/Family.axi"))?;
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;
        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["f"],
              "where": [
                {
                  "kind": "fact",
                  "fact": "?f",
                  "relation": "Fam.Parent",
                  "fields": {
                    "child": "Jamison",
                    "parent": "Bob",
                    "ctx": "FamilyTree",
                    "time": "?t"
                  }
                }
              ],
              "limit": 1
            }"#,
        )?;

        let prepared = q.prepare_with_meta(&db, None)?;
        assert!(prepared.semantic_coverage().is_none());
        assert!(prepared.semantic_claims().is_empty());

        let enriched = prepared.trust_contract_with_meta(Some(&meta));
        assert!(enriched.semantic_coverage.is_some());
        assert!(enriched
            .semantic_claims
            .iter()
            .any(|claim| claim.kind == "declared_relation"));
        Ok(())
    }

    #[test]
    fn query_ir_v1_trust_contract_for_certifiable_query() -> Result<()> {
        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["x"],
              "where": [
                {"kind": "type", "term": "?x", "type": "Node"}
              ],
              "limit": 10
            }"#,
        )?;
        let contract = q.trust_contract()?;
        assert_eq!(contract.trust_class, "certifiable");
        assert_eq!(contract.coverage, "full_query");
        assert_eq!(contract.soundness, "certificate_available_but_not_emitted");
        assert_eq!(
            contract.claim_scope,
            "returned_rows_within_snapshot_and_context"
        );
        assert_eq!(contract.completeness_claim, "not_claimed");
        assert_eq!(contract.ontology_closure_claim, "not_claimed");
        assert_eq!(contract.scope.context, "unscoped");
        assert_eq!(contract.certifiable_disjuncts, None);
        assert_eq!(contract.execution_only_disjuncts, None);
        assert!(contract.reasons.is_empty());
        assert!(contract
            .notes
            .iter()
            .any(|note| note.contains("not a claim that all satisfying rows were returned")));
        Ok(())
    }

    #[test]
    fn query_ir_v1_trust_contract_for_mixed_query() -> Result<()> {
        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["x"],
              "disjuncts": [
                [
                  {"kind": "type", "term": "?x", "type": "Node"}
                ],
                [
                  {"kind": "attr_contains", "term": "?x", "key": "name", "needle": "a"}
                ]
              ],
              "limit": 5
            }"#,
        )?;

        let contract = q.trust_contract()?;
        assert_eq!(contract.trust_class, "mixed");
        assert_eq!(contract.coverage, "mixed_union_of_branches");
        assert_eq!(contract.completeness_claim, "not_claimed");
        assert_eq!(contract.ontology_closure_claim, "not_claimed");
        assert_eq!(contract.certifiable_disjuncts, Some(1));
        assert_eq!(contract.execution_only_disjuncts, Some(1));
        assert!(contract
            .reasons
            .iter()
            .any(|r| r.contains("cannot certify")));
        assert!(contract
            .notes
            .iter()
            .any(|note| note
                .contains("Mixed queries combine certifiable and execution-only branches")));
        Ok(())
    }

    #[test]
    fn query_ir_v1_trust_contract_with_meta_surfaces_semantic_coverage() -> Result<()> {
        let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap_or_else(|_| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
            });
        let db = crate::load_pathdb_for_cli(&repo_root.join("examples/Family.axi"))?;
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;
        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["f"],
              "where": [
                {
                  "kind": "fact",
                  "fact": "?f",
                  "relation": "Fam.Parent",
                  "fields": {
                    "child": "Jamison",
                    "parent": "Bob",
                    "ctx": "FamilyTree",
                    "time": "?t"
                  }
                }
              ],
              "limit": 1
            }"#,
        )?;

        let contract = q.trust_contract_with_meta(Some(&meta))?;
        assert!(contract.semantic_coverage.is_some());
        assert!(contract
            .semantic_claims
            .iter()
            .any(|claim| claim.kind == "declared_relation"));
        assert_eq!(contract.completeness_claim, "not_claimed");
        Ok(())
    }

    #[test]
    fn query_ir_v1_prepare_with_meta_marks_mixed_certifiability() -> Result<()> {
        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["x"],
              "disjuncts": [
                [
                  {"kind": "type", "term": "?x", "type": "Node"}
                ],
                [
                  {"kind": "attr_contains", "term": "?x", "key": "name", "needle": "a"}
                ]
              ],
              "limit": 10
            }"#,
        )?;

        let cert = q.certifiability()?;
        let (certifiable, execution_only) = cert.mixed_disjunct_counts();
        assert_eq!(certifiable, 1);
        assert_eq!(execution_only, 1);
        assert!(
            matches!(cert, QueryCertifiability::Mixed { .. }),
            "disjunction with one certifiable and one execution-only branch should be mixed"
        );
        assert!(cert.reasons().iter().any(|r| r.contains("cannot certify")));
        Ok(())
    }

    #[test]
    fn prepared_query_v1_certify_typed_with_anchor_emits_query_result_v3() -> Result<()> {
        let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap_or_else(|_| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
            });
        let axi_path = repo_root.join("examples/Family.axi");
        let axi_text = std::fs::read_to_string(&axi_path)?;
        let db = crate::load_pathdb_for_cli(&axi_path)?;
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;
        let digest = axiograph_dsl::digest::axi_digest_v1(&axi_text);

        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["p"],
              "where": [
                {
                  "kind": "fact",
                  "fact": "?f",
                  "relation": "Fam.Parent",
                  "fields": {
                    "child": "Carol",
                    "parent": "?p",
                    "ctx": "CensusData",
                    "time": "T2020"
                  }
                }
              ],
              "limit": 10
            }"#,
        )?;

        let prepared = q.prepare_with_meta(&db, Some(&meta))?;
        let cert = prepared.certify_typed_with_anchor(&db, Some(&meta), &digest)?;
        match cert.payload {
            axiograph_pathdb::certificate::CertificatePayloadV2::QueryResultV3 { proof } => {
                assert!(!proof.rows.is_empty());
            }
            other => panic!("expected query_result_v3 typed witness, got {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn prepared_query_v1_execute_answer_returns_validated_artifact() -> Result<()> {
        let axi = r#"
module Demo

schema S:
  object Node

instance I of S:
  Node = {a}
"#;
        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;

        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["x"],
              "where": [
                {"kind": "type", "term": "?x", "type": "Node"}
              ],
              "limit": 10
            }"#,
        )?;

        let mut prepared = q.prepare_with_meta(&db, Some(&meta))?;
        let answer = prepared.execute_answer(&db, Some(&meta))?;
        assert_eq!(answer.lifecycle_state_name(), "validated");
        assert_eq!(answer.trust_contract().trust_class, "certifiable");
        assert_eq!(answer.result().selected_vars, vec!["?x"]);
        assert_eq!(answer.result().rows.len(), 1);
        Ok(())
    }

    #[test]
    fn validated_query_answer_can_transition_to_certified() -> Result<()> {
        let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap_or_else(|_| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
            });
        let axi_path = repo_root.join("examples/Family.axi");
        let axi_text = std::fs::read_to_string(&axi_path)?;
        let digest = AxiDigest::from_axi_text(&axi_text);
        let db = crate::load_pathdb_for_cli(&axi_path)?;
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;

        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["p"],
              "where": [
                {
                  "kind": "fact",
                  "fact": "?f",
                  "relation": "Fam.Parent",
                  "fields": {
                    "child": "Carol",
                    "parent": "?p",
                    "ctx": "CensusData",
                    "time": "T2020"
                  }
                }
              ],
              "limit": 10
            }"#,
        )?;

        let mut prepared = q.prepare_with_meta(&db, Some(&meta))?;
        let answer = prepared.execute_answer(&db, Some(&meta))?;
        let certified =
            prepared.certify_answer_with_anchor(answer, &db, Some(&meta), digest.clone())?;

        assert_eq!(certified.lifecycle_state_name(), "certified");
        assert_eq!(certified.anchor_digest(), &digest);
        assert_eq!(
            certified.trust_contract().soundness,
            "certificate_emitted_row_soundness_unverified"
        );
        match &certified.certificate().payload {
            axiograph_pathdb::certificate::CertificatePayloadV2::QueryResultV3 { proof } => {
                assert!(!proof.rows.is_empty());
            }
            other => panic!("expected query_result_v3 typed witness, got {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn accepted_anchor_bound_prepared_query_carries_anchor_through_answer_states() -> Result<()> {
        let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap_or_else(|_| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
            });
        let axi_path = repo_root.join("examples/Family.axi");
        let axi_text = std::fs::read_to_string(&axi_path)?;
        let digest = AxiDigest::from_axi_text(&axi_text);
        let accepted_axi_anchor = AcceptedAxiAnchor::new(
            axiograph_pathdb::AcceptedSnapshotId::new("accepted:test-family"),
            digest.clone(),
        );
        let db = crate::load_pathdb_for_cli(&axi_path)?;
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;

        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["p"],
              "where": [
                {
                  "kind": "fact",
                  "fact": "?f",
                  "relation": "Fam.Parent",
                  "fields": {
                    "child": "Carol",
                    "parent": "?p",
                    "ctx": "CensusData",
                    "time": "T2020"
                  }
                }
              ],
              "limit": 10
            }"#,
        )?;

        let mut prepared = q.prepare_with_meta(&db, Some(&meta))?;
        let validated = prepared
            .bind_accepted_axi_anchor(accepted_axi_anchor.clone())
            .execute_answer(&db, Some(&meta))?;

        assert_eq!(validated.lifecycle_state_name(), "validated");
        assert_eq!(validated.accepted_axi_anchor(), &accepted_axi_anchor);
        assert_eq!(
            validated.accepted_snapshot_id().as_str(),
            "accepted:test-family"
        );
        assert_eq!(validated.axi_digest(), &digest);

        let certified = prepared
            .bind_accepted_axi_anchor(accepted_axi_anchor.clone())
            .certify_answer(validated, &db, Some(&meta))?;

        assert_eq!(certified.lifecycle_state_name(), "certified");
        assert_eq!(certified.accepted_axi_anchor(), &accepted_axi_anchor);
        assert_eq!(certified.anchor_digest(), &digest);
        match &certified.certificate().payload {
            axiograph_pathdb::certificate::CertificatePayloadV2::QueryResultV3 { proof } => {
                assert!(!proof.rows.is_empty());
            }
            other => panic!("expected query_result_v3 typed witness, got {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn accepted_anchor_bound_prepared_query_rejects_mismatched_anchor_answer() -> Result<()> {
        let axi = r#"
module Demo

schema Demo:
  object Node

instance I of Demo:
  Node = {a}
"#;
        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;

        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["x"],
              "where": [
                {"kind": "type", "term": "?x", "type": "Node"}
              ],
              "limit": 10
            }"#,
        )?;

        let mut prepared = q.prepare_with_meta(&db, Some(&meta))?;
        let answer = prepared
            .bind_accepted_axi_anchor(AcceptedAxiAnchor::new(
                axiograph_pathdb::AcceptedSnapshotId::new("accepted:a"),
                AxiDigest::new("fnv1a64:anchor-a"),
            ))
            .execute_answer(&db, Some(&meta))?;

        let err = prepared
            .bind_accepted_axi_anchor(AcceptedAxiAnchor::new(
                axiograph_pathdb::AcceptedSnapshotId::new("accepted:b"),
                AxiDigest::new("fnv1a64:anchor-b"),
            ))
            .certify_answer(answer, &db, Some(&meta))
            .expect_err("mismatched accepted anchor should be rejected");
        assert!(err
            .to_string()
            .contains("validated query answer is bound to a different accepted anchor"));
        Ok(())
    }

    #[test]
    fn prepared_query_v1_certify_answer_rejects_mismatched_query_answer() -> Result<()> {
        let axi = r#"
module Demo

schema Demo:
  object Node

instance I of Demo:
  Node = {a, b}
"#;
        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)?;
        db.build_indexes();
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;

        let q1: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["x"],
              "where": [
                {"kind": "type", "term": "?x", "type": "Node"}
              ],
              "limit": 1
            }"#,
        )?;
        let q2: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["x"],
              "where": [
                {"kind": "type", "term": "?x", "type": "Node"}
              ],
              "limit": 2
            }"#,
        )?;

        let mut prepared_one = q1.prepare_with_meta(&db, Some(&meta))?;
        let mut prepared_two = q2.prepare_with_meta(&db, Some(&meta))?;
        let answer_one = prepared_one.execute_answer(&db, Some(&meta))?;
        let err = prepared_two
            .certify_answer_with_anchor(
                answer_one,
                &db,
                Some(&meta),
                AxiDigest::new("fnv1a64:test"),
            )
            .expect_err("mismatched prepared query should be rejected");
        assert!(err
            .to_string()
            .contains("does not belong to this prepared query"));
        Ok(())
    }

    #[test]
    fn query_ir_v1_certifiability_from_ir_only_is_available() -> Result<()> {
        let q: QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["x"],
              "where": [
                {"kind": "type", "term": "?x", "type": "Node"}
              ],
              "limit": 10
            }"#,
        )?;
        assert_eq!(q.certifiability()?, QueryCertifiability::Certifiable);
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
                (var_term.clone(), type_name_strategy())
                    .prop_map(|(term, type_name)| { QueryAtomIrV1::Type { term, type_name } }),
                (var_term.clone(), path_expr_strategy(), var_term.clone(),)
                    .prop_map(|(left, path, right)| QueryAtomIrV1::Edge { left, path, right }),
                (var_term.clone(), attr_key_strategy(), attr_value_strategy())
                    .prop_map(|(term, key, value)| QueryAtomIrV1::AttrEq { term, key, value },),
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
