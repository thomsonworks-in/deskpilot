//! Native Ollama chat client built with eframe.

mod app;
mod message;
mod ollama;
mod single_instance;
mod storage;
mod tools;
mod ipc;

use std::sync::Arc;

use app::AiHelperApp;

fn main() -> eframe::Result<()> {
    let Some(instance_guard) = single_instance::acquire_or_focus() else {
        return Ok(());
    };
    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to create Tokio runtime"),
    );

    let (ipc_tx, ipc_rx) = tokio::sync::mpsc::unbounded_channel();
    let r = runtime.clone();
    let ipc_tx_clone = ipc_tx.clone();
    r.spawn(async move {
        ipc::start_server(ipc_tx_clone).await;
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(app::WINDOW_SIZE)
            .with_min_inner_size(app::MIN_WINDOW_SIZE)
            .with_decorations(false),
        ..Default::default()
    };

    let result = eframe::run_native(
        "DeskPilot",
        options,
        Box::new(move |cc| Ok(Box::new(AiHelperApp::new(cc, Arc::clone(&runtime), ipc_rx)))),
    );
    drop(instance_guard);
    result
}
