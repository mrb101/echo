use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use tokio::task;

use crate::models::{Account, AccountStatus, Attachment, Conversation, Message, ProviderId, Role};

#[derive(Debug, Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn conn_ref(&self) -> &Arc<Mutex<Connection>> {
        &self.conn
    }

    pub fn row_to_account_pub(row: &rusqlite::Row) -> Result<Account> {
        Self::row_to_account(row)
    }

    pub async fn new() -> Result<Self> {
        let path = Self::db_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create data directory: {}", parent.display()))?;
        }

        let conn = Connection::open(&path)
            .with_context(|| format!("Failed to open database at {}", path.display()))?;

        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

        let db = Database {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.run_migrations()?;

        Ok(db)
    }

    /// Create an in-memory database (used for testing and as placeholder)
    pub fn new_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let db = Database {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.run_migrations()?;
        Ok(db)
    }

    fn db_path() -> Result<PathBuf> {
        let data_dir = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").expect("HOME not set");
                PathBuf::from(home).join(".local/share")
            });
        Ok(data_dir.join("echo").join("echo.db"))
    }

    fn run_migrations(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER NOT NULL
            );",
        )?;

        let version: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if version < 1 {
            conn.execute_batch(
                "CREATE TABLE accounts (
                    id TEXT PRIMARY KEY,
                    provider TEXT NOT NULL,
                    label TEXT NOT NULL,
                    api_base_url TEXT,
                    default_model TEXT NOT NULL,
                    is_default INTEGER NOT NULL DEFAULT 0,
                    status TEXT NOT NULL DEFAULT 'active',
                    total_tokens_in BIGINT NOT NULL DEFAULT 0,
                    total_tokens_out BIGINT NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE conversations (
                    id TEXT PRIMARY KEY,
                    account_id TEXT NOT NULL,
                    title TEXT NOT NULL,
                    model TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
                );

                CREATE TABLE messages (
                    id TEXT PRIMARY KEY,
                    conversation_id TEXT NOT NULL,
                    role TEXT NOT NULL,
                    content TEXT NOT NULL,
                    model TEXT,
                    tokens_in BIGINT,
                    tokens_out BIGINT,
                    created_at TEXT NOT NULL,
                    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
                );

                CREATE INDEX idx_conversations_account ON conversations(account_id);
                CREATE INDEX idx_conversations_updated ON conversations(updated_at DESC);
                CREATE INDEX idx_messages_conversation ON messages(conversation_id);
                CREATE INDEX idx_messages_created ON messages(created_at);

                INSERT INTO schema_version (version) VALUES (1);",
            )?;
        }

        if version < 2 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS settings (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );

                UPDATE schema_version SET version = 2;",
            )?;
        }

        if version < 3 {
            conn.execute_batch(
                "ALTER TABLE conversations ADD COLUMN system_prompt TEXT;
                 ALTER TABLE messages ADD COLUMN parent_message_id TEXT;
                 ALTER TABLE messages ADD COLUMN is_active INTEGER NOT NULL DEFAULT 1;
                 CREATE INDEX idx_messages_active ON messages(conversation_id, is_active);

                 UPDATE schema_version SET version = 3;",
            )?;
        }

        if version < 4 {
            conn.execute_batch(
                "CREATE TABLE message_attachments (
                    id TEXT PRIMARY KEY,
                    message_id TEXT NOT NULL,
                    mime_type TEXT NOT NULL,
                    filename TEXT,
                    data BLOB NOT NULL,
                    created_at TEXT NOT NULL,
                    FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
                );
                CREATE INDEX idx_attachments_message ON message_attachments(message_id);

                UPDATE schema_version SET version = 4;",
            )?;
        }

        if version < 5 {
            conn.execute_batch(
                "ALTER TABLE conversations ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;

                 UPDATE schema_version SET version = 5;",
            )?;
        }

        Ok(())
    }

    // --- Account CRUD ---

    pub async fn insert_account(&self, account: &Account) -> Result<()> {
        let conn = self.conn.clone();
        let account = account.clone();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            conn.execute(
                "INSERT INTO accounts (id, provider, label, api_base_url, default_model, is_default, status, total_tokens_in, total_tokens_out, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    account.id,
                    account.provider.as_str(),
                    account.label,
                    account.api_base_url,
                    account.default_model,
                    account.is_default as i32,
                    account.status.as_str(),
                    account.total_tokens_in,
                    account.total_tokens_out,
                    account.created_at.to_rfc3339(),
                    account.updated_at.to_rfc3339(),
                ],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn get_account(&self, id: &str) -> Result<Option<Account>> {
        let conn = self.conn.clone();
        let id = id.to_string();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT id, provider, label, api_base_url, default_model, is_default, status, total_tokens_in, total_tokens_out, created_at, updated_at
                 FROM accounts WHERE id = ?1",
            )?;
            let result = stmt.query_row(params![id], |row| Ok(Self::row_to_account(row))).optional()?;
            match result {
                Some(Ok(account)) => Ok(Some(account)),
                Some(Err(e)) => Err(e),
                None => Ok(None),
            }
        })
        .await?
    }

    pub async fn list_accounts(&self) -> Result<Vec<Account>> {
        let conn = self.conn.clone();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT id, provider, label, api_base_url, default_model, is_default, status, total_tokens_in, total_tokens_out, created_at, updated_at
                 FROM accounts ORDER BY provider, label",
            )?;
            let accounts = stmt
                .query_map([], |row| Ok(Self::row_to_account(row)))?
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?;
            Ok(accounts)
        })
        .await?
    }

    pub async fn delete_account(&self, id: &str) -> Result<()> {
        let conn = self.conn.clone();
        let id = id.to_string();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            conn.execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
            Ok(())
        })
        .await?
    }

    pub async fn set_default_account(&self, id: &str, provider: ProviderId) -> Result<()> {
        let conn = self.conn.clone();
        let id = id.to_string();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            conn.execute(
                "UPDATE accounts SET is_default = 0 WHERE provider = ?1",
                params![provider.as_str()],
            )?;
            conn.execute(
                "UPDATE accounts SET is_default = 1 WHERE id = ?1",
                params![id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn update_account_usage(
        &self,
        id: &str,
        tokens_in: i64,
        tokens_out: i64,
    ) -> Result<()> {
        let conn = self.conn.clone();
        let id = id.to_string();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            conn.execute(
                "UPDATE accounts SET total_tokens_in = total_tokens_in + ?1, total_tokens_out = total_tokens_out + ?2, updated_at = ?3 WHERE id = ?4",
                params![tokens_in, tokens_out, Utc::now().to_rfc3339(), id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn has_any_accounts(&self) -> Result<bool> {
        let conn = self.conn.clone();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM accounts", [], |row| row.get(0))?;
            Ok(count > 0)
        })
        .await?
    }

    // --- Conversation CRUD ---

    pub async fn insert_conversation(&self, conversation: &Conversation) -> Result<()> {
        let conn = self.conn.clone();
        let conv = conversation.clone();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            conn.execute(
                "INSERT INTO conversations (id, account_id, title, model, system_prompt, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    conv.id,
                    conv.account_id,
                    conv.title,
                    conv.model,
                    conv.system_prompt,
                    conv.created_at.to_rfc3339(),
                    conv.updated_at.to_rfc3339(),
                ],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn list_conversations(&self) -> Result<Vec<Conversation>> {
        let conn = self.conn.clone();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT c.id, c.account_id, c.title, c.model, c.system_prompt, c.created_at, c.updated_at, c.pinned,
                        (SELECT SUBSTR(m.content, 1, 100) FROM messages m WHERE m.conversation_id = c.id AND m.is_active = 1 ORDER BY m.created_at DESC LIMIT 1) as last_preview
                 FROM conversations c ORDER BY c.pinned DESC, c.updated_at DESC",
            )?;
            let conversations = stmt
                .query_map([], |row| Ok(Self::row_to_conversation(row)))?
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?;
            Ok(conversations)
        })
        .await?
    }

    pub async fn update_conversation_title(&self, id: &str, title: &str) -> Result<()> {
        let conn = self.conn.clone();
        let id = id.to_string();
        let title = title.to_string();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            conn.execute(
                "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
                params![title, Utc::now().to_rfc3339(), id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn update_conversation_model(&self, id: &str, model: &str) -> Result<()> {
        let conn = self.conn.clone();
        let id = id.to_string();
        let model = model.to_string();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            conn.execute(
                "UPDATE conversations SET model = ?1, updated_at = ?2 WHERE id = ?3",
                params![model, Utc::now().to_rfc3339(), id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn update_conversation_timestamp(&self, id: &str) -> Result<()> {
        let conn = self.conn.clone();
        let id = id.to_string();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            conn.execute(
                "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
                params![Utc::now().to_rfc3339(), id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn delete_conversation(&self, id: &str) -> Result<()> {
        let conn = self.conn.clone();
        let id = id.to_string();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            conn.execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
            Ok(())
        })
        .await?
    }

    pub async fn toggle_conversation_pin(&self, id: &str, pinned: bool) -> Result<()> {
        let conn = self.conn.clone();
        let id = id.to_string();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            conn.execute(
                "UPDATE conversations SET pinned = ?1 WHERE id = ?2",
                params![pinned as i32, id],
            )?;
            Ok(())
        })
        .await?
    }

    // --- Message CRUD ---

    pub async fn insert_message(&self, message: &Message) -> Result<()> {
        let conn = self.conn.clone();
        let msg = message.clone();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            conn.execute(
                "INSERT INTO messages (id, conversation_id, role, content, model, tokens_in, tokens_out, parent_message_id, is_active, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    msg.id,
                    msg.conversation_id,
                    msg.role.as_str(),
                    msg.content,
                    msg.model,
                    msg.tokens_in,
                    msg.tokens_out,
                    msg.parent_message_id,
                    msg.is_active as i32,
                    msg.created_at.to_rfc3339(),
                ],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn list_messages(&self, conversation_id: &str) -> Result<Vec<Message>> {
        let conn = self.conn.clone();
        let conversation_id = conversation_id.to_string();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT id, conversation_id, role, content, model, tokens_in, tokens_out, parent_message_id, is_active, created_at
                 FROM messages WHERE conversation_id = ?1 AND is_active = 1 ORDER BY created_at ASC",
            )?;
            let messages = stmt
                .query_map(params![conversation_id], |row| {
                    Ok(Self::row_to_message(row))
                })?
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?;
            Ok(messages)
        })
        .await?
    }

    // --- New Phase 3 methods ---

    pub async fn update_conversation_system_prompt(
        &self,
        id: &str,
        system_prompt: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.clone();
        let id = id.to_string();
        let system_prompt = system_prompt.map(|s| s.to_string());
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            conn.execute(
                "UPDATE conversations SET system_prompt = ?1, updated_at = ?2 WHERE id = ?3",
                params![system_prompt, Utc::now().to_rfc3339(), id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn update_message_content(&self, id: &str, content: &str) -> Result<()> {
        let conn = self.conn.clone();
        let id = id.to_string();
        let content = content.to_string();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            conn.execute(
                "UPDATE messages SET content = ?1 WHERE id = ?2",
                params![content, id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn deactivate_messages_after(
        &self,
        conversation_id: &str,
        after_created_at: &str,
    ) -> Result<()> {
        let conn = self.conn.clone();
        let conversation_id = conversation_id.to_string();
        let after_created_at = after_created_at.to_string();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            conn.execute(
                "UPDATE messages SET is_active = 0 WHERE conversation_id = ?1 AND created_at > ?2",
                params![conversation_id, after_created_at],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn insert_attachment(&self, attachment: &Attachment) -> Result<()> {
        let conn = self.conn.clone();
        let att = attachment.clone();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            conn.execute(
                "INSERT INTO message_attachments (id, message_id, mime_type, filename, data, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    att.id,
                    att.message_id,
                    att.mime_type,
                    att.filename,
                    att.data,
                    att.created_at.to_rfc3339(),
                ],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn list_attachments(&self, message_id: &str) -> Result<Vec<Attachment>> {
        let conn = self.conn.clone();
        let message_id = message_id.to_string();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT id, message_id, mime_type, filename, data, created_at
                 FROM message_attachments WHERE message_id = ?1",
            )?;
            let attachments = stmt
                .query_map(params![message_id], |row| {
                    Ok(Self::row_to_attachment(row))
                })?
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?;
            Ok(attachments)
        })
        .await?
    }

    pub async fn get_conversation(&self, id: &str) -> Result<Option<Conversation>> {
        let conn = self.conn.clone();
        let id = id.to_string();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT c.id, c.account_id, c.title, c.model, c.system_prompt, c.created_at, c.updated_at, c.pinned,
                        (SELECT SUBSTR(m.content, 1, 100) FROM messages m WHERE m.conversation_id = c.id AND m.is_active = 1 ORDER BY m.created_at DESC LIMIT 1) as last_preview
                 FROM conversations c WHERE c.id = ?1",
            )?;
            let result = stmt
                .query_row(params![id], |row| Ok(Self::row_to_conversation(row)))
                .optional()?;
            match result {
                Some(Ok(conv)) => Ok(Some(conv)),
                Some(Err(e)) => Err(e),
                None => Ok(None),
            }
        })
        .await?
    }

    // --- Settings ---

    pub async fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.clone();
        let key = key.to_string();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            let result: Option<String> = conn
                .query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(result)
        })
        .await?
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.clone();
        let key = key.to_string();
        let value = value.to_string();
        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
                params![key, value],
            )?;
            Ok(())
        })
        .await?
    }

    // --- Row helpers ---

    fn row_to_account(row: &rusqlite::Row) -> Result<Account> {
        let provider_str: String = row.get(1)?;
        let status_str: String = row.get(6)?;
        let created_str: String = row.get(9)?;
        let updated_str: String = row.get(10)?;
        let is_default_int: i32 = row.get(5)?;

        Ok(Account {
            id: row.get(0)?,
            provider: ProviderId::from_str(&provider_str)
                .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", provider_str))?,
            label: row.get(2)?,
            api_base_url: row.get(3)?,
            default_model: row.get(4)?,
            is_default: is_default_int != 0,
            status: AccountStatus::from_str(&status_str)
                .ok_or_else(|| anyhow::anyhow!("Unknown status: {}", status_str))?,
            total_tokens_in: row.get(7)?,
            total_tokens_out: row.get(8)?,
            created_at: DateTime::parse_from_rfc3339(&created_str)?.with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&updated_str)?.with_timezone(&Utc),
        })
    }

    fn row_to_conversation(row: &rusqlite::Row) -> Result<Conversation> {
        let created_str: String = row.get(5)?;
        let updated_str: String = row.get(6)?;
        let pinned_int: i32 = row.get(7)?;
        let last_message_preview: Option<String> = row.get(8)?;

        Ok(Conversation {
            id: row.get(0)?,
            account_id: row.get(1)?,
            title: row.get(2)?,
            model: row.get(3)?,
            system_prompt: row.get(4)?,
            pinned: pinned_int != 0,
            last_message_preview,
            created_at: DateTime::parse_from_rfc3339(&created_str)?.with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&updated_str)?.with_timezone(&Utc),
        })
    }

    fn row_to_message(row: &rusqlite::Row) -> Result<Message> {
        let role_str: String = row.get(2)?;
        let is_active_int: i32 = row.get(8)?;
        let created_str: String = row.get(9)?;

        Ok(Message {
            id: row.get(0)?,
            conversation_id: row.get(1)?,
            role: Role::from_str(&role_str)
                .ok_or_else(|| anyhow::anyhow!("Unknown role: {}", role_str))?,
            content: row.get(3)?,
            model: row.get(4)?,
            tokens_in: row.get(5)?,
            tokens_out: row.get(6)?,
            parent_message_id: row.get(7)?,
            is_active: is_active_int != 0,
            created_at: DateTime::parse_from_rfc3339(&created_str)?.with_timezone(&Utc),
            attachments: Vec::new(),
        })
    }

    fn row_to_attachment(row: &rusqlite::Row) -> Result<Attachment> {
        let created_str: String = row.get(5)?;

        Ok(Attachment {
            id: row.get(0)?,
            message_id: row.get(1)?,
            mime_type: row.get(2)?,
            filename: row.get(3)?,
            data: row.get(4)?,
            created_at: DateTime::parse_from_rfc3339(&created_str)?.with_timezone(&Utc),
        })
    }
}

