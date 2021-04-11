#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn wasm_main() {
    main();
}

fn main() {
    use jamjar_examples::gen::{data::VOLUMES, Audio};

    use jamjar::{
        audio::{AudioState, Mixer, Sound, Track},
        resource,
        timing::{RealClock, RealTimestamp},
    };

    jamjar::logging::init_logging();

    let (window, event_loop) =
        jamjar::windowing::window_and_event_loop("Window Test", [512, 256]).unwrap();

    let audio_library = jamjar::resources::map_audio_resources(
        jamjar_examples::gen::Audio::ALL,
        &jamjar::resource_list!("assets/audio"),
    );

    let mut mixer = Mixer::new(audio_library, Some(VOLUMES.clone()));
    let mut clock = RealClock::new_now();
    let mut time_at_change = RealTimestamp::zero();
    let mut track_toggle = false;

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
                                key: Audio::Chime,
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
                                key: Audio::Groove,
                                volume: volume0,
                                playing: volume0 > 0.0,
                                looping: true,
                                feedback_rate: Some(std::time::Duration::from_secs_f64(60. / 80.)),
                            },
                            Track {
                                key: Audio::Duelling,
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
