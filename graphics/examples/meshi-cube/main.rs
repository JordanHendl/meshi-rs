use glam::*;
use meshi_graphics::*;
use meshi_utils::timer::Timer;

fn main() {
    tracing_subscriber::fmt::init();
    let mut engine = RenderEngine::new(&RenderEngineInfo {
        headless: false,
        canvas_extent: Some([1280, 1024]),
    })
    .unwrap();

    // Default database. Given bogus directory so all we have to work with is the default
    // models/materials...
    let mut db = DB::new(&DBInfo {
        base_dir: "",
        layout_file: None,
        pooled_geometry_uploads: false,
    })
    .expect("Unable to create database");

    
    engine.initialize_database(&mut db);

    // Make window for output to render to.
    let display = engine.register_window_display(DisplayInfo {
        window: WindowInfo {
            title: "meshi-cube".to_string(),
            size: [1024, 1024],
            resizable: false,
        },
        ..Default::default()
    });
    
    // Register a camera and assign it to the display.
    let camera = engine.register_camera(&Mat4::IDENTITY);
    engine.attach_camera_to_display(display, camera);
    
    // Register default cube with the engine as an object.
    let cube = engine
        .register_object(&RenderObjectInfo::Model(
            db.fetch_gpu_model("model/cube").unwrap(),
        ))
        .unwrap();
    
    // Update object transform to be the center.
    engine.set_object_transform(cube, &Mat4::from_translation(Vec3::new(0.0, 0.0, 0.0)));

    let mut timer = Timer::new();
    timer.start();

    loop {
        timer.stop();
        engine.update(timer.elapsed_seconds_f32());
        timer.start();
    }
}
