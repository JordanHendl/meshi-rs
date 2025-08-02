use meshi::render::{RenderEngine, RenderEngineInfo, SceneInfo};
use std::fs;
use image::{RgbaImage, Rgba};

fn main() {
    // Create a temporary directory for the database resources.
    let mut dir = std::env::temp_dir();
    dir.push("meshi_scene_test");
    // Ensure a unique directory per run.
    dir.push(format!("{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos()));
    fs::create_dir_all(&dir).unwrap();
    let db_dir = dir.join("database");
    fs::create_dir_all(&db_dir).unwrap();

    // Minimal db.json so Database::new succeeds.
    fs::write(db_dir.join("db.json"), "{}").unwrap();

    // Dummy model file.
    fs::write(db_dir.join("model.gltf"), b"test").unwrap();

    // Dummy image file using the image crate.
    let img_path = db_dir.join("albedo.png");
    let mut img = RgbaImage::new(1,1);
    img.put_pixel(0,0,Rgba([255,0,0,255]));
    img.save(&img_path).unwrap();

    // Initialise renderer pointing at our temp directory.
    let mut render = RenderEngine::new(&RenderEngineInfo {
        application_path: dir.to_str().unwrap().into(),
        scene_info: None,
        headless: true,
    });

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
