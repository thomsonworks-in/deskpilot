use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use eframe::egui::{self, Color32, RichText, ScrollArea, TextEdit};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use tokio::runtime::Runtime;
use tokio::sync::{mpsc, oneshot};

use crate::message::{Message, Role};
use crate::ollama::{OllamaClient, StreamEvent, ToolContext};
use crate::storage::{Conversation, MemoryItem, PersistedState, Project, Storage, TaskItem};
use crate::tools::{discover_skills, Skill};

pub const OLLAMA_BASE_URL: &str = "http://127.0.0.1:11434";
const DEFAULT_MODEL: &str = "qwen3.6:35b-a3b";
const EMBEDDING_MODEL: &str = "qwen3-embedding:0.6b";
pub const WINDOW_SIZE: [f32; 2] = [1180.0, 780.0];
pub const MIN_WINDOW_SIZE: [f32; 2] = [760.0, 520.0];

const CANVAS: Color32 = Color32::from_rgb(10, 13, 18);
const SIDEBAR: Color32 = Color32::from_rgb(14, 18, 24);
const SURFACE: Color32 = Color32::from_rgb(18, 23, 31);
const SURFACE_HIGH: Color32 = Color32::from_rgb(25, 31, 41);
const BORDER: Color32 = Color32::from_rgb(43, 52, 66);
const TEXT: Color32 = Color32::from_rgb(226, 232, 240);
const MUTED: Color32 = Color32::from_rgb(128, 141, 158);
const ACCENT: Color32 = Color32::from_rgb(57, 255, 20); // Neon Green
const BLUE: Color32 = Color32::from_rgb(71, 118, 230);
const DANGER: Color32 = Color32::from_rgb(232, 103, 116);

#[derive(Debug, Clone)]
enum ConnectionStatus {
    Connecting,
    Connected,
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Chat,
    Tasks,
    Memory,
    Logs,
    Skills,
}

struct LogEntry {
    timestamp: u64,
    level: &'static str,
    message: String,
}

pub struct AiHelperApp {
    runtime: Arc<Runtime>,
    client: OllamaClient,
    projects: Vec<Project>,
    conversations: Vec<Conversation>,
    active_project: u64,
    active_conversation: u64,
    events_tx: mpsc::UnboundedSender<StreamEvent>,
    events_rx: mpsc::UnboundedReceiver<StreamEvent>,
    cancel_tx: Option<oneshot::Sender<()>>,
    storage: Storage,
    messages: Vec<Message>,
    tasks: Vec<TaskItem>,
    memories: Vec<MemoryItem>,
    logs: Vec<LogEntry>,
    ipc_rx: mpsc::UnboundedReceiver<String>,
    _tray_icon: Option<tray_icon::TrayIcon>,
    input: String,
    task_input: String,
    memory_input: String,
    memory_search: String,
    models: Vec<String>,
    selected_model: String,
    connection: ConnectionStatus,
    view: View,
    generating: bool,
    high_thinking: bool,
    loading_model: Option<String>,
    model_error: Option<String>,
    scroll_to_bottom: bool,
    active_task: Option<u64>,
    next_id: u64,
    workspace: std::path::PathBuf,
    skills: Vec<Skill>,
    markdown_cache: CommonMarkCache,
    force_quit: bool,
}

impl AiHelperApp {
    pub fn new(cc: &eframe::CreationContext<'_>, runtime: Arc<Runtime>, ipc_rx: mpsc::UnboundedReceiver<String>) -> Self {
        configure_style(&cc.egui_ctx);
        let storage = Storage::new();
        let workspace = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let skills = discover_skills(&workspace);
        let persisted = storage.load().unwrap_or_default();
        let next_id = persisted
            .tasks
            .iter()
            .map(|task| task.id)
            .chain(persisted.memories.iter().map(|memory| memory.id))
            .max()
            .unwrap_or(0)
            + 1;
        let selected_model = if persisted.selected_model.is_empty() {
            DEFAULT_MODEL.to_owned()
        } else {
            persisted.selected_model
        };
        let client = OllamaClient::new(OLLAMA_BASE_URL);
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let loader = client.clone();
        let loader_events = events_tx.clone();
        runtime.spawn(async move {
            match loader.list_models().await {
                Ok(models) => {
                    let _ = loader_events.send(StreamEvent::ModelsLoaded(models));
                }
                Err(error) => {
                    let _ = loader_events.send(StreamEvent::Error(error.to_string()));
                }
            }
        });

        let logs = storage
            .load_logs(500)
            .unwrap_or_default()
            .into_iter()
            .map(|entry| LogEntry {
                timestamp: entry.timestamp,
                level: match entry.level.as_str() {
                    "ERROR" => "ERROR",
                    "WARN" => "WARN",
                    _ => "INFO",
                },
                message: entry.message,
            })
            .collect();
        let icon = tray_icon::Icon::from_rgba(vec![0, 255, 0, 255], 1, 1).unwrap();
        let tray = tray_icon::TrayIconBuilder::new().with_tooltip("DeskPilot").with_icon(icon).build().ok();
        
        let active_project = persisted.projects.first().map(|p| p.id).unwrap_or(1);
        let active_conversation = persisted.conversations.first().map(|c| c.id).unwrap_or(1);

        let mut app = Self {
            storage,
            runtime,
            client,
            events_rx,
            events_tx,
            ipc_rx,
            cancel_tx: None,
            projects: persisted.projects,
            conversations: persisted.conversations,
            active_project,
            active_conversation,
            messages: persisted.messages,
            tasks: persisted.tasks,
            memories: persisted.memories,
            logs,
            _tray_icon: tray,
            input: String::new(),
            task_input: String::new(),
            memory_input: String::new(),
            memory_search: String::new(),
            models: Vec::new(),
            selected_model,
            connection: ConnectionStatus::Connecting,
            view: View::Chat,
            generating: false,
            high_thinking: persisted.high_thinking,
            loading_model: None,
            model_error: None,
            scroll_to_bottom: true,
            active_task: None,
            next_id,
            workspace,
            skills,
            markdown_cache: CommonMarkCache::default(),
            force_quit: false,
        };
        app.log("INFO", "DeskPilot started; loading local Ollama models");
        app
    }

