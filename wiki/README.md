# DeskPilot — Project Wiki

Welcome to the DeskPilot project wiki. This wiki documents the design, architecture, and decisions for this private, local-first Rust desktop assistant.

## Table of Contents

| # | Document | Description |
|---|----------|-------------|
| 01 | [Project Overview](./01-project-overview.md) | Goals, scope, tech stack, and what's intentionally excluded |
| 02 | [Module Structure](./02-module-structure.md) | File layout and each module's responsibilities |
| 03 | [Dependencies](./03-dependencies.md) | Cargo.toml details, version choices, and feature flags |
| 04 | [Data Flow Architecture](./04-data-flow.md) | Channel types, AppState structure, and task wiring diagram |
| 05 | [Cancellation Design](./05-cancellation-design.md) | oneshot-based cancellation flow and lifecycle |
| 06 | [Ollama API Integration](./06-ollama-integration.md) | Model listing, streaming chat, request/response types |
| 07 | [UI Design (egui)](./07-ui-design.md) | Layout, controls, keyboard shortcuts, and visual states |
| 08 | [Verification Plan](./08-verification-plan.md) | Manual test checklist, build verification, error handling tests |
| 09 | [Implementation Order](./09-implementation-order.md) | Step-by-step build sequence with file creation order |
| 10 | [Risks & Mitigations](./10-risks-mitigations.md) | Known risks and how they're addressed |

## Quick Links

- **[README.md](../README.md)** — Root project readme with quick start instructions
- **[Cargo.toml](../Cargo.toml)** — Dependency manifest
- **[src/](../src/)** — Source code directory
