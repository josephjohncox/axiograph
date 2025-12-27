//! Sync Protocol: JSON message formats for LLM ↔ KG communication
//!
//! Defines structured message formats that can be used with any LLM provider.

#![allow(unused_imports)]

use crate::{ExtractedFact, GroundingContext, SchemaContext, StructuredFact};
use serde::{Deserialize, Serialize};

/// Protocol version
pub const PROTOCOL_VERSION: &str = "0.1.0";

/// A sync message from KG to LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMessage {
    pub version: String,
    pub message_type: MessageType,
    pub payload: MessagePayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    /// Grounding context for generation
    GroundingContext,
    /// Request fact extraction
    ExtractRequest,
    /// Schema information
    SchemaInfo,
    /// Validation request
    ValidationRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessagePayload {
    Grounding(GroundingContext),
    Extract {
        text: String,
        schema: SchemaContext,
    },
    Schema(SchemaContext),
    Validate {
        claim: String,
        evidence: Vec<String>,
    },
}

/// Response from LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponse {
    pub version: String,
    pub response_type: ResponseType,
    pub payload: ResponsePayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseType {
    Generated,
    Extracted,
    Validated,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponsePayload {
    /// Generated text with citations
    Generated {
        text: String,
        citations: Vec<String>,
    },
    /// Extracted facts
    Extracted { facts: Vec<StructuredFact> },
    /// Validation result
    Validated {
        valid: bool,
        confidence: f32,
        reasoning: String,
    },
    /// Error
    Error { code: String, message: String },
}

/// Prompt templates for LLM interaction
pub struct PromptTemplates;

impl PromptTemplates {
    /// System prompt for grounded generation
    pub fn grounded_generation() -> &'static str {
        r#"You are a knowledge-grounded assistant. You have access to a structured knowledge graph.

When answering:
1. Use the provided facts as your primary source
2. Cite facts using [FactID] format
3. If uncertain, state your confidence level
4. If the knowledge graph lacks information, say so explicitly
5. Do not hallucinate facts not in the context

Format: Always cite your sources inline."#
    }

    /// System prompt for fact extraction
    pub fn fact_extraction() -> &'static str {
        r#"Extract structured facts from the given text.

Output format (JSON):
{
  "facts": [
    {
      "type": "Entity|Relation|Constraint|TacitKnowledge",
      "content": { ... type-specific fields ... },
      "confidence": 0.0-1.0,
      "source_quote": "relevant text span"
    }
  ]
}

Entity fields: entity_type, name, attributes
Relation fields: rel_type, source, target, attributes
Constraint fields: name, condition, severity
TacitKnowledge fields: rule, confidence, domain

Be precise. Only extract what is explicitly stated or strongly implied."#
    }

    /// System prompt for validation
    pub fn claim_validation() -> &'static str {
        r#"Validate the claim against the provided evidence.

Output format:
{
  "valid": true|false,
  "confidence": 0.0-1.0,
  "reasoning": "explanation",
  "supporting_evidence": ["IDs of supporting facts"],
  "contradicting_evidence": ["IDs of contradicting facts"]
}"#
    }
}

/// Build a grounding message
pub fn build_grounding_message(context: GroundingContext) -> SyncMessage {
    SyncMessage {
        version: PROTOCOL_VERSION.to_string(),
        message_type: MessageType::GroundingContext,
        payload: MessagePayload::Grounding(context),
    }
}

/// Build an extraction request
pub fn build_extraction_request(text: &str, schema: SchemaContext) -> SyncMessage {
    SyncMessage {
        version: PROTOCOL_VERSION.to_string(),
        message_type: MessageType::ExtractRequest,
        payload: MessagePayload::Extract {
            text: text.to_string(),
            schema,
        },
    }
}

/// Parse LLM response
pub fn parse_response(json: &str) -> anyhow::Result<SyncResponse> {
    Ok(serde_json::from_str(json)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_serialization() {
        let msg = SyncMessage {
            version: PROTOCOL_VERSION.to_string(),
            message_type: MessageType::SchemaInfo,
            payload: MessagePayload::Schema(SchemaContext {
                entity_types: vec!["Material".to_string()],
                relation_types: vec!["hasMaterial".to_string()],
                constraints: vec![],
            }),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("Material"));

        let parsed: SyncMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, PROTOCOL_VERSION);
    }
}
