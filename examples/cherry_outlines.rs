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
        draw::{
            cherry::{BasicPush, BuiltinImage, BuiltinShader, PushFlags, ShaderConf, ShaderFlags},
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

    #[derive(Clone, PartialEq, Eq, Hash)]
    enum Shader {
        Outline,
    }

    const OUTLINE_SHADER: &str = "
    struct Push {
        transform: mat4x4<f32>,
    };

    var<push_constant> push: Push;

    @vertex
    fn vertex_main(vertex: VertexInput) -> VertexOutput {
        var output: VertexOutput;
        var view_norm = normalize((push.transform * vertex.normal).xy);
        var view_pos = push.transform * vertex.position;
        output.position = view_pos + vec4(view_norm * globals.pixel_size, 0.0, 0.0) * 4.0 * view_pos.w;
        output.normal = vec3(view_norm, 0.0);
        return output;
    }

    @fragment
    fn fragment_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
        return vec4(0.0, 0.0, 0.0, 1.0);
    }
    ";

    pub async fn run() {
        let resolution = [1280, 720];

        let (window, event_loop) =
            jamjar::windowing::window_and_event_loop("cherry_outlines", resolution).unwrap();

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

        let canvas_config = jamjar::draw::CanvasConfig::set_scaled(resolution);
        let mut context = jamjar::draw::cherry::DrawContext::<Image, Mesh, Shader>::new(
            &window,
            canvas_config,
            2048,
            1,
        )
        .await
        .unwrap();

        context.load_shader::<()>(
            Shader::Outline,
            OUTLINE_SHADER,
            ShaderConf {
                phase: 0,
                shader_flags: ShaderFlags::NO_DEPTH_WRITE | ShaderFlags::BACK_FACE_ONLY,
                push_flags: PushFlags::TRANSFORM,
            },
        );

        use glace::BytesAsset;
        let font = jamjar::font::Font::new(Font::Chocolate11.bytes().into_owned(), 11.);

        let mut clock = jamjar::timing::RealClock::new_now();
        let start = clock.now();

        let mut mouse = WinitMouse::new();

        event_loop.run(move |event, _, control_flow| {
            use jamjar::windowing::event::{Event, WindowEvent};

            context.handle_winit_event(&event);
            mouse.handle_event(&event);

            match event {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => {
                        *control_flow = jamjar::windowing::event_loop::ControlFlow::Exit
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

                    let t = clock.since(start) as f32;

                    let mut ren = context.start_rendering([0.2, 0.6, 1., 1.], mouse_pos, [0.; 4]);

                    ren.perspective_3d(1.0);
                    ren.set_view(
                        (Mat4::translation([0., -1.0, 0.])
                            * matrix::axis_rotation([1., 0., 0.], -0.5))
                        .0,
                    );

                    let sphere_trans = (Mat4::translation([-t.cos(), 0., 4.])
                        * matrix::axis_rotation([t * 1., t * 2., t * 3.], t))
                    .0;
                    let cube_trans = (Mat4::translation([t.cos(), 0., 4.])
                        * matrix::axis_rotation([t * 1., t * 2., t * 3.], t))
                    .0;

                    let objects = [(Mesh::Sphere, sphere_trans), (Mesh::Cube, cube_trans)];

                    for (mesh, trans) in objects {
                        ren.draw_stitched(
                            &Shader::Outline,
                            BuiltinImage::White,
                            &mesh,
                            trans,
                            (),
                            &(),
                            false,
                            None,
                        );

                        ren.draw(
                            BuiltinShader::Basic,
                            BuiltinImage::White,
                            &mesh,
                            trans,
                            BasicPush {
                                tint: [1., 1., 1., 1.],
                                emission: [0., 0., 0., 0.],
                            },
                            &(),
                            false,
                            None,
                        );
                    }

                    ren.ortho_2d();

                    let t = t % 10.;
                    let text = font.layout_wrapped(
                        "cherry_outline",
                        [32., 620.],
                        sf,
                        Some(44.),
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