    fn log(&mut self, level: &'static str, message: impl Into<String>) {
        let entry = LogEntry {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            level,
            message: message.into(),
        };
        let _ = self
            .storage
            .append_log(entry.timestamp, entry.level, &entry.message);
        self.logs.push(entry);
        if self.logs.len() > 500 {
            self.logs.remove(0);
        }
    }

    fn save_state(&self) {
        let state = PersistedState {
            projects: self.projects.clone(),
            conversations: self.conversations.clone(),
            messages: self.messages.clone(),
            tasks: self.tasks.clone(),
            memories: self.memories.clone(),
            selected_model: self.selected_model.clone(),
            high_thinking: self.high_thinking,
        };
        if let Err(error) = self.storage.save(&state) {
            eprintln!("Failed to save state: {error}");
        }
    }

    fn poll_events(&mut self) {
        while let Ok(event) = self.events_rx.try_recv() {
            match event {
                StreamEvent::Started => {
                    self.loading_model = None;
                    self.model_error = None;
                    self.log("INFO", "Ollama response stream started");
                }
                StreamEvent::ContentDelta(delta) => {
                    if let Some(message) = self.messages.last_mut() {
                        message.content.push_str(&delta);
                    }
                    self.scroll_to_bottom = true;
                }
                StreamEvent::ThinkingDelta(delta) => {
                    if let Some(message) = self.messages.last_mut() {
                        message.thinking.push_str(&delta);
                    }
                    self.scroll_to_bottom = true;
                }
                StreamEvent::ModelLoading(model) => {
                    self.loading_model = Some(model.clone());
                    self.log("INFO", format!("Loading model {model} into memory"));
                }
                StreamEvent::ModelReady(model) => {
                    self.loading_model = None;
                    self.model_error = None;
                    self.log("INFO", format!("Model {model} loaded and ready"));
                    self.save_state();
                }
                StreamEvent::ModelLoadFailed { model, error } => {
                    self.loading_model = None;
                    self.model_error = Some(error.clone());
                    self.log("ERROR", format!("Could not load {model}: {error}"));
                }
                StreamEvent::Finished => {
                    self.generating = false;
                    self.loading_model = None;
                    self.cancel_tx = None;
                    if let Some(id) = self.active_task.take() {
                        if let Some(task) = self.tasks.iter_mut().find(|task| task.id == id) {
                            task.done = true;
                        }
                    }
                    self.log("INFO", "Generation completed");
                    self.save_state();
                }
                StreamEvent::Cancelled => {
                    self.generating = false;
                    self.loading_model = None;
                    self.cancel_tx = None;
                    self.active_task = None;
                    self.log("WARN", "Generation cancelled by user");
                    self.save_state();
                }
                StreamEvent::Error(error) => {
                    self.generating = false;
                    self.loading_model = None;
                    self.cancel_tx = None;
                    self.active_task = None;
                    if self.models.is_empty() {
                        self.connection = ConnectionStatus::Error(error.clone());
                    }
                    if self.messages.last().is_some_and(|message| {
                        message.role == Role::Assistant && message.content.is_empty()
                    }) {
                        self.messages.pop();
                    }
                    self.messages.push(Message::new(
                        self.active_conversation,
                        Role::Assistant,
                        format!("Unable to respond: {error}"),
                    ));
                    self.log("ERROR", &error);
                    self.save_state();
                }
                StreamEvent::ModelsLoaded(models) => {
                    self.models = models
                        .into_iter()
                        .filter(|model| !model.to_lowercase().contains("embedding"))
                        .collect();
                    self.connection = ConnectionStatus::Connected;
                    if !self.models.contains(&self.selected_model) {
                        if self.models.iter().any(|model| model == DEFAULT_MODEL) {
                            self.selected_model = DEFAULT_MODEL.to_owned();
                        } else if let Some(model) = self.models.first() {
                            self.selected_model.clone_from(model);
                        }
                    }
                    self.log(
                        "INFO",
                        format!(
                            "Connected to Ollama; {} models available",
                            self.models.len()
                        ),
                    );
                    self.save_state();
                }
                StreamEvent::EmbeddingReady {
                    memory_id,
                    embedding,
                } => {
                    if let Some(memory) = self
                        .memories
                        .iter_mut()
                        .find(|memory| memory.id == memory_id)
                    {
                        memory.embedding = embedding;
                        self.log(
                            "INFO",
                            format!("Memory {memory_id} indexed for semantic recall"),
                        );
                        self.save_state();
                    }
                }
                StreamEvent::Notice(message) => self.log("WARN", message),
                StreamEvent::ToolActivity {
                    name,
                    detail,
                    success,
                } => {
                    self.log(
                        if success { "INFO" } else { "ERROR" },
                        format!("Tool {name}: {detail}"),
                    );
                    if let Some(message) = self.messages.last_mut() {
                        if message.role == Role::Assistant {
                            message.tool_uses.push((name, detail, success));
                        }
                    }
                    self.scroll_to_bottom = true;
                }
                StreamEvent::RestartApp => {
                    self.log("INFO", "Restart requested by agent tool");
                    self.force_quit = true;
                }
            }
        }
    }

