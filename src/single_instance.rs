use single_instance::SingleInstance;

pub struct InstanceGuard(SingleInstance);

pub fn acquire_or_focus() -> Option<InstanceGuard> {
    let instance = SingleInstance::new("VertexInfinity.DeskPilot.SingleInstance").ok()?;
    if !instance.is_single() {
        focus_existing_window();
        return None;
    }
    Some(InstanceGuard(instance))
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        let _ = &self.0;
    }
}

#[cfg(windows)]
fn focus_existing_window() {
    use std::ptr;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };

    let title = "DeskPilot"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let window = unsafe { FindWindowW(ptr::null(), title.as_ptr()) };
    if !window.is_null() {
        unsafe {
            ShowWindow(window, SW_RESTORE);
            SetForegroundWindow(window);
        }
    }
}

#[cfg(not(windows))]
fn focus_existing_window() {}
