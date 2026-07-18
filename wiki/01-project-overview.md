# 01 — Project Overview

## Goal

Build a genuinely native Rust desktop chat client that streams responses from a local Ollama server. No HTML, React, Tauri, Electron, WebView, or browser engine. Pure egui + Rust.

## Scope (v1)

- Chat transcript showing user and assistant messages
- Multiline input field
- Send button (Enter sends, Shift+Enter inserts newline)
- Stop button while generation is active
- Model selector populated from local Ollama (`http://127.0.0.1:11434`)
- Streaming assistant responses from Ollama
- Visible connection indicator and readable errors
- Default model: `qwen3.6:35b-a3b`
- UI remains responsive during generation

## Out of Scope (v1)

- Memory / conversation history persistence
- Tools, function calling, or file editing
- Embeddings or RAG
- Authentication or remote servers
- Packaging / installer / distribution
- Theming beyond basic egui defaults
- Multi-window or tray icon

## Tech Stack

| Layer | Technology |
|-------|-----------|
| UI | `eframe` / `egui` (native Rust GUI) |
| Async runtime | `tokio` |
| HTTP + streaming | `reqwest` with `stream` feature |
| Serialization | `serde` + `serde_json` |
| Streams | `futures-util` |
| Errors | `anyhow` |

## Design Principles

1. **Native first** — No embedded browser, no web tech
2. **Small footprint** — Target ~3–8 MB binary, low RAM
3. **Responsive** — Async streaming never blocks the UI thread
4. **Simple** — Minimal modules, clear data flow, no over-engineering
