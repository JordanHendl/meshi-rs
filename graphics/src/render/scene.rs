use std::ptr::NonNull;

use bento::{
    builder::ComputePipelineBuilder as BentoComputePipelineBuilder, Compiler, OptimizationLevel,
    Request, ShaderLang,
};
use dashi::{
    BindGroup, BindTable, Buffer, BufferInfo, BufferUsage, CommandStream, Context, Handle,
    MemoryVisibility, ShaderResource, ShaderType, cmd::Recording, driver::command::Dispatch,
    utils::gpupool::{DynamicGPUPool, GPUPool},
};
use furikake::{
    GPUState,
    reservations::bindless_camera::ReservedBindlessCamera,
    types::Camera,
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
    pub total_transform: Mat4,
    pub bin_id: u32,
}

#[repr(C)]
pub struct SceneDispatchInfo {
    pub num_bins: u32,
    pub max_objects: u32,
    pub camera_slot: u32,
    pub _padding: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
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
    bin_counts: Handle<Buffer>,
    dispatch: Handle<Buffer>,
    current_camera: Handle<Camera>,
    bin_descriptions: Vec<SceneBin>,
    active_objects: Vec<Handle<SceneObject>>,
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
        let mut ctx: &mut Context = unsafe { self.ctx.as_mut() };
        let state: &State = unsafe { self.state.as_ref() };

        let compiler =
            Compiler::new().map_err(|e| bento::BentoError::InvalidInput(e.to_string()))?;
        let transform_stage = compiler.compile(
            include_str!("shaders/scene_transform.comp.glsl").as_bytes(),
            &Request {
                name: Some("scene_transform".to_string()),
                lang: ShaderLang::Glsl,
                stage: ShaderType::Compute,
                optimization: OptimizationLevel::Performance,
                debug_symbols: false,
            },
        )?;

        let cull_stage = compiler.compile(
            include_str!("shaders/scene_cull.comp.glsl").as_bytes(),
            &Request {
                name: Some("scene_cull".to_string()),
                lang: ShaderLang::Glsl,
                stage: ShaderType::Compute,
                optimization: OptimizationLevel::Performance,
                debug_symbols: false,
            },
        )?;

        let transform_state = BentoComputePipelineBuilder::new()
            .shader_compiled(Some(transform_stage))
            .add_variable(
                "objects",
                ShaderResource::StorageBuffer(self.data.objects_to_process.get_gpu_handle()),
            )
            .build(&mut ctx);

        let mut cull_builder = BentoComputePipelineBuilder::new()
            .shader_compiled(Some(cull_stage))
            .add_variable(
                "objects",
                ShaderResource::StorageBuffer(self.data.objects_to_process.get_gpu_handle()),
            )
            .add_variable(
                "scene_bins",
                ShaderResource::StorageBuffer(self.data.scene_bins),
            )
            .add_variable(
                "culled_bins",
                ShaderResource::StorageBuffer(self.data.draw_bins.get_gpu_handle()),
            )
            .add_variable(
                "bin_counts",
                ShaderResource::StorageBuffer(self.data.bin_counts),
            )
            .add_variable("scene_params", ShaderResource::Buffer(self.data.dispatch));

        if let Ok(binding) = state.binding("meshi_bindless_camera") {
            if let furikake::reservations::ReservedBinding::BindlessBinding(info) =
                binding.binding()
            {
                cull_builder = cull_builder
                    .add_table_variable_with_resources("cameras", info.resources.to_vec());
            }
        }

        let cull_state = cull_builder.build(&mut ctx);

