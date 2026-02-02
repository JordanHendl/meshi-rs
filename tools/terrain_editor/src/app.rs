use std::collections::VecDeque;
use std::ffi::c_void;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use glam::{vec2, Mat4, Vec2, Vec3};
use meshi_ffi_structs::event::{Event, EventSource, EventType, KeyCode};
use meshi_graphics::gui::GuiContext;
use meshi_graphics::{
    rdb::primitives::Vertex,
    rdb::terrain::{
        project_settings_entry, TerrainChunk, TerrainChunkArtifact, TerrainMutationOpKind,
        TerrainProjectSettings,
    },
    Camera, DBInfo, Display, DisplayInfo, EnvironmentLightingSettings, RDBFile, RenderEngine,
    RenderEngineInfo, RendererSelect, SkyFrameSettings, SkyboxFrameSettings, TextInfo,
    TextRenderMode, WindowInfo, DB,
};
use meshi_utils::timer::Timer;
use rfd::FileDialog;
use tracing::warn;

use crate::camera::{CameraController, CameraInput};
use crate::dbgen::{TerrainBrushRequest, TerrainDbgen, TerrainGenerationRequest};
use crate::ui::{
    BrushTool, FocusedInput, TerrainEditorUi, TerrainEditorUiData, TerrainEditorUiInput,
    MENU_ACTION_APPLY_BRUSH, MENU_ACTION_CLOSE_RDB, MENU_ACTION_EARTH_PRESET, MENU_ACTION_GENERATE,
    MENU_ACTION_NEW_RDB, MENU_ACTION_OPEN_RDB, MENU_ACTION_SAVE_RDB, MENU_ACTION_SET_MANUAL,
    MENU_ACTION_SET_PROCEDURAL, MENU_ACTION_SHOW_WORKFLOW, MENU_ACTION_TOGGLE_BRUSH_PANEL,
    MENU_ACTION_TOGGLE_CHUNK_PANEL, MENU_ACTION_TOGGLE_DB_PANEL,
    MENU_ACTION_TOGGLE_GENERATION_PANEL, MENU_ACTION_TOGGLE_WORKFLOW_PANEL,
};
use meshi_graphics::TerrainChunkRef;

const DEFAULT_WINDOW_SIZE: [u32; 2] = [1280, 720];
const DEFAULT_BRUSH_RADIUS: f32 = 8.0;
const DEFAULT_BRUSH_STRENGTH: f32 = 1.0;
const EARTHLIKE_SEED: u64 = 1337;
const EARTHLIKE_FREQUENCY: f32 = 0.0065;
const EARTHLIKE_AMPLITUDE: f32 = 120.0;
const EARTHLIKE_BIOME_FREQUENCY: f32 = 0.003;
const EARTHLIKE_ALGORITHM: &str = "ridge-noise";
const DEFAULT_CHUNK_KEY: &str = "terrain/chunk_0_0";
const DEFAULT_WORLD_CHUNKS: [u32; 2] = [32, 32];
const DEFAULT_VERTEX_RESOLUTION: u32 = 16;
const DEFAULT_VERTEX_COLOR: [f32; 3] = [0.8, 0.2, 0.2];
const GENERATION_CHUNKS_PER_FRAME: usize = 4;

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
    last_cursor: Option<Vec2>,
    window_resized: Option<Vec2>,
    mouse_pressed: bool,
    mouse_down: bool,
    mouse_released: bool,
    key_presses: Vec<KeyCode>,
    shift_down: bool,
    control_down: bool,
    move_forward: bool,
    move_back: bool,
    move_left: bool,
    move_right: bool,
    move_up: bool,
    move_down: bool,
    move_active: bool,
}

#[derive(Debug, Clone, Copy)]
struct PanelVisibility {
    db: bool,
    chunks: bool,
    generation: bool,
    brush: bool,
    workflow: bool,
}

