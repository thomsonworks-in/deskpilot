# DeskPilot

DeskPilot is a private, local-first desktop assistant powered by Ollama. It provides a native Rust interface for chatting with models installed on your own machine.

## Features

- Discovers locally installed Ollama models
- Streams responses without blocking the interface
- Preserves conversation context between messages
- Cancels active generations immediately
- Keeps requests on the local Ollama runtime

## Requirements

- Rust stable toolchain
- Ollama running at `http://127.0.0.1:11434`
- At least one locally installed Ollama model

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
