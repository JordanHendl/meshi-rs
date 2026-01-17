use dashi::Handle;
use glam::{Mat4, Vec2};
use meshi_graphics::{Camera, DB, DBInfo, Display, DisplayInfo, RenderEngine, RenderEngineInfo};
use meshi_graphics::{RendererSelect, TextRenderMode, WindowInfo};

#[derive(Clone, Copy)]
pub struct CameraSetup {
    pub transform: Mat4,
    pub fov_y_radians: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for CameraSetup {
    fn default() -> Self {
        Self {
            transform: Mat4::IDENTITY,
            fov_y_radians: 60f32.to_radians(),
            near: 0.1,
            far: 100.0,
        }
    }
}

pub struct ExampleSetup {
    pub engine: RenderEngine,
    pub db: Box<DB>,
    pub display: Handle<Display>,
    pub camera: Handle<Camera>,
    pub window_size: Vec2,
}

pub fn renderer_from_args(args: &[String], default: RendererSelect) -> RendererSelect {
    if args.iter().any(|arg| arg == "--forward") {
        RendererSelect::Forward
    } else if args.iter().any(|arg| arg == "--deferred") {
        RendererSelect::Deferred
    } else {
        default
    }
}

pub fn init(
    title: &str,
    window_size: [u32; 2],
    camera_setup: CameraSetup,
    renderer: RendererSelect,
) -> ExampleSetup {
    let mut engine = RenderEngine::new(&RenderEngineInfo {
        headless: false,
        canvas_extent: Some(window_size),
        renderer,
        sample_count: None,
    })
    .unwrap();

    let mut db = Box::new(DB::new(&DBInfo {
        base_dir: "",
        layout_file: None,
        pooled_geometry_uploads: false,
    })
    .expect("Unable to create database"));

    engine.initialize_database(&mut db);

    let display = engine.register_window_display(DisplayInfo {
        vsync: false,
        window: WindowInfo {
            title: title.to_string(),
            size: window_size,
            resizable: false,
        },
        ..Default::default()
    });

    let camera = engine.register_camera(&camera_setup.transform);
    engine.attach_camera_to_display(display, camera);
    engine.set_camera_perspective(
        camera,
        camera_setup.fov_y_radians,
        window_size[0] as f32,
        window_size[1] as f32,
        camera_setup.near,
        camera_setup.far,
    );

    ExampleSetup {
        engine,
        db,
        display,
        camera,
        window_size: Vec2::new(window_size[0] as f32, window_size[1] as f32),
    }
}

pub fn text_render_mode(db: &DB) -> TextRenderMode {
    let sdf_font = db.enumerate_sdf_fonts().into_iter().next();
    sdf_font
        .map(|font| TextRenderMode::Sdf { font })
        .unwrap_or(TextRenderMode::Plain)
}
