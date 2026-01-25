use std::ffi::c_void;

use glam::*;
use meshi_ffi_structs::event::*;
use meshi_ffi_structs::{LightFlags, LightInfo, LightType};
use meshi_graphics::*;
use meshi_utils::timer::Timer;
use std::env::*;

#[path = "../common/camera.rs"]
mod common_camera;
#[path = "../common/setup.rs"]
mod common_setup;

use common_camera::CameraController;

fn directional_light(direction: Vec3, color: Vec3, intensity: f32) -> LightInfo {
    LightInfo {
        ty: LightType::Directional,
        flags: LightFlags::CASTS_SHADOWS.bits(),
        intensity,
        range: 0.0,
        color_r: color.x,
        color_g: color.y,
        color_b: color.z,
        pos_x: 0.0,
        pos_y: 0.0,
        pos_z: 0.0,
        dir_x: direction.x,
        dir_y: direction.y,
        dir_z: direction.z,
        spot_inner_angle_rad: 0.0,
        spot_outer_angle_rad: 0.0,
        rect_half_width: 0.0,
        rect_half_height: 0.0,
    }
}

fn point_light(position: Vec3, color: Vec3, intensity: f32, range: f32) -> LightInfo {
    LightInfo {
        ty: LightType::Point,
        flags: LightFlags::NONE.bits(),
        intensity,
        range,
        color_r: color.x,
        color_g: color.y,
        color_b: color.z,
        pos_x: position.x,
        pos_y: position.y,
        pos_z: position.z,
        dir_x: 0.0,
        dir_y: 0.0,
        dir_z: 0.0,
        spot_inner_angle_rad: 0.0,
        spot_outer_angle_rad: 0.0,
        rect_half_width: 0.0,
        rect_half_height: 0.0,
    }
}

fn spot_light(
    position: Vec3,
    direction: Vec3,
    color: Vec3,
    intensity: f32,
    range: f32,
    inner_angle_rad: f32,
    outer_angle_rad: f32,
) -> LightInfo {
    LightInfo {
        ty: LightType::Spot,
        flags: LightFlags::CASTS_SHADOWS.bits(),
        intensity,
        range,
        color_r: color.x,
        color_g: color.y,
        color_b: color.z,
        pos_x: position.x,
        pos_y: position.y,
        pos_z: position.z,
        dir_x: direction.x,
        dir_y: direction.y,
        dir_z: direction.z,
        spot_inner_angle_rad: inner_angle_rad,
        spot_outer_angle_rad: outer_angle_rad,
        rect_half_width: 0.0,
        rect_half_height: 0.0,
    }
}

fn rect_area_light(
    position: Vec3,
    direction: Vec3,
    color: Vec3,
    intensity: f32,
    range: f32,
    half_width: f32,
    half_height: f32,
) -> LightInfo {
    LightInfo {
        ty: LightType::RectArea,
        flags: LightFlags::NONE.bits(),
        intensity,
        range,
        color_r: color.x,
        color_g: color.y,
        color_b: color.z,
        pos_x: position.x,
        pos_y: position.y,
        pos_z: position.z,
        dir_x: direction.x,
        dir_y: direction.y,
        dir_z: direction.z,
        spot_inner_angle_rad: 0.0,
        spot_outer_angle_rad: 0.0,
        rect_half_width: half_width,
        rect_half_height: half_height,
    }
}

fn light_marker_position(light: &LightInfo) -> Option<Vec3> {
    match light.ty {
        LightType::Point | LightType::Spot | LightType::RectArea => {
            Some(Vec3::new(light.pos_x, light.pos_y, light.pos_z))
        }
        _ => None,
    }
}

