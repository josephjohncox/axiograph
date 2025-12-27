//! LLM Provider Interfaces
//!
//! Abstraction over different LLM providers (OpenAI, Anthropic, local models).
//! Currently provides stubs - implement with actual API calls as needed.

use crate::{GroundedFact, GroundingContext, LLMInterface, SchemaContext, StructuredFact};
use async_trait::async_trait;

/// OpenAI provider (stub)
#[cfg(feature = "openai")]
pub struct OpenAIProvider {
    pub api_key: String,
    pub model: String,
    pub base_url: Option<String>,
}

#[cfg(feature = "openai")]
impl OpenAIProvider {
    pub fn new(api_key: &str, model: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
            base_url: None,
        }
    }

    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = Some(url.to_string());
        self
    }
}

#[cfg(feature = "openai")]
#[async_trait]
impl LLMInterface for OpenAIProvider {
    async fn generate_grounded(
        &self,
        prompt: &str,
        context: &GroundingContext,
    ) -> anyhow::Result<String> {
        // Would call OpenAI API
        // For now, stub
        Ok(format!(
            "[OpenAI {}] Response to: {} (with {} facts)",
            self.model,
            prompt.chars().take(50).collect::<String>(),
            context.facts.len()
        ))
    }

    async fn extract_facts(
        &self,
        text: &str,
        _schema: &SchemaContext,
    ) -> anyhow::Result<Vec<StructuredFact>> {
        // Would call OpenAI with extraction prompt
        Ok(vec![])
    }

    async fn validate_claim(
        &self,
        claim: &str,
        evidence: &[GroundedFact],
    ) -> anyhow::Result<(bool, f32, String)> {
        // Would call OpenAI for validation
        Ok((
            true,
            0.8,
            format!("Validated against {} evidence items", evidence.len()),
        ))
    }
}

/// Anthropic provider (stub)
#[cfg(feature = "anthropic")]
pub struct AnthropicProvider {
    pub api_key: String,
    pub model: String,
}

#[cfg(feature = "anthropic")]
impl AnthropicProvider {
    pub fn new(api_key: &str, model: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
        }
    }
}

#[cfg(feature = "anthropic")]
#[async_trait]
impl LLMInterface for AnthropicProvider {
    async fn generate_grounded(
        &self,
        prompt: &str,
        context: &GroundingContext,
    ) -> anyhow::Result<String> {
        Ok(format!(
            "[Anthropic {}] Response to: {} (with {} facts)",
            self.model,
            prompt.chars().take(50).collect::<String>(),
            context.facts.len()
        ))
    }

    async fn extract_facts(
        &self,
        _text: &str,
        _schema: &SchemaContext,
    ) -> anyhow::Result<Vec<StructuredFact>> {
        Ok(vec![])
    }

    async fn validate_claim(
        &self,
        _claim: &str,
        evidence: &[GroundedFact],
    ) -> anyhow::Result<(bool, f32, String)> {
        Ok((
            true,
            0.85,
            format!("Validated with {} items", evidence.len()),
        ))
    }
}

/// Local model provider (e.g., llama.cpp, vLLM)
#[cfg(feature = "local")]
pub struct LocalProvider {
    pub model_path: String,
    pub endpoint: String,
}

#[cfg(feature = "local")]
impl LocalProvider {
    pub fn new(model_path: &str, endpoint: &str) -> Self {
        Self {
            model_path: model_path.to_string(),
            endpoint: endpoint.to_string(),
        }
    }
}

#[cfg(feature = "local")]
#[async_trait]
impl LLMInterface for LocalProvider {
    async fn generate_grounded(
        &self,
        prompt: &str,
        context: &GroundingContext,
    ) -> anyhow::Result<String> {
        Ok(format!(
            "[Local] Response to: {} (with {} facts)",
            prompt.chars().take(50).collect::<String>(),
            context.facts.len()
        ))
    }

    async fn extract_facts(
        &self,
        _text: &str,
        _schema: &SchemaContext,
    ) -> anyhow::Result<Vec<StructuredFact>> {
        Ok(vec![])
    }

