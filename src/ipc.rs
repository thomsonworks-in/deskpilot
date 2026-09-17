use axum::{
    extract::{Path, State},
    response::Html,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::storage::Storage;

#[derive(Deserialize)]
pub struct IpcMessage {
    pub content: String,
}

#[derive(Deserialize)]
pub struct CreateTriggerReq {
    pub project_id: u64,
    pub name: String,
    pub trigger_type: String,
    pub schedule_expr: String,
    pub action_type: String,
    pub action_payload: String,
}

#[derive(Deserialize)]
pub struct SaveSettingsReq {
    pub openrouter_key: Option<String>,
    pub openrouter_model: Option<String>,
    pub active_provider: Option<String>,
}

#[derive(Clone)]
struct AppState {
    tx: mpsc::UnboundedSender<String>,
    storage: Arc<Storage>,
}

const WEB_UI_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>DeskPilot Studio • Autonomous Agent & Workspace</title>
    <script src="https://cdn.tailwindcss.com"></script>
    <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.4.0/css/all.min.css">
    <style>
        body { background-color: #0b0f17; color: #e2e8f0; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; }
        ::-webkit-scrollbar { width: 5px; height: 5px; }
        ::-webkit-scrollbar-thumb { background: #1e293b; border-radius: 4px; }
        ::-webkit-scrollbar-thumb:hover { background: #334155; }
        .glass-panel { background: rgba(15, 23, 42, 0.75); backdrop-filter: blur(12px); border: 1px solid rgba(255, 255, 255, 0.08); }
        .active-tab { background: #1e293b; color: #38bdf8; border-left: 3px solid #38bdf8; }
        .tab-btn { transition: all 0.15s ease; }
        .tab-btn:hover { background: rgba(255, 255, 255, 0.04); color: #f8fafc; }
    </style>
</head>
<body class="flex flex-col h-screen overflow-hidden select-none">
    <!-- Top Global App Bar -->
    <header class="flex items-center justify-between px-5 py-2.5 bg-[#0f172a] border-b border-slate-800/80 z-20">
        <div class="flex items-center space-x-4">
            <div class="flex items-center space-x-2.5">
                <div class="w-8 h-8 rounded-lg bg-gradient-to-br from-emerald-400 to-sky-500 flex items-center justify-center text-black font-bold text-sm shadow-md">
                    ⚡
                </div>
                <div>
                    <h1 class="text-xs font-bold tracking-wider uppercase text-white flex items-center gap-1.5">
                        ThomsonWorks DeskPilot
                        <span class="text-[9px] px-1.5 py-0.5 rounded bg-sky-950 text-sky-400 border border-sky-800 font-mono font-normal">v0.1.0 PRO</span>
                    </h1>
                    <p class="text-[10px] text-slate-400">Autonomous Desktop Agent & Hybrid Reasoning Harness</p>
                </div>
            </div>

            <div class="h-4 w-px bg-slate-800"></div>

            <!-- Workspace Selector -->
            <div class="flex items-center space-x-2 bg-slate-900/90 border border-slate-800 rounded-lg px-2.5 py-1 text-xs text-slate-300">
                <i class="fa-solid fa-folder-tree text-sky-400 text-xs"></i>
                <select id="project-select" onchange="switchProject(this.value)" class="bg-transparent border-none focus:outline-none text-xs text-slate-200 cursor-pointer">
                    <option value="1">Workspace: Control</option>
                    <option value="2">Workspace: DeskPilot</option>
                </select>
            </div>
        </div>

        <!-- Telemetry & Quick Indicators -->
        <div class="flex items-center space-x-5 text-xs">
            <div class="flex items-center space-x-2 text-slate-400">
                <span class="inline-block w-2 h-2 rounded-full bg-emerald-400 animate-pulse"></span>
                <span class="text-[11px] font-mono text-slate-300">Rust Daemon :31415 (Active)</span>
            </div>
            <div class="flex items-center space-x-2 bg-slate-900 border border-slate-800 rounded-lg px-3 py-1">
                <i class="fa-solid fa-microchip text-slate-400 text-xs"></i>
                <span id="active-model-badge" class="text-[11px] font-mono text-emerald-400">Claude 3.7 / Ollama</span>
            </div>
        </div>
    </header>

    <!-- App Body: 3 Columns (Nav, Main Studio, Right Inspector) -->
    <div class="flex-1 flex overflow-hidden">
        
        <!-- Left Navigation & Thread Rail -->
        <aside class="w-64 bg-[#0a0f18] border-r border-slate-800/80 flex flex-col justify-between p-3 select-none">
            <div class="space-y-4">
                <!-- Navigation Modes -->
                <nav class="space-y-1">
                    <button onclick="switchView('chat')" id="nav-chat" class="tab-btn active-tab w-full flex items-center space-x-3 px-3 py-2 rounded-lg text-xs font-medium text-left">
                        <i class="fa-solid fa-terminal w-4 text-center"></i>
                        <span>Live Agent Studio</span>
                    </button>
                    <button onclick="switchView('triggers')" id="nav-triggers" class="tab-btn w-full flex items-center space-x-3 px-3 py-2 rounded-lg text-xs font-medium text-slate-400 text-left">
                        <i class="fa-solid fa-bolt-lightning w-4 text-center text-amber-400"></i>
                        <span>Triggers & Schedules</span>
                    </button>
                    <button onclick="switchView('mcp')" id="nav-mcp" class="tab-btn w-full flex items-center space-x-3 px-3 py-2 rounded-lg text-xs font-medium text-slate-400 text-left">
                        <i class="fa-solid fa-puzzle-piece w-4 text-center text-purple-400"></i>
                        <span>MCP Plugins & Skills</span>
                    </button>
                    <button onclick="switchView('settings')" id="nav-settings" class="tab-btn w-full flex items-center space-x-3 px-3 py-2 rounded-lg text-xs font-medium text-slate-400 text-left">
                        <i class="fa-solid fa-sliders w-4 text-center text-sky-400"></i>
                        <span>Vault & Engine Keys</span>
                    </button>
                </nav>

                <div class="border-t border-slate-800/60 pt-3">
                    <div class="flex items-center justify-between px-2 mb-2">
                        <span class="text-[10px] uppercase font-bold tracking-wider text-slate-400">Sessions</span>
                        <button onclick="newSession()" class="text-slate-400 hover:text-white text-xs"><i class="fa-solid fa-plus"></i></button>
                    </div>
                    <div class="space-y-0.5 overflow-y-auto max-h-56">
                        <div class="p-2 rounded-md bg-slate-900/80 border border-slate-800 text-xs text-slate-200 cursor-pointer flex items-center justify-between">
                            <span class="truncate">#42 • Fix Workspace Rollback</span>
                            <span class="text-[9px] text-emerald-400">Active</span>
                        </div>
                        <div class="p-2 rounded-md text-slate-400 hover:bg-slate-900/40 text-xs cursor-pointer flex items-center justify-between">
                            <span class="truncate">#41 • Dynamic Catalog Discovery</span>
                            <span class="text-[9px] text-slate-400">Done</span>
                        </div>
                    </div>
                </div>
            </div>

            <!-- Workspace Safety Indicator -->
            <div class="p-2.5 rounded-lg bg-slate-900/60 border border-slate-800/80 text-[11px] text-slate-400 space-y-1">
                <div class="flex items-center justify-between">
                    <span class="flex items-center gap-1.5 text-emerald-400 font-semibold"><i class="fa-solid fa-shield-halved"></i> Safety Guard</span>
                    <span class="text-[9px] text-slate-400">Active</span>
                </div>
                <p class="text-[10px] text-slate-400">Time-travel checkpoint ready. Auto git commit enabled.</p>
            </div>
        </aside>

        <!-- Main Workspace Dynamic View Container -->
        <main class="flex-1 flex flex-col justify-between bg-[#0b0f17] overflow-hidden">

            <!-- 1. LIVE AGENT STUDIO VIEW -->
            <section id="view-chat" class="flex-1 flex flex-col justify-between p-6 max-w-5xl mx-auto w-full h-full overflow-hidden">
                <!-- Chat Feed -->
                <div id="messages" class="flex-1 overflow-y-auto space-y-4 pr-3">
                    
                    <!-- Agent Welcome Card -->
                    <div class="glass-panel p-4 rounded-xl border border-slate-800 shadow-sm space-y-2">
                        <div class="flex items-center justify-between">
                            <div class="flex items-center space-x-2">
                                <span class="text-base">🤖</span>
                                <h3 class="text-xs font-bold text-white uppercase tracking-wider">DeskPilot Agent Online</h3>
                            </div>
                            <span class="text-[10px] font-mono px-2 py-0.5 rounded bg-emerald-950 text-emerald-300 border border-emerald-800">Antigravity Standard</span>
                        </div>
                        <p class="text-xs text-slate-300 leading-relaxed">
                            Equipped with <span class="text-sky-300 font-mono">replace_file_content</span> surgical diffing, <span class="text-sky-300 font-mono">rollback_workspace</span>, universal <span class="text-purple-300 font-mono">SKILL.md</span> discovery, and <span class="text-amber-300 font-mono">repo_map</span> context compression.
                        </p>
                    </div>

                    <!-- Example Live Thinking Box -->
                    <div class="border border-amber-900/40 bg-amber-950/20 rounded-xl p-3 text-xs space-y-2">
                        <div class="flex items-center justify-between cursor-pointer" onclick="toggleElement('thinking-body')">
                            <span class="font-semibold text-amber-300 flex items-center gap-2">
                                <i class="fa-solid fa-lightbulb"></i> Reasoning & Planning Stream (DeepSeek-R1 / Claude 3.7)
                            </span>
                            <span class="text-[10px] text-amber-500 font-mono">8 tokens/sec • Click to collapse</span>
                        </div>
                        <div id="thinking-body" class="text-slate-400 font-mono text-[11px] leading-relaxed border-t border-amber-900/30 pt-2">
                            1. Analyzed workspace root: verified Rust tool harness<br>
                            2. Identified candidate files: src/ipc.rs and src/storage.rs<br>
                            3. Selecting surgical replace_file_content over full file overwrite for safety.
                        </div>
                    </div>

                    <!-- Example Tool Activity Card -->
                    <div class="border border-slate-800 bg-slate-900/80 rounded-xl p-3 text-xs space-y-2">
                        <div class="flex items-center justify-between">
                            <span class="font-semibold text-sky-400 flex items-center gap-2">
                                <i class="fa-solid fa-code-compare"></i> Tool: `replace_file_content` (src/storage.rs)
                            </span>
                            <span class="text-[10px] text-emerald-400 font-mono">Success (12ms)</span>
                        </div>
                        <div class="bg-black/60 rounded-lg p-2 font-mono text-[11px] overflow-x-auto text-slate-300">
                            <span class="text-red-400">- old: full overwrite</span><br>
                            <span class="text-emerald-400">+ new: surgical chunk replacement matching exact target context</span>
                        </div>
                    </div>
                </div>

                <!-- Input Action Bar -->
                <div class="pt-3">
                    <div class="flex items-center justify-between px-1 mb-2 text-[11px] text-slate-400">
                        <div class="flex items-center space-x-2">
                            <button onclick="sendQuickCommand('repo_map')" class="px-2 py-0.5 rounded bg-slate-900 border border-slate-800 hover:text-white">🗺️ repo_map</button>
                            <button onclick="sendQuickCommand('rollback_workspace')" class="px-2 py-0.5 rounded bg-slate-900 border border-slate-800 hover:text-amber-400">🔄 rollback</button>
                            <button onclick="sendQuickCommand('git status')" class="px-2 py-0.5 rounded bg-slate-900 border border-slate-800 hover:text-white">🌿 git status</button>
                        </div>
                        <span class="text-[10px] text-slate-400 font-mono">Shift+Enter for newline</span>
                    </div>

                    <div class="flex items-center glass-panel border border-slate-700/80 rounded-xl p-2.5 focus-within:border-sky-500 shadow-xl transition-all">
                        <textarea id="prompt-input" rows="1" placeholder="Instruct DeskPilot or type slash command (/plan, /test, /rollback)..."
                            class="flex-1 bg-transparent px-3 py-1.5 text-sm text-white focus:outline-none placeholder-slate-500 resize-none"
                            onkeydown="if(event.key==='Enter' && !event.shiftKey){ event.preventDefault(); sendMessage(); }"></textarea>
                        <button onclick="sendMessage()" class="bg-gradient-to-r from-emerald-500 to-sky-500 hover:from-emerald-400 hover:to-sky-400 text-slate-950 font-bold text-xs px-5 py-2.5 rounded-lg shadow-md transition-all flex items-center gap-1.5">
                            <span>Execute</span>
                            <i class="fa-solid fa-arrow-up text-[10px]"></i>
                        </button>
                    </div>
                </div>
            </section>

            <!-- 2. TRIGGERS & SCHEDULES AUTOMATION VIEW -->
            <section id="view-triggers" class="hidden flex-1 p-8 max-w-5xl mx-auto w-full overflow-y-auto space-y-6">
                <div class="flex items-center justify-between border-b border-slate-800 pb-4">
                    <div>
                        <h2 class="text-lg font-bold text-white flex items-center gap-2">
                            <i class="fa-solid fa-bolt-lightning text-amber-400"></i> Triggers & Automated Actions
                        </h2>
                        <p class="text-xs text-slate-400">Configure recurring cron schedules, file change watchers, and inbound webhooks.</p>
                    </div>
                    <button onclick="toggleElement('create-trigger-modal')" class="bg-sky-500 hover:bg-sky-400 text-black font-semibold text-xs px-4 py-2 rounded-lg">
                        + New Trigger
                    </button>
                </div>

                <!-- Active Trigger Cards -->
                <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                    <!-- Cron Trigger -->
                    <div class="glass-panel p-4 rounded-xl border border-slate-800 space-y-3">
                        <div class="flex items-center justify-between">
                            <span class="px-2 py-0.5 rounded text-[10px] font-mono bg-blue-950 text-blue-400 border border-blue-800">SCHEDULE / CRON</span>
                            <span class="text-xs text-emerald-400 font-mono">Active</span>
                        </div>
                        <h3 class="text-sm font-semibold text-white">Periodic Health & Test Monitor</h3>
                        <p class="text-xs text-slate-400 font-mono">Interval: `*/30 * * * *` (Every 30 min)</p>
                        <div class="text-xs text-slate-300 bg-slate-900/80 p-2 rounded border border-slate-800">
                            <strong>Action:</strong> Run `cargo check` and report any compiler warnings to chat.
                        </div>
                    </div>

                    <!-- File Watcher Trigger -->
                    <div class="glass-panel p-4 rounded-xl border border-slate-800 space-y-3">
                        <div class="flex items-center justify-between">
                            <span class="px-2 py-0.5 rounded text-[10px] font-mono bg-purple-950 text-purple-400 border border-purple-800">FILE WATCHER</span>
                            <span class="text-xs text-emerald-400 font-mono">Active</span>
                        </div>
                        <h3 class="text-sm font-semibold text-white">Auto Test on Code Save</h3>
                        <p class="text-xs text-slate-400 font-mono">Path: `src/**/*.rs`</p>
                        <div class="text-xs text-slate-300 bg-slate-900/80 p-2 rounded border border-slate-800">
                            <strong>Action:</strong> Run `cargo test` automatically and trigger rollback if regressions occur.
                        </div>
                    </div>

                    <!-- Inbound Webhook Trigger -->
                    <div class="glass-panel p-4 rounded-xl border border-slate-800 space-y-3">
                        <div class="flex items-center justify-between">
                            <span class="px-2 py-0.5 rounded text-[10px] font-mono bg-amber-950 text-amber-400 border border-amber-800">WEBHOOK</span>
                            <span class="text-xs text-emerald-400 font-mono">Ready</span>
                        </div>
                        <h3 class="text-sm font-semibold text-white">GitHub Inbound Webhook</h3>
                        <p class="text-xs text-slate-400 font-mono">POST /api/webhook/github-pr</p>
                        <div class="text-xs text-slate-300 bg-slate-900/80 p-2 rounded border border-slate-800">
                            <strong>Action:</strong> Prompt Claude 3.7 to review PR diff and generate report.
                        </div>
                    </div>
                </div>
            </section>

            <!-- 3. MCP PLUGINS & SKILLS VIEW -->
            <section id="view-mcp" class="hidden flex-1 p-8 max-w-5xl mx-auto w-full overflow-y-auto space-y-6">
                <div class="border-b border-slate-800 pb-4">
                    <h2 class="text-lg font-bold text-white flex items-center gap-2">
                        <i class="fa-solid fa-puzzle-piece text-purple-400"></i> MCP Servers & Claude Skills
                    </h2>
                    <p class="text-xs text-slate-400">Connect to hundreds of Anthropic Model Context Protocol community tools and Claude/Antigravity skills.</p>
                </div>

                <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                    <div class="glass-panel p-4 rounded-xl border border-slate-800 space-y-2">
                        <div class="flex items-center justify-between">
                            <h3 class="text-sm font-semibold text-white">PostgreSQL MCP Server</h3>
                            <span class="text-[10px] px-2 py-0.5 rounded bg-emerald-950 text-emerald-400 border border-emerald-800">Connected</span>
                        </div>
                        <p class="text-xs text-slate-400">Direct SQL schema introspection, read queries, and database health metrics.</p>
                    </div>

                    <div class="glass-panel p-4 rounded-xl border border-slate-800 space-y-2">
                        <div class="flex items-center justify-between">
                            <h3 class="text-sm font-semibold text-white">GitHub MCP Server</h3>
                            <span class="text-[10px] px-2 py-0.5 rounded bg-emerald-950 text-emerald-400 border border-emerald-800">Connected</span>
                        </div>
                        <p class="text-xs text-slate-400">Read PRs, issues, branches, and automate commit pushing.</p>
                    </div>

                    <div class="glass-panel p-4 rounded-xl border border-slate-800 space-y-2">
                        <div class="flex items-center justify-between">
                            <h3 class="text-sm font-semibold text-white">Claude SKILL.md Scanner</h3>
                            <span class="text-[10px] px-2 py-0.5 rounded bg-sky-950 text-sky-400 border border-sky-800">4 Skills Found</span>
                        </div>
                        <p class="text-xs text-slate-400">Scanned .deskpilot/skills, .claude/skills, and .gemini/antigravity.</p>
                    </div>
                </div>
            </section>

            <!-- 4. VAULT & SETTINGS VIEW -->
            <section id="view-settings" class="hidden flex-1 p-8 max-w-4xl mx-auto w-full overflow-y-auto space-y-6">
                <div class="border-b border-slate-800 pb-4">
                    <h2 class="text-lg font-bold text-white flex items-center gap-2">
                        <i class="fa-solid fa-sliders text-sky-400"></i> Engine Vault & API Configuration
                    </h2>
                    <p class="text-xs text-slate-400">Manage OpenRouter, Anthropic, and local Ollama credentials stored securely in local SQLite.</p>
                </div>

                <div class="glass-panel p-6 rounded-xl border border-slate-800 space-y-4">
                    <div>
                        <label class="block text-xs font-semibold text-slate-300 mb-1">Active Execution Engine</label>
                        <select id="setting-provider" class="w-full bg-slate-900 border border-slate-700 rounded-lg p-2.5 text-xs text-white">
                            <option value="OpenRouter">OpenRouter (Claude 3.7, DeepSeek-R1, GPT-4o)</option>
                            <option value="Local Ollama">Local Ollama (100% Offline & Private)</option>
                        </select>
                    </div>

                    <div>
                        <label class="block text-xs font-semibold text-slate-300 mb-1">OpenRouter API Key</label>
                        <input id="setting-or-key" type="password" placeholder="sk-or-v1-..." class="w-full bg-slate-900 border border-slate-700 rounded-lg p-2.5 text-xs text-white" />
                    </div>

                    <div>
                        <label class="block text-xs font-semibold text-slate-300 mb-1">Default Model</label>
                        <input id="setting-or-model" type="text" value="anthropic/claude-3.7-sonnet" class="w-full bg-slate-900 border border-slate-700 rounded-lg p-2.5 text-xs text-white" />
                    </div>

                    <button onclick="saveSettings()" class="bg-sky-500 hover:bg-sky-400 text-black font-bold text-xs px-6 py-2.5 rounded-lg transition-all">
                        Save Configuration
                    </button>
                    <span id="save-status" class="text-xs text-emerald-400 ml-3 hidden font-mono">Saved to SQLite!</span>
                </div>
            </section>
        </main>

        <!-- Right Telemetry & Context Inspector -->
        <aside class="w-72 bg-[#0a0f18] border-l border-slate-800/80 p-4 space-y-5 overflow-y-auto hidden xl:block">
            <!-- Adaptive Memory Card -->
            <div class="space-y-2">
                <div class="flex items-center justify-between">
                    <span class="text-[10px] uppercase font-bold tracking-wider text-slate-400">Adaptive Working Memory</span>
                    <span class="text-[9px] font-mono text-sky-400">O(1)</span>
                </div>
                <div class="bg-slate-900/80 border border-slate-800 p-3 rounded-lg text-xs space-y-1 text-slate-300">
                    <p class="font-semibold text-slate-200">Active Scratchpad:</p>
                    <ul class="list-disc pl-4 text-[11px] text-slate-400 space-y-1">
                        <li>Project rule: Use replace_file_content</li>
                        <li>Database: SQLite WAL mode</li>
                        <li>Git repo clean: branch main</li>
                    </ul>
                </div>
            </div>

            <!-- Open Tasks Card -->
            <div class="space-y-2">
                <span class="text-[10px] uppercase font-bold tracking-wider text-slate-400">Active Tasks</span>
                <div class="space-y-1.5">
                    <div class="flex items-center space-x-2 text-xs text-slate-300 bg-slate-900/60 p-2 rounded border border-slate-800/60">
                        <input type="checkbox" checked class="accent-emerald-500" />
                        <span class="line-through text-slate-400">Upgrade Web Studio UI</span>
                    </div>
                    <div class="flex items-center space-x-2 text-xs text-slate-300 bg-slate-900/60 p-2 rounded border border-slate-800/60">
                        <input type="checkbox" checked class="accent-emerald-500" />
                        <span class="line-through text-slate-400">Universal MCP & Skills</span>
                    </div>
                    <div class="flex items-center space-x-2 text-xs text-slate-300 bg-slate-900/60 p-2 rounded border border-slate-800/60">
                        <input type="checkbox" class="accent-emerald-500" />
                        <span>Triggers & Automation Engine</span>
                    </div>
                </div>
            </div>
        </aside>
    </div>

    <!-- UI Logic Scripts -->
    <script>
        function switchView(viewName) {
            ['chat', 'triggers', 'mcp', 'settings'].forEach(v => {
                const el = document.getElementById('view-' + v);
                const nav = document.getElementById('nav-' + v);
                if (el) el.classList.toggle('hidden', v !== viewName);
                if (nav) nav.classList.toggle('active-tab', v === viewName);
            });
        }

        function toggleElement(id) {
            const el = document.getElementById(id);
            if (el) el.classList.toggle('hidden');
        }

        function sendQuickCommand(cmd) {
            document.getElementById('prompt-input').value = cmd;
            sendMessage();
        }

        async function sendMessage() {
            const input = document.getElementById('prompt-input');
            const text = input.value.trim();
            if (!text) return;
            
            const messages = document.getElementById('messages');
            const userMsg = document.createElement('div');
            userMsg.className = 'p-3 rounded-xl bg-sky-950/40 border border-sky-800/60 text-xs ml-auto max-w-xl text-sky-200 shadow-sm';
            userMsg.innerText = text;
            messages.appendChild(userMsg);
            input.value = '';
            messages.scrollTop = messages.scrollHeight;

            try {
                const res = await fetch('/api/message', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ content: text })
                });
                const reply = document.createElement('div');
                reply.className = 'p-3 rounded-xl bg-slate-900 border border-slate-800 text-xs max-w-xl text-emerald-400 font-mono shadow-sm flex items-center gap-2';
                reply.innerHTML = '<span class="animate-spin text-sm">⚙</span> <span>Dispatched to DeskPilot daemon...</span>';
                messages.appendChild(reply);
                messages.scrollTop = messages.scrollHeight;
            } catch (err) {
                console.error(err);
            }
        }

        async function saveSettings() {
            const or_key = document.getElementById('setting-or-key').value;
            const or_model = document.getElementById('setting-or-model').value;
            const provider = document.getElementById('setting-provider').value;
            try {
                await fetch('/api/settings', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ openrouter_key: or_key, openrouter_model: or_model, active_provider: provider })
                });
                const status = document.getElementById('save-status');
                status.classList.remove('hidden');
                setTimeout(() => status.classList.add('hidden'), 2500);
            } catch (err) {
                console.error(err);
            }
        }
    </script>