pub struct TerrainEditorApp {
    engine: RenderEngine,
    db: Box<DB>,
    display: dashi::Handle<Display>,
    camera: dashi::Handle<Camera>,
    camera_controller: CameraController,
    status_text: dashi::Handle<meshi_graphics::TextObject>,
    window_size: Vec2,
    terrain_mode: TerrainMode,
    terrain_chunks: Vec<TerrainChunkRef>,
    dbgen: TerrainDbgen,
    ui: TerrainEditorUi,
    event_state: Box<EventState>,
    needs_refresh: bool,
    persistence_error: Option<String>,
    rdb_path: PathBuf,
    rdb_path_input: String,
    rdb_open: Option<RDBFile>,
    chunk_keys: Vec<String>,
    selected_chunk_index: Option<usize>,
    db_dirty: bool,
    status_note: Option<String>,
    focused_input: Option<FocusedInput>,
    generation_seed: u64,
    generation_lod: u8,
    generation_graph_id: String,
    generator_frequency: f32,
    generator_amplitude: f32,
    generator_biome_frequency: f32,
    generator_algorithm: String,
    seed_input: String,
    lod_input: String,
    graph_id_input: String,
    world_chunks: [u32; 2],
    world_chunks_x_input: String,
    world_chunks_y_input: String,
    vertex_resolution: u32,
    vertex_resolution_input: String,
    brush_radius: f32,
    brush_strength: f32,
    brush_tool: BrushTool,
    vertex_paint_color: [f32; 3],
    ui_hovered: bool,
    last_world_cursor: Vec2,
    panel_visibility: PanelVisibility,
    pending_generation: VecDeque<[u32; 2]>,
    pending_request: Option<TerrainGenerationRequest>,
    pending_generation_total: usize,
    pending_generation_processed: usize,
    pending_failures: Vec<String>,
    pending_last_chunk_entry: Option<String>,
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
            shadow_cascades: Default::default(),
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
        let camera_controller = CameraController::new(Vec3::new(0.0, 22.0, 32.0));
        let camera_transform = camera_controller.transform();
        engine.set_camera_transform(camera, &camera_transform);
        engine.set_skybox_settings(SkyboxFrameSettings {
            intensity: 0.95,
            use_procedural_cubemap: true,
            update_interval_frames: 1,
            ..Default::default()
        });
        engine.set_environment_lighting(EnvironmentLightingSettings {
            sky: SkyFrameSettings {
                enabled: true,
                sun_color: Vec3::new(1.0, 0.96, 0.9),
                sun_intensity: 6.0,
                moon_color: Vec3::new(0.6, 0.7, 1.0),
                moon_intensity: 0.25,
                auto_sun_enabled: true,
                timer_speed: 1.0 / 3600.0,
                current_time_of_day: 12.0,
                ..Default::default()
            },
            sun_light_intensity: 3.0,
            moon_light_intensity: 0.4,
        });

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
            last_cursor: None,
            window_resized: None,
            mouse_pressed: false,
            mouse_down: false,
            mouse_released: false,
            key_presses: Vec::new(),
            shift_down: false,
            control_down: false,
            move_forward: false,
            move_back: false,
            move_left: false,
            move_right: false,
            move_up: false,
            move_down: false,
            move_active: false,
        });

        let generation_seed = EARTHLIKE_SEED;
        let generation_lod = 0_u8;
        let generation_graph_id = String::new();
        let generator_frequency = EARTHLIKE_FREQUENCY;
        let generator_amplitude = EARTHLIKE_AMPLITUDE;
        let generator_biome_frequency = EARTHLIKE_BIOME_FREQUENCY;
        let generator_algorithm = EARTHLIKE_ALGORITHM.to_string();
        let world_chunks = DEFAULT_WORLD_CHUNKS;
        let vertex_resolution = DEFAULT_VERTEX_RESOLUTION;
        let vertex_paint_color = DEFAULT_VERTEX_COLOR;

        let mut app = Self {
            engine,
            db,
            display,
            camera,
            camera_controller,
            status_text,
            window_size: window_size_vec,
            terrain_mode: TerrainMode::Procedural,
            terrain_chunks: Vec::new(),
            dbgen: TerrainDbgen::new(0),
            ui: TerrainEditorUi::new(window_size_vec),
            event_state,
            needs_refresh: true,
            persistence_error: None,
            rdb_path,
            rdb_path_input: String::new(),
            rdb_open: None,
            chunk_keys: Vec::new(),
            selected_chunk_index: None,
            db_dirty: false,
            status_note: None,
            focused_input: None,
            generation_seed,
            generation_lod,
            generation_graph_id: generation_graph_id.clone(),
            generator_frequency,
            generator_amplitude,
            generator_biome_frequency,
            generator_algorithm,
            seed_input: generation_seed.to_string(),
            lod_input: generation_lod.to_string(),
            graph_id_input: generation_graph_id,
            world_chunks,
            world_chunks_x_input: world_chunks[0].to_string(),
            world_chunks_y_input: world_chunks[1].to_string(),
            vertex_resolution,
            vertex_resolution_input: vertex_resolution.to_string(),
            brush_radius: DEFAULT_BRUSH_RADIUS,
            brush_strength: DEFAULT_BRUSH_STRENGTH,
            brush_tool: BrushTool::Sculpt(TerrainMutationOpKind::SphereAdd),
            vertex_paint_color,
            ui_hovered: false,
            last_world_cursor: Vec2::ZERO,
            panel_visibility: PanelVisibility {
                db: false,
                chunks: true,
                generation: false,
                brush: false,
                workflow: false,
            },
            pending_generation: VecDeque::new(),
            pending_request: None,
            pending_generation_total: 0,
            pending_generation_processed: 0,
            pending_failures: Vec::new(),
            pending_last_chunk_entry: None,
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
                if e.source() == EventSource::Window && e.event_type() == EventType::WindowResized {
                    let size = e.motion2d();
                    state.window_resized = Some(Vec2::new(size.x.max(1.0), size.y.max(1.0)));
                }

                if e.source() == EventSource::Key {
                    if e.event_type() == EventType::Pressed {
                        let key = e.key();
                        match key {
                            KeyCode::Shift => state.shift_down = true,
                            KeyCode::Control => state.control_down = true,
                            KeyCode::W => state.move_forward = true,
                            KeyCode::S => state.move_back = true,
                            KeyCode::A => state.move_left = true,
                            KeyCode::D => state.move_right = true,
                            KeyCode::E => state.move_up = true,
                            KeyCode::Q => state.move_down = true,
                            KeyCode::Space => state.move_active = true,
                            _ => {}
                        }
                        state.key_presses.push(key);
                    } else if e.event_type() == EventType::Released {
                        let key = e.key();
                        match key {
                            KeyCode::Shift => state.shift_down = false,
                            KeyCode::Control => state.control_down = false,
                            KeyCode::W => state.move_forward = false,
                            KeyCode::S => state.move_back = false,
                            KeyCode::A => state.move_left = false,
                            KeyCode::D => state.move_right = false,
                            KeyCode::E => state.move_up = false,
                            KeyCode::Q => state.move_down = false,
                            KeyCode::Space => state.move_active = false,
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
                        state.mouse_down = true;
                    } else if e.event_type() == EventType::Released {
                        state.mouse_down = false;
                        state.mouse_released = true;
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
        self.sync_generation_inputs();
        if let Some(size) = self.event_state.window_resized.take() {
            self.handle_window_resize(size);
        }
        let cursor_delta = self.consume_cursor_delta();

        let mut gui = GuiContext::new();
        let ui_input = TerrainEditorUiInput {
            cursor: self.event_state.cursor,
            mouse_pressed: self.event_state.mouse_pressed,
            mouse_down: self.event_state.mouse_down,
            mouse_released: self.event_state.mouse_released,
        };
        let ui_data = TerrainEditorUiData {
            viewport: self.window_size,
            db_path: &self.rdb_path_input,
            db_dirty: self.db_dirty,
            db_open: self.rdb_open.is_some(),
            chunk_keys: &self.chunk_keys,
            selected_chunk: self.selected_chunk_index,
            seed_input: &self.seed_input,
            lod_input: &self.lod_input,
            graph_id_input: &self.graph_id_input,
            world_chunks_x_input: &self.world_chunks_x_input,
            world_chunks_y_input: &self.world_chunks_y_input,
            vertex_resolution_input: &self.vertex_resolution_input,
            generator_frequency: self.generator_frequency,
            generator_amplitude: self.generator_amplitude,
            generator_biome_frequency: self.generator_biome_frequency,
            brush_tool: self.brush_tool,
            brush_radius: self.brush_radius,
            brush_strength: self.brush_strength,
            vertex_paint_color: self.vertex_paint_color,
            show_db_panel: self.panel_visibility.db,
            show_chunk_panel: self.panel_visibility.chunks,
            show_generation_panel: self.panel_visibility.generation,
            show_brush_panel: self.panel_visibility.brush,
            show_workflow_panel: self.panel_visibility.workflow,
            manual_mode: self.terrain_mode == TerrainMode::Manual,
        };
        let ui_output = self
            .ui
            .build(&mut gui, &ui_input, &ui_data, self.focused_input);

        let mouse_pressed = self.event_state.mouse_pressed;
        if mouse_pressed && ui_output.ui_hovered {
            self.event_state.mouse_pressed = false;
        }
        self.ui_hovered = ui_output.ui_hovered;
        if !self.ui_hovered {
            self.last_world_cursor = self.event_state.cursor;
        }

        let move_active =
            self.event_state.move_active && !self.ui_hovered && self.focused_input.is_none();
        let camera_input = CameraInput {
            forward: self.event_state.move_forward,
            back: self.event_state.move_back,
            left: self.event_state.move_left,
            right: self.event_state.move_right,
            up: self.event_state.move_up,
            down: self.event_state.move_down,
            fast: self.event_state.shift_down,
            move_active,
        };
        let camera_transform = self
            .camera_controller
            .update(dt, &camera_input, cursor_delta);
        self.engine
            .set_camera_transform(self.camera, &camera_transform);

        if ui_output.open_clicked {
            self.open_database_dialog();
        }
        if ui_output.new_clicked {
            self.create_new_database_dialog();
        }
        if ui_output.save_clicked {
            self.save_database_dialog();
        }
        if ui_output.generate_clicked {
            self.refresh_terrain();
        }
        if ui_output.earth_preset_clicked {
            self.apply_earthlike_preset();
        }
        if ui_output.brush_apply_clicked {
            self.apply_brush_at_cursor();
        }
        if ui_output.prev_chunk_clicked {
            self.select_prev_chunk();
        }
        if ui_output.next_chunk_clicked {
            self.select_next_chunk();
        }
        if let Some(index) = ui_output.select_chunk {
            self.select_chunk(index);
        }
        if let Some(tool) = ui_output.brush_tool {
            self.brush_tool = tool;
        }
        if let Some(radius) = ui_output.brush_radius {
            self.brush_radius = radius;
        }
        if let Some(strength) = ui_output.brush_strength {
            self.brush_strength = strength;
        }
        if let Some(value) = ui_output.vertex_paint_r {
            self.vertex_paint_color[0] = value;
        }
        if let Some(value) = ui_output.vertex_paint_g {
            self.vertex_paint_color[1] = value;
        }
        if let Some(value) = ui_output.vertex_paint_b {
            self.vertex_paint_color[2] = value;
        }
        if let Some(value) = ui_output.generator_frequency {
            self.generator_frequency = value;
        }
        if let Some(value) = ui_output.generator_amplitude {
            self.generator_amplitude = value;
        }
        if let Some(value) = ui_output.generator_biome_frequency {
            self.generator_biome_frequency = value;
        }
        if let Some(action) = ui_output.menu_action {
            self.handle_menu_action(action);
        }
        let previous_focus = self.focused_input;
        if let Some(focused) = ui_output.focused_input {
            self.focused_input = Some(focused);
        } else if ui_output.ui_hovered && mouse_pressed {
            self.focused_input = None;
        }
        if previous_focus != self.focused_input {
            self.update_status_text();
        }

        let frame = gui.build_frame();
        self.engine.upload_gui_frame(frame);

        if self.needs_refresh {
            self.refresh_terrain();
            self.needs_refresh = false;
        }
        self.process_generation_queue();

        if self.terrain_mode == TerrainMode::Manual && !self.ui_hovered {
            self.handle_manual_brush();
        }

        self.event_state.mouse_pressed = false;
        self.event_state.mouse_released = false;

        self.engine.update(dt);
    }

    fn refresh_terrain(&mut self) {
        if self.rdb_open.is_none() {
            self.persistence_error = Some("No database open.".to_string());
            self.update_status_text();
            return;
        }

        let request = TerrainGenerationRequest {
            chunk_key: self.current_chunk_key(),
            generator_graph_id: self.generation_graph_id.clone(),
            lod: self.generation_lod,
            generator_frequency: self.generator_frequency,
            generator_amplitude: self.generator_amplitude,
            generator_biome_frequency: self.generator_biome_frequency,
            generator_algorithm: self.generator_algorithm.clone(),
            world_chunks: self.world_chunks,
            vertex_resolution: self.vertex_resolution,
        };

        let total_chunks =
            self.world_chunks[0].saturating_mul(self.world_chunks[1]) as usize;
        if total_chunks == 0 {
            self.status_note = Some("No chunks to generate.".to_string());
            self.update_status_text();
            return;
        }

        let mut pending_generation = VecDeque::with_capacity(total_chunks);
        for y in 0..self.world_chunks[1] {
            for x in 0..self.world_chunks[0] {
                pending_generation.push_back([x, y]);
            }
        }

        self.pending_generation = pending_generation;
        self.pending_request = Some(request);
        self.pending_generation_total = total_chunks;
        self.pending_generation_processed = 0;
        self.pending_failures.clear();
        self.pending_last_chunk_entry = None;
        self.persistence_error = None;
        self.status_note = Some("Terrain generation started.".to_string());
        self.update_status_text();
    }

    fn process_generation_queue(&mut self) {
        if self.pending_generation.is_empty() {
            return;
        }

        let Some(request_template) = self.pending_request.clone() else {
            self.pending_generation.clear();
            return;
        };

        let remaining = self.pending_generation.len();
        let batch_size = GENERATION_CHUNKS_PER_FRAME.min(remaining);
        let mut batch = Vec::with_capacity(batch_size);
        for _ in 0..batch_size {
            if let Some(coords) = self.pending_generation.pop_front() {
                batch.push(coords);
            }
        }

        let (generated_entries, last_chunk_entry, failures) = {
            let Some(rdb) = self.rdb_open.as_mut() else {
                self.persistence_error = Some("No database open.".to_string());
                self.update_status_text();
                self.pending_generation.clear();
                self.pending_request = None;
                return;
            };

            let mut generated_entries = Vec::new();
            let mut last_chunk_entry = None;
            let mut failures = Vec::new();
            for coords in batch {
                let mut request = request_template.clone();
                request.chunk_key = format!("terrain/chunk_{}_{}", coords[0], coords[1]);
                let entry_key = self
                    .dbgen
                    .chunk_entry_for_key(&request.chunk_key, request.lod);
                match self.dbgen.generate_chunk(&request, rdb) {
                    Ok(result) => {
                        generated_entries.push(result.chunk_entry.clone());
                        last_chunk_entry = Some(result.chunk_entry);
                    }
                    Err(err) => {
                        warn!(
                            error = %err,
                            chunk_key = %request.chunk_key,
                            entry_key = %entry_key,
                            "Terrain generation failed."
                        );
                        failures.push(format!("{entry_key}: {err}"));
                    }
                }
            }
            (generated_entries, last_chunk_entry, failures)
        };

        if !generated_entries.is_empty() {
            self.db_dirty = true;
            for chunk_entry in &generated_entries {
                self.ensure_chunk_key(chunk_entry);
            }
        }

        self.pending_generation_processed =
            self.pending_generation_processed.saturating_add(batch_size);
        self.pending_failures.extend(failures);
        if let Some(chunk_entry) = last_chunk_entry {
            self.pending_last_chunk_entry = Some(chunk_entry);
        }

        if self.pending_generation.is_empty() {
            self.pending_request = None;
            if let Some(chunk_entry) = self.pending_last_chunk_entry.clone() {
                self.status_note = Some("Terrain generated.".to_string());
                self.update_rendered_chunk(chunk_entry);
            }
            if !self.pending_failures.is_empty() {
                self.persistence_error = Some(format!(
                    "Generation failed for {} chunks.",
                    self.pending_failures.len()
                ));
                self.status_note = Some(
                    "Terrain generation completed with errors. Check logs for details.".to_string(),
                );
            }
            self.update_status_text();
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
            "Earth-like Terrain Editor | Mode: {} | DB: {} ({}) | Chunk: {}",
            self.terrain_mode.label(),
            self.rdb_path.display(),
            db_status,
            chunk_label
        );
        status.push_str(" | Tab: toggle | Ctrl+O: open | Ctrl+W: close | Ctrl+S: save");
        status.push_str(" | Up/Down: select chunk");
        status.push_str(" | Hold Space + WASDQE to move | Mouse to look");
        if self.focused_input == Some(FocusedInput::DbPath) {
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

    fn handle_window_resize(&mut self, size: Vec2) {
        self.window_size = size;
        self.engine.set_camera_perspective(
            self.camera,
            60f32.to_radians(),
            size.x,
            size.y,
            0.1,
            50000.0,
        );
        self.update_status_text();
    }

    fn consume_cursor_delta(&mut self) -> Vec2 {
        let delta = if let Some(last) = self.event_state.last_cursor {
            self.event_state.cursor - last
        } else {
            Vec2::ZERO
        };
        self.event_state.last_cursor = Some(self.event_state.cursor);
        delta
    }

    fn sync_generation_inputs(&mut self) {
        if let Ok(seed) = self.seed_input.trim().parse::<u64>() {
            if seed != self.generation_seed {
                self.generation_seed = seed;
                self.dbgen.set_seed(seed);
            }
        }

        if let Ok(lod) = self.lod_input.trim().parse::<u8>() {
            self.generation_lod = lod;
        }

        if let Ok(chunks_x) = self.world_chunks_x_input.trim().parse::<u32>() {
            self.world_chunks[0] = chunks_x.max(1);
        }
        if let Ok(chunks_y) = self.world_chunks_y_input.trim().parse::<u32>() {
            self.world_chunks[1] = chunks_y.max(1);
        }

        if let Ok(vertex_resolution) = self.vertex_resolution_input.trim().parse::<u32>() {
            self.vertex_resolution = vertex_resolution.max(1);
        }

        self.generation_graph_id = self.graph_id_input.trim().to_string();
    }

    fn apply_earthlike_preset(&mut self) {
        self.generation_seed = EARTHLIKE_SEED;
        self.seed_input = self.generation_seed.to_string();
        self.dbgen.set_seed(self.generation_seed);

        self.generator_frequency = EARTHLIKE_FREQUENCY;
        self.generator_amplitude = EARTHLIKE_AMPLITUDE;
        self.generator_biome_frequency = EARTHLIKE_BIOME_FREQUENCY;
        self.generator_algorithm = EARTHLIKE_ALGORITHM.to_string();
        self.world_chunks = DEFAULT_WORLD_CHUNKS;
        self.world_chunks_x_input = self.world_chunks[0].to_string();
        self.world_chunks_y_input = self.world_chunks[1].to_string();
        self.vertex_resolution = DEFAULT_VERTEX_RESOLUTION;
        self.vertex_resolution_input = self.vertex_resolution.to_string();

        self.status_note = Some("Earth-like preset applied.".to_string());
        self.update_status_text();
    }

    fn handle_manual_brush(&mut self) {
        if !self.event_state.mouse_pressed {
            return;
        }
        self.event_state.mouse_pressed = false;
        self.apply_brush_at_cursor();
    }

    fn apply_brush_at_cursor(&mut self) {
        if self.brush_tool == BrushTool::VertexPaint {
            self.apply_vertex_paint_at_cursor();
            return;
        }

        let BrushTool::Sculpt(tool) = self.brush_tool else {
            return;
        };
        let chunk_key = self.current_chunk_key();
        let world_pos = self.cursor_to_world(self.last_world_cursor, &chunk_key);

        let Some(mut rdb) = self.rdb_open.take() else {
            self.persistence_error = Some("No database open.".to_string());
            self.update_status_text();
            return;
        };

        let request = TerrainBrushRequest {
            chunk_key: chunk_key.clone(),
            generator_graph_id: self.generation_graph_id.clone(),
            lod: self.generation_lod,
            generator_frequency: self.generator_frequency,
            generator_amplitude: self.generator_amplitude,
            generator_biome_frequency: self.generator_biome_frequency,
            generator_algorithm: self.generator_algorithm.clone(),
            world_chunks: self.world_chunks,
            vertex_resolution: self.vertex_resolution,
            world_pos: [world_pos.x, world_pos.y, world_pos.z],
            radius: self.brush_radius,
            strength: self.brush_strength,
            tool,
        };

        let result = self.dbgen.apply_brush_in_memory(&request, &mut rdb);
        self.rdb_open = Some(rdb);

        match result {
            Ok(result) => {
                self.persistence_error = None;
                self.db_dirty = true;
                self.ensure_chunk_key(&result.chunk_entry);
                self.status_note = Some("Brush applied.".to_string());
                self.update_rendered_chunk(result.chunk_entry);
            }
            Err(err) => {
                warn!(error = %err, "Failed to apply terrain brush.");
                self.persistence_error = Some(format!("Brush apply failed: {err}"));
                self.status_note = Some("Brush apply failed.".to_string());
                self.update_status_text();
            }
        }
    }

    fn apply_vertex_paint_at_cursor(&mut self) {
        let chunk_key = self.current_chunk_key();
        let world_pos = self.cursor_to_world(self.last_world_cursor, &chunk_key);
        let Some(mut rdb) = self.rdb_open.take() else {
            self.persistence_error = Some("No database open.".to_string());
            self.update_status_text();
            return;
        };

        let artifact_entry = self
            .dbgen
            .chunk_entry_for_key(&chunk_key, self.generation_lod);
        let mut artifact = match rdb.fetch::<TerrainChunkArtifact>(&artifact_entry) {
            Ok(artifact) => artifact,
            Err(err) => {
                self.rdb_open = Some(rdb);
                self.persistence_error =
                    Some(format!("Vertex paint failed to load artifact: {err}"));
                self.status_note = Some("Vertex paint failed.".to_string());
                self.update_status_text();
                return;
            }
        };

        let changed = paint_vertices_in_artifact(
            &mut artifact,
            world_pos,
            self.brush_radius,
            self.brush_strength,
            self.vertex_paint_color,
        );

        if !changed {
            self.rdb_open = Some(rdb);
            self.status_note = Some("Vertex paint: no vertices touched.".to_string());
            self.update_status_text();
            return;
        }

        artifact.content_hash = hash_vertex_colors(&artifact.vertices);
        if let Err(err) = rdb.upsert(&artifact_entry, &artifact) {
            self.rdb_open = Some(rdb);
            self.persistence_error = Some(format!("Vertex paint save failed: {err}"));
            self.status_note = Some("Vertex paint failed.".to_string());
            self.update_status_text();
            return;
        }

        self.rdb_open = Some(rdb);
        self.persistence_error = None;
        self.db_dirty = true;
        self.status_note = Some("Vertex paint applied.".to_string());
        self.update_rendered_chunk(chunk_key);
    }

    fn cursor_to_world(&mut self, cursor: Vec2, chunk_key: &str) -> Vec3 {
        let chunk_coords = self.dbgen.chunk_coords_for_key(chunk_key);
        let (tile_size, tiles_per_chunk) = self
            .project_chunk_settings(chunk_key)
            .unwrap_or((1.0, [32_u32, 32_u32]));
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

    fn project_chunk_settings(&mut self, chunk_key: &str) -> Option<(f32, [u32; 2])> {
        let settings = self.project_settings_for_chunk(chunk_key)?;
        Some((settings.tile_size, settings.tiles_per_chunk))
    }

    fn project_settings_for_chunk(&mut self, chunk_key: &str) -> Option<TerrainProjectSettings> {
        let project_key = self.dbgen.project_key_for_chunk(chunk_key);
        let rdb = self.rdb_open.as_mut()?;
        let settings_entry = project_settings_entry(&project_key);
        rdb.fetch::<TerrainProjectSettings>(&settings_entry).ok()
    }

    fn update_rendered_chunk(&mut self, chunk_entry: String) {
        self.update_status_text();
        let Some(rdb) = self.rdb_open.as_mut() else {
            return;
        };

        let project_key = self.dbgen.project_key_for_chunk(&chunk_entry);
        let mut entries = if self.chunk_keys.is_empty() {
            vec![chunk_entry.clone()]
        } else {
            self.chunk_keys.clone()
        };
        if !entries.iter().any(|key| key == &chunk_entry) {
            entries.push(chunk_entry.clone());
            entries.sort();
        }

        let lod = self.generation_lod;
        self.terrain_chunks.clear();
        self.terrain_chunks.extend(entries.into_iter().map(|entry| {
            let artifact_entry = self.dbgen.chunk_entry_for_key(&entry, lod);
            TerrainChunkRef::artifact_entry(artifact_entry)
        }));
        self.engine
            .set_terrain_render_objects_from_rdb(rdb, &project_key, &self.terrain_chunks);
    }

    fn shutdown(self) {
        self.engine.shut_down();
    }

    fn handle_key_press(&mut self, key: KeyCode) {
        if let Some(focused) = self.focused_input {
            self.handle_focused_input(focused, key);
            return;
        }

        let control = self.event_state.control_down;

        match key {
            KeyCode::Tab => {
                let new_mode = match self.terrain_mode {
                    TerrainMode::Procedural => TerrainMode::Manual,
                    TerrainMode::Manual => TerrainMode::Procedural,
                };
                self.set_terrain_mode(new_mode);
            }
            KeyCode::O if control => self.open_database_dialog(),
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

    fn handle_focused_input(&mut self, focused: FocusedInput, key: KeyCode) {
        match key {
            KeyCode::Escape => {
                self.focused_input = None;
                self.status_note = None;
                self.update_status_text();
            }
            KeyCode::Enter => {
                if focused == FocusedInput::DbPath {
                    self.open_database_from_input();
                }
                self.focused_input = None;
                self.status_note = None;
                self.update_status_text();
            }
            KeyCode::Backspace => {
                match focused {
                    FocusedInput::DbPath => {
                        self.rdb_path_input.pop();
                    }
                    FocusedInput::Seed => {
                        self.seed_input.pop();
                    }
                    FocusedInput::Lod => {
                        self.lod_input.pop();
                    }
                    FocusedInput::GeneratorGraph => {
                        self.graph_id_input.pop();
                    }
                    FocusedInput::WorldChunksX => {
                        self.world_chunks_x_input.pop();
                    }
                    FocusedInput::WorldChunksY => {
                        self.world_chunks_y_input.pop();
                    }
                    FocusedInput::VertexResolution => {
                        self.vertex_resolution_input.pop();
                    }
                }
                self.update_status_text();
            }
            _ => {
                if let Some(ch) = keycode_to_char(key, self.event_state.shift_down) {
                    match focused {
                        FocusedInput::DbPath => {
                            self.rdb_path_input.push(ch);
                        }
                        FocusedInput::Seed => {
                            if ch.is_ascii_digit() {
                                self.seed_input.push(ch);
                            }
                        }
                        FocusedInput::Lod => {
                            if ch.is_ascii_digit() {
                                self.lod_input.push(ch);
                            }
                        }
                        FocusedInput::GeneratorGraph => {
                            if !ch.is_control() {
                                self.graph_id_input.push(ch);
                            }
                        }
                        FocusedInput::WorldChunksX => {
                            if ch.is_ascii_digit() {
                                self.world_chunks_x_input.push(ch);
                            }
                        }
                        FocusedInput::WorldChunksY => {
                            if ch.is_ascii_digit() {
                                self.world_chunks_y_input.push(ch);
                            }
                        }
                        FocusedInput::VertexResolution => {
                            if ch.is_ascii_digit() {
                                self.vertex_resolution_input.push(ch);
                            }
                        }
                    }
                    self.update_status_text();
                }
            }
        }
    }

    fn open_database_from_input(&mut self) {
        let path = PathBuf::from(self.rdb_path_input.trim());
        if path.as_os_str().is_empty() {
            self.persistence_error = Some("Database path cannot be empty.".to_string());
            self.update_status_text();
        } else {
            self.open_database(path);
        }
    }

    fn create_new_database(&mut self) {
        self.rdb_open = Some(RDBFile::new());
        if self.rdb_path_input.trim().is_empty() {
            self.rdb_path = PathBuf::from("terrain.rdb");
            self.rdb_path_input = self.rdb_path.to_string_lossy().to_string();
        } else {
            self.rdb_path = PathBuf::from(self.rdb_path_input.trim());
        }
        self.db_dirty = true;
        self.persistence_error = None;
        self.status_note = Some("New database created (unsaved).".to_string());
        self.chunk_keys.clear();
        self.selected_chunk_index = None;
        self.needs_refresh = true;
        self.update_status_text();
    }

    fn create_new_database_dialog(&mut self) {
        let selected = FileDialog::new().set_file_name("terrain.rdb").save_file();
        if let Some(path) = selected {
            self.rdb_open = Some(RDBFile::new());
            self.rdb_path = path;
            self.rdb_path_input = self.rdb_path.to_string_lossy().to_string();
            self.db_dirty = false;
            self.persistence_error = None;
            if let Some(rdb) = self.rdb_open.as_mut() {
                if let Err(err) = rdb.save(&self.rdb_path) {
                    warn!(
                        error = %err,
                        path = %self.rdb_path.display(),
                        "Failed to create new terrain RDB."
                    );
                    self.persistence_error = Some(format!("RDB create failed: {err}"));
                    self.db_dirty = true;
                }
            }
            self.status_note = Some("New database created.".to_string());
            self.chunk_keys.clear();
            self.selected_chunk_index = None;
            self.needs_refresh = true;
            self.update_status_text();
        }
    }

    fn open_database_dialog(&mut self) {
        let selected = FileDialog::new().pick_file();
        if let Some(path) = selected {
            self.open_database(path);
        }
    }

    fn save_database_dialog(&mut self) {
        if self.rdb_open.is_none() {
            self.persistence_error = Some("No database open.".to_string());
            self.update_status_text();
            return;
        }
        if self.rdb_path_input.trim().is_empty() {
            if let Some(path) = FileDialog::new().set_file_name("terrain.rdb").save_file() {
                self.rdb_path = path;
                self.rdb_path_input = self.rdb_path.to_string_lossy().to_string();
            } else {
                return;
            }
        }
        self.commit_database();
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
                self.status_note = Some("Failed to load RDB; created new database.".to_string());
                RDBFile::new()
            }
        };

        self.rdb_open = Some(rdb);
        self.rdb_path = path;
        self.rdb_path_input = self.rdb_path.to_string_lossy().to_string();
        self.db_dirty = false;
        self.persistence_error = None;
        if self.status_note.is_none() {
            self.status_note = Some("Database opened.".to_string());
        }
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
            self.status_note = Some("Database save failed.".to_string());
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
            if rdb.fetch::<TerrainChunk>(&entry.name).is_ok() {
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
            self.selected_chunk_index = self.chunk_keys.iter().position(|key| key == chunk_key);
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
        self.select_chunk(new_index);
    }

    fn select_next_chunk(&mut self) {
        if self.chunk_keys.is_empty() {
            return;
        }
        let index = self.selected_chunk_index.unwrap_or(0);
        let new_index = (index + 1) % self.chunk_keys.len();
        self.select_chunk(new_index);
    }

    fn select_chunk(&mut self, index: usize) {
        if index >= self.chunk_keys.len() {
            return;
        }
        self.selected_chunk_index = Some(index);
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
        let chunk = {
            let Some(rdb) = self.rdb_open.as_mut() else {
                return;
            };
            match rdb.fetch::<TerrainChunk>(&chunk_key) {
                Ok(chunk) => Some(chunk),
                Err(err) => {
                    warn!(
                        error = %err,
                        entry = %chunk_key,
                        "Failed to load terrain chunk."
                    );
                    self.persistence_error = Some(format!("Chunk load failed: {err}"));
                    self.update_status_text();
                    None
                }
            }
        };

        if chunk.is_some() {
            self.persistence_error = None;
            self.update_rendered_chunk(chunk_key);
        }
    }

    fn current_chunk_key(&self) -> String {
        self.selected_chunk_index
            .and_then(|index| self.chunk_keys.get(index))
            .cloned()
            .unwrap_or_else(|| DEFAULT_CHUNK_KEY.to_string())
    }

    fn handle_menu_action(&mut self, action: u32) {
        match action {
            MENU_ACTION_NEW_RDB => self.create_new_database_dialog(),
            MENU_ACTION_OPEN_RDB => self.open_database_dialog(),
            MENU_ACTION_SAVE_RDB => self.save_database_dialog(),
            MENU_ACTION_CLOSE_RDB => self.close_database(),
            MENU_ACTION_SET_PROCEDURAL => self.set_terrain_mode(TerrainMode::Procedural),
            MENU_ACTION_SET_MANUAL => self.set_terrain_mode(TerrainMode::Manual),
            MENU_ACTION_APPLY_BRUSH => self.apply_brush_at_cursor(),
            MENU_ACTION_TOGGLE_DB_PANEL => self.panel_visibility.db = !self.panel_visibility.db,
            MENU_ACTION_TOGGLE_CHUNK_PANEL => {
                self.panel_visibility.chunks = !self.panel_visibility.chunks;
            }
            MENU_ACTION_TOGGLE_GENERATION_PANEL => {
                self.panel_visibility.generation = !self.panel_visibility.generation;
            }
            MENU_ACTION_TOGGLE_BRUSH_PANEL => {
                self.panel_visibility.brush = !self.panel_visibility.brush;
            }
            MENU_ACTION_TOGGLE_WORKFLOW_PANEL => {
                self.panel_visibility.workflow = !self.panel_visibility.workflow;
            }
            MENU_ACTION_SHOW_WORKFLOW => {
                self.panel_visibility.workflow = true;
            }
            MENU_ACTION_EARTH_PRESET => self.apply_earthlike_preset(),
            MENU_ACTION_GENERATE => self.refresh_terrain(),
            _ => {}
        }
    }

    fn set_terrain_mode(&mut self, mode: TerrainMode) {
        if self.terrain_mode == mode {
            return;
        }
        self.terrain_mode = mode;
        match self.terrain_mode {
            TerrainMode::Procedural => {
                self.panel_visibility.generation = true;
                self.panel_visibility.brush = false;
            }
            TerrainMode::Manual => {
                self.panel_visibility.brush = true;
                self.panel_visibility.generation = false;
            }
        }
        self.needs_refresh = true;
        self.update_status_text();
    }
}

fn paint_vertices_in_artifact(
    artifact: &mut TerrainChunkArtifact,
    world_pos: Vec3,
    radius: f32,
    strength: f32,
    target_color: [f32; 3],
) -> bool {
    if artifact.vertices.is_empty() {
        return false;
    }
    let radius = radius.max(0.001);
    let radius_sq = radius * radius;
    let strength = strength.max(0.0);
    let mut changed = false;
    for vertex in &mut artifact.vertices {
        let dx = vertex.position[0] - world_pos.x;
        let dy = vertex.position[1] - world_pos.y;
        let dist_sq = dx * dx + dy * dy;
        if dist_sq <= radius_sq {
            let dist = dist_sq.sqrt();
            let falloff = (1.0 - dist / radius).clamp(0.0, 1.0);
            let blend = (strength * falloff).clamp(0.0, 1.0);
            for channel in 0..3 {
                let current = vertex.color[channel];
                vertex.color[channel] = current + (target_color[channel] - current) * blend;
            }
            vertex.color[3] = 1.0;
            changed = true;
        }
    }
    changed
}

fn hash_vertex_colors(vertices: &[Vertex]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for vertex in vertices {
        for channel in vertex.color {
            channel.to_bits().hash(&mut hasher);
        }
    }
    hasher.finish()
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
