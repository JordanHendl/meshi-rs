pub mod event;
pub mod structs;
mod renderer;

pub use structs::*;
use noren::*;
use std::ffi::c_void;
use dashi::{Context, Display, DisplayInfo, Handle};
use glam::{Mat4, Vec3, Vec4};
use meshi_ffi_structs::FFIMeshObjectInfo;
use meshi_utils::MeshiError;

pub struct RenderEngine {
    ctx: Context,
    display: Option<Display>,
    database: DB,
}

impl RenderEngine {
    pub fn new(info: &RenderEngineInfo) -> Result<Self, MeshiError> {
        let mut ctx = if info.headless {
            Context::new(&Default::default())?
        } else {
            Context::headless(&Default::default())?
        };
        let display = if info.headless {
            Some(ctx.make_display(&DisplayInfo {
                window: todo!(),
                vsync: todo!(),
                buffering: todo!(),
            })?)
        } else {
            None
        };
        let database = DB::new(&DBInfo {
            ctx: &mut ctx,
            base_dir: &info.database_path,
            layout_file: None,
        })?;

        Ok(Self {
            ctx,
            display,
            database,
        })
    }

    pub fn shut_down(&mut self) {
        todo!()
    }

    pub fn register_directional_light(
        &mut self,
        info: &DirectionalLightInfo,
    ) -> Handle<DirectionalLight> {
        todo!()
    }

    pub fn set_directional_light_transform(
        &mut self,
        handle: Handle<DirectionalLight>,
        transform: &Mat4,
    ) {
        todo!()
    }

    pub fn set_directional_light_info(
        &mut self,
        handle: Handle<DirectionalLight>,
        info: &DirectionalLightInfo,
    ) {
        todo!()
    }

    pub fn release_directional_light(&mut self, handle: Handle<DirectionalLight>) {
        todo!()//self.directional_lights.release(handle);
    }

    pub fn register_mesh_object(
        &mut self,
        info: &FFIMeshObjectInfo,
    ) -> Result<Handle<MeshObject>, MeshiError> {
        todo!()
    }

    pub fn release_mesh_object(&mut self, handle: Handle<MeshObject>) {
        todo!()
    }

    pub fn register_mesh_with_renderer(&mut self, handle: Handle<MeshObject>) {
        todo!()
    }

    pub fn create_cube(&mut self) -> Handle<MeshObject> {
        todo!()
    }

    pub fn create_cube_ex(&mut self, info: &CubePrimitiveInfo) -> Handle<MeshObject> {
        todo!()
    }

    pub fn create_sphere(&mut self) -> Handle<MeshObject> {
        todo!()
    }

    pub fn create_sphere_ex(&mut self, info: &SpherePrimitiveInfo) -> Handle<MeshObject> {
        todo!()
    }

    pub fn create_cylinder(&mut self) -> Handle<MeshObject> {
        todo!()
    }

    pub fn create_cylinder_ex(&mut self, info: &CylinderPrimitiveInfo) -> Handle<MeshObject> {
        todo!()
    }

    pub fn create_plane(&mut self) -> Handle<MeshObject> {
        todo!()
    }

    pub fn create_plane_ex(&mut self, info: &PlanePrimitiveInfo) -> Handle<MeshObject> {
        todo!()
    }

    pub fn create_cone(&mut self) -> Handle<MeshObject> {
        todo!()
    }

    pub fn create_cone_ex(&mut self, info: &ConePrimitiveInfo) -> Handle<MeshObject> {
        todo!()
    }

    pub fn create_triangle(&mut self) -> Handle<MeshObject> {
        todo!()
    }

    pub fn set_mesh_object_transform(
        &mut self,
        handle: Handle<MeshObject>,
        transform: &glam::Mat4,
    ) {
        todo!()
    }

    pub fn update(&mut self, _delta_time: f32) {
        //        use winit::event_loop::ControlFlow;
        //        use winit::platform::run_return::EventLoopExtRunReturn;
        //
        //        if self.event_cb.is_some() {
        //            let cb = self.event_cb.as_mut().unwrap();
        //            let mut triggered = false;
        //
        //            if let Some(event_loop) = &mut self.event_loop {
        //                event_loop.run_return(|event, _target, control_flow| {
        //                    *control_flow = ControlFlow::Exit;
        //                    if let Some(mut e) = event::from_winit_event(&event) {
        //                        triggered = true;
        //                        let c = cb.event_cb;
        //                        c(&mut e, cb.user_data);
        //                    }
        //                });
        //            }
        //
        //            if !triggered {
        //                let mut synthetic: event::Event = unsafe { std::mem::zeroed() };
        //                let c = cb.event_cb;
        //                c(&mut synthetic, cb.user_data);
        //            }
        //        }
        //
    }

    pub fn render_to_image(&mut self, extent: [u32; 2]) -> Result<RgbaImage, MeshiError> {
        todo!()
    }

    pub fn set_projection(&mut self, proj: &Mat4) {
        todo!()
    }

    pub fn set_capture_mouse(&mut self, capture: bool) {
        let _ = capture; // window management handled by renderer
    }
    pub fn set_camera(&mut self, camera: &Mat4) {
        todo!()
    }

    pub fn camera_position(&self) -> Vec3 {
        todo!()
    }

    pub fn set_event_cb(
        &mut self,
        event_cb: extern "C" fn(*mut event::Event, *mut c_void),
        user_data: *mut c_void,
    ) {
//        self.event_cb = Some(EventCallbackInfo {
//            event_cb,
//            user_data,
//        });
    }
}
