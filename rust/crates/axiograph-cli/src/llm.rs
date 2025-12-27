//! LLM-driven query assistance for the REPL.
//!
//! Goal: make it easy to ask *generic* questions while keeping the core
//! semantics **structured** and auditable.
//!
//! Design constraints:
//! - The REPL must work in restricted environments (no heavy deps, no network).
//! - LLM support should be optional and pluggable.
//! - The LLM is **untrusted**: it produces *candidate* structured queries; the
//!   engine executes those queries over the loaded snapshot.
//!
//! In other words:
//!   "LLM proposes → Axiograph executes (and later: certifies)"

#![allow(unused_mut, dead_code)]

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
#[cfg(feature = "llm-ollama")]
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use roaring::RoaringBitmap;

use axiograph_ingest_docs::{Chunk, ProposalSourceV1, ProposalV1, ProposalsFileV1};
use axiograph_pathdb::axi_semantics::MetaPlaneIndex;
use axiograph_pathdb::PathDB;
use crate::query_ir::QueryIrV1;

pub(crate) const AXIOGRAPH_LLM_TIMEOUT_SECS_ENV: &str = "AXIOGRAPH_LLM_TIMEOUT_SECS";
pub(crate) const AXIOGRAPH_LLM_MAX_STEPS_ENV: &str = "AXIOGRAPH_LLM_MAX_STEPS";
pub(crate) const AXIOGRAPH_LLM_PROMPT_MAX_TRANSCRIPT_ITEMS_ENV: &str =
    "AXIOGRAPH_LLM_PROMPT_MAX_TRANSCRIPT_ITEMS";
pub(crate) const AXIOGRAPH_LLM_PROMPT_MAX_JSON_STRING_CHARS_ENV: &str =
    "AXIOGRAPH_LLM_PROMPT_MAX_JSON_STRING_CHARS";
pub(crate) const AXIOGRAPH_LLM_PROMPT_MAX_JSON_ARRAY_LEN_ENV: &str =
    "AXIOGRAPH_LLM_PROMPT_MAX_JSON_ARRAY_LEN";
pub(crate) const AXIOGRAPH_LLM_PROMPT_MAX_JSON_OBJECT_KEYS_ENV: &str =
    "AXIOGRAPH_LLM_PROMPT_MAX_JSON_OBJECT_KEYS";
pub(crate) const AXIOGRAPH_LLM_PROMPT_MAX_JSON_DEPTH_ENV: &str =
    "AXIOGRAPH_LLM_PROMPT_MAX_JSON_DEPTH";
pub(crate) const AXIOGRAPH_LLM_PREFETCH_DESCRIBE_ENTITIES_ENV: &str =
    "AXIOGRAPH_LLM_PREFETCH_DESCRIBE_ENTITIES";
pub(crate) const AXIOGRAPH_LLM_PREFETCH_DOCCHUNKS_ENV: &str = "AXIOGRAPH_LLM_PREFETCH_DOCCHUNKS";
pub(crate) const AXIOGRAPH_LLM_PREFETCH_LOOKUP_RELATIONS_ENV: &str =
    "AXIOGRAPH_LLM_PREFETCH_LOOKUP_RELATIONS";
pub(crate) const AXIOGRAPH_LLM_PREFETCH_LOOKUP_TYPES_ENV: &str = "AXIOGRAPH_LLM_PREFETCH_LOOKUP_TYPES";

// Default chosen to keep REPL and discovery workflows responsive while allowing
// local models to take a bit of time.
const DEFAULT_LLM_TIMEOUT_SECS: u64 = 120;
const DEFAULT_LLM_MAX_STEPS: usize = 6;
const DEFAULT_LLM_PROMPT_MAX_TRANSCRIPT_ITEMS: usize = 12;
const DEFAULT_LLM_PROMPT_MAX_JSON_STRING_CHARS: usize = 520;
const DEFAULT_LLM_PROMPT_MAX_JSON_ARRAY_LEN: usize = 64;
const DEFAULT_LLM_PROMPT_MAX_JSON_OBJECT_KEYS: usize = 64;
const DEFAULT_LLM_PROMPT_MAX_JSON_DEPTH: usize = 6;
const DEFAULT_LLM_PREFETCH_DESCRIBE_ENTITIES: usize = 2;
const DEFAULT_LLM_PREFETCH_DOCCHUNKS: usize = 1;
const DEFAULT_LLM_PREFETCH_LOOKUP_RELATIONS: usize = 1;
const DEFAULT_LLM_PREFETCH_LOOKUP_TYPES: usize = 1;

/// Resolve the default maximum number of LLM tool-loop steps.
///
/// Precedence:
/// 1) env var `AXIOGRAPH_LLM_MAX_STEPS`
/// 2) default (`DEFAULT_LLM_MAX_STEPS`)
pub(crate) fn llm_default_max_steps() -> Result<usize> {
    match std::env::var(AXIOGRAPH_LLM_MAX_STEPS_ENV) {
        Ok(v) => {
            let v = v.trim();
            if v.is_empty() {
                return Ok(DEFAULT_LLM_MAX_STEPS);
            }
            let n = v.parse::<usize>().map_err(|_| {
                anyhow!(
                    "invalid {AXIOGRAPH_LLM_MAX_STEPS_ENV}={v:?} (expected integer tool-loop step bound)"
                )
            })?;
            Ok(n.max(1))
        }
        Err(std::env::VarError::NotPresent) => Ok(DEFAULT_LLM_MAX_STEPS),
        Err(e) => Err(anyhow!(
            "failed to read {AXIOGRAPH_LLM_MAX_STEPS_ENV}: {e}"
        )),
    }
}

fn llm_env_usize(name: &str, default: usize, min: usize, max: usize) -> Result<usize> {
    match std::env::var(name) {
        Ok(v) => {
            let v = v.trim();
            if v.is_empty() {
                return Ok(default);
            }
            let parsed = v
                .parse::<usize>()
                .map_err(|_| anyhow!("invalid {name}={v:?} (expected integer)"))?;
            Ok(parsed.clamp(min, max))
        }
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(e) => Err(anyhow!("failed to read {name}: {e}")),
    }
}

#[derive(Debug, Clone, Copy)]
struct PromptJsonLimits {
    max_transcript_items: usize,
    max_depth: usize,
    max_string_chars: usize,
    max_array_len: usize,
    max_object_keys: usize,
}

fn llm_prompt_json_limits() -> Result<PromptJsonLimits> {
    Ok(PromptJsonLimits {
        max_transcript_items: llm_env_usize(
            AXIOGRAPH_LLM_PROMPT_MAX_TRANSCRIPT_ITEMS_ENV,
            DEFAULT_LLM_PROMPT_MAX_TRANSCRIPT_ITEMS,
            1,
            256,
        )?,
        max_depth: llm_env_usize(
            AXIOGRAPH_LLM_PROMPT_MAX_JSON_DEPTH_ENV,
            DEFAULT_LLM_PROMPT_MAX_JSON_DEPTH,
            2,
            32,
        )?,
        max_string_chars: llm_env_usize(
            AXIOGRAPH_LLM_PROMPT_MAX_JSON_STRING_CHARS_ENV,
            DEFAULT_LLM_PROMPT_MAX_JSON_STRING_CHARS,
            32,
            16_384,
        )?,
        max_array_len: llm_env_usize(
            AXIOGRAPH_LLM_PROMPT_MAX_JSON_ARRAY_LEN_ENV,
            DEFAULT_LLM_PROMPT_MAX_JSON_ARRAY_LEN,
            4,
            10_000,
        )?,
        max_object_keys: llm_env_usize(
            AXIOGRAPH_LLM_PROMPT_MAX_JSON_OBJECT_KEYS_ENV,
            DEFAULT_LLM_PROMPT_MAX_JSON_OBJECT_KEYS,
            4,
            10_000,
        )?,
    })
}

fn llm_prefetch_describe_entities() -> Result<usize> {
    llm_env_usize(
        AXIOGRAPH_LLM_PREFETCH_DESCRIBE_ENTITIES_ENV,
        DEFAULT_LLM_PREFETCH_DESCRIBE_ENTITIES,
        0,
        12,
    )
}

fn llm_prefetch_docchunks() -> Result<usize> {
    llm_env_usize(
        AXIOGRAPH_LLM_PREFETCH_DOCCHUNKS_ENV,
        DEFAULT_LLM_PREFETCH_DOCCHUNKS,
        0,
        12,
    )
}

fn llm_prefetch_lookup_relations() -> Result<usize> {
    llm_env_usize(
        AXIOGRAPH_LLM_PREFETCH_LOOKUP_RELATIONS_ENV,
        DEFAULT_LLM_PREFETCH_LOOKUP_RELATIONS,
        0,
        20,
    )
}

fn llm_prefetch_lookup_types() -> Result<usize> {
    llm_env_usize(
        AXIOGRAPH_LLM_PREFETCH_LOOKUP_TYPES_ENV,
        DEFAULT_LLM_PREFETCH_LOOKUP_TYPES,
        0,
        20,
    )
}

/// Resolve the effective LLM timeout.
///
/// Precedence:
/// 1) explicit override (`timeout_secs_override`)
/// 2) env var `AXIOGRAPH_LLM_TIMEOUT_SECS`
/// 3) default (`DEFAULT_LLM_TIMEOUT_SECS`)
///
/// Semantics:
/// - `0` disables the timeout (wait forever)
pub(crate) fn llm_timeout(timeout_secs_override: Option<u64>) -> Result<Option<Duration>> {
    let secs = match timeout_secs_override {
        Some(v) => v,
        None => match std::env::var(AXIOGRAPH_LLM_TIMEOUT_SECS_ENV) {
            Ok(v) => {
                let v = v.trim();
                if v.is_empty() {
                    DEFAULT_LLM_TIMEOUT_SECS
                } else {
                    v.parse::<u64>().map_err(|_| {
                        anyhow!(
                            "invalid {AXIOGRAPH_LLM_TIMEOUT_SECS_ENV}={v:?} (expected integer seconds; 0 disables)"
                        )
                    })?
                }
            }
            Err(std::env::VarError::NotPresent) => DEFAULT_LLM_TIMEOUT_SECS,
            Err(e) => {
                return Err(anyhow!(
                    "failed to read {AXIOGRAPH_LLM_TIMEOUT_SECS_ENV}: {e}"
                ))
            }
        },
    };

    Ok(if secs == 0 {
        None
    } else {
        Some(Duration::from_secs(secs))
    })
}

pub(crate) fn wait_with_output_timeout(
    mut child: std::process::Child,
    timeout: Option<Duration>,
    context: &str,
) -> Result<Output> {
    let Some(timeout) = timeout else {
        return child
            .wait_with_output()
            .map_err(|e| anyhow!("{context}: {e}"));
    };

    let start = Instant::now();
    loop {
        if child
            .try_wait()
            .map_err(|e| anyhow!("{context}: failed to poll child status: {e}"))?
            .is_some()
        {
            break;
        }

        if start.elapsed() > timeout {
            let _ = child.kill();
            let output = child
                .wait_with_output()
                .map_err(|e| anyhow!("{context}: failed to collect output after kill: {e}"))?;
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!(
                "{context}: timed out after {}s (set {AXIOGRAPH_LLM_TIMEOUT_SECS_ENV}=0 to disable). stderr: {}",
                timeout.as_secs(),
                stderr.trim()
            ));
        }

        thread::sleep(Duration::from_millis(50));
    }

    child
        .wait_with_output()
        .map_err(|e| anyhow!("{context}: {e}"))
}

#[derive(Debug, Clone)]
pub enum LlmBackend {
    Disabled,
    /// A deterministic “mock LLM” for local demos/tests: it runs the same
    /// template parser as the `ask` command and returns the compiled AxQL.
    Mock,
    /// Local Ollama server (default `http://127.0.0.1:11434`).
    ///
    /// Note: we prefer IPv4 loopback by default to avoid `localhost` resolving
    /// to ::1 (IPv6) on some platforms when Ollama is only listening on IPv4.
    /// Override via `OLLAMA_HOST`.
    ///
    /// This uses Ollama's native `/api/chat` endpoint so you can run local
    /// models without a separate plugin process.
    #[cfg(feature = "llm-ollama")]
    Ollama {
        host: String,
    },
    /// External command plugin that speaks `axiograph_llm_plugin_v1` over
    /// stdin/stdout JSON.
    Command {
        program: PathBuf,
        args: Vec<String>,
    },
}

impl Default for LlmBackend {
    fn default() -> Self {
        Self::Disabled
    }
}

#[derive(Debug, Clone, Default)]
pub struct LlmState {
    pub backend: LlmBackend,
    pub model: Option<String>,
}

#[derive(Debug, Clone)]
pub enum GeneratedQuery {
    Axql(String),
    QueryIrV1(QueryIrV1),
}

impl LlmState {
    pub fn status_line(&self) -> String {
        let backend = match &self.backend {
            LlmBackend::Disabled => "disabled".to_string(),
            LlmBackend::Mock => "mock".to_string(),
            #[cfg(feature = "llm-ollama")]
            LlmBackend::Ollama { host } => format!("ollama({host})"),
            LlmBackend::Command { program, .. } => format!("command({})", program.display()),
        };
        let model = self.model.as_deref().unwrap_or("(none)");
        format!("llm: backend={backend} model={model}")
    }

    pub fn generate_query(&self, db: &PathDB, question: &str) -> Result<GeneratedQuery> {
        let out = match &self.backend {
            LlmBackend::Disabled => Err(anyhow!("LLM backend is disabled (use `llm use ...`)")),
            LlmBackend::Mock => {
                let tokens: Vec<String> =
                    question.split_whitespace().map(|s| s.to_string()).collect();
                let q = crate::nlq::parse_ask_query(&tokens)?;
                Ok(GeneratedQuery::Axql(crate::nlq::render_axql_query(&q)))
            }
            #[cfg(feature = "llm-ollama")]
            LlmBackend::Ollama { host } => {
                let Some(model) = self.model.as_deref() else {
                    return Err(anyhow!(
                        "no model selected (use `llm model <ollama_model>`; e.g. `llm model llama3.2`)"
                    ));
                };
                ollama_generate_query(host, model, db, question)
            }
            LlmBackend::Command { program, args } => {
                let meta = MetaPlaneIndex::from_db(db).unwrap_or_default();
                let request = PluginRequestV1 {
                    protocol: PLUGIN_PROTOCOL_V2.to_string(),
                    model: self.model.clone(),
                    task: PluginTaskV1::ToQuery {
                        question: question.to_string(),
                        schema: SchemaContextV1::from_db_with_meta(db, &meta),
                    },
                };
                let response = run_plugin(program, args, &request)?;

                if let Some(err) = response.error {
                    return Err(anyhow!("llm plugin error: {err}"));
                }
                if let Some(ir) = response.query_ir_v1 {
                    return Ok(GeneratedQuery::QueryIrV1(ir));
                }
                if let Some(axql) = response.axql {
                    return Ok(GeneratedQuery::Axql(axql));
                }
                Err(anyhow!("llm plugin returned no `query_ir_v1` or `axql`"))
            }
        }?;

        Ok(normalize_generated_query(out))
    }

    pub fn summarize_answer(
        &self,
        db: &PathDB,
        question: &str,
        query: &GeneratedQuery,
        result: &ExecutionResult,
    ) -> Result<Option<String>> {
        match &self.backend {
            LlmBackend::Command { program, args } => {
                let request = PluginRequestV1 {
                    protocol: PLUGIN_PROTOCOL_V2.to_string(),
                    model: self.model.clone(),
                    task: PluginTaskV1::Answer {
                        question: question.to_string(),
                        query: QueryPayloadV1::Axql {
                            axql: match query {
                                GeneratedQuery::Axql(q) => q.clone(),
                                GeneratedQuery::QueryIrV1(ir) => ir.to_axql_text()?,
                            },
                        },
                        results: result.to_plugin_results(db),
                    },
                };
                let response = run_plugin(program, args, &request)?;
                if let Some(err) = response.error {
                    return Err(anyhow!("llm plugin error: {err}"));
                }
                Ok(response.answer)
            }
            #[cfg(feature = "llm-ollama")]
            LlmBackend::Ollama { host } => {
                let Some(model) = self.model.as_deref() else {
                    return Ok(Some(
                        "no model selected (use `llm model <ollama_model>`; e.g. `llm model llama3.2`)"
                            .to_string(),
                    ));
                };
                Ok(Some(ollama_summarize_answer(
                    host, model, db, question, query, result,
                )?))
            }
            _ => Ok(None),
        }
    }
}

fn normalize_generated_query(q: GeneratedQuery) -> GeneratedQuery {
    match q {
        GeneratedQuery::Axql(text) => GeneratedQuery::Axql(normalize_axql_candidate(&text)),
        GeneratedQuery::QueryIrV1(ir) => GeneratedQuery::QueryIrV1(ir),
    }
}

fn normalize_axql_candidate(text: &str) -> String {
    let mut s = text.trim().to_string();
    if s.is_empty() {
        return s;
    }

    // Trim common wrappers produced by models (even when we ask them not to).
    if let Some(rest) = s.strip_prefix("axql:") {
        s = rest.trim().to_string();
    }
    if let Some(rest) = s.strip_prefix("AxQL:") {
        s = rest.trim().to_string();
    }

    // Strip a single surrounding markdown fence.
    if s.starts_with("```") {
        if let Some(end) = s.rfind("```") {
            if end > 0 {
                let inner = &s[3..end];
                s = inner.trim().to_string();
            }
        }
        if let Some(rest) = s.strip_prefix("text") {
            // ```text
            s = rest.trim().to_string();
        }
    }

    // Many LLMs add a trailing ';' out of SQL habit.
    while s.ends_with(';') {
        s.pop();
        s = s.trim_end().to_string();
    }

    // AxQL queries must start with either `where ...` (implicit select) or
    // `select ... where ...`. If the model returns just an atom/conjunction,
    // treat it as a `where` clause.
    let lower = s.to_ascii_lowercase();
    if !(lower.starts_with("where") || lower.starts_with("select")) {
        s = format!("where {s}");
    }

    s = rewrite_common_llm_axql_mistakes(&s);
    s
}

fn rewrite_common_llm_axql_mistakes(text: &str) -> String {
    let mut s = text.to_string();
    s = rewrite_bracketed_limit_syntax(&s);
    s = rewrite_colon_attr_equality(&s);
    s = rewrite_var_is_quoted_string_as_name_attr(&s);
    s
}

fn rewrite_bracketed_limit_syntax(text: &str) -> String {
    // Common LLM mistake: `[...]` around the query limit, like `[limit 10]`.
    //
    // Note: AxQL uses brackets for path expressions (RPQs), so we only rewrite
    // bracket groups that *start* with `limit`.
    let mut out = String::new();
    let mut i = 0usize;
    while let Some(open_rel) = text[i..].find('[') {
        let open = i + open_rel;
        out.push_str(&text[i..open]);

        let Some(close_rel) = text[open + 1..].find(']') else {
            // No closing bracket; emit the rest unchanged.
            out.push_str(&text[open..]);
            return out;
        };
        let close = open + 1 + close_rel;

        let inner = text[open + 1..close].trim();
        if let Some(limit) = parse_bracketed_limit(inner) {
            if !out.is_empty() && !out.ends_with(char::is_whitespace) {
                out.push(' ');
            }
            out.push_str(&limit);
        } else {
            // Keep untouched: this could be a bracketed RPQ like `-[a/b]->`.
            out.push_str(&text[open..=close]);
        }

        i = close + 1;
    }
    out.push_str(&text[i..]);
    out
}

fn parse_bracketed_limit(inner: &str) -> Option<String> {
    // Supports:
    // - limit 10
    // - LIMIT 10
    // - limit=10
    // - limit: 10
    let mut s = inner.trim().to_string();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    if !lower.starts_with("limit") {
        return None;
    }

    // Keep slicing by byte offset: ASCII only ("limit").
    let rest = s[5..].trim_start();
    let rest = rest
        .strip_prefix('=')
        .or_else(|| rest.strip_prefix(':'))
        .unwrap_or(rest);
    let rest = rest.trim_start();

    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    Some(format!("limit {digits}"))
}

fn rewrite_var_is_quoted_string_as_name_attr(text: &str) -> String {
    // Common LLM mistake: `?x is "SomeEntityName"` when it means attribute
    // equality on `name`.
    //
    // We rewrite only when the RHS is a *quoted string* so we don't conflict
    // with the valid type atom form: `?x is TypeName`.
    let mut out = String::new();
    let mut i = 0usize;

    while let Some(q_rel) = text[i..].find('?') {
        let q = i + q_rel;
        out.push_str(&text[i..q]);

        let Some((var_end, var_name)) = parse_var_token(text, q) else {
            // Not a valid variable; emit '?' and continue.
            out.push('?');
            i = q + 1;
            continue;
        };

        // Lookahead: `?var <ws> is <ws> "<string>"`
        let mut j = var_end;
        while let Some(c) = text[j..].chars().next() {
            if c.is_whitespace() {
                j += c.len_utf8();
            } else {
                break;
            }
        }
        if !starts_with_kw_case_insensitive(text, j, "is") {
            out.push_str(&text[q..var_end]);
            i = var_end;
            continue;
        }
        j += 2;
        if let Some(c) = text[j..].chars().next() {
            if c.is_whitespace() {
                // ok
            } else {
                out.push_str(&text[q..var_end]);
                i = var_end;
                continue;
            }
        } else {
            out.push_str(&text[q..var_end]);
            i = var_end;
            continue;
        }
        while let Some(c) = text[j..].chars().next() {
            if c.is_whitespace() {
                j += c.len_utf8();
            } else {
                break;
            }
        }
        let Some((lit_end, lit)) = parse_string_literal(text, j) else {
            out.push_str(&text[q..var_end]);
            i = var_end;
            continue;
        };

        // Rewrite: `?var is "X"` → `?var.name = "X"`
        out.push_str(&var_name);
        out.push_str(".name = ");
        out.push_str(lit);

        i = lit_end;
    }

    out.push_str(&text[i..]);
    out
}

fn rewrite_colon_attr_equality(text: &str) -> String {
    // Common LLM mistake: `?x :name = "Alice"` or `?x :full_name = "..."`.
    //
    // AxQL uses `:` for type constraints only; attribute equality is
    // `?x.name = "Alice"` or `attr(?x, "name", "Alice")`.
    //
    // We rewrite only when the `:<ident>` is followed by `=`.
    let mut out = String::new();
    let mut i = 0usize;

    while let Some(q_rel) = text[i..].find('?') {
        let q = i + q_rel;
        out.push_str(&text[i..q]);

        let Some((var_end, var_name)) = parse_var_token(text, q) else {
            out.push('?');
            i = q + 1;
            continue;
        };

        let mut j = var_end;
        while let Some(c) = text[j..].chars().next() {
            if c.is_whitespace() {
                j += c.len_utf8();
            } else {
                break;
            }
        }
        if text.as_bytes().get(j) != Some(&b':') {
            out.push_str(&text[q..var_end]);
            i = var_end;
            continue;
        }
        j += 1;
        while let Some(c) = text[j..].chars().next() {
            if c.is_whitespace() {
                j += c.len_utf8();
            } else {
                break;
            }
        }

        let Some((key_end, key)) = parse_ident_token(text, j) else {
            out.push_str(&text[q..var_end]);
            i = var_end;
            continue;
        };

        let mut k = key_end;
        while let Some(c) = text[k..].chars().next() {
            if c.is_whitespace() {
                k += c.len_utf8();
            } else {
                break;
            }
        }
        if text.as_bytes().get(k) != Some(&b'=') {
            // This is probably a valid type atom: `?x : TypeName`.
            out.push_str(&text[q..var_end]);
            i = var_end;
            continue;
        }

        // Rewrite `?x :key =` → `?x.key =`
        out.push_str(&var_name);
        out.push('.');
        out.push_str(&key);
        out.push_str(&text[key_end..=k]); // include any spaces before '=' plus '=' itself

        i = k + 1;
    }

    out.push_str(&text[i..]);
    out
}

