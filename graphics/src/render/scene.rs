use std::ptr::NonNull;

use bento::builder::CSOBuilder;
use dashi::*;
use dashi::{
    Buffer, BufferInfo, BufferUsage, BufferView, CommandStream, Context, Handle, MemoryVisibility,
    ShaderResource, UsageBits,
    cmd::Executable,
    driver::command::Dispatch,
    utils::gpupool::{DynamicGPUPool, GPUPool},
};
use furikake::{
    BindlessState, GPUState,
    reservations::bindless_transformations::ReservedBindlessTransformations,
    types::{Camera, Transformation},
};
use glam::Mat4;
use tare::utils::StagedBuffer;
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
    pub transformation: u32,
    pub parent_slot: u32,
    pub dirty: u32,
    pub active: u32,
    pub parent: u32,
    pub child_count: u32,
    pub children: [u32; 16],
}

#[repr(C)]
pub struct CulledObject {
    pub total_transform: Mat4,
    pub transformation: u32,
    pub object_id: u32,
    pub bin_id: u32,
}

#[repr(C)]
struct SceneDispatchInfo {
    pub num_bins: u32,
    pub max_objects: u32,
    pub num_views: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SceneBin {
    pub id: u32,
    pub mask: u32,
}

pub struct GPUSceneLimits {
    pub max_num_scene_objects: u32,
    pub max_num_views: u32,
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
                max_num_scene_objects: 2048,
                max_num_views: 4,
            },
        }
    }
}

const MAX_ACTIVE_VIEWS: usize = 8;

#[repr(C)]
#[derive(Clone, Copy)]
struct ActiveCameras {
    pub count: u32,
    pub _padding: [u32; 3],
    pub slots: [u32; MAX_ACTIVE_VIEWS],
}

impl Default for ActiveCameras {
    fn default() -> Self {
        Self {
            count: 0,
            _padding: [0; 3],
            slots: [u32::MAX; MAX_ACTIVE_VIEWS],
        }
    }
}

struct SceneData {
    scene_bins: Handle<Buffer>, // A buffer of scene bin descriptions... this is used to know which
    // bins to put each scene object into when it passes the cull test.
    objects_to_process: GPUPool<SceneObject>, // Scene objects to be culled.
    draw_bins: DynamicGPUPool,                // In format [0..num_bins][0..max_bin_size] but flat.
    bin_counts: StagedBuffer,
    dispatch: StagedBuffer,
    transformations: ShaderResource,
    transformations_buffer: BufferView,
    bin_descriptions: Vec<SceneBin>,
    active_objects: Vec<Handle<SceneObject>>,
    max_views: u32,
}

#[derive(Default)]
struct SceneComputePipelines {
    cull_state: Option<bento::builder::CSO>,
    transform_state: Option<bento::builder::CSO>,
}

pub struct GPUScene {
    state: NonNull<BindlessState>,
    ctx: NonNull<Context>,
    data: SceneData,
    pipelines: SceneComputePipelines,
    camera: StagedBuffer,
}

impl GPUScene {
    const INVALID_HANDLE: u32 = u32::MAX;

    pub(crate) fn pack_handle<T>(handle: Handle<T>) -> u32 {
        ((handle.generation as u32) << 16) | handle.slot as u32
    }

    pub(crate) fn unpack_handle<T>(packed: u32) -> Handle<T> {
        if packed == Self::INVALID_HANDLE {
            return Handle::default();
        }

        let slot = (packed & 0xFFFF) as u16;
        let generation = (packed >> 16) as u16;
        Handle::new(slot, generation)
    }

    fn alloc_transform(&mut self, initial: Mat4) -> Handle<Transformation> {
        let mut handle = Handle::default();
        let state: &mut BindlessState = unsafe { self.state.as_mut() };

        state
            .reserved_mut::<ReservedBindlessTransformations, _>(
                "meshi_bindless_transformations",
                |transforms| {
                    handle = transforms.add_transform();
                    transforms.transform_mut(handle).transform = initial;
                },
            )
            .expect("allocate bindless transform");

        handle
    }

