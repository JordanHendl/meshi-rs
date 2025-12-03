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

        compiler
            .compile(include_bytes!("shaders/scene_cull.comp.glsl"), &request)
            .map_err(|err| {
                error!("Failed to compile scene culling shader: {err}");
                err
            })
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

        let mut objects_to_process = GPUPool::new(
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

        let mut draw_bins = DynamicGPUPool::new(
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

        let objects_buffer = objects_to_process.get_gpu_handle();
        let draw_bins_buffer = draw_bins.get_gpu_handle();

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

        let pipeline_data = {
            let mut set_variables: [Vec<dashi::BindGroupVariable>; 4] =
                std::array::from_fn(|_| Vec::new());

            if let Ok(shader) = Self::make_bento_shader() {
                for variable in shader.variables.iter() {
                    if let Some(set) = set_variables.get_mut(variable.set as usize) {
                        set.push(variable.kind.clone());
                    }
                }

                if set_variables.iter().all(|set| set.is_empty()) {
                    set_variables[0].extend_from_slice(&[
                        dashi::BindGroupVariable {
                            var_type: dashi::BindGroupVariableType::Storage,
                            binding: 0,
                            count: 1,
                        },
                        dashi::BindGroupVariable {
                            var_type: dashi::BindGroupVariableType::Storage,
                            binding: 1,
                            count: 1,
                        },
                        dashi::BindGroupVariable {
                            var_type: dashi::BindGroupVariableType::Storage,
                            binding: 2,
                            count: 1,
                        },
                        dashi::BindGroupVariable {
                            var_type: dashi::BindGroupVariableType::Uniform,
                            binding: 3,
                            count: 1,
                        },
                    ]);
                }

                let mut bg_layouts = [None, None, None, None];
                let mut bt_layouts = [None, None, None, None];
                let mut bind_groups: ArrayVec<Option<Handle<BindGroup>>, 4> = ArrayVec::new();
                let mut bind_tables: ArrayVec<Option<Handle<BindTable>>, 4> = ArrayVec::new();

                for (set_idx, vars) in set_variables.iter().enumerate() {
                    if vars.is_empty() {
                        bind_groups.push(None);
                        bind_tables.push(None);
                        continue;
                    }

                    let shader_info = ShaderInfo {
                        shader_type: ShaderType::Compute,
                        variables: vars.as_slice(),
                    };

                    let bg_layout_name =
                        format!("{} Scene Culling BG Layout Set {}", info.name, set_idx);
                    let bt_layout_name =
                        format!("{} Scene Culling BT Layout Set {}", info.name, set_idx);

                    let bg_layout = BindGroupLayoutBuilder::new(&bg_layout_name)
                        .shader(shader_info.clone())
                        .build(ctx)
                        .expect("Failed to build bind group layout for scene culling");

                    let bt_layout = BindTableLayoutBuilder::new(&bt_layout_name)
                        .shader(shader_info)
                        .build(ctx)
                        .expect("Failed to build bind table layout for scene culling");

                    bg_layouts[set_idx] = Some(bg_layout);
                    bt_layouts[set_idx] = Some(bt_layout);

                    let bind_group_name =
                        format!("{} Scene Culling Bind Group Set {}", info.name, set_idx);
                    let mut group_builder = BindGroupBuilder::new(&bind_group_name)
                        .layout(bg_layout)
                        .set(set_idx as u32);

                    let mut table_resources: Vec<Vec<IndexedResource>> = Vec::new();

                    for var in vars.iter() {
                        let resource = match var.binding {
                            0 => ShaderResource::StorageBuffer(objects_buffer),
                            1 => ShaderResource::StorageBuffer(draw_bins_buffer),
                            2 => ShaderResource::StorageBuffer(scene_bins),
                            3 => ShaderResource::Buffer(active_camera),
                            _ => ShaderResource::Buffer(active_camera),
                        };

                        group_builder = group_builder.binding(var.binding, resource.clone());
                        table_resources.push(vec![IndexedResource { slot: 0, resource }]);
                    }

                    let bind_group = group_builder
                        .build(ctx)
                        .expect("Failed to build scene culling bind group");

                    let bind_table_name =
                        format!("{} Scene Culling Bind Table Set {}", info.name, set_idx);
                    let mut table_builder = BindTableBuilder::new(&bind_table_name)
                        .layout(bt_layout)
                        .set(set_idx as u32);

                    for (var, resources) in vars.iter().zip(table_resources.iter()) {
                        table_builder = table_builder.binding(var.binding, resources.as_slice());
                    }

                    let bind_table = table_builder
                        .build(ctx)
                        .expect("Failed to build scene culling bind table");

                    bind_groups.push(Some(bind_group));
                    bind_tables.push(Some(bind_table));
                }

                let pipeline_layout = {
                    let mut layout_builder =
                        ComputePipelineLayoutBuilder::new().shader(PipelineShaderInfo {
                            stage: ShaderType::Compute,
                            spirv: shader.spirv.as_slice(),
                            specialization: &[],
                        });

                    for (i, layout) in bg_layouts.iter().enumerate() {
                        if let Some(layout) = layout {
                            layout_builder = layout_builder.bind_group_layout(i, *layout);
                        }
                    }

                    for (i, layout) in bt_layouts.iter().enumerate() {
                        if let Some(layout) = layout {
                            layout_builder = layout_builder.bind_table_layout(i, *layout);
                        }
                    }

                    layout_builder
                        .build(ctx)
                        .expect("Failed to build scene culling pipeline layout")
                };

                let pipeline =
                    ComputePipelineBuilder::new(format!("{} Scene Culling Pipeline", info.name))
                        .layout(pipeline_layout)
                        .build(ctx)
                        .expect("Failed to build scene culling pipeline");

                GPUScenePipelineData {
                    pipeline,
                    bind_groups,
                    bind_tables,
                    curr_camera: active_camera,
                }
            } else {
                GPUScenePipelineData {
                    pipeline: Default::default(),
                    bind_groups: ArrayVec::new(),
                    bind_tables: ArrayVec::new(),
                    curr_camera: active_camera,
                }
            }
        };

        Self {
            state: NonNull::new(state).unwrap(),
            ctx: NonNull::new(info.ctx).unwrap(),
            objects_to_process,
            draw_bins,
            pipeline_data,
            camera: Default::default(),
            scene_bins,
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
