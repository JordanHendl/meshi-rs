use std::ptr::NonNull;

use bento::builder::CSOBuilder;
use dashi::*;
use dashi::cmd::*;
use dashi::{
    Buffer, BufferInfo, BufferUsage, CommandStream, Context, Handle, MemoryVisibility,
    ShaderResource,
    cmd::Executable,
    driver::command::{Dispatch, DrawIndexedIndirect},
    structs::IndexedIndirectCommand,
    utils::gpupool::GPUPool,
};
use furikake::BindlessState;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Particle {
    pub position: [f32; 3],
    pub lifetime: f32,
    pub velocity: [f32; 3],
    pub size: f32,
    pub color: [f32; 4],
}

pub struct ParticleSystemLimits {
    pub max_particles: u32,
}

pub struct ParticleSystemInfo<'a> {
    pub name: &'a str,
    pub ctx: *mut Context,
    pub index_buffer: Handle<Buffer>,
    pub index_count: u32,
    pub first_index: u32,
    pub vertex_offset: i32,
    pub limits: ParticleSystemLimits,
}

impl<'a> Default for ParticleSystemInfo<'a> {
    fn default() -> Self {
        Self {
            name: Default::default(),
            ctx: Default::default(),
            index_buffer: Default::default(),
            index_count: 0,
            first_index: 0,
            vertex_offset: 0,
            limits: ParticleSystemLimits { max_particles: 4096 },
        }
    }
}

struct ParticleSystemData {
    particles: GPUPool<Particle>,
    draw_list: Handle<Buffer>,
    alloc: DynamicAllocator,
    max_particles: u32,
    index_buffer: Handle<Buffer>,
    index_count: u32,
    first_index: u32,
    vertex_offset: i32,
}

#[derive(Default)]
struct ParticleSystemComputePipelines {
    build_draws: Option<bento::builder::CSO>,
}

pub struct ParticleDrawInfo {
    pub pipeline: Handle<GraphicsPipeline>,
    pub bind_tables: [Option<Handle<BindTable>>; 4],
    pub dynamic_buffers: [Option<DynamicBuffer>; 4],
}

pub struct ParticleSystem {
    state: NonNull<BindlessState>,
    ctx: NonNull<Context>,
    data: ParticleSystemData,
    pipelines: ParticleSystemComputePipelines,
}

impl ParticleSystem {
    fn make_pipelines(&mut self) -> Result<ParticleSystemComputePipelines, bento::BentoError> {
        let mut ctx: &mut Context = unsafe { self.ctx.as_mut() };

        let build_draws = CSOBuilder::new()
            .shader(Some(
                include_str!("shaders/build_particle_draws.comp.glsl").as_bytes(),
            ))
            .add_variable(
                "particles",
                ShaderResource::StorageBuffer(self.data.particles.get_gpu_handle().into()),
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

        Ok(ParticleSystemComputePipelines { build_draws })
    }

    pub fn new(info: &ParticleSystemInfo, state: &mut BindlessState) -> Self {
        let ctx: &mut Context = unsafe { &mut (*info.ctx) };
        let particles = GPUPool::new(
            ctx,
            &BufferInfo {
                debug_name: &format!("{} Particles", info.name),
                byte_size: (std::mem::size_of::<Particle>() as u32 * info.limits.max_particles)
                    as u32,
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            },
        )
        .unwrap();

        let draw_list = ctx
            .make_buffer(&BufferInfo {
                debug_name: &format!("{} Particle Draw List", info.name),
                byte_size: std::mem::size_of::<IndexedIndirectCommand>() as u32,
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

        let data = ParticleSystemData {
            particles,
            draw_list,
            alloc,
            max_particles: info.limits.max_particles,
            index_buffer: info.index_buffer,
            index_count: info.index_count,
            first_index: info.first_index,
            vertex_offset: info.vertex_offset,
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

    pub fn register_particle(&mut self, particle: &Particle) -> Handle<Particle> {
        self.data.particles.insert(*particle).unwrap()
    }

    pub fn release_particle(&mut self, handle: Handle<Particle>) {
        self.data.particles.release(handle);
    }

    pub fn particle_buffer(&self) -> Handle<Buffer> {
        self.data.particles.get_gpu_handle()
    }

    pub fn draw_list(&self) -> Handle<Buffer> {
        self.data.draw_list
    }

    pub fn draw_count(&self) -> u32 {
        if self.data.particles.len() == 0 {
            0
        } else {
            1
        }
    }

    pub fn reset(&mut self) {
        self.data.alloc.reset();
    }

    pub fn record_draws(&mut self, draw_info: &ParticleDrawInfo) -> CommandStream<PendingGraphics> {
        let mut stream = CommandStream::new().begin();

        let Some(build_draws) = self.pipelines.build_draws.as_ref() else {
            return CommandStream::<PendingGraphics>::subdraw();
        };

        #[repr(C)]
        struct DrawParams {
            num_particles: u32,
            index_count: u32,
            first_index: u32,
            vertex_offset: i32,
            first_instance: u32,
            _padding: [u32; 3],
        }

        let mut alloc = self.data.alloc.bump().expect("Failed to bump alloc!");
        let params = &mut alloc.slice::<DrawParams>()[0];
        params.num_particles = self.data.particles.len() as u32;
        params.index_count = self.data.index_count;
        params.first_index = self.data.first_index;
        params.vertex_offset = self.data.vertex_offset;
        params.first_instance = 0;
        params._padding = [0; 3];

        stream = stream
            .combine(self.data.particles.sync_up().unwrap())
            .dispatch(&Dispatch {
                x: 1,
                y: 1,
                z: 1,
                pipeline: build_draws.handle,
                bind_tables: build_draws.tables(),
                dynamic_buffers: [None, Some(alloc), None, None],
            })
            .unbind_pipeline();

        CommandStream::<PendingGraphics>::subdraw()
            .combine(stream)
            .bind_graphics_pipeline(draw_info.pipeline)
            .draw_indexed_indirect(&DrawIndexedIndirect {
                indices: self.data.index_buffer,
                indirect: self.data.draw_list,
                bind_tables: draw_info.bind_tables,
                dynamic_buffers: draw_info.dynamic_buffers,
                draw_count: self.draw_count(),
                ..Default::default()
            })
            .unbind_graphics_pipeline()
    }
}
