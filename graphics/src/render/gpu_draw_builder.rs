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

use super::deferred::PerDrawData;

pub struct GPUDrawBuilderLimits {
    pub max_num_objects: u32,
}

pub struct GPUDrawBuilderInfo<'a> {
    pub name: &'a str,
    pub ctx: *mut Context,
    pub cull_results: Handle<Buffer>,
    pub bin_counts: Handle<Buffer>,
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
        }
    }
}

struct GPUDrawBuilderData {
    draw_objects: GPUPool<PerDrawData>,
    draw_list: Handle<Buffer>,
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
                byte_size: (std::mem::size_of::<PerDrawData>() as u32
                    * info.limits.max_num_objects) as u32,
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
                    * info.limits.max_num_objects) as u32,
                visibility: MemoryVisibility::Gpu,
                usage: BufferUsage::STORAGE,
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

    pub fn draw_list(&self) -> Handle<Buffer> {
        self.data.draw_list
    }

    pub fn build_draws(&mut self, bin: u32, view: u32) -> CommandStream<Executable> {
        let stream = CommandStream::new().begin();
        let workgroup_size = 64u32;
        let num_objects = self.data.draw_objects.len() as u32;

        let dispatch_x = ((num_objects.max(1) + workgroup_size - 1) / workgroup_size).max(1);

        let Some(build_draws) = self.pipelines.build_draws.as_ref() else {
            return stream.end();
        };

        struct PerDispatch {
            bin: u32,
            view: u32,
        }

        let mut alloc = self.data.alloc.bump().expect("Failed to bump alloc!");
        let per_dispatch = &mut alloc.slice::<PerDispatch>()[0];
        per_dispatch.bin = bin;
        per_dispatch.view = view;

        stream
            .combine(self.data.draw_objects.sync_up().unwrap())
            .dispatch(&Dispatch {
                x: dispatch_x,
                y: 1,
                z: 1,
                pipeline: build_draws.handle,
                bind_tables: build_draws.tables(),
                dynamic_buffers: [None, Some(alloc), None, None],
            })
            .unbind_pipeline()
            .end()
    }
}
