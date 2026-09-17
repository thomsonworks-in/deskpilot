use axum::{
    extract::{Path, State},
    response::Html,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::storage::Storage;

#[derive(Deserialize)]
pub struct IpcMessage {
    pub content: String,
}

#[derive(Deserialize)]
pub struct CreateTriggerReq {
    pub project_id: u64,
    pub name: String,
    pub trigger_type: String,
    pub schedule_expr: String,
    pub action_type: String,
    pub action_payload: String,
}

#[derive(Deserialize)]
pub struct SaveSettingsReq {
    pub openrouter_key: Option<String>,
    pub openrouter_model: Option<String>,
    pub active_provider: Option<String>,
}

#[derive(Clone)]
struct AppState {
    tx: mpsc::UnboundedSender<String>,
    storage: Arc<Storage>,
}

const WEB_UI_HTML: &str = include_str!("web_ui.html");


pub async fn start_server(tx: mpsc::UnboundedSender<String>) {
    let storage = Arc::new(Storage::new());
    let state = AppState { tx, storage };

    let app = Router::new()
        .route("/", get(|| async { Html(WEB_UI_HTML) }))
        .route("/api/message", post(handle_message))
        .route("/api/triggers/:project_id", get(get_triggers).post(create_trigger))
        .route("/api/settings", post(save_settings))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 31415));
    if let Ok(listener) = tokio::net::TcpListener::bind(addr).await {
        println!("DeskPilot Web Studio available at: http://{}", addr);
        let _ = axum::serve(listener, app).await;
    } else {
        eprintln!("Failed to bind server to {}", addr);
    }
}

async fn handle_message(
    State(state): State<AppState>,
    Json(payload): Json<IpcMessage>,
) -> &'static str {
    let _ = state.tx.send(payload.content);
    "Message dispatched to DeskPilot daemon"
}

async fn get_triggers(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
) -> Json<Vec<crate::storage::TriggerItem>> {
    let list = state.storage.get_triggers(project_id).unwrap_or_default();
    Json(list)
}

async fn create_trigger(
    State(state): State<AppState>,
    Json(payload): Json<CreateTriggerReq>,
) -> &'static str {
    let _ = state.storage.add_trigger(
        payload.project_id,
        &payload.name,
        &payload.trigger_type,
        &payload.schedule_expr,
        &payload.action_type,
        &payload.action_payload,
    );
    "Trigger created"
}

async fn save_settings(
    State(state): State<AppState>,
    Json(payload): Json<SaveSettingsReq>,
) -> &'static str {
    if let Some(key) = payload.openrouter_key {
        let _ = state.storage.set_setting("openrouter_key", &key);
    }
    if let Some(model) = payload.openrouter_model {
        let _ = state.storage.set_setting("openrouter_model", &model);
    }
    if let Some(provider) = payload.active_provider {
        let _ = state.storage.set_setting("active_provider", &provider);
    }
    "Settings saved"
}
