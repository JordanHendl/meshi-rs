use glam::*;
use meshi_graphics::*;
use meshi_utils::timer::Timer;

fn main() {
    let mut engine = RenderEngine::new(&RenderEngineInfo {
        headless: false,
        canvas_extent: Some([1280, 1024]),
    })
    .unwrap();

    let mut db = DB::new(&DBInfo {
        base_dir: "",
        layout_file: None,
    })
    .expect("Unable to create database");

    db.import_dashi_context(engine.context());
    
    let display = engine.register_display(DisplayInfo {
        window: WindowInfo {
            title: "meshi-cube".to_string(),
            size: [1024, 1024],
            resizable: false,
        },
        ..Default::default()
    });

    let camera = engine.register_camera(&Mat4::IDENTITY);
    engine.attach_camera_to_display(display, camera);

    let cube = engine
        .register_object(&RenderObjectInfo::Model(
            db.fetch_gpu_model("model/sphere").unwrap(),
        ))
        .unwrap();
    
    engine.set_object_transform(cube, &Mat4::from_translation(Vec3::new(0.0, 0.0, 0.0)));

    let mut timer = Timer::new();
    timer.start();

    loop {
        timer.stop();
        engine.update(timer.elapsed_seconds_f32());
        timer.start();
    }
}