    async fn validate_claim(
        &self,
        _claim: &str,
        evidence: &[GroundedFact],
    ) -> anyhow::Result<(bool, f32, String)> {
        Ok((
            true,
            0.7,
            format!("Local validation with {} items", evidence.len()),
        ))
    }
}

/// Mock provider for testing
pub struct MockProvider {
    pub responses: Vec<String>,
    response_idx: std::sync::atomic::AtomicUsize,
}

impl MockProvider {
    pub fn new(responses: Vec<String>) -> Self {
        Self {
            responses,
            response_idx: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub fn always(response: &str) -> Self {
        Self::new(vec![response.to_string()])
    }
}

#[async_trait]
impl LLMInterface for MockProvider {
    async fn generate_grounded(
        &self,
        _prompt: &str,
        _context: &GroundingContext,
    ) -> anyhow::Result<String> {
        let idx = self
            .response_idx
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(self
            .responses
            .get(idx % self.responses.len())
            .cloned()
            .unwrap_or_else(|| "Mock response".to_string()))
    }

    async fn extract_facts(
        &self,
        _text: &str,
        _schema: &SchemaContext,
    ) -> anyhow::Result<Vec<StructuredFact>> {
        Ok(vec![StructuredFact::Entity {
            entity_type: "MockEntity".to_string(),
            name: "test".to_string(),
            attributes: std::collections::HashMap::new(),
        }])
    }

    async fn validate_claim(
        &self,
        _claim: &str,
        _evidence: &[GroundedFact],
    ) -> anyhow::Result<(bool, f32, String)> {
        Ok((true, 0.95, "Mock validation".to_string()))
    }
}

/// Select provider based on configuration
pub fn create_provider(
    provider_type: &str,
    config: &std::collections::HashMap<String, String>,
) -> anyhow::Result<Box<dyn LLMInterface>> {
    match provider_type {
        #[cfg(feature = "openai")]
        "openai" => {
            let api_key = config
                .get("api_key")
                .ok_or_else(|| anyhow::anyhow!("OpenAI requires api_key"))?;
            let model = config.get("model").map(|s| s.as_str()).unwrap_or("gpt-4");
            Ok(Box::new(OpenAIProvider::new(api_key, model)))
        }
        #[cfg(feature = "anthropic")]
        "anthropic" => {
            let api_key = config
                .get("api_key")
                .ok_or_else(|| anyhow::anyhow!("Anthropic requires api_key"))?;
            let model = config
                .get("model")
                .map(|s| s.as_str())
                .unwrap_or("claude-3-opus-20240229");
            Ok(Box::new(AnthropicProvider::new(api_key, model)))
        }
        #[cfg(feature = "local")]
        "local" => {
            let model_path = config
                .get("model_path")
                .ok_or_else(|| anyhow::anyhow!("Local requires model_path"))?;
            let endpoint = config
                .get("endpoint")
                .map(|s| s.as_str())
                .unwrap_or("http://localhost:8000");
            Ok(Box::new(LocalProvider::new(model_path, endpoint)))
        }
        "mock" => {
            let response = config
                .get("response")
                .map(|s| s.as_str())
                .unwrap_or("Mock response");
            Ok(Box::new(MockProvider::always(response)))
        }
        _ => Err(anyhow::anyhow!("Unknown provider: {}", provider_type)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_provider() {
        let provider = MockProvider::always("Test response");
        let result = provider
            .generate_grounded(
                "Hello",
                &GroundingContext {
                    facts: vec![],
                    schema_context: None,
                    active_guardrails: vec![],
                    suggested_queries: vec![],
                },
            )
            .await
            .unwrap();

        assert_eq!(result, "Test response");
    }

    #[tokio::test]
    async fn test_mock_extraction() {
        let provider = MockProvider::always("Ignored");
        let facts = provider
            .extract_facts(
                "Some text",
                &SchemaContext {
                    entity_types: vec![],
                    relation_types: vec![],
                    constraints: vec![],
                },
            )
            .await
            .unwrap();

        assert_eq!(facts.len(), 1);
        if let StructuredFact::Entity { entity_type, .. } = &facts[0] {
            assert_eq!(entity_type, "MockEntity");
        }
    }
}
