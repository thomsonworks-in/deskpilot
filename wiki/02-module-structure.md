# Module structure

```text
src/
├── main.rs       Native startup and Tokio runtime ownership
├── app.rs        egui state, rendering, input, and event handling
├── message.rs    Shared transcript and Ollama wire types
└── ollama.rs     Model discovery and streaming HTTP client
```

`main.rs` creates one multi-thread Tokio runtime and keeps it alive for the entire native application. `AiHelperApp` owns the UI state and a direct `StreamEvent` receiver. Background tasks send typed events through that channel; the egui thread polls it without blocking.

There is no bridge thread, shared mutex, duplicate state module, or separate UI module.