fn main() {
    tracing_subscriber::fmt::init();
    let args: Vec<String> = args().collect();
    let renderer = common_setup::renderer_from_args(&args, RendererSelect::Deferred);
    let mut setup = common_setup::init(
        "meshi-lighting",
        [800, 600],
        common_setup::CameraSetup::default(),
        renderer,
    );
    setup.engine.set_skybox_settings(SkyboxFrameSettings {
        intensity: 1.0,
        use_procedural_cubemap: true,
        update_interval_frames: 1,
        ..Default::default()
    });
    let mut sky_settings = SkyFrameSettings {
        enabled: true,
        ..Default::default()
    };
    sky_settings.sun_direction = Some(Vec3::new(-0.2, -1.0, -0.3).normalize());
    setup
        .engine
        .set_environment_lighting(EnvironmentLightingSettings {
            sky: sky_settings,
            ..Default::default()
        });
    let mut cloud_settings = setup.engine.cloud_settings();
    cloud_settings.enabled = true;
    setup.engine.set_cloud_settings(cloud_settings);
    setup.engine.set_ocean_settings(OceanFrameSettings {
        enabled: true,
        ..Default::default()
    });

    let model = setup
        .db
        .fetch_gpu_model("model/default")
        .or_else(|_| setup.db.fetch_gpu_model("model/witch"))
        .expect("Expected a default or witch model in the database");

    let model_handle = setup
        .engine
        .register_object(&RenderObjectInfo::Model(model))
        .unwrap();

    let translation = Mat4::from_translation(Vec3::new(0.0, -0.2, -2.8));
    setup.engine.set_object_transform(model_handle, &translation);

    let lights = [
        directional_light(Vec3::new(-0.2, -70.0, -0.3), Vec3::splat(1.0), 1.4),
        point_light(Vec3::new(1.0, 0.3, -60.0), Vec3::new(1.0, 0.2, 0.2), 7.0, 6.0),
        spot_light(
            Vec3::new(-20.1, 1.1, -2.3),
            Vec3::new(0.2, -1.0, -0.2),
            Vec3::new(0.2, 0.8, 1.0),
            9.0,
            8.0,
            12.0_f32.to_radians(),
            28.0_f32.to_radians(),
        ),
        rect_area_light(
            Vec3::new(0.2, 1.2, -30.0),
            Vec3::new(0.0, -2.0, 0.0),
            Vec3::new(0.9, 0.8, 0.5),
            6.0,
            5.0,
            0.6,
            0.4,
        ),
    ];

    for light in &lights {
        setup.engine.register_light(light);
    }

    let marker_model = setup
        .db
        .fetch_gpu_model("model/sphere")
        .or_else(|_| setup.db.fetch_gpu_model("model/icosphere"))
        .or_else(|_| setup.db.fetch_gpu_model("model/cube"))
        .or_else(|_| setup.db.fetch_gpu_model("model/default"))
        .or_else(|_| setup.db.fetch_gpu_model("model/witch"))
        .expect("Expected a sphere-like model in the database");
    let mut light_markers = Vec::new();
    for light in &lights {
        if let Some(position) = light_marker_position(light) {
            let handle = setup
                .engine
                .register_object(&RenderObjectInfo::Model(marker_model.clone()))
                .unwrap();
            let marker_transform =
                Mat4::from_translation(position) * Mat4::from_scale(Vec3::splat(4.0));
            setup.engine.set_object_transform(handle, &marker_transform);
            light_markers.push(handle);
        }
    }

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
    let angular_velocity = 0.6f32;

    while data.running {
        let now = timer.elapsed_seconds_f32();
        let mut dt = now - last_time;
        dt = dt.min(1.0 / 30.0);
        if !data.paused {
            total_time += dt;
            let rotation = Mat4::from_rotation_y(angular_velocity * total_time);
            let transform = translation * rotation;
            setup.engine.set_object_transform(model_handle, &transform);
        }

        let camera_transform = data.camera.update(dt);
        setup.engine.set_camera_transform(setup.camera, &camera_transform);

        setup.engine.update(dt);
        last_time = now;
    }

    setup.engine.shut_down();
}
