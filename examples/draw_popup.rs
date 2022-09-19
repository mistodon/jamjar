use jamjar::{
    color,
    draw::{popup::Properties, D},
    math::*,
};

fn main() {
    pollster::block_on(run());
}

async fn run() {
    let resolution = [512, 256];

    let (window, event_loop) =
        jamjar::windowing::window_and_event_loop("Window Test", resolution).unwrap();

    let mut canvas_config = jamjar::draw::CanvasConfig::set_scaled(resolution);
    let mut context = jamjar::draw::popup::DrawContext::new(&window, canvas_config)
        .await
        .unwrap();

    let mut clock = jamjar::timing::RealClock::new_now();

    event_loop.run(move |event, _, control_flow| {
        use jamjar::windowing::event::{ElementState, Event, VirtualKeyCode, WindowEvent};

        context.handle_winit_event(&event);

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
                let mut ren = context.start_rendering([0.2, 0., 0.4, 1.], [0.; 4]);

                // TODO: Base on canvas size
                ren.set_projection(matrix::ortho_projection(2.0, 1.0, -1.0, 1.0).0);

                ren.set_view(Mat4::translation([clock.secs().sin() as f32, 0., 0.]).0);

                ren.raw_opaque(
                    "builtin",
                    "pattern",
                    "quad",
                    Properties {
                        transform: Mat4::scale([0.5, 0.5, 0.5, 1.0]).0,
                        tint: color::WHITE,
                        emission: color::TRANS,
                        color_a: color::TRANS,
                        color_b: color::TRANS,
                    },
                );

                ren.raw_opaque(
                    "builtin",
                    "pattern2",
                    "quad",
                    Properties {
                        transform: Mat4::translation([0., 0., 0.1]).0,
                        tint: color::WHITE,
                        emission: color::TRANS,
                        color_a: color::TRANS,
                        color_b: color::TRANS,
                    },
                );

                ren.raw_trans(
                    0 * D,
                    "builtin",
                    "pattern3",
                    "quad",
                    Properties {
                        transform: Mat4::translation([0., 0., 0.05]).0,
                        tint: color::WHITE,
                        emission: color::TRANS,
                        color_a: color::TRANS,
                        color_b: color::TRANS,
                    },
                );

                // ren.finish_with_text(&mut atlas.fonts, None);
            }
            _ => (),
        }
    });
}