fn parse_var_token(text: &str, start: usize) -> Option<(usize, String)> {
    if !text.as_bytes().get(start).is_some_and(|b| *b == b'?') {
        return None;
    }
    let mut i = start + 1;
    let mut chars = text[i..].chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    i += first.len_utf8();
    while let Some(c) = text[i..].chars().next() {
        if c.is_ascii_alphanumeric() || c == '_' {
            i += c.len_utf8();
        } else {
            break;
        }
    }
    Some((i, text[start..i].to_string()))
}

fn parse_ident_token(text: &str, start: usize) -> Option<(usize, String)> {
    let mut i = start;
    let first = text[i..].chars().next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    i += first.len_utf8();
    while let Some(c) = text[i..].chars().next() {
        if c.is_ascii_alphanumeric() || c == '_' {
            i += c.len_utf8();
        } else {
            break;
        }
    }
    Some((i, text[start..i].to_string()))
}

fn starts_with_kw_case_insensitive(text: &str, start: usize, kw: &str) -> bool {
    let Some(slice) = text.get(start..) else {
        return false;
    };
    slice
        .as_bytes()
        .get(..kw.len())
        .is_some_and(|b| b.eq_ignore_ascii_case(kw.as_bytes()))
}

fn parse_string_literal<'a>(text: &'a str, start: usize) -> Option<(usize, &'a str)> {
    let quote = *text.as_bytes().get(start)?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let mut i = start + 1;
    while i < text.len() {
        let b = text.as_bytes()[i];
        if b == b'\\' {
            // Skip escaped char.
            i += 1;
            if i < text.len() {
                i += 1;
            }
            continue;
        }
        if b == quote {
            let end = i + 1;
            return Some((end, &text[start..end]));
        }
        i += 1;
    }
    None
}

#[cfg(feature = "llm-ollama")]
fn normalize_ollama_host(host: &str) -> String {
    let mut host = host.trim().to_string();
    if host.is_empty() {
        // Prefer IPv4 loopback by default. Some local Ollama installs bind on
        // 127.0.0.1 but not ::1, and `localhost` may resolve to ::1 first.
        host = "http://127.0.0.1:11434".to_string();
    }
    if !host.starts_with("http://") && !host.starts_with("https://") {
        host = format!("http://{host}");
    }
    host.trim_end_matches('/').to_string()
}

#[cfg(feature = "llm-ollama")]
pub fn default_ollama_host() -> String {
    normalize_ollama_host(
        &std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://127.0.0.1:11434".to_string()),
    )
}

// =============================================================================
// Ollama backend
// =============================================================================

#[cfg(feature = "llm-ollama")]
fn ollama_generate_query(
    host: &str,
    model: &str,
    db: &PathDB,
    question: &str,
) -> Result<GeneratedQuery> {
    let meta = MetaPlaneIndex::from_db(db).unwrap_or_default();
    let schema = SchemaContextV1::from_db_with_meta(db, &meta);
    let grounding = render_doc_grounding(db, question, 6, 420);
    let name_samples = render_entity_name_samples(db, &schema);

    let system = r#"You translate user questions into structured Axiograph queries.

You MUST return a single JSON object with one of these shapes:
- { "query_ir_v1": { ... } }
- { "axql": "<AxQL query>" }   (fallback; only if you cannot produce query_ir_v1)
- { "error": "<error message>" }

Do not wrap in markdown or code fences."#;

    let schemas_text = if schema.schemas.is_empty() {
        "(none)".to_string()
    } else {
        schema.schemas.join(", ")
    };
    let relation_sigs_text = if schema.relation_signatures.is_empty() {
        "(none)".to_string()
    } else {
        schema
            .relation_signatures
            .iter()
            .take(40)
            .map(|s| format!("- {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let relation_constraints_text = if schema.relation_constraints.is_empty() {
        "(none)".to_string()
    } else {
        schema
            .relation_constraints
            .iter()
            .take(40)
            .map(|s| format!("- {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let contexts_text = if schema.contexts.is_empty() {
        "(none)".to_string()
    } else {
        schema.contexts.join(", ")
    };
    let times_text = if schema.times.is_empty() {
        "(none)".to_string()
    } else {
        schema.times.join(", ")
    };

    let user = format!(
        r#"Question:
{question}

{grounding}

{name_samples}

Schema context:
- Schemas: {schemas}
- Types: {types}
- Relations: {relations}
- Relation signatures (meta-plane):
{relation_signatures}
- Relation constraints (meta-plane):
{relation_constraints}
- Contexts present (data plane): {contexts}
- Times present (data plane): {times}

Query IR (preferred):
- Use `query_ir_v1` with fields:
  - version: 1
  - select: ["?x", ...]   (or omit for implicit select)
  - where: [ atoms... ]   (single conjunction), OR disjuncts: [ [atoms...], [atoms...] ] for OR
  - limit: N (optional)
  - max_hops: N (optional)
  - min_confidence: 0..1 (optional)

Atoms (examples):
- type:      {{"kind":"type","term":"?x","type":"ProtoService"}}
- edge:      {{"kind":"edge","left":"?svc","path":"proto_service_has_rpc","right":"?rpc"}}
- attr_eq:   {{"kind":"attr_eq","term":"?x","key":"name","value":"Alice"}}

Terms:
- variable:  "?x"
- name ref:  "acme.svc0.v1.Service0"   (means name("acme.svc0.v1.Service0"))
- wildcard:  "_"

AxQL is accepted as a fallback (same semantics), but prefer `query_ir_v1`.

Return ONLY the JSON object."#,
        schemas = schemas_text,
        types = compact_join_list(&schema.types, 60, 1800),
        relations = compact_join_list(&schema.relations, 80, 2400),
        relation_signatures = relation_sigs_text,
        relation_constraints = relation_constraints_text,
        contexts = truncate_preview(&contexts_text, 800),
        times = truncate_preview(&times_text, 800),
    );

    // NOTE: While Ollama supports JSON Schema in `format`, some models (and/or
    // Ollama versions) reject more complex schemas. We only need "return a JSON
    // object", so we use the most compatible option: `format: "json"`.
    let content = ollama_chat(host, model, &user, Some(system), Some(json!("json")))?;
    let parsed: PluginResponseV1 = parse_llm_json_object(&content)?;

    if let Some(err) = parsed.error {
        return Err(anyhow!("ollama: {err}"));
    }
    if let Some(ir) = parsed.query_ir_v1 {
        return Ok(GeneratedQuery::QueryIrV1(ir));
    }
    if let Some(axql) = parsed.axql {
        return Ok(GeneratedQuery::Axql(axql));
    }

    Err(anyhow!("ollama returned no `query_ir_v1` or `axql`"))
}

#[cfg(feature = "llm-ollama")]
fn render_doc_grounding(
    db: &PathDB,
    question: &str,
    max_chunks: usize,
    max_chars: usize,
) -> String {
    let snippets = retrieve_doc_grounding_snippets(db, question, max_chunks, max_chars);
    if snippets.is_empty() {
        return "Doc grounding: (no DocChunk loaded; answer from the graph via tools like `describe_entity` / `axql_run`, and note that you have no external doc evidence to cite)".to_string();
    }

    let mut out = String::new();
    out.push_str("Doc grounding (untrusted snippets, for query translation only):\n");
    for s in snippets {
        out.push_str("- ");
        out.push_str(&s);
        out.push('\n');
    }
    out.trim_end().to_string()
}

#[cfg(feature = "llm-ollama")]
fn render_entity_name_samples(db: &PathDB, schema: &SchemaContextV1) -> String {
    // Keep the list short: it exists primarily to help the model choose
    // identifiers that exist in the current snapshot.
    //
    // Note: `lookup_entity` is case-robust, but `name("...")` in AxQL is exact.
    let mut chosen: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    fn has_type(schema: &SchemaContextV1, ty: &str) -> bool {
        schema.types.iter().any(|t| t == ty)
    }

    for ty in [
        "Person",
        "Context",
        "World",
        "Time",
        "Document",
        "DocChunk",
        "ProtoService",
        "ProtoRpc",
        "ProtoMessage",
        "ProtoField",
    ] {
        if has_type(schema, ty) && seen.insert(ty.to_string()) {
            chosen.push(ty.to_string());
        }
    }

    // Add a few "largest" remaining types (deterministic tie-break: type name).
    let mut by_count: Vec<(usize, String)> = Vec::new();
    for ty in &schema.types {
        if seen.contains(ty) {
            continue;
        }
        if ty.starts_with("AxiMeta") {
            continue;
        }
        let count = db.find_by_type(ty).map(|bm| bm.len()).unwrap_or(0) as usize;
        if count == 0 {
            continue;
        }
        by_count.push((count, ty.clone()));
    }
    by_count.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    for (_count, ty) in by_count.into_iter().take(8) {
        if seen.insert(ty.clone()) {
            chosen.push(ty);
        }
    }

    let mut lines: Vec<String> = Vec::new();
    for ty in chosen.iter().take(12) {
        let Some(names) = sample_entity_names(db, ty, 6) else {
            continue;
        };
        if names.is_empty() {
            continue;
        }
        lines.push(format!("- {ty}: {}", names.join(", ")));
    }

    if lines.is_empty() {
        return "Entity examples: (none)".to_string();
    }

    let mut out = String::new();
    out.push_str("Entity examples (use with `lookup_entity`, `describe_entity`, or `name(\"...\")`):\n");
    for l in lines {
        out.push_str(&l);
        out.push('\n');
    }
    out.trim_end().to_string()
}

#[cfg(feature = "llm-ollama")]
fn render_quasi_rag_preview(
    db: &PathDB,
    question: &str,
    snapshot_key: &str,
    embeddings: Option<&crate::embeddings::ResolvedEmbeddingsIndexV1>,
    ollama_embed_host: Option<&str>,
    options: ToolLoopOptions,
) -> String {
    // Treat the tool loop as a quasi-RAG system:
    // - do deterministic retrieval first (token-hash ANN; optionally embeddings),
    // - keep the prompt compact,
    // - and let the model call tools for deeper inspection.
    let query = truncate_preview(question, 420);

    let args = serde_json::json!({
        "query": query,
        "entity_limit": 8,
        "chunk_limit": options.max_doc_chunks.min(8).max(1),
    });

    let Ok(v) = tool_semantic_search(db, &args, snapshot_key, options, embeddings, ollama_embed_host) else {
        return String::new();
    };

    let entities = v
        .get("entity_hits")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    let chunks = v
        .get("chunk_hits")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();

    if entities.is_empty() && chunks.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push_str("Semantic grounding (deterministic pre-retrieval; untrusted):\n");

    if !entities.is_empty() {
        out.push_str("Entities:\n");
        for hit in entities.iter().take(8) {
            let ent = hit.get("entity").cloned().unwrap_or_else(|| serde_json::json!({}));
            let id = ent.get("id").and_then(|x| x.as_u64()).unwrap_or(0);
            let ety = ent.get("entity_type").and_then(|x| x.as_str()).unwrap_or("?");
            let name = ent.get("name").and_then(|x| x.as_str()).unwrap_or("");
            if name.is_empty() {
                out.push_str(&format!("- {ety}#{id}\n"));
            } else {
                out.push_str(&format!("- {ety} \"{name}\" (id={id})\n"));
            }
        }
    }

    if !chunks.is_empty() {
        out.push_str("DocChunks:\n");
        for hit in chunks.iter().take(6) {
            let chunk_id = hit.get("chunk_id").and_then(|x| x.as_str()).unwrap_or("?");
            let doc = hit.get("document_id").and_then(|x| x.as_str()).unwrap_or("");
            let span = hit.get("span_id").and_then(|x| x.as_str()).unwrap_or("");
            let snippet = hit.get("snippet").and_then(|x| x.as_str()).unwrap_or("").trim();
            let mut label = chunk_id.to_string();
            if !doc.is_empty() {
                label.push_str(&format!(" (doc={doc}"));
                if !span.is_empty() {
                    label.push_str(&format!(" span={span}"));
                }
                label.push(')');
            }
            if snippet.is_empty() {
                out.push_str(&format!("- {label}\n"));
            } else {
                out.push_str(&format!("- {label}: {snippet}\n"));
            }
        }
    }

    out.trim_end().to_string()
}

#[cfg(feature = "llm-ollama")]
fn sample_entity_names(db: &PathDB, type_name: &str, max: usize) -> Option<Vec<String>> {
    let bm = db.find_by_type(type_name)?;
    let mut out: Vec<String> = Vec::new();
    for id in bm.iter().take(max) {
        if let Some(name) = entity_attr_string(db, id, "name") {
            out.push(name);
        }
    }
    Some(out)
}

#[cfg(feature = "llm-ollama")]
fn retrieve_doc_grounding_snippets(
    db: &PathDB,
    question: &str,
    max_chunks: usize,
    max_chars: usize,
) -> Vec<String> {
    let Some(doc_chunks) = db.find_by_type("DocChunk") else {
        return Vec::new();
    };

    let tokens = tokenize_grounding_query(question);
    if tokens.is_empty() {
        return Vec::new();
    }

    // OR-style retrieval: any token match.
    //
    // We search both:
    // - `text`        (chunk bodies / doc comments)
    // - `search_text` (semantic metadata + identifiers, e.g. proto FQNs)
    let mut candidates = db.entities_with_attr_fts_any("text", question);
    candidates |= db.entities_with_attr_fts_any("search_text", question);
    candidates &= doc_chunks.clone();

    if candidates.is_empty() {
        return Vec::new();
    }

    // Score by how many query tokens appear in (text OR metadata).
    let mut scored: Vec<(usize, u32)> = Vec::new();
    for id in candidates.iter() {
        let text_lower = entity_attr_string(db, id, "text")
            .unwrap_or_default()
            .to_ascii_lowercase();
        let search_text_lower = entity_attr_string(db, id, "search_text")
            .unwrap_or_default()
            .to_ascii_lowercase();
        let mut score = 0usize;
        for t in &tokens {
            if text_lower.contains(t) || search_text_lower.contains(t) {
                score += 1;
            }
        }
        if score > 0 {
            scored.push((score, id));
        }
    }

    scored.sort_by(|(sa, ia), (sb, ib)| sb.cmp(sa).then_with(|| ia.cmp(ib)));

    let mut out: Vec<String> = Vec::new();
    for (_, id) in scored.into_iter().take(max_chunks) {
        let chunk_id = entity_attr_string(db, id, "chunk_id").unwrap_or_else(|| format!("{id}"));
        let doc = entity_attr_string(db, id, "document_id").unwrap_or_default();
        let span = entity_attr_string(db, id, "span_id").unwrap_or_default();
        let kind = entity_attr_string(db, id, "meta_kind").unwrap_or_default();
        let fqn = entity_attr_string(db, id, "meta_fqn").unwrap_or_default();
        let text = entity_attr_string(db, id, "text").unwrap_or_default();
        let text = truncate_for_prompt(&text, max_chars);

        let mut line = String::new();
        line.push_str(&chunk_id);
        if !doc.is_empty() || !span.is_empty() {
            line.push_str(" (");
            if !doc.is_empty() {
                line.push_str(&doc);
            }
            if !span.is_empty() {
                if !doc.is_empty() {
                    line.push_str(" ");
                }
                line.push_str(&span);
            }
            line.push(')');
        }
        if !kind.is_empty() || !fqn.is_empty() {
            line.push_str(" [");
            if !kind.is_empty() {
                line.push_str(&kind);
            }
            if !fqn.is_empty() {
                if !kind.is_empty() {
                    line.push(' ');
                }
                line.push_str(&fqn);
            }
            line.push(']');
        }
        line.push_str(": ");
        line.push_str(&text);
        out.push(line);
    }

    out
}

#[cfg(feature = "llm-ollama")]
fn entity_attr_string(db: &PathDB, entity_id: u32, key: &str) -> Option<String> {
    let key_id = db.interner.id_of(key)?;
    let value_id = db.entities.get_attr(entity_id, key_id)?;
    db.interner.lookup(value_id)
}

#[cfg(feature = "llm-ollama")]
fn truncate_for_prompt(s: &str, max_chars: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_chars).collect();
    format!("{truncated}…")
}

#[cfg(feature = "llm-ollama")]
fn tokenize_grounding_query(question: &str) -> BTreeSet<String> {
    // Keep it deterministic and aligned with PathDB's `fts` tokenizer (same
    // tokenization rules, stopwords, and minimum token length).
    let tokens = axiograph_pathdb::tokenize_fts_query(question);
    let out: BTreeSet<String> = tokens.into_iter().collect();
    out.into_iter().take(16).collect()
}

#[cfg(feature = "llm-ollama")]
fn ollama_summarize_answer(
    host: &str,
    model: &str,
    db: &PathDB,
    question: &str,
    query: &GeneratedQuery,
    result: &ExecutionResult,
) -> Result<String> {
    let mut preview = result.to_plugin_results(db);
    if preview.rows.len() > 40 {
        preview.rows.truncate(40);
        preview.truncated = true;
    }

    let user = format!(
        r#"Question:
{question}

Query:
{query_json}

Results (preview):
{results_json}

Return a single JSON object with an \"answer\" field.

Write a concise answer grounded ONLY in the results. If the results are empty, say you don't know."#,
        query_json = match query {
            GeneratedQuery::Axql(q) => format!("AxQL: {q}"),
            GeneratedQuery::QueryIrV1(ir) => format!(
                "query_ir_v1 (compiled): {}",
                ir.to_axql_text()
                    .unwrap_or_else(|_| "<invalid query_ir_v1>".to_string())
            ),
        },
        results_json =
            serde_json::to_string_pretty(&preview).unwrap_or_else(|_| "<unprintable>".to_string()),
    );

    let system = r#"You answer questions about an Axiograph snapshot using ONLY the provided query results.
Do not invent entities or relationships that are not in the results.
Be concise.
Return JSON only: {"answer": "..."}."#;

    #[derive(Deserialize)]
    struct AnswerPayload {
        answer: String,
    }

    // `format: "json"` keeps this compatible with a wider range of models.
    let content = ollama_chat(host, model, &user, Some(system), Some(json!("json")))?;
    match parse_llm_json_object::<AnswerPayload>(&content) {
        Ok(payload) => Ok(payload.answer),
        Err(_) => Ok(content.trim().to_string()),
    }
}

#[cfg(feature = "llm-ollama")]
pub(crate) fn ollama_chat(
    host: &str,
    model: &str,
    user: &str,
    system: Option<&str>,
    format: Option<serde_json::Value>,
) -> Result<String> {
    let timeout = llm_timeout(None)?;
    ollama_chat_with_timeout(host, model, user, system, format, timeout)
}

#[cfg(feature = "llm-ollama")]
pub(crate) fn ollama_chat_with_timeout(
    host: &str,
    model: &str,
    user: &str,
    system: Option<&str>,
    format: Option<serde_json::Value>,
    timeout: Option<Duration>,
) -> Result<String> {
    let host = normalize_ollama_host(host);
    let url = format!("{host}/api/chat");

    let mut messages = Vec::new();
    if let Some(system) = system {
        messages.push(json!({ "role": "system", "content": system }));
    }
    messages.push(json!({ "role": "user", "content": user }));

    let mut body = json!({
        "model": model,
        "stream": false,
        "messages": messages,
        "options": {
            "temperature": 0
        }
    });
    let has_format = format.is_some();
    if let Some(format) = format {
        body["format"] = format;
    }

    let mut builder = reqwest::blocking::Client::builder();
    if let Some(timeout) = timeout {
        builder = builder.timeout(timeout);
    }
    let client = builder
        .build()
        .map_err(|e| anyhow!("failed to build http client: {e}"))?;

    let send = |payload: &serde_json::Value| -> Result<reqwest::blocking::Response> {
        client
            .post(&url)
            .json(payload)
            .send()
            .map_err(|e| anyhow!(
                "failed to reach ollama at {url} (is it running?) ({e}). Try: `ollama serve` or set OLLAMA_HOST / pass `--llm-ollama-host`"
            ))
    };

    let mut resp = send(&body)?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();

        // Compatibility fallback:
        // Some Ollama versions expect `format` to be a JSON schema object (not the
        // string `"json"`). When they reject it, retry once with `format` omitted.
        if has_format && text.contains("invalid JSON schema in format") {
            let mut body2 = body.clone();
            if let Some(obj) = body2.as_object_mut() {
                obj.remove("format");
            }
            resp = send(&body2)?;
            if !resp.status().is_success() {
                let status2 = resp.status();
                let text2 = resp.text().unwrap_or_default();
                return Err(anyhow!("ollama http error {status2}: {text2}"));
            }
        } else {
            return Err(anyhow!("ollama http error {status}: {text}"));
        }
    }

    #[derive(Deserialize)]
    struct OllamaChatResponse {
        message: OllamaChatMessage,
    }

    #[derive(Deserialize)]
    struct OllamaChatMessage {
        content: String,
    }

    let out: OllamaChatResponse = resp
        .json()
        .map_err(|e| anyhow!("ollama returned invalid JSON: {e}"))?;
    Ok(out.message.content)
}

/// Compute embeddings for a batch of texts using Ollama.
///
/// Endpoint strategy:
/// - Prefer `/api/embed` (batched) when available.
/// - Fallback to `/api/embeddings` (per-item) for older Ollama versions.
#[cfg(feature = "llm-ollama")]
pub(crate) fn ollama_embed_texts_with_timeout(
    host: &str,
    model: &str,
    texts: &[String],
    timeout: Option<Duration>,
) -> Result<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }

    let host = normalize_ollama_host(host);

    let mut builder = reqwest::blocking::Client::builder();
    if let Some(timeout) = timeout {
        builder = builder.timeout(timeout);
    }
    let client = builder
        .build()
        .map_err(|e| anyhow!("failed to build http client: {e}"))?;

    // ---------------------------------------------------------------------
    // Try the newer batched endpoint first: `/api/embed`.
    // ---------------------------------------------------------------------
    let url_embed = format!("{host}/api/embed");
    let body_embed = serde_json::json!({
        "model": model,
        "input": texts,
        "truncate": true
    });

    let resp_embed = client.post(&url_embed).json(&body_embed).send();
    match resp_embed {
        Ok(resp) if resp.status().is_success() => {
            #[derive(Deserialize)]
            struct EmbedResp {
                embeddings: Vec<Vec<f32>>,
            }

            let out: EmbedResp = resp
                .json()
                .map_err(|e| anyhow!("ollama /api/embed returned invalid JSON: {e}"))?;
            if out.embeddings.len() != texts.len() {
                return Err(anyhow!(
                    "ollama /api/embed returned {} embeddings for {} inputs",
                    out.embeddings.len(),
                    texts.len()
                ));
            }
            return Ok(out.embeddings);
        }
        Ok(resp) => {
            // Non-success: fall back to `/api/embeddings` (older versions).
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            let _ = (status, text);
        }
        Err(e) => {
            // If we can't even reach Ollama, surface that error instead of masking.
            return Err(anyhow!(
                "failed to reach ollama at {url_embed} (is it running?) ({e}). Try: `ollama serve` or set OLLAMA_HOST"
            ));
        }
    }

    // ---------------------------------------------------------------------
    // Fallback: `/api/embeddings` (per-item).
    // ---------------------------------------------------------------------
    let url = format!("{host}/api/embeddings");
    #[derive(Deserialize)]
    struct EmbeddingsResp {
        embedding: Vec<f32>,
    }

    let mut out: Vec<Vec<f32>> = Vec::with_capacity(texts.len());
    for t in texts {
        let body = serde_json::json!({
            "model": model,
            "prompt": t
        });
        let resp = client
            .post(&url)
            .json(&body)
            .send()
            .map_err(|e| anyhow!(
                "failed to reach ollama at {url} (is it running?) ({e}). Try: `ollama serve` or set OLLAMA_HOST"
            ))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            return Err(anyhow!("ollama http error {status}: {text}"));
        }

        let r: EmbeddingsResp = resp
            .json()
            .map_err(|e| anyhow!("ollama /api/embeddings returned invalid JSON: {e}"))?;
        out.push(r.embedding);
    }

    Ok(out)
}

