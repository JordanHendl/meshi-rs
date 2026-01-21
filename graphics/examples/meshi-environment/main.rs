use std::ffi::c_void;

use dashi::Handle;
use glam::*;
use meshi_ffi_structs::event::*;
use meshi_graphics::*;
use meshi_utils::timer::Timer;

#[path = "../common/camera.rs"]
mod common_camera;
#[path = "../common/setup.rs"]
mod common_setup;

use common_camera::CameraController;

fn main() {
    tracing_subscriber::fmt::init();
    let renderer = RendererSelect::Deferred;
    let mut setup = common_setup::init(
        "meshi-environment",
        [1280, 720],
        common_setup::CameraSetup {
            transform: Mat4::from_translation(Vec3::new(0.0, 6.0, 8.0)),
            far: 50_000.0,
            ..Default::default()
        },
        renderer,
    );

    let instruction_text = setup.engine.register_text(&TextInfo {
        text: "Hold Space + WASDQE to fly. Mouse to look around.".to_string(),
        position: Vec2::new(12.0, 12.0),
        color: Vec4::ONE,
        scale: 1.6,
        render_mode: common_setup::text_render_mode(&setup.db),
    });

    struct AppData {
        running: bool,
        camera: CameraController,
        _instruction_text: Handle<TextObject>,
    }

    let mut data = AppData {
        running: true,
        camera: CameraController::new(Vec3::new(0.0, 6.0, 8.0), setup.window_size),
        _instruction_text: instruction_text,
    };

    extern "C" fn callback(event: *mut Event, data: *mut c_void) {
        unsafe {
            let e = &mut (*event);
            let r = &mut (*(data as *mut AppData));
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

    while data.running {
        let now = timer.elapsed_seconds_f32();
        let dt = (now - last_time).min(1.0 / 30.0);
        let camera_transform = data.camera.update(dt);
        setup
            .engine
            .set_camera_transform(setup.camera, &camera_transform);
        setup.engine.update(dt);
        last_time = now;
    }

    setup.engine.shut_down();
}
