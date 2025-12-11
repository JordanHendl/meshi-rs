use std::ptr::NonNull;

use arrayvec::ArrayVec;
use dashi::{
    BindGroup, BindTable, Buffer, BufferInfo, BufferUsage, CommandStream, ComputePipeline, Context,
    Handle, IndexedResource, MemoryVisibility, PipelineShaderInfo, ShaderInfo, ShaderResource,
    ShaderType,
    builders::{
        BindGroupBuilder, BindGroupLayoutBuilder, BindTableBuilder, BindTableLayoutBuilder,
        ComputePipelineBuilder, ComputePipelineLayoutBuilder,
    },
    cmd::Recording,
    driver::command::Dispatch,
    utils::gpupool::{DynamicGPUPool, GPUPool},
};
use furikake::{
    GPUState,
    reservations::bindless_camera::ReservedBindlessCamera,
    types::{Camera, Material},
};
use glam::Mat4;
use noren::meta::DeviceModel;
use resource_pool::resource_list::ResourceList;
use tracing::error;
#[repr(C)]
pub struct SceneObjectInfo {
    pub local: Mat4,
    pub global: Mat4,
    pub scene_mask: u32,
}

#[repr(C)]
pub struct SceneObject {
    pub local_transform: Mat4,
    pub world_transform: Mat4,
    pub scene_mask: u32,
    pub parent_slot: u32,
    pub dirty: u32,
    pub active: u32,
    pub parent: Handle<SceneObject>,
    pub child_count: u32,
    pub children: [Handle<SceneObject>; 16],
}

#[repr(C)]
pub struct CulledObject {
    total_transform: Mat4,
}

#[repr(C)]
pub struct SceneBin {
    pub id: u32,
    pub mask: u32,
}

pub struct GPUSceneLimits {
    pub max_num_scene_objects: u32,
}

pub struct GPUSceneInfo<'a> {
    pub name: &'a str,
    pub ctx: *mut Context,
    pub draw_bins: &'a [SceneBin],
    pub limits: GPUSceneLimits,
}

impl<'a> Default for GPUSceneInfo<'a> {
    fn default() -> Self {
        Self {
            name: Default::default(),
            ctx: Default::default(),
            draw_bins: Default::default(),
            limits: GPUSceneLimits {
                max_num_scene_objects: 0,
            },
        }
    }
}

struct SceneData {
    scene_bins: Handle<Buffer>, // A buffer of scene bin descriptions... this is used to know which
    // bins to put each scene object into when it passes the cull test.
    objects_to_process: GPUPool<SceneObject>, // Scene objects to be culled.
    draw_bins: DynamicGPUPool,                // In format [0..num_bins][0..max_bin_size] but flat.
    current_camera: Handle<Camera>,
}

#[derive(Default)]
struct SceneComputePipelines {
    cull_state: Option<bento::builder::CSO>,
    transform_state: Option<bento::builder::CSO>,
}

pub struct GPUScene<State: GPUState> {
    state: NonNull<State>,
    ctx: NonNull<Context>,
    data: SceneData,
    pipelines: SceneComputePipelines,
    camera: Handle<Camera>,
}

impl<State: GPUState> GPUScene<State> {
    fn make_pipelines(&mut self) -> Result<SceneComputePipelines, bento::BentoError> {
        let compiler = bento::Compiler::new().map_err(|err| {
            error!("Failed to create Bento compiler for scene culling: {err}");
            err
        })?;

        let request = bento::Request {
            name: Some("scene_cull".to_string()),
            lang: bento::ShaderLang::Glsl,
            stage: dashi::ShaderType::Compute,
            optimization: bento::OptimizationLevel::Performance,
            debug_symbols: cfg!(debug_assertions),
        };

        let res = compiler
            .compile(include_bytes!("shaders/scene_cull.comp.glsl"), &request)
            .map_err(|err| {
                error!("Failed to compile scene culling shader: {err}");
                err
            })?;

        //////////////////////////////////////////////////////////////////

        let meshi_bindless_camera = unsafe { self.state.as_mut().binding("meshi_bindless_camera") }
            .expect("Unable to get shader variable!");

        let builder = bento::builder::ComputePipelineBuilder::new();
        builder
            .shader_compiled(Some(res))
            .build(unsafe { self.ctx.as_mut() });

        todo!()
    }

