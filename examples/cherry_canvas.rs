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
    use jamjar::{
        color,
        draw::{
            cherry::{
                BasicPush, BuiltinImage, BuiltinMesh, BuiltinShader, LitPush, PushFlags,
                ShaderConf, ShaderFlags,
            },
            D,
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
        let resolution = Vec2::new([300, 200]);

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

        let mut canvas_config = jamjar::draw::CanvasConfig::set_scaled(resolution.as_f32().0);
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
                                Some(VirtualKeyCode::Key1) => {
                                    canvas_config.resize_mode = ResizeMode::Free;
                                }
                                Some(VirtualKeyCode::Key2) => {
                                    canvas_config.resize_mode =
                                        ResizeMode::SetLogical(resolution.as_f32().0);
                                }
                                Some(VirtualKeyCode::Key3) => {
                                    canvas_config.resize_mode =
                                        ResizeMode::SetLogicalWidth(resolution.0[0] as f32);
                                }
                                Some(VirtualKeyCode::Key4) => {
                                    canvas_config.resize_mode =
                                        ResizeMode::SetLogicalHeight(resolution.0[1] as f32);
                                }
                                Some(VirtualKeyCode::Key5) => {
                                    canvas_config.resize_mode =
                                        ResizeMode::SetLogicalMin(resolution.0[0] as f32);
                                }
                                Some(VirtualKeyCode::Key6) => {
                                    canvas_config.resize_mode =
                                        ResizeMode::SetPhysical(resolution.as_u32().0);
                                }
                                Some(VirtualKeyCode::Key7) => {
                                    canvas_config.resize_mode =
                                        ResizeMode::SetPhysicalWidth(resolution.0[0] as u32);
                                }
                                Some(VirtualKeyCode::Key8) => {
                                    canvas_config.resize_mode =
                                        ResizeMode::SetPhysicalHeight(resolution.0[1] as u32);
                                }
                                Some(VirtualKeyCode::Key9) => {
                                    canvas_config.resize_mode =
                                        ResizeMode::SetPhysicalMin(resolution.0[0] as u32);
                                }
                                Some(VirtualKeyCode::Key0) => {
                                    canvas_config.resize_mode = ResizeMode::Aspect([16, 9]);
                                }
                                Some(VirtualKeyCode::Q) => {
                                    canvas_config.scale_mode = ScaleMode::Set(1.0);
                                }
                                Some(VirtualKeyCode::W) => {
                                    canvas_config.scale_mode = ScaleMode::Max;
                                }
                                Some(VirtualKeyCode::E) => {
                                    canvas_config.scale_mode = ScaleMode::MaxInt;
                                }
                                _ => (),
                            }

                            context.set_canvas_config(canvas_config);
                        }
                    }
                    _ => (),
                },
                Event::MainEventsCleared => {
                    clock.update();
                    window.request_redraw();
                }
                Event::RedrawRequested(_) => {
                    dbg!(context.frame_stats());

                    let mouse_pos = context
                        .window_to_canvas_pos(mouse.position())
                        .unwrap_or([0., 0.]);

                    let sf = context.scale_factor();

                    let t0 = clock.since(start) as f32;
                    let t1 = (t0 * 1.43243) + std::f32::consts::TAU / 2.;

                    let mut ren = context.start_rendering([0.2, 0.6, 1., 1.], mouse_pos, [0.; 4]);

                    // Background
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
                        (Mat4::translation([0., 0., 0.])
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

                    let text = font.layout_wrapped(
                        "cherry_canvas",
                        [16., 170.],
                        sf,
                        Some(16.),
                        1200.,
                        1.,
                        None,
                    );
                    ren.glyphs(&text, [0., 0.], [0.9, 1., 1., 1.], 2 * D, false);

                    let mut canvas_state = String::new();

                    use jamjar::draw::{ResizeMode, ScaleMode};

                    match canvas_config.resize_mode {
                        ResizeMode::Free => canvas_state.push_str("Free"),
                        ResizeMode::SetLogical(_) => canvas_state.push_str("Log"),
                        ResizeMode::SetLogicalWidth(_) => canvas_state.push_str("LogW"),
                        ResizeMode::SetLogicalHeight(_) => canvas_state.push_str("LogH"),
                        ResizeMode::SetLogicalMin(_) => canvas_state.push_str("LogM"),
                        ResizeMode::SetPhysical(_) => canvas_state.push_str("Phys"),
                        ResizeMode::SetPhysicalWidth(_) => canvas_state.push_str("PhysW"),
                        ResizeMode::SetPhysicalHeight(_) => canvas_state.push_str("PhysH"),
                        ResizeMode::SetPhysicalMin(_) => canvas_state.push_str("PhysM"),
                        ResizeMode::Aspect(_) => canvas_state.push_str("Aspect"),
                    }

                    match canvas_config.scale_mode {
                        ScaleMode::Set(_) => canvas_state.push_str(".Set"),
                        ScaleMode::Max => canvas_state.push_str(".Max"),
                        ScaleMode::MaxInt => canvas_state.push_str(".MaxInt"),
                    }

                    let canvas_state = format!("{}\n(Num keys/QWE to change)", canvas_state);

                    let text = font.layout_wrapped(
                        canvas_state,
                        [16., 16.],
                        sf,
                        Some(16.),
                        1200.,
                        1.,
                        None,
                    );
                    ren.glyphs(&text, [0., 0.], [0.9, 1., 1., 1.], 2 * D, false);
                }

                _ => (),
            }
        });
    }
}
