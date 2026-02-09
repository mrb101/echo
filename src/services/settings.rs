use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::database::Database;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub stream_responses: bool,
    pub temperature: f32,
    pub send_with_enter: bool,
    pub color_scheme: ColorScheme,
    pub message_font_size: u32,
    pub code_font_size: u32,
    pub message_spacing: MessageSpacing,
    #[serde(default)]
    pub default_system_prompt: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorScheme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageSpacing {
    Compact,
    Comfortable,
    Spacious,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            stream_responses: true,
            temperature: 1.0,
            send_with_enter: true,
            color_scheme: ColorScheme::System,
            message_font_size: 14,
            code_font_size: 13,
            message_spacing: MessageSpacing::Comfortable,
            default_system_prompt: None,
        }
    }
}

pub struct SettingsService;

impl SettingsService {
    pub async fn load(db: &Database) -> AppSettings {
        match db.get_setting("app_settings").await {
            Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_default(),
            _ => AppSettings::default(),
        }
    }

    pub async fn save(db: &Database, settings: &AppSettings) -> Result<()> {
        let json = serde_json::to_string(settings)?;
        db.set_setting("app_settings", &json).await
    }
}
