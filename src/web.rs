use std::sync::{Mutex, MutexGuard};

use winit::dpi::PhysicalSize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPhase {
    Start,
    End,
    Cancel,
    Move,
}

#[non_exhaustive]
pub enum WebEvent {
    Resized(PhysicalSize<u32>),
    KeyTyped(String),
    Touch {
        id: usize,
        phase: TouchPhase,
        x: f64,
        y: f64,
    },
}

static mut WEB_EVENTS: Option<Mutex<Vec<WebEvent>>> = None;

pub(crate) unsafe fn ensure_events() -> MutexGuard<'static, Vec<WebEvent>> {
    if WEB_EVENTS.is_none() {
        WEB_EVENTS = Some(Mutex::new(Vec::new()));
    }
    WEB_EVENTS.as_mut().unwrap().lock().unwrap()
}

#[cfg(target_arch = "wasm32")]
pub(crate) unsafe fn log_event(event: WebEvent) {
    ensure_events().push(event);
}

pub fn poll_events() -> impl Iterator<Item = WebEvent> {
    let events = unsafe { ensure_events().drain(..).collect::<Vec<_>>() };
    events.into_iter()
}
