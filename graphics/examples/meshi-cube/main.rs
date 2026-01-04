use std::ffi::c_void;

use glam::*;
use meshi_ffi_structs::event::*;
use meshi_graphics::*;
use meshi_utils::timer::Timer;
use std::env::*;

fn main() {
    tracing_subscriber::fmt::init();
    let args: Vec<String> = args().collect();
    let mut renderer = RendererSelect::Deferred;
    if args.len() > 1 {
        if args[1] == "--forward" {
            renderer = RendererSelect::Forward;
        }
    }
    let mut engine = RenderEngine::new(&RenderEngineInfo {
        headless: false,
        canvas_extent: Some([512, 512]),
        renderer,
        sample_count: None,
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
        vsync: false,
        window: WindowInfo {
            title: "meshi-cube".to_string(),
            size: [512, 512],
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
        512.0, // width
        512.0, // height
        0.1,   // near
        100.0, // far
    );

    // Register default cube with the engine as an object.
    let cube = engine
        .register_object(&RenderObjectInfo::Model(
            db.fetch_gpu_model("model/cube").unwrap(),
        ))
        .unwrap();

    let translation = Mat4::from_translation(Vec3::new(0.0, 0.25, -2.5));
    let mut transform = translation;
    // Update object transform to be the center.
    engine.set_object_transform(cube, &transform);

    struct AppData {
        running: bool,
        paused: bool,
    }

    let mut data = AppData {
        running: true,
        paused: false,
    };

    extern "C" fn callback(event: *mut Event, data: *mut c_void) {
        unsafe {
            let e = &mut (*event);
            let r = &mut (*(data as *mut AppData));
            if e.source() == EventSource::Key && e.event_type() == EventType::Pressed {
                if e.key() == KeyCode::Space {
                    r.paused = !r.paused;
                }
            }

            if e.event_type() == EventType::Quit {
                r.running = false;
            }
        }
    }

    engine.set_event_cb(callback, (&mut data as *mut AppData) as *mut c_void);
    let mut timer = Timer::new();
    timer.start();
    let mut last_time = timer.elapsed_seconds_f32();
    let mut total_time = 0.0f32;
    let angular_velocity = 2.0f32;

    while data.running {
        let now = timer.elapsed_seconds_f32();
        let mut dt = now - last_time;
        dt = dt.min(1.0 / 30.0);
        if !data.paused {
            total_time += dt;
            let mut rotation = Mat4::from_rotation_y(angular_velocity * total_time);
            rotation = rotation * Mat4::from_rotation_x(angular_velocity * total_time);
            transform = translation * rotation;
            engine.set_object_transform(cube, &transform);
        }
        engine.update(dt);
        last_time = now;
    }

    engine.shut_down();
}
