# ThomsonWorks DeskPilot

> **The ultra-fast, local-first AI desktop agent — a headless Rust daemon with an Obsidian Velocity Web Studio.**
>
> *Official repository by [ThomsonWorks](https://github.com/thomsonworks-in).*

DeskPilot runs as a **tiny background daemon** (`~10MB binary, ~12MB RAM idle`) and serves a polished web UI at `http://127.0.0.1:31415`. It auto-discovers every local [Ollama](https://ollama.com) model on startup, routes messages to your chosen model, surfaces full thinking-chain traces, and persists all history in local SQLite — **with zero data ever leaving your machine.**

![DeskPilot Obsidian Velocity Web Studio](assets/deskpilot_studio.png)

---

## ⚡ Why DeskPilot? (Key Features)

| Feature | Details |
| :--- | :--- |
| 🚀 **~10MB Binary · ~12MB RAM Idle** | Pure Rust headless daemon. 56% smaller than a native GUI app. Starts in milliseconds. |
| 🔒 **100% Private · Zero Telemetry** | All data stays on your machine. Works fully offline. No accounts. No subscriptions. |
| 🧠 **Thinking-Chain Traces** | Collapsible 💡 reasoning traces shown inline — see exactly how the model thinks. |
| 🤖 **Auto Local Model Detection** | Detects all running Ollama models at launch. Switch models from a live dropdown. |
| 💾 **SQLite WAL Persistence** | 12-message rolling context window. All conversations stored locally across reboots. |
| ⚡ **`dp` — 2-Character Launcher** | Type `dp` to start or focus DeskPilot. Smart: boots if stopped, focuses if running. |
| 🌐 **Obsidian Velocity Web Studio** | Muted amethyst dark-mode UI at `localhost:31415`. Works in any browser, no install. |
| 🔀 **Multi-Model Routing** | Route to local Ollama models or cloud frontier models (OpenRouter, Anthropic, OpenAI). |

---

## 🚀 1-Click Install & Run (Zero Dependencies)

No Node.js, Rust, Python, or Git required. Single binary installation that automatically sets up DeskPilot, creates shortcuts, and launches the Obsidian Velocity Web Studio in your browser.

### Windows (PowerShell):
Open PowerShell and run:
```powershell
irm https://raw.githubusercontent.com/thomsonworks-in/deskpilot/main/install.ps1 | iex
```
*(Or from Command Prompt `cmd.exe`: `powershell -c "irm https://raw.githubusercontent.com/thomsonworks-in/deskpilot/main/install.ps1 | iex"`)*  
*Downloads pre-compiled binary to `%LOCALAPPDATA%\DeskPilot\bin`, configures PATH, creates the `dp` launcher, and instantly opens `http://127.0.0.1:31415`.*

### macOS & Linux (Terminal):
Open Terminal and run:
```bash
curl -fsSL https://raw.githubusercontent.com/thomsonworks-in/deskpilot/main/install.sh | sh
```
*Supports Apple Silicon (M1–M4), Intel Macs, and Linux. Automatically adds `dp` to your shell PATH and opens the Web Studio.*

---

## ⚡ Ultra-Short One-Liner Run Command: `dp`

Once installed, launch DeskPilot anytime with just **2 characters**:

```bash
dp
```

- **If DeskPilot is stopped:** `dp` boots the headless daemon in milliseconds and opens `http://127.0.0.1:31415`.
- **If DeskPilot is already running:** `dp` instantly brings the Web Studio into focus in your browser.
- **Headless Background Service (no browser):**
  ```bash
  deskpilot --headless
  ```

---

## 🔌 Supported Engines & Models

| Mode | Supported Runtimes & Providers | Example Models |
| :--- | :--- | :--- |
| **Local (Private / Offline)** | [Ollama](https://ollama.com), any OpenAI-compatible local runner | Qwen3, DeepSeek-R1, Llama 3.3, Mistral, Gemma 4 |
| **Cloud (Frontier Reasoning)** | [OpenRouter](https://openrouter.ai), DeepSeek API, Anthropic, OpenAI | Claude Sonnet, DeepSeek-V3/R1, GPT-4o, Gemini |
| **Semantic Memory** | SQLite WAL with rolling context + project adaptive context | Zero-latency local retrieval |

---

## 🛠️ Usage & Navigation

- **💬 Chat:** Conversational canvas with user/assistant bubbles, per-model timing badge, and collapsible 💡 thinking-chain traces.
- **🤖 Model Selector:** Live dropdown in the header — grouped into `⚡ Local Offline (Zero Cost)` and `🌐 Cloud Frontier` with `💡 Reasoning` / `⚡ Fast` badges.
- **⚙ Settings:** Configure API keys, default model, and view local runtime status via `/api/settings`.
- **🛠 REST API:** Full JSON API at `:31415` — `/api/models`, `/api/chat`, `/api/settings`, `/api/message`.

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
