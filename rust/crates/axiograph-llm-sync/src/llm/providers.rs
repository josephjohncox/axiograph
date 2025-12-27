//! LLM API Providers
//!
//! Concrete implementations for OpenAI, Anthropic, and local models.

use super::*;
use reqwest::Client;
use std::time::Duration;

// ============================================================================
// Configuration
// ============================================================================

/// LLM configuration loaded from environment or config file
#[derive(Debug, Clone)]
pub struct LLMConfig {
    pub provider: Provider,
    pub api_key: String,
    pub model: String,
    pub base_url: Option<String>,
    pub timeout_secs: u64,
    pub max_retries: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    OpenAI,
    Anthropic,
    Local,
    Azure,
}

impl LLMConfig {
    /// Load from environment variables
    pub fn from_env() -> Result<Self, ConfigError> {
        // Try OpenAI first
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            return Ok(Self {
                provider: Provider::OpenAI,
                api_key: key,
                model: std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4-turbo-preview".to_string()),
                base_url: std::env::var("OPENAI_BASE_URL").ok(),
                timeout_secs: 60,
                max_retries: 3,
            });
        }
        
        // Try Anthropic
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            return Ok(Self {
                provider: Provider::Anthropic,
                api_key: key,
                model: std::env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-3-opus-20240229".to_string()),
                base_url: None,
                timeout_secs: 60,
                max_retries: 3,
            });
        }
        
        // Try local
        if let Ok(url) = std::env::var("LOCAL_LLM_URL") {
            return Ok(Self {
                provider: Provider::Local,
                api_key: String::new(),
                model: std::env::var("LOCAL_LLM_MODEL").unwrap_or_else(|_| "default".to_string()),
                base_url: Some(url),
                timeout_secs: 120,
                max_retries: 1,
            });
        }
        
        Err(ConfigError::NoProviderConfigured)
    }

    /// Create OpenAI config
    pub fn openai(api_key: &str, model: &str) -> Self {
        Self {
            provider: Provider::OpenAI,
            api_key: api_key.to_string(),
            model: model.to_string(),
            base_url: None,
            timeout_secs: 60,
            max_retries: 3,
        }
    }

    /// Create Anthropic config
    pub fn anthropic(api_key: &str, model: &str) -> Self {
        Self {
            provider: Provider::Anthropic,
            api_key: api_key.to_string(),
            model: model.to_string(),
            base_url: None,
            timeout_secs: 60,
            max_retries: 3,
        }
    }

    /// Create local config
    pub fn local(url: &str, model: &str) -> Self {
        Self {
            provider: Provider::Local,
            api_key: String::new(),
            model: model.to_string(),
            base_url: Some(url.to_string()),
            timeout_secs: 120,
            max_retries: 1,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("No LLM provider configured. Set OPENAI_API_KEY, ANTHROPIC_API_KEY, or LOCAL_LLM_URL")]
    NoProviderConfigured,
    #[error("Invalid configuration: {0}")]
    Invalid(String),
}

// ============================================================================
// OpenAI Provider
// ============================================================================

pub struct OpenAIClient {
    client: Client,
    config: LLMConfig,
}

impl OpenAIClient {
    pub fn new(config: LLMConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to create HTTP client");
        
        Self { client, config }
    }

    pub async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, LLMError> {
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.as_deref().unwrap_or("https://api.openai.com/v1")
        );

        let messages: Vec<serde_json::Value> = request.messages.iter().map(|m| {
            serde_json::json!({
                "role": match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                },
                "content": m.content
            })
        }).collect();

        let mut body = serde_json::json!({
            "model": self.config.model,
            "messages": messages,
        });

        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if request.json_schema.is_some() {
            body["response_format"] = serde_json::json!({"type": "json_object"});
        }

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LLMError::Network(e.to_string()))?;

        if response.status() == 429 {
            let retry_after = response.headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok())
                .unwrap_or(60);
            return Err(LLMError::RateLimited { retry_after_ms: retry_after * 1000 });
        }

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(LLMError::Api(format!("API error: {}", error_text)));
        }

        let data: serde_json::Value = response.json().await
            .map_err(|e| LLMError::InvalidResponse(e.to_string()))?;

        let content = data["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let finish_reason = match data["choices"][0]["finish_reason"].as_str() {
            Some("stop") => FinishReason::Stop,
            Some("length") => FinishReason::Length,
            Some("content_filter") => FinishReason::ContentFilter,
            _ => FinishReason::Stop,
        };

        Ok(CompletionResponse {
            content,
            finish_reason,
            usage: Usage {
                prompt_tokens: data["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as usize,
                completion_tokens: data["usage"]["completion_tokens"].as_u64().unwrap_or(0) as usize,
            },
            model: self.config.model.clone(),
        })
    }

    pub async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, LLMError> {
        let url = format!(
            "{}/embeddings",
            self.config.base_url.as_deref().unwrap_or("https://api.openai.com/v1")
        );

        let body = serde_json::json!({
            "model": "text-embedding-3-small",
            "input": texts,
        });

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LLMError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(LLMError::Api(format!("Embedding error: {}", error_text)));
        }

        let data: serde_json::Value = response.json().await
            .map_err(|e| LLMError::InvalidResponse(e.to_string()))?;

        let embeddings = data["data"]
            .as_array()
            .ok_or_else(|| LLMError::InvalidResponse("Missing data array".to_string()))?
            .iter()
            .map(|item| {
                item["embedding"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_f64().map(|f| f as f32))
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .collect();

        Ok(embeddings)
    }
}

