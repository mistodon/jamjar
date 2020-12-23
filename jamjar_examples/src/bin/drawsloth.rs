#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn wasm_main() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(log::Level::Debug).unwrap();
    main();
}

fn main() {
    let (window, event_loop) =
        jamjar::windowing::window_and_event_loop("Window Test", [512, 256]).unwrap();

    let mut context =
        jamjar::drawsloth::DrawContext::<jamjar::gfx::backend::Whatever>::new(&window).unwrap();

    let src_image = image::load_from_memory(&jamjar::resource!("assets/images/blit.png"))
        .unwrap()
        .to_rgba8();

    event_loop.run(move |event, _, control_flow| {
        use jamjar::windowing::event::{Event, WindowEvent};

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    *control_flow = jamjar::windowing::event_loop::ControlFlow::Exit
                }
                WindowEvent::Resized(dims) => {
                    context.resolution_changed(dims.into());
                }
                WindowEvent::ScaleFactorChanged {
                    scale_factor,
                    new_inner_size,
                } => {
                    context.scale_factor_changed(scale_factor, (*new_inner_size).into());
                }
                _ => (),
            },
            Event::MainEventsCleared => {
                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                let ren = context.start_rendering([1., 0., 0., 1.]);
                ren.blit(&src_image);
            }
            _ => (),
        }
    });
}
