use image::{Rgba, RgbaImage};
use meshi::render::{RenderBackend, RenderEngine, RenderEngineInfo};
use serial_test::serial;
use tempfile::tempdir;

fn expected_triangle(width: u32, height: u32) -> RgbaImage {
    let mut img = RgbaImage::new(width, height);
    let v0 = (width as f32 / 2.0, 0.0f32);
    let v1 = (0.0f32, height as f32 - 1.0);
    let v2 = (width as f32 - 1.0, height as f32 - 1.0);
    for y in 0..height {
        for x in 0..width {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let denom = (v1.1 - v2.1) * (v0.0 - v2.0) + (v2.0 - v1.0) * (v0.1 - v2.1);
            let a = ((v1.1 - v2.1) * (px - v2.0) + (v2.0 - v1.0) * (py - v2.1)) / denom;
            let b = ((v2.1 - v0.1) * (px - v2.0) + (v0.0 - v2.0) * (py - v2.1)) / denom;
            let c = 1.0 - a - b;
            if a >= 0.0 && b >= 0.0 && c >= 0.0 {
                img.put_pixel(x, y, Rgba([255, 0, 0, 255]));
            } else {
                img.put_pixel(x, y, Rgba([0, 0, 0, 255]));
            }
        }
    }
    img
}

fn run_backend(backend: RenderBackend) {
    const EXTENT: [u32; 2] = [64, 64];
    let dir = tempdir().unwrap();
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
    })
    .expect("renderer init");

    render.create_triangle();
    let img = render
        .render_to_image(EXTENT)
        .expect("render to image");
    let expected = expected_triangle(EXTENT[0], EXTENT[1]);
    assert_eq!(img.as_raw(), expected.as_raw());
}

#[test]
#[serial]
fn canvas_red_triangle() {
    run_backend(RenderBackend::Canvas);
}

#[test]
#[serial]
fn graph_red_triangle() {
    run_backend(RenderBackend::Graph);
}
