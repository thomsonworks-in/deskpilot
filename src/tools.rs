use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use tokio::process::Command;



#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub kind: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

pub fn discover_skills(workspace: &Path) -> Vec<Skill> {
    let mut roots = vec![
        workspace.join(".deskpilot").join("skills"),
        workspace.join(".claude").join("skills"),
        workspace.join(".gemini").join("skills"),
        workspace.join(".antigravity").join("skills"),
    ];
    if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        roots.push(home.join(".deskpilot").join("skills"));
        roots.push(home.join(".claude").join("skills"));
        roots.push(home.join(".codex").join("skills"));
        roots.push(home.join(".gemini").join("antigravity").join("skills"));
    }
    let mut skills = Vec::new();
    for root in roots {
        let mut directories = vec![(root, 0_u8)];
        while let Some((directory, depth)) = directories.pop() {
            let Ok(entries) = std::fs::read_dir(directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_dir() && depth < 3 {
                    directories.push((entry_path.clone(), depth + 1));
                }
                let path = entry_path.join("SKILL.md");
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let name = frontmatter(&text, "name")
                    .unwrap_or_else(|| entry.file_name().to_string_lossy().into_owned());
                let description = frontmatter(&text, "description")
                    .unwrap_or_else(|| "Local agent skill".to_owned());
                skills.push(Skill {
                    name,
                    description,
                    path,
                });
            }
        }
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills.dedup_by(|a, b| a.name == b.name);
    skills
}

fn frontmatter(text: &str, key: &str) -> Option<String> {
    let body = text.strip_prefix("---")?.split_once("---")?.0;
    body.lines().find_map(|line| {
        let (found, value) = line.split_once(':')?;
        (found.trim() == key).then(|| value.trim().trim_matches(['\'', '"']).to_owned())
    })
}

use crate::storage::ProjectPermissions;

pub fn definitions() -> Vec<ToolDefinition> {
    definitions_for_permissions(&ProjectPermissions::preset("full"))
}

pub fn definitions_for_permissions(perms: &ProjectPermissions) -> Vec<ToolDefinition> {
    let mut list = Vec::new();

    if perms.read_files {
        list.push(definition("read_file", "Read a UTF-8 text file inside the workspace.", serde_json::json!({"type":"object","required":["path"],"properties":{"path":{"type":"string"}}})));
        list.push(definition("list_files", "List files inside a workspace directory.", serde_json::json!({"type":"object","properties":{"path":{"type":"string"}}})));
        list.push(definition("read_skill", "Load the full instructions for an available SKILL.md by skill name.", serde_json::json!({"type":"object","required":["name"],"properties":{"name":{"type":"string"}}})));
        list.push(definition("repo_map", "Generate a condensed structural summary of key code files, definitions, and workspace tree in under 2000 tokens.", serde_json::json!({"type":"object","properties":{}})));
    }

    if perms.web_search {
        list.push(definition("web_search", "Search the live public internet using a search query and return top results.", serde_json::json!({"type":"object","required":["query"],"properties":{"query":{"type":"string"}}})));
        list.push(definition("curl", "Make an HTTP GET request and return the response body. Use only when the user requests internet access.", serde_json::json!({"type":"object","required":["url"],"properties":{"url":{"type":"string"}}})));
    }

    if perms.write_files {
        list.push(definition("write_file", "Write text content to a file in the workspace, overwriting it. Path must be relative to workspace.", serde_json::json!({"type":"object","required":["path", "content"],"properties":{"path":{"type":"string"},"content":{"type":"string"}}})));
        list.push(definition("replace_file_content", "Surgically edit an existing file by finding exact target_content and replacing it with replacement_content. Avoids full file overwrites.", serde_json::json!({"type":"object","required":["path", "target_content", "replacement_content"],"properties":{"path":{"type":"string"},"target_content":{"type":"string"},"replacement_content":{"type":"string"}}})));
    }

    if perms.terminal_exec {
        list.push(definition("powershell", "Run a non-destructive PowerShell command in the workspace.", serde_json::json!({"type":"object","required":["command"],"properties":{"command":{"type":"string"}}})));
        list.push(definition("bash", "Run a non-destructive bash command in the workspace when bash is installed.", serde_json::json!({"type":"object","required":["command"],"properties":{"command":{"type":"string"}}})));
    }

    if perms.git_ops {
        list.push(definition("rollback_workspace", "Rollback files to the last git checkpoint before agent modifications.", serde_json::json!({"type":"object","properties":{}})));
        list.push(definition("git_commit", "Commit all current changes to git with a message.", serde_json::json!({"type":"object","required":["message"],"properties":{"message":{"type":"string"}}})));
    }

    if perms.subagents {
        list.push(definition("delegate_subagent", "Delegate a focused task (like extensive research, code review, or file analysis) to a background subagent.", serde_json::json!({"type":"object","required":["role","task"],"properties":{"role":{"type":"string","description":"Role title (e.g. 'Code Researcher', 'Documentation Reviewer')"},"task":{"type":"string","description":"Clear actionable prompt for subagent to execute"}}})));
    }

    list
}

