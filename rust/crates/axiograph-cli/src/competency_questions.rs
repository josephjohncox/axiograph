//! File and LLM adapters over the shared competency-question service.
use crate::llm::{GeneratedQuery, LlmState};
use anyhow::{anyhow, Result};
use axiograph_pathdb::PathDB;
pub use axiograph_query::competency_questions::*;
use serde_json::Value;
use std::path::Path;

pub fn load_question_prompts(path: &Path) -> Result<Vec<CompetencyQuestionPrompt>> {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    if ext.eq_ignore_ascii_case("json") {
        let text = crate::security::read_utf8_file_bounded(
            path,
            crate::security::MAX_TEXT_INPUT_BYTES,
            "CLI input",
        )?;
        let value: Value = crate::security::parse_json_bounded(
            text.as_bytes(),
            crate::security::MAX_JSON_INPUT_BYTES,
            "competency question input",
        )?;
        return prompts_from_json(value);
    }

    let text = crate::security::read_utf8_file_bounded(
        path,
        crate::security::MAX_TEXT_INPUT_BYTES,
        "CLI input",
    )?;
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
                "unsupported competency question input (expected JSON array or object, got {other})"
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
            "unsupported competency question item (expected string or object, got {other})"
        )),
    }
}
