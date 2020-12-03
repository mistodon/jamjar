pub use winit::*;
use winit::{
    event_loop::EventLoop,
    window::{Window, WindowBuilder},
};

pub fn window_and_event_loop(
    title: &str,
    logical_size: [u32; 2],
) -> Result<(Window, EventLoop<()>), winit::error::OsError> {
    let event_loop = EventLoop::<()>::new();

    window(&event_loop, title, logical_size).map(|w| (w, event_loop))
}

pub fn window(
    event_loop: &EventLoop<()>,
    title: &str,
    logical_size: [u32; 2],
) -> Result<Window, winit::error::OsError> {
    let logical_window_size = {
        use winit::dpi::LogicalSize;
        let logical: LogicalSize<u32> = logical_size.into();
        logical
    };

    let window_builder = WindowBuilder::new()
        .with_title(title)
        .with_inner_size(logical_window_size);

    let window = window_builder.build(event_loop);

    #[cfg(target_arch = "wasm32")]
    if let Ok(window) = window.as_ref() {
        web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .body()
            .unwrap()
            .append_child(&winit::platform::web::WindowExtWebSys::canvas(window))
            .unwrap();
    }

    window
}
