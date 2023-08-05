#[cfg(feature = "audio")]
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

#[cfg(not(feature = "audio"))]
fn main() {
    eprintln!("This example requires the `audio` feature.");
}

#[cfg(feature = "audio")]
mod internal {
    glace::glace! {
        #[path = "examples/assets"]
        mod assets {
            "volumes.yaml": Single + Serde<std::collections::HashMap<Sfx, f32>>,
        }
    }

    use jamjar::{
        audio::{AudioBytes, AudioState, Mixer, Sound, Track},
        timing::{RealClock, RealTimestamp},
    };

    use assets::prelude::*;

    pub async fn run() {
        let resolution = [512, 256];

        let (window, event_loop) =
            jamjar::windowing::window_and_event_loop("audio", [512, 256]).unwrap();

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

        use glace::BytesAsset;

        let mut mixer = Mixer::new();
        let mut clock = RealClock::new_now();
        let mut time_at_change = RealTimestamp::zero();
        let mut track_toggle = false;

        for &sfx in Sfx::ALL {
            let bytes = sfx.value();
            let volume = Volumes.cached().get(&sfx).copied();
            mixer.load_audio(sfx, bytes, volume);
        }

        event_loop.run(move |event, _, control_flow| {
            use jamjar::windowing::event::{ElementState, Event, WindowEvent};

            match event {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => {
                        *control_flow = jamjar::windowing::event_loop::ControlFlow::Exit
                    }
                    WindowEvent::KeyboardInput { input, .. } => {
                        if let ElementState::Pressed = input.state {
                            if mixer.initialized() {
                                time_at_change = clock.now();
                                track_toggle = !track_toggle;

                                mixer.play_sound(Sound {
                                    key: Sfx::Chime,
                                    volume: 1.0,
                                    speed: 1.0,
                                });
                            } else {
                                mixer.init();
                            }
                        }
                    }
                    _ => (),
                },
                Event::MainEventsCleared => {
                    clock.update();

                    let fade_in = clock.since(time_at_change).min(1.0) as f32;
                    let fade_out = 1.0 - fade_in;
                    let volume0 = if track_toggle { fade_out } else { fade_in };
                    let volume1 = if track_toggle { fade_in } else { fade_out };

                    if mixer.initialized() {
                        mixer.update_state(AudioState {
                            sound_volume: 1.0,
                            track_volume: 1.0,
                            tracks: &[
                                Track {
                                    key: Sfx::Groove,
                                    volume: volume0,
                                    playing: volume0 > 0.0,
                                    looping: true,
                                    feedback_rate: Some(std::time::Duration::from_secs_f64(
                                        60. / 80.,
                                    )),
                                },
                                Track {
                                    key: Sfx::Duelling,
                                    volume: volume1,
                                    playing: volume1 > 0.0,
                                    looping: false,
                                    feedback_rate: None,
                                },
                            ],
                        });
                    }

                    for feedback in mixer.feedback() {
                        jamjar::jprintln!("Got feedback from mixer track {}!!!", feedback);
                    }

                    window.request_redraw();
                }
                Event::RedrawRequested(_) => {}
                _ => (),
            }
        });
    }
}
