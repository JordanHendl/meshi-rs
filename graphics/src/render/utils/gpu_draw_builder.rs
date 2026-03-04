use std::ptr::NonNull;

use bento::builder::CSOBuilder;
use dashi::*;
use dashi::{
    Buffer, BufferInfo, BufferUsage, CommandStream, Context, Handle, MemoryVisibility,
    ShaderResource, cmd::Executable, driver::command::Dispatch, utils::gpupool::GPUPool,
};
use furikake::BindlessState;

use crate::render::deferred::PerDrawData;

const MAX_DISPATCH_BINS: usize = 56;

pub struct GPUDrawBuilderLimits {
    pub max_num_objects: u32,
}

pub struct GPUDrawBuilderInfo<'a> {
    pub name: &'a str,
    pub ctx: *mut Context,
    pub cull_results: Handle<Buffer>,
    pub bin_counts: Handle<Buffer>,
    pub num_bins: u32,
    pub limits: GPUDrawBuilderLimits,
}

impl<'a> Default for GPUDrawBuilderInfo<'a> {
    fn default() -> Self {
        Self {
            name: Default::default(),
            ctx: Default::default(),
            limits: GPUDrawBuilderLimits {
                max_num_objects: 2048,
            },
            cull_results: Default::default(),
            bin_counts: Default::default(),
            num_bins: 0,
        }
    }
}

struct GPUDrawBuilderData {
    draw_objects: GPUPool<PerDrawData>,
    draw_list: Handle<Buffer>,
    cull_results: Handle<Buffer>,
    bin_counts: Handle<Buffer>,
    num_bins: u32,
    max_objects: u32,
    active_objects: Vec<Handle<PerDrawData>>,
    alloc: DynamicAllocator,
}

#[derive(Default)]
struct GPUDrawBuilderComputePipelines {
    build_draws: Option<bento::builder::CSO>,
}

pub struct GPUDrawBuilder {
    state: NonNull<BindlessState>,
    ctx: NonNull<Context>,
    data: GPUDrawBuilderData,
    pipelines: GPUDrawBuilderComputePipelines,
}

#[derive(Debug, Clone, Copy)]
struct DispatchLayout {
    dispatch_x: u32,
    dispatch_z: u32,
    bins_len: u32,
    bins: [u32; MAX_DISPATCH_BINS],
}

fn build_dispatch_layout(num_objects: u32, bins: &[u32]) -> DispatchLayout {
    let workgroup_size = 64u32;
    let dispatch_x = num_objects.max(1).div_ceil(workgroup_size).max(1);

    let mut dispatch_bins = [0u32; MAX_DISPATCH_BINS];
    let bins_len = bins.len().min(MAX_DISPATCH_BINS);
    dispatch_bins[..bins_len].copy_from_slice(&bins[..bins_len]);

    DispatchLayout {
        dispatch_x,
        dispatch_z: bins_len.max(1) as u32,
        bins_len: bins_len as u32,
        bins: dispatch_bins,
    }
}

impl GPUDrawBuilder {
    fn make_pipelines(&mut self) -> Result<GPUDrawBuilderComputePipelines, bento::BentoError> {
        let mut ctx: &mut Context = unsafe { self.ctx.as_mut() };
        let state: &BindlessState = unsafe { self.state.as_ref() };

        let build_draws = CSOBuilder::new()
            .shader(Some(
                include_str!("shaders/build_gpu_draws.comp.glsl").as_bytes(),
            ))
            .add_variable(
                "draws",
                ShaderResource::StorageBuffer(self.data.draw_objects.get_gpu_handle().into()),
            )
            .add_variable(
                "culled",
                ShaderResource::StorageBuffer(self.data.cull_results.into()),
            )
            .add_variable(
                "counts",
                ShaderResource::StorageBuffer(self.data.bin_counts.into()),
            )
            .add_variable(
                "draw_list",
                ShaderResource::StorageBuffer(self.data.draw_list.into()),
            )
            .add_variable(
                "params",
                ShaderResource::DynamicStorage(self.data.alloc.state()),
            )
            .build(&mut ctx)
            .ok();

        Ok(GPUDrawBuilderComputePipelines { build_draws })
    }

    pub fn new(info: &GPUDrawBuilderInfo, state: &mut BindlessState) -> Self {
        let ctx: &mut Context = unsafe { &mut (*info.ctx) };
        let draw_objects = GPUPool::new(
            ctx,
            &BufferInfo {
                debug_name: &format!("{} Draw Builder Per-Draw Info", info.name),
                byte_size: (std::mem::size_of::<PerDrawData>() as u32 * info.limits.max_num_objects)
                    as u32,
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            },
        )
        .unwrap();

        let draw_list = ctx
            .make_buffer(&BufferInfo {
                debug_name: &format!("{} Draw Builder Draw List", info.name),
                byte_size: (std::mem::size_of::<IndexedIndirectCommand>() as u32
                    * info.limits.max_num_objects
                    * info.num_bins.max(1)) as u32,
                visibility: MemoryVisibility::Gpu,
                usage: BufferUsage::STORAGE | BufferUsage::INDIRECT,
                initial_data: None,
            })
            .unwrap();

        let alloc = ctx
            .make_dynamic_allocator(&DynamicAllocatorInfo {
                ..Default::default()
            })
            .expect("Unable to create dynamic allocator!");

        let data = GPUDrawBuilderData {
            active_objects: Vec::new(),
            draw_objects,
            alloc,
            draw_list,
            cull_results: info.cull_results,
            bin_counts: info.bin_counts,
            num_bins: info.num_bins,
            max_objects: info.limits.max_num_objects,
        };

        let mut s = Self {
            state: NonNull::new(state).unwrap(),
            ctx: NonNull::new(info.ctx).unwrap(),
            data,
            pipelines: Default::default(),
        };

        s.pipelines = s.make_pipelines().unwrap();

        s
    }

