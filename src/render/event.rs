use std::os::raw::c_uint;

#[allow(dead_code)]
#[repr(C)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum EventType {
    Unknown = 0,
    Quit = 1,
    Pressed = 2,
    Released = 3,
    Joystick = 4,
    Motion2D = 5,
}

#[allow(dead_code)]
#[repr(C)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum EventSource {
    Unknown = 0,
    Key = 1,
    Mouse = 2,
    MouseButton = 3,
    Gamepad = 4,
    Window = 5,
}

#[allow(dead_code)]
#[repr(C)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum KeyCode {
    // Alphanumeric keys
    A = 0,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,

    // Number keys (top row)
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,

    // Function keys
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,

    // Modifier keys
    Shift,
    Control,
    Alt,
    Meta, // Windows key or Command key (Mac)

    // Navigation keys
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,

    // Special keys
    Escape,
    Enter,
    Space,
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,

    // Punctuation and symbols
    Minus,        // -
    Equals,       // =
    LeftBracket,  // [
    RightBracket, // ]
    Backslash,    // \
    Semicolon,    // ;
    Apostrophe,   // '
    Comma,        // ,
    Period,       // .
    Slash,        // /
    GraveAccent,  // `

    // Numpad keys
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,      // +
    NumpadSubtract, // -
    NumpadMultiply, // *
    NumpadDivide,   // /
    NumpadDecimal,  // .
    NumpadEnter,

    // Lock keys
    CapsLock,
    NumLock,
    ScrollLock,

    // Miscellaneous keys
    PrintScreen,
    Pause,
    Menu,

    // Undefined or custom keys
    Undefined,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub enum MouseButton {
    Left,
    Right,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PressPayload {
    key: KeyCode,
    previous: EventType,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Motion2DPayload {
    motion: Vec2,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct MouseButtonPayload {
    button: MouseButton,
    pos: Vec2,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union Payload {
    press: PressPayload,
    motion2d: Motion2DPayload,
    mouse_button: MouseButtonPayload,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Event {
    event_type: EventType,
    source: EventSource,
    payload: Payload,
    timestamp: c_uint,
}

impl Event {
    pub fn event_type(&self) -> EventType {
        self.event_type
    }

    pub fn source(&self) -> EventSource {
        self.source
    }

    pub unsafe fn motion2d(&self) -> Vec2 {
        self.payload.motion2d.motion
    }

    pub unsafe fn key(&self) -> KeyCode {
        self.payload.press.key
    }
}

use glam::{vec2, Vec2};

use winit::event::{
    DeviceEvent, ElementState, Event as WEvent, MouseButton as WMouseButton, MouseScrollDelta,
    VirtualKeyCode, WindowEvent,
};

//impl From<SdlEvent> for Event {
//    fn from(sdl_event: SdlEvent) -> Self {
//        match sdl_event {
//            SdlEvent::Quit { timestamp } => Event {
//                event_type: EventType::Quit,
//                source: EventSource::Unknown,
//                payload: Payload {
//                    press: PressPayload {
//                        key: KeyCode::Undefined,
//                        previous: EventType::Unknown,
//                    },
//                },
//                timestamp,
//            },
//
//            SdlEvent::KeyDown {
//                timestamp,
//                keycode: Some(sdl_keycode),
//                ..
//            } => Event {
//                event_type: EventType::Pressed,
//                source: EventSource::Key,
//                payload: Payload {
//                    press: PressPayload {
//                        key: map_sdl_keycode(sdl_keycode),
//                        previous: EventType::Unknown,
//                    },
//                },
//                timestamp,
//            },
//
//            SdlEvent::KeyUp {
//                timestamp,
//                keycode: Some(sdl_keycode),
//                ..
//            } => Event {
//                event_type: EventType::Released,
//                source: EventSource::Key,
//                payload: Payload {
//                    press: PressPayload {
//                        key: map_sdl_keycode(sdl_keycode),
//                        previous: EventType::Unknown,
//                    },
//                },
//                timestamp,
//            },
//            SdlEvent::MouseButtonDown {
//                timestamp,
//                mouse_btn,
//                x,
//                y,
//                ..
//            } => Event {
//                event_type: EventType::Pressed,
//                source: EventSource::MouseButton,
//                payload: Payload {
//                    mouse_button: MouseButtonPayload {
//                        button: map_sdl_mouse_button(mouse_btn),
//                        pos: vec2(x as f32, y as f32),
//                    },
//                },
//                timestamp,
//            },
//
//            SdlEvent::MouseButtonUp {
//                timestamp,
//                mouse_btn,
//                x,
//                y,
//                ..
//            } => Event {
//                event_type: EventType::Released,
//                source: EventSource::MouseButton,
//                payload: Payload {
//                    mouse_button: MouseButtonPayload {
//                        button: map_sdl_mouse_button(mouse_btn),
//                        pos: vec2(x as f32, y as f32),
//                    },
//                },
//                timestamp,
//            },
//            SdlEvent::MouseMotion {
//                timestamp,
//                xrel,
//                yrel,
//                ..
//            } => Event {
//                event_type: EventType::Motion2D,
//                source: EventSource::Mouse,
//                payload: Payload {
//                    motion2d: Motion2DPayload {
//                        motion: Vec2 {
//                            x: xrel as f32,
//                            y: yrel as f32,
//                        },
//                    },
//                },
//                timestamp,
//            },
//
//            _ => Event {
//                event_type: EventType::Unknown,
//                source: EventSource::Key,
//                payload: Payload {
//                    press: PressPayload {
//                        key: KeyCode::Undefined,
//                        previous: EventType::Unknown,
//                    },
//                },
//                timestamp: 0,
//            },
//        }
//    }
//}
//
//fn map_sdl_mouse_button(button: sdl2::mouse::MouseButton) -> MouseButton {
//    match button {
//        sdl2::mouse::MouseButton::Unknown => return MouseButton::Left,
//        sdl2::mouse::MouseButton::Left => return MouseButton::Left,
//        sdl2::mouse::MouseButton::Middle => todo!(),
//        sdl2::mouse::MouseButton::Right => return MouseButton::Right,
//        sdl2::mouse::MouseButton::X1 => todo!(),
//        sdl2::mouse::MouseButton::X2 => todo!(),
//    }
//}
///// Helper function to map `sdl2::keyboard::Keycode` to `KeyCode`
//fn map_sdl_keycode(sdl_keycode: SdlKeycode) -> KeyCode {
//    match sdl_keycode {
//        SdlKeycode::A => KeyCode::A,
//        SdlKeycode::B => KeyCode::B,
//        SdlKeycode::C => KeyCode::C,
//        SdlKeycode::D => KeyCode::D,
//        SdlKeycode::E => KeyCode::E,
//        SdlKeycode::F => KeyCode::F,
//        SdlKeycode::G => KeyCode::G,
//        SdlKeycode::H => KeyCode::H,
//        SdlKeycode::I => KeyCode::I,
//        SdlKeycode::J => KeyCode::J,
//        SdlKeycode::K => KeyCode::K,
//        SdlKeycode::L => KeyCode::L,
//        SdlKeycode::M => KeyCode::M,
//        SdlKeycode::N => KeyCode::N,
//        SdlKeycode::O => KeyCode::O,
//        SdlKeycode::P => KeyCode::P,
//        SdlKeycode::Q => KeyCode::Q,
//        SdlKeycode::R => KeyCode::R,
//        SdlKeycode::S => KeyCode::S,
//        SdlKeycode::T => KeyCode::T,
//        SdlKeycode::U => KeyCode::U,
//        SdlKeycode::V => KeyCode::V,
//        SdlKeycode::W => KeyCode::W,
//        SdlKeycode::X => KeyCode::X,
//        SdlKeycode::Y => KeyCode::Y,
//        SdlKeycode::Z => KeyCode::Z,
//        SdlKeycode::Num0 => KeyCode::Digit0,
//        SdlKeycode::Num1 => KeyCode::Digit1,
//        SdlKeycode::Num2 => KeyCode::Digit2,
//        SdlKeycode::Num3 => KeyCode::Digit3,
//        SdlKeycode::Num4 => KeyCode::Digit4,
//        SdlKeycode::Num5 => KeyCode::Digit5,
//        SdlKeycode::Num6 => KeyCode::Digit6,
//        SdlKeycode::Num7 => KeyCode::Digit7,
//        SdlKeycode::Num8 => KeyCode::Digit8,
//        SdlKeycode::Num9 => KeyCode::Digit9,
//        SdlKeycode::F1 => KeyCode::F1,
//        SdlKeycode::F2 => KeyCode::F2,
//        SdlKeycode::F3 => KeyCode::F3,
//        SdlKeycode::F4 => KeyCode::F4,
//        SdlKeycode::F5 => KeyCode::F5,
//        SdlKeycode::F6 => KeyCode::F6,
//        SdlKeycode::F7 => KeyCode::F7,
//        SdlKeycode::F8 => KeyCode::F8,
//        SdlKeycode::F9 => KeyCode::F9,
//        SdlKeycode::F10 => KeyCode::F10,
//        SdlKeycode::F11 => KeyCode::F11,
//        SdlKeycode::F12 => KeyCode::F12,
//        SdlKeycode::Left => KeyCode::ArrowLeft,
//        SdlKeycode::Right => KeyCode::ArrowRight,
//        SdlKeycode::Up => KeyCode::ArrowUp,
//        SdlKeycode::Down => KeyCode::ArrowDown,
//        SdlKeycode::Escape => KeyCode::Escape,
//        SdlKeycode::Return => KeyCode::Enter,
//        SdlKeycode::Space => KeyCode::Space,
//        SdlKeycode::Tab => KeyCode::Tab,
//        SdlKeycode::Backspace => KeyCode::Backspace,
//        SdlKeycode::Delete => KeyCode::Delete,
//        SdlKeycode::Insert => KeyCode::Insert,
//        SdlKeycode::Home => KeyCode::Home,
//        SdlKeycode::End => KeyCode::End,
//        SdlKeycode::PageUp => KeyCode::PageUp,
//        SdlKeycode::PageDown => KeyCode::PageDown,
//        _ => KeyCode::Undefined,
//    }
//}

fn map_virtual_keycode(key: VirtualKeyCode) -> KeyCode {
    use VirtualKeyCode::*;
    match key {
        A => KeyCode::A,
        B => KeyCode::B,
        C => KeyCode::C,
        D => KeyCode::D,
        E => KeyCode::E,
        F => KeyCode::F,
        G => KeyCode::G,
        H => KeyCode::H,
        I => KeyCode::I,
        J => KeyCode::J,
        K => KeyCode::K,
        L => KeyCode::L,
        M => KeyCode::M,
        N => KeyCode::N,
        O => KeyCode::O,
        P => KeyCode::P,
        Q => KeyCode::Q,
        R => KeyCode::R,
        S => KeyCode::S,
        T => KeyCode::T,
        U => KeyCode::U,
        V => KeyCode::V,
        W => KeyCode::W,
        X => KeyCode::X,
        Y => KeyCode::Y,
        Z => KeyCode::Z,
        Key0 => KeyCode::Digit0,
        Key1 => KeyCode::Digit1,
        Key2 => KeyCode::Digit2,
        Key3 => KeyCode::Digit3,
        Key4 => KeyCode::Digit4,
        Key5 => KeyCode::Digit5,
        Key6 => KeyCode::Digit6,
        Key7 => KeyCode::Digit7,
        Key8 => KeyCode::Digit8,
        Key9 => KeyCode::Digit9,
        Escape => KeyCode::Escape,
        Return => KeyCode::Enter,
        Space => KeyCode::Space,
        Tab => KeyCode::Tab,
        Back => KeyCode::Backspace,
        Insert => KeyCode::Insert,
        Delete => KeyCode::Delete,
        Home => KeyCode::Home,
        End => KeyCode::End,
        PageUp => KeyCode::PageUp,
        PageDown => KeyCode::PageDown,
        Left => KeyCode::ArrowLeft,
        Right => KeyCode::ArrowRight,
        Up => KeyCode::ArrowUp,
        Down => KeyCode::ArrowDown,
        F1 => KeyCode::F1,
        F2 => KeyCode::F2,
        F3 => KeyCode::F3,
        F4 => KeyCode::F4,
        F5 => KeyCode::F5,
        F6 => KeyCode::F6,
        F7 => KeyCode::F7,
        F8 => KeyCode::F8,
        F9 => KeyCode::F9,
        F10 => KeyCode::F10,
        F11 => KeyCode::F11,
        F12 => KeyCode::F12,
        LShift | RShift => KeyCode::Shift,
        LControl | RControl => KeyCode::Control,
        LAlt | RAlt => KeyCode::Alt,
        LWin | RWin => KeyCode::Meta,
        Minus => KeyCode::Minus,
        Equals => KeyCode::Equals,
        LBracket => KeyCode::LeftBracket,
        RBracket => KeyCode::RightBracket,
        Backslash => KeyCode::Backslash,
        Semicolon => KeyCode::Semicolon,
        Apostrophe => KeyCode::Apostrophe,
        Comma => KeyCode::Comma,
        Period => KeyCode::Period,
        Slash => KeyCode::Slash,
        Grave => KeyCode::GraveAccent,
        Numpad0 => KeyCode::Numpad0,
        Numpad1 => KeyCode::Numpad1,
        Numpad2 => KeyCode::Numpad2,
        Numpad3 => KeyCode::Numpad3,
        Numpad4 => KeyCode::Numpad4,
        Numpad5 => KeyCode::Numpad5,
        Numpad6 => KeyCode::Numpad6,
        Numpad7 => KeyCode::Numpad7,
        Numpad8 => KeyCode::Numpad8,
        Numpad9 => KeyCode::Numpad9,
        NumpadAdd => KeyCode::NumpadAdd,
        NumpadSubtract => KeyCode::NumpadSubtract,
        NumpadMultiply => KeyCode::NumpadMultiply,
        NumpadDivide => KeyCode::NumpadDivide,
        NumpadDecimal => KeyCode::NumpadDecimal,
        NumpadEnter => KeyCode::NumpadEnter,
        Capital => KeyCode::CapsLock,
        Numlock => KeyCode::NumLock,
        Scroll => KeyCode::ScrollLock,
        Snapshot => KeyCode::PrintScreen,
        Pause => KeyCode::Pause,
        Apps => KeyCode::Menu,
        _ => KeyCode::Undefined,
    }
}

pub fn from_winit_event(event: &WEvent<'_, ()>) -> Option<Event> {
    match event {
        WEvent::WindowEvent { event, .. } => match event {
            WindowEvent::CloseRequested => Some(Event {
                event_type: EventType::Quit,
                source: EventSource::Window,
                payload: Payload { press: PressPayload { key: KeyCode::Undefined, previous: EventType::Unknown } },
                timestamp: 0,
            }),
            WindowEvent::KeyboardInput { input, .. } => {
                if let Some(k) = input.virtual_keycode {
                    let key = map_virtual_keycode(k);
                    let et = if input.state == ElementState::Pressed {
                        EventType::Pressed
                    } else {
                        EventType::Released
                    };
                    Some(Event {
                        event_type: et,
                        source: EventSource::Key,
                        payload: Payload { press: PressPayload { key, previous: EventType::Unknown } },
                        timestamp: 0,
                    })
                } else {
                    None
                }
            }
            WindowEvent::CursorMoved { position, .. } => Some(Event {
                event_type: EventType::Motion2D,
                source: EventSource::Mouse,
                payload: Payload { motion2d: Motion2DPayload { motion: vec2(position.x as f32, position.y as f32) } },
                timestamp: 0,
            }),
            WindowEvent::MouseInput { state, button, .. } => {
                let btn = match button {
                    WMouseButton::Left => MouseButton::Left,
                    WMouseButton::Right => MouseButton::Right,
                    _ => MouseButton::Left,
                };
                let et = if *state == ElementState::Pressed {
                    EventType::Pressed
                } else {
                    EventType::Released
                };
                Some(Event {
                    event_type: et,
                    source: EventSource::MouseButton,
                    payload: Payload {
                        mouse_button: MouseButtonPayload {
                            button: btn,
                            pos: vec2(0.0, 0.0),
                        },
                    },
                    timestamp: 0,
                })
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (x, y) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (*x as f32, *y as f32),
                    MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
                };
                Some(Event {
                    event_type: EventType::Motion2D,
                    source: EventSource::Mouse,
                    payload: Payload { motion2d: Motion2DPayload { motion: vec2(x, y) } },
                    timestamp: 0,
                })
            }
            WindowEvent::Resized(size) => Some(Event {
                event_type: EventType::Motion2D,
                source: EventSource::Window,
                payload: Payload {
                    motion2d: Motion2DPayload {
                        motion: vec2(size.width as f32, size.height as f32),
                    },
                },
                timestamp: 0,
            }),
            WindowEvent::Moved(position) => Some(Event {
                event_type: EventType::Motion2D,
                source: EventSource::Window,
                payload: Payload {
                    motion2d: Motion2DPayload {
                        motion: vec2(position.x as f32, position.y as f32),
                    },
                },
                timestamp: 0,
            }),
            WindowEvent::Focused(focused) => {
                let et = if *focused {
                    EventType::Pressed
                } else {
                    EventType::Released
                };
                Some(Event {
                    event_type: et,
                    source: EventSource::Window,
                    payload: Payload {
                        press: PressPayload {
                            key: KeyCode::Undefined,
                            previous: EventType::Unknown,
                        },
                    },
                    timestamp: 0,
                })
            }
            _ => None,
        },
        WEvent::DeviceEvent { event: DeviceEvent::MouseMotion { delta }, .. } => Some(Event {
            event_type: EventType::Motion2D,
            source: EventSource::Mouse,
            payload: Payload {
                motion2d: Motion2DPayload {
                    motion: vec2(delta.0 as f32, delta.1 as f32),
                },
            },
            timestamp: 0,
        }),
        WEvent::DeviceEvent { event: DeviceEvent::MouseWheel { delta }, .. } => {
            let (x, y) = match delta {
                MouseScrollDelta::LineDelta(x, y) => (*x as f32, *y as f32),
                MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
            };
            Some(Event {
                event_type: EventType::Motion2D,
                source: EventSource::Mouse,
                payload: Payload {
                    motion2d: Motion2DPayload {
                        motion: vec2(x, y),
                    },
                },
                timestamp: 0,
            })
        }
        WEvent::DeviceEvent { event: DeviceEvent::Button { state, .. }, .. } => {
            let et = if *state == ElementState::Pressed {
                EventType::Pressed
            } else {
                EventType::Released
            };
            Some(Event {
                event_type: et,
                source: EventSource::Gamepad,
                payload: Payload {
                    press: PressPayload {
                        key: KeyCode::Undefined,
                        previous: EventType::Unknown,
                    },
                },
                timestamp: 0,
            })
        }
        WEvent::DeviceEvent { event: DeviceEvent::Motion { axis, value }, .. } => Some(Event {
            event_type: EventType::Joystick,
            source: EventSource::Gamepad,
            payload: Payload {
                motion2d: Motion2DPayload {
                    motion: vec2(*axis as f32, *value as f32),
                },
            },
            timestamp: 0,
        }),
        _ => None,
    }
}
