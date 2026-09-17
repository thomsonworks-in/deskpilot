use axum::{
    extract::{Path, State},
    response::Html,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;

use crate::storage::Storage;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct IpcMessage {
    pub content: String,
    pub model: Option<String>,
}

#[derive(Deserialize)]
pub struct ChatReq {
    pub message: String,
    pub model: Option<String>,
    pub project_id: Option<u64>,
    pub conversation_id: Option<u64>,
}

#[derive(Serialize)]
pub struct ChatResp {
    pub status: String,
    pub model: String,
    pub reply: String,
    pub thinking: String,
    pub duration_ms: u64,
}

#[derive(Serialize)]
pub struct ModelsResp {
    pub local: Vec<String>,
    pub cloud: Vec<String>,
    pub active: String,
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
    pub selected_model: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateProviderReq {
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    pub api_key: String,
    pub default_model: String,
    pub is_active: Option<bool>,
}

#[derive(Deserialize)]
pub struct OptimiseChatReq {
    pub project_id: Option<u64>,
    pub conversation_id: Option<u64>,
}

#[derive(Deserialize)]
pub struct CreateGotchaReq {
    pub project_id: u64,
    pub subsystem: String,
    pub gotcha_text: String,
    pub invariant_rule: String,
    pub confidence: Option<f64>,
}

#[derive(Serialize)]
pub struct OptimiseChatResp {
    pub status: String,
    pub gotchas_preserved: usize,
    pub message: String,
}

#[derive(Serialize)]
pub struct LocalTestChatResp {
    pub model: String,
    pub reply: String,
    pub thinking: String,
    pub status: String,
    pub duration_ms: u64,
}

#[derive(Serialize)]
pub struct StoredMessageResp {
    pub role: String,
    pub content: String,
    pub thinking: String,
    pub created_at: i64,
}

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<Storage>,
}

const WEB_UI_HTML: &str = include_str!("web_ui.html");

pub async fn start_server(storage: Arc<Storage>) {
    let state = AppState { storage };

    let app = Router::new()
        .route("/", get(|| async { Html(WEB_UI_HTML) }))
        .route("/api/models", get(get_models))
        .route("/api/chat", post(handle_chat))
        .route("/api/message", post(handle_message))
        .route("/api/messages/:conversation_id", get(get_messages))
        .route("/api/chat/test-local", post(test_local_chat))
        .route("/api/chat/optimise", post(optimise_chat))
        .route("/api/gotchas/:project_id", get(get_gotchas).post(create_gotcha))
        .route("/api/triggers/:project_id", get(get_triggers).post(create_trigger))
        .route("/api/providers", get(get_providers).post(create_provider))
        .route("/api/providers/:id", axum::routing::delete(delete_provider))
        .route("/api/providers/:id/activate", post(activate_provider))
        .route("/api/settings", get(get_settings).post(save_settings))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 31415));
    if let Ok(listener) = tokio::net::TcpListener::bind(addr).await {
        println!("DeskPilot Obsidian Velocity Studio live at: http://{}", addr);
        let _ = axum::serve(listener, app).await;
    } else {
        eprintln!("Failed to bind server to {}", addr);
    }
}

async fn handle_message(
    State(state): State<AppState>,
    Json(payload): Json<IpcMessage>,
) -> &'static str {
    if let Some(ref m) = payload.model {
        if !m.is_empty() {
            let _ = state.storage.set_setting("selected_model", m);
        }
    }
    let _ = state.storage.add_message(1, "user", &payload.content, "");
    "Message recorded in DeskPilot storage"
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
    if let Some(m) = payload.selected_model {
        let _ = state.storage.set_setting("selected_model", &m);
    }
    "Settings saved"
}

async fn get_settings(State(state): State<AppState>) -> Json<serde_json::Value> {
    let openrouter_key = state.storage.get_setting("openrouter_key").unwrap_or_default().unwrap_or_default();
    let openrouter_model = state.storage.get_setting("openrouter_model").unwrap_or_default().unwrap_or_else(|| "anthropic/claude-3.7-sonnet".to_string());
    let active_provider = state.storage.get_setting("active_provider").unwrap_or_default().unwrap_or_else(|| "Local Ollama".to_string());
    let selected_model = state.storage.get_setting("selected_model").unwrap_or_default().unwrap_or_else(|| "ornith:9b".to_string());
    
    Json(serde_json::json!({
        "openrouter_key": openrouter_key,
        "openrouter_model": openrouter_model,
        "active_provider": active_provider,
        "selected_model": selected_model,
    }))
}

