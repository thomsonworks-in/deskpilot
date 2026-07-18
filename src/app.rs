use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, Color32, RichText, ScrollArea, TextEdit};
use tokio::runtime::Runtime;
use tokio::sync::{mpsc, oneshot};

use crate::message::{Message, Role};
use crate::ollama::{OllamaClient, StreamEvent};

pub const OLLAMA_BASE_URL: &str = "http://127.0.0.1:11434";
const DEFAULT_MODEL: &str = "qwen3.6:35b-a3b";
pub const WINDOW_SIZE: [f32; 2] = [1100.0, 750.0];
pub const MIN_WINDOW_SIZE: [f32; 2] = [500.0, 400.0];

const BG: Color32 = Color32::from_rgb(18, 18, 22);
const PANEL: Color32 = Color32::from_rgb(25, 25, 30);
const INPUT: Color32 = Color32::from_rgb(35, 35, 42);
const BORDER: Color32 = Color32::from_rgb(55, 55, 65);

#[derive(Debug, Clone)]
enum ConnectionStatus {
    Connecting,
    Connected,
    Error(String),
}

pub struct AiHelperApp {
    runtime: Arc<Runtime>,
    client: OllamaClient,
    event_tx: mpsc::UnboundedSender<StreamEvent>,
    event_rx: mpsc::UnboundedReceiver<StreamEvent>,
    cancel_tx: Option<oneshot::Sender<()>>,
    messages: Vec<Message>,
    input: String,
    models: Vec<String>,
    selected_model: String,
    connection: ConnectionStatus,
    generating: bool,
    scroll_to_bottom: bool,
}

impl AiHelperApp {
    pub fn new(cc: &eframe::CreationContext<'_>, runtime: Arc<Runtime>) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        let client = OllamaClient::new(OLLAMA_BASE_URL);
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let load_client = client.clone();
        let load_events = event_tx.clone();
        runtime.spawn(async move {
            match load_client.list_models().await {
                Ok(models) => {
                    let _ = load_events.send(StreamEvent::ModelsLoaded(models));
                }
                Err(error) => {
                    let _ = load_events.send(StreamEvent::Error(error.to_string()));
                }
            }
        });

        Self {
            runtime,
            client,
            event_tx,
            event_rx,
            cancel_tx: None,
            messages: Vec::new(),
            input: String::new(),
            models: Vec::new(),
            selected_model: DEFAULT_MODEL.into(),
            connection: ConnectionStatus::Connecting,
            generating: false,
            scroll_to_bottom: false,
        }
    }

