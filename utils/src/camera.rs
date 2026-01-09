use glam::{Mat4, Quat, Vec2, Vec3};

#[derive(Debug, Copy, Clone)]
pub struct CameraInput {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub fast: bool,
    pub mouse_delta: Vec2,
}

impl Default for CameraInput {
    fn default() -> Self {
        Self {
            forward: false,
            back: false,
            left: false,
            right: false,
            up: false,
            down: false,
            fast: false,
            mouse_delta: Vec2::ZERO,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct CameraController {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub speed: f32,
    pub fast_speed: f32,
    pub sensitivity: f32,
    pub input: CameraInput,
}

impl CameraController {
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            yaw: 0.0,
            pitch: 0.0,
            speed: 3.0,
            fast_speed: 9.0,
            sensitivity: 0.003,
            input: CameraInput::default(),
        }
    }

    pub fn look_at(position: Vec3, target: Vec3) -> Self {
        let mut controller = Self::new(position);
        controller.set_look_at(target);
        controller
    }

    pub fn set_look_at(&mut self, target: Vec3) {
        let mut forward = target - self.position;
        if forward.length_squared() == 0.0 {
            forward = Vec3::NEG_Z;
        }
        let forward = forward.normalize();
        self.pitch = forward.y.clamp(-1.0, 1.0).asin();
        self.yaw = (-forward.x).atan2(-forward.z);
    }

    pub fn transform(&self) -> Mat4 {
        let rotation = Self::rotation(self.yaw, self.pitch);
        Mat4::from_rotation_translation(rotation, self.position)
    }

    pub fn update(&mut self, dt: f32) -> Mat4 {
        let mouse_delta = self.input.mouse_delta;
        self.input.mouse_delta = Vec2::ZERO;
        self.yaw += mouse_delta.x * self.sensitivity;
        self.pitch = (self.pitch + mouse_delta.y * self.sensitivity).clamp(-1.54, 1.54);

        let rotation = Self::rotation(self.yaw, self.pitch);
        let forward = rotation * Vec3::NEG_Z;
        let right = rotation * Vec3::X;
        let up = rotation * Vec3::Y;

        let mut direction = Vec3::ZERO;
        if self.input.forward {
            direction += forward;
        }
        if self.input.back {
            direction -= forward;
        }
        if self.input.right {
            direction += right;
        }
        if self.input.left {
            direction -= right;
        }
        if self.input.up {
            direction += up;
        }
        if self.input.down {
            direction -= up;
        }
        if direction.length_squared() > 0.0 {
            let speed = if self.input.fast {
                self.fast_speed
            } else {
                self.speed
            };
            self.position += direction.normalize() * speed * dt;
        }

        Mat4::from_rotation_translation(rotation, self.position)
    }

    fn rotation(yaw: f32, pitch: f32) -> Quat {
        Quat::from_axis_angle(Vec3::Y, yaw) * Quat::from_axis_angle(Vec3::X, pitch)
    }
}
