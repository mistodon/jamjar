#[cfg(feature = "draw_cherry")]
fn main() {
    #[cfg(not(web_platform))]
    pollster::block_on(internal::run());

    #[cfg(web_platform)]
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
            cherry::{BasicPush, BuiltinImage, BuiltinShader, LitPush, ShaderConf, SpriteParams},
            D,
        },
        input::{prelude::*, support::winit::WinitMouse},
        math::*,
    };

    glace::glace! {
        #[path = "examples/assets"]
        mod assets {}
    }

    use assets::prelude::*;

    const CUSTOM_SHADER: &str = r#"
    struct Push {
        transform: mat4x4<f32>,
        uv_offset_scale: vec4<f32>,
        time: vec4<f32>,
    };

    var<push_constant> push: Push;

    @vertex
    fn vertex_main(vertex: VertexInput) -> VertexOutput {
        var output: VertexOutput;
        output.position = push.transform * vertex.position + vec4(0.0, 1.0, 0.0, 0.0) * cos(push.time.x);
        output.normal = normalize(push.transform * vertex.normal).xyz;
        output.uv = vertex.uv.xy * (push.uv_offset_scale.zw) + push.uv_offset_scale.xy;
        output.color = vertex.color;
        return output;
    }

    @fragment
    fn fragment_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
        var base_color = textureSample(
            textures,
            textureSampler,
            vertex.uv,
            texture_page.index
        );

        return base_color * vertex.color + vec4(0.0, 1.0, 1.0, 0.0) * cos(push.time.y);
    }
    "#;

    #[repr(C)]
    struct CustomPush {
        time: [f32; 4],
    }

    pub async fn run() {
        let resolution = Vec2::new([512, 256]);

        let (window, event_loop) =
            jamjar::windowing::window_and_event_loop("draw_cherry", [512, 256]).unwrap();

        #[cfg(web_platform)]
        {
            use winit::platform::web::WindowExtWebSys;
            web_sys::window()
                .and_then(|win| win.document())
                .and_then(|doc| doc.body())
                .and_then(|body| {
                    body.append_child(&web_sys::Element::from(
                        window.canvas().expect("Failed to get canvas"),
                    ))
                    .ok()
                })
                .expect("failed to add canvas to document body");
        }

        let canvas_config = jamjar::draw::CanvasConfig::set_scaled(resolution.as_f32().0);
        let mut context = jamjar::draw::cherry::DrawContext::<Image, Mesh, ()>::new(
            &window,
            canvas_config,
            2048,
            1,
        )
        .await
        .unwrap();

        context.load_shader::<CustomPush>((), CUSTOM_SHADER, ShaderConf::default());

        use glace::BytesAsset;
        let font = jamjar::font::Font::new(Font::Chocolate11.bytes().into_owned(), 11.);

        let mut clock = jamjar::timing::RealClock::new_now();
        let start = clock.now();

        let mut mouse = WinitMouse::new();

        event_loop.set_control_flow(jamjar::windowing::event_loop::ControlFlow::Poll);
        let mut frame_pacer = jamjar::timing::FramePacer::new();

        event_loop.run(move |event, elwt| {
            use jamjar::windowing::event::{Event, WindowEvent};

            context.handle_winit_event(&event);
            mouse.handle_event(&event);

            match event {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => {
                        elwt.exit();
                    }
                    _ => (),
                },
                Event::AboutToWait => {
                    clock.update();
                    // let frame_deadline =
                    //     frame_pacer.deadline_for_fps(60.);
                    // elwt.set_control_flow(jamjar::windowing::event_loop::ControlFlow::WaitUntil(frame_deadline));

                    let mouse_pos = context
                        .window_to_canvas_pos(mouse.position())
                        .unwrap_or([0., 0.]);

                    let sf = context.scale_factor();
                    let mut ren = context.start_rendering([0.2, 0., 0.4, 1.], mouse_pos, [0.; 4]);
                    ren.ortho_2d();

                    let t = clock.since(start);
                    let frame = ((t * 4.0) % 4.0) as usize;

                    let f = ren.sprite(
                        &Image::Pattern1,
                        SpriteParams {
                            pixelly: false,
                            ..Default::default()
                        },
                    );
                    let f = ren.sprite(
                        &Image::Pattern2,
                        SpriteParams {
                            pos: f.tr().into(),
                            ..Default::default()
                        },
                    );
                    let f = ren.sprite(
                        &Image::Pattern3,
                        SpriteParams {
                            pos: f.tr().into(),
                            pixelly: false,
                            ..Default::default()
                        },
                    );
                    let _ = ren.sprite(
                        &Image::Wheel,
                        SpriteParams {
                            pos: f.tr().into(),
                            cel: ([frame % 2, frame / 2], [2, 2]),
                            pixelly: false,
                            ..Default::default()
                        },
                    );

                    ren.sprite(
                        &Image::Pattern3,
                        SpriteParams {
                            pos: mouse_pos,
                            pixelly: false,
                            ..Default::default()
                        },
                    );

                    let a = font.layout_wrapped(A, [32., 0.], sf, None, 512. - 32., 1., None);
                    let b = font.layout_wrapped(B, [32., 160.], sf, None, 512. - 32., 1., None);
                    ren.glyphs(&a, [0., 16.], [0., 1., 1., 1.], 2 * D, false);
                    ren.glyphs_partial(&b, [0., 16.], [1., 1., 1., 1.], 2 * D, false, t, |ch| {
                        match ch {
                            ',' | '/' => 1.,
                            _ => 0.1,
                        }
                    });

                    ren.perspective_3d(1.0);

                    let text_label_transform = Mat4::translation([0., 1.0, 2.]) * Mat4::scale([0.01, 0.01, 1., 1.]);

                    let c3d = font.layout_wrapped("AH!\nI wish this was the right way up.", [0., 0.], sf, None, 9e9, 1., None);
                    ren.glyphs3d(&c3d, BuiltinShader::Basic, text_label_transform.0, [0., 0.], [1., 1., 0., 1.], 5 * D, false, false);

                    ren.draw(
                        BuiltinShader::Basic,
                        BuiltinImage::White,
                        &Mesh::ColorCube,
                        text_label_transform.0,
                        BasicPush::default(),
                        &(),
                        false,
                        None,
                    );


                    ren.draw(
                        BuiltinShader::Basic,
                        BuiltinImage::White,
                        &Mesh::ColorCube,
                        (Mat4::translation([0., -0.7, 2.])
                            * matrix::axis_rotation([0., 1., 0.], t as f32))
                        .0,
                        BasicPush::default(),
                        &(),
                        false,
                        None,
                    );
                    ren.draw(
                        BuiltinShader::Lit,
                        BuiltinImage::White,
                        &Mesh::ColorCube,
                        (Mat4::translation([-1.5, -0.7, 2.])
                            * matrix::axis_rotation([0., 1., 0.], t as f32))
                        .0,
                        LitPush {
                            light_dir: vec4(1., -1., 1., 0.).norm_zero().0,
                            light_col: color::WHITE,
                            ..Default::default()
                        },
                        &(),
                        false,
                        None,
                    );
                    ren.draw(
                        BuiltinShader::Simple,
                        BuiltinImage::White,
                        &Mesh::ColorCube,
                        (Mat4::translation([1.5, -0.7, 2.])
                            * matrix::axis_rotation([0., 1., 0.], t as f32))
                        .0,
                        (),
                        &(),
                        false,
                        None,
                    );
                    ren.draw(
                        &(),
                        BuiltinImage::White,
                        &Mesh::ColorCube,
                        (Mat4::translation([0., -0.9, 2.5])
                            * matrix::axis_rotation([0., 1., 0.], t as f32))
                        .0,
                        CustomPush {
                            time: [t as f32, t as f32 / 2.0, 0., 0.],
                        },
                        &(),
                        false,
                        None,
                    );
                }

                _ => (),
            }
        });
    }

    const A: &'static str = r###"
    Text A
    "###;

    const B: &'static str = r###"
    Text
    B
    Fix
    Later
    "###;
}
