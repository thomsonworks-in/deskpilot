# DeskPilot — Project Wiki

Welcome to the DeskPilot project wiki. This wiki documents the design, architecture, and decisions for this private, local-first Rust desktop assistant.

> **Architecture (v3.0 — Web Studio):** DeskPilot is a **headless Rust daemon** serving a **single-page web application** at `http://127.0.0.1:31415`. The former native egui GUI (`app.rs`) has been archived as `app.rs.legacy`. All UI is now in `web_ui.html`.

## Table of Contents

| # | Document | Description |
|---|----------|-------------|
| 01 | [Project Overview](./01-project-overview.md) | Goals, scope, tech stack (Web Studio SPA + Axum daemon) |
| 02 | [Module Structure](./02-module-structure.md) | File layout with sizes, each module's responsibilities |
| 03 | [Dependencies](./03-dependencies.md) | Cargo.toml details, version choices, and feature flags |
| 04 | [Data Flow Architecture](./04-data-flow.md) | REST-based client↔daemon flow, chat loop, startup sequence |
| 05 | [Cancellation Design](./05-cancellation-design.md) | oneshot-based cancellation flow and lifecycle |
| **06** | [**IPC Endpoint Registry**](./06-ipc-endpoint-registry.md) | **⭐ O(1) lookup: all 28 REST API endpoints with line numbers** |
| **07** | [**Web UI Component Registry**](./07-webui-component-registry.md) | **⭐ O(1) lookup: all HTML components, JS functions, CSS tokens** |
| 08 | [Storage & Database Schema](./08-storage-schema.md) | SQLite tables, Storage API, all CRUD functions |
| 09 | [Tool Harness Registry](./09-tool-harness-registry.md) | 14 agent tools with permissions, safety policy |
| **10** | [**Security & Context Architecture**](./10-security-and-context-architecture.md) | **⭐ Origin isolation, XSS protection, context window & sliding limits** |
| 11 | [Adaptive Memory & Harness](./11-adaptive-memory-harness.md) | 4-Layer memory spec, tool safety boundaries |

## Agent Quick Reference

**Before reading ANY source file, check these registries first:**
- Need to modify a **REST endpoint**? → [06-ipc-endpoint-registry.md](./06-ipc-endpoint-registry.md)
- Need to modify **UI components or JS functions**? → [07-webui-component-registry.md](./07-webui-component-registry.md)
- Need to modify **database/storage**? → [08-storage-schema.md](./08-storage-schema.md) *(or check 02-module-structure.md)*
- Need to modify **agent tools**? → [09-tool-harness-registry.md](./09-tool-harness-registry.md) *(or check 02-module-structure.md)*

## Quick Links

- **[README.md](../README.md)** — Root project readme with quick start instructions
- **[Cargo.toml](../Cargo.toml)** — Dependency manifest
- **[src/](../src/)** — Source code directory