#[cfg(feature = "llm-ollama")]
pub(crate) fn parse_llm_json_object<T: for<'de> Deserialize<'de>>(text: &str) -> Result<T> {
    let trimmed = text.trim();
    if let Ok(v) = serde_json::from_str(trimmed) {
        return Ok(v);
    }

    // Best-effort: extract the first JSON object substring.
    let Some(start) = trimmed.find('{') else {
        return Err(anyhow!("LLM did not return JSON (no '{{' found)"));
    };
    let Some(end) = trimmed.rfind('}') else {
        return Err(anyhow!("LLM did not return JSON (no '}}' found)"));
    };
    let candidate = &trimmed[start..=end];
    serde_json::from_str(candidate).map_err(|e| anyhow!("LLM returned invalid JSON: {e}"))
}

#[cfg(test)]
mod tests {
    use super::normalize_axql_candidate;

    #[test]
    fn normalizes_bare_atom_to_where_clause() {
        let s = normalize_axql_candidate("?x : Node");
        assert_eq!(s, "where ?x : Node");
        crate::axql::parse_axql_query(&s).expect("normalized query parses");
    }

    #[test]
    fn strips_trailing_semicolons() {
        let s = normalize_axql_candidate("where ?x : Node;");
        assert_eq!(s, "where ?x : Node");
        crate::axql::parse_axql_query(&s).expect("normalized query parses");
    }

    #[test]
    fn strips_axql_prefix() {
        let s = normalize_axql_candidate("axql: where ?x : Node limit 1");
        assert_eq!(s, "where ?x : Node limit 1");
        crate::axql::parse_axql_query(&s).expect("normalized query parses");
    }

    #[test]
    fn rewrites_common_ollama_mistakes_to_parseable_axql() {
        let cases = [
            // `?x is "..."` (should become `?x.name = "..."`)
            r#"select ?rpc where ?svc is "acme.svc0.v1.Service0", ?svc -proto_service_has_rpc-> ?rpc"#,
            // `?x :name = ...` (should become `?x.name = ...`)
            r#"select ?ep where ?rpc :ProtoRpc, ?rpc :name = "GetWidget", ?rpc :full_name = "acme.svc0.v1.Service0.GetWidget", ?rpc -proto_rpc_http_endpoint-> ?ep"#,
            // `[limit 10]` (should become `limit 10`)
            r#"select ?x where doc_proto_api_0 -mentions_http_endpoint|mentions_rpc-> ?x [limit 10]"#,
            r#"select ?next where acme.svc0.v1.Service0.CreateWidget -observed_next-> ?next [LIMIT 10]"#,
        ];

        for raw in cases {
            let normalized = normalize_axql_candidate(raw);
            crate::axql::parse_axql_query(&normalized).unwrap_or_else(|e| {
                panic!(
                    "normalized query must parse\nraw: {raw}\nnormalized: {normalized}\nerr: {e}"
                )
            });
        }
    }

    #[test]
    fn semantic_search_token_hnsw_finds_basic_entities() {
        let mut db = axiograph_pathdb::PathDB::new();
        db.add_entity("Person", vec![("name", "Alice"), ("description", "likes cats")]);
        db.add_entity("Person", vec![("name", "Bob"), ("description", "likes dogs")]);
        db.add_entity(
            "DocChunk",
            vec![
                ("chunk_id", "chunk_0"),
                ("document_id", "doc"),
                ("span_id", "s0"),
                ("text", "Alice is Bob's parent."),
                ("search_text", "kind=demo_note"),
            ],
        );
        db.build_indexes();

        let args = serde_json::json!({
            "query": "alice",
            "entity_limit": 10,
            "chunk_limit": 10
        });
        let out = super::tool_semantic_search(
            &db,
            &args,
            "test_snapshot_semantic_search_hnsw",
            super::ToolLoopOptions::default(),
            None,
            None,
        )
        .expect("semantic_search");

        let entities = out["entity_hits"].as_array().expect("entity_hits array");
        assert!(
            entities
                .iter()
                .any(|e| e["entity"]["name"].as_str() == Some("Alice")),
            "expected Alice in entity_hits: {out}"
        );

        let chunks = out["chunk_hits"].as_array().expect("chunk_hits array");
        assert!(
            chunks.iter().any(|c| c["chunk_id"].as_str() == Some("chunk_0")),
            "expected chunk_0 in chunk_hits: {out}"
        );
    }

    #[test]
    fn docchunk_get_resolves_by_chunk_id_and_truncates_text() {
        let mut db = axiograph_pathdb::PathDB::new();
        db.add_entity(
            "DocChunk",
            vec![
                ("chunk_id", "chunk_0"),
                ("document_id", "doc"),
                ("span_id", "s0"),
                ("text", "Alice is Bob's parent. Alice is Bob's parent. Alice is Bob's parent."),
            ],
        );
        db.build_indexes();

        let args = serde_json::json!({
            "chunk_id": "chunk_0",
            "max_chars": 32
        });
        let out = super::tool_docchunk_get(&db, &args, super::ToolLoopOptions::default())
            .expect("docchunk_get");

        assert_eq!(out["chunk_id"].as_str(), Some("chunk_0"));
        assert_eq!(out["document_id"].as_str(), Some("doc"));
        assert_eq!(out["span_id"].as_str(), Some("s0"));
        assert_eq!(out["text"].as_str().unwrap_or("").chars().count(), 33); // 32 + ellipsis
        assert_eq!(out["text_truncated"].as_bool(), Some(true));
    }
}

// =============================================================================
// Executing generated queries
// =============================================================================

pub enum ExecutionResult {
    Axql(crate::axql::AxqlResult),
}

impl ExecutionResult {
    fn to_plugin_results(&self, db: &PathDB) -> PluginResultsV1 {
        match self {
            ExecutionResult::Axql(r) => PluginResultsV1::from_axql_result(db, r),
        }
    }
}

pub fn execute_generated_query(db: &PathDB, query: &GeneratedQuery) -> Result<ExecutionResult> {
    Ok(match query {
        GeneratedQuery::Axql(text) => {
            let q = crate::axql::parse_axql_query(text)?;
            ExecutionResult::Axql(crate::axql::execute_axql_query(db, &q)?)
        }
        GeneratedQuery::QueryIrV1(ir) => {
            let q = ir.to_axql_query()?;
            ExecutionResult::Axql(crate::axql::execute_axql_query(db, &q)?)
        }
    })
}

pub fn execute_generated_query_with_meta(
    db: &PathDB,
    query: &GeneratedQuery,
    meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
) -> Result<ExecutionResult> {
    Ok(match query {
        GeneratedQuery::Axql(text) => {
            let q = crate::axql::parse_axql_query(text)?;
            ExecutionResult::Axql(crate::axql::execute_axql_query_with_meta(db, &q, meta)?)
        }
        GeneratedQuery::QueryIrV1(ir) => {
            let q = ir.to_axql_query()?;
            ExecutionResult::Axql(crate::axql::execute_axql_query_with_meta(db, &q, meta)?)
        }
    })
}

// =============================================================================
// LLM tool loop (agentic, structured)
// =============================================================================

/// Tool-loop runner options.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ToolLoopOptions {
    pub max_steps: usize,
    pub max_rows: usize,
    pub max_doc_chunks: usize,
    pub max_doc_chars: usize,
}

