use glam::*;
use meshi_graphics::*;
use meshi_utils::timer::Timer;

fn main() {
    tracing_subscriber::fmt::init();
    let mut engine = RenderEngine::new(&RenderEngineInfo {
        headless: false,
        canvas_extent: Some([1024, 1024]),
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

    // Typical perspective: 60° vertical FOV, window aspect, near/far planes.
    engine.set_camera_perspective(
        camera,
        60f32.to_radians(),
        1024.0, // width
        1024.0, // height
        0.1,    // near
        100.0,  // far
    );

    // Register default cube with the engine as an object.
    let cube = engine
        .register_object(&RenderObjectInfo::Model(
            db.fetch_gpu_model("model/cube").unwrap(),
        ))
        .unwrap();

    let mut transform = Mat4::from_translation(Vec3::new(0.0, 0.0, -5.0));
    // Update object transform to be the center.
    engine.set_object_transform(cube, &transform);

    let mut timer = Timer::new();
    timer.start();
    
    loop {
        timer.stop();
        let dt = timer.elapsed_seconds_f32();
        let rotation = Mat4::from_rotation_y(20000.0 * dt);
        transform = transform * rotation;
        engine.set_object_transform(cube, &transform);
        engine.update(dt);
        timer.start();
    }
}
