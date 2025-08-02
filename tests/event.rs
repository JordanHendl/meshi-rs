use meshi::render::event::Event;
use meshi::*;
use std::ffi::{c_void, CString};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

extern "C" fn cb(_ev: *mut Event, data: *mut c_void) {
    let counter: &AtomicUsize = unsafe { &*(data as *const AtomicUsize) };
    counter.fetch_add(1, Ordering::SeqCst);
}

fn main() {
    let name = CString::new("test").unwrap();
    let loc = CString::new(".").unwrap();
    let info = MeshiEngineInfo {
        application_name: name.as_ptr(),
        application_location: loc.as_ptr(),
        headless: 1,
    };
    let engine = unsafe { meshi_make_engine(&info) };
    let counter = Arc::new(AtomicUsize::new(0));
    unsafe {
        meshi_register_event_callback(engine, Arc::as_ptr(&counter) as *mut _, cb);
        meshi_update(engine);
    }
    assert!(counter.load(Ordering::SeqCst) > 0);
}
