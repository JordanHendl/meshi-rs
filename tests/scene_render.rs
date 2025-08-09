use image::{Rgba, RgbaImage};
use meshi::render::{RenderBackend, RenderEngine, RenderEngineInfo, SceneInfo};
use serial_test::serial;
use tempfile::tempdir;

fn run_backend(backend: RenderBackend) {
    const EXTENT: [u32; 2] = [64, 64];
    let dir = tempdir().unwrap();
    let base = dir.path();
    let db_dir = base.join("database");
    std::fs::create_dir(&db_dir).unwrap();
    std::fs::write(db_dir.join("db.json"), "{}").unwrap();
    std::fs::write(base.join("koji.json"), "{\"nodes\":[],\"edges\":[]}").unwrap();

    let bin_path = db_dir.join("data.bin");
    let mut bin = Vec::new();
    for f in [
        0.0f32, 0.0, 0.0, // v0
        1.0, 0.0, 0.0, // v1
        0.0, 1.0, 0.0, // v2
    ] {
        bin.extend_from_slice(&f.to_le_bytes());
    }
    for i in [0u16, 1, 2] {
        bin.extend_from_slice(&i.to_le_bytes());
    }
    std::fs::write(&bin_path, &bin).unwrap();
    let gltf = format!(
        "{{\n  \"asset\": {{ \"version\": \"2.0\" }},\n  \"scenes\": [{{ \"nodes\": [0] }}],\n  \"scene\": 0,\n  \"nodes\": [{{\"mesh\": 0}}],\n  \"meshes\": [{{ \"primitives\": [{{ \"attributes\": {{ \"POSITION\": 0 }}, \"indices\": 1 }}] }}],\n  \"buffers\": [{{ \"uri\": \"data.bin\", \"byteLength\": {} }}],\n  \"bufferViews\": [{{ \"buffer\": 0, \"byteOffset\": 0, \"byteLength\": 36 }}, {{ \"buffer\": 0, \"byteOffset\": 36, \"byteLength\": 6 }}],\n  \"accessors\": [{{ \"bufferView\": 0, \"componentType\": 5126, \"count\": 3, \"type\": \"VEC3\", \"min\": [0.0,0.0,0.0], \"max\": [1.0,1.0,0.0] }}, {{ \"bufferView\": 1, \"componentType\": 5123, \"count\": 3, \"type\": \"SCALAR\" }}]\n}}",
        bin.len()
    );
    std::fs::write(db_dir.join("model.gltf"), gltf).unwrap();

    let img_path = db_dir.join("albedo.png");
    let mut img = RgbaImage::new(1, 1);
    img.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
    img.save(&img_path).unwrap();

    let mut render = RenderEngine::new(&RenderEngineInfo {
        application_path: base.to_str().unwrap().into(),
        scene_info: None,
       headless: true,
        backend,
        canvas_extent: None,
    })
    .expect("renderer init");

    let scene_info = SceneInfo {
        models: &["model.gltf"],
        images: &["albedo.png"],
    };
    render.set_scene(&scene_info).expect("scene loading failed");

    let img = render.render_to_image(EXTENT).expect("render to image");
    assert!(img.as_raw().iter().any(|&b| b != 0));
}

#[test]
#[serial]
fn canvas_scene_renders() {
    run_backend(RenderBackend::Canvas);
}

#[test]
#[serial]
fn graph_scene_renders() {
    run_backend(RenderBackend::Graph);
}
