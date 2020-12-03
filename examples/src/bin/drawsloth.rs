fn main() {
    let (window, event_loop) =
        jamjar::windowing::window_and_event_loop("Window Test", [512, 256]).unwrap();

    let mut context = jamjar::drawsloth::DrawContext::<gfx_backend_gl::Backend>::new(&window).unwrap();

    event_loop.run(move |event, _, control_flow| {
        use jamjar::windowing::event::{Event, WindowEvent};

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = jamjar::windowing::event_loop::ControlFlow::Exit,
                _ => (),
            },
            Event::MainEventsCleared => {
                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                context.start_rendering([1., 0., 0., 1.]);
            }
            _ => (),
        }
    });
}
