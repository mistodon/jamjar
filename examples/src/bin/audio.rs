use jamjar_examples::gen::{data::VOLUMES, Audio};

use jamjar::{
    audio::{AudioState, Mixer, Sound, Track},
    resource,
};

fn get_time() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

fn main() {
    let (window, event_loop) =
        jamjar::windowing::window_and_event_loop("Window Test", [512, 256]).unwrap();

    let audio_library = jamjar::resources::map_audio_resources(
        jamjar_examples::gen::Audio::ALL,
        &jamjar::resource_list!("assets/audio"),
    );

    let mut mixer = Mixer::new(audio_library, Some(VOLUMES.clone()));
    let mut mock_time = get_time();
    let mut time_at_change = 0.0;
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
                            time_at_change = mock_time;
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
                mock_time = get_time();

                let fade_in = (mock_time - time_at_change).min(1.0) as f32;
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
                            },
                            Track {
                                key: Audio::Duelling,
                                volume: volume1,
                                playing: volume1 > 0.0,
                            },
                        ],
                    });
                }

                window.request_redraw();
            }
            Event::RedrawRequested(_) => {}
            _ => (),
        }
    });
}
