use glam::Mat4;
use meshi_graphics::*;

fn main() {
    let mut engine = RenderEngine::new(&RenderEngineInfo {
        headless: false,
        canvas_extent: Some([1280, 1024]),
    }).unwrap();

    let mut db = DB::new(&DBInfo {
        base_dir: "",
        layout_file: None,
    }).expect("Unable to create database");

    db.import_dashi_context(engine.context());

    let camera = engine.register_camera(&Mat4::IDENTITY);
    let cube = engine.register_object(&RenderObjectInfo::Model(db.fetch_gpu_model("model/sphere").unwrap())).unwrap();
    
 
}
