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
        window: WindowInfo {
            title: "meshi-skinned".to_string(),
            size: [512, 512],
            resizable: false,
        },
        ..Default::default()
    });

    // Register a camera and assign it to the display.
    let camera = engine.register_camera(&Mat4::from_translation(Vec3::new(4.0, 0.0, 0.0)));
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


    let model = db.fetch_gpu_model("model/fox").unwrap();
    let animation_names = model
        .rig
        .as_ref()
        .map(|rig| {
            let mut names: Vec<String> = rig.animations.keys().cloned().collect();
            names.sort();
            names
        })
        .unwrap_or_default();
    let skinned = engine
        .register_object(&RenderObjectInfo::SkinnedModel(SkinnedModelInfo {
            model,
            animation: AnimationState::default(),
        }))
        .unwrap();

    let translation = Mat4::from_translation(Vec3::new(0.0, 0.0, -5.0));
    engine.set_object_transform(skinned, &translation);

    struct AppData {
        running: bool,
        animation_index: usize,
        animation_count: usize,
        animation_changed: bool,
    }

    let mut data = AppData {
        running: true,
        animation_index: 0,
        animation_count: animation_names.len(),
        animation_changed: false,
    };

    extern "C" fn callback(event: *mut Event, data: *mut c_void) {
        unsafe {
            let e = &mut (*event);
            let r = &mut (*(data as *mut AppData));
            if e.source() == EventSource::Key && e.event_type() == EventType::Pressed {
                match e.key() {
                    KeyCode::ArrowLeft => {
                        if r.animation_count > 0 {
                            if r.animation_index == 0 {
                                r.animation_index = r.animation_count - 1;
                            } else {
                                r.animation_index -= 1;
                            }
                            r.animation_changed = true;
                        }
                    }
                    KeyCode::ArrowRight => {
                        if r.animation_count > 0 {
                            r.animation_index = (r.animation_index + 1) % r.animation_count;
                            r.animation_changed = true;
                        }
                    }
                    _ => {}
                }
            }

            if e.source() == EventSource::Window {
                if e.event_type() == EventType::Quit {
                    r.running = false;
                }
            }
        }
    }

    engine.set_event_cb(callback, (&mut data as *mut AppData) as *mut c_void);
    let mut timer = Timer::new();
    timer.start();
    let mut last_time = timer.elapsed_seconds_f32();

    while data.running {
        let now = timer.elapsed_seconds_f32();
        let mut dt = now - last_time;
        dt = dt.min(1.0 / 30.0);
        if data.animation_changed {
            engine.set_skinned_object_animation(
                skinned,
                AnimationState {
                    clip_index: data.animation_index as u32,
                    time_seconds: 0.0,
                    speed: 1.0,
                    looping: true,
                },
            );
            data.animation_changed = false;
        }
        engine.update(dt);
        last_time = now;
    }

    engine.shut_down();
}
