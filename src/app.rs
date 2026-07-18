use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use eframe::egui::{self, Color32, RichText, ScrollArea, TextEdit};
use tokio::runtime::Runtime;
use tokio::sync::{mpsc, oneshot};

use crate::message::{Message, Role};
use crate::ollama::{OllamaClient, StreamEvent};
use crate::storage::{MemoryItem, PersistedState, Storage, TaskItem};

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
const ACCENT: Color32 = Color32::from_rgb(169, 230, 100);
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
}

struct LogEntry {
    timestamp: u64,
    level: &'static str,
    message: String,
}

pub struct AiHelperApp {
    runtime: Arc<Runtime>,
    client: OllamaClient,
    events_tx: mpsc::UnboundedSender<StreamEvent>,
    events_rx: mpsc::UnboundedReceiver<StreamEvent>,
    cancel_tx: Option<oneshot::Sender<()>>,
    storage: Storage,
    messages: Vec<Message>,
    tasks: Vec<TaskItem>,
    memories: Vec<MemoryItem>,
    logs: Vec<LogEntry>,
    input: String,
    task_input: String,
    memory_input: String,
    memory_search: String,
    models: Vec<String>,
    selected_model: String,
    connection: ConnectionStatus,
    view: View,
    generating: bool,
    thinking: String,
    thinking_expanded: bool,
    loading_model: Option<String>,
    scroll_to_bottom: bool,
    active_task: Option<u64>,
    next_id: u64,
}

