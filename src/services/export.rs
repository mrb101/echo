use crate::models::{Conversation, Message, Role};

pub fn export_to_markdown(conversation: &Conversation, messages: &[Message]) -> String {
    let mut output = format!("# {}\n\n", conversation.title);
    output.push_str(&format!(
        "> Model: {} | Date: {}\n\n",
        conversation.model,
        conversation.created_at.format("%Y-%m-%d %H:%M")
    ));

    if let Some(prompt) = &conversation.system_prompt {
        output.push_str(&format!("> System Prompt: {}\n\n", prompt));
    }

    output.push_str("---\n\n");

    for msg in messages {
        let role_label = match msg.role {
            Role::User => "You",
            Role::Assistant => msg.model.as_deref().unwrap_or("Assistant"),
        };
        output.push_str(&format!("### {}\n\n{}\n\n", role_label, msg.content));
    }

    output
}