    fn send(&mut self) {
        let input = self.input.trim().to_owned();
        if input.is_empty()
            || self.generating
            || self.loading_model.is_some()
            || self.models.is_empty()
        {
            return;
        }
        let task_title = format!("Respond to: {}", truncate(&input, 72));
        let task_id = self.next_id;
        self.next_id += 1;
        self.tasks.push(TaskItem {
            id: task_id,
            title: task_title,
            done: false,
        });
        self.active_task = Some(task_id);
        self.messages.push(Message::new(self.active_conversation, Role::User, input.clone()));
        self.messages.push(Message::new(self.active_conversation, Role::Assistant, ""));
        self.input.clear();
        self.generating = true;
        self.scroll_to_bottom = true;
        self.log(
            "INFO",
            format!("Generation requested with model {}", self.selected_model),
        );
        self.save_state();

        let history = self.messages[..self.messages.len() - 1].to_vec();
        let memories = self.memories.clone();
        let open_tasks = self
            .tasks
            .iter()
            .filter(|task| !task.done)
            .map(|task| task.title.clone())
            .collect::<Vec<_>>();
        let query = input;
        let model = self.selected_model.clone();
        let high_thinking = self.high_thinking;
        let workspace = self.workspace.clone();
        let skills = self.skills.clone();
        let client = self.client.clone();
        let events = self.events_tx.clone();
        let (cancel_tx, cancel_rx) = oneshot::channel();
        self.cancel_tx = Some(cancel_tx);
        let active_conversation = self.active_conversation;
        self.runtime.spawn(async move {
            let has_indexed_memory = memories.iter().any(|memory| !memory.embedding.is_empty());
            let selected_memories = if has_indexed_memory {
                match tokio::time::timeout(
                    Duration::from_secs(5),
                    client.embed(EMBEDDING_MODEL, &query),
                ).await {
                    Ok(Ok(query_vector)) => relevant_memories(&memories, &query_vector, 8),
                    Ok(Err(error)) => {
                        let _ = events.send(StreamEvent::Notice(format!("Semantic memory unavailable: {error}")));
                        recent_memories(&memories, 8)
                    }
                    Err(_) => {
                        let _ = events.send(StreamEvent::Notice("Semantic memory lookup timed out; using recent memory".to_owned()));
                        recent_memories(&memories, 8)
                    }
                }
            } else {
                recent_memories(&memories, 8)
            };
            let memory_text = selected_memories.iter().map(|memory| format!("- {memory}")).collect::<Vec<_>>().join("\n");
            let task_text = open_tasks.iter().map(|task| format!("- {task}")).collect::<Vec<_>>().join("\n");
            let skill_text = skills.iter().map(|skill| format!("- {}: {}", skill.name, skill.description)).collect::<Vec<_>>().join("\n");
            let mut contextual_history = vec![Message::new(active_conversation, Role::System, format!(
                "You are DeskPilot, a private local assistant with local tools. Use tools when they provide evidence or are needed to complete the request. Never claim a command ran or a file changed unless its tool succeeded. Use read_skill before following a listed skill. Shell tools are workspace-scoped and reject destructive commands. Use relevant saved memory when helpful. Never claim a memory exists unless it appears below.\n\nAVAILABLE SKILLS\n{}\n\nRELEVANT MEMORY\n{}\n\nOPEN TASKS\n{}",
                if skill_text.is_empty() { "None" } else { &skill_text },
                if memory_text.is_empty() { "None" } else { &memory_text },
                if task_text.is_empty() { "None" } else { &task_text },
            ))];
            contextual_history.extend(history);
            match client.is_model_loaded(&model).await {
                Ok(false) => {
                    let _ = events.send(StreamEvent::ModelLoading(model.clone()));
                }
                Ok(true) => {}
                Err(error) => {
                    let _ = events.send(StreamEvent::Notice(format!(
                        "Could not check model load state: {error}"
                    )));
                }
            }
            if let Err(error) = client.stream_chat(&model, &contextual_history, &events, cancel_rx, high_thinking, ToolContext { workspace: &workspace, skills: &skills }).await {
                let _ = events.send(StreamEvent::Error(error.to_string()));
            }
        });
    }

    fn stop(&mut self) {
        if let Some(cancel) = self.cancel_tx.take() {
            let _ = cancel.send(());
        }
    }

