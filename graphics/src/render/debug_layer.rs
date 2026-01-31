use bento::builder::CSOBuilder;
use dashi::cmd::Executable;
use dashi::driver::command::Dispatch;
use dashi::{
    BufferInfo, BufferUsage, CommandStream, Context, Handle, Image, ImageView, MemoryVisibility,
    ShaderResource,
};
use furikake::BindlessState;
use furikake::PSOBuilderFurikakeExt;
use tare::utils::StagedBuffer;

const DEBUG_LAYER_WORKGROUP_SIZE: u32 = 32;
pub const DEBUG_LAYER_INVALID_BINDLESS: u32 = u32::MAX;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct DebugLayerParams {
    pub output_resolution: [u32; 2],
    pub input_texture: u32,
    pub debug_selection: u32,
    pub debug_textures: [u32; 4],
    pub override_texture: u32,
    pub _padding: [u32; 3],
}

pub struct DebugLayer {
    params: StagedBuffer,
    pipeline: Option<bento::builder::CSO>,
    output_handle: Option<Handle<Image>>,
}

impl DebugLayer {
    pub fn new(ctx: &mut Context) -> Self {
        let params = StagedBuffer::new(
            ctx,
            BufferInfo {
                debug_name: "[MESHI] Debug Layer Params",
                byte_size: (std::mem::size_of::<DebugLayerParams>() as u32).max(256),
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::UNIFORM,
                initial_data: None,
            },
        );

        Self {
            params,
            pipeline: None,
            output_handle: None,
        }
    }

    pub fn update_params(&mut self, params: DebugLayerParams) {
        self.params.as_slice_mut::<DebugLayerParams>()[0] = params;
    }

    fn ensure_pipeline(&mut self, ctx: &mut Context, state: &BindlessState, output: ImageView) {
        if self.output_handle == Some(output.img) && self.pipeline.is_some() {
            return;
        }

        let pipeline = Some(
            CSOBuilder::new()
                .shader(Some(
                    include_str!("shaders/debug_layer.comp.glsl").as_bytes(),
                ))
                .add_reserved_table_variables(state)
                .unwrap()
                .add_variable(
                    "debug_params",
                    ShaderResource::ConstBuffer(self.params.device().into()),
                )
                .add_variable("debug_output", ShaderResource::Image(output))
                .build(ctx)
                .expect("Failed to make debug layer!"),
        );

        self.pipeline = pipeline;
        self.output_handle = Some(output.img);
    }

    pub fn record(
        &mut self,
        ctx: &mut Context,
        state: &BindlessState,
        output: ImageView,
        resolution: [u32; 2],
    ) -> CommandStream<Executable> {
        self.ensure_pipeline(ctx, state, output);

        let Some(pipeline) = self.pipeline.as_ref() else {
            return CommandStream::new().begin().end();
        };

        let dispatch_x =
            ((resolution[0] + DEBUG_LAYER_WORKGROUP_SIZE - 1) / DEBUG_LAYER_WORKGROUP_SIZE).max(1);
        let dispatch_y =
            ((resolution[1] + DEBUG_LAYER_WORKGROUP_SIZE - 1) / DEBUG_LAYER_WORKGROUP_SIZE).max(1);

        CommandStream::new()
            .begin()
            .combine(self.params.sync_up())
            .dispatch(&Dispatch {
                x: dispatch_x,
                y: dispatch_y,
                z: 1,
                pipeline: pipeline.handle,
                bind_tables: pipeline.tables(),
                dynamic_buffers: Default::default(),
            })
            .unbind_pipeline()
            .end()
    }
}
