//! Fact Extraction: Pattern-based and LLM-assisted extraction from text

use crate::{ConversationTurn, ExtractedFact, FactSource, FactStatus, LLMProvider, StructuredFact};
use chrono::Utc;
use regex::Regex;
use std::collections::HashMap;
use uuid::Uuid;

/// Pattern-based extractor for common fact patterns
pub struct PatternExtractor {
    patterns: Vec<ExtractionPattern>,
}

struct ExtractionPattern {
    name: &'static str,
    regex: Regex,
    extract: fn(&regex::Captures) -> Option<StructuredFact>,
    confidence: f32,
}

impl PatternExtractor {
    pub fn new() -> Result<Self, regex::Error> {
        Ok(Self {
            patterns: vec![
                // "X is a Y"
                ExtractionPattern {
                    name: "is_a",
                    regex: Regex::new(r"(?i)(\w+(?:\s+\w+)?)\s+is\s+(?:a|an)\s+(\w+)")?,
                    extract: |cap| {
                        Some(StructuredFact::Entity {
                            entity_type: cap.get(2)?.as_str().to_string(),
                            name: cap.get(1)?.as_str().to_string(),
                            attributes: HashMap::new(),
                        })
                    },
                    confidence: 0.85,
                },
                // "X has Y of Z"
                ExtractionPattern {
                    name: "has_property",
                    regex: Regex::new(r"(?i)(\w+)\s+has\s+(?:a\s+)?(\w+)\s+of\s+([^\.,]+)")?,
                    extract: |cap| {
                        let mut attrs = HashMap::new();
                        attrs.insert(
                            cap.get(2)?.as_str().to_string(),
                            cap.get(3)?.as_str().trim().to_string(),
                        );
                        Some(StructuredFact::Entity {
                            entity_type: "Unknown".to_string(),
                            name: cap.get(1)?.as_str().to_string(),
                            attributes: attrs,
                        })
                    },
                    confidence: 0.8,
                },
                // "X [verb] Y"
                ExtractionPattern {
                    name: "relation",
                    regex: Regex::new(
                        r"(?i)(\w+)\s+(requires|produces|uses|contains|includes)\s+(\w+)",
                    )?,
                    extract: |cap| {
                        Some(StructuredFact::Relation {
                            rel_type: cap.get(2)?.as_str().to_lowercase(),
                            source: cap.get(1)?.as_str().to_string(),
                            target: cap.get(3)?.as_str().to_string(),
                            attributes: HashMap::new(),
                        })
                    },
                    confidence: 0.75,
                },
                // "always/never/should X when Y"
                ExtractionPattern {
                    name: "tacit_rule",
                    regex: Regex::new(
                        r"(?i)(always|never|should|must)\s+(.+?)\s+when\s+(.+?)(?:\.|$)",
                    )?,
                    extract: |cap| {
                        let modal = cap.get(1)?.as_str().to_lowercase();
                        let action = cap.get(2)?.as_str();
                        let condition = cap.get(3)?.as_str();
                        Some(StructuredFact::TacitKnowledge {
                            rule: format!("{} -> {} {}", condition.trim(), modal, action.trim()),
                            confidence: 0.7,
                            domain: "general".to_string(),
                        })
                    },
                    confidence: 0.7,
                },
                // "because/due to" causal patterns
                ExtractionPattern {
                    name: "causal",
                    regex: Regex::new(
                        r"(?i)(\w+(?:\s+\w+)*)\s+(?:because|due to)\s+(.+?)(?:\.|$)",
                    )?,
                    extract: |cap| {
                        Some(StructuredFact::TacitKnowledge {
                            rule: format!(
                                "{} <- {}",
                                cap.get(1)?.as_str().trim(),
                                cap.get(2)?.as_str().trim()
                            ),
                            confidence: 0.65,
                            domain: "causal".to_string(),
                        })
                    },
                    confidence: 0.65,
                },
            ],
        })
    }

    /// Extract facts from text using patterns
    pub fn extract(&self, text: &str) -> Vec<(StructuredFact, f32, &'static str)> {
        let mut results = Vec::new();

        for pattern in &self.patterns {
            for cap in pattern.regex.captures_iter(text) {
                if let Some(fact) = (pattern.extract)(&cap) {
                    results.push((fact, pattern.confidence, pattern.name));
                }
            }
        }

        results
    }

    /// Extract from conversation
    pub fn extract_from_conversation(
        &self,
        conversation: &[ConversationTurn],
        provider: &LLMProvider,
        session_id: Uuid,
    ) -> Vec<ExtractedFact> {
        let mut facts = Vec::new();

        for (idx, turn) in conversation.iter().enumerate() {
            for (structured, confidence, pattern_name) in self.extract(&turn.content) {
                facts.push(ExtractedFact {
                    id: Uuid::new_v4(),
                    claim: format!(
                        "[{}] {}",
                        pattern_name,
                        self.structured_to_claim(&structured)
                    ),
                    structured,
                    confidence,
                    source: FactSource {
                        session_id,
                        provider: provider.clone(),
                        conversation_turns: vec![idx],
                        extraction_timestamp: Utc::now(),
                        human_verified: false,
                    },
                    status: FactStatus::Pending,
                });
            }
        }

        facts
    }

