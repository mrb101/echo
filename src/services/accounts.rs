use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use uuid::Uuid;

use crate::models::{Account, AccountStatus, ProviderId};
use crate::providers::ProviderRouter;
use crate::services::database::Database;
use crate::services::keyring::KeyringService;

pub struct AccountService {
    db: Database,
    keyring: KeyringService,
    router: Arc<ProviderRouter>,
}

impl AccountService {
    pub fn new(db: Database, keyring: KeyringService, router: Arc<ProviderRouter>) -> Self {
        Self {
            db,
            keyring,
            router,
        }
    }

    pub fn keyring_clone(&self) -> KeyringService {
        self.keyring.clone()
    }

    pub async fn add_account(
        &self,
        provider: ProviderId,
        label: String,
        api_key: String,
        base_url: Option<String>,
        default_model: String,
        set_as_default: bool,
    ) -> Result<Account> {
        // Validate credentials with the provider
        self.router
            .validate_credentials(&provider, &api_key, base_url.as_deref())
            .await
            .context("Failed to validate credentials")?;

        let now = Utc::now();
        let account_id = Uuid::new_v4().to_string();
        let key_ref = format!("{}:{}", provider.as_str(), &account_id);

        // Store API key in keyring
        self.keyring
            .store(&key_ref, &api_key)
            .await
            .context("Failed to store API key in keyring")?;

        let account = Account {
            id: account_id,
            provider,
            label,
            api_base_url: base_url,
            default_model,
            is_default: set_as_default,
            status: AccountStatus::Active,
            total_tokens_in: 0,
            total_tokens_out: 0,
            created_at: now,
            updated_at: now,
        };

        self.db
            .insert_account(&account)
            .await
            .context("Failed to save account to database")?;

        // Now set as default if requested (after insert so the account exists)
        if set_as_default {
            self.db
                .set_default_account(&account.id, provider)
                .await
                .context("Failed to set account as default")?;
        }

        Ok(account)
    }

    pub async fn get_account_with_key(&self, account_id: &str) -> Result<(Account, String)> {
        let account = self
            .db
            .get_account(account_id)
            .await?
            .context("Account not found")?;

        let key_ref = format!("{}:{}", account.provider.as_str(), account_id);
        let api_key = self
            .keyring
            .retrieve(&key_ref)
            .await?
            .context("API key not found in keyring")?;

        Ok((account, api_key))
    }

    pub async fn delete_account(&self, account_id: &str) -> Result<()> {
        let account = self
            .db
            .get_account(account_id)
            .await?
            .context("Account not found")?;

        let key_ref = format!("{}:{}", account.provider.as_str(), account_id);

        // Delete from keyring (ignore errors if key doesn't exist)
        let _ = self.keyring.delete(&key_ref).await;

        // Delete from database (cascades to conversations/messages)
        self.db
            .delete_account(account_id)
            .await
            .context("Failed to delete account from database")?;

        Ok(())
    }

}
