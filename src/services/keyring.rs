use std::sync::Arc;

use anyhow::{Context, Result};
use oo7::Keyring;

use crate::config::APP_ID;

const KEYRING_ATTR_APP: &str = "application";
const KEYRING_ATTR_REF: &str = "key-ref";

#[derive(Debug, Clone)]
pub struct KeyringService {
    keyring: Arc<Keyring>,
}

impl KeyringService {
    pub async fn new() -> Result<Self> {
        let keyring = Keyring::new()
            .await
            .context("Failed to initialize keyring")?;
        Ok(Self {
            keyring: Arc::new(keyring),
        })
    }

    pub async fn store(&self, key_ref: &str, secret: &str) -> Result<()> {
        let attributes = Self::attributes(key_ref);
        let attr_refs: Vec<(&str, &str)> =
            attributes.iter().map(|(k, v)| (*k, v.as_str())).collect();

        self.keyring
            .create_item(
                &format!("Echo API Key - {}", key_ref),
                &attr_refs,
                secret,
                true, // replace if exists
            )
            .await
            .context("Failed to store secret in keyring")?;

        Ok(())
    }

    pub async fn retrieve(&self, key_ref: &str) -> Result<Option<String>> {
        let attributes = Self::attributes(key_ref);
        let attr_refs: Vec<(&str, &str)> =
            attributes.iter().map(|(k, v)| (*k, v.as_str())).collect();

        let items = self
            .keyring
            .search_items(&attr_refs)
            .await
            .context("Failed to search keyring")?;

        if let Some(item) = items.first() {
            let secret = item.secret().await.context("Failed to read secret")?;
            let secret_str =
                String::from_utf8(secret.to_vec()).context("Secret is not valid UTF-8")?;
            Ok(Some(secret_str))
        } else {
            Ok(None)
        }
    }

    pub async fn delete(&self, key_ref: &str) -> Result<()> {
        let attributes = Self::attributes(key_ref);
        let attr_refs: Vec<(&str, &str)> =
            attributes.iter().map(|(k, v)| (*k, v.as_str())).collect();

        self.keyring
            .delete(&attr_refs)
            .await
            .context("Failed to delete secret from keyring")?;

        Ok(())
    }

    fn attributes(key_ref: &str) -> Vec<(&'static str, String)> {
        vec![
            (KEYRING_ATTR_APP, APP_ID.to_string()),
            (KEYRING_ATTR_REF, key_ref.to_string()),
        ]
    }
}