    fn switch_model(&mut self, previous: String, next: String) {
        if previous == next {
            return;
        }
        self.loading_model = Some(next.clone());
        self.model_error = None;
        self.log("INFO", format!("Switching model from {previous} to {next}"));
        self.save_state();
        let client = self.client.clone();
        let events = self.events_tx.clone();
        self.runtime.spawn(async move {
            let _ = events.send(StreamEvent::ModelLoading(next.clone()));
            match client.load_model(&next).await {
                Ok(()) => {
                    let _ = events.send(StreamEvent::ModelReady(next));
                }
                Err(error) => {
                    let _ = events.send(StreamEvent::ModelLoadFailed {
                        model: next,
                        error: error.to_string(),
                    });
                }
            }
        });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("DP")
                    .strong()
                    .color(Color32::from_rgb(16, 22, 12))
                    .background_color(ACCENT),
            );
            ui.label(RichText::new("DeskPilot").strong().size(17.0).color(TEXT));
        });
        ui.label(RichText::new("LOCAL ASSISTANT").small().color(MUTED));
        ui.add_space(16.0);
        if ui
            .add_sized(
                [215.0, 38.0],
                egui::Button::new("+  New chat").fill(SURFACE_HIGH),
            )
            .clicked()
        {
            let new_id = self.conversations.iter().map(|c| c.id).max().unwrap_or(0) + 1;
            self.conversations.push(Conversation {
                id: new_id,
                project_id: self.active_project,
                title: "New Chat".to_owned(),
                updated_at: 0,
            });
            self.active_conversation = new_id;
            self.view = View::Chat;
            self.log("INFO", "New conversation started");
            self.save_state();
        }
        ui.add_space(14.0);
        for (view, label) in [
            (View::Chat, "Chat"),
            (View::Tasks, "Tasks"),
            (View::Memory, "Memory"),
            (View::Skills, "Skills"),
            (View::Logs, "Logs"),
        ] {
            let selected = self.view == view;
            let button =
                egui::Button::new(RichText::new(label).color(if selected { ACCENT } else { TEXT }))
                    .fill(if selected {
                        SURFACE_HIGH
                    } else {
                        Color32::TRANSPARENT
                    })
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        if selected {
                            BORDER
                        } else {
                            Color32::TRANSPARENT
                        },
                    ));
            if ui.add_sized([215.0, 36.0], button).clicked() {
                self.view = view;
            }
        }
        ui.add_space(18.0);
        ui.label(RichText::new("CONVERSATIONS").small().strong().color(MUTED));
        ui.add_space(6.0);
        
        let mut to_delete = None;
        ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
            for convo in self.conversations.clone() {
                let is_active = convo.id == self.active_conversation && self.view == View::Chat;
                let fill = if is_active { SURFACE_HIGH } else { Color32::TRANSPARENT };
                let mut title = truncate(&convo.title, 24);
                if title.is_empty() { title = "Empty Chat".to_string(); }
                
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if ui.add_sized([183.0, 32.0], egui::Button::new(RichText::new(title).color(if is_active { ACCENT } else { TEXT })).fill(fill).truncate()).clicked() {
                        self.active_conversation = convo.id;
                        self.active_project = convo.project_id;
                        self.view = View::Chat;
                        self.save_state();
                    }
                    if ui.add_sized([28.0, 32.0], egui::Button::new(RichText::new("×").color(MUTED)).fill(Color32::TRANSPARENT)).clicked() {
                        to_delete = Some(convo.id);
                    }
                });
            }
        });
        if let Some(id) = to_delete {
            self.conversations.retain(|c| c.id != id);
            self.messages.retain(|m| m.conversation_id != id);
            if self.active_conversation == id {
                self.active_conversation = self.conversations.first().map(|c| c.id).unwrap_or(1);
            }
            self.save_state();
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            let done = self.tasks.iter().filter(|task| task.done).count();
            ui.label(
                RichText::new(format!(
                    "{} memories  |  {done}/{} tasks",
                    self.memories.len(),
                    self.tasks.len()
                ))
                .small()
                .color(MUTED),
            );
            ui.label(RichText::new("Private by default").small().color(ACCENT));
        });
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Context").strong().size(14.0).color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let color = if matches!(self.connection, ConnectionStatus::Connected) {
                    ACCENT
                } else {
                    Color32::YELLOW
                };
                ui.colored_label(color, "● Local");
            });
        });
        ui.add_space(12.0);
        ui.separator();
        inspector_label(ui, "CURRENT WORK");
        let active = self
            .active_task
            .and_then(|id| self.tasks.iter().find(|task| task.id == id));
        ui.label(
            RichText::new(
                active
                    .map(|task| task.title.as_str())
                    .unwrap_or("No active task"),
            )
            .color(TEXT),
        );
        let completed = self.tasks.iter().filter(|task| task.done).count();
        ui.label(
            RichText::new(format!("{completed}/{} tasks complete", self.tasks.len()))
                .small()
                .color(MUTED),
        );
        ui.add_space(18.0);
        ui.separator();
        inspector_label(ui, "MEMORY");
        let indexed = self
            .memories
            .iter()
            .filter(|memory| !memory.embedding.is_empty())
            .count();
        ui.label(RichText::new(format!("{} saved memories", self.memories.len())).color(TEXT));
        ui.label(
            RichText::new(format!("{indexed} semantically indexed"))
                .small()
                .color(if indexed > 0 { ACCENT } else { MUTED }),
        );
        ui.add_space(18.0);
        ui.separator();
        inspector_label(ui, "LOCAL DATA");
        ui.label(
            RichText::new(truncate(
                &self.storage.database_path().display().to_string(),
                34,
            ))
            .monospace()
            .small()
            .color(MUTED),
        );
        ui.add_space(14.0);
        egui::Frame::new()
            .fill(Color32::from_rgb(20, 31, 18))
            .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(48, 72, 39)))
            .corner_radius(8.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.label(RichText::new("Private by default").strong().color(ACCENT));
                ui.label(
                    RichText::new("Chats, tasks, memory vectors, and logs stay on this device.")
                        .small()
                        .color(MUTED),
                );
            });
    }

    fn header(&mut self, ui: &mut egui::Ui, title: &str, subtitle: &str) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading(RichText::new(title).color(TEXT));
                ui.label(RichText::new(subtitle).small().color(MUTED));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (color, label) = match &self.connection {
                    ConnectionStatus::Connecting => (Color32::YELLOW, "Connecting".to_owned()),
                    ConnectionStatus::Connected => (ACCENT, "Local".to_owned()),
                    ConnectionStatus::Error(error) => {
                        (DANGER, format!("Offline: {}", truncate(error, 30)))
                    }
                };
                ui.colored_label(color, format!("● {label}"));
            });
        });
        ui.add_space(8.0);
        ui.separator();
    }

    fn chat_input_pane(&mut self, ui: &mut egui::Ui) {
        let enter = ui.input(|input| input.key_pressed(egui::Key::Enter) && !input.modifiers.shift);
        
        egui::Frame::new()
            .fill(Color32::from_rgb(15, 18, 23)) // Very dark, modern input box
            .corner_radius(24.0)
            .inner_margin(egui::Margin::symmetric(20, 16))
            .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(35, 42, 53)))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    // Text Area
                    let width = (ui.available_width() - 20.0).max(120.0);
                    ui.add_sized(
                        [width, 24.0],
                        TextEdit::multiline(&mut self.input)
                            .hint_text("Ask anything...")
                            .frame(false)
                            .desired_rows(1)
                            .text_color(Color32::from_rgb(240, 245, 255)),
                    );
                    
                    ui.add_space(8.0);
                    
                    // Bottom Control Bar
                    ui.horizontal(|ui| {
                        // Left side: context pills
                        ui.spacing_mut().item_spacing.x = 8.0;
                        if ui.add(egui::Button::new(RichText::new("+").strong().color(TEXT)).fill(Color32::from_rgb(30, 35, 45)).corner_radius(12.0)).clicked() {
                            // Add context logic placeholder
                        }
                        
                        ui.add(egui::Button::new(RichText::new("Worktree").small().color(MUTED)).fill(Color32::from_rgb(20, 24, 30)).corner_radius(12.0).stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(35, 42, 53))));
                        
                        // Model dropdown pill
                        let current_model = if self.models.is_empty() { "No models" } else { &self.selected_model };
                        let previous_model = self.selected_model.clone();
                        egui::ComboBox::from_id_salt("model_dropdown")
                            .selected_text(RichText::new(current_model).small().color(MUTED))
                            .show_ui(ui, |ui| {
                                for model in &self.models {
                                    ui.selectable_value(&mut self.selected_model, model.clone(), model);
                                }
                            });
                        if previous_model != self.selected_model {
                            self.switch_model(previous_model, self.selected_model.clone());
                        }

                        // Right side: Send / Stop button
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if self.generating {
                                if ui.add_sized([40.0, 32.0], egui::Button::new(RichText::new("■").color(Color32::WHITE)).fill(Color32::from_rgb(180, 50, 60)).corner_radius(16.0)).clicked() {
                                    self.stop();
                                }
                            } else {
                                let can_send = !self.input.trim().is_empty() && !self.models.is_empty() && self.loading_model.is_none();
                                let btn_color = if can_send { ACCENT } else { Color32::from_rgb(30, 35, 45) };
                                let text_color = if can_send { Color32::BLACK } else { MUTED };
                                if ui.add_enabled(
                                    can_send,
                                    egui::Button::new(RichText::new("↑").color(text_color).strong()).fill(btn_color).corner_radius(16.0),
                                )
                                .clicked()
                                {
                                    self.send();
                                }
                            }
                        });
                    });
                });
            });
            
        if enter && !self.generating {
            while self.input.ends_with(['\r', '\n']) {
                self.input.pop();
            }
            self.send();
        }
    }

    fn chat_view(&mut self, ui: &mut egui::Ui) {
        // Breadcrumb Header
        ui.horizontal(|ui| {
            let project_name = self.projects.iter().find(|p| p.id == self.active_project).map(|p| p.name.clone()).unwrap_or_else(|| "Default".to_owned());
            let convo_title = self.conversations.iter().find(|c| c.id == self.active_conversation).map(|c| c.title.clone()).unwrap_or_else(|| "New Chat".to_owned());
            
            ui.label(RichText::new(&project_name).color(MUTED));
            ui.label(RichText::new("/").color(MUTED));
            ui.label(RichText::new(&convo_title).color(TEXT).strong());
            
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (color, label) = match &self.connection {
                    ConnectionStatus::Connecting => (Color32::YELLOW, "Connecting".to_owned()),
                    ConnectionStatus::Connected => (ACCENT, "Local".to_owned()),
                    ConnectionStatus::Error(error) => {
                        (DANGER, format!("Offline: {}", truncate(error, 30)))
                    }
                };
                ui.colored_label(color, format!("● {label}"));
            });
        });
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(12.0);

        // Messages Area
        ScrollArea::vertical()
            .id_salt("messages")
            .stick_to_bottom(self.scroll_to_bottom)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let active_messages: Vec<_> = self.messages.iter().filter(|m| m.conversation_id == self.active_conversation).collect();
                if active_messages.is_empty() {
                    ui.add_space(80.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("DeskPilot is ready").size(28.0).strong().color(TEXT));
                        ui.add_space(8.0);
                        ui.label(RichText::new("Ask a question, plan work, or save knowledge for later.").size(16.0).color(MUTED));
                    });
                }
                
                let last_active = active_messages.last().copied();
                for message in active_messages {
                    if message.role == Role::System || (message.content.is_empty() && message.thinking.is_empty() && message.tool_uses.is_empty()) {
                        continue;
                    }
                    
                    ui.add_space(20.0);
                    
                    if message.role == Role::User {
                        // Modern right-aligned user slate card
                        ui.horizontal(|ui| {
                            ui.add_space(ui.available_width() * 0.25); // Push to right, occupying max 75% width
                            ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                                egui::Frame::new()
                                    .fill(Color32::from_rgb(30, 41, 59)) // Modern dark slate
                                    .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(51, 65, 85)))
                                    .corner_radius(16.0)
                                    .inner_margin(egui::Margin::symmetric(14, 12))
                                    .show(ui, |ui| {
                                        ui.label(RichText::new(&message.content).color(Color32::WHITE).size(14.5));
                                    });
                            });
                        });
                    } else {
                        // Two-column flat assistant layout
                        ui.horizontal_top(|ui| {
                            // Circular DeskPilot avatar badge
                            let (avatar_text, bg_color, text_color) = ("DP", ACCENT, Color32::BLACK);
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 14.0, bg_color);
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                avatar_text,
                                egui::FontId::proportional(11.0),
                                text_color,
                            );
                            
                            ui.add_space(12.0);
                            
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("DeskPilot").strong().color(TEXT));
                                    if message.created_at > 0 {
                                        ui.label(RichText::new(format_message_time(message.created_at)).small().color(MUTED));
                                    }
                                });
                                ui.add_space(6.0);
                                
                                if !message.thinking.is_empty() {
                                    let is_active = self.generating && last_active.is_some_and(|last| std::ptr::eq(message, last));
                                    let header_text = if is_active { format!("⟳ Thinking...") } else { let words = message.thinking.split_whitespace().count(); format!("💭 Reasoning ({words} words) ›") };
                                    egui::CollapsingHeader::new(RichText::new(header_text).small().color(MUTED).italics())
                                    .default_open(is_active)
                                    .show(ui, |ui| {
                                        ui.label(RichText::new(&message.thinking).monospace().small().color(Color32::from_rgb(140, 140, 160)));
                                    });
                                    ui.add_space(8.0);
                                } else if message.content.is_empty() && message.tool_uses.is_empty() && self.generating {
                                    ui.label(RichText::new(if self.high_thinking { "⟳ Thinking..." } else { "Preparing a response..." }).italics().color(MUTED));
                                }
                                
                                if !message.tool_uses.is_empty() {
                                    let tool_count = message.tool_uses.len();
                                    let all_ok = message.tool_uses.iter().all(|(_, _, ok)| *ok);
                                    let is_active = self.generating && last_active.is_some_and(|last| std::ptr::eq(message, last));
                                    let header_text = if is_active { format!("⚙ Running tools ({tool_count})...") } else { let icon = if all_ok { "✓" } else { "⚠" }; format!("{icon} Used {tool_count} tool{} ›", if tool_count == 1 { "" } else { "s" }) };
                                    let header_color = if all_ok { Color32::from_rgb(100, 180, 120) } else { Color32::from_rgb(255, 180, 80) };
                                    egui::CollapsingHeader::new(RichText::new(header_text).small().strong().color(header_color))
                                    .default_open(is_active)
                                    .show(ui, |ui| {
                                        for (tool_name, detail, success) in &message.tool_uses {
                                            ui.horizontal(|ui| {
                                                let icon = if *success { "✓" } else { "✗" };
                                                let icon_color = if *success { Color32::from_rgb(100, 180, 120) } else { DANGER };
                                                ui.label(RichText::new(icon).color(icon_color).strong());
                                                ui.label(RichText::new(tool_name).monospace().small().strong().color(TEXT));
                                            });
                                            ui.label(RichText::new(detail).monospace().small().color(MUTED));
                                            ui.add_space(4.0);
                                        }
                                    });
                                    ui.add_space(8.0);
                                }
                                
                                if !message.content.is_empty() {
                                    ui.set_max_width(ui.available_width() * 0.95);
                                    ui.style_mut().url_in_tooltip = true;
                                    CommonMarkViewer::new().show(ui, &mut self.markdown_cache, &message.content);
                                }
                            });
                        });
                    }
                }
            });
        self.scroll_to_bottom = false;
    }

    fn tasks_view(&mut self, ui: &mut egui::Ui) {
        self.header(ui, "Tasks", "Track what you and DeskPilot are working on");
        ui.horizontal(|ui| {
            ui.add_sized(
                [ui.available_width() - 100.0, 34.0],
                TextEdit::singleline(&mut self.task_input).hint_text("Add a task..."),
            );
            if ui.button("Add task").clicked() && !self.task_input.trim().is_empty() {
                self.tasks.push(TaskItem {
                    id: self.next_id,
                    title: self.task_input.trim().to_owned(),
                    done: false,
                });
                self.next_id += 1;
                self.task_input.clear();
                self.log("INFO", "Task added");
                self.save_state();
            }
        });
        ui.add_space(12.0);
        let mut delete = None;
        let mut task_changed = false;
        ScrollArea::vertical().show(ui, |ui| {
            for task in &mut self.tasks {
                egui::Frame::new()
                    .fill(SURFACE_HIGH)
                    .corner_radius(8.0)
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut task.done, "").changed() {
                                task_changed = true;
                            }
                            let text = RichText::new(&task.title).color(if task.done {
                                MUTED
                            } else {
                                TEXT
                            });
                            ui.label(if task.done {
                                text.strikethrough()
                            } else {
                                text
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("Delete").clicked() {
                                        delete = Some(task.id);
                                    }
                                },
                            );
                        });
                    });
                ui.add_space(6.0);
            }
        });
        if let Some(id) = delete {
            self.tasks.retain(|task| task.id != id);
            self.save_state();
        }
        if task_changed {
            self.save_state();
        }
        if ui.button("Clear completed").clicked() {
            self.tasks.retain(|task| !task.done);
            self.save_state();
        }
    }

    fn memory_view(&mut self, ui: &mut egui::Ui) {
        self.header(
            ui,
            "Memory",
            "Durable local context included in future conversations",
        );
        ui.horizontal(|ui| {
            ui.add_sized(
                [ui.available_width() - 116.0, 34.0],
                TextEdit::singleline(&mut self.memory_input)
                    .hint_text("Save a preference, fact, or decision..."),
            );
            if ui.button("Save memory").clicked() && !self.memory_input.trim().is_empty() {
                let memory_id = self.next_id;
                let content = self.memory_input.trim().to_owned();
                self.memories.push(MemoryItem {
                    id: memory_id,
                    content: content.clone(),
                    embedding: Vec::new(),
                });
                self.next_id += 1;
                self.memory_input.clear();
                self.log("INFO", "Memory saved; indexing in background");
                self.save_state();
                let client = self.client.clone();
                let events = self.events_tx.clone();
                self.runtime.spawn(async move {
                    match client.embed(EMBEDDING_MODEL, &content).await {
                        Ok(embedding) => {
                            let _ = events.send(StreamEvent::EmbeddingReady {
                                memory_id,
                                embedding,
                            });
                        }
                        Err(error) => {
                            let _ = events.send(StreamEvent::Notice(format!(
                                "Could not index memory: {error}"
                            )));
                        }
                    }
                });
            }
        });
        ui.add_space(8.0);
        ui.add_sized(
            [ui.available_width(), 30.0],
            TextEdit::singleline(&mut self.memory_search).hint_text("Search memories..."),
        );
        ui.add_space(10.0);
        let query = self.memory_search.to_lowercase();
        let mut delete = None;
        ScrollArea::vertical().show(ui, |ui| {
            for memory in self
                .memories
                .iter()
                .filter(|memory| query.is_empty() || memory.content.to_lowercase().contains(&query))
            {
                egui::Frame::new()
                    .fill(SURFACE_HIGH)
                    .corner_radius(8.0)
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&memory.content).color(TEXT));
                                ui.label(
                                    RichText::new(if memory.embedding.is_empty() {
                                        "Pending semantic index"
                                    } else {
                                        "Semantically indexed"
                                    })
                                    .small()
                                    .color(
                                        if memory.embedding.is_empty() {
                                            Color32::YELLOW
                                        } else {
                                            ACCENT
                                        },
                                    ),
                                );
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("Delete").clicked() {
                                        delete = Some(memory.id);
                                    }
                                },
                            );
                        });
                    });
                ui.add_space(6.0);
            }
        });
        if let Some(id) = delete {
            self.memories.retain(|memory| memory.id != id);
            self.log("INFO", "Memory deleted");
            self.save_state();
        }
    }

    fn logs_view(&mut self, ui: &mut egui::Ui) {
        self.header(
            ui,
            "Logs",
            "Local diagnostic events for model and application activity",
        );
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(self.storage.database_path().display().to_string())
                    .small()
                    .color(MUTED),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Clear view").clicked() {
                    self.logs.clear();
                }
            });
        });
        ui.add_space(10.0);
        ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
            for entry in &self.logs {
                let color = match entry.level {
                    "ERROR" => DANGER,
                    "WARN" => Color32::YELLOW,
                    _ => ACCENT,
                };
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(entry.timestamp.to_string())
                            .monospace()
                            .small()
                            .color(MUTED),
                    );
                    ui.label(RichText::new(entry.level).monospace().small().color(color));
                    ui.label(
                        RichText::new(&entry.message)
                            .monospace()
                            .small()
                            .color(TEXT),
                    );
                });
                ui.separator();
            }
        });
    }

    fn skills_view(&mut self, ui: &mut egui::Ui) {
        self.header(
            ui,
            "Skills",
            "Claude-compatible local SKILL.md instructions and agent tools",
        );
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} skills discovered", self.skills.len())).color(TEXT));
            if ui.button("Reload").clicked() {
                self.skills = discover_skills(&self.workspace);
                self.log(
                    "INFO",
                    format!("Reloaded {} local skills", self.skills.len()),
                );
            }
        });
        ui.label(
            RichText::new("Sources: .claude/skills, ~/.claude/skills, ~/.codex/skills")
                .small()
                .color(MUTED),
        );
        ui.add_space(12.0);
        ScrollArea::vertical().show(ui, |ui| {
            for skill in &self.skills {
                egui::Frame::new()
                    .fill(SURFACE)
                    .corner_radius(8.0)
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.label(RichText::new(&skill.name).strong().color(ACCENT));
                        ui.label(RichText::new(&skill.description).color(TEXT));
                        ui.label(
                            RichText::new(skill.path.display().to_string())
                                .monospace()
                                .small()
                                .color(MUTED),
                        );
                    });
                ui.add_space(8.0);
            }
        });
    }
}