</body>
</html>"#;

pub async fn start_server(tx: mpsc::UnboundedSender<String>) {
    let storage = Arc::new(Storage::new());
    let state = AppState { tx, storage };

    let app = Router::new()
        .route("/", get(|| async { Html(WEB_UI_HTML) }))
        .route("/api/message", post(handle_message))
        .route("/api/triggers/:project_id", get(get_triggers).post(create_trigger))
        .route("/api/settings", post(save_settings))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 31415));
    if let Ok(listener) = tokio::net::TcpListener::bind(addr).await {
        println!("DeskPilot Web Studio available at: http://{}", addr);
        let _ = axum::serve(listener, app).await;
    } else {
        eprintln!("Failed to bind server to {}", addr);
    }
}

async fn handle_message(
    State(state): State<AppState>,
    Json(payload): Json<IpcMessage>,
) -> &'static str {
    let _ = state.tx.send(payload.content);
    "Message dispatched to DeskPilot daemon"
}

async fn get_triggers(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
) -> Json<Vec<crate::storage::TriggerItem>> {
    let list = state.storage.get_triggers(project_id).unwrap_or_default();
    Json(list)
}

async fn create_trigger(
    State(state): State<AppState>,
    Json(payload): Json<CreateTriggerReq>,
) -> &'static str {
    let _ = state.storage.add_trigger(
        payload.project_id,
        &payload.name,
        &payload.trigger_type,
        &payload.schedule_expr,
        &payload.action_type,
        &payload.action_payload,
    );
    "Trigger created"
}

async fn save_settings(
    State(state): State<AppState>,
    Json(payload): Json<SaveSettingsReq>,
) -> &'static str {
    if let Some(key) = payload.openrouter_key {
        let _ = state.storage.set_setting("openrouter_key", &key);
    }
    if let Some(model) = payload.openrouter_model {
        let _ = state.storage.set_setting("openrouter_model", &model);
    }
    if let Some(provider) = payload.active_provider {
        let _ = state.storage.set_setting("active_provider", &provider);
    }
    "Settings saved"
}
