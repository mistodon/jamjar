#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn wasm_main() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(log::Level::Debug).unwrap();
    main();
}

fn main() {
    use jamjar::draw::{backend, CanvasConfig};

    let resolution = [512, 256];

    let (window, event_loop) =
        jamjar::windowing::window_and_event_loop("Window Test", resolution).unwrap();

    let src_image = image::load_from_memory(&jamjar::resource!("assets/images/white.png"))
        .unwrap()
        .to_rgba8();

    let mut context =
        jamjar::drawgroovy::DrawContext::<backend::Whatever>::new(&window,
            CanvasConfig::pixel_scaled(resolution),
            src_image)
            .unwrap();

    let mut clock = jamjar::timing::RealClock::new_now();

    event_loop.run(move |event, _, control_flow| {
        use jamjar::windowing::event::{Event, WindowEvent};

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    *control_flow = jamjar::windowing::event_loop::ControlFlow::Exit
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
                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                let t = (clock.secs() % 8.) as f32 / 8.;

                let mut ren = context.start_rendering([0.2, 0., 0.4, 1.]);

                for i in 0..8 {
                    let ii = i as f32 / 8.;
                    let it = (t + ii) % 1.;
                    let a = it * std::f32::consts::TAU;
                    let tt = t * std::f32::consts::TAU;

                    let size = 40. + a.cos() * 10.;

                    let r = 80. + tt.sin() * 20.;

                    let x = 384. + a.cos() * r - size / 2.;
                    let y = 128. + a.sin() * r - size / 2.;

                    let r = it;
                    let g = (it + 0.33) % 1.;
                    let b = (it + 0.66) % 1.;

                    ren.sprite(jamjar::drawgroovy::Sprite {
                        pos: [x, y],
                        size: [size, size],
                        tint: [r, g, b, 1.],
                    });
                }

                for hue in 0..4 {
                    for sat in 0..5 {
                        let v = (4 - sat) as f32 * 0.25;
                        let r = if hue == 0 || hue == 3 { v } else { 0. };
                        let g = if hue == 1 || hue == 3 { v } else { 0. };
                        let b = if hue == 2 || hue == 3 { v } else { 0. };

                        ren.sprite(jamjar::drawgroovy::Sprite {
                            pos: [hue as f32 * 32., sat as f32 * 32.],
                            size: [32., 32.],
                            tint: [r, g, b, 1.],
                        });
                    }
                }

                ren.sprite(jamjar::drawgroovy::Sprite {
                    pos: [500., 128.],
                    size: [50., 50.],
                    tint: [0., 1., 0., 1.],
                });
            }
            _ => (),
        }
    });
}
