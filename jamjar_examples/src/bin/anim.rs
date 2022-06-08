#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn wasm_main() {
    main();
}

fn main() {
    use jamjar::{
        anim::*,
        atlas::{Atlas, FontImageAtlas},
        color,
        draw::{
            backend,
            groove::{DrawContext, Sprite},
            CanvasConfig,
        },
        font::Font,
        input::{Key, WinitKeyboard},
        math::*,
        timing::*,
        utils::*,
        web::{self, WebEvent},
        windowing,
    };

    jamjar::logging::init_logging();

    let resolution = [512, 256];

    let (window, event_loop) =
        windowing::window_and_event_loop("Animation Test", resolution).unwrap();

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
    let mut atlas: FontImageAtlas<String> = FontImageAtlas::new([4096, 4096], 1024);
    atlas.images.insert(("white".to_string(), white_img));
    atlas.images.insert(("bubble".to_string(), bubble_img));
    atlas.compile_into(&mut atlas_image);

    let canvas_config = CanvasConfig::pixel_scaled(resolution);
    let mut context =
        DrawContext::<backend::Whatever>::new(&window, canvas_config, atlas_image.clone(), false)
            .unwrap();

    let mut keyboard = WinitKeyboard::new();

    let mut clock = LogicClock::new_now();

    let mut anim = Anim::new(LogicTimestamp::zero(), 0.3);
    let mut side = Flux::new(false);

    let mut walk_anim = Anim::new(LogicTimestamp::zero(), 0.3);
    let mut walk_pos = Flux::new(0);

    event_loop.run(move |event, _, control_flow| {
        use windowing::event::{Event, WindowEvent};

        keyboard.handle_event(&event);

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
                _ => (),
            },
            Event::MainEventsCleared => {
                clock.update();

                for event in web::poll_events() {
                    match event {
                        WebEvent::Resized(dims) => {
                            context.resolution_changed(dims.into());
                        }
                        _ => (),
                    }
                }

                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                if keyboard.pressed(Key::Space) {
                    let new_value = !side.completed();
                    anim.at_mut(clock.now()).invert();
                    side.change_to(new_value);
                }

                if keyboard.pressed(Key::W) {
                    if let Some(index) = walk_pos.value() {
                        walk_pos.change_to(index + 1);
                        walk_anim.at_mut(clock.now()).restart();
                    }
                }

                if anim.at(clock.now()).finished() {
                    side.complete();
                }

                if walk_anim.at(clock.now()).finished() {
                    walk_pos.complete();
                }

                let sf = context.scale_factor();

                let mut glyphs = vec![];

                let mut ren = context.start_rendering(color::BLACK);

                let left_x = 0.;
                let right_x = resolution[0] as f32 - 120.;
                let a = anim.at(clock.now());
                let t = a.ease_dir_t(|t| t * t, !side.completed());
                let dx = math::lerp(left_x, right_x, t as f32);

                ren.sprite(Sprite::sized(
                    atlas.images.fetch(&"white".into()),
                    [0., 0.],
                    color::BLUE,
                    [resolution[0] as f32, resolution[1] as f32],
                ));

                ren.sprite(Sprite::sized(
                    atlas.images.fetch(&"white".into()),
                    [10. + dx, 10.],
                    if side.is_value() {
                        color::CYAN
                    } else {
                        color::YELLOW
                    },
                    [100., 100.],
                ));

                let positions = [[20., 130.], [50., 170.], [70., 210.], [30., 150.]];

                let walk_t = walk_anim.at(clock.now()).ease_t(|t| t * t) as f32;
                let bubble_pos = {
                    let from = walk_pos.cancelled();
                    let to = walk_pos.completed();
                    let pos_a = positions[from % positions.len()];
                    let pos_b = positions[to % positions.len()];
                    [
                        math::lerp(pos_a[0], pos_b[0], walk_t),
                        math::lerp(pos_a[1], pos_b[1], walk_t),
                    ]
                };

                ren.sprite(Sprite::tinted(
                    atlas.images.fetch(&"bubble".into()),
                    bubble_pos,
                    color::WHITE,
                ));

                glyphs.push(font.layout_line(
                    if side.completed() { "Right" } else { "Left" },
                    [20. + dx, 20.],
                    11.,
                    sf,
                ));

                for glyphs in glyphs {
                    ren.glyphs(&glyphs, [0., 0.], color::BLACK);
                }

                ren.finish_with_text(&mut atlas.fonts, None);

                keyboard.clear_presses();
            }
            _ => (),
        }
    });
}