    pub fn new(info: &GPUSceneInfo, state: &mut State) -> Self {
        let ctx: &mut Context = unsafe { &mut (*info.ctx) };
        let max_scene_objects = info.limits.max_num_scene_objects as usize;
        let scene_object_size = std::mem::size_of::<SceneObject>();
        let culled_object_size = std::mem::size_of::<CulledObject>();
        let culled_object_align = std::mem::align_of::<CulledObject>();
        let total_cull_slots = max_scene_objects * info.draw_bins.len();

        if State::reserved_names()
            .iter()
            .find(|name| **name == "meshi_bindless_camera")
            == None
        {
            // Throw error result here.... we NEED meshi_bindless_materials for material listings.
            panic!()
        }

        let scene_bin_size = std::mem::size_of::<SceneBin>() * info.draw_bins.len();

        let objects_to_process = GPUPool::new(
            ctx,
            &BufferInfo {
                debug_name: &format!("{} Scene Objects", info.name),
                byte_size: (scene_object_size * max_scene_objects) as u32,
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            },
        )
        .unwrap();

        let draw_bins = DynamicGPUPool::new(
            ctx,
            &BufferInfo {
                debug_name: &format!("{} Scene Cull Bins", info.name),
                byte_size: (culled_object_size * total_cull_slots) as u32,
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            },
            culled_object_size,
            culled_object_align,
        )
        .unwrap();

        let scene_bins = ctx
            .make_buffer(&BufferInfo {
                debug_name: &format!("{} Draw Bin Descriptions", info.name),
                byte_size: scene_bin_size as u32,
                visibility: MemoryVisibility::Gpu,
                usage: BufferUsage::STORAGE,
                initial_data: unsafe { Some(info.draw_bins.align_to::<u8>().1) },
            })
            .expect("");

        let active_camera = ctx
            .make_buffer(&BufferInfo {
                debug_name: &format!("{} Active Camera", info.name),
                byte_size: std::mem::size_of::<Camera>() as u32,
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::UNIFORM,
                initial_data: None,
            })
            .expect("Failed to allocate camera buffer");

        let data = SceneData {
            scene_bins,
            objects_to_process,
            draw_bins,
            current_camera: Default::default(),
        };

        let mut s = Self {
            state: NonNull::new(state).unwrap(),
            ctx: NonNull::new(info.ctx).unwrap(),
            data,
            camera: Default::default(),
            pipelines: Default::default(),
        };

        s.pipelines = s.make_pipelines().unwrap();

        s
    }

    pub fn set_active_camera(&mut self, camera: Handle<Camera>) {
        self.camera = camera;
        self.update_active_camera_buffer();
    }

    pub fn register_object(&mut self, info: &SceneObjectInfo) -> Handle<SceneObject> {
        self.data
            .objects_to_process
            .insert(SceneObject {
                local_transform: info.local,
                world_transform: info.global,
                scene_mask: info.scene_mask,
                parent_slot: u32::MAX,
                active: 1,
                dirty: 0,
                parent: Default::default(),
                child_count: 0,
                children: [Handle::default(); 16],
            })
            .unwrap()
    }

    pub fn release_object(&mut self, handle: Handle<SceneObject>) {
        if let Some(object) = self.data.objects_to_process.get_mut_ref(handle) {
            object.active = 0;
            object.dirty = 0;
        }

        self.data.objects_to_process.release(handle);
    }

    pub fn transform_object(&mut self, handle: Handle<SceneObject>, transform: &Mat4) {
        {
            let object = self.data.objects_to_process.get_mut_ref(handle).expect("");
            object.local_transform *= *transform;
            object.dirty = 1;
        }
    }

    pub fn set_object_transform(&mut self, handle: Handle<SceneObject>, transform: &Mat4) {
        {
            let object = self.data.objects_to_process.get_mut_ref(handle).expect("");
            object.local_transform = *transform;
            object.dirty = 1;
        }
    }

    pub fn add_child(&mut self, parent: Handle<SceneObject>, child: Handle<SceneObject>) {
        if let Some(parent_ref) = self.data.objects_to_process.get_mut_ref(parent) {
            if (parent_ref.child_count as usize) < parent_ref.children.len() {
                let idx = parent_ref.child_count as usize;
                parent_ref.children[idx] = child;
                parent_ref.child_count += 1;
            }
        }

        if let Some(child_ref) = self.data.objects_to_process.get_mut_ref(child) {
            child_ref.parent = parent;
            child_ref.parent_slot = parent.slot as u32;
            child_ref.dirty = 1;
        }
    }

