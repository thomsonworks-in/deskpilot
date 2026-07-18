use axum::{routing::post, Json, Router};
use serde::Deserialize;
use tokio::sync::mpsc;
use std::net::SocketAddr;

#[derive(Deserialize)]
pub struct IpcMessage {
    pub content: String,
}

pub async fn start_server(tx: mpsc::UnboundedSender<String>) {
    let app = Router::new().route(
        "/api/message",
        post(move |Json(payload): Json<IpcMessage>| {
            let tx = tx.clone();
            async move {
                let _ = tx.send(payload.content);
                "Message received"
            }
        }),
    );
    let addr = SocketAddr::from(([127, 0, 0, 1], 31415));
    if let Ok(listener) = tokio::net::TcpListener::bind(addr).await {
        let _ = axum::serve(listener, app).await;
    } else {
        eprintln!("Failed to bind IPC server to {}", addr);
    }
}
