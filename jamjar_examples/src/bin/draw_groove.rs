#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn wasm_main() {
    main();
}

fn main() {
    use jamjar::{
        atlas::{Atlas, FontImageAtlas},
        color,
        draw::{
            backend,
            groove::{DrawContext, Sprite},
            CanvasConfig, CanvasMode, D,
        },
        font::Font,
        windowing,
    };

    jamjar::logging::init_logging();

    let resolution = [512, 256];

    let (window, event_loop) = windowing::window_and_event_loop("Window Test", resolution).unwrap();

    let white_img = image::load_from_memory(&jamjar::resource!("assets/images/white.png"))
        .unwrap()
        .to_rgba8();
    let bubble_img = image::load_from_memory(&jamjar::resource!("assets/images/bubble.png"))
        .unwrap()
        .to_rgba8();

    let font = Font::new(
        jamjar::resource!("assets/fonts/chocolate_11.ttf").to_vec(),
        11.,
    );

    let mut atlas_image = image::RgbaImage::new(4096, 4096);
    let mut atlas = FontImageAtlas::new([4096, 4096], 1024);
    atlas.images.insert(("white".to_owned(), white_img));
    atlas.images.insert(("bubble".to_owned(), bubble_img));
    atlas.compile_into(&mut atlas_image);

    let mut canvas_config = CanvasConfig::pixel_scaled(resolution);
    let mut context =
        DrawContext::<backend::Whatever>::new(&window, canvas_config, atlas_image.clone(), false)
            .unwrap();

    let mut clock = jamjar::timing::RealClock::new_now();
    let mut text_start = clock.now();

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

                let sf = context.scale_factor();
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

                    ren.sprite(Sprite::scaled(
                        atlas.images.fetch("bubble"),
                        [x, y],
                        [r, g, b, 1.],
                        [scale, scale],
                    ));
                }

                for hue in 0..4 {
                    for sat in 0..5 {
                        let v = (4 - sat) as f32 * 0.25;
                        let r = if hue == 0 || hue == 3 { v } else { 0. };
                        let g = if hue == 1 || hue == 3 { v } else { 0. };
                        let b = if hue == 2 || hue == 3 { v } else { 0. };

                        ren.sprite(Sprite::scaled(
                            atlas.images.fetch("white"),
                            [hue as f32 * 32., sat as f32 * 32.],
                            [r, g, b, 1.],
                            [0.5, 0.5],
                        ));
                    }
                }

                ren.sprite(Sprite::scaled(
                    atlas.images.fetch("bubble"),
                    [500., 128.],
                    [0., 1., 0., 1.],
                    [3., 3.],
                ));

                let glyphs = font.layout_line("Hello, world!", [160., 100.], 11., sf);
                ren.glyphs(&glyphs, [0., 0.], [1., 0., 1., 1.]);

                let glyphs = font.layout_wrapped("An-absurdly-very-long-first-line but the rest of this thing should wrap.\n\nIncluding manual newlines.",
                    [160., 111.], 11., sf, 260., 0., None);
                ren.glyphs(&glyphs, [0., 0.], [1., 1., 0., 1.]);

                let glyphs = font.layout_wrapped("Text can be aligned from left, center, right - or anywhere in between.",
                    [384., 4.], 11., sf, 512., 0., Some(1.));
                ren.glyphs(&glyphs, [0., 0.], color::WHITE);

                let cost_fn = |ch| match ch {
                    '.' => 0.5,
                    _ => 0.1,
                };

                let glyphs = font.layout_wrapped("This. Is. Stuttering... Text.", [4., 220.], 11., sf, 120., 0., None);
                let (_, typed) = ren.glyphs_partial(&glyphs, [0., 0.], color::CYAN, 0*D, clock.since(text_start), cost_fn);

                let (cur, part_1) = font.layout_wrapped_cur("This. Is. ", [128., 220.], 11., sf, 240., 0., None);
                let (cur, part_2) = font.layout_wrapped_cur("Multicolor...", cur, 11., sf, 240., 0., None);
                let (_, part_3) = font.layout_wrapped_cur("Text...", cur, 11., sf, 240., 0., None);

                let (budget, _) = ren.glyphs_partial(&part_1, [0., 0.], color::WHITE, 0*D, clock.since(text_start), cost_fn);
                let (budget, _) = ren.glyphs_partial(&part_2, [0., 0.], color::GREEN, 0*D, budget, cost_fn);
                let (_, multi_typed) = ren.glyphs_partial(&part_3, [0., 0.], color::CYAN, 0*D, budget, cost_fn);

                if typed.is_none() && multi_typed.is_none() {
                    text_start = clock.now();
                }

                ren.finish_with_text(&mut atlas.fonts, None);
            }
            _ => (),
        }
    });
}
