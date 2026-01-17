use std::ffi::c_void;

use dashi::Handle;
use glam::*;
use meshi_ffi_structs::event::*;
use meshi_graphics::*;
use meshi_utils::timer::Timer;
use std::env::*;

#[path = "../common/setup.rs"]
mod common_setup;

fn keycode_to_char(key: KeyCode, shift: bool) -> Option<char> {
    match key {
        KeyCode::A => Some(if shift { 'A' } else { 'a' }),
        KeyCode::B => Some(if shift { 'B' } else { 'b' }),
        KeyCode::C => Some(if shift { 'C' } else { 'c' }),
        KeyCode::D => Some(if shift { 'D' } else { 'd' }),
        KeyCode::E => Some(if shift { 'E' } else { 'e' }),
        KeyCode::F => Some(if shift { 'F' } else { 'f' }),
        KeyCode::G => Some(if shift { 'G' } else { 'g' }),
        KeyCode::H => Some(if shift { 'H' } else { 'h' }),
        KeyCode::I => Some(if shift { 'I' } else { 'i' }),
        KeyCode::J => Some(if shift { 'J' } else { 'j' }),
        KeyCode::K => Some(if shift { 'K' } else { 'k' }),
        KeyCode::L => Some(if shift { 'L' } else { 'l' }),
        KeyCode::M => Some(if shift { 'M' } else { 'm' }),
        KeyCode::N => Some(if shift { 'N' } else { 'n' }),
        KeyCode::O => Some(if shift { 'O' } else { 'o' }),
        KeyCode::P => Some(if shift { 'P' } else { 'p' }),
        KeyCode::Q => Some(if shift { 'Q' } else { 'q' }),
        KeyCode::R => Some(if shift { 'R' } else { 'r' }),
        KeyCode::S => Some(if shift { 'S' } else { 's' }),
        KeyCode::T => Some(if shift { 'T' } else { 't' }),
        KeyCode::U => Some(if shift { 'U' } else { 'u' }),
        KeyCode::V => Some(if shift { 'V' } else { 'v' }),
        KeyCode::W => Some(if shift { 'W' } else { 'w' }),
        KeyCode::X => Some(if shift { 'X' } else { 'x' }),
        KeyCode::Y => Some(if shift { 'Y' } else { 'y' }),
        KeyCode::Z => Some(if shift { 'Z' } else { 'z' }),
        KeyCode::Digit0 => Some(if shift { ')' } else { '0' }),
        KeyCode::Digit1 => Some(if shift { '!' } else { '1' }),
        KeyCode::Digit2 => Some(if shift { '@' } else { '2' }),
        KeyCode::Digit3 => Some(if shift { '#' } else { '3' }),
        KeyCode::Digit4 => Some(if shift { '$' } else { '4' }),
        KeyCode::Digit5 => Some(if shift { '%' } else { '5' }),
        KeyCode::Digit6 => Some(if shift { '^' } else { '6' }),
        KeyCode::Digit7 => Some(if shift { '&' } else { '7' }),
        KeyCode::Digit8 => Some(if shift { '*' } else { '8' }),
        KeyCode::Digit9 => Some(if shift { '(' } else { '9' }),
        KeyCode::Space => Some(' '),
        KeyCode::Minus => Some(if shift { '_' } else { '-' }),
        KeyCode::Equals => Some(if shift { '+' } else { '=' }),
        KeyCode::LeftBracket => Some(if shift { '{' } else { '[' }),
        KeyCode::RightBracket => Some(if shift { '}' } else { ']' }),
        KeyCode::Backslash => Some(if shift { '|' } else { '\\' }),
        KeyCode::Semicolon => Some(if shift { ':' } else { ';' }),
        KeyCode::Apostrophe => Some(if shift { '"' } else { '\'' }),
        KeyCode::Comma => Some(if shift { '<' } else { ',' }),
        KeyCode::Period => Some(if shift { '>' } else { '.' }),
        KeyCode::Slash => Some(if shift { '?' } else { '/' }),
        KeyCode::GraveAccent => Some(if shift { '~' } else { '`' }),
        _ => None,
    }
}

fn main() {
    tracing_subscriber::fmt::init();
    let args: Vec<String> = args().collect();
    let renderer = common_setup::renderer_from_args(&args, RendererSelect::Deferred);
    let mut setup = common_setup::init(
        "meshi-text",
        [800, 400],
        common_setup::CameraSetup::default(),
        renderer,
    );

    let render_mode = common_setup::text_render_mode(&setup.db);
    let prompt_text = setup.engine.register_text(&TextInfo {
        text: "Type to update text. Backspace deletes. Enter inserts a new line.".to_string(),
        position: Vec2::new(12.0, 12.0),
        color: Vec4::ONE,
        scale: 1.6,
        render_mode: render_mode.clone(),
    });

    let input_text = setup.engine.register_text(&TextInfo {
        text: "Hello Meshi!".to_string(),
        position: Vec2::new(12.0, 48.0),
        color: Vec4::new(0.9, 0.9, 1.0, 1.0),
        scale: 2.0,
        render_mode,
    });

    struct AppData {
        running: bool,
        shift: bool,
        text_changed: bool,
        input: String,
        input_handle: Handle<TextObject>,
        _prompt_handle: Handle<TextObject>,
    }

    let mut data = AppData {
        running: true,
        shift: false,
        text_changed: false,
        input: "Hello Meshi!".to_string(),
        input_handle: input_text,
        _prompt_handle: prompt_text,
    };

    extern "C" fn callback(event: *mut Event, data: *mut c_void) {
        unsafe {
            let e = &mut (*event);
            let r = &mut (*(data as *mut AppData));
            if e.source() == EventSource::Key {
                match e.event_type() {
                    EventType::Pressed => {
                        let key = e.key();
                        match key {
                            KeyCode::Shift => r.shift = true,
                            KeyCode::Backspace => {
                                r.input.pop();
                                r.text_changed = true;
                            }
                            KeyCode::Enter => {
                                r.input.push('\n');
                                r.text_changed = true;
                            }
                            _ => {
                                if let Some(ch) = keycode_to_char(key, r.shift) {
                                    r.input.push(ch);
                                    r.text_changed = true;
                                }
                            }
                        }
                    }
                    EventType::Released => {
                        if e.key() == KeyCode::Shift {
                            r.shift = false;
                        }
                    }
                    _ => {}
                }
            }

            if e.event_type() == EventType::Quit {
                r.running = false;
            }
        }
    }

    setup
        .engine
        .set_event_cb(callback, (&mut data as *mut AppData) as *mut c_void);
    let mut timer = Timer::new();
    timer.start();
    let mut last_time = timer.elapsed_seconds_f32();

    while data.running {
        if data.text_changed {
            setup.engine.set_text(data.input_handle, &data.input);
            data.text_changed = false;
        }

        let now = timer.elapsed_seconds_f32();
        let dt = (now - last_time).min(1.0 / 30.0);
        setup.engine.update(dt);
        last_time = now;
    }

    setup.engine.shut_down();
}
