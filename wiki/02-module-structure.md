# 02 — Module Structure

```text
src/
├── main.rs              [2KB]   Tokio runtime bootstrap, single-instance guard, browser auto-launch
├── ipc.rs               [50KB]  Axum HTTP server (127.0.0.1:31415), 28 REST API endpoints, agent chat loop
├── web_ui.html          [140KB] Embedded single-page web UI (HTML + Tailwind CSS + vanilla JS)
├── storage.rs           [37KB]  SQLite WAL database (11 tables), adaptive memory, project/task/gotcha CRUD
├── message.rs           [5KB]   Message/Role types, wire protocol structs (ChatChunk, PullProgress, etc.)
├── single_instance.rs   [1KB]   Named mutex lock and IPC window focusing
├── tools.rs             [22KB]  STANDALONE: Agentic tools and skill scanner (unhooked from root module tree)
├── ollama.rs            [22KB]  STANDALONE: Legacy Ollama client & streaming (unhooked from root module tree)
└── app.rs.legacy        [97KB]  ARCHIVED: Former native egui GUI (do NOT reference for new features)
```

> **Build Optimization Note:** In release v0.1.3+, `src/main.rs` only mounts active daemon services (`ipc`, `message`, `single_instance`, `storage`). This guarantees `cargo build --release` runs with **0 warnings and 0 errors**.

---

## Key Modules & Responsibilities

1. **`main.rs`** [L1-58]: Entry point. Acquires single-instance mutex, initializes `Storage`, spawns `ipc::start_server()` on `127.0.0.1:31415`, auto-opens browser (unless `--headless`/`--no-open`), handles Ctrl+C graceful shutdown.

2. **`ipc.rs`** [L1-1287]: The application backend. Axum router with 28 REST endpoints serving the web UI and handling all client ↔ daemon communication. Contains the core **agent chat loop** (`handle_chat`, L436-923) which orchestrates multi-step tool execution (up to 5 iterations), workspace context injection, skill/MCP tool discovery, and cloud/local LLM routing.

3. **`web_ui.html`** [L1-2262]: The entire frontend. A single-file SPA with Tailwind CSS, FontAwesome icons, and vanilla ES6 JavaScript. 3-column layout: left sidebar (projects/threads), center canvas (chat + living spec), right inspector (gotchas/repo map). 5-tab settings modal. ~50 JS functions communicating via `fetch()` to the 28 REST endpoints.

4. **`storage.rs`** [L1-785]: SQLite database layer at `%LOCALAPPDATA%\DeskPilot\deskpilot.db` (WAL mode). Manages 11 tables: `settings`, `projects`, `conversations`, `messages`, `tasks`, `memories`, `scratchpad`, `triggers`, `providers`, `gotchas`, `logs`. Handles schema migrations, CRUD operations, and adaptive memory (scratchpad decay, embedding storage).

5. **`tools.rs`** [L1-389]: Defines and executes 14 agent tools: `read_file`, `list_files`, `read_skill`, `repo_map`, `web_search`, `curl`, `powershell`, `bash`, `write_file`, `replace_file_content`, `rollback_workspace`, `git_commit`, `delegate_subagent`, `self_update`. All gated by `ProjectPermissions`. Safety blacklist blocks destructive commands.

6. **`ollama.rs`** [L1-432]: HTTP client for local Ollama (`127.0.0.1:11434`) and OpenAI-compatible cloud providers. Handles NDJSON streaming (`/api/chat`), SSE streaming (`/chat/completions`), model pull progress, embedding, model loading with automatic CUDA crash → CPU fallback, and Control server bootstrap sync.

7. **`mcp.rs`** [L1-279]: Discovers MCP server configs from workspace/global `mcp_servers.json`, spawns child processes over stdio, completes JSON-RPC 2.0 handshake (`initialize` → `notifications/initialized`), and executes `tools/call` with 30s timeout.

8. **`message.rs`** [L1-160]: Core data types: `Role` enum (System/User/Assistant), `Message` struct (with `tool_uses` tracking), wire protocol structs for Ollama API (`ChatRequest`, `ChatChunk`, `EmbedRequest`, `PullProgress`), and `ProviderConfig` for cloud bootstrap.
