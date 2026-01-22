use bento::builder::{AttachmentDesc, PSOBuilder};
use bento::{Compiler, OptimizationLevel, Request, ShaderLang};
use bytemuck::{Pod, Zeroable};
use dashi::cmd::PendingGraphics;
use dashi::driver::command::Draw;
use dashi::{
    BlendFactor, BlendOp, BufferInfo, BufferUsage, ColorBlendState, CommandStream, Context,
    DepthInfo, DynamicState, Format, GraphicsPipelineDetails, IndexedResource, MemoryVisibility,
    Rect2D, SampleCount, ShaderResource, ShaderType, Viewport,
};
use furikake::PSOBuilderFurikakeExt;
use resource_pool::{resource_list::ResourceList, Handle};
use tare::utils::StagedBuffer;
use tracing::{error, warn};

use crate::gui::{GuiBatchMesh, GuiClipRect, GuiFrame};
use crate::{GuiInfo, GuiObject};

#[derive(Clone, Debug)]
struct GuiObjectData {
    info: GuiInfo,
    visible: bool,
    dirty: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct GuiVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub texture_id: u32,
    pub _padding: [u32; 3],
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
    batch_meshes: Vec<GuiBatchMesh>,
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
            batch_meshes: Vec::new(),
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

        let mut pso_builder = PSOBuilder::new()
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
            .add_reserved_table_variable(state, "meshi_bindless_textures")
            .unwrap()
            .add_reserved_table_variable(state, "meshi_bindless_samplers")
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
                dynamic_states: vec![DynamicState::Viewport, DynamicState::Scissor],
                ..Default::default()
            })
            .build(ctx)
            .expect("Failed to build GUI pipeline");

        pso_builder
    }

    pub fn upload_mesh(&mut self, mesh: &GuiMesh) -> GuiMeshRange {
        self.batch_meshes.clear();
        self.upload_mesh_inner(mesh)
    }

    fn upload_mesh_inner(&mut self, mesh: &GuiMesh) -> GuiMeshRange {
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

    pub fn upload_frame(&mut self, frame: GuiFrame) {
        self.batch_meshes = frame.batches;
    }

    pub fn render_gui(
        &mut self,
        viewport: &Viewport,
    ) -> CommandStream<PendingGraphics> {
        
        let s: &mut Self= unsafe{&mut *(self as *mut Self) };
        let cmd = CommandStream::<PendingGraphics>::subdraw();
        let Some(pso) = self.gui_pso.as_ref() else {
            error!("Failed to build gui without a gui pso");
            return cmd;
        };
        let pso_handle = pso.handle;
        let bind_tables = pso.tables();

        if self.vertex_buffer.is_none() || self.index_buffer.is_none() {
            return cmd;
        };

        let mut graphics_cmd = cmd.bind_graphics_pipeline(pso_handle);

        if self.batch_meshes.is_empty() {
            if self.mesh_range.index_count == 0 {
                return graphics_cmd.unbind_graphics_pipeline();
            }

            graphics_cmd = graphics_cmd.update_viewport(viewport).draw(&Draw {
                bind_tables,
                count: self.mesh_range.index_count as u32,
                instance_count: 1,
                ..Default::default()
            });
        } else {
            for batch in &self.batch_meshes {
                let range = Self::upload_mesh_inner(s, &batch.mesh);
                if range.index_count == 0 {
                    continue;
                }

                let scissor = batch
                    .batch
                    .clip_rect
                    .map(|clip| scissor_from_clip(clip, viewport))
                    .unwrap_or(viewport.scissor);
                let batch_viewport = Viewport { scissor, ..*viewport };

                graphics_cmd = graphics_cmd.update_viewport(&batch_viewport).draw(&Draw {
                    bind_tables,
                    count: range.index_count as u32,
                    instance_count: 1,
                    ..Default::default()
                });
            }
        }

        graphics_cmd.unbind_graphics_pipeline()
    }

    pub fn register_gui(&mut self, info: &GuiInfo) -> Handle<GuiObject> {
        let handle = self.objects.push(GuiObjectData {
            info: info.clone(),
            visible: true,
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

    pub fn set_gui_visibility(&mut self, handle: Handle<GuiObject>, visible: bool) {
        if !handle.valid() {
            return;
        }

        let handle = from_handle(handle);
        if !self.objects.entries.iter().any(|entry| entry.slot == handle.slot) {
            return;
        }

        let object = self.objects.get_ref_mut(handle);
        object.visible = visible;
        object.dirty = true;
    }
}

fn scissor_from_clip(clip: GuiClipRect, viewport: &Viewport) -> Rect2D {
    let min_x = clip.min[0].max(viewport.area.x);
    let min_y = clip.min[1].max(viewport.area.y);
    let max_x = clip.max[0].min(viewport.area.x + viewport.area.w);
    let max_y = clip.max[1].min(viewport.area.y + viewport.area.h);

    let x = min_x.floor();
    let y = min_y.floor();
    let w = (max_x.ceil() - x).max(0.0);
    let h = (max_y.ceil() - y).max(0.0);

    Rect2D {
        x: x.max(0.0) as u32,
        y: y.max(0.0) as u32,
        w: w as u32,
        h: h as u32,
    }
}
