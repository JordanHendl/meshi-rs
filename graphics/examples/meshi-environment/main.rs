use std::ffi::c_void;

use dashi::Handle;
use glam::*;
use meshi_ffi_structs::event::*;
use meshi_graphics::*;
use meshi_utils::timer::Timer;
use noren::rdb::terrain::{parse_chunk_artifact_entry, project_settings_entry};
use tracing::warn;

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
            near: 0.5,
            far: 1000.0,
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

    setup.engine.set_cloud_weather_map(None);

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
    let timer_speed = 24.0 / day_length_seconds;

    setup.engine.set_skybox_settings(SkyboxFrameSettings {
        intensity: 1.0,
        use_procedural_cubemap: true,
        update_interval_frames: 1,
        ..Default::default()
    });
    let mut cloud_settings = setup.engine.cloud_settings();
    cloud_settings.enabled = true;
    setup.engine.set_cloud_settings(cloud_settings);
    setup.engine.set_ocean_settings(OceanFrameSettings {
        enabled: true,
        wind_speed: 2.0,
        wave_amplitude: 4.0,
        gerstner_amplitude: 0.35,
        cascade_spectrum_scales: [500.0, 1.0, 0.8],
        ..Default::default()
    });

    if let Some(project_key) = terrain_project_key_from_db(&setup.db) {
        let settings_entry = project_settings_entry(&project_key);
        match setup
            .db
            .terrain_mut()
            .fetch_project_settings(&settings_entry)
        {
            Ok(settings) => {
                let mut render_objects = Vec::new();
                for entry in setup.db.enumerate_terrain_chunks() {
                    match setup.db.terrain_mut().fetch_chunk_artifact(&entry) {
                        Ok(artifact) => {
                            render_objects.push(
                                meshi_graphics::terrain_loader::terrain_render_object_from_artifact(
                                    &settings,
                                    "iceland".to_string(),
                                    artifact,
                                ),
                            );
                        }
                        Err(err) => {
                            warn!("Failed to load terrain artifact '{entry}': {err:?}");
                        }
                    }
                }

                if render_objects.is_empty() {
                    warn!("No terrain artifacts found for project '{project_key}'.");
                } else {
                    setup.engine.set_terrain_render_objects(&render_objects);
                }
            }
            Err(err) => {
                warn!("Failed to load terrain settings '{settings_entry}': {err:?}");
            }
        }
    }

    setup
        .engine
        .set_environment_lighting(EnvironmentLightingSettings {
            sky: SkyFrameSettings {
                enabled: true,
                sun_direction: None,
                sun_color: Vec3::new(1.0, 0.95, 0.85),
                sun_intensity: 8.0,
                sun_angular_radius: 0.0035,
                moon_direction: None,
                moon_color: Vec3::new(0.6, 0.7, 1.0),
                moon_intensity: 0.25,
                moon_angular_radius: 0.0045,
                time_of_day: None,
                timer_speed,
                current_time_of_day: 6.0,
                auto_sun_enabled: true,
                latitude_degrees: Some(latitude_degrees),
                longitude_degrees: Some(longitude_degrees),
            },
            sun_light_intensity: 3.5,
            moon_light_intensity: 0.4,
        });

    while data.running {
        let now = timer.elapsed_seconds_f32();
        let dt = (now - last_time).min(1.0 / 30.0);
        let camera_transform = data.camera.update(dt);
        setup
            .engine
            .set_camera_transform(setup.camera, &camera_transform);

        setup.engine.set_text(
            data.environment_text,
            &format!(
                "Environment lighting: automatic day cycle ({:.1} seconds per day).",
                day_length_seconds
            ),
        );
        setup.engine.update(dt);
        last_time = now;
    }

    setup.engine.shut_down();
}

fn terrain_project_key_from_db(db: &DB) -> Option<String> {
    db.enumerate_terrain_chunks()
        .into_iter()
        .find_map(|entry| parse_chunk_artifact_entry(&entry).map(|parsed| parsed.project_key))
}
