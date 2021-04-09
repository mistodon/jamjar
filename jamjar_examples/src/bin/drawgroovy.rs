#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn wasm_main() {
    main();
}

fn main() {
    use jamjar::{
        atlas::{ImageAtlas},
        draw::{backend, CanvasConfig, CanvasMode},
        drawgroovy::{DrawContext, Sprite},
        windowing,
    };

    jamjar::logging::init_logging();

    let resolution = [512, 256];

    let (window, event_loop) =
        windowing::window_and_event_loop("Window Test", resolution).unwrap();

    let white_img = image::load_from_memory(&jamjar::resource!("assets/images/white.png"))
        .unwrap()
        .to_rgba8();
    let bubble_img = image::load_from_memory(&jamjar::resource!("assets/images/bubble.png"))
        .unwrap()
        .to_rgba8();

    let mut atlas = ImageAtlas::new();
    atlas.insert("white".to_owned(), white_img);
    atlas.insert("bubble".to_owned(), bubble_img);

    let mut canvas_config = CanvasConfig::pixel_scaled(resolution);
    let mut context = DrawContext::<backend::Whatever>::new(
        &window,
        canvas_config,
        atlas.compile(),
    )
    .unwrap();

    let mut clock = jamjar::timing::RealClock::new_now();

    jamjar::jprintln!(
        r#"Press:
1. For fixed scaling
2. For set scaling
3. For pixel scaling
4. For free scaling

0. To toggle between Direct and Intermediate modes"#
    );

    event_loop.run(move |event, _, control_flow| {
        use windowing::event::{ElementState, Event, VirtualKeyCode, WindowEvent};

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    *control_flow = windowing::event_loop::ControlFlow::Exit
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
                        let mut mode = canvas_config.canvas_mode;

                        match input.virtual_keycode {
                            Some(VirtualKeyCode::Key0) => {
                                mode = match mode {
                                    CanvasMode::Direct => CanvasMode::Intermediate,
                                    CanvasMode::Intermediate => CanvasMode::Direct,
                                }
                            }
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

                        canvas_config.canvas_mode = mode;
                        context.set_canvas_config(canvas_config);
                        jamjar::jprintln!("Canvas config changed: {:?}", canvas_config);
                    }
                }
                _ => (),
            },
            Event::MainEventsCleared => {
                clock.update();
                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                let t = (clock.secs() % 8.) as f32 / 8.;

                let mut ren = context.start_rendering([0.2, 0., 0.4, 1.]);

                for i in 0..8 {
                    let ii = i as f32 / 8.;
                    let it = (t + ii) % 1.;
                    let a = it * std::f32::consts::TAU;
                    let tt = t * std::f32::consts::TAU;

                    let size = 40. + a.cos() * 10.;
                    let scale = size / 16.;

                    let r = 80. + tt.sin() * 20.;

                    let x = 384. + a.cos() * r - size / 2.;
                    let y = 128. + a.sin() * r - size / 2.;

                    let r = it;
                    let g = (it + 0.33) % 1.;
                    let b = (it + 0.66) % 1.;

                    ren.sprite(Sprite::scaled(atlas.region("bubble"), [x, y], [r, g, b, 1.], [scale, scale]));
                }

                for hue in 0..4 {
                    for sat in 0..5 {
                        let v = (4 - sat) as f32 * 0.25;
                        let r = if hue == 0 || hue == 3 { v } else { 0. };
                        let g = if hue == 1 || hue == 3 { v } else { 0. };
                        let b = if hue == 2 || hue == 3 { v } else { 0. };

                        ren.sprite(Sprite::scaled(atlas.region("white"), [hue as f32 * 32., sat as f32 * 32.], [r, g, b, 1.], [0.5, 0.5]));
                    }
                }

                ren.sprite(Sprite::scaled(atlas.region("bubble"), [500., 128.], [0., 1., 0., 1.], [3., 3.]));
            }
            _ => (),
        }
    });
}
