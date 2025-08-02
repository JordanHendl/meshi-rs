use meshi::render::{RenderEngine, RenderEngineInfo};
use tempfile::tempdir;
use std::fs;

fn main() {
    // Skip in headless environments similar to other tests.
    if std::env::var("DISPLAY").is_err() && std::env::var("WAYLAND_DISPLAY").is_err() {
        return;
    }

    let dir = tempdir().unwrap();
    let db_dir = dir.path().join("database");
    fs::create_dir_all(&db_dir).unwrap();
    fs::write(db_dir.join("db.json"), "{}").unwrap();

    let mut render = RenderEngine::new(&RenderEngineInfo {
        application_path: dir.path().to_str().unwrap().into(),
        scene_info: None,
        headless: true,
    });

    let mesh = render.insert_dummy_mesh_object();
    let light = render.insert_dummy_directional_light();
    assert!(render.has_mesh_object(mesh));
    assert!(render.has_directional_light(light));
    render.release_mesh_object(mesh);
    render.release_directional_light(light);
    assert!(!render.has_mesh_object(mesh));
    assert!(!render.has_directional_light(light));
}
