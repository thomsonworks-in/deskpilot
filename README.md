# DeskPilot

DeskPilot is a private, local-first desktop assistant powered by Ollama. It provides a native Rust interface for chatting with models installed on your own machine.

## Features

- Discovers locally installed Ollama models
- Streams responses without blocking the interface
- Preserves conversation context between messages
- Cancels active generations immediately
- Tracks assistant and user-created tasks
- Stores durable memories in local SQLite
- Creates vector embeddings for semantic memory retrieval
- Provides an in-app diagnostic log viewer
- Enforces one application instance and focuses the existing window on relaunch
- Keeps requests on the local Ollama runtime

## Requirements

- Rust stable toolchain
- Ollama running at `http://127.0.0.1:11434`
- At least one locally installed Ollama model
- `qwen3-embedding:0.6b` for semantic memory (recent-memory fallback is automatic)

## Local data

DeskPilot stores its SQLite database at `%LOCALAPPDATA%\DeskPilot\deskpilot.db`. The database contains conversations, tasks, memories, embedding vectors, settings, and diagnostic logs. Nothing is uploaded by DeskPilot.

## Run

```powershell
cargo run
```

## Verify

```powershell
cargo fmt -- --check
cargo test
cargo clippy -- -D warnings
```

Architecture notes are available in the [project wiki](wiki/README.md).
