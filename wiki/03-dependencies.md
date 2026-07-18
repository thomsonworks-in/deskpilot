# Dependencies

| Crate | Purpose |
|---|---|
| `eframe`, `egui` | Native window and immediate-mode UI |
| `tokio` | Background runtime, channels, and cancellation |
| `reqwest` | Ollama HTTP requests and byte streaming |
| `serde`, `serde_json` | Request serialization and NDJSON parsing |
| `futures-util` | Incremental response stream polling |
| `anyhow` | Context-rich application errors |

The HTTP client has a 10-second connection timeout but no total request timeout, allowing long-running streamed generations. `Cargo.lock` is committed because DeskPilot is an executable application.