    fn release_transform(&mut self, handle: Handle<Transformation>) {
        let state: &mut BindlessState = unsafe { self.state.as_mut() };
        state
            .reserved_mut::<ReservedBindlessTransformations, _>(
                "meshi_bindless_transformations",
                |transforms| {
                    transforms.remove_transform(handle);
                },
            )
            .expect("release bindless transform");
    }

    fn make_pipelines(&mut self) -> Result<SceneComputePipelines, bento::BentoError> {
        let mut ctx: &mut Context = unsafe { self.ctx.as_mut() };
        let state: &BindlessState = unsafe { self.state.as_ref() };

        let Ok(binding) = state.binding("meshi_bindless_cameras") else {
            return Err(bento::BentoError::InvalidInput("lmao".to_string()));
        };

        let furikake::reservations::ReservedBinding::TableBinding {
            binding: _,
            resources,
        } = binding.binding();

        let transform_state = CSOBuilder::new()
            .shader(Some(
                include_str!("shaders/scene_transform.comp.glsl").as_bytes(),
            ))
            .add_variable(
                "in_list",
                ShaderResource::StorageBuffer(self.data.objects_to_process.get_gpu_handle().into()),
            )
            .add_variable("transformations", self.data.transformations.clone())
            .build(&mut ctx);

        let cull_state = CSOBuilder::new()
            .shader(Some(
                include_str!("shaders/scene_cull.comp.glsl").as_bytes(),
            ))
            .add_variable("cameras", resources[0].resource.clone())
            .add_variable(
                "objects",
                ShaderResource::StorageBuffer(self.data.objects_to_process.get_gpu_handle().into()),
            )
            .add_variable(
                "bins",
                ShaderResource::StorageBuffer(self.data.scene_bins.into()),
            )
            .add_variable(
                "culled",
                ShaderResource::StorageBuffer(self.data.draw_bins.get_gpu_handle().into()),
            )
            .add_variable(
                "counts",
                ShaderResource::StorageBuffer(self.data.bin_counts.device().into()),
            )
            .add_variable(
                "camera",
                ShaderResource::ConstBuffer(self.camera.device().into()),
            )
            .add_variable(
                "params",
                ShaderResource::ConstBuffer(self.data.dispatch.device().into()),
            )
            .build(&mut ctx).unwrap();

        Ok(SceneComputePipelines {
            cull_state: Some(cull_state),
            transform_state: transform_state.ok(),
        })
    }