    pub fn register_draw(&mut self, info: &PerDrawData) -> Handle<PerDrawData> {
        self.data.draw_objects.insert(*info).unwrap()
    }

    pub fn release_draw(&mut self, handle: Handle<PerDrawData>) {
        self.data.draw_objects.release(handle);
    }

    pub fn per_draw_data(&self) -> Handle<Buffer> {
        self.data.draw_objects.get_gpu_handle()
    }

    pub fn draw_count(&self) -> u32 {
        self.data.draw_objects.len() as u32
    }

    pub fn draw_list(&self) -> Handle<Buffer> {
        self.data.draw_list
    }

    pub fn draw_list_for_bin(&self, bin: u32) -> BufferView {
        let offset = bin as u64
            * self.data.max_objects as u64
            * std::mem::size_of::<IndexedIndirectCommand>() as u64;
        BufferView {
            handle: self.data.draw_list,
            size: (self.data.max_objects as usize * std::mem::size_of::<IndexedIndirectCommand>())
                as u64,
            offset,
        }
    }

    pub fn reset(&mut self) {
        self.data.alloc.reset();
    }

    pub fn pre_compute(&mut self) -> CommandStream<Executable> {
        CommandStream::new()
            .begin()
            .combine(self.data.draw_objects.sync_up().unwrap())
            .end()
    }

    pub fn post_compute(&mut self) -> CommandStream<Executable> {
        CommandStream::new().begin().end()
    }

    pub fn build_draws(&mut self, bins: &[u32], view: u32) -> CommandStream<Executable> {
        let stream = CommandStream::new().begin();
        let num_objects = self.data.draw_objects.len() as u32;
        let dispatch_layout = build_dispatch_layout(num_objects, bins);

        let Some(build_draws) = self.pipelines.build_draws.as_ref() else {
            return stream.end();
        };

        #[repr(C)]
        #[derive(Debug)]
        struct PerDispatch {
            view: u32,
            num_bins: u32,
            max_objects: u32,
            num_draws: u32,
            bins_len: u32,
            _padding: [u32; 2],
            bins: [u32; MAX_DISPATCH_BINS],
        }

        let mut alloc = self.data.alloc.bump().expect("Failed to bump alloc!");
        let per_dispatch = &mut alloc.slice::<PerDispatch>()[0];
        per_dispatch.view = view;
        per_dispatch.num_bins = self.data.num_bins;
        per_dispatch.max_objects = self.data.max_objects;
        per_dispatch.num_draws = num_objects;
        per_dispatch.bins_len = dispatch_layout.bins_len;
        per_dispatch._padding = [0; 2];
        per_dispatch.bins = dispatch_layout.bins;
        stream
            .dispatch(&Dispatch {
                x: dispatch_layout.dispatch_x,
                y: 1,
                z: dispatch_layout.dispatch_z,
                pipeline: build_draws.handle,
                bind_tables: build_draws.tables(),
                dynamic_buffers: [None, Some(alloc), None, None],
            })
            .unbind_pipeline()
            .end()
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_DISPATCH_BINS, build_dispatch_layout};

    #[test]
    fn dispatch_layout_uses_z_for_each_bin() {
        let layout = build_dispatch_layout(127, &[3, 8, 13]);
        assert_eq!(layout.dispatch_x, 2);
        assert_eq!(layout.dispatch_z, 3);
        assert_eq!(layout.bins_len, 3);
        assert_eq!(&layout.bins[..3], &[3, 8, 13]);
    }

    #[test]
    fn dispatch_layout_defaults_to_one_z_without_bins() {
        let layout = build_dispatch_layout(0, &[]);
        assert_eq!(layout.dispatch_x, 1);
        assert_eq!(layout.dispatch_z, 1);
        assert_eq!(layout.bins_len, 0);
    }

    #[test]
    fn dispatch_param_payload_fits_dynamic_storage_limit() {
        let payload_size = (7 + MAX_DISPATCH_BINS) * std::mem::size_of::<u32>();
        assert!(payload_size <= 256);
    }

    #[test]
    fn dispatch_layout_caps_bins_to_dynamic_storage_limit() {
        let bins: Vec<u32> = (0..(MAX_DISPATCH_BINS as u32 + 6)).collect();
        let layout = build_dispatch_layout(64, &bins);
        assert_eq!(layout.bins_len, MAX_DISPATCH_BINS as u32);
        assert_eq!(layout.dispatch_z, MAX_DISPATCH_BINS as u32);
        assert_eq!(
            layout.bins[MAX_DISPATCH_BINS - 1],
            (MAX_DISPATCH_BINS - 1) as u32
        );
    }
}
