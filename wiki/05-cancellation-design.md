# Cancellation design

Each generation owns a Tokio `oneshot` pair. The UI stores the sender and the streaming task owns the receiver.

The streaming loop uses `tokio::select!` to wait for either the next HTTP byte chunk or cancellation. Pressing Stop sends the cancellation signal, drops the response stream, emits `StreamEvent::Cancelled`, and clears the active generation state. Starting another request is disabled until the current request finishes or cancellation is acknowledged.

Cancellation does not block the egui thread and does not discard earlier conversation history.
