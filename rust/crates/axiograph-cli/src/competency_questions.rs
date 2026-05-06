//! Competency question generation and translation helpers.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::Path;

use axiograph_pathdb::axi_semantics::MetaPlaneIndex;
use axiograph_pathdb::kernel_ir::{CompiledSchemaIr, TheoryIr};
use axiograph_pathdb::PathDB;

use crate::llm::{GeneratedQuery, LlmState};
use crate::predictive_proposals::CompetencyQuestionV1;

#[derive(Debug, Clone)]
pub struct CompetencyQuestionOptions {
    pub include_types: bool,
    pub include_relations: bool,
    pub include_entity: bool,
    pub min_rows: usize,
    pub weight: f64,
    pub contexts: Vec<String>,
}

impl Default for CompetencyQuestionOptions {
    fn default() -> Self {
        Self {
            include_types: true,
            include_relations: true,
            include_entity: false,
            min_rows: 1,
            weight: 1.0,
            contexts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompetencyQuestionPrompt {
    pub name: Option<String>,
    pub question: Option<String>,
    pub query: Option<String>,
    pub min_rows: Option<usize>,
    pub weight: Option<f64>,
    pub contexts: Vec<String>,
}

pub fn generate_from_schema(
    db: &PathDB,
    options: &CompetencyQuestionOptions,
) -> Result<Vec<CompetencyQuestionV1>> {
    let meta = MetaPlaneIndex::from_db(db)?;
    let mut schema_names: Vec<String> = meta.schemas.keys().cloned().collect();
    schema_names.sort();

    let min_rows = if options.min_rows == 0 {
        1
    } else {
        options.min_rows
    };
    let weight = if options.weight <= 0.0 {
        1.0
    } else {
        options.weight
    };

    let mut out: Vec<CompetencyQuestionV1> = Vec::new();

    for schema_name in schema_names {
        let Some(schema) = meta.schemas.get(&schema_name) else {
            continue;
        };

        if options.include_types {
            let mut types: Vec<String> = schema.object_types.iter().cloned().collect();
            types.sort();
            for ty in types {
                if ty == "Entity" && !options.include_entity {
                    continue;
                }
                let name = format!("type::{schema_name}::{ty}");
                let query = format!("select ?x where ?x is {schema_name}.{ty} limit 1");
                let question = format!("Find a {ty} instance in schema {schema_name}.");
                out.push(CompetencyQuestionV1 {
                    name,
                    question: Some(question),
                    authoring: None,
                    query,
                    min_rows,
                    weight,
                    contexts: options.contexts.clone(),
                });
            }
        }

        if options.include_relations {
            let mut relations: Vec<_> = schema.relation_decls.values().collect();
            relations.sort_by(|a, b| a.name.cmp(&b.name));
            for rel in relations {
                if rel.fields.is_empty() {
                    continue;
                }
                let mut fields: Vec<String> = Vec::new();
                for (idx, field) in rel.fields.iter().enumerate() {
                    let var = format!("?v{idx}");
                    fields.push(format!("{}={}", field.field_name, var));
                }
                let field_sig = fields.join(", ");
                let name = format!("rel::{schema_name}::{}", rel.name);
                let query = format!(
                    "select ?f where ?f = {schema_name}.{}({field_sig}) limit 1",
                    rel.name
                );
                let question = format!(
                    "Find a {schema_name}.{} fact with fields [{}].",
                    rel.name,
                    rel.fields
                        .iter()
                        .map(|f| f.field_name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                out.push(CompetencyQuestionV1 {
                    name,
                    question: Some(question),
                    authoring: None,
                    query,
                    min_rows,
                    weight,
                    contexts: options.contexts.clone(),
                });
            }
        }
    }

    Ok(out)
}

pub fn load_question_prompts(path: &Path) -> Result<Vec<CompetencyQuestionPrompt>> {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    if ext.eq_ignore_ascii_case("json") {
        let text = fs::read_to_string(path)?;
        let value: Value = serde_json::from_str(&text)?;
        return prompts_from_json(value);
    }

    let text = fs::read_to_string(path)?;
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        out.push(CompetencyQuestionPrompt {
            name: None,
            question: Some(trimmed.to_string()),
            query: None,
            min_rows: None,
            weight: None,
            contexts: Vec::new(),
        });
    }
    Ok(out)
}

pub fn prompts_to_competency_questions(
    db: &PathDB,
    llm: Option<&LlmState>,
    prompts: &[CompetencyQuestionPrompt],
    defaults: &CompetencyQuestionOptions,
) -> Result<Vec<CompetencyQuestionV1>> {
    let mut out = Vec::new();
    let min_rows_default = if defaults.min_rows == 0 {
        1
    } else {
        defaults.min_rows
    };
    let weight_default = if defaults.weight <= 0.0 {
        1.0
    } else {
        defaults.weight
    };

    for (idx, prompt) in prompts.iter().enumerate() {
        let name = prompt
            .name
            .clone()
            .unwrap_or_else(|| format!("cq_{}", idx + 1));
        let min_rows = prompt.min_rows.unwrap_or(min_rows_default);
        let weight = prompt.weight.unwrap_or(weight_default);
        let contexts = if prompt.contexts.is_empty() {
            defaults.contexts.clone()
        } else {
            prompt.contexts.clone()
        };

        if let Some(query) = prompt.query.clone() {
            out.push(CompetencyQuestionV1 {
                name,
                question: prompt.question.clone(),
                authoring: None,
                query,
                min_rows,
                weight,
                contexts,
            });
            continue;
        }

        let question = prompt.question.clone().ok_or_else(|| {
            anyhow!("competency question `{name}` missing question text and query")
        })?;
        let llm = llm.ok_or_else(|| {
            anyhow!("competency question `{name}` requires an LLM backend (none configured)")
        })?;
        let generated = llm.generate_query(db, &question)?;
        let query = match generated {
            GeneratedQuery::QueryIrV1(ir) => ir.to_axql_text()?,
        };
        out.push(CompetencyQuestionV1 {
            name,
            question: Some(question),
            authoring: None,
            query,
            min_rows,
            weight,
            contexts,
        });
    }

    Ok(out)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompetencyQuestionTrustV1 {
    pub trust_class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<String>,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_coverage: Option<crate::trust_contract::SemanticCoverageSummaryV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gaps: Vec<crate::trust_contract::TrustGapV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompetencyQuestionEvaluationV1 {
    pub name: String,
    pub rows: usize,
    pub min_rows: usize,
    pub satisfied: bool,
    pub weight: f64,
    pub cost: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepared_query: Option<crate::query_ir::PreparedQueryMetadataV1>,
    pub trust: CompetencyQuestionTrustV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refinement_candidates: Vec<crate::typed_refinement::RuntimeRefinementCandidateV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompetencyCoverageWithTrustV1 {
    pub total: usize,
    pub satisfied: usize,
    pub coverage: f64,
    pub cost: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_theory_check: Option<crate::runtime_theory_check::RuntimeTheoryCheckSummaryV1>,
    #[serde(default)]
    pub questions: Vec<CompetencyQuestionEvaluationV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompetencyQuestionRefinementApplyResultV1 {
    pub question_name: String,
    pub base_question: CompetencyQuestionV1,
    pub refined_question: CompetencyQuestionV1,
    pub query_apply: crate::query_ir::QueryRefinementApplyResultV1,
}

fn runtime_query_refinement_handle_for_competency_question(
    question: &CompetencyQuestionV1,
    handle: &crate::typed_refinement::RuntimeRefinementHandleV1,
) -> Result<crate::typed_refinement::RuntimeRefinementHandleV1> {
    handle.validate()?;
    match &handle.payload {
        crate::typed_refinement::RuntimeRefinementPayloadV1::CompetencyQuestionRepair {
            question_name,
            handle,
        } => {
            if question_name != &question.name {
                return Err(anyhow!(
                    "competency-question refinement handle `{}` targets `{}` but question is `{}`",
                    handle.id,
                    question_name,
                    question.name
                ));
            }
            Ok(crate::typed_refinement::RuntimeRefinementHandleV1::from_query(handle.clone()))
        }
        crate::typed_refinement::RuntimeRefinementPayloadV1::Query { .. } => Ok(handle.clone()),
        _ => Err(anyhow!(
            "runtime refinement handle `{}` is not a competency-question/query refinement",
            handle.id
        )),
    }
}

fn wrap_runtime_refinement_candidate_for_competency_question(
    question_name: &str,
    candidate: crate::typed_refinement::RuntimeRefinementCandidateV1,
) -> Result<crate::typed_refinement::RuntimeRefinementCandidateV1> {
    let crate::typed_refinement::RuntimeRefinementCandidateV1 {
        kind,
        summary,
        handle,
        preview_fragment: _,
        relation,
        schema,
        role,
        target_type,
        target_box,
        question_name: _,
        obligation_id,
        artifact_id,
        resolution,
        theory_obligation_ref,
        theory_subject_refs,
        theory_subject_ref,
    } = candidate;
    let crate::typed_refinement::RuntimeRefinementPayloadV1::Query { handle } = handle.payload
    else {
        return Err(anyhow!(
            "runtime refinement candidate `{summary}` is not query-scoped"
        ));
    };
    let handle = crate::typed_refinement::RuntimeRefinementHandleV1::new_competency_question_repair(
        question_name.to_string(),
        handle,
    );
    Ok(crate::typed_refinement::RuntimeRefinementCandidateV1 {
        kind,
        summary: format!("repair competency question `{question_name}`: {summary}"),
        preview_fragment: handle.preview_fragment(),
        handle,
        relation,
        schema,
        role,
        target_type,
        target_box,
        question_name: Some(question_name.to_string()),
        obligation_id,
        artifact_id,
        resolution,
        theory_obligation_ref,
        theory_subject_refs,
        theory_subject_ref,
    })
}

#[allow(dead_code)]
pub fn apply_runtime_refinement_handle_to_competency_question_result(
    db: &PathDB,
    meta: Option<&MetaPlaneIndex>,
    question: &CompetencyQuestionV1,
    handle: &crate::typed_refinement::RuntimeRefinementHandleV1,
) -> Result<CompetencyQuestionRefinementApplyResultV1> {
    let runtime_query_handle =
        runtime_query_refinement_handle_for_competency_question(question, handle)?;
    let axql = crate::axql::parse_axql_query(&question.query)?;
    let query_ir = crate::query_ir::QueryIrV1::from_axql_query(&axql);
    let prepared = query_ir.prepare_with_meta(db, meta)?;
    let query_apply = prepared.apply_runtime_refinement_handle(db, meta, &runtime_query_handle)?;
    let mut refined_question = question.clone();
    refined_question.query = query_apply.refined_query_ir_v1.to_axql_text()?;
    Ok(CompetencyQuestionRefinementApplyResultV1 {
        question_name: question.name.clone(),
        base_question: question.clone(),
        refined_question,
        query_apply,
    })
}

#[allow(dead_code)]
pub fn apply_runtime_refinement_handle_to_competency_question_result_with_theory_graph(
    db: &PathDB,
    meta: Option<&MetaPlaneIndex>,
    question: &CompetencyQuestionV1,
    handle: &crate::typed_refinement::RuntimeRefinementHandleV1,
    compiled_schema: &CompiledSchemaIr,
    theories: &[TheoryIr],
) -> Result<CompetencyQuestionRefinementApplyResultV1> {
    let runtime_query_handle =
        runtime_query_refinement_handle_for_competency_question(question, handle)?;
    let axql = crate::axql::parse_axql_query(&question.query)?;
    let query_ir = crate::query_ir::QueryIrV1::from_axql_query(&axql);
    let prepared = query_ir.prepare_with_meta(db, meta)?;
    let query_apply = prepared.apply_runtime_refinement_handle_with_theory_graph(
        db,
        meta,
        &runtime_query_handle,
        compiled_schema,
        theories,
    )?;
    let mut refined_question = question.clone();
    refined_question.query = query_apply.refined_query_ir_v1.to_axql_text()?;
    Ok(CompetencyQuestionRefinementApplyResultV1 {
        question_name: question.name.clone(),
        base_question: question.clone(),
        refined_question,
        query_apply,
    })
}

#[allow(dead_code)]
pub fn apply_runtime_refinement_handle_to_competency_question(
    db: &PathDB,
    meta: Option<&MetaPlaneIndex>,
    question: &CompetencyQuestionV1,
    handle: &crate::typed_refinement::RuntimeRefinementHandleV1,
) -> Result<CompetencyQuestionV1> {
    Ok(
        apply_runtime_refinement_handle_to_competency_question_result(db, meta, question, handle)?
            .refined_question,
    )
}

fn normalized_competency_query(
    q: &CompetencyQuestionV1,
) -> Result<(crate::axql::AxqlQuery, usize, f64)> {
    let mut query = crate::axql::parse_axql_query(&q.query)?;
    if !q.contexts.is_empty() {
        let mut ctxs: Vec<crate::axql::AxqlContextSpec> = Vec::new();
        for raw in &q.contexts {
            if let Ok(id) = raw.parse::<u32>() {
                ctxs.push(crate::axql::AxqlContextSpec::EntityId(id));
            } else {
                ctxs.push(crate::axql::AxqlContextSpec::Name(raw.to_string()));
            }
        }
        query.contexts = ctxs;
    }
    let min_rows = if q.min_rows == 0 { 1 } else { q.min_rows };
    let limit = min_rows.min(1000);
    if query.limit == 0 || query.limit > limit {
        query.limit = limit;
    }
    let weight = if q.weight <= 0.0 { 1.0 } else { q.weight };
    Ok((query, min_rows, weight))
}

fn unresolved_authored_competency_question_evaluation(
    q: &CompetencyQuestionV1,
) -> CompetencyQuestionEvaluationV1 {
    let min_rows = if q.min_rows == 0 { 1 } else { q.min_rows };
    let weight = if q.weight <= 0.0 { 1.0 } else { q.weight };
    CompetencyQuestionEvaluationV1 {
        name: q.name.clone(),
        rows: 0,
        min_rows,
        satisfied: false,
        weight,
        cost: weight,
        prepared_query: None,
        trust: CompetencyQuestionTrustV1 {
            trust_class: "unresolved_authoring".to_string(),
            coverage: Some("not_executable_until_lowered_to_typed_query".to_string()),
            reasons: vec![
                "competency question has authored intent but no executable typed query lowering"
                    .to_string(),
            ],
            notes: vec![
                "Use `expect: exists Schema.Rel(role=value, ...)` or `about: Schema.Rel` plus `given: role=value` hints to let the `.cq` loader derive an executable query.".to_string(),
                "Alternatively keep an explicit `axql:` field as a lowering/debug fixture, not as the primary authoring surface.".to_string(),
            ],
            semantic_coverage: None,
            gaps: vec![crate::trust_contract::TrustGapV1 {
                code: "cq_missing_executable_lowering".to_string(),
                subject: Some(q.name.clone()),
                detail: "authored competency question needs a typed lowering before it can satisfy strict CQ or promotion gates".to_string(),
            }],
        },
        refinement_candidates: Vec::new(),
    }
}

pub fn evaluate_competency_questions(
    db: &PathDB,
    questions: &[CompetencyQuestionV1],
) -> Result<crate::predictive_proposals::CompetencyCoverageSummaryV1> {
    let eval = evaluate_competency_questions_with_trust(db, questions)?;
    Ok(crate::predictive_proposals::CompetencyCoverageSummaryV1 {
        total: eval.total,
        satisfied: eval.satisfied,
        coverage: eval.coverage,
        cost: eval.cost,
        questions: eval
            .questions
            .into_iter()
            .map(|q| crate::predictive_proposals::CompetencyQuestionResultV1 {
                name: q.name,
                rows: q.rows,
                min_rows: q.min_rows,
                satisfied: q.satisfied,
                weight: q.weight,
                cost: q.cost,
            })
            .collect(),
    })
}

pub fn evaluate_competency_questions_with_trust(
    db: &PathDB,
    questions: &[CompetencyQuestionV1],
) -> Result<CompetencyCoverageWithTrustV1> {
    if questions.is_empty() {
        return Ok(CompetencyCoverageWithTrustV1::default());
    }

    let meta = MetaPlaneIndex::from_db(db).ok();
    let mut results: Vec<CompetencyQuestionEvaluationV1> = Vec::new();
    let mut satisfied = 0usize;
    let mut total_cost = 0.0;

    for q in questions {
        if q.query.trim().is_empty() {
            let eval = unresolved_authored_competency_question_evaluation(q);
            total_cost += eval.cost;
            results.push(eval);
            continue;
        }
        let (query, min_rows, weight) = normalized_competency_query(q)?;
        let query_ir = crate::query_ir::QueryIrV1::from_axql_query(&query);
        let mut prepared = query_ir.prepare_with_meta(db, meta.as_ref())?;
        let prepared_query = Some(prepared.metadata_with_meta(meta.as_ref())?);
        let refinement_candidates = prepared
            .exploration_view(None)
            .refinement_candidates
            .into_iter()
            .filter(|candidate| !candidate.handle.preview_fragment().contains("?_lookup"))
            .map(|candidate| {
                wrap_runtime_refinement_candidate_for_competency_question(&q.name, candidate)
            })
            .collect::<Result<Vec<_>>>()?;
        let trust = crate::trust_contract::query_user_visible_trust_contract_with_meta(
            &query,
            &prepared.certifiability(),
            false,
            None,
            meta.as_ref(),
        );
        let res = prepared.execute(db, meta.as_ref())?;
        let rows = res.rows.len();
        let ok = rows >= min_rows;
        if ok {
            satisfied += 1;
        }
        let cost = if ok { 0.0 } else { weight };
        total_cost += cost;

        results.push(CompetencyQuestionEvaluationV1 {
            name: q.name.clone(),
            rows,
            min_rows,
            satisfied: ok,
            weight,
            cost,
            prepared_query,
            trust: CompetencyQuestionTrustV1 {
                trust_class: trust.trust_class,
                coverage: Some(trust.coverage),
                reasons: trust.reasons,
                notes: trust.notes,
                semantic_coverage: trust.semantic_coverage,
                gaps: trust.gaps,
            },
            refinement_candidates,
        });
    }

    let total = questions.len();
    let coverage = if total == 0 {
        0.0
    } else {
        satisfied as f64 / total as f64
    };

    Ok(CompetencyCoverageWithTrustV1 {
        total,
        satisfied,
        coverage,
        cost: total_cost,
        runtime_theory_check: None,
        questions: results,
    })
}

#[allow(dead_code)]
pub fn evaluate_competency_questions_with_trust_and_theory_graph(
    db: &PathDB,
    questions: &[CompetencyQuestionV1],
    compiled_schema: &CompiledSchemaIr,
    theories: &[TheoryIr],
) -> Result<CompetencyCoverageWithTrustV1> {
    if questions.is_empty() {
        return Ok(CompetencyCoverageWithTrustV1::default());
    }

    let meta = MetaPlaneIndex::from_db(db).ok();
    let mut results: Vec<CompetencyQuestionEvaluationV1> = Vec::new();
    let mut satisfied = 0usize;
    let mut total_cost = 0.0;

    for q in questions {
        if q.query.trim().is_empty() {
            let eval = unresolved_authored_competency_question_evaluation(q);
            total_cost += eval.cost;
            results.push(eval);
            continue;
        }
        let (query, min_rows, weight) = normalized_competency_query(q)?;
        let query_ir = crate::query_ir::QueryIrV1::from_axql_query(&query);
        let mut prepared = query_ir.prepare_with_meta(db, meta.as_ref())?;
        let prepared_query = Some(prepared.metadata_with_meta(meta.as_ref())?);
        let refinement_candidates = prepared
            .exploration_view_with_theory_graph(None, compiled_schema, theories)
            .refinement_candidates
            .into_iter()
            .filter(|candidate| !candidate.handle.preview_fragment().contains("?_lookup"))
            .map(|candidate| {
                wrap_runtime_refinement_candidate_for_competency_question(&q.name, candidate)
            })
            .collect::<Result<Vec<_>>>()?;
        let trust = crate::trust_contract::query_user_visible_trust_contract_with_meta(
            &query,
            &prepared.certifiability(),
            false,
            None,
            meta.as_ref(),
        );
        let res = prepared.execute(db, meta.as_ref())?;
        let rows = res.rows.len();
        let ok = rows >= min_rows;
        if ok {
            satisfied += 1;
        }
        let cost = if ok { 0.0 } else { weight };
        total_cost += cost;

        results.push(CompetencyQuestionEvaluationV1 {
            name: q.name.clone(),
            rows,
            min_rows,
            satisfied: ok,
            weight,
            cost,
            prepared_query,
            trust: CompetencyQuestionTrustV1 {
                trust_class: trust.trust_class,
                coverage: Some(trust.coverage),
                reasons: trust.reasons,
                notes: trust.notes,
                semantic_coverage: trust.semantic_coverage,
                gaps: trust.gaps,
            },
            refinement_candidates,
        });
    }

    let total = questions.len();
    let coverage = if total == 0 {
        0.0
    } else {
        satisfied as f64 / total as f64
    };

    Ok(CompetencyCoverageWithTrustV1 {
        total,
        satisfied,
        coverage,
        cost: total_cost,
        runtime_theory_check: None,
        questions: results,
    })
}

fn prompts_from_json(value: Value) -> Result<Vec<CompetencyQuestionPrompt>> {
    let value = match value {
        Value::Object(mut map) => {
            if let Some(items) = map.remove("competency_questions") {
                items
            } else if let Some(items) = map.remove("questions") {
                items
            } else {
                Value::Object(map)
            }
        }
        other => other,
    };

    let mut out = Vec::new();
    match value {
        Value::Array(items) => {
            for item in items {
                out.push(prompt_from_value(&item)?);
            }
        }
        Value::Object(_) => {
            out.push(prompt_from_value(&value)?);
        }
        other => {
            return Err(anyhow!(
                "unsupported competency question input (expected JSON array or object, got {})",
                other
            ));
        }
    }
    Ok(out)
}

fn prompt_from_value(value: &Value) -> Result<CompetencyQuestionPrompt> {
    match value {
        Value::String(text) => Ok(CompetencyQuestionPrompt {
            name: None,
            question: Some(text.trim().to_string()),
            query: None,
            min_rows: None,
            weight: None,
            contexts: Vec::new(),
        }),
        Value::Object(map) => {
            let name = map
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let question = map
                .get("question")
                .or_else(|| map.get("text"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let query = map
                .get("query")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let min_rows = map
                .get("min_rows")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let weight = map.get("weight").and_then(|v| v.as_f64());
            let contexts = match map.get("contexts") {
                Some(Value::Array(items)) => items
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect(),
                _ => Vec::new(),
            };

            if query.is_none() && question.is_none() {
                return Err(anyhow!(
                    "competency question JSON objects require `question` or `query`"
                ));
            }

            Ok(CompetencyQuestionPrompt {
                name,
                question,
                query,
                min_rows,
                weight,
                contexts,
            })
        }
        other => Err(anyhow!(
            "unsupported competency question item (expected string or object, got {})",
            other
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn evaluate_competency_questions_with_trust_carries_semantic_coverage() -> Result<()> {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
        let db = crate::load_pathdb_for_cli(&repo_root.join("examples/Family.axi"))?;
        let eval = evaluate_competency_questions_with_trust(
            &db,
            &[CompetencyQuestionV1 {
                name: "jamison_parent".to_string(),
                question: Some("Jamison should have Bob as a parent".to_string()),
                authoring: None,
                query:
                    "select ?f where ?f = Fam.Parent(child=Jamison, parent=Bob, ctx=FamilyTree, time=?t) limit 1"
                        .to_string(),
                min_rows: 1,
                weight: 1.0,
                contexts: Vec::new(),
            }],
        )?;

        let trust = &eval.questions[0].trust;
        assert_eq!(trust.trust_class, "certifiable");
        assert_eq!(trust.coverage.as_deref(), Some("full_query"));
        assert!(trust.semantic_coverage.is_some());
        assert!(trust
            .notes
            .iter()
            .any(|note| note.contains("full ontology closure")));
        assert!(eval.questions[0]
            .refinement_candidates
            .iter()
            .all(|candidate| matches!(
                candidate.handle.domain(),
                crate::typed_refinement::RuntimeRefinementDomainV1::CompetencyQuestionRepair
            )));
        Ok(())
    }

    #[test]
    fn competency_questions_surface_runtime_query_repair_handles() -> Result<()> {
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

        let question = CompetencyQuestionV1 {
            name: "flow_dst".to_string(),
            question: Some("Find a downstream supplier".to_string()),
            authoring: None,
            query: r#"select ?dst where ?f = Demo.Flow(from=a, to=?dst) limit 1"#.to_string(),
            min_rows: 1,
            weight: 1.0,
            contexts: Vec::new(),
        };

        let eval = evaluate_competency_questions_with_trust(&db, std::slice::from_ref(&question))?;
        let prepared_query = eval.questions[0]
            .prepared_query
            .as_ref()
            .expect("CQ report should cite prepared-query metadata");
        assert!(prepared_query.query_ir_id.starts_with("query_ir_v1:"));
        assert!(prepared_query
            .prepared_query_id
            .starts_with("prepared_query_v1:"));
        assert_eq!(prepared_query.trust.trust_class, "certifiable");
        assert_eq!(prepared_query.non_claims.completeness_claim, "not_claimed");
        assert!(prepared_query.kernel_refs.is_empty());
        let candidate = eval.questions[0]
            .refinement_candidates
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.handle.domain(),
                    crate::typed_refinement::RuntimeRefinementDomainV1::CompetencyQuestionRepair
                ) && matches!(
                    candidate.kind,
                    crate::typed_refinement::RuntimeRefinementCandidateKindV1::AddTypeGuard
                ) && candidate.target_type.as_deref() == Some("Demo.Supplier")
            })
            .expect("expected CQ repair candidate");
        let meta = axiograph_pathdb::axi_semantics::MetaPlaneIndex::from_db(&db)?;
        let refined = apply_runtime_refinement_handle_to_competency_question(
            &db,
            Some(&meta),
            &question,
            &candidate.handle,
        )?;
        assert_ne!(refined.query, question.query);
        assert!(refined.query.contains("Demo.Supplier"));
        Ok(())
    }

    #[test]
    fn authored_competency_question_without_lowering_reports_residual_gap() -> Result<()> {
        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(
            &mut db,
            r#"
module Demo
schema Demo:
  object Shipment
instance I of Demo:
  Shipment = {Shipment_1}
"#,
        )?;
        db.build_indexes();

        let question = CompetencyQuestionV1 {
            name: "shipment_rule".to_string(),
            question: Some("Which shipment rule applies?".to_string()),
            authoring: Some(crate::predictive_proposals::CompetencyQuestionAuthoringHintsV1 {
                ask: Some("Which shipment rule applies?".to_string()),
                about: vec!["shipment release".to_string()],
                given: vec!["ERP hold is active".to_string()],
                expect: vec!["a typed release obligation exists".to_string()],
                notes: Vec::new(),
            }),
            query: String::new(),
            min_rows: 1,
            weight: 2.0,
            contexts: Vec::new(),
        };

        let eval = evaluate_competency_questions_with_trust(&db, &[question])?;
        assert_eq!(eval.total, 1);
        assert_eq!(eval.satisfied, 0);
        assert_eq!(eval.cost, 2.0);
        assert_eq!(eval.questions[0].trust.trust_class, "unresolved_authoring");
        assert_eq!(
            eval.questions[0].trust.gaps[0].code,
            "cq_missing_executable_lowering"
        );
        assert!(eval.questions[0].prepared_query.is_none());
        Ok(())
    }

    #[test]
    fn competency_questions_can_attach_compiled_theory_handles_to_repairs() -> Result<()> {
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

        let question = CompetencyQuestionV1 {
            name: "parent_child".to_string(),
            question: Some("Find Alice's child".to_string()),
            authoring: None,
            query: r#"select ?c where ?f = Demo.Parent(parent=Alice, child=?c) limit 1"#
                .to_string(),
            min_rows: 1,
            weight: 1.0,
            contexts: Vec::new(),
        };

        let eval = evaluate_competency_questions_with_trust_and_theory_graph(
            &db,
            std::slice::from_ref(&question),
            &compiled_schema,
            &theories,
        )?;
        assert!(eval.questions[0]
            .refinement_candidates
            .iter()
            .any(|candidate| {
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
    fn competency_question_refinement_apply_result_preserves_theory_graph() -> Result<()> {
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

        let question = CompetencyQuestionV1 {
            name: "flow_dst".to_string(),
            question: Some("Find a downstream supplier".to_string()),
            authoring: None,
            query: r#"select ?dst where ?f = Demo.Flow(from=a, to=?dst) limit 1"#.to_string(),
            min_rows: 1,
            weight: 1.0,
            contexts: Vec::new(),
        };

        let eval = evaluate_competency_questions_with_trust_and_theory_graph(
            &db,
            std::slice::from_ref(&question),
            &compiled_schema,
            &theories,
        )?;
        let candidate = eval.questions[0]
            .refinement_candidates
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.kind,
                    crate::typed_refinement::RuntimeRefinementCandidateKindV1::BindFactRelation
                )
            })
            .expect("expected CQ repair candidate");

        let applied =
            apply_runtime_refinement_handle_to_competency_question_result_with_theory_graph(
                &db,
                Some(&meta),
                &question,
                &candidate.handle,
                &compiled_schema,
                &theories,
            )?;
        assert_ne!(applied.refined_question.query, question.query);
        assert!(applied
            .query_apply
            .refined_exploration
            .refinement_candidates
            .iter()
            .any(|candidate| {
                matches!(
                    candidate.theory_obligation_ref.as_ref(),
                    Some(axiograph_pathdb::kernel_ir::TheoryObligationRefIr::Constraint {
                        relation_name,
                        ..
                    }) if relation_name.as_deref() == Some("Flow")
                )
            }));
        Ok(())
    }
}