// ============================================================================
// Anthropic Provider
// ============================================================================

pub struct AnthropicClient {
    client: Client,
    config: LLMConfig,
}

impl AnthropicClient {
    pub fn new(config: LLMConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to create HTTP client");
        
        Self { client, config }
    }

    pub async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, LLMError> {
        let url = "https://api.anthropic.com/v1/messages";

        // Convert messages to Anthropic format
        let system = request.messages.iter()
            .find(|m| matches!(m.role, Role::System))
            .map(|m| m.content.clone());

        let messages: Vec<serde_json::Value> = request.messages.iter()
            .filter(|m| !matches!(m.role, Role::System))
            .map(|m| {
                serde_json::json!({
                    "role": match m.role {
                        Role::User => "user",
                        Role::Assistant => "assistant",
                        Role::System => "user", // Should be filtered
                    },
                    "content": m.content
                })
            })
            .collect();

        let mut body = serde_json::json!({
            "model": self.config.model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(4096),
        });

        if let Some(sys) = system {
            body["system"] = serde_json::json!(sys);
        }
        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        let response = self.client
            .post(url)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", "2024-01-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LLMError::Network(e.to_string()))?;

        if response.status() == 429 {
            return Err(LLMError::RateLimited { retry_after_ms: 60000 });
        }

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(LLMError::Api(format!("API error: {}", error_text)));
        }

        let data: serde_json::Value = response.json().await
            .map_err(|e| LLMError::InvalidResponse(e.to_string()))?;

        let content = data["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let finish_reason = match data["stop_reason"].as_str() {
            Some("end_turn") => FinishReason::Stop,
            Some("max_tokens") => FinishReason::Length,
            _ => FinishReason::Stop,
        };

        Ok(CompletionResponse {
            content,
            finish_reason,
            usage: Usage {
                prompt_tokens: data["usage"]["input_tokens"].as_u64().unwrap_or(0) as usize,
                completion_tokens: data["usage"]["output_tokens"].as_u64().unwrap_or(0) as usize,
            },
            model: self.config.model.clone(),
        })
    }
}

// ============================================================================
// Local Provider (Ollama, vLLM, etc.)
// ============================================================================

pub struct LocalClient {
    client: Client,
    config: LLMConfig,
}

impl LocalClient {
    pub fn new(config: LLMConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to create HTTP client");
        
        Self { client, config }
    }

    pub async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, LLMError> {
        let base_url = self.config.base_url.as_deref()
            .ok_or_else(|| LLMError::Api("No base URL configured".to_string()))?;

        // Assume OpenAI-compatible API (works with vLLM, Ollama in OpenAI mode)
        let url = format!("{}/v1/chat/completions", base_url);

        let messages: Vec<serde_json::Value> = request.messages.iter().map(|m| {
            serde_json::json!({
                "role": match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                },
                "content": m.content
            })
        }).collect();

        let mut body = serde_json::json!({
            "model": self.config.model,
            "messages": messages,
        });

        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        let response = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LLMError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(LLMError::Api(format!("Local API error: {}", error_text)));
        }

        let data: serde_json::Value = response.json().await
            .map_err(|e| LLMError::InvalidResponse(e.to_string()))?;

        let content = data["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(CompletionResponse {
            content,
            finish_reason: FinishReason::Stop,
            usage: Usage::default(),
            model: self.config.model.clone(),
        })
    }
}

// ============================================================================
// Unified Client
// ============================================================================

/// Unified LLM client that dispatches to the appropriate provider
pub enum UnifiedClient {
    OpenAI(OpenAIClient),
    Anthropic(AnthropicClient),
    Local(LocalClient),
}

impl UnifiedClient {
    /// Create from configuration
    pub fn from_config(config: LLMConfig) -> Self {
        match config.provider {
            Provider::OpenAI | Provider::Azure => Self::OpenAI(OpenAIClient::new(config)),
            Provider::Anthropic => Self::Anthropic(AnthropicClient::new(config)),
            Provider::Local => Self::Local(LocalClient::new(config)),
        }
    }

    /// Create from environment
    pub fn from_env() -> Result<Self, ConfigError> {
        let config = LLMConfig::from_env()?;
        Ok(Self::from_config(config))
    }

    pub async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, LLMError> {
        match self {
            Self::OpenAI(c) => c.complete(request).await,
            Self::Anthropic(c) => c.complete(request).await,
            Self::Local(c) => c.complete(request).await,
        }
    }

    pub async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, LLMError> {
        match self {
            Self::OpenAI(c) => c.embed(texts).await,
            Self::Anthropic(_) => Err(LLMError::Api("Anthropic doesn't support embeddings".to_string())),
            Self::Local(_) => Err(LLMError::Api("Local embeddings not implemented".to_string())),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_from_env() {
        // This will fail unless env vars are set, which is expected
        let result = LLMConfig::from_env();
        // Just check it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_config_creation() {
        let config = LLMConfig::openai("test-key", "gpt-4");
        assert_eq!(config.provider, Provider::OpenAI);
        assert_eq!(config.api_key, "test-key");
        assert_eq!(config.model, "gpt-4");
    }
}

