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

#[derive(Debug, Default)]
pub struct PersistedState {
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
        self.connection()?.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS messages (position INTEGER PRIMARY KEY, role TEXT NOT NULL, content TEXT NOT NULL, thinking TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE IF NOT EXISTS tasks (id INTEGER PRIMARY KEY, title TEXT NOT NULL, done INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS memories (id INTEGER PRIMARY KEY, content TEXT NOT NULL, embedding TEXT NOT NULL DEFAULT '[]');
             CREATE TABLE IF NOT EXISTS logs (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, level TEXT NOT NULL, message TEXT NOT NULL);"
        )?;
        let _ = self.connection()?.execute(
            "ALTER TABLE messages ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = self.connection()?.execute(
            "ALTER TABLE messages ADD COLUMN thinking TEXT NOT NULL DEFAULT ''",
            [],
        );
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
            "SELECT role, content, thinking, created_at FROM messages ORDER BY position",
        )?;
        let messages = statement
            .query_map([], |row| {
                let role: String = row.get(0)?;
                let role = match role.as_str() {
                    "system" => Role::System,
                    "assistant" => Role::Assistant,
                    _ => Role::User,
                };
                let mut message = Message::new(role, row.get::<_, String>(1)?);
                message.thinking = row.get(2)?;
                message.created_at = row.get(3)?;
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

        Ok(PersistedState {
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
        transaction.execute("DELETE FROM tasks", [])?;
        transaction.execute("DELETE FROM memories", [])?;
        for (position, message) in state.messages.iter().enumerate() {
            let role = match message.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            transaction.execute(
                "INSERT INTO messages(position, role, content, thinking, created_at) VALUES(?1, ?2, ?3, ?4, ?5)",
                params![position as i64, role, message.content, message.thinking, message.created_at],
            )?;
        }
        for task in &state.tasks {
            transaction.execute(
                "INSERT INTO tasks(id, title, done) VALUES(?1, ?2, ?3)",
                params![task.id as i64, task.title, task.done as i64],
            )?;
        }
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
}