impl eframe::App for AiHelperApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) {
            if !self.force_quit {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
        }
        if self.force_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        
        while let Ok(msg) = self.ipc_rx.try_recv() {
            self.input = msg;
            self.send();
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        
        if let Ok(_) = tray_icon::TrayIconEvent::receiver().try_recv() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        
        self.poll_events();
        if self.generating || matches!(self.connection, ConnectionStatus::Connecting) {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
        // Custom title bar (full width, no padding, native-looking controls)
        egui::TopBottomPanel::top("titlebar")
            .exact_height(36.0)
            .frame(egui::Frame::new().fill(Color32::from_rgb(12, 12, 14)).inner_margin(egui::Margin::symmetric(0, 0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    
                    // Invisible draggable region
                    let drag_rect = ui.available_rect_before_wrap();
                    let drag_rect = egui::Rect::from_min_size(
                        drag_rect.min,
                        egui::vec2((drag_rect.width() - 138.0).max(0.0), 36.0),
                    );
                    let drag_response = ui.interact(drag_rect, ui.id().with("titlebar_drag"), egui::Sense::click_and_drag());
                    if drag_response.dragged() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if drag_response.double_clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(
                            !ctx.input(|i| i.viewport().maximized.unwrap_or(false))
                        ));
                    }
                    
                    // Allocate layout space for the drag zone
                    ui.allocate_space(egui::vec2((ui.available_width() - 138.0).max(0.0), 36.0));
                    
                    // Right-aligned modern window control buttons (46px wide each)
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        // Close
                        if titlebar_button(ui, "✕", Color32::from_rgb(232, 17, 35), TEXT).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                        }
                        // Maximize
                        let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                        let max_symbol = if is_max { "❐" } else { "□" };
                        if titlebar_button(ui, max_symbol, Color32::from_rgb(45, 50, 60), MUTED).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
                        }
                        // Minimize
                        if titlebar_button(ui, "─", Color32::from_rgb(45, 50, 60), MUTED).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                    });
                });
            });

        egui::SidePanel::left("sidebar")
            .exact_width(240.0)
            .frame(egui::Frame::new()
                .fill(SIDEBAR)
                .inner_margin(12.0)
                .stroke(egui::Stroke::new(1.0_f32, BORDER)))
            .show(ctx, |ui| self.sidebar(ui));

        egui::SidePanel::right("inspector")
            .exact_width(270.0)
            .frame(egui::Frame::new()
                .fill(SIDEBAR)
                .inner_margin(14.0)
                .stroke(egui::Stroke::new(1.0_f32, BORDER)))
            .show(ctx, |ui| self.inspector(ui));

        // If Chat is active, draw the bottom input container before the CentralPanel
        if self.view == View::Chat {
            egui::TopBottomPanel::bottom("chat_input_panel")
                .frame(egui::Frame::none().fill(CANVAS).inner_margin(egui::Margin { left: 20, right: 20, top: 0, bottom: 20 }))
                .show(ctx, |ui| {
                    self.chat_input_pane(ui);
                });
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(CANVAS).inner_margin(20.0))
            .show(ctx, |ui| match self.view {
                View::Chat => self.chat_view(ui),
                View::Tasks => self.tasks_view(ui),
                View::Memory => self.memory_view(ui),
                View::Skills => self.skills_view(ui),
                View::Logs => self.logs_view(ui),
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_state();
    }
}

