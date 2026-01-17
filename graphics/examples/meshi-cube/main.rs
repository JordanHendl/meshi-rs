use std::ffi::c_void;

use glam::*;
use meshi_ffi_structs::event::*;
use meshi_graphics::*;
use meshi_utils::timer::Timer;
use std::env::*;

#[path = "../common/camera.rs"]
mod common_camera;
#[path = "../common/setup.rs"]
mod common_setup;

use common_camera::CameraController;

fn main() {
    tracing_subscriber::fmt::init();
    let args: Vec<String> = args().collect();
    let renderer = common_setup::renderer_from_args(&args, RendererSelect::Deferred);
    let mut setup = common_setup::init(
        "meshi-cube",
        [512, 512],
        common_setup::CameraSetup::default(),
        renderer,
    );

    let text_handle = setup.engine.register_text(&TextInfo {
        text: "avg fps: --".to_string(),
        position: Vec2::new(12.0, 12.0),
        color: Vec4::ONE,
        scale: 2.0,
        render_mode: common_setup::text_render_mode(&setup.db),
    });

    let quad_model = setup
        .db
        .fetch_gpu_model("model/cube")
        .or_else(|_| setup.db.fetch_gpu_model("model/plane"))
        .or_else(|_| setup.db.fetch_gpu_model("model/witch"))
        .expect("Expected a quad-like model in the database");
    let quad = setup
        .engine
        .register_object(&RenderObjectInfo::Model(quad_model))
        .unwrap();

    let translation = Mat4::from_translation(Vec3::new(0.0, 0.25, -2.5));
    let mut transform = translation;
    // Update object transform to be the center.
    setup.engine.set_object_transform(quad, &transform);

    struct AppData {
        running: bool,
        paused: bool,
        camera: CameraController,
    }

    let mut data = AppData {
        running: true,
        paused: false,
        camera: CameraController::new(Vec3::ZERO, setup.window_size),
    };

    extern "C" fn callback(event: *mut Event, data: *mut c_void) {
        unsafe {
            let e = &mut (*event);
            let r = &mut (*(data as *mut AppData));
            if e.source() == EventSource::Key && e.event_type() == EventType::Pressed {
                if e.key() == KeyCode::Enter {
                    r.paused = !r.paused;
                }
            }
            r.camera.handle_event(e);

            if e.event_type() == EventType::Quit {
                r.running = false;
            }
        }
    }

    setup
        .engine
        .set_event_cb(callback, (&mut data as *mut AppData) as *mut c_void);
    let mut timer = Timer::new();
    timer.start();
    let mut last_time = timer.elapsed_seconds_f32();
    let mut total_time = 0.0f32;
    let angular_velocity = 2.0f32;
    
    while data.running {
        if let Some(avg_ms) = setup.engine.average_frame_time_ms() {
            let avg_fps = 1000.0 / avg_ms;
            let text = format!("avg fps: {:.1}", avg_fps);
            setup.engine.set_text(text_handle, &text);
        }
        let now = timer.elapsed_seconds_f32();
        let mut dt = now - last_time;
        dt = dt.min(1.0 / 30.0);
        if !data.paused {
            total_time += dt;
            let mut rotation = Mat4::from_rotation_y(angular_velocity * total_time);
            rotation = rotation * Mat4::from_rotation_x(angular_velocity * total_time);
            transform = translation * rotation;
            setup.engine.set_object_transform(quad, &transform);
        }

        let camera_transform = data.camera.update(dt);
        setup.engine.set_camera_transform(setup.camera, &camera_transform);

        setup.engine.update(dt);
        last_time = now;
    }

    setup.engine.shut_down();
}
