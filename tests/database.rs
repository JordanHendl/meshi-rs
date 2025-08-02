use meshi::render::database::{Database, Error};
use image::{RgbaImage, Rgba};
use std::fs;

fn make_db() -> (std::path::PathBuf, Database) {
    // create unique temp directory
    let mut dir = std::env::temp_dir();
    dir.push("meshi_db_test");
    dir.push(format!("{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("db.json"), "{}").unwrap();

    // create dummy context without initialisation; Database never dereferences it
    let mut ctx = std::mem::MaybeUninit::<dashi::Context>::uninit();
    let db = Database::new(dir.to_str().unwrap(), unsafe { &mut *ctx.as_mut_ptr() }).unwrap();
    (dir, db)
}

#[test]
fn fetch_texture_success() {
    let (dir, mut db) = make_db();
    // create a valid image file
    let img_path = dir.join("ok.png");
    let mut img = RgbaImage::new(1,1);
    img.put_pixel(0,0,Rgba([255,0,0,255]));
    img.save(&img_path).unwrap();

    // register and fetch
    db.load_image("ok.png").unwrap();
    let handle = db.fetch_texture("ok.png");
    assert!(handle.is_ok());
}

#[test]
fn fetch_texture_lookup_error() {
    let (_dir, mut db) = make_db();
    let err = db.fetch_texture("missing.png").unwrap_err();
    match err {
        Error::LookupError(_) => {},
        other => panic!("expected lookup error, got {:?}", other),
    }
}

#[test]
fn fetch_texture_loading_error() {
    let (_dir, mut db) = make_db();
    // register texture that doesn't exist on disk
    db.load_image("absent.png").unwrap();
    let err = db.fetch_texture("absent.png").unwrap_err();
    match err {
        Error::LoadingError(_) => {},
        other => panic!("expected loading error, got {:?}", other),
    }
}