fn configure_style(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(12.0, 12.0);
    style.spacing.button_padding = egui::vec2(16.0, 10.0);
    style.visuals.panel_fill = Color32::from_rgb(18, 18, 20); // Deep Charcoal
    style.visuals.window_fill = Color32::from_rgb(28, 28, 31); // Elevated Card
    style.visuals.extreme_bg_color = Color32::from_rgb(28, 28, 31);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(38, 38, 42);
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(12);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(48, 48, 52);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(12);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(58, 58, 62);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(12);
    style.visuals.selection.bg_fill = Color32::from_rgb(57, 255, 20); // Neon Green Selection
    style.visuals.window_corner_radius = egui::CornerRadius::same(16);
    style.visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 8],
        blur: 16,
        spread: 0,
        color: Color32::from_black_alpha(80),
    };
    ctx.set_style(style);
}

fn truncate(value: &str, max: usize) -> String {
    let mut chars = value.chars();
    let shortened = chars.by_ref().take(max).collect::<String>();
    if chars.next().is_some() {
        format!("{shortened}...")
    } else {
        shortened
    }
}

fn inspector_label(ui: &mut egui::Ui, label: &str) {
    ui.add_space(8.0);
    ui.label(
        RichText::new(label)
            .small()
            .strong()
            .color(Color32::from_rgb(104, 121, 143)),
    );
    ui.add_space(6.0);
}

