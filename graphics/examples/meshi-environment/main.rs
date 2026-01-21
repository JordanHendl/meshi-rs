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
        environment_text: Handle<TextObject>,
    }

    let mut data = AppData {
        running: true,
        camera: CameraController::new(Vec3::new(0.0, 6.0, 8.0), setup.window_size),
        _instruction_text: instruction_text,
        environment_text: setup.engine.register_text(&TextInfo {
            text: "Environment lighting: initializing...".to_string(),
            position: Vec2::new(12.0, 40.0),
            color: Vec4::new(0.9, 0.95, 1.0, 1.0),
            scale: 1.2,
            render_mode: common_setup::text_render_mode(&setup.db),
        }),
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
    let day_length_seconds = 120.0;
    let latitude_degrees = 37.0;
    let longitude_degrees = 0.0;

    setup.engine.set_skybox_settings(SkyboxFrameSettings {
        intensity: 1.0,
        use_procedural_cubemap: true,
        update_interval_frames: 1,
        ..Default::default()
    });

    while data.running {
        let now = timer.elapsed_seconds_f32();
        let dt = (now - last_time).min(1.0 / 30.0);
        let time_of_day = (now / day_length_seconds * 24.0) % 24.0;
        let sun_dir = celestial_direction(time_of_day, latitude_degrees, longitude_degrees, false);
        let moon_dir = celestial_direction(time_of_day, latitude_degrees, longitude_degrees, true);
        let camera_transform = data.camera.update(dt);
        setup
            .engine
            .set_camera_transform(setup.camera, &camera_transform);

        setup
            .engine
            .set_environment_lighting(EnvironmentLightingSettings {
                sky: SkyFrameSettings {
                    sun_direction: None,
                    sun_color: Vec3::new(1.0, 0.95, 0.85),
                    sun_intensity: 8.0,
                    sun_angular_radius: 0.00465,
                    moon_direction: None,
                    moon_color: Vec3::new(0.6, 0.7, 1.0),
                    moon_intensity: 0.25,
                    moon_angular_radius: 0.0026,
                    time_of_day: Some(time_of_day),
                    latitude_degrees: Some(latitude_degrees),
                    longitude_degrees: Some(longitude_degrees),
                },
                sun_light_intensity: 3.5,
                moon_light_intensity: 0.4,
            });

        setup.engine.set_text(
            data.environment_text,
            &format!(
                "Time: {time_of_day:05.2}h | Sun dir: [{:.2}, {:.2}, {:.2}] | Moon dir: [{:.2}, {:.2}, {:.2}]",
                sun_dir.x, sun_dir.y, sun_dir.z, moon_dir.x, moon_dir.y, moon_dir.z
            ),
        );
        setup.engine.update(dt);
        last_time = now;
    }

    setup.engine.shut_down();
}

fn celestial_direction(
    time_of_day: f32,
    latitude_degrees: f32,
    longitude_degrees: f32,
    is_moon: bool,
) -> Vec3 {
    let day_time = time_of_day.rem_euclid(24.0);
    let angle = day_time / 24.0 * std::f32::consts::TAU;
    let elevation = (angle - std::f32::consts::FRAC_PI_2).sin();
    let base = Vec3::new(angle.cos(), elevation, angle.sin());
    let latitude = latitude_degrees.to_radians();
    let longitude = longitude_degrees.to_radians();
    let rotation = Mat3::from_rotation_y(longitude) * Mat3::from_rotation_x(latitude);
    let mut dir = rotation * base;
    if is_moon {
        dir = -dir;
    }
    if dir.length_squared() > 0.0 {
        dir.normalize()
    } else if is_moon {
        -Vec3::Y
    } else {
        Vec3::Y
    }
}
