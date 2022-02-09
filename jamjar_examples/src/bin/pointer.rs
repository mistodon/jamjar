use std::collections::HashMap;

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
            CanvasConfig,
        },
        font::Font,
        input::WinitMouse,
        web::{self, TouchPhase, WebEvent},
        windowing,
    };

    jamjar::logging::init_logging();

    let resolution = [512, 256];

    let (window, event_loop) =
        windowing::window_and_event_loop("Pointer Test", resolution).unwrap();

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

    let mut mouse = WinitMouse::new();

    let mut touches = HashMap::new();

    event_loop.run(move |event, _, control_flow| {
        use windowing::event::{Event, WindowEvent};

        mouse.handle_event(&event);

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
                for event in web::poll_events() {
                    match event {
                        WebEvent::Resized(dims) => {
                            context.resolution_changed(dims.into());
                        }
                        WebEvent::Touch { id, x, y, phase } => match phase {
                            TouchPhase::Start => {
                                touches.insert(id, [x, y]);
                            }
                            TouchPhase::End | TouchPhase::Cancel => {
                                touches.remove(&id);
                            }
                            TouchPhase::Move => {
                                if let Some(touch) = touches.get_mut(&id) {
                                    *touch = [x, y];
                                }
                            }
                        },
                        _ => (),
                    }
                }

                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                let mouse_pos = mouse.position();
                let pointer_pos = context.window_to_canvas_pos(mouse_pos);

                let sf = context.scale_factor();

                let mut glyphs = vec![];
                glyphs.push(font.layout_line(&format!("{:.0?}", mouse_pos), [10., 10.], 11., sf));
                glyphs.push(font.layout_line(&format!("{:.0?}", pointer_pos), [10., 30.], 11., sf));

                let mut touch_pointers = vec![];

                for (i, (id, coord)) in touches.iter().enumerate() {
                    let y = 50. + i as f32 * 20.;
                    let touch_pointer = context.window_to_canvas_pos(*coord);
                    touch_pointers.push(touch_pointer);
                    glyphs.push(font.layout_line(
                        &format!("{}: {:.0?} / {:.0?}", id, coord, touch_pointer),
                        [10., y],
                        11.,
                        sf,
                    ));
                }

                let mut ren = context.start_rendering(color::BLACK);

                ren.sprite(Sprite::sized(
                    atlas.images.fetch(&"white".into()),
                    [0., 0.],
                    color::BLUE,
                    [resolution[0] as f32, resolution[1] as f32],
                ));

                if let Some(pointer_pos) = pointer_pos {
                    ren.sprite(Sprite::tinted(
                        atlas.images.fetch(&"bubble".into()),
                        pointer_pos,
                        color::RED,
                    ));
                }
                for p in touch_pointers.into_iter().filter_map(|x| x) {
                    ren.sprite(Sprite::tinted(
                        atlas.images.fetch(&"bubble".into()),
                        p,
                        color::CYAN,
                    ));
                }

                for glyphs in glyphs {
                    ren.glyphs(&glyphs, [0., 0.], color::WHITE);
                }

                ren.finish_with_text(&mut atlas.fonts, None);
            }
            _ => (),
        }
    });
}
