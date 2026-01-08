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

    struct CameraInput {
        forward: bool,
        back: bool,
        left: bool,
        right: bool,
        up: bool,
        down: bool,
        fast: bool,
        mouse_delta: Vec2,
    }

    struct AppData {
        running: bool,
        paused: bool,
        camera_position: Vec3,
        camera_yaw: f32,
        camera_pitch: f32,
        camera_input: CameraInput,
    }

    let mut data = AppData {
        running: true,
        paused: false,
        camera_position: Vec3::ZERO,
        camera_yaw: 0.0,
        camera_pitch: 0.0,
        camera_input: CameraInput {
            forward: false,
            back: false,
            left: false,
            right: false,
            up: false,
            down: false,
            fast: false,
            mouse_delta: Vec2::ZERO,
        },
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
            if e.source() == EventSource::Key {
                let is_pressed = e.event_type() == EventType::Pressed;
                let is_released = e.event_type() == EventType::Released;
                if is_pressed || is_released {
                    match e.key() {
                        KeyCode::W => r.camera_input.forward = is_pressed,
                        KeyCode::S => r.camera_input.back = is_pressed,
                        KeyCode::A => r.camera_input.left = is_pressed,
                        KeyCode::D => r.camera_input.right = is_pressed,
                        KeyCode::E => r.camera_input.up = is_pressed,
                        KeyCode::Q => r.camera_input.down = is_pressed,
                        KeyCode::Shift => r.camera_input.fast = is_pressed,
                        _ => {}
                    }
                }
            }

            if e.source() == EventSource::Mouse && e.event_type() == EventType::Motion2D {
                r.camera_input.mouse_delta += e.motion2d();
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
    let camera_speed = 3.0f32;
    let camera_fast_speed = 9.0f32;
    let camera_sensitivity = 0.003f32;

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

        let mouse_delta = data.camera_input.mouse_delta;
        data.camera_input.mouse_delta = Vec2::ZERO;
        data.camera_yaw += mouse_delta.x * camera_sensitivity;
        data.camera_pitch =
            (data.camera_pitch + mouse_delta.y * camera_sensitivity).clamp(-1.54, 1.54);

        let rotation = Quat::from_axis_angle(Vec3::Y, data.camera_yaw)
            * Quat::from_axis_angle(Vec3::X, data.camera_pitch);
        let mut direction = Vec3::ZERO;
        if data.camera_input.forward {
            direction += rotation * Vec3::NEG_Z;
        }
        if data.camera_input.back {
            direction += rotation * Vec3::Z;
        }
        if data.camera_input.right {
            direction += rotation * Vec3::X;
        }
        if data.camera_input.left {
            direction += rotation * Vec3::NEG_X;
        }
        if data.camera_input.up {
            direction += Vec3::Y;
        }
        if data.camera_input.down {
            direction += Vec3::NEG_Y;
        }
        if direction.length_squared() > 0.0 {
            let speed = if data.camera_input.fast {
                camera_fast_speed
            } else {
                camera_speed
            };
            data.camera_position += direction.normalize() * speed * dt;
        }

        let camera_transform = Mat4::from_rotation_translation(rotation, data.camera_position);
        engine.set_camera_transform(camera, &camera_transform);

        engine.update(dt);
        last_time = now;
    }

    engine.shut_down();
}