impl Default for ToolLoopOptions {
    fn default() -> Self {
        Self {
            max_steps: 6,
            max_rows: 25,
            max_doc_chunks: 6,
            max_doc_chars: 420,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ToolLoopOutcome {
    pub steps: Vec<ToolLoopTranscriptItemV1>,
    pub final_answer: ToolLoopFinalV1,
    #[serde(default)]
    pub artifacts: ToolLoopArtifactsV1,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ToolSpecV1 {
    pub name: String,
    pub description: String,
    pub args_schema: serde_json::Value,
}

/// Tool-loop artifacts extracted and summarized by the backend.
///
/// Motivation:
/// - The LLM is untrusted; we do not want frontends to interpret an arbitrary
///   transcript to decide what happened.
/// - The backend extracts stable artifacts (e.g. a merged overlay) so UIs/REPLs
///   can render and optionally act on them without re-implementing logic.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct ToolLoopArtifactsV1 {
    /// A merged `proposals.json` overlay produced by the tool loop, if any.
    ///
    /// Shape matches the output of `propose_relation_proposals` /
    /// `propose_relations_proposals`:
    /// - proposals_json (Evidence/Proposals)
    /// - chunks (DocChunk evidence, optional)
    /// - summary (UI-friendly)
    /// - validation (optional; `ok` boolean used for safe gating)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_overlay: Option<serde_json::Value>,
    /// Latest drafted canonical `.axi` text produced by `draft_axi_from_proposals`, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drafted_axi: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ToolCallV1 {
    pub name: String,
    #[serde(default)]
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ToolLoopFinalV1 {
    pub answer: String,
    #[serde(default)]
    pub citations: Vec<String>,
    #[serde(default)]
    pub queries: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ToolLoopTranscriptItemV1 {
    pub tool: String,
    pub args: serde_json::Value,
    pub result: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
struct ToolLoopModelResponseV1 {
    #[serde(default)]
    tool_call: Option<ToolCallV1>,
    #[serde(default)]
    final_answer: Option<ToolLoopFinalV1>,
    #[serde(default)]
    error: Option<String>,
}

fn tool_loop_extract_artifacts(transcript: &[ToolLoopTranscriptItemV1]) -> ToolLoopArtifactsV1 {
    let generated_overlay = tool_loop_extract_generated_overlay(transcript);
    let drafted_axi = transcript
        .iter()
        .rev()
        .find(|s| s.tool == "draft_axi_from_proposals")
        .map(|s| s.result.clone())
        .and_then(|v| if v.get("error").is_some() { None } else { Some(v) });

    ToolLoopArtifactsV1 {
        generated_overlay,
        drafted_axi,
    }
}

fn tool_loop_extract_generated_overlay(transcript: &[ToolLoopTranscriptItemV1]) -> Option<serde_json::Value> {
    #[derive(Clone, Deserialize)]
    struct OverlayToolResult {
        proposals_json: ProposalsFileV1,
        #[serde(default)]
        chunks: Vec<Chunk>,
        #[serde(default)]
        summary: serde_json::Value,
        #[serde(default)]
        validation: Option<serde_json::Value>,
    }

    #[derive(Clone)]
    struct OverlayStep {
        tool: String,
        args: serde_json::Value,
        out: OverlayToolResult,
    }

    let mut overlays: Vec<OverlayStep> = Vec::new();
    for step in transcript {
        if step.tool != "propose_relation_proposals" && step.tool != "propose_relations_proposals" {
            continue;
        }
        if step.result.get("error").is_some() {
            continue;
        }
        let Ok(out) = serde_json::from_value::<OverlayToolResult>(step.result.clone()) else {
            continue;
        };
        overlays.push(OverlayStep {
            tool: step.tool.clone(),
            args: step.args.clone(),
            out,
        });
    }

    if overlays.is_empty() {
        return None;
    }

    // Merge proposals + chunks deterministically and provide a stable summary so
    // frontends don't need to interpret the transcript.
    let mut proposal_seen: HashSet<String> = HashSet::new();
    let mut proposals: Vec<ProposalV1> = Vec::new();
    let mut schema_hint: Option<String> = None;

    for ov in &overlays {
        if schema_hint.is_none() {
            schema_hint = ov.out.proposals_json.schema_hint.clone();
        }
        for p in &ov.out.proposals_json.proposals {
            let id = match p {
                ProposalV1::Entity { meta, .. } => meta.proposal_id.clone(),
                ProposalV1::Relation { meta, .. } => meta.proposal_id.clone(),
            };
            if proposal_seen.insert(id) {
                proposals.push(p.clone());
            }
        }
    }

    let mut chunk_seen: HashSet<String> = HashSet::new();
    let mut chunks: Vec<Chunk> = Vec::new();
    for ov in &overlays {
        for c in &ov.out.chunks {
            if chunk_seen.insert(c.chunk_id.clone()) {
                chunks.push(c.clone());
            }
        }
    }
    chunks.sort_by(|a, b| a.chunk_id.cmp(&b.chunk_id));

    proposals.sort_by(|a, b| {
        let aid = match a {
            ProposalV1::Entity { meta, .. } => &meta.proposal_id,
            ProposalV1::Relation { meta, .. } => &meta.proposal_id,
        };
        let bid = match b {
            ProposalV1::Entity { meta, .. } => &meta.proposal_id,
            ProposalV1::Relation { meta, .. } => &meta.proposal_id,
        };
        aid.cmp(bid)
    });

    let merged_file = ProposalsFileV1 {
        version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
        generated_at: "llm_tool_loop".to_string(),
        source: ProposalSourceV1 {
            source_type: "llm_tool_loop".to_string(),
            locator: "axiograph_llm_agent".to_string(),
        },
        schema_hint,
        proposals,
    };

    // Summarize args into a UI-friendly “what was proposed” payload.
    let mut rel_types: BTreeSet<String> = BTreeSet::new();
    let mut source_names: BTreeSet<String> = BTreeSet::new();
    let mut target_names: BTreeSet<String> = BTreeSet::new();
    let mut tools_used: BTreeSet<String> = BTreeSet::new();
    let mut context: Option<String> = None;
    let mut pairing: Option<String> = None;
    let mut confidence: Option<f64> = None;

    for ov in &overlays {
        tools_used.insert(ov.tool.clone());
        if let Some(rt) = ov.args.get("rel_type").and_then(|v| v.as_str()) {
            if !rt.trim().is_empty() {
                rel_types.insert(rt.trim().to_string());
            }
        }

        if let Some(ctx) = ov.args.get("context").and_then(|v| v.as_str()) {
            let ctx = ctx.trim();
            if !ctx.is_empty() {
                context = Some(ctx.to_string());
            }
        }

        if let Some(c) = ov.args.get("confidence").and_then(|v| v.as_f64()) {
            confidence = Some(c);
        }

        if ov.tool == "propose_relations_proposals" {
            if let Some(p) = ov.args.get("pairing").and_then(|v| v.as_str()) {
                let p = p.trim();
                if !p.is_empty() {
                    pairing = Some(p.to_string());
                }
            }
            if let Some(arr) = ov.args.get("source_names").and_then(|v| v.as_array()) {
                for s in arr {
                    if let Some(name) = s.as_str() {
                        let name = name.trim();
                        if !name.is_empty() {
                            source_names.insert(name.to_string());
                        }
                    }
                }
            }
            if let Some(arr) = ov.args.get("target_names").and_then(|v| v.as_array()) {
                for s in arr {
                    if let Some(name) = s.as_str() {
                        let name = name.trim();
                        if !name.is_empty() {
                            target_names.insert(name.to_string());
                        }
                    }
                }
            }
        } else {
            if let Some(name) = ov.args.get("source_name").and_then(|v| v.as_str()) {
                let name = name.trim();
                if !name.is_empty() {
                    source_names.insert(name.to_string());
                }
            }
            if let Some(name) = ov.args.get("target_name").and_then(|v| v.as_str()) {
                let name = name.trim();
                if !name.is_empty() {
                    target_names.insert(name.to_string());
                }
            }
        }
    }

    let rel_type_single = if rel_types.len() == 1 {
        rel_types.iter().next().cloned()
    } else {
        None
    };

    let summary = serde_json::json!({
        "kind": "merged_llm_overlay_v1",
        "overlays": overlays.len(),
        "rel_type": rel_type_single,
        "rel_types": rel_types.into_iter().collect::<Vec<_>>(),
        "source_names": source_names.into_iter().collect::<Vec<_>>(),
        "target_names": target_names.into_iter().collect::<Vec<_>>(),
        "pairing": pairing,
        "context": context,
        "confidence": confidence,
        "tools": tools_used.into_iter().collect::<Vec<_>>(),
    });

    // Validation merge: compute a safe `ok` bit when possible.
    let mut ok_bits: Vec<bool> = Vec::new();
    let mut sources: Vec<serde_json::Value> = Vec::new();
    for ov in &overlays {
        if let Some(v) = ov.out.validation.clone() {
            if let Some(ok) = v.get("ok").and_then(|x| x.as_bool()) {
                ok_bits.push(ok);
            }
            sources.push(v);
        }
    }
    let validation = if ok_bits.is_empty() {
        None
    } else {
        let ok = !ok_bits.iter().any(|b| !*b);
        Some(serde_json::json!({
            "ok": ok,
            "note": "merged validations from tool-loop proposal generation",
            "sources": sources,
        }))
    };

    Some(serde_json::json!({
        "proposals_json": merged_file,
        "chunks": chunks,
        "summary": summary,
        "validation": validation,
    }))
}

/// Run a structured tool loop:
/// - the model proposes tool calls,
/// - Rust executes tools against the loaded snapshot,
/// - the model produces a final answer grounded in tool outputs.
///
/// This is designed to avoid brittle “LLM outputs raw AxQL text”.
fn finalize_tool_loop_outcome(
    steps: Vec<ToolLoopTranscriptItemV1>,
    final_answer: ToolLoopFinalV1,
) -> ToolLoopOutcome {
    let artifacts = tool_loop_extract_artifacts(&steps);
    ToolLoopOutcome {
        steps,
        final_answer,
        artifacts,
    }
}

pub(crate) fn run_tool_loop_with_meta(
    llm: &LlmState,
    db: &PathDB,
    meta: Option<&MetaPlaneIndex>,
    default_contexts: &[crate::axql::AxqlContextSpec],
    snapshot_key: &str,
    embeddings: Option<&crate::embeddings::ResolvedEmbeddingsIndexV1>,
    ollama_embed_host: Option<&str>,
    query_cache: &mut crate::axql::AxqlPreparedQueryCache,
    question: &str,
    options: ToolLoopOptions,
) -> Result<ToolLoopOutcome> {
    let schema = match meta {
        Some(m) => SchemaContextV1::from_db_with_meta(db, m),
        None => SchemaContextV1::from_db(db),
    };
    let tools = tool_loop_tools_schema();

    let mut transcript: Vec<ToolLoopTranscriptItemV1> = Vec::new();
    // RAG-like flow (backend-owned): prefetch a compact overview + semantic-ish
    // retrieval pack before the first model step. This avoids relying on the
    // model to “remember to retrieve”, and keeps prompts stable on large
    // snapshots.
    //
    // Note: these are *tool outputs*; they are untrusted and may be truncated
    // for the model. The model can always call tools again for deeper detail.
    if transcript.is_empty() {
        let summary_args = serde_json::json!({
            "max_types": 12,
            "max_relations": 12,
            "max_relation_samples": 2
        });
        let mut doc_chunks_loaded = false;
        if let Ok(result) = tool_db_summary(db, &summary_args) {
            doc_chunks_loaded = result
                .get("doc_chunks_loaded")
                .and_then(|x| x.as_bool())
                .unwrap_or(false);
            transcript.push(ToolLoopTranscriptItemV1 {
                tool: "db_summary".to_string(),
                args: summary_args,
                result,
            });
        }

        // Meta-plane prefetch (RAG-like): if the question mentions a known
        // schema type or relation, pull its declaration/constraints so the
        // model has a local “ontology excerpt” before it starts tool-calling.
        if let Some(meta) = meta {
            let terms = extract_identifier_like_terms(question, 36);

            let mut rel_lut: HashSet<String> = HashSet::new();
            let mut type_lut: HashSet<String> = HashSet::new();
            for s in meta.schemas.values() {
                for r in s.relation_decls.values() {
                    rel_lut.insert(r.name.to_ascii_lowercase());
                }
                for t in &s.object_types {
                    type_lut.insert(t.to_ascii_lowercase());
                }
            }

            let want_rels = llm_prefetch_lookup_relations().unwrap_or(0);
            let want_types = llm_prefetch_lookup_types().unwrap_or(0);

            let mut chosen_rels: Vec<String> = Vec::new();
            let mut chosen_types: Vec<String> = Vec::new();
            for t in &terms {
                let lc = t.to_ascii_lowercase();
                if chosen_rels.len() < want_rels && rel_lut.contains(&lc) {
                    chosen_rels.push(t.clone());
                }
                if chosen_types.len() < want_types && type_lut.contains(&lc) {
                    chosen_types.push(t.clone());
                }
                if chosen_rels.len() >= want_rels && chosen_types.len() >= want_types {
                    break;
                }
            }

            for rel in chosen_rels.into_iter().take(want_rels) {
                let args = serde_json::json!({ "relation": rel });
                if let Ok(v) = tool_lookup_relation(Some(meta), &args) {
                    transcript.push(ToolLoopTranscriptItemV1 {
                        tool: "lookup_relation".to_string(),
                        args,
                        result: v,
                    });
                }
            }

            for ty in chosen_types.into_iter().take(want_types) {
                let args = serde_json::json!({ "type": ty });
                if let Ok(v) = tool_lookup_type(db, Some(meta), &args) {
                    transcript.push(ToolLoopTranscriptItemV1 {
                        tool: "lookup_type".to_string(),
                        args,
                        result: v,
                    });
                }
            }
        }

        let search_args = serde_json::json!({
            "query": truncate_preview(question, 420),
            "entity_limit": 8,
            "chunk_limit": options.max_doc_chunks.min(8).max(1),
        });
        if let Ok(result) = tool_semantic_search(
            db,
            &search_args,
            snapshot_key,
            options,
            embeddings,
            ollama_embed_host,
        ) {
            // Add the search itself.
            transcript.push(ToolLoopTranscriptItemV1 {
                tool: "semantic_search".to_string(),
                args: search_args,
                result: result.clone(),
            });

            // Follow up on the top entity hits with a small `describe_entity`
            // so the model has immediate neighborhood context (RAG-like pack).
            let describe_k = llm_prefetch_describe_entities().unwrap_or(0);
            if describe_k > 0 {
                if let Some(hits) = result.get("entity_hits").and_then(|x| x.as_array()) {
                    for hit in hits.iter().take(describe_k) {
                        let id = hit
                            .get("entity")
                            .and_then(|e| e.get("id"))
                            .and_then(|x| x.as_u64())
                            .and_then(|x| u32::try_from(x).ok());
                        let Some(id) = id else { continue };
                        let describe_args = serde_json::json!({
                            "id": id,
                            "max_attrs": 60,
                            "max_rel_types": 16,
                            "out_limit": 10,
                            "in_limit": 10
                        });
                        if let Ok(desc) = describe_entity_v1(db, &describe_args) {
                            transcript.push(ToolLoopTranscriptItemV1 {
                                tool: "describe_entity".to_string(),
                                args: describe_args,
                                result: desc,
                            });
                        }
                    }
                }
            }

            // Optional: fetch full DocChunk texts for the top chunk hits so the
            // model can quote/explain with more evidence context.
            let want_chunks = llm_prefetch_docchunks().unwrap_or(0);
            if want_chunks > 0 {
                if let Some(hits) = result.get("chunk_hits").and_then(|x| x.as_array()) {
                    for hit in hits.iter().take(want_chunks) {
                        let id = hit
                            .get("id")
                            .and_then(|x| x.as_u64())
                            .and_then(|x| u32::try_from(x).ok());
                        let chunk_id = hit
                            .get("chunk_id")
                            .and_then(|x| x.as_str())
                            .map(|s| s.to_string());
                        let args = if let Some(id) = id {
                            serde_json::json!({ "id": id, "max_chars": 2000 })
                        } else if let Some(chunk_id) = chunk_id {
                            serde_json::json!({ "chunk_id": chunk_id, "max_chars": 2000 })
                        } else {
                            continue;
                        };
                        if let Ok(v) = tool_docchunk_get(db, &args, options) {
                            transcript.push(ToolLoopTranscriptItemV1 {
                                tool: "docchunk_get".to_string(),
                                args,
                                result: v,
                            });
                        }
                    }
                }
            }

            // Optional fallback: if we have DocChunks but semantic_search did
            // not return any, run a small lexical FTS pass.
            if doc_chunks_loaded {
                let have_chunks = result
                    .get("chunk_hits")
                    .and_then(|x| x.as_array())
                    .map(|xs| !xs.is_empty())
                    .unwrap_or(false);
                if !have_chunks {
                    let fts_args = serde_json::json!({
                        "query": truncate_preview(question, 420),
                        "limit": options.max_doc_chunks.min(6).max(1),
                    });
                    if let Ok(fts) = tool_fts_chunks(db, &fts_args, options) {
                        transcript.push(ToolLoopTranscriptItemV1 {
                            tool: "fts_chunks".to_string(),
                            args: fts_args,
                            result: fts.clone(),
                        });

                        // If we prefetched chunk text, also expand the first few
                        // lexical hits (by chunk_id).
                        let want_chunks = llm_prefetch_docchunks().unwrap_or(0);
                        if want_chunks > 0 {
                            if let Some(hits) = fts.get("hits").and_then(|x| x.as_array()) {
                                for hit in hits.iter().take(want_chunks) {
                                    let chunk_id = hit
                                        .get("chunk_id")
                                        .and_then(|x| x.as_str())
                                        .map(|s| s.to_string());
                                    let Some(chunk_id) = chunk_id else { continue };
                                    let args = serde_json::json!({ "chunk_id": chunk_id, "max_chars": 2000 });
                                    if let Ok(v) = tool_docchunk_get(db, &args, options) {
                                        transcript.push(ToolLoopTranscriptItemV1 {
                                            tool: "docchunk_get".to_string(),
                                            args,
                                            result: v,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    for _step in 0..options.max_steps.max(1) {
        let resp = llm.tool_loop_step(
            db,
            question,
            &schema,
            &tools,
            &transcript,
            snapshot_key,
            embeddings,
            ollama_embed_host,
            options,
        )?;

        if let Some(err) = resp.error {
            // Treat a model-side error as a hard failure: it indicates the
            // backend could not comply with the JSON/tool schema.
            return Err(anyhow!("llm agent error: {err}"));
        }

        if let Some(model_final) = resp.final_answer {
            // Some local models "comply" by returning trivial placeholders like
            // "{}". Prefer a deterministic tool-grounded summary when we can.
            if is_trivial_model_answer(&model_final.answer) && !transcript.is_empty() {
                let mut final_answer = fallback_tool_loop_final_answer(
                    db,
                    question,
                    &transcript,
                    options,
                    "model returned a trivial final_answer; using tool-grounded summary",
                );
                if !model_final.answer.trim().is_empty() {
                    final_answer.notes.push(format!(
                        "model_answer_preview={}",
                        truncate_preview(&model_final.answer, 200)
                    ));
                }
                return Ok(finalize_tool_loop_outcome(transcript, final_answer));
            }

            return Ok(finalize_tool_loop_outcome(transcript, model_final));
        }

        let Some(tool_call) = resp.tool_call else {
            // Model returned neither a tool call nor a final answer.
            // Some local models will respond with `{}` in JSON mode.
            // Instead of hard-failing, stop and summarize deterministically.
            let final_answer = fallback_tool_loop_final_answer(
                db,
                question,
                &transcript,
                options,
                "model returned neither tool_call nor final_answer",
            );
            return Ok(finalize_tool_loop_outcome(transcript, final_answer));
        };

        let result = execute_tool_call(
            db,
            meta,
            default_contexts,
            snapshot_key,
            embeddings,
            ollama_embed_host,
            query_cache,
            &tool_call,
            options,
        );

        let item = ToolLoopTranscriptItemV1 {
            tool: tool_call.name.clone(),
            args: tool_call.args.clone(),
            result: match result {
                Ok(v) => v,
                Err(e) => serde_json::json!({ "error": e.to_string() }),
            },
        };
        transcript.push(item);
    }

    // Robust fallback:
    // Some models keep calling tools without ever emitting `final_answer`.
    // Instead of hard-failing (which makes the viz UI feel flaky), we
    // deterministically summarize the latest successful tool output.
    let final_answer = fallback_tool_loop_final_answer(
        db,
        question,
        &transcript,
        options,
        "max tool-loop steps reached without final_answer",
    );
    Ok(finalize_tool_loop_outcome(transcript, final_answer))
}

fn fallback_tool_loop_final_answer(
    db: &PathDB,
    question: &str,
    transcript: &[ToolLoopTranscriptItemV1],
    options: ToolLoopOptions,
    reason: &str,
) -> ToolLoopFinalV1 {
    // Prefer the latest `axql_run` result, because it is the most directly
    // answer-like output and also drives UI highlighting.
    for item in transcript.iter().rev() {
        if item.tool != "axql_run" {
            continue;
        }

        #[derive(Deserialize)]
        struct AxqlRunPayload {
            results: PluginResultsV1,
            #[serde(default)]
            query: Option<String>,
        }

        if let Ok(payload) = serde_json::from_value::<AxqlRunPayload>(item.result.clone()) {
            let mut lines = Vec::new();
            if payload.results.rows.is_empty() {
                lines.push("No results.".to_string());
            } else {
                lines.push(format!("Found {} result(s).", payload.results.rows.len()));
                for (i, row) in payload.results.rows.iter().enumerate().take(6) {
                    let mut parts = Vec::new();
                    for (k, v) in row {
                        let label = v.name.clone().unwrap_or_else(|| v.id.to_string());
                        parts.push(format!("{k}={label}"));
                    }
                    lines.push(format!("row {i}: {}", parts.join(", ")));
                }
            }

            let mut notes = Vec::new();
            notes.push(format!("auto-finalized: {reason} (max_steps={})", options.max_steps));
            notes.push(format!("question: {question}"));
            notes.push(format!("snapshot_entities={}", db.entities.len()));

            let mut queries = Vec::new();
            if let Some(q) = payload.query {
                queries.push(q);
            }

            return ToolLoopFinalV1 {
                answer: lines.join("\n"),
                citations: Vec::new(),
                queries,
                notes,
            };
        }

        if let Some(err) = item.result.get("error").and_then(|v| v.as_str()) {
            return ToolLoopFinalV1 {
                answer: format!("Tool error: {err}"),
                citations: Vec::new(),
                queries: Vec::new(),
                notes: vec![format!("auto-finalized: {reason} (max_steps={})", options.max_steps)],
            };
        }
    }

    // Next best: summarize the latest `describe_entity` result.
    for item in transcript.iter().rev() {
        if item.tool != "describe_entity" {
            continue;
        }

        if let Some(err) = item.result.get("error").and_then(|v| v.as_str()) {
            return ToolLoopFinalV1 {
                answer: format!("Tool error: {err}"),
                citations: Vec::new(),
                queries: Vec::new(),
                notes: vec![format!("auto-finalized: {reason} (max_steps={})", options.max_steps)],
            };
        }

        let entity = item
            .result
            .get("entity")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let name = entity
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("(no name)");
        let entity_type = entity
            .get("entity_type")
            .and_then(|v| v.as_str())
            .unwrap_or("(unknown type)");

        fn summarize_edge_groups(v: &serde_json::Value, dir: &str, max_groups: usize) -> Vec<String> {
            let mut lines = Vec::new();
            let Some(groups) = v.as_array() else {
                return lines;
            };
            for g in groups.iter().take(max_groups) {
                let rel = g.get("rel").and_then(|x| x.as_str()).unwrap_or("?");
                let edges = g.get("edges").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                let mut targets = Vec::new();
                for e in edges.iter().take(6) {
                    let ent = e.get("entity").cloned().unwrap_or_else(|| serde_json::json!({}));
                    let ename = ent.get("name").and_then(|x| x.as_str()).unwrap_or("");
                    let ety = ent.get("entity_type").and_then(|x| x.as_str()).unwrap_or("");
                    let id = ent.get("id").and_then(|x| x.as_u64()).unwrap_or(0);
                    if !ename.is_empty() {
                        targets.push(format!("{ename}#{id}"));
                    } else if !ety.is_empty() {
                        targets.push(format!("{ety}#{id}"));
                    } else {
                        targets.push(format!("#{id}"));
                    }
                }
                if !targets.is_empty() {
                    lines.push(format!("{dir} {rel}: {}", targets.join(", ")));
                }
            }
            lines
        }

        let mut lines = Vec::new();
        lines.push(format!("{entity_type} {name}"));
        let outgoing = item
            .result
            .get("outgoing")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([]));
        lines.extend(summarize_edge_groups(&outgoing, "out", 8));

        let incoming = item
            .result
            .get("incoming")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([]));
        lines.extend(summarize_edge_groups(&incoming, "in", 8));
        if lines.len() == 1 {
            lines.push("(no edges in sample)".to_string());
        }

        return ToolLoopFinalV1 {
            answer: lines.join("\n"),
            citations: Vec::new(),
            queries: Vec::new(),
            notes: vec![
                format!("auto-finalized: {reason} (max_steps={})", options.max_steps),
                format!("question: {question}"),
            ],
        };
    }

    // Next: surface proposal generation as a user-facing artifact.
    for item in transcript.iter().rev() {
        if item.tool != "propose_relation_proposals" && item.tool != "propose_relations_proposals" {
            continue;
        }

        if let Some(err) = item.result.get("error").and_then(|v| v.as_str()) {
            return ToolLoopFinalV1 {
                answer: format!("Tool error: {err}"),
                citations: Vec::new(),
                queries: Vec::new(),
                notes: vec![format!("auto-finalized: {reason} (max_steps={})", options.max_steps)],
            };
        }

        let summary = item.result.get("summary").cloned().unwrap_or_default();
        let proposals = item.result.get("proposals_json").cloned().unwrap_or_default();
        let mut lines = Vec::new();
        lines.push("Generated a reviewable `proposals.json` overlay (untrusted).".to_string());
        if let Some(rel) = summary.get("rel_type").and_then(|v| v.as_str()) {
            let ctx = summary.get("context").and_then(|v| v.as_str());

            // Prefer the canonical source/target names when present; otherwise
            // fall back to tool args (useful for the batch tool).
            let src = summary
                .get("source_name")
                .and_then(|v| v.as_str())
                .or_else(|| item.args.get("source_name").and_then(|v| v.as_str()))
                .unwrap_or("?");
            let dst = summary
                .get("target_name")
                .and_then(|v| v.as_str())
                .or_else(|| item.args.get("target_name").and_then(|v| v.as_str()))
                .unwrap_or("?");

            if item.tool == "propose_relations_proposals" {
                let sources = item
                    .args
                    .get("source_names")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                let targets = item
                    .args
                    .get("target_names")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                if let Some(ctx) = ctx {
                    lines.push(format!("Proposed: {rel} for {sources}×{targets} (context={ctx})"));
                } else {
                    lines.push(format!("Proposed: {rel} for {sources}×{targets}"));
                }
            } else if let Some(ctx) = ctx {
                lines.push(format!("Proposed: {src} -{rel}-> {dst} (context={ctx})"));
            } else {
                lines.push(format!("Proposed: {src} -{rel}-> {dst}"));
            }
        }
        lines.push("".to_string());
        lines.push(serde_json::to_string_pretty(&proposals).unwrap_or_else(|_| "{}".to_string()));
        lines.push("".to_string());
        lines.push("Next: commit this to the PathDB WAL (evidence plane), review, then promote into accepted `.axi` when ready.".to_string());

        return ToolLoopFinalV1 {
            answer: lines.join("\n"),
            citations: Vec::new(),
            queries: Vec::new(),
            notes: vec![
                format!("auto-finalized: {reason} (max_steps={})", options.max_steps),
                format!("question: {question}"),
            ],
        };
    }

    // Next: summarize a `db_summary` result (useful for overview questions).
    for item in transcript.iter().rev() {
        if item.tool != "db_summary" {
            continue;
        }

        if let Some(err) = item.result.get("error").and_then(|v| v.as_str()) {
            return ToolLoopFinalV1 {
                answer: format!("Tool error: {err}"),
                citations: Vec::new(),
                queries: Vec::new(),
                notes: vec![format!("auto-finalized: {reason} (max_steps={})", options.max_steps)],
            };
        }

        let entities = item.result.get("entities").and_then(|v| v.as_u64()).unwrap_or(0);
        let relations = item.result.get("relations").and_then(|v| v.as_u64()).unwrap_or(0);
        let doc_chunks_loaded = item
            .result
            .get("doc_chunks_loaded")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut lines = Vec::new();
        lines.push(format!(
            "The current snapshot has {entities} entities and {relations} relations."
        ));
        if doc_chunks_loaded {
            lines.push("Document chunks are loaded (the assistant can cite them as evidence when answering).".to_string());
        } else {
            lines.push("No document chunks are loaded (answers are based only on graph structure).".to_string());
        }

        if let Some(ctxs) = item.result.get("contexts").and_then(|v| v.as_array()) {
            let mut parts = Vec::new();
            for c in ctxs.iter().take(12) {
                let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if !name.is_empty() {
                    parts.push(name.to_string());
                }
            }
            if !parts.is_empty() {
                lines.push(format!("Contexts/worlds: {}", parts.join(", ")));
            }
        }

        if let Some(types) = item.result.get("types").and_then(|v| v.as_array()) {
            let mut parts = Vec::new();
            for t in types.iter().take(8) {
                let ty = t.get("type").and_then(|v| v.as_str()).unwrap_or("?");
                let c = t.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
                let sample = t
                    .get("sample")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str())
                            .take(4)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if sample.is_empty() {
                    parts.push(format!("{ty}({c})"));
                } else {
                    parts.push(format!("{ty}({c}): {}", sample.join(", ")));
                }
            }
            if !parts.is_empty() {
                lines.push(format!("Entities include: {}", parts.join(" • ")));
            }
        }

        if let Some(rels) = item
            .result
            .get("relations_by_type")
            .and_then(|v| v.as_array())
        {
            let mut parts = Vec::new();
            for r in rels.iter().take(10) {
                let rel = r.get("rel").and_then(|v| v.as_str()).unwrap_or("?");
                let c = r.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
                parts.push(format!("{rel}({c})"));
            }
            if !parts.is_empty() {
                lines.push(format!("Relation types: {}", parts.join(", ")));
            }
        }

        return ToolLoopFinalV1 {
            answer: lines.join("\n"),
            citations: Vec::new(),
            queries: Vec::new(),
            notes: vec![
                format!("auto-finalized: {reason} (max_steps={})", options.max_steps),
                format!("question: {question}"),
            ],
        };
    }

    // If we never even ran a query/tool that yields a summary, produce a minimal, honest response.
    ToolLoopFinalV1 {
        answer: "No results (LLM did not produce a final answer).".to_string(),
        citations: Vec::new(),
        queries: Vec::new(),
        notes: vec![format!("auto-finalized: {reason} (max_steps={})", options.max_steps)],
    }
}

fn is_trivial_model_answer(answer: &str) -> bool {
    match answer.trim() {
        "" | "{}" | "null" | "[]" => true,
        other => other.len() <= 2 && other.chars().all(|c| c == '"' || c == '\''),
    }
}

fn tool_loop_tools_schema() -> Vec<ToolSpecV1> {
    vec![
        ToolSpecV1 {
            name: "db_summary".to_string(),
            description: "Summarize what is in the current snapshot (types/relations/contexts/evidence presence). Good first step for overview questions like “explain the facts”.".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "max_types": { "type": "integer", "minimum": 1, "maximum": 50 },
                    "max_relations": { "type": "integer", "minimum": 1, "maximum": 50 },
                    "max_relation_samples": { "type": "integer", "minimum": 0, "maximum": 10 }
                }
            }),
        },
        ToolSpecV1 {
            name: "semantic_search".to_string(),
            description: "Hybrid semantic-ish retrieval over the current snapshot: returns candidate entities and DocChunks relevant to a free-text query (approximate; untrusted).".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": { "type": "string" },
                    "entity_limit": { "type": "integer", "minimum": 1, "maximum": 50 },
                    "chunk_limit": { "type": "integer", "minimum": 1, "maximum": 50 }
                }
            }),
        },
        ToolSpecV1 {
            name: "lookup_entity".to_string(),
            description:
                "Resolve entities by `name` (exact match first; token/fuzzy fallback; optionally filtered by type)."
                    .to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "required": ["name"],
                "properties": {
                    "name": { "type": "string" },
                    "type": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50 }
                }
            }),
        },
        ToolSpecV1 {
            name: "describe_entity".to_string(),
            description: "Summarize an entity: attrs, contexts, equivalences, and grouped in/out edges (useful for “what is connected to X?”).".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "integer", "minimum": 0 },
                    "name": { "type": "string" },
                    "type": { "type": "string" },
                    "max_attrs": { "type": "integer", "minimum": 0, "maximum": 200 },
                    "max_rel_types": { "type": "integer", "minimum": 1, "maximum": 50 },
                    "out_limit": { "type": "integer", "minimum": 0, "maximum": 50 },
                    "in_limit": { "type": "integer", "minimum": 0, "maximum": 50 }
                }
            }),
        },
        ToolSpecV1 {
            name: "lookup_type".to_string(),
            description: "Inspect a type using the meta-plane (supertypes/subtypes, related relations).".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "required": ["type"],
                "properties": {
                    "type": { "type": "string" }
                }
            }),
        },
        ToolSpecV1 {
            name: "lookup_relation".to_string(),
            description: "Inspect a canonical relation declaration using the meta-plane (fields, inferred endpoint mapping, and theory constraints). Useful before generating proposals when direction/field mapping matters.".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "required": ["relation"],
                "properties": {
                    "relation": { "type": "string" },
                    "schema": { "type": "string" }
                }
            }),
        },
        ToolSpecV1 {
            name: "fts_chunks".to_string(),
            description: "Full-text search over `DocChunk` evidence (approximate).".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50 }
                }
            }),
        },
        ToolSpecV1 {
            name: "docchunk_get".to_string(),
            description: "Fetch a single `DocChunk` evidence record by `id` or `chunk_id`, returning bounded text (useful after `semantic_search`/`fts_chunks`).".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "integer", "minimum": 0 },
                    "chunk_id": { "type": "string" },
                    "max_chars": { "type": "integer", "minimum": 32, "maximum": 8000 }
                }
            }),
        },
        ToolSpecV1 {
            name: "axql_elaborate".to_string(),
            description: "Typecheck/elaborate an AxQL query using the meta-plane, returning the elaborated query + inferred types + plan.".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "axql": { "type": "string" },
                    "query_ir_v1": { "type": "object" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
                }
            }),
        },
        ToolSpecV1 {
            name: "axql_run".to_string(),
            description: "Run an AxQL query (or query_ir_v1) over the snapshot (uncertified, unless you later emit a certificate).".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "axql": { "type": "string" },
                    "query_ir_v1": { "type": "object" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
                }
            }),
        },
        ToolSpecV1 {
            name: "viz_render".to_string(),
            description: "Render an HTML neighborhood visualization (restricted to build/llm_agent).".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "required": ["focus_name"],
                "properties": {
                    "focus_name": { "type": "string" },
                    "hops": { "type": "integer", "minimum": 0, "maximum": 6 },
                    "plane": { "type": "string", "enum": ["data", "meta", "both"] },
                    "max_nodes": { "type": "integer", "minimum": 10, "maximum": 5000 },
                    "max_edges": { "type": "integer", "minimum": 10, "maximum": 50000 }
                }
            }),
        },
        ToolSpecV1 {
            name: "quality_report".to_string(),
            description: "Run practical quality/lint checks over the current DB snapshot.".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "profile": { "type": "string", "enum": ["fast", "strict"] },
                    "plane": { "type": "string", "enum": ["meta", "data", "both"] }
                }
            }),
        },
        ToolSpecV1 {
            name: "propose_axi_patch".to_string(),
            description: "Generate a draft canonical `.axi` module from a proposals.json file (deterministic, untrusted; for review).".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "required": ["proposals_path"],
                "properties": {
                    "proposals_path": { "type": "string" },
                    "module_name": { "type": "string" },
                    "schema_name": { "type": "string" },
                    "instance_name": { "type": "string" },
                    "infer_constraints": { "type": "boolean" }
                }
            }),
        },
        ToolSpecV1 {
            name: "draft_axi_from_proposals".to_string(),
            description: "Generate a draft canonical `.axi` module directly from an in-memory `proposals_json` object (deterministic, untrusted; for review).".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "required": ["proposals_json"],
                "properties": {
                    "proposals_json": { "type": "object" },
                    "module_name": { "type": "string" },
                    "schema_name": { "type": "string" },
                    "instance_name": { "type": "string" },
                    "infer_constraints": { "type": "boolean" }
                }
            }),
        },
        ToolSpecV1 {
            name: "propose_relation_proposals".to_string(),
            description: "Generate an untrusted `proposals.json` (Evidence/Proposals schema) for adding a simple relation assertion between two entities in the current snapshot. This does NOT mutate the DB; it produces a reviewable overlay artifact.\n\nImportant: by default, `source_name` binds to the canonical relation's source-ish field (`from`/`source`/`child`/`lhs`) and `target_name` binds to the target-ish field (`to`/`target`/`parent`/`rhs`). If you need to disambiguate direction, set `source_field` and `target_field` explicitly (e.g. for Parent(child,parent): source_field=\"parent\" target_field=\"child\").".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "required": ["rel_type", "source_name", "target_name"],
                "properties": {
                    "rel_type": { "type": "string" },
                    "source_name": { "type": "string" },
                    "target_name": { "type": "string" },
                    "source_type": { "type": "string" },
                    "target_type": { "type": "string" },
                    "source_field": { "type": "string" },
                    "target_field": { "type": "string" },
                    "context": { "type": "string" },
                    "time": { "type": "string" },
                    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
                    "schema_hint": { "type": "string" },
                    "public_rationale": { "type": "string" },
                    "evidence_text": { "type": "string" },
                    "evidence_locator": { "type": "string" },
                    "validate": { "type": "boolean" },
                    "quality_profile": { "type": "string", "enum": ["fast", "strict"] },
                    "quality_plane": { "type": "string", "enum": ["meta", "data", "both"] }
                }
            }),
        },
        ToolSpecV1 {
            name: "propose_relations_proposals".to_string(),
            description: "Generate an untrusted `proposals.json` (Evidence/Proposals schema) for adding *multiple* relation assertions between lists of entities.\n\nThis is the batch form of `propose_relation_proposals`. Use it when the user asks for multiple pairs (e.g. \"Jamison is a child of Alice and Bob\").".to_string(),
            args_schema: serde_json::json!({
                "type": "object",
                "required": ["rel_type", "source_names", "target_names"],
                "properties": {
                    "rel_type": { "type": "string" },
                    "source_names": { "type": "array", "items": { "type": "string" }, "minItems": 1, "maxItems": 50 },
                    "target_names": { "type": "array", "items": { "type": "string" }, "minItems": 1, "maxItems": 50 },
                    "pairing": { "type": "string", "enum": ["cartesian", "zip"] },
                    "source_type": { "type": "string" },
                    "target_type": { "type": "string" },
                    "source_field": { "type": "string" },
                    "target_field": { "type": "string" },
                    "context": { "type": "string" },
                    "time": { "type": "string" },
                    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
                    "schema_hint": { "type": "string" },
                    "public_rationale": { "type": "string" },
                    "evidence_text": { "type": "string" },
                    "evidence_locator": { "type": "string" },
                    "validate": { "type": "boolean" },
                    "quality_profile": { "type": "string", "enum": ["fast", "strict"] },
                    "quality_plane": { "type": "string", "enum": ["meta", "data", "both"] }
                }
            }),
        },
    ]
}

