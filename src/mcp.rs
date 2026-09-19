use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

/// MCP Server configuration (similar to Claude Desktop & Antigravity mcp_servers.json format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfigFile {
    #[serde(default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// Discovered MCP Tool metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub server_name: String,
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// Dynamic MCP Client Manager that communicates with MCP servers over stdio JSON-RPC
pub struct McpManager {
    tools: Arc<Mutex<Vec<McpToolInfo>>>,
    servers: Arc<Mutex<HashMap<String, McpServerConfig>>>,
}

impl McpManager {
    pub fn new() -> Self {
        Self {
            tools: Arc::new(Mutex::new(Vec::new())),
            servers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Discover and register MCP servers from the workspace or user configuration.
    pub async fn load_configs(&self, workspace: &Path) {
        let mut configs = Vec::new();

        // 1. Workspace root `mcp_servers.json`
        let ws_config = workspace.join("mcp_servers.json");
        if ws_config.exists() {
            configs.push(ws_config);
        }

        // 2. Workspace `.deskpilot/mcp_servers.json`
        let ws_dp = workspace.join(".deskpilot").join("mcp_servers.json");
        if ws_dp.exists() {
            configs.push(ws_dp);
        }

        // 3. Global user configuration `~/.deskpilot/mcp_servers.json`
        if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
            let global_dp = home.join(".deskpilot").join("mcp_servers.json");
            if global_dp.exists() {
                configs.push(global_dp);
            }
        }

        let mut all_servers = HashMap::new();
        for config_path in configs {
            if let Ok(content) = tokio::fs::read_to_string(&config_path).await {
                if let Ok(parsed) = serde_json::from_str::<McpConfigFile>(&content) {
                    for (k, v) in parsed.mcp_servers {
                        all_servers.insert(k, v);
                    }
                }
            }
        }

        let mut s_lock = self.servers.lock().await;
        *s_lock = all_servers.clone();
        drop(s_lock);

        // Fetch tool lists from all configured servers
        let mut all_tools = Vec::new();
        for (name, cfg) in all_servers {
            if let Ok(tools) = Self::query_tools(&name, &cfg, workspace).await {
                all_tools.extend(tools);
            }
        }

        let mut t_lock = self.tools.lock().await;
        *t_lock = all_tools;
    }

    /// Query an MCP server for its tool catalog via `tools/list`
    async fn query_tools(server_name: &str, cfg: &McpServerConfig, workspace: &Path) -> Result<Vec<McpToolInfo>> {
        let mut cmd = Command::new(&cfg.command);
        cmd.args(&cfg.args);
        cmd.current_dir(workspace);
        for (k, v) in &cfg.env {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::null());

        let mut child = cmd.spawn().context("failed to spawn MCP server")?;
        let mut stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
        let mut reader = BufReader::new(stdout).lines();

        // 1. Send initialize request
        let init_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "clientInfo": {
                    "name": "deskpilot",
                    "version": "0.1.0"
                },
                "capabilities": {}
            }
        });
        stdin.write_all(format!("{}\n", init_req).as_bytes()).await?;
        stdin.flush().await?;

        // Wait for initialize response (with timeout)
        let _ = tokio::time::timeout(std::time::Duration::from_secs(4), reader.next_line()).await;

        // 2. Send initialized notification
        let initialized_notif = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        stdin.write_all(format!("{}\n", initialized_notif).as_bytes()).await?;
        stdin.flush().await?;

        // 3. Send tools/list request
        let list_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        });
        stdin.write_all(format!("{}\n", list_req).as_bytes()).await?;
        stdin.flush().await?;

        let mut tools = Vec::new();
        if let Ok(Ok(Some(line))) = tokio::time::timeout(std::time::Duration::from_secs(4), reader.next_line()).await {
            if let Ok(res) = serde_json::from_str::<Value>(&line) {
                if let Some(arr) = res.get("result").and_then(|r| r.get("tools")).and_then(|t| t.as_array()) {
                    for t in arr {
                        let name = t.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                        let desc = t.get("description").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                        let schema = t.get("inputSchema").cloned().unwrap_or_else(|| serde_json::json!({"type":"object"}));
                        if !name.is_empty() {
                            tools.push(McpToolInfo {
                                server_name: server_name.to_string(),
                                name,
                                description: desc,
                                input_schema: schema,
                            });
                        }
                    }
                }
            }
        }

        let _ = child.kill().await;
        Ok(tools)
    }

    /// Execute a tool call on an MCP server
    pub async fn execute_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: &Value,
        workspace: &Path,
    ) -> Result<String> {
        let servers = self.servers.lock().await;
        let cfg = servers
            .get(server_name)
            .ok_or_else(|| anyhow!("MCP server '{}' not configured", server_name))?
            .clone();
        drop(servers);

        let mut cmd = Command::new(&cfg.command);
        cmd.args(&cfg.args);
        cmd.current_dir(workspace);
        for (k, v) in &cfg.env {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::null());

        let mut child = cmd.spawn().context("failed to spawn MCP server")?;
        let mut stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
        let mut reader = BufReader::new(stdout).lines();

        // 1. Initialize
        let init_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "clientInfo": {
                    "name": "deskpilot",
                    "version": "0.1.0"
                },
                "capabilities": {}
            }
        });
        stdin.write_all(format!("{}\n", init_req).as_bytes()).await?;
        stdin.flush().await?;
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), reader.next_line()).await;

        let initialized_notif = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        stdin.write_all(format!("{}\n", initialized_notif).as_bytes()).await?;
        stdin.flush().await?;

        // 2. Call tool
        let call_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments
            }
        });
        stdin.write_all(format!("{}\n", call_req).as_bytes()).await?;
        stdin.flush().await?;

        let output = if let Ok(Ok(Some(line))) = tokio::time::timeout(std::time::Duration::from_secs(30), reader.next_line()).await {
            if let Ok(res) = serde_json::from_str::<Value>(&line) {
                if let Some(content) = res.get("result").and_then(|r| r.get("content")).and_then(|c| c.as_array()) {
                    let mut parts = Vec::new();
                    for item in content {
                        if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                            parts.push(text.to_string());
                        }
                    }
                    parts.join("\n")
                } else if let Some(err) = res.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
                    format!("MCP Error: {}", err)
                } else {
                    line
                }
            } else {
                line
            }
        } else {
            "MCP request timed out".to_string()
        };

        let _ = child.kill().await;
        Ok(output)
    }

    /// Get current list of discovered MCP tools
    pub async fn get_tools(&self) -> Vec<McpToolInfo> {
        self.tools.lock().await.clone()
    }
}
