use bento::builder::{AttachmentDesc, PSOBuilder};
use bento::{Compiler, OptimizationLevel, Request, ShaderLang};
use bytemuck::{Pod, Zeroable};
use dashi::cmd::PendingGraphics;
use dashi::driver::command::Draw;
use dashi::{
    BlendFactor, BlendOp, BufferInfo, BufferUsage, ColorBlendState, CommandStream, Context,
    DepthInfo, Format, GraphicsPipelineDetails, IndexedResource, MemoryVisibility, SampleCount,
    ShaderResource, ShaderType, Viewport,
};
use furikake::PSOBuilderFurikakeExt;
use resource_pool::{resource_list::ResourceList, Handle};
use tare::utils::StagedBuffer;
use tracing::{error, warn};

use crate::{GuiInfo, GuiObject};

#[derive(Clone, Debug)]
struct GuiObjectData {
    info: GuiInfo,
    dirty: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct GuiVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Clone, Debug, Default)]
pub struct GuiMesh {
    pub vertices: Vec<GuiVertex>,
    pub indices: Vec<u32>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GuiMeshRange {
    pub vertex_count: usize,
    pub index_count: usize,
}

pub struct GuiRenderer {
    objects: ResourceList<GuiObjectData>,
    gui_pso: Option<bento::builder::PSO>,
    vertex_buffer: Option<StagedBuffer>,
    index_buffer: Option<StagedBuffer>,
    vertex_capacity: usize,
    index_capacity: usize,
    mesh_range: GuiMeshRange,
}

fn to_handle(handle: Handle<GuiObjectData>) -> Handle<GuiObject> {
    Handle::new(handle.slot, handle.generation)
}

fn from_handle(handle: Handle<GuiObject>) -> Handle<GuiObjectData> {
    Handle::new(handle.slot, handle.generation)
}

impl GuiRenderer {
    pub fn new() -> Self {
        Self {
            objects: ResourceList::default(),
            gui_pso: None,
            vertex_buffer: None,
            index_buffer: None,
            vertex_capacity: 65_536,
            index_capacity: 131_072,
            mesh_range: GuiMeshRange::default(),
        }
    }

    pub fn initialize_renderer(
        &mut self,
        ctx: &mut Context,
        state: &mut furikake::BindlessState,
        sample_count: SampleCount,
    ) {
        if self.gui_pso.is_some() {
            return;
        }

        let vertex_buffer = StagedBuffer::new(
            ctx,
            BufferInfo {
                debug_name: "[MESHI] GUI Vertex Buffer",
                byte_size: (std::mem::size_of::<GuiVertex>() * self.vertex_capacity) as u32,
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            },
        );

        let index_buffer = StagedBuffer::new(
            ctx,
            BufferInfo {
                debug_name: "[MESHI] GUI Index Buffer",
                byte_size: (std::mem::size_of::<u32>() * self.index_capacity) as u32,
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            },
        );

        let gui_pso = Self::build_gui_pipeline(ctx, state, sample_count, &vertex_buffer, &index_buffer);
        state.register_pso_tables(&gui_pso);

        self.gui_pso = Some(gui_pso);
        self.vertex_buffer = Some(vertex_buffer);
        self.index_buffer = Some(index_buffer);
    }

