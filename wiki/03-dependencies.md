# 03 — Dependencies
 
| Crate | Version | Purpose |
|---|---|---|
| `axum` | `0.7` | HTTP REST API server & Web Studio host on `127.0.0.1:31415` |
| `tokio` | `1` (`features = ["full"]`) | Async runtime, process spawning, file locks, graceful shutdown |
| `tokio-stream` | `0.1` | Asynchronous streaming utilities |
| `reqwest` | `0.12` (`features = ["stream", "json"]`) | HTTP client for Ollama, OpenRouter, and custom LLM providers |
| `serde`, `serde_json` | `1` | Request serialization, deserialization, JSON-RPC, dynamic payloads |
| `rusqlite` | `0.32` (`features = ["bundled"]`) | Embedded SQLite WAL database persistence at `%LOCALAPPDATA%\DeskPilot\deskpilot.db` |
| `single-instance` | `0.3` | Single-instance process mutex guard and focus dispatcher |
| `chrono` | `0.4` | Timestamps for messages, conversations, and audit logs |
| `anyhow` | `1` | Context-rich error handling across daemon services |
| `futures-util` | `0.3` | Stream polling and stream transformations |
| `tray-icon` | `0.19` | System tray integration support |
| `windows-sys` | `0.59` | Win32 message loop & OS window integration on Windows targets |

> **Note:** The legacy `egui` and `eframe` GUI dependencies have been completely removed. DeskPilot runs as a headless binary with zero native window overhead.
