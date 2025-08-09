use image::{Rgba, RgbaImage};
use meshi::render::{RenderBackend, RenderEngine, RenderEngineInfo, SceneInfo};
use std::fs;

fn main() {
    // Create a temporary directory for the database resources.
    let mut dir = std::env::temp_dir();
    dir.push("meshi_scene_test");
    // Ensure a unique directory per run.
    dir.push(format!(
        "{}",
        std::time::SystemTime::now().elapsed().unwrap().as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    let db_dir = dir.join("database");
    fs::create_dir_all(&db_dir).unwrap();

    // Minimal db.json so Database::new succeeds.
    fs::write(db_dir.join("db.json"), "{}").unwrap();

    // Create a minimal valid glTF model the database can parse.
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
    fs::write(&bin_path, &bin).unwrap();
    let gltf = format!(
        "{{\n  \"asset\": {{ \"version\": \"2.0\" }},\n  \"scenes\": [{{ \"nodes\": [0] }}],\n  \"scene\": 0,\n  \"nodes\": [{{ \"mesh\": 0 }}],\n  \"meshes\": [{{ \"primitives\": [{{ \"attributes\": {{ \"POSITION\": 0 }}, \"indices\": 1 }}] }}],\n  \"buffers\": [{{ \"uri\": \"data.bin\", \"byteLength\": {} }}],\n  \"bufferViews\": [{{ \"buffer\": 0, \"byteOffset\": 0, \"byteLength\": 36 }}, {{ \"buffer\": 0, \"byteOffset\": 36, \"byteLength\": 6 }}],\n  \"accessors\": [{{ \"bufferView\": 0, \"componentType\": 5126, \"count\": 3, \"type\": \"VEC3\", \"min\": [0.0,0.0,0.0], \"max\": [1.0,1.0,0.0] }}, {{ \"bufferView\": 1, \"componentType\": 5123, \"count\": 3, \"type\": \"SCALAR\" }}]\n}}",
        bin.len()
    );
    fs::write(db_dir.join("model.gltf"), gltf).unwrap();

    // Dummy image file using the image crate.
    let img_path = db_dir.join("albedo.png");
    let mut img = RgbaImage::new(1, 1);
    img.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
    img.save(&img_path).unwrap();

    // Initialise renderer pointing at our temp directory.
    let mut render = RenderEngine::new(&RenderEngineInfo {
        application_path: dir.to_str().unwrap().into(),
        scene_info: None,
        headless: true,
        backend: RenderBackend::Canvas,
        canvas_extent: None,
    })
    .expect("failed to initialize renderer");

    // Configure the scene.
    let scene_info = SceneInfo {
        models: &["model.gltf"],
        images: &["albedo.png"],
    };

    // Ensure loading succeeds.
    render.set_scene(&scene_info).expect("scene loading failed");
    // Prevent destructor from running to avoid allocation assertions.
    std::mem::forget(render);
}