    fn build_gui_pipeline(
        ctx: &mut Context,
        state: &mut furikake::BindlessState,
        sample_count: SampleCount,
        vertex_buffer: &StagedBuffer,
        index_buffer: &StagedBuffer,
    ) -> bento::builder::PSO {
        let compiler = Compiler::new().expect("Failed to create shader compiler");
        let base_request = Request {
            name: Some("meshi_gui".to_string()),
            lang: ShaderLang::Slang,
            stage: ShaderType::Vertex,
            optimization: OptimizationLevel::Performance,
            debug_symbols: true,
            defines: Default::default(),
        };

        let vertex = compiler
            .compile(
                include_str!("shaders/gui_vert.slang").as_bytes(),
                &Request {
                    stage: ShaderType::Vertex,
                    ..base_request.clone()
                },
            )
            .expect("Failed to compile GUI vertex shader");
        let fragment = compiler
            .compile(
                include_str!("shaders/gui_frag.slang").as_bytes(),
                &Request {
                    stage: ShaderType::Fragment,
                    ..base_request
                },
            )
            .expect("Failed to compile GUI fragment shader");

        PSOBuilder::new()
            .vertex_compiled(Some(vertex))
            .fragment_compiled(Some(fragment))
            .add_table_variable_with_resources(
                "gui_vertices",
                vec![IndexedResource {
                    resource: ShaderResource::StorageBuffer(vertex_buffer.device().into()),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "gui_indices",
                vec![IndexedResource {
                    resource: ShaderResource::StorageBuffer(index_buffer.device().into()),
                    slot: 0,
                }],
            )
            .add_reserved_table_variables(state)
            .unwrap()
            .add_depth_target(AttachmentDesc {
                format: Format::D24S8,
                samples: sample_count,
            })
            .set_attachment_format(0, Format::BGRA8)
            .set_details(GraphicsPipelineDetails {
                color_blend_states: vec![ColorBlendState {
                    enable: true,
                    src_blend: BlendFactor::SrcAlpha,
                    dst_blend: BlendFactor::InvSrcAlpha,
                    blend_op: BlendOp::Add,
                    src_alpha_blend: BlendFactor::One,
                    dst_alpha_blend: BlendFactor::InvSrcAlpha,
                    alpha_blend_op: BlendOp::Add,
                    write_mask: Default::default(),
                }],
                sample_count,
                depth_test: Some(DepthInfo {
                    should_test: false,
                    should_write: false,
                }),
                ..Default::default()
            })
            .build(ctx)
            .expect("Failed to build GUI pipeline")
    }

    pub fn upload_mesh(&mut self, mesh: &GuiMesh) -> GuiMeshRange {
        let Some(vertex_buffer) = self.vertex_buffer.as_mut() else {
            return GuiMeshRange::default();
        };
        let Some(index_buffer) = self.index_buffer.as_mut() else {
            return GuiMeshRange::default();
        };

        let vertex_count = mesh.vertices.len().min(self.vertex_capacity);
        if mesh.vertices.len() > self.vertex_capacity {
            warn!(
                "GUI vertex buffer overflow ({} > {}), truncating.",
                mesh.vertices.len(),
                self.vertex_capacity
            );
        }

        let index_count = mesh.indices.len().min(self.index_capacity);
        if mesh.indices.len() > self.index_capacity {
            warn!(
                "GUI index buffer overflow ({} > {}), truncating.",
                mesh.indices.len(),
                self.index_capacity
            );
        }

        vertex_buffer.as_slice_mut::<GuiVertex>()[..vertex_count]
            .copy_from_slice(&mesh.vertices[..vertex_count]);
        index_buffer.as_slice_mut::<u32>()[..index_count]
            .copy_from_slice(&mesh.indices[..index_count]);

        self.mesh_range = GuiMeshRange {
            vertex_count,
            index_count,
        };

        self.mesh_range
    }

    pub fn render_gui(
        &mut self,
        viewport: &Viewport,
    ) -> CommandStream<PendingGraphics> {
        let mut cmd = CommandStream::<PendingGraphics>::subdraw();
        let Some(pso) = self.gui_pso.as_ref() else {
            error!("Failed to build gui without a gui pso");
            return cmd;
        };

        if self.mesh_range.index_count == 0 {
            return cmd;
        }

        let Some(vertex_buffer) = self.vertex_buffer.as_ref() else {
            return cmd;
        };
        let Some(index_buffer) = self.index_buffer.as_ref() else {
            return cmd;
        };

        cmd = cmd
            .bind_graphics_pipeline(pso.handle)
            .update_viewport(viewport)
            .draw(&Draw {
                bind_tables: pso.tables(),
                count: self.mesh_range.index_count as u32,
                instance_count: 1,
                ..Default::default()
            })
            .unbind_graphics_pipeline();

        cmd
    }

    pub fn register_gui(&mut self, info: &GuiInfo) -> Handle<GuiObject> {
        let handle = self.objects.push(GuiObjectData {
            info: info.clone(),
            dirty: true,
        });

        to_handle(handle)
    }

    pub fn release_gui(&mut self, handle: Handle<GuiObject>) {
        if !handle.valid() {
            return;
        }

        let handle = from_handle(handle);
        if !self.objects.entries.iter().any(|entry| entry.slot == handle.slot) {
            return;
        }

        self.objects.release(handle);
    }

    pub fn set_gui_info(&mut self, handle: Handle<GuiObject>, info: &GuiInfo) {
        if !handle.valid() {
            return;
        }

        let handle = from_handle(handle);
        if !self.objects.entries.iter().any(|entry| entry.slot == handle.slot) {
            return;
        }

        let object = self.objects.get_ref_mut(handle);
        object.info = info.clone();
        object.dirty = true;
    }
}