pub fn definition(
    name: &str,
    description: &str,
    parameters: serde_json::Value,
) -> ToolDefinition {
    ToolDefinition {
        kind: "function".to_string(),
        function: ToolFunction {
            name: name.to_string(),
            description: description.to_string(),
            parameters,
        },
    }
}

pub async fn execute(
    name: &str,
    arguments: &serde_json::Value,
    workspace: &Path,
    skills: &[Skill],
) -> Result<String> {
    execute_with_permissions(name, arguments, workspace, skills, &ProjectPermissions::preset("full")).await
}

pub async fn execute_with_permissions(
    name: &str,
    arguments: &serde_json::Value,
    workspace: &Path,
    skills: &[Skill],
    perms: &ProjectPermissions,
) -> Result<String> {
    match name {
        "read_file" => {
            if !perms.read_files {
                return Err(anyhow!("Permission denied: read_files is disabled for this workspace"));
            }
            let path = safe_path(workspace, required(arguments, "path")?)?;
            let text = tokio::fs::read_to_string(path)
                .await
                .context("could not read file")?;
            Ok(limit(text, 32_000))
        }
        "list_files" => {
            if !perms.read_files {
                return Err(anyhow!("Permission denied: read_files is disabled for this workspace"));
            }
            let path = safe_path(
                workspace,
                arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("."),
            )?;
            let mut entries = tokio::fs::read_dir(path)
                .await
                .context("could not list directory")?;
            let mut output = Vec::new();
            while let Some(entry) = entries.next_entry().await? {
                output.push(entry.file_name().to_string_lossy().into_owned());
                if output.len() >= 500 {
                    break;
                }
            }
            Ok(output.join("\n"))
        }

        "read_skill" => {
            if !perms.read_files {
                return Err(anyhow!("Permission denied: read_files is disabled for this workspace"));
            }
            let requested = required(arguments, "name")?;
            let skill = skills
                .iter()
                .find(|skill| skill.name.eq_ignore_ascii_case(requested))
                .ok_or_else(|| anyhow!("unknown skill: {requested}"))?;
            Ok(limit(tokio::fs::read_to_string(&skill.path).await?, 48_000))
        }
        "web_search" => {
            if !perms.web_search {
                return Err(anyhow!("Permission denied: web_search is disabled for this workspace"));
            }
            let query = required(arguments, "query")?;
            let url = format!("https://html.duckduckgo.com/html/?q={}", urlencoding_encode(query));
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
                .build()?;
            let response = client.get(&url).send().await?;
            let html = response.text().await?;
            let parsed = strip_search_html(&html);
            Ok(limit(parsed, 16_000))
        }
        "curl" => {
            if !perms.web_search {
                return Err(anyhow!("Permission denied: web_search / internet access is disabled for this workspace"));
            }
            let url = required(arguments, "url")?;
            if !url.starts_with("https://") && !url.starts_with("http://") {
                return Err(anyhow!("only HTTP(S) URLs are allowed"));
            }
            let response = reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()?
                .get(url)
                .send()
                .await?
                .error_for_status()?;
            Ok(limit(response.text().await?, 48_000))
        }
        "powershell" => {
            if !perms.terminal_exec {
                return Err(anyhow!("Permission denied: terminal_exec is disabled for this workspace"));
            }
            run_shell(
                "powershell.exe",
                &["-NoProfile", "-NonInteractive", "-Command"],
                required(arguments, "command")?,
                workspace,
            )
            .await
        }
        "bash" => {
            if !perms.terminal_exec {
                return Err(anyhow!("Permission denied: terminal_exec is disabled for this workspace"));
            }
            run_shell("bash", &["-lc"], required(arguments, "command")?, workspace).await
        }
        "write_file" => {
            if !perms.write_files {
                return Err(anyhow!("Permission denied: write_files is disabled for this workspace"));
            }
            let requested = required(arguments, "path")?;
            let content = required(arguments, "content")?;
            let path = safe_new_path(workspace, requested)?;
            tokio::fs::write(&path, content).await.context("failed to write file")?;
            Ok(format!("file {} written successfully", path.display()))
        }
        "replace_file_content" => {
            if !perms.write_files {
                return Err(anyhow!("Permission denied: write_files is disabled for this workspace"));
            }
            let requested = required(arguments, "path")?;
            let target = required(arguments, "target_content")?;
            let replacement = required(arguments, "replacement_content")?;
            let path = safe_path(workspace, requested)?;
            let current = tokio::fs::read_to_string(&path).await.context("failed to read file for diff edit")?;
            if !current.contains(target) {
                return Err(anyhow!("target_content not found in {}", path.display()));
            }
            let count = current.matches(target).count();
            if count > 1 {
                return Err(anyhow!("target_content occurs {count} times in {}; specify more surrounding context lines for unique match", path.display()));
            }
            let updated = current.replacen(target, replacement, 1);
            tokio::fs::write(&path, updated).await.context("failed to write updated file")?;
            Ok(format!("successfully replaced chunk in {}", path.display()))
        }
        "rollback_workspace" => {
            if !perms.git_ops {
                return Err(anyhow!("Permission denied: git_ops is disabled for this workspace"));
            }
            run_shell("git", &["checkout", "--", "."], "", workspace).await?;
            run_shell("git", &["clean", "-fd"], "", workspace).await?;
            Ok("workspace successfully rolled back to clean git checkpoint".to_owned())
        }
        "repo_map" => {
            if !perms.read_files {
                return Err(anyhow!("Permission denied: read_files is disabled for this workspace"));
            }
            let mut summary = Vec::new();
            let mut stack = vec![(workspace.to_path_buf(), 0_usize)];
            while let Some((dir, depth)) = stack.pop() {
                if depth > 3 || summary.len() > 60 { break; }
                if let Ok(mut entries) = tokio::fs::read_dir(&dir).await {
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        let path = entry.path();
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if name.starts_with('.') || name == "target" || name == "node_modules" { continue; }
                        if path.is_dir() {
                            stack.push((path, depth + 1));
                        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                            if ["rs", "toml", "json", "py", "ts", "js", "md"].contains(&ext) {
                                let rel = path.strip_prefix(workspace).unwrap_or(&path).display().to_string();
                                summary.push(format!("- {rel}"));
                            }
                        }
                    }
                }
            }
            Ok(format!("WORKSPACE STRUCTURE (Top files):\n{}", summary.join("\n")))
        }
        "git_commit" => {
            if !perms.git_ops {
                return Err(anyhow!("Permission denied: git_ops is disabled for this workspace"));
            }
            let message = required(arguments, "message")?;
            run_shell("git", &[], "add .", workspace).await?;
            run_shell("git", &["commit", "-m"], message, workspace).await
        }

        "delegate_subagent" => {
            if !perms.subagents {
                return Err(anyhow!("Permission denied: subagents are disabled for this workspace"));
            }
            let role = required(arguments, "role")?;
            let task = required(arguments, "task")?;
            // Local fast execution via local Ollama or lightweight agent
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(45))
                .build()?;
            let prompt = format!("You are a subagent with role '{}'. Your task: {}\nProvide a direct, concise report of your findings.", role, task);
            let req_body = serde_json::json!({
                "model": "ornith:9b",
                "messages": [{"role": "user", "content": prompt}],
                "stream": false
            });
            match client.post("http://127.0.0.1:11434/api/chat").json(&req_body).send().await {
                Ok(resp) => {
                    if let Ok(body) = resp.json::<serde_json::Value>().await {
                        let content = body.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()).unwrap_or("Subagent completed task with no output.");
                        Ok(format!("[Subagent '{}' Output]:\n{}", role, content))
                    } else {
                        Ok(format!("[Subagent '{}' Output]: Executed task successfully.", role))
                    }
                }
                Err(err) => Ok(format!("[Subagent '{}']: Offline fallback note (Ollama offline: {}). Task recorded: {}", role, err, task))
            }
        }
        "self_update" => {
            let script_path = workspace.join("update.bat");
            tokio::fs::write(&script_path, "@echo off\ntimeout /t 2 /nobreak >nul\ncargo run").await?;
            Command::new("cmd").args(["/C", "start", "", script_path.to_str().unwrap()]).current_dir(workspace).spawn()?;
            Ok("Restarting app...".to_owned())
        }
        _ => Err(anyhow!("unknown tool: {name}")),
    }
}

