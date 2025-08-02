use std::ffi::c_void;

use dashi::{
    utils::{Handle, Pool},
    *,
};
use database::Database;
use dashi::gpu::Display;
use dashi::gpu::DisplayInfo;
use winit::event::{Event as WinitEvent, WindowEvent, DeviceEvent, ElementState, KeyboardInput, VirtualKeyCode, MouseButton as WinitMouseButton};
use winit::event_loop::ControlFlow;
use winit::platform::run_return::EventLoopExtRunReturn;
use glam::{Mat4, Vec4};
use tracing::info;

use crate::object::{FFIMeshObjectInfo, MeshObject, MeshObjectInfo};
pub mod config;
pub mod database;
pub mod event;

pub struct SceneInfo<'a> {
    pub models: &'a [&'a str],
    pub images: &'a [&'a str],
}

#[derive(Default, Clone, Copy)]
#[repr(C)]
pub struct DirectionalLightInfo {
    pub direction: Vec4,
    pub color: Vec4,
    pub intensity: f32,
}

#[derive(Default)]
pub struct DirectionalLight {
    pub transform: Mat4,
    pub info: DirectionalLightInfo,
}
pub struct CameraInfo<'a> {
    pub pass: &'a str,
    pub transform: Mat4,
    pub projection: Mat4,
}

#[derive(Default)]
pub struct RenderEngineInfo<'a> {
    pub application_path: String,
    pub scene_info: Option<SceneInfo<'a>>,
}

struct EventCallbackInfo {
    event_cb: extern "C" fn(*mut event::Event, *mut c_void),
    user_data: *mut c_void,
}

#[allow(dead_code)]
pub struct RenderEngine {
    ctx: Box<dashi::Context>,
    database: Database,
    event_cb: Option<EventCallbackInfo>,
    display: Display,
    mesh_objects: Pool<MeshObject>,
    directional_lights: Pool<DirectionalLight>,
}

#[allow(dead_code)]
impl RenderEngine {
    pub fn new(info: &RenderEngineInfo) -> Self {
        let device = DeviceSelector::new()
            .unwrap()
            .select(DeviceFilter::default().add_required_type(DeviceType::Dedicated))
            .unwrap_or_default();

        info!("Initializing Render Engine with device {}", device);

        let cfg = config::RenderEngineConfig {
            scene_cfg_path: Some(format!("{}/koji.json", info.application_path)),
            database_path: Some(format!("{}/database", info.application_path)),
        };

        // The GPU context that holds all the data.
        let mut ctx = Box::new(gpu::Context::new(&ContextInfo { device }).unwrap());
        let display = ctx.make_display(&DisplayInfo::default()).unwrap();
        //        let event_pump = ctx.get_sdl_ctx().event_pump().unwrap();
        //        let mut scene = Box::new(miso::Scene::new(
        //            &mut ctx,
        //            &miso::SceneInfo {
        //                cfg: cfg.scene_cfg_path,
        //            },
        //        ));

        let database = Database::new(cfg.database_path.as_ref().unwrap(), &mut ctx).unwrap();

        //        let global_camera = scene.register_camera(&CameraInfo {
        //            pass: "ALL",
        //            transform: Default::default(),
        //            projection: Default::default(),
        //        });

        let s = Self {
            ctx,
            //            scene,
            database,
            event_cb: None,
            display,
            mesh_objects: Default::default(),
            directional_lights: Default::default(),
            //            global_camera,
        };

        s
    }

    pub fn register_directional_light(
        &mut self,
        info: &DirectionalLightInfo,
    ) -> Handle<DirectionalLight> {
        let light = DirectionalLight {
            transform: Mat4::IDENTITY,
            info: *info,
        };
        self.directional_lights.insert(light).unwrap()
    }

