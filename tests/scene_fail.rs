use meshi::render::{RenderBackend, RenderEngine, RenderEngineInfo, SceneInfo};
use std::fs;

#[test]
fn records_missing_resources() {
    // Create temporary directory with minimal database.
    let dir = tempfile::tempdir().unwrap();
    let db_dir = dir.path().join("database");
    fs::create_dir(&db_dir).unwrap();
    fs::write(db_dir.join("db.json"), "{}").unwrap();

    let mut render = RenderEngine::new(&RenderEngineInfo {
        application_path: dir.path().to_str().unwrap().into(),
        scene_info: None,
        headless: true,
        backend: RenderBackend::Canvas,
    })
    .unwrap();

    let info = SceneInfo {
        models: &["missing_model.gltf"],
        images: &["missing.png"],
    };
    render.set_scene(&info).unwrap();

    let errors = render.scene_load_errors();
    assert_eq!(errors.models, vec!["missing_model.gltf".to_string()]);
    assert_eq!(errors.images, vec!["missing.png".to_string()]);
}