    fn structured_to_claim(&self, fact: &StructuredFact) -> String {
        match fact {
            StructuredFact::Entity {
                entity_type,
                name,
                attributes,
            } => {
                if attributes.is_empty() {
                    format!("{name} is a {entity_type}")
                } else {
                    let attrs: Vec<String> = attributes
                        .iter()
                        .map(|(k, v)| format!("{k}: {v}"))
                        .collect();
                    format!("{name} is a {entity_type} with {}", attrs.join(", "))
                }
            }
            StructuredFact::Relation {
                rel_type,
                source,
                target,
                ..
            } => {
                format!("{source} {rel_type} {target}")
            }
            StructuredFact::Constraint {
                name, condition, ..
            } => {
                format!("Constraint {name}: {condition}")
            }
            StructuredFact::TacitKnowledge { rule, .. } => {
                format!("Rule: {rule}")
            }
        }
    }
}

/// Domain-specific extraction patterns
pub struct DomainExtractor {
    patterns: Vec<ExtractionPattern>,
}

impl DomainExtractor {
    /// Create machining domain extractor
    pub fn machining() -> Result<Self, regex::Error> {
        Ok(Self {
            patterns: vec![
                ExtractionPattern {
                    name: "speed_limit",
                    regex: Regex::new(r"(?i)(?:speed|sfm|rpm)\s*(?:of|:)?\s*(\d+)\s*(?:sfm|rpm)?")?,
                    extract: |cap| {
                        let speed = cap.get(1)?.as_str();
                        let mut attrs = HashMap::new();
                        attrs.insert("speed".to_string(), speed.to_string());
                        Some(StructuredFact::Entity {
                            entity_type: "CuttingParameter".to_string(),
                            name: format!("speed_{speed}"),
                            attributes: attrs,
                        })
                    },
                    confidence: 0.9,
                },
                ExtractionPattern {
                    name: "coolant_rule",
                    regex: Regex::new(r"(?i)(use|apply|need)\s+coolant")?,
                    extract: |_| {
                        Some(StructuredFact::TacitKnowledge {
                            rule: "cutting -> useCoolant".to_string(),
                            confidence: 0.85,
                            domain: "machining".to_string(),
                        })
                    },
                    confidence: 0.85,
                },
                ExtractionPattern {
                    name: "material_hardness",
                    regex: Regex::new(
                        r"(?i)(\w+)\s+(?:has\s+)?hardness\s+(?:of\s+)?(\d+)\s*(?:hrc|bhn)?",
                    )?,
                    extract: |cap| {
                        let mut attrs = HashMap::new();
                        attrs.insert("hardness".to_string(), cap.get(2)?.as_str().to_string());
                        Some(StructuredFact::Entity {
                            entity_type: "Material".to_string(),
                            name: cap.get(1)?.as_str().to_string(),
                            attributes: attrs,
                        })
                    },
                    confidence: 0.9,
                },
            ],
        })
    }

    /// Create physics domain extractor
    pub fn physics() -> Result<Self, regex::Error> {
        Ok(Self {
            patterns: vec![ExtractionPattern {
                name: "unit_equation",
                regex: Regex::new(r"(?i)(\w+)\s*=\s*(\w+)\s*/\s*(\w+)")?,
                extract: |cap| {
                    let quantity = cap.get(1)?.as_str();
                    let numerator = cap.get(2)?.as_str();
                    let denominator = cap.get(3)?.as_str();
                    Some(StructuredFact::Constraint {
                        name: format!("dim_{quantity}"),
                        condition: format!("{quantity} = {numerator} / {denominator}"),
                        severity: "info".to_string(),
                    })
                },
                confidence: 0.8,
            }],
        })
    }

    pub fn extract(&self, text: &str) -> Vec<(StructuredFact, f32, &'static str)> {
        let mut results = Vec::new();
        for pattern in &self.patterns {
            for cap in pattern.regex.captures_iter(text) {
                if let Some(fact) = (pattern.extract)(&cap) {
                    results.push((fact, pattern.confidence, pattern.name));
                }
            }
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_a_pattern() {
        let extractor = PatternExtractor::new().expect("static extraction patterns compile");
        let results = extractor.extract("Titanium is a Material");

        assert!(!results.is_empty());
        if let StructuredFact::Entity {
            name, entity_type, ..
        } = &results[0].0
        {
            assert_eq!(name, "Titanium");
            assert_eq!(entity_type, "Material");
        } else {
            panic!("Expected Entity");
        }
    }

    #[test]
    fn test_tacit_rule_pattern() {
        let extractor = PatternExtractor::new().expect("static extraction patterns compile");
        let results = extractor.extract("Always use coolant when cutting titanium.");

        assert!(!results.is_empty());
        if let StructuredFact::TacitKnowledge { rule, .. } = &results[0].0 {
            assert!(rule.contains("coolant"));
            assert!(rule.contains("titanium"));
        } else {
            panic!("Expected TacitKnowledge");
        }
    }

    #[test]
    fn test_machining_domain() {
        let extractor = DomainExtractor::machining().expect("static machining patterns compile");
        let results = extractor.extract("Steel has hardness of 30 HRC");

        assert!(!results.is_empty());
        if let StructuredFact::Entity {
            name, attributes, ..
        } = &results[0].0
        {
            assert_eq!(name, "Steel");
            assert_eq!(attributes.get("hardness"), Some(&"30".to_string()));
        }
    }
}