fn relevant_memories(memories: &[MemoryItem], query: &[f32], limit: usize) -> Vec<String> {
    let mut ranked = memories
        .iter()
        .filter_map(|memory| {
            if memory.embedding.len() != query.len() || query.is_empty() {
                return None;
            }
            Some((
                cosine_similarity(&memory.embedding, query),
                memory.content.clone(),
            ))
        })
        .filter(|(score, _)| *score > 0.25)
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.0.total_cmp(&left.0));
    ranked
        .into_iter()
        .take(limit)
        .map(|(_, content)| content)
        .collect()
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    let dot = left.iter().zip(right).map(|(a, b)| a * b).sum::<f32>();
    let left_norm = left.iter().map(|value| value * value).sum::<f32>().sqrt();
    let right_norm = right.iter().map(|value| value * value).sum::<f32>().sqrt();
    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        dot / (left_norm * right_norm)
    }
}

fn recent_memories(memories: &[MemoryItem], limit: usize) -> Vec<String> {
    memories
        .iter()
        .rev()
        .take(limit)
        .map(|memory| memory.content.clone())
        .collect()
}

fn format_message_time(timestamp: i64) -> String {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|time| time.format("%I:%M %p").to_string())
        .unwrap_or_default()
}

fn titlebar_button(ui: &mut egui::Ui, text: &str, hover_bg: Color32, text_color: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(46.0, 36.0), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let is_hovered = response.hovered();
        let bg_color = if is_hovered { hover_bg } else { Color32::TRANSPARENT };
        ui.painter().rect_filled(rect, egui::CornerRadius::ZERO, bg_color);
        
        let final_text_color = if is_hovered { Color32::WHITE } else { text_color };
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(12.0),
            final_text_color
        );
    }
    response
}
