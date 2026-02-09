use async_trait::async_trait;
use base64::Engine;
use reqwest::Client;
use tokio::sync::mpsc;

use super::models::*;
use crate::models::{ProviderId, Role};
use crate::providers::traits::AiProvider;
use crate::providers::types::*;

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com/v1";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: u32 = 8192;

const FALLBACK_MODELS: &[(&str, &str)] = &[
    ("claude-opus-4-0-20250514", "Claude Opus 4"),
    ("claude-sonnet-4-5-20250929", "Claude Sonnet 4.5"),
    ("claude-sonnet-4-0-20250514", "Claude Sonnet 4"),
    ("claude-haiku-3-5-20241022", "Claude Haiku 3.5"),
];

pub struct ClaudeProvider {
    client: Client,
}

impl ClaudeProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    fn base_url(custom: Option<&str>) -> &str {
        custom.unwrap_or(DEFAULT_BASE_URL)
    }

    fn parse_error_message(status: reqwest::StatusCode, body: &str) -> String {
        if let Ok(parsed) = serde_json::from_str::<ClaudeErrorResponse>(body) {
            return format!("HTTP {}: {}", status.as_u16(), parsed.error.message);
        }
        format!("HTTP {}: Request failed", status.as_u16())
    }

    fn translate_role(role: &Role) -> &'static str {
        match role {
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }

    fn build_messages(messages: &[ChatMessage]) -> Vec<ClaudeMessage> {
        messages
            .iter()
            .map(|msg| {
                if msg.images.is_empty() {
                    // Text-only: use simple string content
                    ClaudeMessage {
                        role: Self::translate_role(&msg.role).to_string(),
                        content: ClaudeContent::Text(msg.content.clone()),
                    }
                } else {
                    // Has images: use content block array
                    let mut blocks = Vec::new();

                    for img in &msg.images {
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&img.data);
                        blocks.push(ClaudeContentBlock::Image {
                            source: ClaudeImageSource {
                                source_type: "base64".to_string(),
                                media_type: img.mime_type.clone(),
                                data: b64,
                            },
                        });
                    }

                    blocks.push(ClaudeContentBlock::Text {
                        text: msg.content.clone(),
                    });

                    ClaudeMessage {
                        role: Self::translate_role(&msg.role).to_string(),
                        content: ClaudeContent::Blocks(blocks),
                    }
                }
            })
            .collect()
    }

    fn max_tokens(request: &ChatRequest) -> u32 {
        request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS)
    }

    fn fallback_models() -> Vec<ModelInfo> {
        FALLBACK_MODELS
            .iter()
            .map(|(id, name)| ModelInfo {
                id: id.to_string(),
                name: name.to_string(),
                features: vec![Feature::Chat, Feature::Vision],
            })
            .collect()
    }
}

#[async_trait]
impl AiProvider for ClaudeProvider {
    fn provider_id(&self) -> ProviderId {
        ProviderId::Claude
    }

    async fn validate_credentials(
        &self,
        api_key: &str,
        base_url: Option<&str>,
    ) -> Result<Vec<ModelInfo>, ProviderError> {
        let url = format!("{}/models", Self::base_url(base_url));

        let response = self
            .client
            .get(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .send()
            .await
            .map_err(|e| ProviderError::NetworkError(e.to_string()))?;

        let status = response.status();

        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(ProviderError::AuthError("Invalid API key".to_string()));
        }

        if status.is_success() {
            let models_response: ClaudeModelsResponse = response
                .json()
                .await
                .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

            let models: Vec<ModelInfo> = models_response
                .data
                .into_iter()
                .map(|m| {
                    let name = if m.display_name.is_empty() {
                        m.id.clone()
                    } else {
                        m.display_name
                    };
                    ModelInfo {
                        id: m.id,
                        name,
                        features: vec![Feature::Chat, Feature::Vision],
                    }
                })
                .collect();

            if models.is_empty() {
                return Ok(Self::fallback_models());
            }

            return Ok(models);
        }

        // Non-auth failure: check for auth-related error messages in body
        let body = response.text().await.unwrap_or_default();
        if body.contains("authentication") || body.contains("api_key") {
            return Err(ProviderError::AuthError(Self::parse_error_message(
                status, &body,
            )));
        }

        // Key is likely valid but models endpoint failed — use fallback
        Ok(Self::fallback_models())
    }

    async fn send_message(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let base = Self::base_url(request.base_url.as_deref());
        let url = format!("{}/messages", base);

        let messages = Self::build_messages(&request.messages);

        let claude_request = ClaudeRequest {
            model: request.model.clone(),
            max_tokens: Self::max_tokens(&request),
            messages,
            system: request.system_prompt.clone(),
            temperature: request.temperature,
            stream: None,
        };

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &request.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&claude_request)
            .send()
            .await
            .map_err(|e| ProviderError::NetworkError(e.to_string()))?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            return Err(ProviderError::AuthError("Invalid API key".to_string()));
        }

        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimited {
                retry_after_secs: None,
            });
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::RequestFailed(Self::parse_error_message(
                status, &body,
            )));
        }

        let claude_response: ClaudeResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        let content = claude_response
            .content
            .into_iter()
            .map(|block| match block {
                ClaudeResponseBlock::Text { text } => text,
            })
            .collect::<Vec<_>>()
            .join("");

        if content.is_empty() {
            return Err(ProviderError::InvalidResponse(
                "No content in response".to_string(),
            ));
        }

        let (tokens_in, tokens_out) = claude_response
            .usage
            .map(|u| (u.input_tokens, u.output_tokens))
            .unwrap_or((None, None));

        Ok(ChatResponse {
            content,
            model: request.model,
            tokens_in,
            tokens_out,
        })
    }

    async fn stream_message(
        &self,
        request: ChatRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), ProviderError> {
        use super::stream::parse_sse_stream;

        let base = Self::base_url(request.base_url.as_deref());
        let url = format!("{}/messages", base);

        let messages = Self::build_messages(&request.messages);

        let claude_request = ClaudeRequest {
            model: request.model.clone(),
            max_tokens: Self::max_tokens(&request),
            messages,
            system: request.system_prompt.clone(),
            temperature: request.temperature,
            stream: Some(true),
        };

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &request.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&claude_request)
            .send()
            .await
            .map_err(|e| ProviderError::NetworkError(e.to_string()))?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            return Err(ProviderError::AuthError("Invalid API key".to_string()));
        }

        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimited {
                retry_after_secs: None,
            });
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::RequestFailed(Self::parse_error_message(
                status, &body,
            )));
        }

        parse_sse_stream(response, tx).await;

        Ok(())
    }
}
