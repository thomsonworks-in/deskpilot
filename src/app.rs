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
    Skills,
    Logs,
    Settings,
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
    sidebar_collapsed: bool,
    show_add_project_dialog: bool,
    new_project_name: String,
    show_pull_dialog: bool,
    pull_model_input: String,
    pull_status: Option<String>,
    pull_progress: Option<(u64, u64)>,
    cloud_providers: Vec<crate::message::ProviderConfig>,
    active_provider: String,
    control_api_url: String,
    openrouter_key: String,
    openrouter_model: String,
    settings_saved_notice: Option<String>,
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
            sidebar_collapsed: false,
            show_add_project_dialog: false,
            new_project_name: String::new(),
            show_pull_dialog: false,
            pull_model_input: String::new(),
            pull_status: None,
            pull_progress: None,
            cloud_providers: Vec::new(),
            active_provider: "Local Ollama".to_owned(),
            control_api_url: "http://127.0.0.1:8000".to_owned(),
            openrouter_key: String::new(),
            openrouter_model: "anthropic/claude-3.7-sonnet".to_owned(),
            settings_saved_notice: None,
        };

        // Load persisted OpenRouter settings
        if let Ok(Some(key)) = app.storage.get_setting("openrouter_key") {
            app.openrouter_key = key;
        } else if let Ok(env_key) = std::env::var("OPENROUTER_API_KEY") {
            app.openrouter_key = env_key;
        }
        if let Ok(Some(model)) = app.storage.get_setting("openrouter_model") {
            app.openrouter_model = model;
        }
        if let Ok(Some(provider)) = app.storage.get_setting("active_provider") {
            app.active_provider = provider;
        }

        // Register default OpenRouter cloud provider
        app.cloud_providers.push(crate::message::ProviderConfig {
            name: "OpenRouter".to_owned(),
            base_url: "https://openrouter.ai/api/v1".to_owned(),
            models: vec![
                "anthropic/claude-3.7-sonnet".to_owned(),
                "deepseek/deepseek-r1".to_owned(),
                "deepseek/deepseek-chat".to_owned(),
                "openai/gpt-4o".to_owned(),
                "meta-llama/llama-3.3-70b-instruct".to_owned(),
                "google/gemini-2.0-flash-001".to_owned(),
            ],
            api_key: if app.openrouter_key.is_empty() { None } else { Some(app.openrouter_key.clone()) },
        });

        // Bootstrap providers from Control server in background
        let bootstrap_client = app.client.clone();
        let bootstrap_events = app.events_tx.clone();
        let control_url = app.control_api_url.clone();
        app.runtime.spawn(async move {
            if let Ok(providers) = bootstrap_client.fetch_control_providers(&control_url).await {
                let _ = bootstrap_events.send(StreamEvent::ProvidersLoaded(providers));
            }
        });

        // If OpenRouter key is set or available, fetch dynamic OpenRouter model catalog
        let or_client = app.client.clone();
        let or_events = app.events_tx.clone();
        let or_key = app.openrouter_key.clone();
        app.runtime.spawn(async move {
            if let Ok(models) = or_client.fetch_openrouter_models(&or_key).await {
                let _ = or_events.send(StreamEvent::OpenRouterModelsLoaded(models));
            }
        });

        app.log("INFO", "DeskPilot started with Adaptive Memory & Tool Harness");
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
                StreamEvent::PullProgress { status, completed, total } => {
                    self.pull_status = Some(status);
                    if total > 0 {
                        self.pull_progress = Some((completed, total));
                    }
                }
                StreamEvent::PullFinished(model) => {
                    self.pull_status = Some(format!("Downloaded {model}"));
                    self.pull_progress = None;
                    let reload_client = self.client.clone();
                    let reload_events = self.events_tx.clone();
                    self.runtime.spawn(async move {
                        if let Ok(models) = reload_client.list_models().await {
                            let _ = reload_events.send(StreamEvent::ModelsLoaded(models));
                        }
                    });
                }
                StreamEvent::ProvidersLoaded(providers) => {
                    self.log("INFO", format!("Loaded {} providers from Control API", providers.len()));
                    self.cloud_providers = providers;
                }
                StreamEvent::OpenRouterModelsLoaded(models) => {
                    self.log("INFO", format!("Retrieved {} dynamic models from OpenRouter catalog", models.len()));
                    if let Some(p) = self.cloud_providers.iter_mut().find(|p| p.name == "OpenRouter") {
                        p.models = models;
                    }
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
        let _ = self.storage.record_recent_model(&self.selected_model);
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
        let active_project = self.active_project;
        let active_provider = self.active_provider.clone();
        let cloud_providers = self.cloud_providers.clone();
        let adaptive_memory = self.storage.get_adaptive_context(active_project);
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
                "You are DeskPilot, an elite native autonomous coding & research agent equipped with local tools. When asked about current information, news, models, documentation, or online data, ALWAYS use the `web_search` or `curl` tool to retrieve real-time facts instead of giving knowledge cutoff disclaimers. Use `powershell` for shell commands and `read_file`/`write_file` for files. Maintain O(1) memory awareness.\n\nAVAILABLE SKILLS\n{}\n\nRELEVANT MEMORY\n{}\n\nOPEN TASKS\n{}{}",
                if skill_text.is_empty() { "None" } else { &skill_text },
                if memory_text.is_empty() { "None" } else { &memory_text },
                if task_text.is_empty() { "None" } else { &task_text },
                adaptive_memory
            ))];
            contextual_history.extend(history);

            if active_provider != "Local Ollama" {
                if let Some(provider) = cloud_providers.iter().find(|p| p.name == active_provider) {
                    let api_key = provider.api_key.as_deref().unwrap_or("sk-local");
                    let _ = client.stream_cloud_chat(
                        &provider.base_url,
                        api_key,
                        &model,
                        &contextual_history,
                        &events,
                        cancel_rx,
                        ToolContext {
                            workspace: &workspace,
                            skills: &skills,
                        },
                    ).await;
                    return;
                }
            }

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
        let _ = self.storage.record_recent_model(&next);
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
        if self.sidebar_collapsed {
            // Collapsed layout (60px wide)
            ui.vertical_centered(|ui| {
                // Expand toggle button (hamburger style or ▶)
                if ui.add(egui::Button::new(RichText::new("☰").size(16.0)).fill(Color32::TRANSPARENT)).clicked() {
                    self.sidebar_collapsed = false;
                }
                
                ui.add_space(20.0);
                
                // Circular "+" button for new chat
                let btn = egui::Button::new(RichText::new("+").strong().size(14.0))
                    .fill(SURFACE_HIGH)
                    .corner_radius(16.0);
                if ui.add_sized([32.0, 32.0], btn).clicked() {
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
                
                ui.add_space(20.0);
                
                // Icon-only view buttons
                for (view, icon, tooltip) in [
                    (View::Chat, "💬", "Chat"),
                    (View::Skills, "🛠", "Skills"),
                    (View::Logs, "📝", "Logs"),
                    (View::Settings, "⚙", "Settings & API Keys"),
                ] {
                    let selected = self.view == view;
                    let fill = if selected { SURFACE_HIGH } else { Color32::TRANSPARENT };
                    let stroke = egui::Stroke::new(1.0_f32, if selected { BORDER } else { Color32::TRANSPARENT });
                    let btn = egui::Button::new(RichText::new(icon).size(14.0).color(if selected { ACCENT } else { TEXT }))
                        .fill(fill)
                        .stroke(stroke)
                        .corner_radius(8.0);
                    if ui.add_sized([36.0, 36.0], btn).on_hover_text(tooltip).clicked() {
                        self.view = view;
                    }
                    ui.add_space(8.0);
                }
            });
        } else {
            // Expanded layout (240px wide)
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("DP")
                        .strong()
                        .color(Color32::from_rgb(16, 22, 12))
                        .background_color(ACCENT),
                );
                ui.label(RichText::new("DeskPilot").strong().size(17.0).color(TEXT));
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(RichText::new("◀").size(11.0)).fill(Color32::TRANSPARENT)).clicked() {
                        self.sidebar_collapsed = true;
                    }
                });
            });
            ui.label(RichText::new("LOCAL ASSISTANT").small().color(MUTED));
            ui.add_space(14.0);

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
            
            for (view, icon, label) in [
                (View::Chat, "💬", "Chat"),
                (View::Skills, "🛠", "Skills"),
                (View::Logs, "📝", "Logs"),
                (View::Settings, "⚙", "Settings"),
            ] {
                let selected = self.view == view;
                let (rect, response) = ui.allocate_exact_size(egui::vec2(215.0, 36.0), egui::Sense::click());
                let is_hovered = response.hovered();
                let bg_color = if selected { SURFACE_HIGH } else if is_hovered { Color32::from_rgb(25, 30, 40) } else { Color32::TRANSPARENT };
                ui.painter().rect_filled(rect, 8.0, bg_color);
                if selected {
                    ui.painter().rect_stroke(rect, 8.0, egui::Stroke::new(1.0_f32, BORDER), egui::StrokeKind::Outside);
                }
                
                // Draw label left-aligned inside the rect with padding
                let text_pos = rect.left_center() + egui::vec2(12.0, 0.0);
                ui.painter().text(
                    text_pos,
                    egui::Align2::LEFT_CENTER,
                    format!("{icon}  {label}"),
                    egui::FontId::proportional(14.0),
                    if selected { ACCENT } else { TEXT }
                );
                
                if response.clicked() {
                    self.view = view;
                }
            }
            ui.add_space(18.0);
            
            // Header for PROJECTS & CONVERSATIONS
            ui.horizontal(|ui| {
                ui.label(RichText::new("PROJECTS").small().strong().color(MUTED));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let btn_plus = egui::Button::new(RichText::new("+").strong().color(MUTED)).fill(Color32::TRANSPARENT);
                    if ui.add(btn_plus).on_hover_text("Add Project").clicked() {
                        self.show_add_project_dialog = true;
                    }
                });
            });
            ui.add_space(6.0);
            
            // Hierarchical Projects and Conversations ScrollArea
            ScrollArea::vertical().max_height(250.0).show(ui, |ui| {
                ui.set_width(ui.available_width().max(0.0));
                let projects = self.projects.clone();
                for project in projects {
                    let is_active_project = project.id == self.active_project;
                    
                    // Project Row
                    let (p_rect, p_response) = ui.allocate_exact_size(egui::vec2(215.0, 30.0), egui::Sense::click());
                    let p_hovered = p_response.hovered();
                    let p_bg = if is_active_project { Color32::from_rgb(25, 30, 40) } else if p_hovered { Color32::from_rgb(20, 24, 32) } else { Color32::TRANSPARENT };
                    ui.painter().rect_filled(p_rect, 6.0, p_bg);
                    
                    let p_text_pos = p_rect.left_center() + egui::vec2(8.0, 0.0);
                    ui.painter().text(
                        p_text_pos,
                        egui::Align2::LEFT_CENTER,
                        format!("📁 {}", project.name),
                        egui::FontId::proportional(13.0),
                        if is_active_project { TEXT } else { MUTED }
                    );
                    
                    if p_response.clicked() {
                        self.active_project = project.id;
                        // Select first conversation in this project
                        if let Some(convo) = self.conversations.iter().find(|c| c.project_id == project.id) {
                            self.active_conversation = convo.id;
                        } else {
                            // Create default conversation if none exist
                            let new_id = self.conversations.iter().map(|c| c.id).max().unwrap_or(0) + 1;
                            self.conversations.push(Conversation {
                                id: new_id,
                                project_id: project.id,
                                title: "Main Chat".to_owned(),
                                updated_at: 0,
                            });
                            self.active_conversation = new_id;
                        }
                        self.view = View::Chat;
                        self.save_state();
                    }
                    
                    // Conversations indented under project
                    let project_convos: Vec<_> = self.conversations
                        .iter()
                        .filter(|c| c.project_id == project.id)
                        .cloned()
                        .collect();
                        
                    for convo in project_convos {
                        let is_active_convo = convo.id == self.active_conversation && self.view == View::Chat;
                        let mut title = truncate(&convo.title, 22);
                        if title.is_empty() { title = "Empty Chat".to_string(); }
                        
                        let (c_rect, c_response) = ui.allocate_exact_size(egui::vec2(215.0, 26.0), egui::Sense::click());
                        let c_hovered = c_response.hovered();
                        let c_bg = if is_active_convo { SURFACE_HIGH } else if c_hovered { Color32::from_rgb(20, 25, 35) } else { Color32::TRANSPARENT };
                        ui.painter().rect_filled(c_rect, 6.0, c_bg);
                        if is_active_convo {
                            ui.painter().rect_stroke(c_rect, 6.0, egui::Stroke::new(1.0_f32, BORDER), egui::StrokeKind::Outside);
                        }
                        
                        let c_text_pos = c_rect.left_center() + egui::vec2(24.0, 0.0);
                        ui.painter().text(
                            c_text_pos,
                            egui::Align2::LEFT_CENTER,
                            title,
                            egui::FontId::proportional(12.0),
                            if is_active_convo { ACCENT } else { MUTED }
                        );
                        
                        if c_response.clicked() {
                            self.active_conversation = convo.id;
                            self.active_project = convo.project_id;
                            self.view = View::Chat;
                            self.save_state();
                        }
                    }
                    ui.add_space(4.0);
                }
            });
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
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Private by default").small().color(ACCENT));
                });
            });
        }
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        
        // 1. Connection status
        ui.horizontal(|ui| {
            ui.label(RichText::new("Context").strong().size(14.0).color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (color, label) = match &self.connection {
                    ConnectionStatus::Connecting => (Color32::YELLOW, "Connecting".to_owned()),
                    ConnectionStatus::Connected => (ACCENT, "Local".to_owned()),
                    ConnectionStatus::Error(error) => (DANGER, format!("Offline: {}", truncate(error, 15))),
                };
                ui.colored_label(color, label);
                ui.add_space(6.0);
                let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.0, color);
            });
        });
        ui.add_space(10.0);
        ui.separator();
        
        // 2. CURRENT WORK (Todo list of the agent)
        inspector_label(ui, "AGENT TODO LIST");
        
        // Task list
        let active_tasks: Vec<_> = self.tasks.clone();
        if active_tasks.is_empty() {
            ui.label(RichText::new("No active operations").small().color(MUTED));
        } else {
            ScrollArea::vertical()
                .id_salt("inspector_tasks_scroll")
                .max_height(200.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width().max(0.0));
                    for task in active_tasks {
                        ui.horizontal(|ui| {
                            let dot_color = if task.done { ACCENT } else { Color32::from_rgb(100, 140, 240) };
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                            if task.done {
                                // Draw a checkmark inside a filled box
                                ui.painter().rect_filled(rect, 3.0, SURFACE_HIGH);
                                ui.painter().rect_stroke(rect, 3.0, egui::Stroke::new(1.0, dot_color), egui::StrokeKind::Outside);
                                let center = rect.center();
                                ui.painter().line_segment(
                                    [center + egui::vec2(-3.0, 0.0), center + egui::vec2(-1.0, 2.0)],
                                    egui::Stroke::new(1.5, dot_color),
                                );
                                ui.painter().line_segment(
                                    [center + egui::vec2(-1.0, 2.0), center + egui::vec2(3.0, -2.0)],
                                    egui::Stroke::new(1.5, dot_color),
                                );
                            } else {
                                // Draw empty square outline
                                ui.painter().rect_stroke(
                                    rect,
                                    3.0,
                                    egui::Stroke::new(1.0, Color32::from_rgb(100, 110, 130)),
                                    egui::StrokeKind::Outside,
                                );
                            }
                            ui.add_space(8.0);
                            
                            // High-contrast text sizing
                            let text = RichText::new(task.title).size(13.0).color(if task.done { MUTED } else { TEXT });
                            ui.label(if task.done { text.strikethrough() } else { text });
                        });
                        ui.add_space(6.0);
                    }
                });
        }
        
        ui.add_space(12.0);
        ui.separator();
        
        // 3. MEMORY (Durable local preferences)
        inspector_label(ui, "AGENT MEMORY STATS");
        
        // Stats block
        egui::Frame::new()
            .fill(SURFACE_HIGH)
            .corner_radius(8.0)
            .inner_margin(10.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width().max(0.0));
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Stored Facts:").small().color(MUTED));
                    ui.label(RichText::new(format!("{}", self.memories.len())).small().strong().color(TEXT));
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("RAM Usage:").small().color(MUTED));
                    ui.label(RichText::new("14.2 GB / 32.0 GB").small().color(TEXT));
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("VRAM Usage:").small().color(MUTED));
                    ui.label(RichText::new("6.8 GB / 12.0 GB (Local LLM)").small().color(TEXT));
                });
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
                ui.colored_label(color, label);
                ui.add_space(6.0);
                let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.0, color);
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
                        ui.spacing_mut().interact_size.y = 30.0;
                        ui.spacing_mut().button_padding = egui::vec2(10.0, 6.0);
                        ui.spacing_mut().item_spacing.x = 8.0;

                        // Left side: context pills & download button
                        let btn_plus = egui::Button::new(RichText::new("+").strong().color(TEXT))
                            .fill(Color32::from_rgb(30, 35, 45))
                            .corner_radius(15.0);
                        if ui.add_sized([30.0, 30.0], btn_plus).on_hover_text("Add Context").clicked() {
                            // Add context logic placeholder
                        }

                        let btn_pull = egui::Button::new(RichText::new("⬇").strong().color(ACCENT))
                            .fill(Color32::from_rgb(30, 35, 45))
                            .corner_radius(15.0);
                        if ui.add_sized([30.0, 30.0], btn_pull).on_hover_text("Download / Pull Model").clicked() {
                            self.show_pull_dialog = !self.show_pull_dialog;
                        }

                        // Provider / Harness Switcher Dropdown
                        let current_provider = self.active_provider.clone();
                        egui::ComboBox::from_id_salt("provider_dropdown")
                            .selected_text(RichText::new(format!("🌐 {}", self.active_provider)).small().color(ACCENT))
                            .width(130.0)
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(self.active_provider == "Local Ollama", "🖥 Local Ollama").clicked() {
                                    self.active_provider = "Local Ollama".to_owned();
                                    let _ = self.storage.set_setting("active_provider", "Local Ollama");
                                }
                                for p in &self.cloud_providers {
                                    let is_active = self.active_provider == p.name;
                                    let icon = if p.name.contains("OpenRouter") { "⚡" } else { "☁" };
                                    if ui.selectable_label(is_active, format!("{icon} {}", p.name)).clicked() {
                                        self.active_provider = p.name.clone();
                                        let _ = self.storage.set_setting("active_provider", &p.name);
                                    }
                                }
                            });

                        // Model dropdown pill
                        let current_model = if self.active_provider != "Local Ollama" {
                            if self.selected_model.is_empty() { "anthropic/claude-3.7-sonnet" } else { &self.selected_model }
                        } else if self.models.is_empty() {
                            "No models"
                        } else {
                            &self.selected_model
                        };
                        let mode_suffix = if current_model.contains("gemma") {
                            " (Fast) ⚡"
                        } else if current_model.contains("ornith") || current_model.contains("llama") {
                            " (Thinking) 💡"
                        } else if self.models.is_empty() {
                            ""
                        } else {
                            " (Medium) ⚙"
                        };
                        let selected_btn_text = format!("🤖 {}{}", current_model, mode_suffix);
                        
                        let models = if self.active_provider == "Local Ollama" {
                            self.models.clone()
                        } else if let Some(p) = self.cloud_providers.iter().find(|p| p.name == self.active_provider) {
                            p.models.clone()
                        } else {
                            vec!["anthropic/claude-3.7-sonnet".to_owned(), "deepseek/deepseek-r1".to_owned(), "openai/gpt-4o".to_owned()]
                        };
                        let previous_model = self.selected_model.clone();
                        let recent_models = self.storage.get_recent_models().unwrap_or_default();
                        
                        // Curated lists
                        let frontier_models = [
                            "anthropic/claude-3.7-sonnet",
                            "deepseek/deepseek-r1",
                            "openai/gpt-4o",
                            "google/gemini-2.0-flash-001",
                        ];
                        let budget_models = [
                            "deepseek/deepseek-chat",
                            "meta-llama/llama-3.3-70b-instruct",
                            "qwen/qwen-2.5-coder-32b-instruct",
                            "google/gemini-2.0-flash-lite-preview-02-05:free",
                        ];

                        egui::ComboBox::from_id_salt("model_dropdown")
                            .selected_text(RichText::new(selected_btn_text).small().color(TEXT))
                            .width(210.0)
                            .show_ui(ui, |ui| {
                                egui::ScrollArea::vertical().max_height(350.0).show(ui, |ui| {
                                    let mut render_section = |ui: &mut egui::Ui, title: &str, items: &[String], selected_model: &mut String, switch_flag: &mut Option<(String, String)>| {
                                        if items.is_empty() {
                                            return;
                                        }
                                        ui.add_space(4.0);
                                        ui.label(RichText::new(title).small().strong().color(Color32::from_rgb(140, 160, 190)));
                                        ui.add_space(2.0);

                                        for model in items {
                                            let is_selected = *selected_model == *model;
                                            let (rect, response) = ui.allocate_exact_size(egui::vec2(250.0, 26.0), egui::Sense::click());
                                            let is_hovered = response.hovered();
                                            let bg_color = if is_selected { SURFACE_HIGH } else if is_hovered { Color32::from_rgb(30, 37, 48) } else { Color32::TRANSPARENT };
                                            ui.painter().rect_filled(rect, 4.0, bg_color);
                                            
                                            // Draw model name
                                            let display_name = truncate(model, 26);
                                            ui.painter().text(
                                                rect.left_center() + egui::vec2(8.0, 0.0),
                                                egui::Align2::LEFT_CENTER,
                                                display_name,
                                                egui::FontId::proportional(11.5),
                                                if is_selected { ACCENT } else { TEXT }
                                            );
                                            
                                            // Draw badge right-aligned
                                            let (mode_name, tag_color, tag_icon) = if model.contains("gemma") || model.contains("flash") {
                                                ("Fast", Color32::from_rgb(70, 180, 90), "⚡")
                                            } else if model.contains("r1") || model.contains("thinking") || model.contains("ornith") {
                                                ("Reasoning", Color32::from_rgb(220, 160, 40), "💡")
                                            } else if model.contains("claude") || model.contains("gpt-4") {
                                                ("Frontier", Color32::from_rgb(220, 120, 70), "🏆")
                                            } else if model.contains("free") || model.contains("deepseek-chat") {
                                                ("Budget", Color32::from_rgb(120, 200, 120), "💰")
                                            } else {
                                                ("Model", Color32::from_rgb(80, 120, 220), "⚙")
                                            };
                                            
                                            let badge_rect = egui::Rect::from_center_size(
                                                rect.right_center() - egui::vec2(40.0, 0.0),
                                                egui::vec2(68.0, 15.0)
                                            );
                                            ui.painter().rect_filled(badge_rect, 3.0, Color32::from_rgb(25, 30, 40));
                                            ui.painter().rect_stroke(badge_rect, 3.0, egui::Stroke::new(1.0_f32, Color32::from_rgb(45, 52, 65)), egui::StrokeKind::Outside);
                                            
                                            ui.painter().text(
                                                badge_rect.center(),
                                                egui::Align2::CENTER_CENTER,
                                                format!("{} {}", mode_name, tag_icon),
                                                egui::FontId::proportional(8.5),
                                                tag_color
                                            );
                                            
                                            if response.clicked() {
                                                *switch_flag = Some((selected_model.clone(), model.clone()));
                                                *selected_model = model.clone();
                                                ui.close_menu();
                                            }
                                        }
                                        ui.add_space(3.0);
                                    };

                                    let mut pending_switch: Option<(String, String)> = None;

                                    // Section 1: ⭐ Recently Used
                                    if !recent_models.is_empty() {
                                        render_section(ui, "⭐ RECENTLY USED", &recent_models, &mut self.selected_model, &mut pending_switch);
                                    }

                                    // Section 2: 🏆 Frontier Models (for cloud)
                                    if self.active_provider != "Local Ollama" {
                                        let frontier_vec: Vec<String> = frontier_models.iter().map(|s| (*s).to_string()).collect();
                                        render_section(ui, "🏆 FRONTIER MODELS", &frontier_vec, &mut self.selected_model, &mut pending_switch);

                                        let budget_vec: Vec<String> = budget_models.iter().map(|s| (*s).to_string()).collect();
                                        render_section(ui, "💰 BUDGET & FREE MODELS", &budget_vec, &mut self.selected_model, &mut pending_switch);
                                    }

                                    // Section 3/4: All Models Catalog
                                    let all_header = if self.active_provider == "Local Ollama" {
                                        "🖥 LOCAL OLLAMA MODELS"
                                    } else {
                                        "🌐 ALL AVAILABLE MODELS"
                                    };
                                    render_section(ui, all_header, &models, &mut self.selected_model, &mut pending_switch);

                                    if let Some((prev, next)) = pending_switch {
                                        self.switch_model(prev, next);
                                    }
                                });
                            });

                        // Right side: Send / Stop button
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if self.generating {
                                let btn_stop = egui::Button::new(RichText::new("■").color(Color32::WHITE))
                                    .fill(Color32::from_rgb(180, 50, 60))
                                    .corner_radius(15.0);
                                if ui.add_sized([30.0, 30.0], btn_stop).clicked() {
                                    self.stop();
                                }
                            } else {
                                let has_active_model = if self.active_provider == "Local Ollama" {
                                    !self.models.is_empty()
                                } else {
                                    !self.selected_model.is_empty()
                                };
                                let can_send = !self.input.trim().is_empty() && has_active_model && self.loading_model.is_none();
                                let btn_color = if can_send { ACCENT } else { Color32::from_rgb(30, 35, 45) };
                                let text_color = if can_send { Color32::BLACK } else { MUTED };
                                
                                let btn_send = egui::Button::new(RichText::new("↑").color(text_color).strong())
                                    .fill(btn_color)
                                    .corner_radius(15.0);
                                if ui.add_sized([30.0, 30.0], btn_send).clicked() && can_send {
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
                ui.colored_label(color, label);
                ui.add_space(6.0);
                let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.0, color);
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
                ui.set_width(ui.available_width().max(0.0));
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
                            ui.add_space((ui.available_width() * 0.25).max(0.0)); // Push to right, occupying max 75% width
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

    fn settings_view(&mut self, ui: &mut egui::Ui) {
        self.header(
            ui,
            "Settings & Providers",
            "Configure cloud API keys, OpenRouter routing, and local Ollama connections",
        );

        ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(8.0);

            // Active Provider Selection Card
            egui::Frame::new()
                .fill(SURFACE)
                .corner_radius(10.0)
                .inner_margin(16.0)
                .stroke(egui::Stroke::new(1.0_f32, BORDER))
                .show(ui, |ui| {
                    ui.label(RichText::new("🌐 Active Execution Engine / Provider").strong().color(TEXT));
                    ui.add_space(4.0);
                    ui.label(RichText::new("Choose whether DeskPilot routes requests through local Ollama or cloud providers like OpenRouter.").small().color(MUTED));
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        let is_local = self.active_provider == "Local Ollama";
                        if ui.selectable_label(is_local, "🖥 Local Ollama (100% Private)").clicked() {
                            self.active_provider = "Local Ollama".to_owned();
                            let _ = self.storage.set_setting("active_provider", "Local Ollama");
                        }
                        let is_openrouter = self.active_provider == "OpenRouter";
                        if ui.selectable_label(is_openrouter, "⚡ OpenRouter (Claude, DeepSeek, GPT-4o)").clicked() {
                            self.active_provider = "OpenRouter".to_owned();
                            let _ = self.storage.set_setting("active_provider", "OpenRouter");
                        }
                    });
                });

            ui.add_space(14.0);

            // OpenRouter API Configuration Card
            egui::Frame::new()
                .fill(SURFACE)
                .corner_radius(10.0)
                .inner_margin(16.0)
                .stroke(egui::Stroke::new(1.0_f32, BORDER))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("⚡ OpenRouter API Integration").strong().color(ACCENT));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let status_text = if self.openrouter_key.is_empty() {
                                RichText::new("No Key Set").small().color(MUTED)
                            } else {
                                RichText::new("API Key Ready").small().color(ACCENT)
                            };
                            ui.label(status_text);
                        });
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new("Connect to hundreds of models (Claude 3.7 Sonnet, DeepSeek R1, GPT-4o) with a single OpenRouter key.").small().color(MUTED));
                    ui.add_space(12.0);

                    ui.label(RichText::new("API Key:").small().color(TEXT));
                    let key_edit = TextEdit::singleline(&mut self.openrouter_key)
                        .password(true)
                        .hint_text("sk-or-v1-...");
                    ui.add_sized([ui.available_width().max(200.0), 32.0], key_edit);

                    ui.add_space(8.0);
                    ui.label(RichText::new("Default Cloud Model:").small().color(TEXT));
                    ui.add_sized([ui.available_width().max(200.0), 32.0], TextEdit::singleline(&mut self.openrouter_model).hint_text("anthropic/claude-3.7-sonnet"));

                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("Save Cloud Settings").strong().color(Color32::BLACK)).clicked() {
                            let _ = self.storage.set_setting("openrouter_key", &self.openrouter_key);
                            let _ = self.storage.set_setting("openrouter_model", &self.openrouter_model);
                            // Update runtime provider key
                            if let Some(p) = self.cloud_providers.iter_mut().find(|p| p.name == "OpenRouter") {
                                p.api_key = if self.openrouter_key.is_empty() { None } else { Some(self.openrouter_key.clone()) };
                            }
                            // Re-fetch dynamic model catalog
                            let or_client = self.client.clone();
                            let or_events = self.events_tx.clone();
                            let or_key = self.openrouter_key.clone();
                            self.runtime.spawn(async move {
                                if let Ok(models) = or_client.fetch_openrouter_models(&or_key).await {
                                    let _ = or_events.send(StreamEvent::OpenRouterModelsLoaded(models));
                                }
                            });
                            self.settings_saved_notice = Some("OpenRouter settings saved & models refreshed!".to_owned());
                            self.log("INFO", "OpenRouter credentials saved to local SQLite");
                        }

                        if let Some(notice) = &self.settings_saved_notice {
                            ui.label(RichText::new(notice).small().color(ACCENT));
                        }
                    });
                });

            ui.add_space(14.0);

            // Local Ollama Status & Setup
            egui::Frame::new()
                .fill(SURFACE)
                .corner_radius(10.0)
                .inner_margin(16.0)
                .stroke(egui::Stroke::new(1.0_f32, BORDER))
                .show(ui, |ui| {
                    ui.label(RichText::new("🖥 Local Ollama Runtime").strong().color(TEXT));
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("Endpoint: {} (127.0.0.1:11434)", OLLAMA_BASE_URL)).small().color(MUTED));
                    ui.add_space(6.0);

                    let (status_color, status_text) = match &self.connection {
                        ConnectionStatus::Connected => (ACCENT, format!("Connected — {} models available locally", self.models.len())),
                        ConnectionStatus::Connecting => (Color32::YELLOW, "Connecting to Ollama...".to_string()),
                        ConnectionStatus::Error(e) => (DANGER, format!("Ollama not running: {}", truncate(e, 50))),
                    };
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                        ui.painter().circle_filled(rect.center(), 4.0, status_color);
                        ui.label(RichText::new(status_text).color(status_color));
                    });

                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("⬇ Pull / Download Model").clicked() {
                            self.show_pull_dialog = true;
                        }
                        if ui.button("🔄 Refresh Local Models").clicked() {
                            let client = self.client.clone();
                            let events = self.events_tx.clone();
                            self.runtime.spawn(async move {
                                if let Ok(models) = client.list_models().await {
                                    let _ = events.send(StreamEvent::ModelsLoaded(models));
                                }
                            });
                        }
                    });
                });
        });
    }
}

