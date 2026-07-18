//! Native Ollama chat client built with eframe.

mod app;
mod message;
mod ollama;
mod single_instance;
mod storage;
mod tools;

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

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(app::WINDOW_SIZE)
            .with_min_inner_size(app::MIN_WINDOW_SIZE),
        ..Default::default()
    };

    let result = eframe::run_native(
        "DeskPilot",
        options,
        Box::new(move |cc| Ok(Box::new(AiHelperApp::new(cc, Arc::clone(&runtime))))),
    );
    drop(instance_guard);
    result
}
