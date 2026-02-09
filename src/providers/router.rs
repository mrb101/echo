use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc;

use super::traits::AiProvider;
use super::types::{ChatRequest, ChatResponse, ModelInfo, ProviderError, StreamEvent};
use crate::models::ProviderId;

pub struct ProviderRouter {
    providers: HashMap<ProviderId, Arc<dyn AiProvider>>,
}

impl ProviderRouter {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    pub fn register(&mut self, provider: Arc<dyn AiProvider>) {
        self.providers.insert(provider.provider_id(), provider);
    }

    pub async fn validate_credentials(
        &self,
        provider_id: &ProviderId,
        api_key: &str,
        base_url: Option<&str>,
    ) -> Result<Vec<ModelInfo>, ProviderError> {
        let provider = self.providers.get(provider_id).ok_or_else(|| {
            ProviderError::RequestFailed(format!("Unknown provider: {:?}", provider_id))
        })?;
        provider.validate_credentials(api_key, base_url).await
    }

    pub async fn send_message(
        &self,
        provider_id: &ProviderId,
        request: ChatRequest,
    ) -> Result<ChatResponse, ProviderError> {
        let provider = self.providers.get(provider_id).ok_or_else(|| {
            ProviderError::RequestFailed(format!("Unknown provider: {:?}", provider_id))
        })?;
        provider.send_message(request).await
    }

    pub async fn stream_message(
        &self,
        provider_id: &ProviderId,
        request: ChatRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), ProviderError> {
        let provider = self.providers.get(provider_id).ok_or_else(|| {
            ProviderError::RequestFailed(format!("Unknown provider: {:?}", provider_id))
        })?;
        provider.stream_message(request, tx).await
    }
}
