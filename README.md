# DeskPilot

> **The ultra-fast, local-first autonomous AI desktop agent and workspace assistant built in pure Rust.**

DeskPilot gives you the power of an autonomous AI agent running directly on your operating system. Whether you run 100% private local models (via runtimes like Ollama, llama.cpp, or vLLM) or route to frontier cloud reasoning models (Claude 3.7, DeepSeek-R1, GPT-4o via OpenRouter), DeskPilot gives you instant startup, autonomous file and shell execution, and persistent vector memory with **zero Electron or Chromium RAM bloat**.

---

## ⚡ Why DeskPilot? (Key Differentiators)

- 🚀 **Blazing Native Performance (~30MB RAM):** Built entirely in Rust with `egui`/`eframe`. Launches in milliseconds and uses a fraction of the RAM of typical Electron or web-based AI clients.
- 🔒 **100% Private, Local-First Architecture:** Run offline models directly on your hardware. Your files, embeddings, and chat history stay on your machine in encrypted local SQLite.
- 🧠 **Persistent SQLite Vector Memory:** Built-in semantic retrieval and episodic scratchpads. DeskPilot recalls past project facts, user instructions, and technical context across reboots with zero lookup lag.
- 🛠️ **Autonomous Agent Execution & Tools:** Built-in workspace file editing, PowerShell/Bash command execution, web browsing, git commit automation, and live **Claude Code / Codex `SKILL.md`** discovery.
- 🌐 **Hybrid Local + Multi-Model Routing:** Switch on the fly between your **Local Engine** (e.g. Ollama, local models) and **Cloud Engines** (OpenRouter, DeepSeek API, Anthropic Claude, OpenAI).
- 🖥️ **Native OS Integration:** System tray support, single-instance enforcement, and native workspace switching.

---

## 🚀 Quick Install (Single-Line, Zero Dependencies)

No Node.js, Rust, Python, or Git required. Single binary installation.

### Windows (PowerShell):
Open PowerShell and run:
```powershell
irm https://raw.githubusercontent.com/thomsonworks-in/deskpilot/main/install.ps1 | iex
```
*(Or from Command Prompt `cmd.exe`: `powershell -Command "irm https://raw.githubusercontent.com/thomsonworks-in/deskpilot/main/install.ps1 | iex"`)*  
*Downloads the pre-compiled binary to `%LOCALAPPDATA%\DeskPilot\bin`, configures your PATH, and adds a Start Menu shortcut.*

### macOS & Linux (Terminal):
Open Terminal and run:
```bash
curl -fsSL https://raw.githubusercontent.com/thomsonworks-in/deskpilot/main/install.sh | sh
```
*Supports Apple Silicon (M1/M2/M3/M4), Intel Macs, and x86_64/aarch64 Linux distributions.*

---

## 🔌 Supported Engines & Models

| Mode | Supported Runtimes & Providers | Example Models |
| :--- | :--- | :--- |
| **Local (Private / Offline)** | Local runtimes such as [Ollama](https://ollama.com), local OpenAI-compatible runners | DeepSeek-R1 (1.5B–32B), Qwen 2.5 Coder, Llama 3.3, Mistral |
| **Cloud (Frontier Reasoning)** | [OpenRouter](https://openrouter.ai), DeepSeek API, Anthropic, OpenAI | Claude 3.7 Sonnet, DeepSeek-V3 / R1, GPT-4o, Gemini 2.0 |
| **Semantic Memory** | Local embedding engines (e.g. Ollama `qwen3-embedding:0.6b` or recent-fallback) | Zero-latency local vector similarity matching |

---

## 🛠️ Usage & Navigation

- **💬 Chat View:** Clean conversational canvas with real-time streaming and collapsible reasoning/thinking steps.
- **🌐 Provider Pill:** Toggle instantly in the bottom input bar between `🖥 Local Engine` and `⚡ Cloud / OpenRouter`.
- **⚙ Settings & API Keys:** Configure your OpenRouter keys, default models, and view local runtime status.
- **🛠 Skills Catalog:** Automatically loads custom workflows and agent skills from `<workspace>/.claude/skills` and user directories.

---

## 💻 Building From Source (Developers)

If you have Rust installed:

```bash
git clone https://github.com/thomsonworks-in/deskpilot.git
cd deskpilot
cargo run --release
```

To test:
```bash
cargo test
```

---

## 📂 Local Data & Privacy

DeskPilot stores its local SQLite database at:
- **Windows:** `%LOCALAPPDATA%\DeskPilot\deskpilot.db`
- **macOS/Linux:** `~/.deskpilot/deskpilot.db`

All conversation logs, project contexts, vector embeddings, and API keys are stored locally.

---

## 📄 License

GNU General Public License v3.0 (GPLv3). See [LICENSE](LICENSE) for details.
