use async_trait::async_trait;
use reqwest::Client;
use tokio::sync::mpsc;

use super::models::*;
use crate::models::{ProviderId, Role};
use crate::providers::traits::AiProvider;
use crate::providers::types::{
    ChatMessage, ChatRequest, ChatResponse, ModelInfo, Feature,
    ProviderError, StopReason, StreamEvent, ToolCall, ToolDefinition,
};

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

    fn build_messages(system_prompt: Option<&str>, messages: &[ChatMessage]) -> Vec<OpenAiMessage> {
        let mut result = Vec::new();

        if let Some(prompt) = system_prompt {
            if !prompt.is_empty() {
                result.push(OpenAiMessage {
                    role: "system".to_string(),
                    content: Some(prompt.to_string()),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }
        }

        for msg in messages {
            // Tool results: emit as role="tool" messages
            if !msg.tool_results.is_empty() {
                for tr in &msg.tool_results {
                    result.push(OpenAiMessage {
                        role: "tool".to_string(),
                        content: Some(tr.content.clone()),
                        tool_calls: None,
                        tool_call_id: Some(tr.call_id.clone()),
                    });
                }
                continue;
            }

            // Assistant message with tool calls
            if !msg.tool_calls.is_empty() {
                let tool_calls: Vec<OpenAiToolCall> = msg
                    .tool_calls
                    .iter()
                    .map(|tc| OpenAiToolCall {
                        id: tc.id.clone(),
                        call_type: "function".to_string(),
                        function: OpenAiToolCallFunction {
                            name: tc.name.clone(),
                            arguments: serde_json::to_string(&tc.arguments).unwrap_or_default(),
                        },
                    })
                    .collect();
                result.push(OpenAiMessage {
                    role: Self::translate_role(&msg.role).to_string(),
                    content: if msg.content.is_empty() {
                        None
                    } else {
                        Some(msg.content.clone())
                    },
                    tool_calls: Some(tool_calls),
                    tool_call_id: None,
                });
                continue;
            }

            result.push(OpenAiMessage {
                role: Self::translate_role(&msg.role).to_string(),
                content: Some(msg.content.clone()),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        result
    }

    fn convert_tools(tools: &[ToolDefinition]) -> Option<Vec<OpenAiTool>> {
        if tools.is_empty() {
            None
        } else {
            Some(
                tools
                    .iter()
                    .map(|t| OpenAiTool {
                        tool_type: "function".to_string(),
                        function: OpenAiFunction {
                            name: t.name.clone(),
                            description: t.description.clone(),
                            parameters: t.parameters.clone(),
                        },
                    })
                    .collect(),
            )
        }
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
            ProviderError::RequestFailed("Base URL is required for Local provider".to_string())
        })?;

        let url = format!("{}/v1/models", base.trim_end_matches('/'));

        let mut req = self.client.get(&url);
        if let Some(auth) = Self::build_auth_header(api_key) {
            req = req.header("Authorization", auth);
        }

        let response = req.send().await.map_err(|e| {
            ProviderError::NetworkError(format!("Failed to connect to {}: {}", base, e))
        })?;

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

        let model_list: OpenAiModelList = response.json().await.map_err(|e| {
            ProviderError::InvalidResponse(format!("Failed to parse model list: {}", e))
        })?;

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
            ProviderError::RequestFailed("Base URL is required for Local provider".to_string())
        })?;

        let url = format!("{}/v1/chat/completions", base.trim_end_matches('/'));

        let messages = Self::build_messages(request.system_prompt.as_deref(), &request.messages);

        let openai_request = OpenAiRequest {
            model: request.model.clone(),
            messages,
            stream: false,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            tools: Self::convert_tools(&request.tools),
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
            return Err(ProviderError::RequestFailed(Self::parse_error_message(
                status, &body,
            )));
        }

        let openai_response: OpenAiResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        let choice = openai_response.choices.first();
        let content = choice
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        let tool_calls: Vec<ToolCall> = choice
            .and_then(|c| c.message.tool_calls.as_ref())
            .map(|tcs| {
                tcs.iter()
                    .filter_map(|tc| {
                        let args: serde_json::Value =
                            serde_json::from_str(&tc.function.arguments).ok()?;
                        Some(ToolCall {
                            id: tc.id.clone(),
                            name: tc.function.name.clone(),
                            arguments: args,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let stop_reason = match choice.and_then(|c| c.finish_reason.as_deref()) {
            Some("tool_calls") => Some(StopReason::ToolUse),
            Some("length") => Some(StopReason::MaxTokens),
            _ if !tool_calls.is_empty() => Some(StopReason::ToolUse),
            _ => Some(StopReason::EndTurn),
        };

        if content.is_empty() && tool_calls.is_empty() {
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
            tool_calls,
            stop_reason,
        })
    }

    async fn stream_message(
        &self,
        request: ChatRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), ProviderError> {
        use super::stream::parse_sse_stream;

        let base = request.base_url.as_deref().ok_or_else(|| {
            ProviderError::RequestFailed("Base URL is required for Local provider".to_string())
        })?;

        let url = format!("{}/v1/chat/completions", base.trim_end_matches('/'));

        let messages = Self::build_messages(request.system_prompt.as_deref(), &request.messages);

        let openai_request = OpenAiRequest {
            model: request.model.clone(),
            messages,
            stream: true,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            tools: Self::convert_tools(&request.tools),
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
            return Err(ProviderError::RequestFailed(Self::parse_error_message(
                status, &body,
            )));
        }

        parse_sse_stream(response, tx).await;

        Ok(())
    }
}
