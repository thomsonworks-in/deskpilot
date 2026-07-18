# Data flow

1. The user submits text in `AiHelperApp`.
2. The user message and an empty assistant message are appended to the transcript.
3. A Tokio task calls `POST /api/chat` with the complete prior conversation.
4. Ollama returns newline-delimited JSON. `ollama.rs` buffers partial byte chunks until complete lines are available.
5. Each valid assistant delta becomes `StreamEvent::ContentDelta` on the direct event channel.
6. The egui frame loop polls events, appends deltas, and requests frequent repaints while generation is active.
7. `Finished`, `Cancelled`, or `Error` returns the UI to an idle state.

Model discovery follows the same channel: a startup task calls `GET /api/tags` and sends `ModelsLoaded` or `Error`.

No network operation runs on the egui thread.
