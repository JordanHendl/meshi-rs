use egui::ColorImage;
use egui_glow::glow::{self, HasContext as _};
use egui_glow::EguiGlow;
use glam::Mat4;
use meshi_graphics::*;
use meshi_graphics::{DisplayInfo, RenderEngine, RenderEngineInfo, RendererSelect, WindowInfo};
use std::rc::Rc;
use std::time::Instant;
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

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
    let window_builder = WindowBuilder::new().with_title("Meshi GUI");
    let windowed_context = glutin::ContextBuilder::new()
        .with_vsync(true)
        .build_windowed(window_builder, &event_loop)
        .expect("failed to build window");
    let windowed_context = unsafe {
        windowed_context
            .make_current()
            .expect("failed to make GL context current")
    };
    let gl = unsafe {
        glow::Context::from_loader_function(|symbol| {
            windowed_context.get_proc_address(symbol) as *const _
        })
    };
    let gl = Rc::new(gl);
    let mut egui_glow = EguiGlow::new(windowed_context.window(), gl.clone());
    let mut last_frame_time = Instant::now();
    let mut preview_texture: Option<egui::TextureHandle> = None;
    let mut preview_ready = false;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match &event {
            Event::WindowEvent { event, window_id }
                if *window_id == windowed_context.window().id() =>
            {
                if matches!(event, WindowEvent::CloseRequested) {
                    egui_glow.destroy();
                    *control_flow = ControlFlow::Exit;
                    return;
                }

                let _ = egui_glow.on_event(event);

                match event {
                    WindowEvent::Resized(physical_size) => {
                        windowed_context.resize(*physical_size);
                    }
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        windowed_context.resize(**new_inner_size);
                    }
                    _ => {}
                }
            }
            Event::MainEventsCleared => {
                windowed_context.window().request_redraw();
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
                                preview_texture = Some(
                                    egui_glow.egui_ctx.load_texture("preview", color_image),
                                );
                            }
                            preview_ready = true;
                        }
                    }
                }

                egui_glow.run(windowed_context.window(), |ctx| {
                    let mut show_scene_hierarchy = true;
                    let mut show_inspector = true;
                    let mut show_assets = true;
                    let mut show_console = true;
                    let mut position = [0.0_f32, 1.0, 2.0];
                    let mut visible = true;

                    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
                        egui::menu::bar(ui, |ui| {
                            ui.menu_button("File", |ui| {
                                ui.button("New Scene");
                                ui.button("Open...");
                                ui.button("Save");
                                ui.separator();
                                ui.button("Preferences");
                                ui.separator();
                                ui.button("Quit");
                            });
                            ui.menu_button("Edit", |ui| {
                                ui.button("Undo");
                                ui.button("Redo");
                                ui.separator();
                                ui.button("Cut");
                                ui.button("Copy");
                                ui.button("Paste");
                            });
                            ui.menu_button("View", |ui| {
                                ui.checkbox(&mut show_scene_hierarchy, "Scene Hierarchy");
                                ui.checkbox(&mut show_inspector, "Inspector");
                                ui.checkbox(&mut show_assets, "Assets");
                                ui.checkbox(&mut show_console, "Console");
                            });
                            ui.menu_button("Build", |ui| {
                                ui.button("Build Project");
                                ui.button("Run");
                            });
                            ui.menu_button("Help", |ui| {
                                ui.button("Documentation");
                                ui.button("Report Issue");
                                ui.separator();
                                ui.button("About Meshi");
                            });
                        });
                    });

                    egui::SidePanel::left("scene_hierarchy")
                        .resizable(true)
                        .show(ctx, |ui| {
                            ui.heading("Scene Hierarchy");
                            ui.separator();
                            ui.label("Root");
                            ui.indent("scene_nodes", |ui| {
                                ui.label("Camera");
                                ui.label("Directional Light");
                                ui.label("Player");
                                ui.label("Environment");
                            });
                        });

                    egui::SidePanel::right("inspector_panel")
                        .resizable(true)
                        .show(ctx, |ui| {
                            ui.heading("Inspector");
                            ui.separator();
                            ui.label("Selected: Player");
                            ui.add_space(8.0);
                            ui.group(|ui| {
                                ui.label("Transform");
                                ui.add(egui::DragValue::new(&mut position[0]).prefix("X "));
                                ui.add(egui::DragValue::new(&mut position[1]).prefix("Y "));
                                ui.add(egui::DragValue::new(&mut position[2]).prefix("Z "));
                            });
                            ui.add_space(8.0);
                            ui.group(|ui| {
                                ui.label("Mesh Renderer");
                                ui.checkbox(&mut visible, "Visible");
                                ui.label("Material: Starter");
                            });
                        });

                    egui::TopBottomPanel::bottom("assets_console_panel")
                        .resizable(true)
                        .show(ctx, |ui| {
                            ui.columns(2, |columns| {
                                columns[0].heading("Assets");
                                columns[0].separator();
                                columns[0].label("Meshes/");
                                columns[0].label("Materials/");
                                columns[0].label("Textures/");
                                columns[0].label("Scripts/");

                                columns[1].heading("Console");
                                columns[1].separator();
                                columns[1].label("[Info] Editor ready.");
                                columns[1].label("[Warn] Lighting bake pending.");
                                columns[1].label("[Error] Missing texture: brick_albedo.png");
                            });
                        });

                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.heading("Viewport");
                        ui.separator();
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

                unsafe {
                    gl.clear_color(0.1, 0.1, 0.1, 1.0);
                    gl.clear(glow::COLOR_BUFFER_BIT);
                }
                egui_glow.paint(windowed_context.window());
                windowed_context
                    .swap_buffers()
                    .expect("failed to swap buffers");
            }
            _ => {}
        }
    });
}