        Ok(SceneComputePipelines {
            cull_state,
            transform_state,
        })
    }

    pub fn new(info: &GPUSceneInfo, state: &mut State) -> Self {
        let ctx: &mut Context = unsafe { &mut (*info.ctx) };
        let max_scene_objects = info.limits.max_num_scene_objects as usize;
        let scene_object_size = std::mem::size_of::<SceneObject>();
        let culled_object_size = std::mem::size_of::<CulledObject>();
        let culled_object_align = std::mem::align_of::<CulledObject>();
        let total_cull_slots = max_scene_objects * info.draw_bins.len();
        let bin_counter_size = std::mem::size_of::<u32>() * info.draw_bins.len();

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

        let bin_counts = ctx
            .make_buffer(&BufferInfo {
                debug_name: &format!("{} Scene Bin Counts", info.name),
                byte_size: bin_counter_size as u32,
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            })
            .expect("Failed to allocate bin counter buffer");

        let dispatch = ctx
            .make_buffer(&BufferInfo {
                debug_name: &format!("{} Scene Dispatch", info.name),
                byte_size: std::mem::size_of::<SceneDispatchInfo>() as u32,
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::UNIFORM,
                initial_data: None,
            })
            .expect("Failed to allocate scene dispatch buffer");

        let _active_camera = ctx
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
            bin_counts,
            dispatch,
            current_camera: Default::default(),
            bin_descriptions: info.draw_bins.to_vec(),
            active_objects: Vec::new(),
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
        let handle = self
            .data
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
            .unwrap();

        self.data.active_objects.push(handle);
        handle
    }

    pub fn release_object(&mut self, handle: Handle<SceneObject>) {
        if let Some(object) = self.data.objects_to_process.get_mut_ref(handle) {
            object.active = 0;
            object.dirty = 0;
        }

        self.data.objects_to_process.release(handle);
        self.data
            .active_objects
            .retain(|existing| existing != &handle);
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
        self.data.draw_bins.clear();

        let Some(cull_state) = self.pipelines.cull_state.as_ref() else {
            return (stream, &self.data.draw_bins);
        };
        let Some(transform_state) = self.pipelines.transform_state.as_ref() else {
            return (stream, &self.data.draw_bins);
        };

        if !cull_state.handle.valid() || !transform_state.handle.valid() {
            return (stream, &self.data.draw_bins);
        }

        if !self.camera.valid() {
            return (stream, &self.data.draw_bins);
        }

        let state: &State = unsafe { self.state.as_ref() };
        let Ok(binding) = state.binding("meshi_bindless_camera") else {
            return (stream, &self.data.draw_bins);
        };
        let Some(bindless_camera) = binding.as_any().downcast_ref::<ReservedBindlessCamera>()
        else {
            return (stream, &self.data.draw_bins);
        };
        let _ = bindless_camera.camera(self.camera);

        let ctx: &mut Context = unsafe { self.ctx.as_mut() };

        if let Ok(mut mapped) = ctx.map_buffer_mut::<u32>(self.data.bin_counts) {
            for count in mapped.iter_mut() {
                *count = 0;
            }
            let _ = ctx.unmap_buffer(self.data.bin_counts);
        }

        let camera_slot = match binding.binding() {
            furikake::reservations::ReservedBinding::BindlessBinding(info)
                if (self.camera.slot as usize) < info.resources.len() =>
            {
                self.camera.slot as u32
            }
            _ => u32::MAX,
        };

        if let Ok(mut mapped) = ctx.map_buffer_mut::<SceneDispatchInfo>(self.data.dispatch) {
            mapped[0] = SceneDispatchInfo {
                num_bins: self.data.bin_descriptions.len() as u32,
                max_objects: self.data.objects_to_process.len() as u32,
                camera_slot,
                _padding: 0,
            };
            let _ = ctx.unmap_buffer(self.data.dispatch);
        }

        let _ = self.data.objects_to_process.sync_up(&mut stream);

        let workgroup_size = 64u32;
        let num_objects = self.data.objects_to_process.len() as u32;
        if num_objects == 0 {
            return (stream, &self.data.draw_bins);
        }

        let dispatch_x = (num_objects + workgroup_size - 1) / workgroup_size;

        stream = stream
            .dispatch(&Dispatch {
                x: dispatch_x,
                y: 1,
                z: 1,
                pipeline: transform_state.handle,
                bind_groups: transform_state.bindings(),
                bind_tables: transform_state.tables(),
                dynamic_buffers: Default::default(),
            })
            .unbind_pipeline();

        stream = stream
            .dispatch(&Dispatch {
                x: dispatch_x,
                y: 1,
                z: 1,
                pipeline: cull_state.handle,
                bind_groups: cull_state.bindings(),
                bind_tables: cull_state.tables(),
                dynamic_buffers: Default::default(),
            })
            .unbind_pipeline();

        (stream, &self.data.draw_bins)
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
