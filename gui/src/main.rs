use egui::{ColorImage, Context};
use egui_winit::State;
use glam::Mat4;
use meshi_graphics::*;
use meshi_graphics::{DisplayInfo, RenderEngine, RenderEngineInfo, RendererSelect, WindowInfo};
use std::time::Instant;
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

mod panels;

fn main() {
    let preview_extent = [512, 512];
    let mut render_engine = RenderEngine::new(&RenderEngineInfo {
        headless: true,
        canvas_extent: Some(preview_extent),
        renderer: RendererSelect::Deferred,
        sample_count: None,
    })
    .expect("failed to create render engine");

    // Default database. Given bogus directory so all we have to work with is the default
    // models/materials...
    let mut db = DB::new(&DBInfo {
        base_dir: "",
        layout_file: None,
        pooled_geometry_uploads: false,
    })
    .expect("Unable to create database");

    render_engine.initialize_database(&mut db);

    let display = render_engine.register_cpu_display(DisplayInfo {
        vsync: false,
        window: WindowInfo {
            title: "Meshi Preview".to_string(),
            size: preview_extent,
            resizable: false,
        },
        ..Default::default()
    });
    let camera = render_engine.register_camera(&Mat4::IDENTITY);
    render_engine.attach_camera_to_display(display, camera);

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Meshi GUI")
        .build(&event_loop)
        .expect("failed to build window");

    let mut egui_ctx = Context::default();
    let mut egui_state = State::new(0, &window);
    let mut last_frame_time = Instant::now();
    let mut preview_texture: Option<egui::TextureHandle> = None;
    let mut preview_ready = false;
    let script_provider = panels::scripts::DummyScriptProvider;

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
                let now = Instant::now();
                let delta_time = (now - last_frame_time).as_secs_f32();
                last_frame_time = now;
                render_engine.update(delta_time);

                preview_ready = false;
                if display.valid() {
                    if let Some(frame) = render_engine.frame_dump(display) {
                        if !frame.pixels.is_null()
                            && frame.width > 0
                            && frame.height > 0
                            && (frame.width as usize)
                                .saturating_mul(frame.height as usize)
                                .saturating_mul(4)
                                > 0
                        {
                            let pixel_count = frame.width as usize * frame.height as usize * 4;
                            let bgra_pixels =
                                unsafe { std::slice::from_raw_parts(frame.pixels, pixel_count) };
                            let mut rgba_pixels = Vec::with_capacity(pixel_count);
                            for chunk in bgra_pixels.chunks_exact(4) {
                                rgba_pixels.push(chunk[2]);
                                rgba_pixels.push(chunk[1]);
                                rgba_pixels.push(chunk[0]);
                                rgba_pixels.push(chunk[3]);
                            }
                            let color_image = ColorImage::from_rgba_unmultiplied(
                                [frame.width as usize, frame.height as usize],
                                &rgba_pixels,
                            );
                            if let Some(texture) = preview_texture.as_mut() {
                                texture.set(color_image);
                            } else {
                                preview_texture =
                                    Some(egui_ctx.load_texture("preview", color_image));
                            }
                            preview_ready = true;
                        }
                    }
                }

                let raw_input = egui_state.take_egui_input(&window);
                let full_output = egui_ctx.run(raw_input, |ui| {
                    egui::SidePanel::left("scripts_panel").show(&egui_ctx, |ui| {
                        panels::scripts::render_scripts_panel(ui, &script_provider);
                    });

                    egui::CentralPanel::default().show(ui, |ui| {
                        ui.heading("Preview");
                        if preview_ready {
                            if let Some(texture) = preview_texture.as_ref() {
                                let preview_size = egui::Vec2::new(
                                    preview_extent[0] as f32,
                                    preview_extent[1] as f32,
                                );
                                ui.image(texture.id(), preview_size);
                            }
                        } else {
                            ui.label("Renderer not ready.");
                        }
                    });
                });
                egui_state.handle_platform_output(&window, &egui_ctx, full_output.platform_output);
            }
            _ => {}
        }
    });
}
