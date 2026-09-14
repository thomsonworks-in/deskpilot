# 11 — Adaptive Memory & Agent Harness

## 1. Adaptive Memory Layer (Global Architecture)

DeskPilot implements the 4-layer database-driven memory spec using embedded SQLite WAL storage at `%LOCALAPPDATA%\DeskPilot\deskpilot.db`:

```
┌──────────────────────────────────────────────────────────┐
│                   WORKING MEMORY (L1)                    │
│   Active conversation messages in-memory buffer          │
├──────────────────────────────────────────────────────────┤
│                  EPISODIC SCRATCHPAD (L2)                │
│   Fast O(1) table tracking recent workspace context      │
├──────────────────────────────────────────────────────────┤
│                  SEMANTIC MEMORY (L3)                    │
│   Indexed facts with vector embeddings & confidence score│
├──────────────────────────────────────────────────────────┤
│                  COLD STORAGE ARCHIVE (L4)               │
│   Long-term historical databases and wikis               │
└──────────────────────────────────────────────────────────┘
```

* **Dynamic Context Injection**: Every prompt automatically pulls the latest 15 scratchpad items and relevant semantic nodes, prepending them into the system prompt with zero lookup latency.

---

## 2. Combined Agent Harness (Claude Code + Codex + Open)

DeskPilot combines the best patterns from leading agent architectures:

* **Skill Auto-Discovery**: Traverses `.claude/skills`, `.gemini/skills`, and user profile directories for `SKILL.md` documents.
* **Safe Sandbox Enforcement**: Blocks destructive commands (`rm -rf`, `format`, `shutdown`, `del /`) while allowing native PowerShell automation.
* **Live Search & Browsing**: Provides `web_search` and `curl` primitives so the agent never fails on current knowledge queries.
* **Streamed Tool Calling**: Shows collapsible tool traces directly inline in the chat transcript.
