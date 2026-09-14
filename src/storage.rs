use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::message::{Message, Role};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskItem {
    pub id: u64,
    pub title: String,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: u64,
    pub content: String,
    #[serde(default)]
    pub embedding: Vec<f32>,
}

#[derive(Debug, Clone)]
pub struct StoredLog {
    pub timestamp: u64,
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: u64,
    pub project_id: u64,
    pub title: String,
    pub updated_at: i64,
}

#[derive(Debug, Default)]
pub struct PersistedState {
    pub projects: Vec<Project>,
    pub conversations: Vec<Conversation>,
    pub messages: Vec<Message>,
    pub tasks: Vec<TaskItem>,
    pub memories: Vec<MemoryItem>,
    pub selected_model: String,
    pub high_thinking: bool,
}

pub struct Storage {
    database_path: PathBuf,
}

impl Storage {
    pub fn new() -> Self {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let storage = Self {
            database_path: base.join("DeskPilot").join("deskpilot.db"),
        };
        let _ = storage.initialize();
        storage
    }

    pub fn database_path(&self) -> &PathBuf {
        &self.database_path
    }

    fn connection(&self) -> Result<Connection> {
        if let Some(parent) = self.database_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Connection::open(&self.database_path).context("could not open DeskPilot database")
    }

    fn initialize(&self) -> Result<()> {
        let conn = self.connection()?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS projects (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS conversations (id INTEGER PRIMARY KEY AUTOINCREMENT, project_id INTEGER NOT NULL, title TEXT NOT NULL, updated_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS tasks (id INTEGER PRIMARY KEY, title TEXT NOT NULL, done INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS memories (id INTEGER PRIMARY KEY, project_id INTEGER NOT NULL DEFAULT 1, content TEXT NOT NULL, embedding TEXT NOT NULL DEFAULT '[]', confidence REAL NOT NULL DEFAULT 1.0, access_count INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE IF NOT EXISTS scratchpad (id INTEGER PRIMARY KEY AUTOINCREMENT, project_id INTEGER NOT NULL DEFAULT 1, note TEXT NOT NULL, decay_score REAL NOT NULL DEFAULT 1.0, created_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS logs (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, level TEXT NOT NULL, message TEXT NOT NULL);"
        )?;
        
        let _ = conn.execute("INSERT OR IGNORE INTO projects (id, name) VALUES (1, 'Default Project')", []);
        let _ = conn.execute("INSERT OR IGNORE INTO conversations (id, project_id, title, updated_at) VALUES (1, 1, 'Main Chat', 0)", []);

        let schema: String = conn.query_row("SELECT sql FROM sqlite_master WHERE type='table' AND name='messages'", [], |row| row.get(0)).unwrap_or_default();
        if schema.contains("position INTEGER PRIMARY KEY") {
            conn.execute_batch("
                ALTER TABLE messages RENAME TO messages_old;
                CREATE TABLE messages (id INTEGER PRIMARY KEY AUTOINCREMENT, conversation_id INTEGER NOT NULL DEFAULT 1, position INTEGER NOT NULL, role TEXT NOT NULL, content TEXT NOT NULL, thinking TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL DEFAULT 0);
                INSERT INTO messages (conversation_id, position, role, content, thinking, created_at) SELECT 1, position, role, content, thinking, created_at FROM messages_old;
                DROP TABLE messages_old;
            ")?;
        } else {
            conn.execute("CREATE TABLE IF NOT EXISTS messages (id INTEGER PRIMARY KEY AUTOINCREMENT, conversation_id INTEGER NOT NULL DEFAULT 1, position INTEGER NOT NULL, role TEXT NOT NULL, content TEXT NOT NULL, thinking TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL DEFAULT 0)", [])?;
        }
        
        let _ = conn.execute("ALTER TABLE memories ADD COLUMN project_id INTEGER NOT NULL DEFAULT 1", []);
        
        Ok(())
    }

    pub fn load(&self) -> Result<PersistedState> {
        let connection = self.connection()?;
        let selected_model = connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'selected_model'",
                [],
                |row| row.get(0),
            )
            .unwrap_or_default();
        let high_thinking = connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'high_thinking'",
                [],
                |row| row.get::<_, String>(0),
            )
            .map(|value| value == "true")
            .unwrap_or(false);

        let mut statement = connection.prepare(
            "SELECT conversation_id, role, content, thinking, created_at FROM messages ORDER BY conversation_id, position",
        )?;
        let messages = statement
            .query_map([], |row| {
                let conv_id: u64 = row.get(0)?;
                let role: String = row.get(1)?;
                let role = match role.as_str() {
                    "system" => Role::System,
                    "assistant" => Role::Assistant,
                    _ => Role::User,
                };
                let mut message = Message::new(conv_id, role, row.get::<_, String>(2)?);
                message.thinking = row.get(3)?;
                message.created_at = row.get(4)?;
                Ok(message)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut statement = connection.prepare("SELECT id, title, done FROM tasks ORDER BY id")?;
        let tasks = statement
            .query_map([], |row| {
                Ok(TaskItem {
                    id: row.get::<_, i64>(0)? as u64,
                    title: row.get(1)?,
                    done: row.get::<_, i64>(2)? != 0,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut statement =
            connection.prepare("SELECT id, content, embedding FROM memories ORDER BY id")?;
        let memories = statement
            .query_map([], |row| {
                let embedding: String = row.get(2)?;
                Ok(MemoryItem {
                    id: row.get::<_, i64>(0)? as u64,
                    content: row.get(1)?,
                    embedding: serde_json::from_str(&embedding).unwrap_or_default(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut statement = connection.prepare("SELECT id, name FROM projects ORDER BY id")?;
        let projects = statement.query_map([], |row| Ok(Project { id: row.get(0)?, name: row.get(1)? }))?.collect::<rusqlite::Result<Vec<_>>>()?;

        let mut statement = connection.prepare("SELECT id, project_id, title, updated_at FROM conversations ORDER BY updated_at DESC")?;
        let conversations = statement.query_map([], |row| Ok(Conversation {
            id: row.get(0)?, project_id: row.get(1)?, title: row.get(2)?, updated_at: row.get(3)?
        }))?.collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(PersistedState {
            projects,
            conversations,
            messages,
            tasks,
            memories,
            selected_model,
            high_thinking,
        })
    }

    pub fn load_logs(&self, limit: usize) -> Result<Vec<StoredLog>> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("SELECT timestamp, level, message FROM logs ORDER BY id DESC LIMIT ?1")?;
        let mut logs = statement
            .query_map([limit as i64], |row| {
                Ok(StoredLog {
                    timestamp: row.get::<_, i64>(0)? as u64,
                    level: row.get(1)?,
                    message: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        logs.reverse();
        Ok(logs)
    }

    pub fn save(&self, state: &PersistedState) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute("INSERT INTO settings(key, value) VALUES('selected_model', ?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [&state.selected_model])?;
        transaction.execute("INSERT INTO settings(key, value) VALUES('high_thinking', ?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [state.high_thinking.to_string()])?;
        transaction.execute("DELETE FROM messages", [])?;
        for (position, message) in state.messages.iter().enumerate() {
            let role = match message.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            transaction.execute(
                "INSERT INTO messages(conversation_id, position, role, content, thinking, created_at) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                params![message.conversation_id as i64, position as i64, role, message.content, message.thinking, message.created_at],
            )?;
        }
        transaction.execute("DELETE FROM tasks", [])?;
        for task in &state.tasks {
            transaction.execute(
                "INSERT INTO tasks(id, title, done) VALUES(?1, ?2, ?3)",
                params![task.id as i64, task.title, task.done as i64],
            )?;
        }
        transaction.execute("DELETE FROM memories", [])?;
        for memory in &state.memories {
            transaction.execute(
                "INSERT INTO memories(id, content, embedding) VALUES(?1, ?2, ?3)",
                params![
                    memory.id as i64,
                    memory.content,
                    serde_json::to_string(&memory.embedding)?
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn append_log(&self, timestamp: u64, level: &str, message: &str) -> Result<()> {
        self.connection()?.execute(
            "INSERT INTO logs(timestamp, level, message) VALUES(?1, ?2, ?3)",
            params![timestamp as i64, level, message],
        )?;
        Ok(())
    }

    pub fn append_scratchpad(&self, project_id: u64, note: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        self.connection()?.execute(
            "INSERT INTO scratchpad(project_id, note, decay_score, created_at) VALUES(?1, ?2, 1.0, ?3)",
            params![project_id as i64, note, now],
        )?;
        Ok(())
    }

    pub fn get_scratchpad(&self, project_id: u64) -> Result<Vec<String>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare("SELECT note FROM scratchpad WHERE project_id = ?1 ORDER BY id DESC LIMIT 15")?;
        let notes = stmt.query_map([project_id as i64], |row| row.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(notes)
    }

    pub fn get_adaptive_context(&self, project_id: u64) -> String {
        let scratch = self.get_scratchpad(project_id).unwrap_or_default();
        if scratch.is_empty() {
            return String::new();
        }
        format!(
            "\n\n[ADAPTIVE MEMORY LAYER - ACTIVE SCRATCHPAD]\n{}",
            scratch.join("\n- ")
        )
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query([key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.connection()?;
        conn.execute(
            "INSERT INTO settings(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }
}
