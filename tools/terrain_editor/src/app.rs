use std::ffi::c_void;
use std::path::PathBuf;

use glam::{Mat4, Vec2, Vec3, vec2};
use meshi_ffi_structs::event::{Event, EventSource, EventType, KeyCode};
use meshi_graphics::{
    Camera, DB, DBInfo, Display, DisplayInfo, RDBFile, RenderEngine, RenderEngineInfo,
    RendererSelect, TextInfo, TextRenderMode, WindowInfo,
};
use meshi_utils::timer::Timer;
use tracing::warn;

use crate::dbgen::{TerrainDbgen, TerrainGenerationRequest};
use meshi_graphics::TerrainRenderObject;

const DEFAULT_WINDOW_SIZE: [u32; 2] = [1280, 720];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerrainMode {
    Procedural,
    Manual,
}

impl TerrainMode {
    fn label(self) -> &'static str {
        match self {
            TerrainMode::Procedural => "Procedural",
            TerrainMode::Manual => "Manual",
        }
    }
}

struct EventState {
    running: bool,
    toggle_mode: bool,
}

pub struct TerrainEditorApp {
    engine: RenderEngine,
    db: Box<DB>,
    display: dashi::Handle<Display>,
    camera: dashi::Handle<Camera>,
    status_text: dashi::Handle<meshi_graphics::TextObject>,
    window_size: Vec2,
    terrain_mode: TerrainMode,
    terrain_objects: Vec<TerrainRenderObject>,
    dbgen: TerrainDbgen,
    event_state: Box<EventState>,
    needs_refresh: bool,
    persistence_error: Option<String>,
    rdb_path: PathBuf,
}

impl TerrainEditorApp {
    fn new(title: &str, window_size: [u32; 2]) -> Self {
        let mut engine = RenderEngine::new(&RenderEngineInfo {
            headless: false,
            canvas_extent: Some(window_size),
            renderer: RendererSelect::Deferred,
            sample_count: None,
            skybox_cubemap_entry: None,
            debug_mode: false,
        })
        .expect("Failed to initialize render engine");

        let base_dir = "";
        let mut db = Box::new(
            DB::new(&DBInfo {
                base_dir,
                layout_file: None,
                pooled_geometry_uploads: false,
            })
            .expect("Unable to create database"),
        );
        let rdb_path = if base_dir.is_empty() {
            PathBuf::from("terrain.rdb")
        } else {
            PathBuf::from(base_dir).join("terrain.rdb")
        };

        engine.initialize_database(&mut db);

        let display = engine.register_window_display(DisplayInfo {
            vsync: false,
            window: WindowInfo {
                title: title.to_string(),
                size: window_size,
                resizable: true,
            },
            ..Default::default()
        });

        let camera = engine.register_camera(&Mat4::IDENTITY);
        engine.attach_camera_to_display(display, camera);
        engine.set_camera_perspective(
            camera,
            60f32.to_radians(),
            window_size[0] as f32,
            window_size[1] as f32,
            0.1,
            50000.0,
        );

        let window_size_vec = Vec2::new(window_size[0] as f32, window_size[1] as f32);
        let render_mode = text_render_mode(&db);
        let status_text = engine.register_text(&TextInfo {
            text: "Initializing terrain editor...".to_string(),
            position: vec2(20.0, window_size_vec.y - 40.0),
            color: glam::Vec4::new(0.85, 0.9, 1.0, 1.0),
            scale: 1.1,
            render_mode,
        });

        let event_state = Box::new(EventState {
            running: true,
            toggle_mode: false,
        });

        let mut app = Self {
            engine,
            db,
            display,
            camera,
            status_text,
            window_size: window_size_vec,
            terrain_mode: TerrainMode::Procedural,
            terrain_objects: Vec::new(),
            dbgen: TerrainDbgen::new(0),
            event_state,
            needs_refresh: true,
            persistence_error: None,
            rdb_path,
        };

        app.register_events();
        app.update_status_text();
        app
    }

