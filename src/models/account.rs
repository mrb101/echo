use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderId {
    Gemini,
    Claude,
    Local,
}

impl ProviderId {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderId::Gemini => "gemini",
            ProviderId::Claude => "claude",
            ProviderId::Local => "local",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ProviderId::Gemini => "Google Gemini",
            ProviderId::Claude => "Anthropic Claude",
            ProviderId::Local => "Local (OpenAI Compatible)",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "gemini" => Some(ProviderId::Gemini),
            "claude" => Some(ProviderId::Claude),
            "local" => Some(ProviderId::Local),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountStatus {
    Active,
    Invalid,
}

impl AccountStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccountStatus::Active => "active",
            AccountStatus::Invalid => "invalid",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "active" => Some(AccountStatus::Active),
            "invalid" => Some(AccountStatus::Invalid),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub provider: ProviderId,
    pub label: String,
    pub api_base_url: Option<String>,
    pub default_model: String,
    pub is_default: bool,
    pub status: AccountStatus,
    pub total_tokens_in: i64,
    pub total_tokens_out: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