use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_schema_initialization() {
        let db = Database::new_in_memory().unwrap();
        let accounts = db.list_accounts().await.unwrap();
        assert!(accounts.is_empty());
    }

    #[tokio::test]
    async fn test_account_crud() {
        let db = Database::new_in_memory().unwrap();
        let now = Utc::now();

        let account = Account {
            id: uuid::Uuid::new_v4().to_string(),
            provider: ProviderId::Gemini,
            label: "Test Account".to_string(),
            api_base_url: None,
            default_model: "gemini-2.5-flash".to_string(),
            is_default: true,
            status: AccountStatus::Active,
            total_tokens_in: 0,
            total_tokens_out: 0,
            created_at: now,
            updated_at: now,
        };

        db.insert_account(&account).await.unwrap();

        let fetched = db.get_account(&account.id).await.unwrap().unwrap();
        assert_eq!(fetched.label, "Test Account");
        assert_eq!(fetched.provider, ProviderId::Gemini);

        let all = db.list_accounts().await.unwrap();
        assert_eq!(all.len(), 1);

        assert!(db.has_any_accounts().await.unwrap());

        db.delete_account(&account.id).await.unwrap();
        assert!(!db.has_any_accounts().await.unwrap());
    }

    #[tokio::test]
    async fn test_conversation_and_messages() {
        let db = Database::new_in_memory().unwrap();
        let now = Utc::now();

        let account = Account {
            id: uuid::Uuid::new_v4().to_string(),
            provider: ProviderId::Gemini,
            label: "Test".to_string(),
            api_base_url: None,
            default_model: "gemini-2.5-flash".to_string(),
            is_default: true,
            status: AccountStatus::Active,
            total_tokens_in: 0,
            total_tokens_out: 0,
            created_at: now,
            updated_at: now,
        };
        db.insert_account(&account).await.unwrap();

        let conv = Conversation {
            id: uuid::Uuid::new_v4().to_string(),
            account_id: account.id.clone(),
            title: "Test Chat".to_string(),
            model: "gemini-2.5-flash".to_string(),
            system_prompt: None,
            pinned: false,
            last_message_preview: None,
            created_at: now,
            updated_at: now,
        };
        db.insert_conversation(&conv).await.unwrap();

        let msg = Message {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: conv.id.clone(),
            role: Role::User,
            content: "Hello!".to_string(),
            model: None,
            tokens_in: None,
            tokens_out: None,
            parent_message_id: None,
            is_active: true,
            created_at: now,
            attachments: Vec::new(),
        };
        db.insert_message(&msg).await.unwrap();

        let messages = db.list_messages(&conv.id).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Hello!");

        let convos = db.list_conversations().await.unwrap();
        assert_eq!(convos.len(), 1);

        db.delete_conversation(&conv.id).await.unwrap();
        let convos = db.list_conversations().await.unwrap();
        assert!(convos.is_empty());

        // Messages should be cascade deleted
        let messages = db.list_messages(&conv.id).await.unwrap();
        assert!(messages.is_empty());
    }
}
