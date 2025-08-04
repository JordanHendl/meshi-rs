use std::ffi::CString;
use std::time::Duration;

use glam::Mat4;
use meshi::render::{RenderBackend, RenderEngine, RenderEngineInfo, SceneInfo};
use meshi::FFIMeshObjectInfo;

fn main() {
    // Select backend from the first argument: `canvas` (default) or `graph`.
    let backend = match std::env::args().nth(1).as_deref() {
        Some("graph") => RenderBackend::Graph,
        _ => RenderBackend::Canvas,
    };

    let mut render = RenderEngine::new(&RenderEngineInfo {
        application_path: env!("CARGO_MANIFEST_DIR").into(),
        scene_info: None,
        headless: false,
        backend,
    })
    .expect("failed to initialize renderer");

    // Load a model and texture from the database directory.
    let scene = SceneInfo {
        models: &["model.gltf"],
        images: &["albedo.png"],
    };
    render.set_scene(&scene).expect("scene loading failed");

    // Register the loaded mesh for drawing using its associated texture.
    let mesh = CString::new("model.gltf").unwrap();
    let tex = CString::new("albedo.png").unwrap();
    let info = FFIMeshObjectInfo {
        mesh: mesh.as_ptr(),
        material: tex.as_ptr(),
        transform: Mat4::IDENTITY,
    };
    render
        .register_mesh_object(&info)
        .expect("failed to register mesh object");

    // Run a small render loop to display the scene.
    for _ in 0..120 {
        render.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(16));
    }
}