    fn poll_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                StreamEvent::Started => self.generating = true,
                StreamEvent::ContentDelta(delta) => {
                    match self.messages.last_mut() {
                        Some(message) if message.role == Role::Assistant => {
                            message.content.push_str(&delta)
                        }
                        _ => self.messages.push(Message::new(Role::Assistant, delta)),
                    }
                    self.scroll_to_bottom = true;
                }
                StreamEvent::Finished => {
                    self.generating = false;
                    self.cancel_tx = None;
                }
                StreamEvent::Cancelled => {
                    self.generating = false;
                    self.cancel_tx = None;
                    if self.messages.last().is_some_and(|message| {
                        message.role == Role::Assistant && message.content.is_empty()
                    }) {
                        self.messages.pop();
                    }
                }
                StreamEvent::Error(error) => {
                    self.generating = false;
                    self.cancel_tx = None;
                    if self.models.is_empty() {
                        self.connection = ConnectionStatus::Error(error.clone());
                    }
                    self.messages
                        .push(Message::new(Role::Assistant, format!("Error: {error}")));
                }
                StreamEvent::ModelsLoaded(models) => {
                    self.models = models;
                    self.connection = ConnectionStatus::Connected;
                    if !self.models.contains(&self.selected_model) {
                        if let Some(model) = self.models.first() {
                            self.selected_model.clone_from(model);
                        }
                    }
                }
            }
        }
    }

    fn send(&mut self) {
        let input = self.input.trim().to_owned();
        if input.is_empty() || self.generating || self.models.is_empty() {
            return;
        }
        self.messages.push(Message::new(Role::User, input));
        self.input.clear();
        self.messages.push(Message::new(Role::Assistant, ""));
        self.generating = true;
        self.scroll_to_bottom = true;

        let history = self.messages[..self.messages.len() - 1].to_vec();
        let model = self.selected_model.clone();
        let client = self.client.clone();
        let events = self.event_tx.clone();
        let (cancel_tx, cancel_rx) = oneshot::channel();
        self.cancel_tx = Some(cancel_tx);
        self.runtime.spawn(async move {
            if let Err(error) = client
                .stream_chat(&model, &history, &events, cancel_rx)
                .await
            {
                let _ = events.send(StreamEvent::Error(error.to_string()));
            }
        });
    }

    fn stop(&mut self) {
        if let Some(cancel) = self.cancel_tx.take() {
            let _ = cancel.send(());
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let (color, status) = match &self.connection {
                ConnectionStatus::Connecting => (Color32::YELLOW, "Connecting".to_owned()),
                ConnectionStatus::Connected => (Color32::GREEN, "Connected".to_owned()),
                ConnectionStatus::Error(error) => (Color32::RED, format!("Offline: {error}")),
            };
            ui.colored_label(color, format!("● {status}"));
            ui.add_space(12.0);
            egui::ComboBox::from_id_salt("model")
                .selected_text(&self.selected_model)
                .width(300.0)
                .show_ui(ui, |ui| {
                    for model in &self.models {
                        ui.selectable_value(&mut self.selected_model, model.clone(), model);
                    }
                });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(if self.generating {
                    "Generating..."
                } else {
                    "Ready"
                });
            });
        });
    }

    fn transcript(&mut self, ui: &mut egui::Ui) {
        ScrollArea::vertical()
            .id_salt("messages")
            .stick_to_bottom(self.scroll_to_bottom)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if self.messages.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.weak("Type a message below to start.");
                    });
                }
                for message in &self.messages {
                    if message.content.is_empty() {
                        continue;
                    }
                    let (name, fill) = if message.role == Role::User {
                        ("You", Color32::from_rgb(37, 99, 235))
                    } else {
                        ("Assistant", Color32::from_rgb(40, 40, 48))
                    };
                    ui.label(RichText::new(name).small().strong());
                    egui::Frame::new()
                        .fill(fill)
                        .corner_radius(7.0)
                        .inner_margin(10.0)
                        .show(ui, |ui| {
                            ui.set_max_width(ui.available_width() * 0.88);
                            ui.label(&message.content);
                        });
                    ui.add_space(10.0);
                }
            });
        self.scroll_to_bottom = false;
    }

    fn composer(&mut self, ui: &mut egui::Ui) {
        let send_with_enter =
            ui.input(|input| input.key_pressed(egui::Key::Enter) && !input.modifiers.shift);
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 120.0).max(100.0);
            ui.add_sized(
                [width, 64.0],
                TextEdit::multiline(&mut self.input)
                    .hint_text("Message Ollama... (Enter to send, Shift+Enter for newline)")
                    .desired_rows(2)
                    .background_color(INPUT),
            );
            if self.generating {
                if ui.button("Stop").clicked() {
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
        if send_with_enter && !self.generating {
            while self.input.ends_with(['\r', '\n']) {
                self.input.pop();
            }
            self.send();
        }
    }
}

impl eframe::App for AiHelperApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_events();
        if self.generating || matches!(self.connection, ConnectionStatus::Connecting) {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
        egui::TopBottomPanel::top("header")
            .frame(egui::Frame::new().fill(BG).inner_margin(12.0))
            .show(ctx, |ui| {
                ui.heading("DeskPilot");
                self.top_bar(ui);
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .stroke(egui::Stroke::new(1.0_f32, BORDER))
                    .inner_margin(14.0),
            )
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    let transcript_height = (ui.available_height() - 90.0).max(120.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), transcript_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| self.transcript(ui),
                    );
                    ui.separator();
                    self.composer(ui);
                });
            });
    }
}