    fn register_events(&mut self) {
        extern "C" fn callback(event: *mut Event, data: *mut c_void) {
            unsafe {
                let e = &mut (*event);
                let state = &mut *(data as *mut EventState);

                if e.source() == EventSource::Window && e.event_type() == EventType::Quit {
                    state.running = false;
                }

                if e.source() == EventSource::Key && e.event_type() == EventType::Pressed {
                    if e.key() == KeyCode::Tab {
                        state.toggle_mode = true;
                    }
                }
            }
        }

        let state_ptr = &mut *self.event_state as *mut EventState;
        self.engine
            .set_event_cb(callback, state_ptr as *mut c_void);
    }

    fn update(&mut self, dt: f32) {
        if self.event_state.toggle_mode {
            self.event_state.toggle_mode = false;
            self.terrain_mode = match self.terrain_mode {
                TerrainMode::Procedural => TerrainMode::Manual,
                TerrainMode::Manual => TerrainMode::Procedural,
            };
            self.needs_refresh = true;
            self.update_status_text();
        }

        if self.needs_refresh {
            self.refresh_terrain();
            self.needs_refresh = false;
        }

        self.engine.update(dt);
    }

    fn refresh_terrain(&mut self) {
        let request = TerrainGenerationRequest {
            chunk_key: "terrain/editor-preview".to_string(),
            mode: self.terrain_mode.label().to_string(),
        };

        if let Some(chunk) = self.dbgen.generate_chunk(&request) {
            let mut persistence_error = None;
            let mut rdb = match RDBFile::load(&self.rdb_path) {
                Ok(rdb) => rdb,
                Err(err) => {
                    warn!(
                        error = %err,
                        path = %self.rdb_path.display(),
                        "Failed to load terrain RDB; creating new file."
                    );
                    RDBFile::new()
                }
            };

            if let Err(err) = rdb.upsert(&request.chunk_key, &chunk) {
                warn!(
                    error = %err,
                    entry = %request.chunk_key,
                    "Failed to upsert terrain chunk artifact."
                );
                persistence_error = Some(format!("RDB upsert failed: {err}"));
            } else if let Err(err) = rdb.save(&self.rdb_path) {
                warn!(
                    error = %err,
                    path = %self.rdb_path.display(),
                    "Failed to save terrain RDB."
                );
                persistence_error = Some(format!("RDB save failed: {err}"));
            }

            self.persistence_error = persistence_error;
            self.update_status_text();
            let render_object = TerrainRenderObject {
                key: request.chunk_key.clone(),
                artifact: chunk,
                transform: Mat4::IDENTITY,
            };
            self.terrain_objects.clear();
            self.terrain_objects.push(render_object);
            self.engine.set_terrain_render_objects(&self.terrain_objects);
        }
    }

    fn update_status_text(&mut self) {
        let mut status = format!(
            "Terrain Editor | Mode: {} | DBGen: {} | Tab to toggle",
            self.terrain_mode.label(),
            self.dbgen.status()
        );
        if let Some(error) = &self.persistence_error {
            status.push_str(" | ");
            status.push_str(error);
        }
        self.engine.set_text(self.status_text, &status);
        self.engine.set_text_info(
            self.status_text,
            &TextInfo {
                text: status,
                position: vec2(20.0, self.window_size.y - 40.0),
                color: glam::Vec4::new(0.85, 0.9, 1.0, 1.0),
                scale: 1.1,
                render_mode: text_render_mode(&self.db),
            },
        );
    }

    fn shutdown(mut self) {
        self.engine.shut_down();
    }
}

pub fn run() {
    let mut app = TerrainEditorApp::new("Terrain Editor", DEFAULT_WINDOW_SIZE);

    let mut timer = Timer::new();
    timer.start();
    let mut last_time = timer.elapsed_seconds_f32();

    while app.event_state.running {
        let now = timer.elapsed_seconds_f32();
        let dt = (now - last_time).min(1.0 / 30.0);

        app.update(dt);
        last_time = now;
    }

    app.shutdown();
}

fn text_render_mode(db: &DB) -> TextRenderMode {
    let sdf_font = db.enumerate_sdf_fonts().into_iter().next();
    sdf_font
        .map(|font| TextRenderMode::Sdf { font })
        .unwrap_or(TextRenderMode::Plain)
}
