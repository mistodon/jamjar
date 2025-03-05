use std::sync::Arc;

#[cfg(web_platform)]
use wasm_bindgen::closure::Closure;

use winit::{
    dpi::LogicalSize,
    event_loop::{EventLoop, EventLoopWindowTarget},
    window::{Window, WindowBuilder},
};

#[cfg(web_platform)]
use crate::web::{self, TouchPhase, WebEvent};

pub use winit::*;

#[cfg(web_platform)]
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
) -> Result<(Arc<Window>, EventLoop<()>), winit::error::OsError> {
    let event_loop = EventLoop::<()>::new().expect("TODO: Handle this error case");

    window(&event_loop, title, logical_size).map(|w| (w, event_loop))
}

pub fn window(
    event_loop: &EventLoopWindowTarget<()>,
    title: &str,
    logical_size: [u32; 2],
) -> Result<Arc<Window>, winit::error::OsError> {
    let logical_window_size = {
        let logical: LogicalSize<u32> = logical_size.into();
        logical
    };

    let mut window_builder = WindowBuilder::new()
        .with_title(title)
        .with_inner_size(logical_window_size);

    #[cfg(web_platform)]
    {
        use wasm_bindgen::JsCast;
        use winit::platform::web::WindowBuilderExtWebSys;
        let canvas = web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .get_element_by_id("canvas")
            .unwrap()
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .unwrap();
        window_builder = window_builder.with_canvas(Some(canvas));
    }

    let window = window_builder.build(event_loop);

    // let sys_window = web_sys::window().unwrap();

    // #[cfg(web_platform)]
    // if let Ok(window) = window.as_ref() {
    //     use wasm_bindgen::JsCast;

    //     let keydown = Closure::wrap(Box::new(|e: web_sys::KeyboardEvent| {
    //         unsafe { web::log_event(WebEvent::KeyTyped(e.key())) };
    //     }) as Box<dyn FnMut(_)>);

    //     // let canvas = winit::platform::web::WindowExtWebSys::canvas(window).unwrap();

    //     // canvas
    //     //     .add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref())
    //     //     .unwrap();

    //     keydown.forget();

    //     sys_window
    //         .document()
    //         .unwrap()
    //         .body()
    //         .unwrap()
    //         .append_child(&canvas)
    //         .unwrap();
    // }

    let window = window.map(Arc::new);

    // #[cfg(web_platform)]
    // if let Ok(window_ptr) = window.as_ref().map(Arc::clone) {
    //     use wasm_bindgen::JsCast;

    //     let sf = window_ptr.scale_factor();
    //     let size = web_window_size().to_physical::<u32>(sf);
    //     window_ptr.set_min_inner_size(Some(size));
    //     unsafe { web::log_event(WebEvent::Resized(size)) };

    //     let window = Arc::clone(&window_ptr);
    //     let resize = Closure::wrap(Box::new(move |_: web_sys::Event| {
    //         let sf = window.scale_factor();
    //         let size = web_window_size().to_physical::<u32>(sf);
    //         window.set_min_inner_size(Some(size));
    //         unsafe { web::log_event(WebEvent::Resized(size)) };
    //     }) as Box<dyn FnMut(_)>);

    //     let touch_event = |window: &Arc<Window>, phase: TouchPhase| {
    //         let window = Arc::clone(window);
    //         Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
    //             let sf = window.scale_factor();
    //             let touches = e.target_touches();
    //             for i in 0..touches.length() {
    //                 let touch = touches.item(i).unwrap();
    //                 unsafe {
    //                     web::log_event(WebEvent::Touch {
    //                         id: touch.identifier() as usize,
    //                         phase,
    //                         x: touch.page_x() as f64 * sf,
    //                         y: touch.page_y() as f64 * sf,
    //                     })
    //                 };
    //             }
    //         }) as Box<dyn FnMut(_)>)
    //     };

    //     let touchstart = touch_event(&window_ptr, TouchPhase::Start);
    //     let touchend = touch_event(&window_ptr, TouchPhase::End);
    //     let touchcancel = touch_event(&window_ptr, TouchPhase::Cancel);
    //     let touchmove = touch_event(&window_ptr, TouchPhase::Move);

    //     sys_window
    //         .add_event_listener_with_callback("resize", resize.as_ref().unchecked_ref())
    //         .unwrap();
    //     sys_window
    //         .add_event_listener_with_callback("touchstart", touchstart.as_ref().unchecked_ref())
    //         .unwrap();
    //     sys_window
    //         .add_event_listener_with_callback("touchend", touchend.as_ref().unchecked_ref())
    //         .unwrap();
    //     sys_window
    //         .add_event_listener_with_callback("touchcancel", touchcancel.as_ref().unchecked_ref())
    //         .unwrap();
    //     sys_window
    //         .add_event_listener_with_callback("touchmove", touchmove.as_ref().unchecked_ref())
    //         .unwrap();

    //     resize.forget();
    //     touchstart.forget();
    //     touchend.forget();
    //     touchcancel.forget();
    //     touchmove.forget();
    // }

    window
}
