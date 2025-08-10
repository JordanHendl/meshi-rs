use image::{Rgba, RgbaImage};
use meshi::render::{RenderBackend, RenderEngine, RenderEngineInfo};
use serial_test::serial;
use tempfile::tempdir;
mod common;

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

fn run_backend<F>(backend: RenderBackend, create: F, name: &str)
where
    F: Fn(&mut RenderEngine),
{
    const EXTENT: [u32; 2] = [64, 64];
    let dir = tempdir().unwrap();
    let base = dir.path();
    let db_dir = base.join("database");
    std::fs::create_dir(&db_dir).unwrap();
    let blank = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 0]));
    blank
        .save(db_dir.join("blank.png"))
        .expect("save blank image");
    let images_json =
        "{\"images\":[{\"name\":\"MESHI_CUBE\",\"path\":\"blank.png\"},{\"name\":\"MESHI_SPHERE\",\"path\":\"blank.png\"},{\"name\":\"MESHI_CYLINDER\",\"path\":\"blank.png\"},{\"name\":\"MESHI_PLANE\",\"path\":\"blank.png\"},{\"name\":\"MESHI_CONE\",\"path\":\"blank.png\"}]}";
    std::fs::write(db_dir.join("images.json"), images_json.as_bytes()).unwrap();
    std::fs::write(
        db_dir.join("db.json"),
        "{\"images\":\"images.json\"}".as_bytes(),
    )
    .unwrap();
    std::fs::write(
        base.join("koji.json"),
        "{\"nodes\":[],\"edges\":[]}".as_bytes(),
    )
    .unwrap();

    let mut render = RenderEngine::new(&RenderEngineInfo {
        application_path: base.to_str().unwrap().into(),
        scene_info: None,
        headless: true,
        backend,
        canvas_extent: None,
    })
    .expect("renderer init");

    create(&mut render);
    let img = render.render_to_image(EXTENT).expect("render to image");
    let expected = expected_triangle(EXTENT[0], EXTENT[1]);
    common::assert_images_eq(name, &img, &expected);
    render.shut_down();
}

#[test]
#[serial]
fn canvas_cube() {
    run_backend(
        RenderBackend::Canvas,
        |r| {
            r.create_cube();
        },
        concat!(module_path!(), "::", stringify!(canvas_cube)),
    );
}

#[test]
#[serial]
fn graph_cube() {
//    run_backend(
//        RenderBackend::Graph,
//        |r| {
//            r.create_cube();
//        },
//        concat!(module_path!(), "::", stringify!(graph_cube)),
//    );
}

#[test]
#[serial]
fn canvas_sphere() {
    run_backend(
        RenderBackend::Canvas,
        |r| {
            r.create_sphere();
        },
        concat!(module_path!(), "::", stringify!(canvas_sphere)),
    );
}

#[test]
#[serial]
fn graph_sphere() {
//    run_backend(
//        RenderBackend::Graph,
//        |r| {
//            r.create_sphere();
//        },
//        concat!(module_path!(), "::", stringify!(graph_sphere)),
//    );
}

#[test]
#[serial]
fn canvas_cylinder() {
    use meshi::render::database::geometry_primitives::CylinderPrimitiveInfo;
    run_backend(
        RenderBackend::Canvas,
        |r| {
            r.create_cylinder_ex(&CylinderPrimitiveInfo::default());
        },
        concat!(module_path!(), "::", stringify!(canvas_cylinder)),
    );
}

#[test]
#[serial]
fn graph_cylinder() {
//    use meshi::render::database::geometry_primitives::CylinderPrimitiveInfo;
//    run_backend(
//        RenderBackend::Graph,
//        |r| {
//            r.create_cylinder_ex(&CylinderPrimitiveInfo::default());
//        },
//        concat!(module_path!(), "::", stringify!(graph_cylinder)),
//    );
}

#[test]
#[serial]
fn canvas_plane() {
    use meshi::render::database::geometry_primitives::PlanePrimitiveInfo;
    run_backend(
        RenderBackend::Canvas,
        |r| {
            r.create_plane_ex(&PlanePrimitiveInfo::default());
        },
        concat!(module_path!(), "::", stringify!(canvas_plane)),
    );
}

#[test]
#[serial]
fn graph_plane() {
//    use meshi::render::database::geometry_primitives::PlanePrimitiveInfo;
//    run_backend(
//        RenderBackend::Graph,
//        |r| {
//            r.create_plane_ex(&PlanePrimitiveInfo::default());
//        },
//        concat!(module_path!(), "::", stringify!(graph_plane)),
//    );
}

#[test]
#[serial]
fn canvas_cone() {
    use meshi::render::database::geometry_primitives::ConePrimitiveInfo;
    run_backend(
        RenderBackend::Canvas,
        |r| {
            r.create_cone_ex(&ConePrimitiveInfo::default());
        },
        concat!(module_path!(), "::", stringify!(canvas_cone)),
    );
}

#[test]
#[serial]
fn graph_cone() {
//    use meshi::render::database::geometry_primitives::ConePrimitiveInfo;
//    run_backend(
//        RenderBackend::Graph,
//        |r| {
//            r.create_cone_ex(&ConePrimitiveInfo::default());
//        },
//        concat!(module_path!(), "::", stringify!(graph_cone)),
//    );
}
