use single_instance::SingleInstance;

pub struct InstanceGuard(SingleInstance);

pub fn acquire_or_focus() -> Option<InstanceGuard> {
    let instance = SingleInstance::new("VertexInfinity.DeskPilot.SingleInstance").ok()?;
    if !instance.is_single() {
        println!("DeskPilot daemon is already running on http://127.0.0.1:31415");
        #[cfg(windows)]
        let _ = std::process::Command::new("cmd").args(["/C", "start", "http://127.0.0.1:31415"]).spawn();
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open").arg("http://127.0.0.1:31415").spawn();
        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("xdg-open").arg("http://127.0.0.1:31415").spawn();
        return None;
    }
    Some(InstanceGuard(instance))
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        let _ = &self.0;
    }
}
