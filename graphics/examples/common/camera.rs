use glam::{Mat4, Quat, Vec2, Vec3};
use meshi_ffi_structs::event::{Event, EventSource, EventType, KeyCode};

#[derive(Debug, Clone, Copy)]
pub struct CameraSettings {
    pub speed: f32,
    pub fast_speed: f32,
    pub sensitivity: f32,
    pub pitch_limit: f32,
}

impl Default for CameraSettings {
    fn default() -> Self {
        Self {
            speed: 18.0,
            fast_speed: 128.0,
            sensitivity: 0.006,
            pitch_limit: 1.54,
        }
    }
}

#[derive(Debug, Default)]
struct CameraInput {
    forward: bool,
    back: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    fast: bool,
    move_active: bool,
}

pub struct CameraController {
    position: Vec3,
    yaw: f32,
    pitch: f32,
    input: CameraInput,
    settings: CameraSettings,
    mouse_delta: Vec2,
    window_size: Vec2,
    last_cursor_pos: Option<Vec2>,
    window_focused: bool,
    mouse_in_window: bool,
}

impl CameraController {
    pub fn new(position: Vec3, window_size: Vec2) -> Self {
        Self {
            position,
            yaw: 0.0,
            pitch: 0.0,
            input: CameraInput::default(),
            settings: CameraSettings::default(),
            mouse_delta: Vec2::ZERO,
            window_size,
            last_cursor_pos: None,
            window_focused: true,
            mouse_in_window: true,
        }
    }

    pub fn settings_mut(&mut self) -> &mut CameraSettings {
        &mut self.settings
    }

    pub fn handle_event(&mut self, event: &Event) {
        match (event.source(), event.event_type()) {
            (EventSource::Key, EventType::Pressed | EventType::Released) => {
                let is_pressed = event.event_type() == EventType::Pressed;
                unsafe {
                    match event.key() {
                        KeyCode::W => self.input.forward = is_pressed,
                        KeyCode::S => self.input.back = is_pressed,
                        KeyCode::A => self.input.left = is_pressed,
                        KeyCode::D => self.input.right = is_pressed,
                        KeyCode::E => self.input.up = is_pressed,
                        KeyCode::Q => self.input.down = is_pressed,
                        KeyCode::Shift => self.input.fast = is_pressed,
                        KeyCode::Space => self.input.move_active = is_pressed,
                        _ => {}
                    }
                }
            }
            (EventSource::Mouse, EventType::CursorMoved) => {
                let position = unsafe { event.motion2d() };
                let in_window = position.x >= 0.0
                    && position.y >= 0.0
                    && position.x < self.window_size.x
                    && position.y < self.window_size.y;
                self.mouse_in_window = in_window;
                if self.window_focused && in_window {
                    if let Some(last) = self.last_cursor_pos {
                        self.mouse_delta += position - last;
                    }
                }
                self.last_cursor_pos = Some(position);
            }
            (EventSource::Window, EventType::WindowResized) => {
                let size = unsafe { event.motion2d() };
                self.window_size = Vec2::new(size.x.max(1.0), size.y.max(1.0));
            }
            (EventSource::Window, EventType::WindowFocused) => {
                self.window_focused = true;
                self.last_cursor_pos = None;
            }
            (EventSource::Window, EventType::WindowUnfocused) => {
                self.window_focused = false;
                self.mouse_in_window = false;
                self.last_cursor_pos = None;
                self.mouse_delta = Vec2::ZERO;
            }
            _ => {}
        }
    }

    pub fn update(&mut self, dt: f32) -> Mat4 {
        let mouse_delta = self.mouse_delta;
        self.mouse_delta = Vec2::ZERO;
        if self.input.move_active {
            self.yaw += mouse_delta.x * self.settings.sensitivity;
            self.pitch = (self.pitch + mouse_delta.y * self.settings.sensitivity)
                .clamp(-self.settings.pitch_limit, self.settings.pitch_limit);
        }

        let forward = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
        .normalize();
        let right = forward.cross(Vec3::Y).normalize();
        let mut direction = Vec3::ZERO;
        if self.input.move_active && self.input.forward {
            direction += forward;
        }
        if self.input.move_active && self.input.back {
            direction -= forward;
        }
        if self.input.move_active && self.input.right {
            direction += right;
        }
        if self.input.move_active && self.input.left {
            direction -= right;
        }
        if self.input.move_active && self.input.up {
            direction += Vec3::Y;
        }
        if self.input.move_active && self.input.down {
            direction += Vec3::NEG_Y;
        }
        if direction.length_squared() > 0.0 {
            let speed = if self.input.fast {
                self.settings.fast_speed
            } else {
                self.settings.speed
            };
            self.position += direction.normalize() * speed * dt;
        }

        Mat4::look_to_rh(self.position, forward, Vec3::Y).inverse()
    }
}