fn execute_tool_call(
    db: &PathDB,
    meta: Option<&MetaPlaneIndex>,
    default_contexts: &[crate::axql::AxqlContextSpec],
    snapshot_key: &str,
    embeddings: Option<&crate::embeddings::ResolvedEmbeddingsIndexV1>,
    ollama_embed_host: Option<&str>,
    query_cache: &mut crate::axql::AxqlPreparedQueryCache,
    call: &ToolCallV1,
    options: ToolLoopOptions,
) -> Result<serde_json::Value> {
    match call.name.as_str() {
        "db_summary" => tool_db_summary(db, &call.args),
        "semantic_search" => {
            tool_semantic_search(
                db,
                &call.args,
                snapshot_key,
                options,
                embeddings,
                ollama_embed_host,
            )
        }
        "lookup_entity" => tool_lookup_entity(db, &call.args),
        "describe_entity" => describe_entity_v1(db, &call.args),
        "lookup_type" => tool_lookup_type(db, meta, &call.args),
        "lookup_relation" => tool_lookup_relation(meta, &call.args),
        "fts_chunks" => tool_fts_chunks(db, &call.args, options),
        "docchunk_get" => tool_docchunk_get(db, &call.args, options),
        "axql_elaborate" => tool_axql_elaborate(
            db,
            meta,
            default_contexts,
            snapshot_key,
            query_cache,
            &call.args,
        ),
        "axql_run" => tool_axql_run(
            db,
            meta,
            default_contexts,
            snapshot_key,
            query_cache,
            &call.args,
            options,
        ),
        "viz_render" => tool_viz_render(db, meta, &call.args),
        "quality_report" => tool_quality_report(db, &call.args),
        "propose_axi_patch" => tool_propose_axi_patch(&call.args),
        "draft_axi_from_proposals" => tool_draft_axi_from_proposals(&call.args),
        "propose_relation_proposals" => tool_propose_relation_proposals(db, default_contexts, &call.args),
        "propose_relations_proposals" => tool_propose_relations_proposals(db, default_contexts, &call.args),
        other => Err(anyhow!("unknown tool `{other}`")),
    }
}

fn tool_lookup_entity(db: &PathDB, args: &serde_json::Value) -> Result<serde_json::Value> {
    #[derive(Deserialize)]
    struct Args {
        name: String,
        #[serde(default, rename = "type")]
        type_name: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
    }
    let a: Args = serde_json::from_value(args.clone())
        .map_err(|e| anyhow!("lookup_entity: invalid args: {e}"))?;

    let limit = a.limit.unwrap_or(10).clamp(1, 50);
    let name = a.name.trim();
    if name.is_empty() {
        return Err(anyhow!("lookup_entity: name must be non-empty"));
    }

    let Some(key_id) = db.interner.id_of("name") else {
        return Ok(serde_json::json!({ "matches": [], "note": "db has no `name` attribute" }));
    };
    let want_type = a.type_name.as_deref();

    // Fast path: exact match against interned value ids.
    if let Some(value_id) = db.interner.id_of(name) {
        let ids = db.entities.entities_with_attr_value(key_id, value_id);
        let mut matches = Vec::new();
        for id in ids.iter() {
            let Some(view) = db.get_entity(id) else {
                continue;
            };
            if let Some(want) = want_type {
                if view.entity_type != want {
                    // Also allow virtual types stored in the type index.
                    if !db
                        .find_by_type(want)
                        .map(|bm| bm.contains(id))
                        .unwrap_or(false)
                    {
                        continue;
                    }
                }
            }
            matches.push(EntityViewV1::from_id(db, id));
            if matches.len() >= limit {
                break;
            }
        }

        if !matches.is_empty() {
            return Ok(serde_json::json!({
                "matches": matches,
                "match_kind": "exact"
            }));
        }
    }

    // Robust fallback: token-based name resolution (case-insensitive) so
    // questions like "alice" can still resolve to "Alice".
    let mut candidates = db.entities_with_attr_fts("name", name);
    let mut match_kind = "fts";
    if candidates.is_empty() {
        candidates = db.entities_with_attr_fts_any("name", name);
        match_kind = "fts_any";
    }
    if candidates.is_empty() {
        candidates = db.entities_with_attr_fuzzy("name", name, 2);
        match_kind = "fuzzy";
    }
    if candidates.is_empty() {
        return Ok(serde_json::json!({
            "matches": [],
            "note": "no match (exact/token/fuzzy)",
        }));
    }

    let needle_lc = name.to_ascii_lowercase();
    let mut matches = Vec::new();
    let mut seen = BTreeSet::<u32>::new();

    // Pass 1: prefer case-insensitive exact matches among candidates.
    for id in candidates.iter() {
        if matches.len() >= limit {
            break;
        }
        let Some(view) = db.get_entity(id) else {
            continue;
        };
        let Some(entity_name) = view.attrs.get("name") else {
            continue;
        };
        if entity_name.to_ascii_lowercase() != needle_lc {
            continue;
        }
        if let Some(want) = want_type {
            if view.entity_type != want {
                if !db
                    .find_by_type(want)
                    .map(|bm| bm.contains(id))
                    .unwrap_or(false)
                {
                    continue;
                }
            }
        }
        if seen.insert(id) {
            matches.push(EntityViewV1::from_id(db, id));
        }
    }

    // Pass 2: fill remaining slots with other candidates.
    for id in candidates.iter() {
        if matches.len() >= limit {
            break;
        }
        if seen.contains(&id) {
            continue;
        }
        let Some(view) = db.get_entity(id) else {
            continue;
        };
        if let Some(want) = want_type {
            if view.entity_type != want {
                if !db
                    .find_by_type(want)
                    .map(|bm| bm.contains(id))
                    .unwrap_or(false)
                {
                    continue;
                }
            }
        }
        if seen.insert(id) {
            matches.push(EntityViewV1::from_id(db, id));
        }
    }

    Ok(serde_json::json!({
        "matches": matches,
        "match_kind": match_kind,
        "note": format!("no exact name match; returned {match_kind} candidate(s)")
    }))
}

pub(crate) fn describe_entity_v1(db: &PathDB, args: &serde_json::Value) -> Result<serde_json::Value> {
    #[derive(Deserialize)]
    struct Args {
        #[serde(default)]
        id: Option<u32>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default, rename = "type")]
        type_name: Option<String>,
        #[serde(default)]
        max_attrs: Option<usize>,
        #[serde(default)]
        max_rel_types: Option<usize>,
        #[serde(default)]
        out_limit: Option<usize>,
        #[serde(default)]
        in_limit: Option<usize>,
    }
    let a: Args =
        serde_json::from_value(args.clone()).map_err(|e| anyhow!("describe_entity: invalid args: {e}"))?;

    let entity_id = if let Some(id) = a.id {
        id
    } else if let Some(name) = a.name.as_deref() {
        let name = name.trim();
        if name.is_empty() {
            return Err(anyhow!("describe_entity: name must be non-empty"));
        }
        let Some(key_id) = db.interner.id_of("name") else {
            return Err(anyhow!("describe_entity: db has no `name` attribute"));
        };
        let Some(value_id) = db.interner.id_of(name) else {
            return Err(anyhow!("describe_entity: no entity named `{name}`"));
        };
        let ids = db.entities.entities_with_attr_value(key_id, value_id);
        if ids.is_empty() {
            return Err(anyhow!("describe_entity: no entity named `{name}`"));
        }
        let mut picked: Option<u32> = None;
        for id in ids.iter() {
            let Some(view) = db.get_entity(id) else {
                continue;
            };
            if let Some(want) = a.type_name.as_deref() {
                if view.entity_type != want
                    && !db
                        .find_by_type(want)
                        .map(|bm| bm.contains(id))
                        .unwrap_or(false)
                {
                    continue;
                }
            }
            picked = Some(id);
            break;
        }
        picked.ok_or_else(|| anyhow!("describe_entity: no match after type filter"))?
    } else {
        return Err(anyhow!("describe_entity: expected `id` or `name`"));
    };

    let Some(view) = db.get_entity(entity_id) else {
        return Err(anyhow!("describe_entity: no entity with id {entity_id}"));
    };

    let max_attrs = a.max_attrs.unwrap_or(40).min(200);
    let max_rel_types = a.max_rel_types.unwrap_or(12).clamp(1, 50);
    let out_limit = a.out_limit.unwrap_or(6).min(50);
    let in_limit = a.in_limit.unwrap_or(6).min(50);

    let mut attrs: BTreeMap<String, String> = BTreeMap::new();
    let mut keys: Vec<String> = view.attrs.keys().cloned().collect();
    keys.sort();
    for k in keys.into_iter().take(max_attrs) {
        if let Some(v) = view.attrs.get(&k) {
            attrs.insert(k, v.clone());
        }
    }

    let contexts = db
        .follow_one(entity_id, axiograph_pathdb::axi_meta::REL_AXI_FACT_IN_CONTEXT)
        .iter()
        .take(24)
        .map(|id| EntityViewV1::from_id(db, id))
        .collect::<Vec<_>>();

    let mut equivalences = Vec::new();
    if let Some(eqs) = db.equivalences.get(&entity_id) {
        for (other, ty_id) in eqs.iter().take(24) {
            let ty = db.interner.lookup(*ty_id).unwrap_or_else(|| "?".to_string());
            equivalences.push(serde_json::json!({
                "other": EntityViewV1::from_id(db, *other),
                "kind": ty
            }));
        }
    }

    fn group(
        db: &PathDB,
        rels: Vec<&axiograph_pathdb::Relation>,
        max_rel_types: usize,
        per_rel: usize,
        dir: &str,
    ) -> Vec<serde_json::Value> {
        let mut groups: std::collections::HashMap<String, Vec<(u32, f32)>> = std::collections::HashMap::new();
        for r in rels {
            let label = db.interner.lookup(r.rel_type).unwrap_or_else(|| "?".to_string());
            let endpoint = if dir == "out" { r.target } else { r.source };
            groups.entry(label).or_default().push((endpoint, r.confidence));
        }

        let mut keys: Vec<String> = groups.keys().cloned().collect();
        keys.sort_by_key(|k| std::cmp::Reverse(groups.get(k).map(|v| v.len()).unwrap_or(0)));

        let mut out = Vec::new();
        for k in keys.into_iter().take(max_rel_types) {
            let mut edges = groups.remove(&k).unwrap_or_default();
            edges.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            let sample = edges
                .iter()
                .take(per_rel)
                .map(|(id, conf)| {
                    serde_json::json!({
                        "entity": EntityViewV1::from_id(db, *id),
                        "confidence": conf
                    })
                })
                .collect::<Vec<_>>();
            out.push(serde_json::json!({
                "rel": k,
                "count": edges.len(),
                "edges": sample
            }));
        }
        out
    }

    let outgoing = group(
        db,
        db.relations.outgoing_any(entity_id),
        max_rel_types,
        out_limit,
        "out",
    );
    let incoming = group(
        db,
        db.relations.incoming_any(entity_id),
        max_rel_types,
        in_limit,
        "in",
    );

    // Convenience for UI highlighting: include the entity + all sampled neighbors.
    let mut highlight_ids: BTreeSet<u32> = BTreeSet::new();
    highlight_ids.insert(entity_id);
    for g in outgoing.iter().chain(incoming.iter()) {
        if let Some(edges) = g.get("edges").and_then(|v| v.as_array()) {
            for e in edges {
                if let Some(id) = e.pointer("/entity/id").and_then(|v| v.as_u64()) {
                    highlight_ids.insert(id as u32);
                }
            }
        }
    }

    Ok(serde_json::json!({
        "entity": {
            "id": entity_id,
            "entity_type": view.entity_type,
            "name": view.attrs.get("name").cloned(),
        },
        "attrs": attrs,
        "contexts": contexts,
        "equivalences": equivalences,
        "outgoing": outgoing,
        "incoming": incoming,
        "highlight_ids": highlight_ids.into_iter().collect::<Vec<_>>(),
    }))
}

fn tool_lookup_type(
    db: &PathDB,
    meta: Option<&MetaPlaneIndex>,
    args: &serde_json::Value,
) -> Result<serde_json::Value> {
    #[derive(Deserialize)]
    struct Args {
        #[serde(rename = "type")]
        type_name: String,
    }
    let a: Args =
        serde_json::from_value(args.clone()).map_err(|e| anyhow!("lookup_type: invalid args: {e}"))?;

    let type_name = a.type_name.trim();
    if type_name.is_empty() {
        return Err(anyhow!("lookup_type: type must be non-empty"));
    }

    let mut schemas = Vec::new();
    if let Some(meta) = meta {
        for (schema_name, s) in &meta.schemas {
            if !s.object_types.contains(type_name) {
                continue;
            }
            let mut sups: Vec<String> = s
                .supertypes_of
                .get(type_name)
                .map(|hs| hs.iter().cloned().collect())
                .unwrap_or_else(Vec::new);
            sups.sort();

            let mut subs: Vec<String> = s
                .subtype_decls
                .iter()
                .filter(|d| d.sup == type_name)
                .map(|d| d.sub.clone())
                .collect();
            subs.sort();
            subs.dedup();

            let mut used_in_relations: Vec<String> = Vec::new();
            for (rname, r) in &s.relation_decls {
                if r.fields.iter().any(|f| f.field_type == type_name) {
                    used_in_relations.push(rname.clone());
                }
            }
            used_in_relations.sort();

            schemas.push(serde_json::json!({
                "schema": schema_name,
                "module": s.module_name,
                "supertypes": sups,
                "subtypes": subs,
                "used_in_relations": used_in_relations
            }));
        }
    }

    let count = db.find_by_type(type_name).map(|bm| bm.len()).unwrap_or(0);
    Ok(serde_json::json!({
        "type": type_name,
        "entity_count": count,
        "schemas": schemas
    }))
}

fn tool_lookup_relation(meta: Option<&MetaPlaneIndex>, args: &serde_json::Value) -> Result<serde_json::Value> {
    #[derive(Deserialize)]
    struct Args {
        relation: String,
        #[serde(default)]
        schema: Option<String>,
    }
    let a: Args =
        serde_json::from_value(args.clone()).map_err(|e| anyhow!("lookup_relation: invalid args: {e}"))?;

    let rel = a.relation.trim();
    if rel.is_empty() {
        return Err(anyhow!("lookup_relation: relation must be non-empty"));
    }
    let schema_hint = a.schema.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty());

    let Some(meta) = meta else {
        return Err(anyhow!(
            "lookup_relation: meta-plane not available in this snapshot (no canonical `.axi` schema imported)"
        ));
    };

    fn infer_endpoint_fields_from_decl(
        rel_decl: &axiograph_pathdb::axi_semantics::RelationDecl,
    ) -> (String, String) {
        let names: Vec<&str> = rel_decl.fields.iter().map(|f| f.field_name.as_str()).collect();
        if names.contains(&"from") && names.contains(&"to") {
            return ("from".to_string(), "to".to_string());
        }
        if names.contains(&"source") && names.contains(&"target") {
            return ("source".to_string(), "target".to_string());
        }
        if names.contains(&"lhs") && names.contains(&"rhs") {
            return ("lhs".to_string(), "rhs".to_string());
        }
        if names.contains(&"child") && names.contains(&"parent") {
            return ("child".to_string(), "parent".to_string());
        }
        if rel_decl.fields.len() >= 2 {
            return (
                rel_decl.fields[0].field_name.clone(),
                rel_decl.fields[1].field_name.clone(),
            );
        }
        ("from".to_string(), "to".to_string())
    }

    if let Some(resolved) = crate::relation_resolution::resolve_schema_relation(meta, schema_hint, rel) {
        let mut fields = resolved.rel_decl.fields.clone();
        fields.sort_by_key(|f| f.field_index);
        let (src_field, dst_field) = infer_endpoint_fields_from_decl(resolved.rel_decl);

        let constraints = resolved
            .schema
            .constraints_by_relation
            .get(&resolved.rel_name)
            .cloned()
            .unwrap_or_default();

        let constraints_rendered: Vec<String> = constraints
            .iter()
            .map(|c| match c {
                axiograph_pathdb::axi_semantics::ConstraintDecl::Functional {
                    src_field, dst_field, ..
                } => format!("functional({src_field} -> {dst_field})"),
                axiograph_pathdb::axi_semantics::ConstraintDecl::Symmetric { .. } => "symmetric".to_string(),
                axiograph_pathdb::axi_semantics::ConstraintDecl::Transitive { .. } => "transitive".to_string(),
                axiograph_pathdb::axi_semantics::ConstraintDecl::Key { fields, .. } => {
                    format!("key({})", fields.join(", "))
                }
                axiograph_pathdb::axi_semantics::ConstraintDecl::Unknown { text, .. } => {
                    format!("unknown({text})")
                }
            })
            .collect();

        return Ok(serde_json::json!({
            "match_kind": "resolved",
            "schema": resolved.schema_name,
            "relation": resolved.rel_name,
            "alias_used": resolved.alias_used,
            "orientation": format!("{:?}", resolved.orientation),
            "fields": fields.iter().map(|f| serde_json::json!({
                "name": f.field_name.clone(),
                "type": f.field_type.clone(),
                "index": f.field_index,
            })).collect::<Vec<_>>(),
            "default_mapping": {
                "source_field": src_field,
                "target_field": dst_field,
            },
            "constraints": constraints_rendered,
        }));
    }

    // No unambiguous resolution. Provide candidates to help the model ask a clarifying question.
    let needle = rel.to_ascii_lowercase();
    let mut matches: Vec<serde_json::Value> = Vec::new();
    for (schema_name, schema) in &meta.schemas {
        for (name, decl) in &schema.relation_decls {
            if name.to_ascii_lowercase() != needle && decl.name.to_ascii_lowercase() != needle {
                continue;
            }
            matches.push(serde_json::json!({
                "schema": schema_name,
                "relation": decl.name,
            }));
        }
    }
    matches.sort_by(|a, b| {
        let aschema = a.get("schema").and_then(|v| v.as_str()).unwrap_or("");
        let bschema = b.get("schema").and_then(|v| v.as_str()).unwrap_or("");
        aschema.cmp(bschema)
            .then_with(|| a.get("relation").and_then(|v| v.as_str()).unwrap_or("").cmp(
                b.get("relation").and_then(|v| v.as_str()).unwrap_or("")
            ))
    });

    Ok(serde_json::json!({
        "match_kind": if matches.is_empty() { "none" } else { "ambiguous" },
        "relation_input": rel,
        "schema_hint": schema_hint,
        "matches": matches,
        "note": if matches.is_empty() { "no relation found" } else { "ambiguous; pick a schema or use a more specific relation name" }
    }))
}

fn tool_db_summary(db: &PathDB, args: &serde_json::Value) -> Result<serde_json::Value> {
    #[derive(Deserialize)]
    struct Args {
        #[serde(default)]
        max_types: Option<usize>,
        #[serde(default)]
        max_relations: Option<usize>,
        #[serde(default)]
        max_relation_samples: Option<usize>,
    }
    let a: Args =
        serde_json::from_value(args.clone()).map_err(|e| anyhow!("db_summary: invalid args: {e}"))?;

    let max_types = a.max_types.unwrap_or(12).clamp(1, 50);
    let max_relations = a.max_relations.unwrap_or(12).clamp(1, 50);
    let max_relation_samples = a.max_relation_samples.unwrap_or(4).clamp(0, 10);

    let name_key_id = db.interner.id_of("name");
    let context_type_id = db.interner.id_of("Context");
    let doc_chunk_type_id = db.interner.id_of("DocChunk");

    let mut type_counts: std::collections::HashMap<axiograph_pathdb::StrId, usize> =
        std::collections::HashMap::new();
    let mut type_samples: std::collections::HashMap<axiograph_pathdb::StrId, Vec<String>> =
        std::collections::HashMap::new();
    let mut contexts: Vec<EntityViewV1> = Vec::new();

    for entity_id in 0..db.entities.len() as u32 {
        let Some(type_id) = db.entities.get_type(entity_id) else {
            continue;
        };
        *type_counts.entry(type_id).or_insert(0) += 1;

        // Sample a few identifiers per type.
        if let Some(list) = type_samples.get_mut(&type_id) {
            if list.len() < 6 {
                if let Some(name_key_id) = name_key_id {
                    if let Some(value_id) = db.entities.get_attr(entity_id, name_key_id) {
                        if let Some(name) = db.interner.lookup(value_id) {
                            list.push(name);
                        }
                    }
                }
                if list.len() < 6 {
                    list.push(format!("#{entity_id}"));
                }
            }
        } else {
            let mut list = Vec::new();
            if let Some(name_key_id) = name_key_id {
                if let Some(value_id) = db.entities.get_attr(entity_id, name_key_id) {
                    if let Some(name) = db.interner.lookup(value_id) {
                        list.push(name);
                    }
                }
            }
            if list.is_empty() {
                list.push(format!("#{entity_id}"));
            }
            type_samples.insert(type_id, list);
        }

        // Sample a few context/world nodes for UI scoping hints.
        if contexts.len() < 12 {
            if context_type_id.is_some_and(|tid| tid == type_id) {
                contexts.push(EntityViewV1::from_id(db, entity_id));
            }
        }
    }

    let doc_chunks_loaded = doc_chunk_type_id
        .and_then(|tid| type_counts.get(&tid).copied())
        .unwrap_or(0)
        > 0;

    let mut types_ranked: Vec<(usize, String, Vec<String>)> = Vec::new();
    for (tid, count) in &type_counts {
        let Some(type_name) = db.interner.lookup(*tid) else {
            continue;
        };
        if type_name.starts_with("AxiMeta") {
            continue;
        }
        let samples = type_samples.get(tid).cloned().unwrap_or_default();
        types_ranked.push((*count, type_name, samples));
    }
    types_ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let types_out = types_ranked
        .into_iter()
        .take(max_types)
        .map(|(count, type_name, samples)| {
            serde_json::json!({
                "type": type_name,
                "count": count,
                "sample": samples
            })
        })
        .collect::<Vec<_>>();

    let mut rel_counts: std::collections::HashMap<axiograph_pathdb::StrId, usize> =
        std::collections::HashMap::new();
    let mut rel_samples: std::collections::HashMap<axiograph_pathdb::StrId, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    for rel_id in 0..db.relations.len() as u32 {
        let Some(rel) = db.relations.get_relation(rel_id) else {
            continue;
        };
        *rel_counts.entry(rel.rel_type).or_insert(0) += 1;
        if max_relation_samples == 0 {
            continue;
        }
        let entry = rel_samples.entry(rel.rel_type).or_insert_with(Vec::new);
        if entry.len() >= max_relation_samples {
            continue;
        }
        entry.push(serde_json::json!({
            "source": EntityViewV1::from_id(db, rel.source),
            "target": EntityViewV1::from_id(db, rel.target),
            "confidence": rel.confidence
        }));
    }

    let mut rel_ranked: Vec<(usize, String, Vec<serde_json::Value>)> = Vec::new();
    for (rid, count) in &rel_counts {
        let Some(rel_name) = db.interner.lookup(*rid) else {
            continue;
        };
        let samples = rel_samples.get(rid).cloned().unwrap_or_default();
        rel_ranked.push((*count, rel_name, samples));
    }
    rel_ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let rels_out = rel_ranked
        .into_iter()
        .take(max_relations)
        .map(|(count, rel_name, samples)| {
            serde_json::json!({
                "rel": rel_name,
                "count": count,
                "sample": samples
            })
        })
        .collect::<Vec<_>>();

    Ok(serde_json::json!({
        "entities": db.entities.len(),
        "relations": db.relations.len(),
        "doc_chunks_loaded": doc_chunks_loaded,
        "contexts": contexts,
        "types": types_out,
        "relations_by_type": rels_out
    }))
}

// ============================================================================
// Deterministic retrieval (token-hash embeddings + HNSW ANN index)
// ============================================================================

const TOKEN_HASH_DIM: usize = 128;

fn token_hash_fnv1a64(s: &str) -> u64 {
    let mut h: u64 = 14695981039346656037;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}

fn token_hash_embed_text(text: &str) -> [f32; TOKEN_HASH_DIM] {
    let tokens = axiograph_pathdb::tokenize_fts_query(text);
    let mut v = [0.0f32; TOKEN_HASH_DIM];
    for t in tokens {
        let h = token_hash_fnv1a64(&t);
        let idx = (h % (TOKEN_HASH_DIM as u64)) as usize;
        let sign = if ((h >> 32) & 1) == 0 { 1.0 } else { -1.0 };
        v[idx] += sign;
    }
    // Normalize.
    let mut norm2 = 0.0f32;
    for x in v {
        norm2 += x * x;
    }
    if norm2 > 0.0 {
        let inv = 1.0f32 / norm2.sqrt();
        for x in v.iter_mut() {
            *x *= inv;
        }
    }
    v
}

