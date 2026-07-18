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
    kind: &'static str,
    function: ToolFunction,
}

#[derive(Debug, Clone, Serialize)]
struct ToolFunction {
    name: &'static str,
    description: &'static str,
    parameters: serde_json::Value,
}

pub fn discover_skills(workspace: &Path) -> Vec<Skill> {
    let mut roots = vec![workspace.join(".claude").join("skills")];
    if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        roots.push(home.join(".claude").join("skills"));
        roots.push(home.join(".codex").join("skills"));
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

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        definition("read_file", "Read a UTF-8 text file inside the workspace.", serde_json::json!({"type":"object","required":["path"],"properties":{"path":{"type":"string"}}})),
        definition("list_files", "List files inside a workspace directory.", serde_json::json!({"type":"object","properties":{"path":{"type":"string"}}})),
        definition("read_skill", "Load the full instructions for an available SKILL.md by skill name.", serde_json::json!({"type":"object","required":["name"],"properties":{"name":{"type":"string"}}})),
        definition("curl", "Make an HTTP GET request and return the response body. Use only when the user requests internet access.", serde_json::json!({"type":"object","required":["url"],"properties":{"url":{"type":"string"}}})),
        definition("powershell", "Run a non-destructive PowerShell command in the workspace.", serde_json::json!({"type":"object","required":["command"],"properties":{"command":{"type":"string"}}})),
        definition("bash", "Run a non-destructive bash command in the workspace when bash is installed.", serde_json::json!({"type":"object","required":["command"],"properties":{"command":{"type":"string"}}})),
        definition("write_file", "Write text content to a file in the workspace, overwriting it. Path must be relative to workspace.", serde_json::json!({"type":"object","required":["path", "content"],"properties":{"path":{"type":"string"},"content":{"type":"string"}}})),
        definition("git_commit", "Commit all current changes to git with a message.", serde_json::json!({"type":"object","required":["message"],"properties":{"message":{"type":"string"}}})),
        definition("self_update", "Rebuild and restart the DeskPilot application.", serde_json::json!({"type":"object","properties":{}})),
    ]
}

fn definition(
    name: &'static str,
    description: &'static str,
    parameters: serde_json::Value,
) -> ToolDefinition {
    ToolDefinition {
        kind: "function",
        function: ToolFunction {
            name,
            description,
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
    match name {
        "read_file" => {
            let path = safe_path(workspace, required(arguments, "path")?)?;
            let text = tokio::fs::read_to_string(path)
                .await
                .context("could not read file")?;
            Ok(limit(text, 32_000))
        }
        "list_files" => {
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
            let requested = required(arguments, "name")?;
            let skill = skills
                .iter()
                .find(|skill| skill.name.eq_ignore_ascii_case(requested))
                .ok_or_else(|| anyhow!("unknown skill: {requested}"))?;
            Ok(limit(tokio::fs::read_to_string(&skill.path).await?, 48_000))
        }
        "curl" => {
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
            run_shell(
                "powershell.exe",
                &["-NoProfile", "-NonInteractive", "-Command"],
                required(arguments, "command")?,
                workspace,
            )
            .await
        }
        "bash" => run_shell("bash", &["-lc"], required(arguments, "command")?, workspace).await,
        "write_file" => {
            let requested = required(arguments, "path")?;
            let content = required(arguments, "content")?;
            let path = safe_new_path(workspace, requested)?;
            tokio::fs::write(&path, content).await.context("failed to write file")?;
            Ok(format!("file {} written successfully", path.display()))
        }
        "git_commit" => {
            let message = required(arguments, "message")?;
            run_shell("git", &[], "add .", workspace).await?;
            run_shell("git", &["commit", "-m"], message, workspace).await
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
