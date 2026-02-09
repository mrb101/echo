use async_trait::async_trait;
use tokio::sync::mpsc;

use super::types::{ChatRequest, ChatResponse, ModelInfo, ProviderError, StreamEvent};
use crate::models::ProviderId;

#[async_trait]
pub trait AiProvider: Send + Sync {
    fn provider_id(&self) -> ProviderId;

    async fn validate_credentials(&self, api_key: &str, base_url: Option<&str>)
        -> Result<Vec<ModelInfo>, ProviderError>;

    async fn send_message(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError>;

    async fn stream_message(
        &self,
        request: ChatRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), ProviderError>;
}
