use std::rc::Rc;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::closure::Closure;

use winit::{
    dpi::LogicalSize,
    event_loop::{EventLoop, EventLoopWindowTarget},
    window::{Window, WindowBuilder},
};

#[cfg(target_arch = "wasm32")]
use crate::web::{self, TouchPhase, WebEvent};

pub use winit::*;

#[cfg(target_arch = "wasm32")]
fn web_window_size() -> LogicalSize<f64> {
    let win = web_sys::window().unwrap();
    let (w, h) = (
        win.inner_width().unwrap().as_f64().unwrap(),
        win.inner_height().unwrap().as_f64().unwrap(),
    );
    LogicalSize::new(w, h)
}

pub fn window_and_event_loop(
    title: &str,
    logical_size: [u32; 2],
) -> Result<(Rc<Window>, EventLoop<()>), winit::error::OsError> {
    let event_loop = EventLoop::<()>::new();

    window(&event_loop, title, logical_size).map(|w| (w, event_loop))
}

pub fn window(
    event_loop: &EventLoopWindowTarget<()>,
    title: &str,
    logical_size: [u32; 2],
) -> Result<Rc<Window>, winit::error::OsError> {
    let logical_window_size = {
        let logical: LogicalSize<u32> = logical_size.into();
        logical
    };

    let window_builder = WindowBuilder::new()
        .with_title(title)
        .with_inner_size(logical_window_size);

    let window = window_builder.build(event_loop);

    #[cfg(target_arch = "wasm32")]
    let sys_window = web_sys::window().unwrap();

    #[cfg(target_arch = "wasm32")]
    if let Ok(window) = window.as_ref() {
        use wasm_bindgen::JsCast;

        let keydown = Closure::wrap(Box::new(|e: web_sys::KeyboardEvent| {
            unsafe { web::log_event(WebEvent::KeyTyped(e.key())) };
        }) as Box<dyn FnMut(_)>);

        let canvas = winit::platform::web::WindowExtWebSys::canvas(window);

        canvas
            .add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref())
            .unwrap();

        keydown.forget();

        sys_window
            .document()
            .unwrap()
            .body()
            .unwrap()
            .append_child(&canvas)
            .unwrap();
    }

    let window = window.map(Rc::new);

    #[cfg(target_arch = "wasm32")]
    if let Ok(window_ptr) = window.as_ref().map(Rc::clone) {
        use wasm_bindgen::JsCast;

        let sf = window_ptr.scale_factor();
        let size = web_window_size().to_physical::<u32>(sf);
        window_ptr.set_inner_size(size);
        unsafe { web::log_event(WebEvent::Resized(size)) };

        let window = Rc::clone(&window_ptr);
        let resize = Closure::wrap(Box::new(move |_: web_sys::Event| {
            let sf = window.scale_factor();
            let size = web_window_size().to_physical::<u32>(sf);
            window.set_inner_size(size);
            unsafe { web::log_event(WebEvent::Resized(size)) };
        }) as Box<dyn FnMut(_)>);

        let touch_event = |window: &Rc<Window>, phase: TouchPhase| {
            let window = Rc::clone(window);
            Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
                let sf = window.scale_factor();
                let touches = e.target_touches();
                for i in 0..touches.length() {
                    let touch = touches.item(i).unwrap();
                    unsafe {
                        web::log_event(WebEvent::Touch {
                            id: touch.identifier() as usize,
                            phase,
                            x: touch.page_x() as f64 * sf,
                            y: touch.page_y() as f64 * sf,
                        })
                    };
                }
            }) as Box<dyn FnMut(_)>)
        };

        let touchstart = touch_event(&window_ptr, TouchPhase::Start);
        let touchend = touch_event(&window_ptr, TouchPhase::End);
        let touchcancel = touch_event(&window_ptr, TouchPhase::Cancel);
        let touchmove = touch_event(&window_ptr, TouchPhase::Move);

        sys_window
            .add_event_listener_with_callback("resize", resize.as_ref().unchecked_ref())
            .unwrap();
        sys_window
            .add_event_listener_with_callback("touchstart", touchstart.as_ref().unchecked_ref())
            .unwrap();
        sys_window
            .add_event_listener_with_callback("touchend", touchend.as_ref().unchecked_ref())
            .unwrap();
        sys_window
            .add_event_listener_with_callback("touchcancel", touchcancel.as_ref().unchecked_ref())
            .unwrap();
        sys_window
            .add_event_listener_with_callback("touchmove", touchmove.as_ref().unchecked_ref())
            .unwrap();

        resize.forget();
        touchstart.forget();
        touchend.forget();
        touchcancel.forget();
        touchmove.forget();
    }

    window
}
