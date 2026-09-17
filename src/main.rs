//! DeskPilot: Private, ultra-fast, local-first background AI daemon & Web Studio.

mod ipc;
mod message;
mod ollama;
mod single_instance;
mod storage;
mod tools;

use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let instance_guard = match single_instance::acquire_or_focus() {
        Some(guard) => guard,
        None => return Ok(()),
    };

    println!("====================================================");
    println!("  ThomsonWorks DeskPilot Headless Daemon v0.1.0");
    println!("  Obsidian Velocity Web Studio & Automation Engine");
    println!("====================================================");

    let storage = Arc::new(storage::Storage::new());
    
    // Start Web Studio / REST API server
    let server_task = tokio::spawn(async move {
        ipc::start_server(storage).await;
    });

    println!("DeskPilot daemon running in background on http://127.0.0.1:31415");

    let args: Vec<String> = std::env::args().collect();
    let auto_open = !args.iter().any(|a| a == "--headless" || a == "--no-open");
    if auto_open {
        tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            #[cfg(windows)]
            let _ = std::process::Command::new("cmd").args(["/C", "start", "http://127.0.0.1:31415"]).spawn();
            #[cfg(target_os = "macos")]
            let _ = std::process::Command::new("open").arg("http://127.0.0.1:31415").spawn();
            #[cfg(target_os = "linux")]
            let _ = std::process::Command::new("xdg-open").arg("http://127.0.0.1:31415").spawn();
        });
    }

    println!("Press Ctrl+C to terminate.");

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("\nShutting down DeskPilot daemon gracefully...");
        }
        _ = server_task => {}
    }

    drop(instance_guard);
    Ok(())
}