fn token_hash_dot(a: &[f32; TOKEN_HASH_DIM], b: &[f32; TOKEN_HASH_DIM]) -> f32 {
    let mut s = 0.0f32;
    for i in 0..TOKEN_HASH_DIM {
        s += a[i] * b[i];
    }
    s
}

struct TokenHashAnnSubIndex {
    // Snapshot-local ids (PathDB entity ids).
    ids: Vec<u32>,
    // Embedding vectors aligned with `ids`.
    vectors: Vec<[f32; TOKEN_HASH_DIM]>,
    // ANN structure (search-only after build).
    hnsw: hnsw_rs::prelude::Hnsw<'static, f32, hnsw_rs::prelude::DistL2>,
}

struct TokenHashAnnIndex {
    entities: TokenHashAnnSubIndex,
    docchunks: Option<TokenHashAnnSubIndex>,
}

static TOKEN_HASH_ANN_CACHE: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<std::sync::Mutex<TokenHashAnnIndex>>>>,
> = std::sync::OnceLock::new();

fn token_hash_ann_cache() -> &'static std::sync::Mutex<
    std::collections::HashMap<String, std::sync::Arc<std::sync::Mutex<TokenHashAnnIndex>>>,
> {
    TOKEN_HASH_ANN_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

fn build_entity_graph_text_for_token_hash(db: &PathDB, id: u32) -> Option<String> {
    let view = db.get_entity(id)?;
    if view.entity_type.starts_with("AxiMeta") || view.entity_type == "DocChunk" || view.entity_type == "Document"
    {
        return None;
    }

    let mut text = String::new();
    text.push_str(&view.entity_type);
    if let Some(name) = view.attrs.get("name") {
        text.push(' ');
        text.push_str(name);
    }
    for k in ["search_text", "description", "comment", "iri"] {
        if let Some(v) = view.attrs.get(k) {
            if !v.trim().is_empty() {
                text.push(' ');
                text.push_str(v);
            }
        }
    }

    let mut rels: BTreeSet<String> = BTreeSet::new();
    for rel in db.relations.outgoing_any(id).iter().take(24) {
        if let Some(name) = db.interner.lookup(rel.rel_type) {
            rels.insert(name);
        }
    }
    for rel in db.relations.incoming_any(id).iter().take(24) {
        if let Some(name) = db.interner.lookup(rel.rel_type) {
            rels.insert(name);
        }
    }
    for r in rels.into_iter().take(24) {
        text.push(' ');
        text.push_str(&r);
    }

    Some(text)
}

fn build_docchunk_text_for_token_hash(db: &PathDB, id: u32) -> Option<String> {
    let text = db_entity_attr_string(db, id, "text")?;
    let search_text = db_entity_attr_string(db, id, "search_text").unwrap_or_default();
    let mut combined = String::new();
    combined.push_str(&text);
    if !search_text.trim().is_empty() {
        combined.push('\n');
        combined.push_str(&search_text);
    }
    Some(combined)
}

fn build_hnsw_index(
    ids: Vec<u32>,
    vectors: Vec<[f32; TOKEN_HASH_DIM]>,
) -> Result<TokenHashAnnSubIndex> {
    if ids.is_empty() {
        return Err(anyhow!("no points to index"));
    }
    // HNSW params (conservative defaults):
    // - `m`: max connections per layer
    // - `ef_construction`: construction search width
    let m: usize = 16;
    let ef_construction: usize = 200;

    let nb_elem = ids.len();
    let max_layer = 16.min((nb_elem as f32).ln().trunc() as usize).max(1);

    let hnsw = hnsw_rs::prelude::Hnsw::<f32, hnsw_rs::prelude::DistL2>::new(
        m,
        nb_elem,
        max_layer,
        ef_construction,
        hnsw_rs::prelude::DistL2 {},
    );

    for (i, v) in vectors.iter().enumerate() {
        hnsw.insert((&v[..], i));
    }

    Ok(TokenHashAnnSubIndex { ids, vectors, hnsw })
}

fn get_or_build_token_hash_ann_index(
    snapshot_key: &str,
    db: &PathDB,
) -> Result<std::sync::Arc<std::sync::Mutex<TokenHashAnnIndex>>> {
    // Fast path: cached.
    if let Ok(cache) = token_hash_ann_cache().lock() {
        if let Some(v) = cache.get(snapshot_key).cloned() {
            return Ok(v);
        }
    }

    // Build a fresh index (outside the cache lock).
    let mut entity_ids = Vec::new();
    let mut entity_vecs = Vec::new();
    for id in 0..(db.entities.len() as u32) {
        let Some(text) = build_entity_graph_text_for_token_hash(db, id) else {
            continue;
        };
        entity_ids.push(id);
        entity_vecs.push(token_hash_embed_text(&text));
    }

    let entities = build_hnsw_index(entity_ids, entity_vecs)?;

    let docchunks = if let Some(chunks) = db.find_by_type("DocChunk") {
        let mut ids = Vec::new();
        let mut vecs = Vec::new();
        for id in chunks.iter() {
            let Some(text) = build_docchunk_text_for_token_hash(db, id) else {
                continue;
            };
            ids.push(id);
            vecs.push(token_hash_embed_text(&text));
        }
        Some(build_hnsw_index(ids, vecs)?)
    } else {
        None
    };

    let built = std::sync::Arc::new(std::sync::Mutex::new(TokenHashAnnIndex { entities, docchunks }));

    // Store in cache (best-effort). Keep the cache bounded to avoid unbounded memory growth.
    if let Ok(mut cache) = token_hash_ann_cache().lock() {
        cache.insert(snapshot_key.to_string(), built.clone());
        const MAX_ENTRIES: usize = 4;
        if cache.len() > MAX_ENTRIES {
            // Simple eviction: drop everything except the current key.
            cache.retain(|k, _| k == snapshot_key);
        }
    }

    Ok(built)
}

fn tool_semantic_search(
    db: &PathDB,
    args: &serde_json::Value,
    snapshot_key: &str,
    options: ToolLoopOptions,
    embeddings: Option<&crate::embeddings::ResolvedEmbeddingsIndexV1>,
    ollama_embed_host: Option<&str>,
) -> Result<serde_json::Value> {
    #[derive(Deserialize)]
    struct Args {
        query: String,
        #[serde(default)]
        entity_limit: Option<usize>,
        #[serde(default)]
        chunk_limit: Option<usize>,
    }
    let a: Args = serde_json::from_value(args.clone())
        .map_err(|e| anyhow!("semantic_search: invalid args: {e}"))?;

    let query = a.query.trim();
    if query.is_empty() {
        return Err(anyhow!("semantic_search: query must be non-empty"));
    }

    let entity_limit = a.entity_limit.unwrap_or(12).clamp(1, 50);
    let chunk_limit = a.chunk_limit.unwrap_or(options.max_doc_chunks).clamp(1, 50);

    // Deterministic token-hash retrieval (always-on).
    let qv = token_hash_embed_text(query);

    let mut det_entity_scores: Vec<(f32, u32)> = Vec::new();
    let mut det_chunk_scores: Vec<(f32, u32)> = Vec::new();

    match get_or_build_token_hash_ann_index(snapshot_key, db) {
        Ok(ann) => {
            let ann = ann
                .lock()
                .map_err(|_| anyhow!("semantic_search: ann index lock poisoned"))?;

            // Entities.
            let k = (entity_limit.saturating_mul(4)).clamp(1, 200);
            let ef_search = 64;
            let q = qv.to_vec();
            let neigh = ann.entities.hnsw.search(&q, k, ef_search);
            for n in neigh {
                let idx = n.d_id;
                if idx >= ann.entities.ids.len() {
                    continue;
                }
                let id = ann.entities.ids[idx];
                let sim = token_hash_dot(&qv, &ann.entities.vectors[idx]);
                det_entity_scores.push((sim, id));
            }
            det_entity_scores.sort_by(|(sa, ia), (sb, ib)| sb.total_cmp(sa).then_with(|| ia.cmp(ib)));
            det_entity_scores.truncate(entity_limit);

            // DocChunks.
            if let Some(chunks) = ann.docchunks.as_ref() {
                let k = (chunk_limit.saturating_mul(4)).clamp(1, 200);
                let ef_search = 64;
                let q = qv.to_vec();
                let neigh = chunks.hnsw.search(&q, k, ef_search);
                for n in neigh {
                    let idx = n.d_id;
                    if idx >= chunks.ids.len() {
                        continue;
                    }
                    let id = chunks.ids[idx];
                    let sim = token_hash_dot(&qv, &chunks.vectors[idx]);
                    det_chunk_scores.push((sim, id));
                }
                det_chunk_scores.sort_by(|(sa, ia), (sb, ib)| {
                    sb.total_cmp(sa).then_with(|| ia.cmp(ib))
                });
                det_chunk_scores.truncate(chunk_limit);
            }
        }
        Err(e) => {
            // Fallback: exact scan over token-candidates (slower, but avoids hard-failures).
            let _ = e;

            // Entities fallback (token index candidate set).
            let mut entity_candidates = RoaringBitmap::new();
            for key in ["name", "search_text", "description", "comment", "iri"] {
                entity_candidates |= db.entities_with_attr_fts_any(key, query);
            }
            for id in entity_candidates.iter() {
                let Some(text) = build_entity_graph_text_for_token_hash(db, id) else {
                    continue;
                };
                let ev = token_hash_embed_text(&text);
                det_entity_scores.push((token_hash_dot(&qv, &ev), id));
            }
            det_entity_scores.sort_by(|(sa, ia), (sb, ib)| sb.total_cmp(sa).then_with(|| ia.cmp(ib)));
            det_entity_scores.truncate(entity_limit);

            // DocChunks fallback (token index candidate set).
            if let Some(chunks) = db.find_by_type("DocChunk") {
                let mut candidates = db.entities_with_attr_fts_any("text", query)
                    | db.entities_with_attr_fts_any("search_text", query);
                candidates &= chunks.clone();
                for id in candidates.iter() {
                    let Some(text) = build_docchunk_text_for_token_hash(db, id) else {
                        continue;
                    };
                    let ev = token_hash_embed_text(&text);
                    det_chunk_scores.push((token_hash_dot(&qv, &ev), id));
                }
                det_chunk_scores.sort_by(|(sa, ia), (sb, ib)| {
                    sb.total_cmp(sa).then_with(|| ia.cmp(ib))
                });
                det_chunk_scores.truncate(chunk_limit);
            }
        }
    }

    // Optional: Ollama embedding retrieval (requires snapshot-scoped embeddings + an Ollama host).
    let mut embed_entity_scores: Vec<(f32, u32)> = Vec::new();
    let mut embed_chunk_scores: Vec<(f32, u32)> = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    if let (Some(idx), Some(host)) = (embeddings, ollama_embed_host) {
        #[cfg(feature = "llm-ollama")]
        {
            fn normalize_vec(v: &mut [f32]) {
                let mut norm2 = 0.0f32;
                for x in v.iter() {
                    norm2 += x * x;
                }
                if norm2 <= 0.0 {
                    return;
                }
                let inv = 1.0f32 / norm2.sqrt();
                for x in v.iter_mut() {
                    *x *= inv;
                }
            }

            fn dot_vec(a: &[f32], b: &[f32]) -> f32 {
                let mut s = 0.0f32;
                let n = a.len().min(b.len());
                for i in 0..n {
                    s += a[i] * b[i];
                }
                s
            }

            let timeout = llm_timeout(None)?;

            // Entities.
            if let Some(t) = idx.entities.as_ref() {
                if t.backend == "ollama" {
                    let q = vec![query.to_string()];
                    match ollama_embed_texts_with_timeout(host, &t.model, &q, timeout) {
                        Ok(mut qv) if qv.len() == 1 => {
                            let mut qv = qv.remove(0);
                            normalize_vec(&mut qv);
                            if qv.len() == t.dim {
                                for row in &t.rows {
                                    embed_entity_scores.push((dot_vec(&qv, &row.vector), row.id));
                                }
                                embed_entity_scores.sort_by(|(sa, ia), (sb, ib)| {
                                    sb.total_cmp(sa).then_with(|| ia.cmp(ib))
                                });
                                embed_entity_scores.truncate(entity_limit);
                                notes.push(format!(
                                    "ollama_embeddings: entities={} model={}",
                                    t.rows.len(),
                                    t.model
                                ));
                            } else {
                                notes.push(format!(
                                    "ollama_embeddings skipped: query dim {} != stored dim {} (entities)",
                                    qv.len(),
                                    t.dim
                                ));
                            }
                        }
                        Ok(_) => notes.push("ollama_embeddings skipped: unexpected embeddings response shape".to_string()),
                        Err(e) => notes.push(format!("ollama_embeddings skipped: {e}")),
                    }
                } else {
                    notes.push(format!(
                        "ollama_embeddings skipped: backend {} (entities)",
                        t.backend
                    ));
                }
            }

            // DocChunks.
            if let Some(t) = idx.docchunks.as_ref() {
                if t.backend == "ollama" {
                    let q = vec![query.to_string()];
                    match ollama_embed_texts_with_timeout(host, &t.model, &q, timeout) {
                        Ok(mut qv) if qv.len() == 1 => {
                            let mut qv = qv.remove(0);
                            normalize_vec(&mut qv);
                            if qv.len() == t.dim {
                                for row in &t.rows {
                                    embed_chunk_scores.push((dot_vec(&qv, &row.vector), row.id));
                                }
                                embed_chunk_scores.sort_by(|(sa, ia), (sb, ib)| {
                                    sb.total_cmp(sa).then_with(|| ia.cmp(ib))
                                });
                                embed_chunk_scores.truncate(chunk_limit);
                                notes.push(format!(
                                    "ollama_embeddings: docchunks={} model={}",
                                    t.rows.len(),
                                    t.model
                                ));
                            } else {
                                notes.push(format!(
                                    "ollama_embeddings skipped: query dim {} != stored dim {} (docchunks)",
                                    qv.len(),
                                    t.dim
                                ));
                            }
                        }
                        Ok(_) => notes.push("ollama_embeddings skipped: unexpected embeddings response shape".to_string()),
                        Err(e) => notes.push(format!("ollama_embeddings skipped: {e}")),
                    }
                } else {
                    notes.push(format!(
                        "ollama_embeddings skipped: backend {} (docchunks)",
                        t.backend
                    ));
                }
            }
        }
        #[cfg(not(feature = "llm-ollama"))]
        {
            let _ = (idx, host);
            notes.push("ollama_embeddings unavailable (compiled without `llm-ollama`)".to_string());
        }
    } else if embeddings.is_some() && ollama_embed_host.is_none() {
        notes.push("ollama_embeddings unavailable (no ollama host configured for this tool-loop)".to_string());
    }

    // Merge entity hits (token-hash + optional ollama embeddings) by taking the best similarity per id.
    let mut entity_scores: std::collections::HashMap<u32, (Option<f32>, Option<f32>)> =
        std::collections::HashMap::new();
    for (sim, id) in det_entity_scores {
        entity_scores.insert(id, (Some(sim), None));
    }
    for (sim, id) in embed_entity_scores {
        entity_scores
            .entry(id)
            .and_modify(|e| e.1 = Some(sim))
            .or_insert((None, Some(sim)));
    }

    let mut entity_ranked: Vec<(f32, u32, Option<f32>, Option<f32>)> = Vec::new();
    for (id, (tok, emb)) in entity_scores {
        let combined = match (tok, emb) {
            (Some(a), Some(b)) => a.max(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => continue,
        };
        entity_ranked.push((combined, id, tok, emb));
    }
    entity_ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    let entity_hits = entity_ranked
        .into_iter()
        .take(entity_limit)
        .map(|(sim, id, tok, emb)| {
            serde_json::json!({
                "entity": EntityViewV1::from_id(db, id),
                "similarity": sim,
                "similarity_token_hash": tok,
                "similarity_ollama": emb
            })
        })
        .collect::<Vec<_>>();

    // DocChunk token-hash scores come from the deterministic ANN retrieval (and fallback scan).

    // Merge chunk hits (token-hash + optional ollama embeddings) by taking the best similarity per id.
    let mut chunk_scores: std::collections::HashMap<u32, (Option<f32>, Option<f32>)> =
        std::collections::HashMap::new();
    for (sim, id) in det_chunk_scores {
        chunk_scores.insert(id, (Some(sim), None));
    }
    for (sim, id) in embed_chunk_scores {
        chunk_scores
            .entry(id)
            .and_modify(|e| e.1 = Some(sim))
            .or_insert((None, Some(sim)));
    }

    let mut chunk_ranked: Vec<(f32, u32, Option<f32>, Option<f32>)> = Vec::new();
    for (id, (tok, emb)) in chunk_scores {
        let combined = match (tok, emb) {
            (Some(a), Some(b)) => a.max(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => continue,
        };
        chunk_ranked.push((combined, id, tok, emb));
    }
    chunk_ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    let mut chunk_hits: Vec<serde_json::Value> = Vec::new();
    for (sim, id, tok, emb) in chunk_ranked.into_iter().take(chunk_limit) {
        let chunk_id =
            db_entity_attr_string(db, id, "chunk_id").unwrap_or_else(|| id.to_string());
        let doc = db_entity_attr_string(db, id, "document_id").unwrap_or_default();
        let span = db_entity_attr_string(db, id, "span_id").unwrap_or_default();
        let text = db_entity_attr_string(db, id, "text").unwrap_or_default();
        let snippet = truncate_preview(&text, options.max_doc_chars);
        chunk_hits.push(serde_json::json!({
            "id": id,
            "chunk_id": chunk_id,
            "document_id": doc,
            "span_id": span,
            "snippet": snippet,
            "similarity": sim,
            "similarity_token_hash": tok,
            "similarity_ollama": emb
        }));
    }

    Ok(serde_json::json!({
        "query": query,
        "entity_hits": entity_hits,
        "chunk_hits": chunk_hits,
        "notes": notes,
        "note": "semantic_search is an extension-layer heuristic (token-hash + optional ollama embeddings); validate answers via axql_run / describe_entity"
    }))
}

fn tool_fts_chunks(
    db: &PathDB,
    args: &serde_json::Value,
    options: ToolLoopOptions,
) -> Result<serde_json::Value> {
    #[derive(Deserialize)]
    struct Args {
        query: String,
        #[serde(default)]
        limit: Option<usize>,
    }
    let a: Args =
        serde_json::from_value(args.clone()).map_err(|e| anyhow!("fts_chunks: invalid args: {e}"))?;

    let limit = a.limit.unwrap_or(options.max_doc_chunks).clamp(1, 50);
    let query = a.query.trim();
    if query.is_empty() {
        return Err(anyhow!("fts_chunks: query must be non-empty"));
    }

    let Some(chunks) = db.find_by_type("DocChunk") else {
        return Ok(serde_json::json!({ "hits": [], "note": "no DocChunk loaded (answer from the graph via db_summary / describe_entity / axql_run)" }));
    };

    let mut candidates =
        db.entities_with_attr_fts_any("text", query) | db.entities_with_attr_fts_any("search_text", query);
    candidates &= chunks.clone();

    let mut out = Vec::new();
    for id in candidates.iter().take(limit) {
        let chunk_id = db_entity_attr_string(db, id, "chunk_id").unwrap_or_else(|| id.to_string());
        let doc = db_entity_attr_string(db, id, "document_id").unwrap_or_default();
        let span = db_entity_attr_string(db, id, "span_id").unwrap_or_default();
        let text = db_entity_attr_string(db, id, "text").unwrap_or_default();
        let snippet = truncate_preview(&text, options.max_doc_chars);
        out.push(serde_json::json!({
            "id": id,
            "chunk_id": chunk_id,
            "document_id": doc,
            "span_id": span,
            "snippet": snippet
        }));
    }

    Ok(serde_json::json!({ "hits": out }))
}

fn tool_docchunk_get(
    db: &PathDB,
    args: &serde_json::Value,
    options: ToolLoopOptions,
) -> Result<serde_json::Value> {
    #[derive(Deserialize)]
    struct Args {
        #[serde(default)]
        id: Option<u32>,
        #[serde(default)]
        chunk_id: Option<String>,
        #[serde(default)]
        max_chars: Option<usize>,
    }
    let a: Args = serde_json::from_value(args.clone())
        .map_err(|e| anyhow!("docchunk_get: invalid args: {e}"))?;

    let max_chars = a.max_chars.unwrap_or(2_000).clamp(32, 8_000);

    let resolve_by_id = |id: u32| -> Result<u32> {
        let Some(view) = db.get_entity(id) else {
            return Err(anyhow!("docchunk_get: no entity with id={id}"));
        };
        if view.entity_type != "DocChunk"
            && !db
                .find_by_type("DocChunk")
                .map(|bm| bm.contains(id))
                .unwrap_or(false)
        {
            return Err(anyhow!(
                "docchunk_get: entity id={id} is not a DocChunk (type={})",
                view.entity_type
            ));
        }
        Ok(id)
    };

    let chunk_entity_id = if let Some(id) = a.id {
        resolve_by_id(id)?
    } else if let Some(chunk_id) = a.chunk_id.as_deref() {
        let chunk_id = chunk_id.trim();
        if chunk_id.is_empty() {
            return Err(anyhow!("docchunk_get: chunk_id must be non-empty"));
        }
        let Some(key_id) = db.interner.id_of("chunk_id") else {
            return Err(anyhow!("docchunk_get: db has no `chunk_id` attribute"));
        };
        let Some(value_id) = db.interner.id_of(chunk_id) else {
            return Err(anyhow!("docchunk_get: no DocChunk with chunk_id={chunk_id:?}"));
        };

        let mut ids = db.entities.entities_with_attr_value(key_id, value_id);
        if let Some(bm) = db.find_by_type("DocChunk") {
            ids &= bm.clone();
        }
        if ids.is_empty() {
            return Err(anyhow!("docchunk_get: no DocChunk with chunk_id={chunk_id:?}"));
        }
        ids.iter().next().unwrap_or(0)
    } else {
        return Err(anyhow!("docchunk_get: expected `id` or `chunk_id`"));
    };

    let chunk_id = db_entity_attr_string(db, chunk_entity_id, "chunk_id")
        .unwrap_or_else(|| chunk_entity_id.to_string());
    let document_id = db_entity_attr_string(db, chunk_entity_id, "document_id").unwrap_or_default();
    let span_id = db_entity_attr_string(db, chunk_entity_id, "span_id").unwrap_or_default();
    let meta_kind = db_entity_attr_string(db, chunk_entity_id, "meta_kind").unwrap_or_default();
    let meta_fqn = db_entity_attr_string(db, chunk_entity_id, "meta_fqn").unwrap_or_default();
    let source_path = db_entity_attr_string(db, chunk_entity_id, "source_path").unwrap_or_default();

    let full_text = db_entity_attr_string(db, chunk_entity_id, "text").unwrap_or_default();
    let text_len = full_text.chars().count();
    let text = truncate_preview(&full_text, max_chars);
    let text_truncated = text.chars().count() < text_len;

    Ok(serde_json::json!({
        "id": chunk_entity_id,
        "chunk_id": chunk_id,
        "document_id": document_id,
        "span_id": span_id,
        "meta_kind": meta_kind,
        "meta_fqn": meta_fqn,
        "source_path": source_path,
        "text": text,
        "text_len": text_len,
        "text_truncated": text_truncated,
        "note": format!("bounded to max_chars={max_chars} (tool-loop option max_doc_chars={})", options.max_doc_chars),
    }))
}

fn tool_axql_elaborate(
    db: &PathDB,
    meta: Option<&MetaPlaneIndex>,
    default_contexts: &[crate::axql::AxqlContextSpec],
    snapshot_key: &str,
    query_cache: &mut crate::axql::AxqlPreparedQueryCache,
    args: &serde_json::Value,
) -> Result<serde_json::Value> {
    let mut query = parse_query_from_tool_args(args, "axql_elaborate")?;
    if query.contexts.is_empty() && !default_contexts.is_empty() {
        query.contexts = default_contexts.to_vec();
    }

    // We run the full prepare pipeline so we can return a plan (join order,
    // index hints, etc). This is untrusted tooling output, not a certificate.
    let key = crate::axql::axql_query_cache_key(snapshot_key, &query);
    let prepared = if let Some(p) = query_cache.get_mut(&key) {
        p
    } else {
        let p = crate::axql::prepare_axql_query_with_meta(db, &query, meta)?;
        query_cache.insert(key.clone(), p);
        query_cache
            .get_mut(&key)
            .expect("query cache insert")
    };

    let report = prepared.elaboration_report();
    let inferred_types: BTreeMap<String, Vec<String>> = report.inferred_types.clone();
    let plan = prepared.explain_plan_lines();
    Ok(serde_json::json!({
        "elaborated": prepared.elaborated_query_text(),
        "inferred_types": inferred_types,
        "notes": report.notes.clone(),
        "plan": plan
    }))
}

fn tool_axql_run(
    db: &PathDB,
    meta: Option<&MetaPlaneIndex>,
    default_contexts: &[crate::axql::AxqlContextSpec],
    snapshot_key: &str,
    query_cache: &mut crate::axql::AxqlPreparedQueryCache,
    args: &serde_json::Value,
    options: ToolLoopOptions,
) -> Result<serde_json::Value> {
    let mut query = parse_query_from_tool_args(args, "axql_run")?;
    if query.contexts.is_empty() && !default_contexts.is_empty() {
        query.contexts = default_contexts.to_vec();
    }

    // Safety: cap row count.
    let cap = options.max_rows.clamp(1, 200);
    query.limit = query.limit.min(cap).max(1);

    let result = crate::axql::execute_axql_query_cached(db, &query, meta, snapshot_key, query_cache)?;
    let mut preview = PluginResultsV1::from_axql_result(db, &result);
    if preview.rows.len() > cap {
        preview.rows.truncate(cap);
        preview.truncated = true;
    }

    Ok(serde_json::json!({
        "query": crate::nlq::render_axql_query(&query),
        "results": preview
    }))
}

fn tool_viz_render(
    db: &PathDB,
    meta: Option<&MetaPlaneIndex>,
    args: &serde_json::Value,
) -> Result<serde_json::Value> {
    #[derive(Deserialize)]
    struct Args {
        focus_name: String,
        #[serde(default)]
        hops: Option<usize>,
        #[serde(default)]
        plane: Option<String>,
        #[serde(default)]
        max_nodes: Option<usize>,
        #[serde(default)]
        max_edges: Option<usize>,
    }
    let a: Args =
        serde_json::from_value(args.clone()).map_err(|e| anyhow!("viz_render: invalid args: {e}"))?;

    let focus = a.focus_name.trim();
    if focus.is_empty() {
        return Err(anyhow!("viz_render: focus_name must be non-empty"));
    }

    let Some(focus_id) = crate::viz::resolve_focus_by_name(db, focus)? else {
        return Ok(serde_json::json!({ "error": format!("no entity named `{focus}`") }));
    };

    let hops = a.hops.unwrap_or(2).min(6);
    let plane = a.plane.unwrap_or_else(|| "both".to_string()).to_ascii_lowercase();
    let (include_meta_plane, include_data_plane) = match plane.as_str() {
        "data" => (false, true),
        "meta" => (true, false),
        "both" => (true, true),
        other => {
            return Err(anyhow!(
                "viz_render: unknown plane `{other}` (expected data|meta|both)"
            ))
        }
    };

    let max_nodes = a.max_nodes.unwrap_or(320).clamp(10, 5_000);
    let max_edges = a.max_edges.unwrap_or(8_000).clamp(10, 50_000);

    let options = crate::viz::VizOptions {
        focus_ids: vec![focus_id],
        hops,
        max_nodes,
        max_edges,
        direction: crate::viz::VizDirection::Both,
        include_meta_plane,
        include_data_plane,
        include_equivalences: true,
        typed_overlay: true,
    };

    let g = crate::viz::extract_viz_graph_with_meta(db, &options, meta)?;
    let html = crate::viz::render_html(db, &g)?;

    let out_dir = repo_root().join("build/llm_agent");
    std::fs::create_dir_all(&out_dir)?;
    let filename = format!("viz_{}_{}.html", sanitize_filename(focus), axiograph_dsl::digest::axi_digest_v1(focus));
    let out_path = out_dir.join(filename);
    std::fs::write(&out_path, html)?;

    Ok(serde_json::json!({
        "wrote": out_path.display().to_string(),
        "nodes": g.nodes.len(),
        "edges": g.edges.len(),
        "truncated": g.truncated
    }))
}

fn tool_quality_report(db: &PathDB, args: &serde_json::Value) -> Result<serde_json::Value> {
    #[derive(Deserialize)]
    struct Args {
        #[serde(default)]
        profile: Option<String>,
        #[serde(default)]
        plane: Option<String>,
    }
    let a: Args = serde_json::from_value(args.clone())
        .map_err(|e| anyhow!("quality_report: invalid args: {e}"))?;

    let profile = a.profile.unwrap_or_else(|| "fast".to_string());
    let plane = a.plane.unwrap_or_else(|| "both".to_string());

    let input_label = PathBuf::from("repl:llm_agent");
    let report = crate::quality::run_quality_checks(
        db,
        &input_label,
        &profile.trim().to_ascii_lowercase(),
        &plane.trim().to_ascii_lowercase(),
    )?;
    Ok(serde_json::to_value(report)?)
}

fn tool_propose_axi_patch(args: &serde_json::Value) -> Result<serde_json::Value> {
    #[derive(Deserialize)]
    struct Args {
        proposals_path: String,
        #[serde(default)]
        module_name: Option<String>,
        #[serde(default)]
        schema_name: Option<String>,
        #[serde(default)]
        instance_name: Option<String>,
        #[serde(default)]
        infer_constraints: Option<bool>,
    }
    let a: Args = serde_json::from_value(args.clone())
        .map_err(|e| anyhow!("propose_axi_patch: invalid args: {e}"))?;

    let proposals_path = a.proposals_path.trim();
    if proposals_path.is_empty() {
        return Err(anyhow!("propose_axi_patch: proposals_path must be non-empty"));
    }

    let path = PathBuf::from(proposals_path);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| anyhow!("propose_axi_patch: failed to read {}: {e}", path.display()))?;
    let file: axiograph_ingest_docs::ProposalsFileV1 = serde_json::from_str(&text)
        .map_err(|e| anyhow!("propose_axi_patch: invalid proposals.json: {e}"))?;

    let opts = crate::schema_discovery::DraftAxiModuleOptions {
        module_name: a.module_name.unwrap_or_else(|| "DraftModule".to_string()),
        schema_name: a.schema_name.unwrap_or_else(|| "DraftSchema".to_string()),
        instance_name: a.instance_name.unwrap_or_else(|| "DraftInstance".to_string()),
        infer_constraints: a.infer_constraints.unwrap_or(true),
    };
    let axi = crate::schema_discovery::draft_axi_module_from_proposals(&file, &opts)?;
    let digest = axiograph_dsl::digest::axi_digest_v1(&axi);

    let out_dir = repo_root().join("build/llm_agent");
    std::fs::create_dir_all(&out_dir)?;
    let out_path = out_dir.join(format!("draft_{digest}.axi"));
    std::fs::write(&out_path, &axi)?;

    Ok(serde_json::json!({
        "wrote": out_path.display().to_string(),
        "digest": digest,
        "module_name": opts.module_name,
        "schema_name": opts.schema_name,
        "instance_name": opts.instance_name
    }))
}

fn tool_draft_axi_from_proposals(args: &serde_json::Value) -> Result<serde_json::Value> {
    #[derive(Deserialize)]
    struct Args {
        proposals_json: serde_json::Value,
        #[serde(default)]
        module_name: Option<String>,
        #[serde(default)]
        schema_name: Option<String>,
        #[serde(default)]
        instance_name: Option<String>,
        #[serde(default)]
        infer_constraints: Option<bool>,
    }
    let a: Args = serde_json::from_value(args.clone())
        .map_err(|e| anyhow!("draft_axi_from_proposals: invalid args: {e}"))?;

    let proposals: axiograph_ingest_docs::ProposalsFileV1 = serde_json::from_value(a.proposals_json)
        .map_err(|e| anyhow!("draft_axi_from_proposals: invalid proposals_json: {e}"))?;

    let opts = crate::schema_discovery::DraftAxiModuleOptions {
        module_name: a.module_name.unwrap_or_else(|| "DraftModule".to_string()),
        schema_name: a.schema_name.unwrap_or_else(|| "DraftSchema".to_string()),
        instance_name: a.instance_name.unwrap_or_else(|| "DraftInstance".to_string()),
        infer_constraints: a.infer_constraints.unwrap_or(true),
    };
    let axi_text = crate::schema_discovery::draft_axi_module_from_proposals(&proposals, &opts)?;
    let digest = axiograph_dsl::digest::axi_digest_v1(&axi_text);

    Ok(serde_json::json!({
        "version": "axiograph_discover_draft_axi_v1",
        "digest": digest,
        "module_name": opts.module_name,
        "schema_name": opts.schema_name,
        "instance_name": opts.instance_name,
        "axi_text": axi_text,
    }))
}

fn tool_propose_relation_proposals(
    db: &PathDB,
    default_contexts: &[crate::axql::AxqlContextSpec],
    args: &serde_json::Value,
) -> Result<serde_json::Value> {
    #[derive(Deserialize)]
    struct Args {
        rel_type: String,
        source_name: String,
        target_name: String,
        #[serde(default)]
        source_type: Option<String>,
        #[serde(default)]
        target_type: Option<String>,
        /// Optional override: which relation field `source_name` should bind to.
        ///
        /// Example for `Parent(child, parent)`:
        /// - "Jamison is a child of Bob":  source_field="child",  target_field="parent"
        /// - "Bob is a parent of Jamison": source_field="parent", target_field="child"
        #[serde(default)]
        source_field: Option<String>,
        /// Optional override: which relation field `target_name` should bind to.
        #[serde(default)]
        target_field: Option<String>,
        #[serde(default)]
        context: Option<String>,
        /// Optional explicit time value (when the schema has a `time` field).
        #[serde(default)]
        time: Option<String>,
        #[serde(default)]
        confidence: Option<f64>,
        #[serde(default)]
        schema_hint: Option<String>,
        #[serde(default)]
        public_rationale: Option<String>,
        #[serde(default)]
        evidence_text: Option<String>,
        #[serde(default)]
        evidence_locator: Option<String>,
        #[serde(default)]
        validate: Option<bool>,
        #[serde(default)]
        quality_profile: Option<String>,
        #[serde(default)]
        quality_plane: Option<String>,
    }
    let a: Args = serde_json::from_value(args.clone())
        .map_err(|e| anyhow!("propose_relation_proposals: invalid args: {e}"))?;

    let out = crate::proposal_gen::propose_relation_proposals_v1(
        db,
        default_contexts,
        crate::proposal_gen::ProposeRelationInputV1 {
            rel_type: a.rel_type,
            source_name: a.source_name,
            target_name: a.target_name,
            source_type: a.source_type,
            target_type: a.target_type,
            source_field: a.source_field,
            target_field: a.target_field,
            context: a.context,
            time: a.time,
            confidence: a.confidence,
            schema_hint: a.schema_hint,
            public_rationale: a.public_rationale,
            evidence_text: a.evidence_text,
            evidence_locator: a.evidence_locator,
        },
    )?;

    let validate = a.validate.unwrap_or(true);
    let validation = if validate {
        let profile = a.quality_profile.unwrap_or_else(|| "fast".to_string());
        let plane = a.quality_plane.unwrap_or_else(|| "both".to_string());
        Some(crate::proposals_validate::validate_proposals_v1(
            db,
            &out.proposals,
            &profile,
            &plane,
        )?)
    } else {
        None
    };

    Ok(serde_json::json!({
        "proposals_json": out.proposals,
        "chunks": out.chunks,
        "summary": out.summary,
        "validation": validation,
    }))
}

fn tool_propose_relations_proposals(
    db: &PathDB,
    default_contexts: &[crate::axql::AxqlContextSpec],
    args: &serde_json::Value,
) -> Result<serde_json::Value> {
    #[derive(Deserialize)]
    struct Args {
        rel_type: String,
        source_names: Vec<String>,
        target_names: Vec<String>,
        #[serde(default)]
        pairing: Option<crate::proposal_gen::ProposeRelationsPairingV1>,
        #[serde(default)]
        source_type: Option<String>,
        #[serde(default)]
        target_type: Option<String>,
        #[serde(default)]
        source_field: Option<String>,
        #[serde(default)]
        target_field: Option<String>,
        #[serde(default)]
        context: Option<String>,
        #[serde(default)]
        time: Option<String>,
        #[serde(default)]
        confidence: Option<f64>,
        #[serde(default)]
        schema_hint: Option<String>,
        #[serde(default)]
        public_rationale: Option<String>,
        #[serde(default)]
        evidence_text: Option<String>,
        #[serde(default)]
        evidence_locator: Option<String>,
        #[serde(default)]
        validate: Option<bool>,
        #[serde(default)]
        quality_profile: Option<String>,
        #[serde(default)]
        quality_plane: Option<String>,
    }
    let a: Args = serde_json::from_value(args.clone())
        .map_err(|e| anyhow!("propose_relations_proposals: invalid args: {e}"))?;

    let out = crate::proposal_gen::propose_relations_proposals_v1(
        db,
        default_contexts,
        crate::proposal_gen::ProposeRelationsInputV1 {
            rel_type: a.rel_type,
            source_names: a.source_names,
            target_names: a.target_names,
            pairing: a.pairing,
            source_type: a.source_type,
            target_type: a.target_type,
            source_field: a.source_field,
            target_field: a.target_field,
            context: a.context,
            time: a.time,
            confidence: a.confidence,
            schema_hint: a.schema_hint,
            public_rationale: a.public_rationale,
            evidence_text: a.evidence_text,
            evidence_locator: a.evidence_locator,
        },
    )?;

    let validate = a.validate.unwrap_or(true);
    let validation = if validate {
        let profile = a.quality_profile.unwrap_or_else(|| "fast".to_string());
        let plane = a.quality_plane.unwrap_or_else(|| "both".to_string());
        Some(crate::proposals_validate::validate_proposals_v1(
            db,
            &out.proposals,
            &profile,
            &plane,
        )?)
    } else {
        None
    };

    Ok(serde_json::json!({
        "proposals_json": out.proposals,
        "chunks": out.chunks,
        "summary": out.summary,
        "validation": validation,
    }))
}

fn parse_query_from_tool_args(args: &serde_json::Value, tool: &str) -> Result<crate::axql::AxqlQuery> {
    #[derive(Deserialize)]
    struct Args {
        #[serde(default)]
        axql: Option<String>,
        #[serde(default)]
        query_ir_v1: Option<QueryIrV1>,
        #[serde(default)]
        limit: Option<usize>,
    }
    let a: Args =
        serde_json::from_value(args.clone()).map_err(|e| anyhow!("{tool}: invalid args: {e}"))?;

    let mut q = if let Some(ir) = a.query_ir_v1 {
        ir.to_axql_query()?
    } else if let Some(axql) = a.axql {
        let normalized = normalize_axql_candidate(&axql);
        crate::axql::parse_axql_query(&normalized)?
    } else {
        return Err(anyhow!("{tool}: expected `query_ir_v1` or `axql`"));
    };

    if let Some(limit) = a.limit {
        q.limit = limit;
    }
    Ok(q)
}

fn db_entity_attr_string(db: &PathDB, entity_id: u32, key: &str) -> Option<String> {
    let key_id = db.interner.id_of(key)?;
    let value_id = db.entities.get_attr(entity_id, key_id)?;
    db.interner.lookup(value_id)
}

fn truncate_preview(s: &str, max_chars: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_chars).collect();
    format!("{truncated}…")
}

fn compact_join_list(items: &[String], max_items: usize, max_chars: usize) -> String {
    if items.is_empty() {
        return "(none)".to_string();
    }
    let max_items = max_items.max(1);
    let shown = items.iter().take(max_items).cloned().collect::<Vec<_>>();
    let mut out = shown.join(", ");
    if items.len() > max_items {
        out.push_str(&format!(" (+{} more)", items.len() - max_items));
    }
    truncate_preview(&out, max_chars.max(32))
}

fn extract_identifier_like_terms(question: &str, max_terms: usize) -> Vec<String> {
    // Extract “identifier-ish” tokens so we can prefetch meta info (types/relations)
    // in a RAG-like way, without parsing a full query language.
    //
    // Examples we want to catch:
    // - Parent
    // - ProtoService
    // - proto_service_has_rpc
    // - acme.svc0.v1.Service0.GetWidget
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();

    fn flush(out: &mut Vec<String>, cur: &mut String, max_terms: usize) {
        if out.len() >= max_terms {
            cur.clear();
            return;
        }
        let t = cur.trim_matches(|c: char| c == '-' || c == '.' || c == ':' || c == '/');
        if t.len() >= 2 {
            out.push(t.to_string());
        }
        cur.clear();
    }

    for c in question.chars() {
        let ok = c.is_ascii_alphanumeric()
            || matches!(c, '_' | '.' | ':' | '/' | '-' | '~');
        if ok {
            cur.push(c);
        } else if !cur.is_empty() {
            flush(&mut out, &mut cur, max_terms);
        }
        if out.len() >= max_terms {
            break;
        }
    }
    if !cur.is_empty() && out.len() < max_terms {
        flush(&mut out, &mut cur, max_terms);
    }

    // Dedup while preserving order.
    let mut seen = BTreeSet::<String>::new();
    out.retain(|t| seen.insert(t.to_ascii_lowercase()));
    out
}

fn truncate_json_for_prompt(
    v: &serde_json::Value,
    depth: usize,
    limits: PromptJsonLimits,
) -> serde_json::Value {
    use serde_json::{Map, Number, Value};

    match v {
        Value::Null | Value::Bool(_) | Value::Number(_) => v.clone(),
        Value::String(s) => Value::String(truncate_preview(s, limits.max_string_chars)),
        Value::Array(xs) => {
            if depth >= limits.max_depth {
                return Value::Array(vec![Value::String("…".to_string())]);
            }
            let mut out: Vec<Value> = Vec::new();
            for x in xs.iter().take(limits.max_array_len) {
                out.push(truncate_json_for_prompt(x, depth + 1, limits));
            }
            if xs.len() > limits.max_array_len {
                out.push(Value::String("…".to_string()));
            }
            Value::Array(out)
        }
        Value::Object(m) => {
            if depth >= limits.max_depth {
                let mut out = Map::new();
                out.insert("_truncated".to_string(), Value::Bool(true));
                out.insert("_note".to_string(), Value::String("depth limit".to_string()));
                return Value::Object(out);
            }

            const PRIORITY_KEYS: &[&str] = &[
                "error",
                "note",
                "notes",
                "answer",
                "query",
                "queries",
                "results",
                "rows",
                "vars",
                "matches",
                "hits",
                "entity",
                "entities",
                "attrs",
                "incoming",
                "outgoing",
                "contexts",
                "equivalences",
                "summary",
                "validation",
                "proposals_json",
                "chunks",
                "drafted_axi",
                "elaborated",
                "inferred_types",
                "plan",
            ];

            let mut out = Map::new();
            let mut kept: HashSet<String> = HashSet::new();

            for &k in PRIORITY_KEYS {
                if out.len() >= limits.max_object_keys {
                    break;
                }
                if let Some(v) = m.get(k) {
                    out.insert(k.to_string(), truncate_json_for_prompt(v, depth + 1, limits));
                    kept.insert(k.to_string());
                }
            }

            if out.len() < limits.max_object_keys {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                for k in keys {
                    if out.len() >= limits.max_object_keys {
                        break;
                    }
                    if kept.contains(k) {
                        continue;
                    }
                    if let Some(v) = m.get(k) {
                        out.insert(k.to_string(), truncate_json_for_prompt(v, depth + 1, limits));
                    }
                }
            }

            let omitted = m.len().saturating_sub(out.len());
            if omitted > 0 && out.len() < limits.max_object_keys {
                out.insert(
                    "_omitted_keys".to_string(),
                    Value::Number(Number::from(omitted as u64)),
                );
            }

            Value::Object(out)
        }
    }
}

fn compact_tool_loop_transcript_for_llm(
    transcript: &[ToolLoopTranscriptItemV1],
) -> Result<Vec<ToolLoopTranscriptItemV1>> {
    let limits = llm_prompt_json_limits()?;
    let start = transcript
        .len()
        .saturating_sub(limits.max_transcript_items.max(1));

    Ok(transcript[start..]
        .iter()
        .map(|s| ToolLoopTranscriptItemV1 {
            tool: s.tool.clone(),
            args: truncate_json_for_prompt(&s.args, 0, limits),
            result: truncate_json_for_prompt(&s.result, 0, limits),
        })
        .collect())
}

fn sanitize_filename(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
            out.push(c);
        } else {
            out.push('_');
        }
        if out.len() >= 80 {
            break;
        }
    }
    if out.is_empty() {
        "node".to_string()
    } else {
        out
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
}

