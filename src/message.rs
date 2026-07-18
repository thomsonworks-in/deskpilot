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
    pub role: Role,
    pub content: String,
    #[serde(default, skip_serializing)]
    pub created_at: i64,
}

impl Message {
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
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
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChatChunk {
    pub message: Option<ChatResponseMessage>,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChatResponseMessage {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub thinking: String,
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
