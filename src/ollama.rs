use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use reqwest::Client;
use tokio::sync::{mpsc, oneshot};

use crate::message::{
    ChatChunk, ChatOptions, ChatRequest, EmbedRequest, EmbedResponse, Message, OllamaTagsResponse,
    RunningModelsResponse,
};

#[derive(Debug)]
pub enum StreamEvent {
    Started,
    ContentDelta(String),
    ThinkingDelta(String),
    ModelLoading(String),
    ModelReady(String),
    ModelLoadFailed { model: String, error: String },
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

    pub async fn load_model(&self, model: &str) -> Result<()> {
        let response = self
            .http
            .post(format!("{}/api/generate", self.base_url))
            .json(&serde_json::json!({ "model": model, "keep_alive": -1, "stream": false }))
            .send()
            .await
            .context("could not request model load")?;
        match response_error(response, "model load").await {
            Ok(_) => {}
            Err(error) if is_cuda_runner_failure(&error) => {
                let response = self
                    .http
                    .post(format!("{}/api/generate", self.base_url))
                    .json(&serde_json::json!({
                        "model": model,
                        "keep_alive": -1,
                        "stream": false,
                        "options": { "num_gpu": 0, "num_ctx": 4096 }
                    }))
                    .send()
                    .await
                    .context("could not request CPU model load after CUDA failed")?;
                response_error(response, "CPU model load after CUDA failed").await?;
            }
            Err(error) => return Err(error),
        }
        Ok(())
    }

    pub async fn stream_chat(
        &self,
        model: &str,
        messages: &[Message],
        events: &mpsc::UnboundedSender<StreamEvent>,
        mut cancel: oneshot::Receiver<()>,
        high_thinking: bool,
    ) -> Result<()> {
        let request = ChatRequest {
            model,
            messages,
            stream: true,
            think: high_thinking,
            options: None,
        };
        let response = self
            .http
            .post(format!("{}/api/chat", self.base_url))
            .json(&request)
            .send()
            .await
            .context("could not start Ollama chat")?;
        let response = match response_error(response, "chat request").await {
            Ok(response) => response,
            Err(error) if is_cuda_runner_failure(&error) => {
                let _ = events.send(StreamEvent::Notice(
                    "Ollama's CUDA runner crashed; retrying this model on CPU".to_owned(),
                ));
                let cpu_request = ChatRequest {
                    options: Some(ChatOptions {
                        num_gpu: 0,
                        num_ctx: 4096,
                    }),
                    ..request
                };
                let response = self
                    .http
                    .post(format!("{}/api/chat", self.base_url))
                    .json(&cpu_request)
                    .send()
                    .await
                    .context("could not retry Ollama chat on CPU")?;
                response_error(response, "CPU chat retry after CUDA failed").await?
            }
            Err(error) => return Err(error),
        };
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

fn is_cuda_runner_failure(error: &anyhow::Error) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("cuda error")
        || message.contains("shared object initialization failed")
        || message.contains("llama-server process has terminated")
}

async fn response_error(response: reqwest::Response, operation: &str) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let detail = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.as_str())
                .map(str::to_owned)
        })
        .unwrap_or(body);
    Err(anyhow!(
        "Ollama {operation} failed (HTTP {status}): {detail}"
    ))
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