async fn run_shell(
    program: &str,
    prefix: &[&str],
    command: &str,
    workspace: &Path,
) -> Result<String> {
    let lower = command.to_ascii_lowercase();
    for blocked in [
        "remove-item",
        " rm ",
        "rm -",
        "del /",
        "format ",
        "shutdown",
        "stop-process",
        "git reset --hard",
    ] {
        if lower.contains(blocked) {
            return Err(anyhow!(
                "destructive command rejected by DeskPilot safety policy"
            ));
        }
    }
    let mut process = Command::new(program);
    process
        .args(prefix)
        .arg(command)
        .current_dir(workspace)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(45), process.output())
        .await
        .context("command timed out")??;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(format!(
        "exit code: {}\n{}",
        output.status.code().unwrap_or(-1),
        limit(combined, 32_000)
    ))
}

fn required<'a>(args: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing string argument: {key}"))
}

fn safe_path(workspace: &Path, requested: &str) -> Result<PathBuf> {
    let root = workspace.canonicalize()?;
    let candidate = if Path::new(requested).is_absolute() {
        PathBuf::from(requested)
    } else {
        root.join(requested)
    };
    let canonical = candidate.canonicalize().context("path does not exist")?;
    if !canonical.starts_with(&root) {
        return Err(anyhow!("path is outside the workspace"));
    }
    Ok(canonical)
}

