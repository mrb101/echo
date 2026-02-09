use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub message_id: String,
    pub mime_type: String,
    pub filename: Option<String>,
    pub data: Vec<u8>,
    pub created_at: DateTime<Utc>,
}
