use jamjar::{
    color,
    draw::{
        cherry::{
            BasicPush, BuiltinImage, BuiltinShader, LitPush, PushFlags, ShaderConf, ShaderFlags,
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

#[derive(Clone, PartialEq, Eq, Hash)]
enum Shader {
    Shadowed,
    ShadowFront,
    ShadowBack,
    PointLight,
    DirLight,
}

#[repr(C)]
struct ShadowPush {
    light_dir: [f32; 4],
}

#[repr(C)]
struct ShadowedPush {
    tint: [f32; 4],
    emission: [f32; 4],
    ambient: [f32; 4],
}

#[repr(C)]
struct PointLightPush {
    tint: [f32; 4],
    pos: [f32; 4],
    light: [f32; 4],
}

#[repr(C)]
struct DirLightPush {
    tint: [f32; 4],
    light_dirs: [[f32; 4]; 4],
    light_cols_t: [[f32; 4]; 4],
}

const SHADOWED_SHADER: &str = "
struct Push {
    transform: mat4x4<f32>,
    uv_offset_scale: vec4<f32>,
    tint: vec4<f32>,
    emission: vec4<f32>,
    ambient: vec4<f32>,
};

var<push_constant> push: Push;

@vertex
fn vertex_main(vertex: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = push.transform * vertex.position;
    output.normal = vertex.normal.xyz;
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

    return (base_color * vertex.color * push.tint) * push.ambient + push.emission;
}
";

const POINTLIGHT_SHADER: &str = "
struct Push {
    transform: mat4x4<f32>,
    modelmatrix: mat4x4<f32>,
    uv_offset_scale: vec4<f32>,
    tint: vec4<f32>,
    light_pos: vec4<f32>,
    light_color: vec4<f32>,
};

var<push_constant> push: Push;

@vertex
fn vertex_main(vertex: VertexInput) -> VertexOutput {
    var world_pos_w = push.modelmatrix * vertex.position;
    var world_pos = world_pos_w / world_pos_w.w;
    var to_light = push.light_pos - world_pos;
    var to_light_n = vec4(normalize(to_light.xyz), 0.0);

    var output: VertexOutput;
    output.position = push.transform * vertex.position;
    output.normal = vertex.normal.xyz;
    output.uv = vertex.uv.xy * (push.uv_offset_scale.zw) + push.uv_offset_scale.xy;
    output.color = vertex.color;
    output.custom_a = to_light_n;
    output.custom_b = to_light;
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

    var albedo = base_color * vertex.color * push.tint;
    var lightDot = max(0.0, dot(vertex.custom_a.xyz, vertex.normal));
    var atten = min(1.0, 1.0 / dot(vertex.custom_b, vertex.custom_b));
    return vec4((albedo * (push.light_color * lightDot * atten)).rgb, 1.0);
}
";

const DIRLIGHT_SHADER: &str = "
struct Push {
    transform: mat4x4<f32>,
    uv_offset_scale: vec4<f32>,
    tint: vec4<f32>,
    light_dirs: mat4x4<f32>,
    light_cols_t: mat4x4<f32>,
};

var<push_constant> push: Push;

@vertex
fn vertex_main(vertex: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = push.transform * vertex.position;
    output.normal = vertex.normal.xyz;
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

    var albedo = base_color * vertex.color * push.tint;
    var light_con = push.light_cols_t * (push.light_dirs * vec4(vertex.normal, 0.0));
    return vec4((albedo * light_con).rgb, 1.0);
}
";

const SHADOWVOL_SHADER: &str = "
struct Push {
    transform: mat4x4<f32>,
    model_matrix: mat4x4<f32>,
    light_dir: vec4<f32>,
};

var<push_constant> push: Push;

@vertex
fn vertex_main(vertex: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    var shadow_offset = push.light_dir * step(0.2, dot(vertex.normal.xyz, push.light_dir.xyz)) * 1000.0;
    output.position = push.transform * (vertex.position + shadow_offset) - vec4(vertex.normal.xyz * 0.001, 0.0);
    return output;
}

@fragment
fn fragment_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    return vec4(0.0, 0.0, 0.0, 1.0);
}
";

async fn run() {
    let resolution = [1280, 720];

    let (window, event_loop) =
        jamjar::windowing::window_and_event_loop("Window Test", resolution).unwrap();

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

    context.load_shader::<ShadowedPush>(Shader::Shadowed, SHADOWED_SHADER, ShaderConf::default());

    context.load_shader::<ShadowPush>(
        Shader::ShadowFront,
        SHADOWVOL_SHADER,
        ShaderConf {
            phase: 1,
            shader_flags: ShaderFlags::NO_COLOR_WRITE
                | ShaderFlags::NO_DEPTH_WRITE
                | ShaderFlags::STENCIL_ADD,
            push_flags: PushFlags::TRANSFORM | PushFlags::MODEL_MATRIX,
        },
    );
    // context.load_shader::<ShadowPush>(Shader::ShadowFront, SHADOWVOL_SHADER, ShaderConf {
    //     phase: 0,
    //     shader_flags: ShaderFlags::default(),
    //     push_flags: PushFlags::TRANSFORM | PushFlags::MODEL_MATRIX,
    // });

    context.load_shader::<ShadowPush>(
        Shader::ShadowBack,
        SHADOWVOL_SHADER,
        ShaderConf {
            phase: 2,
            shader_flags: ShaderFlags::NO_COLOR_WRITE
                | ShaderFlags::NO_DEPTH_WRITE
                | ShaderFlags::STENCIL_SUB
                | ShaderFlags::BACK_FACE_ONLY,
            push_flags: PushFlags::TRANSFORM | PushFlags::MODEL_MATRIX,
        },
    );
    context.load_shader::<PointLightPush>(
        Shader::PointLight,
        POINTLIGHT_SHADER,
        ShaderConf {
            phase: 3,
            shader_flags: ShaderFlags::NO_DEPTH_WRITE | ShaderFlags::BLEND_ADD,
            push_flags: PushFlags::default() | PushFlags::MODEL_MATRIX,
        },
    );
    context.load_shader::<DirLightPush>(
        Shader::DirLight,
        DIRLIGHT_SHADER,
        ShaderConf {
            phase: 3,
            shader_flags: ShaderFlags::NO_DEPTH_WRITE
                | ShaderFlags::BLEND_ADD
                | ShaderFlags::STENCIL_HIDES,
            push_flags: PushFlags::default(),
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
                let mouse_pos = context
                    .window_to_canvas_pos(mouse.position())
                    .unwrap_or([0., 0.]);

                let sf = context.scale_factor();

                let t0 = (clock.secs() % 9999.) as f32;
                let t1 = (t0 * 1.43243) + 3.14159;

                let lights = &[
                    ([t0.cos(), 0., t0.sin() + 2., 1.], [1., 0., 0., 0.]),
                    ([t1.cos(), 0., t1.sin() + 2., 1.], [0., 1., 0., 0.]),
                ];

                let key_light_dir = Vec4::new([0.2, 1., -0.35, 0.]).norm();
                let key_shadow_dir = (-key_light_dir).0;

                let light_dirs = [
                    [-1., 0., 0., 0.],
                    key_light_dir.0,
                    [0., 0., -1., 0.],
                    [0., 0., 0., 0.],
                ];
                let light_cols_t = Mat4::new([
                    [0.3, 0.3, 0.0, 0.],
                    [0.3, 0.0, 0.3, 0.],
                    [0.0, 0.3, 0.3, 0.],
                    [0.0, 0.0, 0.0, 0.],
                ])
                .transpose()
                .0;

                let mut ren = context.start_rendering([0.2, 0.6, 1., 1.], mouse_pos, [0.; 4]);

                ren.perspective_3d(1.0);
                ren.set_view(
                    (Mat4::translation([0., -0.5, 0.]) * matrix::axis_rotation([1., 0., 0.], -0.5))
                        .0,
                );

                for &(pos, color) in lights {
                    ren.draw(
                        BuiltinShader::Basic,
                        BuiltinImage::White,
                        &Mesh::Sphere,
                        (Mat4::translation([pos[0], pos[1], pos[2]])
                            * Mat4::scale([0.1, 0.1, 0.1, 1.]))
                        .0,
                        BasicPush {
                            tint: [color[0], color[1], color[2], 1.0],
                            emission: [0.; 4],
                        },
                        false,
                        None,
                    );
                }

                let sphere_trans = (Mat4::translation([0., -0.2, 2.])).0;
                let sphere_2_trans = (Mat4::translation([t0.cos() * 2.0, 0.4, 1.4])).0;
                let cube_trans =
                    (Mat4::translation([0., -2., 2.]) * Mat4::scale([9.0, 1.0, 9.0, 1.0])).0;

                let lit_objects = [
                    (Mesh::Sphere, sphere_trans),
                    (Mesh::Sphere, sphere_2_trans),
                    (Mesh::Cube, cube_trans),
                ];

                for (mesh, trans) in lit_objects {
                    ren.draw(
                        &Shader::Shadowed,
                        BuiltinImage::White,
                        &mesh,
                        trans,
                        ShadowedPush {
                            tint: [1., 1., 1., 1.],
                            emission: [0., 0., 0., 0.],
                            ambient: [0.1, 0.1, 0.15, 1.],
                        },
                        false,
                        None,
                    );

                    ren.draw(
                        &Shader::ShadowFront,
                        BuiltinImage::White,
                        &mesh,
                        trans,
                        ShadowPush {
                            light_dir: key_shadow_dir,
                        },
                        false,
                        None,
                    );
                    ren.draw(
                        &Shader::ShadowBack,
                        BuiltinImage::White,
                        &mesh,
                        trans,
                        ShadowPush {
                            light_dir: key_shadow_dir,
                        },
                        false,
                        None,
                    );

                    ren.draw(
                        &Shader::DirLight,
                        BuiltinImage::White,
                        &mesh,
                        trans,
                        DirLightPush {
                            tint: [1., 1., 1., 1.],
                            light_dirs,
                            light_cols_t,
                        },
                        false,
                        None,
                    );

                    for &(pos, color) in lights {
                        ren.draw(
                            &Shader::PointLight,
                            BuiltinImage::White,
                            &mesh,
                            trans,
                            PointLightPush {
                                tint: [1., 1., 1., 1.],
                                pos: pos,
                                light: color,
                            },
                            false,
                            None,
                        );
                    }
                }
            }

            _ => (),
        }
    });
}