fn safe_new_path(workspace: &Path, requested: &str) -> Result<PathBuf> {
    let root = workspace.canonicalize()?;
    let candidate = if Path::new(requested).is_absolute() {
        PathBuf::from(requested)
    } else {
        root.join(requested)
    };
    let parent = candidate.parent().unwrap_or(Path::new(""));
    let canonical_parent = parent.canonicalize().context("parent directory does not exist")?;
    if !canonical_parent.starts_with(&root) {
        return Err(anyhow!("path is outside the workspace"));
    }
    Ok(candidate)
}

fn limit(mut text: String, max: usize) -> String {
    if text.len() > max {
        text.truncate(max);
        text.push_str("\n[output truncated]");
    }
    text
}

fn urlencoding_encode(s: &str) -> String {
    let mut result = String::new();
    for byte in s.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            b' ' => result.push('+'),
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
}

fn strip_search_html(html: &str) -> String {
    let mut results = Vec::new();
    let mut current = String::new();
    let mut in_tag = false;

    for part in html.split("<a class=\"result__snippet\"") {
        if let Some(snippet_start) = part.split(">").nth(1) {
            if let Some(snippet_body) = snippet_start.split("</a>").next() {
                let mut clean = String::new();
                for c in snippet_body.chars() {
                    if c == '<' { in_tag = true; }
                    else if c == '>' { in_tag = false; }
                    else if !in_tag { clean.push(c); }
                }
                let clean = clean.trim();
                if !clean.is_empty() && clean.len() > 15 {
                    results.push(format!("• {}", clean));
                    if results.len() >= 8 { break; }
                }
            }
        }
    }

    if results.is_empty() {
        // Fallback simple tag stripper
        for c in html.chars() {
            if c == '<' { in_tag = true; }
            else if c == '>' { in_tag = false; }
            else if !in_tag { current.push(c); }
        }
        let lines: Vec<&str> = current.lines().map(|l| l.trim()).filter(|l| l.len() > 20).take(10).collect();
        lines.join("\n")
    } else {
        results.join("\n\n")
    }
}
