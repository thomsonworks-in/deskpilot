use axum::{
    extract::{Path, State},
    response::Html,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;

use std::path::PathBuf;
use crate::storage::Storage;
use crate::tools;



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
    pub completed_task: Option<crate::storage::TaskItem>,
}

#[derive(Deserialize)]
pub struct CreateProjectReq {
    pub name: String,
    pub path: String,
    pub trust_level: Option<String>,
    pub permissions: Option<crate::storage::ProjectPermissions>,
}

#[derive(Deserialize)]
pub struct UpdateProjectPathReq {
    pub path: String,
}

#[derive(Deserialize)]
pub struct UpdateProjectTrustReq {
    pub trust_level: String,
    pub permissions: crate::storage::ProjectPermissions,
}

#[derive(Serialize)]
pub struct PickFolderResp {
    pub selected: bool,
    pub path: String,
    pub name: String,
}


#[derive(Deserialize)]
pub struct CreateConversationReq {
    pub title: String,
}

#[derive(Deserialize)]
pub struct CreateTaskReq {
    pub title: String,
    pub done: Option<bool>,
}

#[derive(Deserialize)]
pub struct ToggleTaskReq {
    pub done: bool,
}

#[derive(Serialize, Clone)]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    pub provider: String,  // "ollama" | "openrouter" | "custom"
    pub is_free: bool,
    pub context_length: u32,
    pub pricing_prompt: String,
}

#[derive(Serialize)]
pub struct ModelsResp {
    pub local: Vec<ModelEntry>,
    pub cloud: Vec<ModelEntry>,
    pub active: String,
    pub has_openrouter_key: bool,
}

#[derive(Serialize)]
pub struct CreditsResp {
    pub provider: String,
    pub label: String,
    pub usage: f64,
    pub limit: Option<f64>,
    pub limit_remaining: Option<f64>,
    pub is_free_tier: bool,
    pub rate_limit_requests: u32,
    pub rate_limit_interval: String,
    pub error: Option<String>,
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
        .route("/api/projects", get(get_projects).post(create_project))
        .route("/api/projects/:id/path", post(update_project_path))
        .route("/api/projects/:id/trust", post(update_project_trust))
        .route("/api/dialog/pick-folder", post(pick_folder_dialog))
        .route("/api/projects/:id/conversations", get(get_conversations).post(create_conversation))

        .route("/api/projects/:id/tasks", get(get_tasks).post(create_task))
        .route("/api/tasks/:id/toggle", post(toggle_task))
        .route("/api/chat/test-local", post(test_local_chat))
        .route("/api/chat/optimise", post(optimise_chat))
        .route("/api/gotchas/:project_id", get(get_gotchas).post(create_gotcha))
        .route("/api/triggers/:project_id", get(get_triggers).post(create_trigger))
        .route("/api/providers", get(get_providers).post(create_provider))
        .route("/api/providers/:id", axum::routing::delete(delete_provider))
        .route("/api/providers/:id/activate", post(activate_provider))
        .route("/api/settings", get(get_settings).post(save_settings))
        .route("/api/credits", get(get_credits))
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

