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
        "meshi-skinned",
        [512, 512],
        common_setup::CameraSetup {
            transform: Mat4::from_translation(Vec3::new(4.0, 0.0, 0.0)),
            far: 10_000.0,
            ..Default::default()
        },
        renderer,
    );

    let model = setup.db.fetch_gpu_model("model/fox").unwrap();
    let animation_names = model
        .rig
        .as_ref()
        .map(|rig| {
            let mut names: Vec<String> = rig.animations.keys().cloned().collect();
            names.sort();
            names
        })
        .unwrap_or_default();
    let skinned = setup
        .engine
        .register_object(&RenderObjectInfo::SkinnedModel(SkinnedModelInfo {
            model,
            animation: AnimationState::default(),
        }))
        .unwrap();

    let translation = Mat4::from_translation(Vec3::new(0.0, 0.0, -5.0));
    setup.engine.set_object_transform(skinned, &translation);

    let status_text = setup.engine.register_text(&TextInfo {
        text: "Animation: -- (use ←/→ to switch)".to_string(),
        position: Vec2::new(12.0, 12.0),
        color: Vec4::ONE,
        scale: 2.0,
        render_mode: common_setup::text_render_mode(&setup.db),
    });

    struct AppData {
        running: bool,
        animation_index: usize,
        animation_count: usize,
        animation_changed: bool,
        camera: CameraController,
    }

    let mut data = AppData {
        running: true,
        animation_index: 0,
        animation_count: animation_names.len(),
        animation_changed: false,
        camera: CameraController::new(Vec3::new(4.0, 0.0, 0.0), setup.window_size),
    };

    assert!(data.animation_count > 0);

    if data.animation_count > 0 {
        setup.engine.set_skinned_object_animation(
            skinned,
            AnimationState {
                clip_index: data.animation_index as u32,
                time_seconds: 0.0,
                speed: 1.0,
                looping: true,
            },
        );
        if let Some(name) = animation_names.get(data.animation_index) {
            setup.engine.set_text(
                status_text,
                &format!("Animation: {} (use ←/→ to switch)", name),
            );
        }
    }
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
            r.camera.handle_event(e);

            if e.source() == EventSource::Window {
                if e.event_type() == EventType::Quit {
                    r.running = false;
                }
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
        let mut dt = now - last_time;
        dt = dt.min(1.0 / 30.0);
        if data.animation_changed {
            setup.engine.set_skinned_object_animation(
                skinned,
                AnimationState {
                    clip_index: data.animation_index as u32,
                    time_seconds: 0.0,
                    speed: 1.0,
                    looping: true,
                },
            );
            if let Some(name) = animation_names.get(data.animation_index) {
                setup.engine.set_text(
                    status_text,
                    &format!("Animation: {} (use ←/→ to switch)", name),
                );
            }
            data.animation_changed = false;
        }

        let camera_transform = data.camera.update(dt);
        setup
            .engine
            .set_camera_transform(setup.camera, &camera_transform);

        setup.engine.update(dt);
        last_time = now;
    }

    setup.engine.shut_down();
}