    pub fn new(info: &GPUSceneInfo, state: &mut BindlessState) -> Self {
        let ctx: &mut Context = unsafe { &mut (*info.ctx) };
        let max_scene_objects = info.limits.max_num_scene_objects as usize;
        let scene_object_size = std::mem::size_of::<SceneObject>();
        let culled_object_size = std::mem::size_of::<CulledObject>();
        let culled_object_align = std::mem::align_of::<CulledObject>();
        let max_views = info.limits.max_num_views as usize;
        let total_cull_slots = max_scene_objects * info.draw_bins.len() * max_views;
        let bin_counter_size = std::mem::size_of::<u32>() * info.draw_bins.len() * max_views;

        if BindlessState::reserved_names()
            .iter()
            .find(|name| **name == "meshi_bindless_cameras")
            == None
        {
            // Throw error result here.... we NEED meshi_bindless_materials for material listings.
            panic!()
        }

        if BindlessState::reserved_names()
            .iter()
            .find(|name| **name == "meshi_bindless_transformations")
            == None
        {
            panic!()
        }

        let scene_bin_size = std::mem::size_of::<SceneBin>() * info.draw_bins.len();

        let transformation_binding = state
            .binding("meshi_bindless_transformations")
            .expect("missing bindless transformations");

        let furikake::reservations::ReservedBinding::TableBinding {
            binding: _,
            resources: transformation_resources,
        } = transformation_binding.binding();

        let transformations = transformation_resources[0].resource.clone();
        let transformations_buffer = match transformation_resources[0].resource {
            ShaderResource::StorageBuffer(view) => view,
            _ => panic!("bindless transformations should be a storage buffer"),
        };

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
        let mut active_camera = StagedBuffer::new(
            ctx,
            BufferInfo {
                debug_name: &format!("{} camera buffer", info.name),
                byte_size: (std::mem::size_of::<ActiveCameras>() as u32).max(256),
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::UNIFORM,
                initial_data: None,
            },
        );

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

        let mut bin_counts = StagedBuffer::new(
            ctx,
            BufferInfo {
                debug_name: &format!("{} Scene Bin Counts", info.name),
                byte_size: (bin_counter_size as u32).max(256),
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            },
        );

        for count in bin_counts.as_slice_mut() {
            *count = 0;
        }

        let mut dispatch = StagedBuffer::new(
            ctx,
            BufferInfo {
                debug_name: &format!("{} Scene Dispatch", info.name),
                byte_size: (std::mem::size_of::<SceneDispatchInfo>() as u32).max(256),
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::UNIFORM,
                initial_data: None,
            },
        );

        let stream = CommandStream::new()
            .begin()
            .prepare_buffer(dispatch.device().handle, UsageBits::HOST_WRITE);

        let mut queue = ctx
            .pool_mut(QueueType::Graphics)
            .begin("scene_cull_test", false)
            .expect("begin compute queue");

        let (_, fence) = stream
            .end()
            .submit(
                &mut queue,
                &SubmitInfo2 {
                    ..Default::default()
                },
            )
            .unwrap();

        dispatch.as_slice_mut()[0] = SceneDispatchInfo {
            num_bins: info.draw_bins.len() as u32,
            max_objects: info.limits.max_num_scene_objects,
            num_views: info.limits.max_num_views,
        };

        {
            let camera_info = active_camera.as_slice_mut::<ActiveCameras>();
            camera_info[0] = ActiveCameras::default();
        }
        let data = SceneData {
            scene_bins,
            objects_to_process,
            draw_bins,
            bin_counts,
            dispatch,
            transformations,
            transformations_buffer,
            bin_descriptions: info.draw_bins.to_vec(),
            active_objects: Vec::new(),
            max_views: info.limits.max_num_views,
        };

        let mut s = Self {
            state: NonNull::new(state).unwrap(),
            ctx: NonNull::new(info.ctx).unwrap(),
            data,
            camera: active_camera,
            pipelines: Default::default(),
        };

        s.pipelines = s.make_pipelines().unwrap();

        s
    }

    pub fn set_active_camera(&mut self, camera: Handle<Camera>) {
        self.set_active_cameras(&[camera]);
    }

    pub fn set_active_cameras(&mut self, cameras: &[Handle<Camera>]) {
        let active_cameras = self.camera.as_slice_mut::<ActiveCameras>();
        let count = cameras
            .len()
            .min(MAX_ACTIVE_VIEWS)
            .min(self.data.max_views as usize);
        active_cameras[0].count = count as u32;

        active_cameras[0].slots = [u32::MAX; MAX_ACTIVE_VIEWS];
        for (idx, handle) in cameras.iter().take(count).enumerate() {
            active_cameras[0].slots[idx] = handle.slot as u32;
        }

        self.data.dispatch.as_slice_mut::<SceneDispatchInfo>()[0].num_views = count as u32;
    }

    pub fn register_object(
        &mut self,
        info: &SceneObjectInfo,
    ) -> (Handle<SceneObject>, Handle<Transformation>) {
        let transformation = self.alloc_transform(info.global);
        let handle = self
            .data
            .objects_to_process
            .insert(SceneObject {
                local_transform: info.local,
                world_transform: info.global,
                scene_mask: info.scene_mask,
                transformation: Self::pack_handle(transformation),
                parent_slot: u32::MAX,
                active: 1,
                dirty: 0,
                parent: Self::INVALID_HANDLE,
                child_count: 0,
                children: [Self::INVALID_HANDLE; 16],
            })
            .unwrap();

        //        self.data.dispatch.as_slice_mut::<SceneDispatchInfo>()[0].max_objects = self.data.objects_to_process.len() as u32;
        self.data.active_objects.push(handle);
        (handle, transformation)
    }

