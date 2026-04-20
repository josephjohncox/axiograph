//! Competency question generation and translation helpers.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::Path;

use axiograph_pathdb::axi_semantics::MetaPlaneIndex;
use axiograph_pathdb::PathDB;

use crate::llm::{GeneratedQuery, LlmState};
use crate::world_model::CompetencyQuestionV1;

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
            GeneratedQuery::Axql(q) => q,
            GeneratedQuery::QueryIrV1(ir) => ir.to_axql_text()?,
        };
        out.push(CompetencyQuestionV1 {
            name,
            question: Some(question),
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
    pub trust: CompetencyQuestionTrustV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompetencyCoverageWithTrustV1 {
    pub total: usize,
    pub satisfied: usize,
    pub coverage: f64,
    pub cost: f64,
    #[serde(default)]
    pub questions: Vec<CompetencyQuestionEvaluationV1>,
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

pub fn evaluate_competency_questions(
    db: &PathDB,
    questions: &[CompetencyQuestionV1],
) -> Result<crate::world_model::CompetencyCoverageSummaryV1> {
    let eval = evaluate_competency_questions_with_trust(db, questions)?;
    Ok(crate::world_model::CompetencyCoverageSummaryV1 {
        total: eval.total,
        satisfied: eval.satisfied,
        coverage: eval.coverage,
        cost: eval.cost,
        questions: eval
            .questions
            .into_iter()
            .map(|q| crate::world_model::CompetencyQuestionResultV1 {
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
        let (query, min_rows, weight) = normalized_competency_query(q)?;
        let mut prepared = crate::axql::prepare_axql_query_with_meta(db, &query, meta.as_ref())?;
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
            trust: CompetencyQuestionTrustV1 {
                trust_class: trust.trust_class,
                coverage: Some(trust.coverage),
                reasons: trust.reasons,
                notes: trust.notes,
                semantic_coverage: trust.semantic_coverage,
                gaps: trust.gaps,
            },
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
        Ok(())
    }
}