    // --- Local Ollama models ---
    let mut local_models: Vec<ModelEntry> = Vec::new();
    if let Ok(resp) = client.get("http://127.0.0.1:11434/api/tags").send().await {
        if let Ok(body) = resp.json::<serde_json::Value>().await {
            if let Some(models) = body.get("models").and_then(|m| m.as_array()) {
                for m in models {
                    if let Some(name) = m.get("name").and_then(|n| n.as_str()) {
                        if !name.to_lowercase().contains("embedding") {
                            local_models.push(ModelEntry {
                                id: name.to_string(),
                                name: name.to_string(),
                                provider: "ollama".to_string(),
                                is_free: true,
                                context_length: 32768,
                                pricing_prompt: "free".to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    // --- OpenRouter cloud models (fetched live if key is set) ---
    let or_key = state.storage.get_setting("openrouter_key").unwrap_or(None).unwrap_or_default();
    let has_openrouter_key = !or_key.is_empty();
    let mut cloud_models: Vec<ModelEntry> = Vec::new();

    if has_openrouter_key {
        let or_client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        if let Ok(resp) = or_client
            .get("https://openrouter.ai/api/v1/models")
            .bearer_auth(&or_key)
            .send()
            .await
        {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
                    for m in data {
                        let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        if id.is_empty() { continue; }
                        let name = m.get("name").and_then(|v| v.as_str()).unwrap_or(&id).to_string();
                        let ctx = m.get("context_length").and_then(|v| v.as_u64()).unwrap_or(32768) as u32;
                        let prompt_price = m.get("pricing")
                            .and_then(|p| p.get("prompt"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("0");
                        let comp_price = m.get("pricing")
                            .and_then(|p| p.get("completion"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("0");
                        let is_free = prompt_price == "0" && comp_price == "0";
                        let pricing_display = if is_free {
                            "free".to_string()
                        } else {
                            format!("${}/1k in", prompt_price)
                        };
                        cloud_models.push(ModelEntry {
                            id,
                            name,
                            provider: "openrouter".to_string(),
                            is_free,
                            context_length: ctx,
                            pricing_prompt: pricing_display,
                        });
                    }
                }
            }
        }
    } else {
        // No OR key — show curated defaults so users know what's available
        let defaults = [
            ("google/gemini-2.0-flash-exp:free", "Gemini 2.0 Flash (Free)", true, 1048576u32),
            ("meta-llama/llama-3.3-70b-instruct:free", "Llama 3.3 70B (Free)", true, 131072),
            ("deepseek/deepseek-r1:free", "DeepSeek R1 (Free)", true, 163840),
            ("anthropic/claude-3.7-sonnet", "Claude 3.7 Sonnet", false, 200000),
            ("openai/gpt-4o", "GPT-4o", false, 128000),
            ("google/gemini-2.5-pro", "Gemini 2.5 Pro", false, 1048576),
        ];
        for (id, name, is_free, ctx) in &defaults {
            cloud_models.push(ModelEntry {
                id: id.to_string(),
                name: name.to_string(),
                provider: "openrouter".to_string(),
                is_free: *is_free,
                context_length: *ctx,
                pricing_prompt: if *is_free { "free".to_string() } else { "paid".to_string() },
            });
        }
    }

    // Also add any custom providers' default models
    if let Ok(providers) = state.storage.get_providers() {
        for p in &providers {
            if p.provider_type != "ollama" && p.provider_type != "openrouter" && !p.default_model.is_empty() {
                cloud_models.push(ModelEntry {
                    id: p.default_model.clone(),
                    name: format!("{} ({})", p.default_model, p.name),
                    provider: p.name.clone(),
                    is_free: false,
                    context_length: 128000,
                    pricing_prompt: "custom".to_string(),
                });
            }
        }
    }

    let active_saved = state.storage.get_setting("selected_model").unwrap_or(None);
    let active = active_saved.unwrap_or_else(|| {
        if let Some(first) = local_models.first() {
            first.id.clone()
        } else {
            "ornith:9b".to_string()
        }
    });

    Json(ModelsResp {
        local: local_models,
        cloud: cloud_models,
        active,
        has_openrouter_key,
    })
}

async fn handle_chat(
    State(state): State<AppState>,
    Json(payload): Json<ChatReq>,
) -> Json<ChatResp> {
    let start = std::time::Instant::now();
    let conversation_id = payload.conversation_id.unwrap_or(1);
    let project_id = payload.project_id.unwrap_or(1);

    let _ = state.storage.add_message(conversation_id, "user", &payload.message, "");

    let target_model = payload.model
        .filter(|m| !m.is_empty())
        .or_else(|| state.storage.get_setting("selected_model").unwrap_or(None))
        .unwrap_or_else(|| "ornith:9b".to_string());

    let _ = state.storage.set_setting("selected_model", &target_model);

    let adaptive_memory = state.storage.get_adaptive_context(project_id);
    let project_obj = state.storage.get_project_by_id(project_id).ok().flatten();
    let perms = project_obj.as_ref().map(|p| p.permissions.clone()).unwrap_or_else(|| crate::storage::ProjectPermissions::preset("readwrite"));
    let trust_level = project_obj.as_ref().map(|p| p.trust_level.clone()).unwrap_or_else(|| "readwrite".to_string());
    let ws_path_str = project_obj.as_ref().map(|p| p.path.clone()).unwrap_or_default();
    let workspace_path = if !ws_path_str.is_empty() {
        PathBuf::from(&ws_path_str)
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    };

    // 1. Build rapid overview of files currently inside the workspace root
    let mut files_summary = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir(&workspace_path).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with('.') && name != "target" && name != "node_modules" {
                if let Ok(ft) = entry.file_type().await {
                    if ft.is_dir() {
                        files_summary.push(format!("{}/", name));
                    } else {
                        files_summary.push(name);
                    }
                }
            }
            if files_summary.len() >= 40 {
                break;
            }
        }
    }
    let files_list_str = if files_summary.is_empty() {
        "(Empty directory or path does not exist yet)".to_string()
    } else {
        files_summary.join(", ")
    };

    // 2. Load living todos
    let active_tasks = state.storage.get_tasks(project_id).unwrap_or_default();
    let tasks_str = if active_tasks.is_empty() {
        "None pending".to_string()
    } else {
        active_tasks
            .iter()
            .take(6)
            .map(|t| format!("- [{}] {}", if t.done { "x" } else { " " }, t.title))
            .collect::<Vec<_>>()
            .join("\n")
    };

    // 3. Load gotchas
    let gotchas = state.storage.get_gotchas(project_id).unwrap_or_default();
    let gotchas_str = if gotchas.is_empty() {
        String::new()
    } else {
        format!(
            "\nPROJECT INVARIANTS & LESSONS LEARNED:\n{}",
            gotchas.iter().take(5).map(|g| format!("- [{}]: {} -> Rule: {}", g.subsystem, g.gotcha_text, g.invariant_rule)).collect::<Vec<_>>().join("\n")
        )
    };

    // 4. Discover custom SKILL.md files
    let skills = tools::discover_skills(&workspace_path);
    let skills_str = if skills.is_empty() {
        String::new()
    } else {
        format!(
            "\nAVAILABLE SKILLS:\n{}",
            skills.iter().map(|s| format!("- {}: {}", s.name, s.description)).collect::<Vec<_>>().join("\n")
        )
    };

    let p_summary = format!(
        "read_files={}, write_files={}, terminal_exec={}, web_search={}, git_ops={}",
        perms.read_files, perms.write_files, perms.terminal_exec, perms.web_search, perms.git_ops
    );

    let system_prompt = format!(
        "You are ThomsonWorks DeskPilot, an autonomous coding & workspace agent. You have native filesystem & tool access.\n\
        You can inspect, read, create, and edit files in the active workspace using your provided tools.\n\n\
        ACTIVE WORKSPACE ENVIRONMENT:\n\
        - OS: Windows (x86_64)\n\
        - Workspace Root: {}\n\
        - Existing Files & Folders: [{}]\n\
        - Trust & Permission Profile: {} ({})\n\
        - Active Living Tasks:\n{}\
        {}\
        {}\
        {}\n\n\
        SECURITY POLICY:\n\
        You are strictly confined to the Active Workspace Root. Do not attempt to access or modify any paths outside it.",
        workspace_path.display(),
        files_list_str,
        trust_level,
        p_summary,
        tasks_str,
        gotchas_str,
        skills_str,
        adaptive_memory
    );



    // Build shared message history
    let mut messages_payload: Vec<serde_json::Value> = Vec::new();
    messages_payload.push(serde_json::json!({ "role": "system", "content": system_prompt }));

    if let Ok(history) = state.storage.get_conversation_messages(conversation_id) {
        for msg in history.iter().rev().take(12).rev() {
            let role_str = match msg.role {
                crate::message::Role::System => "system",
                crate::message::Role::Assistant => "assistant",
                _ => "user",
            };
            messages_payload.push(serde_json::json!({ "role": role_str, "content": msg.content }));
        }
    } else {
        messages_payload.push(serde_json::json!({ "role": "user", "content": payload.message }));
    }

    // --- Route: cloud model (contains '/') → OpenAI-compat provider ---
    let is_cloud = target_model.contains('/');

    if is_cloud {
        // Resolve provider: check active custom provider first, fallback to OpenRouter
        let (base_url, api_key) = {
            // Check if there's an active custom provider that isn't ollama
            let custom = state.storage.get_providers().ok()
                .and_then(|ps| ps.into_iter().find(|p| p.is_active && p.provider_type != "ollama"));
            if let Some(p) = custom {
                (p.base_url, p.api_key)
            } else {
                let key = state.storage.get_setting("openrouter_key")
                    .unwrap_or(None)
                    .unwrap_or_default();
                ("https://openrouter.ai/api/v1".to_string(), key)
            }
        };

        if api_key.is_empty() {
            return Json(ChatResp {
                status: "no_api_key".into(),
                model: target_model,
                reply: "No API key configured. Open Settings (⚙) and add your OpenRouter key to use cloud models.".into(),
                thinking: String::new(),
                duration_ms: start.elapsed().as_millis() as u64,
                completed_task: None,
            });
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let tool_definitions = tools::definitions_for_permissions(&perms);

        let mut final_content = String::new();
        let mut final_thinking = String::new();
        let mut served_model = target_model.clone();


        // Multi-turn tool loop (up to 5 steps)
        for _step in 0..5 {
            let mut req_body = serde_json::json!({
                "model": target_model,
                "messages": messages_payload,
            });
            if !tool_definitions.is_empty() {
                req_body["tools"] = serde_json::to_value(&tool_definitions).unwrap_or_default();
            }

            let res = client
                .post(format!("{}/chat/completions", base_url.trim_end_matches('/')))
                .bearer_auth(&api_key)
                .header("HTTP-Referer", "https://thomsonworks.in")
                .header("X-Title", "DeskPilot")
                .json(&req_body)
                .send()
                .await;

            match res {
                Ok(resp) => {
                    if let Ok(body) = resp.json::<serde_json::Value>().await {
                        if let Some(err) = body.get("error").and_then(|e| e.get("message")).and_then(|v| v.as_str()) {
                            return Json(ChatResp {
                                status: "provider_error".into(),
                                model: target_model,
                                reply: format!("Provider error: {}", err),
                                thinking: String::new(),
                                duration_ms: start.elapsed().as_millis() as u64,
                                completed_task: None,
                            });
                        }

                        if let Some(m) = body.get("model").and_then(|v| v.as_str()) {
                            served_model = m.to_string();
                        }

                        let choice = body.get("choices").and_then(|c| c.get(0));
                        let message_obj = choice.and_then(|c| c.get("message"));
                        let content_opt = message_obj.and_then(|m| m.get("content")).and_then(|v| v.as_str());
                        if let Some(c) = content_opt {
                            final_content = c.to_string();
                        }

                        // Check if model emitted tool calls
                        let tool_calls = message_obj
                            .and_then(|m| m.get("tool_calls"))
                            .and_then(|tc| tc.as_array())
                            .cloned();

                        if let Some(calls) = tool_calls {
                            if !calls.is_empty() {
                                // Add assistant's tool_calls message to context
                                messages_payload.push(message_obj.cloned().unwrap_or_else(|| serde_json::json!({
                                    "role": "assistant",
                                    "content": final_content,
                                    "tool_calls": calls
                                })));

                                // Execute each tool against the active workspace
                                for call in calls {
                                    let call_id = call.get("id").and_then(|v| v.as_str()).unwrap_or("call_1");
                                    let fn_obj = call.get("function");
                                    let fn_name = fn_obj.and_then(|f| f.get("name")).and_then(|v| v.as_str()).unwrap_or("");
                                    let fn_args_raw = fn_obj.and_then(|f| f.get("arguments")).and_then(|v| v.as_str()).unwrap_or("{}");
                                    let fn_args: serde_json::Value = serde_json::from_str(fn_args_raw).unwrap_or_else(|_| serde_json::json!({}));

                                    let exec_result = tools::execute_with_permissions(
                                        fn_name,
                                        &fn_args,
                                        &workspace_path,
                                        &skills,
                                        &perms,
                                    ).await;

                                    let tool_output = match exec_result {
                                        Ok(out) => out,
                                        Err(err) => format!("Error executing {}: {}", fn_name, err),
                                    };

                                    messages_payload.push(serde_json::json!({
                                        "role": "tool",
                                        "tool_call_id": call_id,
                                        "content": tool_output,
                                    }));
                                }
                                // Continue loop so the model sees tool results and formulates final answer
                                continue;
                            }
                        }

                        // No more tool calls: model reached final answer
                        break;
                    } else {
                        return Json(ChatResp {
                            status: "parse_error".into(),
                            model: target_model,
                            reply: "Failed to parse cloud provider response".into(),
                            thinking: String::new(),
                            duration_ms: start.elapsed().as_millis() as u64,
                            completed_task: None,
                        });
                    }
                }
                Err(err) => {
                    return Json(ChatResp {
                        status: "connection_error".into(),
                        model: target_model,
                        reply: format!("Could not reach cloud provider: {err}"),
                        thinking: String::new(),
                        duration_ms: start.elapsed().as_millis() as u64,
                        completed_task: None,
                    });
                }
            }
        }

        let _ = state.storage.add_message(conversation_id, "assistant", &final_content, &final_thinking);
        let completed_task = state.storage.complete_next_task(project_id).ok().flatten();
        let display_model = if served_model != target_model && !served_model.is_empty() {
            format!("{} → {}", target_model, served_model)
        } else {
            served_model
        };

        Json(ChatResp {
            status: "ok".into(),
            model: display_model,
            reply: final_content,
            thinking: final_thinking,
            duration_ms: start.elapsed().as_millis() as u64,
            completed_task,
        })
    } else {
        // --- Route: local Ollama ---
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let tool_definitions = tools::definitions_for_permissions(&perms);
        let mut final_content = String::new();
        let mut final_thinking = String::new();

        // Multi-turn tool loop for Ollama (up to 5 steps)
        for _step in 0..5 {
            let mut req_body = serde_json::json!({
                "model": target_model,
                "messages": messages_payload,
                "stream": false,
            });
            if !tool_definitions.is_empty() {
                req_body["tools"] = serde_json::to_value(&tool_definitions).unwrap_or_default();
            }

            let res = client.post("http://127.0.0.1:11434/api/chat")
                .json(&req_body)
                .send()
                .await;

            match res {
                Ok(resp) => {
                    if let Ok(body) = resp.json::<serde_json::Value>().await {
                        let message_obj = body.get("message");
                        let content = message_obj.and_then(|m| m.get("content")).and_then(|c| c.as_str()).unwrap_or("").to_string();
                        let thinking = message_obj.and_then(|m| m.get("thinking")).and_then(|t| t.as_str()).unwrap_or("").to_string();
                        if !content.is_empty() {
                            final_content = content;
                        }
                        if !thinking.is_empty() {
                            final_thinking = thinking;
                        }

                        let tool_calls = message_obj.and_then(|m| m.get("tool_calls")).and_then(|tc| tc.as_array()).cloned();
                        if let Some(calls) = tool_calls {
                            if !calls.is_empty() {
                                messages_payload.push(message_obj.cloned().unwrap_or_else(|| serde_json::json!({
                                    "role": "assistant",
                                    "content": final_content,
                                    "tool_calls": calls
                                })));

                                for call in calls {
                                    let fn_obj = call.get("function");
                                    let fn_name = fn_obj.and_then(|f| f.get("name")).and_then(|v| v.as_str()).unwrap_or("");
                                    let fn_args = fn_obj.and_then(|f| f.get("arguments")).cloned().unwrap_or_else(|| serde_json::json!({}));

                                    let exec_result = tools::execute_with_permissions(
                                        fn_name,
                                        &fn_args,
                                        &workspace_path,
                                        &skills,
                                        &perms,
                                    ).await;

                                    let tool_output = match exec_result {
                                        Ok(out) => out,
                                        Err(err) => format!("Error executing {}: {}", fn_name, err),
                                    };

                                    messages_payload.push(serde_json::json!({
                                        "role": "tool",
                                        "name": fn_name,
                                        "content": tool_output,
                                    }));
                                }
                                continue;
                            }
                        }

                        break;
                    } else {
                        return Json(ChatResp {
                            status: "parse_error".into(),
                            model: target_model,
                            reply: "Failed to parse Ollama response".into(),
                            thinking: String::new(),
                            duration_ms: start.elapsed().as_millis() as u64,
                            completed_task: None,
                        });
                    }
                }
                Err(err) => {
                    return Json(ChatResp {
                        status: "connection_error".into(),
                        model: target_model,
                        reply: format!("Could not reach local Ollama on 127.0.0.1:11434: {err}"),
                        thinking: String::new(),
                        duration_ms: start.elapsed().as_millis() as u64,
                        completed_task: None,
                    });
                }
            }
        }

        let _ = state.storage.add_message(conversation_id, "assistant", &final_content, &final_thinking);
        let completed_task = state.storage.complete_next_task(project_id).ok().flatten();
        Json(ChatResp {
            status: "ok".into(),
            model: target_model,
            reply: final_content,
            thinking: final_thinking,
            duration_ms: start.elapsed().as_millis() as u64,
            completed_task,
        })
    }
}


async fn get_credits(State(state): State<AppState>) -> Json<CreditsResp> {
    let or_key = state.storage.get_setting("openrouter_key")
        .unwrap_or(None)
        .unwrap_or_default();

    if or_key.is_empty() {
        return Json(CreditsResp {
            provider: "openrouter".into(),
            label: String::new(),
            usage: 0.0,
            limit: None,
            limit_remaining: None,
            is_free_tier: false,
            rate_limit_requests: 0,
            rate_limit_interval: String::new(),
            error: Some("no_key".into()),
        });
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    match client
        .get("https://openrouter.ai/api/v1/auth/key")
        .bearer_auth(&or_key)
        .send()
        .await
    {
        Ok(resp) => {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                let data = body.get("data").cloned().unwrap_or(serde_json::Value::Null);
                let usage = data.get("usage").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let limit = data.get("limit").and_then(|v| v.as_f64());
                let limit_remaining = limit.map(|l| (l - usage).max(0.0));
                let is_free_tier = data.get("is_free_tier").and_then(|v| v.as_bool()).unwrap_or(false);
                let label = data.get("label").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let rate_req = data.get("rate_limit")
                    .and_then(|r| r.get("requests"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(200) as u32;
                let rate_int = data.get("rate_limit")
                    .and_then(|r| r.get("interval"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("10s")
                    .to_string();
                Json(CreditsResp {
                    provider: "openrouter".into(),
                    label,
                    usage,
                    limit,
                    limit_remaining,
                    is_free_tier,
                    rate_limit_requests: rate_req,
                    rate_limit_interval: rate_int,
                    error: None,
                })
            } else {
                Json(CreditsResp {
                    provider: "openrouter".into(),
                    label: String::new(),
                    usage: 0.0,
                    limit: None,
                    limit_remaining: None,
                    is_free_tier: false,
                    rate_limit_requests: 0,
                    rate_limit_interval: String::new(),
                    error: Some("parse_error".into()),
                })
            }
        }
        Err(err) => Json(CreditsResp {
            provider: "openrouter".into(),
            label: String::new(),
            usage: 0.0,
            limit: None,
            limit_remaining: None,
            is_free_tier: false,
            rate_limit_requests: 0,
            rate_limit_interval: String::new(),
            error: Some(err.to_string()),
        }),
    }
}

async fn get_projects(State(state): State<AppState>) -> Json<Vec<crate::storage::Project>> {
    let list = state.storage.get_projects().unwrap_or_default();
    Json(list)
}

async fn create_project(
    State(state): State<AppState>,
    Json(payload): Json<CreateProjectReq>,
) -> Json<serde_json::Value> {
    let id = state.storage.add_project(
        &payload.name,
        &payload.path,
        payload.trust_level.as_deref(),
        payload.permissions.as_ref(),
    ).unwrap_or(1);
    Json(serde_json::json!({
        "id": id,
        "name": payload.name,
        "path": payload.path,
        "trust_level": payload.trust_level.unwrap_or_else(|| "readwrite".to_string()),
    }))
}

async fn update_project_path(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(payload): Json<UpdateProjectPathReq>,
) -> &'static str {
    let _ = state.storage.update_project_path(id, &payload.path);
    "Path updated"
}

async fn update_project_trust(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(payload): Json<UpdateProjectTrustReq>,
) -> &'static str {
    let _ = state.storage.update_project_trust(id, &payload.trust_level, &payload.permissions);
    "Trust updated"
}

async fn pick_folder_dialog() -> Json<PickFolderResp> {
    let picked = tokio::task::spawn_blocking(|| {
        #[cfg(windows)]
        {
            let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.FolderBrowserDialog
$dialog.Description = "Select Project Folder for DeskPilot"
$dialog.ShowNewFolderButton = $true
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
    Write-Output $dialog.SelectedPath
}
"#;
            let output = std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", script])
                .output();

            if let Ok(out) = output {
                let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !stdout.is_empty() {
                    let path = std::path::PathBuf::from(&stdout);
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("project").to_string();
                    return Some((stdout, name));
                }
            }
        }
        None
    }).await.unwrap_or(None);

    match picked {
        Some((path, name)) => Json(PickFolderResp {
            selected: true,
            path,
            name,
        }),
        None => Json(PickFolderResp {
            selected: false,
            path: String::new(),
            name: String::new(),
        }),
    }
}


async fn get_conversations(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
) -> Json<Vec<crate::storage::Conversation>> {
    let list = state.storage.get_conversations(project_id).unwrap_or_default();
    Json(list)
}

async fn create_conversation(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
    Json(payload): Json<CreateConversationReq>,
) -> Json<serde_json::Value> {
    let id = state.storage.add_conversation(project_id, &payload.title).unwrap_or(1);
    Json(serde_json::json!({ "id": id, "project_id": project_id, "title": payload.title }))
}

async fn get_tasks(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
) -> Json<Vec<crate::storage::TaskItem>> {
    let list = state.storage.get_tasks(project_id).unwrap_or_default();
    Json(list)
}

async fn create_task(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
    Json(payload): Json<CreateTaskReq>,
) -> Json<serde_json::Value> {
    let id = state.storage.add_task(project_id, &payload.title, payload.done.unwrap_or(false)).unwrap_or(0);
    Json(serde_json::json!({ "id": id, "title": payload.title, "done": payload.done.unwrap_or(false) }))
}

async fn toggle_task(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(payload): Json<ToggleTaskReq>,
) -> &'static str {
    let _ = state.storage.update_task_done(id, payload.done);
    "Task updated"
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