impl eframe::App for AiHelperApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Ensure window is visible and focused
        if ctx.cumulative_pass_nr() <= 1 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
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
        
        if self.show_add_project_dialog {
            let mut open = true;
            egui::Window::new("Create New Project")
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("Enter project name:");
                    ui.text_edit_singleline(&mut self.new_project_name);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() && !self.new_project_name.trim().is_empty() {
                            let new_proj_id = self.projects.iter().map(|p| p.id).max().unwrap_or(0) + 1;
                            self.projects.push(Project {
                                id: new_proj_id,
                                name: self.new_project_name.trim().to_owned(),
                            });
                            self.active_project = new_proj_id;
                            
                            // Create default chat for this project
                            let new_convo_id = self.conversations.iter().map(|c| c.id).max().unwrap_or(0) + 1;
                            self.conversations.push(Conversation {
                                id: new_convo_id,
                                project_id: new_proj_id,
                                title: "Main Chat".to_owned(),
                                updated_at: 0,
                            });
                            self.active_conversation = new_convo_id;
                            self.new_project_name.clear();
                            self.show_add_project_dialog = false;
                            self.save_state();
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_add_project_dialog = false;
                        }
                    });
                });
            if !open {
                self.show_add_project_dialog = false;
            }
        }

        // Model Downloader / Puller Dialog
        if self.show_pull_dialog {
            let mut open = true;
            egui::Window::new("⬇ Download / Pull Ollama Model")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label("Model identifier (e.g. ornith:9b, qwen3.5:2b, gemma4:e2b):");
                    ui.text_edit_singleline(&mut self.pull_model_input);
                    ui.add_space(8.0);

                    if let Some((completed, total)) = self.pull_progress {
                        let pct = (completed as f32 / total as f32).clamp(0.0, 1.0);
                        let mb_done = completed as f32 / (1024.0 * 1024.0);
                        let mb_total = total as f32 / (1024.0 * 1024.0);
                        ui.add(egui::ProgressBar::new(pct).text(format!("{:.1}% ({:.1} MB / {:.1} MB)", pct * 100.0, mb_done, mb_total)));
                    } else if let Some(ref status) = self.pull_status {
                        ui.label(RichText::new(status).small().color(ACCENT));
                    }

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let can_pull = !self.pull_model_input.trim().is_empty();
                        if ui.add_enabled(can_pull, egui::Button::new("Download")).clicked() {
                            let model_name = self.pull_model_input.trim().to_owned();
                            let pull_client = self.client.clone();
                            let pull_events = self.events_tx.clone();
                            let (cancel_tx, cancel_rx) = oneshot::channel();
                            self.cancel_tx = Some(cancel_tx);
                            self.pull_status = Some(format!("Initiating download for {model_name}..."));
                            self.runtime.spawn(async move {
                                let _ = pull_client.pull_model(&model_name, &pull_events, cancel_rx).await;
                            });
                        }
                        if ui.button("Close").clicked() {
                            self.show_pull_dialog = false;
                        }
                    });
                });
            if !open {
                self.show_pull_dialog = false;
            }
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
                        if titlebar_button(ui, "close", Color32::from_rgb(232, 17, 35), TEXT).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        // Maximize
                        let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                        let max_type = if is_max { "restore" } else { "maximize" };
                        if titlebar_button(ui, max_type, Color32::from_rgb(45, 50, 60), MUTED).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
                        }
                        // Minimize
                        if titlebar_button(ui, "minimize", Color32::from_rgb(45, 50, 60), MUTED).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                    });
                });
            });

        let sidebar_width = if self.sidebar_collapsed { 60.0 } else { 240.0 };
        egui::SidePanel::left("sidebar")
            .exact_width(sidebar_width)
            .frame(egui::Frame::new()
                .fill(SIDEBAR)
                .inner_margin(if self.sidebar_collapsed { egui::Margin::symmetric(4, 12) } else { egui::Margin::symmetric(12, 12) })
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
                View::Skills => self.skills_view(ui),
                View::Logs => self.logs_view(ui),
                View::Settings => self.settings_view(ui),
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

fn titlebar_button(ui: &mut egui::Ui, button_type: &str, hover_bg: Color32, stroke_color: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(46.0, 36.0), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let is_hovered = response.hovered();
        let bg_color = if is_hovered { hover_bg } else { Color32::TRANSPARENT };
        ui.painter().rect_filled(rect, egui::CornerRadius::ZERO, bg_color);
        
        let final_stroke_color = if is_hovered { Color32::WHITE } else { stroke_color };
        let stroke = egui::Stroke::new(1.0_f32, final_stroke_color);
        let center = rect.center();
        
        match button_type {
            "close" => {
                let size = 4.0;
                ui.painter().line_segment(
                    [center + egui::vec2(-size, -size), center + egui::vec2(size, size)],
                    stroke,
                );
                ui.painter().line_segment(
                    [center + egui::vec2(-size, size), center + egui::vec2(size, -size)],
                    stroke,
                );
            }
            "maximize" => {
                let size = 4.0;
                let min = center + egui::vec2(-size, -size);
                let max = center + egui::vec2(size, size);
                ui.painter().rect_stroke(
                    egui::Rect::from_min_max(min, max),
                    egui::CornerRadius::ZERO,
                    stroke,
                    egui::StrokeKind::Outside
                );
            }
            "restore" => {
                // Two overlapping boxes
                // Back box
                let back_min = center + egui::vec2(-2.0, -4.0);
                let back_max = center + egui::vec2(4.0, 2.0);
                ui.painter().rect_stroke(
                    egui::Rect::from_min_max(back_min, back_max),
                    egui::CornerRadius::ZERO,
                    stroke,
                    egui::StrokeKind::Outside
                );
                // Front box (clear background behind it)
                let front_min = center + egui::vec2(-4.0, -2.0);
                let front_max = center + egui::vec2(2.0, 4.0);
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(front_min, front_max),
                    egui::CornerRadius::ZERO,
                    bg_color,
                );
                ui.painter().rect_stroke(
                    egui::Rect::from_min_max(front_min, front_max),
                    egui::CornerRadius::ZERO,
                    stroke,
                    egui::StrokeKind::Outside
                );
            }
            "minimize" => {
                let size = 5.0;
                ui.painter().line_segment(
                    [center + egui::vec2(-size, 0.0), center + egui::vec2(size, 0.0)],
                    stroke,
                );
            }
            _ => {}
        }
    }
    response
}