    pub fn release_object(&mut self, handle: Handle<SceneObject>) {
        if let Some(parent) = self
            .data
            .objects_to_process
            .get_ref(handle)
            .map(|object| Self::unpack_handle(object.parent))
            .filter(|parent| parent.valid())
        {
            self.remove_child(parent, handle);
        }

        if let Some(object) = self.data.objects_to_process.get_ref(handle) {
            let transform_handle = Self::unpack_handle(object.transformation);
            if transform_handle.valid() {
                self.release_transform(transform_handle);
            }
        }

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

    pub fn get_object_transform(&self, handle: Handle<SceneObject>) -> Mat4 {
        let object = self.data.objects_to_process.get_ref(handle).expect("");
        return object.local_transform;
    }

    pub fn add_child(&mut self, parent: Handle<SceneObject>, child: Handle<SceneObject>) {
        if let Some(parent_ref) = self.data.objects_to_process.get_mut_ref(parent) {
            if (parent_ref.child_count as usize) < parent_ref.children.len() {
                let idx = parent_ref.child_count as usize;
                parent_ref.children[idx] = Self::pack_handle(child);
                parent_ref.child_count += 1;
            }
        }

        if let Some(child_ref) = self.data.objects_to_process.get_mut_ref(child) {
            child_ref.parent = Self::pack_handle(parent);
            child_ref.parent_slot = parent.slot as u32;
            child_ref.dirty = 1;
        }
    }

    pub fn remove_child(&mut self, parent: Handle<SceneObject>, child: Handle<SceneObject>) {
        let packed_child = Self::pack_handle(child);
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
            if packed_child == *ch {
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
                parent_ref.children[parent_ref.child_count as usize] = Self::INVALID_HANDLE;
            }
            if let Some(child_ref) = self.data.objects_to_process.get_mut_ref(child) {
                child_ref.parent = Self::INVALID_HANDLE;
                child_ref.parent_slot = u32::MAX;
                child_ref.dirty = 1;
            }
        }
    }

    pub fn cull(&mut self) -> CommandStream<Executable> {
        let stream = CommandStream::new().begin();
        self.data.draw_bins.clear();
        for count in self.data.bin_counts.as_slice_mut::<u32>() {
            *count = 0;
        }

        let workgroup_size = 64u32;
        let num_objects = self.data.objects_to_process.len() as u32;

        let dispatch_x = ((num_objects.max(1) + workgroup_size - 1) / workgroup_size).max(1);

        let Some(transform_state) = self.pipelines.transform_state.as_ref() else {
            error!("No transform state to dispatch!");
            return stream.end();
        };
        let Some(cull_state) = self.pipelines.cull_state.as_ref() else {
            error!("No cull state to dispatch!");
            return stream.end();
        };

        assert!(transform_state.tables()[0].is_some());
        assert!(cull_state.tables()[0].is_some());
        assert!(cull_state.tables()[1].is_some());

        stream
            .combine(self.data.objects_to_process.sync_up().unwrap())
            .combine(self.data.dispatch.sync_up())
            .combine(self.data.bin_counts.sync_up())
            .combine(self.camera.sync_up())
            .prepare_buffer(
                self.data.bin_counts.device().handle,
                UsageBits::COMPUTE_SHADER,
            )
            .prepare_buffer(self.camera.device().handle, UsageBits::COMPUTE_SHADER)
            .prepare_buffer(
                self.data.dispatch.device().handle,
                UsageBits::COMPUTE_SHADER,
            )
            .prepare_buffer(
                self.data.transformations_buffer.handle,
                UsageBits::COMPUTE_SHADER,
            )
            .dispatch(&Dispatch {
                x: dispatch_x,
                y: 1,
                z: 1,
                pipeline: transform_state.handle,
                bind_tables: transform_state.tables(),
                dynamic_buffers: Default::default(),
            })
            .unbind_pipeline()
            .dispatch(&Dispatch {
                x: dispatch_x,
                y: 1,
                z: 1,
                pipeline: cull_state.handle,
                bind_tables: cull_state.tables(),
                dynamic_buffers: Default::default(),
            })
            .unbind_pipeline()
            .end()
    }

