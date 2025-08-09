use image::RgbaImage;
use meshi::render::{RenderBackend, RenderEngine, RenderEngineInfo, SceneInfo};
use serial_test::serial;
use tempfile::tempdir;

fn render_triangle(backend: RenderBackend) -> RgbaImage {
    const EXTENT: [u32; 2] = [64, 64];
    let dir = tempdir().expect("temp dir");
    let base = dir.path();
    let db_dir = base.join("database");
    std::fs::create_dir(&db_dir).unwrap();
    std::fs::write(db_dir.join("db.json"), "{}".as_bytes()).unwrap();
    std::fs::write(base.join("koji.json"), "{\"nodes\":[],\"edges\":[]}".as_bytes()).unwrap();

    let mut render = RenderEngine::new(&RenderEngineInfo {
        application_path: base.to_str().unwrap().into(),
        scene_info: None,
        headless: true,
        backend,
        canvas_extent: None,
    })
    .expect("renderer init");

    let scene_info = SceneInfo { models: &[], images: &[] };
    render.set_scene(&scene_info).expect("scene load");

    render.create_triangle();
    render.render_to_image(EXTENT).expect("render to image")
}

#[test]
#[serial]
fn graph_backend_matches_canvas() {
    let canvas = render_triangle(RenderBackend::Canvas);
    let graph = render_triangle(RenderBackend::Graph);
    assert_eq!(canvas.as_raw(), graph.as_raw());
}

