use egui::Context;
use egui_winit::State;
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

fn main() {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Meshi GUI")
        .build(&event_loop)
        .expect("failed to build window");

    let mut egui_ctx = Context::default();
    let mut egui_state = State::new(0, &window);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match &event {
            Event::WindowEvent { event, window_id } if *window_id == window.id() => {
                if matches!(event, WindowEvent::CloseRequested) {
                    *control_flow = ControlFlow::Exit;
                    return;
                }

                let _ = egui_state.on_event(&egui_ctx, event);
            }
            Event::MainEventsCleared => {
                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                let raw_input = egui_state.take_egui_input(&window);
                let full_output = egui_ctx.run(raw_input, |_ui| {
                    // Placeholder UI. Add widgets here as the GUI evolves.
                });
                egui_state.handle_platform_output(
                    &window,
                    &egui_ctx,
                    full_output.platform_output,
                );
            }
            _ => {}
        }
    });
}