async fn get_models(State(state): State<AppState>) -> Json<ModelsResp> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let mut local_models = Vec::new();
    if let Ok(resp) = client.get("http://127.0.0.1:11434/api/tags").send().await {
        if let Ok(body) = resp.json::<serde_json::Value>().await {
            if let Some(models) = body.get("models").and_then(|m| m.as_array()) {
                for m in models {
                    if let Some(name) = m.get("name").and_then(|n| n.as_str()) {
                        if !name.to_lowercase().contains("embedding") {
                            local_models.push(name.to_string());
                        }
                    }
                }
            }
        }
    }

    let cloud_models = vec![
        "anthropic/claude-3.7-sonnet".to_string(),
        "deepseek/deepseek-r1".to_string(),
        "openai/gpt-4o".to_string(),
        "google/gemini-2.0-flash-001".to_string(),
    ];

    let active_saved = state.storage.get_setting("selected_model").unwrap_or(None);
    let active = active_saved.unwrap_or_else(|| {
        if local_models.iter().any(|m| m == "ornith:9b") {
            "ornith:9b".to_string()
        } else if let Some(first) = local_models.first() {
            first.clone()
        } else {
            "ornith:9b".to_string()
        }
    });

    Json(ModelsResp {
        local: local_models,
        cloud: cloud_models,
        active,
    })
}

async fn handle_chat(
    State(state): State<AppState>,
    Json(payload): Json<ChatReq>,
) -> Json<ChatResp> {
    let start = std::time::Instant::now();
    let conversation_id = payload.conversation_id.unwrap_or(1);
    let project_id = payload.project_id.unwrap_or(1);

    // Append user message to SQLite
    let _ = state.storage.add_message(conversation_id, "user", &payload.message, "");

    // Determine target model
    let target_model = payload.model
        .filter(|m| !m.is_empty())
        .or_else(|| state.storage.get_setting("selected_model").unwrap_or(None))
        .unwrap_or_else(|| "ornith:9b".to_string());

    let _ = state.storage.set_setting("selected_model", &target_model);

    // Build context with adaptive memory
    let adaptive_memory = state.storage.get_adaptive_context(project_id);
    let mut messages_payload = Vec::new();
    messages_payload.push(serde_json::json!({
        "role": "system",
        "content": format!("You are ThomsonWorks DeskPilot, an autonomous coding & workspace agent running locally on Windows Ollama. Provide sharp, surgical responses with technical precision.\n{}", adaptive_memory)
    }));

    if let Ok(history) = state.storage.get_conversation_messages(conversation_id) {
        for msg in history.iter().rev().take(12).rev() {
            let role_str = match msg.role {
                crate::message::Role::System => "system",
                crate::message::Role::Assistant => "assistant",
                _ => "user",
            };
            messages_payload.push(serde_json::json!({
                "role": role_str,
                "content": msg.content,
            }));
        }
    } else {
        messages_payload.push(serde_json::json!({
            "role": "user",
            "content": payload.message,
        }));
    }

    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let res = client.post("http://127.0.0.1:11434/api/chat")
        .json(&serde_json::json!({
            "model": target_model,
            "messages": messages_payload,
            "stream": false
        }))
        .send()
        .await;

    match res {
        Ok(resp) => {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                let content = body.get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_string();
                let thinking = body.get("message")
                    .and_then(|m| m.get("thinking"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();

                let _ = state.storage.add_message(
                    conversation_id,
                    "assistant",
                    &content,
                    &thinking,
                );

                Json(ChatResp {
                    status: "ok".into(),
                    model: target_model,
                    reply: content,
                    thinking,
                    duration_ms: start.elapsed().as_millis() as u64,
                })
            } else {
                Json(ChatResp {
                    status: "parse_error".into(),
                    model: target_model,
                    reply: "Failed to parse JSON response from local Ollama".into(),
                    thinking: String::new(),
                    duration_ms: start.elapsed().as_millis() as u64,
                })
            }
        }
        Err(err) => {
            Json(ChatResp {
                status: "connection_error".into(),
                model: target_model,
                reply: format!("Could not reach local Ollama on 127.0.0.1:11434: {err}"),
                thinking: String::new(),
                duration_ms: start.elapsed().as_millis() as u64,
            })
        }
    }
}

async fn get_providers(
    State(state): State<AppState>,
) -> Json<Vec<crate::storage::ProviderRecord>> {
    let list = state.storage.get_providers().unwrap_or_default();
    Json(list)
}

async fn create_provider(
    State(state): State<AppState>,
    Json(payload): Json<CreateProviderReq>,
) -> &'static str {
    let _ = state.storage.add_provider(
        &payload.name,
        &payload.provider_type,
        &payload.base_url,
        &payload.api_key,
        &payload.default_model,
        payload.is_active.unwrap_or(false),
    );
    "Provider created"
}