    pub fn set_directional_light_transform(
        &mut self,
        handle: Handle<DirectionalLight>,
        transform: &Mat4,
    ) {
        if let Some(light) = self.directional_lights.get_mut_ref(handle) {
            light.transform = *transform;
        }
    }

    pub fn register_mesh_object(&mut self, info: &FFIMeshObjectInfo) -> Handle<MeshObject> {
        let info: MeshObjectInfo = info.into();
        info!(
            "Registering Mesh Object {} with material {}",
            info.mesh, info.material
        );
        let object = info.make_object(&mut self.database);
        self.mesh_objects.insert(object).unwrap()
    }

    pub fn create_cube(&mut self) -> Handle<MeshObject> {
        let info = MeshObjectInfo {
            mesh: "MESHI_CUBE",
            material: "MESHI_CUBE",
            transform: Mat4::IDENTITY,
        };
        let obj = info.make_object(&mut self.database);
        self.mesh_objects.insert(obj).unwrap()
    }

    pub fn create_sphere(&mut self) -> Handle<MeshObject> {
        let info = MeshObjectInfo {
            mesh: "MESHI_SPHERE",
            material: "MESHI_SPHERE",
            transform: Mat4::IDENTITY,
        };
        let obj = info.make_object(&mut self.database);
        self.mesh_objects.insert(obj).unwrap()
    }

    pub fn create_triangle(&mut self) -> Handle<MeshObject> {
        let info = MeshObjectInfo {
            mesh: "MESHI_TRIANGLE",
            material: "MESHI_TRIANGLE",
            transform: Mat4::IDENTITY,
        };
        let obj = info.make_object(&mut self.database);
        self.mesh_objects.insert(obj).unwrap()
    }

    pub fn set_mesh_object_transform(
        &mut self,
        handle: Handle<MeshObject>,
        transform: &glam::Mat4,
    ) {
        //        if let Some(m) = self.mesh_objects.get_ref(handle) {
        //            for t in &m.targets {
        //                self.scene.update_object_transform(*t, transform);
        //            }
        //        }
    }

    pub fn update(&mut self, _delta_time: f32) {
        let mut events = Vec::new();
        {
            let event_loop = self.display.winit_event_loop();
            event_loop.run_return(|e, _t, flow| {
                *flow = ControlFlow::Exit;
                if let Some(ev) = convert_event(e) {
                    events.push(ev);
                }
            });
        }

        if let Some(cb) = self.event_cb.as_mut() {
            for mut ev in events {
                (cb.event_cb)(&mut ev, cb.user_data);
            }
        }

        //        self.scene.update();
    }

    pub fn set_projection(&mut self, proj: &Mat4) {
        //        self.scene
        //            .update_camera_projection(self.global_camera, proj);
    }

    pub fn set_capture_mouse(&mut self, capture: bool) {
        //        self.ctx.get_sdl_ctx().mouse().set_relative_mouse_mode(capture);
    }
    pub fn set_camera(&mut self, camera: &Mat4) {
        //       self.scene
        //           .update_camera_transform(self.global_camera, camera);
    }
    pub fn set_event_cb(
        &mut self,
        event_cb: extern "C" fn(*mut event::Event, *mut c_void),
        user_data: *mut c_void,
    ) {
        self.event_cb = Some(EventCallbackInfo {
            event_cb,
            user_data,
        });
    }

    pub fn set_scene(&mut self, _info: &SceneInfo) {
        // Scene loading not implemented
    }
}

fn convert_event(e: WinitEvent<'_, ()>) -> Option<event::Event> {
    match e {
        WinitEvent::WindowEvent { event, .. } => match event {
            WindowEvent::CloseRequested => Some(event::Event::quit()),
            WindowEvent::KeyboardInput { input: KeyboardInput { virtual_keycode: Some(vk), state, .. }, .. } => {
                let t = if state == ElementState::Pressed { event::EventType::Pressed } else { event::EventType::Released };
                Some(event::Event::key(t, map_keycode(vk)))
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let t = if state == ElementState::Pressed { event::EventType::Pressed } else { event::EventType::Released };
                Some(event::Event::mouse_button(t, map_mouse_button(button), glam::Vec2::ZERO))
            }
            _ => None,
        },
        WinitEvent::DeviceEvent { event: DeviceEvent::MouseMotion { delta }, .. } =>
            Some(event::Event::motion2d(glam::vec2(delta.0 as f32, delta.1 as f32))),
        _ => None,
    }
}