impl AiHelperApp {
    pub fn new(cc: &eframe::CreationContext<'_>, runtime: Arc<Runtime>) -> Self {
        configure_style(&cc.egui_ctx);
        let storage = Storage::new();
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
        let mut app = Self {
            runtime,
            client,
            events_tx,
            events_rx,
            cancel_tx: None,
            storage,
            messages: persisted.messages,
            tasks: persisted.tasks,
            memories: persisted.memories,
            logs,
            input: String::new(),
            task_input: String::new(),
            memory_input: String::new(),
            memory_search: String::new(),
            models: Vec::new(),
            selected_model,
            connection: ConnectionStatus::Connecting,
            view: View::Chat,
            generating: false,
            thinking: String::new(),
            thinking_expanded: true,
            loading_model: None,
            scroll_to_bottom: true,
            active_task: None,
            next_id,
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

    fn save(&mut self) {
        let state = PersistedState {
            messages: self.messages.clone(),
            tasks: self.tasks.clone(),
            memories: self.memories.clone(),
            selected_model: self.selected_model.clone(),
        };
        if let Err(error) = self.storage.save(&state) {
            self.log("ERROR", format!("Could not save local state: {error}"));
        }
    }

    fn poll_events(&mut self) {
        while let Ok(event) = self.events_rx.try_recv() {
            match event {
                StreamEvent::Started => {
                    self.loading_model = None;
                    self.log("INFO", "Ollama response stream started");
                }
                StreamEvent::ContentDelta(delta) => {
                    if let Some(message) = self.messages.last_mut() {
                        message.content.push_str(&delta);
                    }
                    self.scroll_to_bottom = true;
                }
                StreamEvent::ThinkingDelta(delta) => {
                    self.thinking.push_str(&delta);
                    self.scroll_to_bottom = true;
                }
                StreamEvent::ModelLoading(model) => {
                    self.loading_model = Some(model.clone());
                    self.log("INFO", format!("Loading model {model} into memory"));
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
                    self.save();
                }
                StreamEvent::Cancelled => {
                    self.generating = false;
                    self.loading_model = None;
                    self.cancel_tx = None;
                    self.active_task = None;
                    self.log("WARN", "Generation cancelled by user");
                    self.save();
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
                        Role::Assistant,
                        format!("Unable to respond: {error}"),
                    ));
                    self.log("ERROR", &error);
                    self.save();
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
                    self.save();
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
                        self.save();
                    }
                }
                StreamEvent::Notice(message) => self.log("WARN", message),
            }
        }
    }

    fn send(&mut self) {
        let input = self.input.trim().to_owned();
        if input.is_empty() || self.generating || self.models.is_empty() {
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
        self.messages.push(Message::new(Role::User, input.clone()));
        self.messages.push(Message::new(Role::Assistant, ""));
        self.input.clear();
        self.generating = true;
        self.thinking.clear();
        self.thinking_expanded = true;
        self.scroll_to_bottom = true;
        self.log(
            "INFO",
            format!("Generation requested with model {}", self.selected_model),
        );
        self.save();

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
        let client = self.client.clone();
        let events = self.events_tx.clone();
        let (cancel_tx, cancel_rx) = oneshot::channel();
        self.cancel_tx = Some(cancel_tx);
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
            let mut contextual_history = vec![Message::new(Role::System, format!(
                "You are DeskPilot, a private local assistant. Use relevant saved memory when helpful. Never claim a memory exists unless it appears below.\n\nRELEVANT MEMORY\n{}\n\nOPEN TASKS\n{}",
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
            if let Err(error) = client.stream_chat(&model, &contextual_history, &events, cancel_rx).await {
                let _ = events.send(StreamEvent::Error(error.to_string()));
            }
        });
    }

    fn stop(&mut self) {
        if let Some(cancel) = self.cancel_tx.take() {
            let _ = cancel.send(());
        }
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
            self.messages.clear();
            self.view = View::Chat;
            self.log("INFO", "New conversation started");
            self.save();
        }
        ui.add_space(14.0);
        for (view, label) in [
            (View::Chat, "Chat"),
            (View::Tasks, "Tasks"),
            (View::Memory, "Memory"),
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
        ui.label(RichText::new("RECENT").small().strong().color(MUTED));
        ui.add_space(6.0);
        let recent_title = self
            .messages
            .iter()
            .find(|message| message.role == Role::User)
            .map(|message| truncate(&message.content, 28))
            .unwrap_or_else(|| "No conversations yet".to_owned());
        if ui
            .add_sized(
                [215.0, 48.0],
                egui::Button::new(recent_title).fill(if self.view == View::Chat {
                    SURFACE_HIGH
                } else {
                    Color32::TRANSPARENT
                }),
            )
            .clicked()
        {
            self.view = View::Chat;
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
        inspector_label(ui, "MODEL");
        egui::ComboBox::from_id_salt("inspector_model")
            .selected_text(&self.selected_model)
            .width(230.0)
            .show_ui(ui, |ui| {
                for model in &self.models {
                    ui.selectable_value(&mut self.selected_model, model.clone(), model);
                }
            });
        let model_status = if let Some(model) = &self.loading_model {
            format!("Loading {model}...")
        } else if self.generating {
            "Generating locally".to_owned()
        } else {
            "Ready on local Ollama".to_owned()
        };
        ui.label(
            RichText::new(model_status)
                .small()
                .color(if self.loading_model.is_some() {
                    Color32::YELLOW
                } else {
                    MUTED
                }),
        );
        ui.add_space(18.0);
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

    fn chat_view(&mut self, ui: &mut egui::Ui) {
        self.header(ui, "Chat", "Private conversations with your local models");
        let height = (ui.available_height() - 100.0).max(160.0);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ScrollArea::vertical()
                    .id_salt("messages")
                    .stick_to_bottom(self.scroll_to_bottom)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if self.messages.is_empty() {
                            ui.add_space(80.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("Your local copilot is ready")
                                        .size(22.0)
                                        .strong()
                                        .color(TEXT),
                                );
                                ui.label(
                                    RichText::new(
                                        "Ask a question, plan work, or save knowledge for later.",
                                    )
                                    .color(MUTED),
                                );
                            });
                        }
                        for message in &self.messages {
                            if message.role == Role::System || message.content.is_empty() {
                                continue;
                            }
                            let (name, fill) = if message.role == Role::User {
                                ("You", BLUE)
                            } else {
                                ("DeskPilot", SURFACE_HIGH)
                            };
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(name).small().strong().color(
                                    if message.role == Role::User {
                                        Color32::from_rgb(170, 196, 255)
                                    } else {
                                        ACCENT
                                    },
                                ));
                                if message.created_at > 0 {
                                    ui.label(
                                        RichText::new(format_message_time(message.created_at))
                                            .small()
                                            .color(MUTED),
                                    );
                                }
                            });
                            egui::Frame::new()
                                .fill(fill)
                                .corner_radius(9.0)
                                .inner_margin(12.0)
                                .show(ui, |ui| {
                                    ui.set_max_width(ui.available_width() * 0.9);
                                    ui.label(RichText::new(&message.content).color(TEXT));
                                });
                            ui.add_space(12.0);
                        }
                        if self.generating || !self.thinking.is_empty() {
                            egui::CollapsingHeader::new(if self.generating {
                                "Thinking..."
                            } else {
                                "Reasoning"
                            })
                            .default_open(self.thinking_expanded)
                            .show(ui, |ui| {
                                if self.thinking.is_empty() {
                                    ui.label(
                                        RichText::new("Preparing a response...")
                                            .italics()
                                            .color(MUTED),
                                    );
                                } else {
                                    egui::Frame::new()
                                        .fill(SURFACE)
                                        .corner_radius(7.0)
                                        .inner_margin(10.0)
                                        .show(ui, |ui| {
                                            ui.label(
                                                RichText::new(&self.thinking)
                                                    .monospace()
                                                    .small()
                                                    .color(MUTED),
                                            );
                                        });
                                }
                            });
                        }
                    });
            },
        );
        self.scroll_to_bottom = false;
        ui.separator();
        let enter = ui.input(|input| input.key_pressed(egui::Key::Enter) && !input.modifiers.shift);
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 92.0).max(120.0);
            ui.add_sized(
                [width, 58.0],
                TextEdit::multiline(&mut self.input)
                    .hint_text("Ask DeskPilot...")
                    .desired_rows(2),
            );
            if self.generating {
                if ui
                    .add_sized(
                        [78.0, 38.0],
                        egui::Button::new("Stop").fill(Color32::from_rgb(65, 31, 38)),
                    )
                    .clicked()
                {
                    self.stop();
                }
            } else if ui
                .add_enabled(
                    !self.input.trim().is_empty() && !self.models.is_empty(),
                    egui::Button::new("Send"),
                )
                .clicked()
            {
                self.send();
            }
        });
        if enter && !self.generating {
            while self.input.ends_with(['\r', '\n']) {
                self.input.pop();
            }
            self.send();
        }
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
                self.save();
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
            self.save();
        }
        if task_changed {
            self.save();
        }
        if ui.button("Clear completed").clicked() {
            self.tasks.retain(|task| !task.done);
            self.save();
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
                self.save();
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
            self.save();
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
}

impl eframe::App for AiHelperApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_events();
        if self.generating || matches!(self.connection, ConnectionStatus::Connecting) {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
        egui::SidePanel::left("sidebar")
            .exact_width(240.0)
            .frame(egui::Frame::new().fill(SIDEBAR).inner_margin(12.0))
            .show(ctx, |ui| self.sidebar(ui));
        egui::SidePanel::right("inspector")
            .exact_width(270.0)
            .frame(egui::Frame::new().fill(SIDEBAR).inner_margin(14.0))
            .show(ctx, |ui| self.inspector(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(CANVAS).inner_margin(20.0))
            .show(ctx, |ui| match self.view {
                View::Chat => self.chat_view(ui),
                View::Tasks => self.tasks_view(ui),
                View::Memory => self.memory_view(ui),
                View::Logs => self.logs_view(ui),
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save();
    }
}

fn configure_style(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.visuals.panel_fill = CANVAS;
    style.visuals.window_fill = SURFACE;
    style.visuals.extreme_bg_color = SURFACE;
    style.visuals.widgets.inactive.bg_fill = SURFACE_HIGH;
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(33, 41, 53);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(43, 55, 68);
    style.visuals.selection.bg_fill = Color32::from_rgb(68, 99, 49);
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