async fn delete_provider(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> &'static str {
    let _ = state.storage.delete_provider(id);
    "Provider deleted"
}

async fn activate_provider(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> &'static str {
    let _ = state.storage.set_active_provider(id);
    "Provider activated"
}

async fn optimise_chat(
    State(state): State<AppState>,
    Json(payload): Json<OptimiseChatReq>,
) -> Json<OptimiseChatResp> {
    let project_id = payload.project_id.unwrap_or(1);
    let conversation_id = payload.conversation_id.unwrap_or(1);
    let count = state.storage.optimise_conversation(project_id, conversation_id).unwrap_or(0);
    Json(OptimiseChatResp {
        status: "ok".into(),
        gotchas_preserved: count,
        message: format!("Optimised conversation: {} gotchas active in memory", count),
    })
}

async fn get_gotchas(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
) -> Json<Vec<crate::storage::GotchaItem>> {
    let list = state.storage.get_gotchas(project_id).unwrap_or_default();
    Json(list)
}

async fn create_gotcha(
    State(state): State<AppState>,
    Json(payload): Json<CreateGotchaReq>,
) -> &'static str {
    let _ = state.storage.add_gotcha(
        payload.project_id,
        &payload.subsystem,
        &payload.gotcha_text,
        &payload.invariant_rule,
        payload.confidence.unwrap_or(1.0),
    );
    "Gotcha created"
}

async fn get_messages(
    State(state): State<AppState>,
    Path(conversation_id): Path<u64>,
) -> Json<Vec<StoredMessageResp>> {
    let list = state.storage.get_conversation_messages(conversation_id).unwrap_or_default();
    let resp = list.into_iter().map(|m| {
        let role = match m.role {
            crate::message::Role::System => "system",
            crate::message::Role::Assistant => "assistant",
            _ => "user",
        };
        StoredMessageResp {
            role: role.into(),
            content: m.content,
            thinking: m.thinking,
            created_at: m.created_at,
        }
    }).collect();
    Json(resp)
}

async fn test_local_chat() -> Json<LocalTestChatResp> {
    let start = std::time::Instant::now();
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let res = client.post("http://127.0.0.1:11434/api/chat")
        .json(&serde_json::json!({
            "model": "ornith:9b",
            "messages": [
                { "role": "system", "content": "You are ThomsonWorks DeskPilot running locally on Windows Ollama. Respond briefly in one short sentence." },
                { "role": "user", "content": "Confirm your status and model identity." }
            ],
            "stream": false
        }))
        .send()
        .await;

    match res {
        Ok(resp) => {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                let content = body.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()).unwrap_or("Connected!").to_string();
                let thinking = body.get("message").and_then(|m| m.get("thinking")).and_then(|t| t.as_str()).unwrap_or("").to_string();
                Json(LocalTestChatResp {
                    model: "ornith:9b".into(),
                    reply: content,
                    thinking,
                    status: "online".into(),
                    duration_ms: start.elapsed().as_millis() as u64,
                })
            } else {
                Json(LocalTestChatResp {
                    model: "ornith:9b".into(),
                    reply: "Received non-JSON response format from local Ollama".into(),
                    thinking: String::new(),
                    status: "format_error".into(),
                    duration_ms: start.elapsed().as_millis() as u64,
                })
            }
        }
        Err(err) => {
            Json(LocalTestChatResp {
                model: "ornith:9b".into(),
                reply: format!("Could not reach local Ollama on 127.0.0.1:11434: {err}"),
                thinking: String::new(),
                status: "offline".into(),
                duration_ms: start.elapsed().as_millis() as u64,
            })
        }
    }
}

