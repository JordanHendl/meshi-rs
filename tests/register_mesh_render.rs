use meshi::render::{RenderBackend, RenderEngine, RenderEngineInfo};
use serial_test::serial;
use tempfile::tempdir;

fn run_backend(backend: RenderBackend) {
    let dir = tempdir().unwrap();
    let base = dir.path();
    let db_dir = base.join("database");
    std::fs::create_dir(&db_dir).unwrap();
    std::fs::write(db_dir.join("db.json"), "{}").unwrap();
    std::fs::write(base.join("koji.json"), "{\"nodes\":[],\"edges\":[]}").unwrap();

    let mut render = RenderEngine::new(&RenderEngineInfo {
        application_path: base.to_str().unwrap().into(),
        scene_info: None,
        headless: true,
        backend,
        canvas_extent: None,
    })
    .expect("renderer init");

    let handle = render.create_triangle();
    render.update(0.0);
    render.register_mesh_with_renderer(handle);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        render.update(0.0);
    }))
    .is_ok());

    render.shut_down();
}

#[test]
#[serial]
fn canvas_register_mesh_and_render() {
    run_backend(RenderBackend::Canvas);
}

#[test]
#[serial]
fn graph_register_mesh_and_render() {
    run_backend(RenderBackend::Graph);
}
