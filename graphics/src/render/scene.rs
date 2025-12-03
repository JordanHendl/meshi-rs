use std::ptr::NonNull;

use arrayvec::ArrayVec;
use dashi::{
    BindGroup, BindTable, Buffer, BufferInfo, BufferUsage, CommandStream, ComputePipeline, Context,
    Handle, MemoryVisibility,
    cmd::Recording,
    driver::command::Dispatch,
    utils::gpupool::{DynamicGPUPool, GPUPool},
};
use furikake::{
    GPUState,
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
    scene_mask: u32,
    parent: Handle<SceneObject>,
    local_transform: Mat4,
    world_transform: Mat4,
    children: ArrayVec<Handle<SceneObject>, 16>,
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
}

impl<'a> Default for GPUSceneInfo<'a> {
    fn default() -> Self {
        Self {
            name: Default::default(),
            ctx: Default::default(),
            draw_bins: Default::default(),
        }
    }
}

struct GPUScenePipelineData {
    pipeline: Handle<ComputePipeline>,
    bind_groups: ArrayVec<Option<Handle<BindGroup>>, 4>,
    bind_tables: ArrayVec<Option<Handle<BindTable>>, 4>,
    curr_camera: Handle<Buffer>,
}

pub struct GPUScene<State: GPUState> {
    state: NonNull<State>,
    ctx: NonNull<Context>,
    scene_bins: Handle<Buffer>, // A buffer of scene bin descriptions... this is used to know which
    // bins to put each scene object into when it passes the cull test.
    objects_to_process: GPUPool<SceneObject>, // Scene objects to be culled.
    draw_bins: DynamicGPUPool,                // In format [0..num_bins][0..max_bin_size] but flat.
    pipeline_data: GPUScenePipelineData,
    camera: Handle<Camera>,
}

impl<State: GPUState> GPUScene<State> {
    fn make_bento_shader() -> Result<bento::CompilationResult, bento::BentoError> {
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

        compiler.compile(include_bytes!("shaders/scene_cull.comp.glsl"), &request)
            .map_err(|err| {
                error!("Failed to compile scene culling shader: {err}");
                err
            })
    }

    pub fn new(info: &GPUSceneInfo, state: &mut State) -> Self {
        let ctx: &mut Context = unsafe { &mut (*info.ctx) };
        if State::reserved_names()
            .iter()
            .find(|name| **name == "meshi_bindless_materials")
            == None
        {
            // Throw error result here.... we NEED meshi_bindless_materials for material listings.
            panic!()
        }

        if State::reserved_names()
            .iter()
            .find(|name| **name == "meshi_bindless_camera")
            == None
        {
            // Throw error result here.... we NEED meshi_bindless_materials for material listings.
            panic!()
        }

        let scene_bin_size = std::mem::size_of::<SceneBin>() * info.draw_bins.len();

        Self {
            state: NonNull::new(state).unwrap(),
            ctx: NonNull::new(info.ctx).unwrap(),
            objects_to_process: GPUPool::new(
                ctx,
                &BufferInfo {
                    debug_name: todo!(),
                    byte_size: todo!(),
                    visibility: todo!(),
                    usage: todo!(),
                    initial_data: todo!(),
                },
            )
            .unwrap(),
            draw_bins: DynamicGPUPool::new(
                ctx,
                &BufferInfo {
                    debug_name: todo!(),
                    byte_size: todo!(),
                    visibility: todo!(),
                    usage: todo!(),
                    initial_data: todo!(),
                },
                0,
                0,
            )
            .unwrap(),
            camera: Default::default(),
            scene_bins: ctx
                .make_buffer(&BufferInfo {
                    debug_name: &format!("{} Draw Bin Descriptions", info.name),
                    byte_size: scene_bin_size as u32,
                    visibility: MemoryVisibility::Gpu,
                    usage: BufferUsage::STORAGE,
                    initial_data: unsafe { Some(info.draw_bins.align_to::<u8>().1) },
                })
                .expect(""),
            pipeline_data: todo!(),
        }
    }

    pub fn set_active_camera(&mut self, camera: Handle<Camera>) {
        todo!("Set active camera in furikake state's active camera resources to use.")
    }

    pub fn register_object(&mut self, info: &SceneObjectInfo) -> Handle<SceneObject> {
        self.objects_to_process
            .insert(SceneObject {
                local_transform: info.local,
                world_transform: info.global,
                scene_mask: info.scene_mask,
                parent: Default::default(),
                children: Default::default(),
            })
            .unwrap()
    }

    pub fn release_object(&mut self, handle: Handle<SceneObject>) {
        self.objects_to_process.release(handle);
    }

    pub fn transform_object(&mut self, handle: Handle<SceneObject>, transform: &Mat4) {
        self.objects_to_process
            .get_mut_ref(handle)
            .expect("")
            .local_transform *= *transform;
        todo!("Modify scene object transform")
    }

    pub fn set_object_transform(&mut self, handle: Handle<SceneObject>, transform: &Mat4) {
        self.objects_to_process
            .get_mut_ref(handle)
            .expect("")
            .local_transform = *transform;
        todo!("Set scene object transform")
    }

    pub fn add_child(&mut self, parent: Handle<SceneObject>, child: Handle<SceneObject>) {
        self.objects_to_process
            .get_mut_ref(parent)
            .expect("")
            .children
            .push(child);
        self.objects_to_process.get_mut_ref(child).expect("").parent = parent;
    }

    pub fn remove_child(&mut self, parent: Handle<SceneObject>, child: Handle<SceneObject>) {
        let mut child_idx: i32 = -1;
        for (i, ch) in self
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
            self.objects_to_process
                .get_mut_ref(parent)
                .unwrap()
                .children
                .remove(child_idx as usize);
        }
    }

    pub fn cull(&mut self) -> (CommandStream<Recording>, &DynamicGPUPool) {
        let mut stream = CommandStream::new().begin();
        self.objects_to_process.sync_up(&mut stream);

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

        self.draw_bins.sync_down(&mut stream);

        return (stream, &self.draw_bins);
        todo!(
            "Records compute shader dispatches to the gpu, returns ref to dynamic gpu pool. Pool is formatted like [0..num_bins][0..max_bin_meshes] of types CullResult"
        )

        // Idea is: Either user can pull this in a full GPU driven renderer with inderect
        // drawing.... or they can just pull to CPU and then iterate through draw list.
    }
}