impl LlmState {
    fn tool_loop_step(
        &self,
        db: &PathDB,
        question: &str,
        schema: &SchemaContextV1,
        tools: &[ToolSpecV1],
        transcript: &[ToolLoopTranscriptItemV1],
        snapshot_key: &str,
        embeddings: Option<&crate::embeddings::ResolvedEmbeddingsIndexV1>,
        ollama_embed_host: Option<&str>,
        options: ToolLoopOptions,
    ) -> Result<ToolLoopModelResponseV1> {
        match &self.backend {
            LlmBackend::Disabled => Err(anyhow!("LLM backend is disabled (use `llm use ...`)")),
            LlmBackend::Mock => Ok(mock_tool_loop_step(db, question, transcript, options)?),
            #[cfg(feature = "llm-ollama")]
            LlmBackend::Ollama { host } => {
                let Some(model) = self.model.as_deref() else {
                    return Err(anyhow!(
                        "no model selected (use `llm model <ollama_model>`; e.g. `llm model llama3.2`)"
                    ));
                };
                let transcript_for_llm = compact_tool_loop_transcript_for_llm(transcript)?;
                ollama_tool_loop_step(
                    host,
                    model,
                    db,
                    question,
                    schema,
                    tools,
                    &transcript_for_llm,
                    snapshot_key,
                    embeddings,
                    ollama_embed_host,
                    options,
                )
            }
            LlmBackend::Command { program, args } => {
                let transcript_for_llm = compact_tool_loop_transcript_for_llm(transcript)?;
                let request = PluginRequestV2 {
                    protocol: PLUGIN_PROTOCOL_V3.to_string(),
                    model: self.model.clone(),
                    task: PluginTaskV2::ToolLoopStep {
                        question: question.to_string(),
                        schema: schema.clone(),
                        tools: tools.to_vec(),
                        transcript: transcript_for_llm,
                    },
                };
                let response = run_plugin_v3(program, args, &request)?;
                if let Some(err) = response.error {
                    return Err(anyhow!("llm plugin error: {err}"));
                }
                Ok(response)
            }
        }
    }
}

