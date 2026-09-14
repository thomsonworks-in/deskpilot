# 01 — Project Overview

## Goal

Build an enterprise-grade, private native Rust desktop thin client and autonomous agent harness (`deskpilot.exe`). Zero HTML/Electron/Node.js runtime overhead with native `egui` GPU rendering, local Ollama streaming, live internet search, multi-provider cloud failover, and persistent SQLite adaptive memory.

---

## Core Capabilities (v2.0)

- **Pure Native Execution**: Standalone ~11.7 MB Windows executable (`deskpilot.exe`).
- **Live Internet Search**: Built-in `web_search` and `curl` scrapers with zero cutoff limitations.
- **Model Lifecycle & Pull Manager**: In-app dialog with live streaming progress bar (`MB/GB`, percentage) to download models from Ollama (`/api/pull`).
- **Multi-Provider & Control Sync**: Dynamic bootstrap synchronization from `Control` backend (`/api/v1/cli/bootstrap`) with automatic cloud failover (OpenRouter, Groq, HuggingFace).
- **Default Adaptive Memory Layer**: Embedded SQLite (`rusqlite`) storing:
  - `scratchpad` table for fast O(1) context recall with decay tracking.
  - `memories` table with semantic embeddings and confidence scoring.
- **Agentic Tool Harness**:
  - Native PowerShell command execution with safety boundaries.
  - Workspace file reader & writer (`read_file`, `write_file`, `list_files`).
  - Skill auto-discovery (`.claude/skills`, `.gemini/skills`).
  - Multi-step tool execution loop with collapsible UI traces.

---

## Tech Stack

| Layer | Technology |
|---|---|
| **UI** | `eframe` / `egui` 0.31 (GPU-accelerated immediate mode GUI) |
| **Markdown** | `egui_commonmark` with dark code highlighting |
| **Async Runtime** | `tokio` (multi-threaded) |
| **HTTP & Networking** | `reqwest` with streaming JSON & SSE |
| **Storage & Memory** | `rusqlite` (bundled SQLite with WAL mode) |
| **Shell & OS** | Native Win32 / PowerShell / Command subprocesses |
| **System Tray** | `tray-icon` + `windows-sys` single-instance IPC |
