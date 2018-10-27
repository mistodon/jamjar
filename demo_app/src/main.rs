extern crate winit;

fn main() {
    use winit::{Event, EventsLoop, Window, WindowEvent};

    let mut events_loop = EventsLoop::new();
    let _window = Window::new(&events_loop);

    loop {
        let mut quitting = false;

        events_loop.poll_events(|event| match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => quitting = true,
            _ => (),
        });

        if quitting {
            break;
        }
    }
}
