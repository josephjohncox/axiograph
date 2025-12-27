//! Discovery traces (a safe alternative to “chain-of-thought logging”).
//!
//! Axiograph’s discovery pipeline may use LLMs and other heuristics to propose new knowledge.
//! We want these proposals to be auditable without storing unstable or sensitive internal model
//! reasoning. A discovery trace is a structured record containing:
//!
//! - the query/task that was run,
//! - proposals (with confidence and evidence pointers),
//! - optional certificates for any certified conclusions.
//!
//! This module intentionally does **not** attempt to store “raw chain-of-thought”.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{Chunk, EvidencePointer, RepoEdgeV1};

/// A structured proposal produced by discovery (heuristics or LLMs).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DiscoveryProposalV1 {
    /// A proposed relation edge between two identifiers.
    Relation {
        rel_type: String,
        from: String,
        to: String,
        confidence: f64,
        evidence: Vec<EvidencePointer>,
        /// Short public rationale (not raw model hidden reasoning).
        public_rationale: String,
        #[serde(default)]
        metadata: HashMap<String, String>,
    },

    /// A free-form note proposal (still backed by evidence).
    Note {
        title: String,
        confidence: f64,
        evidence: Vec<EvidencePointer>,
        public_rationale: String,
        #[serde(default)]
        metadata: HashMap<String, String>,
    },
}

/// A discovery trace for a single query/task run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryTraceV1 {
    pub trace_id: String,
    pub query: String,
    /// ISO-8601 timestamp string (filled by the caller).
    pub generated_at: String,
    pub proposals: Vec<DiscoveryProposalV1>,
    /// Optional certificate JSON for any certified result produced by the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate_json: Option<serde_json::Value>,
}

/// Suggest simple “mentions” links from chunks to symbols defined in a repo index.
///
/// This is a lightweight, deterministic heuristic meant to demonstrate the *shape* of a discovery
/// pipeline (evidence-backed proposals) without relying on LLM calls.
pub fn suggest_mentions_symbol_trace_v1(
    chunks: &[Chunk],
    edges: &[RepoEdgeV1],
    max_proposals: usize,
    trace_id: String,
    generated_at: String,
) -> anyhow::Result<DiscoveryTraceV1> {
    use std::collections::{BTreeMap, BTreeSet};

    // Build symbol -> (defining file, kind) index from edges.
    let mut symbol_index: BTreeMap<String, (String, String)> = BTreeMap::new();
    for edge in edges {
        if let RepoEdgeV1::DefinesSymbol {
            file,
            symbol,
            symbol_kind,
            ..
        } = edge
        {
            symbol_index
                .entry(symbol.clone())
                .or_insert_with(|| (file.clone(), symbol_kind.clone()));
        }
    }

    let ident_re = regex::Regex::new(r"\b[A-Za-z_][A-Za-z0-9_]*\b")?;

    // (from_file, symbol) -> (confidence, evidence pointers, rationale, metadata)
    let mut proposals: BTreeMap<
        (String, String),
        (f64, Vec<EvidencePointer>, String, HashMap<String, String>),
    > = BTreeMap::new();

    for chunk in chunks {
        let Some(file) = chunk.metadata.get("path").cloned() else {
            continue;
        };

        let language = chunk
            .metadata
            .get("language")
            .cloned()
            .unwrap_or_else(|| "text".to_string());

        let mut seen: BTreeSet<String> = BTreeSet::new();
        for m in ident_re.find_iter(&chunk.text) {
            let ident = m.as_str().to_string();
            if !seen.insert(ident.clone()) {
                continue;
            }

            let Some((defined_in, symbol_kind)) = symbol_index.get(&ident).cloned() else {
                continue;
            };

            let confidence = if defined_in == file { 0.55 } else { 0.75 };
            let key = (file.clone(), ident.clone());

            let evidence = EvidencePointer {
                chunk_id: chunk.chunk_id.clone(),
                locator: Some(file.clone()),
                span_id: Some(chunk.span_id.clone()),
            };

            let public_rationale = format!(
                "Chunk mentions `{}`; it is defined as a `{}` in `{}` (language: {}).",
                ident, symbol_kind, defined_in, language
            );

            let mut metadata = HashMap::new();
            metadata.insert("defined_in".to_string(), defined_in);
            metadata.insert("symbol_kind".to_string(), symbol_kind);
            metadata.insert("language".to_string(), language.clone());

            proposals
                .entry(key)
                .and_modify(|(c, ev, r, _meta)| {
                    *c = (*c).max(confidence);
                    ev.push(evidence.clone());
                    if r.is_empty() {
                        *r = public_rationale.clone();
                    }
                })
                .or_insert_with(|| (confidence, vec![evidence], public_rationale, metadata));

            if proposals.len() >= max_proposals {
                break;
            }
        }

        if proposals.len() >= max_proposals {
            break;
        }
    }

    let proposals: Vec<DiscoveryProposalV1> = proposals
        .into_iter()
        .map(
            |((from, to), (confidence, evidence, public_rationale, metadata))| {
                DiscoveryProposalV1::Relation {
                    rel_type: "MentionsSymbol".to_string(),
                    from,
                    to,
                    confidence,
                    evidence,
                    public_rationale,
                    metadata,
                }
            },
        )
        .collect();

    Ok(DiscoveryTraceV1 {
        trace_id,
        query: "suggest_mentions_symbol_v1".to_string(),
        generated_at,
        proposals,
        certificate_json: None,
    })
}
