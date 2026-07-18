use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use reqwest::Client;
use tokio::sync::{mpsc, oneshot};

use crate::message::{
    ChatChunk, ChatRequest, EmbedRequest, EmbedResponse, Message, OllamaTagsResponse,
    RunningModelsResponse,
};

#[derive(Debug)]
pub enum StreamEvent {
    Started,
    ContentDelta(String),
    ThinkingDelta(String),
    ModelLoading(String),
    Finished,
    Cancelled,
    Error(String),
    ModelsLoaded(Vec<String>),
    EmbeddingReady { memory_id: u64, embedding: Vec<f32> },
    Notice(String),
}

#[derive(Clone)]
pub struct OllamaClient {
    base_url: String,
    http: Client,
}

impl OllamaClient {
    pub fn new(base_url: &str) -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("failed to build HTTP client");
        Self {
            base_url: base_url.into(),
            http,
        }
    }

    pub async fn list_models(&self) -> Result<Vec<String>> {
        let response = self
            .http
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .context("could not connect to Ollama")?;
        let response = response
            .error_for_status()
            .context("Ollama model request failed")?;
        let body: OllamaTagsResponse = response
            .json()
            .await
            .context("invalid Ollama model response")?;
        Ok(body.models.into_iter().map(|model| model.name).collect())
    }

    pub async fn embed(&self, model: &str, input: &str) -> Result<Vec<f32>> {
        let response = self
            .http
            .post(format!("{}/api/embed", self.base_url))
            .json(&EmbedRequest { model, input })
            .send()
            .await
            .context("could not request an embedding")?
            .error_for_status()
            .context("Ollama embedding request failed")?;
        let body: EmbedResponse = response
            .json()
            .await
            .context("invalid Ollama embedding response")?;
        body.embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("Ollama returned no embedding"))
    }

    pub async fn is_model_loaded(&self, model: &str) -> Result<bool> {
        let response = self
            .http
            .get(format!("{}/api/ps", self.base_url))
            .send()
            .await
            .context("could not query loaded Ollama models")?
            .error_for_status()
            .context("Ollama loaded-model request failed")?;
        let body: RunningModelsResponse = response
            .json()
            .await
            .context("invalid Ollama loaded-model response")?;
        Ok(body
            .models
            .iter()
            .any(|running| running.name == model || running.name.starts_with(&format!("{model}:"))))
    }

    pub async fn stream_chat(
        &self,
        model: &str,
        messages: &[Message],
        events: &mpsc::UnboundedSender<StreamEvent>,
        mut cancel: oneshot::Receiver<()>,
    ) -> Result<()> {
        let request = ChatRequest {
            model,
            messages,
            stream: true,
            think: true,
        };
        let response = self
            .http
            .post(format!("{}/api/chat", self.base_url))
            .json(&request)
            .send()
            .await
            .context("could not start Ollama chat")?;
        let response = response
            .error_for_status()
            .context("Ollama chat request failed")?;
        let _ = events.send(StreamEvent::Started);

        let mut bytes = response.bytes_stream();
        let mut buffer = Vec::<u8>::new();
        let mut received_content = false;

        loop {
            tokio::select! {
                _ = &mut cancel => {
                    let _ = events.send(StreamEvent::Cancelled);
                    return Ok(());
                }
                next = bytes.next() => match next {
                    Some(Ok(chunk)) => {
                        buffer.extend_from_slice(&chunk);
                        while let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
                            let line = buffer.drain(..=newline).collect::<Vec<_>>();
                            let line = std::str::from_utf8(&line[..line.len() - 1])
                                .context("Ollama returned non-UTF-8 data")?;
                            if process_line(line, events, &mut received_content)? {
                                return Ok(());
                            }
                        }
                    }
                    Some(Err(error)) => return Err(anyhow!("Ollama stream failed: {error}")),
                    None => {
                        if !buffer.is_empty() {
                            let line = std::str::from_utf8(&buffer)
                                .context("Ollama returned non-UTF-8 data")?;
                            if process_line(line, events, &mut received_content)? {
                                return Ok(());
                            }
                        }
                        if received_content {
                            let _ = events.send(StreamEvent::Finished);
                            return Ok(());
                        }
                        return Err(anyhow!("Ollama stream ended before returning content"));
                    }
                }
            }
        }
    }
}

fn process_line(
    line: &str,
    events: &mpsc::UnboundedSender<StreamEvent>,
    received_content: &mut bool,
) -> Result<bool> {
    let Some(chunk) = ChatChunk::from_line(line).map_err(anyhow::Error::msg)? else {
        return Ok(false);
    };
    if let Some(error) = chunk.error {
        return Err(anyhow!("Ollama error: {error}"));
    }
    if let Some(message) = chunk.message {
        if !message.thinking.is_empty() {
            let _ = events.send(StreamEvent::ThinkingDelta(message.thinking));
        }
        if message.role == "assistant" && !message.content.is_empty() {
            *received_content = true;
            let _ = events.send(StreamEvent::ContentDelta(message.content));
        }
    }
    if chunk.done {
        let _ = events.send(StreamEvent::Finished);
        return Ok(true);
    }
    Ok(false)
}
