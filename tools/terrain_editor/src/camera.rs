use glam::{Mat4, Vec2, Vec3};

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
            fast_speed: 36.0,
            sensitivity: 0.006,
            pitch_limit: 1.54,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CameraInput {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub fast: bool,
    pub move_active: bool,
}

pub struct CameraController {
    position: Vec3,
    yaw: f32,
    pitch: f32,
    settings: CameraSettings,
}

impl CameraController {
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            yaw: 0.0,
            pitch: 0.0,
            settings: CameraSettings::default(),
        }
    }

    pub fn settings_mut(&mut self) -> &mut CameraSettings {
        &mut self.settings
    }

    pub fn transform(&self) -> Mat4 {
        let forward = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
        .normalize();
        Mat4::look_to_rh(self.position, forward, Vec3::Y).inverse()
    }

    pub fn update(&mut self, dt: f32, input: &CameraInput, mouse_delta: Vec2) -> Mat4 {
        if input.move_active {
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
        if input.move_active && input.forward {
            direction += forward;
        }
        if input.move_active && input.back {
            direction -= forward;
        }
        if input.move_active && input.right {
            direction += right;
        }
        if input.move_active && input.left {
            direction -= right;
        }
        if input.move_active && input.up {
            direction += Vec3::Y;
        }
        if input.move_active && input.down {
            direction += Vec3::NEG_Y;
        }
        if direction.length_squared() > 0.0 {
            let speed = if input.fast {
                self.settings.fast_speed
            } else {
                self.settings.speed
            };
            self.position += direction.normalize() * speed * dt;
        }

        Mat4::look_to_rh(self.position, forward, Vec3::Y).inverse()
    }
}
