#[cfg(feature = "draw_cherry")]
fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    pollster::block_on(internal::run());

    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init().expect("could not initialize logger");
        wasm_bindgen_futures::spawn_local(internal::run());
    }
}

#[cfg(not(feature = "draw_cherry"))]
fn main() {
    eprintln!("This example requires the `draw_cherry` feature.");
}

#[cfg(feature = "draw_cherry")]
mod internal {
    use std::time::{Duration, Instant};

    use jamjar::{
        color,
        draw::{
            cherry::{
                BasicPush, BuiltinImage, BuiltinMesh, BuiltinShader, LitPush, PushFlags,
                ShaderConf, ShaderFlags,
            },
            CanvasConfig, CanvasMode, ResizeMode, ScaleMode, D,
        },
        input::WinitMouse,
        math::*,
    };

    glace::glace! {
        #[path = "examples/assets"]
        mod assets {}
    }

    use assets::prelude::*;

    pub async fn run() {
        let resolution = Vec2::new([320, 180]);

        let (window, event_loop) =
            jamjar::windowing::window_and_event_loop("cherry_canvas", resolution.0).unwrap();

        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowExtWebSys;
            web_sys::window()
                .and_then(|win| win.document())
                .and_then(|doc| doc.body())
                .and_then(|body| {
                    body.append_child(&web_sys::Element::from(window.canvas()))
                        .ok()
                })
                .expect("failed to add canvas to document body");
        }

        let mut canvas_config = CanvasConfig {
            canvas_mode: CanvasMode::Direct,
            resize_mode: ResizeMode::SetLogical(resolution.as_f32().0),
            scale_mode: ScaleMode::Max,
        };

        let mut context = jamjar::draw::cherry::DrawContext::<Image, Mesh, ()>::new(
            &window,
            canvas_config.clone(),
            2048,
            1,
        )
        .await
        .unwrap();

        use glace::BytesAsset;
        let font = jamjar::font::Font::new(Font::Chocolate11.bytes().into_owned(), 11.);

        let mut clock = jamjar::timing::RealClock::new_now();
        let start = clock.now();

        let mut mouse = WinitMouse::new();

        let mut frame_timings = vec![];
        let mut target_fps = 0;
        let fps_targets = [60, 90, 120, 240, 5, 10, 15, 30];

        let mut frame_pacer = jamjar::timing::FramePacer::new();

        let mut per_frame_toggle = false;
        let mut vsync = true;

        event_loop.run(move |event, _, control_flow| {
            use jamjar::windowing::event::{ElementState, Event, VirtualKeyCode, WindowEvent};

            context.handle_winit_event(&event);
            mouse.handle_event(&event);

            match event {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => {
                        *control_flow = jamjar::windowing::event_loop::ControlFlow::Exit
                    }
                    WindowEvent::KeyboardInput { input, .. } => {
                        if let ElementState::Pressed = input.state {
                            use jamjar::draw::{ResizeMode, ScaleMode};

                            match input.virtual_keycode {
                                Some(VirtualKeyCode::Space) => {
                                    target_fps = (target_fps + 1) % fps_targets.len();
                                    frame_timings.clear();
                                }
                                Some(VirtualKeyCode::V) => {
                                    vsync = !vsync;
                                    context.set_vsync(vsync);
                                }
                                _ => (),
                            }
                        }
                    }
                    _ => (),
                },
                Event::NewEvents(jamjar::windowing::event::StartCause::ResumeTimeReached {
                    ..
                }) => {
                    window.request_redraw();
                }
                Event::RedrawRequested(_) => {
                    clock.update();

                    let frame_deadline =
                        frame_pacer.deadline_for_fps(fps_targets[target_fps] as f64);
                    *control_flow =
                        jamjar::windowing::event_loop::ControlFlow::WaitUntil(frame_deadline);

                    let frame_stats = context.frame_stats();
                    dbg!(&frame_stats);

                    frame_timings.push(frame_stats.frame_time.as_secs_f64());
                    if frame_timings.len() > 60 {
                        frame_timings.remove(0);
                    }

                    let mouse_pos = context
                        .window_to_canvas_pos(mouse.position())
                        .unwrap_or([0., 0.]);

                    let sf = context.scale_factor();

                    let t0 = clock.since(start) as f32;
                    let t1 = (t0 * 1.43243) + std::f32::consts::TAU / 2.;

                    let mut ren = context.start_rendering([0.2, 0.6, 1., 1.], mouse_pos, [0.; 4]);

                    let dx = if per_frame_toggle { 0. } else { 2. };
                    per_frame_toggle = !per_frame_toggle;

                    ren.ortho_2d();
                    ren.draw(
                        BuiltinShader::Basic,
                        BuiltinImage::White,
                        BuiltinMesh::Quad,
                        (Mat4::translation([0., 0., 0.]) * Mat4::scale([9999., 9999., 1., 1.])).0,
                        BasicPush {
                            // give your money to
                            tint: color::BLACK,
                            emission: color::TRANS,
                            //                   women
                        },
                        &(),
                        false,
                        None,
                    );

                    ren.perspective_3d(1.0);
                    ren.set_view(
                        (Mat4::translation([dx, 0., 0.])
                            * matrix::axis_rotation([1., 0., 0.], -0.5))
                        .0,
                    );

                    let cube_trans = (Mat4::translation([0., -2., 4.])
                        * matrix::axis_rotation([0., 1., 0.], t1))
                    .0;

                    ren.draw(
                        BuiltinShader::Lit,
                        BuiltinImage::White,
                        &Mesh::Cube,
                        cube_trans,
                        LitPush {
                            tint: color::WHITE,
                            emission: color::TRANS,
                            ambient: [0.1, 0.1, 0.1, 1.],
                            light_dir: [1., -1., 1., 0.],
                            light_col: [0.7, 0.7, 0.7, 1.],
                        },
                        &(),
                        false,
                        None,
                    );

                    ren.ortho_2d();

                    let frame_time = frame_timings.iter().sum::<f64>() / frame_timings.len() as f64;
                    let fps = 1. / frame_time;
                    let text = format!(
                        "cherry_fps_options\n{:.2} fps\ntarget: {}\nvsync: {:?}",
                        fps, fps_targets[target_fps], vsync,
                    );
                    let text =
                        font.layout_wrapped(text, [16., 16.], sf, Some(16.), 1200., 1., None);
                    ren.glyphs(&text, [0., 0.], [0.9, 1., 1., 1.], 2 * D, false);
                }

                _ => (),
            }
        });
    }
}
