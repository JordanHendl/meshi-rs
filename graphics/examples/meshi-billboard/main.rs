use std::ffi::c_void;

use dashi::Handle;
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
        "meshi-billboard",
        [768, 512],
        common_setup::CameraSetup::default(),
        renderer,
    );

    let hint_text = setup.engine.register_text(&TextInfo {
        text: "Hold Space + WASDQE to move around billboards.".to_string(),
        position: Vec2::new(12.0, 12.0),
        color: Vec4::ONE,
        scale: 1.5,
        render_mode: common_setup::text_render_mode(&setup.db),
    });

    let mut billboards = Vec::new();
    for z in -2..=2 {
        for x in -2..=2 {

            let ty = if x == -2 {
                BillboardType::Fixed
            } else if  x == 1 {
                BillboardType::AxisAligned
            } else {
                BillboardType::ScreenAligned
            };

            let billboard = setup
                .engine
                .register_object(&RenderObjectInfo::Billboard(BillboardInfo {
                    texture_id: 0,
                    material: None,
                    billboard_type: ty,
                }))
                .unwrap();
            let translation = Mat4::from_translation(Vec3::new(
                x as f32 * 1.5,
                0.5,
                -3.0 + z as f32 * 1.5,
            ));
            setup.engine.set_object_transform(billboard, &translation);
            billboards.push(billboard);
        }
    }

    struct AppData {
        running: bool,
        camera: CameraController,
        _hint_text: Handle<TextObject>,
        _billboards: Vec<Handle<RenderObject>>,
    }

    let mut data = AppData {
        running: true,
        camera: CameraController::new(Vec3::ZERO, setup.window_size),
        _hint_text: hint_text,
        _billboards: billboards,
    };

    extern "C" fn callback(event: *mut Event, data: *mut c_void) {
        unsafe {
            let e = &mut (*event);
            let r = &mut (*(data as *mut AppData));
            if e.source() == EventSource::Window && e.event_type() == EventType::Quit {
                r.running = false;
            }
            r.camera.handle_event(e);
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
