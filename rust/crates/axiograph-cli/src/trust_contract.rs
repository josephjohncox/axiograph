use std::collections::BTreeSet;

use axiograph_pathdb::axi_semantics::{ConstraintDecl, MetaPlaneIndex};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TrustScopeV1 {
    pub anchor: String,
    pub context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TrustContractV1 {
    pub trust_class: String,
    pub soundness: String,
    pub coverage: String,
    pub scope: TrustScopeV1,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub reasons: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certifiable_disjuncts: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_only_disjuncts: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_coverage: Option<SemanticCoverageSummaryV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub semantic_claims: Vec<SemanticClaimSummaryV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gaps: Vec<TrustGapV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QueryTrustContractV1 {
    pub trust_class: String,
    pub soundness: String,
    pub coverage: String,
    pub scope: TrustScopeV1,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub reasons: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certifiable_disjuncts: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_only_disjuncts: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_coverage: Option<SemanticCoverageSummaryV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub semantic_claims: Vec<SemanticClaimSummaryV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gaps: Vec<TrustGapV1>,
    #[serde(default = "default_query_claim_scope")]
    pub claim_scope: String,
    #[serde(default = "default_query_completeness_claim")]
    pub completeness_claim: String,
    #[serde(default = "default_query_ontology_closure_claim")]
    pub ontology_closure_claim: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SemanticCoverageSummaryV1 {
    pub coverage_scope: String,
    pub in_scope_claims: usize,
    pub runtime_visible_claims: usize,
    pub answer_relevant_claims: usize,
    pub review_only_claims: usize,
    pub unsupported_claims: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SemanticClaimSummaryV1 {
    pub subject: String,
    pub kind: String,
    pub runtime_support: String,
    pub certification: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TrustGapV1 {
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    pub detail: String,
}

fn default_query_claim_scope() -> String {
    "finite_query_denotation_within_exact_accepted_module".to_string()
}

fn default_query_completeness_claim() -> String {
    "not_claimed".to_string()
}

fn default_query_ontology_closure_claim() -> String {
    "not_claimed".to_string()
}

fn query_trust_notes(base: &TrustContractV1) -> Vec<String> {
    let mut notes = if base.soundness == "lean_verified_finite_exact_complete" {
        vec![
            "Lean checked every witness and exact equality with the declared bounded finite query denotation.".to_string(),
            "This exact finite result is not an open-world ontology-closure, evidence-exhaustiveness, approximate-search, or unrestricted-HoTT claim.".to_string(),
        ]
    } else {
        vec![
            "Execution or certificate emission without an accepted Lean receipt carries no exact-completeness claim.".to_string(),
            "This contract does not claim full ontology closure or exhaustive reasoning beyond the declared finite query fragment.".to_string(),
        ]
    };

    match base.trust_class.as_str() {
        "mixed" => notes.push(
            "Mixed queries combine certifiable and execution-only branches; only the certifiable fragment is inside the current certificate subset.".to_string(),
        ),
        "execution_only" => notes.push(
            "Execution-only queries have no certificate-backed soundness claim for returned rows; inspect `reasons` for the unsupported operators or context shapes.".to_string(),
        ),
        _ => {}
    }

    if let Some(semantic) = base.semantic_coverage.as_ref() {
        notes.push(format!(
            "Semantic coverage is currently scoped to relations/types mentioned directly by the query; this reports in-scope ontology surfaces ({}) rather than full-theory closure.",
            semantic.coverage_scope
        ));
    }

    notes
}

impl From<TrustContractV1> for QueryTrustContractV1 {
    fn from(base: TrustContractV1) -> Self {
        let finite_exact = base.soundness == "lean_verified_finite_exact_complete";
        QueryTrustContractV1 {
            claim_scope: default_query_claim_scope(),
            completeness_claim: if finite_exact {
                "exact_for_declared_finite_decidable_fragment".to_string()
            } else {
                default_query_completeness_claim()
            },
            ontology_closure_claim: default_query_ontology_closure_claim(),
            notes: query_trust_notes(&base),
            trust_class: base.trust_class,
            soundness: base.soundness,
            coverage: base.coverage,
            scope: base.scope,
            reasons: base.reasons,
            certifiable_disjuncts: base.certifiable_disjuncts,
            execution_only_disjuncts: base.execution_only_disjuncts,
            semantic_coverage: base.semantic_coverage,
            semantic_claims: base.semantic_claims,
            gaps: base.gaps,
        }
    }
}

#[derive(Default)]
struct QuerySemanticSurface {
    schema_names: BTreeSet<String>,
    relation_names: BTreeSet<String>,
    type_names: BTreeSet<String>,
    has_path_atoms: bool,
}

fn unqualified_name(raw: &str) -> &str {
    raw.rsplit('.').next().unwrap_or(raw)
}

fn collect_regex_labels(regex: &crate::axql::AxqlRegex, out: &mut BTreeSet<String>) {
    match regex {
        crate::axql::AxqlRegex::Epsilon => {}
        crate::axql::AxqlRegex::Rel(label) => {
            out.insert(label.clone());
        }
        crate::axql::AxqlRegex::Seq(parts) | crate::axql::AxqlRegex::Alt(parts) => {
            for part in parts {
                collect_regex_labels(part, out);
            }
        }
        crate::axql::AxqlRegex::Star(inner)
        | crate::axql::AxqlRegex::Plus(inner)
        | crate::axql::AxqlRegex::Opt(inner) => collect_regex_labels(inner, out),
    }
}

fn query_semantic_surface(query: &crate::axql::AxqlQuery) -> QuerySemanticSurface {
    let mut surface = QuerySemanticSurface::default();
    for disjunct in &query.disjuncts {
        for atom in disjunct {
            match atom {
                crate::axql::AxqlAtom::Type { type_name, .. } => {
                    surface.type_names.insert(type_name.clone());
                    if let Some((schema, local)) = type_name.rsplit_once('.') {
                        surface.schema_names.insert(schema.to_string());
                        surface.type_names.insert(local.to_string());
                    }
                }
                crate::axql::AxqlAtom::Edge { path, .. } => {
                    surface.has_path_atoms = true;
                    collect_regex_labels(&path.regex, &mut surface.relation_names);
                }
                crate::axql::AxqlAtom::Fact { relation, .. } => {
                    surface.relation_names.insert(relation.clone());
                    if let Some((schema, local)) = relation.rsplit_once('.') {
                        surface.schema_names.insert(schema.to_string());
                        surface.relation_names.insert(local.to_string());
                    }
                }
                crate::axql::AxqlAtom::HasOut { rels, .. } => {
                    surface.has_path_atoms = true;
                    for rel in rels {
                        surface.relation_names.insert(rel.clone());
                    }
                }
                crate::axql::AxqlAtom::Shape {
                    type_name, rels, ..
                } => {
                    if let Some(type_name) = type_name {
                        surface.type_names.insert(type_name.clone());
                        if let Some((schema, local)) = type_name.rsplit_once('.') {
                            surface.schema_names.insert(schema.to_string());
                            surface.type_names.insert(local.to_string());
                        }
                    }
                    if !rels.is_empty() {
                        surface.has_path_atoms = true;
                    }
                    for rel in rels {
                        surface.relation_names.insert(rel.clone());
                    }
                }
                crate::axql::AxqlAtom::AttrEq { .. }
                | crate::axql::AxqlAtom::AttrContains { .. }
                | crate::axql::AxqlAtom::AttrFts { .. }
                | crate::axql::AxqlAtom::AttrFuzzy { .. }
                | crate::axql::AxqlAtom::Attrs { .. } => {}
            }
        }
    }
    surface
}

fn semantic_claim_for_declared_type(schema_name: &str, type_name: &str) -> SemanticClaimSummaryV1 {
    SemanticClaimSummaryV1 {
        subject: format!("{schema_name}.{type_name}"),
        kind: "declared_object_type".to_string(),
        runtime_support: "answer_relevant_runtime_typecheck".to_string(),
        certification: "not_claimed".to_string(),
        status: "in_scope_runtime".to_string(),
    }
}

fn semantic_claim_for_declared_relation(
    schema_name: &str,
    relation_name: &str,
) -> SemanticClaimSummaryV1 {
    SemanticClaimSummaryV1 {
        subject: format!("{schema_name}.{relation_name}"),
        kind: "declared_relation".to_string(),
        runtime_support: "answer_relevant_runtime_typecheck".to_string(),
        certification: "not_claimed".to_string(),
        status: "in_scope_runtime".to_string(),
    }
}

fn semantic_claim_for_constraint(
    schema_name: &str,
    relation_name: &str,
    decl: &ConstraintDecl,
) -> SemanticClaimSummaryV1 {
    let (kind, runtime_support, certification, status) = match decl {
        ConstraintDecl::Functional { .. } => (
            "functional_constraint",
            "execution_acceleration_only",
            "not_claimed",
            "in_scope_execution_acceleration",
        ),
        ConstraintDecl::Key { .. } => (
            "key_constraint",
            "execution_acceleration_only",
            "not_claimed",
            "in_scope_execution_acceleration",
        ),
        ConstraintDecl::Typing { .. } => (
            "typing_constraint",
            "answer_relevant_runtime_typecheck",
            "not_claimed",
            "in_scope_runtime",
        ),
        ConstraintDecl::AtMost { .. } => (
            "at_most_constraint",
            "stored_for_review",
            "not_claimed",
            "in_scope_review_only",
        ),
        ConstraintDecl::SymmetricWhereIn { .. } => (
            "symmetric_where_in_constraint",
            "stored_for_review",
            "not_claimed",
            "in_scope_review_only",
        ),
        ConstraintDecl::Symmetric { .. } => (
            "symmetric_constraint",
            "stored_for_review",
            "not_claimed",
            "in_scope_review_only",
        ),
        ConstraintDecl::Transitive { .. } => (
            "transitive_constraint",
            "stored_for_review",
            "not_claimed",
            "in_scope_review_only",
        ),
        ConstraintDecl::NamedBlock { .. } => (
            "named_block_constraint",
            "stored_for_review",
            "out_of_scope",
            "in_scope_review_only",
        ),
        ConstraintDecl::Unknown { .. } => (
            "unknown_constraint",
            "stored_for_review",
            "out_of_scope",
            "in_scope_review_only",
        ),
    };

    SemanticClaimSummaryV1 {
        subject: format!("{schema_name}.{relation_name}"),
        kind: kind.to_string(),
        runtime_support: runtime_support.to_string(),
        certification: certification.to_string(),
        status: status.to_string(),
    }
}

fn semantic_claim_for_rewrite_rule(schema_name: &str, theory_name: &str) -> SemanticClaimSummaryV1 {
    SemanticClaimSummaryV1 {
        subject: format!("{schema_name}::{theory_name}"),
        kind: "rewrite_rule".to_string(),
        runtime_support: "runtime_rewrite_helper".to_string(),
        certification: "certificate_emittable_subset".to_string(),
        status: "in_scope_runtime".to_string(),
    }
}

fn semantic_claim_for_named_block(schema_name: &str, theory_name: &str) -> SemanticClaimSummaryV1 {
    SemanticClaimSummaryV1 {
        subject: format!("{schema_name}::{theory_name}"),
        kind: "named_block_rule".to_string(),
        runtime_support: "stored_for_review".to_string(),
        certification: "out_of_scope".to_string(),
        status: "in_scope_review_only".to_string(),
    }
}

fn query_semantic_claims(
    query: &crate::axql::AxqlQuery,
    meta: &MetaPlaneIndex,
) -> (
    Option<SemanticCoverageSummaryV1>,
    Vec<SemanticClaimSummaryV1>,
    Vec<TrustGapV1>,
) {
    let surface = query_semantic_surface(query);
    if meta.schemas.is_empty() {
        return (
            Some(SemanticCoverageSummaryV1 {
                coverage_scope: "relations_and_types_mentioned_by_query_only".to_string(),
                in_scope_claims: 0,
                runtime_visible_claims: 0,
                answer_relevant_claims: 0,
                review_only_claims: 0,
                unsupported_claims: 0,
            }),
            Vec::new(),
            vec![TrustGapV1 {
                code: "no_meta_plane_loaded".to_string(),
                subject: None,
                detail: "No canonical schema/theory meta-plane was available, so ontology/business-rule coverage could not be attached to this query trust contract.".to_string(),
            }],
        );
    }

    let mut claims = Vec::new();
    for (schema_name, schema) in &meta.schemas {
        let mut schema_in_scope = surface.schema_names.contains(schema_name);

        for type_name in &surface.type_names {
            let local = unqualified_name(type_name);
            if schema.object_types.contains(local) {
                schema_in_scope = true;
                claims.push(semantic_claim_for_declared_type(schema_name, local));
            }
        }

        let mut matched_relations = BTreeSet::new();
        for relation_name in &surface.relation_names {
            let local = unqualified_name(relation_name);
            if schema.relation_decls.contains_key(local) {
                schema_in_scope = true;
                matched_relations.insert(local.to_string());
            }
        }

        for relation_name in &matched_relations {
            claims.push(semantic_claim_for_declared_relation(
                schema_name,
                relation_name,
            ));
            if let Some(decls) = schema.constraints_by_relation.get(relation_name) {
                for decl in decls {
                    claims.push(semantic_claim_for_constraint(
                        schema_name,
                        relation_name,
                        decl,
                    ));
                }
            }
        }

        if schema_in_scope && surface.has_path_atoms {
            for theory_name in schema.rewrite_rules_by_theory.keys() {
                claims.push(semantic_claim_for_rewrite_rule(schema_name, theory_name));
            }
        }

        if schema_in_scope {
            for theory_name in schema.named_block_constraints_by_theory.keys() {
                claims.push(semantic_claim_for_named_block(schema_name, theory_name));
            }
        }
    }

    claims.sort_by(|a, b| {
        a.subject
            .cmp(&b.subject)
            .then(a.kind.cmp(&b.kind))
            .then(a.status.cmp(&b.status))
    });
    claims.dedup();

    let runtime_visible_claims = claims
        .iter()
        .filter(|claim| claim.status != "in_scope_review_only" && claim.status != "unsupported")
        .count();
    let answer_relevant_claims = claims
        .iter()
        .filter(|claim| claim.runtime_support == "answer_relevant_runtime_typecheck")
        .count();
    let review_only_claims = claims
        .iter()
        .filter(|claim| claim.status == "in_scope_review_only")
        .count();
    let unsupported_claims = claims
        .iter()
        .filter(|claim| claim.status == "unsupported")
        .count();

    let mut gaps = Vec::new();
    if claims.is_empty() {
        gaps.push(TrustGapV1 {
            code: "no_in_scope_ontology_claims".to_string(),
            subject: None,
            detail: "The query did not resolve to any relation/type surfaces with indexed ontology metadata in the current snapshot.".to_string(),
        });
    }
    if review_only_claims > 0 {
        gaps.push(TrustGapV1 {
            code: "review_only_semantic_surfaces".to_string(),
            subject: None,
            detail: format!(
                "{review_only_claims} in-scope ontology/business-rule surfaces are review-only and do not currently contribute a runtime or certificate-backed query claim."
            ),
        });
    }
    if claims
        .iter()
        .any(|claim| claim.runtime_support == "execution_acceleration_only")
    {
        gaps.push(TrustGapV1 {
            code: "execution_acceleration_is_not_answer_soundness".to_string(),
            subject: None,
            detail: "Some in-scope rule surfaces currently support planning/index selection only; that should not be read as a direct business-rule proof for returned answers.".to_string(),
        });
    }
    if surface.has_path_atoms
        && !claims
            .iter()
            .any(|claim| claim.kind == "rewrite_rule" || claim.kind == "declared_relation")
    {
        gaps.push(TrustGapV1 {
            code: "path_query_without_runtime_rewrite_surface".to_string(),
            subject: None,
            detail: "This path-oriented query did not resolve to any rewrite/helper semantic surfaces in the current meta-plane summary.".to_string(),
        });
    }

    (
        Some(SemanticCoverageSummaryV1 {
            coverage_scope: "relations_and_types_mentioned_by_query_only".to_string(),
            in_scope_claims: claims.len(),
            runtime_visible_claims,
            answer_relevant_claims,
            review_only_claims,
            unsupported_claims,
        }),
        claims,
        gaps,
    )
}

#[allow(dead_code)]
pub fn query_context_scope_label(query: &crate::axql::AxqlQuery) -> String {
    match query.contexts.len() {
        0 => "unscoped".to_string(),
        1 => "single_context".to_string(),
        _ => "multi_context".to_string(),
    }
}

#[allow(dead_code)]
pub fn query_trust_contract(
    query: &crate::axql::AxqlQuery,
    certifiability: &crate::axql::QueryCertifiability,
    certificate_emitted: bool,
    certificate_verified: Option<bool>,
) -> TrustContractV1 {
    let (trust_class, coverage, certifiable_disjuncts, execution_only_disjuncts) =
        match certifiability {
            crate::axql::QueryCertifiability::Certifiable => (
                "certifiable".to_string(),
                "full_query".to_string(),
                None,
                None,
            ),
            crate::axql::QueryCertifiability::Mixed {
                certifiable_disjuncts,
                execution_only_disjuncts,
                ..
            } => (
                "mixed".to_string(),
                "mixed_union_of_branches".to_string(),
                Some(*certifiable_disjuncts),
                Some(*execution_only_disjuncts),
            ),
            crate::axql::QueryCertifiability::ExecutionOnly { .. } => (
                "execution_only".to_string(),
                "runtime_only".to_string(),
                None,
                None,
            ),
        };

    let soundness = if certificate_verified == Some(true) {
        "lean_verified_finite_exact_complete".to_string()
    } else if certificate_emitted {
        "finite_exact_certificate_emitted_unverified".to_string()
    } else if matches!(
        certifiability,
        crate::axql::QueryCertifiability::Certifiable
    ) {
        "certificate_available_but_not_emitted".to_string()
    } else {
        "runtime_only_no_certificate_claim".to_string()
    };

    TrustContractV1 {
        trust_class,
        soundness,
        coverage,
        scope: TrustScopeV1 {
            anchor: "snapshot_scoped".to_string(),
            context: query_context_scope_label(query),
        },
        reasons: certifiability.reasons().to_vec(),
        certifiable_disjuncts,
        execution_only_disjuncts,
        semantic_coverage: None,
        semantic_claims: Vec::new(),
        gaps: Vec::new(),
    }
}

#[allow(dead_code)]
pub fn query_trust_contract_with_meta(
    query: &crate::axql::AxqlQuery,
    certifiability: &crate::axql::QueryCertifiability,
    certificate_emitted: bool,
    certificate_verified: Option<bool>,
    meta: Option<&MetaPlaneIndex>,
) -> TrustContractV1 {
    let mut contract = query_trust_contract(
        query,
        certifiability,
        certificate_emitted,
        certificate_verified,
    );
    if let Some(meta) = meta {
        let (semantic_coverage, semantic_claims, gaps) = query_semantic_claims(query, meta);
        contract.semantic_coverage = semantic_coverage;
        contract.semantic_claims = semantic_claims;
        contract.gaps = gaps;
    }
    contract
}

#[allow(dead_code)]
pub fn query_user_visible_trust_contract(
    query: &crate::axql::AxqlQuery,
    certifiability: &crate::axql::QueryCertifiability,
    certificate_emitted: bool,
    certificate_verified: Option<bool>,
) -> QueryTrustContractV1 {
    query_trust_contract(
        query,
        certifiability,
        certificate_emitted,
        certificate_verified,
    )
    .into()
}

#[allow(dead_code)]
pub fn query_user_visible_trust_contract_with_meta(
    query: &crate::axql::AxqlQuery,
    certifiability: &crate::axql::QueryCertifiability,
    certificate_emitted: bool,
    certificate_verified: Option<bool>,
    meta: Option<&MetaPlaneIndex>,
) -> QueryTrustContractV1 {
    query_trust_contract_with_meta(
        query,
        certifiability,
        certificate_emitted,
        certificate_verified,
        meta,
    )
    .into()
}

#[allow(dead_code)]
pub fn certificate_trust_contract(certificate_verified: Option<bool>) -> TrustContractV1 {
    TrustContractV1 {
        trust_class: "certificate".to_string(),
        soundness: if certificate_verified == Some(true) {
            "lean_verified_certificate".to_string()
        } else {
            "certificate_emitted_unverified".to_string()
        },
        coverage: "certificate_payload".to_string(),
        scope: TrustScopeV1 {
            anchor: "snapshot_scoped".to_string(),
            context: "not_applicable".to_string(),
        },
        reasons: Vec::new(),
        certifiable_disjuncts: None,
        execution_only_disjuncts: None,
        semantic_coverage: None,
        semantic_claims: Vec::new(),
        gaps: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn query_user_visible_trust_contract_surfaces_explicit_non_claims() -> anyhow::Result<()> {
        let query = crate::axql::parse_axql_query("select ?x where ?x : Node limit 3")?;
        let trust = query_user_visible_trust_contract(&query, &query.certifiability(), false, None);

        assert_eq!(
            trust.claim_scope,
            "finite_query_denotation_within_exact_accepted_module"
        );
        assert_eq!(trust.completeness_claim, "not_claimed");
        assert_eq!(trust.ontology_closure_claim, "not_claimed");
        assert!(trust
            .notes
            .iter()
            .any(|note| note.contains("no exact-completeness claim")));
        assert!(trust
            .notes
            .iter()
            .any(|note| note.contains("does not claim full ontology closure")));
        Ok(())
    }

    #[test]
    fn query_user_visible_trust_contract_marks_execution_only_soundness_gap() -> anyhow::Result<()>
    {
        let query = crate::axql::parse_axql_query(
            r#"select ?x where contains(?x, "name", "alice") limit 3"#,
        )?;
        let trust = query_user_visible_trust_contract(&query, &query.certifiability(), false, None);

        assert_eq!(trust.trust_class, "execution_only");
        assert!(trust
            .notes
            .iter()
            .any(|note| note.contains("no certificate-backed soundness claim")));
        Ok(())
    }

    #[test]
    fn query_user_visible_trust_contract_with_meta_surfaces_semantic_coverage() -> anyhow::Result<()>
    {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
        let db = crate::load_pathdb_for_cli(&repo_root.join("examples/Family.axi"))?;
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;
        let query = crate::axql::parse_axql_query(
            "select ?f where ?f = Fam.Parent(child=Jamison, parent=Bob, ctx=FamilyTree, time=?t) limit 1",
        )?;
        let trust = query_user_visible_trust_contract_with_meta(
            &query,
            &query.certifiability(),
            false,
            None,
            Some(&meta),
        );

        let semantic = trust.semantic_coverage.as_ref().expect("semantic coverage");
        assert_eq!(
            semantic.coverage_scope,
            "relations_and_types_mentioned_by_query_only"
        );
        assert!(semantic.in_scope_claims > 0);
        assert!(trust
            .semantic_claims
            .iter()
            .any(|claim| claim.kind == "declared_relation"));
        assert_eq!(trust.completeness_claim, "not_claimed");
        assert_eq!(trust.ontology_closure_claim, "not_claimed");
        Ok(())
    }

    #[test]
    fn trusted_contract_json_rejects_floating_point_extensions() -> anyhow::Result<()> {
        let query = crate::axql::parse_axql_query("select ?x where ?x : Node limit 3")?;
        let trust = query_user_visible_trust_contract(&query, &query.certifiability(), false, None);

        let mut unknown_float = serde_json::to_value(&trust)?;
        unknown_float["confidence"] = serde_json::json!(0.5);
        assert!(serde_json::from_value::<QueryTrustContractV1>(unknown_float).is_err());

        let mut integer_field_float = serde_json::to_value(SemanticCoverageSummaryV1 {
            coverage_scope: "finite_query".to_string(),
            in_scope_claims: 1,
            runtime_visible_claims: 1,
            answer_relevant_claims: 1,
            review_only_claims: 0,
            unsupported_claims: 0,
        })?;
        integer_field_float["runtime_visible_claims"] = serde_json::json!(1.5);
        assert!(serde_json::from_value::<SemanticCoverageSummaryV1>(integer_field_float).is_err());
        Ok(())
    }
}
