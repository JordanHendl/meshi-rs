use std::ffi::c_void;
use std::path::PathBuf;

use glam::{Mat4, Vec2, Vec3, vec2};
use meshi_ffi_structs::event::{Event, EventSource, EventType, KeyCode};
use meshi_graphics::{
    Camera, DB, DBInfo, Display, DisplayInfo, RDBFile, RenderEngine, RenderEngineInfo,
    RendererSelect, TextInfo, TextRenderMode, WindowInfo,
    rdb::terrain::{TerrainChunkArtifact, TerrainMutationOpKind},
};
use meshi_utils::timer::Timer;
use tracing::warn;

use crate::dbgen::{TerrainBrushRequest, TerrainDbgen, TerrainGenerationRequest};
use meshi_graphics::TerrainRenderObject;

const DEFAULT_WINDOW_SIZE: [u32; 2] = [1280, 720];
const DEFAULT_BRUSH_RADIUS: f32 = 8.0;
const DEFAULT_BRUSH_STRENGTH: f32 = 1.0;
const DEFAULT_CHUNK_KEY: &str = "terrain/editor-preview";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputMode {
    Normal,
    EditDbPath,
}

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
    cursor: Vec2,
    mouse_pressed: bool,
    key_presses: Vec<KeyCode>,
    shift_down: bool,
    control_down: bool,
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
    rdb_path_input: String,
    rdb_open: Option<RDBFile>,
    chunk_keys: Vec<String>,
    selected_chunk_index: Option<usize>,
    input_mode: InputMode,
    db_dirty: bool,
    status_note: Option<String>,
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
            cursor: Vec2::ZERO,
            mouse_pressed: false,
            key_presses: Vec::new(),
            shift_down: false,
            control_down: false,
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
            rdb_path_input: String::new(),
            rdb_open: None,
            chunk_keys: Vec::new(),
            selected_chunk_index: None,
            input_mode: InputMode::Normal,
            db_dirty: false,
            status_note: None,
        };

        app.rdb_path_input = app.rdb_path.to_string_lossy().to_string();
        app.register_events();
        app.open_database(app.rdb_path.clone());
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

                if e.source() == EventSource::Key {
                    if e.event_type() == EventType::Pressed {
                        let key = e.key();
                        match key {
                            KeyCode::Shift => state.shift_down = true,
                            KeyCode::Control => state.control_down = true,
                            _ => {}
                        }
                        state.key_presses.push(key);
                    } else if e.event_type() == EventType::Released {
                        let key = e.key();
                        match key {
                            KeyCode::Shift => state.shift_down = false,
                            KeyCode::Control => state.control_down = false,
                            _ => {}
                        }
                    }
                }
                if e.source() == EventSource::Mouse && e.event_type() == EventType::CursorMoved {
                    state.cursor = e.motion2d();
                }
                if e.source() == EventSource::MouseButton {
                    if e.event_type() == EventType::Pressed {
                        state.mouse_pressed = true;
                    }
                }
            }
        }

        let state_ptr = &mut *self.event_state as *mut EventState;
        self.engine.set_event_cb(callback, state_ptr as *mut c_void);
    }

    fn update(&mut self, dt: f32) {
        let key_presses = std::mem::take(&mut self.event_state.key_presses);
        for key in key_presses {
            self.handle_key_press(key);
        }

        if self.needs_refresh {
            self.refresh_terrain();
            self.needs_refresh = false;
        }

        if self.terrain_mode == TerrainMode::Manual {
            self.handle_manual_brush();
        }

        self.engine.update(dt);
    }

    fn refresh_terrain(&mut self) {
        let chunk_key = self.current_chunk_key();
        let request = TerrainGenerationRequest {
            chunk_key: chunk_key.clone(),
            mode: self.terrain_mode.label().to_string(),
        };

        let Some(rdb) = self.rdb_open.as_mut() else {
            self.persistence_error = Some("No database open.".to_string());
            self.update_status_text();
            return;
        };

        if let Some(chunk) = self.dbgen.generate_chunk(&request) {
            if let Err(err) = rdb.upsert(&request.chunk_key, &chunk) {
                warn!(
                    error = %err,
                    entry = %request.chunk_key,
                    "Failed to upsert terrain chunk artifact."
                );
                self.persistence_error = Some(format!("RDB upsert failed: {err}"));
            } else {
                self.persistence_error = None;
                self.db_dirty = true;
                self.ensure_chunk_key(&request.chunk_key);
            }

            self.update_rendered_chunk(request.chunk_key.clone(), chunk);
        }
    }

    fn update_status_text(&mut self) {
        let db_status = if self.rdb_open.is_some() {
            if self.db_dirty {
                "open*"
            } else {
                "open"
            }
        } else {
            "closed"
        };
        let chunk_label = self
            .selected_chunk_index
            .and_then(|index| self.chunk_keys.get(index))
            .cloned()
            .unwrap_or_else(|| DEFAULT_CHUNK_KEY.to_string());
        let mut status = format!(
            "Terrain Editor | Mode: {} | DB: {} ({}) | Chunk: {}",
            self.terrain_mode.label(),
            self.rdb_path.display(),
            db_status,
            chunk_label
        );
        status.push_str(" | Tab: toggle | Ctrl+O: open | Ctrl+W: close | Ctrl+S: save");
        status.push_str(" | Up/Down: select chunk");
        if self.input_mode == InputMode::EditDbPath {
            status.push_str("\nDB Path: ");
            status.push_str(&self.rdb_path_input);
            status.push_str(" (Enter to open, Esc to cancel)");
        }
        if let Some(error) = &self.persistence_error {
            status.push_str(" | ");
            status.push_str(error);
        }
        if let Some(note) = &self.status_note {
            status.push_str(" | ");
            status.push_str(note);
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

    fn handle_manual_brush(&mut self) {
        if !self.event_state.mouse_pressed {
            return;
        }
        self.event_state.mouse_pressed = false;

        let chunk_key = self.current_chunk_key();
        let world_pos = self.cursor_to_world(self.event_state.cursor, &chunk_key);

        let Some(rdb) = self.rdb_open.as_mut() else {
            self.persistence_error = Some("No database open.".to_string());
            self.update_status_text();
            return;
        };

        let request = TerrainBrushRequest {
            chunk_key: chunk_key.clone(),
            mode: self.terrain_mode.label().to_string(),
            world_pos: [world_pos.x, world_pos.y, world_pos.z],
            radius: DEFAULT_BRUSH_RADIUS,
            strength: DEFAULT_BRUSH_STRENGTH,
            tool: TerrainMutationOpKind::SphereAdd,
        };

        match self.dbgen.apply_brush_in_memory(&request, rdb) {
            Ok(artifact) => {
                self.persistence_error = None;
                self.db_dirty = true;
                self.ensure_chunk_key(&request.chunk_key);
                self.update_rendered_chunk(request.chunk_key, artifact);
            }
            Err(err) => {
                warn!(error = %err, "Failed to apply terrain brush.");
                self.persistence_error = Some(format!("Brush apply failed: {err}"));
                self.update_status_text();
            }
        }
    }

    fn cursor_to_world(&self, cursor: Vec2, chunk_key: &str) -> Vec3 {
        let chunk_coords = self.dbgen.chunk_coords_for_key(chunk_key);
        let tile_size = 1.0;
        let tiles_per_chunk = [32_u32, 32_u32];
        let chunk_size_x = tiles_per_chunk[0] as f32 * tile_size;
        let chunk_size_y = tiles_per_chunk[1] as f32 * tile_size;
        let origin_x = chunk_coords[0] as f32 * chunk_size_x;
        let origin_y = chunk_coords[1] as f32 * chunk_size_y;
        let u = (cursor.x / self.window_size.x).clamp(0.0, 1.0);
        let v = (cursor.y / self.window_size.y).clamp(0.0, 1.0);
        Vec3::new(
            origin_x + u * chunk_size_x,
            origin_y + (1.0 - v) * chunk_size_y,
            0.0,
        )
    }

    fn update_rendered_chunk(&mut self, chunk_key: String, artifact: TerrainChunkArtifact) {
        self.update_status_text();
        let render_object = TerrainRenderObject {
            key: chunk_key,
            artifact,
            transform: Mat4::IDENTITY,
        };
        self.terrain_objects.clear();
        self.terrain_objects.push(render_object);
        self.engine
            .set_terrain_render_objects(&self.terrain_objects);
    }

    fn shutdown(self) {
        self.engine.shut_down();
    }

    fn handle_key_press(&mut self, key: KeyCode) {
        if self.input_mode == InputMode::EditDbPath {
            self.handle_db_path_input(key);
            return;
        }

        let control = self.event_state.control_down;

        match key {
            KeyCode::Tab => {
                self.terrain_mode = match self.terrain_mode {
                    TerrainMode::Procedural => TerrainMode::Manual,
                    TerrainMode::Manual => TerrainMode::Procedural,
                };
                self.needs_refresh = true;
                self.update_status_text();
            }
            KeyCode::O if control => {
                self.input_mode = InputMode::EditDbPath;
                self.rdb_path_input = self.rdb_path.to_string_lossy().to_string();
                self.status_note = Some("Editing database path".to_string());
                self.update_status_text();
            }
            KeyCode::W if control => {
                self.close_database();
            }
            KeyCode::S if control => {
                self.commit_database();
            }
            KeyCode::ArrowUp => {
                self.select_prev_chunk();
            }
            KeyCode::ArrowDown => {
                self.select_next_chunk();
            }
            _ => {}
        }
    }

    fn handle_db_path_input(&mut self, key: KeyCode) {
        match key {
            KeyCode::Escape => {
                self.input_mode = InputMode::Normal;
                self.status_note = None;
                self.update_status_text();
            }
            KeyCode::Enter => {
                let path = PathBuf::from(self.rdb_path_input.trim());
                if path.as_os_str().is_empty() {
                    self.persistence_error = Some("Database path cannot be empty.".to_string());
                } else {
                    self.open_database(path);
                }
                self.input_mode = InputMode::Normal;
                self.status_note = None;
                self.update_status_text();
            }
            KeyCode::Backspace => {
                self.rdb_path_input.pop();
                self.update_status_text();
            }
            _ => {
                if let Some(ch) = keycode_to_char(key, self.event_state.shift_down) {
                    self.rdb_path_input.push(ch);
                    self.update_status_text();
                }
            }
        }
    }

    fn open_database(&mut self, path: PathBuf) {
        let rdb = match RDBFile::load(&path) {
            Ok(rdb) => rdb,
            Err(err) => {
                warn!(
                    error = %err,
                    path = %path.display(),
                    "Failed to load terrain RDB; creating new file."
                );
                RDBFile::new()
            }
        };

        self.rdb_open = Some(rdb);
        self.rdb_path = path;
        self.rdb_path_input = self.rdb_path.to_string_lossy().to_string();
        self.db_dirty = false;
        self.persistence_error = None;
        self.rebuild_chunk_keys();
        if self.selected_chunk_index.is_none() {
            self.needs_refresh = true;
        } else {
            self.load_selected_chunk();
        }
        self.update_status_text();
    }

    fn close_database(&mut self) {
        self.rdb_open = None;
        self.chunk_keys.clear();
        self.selected_chunk_index = None;
        self.db_dirty = false;
        self.status_note = Some("Database closed".to_string());
        self.update_status_text();
    }

    fn commit_database(&mut self) {
        let Some(rdb) = self.rdb_open.as_mut() else {
            self.persistence_error = Some("No database open.".to_string());
            self.update_status_text();
            return;
        };

        if let Err(err) = rdb.save(&self.rdb_path) {
            warn!(
                error = %err,
                path = %self.rdb_path.display(),
                "Failed to save terrain RDB."
            );
            self.persistence_error = Some(format!("RDB save failed: {err}"));
        } else {
            self.persistence_error = None;
            self.db_dirty = false;
            self.status_note = Some("Database saved.".to_string());
        }
        self.update_status_text();
    }

    fn rebuild_chunk_keys(&mut self) {
        let Some(rdb) = self.rdb_open.as_mut() else {
            self.chunk_keys.clear();
            self.selected_chunk_index = None;
            return;
        };

        let previous_key = self
            .selected_chunk_index
            .and_then(|index| self.chunk_keys.get(index))
            .cloned();
        let mut keys = Vec::new();
        for entry in rdb.entries() {
            if rdb.fetch::<TerrainChunkArtifact>(&entry.name).is_ok() {
                keys.push(entry.name.clone());
            }
        }
        keys.sort();
        self.chunk_keys = keys;
        if self.chunk_keys.is_empty() {
            self.selected_chunk_index = None;
        } else {
            self.selected_chunk_index = previous_key
                .and_then(|key| self.chunk_keys.iter().position(|k| k == &key))
                .or(Some(0));
        }
    }

    fn ensure_chunk_key(&mut self, chunk_key: &str) {
        if !self.chunk_keys.iter().any(|key| key == chunk_key) {
            self.chunk_keys.push(chunk_key.to_string());
            self.chunk_keys.sort();
            self.selected_chunk_index = self
                .chunk_keys
                .iter()
                .position(|key| key == chunk_key);
        }
    }

    fn select_prev_chunk(&mut self) {
        if self.chunk_keys.is_empty() {
            return;
        }
        let index = self.selected_chunk_index.unwrap_or(0);
        let new_index = if index == 0 {
            self.chunk_keys.len() - 1
        } else {
            index - 1
        };
        self.selected_chunk_index = Some(new_index);
        self.load_selected_chunk();
        self.update_status_text();
    }

    fn select_next_chunk(&mut self) {
        if self.chunk_keys.is_empty() {
            return;
        }
        let index = self.selected_chunk_index.unwrap_or(0);
        let new_index = (index + 1) % self.chunk_keys.len();
        self.selected_chunk_index = Some(new_index);
        self.load_selected_chunk();
        self.update_status_text();
    }

    fn load_selected_chunk(&mut self) {
        let Some(index) = self.selected_chunk_index else {
            return;
        };
        let Some(chunk_key) = self.chunk_keys.get(index).cloned() else {
            return;
        };
        let artifact = {
            let Some(rdb) = self.rdb_open.as_mut() else {
                return;
            };
            match rdb.fetch::<TerrainChunkArtifact>(&chunk_key) {
                Ok(artifact) => Some(artifact),
                Err(err) => {
                    warn!(
                        error = %err,
                        entry = %chunk_key,
                        "Failed to load terrain chunk artifact."
                    );
                    self.persistence_error = Some(format!("Chunk load failed: {err}"));
                    self.update_status_text();
                    None
                }
            }
        };

        if let Some(artifact) = artifact {
            self.persistence_error = None;
            self.update_rendered_chunk(chunk_key, artifact);
        }
    }

    fn current_chunk_key(&self) -> String {
        self.selected_chunk_index
            .and_then(|index| self.chunk_keys.get(index))
            .cloned()
            .unwrap_or_else(|| DEFAULT_CHUNK_KEY.to_string())
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

fn keycode_to_char(key: KeyCode, shift: bool) -> Option<char> {
    use KeyCode::*;
    let ch = match key {
        A => 'a',
        B => 'b',
        C => 'c',
        D => 'd',
        E => 'e',
        F => 'f',
        G => 'g',
        H => 'h',
        I => 'i',
        J => 'j',
        K => 'k',
        L => 'l',
        M => 'm',
        N => 'n',
        O => 'o',
        P => 'p',
        Q => 'q',
        R => 'r',
        S => 's',
        T => 't',
        U => 'u',
        V => 'v',
        W => 'w',
        X => 'x',
        Y => 'y',
        Z => 'z',
        Digit0 => '0',
        Digit1 => '1',
        Digit2 => '2',
        Digit3 => '3',
        Digit4 => '4',
        Digit5 => '5',
        Digit6 => '6',
        Digit7 => '7',
        Digit8 => '8',
        Digit9 => '9',
        Minus => '-',
        Equals => '=',
        LeftBracket => '[',
        RightBracket => ']',
        Backslash => '\\',
        Semicolon => ';',
        Apostrophe => '\'',
        Comma => ',',
        Period => '.',
        Slash => '/',
        GraveAccent => '`',
        Space => ' ',
        _ => return None,
    };

    if shift {
        Some(match ch {
            'a'..='z' => ((ch as u8) - b'a' + b'A') as char,
            '1' => '!',
            '2' => '@',
            '3' => '#',
            '4' => '$',
            '5' => '%',
            '6' => '^',
            '7' => '&',
            '8' => '*',
            '9' => '(',
            '0' => ')',
            '-' => '_',
            '=' => '+',
            '[' => '{',
            ']' => '}',
            '\\' => '|',
            ';' => ':',
            '\'' => '"',
            ',' => '<',
            '.' => '>',
            '/' => '?',
            '`' => '~',
            other => other,
        })
    } else {
        Some(ch)
    }
}
