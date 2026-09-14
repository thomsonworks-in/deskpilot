# 02 — Module Structure

```text
src/
├── main.rs            Native Windows startup, single-instance enforcement, Tokio runtime
├── app.rs             egui immediate-mode GUI, custom titlebar, input bar, model manager, tool drawers
├── message.rs         Transcript types, Ollama JSON payloads, PullProgress, and Cloud Provider schemas
├── ollama.rs          Ollama client, /api/pull stream parser, Cloud SSE chat proxy, bootstrap sync
├── storage.rs         SQLite WAL database manager, adaptive memory scratchpad, logs, tasks, projects
├── tools.rs           Native tool harness (powershell, web_search, curl, files, skills discovery)
├── single_instance.rs Named mutex lock and IPC window focusing
└── ipc.rs             Local Windows named pipe / socket IPC
```

---

## Key Modules & Responsibilities

1. **`main.rs`**: Owns the multi-threaded Tokio runtime, enforces single application instance, and launches the `eframe` window viewport.
2. **`app.rs`**: Renders the complete dark-glass UI, manages the event loop, handles model selection & download dialogs, and orchestrates agent message generation.
3. **`tools.rs`**: Implements the tool-calling harness compatible with Claude Code and Codex patterns:
   - `powershell`: Sandboxed Windows command runner.
   - `web_search`: DuckDuckGo / web HTML scraper.
   - `read_file` / `write_file`: Workspace-scoped file I/O.
   - `read_skill`: Loads `SKILL.md` files dynamically.
4. **`storage.rs`**: Provides SQLite storage (`%LOCALAPPDATA%\DeskPilot\deskpilot.db`) implementing the Adaptive Memory Layer.
5. **`ollama.rs`**: Manages HTTP streaming to local Ollama (`http://127.0.0.1:11434`) and cloud providers via OpenAI-compatible endpoints.
