use jamjar::{
    color,
    draw::{
        cherry::{BuiltinImage, BuiltinShader, BasicPush, LitPush},
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

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    pollster::block_on(run());

    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init().expect("could not initialize logger");
        wasm_bindgen_futures::spawn_local(run());
    }
}

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

async fn run() {
    let resolution = [512, 256];

    let (window, event_loop) =
        jamjar::windowing::window_and_event_loop("Window Test", [512, 256]).unwrap();

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
    let mut context =
        jamjar::draw::cherry::DrawContext::<Image, Mesh, ()>::new(&window, canvas_config, 2048, 1)
            .await
            .unwrap();

    context.load_shader::<CustomPush>((), CUSTOM_SHADER, false);

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
                let mouse_pos = context
                    .window_to_canvas_pos(mouse.position())
                    .unwrap_or([0., 0.]);

                let sf = context.scale_factor();
                let mut ren = context.start_rendering([0.2, 0., 0.4, 1.], mouse_pos, [0.; 4]);
                ren.ortho_2d();

                let f = ren.sprite(&Image::Pattern1, [0., 0.], D, false);
                let f = ren.sprite(&Image::Pattern2, f.tr().into(), D, true);
                ren.sprite(&Image::Pattern3, f.tr().into(), D, false);

                ren.sprite(&Image::Pattern3, mouse_pos, D, false);

                let t = clock.since(start);
                let a = font.layout_wrapped(A, [32., 0.], sf, None, 512. - 32., 1., None);
                let b = font.layout_wrapped(B, [32., 160.], sf, None, 512. - 32., 1., None);
                ren.glyphs(&a, [0., 16.], [0., 1., 1., 1.], 2 * D);
                ren.glyphs_partial(&b, [0., 16.], [1., 1., 1., 1.], 2 * D, t, |ch| match ch {
                    ',' | '/' => 1.,
                    _ => 0.1,
                });

                ren.perspective_3d(1.0);
                ren.draw(
                    BuiltinShader::Basic,
                    BuiltinImage::White,
                    &Mesh::Cube,
                    (Mat4::translation([0., -0.7, 2.])
                        * matrix::axis_rotation([0., 1., 0.], t as f32))
                    .0,
                    BasicPush::default(),
                    false,
                    None,
                );
                ren.draw(
                    BuiltinShader::Lit,
                    BuiltinImage::White,
                    &Mesh::Cube,
                    (Mat4::translation([-1.5, -0.7, 2.])
                        * matrix::axis_rotation([0., 1., 0.], t as f32))
                    .0,
                    LitPush {
                        light_dir: vec4(1., -1., 1., 0.).norm_zero().0,
                        light_col: color::WHITE,
                        ..Default::default()
                    },
                    false,
                    None,
                );
                ren.draw(
                    BuiltinShader::Simple,
                    BuiltinImage::White,
                    &Mesh::Cube,
                    (Mat4::translation([1.5, -0.7, 2.])
                        * matrix::axis_rotation([0., 1., 0.], t as f32))
                    .0,
                    (),
                    false,
                    None,
                );
                ren.draw(
                    &(),
                    BuiltinImage::White,
                    &Mesh::Cube,
                    (Mat4::translation([0., -0.9, 2.5])
                        * matrix::axis_rotation([0., 1., 0.], t as f32))
                    .0,
                    CustomPush {
                        time: [t as f32, t as f32 / 2.0, 0., 0.],
                    },
                    false,
                    None,
                );
            }

            _ => (),
        }
    });
}

const A: &'static str = r###"
Well, he collapsed with Stevens-Johnson Syndrome on the E.R. floor
Panic attacked, anaphylactic and ataxic
Well the way he spun his butterfly risked all six his phalanges
Roman candles at both ends in his synapses
And the method with which he recycled his humors
Trojan Horse’d his blood-brain barrier and raised the LD-50, yes, yes
And through flight-or-fight revelation shame, the Black Box Warrior
He skipped this town and headed straight down history
"###;

const B: &'static str = r###"
Shields himself from reason in a Kevlar baby-blue Tuxedo
Quilted from the finest fibers, flesh, and fiberglass, and flowers
His ego a mosquito, evil incarnate/good incognito
Pops placebos for libido, screaming "bless the torpedoes"
"###;
