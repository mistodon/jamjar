#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn wasm_main() {
    main();
}

fn main() {
    use jamjar::draw::{backend, CanvasConfig};

    jamjar::logging::init_logging();

    let resolution = [512, 256];
    let (window, event_loop) =
        jamjar::windowing::window_and_event_loop("Window Test", resolution).unwrap();

    let mut canvas_config = CanvasConfig::set_scaled(resolution);
    let mut context =
        jamjar::drawsloth::DrawContext::<backend::Whatever>::new(&window, canvas_config).unwrap();

    let src_image = image::load_from_memory(&jamjar::resource!("assets/images/blit.png"))
        .unwrap()
        .to_rgba8();

    jamjar::jprintln!(
        r#"Press:
1. For fixed scaling
2. For set scaling
3. For pixel scaling
4. For free scaling"#
    );

    event_loop.run(move |event, _, control_flow| {
        use jamjar::windowing::event::{ElementState, Event, VirtualKeyCode, WindowEvent};

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
                WindowEvent::KeyboardInput { input, .. } => {
                    if let ElementState::Pressed = input.state {
                        match input.virtual_keycode {
                            Some(VirtualKeyCode::Key1) => {
                                canvas_config = CanvasConfig::fixed(resolution);
                            }
                            Some(VirtualKeyCode::Key2) => {
                                canvas_config = CanvasConfig::set_scaled(resolution);
                            }
                            Some(VirtualKeyCode::Key3) => {
                                canvas_config = CanvasConfig::pixel_scaled(resolution);
                            }
                            Some(VirtualKeyCode::Key4) => {
                                canvas_config = CanvasConfig::default();
                            }
                            _ => (),
                        }

                        context.set_canvas_config(canvas_config);
                        jamjar::jprintln!("Canvas config changed: {:?}", canvas_config);
                    }
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
