//! Probabilistic fact extraction from documents
//!
//! Uses patterns and heuristics to extract facts with confidence scores.
//! Confidence values are treated as bounded weights; their algebra and invariants
//! are specified/checked in Lean (see `lean/Axiograph/Prob/Verified.lean`).

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::Chunk;

/// An extracted fact with confidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedFact {
    pub fact_id: String,
    pub domain: String,
    pub statement: String,
    pub fact_type: FactType,
    pub confidence: f64,
    pub source_chunk_id: String,
    pub evidence_span: String,
    pub extracted_entities: HashMap<String, String>,
}

/// Types of facts we extract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FactType {
    Recommendation, // "use X for Y"
    Observation,    // "we saw X"
    Causation,      // "X causes Y"
    Parameter,      // "set X to Y"
    Comparison,     // "X is better than Y"
    Definition,     // "X is defined as Y"
    Procedure,      // "to do X, first Y then Z"
    Constraint,     // "X must be Y"
    Heuristic,      // "rule of thumb: X"
}

/// Extraction pattern with confidence
#[derive(Debug, Clone)]
pub struct ExtractionPattern {
    pub name: String,
    pub domain: String,
    pub fact_type: FactType,
    pub regex: Regex,
    pub base_confidence: f64,
    pub entity_groups: Vec<(usize, String)>, // (group_index, entity_name)
}