    pub fn remove_child(&mut self, parent: Handle<SceneObject>, child: Handle<SceneObject>) {
        let mut child_idx: i32 = -1;
        for (i, ch) in self
            .data
            .objects_to_process
            .get_mut_ref(parent)
            .expect("")
            .children
            .iter()
            .enumerate()
        {
            if child == *ch {
                child_idx = i as i32;
                break;
            }
        }

        if child_idx != -1 {
            if let Some(parent_ref) = self.data.objects_to_process.get_mut_ref(parent) {
                for i in child_idx as usize..(parent_ref.child_count as usize - 1) {
                    parent_ref.children[i] = parent_ref.children[i + 1];
                }
                parent_ref.child_count = parent_ref.child_count.saturating_sub(1);
                parent_ref.children[parent_ref.child_count as usize] = Handle::default();
            }
            if let Some(child_ref) = self.data.objects_to_process.get_mut_ref(child) {
                child_ref.parent = Handle::default();
                child_ref.parent_slot = u32::MAX;
                child_ref.dirty = 1;
            }
        }
    }

    pub fn cull(&mut self) -> (CommandStream<Recording>, &DynamicGPUPool) {
        let mut stream = CommandStream::new().begin();
        self.update_active_camera_buffer();
        self.data.objects_to_process.sync_up(&mut stream);

        stream = self.dispatch_transform_updates(stream);

        stream = stream
            .dispatch(&Dispatch {
                x: todo!(),
                y: todo!(),
                z: todo!(),
                pipeline: todo!(),
                bind_groups: todo!(),
                bind_tables: todo!(),
                dynamic_buffers: todo!(),
            })
            .unbind_pipeline();

        self.data.draw_bins.sync_down(&mut stream);

        return (stream, &self.data.draw_bins);
        todo!(
            "Records compute shader dispatches to the gpu, returns ref to dynamic gpu pool. Pool is formatted like [0..num_bins][0..max_bin_meshes] of types CullResult"
        )

        // Idea is: Either user can pull this in a full GPU driven renderer with inderect
        // drawing.... or they can just pull to CPU and then iterate through draw list.
    }

    fn dispatch_transform_updates(
        &mut self,
        stream: CommandStream<Recording>,
    ) -> CommandStream<Recording> {
        let cull_state = self.pipelines.cull_state.as_ref().unwrap();
        if !cull_state.handle.valid() {
            return stream;
        }

        let num_objects = self.data.objects_to_process.len() as u32;
        if num_objects == 0 {
            return stream;
        }

        let workgroup_size = 64u32;
        let dispatch_x = (num_objects + workgroup_size - 1) / workgroup_size;

        let s = stream
            .dispatch(&Dispatch {
                x: dispatch_x,
                y: 1,
                z: 1,
                pipeline: cull_state.handle,
                bind_groups: [
                    todo!(),
                    Default::default(),
                    Default::default(),
                    Default::default(),
                ],
                bind_tables: [
                    todo!(),
                    Default::default(),
                    Default::default(),
                    Default::default(),
                ],
                dynamic_buffers: Default::default(),
            })
            .unbind_pipeline();

        return s;
    }

    fn update_active_camera_buffer(&mut self) {
        if !self.camera.valid() {
            return;
        }

        //        let state: &State = unsafe { self.state.as_ref() };
        //        if let Ok(binding) = state.binding("meshi_bindless_camera") {
        //            if let Some(bindless_camera) = binding.as_any().downcast_ref::<ReservedBindlessCamera>()
        //            {
        //                let ctx: &mut Context = unsafe { self.ctx.as_mut() };
        //                match ctx.map_buffer_mut::<Camera>(self.data.curr_camera) {
        //                    Ok(mapped) => {
        //                        mapped[0] = *bindless_camera.camera(self.camera);
        //
        //                        if let Err(err) = ctx.unmap_buffer(self.pipeline_data.curr_camera) {
        //                            error!("Failed to unmap active camera buffer: {err:?}");
        //                        }
        //                    }
        //                    Err(err) => {
        //                        error!("Failed to map active camera buffer: {err:?}");
        //                    }
        //                }
        //            }
        //        }
    }
}
