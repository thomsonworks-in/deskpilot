# DeskPilot

DeskPilot is a blazingly fast, private, local-first desktop agent and assistant built in pure Rust. It connects to local models via Ollama and provides agent execution, semantic vector memory, multi-project context management, and native OS integration with zero Electron or Chromium overhead.

## 🚀 Quick Install (Single-Line, Zero Dependencies)

No Node.js, Rust, Python, or Git required for installation.

### Windows (PowerShell):
Open PowerShell and run:
\\powershell
irm https://raw.githubusercontent.com/thomsonworks-in/deskpilot/main/install.ps1 | iex
\*This downloads the latest pre-compiled binary, installs it to \%LOCALAPPDATA%\DeskPilot\bin\, adds it to your PATH, and creates a Start Menu shortcut.*

### macOS & Linux (Terminal):
Open Terminal and run:
\\ash
curl -fsSL https://raw.githubusercontent.com/thomsonworks-in/deskpilot/main/install.sh | sh
\*Supports Apple Silicon (M1/M2/M3/M4), Intel Mac, and x86_64/aarch64 Linux.*

---

## ⚡ Features

- **Blazing Native Performance:** Built with \egui\/\eframe\. Uses ~30MB RAM on idle and launches instantly.
- **Autonomous Agent Execution:** Built-in tools for workspace file management, PowerShell/Bash command execution, and live skill loading.
- **Local Semantic Vector Memory:** Stores durable memories, scratchpads, and embeddings in local SQLite with automatic semantic retrieval using Ollama embeddings.
- **Multi-Project Workspaces:** Seamlessly switch between different project contexts and chat threads.
- **Native OS Integration:** System tray support, single-instance enforcement via named pipes, and duplicate launch window focusing.
- **Zero Cloud Leaks:** All requests, embeddings, and chat data remain 100% on your local machine.

---

## 🛠️ Requirements (For Running)

- [Ollama](https://ollama.com) running locally (\http://127.0.0.1:11434\).
- Any locally pulled model (e.g. \qwen2.5:coder\, \deepseek-r1\, \llama3.2\).
- Recommended embedding model: \qwen3-embedding:0.6b\ (automatic fallback to recency if not installed).

---

## 💻 Development & Building from Source

If you have Rust installed and prefer building from source:

\\ash
git clone https://github.com/thomsonworks-in/deskpilot.git
cd deskpilot
cargo run --release
\
To test:
\\ash
cargo test
\
---

## 📂 Local Data & Privacy

DeskPilot stores its local SQLite database at:
- **Windows:** \%LOCALAPPDATA%\DeskPilot\deskpilot.db- **macOS/Linux:** \~/.deskpilot/deskpilot.db
---

## 📄 License

GNU General Public License v3.0 (GPLv3). See [LICENSE](LICENSE) for details.
