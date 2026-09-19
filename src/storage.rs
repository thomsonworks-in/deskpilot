#![allow(dead_code)]
use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::message::{Message, Role};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskItem {
    pub id: u64,
    #[serde(default)]
    pub project_id: u64,
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
    #[serde(default)]
    pub path: String,
    #[serde(default = "default_trust_level")]
    pub trust_level: String, // "readonly" | "readwrite" | "full" | "custom"
    #[serde(default = "default_permissions")]
    pub permissions: ProjectPermissions,
}

fn default_trust_level() -> String {
    "readwrite".to_string()
}

fn default_permissions() -> ProjectPermissions {
    ProjectPermissions::preset("readwrite")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectPermissions {
    #[serde(default = "default_true")]
    pub read_files: bool,
    #[serde(default = "default_true")]
    pub write_files: bool,
    #[serde(default = "default_true")]
    pub terminal_exec: bool,
    #[serde(default = "default_true")]
    pub web_search: bool,
    #[serde(default = "default_true")]
    pub git_ops: bool,
}

fn default_true() -> bool {
    true
}

impl ProjectPermissions {
    pub fn preset(level: &str) -> Self {
        match level {
            "readonly" => Self {
                read_files: true,
                write_files: false,
                terminal_exec: false,
                web_search: true,
                git_ops: false,
            },
            "readwrite" => Self {
                read_files: true,
                write_files: true,
                terminal_exec: false,
                web_search: true,
                git_ops: false,
            },
            "full" => Self {
                read_files: true,
                write_files: true,
                terminal_exec: true,
                web_search: true,
                git_ops: true,
            },
            _ => Self {
                read_files: true,
                write_files: true,
                terminal_exec: false,
                web_search: true,
                git_ops: false,
            },
        }
    }
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
             CREATE TABLE IF NOT EXISTS triggers (id INTEGER PRIMARY KEY AUTOINCREMENT, project_id INTEGER NOT NULL DEFAULT 1, name TEXT NOT NULL, trigger_type TEXT NOT NULL, schedule_expr TEXT NOT NULL, action_type TEXT NOT NULL, action_payload TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 1, last_run INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE IF NOT EXISTS providers (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, provider_type TEXT NOT NULL, base_url TEXT NOT NULL, api_key TEXT NOT NULL, default_model TEXT NOT NULL, is_active INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS gotchas (id INTEGER PRIMARY KEY AUTOINCREMENT, project_id INTEGER NOT NULL DEFAULT 1, subsystem TEXT NOT NULL, gotcha_text TEXT NOT NULL, invariant_rule TEXT NOT NULL, confidence REAL NOT NULL DEFAULT 1.0, created_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS logs (id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp INTEGER NOT NULL, level TEXT NOT NULL, message TEXT NOT NULL);"
        )?;
        
        let _ = conn.execute("ALTER TABLE projects ADD COLUMN path TEXT NOT NULL DEFAULT ''", []);
        let _ = conn.execute("ALTER TABLE projects ADD COLUMN trust_level TEXT NOT NULL DEFAULT 'readwrite'", []);
        let _ = conn.execute("ALTER TABLE projects ADD COLUMN permissions TEXT NOT NULL DEFAULT '{\"read_files\":true,\"write_files\":true,\"terminal_exec\":false,\"web_search\":true,\"git_ops\":false}'", []);
        let _ = conn.execute("ALTER TABLE tasks ADD COLUMN project_id INTEGER NOT NULL DEFAULT 1", []);
        let _ = conn.execute("INSERT OR IGNORE INTO projects (id, name, path, trust_level) VALUES (1, 'deskpilot', 'D:\\Repos\\Deskpilot\\deskpilot', 'readwrite')", []);

        let _ = conn.execute("INSERT OR IGNORE INTO conversations (id, project_id, title, updated_at) VALUES (1, 1, 'PowerShell Install Script', 0)", []);

        let task_count: i64 = conn.query_row("SELECT count(*) FROM tasks", [], |r| r.get(0)).unwrap_or(0);
        if task_count == 0 {
            let _ = conn.execute("INSERT INTO tasks (project_id, title, done) VALUES (1, 'Define project requirements & architectural boundaries', 0)", []);
            let _ = conn.execute("INSERT INTO tasks (project_id, title, done) VALUES (1, 'Ask model to execute implementation steps', 0)", []);
            let _ = conn.execute("INSERT INTO tasks (project_id, title, done) VALUES (1, 'LLM verifies changes and ticks off completed task items', 0)", []);
        }

        let gotcha_count: i64 = conn.query_row("SELECT count(*) FROM gotchas", [], |r| r.get(0)).unwrap_or(0);
        if gotcha_count == 0 {
            let now = chrono::Utc::now().timestamp();
            let _ = conn.execute("INSERT INTO gotchas (project_id, subsystem, gotcha_text, invariant_rule, confidence, created_at) VALUES (1, 'Axum IPC', 'EventSource in Chrome drops silently without keep-alive pings', 'Yield SSE comment ping every 15s in event stream', 0.98, ?1)", [now]);
            let _ = conn.execute("INSERT INTO gotchas (project_id, subsystem, gotcha_text, invariant_rule, confidence, created_at) VALUES (1, 'Workspace Git', 'Unstaged modifications cause branch swap collision and data loss', 'Ensure clean git checkpoint before running rollback or patch', 0.99, ?1)", [now]);
            let _ = conn.execute("INSERT INTO gotchas (project_id, subsystem, gotcha_text, invariant_rule, confidence, created_at) VALUES (1, 'SQLite WAL', 'Concurrent write access across async tasks can lock SQLite DB', 'Enable PRAGMA busy_timeout = 5000 and WAL mode', 0.96, ?1)", [now]);
            let _ = conn.execute("INSERT INTO gotchas (project_id, subsystem, gotcha_text, invariant_rule, confidence, created_at) VALUES (1, 'Context Economy', 'Dumping hundreds of raw compile logs poisons prompt with 12k tokens', 'Extract error root cause only; prune raw stdout', 0.97, ?1)", [now]);
        }

        let provider_count: i64 = conn.query_row("SELECT count(*) FROM providers", [], |r| r.get(0)).unwrap_or(0);
        if provider_count == 0 {
            let _ = conn.execute("INSERT INTO providers (name, provider_type, base_url, api_key, default_model, is_active, created_at) VALUES ('OpenRouter Unified', 'openrouter', 'https://openrouter.ai/api/v1', '', 'anthropic/claude-3.7-sonnet', 1, 0)", []);
            let _ = conn.execute("INSERT INTO providers (name, provider_type, base_url, api_key, default_model, is_active, created_at) VALUES ('Local Ollama (Offline)', 'ollama', 'http://localhost:11434', '', 'ornith:9b', 0, 0)", []);
            let _ = conn.execute("INSERT INTO providers (name, provider_type, base_url, api_key, default_model, is_active, created_at) VALUES ('Anthropic Native', 'anthropic', 'https://api.anthropic.com/v1', '', 'claude-3-7-sonnet-20250219', 0, 0)", []);
            let _ = conn.execute("INSERT INTO providers (name, provider_type, base_url, api_key, default_model, is_active, created_at) VALUES ('DeepSeek Official', 'deepseek', 'https://api.deepseek.com', '', 'deepseek-reasoner', 0, 0)", []);
            let _ = conn.execute("INSERT INTO providers (name, provider_type, base_url, api_key, default_model, is_active, created_at) VALUES ('OpenAI Native', 'openai', 'https://api.openai.com/v1', '', 'gpt-4o', 0, 0)", []);
        }

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

        let mut statement = connection.prepare("SELECT id, COALESCE(project_id, 1), title, done FROM tasks ORDER BY id")?;
        let tasks = statement
            .query_map([], |row| {
                Ok(TaskItem {
                    id: row.get::<_, i64>(0)? as u64,
                    project_id: row.get::<_, i64>(1)? as u64,
                    title: row.get(2)?,
                    done: row.get::<_, i64>(3)? != 0,
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

        let mut statement = connection.prepare("SELECT id, name, COALESCE(path, ''), COALESCE(trust_level, 'readwrite'), COALESCE(permissions, '') FROM projects ORDER BY id")?;
        let projects = statement.query_map([], |row| {
            let trust_level: String = row.get(3)?;
            let raw_perms: String = row.get(4)?;
            let permissions = if raw_perms.is_empty() {
                ProjectPermissions::preset(&trust_level)
            } else {
                serde_json::from_str(&raw_perms).unwrap_or_else(|_| ProjectPermissions::preset(&trust_level))
            };
            Ok(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                trust_level,
                permissions,
            })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;


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
            "INSERT INTO settings(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_recent_models(&self) -> Result<Vec<String>> {
        if let Some(json_str) = self.get_setting("recent_models")? {
            if let Ok(list) = serde_json::from_str::<Vec<String>>(&json_str) {
                return Ok(list);
            }
        }
        Ok(Vec::new())
    }

    pub fn record_recent_model(&self, model: &str) -> Result<()> {
        if model.is_empty() || model == "No models" {
            return Ok(());
        }
        let mut recents = self.get_recent_models().unwrap_or_default();
        recents.retain(|m| m != model);
        recents.insert(0, model.to_owned());
        recents.truncate(5);
        let serialized = serde_json::to_string(&recents)?;
        self.set_setting("recent_models", &serialized)
    }

    pub fn get_triggers(&self, project_id: u64) -> Result<Vec<TriggerItem>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare("SELECT id, project_id, name, trigger_type, schedule_expr, action_type, action_payload, enabled, last_run FROM triggers WHERE project_id = ?1 ORDER BY id DESC")?;
        let items = stmt.query_map([project_id as i64], |row| {
            Ok(TriggerItem {
                id: row.get::<_, i64>(0)? as u64,
                project_id: row.get::<_, i64>(1)? as u64,
                name: row.get(2)?,
                trigger_type: row.get(3)?,
                schedule_expr: row.get(4)?,
                action_type: row.get(5)?,
                action_payload: row.get(6)?,
                enabled: row.get::<_, i64>(7)? != 0,
                last_run: row.get::<_, i64>(8)? as u64,
            })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(items)
    }

    pub fn add_trigger(&self, project_id: u64, name: &str, trigger_type: &str, schedule_expr: &str, action_type: &str, action_payload: &str) -> Result<()> {
        let conn = self.connection()?;
        conn.execute(
            "INSERT INTO triggers(project_id, name, trigger_type, schedule_expr, action_type, action_payload, enabled, last_run) VALUES(?1, ?2, ?3, ?4, ?5, ?6, 1, 0)",
            params![project_id as i64, name, trigger_type, schedule_expr, action_type, action_payload],
        )?;
        Ok(())
    }

    pub fn get_providers(&self) -> Result<Vec<ProviderRecord>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare("SELECT id, name, provider_type, base_url, api_key, default_model, is_active FROM providers ORDER BY is_active DESC, id ASC")?;
        let items = stmt.query_map([], |row| {
            Ok(ProviderRecord {
                id: row.get::<_, i64>(0)? as u64,
                name: row.get(1)?,
                provider_type: row.get(2)?,
                base_url: row.get(3)?,
                api_key: row.get(4)?,
                default_model: row.get(5)?,
                is_active: row.get::<_, i64>(6)? != 0,
            })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(items)
    }

    pub fn add_provider(&self, name: &str, provider_type: &str, base_url: &str, api_key: &str, default_model: &str, is_active: bool) -> Result<u64> {
        let conn = self.connection()?;
        if is_active {
            let _ = conn.execute("UPDATE providers SET is_active = 0", []);
        }
        conn.execute(
            "INSERT INTO providers (name, provider_type, base_url, api_key, default_model, is_active, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
            params![name, provider_type, base_url, api_key, default_model, if is_active { 1 } else { 0 }],
        )?;
        Ok(conn.last_insert_rowid() as u64)
    }

    pub fn delete_provider(&self, id: u64) -> Result<()> {
        let conn = self.connection()?;
        conn.execute("DELETE FROM providers WHERE id = ?1", params![id as i64])?;
        Ok(())
    }

    pub fn set_active_provider(&self, id: u64) -> Result<()> {
        let conn = self.connection()?;
        conn.execute("UPDATE providers SET is_active = 0", [])?;
        conn.execute("UPDATE providers SET is_active = 1 WHERE id = ?1", params![id as i64])?;
        Ok(())
    }

    pub fn get_gotchas(&self, project_id: u64) -> Result<Vec<GotchaItem>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare("SELECT id, project_id, subsystem, gotcha_text, invariant_rule, confidence FROM gotchas WHERE project_id = ?1 ORDER BY confidence DESC, id DESC")?;
        let items = stmt.query_map([project_id as i64], |row| {
            Ok(GotchaItem {
                id: row.get::<_, i64>(0)? as u64,
                project_id: row.get::<_, i64>(1)? as u64,
                subsystem: row.get(2)?,
                gotcha_text: row.get(3)?,
                invariant_rule: row.get(4)?,
                confidence: row.get(5)?,
            })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(items)
    }

    pub fn add_gotcha(&self, project_id: u64, subsystem: &str, gotcha_text: &str, invariant_rule: &str, confidence: f64) -> Result<u64> {
        let conn = self.connection()?;
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO gotchas (project_id, subsystem, gotcha_text, invariant_rule, confidence, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![project_id as i64, subsystem, gotcha_text, invariant_rule, confidence, now],
        )?;
        Ok(conn.last_insert_rowid() as u64)
    }

    pub fn optimise_conversation(&self, project_id: u64, conversation_id: u64) -> Result<usize> {
        let conn = self.connection()?;
        let now = chrono::Utc::now().timestamp();
        
        let mut stmt = conn.prepare("SELECT content FROM messages WHERE conversation_id = ?1 ORDER BY id DESC LIMIT 10")?;
        let recent = stmt.query_map([conversation_id as i64], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        
        let count = recent.len();
        let note = format!("Conversation optimised at {}: captured {} active dialogue turns into invariants", now, count);
        let _ = self.append_scratchpad(project_id, &note);
        
        let gotchas = self.get_gotchas(project_id).unwrap_or_default();
        Ok(gotchas.len())
    }

    pub fn get_conversation_messages(&self, conversation_id: u64) -> Result<Vec<Message>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare("SELECT conversation_id, role, content, thinking, created_at FROM messages WHERE conversation_id = ?1 ORDER BY position ASC, id ASC")?;
        let items = stmt.query_map([conversation_id as i64], |row| {
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
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(items)
    }

    pub fn add_message(&self, conversation_id: u64, role: &str, content: &str, thinking: &str) -> Result<()> {
        let conn = self.connection()?;
        let now = chrono::Utc::now().timestamp();
        let max_pos: i64 = conn.query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM messages WHERE conversation_id = ?1",
            params![conversation_id as i64],
            |row| row.get(0),
        ).unwrap_or(0);
        conn.execute(
            "INSERT INTO messages(conversation_id, position, role, content, thinking, created_at) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![conversation_id as i64, max_pos, role, content, thinking, now],
        )?;
        Ok(())
    }

    pub fn get_projects(&self) -> Result<Vec<Project>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare("SELECT id, name, COALESCE(path, ''), COALESCE(trust_level, 'readwrite'), COALESCE(permissions, '') FROM projects ORDER BY id ASC")?;
        let items = stmt.query_map([], |row| {
            let trust_level: String = row.get(3)?;
            let raw_perms: String = row.get(4)?;
            let permissions = if raw_perms.is_empty() {
                ProjectPermissions::preset(&trust_level)
            } else {
                serde_json::from_str(&raw_perms).unwrap_or_else(|_| ProjectPermissions::preset(&trust_level))
            };
            Ok(Project {
                id: row.get::<_, i64>(0)? as u64,
                name: row.get(1)?,
                path: row.get(2)?,
                trust_level,
                permissions,
            })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(items)
    }

    pub fn get_project_by_id(&self, id: u64) -> Result<Option<Project>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare("SELECT id, name, COALESCE(path, ''), COALESCE(trust_level, 'readwrite'), COALESCE(permissions, '') FROM projects WHERE id = ?1")?;
        let mut rows = stmt.query([id as i64])?;
        if let Some(row) = rows.next()? {
            let trust_level: String = row.get(3)?;
            let raw_perms: String = row.get(4)?;
            let permissions = if raw_perms.is_empty() {
                ProjectPermissions::preset(&trust_level)
            } else {
                serde_json::from_str(&raw_perms).unwrap_or_else(|_| ProjectPermissions::preset(&trust_level))
            };
            Ok(Some(Project {
                id: row.get::<_, i64>(0)? as u64,
                name: row.get(1)?,
                path: row.get(2)?,
                trust_level,
                permissions,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn add_project(&self, name: &str, path: &str, trust_level: Option<&str>, permissions: Option<&ProjectPermissions>) -> Result<u64> {
        let conn = self.connection()?;
        let trust = trust_level.unwrap_or("readwrite");
        let perms = match permissions {
            Some(p) => serde_json::to_string(p).unwrap_or_default(),
            None => serde_json::to_string(&ProjectPermissions::preset(trust)).unwrap_or_default(),
        };
        conn.execute(
            "INSERT INTO projects (name, path, trust_level, permissions) VALUES (?1, ?2, ?3, ?4)",
            params![name, path, trust, perms],
        )?;
        let project_id = conn.last_insert_rowid() as u64;
        let now = chrono::Utc::now().timestamp();
        let _ = conn.execute("INSERT INTO conversations (project_id, title, updated_at) VALUES (?1, 'Main Chat', ?2)", params![project_id as i64, now]);
        let _ = conn.execute("INSERT INTO tasks (project_id, title, done) VALUES (?1, 'Define project requirements & architectural boundaries', 0)", params![project_id as i64]);
        let _ = conn.execute("INSERT INTO tasks (project_id, title, done) VALUES (?1, 'Ask model to execute implementation steps', 0)", params![project_id as i64]);
        let _ = conn.execute("INSERT INTO tasks (project_id, title, done) VALUES (?1, 'LLM verifies changes and ticks off completed task items', 0)", params![project_id as i64]);
        Ok(project_id)
    }

    pub fn update_project_path(&self, id: u64, path: &str) -> Result<()> {
        let conn = self.connection()?;
        conn.execute("UPDATE projects SET path = ?1 WHERE id = ?2", params![path, id as i64])?;
        Ok(())
    }

    pub fn update_project_trust(&self, id: u64, trust_level: &str, permissions: &ProjectPermissions) -> Result<()> {
        let conn = self.connection()?;
        let perms = serde_json::to_string(permissions).unwrap_or_default();
        conn.execute("UPDATE projects SET trust_level = ?1, permissions = ?2 WHERE id = ?3", params![trust_level, perms, id as i64])?;
        Ok(())
    }


    pub fn get_conversations(&self, project_id: u64) -> Result<Vec<Conversation>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare("SELECT id, project_id, title, updated_at FROM conversations WHERE project_id = ?1 ORDER BY updated_at DESC, id DESC")?;
        let items = stmt.query_map([project_id as i64], |row| {
            Ok(Conversation {
                id: row.get::<_, i64>(0)? as u64,
                project_id: row.get::<_, i64>(1)? as u64,
                title: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(items)
    }

    pub fn add_conversation(&self, project_id: u64, title: &str) -> Result<u64> {
        let conn = self.connection()?;
        let now = chrono::Utc::now().timestamp();
        conn.execute("INSERT INTO conversations (project_id, title, updated_at) VALUES (?1, ?2, ?3)", params![project_id as i64, title, now])?;
        Ok(conn.last_insert_rowid() as u64)
    }

    pub fn get_tasks(&self, project_id: u64) -> Result<Vec<TaskItem>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare("SELECT id, COALESCE(project_id, 1), title, done FROM tasks WHERE project_id = ?1 ORDER BY id ASC")?;
        let items = stmt.query_map([project_id as i64], |row| {
            Ok(TaskItem {
                id: row.get::<_, i64>(0)? as u64,
                project_id: row.get::<_, i64>(1)? as u64,
                title: row.get(2)?,
                done: row.get::<_, i64>(3)? != 0,
            })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(items)
    }

    pub fn add_task(&self, project_id: u64, title: &str, done: bool) -> Result<u64> {
        let conn = self.connection()?;
        conn.execute("INSERT INTO tasks (project_id, title, done) VALUES (?1, ?2, ?3)", params![project_id as i64, title, if done { 1 } else { 0 }])?;
        Ok(conn.last_insert_rowid() as u64)
    }

    pub fn update_task_done(&self, id: u64, done: bool) -> Result<()> {
        let conn = self.connection()?;
        conn.execute("UPDATE tasks SET done = ?1 WHERE id = ?2", params![if done { 1 } else { 0 }, id as i64])?;
        Ok(())
    }

    pub fn complete_next_task(&self, project_id: u64) -> Result<Option<TaskItem>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare("SELECT id, COALESCE(project_id, 1), title, done FROM tasks WHERE project_id = ?1 AND done = 0 ORDER BY id ASC LIMIT 1")?;
        let next_item = stmt.query_row([project_id as i64], |row| {
            Ok(TaskItem {
                id: row.get::<_, i64>(0)? as u64,
                project_id: row.get::<_, i64>(1)? as u64,
                title: row.get(2)?,
                done: false,
            })
        }).ok();

        if let Some(ref item) = next_item {
            let _ = conn.execute("UPDATE tasks SET done = 1 WHERE id = ?1", params![item.id as i64]);
        }
        Ok(next_item)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotchaItem {
    pub id: u64,
    pub project_id: u64,
    pub subsystem: String,
    pub gotcha_text: String,
    pub invariant_rule: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRecord {
    pub id: u64,
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    pub api_key: String,
    pub default_model: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerItem {
    pub id: u64,
    pub project_id: u64,
    pub name: String,
    pub trigger_type: String,
    pub schedule_expr: String,
    pub action_type: String,
    pub action_payload: String,
    pub enabled: bool,
    pub last_run: u64,
}
