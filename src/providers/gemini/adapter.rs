use async_trait::async_trait;
use base64::Engine;
use reqwest::Client;
use tokio::sync::mpsc;

use super::models::*;
use crate::models::{ProviderId, Role};
use crate::providers::traits::AiProvider;
use crate::providers::types::*;

const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

pub struct GeminiProvider {
    client: Client,
}

impl GeminiProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    fn base_url(custom: Option<&str>) -> &str {
        custom.unwrap_or(DEFAULT_BASE_URL)
    }

    /// Parse an API error response body into a user-friendly message.
    fn parse_error_message(status: reqwest::StatusCode, body: &str) -> String {
        // Try to extract a message from Gemini's JSON error format
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(body) {
            if let Some(msg) = parsed["error"]["message"].as_str() {
                return format!("HTTP {}: {}", status.as_u16(), msg);
            }
        }
        format!("HTTP {}: Request failed", status.as_u16())
    }

    fn translate_role(role: &Role) -> &'static str {
        match role {
            Role::User => "user",
            Role::Assistant => "model",
        }
    }

    fn build_contents(messages: &[ChatMessage]) -> Vec<GeminiContent> {
        messages
            .iter()
            .map(|msg| {
                let mut parts = Vec::new();

                // Add image parts first
                for img in &msg.images {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&img.data);
                    parts.push(GeminiPart {
                        text: None,
                        inline_data: Some(GeminiInlineData {
                            mime_type: img.mime_type.clone(),
                            data: b64,
                        }),
                    });
                }

                // Add text part
                parts.push(GeminiPart {
                    text: Some(msg.content.clone()),
                    inline_data: None,
                });

                GeminiContent {
                    role: Self::translate_role(&msg.role).to_string(),
                    parts,
                }
            })
            .collect()
    }
}

#[async_trait]
impl AiProvider for GeminiProvider {
    fn provider_id(&self) -> ProviderId {
        ProviderId::Gemini
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
            .header("x-goog-api-key", api_key)
            .send()
            .await
            .map_err(|e| ProviderError::NetworkError(e.to_string()))?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            return Err(ProviderError::AuthError("Invalid API key".to_string()));
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::RequestFailed(Self::parse_error_message(
                status, &body,
            )));
        }

        let models_response: GeminiModelsResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        let models = models_response
            .models
            .into_iter()
            .filter(|m| {
                m.supported_generation_methods
                    .as_ref()
                    .is_some_and(|methods| methods.iter().any(|m| m == "generateContent"))
            })
            .map(|m| {
                let name = m.name.strip_prefix("models/").unwrap_or(&m.name);
                ModelInfo {
                    id: name.to_string(),
                    name: m.display_name.unwrap_or_else(|| name.to_string()),
                    features: vec![Feature::Chat],
                }
            })
            .collect();

        Ok(models)
    }

    async fn send_message(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let base = Self::base_url(request.base_url.as_deref());
        let url = format!("{}/models/{}:generateContent", base, request.model);

        let contents = Self::build_contents(&request.messages);

        let generation_config = request.temperature.map(|t| GeminiGenerationConfig {
            temperature: Some(t),
        });

        let system_instruction = request.system_prompt.as_ref().map(|prompt| GeminiContent {
            role: "user".to_string(),
            parts: vec![GeminiPart {
                text: Some(prompt.clone()),
                inline_data: None,
            }],
        });

        let gemini_request = GeminiRequest {
            contents,
            system_instruction,
            generation_config,
        };

        let response = self
            .client
            .post(&url)
            .header("x-goog-api-key", &request.api_key)
            .json(&gemini_request)
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

        let gemini_response: GeminiResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        if let Some(error) = gemini_response.error {
            return Err(ProviderError::RequestFailed(
                error.message.unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        let content = gemini_response
            .candidates
            .and_then(|c| c.into_iter().next())
            .and_then(|c| c.content)
            .and_then(|c| c.parts.into_iter().filter_map(|p| p.text).next())
            .ok_or_else(|| ProviderError::InvalidResponse("No content in response".to_string()))?;

        let (tokens_in, tokens_out) = gemini_response
            .usage_metadata
            .map(|u| (u.prompt_token_count, u.candidates_token_count))
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
        let url = format!(
            "{}/models/{}:streamGenerateContent?alt=sse",
            base, request.model
        );

        let contents = Self::build_contents(&request.messages);

        let generation_config = request.temperature.map(|t| GeminiGenerationConfig {
            temperature: Some(t),
        });

        let system_instruction = request.system_prompt.as_ref().map(|prompt| GeminiContent {
            role: "user".to_string(),
            parts: vec![GeminiPart {
                text: Some(prompt.clone()),
                inline_data: None,
            }],
        });

        let gemini_request = GeminiRequest {
            contents,
            system_instruction,
            generation_config,
        };

        let response = self
            .client
            .post(&url)
            .header("x-goog-api-key", &request.api_key)
            .json(&gemini_request)
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
