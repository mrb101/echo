use anyhow::{bail, Result};

use crate::models::{Message, Role};
use crate::services::database::Database;

/// Load messages for a conversation with attachments populated for user messages.
pub async fn load_messages_with_attachments(
    db: &Database,
    conversation_id: &str,
) -> Result<Vec<Message>> {
    let mut messages = db.list_messages(conversation_id).await?;
    for msg in &mut messages {
        if msg.role == Role::User {
            if let Ok(atts) = db.list_attachments(&msg.id).await {
                if !atts.is_empty() {
                    msg.attachments = atts;
                }
            }
        }
    }
    Ok(messages)
}

/// Prepare for message regeneration: deactivate the target assistant message
/// and everything after it, then return the remaining active messages.
pub async fn prepare_regeneration(
    db: &Database,
    conversation_id: &str,
    assistant_msg_id: &str,
) -> Result<Vec<Message>> {
    let messages = db.list_messages(conversation_id).await?;

    let assistant_idx = messages
        .iter()
        .position(|m| m.id == assistant_msg_id)
        .ok_or_else(|| anyhow::anyhow!("Message not found"))?;

    let user_msg = messages[..assistant_idx]
        .iter()
        .rev()
        .find(|m| m.role == Role::User);

    let cutoff_ts = match user_msg {
        Some(m) => m.created_at.to_rfc3339(),
        None => bail!("No preceding user message found"),
    };

    if let Err(e) = db
        .deactivate_messages_after(conversation_id, &cutoff_ts)
        .await
    {
        tracing::error!("Failed to deactivate messages: {}", e);
    }

    load_messages_with_attachments(db, conversation_id).await
}

/// Prepare for message editing: update content, deactivate messages after
/// the edited one, then return remaining active messages.
pub async fn prepare_edit(
    db: &Database,
    conversation_id: &str,
    msg_id: &str,
    new_content: &str,
) -> Result<Vec<Message>> {
    db.update_message_content(msg_id, new_content).await?;

    let messages = db.list_messages(conversation_id).await?;

    if let Some(edited) = messages.iter().find(|m| m.id == msg_id) {
        let after_ts = edited.created_at.to_rfc3339();
        if let Err(e) = db
            .deactivate_messages_after(conversation_id, &after_ts)
            .await
        {
            tracing::error!("Failed to deactivate messages: {}", e);
        }
    }

    load_messages_with_attachments(db, conversation_id).await
}

/// Truncate text to a short title for conversations.
pub fn truncate_title(text: &str) -> String {
    let first_line = text.lines().next().unwrap_or(text);
    if first_line.len() > 50 {
        let boundary = first_line
            .char_indices()
            .take_while(|(i, _)| *i < 47)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(47);
        format!("{}...", &first_line[..boundary])
    } else {
        first_line.to_string()
    }
}
