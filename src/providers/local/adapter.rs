use async_trait::async_trait;
use reqwest::Client;
use tokio::sync::mpsc;

use super::models::*;
use crate::models::{ProviderId, Role};
use crate::providers::traits::AiProvider;
use crate::providers::types::*;

pub struct LocalProvider {
    client: Client,
}

impl LocalProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    fn translate_role(role: &Role) -> &'static str {
        match role {
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }

    fn build_messages(
        system_prompt: Option<&str>,
        messages: &[ChatMessage],
    ) -> Vec<OpenAiMessage> {
        let mut result = Vec::new();

        if let Some(prompt) = system_prompt {
            if !prompt.is_empty() {
                result.push(OpenAiMessage {
                    role: "system".to_string(),
                    content: prompt.to_string(),
                });
            }
        }

        for msg in messages {
            result.push(OpenAiMessage {
                role: Self::translate_role(&msg.role).to_string(),
                content: msg.content.clone(),
            });
        }

        result
    }

    fn build_auth_header(api_key: &str) -> Option<String> {
        if api_key.is_empty() {
            None
        } else {
            Some(format!("Bearer {}", api_key))
        }
    }

    fn parse_error_message(status: reqwest::StatusCode, body: &str) -> String {
        if let Ok(parsed) = serde_json::from_str::<OpenAiErrorResponse>(body) {
            return format!("HTTP {}: {}", status.as_u16(), parsed.error.message);
        }
        format!("HTTP {}: Request failed", status.as_u16())
    }
}

#[async_trait]
impl AiProvider for LocalProvider {
    fn provider_id(&self) -> ProviderId {
        ProviderId::Local
    }

    async fn validate_credentials(
        &self,
        api_key: &str,
        base_url: Option<&str>,
    ) -> Result<Vec<ModelInfo>, ProviderError> {
        let base = base_url.ok_or_else(|| {
            ProviderError::RequestFailed(
                "Base URL is required for Local provider".to_string(),
            )
        })?;

        let url = format!("{}/v1/models", base.trim_end_matches('/'));

        let mut req = self.client.get(&url);
        if let Some(auth) = Self::build_auth_header(api_key) {
            req = req.header("Authorization", auth);
        }

        let response = req
            .send()
            .await
            .map_err(|e| ProviderError::NetworkError(format!("Failed to connect to {}: {}", base, e)))?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            return Err(ProviderError::AuthError("Invalid API key".to_string()));
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::RequestFailed(
                Self::parse_error_message(status, &body),
            ));
        }

        let model_list: OpenAiModelList = response
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(format!("Failed to parse model list: {}", e)))?;

        let models = model_list
            .data
            .into_iter()
            .map(|m| ModelInfo {
                id: m.id.clone(),
                name: m.id,
                features: vec![Feature::Chat, Feature::Streaming],
            })
            .collect();

        Ok(models)
    }

    async fn send_message(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let base = request.base_url.as_deref().ok_or_else(|| {
            ProviderError::RequestFailed(
                "Base URL is required for Local provider".to_string(),
            )
        })?;

        let url = format!("{}/v1/chat/completions", base.trim_end_matches('/'));

        let messages = Self::build_messages(request.system_prompt.as_deref(), &request.messages);

        let openai_request = OpenAiRequest {
            model: request.model.clone(),
            messages,
            stream: false,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
        };

        let mut req = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .json(&openai_request);

        if let Some(auth) = Self::build_auth_header(&request.api_key) {
            req = req.header("Authorization", auth);
        }

        let response = req
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
            return Err(ProviderError::RequestFailed(
                Self::parse_error_message(status, &body),
            ));
        }

        let openai_response: OpenAiResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        let content = openai_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        if content.is_empty() {
            return Err(ProviderError::InvalidResponse(
                "No content in response".to_string(),
            ));
        }

        let (tokens_in, tokens_out) = openai_response
            .usage
            .map(|u| (u.prompt_tokens, u.completion_tokens))
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

        let base = request.base_url.as_deref().ok_or_else(|| {
            ProviderError::RequestFailed(
                "Base URL is required for Local provider".to_string(),
            )
        })?;

        let url = format!("{}/v1/chat/completions", base.trim_end_matches('/'));

        let messages = Self::build_messages(request.system_prompt.as_deref(), &request.messages);

        let openai_request = OpenAiRequest {
            model: request.model.clone(),
            messages,
            stream: true,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
        };

        let mut req = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .json(&openai_request);

        if let Some(auth) = Self::build_auth_header(&request.api_key) {
            req = req.header("Authorization", auth);
        }

        let response = req
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
            return Err(ProviderError::RequestFailed(
                Self::parse_error_message(status, &body),
            ));
        }

        parse_sse_stream(response, tx).await;

        Ok(())
    }
}
