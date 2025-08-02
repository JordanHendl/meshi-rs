use glam::vec2;
use meshi::render::event::{from_winit_event, Event, EventSource, EventType};
use meshi::*;
use std::ffi::{c_void, CString};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use winit::event::{
    DeviceEvent, ElementState, Event as WEvent, MouseScrollDelta, TouchPhase, WindowEvent,
    ModifiersState,
};

extern "C" fn cb(_ev: *mut Event, data: *mut c_void) {
    let counter: &AtomicUsize = unsafe { &*(data as *const AtomicUsize) };
    counter.fetch_add(1, Ordering::SeqCst);
}

fn main() {

    // Test conversion from winit events
    let window_id: winit::window::WindowId = unsafe { std::mem::zeroed() };

    let wheel_event = WEvent::WindowEvent {
        window_id,
        event: WindowEvent::MouseWheel {
            device_id: unsafe { std::mem::zeroed() },
            delta: MouseScrollDelta::LineDelta(1.0, -1.0),
            phase: TouchPhase::Moved,
            modifiers: ModifiersState::empty(),
        },
    };
    let ev = from_winit_event(&wheel_event).expect("mouse wheel");
    assert_eq!(ev.event_type(), EventType::Motion2D);
    assert_eq!(ev.source(), EventSource::Mouse);
    let motion = unsafe { ev.motion2d() };
    assert_eq!(motion, vec2(1.0, -1.0));

    let focus_event = WEvent::WindowEvent {
        window_id,
        event: WindowEvent::Focused(true),
    };
    let ev = from_winit_event(&focus_event).expect("focused");
    assert_eq!(ev.event_type(), EventType::Pressed);
    assert_eq!(ev.source(), EventSource::Window);

    let focus_event = WEvent::WindowEvent {
        window_id,
        event: WindowEvent::Focused(false),
    };
    let ev = from_winit_event(&focus_event).expect("unfocused");
    assert_eq!(ev.event_type(), EventType::Released);
    assert_eq!(ev.source(), EventSource::Window);

    let device_id: winit::event::DeviceId = unsafe { std::mem::zeroed() };

    let button_event = WEvent::DeviceEvent {
        device_id,
        event: DeviceEvent::Button {
            button: 1,
            state: ElementState::Pressed,
        },
    };
    let ev = from_winit_event(&button_event).expect("gamepad button");
    assert_eq!(ev.event_type(), EventType::Pressed);
    assert_eq!(ev.source(), EventSource::Gamepad);

    let motion_event = WEvent::DeviceEvent {
        device_id,
        event: DeviceEvent::Motion { axis: 0, value: 0.5 },
    };
    let ev = from_winit_event(&motion_event).expect("gamepad motion");
    assert_eq!(ev.event_type(), EventType::Joystick);
    assert_eq!(ev.source(), EventSource::Gamepad);
    let motion = unsafe { ev.motion2d() };
    assert_eq!(motion, vec2(0.0, 0.5));

    // Existing engine callback test
    if std::env::var("DISPLAY").is_err() && std::env::var("WAYLAND_DISPLAY").is_err() {
        return;
    }
  
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