    pub fn output_bins(&self) -> &DynamicGPUPool {
        &self.data.draw_bins
    }

    pub fn cull_and_sync(&mut self) -> CommandStream<Executable> {
        CommandStream::new()
            .begin()
            .combine(self.cull())
            .combine(self.data.draw_bins.sync_down().expect("sync culled bins"))
            .combine(self.data.bin_counts.sync_down())
            .combine(self.data.dispatch.sync_down())
            .end()
    }

    pub fn bin_counts(&self) -> &[u32] {
        self.data.bin_counts.as_slice::<u32>()
    }

    pub fn bin_counts_gpu(&self) -> BufferView {
        self.data.bin_counts.device()
    }


    pub fn max_objects_per_bin(&self) -> u32 {
        self.data.dispatch.as_slice::<SceneDispatchInfo>()[0].max_objects
    }

    pub fn num_bins(&self) -> usize {
        self.data.bin_descriptions.len()
    }

    pub fn culled_object(&self, index: u32) -> Option<&CulledObject> {
        self.data
            .draw_bins
            .get_ref::<CulledObject>(Handle::new(index as u16, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dashi::{ContextInfo, DeviceFilter, DeviceSelector, DeviceType, QueueType, SubmitInfo2};
    use furikake::{BindlessState, reservations::bindless_camera::ReservedBindlessCamera};
    use glam::Vec3;

    fn make_test_scene(ctx: &mut Box<Context>, state: &mut Box<BindlessState>) -> GPUScene {
        GPUScene::new(
            &GPUSceneInfo {
                name: "test_scene",
                ctx: ctx.as_mut(),
                draw_bins: &[SceneBin {
                    id: 0,
                    mask: u32::MAX,
                }],
                limits: GPUSceneLimits {
                    max_num_scene_objects: 1024,
                    max_num_views: MAX_ACTIVE_VIEWS as u32,
                },
            },
            state,
        )
    }

    fn setup_scene() -> (Box<Context>, Box<BindlessState>, GPUScene) {
        let device = match DeviceSelector::new()
            .unwrap()
            .select(DeviceFilter::default().add_required_type(DeviceType::Dedicated))
        {
            None => Default::default(),
            Some(d) => d,
        };

        let mut ctx = Box::new(Context::headless(&ContextInfo { device }).expect("create context"));
        let mut state = Box::new(BindlessState::new(ctx.as_mut()));
        let scene = make_test_scene(&mut ctx, &mut state);

        (ctx, state, scene)
    }

    fn make_object_info(local: Mat4, global: Mat4, scene_mask: u32) -> SceneObjectInfo {
        SceneObjectInfo {
            local,
            global,
            scene_mask,
        }
    }

    #[test]
    fn registering_object_tracks_state() {
        let (_ctx, mut state, mut scene) = setup_scene();

        let info = make_object_info(Mat4::IDENTITY, Mat4::IDENTITY, 0xFF);

        let (handle,_) = scene.register_object(&info);

        assert_eq!(scene.data.active_objects.len(), 1);
        assert_eq!(scene.data.active_objects[0], handle);

        let stored = scene.data.objects_to_process.get_ref(handle).unwrap();
        assert_eq!(stored.scene_mask, info.scene_mask);
        assert_eq!(stored.active, 1);
        assert_eq!(stored.local_transform, info.local);
        assert_eq!(stored.world_transform, info.global);
    }

    #[test]
    fn releasing_object_clears_tracking() {
        let (_ctx, mut state, mut scene) = setup_scene();

        let (handle,_) = scene.register_object(&make_object_info(Mat4::IDENTITY, Mat4::IDENTITY, 1));

        scene.release_object(handle);

        assert!(scene.data.active_objects.is_empty());
        assert!(scene.data.objects_to_process.get_ref(handle).is_none());
    }

    #[test]
    fn transforming_object_marks_dirty() {
        let (_ctx, mut state, mut scene) = setup_scene();

        let (handle,_) = scene.register_object(&make_object_info(Mat4::IDENTITY, Mat4::IDENTITY, 1));

        let delta = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
        scene.transform_object(handle, &delta);

        let stored = scene.data.objects_to_process.get_ref(handle).unwrap();
        assert_eq!(stored.local_transform, Mat4::IDENTITY * delta);
        assert_eq!(stored.dirty, 1);
    }

    #[test]
    fn setting_object_transform_replaces_value() {
        let (_ctx, mut state, mut scene) = setup_scene();

        let (handle,_) = scene.register_object(&make_object_info(Mat4::IDENTITY, Mat4::IDENTITY, 1));

        let replacement = Mat4::from_scale(Vec3::splat(2.0));
        scene.set_object_transform(handle, &replacement);

        let stored = scene.data.objects_to_process.get_ref(handle).unwrap();
        assert_eq!(stored.local_transform, replacement);
        assert_eq!(stored.dirty, 1);
    }

    #[test]
    fn adding_and_removing_child_updates_relationships() {
        let (_ctx, mut state, mut scene) = setup_scene();

        let (parent,_) = scene.register_object(&make_object_info(Mat4::IDENTITY, Mat4::IDENTITY, 1));
        let (child,_) = scene.register_object(&make_object_info(Mat4::IDENTITY, Mat4::IDENTITY, 1));

        scene.add_child(parent, child);

        let parent_ref = scene.data.objects_to_process.get_ref(parent).unwrap();
        assert_eq!(parent_ref.child_count, 1);
        assert_eq!(parent_ref.children[0], GPUScene::pack_handle(child));

        let child_ref = scene.data.objects_to_process.get_ref(child).unwrap();
        assert_eq!(child_ref.parent, GPUScene::pack_handle(parent));
        assert_eq!(child_ref.parent_slot, parent.slot as u32);

        scene.remove_child(parent, child);

        let parent_ref = scene.data.objects_to_process.get_ref(parent).unwrap();
        assert_eq!(parent_ref.child_count, 0);
        assert_eq!(parent_ref.children[0], GPUScene::INVALID_HANDLE);

        let child_ref = scene.data.objects_to_process.get_ref(child).unwrap();
        assert_eq!(child_ref.parent, GPUScene::INVALID_HANDLE);
        assert_eq!(child_ref.parent_slot, u32::MAX);
        assert_eq!(child_ref.dirty, 1);
    }

    #[test]
    fn releasing_child_detaches_from_parent() {
        let (_ctx, mut state, mut scene) = setup_scene();

        let (parent,_) = scene.register_object(&make_object_info(Mat4::IDENTITY, Mat4::IDENTITY, 1));
        let (child,_) = scene.register_object(&make_object_info(Mat4::IDENTITY, Mat4::IDENTITY, 1));

        scene.add_child(parent, child);
        scene.release_object(child);

        let parent_ref = scene.data.objects_to_process.get_ref(parent).unwrap();
        assert_eq!(parent_ref.child_count, 0);
        assert_eq!(parent_ref.children[0], GPUScene::INVALID_HANDLE);
    }

    #[test]
    fn setting_active_camera_updates_buffer() {
        let (_ctx, _state, mut scene) = setup_scene();

        let expected_slot: u16 = 7;
        let handle = Handle::<Camera>::new(expected_slot, 1);
        scene.set_active_camera(handle);

        let camera_state = scene.camera.as_slice::<ActiveCameras>()[0];
        assert_eq!(camera_state.count, 1);
        assert_eq!(camera_state.slots[0], expected_slot as u32);
        assert!(camera_state.slots[1..].iter().all(|slot| *slot == u32::MAX));
    }

    #[test]
    fn setting_multiple_active_cameras_clamps_and_orders_slots() {
        let (_ctx, _state, mut scene) = setup_scene();

        let mut handles = Vec::new();
        for i in 0..(MAX_ACTIVE_VIEWS as u16 + 2) {
            handles.push(Handle::<Camera>::new(i, 1));
        }

        scene.set_active_cameras(&handles);

        let camera_state = scene.camera.as_slice::<ActiveCameras>()[0];
        assert_eq!(camera_state.count as usize, MAX_ACTIVE_VIEWS);
        for i in 0..MAX_ACTIVE_VIEWS {
            assert_eq!(camera_state.slots[i], i as u32);
        }
    }

    #[test]
    fn culling_populates_bins_with_parent_child_and_camera() {
        let (mut ctx, mut state, mut scene) = setup_scene();

        let camera_handle = {
            let mut handle = None;
            state
                .reserved_mut::<ReservedBindlessCamera, _>("meshi_bindless_cameras", |cameras| {
                    handle = Some(cameras.add_camera());
                })
                .expect("add camera");
            handle.expect("camera handle")
        };

        state.update().expect("sync camera reservation");

        scene.set_active_camera(camera_handle);

        let parent_transform = Mat4::from_translation(Vec3::new(0.0, 0.0, -2.0));
        let child_local = Mat4::from_translation(Vec3::new(0.0, 0.0, -1.0));

        let (parent,_) = scene.register_object(&make_object_info(
            parent_transform,
            Mat4::IDENTITY,
            u32::MAX,
        ));
        let (child,_) = scene.register_object(&make_object_info(child_local, Mat4::IDENTITY, u32::MAX));

        scene.add_child(parent, child);

        // Record the GPU commands for culling to ensure the pipelines and bindings are
        // properly constructed.
        let commands = scene.cull();

        let mut readback = CommandStream::new()
            .begin()
            .combine(commands)
            .combine(
                scene
                    .data
                    .objects_to_process
                    .sync_down()
                    .expect("download objects"),
            )
            .combine(
                scene
                    .data
                    .draw_bins
                    .sync_down()
                    .expect("download culled bins"),
            )
            .prepare_buffer(scene.data.bin_counts.device().handle, UsageBits::COPY_SRC)
            .combine(scene.data.bin_counts.sync_down())
            .combine(scene.data.dispatch.sync_down())
            .end();

        let mut queue = ctx
            .pool_mut(QueueType::Graphics)
            .begin("scene_cull_test", false)
            .expect("begin compute queue");

        let (_, fence) = readback
            .submit(
                &mut queue,
                &SubmitInfo2 {
                    ..Default::default()
                },
            )
            .unwrap();

        ctx.wait(fence.unwrap()).expect("wait for cull");

        let parent_world = scene
            .data
            .objects_to_process
            .get_ref(parent)
            .expect("parent stored")
            .world_transform;
        let child_world = scene
            .data
            .objects_to_process
            .get_ref(child)
            .expect("child stored")
            .world_transform;

        assert_eq!(
            scene.data.dispatch.as_slice::<SceneDispatchInfo>()[0].max_objects,
            1024
        );
        assert_eq!(parent_world, parent_transform);
        assert_eq!(child_world, parent_transform * child_local);

        let bin_counts = scene.data.bin_counts.as_slice::<u32>();
        assert_eq!(bin_counts[0], 2, "both objects should be visible");

        let first_culled = scene
            .data
            .draw_bins
            .get_ref::<CulledObject>(Handle::new(0, 0))
            .expect("first culled");
        let second_culled = scene
            .data
            .draw_bins
            .get_ref::<CulledObject>(Handle::new(1, 0))
            .expect("second culled");

        assert_eq!(first_culled.bin_id, 0);
        assert_eq!(first_culled.total_transform, child_world);
        assert_eq!(second_culled.bin_id, 0);
        assert_eq!(second_culled.total_transform, parent_world);
    }
}