/// Build default extraction patterns for machining domain
pub fn machining_patterns() -> Vec<ExtractionPattern> {
    vec![
        // -----------------------------------------------------------------
        // Common observations in shop-floor conversations
        // -----------------------------------------------------------------
        ExtractionPattern {
            name: "chatter_observation".to_string(),
            domain: "machining".to_string(),
            fact_type: FactType::Observation,
            regex: Regex::new(
                r"(?i)(?:getting|having|experiencing)\s+(?:a\s+lot\s+of\s+)?(chatter|vibration)",
            )
            .unwrap(),
            base_confidence: 0.80,
            entity_groups: vec![(1, "issue".to_string())],
        },

        // Cutting parameter recommendations
        ExtractionPattern {
            name: "speed_recommendation".to_string(),
            domain: "machining".to_string(),
            fact_type: FactType::Recommendation,
            // Covers common phrasing like:
            // - "Try reducing to 100 SFM."
            // - "Set to 4000 RPM."
            // - "Recommend 60 m/min."
            //
            // We intentionally do not require the material to appear in the same sentence; that
            // kind of context tracking belongs in a later "conversation context" pass.
            regex: Regex::new(
                r"(?i)(?:try\s+(?:reducing|reduce|increasing|increase)\s*(?:to\s*)?|reduce(?:d|ing)?\s*(?:to\s*)?|increase(?:d|ing)?\s*(?:to\s*)?|use\s*(?:at\s*)?|set\s*(?:to\s*)?|run\s*(?:at\s*)?|running\s*(?:at\s*)?|recommend\s*)(\d+(?:\.\d+)?)\s*(sfm|rpm|m/min|m\s*/\s*min)",
            )
            .unwrap(),
            base_confidence: 0.75,
            entity_groups: vec![(1, "speed".to_string()), (2, "unit".to_string())],
        },
        ExtractionPattern {
            name: "speed_parameter".to_string(),
            domain: "machining".to_string(),
            fact_type: FactType::Parameter,
            // Covers common phrasing like:
            // - "About 200 SFM with the carbide endmill"
            // - "Running at 4500 RPM"
            regex: Regex::new(
                r"(?i)(?:about|around|at|running\s+at|run\s+at)\s*(\d+(?:\.\d+)?)\s*(sfm|rpm|m/min|m\s*/\s*min)",
            )
            .unwrap(),
            base_confidence: 0.70,
            entity_groups: vec![(1, "speed".to_string()), (2, "unit".to_string())],
        },
        ExtractionPattern {
            name: "feed_recommendation".to_string(),
            domain: "machining".to_string(),
            fact_type: FactType::Recommendation,
            // Covers common phrasing like:
            // - "For roughing, try 0.004 IPT."
            // - "Recommend 80 ipm"
            // - "Feed: 0.002 mm/tooth"
            regex: Regex::new(
                r"(?i)(?:feed(?:\s+rate)?\s*(?:of|at|:|=)?\s*)?(?:try|recommend|use|run)?\s*(\d+(?:\.\d+)?)\s*(ipt|ipm|fpt|mm/rev|mm/tooth)",
            )
            .unwrap(),
            base_confidence: 0.7,
            entity_groups: vec![(1, "feed".to_string()), (2, "unit".to_string())],
        },
        ExtractionPattern {
            name: "depth_of_cut_recommendation".to_string(),
            domain: "machining".to_string(),
            fact_type: FactType::Recommendation,
            // Covers common phrasing like:
            // - 'maybe 0.050" axial'
            // - '0.5 mm radial'
            regex: Regex::new(r#"(?i)(\d+(?:\.\d+)?)\s*("|in|mm)\s*(axial|radial)"#).unwrap(),
            base_confidence: 0.65,
            entity_groups: vec![
                (1, "depth".to_string()),
                (2, "unit".to_string()),
                (3, "direction".to_string()),
            ],
        },
        ExtractionPattern {
            name: "coolant_recommendation".to_string(),
            domain: "machining".to_string(),
            fact_type: FactType::Recommendation,
            // Very common shop-floor guidance; keep the regex tight to avoid matching every mention.
            regex: Regex::new(r"(?i)(flood coolant|high pressure coolant)").unwrap(),
            base_confidence: 0.70,
            entity_groups: vec![(1, "coolant".to_string())],
        },
        // Tool wear observations
        ExtractionPattern {
            name: "wear_observation".to_string(),
            domain: "machining".to_string(),
            fact_type: FactType::Observation,
            regex: Regex::new(r"(?i)(flank|crater|notch|built-up edge|bue)\s*wear.*?(\d+(?:\.\d+)?)\s*(mm|minutes?|parts?)").unwrap(),
            base_confidence: 0.8,
            entity_groups: vec![(1, "wear_type".to_string()), (2, "amount".to_string()), (3, "unit".to_string())],
        },
        // Chatter/vibration
        ExtractionPattern {
            name: "chatter_condition".to_string(),
            domain: "machining".to_string(),
            fact_type: FactType::Observation,
            regex: Regex::new(r"(?i)chatter.*?(?:at|when|above|below)\s*(\d+)\s*(rpm|sfm|mm)").unwrap(),
            base_confidence: 0.85,
            entity_groups: vec![(1, "threshold".to_string()), (2, "unit".to_string())],
        },
        // Causation patterns
        ExtractionPattern {
            name: "cause_effect".to_string(),
            domain: "machining".to_string(),
            fact_type: FactType::Causation,
            regex: Regex::new(r"(?i)(increasing|decreasing|higher|lower)\s+(\w+)\s+(?:causes|leads to|results in|improves|worsens)\s+(\w+)").unwrap(),
            base_confidence: 0.65,
            entity_groups: vec![(1, "direction".to_string()), (2, "cause".to_string()), (3, "effect".to_string())],
        },
        // Rule of thumb / heuristics
        ExtractionPattern {
            name: "heuristic".to_string(),
            domain: "machining".to_string(),
            fact_type: FactType::Heuristic,
            regex: Regex::new(r"(?i)(?:rule of thumb|generally|typically|as a rule|in practice)[:\s]+(.{10,100})").unwrap(),
            base_confidence: 0.6,
            entity_groups: vec![(1, "rule".to_string())],
        },
        ExtractionPattern {
            name: "heuristic_higher_feeds_lower_speeds".to_string(),
            domain: "machining".to_string(),
            fact_type: FactType::Heuristic,
            regex: Regex::new(r"(?i)(higher\s+feeds,?\s+lower\s+speeds)").unwrap(),
            base_confidence: 0.65,
            entity_groups: vec![(1, "rule".to_string())],
        },
        // Material-tool recommendations
        ExtractionPattern {
            name: "material_tool".to_string(),
            domain: "machining".to_string(),
            fact_type: FactType::Recommendation,
            regex: Regex::new(r"(?i)(?:for|when cutting|machining)\s+(aluminum|steel|titanium|inconel|stainless|brass|copper).*?(?:use|recommend|prefer)\s+(carbide|hss|ceramic|cbn|pcd|diamond)").unwrap(),
            base_confidence: 0.7,
            entity_groups: vec![(1, "material".to_string()), (2, "tool_material".to_string())],
        },
        ExtractionPattern {
            name: "coated_tool_recommendation".to_string(),
            domain: "machining".to_string(),
            fact_type: FactType::Recommendation,
            // Captures phrasing like:
            // - "I always use TiAlN coated carbide."
            // - "Use AlCrN carbide."
            regex: Regex::new(
                r"(?i)(?:always\s+use|use|recommend|prefer)\s+([a-z0-9][a-z0-9\-]*)\s+(?:coated\s+)?(carbide|hss|ceramic|cbn|pcd|diamond)",
            )
            .unwrap(),
            base_confidence: 0.70,
            entity_groups: vec![(1, "coating".to_string()), (2, "tool_material".to_string())],
        },
        // Procedures
        ExtractionPattern {
            name: "procedure_step".to_string(),
            domain: "general".to_string(),
            fact_type: FactType::Procedure,
            regex: Regex::new(r"(?i)(?:first|then|next|finally|step \d+)[:\s]+(.{10,150})").unwrap(),
            base_confidence: 0.55,
            entity_groups: vec![(1, "step".to_string())],
        },
    ]
}

/// Extract facts from a chunk using patterns
pub fn extract_facts_from_chunk(
    chunk: &Chunk,
    patterns: &[ExtractionPattern],
    domain_filter: Option<&str>,
) -> Vec<ExtractedFact> {
    let mut facts = Vec::new();
    let mut fact_counter = 0;

    for pattern in patterns {
        // Apply domain filter if specified
        if let Some(domain) = domain_filter {
            if pattern.domain != domain && pattern.domain != "general" {
                continue;
            }
        }

        for caps in pattern.regex.captures_iter(&chunk.text) {
            let full_match = caps.get(0).unwrap().as_str();

            // Extract entities
            let mut entities = HashMap::new();
            for (group_idx, entity_name) in &pattern.entity_groups {
                if let Some(m) = caps.get(*group_idx) {
                    entities.insert(entity_name.clone(), m.as_str().to_string());
                }
            }

            // Compute confidence based on context
            let confidence = compute_confidence(pattern.base_confidence, chunk, full_match);

            facts.push(ExtractedFact {
                fact_id: format!("{}_{}", chunk.chunk_id, fact_counter),
                domain: pattern.domain.clone(),
                statement: full_match.to_string(),
                fact_type: pattern.fact_type.clone(),
                confidence,
                source_chunk_id: chunk.chunk_id.clone(),
                evidence_span: full_match.to_string(),
                extracted_entities: entities,
            });

            fact_counter += 1;
        }
    }

    facts
}

/// Compute confidence based on context factors
fn compute_confidence(base: f64, chunk: &Chunk, evidence: &str) -> f64 {
    let mut conf = base;

    // Boost for technical source
    if chunk.metadata.get("source_type").map(|s| s.as_str()) == Some("confluence")
        || chunk.metadata.get("source_type").map(|s| s.as_str()) == Some("technical_document")
    {
        conf *= 1.1;
    }

    // Boost for expert source
    if chunk.metadata.contains_key("expert") {
        conf *= 1.15;
    }

    // Penalty for short evidence
    if evidence.len() < 30 {
        conf *= 0.9;
    }

    // Boost for numerical specificity
    let num_re = Regex::new(r"\d+(?:\.\d+)?").unwrap();
    let num_count = num_re.find_iter(evidence).count();
    if num_count >= 2 {
        conf *= 1.1;
    }

    // Penalty for hedging language
    let hedging = ["maybe", "possibly", "might", "could be", "sometimes"];
    for hedge in &hedging {
        if evidence.to_lowercase().contains(hedge) {
            conf *= 0.85;
            break;
        }
    }

    // Clamp to [0, 1]
    conf.min(1.0).max(0.0)
}

/// Aggregate facts, merging duplicates and adjusting confidence
pub fn aggregate_facts(facts: Vec<ExtractedFact>) -> Vec<ExtractedFact> {
    let mut aggregated: HashMap<String, ExtractedFact> = HashMap::new();

    for fact in facts {
        let key = format!("{}:{}", fact.domain, normalize_statement(&fact.statement));

        if let Some(existing) = aggregated.get_mut(&key) {
            // Combine confidence (independent evidence combination)
            existing.confidence = 1.0 - (1.0 - existing.confidence) * (1.0 - fact.confidence);
        } else {
            aggregated.insert(key, fact);
        }
    }

    aggregated.into_values().collect()
}

fn normalize_statement(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Convert extracted facts to Axiograph relation tuples
pub fn facts_to_axi_tuples(facts: &[ExtractedFact]) -> Vec<(String, Vec<(String, String)>)> {
    facts
        .iter()
        .map(|f| {
            let mut fields = vec![
                ("statement".to_string(), f.statement.clone()),
                ("confidence".to_string(), format!("{:.2}", f.confidence)),
                ("source".to_string(), f.source_chunk_id.clone()),
            ];

            for (k, v) in &f.extracted_entities {
                fields.push((k.clone(), v.clone()));
            }

            (format!("{:?}", f.fact_type), fields)
        })
        .collect()
}