fn mock_tool_loop_step(
    db: &PathDB,
    question: &str,
    transcript: &[ToolLoopTranscriptItemV1],
    options: ToolLoopOptions,
) -> Result<ToolLoopModelResponseV1> {
    let has_ran_query = transcript.iter().any(|s| s.tool == "axql_run");
    if !has_ran_query {
        // Deterministic: use the same NLQ templates as `ask` and then run.
        let tokens: Vec<String> = question.split_whitespace().map(|s| s.to_string()).collect();
        let q = crate::nlq::parse_ask_query(&tokens)?;
        let axql = crate::nlq::render_axql_query(&q);
        return Ok(ToolLoopModelResponseV1 {
            tool_call: Some(ToolCallV1 {
                name: "axql_run".to_string(),
                args: serde_json::json!({
                    "axql": axql,
                    "limit": options.max_rows
                }),
            }),
            final_answer: None,
            error: None,
        });
    }

    // If we have an axql_run result, summarize it deterministically.
    if let Some(last) = transcript.last() {
        if last.tool == "axql_run" {
            #[derive(Deserialize)]
            struct ResultsPayload {
                results: PluginResultsV1,
                #[serde(default)]
                query: Option<String>,
            }
            let payload: ResultsPayload = serde_json::from_value(last.result.clone()).unwrap_or(
                ResultsPayload {
                    results: PluginResultsV1 {
                        vars: Vec::new(),
                        rows: Vec::new(),
                        truncated: false,
                    },
                    query: None,
                },
            );

            let mut lines = Vec::new();
            if payload.results.rows.is_empty() {
                lines.push("No results.".to_string());
            } else {
                lines.push(format!("Found {} result(s).", payload.results.rows.len()));
                for (i, row) in payload.results.rows.iter().enumerate().take(6) {
                    let mut parts = Vec::new();
                    for (k, v) in row {
                        let label = v
                            .name
                            .clone()
                            .unwrap_or_else(|| v.id.to_string());
                        parts.push(format!("{k}={label}"));
                    }
                    lines.push(format!("row {i}: {}", parts.join(", ")));
                }
            }

            let mut notes = Vec::new();
            if let Some(q) = payload.query {
                notes.push(format!("query: {q}"));
            }
            notes.push("backend=mock (deterministic)".to_string());

            return Ok(ToolLoopModelResponseV1 {
                tool_call: None,
                final_answer: Some(ToolLoopFinalV1 {
                    answer: lines.join("\n"),
                    citations: Vec::new(),
                    queries: Vec::new(),
                    notes,
                }),
                error: None,
            });
        }
    }

    // Default: stop.
    Ok(ToolLoopModelResponseV1 {
        tool_call: None,
        final_answer: Some(ToolLoopFinalV1 {
            answer: "Done.".to_string(),
            citations: Vec::new(),
            queries: Vec::new(),
            notes: vec![format!("snapshot entities={}", db.entities.len())],
        }),
        error: None,
    })
}

#[cfg(feature = "llm-ollama")]
fn ollama_tool_loop_step(
    host: &str,
    model: &str,
    db: &PathDB,
    question: &str,
    schema: &SchemaContextV1,
    tools: &[ToolSpecV1],
    transcript: &[ToolLoopTranscriptItemV1],
    snapshot_key: &str,
    embeddings: Option<&crate::embeddings::ResolvedEmbeddingsIndexV1>,
    ollama_embed_host: Option<&str>,
    options: ToolLoopOptions,
) -> Result<ToolLoopModelResponseV1> {
    let grounding = render_doc_grounding(db, question, options.max_doc_chunks, options.max_doc_chars);
    let name_samples = render_entity_name_samples(db, schema);

    let rag_preview = if transcript.is_empty() {
        render_quasi_rag_preview(
            db,
            question,
            snapshot_key,
            embeddings,
            ollama_embed_host,
            options,
        )
    } else {
        String::new()
    };

    let schemas_text = if schema.schemas.is_empty() {
        "(none)".to_string()
    } else {
        schema.schemas.join(", ")
    };
    let relation_sigs_text = if schema.relation_signatures.is_empty() {
        "(none)".to_string()
    } else {
        schema
            .relation_signatures
            .iter()
            .take(40)
            .map(|s| format!("- {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let relation_constraints_text = if schema.relation_constraints.is_empty() {
        "(none)".to_string()
    } else {
        schema
            .relation_constraints
            .iter()
            .take(40)
            .map(|s| format!("- {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let contexts_text = if schema.contexts.is_empty() {
        "(none)".to_string()
    } else {
        schema.contexts.join(", ")
    };
    let times_text = if schema.times.is_empty() {
        "(none)".to_string()
    } else {
        schema.times.join(", ")
    };

let system = r#"You are an agent that answers questions by calling tools against an Axiograph snapshot.

You MUST return a single JSON object with one of these shapes:
- { "tool_call": { "name": "<tool>", "args": { ... } } }
- { "final_answer": { "answer": "...", "citations": [...], "queries": [...], "notes": [...] } }
- { "error": "<error message>" }

Examples:
- {"tool_call":{"name":"describe_entity","args":{"name":"Alice","max_rel_types":12,"out_limit":6,"in_limit":6}}}
- {"final_answer":{"answer":"Alice is connected to Bob via Parent(...)","citations":[],"queries":[],"notes":[]}}

Rules:
- Prefer tools over guessing. Do NOT invent entity ids/types/relations.
- Keep tool args small and use conservative limits.
- Tool outputs in the transcript may be truncated or omitted for compactness; if you need more detail, call tools again.
- For broad/overview questions (e.g. “explain the facts”, “what is in the snapshot”), start with `db_summary`.
- For fuzzy/semantic lookup (“what does this mean”, “find related”, “where is X mentioned”), use `semantic_search` and then follow up with `describe_entity` / `axql_run`.
- For doc evidence, use `fts_chunks` or `semantic_search` and then `docchunk_get` to fetch a specific chunk body.
- For requests that would *change* the graph (add/update facts/relationships), do NOT claim the DB changed. Instead, use `propose_relation_proposals` to generate a reviewable `proposals.json` overlay.
- Use `propose_relations_proposals` when the user asks for multiple pairs (e.g. “Jamison is a child of Alice and Bob”).
- If you are unsure how a relation is typed, or which fields are endpoints, use `lookup_relation` first.
- When generating relation proposals, be careful about *direction*:
  - `propose_relation_proposals` maps `source_name` → the relation's source-ish field (`from`/`source`/`child`/`lhs`) and `target_name` → (`to`/`target`/`parent`/`rhs`).
  - If the user’s phrasing is inverse (“Bob is a parent of Jamison”), set `source_field`/`target_field` explicitly (e.g. `source_field="parent"`, `target_field="child"` for `Parent(child,parent)`), or use an alias like `parent_of`.
- If the user wants canonical `.axi` output for a set of proposals, call `draft_axi_from_proposals` (deterministic draft; still untrusted until promoted and checked).
- If `DocChunk` evidence exists and you used it (via `fts_chunks`), cite it in `citations`.
- If no DocChunks are loaded, it is OK to answer from graph structure (AxQL + entity/edge inspection); note that you have no external doc evidence to cite.
- If the question is ambiguous, return a `final_answer` that asks 1 clarifying question rather than guessing.

Return JSON only (no markdown)."#;

    let user = format!(
        r#"Question:
{question}

{grounding}

{rag_preview}

{name_samples}

Schema context (types/relations are only hints; validate via tools):
- Schemas: {schemas}
- Types: {types}
- Relations: {relations}
- Relation signatures (meta-plane; use for correct field mapping):
{relation_signatures}
- Relation constraints (meta-plane):
{relation_constraints}
- Contexts present (data plane): {contexts}
- Times present (data plane): {times}

Available tools (name → args schema):
{tools_json}

Transcript (recent tool calls and results; may be truncated):
{transcript_json}

Return ONLY the JSON object."#,
        schemas = schemas_text,
        types = compact_join_list(&schema.types, 60, 1800),
        relations = compact_join_list(&schema.relations, 80, 2400),
        relation_signatures = relation_sigs_text,
        relation_constraints = relation_constraints_text,
        contexts = contexts_text,
        times = times_text,
        tools_json = serde_json::to_string(tools).unwrap_or_else(|_| "[]".to_string()),
        transcript_json = serde_json::to_string(transcript).unwrap_or_else(|_| "[]".to_string()),
    );

    let content = ollama_chat(host, model, &user, Some(system), Some(serde_json::json!("json")))?;
    parse_ollama_tool_loop_response(&content, options)
}

// =============================================================================
// Plugin protocol (stdin/stdout JSON)
// =============================================================================

#[cfg(feature = "llm-ollama")]
fn parse_ollama_tool_loop_response(
    content: &str,
    options: ToolLoopOptions,
) -> Result<ToolLoopModelResponseV1> {
    // Be permissive: many local models are inconsistent about the exact wrapper
    // shape. We accept:
    // - { "tool_call": { "name": "...", "args": {...} } }
    // - { "final_answer": { ... } }
    // - { "error": "..." }
    // - { "tool": "...", "args": {...} }          (common variant)
    // - { "name": "...", "args": {...} }          (common variant)
    // - { "axql": "..." } / { "query_ir_v1": {...} }  (treated as `axql_run`)
    // - { "answer": "..." } (treated as final answer)
    let v: serde_json::Value = parse_llm_json_object(content)?;

    if let Some(err) = v.get("error").and_then(|x| x.as_str()) {
        return Ok(ToolLoopModelResponseV1 {
            tool_call: None,
            final_answer: None,
            error: Some(err.to_string()),
        });
    }

    if let Some(final_v) = v.get("final_answer") {
        if let Ok(final_answer) = serde_json::from_value::<ToolLoopFinalV1>(final_v.clone()) {
            return Ok(ToolLoopModelResponseV1 {
                tool_call: None,
                final_answer: Some(final_answer),
                error: None,
            });
        }
    }

    // Top-level `answer` (no wrapper).
    if v.get("answer").is_some() && v.get("final_answer").is_none() {
        if let Ok(final_answer) = serde_json::from_value::<ToolLoopFinalV1>(v.clone()) {
            return Ok(ToolLoopModelResponseV1 {
                tool_call: None,
                final_answer: Some(final_answer),
                error: None,
            });
        }
        if let Some(answer) = v.get("answer").and_then(|x| x.as_str()) {
            return Ok(ToolLoopModelResponseV1 {
                tool_call: None,
                final_answer: Some(ToolLoopFinalV1 {
                    answer: answer.to_string(),
                    citations: Vec::new(),
                    queries: Vec::new(),
                    notes: vec!["note: model returned top-level `answer`".to_string()],
                }),
                error: None,
            });
        }
    }

    // Primary wrapper shape.
    if let Some(call_v) = v.get("tool_call") {
        if let Ok(call) = serde_json::from_value::<ToolCallV1>(call_v.clone()) {
            return Ok(ToolLoopModelResponseV1 {
                tool_call: Some(call),
                final_answer: None,
                error: None,
            });
        }
        // Nested variant: {"tool_call":{"tool":"axql_run","args":{...}}}
        if let Some(name) = call_v
            .get("name")
            .or_else(|| call_v.get("tool"))
            .and_then(|x| x.as_str())
        {
            let args = call_v.get("args").cloned().unwrap_or_else(|| serde_json::json!({}));
            return Ok(ToolLoopModelResponseV1 {
                tool_call: Some(ToolCallV1 {
                    name: name.to_string(),
                    args,
                }),
                final_answer: None,
                error: None,
            });
        }
    }

    // Common variant: {"tool":"axql_run","args":{...}} or {"name":"axql_run","args":{...}}
    if let Some(name) = v
        .get("name")
        .or_else(|| v.get("tool"))
        .and_then(|x| x.as_str())
    {
        let args = v.get("args").cloned().unwrap_or_else(|| serde_json::json!({}));
        return Ok(ToolLoopModelResponseV1 {
            tool_call: Some(ToolCallV1 {
                name: name.to_string(),
                args,
            }),
            final_answer: None,
            error: None,
        });
    }

    // Fallback: treat an `axql`/`query_ir_v1` payload as an `axql_run` tool call.
    if v.get("axql").is_some() || v.get("query_ir_v1").is_some() {
        let mut args = serde_json::Map::new();
        if let Some(axql) = v.get("axql").cloned() {
            args.insert("axql".to_string(), axql);
        }
        if let Some(ir) = v.get("query_ir_v1").cloned() {
            args.insert("query_ir_v1".to_string(), ir);
        }
        args.insert(
            "limit".to_string(),
            serde_json::json!(options.max_rows.clamp(1, 200)),
        );
        return Ok(ToolLoopModelResponseV1 {
            tool_call: Some(ToolCallV1 {
                name: "axql_run".to_string(),
                args: serde_json::Value::Object(args),
            }),
            final_answer: None,
            error: None,
        });
    }

    // Last resort: treat this as "no decision". We'll fall back to a
    // deterministic summary based on the tool transcript so far.
    Ok(ToolLoopModelResponseV1 {
        tool_call: None,
        final_answer: None,
        error: None,
    })
}

const PLUGIN_PROTOCOL_V2: &str = "axiograph_llm_plugin_v2";
const PLUGIN_PROTOCOL_V3: &str = "axiograph_llm_plugin_v3";

#[derive(Debug, Clone, Serialize)]
struct PluginRequestV1 {
    protocol: String,
    model: Option<String>,
    task: PluginTaskV1,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PluginTaskV1 {
    ToQuery {
        question: String,
        schema: SchemaContextV1,
    },
    Answer {
        question: String,
        query: QueryPayloadV1,
        results: PluginResultsV1,
    },
}

#[derive(Debug, Clone, Serialize)]
struct PluginRequestV2 {
    protocol: String,
    model: Option<String>,
    task: PluginTaskV2,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PluginTaskV2 {
    ToolLoopStep {
        question: String,
        schema: SchemaContextV1,
        tools: Vec<ToolSpecV1>,
        transcript: Vec<ToolLoopTranscriptItemV1>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum QueryPayloadV1 {
    Axql { axql: String },
}

#[derive(Debug, Clone, Serialize)]
struct SchemaContextV1 {
    types: Vec<String>,
    relations: Vec<String>,
    #[serde(default)]
    schemas: Vec<String>,
    /// Canonical relation signatures from the meta-plane (schema/theory),
    /// intended to help the model choose correct field mappings when proposing
    /// new facts.
    #[serde(default)]
    relation_signatures: Vec<String>,
    /// Theory constraint summaries (keys/functionals/etc) from the meta-plane.
    #[serde(default)]
    relation_constraints: Vec<String>,
    /// Sampled context/world names present in the data plane.
    #[serde(default)]
    contexts: Vec<String>,
    /// Sampled temporal marker names present in the data plane.
    #[serde(default)]
    times: Vec<String>,
}

impl SchemaContextV1 {
    fn from_db(db: &PathDB) -> Self {
        let mut type_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for entity_id in 0..db.entities.len() as u32 {
            let Some(type_id) = db.entities.get_type(entity_id) else {
                continue;
            };
            let Some(name) = db.interner.lookup(type_id) else {
                continue;
            };
            // Hide meta-plane internals from the LLM by default; it can still
            // discover them via tools like `lookup_type`.
            if name.starts_with("AxiMeta") {
                continue;
            }
            *type_counts.entry(name).or_insert(0) += 1;
        }

        let mut relation_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for rel_id in 0..db.relations.len() as u32 {
            let Some(rel) = db.relations.get_relation(rel_id) else {
                continue;
            };
            let Some(name) = db.interner.lookup(rel.rel_type) else {
                continue;
            };
            *relation_counts.entry(name).or_insert(0) += 1;
        }

        fn sample_names(db: &PathDB, type_name: &str, max: usize) -> Vec<String> {
            let Some(bm) = db.find_by_type(type_name) else {
                return Vec::new();
            };
            let mut out: Vec<String> = Vec::new();
            for id in bm.iter().take(max) {
                if let Some(name) = db_entity_attr_string(db, id, "name") {
                    out.push(name);
                }
            }
            out
        }

        let contexts = sample_names(db, "Context", 16);
        let times = sample_names(db, "Time", 16);

        fn top_by_count(mut m: std::collections::HashMap<String, usize>, max: usize) -> Vec<String> {
            let mut v: Vec<(usize, String)> = m.drain().map(|(k, c)| (c, k)).collect();
            v.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
            v.into_iter().take(max).map(|(_, k)| k).collect()
        }

        // Keep these in the prompt when present: they're common across demos.
        let preferred_types = [
            "Person",
            "Context",
            "Time",
            "World",
            "DocChunk",
            "Document",
            "ProtoService",
            "ProtoRpc",
            "ProtoMessage",
            "ProtoField",
        ];

        let mut types: Vec<String> = Vec::new();
        for t in preferred_types {
            if type_counts.contains_key(t) {
                types.push(t.to_string());
            }
        }
        let mut rest = top_by_count(type_counts, 120);
        rest.retain(|t| !types.iter().any(|x| x == t));
        types.extend(rest);

        let relations: Vec<String> = top_by_count(relation_counts, 200);

        Self {
            types,
            relations,
            schemas: Vec::new(),
            relation_signatures: Vec::new(),
            relation_constraints: Vec::new(),
            contexts,
            times,
        }
    }

    fn from_db_with_meta(db: &PathDB, meta: &MetaPlaneIndex) -> Self {
        let mut out = Self::from_db(db);

        let mut schema_names: Vec<String> = meta.schemas.keys().cloned().collect();
        schema_names.sort();
        out.schemas = schema_names.into_iter().take(32).collect();

        fn infer_endpoint_fields_from_decl(rel_decl: &axiograph_pathdb::axi_semantics::RelationDecl) -> (String, String) {
            let names: Vec<&str> = rel_decl.fields.iter().map(|f| f.field_name.as_str()).collect();
            if names.contains(&"from") && names.contains(&"to") {
                return ("from".to_string(), "to".to_string());
            }
            if names.contains(&"source") && names.contains(&"target") {
                return ("source".to_string(), "target".to_string());
            }
            if names.contains(&"lhs") && names.contains(&"rhs") {
                return ("lhs".to_string(), "rhs".to_string());
            }
            if names.contains(&"child") && names.contains(&"parent") {
                return ("child".to_string(), "parent".to_string());
            }
            if rel_decl.fields.len() >= 2 {
                return (
                    rel_decl.fields[0].field_name.clone(),
                    rel_decl.fields[1].field_name.clone(),
                );
            }
            ("from".to_string(), "to".to_string())
        }

        fn render_constraints(cs: &[axiograph_pathdb::axi_semantics::ConstraintDecl]) -> String {
            let mut parts: Vec<String> = Vec::new();
            for c in cs {
                use axiograph_pathdb::axi_semantics::ConstraintDecl as C;
                match c {
                    C::Functional {
                        src_field, dst_field, ..
                    } => parts.push(format!("functional({src_field} -> {dst_field})")),
                    C::Symmetric { .. } => parts.push("symmetric".to_string()),
                    C::Transitive { .. } => parts.push("transitive".to_string()),
                    C::Key { fields, .. } => parts.push(format!("key({})", fields.join(", "))), 
                    C::Unknown { text, .. } => parts.push(format!("unknown({text})")),
                }
            }
            parts.join("; ")
        }

        let mut sigs: Vec<String> = Vec::new();
        let mut constraint_lines: Vec<String> = Vec::new();

        // Rank relation signatures by observed fact-node count, so the prompt
        // stays compact even on large graphs while still covering "common" facts.
        let mut ranked: Vec<(usize, String, &axiograph_pathdb::axi_semantics::SchemaIndex, &axiograph_pathdb::axi_semantics::RelationDecl)> =
            Vec::new();
        for (schema_name, schema) in &meta.schemas {
            for rel in schema.relation_decls.values() {
                let count = db.find_by_type(&rel.name).map(|bm| bm.len()).unwrap_or(0) as usize;
                ranked.push((count, schema_name.clone(), schema, rel));
            }
        }
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)).then_with(|| a.3.name.cmp(&b.3.name)));

        for (_count, schema_name, schema, rel) in ranked.into_iter().take(120) {
                let mut fields = rel.fields.clone();
                fields.sort_by_key(|f| f.field_index);
                let fields_text = fields
                    .iter()
                    .map(|f| format!("{}: {}", f.field_name, f.field_type))
                    .collect::<Vec<_>>()
                    .join(", ");

                let (src_field, dst_field) = infer_endpoint_fields_from_decl(rel);
                sigs.push(format!(
                    "{schema_name}.{}({fields_text})  (source_field={src_field}, target_field={dst_field})",
                    rel.name
                ));

                if let Some(cs) = schema.constraints_by_relation.get(&rel.name) {
                    if !cs.is_empty() {
                        let rendered = render_constraints(cs);
                        if !rendered.trim().is_empty() {
                            constraint_lines.push(format!(
                                "{schema_name}.{}: {rendered}",
                                rel.name
                            ));
                        }
                    }
                }
        }

        out.relation_signatures = sigs.into_iter().take(80).collect();
        out.relation_constraints = constraint_lines.into_iter().take(80).collect();
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PluginResultsV1 {
    vars: Vec<String>,
    rows: Vec<BTreeMap<String, EntityViewV1>>,
    truncated: bool,
}

impl PluginResultsV1 {
    fn from_axql_result(db: &PathDB, r: &crate::axql::AxqlResult) -> Self {
        let vars = r.selected_vars.clone();
        let mut rows: Vec<BTreeMap<String, EntityViewV1>> = Vec::new();
        for row in &r.rows {
            let mut out: BTreeMap<String, EntityViewV1> = BTreeMap::new();
            for (k, id) in row {
                out.insert(k.clone(), EntityViewV1::from_id(db, *id));
            }
            rows.push(out);
        }
        Self {
            vars,
            rows,
            truncated: r.truncated,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EntityViewV1 {
    id: u32,
    entity_type: Option<String>,
    name: Option<String>,
}

impl EntityViewV1 {
    fn from_id(db: &PathDB, id: u32) -> Self {
        let Some(view) = db.get_entity(id) else {
            return Self {
                id,
                entity_type: None,
                name: None,
            };
        };
        Self {
            id,
            entity_type: Some(view.entity_type),
            name: view.attrs.get("name").cloned(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct PluginResponseV1 {
    #[serde(default)]
    axql: Option<String>,
    #[serde(default)]
    query_ir_v1: Option<QueryIrV1>,
    #[serde(default)]
    answer: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

fn run_plugin_v3(
    program: &PathBuf,
    args: &[String],
    request: &PluginRequestV2,
) -> Result<ToolLoopModelResponseV1> {
    let payload = serde_json::to_vec(request)?;

    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("failed to start llm plugin `{}`: {e}", program.display()))?;

    {
        let Some(mut stdin) = child.stdin.take() else {
            return Err(anyhow!("failed to open stdin for llm plugin"));
        };
        use std::io::Write;
        stdin.write_all(&payload)?;
    }

    let timeout = llm_timeout(None)?;
    let out = wait_with_output_timeout(
        child,
        timeout,
        &format!("llm plugin `{}`", program.display()),
    )?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow!(
            "llm plugin `{}` failed (exit={:?}): {}",
            program.display(),
            out.status.code(),
            stderr.trim()
        ));
    }

    let stdout = String::from_utf8(out.stdout).map_err(|e| {
        anyhow!(
            "llm plugin `{}` returned non-utf8 stdout: {e}",
            program.display()
        )
    })?;
    let stdout = stdout.trim();
    serde_json::from_str(stdout).map_err(|e| {
        let preview = stdout.chars().take(300).collect::<String>();
        anyhow!(
            "llm plugin `{}` returned invalid JSON: {e}; stdout starts with: {preview:?}",
            program.display()
        )
    })
}

fn run_plugin(
    program: &PathBuf,
    args: &[String],
    request: &PluginRequestV1,
) -> Result<PluginResponseV1> {
    let payload = serde_json::to_vec(request)?;

    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("failed to start llm plugin `{}`: {e}", program.display()))?;

    {
        let Some(mut stdin) = child.stdin.take() else {
            return Err(anyhow!("failed to open stdin for llm plugin"));
        };
        use std::io::Write;
        stdin.write_all(&payload)?;
    }

    let timeout = llm_timeout(None)?;
    let out = wait_with_output_timeout(
        child,
        timeout,
        &format!("llm plugin `{}`", program.display()),
    )?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow!(
            "llm plugin `{}` failed (exit={:?}): {}",
            program.display(),
            out.status.code(),
            stderr.trim()
        ));
    }

    let stdout = String::from_utf8(out.stdout).map_err(|e| {
        anyhow!(
            "llm plugin `{}` returned non-utf8 stdout: {e}",
            program.display()
        )
    })?;
    let stdout = stdout.trim();
    serde_json::from_str(stdout).map_err(|e| {
        let preview = stdout.chars().take(300).collect::<String>();
        anyhow!(
            "llm plugin `{}` returned invalid JSON: {e}; stdout starts with: {preview:?}",
            program.display()
        )
    })
}
