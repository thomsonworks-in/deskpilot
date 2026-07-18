use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    #[serde(default)]
    pub conversation_id: u64,
    pub role: Role,
    pub content: String,
    #[serde(default)]
    pub thinking: String,
    #[serde(default)]
    pub tool_uses: Vec<(String, String, bool)>,
    #[serde(default, skip_serializing)]
    pub created_at: i64,
}

impl Message {
    pub fn new(conversation_id: u64, role: Role, content: impl Into<String>) -> Self {
        Self {
            conversation_id,
            role,
            content: content.into(),
            thinking: String::new(),
            tool_uses: Vec::new(),
            created_at: chrono::Utc::now().timestamp(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct OllamaTagsResponse {
    pub models: Vec<OllamaModelInfo>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OllamaModelInfo {
    pub name: String,
}

#[derive(Serialize)]
pub(crate) struct ChatRequest<'a> {
    pub model: &'a str,
    pub messages: &'a [Message],
    pub stream: bool,
    pub think: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<ChatOptions>,
}

#[derive(Clone, Copy, Serialize)]
pub(crate) struct ChatOptions {
    pub num_gpu: u32,
    pub num_ctx: u32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChatChunk {
    pub message: Option<ChatResponseMessage>,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChatResponseMessage {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub thinking: String,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ToolCall {
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ToolCallFunction {
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

#[derive(Serialize)]
pub(crate) struct EmbedRequest<'a> {
    pub model: &'a str,
    pub input: &'a str,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EmbedResponse {
    pub embeddings: Vec<Vec<f32>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RunningModelsResponse {
    pub models: Vec<RunningModel>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RunningModel {
    pub name: String,
}

impl ChatChunk {
    pub fn from_line(line: &str) -> Result<Option<Self>, String> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(None);
        }
        serde_json::from_str(line)
            .map(Some)
            .map_err(|error| format!("malformed Ollama JSON chunk: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::ChatChunk;

    #[test]
    fn parses_content_chunk() {
        let chunk = ChatChunk::from_line(
            r#"{"message":{"role":"assistant","content":"hello"},"done":false}"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(chunk.message.unwrap().content, "hello");
        assert!(!chunk.done);
    }

    #[test]
    fn parses_thinking_chunk() {
        let chunk = ChatChunk::from_line(
            r#"{"message":{"role":"assistant","content":"","thinking":"checking"},"done":false}"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(chunk.message.unwrap().thinking, "checking");
    }

    #[test]
    fn rejects_malformed_non_empty_line() {
        assert!(ChatChunk::from_line("not json").is_err());
    }
}