fn map_mouse_button(btn: WinitMouseButton) -> event::MouseButton {
    match btn {
        WinitMouseButton::Left => event::MouseButton::Left,
        WinitMouseButton::Right => event::MouseButton::Right,
        _ => event::MouseButton::Left,
    }
}

fn map_keycode(vk: VirtualKeyCode) -> event::KeyCode {
    use event::KeyCode::*;
    match vk {
        VirtualKeyCode::A => A,
        VirtualKeyCode::B => B,
        VirtualKeyCode::C => C,
        VirtualKeyCode::D => D,
        VirtualKeyCode::E => E,
        VirtualKeyCode::F => F,
        VirtualKeyCode::G => G,
        VirtualKeyCode::H => H,
        VirtualKeyCode::I => I,
        VirtualKeyCode::J => J,
        VirtualKeyCode::K => K,
        VirtualKeyCode::L => L,
        VirtualKeyCode::M => M,
        VirtualKeyCode::N => N,
        VirtualKeyCode::O => O,
        VirtualKeyCode::P => P,
        VirtualKeyCode::Q => Q,
        VirtualKeyCode::R => R,
        VirtualKeyCode::S => S,
        VirtualKeyCode::T => T,
        VirtualKeyCode::U => U,
        VirtualKeyCode::V => V,
        VirtualKeyCode::W => W,
        VirtualKeyCode::X => X,
        VirtualKeyCode::Y => Y,
        VirtualKeyCode::Z => Z,
        VirtualKeyCode::Key0 => Digit0,
        VirtualKeyCode::Key1 => Digit1,
        VirtualKeyCode::Key2 => Digit2,
        VirtualKeyCode::Key3 => Digit3,
        VirtualKeyCode::Key4 => Digit4,
        VirtualKeyCode::Key5 => Digit5,
        VirtualKeyCode::Key6 => Digit6,
        VirtualKeyCode::Key7 => Digit7,
        VirtualKeyCode::Key8 => Digit8,
        VirtualKeyCode::Key9 => Digit9,
        VirtualKeyCode::F1 => F1,
        VirtualKeyCode::F2 => F2,
        VirtualKeyCode::F3 => F3,
        VirtualKeyCode::F4 => F4,
        VirtualKeyCode::F5 => F5,
        VirtualKeyCode::F6 => F6,
        VirtualKeyCode::F7 => F7,
        VirtualKeyCode::F8 => F8,
        VirtualKeyCode::F9 => F9,
        VirtualKeyCode::F10 => F10,
        VirtualKeyCode::F11 => F11,
        VirtualKeyCode::F12 => F12,
        VirtualKeyCode::Escape => Escape,
        VirtualKeyCode::Return => Enter,
        VirtualKeyCode::Space => Space,
        VirtualKeyCode::Tab => Tab,
        VirtualKeyCode::Back => Backspace,
        VirtualKeyCode::Delete => Delete,
        VirtualKeyCode::Insert => Insert,
        VirtualKeyCode::Home => Home,
        VirtualKeyCode::End => End,
        VirtualKeyCode::PageUp => PageUp,
        VirtualKeyCode::PageDown => PageDown,
        VirtualKeyCode::Left => ArrowLeft,
        VirtualKeyCode::Right => ArrowRight,
        VirtualKeyCode::Up => ArrowUp,
        VirtualKeyCode::Down => ArrowDown,
        _ => Undefined,
    }
}
